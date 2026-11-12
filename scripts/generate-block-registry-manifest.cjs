const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { blockReferences, symbolicRegistrations, literalRegistrations, referenceBlockKeys } = require("./block-references.cjs");
const { modernTree, recordedSource, decompRoot: sourceRoot } = require("./source-roots.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const manifestPath = path.join(repositoryRoot, "generated", "minecraft-versions.json");
const outputPath = path.resolve(
  process.env.MC_BLOCK_REGISTRY_MANIFEST ?? path.join(repositoryRoot, "generated", "block-registries.json"),
);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const versionFamily = (version) => version.split(".").slice(0, 2).join(".");

// a release whose sources are absent keeps the entry they last produced, only with --cached
const cachedMode = process.argv.includes("--cached");
const previousRegistries = fs.existsSync(outputPath)
  ? JSON.parse(fs.readFileSync(outputPath, "utf8")).versions ?? {}
  : {};
const runtimeRegistries = JSON.parse(fs.readFileSync(path.join(path.dirname(manifestPath), "runtime-registries.json"), "utf8")).releases;

/**
 * A kept entry with the blocks its client reports and the kept sources did not: their names and
 * state counts are exact, their class and definition stay unknown until a source of the release
 * exists. A kept name the client does not have is an error.
 */
function withRuntimeNames(entry, runtime) {
  if (!runtime) return entry;
  const names = new Set(runtime.blocks.map(([id]) => id));
  const stale = entry.blocks.filter((id) => !names.has(id));
  if (stale.length > 0) throw new Error(`${runtime.runtimeGameVersion}'s client has no ${stale.join(", ")}`);
  const runtimeOnly = [...names].filter((id) => !entry.blocks.includes(id)).sort();
  if (runtimeOnly.length === 0) return entry;
  const classes = { ...entry.classes };
  for (const id of runtimeOnly) classes[id] = { field: null, className: null, initializer: null, confidence: "runtime-name" };
  return {
    ...entry,
    blocks: [...names].sort(),
    classes,
    runtimeOnly,
    unresolvedClassCount: (entry.unresolvedClassCount ?? 0) + runtimeOnly.length,
  };
}

const registries = {};
for (const version of manifest.versions) {
  const tree = modernTree(version) ?? path.join(sourceRoot, versionFamily(version), version);
  const sourcePath = path.join(tree, "net", "minecraft", "world", "level", "block", "Blocks.java");
  if (!fs.existsSync(sourcePath) && previousRegistries[version] && cachedMode) {
    registries[version] = withRuntimeNames(previousRegistries[version], runtimeRegistries[version]);
    continue;
  }
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`Missing source-backed block registry for ${version}: ${sourcePath}; run scripts/materialize-sources.cjs, or pass --cached to keep its entry`);
  }
  const source = fs.readFileSync(sourcePath, "utf8");
  // 26.2 names its blocks by symbol, whose identifiers net.minecraft.references holds
  const references = /\bregister\(\s*BlockItemIds\./.test(source) ? blockReferences(tree) : null;
  if (references) {
    const classes = symbolicRegistrations(source, references);
    const blocks = Object.keys(classes).sort();
    registries[version] = {
      source: recordedSource(version, path.dirname(outputPath), sourcePath),
      sha256: crypto.createHash("sha256").update(source).digest("hex"),
      blocks,
      classes,
      unresolvedClassCount: blocks.filter((name) => !classes[name].className).length,
    };
    continue;
  }
  const classes = literalRegistrations(source, referenceBlockKeys(tree));
  const blocks = Object.keys(classes).sort();
  if (blocks.length === 0) throw new Error(`No block registrations found for ${version}`);
  registries[version] = {
    source: recordedSource(version, path.dirname(outputPath), sourcePath),
    sha256: crypto.createHash("sha256").update(source).digest("hex"),
    blocks,
    classes,
    unresolvedClassCount: blocks.filter((name) => !classes[name].className).length,
  };
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify({
  schema: 1,
  sourceManifest: path.relative(path.dirname(outputPath), manifestPath).replaceAll(path.sep, "/"),
  versions: registries,
}, null, 2)}\n`);
