const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

const repositoryRoot = path.resolve(__dirname, "..");
const manifestPath = path.join(repositoryRoot, "generated", "minecraft-versions.json");
const outputPath = path.resolve(
  process.env.MC_BLOCK_REGISTRY_MANIFEST ?? path.join(repositoryRoot, "generated", "block-registries.json"),
);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const sourceRoot = path.resolve(path.dirname(manifestPath), manifest.sourceRoot);
const versionFamily = (version) => version.split(".").slice(0, 2).join(".");
const registerPattern = /\bregister\s*\(\s*(?:"([a-z0-9_./-]+)"|(?:BlockItemIds|BlockIds)\.([A-Z0-9_]+))\s*,/g;
const declarationPattern = /public\s+static\s+final\s+Block\s+([A-Z0-9_]+)\s*=\s*register\s*\(\s*(?:"([a-z0-9_./-]+)"|(?:BlockItemIds|BlockIds)\.([A-Z0-9_]+))\s*,\s*([\s\S]*?)(?=\);\s*[\r\n])/g;
const classPattern = /\bnew\s+([A-Za-z0-9_]+)\s*\(|\b([A-Za-z0-9_]+)::new\b/;
const helperPattern = /private\s+static\s+[A-Za-z0-9_<>?, ]+\s+([A-Za-z0-9_]+)\s*\([^)]*\)\s*\{([\s\S]*?)\n\s*\}/g;

// a release whose mc-decomp tree is absent keeps the entry its sources last produced
const previousRegistries = fs.existsSync(outputPath)
  ? JSON.parse(fs.readFileSync(outputPath, "utf8")).versions ?? {}
  : {};
const registries = {};
for (const version of manifest.versions) {
  const sourcePath = path.join(
    sourceRoot,
    versionFamily(version),
    version,
    "net",
    "minecraft",
    "world",
    "level",
    "block",
    "Blocks.java",
  );
  if (!fs.existsSync(sourcePath) && previousRegistries[version]) {
    registries[version] = previousRegistries[version];
    continue;
  }
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`Missing source-backed block registry for ${version}: ${sourcePath}`);
  }
  const source = fs.readFileSync(sourcePath, "utf8");
  const names = [];
  for (const match of source.matchAll(registerPattern)) {
    names.push(match[1] ?? match[2].toLowerCase());
  }
  const uniqueNames = [...new Set(names)].sort();
  if (uniqueNames.length === 0) throw new Error(`No block registrations found for ${version}`);
  if (uniqueNames.length !== names.length) {
    throw new Error(`Duplicate block registrations found for ${version}`);
  }
  const classes = {};
  const helpers = {};
  for (const match of source.matchAll(helperPattern)) {
    const classMatch = classPattern.exec(match[2]);
    if (classMatch) helpers[match[1]] = classMatch[1] ?? classMatch[2];
  }
  Object.assign(helpers, {
    log: "RotatedPillarBlock",
    netherStem: "RotatedPillarBlock",
    leaves: "LeavesBlock",
    woodenButton: "ButtonBlock",
    flowerPot: "FlowerPotBlock",
    bed: "BedBlock",
    shulkerBox: "ShulkerBoxBlock",
    stainedGlass: "StainedGlassBlock",
    legacyStair: "StairBlock",
  });
  for (const match of source.matchAll(declarationPattern)) {
    const id = match[2] ?? match[3].toLowerCase();
    const initializer = match[4];
    const classMatch = classPattern.exec(initializer);
    const helperMatch = /^([A-Za-z0-9_]+)\s*\(/.exec(initializer.trim());
    const className = classMatch?.[1]
      ?? classMatch?.[2]
      ?? (helperMatch ? helpers[helperMatch[1]] : null)
      ?? (/^Properties\.(?:of|ofLegacyCopy|ofFullCopy)\(/.test(initializer.trim()) ? "Block" : null);
    classes[id] = {
      field: match[1],
      className,
      initializer: initializer.replace(/\s+/g, " ").trim(),
      confidence: className ? "source" : "unresolved",
    };
  }
  registries[version] = {
    source: path.relative(path.dirname(outputPath), sourcePath).replaceAll(path.sep, "/"),
    sha256: crypto.createHash("sha256").update(source).digest("hex"),
    blocks: uniqueNames,
    classes,
    unresolvedClassCount: uniqueNames.filter((name) => !classes[name]?.className).length,
  };
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify({
  schema: 1,
  sourceManifest: path.relative(path.dirname(outputPath), manifestPath).replaceAll(path.sep, "/"),
  versions: registries,
}, null, 2)}\n`);
