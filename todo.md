# Mineflayer Physics Engine

## Audit scope and status

Updated 2026-11-03 against commit
`5019aa1de1960a32a752185d77b0cfe7aa659f11`.

`[x]` records the specific implementation or checked-in artifact described in
the item. It does not certify vanilla parity, a passing test run, or coverage
of every release. `[ ]` means the complete requirement remains unverified or
unfinished; several open requirements have implemented subsets listed beside
them. Test execution results are recorded separately below.

The previous checklist's repeated historical completions have been consolidated
into entries tied to the current source. The full parity backlog remains open.
No release is certified as 1:1 complete by this audit: the checkout contains
regression tests and a trace comparator, but no checked-in vanilla execution
harness or golden trace corpus establishing that claim.

### Registered versions

The runtime registry in [native/src/versioned/mod.rs](native/src/versioned/mod.rs)
accepts 50 exact release strings. These are implementation registrations, not
50 completed conformance certifications.

| Group | Exact registered releases | Native modules | Generated release tables |
| --- | --- | --- | --- |
| Legacy | `1.8.8`, `1.9.4`, `1.10.2`, `1.11.2`, `1.12.2`, `1.13.2` | 6 | 0 |
| 1.14 | `1.14.4` | 1 | 1 |
| 1.15 | `1.15`, `1.15.1`, `1.15.2` | 3 | 3 |
| 1.16 | `1.16`, `1.16.1`, `1.16.2`, `1.16.3`, `1.16.4`, `1.16.5` | 6 | 6 |
| 1.17 | `1.17`, `1.17.1` | 2 | 2 |
| 1.18 | `1.18`, `1.18.1`, `1.18.2` | 3 | 3 |
| 1.19 | `1.19`, `1.19.1`, `1.19.2`, `1.19.3`, `1.19.4` | 5 | 5 |
| 1.20 | `1.20`, `1.20.1`, `1.20.2`, `1.20.3`, `1.20.4`, `1.20.5`, `1.20.6` | 7 | 7 |
| 1.21 | `1.21`, `1.21.1`, `1.21.2`, `1.21.3`, `1.21.4`, `1.21.5`, `1.21.6`, `1.21.7`, `1.21.8`, `1.21.9`, `1.21.10`, `1.21.11` | 12 | 12 |
| 26.x | `26.1`, `26.1.1`, `26.1.2`, `26.2`, `26.3` | 5 | 5 |
| Total | | 50 | 44 |

[generated/minecraft-versions.json](generated/minecraft-versions.json), the
block registry, block-state definitions, physics-method audit, and
[native/data/releases](native/data/releases) each cover the same 44 non-legacy
releases. The block-state manifest reports zero unresolved property domains;
that is a property-domain result, not proof of complete shapes or behavior.
The six legacy modules are explicitly accepted by `is_legacy` without generated
release tables. The other 44 registrations require generated water/lava entries.
Neither path checks for passing vanilla traces at runtime.

### Validation on this checkout

Environment: Linux x64, Node `v26.12.1`, pnpm `12.15.0`, Cargo `1.99.1`.
Dependencies were installed with `pnpm install --frozen-lockfile`.
The completed Rust test run used `CARGO_BUILD_JOBS=1`,
`CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0` to limit build
memory. These settings change build parallelism/debug information, not source.

| Check | Observed result |
| --- | --- |
| `pnpm build` | Passed. |
| `pnpm lint` | Passed. |
| `pnpm test` with the checked-in native artifacts | 15 tests failed to load the native addon on Linux; 0 passed. This does not establish 15 physics mismatches. |
| `node --test 'test/*.test.cjs'` with a locally compiled Linux addon | 15 passed, 0 failed across 3 suites. |
| `node scripts/native-smoke.cjs` with that local addon | Failed at line 197: `fluidHeightAt(1, 0, 0)` returned `0.8888888955116272`; the assertion expects `1`. Assertions after that failure were not reached. |
| `cargo test --locked --manifest-path native/Cargo.toml` with the build settings above | 446 passed, 0 failed, 0 ignored; 0 doc tests. |
| `node scripts/verify-native-package.cjs` | Failed: the verifier expects 43 generated release modules, but the checkout contains 44. |
| `node scripts/verify-version-coverage.cjs` | Blocked by missing external `../mc-decomp/src` (`ENOENT`). Source fingerprints could not be revalidated. |
| `npm pack --dry-run --json` after TypeScript compilation | Completed. The file list includes only Windows `.node` binaries, plus generation/audit JSON and `test/mineflayer.test.cjs`. This is not a clean-consumer installation test. |

The tracked binaries are `index.win32-x64-msvc.node` and
`index.win-32-x-64-msvc.node`; the loader references the former naming pattern.
No tracked Linux or macOS binary is present. The adapter/smoke reruns loaded a
local debug build of the checked-in Rust/data through
`NAPI_RS_NATIVE_LIBRARY_PATH`, with the compiled `.so` copied to a `.node` file.
This bypasses source regeneration and does not validate release packaging.
`native:test` and
`native:verify-package` first run `native:build`, which regenerates data from the
external source tree; successful regeneration and release packaging were not
established here.

## Definition of 1:1 parity

- [ ] Define the exact parity target for every supported entity and interaction.
- [ ] Define the authoritative vanilla source for each release and patch.
- [ ] Reproduce the same tick order, method order, branch conditions, constants,
      floating-point operations, clamping, rounding, and state mutations.
- [ ] Reproduce server-authoritative behavior where Mineflayer receives server
      state, and client-prediction behavior where the client predicts movement.
- [ ] Reproduce observable outputs: position, velocity, bounding box, pose,
      collision flags, fluid flags, fall distance, fall damage, passengers,
      vehicle state, effects, and emitted interaction events.
- [ ] Never use a nearest-version approximation for a requested version.
- [ ] Never use a generic implementation when the target version has a
      different vanilla implementation.
- [ ] Reject a version when its source, generated data, and conformance tests
      are not present.

## Current next work priorities

- [ ] Repair the package verifier's release count and validate all 44 generated
      releases against the manifest, not a fixed historical count.
- [ ] Make the native build/test path reproducible on Linux with pinned source
      inputs and portable paths, and publish/load the correct platform artifact.
- [ ] Run Rust, native smoke, and adapter suites successfully; preserve exact
      failures separately from unavailable prerequisites.
- [ ] Reconcile the first failing native smoke assertion at
      `scripts/native-smoke.cjs:197` with source-fluid height semantics and the
      Rust regression `source_fluid_is_eight_ninths_tall_without_the_same_fluid_above`.
      Re-run the full smoke script afterward; later assertions remain untested.
- [ ] Add the vanilla harness and versioned golden traces needed to establish
      parity beyond the existing regression cases.
- [ ] Audit the packed `NativePlayer`/Mineflayer state bridge against the object
      API, including damage/callback outputs and attribute edge cases.
- [ ] Continue source-specialized travel, collision, entity, vehicle, effect,
      and prediction conformance work.
- [ ] Re-run targeted validation after every substantive physics behavioral fix;
      do not claim parity completion while the behavior test matrix remains red.

## Version architecture

Evidence: [native/src/versioned/mod.rs](native/src/versioned/mod.rs),
[native/src/simulation.rs](native/src/simulation.rs),
[src/index.ts](src/index.ts), and [src/mineflayer/index.ts](src/mineflayer/index.ts).

- [x] Provide a dedicated Rust `Engine` and `Profile` module, exact dispatch
      arm, and explicit behavior contract for each of the 50 registered releases.
- [x] Declare release-specific movement constants, fluid policies, and landing
      rounding policies in the concrete profiles.
- [x] Register `26.3`, including its generated data and restitution path.
- [x] Export `NativeEngine`, `NativePhysics`, `NativeBoatPhysics`,
      `NativeRiddenMobPhysics`, `NativeWorldSync`, and the native binding types.
- [ ] Verify every shared implementation against each release it serves.
      Dedicated profiles still call shared simulation, collision, vehicle, and
      projectile code containing version branches.
- [ ] Restore or explicitly retire the original per-version TypeScript class
      and factory requirement. The current public API uses registry/options
      constructors; it has no per-release TypeScript class registry.
- [ ] Document deterministic boundary inputs for behavior Mineflayer cannot
      observe, including entity tick order, piston progress, and random impulses.

## Version and source matrix

Evidence: [generated/minecraft-versions.json](generated/minecraft-versions.json),
[scripts/generate-version-manifest.cjs](scripts/generate-version-manifest.cjs),
and [native/src/versioned/mod.rs](native/src/versioned/mod.rs).

- [x] Store source file hashes, tracked-method hashes, incomplete-source
      records, and method diffs for the 44 manifest releases.
- [x] Explicitly register the six legacy releases listed in the version table.
- [ ] Build an authoritative release inventory from Mojang metadata. The current
      manifest generator enumerates locally supplied source directories.
- [ ] Add protocol IDs, mappings revisions, and data revisions to the manifest;
      those fields are absent from the current manifest schema.
- [ ] Add source/data provenance for the six legacy runtime registrations.
- [ ] Make source availability, complete generated data, and conformance status
      explicit admission requirements rather than equating dispatch with parity.

- [ ] Include release candidates and snapshots only when explicitly supported.
- [ ] Maintain explicit aliases only when a verified source/data diff proves the
      behavior is identical.
- [ ] Generate the version registry and TypeScript exports from the manifest.
- [ ] Add 1.14.4, 1.15.2, and 1.16.5 as the first fully verified snapshots.
- [ ] Check release completeness against authoritative metadata. All listed
      1.15 through 1.21.11 patch strings already have native registrations.
- [ ] Add 1.21.2 minecart/vehicle changes as a separate implementation when
      source behavior differs.
- [ ] Add 1.21.5, 1.21.6, 1.21.9, and 1.21.11 breakpoints independently when
      their source behavior differs.
- [ ] Verify the registered `26.1`, `26.1.1`, `26.1.2`, `26.2`, and `26.3`
      sources/data and add any further explicitly requested 26.x release.
- [ ] Define conformance scope for the already enabled legacy releases and
      `1.14.4`; their presence no longer matches the old out-of-phase assumption.

## Source extraction and code generation

Evidence: [scripts](scripts), [generated](generated), and
[native/data/mod.rs](native/data/mod.rs).

- [x] Provide generators for version fingerprints, block registries, block-state
      definitions, Rust release tables, tracked physics methods, and breakpoints.
- [x] Store `Entity.move`, `LivingEntity.travel`, and `Player.travel` method
      entries for all 44 releases in the checked-in physics-method audit.
- [x] Generate block property domains/defaults, state counts and indices,
      water/lava state definitions, tag records, and source fingerprints.
- [x] Split Rust data into 44 release modules with shared lookup functions.
- [x] Distinguish direct shape methods from inherited methods, retain per-method
      fingerprints/dependencies/predicates, and expose literal box candidates.
- [x] Refuse state-dependent methods in the generated exact-shape lookup and
      validate supplied state tuples at the state-qualified N-API entry point.
- [x] Pool generated shape/dependency arrays and cache method bodies and source
      chains; default source-index concurrency to host parallelism capped at
      eight, with `GENERATOR_WORKERS` clamped to the range one through eight.
- [ ] Supply pinned external decompiled-source inputs and re-run their hash
      checks. Checked-in hashes cannot establish that unavailable inputs match.
- [ ] Normalize source/output paths across platforms. The version generator's
      defaults and checked-in `sourceRoot` contain Windows separators; several
      downstream generators resolve that manifest path directly.

- [ ] Extract projectile motion and collision methods.
- [ ] Extract entity dimensions, poses, eye heights, and passenger offsets.
- [ ] Extract block collision, outline, interaction, support, friction,
      slipperiness, fluid, and movement-effect behavior.
- [ ] Extract voxel-shape composition, axis clipping, and collision iteration.
- [ ] Extract attributes, modifiers, effects, enchantments, and equipment
      modifiers relevant to movement.
- [ ] Preserve source operation order in generated intermediate representations.
- [ ] Complete extraction of all required Rust movement constants and lookup
      tables; existing block/fluid tables do not cover every physics rule.
- [ ] Generate source provenance for every generated rule and table entry.
- [ ] Make generation deterministic and reproducible on Windows, Linux, and
      macOS.
- [ ] Fail generation on unresolved mappings, missing methods, unsupported
      bytecode, or ambiguous behavior.
- [ ] Add generated-data integrity checks to CI.

## Core simulation model

Evidence: [native/src/simulation.rs](native/src/simulation.rs),
[native/src/napi_api.rs](native/src/napi_api.rs), and
[native/src/napi_player.rs](native/src/napi_player.rs).

- [x] Define `SimulationState`, `TickInputSnapshot`, and `TickOutputSnapshot` for
      the currently represented fields. Complete vanilla state remains open.
- [x] Carry gravity, step height, no-gravity, local-control, passenger, pose,
      fall metrics, effects, and movement-attribute inputs in the native state.
- [x] Preserve state between calls with `NativePlayer`, packed input/output
      arrays, explicit state updates, nearby-entity inputs, and piston pushes.
- [x] Implement tick counters, jump cooldown, auto-jump, riptide, stuck-motion,
      previous-position, and pre-1.17 retained-bounding-box state.
- [x] Implement versioned unloaded-column sinking and removed-player movement
      branches, with regression cases in `simulation_tests.rs`.
- [ ] Audit the object API, packed player API, and Mineflayer adapter field by
      field. Their state surfaces differ; native availability alone does not
      establish adapter input/output preservation.

- [ ] Represent all vanilla entity flags and tracked state that affects motion.
- [ ] Represent exact numeric types and conversion boundaries used by each
      version.
- [ ] Preserve Java float behavior where vanilla uses float.
- [ ] Preserve Java double behavior where vanilla uses double.
- [ ] Define NaN, infinity, overflow, and invalid-input handling explicitly.
- [ ] Implement exact tick ordering, including pre-tick, movement, collision,
      post-movement, effects, fall distance, and event phases.
- [ ] Implement deterministic random state where random values affect physics.
- [ ] Implement entity age, tick counters, and cooldowns that affect movement.
- [ ] Implement loaded/unloaded chunk behavior and client prediction behavior.
- [ ] Implement world-border and dimension boundary behavior.
- [ ] Implement portal and teleport state where it affects movement.
- [ ] Implement removal, riding, sleeping, death, and spectator state handling.

## Mathematics and geometry

Evidence: [native/src/math.rs](native/src/math.rs),
[native/src/trig.rs](native/src/trig.rs), and
[native/src/voxel_shape.rs](native/src/voxel_shape.rs).

- [x] Implement vector arithmetic, normalization, products, interpolation,
      rotations, direction vectors, and numeric helpers with unit-test cases.
- [x] Implement AABB transforms, containment/intersection, distances, segment
      clipping, and ray broad-phase checks with boundary regression cases.
- [x] Implement indexed voxel-box unions, empty/bounds queries, axis clipping,
      degenerate-box filtering, and block-centered quarter turns.
- [x] Provide quantized `Mth` lookup trigonometry and native sine/cosine routines
      with Java-value regression fixtures. Cross-platform identity remains a
      separate unverified requirement.

- [ ] Implement Java-compatible vector arithmetic.
- [ ] Implement Java-compatible trigonometric functions and angle conversion.
- [ ] Implement normalization, length, horizontal length, dot products, and
      cross products.
- [ ] Implement exact floor, ceil, round, clamp, and epsilon semantics.
- [ ] Implement axis-aligned bounding boxes with exact inclusive/exclusive
      boundary behavior.
- [ ] Implement box inflation, deflation, expansion, contraction, movement,
      intersection, containment, and ray clipping.
- [ ] Implement shape simplification only when vanilla performs it.
- [ ] Implement collision epsilon and contact behavior.
- [ ] Add exhaustive geometry tests for touching, overlapping, negative, and
      very large coordinates.

## World representation

Evidence: [native/src/world.rs](native/src/world.rs),
[native/src/napi_api.rs](native/src/napi_api.rs), and
[src/mineflayer/world.ts](src/mineflayer/world.ts).

- [x] Store version-qualified block identities, properties, tags, block-entity
      kind metadata, friction, movement effects, and independent collision,
      outline, and interaction box channels.
- [x] Store fluid levels, source/falling flags, tags, and flow vectors; expose
      own/same-fluid-above height queries and direct/batched mutation APIs.
- [x] Distinguish absent blocks from explicit air, track loaded chunks and
      section presence, and store configurable height limits and world borders.
- [x] Implement incremental and batched updates, negative-coordinate chunk
      indexing, and deterministic collision-query ordering.
- [x] Synchronize nearby prismarine blocks into `NativeWorld`; use change and
      chunk events when available, and rescan worlds without event support.
- [x] Check `getColumnAt` before treating state ID zero as a loaded air column.
- [ ] Audit adapter shapes and world properties against source per release.
      The adapter consumes prismarine block geometry and explicit mappings;
      generated literal candidates are not a complete runtime shape registry.

- [ ] Store block entities only where they affect collision or movement.
- [ ] Implement chunk sections, height limits, min build height, and world
      border per version.
- [ ] Implement collision queries without lossy conversion through JavaScript.
- [ ] Generate complete block tags used by movement and collision.
- [ ] Generate version-specific shape tables from vanilla registries.
  - [ ] Resolve state-dependent shape branches into exact per-state geometry;
        literal candidates alone are not accepted as final parity.
- [ ] Verify generated shape tables against every state in each version.

## Entity base behavior

Evidence: [native/src/entity.rs](native/src/entity.rs),
[native/src/simulation.rs](native/src/simulation.rs),
[native/src/piston.rs](native/src/piston.rs), and
[native/src/simulation_tests.rs](native/src/simulation_tests.rs).

- [x] Implement no-physics movement, versioned collision/on-ground updates,
      support-block lookup, inside-wall checks, edge backoff, and pose refresh.
- [x] Implement pairwise entity pushes, pushthrough policies, same-vehicle
      exclusions, and local-player nearby-entity tick ordering.
- [x] Return the impulses applied back to nearby entities and expose them as
      `pushedEntities` through the player adapter.
- [x] Apply supplied moving-piston snapshots after the local-player tick, with
      legacy branches and per-axis accumulated displacement limits.
- [x] Implement fall accumulation/reset, safe-fall-distance, immunity,
      multiplier, versioned ceil/floor policies, and block-fall callback state.
- [ ] Verify zero-Y collision flags and stepping per release. The old blanket
      preservation/reset claims were contradictory; the current code has
      version-specific paths that require separate conformance fixtures.

- [ ] Port `Entity` construction, dimensions, position, rotation, and velocity.
- [ ] Port `Entity.move` exactly for every version.
- [ ] Port movement type semantics.
- [ ] Port collision flags and their reset/update rules.
- [ ] Port supporting-block and stepping detection.
- [ ] Port suffocation, inside-wall, and border checks where movement-visible.
- [ ] Port piston movement and entity displacement.
- [ ] Port entity push and collision with other entities.
- [ ] Port riding, passenger offsets, dismount checks, and vehicle attachment.
- [ ] Port teleport, interpolation, and synchronization state where exposed.
- [ ] Port fire, freezing, powder snow, and environmental movement effects.
- [ ] Port fall-distance accumulation and landing reset.
- [ ] Port safe-fall-distance and fall-damage calculations.
- [ ] Port entity dimensions and pose refresh transitions.

## Living entities and players

Evidence: [src/mineflayer/player.ts](src/mineflayer/player.ts),
[native/src/simulation.rs](native/src/simulation.rs), and
[native/src/simulation_tests.rs](native/src/simulation_tests.rs).

- [x] Implement a Mineflayer player adapter with sprint/crouch/swim/fly input
      transitions, pose fitting, scale, yaw/pitch handling, and native ticks.
- [x] Carry the synced sprint modifier separately from sprint input, including
      speed/slowness modifier identification and previous sprint state.
- [x] Implement ground/fluid/climbable jumping, cooldowns, sprint-jump impulse,
      auto-jump, riptide launch, and legacy movement branches.
- [x] Forward item-use speed multipliers and removed-player state through the
      packed player input boundary.
- [x] Apply native position, velocity, collision/fluid flags, fall distance,
      jump counters, stuck multiplier, and retained box back to adapter state.
- [ ] Audit the remaining state bridge, including native fall damage, freeze
      and contact damage, block-fall callbacks, and fall-damage immunity.
      `NativePhysics.takeOutput` does not propagate every object-API field.

- [ ] Port `LivingEntity.travel` for every version.
- [ ] Port `Player.travel` and client-local movement overrides.
- [ ] Port `LocalPlayer` input and prediction-specific movement.
- [ ] Port base movement speed and all attribute modifiers.
- [ ] Port sprinting requirements, sprint transitions, and sprint multiplier.
- [ ] Port sneaking movement and edge-safe movement.
- [ ] Port jumping, jump delay, jump cooldown, and jump cancellation.
- [ ] Port jump boost and jump strength.
- [ ] Port air, ground, water, lava, and climbable movement.
- [ ] Port swimming pose and swimming acceleration completely per source
      release.
- [ ] Port crawling-specific pose selection and swimming acceleration.
- [ ] Port flying, creative flight, spectator flight, and ability flags.
- [ ] Port no-gravity and custom-gravity behavior.
- [ ] Port knockback, velocity impulses, and velocity damping.
- [ ] Port movement input normalization and yaw/pitch transforms.
- [ ] Port movement statistics only when they affect exhaustion or state.
- [ ] Port food/exhaustion movement effects where applicable.
- [ ] Port shield-use, item-use, and attack movement penalties.
- [ ] Port eye height, camera height, and fluid-eye checks.
- [ ] Port world-border collision and unloaded-world fallback behavior.

## Status effects and attributes

Evidence: [src/mineflayer/base.ts](src/mineflayer/base.ts),
[src/mineflayer/player.ts](src/mineflayer/player.ts), and
[native/src/simulation.rs](native/src/simulation.rs).

- [x] Resolve attribute resource keys from the supplied prismarine registry,
      apply modifier operation order, and clamp resolved values to its ranges.
- [x] Forward player gravity, step height, jump strength, safe fall distance,
      sneaking speed, movement/water efficiency, drag/friction modifiers,
      bounciness, and the represented effects/enchantments.
- [x] Separate legacy Depth Strider interpolation from modern water-movement
      efficiency in concrete version profiles.
- [x] Implement Slow Falling, Levitation, Speed/Slowness, Jump Boost,
      Dolphin's Grace, Soul Speed, Swift Sneak, and Weaving movement branches.
- [ ] Define and test compact attribute objects without `modifiers`. The
      current attribute helpers iterate that required array; the old claim
      that missing arrays are accepted is not supported by this adapter.
- [ ] Add the previously claimed generic `EntityState` attribute forwarding
      and clone/merge guarantees, or remove them from the API plan. This
      checkout has no repository-owned generic TypeScript `EntityState` class.

- [ ] Implement the complete movement-relevant attribute system.
- [ ] Implement base values, additive modifiers, multiplicative modifiers, and
      operation order.
- [ ] Implement version-specific attribute names and defaults.
- [ ] Implement movement speed, gravity, step height, safe fall distance, scale,
      jump strength, water movement efficiency, and attack-related movement
      attributes where present.
- [ ] Implement speed, slowness, jump boost, levitation, slow falling, dolphins
      grace, conduit power, blindness, darkness, and movement-relevant effects.
- [ ] Implement effect amplifier and duration semantics.
- [ ] Implement effect tick ordering and expiration.
- [ ] Implement equipment and enchantment modifiers.
- [ ] Implement Depth Strider, Soul Speed, Swift Sneak, Frost Walker, and all
      movement-relevant enchantments per version.
- [ ] Implement effect interactions with fluids, blocks, poses, and vehicles.

## Fluids

Evidence: [native/src/legacy_fluid.rs](native/src/legacy_fluid.rs),
[native/src/world.rs](native/src/world.rs),
[native/src/simulation.rs](native/src/simulation.rs), and
[src/mineflayer/blocks.ts](src/mineflayer/blocks.ts).

- [x] Implement legacy and modern water/lava sampling, fluid current
      aggregation, height/eye predicates, and ultra-warm lava current inputs.
- [x] Select travel splitting, fluid-falling conditions, water efficiency,
      drag, input speed, and gravity divisors through version profiles.
- [x] Implement player swimming steering, fluid jump/escape checks,
      waterlogged continuation, and bubble-column direction/surface handling.
- [x] Add regression cases for legacy fluid boundaries and modern bubble-column
      traversal changes, including 1.21.5, 1.21.9, and 1.21.10.

- [ ] Implement water source, flowing, falling, and level behavior.
- [ ] Implement lava source, flowing, falling, and level behavior.
- [ ] Implement waterlogged block behavior.
- [ ] Implement fluid height and eye-in-fluid checks.
- [ ] Implement fluid flow vectors and current acceleration.
- [ ] Implement water drag and input speed per version.
- [ ] Implement lava drag and input speed per version.
- [ ] Implement fluid jump thresholds.
- [ ] Implement bubble columns, direction, drag, and entity effects.
- [ ] Implement fluid interactions with ladders, vines, scaffolding, and webs.
- [ ] Implement fluid interaction with boats, striders, fish, and other aquatic
      entities.
- [ ] Implement fluid collision and standing-on-fluid rules.
- [ ] Implement fluid behavior around partial and waterlogged shapes.
- [ ] Generate fluid tags and fluid-state tables per version.

## Blocks and movement surfaces

Evidence: [src/mineflayer/blocks.ts](src/mineflayer/blocks.ts),
[src/mineflayer/legacy.ts](src/mineflayer/legacy.ts),
[native/src/block_traversal.rs](native/src/block_traversal.rs), and
[native/src/simulation.rs](native/src/simulation.rs).

- [x] Map prismarine geometry, selected friction values, fluids, climbability,
      suffocation predicates, offsets, and block movement effects to native data.
- [x] Resolve legacy neighbor-dependent shapes, including fence connections.
- [x] Implement stuck-motion effects, honey sliding, slime/bed bounce, soul
      block slowdown, powder-snow collision/freezing, and contact-damage signals.
- [x] Implement movement-path block traversal with release-specific branches
      for 1.21.2, 1.21.5, 1.21.9, and 1.21.10.
- [x] Add the `26.3` shelf-mushroom bed-effect mapping and a regression case
      for the restitution threshold change at a landing speed equal to gravity.

- [ ] Generate every block-state collision shape.
- [ ] Generate every block-state outline/support shape used by movement.
- [ ] Generate friction and slipperiness exactly.
- [ ] Implement ice, packed ice, blue ice, and version-specific slipperiness.
- [ ] Implement honey block movement and side adhesion.
- [ ] Implement slime block bounce and sneaking behavior.
- [ ] Implement cobweb movement.
- [ ] Implement powder snow freezing damage and immunity behavior.
- [ ] Implement berry bush, cactus, magma, campfire, and damage surfaces where
      movement state is affected.
- [ ] Implement carpets, snow layers, slabs, stairs, fences, walls, gates,
      trapdoors, doors, beds, signs, rails, and all partial shapes.
- [ ] Implement piston movement and moving-block collision.
- [ ] Implement honey/slime piston interactions where entity movement changes.
- [ ] Implement block tags and predicates used by vanilla movement methods.

## Collision and stepping

Evidence: [native/src/collision.rs](native/src/collision.rs),
[native/src/simulation.rs](native/src/simulation.rs), and
[native/src/voxel_shape.rs](native/src/voxel_shape.rs).

- [x] Implement swept box queries, voxel-grid clipping, axis-order selection,
      and collision tolerances with focused regression cases.
- [x] Keep separate legacy stepping, lifted stepping through 1.20.6, and
      candidate-height stepping from 1.21; both modern algorithms still exist.
- [x] Implement world-border collision, sneaking edge backoff, and collision
      with supplied nearby entities and moving-piston snapshots.

- [ ] Port vanilla swept movement against voxel shapes.
- [ ] Port exact axis clipping order.
- [ ] Port horizontal collision and vertical collision flags.
- [ ] Port step-up candidate generation and selection.
- [ ] Port step-down and landing behavior.
- [ ] Port sneaking edge prevention.
- [ ] Port collision behavior at exact block boundaries.
- [ ] Port entity collision and push resolution.
- [ ] Port collision against moving pistons and entities.
- [ ] Port world-border collision.
- [ ] Port shape emptiness, face culling, and support checks.
- [ ] Test all combinations of partial blocks, diagonal movement, corners,
      stairs, slabs, fences, walls, and negative coordinates.

## Elytra and fall flying

Evidence: [src/mineflayer/player.ts](src/mineflayer/player.ts),
[native/src/simulation.rs](native/src/simulation.rs), and
[native/src/napi_api.rs](native/src/napi_api.rs).

- [x] Implement glide steering/lift/drag calculations and adapter flight/pose
      transitions, including creative-flight interactions.
- [x] Apply attached-firework boosts before or after the player according to
      caller-supplied rocket tick ordering; retain adapter duration state.

- [ ] Port fall-flying eligibility and activation.
- [ ] Port pitch/look-vector glide equations per version.
- [ ] Port horizontal steering, lift, drag, and descent limits.
- [ ] Port collision damage and wall impact handling.
- [ ] Port landing, fall-distance, and flag reset behavior.
- [ ] Port firework propulsion, duration, and vector semantics.
- [ ] Port rocket/entity interaction and invalid propulsion handling.
- [ ] Port Elytra durability state only where it affects flight.

## Vehicles

Evidence: [native/src/vehicle.rs](native/src/vehicle.rs),
[native/src/ridden.rs](native/src/ridden.rs), and
[src/mineflayer/vehicles.ts](src/mineflayer/vehicles.ts).

- [x] Implement version-validated boat and ridden-mob tick entry points and
      Mineflayer `NativeBoatPhysics`/`NativeRiddenMobPhysics` adapters.
- [x] Implement boat water statuses, buoyancy/drag, steering and delta rotation,
      bubble-column state, swept collision, ground contact, and passenger offsets.
- [x] Implement ridden kinds for pigs, striders, camels/camel husks, happy
      ghasts, nautiluses/zombie nautiluses, horses, donkeys, mules, skeleton/zombie
      horses, llamas, and trader llamas. This lists implemented dispatch kinds,
      not proof that every entity/version combination is valid or complete.
- [x] Implement rider input, horse jump/rearing state, pig/strider boost state,
      camel/nautilus dash state, and specialized flying/aquatic travel branches.
- [x] Implement ridden-mob teleport interpolation and horse stand animation;
      adapter regression cases cover these paths.
- [ ] Add an executable minecart physics path. The physics-method audit includes
      Java minecart methods, but `VehicleKind` currently contains only `Boat`
      and `Horse`.

- [ ] Port boat status detection and water-height calculations.
- [ ] Port boat paddling, steering, acceleration, drag, and land movement.
- [ ] Port boat bubble columns, waves, fall, and collision behavior.
- [ ] Port horse, donkey, mule, skeleton horse, and zombie horse travel.
- [ ] Port horse jump strength, jump power, movement speed, and attributes.
- [ ] Port horse rider input and diagonal movement.
- [ ] Port horse water behavior and dismount behavior.
- [ ] Port pig, strider, camel, and all rideable entities in supported versions.
- [ ] Port minecart rail following, slopes, acceleration, drag, and collision.
- [ ] Port powered, furnace, hopper, TNT, chest, and command minecart behavior
      where movement differs.
- [ ] Port post-1.21.2 experimental/reworked minecart behavior when supported.
- [ ] Port mount/flying behavior introduced in later versions.

## Projectiles and moving entities

Evidence: [native/src/projectile.rs](native/src/projectile.rs) and
[native/src/napi_api.rs](native/src/napi_api.rs).

- [x] Implement generic projectile family dispatch, ID normalization,
      gravity/drag defaults, block tracing, and first-tick state transport.
- [x] Implement specialized arrow and trident paths, arrow in-ground lifetime,
      shake state, supporting-state identity, and explicit start-falling impulse.
- [x] Implement firework acceleration/shot-at-angle handling, fireball power
      acceleration, and the modern thrown-projectile bubble-column prelude.
- [x] Expose versioned batched projectile, arrow, and trident operations.

- [ ] Port arrow, spectral arrow, tipped arrow, trident, snowball, egg, pearl,
      potion, firework, fishing hook, and other projectile motion.
- [ ] Port projectile gravity, drag, water drag, and collision.
- [ ] Port projectile target tolerance and version-specific changes.
- [ ] Port entity hitboxes and block hitboxes for projectile ray tracing.
- [ ] Port TNT, falling blocks, experience orbs, items, and environmental
      moving entities where Mineflayer prediction requires them.
- [ ] Port dragon, vex, guardian, fish, squid, flying mobs, and special travel
      overrides exposed by the supported entity set.

## N-API-RS and TypeScript surface

Evidence: [native/src/napi_api.rs](native/src/napi_api.rs),
[native/src/napi_player.rs](native/src/napi_player.rs),
[src/native.ts](src/native.ts), and [src/index.ts](src/index.ts).

- [x] Expose native world/state APIs, shape provenance/lookups, versioned ticks,
      pose refresh, entity pushes, and boat/horse/projectile batch operations.
- [x] Expose bulk ticking, versioned multi-entity ticking, and a persistent
      `NativePlayer`.
- [x] Keep native collision/simulation loops operating on Rust world snapshots
      without JavaScript callbacks inside those loops.
- [x] Reject unsupported releases at version-validated native entry points.
      Adapter constructors retain the requested version; validation may occur
      when the native player is created or a vehicle tick is requested.
- [x] Keep generated native declarations and a manually maintained TypeScript
      binding interface in the repository; compiling the latter alone does not
      verify agreement with the binary or generated declarations.
- [ ] Add a clean integration test using the external
      `EPhysicsCtx.FROM_BOT(physics, bot)` and actual dependency-owned states.
      Existing adapter tests construct plain state objects. The default loader
      only attaches `bot.NativePhysics`; it does not install a ticking adapter.

- [ ] Define version-specific Rust constructors and lifecycle ownership.
- [ ] Define version-specific TypeScript classes with stable names.
- [ ] Implement `FROM_BOT -> version -> engine` construction.
- [ ] Add direct constructors for every supported version.
- [ ] Expose complete world, block-state, fluid, entity, player, and vehicle
      inputs without lossy coercion.
- [ ] Expose complete output state, including all flags and derived values.
- [ ] Validate all inputs and report errors with actionable messages.
- [ ] Preserve ownership and lifetime guarantees across N-API boundaries.
- [ ] Generate declarations from the Rust bindings and verify them in TypeScript.
- [ ] Remove obsolete generic version gates once version-specific engines cover
      the same behavior.
- [ ] Provide migration adapters only outside the core 1:1 engine.
- [x] Wire player and vehicle snapshots plus nearby prismarine world updates
      into native ticks through the current Mineflayer adapters.
- [ ] Complete equivalent adapter coverage for all remaining entity families.
- [ ] Wire engine outputs back to Mineflayer entities without mutating unrelated
      state.
- [ ] Add serialization for deterministic replay inputs and outputs.

## Differential and conformance testing

Evidence: [native/src/simulation_tests.rs](native/src/simulation_tests.rs),
the Rust modules' unit tests, [test/mineflayer.test.cjs](test/mineflayer.test.cjs),
[scripts/native-smoke.cjs](scripts/native-smoke.cjs), and
[scripts/compare-tick-traces.cjs](scripts/compare-tick-traces.cjs).

- [x] Include Rust unit/regression cases for math, geometry, worlds, movement,
      versions, block effects, entities, pistons, vehicles, and projectiles.
- [x] Run all 446 Rust tests successfully on Linux with the audited source and
      checked-in generated data; no source regeneration was required.
- [x] Include 15 adapter tests covering world sync, legacy fence/column behavior,
      player movement/poses/entities, and ridden-mob animation/interpolation.
- [x] Run all 15 adapter tests successfully against a locally compiled Linux
      debug addon at the audited commit.
- [x] Provide native API smoke assertions and a JSON trace comparator accepting
      `frames` or `observed`, with exact comparison by default and optional epsilon.
- [ ] Get the full native smoke suite passing against the current locally built
      addon; Rust and adapter regressions pass, but the smoke script still stops
      at the source-fluid-height assertion recorded above.
- [ ] Distinguish Rust formula/breakpoint regressions from vanilla differential
      fixtures. No checked-in harness or golden trace corpus supplies the latter.

- [ ] Build a vanilla harness for each supported version.
- [ ] Run identical initial state, world, input, and tick sequences in vanilla
      and Rust.
- [ ] Capture every observable state field after every tick.
- [ ] Compare position, velocity, rotation, dimensions, pose, collision flags,
      fluid flags, fall state, effects, passengers, and vehicle state.
- [ ] Compare intermediate phases when final state matches accidentally.
- [ ] Add golden traces for every movement mode.
- [ ] Add tests for empty worlds, full blocks, partial blocks, stairs, slabs,
      fences, walls, fluids, waterlogged blocks, and world borders.
- [ ] Add tests for positive, negative, zero, subnormal, and very large
      coordinates.
- [ ] Add tests for diagonal input, yaw/pitch boundaries, and precision edges.
- [ ] Add tests for every effect, attribute, enchantment, pose, and ability.
- [ ] Add tests for every vehicle and projectile.
- [ ] Complete chunk load/unload/reload and incremental-update coverage; the
      existing adapter tests cover changed blocks and unloaded columns.
- [ ] Add long-run deterministic replay tests.
- [ ] Add randomized property tests against the vanilla harness.
- [ ] Add version-to-version differential tests to detect unintended sharing.
- [ ] Require zero unexplained mismatches before marking a version complete.
- [ ] Store mismatch traces, source references, and fixes in the repository.

## Performance and reliability

Evidence: [native/src/napi_player.rs](native/src/napi_player.rs),
[src/mineflayer/world.ts](src/mineflayer/world.ts), and
[scripts/generate-native-data.cjs](scripts/generate-native-data.cjs).

- [x] Implement persistent player state with typed-array exchange, batched
      native operations, event-driven world caching, and pooled generated arrays.
- [ ] Measure the effect of those optimizations. No benchmark result is implied
      by their presence or by passing functional tests.
- [ ] Add explicit world-sync listener disposal and test lifecycle cleanup;
      event subscriptions alone do not establish bounded resource ownership.

- [ ] Benchmark single-entity and multi-entity ticks per supported version.
- [ ] Benchmark world queries, collision queries, and N-API boundary overhead.
- [ ] Measure allocations and memory usage for long simulations.
- [ ] Ensure generated tables are compact and cache-friendly.
- [ ] Ensure deterministic results across supported platforms.
- [ ] Add overflow, malformed world, and invalid-state robustness tests.
- [ ] Add cancellation and resource cleanup for long-running bulk simulations.
- [ ] Verify thread-safety rules and document whether engines are `Send`/`Sync`.
- [ ] Add reproducible release builds with locked dependencies.

## Documentation and package integrity

Evidence: [package.json](package.json), [.npmignore](.npmignore),
[native/artifacts](native/artifacts), [README.md](README.md), and
[scripts/verify-native-package.cjs](scripts/verify-native-package.cjs).

- [x] Include the native loader/declarations, Windows artifacts, generated Rust
      data, TypeScript build configuration, and dependency lockfiles.
- [ ] Replace the package verifier's fixed count of 43 with manifest-derived
      coverage, including `26.3`; then re-run every assertion.
- [ ] Build/test Linux and macOS artifacts and verify platform-specific loading.
      Merely finding any `.node` file is insufficient for package verification.
- [ ] Fix the npm publication allowlist. The dry run includes all audit JSON
      (including Java method bodies in `physics-methods.json`), generated Rust
      source, scripts, and the singular `test/` directory; `.npmignore` excludes
      `tests/`, which does not exclude that adapter test file.
- [ ] Audit dependency declarations for clean consumers. All declared packages
      are currently `devDependencies`, while the public TypeScript declarations
      reference Mineflayer and `@nxg-org/mineflayer-physics-util` types.
- [ ] Reconcile the two tracked Windows binary names and verify the loader,
      generated types, manual binding types, and artifact come from one build.

- [ ] Document the supported versions and their completion status.
- [ ] Document the actual architecture: concrete Rust profiles, shared
      implementation paths, generated data, and generic TypeScript adapters.
- [ ] Document the world synchronization contract.
- [ ] Document all input and output fields.
- [ ] Document unsupported behavior explicitly.
- [ ] Exclude decompiled sources, temporary jars, and generator caches from npm.
- [ ] Include only generated runtime data required by the package.
- [ ] Verify the packed/published package contains the intended platform
      binaries, loader, types, runtime data, and documented adapter exports.
- [ ] Test installation and loading from a clean consumer project.
- [ ] Test Windows, Linux, and macOS packaging.

## Per-version completion gate

Apply this gate separately to each release before claiming full parity.
The unchecked rows are acceptance requirements, not a claim that every
individual subtask is absent for every release.

- [ ] Required vanilla source and mappings are available and fingerprinted.
- [ ] All required generated block, fluid, entity, attribute, and shape data is
      present and integrity-checked.
- [ ] The version has its own Rust engine module.
- [ ] The documented TypeScript construction path selects the exact release;
      resolve the original per-version class/factory requirement explicitly.
- [ ] No shared fallback is used for any version-specific behavior.
- [ ] Entity, player, fluid, collision, vehicle, projectile, effect, attribute,
      pose, and tick-order behavior is implemented.
- [ ] Vanilla differential traces pass with zero unexplained mismatches.
- [ ] Long-run deterministic replay passes.
- [ ] N-API and TypeScript integration tests pass.
- [ ] Package loading works from a clean installation.
- [ ] Performance and memory regressions are understood and accepted.
- [ ] Source provenance and known limitations are documented.

## Final release gate

- [ ] Every requested version has passed its per-version completion gate.
- [ ] No requested subsystem remains marked partial, approximate, or inherited
      without a verified identical source implementation.
- [ ] No generic version gate changes behavior for a completed version.
- [ ] All generated artifacts are reproducible from scripts and pinned inputs.
- [ ] Rust tests, TypeScript tests, differential tests, fuzz tests, benchmarks,
      lint, build, and package verification pass.
- [ ] A clean checkout can regenerate the complete engine and package.
- [ ] A clean consumer project can instantiate an engine directly and through
      `FROM_BOT`.
- [ ] The release notes list exact supported versions and verification evidence.
