const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { methodsNamed } = require("./java-declarations.cjs");
const { serializePath } = require("./portable-paths.cjs");
const { registeredReleases } = require("./registered-releases.cjs");
const { modernTree, recordedSource } = require("./source-roots.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { PINNED_SOURCES, PINNED_CLIENTS } = require("./source-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const sourceRoot = path.resolve(process.env.MC_DECOMP_SRC ?? path.join(repositoryRoot, "..", "mc-decomp", "src"));
const outputPath = path.resolve(process.env.MC_PHYSICS_VERSION_MANIFEST ?? path.join(repositoryRoot, "generated", "minecraft-versions.json"));

const requiredFiles = [
  "net/minecraft/world/entity/Entity.java",
  "net/minecraft/world/entity/LivingEntity.java",
  "net/minecraft/world/entity/player/Player.java",
  "net/minecraft/world/phys/shapes/VoxelShape.java",
  "net/minecraft/world/phys/AABB.java",
];
const optionalFiles = [
  "net/minecraft/client/player/LocalPlayer.java",
  "net/minecraft/world/entity/animal/horse/AbstractHorse.java",
  "net/minecraft/world/entity/vehicle/Boat.java",
  "net/minecraft/world/entity/vehicle/AbstractBoat.java",
  "net/minecraft/world/entity/vehicle/AbstractMinecart.java",
];
// where a tracked file moved to, by the release that moved it
const relocations = {
  "net/minecraft/world/entity/animal/horse/AbstractHorse.java": ["net/minecraft/world/entity/animal/equine/AbstractHorse.java"],
  "net/minecraft/world/entity/vehicle/Boat.java": ["net/minecraft/world/entity/vehicle/boat/Boat.java"],
  "net/minecraft/world/entity/vehicle/AbstractBoat.java": ["net/minecraft/world/entity/vehicle/boat/AbstractBoat.java"],
  "net/minecraft/world/entity/vehicle/AbstractMinecart.java": ["net/minecraft/world/entity/vehicle/minecart/AbstractMinecart.java"],
};
const trackedMethods = {
  "net/minecraft/world/entity/Entity.java": ["move"],
  "net/minecraft/world/entity/LivingEntity.java": ["travel"],
  "net/minecraft/world/entity/player/Player.java": ["travel"],
  "net/minecraft/client/player/LocalPlayer.java": ["aiStep"],
  "net/minecraft/world/entity/animal/horse/AbstractHorse.java": ["travel"],
  "net/minecraft/world/entity/vehicle/Boat.java": ["controlBoat", "tick"],
  "net/minecraft/world/entity/vehicle/AbstractBoat.java": ["controlBoat", "tick"],
  "net/minecraft/world/entity/vehicle/AbstractMinecart.java": ["moveAlongTrack", "comeOffTrack"],
};
// A directory holding sources is not a release the engine supports: a snapshot, -pre or -rc tree, or
// a release nobody registered, stays out of the manifest and of the method diffs until it has its
// own module in native/src/versioned, under its own full name.
const admittedVersions = () => new Set(registeredReleases().map(({ version }) => version));

function compareVersions(left, right) {
  const leftParts = left.split(/[.+-]/).map((part) => Number(part) || 0);
  const rightParts = right.split(/[.+-]/).map((part) => Number(part) || 0);
  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return left.localeCompare(right);
}

/** The declarations of a method, each overload with its signature and span, never a call. */
function extractMethod(source, methodName) {
  const declarations = methodsNamed(source, methodName);
  if (declarations.length === 0) return null;
  const text = declarations.map((declaration) => declaration.source).join("\n");
  return {
    sha256: crypto.createHash("sha256").update(text).digest("hex"),
    start: declarations[0].start,
    end: declarations.at(-1).end,
    line: declarations[0].line,
    bytes: Buffer.byteLength(text),
    ...(declarations.length > 1 ? { overloads: declarations.length } : {}),
    declarations: declarations.map(({ declaringClass, signature, line, start, end, source: body }) => ({
      declaringClass,
      signature,
      line,
      start,
      end,
      bytes: Buffer.byteLength(body),
      sha256: crypto.createHash("sha256").update(body).digest("hex"),
    })),
  };
}

/** Which decompile a release's text is: hashes compare only within one. */
function decompileOf(version) {
  if (PINNED_SOURCES[version]) return "extracted-minecraft-data";
  if (PINNED_CLIENTS[version]) return "vineflower";
  return "mc-source";
}

/** The first present location of a tracked file under `tree`. */
function locate(tree, relativePath) {
  return [relativePath, ...(relocations[relativePath] ?? [])].find((candidate) => fs.existsSync(path.join(tree, candidate))) ?? null;
}

/** A release's entry from its sources: every file's hash and every tracked declaration. */
function releaseEntry(version, versionRoot) {
  const files = [...requiredFiles, ...optionalFiles].map((relativePath) => {
    const found = locate(versionRoot, relativePath);
    const required = requiredFiles.includes(relativePath);
    if (!found) return { path: relativePath, required, present: false };
    const filePath = path.join(versionRoot, found);
    const source = fs.readFileSync(filePath);
    return {
      path: found,
      required,
      present: true,
      source: recordedSource(version, path.dirname(outputPath), filePath),
      sha256: crypto.createHash("sha256").update(source).digest("hex"),
      bytes: source.byteLength,
    };
  });
  const methods = {};
  for (const [relativePath, methodNames] of Object.entries(trackedMethods)) {
    const found = locate(versionRoot, relativePath);
    if (!found) continue;
    const source = fs.readFileSync(path.join(versionRoot, found), "utf8");
    for (const methodName of methodNames) {
      const method = extractMethod(source, methodName);
      if (method) methods[`${relativePath}#${methodName}`] = method;
    }
  }
  return { decompile: decompileOf(version), files, methods };
}

function main() {
const cachedMode = process.argv.includes("--cached");
const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")) : null;
const admitted = admittedVersions();
const legacy = new Set(LEGACY_RELEASES.map(({ version }) => version));
const versions = [];
const sourceFiles = {};
const incompleteVersions = [];
const absent = [];
const decompTree = (version) => path.join(sourceRoot, version.split(".").slice(0, 2).join("."), version);
for (const version of [...admitted].filter((candidate) => !legacy.has(candidate))) {
  // a tree named by MC_DECOMP_SRC stands alone: only its releases are read
  const named = process.env.MC_DECOMP_SRC !== undefined;
  const versionRoot = fs.existsSync(decompTree(version)) ? decompTree(version) : named ? null : modernTree(version);
  if (!versionRoot) {
    if (previous?.sourceFiles?.[version] && cachedMode) {
      versions.push(version);
      sourceFiles[version] = previous.sourceFiles[version];
    } else if (process.env.MC_DECOMP_SRC === undefined) {
      absent.push(version);
    }
    continue;
  }
  const hasSource = [...requiredFiles, ...optionalFiles].some((relativePath) => locate(versionRoot, relativePath));
  if (!hasSource) continue;
  versions.push(version);
  const missingRequired = requiredFiles.filter((relativePath) => !fs.existsSync(path.join(versionRoot, relativePath)));
  if (missingRequired.length > 0) incompleteVersions.push({ version, missingRequired });
  sourceFiles[version] = releaseEntry(version, versionRoot);
}
if (absent.length > 0) {
  throw new Error(`No sources of ${absent.join(", ")}: run scripts/materialize-sources.cjs, or pass --cached to keep their entries`);
}
versions.sort(compareVersions);
if (versions.length === 0) throw new Error(`No sources at ${sourceRoot} and no manifest to keep`);
if (incompleteVersions.length > 0) {
  const details = incompleteVersions
    .map(({ version, missingRequired }) => `${version}: ${missingRequired.join(", ")}`)
    .join("; ");
  throw new Error(`Incomplete physics source snapshots: ${details}`);
}
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
const methodDiffs = {};
const unavailableDiffs = [];
for (let index = 1; index < versions.length; index += 1) {
  const previous = versions[index - 1];
  const current = versions[index];
  // text of two decompilers differs where the code does not: not a change, and not evidence of none
  if (sourceFiles[previous].decompile !== sourceFiles[current].decompile) {
    unavailableDiffs.push(`${previous}->${current}`);
    continue;
  }
  const previousMethods = sourceFiles[previous].methods;
  const currentMethods = sourceFiles[current].methods;
  const methodNames = new Set([...Object.keys(previousMethods), ...Object.keys(currentMethods)]);
  const changed = [...methodNames].filter((methodName) => {
    return previousMethods[methodName]?.sha256 !== currentMethods[methodName]?.sha256;
  });
  if (changed.length > 0) methodDiffs[`${previous}->${current}`] = changed;
}
fs.writeFileSync(outputPath, `${JSON.stringify({
  sourceRoot: serializePath(path.dirname(outputPath), sourceRoot),
  requiredFiles,
  optionalFiles,
  versions,
  incompleteVersions,
  sourceFiles,
  methodDiffs,
  unavailableDiffs,
}, null, 2)}\n`);
}

if (require.main === module) main();

module.exports = { extractMethod };
