# Limitations

What the engine does not do, does differently from vanilla, or has not shown. A release or a method being available implies
none of it: `docs/release-status.md` says what each release has shown. Read this page by your release, your entry point and
your scenario.

Dated 2026-11-06, commit `0eaaa0d`. The final pass is still running, so nothing here counts its result.

## The four kinds

| Kind | Meaning | Reads in a replay as |
| --- | --- | --- |
| not implemented | The engine has no path for it, on purpose or not yet | `unsupported`, or refused by name |
| differs | The code departs from vanilla. The source says so, or a test does | `known` if listed, otherwise `unexpected` |
| untested | Code exists, with no current trace against vanilla | `untested`, or no pin |
| no source evidence | The release's source cannot be cited, so a match has nothing to be held to | `blocked` in `src/release-status.json` |

## Is my case verified?

1. Find your release in `docs/release-status.md`. No release is admitted, so the answer for parity is no.
2. Run `node scripts/release-admission.cjs <release>` to see which cells have a current pin. At this commit none has.
3. Find your entry point below. `docs/parity-contract.md` says what each interaction covers.
4. Find your scenario in `docs/capability-table.md`, and the recordings for it in `docs/recorder-coverage.md`.
5. Pass `requireParity: true` to have an engine refuse any release that is not admitted.

## Every release

| Limitation | Kind | Consequence | Evidence |
| --- | --- | --- | --- |
| No release is admitted | untested | `requireParity: true` refuses all 50 | `src/release-status.json`, B288 |
| Every pin is stale | untested | 23 pins on record, none on the current engine | `node scripts/release-admission.cjs 26.3`, `generated/conformance-evidence.json` |
| The corpus is current, the pins are not | untested | Every compiled scenario of every release is recorded `ok` in the corpus that the recorder's `corpus/` pins, and two captures of it agree (B219, B235); no pin is made from it yet | the recorder's `corpus/26.3.json`, `docs/release-status.md` |
| A pin does not show a branch was reached | untested | Three of nine moved gates had no check until a fixture was written | `docs/validation-records.md` |
| A shared function with no `at_least` gate, or a gate spelled as a profile rule, has no source comparison | untested | All 323 gated functions are `source-compared`; the others claim nothing (B280) | `generated/shared-gates.json`, `docs/shared-code-audit.md` |
| Random impulses other than the fire draws are not inputs | not implemented | Knockback spread, fishing and potion scatter are not predicted | `docs/boundary-inputs.md`, B058 |
| AI, pathfinding, targets and mob jumps | not implemented | The caller gives their result as input | `docs/mob-travel.md` |
| Hunger and exhaustion | not implemented | A tick never changes them | `docs/determinism.md` |
| A `ServerPlayer`'s move validation | not implemented | The engine does not model it | `docs/authority.md` |
| Remote entities, air supply, health, inventory, general `Entity` state | not implemented | Local-player parity says nothing of them | `docs/state-contract.md` |
| An infinite motion before 1.21.11, or a finite one past `MAX_WALKED_CELLS` | differs | Refused as "the tick's move", where vanilla walks it | `docs/numeric-contract.md`, `test/numeric-limits.test.cjs` |
| A motion past `MAX_SYNCED_CELLS` in the adapter | differs | The tick is skipped and `state.moveRefused` is set | `docs/numeric-contract.md`, B313 |
| `Math.sin`, `Math.cos` on x86_64 HotSpot | exact | Every finite float and 22 million other angles, on Java 21.0.11 and 25.0.3 | `docs/numeric-contract.md` |
| `Math.sin`, `Math.cos` on arm64 HotSpot | differs | Other code there: about 1 angle in 6 by tickwright (Oracle JDK 24, Apple silicon), not measured here. The engine gives x86_64's | `docs/numeric-contract.md` |
| `Math.sin`, `Math.cos` on Java 8 within pi/4 (x87 `fsin`, `fcos`) | differs | Releases up to 1.16.5 follow Java 8 (`trig::java8`). Past pi/4 it is exact; within it the engine rounds the true value to 64 bits and then to a double, and the processor's own `fsin` decides where that lands within a 64-bit unit of a midpoint: 9 of 1,593,644 measured call-site values (the 1.8.8 boat, the cart push to 1.13.2, `COS_TAB`) on an AMD EPYC under Linux. Windows (53-bit x87 precision) and Intel were not measured. B315 | `docs/numeric-contract.md` |
| Java 8's mode has no recording | untested | The recorder runs every release on Java 21 or 25, so its replays set Java 21 (`setMathJava`); the Java 8 mode rests on digests and per-value measurements of Temurin 8, not on a recording. B315 | `docs/numeric-contract.md` |
| `Math` of Java 16 (1.17, 1.17.1) | untested | Not run; inferred from 11 and 17, which give 21's sin and cos digests. 1.17 has no direct call outside the `Mth` tables. B315 | `docs/numeric-contract.md` |
| Equal results across targets | untested | macOS, ARM and a browser did not run the corpus. It says nothing of equality with the JVM | `docs/determinism.md` |

## By release

| Releases | Limitation | Kind | Evidence |
| --- | --- | --- | --- |
| 1.8.x | `EmptyChunk` keeps what is set in it. That is not modelled | differs | `native/src/world.rs` `set_block` |
| 1.13.2 | `me_mob_phantom_circles` is intermittently not deterministic | untested | B314 |
| 1.13.x | The empty chunk gives back the fluid set in it. That is not modelled | differs | `native/src/world.rs` `set_block` |
| 26.2, 26.3 | The published `minecraft-data` has no table. The adapter needs a build that does | not implemented | `README.md` |
| 1.19.3, 1.19.4 | The camel is an experiment there (FeatureFlags.UPDATE_1_20): its scenarios are not compiled | untested | the recorder's `scenarios/dsl/compile.ts` |
| 1.20.3 to 1.20.6 | The breeze and its wind charge are an experiment there (FeatureFlags.UPDATE_1_21): their scenarios are not compiled | untested | the recorder's `scenarios/dsl/compile.ts` |

## By entry point

| Entry point | Limitation | Kind | Evidence |
| --- | --- | --- | --- |
| `simulateTick`, `simulateTicks` | Refused: a concrete version is required | not implemented | `native/src/napi_api.rs` |
| `simulateBoatTick`, `simulateHorseTick`, `simulateProjectileTick`, `simulateArrowTick`, `simulateTridentTick` | Run at the default release, with no version argument | differs | `docs/engine-architecture.md` |
| `simulateVersionedTick` | Leaves out the scene the next tick reads. Use `simulateVersionedTickOutcome` to carry a state | not implemented | `docs/state-contract.md` |
| `simulateVersionedProjectileTicks`, 1.14.4 - 1.15.2 | A rocket with explosions that collides ends on the client as well (1.14.4 FireworkRocketEntity.java:176-197, no side test). The plain flight holds no hit state, so it flies on until the server's removal | differs | `native/src/projectile.rs` `tick_firework` |
| `simulateVersionedMobTravel` | A `mob` kind outside the table of `docs/mob-travel.md` travels as `living`, which holds because no such class overrides `travel`; the family says nothing of a match, and only the `me_mob*` kinds are recorded. The dragon and the shulker are refused | untested | `docs/mob-travel.md`, `native/src/entity_travel.rs` |
| Vehicle phases | A vehicle has no trace inside its tick. Captured, not compared | not implemented | the recorder's `README.md` |
| Minecarts | Rail and subtype physics are not complete. The experiment needs `minecart_improvements` | not implemented | B189, `docs/parity-contract.md` |
| A live bot through mineflayer's plugin | The plugin does not yet call `MinecartTracker`, `pushByMovingPiston` for a vehicle, or fill `nearbyEntities` for vehicles | not implemented | B312 |
| Adapter and bot replay layers | Done in part: the bot drives movement, the keys of the hands (Minecraft.handleKeybinds) and the clicks on a container's screen through the fork; what the client sent of chat, signs, books and commands is counted, not driven | untested | the recorder's `README.md`, B221 |
| Bot replay, a passenger in nether lava | A passenger in lava takes the lava's current from mineflayer-physics-utils' fluid probe, which pushes at the overworld's 0.0023333333333333335 only (its ultra-warm 0.007 is a TODO there): in an ultra-warm dimension the fork leaves the push out | not implemented | `lib/plugins/physics.js` riderInLava, mineflayer-physics-utils botcraft.ts fluidPhysics |
| Bot replay, a horse's random rear | 1.9 - 1.19.2 a horse the client controls rears on its own now and then: Mob.baseTick plays its ambient sound when the client's own Random says so, and AbstractHorse.getAmbientSound stands it one time in ten, as getHurtSound does one time in three on a hurt the server sends (1.16.5 Mob.java:248-251, AbstractHorse.java:385-401, 714-719). The bot report tells such a rear from the rows (the recorder's replay/limitations.ts: the mount's standCounter leaving 0 with no jump release or use that stands it, the divergence on that tick or the next) and lists it as `limit`, not FAIL. No packet carries that Random. The recorder seeds it (EntityRandomMixin, from the case's seed and the entity's order among the client's entities, Recorder.seedOf), but the draws it made before the body began (from its client spawn, AbstractHorse.aiStep's tail and the other draws of its ticks) are in no row, and a model of the baseTick and aiStep draws from the derived seed does not land on the recorded rear tick, so neither the bot nor the harness can tell when it rears: `v_horse_jump_full_charge`, `v_horse_turn`, `v_horse_into_lava`, `v_horse_step_up_block`, `v_swim_*`, `cf_ride_horse_stops_in_web` and `cf_ride_edit_push_jump` (1.11 - 1.19.2) diverge from the tick it stands on | cannot | vanilla client randomness |
| Bot replay, a 1.8 moving piston head | Before 1.9 a moving piston's collision box is the pushed piston head's Block.getCollisionBox, which reads the bounds of the one PistonHeadBlock every head shares (1.8.9 PistonExtensionBlock.java:121-133, Block.java:401-405). PistonHeadBlock.appendCollisionBoxes leaves them a full cube (PistonHeadBlock.java:100-105), and Block.rayTrace sets them to the head's own shape (Block.java:512-514, PistonHeadBlock.java:136-138) from the crosshair pick the client runs every frame it renders (GameRenderer.updateTargetedEntity, GameRenderer.java:316-323, 1010-1016, MinecraftClient.java:862). Which was last depends on the frames between the ticks, which no row holds; from 1.9 the box is the block state's (1.9.4 PistonHeadBlock.java:54). `pm_ride_boat_lifted` and `pm_opposed_same_tick` on 1.8.8 diverge from such a push. The bot report tells it from the rows (the recorder's replay/limitations.ts: a position divergence within three ticks of a piston's block event) and lists it as `limit` | cannot | vanilla client render state |
| Bot replay, a mount's walk animation | A strider seats its rider by its walk animation (Strider.getPassengerAttachmentPoint). The rows do not carry it: the harness seeds it from the mount's one move of a first tick (age 1), and an older mount starts it at rest | not implemented | the recorder's `replay/bot.ts` |
| `state-bridge` equivalence | Checked on listed fixtures, not over the state space. Vehicles are not classified there | untested | `docs/state-bridge.md` |
| World sync | A chunk the caller never wrote reads as air, not as an error | differs | `docs/synchronization.md` |
| `NativeWorldSync.refused` | A block the engine refuses stays a gap. The caller must read this map | differs | `src/mineflayer/world.ts`, `test/chunk-lifecycle.test.cjs` |

## By platform

| Target | Limitation | Evidence |
| --- | --- | --- |
| macOS, Linux musl, Linux arm64 | Not built, not supported. The loader's branches are no support | `README.md`, `docs/release-build.md`, B264 |
| One combined tarball | None exists: each tarball holds its host's targets | `docs/release-build.md` |
| Browser | The determinism corpus did not run in a browser | `docs/determinism.md` |

## Findings of the source audits

`docs/parity-dispositions.json` holds what became of D-161 to D-229 in `parity-deviations.md`. Its status counts are
not an active defect census. Each row below is a source argument: read from vanilla's code, with no trace or regression
that shows the difference. They were dispositioned once, under B233, and were not each re-read for this page.
The recorder's `replay/test/evidence.test.ts` holds that file's counts. An entry leaves this table when its disposition changes.
Each active or fixed-not-validated row keeps the families its scope touches incomplete in `generated/subsystem-matrix.json` (the recorder's docs/subsystem-matrix.md), however
their replays went: no recorded case differs there, and these say the code differs where nothing was recorded.

### Active: the code differs by the source

| Finding | Releases | Scope | Title | Tickets |
| --- | --- | --- | --- | --- |

### Fixed, not validated

The code was changed. Nothing yet shows the change against vanilla.

| Finding | Releases | Title | Tickets |
| --- | --- | --- | --- |

## Excluded on purpose

Behaviour left outside the requested scope, so that no matrix cell counts it as verified. The subsystem matrix
(`generated/subsystem-matrix.json`, the recorder's docs/subsystem-matrix.md) lists each beside the families it touches; the replays compare everything up to it and stop there.

| Behaviour | Cases and modes | Why it is left out | Source | What is verified instead |
| --- | --- | --- | --- | --- |
| The fishing hook's bite, and its rod's retrieve | every fishing hook (`m_fishing`, `m_flow_fish`); projectiles | The bite is the server's random: `catchingFish` draws `timeUntilLured`, `timeUntilHooked` and `nibble` from the hook's `random`, and the client's `DATA_BITING` update pulls the hook by `-0.4F * Mth.nextFloat(syncronizedRandom, 0.6F, 1.0F)`, each biting tick by `-0.1 * nextFloat() * nextFloat()` of `syncronizedRandom`, a random seeded from the UUID and the game time. No recording holds either random's state, and the engine has no bite. The retrieve is the rod's use (`FishingRodItem.use` calls `FishingHook.retrieve`), a player's action that ends the hook, not the hook's physics. | 26.3 `FishingHook.java:123-136` (onSyncedDataUpdated), `:145` (the seed), `:205-218`, `:285-331` (catchingFish; the bite's `FISHING_BOBBER_SPLASH` at `:326`, `DATA_BITING` at `:331`), `:417` (retrieve), `FishingRodItem.java:27`; 1.8.8 `FishingHook.java:333-347` (`random.splash` at `:343`), `:446` | The hook's flight, its bobbing (the one `nextFloat` of each pull read back from the motion, k / 2^24) and its drag, every tick up to the bite or the retrieve, on all 50 releases: the recorder's `replay/projectile.ts` stops a hook at the row whose packets bring the bite's splash (`random.splash` before 1.9, `entity.bobber.splash` to 1.12.2, `entity.fishing_bobber.splash` after) or after the player's use of an item, and says where it stopped. |

## Keeping this page

- A limitation leaves this page only with a current trace or test. `test/limitations.test.cjs` holds the two finding tables to
  `docs/parity-dispositions.json`, and the first tables to the repository's own files.
- The older report `parity-deviations.md` stays as history. Its counts are of that audit, not of today.
- The audit folder `audit/physics-version-review-20261104/` is gone from this machine (`docs/finding-ledger.md`). M01 to
  M32 are each fixed in their tickets, and that does not extend past what each ticket covered.
