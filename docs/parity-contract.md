# Parity contract and release admission

A release being registered means it runs. It does not mean it matches vanilla. This file defines what
"matches" is measured against, and how each claim is made and lost.

The contract is `scripts/parity-contract.cjs`. It is code so that `test/parity-contract.test.cjs` can
hold it to the package: every interaction must name entry points the package exports, scenarios that
exist, and a start release that is registered.

## Interactions and cells

An interaction states what it accepts, what state it owns, what it reports, who has authority over it,
and what context it needs (`world`, `passenger`, `target`, `random`, `tick order`). Families are
`player`, `living`, `boat`, `mount`, `minecart`, `projectile` and `environment`.

A cell is one release of one interaction. Its status is computed by `cell()`:

| Status | Meaning |
| --- | --- |
| `unavailable` | The release is older than `since`. The feature is absent, not approximated. |
| `implemented` | An entry point serves it, and no current trace is pinned. |
| `partial` | A declared gap, or a pinned trace that replayed part of the scenarios or did not all match. |
| `verified` | A current pin replayed every scenario of the cell, and all of them matched. |

`since` is checked against minecraft-data for the interactions tied to one entity or item. Experimental
appearances before a release ships the feature (camel from 1.19.3, wind charge from 1.20.3) are listed in
the test. The experiment in `minecart.experimental` also needs the world to enable
`minecart_improvements`: the release alone does not select it.

## Pins

The recorder package's `scripts/pin-conformance.cjs` (@minecraft-physics-native/recorder) runs the replay of a cell and
records the result in this repository's `generated/conformance-evidence.json`:

```
node scripts/pin-conformance.cjs --version 26.3 --interaction player.ground --recordings <dir>   # in the recorder package
```

A pin holds the contract's scope hash, the hash of the release's source record, the tables hash, a digest
of `native/src` without its test items, the addon's hash, the commit, the hash of the recordings' index, the
scenario set, and the counts. A pin stops counting when any of these changes:

- the interaction's definition (scope), which is how a change of scope invalidates a completion claim;
- the release's source record or tables;
- the engine source. Any native change drops every cell back to `implemented` or `partial` until it is
  replayed. That is intended: a trace speaks for the code it ran.

A `partial` result is what pinning a scenario subset gives. `selected` must equal `corpus`, the
scenarios of the cell in the recorded index, for a cell to verify.

The seal stops an accidental or casual hand edit. It is a plain hash with no secret, so it does not stop
someone who recomputes it. The recordings are not in the repository, so a pin cannot be re-derived from a
checkout: re-running `pin-conformance.cjs` against the recordings is the check.

## Admission

`scripts/release-admission.cjs` reports four independent dimensions:

| Dimension | Passing | Comes from |
| --- | --- | --- |
| `source` | `exact`, or `alias` for a proved alias | `scripts/source-admission.cjs`, `docs/source-records.md` |
| `data` | `complete` | the joined tables against the running client's registry |
| `implementation` | `complete` | every required interaction has its entry points and no declared gap |
| `conformance` | `verified` | every required cell of the contract is `verified` |

`conformance` is `unverified` with no current pin, `partial` with some, and `verified` only when no
required cell is short. Each dimension fails alone, with its reasons, so a stale table does not read as a
failed trace. A release with nothing pinned stays usable: running a registered release is the development
workflow, and it carries no parity claim.

`admitted()` needs all four. No flag sets it.

### At construction

`src/release-status.json` is the status the package ships, written by
`node scripts/release-admission.cjs --write`. `--check` and `test/release-admission.test.cjs` fail when it
differs from what the evidence supports.

`releaseStatus(version)` returns it. `requireParity: true` on any engine refuses a release that is not
admitted, before the binding loads, and names the dimensions that keep it out:

```js
createNativePhysics(registry, { requireParity: true });
// Error: 1.12.2 is not admitted for parity: source blocked (...), conformance unverified (...)
```

Conformance ships only as `admitted`, because pins age with every native change and the partial detail
would otherwise make every native commit rewrite that file.

## Family status

What the contract can say today, per family. `generated/conformance-evidence.json` holds the pins behind
it, `node scripts/release-admission.cjs <version>` gives a release's detail, and `docs/release-status.md` lists
every release.

| Family | Status | Why |
| --- | --- | --- |
| player | incomplete | Recorded on all 50 in the 2026-11-06 pass, which predates later changes. No current pin. |
| living | incomplete | `physrec/me_mob*` recorded; no current pin. A `mob` kind with no override travels as `living`; `unknown` is only a name outside the game, and the dragon and the shulker are refused (`docs/mob-travel.md`). |
| boat | incomplete | Recorded; no current pin. |
| mount | incomplete | As boat. |
| minecart | incomplete | Recorded; the experiment is separate work (B027). |
| projectile | incomplete | Recorded; no pin against this build. |
| environment | incomplete | Recorded; no pin against this build. |

No family has earned its status from a registered profile or from passing unit tests: only a pin can
raise a cell above `implemented`.

### On record

Pinned with `--phases` where the replay has it, on engine `47114c52` at commit `af2ca6a`. Any native change makes a pin stale,
as it should, until pinned again. All of these are stale at this writing, and `docs/release-status.md` has the count.

| Release | Cells pinned then | Not pinned |
| --- | --- | --- |
| 26.3 | `player.hazards` 30, `player.climb` 50, `player.fall` 161, `player.piston` 55, `boat.control` 24, `mount.camel` 5, `mount.horse` 16, `mount.strider` 4, `mount.pig` 3, `living.mob` 49, `projectile.trident` 4, `projectile.thrown` 15, `environment.tnt` 7, `environment.falling` 8, `environment.item` 14, `environment.orb` 9, `projectile.arrow` 23 | 13 other cells unpinned |
| 26.1.1, 26.1.2 | `player.hazards` 30, `player.climb` 50 | all other cells unpinned |
| 1.21.5 | `player.hazards` 30, `player.fall` 161 | all other cells unpinned |

Seventeen of 26.3's 30 cells was no parity claim about 26.3, and a verified cell is not a tested branch:
`docs/validation-records.md` moves gates inside these cells that their scenarios do not notice.

## Legacy and 1.14.4 scope

The seven oldest registrations are held to the same contract, with these differences. None of them is an
exemption. Every cell below stays short of `verified` until a trace of that exact release is pinned.

| Release | World data | Source | Not available | Evidence missing |
| --- | --- | --- | --- | --- |
| 1.8.8 | numeric id and metadata | alias of 1.8.9 (`release-aliases.json`, scope in the record) | elytra, riptide, trident, strider, camel, wind charge, experiment | a current replay |
| 1.9.4 | numeric | blocked: tree pinned by hash, not reproducible | riptide, trident, strider, camel, wind charge, experiment | a current replay, a reproducible source |
| 1.10.2 | numeric | blocked | as 1.9.4 | as 1.9.4 |
| 1.11.2 | numeric | blocked | as 1.9.4 | as 1.9.4 |
| 1.12.2 | numeric | blocked | as 1.9.4 | as 1.9.4 |
| 1.13.2 | flattened block states; first fluid tags | blocked | strider, camel, wind charge, experiment | as 1.9.4 |
| 1.14.4 | flattened | exact: commit `16d77f2e` | strider, camel, wind charge, experiment | a current replay |

Legacy behaviour is not modern data under an old version string:

- `generated/legacy-release-data.json` marks `flattened` per release, and `test/source-admission.test.cjs`
  holds 1.8.8 to 1.12.2 to numeric metadata with no tags, and 1.13.2 to flattened states with tags.
- `test/legacy-behaviour.test.cjs` runs numeric-metadata worlds from each release's own minecraft-data
  through `NativeWorldSync` and the release's own fluid path, and fails if a release is read as flattened.

The scenarios compile for these releases, and the 2026-11-06 pass recorded them (`docs/recorder-coverage.md`). What is missing is
a clean pass on the current mod, and its replay.
