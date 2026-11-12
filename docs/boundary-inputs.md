# Boundary inputs

Mineflayer cannot observe everything vanilla's tick reads. A deterministic prediction takes those facts as
explicit inputs. This lists each one, who owns it, how it enters, and what happens when it is absent.
`test/authority-boundaries.test.cjs` and `test/native-fire-draws.test.cjs` cover the behaviour.

| Fact | Owner | Input | Timing | Absent |
| --- | --- | --- | --- | --- |
| Order of the nearby entities | server's entity list | `nearbyEntities`, in tick order; `ticksBeforePlayer` for those that tick first | one tick's scene | `[]` is a legal empty scene, and equal to no field |
| Attached rockets | server | `fireworkRockets`, `fireworkRocketsBeforePlayer`, or `attached_rocket` entities in the scene | one tick | 0 rockets is legal |
| Raw `fireworkBoost` | caller | `fireworkBoost` on the state, applied at its documented point, apart from an attached rocket | one tick | no boost |
| Piston progress and direction | server, through block events | `movingPistons`, or `MovingBlockTracker` from the bot's packets | per tick | no moving piston: a piston mineflayer never saw cannot be inferred |
| Fire draws of a `ServerPlayer` | server's random | `serverFireIncrement` or the ordered `serverFireIncrements` | one per ignition | **refused**: a stream that runs out, and a burning `ServerPlayer` given neither field |
| Movement input | caller | the keys in `control`; `heading` is the adapter's own scaled read of them, read back, never an input. The engine's `input.x` and `input.z` take any float: vanilla's keys are its only source (`KeyboardInput`), and an analog vector goes through the same normalize, scales and square correction, held by bits to the jars from 1.21.5 (`java-oracle-input.txt`). Before 1.21.5 the same floats are only multiplied, in source order, with no jar check | every tick | no movement |
| Knockback on the player | server | a velocity packet. `NativePhysics.knockback` (`livingKnockback`) is for a caller that decides hits itself: strength, direction and the resistance's roll to 1.15 are its own to give | at the hit | no change. None from 1.21 for a direction vanilla redraws at random |
| Other random impulses | server | not modelled as inputs | | B058 is open; a recording's outcome is seeded in replay |
| Projectile target candidates | scene | `TargetEntity` list | one tick | no candidate means no hit |
| Server corrections | server | a position packet, at the next tick's start | between ticks | none |
| World updates | server | `applyBlockUpdates`, in packet order, before the tick that reads them | between ticks | the world as last known |
| Experiment selection | world | `minecart_improvements` in the scenario, or the adapter's option | whole run | the default carts |

## Missing is not zero

A legal empty list stays legal: no entities is an empty scene. A stream of draws that runs out is refused rather
than guessed.

A burning `ServerPlayer` given neither `serverFireIncrement` nor `serverFireIncrements` is refused with
`needs its fire draw`. Vanilla draws 1 or 2 at random there (`nextInt(1, 3)`), and reading 1 made a missing draw look
like a legal one. No draw is asked of a client, of a non-player, or before 1.21.2, where the branch does not exist, and
a state that gives its draw ticks as it did. A `serverFireIncrement` outside 1 and 2 is still clamped.

Replay keeps the same distinction in the recorder's `replay/verdict.ts`. A recording whose ticks the engine does not
model is `unsupported`, one with nothing to compare is `untested`, and only `pass` means the engine matched. A
selection with nothing compared fails (`tallyFails`), so a final position that happens to match cannot stand in
for context that was never given.

## Ordering

Entity pushes and rocket boosts are applied in scene order, and a boost is not commutative with a push. The test
swaps a pushing entity and an attached rocket and requires a different velocity, and requires the same ordered
scene to reproduce the same state exactly.

## Not covered

- Random impulses beyond the fire draws: knockback spread, fishing, potion scatter and the like (B058).
- Multi-tick continuity across a real server's packet timing. Continuous replay is B223.
