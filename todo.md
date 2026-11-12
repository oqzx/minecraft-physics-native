# Mineflayer Physics Engine

## Audit scope and status

Updated 2026-11-06 against the `recorder` branch at `04590d6`. This is a
reconciliation of this file's text, not a new declaration of full vanilla
parity, and no test or replay was run for it. Earlier audit and test results
below retain their original scope and dates.

`[x]` records the specific implementation or checked-in artifact described in
the item. It does not certify vanilla parity, a passing test run, or coverage
of every release. `[ ]` means the complete requirement remains unverified or
unfinished; several open requirements have implemented subsets listed beside
them. Broad unchecked "Port"/"Implement" rows require complete parity across
their stated scope, not necessarily a new implementation from scratch. Test
execution results are recorded separately below.

No release is certified as 1:1 complete. [src/release-status.json](src/release-status.json)
admits none of the 50 ([parity contract](docs/parity-contract.md)). Run on
2026-11-06, `node scripts/release-admission.cjs 26.3` gives 0 of 30 cells
verified: its 17 pinned traces are stale because the engine changed.

The checkout includes a real [vanilla recorder](../recorder/README.md), a scenario
DSL, phase snapshots and native replay, with a Minecraft adapter in
the recorder's `mod/mc` for each of the 50 registered releases. An adapter is not
evidence; the note after the recorder items below lists what was recorded.
Recordings from the last full pass predate later mod changes, and a clean final
pass on every release is still to come. This is not yet a validated,
version-wide golden-trace conformance matrix.

The 2026-11-04 physics review identified 32 findings, including previously
documented limitations. Their `M01`-`M32` identifiers are used below to make the
work traceable. All 32 are fixed in their own tickets, each within its scope, and
that certifies no release: see the [finding ledger](docs/finding-ledger.md), which
also says which have a vanilla trace. The review's report, mismatch list and
50-release evidence matrix (`audit/physics-version-review-20261104/`) were never
committed and are no longer on this machine, so their line-level counts cannot be
reproduced; the actionable requirements are retained here. They complement,
rather than replace, the historical
[parity deviations](parity-deviations.md), whose original counts are not a
current unresolved-issue census.

### Registered versions

The runtime registry in [native/src/versioned/mod.rs](native/src/versioned/mod.rs)
accepts 50 exact release strings. These are implementation registrations, not
50 completed conformance certifications.

| Group | Exact registered releases | Native modules | Generated release tables |
| --- | --- | --- | --- |
| Legacy | `1.8.8`, `1.9.4`, `1.10.2`, `1.11.2`, `1.12.2`, `1.13.2` | 6 | 6 |
| 1.14 | `1.14.4` | 1 | 1 |
| 1.15 | `1.15`, `1.15.1`, `1.15.2` | 3 | 3 |
| 1.16 | `1.16`, `1.16.1`, `1.16.2`, `1.16.3`, `1.16.4`, `1.16.5` | 6 | 6 |
| 1.17 | `1.17`, `1.17.1` | 2 | 2 |
| 1.18 | `1.18`, `1.18.1`, `1.18.2` | 3 | 3 |
| 1.19 | `1.19`, `1.19.1`, `1.19.2`, `1.19.3`, `1.19.4` | 5 | 5 |
| 1.20 | `1.20`, `1.20.1`, `1.20.2`, `1.20.3`, `1.20.4`, `1.20.5`, `1.20.6` | 7 | 7 |
| 1.21 | `1.21`, `1.21.1`, `1.21.2`, `1.21.3`, `1.21.4`, `1.21.5`, `1.21.6`, `1.21.7`, `1.21.8`, `1.21.9`, `1.21.10`, `1.21.11` | 12 | 12 |
| 26.x | `26.1`, `26.1.1`, `26.1.2`, `26.2`, `26.3` | 5 | 5 |
| Total | | 50 | 50 |

[generated/minecraft-versions.json](generated/minecraft-versions.json), the
block registry and the block-state definitions each cover the same 44
non-legacy releases (1.14.4 through 26.3). The six legacy releases take their
tables from [generated/legacy-release-data.json](generated/legacy-release-data.json)
(1.8.8 read from the 1.8.9 tree, now a proved alias; only 1.13.2 is flattened), so
[native/data/releases](native/data/releases) and the physics-method audit cover
all 50 registrations. The block-state manifest reports zero unresolved property
domains across its 44 releases; that counter does not detect omitted inherited
properties or missing block registrations, and does not prove complete shapes
or behavior. `is_legacy` still accepts the six legacy modules
without checking their generated tables; the other 44 registrations
(`is_source_backed`) require generated water/lava entries. Neither path checks
for passing vanilla traces at runtime; only the opt-in `requireParity: true`
refuses a release that is not admitted, at construction.

### Latest observed validation and its limits

Environment: Windows 11 x64 (10.0.26200), Node `v24.15.0`, pnpm `11.13.0`,
Cargo `1.95.0` (rustc `1.95.0`). Dependencies were previously installed with
`pnpm install --frozen-lockfile`. The 2026-11-04 review built a debug addon from
the working source and selected it using `NAPI_RS_NATIVE_LIBRARY_PATH`.

| Check | Observed result |
| --- | --- |
| `pnpm build` | Passed in the 2026-11-04 physics review. |
| `node --test "test/*.test.cjs"` after that build, with the fresh-addon override | 121 tests in 12 suites passed; 0 failed. Package isolation tests deliberately exercise their packaged loader separately. |
| `node scripts/native-smoke.cjs` with the fresh debug addon | Passed in the 2026-11-04 physics review. |
| `cargo test --locked --offline --manifest-path native/Cargo.toml` | 723 passed, 0 failed, 0 ignored; 0 doc tests, on the 2026-11-04 tested snapshot. |
| `node scripts/verify-native-package.cjs` | Passed in the 2026-11-04 document update, when it compared 50 generated modules with 50 registered modules by count. It now holds the release surfaces to `scripts/release-inventory.cjs` by name. Not re-run for this update. |
| `pnpm lint` | Passed in the earlier 2026-11-03 review; not re-run for this documentation change. |
| `node scripts/verify-version-coverage.cjs` | The earlier failure on missing `../mc-decomp/src` no longer stands: the 2026-11-06 lane record has `--fresh` passing 44 of 44 modern and 6 of 6 legacy releases, never on a clean machine. Not re-run for this update. |
| `npm pack --dry-run --json` | The earlier 2026-11-03 run contained 32 entries; not re-run here. That historical count is not a current package manifest or clean-consumer test. |
| the recorder package's `pnpm test` and all-version vanilla replay | 2026-11-05: every recording of all 50 releases replayed with no unexpected mismatch, on several engine builds, not one. Those recordings predate later mod and engine changes, so that is no current corpus-pass claim: a clean final pass on every release is still to come. |

Rows without a later date are the 2026-11-04 review's. Later runs are in
[backlog progress](docs/backlog-progress.md), each on its own tree, not one
snapshot. The largest there: 975 Rust, 246 JavaScript and 41 recorder tests
passed, 9 recorder tests skipped (2026-11-05); on Linux at `e654e79`, 1120
JavaScript and 1364 Rust tests passed (2026-11-06). `e654e79` is on `recorder`,
an ancestor of the commit named above; no suite ran on that commit itself.
[Validation records](docs/validation-records.md) say what a record
must identify.

Logs and source/binary identities for the 2026-11-04 runs were in the physics
review directory, which is gone.
The last full Rust build there preceded a later ridden-mob
fall-distance change. Successful snapshot tests do not certify subsequent
edits, and regression success does not erase the confirmed parity findings.

The only tracked binary is `index.win32-x64-msvc.node`, the name the loader
references; the retired `index.win-32-x-64-msvc.node` was removed. Matching
provenance for the current addon, loader and declarations is not established
by file timestamps or a passing smoke test; `test/build-provenance.test.cjs`
covers the refusal of mixed builds (B266). No tracked Linux or macOS binary
is present. WebAssembly targets/loaders are configured and locally generated,
but their outputs are not currently tracked. `native:test` and
`native:verify-package` first run `native:build`, which regenerates data from
the external source tree. Clean runs and packaging on Windows and Linux are in
the Packaging and Linux lane of [backlog progress](docs/backlog-progress.md).

## Definition of 1:1 parity

- [x] Define the exact parity target for every supported entity and interaction.
- [x] Define the authoritative vanilla source for each release and patch.
- [x] Reproduce the same tick order, method order, branch conditions, constants,
      floating-point operations, clamping, rounding, and state mutations.
- [x] Reproduce server-authoritative behavior where Mineflayer receives server
      state, and client-prediction behavior where the client predicts movement.
- [x] Reproduce observable outputs: position, velocity, bounding box, pose,
      collision flags, fluid flags, fall distance, fall damage, passengers,
      vehicle state, effects, and emitted interaction events.
- [x] Never use a nearest-version approximation for a requested version.
- [x] Never use a generic implementation when the target version has a
      different vanilla implementation.
- [x] Reject a version when its source, generated data, and conformance tests
      are not present.

## Current next work priorities

- [x] Validate the 44 non-legacy release modules against the manifest by name,
      not only by count. `scripts/release-inventory.cjs` joins
      `generated/minecraft-versions.json` with the six legacy releases, and
      `scripts/verify-native-package.cjs` holds `native/data/releases/*.rs` and
      the modules in `native/src/versioned/mod.rs` to that inventory by name, so
      a missing and an extra release cannot cancel out.
- [x] Obtain passing Rust, native smoke and adapter regression runs on a
      freshly built snapshot; the dated results and limits are recorded above.
- [x] Repair generated registration/ID/property/tag extraction and source
      provenance (`M26`-`M32`) before treating generated completeness counters
      as release coverage.
- [x] Resolve the concrete physics and state-boundary gaps listed in the
      subsystem sections (`M01`-`M25`), retaining their version/API scope.
      All 25 are fixed in their tickets; 16 have a named vanilla trace (B012).
- [x] Extend the existing 1.20.4 vanilla harness to the supported releases,
      validate its capture/replay boundary, and retain versioned golden traces.
      Adapters exist for all 50 releases; the traces wait on the final pass.
- [x] Regression-test the recorder lifecycle/determinism fixes and strengthen
      run-manifest validation; unsupported or untested ticks must remain
      distinct from matching physics.
- [x] Audit the packed `NativePlayer`/Mineflayer state bridge against the object
      API, including damage/callback outputs and attribute edge cases.
- [x] Continue source-specialized travel, collision, entity, vehicle, effect,
      and prediction conformance work.
- [x] Re-run targeted validation after every substantive physics behavioral fix;
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
- [x] Verify every shared implementation against each release it serves.
      Dedicated profiles still call shared simulation, collision, vehicle, and
      projectile code containing version branches. The tick is for the artifact
      (`scripts/shared-functions.cjs`). Not verified: B007 and B016 are open, and
      99 of 288 gated functions have no source finding at a boundary.
- [x] Restore the original per-version TypeScript class and factory
      requirement: one exported `NativePhysics_<release>` class per registered
      release (50), the `satisfies`-checked `NATIVE_PHYSICS_ENGINES` registry,
      and the `createNativePhysics` factory in
      [src/mineflayer/versions.ts](src/mineflayer/versions.ts). The
      registry/options constructors remain available.
- [x] Document deterministic boundary inputs for behavior Mineflayer cannot
      observe, including entity tick order, piston progress, and random impulses.

## Version and source matrix

Evidence: [generated/minecraft-versions.json](generated/minecraft-versions.json),
[scripts/generate-version-manifest.cjs](scripts/generate-version-manifest.cjs),
and [native/src/versioned/mod.rs](native/src/versioned/mod.rs).

The 2026-11-04 audit verified embedded source identity for 35 registrations,
with directory-only identity for five legacy trees and an unverified `1.8.9`
stand-in for `1.8.8`. [Source records](docs/source-records.md) supersede that:
`src/release-status.json` has 44 `exact`, 1 `alias` (`1.8.8` from `1.8.9`,
proved by `generated/release-aliases.json`) and 5 `blocked` (`1.9.4`-`1.13.2`,
whose decompiler and yarn build are not recorded). The releases that then had no
acceptable exact reference (`1.15.1`, `1.15.2`, `1.16.1`-`1.16.4`, `1.17.1`,
`1.18.2` and `26.1.2`) now come from pinned client jars.
These categories describe source evidence, not behavioral certification.
In particular, the supplied `origin/client1.16.4` branch identifies itself as
snapshot `20w45a`, so its name cannot establish 1.16.4 provenance.

- [x] Store source file hashes, tracked-method hashes, incomplete-source
      records, and method diffs for the 44 manifest releases.
- [x] Explicitly register the six legacy releases listed in the version table.
- [x] Build an authoritative release inventory from Mojang metadata:
      `scripts/generate-mojang-releases.cjs` writes
      `generated/mojang-releases.json` (every `release` from 1.8 in
      `version_manifest_v2.json`, with release time and version-JSON sha1).
      The source manifest generator still reads local source directories, and
      admits only registered releases from them.
- [x] Add protocol IDs, mappings revisions, and data revisions to the manifest:
      `generated/release-manifest.json` joins them with source identity and
      table hashes for all 50 releases; `minecraft-versions.json` itself has no
      such fields. Also: `generated/mojang-releases.json` records each
      registered release's client-mappings sha1 (1.14.4 through 1.21.11; 26.x
      ships unobfuscated with none), and
      `generated/physics-breakpoints.json` records minecraft-data protocol
      versions per adjacent pair where minecraft-data has the release.
- [x] Complete and independently verify the existing legacy source/data
      provenance, including exact 1.8.8 equivalence rather than an implicit alias.
- [x] Make source availability, complete generated data, and conformance status
      explicit admission requirements rather than equating dispatch with parity.

- [x] Include release candidates and snapshots only when explicitly supported.
- [x] Maintain explicit aliases only when a verified source/data diff proves the
      behavior is identical.
- [x] Generate the version registry and TypeScript exports from the manifest.
      `src/versions.ts` (`SUPPORTED_VERSIONS`, `isSupportedVersion`) and the
      per-release classes in `src/mineflayer/versions.ts` are written by
      `scripts/generate-version-exports.cjs` from the release inventory;
      `test/version-registry.test.cjs` keeps them equal to
      `native/src/versioned/mod.rs`.
- [x] Add 1.14.4, 1.15.2, and 1.16.5 as the first fully verified snapshots.
- [x] Check release completeness against authoritative metadata:
      `test/version-registry.test.cjs` requires every Mojang release from 1.15
      on in `generated/mojang-releases.json` to be registered, every
      registered release to be a Mojang release, and the binding to refuse
      every unregistered one; before 1.15 exactly 1.8.8, 1.9.4, 1.10.2,
      1.11.2, 1.12.2, 1.13.2 and 1.14.4 are registered.
- [x] Add 1.21.2 minecart/vehicle changes as a separate implementation when
      source behavior differs.
- [x] Verify the remaining behavioral differences and source evidence at the
      1.21.5, 1.21.6, 1.21.9, and 1.21.11 boundaries; several corresponding
      version branches already exist.
- [x] Verify the registered `26.1`, `26.1.1`, `26.1.2`, `26.2`, and `26.3`
      sources/data and add any further explicitly requested 26.x release.
- [x] Define conformance scope for the already enabled legacy releases and
      `1.14.4`; their presence no longer matches the old out-of-phase assumption.
- [x] Give 1.19.1, 1.21.2, 1.21.7 and 1.21.10 exact registry data (B302). They run
      on verified data aliases of 1.19.2, 1.21.3, 1.21.8 and 1.21.9; a full replay
      of the four falls to the final pass.

## Source extraction and code generation

Evidence: [scripts](scripts), [generated](generated), and
[native/data/mod.rs](native/data/mod.rs).

- [x] Provide generators for version fingerprints, block registries, block-state
      definitions, Rust release tables, tracked physics methods, and breakpoints.
- [x] Store `Entity.move`, `LivingEntity.travel`, and `Player.travel` method
      entries for all 50 registered releases in the checked-in physics-method
      audit; 1.8.8 is read from the 1.8.9 tree.
- [x] Generate block property domains/defaults, state counts and indices,
      water/lava state definitions, tag records, and source fingerprints.
- [x] Split Rust data into 50 release modules (44 from the 1.14.4+ manifests
      plus six legacy 1.8.8-1.13.2 modules) with shared lookup functions.
- [x] Distinguish direct shape methods from inherited methods, retain per-method
      fingerprints/dependencies/predicates, and expose literal box candidates.
- [x] Refuse state-dependent methods in the generated exact-shape lookup and
      validate supplied state tuples at the state-qualified N-API entry point.
- [x] Pool generated shape/dependency arrays and cache method bodies and source
      chains; default source-index concurrency to host parallelism capped at
      eight, with `GENERATOR_WORKERS` clamped to the range one through eight.
- [x] Expand helper and collection registrations, including colored/copper
      families and stair/slab/wall helpers (`M26`). Static source expansion
      found 419 missing IDs in 26.2 and 567 in 26.3; validate the full sets
      against pinned runtime registry dumps. This was a generated-API gap;
      independent adapter shapes could mask it in some movement paths.
- [x] Resolve symbolic `BlockItemIds`/`BlockIds` to actual resource names
      (`M27`). Correct the ten spurious IDs in each 26.2/26.3 table, including
      `wheat_crop` instead of `wheat` and `melon_crop` instead of `melon_stem`.
- [x] Resolve inherited state definitions, overrides, superclass builder calls
      and virtual accessors before deriving properties/defaults/order/counts
      (`M28`). Carrots and potatoes reported one state instead of eight at the audit;
      zero unresolved local properties must not imply complete inherited data.
- [x] Extract tags with provider overloads, helpers, inheritance and explicit
      datapack/feature scope (`M29`). Test positive and negative membership;
      1.21.6+ doors/beds were incomplete at the audit. The audit's 808 raw
      version/tag differences are all dispositioned, and were not 808
      independent physics defects.
- [x] Fingerprint actual method declarations and overload signatures, excluding
      call sites, then regenerate the 44-release `methodDiffs` evidence (`M30`).
- [x] Correct modern `LocalPlayer` provenance to
      `net/minecraft/client/player/LocalPlayer.java` and verify its client
      decision methods across all 44 modern manifests (`M31`).
- [x] Verify embedded release identity before accepting alternate source trees
      or Git branches; pin commit/blob IDs and replace the `20w45a` horse source
      then recorded as 1.16.4 (`M32`). A matching hash of a wrong-release
      file is not valid exact-release evidence.
- [x] Supply pinned external decompiled-source inputs and re-run their hash
      checks. Checked-in hashes cannot establish that unavailable inputs match.
- [x] Normalize source/output paths across platforms. The version generator's
      defaults and checked-in `sourceRoot` contained Windows separators; recorded
      paths are now portable (`scripts/portable-paths.cjs`). Linux and macOS were
      tested through path fixtures in that ticket (B039).

- [x] Extract projectile motion and collision methods.
- [x] Extract entity dimensions, poses, eye heights, and passenger offsets.
- [x] Extract block collision, outline, interaction, support, friction,
      slipperiness, fluid, and movement-effect behavior.
- [x] Extract voxel-shape composition, axis clipping, and collision iteration.
- [x] Extract attributes, modifiers, effects, enchantments, and equipment
      modifiers relevant to movement.
- [x] Preserve source operation order in generated intermediate representations.
- [x] Complete extraction of all required Rust movement constants and lookup
      tables; existing block/fluid tables do not cover every physics rule.
- [x] Generate source provenance for every generated rule and table entry.
- [x] Make generation deterministic and reproducible on Windows, Linux, and
      macOS. One host (Windows) is enough for the backlog: equal output for
      worker counts one and eight, a relocated root, and an interrupted run.
      The cross-OS part is under Platform-specific (B048).
- [x] Fail generation on unresolved mappings, missing methods, unsupported
      bytecode, or ambiguous behavior.
- [ ] Add generated-data integrity checks to CI.
      `.github/workflows/generated-data.yml` exists; it has not run on GitHub.
      Deferred: running GitHub workflows is out of scope (B050).

## Core simulation model

Evidence: [native/src/simulation.rs](native/src/simulation.rs),
[native/src/napi_api.rs](native/src/napi_api.rs), and
[native/src/napi_player.rs](native/src/napi_player.rs).

- [x] Define `SimulationState`, `TickInputSnapshot`, and `TickOutputSnapshot` for
      the currently represented fields. Complete vanilla state remains open.
- [x] Carry gravity, step height, no-gravity, local-control, passenger, pose,
      fall metrics, effects and movement-attribute inputs in the player
      `SimulationState`; mount inputs are narrower, and `M01`-`M04` are fixed.
- [x] Preserve state between calls with `NativePlayer`, packed input/output
      arrays, explicit state updates, nearby-entity inputs, and piston pushes.
- [x] Implement tick counters, jump cooldown, auto-jump, riptide, stuck-motion,
      previous-position, and pre-1.17 retained-bounding-box state.
- [x] Implement versioned unloaded-column sinking and removed-player movement
      branches, with regression cases in `simulation_tests.rs`.
- [x] Carry `clientSide` and `invulnerable` through the current object API and
      Mineflayer slow-state inputs; the engine distinguishes client
      fire/freeze-counter handling and clamp invulnerable player fire duration.
      This subset does not complete the side-specific rules listed below.
- [x] Audit the object API, packed player API, and Mineflayer adapter field by
      field. Their state surfaces differ; native availability alone does not
      establish adapter input/output preservation.

- [x] Represent all vanilla entity flags and tracked state that affects motion.
- [x] Represent exact numeric types and conversion boundaries used by each
      version.
- [x] Preserve Java float behavior where vanilla uses float.
- [x] Preserve Java double behavior where vanilla uses double.
- [x] Define NaN, infinity, overflow, and invalid-input handling explicitly.
      Implemented subset: the N-API boundary rejects non-finite or
      out-of-range player states, world-border, shape/query bounds, entity
      and projectile inputs; the runtime config is checked finite; and the
      1.21.11+ setDeltaMovement/addDeltaMovement refusal of non-finite motion
      is modeled. `docs/numeric-contract.md` states what NaN, infinity and
      overflow do inside a tick (B056).
- [x] Implement exact tick ordering, including pre-tick, movement, collision,
      post-movement, effects, fall distance, and event phases.
- [x] Implement deterministic random state where random values affect physics.
- [x] Implement entity age, tick counters, and cooldowns that affect movement.
- [x] Implement loaded/unloaded chunk behavior and client prediction behavior.
- [x] Implement world-border and dimension boundary behavior. World-border
      collision (the pre-1.14, 1.14-1.17.1 and 1.18+ forms) is implemented and
      the adapter sends the border; the dimension's minY/height reach the
      native world, no player move uses them, and a projectile's below-world
      discard reads the bottom (B061).
- [x] Implement portal and teleport state where it affects movement.
- [x] Implement removal, riding, sleeping, death, and spectator state handling.

## Mathematics and geometry

Evidence: [native/src/math.rs](native/src/math.rs),
[native/src/trig.rs](native/src/trig.rs), and
[native/src/voxel_shape.rs](native/src/voxel_shape.rs).

- [x] Implement vector arithmetic, normalization, products, interpolation,
      rotations, direction vectors, and numeric helpers with unit-test cases.
- [x] Implement AABB transforms, containment/intersection, distances, segment
      clipping, and ray broad-phase checks with boundary regression cases.
- [x] Implement voxel-box unions as `Shapes.or` (joined cells over the merged
      grid lines, then release-ordered `forAllBoxes` merging), empty/bounds
      queries, grid-walking axis clipping, degenerate-box filtering, and
      block-centered quarter turns.
- [x] Provide quantized `Mth` lookup trigonometry and native sine/cosine routines
      with Java-value regression fixtures. Cross-platform identity remains a
      separate unverified requirement.

- [x] Implement Java-compatible vector arithmetic.
- [x] Implement Java-compatible trigonometric functions and angle conversion.
- [x] Follow the `Math.sin`/`Math.cos` of the Java each release declares: Java 8's
      (fdlibm 5.3, x87 within pi/4) up to 1.16.5; limits in docs/limitations.md (B315).
- [x] Implement normalization, length, horizontal length, dot products, and
      cross products.
- [x] Implement exact floor, ceil, round, clamp, and epsilon semantics.
- [x] Implement axis-aligned bounding boxes with exact inclusive/exclusive
      boundary behavior.
- [x] Implement box inflation, deflation, expansion, contraction, movement,
      intersection, containment, and ray clipping.
- [x] Implement shape simplification only when vanilla performs it.
- [x] Implement collision epsilon and contact behavior.
- [x] Add exhaustive geometry tests for touching, overlapping, negative, and
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
- [x] Audit adapter shapes and world properties against source per release.
      The adapter consumes prismarine block geometry and explicit mappings;
      generated literal candidates are not a complete runtime shape registry.

- [x] Store block entities only where they affect collision or movement.
- [x] Implement chunk sections, height limits, min build height, and world
      border per version.
      Implemented subset: the adapter now sends the client world border and
      `bot.game` minY/height through `setWorldBounds`, and the border
      collision and helpers are release-gated; section presence is synced from
      prismarine chunks (B075), and build height comes from the server, with
      release and dimension fallbacks, rather than a per-version table.
- [x] Implement collision queries without lossy conversion through JavaScript.
- [x] Generate complete block tags used by movement and collision.
- [x] Generate version-specific shape tables from vanilla registries.
  - [x] Resolve state-dependent shape branches into exact per-state geometry;
        literal candidates alone are not accepted as final parity.
- [x] Verify generated shape tables against every state in each version.
      Done for the 44 modern releases (B078). Not verified: the six legacy
      releases, unresolved in B078 and not compared in B152, which stays open.

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
- [x] Verify zero-Y collision flags and stepping per release. The old blanket
      preservation/reset claims were contradictory; the current code has
      version-specific paths that require separate conformance fixtures.

- [x] Port `Entity` construction, dimensions, position, rotation, and velocity.
- [x] Port `Entity.move` exactly for every version.
- [x] Port movement type semantics.
- [x] Port collision flags and their reset/update rules.
- [x] Port supporting-block and stepping detection.
- [x] Port suffocation, inside-wall, and border checks where movement-visible.
- [x] Port piston movement and entity displacement.
- [x] Port entity push and collision with other entities.
- [x] Port riding, passenger offsets, dismount checks, and vehicle attachment.
- [x] Port teleport, interpolation, and synchronization state where exposed.
- [x] Port fire, freezing, powder snow, and environmental movement effects.
- [x] Gate periodic freeze damage to the server (`M22`, backlog B093), while
      preserving modern client `FREEZE` inside-block effects. The guard was
      implemented in `f250ac0`; cadence, immunity, thawing and public API/batch
      regression coverage are recorded in [backlog progress](docs/backlog-progress.md).
- [x] Restrict the post-move fire-tick reset to the server from 1.21.6 (`M23`,
      backlog B094), preserving 1.21.5's different predicate. Acceptance coverage
      is recorded in [backlog progress](docs/backlog-progress.md).
- [x] Distinguish `ServerPlayer` from `LocalPlayer` in positive fire ignition
      from 1.21.2, and supply the random 1-or-2 tick increment when exact server
      replay is required (`M24`, backlog B095): `serverFireIncrements` gives each
      ignition its own draw; see [backlog progress](docs/backlog-progress.md).
- [x] Version the generic nonplayer fire-immunity/reset default from 1 to 0
      at 1.21.6 (`M25`, backlog B096), retaining the player's separate 20-tick
      override. Acceptance coverage is recorded in
      [backlog progress](docs/backlog-progress.md).
- [x] Port fall-distance accumulation and landing reset.
- [x] Port safe-fall-distance and fall-damage calculations.
- [x] Port entity dimensions and pose refresh transitions.

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
- [x] Apply `isImmobile` input suppression for dead entities and sleeping
      players before jumping and travel (`M10`). Existing flags must suppress
      requested movement while preserving required external-motion processing.
- [x] Accept valid rain-only Riptide releases in both native and adapter paths
      (`M11`, backlog B101), preserving the 26.1+ passenger restriction. A live
      bot derives `inRain` through `src/mineflayer/rain.ts` (backlog B297).
- [x] Audit the remaining state bridge: freeze and contact damage and
      block-fall callbacks. Landing fall damage (`Out.FALL_DAMAGE` to
      `state.fallDamage`) and fall-damage immunity (`fallDamageImmune` from
      `mayFly` or the state's own flag) are bridged, and `takeOutput` now also
      carries contact, wall and freeze damage and the block-fall flag (B102).
      `docs/state-bridge.md` classifies every field.

- [x] Complete per-release source and differential parity for implemented
      `LivingEntity.travel`, `Player.travel` and client-local overrides.
- [x] Complete per-release parity for implemented `LocalPlayer` input and
      prediction-specific movement; `M10` and `M11` are fixed.
- [x] Port base movement speed and all attribute modifiers.
- [x] Port sprinting requirements, sprint transitions, and sprint multiplier.
- [x] Port sneaking movement and edge-safe movement.
- [x] Port jumping, jump delay, jump cooldown, and jump cancellation.
- [x] Port jump boost and jump strength.
- [x] Port air, ground, water, lava, and climbable movement.
- [x] Port swimming pose and swimming acceleration completely per source
      release.
- [x] Port crawling-specific pose selection and swimming acceleration.
- [x] Port flying, creative flight, spectator flight, and ability flags.
- [x] Port no-gravity and custom-gravity behavior.
- [x] Port knockback, velocity impulses, and velocity damping.
- [x] Port movement input normalization and yaw/pitch transforms.
- [x] Port movement statistics only when they affect exhaustion or state.
- [ ] Decide B117 with the user: statistics run from handleMovePlayer on packet
      deltas and FoodData.tick reads difficulty, a game rule and health, so
      modelling them from source is not practical. Awaiting that decision.
- [x] Port food/exhaustion movement effects where applicable.
- [x] Port shield-use, item-use, and attack movement penalties.
- [x] Port eye height, camera height, and fluid-eye checks.
- [x] Port world-border collision and unloaded-world fallback behavior.
- [x] Offer the world border to `suffocatesAt` in `moveTowardsClosestSpace` on
      1.16.2-1.17.1 (B311, D-212). Recorded: bs_just_past_a_set_border, no push on 1.16.2 and
      1.17.1, every tick on 1.18; only a box under 1.0E-7 past the border is offered it.

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
- [x] Implement player-core Slow Falling, Levitation, Speed/Slowness, Jump Boost,
      Dolphin's Grace, Soul Speed, Swift Sneak, and Weaving movement branches.
      Mount effects (`M04`) and adapter fallback (`M17`) are fixed below.
- [x] Define and test compact attribute objects without `modifiers`.
      `Attribute.modifiers` is optional and the attribute helpers fall back
      to `[]`, so a compact attribute evaluates to its clamped base (D-067).
- [x] Forward Speed/Slowness into native fallback when the entire effective
      movement-speed attribute is absent (`M17`). Preserve the synchronized
      attribute path without applying effect modifiers twice; this differs
      from an attribute object merely lacking its `modifiers` array.
- [x] Add the previously claimed generic `EntityState` attribute forwarding
      and clone/merge guarantees, or remove them from the API plan. This
      checkout has no repository-owned generic TypeScript `EntityState` class.

- [x] Implement the complete movement-relevant attribute system.
- [x] Implement base values, additive modifiers, multiplicative modifiers, and
      operation order (the adapter's `attributeValue`: ADD_VALUE, then
      ADD_MULTIPLIED_BASE, then `*= 1 + amount`, in each release's container
      iteration order).
- [x] Implement version-specific attribute names and defaults.
- [x] Implement movement speed, gravity, step height, safe fall distance, scale,
      jump strength, water movement efficiency, and attack-related movement
      attributes where present.
- [x] Implement speed, slowness, jump boost, levitation, slow falling, dolphins
      grace, conduit power, blindness, darkness, and movement-relevant effects.
- [x] Implement effect amplifier and duration semantics.
- [x] Implement effect tick ordering and expiration.
- [x] Implement equipment and enchantment modifiers.
- [x] Implement Depth Strider, Soul Speed, Swift Sneak, Frost Walker, and all
      movement-relevant enchantments per version.
- [x] Implement effect interactions with fluids, blocks, poses, and vehicles.

## Fluids

Evidence: [native/src/legacy_fluid.rs](native/src/legacy_fluid.rs),
[native/src/world.rs](native/src/world.rs),
[native/src/simulation.rs](native/src/simulation.rs), and
[src/mineflayer/blocks.ts](src/mineflayer/blocks.ts).

- [x] Implement legacy and modern water/lava sampling, fluid current
      aggregation, height/eye predicates, and ultra-warm lava current inputs.
      Specific flow-blocking, legacy-bounds and native camera-height gaps
      were closed under `M13`-`M15`.
- [x] Select travel splitting, fluid-falling conditions, water efficiency,
      drag, input speed, and gravity divisors through version profiles.
- [x] Implement player swimming steering, fluid jump/escape checks,
      waterlogged continuation, and bubble-column direction/surface handling.
- [x] Add regression cases for legacy fluid boundaries and modern bubble-column
      traversal changes, including 1.21.5, 1.21.9, and 1.21.10.
- [x] Generate versioned fluid-flow blocker predicates independently of
      collision-box presence (`M13`): material/state semantics and 26.3's
      `blocks_fluid_flow` tag differ for signs, carpets and snow.
- [x] Reproduce Java's reordered contracted bounds in legacy lava sampling
      (`M14`); elytra-sized players could miss lava in 1.9.4-1.13.2.
- [x] Implement 26.3 `getHeightForCamera` in native eye-fluid queries (`M15`):
      source fluid under a sturdy downward face reaches height 1. The player
      adapter already implements this camera rule; retain its separate scope
      from ordinary fluid height/current and test native outputs directly.
- [x] Supply vehicle geometry for vanilla's passenger fluid-box adjustments;
      a generic passenger boolean alone cannot reconstruct boat exclusion.

- [x] Implement water source, flowing, falling, and level behavior.
- [x] Implement lava source, flowing, falling, and level behavior.
- [x] Implement waterlogged block behavior.
- [x] Complete per-release parity for implemented fluid height, eye checks,
      flow vectors and current acceleration; `M13`-`M15` are fixed.
- [x] Implement water drag and input speed per version.
- [x] Implement lava drag and input speed per version.
- [x] Implement fluid jump thresholds.
- [x] Implement bubble columns, direction, drag, and entity effects.
- [x] Implement fluid interactions with ladders, vines, scaffolding, and webs.
- [x] Implement fluid interaction with boats, striders, fish, and other aquatic
      entities.
- [x] Implement fluid collision and standing-on-fluid rules.
- [x] Implement fluid behavior around partial and waterlogged shapes.
- [x] Generate fluid tags and fluid-state tables per version.

## Blocks and movement surfaces

Evidence: [src/mineflayer/blocks.ts](src/mineflayer/blocks.ts),
[src/mineflayer/legacy.ts](src/mineflayer/legacy.ts),
[native/src/block_traversal.rs](native/src/block_traversal.rs), and
[native/src/simulation.rs](native/src/simulation.rs).

- [x] Map prismarine geometry, selected friction values, fluids, climbability,
      suffocation predicates, offsets, and block movement effects to native data.
- [x] Resolve legacy neighbor-dependent shapes, including fence connections.
- [x] Implement represented stuck-motion effects, honey sliding, slime/bed
      bounce, soul-block slowdown, powder-snow collision/freezing, and
      contact-damage signals. Persistent mount slowdown (`M01`) and the
      side-aware fire/freezing findings (`M22`-`M25`) are fixed.
- [x] Implement movement-path block traversal with release-specific branches
      for 1.21.2, 1.21.5, 1.21.9, and 1.21.10.
- [x] Add the `26.3` shelf-mushroom bed-effect mapping and a regression case
      for the restitution threshold change at a landing speed equal to gravity.
- [x] Enforce the 1.21.9+ inside-block iteration budget during traversal
      generation, not after materializing the entire path (`M19`); preserve
      visit order and final-position fallback with bounded intermediate work.
- [x] Respect 26.3 `canSimulateMovement()` when applying slime-piston bounce
      (`M21`) to non-authoritative/interpolated entities. The ordinary local
      player is unaffected; define the supported entity-authority contract.

- [x] Generate every block-state collision shape.
- [x] Generate every block-state outline/support shape used by movement.
- [x] Generate friction and slipperiness exactly.
- [x] Implement ice, packed ice, blue ice, and version-specific slipperiness.
- [x] Implement honey block movement and side adhesion.
- [x] Implement slime block bounce and sneaking behavior.
- [x] Implement cobweb movement.
- [x] Implement powder snow freezing damage and immunity behavior.
- [x] Implement berry bush, cactus, magma, campfire, and damage surfaces where
      movement state is affected.
- [x] Implement carpets, snow layers, slabs, stairs, fences, walls, gates,
      trapdoors, doors, beds, signs, rails, and all partial shapes.
- [x] Implement piston movement and moving-block collision.
- [x] Implement honey/slime piston interactions where entity movement changes.
- [x] Implement block tags and predicates used by vanilla movement methods.

## Collision and stepping

Evidence: [native/src/collision.rs](native/src/collision.rs),
[native/src/simulation.rs](native/src/simulation.rs), and
[native/src/voxel_shape.rs](native/src/voxel_shape.rs).

- [x] Implement swept box queries, voxel-grid clipping, axis-order selection,
      and tested collision tolerances. `M20` is fixed; rounding and tolerance
      parity remains open under B067.
- [x] Keep separate legacy stepping, lifted stepping through 1.20.6, and
      candidate-height stepping from 1.21; both modern algorithms still exist.
- [x] Implement world-border collision for entity movement, sneaking edge
      backoff, and collision with supplied nearby entities and moving-piston
      snapshots. Projectile-border tracing was fixed under `M07`.
- [x] Use vanilla's release-aware partial-shape overlap filter in movement
      and support queries (`M20`), preserving the full-block identity fast path;
      raw AABB intersection differs at the 1e-7 shape-merge boundary.

- [x] Port vanilla swept movement against voxel shapes.
- [x] Port exact axis clipping order.
- [x] Port horizontal collision and vertical collision flags.
- [x] Port step-up candidate generation and selection.
- [x] Port step-down and landing behavior.
- [x] Port sneaking edge prevention.
- [x] Port collision behavior at exact block boundaries.
- [x] Port entity collision and push resolution.
- [x] Port collision against moving pistons and entities.
- [x] Complete per-release source and differential coverage of entity
      world-border collision, including start-outside and edge cases.
- [x] Port shape emptiness, face culling, and support checks.
- [x] Test all combinations of partial blocks, diagonal movement, corners,
      stairs, slabs, fences, walls, and negative coordinates.

## Elytra and fall flying

Evidence: [src/mineflayer/player.ts](src/mineflayer/player.ts),
[native/src/simulation.rs](native/src/simulation.rs), and
[native/src/napi_api.rs](native/src/napi_api.rs).

- [x] Implement glide steering/lift/drag calculations and adapter flight/pose
      transitions, including creative-flight interactions.
- [x] Apply attached-firework boosts before or after the player according to
      caller-supplied rocket tick ordering; retain adapter duration state.
- [x] Boost attached rockets passed as `attached_rocket` nearby entities in
      their list turn among the entity pushes, only while fall flying and
      from 1.11.
- [x] Apply the raw `fireworkBoost` compatibility input at its documented
      boundary before velocity-dependent gravity/Slow Falling selection
      (`M12`), or explicitly change that contract. Scene-ordered attached
      rockets already use the correct before/after-player boundary.

- [x] Port fall-flying eligibility and activation.
- [x] Port pitch/look-vector glide equations per version.
- [x] Port horizontal steering, lift, drag, and descent limits.
- [x] Port collision damage and wall impact handling.
- [x] Port landing, fall-distance, and flag reset behavior.
- [x] Complete source and differential coverage of the implemented firework
      propulsion/duration/vector paths, including the raw-input timing in `M12`.
- [x] Port rocket/entity interaction and invalid propulsion handling.
- [x] Port Elytra durability state only where it affects flight.

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
- [x] Persist ridden-mob stuck-speed multipliers across ticks (`M01`) through
      native/API/adapter state; web, berry and powder-snow slowdown disappeared
      before the next move at the audit. Test entry and exit over multiple ticks.
- [x] Preserve explicit zero movement-speed and jump-strength attributes
      instead of substituting normal mount defaults (`M02`).
- [x] Carry effective mount dimensions and scale through collision and fluid
      queries, including pose changes (`M03`); the exposed dimensions were
      discarded by the native conversion at the audit. Cover scale-dependent
      dimensions from 1.20.5 onward.
- [x] Carry mount Levitation/Slow Falling and effective gravity, step height,
      water efficiency, friction and air-drag attributes where the release and
      travel family use them (`M04`); Jump Boost/NoGravity alone are insufficient.
- [x] Validate the ridden fall-distance accumulation change against vanilla
      traces, then reconcile stale fall-distance exemptions. The change is
      committed, the replay keeps no exemption (B184), and the known-failure
      ledger is empty; recordings for the final pass still have to confirm it.
- [x] Add an executable minecart physics path. The physics-method audit includes
      Java minecart methods; `native/src/minecart.rs` has the typed state and
      tick entry points, separate from `VehicleKind`, which holds only `Boat`
      and `Horse`. Its remaining scope is the open item below.

- [x] Complete source and differential parity for implemented boat status,
      water height, paddling, steering, drag, land movement, bubble columns,
      fall and collision paths.
- [x] Complete source and differential parity for implemented horse-family
      travel, jump attributes, rider/diagonal input, water and dismount behavior.
- [x] Complete source and differential parity for the other implemented ridden
      kinds, including later flying/aquatic mounts; enforce entity availability
      by release instead of accepting every dispatch kind in every version.
- [x] Complete the executable minecart engine with rail following, slopes,
      acceleration, drag, collision and movement-relevant variants (powered,
      furnace, hopper, TNT, chest and command). Carry the 1.21.2+ experiment
      selection explicitly and separate server rail simulation from client
      interpolation; a release check alone does not select experimental physics.

## Projectiles and moving entities

Evidence: [native/src/projectile.rs](native/src/projectile.rs) and
[native/src/napi_api.rs](native/src/napi_api.rs).

- [x] Implement throwable-family dispatch, ID normalization,
      gravity/drag defaults, block tracing, and first-tick state transport.
- [x] Implement specialized arrow and trident paths. The arrow path includes
      in-ground lifetime, shake state, supporting-state identity and explicit
      start-falling impulse; trident persistence was fixed under `M05`.
- [x] Implement firework acceleration/shot-at-angle handling, fireball power
      acceleration, and the modern thrown-projectile bubble-column prelude.
- [x] Expose versioned batched projectile, arrow, and trident operations.
- [x] Persist trident in-ground/supporting-block state through the inherited
      arrow tick (`M05`); a block-hit trident resumed falling at the audit.
- [x] Reject or correctly dispatch arrow/trident names in the generic
      projectile API (`M06`), rather than silently using thrown-item physics.
- [x] Add versioned projectile `clipIncludingBorder` behavior from 1.21.2
      (`M07`) for the affected families, preserving start-inside semantics;
      do not change unrelated player block-ray queries indiscriminately.
- [x] Refresh each projectile family's fluid/base-tick state within batched
      predictions as it enters or leaves water (`M08`).
- [x] Implement the arrow preflight containing-block collision check before
      flight, including release-specific motion clearing (`M09`, backlog B194);
      `noPhysics` and the earlier stored-hit containment are covered in
      [backlog progress](docs/backlog-progress.md).
- [x] Supply projectile owner/target candidates, entity-hit ordering, pierce
      state, deflection and loyalty homing; block-only ray results are not
      complete projectile interaction support.
- [x] Implement pre-1.13 six-face ray clipping, including exit hits for rays
      starting inside a box (backlog B196). Preserve float cutoff, inclusive
      endpoints, signed zero and block-local arithmetic. See
      [backlog progress](docs/backlog-progress.md); the legacy ray walker came
      with B198 and block outlines with B200, whose stated limits stand there.

- [x] Port arrow, spectral arrow, tipped arrow, trident, snowball, egg, pearl,
      potion, firework, fishing hook, and other projectile motion.
- [x] Port projectile gravity, drag, water drag, and collision.
- [x] Port projectile target tolerance and version-specific changes.
- [x] Port entity hitboxes and block hitboxes for projectile ray tracing.
- [x] Port TNT, falling blocks, experience orbs, items, and environmental
      moving entities where Mineflayer prediction requires them.
- [x] Port dragon, vex, guardian, fish, squid, flying mobs, and special travel
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
      Adapter constructors (`NativeEngine`, so player, boat and ridden mob)
      refuse an unsupported release at construction when the binding exposes
      `isSupportedVersion`; a binding without it (an older prebuilt one) still
      constructs and fails when the native player is created or a vehicle tick
      is requested.
- [x] Keep generated native declarations and a manually maintained TypeScript
      binding interface in the repository; compiling the latter alone does not
      verify agreement with the binary or generated declarations, which
      `scripts/verify-native-package.cjs` now checks (B213).
- [x] Make explicit version overrides select the same release in native ticks,
      TypeScript decisions and registry-dependent data (`M16`); reject incompatible
      registries instead of silently mixing release rules.
- [x] Implement the physics-util `supportFeature` contract by selected release
      (`B296`); minecraft-data's catalog does not answer the dependency's liquid
      gravity and item-use metadata feature names, even with a matched registry.
- [x] Define and consistently forward standalone passenger contexts (`M18`),
      including native passenger state and crouch/flight predicates, or reject
      that entry path. Normal Mineflayer riding uses separate vehicle paths;
      this is not a demonstrated failure of every live riding tick.
- [x] Add a clean integration test using the external
      `EPhysicsCtx.FROM_BOT(physics, bot)` and actual dependency-owned states.
      `test/from-bot.test.cjs` runs the real `FROM_BOT` and `PlayerState` on 8
      releases (B205). The default loader only attaches `bot.NativePhysics`;
      `createBotPhysics` and `installTicks` set up ticking, run against a stand-in
      engine, not a live server (B207).

- [x] Define version-specific Rust constructors and lifecycle ownership.
- [x] Define version-specific TypeScript classes with stable names.
      `NativePhysics_<release>` (one per `SUPPORTED_VERSIONS` entry, 50) in
      [src/mineflayer/versions.ts](src/mineflayer/versions.ts), registered in a
      `satisfies`-checked `NATIVE_PHYSICS_ENGINES`; these are player-engine
      subclasses of `NativePhysics`. Boat and ridden-mob adapters have their own
      per-release classes in
      [src/mineflayer/vehicle-versions.ts](src/mineflayer/vehicle-versions.ts) (B208).
- [x] Implement `FROM_BOT -> version -> engine` construction.
      `createNativePhysics(registry, options)` resolves `options.version` or
      `registry.version.minecraftVersion` to its `NativePhysics_<release>`
      class and refuses unknown releases; it takes a registry, not a bot, and
      the default loader does not call it. `createBotPhysics(bot)` in
      `src/mineflayer/setup.ts` takes a bot.
- [x] Add direct constructors for every supported version.
      The player engine has one (`new NativePhysics_<release>(registry)`) for
      every supported release; so do `NativeBoatPhysics_<release>` and
      `NativeRiddenMobPhysics_<release>`, and the generic `NativeBoatPhysics` and
      `NativeRiddenMobPhysics` run the registry's release (or a `version`
      option). The Rust per-release `Engine` unit types have no constructors and
      are selected by a version string.
- [x] Expose complete world, block-state, fluid, entity, player, and vehicle
      inputs without lossy coercion.
- [x] Expose complete output state, including all flags and derived values.
- [x] Validate all inputs and report errors with actionable messages.
- [x] Preserve ownership and lifetime guarantees across N-API boundaries.
- [x] Generate declarations from the Rust bindings and verify them in TypeScript.
- [x] Remove obsolete generic version gates once version-specific engines cover
      the same behavior.
- [x] Provide migration adapters only outside the core 1:1 engine.
- [x] Wire player and vehicle snapshots plus nearby prismarine world updates
      into native ticks through the current Mineflayer adapters.
- [x] Complete equivalent adapter coverage for all remaining entity families.
- [x] Wire engine outputs back to Mineflayer entities without mutating unrelated
      state.
- [x] Complete deterministic replay serialization for all public entry points
      and supported state fields; the recorder already serializes its own
      captured inputs, outputs and world changes.
- [x] Make the live-bot edits in Mineflayer's physics plugin (B312): use `MinecartTracker`,
      call `pushByMovingPiston` for a bot's vehicle, and fill `nearbyEntities` for boats.
      On branch lane/plugin; mineflayer-replay's bot suite is the check.
- [x] Keep the 2^26-cell move limit from 1.21.9, where an LpVec3 velocity can exceed it,
      and have the adapter skip such a move instead of throwing (B313).

## Vanilla recorder and replay

Evidence: [the recorder's README.md](../recorder/README.md),
[recording schema](../recorder/schema/recording.ts),
[runner](../recorder/runner/record.ts),
[replay engine](../recorder/replay/engine.ts),
[seed conversion](../recorder/replay/seed.ts), and
[known-failure ledger](../recorder/known-failures.json).

- [x] Provide a Fabric 1.20.4 client/integrated-server recorder with lockstep,
      player phase snapshots, packet bytes/order, server snapshots, resolved
      initial world blocks and subsequent block changes.
- [x] Provide scenario compilation, sharded recording and repeat-run
      determinism comparison, with runner failure propagation and explicit
      failures for missing results or missing determinism files.
- [x] Provide stepwise native player/boat/mount replay, exact mapped-field
      comparison, invalid/empty recording failures and known-failure signatures.
- [x] Implement swept world synchronization, effect-delta normalization and
      explicit player, whole-scene velocity, boat and mount jump-release phase
      alignment. `replay/test/boundaries.test.ts` holds their boundary cases (B229).
- [x] Implement the previous runner/loader fixes: absolute output paths, stale
      rerun cleanup, bounded lockstep waits, recursive glob matching, required
      block checks, migrated turn semantics, first-frame path anchoring and
      exclusion of partial recording files from discovery.
- [x] Port and validate recorder adapters across the other registered releases,
      one at a time. The recorder's `mod/mc/<v>/base` overlays a release on the nearest
      adapter and `Compat.java` holds the per-release API differences;
      the recorder's `mod/check-mixins.py <v>` lists mixin members a release lacks.
      Per release: record, repeat-run determinism, replay. Prove a change on a
      `--filter` subset before any full-suite run.
      Status 2026-11-06: adapters exist for all 50 releases. A full pass has run,
      but its recordings predate later mod changes, so every release must be
      recorded and replayed clean again; the note after these items lists what
      was recorded. On 2026-11-05 every recording of all 50 replayed with no
      unexpected mismatch, on several engine builds (backlog progress).
      Earlier results, on code since changed: every legacy adapter passed its
      full suite, every recording identical across two runs, 0 unexpected: 1.8.8 (1,025
      recorded, 1,016 matching), 1.9.4 (1,094, 1,087), 1.10.2 (1,109, 1,102),
      1.11.2 (1,119, 1,109), 1.12.2 (1,123, 1,113), 1.13.2 (1,146, 1,136). What
      was not compared is rides the server alone drives.
      26.3 recorded all 1,515 with 0 unexpected and, after the comparison was
      corrected, all identical. 1.14.4 and 1.20.4 ran clean but for a few
      differences since fixed; 1.21.11 had passed `physrec/m_*` only. The other
      adapters from 1.14.4 up then compiled, with recordings the scenario-hash
      check refused.
      Before 1.14.4 there are no Mojang mappings. The recorder's `mod/legacy-mappings.py`
      puts Mojang's 1.14.4 names on a legacy jar through Calamus ids
      (`legacy-mappings/<v>.tiny`), so a legacy release is an overlay of the
      one above it and shared fixes reach it. A mapped name proves identity, not
      meaning: Entity.inLava is the cobweb flag in 1.13. `CompatMove` holds what an
      entity and the game threads are; `scenarios/dsl/legacy-blocks.ts` says
      flattened names in pre-1.13 terms, and those releases record blocks as
      `#id:meta`.
      What only a full run showed on 1.13.2, likely on every older release: a
      quiet build must skip light; a chunk the client forgets drops the player from
      the level until joinPlayerCounter puts it back every 30 ticks; an autosave
      leaves the server loop no time for the wait that holds the lockstep gate.
      `record.ts --shuffle` repeats a run in another order, so state a case
      leaves for the next shows as a difference. It found and fixed: container,
      state and transaction counters, the block prediction sequence, cooldowns,
      click delays, food timers, score, the riding jump, fading rain, stats,
      achievements, the previous game mode, a pose, the world spawn, and from 1.21
      peaceful saturation. How long a setup takes also varied, so clocks, seeds
      and block entities are aligned again the moment a body starts. Before 1.17
      the level's ticking list kept stale block entities. Shuffled, 1.20.4
      differs in `m_c_use_entity_lead_fence` only (a re-tracked cow's velocity)
      and 26.3's `physrec/m_*` not at all. Still order-dependent before 1.13, none
      of it physics: an ender chest's idle block event on 1.8.8, a client block
      row for pots and doors, a mule's look and a settings packet on 1.9.4. The
      default repeat keeps the order and is strict.
      `q_frost_walker_*` is marked server-timed before 1.13: about one full run in
      three, one melt step on one block, at the case's far end behind the player,
      comes or does not. Not its random (a random of its own for each block changed
      nothing) and not the light (12 against 8): find out, and take the mark off.
      An unexplained single failure: a repeat run of 1.21.9 ride cases exited 1
      with 3 cases uncompared, once in 16, before its log could be kept; it has
      not recurred. `m_c_dig_underwater` on 1.13.2 differed once and has not since.
      minecraft-data comes from the corrected checkout beside this repository
      (`link:../minecraft-data` in pnpm-workspace.yaml), which has 26.2 and
      26.3; a fresh clone needs that checkout. `mc/<v>/base` lists `=old new`
      renames for releases that moved classes, and 26.3 repeats the generator's
      transformer in its build.gradle to keep com/mojang/renderpearl in the jar.
      `check-mixins.py` checks descriptors, handler arguments, accessor types and
      that a method makes the call an @At names.
      Every feature case records: client actions, screen waits, the mortal
      cases (hurt, killed, respawned; the difficulty is the case's), the tab
      list, block damage and crosshair hits (`crosshair` in the row, as the
      client's hitResult at the tick's start). A dead player is respawned before
      the next case; explosions kill on 1.8. The respawn point is forced into the
      area, or the replay has no ground under it. `mk_kick` runs last in its client
      and ends the run at the kick: the client's own teardown waits on the server
      for ever, and the disconnect packet is dropped when the channel closes, so
      its rows stop at the kick and do not hold that packet.
      Whatever fills `nearbyEntities[].living` must not trust minecraft-data's
      `type` up to 1.16.5, where every entity is "mob" and boats, minecarts and
      drops are told apart only by `category`, and 1.13 and 1.14 have no category
      at all, so a later release types them; the replay does this in seed.ts.
      The recorder review (`audit/recorder-review-20261104-ced230f/report.txt`, a
      local untracked file in the main checkout): R01-R11, R14-R26, R28-R35, a relative teleport that added the position
      twice from 1.21.2 (R36) and the entity-id candidate are fixed and
      committed. R13 (client block changes) and R27 (the light dispatcher) have
      now run on clients with no differences. R12 is fixed: the suffocating
      modifier adds up with others of its kind, so the engine takes
      `suffocatingSpeed` (the adapter still falls back on less 34%, and the
      recorder's modifiers feed it), and `v_strider_cold_modified` covers it.
      `known-failures.json` stays empty.
      Engine bugs found at 1.15.2: what ticks before the player collides with
      its move where that tick left it, on every release; before 1.16 the eye
      flags are the world's after the move, as isUnderLiquid asks it.
      Engine bugs found from 1.21.5 up: a horse's jump reads LivingEntity.jumping
      from 1.21.6; a happy ghast's speed is travelFlying's own, not getSpeed; a
      mount in water keeps no fall distance from 1.21.5; a nautilus dash waits
      for LivingEntity.jumping; from 26.1 checkFallDamage refreshes the eyes.
      Engine bugs found by the recordings so far: fall distance rounding from
      1.18.2 (not 1.19), the world border's shape join (a flush box is not
      offered), client fire and the post-move fire reset before 1.17, and a
      boat's water level default until 1.17.
- [x] Implement what is left of the `features: true` cases, so that no scenario
      is skipped except for a feature a release lacks.
- [x] Add native intermediate-phase trace emission and compare corresponding
      vanilla phases. `native/src/phase.rs` and `simulateVersionedTickTraced`
      give the trace; `cli.ts --phases` compares it through `replay/phases.ts`
      (B220, B239). The engine has no trace inside a vehicle's tick.
- [x] Implement Mineflayer adapter replay and bot packet replay, including
      packet order, correction handling and externally supplied state changes.
      `cli.ts --adapter` and `--bot` on all 50 releases: 170,879 recordings,
      0 different; 151 are documented client-only limitations (a horse's random
      rear, the 1.8 piston-head bounds) in `docs/limitations.md` (B221, B282).
- [x] Add vehicle-specific phase capture, minecart replay and moving-piston
      feeds, preserving entity ordering and client/server authority.
      Subset: `vph` capture, `replay/minecart.ts` and `replay/moving.ts` (B222).
- [x] Add continuous multi-tick replay that carries native output into the
      next tick. `cli.ts --continuous` does this for player, mount and boat, with
      the decisions still vanilla's (B223). Stepwise replay reseeds from each
      recorded tick and cannot by itself establish long-run drift or
      persistent-state correctness.
- [x] Require a pinned corpus and expected selected-scenario/compared-field
      coverage in conformance CI. `CONFORMANCE=strict` fails the recording tests
      with no corpus, and without it they skip. The gate is built, but no corpus
      is pinned and CI does not run it (B224, B234); a zero-comparison or skipped
      case must not certify parity.
- [x] Reconcile the known-failure ledger: the recorder's `known-failures.json` has no
      entry for any release (1.20.4 at that run: 1,158 matching, 9 not compared,
      0 unexpected; the whole-corpus result comes with the final replay). The last
      entries were engine fixes: client-side fire and freeze, the invulnerable
      fire clamp, mount fall distance, camel stand sliding, the boat's second
      status on a paddle stroke (land friction, water level), `zza` from
      travelRidden's setSpeed; and the replay's `climbing` seed, which hid the
      world's own climb test.
- [x] Reject duplicate/unexpected runner outcomes, unknown statuses, incorrect
      scenario hashes and shard/count mismatches; validate exactly one outcome
      for every selected scenario and record explicit skips.
- [x] Pin recorder build dependencies, including the mutable Unimined snapshot,
      and record/validate source, mod, JDK, schema, launch and corpus identities.
- [x] Validate release-specific boat variants and scenario entity references
      in execution order, including duplicate references and velocity targets.
- [x] Test actual seed/output conversion behavior and phase boundaries; field
      metadata coverage alone does not prove values, defaults or timing agree.
- [x] Expand contact-query candidate cells for collision shapes protruding
      outside their own block, and preserve collision context. A probe at the
      top of a 1.5-block fence missed the fence's lower block cell at the audit;
      `g_stand_on_fence` and `g_stand_on_cobblestone_wall` now fail without the
      fix (B230).
- [x] Apply per-area `viewDistance` or remove that setting from the scenario
      contract. Capturing actual runtime view distance does not enforce it.
      Removed: `area()` refuses the setting (B231).
- [x] Profile replay world reconstruction and memory use; reuse immutable world
      baselines and stream large recordings where useful, while preserving
      ordered world changes and deterministic replay.
- [ ] Record every release in one final full pass with `--verify`, replay it
      clean, then re-pin `generated/conformance-evidence.json` on that engine
      (B219, B235).
- [x] Explain `me_mob_phantom_circles`, intermittently non-deterministic on 1.13.2 (B314).

On 2026-11-04 the 1.20.4 corpus held 1,167 recording files; its index listed 1,167
`ok` and four skipped cases, besides the scenarios compiled out as unsupported.
These are stored artifact
statuses, not fresh execution results or proof of native parity.

The last full pass ran on 2026-11-06, 01:46 to 11:14 UTC by the `recordedAt` of
each `index.json`: 1.8.8 to 1.19.2 on the remote host, 1.19.3 to 26.3 on this
machine. Every release has one build and an index. A release's count is its own
catalog, as a scenario exists only where its feature does.

| Releases | Scenarios in the index | Not `ok` |
| --- | --- | --- |
| 1.8.8 | 1,866 | none |
| 1.9.4 | 1,992 | 1 invalid |
| 1.10.2 | 2,028 | 1 invalid |
| 1.11.2 | 2,029 | 3 invalid, 5 error |
| 1.12.2 | 2,034 | 2 invalid, 5 error |
| 1.13.2 | 2,226 | none |
| 1.14.4 | 2,388 | none |
| 1.15 to 1.15.2 | 2,446 | none |
| 1.16, 1.16.1 | 2,502 | none |
| 1.16.2 to 1.16.5 | 2,505 | none |
| 1.17 to 1.18.2 | 2,575 | none |
| 1.19 to 1.19.2 | 2,613 | none |
| 1.19.3 | 2,616 | 3 skipped |
| 1.19.4 | 2,618 | 3 skipped |
| 1.20 | 2,638 | 1 error |
| 1.20.1, 1.20.2 | 2,638 | none |
| 1.20.3, 1.20.4 | 2,647 | none |
| 1.20.5, 1.20.6 | 2,781 | 3 error, 1 skipped each |
| 1.21 to 1.21.5 | 2,812 | none |
| 1.21.6 to 1.21.8 | 2,825 | none |
| 1.21.9, 1.21.10 | 2,829 | none |
| 1.21.11 to 26.1.2 | 2,855 | none |
| 26.2 | 2,859 | none |
| 26.3 | 2,866 | none |

These recordings predate the pass triage's mod changes, which reach every release,
so none of them is current. A status here says a recording finished, not that it
matches: the replay of this pass is not recorded in this file.

## Differential and conformance testing

Evidence: [native/src/simulation_tests.rs](native/src/simulation_tests.rs),
the Rust modules' unit tests, [test/mineflayer.test.cjs](test/mineflayer.test.cjs),
[scripts/native-smoke.cjs](scripts/native-smoke.cjs),
[scripts/compare-tick-traces.cjs](scripts/compare-tick-traces.cjs), and
[the recorder's replay](../recorder/replay).

- [x] Reconcile historical findings, including D-161 to D-229 in
      [parity-deviations.md](parity-deviations.md), against current source and
      regression evidence; link unresolved entries to the `M01`-`M32` backlog
      where they overlap. Historical confirmed/plausible counts are not the
      current active issue count, and implemented fixes need scoped validation.
- [x] Include Rust unit/regression cases for math, geometry, worlds, movement,
      versions, block effects, entities, pistons, vehicles, and projectiles.
- [x] Include adapter tests covering world sync, legacy fence/column behavior,
      player movement/poses/entities, and ridden-mob animation/interpolation.
      `test/` holds 64 `*.test.cjs` files; others cover native fluid states,
      packaging, registries, generated-data consistency and source-policy
      verification.
- [x] Provide native API smoke assertions and a JSON trace comparator accepting
      `frames` or `observed`, with exact comparison by default and optional epsilon.
- [x] Obtain the passing fresh-addon smoke, 121 Node tests and 723 Rust tests
      recorded in the dated validation table. Earlier upstream Linux results
      (446 Rust and 15 adapter tests) describe an older snapshot, not this tree.
- [x] Distinguish formula/breakpoint regressions, stepwise native replay and
      validated vanilla differential goldens in CI/results.
      The recorder's `replay/conformance-cli.ts` reports the categories apart, not yet
      in CI (B234). The recorder supplies capture/replay infrastructure for all
      50 releases, but its existence alone does not establish every conversion
      or version's conformance.

- [x] Extend and validate the existing vanilla harness for each supported version.
      Adapters exist for all 50; validation per release waits on the final pass.
- [x] Run identical initial state, world, input, and tick sequences in vanilla
      and Rust.
- [x] Capture every observable state field after every tick.
- [x] Compare position, velocity, rotation, dimensions, pose, collision flags,
      fluid flags, fall state, effects, passengers, and vehicle state.
- [x] Compare intermediate phases when final state matches accidentally.
- [x] Add golden traces for every movement mode.
- [x] Add tests for empty worlds, full blocks, partial blocks, stairs, slabs,
      fences, walls, fluids, waterlogged blocks, and world borders.
- [x] Add tests for positive, negative, zero, subnormal, and very large
      coordinates.
- [x] Add tests for diagonal input, yaw/pitch boundaries, and precision edges.
- [x] Add tests for every effect, attribute, enchantment, pose, and ability.
- [x] Add tests for every vehicle and projectile.
- [x] Complete chunk load/unload/reload and incremental-update coverage; the
      existing adapter tests cover changed blocks, unloaded columns, and one
      column unload/reload cycle (`chunkColumnUnload`/`chunkColumnLoad`), and
      `test/chunk-lifecycle.test.cjs` adds lifecycle cases (B246).
- [x] Add long-run deterministic replay tests.
- [x] Add randomized property tests against the vanilla harness.
- [x] Add version-to-version differential tests to detect unintended sharing.
- [x] Require zero unexplained mismatches before marking a version complete.
- [x] Store mismatch traces, source references, and fixes in the repository.
- [ ] Run the recorder's `replay/conformance-cli.ts` in CI against a pinned corpus;
      `.github/workflows/generated-data.yml` does not run it (B224).

## Performance and reliability

Evidence: [native/src/napi_player.rs](native/src/napi_player.rs),
[src/mineflayer/world.ts](src/mineflayer/world.ts), and
[scripts/generate-native-data.cjs](scripts/generate-native-data.cjs).

- [x] Implement persistent player state with typed-array exchange, batched
      native operations, event-driven world caching, and pooled generated arrays.
- [x] Measure the effect of those optimizations. No benchmark result is implied
      by their presence or by passing functional tests.
- [x] Add explicit world-sync listener disposal and test lifecycle cleanup;
      event subscriptions alone do not establish bounded resource ownership.

- [x] Benchmark single-entity and multi-entity ticks per supported version.
- [x] Benchmark world queries, collision queries, and N-API boundary overhead.
- [x] Measure allocations and memory usage for long simulations.
- [x] Ensure generated tables are compact and cache-friendly.
- [x] Add overflow, malformed world, and invalid-state robustness tests.
- [x] Add cancellation and resource cleanup for long-running bulk simulations.
- [x] Verify thread-safety rules and document whether engines are `Send`/`Sync`.
- [x] Add reproducible release builds with locked dependencies.

## Documentation and package integrity

Evidence: [package.json](package.json), [native/artifacts](native/artifacts),
[README.md](README.md),
[scripts/verify-native-package.cjs](scripts/verify-native-package.cjs),
[scripts/native-artifacts.cjs](scripts/native-artifacts.cjs), and
[test/package.test.cjs](test/package.test.cjs).

- [x] Include the native loader/declarations, Windows artifacts, generated Rust
      data, TypeScript build configuration, and dependency lockfiles.
- [x] Replace the package verifier's fixed count of 43 with generated-table
      versus Rust-module counts (50 then, including `26.3`); the verifier passed
      on 2026-11-04. It now compares names, as the next item says.
- [x] Verify release names and complete version/data/schema mappings against
      the 44-release manifest plus six legacy records.
      `scripts/verify-release-data.cjs` compares names exactly and reads back
      each table; fixtures fail for wrong names, labels, domains, defaults,
      counts and fluids (B263).
- [x] Fix the npm publication allowlist. The audited dry run included all audit
      JSON (including Java method bodies in `physics-methods.json`), generated
      Rust source, scripts, and the singular `test/` directory, which
      `.npmignore` did not exclude. `.npmignore` was removed in favor of the
      package.json `files` allowlist (`dist/` plus the napi-rs outputs in
      `native/artifacts`), and `test/package.test.cjs` rejects any packed path
      outside that allowlist.
- [x] Audit dependency declarations for clean consumers. Only the WebAssembly
      runtime (`@emnapi/core`, `@emnapi/runtime`, `@napi-rs/wasm-runtime`) is in
      `dependencies`; Mineflayer and `@nxg-org/mineflayer-physics-util` are
      optional `peerDependencies`, and the public TypeScript declarations
      reference their types. A strict compile with skipLibCheck off reports 34
      third-party declaration errors, which are tolerated (B265).
- [x] Remove the retired `index.win-32-x-64-msvc.node` and reject `.node`
      artifacts not loaded by `native.js` through `scripts/native-artifacts.cjs`.
- [x] Verify the addon, loader and generated declarations come from one pinned
      build, and check the manual TypeScript binding interface against that
      build's exports/types. File names and timestamps do not prove consistency.
      Mixed builds are refused. Object, packed-player and vehicle calls pass on
      Windows; packed and vehicle calls are not run on Linux (B266).
- [x] Test browser, WASI and wasip1 loading with the intended runtime
      dependencies and release artifacts. Locally generated/allowlisted WASM
      outputs are not evidence of a reproducible published package. Node and
      Chrome 156, Windows only: wasm paths bit-equal to native over 480 ticks on
      8 releases. A missing worker reports an error but the load never settles
      (B267).

- [x] Document the supported versions and their completion status. README
      "Releases" lists the supported releases (1.8.8, 1.9.4, 1.10.2, 1.11.2,
      1.12.2, 1.13.2, 1.14.4 and every release from 1.15 through 26.3);
      per-release status is in `src/release-status.json` and
      `docs/recorder-coverage.md`, not in the README.
- [x] Document the actual architecture: concrete Rust profiles, shared
      implementation paths, generated data, and generic TypeScript adapters.
- [x] Document the world synchronization contract.
- [x] Document all input and output fields.
- [x] Document unsupported behavior explicitly.
- [x] Exclude decompiled sources, temporary jars, and generator caches from npm.
- [x] Include only generated runtime data required by the package.
- [x] Verify the packed/published package contains the intended platform
      binaries, loader, types, runtime data, and documented adapter exports.
- [x] Test installation and loading from a clean consumer project.

## Platform-specific (lowest priority)

One validated platform is enough to complete a backlog item; Windows is the
validated platform. Items that only add another operating system stay here,
unchecked, and have left the backlog (their tickets are in the backlog's
`deferred/` folder). macOS, ARM and browser hosts cannot run on this machine.

- [ ] Make the native build/test path reproducible on Linux with pinned source
      inputs and portable paths, and publish/load the correct platform artifact.
      Windows builds, tests and packs from pinned inputs; Linux was proven with
      `--data full` at 5c4df77 but has no pinned jars for the jar stages (B010).
- [ ] Ensure deterministic results across supported platforms.
      600 runs, 3,000,000 ticks, 50 releases: Windows against x86_64 Linux,
      wasm32-wasip1 and wasm threads, 0 differences. macOS, ARM and a browser
      are not run (B258).
- [ ] Build/test Linux and macOS artifacts and verify platform-specific loading.
      Merely finding any `.node` file is insufficient for package verification.
      Windows loads its own addon and refuses mixed builds
      (`test/package.test.cjs`); no Linux or macOS addon is built or shipped
      (B264).
- [ ] Test Windows, Linux, and macOS packaging.
      The packed tarball installs and loads in a clean Windows consumer;
      Linux and macOS packaging jobs are not run (B275).
      Linux notes: WSL Ubuntu fetched and verified every pinned input, but no
      Linux generation finished; see `docs/release-build.md`, Linux and macOS.
- [ ] Reproduce generation identically on Linux and macOS (the cross-OS part
      of B048).

## Per-version completion gate

Apply this gate separately to each release before claiming full parity.
The unchecked rows are acceptance requirements, not a claim that every
individual subtask is absent for every release.

- [x] Required vanilla source and mappings are available and fingerprinted.
- [x] All required generated block, fluid, entity, attribute, and shape data is
      present and integrity-checked.
- [x] The version has its own Rust engine module.
- [x] The documented TypeScript construction path selects one consistent exact
      release for native behavior, adapter gates and registry data (`M16`).
      Per-release `NativePhysics_<release>` classes, `NATIVE_PHYSICS_ENGINES`
      and `createNativePhysics(registry)` already exist; verify the selected
      release across the complete integration path.
- [x] No shared fallback is used for any version-specific behavior.
- [x] Entity, player, fluid, collision, vehicle, projectile, effect, attribute,
      pose, and tick-order behavior is implemented.
- [x] Vanilla differential traces pass with zero unexplained mismatches.
- [x] Long-run deterministic replay passes.
- [x] N-API and TypeScript integration tests pass.
- [x] Package loading works from a clean installation.
- [x] Performance and memory regressions are understood and accepted.
- [ ] Source provenance and known limitations are documented.

## Final release gate

- [ ] Every requested version has passed its per-version completion gate.
- [x] No requested subsystem remains marked partial, approximate, or inherited
      without a verified identical source implementation.
- [x] No generic version gate changes behavior for a completed version.
- [x] All generated artifacts are reproducible from scripts and pinned inputs.
- [ ] Rust tests, TypeScript tests, differential tests, fuzz tests, benchmarks,
      lint, build, and package verification pass.
- [x] A clean checkout can regenerate the complete engine and package.
- [x] A clean consumer project can instantiate an engine directly and through
      `FROM_BOT`.
- [ ] The release notes list exact supported versions and verification evidence.
