// Which fields of a state a tick hands back: the `kept` column of docs/field-maps.md, measured on the addon, not written down.
// A tick on 1.21.5 from the seed below; a field it does not hand back is an input of that tick or absent in its scene, so the
// caller owns it. scripts/field-catalogue.cjs renders what this measures, and test/field-catalogue.test.cjs fails when the page
// and the addon disagree.
const position = { x: 0.5, y: 100, z: 0.5 };
const velocity = { x: 0, y: 0, z: 0 };

/** The release the column is measured on. */
const RELEASE = "1.21.5";

/** Per state: the seed of its tick, and the tick that gives the state back. */
const TICKS = {
  NativeBoatState: { seed: { position, velocity, yaw: 0, forward: false, left: false, right: false, inWater: false }, tick: (binding, spec) => binding.simulateVersionedBoatTick(RELEASE, new binding.NativeWorld(), spec) },
  NativeHorseState: { seed: { position, velocity, yaw: 0, forward: 0, strafe: 0, jumpRequested: false, onGround: false, inWater: false }, tick: (binding, spec) => binding.simulateVersionedHorseTick(RELEASE, new binding.NativeWorld(), spec) },
  NativeMinecartState: { seed: { variant: "minecart", position, velocity, yRot: 0, flipped: false, onGround: false, inWater: false }, tick: (binding, spec) => binding.simulateVersionedMinecartTick(RELEASE, new binding.NativeWorld(), spec, { improvements: false }).state },
  NativeMovingEntityState: { seed: { kind: "item", position, velocity }, tick: (binding, spec) => binding.simulateVersionedMovingEntityTick(RELEASE, new binding.NativeWorld(), spec).state },
  NativeProjectileState: { seed: { kind: "snowball", position, velocity }, tick: (binding, spec) => binding.simulateVersionedProjectileTicks(RELEASE, new binding.NativeWorld(), [spec], 1)[0] },
  NativeArrowState: { seed: { position, velocity }, tick: (binding, spec) => binding.simulateVersionedArrowTicks(RELEASE, new binding.NativeWorld(), [spec], 1)[0] },
  NativeTridentState: { seed: { position, velocity }, tick: (binding, spec) => binding.simulateVersionedTridentTicks(RELEASE, new binding.NativeWorld(), [spec], 1)[0] },
};

/**
 * interface -> the fields of `members` (interface -> field names, as declared) that one real tick does not hand back, in
 * declaration order.
 */
function measureKept(binding, members) {
  const notKept = {};
  for (const [name, { seed, tick }] of Object.entries(TICKS)) {
    const output = tick(binding, seed);
    notKept[name] = members[name].filter((field) => output[field] === undefined || output[field] === null);
  }
  return notKept;
}

module.exports = { RELEASE, TICKS, measureKept };
