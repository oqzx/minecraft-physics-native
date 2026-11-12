# Projectile families by release and side

Which family the engine flies, from which release, on which side, and what stays the caller's. Sources are the
per-release decompiled trees; the numbers are in `native/src/projectile.rs` next to the citation of each branch.

Every family goes through `simulateVersionedProjectileTicks`, or `simulateVersionedArrowTicks` and
`simulateVersionedTridentTicks` for arrows and tridents, with the `...HitTick` variants when targets are given.
`--projectiles` in the recorder's `replay/cli.ts` flies each one through the server's recorded view.

| Family | Kinds taken | First release | Gravity | Inertia | Size | Recorded in |
| --- | --- | --- | --- | --- | --- | --- |
| Snowball, egg, pearl | `snowball`, `egg`, `ender_pearl` and spellings | 1.8 | 0.03 | 0.99F (water 0.8F) | 0.25 | `pj_snowball_*`, `pj_egg_*`, `pj_ender_pearl_*` |
| Potion | `potion`, `splash_potion`, `lingering_potion` | 1.8 | 0.05 | as above | 0.25 | `pj_potion_*` |
| Experience bottle | `experience_bottle`, `xp_bottle`, `thrownexpbottle` | 1.8 | 0.07 | as above | 0.25 | `pj_experience_bottle_*` |
| Arrow | `arrow`, `spectral_arrow`, `tipped_arrow` | 1.8 (`spectral_arrow`, `tipped_arrow` 1.9) | 0.05 | 0.99F (water 0.6F) | 0.5 | `pj_arrow_*`, `pj_bow_*`, `pj_pierce_*` |
| Trident | `trident` | 1.13 | 0.05 | 0.99F (water 0.99F) | 0.5 | `pj_trident_*` |
| Firework rocket | `firework_rocket`, `fireworks_rocket` | 1.8 | none | none | 0.25 | `pj_firework_*`, `f_elytra_rocket_*` |
| Fishing hook | `fishing_bobber`, `fishing_hook` | 1.11 by this API, refused before | 0.03 | 0.92 | 0.25 | `pj_fishing_hook_*` |
| Fireball, small fireball, wither skull | `fireball`, `small_fireball`, `wither_skull` | 1.8 | none | 0.95F | 1, 0.3125, 0.3125 | `pj_*fireball*`, `pj_wither_skull_*` |
| Dragon fireball | `dragon_fireball` | 1.9 | none | 0.95F | 1 | `pj_dragon_fireball_*` |
| Wind charge | `wind_charge`, `breeze_wind_charge` | 1.20.3 (`breeze_wind_charge` 1.20.5) | none | 1 | 0.3125 | `pj_wind_charge_*` |
| Llama spit | `llama_spit` | 1.11 | 0.06 | 0.99F | 0.25 | `pj_llama_spit_*` |
| Shulker bullet | `shulker_bullet` | 1.9 | 0.04 | none | 0.3125 | `pj_shulker_bullet_*` |

A name the table does not have is refused, and a state with no name at all is `Generic`, the thrown item's tick
(`test/mineflayer.test.cjs`); a kind asked of a release before its first is refused with its name. Each value above is the family's default: `gravity`, `airDrag`,
`power` and `accelerationPower` of a state override it where the family's tick reads them. A rocket and a hook before 1.11
read neither `gravity` nor `airDrag`, and a shulker bullet reads no `airDrag`.

## Boundaries the integration follows

| Release | What changes | Families |
| --- | --- | --- |
| 1.9 | A thrown item ignores what stood at the throw until 1.15 | Thrown |
| 1.13 | Clip by shape, not the six-face ray of `legacy` | All |
| 1.14 | A rocket has a hit test; before it has none | Firework |
| 1.16.2 | Bubble columns of the box before the move | Thrown |
| 1.18.2 | An attached rocket stands at a gliding player's hand | Firework |
| 1.20.5 | Gravity is a double, not a widened float (hook from 26.3, bullet always) | Thrown, arrows, spit |
| 1.21 | `accelerationPower` along the motion replaces `power` | Hurting |
| 1.21.2 | Pre-move integration, `clipIncludingBorder`, first-tick bubble scan | Thrown, hurting; arrows keep their original-motion move |
| 1.21.5 | Unclamped bubble pushes of Projectile, with a pearl's own | Thrown |
| 26.3 | First-tick scan order | Thrown |

## Sides

What each path predicts, and which side's rules it follows:

| Path | Follows | Caller's |
| --- | --- | --- |
| Thrown, hurting, spit | Both sides: the code is shared | The discard. A client's discard is a prediction of the server's |
| Arrow, trident | The server's embedded ticks from 1.21.2 | `hurt`, what `Entity.hurt` answers |
| Rocket, free | Both sides | `lifeTime`, which is the server's random |
| Rocket, attached | Follows `holder`; the boost is the player engine's (`docs/flight-phases.md`) | `attached`, `holder`, whether the id resolves |
| Fishing hook | The client's tick | `stopFishing`, bite and pull, `groundRelease` before 1.11 |
| Shulker bullet | The client's position update | `bulletDelta`: the server draws each next delta and steers by it |

Server homing is not modelled: a shulker bullet flies by a delta the caller supplies, which `pj_shulker_bullet_steers_to_player`
takes from the recording. Hits, reactions and damage are in `native/src/projectile_hits.rs`. Not modelled: a pearl's teleport
of its owner, portals stopping a hit, a hook's bite pulls, and explosions (the engine says a rocket ends, and what it does to
the world is the caller's).

## World border and bubble columns

`pj_*_border_crossing` cross a 24 wide border on the way out, and `pj_arrow_border_from_outside` starts past it, so that from
1.21.2 only a crossing is a hit. `pj_snowball_bubble_*` and `pj_ender_pearl_bubble_*` are summoned still in a column: the replay
starts them from the summon, so their first tick is compared.

## Multipart targets

The level lists a dragon's parts among its entities, after it (from 1.21.4, after all the others), and projectiles hit those, never
the dragon. So the parts are candidates like any other target, in that order, and `ProjectileScene.dragonParts` takes them from
`NativeMovingEntityPhysics.tickDragon`. `pj_arrow_hits_dragon_body` and `pj_arrow_through_dragon_box_past_parts` hold this.
