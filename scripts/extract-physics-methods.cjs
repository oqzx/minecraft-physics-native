/* eslint-disable no-console */
// Writes generated/physics-methods.json: for every release the engine registers
// (scripts/registered-releases.cjs), the tracked physics methods of its vanilla sources, each
// with its line and the sha256 of its text.
//
// A release's files come from its mc-decomp tree (Mojang names, generated/minecraft-versions.json
// sourceRoot). A release whose tree is absent keeps the files of its previous entry as they are.
// A tracked file neither gives is read from another decompile of the release, recorded with its
// `origin` and `mapping`: the tree under MC_DECOMPILED_ROOT (default ../../pr/mc-decompiled from
// the package; Yarn names up to 1.21.4, Mojang names from 1.21.5), else the origin/client<release>
// branch of MC_EXTRACTED_DATA_REPO (Mojang names, read with `git show`, never checked out). The
// releases before the mc-decomp trees (1.8.8 - 1.13.2) are read from MC_DECOMPILED_ROOT like
// scripts/legacy-release-data.cjs (1.8.8 from the 1.8.9 tree). Under Yarn names a method is found
// by its Yarn or intermediary name (`matchedName`); hashes of different origins are not compared
// (scripts/report-physics-breakpoints.cjs).
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { execFileSync } = require("node:child_process");
const { registeredReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const manifestPath = path.join(repositoryRoot, "generated", "minecraft-versions.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const sourceRoot = path.resolve(path.dirname(manifestPath), manifest.sourceRoot);
const outputPath = path.join(repositoryRoot, "generated", "physics-methods.json");
const decompiledRoot = process.env.MC_DECOMPILED_ROOT
  ?? path.join(repositoryRoot, "..", "..", "pr", "mc-decompiled");
const extractedRepo = process.env.MC_EXTRACTED_DATA_REPO
  ?? "D:/projects/mc-zuri/mc-data/extracted_minecraft_data";

// The tracked files by their Mojang names, with their methods.
const targets = [
  ["Entity.java", ["move", "collide", "push", "rideTick"]],
  ["LivingEntity.java", ["travel", "travelInFluid", "handleRelativeFrictionAndCalculateMovement", "causeFallDamage"]],
  ["Player.java", ["travel", "aiStep", "rideTick"]],
  ["LocalPlayer.java", ["aiStep", "sendPosition"]],
  ["Boat.java", ["floatBoat", "controlBoat", "tick"]],
  // Boat's movement moved to AbstractBoat in 1.21.2
  ["AbstractBoat.java", ["floatBoat", "controlBoat", "tick"]],
  ["AbstractMinecart.java", ["moveAlongTrack", "tick"]],
  // the ridden horse's travel (travelRidden's tickRidden and getRiddenInput from 1.19.4)
  ["AbstractHorse.java", ["travel", "tickRidden", "getRiddenInput"]],
];

// Where each tracked file is under each mapping, first found first.
const locations = {
  mojang: {
    "Entity.java": ["net/minecraft/world/entity/Entity.java"],
    "LivingEntity.java": ["net/minecraft/world/entity/LivingEntity.java"],
    "Player.java": ["net/minecraft/world/entity/player/Player.java"],
    "LocalPlayer.java": ["net/minecraft/client/player/LocalPlayer.java"],
    "Boat.java": ["net/minecraft/world/entity/vehicle/Boat.java", "net/minecraft/world/entity/vehicle/boat/Boat.java"],
    "AbstractBoat.java": [
      "net/minecraft/world/entity/vehicle/AbstractBoat.java",
      "net/minecraft/world/entity/vehicle/boat/AbstractBoat.java",
    ],
    "AbstractMinecart.java": [
      "net/minecraft/world/entity/vehicle/AbstractMinecart.java",
      "net/minecraft/world/entity/vehicle/minecart/AbstractMinecart.java",
    ],
    "AbstractHorse.java": [
      "net/minecraft/world/entity/animal/horse/AbstractHorse.java",
      "net/minecraft/world/entity/animal/equine/AbstractHorse.java",
    ],
  },
  yarn: {
    "Entity.java": ["net/minecraft/entity/Entity.java"],
    "LivingEntity.java": ["net/minecraft/entity/LivingEntity.java"],
    "Player.java": ["net/minecraft/entity/player/PlayerEntity.java"],
    "LocalPlayer.java": [
      "net/minecraft/client/network/ClientPlayerEntity.java",
      "net/minecraft/entity/player/ClientPlayerEntity.java",
    ],
    "Boat.java": ["net/minecraft/entity/vehicle/BoatEntity.java"],
    "AbstractBoat.java": ["net/minecraft/entity/vehicle/AbstractBoatEntity.java"],
    "AbstractMinecart.java": ["net/minecraft/entity/vehicle/AbstractMinecartEntity.java"],
    // the horse base class: HorseBaseEntity before 1.11 and again 1.14 - 1.16, AbstractHorseEntity
    // in net.minecraft.entity 1.11 - 1.13
    "AbstractHorse.java": [
      "net/minecraft/entity/passive/AbstractHorseEntity.java",
      "net/minecraft/entity/AbstractHorseEntity.java",
      "net/minecraft/entity/passive/HorseBaseEntity.java",
    ],
  },
};

// The Yarn (and intermediary, for methods Yarn had not named) names of the tracked methods.
const yarnMethodNames = {
  "Entity.java": { collide: ["adjustMovementForCollisions"], push: ["pushAwayFrom"], rideTick: ["tickRiding"] },
  "LivingEntity.java": {
    travel: ["travel", "method_2657"],
    handleRelativeFrictionAndCalculateMovement: ["applyMovementInput"],
    causeFallDamage: ["handleFallDamage"],
  },
  "Player.java": { travel: ["travel", "method_2657"], aiStep: ["tickMovement"], rideTick: ["tickRiding"] },
  "LocalPlayer.java": { aiStep: ["tickMovement"], sendPosition: ["sendMovementPackets"] },
  "Boat.java": { floatBoat: ["updateVelocity"], controlBoat: ["updatePaddles"] },
  "AbstractBoat.java": { floatBoat: ["updateVelocity"], controlBoat: ["updatePaddles"] },
  "AbstractMinecart.java": { moveAlongTrack: ["moveOnRail"] },
  "AbstractHorse.java": {
    travel: ["travel", "method_2657"],
    tickRidden: ["tickControlled"],
    getRiddenInput: ["getControlledMovementInput"],
  },
};

function methodNames(fileName, name, mapping) {
  return mapping === "yarn" ? yarnMethodNames[fileName]?.[name] ?? [name] : [name];
}

function extractMethods(source, fileName, names, mapping) {
  const methods = {};
  for (const name of names) {
    const candidates = [];
    for (const sourceName of methodNames(fileName, name, mapping)) {
      const pattern = new RegExp(
        `^[ \\t]*(?:(?:public|protected|private|final|static|abstract|native|synchronized|strictfp)\\s+)*[\\w$<>, ?\\[\\]]+\\s+${sourceName}\\s*\\([^{}\\n]*\\)\\s*(?:throws [^{\\n]+)?\\{`,
        "gm",
      );
      let match;
      while ((match = pattern.exec(source)) !== null) {
        let depth = 0;
        let end = match.index;
        let inString = false;
        let escaped = false;
        for (; end < source.length; end += 1) {
          const character = source[end];
          if (inString) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === '"') inString = false;
            continue;
          }
          if (character === '"') {
            inString = true;
          } else if (character === "{") {
            depth += 1;
          } else if (character === "}") {
            depth -= 1;
            if (depth === 0) {
              end += 1;
              break;
            }
          }
        }
        if (depth === 0) {
          const text = source.slice(match.index, end);
          const line = source.slice(0, match.index).split("\n").length;
          candidates.push({
            line,
            ...(sourceName !== name ? { matchedName: sourceName } : {}),
            sha256: crypto.createHash("sha256").update(text).digest("hex"),
            source: text,
          });
        }
      }
      if (candidates.length > 0) break;
    }
    if (candidates.length > 0) methods[name] = candidates;
  }
  return methods;
}

/** The files of a release whose mc-decomp tree is present (found by name, Mojang names), else null. */
function mcDecompFiles(version) {
  const versionFamily = version.split(".").slice(0, 2).join(".");
  const versionRoot = path.join(sourceRoot, versionFamily, version);
  if (!fs.existsSync(versionRoot)) return null;
  const files = {};
  for (const [fileName, names] of targets) {
    const matches = [];
    const visit = (directory) => {
      if (!fs.existsSync(directory)) return;
      for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        const entryPath = path.join(directory, entry.name);
        if (entry.isDirectory()) visit(entryPath);
        else if (entry.name === fileName) matches.push(entryPath);
      }
    };
    visit(versionRoot);
    if (matches.length === 0) continue;
    const source = fs.readFileSync(matches[0], "utf8");
    files[fileName] = {
      path: path.relative(sourceRoot, matches[0]),
      fileSha256: crypto.createHash("sha256").update(source).digest("hex"),
      methods: extractMethods(source, fileName, names, "mojang"),
    };
  }
  return files;
}

const gitTrees = new Map();
function gitTree(version) {
  if (!gitTrees.has(version)) {
    let files = null;
    if (fs.existsSync(extractedRepo)) {
      try {
        files = new Set(execFileSync(
          "git",
          ["-C", extractedRepo, "ls-tree", "-r", "--name-only", `origin/client${version}`, "--", "client/net/minecraft"],
          { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], maxBuffer: 256 * 1024 * 1024 },
        ).split("\n").filter(Boolean));
      } catch {
        files = null;
      }
    }
    gitTrees.set(version, files);
  }
  return gitTrees.get(version);
}

/** A tracked file of `sourceRelease` from another decompile: { text, path, origin, mapping } or null. */
function alternativeFile(sourceRelease, fileName) {
  const local = path.join(decompiledRoot, sourceRelease);
  if (fs.existsSync(local)) {
    const mapping = fs.existsSync(path.join(local, "net/minecraft/world/entity/Entity.java")) ? "mojang" : "yarn";
    for (const relativePath of locations[mapping][fileName]) {
      const filePath = path.join(local, relativePath);
      if (fs.existsSync(filePath)) {
        return {
          text: fs.readFileSync(filePath, "utf8"),
          path: `${sourceRelease}/${relativePath}`,
          origin: "mc-decompiled",
          mapping,
        };
      }
    }
    return null;
  }
  const tree = gitTree(sourceRelease);
  if (!tree) return null;
  for (const relativePath of locations.mojang[fileName]) {
    const object = `client/${relativePath}`;
    if (!tree.has(object)) continue;
    const text = execFileSync("git", ["-C", extractedRepo, "show", `origin/client${sourceRelease}:${object}`], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
      maxBuffer: 64 * 1024 * 1024,
    });
    return { text, path: `origin/client${sourceRelease}:${object}`, origin: "extracted-minecraft-data", mapping: "mojang" };
  }
  return null;
}

const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {} : {};
const legacySources = new Map(LEGACY_RELEASES.map((release) => [release.version, release.source]));

const releases = {};
for (const { version } of registeredReleases()) {
  const sourceRelease = legacySources.get(version) ?? version;
  const files = mcDecompFiles(version) ?? { ...(previous[version]?.files ?? {}) };
  for (const [fileName, names] of targets) {
    if (files[fileName]) continue;
    const found = alternativeFile(sourceRelease, fileName);
    if (!found) continue;
    files[fileName] = {
      path: found.path,
      origin: found.origin,
      mapping: found.mapping,
      ...(sourceRelease !== version ? { sourceRelease } : {}),
      fileSha256: crypto.createHash("sha256").update(found.text).digest("hex"),
      methods: extractMethods(found.text, fileName, names, found.mapping),
    };
  }
  releases[version] = {
    files: Object.fromEntries(targets.filter(([fileName]) => files[fileName]).map(([fileName]) => [fileName, files[fileName]])),
    // tracked files no source read for the release has (absent from it, or no source of it at hand)
    absentFiles: targets.map(([fileName]) => fileName).filter((fileName) => !files[fileName]),
    behaviorFeatures: {
      fluidTravelSplit: Boolean(files["LivingEntity.java"]?.methods?.travelInFluid?.length),
      entityPushthrough: Boolean(
        files["Entity.java"]?.methods?.push?.some((method) =>
          /pushthrough/.test(method.source),
        ),
      ),
      clientPredictionSource: Boolean(files["LocalPlayer.java"]),
      boatSource: Boolean(files["Boat.java"] || files["AbstractBoat.java"]),
      minecartSource: Boolean(files["AbstractMinecart.java"]),
      horseSource: Boolean(files["AbstractHorse.java"]),
    },
  };
}

const output = {
  schema: 1,
  generatedBy: "scripts/extract-physics-methods.cjs",
  sourceRoot: manifest.sourceRoot,
  releases,
};
fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`);
console.log(`Extracted physics methods for ${Object.keys(releases).length} releases.`);
