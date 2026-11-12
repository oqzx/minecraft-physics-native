# Entity.move phases by release

Ticket B083. Per-release mapping of vanilla `Entity.move` to the native engine: source methods, state writes, early returns.

Status words: `mapped` means both sides were read and agree, or the difference is proved to reach no engine output (the item id says why). `no-op` means vanilla writes only what no engine output depends on: sounds, game events, render and statistics fields; section 3 gives the reason per item. `open` means a reachable difference is left, with the precise reason in section 3. `n/a` means that path never takes the phase. A pair `a / b` gives the tick's own move, then `move_entity` of piston.rs. No `differs` or `unmapped` is left.

Since B083's port the player's Entity.move paths (the tick's own move, `move_entity` of a piston or shulker box, the riptide lift) are one function, `entity_move` in simulation.rs, and every other entity's `run_vehicle_move` in vehicle.rs shares its fall-reset clip (`reset_fall_on_clip`), noPhysics branch (`move_without_physics`) and 1.21.4 vertical gate (`refreshes_vertical_collision`). Section 3 names each.

Abbreviations. `tree:N` is Entity.java line N in that tree: `E:\pr\mc-decompiled\<tree>\net\minecraft\entity\Entity.java` up to 1.21.4, `...\net\minecraft\world\entity\Entity.java` from 1.21.5, `E:\pr\mc-source\<tree>\client\net\minecraft\world\entity\Entity.java` for 26.2 and 26.3. Trees 1.21.5 onward are mojang-named. Older ones are yarn-named. Other vanilla files are named in full. Native line numbers in the tables (`sim` simulation.rs, `pis` piston.rs, `col` collision.rs, `veh` vehicle.rs, `mov` moving_entity.rs, all in `native\src`) are at commit 867132d, before the port; section 3 names the functions that now hold each phase. `P` is `move_entity` of piston.rs.

## 1. Eras

Found by extracting `move` from all 33 trees and diffing consecutive pairs with whitespace ignored. Pairs with no text difference: 1.19.2 to 1.19.3; 1.20, 1.20.2, 1.20.4, 1.20.5, 1.21; 1.21.6, 1.21.7, 1.21.8; 1.21.10, 1.21.11. Pairs with name changes only: 1.19 to 1.19.2 (emitter argument), 1.19.3 to 1.19.4 (`getControllingPassenger`). 1.7 is not registered and was not placed.

| Era | Exact trees | Registered, no exact source (placement unverified) |
| --- | --- | --- |
| A | 1.9.4, 1.10.2 (1.8.9 tree is not registered) | 1.8.8, only the 1.8.9 stand-in |
| B | 1.11.2, 1.12.2 | |
| C | 1.13.2 | |
| D | none registered; tree 1.14 is exactly 1.14 by its version.json | 1.14.4 (between D and E, not bracketed) |
| E | 1.15 | 1.15.1, 1.15.2 (between E and F, not bracketed) |
| F | 1.16, 1.16.5 | 1.16.1 - 1.16.4 (bracketed by F, inner `inLava` change falls in them) |
| G | 1.17, 1.18 | 1.17.1 (bracketed), 1.18.1, 1.18.2 (between G and H, not bracketed) |
| H | 1.19, 1.19.2, 1.19.3, 1.19.4 | 1.19.1 (bracketed) |
| I | 1.20, 1.20.2, 1.20.4, 1.20.5, 1.21 | 1.20.1, 1.20.3, 1.20.6 (bracketed), 1.21.1 (between I and J, not bracketed) |
| J | 1.21.3, 1.21.4 | 1.21.2 (between I and J, not bracketed) |
| K | 1.21.5 | |
| L | 1.21.6, 1.21.7, 1.21.8 | |
| M | 1.21.9, 1.21.10, 1.21.11 | 26.1, 26.1.1, 26.1.2 (between M and N, not bracketed) |
| N | 26.2 | |
| O | 26.3 | |

Bracketed means both neighbouring trees are in the same era. It is not verification; the table below is.

Exact placement of every modern release, from the single-decompiler trees `E:\mc-physics-demo-wt\compare-cache\<v>\client\net\minecraft\world\entity\Entity.java` (all 44 releases 1.14.4 - 26.3), by hashing `move` with whitespace and `final` dropped and diffing consecutive releases:

| Same `move` text | Era | Note |
| --- | --- | --- |
| 1.14.4 | D | its own; collide has the single-axis split (1.14.4 Entity.java:725-731), so the native 1.14 gate holds |
| 1.15, 1.15.1, 1.15.2 | E | |
| 1.16, 1.16.1 | F | `inLava = false` kept |
| 1.16.2 - 1.16.5 | F | `inLava = false` gone: native gate 1.16.2 |
| 1.17, 1.17.1 | G | |
| 1.18, 1.18.1 | G | |
| 1.18.2 | G to H | the fall-reset clip, verticalCollisionBelow and the velocity stop by flags arrive here, not in 1.19: native gates 1.18.2 |
| 1.19 | H | the stepOn test loses isSteppingCarefully (the blocks test it); STEP event rewritten |
| 1.19.1 - 1.19.4 | H | STEP event context only |
| 1.20 - 1.20.6 | I | |
| 1.21, 1.21.1 | I | setOnGroundWithMovement rename only |
| 1.21.2, 1.21.3 | J | native gates 1.21.2 |
| 1.21.4 | J | |
| 1.21.5 | K | |
| 1.21.6 - 1.21.8 | L | |
| 1.21.9 | M | |
| 1.21.10, 1.21.11 | M | |
| 26.1 - 26.1.2 | M | from 1.21.11 only names change (deobfuscated): no restitution, as versioned/mod.rs has it |
| 26.2 | N | |
| 26.3 | O | |

1.8.8 has its own tree, `E:\mc-physics-demo-wt\legacy-src\1.8.8\net\minecraft\world\entity\Entity.java`; its one difference from the 1.8.9 stand-in in this map is D28.

Boundaries in `move` itself, with a line on both sides:

| Change | Before | After |
| --- | --- | --- |
| A to B: mover type argument | 1.10.2:549 | 1.11.2:551 |
| A to B: piston limit before the move (before-side line is the first line after noClip) | 1.10.2:554 | 1.11.2:556 |
| A to B: edge probe -1.0 to -stepHeight | 1.10.2:575 | 1.11.2:614 |
| A to B: emission gate rewritten | 1.10.2:770 | 1.11.2:827 |
| A to B: step sound moves to the else of the swim sound | 1.10.2:798 | 1.11.2:856 |
| B to C: Box lists to VoxelShapes | 1.12.2:667 | 1.13.2:674 |
| C to D: piston limit to a helper; `move` shrinks from 307 to 128 lines | 1.13.2:569 | 1.14:545 |
| C to D: web flag to multiplier | 1.13.2:611 | 1.14:553 |
| C to D: exact to approximate horizontal test | 1.13.2:771 | 1.14:568 |
| C to D: position write now behind 1.0E-7 (before-side line is the flag block) | 1.13.2:773 | 1.14:561 |
| D to E: landing pos to helper | 1.14:573 | 1.15:568 |
| D to E: stepOn out of the emission gate | 1.14:610 | 1.15:585 |
| D to E: speed factor added (before-side line is the following statement) | 1.14:646 | 1.15:629 |
| E to F: collided removed | 1.15:567 | 1.16:604 |
| E to F: fire test, burn gone | 1.15:631 | 1.16:669 |
| inside F: `inLava = false` removed | 1.16:658 | 1.16.5:689 |
| F to G: box and centre to setPosition | 1.16.5:609 | 1.17:537 |
| F to G: isRemoved exit after fall | 1.16.5:639 | 1.17:568 |
| F to G: canClimb to MoveEffect | 1.16.5:658 | 1.17:590 |
| inside G: collidedSoftly added | 1.17:563 | 1.18:572 |
| G to H: fall-reset clip added | 1.18:563 | 1.19:571 |
| G to H: velocity stop by flags | 1.18:585 | 1.19:608 |
| G to H: stepOn test leaves move | 1.18:599 | 1.19:617 |
| H to I: support block | 1.19.4:602 | 1.20:646 |
| H to I: step sounds to stepOnBlock | 1.19.4:652 | 1.20:684 |
| I to J: inside check leaves move (after-side line is what now follows the emission) | 1.21:713 | 1.21.3:712 |
| I to J: position write test | 1.21:624 | 1.21.3:654 |
| I to J: fall side gate | 1.21:656 | 1.21.3:686 |
| I to J: setMovement | 1.21:653 | 1.21.3:683 |
| inside J: vertical flags gated | 1.21.3:683 | 1.21.4:683 |
| inside J: fall skips player-controlled | 1.21.3:686 | 1.21.4:697 |
| J to K: on-fire write gone (after-side line is the first statement that follows) | 1.21.4:643 | 1.21.5:646 |
| J to K: per-axis Movement list | 1.21.4:675 | 1.21.5:685 |
| J to K: gate to isLocalInstanceAuthoritative | 1.21.4:697 | 1.21.5:709 |
| K to L: one Movement | 1.21.5:685 | 1.21.6:702 |
| L to M: noPhysics clears flags | 1.21.8:670 | 1.21.9:679 |
| L to M: clip cap | 1.21.8:694 | 1.21.9:704 |
| L to M: Movement holds requested delta | 1.21.8:702 | 1.21.9:715 |
| inside M: stuck speed skips PISTON | 1.21.9:694 | 1.21.10:694 |
| M to N: restitution replaces velocity stop and callback (26.1 - 26.1.2 no source between) | 1.21.11:768 | 26.2:815 |
| N to O: IDE guard | 26.2:744 | 26.3:764 |
| N to O: recordMovement | 26.2:784 | 26.3:768 |

Boundaries in helpers, which the era list does not split on:

| Helper change, `move` body unchanged | Before | After | Native gate |
| --- | --- | --- | --- |
| single-axis split appears | 1.14:796 (none) | 1.15:775 | sim:3948 from 1.14 (1.14.4 no source) |
| list colliders, canCollide border, single-axis split gone | 1.17:795 | 1.18:801 | sim:3926, 3948 gate 1.18 |
| candidate step heights | 1.20.5:887 | 1.21:891 | sim:3942 gate 1.21 (1.20.6 no source) |
| inside-box inset 0.001 to 1.0E-7 | 1.19.2:893 | 1.19.3:901 | sim:1666 gate 1.19.3 |
| inside-box inset 1.0E-7 to 1.0E-5F by blocks visited | 1.21:1017 | 1.21.3:1062 | sim:1668 gate 1.21.2 (no source) |
| entity query grows by maxUpStep | 26.2:1156 | 26.3:1187 | sim:2841 gate 26.3 |
| restitution rest test `<` to `<=` gravity | 26.2:848 | 26.3:878 | sim:3798 gate 26.3 |
| reset of fire ticks | 1.21.5:817 | 1.21.6:851 | sim:2545 gate 1.21.6 |

Inner changes of no physics weight: 1.8.9 to 1.9.4 Block to BlockState, indexed loops, the box in the fire test named `increment` in 1.8.9 and `contract` in 1.9.4, the same operation (1.8.9 Box.java:185-192); 1.9.4 to 1.10.2 a constant inlined; 1.11.2 to 1.12.2 fly sound added (1.12.2 move, end of the emission block).

The word `yarn name` matters in eras A-C: `inLava` there is the web flag, and `updateSubmergedInWaterState` in the noClip branch is the position-from-box step (1.7:463-466 shows it inline).

## 2. Phase tables

One table per era, rows in source order of the first tree. `as X` in a cell means the same cell of the same row in era X. Rows with no line in `move` come last. In the Line column, `tree:a, b` gives `a` in `move` and `b` in the helper. An `absent` line means no such phase in `move`.

### Era A: inline, no mover type

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: box offset, position from box | 1.8.9:462; 1.10.2:550 | box, x/y/z | yes: everything below stays unwritten (flags, onGround, fall, motion, callbacks, inside, speed, fire) | tick sim:5048-5053, displace sim:4006-4023, gates sim:5098, 4355, 5200, 5231, 1939, 2460; P pis:787-797 | mapped / mapped |
| stuck speed multiplier | web flag (yarn name `inLava`): movement *0.25/0.05F/0.25 | 1.8.9:470; 1.10.2:558 | flag false, motion 0 | - | slowed_by_stuck_speed sim:3848-3856, from sim:5033; P pis:820 | mapped / mapped |
| edge backoff | inline: on ground, sneaking, player; probe at -1.0 | 1.8.9:483; 1.10.2:571 | local movement | - | sim:5040 -> back_off_from_edge_before_1_14 sim:3508 (drop 1.0 sim:3522); P pis:830-832 | mapped / mapped |
| collide | inline Box offsets, y then x then z | 1.8.9:532; 1.10.2:620 | box per axis, local movement | - | collide_with_world sim:3888-3924 -> collide_legacy col:963, voxel false | mapped / mapped |
| setPos | box set per axis, always | 1.8.9:539; 1.10.2:628 | box, x/y/z | - | displace sim:4013-4014 (moved_bounds) | mapped / mapped |
| step-up | inline: swept and lifted tries, lowered, keep if further | 1.8.9:553; 1.10.2:644 | box, local movement | - | col:1001-1045 | mapped / mapped |
| collision flags | exact `!=` per axis; vertical; `collided` (1.8.9 `colliding`) | 1.8.9:638; 1.10.2:737 | horizontalCollision, verticalCollision, collided | - | sim:5088-5111; P pis:836-858. `collided` not held | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.8.9:640; 1.10.2:739 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | landing pos inline (fence, wall, gate under air), `fall`; 1.8.9 fallDistance reset only inside `> 0` (1.8.9:802) | 1.8.9:655; 1.10.2:756 | fallDistance; onLandedUpon -> damage. LivingEntity: water refresh first (1.8.9 LivingEntity:209) | - | landing_position sim:1814-1830, check_fall_damage sim:4338-4495, sim:5121; P pis:875. Reset unconditional sim:4476 (1.8.9 only differs, negative distance). Particles none | mapped (D16, D26) / mapped (D16, D26) |
| velocity zeroing | exact compare, x then z | 1.8.9:656; 1.10.2:757 | velocityX, velocityZ | - | sim:5172-5177, 5196-5198; P stop_blocked_motion pis:964-986 | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.setEntityVelocity if y differs. Overriders: Block, Slime; Bed from 1.12.2 | 1.8.9:665; 1.10.2:767 | velocityY | - | sim:5178-5195, slime in check_fall_damage sim:4403-4415 (earlier than vanilla); P pis:987-999 | mapped / mapped |
| movement emission | gate `canClimb() && !hasVehicle`, sounds only | 1.8.9:668; 1.10.2:770 | - | - | none | no-op (D22) / no-op (D22) |
| stepOn (onSteppedOn) | inside the canClimb gate, `lv != null && onGround`; not a sneaking player | 1.8.9:677; 1.10.2:779 | block-specific | - | apply_block_effects sim:1932-1963, from sim:5228; P pis:915. Slime, magma only. Redstone ore, stairs forward not held | mapped (D25) / mapped (D25) |
| walking effects | horizontalSpeed, distanceTraveled, step and swim sounds | 1.8.9:680; 1.10.2:782 | those, next step distance; swim sound uses random | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | in move, after walking effects | 1.8.9:701; 1.10.2:803 | block-specific | - | sim:5223-5230, blocks_inside sim:1662; P pis:908-917 | mapped / mapped |
| fire | wet test (water, rain; bubble column in C); burn(1); fireTicks +1, 8 s on 0; reset; extinguish | 1.8.9:710; 1.10.2:812 | fireTicks, damage | - | legacy_inside_fire sim:2389-2436, run after travel sim:5351. wet is in_water only (sim:2415), no burn damage, extra fire_immune test (sim:2417). P none (pis:692) | mapped / mapped |
| piston limit | none: 1.8.9, 1.10.2 move take no mover type | absent | - | - | pis:258-261 route to push_before_1_9, push_before_1_11 (no limit) | mapped / mapped |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |
| isRemoved exit | none | absent | - | - | gate 1.17 sim:5144 | mapped / mapped |
| block speed factor | none | absent | - | - | gate 1.15 sim:5231, pis:918 | mapped / mapped |

### Era B: MovementType, piston limit inline

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: box offset, position from box | 1.11.2:552; 1.12.2:555 | box, x/y/z | yes: everything below stays unwritten (flags, onGround, fall, motion, callbacks, inside, speed, fire) | tick sim:5048-5053, displace sim:4006-4023, gates sim:5098, 4355, 5200, 5231, 1939, 2460; P pis:787-797 | mapped / mapped |
| piston limit | inline clamp to +/-0.51 per axis, first non-zero axis | 1.11.2:556; 1.12.2:559 | pistonMovementDelta, its game time | yes: abs(limited) <= 1.0E-5F, or all zero. All below unwritten; delta array is written | limit_piston_movement pis:610-640, from pis:651. Vanilla keeps the other axes, native zeroes them; vanilla callers pass one axis only (1.11.2 PistonBlockEntity.java:161, 245) | n/a / mapped |
| stuck speed multiplier | web flag (yarn name `inLava`): movement *0.25/0.05F/0.25 | 1.11.2:598; 1.12.2:601 | flag false, motion 0 | - | slowed_by_stuck_speed sim:3848-3856, from sim:5033; P pis:820 | mapped / mapped |
| edge backoff | inline: adds SELF or PLAYER; probe at -stepHeight | 1.11.2:611; 1.12.2:614 | local movement | - | as A, drop stepHeight sim:3520; P pis:830 runs for SHULKER_BOX (`by_piston` false) | mapped / mapped |
| collide | inline Box offsets, y then x then z | 1.11.2:664; 1.12.2:667 | box per axis, local movement | - | collide_with_world sim:3888-3924 -> collide_legacy col:963, voxel false | mapped / mapped |
| setPos | box set per axis, always | 1.11.2:673; 1.12.2:676 | box, x/y/z | - | displace sim:4013-4014 (moved_bounds) | mapped / mapped |
| step-up | inline: swept and lifted tries, lowered, keep if further | 1.11.2:701; 1.12.2:704 | box, local movement | - | col:1001-1045 | mapped / mapped |
| collision flags | exact `!=` per axis; vertical; `collided` (1.8.9 `colliding`) | 1.11.2:794; 1.12.2:797 | horizontalCollision, verticalCollision, collided | - | sim:5088-5111; P pis:836-858. `collided` not held | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.11.2:796; 1.12.2:799 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | landing pos inline (fence, wall, gate under air), `fall`; 1.8.9 fallDistance reset only inside `> 0` (1.8.9:802) | 1.11.2:813; 1.12.2:816 | fallDistance; onLandedUpon -> damage. LivingEntity: water refresh first (1.8.9 LivingEntity:209) | - | landing_position sim:1814-1830, check_fall_damage sim:4338-4495, sim:5121; P pis:875. Reset unconditional sim:4476 (1.8.9 only differs, negative distance). Particles none | mapped (D16, D26) / mapped (D16, D26) |
| velocity zeroing | as A | 1.11.2:814; 1.12.2:817 | as A | - | as A | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.setEntityVelocity if y differs. Overriders: Block, Slime; Bed from 1.12.2 | 1.11.2:824; 1.12.2:827 | velocityY | - | sim:5178-5195, slime in check_fall_damage sim:4403-4415 (earlier than vanilla); P pis:987-999 | mapped / mapped |
| movement emission | gate `canClimb() && !hasVehicle`, sounds only | 1.11.2:827; 1.12.2:830 | - | - | none | no-op (D22) / no-op (D22) |
| stepOn (onSteppedOn) | inside the canClimb gate, `lv != null && onGround`; not a sneaking player | 1.11.2:836; 1.12.2:839 | block-specific | - | apply_block_effects sim:1932-1963, from sim:5228; P pis:915. Slime, magma only. Redstone ore, stairs forward not held | mapped (D25) / mapped (D25) |
| walking effects | horizontalSpeed, distanceTraveled, step and swim sounds | 1.11.2:839; 1.12.2:842 | those, next step distance; swim sound uses random | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | in move, after walking effects | 1.11.2:862; 1.12.2:867 | block-specific | - | sim:5223-5230, blocks_inside sim:1662; P pis:908-917 | mapped / mapped |
| fire | wet test (water, rain; bubble column in C); burn(1); fireTicks +1, 8 s on 0; reset; extinguish | 1.11.2:871; 1.12.2:876 | fireTicks, damage | - | legacy_inside_fire sim:2389-2436, run after travel sim:5351. wet is in_water only (sim:2415), no burn damage, extra fire_immune test (sim:2417). P none (pis:692) | mapped / mapped |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |
| isRemoved exit | none | absent | - | - | gate 1.17 sim:5144 | mapped / mapped |
| block speed factor | none | absent | - | - | gate 1.15 sim:5231, pis:918 | mapped / mapped |

### Era C: VoxelShapes inline

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: box offset, position from box | 1.13.2:565 | box, x/y/z | yes: everything below stays unwritten (flags, onGround, fall, motion, callbacks, inside, speed, fire) | tick sim:5048-5053, displace sim:4006-4023, gates sim:5098, 4355, 5200, 5231, 1939, 2460; P pis:787-797 | mapped / mapped |
| piston limit | inline clamp to +/-0.51 per axis, first non-zero axis | 1.13.2:569 | pistonMovementDelta, its game time | yes: abs(limited) <= 1.0E-5F, or all zero. All below unwritten; delta array is written | limit_piston_movement pis:610-640, from pis:651. Vanilla keeps the other axes, native zeroes them; vanilla callers pass one axis only (1.11.2 PistonBlockEntity.java:161, 245) | n/a / mapped |
| stuck speed multiplier | web flag (yarn name `inLava`): movement *0.25/0.05F/0.25 | 1.13.2:611 | flag false, motion 0 | - | slowed_by_stuck_speed sim:3848-3856, from sim:5033; P pis:820 | mapped / mapped |
| edge backoff | inline: adds SELF or PLAYER; probe at -stepHeight | 1.13.2:624 | local movement | - | as A, drop stepHeight sim:3520; P pis:830 runs for SHULKER_BOX (`by_piston` false) | mapped / mapped |
| collide | inline calculateMaxOffset, y x z | 1.13.2:674 | box per axis | - | collide_legacy voxel true (gate sim:3909) | mapped / mapped |
| setPos | box set per axis, always | 1.13.2:677 | box, x/y/z | - | displace sim:4013-4014 (moved_bounds) | mapped / mapped |
| step-up | inline: swept and lifted tries, lowered, keep if further | 1.13.2:696 | box, local movement | - | col:1001-1045 | mapped / mapped |
| collision flags | exact `!=` per axis; vertical; `collided` (1.8.9 `colliding`) | 1.13.2:771 | horizontalCollision, verticalCollision, collided | - | sim:5088-5111; P pis:836-858. `collided` not held | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.13.2:773 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | landing pos inline (fence, wall, gate under air), `fall`; 1.8.9 fallDistance reset only inside `> 0` (1.8.9:802) | 1.13.2:790 | fallDistance; onLandedUpon -> damage. LivingEntity: water refresh first (1.8.9 LivingEntity:209) | - | landing_position sim:1814-1830, check_fall_damage sim:4338-4495, sim:5121; P pis:875. Reset unconditional sim:4476 (1.8.9 only differs, negative distance). Particles none | mapped (D16, D26) / mapped (D16, D26) |
| velocity zeroing | as A | 1.13.2:791 | as A | - | as A | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.13.2:801 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| movement emission | gate `canClimb() && !hasVehicle`, sounds only | 1.13.2:804 | - | - | none | no-op (D22) / no-op (D22) |
| stepOn (onSteppedOn) | inside the canClimb gate, `lv != null && onGround`; not a sneaking player | 1.13.2:813 | block-specific | - | apply_block_effects sim:1932-1963, from sim:5228; P pis:915. Slime, magma only. Redstone ore, stairs forward not held | mapped (D25) / mapped (D25) |
| walking effects | horizontalSpeed, distanceTraveled, step and swim sounds | 1.13.2:816 | those, next step distance; swim sound uses random | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | in move, after walking effects | 1.13.2:841 | block-specific | - | sim:5223-5230, blocks_inside sim:1662; P pis:908-917 | mapped / mapped |
| fire | wet test (water, rain; bubble column in C); burn(1); fireTicks +1, 8 s on 0; reset; extinguish | 1.13.2:850 | fireTicks, damage | - | legacy_inside_fire sim:2389-2436, run after travel sim:5351. wet is in_water only (sim:2415), no burn damage, extra fire_immune test (sim:2417). P none (pis:692) | mapped / mapped |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |
| isRemoved exit | none | absent | - | - | gate 1.17 sim:5144 | mapped / mapped |
| block speed factor | none | absent | - | - | gate 1.15 sim:5231, pis:918 | mapped / mapped |

### Era D: helpers, Vec3d, multiplier

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: box offset, position from box | 1.14:541 | box, x/y/z | yes: everything below stays unwritten (flags, onGround, fall, motion, callbacks, inside, speed, fire) | tick sim:5048-5053, displace sim:4006-4023, gates sim:5098, 4355, 5200, 5231, 1939, 2460; P pis:787-797 | mapped / mapped |
| piston limit | adjustMovementForPiston, then `equals(ZERO)` | 1.14:545, 719 | delta array, game time | yes: Vec3d.ZERO. All below unwritten | pis:610-640; `to_bits` test pis:638 for Double.compare. Reset per game time: per call, carried by `piston_deltas` arg (napi_api.rs:2619, 2647) | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.14:553 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | Entity.clipSneakingMovement (:669): player, SELF or PLAYER, onGround, sneaking | 1.14:559, 669 | local movement | - | back_off_from_edge sim:3410-3488 (registered 1.14.4 only); P as B | mapped / mapped |
| collide | helper; every axis walks the world (method_17833 :796). No single-axis split in this tree | 1.14:560, 752 | none | - | collide_shapes col:524 with single_axis_walk sim:3948 from 1.14. Native comment cites client1.14.4, not available here. Tree 1.14 does not show it | mapped / mapped |
| step-up | in helper: swept and lifted | 1.14:560, 774 | none | - | StepUp::Lifted, lifted_step_up col:555-579, gate sim:3942 | mapped / mapped |
| setPos | only if lenSq > 1.0E-7; box then position | 1.14:561 | box, x/y/z | - | moves_position sim:3371, displace sim:4015-4016 | mapped / mapped |
| collision flags | horizontal by approximatelyEquals (1.0E-5F); collided kept | 1.14:568 | as A | - | vanilla_equal col:12, gate sim:5089. `collided` not held | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.14:570 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | landing pos inline (fence, wall, gate under air), `fall`; 1.8.9 fallDistance reset only inside `> 0` (1.8.9:802) | 1.14:587 | fallDistance; onLandedUpon -> damage. LivingEntity: water refresh first (1.8.9 LivingEntity:209) | - | landing_position sim:1814-1830, check_fall_damage sim:4338-4495, sim:5121; P pis:875. Reset unconditional sim:4476 (1.8.9 only differs, negative distance). Particles none | mapped (D16, D26) / mapped (D16, D26) |
| velocity zeroing | setVelocity(0, y, z) then setVelocity(x, y, 0) from one read: z stop restores x | 1.14:589 | velocity | - | sim:5168-5171; P pis:975-977 | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.14:599 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| movement emission | gate `canClimb() && !hasVehicle`, sounds only | 1.14:602 | - | - | none | no-op (D22) / no-op (D22) |
| stepOn (onSteppedOn) | as A, `onGround` only | 1.14:611 | block-specific | - | as A; turtle egg not held | mapped (D25) / mapped (D25) |
| walking effects | as A | 1.14:614 | as A | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | `inLava = false` first | 1.14:638 | + inLava | - | update_in_lava_after_move sim:1442-1454, gate 1.16.2 | mapped / mapped |
| fire | as C; wet is isTouchingWater (water, rain, bubble) | 1.14:647 | fireTicks, damage | - | as A | mapped / mapped |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |
| isRemoved exit | none | absent | - | - | gate 1.17 sim:5144 | mapped / mapped |
| block speed factor | none | absent | - | - | gate 1.15 sim:5231, pis:918 | mapped / mapped |

### Era E: landing pos helper, stepOn outside the emission gate, speed factor

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: box offset, position from box | 1.15:537 | box, x/y/z | yes: everything below stays unwritten (flags, onGround, fall, motion, callbacks, inside, speed, fire) | tick sim:5048-5053, displace sim:4006-4023, gates sim:5098, 4355, 5200, 5231, 1939, 2460; P pis:787-797 | mapped / mapped |
| piston limit | adjustMovementForPiston, then `equals(ZERO)` | 1.15:541, 694 | delta array, game time | yes: Vec3d.ZERO. All below unwritten | pis:610-640; `to_bits` test pis:638 for Double.compare. Reset per game time: per call, carried by `piston_deltas` arg (napi_api.rs:2619, 2647) | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.15:549 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | PlayerEntity override (1.15:1025, 1.16:1035, 1.16.5:1066). 1.16.5 adds !flying and an above-ground test | 1.15:555 | local movement | - | sim:3429 gate 1.16.2 (no source; bracketed by 1.16 and 1.16.5); P as B | mapped / mapped |
| collide | helper; single-axis split | 1.15:556, 775 | none | - | collide_with_world sim:3925-3973, col:524 | mapped / mapped |
| step-up | in helper: swept and lifted | 1.15:556, 741 | none | - | StepUp::Lifted, lifted_step_up col:555-579, gate sim:3942 | mapped / mapped |
| setPos | only if lenSq > 1.0E-7; box then position | 1.15:557 | box, x/y/z | - | moves_position sim:3371, displace sim:4015-4016 | mapped / mapped |
| collision flags | horizontal by approximatelyEquals (1.0E-5F); collided kept | 1.15:564 | as A | - | vanilla_equal col:12, gate sim:5089. `collided` not held | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.15:566 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | getLandingPos, `fall` | 1.15:570, 653 | as A; 1.16.5 soul speed on landing (LivingEntity:503-506) | - | as A. Soul speed boost, particles not in the move | mapped (D16, D26) / mapped (D16, D26) |
| velocity zeroing | as D | 1.15:572 | velocity | - | as D; gate 1.18.2 (no source; 1.18 old, 1.19 new) | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.15:582 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| stepOn (onSteppedOn) | own test: `onGround && !bypassesSteppingEffects` | 1.15:586 | block-specific | - | as A | mapped (D25) / mapped (D25) |
| movement emission | gate `canClimb() && !hasVehicle`, sounds only | 1.15:589 | - | - | none | no-op (D22) / no-op (D22) |
| walking effects | as A | 1.15:597 | as A; 1.17 speed field | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | `inLava = false` first | 1.15:621 | + inLava | - | update_in_lava_after_move sim:1442-1454, gate 1.16.2 | mapped / mapped |
| block speed factor | x, z scaled, after inside check; last in J | 1.15:629 | velocity x, z | - | block_speed_factor sim:1730, sim:5231-5235; P pis:918-922 | mapped / mapped |
| fire | as C; wet is isTouchingWater (water, rain, bubble) | 1.15:631 | fireTicks, damage | - | as A | mapped / mapped |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |
| isRemoved exit | none | absent | - | - | gate 1.17 sim:5144 | mapped / mapped |

### Era F: no collided flag, no burn in move

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: box offset, position from box | 1.16:576; 1.16.5:607 | box, x/y/z | yes: everything below stays unwritten (flags, onGround, fall, motion, callbacks, inside, speed, fire) | tick sim:5048-5053, displace sim:4006-4023, gates sim:5098, 4355, 5200, 5231, 1939, 2460; P pis:787-797 | mapped / mapped |
| piston limit | adjustMovementForPiston, then `equals(ZERO)` | 1.16:580, 724; 1.16.5:611, 754 | delta array, game time | yes: Vec3d.ZERO. All below unwritten | pis:610-640; `to_bits` test pis:638 for Double.compare. Reset per game time: per call, carried by `piston_deltas` arg (napi_api.rs:2619, 2647) | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.16:588; 1.16.5:619 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | PlayerEntity override (1.15:1025, 1.16:1035, 1.16.5:1066). 1.16.5 adds !flying and an above-ground test | 1.16:594; 1.16.5:625 | local movement | - | sim:3429 gate 1.16.2 (no source; bracketed by 1.16 and 1.16.5); P as B | mapped / mapped |
| collide | helper; single-axis split | 1.16:595, 805; 1.16.5:626, 835 | none | - | collide_with_world sim:3925-3973, col:524 | mapped / mapped |
| step-up | in helper: swept and lifted | 1.16:595, 771; 1.16.5:626, 801 | none | - | StepUp::Lifted, lifted_step_up col:555-579, gate sim:3942 | mapped / mapped |
| setPos | only if lenSq > 1.0E-7; box then position | 1.16:596; 1.16.5:627 | box, x/y/z | - | moves_position sim:3371, displace sim:4015-4016 | mapped / mapped |
| collision flags | as D, `collided` gone | 1.16:603; 1.16.5:634 | horizontalCollision, verticalCollision | - | as D | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.16:605; 1.16.5:636 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | getLandingPos, `fall` | 1.16:608, 683; 1.16.5:639, 713 | as A; 1.16.5 soul speed on landing (LivingEntity:503-506) | - | as A. Soul speed boost, particles not in the move | mapped (D16, D26) / mapped (D16, D26) |
| velocity zeroing | as D | 1.16:610; 1.16.5:641 | velocity | - | as D; gate 1.18.2 (no source; 1.18 old, 1.19 new) | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.16:620; 1.16.5:651 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| stepOn (onSteppedOn) | own test: `onGround && !bypassesSteppingEffects` | 1.16:624; 1.16.5:655 | block-specific | - | as A | mapped (D25) / mapped (D25) |
| movement emission | gate `canClimb() && !hasVehicle`, sounds only | 1.16:627; 1.16.5:658 | - | - | none | no-op (D22) / no-op (D22) |
| walking effects | as A | 1.16:635; 1.16.5:666 | as A; 1.17 speed field | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | 1.16 as D; 1.16.5 no inLava write | 1.16:659; 1.16.5:689 | as D | - | gate 1.16.2 (no source; bracketed) | mapped / mapped |
| block speed factor | x, z scaled, after inside check; last in J | 1.16:667; 1.16.5:697 | velocity x, z | - | block_speed_factor sim:1730, sim:5231-5235; P pis:918-922 | mapped / mapped |
| fire | no burn; reset; extinguish on isWet | 1.16:669; 1.16.5:699 | fireTicks | - | sim:2401-2412, after travel; P none | mapped / mapped |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |
| isRemoved exit | none | absent | - | - | gate 1.17 sim:5144 | mapped / mapped |

### Era G: setPosition, isRemoved exit, MoveEffect

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: setPosition | 1.17:536; 1.18:543 | x/y/z, box refit | as A-F | as A-F | mapped / mapped |
| piston limit | as D, after wasOnFire write | 1.17:540, 718; 1.18:547, 735 | delta array, game time; wasOnFire before it | yes: ZERO. wasOnFire already written, all else unwritten | as D; wasOnFire not held, see row fire | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.17:548; 1.18:555 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | PlayerEntity override (1.17:1031, 1.18:1018) | 1.17:554; 1.18:561 | local movement | - | as E; P as B | mapped / mapped |
| collide | helper; 1.18 list colliders, WorldBorder.canCollide, no single-axis split | 1.17:555; 1.18:562 | none | - | gates 1.18 sim:3926, 3948 | mapped / mapped |
| step-up | in helper: swept and lifted | 1.17:555, 765; 1.18:562, 776 | none | - | StepUp::Lifted, lifted_step_up col:555-579, gate sim:3942 | mapped / mapped |
| setPos | setPosition under the same test | 1.17:557; 1.18:564 | x/y/z, box | - | displace sim:4017-4018 | mapped / mapped |
| collision flags | as F; 1.18 adds collidedSoftly (1.17 none). Entity default false, override only in ClientPlayerEntity | 1.17:562; 1.18:569 | + collidedSoftly (1.18) | - | sim:5100-5103, pis:847-850: is_player without a side test | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.17:564; 1.18:577 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | as E | 1.17:567; 1.18:580 | as E | - | as E | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.17:568; 1.18:581 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | as D | 1.17:572; 1.18:585 | velocity | - | as D; gate 1.18.2 (no source; 1.18 old, 1.19 new) | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.17:582; 1.18:595 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| stepOn (onSteppedOn) | own test: `onGround && !bypassesSteppingEffects` | 1.17:586; 1.18:599 | block-specific | - | as A | mapped (D25) / mapped (D25) |
| movement emission | MoveEffect gate; STEP, SWIM events | 1.17:590; 1.18:603 | - | - | none | no-op (D22) / no-op (D22) |
| walking effects | as A | 1.17:599; 1.18:612 | as A; 1.17 speed field | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | in move | 1.17:630; 1.18:643 | block-specific | - | as A. Inset 0.001 to 1.19.2, 1.0E-7 from 1.19.3 (sim:1666, 1.19.2 Entity:893, 1.19.3:901) | mapped / mapped |
| block speed factor | x, z scaled, after inside check; last in J | 1.17:631; 1.18:644 | velocity x, z | - | block_speed_factor sim:1730, sim:5231-5235; P pis:918-922 | mapped / mapped |
| fire | wasOnFire at top; 1.0E-6 box; reset; extinguish on powder snow or wet | 1.17:634; 1.18:647 | wasOnFire, fireTicks | - | apply_inside_fire_effects sim:2468-2507, after travel; wasOnFire not held; P pis:702 | mapped (D23) / mapped (D23) |
| fall-reset clip | none in 1.8.9 - 1.18 | absent | - | - | gate 1.18.2 sim:5064 (no source; 1.18 has none, 1.19 has it) | mapped / n/a |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |

### Era H: fall-reset clip, per-axis velocity stop

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: setPosition | 1.19:549; 1.19.4:552 | x/y/z, box refit | as A-F | as A-F | mapped / mapped |
| piston limit | as D, after wasOnFire write | 1.19:553, 763; 1.19.4:556, 774 | delta array, game time; wasOnFire before it | yes: ZERO. wasOnFire already written, all else unwritten | as D; wasOnFire not held, see row fire | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.19:561; 1.19.4:564 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | adds `movement.y <= 0.0` (1.19 PlayerEntity:1059-1061) | 1.19:567; 1.19.4:570 | local movement | - | sim:3434 `delta.y > 0.0`: same but for NaN y; P as B | mapped / mapped |
| collide | helper | 1.19:568; 1.19.4:571 | none | - | as G | mapped / mapped |
| step-up | in helper: swept and lifted | 1.19:568, 804; 1.19.4:571, 815 | none | - | StepUp::Lifted, lifted_step_up col:555-579, gate sim:3942 | mapped / mapped |
| fall-reset clip | fallDistance != 0, d >= 1: clip FALLDAMAGE_RESETTING, Fluid.WATER over the whole move | 1.19:571; 1.19.4:574 | fallDistance 0 on hit | - | tick sim:5063-5086; P none | mapped / mapped |
| setPos | as G | 1.19:583; 1.19.4:586 | as G | - | as G | mapped / mapped |
| collision flags | adds verticalCollisionBelow (groundCollision) | 1.19:590; 1.19.4:593 | + verticalCollisionBelow | - | sim:5111; rest as G | mapped / mapped |
| onGround and support block | `onGround = vertical && y < 0` | 1.19:599; 1.19.4:602 | onGround | - | sim:5112; P pis:859. check_supporting_block also runs but is read only from 1.20 | mapped / mapped |
| checkFallDamage | as E | 1.19:602; 1.19.4:605 | as E | - | as E | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.19:603; 1.19.4:606 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | one write, by the approximate per-axis flags | 1.19:608; 1.19.4:611 | velocity | - | sim:5166-5167; P pis:973-974 | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.19:613; 1.19.4:616 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| stepOn (onSteppedOn) | `onGround` only; blocks test sneaking | 1.19:617; 1.19.4:620 | block-specific | - | as A; sculk sensor, shrieker not held | mapped (D25) / mapped (D25) |
| movement emission | MoveEffect gate; STEP, SWIM events | 1.19:621; 1.19.4:624 | - | - | none | no-op (D22) / no-op (D22) |
| walking effects | as A | 1.19:631; 1.19.4:634 | as A; 1.17 speed field | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | in move | 1.19:662; 1.19.4:665 | block-specific | - | as A. Inset 0.001 to 1.19.2, 1.0E-7 from 1.19.3 (sim:1666, 1.19.2 Entity:893, 1.19.3:901) | mapped / mapped |
| block speed factor | x, z scaled, after inside check; last in J | 1.19:663; 1.19.4:666 | velocity x, z | - | block_speed_factor sim:1730, sim:5231-5235; P pis:918-922 | mapped / mapped |
| fire | wasOnFire at top; 1.0E-6 box; reset; extinguish on powder snow or wet | 1.19:666; 1.19.4:669 | wasOnFire, fireTicks | - | apply_inside_fire_effects sim:2468-2507, after travel; wasOnFire not held; P pis:702 | mapped (D23) / mapped (D23) |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |

### Era I: support block, stepOnBlock

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: setPosition | 1.20:596; 1.21:603 | x/y/z, box refit | as A-F | as A-F | mapped / mapped |
| piston limit | as D, after wasOnFire write | 1.20:600, 843; 1.21:607, 850 | delta array, game time; wasOnFire before it | yes: ZERO. wasOnFire already written, all else unwritten | as D; wasOnFire not held, see row fire | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.20:608; 1.21:615 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | 1.20.5 feet slab, `!(y > 0.0)` (PlayerEntity:1070-1076, 1121-1127) | 1.20:614; 1.21:621 | local movement | - | sim:3441-3468 gate 1.20.5; P as B. NaN note of H holds for 1.20 - 1.20.4 | mapped / mapped |
| collide | helper | 1.20:615; 1.21:622 | none | - | as G | mapped / mapped |
| step-up | 1.20 - 1.20.5 swept and lifted; 1.21 candidate heights (body of move identical) | 1.20:615, 884; 1.21:622, 891 | none | - | gate 1.21 sim:3942; candidate_step_up col:584-638, candidate_step_heights col:640-669 | mapped / mapped |
| fall-reset clip | fallDistance != 0, d >= 1: clip FALLDAMAGE_RESETTING, Fluid.WATER over the whole move | 1.20:618; 1.21:625 | fallDistance 0 on hit | - | tick sim:5063-5086; P none | mapped / mapped |
| setPos | as G | 1.20:630; 1.21:637 | as G | - | as G | mapped / mapped |
| collision flags | adds verticalCollisionBelow (groundCollision) | 1.20:637; 1.21:644 | + verticalCollisionBelow | - | sim:5111; rest as G | mapped / mapped |
| onGround and support block | setOnGround(ground, movement) | 1.20:646; 1.21:653 | onGround, supportingBlockPos, forceUpdate flag | - | check_supporting_block sim:1904-1930, from sim:5113; P pis:860 | mapped / mapped |
| checkFallDamage | as E; getLandingPos via supporting block; HIT_GROUND event | 1.20:649; 1.21:656 | as E | - | on_position sim:1835-1852 gate 1.20 | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.20:650; 1.21:657 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | one write, by the approximate per-axis flags | 1.20:655; 1.21:662 | velocity | - | sim:5166-5167; P pis:973-974 | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | Block.onEntityLand if y differs | 1.20:660; 1.21:667 | velocity y | - | as A; 1.14+ multiplies by 0 (sim:5189) | mapped / mapped |
| stepOn (onSteppedOn) | `onGround` only; blocks test sneaking | 1.20:664; 1.21:671 | block-specific | - | as A; sculk sensor, shrieker not held | mapped (D25) / mapped (D25) |
| movement emission | MoveEffect gate; STEP, SWIM events | 1.20:668; 1.21:675 | - | - | none | no-op (D22) / no-op (D22) |
| walking effects | as A | 1.20:680; 1.21:687 | as A; 1.17 speed field | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | in move | 1.20:706; 1.21:713 | block-specific | - | as A. Inset 0.001 to 1.19.2, 1.0E-7 from 1.19.3 (sim:1666, 1.19.2 Entity:893, 1.19.3:901) | mapped / mapped |
| block speed factor | x, z scaled, after inside check; last in J | 1.20:707; 1.21:714 | velocity x, z | - | block_speed_factor sim:1730, sim:5231-5235; P pis:918-922 | mapped / mapped |
| fire | wasOnFire at top; 1.0E-6 box; reset; extinguish on powder snow or wet | 1.20:710; 1.21:717 | wasOnFire, fireTicks | - | apply_inside_fire_effects sim:2468-2507, after travel; wasOnFire not held; P pis:702 | mapped (D23) / mapped (D23) |
| movement recording | none | absent | - | - | none needed | mapped / mapped |
| multiple moves in one tick | each Entity.move runs its own inside check | absent | - | - | tick, P, riptide lift sim:4085-4091 each run theirs | mapped / mapped |

### Era J: inside blocks and fire leave move, side gates

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noClip)`: setPosition | 1.21.3:632; 1.21.4:640 | x/y/z, box refit | as A-F | as A-F | mapped / mapped |
| piston limit | as D, after wasOnFire write | 1.21.3:636, 891; 1.21.4:644, 903 | delta array, game time; wasOnFire before it | yes: ZERO. wasOnFire already written, all else unwritten | as D; wasOnFire not held, see row fire | n/a / mapped |
| stuck speed multiplier | multiplier > 1.0E-7: scale | 1.21.3:645; 1.21.4:653 | multiplier ZERO, motion ZERO | - | as A; native-only write state.move_slowed sim:5031 | mapped / mapped |
| edge backoff | as I (1.21.4 PlayerEntity:1073-1079) | 1.21.3:651; 1.21.4:659 | local movement | - | as I; P as B | mapped / mapped |
| collide | helper | 1.21.3:652; 1.21.4:660 | none | - | as G | mapped / mapped |
| step-up | candidate heights | 1.21.3:652, 932; 1.21.4:660, 944 | none | - | as I | mapped / mapped |
| fall-reset clip | as H | 1.21.3:655; 1.21.4:663 | as H | - | as H | mapped / mapped |
| setPos | test also `movement.lengthSq - d < 1.0E-7` | 1.21.3:667; 1.21.4:675 | as G | - | gate 1.21.2 sim:3374 (no source; 1.21 without, 1.21.3 with) | mapped / mapped |
| collision flags | 1.21.3 as H. 1.21.4 vertical pair only if `abs(y) > 0` or logical side | 1.21.3:674; 1.21.4:682 | as H; vertical pair may stay | - | gate 1.21.4 sim:5106-5108 `desired.y != 0.0` (NaN: D5, fixed); P pis:853-855 `abs() > 0.0` | mapped / mapped |
| onGround and support block | setMovement(ground, horizontal, movement); 1.21.4 inside the vertical gate | 1.21.3:683; 1.21.4:686 | as I, horizontalCollision again | - | as I, gate sim:5106 | mapped / mapped |
| checkFallDamage | 1.21.3 `!client or logical side`. 1.21.4 also `!isControlledByPlayer` (true for a player, 1.21.4 PlayerEntity:2214) | 1.21.3:687; 1.21.4:698 | as E | - | sim:4355-4359. Native runs it on a 1.21.4 server player, vanilla Entity.move does not | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.21.3:690; 1.21.4:701 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | as H | 1.21.3:695; 1.21.4:706 | velocity | - | as H | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | only on logical side | 1.21.3:701; 1.21.4:712 | velocity y | - | simulates_landing sim:4500-4502 | mapped / mapped |
| movement emission | also `!client or logical side` | 1.21.3:707; 1.21.4:718 | - | - | none | no-op (D22) / no-op (D22) |
| block speed factor | x, z scaled, after inside check; last in J | 1.21.3:712; 1.21.4:723 | velocity x, z | - | block_speed_factor sim:1730, sim:5231-5235; P pis:918-922 | mapped / mapped |
| movement recording | none in move; tickBlockCollision uses lastRenderPos -> pos (1.21.3:753, 1.21.4:764) | absent | - | - | movements_this_tick sim:2738-2739 whole tick; `old_position` is after the riptide lift (sim:4649, 4664) | mapped / mapped |
| multiple moves in one tick | move queues nothing; `queueBlockCollisionCheck` exists (1.21.3:1056) but no call in Entity.move | absent | - | - | whole-tick path sim:2738; P pis:662-666 | mapped / mapped |
| stepOn (onSteppedOn) | left move: tickBlockCollision, first step | 1.21.3:761; 1.21.4:772 | block-specific | - | sim:5338-5342 after travel, no side gate (vanilla 1.21.3 LivingEntity:2784); P pis:656-674 | mapped (D25) / mapped (D25) |
| walking effects | applyMoveEffect | 1.21.3:719; 1.21.4:730 | as A | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | left move: tickBlockCollision at end of tick | 1.21.3:765; 1.21.4:778 | block-specific | - | sim:5338-5345 after travel; no side gate; P pis:656-675 | mapped / mapped |
| fire | left move: tickBlockCollision, by blocks visited | 1.21.3:766; 1.21.4:779 | onFire, fireTicks | - | sim:2485-2506, sim:5351 at end of tick; onFire not held (sound only) | mapped / mapped |

### Era K: authoritative gates, per-axis Movement list

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noPhysics)`: setPos | 1.21.5:643 | position | as A-F | as A-F | mapped / mapped |
| piston limit | limitPistonMovement, then `equals(ZERO)` | 1.21.5:646, 941 | pistonDeltas, pistonDeltasGameTime | yes: ZERO. All below unwritten | as D | n/a / mapped |
| stuck speed multiplier | as D | 1.21.5:655 | as D | - | as D | mapped / mapped |
| edge backoff | Player.maybeBackOffFromEdge (26.3 Player:831-833), canFallAtLeast slab, double fallDistance | 1.21.5:661 | local movement | - | sim:3446-3458, 3472-3477 gate 1.21.5; P as B | mapped / mapped |
| collide | Entity.collide | 1.21.5:662 | none | - | as G | mapped / mapped |
| step-up | candidate heights in Entity.collide | 1.21.5:662, 982 | none | - | as I | mapped / mapped |
| fall-reset clip | as H, double fallDistance | 1.21.5:665 | as H | - | as H | mapped / mapped |
| movement recording | one Movement per non-zero axis in axisStepOrder(collided), as a list | 1.21.5:685 | movementThisTick | - | computed after the fact sim:5339, 2724-2760; split by axis block_traversal.rs:165-176. P: single Movement pis:667-671, as vanilla applyEffectsFromBlocks(from, to) | mapped / mapped |
| setPos | as J | 1.21.5:686 | position | - | as J | mapped / mapped |
| collision flags | gate on isLocalInstanceAuthoritative | 1.21.5:693 | as J | - | as J | mapped / mapped |
| onGround and support block | setOnGroundWithMovement inside the gate | 1.21.5:697 | as J | - | as J | mapped / mapped |
| checkFallDamage | isLocalInstanceAuthoritative; drop counted before landing, not in water | 1.21.5:709 | as E; lastClimbablePos cleared (LivingEntity) | - | sim:4355-4359, 4394-4398. Rounding and attribute damage by profile; P by release pis:866-874. lastClimbablePos, onChangedBlock, particles, HIT_GROUND not in the move | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.21.5:712 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | as H | 1.21.5:717 | deltaMovement | - | as H | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | only if canSimulateMovement | 1.21.5:723 | deltaMovement y | - | as J | mapped / mapped |
| movement emission | MovementEmission gate | 1.21.5:729 | - | - | none | no-op (D22) / no-op (D22) |
| block speed factor | as J | 1.21.5:734 | deltaMovement x, z | - | as E | mapped / mapped |
| multiple moves in one tick | every Entity.move of the tick adds to movementThisTick; applyEffectsFromBlocks takes all of them and clears. `applyEffectsFromBlocks(from, to)` of a piston does not clear them | absent | - | - | tick list: a held shulker box push (`pending_movement`), the riptide lift, the tick move and the remainder (`movements_this_tick`). A piston's move is taken back by the piston from 1.21.5. Minecart holds its own list (minecart.rs) | fixed (D17) / fixed (D17) |
| stepOn (onSteppedOn) | applyEffectsFromBlocks, first step | 1.21.5:802 | block-specific | - | as J (26.3 LivingEntity:3064) | mapped (D25) / mapped (D25) |
| walking effects | applyMovementEmissionAndPlaySound | 1.21.5:741 | moveDist, flyDist, nextStep | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | applyEffectsFromBlocks at end of tick | 1.21.5:807 | block-specific, effect steps | - | blocks_inside_moves sim:2569; no side gate; P pis:656-675 | mapped / mapped |
| fire | applyEffectsFromBlocks | 1.21.5:809 | fireTicks, freeze | - | sim:2509-2555; gate 1.21.6 sim:2545 | mapped / mapped |

### Era L: one Movement per move

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noPhysics)`: setPos | 1.21.6:670; 1.21.8:670 | position | as A-F | as A-F | mapped / mapped |
| piston limit | limitPistonMovement, then `equals(ZERO)` | 1.21.6:673, 975; 1.21.8:673, 975 | pistonDeltas, pistonDeltasGameTime | yes: ZERO. All below unwritten | as D | n/a / mapped |
| stuck speed multiplier | as D | 1.21.6:682; 1.21.8:682 | as D | - | as D | mapped / mapped |
| edge backoff | Player.maybeBackOffFromEdge (26.3 Player:831-833), canFallAtLeast slab, double fallDistance | 1.21.6:688; 1.21.8:688 | local movement | - | sim:3446-3458, 3472-3477 gate 1.21.5; P as B | mapped / mapped |
| collide | Entity.collide | 1.21.6:689; 1.21.8:689 | none | - | as G | mapped / mapped |
| step-up | candidate heights in Entity.collide | 1.21.6:689, 1016; 1.21.8:689, 1016 | none | - | as I | mapped / mapped |
| fall-reset clip | as H, double fallDistance | 1.21.6:692; 1.21.8:692 | as H | - | as H | mapped / mapped |
| movement recording | one Movement, axisIndependant true; cap 100 (1.21.6:804-813) | 1.21.6:702; 1.21.8:702 | movementThisTick | - | as K, axis order from moved sim:2745; cap not modelled | mapped (D18) / mapped (D18) |
| setPos | as J | 1.21.6:703; 1.21.8:703 | position | - | as J | mapped / mapped |
| collision flags | gate on isLocalInstanceAuthoritative | 1.21.6:710; 1.21.8:710 | as J | - | as J | mapped / mapped |
| onGround and support block | setOnGroundWithMovement inside the gate | 1.21.6:714; 1.21.8:714 | as J | - | as J | mapped / mapped |
| checkFallDamage | isLocalInstanceAuthoritative; drop counted before landing, not in water | 1.21.6:726; 1.21.8:726 | as E; lastClimbablePos cleared (LivingEntity) | - | sim:4355-4359, 4394-4398. Rounding and attribute damage by profile; P by release pis:866-874. lastClimbablePos, onChangedBlock, particles, HIT_GROUND not in the move | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.21.6:729; 1.21.8:729 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | as H | 1.21.6:734; 1.21.8:734 | deltaMovement | - | as H | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | only if canSimulateMovement | 1.21.6:740; 1.21.8:740 | deltaMovement y | - | as J | mapped / mapped |
| movement emission | MovementEmission gate | 1.21.6:746; 1.21.8:746 | - | - | none | no-op (D22) / no-op (D22) |
| block speed factor | as J | 1.21.6:751; 1.21.8:751 | deltaMovement x, z | - | as E | mapped / mapped |
| multiple moves in one tick | every Entity.move of the tick adds to movementThisTick; applyEffectsFromBlocks takes all of them and clears. `applyEffectsFromBlocks(from, to)` of a piston does not clear them | absent | - | - | tick list: a held shulker box push (`pending_movement`), the riptide lift, the tick move and the remainder (`movements_this_tick`). A piston's move is taken back by the piston from 1.21.5. Minecart holds its own list (minecart.rs) | fixed (D17) / fixed (D17) |
| stepOn (onSteppedOn) | applyEffectsFromBlocks, first step | 1.21.6:834; 1.21.8:834 | block-specific | - | as J (26.3 LivingEntity:3064) | mapped (D25) / mapped (D25) |
| walking effects | applyMovementEmissionAndPlaySound | 1.21.6:758; 1.21.8:758 | moveDist, flyDist, nextStep | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | applyEffectsFromBlocks at end of tick | 1.21.6:840; 1.21.8:840 | block-specific, effect steps | - | blocks_inside_moves sim:2569; no side gate; P pis:656-675 | mapped / mapped |
| fire | applyEffectsFromBlocks | 1.21.6:842; 1.21.8:842 | fireTicks, freeze | - | sim:2509-2555; gate 1.21.6 sim:2545 | mapped / mapped |

### Era M: noPhysics clears flags, clip cap, requested delta in Movement

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noPhysics)`: setPos, four flags false | 1.21.9:677; 1.21.11:692 | position, horizontalCollision, verticalCollision, verticalCollisionBelow, minorHorizontalCollision | as A-F, but the four flags are written | tick sim:5115-5119 (gate 1.21.9); P pis:790-795 | mapped / mapped |
| piston limit | limitPistonMovement, then `equals(ZERO)` | 1.21.9:684, 988; 1.21.11:699, 1010 | pistonDeltas, pistonDeltasGameTime | yes: ZERO. All below unwritten | as D | n/a / mapped |
| stuck speed multiplier | as D; from 1.21.10 not scaled for PISTON, still used up (1.21.9 scales) | 1.21.9:693; 1.21.11:708 | as D | - | P pis:824-828 gate 1.21.10 | mapped / mapped |
| edge backoff | Player.maybeBackOffFromEdge (26.3 Player:831-833), canFallAtLeast slab, double fallDistance | 1.21.9:699; 1.21.11:717 | local movement | - | sim:3446-3458, 3472-3477 gate 1.21.5; P as B | mapped / mapped |
| collide | Entity.collide | 1.21.9:700; 1.21.11:718 | none | - | as G | mapped / mapped |
| step-up | candidate heights in Entity.collide | 1.21.9:700, 1036; 1.21.11:718, 1058 | none | - | as I | mapped / mapped |
| fall-reset clip | clip capped at min(length, 8) | 1.21.9:703; 1.21.11:721 | as H | - | gate 1.21.9 sim:5068-5076; P none | mapped / mapped |
| movement recording | one Movement carrying the requested delta; cap 100 | 1.21.9:715; 1.21.11:733 | movementThisTick | - | gate 1.21.9 sim:2742-2743; cap not modelled | mapped (D18) / mapped (D18) |
| setPos | as J | 1.21.9:716; 1.21.11:734 | position | - | as J | mapped / mapped |
| collision flags | gate on isLocalInstanceAuthoritative | 1.21.9:723; 1.21.11:741 | as J | - | as J | mapped / mapped |
| onGround and support block | setOnGroundWithMovement inside the gate | 1.21.9:727; 1.21.11:745 | as J | - | as J | mapped / mapped |
| checkFallDamage | isLocalInstanceAuthoritative; drop counted before landing, not in water | 1.21.9:739; 1.21.11:757 | as E; lastClimbablePos cleared (LivingEntity) | - | sim:4355-4359, 4394-4398. Rounding and attribute damage by profile; P by release pis:866-874. lastClimbablePos, onChangedBlock, particles, HIT_GROUND not in the move | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 1.21.9:742; 1.21.11:760 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | as H | 1.21.9:747; 1.21.11:765 | deltaMovement | - | as H | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | only if canSimulateMovement | 1.21.9:753; 1.21.11:771 | deltaMovement y | - | as J | mapped / mapped |
| movement emission | MovementEmission gate | 1.21.9:759; 1.21.11:777 | - | - | none | no-op (D22) / no-op (D22) |
| block speed factor | as J | 1.21.9:764; 1.21.11:782 | deltaMovement x, z | - | as E | mapped / mapped |
| multiple moves in one tick | every Entity.move of the tick adds to movementThisTick; applyEffectsFromBlocks takes all of them and clears. `applyEffectsFromBlocks(from, to)` of a piston does not clear them | absent | - | - | tick list: a held shulker box push (`pending_movement`), the riptide lift, the tick move and the remainder (`movements_this_tick`). A piston's move is taken back by the piston from 1.21.5. Minecart holds its own list (minecart.rs) | fixed (D17) / fixed (D17) |
| stepOn (onSteppedOn) | applyEffectsFromBlocks, first step | 1.21.9:847; 1.21.11:869 | block-specific | - | as J (26.3 LivingEntity:3064) | mapped (D25) / mapped (D25) |
| walking effects | applyMovementEmissionAndPlaySound | 1.21.9:771; 1.21.11:789 | moveDist, flyDist, nextStep | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | applyEffectsFromBlocks at end of tick | 1.21.9:853; 1.21.11:875 | block-specific, effect steps | - | blocks_inside_moves sim:2569; no side gate; P pis:656-675 | mapped / mapped |
| fire | applyEffectsFromBlocks | 1.21.9:855; 1.21.11:877 | fireTicks, freeze | - | sim:2509-2555; gate 1.21.6 sim:2545 | mapped / mapped |

### Era N: restitution

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noPhysics)`: setPos, four flags false | 26.2:744 | position, horizontalCollision, verticalCollision, verticalCollisionBelow, minorHorizontalCollision | as A-F, but the four flags are written | tick sim:5115-5119 (gate 1.21.9); P pis:790-795 | mapped / mapped |
| piston limit | limitPistonMovement, then `equals(ZERO)` | 26.2:751, 1114 | pistonDeltas, pistonDeltasGameTime | yes: ZERO. All below unwritten | as D | n/a / mapped |
| stuck speed multiplier | as D; from 1.21.10 not scaled for PISTON, still used up (1.21.9 scales) | 26.2:760 | as D | - | P pis:824-828 gate 1.21.10 | mapped / mapped |
| edge backoff | Player.maybeBackOffFromEdge (26.3 Player:831-833), canFallAtLeast slab, double fallDistance | 26.2:769 | local movement | - | sim:3446-3458, 3472-3477 gate 1.21.5; P as B | mapped / mapped |
| collide | Entity.collide | 26.2:770 | none | - | as G | mapped / mapped |
| step-up | candidate heights in Entity.collide | 26.2:770, 1162 | none | - | as I | mapped / mapped |
| fall-reset clip | clip capped at min(length, 8) | 26.2:773 | as H | - | gate 1.21.9 sim:5068-5076; P none | mapped / mapped |
| movement recording | one Movement carrying the requested delta; cap 100 | 26.2:784 | movementThisTick | - | gate 1.21.9 sim:2742-2743; cap not modelled | mapped (D18) / mapped (D18) |
| setPos | as J | 26.2:785 | position | - | as J | mapped / mapped |
| collision flags | gate on isLocalInstanceAuthoritative | 26.2:792 | as J | - | as J | mapped / mapped |
| onGround and support block | setOnGroundWithMovement inside the gate | 26.2:797 | as J | - | as J | mapped / mapped |
| checkFallDamage | isLocalInstanceAuthoritative; drop counted before landing, not in water | 26.2:809 | as E; lastClimbablePos cleared (LivingEntity) | - | sim:4355-4359, 4394-4398. Rounding and attribute damage by profile; P by release pis:866-874. lastClimbablePos, onChangedBlock, particles, HIT_GROUND not in the move | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 26.2:812 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | replaced by restituteMovementAfterCollisions | 26.2:816, 833 | deltaMovement | - | restitute_movement_after_collisions sim:3761-3835; tick sim:5152-5164 omits canSimulateMovement, P pis:892-904 has it | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | no block hook; restitution; `<` gravity; BOUNCE event, syncPosition | 26.2:816, 867 | deltaMovement, syncPosition | - | sim:3794-3831, gate 26.3 sim:3798; event and syncPosition none; tick passes y_collision sim:5161 | mapped / mapped |
| movement emission | MovementEmission gate | 26.2:821 | - | - | none | no-op (D22) / no-op (D22) |
| block speed factor | as J | 26.2:826 | deltaMovement x, z | - | as E | mapped / mapped |
| multiple moves in one tick | every Entity.move of the tick adds to movementThisTick; applyEffectsFromBlocks takes all of them and clears. `applyEffectsFromBlocks(from, to)` of a piston does not clear them | absent | - | - | tick list: a held shulker box push (`pending_movement`), the riptide lift, the tick move and the remainder (`movements_this_tick`). A piston's move is taken back by the piston from 1.21.5. Minecart holds its own list (minecart.rs) | fixed (D17) / fixed (D17) |
| stepOn (onSteppedOn) | applyEffectsFromBlocks, first step | 26.2:981 | block-specific | - | as J (26.3 LivingEntity:3064) | mapped (D25) / mapped (D25) |
| walking effects | applyMovementEmissionAndPlaySound | 26.2:895 | moveDist, flyDist, nextStep | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | applyEffectsFromBlocks at end of tick | 26.2:987 | block-specific, effect steps | - | blocks_inside_moves sim:2569; no side gate; P pis:656-675 | mapped / mapped |
| fire | applyEffectsFromBlocks | 26.2:989 | fireTicks, freeze | - | sim:2509-2555; gate 1.21.6 sim:2545 | mapped / mapped |

### Era O: restitution, recordMovement, IDE guard

| Phase | Vanilla | Line | State written | Early return, what stays unwritten | Native, gate | tick / P |
| --- | --- | --- | --- | --- | --- | --- |
| noPhysics | `if (noPhysics)`: setPos, four flags false | 26.3:766 | position, horizontalCollision, verticalCollision, verticalCollisionBelow, minorHorizontalCollision | as A-F, but the four flags are written | tick sim:5115-5119 (gate 1.21.9); P pis:790-795 | mapped / mapped |
| piston limit | limitPistonMovement, then `equals(ZERO)` | 26.3:774, 1145 | pistonDeltas, pistonDeltasGameTime | yes: ZERO. All below unwritten | as D | n/a / mapped |
| stuck speed multiplier | as D; from 1.21.10 not scaled for PISTON, still used up (1.21.9 scales) | 26.3:783 | as D | - | P pis:824-828 gate 1.21.10 | mapped / mapped |
| edge backoff | Player.maybeBackOffFromEdge (26.3 Player:831-833), canFallAtLeast slab, double fallDistance | 26.3:792 | local movement | - | sim:3446-3458, 3472-3477 gate 1.21.5; P as B | mapped / mapped |
| collide | entity query also grows by maxUpStep | 26.3:793, 1187 | none | - | entity_collision_query gate 26.3 sim:2841-2844 | mapped / mapped |
| step-up | candidate heights in Entity.collide | 26.3:793, 1193 | none | - | as I | mapped / mapped |
| fall-reset clip | clip capped at min(length, 8) | 26.3:796 | as H | - | gate 1.21.9 sim:5068-5076; P none | mapped / mapped |
| movement recording | as M; plus recordMovement in both branches (26.3:768, 857) | 26.3:807 | movementThisTick; interpolationHandler predicted movement | - | as M; recordMovement none: no engine output (D19) | no-op (D19) / no-op (D19) |
| setPos | as J | 26.3:808 | position | - | as J | mapped / mapped |
| collision flags | gate on isLocalInstanceAuthoritative | 26.3:816 | as J | - | as J | mapped / mapped |
| onGround and support block | setOnGroundWithMovement inside the gate | 26.3:821 | as J | - | as J | mapped / mapped |
| checkFallDamage | isLocalInstanceAuthoritative; drop counted before landing, not in water | 26.3:833 | as E; lastClimbablePos cleared (LivingEntity) | - | sim:4355-4359, 4394-4398. Rounding and attribute damage by profile; P by release pis:866-874. lastClimbablePos, onChangedBlock, particles, HIT_GROUND not in the move | mapped (D16, D26) / mapped (D16, D26) |
| isRemoved exit | after fall: pop and stop | 26.3:836 | - | yes: motion stop, callback, stepOn, emission, inside, speed, fire unwritten | tick sim:5144, 5152, 5165, 5223, 5231, 5350; `removed` is an input flag only (napi_player.rs:498), and the tick returns before all this from 1.19.1 (sim:4571). P no test | mapped / mapped |
| velocity zeroing | replaced by restituteMovementAfterCollisions | 26.3:840, 863 | deltaMovement | - | restitute_movement_after_collisions sim:3761-3835; tick sim:5152-5164 omits canSimulateMovement, P pis:892-904 has it | mapped / mapped |
| landing callback (afterFallOn, updateEntityAfterFallOn, bounce) | as N, `<=` gravity; Block.bounceOn (ShelfMushroomBlock override) | 26.3:840, 897 | as N | - | as N; bounceOn none | mapped / mapped |
| movement emission | MovementEmission gate | 26.3:845 | - | - | none | no-op (D22) / no-op (D22) |
| block speed factor | as J | 26.3:850 | deltaMovement x, z | - | as E | mapped / mapped |
| multiple moves in one tick | every Entity.move of the tick adds to movementThisTick; applyEffectsFromBlocks takes all of them and clears. `applyEffectsFromBlocks(from, to)` of a piston does not clear them | absent | - | - | tick list: a held shulker box push (`pending_movement`), the riptide lift, the tick move and the remainder (`movements_this_tick`). A piston's move is taken back by the piston from 1.21.5. Minecart holds its own list (minecart.rs) | fixed (D17) / fixed (D17) |
| stepOn (onSteppedOn) | applyEffectsFromBlocks, first step | 26.3:1012 | block-specific | - | as J (26.3 LivingEntity:3064) | mapped (D25) / mapped (D25) |
| walking effects | applyMovementEmissionAndPlaySound | 26.3:926 | moveDist, flyDist, nextStep | - | none | no-op (D21) / no-op (D21) |
| checkInsideBlocks timing | applyEffectsFromBlocks at end of tick | 26.3:1018 | block-specific, effect steps | - | blocks_inside_moves sim:2569; no side gate; P pis:656-675 | mapped / mapped |
| fire | applyEffectsFromBlocks | 26.3:1020 | fireTicks, freeze | - | sim:2509-2555; gate 1.21.6 sim:2545 | mapped / mapped |

## 3. Items and their resolution

### One move for the player, shared parts for every entity

The player's Entity.move is `entity_move` (simulation.rs), called by the tick (`tick_with_rules_and_rounding_and_restitution`), by `launch_riptide` for the riptide lift, and by `move_entity` (piston.rs) for a piston's push, a shulker box's push and the pre-1.11 moveEntity of a piston. It takes the `MoverType` (`SelfOrPlayer`, `Piston`, `ShulkerBox`) and runs, in vanilla's order: the stuck speed (not a PISTON move's from 1.21.10), the size guard, the edge back-off of SELF and PLAYER, noPhysics (`move_without_physics`), the collision, the fall-reset clip (`reset_fall_on_clip`), the position, the flags with the 1.21.4 gate (`refreshes_vertical_collision`) and the local player's minor collision, the support, `check_fall_damage`, the isRemoved exit, restitution from 26.2 (where `can_simulate_movement`, reading the field verticalCollision) or `stop_blocked_motion` before, the inLava of 1.14 - 1.16.1, checkInsideBlocks before 1.21.2 and the speed factor from 1.15. Each caller adds only what vanilla adds around Entity.move: the tick its travel, auto jump (after the move, as LocalPlayer.move) and fire; `move_entity` the piston move's fire and powder snow and the auto jump.

Every other entity's move is `run_vehicle_move` (vehicle.rs), which keeps its own collision (`collide_vehicle`, `legacy_collide`: entity colliders, MinecartCollisionContext, no back-off) but takes `reset_fall_on_clip` and `refreshes_vertical_collision` from the same code, and with `MoveRules::counts_fall` checkFallDamage at its place in the move (items, orbs, TNT, falling blocks, a piston's non-player body). Its callers take `move_without_physics` for noPhysics (`move_body` of piston.rs, moving_entity.rs).

### List

| Id | Resolution | What | Native now | Vanilla |
| --- | --- | --- | --- | --- |
| D1 | fixed | A shulker box's push backed off from edges. `MoverType::ShulkerBox` takes no back-off; pre-1.11 piston calls take `SelfOrPlayer` | `entity_move`, shulker_box.rs `move_entity(.., MoverType::ShulkerBox)`; test `a_shulker_box_push_does_not_back_off_from_an_edge` | 1.11.2:611; 26.3 Player.java:831-833 |
| D2 | fixed | Fall-reset clip missing in piston moves and every other entity's move | `reset_fall_on_clip` in `entity_move` and `run_vehicle_move`; test `every_move_of_a_block_or_more_resets_a_fall_through_a_resetting_block` | 1.18.2 Entity.java (clip first; 1.18.1 none), 1.21.9:703-711, 26.3:776-784 |
| D3 | fixed | The tick restituted without canSimulateMovement | `entity_move`; test `the_tick_restitutes_only_where_the_instance_simulates` | 26.2:815, 26.3:820 |
| D4 | fixed | The tick passed this move's y collision, not the field verticalCollision | `entity_move` passes `state.collided_vertically`; test `restitution_reads_the_kept_vertical_collision` | 26.2:846, 26.3:857 |
| D5 | fixed | `!= 0.0` true for NaN where vanilla `Math.abs(y) > 0.0` is false (vertical gate, 26.2 fall callback) | `refreshes_vertical_collision`, `block_fall_callback_runs`; test `a_nan_vertical_move_refreshes_nothing` | 1.21.4:683, 26.2:798 |
| D6 | fixed | Other entities' moves refreshed the vertical flags on every move from 1.21.4; a projectile kept its ground by `== 0.0` | `run_vehicle_move` (move_body's restore dropped), projectile.rs `!(y.abs() > 0.0)`; test `another_entitys_level_move_keeps_its_ground_where_it_does_not_control_it` | 1.21.4:682-686, 26.3:798-803 |
| D7 | fixed | move_body's noPhysics kept the flags from 1.21.9 | `move_without_physics` in every noPhysics branch; test `a_move_without_physics_clears_the_flags_from_1_21_9` | 1.21.9:677-684 |
| D8 | fixed | No isRemoved exit in a piston's move | `entity_move` (shared). Unreachable for pistons in vanilla too: a removed entity is out of its section, so the piston's getEntities never finds it | 1.17:568, 26.3:817 |
| D9 | mapped | End-of-tick block effects: the side gate is `applies_effects_from_blocks` (client only for what it controls). Removed cannot reach it: the tick returns for a removed state from 1.19.1, before all 1.21.2+ code | simulation.rs `applies_effects_from_blocks` | 1.21.3 LivingEntity.java:2784-2785, 26.3 LivingEntity.java:3064-3065 |
| D10 | mapped | Fire runs after travel up to 1.21.1. Its inputs (the blocks the move left the box in, in_water, in_rain, the block at the feet, the fire ticks) are not written by the travel that follows the move, which writes only velocity, fall distance, the glide and flight flags | tick, `apply_inside_fire_effects` | 1.8.9:710, 1.21:717 |
| D11 | fixed (before B083's port) | Wet test: `is_wet` holds water, rain and bubble columns | simulation.rs `is_wet` | 1.8.9:829-832, 1.15:1003-1004 |
| D12 | no-op | burn(1): the engine models no health. The damage source inFire has no attacker, so no knockback (EntityLivingBase.attackEntityFrom knocks back only from an entity source) | none | 1.8.9:711, 1.15:639 |
| D13 | fixed (before B083's port) | Fire ticks rise under fire immunity as vanilla does: `legacy_inside_fire` has no immunity test | simulation.rs `legacy_inside_fire` | 1.8.9:713, 1.15:633 |
| D14 | fixed for the player; mapped for others | A piston's or shulker box's move before 1.17 now runs the move's fire test. Minecarts hold their own (minecart.rs fire test); items, orbs, TNT, falling blocks, boats and mobs keep no fire ticks in their states, so the test reaches no output of theirs | piston.rs `apply_powder_snow`; test `a_piston_move_into_fire_raises_the_fire_ticks_before_1_17` | 1.12.2:876-886, 1.15.2:631-653 |
| D15 | fixed | Minor horizontal collision only for the local player (client side, controlled) | `minor_horizontal_collision`; test `only_the_local_player_has_a_minor_horizontal_collision` | 1.18 LocalPlayer.java (override), 26.3 Entity.java:805-809 |
| D16 | mapped | 1.21.4 server player: `check_fall_damage` stands for ServerPlayer.doCheckFallDamage of the received move, which calls the same checkFallDamage (the native comment at `check_fall_damage` cites 1.21.4 ServerPlayNetworkHandler.java:999, 1018); the client skips it as vanilla | `check_fall_damage` | 1.21.4:697, 1.21.4 PlayerEntity.java:2214 |
| D17 | fixed | The riptide lift's Movement is first in the tick's list (TridentItem.releaseUsing runs before the tick sets its old position). A shulker box's push is an Entity.move that records its Movement and applies nothing (26.3 ShulkerBoxBlockEntity.java:133), so it stays in movementThisTick until the entity's next applyEffectsFromBlocks: `move_entity` holds it in `SimulationState::pending_movements` (from 1.21.5; 1.21.4 records no move), the state carries it through JS as `pendingMovements`, and the next tick's list takes it first. A piston's push does not stay: from 1.21.5 the piston applies its own move and takes it back (1.21.5 PistonMovingBlockEntity.java:182-183, 26.3 :167-168), and 1.21.2 - 1.21.4 apply the move without recording it (1.21.4 Entity.java:788-802). A second push before the tick is a batch of its own after the first (1.21.5 Entity.java:720, 811-815), up to four, past which the first two join as 26.3 joins past its 100 (26.3 :977-985). | `recorded_movement`, `hold_pending_movement` (piston.rs), tick; tests `a_recorded_move_is_one_movement_from_1_21_5`, `a_shulker_box_push_waits_in_the_movement_list_for_the_next_tick_from_1_21_5` | 1.21.5:685, 26.3:788, 807, 946-972; 26.3 ShulkerBoxBlockEntity.java:133, PistonMovingBlockEntity.java:165-169 |
| D18 | mapped | addMovementThisTick's cap of 100. The engine's list holds at most three entries a tick (the lift, the own move, the remainder); the cap needs 99 moves before one applyEffectsFromBlocks | none needed | 1.21.6:804-813, 26.3:963-972 |
| D19 | no-op (no engine output) | 26.3 recordMovement: a PLAYER, PISTON, SHULKER_BOX or SHULKER move calls InterpolationHandler.applyPredictedMovement. A LocalPlayer's handler is the no-op one (26.3 InterpolationHandler.java:26-51), so the engine's player has nothing to shift. On a client, an interpolated entity's AbstractInterpolationHandler shifts its target by the move where the shifted box is free and shifts its last position with it (26.3 AbstractInterpolationHandler.java:45-48, 60-66); 1.21.5 - 26.2 come to the same shift at the next interpolate, from the position's change since the last one (26.2 InterpolationHandler.interpolate, previousTickPosition), so 26.3 changes only when the shift and its collision test happen. On a server it adds to SteppedInterpolationTracker.predictedDelta, which shifts the position steps sent to clients (26.3 SteppedInterpolationTracker.java:23-25, 35-40), never the entity. The only interpolated entity the engine keeps is a client's minecart (`MinecartClientLerp`, the bot's MinecartTracker), and nothing in the engine or the bot pushes one by a piston or shulker box: the stepwise replay pushes the client's cart after its interpolation by position alone and seeds the next tick's target from the recording, which already holds the shifted target. Implementing it needs a client piston push of an interpolated cart in the engine first, with the 1.21.5 - 26.2 previousTickPosition shift beside it | none | 26.3:768, 857-861; 26.3 MoverType.java:13-15, AbstractInterpolationHandler.java:45-66, SteppedInterpolationTracker.java:23-40; 26.2 InterpolationHandler.java interpolate |
| D20 | mapped (unreachable) | The IDE guard: SharedConstants.IS_RUNNING_IN_IDE is a static boolean no client class assigns (26.3 SharedConstants.java:140), so it stays false in the game | none needed | 26.3:740-744 |
| D21 | no-op | Walking effects: moveDist, flyDist, nextStep, walkDist (26.3 LocalPlayer addWalkedDistance), step, swim and flap sounds. Read by sounds, game events, view bobbing and statistics only. The 1.8 - 1.12 swim sound draws the entity's random, which no engine path of the player reads | none | 1.8.9:680, 26.3:909-944 |
| D22 | no-op | Movement emission gate, STEP and SWIM game events | none | 1.17:590, 26.3:824-829 |
| D23 | no-op | wasOnFire, onFire and the extinguish sound: wasOnFire is read only by the extinguish sound | none | 1.17:539, 1.21.3:635 |
| D24 | mapped | `collided` (1.8 - 1.15): its only reader is FireworkRocketEntity.performHitChecks of 1.14 - 1.15 (1.15 FireworkRocketEntity.java:229), which projectile.rs `tick_firework` decides from its own move's flags | projectile.rs `tick_firework` | 1.15:572 |
| D25 | mapped (slime, magma); no-op (rest) | stepOn of redstone ore (lit state, same shape), turtle egg (server, random break), sculk sensor and shrieker (game events), stairs (forward to a base block with no stepOn effect) | `apply_block_effects` | 1.8.9, 1.14, 1.19 block trees; 26.3 stepOn overrides |
| D26 | no-op | checkFallDamage side effects: farmland and turtle egg block changes (server only, random), HIT_GROUND, landing particles, soul speed (server only; the attribute reaches the engine as input), onChangedBlock, lastClimbablePos (death message) | `check_fall_damage` | 1.21:1183, 1.16.5 LivingEntity.java:503-518, 26.3 LivingEntity.java:325-360 |
| D27 | no-op | GameEvent.BOUNCE, syncPosition (a network flag), 26.3 Block.bounceOn, whose one override plays a sound (26.3 ShelfMushroomBlock.java:69-74) | none | 26.2:866-869, 26.3:879-883 |
| D28 | fixed | 1.8.8 ends a landed fall only when above 0 (exact 1.8.8 tree) | `resets_landed_fall`; test `a_negative_fall_distance_outlives_a_landing_before_1_9` | 1.8.8 Entity.java:790-800; 1.9.4 :891-897 |
| D29 | mapped | 1.14.4 has the single-axis split (1.14.4 Entity.java:725-731), so the gate at 1.14 holds for the registered 1.14.4 | collide_with_world | 1.14.4, 1.15:775 |
| D30 | fixed | Auto jump ran before the block effects and speed factor; now after the whole move, as LocalPlayer.move | tick | 1.20.4 ClientPlayerEntity.java:870-871, 26.3 LocalPlayer.java:980-983 |
| D31 | no-op | LocalPlayer.move addWalkedDistance: walkDist, view bobbing | none | 26.3 LocalPlayer.java:984 |
| D32 | fixed | The slime block's bounce now comes after the isRemoved exit; the fall still ends in checkFallDamage | `check_fall_damage`; test `a_removed_entity_lands_without_the_bounce_or_the_speed_factor` | 1.17:567-582, 1.21.11:757-771 |
| D33 | fixed | Other entities counted the fall after the motion stop and the blocks inside; now in the move, before them (`counts_fall`), and the cobweb post-hoc reset is gone | `run_vehicle_move`; test `a_cobweb_ends_the_fall_the_move_counted_first` | 1.20.4:757-764, 26.3:811-815 |
| D34 | fixed | 1.19 - 1.20.4 back-off skips a NaN y | `back_off_from_edge`; test `a_nan_vertical_move_backs_off_from_1_20_5_only` | 1.20.4 Player.java:1047, 1.20.5 :1029 |
| D35 | mapped (engine guard) | A move whose swept box covers more than MAX_WALKED_CELLS cells is not run and `move_out_of_range` makes the bindings refuse the tick: no result is reported to diverge. Now in `entity_move` for every player move | `entity_move`, `run_vehicle_move` | none |

## 4. Releases with no exact source

None now. Every modern registered release has its own tree in `E:\mc-physics-demo-wt\source-cache` and `E:\mc-physics-demo-wt\compare-cache` (section 1 places each by its own `move`), and 1.8.8 has `E:\mc-physics-demo-wt\legacy-src\1.8.8`. Trees 1.9.4 to 1.13.2 are `E:\mc-physics-demo-wt\legacy-src\<v>` and the mc-decompiled trees of the same name.

## 5. Checked and not checked

Checked: cargo tests (`entity_move_tests.rs`, 14 tests for the items above; the bit-exact OracleMove fixtures of `oracle_move_tests.rs` through the shared move) and replays of the entity, vehicle and move families on eleven releases (B083's lane report gives the counts).

Not checked:

- Voxel traversal: `collide_over_grid`, `Collider`, `legacy_colliders` ordering, `blocks_intersected_between`, `shape_meets_query`.
- Block.onEntityCollision bodies past those named, `apply_inside_block_effects`, `apply_inside_block_signals`.
- `onLandedUpon` damage values, attribute damage, `accumulated_fall_distance`.
- Entity subclasses that override `move` (Shulker, AbstractArrow, AbstractMinecart, BlockAttachedEntity, ItemFrame) and `checkFallDamage` (ServerPlayer, Bat, Allay, Bee, HappyGhast, Parrot, Ghast, Phantom, Strider, AbstractBoat) are their families' code, not this map's.
- Releases with a tree but no recording here: 1.16.3 - 1.19 have no recordings in the final set.
