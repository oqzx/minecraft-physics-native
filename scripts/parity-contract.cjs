// The parity contract (B001): every entity family and interaction the package promises, with what
// each accepts, owns, reports and who has authority over it, and the scenarios and entry points that
// stand behind it. A cell is one release of one interaction, and its status is computed, never set:
//
//   unavailable  the release has no such thing (`since`), so it is absent, not approximated
//   implemented  an entry point serves it and no trace has been pinned for this release
//   partial      a known gap, or pinned traces that cover part of the scenarios or do not all match
//   verified     pinned traces of every scenario of the cell, all matching, on the current scope,
//                source, tables and engine
//
// `requires` names the context the interaction cannot run without: world, passenger, target, random,
// tick order. docs/parity-contract.md says how to read this file. Changing an interaction changes its
// scope hash, and that alone takes the claim of every release pinned to the old one.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { canonicalReleases } = require("./release-inventory.cjs");
const { compareReleases } = require("./registered-releases.cjs");
const { codeLines } = require("./shared-gates.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const WORLD = "world";
const PASSENGER = "passenger";
const TARGET = "target";
const RANDOM = "random";
const ORDER = "tick order";
const FIRST = "1.8.8";

const PLAYER_STATE = "position, velocity, rotation, pose, flags, fall distance, jump and sprint counters";
const PLAYER_OUTPUT = "position, velocity, bounding box, pose, collision and fluid flags, fall distance";
const CLIENT = "client prediction, corrected by the server's position packets";
const ENTITY_OUTPUT = "position, velocity, on-ground, fluid and removal state";

const player = (id, title, scenarios, extra = {}) => ({
  id: `player.${id}`, family: "player", title, scenarios,
  inputs: "keys, rotation, attributes, effects, abilities",
  state: PLAYER_STATE,
  outputs: PLAYER_OUTPUT,
  authority: CLIENT,
  requires: [WORLD, ORDER],
  api: ["simulateVersionedTick", "NativePhysics"],
  since: FIRST,
  ...extra,
});

const INTERACTIONS = [
  player("ground", "walking, sprinting, sneaking, jumping and stepping", ["walk/*", "jump/*", "surface/*", "physrec/g_*", "physrec/j_*", "physrec/s_*"]),
  player("fluid", "swimming, wading and fluid currents", ["physrec/wa_*", "physrec/lf_*", "physrec/fj_*", "physrec/jo_*", "physrec/mf_*"]),
  player("climb", "ladders and climbables", ["physrec/cc_*", "physrec/fc_*"]),
  player("effects", "movement effects and attributes", ["physrec/e_*", "physrec/gr_*"]),
  player("flight", "creative and spectator flight", ["physrec/f_creative*", "physrec/f_fly*", "physrec/f_spectator*"]),
  player("elytra", "elytra flight", ["physrec/f_elytra*"], { since: "1.9.4", item: "elytra" }),
  player("riptide", "riptide launches", ["physrec/q_riptide*"], { since: "1.13.2", item: "trident" }),
  player("fall", "landing, fall distance and fall damage", ["physrec/fd_*", "physrec/b_land*"], {
    outputs: `${PLAYER_OUTPUT}, fall damage`,
    authority: "the server deals the damage; the client predicts the landing",
  }),
  player("hazards", "fire, freezing and contact damage", ["physrec/hz_*"], {
    outputs: `${PLAYER_OUTPUT}, fire and freeze counters, damage events`,
    authority: "server-only effects are reported, never applied by a client state (docs/authority.md)",
    requires: [WORLD, ORDER, RANDOM],
  }),
  player("piston", "moving pistons and block pushes", ["physrec/pm_*", "physrec/ph_*"], {
    requires: [WORLD, ORDER],
    inputs: "moving block entities with their progress, which mineflayer does not observe (docs/boundary-inputs.md)",
  }),
  player("entities", "entity pushes and knockback", ["physrec/eo_*", "physrec/et_*", "physrec/x_knockback*"], {
    requires: [WORLD, TARGET, ORDER],
  }),
  {
    id: "player.adapter", family: "player", title: "the mineflayer adapter's own decisions", scenarios: ["physrec/j_*", "physrec/s_*"],
    inputs: "the bot, its registry and its world", state: PLAYER_STATE, outputs: PLAYER_OUTPUT, authority: CLIENT,
    requires: [WORLD, ORDER], api: ["createBotPhysics", "createNativePhysics"], since: FIRST, replay: ["--adapter"],
  },
  {
    id: "living.mob", family: "living", title: "mob travel", scenarios: ["physrec/me_mob*"],
    inputs: "the mob's kind, attributes and input", state: "position, velocity, flags", outputs: ENTITY_OUTPUT,
    authority: "the server's entity; mineflayer sees it interpolated", requires: [WORLD, ORDER],
    api: ["simulateVersionedMobTravel", "simulateVersionedEntityTicks"], since: FIRST,
  },
  {
    id: "boat.control", family: "boat", title: "boat control and water states", scenarios: ["physrec/v_boat*", "physrec/v_raft*"],
    inputs: "paddles and rotation", state: "boat status, last-y, water level, paddle state", outputs: ENTITY_OUTPUT,
    authority: "the controlling client", requires: [WORLD, PASSENGER, ORDER],
    api: ["simulateVersionedBoatTick", "NativeBoatPhysics"], since: FIRST,
  },
  ...[
    ["horse", "horses and donkeys", ["physrec/v_horse*", "physrec/v_donkey*"], FIRST, null],
    ["pig", "pigs", ["physrec/v_pig*"], FIRST, null],
    ["strider", "striders", ["physrec/v_strider*"], "1.16", "strider"],
    ["camel", "camels", ["physrec/v_camel*"], "1.20", "camel"],
  ].map(([id, title, scenarios, since, entity]) => ({
    id: `mount.${id}`, family: "mount", title: `ridden ${title}`, scenarios,
    inputs: "rider input and the mount's attributes", state: "position, velocity, jump charge, stuck speed multiplier",
    outputs: ENTITY_OUTPUT, authority: "the controlling client", requires: [WORLD, PASSENGER, ORDER],
    api: ["simulateVersionedHorseTick", "NativeRiddenMobPhysics"], since, ...(entity ? { entity } : {}),
  })),
  {
    id: "minecart.default", family: "minecart", title: "default minecarts", scenarios: ["physrec/mc_*"],
    inputs: "the cart, its rails and rider", state: "position, velocity, rail state", outputs: ENTITY_OUTPUT,
    authority: "the server; a client lerps it", requires: [WORLD, PASSENGER, ORDER],
    api: ["simulateVersionedMinecartScene", "NativeMinecartPhysics"], since: FIRST,
  },
  {
    id: "minecart.experimental", family: "minecart", title: "the minecart_improvements experiment", scenarios: ["physrec/mx_*"],
    inputs: "as default minecarts, on a world with the feature enabled", state: "position, velocity, rail state", outputs: ENTITY_OUTPUT,
    authority: "the server; a client lerps it", requires: [WORLD, PASSENGER, ORDER],
    api: ["simulateVersionedMinecartScene", "MinecartTracker"], since: "1.21.2",
    configuration: "the release alone does not select it: the world must enable minecart_improvements",
  },
  ...[
    ["arrow", "arrows", ["physrec/pj_arrow*", "physrec/pj_bow*", "physrec/pj_pierce*"], FIRST, null, "simulateVersionedArrowTicks"],
    ["trident", "tridents", ["physrec/pj_trident*"], "1.13.2", "trident", "simulateVersionedTridentTicks"],
    ["thrown", "thrown items", ["physrec/pj_snowball*", "physrec/pj_egg*", "physrec/pj_ender*", "physrec/pj_potion*", "physrec/pj_experience*"], FIRST, null, "simulateVersionedProjectileTicks"],
    ["fireball", "fireballs and other powered projectiles", ["physrec/pj_fireball*", "physrec/pj_small*", "physrec/pj_dragon*", "physrec/pj_wither*"], FIRST, null, "simulateVersionedProjectileTicks"],
    ["wind", "wind charges", ["physrec/pj_wind*"], "1.20.5", "wind_charge", "simulateVersionedProjectileTicks"],
  ].map(([id, title, scenarios, since, entity, api]) => ({
    id: `projectile.${id}`, family: "projectile", title, scenarios,
    inputs: "the projectile's state and the scene's targets", state: "position, velocity, in-ground state, life",
    outputs: `${ENTITY_OUTPUT}, hits`, authority: "the server", requires: [WORLD, TARGET, ORDER, RANDOM],
    api: [api, "NativeProjectilePhysics"], since, replay: ["--projectiles"], ...(entity ? { entity } : {}),
  })),
  ...[
    ["tnt", "primed TNT", ["physrec/me_tnt*"]],
    ["falling", "falling blocks", ["physrec/me_falling*"]],
    ["item", "items", ["physrec/me_item*"]],
    ["orb", "experience orbs", ["physrec/me_orb*"]],
  ].map(([id, title, scenarios]) => ({
    id: `environment.${id}`, family: "environment", title, scenarios,
    inputs: "the entity's kind and state", state: "position, velocity, flags", outputs: ENTITY_OUTPUT,
    authority: "the server", requires: [WORLD, ORDER],
    api: ["simulateVersionedMovingEntityTick", "NativeMovingEntityPhysics"], since: FIRST,
  })),
  {
    id: "environment.dragon", family: "environment", title: "the ender dragon's wing pushes", scenarios: ["physrec/me_dragon*"],
    inputs: "the dragon's parts and the entities in reach", state: "position, velocity, phase", outputs: ENTITY_OUTPUT,
    authority: "the server", requires: [WORLD, TARGET, ORDER, RANDOM],
    api: ["simulateVersionedDragonTick", "dragonWingPushesForVersion"], since: FIRST,
  },
];

const FAMILIES = ["player", "living", "boat", "mount", "minecart", "projectile", "environment"];
const STATUSES = ["unavailable", "implemented", "partial", "verified"];
const CONTEXTS = [WORLD, PASSENGER, TARGET, RANDOM, ORDER];

const sha256 = (text) => crypto.createHash("sha256").update(text).digest("hex");

/** What an interaction promises: any change to it takes the claims made under the old promise. */
const scopeHash = (interaction) => sha256(JSON.stringify(interaction));

/** The engine as source, its tests left out: a change of native behaviour takes every trace claim made of it, and a test's does not. */
function engineDigest(root = repositoryRoot) {
  const hash = crypto.createHash("sha256");
  const walk = (directory) => fs.readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => (left.name < right.name ? -1 : 1))
    .flatMap((entry) => (entry.isDirectory() ? walk(path.join(directory, entry.name)) : [path.join(directory, entry.name)]));
  for (const file of walk(path.join(root, "native", "src")).filter((name) => name.endsWith(".rs") && !/tests\.rs$/.test(name))) {
    hash.update(path.relative(root, file).replaceAll("\\", "/")).update("\0").update(codeLines(fs.readFileSync(file, "utf8")).join("\n")).update("\0");
  }
  return hash.digest("hex");
}

/** A pin's seal: a hand edit of any of its facts has to be made again through the pinning script. */
const sealOf = (pin) => {
  const { seal, ...facts } = pin;
  return sha256(JSON.stringify(facts));
};

const available = (interaction, version) => compareReleases(version, interaction.since) >= 0;

/** Why a pinned trace no longer speaks for a cell: empty when it still does. */
function pinProblems(pin, interaction, identity) {
  const problems = [];
  if (pin.seal !== sealOf(pin)) problems.push("its seal does not match its facts");
  if (pin.scope !== scopeHash(interaction)) problems.push("the contract of the interaction changed");
  if (pin.source !== identity.source) problems.push("the release's source record changed");
  if (pin.tables !== identity.tables) problems.push("the release's tables changed");
  if (pin.engine !== identity.engine) problems.push("the engine changed");
  return problems;
}

/** A pin is a complete pass when it replayed the whole of the cell and every recording matched. */
const completePass = (pin) => pin.selected > 0 && pin.selected === pin.corpus && pin.matching === pin.selected && pin.exit === 0;

/** The status of one release of one interaction, with its reasons. */
function cell(version, interaction, pins, identity) {
  if (!available(interaction, version)) return { status: "unavailable", reasons: [`not in the game before ${interaction.since}`] };
  const gaps = (interaction.gaps ?? []).filter((gap) => !gap.from || compareReleases(version, gap.from) >= 0);
  const reasons = gaps.map((gap) => `gap (${gap.ticket}): ${gap.reason}`);
  const pin = pins?.[version]?.[interaction.id];
  if (!pin) return { status: gaps.length > 0 ? "partial" : "implemented", reasons: [...reasons, "no pinned trace evidence"] };
  const stale = pinProblems(pin, interaction, identity);
  if (stale.length > 0) return { status: gaps.length > 0 ? "partial" : "implemented", reasons: [...reasons, ...stale.map((problem) => `the pinned trace is stale: ${problem}`)] };
  if (completePass(pin) && gaps.length === 0) return { status: "verified", reasons: [] };
  return { status: "partial", reasons: [...reasons, `${pin.matching} of ${pin.corpus} scenarios match in the pinned trace (${pin.selected} replayed)`] };
}

/** Every cell of a release, by interaction. */
function contractCells(version, pins, identity, interactions = INTERACTIONS) {
  return Object.fromEntries(interactions.map((interaction) => [interaction.id, cell(version, interaction, pins, identity)]));
}

/** What is wrong with the contract itself: names repeated, cells without an owner, an unknown word. */
function contractProblems(interactions = INTERACTIONS, versions = canonicalReleases()) {
  const problems = [];
  const seen = new Set();
  for (const interaction of interactions) {
    if (seen.has(interaction.id)) problems.push(`${interaction.id} is declared twice`);
    seen.add(interaction.id);
    if (!FAMILIES.includes(interaction.family)) problems.push(`${interaction.id} is of no family`);
    if (!versions.includes(interaction.since)) problems.push(`${interaction.id} starts at ${interaction.since}, which is no registered release`);
    for (const field of ["title", "inputs", "state", "outputs", "authority"]) if (!interaction[field]) problems.push(`${interaction.id} has no ${field}`);
    if (!(interaction.scenarios?.length > 0)) problems.push(`${interaction.id} has no scenario inventory`);
    if (!(interaction.api?.length > 0)) problems.push(`${interaction.id} names no entry point`);
    for (const context of interaction.requires ?? []) if (!CONTEXTS.includes(context)) problems.push(`${interaction.id} requires ${context}`);
  }
  for (const family of FAMILIES) if (!interactions.some((interaction) => interaction.family === family)) problems.push(`no interaction of ${family}`);
  return problems;
}

module.exports = {
  CONTEXTS, FAMILIES, INTERACTIONS, STATUSES,
  available, cell, completePass, contractCells, contractProblems, engineDigest, pinProblems, scopeHash, sealOf,
};
