// The shared round-trip table of the non-player states (B005, docs/observable-outputs.md). Every field
// of each state is in exactly one class, and test/state-roundtrip.test.cjs fails on a field left out:
//   kept        comes back from a tick as it went in, false, true, 0 and 1 alike
//   evolved     comes back, recomputed by the tick: the value is the tick's, not the input's
//   config      read by the tick and not returned, so the caller keeps it between ticks
//   refused     only certain values are legal: a bad one is an error, never a silent default
//   structured  a vector, box, list or name, each with its own tests
// A field not in `evolved`, `config`, `refused` or `structured` is kept. `zeroMatters` holds the fields whose
// explicit zero must move the state differently from leaving them out. `evolvedUntil` holds the kept fields a tick still
// recomputes on the older releases, up to the last that does: measured on every release by test/field-boundaries.test.cjs.
// The player's states are in scripts/state-bridge.cjs. Classes are what the engine does, read by a tick
// in empty air: that a field is `kept` says it is not lost, not that its value is vanilla's.
const position = { x: 0.5, y: 100, z: 0.5 };
const still = { x: 0, y: 0, z: 0 };

const STATES = {
  NativeBoatState: {
    family: "boat",
    evolved: ["yaw","deltaRotation","inWater","waterLevel","collidedHorizontally","collidedVertically","onGround","lastVerticalVelocity","fallDistance","lerpSteps"],
    config: [],
    refused: [],
    structured: ["nearbyEntities","position","velocity","status","previousStatus","boundingBox","stuckSpeedMultiplier","paddlePhases","lerpPosition"],
    zeroMatters: [],
    evolvedUntil: { yawDegrees: "1.8.8", speedMultiplier: "1.8.8" },
    base: { position, velocity: still, yaw: 0, forward: false, left: false, right: false, inWater: false },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedBoatTick(version, world, state),
  },
  NativeHorseState: {
    family: "mount",
    evolved: ["yaw","onGround","inWater","collidedHorizontally","collidedVertically","yawDegrees","pitchDegrees","inLava","lavaHeightPositive","riddenSpeed","ownForward","allowStandSliding","standCounter"],
    config: ["riderJump","waterHeight"],
    refused: ["halfWidth","height"],
    structured: ["nearbyEntities","position","velocity","entityType","boundingBox","previousPosition","lerpPosition","stuckSpeedMultiplier"],
    zeroMatters: ["gravity"],
    base: { position, velocity: still, yaw: 0, forward: 0, strafe: 0, jumpRequested: false, onGround: false, inWater: false },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedHorseTick(version, world, state),
  },
  NativeMinecartState: {
    family: "minecart",
    evolved: ["xRot","onRails","onGround","firstTick","inWater","collidedHorizontally","collidedVertically","fallDistance","tickCount","hurtTime","damage","portalCooldown"],
    config: ["fuel","comparatorSignal","fuse","creativeIgnition","explosionPowerBase","explosionSpeedFactor","randomSeed","enabled","spawnDelay","minSpawnDelay","maxSpawnDelay","requiredPlayerRange"],
    refused: [],
    structured: ["variant","position","velocity","stuckSpeedMultiplier","boundingBox","rider","push","inventory","portal","portalEntry","removed","team"],
    zeroMatters: [],
    evolvedUntil: { insidePortal: "1.20.6", portalTime: "1.20.6" },
    base: { variant: "minecart", position, velocity: still, yRot: 0, flipped: false, onGround: false, inWater: false },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedMinecartTick(version, world, state, { improvements: false }),
  },
  NativeProjectileState: {
    family: "projectile",
    evolved: ["inWater","firstTick"],
    config: ["gravity","airDrag","accelerationPower","dangerous","ultraWarm","shotAtAngle","bobRandom","noPhysics"],
    refused: [],
    structured: ["position","velocity","power","kind","boundingBox","stuckSpeedMultiplier"],
    zeroMatters: ["gravity"],
    base: { kind: "snowball", position, velocity: still },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedProjectileTicks(version, world, [state], 1)[0],
  },
  NativeArrowState: {
    family: "projectile",
    evolved: ["inWater","inGround","inGroundTime","shakeTime"],
    config: ["gravity","airDrag","waterInertia","noPhysics","ultraWarm"],
    refused: [],
    structured: ["position","velocity","fallImpulse","lastBlockPosition"],
    zeroMatters: ["gravity"],
    base: { position, velocity: still },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedArrowTicks(version, world, [state], 1)[0],
  },
  NativeTridentState: {
    family: "projectile",
    evolved: ["inWater","inGround","inGroundTime","shakeTime"],
    config: ["gravity","airDrag","waterInertia","noPhysics","ultraWarm"],
    refused: [],
    structured: ["position","velocity","fallImpulse","lastBlockPosition"],
    zeroMatters: ["gravity"],
    base: { position, velocity: still },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedTridentTicks(version, world, [state], 1)[0],
  },
  NativeMovingEntityState: {
    family: "environment",
    evolved: ["tickCount","collidedHorizontally","collidedVertically","verticalCollisionBelow","inWater","inLava","noPhysics","firstTick","age","pickupDelay"],
    config: [],
    refused: [],
    structured: ["kind","authority","position","velocity","boundingBox","stuckSpeedMultiplier","lastMovements"],
    zeroMatters: [],
    evolvedUntil: { onGround: "1.14.4", fallDistance: "1.21.1" },
    base: { kind: "item", position, velocity: still },
    tick: (native, world, state, version = "26.3") => native.simulateVersionedMovingEntityTick(version, world, state),
  },
};

module.exports = { STATES };
