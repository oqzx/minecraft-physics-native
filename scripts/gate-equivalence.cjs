// That a removed gate changed nothing, on all 50 releases: one fixed battery run on the addon with the gate and the addon
// without, compared result by result.
//   NAPI_RS_NATIVE_LIBRARY_PATH=<addon> node scripts/gate-equivalence.cjs dump <file>
//   node scripts/gate-equivalence.cjs compare <before> <after>
// The battery reaches what the gates removed at B214 stood in: the capabilities, the knockback at lengths around both
// thresholds, and a player, a boat and the moving entities walking at the world border and into walls. `compare` also says
// on how many adjacent pairs the battery itself differs and how many runs were refused: a battery blind to the releases, or
// one that only collects refusals, would prove nothing.
const crypto = require("node:crypto");
const fs = require("node:fs");
const { canonicalReleases } = require("./release-inventory.cjs");

function worldOf(native) {
  const world = new native.NativeWorld();
  world.setWorldBounds(-64, 320, 0, 0, 12);
  for (let chunkX = -1; chunkX <= 0; chunkX += 1) for (let chunkZ = -1; chunkZ <= 0; chunkZ += 1) world.setChunkLoaded(chunkX, chunkZ, true);
  const blocks = [];
  for (let x = -7; x <= 7; x += 1) {
    for (let z = -7; z <= 7; z += 1) {
      blocks.push({ x, y: 9, z, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 } });
    }
  }
  blocks.push({ x: 2, y: 10, z: 0, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 0.5, maxZ: 1, slipperiness: 0.6, effect: 0 } });
  world.applyBlockUpdates(blocks);
  return world;
}

const STARTS = [[0.5, 10, 0.5], [5.2, 10, 0.5], [5.71, 10, 0.5], [6.5, 10, 0.5], [0.5, 10, 5.71], [1.2, 10, 0.5]];
const MOTIONS = [[0, 0, 0], [0.3, 0, 0], [0.5e-7, 0, 0], [0, 0, 0.3], [0.2, -0.1, 0.2], [0.25, 0.2, 0]];
const vec = ([x, y, z]) => ({ x, y, z });

function battery(native, release) {
  const out = { capabilities: native.versionCapabilities(...`${release}.0.0`.split(".").slice(0, 3).map(Number)) };
  const lengths = [0, 1e-6, 1e-5, 2e-3, 3.1e-3, 3.2e-3, 1e-4, 9.9e-5, 0.1, 0.7];
  out.knockback = [];
  for (const x of lengths) {
    for (const z of [0, x, -x / 2]) {
      for (const [strength, resistance] of [[0.4, 0], [0.4, 0.25], [0.9, 1]]) {
        out.knockback.push(native.livingKnockback(release, { x: 0.1, y: -0.2, z: 0.05 }, true, strength, x, z, resistance, 0.5) ?? null);
      }
    }
  }
  const world = worldOf(native);
  let refused = 0;
  let runs = 0;
  const run = (name, step) => {
    out[name] = [];
    for (const start of STARTS) {
      for (const motion of MOTIONS) {
        runs += 1;
        try {
          out[name].push(step(start, motion));
        } catch (error) {
          refused += 1;
          out[name].push(`refused: ${String(error.message).slice(0, 80)}`);
        }
      }
    }
  };
  run("player", (start, motion) => {
    let state = {
      position: vec(start), velocity: vec(motion), halfWidth: Math.fround(0.3), height: Math.fround(1.8), onGround: false,
      collidedHorizontally: false, collidedVertically: false, isPlayer: true, clientSide: true, controlledByLocalInstance: true,
    };
    for (let tick = 0; tick < 5; tick += 1) state = native.simulateVersionedTick(release, world, state, {});
    return state;
  });
  run("boat", (start, motion) => {
    let state = { position: vec(start), velocity: vec(motion), yaw: 0, forward: true, left: false, right: false, inWater: false };
    for (let tick = 0; tick < 5; tick += 1) state = native.simulateVersionedBoatTick(release, world, state);
    return state;
  });
  for (const kind of ["tnt", "falling_block", "item", "experience_orb"]) {
    run(kind, (start, motion) => {
      let state = { kind, position: vec(start), velocity: vec(motion) };
      for (let tick = 0; tick < 5; tick += 1) state = native.simulateVersionedMovingEntityTick(release, world, state, { random: new Array(64).fill(0.5) }).state ?? state;
      return state;
    });
  }
  return { out, refused, runs };
}

const command = process.argv[2];
if (command === "dump") {
  const { LoadNativePhysics } = require("..");
  const native = LoadNativePhysics();
  const digest = (value) => crypto.createHash("sha256").update(JSON.stringify(value)).digest("hex");
  const digests = {};
  const sections = {};
  const refusals = {};
  let runs = 0;
  for (const release of canonicalReleases()) {
    const result = battery(native, release);
    digests[release] = digest(result.out);
    sections[release] = Object.fromEntries(Object.entries(result.out).map(([name, value]) => [name, digest(value).slice(0, 16)]));
    refusals[release] = result.refused;
    ({ runs } = result);
  }
  fs.writeFileSync(process.argv[3], `${JSON.stringify({ runs, digests, sections, refusals }, null, 1)}\n`);
  console.log(`${Object.keys(digests).length} releases, ${runs} world runs each, ${Object.values(refusals).reduce((a, b) => a + b, 0)} refused`);
} else if (command === "compare") {
  const [left, right] = process.argv.slice(3, 5).map((file) => JSON.parse(fs.readFileSync(file, "utf8")));
  const releases = Object.keys(left.digests);
  const differing = releases.filter((release) => left.digests[release] !== right.digests[release]);
  const adjacent = releases.slice(1).filter((release, index) => left.digests[release] !== left.digests[releases[index]]).length;
  const refused = Object.values(left.refusals).reduce((a, b) => a + b, 0);
  console.log(`${releases.length} releases, ${differing.length} differ${differing.length ? `: ${differing.join(" ")}` : ""}; the battery tells ${adjacent} of ${releases.length - 1} adjacent pairs apart; ${refused} of ${left.runs * releases.length} world runs refused`);
  for (const release of differing) {
    const names = Object.keys(left.sections[release]).filter((name) => left.sections[release][name] !== right.sections[release][name]);
    console.log(`  ${release}: ${names.join(", ")}`);
  }
  process.exit(differing.length === 0 ? 0 : 1);
} else {
  console.error("usage: gate-equivalence.cjs dump <file> | compare <before> <after>");
  process.exit(1);
}
