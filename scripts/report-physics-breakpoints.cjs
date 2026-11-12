/* eslint-disable no-console */
// Writes generated/physics-breakpoints.json: for each pair of adjacent registered releases
// (scripts/registered-releases.cjs), what physics reads that differs between them, by source:
//
// - source: the tracked vanilla methods (generated/physics-methods.json), compared only between
//   files of the same decompile (origin and mapping); the others are `unavailable`;
// - mappings: the member fingerprints of the classes physics reads in Mojang's client mappings
//   (generated/mojang-releases.json, 1.14.4 on);
// - registries: the block registry and block states of the generated release data
//   (native/data/releases), and the entities (with their sizes), effects and attributes of
//   minecraft-data;
// - dataPacks: the block and fluid tags of the generated release data;
// - protocol: minecraft-data's protocol version and the definitions of the packets that carry
//   movement, velocity, abilities, attributes and effects.
//
// A pair is listed when any of them differs or a source is missing for either release.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { registeredReleases } = require("./registered-releases.cjs");

const root = path.resolve(__dirname, "..");
const readJson = (relative) => {
  const file = path.join(root, relative);
  return fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : null;
};
const audit = readJson("generated/physics-methods.json");
const mojang = readJson("generated/mojang-releases.json");
const versions = registeredReleases().map(({ version }) => version);
const sha256 = (value) => crypto.createHash("sha256").update(JSON.stringify(value)).digest("hex");

// --- source ---------------------------------------------------------------------------------------

// Every method the source audit tracks (scripts/extract-physics-methods.cjs), in the order it
// first lists them.
const methods = [];
for (const release of versions) {
  for (const [file, entry] of Object.entries(audit.releases[release]?.files ?? {})) {
    for (const method of Object.keys(entry.methods ?? {})) {
      const key = `${file}:${method}`;
      if (!methods.includes(key)) methods.push(key);
    }
  }
}

function fileEntry(release, key) {
  return audit.releases[release]?.files?.[key.split(":")[0]];
}

function hashes(release, key) {
  return (fileEntry(release, key)?.methods?.[key.split(":")[1]] ?? []).map((entry) => entry.sha256).join(",");
}

/** The decompile a file entry comes from: the mc-decomp tree unless it records another. */
function decompile(entry) {
  return `${entry.origin ?? "mc-decomp"}/${entry.mapping ?? "mojang"}`;
}

function comparableSource(previous, current, key) {
  const left = fileEntry(previous, key);
  const right = fileEntry(current, key);
  return Boolean(left && right && decompile(left) === decompile(right));
}

// --- mappings -------------------------------------------------------------------------------------

const mappingsByRelease = new Map((mojang?.releases ?? []).map((release) => [release.id, release.mappings ?? null]));
const comparedClasses = mojang?.mappedClasses ?? [];

// --- generated release data (registries and data packs) -------------------------------------------

function releaseData(release) {
  const file = path.join(root, "native", "data", "releases", `v${release.replaceAll(".", "_")}.rs`);
  if (!fs.existsSync(file)) return null;
  const source = fs.readFileSync(file, "utf8");
  const arrays = new Map();
  for (const match of source.matchAll(/pub static (\w+): &\[&str\] = &\[(.*?)\];/g)) {
    arrays.set(match[1], [...match[2].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((name) => name[1]));
  }
  const blocks = {};
  for (const match of source.matchAll(/GeneratedBlock \{ id: "([^"]+)",.*?property_order: (\w+), state_count: (Some\((\d+)\)|None)/g)) {
    blocks[match[1]] = { properties: arrays.get(match[2]) ?? [], states: match[4] ? Number(match[4]) : null };
  }
  const blockTags = {};
  for (const match of source.matchAll(/GeneratedBlockTag \{ id: "([^"]+)",.*?blocks: (\w+), includes: (\w+),/g)) {
    blockTags[match[1]] = sha256({
      blocks: [...(arrays.get(match[2]) ?? [])].sort(),
      includes: [...(arrays.get(match[3]) ?? [])].sort(),
    });
  }
  const fluidTags = [...source.matchAll(/GeneratedFluidTag \{ id: "([^"]+)"/g)].map((match) => match[1]);
  return { blocks, blockTags, fluidTags: [...new Set(fluidTags)].sort() };
}

// --- minecraft-data (registries and protocol) -----------------------------------------------------

let minecraftData = null;
try {
  minecraftData = require("minecraft-data");
} catch (error) {
  // the categories it feeds would quietly vanish from the report
  if (require.main === module && !process.argv.includes("--cached")) throw new Error(`minecraft-data does not load: ${error.message}; run scripts/bootstrap-inputs.cjs`);
  minecraftData = null;
}

const comparedPackets = {
  toServer: ["flying", "position", "look", "position_look", "entity_action", "abilities", "steer_vehicle", "player_input", "vehicle_move"],
  toClient: [
    "position", "entity_velocity", "explosion", "abilities", "update_attributes", "entity_update_attributes",
    "entity_effect", "remove_entity_effect", "vehicle_move", "sync_entity_position", "game_state_change",
    "set_passengers", "attach_entity", "initialize_world_border",
  ],
};

function dataOf(release) {
  const data = minecraftData?.(release);
  if (!data || data.version?.minecraftVersion !== release) return null;
  const packets = {};
  for (const [direction, names] of Object.entries(comparedPackets)) {
    const types = data.protocol?.play?.[direction]?.types ?? {};
    for (const name of names) {
      if (types[`packet_${name}`]) packets[`${direction}:${name}`] = sha256(types[`packet_${name}`]);
    }
  }
  return {
    protocolVersion: data.version.version,
    packets,
    entities: Object.fromEntries((data.entitiesArray ?? []).map((entity) => [entity.name, `${entity.width}x${entity.height}`])),
    effects: (data.effectsArray ?? []).map((effect) => effect.name).sort(),
    attributes: Object.fromEntries((data.attributesArray ?? []).map((attribute) => [
      attribute.resource ?? attribute.name,
      `${attribute.default}:${attribute.min}:${attribute.max}`,
    ])),
  };
}

// --- comparison -----------------------------------------------------------------------------------

/** Keys added, removed, and (for maps) changed between two key sets or maps. */
function difference(left, right) {
  const leftMap = Array.isArray(left) ? Object.fromEntries(left.map((key) => [key, true])) : left;
  const rightMap = Array.isArray(right) ? Object.fromEntries(right.map((key) => [key, true])) : right;
  const added = Object.keys(rightMap).filter((key) => !(key in leftMap));
  const removed = Object.keys(leftMap).filter((key) => !(key in rightMap));
  const changed = Object.keys(rightMap).filter((key) => key in leftMap && JSON.stringify(leftMap[key]) !== JSON.stringify(rightMap[key]));
  const result = {};
  if (added.length > 0) result.added = added;
  if (removed.length > 0) result.removed = removed;
  if (changed.length > 0) result.changed = changed;
  return Object.keys(result).length > 0 ? result : null;
}

const breakpoints = [];
for (let index = 1; index < versions.length; index += 1) {
  const previous = versions[index - 1];
  const current = versions[index];
  const entry = { previous, current, categories: [] };
  const unavailableSources = [];

  const comparable = methods.filter((method) => comparableSource(previous, current, method));
  entry.changed = comparable.filter((method) => hashes(previous, method) !== hashes(current, method));
  const unavailable = methods.filter((method) => !comparable.includes(method));
  if (entry.changed.length > 0) entry.categories.push("source");
  if (unavailable.length > 0) entry.unavailable = unavailable;

  const leftMappings = mappingsByRelease.get(previous);
  const rightMappings = mappingsByRelease.get(current);
  if (leftMappings && rightMappings) {
    const mappings = difference(
      Object.fromEntries(Object.entries(leftMappings.classes).map(([name, value]) => [name, value.sha256])),
      Object.fromEntries(Object.entries(rightMappings.classes).map(([name, value]) => [name, value.sha256])),
    );
    if (mappings) {
      entry.mappings = mappings;
      entry.categories.push("mappings");
    }
  } else {
    unavailableSources.push("mappings");
  }

  const leftData = releaseData(previous);
  const rightData = releaseData(current);
  const leftMinecraftData = dataOf(previous);
  const rightMinecraftData = dataOf(current);
  const registries = {};
  const dataPacks = {};
  if (leftData && rightData) {
    const blocks = difference(Object.keys(leftData.blocks), Object.keys(rightData.blocks));
    if (blocks) registries.blocks = blocks;
    const states = difference(leftData.blocks, rightData.blocks)?.changed;
    if (states) registries.blockStates = states;
    const blockTags = difference(leftData.blockTags, rightData.blockTags);
    if (blockTags) dataPacks.blockTags = blockTags;
    const fluidTags = difference(leftData.fluidTags, rightData.fluidTags);
    if (fluidTags) dataPacks.fluidTags = fluidTags;
  } else {
    unavailableSources.push("releaseData");
  }
  if (leftMinecraftData && rightMinecraftData) {
    for (const name of ["entities", "effects", "attributes"]) {
      const changed = difference(leftMinecraftData[name], rightMinecraftData[name]);
      if (changed) registries[name] = changed;
    }
    const packets = difference(leftMinecraftData.packets, rightMinecraftData.packets);
    entry.protocol = {
      previous: leftMinecraftData.protocolVersion,
      current: rightMinecraftData.protocolVersion,
      ...(packets ? { packets } : {}),
    };
    if (packets) entry.categories.push("protocol");
  } else {
    unavailableSources.push("minecraftData");
  }
  if (Object.keys(registries).length > 0) {
    entry.registries = registries;
    entry.categories.push("registries");
  }
  if (Object.keys(dataPacks).length > 0) {
    entry.dataPacks = dataPacks;
    entry.categories.push("dataPacks");
  }
  if (unavailableSources.length > 0) entry.unavailableSources = unavailableSources;
  if (entry.categories.length > 0 || unavailable.length > 0 || unavailableSources.length > 0) breakpoints.push(entry);
}

const output = {
  schema: 3,
  sourceAudit: "generated/physics-methods.json",
  mappings: "generated/mojang-releases.json",
  releaseData: "native/data/releases",
  releases: versions,
  comparedMethods: methods,
  comparedClasses,
  comparedPackets,
  breakpoints,
};
const outputPath = path.join(root, "generated", "physics-breakpoints.json");
fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`);
console.log(`Recorded ${breakpoints.length} physics breakpoints across ${versions.length} releases.`);
