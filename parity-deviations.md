# minecraft-physics-native: vanilla parity audit

This report checks the Rust engine `minecraft-physics-native` (branch `mc-physics-demo`) and its TypeScript Mineflayer adapter (`src/mineflayer/*`) against the decompiled vanilla client (`D:/projects/mc-zuri/mc-data/extracted_minecraft_data`, branches `origin/client<ver>`). It also checks whether the `[x]` claims in `todo.md` hold.

Every deviation was checked by an adversarial verifier. When a verifier corrected the description, the version range or the severity, this report uses the corrected version. Paths are relative to the repo root (`native/src/...`, `src/mineflayer/...`). Java paths are given as `origin/client<ver>:<path>`, with `client/net/minecraft/` usually left out.

## 1. Summary

> **Update 2026-11-03:** a second audit against the vanilla sources (section 8) adds D-161 to D-229: 60 confirmed and 9 plausible deviations, 4 of them major (1.21+ attribute keys, the Depth Strider slot, 1.8.8/1.9.4 farmland, trident in-ground state).

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

### Fix status

Each D-xxx and X-n section ends with a **Fix:** bullet. Of the 162 entries (160 deviations and the 2 disputed claims):

| Outcome | Count |
|---|---|
| fixed | 145 |
| not a deviation (refuted by the Java sources) | 2 (D-132, X-1) |
| deferred | 15 |

Deferred, with the reason each one is still open:
- **D-032**: blocksMotion needs per-state Material/BlockState data the repo does not have, and the block-data generator's source tree is missing.
- **D-035**: rejected twice in review and kept as a partial improvement; Strider.fall's extra block-collision pass is still open.
- **D-053**: riptide in rain needs rain level, sky visibility, heightmaps and per-version biome temperature noise.
- **D-073**: ridden mobs and vehicles need supporting-block state carried between ticks.
- **D-080**: vehicle.rs and ridden.rs still use the modern snow shapes and `is_free` before 1.13.
- **D-081**: shulker box shapes need a client-side block-entity animation tracker and time-varying block shapes.
- **D-086**: the block data can be regenerated only from decompiled trees that are not on this machine, and the 1.21.5+ shape combinators need a real evaluator.
- **D-088**: the 1.21.5+ root-owner walk differs for chains of non-player owners.
- **D-092**: shulker peek boxes need a per-shulker client peek tracker and an engine move inside the shulker's tick.
- **D-093**: sulfur_cube sizing, the float rounding of the scale reset, and the 1.9-1.13 `updateSize` rules (mineflayer `entitySize.js`).
- **D-095**: other client-side self-removals need per-entity client tick simulation that mineflayer does not run.
- **D-099**: moving pistons need a moving-block layer in the world and the moving_piston edge-cell rule in the collision loop.
- **D-100**: the trackers need a client-side PistonStructureResolver and per-release push-reaction data.
- **D-110**: vehicle ticks receive no nearby entities, and the adapter's entity kinds lack the data canVehicleCollide needs.
- **X-2**: before 1.13 the auto-jump cancel needs the ceiling probe to see moving-piston cells (D-099).

`todo.md` was reconciled with these outcomes. Every `[parity: …]` tag now names only deferred IDs. The unchecked items 259, 995, 996, 1003, 1004, 1020 and 1151 were re-checked and ticked, and so was 1110 (arrow life, fixed with D-140). Items 54, 57 and 88 are unticked: they claim that package verification and the smoke suite pass, but today `scripts/verify-native-package.cjs` expects 43 release modules where there are 44, and `scripts/native-smoke.cjs` fails at its `fluidHeightAt` assertions.

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
- **Fix:** fixed — `is_shapes_block` (world.rs) takes the strict `intersects` branch only when the box is the block's whole collision shape for the entity, the Shapes.block() object, so a unit cube joined from smaller boxes goes through the merger tolerance; and 1.13.x moves the lowered sneak box as it is, with no 1e-7 deflate (1.13.2 Entity.java:627, 639, 650). Tests: `the_fluid_hop_box_meets_a_shapes_block_cube_strictly_and_a_joined_one_merged`, the 1.13.2 rows of the 1.5e-7 sneak-edge test.

#### D-014 · No-collision checks ignore entity colliders and the world border
- **Versions:** all; the border form differs per era (stone outside the border in 1.8-1.12, the border shape in 1.13-1.17, borderCollision from 1.18) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `CollisionGetter.noCollision = noBlockCollision && noEntityCollision && noBorderCollision`. In 1.8.9, getCollidingBoundingBoxes adds entity boxes. (`origin/client1.21.11 CollisionGetter.java:52-54`; `origin/client1.8.9 World.java:886-935`)
- **Native:** block shapes only. Entity colliders and the border are used only in `collide_with_world` (`native/src/world.rs:905-907, 1058-1075`; `simulation.rs:1785-1797`).
- **Impact:**
  - Sneaking on a boat or shulker over a gap: native freezes the player; vanilla lets it walk to the entity's edge.
  - The fluid hop next to a boat, and the pose fit under a boat or shulker, also differ.
- **todo.md:** none
- **Fix:** fixed — 1.13.2 `can_fall` now offsets the lowered box with no deflate (1.13.2 Entity.java:626-650), so blocks and entity colliders that overlap it by [1e-7, 2e-7) hold the sneaker as in vanilla. Test: `an_entity_collider_the_sneaker_overlaps_by_5e_8_holds_it_in_1_13_2`.

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
- **Fix:** fixed — `Aabb::collided_along_vector(vector, boxes, inset)` (math.rs) uses 0 for 1.21.5-1.21.8 (1.21.5 AABB.java:392) and 1e-7 from 1.21.9 (1.21.9 AABB.java:396); `blocks_inside_moves` (simulation.rs) applies the inside-block effects in visit order on a running copy of the entity, so an earlier fall reset is seen by a later block; the sweet berry bush goes through the same swept visit with its (0.8F, 0.75, 0.8F) multiplier and fall reset. Tests: `a_honey_slide_on_the_way_ends_the_fall_a_later_powder_snow_reads_from_1_21_5` and the berry-bush rows.

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
- **Fix:** fixed — the engine keeps `remaining_fire_ticks`: before 1.21.5 the burning baseTick clears the freeze ticks (1.17 Entity.java:419-439) and fire contact counts the ticks up to ignition; from 1.21.5 igniteForTicks clears them (1.21.5 Entity.java:565-571), and from 1.21.9 fire, lava and the lava cauldron (codes 24-27, its inside shape) apply CLEAR_FREEZE. clearFire is setRemainingFireTicks(0) up to 1.21.8 and min(0, ticks) from 1.21.9 (1.21.5 Entity.java:581-583, 1.21.8 :608-610, 1.21.9 :615-617), so rain or powder snow on a fire-free player of 1.21.5-1.21.8 leaves 0 and its next fire contact clears the freeze ticks at once. Only the count up is clamped to the freeze limit; the count down is max(0, i - 2) (1.17 LivingEntity.java:2546-2552). Tests: `rain_puts_a_fire_free_players_ticks_to_0_before_1_21_9`, `freeze_ticks_over_the_limit_fall_by_two`, `fire_ignition_clears_the_freeze_ticks_at_once_from_1_21_5_and_clear_freeze_from_1_21_9`, `a_lava_cauldron_clears_the_freeze_ticks_as_the_release_applies_it`.

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
- **Fix:** fixed — the adapter's slow inputs now send `ultraWarm = (bot.game.dimension === 'the_nether')`, so the engine's existing 1.16+ `0.007` scale applies (the nether is the only vanilla ultrawarm/FAST_LAVA dimension); a custom ultrawarm datapack dimension is still not seen, because prismarine-registry drops the dimension_type ultrawarm field.

#### D-028 · Adapter keeps the 0.1111111119389534 eye offset in 1.20.5-1.20.6
- **Versions:** 1.20.5, 1.20.6 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `updateFluidOnEyes` uses `getEyeY()` with no offset from 1.20.5; 1.20.4 subtracts 0.1111111119389534 (`origin/client1.20.5 Entity.java:1276`, `1.20.4 :1227`).
- **Native:** `if (!this.lt('1.16') && this.lt('1.21')) eyeY -= 0.1111111119389534;` (`src/mineflayer/player.ts:502`)
- **Impact:** within ~0.111 of the water surface, isUnderWater is true too early or stays true too long. This changes the swim start, the sprint gates and the SWIMMING pose/box.
- **todo.md:** 856 (open)
- **Fix:** fixed — the adapter's eye offset gate is now `lt('1.20.5')` (checked against every source 1.16-1.20.4 and 1.20.5/1.21); a test puts the eyes 1.95 over a source block and is under water in 1.20.4 only.

#### D-029 · 26.3 profile drops the gravity-argument check in getFluidFallingAdjustedMovement
- **Versions:** 26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `if (baseGravity != 0.0 && !isSprinting())` (unchanged from 1.20.5 to 26.3) (`origin/client26.3 LivingEntity.java:2608-2620`).
- **Native:** `fluid_falling_checks_gravity_argument: false` (`native/src/versioned/v-26_3.rs:29`; every other profile from 1.20.5 has true), read at `simulation.rs:1064-1072`.
- **Impact:** with a gravity attribute of 0, a falling player in water or shallow lava is snapped to -0.003 per tick and sinks.
- **todo.md:** 891 `[x]` (deviates; its text also gives the wrong breakpoint, the real one is 1.20.5), 874 `[x]` (partial)
- **Fix:** fixed — the 26.3 profile sets `fluid_falling_checks_gravity_argument: true` like 1.20.5-26.2 (pre-1.20.5 stays false, per the 1.20.4 source); a test with gravity 0 in water asserts velocity.y == 0 for 1.20.5, 1.21.11, 26.2 and 26.3.

#### D-030 · Swim-steering head test: water only, and y+0.9
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** steering applies when any non-empty fluid (lava included) is at `BlockPos(x, (y + 1.0) - 0.1, z)` (`origin/client1.21.11 Player.java:travel:1326`, `1.13.2 EntityPlayer.java:1424`).
- **Native:** `floor(y + 0.9)`, Water or Waterlogged only (`native/src/simulation.rs:2308-2316`, used at :2376).
- **Impact:** with lava at head height, or at y values such as 3.1 or 63.1, vertical velocity differs by `(look.y - vy) * 0.06`.
- **todo.md:** 885 `[x]` (deviates), 901 `[x]` (partial)
- **Fix:** fixed — the swim-steering head cell is now `floor((y + 1.0) - 0.1)` and any non-empty fluid (lava included) counts, as in every source 1.13.2-26.3; tests cover y=3.1, lava at the head and no fluid.

#### D-031 · Adapter requires water at the feet to start swimming before 1.17.1
- **Versions:** 1.13.2-1.17.0 (javap: the check arrives in 1.17.1) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `setSwimming(isSprinting() && isUnderWater() && !isPassenger())`; the `getFluidState(blockPosition).is(WATER)` check first appears in 1.17.1 (`origin/client1.16.4 Entity.java:985-990`; 1.17.1 `Entity.updateSwimming` bytecode 63-76).
- **Native:** `swimming = sprinting && isUnderWater && isWaterish(feet)` for every release from 1.13 (`src/mineflayer/player.ts:562-571`).
- **Impact:** a sprinting player whose eyes are under water but whose feet block has no water starts swimming only in vanilla.
- **todo.md:** none
- **Fix:** fixed — the feet-water check applies only from 1.17.1 (absent in the 1.17 decompile, present in 1.17.1 `Entity.updateSwimming` bytecode 62-79 and 1.18); tests assert swimming for 1.16.5 and 1.17 and not for 1.17.1 and 1.18.2.

#### D-032 · Adapter flow vector approximates blocksMotion and isFaceSturdy
- **Versions:** 1.13.2-26.3 (flattened path) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - Whether to look at the fluid below a neighbour uses Material.blocksMotion (1.13-1.19), `BlockState.blocksMotion()` (1.20-26.2), or the `BLOCKS_FLUID_FLOW` tag (26.3).
  - The falling -6 turn uses isFaceSturdy on the union of the support shape. Leaves are never sturdy from 1.16; in 1.13.2 there is an exception list.
  - Sources: `origin/client1.16.4 FlowingFluid.java:62,115`, `1.20.4 :70,109-117`, `26.3 :61`, `1.13.2 :67,131-143`.
- **Native:** "the neighbour has no collision box" and "one box spans the face" (`src/mineflayer/blocks.ts:151-156, 172, 197-199`).
- **Impact:** wrong water current next to carpets, snow, scaffolding, signs and banners, leaves, stairs, soul sand and similar blocks.
- **todo.md:** 863 (open)
- **Fix:** deferred — the isFaceSturdy half is fixed (`faceFull` implements `Block.isFaceFull` on the per-release support shape: 1.13 isSolidFace exceptions, 1.14-1.15 collision shape without #leaves, 1.16+ getBlockSupportShape overrides); the blocksMotion test stays approximated by "has a collision box", because the per-state Material.blocksMotion/BlockState.blocksMotion data is not in the repo, the block-data generator's decompiled tree is missing, and 26.3's `#blocks_motion` tag has no members in the generated data (using it broke the 26.3 r_free_play replay).

#### D-033 · Before 1.21.5 the bubble-column top treats non-colliding blocks as air
- **Versions:** 1.13.2-1.21.4 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the surface push needs `getBlockState(above).isAir()`; a torch, sign or similar block gives the inside push (0.7/-0.3 caps) and a fall reset (`origin/client1.21.4 BubbleColumnBlock.java:entityInside:49-52`).
- **Native:** `world.is_air_at(above)` is correct, but the adapter never writes non-colliding, non-fluid, effect-less blocks, so they read as air (`native/src/simulation.rs:948-953`; `src/mineflayer/blocks.ts:236-262`).
- **Impact:** native applies the 1.8/-0.9 surface push and keeps the fall distance.
- **todo.md:** 908 `[x]` (partial)
- **Fix:** fixed — the adapter now writes every block other than air, cave_air and void_air with its id and an empty shape, so `is_air_at` (already correct) sees torches and signs; this also corrects the powder-snow visit and llama-spit air tests.

#### D-034 · Vec3.normalize threshold stays at 1.0E-4 after 1.21.2
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `normalize` returns zero only when the length is `< 9.999999747378752E-6`; this threshold arrives in 1.21.2 (`origin/client1.21.2 Vec3.java:71-74`; 1.21.1 uses `< 1.0E-4`).
- **Native:** `normalize` uses `< 1.0e-4` for every release in `push_by_fluid` (`native/src/simulation.rs:629-636, 584-607`). `math.rs::normalized` (1e-5) is unused.
- **Impact:** a slow player in a faint current (impulse length 1e-5 to 1e-4) gets no 0.0045 push.
- **todo.md:** 439 `[x]` (deviates)
- **Fix:** fixed — from 1.21.2 the `normalize` used by `push_by_fluid` returns zero below `f64::from(1.0e-5_f32)`, and the adapter's FlowingFluid.getFlow normalize in blocks.ts uses the same threshold.

#### D-035 · 1.16 and 1.16.1 lava handling while in water
- **Versions:** 1.16, 1.16.1 · **Severity:** minor · **Status:** confirmed (javap)
- **Vanilla:** `updateInWaterStateAndDoFluidPushing` returns early when in water, so there is no lava push and the lava height is 0. `isInLava` is the `isTouchingLava` flag set by `LiquidBlock.entityInside`. The 1.16.2+ rule (always measure, fluidHeight>0) starts in 1.16.2.
- **Native:** the 1.16.2 semantics from 1.16.0 (`native/src/simulation.rs:2192-2201`; `ridden.rs:358-362`).
- **Impact:** an extra lava current and a nonzero lava jump height while in water touching lava.
- **todo.md:** none
- **Fix:** deferred — the behaviour (strider warmth/fall in 1.16-1.16.1, the 1.14 <= r < 1.16.2 piston-move inLava reset, the N-API round trip) was reviewed as matching vanilla, but the fix was rejected twice in review and is kept as a partial improvement; the 1.16.2 bound is bracketed by the 1.16 and 1.16.4/1.16.5 sources, and Strider.fall's extra checkBlockCollision re-applying block effects stays open.

#### D-036 · From 26.1, fluid contact ignores unloaded chunks
- **Versions:** 26.1-26.3 (1.8.8-1.21.11 have no deviation, because the client's hasChunk always returns true) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `EntityFluidInteraction.update` skips the whole scan if any chunk under the box widened by 1 is not loaded (`origin/client26.1 EntityFluidInteraction.java:44, 94-110`).
- **Native:** `fluid_contact` never checks loaded state (`native/src/world.rs:533-579`).
- **Impact:** water and lava state and push at the edge of loaded chunks.
- **todo.md:** none
- **Fix:** fixed — from 26.1 `fluid_contact` and `eyes_in_fluid` return nothing unless every chunk under the deflated box widened by one is loaded (`WorldSnapshot::is_chunk_loaded`), and the adapter checks the same through `NativeWorldSync.chunkLoaded`; earlier releases are unchanged.

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
- **Fix:** fixed — the new `WorldSnapshot::eyes_in_fluid` uses each release's form (isInsideOfMaterial before 1.13; float `y + (h + 0.11111111F)` for 1.13-1.15; `eyeY - 0.11111111F` for 1.16-1.20.4; `eyeY` for 1.20.5-1.21.11; EntityFluidInteraction's inclusive eye column from 26.1) with the eye at y + the float eye height; the boat-passenger exception and 26.3 getHeightForCamera are not modelled in these output-only flags.

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
- **Fix:** fixed — the 1.13.2 claim is refuted by bytecode (1.13.2 is `y + (h + 0.11111111F)`, which the adapter had; the decompiled form is an artefact), and the 1.14-1.15 branch now uses that order; from 26.1 the adapter tests every water cell in the eye column inclusively in double with the loaded-chunk check, and from 26.3 a source under a block with a full bottom support face tops at 1.

### 2.5 Sprint, pose, flight, elytra, auto-jump (engine and adapter)

#### D-039 · Adapter crouches a flying player, so flight with sneak held is slowed to 0.3
- **Versions:** 1.15-26.3 (1.15-1.16.3 inferred; 1.14.x is correct because vanilla 1.14 also scales and then divides by 0.3) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `crouching = !abilities.flying && !isSwimming() && … && (isShiftKeyDown() || …)`, so `isMovingSlowly` is false while flying (`origin/client1.16.4 LocalPlayer.java:603`, `1.21.11 :715`, `26.3 :764`).
- **Native:** `crouching = !swimmingAndNotFlying(state) && …`, which is true while flying (`src/mineflayer/player.ts:573-575, 594-599`). MOVING_SLOWLY then reaches the engine's input scaling (`native/src/simulation.rs:1276-1282, 1300-1302`).
- **Impact:** descending creative or spectator flight moves at ~30% of vanilla horizontal speed. Starting a sprint is refused, and in 1.21.4 an existing sprint is stopped.
- **todo.md:** none
- **Fix:** fixed — From 1.14 the adapter sets `crouching = !flying && !swimming && …` and counts spectator as flying, as vanilla `abilities.flying` does; the range is 1.15-26.3 (confirmed from the 1.15-1.19.2 sources, not inferred), and canStartSprinting and the 1.21.4 shouldStopSprinting, which read crouching, are fixed with it.

#### D-040 · From 1.17 the adapter refreshes the pose before the move
- **Versions:** 1.17-26.3 (1.17.x bracketed by 1.16.4 and 1.18.1) · **Severity:** major (one verifier rated it minor: only geometry-constrained cases change position) · **Status:** confirmed
- **Vanilla:** `updatePlayerPose()` is the last call in `Player.tick`, after the move. Nothing else on the client sets the pose. (`origin/client1.21.11 Player.java:249-274`, `1.18.1 :233/276`, `26.3 :274`)
- **Native:** `if (gt('1.13.2') && !lt('1.17')) this.updatePoses(state, sync)` before the native tick (`src/mineflayer/player.ts:211-212`). The box and eye height sent to native then belong to the new pose.
- **Impact:** on the first tick of sneaking, swimming or gliding (start or stop), the move uses the new box height, so ceiling, 1-high gap, fluid contact and inside_wall results differ for one tick.
- **todo.md:** 406 (open), 744 `[x]` (partial)
- **Fix:** fixed — The pre-move `updatePoses` call for 1.17+ is removed, so the pose is refreshed only after the move, as `updatePlayerPose` is the last call in `Player.tick` (1.17 and 1.18 sources).

#### D-041 · From 1.21.5, gliding into a climbable block neither stops the glide nor switches to air travel
- **Versions:** 1.21.5-26.3 (from 1.21.11, `#can_glide_through` blocks are exempt) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `travelFallFlying`: `if (onClimbable()) { travelInAir(input); stopFallFlying(); }` (`origin/client1.21.5 LivingEntity.java:2273-2276`, `26.3 :2499-2502`).
- **Native:** `gliding = fall_flying && fluid Empty`. The climbable clamp, climb and drag are skipped while gliding, and neither the engine nor the adapter clears `fall_flying`. (`native/src/simulation.rs:2464-2477, 2706-2713`; `src/mineflayer/player.ts:284`)
- **Impact:** an elytra flight into a ladder or scaffolding keeps elytra physics and the 0.6 box.
- **todo.md:** 1022 (open)
- **Fix:** fixed — From 1.21.5 a fall-flying entity on a climbable block (no fluid) travels as in air (input, 0.15F clamp, gravity, drag, climb-up) and fall flying is then cleared; the new block effect `GlideThroughClimbable` exempts `#can_glide_through` blocks from 1.21.11 (26.x tag data assumed to match), and a new FALL_FLYING output flag lets the adapter clear `state.fallFlying`.

#### D-042 · 1.8.8: flying while sneaking undoes the 0.3 sneak slowdown
- **Versions:** 1.8.8 (1.9-1.12 unknown; the undo exists in 1.13.2) · **Severity:** major · **Status:** confirmed
- **Vanilla:** 1.8.9 `EntityPlayerSP.onLivingUpdate` only changes motionY for flight sneak and jump; the impulses stay at ×0.3 (`origin/client1.8.9 EntityPlayerSP.java:561-569`; `1.13.2 :760-766` adds `/0.3D`).
- **Native:** `if flying && sneaking && !at_least(1,15,0) { input /= 0.3 }` (`native/src/simulation.rs:1286-1288`).
- **Impact:** sneak-descending flight on 1.8.8 is about 3.33× faster horizontally.
- **todo.md:** none
- **Fix:** fixed — The sources show the /0.3 undo in 1.9.4-1.14 and not in 1.8.9 or from 1.15, so the gate is now `at_least(1,9,0) && !at_least(1,15,0)` and only the 1.8.8 profile changes (1.9.0-1.9.3 bracketed by 1.8.9 and 1.9.4).

#### D-043 · Auto-jump obstacle scan leaves out entity colliders
- **Versions:** 1.10.2-26.3 (1.13.2-26.3 verified; 1.10-1.12 bracketed) · **Severity:** major · **Status:** partially-confirmed (minecarts are never colliders; boats in all versions, shulkers from 1.11)
- **Vanilla:** `level.getCollisions(this, testBox)` includes entity shapes: before block shapes from 1.18, after them before that (`origin/client1.21.11 LocalPlayer.java:981`, `CollisionGetter.java:85-89`; `1.16.4 CollisionGetter.java:60-61`).
- **Native:** `update_auto_jump` iterates only `block_collision_boxes_in_order` (`native/src/simulation.rs:2052`), while the move itself does use `entity_collisions` (:1786).
- **Impact:** with auto-jump on, the player does not jump onto boats or shulkers, and from 1.18 a different obstacle can be chosen.
- **todo.md:** none
- **Fix:** fixed — `update_auto_jump` now scans the move's entity colliders (boats, shulkers, a happy ghast from above) with the block boxes, blocks first before 1.18 and entities first from 1.18, with the 1e-7 inflated entity query from 1.14; the 1.14+ world-border shape is still not added.

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
- **Fix:** fixed — the spin attack now sweeps from the box after `launch_riptide`'s 1.2 lift (1.16.5 LivingEntity.java:2504), and the adapter tells marker (flag bit 16, no box) and small (bit 1, half size) armor stands apart (`player.ts` armorStandBox). The pose order follows each release (gliding first up to 1.21.4, sleeping and swimming first from 1.21.5) and passengers and spectators skip the fit. Test: `the_spin_attack_sweeps_from_the_box_past_the_riptide_lift` and the adapter pose tests.

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
- **Fix:** fixed — 1.13.2-1.19.3 have no fall-flying term, the in-water test is limited to the sprint-key path, and the 1.21.4 term now uses `!isMovingSlowly(state)` in place of `!state.crouching`, as the 1.21.5+ branch does.

#### D-046 · 1.21.4 shouldStopSprinting omits isFallFlying
- **Versions:** 1.21.4 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `isFallFlying() || hasBlindness() || isMovingSlowly() || isPassenger() && !isRidingCamel() || …` (`origin/client1.21.4 LocalPlayer.java:795-796`).
- **Native:** `src/mineflayer/player.ts:604-608` has no fall-flying term and no passenger term.
- **Impact:** the sprint flag and sprint modifier survive a glide.
- **todo.md:** none
- **Fix:** fixed — The 1.21.4 shouldStopSprinting now includes `state.fallFlying` (the previous tick's flag); the passenger term is not added because the player tick in player.ts models no vehicle (riding is in vehicles.ts).

#### D-047 · Swim-sprint stop uses the 1.21.5 rule (blindness, low food) on older releases
- **Versions:** 1.13.2-1.21.4 (blindness part 1.13.2-1.21.3) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `if (!onGround && !shift && (!forwardImpulse || !food) || !isInWater()) setSprinting(false)`, with no blindness stop (`origin/client1.14.4 LocalPlayer.java:632-640`, `1.21.4 :691-697`; the new rule is in `1.21.5 :819-821`).
- **Native:** `shouldStopSwimSprinting = blindness || !inWater || … || !food` (`src/mineflayer/player.ts:710-715`).
- **Impact:** with low food on the ground or holding shift, or under blindness, swimming ends early (pose, drag and speed differ).
- **todo.md:** none
- **Fix:** fixed — Before 1.21.5 the swim-sprint stop is `(!onGround && !sneak && (!forward || !food)) || !isInWater` with no blindness term (forward meaning >= 0.8 in the 1.13.2 form); on 1.21.4 blindness still ends the sprint through shouldStopSprinting.

#### D-048 · From 1.21.9, flying players are exempt from the shallow-water sprint block; the adapter is not
- **Versions:** 1.21.9-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `isSprintingPossible(getAbilities().flying)` → `(allowedInShallowWater || !isInShallowWater())` (`origin/client1.21.9 LocalPlayer.java:814-815, 1014-1019`).
- **Native:** `src/mineflayer/player.ts:685, 707` always block.
- **Impact:** creative flight skimming water loses the sprint (2× flying speed).
- **todo.md:** 719 (open)
- **Fix:** fixed — From 1.21.9 the shallow-water term in canStartSprinting and shouldStopRunSprinting is skipped while `state.flying` is set; 1.21.6-1.21.8 keep the 1.21.5 rule.

#### D-049 · Adapter clears fall-flying after a flying tick on 1.21.2+
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the client no longer clears flag 7 in Player.travel; only the server does (`origin/client1.21.2 Player.java:1399-1421`, `LivingEntity.java:2864-2869`).
- **Native:** `if (flyingDuringMove) state.fallFlying = false` for all releases (`src/mineflayer/player.ts:284`). The engine gates this correctly (`simulation.rs:2135, 2785`).
- **Impact:** the glide stops a tick or more early when creative flight is toggled.
- **todo.md:** none
- **Fix:** fixed — The adapter's `flyingDuringMove` clear is now gated `!lt('1.9') && lt('1.21.2')`, matching the engine's `flight_resets_fall_before_travel` gate.

#### D-050 · 26.3 refuses to start fall flying in lava; the adapter checks only water
- **Versions:** 26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `tryToStartFallFlying` requires `!isInLiquid()` (`origin/client26.3 Player.java:1351-1352`).
- **Native:** `!state.isInWater` only (`src/mineflayer/player.ts:755-760`).
- **Impact:** the FALL_FLYING pose and glide start in lava.
- **todo.md:** 1016 (open)
- **Fix:** fixed — From 26.3 the elytra start also requires `!state.isInLava` (the previous tick's lava flag, the tick-start fluid state aiStep reads); 26.1 and 26.2 check only water.

#### D-051 · Elytra start checks climbable from 1.15; vanilla adds the check in 1.15.2
- **Versions:** 1.15, 1.15.1 · **Severity:** minor · **Status:** confirmed (from `generated/physics-methods.json`)
- **Vanilla:** the 1.15 and 1.15.1 `LocalPlayer.aiStep` have no `onLadder()` check; 1.15.2 adds it.
- **Native:** `!state.onClimbable` from 1.15 (`src/mineflayer/player.ts:757`).
- **Impact:** no glide start on ladders or vines in 1.15 and 1.15.1.
- **todo.md:** none
- **Fix:** fixed — The climbable term in the elytra start now applies only from 1.15.2: 1.15 ClientPlayerEntity.java:711-713 has no climbable test, physics-methods.json shows it added in 1.15.2, and 1.16 :742 has `!this.isClimbing()`.

#### D-052 · Auto-jump's synthetic jump is not counted in wasJumping
- **Versions:** flight toggle 1.13.2-26.3 (1.11-1.12 likely); elytra start 1.15-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `wasJumping` is read from `input.keyPresses` before the tick, and that includes `makeJump()`. The flight double-tap also requires no auto-jump on the tick. (`origin/client1.21.11 LocalPlayer.java:711-722, 768, 784`)
- **Native:** `freshJump = !prevControl.jump && control.jump`, from raw keys (`src/mineflayer/player.ts:739, 744, 757, 294`).
- **Impact:** the adapter acts on jump presses that vanilla ignores after or during an auto-jump.
- **todo.md:** none
- **Fix:** fixed — inputsToFly now uses `jumping = control.jump || autoJumping` and `wasJumping = prevControl.jump`, with `prevControl.jump` stored as `control.jump || autoJumping`, so the flight toggle ignores the auto-jump tick; the toggle part applies from 1.10 (confirmed 1.10.2-26.3), not only from 1.13.2.

#### D-053 · Riptide launch needs water; vanilla also allows rain
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `isInWaterOrRain()` (`isWet()` in 1.13.2) (`origin/client1.21.11 TridentItem.java:releaseUsing`; `1.13.2 ItemTrident`).
- **Native:** `state.isInWater` only, and there is no rain input (`src/mineflayer/player.ts:180`).
- **Impact:** no launch in rain (push, spin, 1.2 lift).
- **todo.md:** none
- **Fix:** deferred — Confirmed real (1.21 TridentItem.java:83 `isTouchingWaterOrRain()`, 1.13.2 `isWet()`), but rain needs the client rain level, sky visibility, the MOTION_BLOCKING heightmap and the biome precipitation with its per-version temperature noise, none of which the adapter or engine has; a partial check would add new deviations.

#### D-054 · Riptide ground lift skips the inside-block pass
- **Versions:** 1.13.2-1.21.1 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the 1.2 lift is a full `Entity.move`, so `checkInsideBlocks` runs (bubble columns, cobweb and so on) (`origin/client1.20.4 Entity.java:713`; `TridentItem.releaseUsing`).
- **Native:** `launch_riptide` only collides and moves (`native/src/simulation.rs:1888-1912`).
- **Impact:** one bubble-column or stuck application fewer on the launch tick.
- **todo.md:** none
- **Fix:** fixed — Below 1.21.2 `launch_riptide` now runs the inside-block movement effects after the 1.2 lift and before the 1.15+ speed factor, as `Entity.move` does (1.13.2 Entity.java:841, 1.20.4/1.21 :713); a new MOVE_SLOWED output flag gives the adapter `isInWeb` from the main move.

#### D-055 · 1.8.8: the 600-tick sprint timeout is missing
- **Versions:** 1.8.8 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `sprintingTicksLeft = sprinting ? 600 : 0`, counted down, then `setSprinting(false)` (`origin/client1.8.9 EntityPlayerSP.java:337-339, 463-469`).
- **Native:** no counter (`src/mineflayer/player.ts:641-731`).
- **Impact:** a double-tap sprint without the sprint key lasts past 30 s.
- **todo.md:** none
- **Fix:** fixed — Below 1.9 the adapter keeps `sprintingTicksLeft` (600 on sprint start, 0 on stop), counts it down first in inputsToSprint and ends the sprint at 0, as 1.8.9 ClientPlayerEntity.java:349-352, 487-492 does.

#### D-056 · Sprint-key start is blocked by the previous tick's sneak before 1.14
- **Versions:** 1.8.8-1.13.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** the sprint-key branch has no sneak condition; only the double-tap branch checks the previous sneak (`origin/client1.8.9 EntityPlayerSP.java:528-536`; `1.13.2 :699-707`).
- **Native:** `canStartSprinting` has `(!crouching || isUnderWater)` with crouching = previous sneak (`src/mineflayer/player.ts:595, 665, 677-685`).
- **Impact:** a one-tick sprint lag after letting go of sneak.
- **todo.md:** none
- **Fix:** fixed — The crouching term is gone from canStartSprinting before 1.19.4, so only the double-tap branch tests the previous sneak (the sprint-key branch has no sneak term in 1.8.9 :567-573 through 1.13.2 :737-743); a regression test covers 1.8.8, 1.12.2 and 1.13.2.

#### D-057 · The adapter never imports the no-gravity flag
- **Versions:** 1.10.2-26.3 (1.8.8/1.9.4 have no shared flag) · **Severity:** minor · **Status:** partially-confirmed (`vehicle.rs`/`ridden.rs` also lack any no-gravity handling)
- **Vanilla:** `Entity.getGravity` returns 0 when `isNoGravity()` (`origin/client1.21.11 Entity.java:1443-1445`).
- **Native:** the engine supports `no_gravity`, but `src/mineflayer/player.ts:302-405` and `vehicles.ts` never set it.
- **Impact:** a player or mount with the NoGravity metadata still falls.
- **todo.md:** 395 `[x]` (non-physics-false), 761 `[x]` (non-physics-false)
- **Fix:** fixed — the adapter imports the flag for the player, boats and ridden mobs (`NativeEngine.noGravityOf`: the state's own flag, else metadata[5], from 1.10) and the N-API boat, horse and ridden-mob states carry it. Boat.floatBoat drops the gravity but keeps the buoyancy (1.20.5 BoatEntity.java:582-615), LivingEntity.travel skips it in air, water and lava (1.12.2 LivingEntity.java:1549-1603, 1.16.5 :2018-2122), and from 26.2 the landing bounce uses getEffectiveGravity, 0 under NoGravity, for its threshold and gravity compensation (26.2 Entity.java:848, 855, 887-889, 1532-1533; `MoveRules::with_gravity_of`). Tests: `a_boat_with_no_gravity_does_not_fall_but_keeps_its_buoyancy`, `a_ridden_mob_with_no_gravity_takes_no_gravity_in_air_water_or_lava`, `a_no_gravity_mob_bounces_off_a_slow_landing_from_26_2`, `a_no_gravity_boat_bounces_with_no_gravity_compensation_from_26_2`.

#### D-058 · Eye heights for SLEEPING and DYING
- **Versions:** 1.14.4-26.3 (observable for sleeping 1.14.4-1.15.2 and 1.21-26.3) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** SLEEPING eye is 0.2F and DYING eye is 1.62F (fixed) (`origin/client1.20.4 LivingEntity.java:3286-3288`; `1.21.11 LivingEntity.java:3878`, `Avatar.java:55`).
- **Native:** `player.rs:41-45` and `napi_api.rs:2083-2090` give SLEEPING 1.62. The adapter gives both 0.4, and scales DYING (`src/mineflayer/player.ts:889-902`).
- **Impact:** eye-in-water for a sleeping player over a bed with water above.
- **todo.md:** 356 `[x]` (deviates)
- **Fix:** fixed — The adapter gives SLEEPING an eye height of 0.2F (minus 0.08F when sneaking on 1.8) and DYING 1.62, both unscaled, and the engine's `Pose::Sleeping` and N-API `pose_eye_height(2)` return 0.2F (range checked in every 1.14-1.20.4 source).

#### D-059 · 1.13.2 auto-jump obstacle order: full cubes are deferred and merged
- **Versions:** 1.13.2 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** full-cube cells are collected in a bitset and appended at the end as one merged VoxelShapeInt, so partial shapes are tested first, and the "above" probe starts from the merged box's center (`origin/client1.13.2 IWorldReaderBase.java:func_212391_a:112-160`; `EntityPlayerSP.java:902-905`).
- **Native:** per-cell order for every release (`native/src/world.rs:935-975`; `simulation.rs:2052-2081`).
- **Impact:** next to fences or slabs, or a stacked block, the jump/no-jump decision differs.
- **todo.md:** none
- **Fix:** fixed — For 1.13 and 1.14.0 (`at_least(1,13,0) && !at_least(1,14,1)`, bracketed by the 1.14 and 1.14.4 sources) the new `block_collision_boxes_merging_full_cubes` emits partial shapes in walk order and the full cubes last as merged boxes, as the bitset deferral does.

#### D-060 · Before 1.14 the flight vertical input is applied as one merged impulse
- **Versions:** 1.8.8-1.13.2 · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** separate `motionY -= a` and `motionY += a` (`origin/client1.8.9 EntityPlayerSP.java:561-569`; `1.13.2 :760-771`).
- **Native:** a net `direction` impulse (`native/src/simulation.rs:2213-2218`).
- **Impact:** a 1-ulp difference in vy with jump and sneak held together.
- **todo.md:** 757 `[x]` (holds; cosmetic)
- **Fix:** fixed — Before 1.14 the flight vertical input is two separate double steps (`vy -= f64(flySpeed*3.0F)` when sneaking, then `+=` when jumping); from 1.14 the netted impulse stays.

#### D-061 · Adapter scales the fixed DYING box by SCALE
- **Versions:** 1.20.5-26.3 · **Severity:** cosmetic · **Status:** partially-confirmed
- **Vanilla:** `EntityDimensions.fixed(0.2F, 0.2F)` ignores scale (`origin/client1.21.11 Avatar.java:55`).
- **Native:** `src/mineflayer/player.ts:858-862` exempts only SLEEPING.
- **Impact:** at most a transient tick (DYING is replaced on 1.17+).
- **todo.md:** none
- **Fix:** fixed — `poseSize` no longer scales DYING, as it already left SLEEPING alone, and the DYING eye height is unscaled too.

#### D-062 · Auto-jump start-Y gate at 1.16; vanilla 1.15 already uses position()
- **Versions:** 1.15-1.15.2 · **Severity:** cosmetic · **Status:** confirmed (javap)
- **Vanilla:** `updateAutoJump` uses `position()` from 1.15.
- **Native:** `feet_y` uses `bounds.min.y` before 1.16 (`native/src/simulation.rs:1975-1979`).
- **Impact:** none for consistent input (position.y == minY before 1.17).
- **todo.md:** none
- **Fix:** fixed — The `feet_y` gate moved from 1.16 to 1.15 (1.15 uses `getPos()`, 1.14 and 1.13.2 the box minY).

### 2.6 Attributes and effects

#### D-063 · Before 1.20.5, effects with negative-byte amplifiers are dropped
- **Versions:** 1.8.8-1.20.4 (Levitation 1.9+, Slow Falling and Dolphin's Grace 1.13+) · **Severity:** major · **Status:** confirmed
- **Vanilla:** the effect packet reads the amplifier as a signed `readByte()`, and `MobEffectInstance` does not clamp it before 1.20.5. So `/effect … 255` is amp -1 and stays active: Levitation targets 0 (hover), and Jump Boost 128-254 lowers jump power. (`origin/client1.20.4 ClientboundUpdateMobEffectPacket.java:50`, `MobEffectInstance.java:76`, `LivingEntity.java:2017, 2157`; `1.8.9 S1DPacketEntityEffect.java:37`)
- **Native:** `getEffectLevel = amp + 1`, and the adapter treats a level ≤ 0 as no effect. The N-API fields are `Option<u8>`. (`src/mineflayer/base.ts:93-98`, `player.ts:380-400`; `native/src/napi_api.rs:112-118`)
- **Impact:** a Levitation 255 hover falls in native. Slow Falling, Dolphin's Grace, Blindness (sprint) and negative Jump Boost are ignored.
- **todo.md:** 820 `[x]` (partial), 730 `[x]` (deviates)
- **Fix:** fixed — the levitation, speed, slowness and jump-boost amplifiers are now `Option<i32>` in the N-API and `SimulationState`, and the adapter reads the amplifier as a signed byte before 1.20.5 (clamped 0-255 from 1.20.5, as the varint packet and `MobEffectInstance` do), so amp -1 Levitation, Slow Falling, Dolphin's Grace, Blindness and negative Jump Boost stay active.

#### D-064 · movement_speed and jump_strength lose every op1/op2 modifier except sprint, speed and slowness
- **Versions:** movement_speed in all versions; jump_strength 1.20.5-26.3 · **Severity:** major (not visible in vanilla survival, where those are the only multiplicative modifiers) · **Status:** partially-confirmed
- **Vanilla:** `AttributeInstance.calculateValue` folds every ADD_VALUE, ADD_MULTIPLIED_BASE and ADD_MULTIPLIED_TOTAL modifier, then clamps; getSpeed and getJumpPower use the full value (`origin/client26.3 AttributeInstance.java:151-169`; `1.21.11 LivingEntity.java:2224-2225`).
- **Native:** the adapter sends base + ADD_VALUE only (`src/mineflayer/base.ts:163-167`; `player.ts:378, 389`). The engine re-applies only sprint, speed and slowness (`native/src/simulation.rs:203-228`). The adapter's own `jumpFromGround` uses the full value (`player.ts:769`).
- **Impact:** wrong ground, air and water speed and jump height under item, `/attribute` or plugin modifiers.
- **todo.md:** 26 `[x]` (partial), 794 `[x]` (partial), 800 `[x]` (partial)
- **Fix:** fixed — new `SimulationState.movement_speed_value` (`movementSpeedValue`) carries the fully folded attribute value and travel uses it in place of the engine's sprint/speed/slowness fold; the adapter sends the full synced movement_speed and jump_strength values, so op1/op2 modifiers are kept.

#### D-065 · From 1.21.2, Slow Falling uses the pre-move velocity
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `travelInAir` calls `getEffectiveGravity()` after the move, so it tests the post-collision delta (`origin/client1.21.2 LivingEntity.java:2194-2197, 2213-2229`).
- **Native:** `going_down` is computed before the move (`native/src/simulation.rs:2381-2386`, used at :2724).
- **Impact:** with Slow Falling, a ceiling-hit jump gets 0.08 gravity in native (vanilla 0.01), and a slime or bed bounce gets 0.01 in native (vanilla 0.08).
- **todo.md:** 818 `[x]` (partial)
- **Fix:** fixed — from 1.21.2 the air branch computes Slow Falling gravity from the post-move `velocity.y` (before the local climbable 0.2 override), as `travelInAir` does; 1.21 and the fluid/glide paths keep the pre-move delta, as vanilla does.

#### D-066 · Adapter ADD_MULTIPLIED_TOTAL formula and modifier order
- **Versions:** effective on 1.20.5-26.3 (the attributes resolved this way exist only there) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `total *= 1.0 + amount`, iterated in hash-container order (`origin/client1.21.11 AttributeInstance.java:150-172`).
- **Native:** `value += value * amount`, in packet order (`src/mineflayer/base.ts:140-154`).
- **Impact:** 1-ulp differences (about 26% of inputs) in gravity, step height and scale under custom modifiers.
- **todo.md:** 34 `[x]` (deviates), 791 `[x]` (partial)
- **Fix:** fixed — ADD_MULTIPLIED_TOTAL is applied as `value *= 1.0 + amount`, and modifiers iterate in the modelled per-release container order (HashSet copy 1.8-1.15, UUID HashSet 1.16-1.20.4, fastutil map by UUID in 1.20.5, by Identifier from 1.21), which reproduces the engine's existing sprint/speed/slowness orders.

#### D-067 · Compact attribute objects without a modifiers array throw
- **Versions:** all · **Severity:** minor · **Status:** confirmed
- **Vanilla:** an attribute with no modifiers evaluates to its clamped base.
- **Native:** `for (const modifier of attribute.modifiers)` and `.modifiers.filter/find/some` have no fallback (`src/mineflayer/base.ts:148-150, 166`; `player.ts:412, 427`). Stock Mineflayer always fills the array.
- **Impact:** a TypeError stops the tick for custom callers.
- **todo.md:** 34 `[x]` (deviates)
- **Fix:** fixed — `Attribute.modifiers` is optional and falls back to `[]`, so a compact attribute evaluates to its clamped base; the clamp also maps NaN to the minimum from 1.19, matching `sanitizeValue`.

#### D-068 · Generic entity descriptor drops movement_efficiency, air_drag_modifier, friction_modifier and other fields
- **Versions:** 1.21-26.3 / 26.2-26.3 / all · **Severity:** minor (an unused helper) · **Status:** partially-confirmed
- **Vanilla:** every LivingEntity reads MOVEMENT_EFFICIENCY (1.21+) and FRICTION/AIR_DRAG modifiers (26.2+).
- **Native:** `create_simulation_state` forwards `water_movement_efficiency` only. It drops movement_efficiency, the modifiers, sneaking_speed, flying, flying_speed, jump_ticks, using_item and no_physics. (`native/src/napi_api.rs:122-124, 1960-2041`)
- **Impact:** wrong soul-sand speed, friction and drag for callers of `createSimulationState`.
- **todo.md:** 29 `[x]` (deviates)
- **Fix:** fixed — `createSimulationState` now forwards movement_efficiency, air_drag_modifier and friction_modifier (with their presence flags), sneaking_speed, jump_ticks, using_item, flying, flying_speed and no_physics; version gating stays in the engine.

#### D-069 · Per-entity gravity and step height are not validated on the NativePlayer/simulateVersionedTick paths
- **Versions:** all (attributes exist from 1.20.5) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `RangedAttribute.sanitizeValue` maps NaN to the minimum and clamps (gravity -1..1, step 0..10).
- **Native:** `valid_state` is called only on the descriptor path (`native/src/napi_api.rs:2757-2782, 2034`). It also rejects negative gravity, which vanilla allows.
- **Impact:** NaN velocity is silently zeroed on the next tick instead of being rejected.
- **todo.md:** 397 `[x]` (non-physics-false)
- **Fix:** fixed — the piston path's `move_entity` now sanitizes the per-entity step height like the main tick (NaN to 0, clamp 0..10), with a test of a pushed player against a 12-block wall; the descriptor's `valid_state` gravity range check is API-boundary validation and was left unchanged.

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
- **Fix:** fixed — `collide_shapes` takes the colliders as VoxelShapes (`Colliders::listed`: entity colliders, the border, then the blocks as whole shapes in Cursor3D order) and checks 1e-7 once before each shape (`shapes_collide`). 1.14-1.17.1 also meet the border shape getBlockCollisions offers again at the head of the blocks for a box touching it (1.15 CollisionView.java:80-88, 1.16.5 BlockCollisionSpliterator.java:56-58, 100-112; `Colliders::block_border`, `WorldSnapshot::block_collisions_offer_world_border`). On 1.13.2 the player and the vehicles (`vehicle.rs` `legacy_collide`) go through `collide_legacy`: each block's boxes one shape, met with x fastest, the full cubes joined in one shape after them (1.13.2 RenderBlockView.java:113-153). Tests: `the_sub_epsilon_check_falls_between_whole_shapes`, `colliders_go_entities_border_then_blocks_in_cursor_order`, `the_border_offered_again_by_the_block_stream_is_another_check`, `the_block_stream_offers_the_border_to_a_box_touching_it_from_1_14_to_1_17`, `a_1_13_vehicle_checks_the_sub_epsilon_move_once_a_block_shape`.

#### D-075 · The 1.14-1.16 single-axis Shapes.collide path is not modelled
- **Versions:** 1.14.4-1.16.5 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** a move along a single axis goes through `collideBoundingBoxHeuristically → Shapes.collide(AABB, LevelReader, …)`. It collides in block-local coordinates, returns 0 at `|d| < 1e-7` after every cell, uses lastC, and adds entity and border shapes afterwards with no zero check. (`origin/client1.16.4 Entity.java:740-750, 785`; `Shapes.java:193-260`)
- **Native:** the flat list approach from 1.17+ for every release from 1.14 (`native/src/simulation.rs:1806-1819`; `collision.rs:236, 404-473`).
- **Impact:** last-bit position residuals in rare tight contacts.
- **todo.md:** 509 `[x]` (partial)
- **Fix:** fixed — for 1.14-1.17.1 a move along one axis goes through `collide_single_axis` / `walk_axis`, VoxelShapes.calculatePushVelocity line by line (1.16.5 VoxelShapes.java:243-306): the 1e-6 thin-box pass-through, the 1e-7 drop before the walk and after each cell's shape, the AxisCycle walk order, the face cells only for large shapes, then the entity colliders and the border each with its own check (`collision.rs`; gated at simulation.rs and vehicle.rs `single_axis_walk`).

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
- **Fix:** fixed — `src/mineflayer/blocks.ts` marks chorus_flower as never in-wall or suffocating (LEGACY_NOT_IN_WALL and NEVER_SUFFOCATING): its material does not block movement (Material.PLANT in 1.9.4-1.19.4, 1.12.2 Block.java:323-325, 1.13.2 Block.java:362-364; notSolid from 1.20). Test: "never takes the chorus flower for a wall or a suffocating block".

#### D-083 · Unloaded-chunk semantics cover only shape queries; set_block re-loads the chunk
- **Versions:** all (direct API; the adapter removes cells itself) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** a missing chunk reads as air with an empty fluid for every query, and `setBlockState` on an empty chunk is a no-op (`origin/client1.18.1 ClientChunkCache.java:getChunk`, `EmptyLevelChunk.java:21-31`).
- **Native:** fluids, friction, effects, suffocation and support read stored cells of unloaded chunks, and `set_block` inserts into `loaded_chunks` (`native/src/world.rs:199-201, 504, 533, 674, 681, 1028, 1063, 1201`).
- **Impact:** wrong fluid and friction state for direct N-API callers.
- **todo.md:** 557 `[x]` (partial)
- **Fix:** fixed — `block_state_id`, `block_entity`, `contains_block`, `is_section_present`, `section_block_count` and the outline/interaction extra-shape reads (world.rs) are gated on the chunk being loaded, so an unloaded chunk reads as EmptyLevelChunk. Test: `every_read_of_an_unloaded_chunk_gives_the_empty_chunk` (unload, empty reads, reload).

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
- **Fix:** deferred — the common case of a player-owned animal stays fixed, but the 1.21.5+ root-owner walk differs from `OwnableEntity.getRootOwner` (1.21.11 `OwnableEntity.java:20-36`): it needs the tame bit instead of following `getOwner()` for every OwnableEntity, returns an intermediate owner's team where vanilla returns null, and does not limit the lookup to LivingEntity (`EntityReference.java:57-69`); these need non-player owner chains and a chain test.

#### D-089 · Happy ghast collider ignores staysStill
- **Versions:** 1.21.6-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** an adult happy ghast collides when the player is above `bbox.maxY`, **or** while `isOnStillTimeout()` is true (synced STAYS_STILL: 10 ticks after a dismount, and while a player stands on top) (`origin/client1.21.11 HappyGhast.java:511-521, 487-489`).
- **Native:** always `only_from_above` (`native/src/entity.rs:167-170`; `world.rs:190-197`). The adapter receives `staysStill` but drops it (`src/mineflayer/player.ts:468`).
- **Impact:** the player passes through a still ghast from the side or from below.
- **todo.md:** none
- **Fix:** fixed — the adapter maps an adult happy ghast with `staysStill` to the new `happy_ghast_stays_still` kind (NativePlayer kind 6), a collider from every side that pushes and bounces a riptide spin like a living entity, matching `canBeCollidedWith` (1.21.6 `HappyGhast.java:544-553` through 26.3 `:522-531`).

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
- **Fix:** fixed — the adapter maps bat, parrot, ender dragon and vex to `still_living` (no push, no collider, still bounces a spin) and a spectator player to `inert`, as the sources confirm from 1.9.4 (bat/dragon/spectator), 1.11.2 (vex) and 1.12.2 (parrot) through 1.21.11.

#### D-091 · A sleeping or dead player is still pushed
- **Versions:** 1.9.4-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `LivingEntity.push(Entity)`: `if (!isSleeping()) super.push`; `isPushable` requires `isAlive()` (`origin/client1.21.11 LivingEntity.java:2188-2192, 3202-3204`).
- **Native:** no pose or health check (`native/src/entity.rs:231-236`).
- **Impact:** the player in bed, or dying, is pushed out of position.
- **todo.md:** none
- **Fix:** fixed — a new `dead` state (NAPI field, input flag 1<<24) and the SLEEPING input flag now gate pushes: removed from 1.9, dead and climbable from 1.11 (`isPushable` is just `!removed` in 1.9.4 `:1855` and 1.10.2 `:1891`), and sleeping blocks living pushers only, since a boat pushes through `Entity.push` (1.21.11 `AbstractBoat.java:178-186`).

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
- **Fix:** deferred — the box follows the client's own `currentPeekAmount` (1.21.11 `Shulker.java:201-240`), which mineflayer does not track per shulker, and the lift needs a new engine move of the local player inside the shulker's tick; neither exists yet.

#### D-093 · Pusher and collider boxes ignore size-changing state
- **Versions:** slime size all; remote player poses 1.9.4-26.3; scale 1.20.5+ · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** a slime's box is 0.52·size (0.51/0.255·2.04 in older releases). A remote player's box follows its pose (gliding, swimming 0.6, sleeping 0.2, sneaking 1.65 before 1.14), and it is scaled by SCALE. (`origin/client1.21.11 Slime.java:327-329`; `Avatar.java:54-55`; `1.13.2 EntityPlayer.java:335-362`)
- **Native:** registry size, with only baby and crouch (1.14+) adjustments (`mineflayer/lib/plugins/entities.js:9, 209-241, 913-921`; `playerState.ts:78-80`).
- **Impact:** pushes are missed or added near slimes and posed or scaled remote players.
- **todo.md:** none
- **Fix:** deferred — cube-mob and pose sizing are improved, but 26.2+ `sulfur_cube` (`EntityTypes.java:471` 0.49F) is missing from mineflayer's `entitySize.js`, the scale-reset path lacks the float rounding of `EntityDimensions.scale`, and the 1.9-1.13 `updateSize` block check, tick timing and min-corner anchoring are not modelled.

#### D-094 · Rocket boosts and entity pushes are applied as separate passes
- **Versions:** 1.11.2-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** each entity runs its whole tick in list order. A pusher that ticks before the rocket pushes the player first, and the boost then halves that push (`0.5v + 0.85·look`). (`origin/client1.21.11 FireworkRocketEntity.java:125-126`; `ClientLevel.tickEntities`)
- **Native:** in each group, all boosts first and then all pushes (`native/src/napi_api.rs:1596-1612`; `entity.rs:304-322`).
- **Impact:** the velocity of a boosted glider overlapping an entity is off by up to 0.025 b/t per entity.
- **todo.md:** 683 `[x]` (deviates)
- **Fix:** fixed — a new `attached_rocket` kind (code 7) boosts in its turn among the pushes in `push_by_nearby_entities`, as `FireworkRocketEntity.tick` does in list order (1.21.11 `:124-131`); the live bot and replay still use the count path because they do not mark attached rockets.

#### D-095 · 1.14-1.16 "ticks before the player" hash-slot model
- **Versions:** 1.14.4-1.16.5 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Int2ObjectOpenHashMap(32)` holds 24 entries, rehashes on the 25th put, shrinks only below maxFill/4, and slots depend on the insert and remove history (`origin/client1.14.4 MultiPlayerLevel.java:69, 105-145`).
- **Native:** the model grows at `count ≥ size·0.75`, never keeps a grown table, and assumes id-order insertion (`mineflayer-physics-utils playerState.ts:~40-56`; `mineflayer physics.js:2854-2892`).
- **Impact:** at the threshold counts and after despawns, entities are classed wrongly, so the push uses the wrong box or pass and rockets are ordered wrongly.
- **todo.md:** none
- **Fix:** deferred — `playerState.ts` now reads mineflayer's real `_entityTickOrder` map, collect packets and client-side death removal are modelled with fastutil's exact growth and iterator removal, but the other client-side self-removals in `ClientWorld.tickEntities` (item age, XP orbs, projectiles, falling blocks and so on) need per-entity client tick simulation that mineflayer does not run.

#### D-096 · 1.9.4-1.10.2: a player on a ladder is still pushed in vanilla, but native skips it
- **Versions:** 1.9.4, 1.10.2 · **Severity:** minor · **Status:** confirmed (yarn)
- **Vanilla:** `isPushable` = `!removed` in 1.9 and 1.10; the climbing exemption arrives in 1.11 (yarn 1.9.4 and 1.10.2 `LivingEntity.isPushable`; 1.11.2 `:1935`).
- **Native:** `if climbable(world, state) return` for every release from 1.9 (`native/src/entity.rs:234-236`).
- **Impact:** a climbing player keeps its velocity where vanilla adds the push.
- **todo.md:** 692 `[x]` (holds otherwise)
- **Fix:** fixed — the climbing gate is now `release.at_least(1, 11, 0)` (`isPushable` is `!removed` in 1.9.4 `LivingEntity.java:1855` and 1.10.2 `:1891`, `isAlive() && !isClimbing()` from 1.11.2 `:1952`), tested by `a_player_on_a_climbable_or_a_spectator_is_not_pushed`.

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
- **Fix:** fixed — `entity_push_impulses_with_pushthrough` applies the calling entity's `1.0F - pushthrough` to both sides, reuses the per-release float/double sqrt and `>= 0.01F` threshold, gates `isPushable` only from 1.19 (1.18 `:1269-1275` lacks it), and treats a vehicle as an entity with passengers.

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
- **Fix:** fixed — `simulateVersionedEntityTicks` now follows the server-side LivingEntity model: each entity moves and then runs `tickCramming` with live positions, using the 0.2F-grown box in 1.8 (1.8.9 `:1482-1499`) and the plain box from 1.9, with passengers from `vehicleId` and same-vehicle by root vehicle from 1.9.

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
- **Fix:** deferred — the native world needs a moving-block layer with block-entity shapes offset by getExtendedProgress (1.21.11 `PistonMovingBlockEntity.java:342-367`), BlockCollisions' 2-edge read for moving_piston (`BlockCollisions.java:79`) and the pre-1.11 shapes, which changes the core collision loop for every release; most of the impact also depends on pushed-block tracking (D-100).

#### D-100 · Blocks pushed or pulled by a piston never push the player (Mineflayer path)
- **Versions:** all · **Severity:** major · **Status:** confirmed
- **Vanilla:** the client runs moveBlocks. Every pushed or pulled block becomes a non-source moving block entity that sweeps its own shape by 0.5 per tick and pushes entities. (`origin/client1.21.11 PistonBaseBlock.java:253-320`; `PistonMovingBlockEntity.java:107-114`; `1.8.9 TileEntityPiston.java:76-105`)
- **Native:** the trackers only model the piston's own head and base, always `source: true` (`mineflayer/lib/movingPistons.js:1-5, 44-66`; `mineflayer-replay/lib/pistons.js:5`). The engine supports `source=false` (`native/src/piston.rs:24-36`), but nothing feeds it.
- **Impact:** a player in front of a pushed block, or on a block elevator, is not moved.
- **todo.md:** 677 (open)
- **Fix:** deferred — the trackers would have to run PistonStructureResolver on the client (1.21.11 `PistonBaseBlock.java:253-320`), since the server's moving_piston updates arrive a tick late, and that needs per-release push-reaction, immovable-block and stickiness data that neither minecraft-data nor `native/data` has; the engine side (source=false pushes, slime/honey identity through N-API and the adapter) is ready.

#### D-101 · Adapter gives a retracting piston a SHORT head past 0.25 in 1.13.2-1.15.2
- **Versions:** 1.13.2, 1.14.4, 1.15, 1.15.1, 1.15.2 · **Severity:** major · **Status:** confirmed (javap 1.15.2 vs 1.16)
- **Vanilla:** `getCollisionRelatedBlockState` sets only TYPE and FACING, so SHORT stays false (the long arm reaching -0.25). `SHORT = progress > 0.25F` arrives in 1.16. (`origin/client1.14.4 PistonMovingBlockEntity.java:98-100`; `1.16.4 :94`)
- **Native:** `short = !extending && progress > Math.fround(0.25)` for every release from 1.13 (`mineflayer/lib/movingPistons.js:65`; the same in `mineflayer-replay/lib/pistons.js:94`, so the tests do not catch it).
- **Impact:** a player flush behind a retracting piston is pushed 0.26 blocks in vanilla at progress 0.5, and not at all in native.
- **todo.md:** 677 (open)
- **Fix:** fixed — `mineflayer/lib/movingPistons.js` and `mineflayer-replay/lib/pistons.js` set SHORT only from 1.16 (1.13.2/1.14/1.15 set only TYPE and FACING; 1.16 sets `SHORT = progress > 0.25F`), so the retracting arm reaches -0.25 on 1.13.2-1.15.2.

#### D-102 · Slime-block piston bounce is missing
- **Versions:** all; 1.9-1.12 unverified (1.8.9 replaces the move only when extending; 1.13.2+ sets the velocity and then pushes) · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** a moving slime block sets the entity's velocity on the movement axis to ±1 (a client LocalPlayer included) and then pushes (`origin/client26.2 PistonMovingBlockEntity.java:111-170`; `26.3 :118-134`; `1.8.9 TileEntityPiston.java:93-105`).
- **Native:** `MovingPiston` carries no block identity, and push only moves the player (`native/src/piston.rs:22-36, 80-152, 229-278`; `napi_api.rs:287-297`; `src/mineflayer/player.ts:51-58`).
- **Impact:** slime launchers do not launch the player.
- **todo.md:** 989 (open)
- **Fix:** fixed — on 1.9-1.10 a non-source pushed block now uses the bounds of its outline boxes (new `MovingPiston.outline_shapes` / N-API `outlineShapes`, full cube by default), as 1.9.4 `PistonBlockEntity.java:83-88` calls `Block.getCollisionBox(state, BlockView, pos)` (`Block.java:352`); box arithmetic follows vanilla's float order and the slime bounce was verified. Mineflayer still feeds only source pistons (D-100).

#### D-103 · Honey-block piston carry is missing
- **Versions:** 1.15-26.3 (the isSupportedBy alternative from 1.20) · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** `moveStuckEntities` carries on-ground entities above a honey block that moves horizontally, by deltaProgress (`origin/client1.16.4 PistonMovingBlockEntity.java:178-204`; `26.2 :187-201, 314`).
- **Native:** only the moveCollidedEntities equivalent exists (`native/src/piston.rs:54-74`).
- **Impact:** a player on a pushed honey block stays put; vanilla carries it 0.5 per tick.
- **todo.md:** 989 (open)
- **Fix:** fixed — a new `carry()` in `piston.rs` moves on-ground players on or supported by a horizontally moving honey block by deltaProgress through `move(PISTON)` from 1.15 (isSupportedBy from 1.20), with the box top 1.5000000999999998 in 1.15-1.19.4 and 1.5000010000000001 from 1.20. Mineflayer needs D-100 before a honey block is fed.

#### D-104 · From 1.21.10, piston moves are still scaled by the stuck multiplier
- **Versions:** 1.21.10-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `if (moverType != MoverType.PISTON) delta = delta.multiply(stuckSpeedMultiplier)`, while still clearing the multiplier (`origin/client1.21.10 Entity.java:729-735`; `26.3 :783-789`; absent in 1.21.9).
- **Native:** `slowed_by_stuck_speed` runs for every piston move (`native/src/piston.rs:334`; `simulation.rs:1749-1757`).
- **Impact:** a piston-pushed player in a cobweb moves 0.25× (0.05× vertically).
- **todo.md:** none
- **Fix:** fixed — from 1.21.10 a piston move skips the stuck-speed multiply but still clears the multiplier and the motion (1.21.10 `Entity.java:693-699`, 26.3 `:783-789`; 1.21.9 `:693-696` still multiplies).

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
- **Fix:** fixed — piston moves now apply powder snow (FREEZE steps from 1.21.5, `setInPowderSnow` on 1.17-1.21.4), and restitution after collisions runs only on a vertical or horizontal collision in both the piston and tick paths (26.2 `Entity.java:793, 815`; 26.3 `:818, 837`); the output-only contact damage, web and fall-damage signals on a push are left as they were.

#### D-106 · 1.8: piston push area is the full cell
- **Versions:** 1.8.8 · **Severity:** minor · **Status:** partially-confirmed (the head resets to a full cube, so the main case is slabs)
- **Vanilla:** the push area is the moved block's current bounds box, drawn in by the remaining progress; there is no push for air or a null box (`origin/client1.8.9 BlockPistonMoving.java:154-184`).
- **Native:** `piston.cell()` (`native/src/piston.rs:257-268`).
- **Impact:** a slab moved by a piston pushes players in its empty half.
- **todo.md:** none
- **Fix:** fixed — `push_before_1_9` uses the moved block's own box and pushes nothing without one (1.8.9 `PistonBlockEntity.java:85-91`, `PistonExtensionBlock.java:166-200`); source heads and retracting bases stay full cubes (`PistonHeadBlock.java:100-105`, `PistonBlock.java:177-222`), with a test added.

#### D-107 · 1.9-1.10 piston push ignores the source flag and the block shape
- **Versions:** 1.9.4, 1.10.2 · **Severity:** minor (N-API only; Mineflayer sends source pistons only) · **Status:** confirmed (yarn)
- **Vanilla:** the swept box is the moved block's own `getCollisionBox` (head plate, unextended base cube, slab and so on) (yarn 1.9.4 `PistonBlockEntity.java:75-149`).
- **Native:** every extending block uses the head plate and every retracting one a full cube (`native/src/piston.rs:180-184`).
- **Impact:** wrong push area and amount for non-source moving blocks.
- **todo.md:** 677 (open)
- **Fix:** fixed — same change as D-102: the 1.9-1.10 non-source box is the moved block's outline bounds (full cube when none is given), matching 1.9.4 `PistonBlockEntity.java:83-88` and 1.10.2 `:87`, so fences give 1.0, soul sand a full cube and rails and open gates push; the corner is computed as `(local + f64(offset_f32 * step_f32)) + pos`.

#### D-108 · Mineflayer advances moving pistons only on ticks where the player is simulated
- **Versions:** all · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `PistonMovingBlockEntity.tick` runs every client tick in `tickBlockEntities` (`origin/client1.21.11 PistonMovingBlockEntity.java:286-318`; `Minecraft.java:~1798-1800`).
- **Native:** `movingPistons.tick` runs only if `playerTicked` (`mineflayer/lib/plugins/physics.js:2298, 2483, 2527-2531`). Vehicle and ridden paths have no piston push at all.
- **Impact:** piston progress lags around riding or dismounting, and controlled vehicles are never pushed.
- **todo.md:** none
- **Fix:** deferred — `mineflayer/lib/plugins/physics.js` now ticks moving pistons on every client tick, after `clientTickWithoutPlayer` on the early-return paths (1.21.11 `Minecraft.java:1794, 1805, 1807`), with a test; the vehicle push (`PistonMovingBlockEntity.java:286-318`) is `simulateVersionedEntityPistonPush` and `pushByMovingPiston` on the boat and ridden mob engines, for anything that is no player; `pm_ride_*` match, but for the mineflayer plugin, whose call site is outside this repository.

### 2.10 Vehicles and ridden mobs

#### D-109 · Boats never run block-inside effects
- **Versions:** cobweb all; bubble column (submerged) 1.13.2+; honey slide 1.15+; powder snow 1.17+ · **Severity:** major (one verifier minor) · **Status:** partially-confirmed
- **Vanilla:** after its move the boat runs `entityInside` for the blocks it touches: once in 1.8.9, twice from 1.13 (Entity.move plus Boat.tick), and `applyEffectsFromBlocks` twice from 1.21.2 (`origin/client1.21.1 Entity.java:738`, `Boat.java:308`; `26.3 AbstractBoat.java:236-237`; `1.13.2 EntityBoat.java:285`).
- **Native:** `move_vehicle(…, |_, _| {})` in both boat paths, and `stuck_speed_multiplier` is reset every tick (`native/src/vehicle.rs:194-201, 627-634, 813-815, 233-246`).
- **Impact:** a boat in a submerged bubble column, a cobweb, powder snow or on honey moves differently.
- **todo.md:** 1033 (open)
- **Fix:** fixed — the boat's `Entity.move` inside callback (1.8 legacy and 1.9-1.21.1 paths) runs stepOn plus inside effects only from 1.15 and inside effects alone before (1.12.2 `Entity.java:830/839` calls onSteppedOn only when canClimb, which a boat is not; 1.15 `Entity.java:585` gates it by onGround). New test `a_boat_takes_the_slime_step_on_only_from_1_15`.

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
- **Fix:** deferred — the Java confirms it (1.21.11 `AbstractBoat.java:110-116` canVehicleCollide; `LivingEntity.java:3201`; `AbstractMinecart.java:162`), but the vehicle states the plugin ticks and `simulateVersionedBoatTick`/`simulateVersionedHorseTick` carry no nearby entities, and the adapter's NearbyEntity kinds do not separate pushable entities from items, mark same-vehicle passengers or give climbing state, so new data from the plugin and mineflayer-physics-utils is needed first.

#### D-111 · Boats and ridden mobs always use the slow lava push
- **Versions:** 1.16-26.3 · **Severity:** major (one verifier minor: only flowing lava) · **Status:** partially-confirmed
- **Vanilla:** every pushed entity uses 0.007 in an ultraWarm / FAST_LAVA dimension (`origin/client1.16.4 Entity.java:997`; `26.3 :1686-1706`; Strider has no isPushedByFluid override).
- **Native:** hard-coded 0.0023333333333333335; there is no ultra_warm field in the boat or ridden state (`native/src/vehicle.rs:431-434`; `ridden.rs:357-361, 938-944`).
- **Impact:** a strider ridden through flowing Nether lava gets 1/3 of the current.
- **todo.md:** 1047 `[x]` (partial), 1034 `[x]` (partial)
- **Fix:** fixed — new `lava_flow_scale(ultra_warm)` in `vehicle.rs` (0.007 in an ultrawarm / FAST_LAVA dimension) used by the boat fluid push, the ridden-mob base-tick push and the 26.1+ checkFallDamage lava push; `ultra_warm`/`ultraWarm` added to the boat and ridden-mob states and forwarded by the adapter and the plugin (`dimension === 'the_nether'`).

#### D-112 · Ridden horses, donkeys, mules, llamas and camels climb
- **Versions:** 1.8.8 and 1.9.4-26.3 per kind (llama 1.11.2+, trader llama 1.14.4+, camel 1.19.3+); 1.9-1.12 bracketed · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** `AbstractHorse.onClimbable()` (onLadder / isOnLadder) returns false; Camel and AbstractChestedHorse inherit this (`origin/client1.21.11 equine/AbstractHorse.java:917`; `1.14.4 :891`; `1.13.2 :910`; `1.8.9 EntityHorse.java:1343`).
- **Native:** `climbable()` ignores the mob kind. Travel applies the ±0.15 clamp, the -0.15 fall cap, the fall reset and the 0.2 climb, in air and in water (`native/src/ridden.rs:826-840, 871-873`; `simulation.rs:996-1003`). Pig and strider are correct.
- **Impact:** mounts are slowed and climb walls in ladder, vine or scaffolding cells.
- **todo.md:** 1059 (open)
- **Fix:** fixed — new `RiddenMobKind::climbs()`/`on_climbable()`: horses, donkeys, mules, llamas and camels never climb (`AbstractHorse.onClimbable` is false in every source, 1.7 to 26.3), so travel and travel_in_fluid skip the 0.15 clamp, fall cap, fall reset and 0.2 climb for them; pig and strider still climb.

#### D-113 · A sitting or standing-up camel dashes on jump
- **Versions:** 1.19.3-26.3 (camel_husk too) · **Severity:** major · **Status:** confirmed
- **Vanilla:** `LocalPlayer.jumpableVehicle()` requires `canJump()`, which is `!refuseToMove() && super.canJump()`. Otherwise the jump scale is 0 and onPlayerJump is not called. (`origin/client1.21.11 LocalPlayer.java:539-551, 816-842`; `Camel.java:236-238, 253-255`)
- **Native:** `riderJump` and `rider_jump` check only cooldown and onGround (`src/mineflayer/vehicles.ts:510`; `native/src/ridden.rs:1053-1056`). `tick_ridden` dashes without the refuse check (`ridden.rs:436-445`). The plugin also sends START_RIDING_JUMP.
- **Impact:** about 0.6 of vertical hop during the 2.6 s stand-up, and a 55-tick cooldown that removes the sprint bonus.
- **todo.md:** 1072 (open)
- **Fix:** fixed — the camel jump gate tests `pose_time - 1` against 40/52 in the engine, the adapter (`canCamelJump`) and the plugin, since canJump reads the game time the camel's tick read; `useCamelPose` derives sitting from the SITTING pose and positive pose time before 1.19.4 (1.19.3 `CamelEntity.java:547-549, 569, 591-593`), and mineflayer's entity data names index 6 `pose` from 1.14.

#### D-114 · Strider floatStrider grounds the strider only over a still lava source
- **Versions:** 1.16-26.3 · **Severity:** major · **Status:** confirmed
- **Vanilla:** `if (isInLava()) { if (isAbove(LiquidBlock.SHAPE_STABLE, blockPosition(), true) && !lavaAbove) setOnGround(true); else scale(0.5).add(0, 0.05, 0); }`. There is no source check, which belongs only to the collision shape. (`origin/client1.21.11 Strider.java:313-322`; `EntityCollisionContext.java:72-74`)
- **Native:** `is_lava_surface` requires `fluid == Lava && fluid_source` (`native/src/ridden.rs:1006-1012`; `vehicle.rs:919-929`).
- **Impact:** a strider on flowing lava bobs up and halves its speed every tick.
- **todo.md:** 1072 (open)
- **Fix:** fixed — `after_ridden_travel` grounds the strider when `y > blockY + 0.5 - 1.0E-5F` and there is no lava above, with no source test (1.16 `StriderEntity.java:334-343`, 1.21.11 `Strider.java:312-322`, 26.3 `:322-336`); `is_lava_surface` keeps its source test for the collision shape only.

#### D-115 · Boats and ridden mobs use the 1.14+ water scan before 1.13
- **Versions:** 1.8.8-1.12.2 (major); 1.13.2 (own-height rule, negligible) · **Severity:** major · **Status:** partially-confirmed
- **Vanilla:** `handleWaterMovement` over the box shrunk by 0.4F vertically and 0.001 elsewhere; the current is the normalized sum of the unscaled flows ×0.014 (`origin/client1.8.9 Entity.java:750-764`, `World.java:1495-1535`; `EntityLivingBase.java:143-146`).
- **Native:** `world.fluid_contact` (the 1.14+ algorithm) for all releases (`native/src/vehicle.rs:416-437`; `ridden.rs:341-366, 929-937`). The player path uses `legacy_fluid`.
- **Impact:** at water surfaces, mounts switch between water and land travel about 0.29 blocks apart, and the current push differs.
- **todo.md:** 1047 `[x]` (partial)
- **Fix:** fixed — `legacy_fluid::handle_water_movement` applies the 0.4F vertical shrink and the 0.001 shrink with the corners re-ordered after each (the AxisAlignedBB constructor), so a boat shorter than 0.8 scans the vanilla rows; the legacy water scan test gains the Y=100.7 case.

#### D-116 · Before 1.14, ridden-mob isInLava uses a box deflated by only 0.001
- **Versions:** 1.9.4-1.13.2 (1.8.8 is not client-observable) · **Severity:** major (one verifier minor) · **Status:** confirmed
- **Vanilla:** `isMaterialInBB(bb.expand(-0.1F, -0.4F, -0.1F), LAVA)` (`origin/client1.13.2 Entity.java:1140-1141`; `1.8.9 :831-832`).
- **Native:** `has_fluid_block_inside` with a 0.001 deflation (`native/src/ridden.rs:363-365`; `world.rs:582-590`). `legacy_fluid::touching_lava` exists but is unused here.
- **Impact:** at lava edges a mount takes lava travel (0.5 drag) where vanilla keeps air travel.
- **todo.md:** 1072 (open)
- **Fix:** fixed — before 1.14 ridden-mob isInLava uses `legacy_fluid::touching_lava` (`expand(-0.1F, -0.4F, -0.1F)`, the same in 1.9.4-1.13.2) instead of the 0.001-deflated box; 1.14+ is unchanged.

#### D-117 · 1.8.8: ridden horses and pigs are simulated on the client
- **Versions:** 1.8.8 · **Severity:** major (one verifier minor: the shipped plugin never routes 1.8 mounts here) · **Status:** partially-confirmed
- **Vanilla:** 1.8.9 ridden-mob travel runs only when `!isRemote`; the client interpolates (`origin/client1.8.9 EntityLivingBase.java:1108, 1540`; `EntityHorse.java:1024-1026`).
- **Native:** `tick_ridden_mob` runs full physics for 1.8.8 (`native/src/ridden.rs:232-337`; `napi_api.rs:2419-2479`; `src/mineflayer/vehicles.ts:316-318`).
- **Impact:** predictions from physics that the vanilla client never runs.
- **todo.md:** 1072 (open)
- **Fix:** fixed — the 1.8 ridden-mob branch does not simulate the mount and only applies `HorseBaseEntity.travel`'s setRotation (yaw `% 360`, pitch `* 0.5 % 360`; 1.8.9 `HorseBaseEntity.java:1066-1068`, `Entity.java:321-323`, the 1.8.9 tree standing in for 1.8.8).

#### D-118 · Boat bubble-column activation and impulse are claimed done but not implemented
- **Versions:** 1.13.2-26.3 (claim); 26.3 observable · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:**
  - Up to 26.2, the 60-tick activation and the 2.7/0.6/-0.7 launch run only on the server.
  - 26.3 adds entity events 71/72, and `handleBubbleColumnEffect` runs on the client (down: -0.7; up: y set to 0.6).
  - Sources: `origin/client1.21.1 Boat.java:160-167, 335-372`; `26.3 AbstractBoat.java:286-330`.
- **Native:** `bubble_time` and the related flags are only copied through N-API (`native/src/vehicle.rs:59-62`; `napi_api.rs:3877-3882`).
- **Impact:** 26.3 controlled boats over bubble columns keep their velocity.
- **todo.md:** 1037, 1039 `[x]` (not-implemented)
- **Fix:** fixed — the 60-tick activation and launch are server-only up to 26.2; for the client-side 26.3 handling, new `NativeBoatPhysics.handleBubbleColumnEffect(ctx, dragDown)` (26.3+: event 71 adds -0.7 to vel.y, event 72 sets it to 0.6), called by the mineflayer plugin on entity_status 71/72 for the steered boat.

#### D-119 · Boat getGroundFriction counts lily pads
- **Versions:** 1.9.4-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `!(block instanceof WaterlilyBlock)` is excluded from both the sum and the count (`origin/client1.21.1 Boat.java:487-520`; `1.13.2 EntityBoat.java:480`).
- **Native:** every block whose box meets the slice is counted (`native/src/vehicle.rs:371-413`).
- **Impact:** a boat that touches only a lily pad is ON_LAND (friction 0.6) in native instead of IN_AIR.
- **todo.md:** 1034 `[x]` (partial)
- **Fix:** fixed — new `BlockEffect::LilyPad` (code 22) set by the adapter for `lily_pad`/`waterlily`; `ground_friction` leaves lily pads out of both the sum and the count.

#### D-120 · 1.8.8 legacy boat skips the snow-layer and lily-pad clearing, and collides with lily pads
- **Versions:** 1.8.8 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** before moving, the client boat removes snow layers and destroys lily pads at its four corners, and `BlockLilyPad` adds no box for `EntityBoat` (`origin/client1.8.9 EntityBoat.java:onUpdate`; `BlockLilyPad.java`).
- **Native:** no clearing; generic collision (`native/src/vehicle.rs:607-634`).
- **Impact:** a 1.8 boat is stopped or lifted by snow and lily pads.
- **todo.md:** none
- **Fix:** fixed — new `BlockEffect::SnowLayer` (code 23) and `legacy_boat_world`: before 1.13 the boat moves in a world without the lily pads in reach (`BlockLilyPad` adds no box for a boat in 1.9.4-1.12.2 too, a wider range than stated), and the 1.8 legacy boat also clears snow layers and lily pads at its four corners, feet and above, before the move.

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
- **Fix:** fixed — `passenger_position` is replaced by vanilla `boat_passenger_position(boat, BoatSeat, release)` with the 1.8, 1.9-1.20.1, 1.20.2-1.20.4 and 1.20.5+ profiles (float Mth / JOML trig, seat offsets, raft and chest-boat offsets), plus `boat_passenger_yaw` (deltaRotation and the 105° clamp); `tick_boat_with_passenger` now drives `tick_boat_in_world`. 1.18.2 is bracketed by 1.18 and 1.19. The live client still positions the rider in mineflayer's physics.js.

#### D-122 · The unversioned simulateBoatTick / simulateHorseTick are non-vanilla approximations
- **Versions:** all (version-independent API) · **Severity:** minor (not used by the adapter) · **Status:** confirmed
- **Vanilla:** status-dependent friction (0.9/0.45/landFriction), `--deltaRotation` for left, float Mth trig; horse travel through `LivingEntity.travel` (`origin/client1.21.1 Boat.java:587-664`).
- **Native:** fixed drag 0.92/0.8, left `+= 1`, yaw in radians, f64 trig; the horse uses a fixed gravity of 0.08 and drag 0.91/0.8 (`native/src/vehicle.rs:134-162, 1172-1211`; `napi_api.rs:2353-2383, 2463-2468`).
- **Impact:** wrong motion for callers of these exports.
- **todo.md:** 1031 `[x]` (holds for the world path)
- **Fix:** fixed — the 1.21.11+ `floatInWaterWhileRidden` 0.04F lift (water deeper than 0.4) is added on the world-less horse path (`waterHeight`) and in `travel_in_fluid` (26.3 also after lava travel); world-less `tick_boat` keeps the given land friction across ticks; `scripts/native-smoke.cjs:473` now asserts the IN_AIR result.

#### D-123 · Horse riding jump omits Jump Boost
- **Versions:** all where the client moves the horse (1.9.4-26.3; native also 1.8.8) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** adds `0.1F * (amp + 1)` (`getJumpBoostPower`) (`origin/client1.16.4 AbstractHorse.java:686-689`; `1.20.4 :800-801`; `1.21.1 LivingEntity.getJumpPower:2073`).
- **Native:** `horse_jump` has no boost term (`native/src/ridden.rs:637-663`).
- **Impact:** a horse under Jump Boost jumps lower.
- **todo.md:** 1067 (open)
- **Fix:** fixed — `horse_jump` adds Jump Boost `(amp+1)*0.1F` (as a double before 1.20.5, inside getJumpPower's float sum from 1.20.5), and the camel dash too; `jump_boost_amplifier`/`jumpBoostAmplifier` added and sent by the adapter.

#### D-124 · Camel refuse-to-move zeroing runs after native's fluid branch
- **Versions:** 1.19.3-26.3 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `Camel.travel` zeroes x/z when `refuseToMove() && onGround()` before `super.travel` picks a fluid branch (`origin/client1.21.11 Camel.java:219-226`).
- **Native:** returns to `travel_in_fluid` first (`native/src/ridden.rs:804-813`).
- **Impact:** a sitting camel in lava drifts with the current.
- **todo.md:** 1072 (open)
- **Fix:** fixed — `travel()` zeroes a refusing camel's x/z motion and input on the ground before the happy-ghast, nautilus and fluid branches, as `Camel.travel` does before `super.travel`.

#### D-125 · A sitting camel keeps the standing box
- **Versions:** 1.19.3-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** SITTING dimensions use height `2.375 - 1.43` (`origin/client1.21.11 Camel.java:70, 138-139, 604`).
- **Native:** always `(1.7, 2.375)` (`native/src/ridden.rs:62, 212-224`).
- **Impact:** a sitting camel's ceiling collision and fluid contact differ.
- **todo.md:** 1072 (open)
- **Fix:** fixed — new `RiddenMobState::dimensions()`: a sitting camel is `2.375F - 1.43F` high for the body, `box_at` and the N-API output, refreshed when the rider stands it up.

#### D-126 · The camel DASH sync does not start the 55-tick cooldown
- **Versions:** 1.19.3-26.3 · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `onSyncedDataUpdated(DASH)` sets `dashCooldown = dashCooldown == 0 ? 55 : dashCooldown` (`origin/client1.21.11 Camel.java:573-576`).
- **Native:** `syncDash` sets a cooldown only for the nautilus (`src/mineflayer/vehicles.ts:514-517`).
- **Impact:** after a dash run only on the server, native keeps the sprint bonus and allows another dash.
- **todo.md:** 1072 (open)
- **Fix:** fixed — `syncDash` starts a camel's cooldown at 55 when it is 0 (a nautilus still 40), as `onSyncedDataUpdated(DASH)` does (identical 1.19.3-26.3).

#### D-127 · Donkey height is 1.5 in every release
- **Versions:** 1.8.8-1.13.2 (and the 1.8.8 width of every horse: 1.4 in vanilla) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the AbstractHorse/EntityHorse size is 1.3964844×1.6 (1.4×1.6 in 1.8.9); the donkey gets 1.5 from 1.14 (`origin/client1.13.2 AbstractHorse.java:86`; `1.14.4 EntityType.java:514`; `1.8.9 EntityHorse.java:88`).
- **Native:** `Donkey => (1.3964844, 1.5)` with no version gate (`native/src/ridden.rs:66`).
- **Impact:** the ridden donkey's ceiling and fluid contact differ.
- **todo.md:** 1059 (open)
- **Fix:** fixed — ridden-mob dimensions take the release: every horse kind is 1.4×1.6 before 1.9 and 1.3964844×1.6 to 1.13.2, and the donkey is 1.5 high from 1.14; `native_ridden_mob_state` reports them.

#### D-128 · Skeleton horse 0.96 water drag before 1.13
- **Versions:** 1.8.8-1.12.2 (adapter path 1.11.2-1.12.2) · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the 0.96F override (`func_189749_co`) appears in 1.13; earlier releases use 0.8F (`origin/client1.13.2 EntitySkeletonHorse.java:131`; yarn 1.10.2-1.12.2 `getWaterSlowDown` 0.8F).
- **Native:** `slow_down = SkeletonHorse ? 0.96 : 0.8` with no version gate (`native/src/ridden.rs:874-878`).
- **Impact:** about 5× higher swim terminal speed.
- **todo.md:** 1071 (open)
- **Fix:** fixed — the skeleton horse's 0.96F water slow-down applies only from 1.13; earlier releases use 0.8F (checked 1.8.9-1.12.2).

#### D-129 · Ridden mob in water gets the ladder 0.2 lift before 1.14
- **Versions:** 1.8.8-1.13.2 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** the water branch has no climbable check before 1.14 (only the 0.3 hop) (`origin/client1.13.2 EntityLivingBase.java:1637-1676`; `1.14.4 LivingEntity.java:1880`).
- **Native:** `if collided_horizontally && climbable { y = 0.2 }` with no version gate (`native/src/ridden.rs:870-873`). The player path gates it at 1.14.
- **Impact:** a mount pushed against a ladder in water rises at 0.2 per tick.
- **todo.md:** 1059 (open)
- **Fix:** fixed — the 0.2 ladder climb in water is gated at 1.14 in `travel_in_fluid`, as on the player path.

#### D-130 · 1.9.4-1.10.2 pig boost: the tick the boost ends still adds the boost term
- **Versions:** 1.9.4, 1.10.2 · **Severity:** minor · **Status:** confirmed (yarn)
- **Vanilla:** `if (boostTime++ > total) boosting = false; h += h*1.15F*sin(...)` in the same block; 1.11 moved the check out (yarn 1.9.4 `PigEntity.travel:199-225`).
- **Native:** the 1.11+ order (`native/src/ridden.rs:426-431, 545-549`).
- **Impact:** about 5% speed difference on the last boost tick.
- **todo.md:** 1072 (open)
- **Fix:** fixed — `steering_speed` takes a `boosted` flag that, before 1.11, includes the boost that ended this tick, so 1.9.4-1.10.2 still add the boost term on that tick.

#### D-131 · Strider runs checkInsideBlocks three times per tick in 1.16-1.21.1
- **Versions:** 1.16.4-1.21.1 (1.16.0-1.16.3 likely) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Strider.checkFallDamage` and `Strider.tick` each call `checkInsideBlocks`, in addition to `Entity.move`; removed in 1.21.2 (`origin/client1.16.4 Strider.java:273-296`; `1.20.4 :297-337`).
- **Native:** applies the effects once (`native/src/ridden.rs:920-926`).
- **Impact:** a strider in a bubble column accelerates about 3× faster in vanilla.
- **todo.md:** 1072 (open)
- **Fix:** fixed — new `MoveRules.checks_inside_at_fall` runs a 1.16-1.21.1 strider's inside effects at its checkFallDamage point, and `after_ridden_travel` runs them again after floatStrider; the sources also show `ItemSteerable.travel`'s extra tryCheckBlockCollision in 1.17-1.19.3 for steered pigs and striders, now modelled too.

#### D-132 · 1.11-1.18.2 stickless pig or strider is not client-controlled in the plugin; native's unsteered strider is wrong
- **Versions:** pig 1.11.2-1.18.2, strider 1.16.4-1.18.2 · **Severity:** minor · **Status:** partially-confirmed
- **Vanilla:** `getControllingPassenger = getFirstPassenger` until 1.19. A stickless rider still drives the mob on the client: own zza, step 0.5, flyingSpeed 0.02, and a strider resets speed to getMoveSpeed every tick. (`origin/client1.16.4 Pig.java:79-80`, `Strider.java:190, 244-247`, `ItemSteerable.java`)
- **Native:** `mineflayer/lib/plugins/physics.js:1213-1217, 1254` requires the stick from 1.11. Native's unsteered strider uses step 1.0, air speed `ridden_speed*0.1` and a decaying zza (`native/src/ridden.rs:292-298, 420-423, 741-747`); this is latent.
- **Impact:** no client prediction or vehicle move for a stickless rider.
- **todo.md:** 1072 (open)
- **Fix:** not a deviation — in 1.11-1.18 `MobEntity.canPassengerSteer` is `canBeControlledByRider() && super…` (1.11.2 `MobEntity.java:1087-1089`, 1.18 `:1274-1275`) and pig/strider `canBeControlledByRider` require the stick (1.12.2 `PigEntity.java:80-87`, 1.16 `StriderEntity.java:193-201`), so the client never moves a stickless mount; recordings 1.11.2-1.18.2 confirm it, and the plugin's <1.11 gate is kept with a new replay test.

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
- **Fix:** fixed — the smoke test now expects float sizes (`Math.fround(0.2)`, `Math.fround(1.8)`), ids 6 and 7 with no version still throw as ambiguous, and `createSimulationState` rejects every pose for 1.8.8-1.13.2 ("entity poses are unavailable before 1.14"), as vanilla has no `EntityPose` before 1.14; a Rust test covers float heights, the ambiguous ids and the pre-1.14 rejection.

#### D-154 · simulateVersionedTicks ignores rockets, nearby entities and moving pistons
- **Versions:** all · **Severity:** minor · **Status:** confirmed
- **Vanilla:** attached rockets boost and entities push every tick.
- **Native:** the bulk path calls only `versioned::dispatch` (`native/src/napi_api.rs:1698-1725`, compare 1530-1639).
- **Impact:** N bulk ticks differ from N single ticks.
- **todo.md:** 1149 `[x]` (partial)
- **Fix:** fixed — before 1.11 simulateVersionedTicks drops a piston after its ending tick (1.8.9 PistonBlockEntity.java:128-136, 1.10.2 :166-171); from 1.11 the ended block pushes nothing and is kept. TickScene carries `rocket_ticks` and counts a tick off each attached rocket, ending its boost (napi_api.rs LocalPlayerScene). Tests: `bulk_ticks_drop_a_piston_after_its_ending_tick_before_1_11`, `bulk_ticks_end_the_attached_rockets_after_their_ticks`.

#### D-155 · Generic N-API state defaults flyingSpeed to 0.02
- **Versions:** all (direct callers only) · **Severity:** minor · **Status:** confirmed
- **Vanilla:** `Abilities.flyingSpeed = 0.05F` (`origin/client1.21.11 Abilities.java:19`).
- **Native:** `flying_speed.unwrap_or(0.02)` (`native/src/napi_api.rs:675`; `simulation.rs:386`).
- **Impact:** flight is 2.5× too slow when the field is omitted.
- **todo.md:** 1145 `[x]` (holds otherwise)
- **Fix:** fixed — the generic N-API, `SimulationState` and test-encoder defaults are now `f64::from(0.05_f32)`, matching `flySpeed`/`flyingSpeed = 0.05F` in every decompiled release from 1.8.9 to 1.21.11.

#### D-156 · versionCapabilities reports water movement efficiency from 1.21.1
- **Versions:** 1.21 · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** 1.21.0 registers and uses WATER_MOVEMENT_EFFICIENCY (`origin/client1.21 LivingEntity.java:309, 2130`).
- **Native:** `at_least(1, 21, 1)`, with a test locking in the wrong value (`native/src/version.rs:40, 146-150`). The 1.21 travel profile is correct.
- **Impact:** only callers of `versionCapabilities`.
- **todo.md:** 21 `[x]` (partial)
- **Fix:** fixed — `water_movement_efficiency` is now `at_least(1, 21, 0)`; the test asserts true for 1.21.0 and false for 1.20.6.

#### D-157 · The behaviour contract registry is not consulted by dispatch, and it is circular
- **Versions:** all · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** n/a.
- **Native:** `dispatch` matches the version string only. `contract_for_version` mostly reads back `Profile::rules()` and is used only by one test (`native/src/versioned/mod.rs:126-187, 256-761, 1038-1180`).
- **Impact:** it could not catch D-004.
- **todo.md:** 41 `[x]` (partial)
- **Fix:** fixed — `VersionBehaviorContract::for_release` derives the contract from the release alone (travel split 1.21.2, gravity check 1.20.5, efficiency attribute 1.21, fall-damage Floor 1.21.5), and every dispatch arm goes through `tick_conforming`, which refuses a profile whose rules or landing rounding disagree; a D-004-style profile is rejected.

#### D-158 · Profile fields are dead or overwritten
- **Versions:** all · **Severity:** cosmetic · **Status:** confirmed
- **Vanilla:** the divisor is 16 in every 1.13+ release (`origin/client1.20.4 LivingEntity.java:2227`).
- **Native:** `water_gravity_divisor` is never read (a literal `/16.0` is used); gravity and step height are overwritten by the config; the Default rules carry 5.0 and 0.98 (`native/src/simulation.rs:1056-1061, 2118-2129, 130-155`).
- **Impact:** none today; future profile edits would have no effect.
- **todo.md:** 285 `[x]` (deviates), 283 `[x]` (non-physics-false)
- **Fix:** fixed — the water and shallow-lava paths now read `rules.water_gravity_divisor` (16.0 in every 1.13+ source; unused before 1.13, where a literal 0.02 is subtracted), default config values no longer overwrite profile gravity and step height, and the Default rules carry 16.0 and 0.91.

#### D-159 · physics-breakpoints.json is stale
- **Versions:** manifest · **Severity:** cosmetic · **Status:** confirmed
- **Native:** it is missing the transitions 1.20.6→1.21, 1.21.1→1.21.2, 1.21.3→1.21.4, 1.21.5→1.21.6, 1.21.8→1.21.9, 1.21.9→1.21.10, 1.21.11→26.1 and 26.1.2→26.2, and lists non-adjacent ones. It only compares the hashes of 3 methods (`generated/physics-breakpoints.json`; `scripts/report-physics-breakpoints.cjs`).
- **Impact:** wrong documentation.
- **todo.md:** 337 `[x]` (non-physics-false)
- **Fix:** fixed — the report script now compares all 18 tracked methods across adjacent audit releases only, lists missing files as `unavailable`, and the regenerated schema-2 report has 26 breakpoints including every transition named above; coverage still starts at 1.14.4 because the older source root is not available.

#### D-160 · The claim that TypeScript validates the version before constructing an engine is false
- **Versions:** all · **Severity:** cosmetic · **Status:** confirmed
- **Native:** `NativePhysics`, `NativeBoatPhysics` and `NativeRiddenMobPhysics` construct for any release. Rust rejects unsupported versions only at the first tick (`src/mineflayer/base.ts:60-65`, `player.ts:117-120, 319`; `native/src/napi_player.rs:129-133`).
- **Impact:** the error appears later than documented. No physics effect.
- **todo.md:** 1158 `[x]` (non-physics-false)
- **Fix:** fixed — the constructors check `isSupportedVersion` only when the binding provides it (now optional in `NativePhysicsBinding`), so older prebuilt bindings construct and fail at the first tick while current ones refuse at construction; a test covers a binding without the method.

---

## 3. Disputed

These are claims on which verifiers, or auditors, reached opposite conclusions. Neither is counted among the 160 deviations.

### X-1 · Client unloaded-chunk sink (-0.1) limited to releases before 1.14
- **Claim:** `LivingEntity.travel`/`travelInAir` keeps `else if (isClientSide && !hasChunkAt(posBelow)) vy = y > minY ? -0.1 : 0` through 26.3. Native applies it only before 1.14 (`native/src/simulation.rs:2716-2725`).
- **Against (3 verifiers):** the client level's `hasChunk(int,int)` returns true in every release from 1.14 (`origin/client1.14.4 MultiPlayerLevel.java:232`; `ClientLevel.java` in 1.16.4:220, 1.20.4:312, 1.21.11:381, 26.3:490), and `LevelReader.hasChunkAt` only forwards to it. So the branch cannot run on a 1.14+ client, and the pre-1.21.2 `LocalPlayer.tick` chunk guard can never fail. Native's pre-1.14 gate is correct (1.8.9 and 1.13.2 check `Chunk.isLoaded()`).
- **For (1 verifier, partially-confirmed major):** in 1.21.2-26.3 the `LocalPlayer.tick` chunk guard is gone, so the sink would apply after teleports. It also argued that 1.8.8-1.21.1 lack the tick-freeze while the player's column is unloaded (`origin/client1.8.9 EntityPlayerSP.java:104-105`). That verifier did not address the `hasChunk → true` override.
- **Assessment:** the refutation evidence is direct and specific. For 1.14+ this is most likely no deviation. The pre-1.14 tick freeze (vanilla `EntityPlayerSP.onUpdate` skipped while `isBlockLoaded` is false) would be a gate in the Mineflayer plugin, not in the engine; it is left unresolved.
- **Fix:** not a deviation — the client level's chunk check (`isChunkLoaded(int,int)`, `hasChunk` from 1.21.5) returns true in every 1.14-26.3 source checked (1.14 through 1.21.11 and 26.3), and every BlockPos overload forwards to it, so the -0.1 sink cannot run on a 1.14+ client; native's pre-1.14 gate matches the `blockExists`/`Chunk.isLoaded()` check of 1.9.4-1.13.2.

### X-2 · Piston-driven moves of the local player skip `updateAutoJump`
- **For (gap-pistons auditor, partially-confirmed minor):** `LocalPlayer.move` runs `updateAutoJump` after every move, piston moves included (`origin/client1.21.11 LocalPlayer.java:912-920`). Native's piston path (`native/src/piston.rs:321-368`) never does. This is observable only for a **downward** push that leaves the player on the ground.
- **Against (gap-autojump auditor, refuted):** `canAutoJump` requires `onGround`. Horizontal and upward piston moves always clear onGround, and for a downward push the moving piston block fills the head cell or the cell above it, so the ceiling check returns early. The vanilla call is therefore always a no-op (verified on 1.13.2, 1.21.11 and 26.3).
- **Assessment:** the refutation covers the downward case with a concrete ceiling argument. Most likely no observable deviation.
- **Fix:** deferred — kept the partial fix: before 1.13 piston moves return before `updateAutoJump`, which is exact when the pressing block is in the player's centre column; vanilla 1.10.2-1.12.2 cancels only for a moving-piston tile entity at the centre x/z, so an edge overlap still differs, and that needs the ceiling probe to see moving-piston cells (D-099). From 1.13 behaviour matches vanilla.

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
| 388 | TickInputSnapshot is complete | partial | Leaves out boosts, pushes, pistons. Now fixed: `TickScene` carries them, and `for_profile` takes each profile's own restitution and fall-damage policy (`VersionPhysicsProfile::restitution_enabled` / `modern_fall_damage`), checked against every release's engine |
| 389 | TickOutputSnapshot | partial | Lacks pushes and piston deltas. Now fixed: pushes, piston deltas, the spin-attack end and the next tick's scene (`TickScene::advance`), shared with simulateVersionedTick (`tick_scene`) and returned by `simulateVersionedTickOutcome` |
| 393 | Clone/merge keep gravity/step/no-gravity | partial | No no-gravity field. EntityState now carries and merges it; PlayerState is mineflayer-physics-utils' class, whose clone()/merge() (src/physics/states/playerState.ts:580-672) still drop an explicit noGravity |
| 395 | Import the no-gravity flag | non-physics-false | Not read by the adapter (D-057). Now fixed for the player and mounts (D-057) |
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
| 509 | collide sub-1e-7 cutoff | partial | Per box; the 1.14-1.16 path is missing (D-074, D-075). Now fixed (D-074, D-075) |
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
| 966 | Freeze-tick accumulation | partial | Pre-move box; server-side in vanilla (D-024). Now fixed (D-024) |
| 973 | Cactus/campfire/magma contact damage | partial | Magma gate (D-023) |
| 978 | Fire clears freeze | partial | Lava and lava cauldron also clear it; pre-1.21.2 burning (D-024). Now fixed (D-024) |
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


## 8. Vanilla-source re-audit (2026-11-03)

A second audit ran on 2026-11-03 against the `parity/todo` branch. It used three source sets: the Mojang-named `origin/client<ver>` branches of `extracted_minecraft_data`, the Yarn-named decompiles in `E:/pr/mc-decompiled` (1.7-1.21.11, which fill 1.9.4-1.12.2, 1.15, 1.16.x, 1.17 and 1.18), and `E:/pr/mc-source` (26.2, 26.3). One auditor per area (20 areas) compared the engine with vanilla across the release eras. Two independent verifiers then checked each claim. One re-read both sides of the code; the other checked the release range and whether an earlier D-### entry already covers the claim.

- **Confirmed** (both verifiers): 60 entries after merging duplicates. **Plausible** (one verifier): 9. **Refuted**: 9 (8.4).
- Severity uses the scale from section 1. `minor (low)` marks rare cases, numeric edge cases, or cases reachable only through N-API inputs that the Mineflayer adapter never sends.
- Entries tagged *relates to D-###* extend or reopen an earlier entry; the rest are new.
- Caveat: `origin/client1.16.4` contains a 20w45a (1.17 snapshot) tree, so 1.16-specific citations that rely on it were bracketed with the decompiled 1.16/1.16.5 jars.

### 8.1 Confirmed, by severity

#### D-161 · Attribute keys never match what Mineflayer stores on 1.21+, so speed/slowness effects, Depth Strider (water_movement_efficiency), gravity, step height, jump strength, scale and the other attributes are silently dropped
- **Versions:** 1.21-1.21.11, 26.1 (1.20.5/1.20.6 unaffected; 26.1.1+ unverified, since minecraft-data 3.117.0 has no data for them) · **Severity:** major · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** The client's AttributeInstance for MOVEMENT_SPEED holds every modifier the server syncs (speed/slowness effect modifiers 'effect.speed' +0.2*(amp+1) and 'effect.slowness' -0.15*(amp+1) ADD_MULTIPLIED_TOTAL), and getSpeed()/getAttributeValue(WATER_MOVEMENT_EFFICIENCY)/GRAVITY/STEP_HEIGHT/JUMP_STRENGTH read the folded value. (`origin/client1.21.11 client/net/minecraft/world/effect/MobEffects.java:69-70; LivingEntity.java:2225 getJumpPower, :2352 travelInWater; AttributeInstance.java:150-172`)
- **Native:** attributeKey builds the key from minecraft-data attributes.json `resource`: `attributeKey = !data ? null : namespaced && ... ? `minecraft:${data.resource}` : data.resource`. On 1.21/1.21.1 that is 'minecraft:generic.movement_speed', on 1.21.3-26.1 'minecraft:movement_speed'. Mineflayer 4.37.1 stores attributes under the protocol mapper string (`entity.attributes[prop.key ?? prop.name]`, entities.js:743), which the installed minecraft-data 3.117.0 protocol maps to 'generic.movement_speed', 'generic.gravity', 'generic.step_height', 'generic.jump_strength', and so on. A script over every release flagged movementSpeed, gravity, stepHeight, jumpStrength, waterMovementEfficiency, movementEfficiency, safeFallDistance, sneakingSpeed, scale, burningTime and fallDamageMultiplier as missing from the mapping for 1.21 through 26.1. So attributeValueOf returns undefined. setSprinting then writes a synthetic `state.attributes['minecraft:movement_speed'] = { value: synced?.value ?? f32(0.1), modifiers: [sprint] }` that carries no effect modifiers. The result: Speed/Slowness are ignored, the engine's fallback movement_speed_attribute gets no speedEffectAmplifier from the adapter, water_movement_efficiency stays 0 (Depth Strider does nothing on 1.21+), and gravity/step/jump attribute modifiers are lost. The adapter's tests build attributes with physics.attributeKey(...), so they cannot catch this. (`src/mineflayer/base.ts:158-173 (attributeKey/attribute), player.ts:478-516 (attributesAndEffects), player.ts:1022-1030 (setSprinting)`)

#### D-162 · Depth Strider (and Soul Speed) level comes from the helmet slot via mineflayer-physics-util's PlayerState
- **Versions:** 1.8.8-1.20.6 (Depth Strider on boots ignored; on a helmet vanilla counts it too, via ARMOR_SLOTS); Soul Speed 1.16-1.20.6 (boots ignored, helmet wrongly counted) · **Severity:** major · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** travel: `float var33 = (float)EnchantmentHelper.getDepthStrider(this); if (var33 > 3.0F) var33 = 3.0F; ...`. getDepthStrider = getEquipmentLevel(DEPTH_STRIDER, entity), which reads only the enchantment's FEET slot. In 1.8.9 it is getLevel(id, entity.getArmorStacks()), the highest level on any armor piece. (`origin/client1.20.4 client/net/minecraft/world/entity/LivingEntity.java:2060-2072 travel; E:/pr/mc-decompiled/1.12.2,1.16.5,1.20.5 EnchantmentHelper.getDepthStrider; 1.8.9 EnchantmentHelper.java:182`)
- **Native:** The adapter passes `state.depthStrider` unchanged. mineflayer-physics-util's PlayerState.update sets it from `const boots = bot.entity.equipment[5]` (playerState.js:262-266). Mineflayer's own-entity equipment getter is `[heldItem, slots[45], slots[8], slots[7], slots[6], slots[5]]` (inventory.js:64-66), so index 5 is the helmet. On 1.8 the array has 5 entries, so it is undefined and depthStrider is always 0. Boots Depth Strider does nothing and a Depth Strider helmet takes effect. The adapter already reads boots correctly for Frost Walker (bootsEnchantmentLevel via getEquipmentDestSlot('feet')) but not for depth_strider/soul_speed. (`src/mineflayer/player.ts:388-391 (`depthStrider: state.depthStrider, soulSpeed: state.soulSpeed`), consumed at native/src/simulation.rs:4091-4096`)

#### D-163 · 1.8.8 and 1.9.4: farmland collides as a full cube in vanilla, but the engine uses the 15/16 box
- **Versions:** 1.8.8, 1.9.4 · **Severity:** major · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 1.8.9: `public AxisAlignedBB func_180640_a(World w, BlockPos p, IBlockState s) { return new AxisAlignedBB(x, y, z, x + 1, y + 1, z + 1); }`, a full cube while the block bounds are 0.9375. 1.9.4: `public Box getCollisionBox(BlockState state, World world, BlockPos pos) { return collisionBox; }`, where Block.collisionBox = `new Box(0,0,0,1,1,1)` (FULL_BLOCK_AABB). From 1.10 the override is gone, so the collision is the 0.9375 bounding box. (`origin/client1.8.9:client/net/minecraft/block/BlockFarmland.java:32-34 (getCollisionBoundingBox); E:/pr/mc-decompiled/1.9.4/net/minecraft/block/FarmlandBlock.java:53-56 (getCollisionBox(BlockState, World, BlockPos)) and 1.9.4 Block.java:47; 1.10.2 FarmlandBlock.java has no such override`)
- **Native:** minecraft-data gives 1.8 and 1.9 farmland [[0,0,0,1,0.9375,1]], the same as 1.10+ (the pc/1.8 to pc/1.12 shape files are identical for every block). The adapter's legacy override table (stairs, doors, chests, chorus, the 1-layer snow) has no farmland case, so the engine collides with 0.9375. A player on farmland stands at y+0.9375 instead of y+1.0 in 1.8.8 and 1.9.4, and step-up and edge contacts shift by 1/16. (`src/mineflayer/blocks.ts:191 (describeBlock `boxes: block.shapes.map(...)`), blocks.ts:482 (`this.legacy.shapes(...) ?? described.boxes`); src/mineflayer/legacy.ts:242-257 (shapes() has no farmland case); minecraft-data pc/1.8 and pc/1.9 blockCollisionShapes farmland = [[0,0,0,1,0.9375,1]]`)

#### D-164 · Trident has no in-ground state: after a block hit it keeps flying or falling on later ticks
- **Versions:** 1.13.2-26.3 · **Severity:** major · **Kind:** missing · **Status:** confirmed
- **Vanilla:** onHitBlock sets `this.setInGround(true)` (≤1.21.1 `this.inGround = true`). On the next tick AbstractArrow.tick takes `if (this.isInGround() && physicsEnabled) { ... ++this.inGroundTime; ... }`, which runs no clip, no move, no inertia and no gravity. A trident is an AbstractArrow, so it stays where it stuck. (`origin/client1.21.1 AbstractArrow.java:192-199 (tick, inGround branch); 26.2 projectile/arrow/AbstractArrow.java:192-209, onHitBlock :536-554; ThrownTrident.tick delegates to super.tick()`)
- **Native:** `tick_trident_in_release` always calls `let hit = arrow_flight(&mut state.position, &mut state.velocity, ...)` and returns only `block_hit: hit.is_some()`. From 1.21.2 a hit leaves v=0, and on the next tick arrow_flight runs `velocity.y -= motion.gravity` and moves the trident. Before 1.21.2 the hit tick leaves v=(offset*0.99, -gravity), and the trident keeps travelling. In a multi-tick versioned call, every trident that hits a block drifts away from its stuck position. (`native/src/projectile.rs:355-427 (TridentState has no in_ground field; tick_trident_in_release calls arrow_flight unconditionally); native/src/napi_api.rs:3017-3044 (simulate_versioned_trident_ticks loops this tick for N ticks)`)

#### D-165 · Before 1.15 the block stepOn (slime slow-down, magma damage) runs for a flying player; vanilla skips it because Player.makeStepSound/canTriggerWalking is !flying
- **Versions:** 1.8.8-1.14.4 · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 1.8.9-1.14.4 call stepOn only inside the step-sound block: `if (this.makeStepSound() && (!this.onGround || !this.isSneaking() || !(this instanceof Player)) && !this.isPassenger()) { ... if (this.onGround) { var23.stepOn(this.level, var7, this); } ...}`, and Player has `protected boolean makeStepSound() { return !this.abilities.flying; }` (1.8.9 `func_70041_e_() { return !this.field_71075_bZ.field_75100_b; }`, 1.12.2 `canClimb() { return !this.abilities.flying; }`). So on the tick a flying player lands, while flying is still true, a slime under it does not scale x/z by 0.4+|vy|*0.2, and magma does no damage. From 1.15 the gate is only `if (this.onGround && !this.bypassesSteppingEffects()) lv5.onSteppedOn(...)`. (`origin/client1.14.4 world/entity/Entity.java move ~:533-542 + player/Player.java:1597-1599; origin/client1.8.9 entity/Entity.java moveEntity + EntityPlayer.java:1524-1526; mc-decompiled 1.12.2 Entity.java:838-839 + PlayerEntity.java:1649-1651; 1.15 Entity.java:585-586 (gate removed)`)
- **Native:** `if state.on_ground { match world.effect_at(landing_position(...)) { BlockEffect::Slime if !stepping_carefully && state.velocity.y.abs() < 0.1 => { state.velocity.x *= step_factor; state.velocity.z *= step_factor; } BlockEffect::Magma if !stepping_carefully && !state.frost_walker => { state.contact_damage += 1.0; } ... } }`. It has no flying test in any release, so a flying player that touches down slowly on slime is slowed horizontally, and gets magma damage, on 1.8.8-1.14.4. The flag is cleared only at :4513, after this. (`native/src/simulation.rs:1510-1538 (apply_block_effects), called from :4389-4397 (move, <1.21.2) and :3385-3391 (riptide lift)`)

#### D-166 · 26.3 containsAnyLiquid scans an inclusive floor(min)..=floor(max) box, but the engine keeps the floor..ceil-exclusive scan, so the 0.3 fluid hop sees one more column after a +X/+Z wall hit
- **Versions:** 26.3 · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 26.3: `return this.findBlocksIn(box).filterState((state) -> !state.getFluidState().isEmpty()).anyMatched();` with `findBlocksIn(AABB box) { return this.findBlocksIn(BlockPos.containing(box.getMinPosition()), BlockPos.containing(box.getMaxPosition())); }`. BlockScanUtils.findBlocks then walks minX..=maxX (inclusive). 26.2 and earlier: `int x1 = Mth.ceil(box.maxX); ... for(int x = x0; x < x1; ++x)` (exclusive). jumpOutOfFluid's `isFree(...)` is `noCollision(this, box) && !containsAnyLiquid(box)` (26.3 Entity.java:711-713). (`E:/pr/mc-source/26.3 client/net/minecraft/world/level/LevelReader.java:142-144 + :158-160, blockscan/BlockScanUtils.java:17-24 (inclusive); old form in 26.2 LevelReader.java:138-150 and the diff E:/pr/mc-source/26.2_to_26.3.diff:115701-115722`)
- **Native:** `(bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32).any(|x| ... (bounds.min.y.floor()..bounds.max.y.ceil()) ... (bounds.min.z.floor()..bounds.max.z.ceil())` is the same for every release. A hop box whose max is a whole number (the normal case after a +X or +Z horizontal collision, where the box max sits exactly on the wall face and vx/vz are 0) leaves out the cell column at floor(max) that 26.3 includes. Example: swimming +X into a ledge with water on top of it (column x = wall). 26.3 refuses the 0.3 hop, and the engine grants it. The same gap reaches every other caller of contains_any_liquid / is_free_for_entity on 26.3, such as the ridden-mob hop. (`native/src/world.rs:1801-1808 (contains_any_liquid), used by is_free_for_entity :1425-1432 from simulation.rs:1198-1223 fluid_travel_hop`)

#### D-167 · Attributes added in 1.20.5 (gravity, jump_strength, step_height, scale, safe_fall_distance) are never resolved on 1.20.5-1.20.6
- **Versions:** 1.20.5-1.20.6 · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 1.20.5 adds GRAVITY, JUMP_STRENGTH, STEP_HEIGHT, SCALE and SAFE_FALL_DISTANCE. They are synced (packet ids 11 generic.gravity, 12 generic.jump_strength, 18 generic.safe_fall_distance, 19 generic.scale, 21 generic.step_height). travel uses `this.getGravity()` and getJumpPower uses `(float)this.getAttributeValue(Attributes.JUMP_STRENGTH)`. Jump Boost adds +1 SAFE_FALL_DISTANCE per level. (`origin/client1.20.6 client/net/minecraft/world/entity/LivingEntity.java:2107 travel getGravity; origin/client1.20.5 LivingEntity.java:2063-2065 getJumpPower; origin/client1.20.5 MobEffects.java:77`)
- **Native:** minecraft-data dataPaths maps the 1.20.5 and 1.20.6 attributes to pc/1.17, which has no gravity/jumpStrength/stepHeight/scale/safeFallDistance entries. attributeKey returns null and attributeValueOf returns undefined, so the engine uses the defaults (0.08, 0.42, 0.6, scale 1, safe fall 3) and ignores any synced value or modifier. (`src/mineflayer/base.ts:158-166 (attributeKey reads registry.attributesByName); player.ts:209 (scale), 485, 499-501`)

#### D-168 · Projectile block ray trace for 1.8.8-1.13.2 uses the 1.14+ traverseBlocks/AABB.clip instead of the old 200-step rayTraceBlocks with inclusive six-face intercepts
- **Versions:** 1.8.8-1.12.2 (1.13.2 differs only in the 200-step walk vs. traverseBlocks, which rarely or never shows up) · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** Before 1.14 the walk is not traverseBlocks. It starts at floor(start), loops `while (o-- >= 0)` (at most 200 steps) and stops when `l == i && m == j && n == k` (the end cell). Each step moves the start to the face it crosses: `start = new Vec3d(d, start.y + r * g, start.z + s * g)`, with `if (g == -0.0) g = -1.0E-4`. In 1.12.2 the block is then traced from that moved start (`lv7.method_11711(this, lv, start, end)`). The box intercept tests all six faces, exit faces included (`method_12111(this.minX…)`, then `this.maxX`, …), accepts `!(h < 0.0) && !(h > 1.0)` (t from 0 to 1 inclusive), uses inclusive bounds with no epsilon (`vec.y >= this.minY && vec.y <= this.maxY`), skips an axis when `e * e < 1.0E-7F`, and keeps the nearest intercept by squaredDistanceTo. 1.13.2 keeps the 200-step face walk but traces each cell with VoxelShape.rayTrace from the original endpoints. (`E:/pr/mc-decompiled/1.12.2 net/minecraft/world/World.java rayTrace(Vec3d,Vec3d,boolean,boolean,boolean) :882-1008; net/minecraft/util/math/Box.java method_585/method_12111/intersectsYZ :294-368; Vec3d.java method_12124 :112-122; 1.13.2 World.java method_3615 :730-890`)
- **Native:** clip_blocks_between has no release gate. Every release uses the 1.14+ BlockGetter.traverseBlocks: `lerp(-1.0e-7, to.x, from.x)` and `lerp(-1.0e-7, from.x, to.x)` extend both ends, the DDA has no step cap, and each cell goes through `clip_block`. Before 1.13, clip_block skips only the inside-point rule and otherwise uses `bounds.clip_segment(from, to)`. That function tests entering faces only (`let plane = if direction > 0.0 { lower } else { upper }`), requires `0.0 < t && t < best`, and allows ±1e-7 around the bounds. (`native/src/projectile.rs:1168-1250 (clip_blocks_between, clip_block); native/src/math.rs:1927-1971 (Aabb::clip_segment)`)

#### D-169 · 26.2 and 26.3: the generated #fall_damage_resetting and #climbable tags are empty, so the FALLDAMAGE_RESETTING clip never fires
- **Versions:** 26.2, 26.3 · **Severity:** minor · **Kind:** mismatch · **Status:** confirmed · relates to D-003
- **Vanilla:** `this.tag(BlockTags.CLIMBABLE).add(BlockItemIds.LADDER, BlockItemIds.VINE, BlockItemIds.SCAFFOLDING, BlockItemIds.WEEPING_VINES).add(BlockIds.WEEPING_VINES_PLANT).add(BlockItemIds.TWISTING_VINES).add(BlockIds.TWISTING_VINES_PLANT).add(BlockItemIds.GLOW_BERRY_CROP).add(BlockIds.CAVE_VINES_PLANT); this.tag(BlockTags.FALL_DAMAGE_RESETTING).addTag(BlockTags.CLIMBABLE).add(BlockItemIds.SWEET_BERRY_CROP, BlockItemIds.COBWEB);`. These are the same members as 26.1 (ladder, vines, scaffolding, cave vines, sweet berry bush, cobweb). (`E:/pr/mc-source/26.2/client/net/minecraft/data/tags/VanillaBlockTagsProvider.java:71-72; 26.3 same file :72-73`)
- **Native:** The generator does not resolve the 26.2+ `BlockItemIds.*`/`BlockIds.*` provider form. The 26.2/26.3 tables keep only `includes: ["climbable"]` with empty block lists. 26.1.x has ["cobweb","sweet_berry_bush"] and the 9 climbables. `generatedBlockHasTag('26.3', name, 'fall_damage_resetting')` is therefore false for every block. The adapter never sets `fallDamageResetting`, and the D-003 Entity.move clip does not reset fall distance on 26.2/26.3 for a fast fall (≥1 block/tick) through ladders, vines, scaffolding, cobweb or berry bushes. (`native/data/releases/v26_2.rs:113,115 and v26_3.rs:117,119 (`V26_x_CLIMBABLE_BLOCKS: &[&str] = &[]`, `V26_x_FALL_DAMAGE_RESETTING_BLOCKS: &[&str] = &[]`); native/data/mod.rs block_has_tag; src/mineflayer/world.ts:111 (hasTag = binding.generatedBlockHasTag) → blocks.ts:465-472 isFallDamageResetting`)

#### D-170 · 1.8.8: ladder collision box is 2/16 thick in vanilla, 3/16 in the engine
- **Versions:** 1.8.8 · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** getCollisionBoundingBox calls setBlockBoundsBasedOnState: `float var4 = 0.125F; case NORTH: setBlockBounds(0.0F, 0.0F, 1.0F - var4, 1.0F, 1.0F, 1.0F); case SOUTH: (0,0,0,1,1,var4); case WEST: (1.0F - var4,0,0,1,1,1); EAST: (0,0,0,var4,1,1)`, so the box is 0.125 thick. 1.9+ uses `new Box(0.0, 0.0, 0.0, 0.1875, 1.0, 1.0)` etc. (`origin/client1.8.9:client/net/minecraft/block/BlockLadder.java func_180640_a / func_180654_a; 1.9.4 LadderBlock.java:19-22 (3/16 boxes)`)
- **Native:** minecraft-data's 1.8 shapes are the 1.9 ones: 0.1875 (0.8125..1). With no 1.8 override, a 1.8.8 player pressed against or climbing a ladder is held 1/16 further from the ladder's cell face than vanilla. (`src/mineflayer/blocks.ts:191, 482; src/mineflayer/legacy.ts:242-257 (no ladder override); minecraft-data pc/1.8 ladder shapes [[0,0,0.8125,1,1,1]] / [[0,0,0,0.1875,1,1]] …`)

#### D-171 · 1.8.8: lily pad collision box is the full-width 1/64 slab in vanilla, the 1.9 inset 1.5/16 box in the engine
- **Versions:** 1.8.8 · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** `float var1 = 0.5F; float var2 = 0.015625F; this.setBlockBounds(0.5F - var1, 0.0F, 0.5F - var1, 0.5F + var1, var2, 0.5F + var1);`, and getCollisionBoundingBox returns those bounds: (0,0,0)-(1,0.015625,1). It collides with every entity except boats. 1.9 changed it to `new Box(0.0625, 0.0, 0.0625, 0.9375, 0.09375, 0.9375)`. (`origin/client1.8.9:client/net/minecraft/block/BlockLilyPad.java:18-20 (ctor bounds), :31-33 (getCollisionBoundingBox), :24-29 (addCollisionBoxesToList skips only boats); 1.9.4 LilyPadBlock.java:15`)
- **Native:** Uses the 1.9 box from minecraft-data. A 1.8.8 player on a lily pad stands at y+0.09375 instead of y+0.015625, and collides with the 1/16 border differently. (`src/mineflayer/blocks.ts:191, 482; legacy.ts shapes() (no waterlily case); minecraft-data pc/1.8 waterlily [[0.0625,0,0.0625,0.9375,0.09375,0.9375]]`)

#### D-172 · A piston move cut to zero by limitPistonMovement skips the block effects that vanilla still applies (1.21.2+)
- **Versions:** 1.21.2-26.3 · **Severity:** minor · **Kind:** mismatch · **Status:** confirmed · relates to D-105
- **Vanilla:** moveEntityByPiston calls applyEffectsFromBlocks whatever happens inside move(): 1.21.5+/26.3 `Vec3 var5 = var1.position(); var1.move(MoverType.PISTON, ...); var1.applyEffectsFromBlocks(var5, var1.position()); var1.removeLatestMovementRecording();`, and 1.21.2-1.21.4 `var1.move(MoverType.PISTON, ...); var1.applyEffectsFromBlocks();`. Entity.move returns early (`var2 = this.limitPistonMovement(var2); if (var2.equals(Vec3.ZERO)) return;`), but the effects still run. applyEffectsFromBlocks(List) runs stepOn when onGround (slime: x/z motion *= 0.4+|vy|*0.2) and checkInsideBlocks (bubble column, honey slide, cobweb/berry stuck multiplier, powder snow) over position->position from 1.21.5, or oldPosition()->position on 1.21.2-1.21.4. (`origin/client1.21.11 PistonMovingBlockEntity.java moveEntityByPiston (~:171-177); origin/client1.21.3 PistonMovingBlockEntity.java:176-181 and Entity.java:776-790; Entity.java (1.21.11) :736-740 and :898-920`)
- **Native:** After the 0.51 clamp the engine returns before the effects block: `if amount.abs() <= 9.999999747378752e-6 { return; }` (:463-465) and `if movement == Vec3::ZERO { return; }` (:474-476). So `blocks_inside_moves` / `apply_block_effects` / `apply_powder_snow` run only when the limited movement is non-zero. (`native/src/piston.rs:453-476 (move_by_piston), effects at :479-498`)

#### D-173 · 1.13.x getWaterLevelAbove adds the block's y to the fluid height inside the max, so any water layer at y>=1 counts as full
- **Versions:** 1.13.2 · **Severity:** minor · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** 1.13.2: `var11 = Math.max(var11, (float)var10 + var14.func_206885_f()); if (var11 >= 1.0F) continue label160; ... if (var11 < 1.0F) return (float)var8.getY() + var11; ... return (float)(var5 + 1);`. blockHeight is absolute (y + height), so at any y >= 1 a layer with water is skipped as full. The result is the first layer with no water at all (returned as y + 0), or maxY + 1. 1.14.4 and later use `Math.max(var11, var14.getHeight(level, pos))`. (`origin/client1.13.2 client/net/minecraft/entity/item/EntityBoat.java func_184451_k (getWaterLevelAbove) ~:399-436; E:/pr/mc-decompiled/1.13.2 BoatEntity.method_11332 :433-468; fixed in 1.14.4 Boat.java:419`)
- **Native:** Uses the 1.14+ form for every release from 1.9: `height = height.max(water_height(world, (x, y, z), release)); if height >= 1.0 { continue 'layers; } ... return y as f32 + height;`. For 1.13.2, a boat that falls from the air into water whose top layer is in the scanned range snaps to y + h - 0.5625 + 0.101. Vanilla moves it to the next empty layer (y+1), or to maxY+1. The entering snap height differs by up to about 1 block. (`native/src/vehicle.rs:587-609 (water_level_above), used by float_boat at :714`)

#### D-174 · 1.13.x boat status scans use the 'same fluid above = 1.0' height; 1.13.2 FluidState.getHeight is level/9 only
- **Versions:** 1.13.2 · **Severity:** minor · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** 1.13.2 boat scans call `var16.func_206885_f()`, which is `getFluid().func_207181_a(this)` = `(float)var1.func_206882_g() / 9.0F`. It takes no world and has no rule that water above gives 1.0, so a source under water is 8/9 high. Example: `var2 < (double)((float)y + var16.func_206885_f())` in isUnderwater, and `(float)var12 + var14.func_206885_f()` in checkInWater. The world-aware getHeight(level,pos) with the above-rule starts in 1.14. (`origin/client1.13.2 EntityBoat.java func_184444_v (isUnderwater) :~561-575, func_184446_u (checkInWater) :~515-531, func_184451_k; IFluidState.func_206885_f :28-30 -> FlowingFluid.func_207181_a :465-467`)
- **Native:** For 1.13.2, water_height returns world.fluid_height_at, and fluid_height_at gives `if fluids_match(block.fluid, above.fluid) { return 1.0; }`. A boat in a water column whose top is in (y+0.889, y+0.999) is UNDER_WATER (friction 0.45, buoyancy 0.01) in native but not in vanilla. checkInWater's waterLevel (buoyancy) and getWaterLevelAbove differ the same way. The engine already models the 1.13 own-height rule elsewhere (legacy_fluid::own_height, projectile::own_water_height), but not in the boat scans. (`native/src/vehicle.rs:496-499 (water_height: `if release.at_least(1, 13, 0) { return world.fluid_height_at(position) as f32; }`), used by underwater_status :548, water_level_in :576, water_level_above :599; world.rs:776-792 (fluid_height_at returns 1.0 when the block above has the same fluid)`)

#### D-175 · Ridden horses (and the 1.19.3 camel) skip AbstractHorse.travel's extra tryCheckInsideBlocks in 1.17-1.19.3
- **Versions:** 1.17-1.19.3 (horse, donkey, mule, skeleton horse, zombie horse; camel in 1.19.3 only) · **Severity:** minor · **Kind:** missing · **Status:** confirmed · relates to D-131
- **Vanilla:** In the steered branch, after super.travel: `if (this.onGround) { this.playerJumpPendingScale = 0.0F; this.setIsJumping(false); } this.calculateEntityAnimation(this, false); this.tryCheckInsideBlocks();`. So entityInside runs a second time on every steered tick, after the move's own checkInsideBlocks. In 1.19.3 Camel.travel calls super.travel, which is AbstractHorse.travel, so the camel gets it too. This is the same extra pass ItemSteerable.travel has for pigs and striders. (`origin/client1.18.1 client/net/minecraft/world/entity/animal/horse/AbstractHorse.java travel (:724-730); origin/client1.19.3 AbstractHorse.java travel (:769-775); E:/pr/mc-decompiled/1.17 net/minecraft/entity/passive/HorseBaseEntity.java:759-765, 1.19 AbstractHorseEntity.java:763; none in 1.16.4/1.16.5 or 1.19.4 (from 1.19.4 travelRidden only calls it in the not-controlled branch)`)
- **Native:** Only pigs and striders get the second pass: `let steered = matches!(mob.kind, RiddenMobKind::Pig | RiddenMobKind::Strider) && mob.saddled; if steered && release.at_least(1, 17, 0) && !release.at_least(1, 19, 4) { check_inside_blocks(&mut body, world, release); }`. Horse kinds and the camel get the move's own inside-block effects once. (`native/src/ridden.rs:396-401 (tick_ridden_mob)`)

#### D-176 · Water and lava currents never push projectiles (Entity.baseTick fluid push)
- **Versions:** 1.8.8-26.3 (all registered releases; water current in all, lava current only in later releases, not in 1.8.9) · **Severity:** minor · **Kind:** missing · **Status:** confirmed
- **Vanilla:** Every projectile's super.tick() runs Entity.baseTick: `if (this.isPushedByFluid()) { if (inWater) this.fluidInteraction.applyCurrentTo(FluidTags.WATER, this, 0.014); if (inLava) ... applyCurrentTo(FluidTags.LAVA, this, lavaFlowScale); }`. No projectile overrides isPushedByFluid except AbstractArrow (`return !this.isInGround();`). Thrown items, fireballs, fishing hooks (bobbers drift in flowing water), fireworks, llama spit and flying arrows/tridents are all pushed by the current: at the end of the tick from 1.21.2 (super.tick() after setPos), at the start before that. (`26.2 Entity.java:1651-1672 updateFluidInteraction (called from baseTick :554); 26.2 AbstractArrow.java isPushedByFluid; 1.21.1 Entity.updateInWaterStateAndDoFluidPushing; 1.8.9 Entity.handleWaterMovement`)
- **Native:** No tick takes a flow vector or adds a current impulse; `in_water` only selects the 0.8F/0.6F/0.99F inertia. (`native/src/projectile.rs whole file: ProjectileState/ArrowState/TridentState carry only `in_water: bool`; tick_throwable, tick_hurting_projectile, arrow_flight, tick_fishing_hook, tick_firework, tick_llama_spit and tick_shulker_bullet add no fluid flow`)

#### D-177 · Bubble columns never push arrows, tridents or hurting projectiles (checkInsideBlocks / applyEffectsFromBlocks)
- **Versions:** arrows/tridents 1.13.2-26.3; fireballs, wither skull, dragon fireball and wind charge 1.16.2-26.3 · **Severity:** minor · **Kind:** missing · **Status:** confirmed
- **Vanilla:** BubbleColumnBlock.entityInside calls entity.onAboveBubbleColumn/onInsideBubbleColumn on any entity. The clamped Entity versions apply until 1.21.4, Projectile's unclamped +0.1/+0.06/-0.03 from 1.21.5. Arrows: before 1.21.2 the push comes after inertia and gravity, at the new position; from 1.21.2 it comes after setPos and before air drag and gravity (1.21.2-1.21.3 also before water inertia), and also on in-ground ticks. Hurting projectiles: before 1.21.2 it comes at the old position, before the move takes the pushed motion; from 1.21.2 it comes after setPos (swept old→new position). ShulkerBullet also calls applyEffectsFromBlocks after its move. (`1.13.2 EntityArrow.func_70071_h_ (`this.func_70107_b(...); this.func_145775_I();` after gravity); origin/client1.21.1 AbstractArrow.java:277-278; 26.2 AbstractArrow.stepMoveAndHit (`this.setPos(nextLocation); this.applyEffectsFromBlocks(initialPosition, nextLocation);`); origin/client1.16.4 AbstractHurtingProjectile.java:72 `this.checkInsideBlocks();` (also 1.16.5 ExplosiveProjectileEntity:76; absent in 1.14.4/1.16.0); 26.2 AbstractHurtingProjectile.java:85 `this.applyEffectsFromBlocks();``)
- **Native:** Only tick_throwable calls push_by_bubble_column (blocks_inside before 1.21.2, blocks_visited from 1.21.2). arrow_flight ends with `*velocity = velocity.scale(inertia); velocity.y -= motion.gravity;` (≤1.21.1) or `velocity.y -= motion.gravity` (1.21.2+). tick_hurting_projectile ends with `state.velocity = accelerated(state)` or `state.position = ...`. Neither applies any bubble-column effect, and neither does tick_shulker_bullet. (`native/src/projectile.rs:568-637 (arrow_flight: no push_by_bubble_column), :791-833 (tick_hurting_projectile: no push_by_bubble_column)`)

#### D-178 · Arrow tick lacks the start-of-tick 'position inside a block collider sets inGround' check (and the 1.21.5+ velocity reset)
- **Versions:** 1.14.4-26.3 (blockPosition test; velocity reset 1.21.5-26.3); 1.8.8-1.13.2 only via the stored hit block after an arrow has stuck and fallen · **Severity:** minor · **Kind:** missing · **Status:** confirmed
- **Vanilla:** Before anything else: `BlockState blockState = level.getBlockState(blockPos); if (!blockState.isAir() && physicsEnabled) { VoxelShape shape = blockState.getCollisionShape(...); for (AABB aabb : shape.toAabbs()) if (aabb.move(blockPos).contains(position)) { this.setDeltaMovement(Vec3.ZERO) /*1.21.5+*/; this.setInGround(true); break; } }`. Until 1.13.2 the tested block is the stored hit block (xTile/yTile/zTile); from 1.14 it is the arrow's blockPosition. So an arrow whose point is inside a collider becomes in-ground that same tick and does not fly. (`26.2 AbstractArrow.java:166-183; origin/client1.21.2 AbstractArrow.java:166-182; origin/client1.21.5 :170-180; origin/client1.14.4 :160-174; 1.13.2 EntityArrow.func_70071_h_ (xTile block); 1.8.9 EntityArrow tick (Block.getCollisionBox contains)`)
- **Native:** No such test. An ArrowState with in_ground=false whose position is inside a block's collision box (spawned inside a block, block placed on it, or a grazing pull-back that ends in a neighbouring block) runs arrow_flight, moving and gaining gravity. From 1.21.5 its velocity is also not zeroed. (`native/src/projectile.rs:448-505 (tick_arrow_in_release starts with shake_time, then `if state.in_ground`)`)

#### D-179 · Throwables and llama spit clip OUTLINE shapes in 1.14-1.16.1; engine clips colliders
- **Versions:** 1.14.4-1.16.1 · **Severity:** minor · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** From 1.14 until the 1.16.2 rewrite, snowballs, eggs, ender pearls, potions, XP bottles and llama spit test blocks with ClipContext.Block.OUTLINE. They register hits on blocks with no collision shape (tall grass, flowers, torches, etc.), and on fences and walls at their 1.0-tall outline instead of the 1.5-tall collider. (`origin/client1.14.4 ThrowableProjectile.java:127-129 (`ProjectileUtil.getHitResult(this, var1, ..., ClipContext.Block.OUTLINE, true)`), LlamaSpit.java:55-57 (OUTLINE); mc-decompiled 1.15 ThrownEntity.java:129 and 1.16 thrown/ThrownEntity.java:50 (`RayTraceContext.ShapeType.OUTLINE`); origin/client1.16.4 ProjectileUtil.java:28 (COLLIDER only) and 1.16.5 ThrownEntity.java:49`)
- **Native:** `nearest_block_hit` always uses `ClipShape::Collider`. The outline clip is used only for the 1.20.3-1.20.4 wind charge. block_hit (and llama spit's hit) differ for these blocks. Positions are unaffected because before 1.21.2 the throwable moves the full v. (`native/src/projectile.rs:707, :735 (tick_throwable nearest_block_hit), :974 (tick_llama_spit), :1134-1141 nearest_block_hit → ClipShape::Collider`)

#### D-180 · Pre-1.14 projectile block trace: 1.13.2 uses outline shapes, 1.8.8-1.12.2 the selection bounding box (with no-collision blocks for throwables/fireballs/spit) and the legacy traversal
- **Versions:** 1.8.8-1.13.2 · **Severity:** minor · **Kind:** mismatch · **Status:** confirmed · relates to D-144
- **Vanilla:** 1.13.2: the arrow/fishing-hook/fireball (bl=true) clip skips blocks whose collision shape is empty, but clips the OUTLINE shape of the others. Throwables and llama spit (bl=false) clip the outline of every block, including grass and torches. 1.8.8-1.12.2: the trace clips the block's selection bounding box (`getCollisionBox` in yarn 1.12 = the 1.0-tall fence box); throwables, fireballs and llama spit include blocks with no collision box (canCollide → hasCollision() is true). The traversal is the 200-step `999.0`/`-1.0E-4` stepping of World.rayTrace, and AxisAlignedBB.calculateIntercept also returns exit faces. (`mc-decompiled 1.13.2 World.method_3615 (rayTraceBlocks) :730-860 + Block.method_414 :580-583 (`arg.getOutlineShape(arg2, arg3).rayTrace(...)`); 1.12.2 World.rayTrace :882-1010 + Block.method_414 :515-516 (`arg.getCollisionBox(arg2, arg3)` = the selection box, e.g. FenceBlock field_12667 1.0-tall); 1.12.2/1.8.9 ThrowableEntity `this.world.rayTrace(lv, lv2)` (bl2=false), AbstractArrowEntity `rayTrace(lv5, lv6, false, true, false)``)
- **Native:** Collision boxes and the modern traverseBlocks for every release. For pre-1.14 arrows the hit point decides the pull-back and stuck position, so an arrow passing 1.0-1.5 above a fence hits in the engine but flies on in vanilla, and an arrow passing through a fence's outline-only area does the opposite. For throwables only block_hit differs. (`native/src/projectile.rs:1146-1306 (clip_blocks_between uses the 1.14+ traverseBlocks for every release; clip_block uses world.block_collision_boxes for ClipShape::Collider; arrow_flight passes ClipShape::Collider)`)

#### D-181 · Trident loyalty return (noPhysics homing) is not modelled although TridentState carries dealt_damage
- **Versions:** 1.13.2-26.3 · **Severity:** minor · **Kind:** missing · **Status:** confirmed
- **Vanilla:** `if (this.inGroundTime > 4) this.dealtDamage = true; if (loyalty > 0 && (dealtDamage || isNoPhysics()) && owner != null) { setNoPhysics(true); Vec3 vec = owner.getEyePosition().subtract(position()); setPosRaw(getX(), getY() + vec.y * 0.015 * loyalty, getZ()); setDeltaMovement(getDeltaMovement().scale(0.95).add(vec.normalize().scale(0.05 * loyalty))); }`. Then AbstractArrow.tick with physics off: no in-ground test, no clip, a move by the motion, inertia, and no gravity (`if (physicsEnabled && !isInGround()) applyGravity()`). (`26.2 projectile/arrow/ThrownTrident.java tick (:64-100); AbstractArrow.tick noPhysics branch (`this.setPos(originalPosition.add(movement)); this.applyEffectsFromBlocks();`)`)
- **Native:** tick_trident_in_release always runs the physics flight (clip, gravity). A returning loyalty trident follows a ballistic path instead of homing, and dealt_damage has no effect. (`native/src/projectile.rs:355-427 (TridentState.dealt_damage is never read; no loyalty or owner input)`)

#### D-182 · isImmobile is not modelled: a dead or sleeping player still takes WASD input and jumps
- **Versions:** all (1.8.8-26.3) · **Severity:** minor · **Kind:** missing · **Status:** confirmed
- **Vanilla:** LivingEntity.aiStep zeroes the input and the jump while the entity is immobile, before the jump block: `if (this.isImmobile()) { this.jumping = false; this.xxa = 0.0F; this.zza = 0.0F; } else if (this.isEffectiveAi()) {...}`. In 26.3 this comes after applyInput, so it still wins. Player.isImmobile is `super.isImmobile() || this.isSleeping()` and LivingEntity.isImmobile is `this.isDeadOrDying()`. 1.8.9 does the same with func_70610_aX = `getHealth() <= 0.0F || isPlayerSleeping()`. (`origin/client1.21.4 world/entity/LivingEntity.java aiStep:2705-2708, isImmobile:2070; client26.3 LivingEntity.java:3000, 2254-2256, Player.java:823-825; origin/client1.8.9 EntityLivingBase.onLivingUpdate (func_70610_aX), EntityPlayer.java:368-370`)
- **Native:** The adapter sends `state.sleeping` (SLEEPING flag) and `state.dead` (DEAD flag, health <= 0), but they only gate pushes and isInWall. movement_input builds xxa/zza from `state.input` without checking either flag, and `let jumping = state.jump_requested && !(state.is_player && state.flying && ...)` ignores them too. The adapter fills FORWARD/STRAFE and JUMP_REQUESTED straight from `control.*`. (`native/src/simulation.rs:2529-2538 (movement_input), 3999-4001 (jumping), src/mineflayer/player.ts:240-243, 262-274`)

#### D-183 · Boats and ridden mobs never add to fallDistance in their move, so the value the adapter sends and reads back is never updated (powder snow FALLING_COLLISION_SHAPE never applies)
- **Versions:** 1.17-26.3 (powder-snow falling shape, observable); 1.8.8-26.3 (vehicle/ridden fallDistance never accumulates) · **Severity:** minor · **Kind:** missing · **Status:** confirmed
- **Vanilla:** Entity.move calls checkFallDamage for every client-moved entity, the steered horse, pig or strider included: `if (var3) { ... this.resetFallDistance(); } else if (var1 < 0.0) { this.fallDistance -= (float)var1; }`. PowderSnowBlock.getCollisionShape then reads it: `if (var6.fallDistance > 2.5F) { return FALLING_COLLISION_SHAPE; }`. Otherwise a mob that cannot walk on powder snow gets `Shapes.empty()`. (`origin/client1.21.1 client/net/minecraft/world/entity/Entity.java checkFallDamage :1211-1225; PowderSnowBlock.getCollisionShape :91-107`)
- **Native:** move_vehicle sets `body.collided_vertically = y_collision; body.vertical_collision_below = y_collision && requested.y < 0.0; body.on_ground = body.vertical_collision_below;` but never adds the drop to fall_distance. In vehicle.rs, fall_distance only appears in the collision context (`fall_distance: body.fall_distance`). ridden.rs only resets it to 0, halves it in lava, and copies it back (`mob.fall_distance = body.fall_distance`). The adapter sends `fallDistance: mob.fallDistance ?? 0` and stores `mob.fallDistance = output.fallDistance ?? 0`, so the value never grows past what the caller supplied. A mount that falls more than 2.5 blocks onto powder snow therefore passes through the 0.9-tall falling shape where vanilla stops it for the landing tick. (`native/src/vehicle.rs:1091-1175 (move_vehicle), :1221-1226 (collide_vehicle context); native/src/ridden.rs:294, 357, 410, 440, 468, 981, 1166; src/mineflayer/vehicles.ts:337, 372`)

#### D-184 · From 1.21.2, a non-local entity still gets the landing block's velocity update (vy*0, slime bounce, bed bounce) and the slime bounce's fall reset
- **Versions:** 1.21.2-26.1.2 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 1.21.2-1.21.4: `if (this.isControlledByLocalInstance()) { Block var14 = var10.getBlock(); if (var2.y != var4.y) { var14.updateEntityMovementAfterFallOn(this.level(), this); } }`. 1.21.5+: `if (this.canSimulateMovement()) { ... if (var2.y != var4.y) var18.updateEntityMovementAfterFallOn(...) }`, where canSimulateMovement() returns isLocalInstanceAuthoritative(). A non-local entity therefore keeps its vy on landing (no *0.0, no SlimeBlock/BedBlock bounceUp). Its checkFallDamage is skipped as well, so its fallDistance is not reset either. (`origin/client1.21.2 and 1.21.4 world/entity/Entity.java move(), around lines 721-736; origin/client1.21.11 Entity.java:803-808`)
- **Native:** Only the cosmetic flag is gated: `state.block_fall_callback = ... && (!rules.release.at_least(1, 21, 2) || state.controlled_by_local_instance)`. The movement code is not: `let bounced = !fall.restitution_enabled && y_collision && landed_on == BlockEffect::Slime && !state.sneaking; if bounced { if state.velocity.y < 0.0 { state.velocity.y = -state.velocity.y; } state.fall_distance = 0.0; ...}`, and later `if y_collision && !bounced { if landed_on_bed && !state.sneaking {...bounce...} else { state.velocity.y = state.velocity.y * 0.0 } }`. Both run whatever controlled_by_local_instance is. Only N-API callers that pass controlled_by_local_instance=false see this. The adapter defaults it to true. (`native/src/simulation.rs:3677-3688 (bounced), 4340-4368 (y_collision branch)`)

#### D-185 · A ground jump resets fall_distance to 0, which vanilla jumpFromGround never does
- **Versions:** all registered releases (1.8.8-26.3); observable only when the caller supplies an on-ground state with fall_distance > 0 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed (reported by 2 areas)
- **Vanilla:** 1.21.11: `float var1 = this.getJumpPower(); if (!(var1 <= 1.0E-5F)) { ... setDeltaMovement(var2.x, Math.max((double)var1, var2.y), var2.z); if (isSprinting()) addDeltaMovement(...); this.needsSync = true; }`. There is no fallDistance write. 1.8.9 jump() only sets motionY, applies the Jump Boost and sprint boosts, and sets isAirBorne. Player has no jumpFromGround override in 1.21.11. (`origin/client1.21.11 LivingEntity.jumpFromGround; origin/client1.8.9 EntityLivingBase.func_70664_aZ (jump), line 1082`)
- **Native:** `} else if jump == FluidJump::Ground && state.jump_ticks == 0 { ... state.fall_distance = 0.0; state.jump_ticks = 10; }`. The reset happens even when 1.20.5+ skips the impulse (power <= 1e-5). In normal play the value is already 0 here, because landing reset it. It shows only when the state arrives on_ground with fall_distance > 0, for example a caller-supplied state or a 1.21.4 player, whose checkFallDamage never runs. parity-deviations.md lists 'the jump-time fall_distance = 0' among small differences noticed but not reported. (`native/src/simulation.rs:4013-4036 (`state.fall_distance = 0.0;` in the FluidJump::Ground branch)`)

#### D-186 · Landing computes damage when fall_distance is 0; vanilla calls fallOn only when fallDistance > 0
- **Versions:** 1.20.5-26.3 (only with SAFE_FALL_DISTANCE below 0, or below 2 on an upward dripstone tip in 1.20.5-1.21.4 / below 2.5 from 1.21.5) · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed
- **Vanilla:** `if (var3) { if (this.fallDistance > 0.0) { var4.getBlock().fallOn(..., this.fallDistance); ... } this.resetFallDistance(); }`. Standing on the ground with fallDistance 0 never reaches calculateFallDamage. SAFE_FALL_DISTANCE is a synced RangedAttribute(3.0, -1024.0, 1024.0). (`origin/client1.21.11 Entity.checkFallDamage :1472-1484 (also 1.20.4, 26.3 same shape); 1.21.4 Player.causeFallDamage `if (var4 > 0.0F && super.causeFallDamage(...))``)
- **Native:** `if checks_fall_damage && state.on_ground && !bounced { state.fall_damage = if ... else if fall.modern_fall_damage { attribute_landing_damage(landing_fall_distance, state.safe_fall_distance, ...) } ...; state.fall_distance = 0.0; }` runs on every grounded tick with no `fall_distance > 0` test. With safe_fall_distance = -1, floor((0 + 1e-6 + 1) * 1) = 1 damage every tick. On an upward dripstone tip with safe = 2, floor((0 + 2.5 + 1e-6 - 2) * 2) = 1 every tick. Vanilla gives 0. Only the fall_damage output is affected. (`native/src/simulation.rs:3718-3745`)

#### D-187 · A ceiling hit over a slime block resets fall_distance and fall_damage (bounce test does not require onGround)
- **Versions:** 1.8.8-26.1.2 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** The fall reset happens only in checkFallDamage under `if (var3 /*onGround*/) { ... this.fallDistance = 0.0F; }`. A ceiling collision has onGround false and dy >= 0, so fallDistance is kept. SlimeBlock.updateEntityAfterFallOn (any vertical collision) only flips a negative vy: `if (var3.y < 0.0D) setDeltaMovement(var3.x, -var3.y * var4, var3.z)`. It never touches fallDistance. (`origin/client1.14.4 Entity.checkFallDamage and move() :518-534; origin/client1.21.1 SlimeBlock.updateEntityAfterFallOn/bounceUp`)
- **Native:** `let bounced = !fall.restitution_enabled && y_collision && landed_on == BlockEffect::Slime && !state.sneaking; if bounced { if state.velocity.y < 0.0 { ... } state.fall_distance = 0.0; state.fall_damage = 0.0; }`. y_collision is also true for a ceiling hit, so an entity rising into a ceiling with a slime block at floor(y-0.2) loses its fall_distance and its last fall_damage. The velocity handling matches vanilla: no flip for vy > 0, and the *0 branch is skipped. (`native/src/simulation.rs:3676-3688`)

#### D-188 · From 1.20, getOnPosLegacy's fence/wall/fence-gate exception for the landing block is missing
- **Versions:** 1.20-26.3 (observable in fall_damage only with a non-default safe_fall_distance attribute, from 1.20.5, or from 1.21.5 when a fast fall with a recently reset fall distance passes 3 on the landing tick) · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed
- **Vanilla:** `if (mainSupportingBlockPos.isPresent()) { BlockPos var5 = ...get(); if (!(var1 > 1.0E-5F)) return var5; BlockState var6 = level.getBlockState(var5); return (!((double)var1 <= 0.5) || !var6.is(BlockTags.FENCES)) && !var6.is(BlockTags.WALLS) && !(var6.getBlock() instanceof FenceGateBlock) ? var5.atY(Mth.floor(this.position.y - (double)var1)) : var5; }`. Standing on a fence, wall or gate, the block passed to fallOn and updateEntityMovementAfterFallOn is the support block itself. (`origin/client1.21.11 Entity.getOnPos(float) :1003-1018 (same from 1.20)`)
- **Native:** `Some((x, _, z)) => (x, below.1, z)`. It always takes floor(y - 0.2f) at the support's x and z, which is the block at the feet above the fence or wall. The output differs only if that feet block has a fall effect, such as powder snow above a fence (no damage instead of the default). With default attributes that does not change damage, because a fall of more than 2.5 lands on powder snow's falling shape. Listed in parity-deviations.md under 'small differences noticed but not reported' (getOnPos fence fallback for landings). Pre-1.20 behaves the same as the engine, since vanilla falls back only when the block at y-0.2 is air. (`native/src/simulation.rs:1411-1433 (landing_position / on_position)`)
- **Fix:** fixed — `on_position` takes the world and stops at the support for a wall or gate (new tag bit `WALL_OR_GATE`, 32) and for a fence up to offset 0.5; `landing_position` looks through air to a fence, wall or gate before 1.20, which no block effect shows. Also seen at default attributes: an upward dripstone tip on the block, which a player standing on the arm misses (vanilla 2 damage, the engine 9). Cases: `fd_on_*_under_a_tip_h5`, and `get_on_pos_stops_at_a_supporting_wall_or_gate_and_a_fence_up_to_half_a_block`.

#### D-189 · From 1.21.5 the riptide ground lift's move is recorded in movementThisTick and checked by applyEffectsFromBlocks; the engine leaves the lift path out
- **Versions:** 1.21.5-26.3 · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed (reported by 2 areas)
- **Vanilla:** releaseUsing runs from handleKeybinds before the entity tick: `if (var5.onGround()) { var5.move(MoverType.SELF, new Vec3(0.0, 1.1999999284744263, 0.0)); }`. From 1.21.5 that Entity.move does `this.addMovementThisTick(new Movement(var13, var8, var2));` (1.21.5: `this.movementThisTick.add(var8)`). Nothing clears the list before the tick's `applyEffectsFromBlocks()`: `this.finalMovementsThisTick.addAll(this.movementThisTick); ... this.applyEffectsFromBlocks(this.finalMovementsThisTick);`. So the lift (pre-lift box to post-lift box) is swept first, then the travel move. Blocks only the pre-lift box touches get entityInside: a cobweb or sweet berry bush at the feet (makeStuckInBlock and fall reset), a bubble column (1.21.5-1.21.9 always, from 1.21.10 only when precise), and powder snow FREEZE, plus fluid and fire effects. (`origin/client1.21.11 TridentItem.java:98-100; Entity.java:767-770 (move -> addMovementThisTick), :859-870 (applyEffectsFromBlocks); Minecraft.java:1787 handleKeybinds before :1798 level.tickEntities; ClientLevel.java:333 setOldPosAndRot. 1.21.5 Entity.java:707-720 records the same way; 1.21.2-1.21.4 do not record (applyEffectsFromBlocks(oldPosition(), position) only)`)
- **Native:** For 1.21.2+ launch_riptide skips the inside pass (`if !release.at_least(1, 21, 2) { ... apply_block_effects(...) }`), and the comment says the tick's check "runs from where the tick started (1.21.3 ...)". old_position is taken after the lift, and movements_this_tick returns only `Movement { from: old_position, to, axis_order }` (plus the tail), so the lift's blocks are never visited in 1.21.5+. This is correct for 1.21.2-1.21.4 only. (`native/src/simulation.rs:3381-3391 (launch_riptide: inside effects only `if !release.at_least(1, 21, 2)`), :3835 (`let old_position = state.position;` after the launch), :2144-2180 (movements_this_tick builds only the tick's own move plus the tail)`)

#### D-190 · 1.21.4 BlockGetter.boxTraverseBlocks dropped the ±1e-7 travel nudge; the engine still applies it for 1.21.4
- **Versions:** 1.21.4 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 1.21.2 and 1.21.3: `Vec3 var6 = var3.normalize().scale(1.0E-7); Vec3 var7 = var2.getMinPosition().add(var6); Vec3 var8 = var2.getMinPosition().subtract(var3).subtract(var6); addCollisionsAlongTravel(var5, var8, var7, var2);`. 1.21.4: `Vec3 var6 = var2.getMinPosition(); Vec3 var7 = var6.subtract(var3); addCollisionsAlongTravel(var5, var7, var6, var2);`, with no nudge. (`origin/client1.21.4 world/level/BlockGetter.java boxTraverseBlocks (~:166-180) vs origin/client1.21.2 / 1.21.3 same method; mc-decompiled 1.21.4 BlockView.java:172-186 vs 1.21.3 :172-184`)
- **Native:** `let nudge = normalize(travel).scale(1.0e-7); let end = bounds.min + nudge; let start = bounds.min - travel - nudge; walk_cells(start, end, Some(16), ...)` runs for every release below 1.21.5, with no 1.21.4 gate. On 1.21.4, for moves of about 1 block or more per tick, the cells the swept walk enters can differ at exact cell boundaries. That changes which cobweb, berry bush, bubble column, honey or powder snow cells get entityInside. (`native/src/block_traversal.rs:244-274 (box_traverse_blocks, used by blocks_visited for <1.21.5 at :132-145, reached from simulation.rs blocks_inside_moves :1996-2004)`)

#### D-191 · 1.13.2 water travel uses the post-drag motionY <= 0 for the -0.003 snap; the engine uses the pre-move falling flag
- **Versions:** 1.13.2 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** `this.field_70181_x *= 0.800000011920929D; ... if (!this.func_189652_ae() && !this.func_70051_ag()) { if (this.field_70181_x <= 0.0D && Math.abs(this.field_70181_x - 0.005D) >= 0.003D && Math.abs(this.field_70181_x - var4 / 16.0D) < 0.003D) { this.field_70181_x = -0.003D; } else { this.field_70181_x -= var4 / 16.0D; } }`. The falling test reads the current, post-move and post-drag motionY. From 1.14 it is the pre-move `var4` flag (1.14.4 LivingEntity.travel: `if (var4 && Math.abs(var11.y - 0.005D) >= 0.003D ...`). (`origin/client1.13.2 client/net/minecraft/entity/EntityLivingBase.java func_191986_a (travel), water branch`)
- **Native:** Every release passes the pre-move `going_down` as `falling`. On 1.13.2 the results differ only when the move turns a non-positive motion positive with a post-drag y in (0, 0.002] under Slow Falling (gravity 0.01), for example a bubble column raising y inside the move. The engine snaps to -0.003 there, while vanilla subtracts 0.000625. (`native/src/simulation.rs:4063 `let going_down = state.velocity.y <= 0.0;` passed to apply_fluid_travel_vertical :1235-1244 through fluid_falling_adjusted_y :2329-2343`)

#### D-192 · 1.14.x spectator holding sneak: the engine slows the input by 0.3 and then divides it back, but vanilla never slows a spectator, so its flight input ends up 1.0 long instead of 0.98
- **Versions:** 1.14.4 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed (reported by 3 areas)
- **Vanilla:** LocalPlayer.aiStep calls `this.input.tick(var4, this.isSpectator());`. KeyboardInput only slows when `if (!var2 && (this.sneakKeyDown || var1)) { leftImpulse = (float)((double)leftImpulse * 0.3D); ... }`, so a spectator is never slowed. The flight block still runs for a spectator: `if (this.abilities.flying && this.isControlledCamera()) { if (this.input.sneakKeyDown) { leftImpulse = (float)((double)leftImpulse / 0.3D); forwardImpulse = (float)((double)forwardImpulse / 0.3D); ...`. The impulse becomes 3.333; times 0.98F that is 3.27, which getInputVector normalizes to length 1.0. (`origin/client1.14.4 client/net/minecraft/client/player/KeyboardInput.java tick(boolean,boolean); LocalPlayer.java:593 and :682-689`)
- **Native:** For 1.14 the adapter reports MOVING_SLOWLY whenever sneak is held, spectator or not (`if (this.lt('1.15') && state.control.sneak) return true;`). The engine then does `if state.moving_slowly.unwrap_or(state.sneaking) { input = input.map(|impulse| (f64::from(impulse) * 0.3) as f32) }`, followed by the undo `if state.flying && state.sneaking && release.at_least(1, 9, 0) && !release.at_least(1, 15, 0) { input = input.map(|impulse| (f64::from(impulse) / 0.3) as f32) }`. The net impulse is 1.0, so 0.98 after scale_input. A 1.14.4 spectator flying along one axis with sneak held therefore gets 0.98x the vanilla horizontal acceleration. Diagonal input is normalized on both sides and matches. (`src/mineflayer/player.ts:845-852 (isMovingSlowly: `if (this.lt('1.15') && state.control.sneak) return true;`) + native/src/simulation.rs:2557-2572 (input_move_vector)`)

#### D-193 · The adapter's ground jump when flight is toggled on uses max(power, vy) from 1.20.5; vanilla only adds the max in 1.21.2
- **Versions:** 1.20.5-1.21.1 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed (reported by 2 areas)
- **Vanilla:** 1.20.5-1.21.1: `float var1 = this.getJumpPower(); if (!(var1 <= 1.0E-5F)) { Vec3 var2 = this.getDeltaMovement(); this.setDeltaMovement(var2.x, (double)var1, var2.z); ...`, which sets y outright. The max only arrives in 1.21.2: `this.setDeltaMovement(var2.x, Math.max((double)var1, var2.y), var2.z);`. The block jump factor is read at blockPosition() and at getBlockPosBelowThatAffectsMyMovement() (getOnPos(0.500001F): the supporting block's x/z with y = floor(y-0.500001)). (`origin/client1.20.6 / origin/client1.21.1 world/entity/LivingEntity.java jumpFromGround(); origin/client1.21.2 LivingEntity.jumpFromGround; LocalPlayer.aiStep flight toggle (1.20.6 :714-718)`)
- **Native:** The adapter's own LivingEntity.jumpFromGround, which runs when a player toggles flight on while on the ground, always sets `state.vel.y = Math.max(power, state.vel.y)` from 1.20.5 on. If vy already exceeds the jump power (for example on a slime bounce or in an upward bubble column), 1.20.5-1.21.1 keeps the larger vy where vanilla lowers it to the jump power. The jump-factor position also uses `state.supportingBlockPos` as it is rather than its `atY(floor(y-0.500001))`. The native engine's own jump_power and its max gate at 1.21.2 are correct; only this adapter path is off. (`src/mineflayer/player.ts:1056 (`if (state.flying && state.onGround && !this.lt('1.20.5')) this.jumpFromGround(state, sync);`), :1075-1088 (`state.vel.y = Math.max(power, state.vel.y);`)`)

#### D-194 · The default PhysicsConfig movement speed is 0.1 (double), not the player's base 0.1F, so a sprinting player's float speed differs by one ulp for direct N-API callers
- **Versions:** all (only direct N-API/simulate callers that omit both movementSpeed and movementSpeedValue, and only when a sprint/speed/slowness modifier is active) · **Severity:** minor (low) · **Kind:** precision · **Status:** confirmed
- **Vanilla:** The base is 0.1F = 0.10000000149011612. Sprinting gives (float)(0.10000000149 * 1.30000001192) = 0.13000001F, which feeds getFrictionInfluencedSpeed. (`Player.createAttributes `.add(Attributes.MOVEMENT_SPEED, 0.1F)` (1.14.4-26.3); Player.getSpeed `return (float)this.getAttributeValue(Attributes.MOVEMENT_SPEED);``)
- **Native:** When state.movement_speed is None, the base is the exact double 0.1. Sprinting gives (0.1 * 1.3000000119) as f32 = 0.13F, one float ulp below vanilla's 0.13000001F, so every sprinting ground tick accelerates slightly less. This is the same class of issue as D-155 (the flyingSpeed default). The Mineflayer adapter sends f32(0.1) and is not affected. (`native/src/simulation.rs:236 (`movement_speed: 0.1,`), :4081-4090 (movement_speed_attribute(state.movement_speed.unwrap_or(config.movement_speed), ...)); native/src/napi_api.rs:182`)

#### D-195 · 1.10-1.12 auto-jump walks the obstacle cells z, y, x; vanilla walks x, z, y, so the first crossing obstacle can differ
- **Versions:** 1.10.2-1.12.2 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** getCollisionBoxes loops `for (int o = i; o < j; o++) { for (int p = m; p < n; p++) { ... for (int q = k; q < l; q++) { ... appendCollisionBoxes } } }`: x outermost, then z, then y innermost. updateAutoJump takes the FIRST box in that list that crosses either probe line and breaks, so the order picks the obstacle. (`E:/pr/mc-decompiled/1.12.2 net/minecraft/world/World.java:1127-1175 (method_13689 = getCollisionBoxes) and :1180 doesBoxCollide; ClientPlayerEntity.java method_13426 (updateAutoJump) `List<Box> list = this.world.doesBoxCollide(this, lv12); ... for (Box lv19 : list) { if (lv19.intersects(lv15, lv16) || lv19.intersects(lv17, lv18)) { s = ...; ...; break; } }``)
- **Native:** Before 1.13 the engine uses `world.block_collision_boxes_in_order(area, &context)`, which iterates `for z { for y { for x {...} } }` (Cursor3D order, correct for 1.14+). Example: moving +z with obstacles at (x0, z+2) on the left line and (x0+1, z+1) on the right line. Vanilla takes (x0, z+2) first (lower x), the engine takes (x0+1, z+1) first (lower z). If one is a slab and the other a full block, the auto-jump decision flips. (`native/src/simulation.rs:3554-3558 (non-1.13 branch to block_collision_boxes_in_order), :3566-3595 (first-match loop); native/src/world.rs:1455-1493`)

#### D-196 · 1.10-1.12 auto-jump probes the blocks above with getCollisionBoundingBox (one box: the 1.0 outline for fences, 0.125 for a brewing stand); the engine uses the full collision shape
- **Versions:** 1.10.2-1.12.2 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** `if ((lv24 = lv23.method_11726(this.world, lv22)) != null) { s = (float)lv24.maxY + lv22.getY(); if (s - this.getBoundingBox().minY > o) return; }`. method_11726 is getCollisionBoundingBox, which defaults to `state.getCollisionBox(view,pos)` (the outline box). For a fence that is field_12667 (maxY 1.0, not the 1.5 collision posts). For a brewing stand it is field_12592 = (0,0,0,1,0.125,1); the 0.875 rod is only in appendCollisionBoxes. An end portal frame stays 0.8125 even with an eye. The ceiling checks use the same `== null` test. (`E:/pr/mc-decompiled/1.12.2 net/minecraft/entity/player/ClientPlayerEntity.java method_13426; net/minecraft/state/StateManager.java:388-390; block/Block.java:405-407; block/FenceBlock.java:29-46, 91-93; block/BrewingStandBlock.java:34, 81-91; block/EndPortalFrameBlock.java:31-56`)
- **Native:** The engine takes the maximum Y of the block's full collision boxes (1.5 for a fence, 0.875 for a brewing stand, 1.0 for a frame with an eye). Example: a full block in the path with a brewing stand on top. Vanilla's rise is 1.125 <= 1.2, so it auto-jumps. The engine's rise is 1.875 > 1.2, so it returns without jumping. With Jump Boost the fence and portal-frame cases flip in the same way. (`native/src/simulation.rs:3496-3508 (ceiling probe), :3577-3592 (`world.block_collision_boxes_for_entity(above, &context)` then `.map(|bounds| bounds.max.y).reduce(f64::max)`)`)

#### D-197 · Riptide spin: an overlapping spectator player keeps the spin from ending on a wall
- **Versions:** 1.13.2-26.3 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** `List var4 = this.level().getEntities(this, var3);` with `getEntities(Entity, AABB) { return this.getEntities(var1, var2, EntitySelector.NO_SPECTATORS); }`, so spectators are never in the list. `if (!var4.isEmpty()) {...} else if (this.horizontalCollision) { this.autoSpinAttackTicks = 0; }`: with only a spectator in the swept box the list is empty and a wall ends the spin. (`origin/client1.21.11 LivingEntity.java:3090-3104 checkAutoSpinAttack; EntityGetter.java:29-31 (same in 1.16.4 :33-35); 1.13.2 EntityLivingBase func_204801_a (func_72839_b)`)
- **Native:** The adapter maps a spectator player to 'inert' (`if (entity.name === 'player' && this.isSpectatorPlayer(state, entity)) return 'inert';`). check_auto_spin_attack then counts every kind, inert included: `let mut touched = entities.iter().filter(|entity| entity.bounding_box.intersects(swept)).peekable(); if touched.peek().is_none() { return state.collided_horizontally; }`. An overlapping spectator makes the list non-empty, so the horizontal-collision end is skipped and the spin goes on. The player.ts comment says spectators do not meet the spin, but the engine does not act on that. (`native/src/entity.rs:226-252 (check_auto_spin_attack); src/mineflayer/player.ts:675 (entityKind)`)

#### D-198 · fireworkBoost input is added after jump, slow-fall gravity sign and flight Y were taken, though it stands for a rocket that ticked before aiStep
- **Versions:** 1.11.2-26.3 (jump overwrite on landing); the Slow Falling gravity and fall reset parts apply from 1.13.2 · **Severity:** minor (low) · **Kind:** tick-order · **Status:** confirmed · relates to D-008
- **Vanilla:** A rocket that ticks before the player sets `setDeltaMovement(var8.add(var3.x * 0.1 + (var3.x * 1.5 - var8.x) * 0.5, ...))` before Player.tick and aiStep run. jumpFromGround, the Slow Falling `y <= 0` test (getEffectiveGravity / travel var4), the slow-fall fall-distance reset and Player.travel's saved flight Y all read the boosted velocity. (`origin/client1.21.11 FireworkRocketEntity.java:119-126 (tick, before the player in ClientLevel.tickEntities); LivingEntity.java:2267-2270 getEffectiveGravity; 1.20.1 LivingEntity.travel start (`boolean var4 = this.getDeltaMovement().y <= 0.0`)`)
- **Native:** The precomputed `firework_boost` is added only just before the glide. going_down, and so travel_gravity (min 0.01 under Slow Falling), the 1.13-1.19.4 slow-fall fall reset, flying_vertical_velocity and the jump's Y all use the velocity before the boost. A jump also ends up as jump + boost.y where vanilla overwrites Y. The engine's own comment says the boost 'has already run when aiStep starts', and only the 1.21.2+ fall check is reordered to match. (`native/src/simulation.rs:4063 (`let going_down = state.velocity.y <= 0.0;`), 4072-4078 (travel_gravity), 4172 (`state.add_delta_movement(state.firework_boost, ...)`); the comment at 4040-4044`)

#### D-199 · Per-attempt block filter (Colliders::listed, candidate_step_up) uses strict AABB.intersects instead of BlockCollisions' joinIsNotEmpty merge tolerance
- **Versions:** 1.14.4-26.3 (1.14.4-1.15.2 for every shape, 1.16+ for shapes other than Shapes.block(); in 1.14.4-1.17.1 only multi-axis moves and step attempts, since single-axis moves use walk_axis) · **Severity:** minor (low) · **Kind:** precision · **Status:** confirmed
- **Vanilla:** For a full cube: `if (var7 == Shapes.block()) { if (!this.box.intersects(x, y, z, x+1, y+1, z+1)) continue; ... }`. For any other shape: `VoxelShape var8 = var7.move(pos); if (var8.isEmpty() || !Shapes.joinIsNotEmpty(var8, this.entityShape, BooleanOp.AND)) continue;` (1.14.4: joinIsNotEmpty for every block). The IndirectMerger behind joinIsNotEmpty folds coordinates closer than 1e-7, so a shape that overlaps the query box by less than about 1e-7 on some axis is not returned. (`origin/client1.21.11 client/net/minecraft/world/level/BlockCollisions.java computeNext; origin/client1.14.4 LevelReader.getBlockCollisions; 1.16.5/1.18 BlockCollisionSpliterator.computeNext`)
- **Native:** Every attempt (the main collide, each Lifted call, and the 1.21+ step query) keeps a block whose boxes satisfy the strict `bounds.intersects(query)` (overlap > 0). The engine already models the merger tolerance in no_collision_for_entity (world.rs shape_meets_query / merged_sections_meet) but not here. A shape overlapping the swept box by 0 < d < 1e-7 is therefore a collider in the engine but not in vanilla. Example: a +x move whose end lands 5e-8 short of a slab, stair or fence face. Vanilla does not list that block and moves the full dx. The engine clips dx to dx - 5e-8. Mth.equal hides the flag change, but the position differs by about 5e-8 for that tick. The extra shape can also add step-up candidate Y coordinates, which does not change the chosen height. (`native/src/collision.rs:283-302 (Colliders::listed: `.filter(|shape| shape.boxes.iter().any(|bounds| bounds.intersects(query)))`), collision.rs:529-534 (candidate_step_up block filter, same test); world.rs:1884 collect_collision_shapes uses the same strict test`)

#### D-200 · Projectile VoxelShape.clip start-inside test compares world coordinates instead of block-local findIndex
- **Versions:** 1.13.2-26.3 · **Severity:** minor (low) · **Kind:** precision · **Status:** confirmed · relates to D-144
- **Vanilla:** The point is made block-local before the cell test. 1.13.2 uses `Vec3d lv3 = vec3d.add(lv.multiply(0.001)).subtract(pos.getX(), pos.getY(), pos.getZ()); return this.contains(lv3.x, lv3.y, lv3.z) ? ...`. Modern code uses `this.shape.isFullWide(this.findIndex(Axis.X, inside.x - pos.getX()), ...)` against the shape's local coords. (`E:/pr/mc-source/26.2 VoxelShape.java clip :119-131; 1.13.2 decompiled net/minecraft/util/shapes/VoxelShape.java rayTrace :150-165`)
- **Native:** Tests the world-space point against world-space boxes (`bounds.contains(inside)`), whose faces are local + pos. Near a face, `x - X < local` and `x < local + X` can round differently, so the inside hit can be taken or missed differently from vanilla. This is rare and differs by one ulp. (`native/src/projectile.rs:1278-1284 (let inside = from + delta.scale(0.001); boxes.iter().any(|bounds| bounds.contains(inside)))`)

#### D-201 · 1.8.8: piston head arm collision boxes are 1.9's (reach ±0.25 out of the cell) instead of 1.8's
- **Versions:** 1.8.8 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** The 1.8 arm stays inside the cell with odd extents: `case DOWN: setBlockBounds(0.375F, 0.25F, 0.375F, 0.625F, 1.0F, 0.625F); case UP: (0.375F,0,0.375F,0.625F,0.75F,0.625F); case NORTH: (0.25F,0.375F,0.25F,0.75F,0.625F,1.0F); case SOUTH: (0.25F,0.375F,0,0.75F,0.625F,0.75F); case WEST: (0.375F,0.25F,0.25F,0.625F,0.75F,1.0F); case EAST: (0,0.375F,0.25F,0.75F,0.625F,0.75F)`. 1.9 uses the 4/16 arms reaching 0.25 into the next cell (`new Box(0.375, 0.25, 0.375, 0.625, 1.25, 0.625)`). (`origin/client1.8.9:client/net/minecraft/block/BlockPistonExtension.java func_180638_a, func_176323_e (lines ~82-117); 1.9.4 PistonHeadBlock.java:32-37`)
- **Native:** The 1.8.8 player collides with the 1.9 arms. The arm reaching out of the cell also makes the head a large shape, so the engine's bordering-cell scan includes it where 1.8's World.getCollisionBoxes never looks. (`src/mineflayer/blocks.ts:191, 482; legacy.ts shapes() (no piston_head case); minecraft-data pc/1.8 piston_head e.g. [[0,0,0,1,0.25,1],[0.375,0.25,0.375,0.625,1.25,0.625]]`)

#### D-202 · 1.13.2: bed and cauldron collision shapes are the 1.14 ones (legs) instead of 1.13's solid shapes
- **Versions:** 1.13.2 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 1.13.2 bed: `field_196351_c = Block.makeCuboidShape(0,0,0,16,9,16)` (a solid 9/16 slab). 1.13.2 cauldron: `VoxelShapes.combineAndSimplify(VoxelShapes.fullCube(), INSIDE(2,4,2,14,16,14), ONLY_FIRST)` (solid up to 4/16). Both got legs only in 1.14 (BASE box(0,3,0,16,9,16) plus LEG_* boxes; the cauldron's box(0,0,4,16,3,12)… under-cut). (`origin/client1.13.2:client/net/minecraft/block/BlockBed.java:203-205, 296; BlockCauldron.java:243-244; origin/client1.14.4 BedBlock.java:295-299, CauldronBlock.java:231`)
- **Native:** minecraft-data 1.13.2 gives the legged 1.14 shapes: the bed has empty space under 3/16 except the legs, and the cauldron is open under 3/16. For a player this differs only in rare contacts (thin entities, boats or vehicle boxes under 3/16, and ulp-level collide order). Top surfaces match. (`src/mineflayer/blocks.ts:191 (minecraft-data pc/1.13.2 blockCollisionShapes, which is identical to pc/1.14.4 for every block)`)

#### D-203 · Generated block data: state counts and property lists miss inherited and some declared properties
- **Versions:** 1.14.4-26.3 (releases with generated data; N-API generated* helpers only, player movement unaffected) · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed · relates to D-086
- **Vanilla:** 1.20.4 state counts: chain 6, big_dripleaf 32, oak_fence 32, glass_pane 32, oak_trapdoor 64, white_bed 16, oak_stairs 80, oak_door 64, lantern (1.16.5) 4. (`origin/client1.20.4 ChainBlock (extends RotatedPillarBlock: AXIS), BigDripleafBlock (extends HorizontalDirectionalBlock: FACING), CrossCollisionBlock (NORTH/EAST/SOUTH/WEST/WATERLOGGED), TrapDoorBlock (FACING,OPEN,HALF,POWERED,WATERLOGGED), BedBlock (FACING,PART,OCCUPIED), StairBlock (FACING,HALF,SHAPE,WATERLOGGED)`)
- **Native:** `generatedBlockStateCount`, `generatedStateIndex` and `generatedMethodShapePredicatesMatchState` give wrong counts and indices, or refuse valid property sets, for these blocks. The player path does not use them (shapes come from minecraft-data), so movement is unaffected. (`native/data/releases/v1_20_4.rs (chain: property_order ["WATERLOGGED"], state_count 2; big_dripleaf: ["WATERLOGGED","TILT"], 8; oak_fence 1; glass_pane 1; oak_trapdoor 16; white_bed 4; oak_stairs 120); v1_16_5 lantern 1; v1_21_11 oak_door 400; v26_3 oak_door 192; native/data/mod.rs state_index`)

#### D-204 · A dying shulker or happy ghast (health 0, still in the world during its 20-tick death animation) is still an entity collider
- **Versions:** 1.9.4-26.3 (shulker); 1.21.6-26.3 (happy ghast) · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed · relates to D-092
- **Vanilla:** Shulker: `public boolean canBeCollidedWith(@Nullable Entity var1) { return this.isAlive(); }` (1.9-1.13 `getBox() { return this.isAlive() ? this.getBoundingBox() : null; }`). HappyGhast: `if (!this.isBaby() && this.isAlive()) { ... } else { return false; }`. A mob whose health has reached 0 stays in the level until tickDeath removes it, but from that moment it is not a collider. (`origin/client1.21.11 world/entity/monster/Shulker.java:421-423; animal/happyghast/HappyGhast.java:511-521; origin/client26.3 Shulker.java:441-443, HappyGhast.java:522-531; 1.9.4 decompiled ShulkerEntity getBox :420-422; 1.14.4 getCollideBox`)
- **Native:** `Self::Boat | Self::Shulker | Self::HappyGhastStaysStill => Some(EntityCollider { bounds, only_from_above: false })`, `Self::HappyGhast => Some(EntityCollider { bounds, only_from_above: true })`. The kind comes only from the name and the baby/staysStill flags (`if (entity.name === 'shulker') return 'shulker'`), and nothing tells the engine the entity's health. A dying shulker or happy ghast blocks the player's move, or holds it up, for up to 20 more ticks. (`native/src/entity.rs:187-199 (NearbyEntityKind::collider); src/mineflayer/player.ts:664-667 (entityKind); src/mineflayer/types.ts:47-66 (NearbyEntity has no health or alive field)`)

#### D-205 · The riptide spin check counts spectator players as touched entities, which vanilla's getEntities leaves out (NO_SPECTATORS)
- **Versions:** 1.13.2-26.3 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** `List var4 = this.level().getEntities(this, var3); if (!var4.isEmpty()) { for ... if (var6 instanceof LivingEntity) {... scale(-0.2); break;} } else if (this.horizontalCollision) { this.autoSpinAttackTicks = 0; }`, where `getEntities(Entity, AABB)` = `getEntities(var1, var2, EntitySelector.NO_SPECTATORS)`. A spectator player in the swept box is not in the list, so the spin still ends on a wall. (`origin/client1.21.11 world/entity/LivingEntity.java:3090-3104 checkAutoSpinAttack; world/level/EntityGetter.java:28-30; 1.13.2 decompiled class_3593.java:18-19 (EntityPredicate.field_16705 = EXCEPT_SPECTATOR)`)
- **Native:** `let mut touched = entities.iter().filter(|entity| entity.bounding_box.intersects(swept)).peekable(); if touched.peek().is_none() { return state.collided_horizontally; }`. Every nearby entity counts here, Inert ones included. The adapter sends a spectator player as kind 'inert' with its real box, so the list is non-empty. The spin then neither bounces (the spectator is not living) nor ends on a horizontal collision, while vanilla ends it. (`native/src/entity.rs:226-253 (check_auto_spin_attack); src/mineflayer/player.ts:675 (spectator player -> 'inert', box still sent at :547-559)`)

#### D-206 · A local player that carries passengers (isVehicle) still takes entity pushes
- **Versions:** 1.9.4-26.3 · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed
- **Vanilla:** In Entity.push(Entity) the pushed side takes the impulse only `if (!this.isVehicle() && this.isPushable())` (1.9-1.18: `if (!this.hasPassengers())`). A player with a passenger (one mounted by command or by a plugin) gets no push from a mob or a boat. (`origin/client1.16.4 world/entity/Entity.java:1240-1246; origin/client1.21.11 Entity.java:1830-1836; origin/client26.3 Entity.java:1931-1937; 1.9.4 decompiled Entity.java:1164-1170`)
- **Native:** push_by_nearby_entities adds the impulse to the player whenever `pushable` (spectator, removed, dead, climbable) and the team rules allow it. The player's own passengers are never checked (EntityPush/NearbyEntity only carry the *other* side's isVehicle). (`native/src/entity.rs:294-357 (push_by_nearby_entities: `state.velocity = state.velocity + ...` with no vehicle check on the player)`)

#### D-207 · 1.8 pushed-block area is not normalised when the draw-in passes the opposite face
- **Versions:** 1.8.8 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed · relates to D-106
- **Vanilla:** BlockPistonMoving.getCollisionBox draws in the same way (`if (dir.getOffsetY() < 0) e -= dir.getOffsetY() * progress; else i -= dir.getOffsetY() * progress;`) but builds the result with `new Box(d, e, g, h, i, j)`, whose constructor sorts each axis (`this.minX = Math.min(d, g); ... this.maxX = Math.max(d, g)`). Example: a bottom trapdoor (0..0.1875) moved up at progress 0.5 gives Box y [pos-0.3125, pos], which still catches and pushes an entity just below it. (`E:/pr/mc-decompiled/1.8.9 net/minecraft/block/PistonExtensionBlock.java:165-200; net/minecraft/util/math/Box.java:22-29`)
- **Native:** `if step < 0 { *min -= inset; } else { *max -= inset; }` changes the box in place, and Aabb::new keeps min > max as given. `intersects` then rejects it, so a thin block (carpet, snow layer, trapdoor, repeater, daylight sensor) moved along its thin axis with left = 0.5 pushes nothing. (`native/src/piston.rs:349-360 (push_before_1_9 draw_in) with math.rs:1767 (Aabb::new does not sort min/max)`)

#### D-208 · 1.21.5+ boat block-effect pass records the move from the pre-tick position, not from where the move started after the floatBoat snap
- **Versions:** 1.21.5-26.3 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** The recorded movement starts at this.position() at the start of Entity.move. That is after floatBoat's `this.setPos(this.getX(), targetY, this.getZ())` when the boat comes out of the air onto water. The first applyEffectsFromBlocks pass covers snapped -> snapped+moved and needs no tail segment. Only the second pass uses oldPosition() -> position(). (`origin/client1.21.11 Entity.java:767-770 (move: `Vec3 var13 = this.position(); Vec3 var8 = var13.add(var4); this.addMovementThisTick(new Movement(var13, var8, var2));`) and :859-869 (applyEffectsFromBlocks); AbstractBoat.floatBoat setPos before move (26.3 AbstractBoat.java floatBoat)`)
- **Native:** movements_this_tick receives the pre-tick old_position. On a snap tick the first pass covers old -> old+moved at the pre-snap height, then a tail old+moved -> final position. These are different block cells, so inside effects (bubble column, cobweb, powder snow, etc.) can be applied or missed differently on the tick the boat lands in water. (`native/src/vehicle.rs:357 (`let old_position = state.position;` before float_boat), :477-483 (movements_this_tick(body, old_position, ...)); simulation.rs:2161-2178 (`from: old_position, to: old_position + moved` plus a tail segment to state.position)`)

#### D-209 · A sitting camel in water does not stand up instantly (Camel.tick standUpInstantly)
- **Versions:** 1.20-26.3 · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed
- **Vanilla:** After super.tick (the travel), Camel.tick runs `if (this.isCamelSitting() && this.isInWater()) { this.standUpInstantly(); }`. standUpInstantly does `setPose(Pose.STANDING)` and `resetLastPoseChangeTickToFullStand(gameTime)` (`max(0, gameTime - 52 - 1)`). This sets the client's own entity data, so refuseToMove() is false on the next tick and the box goes back to the full 2.375 height. (`origin/client1.20 Camel.java tick (:190); origin/client1.21.11 Camel.java tick (:175-177); E:/pr/mc-source/26.3 Camel.java tick (:182-184); absent in origin/client1.19.3/1.19.4`)
- **Native:** The camel arm only ends the dash, decrements dash_cooldown and does `mob.pose_time += 1`. `mob.sitting` stays true in water, so the next ticks keep refusing input (zero input, no rotation) and keep the 2.375 - 1.43 sitting box until the server's pose sync arrives. (`native/src/ridden.rs:1233-1242 (after_ridden_travel camel arm)`)

#### D-210 · 26.3 fishing hook gravity is 0.03F (0.029999999329447746) and now counts as effective gravity in Entity.move
- **Versions:** 26.3 · **Severity:** minor (low) · **Kind:** precision · **Status:** confirmed
- **Vanilla:** 26.3: `this.setDeltaMovement(this.getDeltaMovement().add(0.0, -this.getDefaultGravity(), 0.0));` with `protected double getDefaultGravity() { return 0.029999999329447746; }` (26.2 and earlier: literal `-0.03`). Because getEffectiveGravity() = getGravity() = getDefaultGravity(), Entity.restituteMovementAfterCollisions now uses 0.03F for the hook's `-currentMovement.y < getEffectiveGravity()` bounce gate and gravity compensation. (`E:/pr/mc-source/26.3 FishingHook.java:227 and :468-469; 26.2_to_26.3.diff @@ -224 / +457; 26.3 Entity.java:918-919, 1563-1564`)
- **Native:** `default_gravity_in` returns the double 0.03 for FishingHook in every release (its comment says the hook 'subtract[s] a double literal in every release'), and the hook's move passes `PROJECTILE_MOVE.gravity = 0.0` to the restitution. (`native/src/projectile.rs:117-150 (default_gravity / default_gravity_in: FishingHook → 0.03 in every release); :18-26 PROJECTILE_MOVE gravity 0.0 used by move_projectile`)

#### D-211 · Riptide ground lift ignores the landing block's updateEntityAfterFallOn on a ceiling hit (slime/bed bounce)
- **Versions:** 1.13.2-26.1.2 · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed
- **Vanilla:** The 1.2 lift is a full Entity.move. When it hits a ceiling (`var2.y != var4.y`), the block at getOnPosLegacy (y-0.2 after the move) runs updateEntityMovementAfterFallOn. For a slime block that is not suppressing the bounce, `if (vec.y < 0) setDeltaMovement(x, -vec.y * 1.0, z)`. A bed gives `-vec.y * 0.66`. Other blocks multiply y by 0. A player who launches while looking down (riptide push y < 0) under a ceiling less than 0.2 above the head, standing on slime or a bed, gets y reflected. (`origin/client1.14.4 Entity.move (BlockPos(x, y-0.2, z) state, then `if (var2.y != var3.y) var23.getBlock().updateEntityAfterFallOn(this.level, this)`); origin/client1.21.4 Entity.java:718-735 (`BlockPos var9 = this.getOnPosLegacy(); ... if (this.isControlledByLocalInstance()) { if (var2.y != var4.y) var14.updateEntityMovementAfterFallOn(this.level(), this); }`); 1.13.2 Entity.move -> Block.onLanded; SlimeBlock.bounceUp, BedBlock.bounceUp`)
- **Native:** `if state.collided_vertically { state.velocity.y = if release.at_least(1, 14, 0) { state.velocity.y * 0.0 } else { 0.0 }; }` is unconditional. There is no landing-block lookup and no slime or bed bounce, unlike the tick's own move (simulation.rs:3676-3688, 4339-4360) and the external move in piston.rs:595-636. From 26.2 vanilla's restituteMovementAfterCollisions uses only the entity restitution for a non-below collision, so the gap closes there. (`native/src/simulation.rs:3374-3380 (launch_riptide)`)

#### D-212 · 1.16.2-1.17.1 suffocatesAt also gets the world border shape from getBlockCollisions(entity, ...)
- **Versions:** 1.16.2-1.17.1 · **Severity:** minor (low) · **Kind:** missing · **Status:** confirmed
- **Vanilla:** noBlockCollision(entity, box, predicate) streams a CollisionSpliterator with `needsBorderCheck = entity != null`. Its first tryAdvance offers `border.getCollisionShape()` without applying the predicate whenever the player's own box is not fully within the border, is not outside it and is close to it (within 1e-7). Then every suffocatesAt is true, all four neighbours included, so no push-out direction is ever chosen. (`origin/client1.16.4 LocalPlayer.suffocatesAt :427-433 (`!this.level.noBlockCollision(this, var3, isSuffocating)`); CollisionGetter.java:68-69; CollisionSpliterator.java:59-60, worldBorderCheck; 1.17 ClientPlayerEntity.wouldCollideAt :441-445 -> CollisionView.hasBlockCollision -> BlockCollisionSpliterator.offerWorldBorderShape`)
- **Native:** `world.collides_with_suffocating_block(column, release, true)` tests only blocks. A player inside a suffocating block while touching the border from inside is pushed by 0.1, where vanilla does not push. (is_in_wall models the same border offer for 1.16-1.17.1 at simulation.rs:2950; move_towards_closest_space does not.) (`native/src/simulation.rs:2974-2982 (move_towards_closest_space suffocates_at)`)

#### D-213 · 26.3: moveTowardsClosestSpace is skipped for passengers, but the engine still runs it
- **Versions:** 26.3 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** 26.3 changes `if (!this.noPhysics) {` to `if (!this.noPhysics && !this.isPassenger()) { this.moveTowardsClosestSpace(...) x4 }`. (`E:/pr/mc-source/26.2_to_26.3.diff :40771-40775 (LocalPlayer.aiStep)`)
- **Native:** `if state.is_player && !state.no_physics { move_towards_closest_space(state, world, rules.release); }` has no `state.passenger` gate for 26.3. A passenger state sent through the N-API descriptor (napi_api.rs:2367) still gets the ±0.1 X/Z push-out velocity. (`native/src/simulation.rs:3911-3913`)

#### D-214 · Riptide launch runs after the before-player rocket boosts and entity pushes instead of before every entity
- **Versions:** 1.13.2-26.3 · **Severity:** minor (low) · **Kind:** tick-order · **Status:** confirmed
- **Vanilla:** Releasing the trident is handled in Minecraft.handleKeybinds (`this.gameMode.releaseUsingItem(this.player)`), which runs before `this.level.tickEntities()`. TridentItem.releaseUsing therefore applies `player.push(xd, yd, zd)` and, on the ground, `player.move(MoverType.SELF, new Vec3(0.0, 1.1999999284744263, 0.0))` before any entity ticks, including attached rockets and pushers that tick before the player. (`client26.3 client/Minecraft.java tick:1786-1799 (handleKeybinds before level.tickEntities), :1930 releaseUsingItem; client26.3 world/item/TridentItem.java:95-99`)
- **Native:** tick_local_player_scene first runs `boost_by_attached_rockets(state, rockets_before_player, release)` and `push_by_nearby_entities(state, world, nearby_entities, true, release)`, which take `state.bounds()` before the lift. Only then does the tick closure run `launch_riptide`. A rocket boost (0.5v + 0.85*look) taken before the riptide push differs by 0.5*push, and the lifted box (y+1.2) can stop overlapping a short pusher that the engine still counts. (`native/src/simulation.rs:778-779 (tick_local_player_scene: boosts and pushes for the entities ticked before the player), 3828-3834 (`state.riptide_launch.take()` inside the player's tick)`)

#### D-215 · From 1.19.1 a player removed this tick skips the whole tick, including the baseTick that vanilla still runs
- **Versions:** 1.19.1-26.3 · **Severity:** minor (low) · **Kind:** tick-order · **Status:** confirmed
- **Vanilla:** The removal comes from LocalPlayer.tickDeath, which LivingEntity.baseTick calls after `super.baseTick()`. So on the removal tick Entity.baseTick still runs updateInWaterStateAndDoFluidPushing (water push, resetFallDistance in water, lava push) and `if (this.isInLava()) { ... this.fallDistance *= 0.5F; }`. Only `if (!this.isRemoved()) { this.aiStep(); }` is skipped. (`origin/client1.21.4 world/entity/LivingEntity.java baseTick:378 (super.baseTick) and :455 (tickDeath); origin/client1.19.1 LivingEntity.java:2294-2295; Entity.baseTick`)
- **Native:** `if state.removed && rules.release.at_least(1, 19, 1) { return TickResult { displacement: Vec3::ZERO, ... } }` returns before the fluid push, the water fall reset and the lava fall halving, so the removed entity's velocity and fall distance are left as they were. (`native/src/simulation.rs:3770-3777`)

#### D-216 · Generic NativeSimulationState has no yaw-in-degrees field, so the 1.8.8 moveFlying yaw fix cannot be used through simulateVersionedTick/Ticks/TickOutcome
- **Versions:** 1.8.8 · **Severity:** minor (low) · **Kind:** precision · **Status:** confirmed · relates to D-145
- **Vanilla:** `float var5 = MathHelper.func_76126_a(this.field_70177_z * 3.1415927F / 180.0F); float var6 = MathHelper.func_76134_b(this.field_70177_z * 3.1415927F / 180.0F);` This rounds twice in float, starting from the float yaw in degrees. (`origin/client1.8.9 client/net/minecraft/entity/Entity.java func_70060_a (moveFlying) :846-847`)
- **Native:** NativePlayer sets `state.yaw_degrees = Some(values[YAW_DEGREES] as f32)`, and yaw_radians then applies the two-rounding 1.8 formula. The generic `From<NativeSimulationState> for SimulationState` always writes `yaw_degrees: None`, and NativeSimulationState has no field to carry it. So `move_relative_yaw` returns `state.yaw`, the caller's radians (the adapter's convention is `f32(yaw*0.017453292F)`). About 0.13% of yaws pick the adjacent sine-table entry. Only direct N-API callers are affected; the Mineflayer NativePlayer path is correct. (`native/src/napi_api.rs:196-310 (NativeSimulationState has no yaw_degrees), :774 (`yaw_degrees: None`); native/src/simulation.rs:2652-2666 (move_relative_yaw falls back to `state.yaw`)`)

#### D-217 · Ridden-mob movementSpeed/jumpStrength of exactly 0 are replaced by the mob's default base
- **Versions:** 1.8.8-26.3 (all registered releases; ridden-mob simulation is not version-gated beyond is_supported) · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** The attribute value is used as it is. Slowness VII or stronger (1 - 0.15*7 < 0) clamps MOVEMENT_SPEED to 0.0, and the ridden mob does not move. A JUMP_STRENGTH of 0 gives no jump. (`LivingEntity.getSpeed / AbstractHorse.getRiddenSpeed: `(float)this.getAttributeValue(Attributes.MOVEMENT_SPEED)` (RangedAttribute clamps to min 0.0)`)
- **Native:** `movement_speed: positive(value.movement_speed, defaults.movement_speed), jump_strength: positive(value.jump_strength, defaults.jump_strength)`. A real 0 sent by vehicles.ts (`movementSpeed: mob.movementSpeed`) is read as missing, and the mob moves or jumps at its default base speed. (`native/src/napi_api.rs:5383, 5398-5399 (`let positive = |value, default| if value > 0.0 { value } else { default }`)`)

#### D-218 · 1.9-1.12 eye height ignores the 0.4F glide / 0.6F-box case
- **Versions:** 1.9.4-1.12.2 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** confirmed
- **Vanilla:** `float f = 1.62F; if (isSleeping()) f = 0.2F; else if (isSneaking() || height == 1.65F) f -= 0.08F; else if (isElytraFlying() || height == 0.6F) f = 0.4F;` (`decompiled 1.9.4/1.10.2/1.12.2 net/minecraft/entity/player/PlayerEntity.java getEyeHeight (1.12.2 :1769-1780)`)
- **Native:** The 0.4F branch is gated to 1.13 only, so a gliding 1.9-1.12 player (kept box 0.6F) is sent EYE_HEIGHT 1.62F. The engine's is_in_wall uses this (simulation.rs:2919-2928), and so does the adapter's legacy eye-in-water test (updateLegacyFluids:799-802). (`src/mineflayer/player.ts:1239-1267 (eyeHeight: `const lowBox = !this.lt('1.13') && this.lt('1.14') && ...`)`)

#### D-219 · Before 1.14 the fluid probes use a pose-sized standing box, not the kept box (1.65F sneaking, 0.6F gliding or swimming)
- **Versions:** 1.13.2 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** confirmed
- **Vanilla:** Water contact is tested on the entity's actual bounding box. Before 1.14 that is the kept box, whose height EntityPlayer.updateSize sets to 1.65F sneaking or 0.6F gliding (1.13: also swimming or spin). (`decompiled 1.12.2 Entity.java:1051 (`this.getBoundingBox().expand(0.0, -0.4F, 0.0).contract(0.001)`); decompiled 1.13.2 Entity.java:2511-2512 (`Box lv = this.getBoundingBox().contract(0.001)`)`)
- **Native:** Builds the probe from POSE_SIZES[state.pose]. Pre-1.14 pose is never updated by the adapter, so this is normally the 1.8F standing box. The probe then reaches water up to 1.2 blocks higher than vanilla's: for example, a sneaking player whose feet are 0.601-0.75 into a block with water two cells up, or a 1.13 swimmer or crawler in an air pocket under water. In those cases isInWater is wrongly true, which affects swimming, sprint and fluid decisions. (`src/mineflayer/player.ts:785-787 (updateLegacyFluids: `this.collider(state.pose, state.pos, 1)`), :715 (1.13 path: `this.collider(state.pose, state.pos, state.scale)`)`)

#### D-220 · In-water cell scan includes cell floor(max) when the deflated max is an exact integer
- **Versions:** 1.13.2-26.3 · **Severity:** minor (low) · **Kind:** precision · **Status:** confirmed
- **Vanilla:** `int var6 = Mth.ceil(var4.maxX); ... for(int var18 = var5; var18 < var6; ++var18)`, so an exact-integer maxX/maxY/maxZ excludes that cell. (`origin/client1.21.11 Entity.java updateFluidHeightAndDoFluidPushing; decompiled 1.13.2 Entity.java:2512-2530`)
- **Native:** Iterates `<= Math.floor(max)`, which adds one extra column or row when the deflated max is an integer (e.g. y = k+0.201 standing, x = k+0.701). A water cell there sets isInWater, because any water's top is >= minY. (`src/mineflayer/player.ts:728-730 (`x <= Math.floor(deflated.maxX)`, same for y and z)`)

### 8.2 Plausible (one verifier)

#### D-221 · 1.19-1.20.4 rising-move guard is written as `delta.y > 0.0`, so a NaN vertical delta is handled differently than vanilla's `var1.y <= 0.0`
- **Versions:** 1.19-1.20.4 (registered: 1.19, 1.19.1, 1.19.2, 1.19.3, 1.19.4, 1.20, 1.20.1, 1.20.2, 1.20.3, 1.20.4) · **Severity:** minor (low) · **Kind:** precision · **Status:** plausible
- **Vanilla:** 1.19-1.20.4: `if (!this.abilities.flying && var1.y <= 0.0 && (var2 == MoverType.SELF || var2 == MoverType.PLAYER) && this.isStayingOnGroundSurface() && this.isAboveGround())`. With y NaN, `NaN <= 0.0` is false, so there is no back-off. From 1.20.5, `!(var1.y > 0.0)` is true for NaN, so the player backs off. (`origin/client1.19 client/net/minecraft/world/entity/player/Player.java maybeBackOffFromEdge:1056-1061 (same in client1.20.4 :1045-1050); from client1.20.5 :1029 it is `!(var1.y > 0.0)``)
- **Native:** `|| (release.at_least(1, 19, 0) && delta.y > 0.0)` returns early. For NaN, `NaN > 0.0` is false, so the engine backs off in 1.19-1.20.4 where vanilla does not. This is right from 1.20.5 on. A NaN vertical motion should not occur in normal play, so this only matters if NaN gets in. (`native/src/simulation.rs:2769`)

#### D-222 · 1.11.x: an attached rocket that ticks before the player boosts along the head yaw, which still holds last tick's yaw
- **Versions:** 1.11.2 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** plausible
- **Vanilla:** In 1.9-1.11 the living getRotation() uses headYaw. The player copies yaw into headYaw only in tickNewAi, inside its own aiStep. A rocket that ticks before the player therefore boosts along (current pitch, previous tick's yaw). From 1.12 getRotation uses this.yaw. (`E:/pr/mc-decompiled/1.11.2 net/minecraft/entity/FireworkRocketEntity.java:115 (`this.shooter.getRotation()`), LivingEntity.java:1927-1929 (`getRotation() { return this.getRotationVector(1.0F); }`) and getRotationVector(1.0F) -> `getRotationVector(this.pitch, this.headYaw)`; PlayerEntity.java:466-469 tickNewAi `this.headYaw = this.yaw;`. 1.12.2 Entity.java:1739-1741 changes getRotation to (pitch, yaw)`)
- **Native:** Every rocket boost uses `let look = state.look;`, the view vector of the current yaw and pitch, including the rockets before the player (`boost_by_attached_rockets(state, rockets_before_player, release)` in tick_local_player_scene). The engine has no previous-yaw input. (`native/src/entity.rs:385-402 boost_by_attached_rockets (uses state.look); src/mineflayer/player.ts:277 (look from the current yaw)`)

#### D-223 · Adapter cancels an attached rocket once the player is not gliding; vanilla keeps it attached and it boosts again if the glide restarts
- **Versions:** 1.11.2-26.3 (count/duration path only; rockets passed as attached_rocket nearby entities already match vanilla) · **Severity:** minor (low) · **Kind:** mismatch · **Status:** plausible
- **Vanilla:** `if (this.attachedToEntity != null) { if (this.attachedToEntity.isFallFlying()) { ...boost... } else { var2 = Vec3.ZERO; } this.setPos(...); }`. When the owner is not fall flying the rocket stays attached and keeps counting its life. It explodes only at its lifetime, on the server, and it boosts again on any later tick on which the owner glides. (`origin/client1.21.11 FireworkRocketEntity.java:105-131 tick`)
- **Native:** The first tick that sees `!state.fallFlying` sets fireworkRocketDuration = 0, so the rocket is gone for good. A glide that stops (1.21.5+ stopFallFlying on a climbable, or a creative flight toggle) and restarts before the rocket's lifetime ends gets no further boosts, where vanilla boosts again. (`src/mineflayer/player.ts:223-227 and 253-256 (`if (state.fallFlying) rocketsBefore = ...; else state.fireworkRocketDuration = 0;`)`)

#### D-224 · Slow Falling gravity is chosen from the velocity before the firework boost, although the engine applies the boost as already ticked
- **Versions:** 1.13.2-26.3 (fireworkBoost descriptor path only; the scene's attached-rocket path boosts before travel and is not affected) · **Severity:** minor (low) · **Kind:** tick-order · **Status:** plausible
- **Vanilla:** From 1.21.2 updateFallFlyingMovement reads `double var8 = this.getEffectiveGravity();`, which tests `this.getDeltaMovement().y <= 0.0`. Before 1.21.2, travel opens with `boolean var4 = this.getDeltaMovement().y <= 0.0; if (var4 && hasEffect(SLOW_FALLING)) var2 = 0.01` (Math.min from 1.20.5). Under the engine's own model (the D-019 fix: 'the rocket has already ticked'), getDeltaMovement already includes the rocket boost at this point. (`origin/client1.21.11 client/net/minecraft/world/entity/LivingEntity.java:2267-2269 getEffectiveGravity, :2434-2441 updateFallFlyingMovement; origin/client1.19.4 LivingEntity.java:1994-1997; origin/client1.21.1 :2117-2120`)
- **Native:** going_down/travel_gravity are computed before `state.add_delta_movement(state.firework_boost, ...)`. The glide then runs on the boosted velocity but with gravity picked from the pre-boost sign: 0.01 instead of 0.08 (or the reverse) when the boost flips the sign of y. Before 1.20 the travel fall-distance reset under Slow Falling (:4066) uses the same pre-boost test. (`native/src/simulation.rs:4062-4078 (`let going_down = state.velocity.y <= 0.0;` ... `else if state.slow_falling && going_down { rules.gravity.min(0.01) }`), boost added at :4172, glide(state, travel_gravity, ...) at :4185`)

#### D-225 · 1.21.9+ forEachBlockIntersectedBetween corner walk uses the recomputed toCorner - fromCorner as the step vector, not deltaMove
- **Versions:** 1.21.9-26.3 · **Severity:** minor (low) · **Kind:** precision · **Status:** plausible
- **Vanilla:** Signs and step lengths come from the travel vector itself: `int signX = Mth.sign(deltaMove.x); ... double tDeltaX = signX == 0 ? 1.7976931348623157E308 : (double)signX / deltaMove.x;`. Only `Vec3 fromCorner = toCorner.subtract(deltaMove);` and the AABB.clip(fromCorner, toCorner) use the corners. (`origin/client1.21.11 client/net/minecraft/world/level/BlockGetter.java addCollisionsAlongTravel (same in 1.21.9/1.21.10 and 26.1-26.3)`)
- **Native:** walk_cells derives the direction from its endpoints: `let delta = to - from; ... let signs = (sign(delta.x), ...); f64::from(sign) / delta`. Here (to_corner - travel) is subtracted back from to_corner. This is exact in the 1.21.2 and 1.21.5 variants, where vanilla also subtracts inside the walk. In 1.21.9+ the result can differ from deltaMove by an ulp of the corner coordinate. Exact ties such as equal |dx| and |dz| then break differently, which changes the iteration index (advanceStep grouping and the 16-iteration budget). A component tiny next to the coordinate can round to 0, so the sign is 0 where vanilla's is ±1. Vanilla then still steps a negative axis at t=0 when frac(fromCorner) == 0. (`native/src/block_traversal.rs:349-351 (from_corner = to_corner - travel; walk_cells(from_corner, to_corner, ...)) and :381-404 (let delta = to - from; signs/steps from delta)`)

#### D-226 · checkInWater's starting waterLevel is Double.MIN_VALUE until 1.16.5, but the engine uses -Double.MAX_VALUE from 1.13
- **Versions:** 1.13.2-1.16.5 · **Severity:** minor (low) · **Kind:** version-gate · **Status:** plausible
- **Vanilla:** waterLevel starts at 4.9E-324 (Double.MIN_VALUE) in 1.9-1.16.5 and at -Double.MAX_VALUE from 1.17. (`origin/client1.14.4 Boat.java:518 and 1.16.4 :492 `this.waterLevel = 4.9E-324D;`; E:/pr/mc-decompiled/1.15 and 1.16.5 BoatEntity `this.waterLevel = Double.MIN_VALUE;`; 1.17 decompiled :531 and 1.18.1 Boat.java:505 `-1.7976931348623157E308``)
- **Native:** Switches to -f64::MAX at 1.13 instead of 1.17. The difference shows only in the reported water_level (round-tripped through napi_api.rs:5610) when no water is found. Movement is unaffected, because floatBoat reads waterLevel only in IN_WATER, where it was just set. (`native/src/vehicle.rs:564-568 (`if release.at_least(1, 13, 0) { -f64::MAX } else { f64::from_bits(1) }`)`)

#### D-227 · 1.19.3 camel that refuses to move uses AbstractHorse.travel's non-controlled branch (its own decaying input, flyingSpeed 0.02); the engine uses zero input
- **Versions:** 1.19.3 · **Severity:** minor (low) · **Kind:** mismatch · **Status:** plausible
- **Vanilla:** 1.19.3 has no getRiddenInput/tickRidden. AbstractHorse.travel steers only when `this.isVehicle() && var2 != null && !this.mountIgnoresControllerInput(var2)`. Camel.mountIgnoresControllerInput returns `this.refuseToMove() || super...`, so a sitting or standing-up camel takes `else { this.flyingSpeed = 0.02F; super.travel(var1); }`. That uses the camel's own (xxa, yya, zza): aiStep decays it by 0.98, and Mob.setSpeed set it to the driven speed on earlier steered ticks. There is no setRot and no setSpeed. Camel.travel zeroes the input only `if (this.refuseToMove() && this.isOnGround())`. (`origin/client1.19.3 client/net/minecraft/world/entity/animal/camel/Camel.java travel/mountIgnoresControllerInput (:219-249); AbstractHorse.java travel (:739-780)`)
- **Native:** Uses the 1.19.4+ getRiddenInput shape for every release: `if refuses { Vec3::ZERO } else { horse_input(rider) }`, sets `mob.ridden_speed = movement_speed + bonus`, and the air speed is `legacy_flying_base(...) * 0.1`. Off the ground (in the air or in water while sitting down or during the 52-tick stand-up), a 1.19.3 camel keeps drifting on its residual zza at speed 0.02 in vanilla but stops accepting input in the engine. (`native/src/ridden.rs:566-595 (tick_ridden Camel arm), 371-377 (air_speed)`)

#### D-228 · 1.17-1.21.1: the riptide lift's Entity.move skips its fire part (fire-block ignition count and the -getFireImmuneTicks reset)
- **Versions:** 1.17-1.21.1 · **Severity:** minor (low) · **Kind:** missing · **Status:** plausible · relates to D-054
- **Vanilla:** Every Entity.move, the lift included, runs `this.tryCheckBlockCollision();` (fire blocks: `entity.setFireTicks(entity.getFireTicks() + 1); if (entity.getFireTicks() == 0) entity.setOnFireFor(8);`, lava cauldron setOnFireFromLava, water cauldron extinguish). It then runs `if (getStatesInBoxIfLoaded(box.contract(1.0E-6)).noneMatch(FIRE||LAVA)) { if (this.fireTicks <= 0) this.setFireTicks(-this.getBurningDuration()); }` and `if (this.isOnFire() && (this.inPowderSnow || this.isWet())) this.setFireTicks(-this.getBurningDuration());`. (`1.17 (E:/pr/mc-decompiled/1.17) Entity.move :629-646; AbstractFireBlock.onEntityCollision :121-129`)
- **Native:** Before 1.21.2 the lift runs `apply_block_effects(state, world, support, release, &inside)` (movement effects only). apply_inside_fire_effects runs only after the tick's travel move. So a fire block touched by the post-lift box counts one ignition tick fewer, and the lift's reset and extinguish step is skipped. This reaches movement only through the fire-cleared freeze ticks (powder-snow slow). (`native/src/simulation.rs:3385-3391 (launch_riptide calls apply_block_effects only)`)

#### D-229 · 26.3 elytra start tests lava with last tick's pre-move engine flag
- **Versions:** 26.3 · **Severity:** minor (low) · **Kind:** tick-order · **Status:** plausible · relates to D-050
- **Vanilla:** `if (!this.isFallFlying() && this.canGlide() && !this.isInLiquid())`, with isInLava refreshed in this tick's baseTick at the current position. (`origin/client26.3 Player.java:1351-1352 tryToStartFallFlying; Entity.baseTick updateFluidInteraction (:574) before aiStep`)
- **Native:** updateFluids recomputes only isInWater before the aiStep port. isInLava is the engine output of the previous tick, measured before that tick's move, so the lava gate lags vanilla by one tick when entering or leaving lava. (`src/mineflayer/player.ts:1066 (`const inLava = state.isInLava && !this.lt('26.3')`); state.isInLava is set only in takeOutput (:431) from the engine's base-tick value (simulation.rs:3898-3899)`)

### 8.3 Areas checked 1:1 and gaps

- **Fall distance and fall damage.** Matched vanilla: Entity.checkFallDamage/fall/updateFallState order (onGround: fallOn if fd>0, then reset; else accumulate if dy<0): 1.8.9, 1.9.4, 1.12.2, 1.13.2, 1.14.4, 1.16.4, 1.18.1, 1.19, 1.20.4, 1.21.4; 1.21.5+ order (water refresh, count the drop if !inWater && dy<0, then land): 1.21.5, 1.21.11, 26.1, 26.2, 26.3 (D-001 fixed); accumulated_fall_distance precision: (float)((double)fd - dy) for 1.8.9-1.18.1, fd -= (float)dy in float for 1.19-1.21.4, fd -= (double)(float)dy from 1.21.5 (D-005); LivingEntity.checkFallDamage water refresh before super (legacy handleWaterMovement before 1.13, updateInWaterState in 1.14, updateInWaterStateAndDoWaterCurrentPushing in 1.16-1.21.11, updateFluidInteraction with lava in 26.1+); checkFallDamage gating: all releases before 1.21.2; isControlledByLocalInstance for 1.21.2-1.21.3; the 1.21.4 `&& !isControlledByClient()` player exclusion (Player.isControlledByClient returns true); isLocalInstanceAuthoritative from 1.21.5; landing_damage before 1.20.5: Mth.ceil((fd - 3.0F - jumpBoost) * mult) in float (1.8.9, 1.12.2, 1.13.2, 1.16.4, 1.19.4, 1.20.4); attribute_landing_damage: 1.20.5-1.21.4 Mth.ceil((double)((fd - (float)SAFE) * blockMult) * FDM); 1.21.5-26.3 Mth.floor((fd + 1.0E-6 - SAFE) * (double)blockMult * FDM); rounding contract Ceil before 1.21.5, Floor from it; modern_fall_damage profiles 1.20.5, 1.20.6 and 1.21+; Player.causeFallDamage mayfly gate covered by fall_damage_immune (1.12.2, 1.20.4, 1.21.4, 1.21.11); HayBlock 0.2F from 1.9 (1.9.4, 1.10.2, 1.11.2, 1.12.2, 1.13.2, 1.14.4, 1.21.11, 26.3; absent in 1.8.9); HoneyBlock 0.2F (1.15, 1.16.4, 1.21.11, 26.3); BedBlock fd*0.5 from 1.12 (absent in 1.11.2; 1.12.2-26.2), and the 26.3 Block.fallOn fallDistanceReduction 0.5 for beds and shelf_mushroom, with straw_bed excluded; Bed bounce -vy*0.66F for LivingEntity (1.12.2-26.1); Slime fallOn: causeFallDamage(fd, 0) unless sneaking before 1.21.2; from 1.21.2 no damage either way (1.21.2, 1.21.11, 26.2, 26.3); slime bounce -vy for LivingEntity; PowderSnowBlock fallOn without damage from 1.18 (absent in 1.17 jar); PointedDripstone upward tip: fd+2.0F with mult 2.0F for 1.17-1.21.4, fd+2.5 double from 1.21.5; the sulfur spike has no fallOn (26.3); Block.updateEntityAfterFallOn default: vy*0.0 from 1.14 and motionY=0 before; Landing block position: floor(y-0.20000000298023224) before 1.20 (the fence fallback is equivalent because it applies only over air), support-based from 1.20; Fall-reset clip in Entity.move: 1.19 end = position+movement; 1.21.9+ min(length, 8) along the normalized movement; fd != 0 and lengthSqr >= 1; Lava halving of fallDistance in baseTick (1.8.9, 1.14.4, 1.16.4, 1.21.11, 26.1, 26.3); checkFallDistanceAccumulation (1.21.11): vy > -0.5 && fd > 1.0 -> 1.0.
  Not covered: Not covered: 1.18.2 and 1.17.1 have no source; the engine treats 1.18.2 as 1.18.1 for accumulation and gates the clip at 1.18.2, and neither can be verified. 1.9.4-1.11.2 and 1.15.x were checked only for the Entity.fall shape and the block overrides (decompiled). Ridden-mob and vehicle fall handling (ridden.rs, vehicle.rs, Strider.fall) is out of scope; move_mob does a water refresh, but whether it accumulates or lands was not traced. The 26.2/26.3 restituteMovementAfterCollisions bounce math is left to the collision/restitution audit; only its interaction with fallOn damage was checked. Inside-block fall resets (cobweb/stuck, honey slide, bubble column, berry bush, powder snow falling shape), Slow Falling/Levitation/elytra/flight resets (D-006/D-007/D-008/D-011) and the sneak-edge fallDistance < maxUpStep check were not re-verified line by line. The adapter's synced SAFE_FALL_DISTANCE and FALL_DAMAGE_MULTIPLIER inputs were only read, not traced end to end. Wind-charge/mace impulse fall clamping (Player.currentImpulseImpactPos) is set only server-side (ServerPlayer, MaceItem), so it does not apply to the client simulation.
- **Sneak edge back-off and no-collision queries.** Matched vanilla: back_off_while vs vanilla 0.05 loops, x / z / diagonal order. Pre-1.20.5 `x < 0.05 && x >= -0.05 -> 0 else x -= / += 0.05` (1.8.9, 1.10.2, 1.12.2, 1.13.2 Entity.move; client1.14.4 Entity.applySneaking; 1.16.4 / 1.18.1 / 1.19 / 1.20.4 Player) and 1.20.5+ `Math.signum(x)*0.05` with `abs<=0.05 -> 0` (client1.20.5, 1.21.1, 1.21.4, 1.21.5, 1.21.11, 26.1, 26.2, 26.3) give identical doubles. Java signum(-0.0) vs Rust signum is unreachable because the loop requires x != 0.; back_off_from_edge_before_1_14: gate onGround && sneaking && player && !noClip; drop is 1.0 for 1.8.9-1.10.2 and -(double)stepHeight from 1.11.2 (1.9.4/1.10.2 `offset(x,-1.0,0)`, 1.11.2/1.12.2/1.13.2 `offset(x,-this.stepHeight,0)`); whole box offset with no deflate. The query happens after the web slowdown, as in vanilla. PISTON moves skip it from 1.11 (MoverType), and before 1.11 they do not skip it (moveEntity).; back_off_from_edge 1.14.4-1.16.1: `this instanceof Player && (SELF||PLAYER) && onGround && isSneaking()`, no flying or y test, whole box moved by (x, (double)(-maxUpStep), z) (client1.14.4 Entity.java:600-649; decompiled 1.15 / 1.16 PlayerEntity.adjustMovementForSneaking).; 1.16.2-1.18.x: `!abilities.flying && ... && isStayingOnGroundSurface() && isAboveGround()`; isAboveGround = `onGround || fallDistance < maxUpStep && !noCollision(move(0, (double)(fallDistance - maxUpStep), 0))` with float subtraction. The engine's -(f64)(step - fd) equals (f64)(fd - step) exactly (1.16.4, 1.16.5, 1.17, 1.18, client1.18.1).; 1.19-1.20.4: y<=0 guard, maxUpStep() float, whole-box probes (client1.19, 1.20.4; decompiled 1.19.2, 1.20, 1.20.2).; 1.20.5-1.21.4 canFallAtLeast `new AABB(minX+x, minY-(double)step-9.999999747378752E-6, minZ+z, maxX+x, minY, maxZ+z)` with float isAboveGround `var1 - fallDistance` (client1.20.5, 1.20.6, 1.21, 1.21.1, 1.21.2, 1.21.3, 1.21.4): same operation order and constants as the engine.; 1.21.5+ canFallAtLeast `minX+1.0E-7+x, minY-h-1.0E-7, ..., maxX-1.0E-7+x, minY, maxZ-1.0E-7+z` with double fallDistance `(double)var1 - fallDistance` (client1.21.5, 1.21.6, 1.21.11, 26.1, 26.2, 26.3; mc-source 26.3 Player.java:831-883): 1:1.; Call order in Entity.move: stuck-speed scaling, then maybeBackOffFromEdge, then collide, only when !noPhysics; 1.21.10+ PISTON is not scaled (client1.21.11 Entity.java:735-756). This matches simulation.rs:4207-4227 and piston.rs:565-577.; moves_position: `var4.lengthSqr() > 1.0E-7` up to 1.21.1, plus `|| var2.lengthSqr() - var5 < 1.0E-7` from 1.21.2 (client1.21.1 Entity.java:653, client1.21.2 :682, client1.21.11). Gate and operand (requested = backed-off delta) match.; no_collision_for_entity structure: 1.18+ noBlockCollision && noEntityCollision && noBorderCollision (client1.18.1, 1.20.4, 1.21.4, 1.21.11, 26.3 CollisionGetter); 1.14.4 getCollisions allMatch isEmpty, with the border shape offered first when the entity touches it from inside (deflate/inflate 1e-7) and entity collisions skipped when size<1e-7 and otherwise inflated by 1e-7 (client1.14.4 LevelReader/EntityGetter); 1.16 adds the isInWorldBorder shortcut (decompiled 1.16 BlockCollisionSpliterator); 1.13.2 World.method_16369 concatenates entity boxes (class_3593: expand 0.25, intersects the query); 1.12.2 World.getCollisionBoxes adds stone outside the border for an entity inside, plus entity boxes. The outside-border side effect is idempotent across repeated probes, so passing the flag immutably matches.; isStayingOnGroundSurface = isShiftKeyDown in every release 1.15-26.3, and 1.14.4 isSneaking = input.sneakKeyDown, both modeled by state.sneaking..
  Not covered: I could not check the exact 1.16.1 -> 1.16.2 breakpoint (isAboveGround and the flying test) or the 1.18.1 -> 1.18.2 / 1.18.2 -> 1.19 boundaries for the y guard from source. The git repo has only 1.16.4, 1.18.1 and 1.19, and the decompiled jars have 1.16, 1.16.5, 1.17, 1.18 and 1.19. These boundaries agree with D-012's javap-pinned notes. I did not re-audit how the shared block-shape collection works inside no_collision_for_entity: cell ranges, the shape_meets_query / is_shapes_block merger tolerance, large-shape neighbour cells, pre-1.13 corner-column skipping in World.getCollisionBoxes, or scaffolding and powder-snow context. I did not audit entity_collisions filtering (canCollideWith, vehicles) or world-border geometry helpers beyond their gates. These are shared collision code covered by D-013/D-014/D-015 and other audit areas. I also did not trace the inputs: state.sneaking from client input, fall_distance float accumulation (accumulated_fall_distance), and rules.step_height for the 1.20.5+ STEP_HEIGHT attribute, or the LivingEntity.maxUpStep max(…,1) when a Player controls the entity.
- **Block surfaces, speed/jump factors, stuck blocks, climbing, inside-block effects.** Matched vanilla: block_speed_factor: water/bubble-column exception, 0.4 soul sand/honey, Player flying/fall-flying 1.0, 1.16-1.20.6 Soul Speed (1.16/1.16.1 y-0.2F, 1.16.2+ block below), 1.21+ Mth.lerp(movement_efficiency) in float; applied in Entity.move after the inside pass (1.15-26.3: 1.15 decomp, 1.16.4, 1.18.1, 1.19.4, 1.20.4, 1.21.1, 1.21.11, 26.3); block_jump_factor: honey at blockPosition, else block below that affects movement (1.15 decomp, 1.16.4-26.3); block_below_affecting_movement: y-1.0 before 1.15 (1.14.4 travel minY-1.0), y-0.5000001 for 1.15-1.19.4 (BlockPos/containing floor), getOnPos(0.500001F) with the supporting block's x/z from 1.20 (1.20.4, 1.21.11, 26.3). The wall/gate exemption gives the same cell for 0.500001; landing_position/stepOn cell: floor(y-0.2F) as f32 in 1.8.9-1.19.4, getOnPosLegacy from 1.20. The fence-fallback/fence-wall exemptions only swap in fences, walls or gates, which have no stepOn; SlimeBlock.stepOn (|vy|<0.1, 0.4+|vy|*0.2, isSneaking/isSteppingCarefully) 1.8.9, 1.14.4, 1.16.4, 1.19.4, 1.21.1, 1.21.11, 26.3; 1.15-1.18 entity-level !isSteppingCarefully gate; stepOn/inside ordering: before 1.21.2 after velocity zeroing and updateEntityAfterFallOn inside move; from 1.21.2 in applyEffectsFromBlocks after travel (1.21.4, 1.21.11, 26.3); blocks_inside: 0.001 deflation before 1.19.3, 1e-7 1.19.3-1.21.1, x/y/z loop order (1.8.9, 1.14.4, 1.19.1, 1.19.3, 1.20.4, 1.21.1); stuck_speed_multiplier_inside: WebBlock (0.25, 0.05F, 0.25) and Weaving (0.5, 0.25, 0.5); sweet berry (0.8F, 0.75, 0.8F) with fox/bee immunity; powder snow (0.9F, 1.5, 0.9F) gated on feet block or non-living; Player.makeStuckInBlock flying gate incl. no fall reset; pre-1.14 setInWeb y*0.05F and Player override (1.8.9, 1.12.2, 1.14.4, 1.16.4, 1.18.1, 1.20.4, 1.21.1-1.21.11, 26.3); push_by_bubble_column: onAboveBubbleCol/onInsideBubbleColumn constants and fall reset; isAir test before 1.21.5, empty collision and empty fluid from 1.21.5; isPrecise gate from 1.21.10; Player flying exemption from 1.21.2 (1.13.2, 1.14.4, 1.16.4, 1.20.4, 1.21.1, 1.21.2, 1.21.4, 1.21.5-1.21.11, 26.3); honey_slide_falling/sliding_down_honey: isSlidingDown and doSlideMovement, with getOldDeltaY/getNewDeltaY from 1.21.2 (1.16.4, 1.21.1, 1.21.2, 1.21.11, 26.3); pre-1.15 soul sand entityInside x/z*0.4 (1.8.9, 1.13.2, 1.14.4); none in 1.15; climbable: ladder/vine (+trapdoor over a same-facing ladder from 1.9, scaffolding from 1.14, #climbable from 1.16); Player flying gate from 1.21.4 (absent in 1.21.3); #can_glide_through gate from 1.21.11 (absent in 1.21.10) and its tag contents in 1.21.11/26.3; 0.15F clamps, jumping climb from 1.14, powder-snow climb (feet block before 1.21.5, wasInPowderSnow from 1.21.5); blocks_inside_moves / movements_this_tick: Movement recording condition, 9.999999439624929E-11 tail, axis split by collided movement 1.21.5-1.21.8 and by requested movement from 1.21.9, 16-step budget and final-box fallback from 1.21.9, visited-set marking of failed blocks before 1.21.9, shape test (powder snow FALLING_COLLISION_SHAPE past 2.5, cauldron content from 1.21.9), fluid AABB test, AABB.collidedAlongVector inset 0 (1.21.5) vs 1e-7 (1.21.11), forEachBlockIntersectedBetween 1.21.9+ (betweenCornersInDirection, furthest corner walk); 26.3 applyEffectsFromBlocks/checkInsideBlocks/getOnPos/getBlockSpeedFactor/HoneyBlock/WebBlock/PowderSnowBlock/BubbleColumnBlock match 1.21.11 apart from canSimulateMovement guards (true for the local player).
  Not covered: I did not compare the 1.21.5-1.21.8 forEachBlockIntersectedBetween/addCollisionsAlongTravel walk index by index; I only checked its structure and thresholds. walk_cells and clip were taken as correct apart from the 1.21.4 nudge. I did not audit the ridden.rs, vehicle.rs, piston.rs and projectile.rs callers or ridden.rs's own block_jump_factor/on_climbable (out of the simulation.rs scope). Boat bubble-column overrides were only noted. 1.9.4-1.12.2 and 1.15.x were only spot-checked through the Yarn decompiles (stepOn gate, 0.2F offset, setInWeb, velocity-affecting pos); 1.17.x and 26.1/26.2 were not opened separately. The adapter's sneaking/stepping_carefully semantics (LocalPlayer.isShiftKeyDown is the raw shift key, even while flying) were not traced end to end. apply_inside_block_signals (contact damage, freeze ticks) was only skimmed because D-024 covers it. The fire-immune check on magma (1.10-1.18 `!fireImmune()`) affects only the contact_damage signal and is not reported.
- **Fluids (player path).** Matched vanilla: push_by_fluid vs Entity.updateFluidHeightAndDoFluidPushing / checkAndHandleWater: average for the player, normalize for others, float-sqrt length/normalize before 1.17 (Vec3d via MathHelper.sqrt in 1.15/1.16/1.16.5, Math.sqrt from 1.17), the 0.003/0.0045 minimum from 1.16 (absent in 1.14.4/1.15), the 1.0E-5F normalize from 1.21.2, the 26.1+ EntityFluidInteraction currentCount/lengthSqr<9.999999747378752E-6 gate (1.14.4, 1.15, 1.16, 1.16.5, 1.21.11, 26.1, 26.3); world.fluid_contact before 26.1: deflate 0.001, floor..ceil x/y/z order, float (y + getHeight), touching when top >= deflated minY, the running-max height scaled by flow when < 0.4, height measured from the deflated minY (1.14.4, 1.16.4, 1.21.11); world.fluid_contact from 26.1: double fluidBottom + height, height from the undeflated minY, touching = height > 0, chunk-loaded gate over the cells widened by 1; 26.3 per-Holder Tracker/CurrentAccumulator split gives the same height and flow scaling (26.1, 26.3); fluid_height_at vs FlowingFluid.getHeight/getOwnHeight: hasSameAbove gives 1.0F, else amount/9F with source/falling = 8 (1.14.4, 1.21.11, 26.3); 1.13 own height only (fluid_contact_1_13 vs 1.13.2 func_210500_b); legacy_fluid::handle_water_movement vs 1.8.9 World.func_72918_a / 1.12.2 handleMaterialAcceleration: expand(0,-0.4,0)+contract(0.001), floor(max+1) in 1.8 and ceil from 1.9, float (y+1 - heightPercent), int top comparison, normalized summed flow * 0.014 gated on canFly/!flying; legacy_fluid::touching_lava vs 1.8.9/1.12.2/1.13.2 isInLava: box shrunk by 0.1F/0.4F, Material.LAVA, isMaterialInBB ranges; legacy_fluid::is_free / contains_any_liquid vs 1.8.9 isAnyLiquid (floor..floor+1), 1.12.2 containsFluid and 1.13.2 IWorldReaderBase.containsAnyLiquid (floor..ceil), 1.21.11 and 26.2; legacy_fluid::sink_in_water and the lava branch vs 1.8.9 / 1.12.2 moveEntityWithHeading: *0.8F then -0.02 (with the 1.12 noGravity check), lava *0.5 then -0.02, 0.6F hop offset and 0.3F hop, depth strider cap 3 / *0.5F off ground / /3F; legacy_fluid_travel_after_move vs LivingEntity.travel 1.13.2, 1.14.4, 1.16.4, 1.18.1, 1.19.4, 1.20.4, 1.20.6, 1.21.1: water climb 0.2 from 1.14 only, multiply(slowdown, 0.8, slowdown), sprint 0.9F, dolphins 0.96F after efficiency, depth strider /3F until 1.20.6 and the WATER_MOVEMENT_EFFICIENCY attribute from 1.21, lava 0.5 with the shallow-lava (<= jump threshold) branch from 1.16, -gravity/4 under !isNoGravity (var2 != 0 from 1.20.5), hop isFree(x, y+0.6000000238418579-getY()+startY, z); travel_in_fluid_after_move vs 1.21.2/1.21.4/1.21.5 travelInFluid and 1.21.11/26.1/26.3 travelInWater/travelInLava/jumpOutOfFluid: getEffectiveGravity from the start-of-travel delta, the same water/lava math, hop after both branches; fluid_falling_adjusted_y and fluid_falling_gravity_enabled vs getFluidFallingAdjustedMovement: !isNoGravity && !isSprinting until 1.20.4, gravity != 0 && !isSprinting from 1.20.5 (1.20.4, 1.20.6, 1.21.1, 1.21.11, 26.3); fluid_jump vs aiStep jump: 1.8.9/1.12.2 isInWater-or-lava swim, 1.13.2/1.14.4/1.15 waterHeight vs 0.4 then isInLava, 1.16+ isAffectedByFluids gate with getFluidJumpThreshold and the lava height while in lava, 26.3 isInShallowFluid(LAVA) form equivalent; jumpInLiquid +0.04F; update_in_lava_after_move: 1.14.4 move `isInLava = false; checkInsideBlocks()` with LiquidBlock.entityInside setInLava (deflated box, inclusive), 1.16 FluidBlock.onEntityCollision `minY < f || f > maxY`; after-move water refresh in checkFallDamage (`if (!isInWater()) updateInWaterState...`): 1.8.9, 1.12.2, 1.13.2, 1.14.4, 1.16.4, 1.21.4 (skipped for the client player by !isControlledByClient), 1.21.5, 1.21.11; 26.1/26.3 updateFluidInteraction also refreshing and pushing lava; base-tick order water then lava, resetFallDistance in water, ultraWarm/FAST_LAVA 0.007 vs 0.0023333333333333335, isInLava = lava height > 0 from 1.16.2 (1.16.4, 1.21.11, 26.1, 26.3); adapter flowAt vs FlowingFluid.getFlow (1.21.11): affectsFlow, own-height deltas in float incl. 0.8888889F, falling -6 after normalize, final normalize; legacyFlowAt vs 1.12.2 BlockLiquid flow (decay ints, below-neighbour -8, level>=8 falling).
  Not covered: I did not re-audit 1.16.0-1.16.1 lava handling while in water (D-035, deferred). I did not compare the adapter's blocksMotion/isFaceSturdy approximation in getFlow (D-032). Player.travel swim steering (D-030) and eye-in-fluid flags (D-037/D-038, including 26.3 getHeightForCamera, which the engine's own eyes_in_fluid leaves out) are outside the movement math. firstTick in isInLava() (1.16.2+), boat/vehicle passenger exceptions in updateInWaterState*, and the bubble-column effects applied inside the move were not checked. Sources for 1.9.4-1.11.2, 1.15.x, 1.16.0-1.16.3, 1.17.x and 1.18.2 were only spot-checked through the decompiled jars (1.15, 1.16, 1.16.5, 1.17 Vec3d) or bracketed by neighbouring releases. I did not check whether 26.3 also changed noCollision (the other half of isFree) to the inclusive BlockScanUtils scan; that belongs to the collision area but would also affect the fluid hop. I ran no tests or engine code; the findings come from reading the source.
- **Ground/air travel, input, friction, jump.** Matched vanilla: Velocity zeroing thresholds: 0.005 in 1.8.9; 0.003 per axis in 1.9.4, 1.12.2, 1.13.2, 1.14.4-1.21.4; from 1.21.5 the player uses horizontalDistanceSqr < 9.0E-6 (0.003*0.003 == 9.0E-6 as a double) and y < 0.003 (1.21.5, 1.21.11, 26.1-26.3). Placement after goDownInWater and the flight impulse, before jump, matches LocalPlayer/LivingEntity.aiStep.; input_move_vector before 1.21.5: 1.8.9/1.13.2 MovementInputFromOptions `(float)((double)x*0.3D)`; 1.14.4-1.18.1 KeyboardInput double 0.3; 1.19-1.21.3 float sneak factor; item-use 0.2F after the sneak through 1.21.3 and before it from 1.21.4 (1.21.2/1.21.3 tick(isMovingSlowly, var5) vs 1.21.4 LocalPlayer:642-655).; Flying sneak undo `(float)((double)x / 0.3D)`, 1.9-1.14 only (1.9.4/1.12.2 decompiled, 1.13.2 :760-771, 1.14.4 :682-689; absent in 1.15 decompiled and in 1.8.9).; 1.21.5+ KeyboardInput normalization (Vec2.normalized, threshold 1.0E-4F) and LocalPlayer.modifyInput / modifyInputSpeedForSquareMovement / distanceToUnitSquare, including the 0.98F, item multiplier and SNEAKING_SPEED order (1.21.5, 26.3).; movement_input: 0.98F scale for non-players and for players before 1.21.5; yya passed through unscaled.; getInputVector / moveRelative from 1.14: lengthSqr < 1.0E-7, normalize above 1.0, scale by the float speed, Mth.sin(float) before 1.21.11 and Mth.sin(double) from 1.21.11 (1.14.4, 1.16.4, 1.19.4, 1.21.1, 1.21.4, 1.21.11, 26.3).; Float moveFlying before 1.14: the 1.0E-4F gate, sqrt clamped to 1, the 3.1415927F/180 yaw form in 1.8.9 and 0.017453292F from 1.9 (1.8.9, 1.9.4, 1.13.2).; getFrictionInfluencedSpeed: 0.16277136F (1.8.9-1.12.2) and 0.16277137F (1.13.2) over (friction*0.91F)^3; 0.21600002F/f^3 for 1.14.4-26.1; the 26.2/26.3 `(double)blockFriction > 0.6` branch.; Air travel: ground inertia friction*0.91F, 0.91F off ground, vertical 0.98F; levitation formula; gravity, then the Slow Falling min(…, 0.01) (1.14-1.20.4 fixed 0.01, 1.20.5+ getGravity, 1.21.2+ post-move getEffectiveGravity); the climb 0.2 with horizontalCollision||jumping from 1.14; 26.2/26.3 computeModifiedFriction for friction, air drag and vertical drag (and 1-(1-f) is exact for every vanilla friction >= 0.5 in 1.21.2-26.1).; Player flying speed: Player.flyingSpeed is set after super.aiStep, so the previous tick's sprint, before 1.19.4 (1.14.4-1.19.3, plus jumpMovementFactor in 1.8.9/1.13.2 with (double)0.02F*0.3D == 0.005999999865889549); `+= 0.006F` in 1.19/1.19.3; 0.025999999F from 1.19.4; abilities flyingSpeed*2 when sprinting; the flight y*0.6 carry (1.14.4, 1.19.4, 1.21.1, 1.21.2, 1.21.5, 26.3).; Flight vertical impulse: separate double sneak/jump steps before 1.14, one netted float impulse from 1.14 (1.8.9, 1.13.2, 1.14.4, 26.3).; goDownInWater -0.03999999910593033: the isAffectedByFluids gate from 1.16 (1.14.4, 1.16.4, 1.21.4, 26.3).; Jump selection in LivingEntity.aiStep: 1.8.9 legacy; 1.13.2/1.14.4 waterHeight > 0.4 rules; the restructured 1.16.4, 1.18.1, 1.21.1, 1.21.4 and 26.2 isInShallowFluid forms all match the engine's fluid_jump predicate; jumpInLiquid +0.04 for both fluids; noJumpDelay 10/0 handling; flying players excluded from 1.16 (isAffectedByFluids).; jumpFromGround/jump_power: 0.42F with a double boost before 1.14; float boost in 1.14.4-1.17; double boost in 1.18.1/1.19.4; float in 1.20-1.20.4; JUMP_STRENGTH with the 1.0E-5F gate from 1.20.5; max(power, vy) from 1.21.2; float sprint-jump form before 1.20.5 and double from 1.20.5.; Player.getSpeed reads the attribute live in every release, so no one-tick lag on the sprint modifier; movement_speed_attribute ADD_MULTIPLIED_TOTAL amounts 0.3F, 0.2F, -0.15F.; The 26.2->26.3 diff has no travel, jump or input changes in LivingEntity, Player or LocalPlayer within this scope..
  Not covered: I could not verify the exact 1.18.2 switch of the non-flying sprint airborne speed from the double form to `+= 0.006F`, because there are no 1.18.2 sources: 1.18.1 has the double form and 1.19 has the float form. The same gap leaves the 1.17.0 vs 1.17.1 jump-boost double switch inferred: 1.17 decompiled is float and 1.18.1 is double. 1.9.4-1.12.2 and 1.15 were checked only through the Yarn decompiles: moveFlying, the 0.16277136F constant, the 0.003 thresholds, the flying undo and 1.15 jump. 1.16.0-1.16.3, 1.16.5 and 1.17.x travel were bracketed rather than read line by line. I did not re-audit the attribute modifier iteration order (D-066), the jump block-factor position (block_below_affecting_movement), the ladder/powder-snow climbable checks, the fall-distance interactions or the fluid travel branches; those belong to other areas or are already documented. I took the adapter's sneaking-speed and swift-sneak computation as given, apart from the 1.14 spectator case. The engine's unconditional fall_distance = 0 on a ground jump has no vanilla counterpart in jumpFromGround, but it cannot be observed: the fall distance is already 0 whenever that branch is reachable (on ground, or in water where updateFluidInteraction resets it), so I did not report it. The client unloaded-chunk -0.1 sink being limited to releases before 1.14 is already refuted as X-1.
- **Sprint, pose, creative/spectator flight, auto-jump.** Matched vanilla: update_auto_jump structure vs LocalPlayer.updateAutoJump: canAutoJump terms (sneak = isShiftKeyDown/isSneaking, passenger, onGround, isMoving, jump factor >= 1), the 0.001 fallback to input*speed with yaw*0.017453292F, the 7-tick look-ahead max(speed*7, 1/invLen), the AABB and inflate(width,0,width), the 0.5099999904632568 lift, cross(0,1,0)*width*0.5F, the 1.4E-45F sentinel, the above-block loop with ceiling.above() for t>1, the rise test (!(<=0.5) && !(>jump)): 1.10.2, 1.12.2, 1.14.4, 1.16.4, 1.18.1, 1.19.4, 1.20.4, 1.21.1, 1.21.4, 1.21.5, 1.21.11, 26.3; auto_jump_inverse_length gates: fastInvSqrt(double) <=1.14.4, fastInvSqrt(float) 1.15-1.19.3, Mth.invSqrt (joml 1/sqrt) from 1.19.4 (1.19, 1.19.3 and 1.19.4 sources checked); feet_y gate (box minY before 1.15, position() from 1.15) and getY() in the rise test; forward_direction vs Vec3.directionFromRotation / getVectorForRotation (cos(-y*c - PI), -cos(-x*c)) with floatRadians yaw and pitch; entity-then-block obstacle order from 1.18, blocks-then-entities before (1.14.4 getCollisions Stream.concat); getSpeed() for auto-jump and travel = Player.getSpeed() attribute value (Player overrides it in 1.8.9, 1.12.2, 1.14.4-1.21.11), so the current-tick sprint modifier is right; flying vertical impulse: two double steps before 1.14, netted float (dir*flySpeed*3F) from 1.14, read from the post-tick jump (incl. makeJump) and shift (1.12.2, 1.13.2, 1.14.4, 1.15, 1.21.11); Player.travel flight: vy saved after the swim steering, y*0.6 after travel, fall reset and glide clear 1.9-1.21.1, fall reset in Player.aiStep from 1.21.2; getFlyingSpeed (abilities*2 when sprinting); getBlockSpeedFactor 1.0 while flying or fall flying (1.15-1.21.11); LocalPlayer.aiStep flight toggle: spectator/isAlwaysFlying forces flying, !wasJumping && jump && !autoJump, 7-tick jumpTriggerTime, !isSwimming from 1.13, jumpFromGround on ground from 1.20.5 (absent in 1.20.4), decrement in Player.aiStep after the toggle, landing clears flying unless spectator; elytra start: !flying in the aiStep (1.15-1.21.1) and through Player.canGlide (1.21.2-26.3), climbable from 1.15.2, levitation, water (lava in 26.3); sprint start/stop: 1.12.2, 1.13.2, 1.14.4, 1.15.2, 1.16.4, 1.19.4, 1.21.1, 1.21.4 (shouldStopSprinting), 1.21.5, 1.21.11, 26.3; trigger reset on previous sneak from 1.16 (absent in 1.15.2), on back key from 1.21.5; isSprintingPossible/isMobilityRestricted = blindness; crouching formula (!flying && !swimming && fits CROUCHING && (prev shift || !fits STANDING)) 1.14.4-26.3, and Player.isSwimming = !flying && !spectator && swimming; updatePlayerPose and getDesiredPose order (glide first <=1.21.4, sleeping/swimming first from 1.21.5), spectator/passenger bypass, CROUCHING then SWIMMING fallback (1.14.4, 1.16.4, 1.21.4, 1.21.5, 1.21.11).
  Not covered: Not compared: - The order of the boxes inside one multi-box VoxelShape (toAabbs) against the engine's stored order, for the auto-jump first match. This needs per-block shape data. - The world-border shape in the auto-jump obstacle list (1.14+). The engine notes it is not added (D-043). - The 1.10-1.12 border "stone" cells and entity getCollisionBox(Entity) boxes in doesBoxCollide. - Passenger and vehicle terms (vehicleCanSprint, passenger onGround, jumpableVehicle in the 1.21.11 flight toggle, !isPassenger in crouching). The player tick does not model riding. - isSlowDueToUsingItem's USE_EFFECTS.canSprint: the adapter has no version gate, but the value comes from the caller. - canPlayerFitWithinBlocksAndEntitiesWhen checking entities (boats, shulkers) for pose and crouch. Known area, covered by D-0xx collider items. - Vanilla 1.8.9 and 1.9.4-1.11.2 sprint paths: only spot-checked through the 1.12.2 and 1.10.2 decompiles. - 1.15.1, 1.17.x and 1.18.x LocalPlayer: inferred from neighbouring releases and the physics-methods.json snapshots (1.15.2, 1.16, 1.17, 1.17.1) only.
- **Elytra fall flying and fireworks.** Matched vanilla: Glide math 1.9.4/1.10.2/1.11.2/1.12.2 (decompiled LivingEntity.travel): -0.08 + n*0.06, dive/climb/redirect terms, 0.99F/0.98F/0.99F drag, fallDistance=1.0 when vy>-0.5, float Mth.cos/sin of pitch*0.017453292F (lift n*(n*min) equals (n*n)*1.0); Glide 1.13.2/1.14.4/1.16.4/1.18.1: gravity*(-1+lift*0.75) with lift=(float)(cos*cos*min(1,len/0.4)), Mth.cos float table; Slow Falling 0.01 when vy<=0; NoGravity ignored before 1.20.5; Glide 1.18.2 (physics-methods.json) through 1.21.1: Math.cos(double) lift, checkSlowFallDistance from 1.19.3 vs fd=1.0 before; engine gate at 1.18.2/1.19.3 correct; 1.21.2-1.21.4 updateFallFlyingMovement (Mth.square(Math.cos), no look-length factor, getEffectiveGravity incl. getGravity NoGravity=0) and the aiStep updateFallFlying -> checkSlowFallDistance/checkFallDistanceAccumulation before travel in any fluid; 1.21.5-26.3 travelFallFlying onClimbable -> travelInAir + stopFallFlying (engine glide_stopped), 1.21.11 #can_glide_through exemption, Mth.sin(double) from 1.21.11 (mth_sin gate); 26.1 and 26.3 updateFallFlyingMovement identical to 1.21.11; FireworkRocketEntity attached boost formula 1.11.2/1.12.2/1.14.4/1.16.4/1.20.1/1.21.1/1.21.4/1.21.11 (v + look*0.1 + (look*1.5 - v)*0.5, only while isFallFlying, 1.11+ gate); rockets before/after player order and post-tick flag check; getLookAngle = calculateViewVector(xRot, yRot) 1.13+ (float trig, Mth.cos(double) from 1.21.11) and 1.12.2 getVectorForRotation vs TS viewVector; checkAutoSpinAttack 1.13.2/1.14.4/1.20.1/1.21.11/26.3: minmax(before, after) box, first LivingEntity -> scale(-0.2) and end, else horizontalCollision ends; decrement-then-check order in TS; LocalPlayer elytra start 1.14.4 (server-only, no local flag), 1.15 (no climbable, no levitation), 1.15.2/1.16+ (climbable, levitation), 1.21.2+ canGlide (incl. Player.canGlide !flying), 26.2 !isInWater, 26.3 !isInLiquid (water||lava); Pre-1.21.2 Player.travel flying clears flag 7 and fallDistance; 1.21.2+ only server (updateFallFlying is !isClientSide) — engine and adapter gates match; Pre-1.21.2 LivingEntity.updateFallFlying/onGround flag clear are server-only (no client-side stop).
  Not covered: I did not check trig::cos against HotSpot's Math.cos intrinsic, which may differ from fdlibm by 1 ulp in 1.18.2+. I did not check vector_length or the per-release sine-table construction beyond the index math. The entity set that the caller (mineflayer plugin) puts into nearbyEntities is unknown: vanilla getEntities includes items, XP orbs, arrows and so on, and leaving any of them out would change the empty-list wall-stop test in checkAutoSpinAttack. The vehicle and passenger terms of tryToStartFallFlying (the !isPassenger term from 1.15) are not modelled, because the player.ts tick has no vehicle. Server-side behaviour is out of scope: elytra durability, fly-into-wall damage, the ground stop. No source exists for 1.15.x, 1.17.x or 1.18.2 beyond the decompiles and physics-methods.json, so 1.17.1 was bracketed by 1.17 and 1.18. FireworkRocketItem.use (attaching only while gliding) and the rocket's lifetime and despawn bookkeeping were not traced beyond the adapter's duration counter.
- **Attributes and status effects.** Matched vanilla: AttributeInstance.calculateValue fold order (base+ADD, +base*MULTIPLY_BASE, *=1+MULTIPLY_TOTAL) in base.ts attributeValue matches 1.14.4 ModifiableAttributeInstance, 1.16.4, 1.20.4, 1.20.5, 1.21.1, 1.21.11 and 26.3; RangedAttribute.sanitizeValue: NaN passes through Mth.clamp before 1.19 and maps to the minimum from 1.19 (1.18.1 vs 1.19); ranges for movement_speed 0..1024 (1.8.9-26.3), gravity -1..1, step 0..10, jump 0..32, water/movement efficiency 0..1; Modifier iteration order: HashSet copy before 1.16, HashSet by UUID 1.16-1.20.4, fastutil Object2ObjectOpenHashMap by UUID in 1.20.5 and by Identifier from 1.21 (hashCode = id.hashCode, and 26.3 Identifier.hashCode = 31*ns+path). Fallback movement_speed_attribute orders [slowness,sprint,speed] / [sprint,slowness,speed] / [sprint,speed,slowness] match the computed bucket orders; Speed/Slowness amounts 0.20000000298023224 / -0.15000000596046448 MULTIPLY_TOTAL*(amp+1) in 1.16.4, 1.20.4, 1.20.5, 1.21.1, 1.21.11; sprint modifier 0.3F op 2; Effect amplifier decoding: signed byte with no clamp before 1.20.5, varint clamped 0..255 from 1.20.5 (1.8.9/1.12.2/1.16.5 packets, 1.20.4/1.20.5 MobEffectInstance); effect ids become 0-based at 1.20.2 in both vanilla and minecraft-data; Jump boost in jump power: double add before 1.14 (1.8.9, 1.13.2), float 1.14-1.17.0 (1.16.5, 1.17), double getJumpBoostPower 1.17.1-1.19.4 (1.18.1, 1.19.4), float from 1.20; 1.20.5+ (float)JUMP_STRENGTH*factor+boost with the 1.0E-5F gate; 1.21.2+ max(power, vel.y); Auto-jump height 1.2F + (amp+1)*0.75F (1.14.4, 1.20.5, 1.21.11, 26.3 LocalPlayer); Levitation: y += (0.05*(amp+1) - y)*0.2 in the air only (1.12.2, 1.13.2, 1.19.4, 1.21.1, 1.21.11 travelInAir, 26.3); fall reset in travel 1.13-1.19.4, none 1.9-1.12, aiStep reset with slow falling from 1.20; elytra start blocked from 1.16; Slow falling: 0.01 (min with gravity from 1.20.5) when y<=0 at travel start; fall reset in travel before 1.20; 1.21.2+ getEffectiveGravity after the move in travelInAir; fluid paths use the travel-start test (1.21.11 travelInFluid); bounce uses getEffectiveGravity on currentMovement (26.3 Entity.restituteMovementAfterCollisions); Water travel: depth strider clamp 3, *0.5 off ground, /3F before 1.21 and the WATER_MOVEMENT_EFFICIENCY attribute without /3 from 1.21.0 (1.20.6 vs 1.21); dolphins grace 0.96F applied after the lerp (1.13.2-26.3); input speed lerp toward getSpeed(); MOVEMENT_EFFICIENCY Mth.lerp(eff, blockSpeedFactor, 1.0F) from 1.21 (1.21.11 LivingEntity.java:472; same Mth.lerp in 26.3); Swift Sneak clamp(0.3F + level*0.15F, 0, 1) 1.19-1.20.6, attribute from 1.21; 26.2->26.3 diff: no changes to effect/attribute physics.
  Not covered: I did not run the adapter against a live 1.21+ server. The attribute-key mismatch comes from reading mineflayer 4.37.1's updateAttributes and the installed minecraft-data 3.117.0 protocol mapper strings (checked with a script over the registered releases). 1.21.2, 26.2 and 26.3 have no minecraft-data attributes/protocol entry installed, so I could not check their keys. I did not check whether prismarine-item exposes `nbt` for 1.20.5+ items, which physics-util's enchantment read depends on. I also skipped: Player.getFlyingSpeed (sprint flying), fall-damage rounding beyond the jump-boost term, the vehicle/mob effect paths in vehicles.ts, server-side-only effect handling (updateFallFlying only clears the flag on the server), and the fastutil rehash collision order for more than 24 modifiers on one operation.
- **Collision and stepping.** Matched vanilla: Entity.collide main flow (lengthSqr()==0 shortcut, landing/onGround step gate `maxUpStep > 0 && (onGround || y-collided && y<0) && (x||z collided)`), 1.14.4, 1.16.4, 1.17 (decompiled), 1.18.1, 1.20.6, 1.21, 1.21.11, 26.1, 26.2, 26.3 vs collide_shapes; Lifted step-up (stepped / lift on box expanded horizontally / `lift.y < step` / horizontalDistanceSqr compares / lowering by -stepped.y + movement.y), 1.14.4-1.20.6 vs lifted_step_up; 1.21+ candidate step heights: grounded box, stepUp query with -9.999999747378752E-6 down when not landing, float candidates, skip `(float)collided.y`, break on > maxStep, ascending sort, first candidate that goes further wins, subtract distanceToGround (1.21, 1.21.2, 1.21.11, 26.1-26.3) vs candidate_step_up/candidate_step_heights incl. entity Shapes.create grid coords; collideWithShapes / collideBoundingBoxLegacy axis order (Y, then Z first when |x|<|z|, box moved only after non-zero axes; Direction.axisStepOrder in 1.21.5+) vs collide_axes; Shapes.collide 1e-7 check between whole shapes (1.13.2 calculateMaxOffset, 1.21.11, 26.3) vs shapes_collide; VoxelShape.collideX grid walk (1.13.2 method_18069, 1.21.11): cross-section index ranges, first full layer, gap tolerances vs collide_over_grid / clip_box (box-by-box equals grid when not entered); Collider order entities -> border -> blocks in Cursor3D order (Entity.collectColliders 1.18+; RewindableStream entities+border then getBlockCollisions in 1.14-1.17); 1.14.4-1.17.1 collideBoundingBoxHeuristically single-axis walk (Shapes.collide(AABB, LevelReader,...) 1.14.4; VoxelShapes.calculatePushVelocity 1.16.5): 1e-6 thin box, 1e-7 drop, AxisCycle mapping, lastC, face/edge cell rules, block-local coords, others with no check between; gone in 1.18 (decompiled 1.18 Entity); 1.14-1.17 border in collide stream via !joinIsNotEmpty(border, box.deflate(1e-7)); getBlockCollisions border offer (1.14.4 LevelReader); 1.18+ WorldBorder.isInsideCloseToBorder (absMax margin >=1, distance < 2*margin, isWithinBounds `>` until 1.20.6, `>=` from 1.21) and getCollisionShape floor/ceil edges, 1.18.1/1.20.6/1.21/1.21.11/26.3; Pre-1.13 Entity.move AABB path (1.8.9, 1.9.4, 1.10.2, 1.11.2, 1.12.2): Y,X,Z offsets, calculateX/Y/ZOffset strict tests, step-up swept vs lifted, `w > x` pick, lowering, revert when `s*s+u*u >= ...`, flags exact !=; 1.13.2 Entity.move VoxelShape path and getCollisionBoxes (RenderBlockView method_16378/16367/16369): entered-block exclusion, full cubes joined after blocks, entities last, border stone cells and isOutsideBorder update; updates_outside_border vs 1.8.9/1.12.2 World.getCollisionBoxes column ranges, skipped corners, loaded-chunk gate; is_inside_legacy_world_border ±1 hysteresis; Entity.move flags: horizontalCollision exact before 1.14, Mth.equal (1.0E-5F) from 1.14; verticalCollision/verticalCollisionBelow/onGround; 1.21.4 `|y|>0 || local` refresh gate; 1.21.9 noPhysics flag reset; LocalPlayer.isHorizontalCollisionMinor 1.18.1, 1.21.1, 1.21.2, 1.21.5, 1.21.11, 26.1, 26.3 (float yaw radians, xxa/zza, 9.999999747378752E-6 thresholds, acos < 0.13962633907794952); restituteMovementAfterCollisions 26.2 (`<`) and 26.3 (`<=`), Mth.lerp drag, gravity compensation, gate `movedVertically && verticalCollision || horizontalCollision`; entity_collision_query: getEntityCollisions inflate 1e-7 from 1.14 (1.14.4, 1.16.4, 1.18.1, 1.21.11), 26.3 expandTowards(0, maxUpStep, 0); moves_position: `> 1e-7` until 1.21.1, `|| delta.lengthSqr() - len < 1e-7` from 1.21.2.
  Not covered: I did not audit the block shape sources that feed these functions: world.rs collect_collision_shapes cell ranges and the large-shape face rule, generated shape data, and whether vanilla shapes that are not optimized carry extra grid coordinates beyond the union of their boxes' grids. Those coordinates would change VoxelShape.getCoords for step candidates and the grid walk. Moving-piston edge cells are already deferred as D-099. Callers outside the stated scope were not checked: piston.rs, vehicle.rs legacy_collide, and the riptide lift at simulation.rs:3365. The TypeScript adapter's world-border and outsideBorder plumbing was not checked either. For 1.9.4-1.11.2 I compared only the Entity.move collision and step structure; their World.getCollisionBoxes border loops are bracketed by 1.8.9 and 1.12.2. 1.15.x and 1.16.5 single-axis walk details are taken from the 1.14.4, 1.16.5 and 1.17 sources. The 1.18.1 Shapes.java in the git repo is empty, so 1.18 Shapes.collide was checked against decompiled 1.18 plus 1.21.11/26.3. I did not verify every flow-on effect of the moving-border lerp (D-070), the per-entity canCollideWith / only_from_above filtering, or the input vector feeding isHorizontalCollisionMinor beyond the 1.21.5 modifyInput check.
- **Voxel shapes, math, trig, block traversal.** Matched vanilla: Mth sine table and sin/cos: legacy table (double)i*PI*2.0/65536.0 and (int)(f*10430.378F)&65535 (cos +16384.0F) for 1.8.9, 1.12.2, 1.14.4, 1.18.1, 1.19.4, 1.20.6, 1.21.1, 1.21.4, 1.21.5-1.21.10; modern table i/10430.378350470453 with the (long)(d*scale)&65535 double index from 1.21.11 and in 26.1-26.3; mth_sin/mth_cos gate at 1.21.11 is exact (math.rs:290-311, simulation.rs:2605-2619); Mth.atan2 with FRAC_BIAS, ASIN_TAB/COS_TAB (Math.asin, Math.cos), fastInvSqrt and the 0.16666666666666666 correction: 1.8.9, 1.14.4, 1.19.4, 1.21.11, 26.3 (math.rs:453-486); fastInvSqrt(double) 6910469410427058090L-(l>>1) arithmetic shift; fastInvSqrt(float) 1597463007 with floatToIntBits for 1.15-1.19.3; invSqrt = JOML 1/sqrt from 1.19.4; auto-jump gate 1.15 / 1.19.4; Mth.floor/ceil (int)/(long) truncation forms, Mth.abs(float) gate at 1.14, Mth.clamp double/float legacy vs Math.min form gated at 1.19.4 (1.19.3 vs 1.19.4 decompiled), Mth.sign, Mth.frac, Mth.lerp, wrapDegrees float/double/int/long, equal() threshold (double)1.0E-5F; Vec3 xRot/yRot/zRot and directionFromRotation float/table usage (1.21.11); AABB.clip / getDirection / clipPoint order, epsilons and 0<t<ds[0] strictness (1.21.11, 26.3) vs Aabb::clip_segment and block_traversal::clip; AABB.collidedAlongVector: center via Mth.lerp(0.5), half-size inflate with 0 inset in 1.21.2-1.21.8 and -1e-7 from 1.21.9, contains half-open, clip fallback; AABB contract/expandTowards/inflate/deflate/intersect/minmax/contains/intersects/distanceToSqr semantics; BlockGetter.boxTraverseBlocks + addCollisionsAlongTravel (1.21.2-1.21.4): Mth.square(0.99999F) float threshold, normalize(1.0E-5F)*1e-7 nudge, p++ > 16 cap, clamp with (double)1.0E-5F, floor(hit+size) fill, betweenClosed X-Y-Z order; BlockGetter.forEachBlockIntersectedBetween 1.21.5-1.21.8: no nudge, step index = post-increment counter, final box at steps+1, short move index 0, boolean early exit in 1.21.6-1.21.8; BlockGetter.forEachBlockIntersectedBetween 1.21.9-26.3: 1.0E-5F short-move threshold, betweenCornersInDirection of the start box (index 0), furthest-corner walk counting only clip hits, no 16 cap, opposite corner floor(hit - size*corner), final box at iterations+1, Direction.axisStepOrder, getFurthestCorner; Entity.checkInsideBlocks driver in block_traversal: deflate 9.999999747378752E-6, 16 budget per movement with var11+1 accounting, budget<=0 fallback visit of the final box, 1.21.10 precise flag (distanceToSqr > square(0.9999900000002526) || box.intersects(pos)), visited-set semantics before/from 1.21.9; BlockGetter.traverseBlocks for 1.14+ (lerp(-1.0E-7) endpoints, DDA tie-breaking, Double.MAX_VALUE steps) and VoxelShape.clip short-ray (lengthSqr<1e-7) rule from 1.13; Shapes.or/join = joinUnoptimized(...).optimize() always, Shapes.create/findBits (-1e-7, 1.0000001, 1e-7*2^i), CubePointRange grid i/2^n + offset, IndirectMerger first-kept 1e-7 dedup (voxel_shape.rs or + collision.rs grid_lines/grid_bits); BitSetDiscreteVoxelShape.forAllBoxes: X-outer/Y-middle before 1.18 (1.14.4 git, 1.16.5/1.17 decompiled; old VoxelSet widening to lower X/Y provably finds nothing) and Y-outer/X-middle from 1.18 (1.18 decompiled, 1.18.1 git); strip/rectangle clearing order; VoxelShape.collideX over the grid (findIndex binary search, 1e-7 offsets, first full layer returns, gap >= -1e-7 / <= 1e-7) in collide_over_grid; trig.rs: Java Math.sin/cos implementation (correctly rounded double-double); only used to build tables and atan COS_TAB.
  Not covered: I could not run Java, so trig.rs is not confirmed bit-for-bit against HotSpot's Math.sin/Math.cos intrinsics. Its own comments say rare near-tie differences exist; after the narrowing to float for the sine tables they almost never matter. In voxel_shape.rs, the VoxelShape struct (union/collide/rotated_*) is called only from tests (world.rs set_block_voxel_shape has no production caller). The production collision path is BlockVoxelShape in collision.rs: I checked only the grid_lines/grid_bits/collide_over_grid helpers it shares, not its construction from generated block data or the Shapes.collide ordering. I did not compare the quarter-turn rotation helpers with OctahedralGroup rotation because nothing in production calls them. I did not check the IndexMerger variants of 1.13-1.16 (DoubleListPair/PairList) for differences from the modern IndirectMerger dedup. Many Mth helpers in math.rs (hsv, catmullrom, murmur, outFromOrigin, binarySearch, lerpDiscrete, etc.) are not used in movement and were only spot-checked. Source caveat: the git branch origin/client1.16.4 actually holds 20w45a (a 1.17 snapshot), so for 1.14-1.16.5 I used the 1.14.4 git branch and the decompiled 1.15/1.16/1.16.5 jars instead. The Entity-level callers in simulation.rs are outside this area and not audited here: who builds Movement.axis_order, the inside-effect predicates, and the air handling.
- **World representation, block data and generated shapes.** Matched vanilla: ScaffoldingBlock.getCollisionShape (1.14.4, 1.16.4, 1.18.1, 1.20.4, 1.21.4, 1.21.5, 1.21.11, 26.3) vs world.rs entity_collision_shape: isAbove(1.0)&&!descending → stored STABLE shape (minecraft-data stores SHAPE_STABLE for all states), DISTANCE!=0&&BOTTOM&&isAbove(0.0) → 0.125 slab, else empty; the 1e-5F (9.999999747378752e-6) isAbove epsilon; the 1.21.5+ isPlacement branch is irrelevant to movement; PowderSnowBlock.getCollisionShape (1.18.1, 1.20.4, 1.21.4, 1.21.5, 1.21.11, 26.3): fallDistance>2.5 → 0.8999999761581421 box first, then walkable (leather boots/tag) && isAbove(block) && !descending → full block, else empty; stored data empty; EntityCollisionContext (1.14.4 boundingBox.minY + isSneaking; 1.16+ getY + isDescending=isShiftKeyDown; 1.21.5+ placement flag; 26.2/26.3 unchanged) vs entity_collision_context bottom=position.y, descending=sneaking; CanEntityWalkOnPowderSnow unchanged 1.18-26.3 (tag or LEATHER_BOOTS); BigDripleaf tilt collision shapes (NONE/UNSTABLE 11-15, PARTIAL 11-13, FULL empty) in minecraft-data 1.17-26.1; tilt is server-driven; Bamboo and pointed dripstone XZ offset: Mth.getSeed(x,0,z), ((float)(n/15F)-0.5)*0.5 clamped to the max offset (1.14 unclamped is equivalent for bamboo), minecraft-data shapes carry the (0,0) offset of -0.25/-0.125 that blocks.ts subtracts; minecraft-data state-count alignment of blockCollisionShapes vs blocks.json for every mapped release 1.13-26.1 (only 1.16/1.16.1 chain/lantern misaligned, handled by D-071 override; override boxes verified against 1.16.4 ChainBlock/LanternBlock); Legacy stair shape (1.8.9 BlockStairs func_176306_h/func_176304_i and 1.12 getStairsShape): ahead/behind and isDifferentStairs checks, quarter-box placement for outer/inner left/right, metadata→facing (5-(meta&3)) and half bit; 1.12 stair getBlockFaceShape for fence/pane/wall joins (STRAIGHT facing==face; INNER_RIGHT facing==face||face.rotateYCCW; INNER_LEFT face.rotateY; outer none); legacy-connections.json 1.8 fence (two-box addCollisionBoxesToList), 1.8 pane (half/full arms), 1.8-1.12 wall single clip box with the 0.3125-0.6875 straight case and 1.5 height, 1.9-1.12 fence/pane post+arms; 1.8.9 collision boxes of trapdoor (0.1875), door (0.1875), snow ((layers-1)*0.125), soul sand, cactus, cake, cocoa, anvil, end portal frame, brewing stand, cauldron, hopper, carpet, bed, flower pot, skull, ender chest, dragon egg, piston base, fence gate vs minecraft-data pc/1.8; 1.9.4-1.12.2 overrides of getCollisionBox(state, World, pos) (only farmland differs from the bounding box; grass path, cactus, soul sand, snow, walls and gates agree with minecraft-data); World.containsAnyLiquid cell ranges (1.8 floor..=floor inclusive; 1.9+/1.14+ floor..ceil exclusive) vs legacy_fluid.rs is_free / world.rs contains_any_liquid; Modern fence/pane/wall/stair/bed collision boxes in minecraft-data (CrossCollisionBlock 2/2/24, IronBars 1/1/16, WallBlock makeShapes(4,3,24…)) for 1.14-26.1; has_large_collision_shape / bordering-cell scan: dynamic-shape blocks (bamboo, dripstone) never reach beyond their cell, so the cache==null 'always large' rule does not change results; 26.2 to 26.3 EntityCollisionContext and ScaffoldingBlock/PowderSnowBlock collision unchanged (E:/pr/mc-source diff).
  Not covered: - minecraft-data 3.117 in node_modules has block data only up to 26.1, so I could not inspect the shapes the adapter uses for 26.1.1, 26.1.2, 26.2 and 26.3, which come from whatever registry the consumer supplies. Their new blocks (sulfur_spike, shelf_mushroom, straw_bed, the copper chains) were not checked block by block. Neither were 1.19.1 and 1.21.2/1.21.7, which have no minecraft-data dataPaths entry. - Block-by-block checking of every modern shape was limited to diffs between adjacent minecraft-data versions plus spot checks. I did not compare every block's collision shape to the Java source for each release. - The 1.8 chest and anvil collision boxes use whatever bounds the shared Block instance was last given (no getCollisionBoundingBox override in 1.8.9), so they cannot be reproduced 1:1. I left them as they are. - LiquidBlock's canStandOnFluid / getLiquidCollisionShape (striders and other mobs, 26.3) belongs to the ridden-mob area and was not checked. - Moving piston and shulker box shapes are deferred (D-099, D-081). - Fluid queries in world.rs (fluid_contact, fluid heights, flow) and the suffocation/in-wall push were left to the fluid and player audits. I only confirmed that Material.ICE blocks motion, so the 1.14-1.19 isSuffocating default holds for ice. - The generated shape pools in native/data/releases were checked only for scaffolding, powder snow, big dripleaf, snow, lanterns, chains, carpets, fences, panes, trapdoors, doors, beds and stairs. The provenance sha256 values could not be checked because the decompiled trees the generator recorded are not on this machine. - Nothing was built or run.
- **Entity push and entity colliders.** Matched vanilla: push_impulse / Entity.push(Entity): absMax, `>= 0.01F` (0.009999999776482582), float Mth.sqrt before 1.17 and Math.sqrt from 1.17 (1.16.5 vs 1.17 decompiled), 1/d clamp to 1, x0.05F, order of ops: 1.8.9, 1.9.4, 1.16.4, 1.17, 1.18.1, 1.19, 1.21.11, 26.3; entity_push_impulses_with_pushthrough: (1.0F - this.pushthrough) as a float widened to double, only for releases before 1.17 (entity_pushthrough_enabled), applied to `this`; isVehicle/hasPassengers on each side; the isPushable gate from 1.19 exactly (absent in 1.18.1, present in 1.19); isPassengerOfSameVehicle from 1.9, rider/vehicle in 1.8.9; push_by_nearby_entities pushable gate: no client-side cramming before 1.9 (1.8.9 LivingEntity.tickMovement `if (!this.world.isClient) tickCramming()`; 1.8.9 boats and minecarts push only boats and minecarts); from 1.9 pushableBy(pusher).test(localPlayer) (1.9.4 EntityPredicate.method_13025, 1.14.4/1.20.6/26.3 EntitySelector.pushableBy, 1.21.5+ ClientLevel.getPushableEntities); isPushable is `!removed` in 1.9.4-1.10.2 and isAlive && !isClimbing from 1.11.2, with !isSpectator added in 1.19; Living pusher: M.doPush(P) -> P.push(M), the player gets -(M-P) normalized; the sleeping skip (1.9.4 PlayerEntity.pushAwayFrom, 1.14.4/26.3 LivingEntity.push); the player box meets the pusher's post-tick box (getBoundingBox after travel); RemotePlayer/OtherClientPlayer push too (1.9.4, 1.14.4-26.3); Boat pusher: inflate(0.2F,-0.01F,0.2F) symmetric (1.9.4 Box.expand grows both sides; 1.21.11/26.3 0.20000000298023224, -0.009999999776482582), the Boat.push minY<=minY condition, B.push(P) through Entity.push (no sleeping check), the player gets +(P-B): 1.9.4, 1.14.4, 1.21.11, 26.3; Minecarts never push the local player on the client (the push sits in the server branch: 1.12.2 AbstractMinecartEntity:314/374, 1.14.4 AbstractMinecart.tick, 1.21.11 Old/NewMinecartBehavior are ServerLevel-only) and are never colliders (getBox null in 1.8.9-1.13.2; no canBeCollidedWith override 1.14-26.3); Collider set: Boat (getBox in 1.8.9-1.13.2, getCollisionBox 1.14-1.15, isCollidable 1.16, canBeCollidedWith 1.17-26.3), Shulker (isAlive), HappyGhast (client: player position().y >= maxY, or isOnStillTimeout; not baby): 1.8.9, 1.9.4, 1.12.2, 1.13.2, 1.14.4, 1.16.5, 1.18.1, 1.19, 1.20.6, 1.21.4, 1.21.5, 1.21.11, 26.3; Level.getEntityCollisions query: swept.inflate(1e-7) with strict AABB.intersects from 1.14 (1.14.4 EntityGetter:54-66, 1.16.5 EntityView:59-70, 1.19/1.21.11/26.3 EntityGetter); 26.3 adds the expandTowards(0,maxUpStep,0); before 26.3 the step-up reuses the first entity list (1.14.4/1.20.6 Entity.collide), with no requery; pre-1.14 the superset query (the block query) gives the same clip results, since boxes outside the swept box cannot shorten a sequential axis clip; player getHardCollisionBox/getCollideAgainstBox is null (Entity default, no Player/LivingEntity override 1.8.9-1.15), so only the other entity's own box counts; entity_collision_context: descending = isShiftKeyDown (Entity.isDescending 1.16.4-26.3; 1.14.4 isSneaking), bottom = getY() (1.14.4 uses getBoundingBox().minY, which equals y there); the HappyGhast only_from_above test uses the same position().y; check_auto_spin_attack: the -0.2 scale; a non-empty list with no LivingEntity does not end the spin even on a wall; the ArmorStand counts as living (1.14.4, 1.19, 1.20.6, 1.21.4, 1.21.11, 26.3); Adapter kinds: bat/parrot/ender_dragon/vex never push (Bat.pushEntities empty, Parrot.doPush skips players, EnderDragon noPhysics, Vex noPhysics during tick); Warden, IronGolem and SulfurCube doPush call super; Creaking and AbstractHorse isPushable only affect themselves being pushed.
  Not covered: I could not confirm the exact release where the 1.19 isPushable gate in Entity.push arrived: there is no 1.18.2 source (only 1.18 decompiled and git 1.18.1 lack it, git 1.19 has it). 1.9.0-1.9.3, 1.10.x and 1.11.0/1.11.1 are bracketed only by 1.9.4/1.10.2/1.11.2. I did not re-audit tick order (ticks_before_player, the client entity-list order and rocket interleaving) or the multi-entity simulate_versioned_entities loop beyond its push-impulse call. That loop does not model the receiver's sleeping check, the Boat/Shulker push overrides or entity cramming, and it does not move the local player. Pre-1.14 collide_legacy block/entity merging (D-076), pose and box sizes of pushers (D-093) and shulker peek boxes (D-092) were also out of scope for re-verification. 26.1, 26.1.1 and 26.2 were spot-checked only through the 26.3 sources and the 26.2_to_26.3 diff; I did not open them individually.
- **Pistons.** Matched vanilla: moveCollidedEntities 1.11-26.3: distance (double)(progress+0.5F - progress) in float, moveByPositionAndProgress (double)getExtendedProgress * step + pos, per-shape getMovementArea/getMovement with early break at >= deltaProgress, `min(delta, d) + 0.01`, the no-push tick once progressO >= 1 (1.11.2, 1.12.2, 1.13.2, 1.14.4, 1.15, 1.16, 1.16.4, 1.21.1, 1.21.11, 26.3); Slime bounce area: bounds min'd/max'd with the unit cube before 1.16 (getMinMaxPiecesAABB / method_13753), plain shape.bounds() from 1.16; per-axis setDeltaMovement to the movement step; LocalPlayer included (ServerPlayer exclusion only, 26.3 canSimulateMovement); fixEntityWithinPistonBase 1.11-26.3: unit cell at pos, opposite direction, +0.01 twice, |var8-var10| < 0.01, min(var8, d)+0.01, only after a non-zero push of a retracting source; PistonMath.getMovementArea and getMovement switch per direction (1.21.11, 1.12.2); limitPistonMovement / applyPistonMovementRestriction: first non-zero axis, clamp(±0.51), 9.999999747378752E-6 cutoff, lengthSqr<=1e-7 bypass only from 1.14 (1.11.2, 1.12.2, 1.13.2, 1.14.4, 1.21.11, 26.3); Stuck multiplier after the limit; PISTON exempt from the multiply from 1.21.10 but still cleared (1.21.11, 26.3); No sneak edge back-off for PISTON moves 1.11+, back-off for the plain move() of 1.8-1.10; Pre-26.2 motion stop: exact compare <1.14, the 1.14-1.18.1 x-restore quirk (1.14.4 Entity.java:519-526), Mth.equal from 1.18.2; landing y*0 / 0 / bed 0.66F; getBlockSpeedFactor from 1.15; 26.2/26.3 restituteMovementAfterCollisions gate `(movedVertically && verticalCollision) || horizontalCollision`; moveStuckEntities 1.15-26.3: honey only, horizontal only, box top = shape max Y, ceiling 1.5000000999999998 up to 1.19.4 and 1.5000010000000001 from 1.20, onGround + position-inside test, isSupportedBy from 1.20, called after moveCollidedEntities; 1.21.2-1.21.4 effects path oldPosition()->position vs 1.21.5+ previousPos->position (when the move is non-zero); 1.9.4/1.10.2 TileEntityPiston: union of the box at lastProgress and at progress, float offset*step then pos, signed per-axis +0.01, slime bounce then move, extra push on the ended tick, plain move(); 1.8.9 TileEntityPiston: extending-only push of (progress-lastProgress)+0.0625F along the facing, 0.25F on the ended tick, extending slime bounce instead of the move, float distance*step widened; LocalPlayer.move -> updateAutoJump((float)dx, (float)dz) after a non-zero piston move (1.13.2, 1.14.4, 1.21.11).
  Not covered: I did not re-verify the shared simulation.rs helpers that piston.rs calls (collide_with_world, check_fall_damage, apply_block_effects/stepOn gating, blocks_inside_moves, update_in_lava_after_move, kept_supporting_block); they belong to the main-tick audit. I did not check which shapes, source/extending flags or the SHORT head the adapter feeds (src/mineflayer, mineflayer/lib/movingPistons.js); the D-099/D-100/D-101 items cover them. I did not check NOCLIP collision with other moving-piston blocks, which depends on D-099. I did not check the pistonDeltas game-time reset against the client's level gameTime (the engine resets once per player step, napi_player.rs:271). For 1.15.x, 1.16-1.16.1, 1.17.x and 1.18.x I relied on the decompiled 1.15, 1.16, 1.18 and 1.19 jars and on git 1.16.4/1.18.1/1.19.4/1.20; 1.9.4/1.10.2 head and base getCollisionBox(BlockView) shapes were taken from D-107's prior verification and not re-read.
- **Boats.** Matched vanilla: floatBoat (1.9.4, 1.12.2, 1.13.2, 1.14.4, 1.16.4, 1.18.1, 1.20.4, 1.20.5, 1.21.1, 1.21.2-26.3): constants 0.05/0.9/0.45/landFriction, float -0.04F gravity before 1.20.5 and double getGravity() from 1.20.5, NoGravity gate, -7.0E-4 for UNDER_FLOWING, 0.01F buoyancy, lift 0.06153846016296973 or 1.9.4 -d/0.65 or getDefaultGravity()/0.65, *0.75, deltaRotation*=invFriction in float, landFriction /= 2 for a player; floatBoat entering-water snap: waterLevel = minY/getY + bbHeight, float (level - bbHeight) + 0.101, motionY = 0 before 1.14 and multiply(1,0,1) (-0.0) from 1.14, lastYd = 0, noCollision gate starting at 1.21.0 (absent in 1.20.6, present in 1.21), status forced to IN_WATER; controlBoat (1.9.4-26.3): --/++deltaRotation in float, 0.005F turn-only acceleration, yaw += deltaRotation, +0.04F / -0.005F, Mth.sin(-yRot*0.017453292F)*acc and Mth.cos(yRot*0.017453292F)*acc in float; getStatus / isUnderwater / checkInWater ordering and loop bounds (1.9.4, 1.12.2, 1.14.4+, 26.3), flowing-first early return, waterLevel = maxY when under water; getWaterLevelAbove for 1.9.4-1.12.2 (relative legacy getBlockLiquidHeight) and 1.14.4-26.3; legacy water height (1.9.4/1.12.2 method_11326: level&7==0 with water above -> 1.0, else 1 - (level>=8?0:level)+1)/9); getGroundFriction (1.12.2, 1.13.2, 1.21.11, 26.3): slice minY-0.001, ±1 cell bounds, edges==2 skip, edge-layer skip, lily pad exclusion (instanceof from 1.13, no boat box before), float sum / count giving NaN; empty-context scaffolding isAbove returns canAscend -> stable shape in all versions; Boat.tick order (1.9-26.3): oldStatus/status before super.tick fluid push, then floatBoat, controlBoat, move; second checkInsideBlocks after the move 1.9-1.21.1; two applyEffectsFromBlocks from 1.21.2 (1.21.2-1.21.4 each [old->pos]; 1.21.5+ recorded then [old->pos]); AbstractBoat 1.21.2->26.3 diffs (only bubble column, interpolation, positionRider/clampRotation refactors, getAirDrag 1.0F); 1.8.9 EntityBoat.onUpdate client branch for the local rider (isBoatEmpty false from NetHandlerPlayClient attach): 5-slice isAABBInMaterial, 0.04F*(2f-1) / 0.007F, rider yaw+strafe*90 float trig, speedMultiplier 0.07-0.35 /35, 0.35 cap, snow/lily corner clearing, onGround *0.5 on all axes, 0.99F/0.95F decay skipped on fast horizontal collision, atan2 yaw turn clamped to ±20 and setRotation %360; Passenger position: 1.8.9 updateRiderPosition (cos/sin*0.4, -0.3 + yOffset); 1.9-1.20.1 Vec3(f,0,0).yRot(-yRot*0.017453292F-1.5707964F) with float (mounted + passenger) height and animal +0.2 double/float switch at 1.19; 1.20.2 JOML Vector3f.rotateY plus ridingOffset; 1.20.5+ Vec3(0, rideHeight, offset).yRot minus vehicle attachment; Passenger yaw clamp: += deltaRotation, wrap, clamp ±105, pre-1.17 yaw+(c-w) vs 1.17+ setYRot(yaw+c-w); 26.3 authoritative gate and calculatePassengerBodyYRot are equivalent for a lone player; lava_flow_scale 0.007 / 0.0023333333333333335 (1.16+); boat fluid push excludes nothing for the boat itself (getVehicle() instanceof Boat applies to riders only).
  Not covered: Not checked: the generic parts of move_vehicle (collide_shapes, the 26.2 restitution, fall_on, block speed factor, inside-effect traversal internals) and the generic fluid push (water_contact / fluid_contact, 26.x EntityFluidInteraction), because other audit areas cover them. Entity-entity collisions in the boat move and in the 1.21+ noCollision snap check are known and deferred (D-110). The bubble-column server logic is D-118. The world-less tick_boat / simulateBoatTick is a toy path (D-122). 1.10.2, 1.11.2, 1.15.x and 1.17-1.19.x boats were only spot-checked (waterLevel init in 1.15/1.16.5/1.17 decompiled). The floatBoat/getStatus code looked unchanged across the bracketing releases (1.9.4, 1.12.2, 1.14.4, 1.16.4, 1.18.1, 1.20.4). Exact JOML version and fastmath settings for 1.20.2-1.20.4 were not checked against the bundled jar. The 1.13.2 own-height finding does not check whether the adapter sends fluid_level for 1.13 in a way that would make a separate own-height helper exact.
- **Horses and ridden mobs.** Matched vanilla: AbstractHorse rider input (getRiddenInput / pre-1.19.4 travel): xxa*0.5F, zza and *0.25F when <= 0, input zeroed while rearing on the ground with no pending jump and !allowStandSliding, input computed before tickRidden. Checked 1.12.2, 1.14.4, 1.16.4, 1.18.1, 1.19.3, 1.19.4, 1.21.5, 1.21.11, 26.3; Horse riding jump: no block factor in 1.14.4, getBlockJumpFactor from 1.15 (decompiled 1.15), double JUMP_STRENGTH*scale*factor + float Jump Boost until 1.20.4, float getJumpPower(scale) from 1.20.5, forward 0.4F*sin/cos*scale only when forward > 0; pre-1.19.4 !isJumping gate and on-ground reset after travel; 1.19.4-1.21.5 setIsJumping(false) before the check; from 1.21.6 isJumping() is LivingEntity.jumping (equivalent); Rearing: stand()/standIfPossible gates (isControlledByLocalInstance||isEffectiveAi before 1.19.4, isEffectiveAi 1.19.4-1.21.5, (isEffectiveAi||!client) with setStanding(20) and a countdown from 1.21.6), the counter after super.tick, allowStandSliding cleared when not standing; onPlayerJump clamp and getPlayerJumpPendingScale 0.4F+0.4F*p/90F; 1.12.2, 1.14.4, 1.18.1, 1.19.3, 1.19.4, 1.21.4, 1.21.5, 1.21.6, 1.21.11; Air speed: flyingSpeed = getSpeed()*0.1F before setSpeed pre-1.19.4 (horse, pig, strider using getMoveSpeed), 0.02F when unsteered; getFlyingSpeed() = getSpeed()*0.1F with a player controller from 1.19.4 (1.19.4, 1.21.5, 1.21.11, 26.3); Pig/strider steering: pre-1.19.4 getSteeringSpeed (pig 0.225F; strider 0.55F/0.23F) plus speed*1.15F*sin, boostTime++ > total placement; 1.19.4+ attr*0.225*boostFactor (pig) and attr(with -0.34 ADD_MULTIPLIED_BASE modifier)*0.35F/0.55F*boostFactor (strider); tickRidden rotation and tickBoost before getRiddenSpeed (1.12.2, 1.14.4, 1.16.4, 1.18.1, 1.19.3, 1.19.4, 1.20.4, 1.21.4, 1.21.11); Strider warmth: blockPosition, getBlockStateOn(0.2) / getBlockStateOnLegacy and the getFluidHeight(LAVA) of the previous tick, computed before super.tick; !isNoAi gate from 1.19 (1.16.4, 1.18.1, 1.19, 1.19.3, 1.21.11); Strider floatStrider and checkInsideBlocks in checkFallDamage and tick for 1.16-1.21.1, gone in 1.21.2 (1.21.1-1.21.4 counts); Step heights: steered pig/strider 1.0, unsteered 0.5 pre-1.19, AbstractHorse 1.0, camel 1.5, maxUpStep max(step,1.0) with a player controller (1.14.4, 1.18.1, 1.19.3, 26.3); Camel 1.19.4-26.3: getRiddenRotation/getRiddenInput refusal, tickRidden standUp on zza > 0 after super.tickRidden, getRiddenSpeed sprint bonus when cooldown == 0, onPlayerJump gates, executeRidersJump (double before 1.20.5, float getJumpPower from 1.20.5, 22.2222F/1.4285F, 55 cooldown), Camel.travel refusal zeroing before the fluid branch; LivingEntity.travelRidden order (getRiddenInput, tickRidden, setSpeed, travel) and NoAI handling: canSimulateMovement includes !isNoAi only in 1.21.5 (1.19.4, 1.21.1, 1.21.4, 1.21.5, 1.21.11, 26.2, 26.3 Mob); aiStep: 0.98 fade when !isEffectiveAi before 1.21.5, no fade from 1.21.5 (canSimulateMovement), per-axis 0.003 negligible for non-players (1.14.4-1.21.11); travelInWater/travelInLava for ridden mobs: 0.02F input, slow-down 0.8F (skeleton horse 0.96F from 1.13), 0.2 ladder lift before the multiply from 1.14, getFluidFallingAdjustedMovement, lava 0.5/0.8 under the jump threshold from 1.16, -g/4, jumpOutOfFluid 0.3 (1.12.2 decompiled, 1.13.2, 1.21.11, 26.3); floatInWaterWhileRidden: CAN_FLOAT_WHILE_RIDDEN = horse, zombie_horse, mule, donkey, camel, camel_husk; water only in 1.21.11-26.2; 26.3 floatInLiquidWhileRidden(ENTITY_FLOATABLE = water) after both water and lava travel; 26.3 travelInAir FRICTION_MODIFIER/AIR_DRAG_MODIFIER: computeModifiedFriction is exact identity at the default modifier 1.0, so friction*0.91F and 0.98F are unchanged; HappyGhast riding (26.3): getRiddenInput look projection, +0.5 up on jump, *3.9*FLYING_SPEED, 0.08F turn, travelFlying speed FLYING_SPEED*5F/3F with 0.8/0.5/0.91 drag; AbstractNautilus riding (1.21.11, 26.3): look input, 0.5F turn, getRiddenSpeed 0.0325F/0.02F, travelInWater override (getSpeed, scale 0.9, no jump-out), dash (1.2F/0.5F)*scale*speed*blockSpeedFactor along the rider's look with cooldown 40, not pushed by fluid; vehicle.rs tick_horse (world-less, default release 26.2): matches its documented approximation (0.003 negligible, AbstractHorse input, float getJumpPower(1.0), onGround read before the jump clears it, 0.6 ground friction, getSpeed()*0.1F air, 0.02F/0.8F water with fluid-falling adjust and the 0.04F float); remaining gaps (no collisions/jump-out, radian yaw) are D-122.
  Not covered: I did not check the Llama and trader llama rider gate. Vanilla never steers a llama (canBeControlledByRider false 1.14-1.18, getControllingPassenger null in 1.19.3, isSaddleable false, so no controlling passenger), but ridden.rs treats llamas as horses and steers them whenever the caller passes saddled=true. Whether mineflayer-physics-utils ever sends a ridden llama with saddled=true is decided outside this repo, so I could not confirm a real effect. The shared helpers ridden.rs calls are outside this scope: move_vehicle, block_below_affecting_movement, landing_position/on_position (which drop the getOnPos fence/wall/gate fallback), block_speed_factor, friction_influenced_speed and apply_block_effects. Vec3.normalize uses 1.0E-4 before 1.21.11 and 1.0E-5F after; it cannot change a camel dash because the camel's pitch is at most 45 degrees. The adapter's lerp feeding is not checked: vanilla 1.14-1.21.1 zeroes or skips lerpSteps for a locally controlled mob, and the engine applies any lerp_position it is given. Cosmetic state is not checked: Camel/Nautilus dashing end conditions (dashCooldown < 55 with onGround||isInWater in 1.19.3-1.19.4) and stand animation. For 1.9.4-1.12.2 I only read 1.12.2 (decompiled AbstractHorseEntity/PigEntity), relying on D-128/D-130 for 1.9.4-1.10.2 details. The 1.15.x and 1.17.x horse paths were checked only through the decompiled HorseBaseEntity jump factor and tryCheckBlockCollision lines. I did not check the zombie/skeleton horse attribute defaults for 26.x.
- **Projectiles.** Matched vanilla: ThrowableProjectile.tick 1.21.2-26.3: first-tick bubble column, gravity, inertia 0.8F/0.99F, hit-or-move, applyEffectsFromBlocks bubble sweep (26.2, 1.21.2; 26.3 findBlocksIn order); ThrowableProjectile.tick ≤1.21.1: hit test with old v, checkInsideBlocks from 1.16.2 at the old position, move by the post-bubble v, then inertia and gravity (1.21.1, 1.20.4, 1.16.5 vs 1.16.0, 1.14.4, 1.8.9); Throwable gravities: 0.03F/0.05F/0.07F floats before 1.20.5, doubles from 1.20.5 (1.20.4 vs 1.20.5); llama spit 0.06F→0.06, arrow 0.05F→0.05; BubbleColumnBlock.entityInside: isAir until 1.21.4, empty collision and fluid from 1.21.5; Entity clamps vs Projectile unclamped from 1.21.5 (1.21.2, 26.2); AbstractHurtingProjectile.tick: ≤1.21.1 move by the old v then (v+power)*inertia; from 1.21.2 inertia first then clip/move; xPower until 1.20.6, accelerationPower*normalize from 1.21; 0.95F/0.8F, WitherSkull 0.73F, wind charge 1.0F liquid inertia and 1.20.3-1.20.4 OUTLINE clip (1.8.9, 1.13.2, 1.14.4, 1.16.4, 1.20.4, 1.20.5, 1.20.6, 1.21, 1.21.1, 1.21.2, 26.2); Vec3.normalize: float sqrt before 1.17, 1e-4 vs 1e-5F threshold switch at 1.21.2 (1.21.1 vs 1.21.2); AbstractArrow flight: 1.21.2+ captured-movement clip, signum*0.05F pull-back with v=0, water inertia ordering 1.21.2-1.21.3 vs 1.21.4+; ≤1.21.1 v=hit-pos, normalize*0.05 pull-back (1.14+), 1.13.2 float /(sqrt*20F), 1.8.9-1.12.2 /sqrt*0.05F, then move, inertia 0.6F/0.99F, gravity even on the hit tick; Arrow in-ground fall conditions: 1.8.9-1.10.2 block+meta only, 1.11-1.13.2 hit block + box.grow(0.05), 1.14-1.15 floor(pos) + box.inflate(0.06), 1.16+ point.inflate(0.06); shakeTime/inGroundTime counters; 1200-tick despawn; Trident water inertia 0.99F (26.2 ThrownTrident:222); BlockGetter.traverseBlocks 1e-7 lerp and stepping, VoxelShape.clip short-ray/start-inside rules (26.2); FireworkRocketEntity unattached tick: 1.15 boost, 1.16 restore of v, 1.16.2 horizontalCollision factor, hit before the move from 1.21.2 (1.12.2, 1.14.4, 1.15, 1.16, 1.16.4, 26.2); FishingHook client tick 26.2: flying→bobbing at liquidHeight>0, onHitBlock normalize*distanceToSqr, bobbing pull, -0.03 gate with onGround from 1.21.6 (1.21.5 vs 1.21.6), 0.92 after the move; LlamaSpit tick: 0.99F, 0.06F gravity, no-air/in-water discard, bubble-column discard 1.13-1.21.4 (1.14.4, 1.20.4, 26.2); ShulkerBullet client tick: move by v only (26.2); 26.2→26.3 diff: ThrowableProjectile first-tick order change mirrored; AbstractArrow/ProjectileUtil changes are entity-hit only.
  Not covered: Projectile.shoot/shootFromRotation (uncertainty 0.0075 gaussian, inherited shooter movement) is not implemented anywhere in the engine (no grep hits), so there is nothing to compare. I did not compare entity hits (ProjectileUtil entity margin/pierce/deflection), world-border clips (clipIncludingBorder) or projectile removal, because the engine has no entities or border. Deferred for depth: fishing hook ticks before 1.16 (1.11-1.15 variants and the 1.13 own-water-height), the move_vehicle internals used by the fishing hook and firework (cobweb/stuck-speed via checkInsideBlocks, 26.2 restitution, the 26.3 MoveSimulationType SERVER_AND_CLIENT effect on canSimulateMovement), and the 1.9.4-1.11.2 decompiled throwable/fireball code beyond the arrow in-ground gate. I took the 1.16.2 start of the fireball checkInsideBlocks from the 1.16.0 (absent) vs 1.16.4/1.16.5 (present) sources plus the throwable jar check cited in the doc; I did not check the 1.16.1/1.16.2 jars myself. I did not verify the exact release (1.16.2 assumed) where throwable/llama spit OUTLINE clipping became COLLIDER beyond 1.16.0 (OUTLINE) vs 1.16.4 (COLLIDER). The shared block_traversal.rs (blocks_visited) and blocks_inside are outside this file and were taken as correct.
- **Fire, base tick, in-wall and push-out-of-blocks.** Matched vanilla: fire_immune_ticks: Player 20 / Entity 1 (1.17, 1.21.5); clear_fire: setRemainingFireTicks(0) through 1.21.8, min(0, ticks) from 1.21.9 (client1.21.4/1.21.5/1.21.9 Entity.clearFire); ignite_for_ticks: 1.17-1.20.6 transformFireDuration/getFireAfterDampener `duration - floor(duration * (lvl * 0.15F))` (same float order); 1.21+ Mth.ceil((double)t * BURNING_TIME); clearFreeze in igniteForTicks from 1.21.5; fire_ignite: 1.17-1.21.1 (+1, == 0 -> 8 s) and 1.21.2-26.3 (<0 +1, ServerPlayer +nextInt(1,3) taken as 1, >= 0 -> 8 s) (BaseFireBlock 1.20.6/1.21.1/1.21.2/1.21.4/1.21.5/1.21.11); lava_ignite: setOnFireFromLava/lavaHurt/lavaIgnite of 300 ticks, fire-immune gate (1.17, 1.20.6, 1.21, 1.21.4, 1.21.5, 1.21.9); base_tick_fire order and gates: water extinguish in updateInWaterState up to 1.21.4 (removed in 1.21.5), fire tick decrement / immune -4 then clear (to 1.21.8) / immune clear (1.21.9+), freeze clear while burning up to 1.21.4, lava ignite in baseTick up to 1.21.4, LivingEntity fireImmune clear (the 1.21.9+ removal is output-equivalent) and isWet extinguish up to 1.21.4, boat (non-LivingEntity) skip; 26.3 baseTick unchanged; apply_inside_fire_effects 1.17-1.21.1: per-block fire ignite, lava cauldron, layered cauldron extinguish with isEntityInsideContent, powder snow flag, contract(1e-6) fire/lava scan with floor..floor cells, reset rules (1.17 Entity.move :629-646); apply_inside_fire_effects 1.21.2-1.21.4: blocksInside fire/lava test, reset and wet/powder-snow reset (client1.21.4 Entity.applyEffectsFromBlocks :787-821); cauldron and fire inside shapes default to Shapes.block(); apply_inside_fire_effects 1.21.5-26.3: wasOnFire/previous ticks, InsideBlockEffectType order FREEZE, CLEAR_FREEZE, FIRE_IGNITE, LAVA_IGNITE, EXTINGUISH, isInRain clearFire, 1.21.5 vs 1.21.6+ reset condition; is_in_wall: 8-corner test with float offsets (1.8.9, 1.14.4, 1.15), 1.16 0.1F box via getBlockCollisions with border, 1.17 1e-6 box, 1.18.1 shape moved to the eye position, 1.19-1.21.11 non-air isSuffocating shape at its cell; LivingEntity/Player sleeping gate; move_towards_closest_space 1.14.4-1.16.1: checkInBlock/blocked floor(minY)..ceil(maxY) column, 9999.0, WEST/EAST/NORTH/SOUTH order, 0.1D velocity; move_towards_closest_space 1.16.2-26.2: deflate(1e-7) column suffocatesAt, Double.MAX_VALUE, direction order and 0.1 * step (1.16.4, 1.18.1, 1.20.6, 1.21.4, 1.21.11); push_out_of_blocks_before_1_14: 1.8.9, 1.12.2 and 1.13.2 corners at minY+0.5 and +-width*0.35, isOpenBlockSpace two-cell test, neighbour order, (double)0.1F velocity, noClip gate; launch_riptide push: float trig with 0.017453292F radians, Mth.sqrt length, `v *= s / len` order, strength 3*((1+lvl)/4) before 1.21 and 1.5+0.75*(lvl-1) from 1.21, 26.1+ passenger gate, 1.2 lift only on ground, 1.21.11 Mth.sin(double); displace: setLocationFromBoundingbox before 1.17, setPos from 1.17, lengthSqr > 1e-7 and the 1.21.2+ `requested - moved < 1e-7` condition (1.14.4, 1.16.4, 1.21.1, 1.21.2, 1.21.11 Entity.move).
  Not covered: I did not open the blocks_inside_moves / blocks_visited step generation from 1.21.5 (which cells produce FIRE_IGNITE, LAVA_IGNITE or EXTINGUISH steps, water's EXTINGUISH, the 1.21.9 cauldron inside shapes) beyond its doc comment, because it is shared visit logic outside this area. I did not check the per-release suffocating, in-wall and isNormalCube block data that the Mineflayer adapter sends (D-016, D-017, D-082 cover it); I compared only the logic that consumes it. For 1.9.4-1.12.2 and 1.15-1.17 I read only the decompiled jars for pushOutOfBlocks, isInsideWall and the 1.17 fire code, not every intermediate release. The fall-distance reset clip in Entity.move (FALLDAMAGE_RESETTING for moves with lengthSqr >= 1, which the 1.2 lift reaches) is not modelled in launch_riptide. I left it out because a grounded player normally has no fall distance. Whether the Mineflayer path ever ticks a passenger player through tick_with_rules is unverified (it affects how much the 26.3 passenger-gate finding matters). Rain-dependent paths (isInRain, isWet rain, riptide in rain, D-053) depend on the caller's in_rain input and I did not check them further.
- **Whole-tick order and version dispatch.** Matched vanilla: LivingEntity.aiStep order (noJumpDelay decrement -> negligible-velocity zeroing -> jump -> updateFallFlying (1.21.2+) -> Slow Falling/Levitation reset (1.20+) -> travel -> checkAutoSpinAttack -> pushEntities), checked against 1.8.9, 1.9.4 (yarn), 1.13.2, 1.15/1.16 (yarn), 1.19.4, 1.20, 1.21.4, 1.21.5 and 26.3; Negligible-velocity thresholds: 0.005 in 1.8.x, 0.003 from 1.9 (yarn 1.9.4 LivingEntity:1689); player horizontalDistanceSqr < 9.0E-6 from 1.21.5 (missing in 1.21.4, present in 1.21.5 and 26.3); 0.003*0.003 == 9.0E-6 in double; Fluid jump branches: pre-1.13 (water, then lava, then ground), 1.13-1.15 (waterHeight 0.4 rule, lava always swims), 1.16+ isAffectedByFluids gate with the eye-based threshold; the 26.2+ isInShallowFluid(LAVA) form is equivalent; jumpFromGround: 1.20.5 1e-5 guard and double sprint impulse, 1.21.2 Math.max(power, vy); jumpInLiquid and goDownInWater +-0.03999999910593033 (1.21.4, 26.3); LocalPlayer.aiStep order: autoJump makeJump -> moveTowardsClosestSpace -> goDownInWater (1.13+, isAffectedByFluids from 1.16) -> flight vertical input (two steps before 1.14, one netted impulse from 1.14) -> super.aiStep -> flying=false when onGround (1.8.9, 1.13.2, 1.14.4-1.21.11, 26.3); Player.aiStep flight resetFallDistance before super.aiStep from 1.21.2; before that Player.travel resets it with setSharedFlag(7,false) after travel (1.21.1 vs 1.21.2); Swim steering inside Player.travel, before the flight y capture (1.21.1, 1.21.2); Slow Falling/Levitation reset in aiStep from 1.20; in travel when going down before that (1.19.4 vs 1.20); LivingEntity.tick `if (!isRemoved()) aiStep()` gate at 1.19.1 (1.19 vs 1.19.1); Entity.baseTick order (water/lava push and water fall reset, then lava fallDistance*=0.5) for 1.21.4 and 26.3 updateFluidInteraction; flow scales 0.014/0.007/0.0023333333333333335 in 26.3; checkAutoSpinAttack (1.13.2, 1.21.x): box taken before travel, swept minmax, first LivingEntity bounces -0.2, otherwise horizontalCollision ends the spin; Scene order: entities, then block entities (pistons) in Minecraft.tick/ClientLevel (1.21.4, 26.3); the local player's own client pushEntities pushes nothing (1.14.4, 1.21.4); FireworkRocketEntity boost formula and its isFallFlying gate (1.21.4, 26.3); Pre-1.14 unloaded-column -0.1 sink with (int) cast and an always-true chunkExists (1.8.9 ChunkProviderClient); 1.14+ gate consistent with X-1; Profile table across all 50 v-*.rs modules: gravity, drags, lava flow 0.014 before 1.16 and 0.0023333 from 1.16, travel split 1.21.2, gravity-argument check 1.20.5, efficiency attribute and divisor 1.21, Floor rounding 1.21.5, modern fall damage 1.20.5+, restitution 26.2+; contract enforced by tick_conforming in versioned/mod.rs; version.rs capability gates: fluid 1.13, honey 1.15, fall flying 1.9, water efficiency 1.21, minor horizontal collision 1.18 (absent in yarn 1.17).
  Not covered: I did not compare the travel internals line by line (relative movement, friction, drag, glide, fluid travel), Entity.move collision, fall damage, or the adapter's sprint, crouch, pose and swimming logic, because they belong to other areas. Only the order in which they run within the tick was checked. For 1.9.4-1.12.2, 1.15 and 1.17 I spot-checked the yarn decompiles for aiStep only. For 1.15.x/1.16.0-1.16.3 lava-state timing and the 1.14-1.16 entity hash-order model (D-095) I relied on the existing deviation notes. The N-API bulk path (simulateVersionedTicks/TickInputSnapshot.next) keeps auto_spin_attack as a bool with no 20-tick countdown, and a riptide launch never sets it. I noted this but did not report it, because it is N-API only and close to D-154. The generic N-API firework_boost input is added inside travel whether or not the player glides. It is not vanilla, but the Mineflayer path does not use it, so it is not reported.
- **N-API state bridge and output fidelity.** Matched vanilla: NativePlayer input/output index and flag tables: src/native.ts:499-587 matches native/src/napi_player.rs input/input_flag/output/output_flag one to one (all releases); deltaMovement and position: lossless f64 in both directions; 1.21.11+ setDeltaMovement non-finite refusal gated as in 1.21.11 Entity.java:3664-3668; onGround / horizontalCollision / minorHorizontalCollision / verticalCollision round trip; the adapter's VERTICAL_COLLISION_BELOW = verticalCollision && onGround equals the engine's value, because every refresh path sets on_ground = vertical_collision_below (simulation.rs:4280-4290, piston.rs:597-598, riptide lift :3371) and the local player always refreshes (1.21.4+ gate); fallDistance: the engine stores f32-representable values before 1.21.5 (accumulated_fall_distance) and JS f64 carries them losslessly; landing damage reset per tick (take_input sets fall_damage = 0); noJumpDelay (JUMP_TICKS) and autoJumpTime: integer round trip, decremented as in LivingEntity.aiStep / LocalPlayer.aiStep; stuckSpeedMultiplier: Vec3 double round trip; persists to the next move as Entity.move consumes it (1.14-26.3); flyingSpeed: abilities float sent each tick (flySpeed ?? 0.05F); engine flying_speed() matches 1.8.9 EntityPlayer.onLivingUpdate (after super, so wasSprinting), 1.16.4 Player.aiStep :480-482 / travel :1364-1369, and 1.19.4+ getFlyingSpeed; Sprint modifier: adapter setSprinting adds (double)0.3F MULTIPLY_TOTAL at once; attribute fold base/op0/op1/op2 in double with RangedAttribute clamp (base.ts:181-199); SPRINT_MODIFIER flag computed after the sprint decision; slow-input setState picks up the new movementSpeedValue the same tick; Yaw/pitch: floatRadians = f32(f32(deg)*0.017453292F) matches moveRelative 1.9+; YAW_DEGREES carried for 1.8 (NativePlayer path); Pose sizes and eye heights sent as f32 (Player pose table: 0.6x1.8, 0.6x1.5, 0.6x0.6, fixed 0.2x0.2; eyes 1.62F/1.27F/0.4F/0.2F; scale applied in float, 1.20.5+); NativePlayer.set_state(state()) round trip (carryFor when slow inputs change): every engine-kept field survives (previous_position, bounding_box, supporting_block(+kept), on_ground_no_blocks, outside_border, in_web, remaining_fire_ticks, ticks_frozen, tick_count); minor_horizontal_collision_supported is recomputed each tick in tick_local_player_scene; yaw_degrees/riptide_lift are per tick; checkSupportingBlock port (simulation.rs:1483-1508) vs 1.20.4 Entity.java:580-600; Flight ends on landing outside spectator in both the engine (simulation.rs:4513) and the adapter (player.ts:336), as in LocalPlayer.aiStep after super.aiStep; Boat state round trip: deltaRotation/landFriction float casts, yawDegrees f32 (vehicles.ts:240-288, napi_api.rs:5540-5560).
  Not covered: I did not check the client-side powder-snow speed modifier (LivingEntity.removeFrost/tryAddFrost after travel, which runs on the client in 1.17-1.21.4 and only in ServerLevel by 1.21.11). Neither the engine nor the adapter recomputes it from the synced ticksFrozen; the adapter relies on the server's attribute sync. Whether that can differ for a whole tick depends on packet timing, so I did not report it. PlayerState.supportingBlockPos is never written from the engine; it is an output-only field and no adapter decision reads it. I did not audit the API defaults of the generic NativeSimulationState path for direct callers (isPlayer=false, eyeHeight=height*0.85, no speed/slowness amplifiers from the adapter): these are API conventions rather than vanilla ports, and D-064/D-068 cover the related attribute losses. Not checked: a server teleport to the exact same coordinates (UNMOVED stays set, so the pre-1.17 kept box is not replaced the way setPos would), and the camel's initial yawDegrees (?? 0) when sitting; both are rare and depend on the external state library. I did not trace engine-internal physics behind the bridge (travel, collision, fluids), which belongs to other audit areas.
- **TypeScript Mineflayer adapter logic.** Matched vanilla: LocalPlayer.aiStep sprint start/stop (double tap, sprint key, run/swim stop, trigger-time decrement and resets): 1.8.9, 1.12.2, 1.13.2, 1.14.4, 1.15, 1.16, 1.19.3, 1.19.4, 1.20.6, 1.21.1, 1.21.2, 1.21.4 (shouldStopSprinting), 1.21.5, 1.21.8, 1.21.9 (isSprintingPossible), 1.21.11, 26.3; 1.8 600-tick sprint timeout and setSprinting counter (1.8.9); Input slowdown order: sneak then item (1.8-1.21.3, sneak via input.tick from 1.19 with the swift-sneak clamp), item then sneak in 1.21.4, KeyboardInput normalization plus engine modifyInput from 1.21.5 (1.21.5, 1.21.11, 26.3); Crouching formula 1.15-26.3 (isInSneakingPose / crouching field, previous-tick shift key); Flight double-jump toggle incl. spectator and auto-jump guard, jumpFromGround on ground from 1.20.5 (1.20.4 vs 1.20.5), jumpTriggerTime decrement order, flying cleared on ground; Elytra start: none before 1.15 (packet only); tryToStartFallFlying 1.15/1.16 (levitation from 1.16), 1.21.2+ canGlide with Player !flying, 26.3 isInLiquid; LivingEntity.jumpFromGround power/sprint boost (1.21.11) and LivingEntity.aiStep jump gate isAffectedByFluids for flying players (1.16.4-1.21.11); Player.travel glide-flag clear 1.21.1 vs 1.21.2; Player.updatePlayerPose / getDesiredPose 1.14.4, 1.16.4, 1.21.4, 1.21.5, 1.21.11, 26.3; Legacy EntityPlayer.updateSize heights 1.9.4/1.12.2/1.13.2 and 1.13 getEyeHeight; Entity.updateSwimming and Player.updateSwimming (1.13 feet-free, 1.17.1+ feet water); 1.16-1.20.4 eye offset 0.11111111F, 1.20.5+ getEyeY, 1.13-1.15 float y+(h+0.11111111F); LocalPlayer.isUnderWater = wasUnderwater set at Player.tick start; LivingEntity.onClimbable incl. trapdoor-over-ladder (1.9+), Player.onClimbable flying (1.21.4+), can_glide_through (1.21.11), 26.3 climbable tag; Mth.sin/cos tables and argument casts (1.8.9, 1.12.2, 1.14.4, 1.21.10 vs 1.21.11 double, 26.3); calculateViewVector 1.13+ and getVectorForRotation pre-1.13; LivingEntity.setSprinting modifier 0.30000001192092896 ADD_MULTIPLIED_TOTAL and AttributeInstance.calculateValue op order; HashSet (UUID.hashCode) and Object2ObjectOpenHashMap iteration containers (1.16.4, 1.20.6); FlowingFluid.getFlow (affectsFlow, 0.8888889 drop, falling -6 turn, isSolidFace far-face quirk, ice exclusion) and legacy BlockLiquid.getFlow 1.12.2 with float-sqrt normalize; EntitySelector.pushableBy team rules (pusher rule vs local player's rule, isAlliedTo identity); Entity.refreshDimensions refit (1.14.4) and the engine's kept_bounding_box refit at tick start; AbstractHorse.onPlayerJump scale and rearing gates 1.19.3/1.19.4/1.21.4/1.21.5/1.21.6/1.21.11; Camel.canJump/refuseToMove (1.21.11); Mth.wrapDegrees and Entity.setRot.
  Not covered: I could not read 1.15.1/1.15.2 or 1.17.x source exactly: only decompiled 1.15 and 1.17 are available, so the adapter's gt('1.15.2') sneak reset, the 1.15.2 climbable gate and the no-deflate end at 1.16 are bracketed, not confirmed. The data tables in legacy-connections.json (fence, pane and wall joins, normalCube, flowBlocksMovement, flowSolid) and the minecraft-data shapes were not audited entry by entry. I also skipped legacy door, stairs and chest shapes against each pre-1.13 release, the 1.8.9 legacy flow form, and the 1.13 isSolidFace exception list beyond spot checks. I did not trace how Mineflayer itself sets isUsingItem, itemUseTicks, useItemCanSprint, fallFlying and pose, so the riptide >=10-tick charge count and the server-cleared glide timing are inferred from adapter comments. vehicles.ts boat and horse plumbing was only skimmed, apart from riderJump, camel jump and lerp. The interpolation math (lerpStep across 1.20.2, 1.21.5 and 26.3) was not compared line by line. world.ts sync and caching and versions.ts dispatch are not vanilla logic and were not compared. The 26.1-26.3 EntityFluidInteraction eye scan is covered by D-038, which I did not re-derive. The world-border lerp state and touchingUnloadedChunk (1.16-1.21.11), which the adapter applies only from 26.1, were noted but not quantified.

**Completeness critic.**

Completeness critique of the parity audit. I read the engine myself; nothing was built or run.

**(1) Engine functions or behaviors no area audited**
- **`WorldSnapshot::supporting_block_position`** (world.rs:1923). This is vanilla `CollisionGetter.findSupportingBlock`, and it feeds `check_supporting_block`, `main_supporting_block` and every 1.20+ `getOnPos` (friction, jump factor, stepOn, landing). The bridge area only checked the 1.20.4 driver around it. The loop order (z, y, x) and the tie-break (BlockPos.compareTo, y then z then x, largest wins) match 1.21.11.
  - It tests each box with strict `AABB.intersects`. Vanilla `BlockCollisions` uses `joinIsNotEmpty` for any shape that is not `Shapes.block()`. This is the same class of defect as the Collision area's CONFIRMED `Colliders::listed` finding, but a separate call site (1.20 to 26.3). Nobody has looked at it.
- **Swim steering**, simulation.rs:3983-4060 (Player.travel swim: `swimming_head_in_fluid`, 0.085/0.06, look.y). Every area skipped it as D-030. It has never been re-verified for 26.x, where the head test reads any fluid, lava included.
- **Eye-in-fluid** (world.rs `eyes_in_fluid`, adapter player.ts:717-760). Skipped as D-037/D-038, and the 26.3 `getHeightForCamera` change is unaudited.
- **Block friction table** (src/mineflayer/blocks.ts:19 `FRICTION`, consumed by `slipperiness_at`):
  - Releases: travel formulas were verified, but no area checked the table's values or names per release. In particular: frosted_ice only from 1.9, blue_ice from 1.13, and the 1.8 `slime` name.
  - Gaps: nobody checked whether new 26.x blocks carry `.friction()`, or whether `bounceRestitution` (blocks.ts:185: slime 1, bed 0.75) matches 26.2/26.3 Blocks.java. Shelf_mushroom and straw_bed were only covered for `fallOn`.
- **Spectator / noPhysics move path** (simulation.rs:3789, 4219, 4269). Only the 1.21.9 flag reset was checked. These were not compared: `Player.tick` setting `noPhysics = isSpectator` and the onGround clear, the pre-1.17 `setBoundingBox` + `resetPositionToBB` form, and the 1.8 `noClip` moveEntity.
- **LocalPlayer.tick load gate**:
  - From 1.21.4 vanilla skips `super.tick()` until `hasClientLoaded()` (respawn/login, 60-tick timeout). Neither the engine nor the adapter models this.
  - Before 1.14, `EntityPlayerSP.onUpdate` has an `isBlockLoaded` gate, which can fail. X-1 refuted only the 1.14+ case.
- **Mob movement in `ridden.rs::move_mob` and `simulate_versioned_entity_ticks`** (napi_api.rs:2038). Nobody traced the non-player entity tick: accumulate vs land fall distance, inside-block effects, stuck speed.
- **N-API-only helpers**: `clamp_to_world_border` (world.rs:661; the 1.21 / 1.21.2 inset forms) and the moving-border lerp (D-070). Neither was compared.
- **Adapter data plumbing that is not vanilla logic but feeds it**: world.ts sync timing, versions.ts class dispatch, and the legacy-connections.json tables entry by entry. Every area took these as given.

**(2) Release eras not clearly compared**
- 1.9.4-1.11.2, 1.15.x, 1.16.0-1.16.3, 1.17.x and 1.18.2 were checked only through Yarn decompiles or by bracketing neighbours, in every area. 1.18.2 has no source at all, yet the engine has gates at 1.18.2: the clip, glide `Math.cos`, the `Mth.equal` motion stop, and possibly the sprint 0.006F.
- 1.15.1 / 1.15.2 and 1.17.1 boundaries are inferred, not seen: sneak reset, climbable glide gate, jump-boost double switch.
- 26.1.1 and 26.1.2: no area opened them, but I diffed origin/client26.1 → 26.1.1 → 26.1.2-rc-1 under world/entity, level/block, client/player and phys and found no changes. Treating them as 26.1 is safe.
- Branch caveat: origin/client1.16.4 holds a 20w45a snapshot, according to the Voxel area. Any area that cited "1.16.4" from git for a 1.16.x-specific gate (Fluids, Sneak, Pistons, Projectiles, Boats, Push) may have compared against 1.17-snapshot code. Those citations need re-checking against decompiled 1.16 / 1.16.5.
- The adapter has no minecraft-data for 1.19.1, 1.21.2, 1.21.7, 26.1.1-26.3, so shapes and attribute keys there are unverified.

**(3) Most important gaps, ranked**
1. **Attribute keys on 1.21+**: the CONFIRMED mismatch silently drops speed, slowness, depth strider, gravity, step height and jump strength. Its scope on 26.1.1-26.3 is unchecked, and it is the highest-impact item.
2. **Strict-intersects merge tolerance**: extend the Collision finding to `findSupportingBlock` (above) and to `no_collision_for_entity` / `shape_meets_query`, which the Sneak area left out. These are the same tolerance class.
3. **Swim steering and eye-in-fluid** (D-030/D-037/D-038) were never re-derived. They decide swim-up and sprint-swim on every water tick.
4. **26.2/26.3 empty `#fall_damage_resetting` and `#climbable` tags** (CONFIRMED by the World area). The Block-surface area verified climbable "tag contents in 1.21.11/26.3" from source, but nobody checked what the adapter actually ships for 26.x. Climbing may be broken there.
5. **1.14.4 spectator sneak**: the Ground area and the Sprint area both report it as CONFIRMED, but give different resulting flight input lengths (1.0 vs 0.98 against 3.33 vs 1.0). One verified line-by-line trace is needed.
6. **Ground-jump `fall_distance = 0`**: the Ground area judged it unobservable, Whole-tick rated it PLAUSIBLE and Fall-distance CONFIRMED. One decision is needed, with the 1.21.4 local-player and non-local 1.21.2+ cases spelled out.
7. **Riptide lift**: block effects and the slime/bed bounce are reported three times, across the Block-surface, Fire and Whole-tick areas. These should merge into one item. The FALLDAMAGE_RESETTING clip on the 1.2 lift is still unmodelled.
8. **Llama riding**: ridden.rs steers llamas whenever `saddled=true` is passed. This is unconfirmed but cheap to gate.

### 8.4 Refuted claims

- Before 26.1, the fluid scans skip vanilla's loaded-chunk gate (hasChunksAt / isAreaLoaded / touchingUnloadedChunk) (Fluids (player path)). NOT: On the client, the vanilla loaded-chunk gate never fails in any release before 26.1, so the engine skipping it changes nothing. In 1.14 to 1.21.11, hasChunksAt and touchingUnloadedChunk both go through Level.hasChunk. The client overrides that to always return true: origin/client1.21.11 ClientLevel.java:381-383, origin/client1.16.4 ClientLevel.java:220-222 and origin/client1.14.4 MultiPlayerL
- Adapter counts a FALL_FLYING pose with the glide flag clear as crawling (isMovingSlowly); vanilla isVisuallyCrawling is SWIMMING only (Sprint, pose, creative/spectator flight, auto-jump). NOT: The claim is wrong because it checks only the base method in Entity and misses the override in LivingEntity. LivingEntity.isVisuallySwimming returns `super.isVisuallySwimming() || !this.isFallFlying() && this.hasPose(Pose.FALL_FLYING)`. I found this in 1.14.4 LivingEntity.java:2597-2598, 1.21.11 LivingEntity.java:3485-3486 and 26.3 LivingEntity.java:3602-3603. Entity.isVisuallyCrawling calls 
- Vec3::normalized uses a 1e-5 cutoff and multiplies by the reciprocal; vanilla divides and its cutoff depends on the release (Voxel shapes, math, trig, block traversal). NOT: The quoted engine code is accurate: native/src/math.rs:273-281 uses a 1e-5 cutoff and multiplies by the reciprocal. But the only caller of Vec3::normalized in native/src and src is the unit test at math.rs:1448. No production path calls it, so it cannot change movement output. The normalize calls that do run are separate helpers. push_by_fluid in simulation.rs uses the release-gated threshold
- updateAutoJump is not run after a piston move that limitPistonMovement cuts to zero (1.13+) (Pistons). NOT: The code difference is real, but I don't think it can show up in movement output. Both sides were read directly: - Vanilla: LocalPlayer.move calls updateAutoJump(dx, dz) after super.move no matter what. That includes the case where Entity.move returns early because the push was cut to zero (26.3/1.21.11 `if (var2.equals(Vec3.ZERO)) return;`; 1.13.2 EntityPlayerSP.func_70091_d wraps Entity.fun
- 1.13.2 pushOutOfBlocks: the swimming test uses the raw swimming flag, not EntityPlayer.isSwimming (which is false while flying) (Fire, base tick, in-wall and push-out-of-blocks). NOT: The vanilla side of the claim is accurate. In 1.13.2, EntityPlayerSP.func_205027_h branches on func_203007_ba(), and EntityPlayer overrides that as `!capabilities.isFlying && !isSpectator() && super.isSwimming()`. The engine's `state.swimming` is not the raw flag, though. The TypeScript adapter sets it in updateSwimming (src/mineflayer/player.ts:826-835), which runs every tick before the nati
- isMovingSlowly treats a FALL_FLYING pose without the glide flag as crawling (non-vanilla term) (TypeScript Mineflayer adapter logic). NOT: The claim misses a subclass override. LivingEntity overrides isVisuallySwimming in every release checked (1.14.4 LivingEntity.java:2597-2598, 1.16.4 :2857-2858, 1.21.4 :3270-3271, 1.21.11 :3485-3486, 26.3 :3602-3603). The override is `return super.isVisuallySwimming() || !this.isFallFlying() && this.hasPose(Pose.FALL_FLYING);`. Entity.isVisuallyCrawling is `isVisuallySwimming() && !isInWater(
- Pose and size fit checks deflate by 1e-7 on 1.13.2-1.15.x; vanilla adds the deflate only in 1.16 (TypeScript Mineflayer adapter logic). NOT: The claim is wrong. In vanilla 1.13.2-1.15 the call sites have no explicit contract(1e-7), but the check they call ignores overlaps under 1e-7 anyway, so the adapter's 1e-7 deflate gives the same result. Engine side (confirmed): src/mineflayer/player.ts:860-861 and 1109 deflate the box by 1e-7 and pass it to isFree (:1284), which is a strict AABB test. :1149 deflates for 1.13 and later. Vanil
- Pre-1.13 eye-in-water uses double 1.62 and a re-associated 0.4F+0.001 inset (TypeScript Mineflayer adapter logic). NOT: The arithmetic described is accurate. updateLegacyFluids (player.ts:786, 799) does use the double 1.62 for a standing pre-1.14 player, because eyeHeight returns 1.62 at line 1262 and only the 1.13+ path at line 745 wraps it in f32. It also re-associates the inset as minY + (f32(0.4) + 0.001), while vanilla 1.8.9/1.12.2 does expand(0,-0.4000000059604645,0).contract(0.001). But nothing that aff
- Horse rearing from the rider's jump ignores NoAI from 1.21.5 (TypeScript Mineflayer adapter logic). NOT: The vanilla side of the claim is correct. In 1.21.5, AbstractHorse.standIfPossible (:674-680) requires canPerformRearing() && isEffectiveAi(). Entity.isEffectiveAi there is isLocalInstanceAuthoritative() (:3242), and Mob.isEffectiveAi (:1195) adds && !isNoAi(). In 1.21.6+ the extra condition `|| !level.isClientSide` is false on the client. So a NoAI horse does not rear on the client, while ve
