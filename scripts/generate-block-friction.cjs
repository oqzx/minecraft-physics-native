// Writes generated/block-friction.json: for every registered release, the friction of its blocks out of its own
// block registry source: a legacy release's decompiled jar under MC_LEGACY_SOURCES, a modern release's exact tree
// (scripts/source-roots.cjs: MC_SOURCE_ROOT for 26.2 and 26.3, else its pinned commit or client under MC_SOURCE_CACHE),
// all Mojang-named; a release with none is listed as unverified, and its previous entry stays. A source that does not resolve fails the run (scripts/block-friction.cjs).
// Pass --check to compare instead of writing: it fails when the output differs.

const childProcess = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { registeredReleases } = require("./registered-releases.cjs");
const { PINNED_CLIENTS, PINNED_SOURCES, NAMED_JARS } = require("./source-pins.cjs");
const { modernTree, legacyTree } = require("./source-roots.cjs");
const {
  parseDefaultFriction, parseRegistryFriction, parseLegacyFriction, parseLegacyClass, sha256,
} = require("./block-friction.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "block-friction.json");
const sourceRoot = process.env.MC_SOURCE_ROOT ?? path.join(repositoryRoot, "..", "..", "pr", "mc-source");
// a registered release read from another's source: none since 1.8.8 is decompiled from its own jar
const STAND_INS = {};

const read = (file) => (fs.existsSync(file) ? fs.readFileSync(file, "utf8") : undefined);

function directoryTree(root, identity, label) {
  const files = (relative) => read(path.join(root, relative));
  const list = (relative) => {
    const directory = path.join(root, relative);
    return fs.existsSync(directory) ? fs.readdirSync(directory).filter((name) => name.endsWith(".java")).map((name) => path.join(relative, name)) : [];
  };
  // (git grep on the plain files: a tree holds a thousand of them)
  const grep = (pattern, relative) => {
    const directory = path.join(root, relative);
    if (!fs.existsSync(directory)) return [];
    const found = childProcess.spawnSync("git", ["grep", "--no-index", "-l", "-E", pattern, "--", "."], { cwd: directory, encoding: "utf8", maxBuffer: 1 << 26 });
    if (found.status !== 0 && found.status !== 1) throw new Error(`${label}: git grep failed: ${found.stderr}`);
    return found.stdout.split("\n").filter(Boolean).map((name) => path.join(relative, name));
  };
  return { identity, label, root, read: files, list, grep };
}

function treeOf(release) {
  const legacy = legacyTree(release);
  if (legacy) return directoryTree(legacy, `legacy ${NAMED_JARS[release]}`, `legacy/${release}`);
  const tree = modernTree(release);
  if (!tree) return undefined;
  const fromRoot = path.relative(sourceRoot, tree);
  if (!fromRoot.startsWith("..") && !path.isAbsolute(fromRoot)) return directoryTree(tree, `mc-source ${release}`, `mc-source/${release}/client`);
  if (PINNED_CLIENTS[release]) return directoryTree(tree, `client ${PINNED_CLIENTS[release].client}`, `client/${release}`);
  return directoryTree(tree, `origin/client${release} ${PINNED_SOURCES[release]}`, `commit/${release}`);
}

const MOJANG = path.join("net", "minecraft", "world", "level", "block");
const YARN = path.join("net", "minecraft", "block");

/** The blocks' friction in a tree: which file registers them and where the default is. */
function frictionOf(tree, release) {
  const family = tree.read(path.join(MOJANG, "Blocks.java")) != null ? MOJANG : YARN;
  const blocks = tree.read(path.join(family, "Blocks.java"));
  if (blocks == null) throw new Error(`${release}: ${tree.label} has no Blocks.java`);
  const blockSource = tree.read(path.join(family, "Block.java"));
  if (blockSource == null) throw new Error(`${release}: ${tree.label} has no Block.java`);
  const statedefault = tree.read(path.join(family, "state", "BlockBehaviour.java")) ?? tree.read(path.join(family, "AbstractBlock.java"));
  const defaultFriction = parseDefaultFriction(`${blockSource}\n${statedefault ?? ""}`, `${release} default`);

  // an override of the getter takes the block out of what the registry says
  const overrides = tree.grep("float +get(Friction|Slipperiness) *\\(", family).filter((file) => !/[\\/](Block|AbstractBlock|BlockBehaviour)\.java$/.test(file));
  if (overrides.length > 0) throw new Error(`${release}: ${overrides.join(", ")} override the friction`);

  const legacy = /\bregister\s*\(\s*\d+\s*,\s*"/.test(blockSource);
  const registry = legacy ? blockSource : /\.(friction|slipperiness)\(/.test(blocks) ? blocks : blockSource;
  let frictions;
  if (legacy) {
    // the classes that set it, and down the classes that extend them
    // (the recorder's mapping leaves a legacy block class without a Mojang name in net/minecraft/block, so the whole
    // of net/minecraft is searched)
    const scope = path.join("net", "minecraft");
    const classes = {};
    let found = tree.grep("this\\.(friction|slipperiness) *=", scope);
    while (found.length > 0) {
      for (const file of found) {
        const parsed = parseLegacyClass(tree.read(file) ?? "");
        if (parsed) classes[parsed.name] = parsed;
      }
      const names = Object.keys(classes).join("|");
      found = tree.grep(`class +[A-Za-z0-9_]+ +extends +(${names})( |$)`, scope).filter((file) => !classes[path.basename(file, ".java")]);
    }
    const declared = new Set(tree.grep("^(public |abstract |final )*class ", scope).map((file) => path.basename(file, ".java")));
    declared.add("Block");
    frictions = parseLegacyFriction(registry, classes, (name) => declared.has(name), release);
  } else {
    frictions = parseRegistryFriction(registry, release);
  }
  return {
    source: tree.identity,
    file: path.join(family, registry === blocks ? "Blocks.java" : "Block.java").replaceAll("\\", "/"),
    sha256: sha256(registry.replaceAll("\r\n", "\n")),
    default: defaultFriction,
    frictions,
  };
}

function generate() {
  const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {} : {};
  const releases = {};
  const unverified = [];
  for (const { version } of registeredReleases()) {
    const tree = treeOf(version);
    if (tree) {
      releases[version] = frictionOf(tree, version);
    } else if (previous[version]) {
      releases[version] = previous[version];
    } else {
      unverified.push(version);
    }
  }
  return { schema: 1, generatedBy: "scripts/generate-block-friction.cjs", standIns: STAND_INS, unverified, releases };
}

function main() {
  const output = `${JSON.stringify(generate(), null, 2)}\n`;
  if (process.argv.includes("--check")) {
    if (read(outputPath)?.replaceAll("\r\n", "\n") !== output) throw new Error(`${outputPath} is out of date`);
    return;
  }
  fs.writeFileSync(outputPath, output);
}

if (require.main === module) main();

module.exports = { frictionOf, treeOf };
