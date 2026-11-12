# Finding ledger

The 32 findings of the 2026-11-04 physics and data review, each kept separate. A finding is **fixed** when its
ticket closed with its regression evidence. That does not extend to behaviour the ticket did not scope, and it
certifies no release: `docs/parity-contract.md` does that.

`audit/physics-version-review-20261104/` is the review's own output. It was never committed and is no longer on
this machine, so titles and scopes come from the backlog's `FINDINGS-INDEX.md`, which quotes it. Line-level
before and after counts from that audit cannot be reproduced here.

`test/finding-ledger.test.cjs` keeps every finding listed, once, and every named test on disk.

## Physics and state: M01 to M25

| Finding | Ticket | Audited title | Scope | State | Regression evidence |
| --- | --- | --- | --- | --- | --- |
| M01 | B180 | Slowdown state disappears between mount ticks | 1.9.4-26.3 | fixed | `a1e3f84`, `test/mineflayer.test.cjs` |
| M02 | B181 | Explicit zero attributes become normal defaults | Locally simulated mount releases; reproduced 1.20.4,1.21.9,26.3 | fixed | `d6c781f`, `test/mineflayer.test.cjs` |
| M03 | B182 | Effective collision dimensions are ignored | API all mount releases; vanilla SCALE feature 1.20.5+ | fixed | `585a2ee`, `test/mineflayer.test.cjs` |
| M04 | B183 | Movement effects and modern attributes are absent | Levitation1.9.4+; Slow Falling1.13.2+; modern attributes at documented releases | fixed | `b62e9c7`, `test/mineflayer.test.cjs` |
| M05 | B190 | Tridents do not remain embedded after a block hit | 1.13.2-26.3 | fixed | `bbdc88e`, `test/mineflayer.test.cjs` |
| M06 | B191 | Generic arrow/trident names use thrown-item integration | Recognized families throughout supported versions; air difference especially1.21.2+ | fixed | `00533d8`, `test/mineflayer.test.cjs` |
| M07 | B192 | World-border hits are omitted | 1.21.2-26.3 | fixed | `311c593`, `native/src/projectile.rs` |
| M08 | B193 | Batch simulation freezes the initial water flag | Supported projectile families consuming inWater | fixed | `47dbec2`, `test/mineflayer.test.cjs` |
| M09 | B194 | Arrow preflight containment test is missing | Confirmed1.14.4-26.3; earlier trigger differs | fixed | `110ead3`, `native/src/projectile.rs` |
| M10 | B100 | Dead and sleeping players still walk and jump | Common path across50 registered releases; representative source checkpoints | fixed | `9ba5f7b`, `native/src/simulation_tests.rs`, `test/mineflayer.test.cjs` |
| M11 | B101 | Rain-only Riptide launches are refused | 1.13.2-26.3 | fixed | `07d16b1`, `test/mineflayer.test.cjs` |
| M12 | B173 | Raw firework impulse reaches gravity selection too late | Riptide/Slow-Falling-era probes1.13.2-26.3 | fixed | `77ce8c3`, `native/src/simulation_tests.rs` |
| M13 | B133 | Flow blocking is inferred from collision boxes | Flattened versions; sampled1.13.2-1.21.11 and26.3 source | fixed | `929b2a0`, `test/mineflayer.test.cjs` |
| M14 | B134 | Contracted legacy lava bounds are not reordered | 1.9.4-1.13.2 elytra-sized player; legacy helper more generally | fixed | `6d2bd2c`, `native/src/simulation_tests.rs` |
| M15 | B135 | Native eye-fluid height misses the new camera surface | 26.3 | fixed | `a020535`, `native/src/simulation_tests.rs` |
| M16 | B203 | Requested native version and TypeScript gates can disagree | Any explicit version override paired with a different registry | fixed | `c18514d`, `test/mineflayer.test.cjs`, `test/registry-release.test.cjs` |
| M17 | B122 | Compact states lose Speed and Slowness | Probed1.8.8,1.20.4,1.21.11 | fixed | `e7c1d0a`, `test/mineflayer.test.cjs` |
| M18 | B204 | Passenger state does not reach native predicates | Probed1.20.4,1.21.11; shared bridge | fixed | `a76913e`, `native/src/simulation_tests.rs`, `test/mineflayer.test.cjs` |
| M19 | B150 | Traversal allocates the entire path before its16-step cap | 1.21.9-26.3 | fixed | `5a71d85`, `native/src/simulation_tests.rs` |
| M20 | B165 | Partial-shape filtering uses the wrong overlap tolerance | Modern collision paths; checked1.18-26.3, related1.14.4-1.17.1 filter | fixed | `edf236e`, `native/src/simulation_tests.rs` |
| M21 | B151 | Slime bounce misses canSimulateMovement eligibility | 26.3 | fixed | `9aea0cc`, `test/mineflayer.test.cjs`, `test/piston-authority.test.cjs` |
| M22 | B093 | Periodic freeze damage still runs on clients | 1.17+ support; verified current1.21.11/26.3 source | fixed | `dc4f918`, `native/src/simulation_tests.rs`, `test/native-freeze.test.cjs` |
| M23 | B094 | Modern fire reset still runs on clients | 1.21.6-26.3 | fixed | `f47cb30`, `native/src/simulation_tests.rs` |
| M24 | B095 | Ignition conflates LocalPlayer and ServerPlayer and fixes RNG | 1.21.2+ modern fire branch | fixed | `95218bb`, `native/src/simulation_tests.rs`, `test/native-fire-draws.test.cjs` |
| M25 | B096 | Generic fire-immunity reset default remains1 instead of0 | 1.21.6-26.3; exact local1.21.5/1.21.6 comparison | fixed | `f47cb30`, `native/src/simulation_tests.rs` |

Every one of the 25 is fixed in its own ticket. Open tickets that cite a fixed finding (B104, B138, B140, B187,
B244 and others) are related follow-ups, and each carries its own scope.

### Traces of the findings

A fix above is what the ticket closed on, which for most of these was a unit test. This is what a vanilla trace adds: each
finding's own scenarios replayed on the final engine of `lane/fluids` on 2026-11-07, commit `9e0cf54`, addon sha256 `9325fa78e4b9e3f3`,
with each report's `replay` block naming that addon. The older tables here ran on a pre-fix engine and are gone.

Recordings of 1.19.4 and later come from the final pass. Those of 1.9.4, 1.13.2 and 1.14.4 were made for this table with
`record.ts --shards 1 --filter`, as no current recording of them exists. Before means a release the finding does not reach.

What each mode compares: `stepwise --phases` the native tick's player fields, carts, vehicles and vanilla's phases; `--projectiles` each
projectile's path and hits; `--adapter` the adapter's decisions.

The last column says whether a recording reaches the finding's branch. `scripts/gate-mutants.cjs` flips a gate on one release, and a
recording that then stops matching is one that reaches that gate and sees its result. "Seen" names the gate and the case. "Reached
only" means the gate ran on those recordings and they match either way, so the scenario does not show the finding: its evidence
stays the unit test above. "Not shown" means the fix is not one gate that can be flipped, so nothing here shows a recording reaching it.

| Finding | Release | Scenarios | Mode | Matching | Reaches the branch |
| --- | --- | --- | --- | --- | --- |
| M01 | 26.3 | `v_horse*`, `v_camel*`, `v_strider*` | phases | 24 of 24 | Not shown |
| M02 | 26.3 | `v_pig*`, `v_donkey*` | phases | 4 of 4 | Not shown |
| M03 | 26.3, 1.20.5 | `v_scaled_*` | phases | 4 of 4 on each | Yes by construction: each case sets the attribute, and `horse_jump`'s 1.20.5 gate is seen by `v_scaled_horse_jump` |
| M03, before | 1.20.4 | `v_horse_*` | phases | 15 of 15 | No scale to carry |
| M04 | 26.3, 1.13.2 | `v_effect_*` | phases | 3 of 3 on each | Not shown. Each case holds the effect on the mount |
| M05 | 26.3 | `pj_trident*` | projectiles | 6 of 6 | Not shown |
| M06 | 26.3 | `pj_arrow*`, `pj_bow*`, `pj_pierce*` | projectiles | 39 of 39 | Not shown |
| M07 | 26.3 | `pj_*_border_*` | projectiles | 8 of 8 | Seen on 1.21.2 below |
| M07 | 1.21.2 | `pj_*_border_*` | projectiles | 8 of 8 | Seen: `arrow_flight`, `tick_throwable`, `tick_hurting_projectile`, `clamp_to_world_border`, by `pj_*_border_*` |
| M07, before | 1.21.1 | `pj_*_border_*` | projectiles | 8 of 8 | Seen: the same three flights moved down a release |
| M08 | 26.3 | `pj_snowball*`, `pj_egg*` | projectiles | 32 of 32 | Not shown |
| M09 | 26.3 | `pj_arrow_*` | projectiles | 34 of 34 | Not shown |
| M09, trigger differs | 1.13.2 | `pj_arrow_still_in_stone` | projectiles | 1 of 1 | Not shown |
| M10 | 26.3, 1.21.5 | `ps_*` | phases | 9 of 9 on each | Not shown |
| M11 | 26.3 | `q_riptide*`, `q_rain*` | phases | 10 of 10 | Not shown |
| M12 | 26.3 | `pj_firework*` | projectiles | 7 of 7 | Not shown |
| M13 | 26.3 | `b_flow*`, `m_flow*` | phases | 15 of 15 | Not shown |
| M14 | 1.13.2, 1.9.4 | `f_elytra_into_lava_*` | phases | 2 of 2 on each | Yes: in `f_elytra_into_lava_45` 11 ticks have a box 0.6 tall in lava, whose shrunk box needs the reorder |
| M14, after | 1.14.4 | `f_elytra_into_lava_*` | phases | 2 of 2 | Lava is measured another way from 1.14 |
| M15 | 26.3 | `wh_*` | phases | 33 of 33 | Reached only: `eyes_in_fluid_in_area` 26.3 |
| M15, before | 26.2 | `wh_*` | phases | 33 of 33 | Reached only |
| M16 | 26.1.2 on 26.1's registry | `v_horse*` | adapter | 15 of 15 | Not shown |
| M16 | 1.21.2 on 1.21.3's registry | `fd_*` | adapter | 41 of 41 | Not shown |
| M17 | 26.3 | `e_speed*`, `e_slowness*` | phases | 25 of 25 | Not shown |
| M18 | 1.21.11 | `c_ride_*` | adapter, phases | 75 of 75, 72 of 75 with 3 server-driven and not compared | Not shown |
| M18 | 1.20.4 | `c_ride_*` | adapter, phases | 45 of 45, 43 of 45 with 2 server-driven and not compared | Not shown |
| M19 | 1.21.9 | `b_cobweb*`, `b_berry*` | phases | 8 of 8 | Reached only: `blocks_visited_where` 1.21.9. No move here needs the budget |
| M19, before | 1.21.8 | `b_cobweb*`, `b_berry*` | phases | 8 of 8 | Reached only |
| M20 | 26.3 | `rg_*` | phases | 112 of 112 | Not shown |
| M21 | 26.3 | `ph_slime*` | phases | 17 of 17 | Reached only: `can_simulate_movement` 26.3 |
| M21, before | 26.2 | `ph_slime*` | phases | 17 of 17 | Reached only |
| M22 | 26.3 | `hz_freeze*` | phases | 1 of 1 | Not shown |
| M23 | 1.21.6 | `hz_burn*`, `hz_campfire*` | phases | 10 of 10 | Reached only: `apply_inside_fire_effects` 1.21.6 |
| M23, before | 1.21.5 | `hz_burn*`, `hz_campfire*` | phases | 10 of 10 | Reached only |
| M24 | 1.21.2 | `hz_*` | phases | 38 of 38 | Seen: `fire_ignite` 1.21.2, by `hz_fire_in_rain` |
| M24, before | 1.21.1 | `hz_*` | phases | 38 of 38 | Seen in the sweep, by `hz_fire_walk_through` |
| M25 | 1.21.6 | `mc_*fire*` | phases | 5 of 5 | Reached only: `fire_immune_ticks` 1.21.6 |
| M25, before | 1.21.5 | `mc_*fire*` | phases | 5 of 5 | Reached only |

So a trace shows the finding's branch for M03, M07, M14 and M24. Sixteen findings are matched by scenarios nothing proves reach their branch,
and M15, M19, M21, M23 and M25 by scenarios that run the gate and do not depend on it. Those scenarios match, and show no
regression, but they do not show the bug would have been seen.

## Data and provenance: M26 to M32

| Finding | Ticket | Audited title | Scope | State | Regression evidence |
| --- | --- | --- | --- | --- | --- |
| M26 | B031 | Collection-registered block families are omitted | 26.2 and26.3 | fixed | `802268a`, `test/block-registries.test.cjs` |
| M27 | B032 | Symbolic block identifier names become incorrect resource IDs | 26.2 and26.3 | fixed | `802268a`, `test/block-registries.test.cjs` |
| M28 | B033 | Inherited state properties disappear | At least1.14.4,1.20.4,1.21.11,26.1 examples | fixed | `802268a`, `test/block-friction.test.cjs`, `test/block-registries.test.cjs` |
| M29 | B034 | Tags are incomplete or differ from release resources | Concrete1.20.4 and1.21.6-1.21.11 differences; see sidecars | fixed | `25545fb`, `test/block-tags.test.cjs` |
| M30 | B035 | Method fingerprints can hash a call-site block | 44-release source manifest | fixed | `f5a7c42`, `test/generator-paths.test.cjs`, `test/java-declarations.test.cjs` |
| M31 | B036 | LocalPlayer is absent from the source manifest | All44 nonlegacy manifest releases | fixed | `912278f`, `test/version-registry.test.cjs` |
| M32 | B037 | A snapshot branch is certified as release1.16.4 | 1.16.4 horse source audit entry | fixed | `083f3d4`, `test/release-manifest.test.cjs` |

### M29

B034 closed it. `generated/block-tags.json` keeps default and experimental memberships apart, with memberships read from
each release's jar. All 808 audit entries are dispositioned: 642 missing data, 148 configuration, 18 wrong identifier, 0
unresolved. So they were not 808 independent defects. The before and after discrepancy counts of the audit itself cannot be
reproduced, as its directory is gone: the disposition stands for them.

### Focused regressions

| Case | Test |
| --- | --- |
| Carrots, potatoes, wheat and melon resource ids | `test/block-registries.test.cjs` |
| Helper and collection families | `test/block-registries.test.cjs` |
| Positive and negative tag membership | `native/src/versioned/mod.rs` tests, `test/block-registries.test.cjs` |
| Overload and call-site fingerprints | `test/version-registry.test.cjs`, `test/java-declarations.test.cjs` |
| `LocalPlayer` path | `test/version-registry.test.cjs` |
| Wrong embedded identity | `test/source-admission.test.cjs`, `test/release-manifest.test.cjs` |

Missing or wrong-release evidence cannot reach a completed status: `docs/source-records.md` and
`scripts/source-admission.cjs` fail it, and admission needs that dimension to pass.
