# Validation records

A result is worth only as much as the identity of what ran. The recorded November 4 results described a snapshot, and
a passing smoke test does not name the binary it loaded.

## What a record holds

`node scripts/validation-record.cjs` prints the commit, a digest of any changes on top of it, the engine digest, each
addon by content, the hash of each generated file, the toolchain, and the environment overrides that change what runs
(`CARGO_TARGET_DIR`, `NODE_PATH`, `REPLAY_RECORDINGS`, `NAPI_RS_NATIVE_LIBRARY_PATH`,
`MC_SOURCE_ROOT`). With `-- <command>` it runs the command and adds its exit code and an output hash.

The recorder's `replay/cli.ts --json` writes `replay` beside its results, with the addon's hash, codec, commit and dirty state.
A pin in `generated/conformance-evidence.json` holds the same identities.

The addon's hash is not stable across builds of the same source: the clean source built as `7fab101f`, `7a7fba4d` and
`084cb5be`. So the engine digest, which is of the source, is what decides whether a pin is current, and the addon's hash
says which binary ran.

## Completion stays blocked

`admitted()` needs all four dimensions (`docs/parity-contract.md`). A native change takes every pin, a replay that does
not all match leaves a cell `partial`, and the recorder's `known-failures.json` stays empty. Stale evidence is reported as
stale, and `test/parity-contract.test.cjs` proves each of those cases.

## A deliberately reintroduced bug

Run 2026-11-06 on commit `3d5ce08` plus the fixture, rustc 1.95.0, Node 24.15.0. Each mutation was applied alone to the
source, the addon rebuilt, and the checks run on that addon, then the source restored.

| Mutation | Engine | Addon | Focused tests | `physrec/fd_*` on 1.21.5 |
| --- | --- | --- | --- | --- |
| None | `8c531ab6` | `084cb5be` | 8 pass | 41 of 41 |
| The double-form gate of `attribute_landing_damage` moved from 1.21.5 to 1.21.6 | `7fba8a4b` | `77f6117a` | **1 fails** | 41 of 41: **not detected** |
| 1.21.5's rounding set to `Ceil` | `3c6b8f63` | `fda8cc9d` | **6 fail** | **refused**: `the 1.21.5 profile breaks its behaviour contract` |

The first mutation is the finding that matters. It first passed everything: the existing fall-damage tests, all 41
`fd_*` replays on 1.21.5 and on 1.21.4, and the first version of the new fixture, whose landing move added 0.2 to the
fall distance and carried it out of the window where the forms differ. Only a fixture at exactly 3.9999995 blocks tells
them apart. So a matching replay shows that no recorded scenario reached a difference, and no more than that.

`test/release-boundaries.test.cjs` holds that fixture. Not every gate has had this done: this was one gate and one
policy, not an audit of all the gated functions in `generated/shared-gates.json`, 295 at this writing.

## A failure this process hit

The machine rebooted during the restore build, leaving the mutated `77f6117a` addon in `native/artifacts` with clean
sources. A check on the identity caught it, since the file's hash was the mutation's, and the fixture failed on it.
Line endings aside, the sources had been restored. A record taken before trusting a result is what shows this.

## Moving gates, a wider run

Eight gates were each moved by one release, in one build. A gate moved so changes one release alone, so a failure on that
release is that gate's. The replay is a selection of that release with `--phases`, and the unit column is all of
`test/*.test.cjs` on that addon.

| Gate moved | Replay of its release | Caught by |
| --- | --- | --- |
| 1.20.5, `jump_from_ground`: no jump up to `1.0E-5` | 1 of 119 fails | replay, `e_attr_jump_strength_0` alone |
| 1.21.2, `jump_from_ground`: `max(power, y)` | 104 of 104 match | **nothing**, until a fixture was added |
| 1.21.5, `accumulated_fall_distance` | 158 of 161 fail | replay |
| 1.21.6, the post-move fire reset | 30 of 30 match | unit: `test/authority-boundaries.test.cjs` |
| 1.21.9, `movements_this_tick`: the axis order | 288 of 288 match | **nothing**, until a fixture was added |
| 1.21.11, ridden water float | 22 of 44 fail | replay |
| 26.2, boat restitution | 28 of 98 fail | replay |
| 26.3, `canSimulateMovement` | 55 of 55 match | unit: `test/piston-authority.test.cjs` |

So traces caught four of eight, unit tests two more, and two gates had no check at all. With the 1.21.5 double-form gate
above, three of nine moved gates had no check until a fixture was written, and two more only a unit test. A pinned cell says
the recorded scenarios match. It does not say they reach every branch.

The two fixtures, `test/release-boundaries.test.cjs` (a jump with a faster rise) and
`the_recorded_move_splits_by_the_requested_axes_from_1_21_9`, were then proved the same way:

| Gate moved | Before the fixture | With it |
| --- | --- | --- |
| 1.21.2, `max(power, y)` | nothing fails | the jump fixture fails |
| 1.21.9, the axis order | nothing fails | the Rust test fails |

That unit column also failed shape tests on many releases on the mutated addon, none on the clean one. Which of the eight
moved them was not worked out: the batch shows that some gate does, not which.

This is nine gates and one policy, of the 295 gated functions in `generated/shared-gates.json` at this writing, chosen by hand
at the listed boundaries: not a sample.

## A failure this process hit: a mutated addon called clean

The first restore of those eight gates copied the sources back with their old modified times, so cargo saw nothing to do
and "build clean" left the mutated addon in place, with an exit of 0 and clean sources. A run of finding replays and pins
then ran on it. `1.21.5/physrec/ps_death_tnt_scale_*` failed by 3.7e-9 in `fallDistance`, on that release alone, which is
the signature of the moved gate and no engine bug. The results were thrown away, the sources touched, the addon rebuilt
(the build log now has to show a compile), and everything above re-run on it. A build's exit code does not show that a
binary is what its source says: only its behaviour does.
