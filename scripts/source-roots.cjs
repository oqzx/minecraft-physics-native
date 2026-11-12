// Where a modern release's Mojang-named sources are: its mc-decomp tree
// (generated/minecraft-versions.json sourceRoot), else the client of MC_SOURCE_ROOT (default
// ../../pr/mc-source from the package, 26.2 and 26.3), else the client unpacked from its pinned
// commit or decompiled from its pinned jar under MC_SOURCE_CACHE (scripts/materialize-sources.cjs).
// The embedded version of the last two is checked.
const fs = require("node:fs");
const path = require("node:path");
const { directoryIdentity } = require("./source-identity.cjs");
const { resolveRecordedPath, serializePath } = require("./portable-paths.cjs");
const { mcSourceRoot: sourceRootOf, pinOf, sourceCache } = require("./source-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const manifestPath = path.join(repositoryRoot, "generated", "minecraft-versions.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const decompRoot = resolveRecordedPath(path.dirname(manifestPath), manifest.sourceRoot);
const mcSourceRoot = sourceRootOf();

const versionFamily = (version) => version.split(".").slice(0, 2).join(".");

/** The directory holding net/minecraft for a release, null when no exact source is at hand. */
function modernTree(version) {
  const tree = path.join(decompRoot, versionFamily(version), version);
  if (fs.existsSync(path.join(tree, "net", "minecraft"))) return tree;
  const release = path.join(mcSourceRoot, version);
  const client = path.join(release, "client");
  if (fs.existsSync(path.join(client, "net", "minecraft")) && directoryIdentity(release) === version) {
    return client;
  }
  const unpacked = path.join(sourceCache(), version);
  if (pinOf(version) && fs.existsSync(path.join(unpacked, "client", "net", "minecraft"))
    && directoryIdentity(unpacked) === version) {
    return path.join(unpacked, "client");
  }
  return null;
}

/** MC_LEGACY_SOURCES (default legacy-src beside the source cache): the legacy releases' own jars, decompiled. */
const legacySourcesRoot = () => process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src");

/**
 * The Mojang-named tree of a legacy release (1.8.8 - 1.13.2): its pinned jar remapped by legacy-mappings/<release>.tiny
 * and decompiled by scripts/materialize-sources.cjs, null when it is not at hand.
 */
function legacyTree(version) {
  const tree = path.join(legacySourcesRoot(), version);
  return fs.existsSync(path.join(tree, "net", "minecraft")) ? tree : null;
}

/**
 * How a manifest in `manifestDirectory` records a release's source file: `<pin>:<path>` when the
 * file comes from a pinned commit or client, `mc-source:<path>` when it comes from MC_SOURCE_ROOT,
 * both of which read the same on every host whatever its layout, else its path.
 */
function recordedSource(version, manifestDirectory, filePath) {
  const unpacked = path.join(sourceCache(), version);
  if (pinOf(version) && !path.relative(unpacked, filePath).startsWith("..")) {
    return `${pinOf(version)}:${path.relative(unpacked, filePath).replaceAll(path.sep, "/")}`;
  }
  const fromRoot = path.relative(mcSourceRoot, filePath);
  if (!fromRoot.startsWith("..") && !path.isAbsolute(fromRoot)) return `mc-source:${fromRoot.replaceAll(path.sep, "/")}`;
  return serializePath(manifestDirectory, filePath);
}

module.exports = { modernTree, legacyTree, legacySourcesRoot, recordedSource, versionFamily, repositoryRoot, manifest, manifestPath, decompRoot };
