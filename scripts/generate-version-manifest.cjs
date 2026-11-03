const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

const sourceRoot = path.resolve(process.env.MC_DECOMP_SRC ?? "..\\mc-decomp\\src");
const outputPath = path.resolve(process.env.MC_PHYSICS_VERSION_MANIFEST ?? "generated\\minecraft-versions.json");

const requiredFiles = [
  "net/minecraft/world/entity/Entity.java",
  "net/minecraft/world/entity/LivingEntity.java",
  "net/minecraft/world/entity/player/Player.java",
  "net/minecraft/world/phys/shapes/VoxelShape.java",
  "net/minecraft/world/phys/AABB.java",
];
const optionalFiles = [
  "net/minecraft/world/entity/player/LocalPlayer.java",
  "net/minecraft/world/entity/animal/horse/AbstractHorse.java",
  "net/minecraft/world/entity/vehicle/Boat.java",
  "net/minecraft/world/entity/vehicle/AbstractMinecart.java",
];
const trackedMethods = {
  "net/minecraft/world/entity/Entity.java": ["move"],
  "net/minecraft/world/entity/LivingEntity.java": ["travel"],
  "net/minecraft/world/entity/player/Player.java": ["travel"],
  "net/minecraft/world/entity/player/LocalPlayer.java": ["aiStep", "travel"],
  "net/minecraft/world/entity/animal/horse/AbstractHorse.java": ["travel"],
  "net/minecraft/world/entity/vehicle/Boat.java": ["controlBoat", "tick"],
  "net/minecraft/world/entity/vehicle/AbstractMinecart.java": ["moveAlongTrack", "comeOffTrack"],
};
const versionPattern = /^\d+\.\d+(?:\.\d+)?(?:[-+].*)?$/;

function compareVersions(left, right) {
  const leftParts = left.split(/[.+-]/).map((part) => Number(part) || 0);
  const rightParts = right.split(/[.+-]/).map((part) => Number(part) || 0);
  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return left.localeCompare(right);
}

function extractMethod(source, methodName) {
  const methodStart = source.search(new RegExp(`\\b${methodName}\\s*\\(`));
  if (methodStart < 0) return null;
  const bodyStart = source.indexOf("{", methodStart);
  if (bodyStart < 0) return null;
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        const body = source.slice(methodStart, index + 1);
        return {
          sha256: crypto.createHash("sha256").update(body).digest("hex"),
          start: methodStart,
          end: index + 1,
          bytes: Buffer.byteLength(body),
        };
      }
    }
  }
  return null;
}

const versions = [];
const sourceFiles = {};
const incompleteVersions = [];
if (fs.existsSync(sourceRoot)) {
  for (const era of fs.readdirSync(sourceRoot, { withFileTypes: true })) {
    if (!era.isDirectory() || !/^\d+\.\d+$/.test(era.name)) continue;
    const eraPath = path.join(sourceRoot, era.name);
    for (const release of fs.readdirSync(eraPath, { withFileTypes: true })) {
      if (release.isDirectory() && versionPattern.test(release.name)) {
        const version = release.name;
        const versionRoot = path.join(eraPath, release.name);
        const hasSource = [...requiredFiles, ...optionalFiles]
          .some((relativePath) => fs.existsSync(path.join(versionRoot, relativePath)));
        if (!hasSource) continue;
        versions.push(version);
        const missingRequired = requiredFiles.filter((relativePath) => {
          return !fs.existsSync(path.join(versionRoot, relativePath));
        });
        if (missingRequired.length > 0) incompleteVersions.push({ version, missingRequired });
        const files = [...requiredFiles, ...optionalFiles]
          .map((relativePath) => {
            const filePath = path.join(versionRoot, relativePath);
            if (!fs.existsSync(filePath)) return { path: relativePath, required: requiredFiles.includes(relativePath), present: false };
            const source = fs.readFileSync(filePath);
            return {
              path: relativePath.replaceAll(path.sep, "/"),
              required: requiredFiles.includes(relativePath),
              present: true,
              sha256: crypto.createHash("sha256").update(source).digest("hex"),
              bytes: source.byteLength,
            };
          })
        sourceFiles[version] = { files, methods: {} };
        for (const [relativePath, methodNames] of Object.entries(trackedMethods)) {
          const filePath = path.join(versionRoot, relativePath);
          if (!fs.existsSync(filePath)) continue;
          const source = fs.readFileSync(filePath, "utf8");
          for (const methodName of methodNames) {
            const method = extractMethod(source, methodName);
            if (method) sourceFiles[version].methods[`${relativePath}#${methodName}`] = method;
          }
        }
      }
    }
  }
}

versions.sort(compareVersions);
if (incompleteVersions.length > 0) {
  const details = incompleteVersions
    .map(({ version, missingRequired }) => `${version}: ${missingRequired.join(", ")}`)
    .join("; ");
  throw new Error(`Incomplete physics source snapshots: ${details}`);
}
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
const methodDiffs = {};
for (let index = 1; index < versions.length; index += 1) {
  const previous = versions[index - 1];
  const current = versions[index];
  const previousMethods = sourceFiles[previous].methods;
  const currentMethods = sourceFiles[current].methods;
  const methodNames = new Set([...Object.keys(previousMethods), ...Object.keys(currentMethods)]);
  const changed = [...methodNames].filter((methodName) => {
    return previousMethods[methodName]?.sha256 !== currentMethods[methodName]?.sha256;
  });
  if (changed.length > 0) methodDiffs[`${previous}->${current}`] = changed;
}
fs.writeFileSync(outputPath, `${JSON.stringify({
  sourceRoot: path.relative(path.dirname(outputPath), sourceRoot),
  requiredFiles,
  optionalFiles,
  versions,
  incompleteVersions,
  sourceFiles,
  methodDiffs,
}, null, 2)}\n`);
