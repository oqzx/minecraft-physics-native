# Vanilla random consumers near movement

The audit behind `docs/determinism.md`. `scripts/audit-random-draws.cjs` walks the source of all 50 registered releases and
writes `test/fixtures/random-draws.tsv`. `test/fixtures/random-draw-dispositions.json` says what each consumer reaches, and
`test/random-draws.test.cjs` fails on a consumer nothing says anything about, in any release.

## What was read

| releases | source |
|---|---|
| 42, 1.14.4 to 26.1.2 | the source cache, `<release>/client`, Mojang-named. Local names are lost in many, so a generator held in a `var` is followed |
| 26.2, 26.3 | `E:\pr\mc-source` |
| 1.8.8, 1.9.4, 1.10.2, 1.11.2, 1.12.2, 1.13.2 | each client by its pinned sha1, remapped with `legacy-mappings/<release>.tiny` and decompiled with Vineflower 1.12.0 |

No release is left out, and no release stands in for another: 1.8.8 is its own jar, and 1.7, 1.8.9 and 1.14 are not read.
Every tree has one set of names, so nothing is a rename, though before 1.14 a member that 1.14.4 lacks keeps a Yarn or
intermediary name.

Three searches run on each tree:

- **Consumers.** Each method of `Entity`, `LivingEntity`, `Mob`, `Player`, `ServerPlayer`, `LocalPlayer`, the ridden mobs, the
  vehicles, projectiles and item entities, and each method of a block that is handed an entity, with the helpers of its own
  class. That last clause is what finds `BaseFireBlock.fireIgnite`, which an `InsideBlockEffectType` calls from 1.21.5.
- **Draws.** A call on a generator, however held, `Mth`'s and `Util`'s random helpers, `Math.random`, and a generator handed to
  anything else, which is named as the callee.
- **Reach.** From a `LocalPlayer`'s `tick`, `rideTick`, `aiStep`, `serverAiStep`, `travel`, `move` and `baseTick`, every call it
  makes on itself, up its superclasses and through `super`, with the path to each draw.

Reach does not follow a call on another object (`level.`, `vehicle.`, an item stack, a mob effect). A block's callback is the
one such call that can touch movement, and those are covered by the consumer search. A draw inside another object's own method
is not walked. A statement mentioning a particle or a sound counts as cosmetic; a draw kept in a local first does not, so it
comes out as `state` and the dispositions say what it does.

## What reaches movement

| consumer | side | draw | reaches | in the engine |
|---|---|---|---|---|
| `BaseFireBlock.entityInside` (1.21.2 to 1.21.4), `fireIgnite` (1.21.5 on), a ServerPlayer | server | `nextInt(1, 3)` | fire ticks | `serverFireIncrements`, ordered |
| `ItemEntity.tick` in lava (to 1.15.2), `ExperienceOrb.tick` in lava (all) | both | `(nextFloat() - nextFloat()) * 0.2F` | motion | `MovingEntityInputs.random`, ordered |
| `LivingEntity.knockback` with no direction, `LivingEntity.java:1594-1595` | server | `nextDouble` pairs, until not tiny | motion | excluded: knockback is the server's and reaches a client as motion. No entry point takes it |
| `Entity.moveTowardsClosestSpace`, `checkInBlock` before 1.16.2, items and orbs | both | `nextFloat() * 0.2F + 0.1F` | motion | `MovingEntityInputs.random`; a player uses LocalPlayer's own push, which has no draw |
| `FishingHook.tick` bobbing | both | `nextFloat` | the hook's pull | `bob_random`, the caller's |
| `AbstractArrow.tick` and `ThrowableProjectile.tick` to 1.15.2, `AbstractHurtingProjectile.tick` to 1.10.2, then `AbstractArrow.startFalling` | server | `nextFloat() * 0.2F` | motion off a block | the ground release, the caller's |
| `Projectile.deflect` | server | `nextFloat` | a deflected motion | `deflect_random`, the caller's |
| `Pig.boost`, `Strider.boost` | server | `nextInt(841) + 140` | a boost's length | the synced boost time |
| `PowderSnowBlock.entityInside`, `random.nextBoolean()` | client | particles only | nothing moved | excluded: cosmetic |
| `FarmlandBlock.fallOn` | server | `nextFloat() < fallDistance - 0.5` | the block turns to dirt | excluded: changes the world, which the caller owns |
| `FallingBlockEntity.causeFallDamage` | server | `nextFloat` | an anvil's damage to itself | excluded: no motion |
| `Projectile.getMovementToShoot`, `FishingHook`, `FireworkRocketEntity`, `ThrownExperienceBottle`, `EyeOfEnder.signalTo` | server | gaussian or triangle | the first motion | the caller hands the motion in |
| `ItemEntity`, `PrimedTnt` construction | server | `nextDouble`, `nextFloat` | the first motion and yaw | the caller hands the motion in |
| `LivingEntity` construction, `LivingEntity.java:283` | both | `nextFloat() * 2 * PI` | body yaw | no movement field |
| `LivingEntity.java:568`, oxygen bonus | server | `nextDouble` | air supply | no field |

## A LocalPlayer's own tick

The walk finds draws in that tick in every release, and none moves the player. `test/random-draw-dispositions.json`'s `reach`
list holds each one, and the test fails on any it does not name.

| draw | what it does |
|---|---|
| `LivingEntity.baseTick`, `tickDeath`, `tickEffects`, `spawnItemParticles`, `spawnItemUseParticles`, `spawnSoulSpeedParticle`, `getVoicePitch`; `Entity.getRandomX/Y/Z`, `doWaterSplashEffect`, `spawnSprintParticle`; `Player.drink`, `playShoulderEntityAmbientSound` | particles and sounds, the draw held in a local |
| `LivingEntity.tryAddSoulSpeed`, 1.16 to 1.20.6, `nextFloat() < 0.04F` | the boots' durability. The speed modifier is added before and without it (1.20.4 `LivingEntity.java:505-510`) |
| `Player.aiStep`, 1.17 on, `Util.getRandom` | which touched experience orb is picked up. `playerTouch` gives experience and moves nothing |
| `LivingEntity.updateFallFlying`, 1.21.2 on | which glider loses durability, inside `!isClientSide` |
| `LivingEntity.pushEntities`, 1.11.2 on | cramming damage, inside the `ServerLevel` branch. `doPush` draws nothing |
| `LivingEntity.knockback` | reached only by the name-based walk through `hurt`/`hurtServer`, which return at once or need a `ServerLevel`. A client gets knockback as motion |

So a client state needs no draw, for its own tick. An earlier version of this page said no draw is in that tick at all, which
holds only for draws that move something.

## Across releases

Each row is a change to something that reaches motion or a represented value, from the fixture. The full per-release list is
`test/fixtures/random-draws.tsv`.

| from | came or went | how it arrives |
|---|---|---|
| 1.8.8 | fire has no draw. Items and orbs in lava, `checkInBlock`, projectile ground release, every first motion | as the table above |
| 1.10.2 is the last | `AbstractHurtingProjectile.tick` | its ground release |
| 1.15.2 is the last | `ItemEntity.tick`, `AbstractArrow.tick`, `ThrowableProjectile.tick` in their old shape; the arrow's becomes `startFalling` at 1.16 | `MovingEntityInputs.random`, the ground release |
| 1.16.2 | `Entity.checkInBlock` is now `moveTowardsClosestSpace` | same draw, `MovingEntityInputs.random` |
| 1.20.3 | `AbstractArrow.deflect`, `Projectile.deflect` from 1.20.5, `ProjectileDeflection` from 26.1 | `deflect_random` |
| 1.21.2 | `BaseFireBlock.entityInside` gains a ServerPlayer's `nextInt(1, 3)`: 1.21.1's own tree has none, 1.21.2's has it | `serverFireIncrements` |
| 1.21.5 | the draw moves to `BaseFireBlock.fireIgnite`, which an `InsideBlockEffectType` calls | the same stream |
| 26.2 | `PrimedTnt.getRandomShortFuse` | the caller's fuse |
