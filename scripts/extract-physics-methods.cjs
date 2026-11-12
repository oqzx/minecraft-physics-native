/* eslint-disable no-console */
// Writes generated/physics-methods.json: for every release the engine registers
// (scripts/registered-releases.cjs), the tracked physics methods of its vanilla sources, each
// with its line and the sha256 of its text.
//
// A modern release's files come from its exact Mojang-named tree (scripts/source-roots.cjs), recorded
// with their `origin` (the decompile they are) and `source`. A release without one fails the run;
// --cached keeps its previous entry instead, marked `cached`. The legacy releases (1.8.8 - 1.13.2) are
// read from their own decompiled jars under MC_LEGACY_SOURCES like scripts/legacy-release-data.cjs, whose
// mapping gives these files and methods their Mojang names (origin `legacy-jar`); hashes of different
// origins are not compared (scripts/report-physics-breakpoints.cjs). A file absent from a source that was
// read does not exist in the release; `absentMethods` are a present file's targets it does not declare.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { registeredReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { methodsNamed } = require("./java-declarations.cjs");
const { modernTree, legacyTree, recordedSource } = require("./source-roots.cjs");
const { PINNED_SOURCES, PINNED_CLIENTS } = require("./source-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "physics-methods.json");

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
  // projectiles: the base tick, each family's motion, hit selection and impact callbacks
  ["Projectile.java", ["tick", "shoot", "shootFromRotation", "checkLeftOwner", "canHitEntity", "onHit", "onHitEntity", "onHitBlock", "updateRotation"]],
  ["ProjectileUtil.java", ["getHitResult", "getHitResultOnMoveVector", "getEntityHitResult", "rotateTowardsMovement"]],
  ["ThrowableProjectile.java", ["tick", "getGravity", "getDefaultGravity", "applyInertia", "handleFirstTickBubbleColumn"]],
  ["AbstractArrow.java", ["tick", "shoot", "findHitEntity", "onHitEntity", "onHitBlock", "canHitEntity", "getWaterInertia", "stepMoveAndHit", "applyInertia", "getDefaultGravity"]],
  ["ThrownTrident.java", ["tick", "findHitEntity", "onHitEntity", "onHitBlock", "getWaterInertia", "tryPickup"]],
  ["AbstractHurtingProjectile.java", ["tick", "getInertia", "getLiquidInertia", "canHitEntity", "applyInertia"]],
  ["FireworkRocketEntity.java", ["tick", "explode"]],
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
    "Projectile.java": ["net/minecraft/world/entity/projectile/Projectile.java"],
    "ProjectileUtil.java": ["net/minecraft/world/entity/projectile/ProjectileUtil.java"],
    "ThrowableProjectile.java": ["net/minecraft/world/entity/projectile/ThrowableProjectile.java"],
    "AbstractArrow.java": [
      "net/minecraft/world/entity/projectile/AbstractArrow.java",
      "net/minecraft/world/entity/projectile/arrow/AbstractArrow.java",
    ],
    "ThrownTrident.java": [
      "net/minecraft/world/entity/projectile/ThrownTrident.java",
      "net/minecraft/world/entity/projectile/arrow/ThrownTrident.java",
    ],
    "AbstractHurtingProjectile.java": [
      "net/minecraft/world/entity/projectile/AbstractHurtingProjectile.java",
      "net/minecraft/world/entity/projectile/hurtingprojectile/AbstractHurtingProjectile.java",
    ],
    "FireworkRocketEntity.java": ["net/minecraft/world/entity/projectile/FireworkRocketEntity.java"],
  },
};

// (each tracked method under its Mojang name: the legacy decompiles are Mojang-named too)
const methodNames = (fileName, name) => [name];

function extractMethods(source, fileName, names, mapping) {
  const methods = {};
  for (const name of names) {
    for (const sourceName of methodNames(fileName, name, mapping)) {
      const declarations = methodsNamed(source, sourceName);
      if (declarations.length === 0) continue;
      methods[name] = declarations.map(({ declaringClass, signature, line, sha256, source: text }) => ({
        declaringClass,
        signature,
        line,
        ...(sourceName !== name ? { matchedName: sourceName } : {}),
        sha256,
        source: text,
      }));
      break;
    }
  }
  return methods;
}

const originOf = (version) => (PINNED_SOURCES[version] ? "extracted-minecraft-data" : PINNED_CLIENTS[version] ? "client-jar" : "mc-source");

/** The files of a modern release from its exact tree, else null. */
function treeFiles(version) {
  const tree = modernTree(version);
  if (!tree) return null;
  const files = {};
  for (const [fileName, names] of targets) {
    const relativePath = locations.mojang[fileName].find((candidate) => fs.existsSync(path.join(tree, candidate)));
    if (!relativePath) continue;
    const filePath = path.join(tree, relativePath);
    const source = fs.readFileSync(filePath, "utf8");
    files[fileName] = {
      path: recordedSource(version, path.dirname(outputPath), filePath),
      origin: originOf(version),
      mapping: "mojang",
      fileSha256: crypto.createHash("sha256").update(source).digest("hex"),
      methods: extractMethods(source, fileName, names, "mojang"),
    };
  }
  return files;
}

/** A tracked file of a legacy release's decompiled jar: { text, path, origin, mapping } or null. */
function legacyFile(version, fileName) {
  const tree = legacyTree(version);
  for (const relativePath of locations.mojang[fileName] ?? []) {
    const filePath = path.join(tree, relativePath);
    if (fs.existsSync(filePath)) {
      return { text: fs.readFileSync(filePath, "utf8"), path: `${version}/${relativePath}`, origin: "legacy-jar", mapping: "mojang" };
    }
  }
  return null;
}

const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {} : {};
const legacySources = new Map(LEGACY_RELEASES.map((release) => [release.version, release.source]));

const cachedMode = process.argv.includes("--cached");
const releases = {};
const unavailable = [];
for (const { version } of registeredReleases()) {
  const sourceRelease = legacySources.get(version) ?? version;
  const legacy = legacySources.has(version);
  const tree = legacy ? null : treeFiles(version);
  if (!legacy && !tree) {
    if (cachedMode && previous[version]) releases[version] = { ...previous[version], cached: true };
    else unavailable.push(version);
    continue;
  }
  if (legacy && !legacyTree(version)) {
    if (cachedMode && previous[version]) releases[version] = { ...previous[version], cached: true };
    else unavailable.push(version);
    continue;
  }
  const files = tree ?? {};
  for (const [fileName, names] of legacy ? targets : []) {
    const found = legacyFile(version, fileName);
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
    absentMethods: Object.fromEntries(targets.filter(([fileName]) => files[fileName]).flatMap(([fileName, names]) => {
      const missing = names.filter((name) => !files[fileName].methods[name]);
      return missing.length > 0 ? [[fileName, missing]] : [];
    })),
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

if (unavailable.length > 0) {
  throw new Error(`No sources of ${unavailable.join(", ")}: run scripts/materialize-sources.cjs, or pass --cached to keep their entries`);
}

const output = {
  schema: 2,
  generatedBy: "scripts/extract-physics-methods.cjs",
  releases,
};
fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`);
console.log(`Extracted physics methods for ${Object.keys(releases).length} releases.`);
