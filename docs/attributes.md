# Movement attributes

`test/fixtures/attributes.tsv` holds what the jars of all 50 registered releases register:
`scripts/java-oracle/OracleAttributes.java` reads 1.16 on from each jar's own registry and `DefaultAttributes`, with the key,
default, range, player base and zombie base, and `OracleAttributesLegacy.java` reads 1.8.8 to 1.15 from
`SharedMonsterAttributes`, with each entity's own `registerAttributes` for the bases.

Mojang published no names before 1.14. There the jar is the release's own client, by its sha1, remapped with
`legacy-mappings/<release>.tiny`, the mapping the recorder builds on (the recorder's `mod/legacy-mappings.py`). The names are
1.14.4's carried across, but the values come from running that release's bytecode. Each row of 1.9.4 to 1.13.2 was also
compared with `EntityAttributes.java` in `E:\pr\mc-decompiled`; 1.8.8 has no tree, so its rows have the jar alone.
`test/attributes.test.cjs` compares minecraft-data and the adapter's fallbacks against it.

| attribute | from | player base | consumer | absent means |
|---|---|---|---|---|
| `movement_speed` | always | 0.1F | `movementSpeedValue`, `travel`'s speed | `movementSpeed` with the effect amplifiers |
| `scale` | 1.20.5 | 1 | the adapter's dimensions and eyes | 1 |
| `gravity` | 1.20.5 | 0.08 | `gravity`, sanitized -1..1 | the release's rule |
| `step_height` | 1.20.5 | 0.6 | `stepHeight`, sanitized 0..10 | 0.6 |
| `jump_strength` | 1.20.5 on a player | 0.42F | `jumpStrength` | 0.42F |
| `safe_fall_distance` | 1.20.5 | 3 | fall damage | 3 plus Jump Boost |
| `fall_damage_multiplier` | 1.20.5 | 1 | fall damage | 1 |
| `sneaking_speed` | 1.21 | 0.3 | `sneakingSpeed` | 0.3 and Swift Sneak |
| `burning_time` | 1.21 | 1 | `burningTime`, fire ticks | 1 |
| `movement_efficiency` | 1.21 | 0 | `movementEfficiency` | 0 |
| `water_movement_efficiency` | 1.21 | 0 | `waterMovementEfficiency` | Depth Strider |
| `air_drag_modifier` | 26.2 | 1 | `airDragModifier` | 1 |
| `friction_modifier` | 26.2 | 1 | `frictionModifier` | 1 |
| `bounciness` | 26.2 | 0 | `entityBounciness` | 0 |

Before 1.16 the keys are `generic.movementSpeed` and the like, and only `movement_speed` is consumed.
`attack_knockback` first registers at 1.14, `flying_speed` at 1.12, and `attack_speed`, `armor`, `armor_toughness` and `luck`
at 1.9.

## Accepted without a movement consumer

These are registered and synced, and move nothing. The adapter does not read them, so no field pretends otherwise:
`armor`, `armor_toughness`, `attack_damage`, `attack_speed`, `block_break_speed`, `block_interaction_range`,
`entity_interaction_range`, `follow_range`, `luck`, `max_health`, `max_absorption`, `oxygen_bonus`, `spawn_reinforcements`,
`submerged_mining_speed`, `sweeping_damage_ratio`, `tempt_range`, `waypoint_*`, `camera_*`.

`attack_knockback` reaches movement through one event: a spear stab, `stabHit`, carrying the weapon's knockback. Knockback
to other entities is the server's and arrives as motion.

## Generic and entity defaults differ

The registered default of `movement_speed` is 0.7, a player's base is 0.1F and a zombie's 0.23F. The engine never reads
a registered default: only an entity's base and synced values.

Up to 1.20.6 the jars register `movement_speed` and `flying_speed` as 0.7F and 0.4F widened, while minecraft-data holds
0.7 and 0.4. Nothing reads these two defaults, so no movement changes, and the test states the difference exactly.
Up to 1.13.2 minecraft-data also holds 1 as the least `max_health`, where the jars have 0. Nothing here reads that either.

## Modifiers

Vanilla's `AttributeInstance.calculateValue`, then the range: `BaseEntity.attributeValue`. Modifiers come with each
attributes packet, so a replacement or a removal is just the next packet. The iteration order is the container's per
release: `iterationOrder`. The fold's sum depends on it only through rounding. The adapter's `setSprinting` copy
is `unsynced` until the server's next packet replaces it.

## Other entities

| family | entry point | effective values it takes |
|---|---|---|
| ridden mob | `simulateVersionedHorseTick` | `movementSpeed`, `jumpStrength`, `gravity`, `stepHeight`, `waterMovementEfficiency`, `airDragModifier`, `frictionModifier`, its size for `scale` |
| generic mob | `simulateVersionedMobTravel` | `speed`, `maxStep`, `gravity`, its size for `scale` |
| moving entity | `simulateVersionedMovingEntityTick` | none: items, orbs, TNT and falling blocks are no LivingEntity |

The caller folds each one with the same modifiers, and an entity's base is its own: see the zombie column of
`test/fixtures/attributes.tsv`.

## `knockback_resistance` and `explosion_knockback_resistance`

Two consumers exist in 26.3: `LivingEntity.knockback` (`LivingEntity.java:1589`) and `ServerExplosion` (`:185`). Both run on
the server and reach a client as motion, in `set_entity_motion` and the explosion packet, already scaled. The engine has no
knockback or explosion entry point, so neither attribute has a consumer here and none is taken. A stab's recoil on the
attacker reads neither (`Player.causeExtraKnockback`, `stabHit`).
