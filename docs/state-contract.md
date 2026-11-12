# Player state across the public entry points

Three surfaces carry a player's state: the object API (`simulateVersionedTick`, `simulateVersionedTickOutcome`,
`simulateVersionedTicks`), the packed `NativePlayer`, and the Mineflayer adapter on top of it.
`test/state-contract.test.cjs` proves what this page says through the public entry points, and fails if the matrix
below loses or gains a field.

## Owners

- **caller, every tick**: the packed input array sends it each tick; the object API takes it in the state.
- **caller, kept**: rare inputs (attributes, effects, enchantments). `setState` replaces them; they stay between ticks.
- **engine, seeded**: the engine's own between ticks. A caller seeds it and reads it back, and must hand back what it
  read, not a guess.
- **one tick**: an event of the tick that just ran. The next tick clears it, and reading it replays nothing.
- **caller, the tick's scene**: what the world around the player brought to this tick.

## Omission

`absent` is not `0` or `false`. Options keep the difference through every path: `gravity: 0` is no gravity,
absent is the release's own. The packed path has no absent: it sends a number for each field, so a state seeded
through it holds `sprintModifier: false` where an object state may hold nothing, and these two fold the movement speed
differently. To compare the paths, seed the object state with the explicit values.

## What each read returns

- `simulateVersionedTick`: the motion and flags. It leaves out the scene the next tick reads.
- `simulateVersionedTickOutcome`: the state to pass back as the next tick's, with what the tick wrote.
- `NativePlayer.tick`: the output array, the narrow path. `NativePlayer.state()` holds every other field, and a read
  changes nothing. Cooldowns such as `tickCount`, the fire and freeze counters and `supportingBlock` are read there.

## Clone, reseed and teleport

- A clone is `new NativePlayer(version, player.state(), config)`. It keeps every engine field and goes on alone.
  The adapter's `cloneState` does this and leaves a pending one-tick event with the state it came to.
- `setState` is a reseed: it lets go of what the last tick reported, an unfinished `tickMove`, and the world.
- A teleport is a position set without the `UNMOVED` flag. It drops `previousPosition`, so the support check of the
  next tick starts over. Velocity, rotation and the counters stay. `docs/portals-and-teleports.md` has each kind.
- A state ticked in one `NativeWorld` is refused by another until `setState` moves it: support and contact mean
  nothing across worlds.

## Examples

These run in `test/doc-examples.test.cjs`, as the matrix below does in the tests named above.

Omitted is not zero. `gravity: 0` is no gravity, and leaving it out is the release's own:

```js doctest
const world = new binding.NativeWorld();
const falling = (extra) => ({
    position: { x: 0.5, y: 64, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 },
    halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false, ...extra,
});
const omitted = binding.simulateVersionedTick('1.21.11', world, falling({}), {});
const zero = binding.simulateVersionedTick('1.21.11', world, falling({ gravity: 0 }), {});
assert.ok(omitted.velocity.y < 0);
assert.equal(zero.velocity.y, 0);
```

A kept value goes on only if the caller hands it back. `simulateVersionedTickOutcome` gives the state to pass on, so its
`tickCount` counts. A state rebuilt by hand starts it again:

```js doctest
let carried = falling({});
for (let tick = 0; tick < 3; tick += 1) carried = binding.simulateVersionedTickOutcome('1.21.11', world, carried, {}).state;
assert.equal(carried.tickCount, 3);

const rebuilt = binding.simulateVersionedTickOutcome('1.21.11', world, falling({ position: carried.position, velocity: carried.velocity }), {}).state;
assert.equal(rebuilt.tickCount, 1);
```

## Base Entity properties outside this contract

The player contract covers the local player and a server's copy of it. Remote entities are extrapolated from packets,
which the engine does not model; air supply, health, absorption, inventory, passengers beyond the seat, custom names and
glow flags have no movement consumer and no field. Local-player parity says nothing of general Entity parity.

## Phases

`node replay/cli.ts --phases` in the recorder package compares `simulateVersionedTickTraced` with vanilla's snapshots at each point of
the tick both have. A block's effects and the entity pushes have no phase of their own: they run between `afterTravel` and
`afterAiStep`, which bracket them.

## Authority

`clientSide`, `controlledByLocalInstance` and `canSimulateMovement` are three separate answers, as they are in vanilla.
`isPlayer` alone cannot say LocalPlayer from ServerPlayer: `clientSide: false` on a player is the server's copy. See
`test/fall-damage-sides.test.cjs`, `native/src/oracle_move_tests.rs`'s authority table, and `docs/piston-authority.md`.

## Counters

Every counter is a Java int in vanilla, and starts at 0 unless a row says otherwise. `SimulationState`'s own are documented
above its struct in `native/src/simulation.rs`; these are the other families'. A batch advances each exactly once a tick:
`test/state-transitions.test.cjs`.

| field | absent means | owner | changes at | packed in | packed out | note |
|---|---|---|---|---|---|---|
| `position` | required | caller, every tick | the move | yes | yes |  |
| `velocity` | required | caller, every tick | base tick, aiStep, the move | yes | yes |  |
| `yaw` | 0 | caller, every tick | caller | yes | no | radians, the float angle widened |
| `yawDegrees` | absent | caller, kept | caller | no | no | float degrees |
| `pitch` | 0 | caller, every tick | caller | yes | no |  |
| `halfWidth` | required | caller, every tick | caller | yes | no |  |
| `height` | required | caller, every tick | caller | yes | no |  |
| `onGround` | required | engine, seeded | the move | yes | yes |  |
| `collidedHorizontally` | required | engine, seeded | the move | yes | yes |  |
| `minorHorizontalCollision` | false | engine, seeded | the move | yes | yes |  |
| `collidedVertically` | required | engine, seeded | the move | yes | yes |  |
| `verticalCollisionBelow` | false | engine, seeded | the move | yes | yes |  |
| `blockFallCallback` | false | one tick | cleared at tick start, set by the move | no | yes |  |
| `entityBounciness` | 0 | caller, kept | caller | no | no |  |
| `blockBounciness` | 0 | caller, kept | caller | no | no |  |
| `input` | (0, 0, 0) | caller, every tick | caller, cleared on an immobile entity | yes | no |  |
| `jumpRequested` | false | caller, every tick | caller, cleared on an immobile entity | yes | no |  |
| `jumpTicks` | 0 | engine, seeded | top of aiStep | yes | yes |  |
| `autoJump` | false | caller, kept | caller | yes | no |  |
| `autoJumpTime` | 0 | engine, seeded | aiStep | yes | yes |  |
| `riptideLaunch` | absent | caller, every tick | taken at tick start | yes | no |  |
| `pendingMovements` | absent | engine, seeded | the shulker boxes' pushes, in order, taken by the next tick's block effects | no | yes | 1.21.5+ |
| `sprinting` | false | caller, every tick | caller, ended by a stab or a wall | yes | no |  |
| `wasSprinting` | false | caller, every tick | caller | yes | no |  |
| `sprintModifier` | absent | caller, kept | caller | yes | no | absent folds the speed itself: not the same as false |
| `inWater` | false | engine, seeded | base tick | yes | yes |  |
| `inLava` | false | engine, seeded | base tick | yes | yes |  |
| `ultraWarm` | false | caller, kept | caller | no | no |  |
| `isPlayer` | false | caller, kept | caller | no | no |  |
| `controlledByLocalInstance` | false | caller, kept | caller | no | no |  |
| `canSimulateMovement` | absent | caller, kept | caller | no | no | absent derives from side and local control |
| `nonLiving` | false | caller, kept | caller | no | no |  |
| `passenger` | false | caller, kept | caller | no | no |  |
| `isVehicle` | false | caller, kept | caller | no | no | carries passengers: from 1.9 not pushed |
| `boatVehicle` | absent | caller, the tick's scene | caller | no | no |  |
| `inWeb` | false | engine, seeded | block effects | no | yes |  |
| `weaving` | false | caller, kept | caller | no | no |  |
| `frostWalker` | false | caller, kept | caller | no | no |  |
| `sweetBerryBushImmune` | false | caller, kept | caller | no | no |  |
| `powderSnowImmune` | false | caller, kept | caller | no | no |  |
| `powderSnowWalkable` | false | caller, kept | caller | no | no |  |
| `inPowderSnow` | false | engine, seeded | block effects | yes | yes |  |
| `fireImmune` | false | caller, kept | caller | no | no |  |
| `fireResistance` | false | caller, kept | caller | no | no |  |
| `onFireFlag` | false | caller, kept | caller | no | no |  |
| `stabHit` | absent | one tick | taken at tick start | no | no | taken once, on a server state |
| `inRain` | false | caller, kept | caller | no | no |  |
| `fireProtection` | 0 | caller, kept | caller | no | no |  |
| `burningTime` | absent | caller, kept | caller | no | no |  |
| `remainingFireTicks` | absent | engine, seeded | base tick, block effects | no | no | absent is the field's initial value |
| `serverFireIncrement` | absent | caller, kept | caller | no | no | a fixed draw for every ignition, an approximation |
| `serverFireIncrements` | absent | engine, seeded | block effects | no | no | recorded draws, one per ignition; see B058 |
| `ticksFrozen` | 0 | engine, seeded | aiStep, block effects | no | no |  |
| `ticksRequiredToFreeze` | 140 | caller, kept | caller | no | no |  |
| `tickCount` | 0 | engine, seeded | tick start | no | no | a Java int: wraps |
| `freezeDamage` | 0 | one tick | aiStep | no | yes | a running count: the amount of a tick is its rise |
| `contactDamage` | 0 | one tick | cleared at tick start, block effects | no | yes |  |
| `wallDamage` | 0 | one tick | cleared at tick start, travel | no | yes |  |
| `insideWall` | false | engine, seeded | base tick | no | no |  |
| `sleeping` | false | caller, kept | caller | yes | no |  |
| `eyeHeight` | absent | caller, every tick | caller | yes | no |  |
| `eyeInWater` | false | engine, seeded | base tick | no | yes |  |
| `eyeInLava` | false | engine, seeded | base tick | no | no |  |
| `slowFalling` | false | caller, kept | caller | no | no |  |
| `levitation` | false | caller, kept | caller | no | no |  |
| `levitationAmplifier` | absent | caller, kept | caller | no | no |  |
| `fallDistance` | 0 | engine, seeded | base tick, the move | yes | yes | float before 1.21.5, double from it |
| `safeFallDistance` | 3 | caller, kept | caller | no | no |  |
| `fallDamageImmune` | false | caller, kept | caller | no | no |  |
| `clientSide` | false | caller, kept | caller | no | no | selects the client rules where vanilla differs |
| `heightAfterTick` | absent | caller, every tick | caller | no | no |  |
| `entityType` | absent | caller, kept | caller | no | no | validated against the release registry |
| `invulnerable` | false | caller, kept | caller | no | no |  |
| `fallDamageMultiplier` | 1 | caller, kept | caller | no | no |  |
| `sneaking` | false | caller, every tick | caller | yes | no |  |
| `movingSlowly` | absent | caller, every tick | caller | yes | no |  |
| `steppingCarefully` | false | caller, kept | caller | no | no |  |
| `usingItem` | false | caller, every tick | caller | yes | no |  |
| `itemUseSpeedMultiplier` | absent | caller, every tick | caller | yes | no |  |
| `swimming` | false | caller, every tick | caller | yes | no |  |
| `flying` | false | caller, every tick | caller | yes | no |  |
| `flyingSpeed` | 0.05000000074505806 | caller, every tick | caller | yes | no |  |
| `climbing` | false | caller, kept | caller | no | no |  |
| `noPhysics` | false | caller, kept | caller | yes | no |  |
| `noGravity` | false | caller, kept | caller | no | no |  |
| `removed` | false | caller, kept | caller | yes | no |  |
| `dead` | false | caller, kept | caller | yes | no |  |
| `gravity` | absent | caller, kept | caller | no | no | attribute, sanitized -1..1, NaN the minimum |
| `stepHeight` | absent | caller, kept | caller | no | no | attribute, sanitized 0..10, NaN the minimum |
| `movementSpeed` | absent | caller, kept | caller | no | no |  |
| `movementSpeedValue` | absent | caller, kept | caller | no | no | the attribute with every modifier; absent falls back to movementSpeed and the effects |
| `movementSpeedAttribute` | absent | caller, kept | caller | no | no |  |
| `jumpStrength` | absent | caller, kept | caller | no | no | attribute; 0 is legal and does not jump from 1.20.5 |
| `speedEffectAmplifier` | absent | caller, kept | caller | no | no |  |
| `slownessEffectAmplifier` | absent | caller, kept | caller | no | no |  |
| `jumpBoostAmplifier` | absent | caller, kept | caller | no | no |  |
| `dolphinsGrace` | 0 | caller, kept | caller | no | no |  |
| `depthStrider` | 0 | caller, kept | caller | no | no |  |
| `soulSpeed` | 0 | caller, kept | caller | no | no |  |
| `waterMovementEfficiency` | 0 | caller, kept | caller | no | no |  |
| `movementEfficiency` | 0 | caller, kept | caller | no | no |  |
| `airDragModifier` | absent | caller, kept | caller | no | no |  |
| `frictionModifier` | absent | caller, kept | caller | no | no |  |
| `sneakingSpeed` | absent | caller, kept | caller | no | no |  |
| `fallDamage` | 0 | one tick | cleared at tick start, set by the move | no | yes |  |
| `fallFlying` | false | engine, seeded | aiStep | yes | yes |  |
| `look` | (0, 0, 1) | caller, every tick | caller | yes | no |  |
| `fireworkBoost` | (0, 0, 0) | caller, the tick's scene | caller | no | no |  |
| `stuckSpeedMultiplier` | (0, 0, 0) | engine, seeded | block effects, read by the move | yes | yes |  |
| `previousPosition` | absent | engine, seeded | after the move | no | no | packed: kept with the UNMOVED flag, dropped without it |
| `boundingBox` | absent | engine, seeded | tick start, the move | no | yes | before 1.17 only; packed in through setBoundingBox |
| `supportingBlock` | absent | engine, seeded | the move | no | no |  |
| `supportingBlockKept` | false | engine, seeded | the move | no | no |  |
| `onGroundNoBlocks` | false | engine, seeded | the move | no | no |  |
| `outsideBorder` | false | engine, seeded | the move | no | no |  |
| `nearbyEntities` | absent | caller, the tick's scene | caller | no | no |  |
| `fireworkRockets` | absent | caller, the tick's scene | caller | yes | no |  |
| `fireworkRocketsBeforePlayer` | absent | caller, the tick's scene | caller | yes | no |  |
| `fireworkRocketTicks` | absent | caller, the tick's scene | caller | no | no |  |
| `movingPistons` | absent | caller, the tick's scene | caller | no | no |  |
