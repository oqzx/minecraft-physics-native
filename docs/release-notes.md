# Release notes

## Unreleased: 1.0.0 candidate

This is not a release. `package.json` says 1.0.0, nothing is tagged, and no release is certified to match vanilla. These notes
state what the tree holds. The final pass result below is generated from the release matrix record (`scripts/release-matrix.cjs`)
and the per-release evidence rows (`scripts/release-evidence.cjs`), and claims nothing those records do not show.

<!-- release-notes.cjs: begin. Generated from the release records: do not edit. -->

### Final pass result

From `audit/release-matrix/dd6395114b56-2026-11-10.json` and `audit/release-evidence/dd6395114b56-2026-11-10.json`. It counts what those records show, and nothing older.

| Item | Value |
| --- | --- |
| Commit and engine digest of the pass | `dd6395114b5693a616f3729d69053262fbf5f893`, engine `ac180bd41a5e` |
| Matrix verdict | red (605 reason(s), `docs/release-matrix.md`) |
| Units passed | 4 of 5 run; 3 job(s) on 3 release(s) asked for |
| Pinned corpus | 9626 of 9626 recordings |
| Known failures | 0 |
| Evidence rows complete | 0 of 50 (`docs/release-evidence.md`) |
| Published list | 1 disagreement(s) with the rows |

Releases supported by this candidate, each with a complete evidence row: none.

### Package as packed

| Item | Value |
| --- | --- |
| Tarball | `minecraft-physics-native-1.0.0.tgz` |
| Files | 70 |
| Public paths of package.json | `dist/index.js`, `dist/index.d.ts` |

<!-- release-notes.cjs: end -->

### Versions

The engine registers these 50 exact releases, and no others:

1.8.8, 1.9.4, 1.10.2, 1.11.2, 1.12.2, 1.13.2, 1.14.4, 1.15, 1.15.1, 1.15.2, 1.16, 1.16.1, 1.16.2, 1.16.3, 1.16.4, 1.16.5,
1.17, 1.17.1, 1.18, 1.18.1, 1.18.2, 1.19, 1.19.1, 1.19.2, 1.19.3, 1.19.4, 1.20, 1.20.1, 1.20.2, 1.20.3, 1.20.4, 1.20.5, 1.20.6,
1.21, 1.21.1, 1.21.2, 1.21.3, 1.21.4, 1.21.5, 1.21.6, 1.21.7, 1.21.8, 1.21.9, 1.21.10, 1.21.11, 26.1, 26.1.1, 26.1.2, 26.2, 26.3

Registered means the release runs. `docs/release-status.md` gives each release's standing, with its source, data and pins.

| Standing today | Releases |
| --- | --- |
| Admitted | 0 |
| Source exact | 50 |
| Pins on the current engine | 0 of 23 on record |

Six legacy releases, 1.8.8 to 1.13.2, come from legacy-record data. The other 44 come from the pinned source manifest.

### Scope

| Part | State |
| --- | --- |
| Local player, object API, packed `NativePlayer`, adapter | available, no release admitted |
| Boats, ridden mobs, mob travel, moving entities | available, no release admitted |
| Projectiles, default and experimental minecarts | available, no release admitted. Rail and subtype minecarts are open (B189) |
| Entities the server drives | interpolation helpers only: `docs/synchronization.md` |

Known exclusions are in `docs/limitations.md`.

### Package

| Item | Value |
| --- | --- |
| Node | 24 or later (`engines`) |
| Dependencies | `@emnapi/core` and `@emnapi/runtime` 2.0.0-alpha.6, `@napi-rs/wasm-runtime` 1.2.5 |
| Optional peers | `mineflayer@^4.37.0`, `@nxg-org/mineflayer-physics-util@^1.9.10` |
| Rust | 1.95.0 (`rust-toolchain.toml`) |
| Main entry | `dist/index.js` |
| Other public path | `dist/native`, and the WebAssembly loaders under `native/artifacts` |
| Packed layout | version 2 |
| Checkpoint schema | 1 |

The files `npm pack` lists, and the public paths of `package.json` it holds, are in "Package as packed" above, generated from
this tree by `scripts/release-notes.cjs`, which `--check` holds to the tarball. A tarball from `scripts/release-build.cjs`
also holds `provenance.<target>.json` and the targets it was built for. Each tarball is one host's distribution, and there is
no combined one.

| Target | Standing |
| --- | --- |
| `x86_64-pc-windows-msvc` | declared |
| `x86_64-unknown-linux-gnu` (glibc) | declared |
| `wasm32-wasip1`, `wasm32-wasip1-threads` | declared |
| macOS, Linux musl, Linux arm64 | not declared, not built |

No tarball was built for these notes, and none is named here. A release must add the tarball, its provenance records and
the result of `scripts/consumer-check.cjs` and `scripts/wasm-check.cjs` for that build.

### Changes to know about

There is no earlier published version to compare with, so these are the behaviours a user of an earlier checkout meets.

- A registry must declare the selected release. Same-era overrides fail (`docs/registry-release-contract.md`).
- `simulateTick` and `simulateTicks` refuse, and ask for a version.
- An engine with `requireParity: true` refuses any release that is not admitted, which today is all of them.
- `NativePhysics.simulate` skips a tick past `MAX_SYNCED_CELLS` and sets `state.moveRefused`.
- A burning `ServerPlayer` with no fire draw is refused from 1.21.2 (`docs/boundary-inputs.md`).

Behaviour gates by release are in `docs/version-gates.md` and `docs/release-boundaries.md`.

### Verification

Each item is what was run, and when. None is a certification of this tree.

| Evidence | What it shows | Dated |
| --- | --- | --- |
| Last completed recording pass, all 50 releases (`docs/recorder-coverage.md`) | Recordings finished. Not current, and its replay is not recorded | 2026-11-06 |
| Replay of every recording then on hand, with no unexpected mismatch | That build only, and on several builds, not one (`todo.md`). Those recordings predate later changes | 2026-11-05 |
| 23 pins (`generated/conformance-evidence.json`) | Selected cells of 26.3, 26.1.1, 26.1.2 and 1.21.5, on engine `47114c52`. Stale | at commit `af2ca6a` |
| Determinism corpus: 600 runs, 3,000,000 ticks, 50 releases | Windows, Linux, wasm32-wasip1 and threads agree. No statement about the JVM (`docs/determinism.md`) | 2026-11-06 |
| Numeric fixtures against HotSpot (`docs/numeric-contract.md`) | Bit equality on the listed fixtures | see that page |
| The recorder's `known-failures.json` | Empty | current |

`docs/validation-records.md` explains why a matching replay shows only that no recorded scenario reached a difference.

### Blocked or open before a release

- Pins on the current engine from the pinned corpus: B288. (The corpus itself is recorded and determinism-checked: B219, B224, B235.)
- Every release through its completion gate: B288.
- Exact registry data for four aliased releases: B302.
- The full test matrix: B292.
