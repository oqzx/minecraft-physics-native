# Fall flying: start, tick, rockets, landing

What the engine does for an elytra glide, in the order of a client tick, and which side owns each decision. Numbers and
citations are in `native/src/simulation.rs` and `src/mineflayer/player.ts` beside the code.

## Who decides

| Decision | Side | Where |
| --- | --- | --- |
| Start on a fresh jump (`LocalPlayer.aiStep`, `Player.tryToStartFallFlying`) | Client from 1.15, which tells the server. Before, the server starts it and the shared flag arrives | Adapter: `fallFlying` follows the rule below from 1.15; before, the caller gives the flag |
| The flag staying up (`LivingEntity.updateFallFlying`) | Server only: `canGlide` false clears shared flag 7 | The caller gives what the server sent. The native tick clears the flag only on the climbable stop and creative flight |
| Elytra damage | Server only | Not simulated (B179) |

Direct native callers set `fallFlying` on the state: it is an authorized state, and no native entry point refuses it. The adapter's
contract is the start rule. `Player.canGlide` is: not on the ground, no passenger, no Levitation (from 1.16), a usable glider worn;
and `tryToStartFallFlying` adds no water, and from 26.3 no lava. Held jump does not start one (`jumping && !wasJumping`). A passenger
is refused by the adapter's check as well as the server's: `f_elytra_no_deploy_riding_boat` and `_pig` record that, with no glide.

## A tick

| Order | Step | Releases |
| --- | --- | --- |
| 1 | Attached rockets before the player, entities that tick first | From 1.11 |
| 2 | `checkFallDistanceAccumulation`: `vy > -0.5` lowers a fall distance over 1 to 1, in any fluid | From 1.21.2, every tick the flag is up |
| 3 | The glide of travel | No fluid only. Climbing from 1.21.5 travels in the air and stops the glide after it |
| 3a | The same check inside the glide (before 1.21.2): lowers it from 1.19.3, sets it to 1 outright from 1.9 to 1.19.2 | 1.9 to 1.21.1 |
| 4 | `updateFallFlying`: only the ticks counter on the client | All |
| 5 | Move, collisions, `checkFallDamage`, landing | All: the fall counts before the landing from 1.21.5, outside water |
| 6 | Rockets and entities after the player | From 1.11 |
| 7 | Moving pistons | All |

`onGround` ends a glide on the server, whose flag reaches the client on a later tick: the row after a landing can still show
`fallFlying`. `gl_*`, `f_elytra_land_*` and `f_elytra_mounted_mid_glide` compare that as the client saw it, so nothing here predicts
the server's clearing. The flag is an input, and a caller that has no server stops it when `canGlide` turns false.

Creative flight: `flying` in travel resets the fall distance and the glide from 1.9 (1.8 only scales the motion).
`f_elytra_creative_double_jump` and `f_fly_*` hold it.

## Rockets

| Path | Input | Order | Use |
| --- | --- | --- | --- |
| Scene | `TickScene.fireworkRockets`, `fireworkRocketsBeforePlayer` | At the start of its side of the player | A count |
| Nearby entities | `attached_rocket` in `nearbyEntities` | In its turn among the entities | The client's exact order |
| Duration | `fireworkRocketTicks`, one per rocket | Boosts, then counts down; an ended rocket drops | Batches |
| Raw input | `fireworkBoost` | Before the tick reads its motion, once a tick, between the scene's rockets | Compatibility |
| Adapter | `fireworkRocketDuration` | Kept while the glide pauses | A live bot |

Each boost is `v + look * 0.1 + (look * 1.5 - v) * 0.5`, one after another, so two rockets are not twice one. A rocket boosts only while
its holder glides, and from 1.11. Mismatched counts (`fireworkRocketTicks` of another length) are refused, and more than 65536
rockets too. Non-finite motion follows `validate_tick_state` and `add_delta_movement`.

The rocket entity is `simulateVersionedProjectileHitTick`:

| State | Rocket's tick (26.3 `FireworkRocketEntity.java:108-180`) |
| --- | --- |
| Free | Boost unless shot at an angle, hit test, `Entity.move`, the motion put back |
| `attached` and `holder` | Stands at the holder, at a gliding player's hand from 1.18.2, takes its motion, hit test, no move |
| `attached`, no `holder` | An id nothing answers to, or not a `LivingEntity`: no motion, hit test from where it is |
| Any | `life + 1`; past `lifeTime` it ends, as on any entity hit or a block hit with explosions (`discarded`) |

Vanilla never clears the attachment, and keeps a resolved entity after it is removed, so a rocket does not go free when its
holder is gone. What it is attached to is the caller's to hold. The explosion is the server's: `discarded` says the rocket ended, a client's
says so as a prediction, and damage is not computed. `f_elytra_rocket_*` replay through `--projectiles` as well as the player's replay.
