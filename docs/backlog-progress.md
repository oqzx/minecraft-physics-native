# Physics backlog progress

This records narrowly completed actions from the separate B001–B295 backlog.
It does not certify complete vanilla parity for a release. The older backlog
documents and the audit overview in `todo.md` describe their original snapshots.

## B093 — Server-only periodic freeze damage

The production guard was already merged in `f250ac0`. Acceptance coverage now
checks two consecutive damage boundaries (ticks 39–81), client/server ownership,
immunity, counters at and above the limit, and leaving powder snow. The 1.21.5
client inside-block FREEZE effect is kept separate from server periodic damage.
The public object API test carries accumulated `freezeDamage` through repeated
ticks and a 43-tick batch and checks unchanged stationary movement.

Checks cover registered releases 1.17, 1.20.4, 1.21.4, 1.21.5, 1.21.11 and 26.3.
The relevant implementation is `native/src/simulation.rs`; regression tests are
in `native/src/simulation_tests.rs` and `test/native-freeze.test.cjs`.

Source evidence:

- `E:/pr/mc-decompiled/1.17/net/minecraft/entity/LivingEntity.java:2557`
  and `1.20.4/net/minecraft/entity/LivingEntity.java:2620`: server guard.
- `E:/pr/mc-decompiled/1.21.5/net/minecraft/world/entity/LivingEntity.java:2776`
  and `E:/pr/mc-source/26.3/client/net/minecraft/world/entity/LivingEntity.java:3074`:
  freezing section runs on `ServerLevel`.
- `E:/pr/mc-source/26.3/client/net/minecraft/world/entity/InsideBlockEffectType.java:7`:
  FREEZE increments the counter independently of periodic damage authority.
- `D:/projects/mc-zuri/mc-data/extracted_minecraft_data`, commit
  `09859053de9898f726a798244c1d219ec1d42dee`, path
  `client/net/minecraft/world/entity/LivingEntity.java`, blob
  `977e2fd3f3f268bbadd2273a5d8ff2fd6b0c562f`: independent 1.21.11 reference.
- `D:/minecraft/java/physics-data-generator` supplies powder-snow collision
  observations, but those are not evidence of periodic damage authority.

These are source-derived regression checks; no new vanilla client recording
or full release conformance run is claimed.

Validation on 2026-11-04: `cargo test --locked --offline --manifest-path
native/Cargo.toml` passed 764 tests; TypeScript compilation passed. The full
JavaScript run passed 133 of 134 tests after setting the explicit source root;
its remaining dependency-resolution failure passed on recheck (all four tests
in `legacy-release-data.test.cjs` and `native-freeze.test.cjs` passed). Native
API tests loaded the newly compiled debug addon. Worktree test configuration:
`MC_DECOMPILED_ROOT=E:/pr/mc-decompiled`, `NODE_PATH=E:/mc-physics-demo`, and
`NAPI_RS_NATIVE_LIBRARY_PATH` pointing to that worktree's debug addon.

## B196 — Legacy six-face projectile clipping

Pre-1.13 projectiles now use the historical six-face AABB clip. It returns exit
hits for rays starting inside the box, accepts segment endpoints and face edges,
uses the float-promoted squared-axis cutoff, and preserves Java's face tie order
and zero normalization. Block queries subtract the block coordinate before
clipping local geometry and then add it back, preserving large-coordinate
rounding. The 1.13+ entering-face and inside-shape behavior remains unchanged.

Two public API regression cases failed on the pre-change freshly built addon:
an interior ray missed its exit face, and a tiny diagonal ray incorrectly hit.
Coverage now also includes all six directions, zero-length and parallel rays,
the exact float cutoff, corner/edge/end hits, supplied multipart shapes and
positive/negative large-coordinate partial shapes. Independent source review
found no blocking defect.

Primary references in `E:/pr/mc-decompiled`:

- `1.8.9/net/minecraft/util/math/Box.java:195` and `Vec3d.java:81`.
- `1.9.4/net/minecraft/util/math/Box.java:233` and `Vec3d.java:96`.
- `1.10.2/net/minecraft/util/math/Box.java:252` and `Vec3d.java:112`.
- `1.11.2` and `1.12.2/net/minecraft/util/math/Box.java:294`,
  `Vec3d.java:112`; `1.12.2/net/minecraft/block/Block.java:520` establishes
  block-local arithmetic.
- `1.13.2/net/minecraft/util/shapes/VoxelShape.java:150` and
  `util/math/Box.java:241` establish the transition to the modern clip.

The corresponding 1.13.2 sources were independently checked in
`D:/projects/mc-zuri/mc-data/extracted_minecraft_data`, commit
`a071431ad932bf05efaa130235160a4caa98afdb`, paths
`client/net/minecraft/util/math/AxisAlignedBB.java` and
`client/net/minecraft/util/math/shapes/VoxelShape.java`.
Modern controls also reference `E:/pr/mc-source/26.2/client/net/minecraft/world/phys/shapes/VoxelShape.java`.
`D:/minecraft/java/physics-data-generator` remains supporting capture tooling;
this change does not claim a newly recorded vanilla projectile trace.

Scope limits: the registered 1.8.8 path uses the available 1.8.9 stand-in;
its exact source provenance is still unverified. This completes the six-face
primitive and its projectile dispatch, not legacy `World.rayTrace` traversal
or each block's multipart-selection override (for example, stairs). Those remain
part of the broader projectile/collision backlog, B198/B200.

Validation on 2026-11-04: all 771 Rust unit tests and all 136 JavaScript tests
passed, including B093's API coverage and the package/smoke checks. Rebuilt the
checked-in Windows addon with `cargo build --locked --offline --manifest-path
native/Cargo.toml --release`; tests used that release artifact through the normal
loader. Addon SHA256:
`5f0322a88c6ed20a53ae79be4fa096bf6c1580324f61e1e427297620733c9472`.
Logs are retained in `audit/physics-backlog-20261104-dc4f918` in the main checkout.

## B136 — Passenger fluid geometry

The optional `boatVehicle` input supplies absolute boat bounds and underwater
status at the fluid-query phase. Native object APIs, entity descriptors, packed
players and the Mineflayer adapter retain it across ticks and apply it only while
the entity is a passenger. Malformed bounds are rejected before caller state is
mutated. The adapter snapshots nested coordinates, including in-place changes.

Historical boat water and eye exclusions now follow their separate release
boundaries. Since 26.1, the boat can remove or trim the already-deflated fluid
scan for water, lava, eyes and current; depth still uses the original body bottom.
Physical collision dimensions and eye coordinates remain the passenger's own.
Post-move fluid refreshes use the same vehicle-aware queries.

See `docs/passenger-fluid-context.md` for the input contract, full breakpoint
table, and references to all four requested source collections. Important exact
boundaries are 1.16.2 (inclusive vertical eye exclusion), 1.19.3 (underwater boats
permit water contact), 1.20.5 (actual eye height), and 26.1 (adjusted shared scan).
Historical Java in `E:/pr/mc-decompiled` is the primary reference. The 1.16.1–1.16.4
boundary and 26.1.2 methods were also inspected in release-identified cached jars
using the installed JDK; their hashes and complete method bytecode are retained.
No new vanilla recording or complete release-conformance claim is made.

The context is caller-supplied and fixed during a batch. This closes the missing
fluid geometry input, not boat motion, seat placement or the full passenger tick.
Registered 1.8.8 remains unchanged with only the available 1.8.9 stand-in evidence.

Source excerpts, pinned Git files, bytecode, version IDs, commands and artifact
hashes are under `audit/physics-backlog-20261104-dc4f918/b136-evidence` in the main
checkout. Independent source/bridge review found no remaining blocker.

Validation on 2026-11-04: all 780 Rust tests and all 143 JavaScript tests passed,
including package verification and native smoke checks; TypeScript compilation
passed. Regenerated the native declarations and Windows release addon with the
N-API CLI, passing `--locked --offline` to Cargo. The JavaScript suite loaded that
artifact through the normal loader. Addon SHA256:
`1cfd0a174436c92fa7952a552c4f153205e601de6a502b0aba6324785e6ba8f6`.

## B151 — Slime-piston authority and historical server players

The 1.16.2–26.2 slime-piston path now skips both bounce and displacement for a
server player, while ordinary pistons, honey carry and subsequent pistons still
run. Exact cached 1.16.1–1.16.4 bytecode pins the start of this rule. Since 26.3,
only the slime impulse is conditional; the piston still moves remote entities.

The optional `canSimulateMovement` input represents the exact 26.3 result for
entity modes not inferable from existing state. It round-trips through descriptors,
object/batch APIs, packed state and adapter replacements without freezing an
inferred fallback into the returned state. See `docs/piston-authority.md` for
default modes, all four reference collections, pinned sources and scope limits.

Piston displacement now preserves ground/support state on horizontal moves when
the 1.21.4+ local-authority guard requires it. Collision restitution follows its
separate simulation guard from 26.2, and uses retained vertical collision flags.
Inside-block callbacks still run. Tests distinguish simulation permission from
ownership of ground flags, including remote wall collisions and floor bounce.

Three public API regressions failed on the preceding release addon. Core checks
cover six directions, extension/retraction, exact patch boundaries, multiple
pistons, honey, explicit/default authority modes, callbacks and support retention.
Independent review found no remaining blocker. Nonliving restitution's separate
`0.8F` block factor remains outside this authority contract and is recorded in B163.

Downstream validation also found the recorder's exhaustive field registry needed
entries for B136's boat context and this authority override. Both now have explicit
exclusion reasons: ridden replay runs the vehicle layer rather than a passenger
fluid tick; the local player's inferred movement permission is already exact.
No new passenger recorder coverage is claimed. Source and bytecode evidence is
in `audit/physics-backlog-20261104-dc4f918/b151-evidence` in the main checkout.

Validation on 2026-11-04: all 789 Rust unit tests and all 148 JavaScript tests
passed; TypeScript and recorder type checks passed. Recorder schema/scenario/replay
tests passed 22 checks with four recording-dependent skips (the worktree does not
attach the original checkout's recording corpus). The generated-Java check passed
after regenerating checkout line endings; Git confirmed no schema content changes.
This is not a full replay of the archived recordings. Regenerated native types
and tested the rebuilt Windows release addon through the normal loader. SHA256:
`9c97c63344a159fd8bcdc7dbc97de09e26ba315e9f58f29d28b34579994b8397`.

## B203 — Consistent registry and release selection

Generic/fixed/factory player engines, boat/mount engines, standalone world sync
and block mappers now require an exact supported registry/release match before
accessing the native binding or allocating a world. Optional registry equality
callbacks must confirm their own declaration; they cannot approve aliases.
Mapper behavior uses numeric checks derived from the selected release.

This prevents hybrid attribute names, missing newer attributes and feature/block
data from another release. Matched direct, fixed and factory engines retain
identical attribute-sensitive travel across the 1.20.5 and 1.21.5 boundaries and
26.3. Tests also cover opposite-direction mismatches, same-protocol patch pairs,
malformed IDs, early validation and inconsistent dependency fallback identities.
Three public regressions failed before the change; matched behavior already passed.

See `docs/registry-release-contract.md` for the stricter public contract,
migration and the six unverified aliases in the inspected data checkout. Older
adapter test proxies were replaced with actual matching registries/worlds.
The native release modules remain available even where the adapter's dependency
lacks coherent exact data. Arbitrarily mislabeled registry contents are not
deep-verified. The separate physics-util feature-catalog defect is now B296.

The source/metadata plan and independent bridge review are retained in the main
checkout's `audit/physics-backlog-20261104-dc4f918`. No fresh vanilla recording or
archived-recording replay is claimed. This changes TypeScript validation, not
native physics algorithms or the B151 release addon.

Validation on 2026-11-04: all 153 JavaScript tests passed, including package/smoke
checks; main and recorder TypeScript checks passed. Recorder tests passed 22
checks with the same four recording-dependent skips. Native code is unchanged
from the prior 789-test Rust run. Independent review found no blocking issue.

## B094 and B096 — Fire reset side and generic fire delay

Both production fixes were already on `recorder` when these actions were
revalidated; the tickets described the audit snapshot. This work closes their
acceptance coverage and changes no physics.

`native/src/simulation_tests.rs` now pairs client and server, player and generic
entity, with counters -5, 0, 1, 2 and 160 on 1.21.5, 1.21.6, 1.21.11 and 26.3,
dry and in rain. It also covers ignition and extinguishing in one step, where the
ticks read before `checkInsideBlocks` decide the reset, the generic delay through
extinguishing and the next fire contact, and the invulnerable player's clamp.
`native/src/napi_player.rs` checks that the kept player and object state agree on
`remainingFireTicks` for 1.21.5, 1.21.6 and 26.3, from an omitted counter and from
explicit -5, 0 and 160.

The matrix separates the client's `clearFire` from the reset it was first assumed
to be: `Entity.clearFire` writes 0 before 1.21.9, so a client's -5 becomes 0 at
1.21.6 and stays -5 from 1.21.9. Only the matching-release cases above were run.

Source evidence: `E:/pr/mc-decompiled/1.21.5/net/minecraft/world/entity/Entity.java:817`
(older predicate) and `1.21.6/net/minecraft/world/entity/Entity.java:609,851`
(`clearFire` and the server guard). These are source-derived checks; no new
vanilla recording is claimed, and 26.1.2 has no exact source reference.

Validation on 2026-11-05: `cargo test --locked --offline --manifest-path
native/Cargo.toml` passed 793 tests.

## B095 — Ordered ServerPlayer fire draws

`serverFireIncrements` carries the `nextInt(1, 3)` of each ignition in order. A
draw is read only where vanilla reads it: 1.21.2 and later, a non-immune
ServerPlayer, with the counter at 0 or above at that ignition. The state returned
holds the draws not read, so a stream continues across calls.

The stream is stored as one bit per draw inside `SimulationState`, which keeps the
state `Copy` and carries the stream through every tick path, batches and the kept
`NativePlayer`. It therefore holds at most 128 draws; a longer replay refills it
between calls. A value other than 1 or 2, an oversized stream, or a stream given
beside `serverFireIncrement` is refused before any state changes. Running out fails
the call, and a kept player returns to its state before that tick. The scalar
`serverFireIncrement` remains as a documented fixed-value approximation.

The draws are supplied observations of the level's random. The engine does not
reproduce that random, and a client reads none, so the recorder excludes the field.
Rust and N-API tests cover draw order, consumption, version boundary, exhaustion and
tick/batch/kept-player agreement. No new vanilla recording is claimed.

Validation on 2026-11-05: 799 Rust tests passed; 155 of 156 JavaScript tests passed
in the worktree. The one failure, `matches the 1.13.2 block states minecraft-data
lists`, could not resolve `minecraft-data` through the worktree's `node_modules`
junction; it passes in the main checkout. Main and recorder TypeScript checks passed.

## B194 — Arrow containment and noPhysics

The containment check was already on `recorder`: the block at the arrow from 1.14,
the block of the last hit before that, with motion cleared from 1.21.5. What was
missing is `noPhysics`, which 26.3 `AbstractArrow.tick` reads before containment.
Arrows and tridents now take `noPhysics`, honoured from 1.13 (1.13.2
`AbstractArrowEntity.java:156`) and ignored earlier, where the flag does not exist.
Such an arrow skips containment, the in-ground branch, the block ray and gravity,
and keeps its air or water inertia.

Tests cover both sides of 1.13, embedded and wet no-physics arrows, a partial shape,
the top face, the lower faces across the 1.13 `Box.contains` change, and a block
placed around an arrow between ticks. No new vanilla recording is claimed.

Validation on 2026-11-05: 802 Rust tests passed; 155 of 156 JavaScript tests passed,
with the same worktree-only `minecraft-data` failure as B095. Main and recorder
TypeScript checks passed.

Both B095 and this change add fields to the native addon. The tracked
`native/artifacts/index.win32-x64-msvc.node` was not rebuilt on `recorder`, because
the main checkout holds an uncommitted copy of it; until it is rebuilt,
`test/native-fire-draws.test.cjs` fails there and the new fields are ignored.

## B101 — Rain-only Riptide releases

Native code and the adapter already accepted a release with `inRain`. Adapter tests
now check the passenger restriction on both sides of 26.1, the ten-tick charge, a
trident without Riptide, no launch before 1.13, and the launch's movement.

This closes the release path, not rain detection. `inRain` is supplied by the
caller, and nothing fills it on a live bot, so a live bot does not yet launch from
rain alone. Vanilla exposure needs weather, sky, heightmap and biome precipitation,
so that work is tracked separately as B297 rather than approximated from the global
weather flag. No new vanilla recording is claimed.

Validation on 2026-11-05: the adapter test passed; only a test changed.

## B190, B191, B192 and B193 — Projectile acceptance and base tick

Closing the acceptance matrices exposed real defects, now fixed against source:

- The border bounce starts at 1.21.2, not 1.21.5: `clipIncludingBorder` and
  `deflectsAgainstWorldBorder` are present in 1.21.3 and 1.21.4
  (`ProjectileEntity.java:254-266`). An arrow or trident never embeds in the border.
  The old test that claimed embedding through 1.21.4 was wrong and was replaced.
- The fishing hook bounces too, and on a client keeps its motion in 26.3, where
  deflect is server-only (26.3 `Projectile.java:278-284`).
- An arrow's base tick runs on the tick of a hit and not on the tick it falls
  (26.3 `AbstractArrow.java:192-212, 251`). A current pushes an embedded arrow until
  1.21.4 (1.21.5 `AbstractArrow.java:319`).
- Rocket, hook, spit and shulker bullet open with the base tick, so they refresh
  water and take its push. The bullet's box is 0.3125 (1.21.5 `EntityType.java:800`).

Arrow and trident tick results now return `life`, which a one-tick caller needs for
the 1200-tick count. Loyalty stays unmodelled and entity-versus-block hit ordering is
untested. Lava's current on projectiles is B298. No new vanilla recording is claimed.

Validation on 2026-11-05: 827 Rust tests passed. With a rebuilt addon 164 of 165
JavaScript tests passed, the failure being the worktree-only `minecraft-data` one.
Four of the new adapter tests need that rebuild, like `native-fire-draws`.

## B133, B135, B150 and B165 — Fluid and collision acceptance

- B133: the 1.13.2 flow-blocker generator missed `air`, `tripwire_hook`,
  `chorus_plant` and `kelp`, so air blocked the flow look-below (1.13.2
  `FlowableFluid.java:64`, `Material.java:7`). `src/mineflayer/flow-blockers.json` was
  regenerated and only 1.13.2 changed. A 26.3 `BlockMapper` without a tag test now
  uses the 26.2 predicate, not collision boxes.
- B135: tests cover both fluids, equality at 8/9F and 1, partial faces, unchanged
  depth and current, and the native eye flags.
- B150: test-only work counters prove the traversal budget bounds the walk; removing
  the check fails the test. Order is compared with a transliteration of the source,
  with 1.21.8/1.21.9 controls and web, powder snow and bubble effects.
- B165: exact 1e-7 is covered at collider and support level. The tick test uses 5e-8
  and 2e-7 on either side of it.

B134 has its tests merged (lava against elytra travel, a height sweep, negative and
integer boundaries, the 1.14.4 control) but stays open: the expected values are
source-predicted, and its criteria also ask for a live trace, which was not recorded.

Validation on 2026-11-05: Rust and JavaScript results are as run after rebasing on
`recorder`; no new vanilla recording is claimed.

## B100, B122, B173 and B204 — Immobility, effects, firework gravity, ride tick

- B100: the object API lost its keys after an immobile tick, and auto-jump read the
  cleared input; both are fixed (26.3 `LocalPlayer.java:983,1003,1110-1114`).
- B122 and B173: tests only, with no behaviour change.
- B204: a standalone passenger now takes a ride tick at every entry point instead of
  ordinary travel. Motion is zeroed, the tick runs, and it ends on the seat the caller
  gave, with the fall reset. Swimming, glide start, flight travel, off-ground from
  1.21.4 and closest-space from 26.3 follow their passenger gates. `simulatePlayer` is
  safe on a riding bot; `src/mineflayer/types.ts` holds the contract. The spin sweep
  sees the seat, and the dismount offset is the caller's. One expectation changed: a
  1.21.11 passenger's Riptide launch no longer moves it, as rideTick zeroes the motion
  (`Entity.rideTick`). Passenger sprinting is B300.

Validation on 2026-11-05: 872 Rust tests passed. With a rebuilt addon 176 of 177
JavaScript tests passed, the failure being the worktree-only `minecraft-data` one.
Replaying `physrec/c_ride_*` recordings: 1.12.2 34 of 36 and 1.20.4 43 of 45 match, the
rest not compared; 26.3 72 of 76 match, 3 not compared, 1 failing. The failure,
`c_ride_strider_boost`, also fails on the older addon, so it is not from this work; it
is B299.

## B180, B181, B182 and B183 — Mount acceptance

- B180: slowdown stays in the adapter's carry and is now also written to the state,
  so a position correction keeps it (26.3 `Entity.java:783-790`). `NativeHorseState`
  gains `weaving`. Tests cover entry, stay and exit for horse, pig, llama and camel.
- B181: tests only.
- B182: `getFluidJumpThreshold` uses scaled eyes (`Entity.java:3803`), and the adapter
  refreshes size when a camel stands.
- B183: attributes are sanitized as `RangedAttribute`, as NaN drag or friction poisoned
  velocity. Slow Falling is read after the move from 1.21.2, bounces use the modified
  air drag, gravity 0 skips the fluid adjustment, landing is `<=` from 26.3 (shared
  with boats), and a non-finite or negative `flyingSpeed` is refused. Attributes above
  their maximum on the raw API are B301.

There is no separate mount replay path, so these tests compare batches with repeated
calls; the recorded evidence is the riding replay below.

Validation on 2026-11-05: 915 Rust tests passed. With a rebuilt addon 193 of 194
JavaScript tests passed, the failure being the worktree-only `minecraft-data` one.
`physrec/c_ride_*` replay gave the same results as for B204, including B299.

For B133, B135, B150 and B165 the run after rebasing was 853 Rust tests and 163 of 170
JavaScript tests; the seven failures were the worktree `minecraft-data` test and tests
waiting on the addon rebuild, all of which pass here with a rebuilt addon.

## Replay after the B-series merges, and B299

An addon built from the merged engine replayed every recording of three releases:
1.8.8 had 1027 of 1036 matching, 1.20.4 had 1428 of 1438, and 26.3 had 1516 of 1528,
the rest not compared apart from one failure. These merges caused no regression there.

That failure, `physrec/c_ride_strider_boost` on 26.3, was a stale recording, not an
engine fault. Recordings made before `90a550f` have no `movementSpeedModifiers`, which
the replay's strider speeds now read. The case failed the same way on 15 more releases.
Re-recording it on all 16 with `--verify` gave identical runs, and each replay matches.
Recordings are not tracked, so this changes no file in the repository.

The other 21 releases with a strider case match without re-recording. Replay cannot
run on 1.19.1, 1.21.2, 1.21.7 and 1.21.10, as B203 refuses their aliased registry data;
that is B302. The other releases were not replayed in full after these merges.

## B298 — Lava current on projectiles

Projectiles, arrows and tridents now take lava's current from 1.16, after water's in
the same base tick: 0.0023333333333333335, or 0.007 with the new `ultraWarm` state
(1.16.5 `Entity.java:1067-1072`, 26.3 `:1698-1707`). 1.15 has no lava current. Lava
does not replace the air's inertia. An embedded arrow is not pushed from 1.21.5.

These are source-derived tests; no new vanilla recording is claimed.

Validation on 2026-11-05: 917 Rust tests and 194 of 194 JavaScript tests passed with a
rebuilt addon and `NODE_PATH` pointing at the `minecraft-data` checkout's parent, which
also clears the worktree-only failure noted earlier.

## B301 — Raw mount attributes above their maximum

The raw N-API mount paths now refuse `movementSpeed` above 1024 and `jumpStrength` above
2 up to 1.20.4 and 32 from 1.20.5, as they already refused negative values. Refusing
keeps the existing contract that these inputs are effective attribute values. The
adapter clamps before it calls them, so its results are unchanged.

Validation on 2026-11-05: 918 Rust tests passed. No JavaScript surface changed.

## B300 — Passenger sprinting

The adapter now follows the vehicle's sprint rules. A passenger needs no food from
1.19.3. From 1.19.4 it starts a sprint only if `vehicleCanSprint` is set, and a
double-tapped sprint reads the vehicle's ground (`vehicleOnGround`, 1.19.3 either, from
1.19.4 the vehicle's alone). From 1.21.4 a sprint stops when the vehicle cannot sprint,
and from 1.21.5 a swim sprint too. 1.21.4 tests for a camel rather than `canSprint`, so
a player carried as a vehicle differs there; no case covers that. The caller supplies
both new fields; `vehicleCanSprint` defaults to false, as `Entity.canSprint` does.

Validation on 2026-11-05: 196 of 196 JavaScript tests passed with a rebuilt addon, and
`physrec/c_ride_*` replay results are unchanged on the releases around the boundary.

## B134 — Legacy lava sampling, live trace

`physrec/f_elytra_into_lava_45` glides from a tower into a lava pool, and its box is
0.6 high from tick 8 to the lava at tick 57. Recorded with `--verify` on 1.9.4, 1.10.2,
1.11.2, 1.12.2, 1.13.2 and the 1.14.4 control, every run was identical and every replay
matches. With Java's reordering removed from `touching_lava`, 1.9.4, 1.12.2 and 1.13.2
fail at tick 58 with `isInLava: expected true got false`, so the trace does catch the
defect. The 20-degree case passes the pool without touching lava and proves nothing
here. On 1.13.2 `fallFlying` is recorded at tick 0 only, though it does change on 26.3, so the box height is the evidence of a
glide. The 1.14.4 result is a control, source-predicted and now live.

The new `skyLava` area is last in the list, so existing areas keep their places.

While adding this, the recorder type check showed `NativeHorseState.weaving` from B180
missing from the schema; that branch's checks had not included it. It is mapped now,
and the recorder type check and 14 schema and scenario tests pass.

## Partial work on B102, B104, B106, B113 and B119

Five defects found by reading these tickets against source are fixed. The tickets stay
open, as each covers more than these fixes.

- B119: a passenger skips the item-use slowdown, on every release from 1.7 (1.8.9
  `ClientPlayerEntity.java:540`, 26.3 `LocalPlayer.java:695`). `attackHit` applies a
  landed hit's recoil once, with full strength from 1.9 and the sprint bonus from
  1.21.11. The caller must set it only for hits on remote players. No recording covers
  attacks, so that part rests on source and unit tests.
- B113: a passenger's flight toggles only on a jumpable vehicle from 1.21.11
  (`vehicleJumpable`).
- B104 and B106: the double-tap window reads `sprintWindow` from 1.21.9, and an illegal
  value gives the default 7.
- B102: contact damage is 0 on a client state (26.3 `Entity.java:1969`). It is not
  forwarded through the packed state, as it has no consumer there.

1.21.10 cannot load in the JavaScript tests (B302), so 1.21.9 stands in for it.

Validation on 2026-11-05: 920 Rust, 202 JavaScript and 14 recorder tests passed. Riding
replays on 26.3, 1.20.4 and 1.13.2 and the 125 `*use*` cases on 26.3 match their baseline.

## Replay of all 50 registered releases

On 2026-11-05 every recording of every registered release was replayed, with no
unexpected mismatch left. Between 569 (1.15.2) and 1528 (26.3) cases per release match or
are marked not compared, at most 11 for a release.

- Engine: not one build. 26.1.1, 26.1.2, 26.2, 26.3, 1.19.1, 1.21.2, 1.21.7 and 1.21.10 ran
  on the final merged engine. 1.8.8 and 1.20.4 ran after the mount merge, and the others
  after B299, before B298, B300, B301 and the player gates. Those later changes were
  checked with targeted riding and item-use replays only.
- Stale recordings: 2 strider scenarios, `c_ride_strider_boost` and `v_strider_*`, predated
  `movementSpeedModifiers` on 1.19.4 to 26.3 and were re-recorded with `--verify`.
- A passing replay says the engine matches the cases recorded. It is not proof of parity
  for behaviour no case exercises.

## Partial work on B144, B163, B175 and B178

Four defects found against source are fixed. Each ticket has acceptance work left, so
all four stay open. These rest on source and unit tests: the replays run do not reach
the new paths.

- B144: a bubble column's push asks `canSimulateMovement` from 26.3 (`Entity.java:2974,
  3006`; 26.2 has no gate). Boat and column acceptance remains.
- B163: `nonLiving` gives 0.8F restitution from 26.2, with the older slime (1.9) and bed
  (1.12) bounce. The piston carry and direction matrix remains.
- B175: `wallDamage` reports per-tick, server-only wall impact from 1.9
  (`LivingEntity.java:2505-2510,2546-2553`). `f_elytra_wall_hit` shows no health change,
  so no trace compares it yet.
- B178: rockets honour `noPhysics` for hits and move (`FireworkRocketEntity.java:152`).
  Attachment, removal and explosion remain, and entity hits are B195.

## New scenarios and what they found

Two batches of scenarios, 380 cases, cover behaviour nothing recorded before. They ran on
1.8.8 and 26.3 first. The interaction batch gave 34 unexpected failures, all explained:

- Lava's `Vec3.add` clears -0.0 (26.3 `LivingEntity.java:2474`). The engine kept it.
- The world border collides over its grid past a corner (`VoxelShape.java:226-238`).
- A moving piston has its own shape (`PistonMovingBlockEntity.java:327-352`, 1.8.9
  `PistonExtensionBlock.java:122-197`), now given through `setMovingBlock`.
- The replay lacked team rules and `isOutsideBorder`.
- `movingSlowly` is left uncompared on the tick a glide ends on a climbable, because
  vanilla's pose lags. That is an exclusion, not a fix, and B309 tracks it.

Fall damage on 1.15 and later was never dealt in recordings, because the recorder turns
the rule off. A `mortal` case now turns it on, and the replay compares landings with
`srv.health` for `fd_*`. Those match on 1.8.8, 1.14.4, 1.15.2, 1.20.4 and 26.3, and a
wrong value failed there before the recording was fixed. This covers fall damage only:
contact and freeze damage are still not compared, so B305 stays open.

The engine now keeps a landing's damage to its own tick, and a jump leaves the fall
distance alone (26.3 `LivingEntity.java:2303`). `ps_dying_tnt_*` is server-timed. Replay
has no chunk model (B306). Four new cases are invalid and not compared (B309).

## B306, B307 and the replay model

- B306: the replay follows `map_chunk`, its bulk form and `unload_chunk` from recorded
  packets, so 1.8.8 `cu_tp_away_and_back` matches, and removing the chunk state fails it
  at tick 4. Only 1.8.8 has this case recorded before 1.14 so far.
- B307: a piston skips NOCLIP entities from 1.11 (1.10.2 has no such test; 26.3
  `PistonMovingBlockEntity.java:327-338`). Spectators stay excluded.
- B308, partial: `NativeWorldSync` tracks moving blocks from what the adapter sees.
  Slime and honey branching and pushability by name remain.
- B305, partial: `hz_cactus_*` compares contact damage, and `fields.ts` now says what is
  compared. Campfire and freeze cannot be compared yet: a mortal case must enable fire
  damage, and a mortal freeze case over 140 ticks is needed. Wall damage and replaying
  an elytra start through the adapter also remain.
- `es_slab_under_shulker`: `Player.tick`'s pose update crouches the player before the
  shulker ticks (26.2 `Player.java:266`), and the engine pushed with the standing box.
  Fixed from 1.14; the cases match on 1.21.1, 26.1.2, 26.2 and 26.3. B310 tracks the
  same order in the adapter.

Validation on 2026-11-05: 967 Rust, 213 JavaScript and 41 recorder tests passed, 9
recorder tests skipped. `pm`, `hz` and `fd` replays match, and `c_ride` has no
unexpected mismatch.

## B310, and partial work on B146, B154, B174 and B179

- B310, closed: `NativePlayer` gains `tickMove` and `finishTick`, so the adapter pushes
  after the pose update, and pose and crouch fits see entity colliders (26.3
  `Player.java:364-366`). The 1e-7 shrink applies from 1.16. The campfire has no
  inside-shape override, so it stays a full block.
- B174: tests only, as the adapter already matched source. No recorded traces, as the
  replay seeds vanilla's decisions, and 1.15.1 and 1.15.2 have no exact source.
- B179: `validElytraEquipped` is derived from the inventory, `isUsable` before 1.21.2
  and `canGlideUsing` after. The server's 20-tick interval is not modelled.
- B154: `generated/block-friction.json` covers 41 of 50 releases, as 9 lack exact
  source. The adapter's table matches on the 36 compared, and an unresolvable
  generation throws.
- B146: `entityTravelSupport` gives the inventory and refuses unsupported families.
  Fish travel is not implemented.

The pose fit still ignores the world border.

Validation on 2026-11-05: 975 Rust, 246 JavaScript and 41 recorder tests passed, 9
recorder tests skipped. `pm`, `hz` and `c_ride` replays on 1.8.8, 1.20.4 and 26.3 and
`es` on the latter two have no unexpected mismatch. `entityTravelSupport` needs a
rebuilt addon and `native.js`.

## Players and effects: first lane

Commits: dcb1a70, cf530f1, a6af7a6, e11e217, bacb53b, b906cdd.

- B108: gates proved against source; a jump too weak to fly still sets delay 10.
- B109: the flight-start jump follows jumpFromGround (1.21.2 gate, effect amplifier, kept supporting block); without a synced attribute Jump Boost raises the safe fall distance from 1.20.5. Replay of e_*, q_*, f_* matches on 26.3 and 1.8.8.
- B127: effect consumers listed against source; Darkness and Conduit Power move nothing.
- B128: effects are client snapshots with a signed byte before 1.20.5 and a clamp after, pinned by test. There is deliberately no server-side effect scheduler, so stronger-shorter replacement has nothing to run on a client.
- B129: the removal packet ends an effect, not the client's count; e_speed_runs_out_walk and e_levitation_runs_out record it.
- B130: armor enchantments come from the bot's inventory in both item formats, with the state fields as fallback. Rests on source and unit tests: the replay does not run the adapter.
- B131: Depth Strider reads any armor, Soul Speed boots, Swift Sneak leggings, Riptide the item in use. Frost Walker terrain is server-side. Rests on source and unit tests.
- B102: Contact, wall and freeze results and the block-fall flag are now in the packed output and adapter state. Left: the flag covers the player's own move, not a piston's.
- B104: The adapter's counters start at 0. Left: physics-utils clone() still loses fields and the carry; that code lives in that library.
- B113: The flight-start jump is fixed too. The rest of the ticket remains.
- B119: Spear stabs are server-only, so they have no client recoil. Left: recorded attackHit evidence, which needs a case with a remote player.

parity-deviations D-162 is fixed; its statement that Depth Strider reads the feet only is wrong, vanilla uses all armor.

## Recorder: mortal hazards, repaired cases, new coverage

Commits: 7a22698, fb87980, a74a171, 8096e66 before their rebase.

- B305: mortal now enables fire damage from 1.15 and freeze damage from 1.17, and hz_freeze_powder_snow records hits at t159, t199, t239 on 26.3. Left: the replay does not compare campfire or freeze hits with srv.health yet.
- B309: v_boat_*, c_ride_oak_chest_boat_onto_shore and ps_sleep_scale_2 now do what they say, each with a failing check. ps_dying_tnt_* are renamed ps_death_tnt_*: a player never reaches pose DYING (ServerPlayer.die skips LivingEntity.die, read from 1.14 to 26.3). Left: on 1.13.2 v_boat_bubble_up fails 'never came to rest' when v_boat_off_ledge_into_water runs directly before it, which a shuffled run could hit; sign-of-zero divergences on a boat falling into water (26.3, 1.20.4) and on mounts at a shore (26.3).

New cases: hz_freeze_powder_snow, np_* (nether portal contact), ep_* (end portal jump and frame), v_swim_* (horse family and camel in water), v_dismount_* (obstructed dismount). Left out because they leave the overworld or need written block entity data: finishing the nether wait, end portal block contact, end gateways. A llama swim is left out too: it wanders with AI and hangs without it.

## Collision, elytra and vehicles lane

Commits: 920312f c30f3f1 c95342e 3ef72b9 a24d8d4 3d474a6 dea032b 0c951d3 6bbfaf0 .

- B188: parse_for gates each ridden name by the release that registers its entity type, tested on both sides of each gate; c_ride and v_ traces match on 26.3.
- B175: the replay compares wallDamage with server health from the server's own motion; head-on, glancing and near-wall cases match on 1.9.4 to 26.3. The exact threshold stays unit-tested: a recording cannot land on it.
- B184: ledge and lava fall distance match on the recorded releases 1.8.8 to 26.3, with no exemption left in the replay; the ledger cases replay clean on 26.3 and 1.8.8.
- B170: bo_ and bw_ cover every edge, corner, outside start, clamp and moving border, and match on 26.3, 1.8.8, 1.12.2, 1.14.4 and 1.17.1; no code change.
- B187: Mounts refresh fluids per release (26.1 depth, live lava before 1.14, 1.21.4 skip). Left: safe dismount selection through an API.
- B178: The adapter keeps an attached rocket through a pause in the glide. Left: entity attachment, target removal and explosion.
- B166: rg_ corner matrix of 112 cases matches on six releases, no bug found. Left: the ticket's wider matrix.
- B172: rg_ matrix added, bounded, with no seed or palette reporting, so the ticket is not fully met.
- B171: Support tie-break control test added; it passes without a code change. Face and camera consumers are unaudited.

Engine fixes in this lane: a mob reads the fluid its move reached from 26.1, lava is a live query before 1.14, and 1.21.4 does not refresh the water of a player-controlled mount.

## Fluids and blocks lane

Commits: 0197cbd b83f139 0bd70a8 .

- B160: a grown berry bush hurts on 0.003F or more along x or z, age 0 deals none, and Fire Resistance refuses magma, fire and campfires; hz_berry, hz_magma and hz_burn compare srv.health.
- B143: the escape hop over 9 kinds of bank, in water and lava, is matched by jo_ on 14 releases; no bug found.
- B156: side slide thresholds, fast fall and scale matched by hw_ for players; no bug found.
- B157: slime against a ceiling matched by sj_ on either side of the 26.2 restitution change. Nonliving bounce is unit-tested only.
- B158: the flying exclusion matched by fy_cobweb with wv_, pw_ and fc_web. Ridden cases are unit-tested only.
- B138: The client's isOnFire reads the shared flag from 1.11 and the replay follows it from packets; mf_ matches. Left: ultra-warm lava cannot be compared without a mod change.
- B144: Player side matched by bt_, with the 1.21.5 gate visible in the data. Left: boat acceptance, with B146.
- B163: Honey and slime piston direction matrix matched by ph_. Left: simultaneous pistons on honey or slime, and interpolated 26.2 and 26.3 cases.

Known open from this lane: fy_powder_snow diverges on movementSpeed for 3 ticks on 1.17 to 1.21.4, because the client adds its frost modifier at the end of aiStep and the engine only has the folded value the server syncs.

## Projectile entity hits: arrows and tridents

Commits: lane/projectile, merged at the tip below.

- B195: Arrows and tridents are done: ordered targets, owner, pierce, deflection, loyalty, structured hits; they match vanilla on 26.3, 1.21.11, 1.21.6, 1.21.1, 1.20.4, 1.13.2 and 1.8.8. Left: entity hits for snowball, egg, pearl, potion, fireballs, wind charge, llama spit, shulker bullet, firework and fishing hook.
- B199: Arrow and trident margins, search box, ties and containment are done per release, recorded on both sides of 1.21.6 and 1.21.11. Left: the other families' queries.
- B200: Effective target boxes and hit faces go through N-API; slab, fence and no-collision rays are recorded. Left: multipart targets, which the caller lists as parts today.

The replay has a projectile path: node replay/cli.ts (in the recorder package) --projectiles --version <v> --case "physrec/pj_*". Not recorded: real hurts on a player (the recorder's player is invulnerable), a projectile's first tick, 1.20.3 and 1.20.4 tag deflection, and same-vehicle exclusion.

## Core, math and world lane

Commits: 8fa402a b88f658 16b82c8 003183a f13122c .

- B066: one release-aware Vec3.normalized and length_in replace four copies, checked against real-jar Vec3 on 1.16.1, 1.21.11 and 26.3.
- B068: Aabb::ordered is Java's constructor and the box transforms use it, checked against the real AABB on three jars.
- B075: the world height updates independently of the border, with release and dimension fallbacks; prismarine section presence is synced.
- B089: pushthrough ends at 1.17 inside the Rust helper; entity ticks skip a sleeper's own push (Player from 1.9, LivingEntity from 1.14). No recording covers the sleeper push.
- B059: tick_count wraps as a Java int. Left: the full inventory of counters.
- B067: Floor and ceil wrap until 1.21.11 and saturate from 26.1, in gated functions production code does not call yet; behaviour differs only outside the int range. Left: the call-site inventory.
- B064: Primitives proven by the Java oracle. Left: a recorded call site and a reassociation regression.
- B065: Primitives proven by the Java oracle. Left: WASM and very large inputs.
- B069: Primitives proven by the Java oracle. Left: selected faces and pre-1.13 rays, for which no jar exists.
- B071: Primitives proven by the Java oracle. Left: flags and support block from Java.

New test tool: scripts/java-oracle/Oracle.java with native/fixtures/java-oracle-*.txt and native/src/oracle_tests.rs runs the real AABB, Vec3, Mth and Shapes classes from the jars and compares them bit for bit with Rust.

## Minecarts

Commits: a2f8051 423e5a4 4d80a4f .

- B185: native/src/minecart.rs has the typed state and tick entry points, the improved behaviour is selected by an explicit flag and rejected before 1.21.2, and it reaches N-API, TypeScript and the adapter with diagnostics.
- B189: Movement is done: rails, curves, slopes, powered, halting and activator rails, drag, off-rail, water, collisions, cart-to-cart pushes, furnace, chest and hopper drag, the improved path and client interpolation. The old path matches vanilla on 26.3, 1.21.2, 1.21.1, 1.20.4, 1.13.2 and 1.8.8, the improved path on 1.21.2 and 26.3. Left: TNT fuse and explosion, hopper pickup, command blocks, fire and lava damage, portals, fall distance off the rail, carts on lava, bubble columns, honey or slime, and adapter packet wiring. Gates at 1.9, 1.10, 1.14, 1.16, 1.17, 1.21.5 and 26.2 rest on source reading until recorded.
- B027: Old and improved minecart paths are separate, selected by the flag, and tested on the same 1.21.2 release.

mx_* cases run on their own from 1.21.2: record.ts --features minecart_improvements --filter physrec/mx_*. Older recordings have no minecart object, so their carts are not compared until re-recorded.

## Recorder and replay follow-ups

Commits: 58e1d5c 20d0591 5e65de8 ea6f98f 0119ff2 044fab9 d8c4183 163ace0 b32f396 .

- B225: the known-failures ledger has 0 entries, and native.test.ts and cli.ts keep the signature and passes-now checks; the whole-corpus result comes with the final replay.
- B226: runner/outcomes.ts checks a run's results against its selection, and a success needs its finished recording.
- B228: before 1.13 a boat entity took any wood, and cherry_boat passed on 1.19.4 where it is an experiment; both refused now.
- B230: the contact probe already scans the cell below; g_stand_on_fence and g_stand_on_cobblestone_wall fail without it.
- B305: Campfire and freeze hits are compared with srv.health. Left: an elytra start through the adapter's own decision.
- B227: A launch for the wrong release, a missing java or classpath entry, and a stale --no-build fail before capture. Left: Unimined is not pinned, there is no build provenance, and intermediate adapter folders are not covered.
- B229: seed.test.ts has 13 hand-worked conversions and found no bug. Left: pushes and rockets across rows, and mount release across consecutive rows.

Also fixed: Vec3.add's 0.0 clears a negative zero on a boat from 1.14 and a floating mount from 1.21.11, and PREPARE takes the client off its vehicle first, which ends the 1.13.2 leak between boat cases.

## Core lane, second batch

Commits: 723b850 a9b05d7 9d8ad6e 2ba6f3e 995813a a72c4a1 bac84b6 .

- B061: a projectile discards itself 64 below the level's bottom, recorded on 26.3 and 1.8.8; vertical limits are lookups, not walls, and the per-path record is in the ticket's lane notes. Boats and minecarts have no discard output yet.
- B063: stale note closed: passenger reaches the engine, and the lifecycle phases are tested.
- B087: no bug; a table test across 7 releases pins is_in_wall.
- B102: a piston's landing sets the fall callback, with the same gate as the player's own move. No recording shows a player landing by piston.
- B060: only the client sinks over an unloaded column, and only before 1.14; initial-load gating stays with the caller.
- B121: closed by the B060 source trace; bo_ and bw_ already cover the border.
- B059: Projectile tick counts wrap as a Java int and the counter inventory is on SimulationState. Left: teleport, simultaneous-expiry and skipped-tick tests, and boat and ridden counters beyond their listing.
- B067: Sites with a release in scope use the gated floor and ceil. Left deliberately: block_position, block_traversal and the public shape queries, which have no release and differ only beyond 2^31 blocks, outside any reachable world. Left: the round, clamp and epsilon inventory.

## Moving entities and mob travel

Commits: cc206af ca3a0bc cdaa541 d599332 ad0d641 a18978b .

- B201: TNT, falling block, item and orb ticks with authority and events, through N-API and the adapter; physrec/me_* are exact on the server view on 26.3, 1.21.1, 1.20.4, 1.13.2 and 1.8.8. Merges, pick-ups and burns stay the server's; orbs from 1.21.5 have no client compare.
- B202: Travel of squid, fish, guardian, ghast, phantom, allay and vex from supplied control and motion; physrec/me_mob_* are exact on the same releases. Left: the dragon's flight and multipart boxes, and mobs with no override of their own (bee, parrot, blaze).

node replay/cli.ts (in the recorder package) --moving and --moving-client replay these; the server view is automatic for physrec/me_*. The recorder's mod gained MobTravelMixin, and record.ts passes --add-opens=java.base/java.util.

## API surface and reliability

Commits: d0ed44a ddb47c3 8591914 4720488 dffbe71 f8bc3e7 4a33804 .

- B205: real FROM_BOT and PlayerState tested on 8 releases and from the packed copy.
- B207: createBotPhysics gives context, installTicks and dispose. installTicks ran against a stand-in engine, not a live server.
- B211: packed, object and batch input are validated with atomic failure, and errors name the field.
- B213: declarations and layouts are checked against the addon in verify-native-package; wasm loaders are not covered.
- B217: already satisfied, proved by test: clone, failed tick and apply leave the bot's nested state alone.
- B253: world sync detaches its listeners, with dispose, engine ownership and bot end handling.
- B259: no panic or hang on extreme input on the player and entity paths. Boat, horse, projectile and minecart entry points were not probed, and nothing was fuzzed.
- B261: Send and Sync assertions and worker tests on native targets; wasm is not verified.
- B212: Tests are in test/native-workers.test.cjs and setState resets. Left: the written contract.
- B206: setState resets reports. Left: the constructor contract.

New refusals: a move or query over 2^26 cells, a position over 2^30 or infinite, over 65536 rockets, an adapter sync over 2^22 cells, and an input array that is not exactly 29 long. setBoundingBox and finishTick throw on bad input. A riptide level over 255 is clamped in the adapter, as vanilla does.

## Frost speed, packet decoding and fluid coverage

Commits: dd58e8a 2d3adfc ceb2978 c051d55 4655890 af37a5f d0ed44a .

- B137: Water levels, water under water, falling columns and near-limit height matched by wa_ on 14 releases, no bug found. Left: chunk-edge cases beyond cw_.
- B139: Blocks that hold water matched by wh_ from 1.13, no bug found. Left: toggling beyond wl_.
- B141: Depth Strider with Dolphin's Grace, other effects and the efficiency and gravity attributes matched by dg_, no bug found.
- B142: Effects, sprint and attributes in lava matched by ld_, no bug found.
- B145: Ladders, vines and trapdoor ladders in a current matched by cc_, no bug found.
- B155: Ice crossings and friction under carpet, snow, slab and trapdoor matched by ic_, no bug found. Left: frosted ice and boats.
- B159: Leather immunity, frost among other modifiers and fire destroying the snow matched by pz_, and the frost modifier is exact. Left: cauldrons, the two-tick fixtures and entity-kind variations.

The replay's packet decoding looked for the fork at a path that doesn't exist beside a worktree and fell back to the published codec: it now resolves the fork from node_modules, and recordings.test.ts passes on 1.21.9, 26.2 and 26.3. movementSpeedAttribute makes the client's frost modifier exact, which clears the fy_powder_snow divergence listed above. Left: the mineflayer adapter doesn't send it, so a live bot on 1.17 to 1.21.4 still lags by a tick. FENCE_LIKE moved to its own tag bit, where it had collided with minecart.rs's TAG_RAIL.

## Registries, tables and source identity

Commits: ca37f72 083f3d4 802268a b56a6e2 .

- B020: generated/release-manifest.json joins protocol, data version, mappings, source identity and table hash for all 50 releases; a disagreement stops the join.
- B023: only registered releases enter the version manifest; prerelease directories cannot.
- B031: registries equal the client dumps for 43 of 44 modern releases, read from exact pinned sources. 26.1.2 has names and state counts but no classes, properties or shapes for 107 blocks.
- B032: the ten spurious IDs are gone from 26.2 and 26.3, proved by tests, with block and item names kept apart.
- B036: the LocalPlayer path and aiStep are in all 44 manifests. A wider decision-method inventory remains.
- B037: sources are admitted only by embedded identity, commits are pinned, 20w45a is rejected and unlabelled branches are refused.
- B039: recorded paths are portable and resolved against the manifest. Linux and macOS were only tested through path fixtures.
- B033: State definitions are read through inheritance, filters and accessors, and client state counts are published. Left: 4 dynamic classes (GlowLichenBlock, SculkVeinBlock, MultifaceBlock, ChiseledBookShelfBlock) and the 15 releases without Mojang-named exact source; state_index refuses those blocks.
- B034: Oak door and red bed tags hold, the 1.20.3 copper contamination is gone, and the empty 26.1 and 26.1.1 tags are filled. Left: the disposition of the audit's 808 differences.

The block tables were missing 6 blocks from 1.20.3 and up to 113 per release from 1.21.2 to 26.1.2, with wrong state counts; generated/runtime-registries.json is a new oracle built from the running-client dumps for all 44 modern releases. Follow-ups: verify-version-coverage.cjs needs to work without mc-decomp, and 26.1.2 needs class data from an exact source.

## Vehicles, adapter layer and tolerances

Commits: 9c7d189 b2c161f 3b44ead 9d8495d a18a70a 5da47af cc75a7c e77eeff ec07453 3037042 cb770cc .

- B179: the client neither damages the glider nor clears the flag; f_elytra_breaks_* show the server ending the glide at fallFlyTicks 20 and 40, matched on 1.9.4, 1.21.1, 1.21.4 and 26.3.
- B186: A ridden boat and mount now meet entities: a boat everything pushable, and every entity up to 1.10, a mount boats and shulkers, matched on 6 releases. The caller must fill nearbyEntities, so a live bot's boat still ignores entities until the plugin does. Left: land friction ledger, passenger placement traces, D-108.
- B174: cli.ts --adapter compares the decisions the replay seeds; 26.3, 1.14.4 and 1.8.8 are clean. Left: riding, and 1.15 to 1.15.2 traces, which hung while recording here.
- B169: The replay now feeds piston pushes, and pm_, ph_ and n_piston match 32 of 32 on 26.3. Earlier pm_ and ph_ claims covered moving-block shapes only. Left: a recording's first push isn't compared, because Entity.pistonDeltas survives between cases and isn't recorded.
- B168: Pushes at their limits matched by n_* on 4 releases, no bug. Left: team and vehicle combinations.
- B167: ce_* wall and corner tolerances matched on 4 releases, no bug. Left: slab tops, negative coordinates, step height.
- B154: No change: 9 releases still lack exact source.
- B146: Fish, squid and guardian are in moving_entity.rs. Dolphin, turtle, drowned, axolotl, tadpole and frog remain.

The pose fit meets the border from 1.14 (by shape from 1.18), bo_pose_* on five releases. parity-deviations: D-110's engine side is done; D-080 has no practical effect. Open recorder item: f_elytra_* reported 'a server task did not finish' on 1.14 to 1.16 here.

## Every projectile family, and real damage to the player

Commits: 0d45dde 62cd1b6 f912371 .

- B195: entity hits for every family with server-only decisions as explicit inputs, recorded on 17 releases with 0 divergences. Not modelled: a pearl's teleport of its owner, portals stopping a hit, and bite pulls on a hook.
- B199: every family's query, margin and owner rule is ported by release.
- B197: All families fly server-side through the replay. Left: publishing the family-by-release capability table in docs/.
- B198: Legacy ray walker, gravity float and double, and -0 fixes are recorded through water and into walls. Left: first-tick bubble columns and border crossings.
- B200: Outlines now reach the world. Left: multipart targets, which the caller still lists as parts.

pj_mortal_* record real hits on the player: health falls and the replay compares them both ways. pj_mortal_own_arrow differs between runs before 1.21.1 because LivingEntity.hurt draws knockback from Math.random when the attacker stands at the target's position, so it is server-timed there. Unit-tested only: a pearl passing its owner, a dragon fireball on its owner, a fireball's owner immunity, 1.20.5 and 1.20.6 deflection power, the wind charge height burst, hooks in water. A fireball with no power has a NaN box on a pre-1.13 client, which the replay now leaves out.

## Floors, ceilings, rounding and tolerances

Commits: 571f66e 671014d 753390c 018e281 f5941ad  and the ten before them.

- B067: Every floor and ceil is converted to the release's form, with collisionShapes, outlineShapes and interactionShapes taking an optional version; the round and clamp inventory is done. Left: grid_bits before 1.17 needs abs(f - floor(f)) < 1e-7 (M1), which no block shape reaches, and 1.13.2 legacy_colliders' L-shaped query (M3), plus D3 and D4 of the audit.

Found on the way: arrow damage has three forms by release, Mth.frac is Math.floor to 1.13.2, ItemEntity's cell test compares ints, clampToBounds returned non-int coordinates, round_i32 was wrong at 0.49999999999999994, and a minecart clamp panicked on NaN. The 100k-tick timing shows no slowdown. New bug to ticket: on_position lacks getOnPos's tag branch (26.3 Entity.java:1115), inside the int range, so a player landing on a fence reads a different block.

## API contracts and robustness of every entry point

Commits: 0b65d3c 4d5cad2 e1fbc65 55c2f20 3b28748 7e91931 16d846f .

- B206: ReleaseEngine and the ownership contract are in lib.rs; a NativePlayer is tied to its world until setState.
- B208: generated per-release boat and ridden mob classes, registries, factories and canRide; version is immutable.
- B212: contract written in lib.rs, with tests for no retained input and world reuse.
- B216: kinds are gated by first release, with a mob family and predictProjectile; dolphin, turtle, axolotl, tadpole, frog and drowned stay refused as unsupported (B146).
- B209: An unknown boat status and a non-horse in simulateHorseTick are refused. Left: the full field map and packed layout versioning.

B259's gap is closed too: boat, ridden mob, projectile, arrow, trident, moving entity, mob travel and minecart entry points are probed, which found real hangs on huge moves, kept boxes, step heights and rays; ticks counts stay unbounded (B260). Open decision: from 1.21.9 an LpVec3 velocity can legally exceed the 2^26-cell move limit, which is refused.

## Recorder budget, dead clients and the first piston push

Commits: ae59e72 c6a093a ef56725 48d2aa5 .

- B169: the first push is compared; pm_, ph_, n_piston and pw_ match on 26.3, 1.20.4, 1.13.2, 1.11.2, 1.10.2, 1.9.4 and 1.8.8, two identical recordings each.
- B168: Simultaneous pushers (pm_opposed_*, pm_twin_*) now match on seven releases. Left: team and vehicle combinations.
- B162: Moving-block shapes advance within a tick and record in tick order. Left: the 1.9 to 1.10 head outline stays approximate.

The 'server task did not finish' failures on 1.14 to 1.17 were a 60 s budget against 69 to 87 s of first-use chunk lighting, reproduced on an idle machine: the budget now applies to a task that stops moving, and a dead client's remaining cases go to a new client, up to 5 restarts. The piston work also fixed moving-block tick order, pistons advancing within a tick, zero-depth blocks on 1.8, and fed outlines on 1.9 to 1.10. All piston cases need recording again on every release. A long adapter replay must yield between recordings: a WeakRef's target lives until the job ends.

## Live-bot frost, the adapter's glide start and more surfaces

Commits: lane/fluids to c1a11f4 899de42 207d3e7 .

- B305: campfire and freeze hits are compared, and the adapter's glide starts and refusals match recorded activations: --adapter on f_elytra_* shows 0 unexpected on 17 releases. Before 1.15 the server's flag starts the glide, so the adapter decides nothing there.
- B161: pa_* adds doors, beds, gates, trapdoors and top stairs (93 cases), no bug; 1.8's anvil collides as a full cube, fixed. Left: remaining state and neighbour families, step-ups, epsilon edges.
- B149: The mapper's water plants match source, and a waterlogged copper grate holds a falling source, fixed. Left: generated fluid tables and tags.
- B148: fp_ adds 21 flow blockers and fs_ more faces, no bug. Left: eye height under sturdy ceilings, which needs a small-scale case on 26.3.
- B153: Falling water beside snow layers matched by fs_. Left: comparing collision, outline and support shapes per channel.
- B164: su_ separates collision from suffocation and fp_ from flow, no bug. Left: whole tag sets against pinned data.

A live bot now applies the client's frost modifier on 1.17 to 1.21.4: NativePlayer keeps the attribute and reports FROST, tested with a stub bot, not a live server.

## Source inputs and generation

Commits: 30b8100, 72d66ed, bec4f21, f5a7c42, 572f3fb, 02c3d69.

- B033: all 44 releases covered, dynamic classes included.
- B024: the 1.8.8 alias is proved: 21 of 2541 classes differ and every one was reviewed.
- B035: declarations are compared structurally; cross-decompiler pairs are not available, so that check is the proof here.
- B021: Source pinning done; release-admission script still to finish.
- B022: scripts/release-admission.cjs added; not yet wired into a run.
- B029: Generation inputs are pinned; the remaining items stay open.
- B038: Partly covered by the generation work; left open.
- B040: Partly covered by the generation work; left open.
- B048: Partly covered by the generation work; left open.
- B049: Partly covered by the generation work; left open.
- B050: The workflow exists but has never been run.
- B154: Friction checks 50 of 50 releases. Left: boundary and boat tests.

verify-version-coverage.cjs works, 44 of 44 with --fresh. 26.1.2 was filled from the release jar through Vineflower with scripts/materialize-sources.cjs (clientJar sha1 4e618f09), 107 of 107. 14 other releases were decompiled from unimined jars pinned by sha256. The rc branch shows 0 differences in names, classes and state definitions, as a cross-check only. E:/pr/minecraft-data-generator was not used. Source cache: E:/mc-physics-demo-wt/source-cache.

## Minecart side effects

Commits: 72d66ed, bec4f21, f5a7c42, 572f3fb, 02c3d69, 2bd6410, ae59e72, 506bc2d, c6a093a, ef56725, 48d2aa5, 899de42.

- B189: Side effects, damage, portals, below-world, fall distance and the bot's MinecartTracker are in; replay compares the old path on 14 releases and the improved one on 1.21.2, 1.21.5, 26.2, 26.3. Left: hurt_minecart is unit-only, blast on nearby entities, hopper taking from a container, spawner cart, end gateway, netherExit 1.11 to 1.13, 1.16.2 gates, fall distance at 1.19.
- B027: The 1.21.2 cart path is separate and replayed with the improved rules; the left items are under B189.

Furnace push break found on 1.15.2 and fixed. 26.2 no longer shows a window while recording. Live bots still need three edits in the mineflayer plugin to use MinecartTracker.

## Landing support and collision grids

Commits: 5e88707, 9972255, 617af74, 7d09cfe.

- B067: M1 (grid test Floor before 1.17, Round from it) and M3 (1.13.2 L-shaped query and mergers) fixed against real jars: Oracle113 over 3000 cases went from 480 query and 269 movement differences to 0. D3 is double in 1.21.2 to 26.3 by javap. D4's fallback matches the jar on all 338 distinct 26.3 shapes and BlockMapper on all 35723 states. Only the acceptance review is left.
- B146: The moves lane did not add kinds. The five listed kinds are on the entities lane; tadpole to be confirmed there.

on_position reads the world from 1.20 (D-188, 26.3 Entity.java:1115) with a new WALL_OR_GATE tag bit 32; recorded fd_on_* cases need 1.17+ and match on 26.3, 1.20 and 1.19.4. 1.13.2 is about 60% slower per tick from the M3 port (about 4 microseconds), other releases unchanged. Oracle113 does not run step-up, entity colliders or world-border cells.

## Dragon, mobs and aquatic kinds

Commits: 60640d1, c1da521, fa9b357, 164063c.

- B202: Dragon flight, parts and wing pushes, and every mob in the list with block effects and fall distance, replay exact on 1.8.8, 1.13.2, 1.20.4, 1.21.1 and 26.3. Mob levitation, slow falling, dolphin's grace, gravity and water efficiency are in travelFrom with 4 new cases. Left: me_mob_phantom_circles is intermittently non-deterministic on 1.13.2.
- B146: Dolphin, turtle, drowned, axolotl, tadpole and frog travel are in. me_mob_tadpole_swims still to verify across releases in the final pass.

Ridden fall order had the same bug and is fixed for mounts; a unit test covers it, no recorded ridden case exercises it. SensorTimeMixin applies only where the static RANDOM exists; camel on 1.20.4 and happy ghast and nautilus on 1.21.11 show 0 changed rows, other AI-on brain mobs were not sampled. me_mob_* need re-recording on all releases.

## Rain, stabs, client fall damage, registries

Commits: c8af7bf, 7d5e7d1, 472224a, f821066, 279a168, 78e8d45, 9ae5be1, 9f61ba0.

- B304: fallDamage is a prediction on client states, contract in src/native.ts. The client also runs causeFallDamage; only its hurt does nothing (26.3 LivingEntity.java:1737, Entity.java:1970). 1.21.4 skips a player's landing on both sides of Entity.move: a server state now reports it through handleFall, and 1.21.2 to 1.21.4 server states run the check for non-local entities. fd_* 38 of 38 on 1.21.4 fresh, and on seven neighbours. Explosion and wind-charge impulse context is not modelled, and the contract says so.
- B303: The ticket's premise was half wrong: stabs recoil on the server only, the client only sends STAB (26.3 MultiPlayerGameMode.java:513). stabHit on server states from 1.21.11: 0.6 once to 26.1.2, twice from 26.2 when the weapon's knockback is above 0. m_click_spear_stab_cow cases match on 1.21.11, 26.1, 26.2, 26.3; the multiplier itself rests on source since srv.vel is 0 on x and z.
- B297: src/mineflayer/rain.ts with per-release rules, recorded q_rain_* on six releases. Limits: BiomeManager's seeded zoom is not ported, so positions near a biome edge can read the wrong biome; seaLevel is not read from the packet.
- B302: Data aliases are explicit and verified: 1.19.1 to 1.19.2, 1.21.2 to 1.21.3, 1.21.7 to 1.21.8, 1.21.10 to 1.21.9, byte-identical data generator trees. Left: a full replay of these four, which the final pass does.
- B308: PistonStructureResolver is ported to the adapter with 7 tests. Left: isPushable is by name; an exact PushReaction table needs a per-release generator, and no recording drives the tracker.

The vanilla data generator tree comparison covers 4473, 5949, 6113 and 6319 files. generate-data-aliases.cjs needs a JDK and stays out of generate-all; it has --check.

## Recorder repairs before the final pass

Commits: d62c938, 8496fee, cfb71bb.

- B309: Repaired: web cases stand astride the face, the slab goes in at tick 1, glass no longer blocks the wet ladder lane, b_flow_idle_drift starts in the current. Each has a check that fails when it stops doing what it says, and checks take @range. gl_onto_slime_sneaking's correction is vanilla's from 1.20.5 and allowed only from there; the line causing it was not found. The 1.13.2 v_boat_bubble_up report did not reproduce and is unexplained.
- B231: viewDistance removed: it was never applied, a case asking for 19 ran at 4. area() refuses it and the header keeps the actual distance, 3 before 1.15 and 4 from it. Every scenario hash changes.
- B227: Unimined pinned to 1.3.16-20260511.125954-9 with SHA-256 in gradle/verification-metadata.xml, tested with a wrong hash. Every header carries provenance from runner/provenance.ts; --no-build now compares content across the adapter chain. Identity rebuilt identically on 1.20.4 only.
- B014: runner/session.ts split out so tests drive real fake clients; every process and output failure in the ticket is covered. Replay reports pass, known, unsupported and untested, and a selection with nothing compared fails.

The whole corpus is stale after this lane until the final pass records it. The 1.20.3 and 1.20.4 explosion sound misreads in protocol data; the motion before it reads true, and recordings.test.ts exempts that packet in that range. Merged with v_boat_into_minecart failing on fresh recordings of 1.13.2 and 1.20.4: an engine gap, since the cart does not take a boat's push, and not from this lane.

## Boat pushes on carts

Commits: 85eea4f and the 26.3 window hook.

- B189: A boat's tick now pushes the carts it reaches (Boat.tick to Boat.push to the generic Entity.push, 1.9.4 BoatEntity.java:277, 26.3 AbstractBoat.java:259), active from 1.9, with level order from entity ids. v_boat_into_minecart replays on 1.8.8, 1.9.4, 1.13.2, 1.20.4, 26.3. Left beyond the earlier list: a boat ticking ahead of a cart and the mob order have no recording, team rules are not modelled, and armor stand and bat are treated as ordinary pushers.

26.2's window redirect is inherited by 26.3, which has no such call, and killed its clients: the redirect is now optional. 26.3 is re-recorded in full after this.

## Pass triage: legacy findings, the nether cart, the own arrow

Commits: 2d190bc, 76c3ed7.


Recorder repairs from the first full pass. CompatMove.collisionBoxes read outline boxes before 1.13, so g_stand_on_fence and g_stand_on_cobblestone_wall failed on 1.8.8 to 1.12.2 since they were added; it now reads the boxes the block appends. 1.8.8 ran the mood sound on the server, parting the level's random stream per run; CompatMove.seed pins its countdown. The cart of mc_nether_portal stayed in the nether and came back in m_dimension; clearOtherLevels removes it. An arrow landing exactly on its shooter took Math.random in hurt or knockback; KnockbackRandomMixin draws from the hurt entity's seeded random. me_falling_* errored on 1.11.2 and 1.12.2 saving a blockless falling block. gl_onto_slime_sneaking is vanilla on 1.11.2 and from 1.20.5: the sneak ledge check's reach is shorter than the gap to the floor. rg_fence_neg_315_jump on 1.8.8 never differed locally, so its cause is not confirmed. To re-record: all of 1.8.8, and the cases stale.ts lists elsewhere.

## Player lane: scale timing, minecart kind, isUnderWater, eyes after the move, kept box

Commits: 7101fbf, 4d5a3ba.

- B106: Window edges, food 6/7, blindness, item use, wall collisions, swim and flight, release and re-press compared on the sprint flag across eight families. Only the default sprint window was recorded.
- B114: gr_ matrix and e_attr_gravity pass through the adapter on the 8 pass releases and gr_ on 1.21.4. 1.20.5-1.21.3 follow from the final pass.
- B097: piston and ridden paths not rerun; fd_, lf_, wa_, hw_ clean.
- B098: full fd_ matrix only on 1.21.8+; coverage report unwritten.
- B099: nothing in src calls refreshDimensions on a synced pose, so a live 1.14-1.16 bot drifts by ulps; passenger and spectator pose exceptions unrecorded.
- B103: per-release matrix incomplete.
- B104: callers need switching to NativePhysics.cloneState; ridden ticks not asked.
- B105: no recorded modifier mixture exposing rounding order.
- B107: passenger context not compared.
- B110: lava and ladder overlap checked on 1.21.8+ only.
- B111: 1.17.1 feet-water gate unrecorded.
- B112: spectator entry, dismount and changing-block cases missing.
- B113: 1.18.2 arithmetic unverified; spin and rocket ticks excluded.
- B115: resistance and the knockback calculation unmodelled; impulses on sleeping or dead entities unrecorded.
- B116: no analog input magnitudes.
- B119: a recorded attackHit needs a remote player, which needs a mod change.
- B120: eyes at the fluid surface and unloaded adjacent columns not recorded.

Five bugs: a bot near a minecart threw on its tick (entity kind 8 refused); a synced scale moved the box a tick early; isUnderWater was wrongly ANDed with isInWater from 1.16; from 26.1 eyes refresh after a move into water; on 1.14-1.16 the player's own pose change did not refit the kept box. New: NativePhysics.cloneState (no callers yet), replay --adapter --motion, physrec/ie_* 17 cases to record on every release.

## Cart replay: the player's server tick, a rider getting off, coal

Commits: 56053a9, da0a0d0.


Three minecart cases had failed on every release since they were added and the subsets had missed them. c_ride_minecart_dismount: a rider that gets off is in no view after the tick, so the replay gave it stale input, and its push comes at its own turn after leaving the cart (getsOff). m_c_use_furnace_minecart_coal: the coal of the tick's packets (MinecartFurnace.addFuel) is seeded. v_dismount_minecart_pocket: a player's server tick travels by its velocity and pushes from there before the connection snaps it back (26.3 ServerGamePacketListenerImpl.java:327-328, from 1.9), so the views never show the place it pushes from. Checked: the 17 minecart cases on 26.3, 26.2, 1.21.11, 1.21.5, 1.21.1 and 84 mc_ cases on 26.2 and 1.21.5. A wall stopping that travel is not modelled.

## Moves lane: Entity.move, shape walks and clips against the real jars

Commits: abf45f6, 593d64d, 0e8fc52.

- B064: Bit-exact vector sequences against the jar, HotSpot 25.0.3 named in the test; getInputVector is a real call site.
- B069: Face, hit and order for modern and pre-1.13 clips from jars; pj_* is the scenario.
- B071: Adjacent floats around 1e-7 with displacement, flags and support from the real Entity.move, plus mv_top_*.
- B072: Coverage table in the oracle_tests.rs header; controls are oracle lines.
- B081: mv_zy_* plus the owner and non-owner oracle on 1.21.3-1.21.5.
- B084: Cause matrix; the oracle has all 5 causes. SHULKER, SHULKER_BOX and PLAYER have no engine entry point and are scoped out.
- B166: Entity.move matches on 1.18.2-26.3 through the jar; below that mv_* and rg_* recordings. Older oracle runs stay explicitly open as the ticket allows: static tags are unbound in those jars.
- B167: mv_top_*, mv_stepeq_* and the -0.08 threshold on 26.2 and 26.3.
- B171: Empty and merged shapes, grid lines and all faces against jars.
- B172: mv_mix_*: 20 seeds, both quadrants, a seeded palette.
- B065: Linux, macOS and wasip1-threads did not run; the fixture is the math vectors through a separate crate, not the engine under WASM.
- B070: IndirectMerger not ported in VoxelShape::union, which only tests call; near-coincident lines within 1e-7 skipped.
- B083: Per-release phase mapping unwritten; the tick's move and move_entity remain two implementations.
- B085: Packed output has no verticalCollisionBelow; packed round trip with a next-tick check not added.
- B086: Honey carry, moving platforms and teleport reseed not exercised.
- B161: Legacy neighbour shapes not run; bamboo and dripstone grids when already inside; rotated anvils' extra Y step candidates; dynamic neighbour invalidation.

Seven bugs fixed: a noPhysics piston move clears flags from 1.21.9; Vec3.equals in Entity.move(PISTON) tells -0.0 from 0.0; from 1.21.2 the landing stop and slime bounce do not run on a client that does not own the entity; the slime block ended a fall outside checkFallDamage; multi-box shapes walk the grid; restitution goes through setDeltaMovement; AABB.clip over several boxes shares one fraction. New: OracleMove (9000 cases on each of 30 jars, bit-exact), OracleGeometry, OracleLegacyClip, OracleStates, physrec/mv_* up to 401 cases per release, to record on every release. Found: minecraft-data's 1.18-1.18.2 collision tables hold another decomposition (1153 states the same solid in different boxes). pj_arrow_enderman fails on the base build too: not this lane.

## Release contracts, provenance and audits

No release is certified by any of this. `docs/parity-contract.md` is the entry point.

- B001, B008, B022: `scripts/parity-contract.cjs` and `scripts/release-admission.cjs`: source, data, implementation and conformance, each computed. `requireParity` refuses at construction.
- B002, B021: `docs/source-records.md`, `scripts/source-admission.cjs`. 1.9.4 to 1.13.2 are visibly blocked.
- B004: `docs/authority.md`, `test/authority-boundaries.test.cjs`.
- B006: every entry naming a release goes through one exact check; `entityTravelSupport`, the world border, a descriptor's version and block-state versions used to take a release that is not registered.
- B009, B025: `scripts/release-inventory.cjs`, `scripts/generate-version-exports.cjs`.
- B015: `docs/state-bridge.md`, `test/state-bridge.test.cjs`.
- B017: `docs/validation-records.md`. Moving the 1.21.5 double-form gate passed all 41 `physrec/fd_*` replays; a fixture at 3.9999995 now fails on it.
- B028: `docs/release-boundaries.md`.
- B030: `docs/parity-contract.md`, `test/legacy-behaviour.test.cjs`.
- Left open: B003 (no native phase trace, B220), B005 (non-player round trips), B007 and B018 (ungated functions are not listed), B011 (M29), B012 (per-finding traces), B013 and B026 (no recordings before 1.20.3), B016, B019 (an absent fire draw reads as 1), B029 (26.1.1 and 26.1.2 cannot be replayed).
## moves lane, second batch

Commits: 4ce173a 47bf453 017358a 4bc0910 51b3758 7d450a2.

- B056: docs/numeric-contract.md and test/numeric-limits.test.cjs: object and packed paths refuse the same inputs, name the field, leave the state unchanged. NaN starts match the real aiStep by bits on 1.21.10 and 1.21.11. Remaining deviation documented: before 1.21.11 Java walks an infinite motion to the border, the engine refuses it.
- B062: docs/portals-and-teleports.md and test/lifecycle.test.cjs; 26.3 replays clean for m_rotate_teleport, cu_tp_away_and_back, mc_nether_portal, mc_end_portal, m_dimension.
- B118: Exhaustion is client-only by decision, docs/determinism.md; test covers food 6/7, mayFly, 40 sprinting ticks, an update between ticks, a fork. Server exhaustion stays under B117.
- B123: No generic EntityState class: the API is cloneState, now deep; test covers nested edits, controls, absent attributes.
- B215: No older calling convention exists, so no adapter: docs/state-contract.md. Unknown releases and other packed layouts are refused; simulateTick is a documented default entry, not a migration fallback (default gates stay under B214).
- B051: Object and packed comparison across effect expiry and passenger transitions missing.
- B052: Matrix has no mutation-phase column; removed, dead, passenger, noPhysics, authority combinations untested.
- B053: Engine does not run under WASM; math and friction vectors match on wasm32-wasip1, walk lines native only.
- B054: No Java fixture for jump rejection or pose refresh.
- B055: Same WASM limit as B053; Linux and macOS not run.
- B057: Needed a phase trace; recorder lane now adds it, compare by phase.
- B058: Consumer list searched the engine, not vanilla; no cosmetic-event test.
- B059: Death transition untested; boat and ridden counters only listed.
- B067: Review found gaps: Mth.equal (1.0E-5F) and the traversal 1.0E-5F squared test have no oracle lines.
- B073: No per-release adapter comparison; neighbour shapes open from B161.
- B074: Shulker box opening geometry not implemented, needs a block-event input.
- B076: Interaction channel and support queries not in the boundary fixtures.
- B082: No test that a dimension or rotation change leaves unrelated state alone.
- B117: Ownership decided and client side tested; server-mode exhaustion not modelled.
- B124: Matrix covers players only; generic living and ridden consumers missing; knockback_resistance not audited.
- B125: Table covers 20 jars 1.16.1 to 26.3; 1.14 and 1.15 failed to compile; 1.20.5 absent.
- B126: Scale extremes, attack knockback, object versus packed not tested.
- B209: Field map covers the player only.
- B210: Adapter agreement and contact damage missing from the agreement test.
- B214: No gates removed.
- B218: Player only; boat, ridden, cart, projectile, moving-entity states have no checkpoint.

Fixed on the way: cloneState shared nested state; N-API truncated fractional cells and wrapped 2^33 at world writes, now refused on writes; packed output lacked verticalCollisionBelow, LAYOUT_VERSION 2.

## recorder lane, second batch

Commits: 85e825a 14ad9de be8ee00 112c0a0 1128aac 9cb6388 83d5431 84f8a5a 8a621b5 b782845 3d221bb, merge 6282efc.

- B220: native/src/phase.rs and simulateVersionedTickTraced; traced and untraced ticks bit-identical, tests for compensating errors at the right phase, and missing, duplicate and no-travel phases.
- B239: Same phase comparison. Phases ran on 1.20.3 and later; below that only 8 local cases.
- B223: --continuous carries predicted fields for player, mount, boat: 596 recordings, 32,386 ticks, seeded only on the first tick. Decisions stay vanilla's and each result says so.
- B247: A 1e-6 motion nudge shows on 1 tick stepwise and 43 continuous; a lost web slowdown is caught only by continuous.
- B229: boundaries.test.ts: row shifts of rockets, mount release, paddles, no-server-view rows, first and last rows, before and after player entities.
- B233: docs/parity-dispositions.json, 69 entries with counts tested from the file. D-212 needs a new ticket; D-163, D-170, D-171, D-201 are source-argument only.
- B234: conformance-cli.ts reports evidence categories separately; each blocker tested. Not wired into CI.
- B250: Absent recordings cannot pass; a reseeded run demotes to stepwise; seeded divergence and obsolete known failure tested.
- B242: OracleEdges.java over the real 26.3 Mth: 339 lines bit for bit; tick edges on 1.8.8 to 26.3 in numeric_tests.rs.
- B246: test/chunk-lifecycle.test.cjs, 10 pass.
- B248: random-run.ts: seed 7, 16 cases twice, 0 nondeterministic; planted 1.21.4 rules on 1.21.5 found in 9 of 10, shrunk from 7 steps to 1.
- B249: differential.ts: 4 gates, each with a changed case, a control and older rules failing.
- B251: the recorder's evidence E-001, E-002 with fixtures and checksums; evidence.ts replays them on the current build.
- B219: Needs the final pass per release.
- B235: Needs the final pass per release.
- B221: No fake-client bot replay: no C2S byte, order or exactly-once checks.
- B222: No vehicle-specific phase capture; observation radius unverified.
- B224: Gate built; no manifest pinned, needs final recordings.
- B232: World median 76.9 to 0.46 ms, replay 282 to 190 ms, RSS 482 to 406 MB. Streaming not built, no regression threshold, large fixtures not profiled.
- B236: No cell-by-cell comparison of resolved client blocks with NativeWorld.
- B237: No inventory per entity family; 39 recorded fields have no native use.
- B238: Reordered effect and passenger entries not tested.
- B240: matrix.ts names real cases; only 26.3 has family sweeps.
- B241: As B240.
- B243: Player against mount missing: a controlled mount ignores server velocity.
- B244: No release-qualified capability table.
- B245: Start-inside border clipping, water entry within a batch, per-release matrix not done.

Engine fixes: a tick with no rocket no longer turns -0.0 into 0.0; the Slow Falling, Levitation and flight fall reset moved just before travel; vertical impulses are Vec3.add from 1.14; a rescanned world with no events no longer keeps a gone column loaded. Riptide launch fed as an input. New physrec/cf_* on every release in the final pass.

## Release contracts lane

Commits: fc57e54 3d5ce08 c984ae9 67bf3cf 768484d 1c0aa75.

- B001: scripts/parity-contract.cjs: 30 interactions in 7 families, computed cell status per release; a scope change voids the claim.
- B002: docs/source-records.md, scripts/source-admission.cjs: 44 exact, 1 alias, 5 blocked; wrong identity, missing mappings, changed hashes each fail by name.
- B004: docs/authority.md, test/authority-boundaries.test.cjs.
- B006: entityTravelSupport, world border, descriptor version, pose lookup and block-state versions could accept unregistered releases; all now go through registered_release. test/exact-release.test.cjs.
- B008: scripts/release-admission.cjs: four computed dimensions, each failing independently.
- B022: requireParity refuses at construction naming what is missing; pin-conformance wired into real runs.
- B009: scripts/release-inventory.cjs, generate-version-exports.cjs.
- B025: Same-count swaps fail with both names reported.
- B015: docs/state-bridge.md, test/state-bridge.test.cjs: every field classified; packed and object agree on 10 fixtures across 5 releases.
- B017: docs/validation-records.md: deliberate bugs failed their tests on fresh addons, identities recorded.
- B021: 1.8.8 a proved alias; 1.9.4 to 1.13.2 carry an explicit blocker.
- B028: docs/release-boundaries.md, test/release-boundaries.test.cjs: both sides of each boundary match (109, 30, 46, 44 scenarios). Sensitivity by mutation only at 1.21.5.
- B030: Scope per release in docs/parity-contract.md, test/legacy-behaviour.test.cjs, unit-level.
- B003: Blocked on the phase trace, which B220 has now landed: compare intermediate phases.
- B005: Round-trip fixtures cover the player only.
- B007: Not every shared interval has source comparisons and boundary coverage.
- B011: 808 tag differences now dispositioned by B034 once merged; the audit directory is gone so before and after counts cannot be reproduced.
- B012: All 25 findings fixed and listed once; most have unit evidence only.
- B013: Traces for 1.14.4, 1.15.2, 1.16.5 need final recordings.
- B026: As B013.
- B016: Every family classified, all incomplete.
- B018: generated/shared-gates.json lists 221 gated functions; ungated shared functions and caller maps missing.
- B019: A burning ServerPlayer with no fire draw silently reads 1 (simulation.rs:2222), pinned as a known gap.
- B029: 26.1.1 and 26.1.2 cannot replay: minecraft-data lacks them and the alias is refused without proof the jars match.

A pinned player.fall does not prove every branch: moving the 1.21.5 double-form gate to 1.21.6 passed all 41 fd_* replays, only a fixture at 3.9999995 catches it. Pins go stale on any native change: re-pin after the last merge.

## Generated data lane

Commits: 25545fb 61b2ddc f8ca7a7 b38cd31 55f1aca cfd41cd 83fc1b9 f590813 e1cc509 a0cd0f8 47cfb3f.

- B034: generated/block-tags.json keeps default and experimental memberships apart; all 808 audit entries dispositioned (642 missing-data, 148 configuration, 18 wrong-identifier, 0 unresolved); Rust tables bound to the jar by hash.
- B077: Consumer table in test/block-tags.test.cjs with each tag's first release from the jars.
- B078: ShapeDump.java over 1,034,494 states on 44 releases: exact per state and channel, a fresh jar run reproduced all tables byte for byte. Six legacy releases documented unresolved.
- B079: generatedStateShape returns exact, contextual with dependsOn, or absent. A legacy release returns null, not an explicit status.
- B080: Verifier driven by client dumps; injected defects fail; 6 releases visibly unavailable.
- B153: Channels compared independently on 21 releases. 58 offset-plant states on 26.3 keep candidate bounds, their outline needs the position.
- B154: Friction equals the oracle on every release; s_straddle 4/4 on 26.3, 1.21.4, 1.20.6; v_boat_astride_ice discriminates (landFriction 0.395).
- B164: All tag sets on 45 releases, predicates on 21 modern ones. Predicates of 1.13.2 and earlier not oracle-compared.
- B038: --fresh passes 44/44 and 6/6 but has never run on a clean machine.
- B040: 18 kinds by 50 releases; impact-state write fixtures missing.
- B041: Passenger attachments and pre-1.20.5 eye heights need a live entity.
- B042: Speed and jump factors against consumers, callback-order fixtures missing.
- B043: No declaration-level map of native primitives.
- B044: Enchantment and equipment rules, attribute-to-inventory link missing.
- B045: No typed IR; not started.
- B046: No inventory of movement constants; not started.
- B047: Hash provenance only for tags, shapes, entity data, friction.
- B048: Needs Linux and macOS; worker counts 1 and 8 not run.
- B049: M30 and M31 are assertions, they produce no nonzero generation exit.
- B050: Workflow never run on GitHub; generate-all --check was cut off by the reboot.
- B152: Six legacy releases not compared.

Fixed: 1.14 to 1.15.2 leaves are sturdy to fluids; exact per-state outlines (241 blocks on 26.3 had none); waterlogged no-collision tags; hanging sign support shape; the legacy manifest silently kept stale entries; generated gzip tables are binary in .gitattributes. 1.18 to 1.18.2 collision now comes from the jar.

## Release contracts, second batch

- B003: every listed boundary replays with `--phases`, both sides matching (93, 64, 109, 44). `docs/tick-order.md`.
- B005: `scripts/state-roundtrip.cjs`, `test/state-roundtrip.test.cjs`: boat, mount, cart, projectile, arrow, trident and moving entity states.
- B011: M29 closed by B034, so the ledger has no open finding.
- B018: `scripts/shared-functions.cjs`: all 1292 native functions with callers, families and interval, the unverified marker the default. The gate list reads 288, not 221: test code had been cut from the first `#[cfg(test)]`.
- B019: a burning ServerPlayer given no draw is refused.
- B029: 26.1.1 and 26.1.2 replay on 26.1's registry through a proved alias: 8594 identical data files, 3 and 9 reviewed classes.
- Mutation: nine gates and one policy moved. Of the eight in the batch replays caught 4, unit tests 2, and nothing 2 until fixtures were added, which now fail on them. `docs/validation-records.md`.
- Left open: B007 and B016 (99 gated functions without a source finding, no recordings before 1.20.3), B012 (8 findings with no named trace, M14 with none possible), B013 and B026 (final recordings).
## Entity and vehicle lane

Commits: 49dacd1 0d29fa6 f683275 6e7d52f 4014cf3 ef3e325 2150d86.

- B088: pm_*, ph_*, n_piston*, me_*_piston, pm_ride_* match; 749/749 on 7 releases; limits in docs/piston-authority.md.
- B162: Head plate fixed; pm_* and n_piston* clean on 1.9.4 and 1.10.2.
- B163: ph_*twin*, ph_honey_beside_slime*, ph_honey_powder_snow, item and TNT piston cases on both sides.
- B168: et_*_boat_*, et_cow_shares_boat, et_walk_into_* on the et_/eo_/es_/n_ set.
- B174: f_elytra_no_deploy_riding_*; f_elytra_* on 1.15 to 1.15.2; --adapter 0 unexpected over 1153 worktree and 1694 pass recordings.
- B176: docs/flight-phases.md, gl_*, f_elytra_mounted_mid_glide.
- B177: f_elytra_rocket_yaw_*, two_in_a_row, lands_while_burning, when_it_breaks.
- B178: Rocket holder and attachment model through --projectiles; explosion damage out of scope, documented.
- B186: Rider seat compared every boat tick; D-108 done in the engine.
- B187: v_scaled_*, v_effect_*. No dismount API: the server sends the position.
- B091: docs/synchronization.md, test/sync-contract.test.cjs.
- B197: docs/projectile-capabilities.md.
- B198: pj_*_bubble_*, border_crossing, no_gravity, pj_arrow_still_in_stone, water cases, sliding rocket.
- B090: The entity lane's sneak step-off and the recorder lane's server-tick move together: v_dismount_minecart_pocket and c_ride_minecart_dismount match on 26.3, 1.21.8 and 1.20.4, mc_* 84/84 on each with --moving.
- B092: Rain clearing fire unit-tested only; replay has no rain input mid-case.
- B200: Before 1.13 only torch and open gate outlines; a gate in a wall not lowered.

pj_arrow_enderman was a replay bug: the enderman's position was read after its teleport. Fixed 7/7. Other fixes: pistons now push non-players (simulateVersionedEntityPistonPush); packed tick accepts kind 8; client landing gate 1.21.2 to 26.3; no-AI rearing from 1.21.5; head plate on 1.9/1.10; open gate before 1.13; rockets and hooks keep their box before 1.17; zero-height boxes stop vehicles before 1.13. The plugin must call pushByMovingPiston for a live bot's vehicle.

## Recorder lane, third batch, and pass triage

Commits: 86f62e3 e140371 3fd911c e7f2421 c5d77fa 8e2d493; triage 9d4f106 9fff977 aa32ab5 9bfd747 a4ce860 eb6acc4 43d892d.

- B236: world-compare.ts compares every cell's boxes and fluid from the resolved state against NativeWorld; a tampered cell is caught. Fixture proof on 26.3; legacy adapters unproven.
- B238: Effects compare as a set (activeEffects is a HashMap); passengers stay ordered. Tested both ways.
- B243: cf_cut_mount_diagonal reaches a tick with both components under 0.003 and the length over it, then stops, on the release where the player keeps its motion.
- B245: Mount in web, trident, arrow held by a block, water crossing, border from both sides; projectile replay now passes the border.
- B232: Budget in the recorder's replay/profile-budget.json: +30% CPU per row, +25% peak memory, accepted by the lead. CPU rather than clock: clock moved 54% between runs, CPU 16%. Streaming not built: parse is 86 to 155 ms of a multi-second replay. f_elytra_rocket_level at 21 to 26 ms a row is unexplained and has no budget.
- B221: the recorder's replay/bot.ts and --bot detect missing, extra, reordered, byte-different packets. 26.3 byte-matches j 82/82, s 153/154, wa 32/32, gr 43/43, f 61/61, pb 82/82, g 209/209, e 94/95, b 228/233, n 26/27, v 59/100. Hands not driven; other releases not swept. Two fork questions wait on the user.
- B222: vph capture and compareVehicleMove done; the engine has no trace inside a vehicle's tick; radius sufficiency unverified.
- B237: All 39 fields have a verdict, 9 are declared gaps (leftImpulse, inputJumping, inputShiftKeyDown, xxa, yya, zza, useItemRemainingTicks, jumpRidingScale, jumpRidingTicks); RECORDED_ONLY in schema/fields.ts still claims comparisons that do not exist.
- B244: docs/capability-table.md generated from all 50 releases; no effects-only versus effective-attribute comparison; cf_attr_burning_time barely ignites; two cf_ cases compare motion, not damage.

Pass triage: chested mounts now have a real container, the 1.20.5 and 1.20.6 hang; ServerPlayer xxa and zza and stuck arrows reset between cases; Level.randValue pinned each tick; ClientChunkEntriesMixin on 1.14.4 to 1.16.5 clears a stale chunk entry, the cause of the et_ doubling is a source argument, not reproduced; strider cases rest on stone; record.ts names every failed scenario. Mod changes on every release: a clean full pass is required. B311 filed for D-212.

## Fluid behaviour lane

Commits: 94fa42f d2de919 c045d3e a9a5213 ed7beeb b1c05ac 94e0e27 e3b7dba, leaves a5a60de.

- B137: wa_, lf_, cw_near, cw_neg, cw_high match on 1.13.2, 1.14.4, 1.16.x and 1.20 to 26.3 except 1.20.3, 26.1.1, 26.1.2.
- B139: wn_ dries, replaced, walk_into plus wl_, wh_ on 16 releases.
- B141: we_e*, we_strider_afloat_right, we_slow_falling_wall, we_gravity_zero_wall, dg_.
- B142: ls_ plain, gravity, slow falling, levitation on both sides of the shallow branch, ld_, fj_lava_*, jo_lava_*.
- B146: entityTravelSupport inventory; sl_, bf_under_*, me_mob_* including me_mob_tadpole_swims on 1.20 to 26.3 except 1.20.3, 26.1.1, 26.1.2.
- B147: liquidCollisionBox, sl_ strider at y=99.5; the forced query is unit-tested against source only.
- B148: ec_ eyeInWater under stone and slabs at scale 0.55 and 0.6, fluidHeightWater from 8/9.
- B149: generated/fluid-tables.json for 49 releases, reproducible with --check; 1.8.8 unverified.
- B155: ic_, s_*_frosted_ice, v_boat_ice, bf_*. bf_lily_on_ice since 1.15: up to 1.14.4 the client breaks the pad itself, not modelled.
- B132: ex_* leaving water, effects gone in the jump, crawl with weaving; crawl pose and water entry checked directly on 26.3.
- B138: Ultra-warm lava has no differential: recordings have overworld blocks only, needs a mod change.
- B140: Matrix generated with 0 mismatches; no recorded loaded-chunk fixture; identities per document; rows unverified until the final pass.
- B144: Server's own boat bubble handling not checked.
- B145: No dismount beside a climbable in fluid.
- B159: pc_cauldron_*_burning never enters the cauldron, proves nothing.

Fixed: boat water height on 1.13; the controlled-by-player fall skip on 1.21.4 hit plain mobs. The 1.15 leaves disagreement between two lanes resolved from source and recordings: 1.14 sturdy, 1.15 not, since MinecraftServer rebuilds BlockState's cache after data packs load; a 1.15 client alone on a dedicated server has no such rebuild and is not recorded.

## Moves lane, third batch

Commits: 935581e 1d8f294 75d10f6 cfa0041 7ba8efa e96e385.

- B051: test/state-transitions.test.cjs: effect expiry and seats on object and packed paths.
- B052: All 32 flag combinations on 5 releases; the matrix has a 'changes at' column.
- B053: Engine builds for wasm32-wasip1 behind a bindings feature: 512 walks, 26228 ticks, 0 differ from Java, digest 55bc5299d48e37b1 on host and wasm. Threads variant, Linux, macOS not run.
- B054: Jump rejection on the attribute's ulps and pose sizes from jars.
- B055: As B053.
- B057: --phases clean on q_, n_, fd_, m_d on 26.3, 1.21.5, 1.20.4 and v_, death, s_, g_ on 26.3. Block effects and pushes have no phase of their own. Below 1.20.2 not compared.
- B059: Death transition, boat and horse batches, counter table in docs/state-contract.md.
- B076: Interaction channel and support in the boundary fixtures.
- B082: A turn or resize leaves state alone.
- B124: Generic and ridden consumers mapped; knockback_resistance audited, server-side only.
- B126: Zero and negative gravity, speed, step, jump, safe fall on both paths.
- B067: Mth.equal call sites on 3 jars and BlockGetter walks on 9 jars match with no engine bug; a run mutating the traversal squares alone was not captured, so their discrimination is unconfirmed.
- B074: Implemented in native/src/shulker_box.rs, bit-exact on 1.16.5, 1.17.1, 1.20.4, 26.3; no recording covers lids, 1.14.4 and 1.15 pushes and 1.11 to 1.13 not run.
- B058: docs/random-consumers.md is 26.3 only.
- B073: Collision agrees on 45 releases; outline and interaction channels not compared.
- B117: Not practical from source: checkMovementStatistics runs from handleMovePlayer on packet deltas and FoodData.tick reads difficulty, a game rule and health. Goes to the user.
- B125: 23 of 45 jars; 1.14.4, 1.15.2 rows have no entity bases.
- B209: Layouts versioned; docs/field-maps.md lists type and optionality only, no units or persistence.
- B210: Adapter agreement on contact damage untested.
- B214: No gate removed; per-gate boundary evidence missing; simulateTick takes no release.
- B218: Moving entity, projectile, arrow, trident kinds accepted but never run.

Shulker lids implemented with ShulkerBoxTracker; before 1.17 a closing lid also pushes. Before about 1.21.9 an infinite motion hangs the collision walk in Java.

## Entity lane, third batch

Commits: dbf18cd 8f2810d 309241f 2f17246 2b7f74f 9f32262.

- B092: the recorder's replay/rain.ts follows the rain level from packets; hz_fire_in_rain restored; 22/22 on 26.3, 1.21.8, 1.20.4, 1.16.5.
- B200: OracleLegacyOutlines reads every state from jars of 1.8.8 to 1.12.2; pj_snowball_at_* 47/47. Joined redstone wire, 1.8 portal box, actual-state metadata not proved.
- B202: docs/mob-travel.md: the engine holds travel, AI results are inputs; 7 new me_mob_ cases; all me_ 652/652.
- B308: The adapter already called setMovingBlocks; isPushable was wrong by name. OraclePushReactions read all 50 jars into push-reactions.json. Default states only.
- B027: Old and improved paths selected by the flag on the same release; server motion and client interpolation separated.
- B085: Packed output carries verticalCollisionBelow; round trip test.
- B086: mainSupportingBlockPos compared in pm_ and ph_; teleports covered.
- B097: pm_ride_* and v_* compare fallDistance; two cart gates fixed.
- B099: Server pose refits the kept box on 1.14 to 1.16; passenger and spectator poses not re-audited; nothing targets the 1.21.5 precedence change.
- B189: hurt_minecart has no replay input; a hopper taking from a container needs a mod field; end gateway and blast effects server-side, not modelled. Gates fixed: cart lava from 1.16.2, cart fall fsub from 1.18.2.

Left by the lane: B098, B103, B105, B107, B110 to B113, B115, B116, B120 are player work; B104 callers in another library; B119 needs a mod change; B065, B070, B083, B161 open.

## Release contracts lane, second batch

Commits: 74952d5 a73b9b2 c8949c8 547042e 05b483f f181fd1.

- B003: --phases on both sides of each boundary: 1.20.4/5 93, 1.21.1/2 64, 1.21.4/5 109, 1.21.5/6 30, 1.21.8/9 46, 1.21.10/11 44. Only PHASE_FIELDS compared at each phase.
- B029: 26.1.1 and 26.1.2 replay, 180 of 180 with phases: all 8594 data-generator files byte-identical across 26.1 to 26.1.2, 3 and 9 of 10682 client classes differ, each reviewed by bytecode diff; an unreviewed class fails generation.
- B019: A burning ServerPlayer with no draw is refused ('needs its fire draw'): a behaviour change for callers relying on the hidden 1.
- B005: scripts/state-roundtrip.cjs: every field of boat, mount, cart, projectile, arrow, trident, moving entity classified; shows a field is not lost, on one tick in air on 26.3.
- B018: scripts/shared-functions.cjs: 1292 functions with callers and interval, unverified by default, computed on demand. 258 unreached and unclassified.
- B011: M29 closed by B034, so the ledger has no open finding; the audit directory is gone, so its before and after counts cannot be reproduced.
- B007: 99 of 288 gated functions have no source finding at a boundary.
- B012: 16 of 25 findings have named traces; M03, M04, M07, M09, M16, M18, M24, M25 none, M14 needs legacy recordings.
- B016: Not all shared paths have release evidence.
- B013: Final recordings.
- B026: Final recordings.

Mutation testing: nine gates and one policy moved of 288; 3 of 9 gates had no check until a fixture was added, 2 only a unit test. A verified cell means the recorded scenarios match, not that they reach every branch. shared-gates.cjs counted 221, the real count is 288.

## Packaging and Linux lane

Commits: a0dd649 056d238 610941f 5c4df77 9500e44 79612e2 6948cc8 e654e79 deabff1.

- B262: Windows: two clean builds byte-identical in different directories after /Brepro. Linux: two clean runs at e654e79 give identical tarball 5a529625 and compare-builds equal. Wasm repeat builds not run.
- B263: verify-release-data.cjs compares names exactly and reads back each table; fixtures fail for wrong names, labels, domains, defaults, counts, fluids.
- B265: Strict compile with skipLibCheck off; 34 third-party declaration errors tolerated and reported.
- B266: Mixed builds refused; object, packed-player and vehicle calls pass on Windows; packed and vehicle not on Linux.
- B267: Node and Chrome 156, Windows only: wasm paths bit-equal to native over 480 ticks on 8 releases. A missing worker reports an error but the load never settles.
- B273: Windows and wasm tarball and Linux tarball each checked against their records.
- B274: Windows and Linux, Node 24.
- B285: Windows, Linux and wasm.
- B294: Windows and Linux on 8 releases; no 26.x FROM_BOT, npm minecraft-data lacks it; effects not exercised.
- B010: Proven with --data full at 5c4df77; at the tip Linux has no pinned jars for the jar stages.
- B291: Same gap; network and oracle outputs cannot be regenerated from pins alone.
- B293: Same gap, and sources were copied by hand, not materialized from jars.
- B264: macOS unavailable.
- B275: macOS unavailable.

Linux at e654e79: clean runs a and b2 DONE, 1120 JS and 1364 Rust tests pass. Clean runs found: manifest paths depending on checkout depth, generators silently preferring a tree, a case-insensitive 1.14.4 path, wasm over 8 MB refused by Chrome, bot.world not type-checking. LoadNativePhysics now refuses an unrequested WebAssembly fallback on a declared native platform. rust-toolchain.toml pins 1.95.0.

## Moves lane, fourth batch

Commits: e0a6662 c99a0ef.

- B067: Every threshold held by mutation from a clean baseline: Mth.square(1.0E-5F) 304 lines as a double, 24 with <= for <; Mth.square(0.99999F) 346 and 44; Mth.equal 624 of 1872 walls on 1.18.2. Results in docs/numeric-contract.md. Found a real bug: 1.21.4's walk has no 1.0E-7 at its ends, fixed by gate on 1.21.2 to 1.21.4.
- B074: physrec/sb_*, 10 cases, 6 facings, on 11 releases 1.11.2 to 26.3, all match; lids off, 1.11.2 to 1.17.1 fail all 10. Found a shut lid staying one tick too long. Other patch releases not recorded; 1.14.4 has no jar oracle.
- B210: Adapter agrees on landings, a teleport and support, and takes no contact damage as a client.
- B218: Player, boat, horse, minecart, snowball, arrow, trident, item, TNT, orb resume by bits on all 50 releases; lids on 26.3.
- B311: Only half the premise held: a box exactly on the border is pushed; the offer happens for a box under 1.0E-7 past it. bs_just_past_a_set_border: no push on 1.16.2 and 1.17.1, 1.18 pushes every tick, 1.16.1 once. Native matches by phase on all four.
- B058: Audit covers 31 source trees; 19 releases have no tree; helper-routed draws searched on 26.3 only.
- B073: Outline and interaction on 21 releases; contextual states skipped; pre-1.13 has no named jar.
- B125: 44 jars 1.14 to 26.3; before 1.14 no named jar.
- B209: 40 rows say only 'as vanilla holds it'; adapter side unmapped; boundary round trips cover the horse only.
- B214: docs/version-gates.md lists every boundary; 257 of 541 sites uncited, no per-gate mutation, no gate removed.

Open regression found by this lane, not from it: physrec/g_step_*_lectern, being fixed separately.

## Fluids second batch and the 1.8.8 cart fix

Commits: 7df756c 4896efc 3d0f84b 0e8b202 3c6f7f0.

- B138: uw_* builds the same blocks in the nether by commands, no mod change: 10 cases match on 15 releases 1.16.2 to 26.3; uw_level_3 vx 0.0047 overworld, 0.0140 nether.
- B144: Native boat gained AbstractBoat.tickBubbleColumn via serverSide; me_boat_* compared per server tick on every evidence release from 1.13.2; bc_ past the 60 ticks. Events 71 and 72 not independently proved at adapter level.
- B145: fc_dismount_boat_ladder_wet, _jump, fc_dismount_strider_ladder_over_lava; found the 1.14 slowdown seed used the previous tick's key.
- B159: pc_cauldron_*: the player is inside, srv.remainingFireTicks 160 to -20, powder_snow_cauldron becomes water_cauldron[level=2].
- B140: Fixture and per-result identities done, 850 results, 0 mismatched, 346 unverified until the final pass.

1.8.8 cart: before 1.9 a player ticks from its move packet ahead of the level, and the gets_off step ran there, removing the rider before the cart's own tick; a rider thrown off by an activator rail now rides through it. Boundary at_least(1,9,0). mc_* and cart cases match on 8 releases freshly recorded. Still different: v_dismount_minecart_pocket on 1.21.2 to 1.21.4 by one ulp at t=12, cause unknown.

## Generated data lane, second batch

Commits: 02b431c c9c8ef0 407bbba fdb73d8 5fbc78a e0e2ac6 23f63b9 badf55c ef276a7 c4655e5 be4daba 6e817a8.

- B040: onHit, onHitBlock, onHitEntity keep their writes in source order for 18 kinds by 50 releases; an arrow's order flips at 1.21.2.
- B042: Soul sand and honey are the only slow blocks of any release, honey the only low jump; Entity.move's callback order pinned in 6 eras.
- B044: Every attribute the adapter reads is linked to the jar inventory; Depth Strider's slots move to feet at 1.21; q_depth_strider_boots_off_mid_swim on 1.20.4 and 26.3.
- B045: scripts/java-ir.cjs, a typed parser and evaluator that refuses the rest by line: 4 vanilla methods against the jar and native by bits. No collision method.
- B047: explain() traces a value to hashed inputs; generated/provenance.json detects edits. The native/data Rust tables are not in it.
- B049: verify-generation-invariants.cjs is generate-all's last stage; M30, M31 exit nonzero on tampered copies.
- B038: Never run on a clean machine.
- B041: Rider points at a non-zero yaw and at scale missing.
- B043: Simplification and iteration decisions not extracted; 6 Yarn-named releases unmapped.
- B046: generated/movement-constants.json: 149 source, 4 jdk, 0 unresolved; a match does not prove the right site or type.
- B048: Needs Linux and macOS; worker counts 1 and 8 not run.
- B050: generate-all --check exit 0 in 45 minutes before the last merge; workflow never run on GitHub.
- B152: LegacyShapeDump matches state by state for a block alone on all 6 legacy releases; neighbour-connected states need recordings.

Found: minecraft-data has later shapes for the 1.13.2 bed and cauldron, fixed from the jar, not live-proven; rider points depended on a constructor's random yaw. A new public collision function fails generate-primitive-map until placed; a changed generator or oracle fails provenance --check.

## Performance and feature contract lane

Commits: 4499fd7 ce00f11 242aedb a4c984e 0388d7b 0a4eeb8 4bd3fa4.

- B296: src/mineflayer/physics-features.ts answers the 8 dependency feature names from the selected release; test/physics-features.test.cjs re-reads breakpoints from source trees of 50 releases. The dependency's EntityState.isUsingItem stays wrong on 1.8, 1.9, 1.14 to 1.16.5, documented.
- B252: scripts/bench/run.cjs --suite optimizations proves paths equal to the bit before timing; caching lt and gt took JS allocation per tick from 39.7 to 11.7 kB on 1.12.2. No time budget: no stable baseline on this machine.
- B254: All 50 releases, five scenes, docs/benchmarks/ticks-win32.json; regressions guarded by counts.
- B255: Every stage timed alone with counters, docs/benchmarks.md; the adapter's JS costs more than the native tick.
- B256: Six workloads on 1.12.2, 1.20.4, 26.3: heap flat, native 0 bytes kept over 5000 ticks. Windows only, 3 of 50 releases.
- B257: Shape tables cost 4.2 MB of addon, no measurable load cost; 23 identical tables share a module, source 16.7 to 8.1 MB, 4,289,260 lookups identical.
- B260: NativeBulkRun and runBulk cover seven batch families; chunks and one long call compare deep-equal; docs/bulk-simulation.md.
- B258: 600 runs, 3,000,000 ticks, 50 releases: Windows against Linux, wasm32-wasip1 and threads, 0 differences. macOS, ARM and a browser cannot run here.

Adapter: the Riptide hand now comes from the tracked flags in every release.

## Recorder lane, fourth batch

Commits: a9cbc23 2eea62c 3cf24dc ab7f2d3 30a415c fdf7664 a024fa8 f4b7810 3a92c1f bffc0f2.

- B222: simulateVersioned{Boat,Horse}TickTraced compared at the head and tail of the vehicle's move on 15 boat and 14 mount recordings; pistons 244/244 with phases; radius.ts over 8094 recordings, 3 short, all me_dragon_wings_throw_a_mob.
- B237: All 39 fields decided, 0 gaps: xxa, yya, zza, jumping against a new travelVector, 3698 recordings on 7 releases, mutation shown; jumpRiding compared at release. A strider's walkAnimation is in no snapshot, stated.
- B244: Three mortal cases match; effects-only 90/100, the 10 being transition ticks where the attribute packet follows the effect by a tick; adapter motion 623/623. docs/capability-table.md generated from 50 releases.
- B221: Hands and betweenTicks done; families j s v n e et on 8 releases 3503/3674. The remainder needs fork changes, on lane/plugin (B312). Screens not driven; below 1.19.3 not swept.
- B189: Hits reach the replay via simulateVersionedMinecartHurt; mc_hurt_* match on 3 releases, mx_hurt_* not recorded. Left: hopper taking from a container (mod field), spawner cart, end gateway, blast on nearby entities.

The v_dismount_minecart_pocket ulp on 1.21.2 to 1.21.4: from 1.21.2 requestTeleport also zeroes the velocity, so the player does not travel that tick; fixed in the harness, physrec/*minecart* 119/119 on 7 releases. Raft seat on 1.20.4 fixed. Mod needs noted: hopper contents, mount.walkAnimation. To record after the pass: cf_attr_fall_damage_multiplier_mortal, cf_feather_falling_boots_mortal, cf_attr_burning_time_burns, mc_hurt_*, mx_hurt_*.

## Final pass triage

Commits: 8d3a374 2c737f0 2db9405 2195dbb 07f7fc4.

- B313: Player: velocities past MAX_SYNCED_CELLS skip the tick, zero the velocity and set state.moveRefused, tested, in docs/numeric-contract.md. The adapter's sync throws first, around 4e5 blocks a tick on one axis. Vehicles, moving entities and projectiles still throw.
- B314: me_mob_* 45 ids identical in 4 of 4 runs on 1.13.2 and the pass pair matched; no differing pair and no mechanism; the lockstep race fixed here is only a suspect.

Fixed: a lever before the flattening; chunksHeld read the server's chunk maps from the client thread, a crash in a mid-rehash table (x_far29m); the server now runs nothing while the client handles a tick's packets, the pm_ride_*_lifted and mv_top_* differences; area builds until 1.12.2 kept one mutable position as every block entity's key. Mod changes on every release. Still open: et_* and eo_* doubling on 1.14.4 and 1.15.1, unreproduced.

## Docs lane

Commits: 2edf47b.

- B268: docs/release-status.md generated by scripts/release-status-doc.cjs, held by a test: all 50 releases; 0 admitted, 40 untested, 5 short of recordings, 5 blocked.
- B269: docs/engine-architecture.md traces a direct tick and a FROM_BOT tick; a test checks every path it names. Add-a-release steps read from scripts, never run.
- B270: docs/synchronization.md gained ownership, tick order, swept regions, unknown against air, events against polling, border and height, a recipe; examples run in test/doc-examples.test.cjs.
- B272: docs/limitations.md by release, entry point and platform: 41 active, 5 fixed-unvalidated, tied to docs/parity-dispositions.json by a test. Not each re-read against source.
- B271: docs/field-catalogue.md: 86 interfaces, 876 members, 136 entry points, held by a test. Per-field release ranges not done; about 500 fields have name, type and direction only.
- B295: docs/release-notes.md written and marked unreleased; needs the pass result, pins and a named tarball.

Gated functions are 295, not 288: 193 source-compared, 102 unverified. Pins are stale, 0 of 23 current: re-pin after the last merge. New checks: scripts/field-catalogue.cjs after native.d.ts or packed layout changes, scripts/release-status-doc.cjs after a re-pin. src/native.ts hand interfaces had drifted, fixed.

## Last fixes lane

Commits: 51312ef 944bf80 4ab332d 6419fbd.

- B313: Boats, ridden mobs, carts, moving entities and projectiles drop the motion, skip the tick and say moveRefused: test/move-refused.test.cjs, 18 tests up to 1.7179869183E10 and Infinity; in docs/numeric-contract.md. A cart in a scene sits its tick out; hurt still lands.
- B314: No differing pair; the stranded chunk entry and the lockstep hold are both fixed now, either may have been the cause.

et_ and eo_ doubling on 1.14.4 and 1.15.1: when the view centre moves before the player's checkChunk the old list entry cannot be removed and the ring chunk is reused; ClientChunkEntriesMixin now clears such entries on arrival. Shown by forcing the order on 1.14.4 (x3 pushes, x1 with the fix); not seen unforced. Adapter: movingPistonsOf, movingBlockTrackerOf, pushByMovingPistons are public; a retracting sticky piston ends the block it pushes. Bot replay seeds carts and rafts.

## Server-side statistics

Commits: none: no code change.

- B117: Server side only, closed at the user's decision. Movement statistics add exhaustion through causeFoodExhaustion (addExhaustion before 1.14), which is guarded by !isClientSide, read on 1.8.9, 1.12.2, 1.14.4 and 1.21.11: a client never changes its own food by moving, whichever class holds checkMovementStatistics. The engine keeps taking food as synchronised state, as sprint eligibility does. Server-mode exhaustion is not modelled and is not claimed.

## Mineflayer fork

Commits: mineflayer 5125443c b617942e ad7f876b 2e901c14 02e71fef 348ab605.

- B312: The fork's physics.js uses MinecartTracker, pushes a vehicle by moving pistons and fills nearbyEntities; merged fast-forward into mc-physics-demo after Grim, not pushed. mineflayer-replay on 1.20.4 passes 2690 of 2690, so the 38 failures are gone; other releases were not rerun at this tip. mineflayer-survival's grim.test.ts under lockstep on 1.21.11 with the local Grim build: 15 of 15 and 0 alerts over about 4000 ticks, with the JS physics and with the native engine, so the set-back loop stays fixed.

## Exact trig

Commits: 4fd5c894 d30c800f.

- B065: Math.sin and Math.cos are now Intel LIBM's, merged: equal to JDK 21.0.11 and 25.0.3 by raw-bit digests on every finite float, the Mth tables, 22,000,000 random values and 160,361 hard cases, on Windows x64 native and wasip1. Left: the same vectors on a Linux native build. Java 8, macOS and arm64 did not run.

## Exact trig on Linux

Commits: 4fd5c894 d30c800f.

- B065: x86_64 Linux native on Temurin 21.0.13: Rust equals the JDK on all 529 digest lines of the same sets as Windows, including every finite float, the Mth tables and atan tables, and cargo test --lib trig passes 12 of 12. The Windows boundaries vector file gives the Windows digests on Linux. With Windows x64 and wasm32-wasip1, the oracle runs on each supported target here; macOS and arm64 stay with B264 and B275, as they are not built. Older JDKs differ on direct calls, and the Mth sine tables are equal on all of them: B315.

## Public field contract lane

Commits: 1d2ffcd3 c43dd82e 80232779 5d1d23fc.

- B209: N-API lossy integers refused by field name; round trips on all 50 releases for player, boat, mount, cart, moving entity, projectile and arrow, trident from 1.13.2; adapter against binding on 44. Left: the adapter path on 1.19.1, 1.21.2, 1.21.7, 1.21.10, 26.1.1, 26.1.2 (exact data on the way), world inputs beyond 26.3, field names in non-player refusals, addons not loaded through LoadNativePhysics.
- B271: Catalogue and field maps generated from contracts with 484 source anchors; the vague rows are gone and the examples are doctests. Left: 671 rows have a family range unchecked by name below 1.14.4, MECHANIC_ANCHORS was sorted by judgement, 1,013 rows written by subagents had about 12 sampled, and the kept column is a snapshot.

## Extraction and randomness lane

Commits: 1d51e90c b73e161c 45fc4454 d68864c1 1d417884 8b5b08d4 ec41b387 25de7b6c.

- B125: test/fixtures/attributes.tsv covers all 50 releases with keys, defaults, ranges, player and zombie bases; the six legacy releases come from their own remapped jars, sha1 checked against version.json, and 1.9.4 to 1.13.2 match EntityAttributes.java on 47 rows. 1.8.8 has no tree, so its rows rest on its jar. minecraft-data holds 1 as the least max_health up to 1.13.2 where the jars have 0, pinned by release; no movement reads it.
- B041: Entity.positionRider called on built vehicles: 4129 rows across all 50 releases at yaw 0, 37.5 and 90, per seat, at scale 2 from 1.20.5 and as a baby; the engine's boat seat equals them on x and z, alone and with two aboard. Legacy sizes, eyes and offsets from LegacyEntityDump, agreeing with minecraft-data-generator on 253 entities. Limits: other seats come from the caller, y is not compared with the engine, pig and chicken from 1.21.5 and minecart from 1.21.2 have no rider rows, legacy eyeCrouching is null and the legacy player's eye is not read.
- B058: scripts/audit-random-draws.cjs walks all 50 releases into test/fixtures/random-draws.tsv, helper-routed draws included; draws in a LocalPlayer's tick are each named, none moves the player, and an unnamed draw fails the test. Both fire increments, LocalPlayer unread on all 50, clone, restore, missing draw, version mismatch and unrelated world events are tested. Limits: calls on other objects are not walked, mob AI and block classes pass by class rules, a particle or sound in a statement counts as cosmetic, and exact server randomness stays excluded.

## Player lane

Commits: fcd85507 680411b6 0fe2c7f6 867132d0 c255224a b6111f29 ecf6c866 97b2ced0 83ccd93a 15c77763 618a42eb 984a8c16 ce166ece 2aa5037e 84f7ee58 7ea255bf.

- B098: fd_ cases on 50 releases with no difference, each landing held to the server's health; the matrix source column splits exact, inferred and runtime. On 1.17 and 1.17.1 a player takes no damage landing on powder snow, shown by two recordings and the source.
- B099: pl_glide_into_water_sprint_80 at t=37 is fall_flying on 1.21.4 and swimming on 1.21.5, both replay; passenger and spectator poses in a tunnel recorded and replayed.
- B105: pl_mod_mix_* pick a base where the fold order decides the float: the real order matches on 1.21.4, 1.21.5 and 26.3, reversed or sorted orders fail 2 of 4 cases.
- B107: Ridden ticks now compare decisions and input. Limit: 1.19.3 on from the corpus and 1.14 on through pl_passenger_*; not compared on 1.8 to 1.13.
- B110: Lava and ladder overlap families on 50 releases, no difference.
- B111: pl_swim_eyes_only_sprint swims on 1.17 and does not on 1.17.1 on the real clients; 1.13 fluids now use the box updateSize left, and to 1.15 the eyes are a late query.
- B112: Spectator entry, dismount into a tunnel and ceilings that change are recorded and match.
- B115: living_knockback, six release families, by bits against the jars on 1.17.1, 1.18.2, 1.20.4, 1.20.5, 1.21 and 26.3; 1.8 to 1.16 against recorded arrow packets at 1/8000 precision at resistance 0 and 1. At 0.5 those releases roll, so no formula check there; 1.21.5 and 1.16.5 would not compile under the probe.
- B116: java-oracle-input.txt by bits on 1.21.5, 1.21.11 and 26.3. Limit: vanilla has no analog input path, and before 1.21.5 it is float multiplication only, with no jar check.
- B070: Mergers and optimize ported; OracleUnion holds ordered boxes and moves before and after optimize on 1.14.4, 1.16.5, 1.17.1, 1.18.2, 1.21.11 and 26.3. Limits: 1.13.2 has no entry in that oracle, and union is still called only by tests.
- B103: Matrix complete and stepwise with phases clean on all 50. Left: the 19 adapter differences under B113.
- B113: 1.18.2 clean, spin and rocket ticks included, clone and resume work. Left: f_elytra_rocket_hits_a_mob and f_elytra_wall_hit_rocket differ in the adapter layer on 1.14.4 to 1.16.5; stepwise matches, cause unknown.
- B119: A vehicle's hurtClient is true, and the hit tick matches. Left: pl_attack_sprint_boat, pl_attack_walk_boat and pl_attack_sprint_cow differ on later contact ticks on 1.14.4 to 1.16.5 in the adapter and on 1.15 and 1.15.2 stepwise; cause unknown.
- B120: Eyes one double from the surface recorded on all releases. Left: an unloaded adjacent column, which needs a forget-chunk server op in the mod.
- B083: docs/entity-move-phases.md maps 15 eras with 35 differs or unmapped items, read only. Three implementations of move: the tick, move_entity and move_vehicle_among.
- B161: pl_nb_*, pl_inside_* and pl_step_anvil_* match. Left: neighbour cases do not run on 1.8.8, and legacy connection states are sampled, not every permutation.
- B104: state.clone() calls cloneState once the engine has ticked the state. Left: a state never ticked, ridden ticks before 1.19.3, mineflayer-replay not run.

## Movement constants, suspected bugs and the 1.14 to 1.16 differences

Commits: bf92e173 99fa6db3 99b234dd d274ab76 891155a7 fe4ec12b.

- B113: Three causes, none in the engine or the adapter path. The adapter replay seeded velocity from tickStart, counting a push twice for an entity that ticks before the player; it now seeds from row.pre as engine.ts does. seed.ts took a firework rocket for a mob where minecraft-data says UNKNOWN on 1.15 to 1.16.5. The two flight cases now match on all ten releases from 1.13.2 to 1.17.1.
- B103: Held open only by the 19 adapter differences of B113, which are gone. docs/player-parity-matrix.md still shows the old rows until parity-report reruns.
- B316: Each item checked against the source. Fixed with tests: inLava from 1.14 to 1.16.1, a riding hook's box from 1.15 to 1.16.5, pearl_lost's wonGame from 1.21.9, the pearl game rule from 1.20.2, minecart inventory refused on a non-hopper, three doc comments, four unused adapter fields, off-hand riptide, and the legacy eye as a float. Struck: the pre-1.11 hook (1.8.9 FishingBobberEntity.java:203-206) and the boots fallback (the fork always sets the field). Lava and hook fixes rest on source and unit tests until the top-up recordings.
- B119: Stale recordings: the boat or cow pushes twice in one tick on 1.15 pl_attack_sprint_boat, pl_attack_sprint_cow, pl_attack_walk_boat and 1.15.2 pl_attack_sprint_boat. Re-recorded at HEAD, 7 of 7 in both layers. Left: the top-up replaces them. The link to 6419fbd8 rests on timing and the clean re-record, not a reproduction.
- B046: generated/movement-sites.json lists 1714 literal rows with rustc's types: 679 extracted, 322 module, 573 structural, 111 reviewed, 29 open; a new literal with no row fails a test. Left: whole-number constants are not inventoried, 322 module rows are not tied to a site, 29 open sites, 150 gate uses with no release named.

## Unloaded column

Commits: e5527f5b.

- B120: A forgetChunk server op, with CompatChunks per packet era, and physrec/ul_water_across_the_column: recorded with verify and replayed on 13 releases from 1.8.8 to 26.3, adapter on 7, 0 unexpected; with the forget packets stripped on 26.3, 16 of 47 ticks diverge. 8 existing cases on 1.21.5 compare the same across the change. compileJava passed on 23 releases; the releases between overlay edges, and the 1.9.4 resend at runtime, are not checked until the top-up records them.

## Shapes lane

Commits: d4377d0e ff7afaf4 5b653751 75583c2a 97675187.

- B043: generated/primitive-map.json maps all 50 releases, the six legacy by their own classes; 28 walk, join and clip methods keep branches, shape calls and tolerances in source order per era, and each optimize call site is listed. OracleEdgeCollide: 7,399 fixture lines on 1.8.8, 1.12.2, 1.13.2, 1.14.4, 1.18.2, 1.21.11 and 26.3 for touching planes, gaps around 1.0E-7, negative cells and axis order all match; reversing x and z in native fails the test. Limits: 1.18.1 Shapes.java is empty in the source cache and listed unread; before 1.14 axis order has no whole-move jar fixture.
- B152: LegacyNeighbourDump on 1.8.8 to 1.12.2: 1.4M to 2.7M comparisons per release, no collision difference; position, entity and real moving-block contexts on all 44 modern jars, no difference. Left: no jar fixture for moving blocks on the six legacy releases, and shulker boxes are outside OracleContext.
- B073: docs/adapter-audit.json covers all 50 releases: 1,034,494 modern states with 0 actual differences on six channels, 3,371 masked data defects kept apart, 622,860 contextual answers. Left: redstone wire and vine outlines before 1.13 ignore neighbours, a falling block with its feet in powder snow, and B152's legacy gaps.

## Generic gates lane

Commits: 3b93ad82 155563e8 6fe9d404 9e0cf54f e0cdcf4f 26ec85eb 738f9a2e d38bdb51 5bfe07ef.

- B214: Dead and duplicate gates removed. A copy of the final engine with them put back against it: 216 world runs on each of 50 releases, 10,800 in all, plus knockback and capabilities, 0 of 50 differ and 0 refused; moving a kept gate shows, so the battery is not blind. It does not see the player's 1.18 border gate moved, which rests on reading. docs/version-gates.md records each removal.
- B012: All 25 findings replayed on the final engine with --json and addon identity; everything matches, and M14 has both sides. A trace is shown to reach the finding's branch for M03, M07, M14 and M24; M15, M19, M21, M23 and M25 run their gate but match with it flipped, so their proof stays the unit test; the other 16 are not one gate that can be flipped, and are not shown either way. docs/finding-ledger.md says so.
- B007: Projectile names outside the table are refused. Left: 100 of 294 gated functions unverified in generated/shared-gates.json, 126 of 746 gates cite no source, entityTravelSupport says unknown and travels as living.
- B016: Not worked directly. Left: a method and behaviour matrix by release, comparing vehicle phases, B189, recordings below 1.19.3 beyond subsets, source comparison for the 100 functions of B007.
- B290: Sweep of 746 gates on the final engine: 355 noticed on both sides, 160 on one, 155 ran with nothing noticing, 76 never ran. Sampled: replays capped at 40 per gate, every third recording from 1.19.3 on, and generation-integrity.test.cjs gave no hits. Left: evidence for 231 gates, with the top-up's older recordings in the sweep.

## Moves lane

Commits: 17067566 0aba2bf6 f7e3b26e 8e2d8ac6 3e97b125.

- B046: Whole numbers inventoried too: 3359 rows, 1110 extracted, 399 module, 312 reviewed, 1538 structural, 0 open (was 29). Adapter 1e-5 comparisons f32 (1.21.5 ClientInput.java:24), jump probe 0.5000001 before 1.20 (1.19.4 Entity.java:771), dead effects_in_bounds and Pose::dimensions removed, piston ceiling extracted on 1.16.5 PistonMovingBlockEntity.java:190 and 1.20 :191. Left: the 399 module rows tied to one site each, 53 of 624 gate uses with no release named.
- B007: All 44 modern jars through pinned Vineflower 1.12.0 into E:/mc-physics-demo-wt/compare-cache; scripts/compare-gate-sources.cjs and generated/gate-sources.json drive shared-gates. 218 uncited gates read on both sides: 209 right, 9 wrong; flying mob ground cell before 1.20 and move_towards_closest_space at 1.11 fixed. Left: 7 of 294 functions unverified (minecart attack@1.20.3, clip_blocks_between@1.13 wrong; in_end_portal@1.17, player_pose_size@1.17, ridden_mob_state@1.20.5, fall_damage_resetting_boxes@1.21.9, ridden dimensions@1.9,1.14 change only a field), and the gates the review found wrong.

## Review findings lane

Commits: 47e7d75f 61cea3d4 5e57e5ee 9c2790b4 b9a518e7.

- B007: Six gates the review found wrong fixed, each checked in source with a test: creative hit on a named cart before 1.20.3 (1.20.2 AbstractMinecart.java:232-239), no spectator candidates in any release (1.13.2 EntityGetter.java:18-20), lava push in water from 1.16.2 (Entity.java:1064-1070), the 1.13.2 ray trace on the outline with the old walk (Level.java:719-878), cauldron inside shapes from 1.21.6, the block-effects guard after travel from 1.21.2 (LivingEntity.java:2817). Field, constant and constructor gates are now compared by declaration: 296 of 296 gated functions source-compared, 0 unverified. Left: boundary recordings (pj_* on 1.13.2 to 1.16.2 are stale), recount of gates with no citation, entityTravelSupport unknown.
- B046: Module rows 397 (extracted 1112), 0 open. Left: module rows tied to one site each, gate uses with no release named.

## Shapes lane (player worktree)

Commits: 62ff56d0 3d405304 8463eaeb 06346dcd ef5dfe98 8449ae24 baec4d47.

- B073: Redstone wire (1.9-1.12) and vine (1.8-1.12) outlines read their neighbours from tables LegacyNeighbourDump measures against each jar (src/mineflayer/legacy-outline-neighbours.json); test/legacy-neighbours.test.cjs has no exceptions left, 0 outline differences on every release. Powder snow holds a falling block wherever its feet are (1.17 PowderSnowBlock.java:90-96, 26.3 :98-104), falling rows compared at every height on all 44 modern releases. Audit: 1,034,494 modern states, 0 differences on six channels, 645,612 contextual answers.
- B152: Moving blocks on the six legacy releases: LegacyNeighbourDump entities mode places a real moving-piston block entity, test/legacy-block-entities.test.cjs compares 3,600-4,464 cases per release against the trackers and NativeWorldSync, 0 differences after fixing 1.9-1.10 push box as outline, 1.11-1.12 short arm on the block entity, 1.8 moved base as full cube, zero-height boxes before 1.13. Shulker boxes in OracleContext with every lid step and facing: all 44 modern releases match; legacy dump covers 1.11.2-1.13.2. Limit: on 1.8 the jar's moving box for blocks without a collision override depends on the bounds last set; the fixture sets them from the block's own state.
- B161: Legacy connection states drawn at random at every cell a block reads, 50,924 draws on 1.8.8 and 64,336 on 1.9.4-1.12.2, 0 differences; pl_nb_* now run on 1.8.8. Left: record and replay the 26 pl_nb_* cases on 1.8.8.

## Citations lane

Commits: d16f303f c5ea8db7 de4c499e b7d98676 e4d64913.

- B046: 3448 movement sites (scripts/movement-sites.cjs had read nothing after a test module in the middle of a file): 1397 extracted, 443 reviewed, 1608 structural, 0 module (was 397), 0 open. Each former module row is tied in audit/movement-site-reviews.json to the vanilla line that holds the number at the site's type (proved by the generator, badSiteReviews empty) or named as the engine's own with its expression; friction table, default friction and dragon fly speed are dispositions. Gate uses with no release named: 0 of 629.
- B007: Gates citing no line of a release either side: 0 of 752 (scripts/gate-citations.cjs, test/gate-citations.test.cjs; 155 of 750 at the start). Five more gates fixed with tests: rocket collision boost at 1.16.4 (FireworkRocketEntity.java:155), item in lava thrown only until 1.15.2 (1.16 ItemEntity.java:136-137), cart fire cells by Mth.ceil from 1.9 (1.9.4 Level.java:1582-1588), hopper cart past its first item from 1.19.4 (MinecartHopper.java:92-97), boat in water not pushed by lava in 1.16 and 1.16.1 (1.16 Entity.java:1041-1044). entityTravelSupport has no unknown kind; dragon routed to its own tick, shulker unsupported. Left: pj_* recordings on 1.13.2 to 1.16.2 (stale or absent), being re-recorded.

## Fields and input lane (player worktree)

Commits: 4e35374d f9916f57.

- B271: Evidence reads the six legacy trees through the recorder mappings and follows members by Fabric intermediary id (E:/mc-physics-demo-wt/intermediary): rows with an unchecked lower bound 735 to 11; 182 rows carry the member's own range back to 1.8.8. test/field-evidence.test.cjs reads back all 968 recorded places as declarations, all 299 binding rows and all 61 why citations; four anchors that matched an unrelated use corrected; depthStrider rows now run to 26.3; the kept column is measured on the addon at generation and checked both ways. Left: 11 rows the mappings cannot follow (26.2 members, Entity.getPose and HitResult.getLocation at 1.13.2, three Boat overrides) and MECHANIC_ANCHORS kept as a tested hand list.
- B104: A state never ticked clones through cloneState (test/from-bot.test.cjs). Ridden ticks before 1.19.3: 233 recordings, 2186 ridden ticks on 1.9.4-1.19.2 and 9 new ridden cases on 1.8.8 (587 ticks) match through the adapter. mineflayer-replay with REPLAY_ENGINE=native on 30 versions: every engine case passes once the harness puts recorded equipment in the bot's inventory (branch player-replay-equipment 2f9952f in E:/mc-physics-demo). Left: bot-layer differences, sneak release x off by one ulp on 1.9.4-1.16.5, b_powder_snow_sneak_inside vel.z on 1.19.2/1.21.3/1.21.4, v_horse_step_up_block yaw -90 against 270 on 1.18.2; merge the harness branch.

## Entity.move lane

Commits: d3bc85d2 d22dc591 d1d93330 52c47661 deb34c8f a2fc2481 6d586fb4.

- B083: One Entity.move for the player (simulation.rs entity_move, called by the tick, the riptide lift and piston/shulker pushes with a MoverType); every other entity's run_vehicle_move shares reset_fall_on_clip, move_without_physics and refreshes_vertical_collision and counts the fall inside the move. Of the 35 items in docs/entity-move-phases.md: 16 fixed with tests in native/src/entity_move_tests.rs, 2 fixed before, 8 proved unreachable with the source cited, 8 no-op, D17 in part; all 44 modern releases placed by hashing move in the compare-cache trees (the fall-reset clip arrives in 1.18.2). Replays of mv, me, mc, v, b, n, su, sb, es, q_riptide, pj on 11 releases: 11502 recordings, 11492 match, 3 unsupported, 7 fail identically on the previous tip (n_walk_into_minecart on 1.8.8-1.16, pj_fishing_hook_on_ground 1.8.8, pj_firework_climbs_into_wall and pj_fishing_hook_hits_target 1.16). Left: D17 a piston push kept in the entity's movement list until its next tick, D19 the 26.3 interpolation target of an entity the client does not simulate, the 7 failing replays, vanilla fixtures and first-divergence traces.

## Minecart lane

Commits: 9858b659 8a3e3458 066a5948 f6306c77 87e86c14 5e62467e.

- B189: Engine: TNT cart blast on nearby entities (ServerExplosion.hurtEntities, Explosion.explode before 1.21.2, with float roots before 1.17, whole-number hurt to 1.20.2, outlines before 1.16, zero-length direction from 1.21.9 and the other gates cited), end gateways (block entity tick before 1.21 with its 1.11, 1.16 and 1.16.2 changes, portal entry from 1.21; an undecided pick is reported, not guessed), spawner cart countdown (spawnerBlocksEnabled from 1.21.9; the spawn itself is the server's), hopper cart taking from the recorded container. The mod records container, spawner, gateways and portal fields server-side. 14 Rust, 2 N-API and 3 replay tests; mc_* and mx_* replays on 26.3, 1.21.5, 1.21.2, 1.20.4, 1.13.2, 1.12.2, 1.8.8 all match. Left: record mc_hopper_from_chest, mc_hopper_under_chest_rolling, mc_tnt_blast_on_carts, mc_spawner_counts_down, mc_spawner_out_of_range, mc_end_gateway and their mx_ twins plus mx_hurt_*, and re-record mc_/mx_ for the new fields; item pushes and blast resistance not modelled.

## Replay failures lane

Commits: 58069b11 c158d214 839ac750 43fbe6da 39283bc7 abb3046a 2e1797f2 1ccfcaed 3b4370f8.

- B083: D17: a shulker box push waits in the entity's movement list until its next tick from 1.21.5 (26.3 ShulkerBoxBlockEntity.java:133, Entity.java:807, :960-963), carried as SimulationState.pending_movement; the piston's own push is removed by removeLatestMovementRecording (1.21.5 PistonMovingBlockEntity.java:182-183). D19 has no engine output: the only interpolated entity the engine keeps is a client minecart, and the stepwise replay reseeds the recorded target (docs/entity-move-phases.md). The seven replays failing on the previous tip pass: the player pushes a cart from its own server tick (1.8.8 ServerGamePacketListenerImpl.java:290-291), drops negligible velocity and takes the box middle before 1.17; a fishing hook's box is remade by setPos before 1.17; a rocket out of the area's box is out of view. Replays on all 41 recorded releases: n_, mc_, pj_ 9069 of 9069 stepwise, pj_ --projectiles 4622 of 4622.
- B007: A 1.13.2 fireball meets the cells past the world border with a one-block margin chosen by its isOutsideWorldBorder flag (1.13.2 ProjectileUtil.java:27, LevelReader.java:113-155), from 1.14 only a box meeting the edge from inside (1.14.4 LevelReader.java:147-157). Left: pj_* recordings on 1.13.2 to 1.16.2 are being re-recorded.

## Platform scope: one validated platform

Commits: none (todo and backlog only).

- B048: Narrowed to one host (Windows): equal output for worker counts one and eight, a relocated root, an interrupted run left intact, cached and fresh modes reported apart. The cross-OS part moved to todo.md Platform-specific (lowest priority).

Scope change: one validated platform completes a backlog item, and Windows is the validated platform. B010, B258, B264 and B275 only add another operating system, so they left the backlog for deferred/ (python move-to-deferred.py). Their todo.md lines stay unchecked under the new section Platform-specific (lowest priority), with what Windows and Linux already showed. macOS, ARM and browser hosts stay in todo only.

## Exact data for six releases (lane moves)

Commits: 55c08564 a481702d 90b9f6b9 7aba7318 7989e706; minecraft-data bc37177 (inner 8ca30d50); minecraft-data-generator 0c1154c.

- B302: 1.19.1, 1.21.2, 1.21.7, 1.21.10, 26.1.1 and 26.1.2 run on their own registries; the runtime aliases (dataReleaseOf, the bot special case, the recorder's SAME_DATA) are gone and the alias proof lives in generated/data-aliases.json. Protocol and data versions match each jar's version.json. Every final-pass recording of the six replays stepwise, continuous, projectiles, adapter --motion and carts with only failures the pair release shares; --bot on 1.19.1: 3151 match, the other 131 fail the same way on 1.19.2. The 1.21.7 recipe generator bug (295 shapeless recipes without ingredients) is fixed in minecraft-data-generator 0c1154c; 1.21.5 and 1.21.6 have the same generator bug but correct tables.
- B209: The adapter-against-binding tests run on all 50 releases, the six included, and the filter that dropped refused registries is gone. Non-player refusals (horse, ridden mob, boat, projectile, arrow, trident, moving entity, entity push, moving piston, entity descriptor) name the field, value and rule. guardNativeBinding checks bindings passed through options.binding, NativeWorldSync or ShulkerBoxTracker; a raw addon called outside the package stays unchecked. The world round trips that ran only on 26.3 run on every release with its own stateVersion and last state id.

scripts/source-pins.cjs pins minecraft-data bc37177, which must be pushed to mc-zuri/minecraft-data-node before a fresh bootstrap clone works. Shared failures worth a re-record: m_dimension and uw_* on 1.21.10 and 26.1.x.

## Bot layer lane (player worktree)

Commits: 683dc0eb 889f4d74 882d5af6 224e852a 1ec3c533 eebbeb1c.

- B271: No catalogue row keeps an unchecked lower bound (was 12 on 8 anchors): each edge the mappings cannot follow is read in the tree of the release before it (scripts/field-read-edges.cjs), checked when docs/field-evidence.json is written; 26.2 members Block.getBounceRestitution and Entity.getAirDrag are mechanic anchors. test/field-evidence.test.cjs fails on any unchecked row. MECHANIC_ANCHORS stays a hand list tested against real identity boundaries; deriving it from gates or row text did not hold, and the ticket's criteria do not ask for it. Entity.Movement read at 1.21.1 and 1.21.2 (scripts/field-read-edges.cjs).
- B104: Sneak release one ulp off on 1.9.4-1.16.5 fixed: the box the end-of-tick size update refits is sent to the engine before the pushes after the tick (1.12.2 Player.java:324,386-392, Entity.java:382-394). Harness: the bot starts with the synced frozen ticks (1.17 LivingEntity.java:499-520), branch player-replay-equipment c992f0f on 2f9952f in E:/mc-physics-demo. mineflayer-replay with REPLAY_ENGINE=native: 41 of 42 runs pass on 30 versions. The 1.18.2 v_horse_step_up_block case passes: a passenger start primes the bot with the look it holds, as a passenger sends its look every tick without updating the last-sent field (1.18.2 LocalPlayer.java:186-187); harness commit 81c0d6c. mineflayer-replay on 1.18.2: native 3659 and default 3659 pass, 0 fail; the other 41 runs passed before.

## Build reproducibility lane (Windows)

Commits: 7d79d181 7a810b19 21ac9f1f 22861e60 31931941.

- B048: Windows only, as the user scoped it. With pinned jar inputs (scripts/fetch-jar-inputs.cjs, sha1 from piston-meta, named jars by sha256) generate-all with GENERATOR_WORKERS=8 (49 min) and GENERATOR_WORKERS=1 in a second checkout at E:/mc-physics-demo-wt/reloc/a/b/minecraft-physics-native with every location explicit (60 min) give equal scripts/generated-hashes.cjs inventories, 150 of 150 files, equal to the commit. generate-all keeps a journal: a run killed in its third stage was restored by --recover with 0 of 150 different, and test/generation-integrity.test.cjs kills a run after a stage half-writes a table and has the next run restore it. Each run ends generation: fresh or cached with the inputs it kept. Fixed: block-shapes.json recorded the host's missing libraries; the legacy named jar was chosen by mtime in one generator and by a path hash in another; a legacy release's entity data silently became unavailable without its jar. The cross-OS part is in todo.md Platform-specific.

Linux notes (WSL Ubuntu 26.04, Temurin 25.0.3+9, node 24.15.0, rustc 1.95.0): every Mojang input fetched and verified by sha1 and every named jar by sha256, bootstrap-inputs passed with sources at other roots; no Linux generation finished. Unimined names a legacy jar's folder after the mapping file's bytes as checked out, so Linux and Windows look in different folders; minecraft-data bc37177 and data 8ca30d50 are not on GitHub (MC_MINECRAFT_DATA_SOURCE, MC_MINECRAFT_DATA_SUBMODULE_SOURCE); source trees must sit on the Linux file system, not /mnt. See docs/release-build.md.

## Thin and connected blocks on 1.8.8

Commits: recordings only (local final pass at 389d7bff), replayed at 0a7e236c.

- B161: The last gap, the 26 pl_nb_* cases on 1.8.8, is recorded (verify 0 different) and replays 26 of 26 stepwise, 26 of 26 through the adapter and 26 of 26 continuous at 0a7e236c. With the 50,924 and 64,336 random connection-state draws (0 differences) and the shape lanes' per-state geometry, every family's geometry, approaches, corners, step-up/down, waterlogged and edge cases are covered on all releases.

## Minecart blast resistance and item blasts (lane carts)

Commits: d11b5d64.

- B189: TNT cart rays by block resistance per release (1352 rays, Material.AIR before 1.13, fluid resistance from 1.13, level bounds from 1.17, rails spared), returning certain and possible blocks or the exact list with levelRandom; the blast hurts and pushes item entities (whole-number health, blast-proof stacks, the 1.21.2-1.21.5 water check); a cart's own move pushes no item (ItemEntity is not pushable). Fresh recordings at 0a7e236c: mc_* all match on 1.8.8, 1.12.2, 1.13.2, 1.16.5, 1.20.4, 1.21.1, 1.21.2, 1.21.5; mx_* 93 of 94 on 1.21.2 and 1.21.5. Left: mx_end_gateway y after the gateway exit (t=38, 100.0225 against 99.96) on the improved path; record mc_/mx_tnt_blast_on_items, _on_items_new and _by_obsidian.

## Trig target per declared JVM (lane moves)

Commits: 96318692 985635d9.

- B315: Each release follows the Math of the Java version its version json declares (math::declared_java_major): Java 8 up to 1.16.5, 16, 17, 21, 25. trig::java8 ports fdlibm 5.3 and models x87 fsin/fcos within pi/4. Direct Math.sin/cos sites (1.8.8 boat seat and paddle, cart push to 1.13.2, glide, 1.20.2-1.20.4 passenger) and Mth.atan2's COS_TAB use it. Measured on Temurin 1.8.0_512: past pi/4 0 of 14.27M call-site values differ; within pi/4 9 of 1,593,644 differ, listed in docs/limitations.md with the untested Java 8 mode (the recorder runs Java 21/25, so replays set Java 21 via setMathJava). Gate: cargo 1458, JS 2281, recorder 267, 0 failed.

## Source inputs from pins (lane repro, Windows)

Commits: 64863f06 f4aed681 93af249b.

- B293: On a fresh clone on Windows: fetch-jar-inputs --check (3112 files at pins), materialize-sources rebuilt all 52 Mojang-named trees (26.2/26.3 from pinned mc-source commits, six legacy decompiles through Vineflower 1.12.0) equal to their hash pins, bootstrap-inputs --check passed, release-build --data full ended 'generation: fresh' with 151 of 151 generated files equal to the commit, and consumer-check on the tarball passed with its deliberate failures failing. A second clone made the same 52 trees, and compare-builds agreed with the first on all 7 inputs, the toolchain and all 66 shipped files. Missing, wrong-release and edited trees fail with a message naming the tree. The 30 yarn-named trees are supplied and held by hash only (B038). Linux and macOS deferred (platform scope).
- B038: Every tree read is now pinned by hash (scripts/source-tree-pins.json, 82 trees) and bad trees fail by name. Left: the 24 modern and 6 legacy yarn-named trees under E:/pr/mc-decompiled cannot be made by a script (no record of the yarn build or decompiler); port the generators that read them to the Mojang-named trees. The 27 releases from extracted_minecraft_data are pinned commits, not decompiled from jars.
- B291: Two clean runs give equal content; network and oracle inventory entries record pin and detect; generate-mojang-releases holds version json sha1s and refuses a moved pin (Mojang's list now gives new sha1s for all 26 releases 1.19.3-26.3; pins unchanged). Left: no change detection for generated/gate-mutation.json, the tag-audit-dispositions input, data-aliases.json's jar identity and movement-literal-types.json; network and oracle outputs need live inputs.

## Hopper cart containers and TNT replays (lane carts, round 2)

Commits: b2df5284.

- B189: The mod records a hopper cart's containers (the 26 cells around the one above the hopper, via HopperBlockEntity.getContainerAt), so the arrival tick has its input; mc_/mx_hopper_from_chest and _under_chest_rolling re-recorded on all releases and matching. TNT recordings at c49b9f8a: mc_tnt_blast_* match on 50 of 50 releases, mx_tnt_blast_* on 15 of 15. The adapter's simulateScene syncs the world at gateway exits. Left: mx_end_gateway and the 1.21.x portal cooldowns and mx_top_speed on 1.21.11 are fixed on lane/player (5ec3d818, e9c55c4e, 6e431b2e) and need that merge plus a 1.21.11 mx_top_speed re-record; a boat ticking ahead of a cart and mob order have no recording; team rules are not modelled; armor stands and bats are treated as ordinary pushers.

## Generators off the yarn trees; change detection (lane repro, second pass)

Commits: 596fdd08 80d3a0a3 02da1024 f300bb73.

- B291: Every generated artifact either has a documented pinned step or is a captured input marked `capture` in generated-inventory.cjs, and the check fails on any entry with detection 'none'. gate-mutation.json is pinned to the commit it names (gate-report --commit refuses a sweep of other gates); the tag-audit input is held by sha256 and its client jars by sha1 (--accept-audit to override), reproducing the committed file byte for byte; data-aliases checks each jar's identity (client and server sha1, intermediary sha256); movement-literal-types records a code hash of native/src and rustc, recomputed by a test. Pinned: shared-gates, gate-sources, release-aliases, data-aliases, flow-blockers (--check), vehicle-versions, gen-java, movement-literal-types. Captured at a pin: mojang-releases, runtime-registries, conformance-evidence, gate-mutation, tag-audit. generate-all --check: exit 0, generation fresh. Windows.
- B038: Every generated table now reads Mojang-named trees made from pins; the six legacy releases are decompiled from their own jars remapped by legacy-mappings (1.8.8 no longer read as 1.8.9; 1.8.8-1.13.2 now exact). Block-by-block equal to the yarn output apart from names; real data changes explained (fluid-tables gains 1.8.8 and the missing water/lava tags; flow-blockers forces shulker boxes solid for 1.21.5-1.21.11 and 26.2). Left: code comments in native/src, src/mineflayer and field-contracts cite yarn files and lines, which generate-movement-sites checks, so the 30 yarn trees stay pinned for that one use.

## Minecart team rules, armor stands, bats and order (lane carts, round 3)

Commits: d231bdfc.

- B189: Team collision rules (EntitySelector.pushableBy from 1.9) at every vanilla call site; the mod records each server entity's team; bats are not pushable and push nothing; armor stands are not pushable and push only nearby rideable carts (picking themselves up before 1.10). New cases team_*, armor_stand_beside, into_an_armor_stand, into_a_bat, bat_beside, boat_ahead and mobs_either_side match on all 50 releases (mc_) and 15 (mx_); without the new inputs about half fail. Criteria 1-4 met. Left: criterion 5 (1.21.2 worlds with the experiment toggled) needs lane/player's mx_end_gateway and mx_top_speed fixes merged and replayed clean.

## Phantom determinism on 1.13.2

Commits: none (evidence only).

- B314: 12 local runs of physrec/me_mob_phantom_circles on 1.13.2, each with --verify: every pair identical, and run 1 identical to each of runs 2-12 (compare-runs). Mechanism (1.13.2 Phantom.java): every draw is the phantom's own Entity.random, seeded by EntityRandomMixin from the case seed and creation count reset by ALIGN; with no finalizeSpawn its anchor is its own block, and the invulnerable recorder player is never targeted (TargetGoal.java:127), so the flight depends only on seed, summon tick and terrain. The earlier difference fits the lockstep hold fixed in 2195dbb1 (a summon task at a tick's start could land a round apart); 6419fbd8's mixin does not apply to 1.13.2. known-failures.json empty, nothing loosened.

## Final replay matrix (lane player)

Commits: 059c6bd5 eb5ab263 6e431b2e 5ec3d818 e9c55c4e 8315854d 0163e118 3779380a b6264365 591c8624 1a5789ee 6db88b88 7c0d2295 249791db b15772f8 41910767 69979eb1 205a685b dbb45eb4 5f1f7ad6 8f4bb62f 095adf97 d33d8da0 ecad0af7 a8150996 d97921d2 a5fd0999.

- B013: All 50 releases have a recorded corpus with build and provenance identities (corpus/<v>/builds); the strict gate refuses a missing corpus, an empty selection or zero comparisons; goldens keep their identities. Final matrix over 50 releases in 9 modes, scratchpad final-replay/matrix.md.
- B140: Fluid parity matrix on the final corpus: 778 cells matched, 0 mismatched, each with per-result identities (final-replay/fluid-parity-matrix.md). The 5 bubble-column cells before 1.13 are n/a (no bubble columns); 1.19's aquatic cell has no recording.
- B224: the recorder's corpus/<v>.json pins every recording of all 50 releases (sha256, rows, scenario hash, build); pinning refuses stale recordings; corpus-manifest.test.ts runs in the gate and conformance-cli check holds the digests. Recordings not yet pinned (1.19.1-1.20.1 gaps, 1.21.11 mx_top_speed, the new cart push cases) are reported as unpinned until re-recorded and re-pinned.
- B240: Coverage matrix over all 50 releases: 709 rows ok, 41 GAP, every GAP a movement mode the release lacks (named, not compiled); Riptide in rain only and Slow Falling across zero covered where present.
- B241: Coverage matrix: 337 rows ok, 13 GAP, all features absent in that release (waterlogging, fences in old releases); native and adapter layers both clean.
- B026: 1.14.4, 1.15.2 and 1.16.5 are clean in stepwise phases, continuous and both adapter modes. Left: bot mode (113, 111, 128 failures, from the mineflayer fork) and the moving server-boat pushes.
- B219: Outcomes published for every release; repeat captures across shard orders not run.
- B235: Two reproducible captures per adapter not yet run.
- B282: Remaining mismatches are explained but unfixed: bot mode (fork: 1.21.2 gating, entity_action ids on 1.15-1.18.1, passenger travel, seats, shulker lids, click mount), moving server boats get no neighbour or hurt, fishing after the bite (synchronised random), 1.8.8 anvil bounds (crosshair-dependent shared Block bounds), boat lerp on 1.21.2-1.21.4 (f_elytra_mounted_mid_glide), 1.18/1.18.1 client falling blocks (wall clock), ph_slime_*_front continuous on 1.9.4/1.10.2.
- B283: Continuous has 15 unexpected (same set as B282's non-bot items); a repeated-run determinism check not run.
- B189: mx_end_gateway, the 1.21.x portal cooldown and mx_top_speed's 1.21.11 gamerule are now on recorder; left: re-record 1.21.11 mx_top_speed and replay mx_ clean.

## Minecart experiment worlds (B189 close)

Commits: 5ec3d818 e9c55c4e 6e431b2e b2df5284 d231bdfc.

- B189: Criteria 1-4 met in the carts lanes (executable engine and subtypes, experiment chosen by flag, server simulation apart from client interpolation, rails, water, collisions, speed, team rules, armor stands, bats, order). Criterion 5: with lane/player merged and 1.21.11 mx_top_speed re-recorded at c5e75b8f (verify identical), every mx_ recording matches stepwise with phases on 1.21.5, 1.21.11 and 26.3 (106 of 106 each), and mx_top_speed matches continuously. Not compared: the exact blown block list (level random not recorded) and item velocity after a blast.

## Engine citations on the Mojang-named trees (lane repro, B038)

Commits: 617955f3 58939736 b5380e60.

- B038: Every source tree the build reads is now made by script from pinned inputs and held to a hash pin; the 30 hand-supplied yarn trees are removed from bootstrap-inputs, source-tree-pins, tree-pins, generated-inventory and materialize-sources. The engine's citations in native/src, src/mineflayer, field-contracts and the site reviews now cite registered releases' Mojang-named trees (1.8.9 as 1.8.8, 1.14 as 1.14.4); a comment-stripped compare shows no code change. Movement sites against the yarn run: 3621 sites, 3560 the same, 30 better, 1 worse, no new open site. Accepted exception: simulation.rs back_off_from_edge 1.0e-5_f32 (1.20.5-1.21.4) proves at file level only, as every Mojang-named decompile prints it widened. Gate: cargo 1458, JS 2291, recorder 267, 0 failed.

## Repeat captures (lane player, round 2)

Commits: evidence only (captures in E:/mc-physics-demo-wt/determinism).

- B219: 200 cases from 66 families on 1.8.8, 1.12.2, 1.16.5, 1.20.4, 1.21.5 and 26.3, captured three times (2 shards; 3 shards; 3 shards shuffled verify). compare-runs differs on 2-5 cases per release, and row by row every such difference is server packet timing (chat, chunks) outside the physics rows: pre, post, phases and server state are identical. Continuous and stepwise-with-phases replay match 200 of 200 on every capture and release. Logs: scratchpad determinism/.
- B235: Same repeat captures as B219: two reproducible captures per adapter, with different shard counts and orders, give identical physics; only packet timing differs; replay of every capture matches 200 of 200.
- B283: Repeat captures deterministic in physics (B219). Left: item age off by one after a merge (documented limit), tnt_blast_on_items item position, and the bot part (fork lane).

## Mineflayer fork bot fixes (lane plugin, fork commits bfbf7574 4c0d173a f2803831 b849a7b7 9be0b52e; fast-forwarded into the fork's mc-physics-demo branch, not pushed)

Commits: 43c239ae 537240b8.

- B026: Bot mode on the corpus, different/matching, matrix to now: 1.21.2 3741/0 to 79/3682, 1.15 899/2236 to 61/3108, 1.18.1 970/2330 to 66/3268, 1.20.4 132/3270 to 59/3353, 26.3 112/3683 to 88/3731. Fixed in the fork: 1.21.2 tick_end and player_input, entity_action ids on 1.15-1.18.1, passenger travel (under water from 1.19.3, slow falling, levitation, bubble columns, cobweb, lava, the water current from 1.13), the chest boat seat before 1.20.5, shulker lids. Harness: mounts seeded as summoned, equipment enchantments kept, keys the bot cannot press listed in docs/limitations.md. Left: the boat's own bubble column under water, bf_under_falling x, seats (chest boat float turn before 1.20.5, strider, two passengers, scaled horse), click-mounted pig and strider, piston-lifted rides, mv_sup_*_edge before 1.14, q_rain, elytra rocket timing, pl_attack knockback, v_boat_into_*, strider cases. minecraft-data follow-up: newPlayerInputPacket from 1.21.2; entity_action actionId mapper on pc 1.15-1.18.
- B282: Bot part as B026.

## Shared gates source-compared (lane repro, B007)

Commits: d9d1b66e 6041dcf4.

- B007: Every gated function in generated/shared-gates.json has a source finding at each of its boundaries: 322 of 322 (321 before), gates with no finding 0 (4 before), stale reviews 0. The last, math.rs#declared_java_major at 1.17, 1.18, 1.20.5 and 26.1, is now compared through a versionJson gate review that reads each release's pinned version json (sha1 checked against generated/mojang-releases.json). entityTravelSupport returns unknown only for a name that is no game kind, and every player, mob and ridden tick refuses it; generic living travel runs only for the explicit living kind. No engine gate was found wrong. Gate: cargo 1462, JS 2294, recorder 320.

## Moving replay round 3 (lane player)

Commits: 48fd9bfe 04426a20 778a9031 d5053dc8 22c2242c 44d32c65 7ee28d35 d0742054 f40ea3c5 0793ece8 620237cc 91b5585a.

- B282: Non-bot: moving-server failures 705 to 94 over the corpus. Fixed: server boats pushed by the player, entities and hits (Boat.push rules); ph_slime_*_front on 1.9/1.10; boat lerp on 1.21.2-1.21.4 (engine tick_lerp plus mod capture); 1.8.8 anvil shared bounds pinned in the mod (re-recorded); arrows stuck before 1.13; armor stands (NoAI is Mob's); skeleton horse and strider after dismount; TNT blast on items; -0 pulled by Vec3.add gravity from 1.14 (engine). Left: server player movement (1.8 tickPlayer per move packet; 1.14.4-1.15.2 playerTick) for the 1.8.8 server boat and pl_passenger_tunnel_dismount_boat; item age after a merge (m_flow_equip, shears). Bot part: fork lane.
- B283: Continuous and repeat captures clean (B219, B235). Left: item age after a merge in the moving replay, and the bot part.

## Moving replay round 4 (lane player)

Commits: 55588185.

- B282: Moving-server replay of the whole corpus: 0 failures over 169,613 replays on all 50 releases (94 before). Item merges follow the level's id order, tryToMerge's survivor rule and merge's age and pickupDelay; the server player's own tick is modelled (1.8 tickPlayer per move packet before the move; from 1.9 in the connection tick), fixing the 1.8.8 server boats and pl_passenger_tunnel_dismount_boat. Left: the bot part (fork lane).
- B283: Non-bot part done: repeat captures deterministic, item age modelled, remaining limits (fishing bite, 1.18/1.18.1 client falling blocks) explained from source in docs/limitations.md. Left: the bot part (fork lane).

## Item use and attack penalties (lane player, B119)

Commits: 4d9998e2 10a8bbd2 c114ad09.

- B119: The mod can summon a remote player target (FakeTarget, seven constructor eras, all 50 overlays compile), so a recorded attack hits a living target whose client hurt is true. New cases pl_attack_sprint_player, pl_attack_walk_player, pl_attack_walk_player_knockback, q_eat_riding_horse, q_shield_riding_horse (from 1.9) and q_eat_speed_zero (from 1.21.11) recorded on 1.8.8, 1.9.4, 1.16.5, 1.20.4, 1.21.11 and 26.3; all match stepwise, continuous, adapter --motion and moving. Criteria: use start/end (q_eat_until_done, q_use_in_air, q_bow_draw_*), shield vs other item (q_shield_walk, q_eat_walk), diagonal (ie_eat_diagonal, pl_attack_use_diagonal_shield), use plus sneak (q_shield_sneak, ie_*_sneak_diagonal), speed zero (q_eat_speed_zero), passenger (riding horse: no use slowdown, LocalPlayer.java:695), attack success (boats, remote player) and failure (air, cow). Spear stabs are server-only (MultiPlayerGameMode.piercingAttack sends STAB; PiercingWeapon.attack runs in the server's handler), so no client recoil. pl_attack_* on 1.14.4-1.16.5 match in every mode. Remaining 44 releases are being recorded for the corpus.

## Completion-gate audit (lane player)

Commits: 2276583c 171715c9.

- B276: All 50 releases exact in src/release-status.json; test/source-admission.test.cjs fails a wrong embedded identity, missing mappings and changed hashes; scripts/tree-pins.cjs --check re-hashed all 52 source trees against their pins (exit 0); test/clean-build.test.cjs holds each tree to its pinned bytes.
- B277: Data dimension complete for all 50: admission compares each table with the running-client registry dump and names missing blocks; test/generation-integrity.test.cjs covers a lost block family, a dropped inherited property and missing or experimental tags and regenerates to the same bytes; release-data and block-tag tests cover state domains, fluids and tags. The 6 legacy releases are held to their remapped jars (no running-client dump).
- B278: test/version-registry.test.cjs checks each release's module name, VERSION and dispatch arm; tick_conforming refuses a profile whose release or contract does not match, covered for all 50 by the Rust contract test; docs/release-status.md reports registered and admitted apart.
- B279: test/registry-release.test.cjs rejects mismatched registry/release pairs before the binding is read across every constructor and checks generic, fixed and factory engines tick alike; test/from-bot.test.cjs checks the exact engine on both sides of a boundary; test/physics-features.test.cjs checks per-release feature answers.
- B284: The gate builds a fresh addon and runs all JS tests on it; a validation record (commit dd35efdb, engine 5a9661a8) of the package, declarations, build-provenance, from-bot, registry-release, state-roundtrip and field-boundaries tests exited 0; stale declaration/loader pairs are refused; zero/false/absent values covered; the packed tarball loads.
- B286: Benchmarks re-run on dd35efdb against the 2026-11-06 baseline on the same machine: allocation counts equal on every release tested, 0 bytes retained over 5000 ticks (native/tests/allocations.rs repaired and now in the gate), memory flat across rounds, CPU medians 0.64-0.72 of baseline; outcome digests changed only for 26.2/26.3 scenes from an engine change since the baseline, and every 26.2/26.3 corpus recording replays clean. docs/benchmarks.md and docs/benchmarks/*-20261109-win32.json.
- B280: All 323 gated functions source-compared and generic dispatch refuses unknown families. Left: an equivalence claim for each shared function with no at_least gate and for profile-rule gates (docs/shared-code-audit.md, Not covered).
- B281: Left: regenerate the subsystem matrix from current runs (B016) and the bot (B026).
- B287: Stale claims corrected. Left: completion claims cite current pins (B288) and a regenerated recorder-coverage page.
- B288: Source, data and implementation complete for all 50. Left: conformance pins on the current engine from the pinned corpus.
- B289: Same as B281.
- B290: Left: re-run scripts/gate-mutants.cjs against the full corpus and add fixtures for gates no case notices (155 unnoticed, 76 never ran before the corpus covered every release).
- B292: Left: a defined matrix (replay modes over the corpus, fuzz, long-run, allocations and benches, package targets) on one pinned candidate with a retained record; the bot mode (B026).
- B295: Left: final pass result, pins and a named tarball.

## Mineflayer fork round 3 (fork eee17c03 d0d6bde1; fork mc-physics-demo fast-forwarded, not pushed)

Commits: fa0af1f1.

- B026: Bot, different recordings, before to after: 1.15 61 to 49, 1.18.1 66 to 46, 1.20.4 59 to 40, 1.21.2 79 to 44, 26.3 88 to 66. Fixed: chest boat float seat turn on 1.20.2-1.20.4, two-passenger boats, scaled horses, strider seat and suffocating speed; the steered boat takes the server's motion; bubble column push order and box per release; aiStep cutoff for passengers; crouched passengers 1.15-1.20.1; piston pushes of ridden vehicles (adapter pushEntityByMovingPistons, movingBlocksFrom); the native engine no longer stops boat physics after three corrections. Left: interpolation ulps in pm_ride_minecart_lifted (1.18.1) and pm_ride_pig_lifted (26.3); click-mounted pig on the click tick; strider walk animation in water; mv_sup_*_edge before 1.14; q_rain, elytra rocket timing, pl_attack, v_boat_into_*.
- B282: Bot part as B026.

## Subsystem matrix (lane/repro df8fe2d2; minecraft-data fe40d4c0, data b483a458, not pushed)

Commits: 79a4bdfa b75afe22 7efeaeea.

- B016: docs/subsystem-matrix.md on full replays of every mode on all 50 releases (run2 on 79a4bdfa, run3 on d18655e5 for the 255 B119 cases and the adapter modes). Verified on 50/50: travel, collision, effects, vehicle. entity is blocked only by the fishing bite (m_fishing, m_flow_fish: random state not recorded, docs/limitations.md). prediction is incomplete: the bot (B026) and the 1.18/1.18.1 falling-block client landing (B283). Fixed the 1.19 entity metadata decoding (pc/1.19 types 19-22) in minecraft-data.
- B283: Moving client: only the 1.18 and 1.18.1 falling blocks (me_falling_*, m_c_dig_gravel_falls) stop short of the server's landing. Moving server: 0 failures on all 50.

## Falling blocks held on the 1.18 client (lane/repro 3b4c6629)

Commits: 76d49bcb 73ba191b.

- B283: Moving client: 0 failures on all 50 releases (7778 ok). 1.18 and 1.18.1 keep a removed falling block for 50 ms (FallingBlockEntity.java:105-109, 213-221; ClientLevel.shouldDelayFallingBlockEntityRemoval); the replay holds it with tickCount + 1. Left: the bot part (B026).
- B016: prediction now waits only on the bot and bot-carts (B026); entity stays blocked by the fishing bite.

## Shared-equivalence claims and the whole-corpus gate sweep (lane/player b01c3536)

Commits: 94a7744a b01c3536.

- B280: scripts/shared-equivalence.cjs writes generated/shared-equivalence.json, held by test/shared-equivalence.test.cjs. Profile rules: 24 of 24 agree with every release's source. Ungated functions: 949; 3 hold from source, 292 cite a method that changes at some boundary (2,245 function-boundaries) and need a review in audit/shared-equivalence-reviews.json, 33 cite methods in no tree, 621 cite nothing (130 binding glue).
- B290: 781 gates swept against every pinned recording of all 50 releases (gate-sweep.ts --wide, cap 300): 489 caught on both sides, 136 on one, 132 never caught, 24 never run. 292 gates need a fixture or a behaviour-neutral source reason. Sweep data in E:/mc-physics-demo-wt/gate-sweep-b290.

## Requested subsystems in the matrix, fishing exclusion (lane/repro eefee44d)

Commits: b00a236e 5b20882a eefee44d.

- B281: Every requested subsystem has its own matrix family (travel, collision, effects, entity, vehicle, fluid, projectiles, attributes, poses, tick-order), replay-verified on 50/50 releases with 0 unexplained differences. The fishing bite and retrieve are an explicit exclusion (docs/limitations.md); the hook is compared up to the bite. Left: 46 findings in docs/parity-dispositions.json (41 active, 5 fixed-unvalidated) keep all ten families incomplete; split over the repro, findb and findc lanes.
- B289: Cells: all 500 requested cells verified with their own cases; docs agree with the matrix (release-status and authority corrected). Left: the open findings as B281, and the prediction cells waiting on the bot (B026).

## Mineflayer fork round 4 (fork 217abd57 fd496fc2; fork mc-physics-demo fast-forwarded, not pushed)

Commits: 9b3d2d00 865e7b32 01a2bfcf.

- B026: Bot, all 50 releases: 169,868 recordings, 167,113 matching, 2,591 different (1.8.8 170 to 1.20.3/1.20.4 33; 26.3 55). Five-release, round 3 to now: 1.15 49 to 43, 1.18.1 46 to 44, 1.20.4 40 to 33, 1.21.2 44 to 39, 26.3 66 to 55. Fixed: entities spawn off the ground (click-mounted pig and strider), a hurt mount's walk animation (strider seat), the lava current on passengers (26.1 move push), Swift Sneak 1.19-1.20.1, scaled strider seat 1.20.5+, kept box before 1.14 (all mv_sup_* on 1.12.2). Left: m_c_open_*/m_c_dig_*/m_hotbar/m_bed/q_rain_* (client key handling the harness does not drive), pl_attack_* sprint re-set, v_boat_into_* (stops at 4.0, vanilla 4.19), f_elytra_rocket_*, m_c_mount_click_* before 1.13 (missing look), interpolation ulps (pm_ride_pig_lifted 26.3, pm_ride_minecart_lifted 1.18.1), c_ride_oak*, v_horse_*, v_swim_*, v_pig_*, the 1.21.4-1.21.8 bump, 1.8.8.
- B282: Bot part as B026.
- B283: Bot part as B026.
- B221: Bot packet replay runs on all 50 releases; differences as B026.

## Gate fixtures for the entity files (lane/gates2 4121db82)

Commits: 943f01bc 06aa4518 05e1c497 3cfb4168 a7107021 4a19cbd9 09ee3b7f 67b5b34e.

- B290: minecart, ridden, moving_entity, projectile, projectile_hits, vehicle and piston: 178 of 181 open gates closed by Rust fixtures (rechecked with gate-mutants recheck) or reviewed reasons in audit/gate-reviews.json; gate-report gains a justified column. Engine: 627 both, 30 justified, 52 one, 48 none, 24 not run. Left in these files: minecart 126 (1.21.4 side), ridden 409 (engine bug: fluid_jump_threshold before 1.20.5 ignores eye height < 0.4, 1.20.4 Entity.java:3148), vehicle 740 (1.17.1 side); possible walk_axis moving-piston edge deviation 1.14.4-1.17.1 (Shapes.java:224-270). Remaining files and the 24 never-run gates: lane/player.

## Gate fixtures for the player files and never-run gates (lane/player cd0663b6)

Commits: 4b8290b2 cd0663b6.

- B290: Engine: 781 gates, 726 both, 47 justified (audit/gate-reviews.json), 6 one, 2 none, 0 not run (was 489/136/132/24). Player lane: 76 fixtures (gate_fixtures_tests.rs and five *_gate_tests.rs) and 17 reviews; no engine bugs. Left: 500 entity_eyes_in_fluid 26.1, 618 collide_with_world 1.13, 762 fluid_contact 26.1 (after side); 623 collide_with_world 1.18 and 646 update_auto_jump 1.14 (neither side); 126, 409, 740 (gates2 follow-up).

## Parity findings: collision/fluid/landing (lane/findb) and player/travel (lane/repro 13e8e28c); player-lane last gates (3275d264)

Commits: 662505c2 1064873d 17be20fb 377f5b80 47eb57cc 3275d264.

- B281: Findings: findb settled all 17 (D-165 D-166 D-173 D-183 D-191 D-204 D-206 D-207 D-219 D-220 D-225 fixed; D-174 D-184 D-187 D-190 D-202 D-212 already fixed, now validated); repro settled 16 of 18 (D-192 D-194 D-195 D-196 D-197 D-215 D-228 fixed; D-161 D-167 D-186 D-189 D-211 D-216 D-218 D-221 D-229 validated). Left: D-214 (riptide launch order), D-222 (1.11.2 rocket boost along yHeadRot), and the findc set (merging).
- B290: Engine: 731 both, 48 justified, 2 one (126 now caught; 740 waits on the walk_axis fix), 0 none, 0 not run.

## Riptide launch order and the 1.11 rocket head yaw (lane/repro 19cf6973)

Commits: 4a5f2989 9aa1af97.

- B281: D-214 (riptide release before the entities' ticks, versioned::release_riptide) and D-222 (1.11.x rocket boost along yHeadRot, head_look input, packed layout v3) fixed with regressions; all 18 player/travel findings settled; tick-order verified. Left: the findc findings (D-172 D-175 D-177 D-179 D-180 D-200 D-208 D-209 D-210 D-227 active, D-226 unvalidated; merging) and six unverified gates from the merged lanes (simulation apply_fluid_travel_vertical 1.13/1.14, apply_block_effects 1.15; vehicle tick_boat_in_world 1.21.2, count_boat_fall 1.9, water_level_above 1.13/1.14; world contains_any_liquid 26.3).

## Source-compared gates of the merged lanes (lane/repro 419e362b)

Commits: d81b979e 419e362b.

- B281: All gates source-compared (8 new reviews); attributes, poses and tick-order verified on 50/50. Travel, collision, effects, entity, vehicle, fluid and projectiles are replay-verified on 50/50 with every gate compared, incomplete only on the findc findings (D-172 D-175 D-177 D-179 D-180 D-200 D-208 D-209 D-210 D-226 D-227), settled on lane/findc and merging.
- B289: As B281; prediction waits on the bot (B026).

## Engine fixes from the gate work (lane/gates2)

Commits: 36f32867 e98fdc51 5fa8ccaa 1cdd9490 7adb428d.

- B290: walk_axis counts moving pistons in edge cells 1.14-1.17.1 (1.17.1 Shapes.java:268) and the 1.11-1.12 legacy walk adds the layer-under edge cell (1.12.2 Level.java:1124-1128); recording pm_fence_pushed_from_under pinned and matching on all 50. fluid_jump_threshold gated at 1.16 (409); 740 caught by a fixture; 1.13 vehicle obstacle scan widened; shulker-box pushes kept as separate batches from 1.21.5 (pendingMovements); unsaddled strider speed from 1.19.4. New gates renumber the sweep: a fresh sweep is due once engine work settles.

## findc, mcbugs, carts2, bot round 5, release tooling; temp-folder leak fixed (recorder 2d192d90)

Commits: 43b8db3 c6a46e85 1c2b3966 d05ad461 c6d1fe4e 4d1654e8 2ffa8074 effc3f68 bc50143d dd639511 8ff3912e.

- B281: All 69 findings settled (67 fixed-validated, 2 duplicate): findc D-172 D-175 D-177 D-179 D-180 D-200 D-208 D-209 D-210 D-226 D-227. Projectiles: honey slide, hook and rocket block effects, cobweb stuck speed; pre-1.13 full_cube from the jars (legacy-full-cubes.json). mcbugs: 8 minecart bugs fixed with 65 new pinned recordings (creative hit on invulnerable cart, removed cart stops its loop from 1.21.2, boarding cooldown from 1.9, no self-rider push, portal/gateway riders from 1.21, blast NaN/zero/friendly-fire/ignition, comparator 99 from 1.20.5, adjust_to_rails ulp).
- B026: Bot, 50 releases: carts2 drives keys, screen clicks, trades, anvil, enchanting; m_c_open/m_c_dig/m_hotbar/m_bed/pl_impulse_sleeping/q_rain match on all 50; past-the-hands and codec 0. Round 5: pl_attack, v_boat_into, 1.21.4-1.21.8 bump fixed. Different down from 2,591 to about 1,500. Left: f_elytra_rocket, mount clicks before 1.13, v_horse/v_pig/v_swim/c_ride, j_walk_yaw, ph_slime_down, mx_ride, 1.8.8, interpolation ulps.
- B292: scripts/release-matrix.cjs (never green on nothing, retained record, --resume), release-evidence.cjs (B288 rows), release-notes.cjs (B295); conformance complete on 1.8.8, 1.20.4, 26.3 after modelling ridden mob interpolation (mob-lerp.ts); bench acceptance audit/bench-acceptance.json; fuzz job proven on 1.20.4.
- B288: Evidence rows generated; 0 of 50 complete until the final pass.

## Bot client actions, round 2 (lane/carts2 4648ed29)

Commits: c9d6147b 489ff814.

- B026: Bot, 50 releases at 656a776f: 1,517 to 1,136 different. Now matching: m_click2_boat_paddle, v_boat_into_oak_boat/minecart, pl_attack_sprint_*, two f_elytra_rocket yaw cases, m_c_use_lectern_sneak, m_c_use_entity_horse_saddle, several mount clicks. The pl_attack_sprint_boat_weak regression was a merge (a7b90ae4) that lost the miss reset of the attack strength (Minecraft.startAttack MISS, 1.20.4 Minecraft.java:1702-1707); restored. minecraft-data d0658d9/e0313e69 adds 26.3's dappled_forest.

## Shared-equivalence closed (lane/equiv 04f8691c)

Commits: 435a7407 0542afc1 0c3d09b0 744d02f4 65e7c249 255051e5 d3775e13 75be38b6 22572a3f 327ede9f aaa03239 d15ce654.

- B280: generated/shared-equivalence.json: 0 functions in review, 0 uncited, 0 unresolved, 0 open function-boundaries (3052: 2777 reviewed, 84 mechanical, 191 unreached each with a source-backed reason); profile rules 24/24. Reach re-probed at 0c3d09b0 with the fixed probe (old probe misplaced 249 of 1489 hits). Engine fixes: 1.21.5 bubble-column -0 push; set_pose_height squared from the corner with the float width; minecart jockeys from 1.9; legacy vehicle border stone before 1.13; shulkerBoxBlockEvent per release; concrete powder trace before and from 1.14; 26.2 mob landing bounce gravity and air drag; shouldDiscardFriction from 1.17; the shared empty chunk before 1.14 (MCPNWLD3); border stone and bedrock without a loaded chunk before 1.14. Left: the final sweep and release pass to certify on one candidate; 1.9.4 pm_ride_pig_lifted (vehicle y 101.0099 vs 101 at t3) under investigation.

## Ridden pig before 1.11 (lane/player 22c3f3de)

Commits: 52e3b9f6.

- B283: pm_ride_pig_lifted on 1.9.4/1.10.2: before 1.11 a ridden mob's controller is its first passenger (1.10.2 Entity.java:2285-2288, Pig.java:73-75), so the client travels an unsteered pig itself; the replay now ticks it as unsteered. All 341 ride cases on 1.8.8-1.10.2 match. Conformance on 1.9.4/1.10.2 still blocked by scenarios compiled but not pinned, and 1.9.4 mc_hurt_invulnerable_by_creative (removed expected true at t1).

## Client key handling, moving-piston pose, minecart improvements (lane/carts2 550aabe1)

Commits: d7cb0996 e327e3f0 50923951 550aabe1. Fork: lane/plugin merged into lane/plugin2 (efe80650), mc-physics-demo fast-forwarded to it.

- B221/B282: j_walk_yaw (1.8.8-1.19.2): vanilla sends a rotation whenever yRot - yRotLast != 0, with the held degrees (1.16.5 LocalPlayer.java:229-243); the harness turns on bot.physics.vanillaYaw. Fixed on 24 releases.
- B221/B282: ph_slime_down_on_head (1.14.4-26.3): the pose check (canEnterPose -> noCollision) meets the block a piston is still moving (MovingPistonBlock.getCollisionShape via PistonMovingBlockEntity). NativeWorld.setMovingBlock returns the shifted boxes; the adapter's isFree reads them, including edge cells from 1.14.4. Fixed on 44 releases; test in test/mineflayer.test.cjs.
- B221/B282: mx_ride_* (1.21.2+): the harness skips the configuration phase, so the tracker now takes minecart_improvements from the recording header's features (NewMinecartBehavior.lerpClientPositionAndRotation, lerp delay 3). Fixed on 15 releases.
- 50-release --bot: failing cases 1136 -> 579, none new. Gate: Rust 1752+1, node 2333+328 passed.

## Generic gates proven (lane/gates2 92418a4a) and bot round 6 (lane/carts-rebased 936ad25c)

Commits: 8925f9d6 2e320a3f 086fec73 a473e477 9284e25b 92418a4a 66447c7c cd4ea4f7.

- B290: final sweep of 105c19da over every pinned recording of all 50 releases (gate-sweep-final, engine and adapter phases): 856 gates, 800 caught on both sides of their boundary, 56 justified with vanilla lines on both sides (audit/gate-reviews.json), 0 one-side, 0 none, 0 not run, 856/856 cited (docs/version-gates.md, generated/gate-mutation.json). 28 new fixtures each flipped with gate-mutants recheck and caught; no engine bugs. Gate OK on recorder cd4ea4f7 (Rust 1774+1, node 2333+328).
- B221: bot round 6: rocket/elytra (unsent spawns seeded; eye 0.4 while gliding before 1.14), pig/strider spawn boost (riddenSeed), vanillaYaw. 50-release --bot FAIL 1789 -> 553, BAD 228 -> 0. Horse random rear (Mob.baseTick ambient sound, client-only Random; 1.16.5 AbstractHorse.java:395-399,714-718) documented in docs/limitations.md. Left: f_elytra_mounted_mid_glide (1.21.2 boat lerp target), 1.9.4/1.10.2 mount clicks, boat/chest-boat dismount, cf_ride_horse_stops_in_web, pl_mod_mix_walk_b, pl_attack_sprint_boat, 1.8.8 v_boat_*, interpolation ulps.
- B282: see B221: bot-layer differences now 2-17 per release (fewer on 1.19.4-1.21.x), only horse random rear is a documented limitation.

## Conformance on all 50 releases (lane/repro 258cc084), client key handling round 2 (lane/carts2 21cd6e1b), bot round 7 (lane/carts-rebased 647fdab8)

Commits: 11cbc028 54389775 258cc084 004f2f27 3b8be0b5 32ce048c(fork) 056dcc02(fork) d3b53aa8 aaab1f9e f4414dd9 21cd6e1b bf2a9a1f(fork) 647fdab8 fa3b3203 08bcfcd6.

- B283: lane/repro: 783 more compiled scenarios recorded with --verify on the recorder host and pinned (identical in both runs); 1566/1566 phase and continuous replays match. 54389775: the unsteered ridden-mob path starts at 1.9, since 1.8.8 has no controlling passenger (getControllingPassenger first in 1.9.4 Entity.java:2175, :2243). Conformance --phases --continuous is complete on all 50 releases.
- B289: subsystem matrix: travel, collision, effects, entity, vehicle, fluid, projectiles, attributes, poses and tick-order verified on 50/50 releases with 0 unexplained differences; every version gate source-compared (shulker box push at 1.11, shulker bullet at 1.21.5 added). Prediction stays incomplete on B026.
- B221: lane/carts2: 1.16-1.16.1 strider rider offset and lava flag, pre-1.14 unloaded-chunk ticking (corrects fork a8523de3), chunk check on unticked rows, 1.21.11+ spear stab, attribute modifiers in the server's order; 579 -> 463 on its base. Bot round 7: m_flow_trade_legacy (merchant offers fed to the fork, pre-1.11 zero-count shift-click), f_elytra_mounted_mid_glide 1.21.2-26.2 (boat lerp target, pose-dependent seat, passenger glide), 1.8.8 pig click; 553 -> 347 on its base, 0 BAD, none new. Both merged in fa3b3203; the combined count was not re-measured. Left: q_eat_speed_zero 1.21.11+ (recorder must save item components, then re-record), eo_cow_before_player_after_respawn 1.11.2-1.13.2 (pre-1.14 entitySlots in mineflayer-physics-utils), boat/chest-boat dismount 1.21.2+ and pl_attack_sprint_boat, 1.9.4/1.10.2 horse mount clicks, cf_ride_horse_stops_in_web, f_elytra_mounted_mid_glide on 26.3, pm_ride_minecart_lifted, pm_step_moving_slab_d3 (1 ulp), interpolation ulps, small groups listed in docs/backlog-progress.md.
- B282: see B221: at most 347 bot-layer differences remain across 50 releases, each in a named group; the horse random rear is the only documented limitation so far. Not closed: the remaining groups are not yet fixed or source-explained.

## Bot replay complete on all 50 releases (bot rounds 8-9, carts2 rounds 3-5)

Commits: lane/carts-rebased c7b38a50 27cded43, lane/carts2 2bc55859 0cca04bb, merge 8442b814, 50be2179; fork mc-physics-demo 07fb7558.

- B221: definitive 50-release --bot run at 50be2179 (fork 07fb7558): 170,879 pinned recordings, 170,728 matching, 151 limitation, 0 different, 0 unsupported, 0 codec; known-failures.json empty. The 151 are two documented client-only causes the report detects from the rows (the recorder's replay/limitations.ts): a horse's random rear (149, 1.9-1.19.2; docs/limitations.md with 1.16.5 Mob.java:248-251, AbstractHorse.java:385-401, 714-719) and the 1.8 shared piston-head bounds set by the per-frame crosshair pick (2, 1.8.8; PistonExtensionBlock.java:121-133, Block.java:401-405, 512-514). Teleport acknowledgements, corrections, velocity, explosions, effects, mounts, screens and world updates are exercised; C2S packets compared by name, order and bytes including packet-handling phases.
- B282: every required scenario matches on all 50 releases in the bot, adapter and native layers (B221, conformance --phases --continuous complete on all 50); the only exclusions are the two visible client-only limitations in docs/limitations.md, reported as `limit`, never as a pass; an absent or reduced corpus fails the gate (release-matrix, conformance-cli).

## Prediction verified; every subsystem family verified on all 50 releases (lane/prediction 8fa53ec9)

Commits: b3ddd471 e398b22a 8fa53ec9.

- B026: prediction verified on 50/50 (docs/subsystem-matrix.md, generated/subsystem-matrix.json: unexplained []): the 50-release --bot run at 50be2179 (170,879 recordings, 0 different, 151 documented client-only limits) and --bot-carts on all 50 (18 or 22 matching each, 0 unexpected) after b3ddd471 (bot-carts pushes the cart with the world's moving pistons) and e398b22a (1.15-1.16.1 near teleport lerps over 3 steps, 1.15 ClientPacketListener.java:581-582). 1.14.4, 1.15.2 and 1.16.5 each have their exact decompiled source in source-cache, per-release corpus manifests with hashes, and zero unexplained mismatches in every family.
- B016: every requested family (travel, entity, fluid, collision, vehicle, projectiles, effects, attributes, poses, tick-order, prediction) is verified on 50/50 releases with every gated function source-compared and every boundary covered; shared paths carry equivalence reviews (B280) and each version gate is mutation-proven (B290). No family earns its status from a profile or shared unit tests: each is per-release replay evidence (generated/subsystem-matrix.json).
- B280: generated/shared-equivalence.json: 0 in review, 0 uncited, 0 unresolved, 0 open function-boundaries, profile rules 24/24; every version gate caught or source-justified on both sides on all 50 releases (B290, docs/version-gates.md); no family relies on a shared fallback without a bounded claim.
- B281: every required subsystem family is individually evidenced and verified on 50/50 releases (subsystem matrix, unexplained []); all 69 findings settled (67 fixed-validated, 2 duplicate); no required capability left unsupported.
- B289: no matrix cell is partial, approximate, untested or inherited: all 11 families verified on 50/50; the only exclusions are two documented client-only limitations, listed per release as excluded on purpose with docs/limitations.md rows and detected from the rows (the recorder's replay/limitations.ts); each closed mismatch has its pinned recording.

## Long-run determinism confirmed (B283)

Commits: 50be2179 (addon 83526487b916).

- B283: the determinism corpus (scripts/determinism/corpus.cjs, regenerated for world checkpoint schema MCPNWLD3) replayed twice on the same pinned addon, 5000 ticks a scene: 600 runs, 3,000,000 ticks on all 50 releases, 0 differences (scripts/determinism/compare.cjs). Continuous conformance (--continuous carries native output into the next tick) is complete on all 50 releases, and the recorder's replay/test/continuous.test.ts perturbs persistent state (a nudge, a lost field) and asserts the continuous replay detects the divergence that per-tick reseeding hides.
