# Release boundaries

Adjacent-release checks for the boundaries where late 1.21 and 26.x behaviour changes. Each row has three separate
questions: what the source says, whether a replay of both sides matched, and whether any check would have failed had
the gate been wrong. The third is not implied by the second: `docs/validation-records.md` records a gate moved a whole
release that 41 matching replays did not notice.

Replays are subsets of the corpus, run on 2026-11-06 with `node replay/cli.ts --version <v> --case <cases>` in the recorder package
against the recordings described in `docs/recorder-coverage.md`. They are boundary checks, not certification: only
`generated/conformance-evidence.json` is, and these counts carry no pin.

Status words: **matched** means both sides replayed with no divergence, **sensitive** means a check is known to fail on
the wrong behaviour, **unverified** means nothing has shown that.

## Boundaries

Line numbers are of `E:/pr/mc-decompiled/<release>`, as in the engine's own comments, unless a commit is named. The
pinned commits of `scripts/source-pins.cjs` are formatted differently, so their lines differ. 1.21.4 is the yarn-named
tree, so its `LivingEntity` is at `net/minecraft/entity/`.

### 1.21.4 to 1.21.5

| Change | Source | Fixture | Status |
| --- | --- | --- | --- |
| Fall damage: ceil to floor | 1.21.4 `LivingEntity.java:1657-1664`, 1.21.5 `:1672-1682` | `test/release-boundaries.test.cjs`, `test/fall-damage-sides.test.cjs`, `test/landing-policy.test.cjs` | matched, sensitive |
| Fall damage: float to double with `1.0E-6` | as above | `test/release-boundaries.test.cjs`, at a distance the two forms round differently | sensitive by unit; **the replays do not reach it** |
| `fallDistance` becomes a double | 1.21.5 `Entity.java:1286` | `accumulated_fall_distance`, 158 of 161 replays fail when moved | matched, sensitive |
| A climbable block stops a glide | `LivingEntity.travelFallFlying`, 1.21.5 | `simulation_tests.rs`, `physrec/fc_*` | matched |

Replayed: `physrec/fd_*`, `physrec/fc_*`, `physrec/f_elytra*`, 109 of 109 on each side.

### 1.21.5 to 1.21.6

| Change | Source | Fixture | Status |
| --- | --- | --- | --- |
| Post-move fire reset is the server's alone | 1.21.6 `Entity.java:851`, 26.3 `:1028-1030` | `test/authority-boundaries.test.cjs` | matched, sensitive by unit: the 30 `hz_*` replays do not notice it moved |
| Generic fire-immunity reset default 0 | 1.21.5 `Entity.java:212`, 1.21.6 `:233` | `native/src/simulation_tests.rs` (B096) | matched |

Replayed: `physrec/hz_*`, 30 of 30 on each side.

### 1.21.8 to 1.21.9

| Change | Source | Fixture | Status |
| --- | --- | --- | --- |
| A recorded move splits by the requested axes, not the collided ones | 1.21.8 `Entity.java:702`, 1.21.9 `:715` | `the_recorded_move_splits_by_the_requested_axes_from_1_21_9` | sensitive by unit: 288 replays do not notice it moved |
| Inside-block traversal: the box left comes first, capped | `BlockGetter.forEachBlockIntersectedBetween` from 1.21.9 | `block_traversal.rs`, `moves_visit_the_box_they_leave_first_in_their_direction_from_1_21_9`, `the_older_walk_keeps_its_own_limit_on_the_other_side_of_1_21_9` | matched, sensitive by unit |
| A cell that failed may be visited again | 1.21.8 `Entity.java:1161`, 1.21.9 `:1205` | `a_cell_reached_again_is_visited_once_unless_it_failed_from_1_21_9` | matched, sensitive by unit |

Replayed: `physrec/hz_*`, `physrec/b_cobweb*`, `physrec/b_berry*`, `physrec/b_powder*`, 46 of 46 on each side.

### 1.21.10 to 1.21.11

| Change | Source | Fixture | Status |
| --- | --- | --- | --- |
| `Mth.sin` indexes through a double and a long | 1.21.10 `Mth.java:51-56` (float, commit `bd23da11`), 1.21.11 `Mth.java:50-55` (double) | `native/src/math.rs` tests | matched, sensitive by unit |
| Ridden horses and camels float in deep water | `native/src/ridden.rs:1486` | `a_ridden_horse_or_camel_floats_in_deep_water_from_1_21_11`, 22 of 44 replays fail when moved | matched, sensitive |
| A motion guard in `setDeltaMovement` | 1.21.11 `Entity.java:3664-3674` | `native/src/simulation.rs` | matched |

Replayed: `physrec/v_horse*`, `physrec/v_camel*`, `physrec/j_strafe*`, `physrec/j_sprint*`, 44 of 44 on each side.

### Earlier gates, replayed for B003

| Boundary | Replayed | Result |
| --- | --- | --- |
| 1.20.4 to 1.20.5, falling fluid checks the gravity argument | `physrec/wa_*`, `lf_*`, `mf_*`, `jo_*` | 93 of 93 on each side |
| 1.21.1 to 1.21.2, travel split | `physrec/wa_*`, `physrec/fj_*` | 64 of 64 on each side |
| 1.21.1 to 1.21.2, a jump keeps a faster rise | `test/release-boundaries.test.cjs`: 104 `j_*` replays do not notice it moved | sensitive by unit |
| 1.20.5 to 1.20.6, unchanged | `physrec/fj_*`, `physrec/gr_*` | 75 of 75 on each side |

Those patterns differ between 1.20.4 and later on purpose: `fj_*` and `gr_*` barely exist before the gravity attribute,
and selecting them gave 1 scenario on 1.20.4.

## 26.x

| Release | Source | Data | Replay of the selection below |
| --- | --- | --- | --- |
| 26.1 | exact: commit `25049467` | complete | 180 of 180 |
| 26.1.1 | exact: commit `2322018b` | complete | 180 of 180 |
| 26.1.2 | exact: client jar `4e618f09`, vineflower | complete | 180 of 180 |
| 26.2 | exact: `mc-source` tree, client `2dc72797` | complete | 180 of 180 |
| 26.3 | exact: `mc-source` tree, client `e877b6a0` | complete | 180 of 180 |

Selection: `physrec/wa_*`, `wh_*`, `pm_*`, `ph_*`, `fj_*`, `v_boat*`, `v_camel*`.

`data` is the admission dimension: each release's tables hold what its own running client's registry dump holds.

26.1.1 and 26.1.2 replay on their own registries, exact releases of minecraft-data that read 26.1's tables by path
(`generated/data-aliases.json`, `docs/registry-release-contract.md`). The proof is B302's: the vanilla data generator of all
three server jars gives the same 8594 files, byte for byte, their `en_us.json` is the same, and each differing client class
has a reason read from a bytecode diff. Their data versions stay different (4786, 4788, 4790), and shared tables cover
registry data, never physics. 26.1.1 differs from 26.1 in 3 of 10682 classes: the version constants and one
branch of the social screen. 26.1.2 differs in 9: those, a checkbox tooltip on the report screen, and `handleAttack`
refusing a spectator's attack, which is a server packet handler and no movement or registry.

All counts below and above are with `--phases`.

### Boundary fixtures

| Change | Release | Source | Fixture |
| --- | --- | --- | --- |
| Fluid interaction redesign, passenger query | 26.1 | 26.3 `EntityFluidInteraction.java:57, 91` | `test/passenger-fluids.test.cjs` over all five releases |
| Eye fluid takes `getHeightForCamera` | 26.3 | 26.3 `FluidState.java` | `the_eyes_take_the_camera_height_of_a_source_under_a_sturdy_block_from_26_3` and the four after it |
| Slime bounce needs `canSimulateMovement` | 26.3 | 26.3 `Entity.java:839` | `test/piston-authority.test.cjs`, which fails when moved; 55 replays do not |
| Vehicle restitution | 26.2 | 26.3 `Entity.java:867-872` | `native/src/simulation_tests.rs`; 28 of 98 replays fail when moved |

No further 26.x release is added: a new one needs a request and its own source.

## Not covered

- Only these selections were replayed, not each release's corpus.
- "Sensitive" means moving the gate failed a replay; "sensitive by unit" means only a unit test tells the sides apart, and
  where the row says so, the replays were run with the gate moved and did not notice.
- Intermediate phases are compared in `PHASE_FIELDS` only (`docs/tick-order.md`).
