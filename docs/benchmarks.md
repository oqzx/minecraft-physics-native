# Benchmarks

Measured on 2026-11-06 on one shared Windows machine, with other builds and recordings running. Every result file in
`docs/benchmarks/` carries the commit, the addon and generated-data hashes, the CPU, the Node and Rust versions, the release
profile and all raw repetitions.

## How to read these numbers

Exact, and unaffected by load:

- counts: heap bytes and allocations a native tick makes, cells, shape candidates and transferred bytes
- table and addon sizes
- digests

Load-sensitive, whatever the clock:

- All CPU times. Their `spread` ((max - min) / median of the kept repetitions) has a median of 0.25 to 0.40 here, and single
  samples go well past that.
- Wall clock latencies and the stage shares, which are labelled `loadSensitive` in the files. The median wall-to-CPU ratio was
  1.24, up to 3.65.

A CPU time here says what order a cost is. A difference under about 40% between two figures is noise, and a release
that looks faster than its neighbour has not been shown to be. For that reason no time budget is set: the ticket asks for one
only after a stable baseline, and this machine does not give one. Regressions are guarded by what does not move:
`native/tests/allocations.rs` for the native heap, and the replay budget in the recorder's `replay/profile-budget.json`.

## Commands

```
node scripts/bench/run.cjs --suite ticks          # every release, about 15 CPU minutes
node scripts/bench/run.cjs --suite overhead
node scripts/bench/run.cjs --suite optimizations
node --expose-gc scripts/bench/memory.cjs [--scale n]
node scripts/bench/tables.cjs
node scripts/bench/table-digest.cjs out.json      # every shape lookup, by digest
cargo test --release --manifest-path native/Cargo.toml --test allocations -- --nocapture
```

Each timed path first proves its outcome equal to its baseline's, to the bit of position and velocity, and a difference stops
the run. `--min-cpu-ms` sets a repetition's length; these results used 250. Five repetitions are kept after two of warmup.

`ticks-win32.json` was measured before the release-comparison and shape-module commits, neither of which touches a
native tick path. The rest were measured after them, except the `*-before-release-cache-*` files.

## Ticks by release (`ticks-win32.json`)

All 50 registrations have an entry: `empty`, `dense`, `fluid`, `interaction` and `vehicle` (a boat), under the scalar, kept
(`NativePlayer` with typed arrays) and batch paths where the path exists. No scene is unsupported in any release. minecraft-data has
no table of 26.1.1 or 26.1.2, so their scenes are 26.1's blocks and only the ticks are theirs.

CPU microseconds per tick over the 50 releases:

| scene | scalar object call, min / median / max | kept player | batch |
| --- | --- | --- | --- |
| empty | 78 / 91 / 118 | 11 / 13 / 27 | 5.7 / 8.3 / 12 |
| dense | 99 / 118 / 157 | 25 / 34 / 45 | 20 / 28 / 48 |
| fluid | 135 / 167 / 219 | 58 / 78 / 115 | 52 / 67 / 88 |

Startup, conversion and steady state are apart in `scaling`, on 1.12.2, 1.16.5, 1.20.4 and 26.3:

- `<E>x0` is a batch of no ticks, conversion alone: 42 to 99 µs per state.
- `<E>x200` is steady state plus a little conversion: 21 to 33 µs an entity tick, flat from 1 to 64 entities.
- A kept `NativePlayer` costs 35 to 43 µs to make, and a cold 90k-cell world 240 to 390 ms.
- The interaction pile stays near 30 µs an entity tick from 1 to 256 entities, four ticks long. The pairwise push loop is
  quadratic, but at this size it does not show past the noise. In all 50 releases the pile's outcome changes when
  its order does, so the order is real work and the batch keeps it.

Latency, wall clock and load-sensitive: a kept tick's median is about 25 µs. Its p99 ran from 0.3 to 4 ms, which is the other
work on this machine as much as the engine.

## World, collision and the boundary (`overhead-win32.json`)

Figures are for 1.12.2, 1.16.5, 1.20.4 and 26.3; the ranges are over those four. Each stage is timed alone.

| stage | CPU | workload |
| --- | --- | --- |
| empty N-API call | 0.2 - 0.4 µs | `isSupportedVersion` |
| scalar world query | 0.4 - 0.5 µs | `isInsideBuildHeight`, `fluidAt`, `blockTagsAt` |
| object result | 3.4 - 4.0 µs | `blockAt` |
| object state in, out | 24 - 26, 26 - 29 µs | 84 fields |
| typed arrays | 1 µs to copy | 232 bytes in, 192 out |
| shapes, player box | 34 - 40 µs | 45 cells, 13 boxes |
| shapes, large | 1.2 - 1.4 ms | 1445 cells, 703 boxes, 34 kB of results |
| shapes, huge | 11 - 13 ms | 12005 cells, 6336 boxes |
| swept tick at 0.2, 8, 40 blocks a tick | 90 - 102, 101 - 163, 171 - 318 µs | object path, conversion included |
| cold sync | 250 - 330 ms | 90307 cells |
| warm sync, nothing changed | 0.03 - 0.05 µs | event-driven skip |
| one block changed | 0.7 - 1.0 ms | rewrites its neighbours too |
| chunk churn | 17 - 21 ms | one new 16 × 19 × 16 column |
| block mapping | 0.5 - 1.3 µs a block | `describe`, cached per state id afterwards |

The boundary's cost is the object conversion. A scalar tick spends about 50 µs on it, which is most of an empty tick. A query
returning objects costs about 1.8 µs per box and is the only stage that grows with the shapes.

An adapter tick in the dense scene, CPU and then wall-clock shares from one run:

| release | CPU | sync | native | adapter JS | total wall |
| --- | --- | --- | --- | --- | --- |
| 1.12.2 | 27 µs | 0.3 | 17 | 30 | 48 |
| 1.16.5 | 85 µs | 0.4 | 33 | 51 | 84 |
| 1.20.4 | 49 µs | 0.3 | 24 | 53 | 78 |
| 26.3 | 64 µs | 0.7 | 35 | 88 | 123 |

So the adapter's JavaScript takes more of a tick than the engine. Most of a tick's sync is the skip, and a tick that changes
blocks adds its row from the table above.

## What the optimizations give (`optimizations-win32.json`)

A release against itself, one player, 200 ticks, outcomes equal to the bit. The ratios are CPU time over the object-call
baseline and carry the noise of both sides.

| release | plain: kept, batch | dense: kept, batch | fluid: kept, batch |
| --- | --- | --- | --- |
| 1.12.2 | 0.19, 0.11 | 0.29, 0.21 | 0.88, 0.80 |
| 1.16.5 | 0.11, 0.07 | 0.28, 0.20 | 0.50, 0.39 |
| 1.20.4 | 0.13, 0.06 | 0.22, 0.16 | 0.47, 0.43 |
| 26.3 | 0.17, 0.12 | 0.33, 0.28 | 0.51, 0.45 |

- Kept players and batches save the object conversion, so the saving is largest where a tick does least. It is real in every
  row but the 1.12.2 fluid one, whose 0.88 is inside the noise: the ticks suite gives 0.50 for the same path.
- The event-driven sync of 2601 cells costs 0.02 µs where a rescan costs 350 to 1340 µs, with equal world digests. This
  is the largest effect here, at the cost of listeners that disposal must release (B253).
- The pool of distinct shapes is counted, not timed: `pooling` in `tables.json` gives the boxes with and without it.

## Finding: the release comparison

`lt` and `gt` split two strings on every call, and a tick asks about a hundred times. The allocation profile named them
first. An engine's release never changes, so it now keeps each answer (`NativeEngine.againstRelease`).

| release | JS allocated a tick, before → after | whole tick CPU, before → after |
| --- | --- | --- |
| 1.12.2 | 39.7 → 11.7 kB | 58 → 27 µs |
| 26.3 | 50.0 → 24.7 kB | 122 → 64 µs |

The allocation is a sampled heap profile. A later run of the same code gave 16.3 and 26.6 kB, so it moves by some kB, far less than the change. The CPU column is load-sensitive: all four
releases fell, by about half, but the before figure of 1.12.2 has a spread of 2.9, so read it as a direction and not a ratio.
What remains in a tick is `borderLeavesFree`, `Object.assign`, `poseFits`, `attributesAndEffects` and `Object.entries`.

## Memory (`memory-win32.json`, `memory-long-win32.json`)

Each workload ran alone in its own process, on 1.12.2, 1.20.4 and 26.3, with the system allocator, 12 rounds and a full GC after
each. A turn of the event loop before the GC matters: a `WeakRef` holds its target to the end of the job, so a workload
that never yields shows every dropped world as retained, which is an artefact of the harness.

| workload | heap after warmup | RSS | caches |
| --- | --- | --- | --- |
| stationary, 24000 ticks | flat, under 6 kB a round | flat, 100 - 125 MB | 3 listeners |
| traversal, 2400 chunk steps | bounded | 340 - 430 MB | `ids` stays under `CACHE_LIMIT`, which resets it and the NativeWorld |
| churn, 240000 changes of a cell | flat | steps, then flat | `changed` 0 |
| replacement, 1200 worlds left unreleased | flat, under 10 kB a round | flat, about 245 MB | the worlds go with GC |
| entities, 600 batches of 256 states and 20 ticks | flat | a step to 220 MB, then flat | none |
| construction, 1200 engines | flat | steps to 220 MB, then flat | 0 listeners left |

`entities`, `construction` and `churn` showed a slope in the first run, which is why `memory-long-win32.json` repeats them at five
times the work on 26.3: RSS steps to 220 MB and holds. That is retained allocator capacity, not growth. `external` stays flat in
every workload, since the addon reports nothing to V8. So a NativeWorld a sync drops waits for a GC that does not know its
size. Traversal is bounded with this, and its 430 MB is the measured cost of it.

The native tick (`native/tests/allocations.rs`, exact counts): 3.5 allocations and 174 bytes a tick on 1.8.8, 1.12.2 and 1.20.4,
4.5 and 398 on 1.16.5, 24 and 2336 on 1.21.11 and 26.3. Every release keeps 0 bytes over 5000 ticks. The walker
runs off its floor partway, so these counts include falling. The JavaScript side allocates about 12 to 27 kB a tick, all young
garbage. Native allocation is not where a tick's memory goes.

## Generated tables (`tables.json`)

| | files | distinct | source bytes | legacy | modern |
| --- | --- | --- | --- | --- | --- |
| `native/data/releases` | 50 | 50 | 88.5 MB | 3.1 MB (6) | 85.4 MB (44) |
| `native/data/shapes`, before | 44 | 21 | 16.7 MB | none | 16.7 MB |
| `native/data/shapes`, now | 21 | 21 | 8.1 MB | none | 8.1 MB |

What the shape tables cost, from a build with `block_shapes::table` empty:

| | addon | `.rdata` | `.reloc` | `.text` |
| --- | --- | --- | --- | --- |
| with shapes | 16.54 MB | 13.29 MB | 0.83 MB | 1.861 MB |
| without | 12.32 MB | 9.52 MB | 0.40 MB | 1.855 MB |

So 16.7 MB of Rust costs 4.2 MB of addon, a third more. The load does not pay for it: `require` took 4.3 and 4.5 ms of wall clock,
median of 30 alternating loads, within noise, with RSS at load 0.8 MB either way, because the tables are mapped and paged on
use.

23 of the 44 tables were the same to the byte, and now share one module (`generate-block-shapes.cjs`). Each release keeps its own
`.json.gz` and report row, so provenance per release is untouched. The addon did not change size at all, to the byte in
every section: the linker had already folded them. The gain is 8.6 MB of source for rustc to parse.

All 4289260 shape lookups (every block, state and channel of all 50 releases) have the same digests before and after
(`table-digest.cjs`). `generate-all.cjs --check --cached` finds no drift from a fresh generation; a release whose jar
is not at hand keeps its table under `--cached`.

A lookup costs 0.2 to 1.3 µs and rises with the block count (a linear `find`), but the adapter caches a block's description per
state id, so no tick pays it. Lines shared across releases come to 1.6 MB of the 16.7 MB, so a cross-release pool could go further,
at the price of the per-release layout. With the linker already folding, and no measured load or lookup cost to win back, it
is not worth doing.

## Re-run on the candidate of 2026-11-09 (B286)

Commit `dd35efdb`, engine digest `5a9661a8`, the same machine, other lanes' builds and recordings running. The results are
`docs/benchmarks/ticks-20261109-win32.json` and `docs/benchmarks/memory-20261109-win32.json`; the files above stay the
2026-11-06 baseline (commit `242aedb2`), 56 native commits earlier.

- The native heap, exact: `native/tests/allocations.rs` gives the baseline's counts to the digit on every release it runs (3.48
  allocations and 174 bytes a tick on 1.8.8, 1.12.2 and 1.20.4, 4.48 and 398 on 1.16.5, 24.1 and 2336 on 1.21.11 and 26.3) and
  0 bytes kept over 5000 ticks. That test had stopped compiling, because `SimulationState` gained a private field its struct
  update could not name, and `cargo test --lib`, which the gate runs, does not build it. It is mended, and needs a run of its own.
- Memory: every workload is flat over its rounds (the last half's maximum within 30 kB of the first half's), traversal rises and
  falls with the GC as before, and no listener is left. The JavaScript heap sits 0.5 to 2.2 MB above the baseline in every
  workload from the first round on: a constant, not growth.
- CPU: the median of new over old is 0.64 to 0.72 for every scene and path over the 50 releases, the worst 1.13 (one batch of
  `empty`). Within this machine's noise that is no regression; it is not shown to be a gain either.
- Outcomes: the digest of each scene's result is the baseline's in every release but 26.2 and 26.3, whose `empty`, `dense` and
  `interaction` scenes move differently now. Engine changes since the baseline did that, not a timing; which commit was not
  worked out. Every 26.2 and 26.3 recording of the corpus replays without a difference on this engine.

Accepted: no regression in an exact count or in memory, and the CPU figures are inside the noise this page describes.

## Determinism

See `docs/determinism.md`.
