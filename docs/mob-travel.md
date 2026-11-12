# Mob travel: what a client physics engine owns

`simulateVersionedMobTravel` is `LivingEntity.travel` and its overrides, run from what the mob's AI left. The engine holds the
physics of the move; it holds no AI. That line follows the tick itself: `aiStep` lets the AI set `xxa`, `zza`, `jumping` and motion,
then `travel` integrates them, so the AI's result is an input and the integration is deterministic.

| In the engine | Caller's input |
| --- | --- |
| Gravity, drag, fluid inertia, flight friction, `moveRelative`, `Entity.move`, collision, fall distance, block effects | `input`, `speed`, `yawDegrees`: the move control's result |
| Each kind's own travel, table below | `hasTarget`, `guardianMoving`, `wantsToSwim`, `nearHome`: what the goals decided |
| Dragon flight, parts and wing pushes (`simulateVersionedDragonTick`) | The phase and its target |

Pathfinding, target selection, goal timing, the random draws of AI, and a mob's jump or hop are left out. A jump is motion set in `aiStep`
before `travel` runs, so it arrives in `velocity`. So do a blaze's slow fall, a chicken's flutter and a slime's hop.

Client and server: a mob the player does not ride is the server's. The client interpolates it, so this path predicts the server's move.
Ridden mobs are `simulateVersionedHorseTick`, happy ghasts and nautiluses included, and are not routed through here.

| Kind | Travel |
| --- | --- |
| Squid, glow squid | `Squid.travel`: moves by its motion alone |
| Cod, salmon, pufferfish, tropical fish, tadpole | `AbstractFish`: water move with its own drag, flops on land |
| Guardian, elder guardian | Water move, 0.9 drag, sinks with no target |
| Ghast, phantom, allay, vex, bee, parrot, bat | Flying friction by kind; vex passes through blocks |
| Dolphin, turtle, axolotl, frog, drowned, polar bear | Own swim speeds and water friction |
| Spider, cave spider, wither, armor stand, sulfur cube | Their climbing, stuck and fluid exceptions |
| Unridden strider, skeleton horse | As their ridden kinds' travel with no rider |
| `living` | Any other `LivingEntity`: it has no `travel` of its own (`me_mob_chicken_flutters`, `me_mob_slime_hops`, `me_mob_magma_cube_in_lava`, `me_mob_breeze_hops`, `me_mob_iron_golem_stands`, `me_mob_rabbit_hops`, `me_mob_creeper_in_water`) |

`entityTravelSupport` names the family of a kind by release. Every kind of every release's registry has one
(`test/mineflayer.test.cjs`), so `unknown` is only a name outside the game, and this path refuses it. A `mob` kind that is not in the
table travels as `living`: no class below LivingEntity overrides `travel` (or `travelInWater`, `travelInFluid`) but those of the table,
the ridden kinds and the player, read in the 1.8.8, 1.12.2, 1.13.2, 1.14.4, 1.16.5, 1.17, 1.19.4, 1.20.6, 1.21.2, 1.21.11 and 26.3
trees. Two kinds are taken out of `mob` and refused here: the dragon (`dragon`), whose aiStep moves it and never calls `travel`
(1.8.8 EnderDragon.java:102-220, 26.3 EnderDragon.java:146-231), and the shulker (`unsupported`), whose travel leaves nothing: 1.9.4
Shulker.java:365-369 zeroes its motion after aiStep, 1.16.5 Shulker.java:257 puts it back on its block in tick, and 26.3
Shulker.java:295-300 holds no motion at all. Interpolation
of a remote mob is `lerpPosition` on the client's side, and is no part of this.
