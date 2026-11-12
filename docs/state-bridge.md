# State bridge

How a player's state crosses from TypeScript into the engine and back. `NativePhysics` drives a `NativePlayer`:
a packed `Float64Array` each tick for what changes every tick, and `setState` for everything else, which the
player keeps between ticks.

`scripts/state-bridge.cjs` classifies every field of `NativeSimulationState`, and `test/state-bridge.test.cjs` fails
on a field left out or on a channel the layout does not have. So a new field cannot reach one path and silently miss
the other.

| Channel | Fields | Carried by |
| --- | --- | --- |
| Packed slots | `PACKED` | `NativePlayerInput` |
| Input flags | `FLAGS` | `NativePlayerInputFlag`, in slot `FLAGS` |
| Kept | `KEPT` | `setState`, returned by `state()` |
| Scene | `SCENE` | the tick's `entities` array, and `pushByMovingPistons` |
| Outputs | `OUTPUTS`, `OUTPUT_FLAGS` | `NativePlayerOutput` and its flags |

`NATIVE_SIM` in the recorder's `schema/fields.ts` does the same for the replay: it is typed over every key, so `tsc`
fails on a field the recorder has not accounted for.

## Rejections

Nothing is dropped silently:

- an input flag above the known bits is refused as a layout this build does not know (`KNOWN` in
  `native/src/napi_player.rs`);
- `serverFireIncrement` and `serverFireIncrements` together are refused;
- a stream of fire draws that runs out is refused;
- a release that is not registered is refused at construction.

## Absent and zero

An attribute that is absent uses vanilla's default; an explicit `0` is a value. The test holds `movementSpeedValue`,
`jumpStrength`, `gravity`, `stepHeight`, `fallDamageMultiplier` and `safeFallDistance` to that through
`NativePlayer.state()`, and shows that a zero speed and a zero gravity move differently from none.

## Equivalence

`test/state-bridge.test.cjs` ticks the same state through `simulateVersionedTick` and through a packed
`NativePlayer` on 1.8.8, 1.12.2, 1.20.4, 1.21.5 and 26.3, with nondefault values: sprint, sneak, slowed, falling,
landing, no gravity, custom gravity, item use, jumping. Position, velocity, fall distance, fall damage and the
collision flags must be identical, bit for bit. Damage outputs are also covered on both sides of authority.

`test/from-bot.test.cjs` shows the adapter writes only what `apply` writes and leaves the bot's other fields and
nested objects alone. `test/mineflayer.test.cjs` covers the packed path through a whole sleep, and forwarded movement
authority.

## Not covered

- Equivalence is checked on the listed fixtures, not over the full state space.
- Vehicles have their own bridges (`NativeBoatState`, `NativeHorseState`), which this does not classify.
