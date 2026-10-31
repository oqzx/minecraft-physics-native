# minecraft-physics-native: vanilla parity audit

This report checks the Rust engine `minecraft-physics-native` (branch `mc-physics-demo`) and its TypeScript Mineflayer adapter (`src/mineflayer/*`) against the decompiled vanilla client (`D:/projects/mc-zuri/mc-data/extracted_minecraft_data`, branches `origin/client<ver>`). It also checks whether the `[x]` claims in `todo.md` hold.

Every deviation was checked by an adversarial verifier. When a verifier corrected the description, the version range or the severity, this report uses the corrected version. Paths are relative to the repo root (`native/src/...`, `src/mineflayer/...`). Java paths are given as `origin/client<ver>:<path>`, with `client/net/minecraft/` usually left out.

## 1. Summary

**Verdict: the engine is not 1:1 with the vanilla client.** The core travel math (input vectors, friction, air/water/lava drag, jump power, elytra glide, the step-up algorithms, per-version Mth tables) matches vanilla closely; section 5 lists what was checked. Outside that core, many paths deviate:
- version gates (one modern algorithm applied to older releases)
- tick ordering around fall distance
- world queries that ignore entity and context state
- adapter-side decisions (sprint, crouch, pose, eye checks)
- inputs the adapter never sends (world border, ultra-warm, no-gravity, Weaving, fall-damage-resetting tags)
- pistons
- vehicles and ridden mobs
- every projectile family

**Missing sources.** The reference repo has no vanilla sources for 1.9.4-1.12.2. Findings for those releases are inferred from 1.8.9 and 1.13.2, or from the yarn-decompiled 1.9.4/1.10.2/1.11.2/1.12.2 jars where an auditor used them. 1.15.x, 1.16.0-1.16.3, 1.17.x, 1.18.2 and 1.19.2 also have no source branch; some auditors used `javap` on mojmap jars or `generated/physics-methods.json` for them.

### Counts (160 merged deviations, refuted claims excluded)

| Severity | Count | | Status | Count |
|---|---|---|---|---|
| major | 40 | | confirmed | 107 |
| minor | 107 | | partially-confirmed | 53 |
| cosmetic | 13 | | disputed (section 3) | 2 |
| | | | refuted (appendix) | 8 |

"Minor" covers some items that are wrong but only show in output-only fields (for example `fall_damage`), in N-API helpers the Mineflayer path does not use, or in rare geometry. Each entry states its scope.

`todo.md`: auditors gave a verdict to 654 items. 414 of them are checked `[x]`, and **162 of those 414 do not hold** (50 deviate, 86 are only partly true, 2 are not implemented, 24 are false non-physics claims). See section 4.

### Top issues

1. **D-001**: from 1.21.5, the landing tick's drop is left out of fall damage, and fall distance keeps growing in water (1.21.5-26.3).
2. **D-002**: lava never halves fall distance (all versions).
3. **D-012**: sneak edge back-off uses the 1.21.5 algorithm for every release from 1.14. Sneak-jumps leave the edge in 1.14-1.18.
4. **D-013**: no-collision checks count boxes that do not touch the query box (sneak back-off, fluid hop), in all versions.
5. **D-018**: block speed factor ignores the water/bubble-column exception, so soul sand or honey under water slows the player (1.16-26.3).
6. **D-019**: a flying player is still clamped by ladders and vines and is not pushable (1.21.4-26.3).
7. **D-027 / D-111**: the adapter never sends ultraWarm, so Nether lava currents are 3x too weak for the player, boats and ridden mobs.
8. **D-039**: the adapter crouches a flying player, so flight while holding sneak is slowed to 0.3x (1.15-26.3).
9. **D-063**: before 1.20.5, effects with amplifier 128-255 (negative bytes, e.g. "Levitation 255" hover) are dropped.
10. **D-070**: the adapter never sends the world border, so border collision never happens on custom-border servers.
11. **D-088**: team collisionRule is ignored, so entities push the player on `never` teams (1.9-26.3).
12. **D-099 / D-100**: moving pistons and pushed blocks have no collision and push nothing.
13. **D-112**: ridden horses, camels and llamas climb ladders and vines (vanilla AbstractHorse never climbs).
14. **D-133**: every projectile family uses one generic version-less tick (1.21.2+ order everywhere).
15. **D-028 / D-040**: the adapter keeps the 0.111 eye offset in 1.20.5-1.20.6, and from 1.17 refreshes the pose one tick early.

---

## 2. Deviations by area

Each area is ordered by severity, then by impact.

### 2.1 Fall distance and fall damage

#### D-001 · From 1.21.5, checkFallDamage order: landing-tick drop not counted, accumulation in water
- **Versions:** 1.21.5-26.3 · **Severity:** major · **Status:** confirmed (multiple auditors)
- **Vanilla:** `if (!isInWater() && y < 0) fallDistance -= (double)(float)y;` runs first; then `if (onGround) { fallOn(fallDistance); resetFallDistance(); }`. LivingEntity.checkFallDamage refreshes the water state before this. (`origin/client1.21.5:world/entity/Entity.java:1321-1335`, `origin/client26.3 Entity.java:1596-1610`; 1.21.4 uses the old `if onGround … else if y<0` form, `Entity.java:1287-1299`)
- **Native:** the pre-1.21.5 if/else is used for every release. Landing damage uses `fall_distance` before this tick's drop is added. Accumulation `fall_distance += -displacement.y` has no `in_water` check and no f32 cast. (`native/src/simulation.rs:2592-2628`)
- **Impact:** `fall_damage` is too low by the landing tick's drop (up to ~3.9 blocks, often 1 HP, e.g. a 5-block drop gives 1 instead of 2). `fallDistance` reads |dy| instead of 0 while sinking in water, and the extra carries into a fall out of the water.
- **todo.md:** 165 `[x]` (deviates); the test at `simulation_tests.rs:2616-2648` asserts the non-vanilla value.
- **Fix:** fixed — From 1.21.5 the move's drop is counted first, only when not in water, as `fd - (double)(float)dy`, after the water refresh and before the landing, so landing damage includes it; 1.21.4 and older keep the onGround/else-if form.

#### D-002 · Lava never halves fallDistance (Entity.baseTick)
- **Versions:** all (1.8.8-26.3; 1.9-1.12 bracketed) · **Severity:** major · **Status:** confirmed
- **Vanilla:** every tick in lava, `fallDistance *= 0.5F` (`*= 0.5` once the field is a double), before aiStep. The client runs this too. (`origin/client1.8.9:entity/Entity.java:315-318`, `origin/client1.14.4:world/entity/Entity.java:400-402`, `origin/client1.21.11 Entity.java:558-559`, `origin/client26.3 Entity.java:593-594`)
- **Native:** the tick-start lava block sets `in_lava` and pushes, but never touches `fall_distance`. The only `*0.5` is the bed landing. The ridden path (`ridden.rs:339-366`) has the same gap. (`native/src/simulation.rs:2176-2204`)
- **Impact:** fall distance and landing damage are too high after any tick in lava. Powder snow (>2.5) and the sneak `isAboveGround` test are also affected.
- **todo.md:** 406, 852 (open)
- **Fix:** fixed — In lava, `fall_distance *= 0.5` now runs at tick start on the player path (1.14-1.15 using the in_lava kept from the last move) and in the ridden push_by_fluids path, for every release.

#### D-003 · Fall-damage-resetting clip in Entity.move is wrong, and the adapter never feeds it
- **Versions:** 1.19-26.3 (very likely 1.18.2: the mojmap 1.18.2 jar references FALLDAMAGE_RESETTING; 1.18.1 has none). The 8-block cap applies from 1.21.9. Before 1.18.2 it matters only to direct N-API callers. · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** if `fallDistance != 0` and the collided move has lengthSqr ≥ 1, the client clips along the move with `ClipContext.Block.FALLDAMAGE_RESETTING` (every `#fall_damage_resetting` block counts as a **full cube**) and `Fluid.WATER`, and calls resetFallDistance on a hit. checkFallDamage then still adds this tick's dy. From 1.21.9 the clip covers only `min(length, 8.0)` and treats END_GATEWAY/END_PORTAL as full cubes for players. (`origin/client1.19 Entity.java:579-585`, `origin/client1.21.11 Entity.java:757-765`, `ClipContext.java:61-75`)
- **Native:** `src/mineflayer/blocks.ts:236-262` never sends `fallDamageResetting`, and the generated tag data is never applied, so on the Mineflayer path the reset never fires. When an N-API caller does set the flag, native still differs:
  - it ray-tests the stored collision box, so vines, cobweb and berry bushes never hit and ladders only by their thin box;
  - water is ignored;
  - after a reset it skips that tick's accumulation;
  - there is no 8-block cap and no version gate.

  (`native/src/simulation.rs:2514-2522, 2626`; `native/src/world.rs:1078-1117`)
- **Impact:** a fast fall (≥1 block per tick) through vines, ladders, scaffolding, cobweb, berry bushes or thin water keeps its fall distance and damage, where vanilla resets it.
- **todo.md:** 160 `[x]` (deviates), 163 `[x]` (partial)
- **Fix:** fixed — Gated at 1.18.2 (bracketed by 1.18.1 and 1.19); the ray test is replaced by a port of `BlockGetter.clip` with a FallDamageResetting shape (tagged blocks as full cubes, plus water), the end portal/gateway for players and the 8-block cap from 1.21.9, and no skip of the tick's accumulation; the adapter now sends `fallDamageResetting` from the generated tag data and `playerFallDamageResetting`.

#### D-004 · Fall-damage formula and rounding at the wrong releases; safe-fall-distance defaults
- **Versions:** 1.20.5, 1.20.6, 1.21-1.21.5, plus the adapter input for 1.8.8-1.20.4 and the N-API default for 1.20.5+ · **Severity:** minor (only the output field `fall_damage`, which the adapter never reads; some verifiers rated it major) · **Status:** confirmed
- **Vanilla:**
  - ≤1.20.4: `Mth.ceil((fd - 3.0F - jumpBoost) * mult)`.
  - 1.20.5-1.21.4: `Mth.ceil((double)((fd - (float)SAFE_FALL_DISTANCE) * mult) * FALL_DAMAGE_MULTIPLIER)` (float, no epsilon; 1.20.5 and 1.20.6 identical).
  - 1.21.5+: `Mth.floor((fd + 1.0E-6 - SAFE) * mult * FDM)`.
  - SAFE_FALL_DISTANCE defaults to 3.0, and Jump Boost adds +1 per level.
  - Sources: `origin/client1.20.4 LivingEntity.java:1566-1573`, `1.20.5/1.20.6 :1593-1600`, `1.21.4 :1621-1628`, `1.21.5 :1643-1654`, `1.20.5 MobEffects.java:77`.
- **Native:** `tick_with_profile` turns the modern formula on via `water_movement_efficiency_attribute` (`simulation.rs:82-96`).
  - 1.20.5 and 1.21-1.21.4 use `ceil(fd + 1e-6 - safe)` in f64. An exact integer excess (including fd == safe) gives +1.
  - 1.21.5 uses Ceil (`v-1_21_5.rs:12`); Floor starts at 1.21.6.
  - 1.20.6 takes the legacy path `ceil(fd - 3 - safe - jb)` (`v-1_20_6.rs:40,52`; `simulation.rs:64-80`), which counts safe distance and Jump Boost twice.
  - The adapter always sends `safeFallDistance ?? 3` (`player.ts:379`), so the legacy threshold before 1.20.5 is 6 blocks.
  - The N-API default is `safe_fall_distance = 0.0` (`napi_api.rs:665`).
- **Impact:** `fall_damage` from `NativePlayer.state()` / `simulateVersionedTick` is off by ≥1 point. Position and velocity are unaffected.
- **todo.md:** 131 `[x]` deviates, 134 partial, 208/210 partial, 219 deviates, 704/706 deviates, 804 deviates
- **Fix:** fixed — Landing damage now follows vanilla per release: before 1.20.5 f32 `ceil((fd - 3 - jumpBoost) * mult)` with safe_fall_distance standing for the 3.0, 1.20.5-1.21.4 `ceil((double)((float)fd - safe) * FDM)` without epsilon (1.20.6 included), and `floor((fd + 1e-6 - safe) * FDM)` from 1.21.5 (1.21.5 profile now Floor); the N-API default safe_fall_distance is 3.0.

#### D-005 · Fall distance accumulates as unrounded f64
- **Versions:** all; observable at boundaries for 1.8.8-1.21.4, negligible from 1.21.5 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** fallDistance is a float field before 1.21.5: `(float)((double)fd - y)` up to 1.18.1, `fd -= (float)y` for 1.19-1.21.4. From 1.21.5 it is a double field with `fd -= (double)(float)y`. (`origin/client1.8.9 Entity.java:715`, `1.20.4 :1130`, `1.21.5 :1323`, `26.3 :1598`)
- **Native:** `state.fall_distance += -displacement.y` in f64, with no f32 rounding. Landing math is also f64. (`native/src/simulation.rs:280, 2627, 64-80`)
- **Impact:** about 1e-7 relative drift, which can flip a damage ceil, the `fallDistance < maxUpStep` check, or the powder-snow >2.5 test at the margins.
- **todo.md:** 403 (open)
- **Fix:** fixed — New `accumulated_fall_distance()` reproduces vanilla: `(float)((double)fd - dy)` up to 1.18.x (1.18.2 taken as 1.18.1), `(float)fd - (float)dy` for 1.19-1.21.4, `fd - (double)(float)dy` from 1.21.5; pre-1.21.5 landing math is f32.

#### D-006 · Elytra fall-distance rule before 1.19.3
- **Versions:** 1.9.4-1.19.2 (1.13.2-1.19.1 verified, 1.19.2 inferred from javap and the 1.19.3 change) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** while gliding, `if (dy > -0.5) fallDistance = 1.0F` with no other condition, so smaller values are raised to 1. The `> 1.0F` clamp-down (checkSlowFallDistance) only arrives in 1.19.3. (`origin/client1.14.4 LivingEntity.java:1785-1786`, `1.19.1 :2107-2108`, `1.19.3 Entity.java:2169-2173`)
- **Native:** applies the 1.19.3+ rule `vy > -0.5 && fall_distance > 1.0` for every release (`native/src/simulation.rs:461-465, 2471-2473`).
- **Impact:** early-glide fallDistance can be up to 1.0 lower than vanilla. Landing damage can differ by 1, and so can the sneak and powder-snow checks.
- **todo.md:** 699 `[x]` (deviates)
- **Fix:** fixed — Before 1.19.3 the glide sets `fall_distance = 1.0` whenever vy > -0.5 (verified in LivingEntity.travel for 1.9.4-1.19.2); 1.19.3-1.21.1 keep the `> 1.0` clamp and 1.21.2+ use the aiStep check.

#### D-007 · Slow Falling / Levitation fall reset uses the 1.20+ rule everywhere
- **Versions:** 1.13.2-1.19.4 verified; 1.9.4-1.12.2 for Levitation (yarn shows no reset at all there) · **Severity:** minor · **Status:** confirmed
- **Vanilla:**
  - ≤1.19.4: Slow Falling resets only when `vy <= 0` at travel start; Levitation resets only in the air branch, after move.
  - 1.9-1.12: Levitation never resets fallDistance.
  - 1.20+: `aiStep` resets for either effect before travel, unconditionally.
  - Sources: `origin/client1.14.4 LivingEntity.java:1755-1763,1840-1842`, `1.19.4 :1992-1999,2103-2105`, `1.20 aiStep:2578-2579`; yarn 1.12.2 `LivingEntity.java:1533-1534`.
- **Native:** resets for either effect at tick start in every release (`native/src/simulation.rs:2134-2139`).
- **Impact:** a downward levitation tick ends at -dy in vanilla and 0 in native. A landing on the tick the effect starts takes damage only in vanilla. Elytra or lava under Levitation is reset only in native.
- **todo.md:** 701 `[x]` (deviates)
- **Fix:** fixed — The tick-start reset is now gated at 1.20; before it, Slow Falling resets at travel start only when vy <= 0 and Levitation resets after the air move from 1.13 (none in 1.9-1.12), as vanilla 1.13.2-1.19.4 travel does.

#### D-008 · From 1.21.2, fall-flying accumulation check skipped in fluid
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `aiStep → updateFallFlying → checkFallDistanceAccumulation` runs every tick the fall-flying flag is set, before travel, in any fluid (`origin/client1.21.11 LivingEntity.java:2942-2944, 3021-3022`).
- **Native:** the clamp runs only on the in-air glide path, where `gliding` requires an empty fluid (`native/src/simulation.rs:2464, 2471-2473`).
- **Impact:** a fall-flying player in lava keeps a larger fall distance.
- **todo.md:** 1022 (open)
- **Fix:** fixed — From 1.21.2 the check runs in aiStep in any fluid, before travel, on the velocity with this tick's firework boost added (then restored), matching vanilla's order where the rocket has already ticked.

#### D-009 · Per-block fallOn damage modifiers and the Player mayfly gate are missing
- **Versions:** hay 1.13.2-26.3 (absent in 1.8.9; 1.10-1.12 unverified); honey 1.15+; powder snow and pointed dripstone 1.17+ (dripstone +2.0 up to 1.21.4, +2.5 from 1.21.5); mayfly gate in all versions · **Severity:** minor (output only) · **Status:** partially-confirmed
- **Vanilla:**
  - hay and honey: `causeFallDamage(fd, 0.2F)`;
  - powder snow: sound only;
  - upward dripstone tip: `causeFallDamage(fd + 2.5, 2.0F)`;
  - `Player.causeFallDamage` returns early if `abilities.mayfly`.
  - Sources: `origin/client1.21.11 HayBlock.java:23`, `HoneyBlock.java:48-54`, `PowderSnowBlock.java:84-90`, `PointedDripstoneBlock.java:136-141`, `Player.java:1364-1390`.
- **Native:** only Slime (bounce/cushion) and Bed (×0.5, from 1.12) are block-specific; everything else uses the plain formula (`native/src/simulation.rs:2580-2625`). The adapter never sets `fallDamageImmune`.
- **Impact:** `fall_damage` is wrong (e.g. a 20-block drop onto hay gives 17 instead of 3-4). Movement is unaffected.
- **todo.md:** 703, 935, 971 (open)
- **Fix:** fixed — Hay (0.2F from 1.9), honey (0.2F from 1.15), powder snow (no damage from 1.18) and upward dripstone tips (+2.0 up to 1.21.4, +2.5 from 1.21.5, mult 2.0F) are new landing block effects, and the adapter now sends `fallDamageImmune = mayFly`, so the engine's existing immunity gate covers players that may fly.

#### D-010 · Sneak landing on slime: no damage from 1.21.2
- **Versions:** 1.21.2, 1.21.3, 1.21.5-26.1.2 (1.21.4's player skips the check in both engines; 26.2+ is correct through `cushioned`) · **Severity:** minor (output only) · **Status:** confirmed
- **Vanilla:** `if (!entity.isSuppressingBounce()) entity.causeFallDamage(fd, 0.0F)`, with no else branch (`origin/client1.21.2 SlimeBlock.java:fallOn:24`; 1.21.1 still calls super).
- **Native:** `bounced` requires `!sneaking`, so a sneaking landing computes normal damage (`native/src/simulation.rs:2582-2625`).
- **Impact:** native reports damage that vanilla never deals.
- **todo.md:** 938 (open)
- **Fix:** fixed — From 1.21.2 a slime landing that does not bounce deals no damage (vanilla `if (!isSuppressingBounce()) causeFallDamage(fd, 0.0F)` with no else); fall distance is still reset.

#### D-011 · 1.8.8: flight resets fallDistance every tick
- **Versions:** 1.8.8 (1.9-1.12 most likely correct, but there is no source to check) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** 1.8.9 `EntityPlayer.moveEntityWithHeading` (flying) only scales motionY by 0.6 and never resets fallDistance; the reset appears by 1.13.2 (`origin/client1.8.9 EntityPlayer.java:1246-1261`; `1.13.2 EntityPlayer.java:~1429-1438`).
- **Native:** sets `fall_distance = 0` after travel for every release before 1.21.2 (`native/src/simulation.rs:2783-2788`).
- **Impact:** the fall distance output is 0 while flying downward on 1.8.8.
- **todo.md:** none
- **Fix:** fixed — The after-travel flight reset of fall distance and fall_flying is now gated at 1.9 (1.8.9 PlayerEntity.travel only scales vy by 0.6; yarn 1.9.4 adds the reset).

### 2.2 Sneak edge back-off and no-collision queries

#### D-012 · Sneak edge back-off uses the 1.21.5 algorithm for every release from 1.14
- **Versions:** 1.14.4-1.18.x major; 1.19-1.20.4 minor (box shape); 1.20.5-1.21.4 epsilon only; 1.20.5-26.3 isAboveGround box epsilon only. Breakpoints pinned with javap: isAboveGround and the flying test arrive in 1.16.2, the y≤0 guard in 1.19. 1.15.x is bracketed. · **Severity:** major · **Status:** confirmed
- **Vanilla:**
  - 1.14.4-1.16.1: back off only when `onGround && isSneaking` (no flying, isAboveGround or y test), probing with the whole box moved by `(x, -maxUpStep, z)`.
  - 1.16.2-1.18.x: adds `!flying && isAboveGround()` (whole box), still no y test.
  - 1.19-1.20.4: adds `y <= 0`.
  - 1.20.5-1.21.4: `canFallAtLeast` feet slab `minY - step - 9.999999747378752E-6`, no inset, also used by isAboveGround.
  - 1.21.5+: slab inset by 1e-7.
  - maxUpStep is the float 0.6F throughout.
  - Sources: `origin/client1.14.4 Entity.java:applySneaking:600-640`; `1.16.4 Player.java:969-1027`; `1.18.1 :983,1040`; `1.19 :1056-1061`; `1.20.5 :1027-1078`; `1.21.5 :1019-1025`.
- **Native:** one path for every release from 1.14. It returns early on `flying || delta.y > 0`, uses a whole-box isAboveGround, and the 1.21.5 1e-7-inset slab `can_fall`, with step as the f64 0.6. (`native/src/simulation.rs:1429-1482`, call at :2496)
- **Impact:**
  - 1.14-1.18: a sneak-jump at an edge is not held back (vanilla clamps it on the jump tick, and while rising in 1.16-1.18).
  - 1.14-1.16.1: native backs off while airborne near the ground.
  - Before 1.20.5: a block at body height beyond the edge changes vanilla's result but not native's.
- **todo.md:** 169 `[x]` (deviates), 1005 (open; marked deviates by the auditor)
- **Fix:** fixed — back_off_from_edge from 1.14 now follows each vanilla era (1.14-1.16.1 onGround+sneaking with the whole box at (x, -step, z); 1.16.2-1.18 adds !flying and whole-box isAboveGround; 1.19-1.20.4 adds the y>0 check; 1.20.5-1.21.4 the un-inset feet slab with float fallDistance; 1.21.5+ the 1e-7 inset with doubles), with the step as the float maxUpStep and every probe through the entity-aware no_collision_for_entity.

#### D-013 · No-collision checks count boxes that do not touch the query box
- **Versions:** all (the sneak path from 1.14; the fluid hop in every version, plus the ridden-mob hop) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `BlockCollisions.computeNext` returns a shape only if it actually overlaps the query (`box.intersects` for full cubes, `joinIsNotEmpty(…, AND)` otherwise). 1.8.9 `Block.addCollisionBoxesToList` uses `intersectsWith`. (`origin/client1.21.11 BlockCollisions.java:86-104`; `origin/client1.14.4 LevelReader.java` getBlockCollisions; `origin/client1.8.9 Block.java:306-310`)
- **Native:** for "inside" cells (`floor(min) ≤ c < ceil(max)`), every box is kept without an intersection test. `no_collision`, `no_collision_for_entity` and `is_free` only check `is_empty()`. (`native/src/world.rs:1146-1180, 905-907, 1058-1075`; consumers `simulation.rs:1457-1480, 2756-2769`, `legacy_fluid.rs:131-141`, `ridden.rs:903`)
- **Impact:**
  - A sneaking player walks off ledges whose lower cell holds a carpet, snow layer, trapdoor, out-of-reach fence/pane/ladder and similar blocks.
  - The 0.3 water/lava escape hop is refused next to partial blocks the hop box does not touch.
- **todo.md:** 145 `[x]` (partial)
- **Fix:** deferred — the vanilla merger tolerance is ported (shape_meets_query, query/block order, strict Shapes.block() branch, strict pre-1.13), but 1.13.x still deflates the lowered sneak box by 1e-7 where 1.13.2 Entity.java:626-650 does a plain offset (overlaps in [1e-7, 2e-7) back off in native), the matching test comment cites a deflate that does not exist, and world.rs is_full_cube_box takes the strict branch for any whole-coordinate unit cube, not only the Shapes.block() object (1.16.5 BlockCollisionSpliterator.java:74).

#### D-014 · No-collision checks ignore entity colliders and the world border
- **Versions:** all; the border form differs per era (stone outside the border in 1.8-1.12, the border shape in 1.13-1.17, borderCollision from 1.18) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `CollisionGetter.noCollision = noBlockCollision && noEntityCollision && noBorderCollision`. In 1.8.9, getCollidingBoundingBoxes adds entity boxes. (`origin/client1.21.11 CollisionGetter.java:52-54`; `origin/client1.8.9 World.java:886-935`)
- **Native:** block shapes only. Entity colliders and the border are used only in `collide_with_world` (`native/src/world.rs:905-907, 1058-1075`; `simulation.rs:1785-1797`).
- **Impact:**
  - Sneaking on a boat or shulker over a gap: native freezes the player; vanilla lets it walk to the entity's edge.
  - The fluid hop next to a boat, and the pose fit under a boat or shulker, also differ.
- **todo.md:** none
- **Fix:** deferred — the sneak and fluid-hop queries now include world-border and entity hard boxes (matching World.java for 1.8.9-1.12.2), but 1.13.2 can_fall still deflates the lowered box by 1e-7 (vanilla Entity.java:626 has none), so blocks overlapping by [1e-7, 2e-7) and entity colliders at overlaps <= 1e-7 do not hold the sneaker; the deflate must be dropped for 1.13.x with 1.13.2 test rows added.

#### D-015 · Context-free collision queries always treat scaffolding as its stable shape
- **Versions:** 1.14.4-26.3 (the boat float fit from 1.21) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Entity.isFree` uses `level.noCollision(this, box)` with `CollisionContext.of(entity)`, so scaffolding is empty or a 0.125 slab for an entity that is not above it. (`origin/client1.21.11 Entity.java:672-678`, `ScaffoldingBlock.getCollisionShape:113-121`; `AbstractBoat.java:521`)
- **Native:** `is_free`, `collision_shapes` and the boat float fit pass no context, so the stored stable shape (legs plus top plate) is always used (`native/src/world.rs:1058-1060, 1119-1121, ~1156`; `simulation.rs:2757-2770`; `ridden.rs:903`; `vehicle.rs:456-459`).
- **Impact:** the 0.3 fluid hop (player and ridden mob) and the 1.21+ boat snap are refused next to or under scaffolding.
- **todo.md:** 145 `[x]` (partial)
- **Fix:** fixed — the player and ridden-mob fluid hop use world.is_free_for_entity and the 1.21+ boat float fit uses no_collision_for_entity, each with the entity's collision context, so scaffolding is empty or a 0.125 slab for an entity not above it (1.14.4-26.3).

#### D-016 · 1.13.2 pushOutOfBlocks uses "one full collision box" as the normal-cube test
- **Versions:** 1.13.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the open-space test is `!isNormalCube` = `material.isOpaque() && isFullCube && !canProvidePower`. So ice, glowstone, sea lantern, beacon, tnt, redstone block, observer, retracted piston and shulker box count as open, and soul sand counts as blocked. (`origin/client1.13.2 EntityPlayerSP.java:386-444`, `EntityPlayer.java:1454-1460`, `Block.java:278-280`)
- **Native:** `is_suffocating` falls back to a single 0..1 box. The adapter sends `suffocating` overrides only for name lists, and its soil list only from 1.16. (`native/src/world.rs:1026-1036`; `src/mineflayer/blocks.ts:63-64, 229, 248`; `simulation.rs:1618-1660`)
- **Impact:** a different ±0.1 X/Z push-out next to these blocks.
- **todo.md:** 674 (open)
- **Fix:** fixed — the Mineflayer adapter now sends suffocating = isNormalCube for 1.13.2 (soul sand blocked; glass, leaves, ice, glowstone, sea lantern, beacon, tnt, chorus flower, redstone block, observer, pistons and shulker boxes open, derived from each block's material and isFullCube/emitsRedstonePower overrides), and sends inWall separately so isInsideWall keeps its causesSuffocation approximation.

#### D-017 · 1.16 and 1.16.1 still use the old LocalPlayer.checkInBlock push-out
- **Versions:** 1.16, 1.16.1 · **Severity:** minor · **Status:** confirmed (javap)
- **Vanilla:** in 1.16 and 1.16.1, `checkInBlock` and `blocked()` test whether any cell from floor(minY) to ceil(maxY) is `isSuffocating`, regardless of collision shape, with `closest = 9999.0`. The deflated-column `suffocatesAt` test arrives in 1.16.2. The soils (farmland, soul sand, grass path) only get `isSuffocating` in 1.16.2. (1.16.1 `LocalPlayer.checkInBlock` bytecode at offsets 46-173; 1.16.2 `suffocatesAt` at 5-63)
- **Native:** the 1.16.2 column test from 1.16.0 (`native/src/simulation.rs:1566, 1598`), and the adapter marks the soils suffocating from 1.16.0 (`src/mineflayer/blocks.ts:229`).
- **Impact:** differences only at overlaps under 1e-7, and a push-out from soils that 1.16/1.16.1 do not do.
- **todo.md:** none
- **Fix:** fixed — through 1.16.1 move_towards_closest_space uses the per-cell isSuffocating column test (floor(minY)..ceil(maxY)) with closest = 9999.0, the deflated-column suffocatesAt with Double.MAX_VALUE from 1.16.2, and the adapter marks farmland, grass/dirt path, soul sand and mud suffocating only from 1.16.2.

### 2.3 Block surfaces and climbing

#### D-018 · Block speed factor ignores the water/bubble-column exception
- **Versions:** 1.16.4-26.3 verified (1.16.4, 1.18.1, 1.19.4, 1.20.4, 1.21.11, 26.1, 26.3). 1.15.x is likely affected but has no source (1.14.4 has no speed factor). · **Severity:** major (several verifiers rated it minor because the window is narrow) · **Status:** confirmed
- **Vanilla:** `if (!state.is(WATER) && !state.is(BUBBLE_COLUMN)) return f == 1.0 ? below.speedFactor : f; else return f;`. The block below is never read while the feet are in water or a bubble column. 1.21+ lerps by MOVEMENT_EFFICIENCY on top. (`origin/client1.16.4 Entity.java:652-660`, `1.21.11 :1026-1034`, `26.3 :1131-1139`)
- **Native:** `factor_at` maps only SoulSand/Honey to 0.4; a 1.0 at the feet always falls through to the block below (`native/src/simulation.rs:729-762`, applied at :2696-2700, :1908-1912, `ridden.rs:693,716`, `vehicle.rs:817`, `piston.rs:364`).
- **Impact:** with the feet in water or a bubble column at y∈[n+1, n+1.5) above soul sand or honey (swimming low over it, the bottom of soul-sand bubble elevators), x/z velocity is multiplied by 0.4 per move. Standing on soul sand or honey is unaffected.
- **todo.md:** 933 `[x]` (partial), 827 `[x]` (partial)
- **Fix:** fixed — `block_speed_factor` keeps the feet block's 1.0 when the feet are in the water block (new `BlockState.water_block`, sent as `waterBlock` by the adapter) or a bubble column, from 1.15 (the first release with a speed factor) through 26.3; waterlogged blocks and water plants still read the block below.

#### D-019 · Flying players still climb (Player.onClimbable is false while flying from 1.21.4)
- **Versions:** 1.21.4-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `onClimbable() { return abilities.flying ? false : super.onClimbable(); }`. While flying, handleOnClimbable does no clamp and no fall reset, and `isPushable` stays true. (`origin/client1.21.4 Player.java:2053-2055`, `26.3 :1876-1878`; absent in 1.21.3; `LivingEntity.java:2367-2382, 3029-3031`)
- **Native:** `climbable()` has no flying or version check (`native/src/simulation.rs:996-1003`). The pre-move clamp ±0.15 and y ≥ -0.15 applies to fliers (:2477-2492), and the entity-push early return skips pushes (`native/src/entity.rs:234-236`).
- **Impact:** creative flight through ladders, vines or scaffolding is capped at 0.15 b/t horizontally and -0.15 b/t down, and no entity pushes apply. The post-move 0.2 climb is masked by the vy×0.6 flight restore.
- **todo.md:** 956 `[x]` (deviates)
- **Fix:** fixed — `climbable()` takes the release and is false for a flying player from 1.21.4 (1.21.3 has no override), removing the 0.15 clamp, climb fall reset and 0.2 climb and making the flier pushable again; the adapter's `isInClimbable` follows the same rule.

#### D-020 · From 1.21.2, cobweb and berry-bush stuck multipliers come only from the final box
- **Versions:** 1.21.2-1.21.4 (only moves ≥ ~1 block per tick); 1.21.5-26.3 (any move, including the exit tick) · **Severity:** major · **Status:** partially-confirmed (powder snow is effectively unaffected because of its feet-block gate)
- **Vanilla:** `applyEffectsFromBlocks` calls `entityInside → makeStuckInBlock` for every block crossed along the tick's movements: boxTraverseBlocks in 1.21.2-1.21.4, axis-split `forEachBlockIntersectedBetween` including the start box from 1.21.5. (`origin/client1.21.11 Entity.java:859-913, 1198-1290`; `BlockGetter.java:161-195`; `WebBlock.java:25-34`)
- **Native:** `stuck_speed_multiplier_inside` only looks at the deflated final box. The swept `blocks_visited` list is used only for bubble columns, soul sand and honey. (`native/src/simulation.rs:707-727, 857-891, 2791-2807`)
- **Impact:** a fast fall through a cobweb or bush, and the tick of leaving one, give vanilla a stuck move (0.25/0.05/0.25) and a fall reset that native does not.
- **todo.md:** 957/959 `[x]` (partial)
- **Fix:** deferred — the powder-snow falling-shape test from 1.21.5, the pre/post-1.21.9 visit order and the lava fluid AABB clip are in, but `Aabb::collided_along_vector` still subtracts 1e-7 in every release while 1.21.5-1.21.8 inflate by exactly half the size (1.21.5 AABB.java:392 vs 1.21.9 :396); that needs a release-gated epsilon, and earlier honey/bubble-column fall resets in the visit order and the non-full cauldron/end-portal/tripwire inside shapes are not modelled.

#### D-021 · Bounce scale chosen by "is player" rather than LivingEntity; honey SUPPRESSES_BOUNCE never sent
- **Versions:** all (0.8 for non-player living entities; 1.8.8 has no 0.8 at all in vanilla); 26.2-26.3 for honey · **Severity:** minor (not reachable on the player path for the scale) · **Status:** confirmed
- **Vanilla:** slime and bed `bounceUp` use `entity instanceof LivingEntity ? 1.0 : 0.8`; 26.2+ `getBlockBounciness` applies ×0.8F only to non-living entities; `SUPPRESSES_BOUNCE = honey_block`. (`origin/client1.21.11 SlimeBlock.java:43`, `BedBlock.java:142`; `26.3 Entity.java:878, 905-911`; `VanillaBlockTagsProvider.java:224`; `1.8.9 BlockSlime`: no scale)
- **Native:** `bounce_scale = is_player ? 1.0 : 0.8` (`native/src/simulation.rs:2581, 1712-1716`). The adapter sends no `suppressesBounce` (`src/mineflayer/blocks.ts:124-134`).
- **Impact:** mobs simulated over N-API bounce 20% lower. In 26.2+, a player with a BOUNCINESS attribute bounces on honey.
- **todo.md:** 171 `[x]` (partial), 939 `[x]` (partial)
- **Fix:** fixed — the `is_player` bounce scale and the non-player 0.8F block bounciness are removed, since SimulationState only moves LivingEntity paths (vanilla scales only non-living entities); boats keep 0.8 from 1.9 (1.8.9 SlimeBlock is a plain -motionY), and the adapter sends `suppressesBounce: true` for honey_block.

#### D-022 · Adapter never sends the Weaving effect
- **Versions:** 1.20.5-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** cobweb with WEAVING uses `(0.5, 0.25, 0.5)` (`origin/client1.21.11 WebBlock.java:25-33`).
- **Native:** the engine supports it (`native/src/simulation.rs:716`), but `src/mineflayer/player.ts:337-405` never sets `weaving`, and `napi_api.rs:647` defaults it to false.
- **Impact:** a player with Weaving moves about half as fast horizontally and 5× slower vertically in cobwebs than vanilla.
- **todo.md:** 951 `[x]` (partial)
- **Fix:** fixed — the adapter sends `weaving` when `state.effects` holds minecraft-data's Weaving effect (looked up by name via a new `effectLevelByName`), false on releases without it; the engine already applied (0.5, 0.25, 0.5).

#### D-023 · Magma damage gate: an extra fall-flying condition, and no frost walker check
- **Versions:** all magma versions (fall-flying gate); 1.13.2-1.20.6 (frost walker in stepOn) · **Severity:** minor (contact-damage output only) · **Status:** partially-confirmed (the claimed 1.14 sneak difference was refuted: 1.14.4 Entity.move also skips stepOn for sneaking players)
- **Vanilla:** `MagmaBlock.stepOn` checks `!isSteppingCarefully && LivingEntity && !hasFrostWalker` (≤1.20.6); nothing checks fall flying (`origin/client1.20.4 MagmaBlock.java:31`, `1.21.11 :29`).
- **Native:** `BlockEffect::Magma if !stepping_carefully && !state.fall_flying` (`native/src/simulation.rs:852`).
- **Impact:** wrong `contact_damage` on a fall-flying landing tick and with frost walker boots.
- **todo.md:** 973 `[x]` (partial), 825 (open)
- **Fix:** fixed — the fall-flying gate is removed, and a new `frost_walker` state input (N-API `frostWalker`, read by the adapter from the boots in nbt or 1.20.5+ component form) spares magma damage in every magma version (1.10+ stepOn; 1.21+ the enchantment's #burn_from_stepping immunity) and lit campfires from 1.14.

#### D-024 · Inside-block signals use the pre-move box, a fixed 1e-5 deflation and different freeze clearing
- **Versions:** deflation 1.8.8-1.21.1; one-tick lag in all versions; freeze clearing 1.17-26.3 · **Severity:** minor (signals only: `contact_damage`, `in_web`, `ticks_frozen`, `freeze_damage`) · **Status:** confirmed
- **Vanilla:**
  - checkInsideBlocks runs on the post-move box; from 1.21.2 on the swept movements.
  - The deflation is 0.001 before 1.19.3 and 1e-7 for 1.19.3-1.21.1.
  - Freeze ticks are kept on the server. Burning clears them before 1.21.2; from 1.21.2 fire, lava and lava cauldrons apply CLEAR_FREEZE.
  - Sources: `origin/client1.20.4 Entity.java:976`, `1.19.1`, `1.8.9 Entity.java:648-650`, `1.21.11 InsideBlockEffectType.java`.
- **Native:** `effects_in_bounds(bounds)` on the start-of-tick box with 1e-5 (`native/src/simulation.rs:2154, 2263-2300`; `world.rs:740-741`). Only fire and soul fire clear freeze.
- **Impact:** the signals are one tick late at boundaries, and freeze ticks are not cleared by lava or burning.
- **todo.md:** 966, 978, 982, 984 `[x]` (all partial)
- **Fix:** deferred — from 1.21.5 the lava test now sweeps from the step start box and freezing is grouped per StepBasedCollector step with CLEAR_FREEZE/ignite in enum order, but 1.17-1.21.4 (and fire in 1.21.5-1.21.8) clear on fireTicks/remainingFireTicks state the engine does not keep, and the 1.21.9+ lava-cauldron CLEAR_FREEZE needs a cauldron inside shape and block effect the world data lacks.

#### D-025 · 1.16 and 1.16.1 Soul Speed reads the block at y-0.5000001, not getBlockStateOn
- **Versions:** 1.16, 1.16.1 · **Severity:** minor · **Status:** confirmed (javap)
- **Vanilla:** `onSoulSpeedBlock = getBlockStateOn().is(SOUL_SPEED_BLOCKS)`, with getOnPos at `floor(y - 0.20000000298023224)` and the fence fallback. From 1.16.2 it uses getBlockPosBelowThatAffectsMyMovement. (1.16.1 `LivingEntity.onSoulSpeedBlock` and `Entity.getOnPos` bytecode; `origin/client1.16.4 LivingEntity.java:404-410`)
- **Native:** the y-0.5000001 position for 1.16+ (`native/src/simulation.rs:742, 750-757`).
- **Impact:** when the feet are 0.2-0.5 above the soul-sand cell (on a bottom slab over soul sand, or jumping off soul sand with Soul Speed), vanilla gives 0.4 and native 1.0.
- **todo.md:** 827 `[x]` (partial)
- **Fix:** fixed — in 1.16 and 1.16.1 `block_speed_factor` reads the Soul Speed block at the new `soul_speed_position`, floor(y - f64(0.2f32)) (getBlockStateOn); 1.16.2+ keep the block below that affects movement (1.16.2-1.16.4 bracketed by 1.16.1 and 1.16.5).

#### D-026 · The block fall callback flag matches no vanilla hook
- **Versions:** all · **Severity:** cosmetic (no physics state reads it) · **Status:** partially-confirmed
- **Vanilla:** `updateEntityMovementAfterFallOn` fires on any vertical collision, ceilings included (gated on local control from 1.21.2; gone in 26.2). 26.2 has restitution plus `fallOn` (requires fallDistance > 0). (`origin/client1.21.1 Entity.java:691-693`; `1.21.2 :720-724`; `26.2 Entity.java:815, 1565-1577`)
- **Native:** `block_fall_callback = vertical_collision_below && controlled_by_local_instance` in every release (`native/src/simulation.rs:2542-2543`).
- **Impact:** only external readers of the typed event see the difference.
- **todo.md:** 157 `[x]` (partial), 176 `[x]` (partial)
- **Fix:** fixed — from 26.2 `block_fall_callback` also requires a non-zero vertical delta after `maybeBackOffFromEdge` (vanilla movedVertically); tests cover the 1.21.2 local-control gate and a no-y step-up that is true in 1.21.11/26.1 and false in 26.2/26.3.

### 2.4 Fluids (player path)

#### D-027 · Adapter never sends ultraWarm/FAST_LAVA, so Nether lava pushes at 0.0023333 instead of 0.007
- **Versions:** 1.16-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `dimensionType().ultraWarm() ? 0.007 : 0.0023333333333333335` (FAST_LAVA from 1.21.11) (`origin/client1.16.4 Entity.java:997`, `1.21.11 :1561`, `26.3 :1704`).
- **Native:** the engine supports `state.ultra_warm` (`native/src/simulation.rs:2192-2199, 2570-2575`), but `src/mineflayer/player.ts:302-405` never sets it, and `napi_api.rs:642` defaults it to false.
- **Impact:** in flowing Nether lava the player's current push is 1/3 of vanilla's (0.0045 instead of 0.007 near rest).
- **todo.md:** 309 `[x]` (partial); 311/313 hold for the engine only

#### D-028 · Adapter keeps the 0.1111111119389534 eye offset in 1.20.5-1.20.6
- **Versions:** 1.20.5, 1.20.6 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `updateFluidOnEyes` uses `getEyeY()` with no offset from 1.20.5; 1.20.4 subtracts 0.1111111119389534 (`origin/client1.20.5 Entity.java:1276`, `1.20.4 :1227`).
- **Native:** `if (!this.lt('1.16') && this.lt('1.21')) eyeY -= 0.1111111119389534;` (`src/mineflayer/player.ts:502`)
- **Impact:** within ~0.111 of the water surface, isUnderWater is true too early or stays true too long. This changes the swim start, the sprint gates and the SWIMMING pose/box.
- **todo.md:** 856 (open)

#### D-029 · 26.3 profile drops the gravity-argument check in getFluidFallingAdjustedMovement
- **Versions:** 26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `if (baseGravity != 0.0 && !isSprinting())` (unchanged from 1.20.5 to 26.3) (`origin/client26.3 LivingEntity.java:2608-2620`).
- **Native:** `fluid_falling_checks_gravity_argument: false` (`native/src/versioned/v-26_3.rs:29`; every other profile from 1.20.5 has true), read at `simulation.rs:1064-1072`.
- **Impact:** with a gravity attribute of 0, a falling player in water or shallow lava is snapped to -0.003 per tick and sinks.
- **todo.md:** 891 `[x]` (deviates; its text also gives the wrong breakpoint, the real one is 1.20.5), 874 `[x]` (partial)

#### D-030 · Swim-steering head test: water only, and y+0.9
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** steering applies when any non-empty fluid (lava included) is at `BlockPos(x, (y + 1.0) - 0.1, z)` (`origin/client1.21.11 Player.java:travel:1326`, `1.13.2 EntityPlayer.java:1424`).
- **Native:** `floor(y + 0.9)`, Water or Waterlogged only (`native/src/simulation.rs:2308-2316`, used at :2376).
- **Impact:** with lava at head height, or at y values such as 3.1 or 63.1, vertical velocity differs by `(look.y - vy) * 0.06`.
- **todo.md:** 885 `[x]` (deviates), 901 `[x]` (partial)

#### D-031 · Adapter requires water at the feet to start swimming before 1.17.1
- **Versions:** 1.13.2-1.17.0 (javap: the check arrives in 1.17.1) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `setSwimming(isSprinting() && isUnderWater() && !isPassenger())`; the `getFluidState(blockPosition).is(WATER)` check first appears in 1.17.1 (`origin/client1.16.4 Entity.java:985-990`; 1.17.1 `Entity.updateSwimming` bytecode 63-76).
- **Native:** `swimming = sprinting && isUnderWater && isWaterish(feet)` for every release from 1.13 (`src/mineflayer/player.ts:562-571`).
- **Impact:** a sprinting player whose eyes are under water but whose feet block has no water starts swimming only in vanilla.
- **todo.md:** none

#### D-032 · Adapter flow vector approximates blocksMotion and isFaceSturdy
- **Versions:** 1.13.2-26.3 (flattened path) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - Whether to look at the fluid below a neighbour uses Material.blocksMotion (1.13-1.19), `BlockState.blocksMotion()` (1.20-26.2), or the `BLOCKS_FLUID_FLOW` tag (26.3).
  - The falling -6 turn uses isFaceSturdy on the union of the support shape. Leaves are never sturdy from 1.16; in 1.13.2 there is an exception list.
  - Sources: `origin/client1.16.4 FlowingFluid.java:62,115`, `1.20.4 :70,109-117`, `26.3 :61`, `1.13.2 :67,131-143`.
- **Native:** "the neighbour has no collision box" and "one box spans the face" (`src/mineflayer/blocks.ts:151-156, 172, 197-199`).
- **Impact:** wrong water current next to carpets, snow, scaffolding, signs and banners, leaves, stairs, soul sand and similar blocks.
- **todo.md:** 863 (open)

#### D-033 · Before 1.21.5 the bubble-column top treats non-colliding blocks as air
- **Versions:** 1.13.2-1.21.4 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the surface push needs `getBlockState(above).isAir()`; a torch, sign or similar block gives the inside push (0.7/-0.3 caps) and a fall reset (`origin/client1.21.4 BubbleColumnBlock.java:entityInside:49-52`).
- **Native:** `world.is_air_at(above)` is correct, but the adapter never writes non-colliding, non-fluid, effect-less blocks, so they read as air (`native/src/simulation.rs:948-953`; `src/mineflayer/blocks.ts:236-262`).
- **Impact:** native applies the 1.8/-0.9 surface push and keeps the fall distance.
- **todo.md:** 908 `[x]` (partial)

#### D-034 · Vec3.normalize threshold stays at 1.0E-4 after 1.21.2
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `normalize` returns zero only when the length is `< 9.999999747378752E-6`; this threshold arrives in 1.21.2 (`origin/client1.21.2 Vec3.java:71-74`; 1.21.1 uses `< 1.0E-4`).
- **Native:** `normalize` uses `< 1.0e-4` for every release in `push_by_fluid` (`native/src/simulation.rs:629-636, 584-607`). `math.rs::normalized` (1e-5) is unused.
- **Impact:** a slow player in a faint current (impulse length 1e-5 to 1e-4) gets no 0.0045 push.
- **todo.md:** 439 `[x]` (deviates)

#### D-035 · 1.16 and 1.16.1 lava handling while in water
- **Versions:** 1.16, 1.16.1 · **Severity:** minor · **Status:** confirmed (javap)
- **Vanilla:** `updateInWaterStateAndDoFluidPushing` returns early when in water, so there is no lava push and the lava height is 0. `isInLava` is the `isTouchingLava` flag set by `LiquidBlock.entityInside`. The 1.16.2+ rule (always measure, fluidHeight>0) starts in 1.16.2.
- **Native:** the 1.16.2 semantics from 1.16.0 (`native/src/simulation.rs:2192-2201`; `ridden.rs:358-362`).
- **Impact:** an extra lava current and a nonzero lava jump height while in water touching lava.
- **todo.md:** none

#### D-036 · From 26.1, fluid contact ignores unloaded chunks
- **Versions:** 26.1-26.3 (1.8.8-1.21.11 have no deviation, because the client's hasChunk always returns true) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `EntityFluidInteraction.update` skips the whole scan if any chunk under the box widened by 1 is not loaded (`origin/client26.1 EntityFluidInteraction.java:44, 94-110`).
- **Native:** `fluid_contact` never checks loaded state (`native/src/world.rs:533-579`).
- **Impact:** water and lava state and push at the edge of loaded chunks.
- **todo.md:** none

#### D-037 · Native eye_in_water/eye_in_lava sampled at y+height-0.1
- **Versions:** all · **Severity:** minor (output-only flags; the adapter computes its own) · **Status:** confirmed
- **Vanilla:**
  - 1.21+ uses `getEyeY()`.
  - 1.16-1.20.4 subtracts 0.1111111119389534.
  - 1.13-1.15 adds +0.11111111F to the fluid top.
  - 1.8.9 uses the `isInsideOfMaterial` form.
  - Sources: `origin/client1.21.11 Entity.java:1588-1609`, `1.20.4 :1224-1238`, `1.14.4 :1152-1163`, `1.8.9 :816-828`.
- **Native:** `eye_y = position.y + height - 0.1` (`native/src/simulation.rs:2296-2321`).
- **Impact:** wrong `eyeInWater`/`eyeInLava` N-API outputs near the surface.
- **todo.md:** 782 `[x]` (deviates), 860 `[x]` (partial)

#### D-038 · Adapter eye-in-water precision
- **Versions:** 1.13.2 (float add order); 26.1-26.3 (inclusive double test over the whole box); 26.3 (`getHeightForCamera`) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - 1.13.2: `((float)y + h) + 0.11111111F`.
  - 26.1+: `eyeY >= fluidBottom && eyeY <= fluidTop`, in double.
  - 26.3: a source fluid under a sturdy ceiling uses 1.0.
  - Sources: `origin/client1.13.2 Entity.java:1136`; `26.1 EntityFluidInteraction.java:61,70`; `26.3 :84-87`, `FluidState.java:53-60`.
- **Native:** `src/mineflayer/player.ts:510-512` (float, strict, ordinary height).
- **Impact:** the 26.3 camera-height case changes isUnderWater, and therefore the sprint and swim decisions; the rest is within 1 ulp.
- **todo.md:** 856 (open)

### 2.5 Sprint, pose, flight, elytra, auto-jump (engine and adapter)

#### D-039 · Adapter crouches a flying player, so flight with sneak held is slowed to 0.3
- **Versions:** 1.15-26.3 (1.15-1.16.3 inferred; 1.14.x is correct because vanilla 1.14 also scales and then divides by 0.3) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `crouching = !abilities.flying && !isSwimming() && … && (isShiftKeyDown() || …)`, so `isMovingSlowly` is false while flying (`origin/client1.16.4 LocalPlayer.java:603`, `1.21.11 :715`, `26.3 :764`).
- **Native:** `crouching = !swimmingAndNotFlying(state) && …`, which is true while flying (`src/mineflayer/player.ts:573-575, 594-599`). MOVING_SLOWLY then reaches the engine's input scaling (`native/src/simulation.rs:1276-1282, 1300-1302`).
- **Impact:** descending creative or spectator flight moves at ~30% of vanilla horizontal speed. Starting a sprint is refused, and in 1.21.4 an existing sprint is stopped.
- **todo.md:** none

#### D-040 · From 1.17 the adapter refreshes the pose before the move
- **Versions:** 1.17-26.3 (1.17.x bracketed by 1.16.4 and 1.18.1) · **Severity:** major (one verifier rated it minor: only geometry-constrained cases change position) · **Status:** confirmed
- **Vanilla:** `updatePlayerPose()` is the last call in `Player.tick`, after the move. Nothing else on the client sets the pose. (`origin/client1.21.11 Player.java:249-274`, `1.18.1 :233/276`, `26.3 :274`)
- **Native:** `if (gt('1.13.2') && !lt('1.17')) this.updatePoses(state, sync)` before the native tick (`src/mineflayer/player.ts:211-212`). The box and eye height sent to native then belong to the new pose.
- **Impact:** on the first tick of sneaking, swimming or gliding (start or stop), the move uses the new box height, so ceiling, 1-high gap, fluid contact and inside_wall results differ for one tick.
- **todo.md:** 406 (open), 744 `[x]` (partial)

#### D-041 · From 1.21.5, gliding into a climbable block neither stops the glide nor switches to air travel
- **Versions:** 1.21.5-26.3 (from 1.21.11, `#can_glide_through` blocks are exempt) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `travelFallFlying`: `if (onClimbable()) { travelInAir(input); stopFallFlying(); }` (`origin/client1.21.5 LivingEntity.java:2273-2276`, `26.3 :2499-2502`).
- **Native:** `gliding = fall_flying && fluid Empty`. The climbable clamp, climb and drag are skipped while gliding, and neither the engine nor the adapter clears `fall_flying`. (`native/src/simulation.rs:2464-2477, 2706-2713`; `src/mineflayer/player.ts:284`)
- **Impact:** an elytra flight into a ladder or scaffolding keeps elytra physics and the 0.6 box.
- **todo.md:** 1022 (open)

#### D-042 · 1.8.8: flying while sneaking undoes the 0.3 sneak slowdown
- **Versions:** 1.8.8 (1.9-1.12 unknown; the undo exists in 1.13.2) · **Severity:** major · **Status:** confirmed
- **Vanilla:** 1.8.9 `EntityPlayerSP.onLivingUpdate` only changes motionY for flight sneak and jump; the impulses stay at ×0.3 (`origin/client1.8.9 EntityPlayerSP.java:561-569`; `1.13.2 :760-766` adds `/0.3D`).
- **Native:** `if flying && sneaking && !at_least(1,15,0) { input /= 0.3 }` (`native/src/simulation.rs:1286-1288`).
- **Impact:** sneak-descending flight on 1.8.8 is about 3.33× faster horizontally.
- **todo.md:** none

#### D-043 · Auto-jump obstacle scan leaves out entity colliders
- **Versions:** 1.10.2-26.3 (1.13.2-26.3 verified; 1.10-1.12 bracketed) · **Severity:** major · **Status:** partially-confirmed (minecarts are never colliders; boats in all versions, shulkers from 1.11)
- **Vanilla:** `level.getCollisions(this, testBox)` includes entity shapes: before block shapes from 1.18, after them before that (`origin/client1.21.11 LocalPlayer.java:981`, `CollisionGetter.java:85-89`; `1.16.4 CollisionGetter.java:60-61`).
- **Native:** `update_auto_jump` iterates only `block_collision_boxes_in_order` (`native/src/simulation.rs:2052`), while the move itself does use `entity_collisions` (:1786).
- **Impact:** with auto-jump on, the player does not jump onto boats or shulkers, and from 1.18 a different obstacle can be chosen.
- **todo.md:** none

#### D-044 · Riptide spin: SPIN_ATTACK box and -0.2 rebound missing; pose priority and passenger rules
- **Versions:** 1.13.2 (legacy size), 1.14.4-26.3 (pose); 1.21.5-26.3 (priority order, ordinal only) · **Severity:** minor (the 1.13.2 verifiers rated it major and minor) · **Status:** partially-confirmed
- **Vanilla:**
  - While the synced flag `isAutoSpinAttack` is set, the pose is `SPIN_ATTACK` (0.6×0.6, eye 0.4), or 0.6×0.6 via `EntityPlayer.updateSize` in 1.13.2.
  - `checkAutoSpinAttack` scales velocity by -0.2 when the spin hits a LivingEntity.
  - From 1.21.5 the priority is sleeping, then swimming, then fall flying, then spin.
  - The fit fallback is skipped for passengers.
  - Sources: `origin/client1.14.4 Player.java:333-361`, `1.21.11 Player.java:336-365`, `LivingEntity.java:3090-3104`, `1.13.2 EntityPlayer.java:336-365`.
- **Native:** `updatePoses` and `updateLegacySize` never select spin; the adapter ends the spin only on a horizontal collision; there is no passenger bypass (`src/mineflayer/player.ts:784-818, 279-283`).
- **Impact:** during a riptide the player keeps a 1.8 (or 1.5/1.65) box, so ceilings and gaps differ, and hits on mobs do not bounce the player back.
- **todo.md:** 751 `[x]` (deviates), 749 `[x]` (partial)

#### D-045 · canStartSprinting: fall-flying and in-water terms used for 1.13.2-1.21.4
- **Versions:** 1.13.2-1.21.4 · **Severity:** minor · **Status:** confirmed
- **Vanilla:**
  - 1.13.2-1.19.3 have no fall-flying test.
  - 1.19.4-1.21.4 use a strict `!isFallFlying()`.
  - The in-water test applies only to the sprint-key path.
  - The adapter's combined form first appears in 1.21.5.
  - Sources: `origin/client1.16.4 LocalPlayer.java:633-655`, `1.21.4 :676-688, 1012-1013`, `1.21.5 :1019-1020`.
- **Native:** `src/mineflayer/player.ts:677-686` applies the 1.21.5 terms to both paths.
- **Impact:** sprint state differs while and after gliding, and a double-tap that starts in shallow water does not arm.
- **todo.md:** none

#### D-046 · 1.21.4 shouldStopSprinting omits isFallFlying
- **Versions:** 1.21.4 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `isFallFlying() || hasBlindness() || isMovingSlowly() || isPassenger() && !isRidingCamel() || …` (`origin/client1.21.4 LocalPlayer.java:795-796`).
- **Native:** `src/mineflayer/player.ts:604-608` has no fall-flying term and no passenger term.
- **Impact:** the sprint flag and sprint modifier survive a glide.
- **todo.md:** none

#### D-047 · Swim-sprint stop uses the 1.21.5 rule (blindness, low food) on older releases
- **Versions:** 1.13.2-1.21.4 (blindness part 1.13.2-1.21.3) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `if (!onGround && !shift && (!forwardImpulse || !food) || !isInWater()) setSprinting(false)`, with no blindness stop (`origin/client1.14.4 LocalPlayer.java:632-640`, `1.21.4 :691-697`; the new rule is in `1.21.5 :819-821`).
- **Native:** `shouldStopSwimSprinting = blindness || !inWater || … || !food` (`src/mineflayer/player.ts:710-715`).
- **Impact:** with low food on the ground or holding shift, or under blindness, swimming ends early (pose, drag and speed differ).
- **todo.md:** none

#### D-048 · From 1.21.9, flying players are exempt from the shallow-water sprint block; the adapter is not
- **Versions:** 1.21.9-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `isSprintingPossible(getAbilities().flying)` → `(allowedInShallowWater || !isInShallowWater())` (`origin/client1.21.9 LocalPlayer.java:814-815, 1014-1019`).
- **Native:** `src/mineflayer/player.ts:685, 707` always block.
- **Impact:** creative flight skimming water loses the sprint (2× flying speed).
- **todo.md:** 719 (open)

#### D-049 · Adapter clears fall-flying after a flying tick on 1.21.2+
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the client no longer clears flag 7 in Player.travel; only the server does (`origin/client1.21.2 Player.java:1399-1421`, `LivingEntity.java:2864-2869`).
- **Native:** `if (flyingDuringMove) state.fallFlying = false` for all releases (`src/mineflayer/player.ts:284`). The engine gates this correctly (`simulation.rs:2135, 2785`).
- **Impact:** the glide stops a tick or more early when creative flight is toggled.
- **todo.md:** none

#### D-050 · 26.3 refuses to start fall flying in lava; the adapter checks only water
- **Versions:** 26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `tryToStartFallFlying` requires `!isInLiquid()` (`origin/client26.3 Player.java:1351-1352`).
- **Native:** `!state.isInWater` only (`src/mineflayer/player.ts:755-760`).
- **Impact:** the FALL_FLYING pose and glide start in lava.
- **todo.md:** 1016 (open)

#### D-051 · Elytra start checks climbable from 1.15; vanilla adds the check in 1.15.2
- **Versions:** 1.15, 1.15.1 · **Severity:** minor · **Status:** confirmed (from `generated/physics-methods.json`)
- **Vanilla:** the 1.15 and 1.15.1 `LocalPlayer.aiStep` have no `onLadder()` check; 1.15.2 adds it.
- **Native:** `!state.onClimbable` from 1.15 (`src/mineflayer/player.ts:757`).
- **Impact:** no glide start on ladders or vines in 1.15 and 1.15.1.
- **todo.md:** none

#### D-052 · Auto-jump's synthetic jump is not counted in wasJumping
- **Versions:** flight toggle 1.13.2-26.3 (1.11-1.12 likely); elytra start 1.15-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `wasJumping` is read from `input.keyPresses` before the tick, and that includes `makeJump()`. The flight double-tap also requires no auto-jump on the tick. (`origin/client1.21.11 LocalPlayer.java:711-722, 768, 784`)
- **Native:** `freshJump = !prevControl.jump && control.jump`, from raw keys (`src/mineflayer/player.ts:739, 744, 757, 294`).
- **Impact:** the adapter acts on jump presses that vanilla ignores after or during an auto-jump.
- **todo.md:** none

#### D-053 · Riptide launch needs water; vanilla also allows rain
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `isInWaterOrRain()` (`isWet()` in 1.13.2) (`origin/client1.21.11 TridentItem.java:releaseUsing`; `1.13.2 ItemTrident`).
- **Native:** `state.isInWater` only, and there is no rain input (`src/mineflayer/player.ts:180`).
- **Impact:** no launch in rain (push, spin, 1.2 lift).
- **todo.md:** none

#### D-054 · Riptide ground lift skips the inside-block pass
- **Versions:** 1.13.2-1.21.1 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the 1.2 lift is a full `Entity.move`, so `checkInsideBlocks` runs (bubble columns, cobweb and so on) (`origin/client1.20.4 Entity.java:713`; `TridentItem.releaseUsing`).
- **Native:** `launch_riptide` only collides and moves (`native/src/simulation.rs:1888-1912`).
- **Impact:** one bubble-column or stuck application fewer on the launch tick.
- **todo.md:** none

#### D-055 · 1.8.8: the 600-tick sprint timeout is missing
- **Versions:** 1.8.8 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `sprintingTicksLeft = sprinting ? 600 : 0`, counted down, then `setSprinting(false)` (`origin/client1.8.9 EntityPlayerSP.java:337-339, 463-469`).
- **Native:** no counter (`src/mineflayer/player.ts:641-731`).
- **Impact:** a double-tap sprint without the sprint key lasts past 30 s.
- **todo.md:** none

#### D-056 · Sprint-key start is blocked by the previous tick's sneak before 1.14
- **Versions:** 1.8.8-1.13.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the sprint-key branch has no sneak condition; only the double-tap branch checks the previous sneak (`origin/client1.8.9 EntityPlayerSP.java:528-536`; `1.13.2 :699-707`).
- **Native:** `canStartSprinting` has `(!crouching || isUnderWater)` with crouching = previous sneak (`src/mineflayer/player.ts:595, 665, 677-685`).
- **Impact:** a one-tick sprint lag after letting go of sneak.
- **todo.md:** none

#### D-057 · The adapter never imports the no-gravity flag
- **Versions:** 1.10.2-26.3 (1.8.8/1.9.4 have no shared flag) · **Severity:** minor · **Status:** partially-confirmed (`vehicle.rs`/`ridden.rs` also lack any no-gravity handling)
- **Vanilla:** `Entity.getGravity` returns 0 when `isNoGravity()` (`origin/client1.21.11 Entity.java:1443-1445`).
- **Native:** the engine supports `no_gravity`, but `src/mineflayer/player.ts:302-405` and `vehicles.ts` never set it.
- **Impact:** a player or mount with the NoGravity metadata still falls.
- **todo.md:** 395 `[x]` (non-physics-false), 761 `[x]` (non-physics-false)

#### D-058 · Eye heights for SLEEPING and DYING
- **Versions:** 1.14.4-26.3 (observable for sleeping 1.14.4-1.15.2 and 1.21-26.3) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** SLEEPING eye is 0.2F and DYING eye is 1.62F (fixed) (`origin/client1.20.4 LivingEntity.java:3286-3288`; `1.21.11 LivingEntity.java:3878`, `Avatar.java:55`).
- **Native:** `player.rs:41-45` and `napi_api.rs:2083-2090` give SLEEPING 1.62. The adapter gives both 0.4, and scales DYING (`src/mineflayer/player.ts:889-902`).
- **Impact:** eye-in-water for a sleeping player over a bed with water above.
- **todo.md:** 356 `[x]` (deviates)

#### D-059 · 1.13.2 auto-jump obstacle order: full cubes are deferred and merged
- **Versions:** 1.13.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** full-cube cells are collected in a bitset and appended at the end as one merged VoxelShapeInt, so partial shapes are tested first, and the "above" probe starts from the merged box's center (`origin/client1.13.2 IWorldReaderBase.java:func_212391_a:112-160`; `EntityPlayerSP.java:902-905`).
- **Native:** per-cell order for every release (`native/src/world.rs:935-975`; `simulation.rs:2052-2081`).
- **Impact:** next to fences or slabs, or a stacked block, the jump/no-jump decision differs.
- **todo.md:** none

#### D-060 · Before 1.14 the flight vertical input is applied as one merged impulse
- **Versions:** 1.8.8-1.13.2 · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** separate `motionY -= a` and `motionY += a` (`origin/client1.8.9 EntityPlayerSP.java:561-569`; `1.13.2 :760-771`).
- **Native:** a net `direction` impulse (`native/src/simulation.rs:2213-2218`).
- **Impact:** a 1-ulp difference in vy with jump and sneak held together.
- **todo.md:** 757 `[x]` (holds; cosmetic)

#### D-061 · Adapter scales the fixed DYING box by SCALE
- **Versions:** 1.20.5-26.3 · **Severity:** cosmetic · **Status:** partially-confirmed
- **Vanilla:** `EntityDimensions.fixed(0.2F, 0.2F)` ignores scale (`origin/client1.21.11 Avatar.java:55`).
- **Native:** `src/mineflayer/player.ts:858-862` exempts only SLEEPING.
- **Impact:** at most a transient tick (DYING is replaced on 1.17+).
- **todo.md:** none

#### D-062 · Auto-jump start-Y gate at 1.16; vanilla 1.15 already uses position()
- **Versions:** 1.15-1.15.2 · **Severity:** cosmetic · **Status:** confirmed (javap)
- **Vanilla:** `updateAutoJump` uses `position()` from 1.15.
- **Native:** `feet_y` uses `bounds.min.y` before 1.16 (`native/src/simulation.rs:1975-1979`).
- **Impact:** none for consistent input (position.y == minY before 1.17).
- **todo.md:** none

### 2.6 Attributes and effects

#### D-063 · Before 1.20.5, effects with negative-byte amplifiers are dropped
- **Versions:** 1.8.8-1.20.4 (Levitation 1.9+, Slow Falling and Dolphin's Grace 1.13+) · **Severity:** major · **Status:** confirmed
- **Vanilla:** the effect packet reads the amplifier as a signed `readByte()`, and `MobEffectInstance` does not clamp it before 1.20.5. So `/effect … 255` is amp -1 and stays active: Levitation targets 0 (hover), and Jump Boost 128-254 lowers jump power. (`origin/client1.20.4 ClientboundUpdateMobEffectPacket.java:50`, `MobEffectInstance.java:76`, `LivingEntity.java:2017, 2157`; `1.8.9 S1DPacketEntityEffect.java:37`)
- **Native:** `getEffectLevel = amp + 1`, and the adapter treats a level ≤ 0 as no effect. The N-API fields are `Option<u8>`. (`src/mineflayer/base.ts:93-98`, `player.ts:380-400`; `native/src/napi_api.rs:112-118`)
- **Impact:** a Levitation 255 hover falls in native. Slow Falling, Dolphin's Grace, Blindness (sprint) and negative Jump Boost are ignored.
- **todo.md:** 820 `[x]` (partial), 730 `[x]` (deviates)

#### D-064 · movement_speed and jump_strength lose every op1/op2 modifier except sprint, speed and slowness
- **Versions:** movement_speed in all versions; jump_strength 1.20.5-26.3 · **Severity:** major (not visible in vanilla survival, where those are the only multiplicative modifiers) · **Status:** partially-confirmed
- **Vanilla:** `AttributeInstance.calculateValue` folds every ADD_VALUE, ADD_MULTIPLIED_BASE and ADD_MULTIPLIED_TOTAL modifier, then clamps; getSpeed and getJumpPower use the full value (`origin/client26.3 AttributeInstance.java:151-169`; `1.21.11 LivingEntity.java:2224-2225`).
- **Native:** the adapter sends base + ADD_VALUE only (`src/mineflayer/base.ts:163-167`; `player.ts:378, 389`). The engine re-applies only sprint, speed and slowness (`native/src/simulation.rs:203-228`). The adapter's own `jumpFromGround` uses the full value (`player.ts:769`).
- **Impact:** wrong ground, air and water speed and jump height under item, `/attribute` or plugin modifiers.
- **todo.md:** 26 `[x]` (partial), 794 `[x]` (partial), 800 `[x]` (partial)

#### D-065 · From 1.21.2, Slow Falling uses the pre-move velocity
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `travelInAir` calls `getEffectiveGravity()` after the move, so it tests the post-collision delta (`origin/client1.21.2 LivingEntity.java:2194-2197, 2213-2229`).
- **Native:** `going_down` is computed before the move (`native/src/simulation.rs:2381-2386`, used at :2724).
- **Impact:** with Slow Falling, a ceiling-hit jump gets 0.08 gravity in native (vanilla 0.01), and a slime or bed bounce gets 0.01 in native (vanilla 0.08).
- **todo.md:** 818 `[x]` (partial)

#### D-066 · Adapter ADD_MULTIPLIED_TOTAL formula and modifier order
- **Versions:** effective on 1.20.5-26.3 (the attributes resolved this way exist only there) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `total *= 1.0 + amount`, iterated in hash-container order (`origin/client1.21.11 AttributeInstance.java:150-172`).
- **Native:** `value += value * amount`, in packet order (`src/mineflayer/base.ts:140-154`).
- **Impact:** 1-ulp differences (about 26% of inputs) in gravity, step height and scale under custom modifiers.
- **todo.md:** 34 `[x]` (deviates), 791 `[x]` (partial)

#### D-067 · Compact attribute objects without a modifiers array throw
- **Versions:** all · **Severity:** minor · **Status:** confirmed
- **Vanilla:** an attribute with no modifiers evaluates to its clamped base.
- **Native:** `for (const modifier of attribute.modifiers)` and `.modifiers.filter/find/some` have no fallback (`src/mineflayer/base.ts:148-150, 166`; `player.ts:412, 427`). Stock Mineflayer always fills the array.
- **Impact:** a TypeError stops the tick for custom callers.
- **todo.md:** 34 `[x]` (deviates)

#### D-068 · Generic entity descriptor drops movement_efficiency, air_drag_modifier, friction_modifier and other fields
- **Versions:** 1.21-26.3 / 26.2-26.3 / all · **Severity:** minor (an unused helper) · **Status:** partially-confirmed
- **Vanilla:** every LivingEntity reads MOVEMENT_EFFICIENCY (1.21+) and FRICTION/AIR_DRAG modifiers (26.2+).
- **Native:** `create_simulation_state` forwards `water_movement_efficiency` only. It drops movement_efficiency, the modifiers, sneaking_speed, flying, flying_speed, jump_ticks, using_item and no_physics. (`native/src/napi_api.rs:122-124, 1960-2041`)
- **Impact:** wrong soul-sand speed, friction and drag for callers of `createSimulationState`.
- **todo.md:** 29 `[x]` (deviates)

#### D-069 · Per-entity gravity and step height are not validated on the NativePlayer/simulateVersionedTick paths
- **Versions:** all (attributes exist from 1.20.5) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `RangedAttribute.sanitizeValue` maps NaN to the minimum and clamps (gravity -1..1, step 0..10).
- **Native:** `valid_state` is called only on the descriptor path (`native/src/napi_api.rs:2757-2782, 2034`). It also rejects negative gravity, which vanilla allows.
- **Impact:** NaN velocity is silently zeroed on the next tick instead of being rejected.
- **todo.md:** 397 `[x]` (non-physics-false)

### 2.7 Collision, world representation and block data

#### D-070 · The adapter never sends the world border
- **Versions:** all · **Severity:** major · **Status:** confirmed
- **Vanilla:** the client WorldBorder follows the server's packets, and `Entity.collide` collides with it (`origin/client1.21.11 Entity.java:collectColliders:1146-1160`, `ClientPacketListener.java:1681-1717`; `1.8.9 World.java:885-920`).
- **Native:** `setWorldBounds` exists (`native/src/napi_api.rs:851-876`), but `src/mineflayer/*.ts` never calls it, so the border stays at the default ±29,999,984 (`native/src/world.rs:151-171`). The native world also has no border lerp state.
- **Impact:** on servers with a custom or shrinking border, the player walks through it and the server corrects the position.
- **todo.md:** 561 `[x]` (storage only, holds); 555 (open)
- **Fix:** fixed — mineflayer's world border plugin now models the 1.21.11+ tick-based moving border (ticked at the end of each client tick) and exposes previousSize and absoluteMaxSize; the adapter sends the border through `setWorldBounds` (new optional `borderAbsoluteMaxSize`, clamped in `world_border_edges`), colliding with the previous tick's edges.

#### D-071 · 1.16 and 1.16.1: chains, lanterns and soul lanterns get the wrong collision box
- **Versions:** 1.16, 1.16.1 · **Severity:** major (one verifier minor) · **Status:** confirmed
- **Vanilla:** the 1.16.1 chain has only `waterlogged` and is always vertical, box (6.5,0,6.5)-(9.5,16,9.5)/16. A standing lantern is `box(5,0,5,11,7,11)+box(6,7,6,10,9,10)` (top 9/16). (`origin/client1.16.4 ChainBlock.java`, `LanternBlock.java:53-54, 93`; minecraft-data `pc/1.16.1/blocks.json` state counts)
- **Native:** minecraft-data maps 1.16/1.16.1 shapes to `pc/1.16.1/blockCollisionShapes.json`, which has the 1.16.2 layout. prismarine-block indexes it by `stateId - minStateId`, so both chain states become the x-axis bar and `hanging=false` lanterns get the hanging shape. The adapter passes these through unchanged (`src/mineflayer/blocks.ts:131, 274`).
- **Impact:** a vertical chain collides as a horizontal x-bar (the player can stand on it at y+0.59375). A player stands 1/16 too high on standing lanterns.
- **todo.md:** 986 (open)
- **Fix:** fixed — for 1.16 to before 1.16.2, `BlockMapper.boxesAt` gives the upright chain box and the standing/hanging lantern and soul lantern boxes; 1.16.1 is bracketed by the 1.16 and 1.16.4/1.16.5 sources and placed on the 1.16 side by minecraft-data's state counts.

#### D-072 · Zero-Y vertical-collision skip and the local-control fall-damage gate apply to the wrong releases
- **Versions:** 1.8.8-1.21.3 (zero-Y skip); 1.21.2-26.3 (fall-damage gate) · **Severity:** minor (non-local N-API entities only; the player is local) · **Status:** confirmed
- **Vanilla:**
  - verticalCollision, onGround and the support block refresh on every move until 1.21.4 adds `|y| > 0 || isControlledByOrIsLocalPlayer()`.
  - From 1.21.2, `checkFallDamage` runs only if `!isClientSide || isControlledByLocalInstance()` (`isLocalInstanceAuthoritative()` from 1.21.5).
  - Sources: `origin/client1.21.1 Entity.java:670,681`; `1.21.2 :709`; `1.21.4 :707,721`.
- **Native:** the `desired.y != 0.0 || controlled_by_local_instance` guard applies to every release (`native/src/simulation.rs:2539-2545`). `checks_fall_damage` ignores local control (`:2553-2554`). The N-API default is `controlled_by_local_instance = false` (`napi_api.rs:644`).
- **Impact:** non-local entities keep stale onGround and collision flags on a zero-Y move, and from 1.21.2 they gain fall distance and get a water push that vanilla skips.
- **todo.md:** 139 `[x]` (deviates), 148 `[x]` (partial), 664 `[x]` (deviates)
- **Fix:** fixed — the zero-Y vertical-collision guard now applies only from 1.21.4, and from 1.21.2 fall damage also requires `controlled_by_local_instance` (the 1.21.4 player exclusion is kept); tests modelling a local player now set that flag.

#### D-073 · mainSupportingBlockPos is recomputed without state
- **Versions:** 1.20-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the value is kept between moves. The shifted retry is skipped when `onGroundNoBlocks`, a null movement keeps the old value, and the entity collision context is used. (`origin/client1.21.11 Entity.java:checkSupportingBlock:699-722`)
- **Native:** recomputed at every call from the current world with an unconditional retry and context-free shapes (`native/src/simulation.rs:812-833, 2153, 2545`).
- **Impact:** rare friction, speed or landing-column differences (supported by entities or powder snow, blocks changing between ticks).
- **todo.md:** 167 `[x]` (holds apart from this memory)
- **Fix:** deferred — a state arriving with on_ground=false now clears supporting_block/on_ground_no_blocks, but ridden.rs and vehicle.rs still call the stateless `main_supporting_block` every tick; fixing it needs per-entity supporting-block state carried between ticks for ridden mobs and vehicles.

#### D-074 · Shapes.collide early-zero check applied per box, in a different order
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `|d| < 1e-7 → 0` is checked between whole VoxelShapes. Each shape resolves in one grid walk. Order: entity shapes, then border, then blocks in Cursor3D order. (`origin/client1.21.11 Shapes.java:collide:175-185`; `Entity.java:1146-1160`)
- **Native:** checked before every box, with boxes sorted by min.x and entity and border boxes last (`native/src/collision.rs:445-470`; `world.rs:1123-1197`; `simulation.rs:1783-1797`).
- **Impact:** ulp-level position residuals (about 1e-16) differ, for example when pressing into a lone stair or wall.
- **todo.md:** 506 `[x]` (non-physics-true), 509 `[x]` (partial)
- **Fix:** deferred — the collision pipeline works on one flat `Vec<Aabb>` with no shape identity; checking zero between whole shapes in Cursor3D order needs a collision-core refactor (about 40 call sites) for ulp-level residuals, best done together with D-075.

#### D-075 · The 1.14-1.16 single-axis Shapes.collide path is not modelled
- **Versions:** 1.14.4-1.16.5 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** a move along a single axis goes through `collideBoundingBoxHeuristically → Shapes.collide(AABB, LevelReader, …)`. It collides in block-local coordinates, returns 0 at `|d| < 1e-7` after every cell, uses lastC, and adds entity and border shapes afterwards with no zero check. (`origin/client1.16.4 Entity.java:740-750, 785`; `Shapes.java:193-260`)
- **Native:** the flat list approach from 1.17+ for every release from 1.14 (`native/src/simulation.rs:1806-1819`; `collision.rs:236, 404-473`).
- **Impact:** last-bit position residuals in rare tight contacts.
- **todo.md:** 509 `[x]` (partial)
- **Fix:** deferred — the range is 1.14-1.17.1 (the 1.17 source has the path, 1.18 does not); the path walks the world cell by cell and needs moving-piston identity, while `collide()` is a pure function on a precomputed obstacle list, so it needs world-backed axis collision plumbed into collide and the step-up.

#### D-076 · Entity collider boxes are grouped with blocks by floor(min)
- **Versions:** 1.13.2 (observable); 1.21-26.3 (theoretical only) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** 1.13 excludes blocks the original box already meets per block position, for block shapes only; entity shapes are appended without exclusion (`origin/client1.13.2 IWorldReaderBase.java:112-156`, `World.java:1914-1916`).
- **Native:** entity boxes are mixed into the block list and keyed by `block_origin(floor(min))` (`native/src/collision.rs:571-605, 363-383`).
- **Impact:** in 1.13.2, a player overlapping a boat that rests on a carpet or snow layer loses that block as a collider and sinks 1/16 or 1/8.
- **todo.md:** none
- **Fix:** fixed — entity colliders travel in their own list: in 1.13.2 the already-met exclusion applies to blocks only and entity boxes are appended after the blocks, from 1.14 entity colliders come first, and from 1.21 the candidate step-up uses every entity collider plus the block shapes.

#### D-077 · 26.3 entity-collider query is not extended up by the step height
- **Versions:** 26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `getEntityCollisions(this, aabb.expandTowards(movement).expandTowards(0, maxUpStep(), 0))`; 26.2 uses `expandTowards(movement)` only (`origin/client26.3 Entity.java:1187`).
- **Native:** `swept.inflate(1e-7)` for every release from 1.14 (`native/src/simulation.rs:988-994, 1785-1787`).
- **Impact:** a step-up into a boat or shulker within step height succeeds in native but is blocked in vanilla.
- **todo.md:** 1007 (open)
- **Fix:** fixed — from 26.3 the entity-collider query box is stretched up by maxUpStep (f32 widened to f64) before the 1e-7 inflate; 26.1 and 26.2 are unchanged.

#### D-078 · Before 1.14, world-border collision uses the 1.14 deflated-box rule
- **Versions:** 1.8.8-1.13.2 (1.9-1.12 bracketed) · **Severity:** minor (direct API only, see D-070) · **Status:** confirmed
- **Vanilla:** an "inside" position test with ±1 hysteresis via the isOutsideBorder flag. Cells outside the border collide as stone or a full cube, only in loaded chunks. (`origin/client1.8.9 World.java:886-920, 942-960`; `1.13.2 IWorldReaderBase.java:112-203`)
- **Native:** border slabs apply only when the deflated box overlaps none of them (`native/src/simulation.rs:1789-1797`).
- **Impact:** with a shrinking border, or an entity just outside it, collision differs.
- **todo.md:** 555 (open)
- **Fix:** fixed — before 1.14 the border follows vanilla's inside test with ±1 hysteresis through a new persistent `outside_border` state (napi `outsideBorder`): when inside, loaded cells outside the border become stone full cubes, otherwise the border does not collide; the 1.9-1.12 releases are confirmed in their sources and 1.8.8 is bracketed by 1.8.9 (vehicle.rs keeps its old pre-1.18 rule).

#### D-079 · 1.12 fence, pane and wall joins ignore stair corner shapes
- **Versions:** 1.12.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** a join requires the neighbour face to be SOLID from `getBlockFaceShape`. Stairs resolve their corner SHAPE first: inner corners have two solid sides, outer corners none. (yarn 1.12.2 `FenceBlock`, `PaneBlock:142-148`, `WallBlock:152-158`, `StairsBlock:165-186`)
- **Native:** `legacy-connections.json` keys stairs by id and metadata as straight stairs (`src/mineflayer/legacy.ts:95-101, 223`).
- **Impact:** fence, pane and wall arms are missing or extra next to corner stairs.
- **todo.md:** none
- **Fix:** fixed — a 1.12 fence, pane or wall joins a neighbouring stair only if the stair's computed shape (from its own neighbours) has a SOLID face toward it; `LegacyShapes.reach` (2 in 1.12) makes NativeWorldSync refresh fences two cells from a changed stair.

#### D-080 · Before 1.13, snow layers=1 has no collision box
- **Versions:** 1.8.8-1.12.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `BlockSnow` returns a zero-height box at the cell bottom, which acts as a floor (`origin/client1.8.9 BlockSnow.java:40-44`; `AxisAlignedBB.calculateYOffset`).
- **Native:** minecraft-data gives `[]` for metadata 0/8 (its generator drops boxes that only touch), and `legacy.ts` has no override.
- **Impact:** a player on a 1-layer snow over 8-layer snow or soul sand sits 0.125-0.25 lower.
- **todo.md:** 986 (open)
- **Fix:** deferred — the player path now keeps the zero-height 1-layer snow box with the strict-intersect filter before 1.13, but vehicle.rs `legacy_colliders` and ridden.rs `travel_in_fluid`'s hop still use the non-flat modern shapes before 1.13.

#### D-081 · Shulker box collision is always a full block
- **Versions:** 1.11.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** while open, the shape grows by up to 0.5 along FACING, and the block entity also moves colliding entities (`origin/client1.20.4 ShulkerBoxBlock.java:253-256`, `ShulkerBoxBlockEntity.java:107-119`; `26.3 :139-146`).
- **Native:** the static full cube; the block entity is not used by collision (`src/mineflayer/blocks.ts:131`; `native/src/world.rs:983-1009, 236-246`).
- **Impact:** the player stands at y+1.0 instead of y+1.5 on an open shulker box facing up.
- **todo.md:** 986 (open)
- **Fix:** deferred — the shape follows the client-side ShulkerBoxBlockEntity animation (block_action driven, 0.1 per tick), which neither mineflayer nor the adapter tracks, the engine has no time-varying block shapes, and `moveCollidedEntities` pushes would also be needed in vanilla tick order.

#### D-082 · inside_wall ignores isSuffocating and uses one algorithm for every release
- **Versions:** all · **Severity:** minor (output flag only) · **Status:** confirmed
- **Vanilla:**
  - 1.19+: `!isAir && isSuffocating && joinIsNotEmpty` on a 1e-6 eye box.
  - 1.16.x: a 0.1-tall box.
  - 1.14 and earlier: an 8-corner `isViewBlocking` / normal-cube test.
  - LivingEntity adds `!isSleeping()`.
  - Sources: `origin/client1.21.11 Entity.java:2192-2203`, `1.16.4 :1610-1621`, `1.14.4 :1661-1680`, `1.8.9 :1245-1265`.
- **Native:** any collision box meeting the 1e-6 eye box (`native/src/simulation.rs:2156-2174`).
- **Impact:** `insideWall` is true inside glass, slabs, fences and similar blocks.
- **todo.md:** 675 `[x]` (deviates)
- **Fix:** deferred — the per-version in-wall predicate (1.8.9-1.13.2) and the 1.16-1.17 border test are in, but chorus_flower is still wrongly in-wall (blocks.ts `legacyInWall` for 1.9-1.12.2, `is_suffocating` fallback from 1.13) and needs a fix with 1.12.2 and 1.13.2 tests.

#### D-083 · Unloaded-chunk semantics cover only shape queries; set_block re-loads the chunk
- **Versions:** all (direct API; the adapter removes cells itself) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** a missing chunk reads as air with an empty fluid for every query, and `setBlockState` on an empty chunk is a no-op (`origin/client1.18.1 ClientChunkCache.java:getChunk`, `EmptyLevelChunk.java:21-31`).
- **Native:** fluids, friction, effects, suffocation and support read stored cells of unloaded chunks, and `set_block` inserts into `loaded_chunks` (`native/src/world.rs:199-201, 504, 533, 674, 681, 1028, 1063, 1201`).
- **Impact:** wrong fluid and friction state for direct N-API callers.
- **todo.md:** 557 `[x]` (partial)
- **Fix:** deferred — writes into unloaded chunks are now dropped as vanilla EmptyChunk does, but the read side (`block_entity`, `block_state_id`, `contains_block`) is not gated on loaded chunks yet, and the adapter change has no unload/reload test.

#### D-084 · Batched metadata-only block update drops extra boxes
- **Versions:** all · **Severity:** minor (direct API only) · **Status:** confirmed
- **Vanilla:** block geometry is fixed per state.
- **Native:** a metadata-only update rebuilds the block from the first box. `set_block` clears `extra_shapes`, `extra_outline_shapes` and `extra_interaction_shapes`. (`native/src/napi_api.rs:1181-1243`; `world.rs:199-204, 444-468`)
- **Impact:** fences, stairs and walls lose collision boxes.
- **todo.md:** 569 `[x]` (deviates)
- **Fix:** fixed — a metadata-only batched update now seeds its entry from the block's full stored geometry (`WorldSnapshot::block_geometry`) or keeps the entry an earlier update created, and is rewritten with all collision, outline and interaction boxes.

#### D-085 · remove_block leaves extra outline and interaction boxes behind
- **Versions:** all · **Severity:** minor · **Status:** confirmed
- **Vanilla:** air has no outline or interaction shape.
- **Native:** `remove_block` clears only `extra_shapes` (`native/src/world.rs:254-275`), and `outline_shapes` / `interaction_shapes` still return the old boxes (`:817, :859`).
- **Impact:** ghost outline and interaction boxes for raycast-style API consumers. No physics effect.
- **todo.md:** 537, 539 `[x]` (partial)
- **Fix:** fixed — `remove_block` now also clears the extra outline and interaction boxes.

#### D-086 · Generated block shape data is empty or wrong from 1.21.5 but reported as exact
- **Versions:** 1.21.5-26.3 (soul sand, honey, mud); all versions for the scaffolding source pools · **Severity:** minor (not used on the player path) · **Status:** partially-confirmed
- **Vanilla:** `SoulSandBlock SHAPE = Block.column(16, 0, 14)`; honey is `Block.column(14, 0, 15)`; scaffolding is a top slab plus legs (`origin/client1.21.11 SoulSandBlock.java:21`, `HoneyBlock.java:30`, `ScaffoldingBlock.java:30-33`).
- **Native:** the generator does not parse `Block.column`, so `generatedExactMethodShapeBoxes` returns `[]` (`native/data/releases/v1_21_11.rs:9583`; `native/data/mod.rs:297-305`).
- **Impact:** wrong results from the generated-shape N-API.
- **todo.md:** 363 (open)
- **Fix:** deferred — the data can only be regenerated from the decompiled trees the generator records by path and sha256, which are not on this machine, and the 1.21.5+ `Block.column`/rotation combinators need a real shape-expression evaluator; hand-editing would desync the data from its generator.

#### D-087 · World-border helper API is not version-shaped
- **Versions:** all (the `>=` vs `>` difference for 1.18-1.20.6) · **Severity:** cosmetic · **Status:** partially-confirmed
- **Vanilla:**
  - `isWithinBounds` uses a strict `>` up to 1.20.6 and `>=` from 1.21.
  - The edges are clamped to absoluteMaxSize.
  - `clampToBounds` returns a BlockPos up to 1.21.1 and `maxX - 9.999999747378752E-6` from 1.21.2.
  - Sources: `origin/client1.20.6 WorldBorder.java:44-57`, `1.21.1 :57-74`, `1.21.2 :82`.
- **Native:** one form, `>=`, unclamped, `1.0e-5` (`native/src/world.rs:350-356, 374-387, 412-423`).
- **Impact:** differs only at exact equality (the border collision check in 1.18-1.20.6 can be affected).
- **todo.md:** 565 `[x]` (deviates)
- **Fix:** fixed — the border helpers take the release and use the clamped edges: strict west/north test before 1.21, block-corner clamp before 1.21.2 (`[min,max-1]` in 1.21-1.21.1), and `max - 9.999999747378752E-6` from 1.21.2; the N-API helpers take an optional release (newest by default).

### 2.8 Entity push and entity colliders (player path)

#### D-088 · Team collision rules are ignored
- **Versions:** 1.9.4-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `EntitySelector.pushableBy(pusher)` applies `collisionRule`: NEVER on the pusher means no pushes; the target rule NEVER, PUSH_OWN_TEAM and PUSH_OTHER_TEAMS are honored on both sides (`origin/client1.21.11 EntitySelector.java:40-70`; `1.13.2 EntitySelectors.java:39-62`).
- **Native:** `push_by_nearby_entities` has no team input, and the adapters send no team data (`native/src/entity.rs:223-276`; `src/mineflayer/player.ts:435-470`).
- **Impact:** on servers with `collisionRule never` (most minigame and PvP servers), native adds up to 0.05 b/t of push per overlapping entity.
- **todo.md:** 678 (open)

#### D-089 · Happy ghast collider ignores staysStill
- **Versions:** 1.21.6-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** an adult happy ghast collides when the player is above `bbox.maxY`, **or** while `isOnStillTimeout()` is true (synced STAYS_STILL: 10 ticks after a dismount, and while a player stands on top) (`origin/client1.21.11 HappyGhast.java:511-521, 487-489`).
- **Native:** always `only_from_above` (`native/src/entity.rs:167-170`; `world.rs:190-197`). The adapter receives `staysStill` but drops it (`src/mineflayer/player.ts:468`).
- **Impact:** the player passes through a still ghast from the side or from below.
- **todo.md:** none

#### D-090 · Pushers that never push the local player in vanilla still push in native
- **Versions:** bat and ender dragon 1.9.4-26.3; vex 1.11.2+; parrot 1.12.2+; spectator players 1.9.4+ · **Severity:** minor · **Status:** confirmed
- **Vanilla:**
  - Bat: `doPush` and `pushEntities` are empty.
  - Parrot: `doPush` skips players.
  - Ender dragon: `aiStep` never pushes.
  - Vex: `noPhysics` is set during its tick.
  - Spectator players: `noPhysics`.
  - `Entity.push` returns early on noPhysics.
  - Sources: `origin/client1.21.11 Bat.java:83-87`, `Parrot.java:306-310`, `EnderDragon.java:145`, `Vex.java:69-72`, `Entity.java:1811-1813`, `Player.java:224`.
- **Native:** all of them are kind "living" (`native/src/entity.rs:263-271`; `mineflayer-physics-utils playerState.ts:26, 95`).
- **Impact:** extra pushes of up to 0.05 b/t.
- **todo.md:** none

#### D-091 · A sleeping or dead player is still pushed
- **Versions:** 1.9.4-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `LivingEntity.push(Entity)`: `if (!isSleeping()) super.push`; `isPushable` requires `isAlive()` (`origin/client1.21.11 LivingEntity.java:2188-2192, 3202-3204`).
- **Native:** no pose or health check (`native/src/entity.rs:231-236`).
- **Impact:** the player in bed, or dying, is pushed out of position.
- **todo.md:** none

#### D-092 · Shulker collider uses the fixed 1×1×1 box
- **Versions:** 1.9.4-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:**
  - The box grows by `physicalPeek` (up to 1.0) opposite the attach face.
  - When the peek grows, entities are moved by MoverType.SHULKER.
  - The shulker collides only while it is alive.
  - Sources: `origin/client1.21.11 Shulker.java:188-240, 421-423`; `1.13.2 EntityShulker.java:258-300`.
- **Native:** a static registry box, with no lift and no alive check (`native/src/entity.rs:163-166`; `playerState.ts:78-80`).
- **Impact:** next to or on an open shulker, the player is neither blocked by the taller box nor lifted.
- **todo.md:** none

#### D-093 · Pusher and collider boxes ignore size-changing state
- **Versions:** slime size all; remote player poses 1.9.4-26.3; scale 1.20.5+ · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** a slime's box is 0.52·size (0.51/0.255·2.04 in older releases). A remote player's box follows its pose (gliding, swimming 0.6, sleeping 0.2, sneaking 1.65 before 1.14), and it is scaled by SCALE. (`origin/client1.21.11 Slime.java:327-329`; `Avatar.java:54-55`; `1.13.2 EntityPlayer.java:335-362`)
- **Native:** registry size, with only baby and crouch (1.14+) adjustments (`mineflayer/lib/plugins/entities.js:9, 209-241, 913-921`; `playerState.ts:78-80`).
- **Impact:** pushes are missed or added near slimes and posed or scaled remote players.
- **todo.md:** none

#### D-094 · Rocket boosts and entity pushes are applied as separate passes
- **Versions:** 1.11.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** each entity runs its whole tick in list order. A pusher that ticks before the rocket pushes the player first, and the boost then halves that push (`0.5v + 0.85·look`). (`origin/client1.21.11 FireworkRocketEntity.java:125-126`; `ClientLevel.tickEntities`)
- **Native:** in each group, all boosts first and then all pushes (`native/src/napi_api.rs:1596-1612`; `entity.rs:304-322`).
- **Impact:** the velocity of a boosted glider overlapping an entity is off by up to 0.025 b/t per entity.
- **todo.md:** 683 `[x]` (deviates)

#### D-095 · 1.14-1.16 "ticks before the player" hash-slot model
- **Versions:** 1.14.4-1.16.5 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Int2ObjectOpenHashMap(32)` holds 24 entries, rehashes on the 25th put, shrinks only below maxFill/4, and slots depend on the insert and remove history (`origin/client1.14.4 MultiPlayerLevel.java:69, 105-145`).
- **Native:** the model grows at `count ≥ size·0.75`, never keeps a grown table, and assumes id-order insertion (`mineflayer-physics-utils playerState.ts:~40-56`; `mineflayer physics.js:2854-2892`).
- **Impact:** at the threshold counts and after despawns, entities are classed wrongly, so the push uses the wrong box or pass and rockets are ordered wrongly.
- **todo.md:** none

#### D-096 · 1.9.4-1.10.2: a player on a ladder is still pushed in vanilla, but native skips it
- **Versions:** 1.9.4, 1.10.2 · **Severity:** minor · **Status:** confirmed (yarn)
- **Vanilla:** `isPushable` = `!removed` in 1.9 and 1.10; the climbing exemption arrives in 1.11 (yarn 1.9.4 and 1.10.2 `LivingEntity.isPushable`; 1.11.2 `:1935`).
- **Native:** `if climbable(world, state) return` for every release from 1.9 (`native/src/entity.rs:234-236`).
- **Impact:** a climbing player keeps its velocity where vanilla adds the push.
- **todo.md:** 692 `[x]` (holds otherwise)

#### D-097 · N-API Entity.push: pushthrough ownership, sqrt precision, pushable gate, threshold and vehicle meaning
- **Versions:** pushthrough and f32 sqrt 1.8.8-1.16.5; pushable gate before 1.19; threshold and vehicle meaning in all versions · **Severity:** minor (N-API only; the client never assigns pushthrough) · **Status:** partially-confirmed
- **Vanilla:**
  - The calling entity's `(1.0F - this.pushthrough)` scales both impulses.
  - `Mth.sqrt` is float before 1.17.
  - The threshold is `0.009999999776482582`.
  - `isPushable` is checked only from 1.19.
  - `isVehicle()` means "has passengers".
  - Sources: `origin/client1.16.4 Entity.java:1219-1251`, `1.8.9 :938-969`, `1.19 :1297-1326`.
- **Native:** per-side pushthrough, an f64 sqrt, `< 0.01`, a pushable gate in every release, and "has a vehicle" treated as vehicle (`native/src/entity.rs:84-128`; `napi_api.rs:1800-1812`).
- **Impact:** wrong push impulses from `simulateVersionedEntityPush/Ticks`; ridden vehicles are pushed and their riders are not.
- **todo.md:** 120 `[x]` (deviates), 681 `[x]` (deviates), 689/691 `[x]` (partial)

#### D-098 · The multi-entity push pipeline runs once after all moves
- **Versions:** all · **Severity:** minor (N-API only) · **Status:** partially-confirmed
- **Vanilla:**
  - The client pushes only the local player, during each pusher's aiStep, in tick order.
  - 1.8.9 pushes only on the server.
  - `isVehicle` means "has passengers".
  - Sources: `origin/client1.21.11 LivingEntity.java:2997, 3061-3088`, `EntitySelector.java:40-56`; `1.8.9 EntityLivingBase.java:1425`.
- **Native:** every overlapping pair is pushed once with end-of-tick positions, and a passenger is treated as the vehicle (`native/src/napi_api.rs:1772-1820`).
- **Impact:** wrong push results from the multi-entity N-API.
- **todo.md:** 683 `[x]` (deviates)

### 2.9 Pistons

#### D-099 · Moving piston blocks have no collision in the native world
- **Versions:** all (1.8.8-26.3). The retracting base cell also loses its collision in 1.8.8-1.14.4; 1.15.x is unverified (between 1.14.4 at flag 3 and 1.16.4 at flag 20). · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:**
  - A MOVING_PISTON block collides as the moved block or head, offset by `getExtendedProgress(progress)`. This uses the SHORT head rule and the extended base for a retracting source.
  - `BlockCollisions` also reads moving_piston in 2-edge cells.
  - The NOCLIP same-direction rule applies.
  - Before 1.11, the box is swept (1.9-1.10) or the BlockPistonMoving box (1.8).
  - Sources: `origin/client1.21.11 PistonMovingBlockEntity.java:342-367`, `BlockCollisions.java:~86`; `1.8.9 BlockPistonMoving.java:110-121`.
- **Native:**
  - `moving_piston` has an empty shape in the data.
  - 2-edge cells are never read (`native/src/world.rs:955-958`).
  - `piston.rs` only pushes the player.
  - `mineflayer/lib/movingPistons.js` places the final block only at the end.
- **Impact:** walking into an extending head, or standing on piston-moved blocks (elevators, flying machines). From 1.13 the client keeps stale static blocks at the old cells while blocks move. From 1.8 to 1.14 the player can fall into the retracting base cell.
- **todo.md:** 988, 1008 (open)

#### D-100 · Blocks pushed or pulled by a piston never push the player (Mineflayer path)
- **Versions:** all · **Severity:** major · **Status:** confirmed
- **Vanilla:** the client runs moveBlocks. Every pushed or pulled block becomes a non-source moving block entity that sweeps its own shape by 0.5 per tick and pushes entities. (`origin/client1.21.11 PistonBaseBlock.java:253-320`; `PistonMovingBlockEntity.java:107-114`; `1.8.9 TileEntityPiston.java:76-105`)
- **Native:** the trackers only model the piston's own head and base, always `source: true` (`mineflayer/lib/movingPistons.js:1-5, 44-66`; `mineflayer-replay/lib/pistons.js:5`). The engine supports `source=false` (`native/src/piston.rs:24-36`), but nothing feeds it.
- **Impact:** a player in front of a pushed block, or on a block elevator, is not moved.
- **todo.md:** 677 (open)

#### D-101 · Adapter gives a retracting piston a SHORT head past 0.25 in 1.13.2-1.15.2
- **Versions:** 1.13.2, 1.14.4, 1.15, 1.15.1, 1.15.2 · **Severity:** major · **Status:** confirmed (javap 1.15.2 vs 1.16)
- **Vanilla:** `getCollisionRelatedBlockState` sets only TYPE and FACING, so SHORT stays false (the long arm reaching -0.25). `SHORT = progress > 0.25F` arrives in 1.16. (`origin/client1.14.4 PistonMovingBlockEntity.java:98-100`; `1.16.4 :94`)
- **Native:** `short = !extending && progress > Math.fround(0.25)` for every release from 1.13 (`mineflayer/lib/movingPistons.js:65`; the same in `mineflayer-replay/lib/pistons.js:94`, so the tests do not catch it).
- **Impact:** a player flush behind a retracting piston is pushed 0.26 blocks in vanilla at progress 0.5, and not at all in native.
- **todo.md:** 677 (open)

#### D-102 · Slime-block piston bounce is missing
- **Versions:** all; 1.9-1.12 unverified (1.8.9 replaces the move only when extending; 1.13.2+ sets the velocity and then pushes) · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** a moving slime block sets the entity's velocity on the movement axis to ±1 (a client LocalPlayer included) and then pushes (`origin/client26.2 PistonMovingBlockEntity.java:111-170`; `26.3 :118-134`; `1.8.9 TileEntityPiston.java:93-105`).
- **Native:** `MovingPiston` carries no block identity, and push only moves the player (`native/src/piston.rs:22-36, 80-152, 229-278`; `napi_api.rs:287-297`; `src/mineflayer/player.ts:51-58`).
- **Impact:** slime launchers do not launch the player.
- **todo.md:** 989 (open)

#### D-103 · Honey-block piston carry is missing
- **Versions:** 1.15-26.3 (the isSupportedBy alternative from 1.20) · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** `moveStuckEntities` carries on-ground entities above a honey block that moves horizontally, by deltaProgress (`origin/client1.16.4 PistonMovingBlockEntity.java:178-204`; `26.2 :187-201, 314`).
- **Native:** only the moveCollidedEntities equivalent exists (`native/src/piston.rs:54-74`).
- **Impact:** a player on a pushed honey block stays put; vanilla carries it 0.5 per tick.
- **todo.md:** 989 (open)

#### D-104 · From 1.21.10, piston moves are still scaled by the stuck multiplier
- **Versions:** 1.21.10-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `if (moverType != MoverType.PISTON) delta = delta.multiply(stuckSpeedMultiplier)`, while still clearing the multiplier (`origin/client1.21.10 Entity.java:729-735`; `26.3 :783-789`; absent in 1.21.9).
- **Native:** `slowed_by_stuck_speed` runs for every piston move (`native/src/piston.rs:334`; `simulation.rs:1749-1757`).
- **Impact:** a piston-pushed player in a cobweb moves 0.25× (0.05× vertically).
- **todo.md:** none

#### D-105 · Piston-driven moves skip inside-block effects, the fall check and the landing bounce
- **Versions:** all · **Severity:** minor · **Status:** confirmed
- **Vanilla:**
  - The PISTON move runs `checkInsideBlocks`, or `applyEffectsFromBlocks(prev, pos)` from 1.21.2.
  - It runs `checkFallDamage`.
  - On landing it runs updateEntityAfterFallOn or restitution (slime/bed).
  - Sources: `origin/client1.21.11 PistonMovingBlockEntity.java:182`, `Entity.java:728-822`.
- **Native:** `move_entity` only collides, sets flags, stops blocked motion and applies the speed factor (`native/src/piston.rs:321-420`).
- **Impact:** cobweb and bubble effects are skipped after a push, a downward push onto slime does not bounce, and fall distance is not updated.
- **todo.md:** 677, 988, 989 (open)

#### D-106 · 1.8: piston push area is the full cell
- **Versions:** 1.8.8 · **Severity:** minor · **Status:** partially-confirmed (the head resets to a full cube, so the main case is slabs)
- **Vanilla:** the push area is the moved block's current bounds box, drawn in by the remaining progress; there is no push for air or a null box (`origin/client1.8.9 BlockPistonMoving.java:154-184`).
- **Native:** `piston.cell()` (`native/src/piston.rs:257-268`).
- **Impact:** a slab moved by a piston pushes players in its empty half.
- **todo.md:** none

#### D-107 · 1.9-1.10 piston push ignores the source flag and the block shape
- **Versions:** 1.9.4, 1.10.2 · **Severity:** minor (N-API only; Mineflayer sends source pistons only) · **Status:** confirmed (yarn)
- **Vanilla:** the swept box is the moved block's own `getCollisionBox` (head plate, unextended base cube, slab and so on) (yarn 1.9.4 `PistonBlockEntity.java:75-149`).
- **Native:** every extending block uses the head plate and every retracting one a full cube (`native/src/piston.rs:180-184`).
- **Impact:** wrong push area and amount for non-source moving blocks.
- **todo.md:** 677 (open)

#### D-108 · Mineflayer advances moving pistons only on ticks where the player is simulated
- **Versions:** all · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `PistonMovingBlockEntity.tick` runs every client tick in `tickBlockEntities` (`origin/client1.21.11 PistonMovingBlockEntity.java:286-318`; `Minecraft.java:~1798-1800`).
- **Native:** `movingPistons.tick` runs only if `playerTicked` (`mineflayer/lib/plugins/physics.js:2298, 2483, 2527-2531`). Vehicle and ridden paths have no piston push at all.
- **Impact:** piston progress lags around riding or dismounting, and controlled vehicles are never pushed.
- **todo.md:** none

### 2.10 Vehicles and ridden mobs

#### D-109 · Boats never run block-inside effects
- **Versions:** cobweb all; bubble column (submerged) 1.13.2+; honey slide 1.15+; powder snow 1.17+ · **Severity:** major (one verifier minor) · **Status:** partially-confirmed
- **Vanilla:** after its move the boat runs `entityInside` for the blocks it touches: once in 1.8.9, twice from 1.13 (Entity.move plus Boat.tick), and `applyEffectsFromBlocks` twice from 1.21.2 (`origin/client1.21.1 Entity.java:738`, `Boat.java:308`; `26.3 AbstractBoat.java:236-237`; `1.13.2 EntityBoat.java:285`).
- **Native:** `move_vehicle(…, |_, _| {})` in both boat paths, and `stuck_speed_multiplier` is reset every tick (`native/src/vehicle.rs:194-201, 627-634, 813-815, 233-246`).
- **Impact:** a boat in a submerged bubble column, a cobweb, powder snow or on honey moves differently.
- **todo.md:** 1033 (open)

#### D-110 · Boats and ridden mobs never collide with entities
- **Versions:** all · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:**
  - A boat's move is blocked by other boats, shulkers and every pushable entity (minecarts, mobs, players), but not its own passengers.
  - Living mounts are blocked by boats and shulkers (and happy ghasts from above).
  - In 1.8.9, boats collide with nearby entity boxes.
  - Sources: `origin/client1.21.1 Boat.java:122-136`, `Entity.java:911, 1981-1987`; `1.8.9 EntityBoat.java:52-58`, `World.java:886-935`.
- **Native:** entity colliders are set only around the player's dispatch (`native/src/napi_api.rs:1599-1603`). The vehicle ticks run with none (`vehicle.rs:866-867`; `napi_api.rs:2430-2479`; `src/mineflayer/vehicles.ts:199-230`).
- **Impact:** a boat or mount drives through other boats, shulkers and (for boats) mobs and players.
- **todo.md:** 1033 (open)

#### D-111 · Boats and ridden mobs always use the slow lava push
- **Versions:** 1.16-26.3 · **Severity:** major (one verifier minor: only flowing lava) · **Status:** partially-confirmed
- **Vanilla:** every pushed entity uses 0.007 in an ultraWarm / FAST_LAVA dimension (`origin/client1.16.4 Entity.java:997`; `26.3 :1686-1706`; Strider has no isPushedByFluid override).
- **Native:** hard-coded 0.0023333333333333335; there is no ultra_warm field in the boat or ridden state (`native/src/vehicle.rs:431-434`; `ridden.rs:357-361, 938-944`).
- **Impact:** a strider ridden through flowing Nether lava gets 1/3 of the current.
- **todo.md:** 1047 `[x]` (partial), 1034 `[x]` (partial)

#### D-112 · Ridden horses, donkeys, mules, llamas and camels climb
- **Versions:** 1.8.8 and 1.9.4-26.3 per kind (llama 1.11.2+, trader llama 1.14.4+, camel 1.19.3+); 1.9-1.12 bracketed · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** `AbstractHorse.onClimbable()` (onLadder / isOnLadder) returns false; Camel and AbstractChestedHorse inherit this (`origin/client1.21.11 equine/AbstractHorse.java:917`; `1.14.4 :891`; `1.13.2 :910`; `1.8.9 EntityHorse.java:1343`).
- **Native:** `climbable()` ignores the mob kind. Travel applies the ±0.15 clamp, the -0.15 fall cap, the fall reset and the 0.2 climb, in air and in water (`native/src/ridden.rs:826-840, 871-873`; `simulation.rs:996-1003`). Pig and strider are correct.
- **Impact:** mounts are slowed and climb walls in ladder, vine or scaffolding cells.
- **todo.md:** 1059 (open)

#### D-113 · A sitting or standing-up camel dashes on jump
- **Versions:** 1.19.3-26.3 (camel_husk too) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `LocalPlayer.jumpableVehicle()` requires `canJump()`, which is `!refuseToMove() && super.canJump()`. Otherwise the jump scale is 0 and onPlayerJump is not called. (`origin/client1.21.11 LocalPlayer.java:539-551, 816-842`; `Camel.java:236-238, 253-255`)
- **Native:** `riderJump` and `rider_jump` check only cooldown and onGround (`src/mineflayer/vehicles.ts:510`; `native/src/ridden.rs:1053-1056`). `tick_ridden` dashes without the refuse check (`ridden.rs:436-445`). The plugin also sends START_RIDING_JUMP.
- **Impact:** about 0.6 of vertical hop during the 2.6 s stand-up, and a 55-tick cooldown that removes the sprint bonus.
- **todo.md:** 1072 (open)

#### D-114 · Strider floatStrider grounds the strider only over a still lava source
- **Versions:** 1.16-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `if (isInLava()) { if (isAbove(LiquidBlock.SHAPE_STABLE, blockPosition(), true) && !lavaAbove) setOnGround(true); else scale(0.5).add(0, 0.05, 0); }`. There is no source check, which belongs only to the collision shape. (`origin/client1.21.11 Strider.java:313-322`; `EntityCollisionContext.java:72-74`)
- **Native:** `is_lava_surface` requires `fluid == Lava && fluid_source` (`native/src/ridden.rs:1006-1012`; `vehicle.rs:919-929`).
- **Impact:** a strider on flowing lava bobs up and halves its speed every tick.
- **todo.md:** 1072 (open)

#### D-115 · Boats and ridden mobs use the 1.14+ water scan before 1.13
- **Versions:** 1.8.8-1.12.2 (major); 1.13.2 (own-height rule, negligible) · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** `handleWaterMovement` over the box shrunk by 0.4F vertically and 0.001 elsewhere; the current is the normalized sum of the unscaled flows ×0.014 (`origin/client1.8.9 Entity.java:750-764`, `World.java:1495-1535`; `EntityLivingBase.java:143-146`).
- **Native:** `world.fluid_contact` (the 1.14+ algorithm) for all releases (`native/src/vehicle.rs:416-437`; `ridden.rs:341-366, 929-937`). The player path uses `legacy_fluid`.
- **Impact:** at water surfaces, mounts switch between water and land travel about 0.29 blocks apart, and the current push differs.
- **todo.md:** 1047 `[x]` (partial)

#### D-116 · Before 1.14, ridden-mob isInLava uses a box deflated by only 0.001
- **Versions:** 1.9.4-1.13.2 (1.8.8 is not client-observable) · **Severity:** major (one verifier minor) · **Status:** confirmed
- **Vanilla:** `isMaterialInBB(bb.expand(-0.1F, -0.4F, -0.1F), LAVA)` (`origin/client1.13.2 Entity.java:1140-1141`; `1.8.9 :831-832`).
- **Native:** `has_fluid_block_inside` with a 0.001 deflation (`native/src/ridden.rs:363-365`; `world.rs:582-590`). `legacy_fluid::touching_lava` exists but is unused here.
- **Impact:** at lava edges a mount takes lava travel (0.5 drag) where vanilla keeps air travel.
- **todo.md:** 1072 (open)

#### D-117 · 1.8.8: ridden horses and pigs are simulated on the client
- **Versions:** 1.8.8 · **Severity:** major (one verifier minor: the shipped plugin never routes 1.8 mounts here) · **Status:** partially-confirmed
- **Vanilla:** 1.8.9 ridden-mob travel runs only when `!isRemote`; the client interpolates (`origin/client1.8.9 EntityLivingBase.java:1108, 1540`; `EntityHorse.java:1024-1026`).
- **Native:** `tick_ridden_mob` runs full physics for 1.8.8 (`native/src/ridden.rs:232-337`; `napi_api.rs:2419-2479`; `src/mineflayer/vehicles.ts:316-318`).
- **Impact:** predictions from physics that the vanilla client never runs.
- **todo.md:** 1072 (open)

#### D-118 · Boat bubble-column activation and impulse are claimed done but not implemented
- **Versions:** 1.13.2-26.3 (claim); 26.3 observable · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - Up to 26.2, the 60-tick activation and the 2.7/0.6/-0.7 launch run only on the server.
  - 26.3 adds entity events 71/72, and `handleBubbleColumnEffect` runs on the client (down: -0.7; up: y set to 0.6).
  - Sources: `origin/client1.21.1 Boat.java:160-167, 335-372`; `26.3 AbstractBoat.java:286-330`.
- **Native:** `bubble_time` and the related flags are only copied through N-API (`native/src/vehicle.rs:59-62`; `napi_api.rs:3877-3882`).
- **Impact:** 26.3 controlled boats over bubble columns keep their velocity.
- **todo.md:** 1037, 1039 `[x]` (not-implemented)

#### D-119 · Boat getGroundFriction counts lily pads
- **Versions:** 1.9.4-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `!(block instanceof WaterlilyBlock)` is excluded from both the sum and the count (`origin/client1.21.1 Boat.java:487-520`; `1.13.2 EntityBoat.java:480`).
- **Native:** every block whose box meets the slice is counted (`native/src/vehicle.rs:371-413`).
- **Impact:** a boat that touches only a lily pad is ON_LAND (friction 0.6) in native instead of IN_AIR.
- **todo.md:** 1034 `[x]` (partial)

#### D-120 · 1.8.8 legacy boat skips the snow-layer and lily-pad clearing, and collides with lily pads
- **Versions:** 1.8.8 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** before moving, the client boat removes snow layers and destroys lily pads at its four corners, and `BlockLilyPad` adds no box for `EntityBoat` (`origin/client1.8.9 EntityBoat.java:onUpdate`; `BlockLilyPad.java`).
- **Native:** no clearing; generic collision (`native/src/vehicle.rs:607-634`).
- **Impact:** a 1.8 boat is stopped or lifted by snow and lily pads.
- **todo.md:** none

#### D-121 · Boat passenger positioning helper is generic, uses f64 trig, and is not wired in
- **Versions:** all · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - 1.9+: `getPassengerAttachmentPoint` (z 0/0.2F/-0.6F, y height/3F) rotated with float Mth.
  - The rider's yaw is turned by deltaRotation and clamped to ±105°.
  - 1.8.9 uses `updateRiderPosition` (cos/sin·0.4).
  - Sources: `origin/client1.21.1 Boat.java:142-157, 668-727`; `Vec3.java:204-210`.
- **Native:** `passenger_position` rotates a caller-supplied offset with f64 `sin_cos`. `tick_boat_with_passenger` drives the toy `tick_boat`. Nothing calls either. (`native/src/vehicle.rs:115-132`)
- **Impact:** rider position and yaw are not modelled.
- **todo.md:** 410 `[x]` (deviates), 1057 `[x]` (deviates), 1078 `[x]` (partial)

#### D-122 · The unversioned simulateBoatTick / simulateHorseTick are non-vanilla approximations
- **Versions:** all (version-independent API) · **Severity:** minor (not used by the adapter) · **Status:** confirmed
- **Vanilla:** status-dependent friction (0.9/0.45/landFriction), `--deltaRotation` for left, float Mth trig; horse travel through `LivingEntity.travel` (`origin/client1.21.1 Boat.java:587-664`).
- **Native:** fixed drag 0.92/0.8, left `+= 1`, yaw in radians, f64 trig; the horse uses a fixed gravity of 0.08 and drag 0.91/0.8 (`native/src/vehicle.rs:134-162, 1172-1211`; `napi_api.rs:2353-2383, 2463-2468`).
- **Impact:** wrong motion for callers of these exports.
- **todo.md:** 1031 `[x]` (holds for the world path)

#### D-123 · Horse riding jump omits Jump Boost
- **Versions:** all where the client moves the horse (1.9.4-26.3; native also 1.8.8) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** adds `0.1F * (amp + 1)` (`getJumpBoostPower`) (`origin/client1.16.4 AbstractHorse.java:686-689`; `1.20.4 :800-801`; `1.21.1 LivingEntity.getJumpPower:2073`).
- **Native:** `horse_jump` has no boost term (`native/src/ridden.rs:637-663`).
- **Impact:** a horse under Jump Boost jumps lower.
- **todo.md:** 1067 (open)

#### D-124 · Camel refuse-to-move zeroing runs after native's fluid branch
- **Versions:** 1.19.3-26.3 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `Camel.travel` zeroes x/z when `refuseToMove() && onGround()` before `super.travel` picks a fluid branch (`origin/client1.21.11 Camel.java:219-226`).
- **Native:** returns to `travel_in_fluid` first (`native/src/ridden.rs:804-813`).
- **Impact:** a sitting camel in lava drifts with the current.
- **todo.md:** 1072 (open)

#### D-125 · A sitting camel keeps the standing box
- **Versions:** 1.19.3-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** SITTING dimensions use height `2.375 - 1.43` (`origin/client1.21.11 Camel.java:70, 138-139, 604`).
- **Native:** always `(1.7, 2.375)` (`native/src/ridden.rs:62, 212-224`).
- **Impact:** a sitting camel's ceiling collision and fluid contact differ.
- **todo.md:** 1072 (open)

#### D-126 · The camel DASH sync does not start the 55-tick cooldown
- **Versions:** 1.19.3-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `onSyncedDataUpdated(DASH)` sets `dashCooldown = dashCooldown == 0 ? 55 : dashCooldown` (`origin/client1.21.11 Camel.java:573-576`).
- **Native:** `syncDash` sets a cooldown only for the nautilus (`src/mineflayer/vehicles.ts:514-517`).
- **Impact:** after a dash run only on the server, native keeps the sprint bonus and allows another dash.
- **todo.md:** 1072 (open)

#### D-127 · Donkey height is 1.5 in every release
- **Versions:** 1.8.8-1.13.2 (and the 1.8.8 width of every horse: 1.4 in vanilla) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the AbstractHorse/EntityHorse size is 1.3964844×1.6 (1.4×1.6 in 1.8.9); the donkey gets 1.5 from 1.14 (`origin/client1.13.2 AbstractHorse.java:86`; `1.14.4 EntityType.java:514`; `1.8.9 EntityHorse.java:88`).
- **Native:** `Donkey => (1.3964844, 1.5)` with no version gate (`native/src/ridden.rs:66`).
- **Impact:** the ridden donkey's ceiling and fluid contact differ.
- **todo.md:** 1059 (open)

#### D-128 · Skeleton horse 0.96 water drag before 1.13
- **Versions:** 1.8.8-1.12.2 (adapter path 1.11.2-1.12.2) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the 0.96F override (`func_189749_co`) appears in 1.13; earlier releases use 0.8F (`origin/client1.13.2 EntitySkeletonHorse.java:131`; yarn 1.10.2-1.12.2 `getWaterSlowDown` 0.8F).
- **Native:** `slow_down = SkeletonHorse ? 0.96 : 0.8` with no version gate (`native/src/ridden.rs:874-878`).
- **Impact:** about 5× higher swim terminal speed.
- **todo.md:** 1071 (open)

#### D-129 · Ridden mob in water gets the ladder 0.2 lift before 1.14
- **Versions:** 1.8.8-1.13.2 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the water branch has no climbable check before 1.14 (only the 0.3 hop) (`origin/client1.13.2 EntityLivingBase.java:1637-1676`; `1.14.4 LivingEntity.java:1880`).
- **Native:** `if collided_horizontally && climbable { y = 0.2 }` with no version gate (`native/src/ridden.rs:870-873`). The player path gates it at 1.14.
- **Impact:** a mount pushed against a ladder in water rises at 0.2 per tick.
- **todo.md:** 1059 (open)

#### D-130 · 1.9.4-1.10.2 pig boost: the tick the boost ends still adds the boost term
- **Versions:** 1.9.4, 1.10.2 · **Severity:** minor · **Status:** confirmed (yarn)
- **Vanilla:** `if (boostTime++ > total) boosting = false; h += h*1.15F*sin(...)` in the same block; 1.11 moved the check out (yarn 1.9.4 `PigEntity.travel:199-225`).
- **Native:** the 1.11+ order (`native/src/ridden.rs:426-431, 545-549`).
- **Impact:** about 5% speed difference on the last boost tick.
- **todo.md:** 1072 (open)

#### D-131 · Strider runs checkInsideBlocks three times per tick in 1.16-1.21.1
- **Versions:** 1.16.4-1.21.1 (1.16.0-1.16.3 likely) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Strider.checkFallDamage` and `Strider.tick` each call `checkInsideBlocks`, in addition to `Entity.move`; removed in 1.21.2 (`origin/client1.16.4 Strider.java:273-296`; `1.20.4 :297-337`).
- **Native:** applies the effects once (`native/src/ridden.rs:920-926`).
- **Impact:** a strider in a bubble column accelerates about 3× faster in vanilla.
- **todo.md:** 1072 (open)

#### D-132 · 1.11-1.18.2 stickless pig or strider is not client-controlled in the plugin; native's unsteered strider is wrong
- **Versions:** pig 1.11.2-1.18.2, strider 1.16.4-1.18.2 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `getControllingPassenger = getFirstPassenger` until 1.19. A stickless rider still drives the mob on the client: own zza, step 0.5, flyingSpeed 0.02, and a strider resets speed to getMoveSpeed every tick. (`origin/client1.16.4 Pig.java:79-80`, `Strider.java:190, 244-247`, `ItemSteerable.java`)
- **Native:** `mineflayer/lib/plugins/physics.js:1213-1217, 1254` requires the stick from 1.11. Native's unsteered strider uses step 1.0, air speed `ridden_speed*0.1` and a decaying zza (`native/src/ridden.rs:292-298, 420-423, 741-747`); this is latent.
- **Impact:** no client prediction or vehicle move for a stickless rider.
- **todo.md:** 1072 (open)

### 2.11 Projectiles (N-API exports only; the Mineflayer adapter and web client never call them)

#### D-133 · Throwable projectile tick: the 1.21.2+ order is used for every release, with double gravity
- **Versions:** order and hit position 1.8.8-1.21.1 (1.9-1.12 bracketed); float gravity 1.8.8-1.20.4 · **Severity:** major (verifiers split major/minor because only the API is affected) · **Status:** confirmed
- **Vanilla:**
  - ≤1.21.1: the hit test and move use the old velocity, then drag (0.99F, or 0.8F in water), then gravity; position is pos+v even on a hit.
  - 1.21.2+: first-tick bubble column, gravity, inertia, then move.
  - Before 1.20.5, gravity is a float widened to double (0.03F, potion 0.05F, XP bottle 0.07F).
  - Sources: `origin/client1.21.1 ThrowableProjectile.java:39-66`; `1.21.2 :43-62`; `1.20.4 :66-95`; `1.8.9 EntityThrowable.java:188-230`.
- **Native:** gravity, then inertia, then a move by the new velocity, for every version; f64 gravity literals (`native/src/projectile.rs:78-90, 483-533`; `napi_api.rs:2514-2543`).
- **Impact:** snowball, egg, pearl, potion and XP-bottle positions are wrong from tick 1 on 1.8.8-1.21.1.
- **todo.md:** 347 `[x]` (deviates), 1088 `[x]` (deviates), 105/107 `[x]`
- **Fix:** fixed — the ≤1.21.1 order (bubble-column step via blocks_inside from 1.16.2, hit test and move by the old v, then inertia and gravity) is now used before 1.21.2, with float gravity before 1.20.5; from 1.21.2 the bubble columns along the move push the projectile every tick after setPos. The 1.16.2 start was confirmed with javap on the 1.16.1/1.16.2/1.16.3 client jars.

#### D-134 · Hurting projectiles (fireballs, wither skull, dragon fireball)
- **Versions:** order 1.21.2-26.3; normalized accelerationPower 1.21-26.3; hit position 1.8.8-1.21.1; float constants and the wither skull in all versions · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:**
  - From 1.21.2, `applyInertia()` runs before the hit test and the move: `v = (v + normalize(v)*accelerationPower) * inertia`.
  - Before 1.21 the xPower vector is applied after the move.
  - Inertia is 0.95F (0.8F in liquid); a dangerous WitherSkull uses 0.73F.
  - Sources: `origin/client1.21.2 AbstractHurtingProjectile.java:69-113`; `1.21.1 :66-108`; `1.20.6 :100`; `26.2 :96-144`; `WitherSkull.java:41-42`.
- **Native:** always moves with the old velocity, then applies `(v + fixed power) * (0.95 or 0.8 as f64)` (`native/src/projectile.rs:93-102, 493-528`). The 26.x bubble prelude is also applied.
- **Impact:** fireball trajectories drift from tick 1 on 1.21.2+. On 1.21+ the default zero power makes the fireball decay.
- **todo.md:** 109 `[x]` (deviates), 111 `[x]` (deviates)
- **Fix:** fixed — from 1.21.2 inertia comes first (`v=(v+push)*inertia`, then the hit test and the move); before, move by the old v, then inertia; the push is normalize(v)*accelerationPower from 1.21 (new acceleration_power input) and the power vector before; inertia 0.95F / 0.8F in water, 0.73F for a dangerous wither skull.

#### D-135 · Fishing hook, llama spit, firework rocket and shulker bullet run through the throwable pipeline
- **Versions:** all (llama 1.11+, shulker 1.9+) · **Severity:** major (verifiers split major/minor) · **Status:** partially-confirmed
- **Vanilla:**
  - Fishing hook: -0.03 only out of water, off ground and unhooked; then `move(SELF)`, zero on collision, ×0.92 after the move, plus the bobbing state machine.
  - Llama spit: moves by the old v, then ×0.99, -0.06; discarded in water.
  - Firework: boost, then `move(SELF)` with sliding collision; no drag or water inertia.
  - Shulker bullet: no gravity on the client.
  - Sources: `origin/client1.21.11 FishingHook.java:225-238`, `LlamaSpit.java:36-54`, `FireworkRocketEntity.java:139-149`, `ShulkerBullet.java:184-207`.
- **Native:** shared bubble prelude, then gravity, then 0.8F in water or air_drag, then ray-stop (`native/src/projectile.rs:433-533`, defaults :78-103).
- **Impact:** wrong trajectories (e.g. the hook moves `(v-0.03)*0.92` instead of `v-0.03`; a rocket in water slows down).
- **todo.md:** 93 `[x]` (deviates), 101 `[x]` (partial), 1124 `[x]` (deviates), 1127 `[x]` (partial)
- **Fix:** fixed — hook gravity gate at_least(1,21,6) and a 1.21.4+ zero-y move keeps on_ground; 1.13.x hook uses its own water height; llama spit moves before the removal check only before 1.15 and is removed in bubble columns from 1.13 until 1.21.4.

#### D-136 · An arrow in water moves by the water-scaled velocity
- **Versions:** all · **Severity:** major (one verifier minor: API only) · **Status:** confirmed
- **Vanilla:** the move and clip always use the velocity from the start of the tick. Water inertia affects only the next tick (1.21.4+ applies it before the clip, but clips with the captured movement). (`origin/client26.2 AbstractArrow.java:166-253`; `1.21.1 :239-277`; `1.8.9 EntityArrow.java:316-357`)
- **Native:** scales by `water_inertia` and then moves by the result (`native/src/projectile.rs:355-360`). The test at :697-719 asserts this.
- **Impact:** an in-water arrow travels 0.6× the vanilla distance per tick.
- **todo.md:** 1106 `[x]` (deviates)
- **Fix:** fixed — tick_arrow clips and moves by the motion captured at the start of the tick; water inertia only scales the velocity carried to the next tick, and the test now asserts pos=(1,1).

#### D-137 · Trident tick: gravity before drag, water inertia applied twice
- **Versions:** 1.13.2-26.3 (the API also accepts 1.8.8-1.12.2) · **Severity:** major (verifiers split) · **Status:** partially-confirmed
- **Vanilla:** `ThrownTrident` uses the AbstractArrow tick: move by the captured movement, then 0.99 once (water or air), then gravity 0.05 if not in ground. On a hit it pulls back by 0.05 and sticks. (`origin/client26.2 AbstractArrow.java:166-253`, `ThrownTrident.java:222`; `1.13.2 EntityArrow.java`)
- **Native:** water ×0.99, then gravity, then ×0.99 again, then move; on a hit it stops at the exact point (`native/src/projectile.rs:262-309`). The test at :849-860 asserts 0.9801.
- **Impact:** from v=(1,1,0) in water, vanilla gives pos=(1,1,0), v=(0.99,0.94); native gives (0.9801,0.9306).
- **todo.md:** 1120 `[x]` (deviates)
- **Fix:** fixed — tick_trident follows AbstractArrow: move by the captured movement, one 0.99F (water inertia or air drag), then gravity 0.05; a block hit sets pos = hit - signum(v)*0.05F and v=0; the test asserts vanilla pos (1,1,0) and v (0.99F, 0.99F-0.05).

#### D-138 · First-tick bubble column: gated to 26.x, reads one cell only, and clamped
- **Versions:** 1.21.2-1.21.11 (missing); 26.1-26.3 (single cell; clamps that vanilla's Projectile overrides drop from 1.21.5) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `handleFirstTickBubbleColumn` exists from 1.21.2. It iterates every bubble-column cell in `betweenClosed(getBoundingBox())`, and `Projectile.onAbove/onInsideBubbleColumn` are unclamped from 1.21.5. (`origin/client1.21.2 ThrowableProjectile.java:44, 83-95`; `26.2 Projectile.java:149-159`)
- **Native:** enabled only when the version starts with "26.1", "26.2" or "26.3"; reads only `floor(position)`; applies the Entity clamps (`native/src/napi_api.rs:2534-2538`; `projectile.rs:445-472`).
- **Impact:** the first-tick vertical velocity differs.
- **todo.md:** 1085 `[x]` (partial), 1088 `[x]` (deviates)
- **Fix:** fixed — the first-tick step now runs from 1.21.2 over every bubble-column cell in the box (betweenClosed order, sectionBasedScan order from 26.3), unclamped from 1.21.5 except for ender pearls; the stateAbove collision-shape position difference has no effect since emptiness is position-independent.

#### D-139 · Arrow block impact: no 0.05 pull-back, wrong pre-1.21.2 velocity
- **Versions:** all · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - 1.21.2+: position = `hit - signum(v)*0.05`, velocity zero.
  - ≤1.21.1: velocity = hit-pos, pulled back by `normalize*0.05`, then moved, then drag and gravity are still applied.
  - Sources: `origin/client26.2 AbstractArrow.java:536-554`; `1.21.1 :454-470`; `1.8.9 EntityArrow.java:297-305`.
- **Native:** position = the exact hit point, velocity zero (`native/src/projectile.rs:377-402`).
- **Impact:** the resting position of a stuck arrow is off by up to 0.05 per axis.
- **todo.md:** 1106 `[x]` (deviates)
- **Fix:** fixed — new shared arrow_flight(): 1.21.2+ hit - signum(v)*0.05F with v=0; 1.14-1.21.1 v=hit-pos pulled back by normalize*0.05F then moved, dragged and gravity applied; the 1.13.2 and 1.8.9-1.12.2 float forms are reproduced.

#### D-140 · Arrow in-ground fall condition: OR instead of AND
- **Versions:** 1.13.2-26.3 (1.8.8 differs in other ways) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the arrow falls only if `lastState != blockState && noCollision(point.inflate(0.06))`; lastState is the hit block. From 1.21.2 this runs on the server only. (`origin/client26.2 AbstractArrow.java:194-199, 327-329, 537`)
- **Native:** falls if there is no collision within 1e-7 **or** the block id at floor(pos) changed (`native/src/projectile.rs:313-353, 383-391`). The test at :805-846 asserts non-vanilla behaviour.
- **Impact:** an arrow falls when its block is replaced by another solid block.
- **todo.md:** 1115, 1118 `[x]` (deviates), 1110 `[x]` (partial)
- **Fix:** fixed — the arrow falls when the state changed AND nothing collides, with four per-release variants (1.8.9-1.10.2, 1.11-1.13.2 with the hit block's own state via last_block_position and the 0.5 box inflated by 0.05, 1.14-1.15 floor(pos) with the box inflated by 0.06, 1.16+ the point inflated by 0.06); lastState comes from the clip's hit block.

#### D-141 · Arrow inGroundTime and shakeTime are off by one
- **Versions:** all (inGroundTime 1.9+) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** inGroundTime stays 0 on the hit tick; shakeTime is decremented at the top of every tick; the startFalling tick still increments inGroundTime (`origin/client26.2 AbstractArrow.java:186-213`).
- **Native:** sets `in_ground_time = 1` on the hit tick, and on the fall tick resets it without decrementing shake (`native/src/projectile.rs:338-397`).
- **Impact:** the counters exported in the arrow state are off by one.
- **todo.md:** 1111 `[x]` (deviates)
- **Fix:** fixed — shakeTime decrements at the top of every tick; inGroundTime stays 0 on the hit tick, increments on every in-ground tick including the fall tick, and resets in flight.

#### D-142 · Versioned arrow and trident ticks ignore the version
- **Versions:** ordering and water 1.8.8-1.21.3; pull-back form; float gravity 0.05F 1.8.8-1.20.4 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the AbstractArrow tick changed at 1.21.2 and 1.21.4. Before 1.20.5, gravity is `0.05000000074505806`. (`origin/client1.21.1/1.21.2/1.21.4 AbstractArrow.java`; `1.20.4 :258`)
- **Native:** one arrow and one trident tick for every release (`native/src/napi_api.rs:2556-2621`).
- **Impact:** older-release arrow trajectories inherit D-136, D-137 and D-139 plus the gravity epsilon.
- **todo.md:** 1106 `[x]` (deviates)
- **Fix:** fixed — added tick_arrow_in_release / tick_trident_in_release, used by the versioned N-API calls with the parsed release; gravity defaults to 0.05F before 1.20.5 and the versioned trident call rejects releases before 1.13.

#### D-143 · Projectile family resolver misses real entity ids
- **Versions:** 1.8.8 (`ThrownExpBottle`, `FireworksRocketEntity`); 1.11-1.12 (`fireworks_rocket`, unverified); 1.13.2+ (`fishing_bobber`); 1.21.5+ (`splash_potion`, `lingering_potion`); 1.21+ wind charges · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `origin/client1.21.11 EntityType.java` registers fishing_bobber, splash_potion, lingering_potion, wind_charge and breeze_wind_charge; `1.8.9 EntityList.java:240, 245`.
- **Native:** these resolve to Generic and the versioned API errors (`native/src/projectile.rs:134-165`; `napi_api.rs:2636-2639`).
- **Impact:** callers passing registry names cannot simulate these projectiles.
- **todo.md:** 113, 117 `[x]` (partial)
- **Fix:** fixed — fishing_bobber, splash_potion/lingering_potion, fireworks_rocket/FireworksRocketEntity and ThrownExpBottle now resolve; wind_charge and breeze_wind_charge map to a new WindCharge family (hurting tick, zero gravity, inertia 1.0F, outline-shape clip in 1.20.3-1.20.4), available from 1.20.3.

#### D-144 · Projectile block trace lacks VoxelShape.clip's start-inside and short-ray rules
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `lengthSqr < 1e-7 → no hit`; if `from + 0.001*d` is inside the shape, an inside hit at that point (`origin/client26.2 VoxelShape.java:119-131`).
- **Native:** nearest `Aabb::clip_segment` over the boxes (entering faces only) (`native/src/projectile.rs:283-291, 367-375, 501-509`; `math.rs:1509-1553`).
- **Impact:** a projectile that starts inside a block shape passes through it.
- **todo.md:** 1131 (open)
- **Fix:** fixed — the 1.13+ start-inside and short-ray rules are in place; the 1.12.2 test no longer asserts a no-hit as vanilla (pre-1.13 vanilla also hits exit faces, which native does not model; to be logged as a new deviation).

### 2.12 Math and geometry helpers

#### D-145 · 1.8.8 moveFlying turns the yaw into radians with a different float formula
- **Versions:** 1.8.8 (1.9-1.12 unknown; 1.13.2+ correct) · **Severity:** minor (verifier corrected this from major) · **Status:** partially-confirmed
- **Vanilla:** `MathHelper.sin(rotationYaw * 3.1415927F / 180.0F)` (two roundings); only `jump()` uses `*0.017453292F` (`origin/client1.8.9 Entity.java:func_70060_a:846-847`, `EntityLivingBase.java:1089`).
- **Native:** the adapter sends `f32(yaw*0.017453292F)`, used for moveFlying too (`src/mineflayer/player.ts:246`, `mth.ts:63-65`; `native/src/simulation.rs:1365-1379`).
- **Impact:** about 0.13% of yaws use the adjacent sine-table entry (about 1e-4 relative acceleration error).
- **todo.md:** 432 `[x]` (partial)
- **Fix:** fixed — the engine now receives the yaw in degrees (`SimulationState.yaw_degrees`, NativePlayer input slot `YAW_DEGREES`) and `simulation::yaw_radians` uses 1.8.x's `yaw * (float)PI / 180F` (two roundings, 1.8.9 Entity.java:942-943, HorseBaseEntity.java:1091) before 1.9 and `*0.017453292F` from 1.9 on (1.9.4 Entity.java:1050 through 1.14), for the player's and the ridden mob's moveFlying and the horse jump; 1.9-1.12 were already correct.

#### D-146 · Aabb::ray_intersects pairs cross components with the wrong half-extents
- **Versions:** all (used by the D-003 fall-reset ray) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `|cross.x| > hy|dz|+hz|dy|`, `|cross.y| > hx|dz|+hz|dx|`, `|cross.z| < hx|dy|+hy|dx|` (`origin/client1.21.11 Mth.java:350-381`).
- **Native:** the components are rotated by one position (`native/src/math.rs:1566-1592`); the test uses only a unit cube.
- **Impact:** 1.85% wrong answers on a 4×1×1 box. Vanilla itself uses `level.clip` for this check (see D-003).
- **todo.md:** 490 `[x]` (deviates)
- **Fix:** fixed — `Aabb::ray_intersects` now pairs each cross component with the half-extents as `Mth.rayIntersectsAABB` does (same in 1.17 through 26.3), keeping vanilla's `<=` / `<` boundaries; the helper does not exist before 1.17.

#### D-147 · murmurHash3Mixer constants are wrong; getSeed lacks the int overflow
- **Versions:** all (helpers; unused, and the adapter's own seed is correct) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `*= -2048144789`, `*= -1028477387`; `(long)(x * 3129871)` multiplied in int (`origin/client1.21.11 Mth.java:326-330, 517-524`).
- **Native:** `-2048147789` / `-1028478737`; `i64::from(x)*3129871` (`native/src/math.rs:830-844`). The test asserts the wrong value.
- **Impact:** wrong hashes for any future consumer.
- **todo.md:** 457, 459 `[x]` (deviates)
- **Fix:** fixed — the mixer constants are now `-2048144789` / `-1028477387` (every source from 1.9.4 to 26.3) and `get_seed` multiplies `x.wrapping_mul(3_129_871)` in int and adds with wrapping long arithmetic, as `x * 3129871 ^ z * 116129781L ^ y` does from 1.7 on; the test now asserts Java-computed values.

#### D-148 · createSimulationState fallback look vector uses Math.sin/cos
- **Versions:** all (only when `look` is omitted) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `calculateViewVector` uses Mth table lookups with float products (`origin/client1.21.11 Entity.java:1913-1921`).
- **Native:** correctly rounded f64 sin/cos of the yaw and pitch (`native/src/napi_api.rs:2020-2028`).
- **Impact:** elytra and swimming steering through this API differ by about 1e-4 per component.
- **todo.md:** 432
- **Fix:** fixed — the createSimulationState fallback calls a new `view_vector(pitch, yaw, release)` that uses the release's Mth table and float products `(k*l, -m, j*l)` from 1.13 on (1.13.2-26.3 getRotationVector/calculateViewVector) and the `getVectorForRotation` formula before 1.13 (1.8.9, 1.12.2).

#### D-149 · Public VoxelShape::collide clips box by box
- **Versions:** 1.13.2-26.3 (Rust library API only) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `collideX` walks the merged cell grid, stopping at the first full layer, including internal grid lines (`origin/client1.21.11 VoxelShape.java:203-260`).
- **Native:** folds `clip_axis` over each box (`native/src/voxel_shape.rs:62-71`). The production `BlockVoxelShape::collide` is correct.
- **Impact:** wrong results for a box already inside a multi-cell shape.
- **todo.md:** 495 `[x]` (partial)
- **Fix:** fixed — `union()` is now `Shapes.or` (joinUnoptimized over the merged grid lines, then a port of `BitSetDiscreteVoxelShape.forAllBoxes` merging Z runs, then X, then Y), with the cell walk order gated at 1.18 (Y, X, Z) versus 1.13-1.17 (X, Y, Z), so `collide` sees the merged grid; `union` now takes the release and the tests that expected per-box results were corrected.

#### D-150 · direction_from_rotation (PlayerState::look_vector) is not vanilla
- **Versions:** all · **Severity:** cosmetic (unused) · **Status:** partially-confirmed
- **Vanilla:** `Mth.cos((double)(-yRot*0.017453292F - 3.1415927F))` with the version table and float products (`origin/client1.21.11 Vec3.java:251-257`).
- **Native:** a single rounding of `(-yaw-180)*K`, the modern table and f64 products (`native/src/math.rs:209-217`).
- **Impact:** low-bit differences, or one table step before 1.21.11.
- **todo.md:** 425 `[x]` (deviates)
- **Fix:** fixed — `Vec3::direction_from_rotation(pitch: f32, yaw: f32, release)` computes `-yaw*0.017453292F - 3.1415927F` and `-pitch*0.017453292F` in float, uses the release's Mth table via `mth_sin`/`mth_cos` and widens the float products, as Vec3.directionFromRotation does from 1.8.9 to 26.3; `PlayerState::look_vector` takes the release too.

#### D-151 · Mth integer helpers: log2, floorDiv/floorMod, clamp panic, equal threshold
- **Versions:** all (helpers; the simulation uses `collision::vanilla_equal`, which is correct) · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** `log2(0) = -1`; `Math.floorDiv` and `Math.floorMod`; `clamp(int)` = `min(max(v, min), max)`; `equal` uses `9.999999747378752E-6` (`origin/client1.21.11 Mth.java:103-176, 301-307`).
- **Native:** log2 returns 0, Euclidean division, `Ord::clamp` panics when min > max, `equal_f64` uses 1e-5 (`native/src/math.rs:500-608, 803-805, 924-926`).
- **Impact:** wrong results or panics on negative divisors and inverted bounds.
- **todo.md:** 443-453 `[x]` (partial)
- **Fix:** fixed — `is_power_of_two` now uses `value & value.wrapping_sub(1)`, so `i32::MIN` wraps like Java int arithmetic (Mth.java:294-296) instead of panicking with overflow checks on; the other integer helpers were verified as vanilla and the math tests pass in debug and release builds.

#### D-152 · Float-only Mth/Vec3 helpers are implemented in double
- **Versions:** all (dead code, apart from `fast_inv_sqrt` with non-negative input) · **Severity:** cosmetic · **Status:** partially-confirmed
- **Vanilla:** catmullrom, rotLerpRad (3.1415927F/6.2831855F), triangleWave, approach and lerpInt take float arguments. `Vec3.projectedOn` scales by dot then by `1/lenSq`, and `rotation()` multiplies by 57.295776F. (`origin/client1.21.11 Mth.java`, `Vec3.java`)
- **Native:** f64 versions, one modern table, a fused division (`native/src/math.rs:116-123, 176-225, 552-816`).
- **Impact:** none observable today.
- **todo.md:** 423, 427, 429, 455, 464, 474 `[x]` (partial)
- **Fix:** fixed — catmull_rom, rotation_lerp_radians, triangle_wave, approach, the degree-difference helpers and rotate_if_necessary are now f32, lerp_i32/lerp_discrete floor a float product, `Vec3::x_rot`/`y_rot`/`z_rot` take float radians and the release's table, `Vec3::rotation` returns float degrees from fdlibm atan2/asin, and `projected_on` scales by the dot product before `1/lengthSqr`; expected values were computed by running the vanilla formulas in Java.

### 2.13 Version architecture, N-API surface and tooling

#### D-153 · refreshVersionedPose and generic pose dimensions
- **Versions:** fallback id and float dims 1.14.4-26.3; pose 6/7 table 1.15, 1.15.1, 1.16-1.16.4; poses accepted 1.8.8-1.13.2; no deflate in 1.14.x; SCALE 1.20.5+ · **Severity:** minor (no caller in the workspace; the adapter uses its own path) · **Status:** confirmed
- **Vanilla:**
  - The fallback pose is SWIMMING (id 3).
  - id 6 is DYING through 1.16.5; LONG_JUMPING=6 and DYING=7 from 1.17.
  - Boxes use float dims, scaled from 1.20.5.
  - The fit uses the entity context and entity colliders.
  - SLEEPING eye is 0.2F.
  - Sources: `origin/client1.21.11 Player.java:336-370`, `Pose.java`, `Avatar.java:54-55`; `1.16.4 Pose.java`; `1.14.4 Entity.java:1769-1770`.
- **Native:** falls back to id 1 (FALL_FLYING); `is_pre_long_jumping_version` matches only "1.14.4", "1.15.2" and "1.16.5"; f64 0.3/1.8; always deflates; a context-free block-only fit (`native/src/napi_api.rs:1893-1961, 2043-2090`).
- **Impact:** wrong pose id and box for N-API callers.
- **todo.md:** 356, 359, 708, 710 `[x]` (deviates)

#### D-154 · simulateVersionedTicks ignores rockets, nearby entities and moving pistons
- **Versions:** all · **Severity:** minor · **Status:** confirmed
- **Vanilla:** attached rockets boost and entities push every tick.
- **Native:** the bulk path calls only `versioned::dispatch` (`native/src/napi_api.rs:1698-1725`, compare 1530-1639).
- **Impact:** N bulk ticks differ from N single ticks.
- **todo.md:** 1149 `[x]` (partial)

#### D-155 · Generic N-API state defaults flyingSpeed to 0.02
- **Versions:** all (direct callers only) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Abilities.flyingSpeed = 0.05F` (`origin/client1.21.11 Abilities.java:19`).
- **Native:** `flying_speed.unwrap_or(0.02)` (`native/src/napi_api.rs:675`; `simulation.rs:386`).
- **Impact:** flight is 2.5× too slow when the field is omitted.
- **todo.md:** 1145 `[x]` (holds otherwise)

#### D-156 · versionCapabilities reports water movement efficiency from 1.21.1
- **Versions:** 1.21 · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** 1.21.0 registers and uses WATER_MOVEMENT_EFFICIENCY (`origin/client1.21 LivingEntity.java:309, 2130`).
- **Native:** `at_least(1, 21, 1)`, with a test locking in the wrong value (`native/src/version.rs:40, 146-150`). The 1.21 travel profile is correct.
- **Impact:** only callers of `versionCapabilities`.
- **todo.md:** 21 `[x]` (partial)

#### D-157 · The behaviour contract registry is not consulted by dispatch, and it is circular
- **Versions:** all · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** n/a.
- **Native:** `dispatch` matches the version string only. `contract_for_version` mostly reads back `Profile::rules()` and is used only by one test (`native/src/versioned/mod.rs:126-187, 256-761, 1038-1180`).
- **Impact:** it could not catch D-004.
- **todo.md:** 41 `[x]` (partial)

#### D-158 · Profile fields are dead or overwritten
- **Versions:** all · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** the divisor is 16 in every 1.13+ release (`origin/client1.20.4 LivingEntity.java:2227`).
- **Native:** `water_gravity_divisor` is never read (a literal `/16.0` is used); gravity and step height are overwritten by the config; the Default rules carry 5.0 and 0.98 (`native/src/simulation.rs:1056-1061, 2118-2129, 130-155`).
- **Impact:** none today; future profile edits would have no effect.
- **todo.md:** 285 `[x]` (deviates), 283 `[x]` (non-physics-false)

#### D-159 · physics-breakpoints.json is stale
- **Versions:** manifest · **Severity:** cosmetic · **Status:** confirmed
- **Native:** it is missing the transitions 1.20.6→1.21, 1.21.1→1.21.2, 1.21.3→1.21.4, 1.21.5→1.21.6, 1.21.8→1.21.9, 1.21.9→1.21.10, 1.21.11→26.1 and 26.1.2→26.2, and lists non-adjacent ones. It only compares the hashes of 3 methods (`generated/physics-breakpoints.json`; `scripts/report-physics-breakpoints.cjs`).
- **Impact:** wrong documentation.
- **todo.md:** 337 `[x]` (non-physics-false)

#### D-160 · The claim that TypeScript validates the version before constructing an engine is false
- **Versions:** all · **Severity:** cosmetic · **Status:** confirmed
- **Native:** `NativePhysics`, `NativeBoatPhysics` and `NativeRiddenMobPhysics` construct for any release. Rust rejects unsupported versions only at the first tick (`src/mineflayer/base.ts:60-65`, `player.ts:117-120, 319`; `native/src/napi_player.rs:129-133`).
- **Impact:** the error appears later than documented. No physics effect.
- **todo.md:** 1158 `[x]` (non-physics-false)

---

## 3. Disputed

These are claims on which verifiers, or auditors, reached opposite conclusions. Neither is counted among the 160 deviations.

### X-1 · Client unloaded-chunk sink (-0.1) limited to releases before 1.14
- **Claim:** `LivingEntity.travel`/`travelInAir` keeps `else if (isClientSide && !hasChunkAt(posBelow)) vy = y > minY ? -0.1 : 0` through 26.3. Native applies it only before 1.14 (`native/src/simulation.rs:2716-2725`).
- **Against (3 verifiers):** the client level's `hasChunk(int,int)` returns true in every release from 1.14 (`origin/client1.14.4 MultiPlayerLevel.java:232`; `ClientLevel.java` in 1.16.4:220, 1.20.4:312, 1.21.11:381, 26.3:490), and `LevelReader.hasChunkAt` only forwards to it. So the branch cannot run on a 1.14+ client, and the pre-1.21.2 `LocalPlayer.tick` chunk guard can never fail. Native's pre-1.14 gate is correct (1.8.9 and 1.13.2 check `Chunk.isLoaded()`).
- **For (1 verifier, partially-confirmed major):** in 1.21.2-26.3 the `LocalPlayer.tick` chunk guard is gone, so the sink would apply after teleports. It also argued that 1.8.8-1.21.1 lack the tick-freeze while the player's column is unloaded (`origin/client1.8.9 EntityPlayerSP.java:104-105`). That verifier did not address the `hasChunk → true` override.
- **Assessment:** the refutation evidence is direct and specific. For 1.14+ this is most likely no deviation. The pre-1.14 tick freeze (vanilla `EntityPlayerSP.onUpdate` skipped while `isBlockLoaded` is false) would be a gate in the Mineflayer plugin, not in the engine; it is left unresolved.

### X-2 · Piston-driven moves of the local player skip `updateAutoJump`
- **For (gap-pistons auditor, partially-confirmed minor):** `LocalPlayer.move` runs `updateAutoJump` after every move, piston moves included (`origin/client1.21.11 LocalPlayer.java:912-920`). Native's piston path (`native/src/piston.rs:321-368`) never does. This is observable only for a **downward** push that leaves the player on the ground.
- **Against (gap-autojump auditor, refuted):** `canAutoJump` requires `onGround`. Horizontal and upward piston moves always clear onGround, and for a downward push the moving piston block fills the head cell or the cell above it, so the ceiling check returns early. The vanilla call is therefore always a no-op (verified on 1.13.2, 1.21.11 and 26.3).
- **Assessment:** the refutation covers the downward case with a concrete ceiling argument. Most likely no observable deviation.

---

## 4. todo.md claims that do not hold

### 4.1 Verdict counts (all 654 items the auditors assessed)

| Verdict | Count |
|---|---|
| open-correctly (unchecked and still open) | 225 |
| matches-vanilla | 128 |
| non-physics-true | 123 |
| partial | 92 |
| deviates | 52 |
| non-physics-false | 24 |
| unverifiable | 8 |
| not-implemented | 2 |
| **total** | **654** |

414 of the assessed items are checked `[x]`.
- **Hold (252):** matches-vanilla 123, non-physics-true 121, unverifiable 8.
- **Do not hold (162):** deviates 50, partial 86, not-implemented 2, non-physics-false 24.
- **Unchecked items whose code is already further along than the box says:** 995, 996, 1003, 1004 and 1020 (match vanilla) and 259 and 1151 (true).
- **Unchecked items whose code deviates or is only partial:** 994, 1005, 1006, 1009, 1016, 1019, 1022, 1023.

### 4.2 Checked `[x]` items that do not hold (162)

| Line | Item (gist) | Verdict | Note |
|---|---|---|---|
| 21 | WATER_MOVEMENT_EFFICIENCY modelled separately | partial | Travel is correct, but `versionCapabilities` says 1.21.1 (D-156) |
| 24 | Differential tests for every release | non-physics-false | No loop over all releases |
| 26 | Efficiency/drag/friction keep modifier resolution | partial | op2 formula and order wrong; speed/jump drop op1/op2 (D-064, D-066) |
| 29 | Same forwarding for generic EntityState | deviates | Descriptor drops efficiency and the modifiers (D-068) |
| 34 | Accept compact attributes; keep operation order | deviates | Compact objects throw (D-067); op2 form (D-066) |
| 41 | Per-version behaviour contract before dispatch | partial | Never consulted; circular (D-157) |
| 93 | Firework tick uses the Java ordering | deviates | Shared gravity/inertia and clip-stop (D-135) |
| 101 | llama_spit, shulker_bullet use Java gravity/drag | partial | Shulker bullet gravity is server-only (D-135) |
| 105 | Potion/XP-bottle gravity defaults | partial | Pre-1.20.5 values are floats; order (D-133) |
| 107 | Fishing hook gravity, fireballs 0 | deviates | Hook gravity is conditional and drag comes after the move (D-135) |
| 109 | Fireball/hook/firework inertia | deviates | 0.95F/0.8F floats, wither skull 0.73F, hook order (D-134) |
| 111 | Hurting power applied after the move | deviates | Before the move from 1.21.2; normalized power from 1.21 (D-134) |
| 113 | Modern ids normalize | partial | fishing_bobber, splash/lingering_potion, wind charges missing (D-143) |
| 117 | Legacy and prefixed ids | partial | fireworks_rocket, ThrownExpBottle missing (D-143) |
| 120 | Entity.push independent pushthrough per side | deviates | Vanilla uses the calling entity's value for both sides (D-097) |
| 131 | Modern fall-power routed to the 26.x profiles | deviates | Also 1.20.5/1.21+; 1.21.5 Ceil; 1.20.6 legacy (D-004) |
| 134 | Legacy landing damage | partial | Subtracts the safe distance as well; adapter threshold 6 (D-004) |
| 136 | fallDamageImmune wired to EntityState | non-physics-false | No such wiring |
| 139 | Refresh vertical flags on zero-Y local moves | deviates | Skip applied to every release; vanilla only from 1.21.4 (D-072) |
| 145 | Escape-jump gating with `is_free` | partial | Counts non-touching boxes; no entities or border; no scaffolding context (D-013, D-014, D-015) |
| 148 | controlledByLocalInstance gating | partial | Does not gate checkFallDamage from 1.21.2 (D-072) |
| 153 | Apply safe-fall/immune/bounciness back | non-physics-false | No apply-back exists |
| 157 | Block-fall callback gated on local control | partial | Semantics match no vanilla hook (D-026) |
| 160 | Fall-damage-resetting ray, reset-before-accumulate | deviates | Collision box, no water, skips accumulation, no gate or cap (D-003) |
| 163 | fallDamageResetting at the boundary | partial | The adapter never sets it (D-003) |
| 165 | Post-move water guard in checkFallDamage | deviates | Not implemented for 1.21.5+ (D-001) |
| 169 | maybeBackOffFromEdge branch | deviates | 1.21.5 shape for every release from 1.14 (D-012) |
| 171 | 26.2 restitution, living/non-living | partial | Keyed on is_player; honey tag not sent (D-021) |
| 176 | Typed block-fall callback event | partial | Fires on every grounded tick, in every release (D-026) |
| 179 | 26.1/26.2 profiles, floor rounding | partial | Floor damage still misses the 1.21.5 order (D-001) |
| 204 | 1.20.2/1.20.3 TS version classes | non-physics-false | There is no TS per-version class or registry |
| 206 | 1.20 dedicated TS version | non-physics-false | Same as 204 |
| 208 | Landing rounding as an explicit policy | partial | Policy wrong for 1.20.5-1.21.5 (D-004) |
| 210 | 1.21.x floor policy | partial | Breakpoint is 1.21.5; native 1.21.5 uses Ceil (D-004) |
| 219 | Validate the landing policy against the source | deviates | Script never compares with the Rust profile |
| 236 | Version list from Mojang metadata | non-physics-false | Enumerates local decompiled directories |
| 239 | Diff mappings/registries/protocol | non-physics-false | Text hashes of 3 methods only |
| 249 | Public engine class per version | non-physics-false | None in TS |
| 252 | Native/TS registry kept in sync | non-physics-false | No TS registry |
| 281 | Compile-time version registry | non-physics-false | None in `src/` |
| 283 | Extract gravity/step per snapshot | non-physics-false | The generator was removed; config overrides the values (D-158) |
| 285 | Water fluid-falling divisor extracted | deviates | Never read; literal `/16` is used (D-158) |
| 287 | Validate divisors | non-physics-false | No generator or validation |
| 301 | Separate split fluid handlers | partial | One path; only the friction modifiers are toggled |
| 309 | Ultra-warm state and 0.007 lava | partial | The adapter never sets it (D-027) |
| 330 | Package excludes decompiled sources | non-physics-false | `npm pack` ships `generated/physics-methods.json` |
| 337 | Breakpoint report | non-physics-false | Stale (D-159) |
| 339 | Audit every registered release | non-physics-false | 1.8.8-1.13.2 not audited |
| 346 | Track Boat/AbstractHorse methods | non-physics-false | Boat methods are `[]` from 1.21.2; AbstractHorse is not tracked |
| 347 | Source-backed generic projectile tick | deviates | Unversioned (D-133, D-138) |
| 356 | Pose dimensions and eye heights | deviates | SLEEPING and DYING eye heights (D-058, D-153) |
| 359 | Historical DYING ordinals | deviates | 1.15-1.16.4 wrong; the todo text itself says "7 from 1.16" (D-153) |
| 388 | TickInputSnapshot is complete | partial | Leaves out boosts, pushes, pistons |
| 389 | TickOutputSnapshot | partial | Lacks pushes and piston deltas |
| 393 | Clone/merge keep gravity/step/no-gravity | partial | No no-gravity field |
| 395 | Import the no-gravity flag | non-physics-false | Not read by the adapter (D-057) |
| 397 | Reject bad gravity/step at the boundary | non-physics-false | Descriptor path only (D-069) |
| 410 | Passenger ordering and rotated offsets | deviates | Toy helpers, not vanilla (D-121) |
| 423 | Vec3 helpers | partial | x/y/zRot use the modern table for every release (D-152) |
| 425 | Direction vector from pitch/yaw | deviates | Rounding, table and products (D-150) |
| 427 | rotateClockwise90/projectedOn | partial | projectedOn uses a fused division (D-152) |
| 429 | Offsets/closerThan/rotation | partial | rotation() uses f64 (D-152) |
| 432 | Mth sine tables and degree conversion | partial | 1.8.8 moveFlying formula (D-145) |
| 439 | Normalize 1.0E-5 threshold | deviates | Production uses 1e-4 for every release (D-034) |
| 443 | lfloor/clamp/positiveModulo… | partial | Clamp panic, rem_euclid (D-151) |
| 445 | clampedLerp/equal/round | partial | equal_f64 uses 1e-5 (D-151) |
| 449 | log2/approach | partial | log2(≤0)=0; approach in f64 (D-151, D-152) |
| 451 | floorDiv/pack | partial | div_euclid (D-151) |
| 453 | positiveCeilDiv… | partial | div_euclid (D-151) |
| 455 | catmullrom/rotLerpRad/triangleWave | partial | f64 instead of float (D-152) |
| 457 | Block seed helpers | deviates | No int overflow (D-147) |
| 459 | Binary search, MurmurHash3 mixer | deviates | Wrong constants (D-147) |
| 464 | fastInvSqrt helpers | partial | Logical shift (negative inputs only) |
| 474 | sqrt/abs/length helpers | partial | length_2_f32 is version dependent |
| 490 | Ray-vs-AABB test | deviates | Axis pairing (D-146) |
| 495 | VoxelShape.collide with sub-1e-7 handling | partial | Box by box, not the grid (D-149) |
| 500 | Non-empty shape test | partial | No <1e-7 empty rule |
| 509 | collide sub-1e-7 cutoff | partial | Per box; the 1.14-1.16 path is missing (D-074, D-075) |
| 520 | Versioned block ids validated | partial | Batched `stateVersion` is only parsed |
| 537 | Multi-box outline shapes | partial | Ghosts after removal (D-085) |
| 539 | Interaction geometry | partial | Ghosts after removal (D-085) |
| 557 | Loaded/unloaded chunk semantics | partial | Shape queries only (D-083) |
| 565 | Source-shaped border helpers | deviates | Not version-shaped (D-087) |
| 569 | Batch updates, metadata-only merge | deviates | Extra boxes dropped (D-084) |
| 587 | Generated data for supported releases | partial | None for 1.8.8-1.13.2 |
| 593 | Generated tables for every release | partial | 1.14.4-26.3 only |
| 595 | Same as 593 | partial | 1.14.4-26.3 only |
| 601 | Every state domain resolved | non-physics-false | creaking_heart `state_count: None` (1.21.2/1.21.3) |
| 648 | Shape tables for all releases | partial | 1.14.4-26.3 only |
| 664 | Keep vertical flags on zero-Y moves | deviates | All releases; vanilla only from 1.21.4 (D-072) |
| 675 | Eye-box inside-wall detection | deviates | No isSuffocating; one algorithm (D-082) |
| 679 | Pairwise push op | partial | Carries the D-097 deviations |
| 681 | Legacy pushthrough selection | deviates | Per side instead of the calling entity's (D-097) |
| 683 | Multi-entity push pipeline | deviates | Order; passenger treated as vehicle (D-094, D-098) |
| 689 | Double-precision push normalization | partial | Float sqrt before 1.17 (D-097) |
| 691 | Threshold and pushability gates | partial | f64 0.01; pushable gate before 1.19 (D-097) |
| 699 | checkFallDistanceAccumulation thresholds | deviates | Pre-1.19.3 rule differs (D-006) |
| 701 | Slow Falling/Levitation reset before the move | deviates | Only correct from 1.20 (D-007) |
| 704 | Safe-fall-distance handling | deviates | 1.20.6 legacy; N-API default 0 (D-004) |
| 706 | Landing damage rounded up | deviates | 1.21.5 should floor; epsilon (D-004) |
| 708 | Versioned pose-refresh op | deviates | Fallback id, block-only fit, eye heights (D-153) |
| 710 | Reject unavailable poses | deviates | Pre-1.14 accepted; 1.15-1.16.4 table (D-153) |
| 720 | Sprint gating moved out of the engine | partial | The adapter's sprint logic deviates (D-045 to D-048) |
| 730 | No narrowing of the amplifier byte | deviates | Negative pre-1.20.5 amplifiers lost (D-063) |
| 744 | Pose refresh in the tick | partial | Adapter-side; runs twice and early from 1.17 (D-040) |
| 749 | Passenger bypasses the pose fit | partial | Native only; the adapter lacks it (D-044) |
| 751 | Desired-pose priority incl. spin attack | deviates | No SPIN_ATTACK; old order (D-044) |
| 761 | Adapter imports no-gravity | non-physics-false | Never read (D-057) |
| 766 | setDeltaMovement finite guard | partial | Applied to every release (vanilla only from 1.21.11) |
| 768 | addDeltaMovement guard | partial | Same as 766 |
| 774 | Fall distance/damage bridged | partial | Damage is not on the Mineflayer bridge |
| 782 | Eye-in-water/lava from the surface height | deviates | Sampled at y+height-0.1 (D-037) |
| 791 | Java modifier operation order | partial | op2 formula; packet order (D-066) |
| 794 | Forward the movement-speed attribute | partial | op1/op2 others dropped (D-064) |
| 800 | Forward jump_strength | partial | Base + ADD only (D-064) |
| 804 | Forward safe_fall_distance, default 3.0 | deviates | Adapter 3 on legacy; N-API 0 (D-004) |
| 806 | Movement efficiency lerp | partial | Base factor lacks the water exception (D-018) |
| 816 | Levitation targets and easing | partial | Reset placement; negative amplifiers (D-007, D-063) |
| 818 | Slow Falling gravity cap | partial | Pre-move test from 1.21.2 (D-065) |
| 820 | Preserve the Levitation amplifier | partial | Signed byte lost (D-063) |
| 827 | Soul Speed override / efficiency | partial | Water exception; 1.16/1.16.1 position (D-018, D-025) |
| 843 | Smoke asserts source/falling heights | non-physics-false | The assertions are stale and would fail |
| 860 | Surface height for immersion checks | partial | Native eye check at y+h-0.1 (D-037) |
| 874 | Fluid-falling bypasses | partial | 26.3 flag (D-029) |
| 885 | Swim head lookup at Y+0.9 | deviates | Vanilla uses (y+1.0)-0.1 and any fluid (D-030) |
| 891 | Fluid-falling breakpoint | deviates | Breakpoint is 1.20.5; the 26.3 profile is wrong (D-029) |
| 894 | 1.18.2/1.19 fluid-falling fixtures | non-physics-false | No such fixture |
| 901 | Swim steering constants | partial | Head test (D-030) |
| 908 | Bubble-column surface from air occupancy | partial | The adapter omits non-colliding blocks (D-033) |
| 915 | no_gravity in fluid falling | partial | 26.3 flag (D-029) |
| 933 | Soul sand speed factor ordering | partial | Water/bubble exception missing (D-018) |
| 939 | Slime sneak, scaling, damping | partial | is_player scaling; 1.8 has no 0.8 (D-021) |
| 951 | Weaving multiplier | partial | The adapter never sets it (D-022) |
| 956 | Ladders, vines, scaffolding | deviates | Flying exemption from 1.21.4 (D-019) |
| 957 | Powder snow stuck multiplier | partial | 1.21.2+ final box only (D-020) |
| 959 | Sweet berry stuck multiplier | partial | 1.21.2+ final box only (D-020) |
| 962 | Powder-snow immunity state | partial | Matches vanilla but not the item text |
| 966 | Freeze-tick accumulation | partial | Pre-move box; server-side in vanilla (D-024) |
| 973 | Cactus/campfire/magma contact damage | partial | Magma gate (D-023) |
| 978 | Fire clears freeze | partial | Lava and lava cauldron also clear it; pre-1.21.2 burning (D-024) |
| 982 | Inside-block effects over the whole bounds | partial | Pre-move box (D-024) |
| 984 | 1.0E-5 deflation | partial | Vanilla value only from 1.21.2 (D-024) |
| 1034 | Boat status/buoyancy through the snapshot | partial | Lily pads; lava scale (D-119, D-111) |
| 1037 | Boat bubble-column state and impulse | not-implemented | Fields only copied (D-118) |
| 1039 | 2.7 launch for player passengers | not-implemented | Not present (D-118) |
| 1047 | Horse water state from the snapshot | partial | 1.14+ scan everywhere; lava scale (D-111, D-115) |
| 1055 | UNDER_WATER status tests | non-physics-false | No such test |
| 1057 | Boat passenger positioning | deviates | Generic f64 helper, unwired (D-121) |
| 1078 | Vehicle then passenger order | partial | Uses the toy boat tick (D-121) |
| 1085 | 26.2 first-tick bubble column | partial | One cell only; clamps (D-138) |
| 1088 | Bubble column gated to 26.1+ | deviates | Exists from 1.21.2; no legacy order (D-133, D-138) |
| 1090 | Mth.clamp NaN propagation | partial | Only 1.19.4+ semantics; unused |
| 1106 | Arrow tick with 26.2 ordering | deviates | Water move; pull-back; unversioned (D-136, D-139, D-142) |
| 1110 | Arrow life/despawn | partial | Keep/fall condition; server-only from 1.21.2 |
| 1111 | inGroundTime/shakeTime lifecycle | deviates | Off by one (D-141) |
| 1115 | startFalling transition | deviates | OR vs AND (D-140) |
| 1118 | Replacement shapes trigger lastState | deviates | Id at floor(hit) (D-140) |
| 1120 | Trident motion path | deviates | Order; double inertia (D-137) |
| 1124 | Projectile family dispatch | deviates | Non-throwables use the throwable order (D-135) |
| 1127 | Firework acceleration | partial | Inertia and ray-stop after it (D-135) |
| 1149 | simulateVersionedTicks | partial | Drops rockets, pushes, pistons (D-154) |
| 1158 | TS validates the version before construction | non-physics-false | No TS check (D-160) |

---

## 5. Paths verified 1:1

These were checked against vanilla source (1.8.9, 1.13.2, 1.14.4, 1.16.4, 1.18.1, 1.19.x, 1.20.x, 1.21.x, 26.1-26.3) and found to match, apart from deviations listed above.

**Input and movement core**
- Mth sine tables: legacy `(float)Math.sin(i*2π/65536)` with `(int)(f*10430.378F)&0xffff`; modern double index from 1.21.11.
- `getInputVector`/`moveRelative` (1.14+), the float moveRelative before 1.14, and the 1.21.5+ normalized move vector with `modifyInput`.
- Sneak, item-use and swift-sneak scaling per version; the 1.14.x flight `/0.3` restore.
- Ground acceleration: `0.21600002F/f³` from 1.14, `0.16277136F` (≤1.12) and `0.16277137F` (1.13); the 26.2+ `friction > 0.6` branch.
- Air, water and lava travel constants: 0.91F/0.98F, water 0.8F with sprint 0.9F (1.13+); Depth Strider clamp 3, ×0.5 off ground, `/3F` before 1.21 and the attribute from 1.21.0; Dolphin's Grace 0.96F; lava 0.5 and g/4; the shallow-lava branch from 1.16; the fluid-falling adjustment gates (≤1.20.4 noGravity, 1.20.5-26.2 gravity argument).
- Jump power per version: double boost for 1.17.1-1.19.x, the 1e-5 gate from 1.20.5, `max(power, vy)` from 1.21.2; sprint-jump float and double forms.
- Negligible-velocity thresholds: 0.005 (1.8), 0.003, and the player horizontalDistanceSqr < 9.0E-6 from 1.21.5.
- Flying speed: `0.02F + 0.006` in its three forms, 0.025999999F from 1.19.4, abilities flyingSpeed ×2 when sprinting; the y×0.6 carry; flight fall-reset placement before and after 1.21.2.
- Movement-speed modifier iteration order per container type (HashSet before 1.20.5, UUID map in 1.20.5-1.20.6, Identifier map from 1.21); the 1.8 bucket order.

**Collision and stepping**
- Axis order (YXZ/YZX, Direction.axisStepOrder).
- Single-box `collideX` epsilons; the `BlockVoxelShape` grid collide; `findBits`; IndexMerger lines.
- Lifted step-up for 1.14-1.20.6 and candidate step heights for 1.21+.
- Pre-1.14 AABB offsets and legacy step-up.
- `Mth.equal` horizontal collision from 1.14.
- `BlockCollisions` cursor order and edge rule; `findSupportingBlock` distance and compareTo.
- Border collision before 1.18 and isInsideCloseToBorder from 1.18 in the engine.
- Kept bounding box and refreshDimensions refit for 1.14-1.16.
- Pre-1.14 sneak back-off (1.8 drop of -1.0, -stepHeight later) and the 0.05 stepping loops.

**Fluids**
- Legacy `handleWaterMovement`, `isInLava`, the fluid hop and jump for 1.8.9-1.13.2.
- The 1.13.2 and 1.14+ fluid-height push (average/normalize, height-scaled flow below 0.4, 0.0045 minimum from 1.16, float length before 1.17).
- 26.1+ EntityFluidInteraction.
- Bubble-column constants and the 1.21.5+ "nothing above" rule.
- FlowingFluid `getHeight`/`getOwnHeight` and the adapter's flow arithmetic; the legacy BlockLiquid flow vector.
- `getFluidJumpThreshold`; `goDownInWater` gates.

**Blocks and effects**
- Friction table and friction position per version.
- Slime damping and bounce; bed bounce.
- 26.2/26.3 restitution.
- Cobweb, berry-bush and powder-snow constants; powder-snow and scaffolding collision context.
- Honey slide; the soul sand/honey 0.4 and honey jump 0.5 factors; legacy soul sand.
- Climbing clamp, sneak hold and 0.2 climb (apart from D-019); trapdoor ladders.
- Freeze cadence; campfire and fire damage values.
- Levitation easing and placement; the Slow Falling cap value; Speed and Slowness from synced modifiers.

**Elytra, sprint and auto-jump**
- Elytra glide math per era (float or double lift, look factor), apart from the fall-distance and climbable issues.
- Attached-rocket boost formula.
- Auto-jump gates, the inverse-length helper per version, the look-ahead, side lines, step loop and jump height.
- Riptide push, strength and passenger gate, and the ground lift values.
- `isHorizontalCollisionMinor` from 1.18.
- The 1.21.5+ sprint state machine (apart from D-048); the creative fly toggle; `updatePoses` priority up to 1.21.4 (apart from spin attack).

**Vehicles**
- Boat `floatBoat`, `controlBoat`, getStatus, water level, entering-water snap (noCollision gate from 1.21), tick order, and the 1.8.9 EntityBoat.
- AbstractHorse ridden input, speed, jump, rearing and stand animation.
- Camel speed, dash and cooldown (apart from D-113, D-124 to D-126).
- Nautilus; pig and strider steering per era; strider lava collision shape; mob dimensions and base speeds.

**Projectiles and pistons**
- `AABB.clip`; 1.21.2+ throwable core order; 26.2 default gravities; firework acceleration; arrow in-air order (1.21.5+); trident and arrow constants.
- Piston `moveCollidedEntities` geometry for 1.11+, `limitPistonMovement` ±0.51 and thresholds, `fixEntityWithinPistonBase`, the 1.9-1.10 and 1.8 source-piston algorithms, and the 1.16+ SHORT rule.

**Entity push and colliders**
- Boat push reach and gate; LivingEntity push direction and AABB test.
- Client local-player-only push rule; no client push before 1.9.
- Entity collision query (`expandTowards` inflated by 1e-7).
- Happy ghast from-above rule; same-vehicle exclusion; the 0.05 push impulse in the player tick with the float sqrt switch at 1.17.

**Data and adapter**
- Block offset (`getSeed`, max offset) in the adapter.
- minecraft-data shapes for fences, walls, gates, panes, snow, chests, hoppers, cauldrons, end portal frame, lily pad, farmland, soul sand and honey (modern releases).
- Legacy stairs, door, chest and chorus shapes; 1.8 joins; the legacy normal-cube table; shape state-count alignment except 1.16/1.16.1.
- Every `has_*_shape_method` flag in the 44 generated release files has a matching sha256 (40801 blocks).

## 6. Coverage gaps / not verified

- **Missing vanilla sources:** 1.9.4-1.12.2 (bracketed by 1.8.9 and 1.13.2; some auditors used yarn jars for 1.9.4/1.10.2/1.11.2/1.12.2), 1.15.x, 1.16.0-1.16.3, 1.16.5, 1.17.x, 1.18.2, 1.19.2 and the final 26.1.2 (26.1.2-rc-1 was used). Breakpoints that fall in these gaps are inferred: elytra lift doubles at 1.18.2; minor collision starts at 1.18; the sneak drop changes at 1.11; rocket boosts start at 1.11; the jump-boost float/double change is at 1.17 or 1.17.1; the 1.15 speed-factor water exception.
- **No builds or tests were run.** All findings come from reading code, plus small numeric scripts and a read-only N-API probe (D-013, the smoke-test heights).
- **Not audited in depth:**
  - Generated block data (`native/data/releases`) beyond spot checks.
  - The legacy join and flow tables (`legacy-connections.json`) block by block.
  - mineflayer-physics-utils' `FROM_BOT` state population.
  - The Mineflayer plugin's rider input, seating and llama controllability.
  - Minecart physics (absent; todo items open).
  - Happy ghast flight beyond spot checks.
  - Wind charge, eye of ender and trident loyalty.
  - OctahedralGroup rotation sense.
  - The 1.13 `legacy_colliders` full-cube ordering, beyond D-076.
- **Small differences noticed but not reported:** IndirectMerger 1e-7 dedup tolerance; isFree's 2e-7 vs 1e-7 overlap tolerance; the getOnPos fence fallback for landings; the jump-time `fall_distance = 0`; the `isUnderWater && isInWater` adapter condition; the hard-coded sprintWindow of 7 from 1.21.11.
- **Impact scope.** Many findings affect only:
  - the `fall_damage` output (D-004, D-009, D-010);
  - N-API helpers the Mineflayer path does not call (projectiles, the generic entity descriptor, multi-entity push, `refreshVersionedPose`, the unversioned boat/horse ticks, math helpers);
  - direct-API world features (block-entity shapes, unloaded chunks, metadata updates).

## 7. Appendix: claims checked and refuted

| Claim | Why it was refuted |
|---|---|
| Client unloaded-chunk -0.1 sink only applied before 1.14 (core-tick-order) | `MultiPlayerLevel`/`ClientLevel.hasChunk` always returns true from 1.14, so the branch cannot run; native's pre-1.14 gate is correct (see also X-1). |
| `fall_damage_multiplier` attribute never forwarded | It affects only the `fall_damage` output, which the adapter does not read; vanilla's client uses it only for sounds. A forwarding gap with no physics effect (todo 797 is open). |
| `isHorizontalCollisionMinor` uses the platform `acos` | The only threshold-adjacent input gives identical results under MSVC, fdlibm and correct rounding; every other input is ≥27 ulps from the threshold. |
| `clamp_f64/f32` NaN-max semantics follow 1.19.4+ for every release | The helpers are only called by dead code; no physics output differs. |
| Ridden mob lerps on a teleport before 1.21.2 | The plugin starts interpolation only for ≥1.21.2 (or 1.8.8, which matches 1.8.9); on 1.9-1.21.1 lerpSteps stays 0 and the 0.98 fade matches vanilla. |
| 1.18.2 sneak input scale gated at 1.19 (float vs double 0.3) | Keyboard impulses are only -1, 0 or 1, for which both formulas give the same f32. |
| Adapter cancels an attached rocket's duration when not gliding | The plugin re-arms `fireworkRocketDuration = 1` every tick while an attached rocket exists, so the boost resumes as in vanilla. |
| Piston moves never run `updateAutoJump` (gap-autojump) | `canAutoJump` needs onGround, which piston moves clear, or the moving block fills the ceiling cell; see X-2 for the opposing partial confirmation. |

