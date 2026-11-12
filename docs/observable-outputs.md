# Observable outputs

What the engine promises to report, per entry point, and how each output is told from a value that is merely
missing. The field-level inventory lives in code, so it cannot drift:

| Inventory | Holds | Enforced by |
| --- | --- | --- |
| The recorder's `schema/fields.ts` `PLAYER_FIELDS` | each recorded field: type, side, vanilla source, comparison | `gen-java.ts`, the recorder's `schema/test` |
| The recorder's `schema/fields.ts` `NATIVE_SIM` | each `NativeSimulationState` field: compared from a recording, seeded, or excluded with its reason | `tsc`, typed over every key |
| `scripts/state-bridge.cjs` | each field's way through a `NativePlayer` | `test/state-bridge.test.cjs` |
| `scripts/state-roundtrip.cjs` | each field of the boat, mount, cart, projectile, arrow, trident and moving entity states: kept, evolved, config, refused or structured | `test/state-roundtrip.test.cjs` |
| `scripts/parity-contract.cjs` | each interaction's outputs and authority | `test/parity-contract.test.cjs` |

## Ownership

| Kind | Meaning | Examples |
| --- | --- | --- |
| Predicted | the engine computes it, a replay compares it | position, velocity, flags, fall distance |
| Observed | the caller supplies it | attributes, effects, rotation, fluid-independent input |
| Derived | computed from the others | bounding box from 1.17, look vector |
| Per-tick output | reported for one tick, then cleared | `fallDamage`, `contactDamage`, `wallDamage` |
| Running count | carried across ticks; a tick's amount is its rise | `freezeDamage` |

## Units and numeric types

Positions and velocities are doubles, in blocks and blocks per tick. Rotations cross the API in radians and
are held to `float32` when compared, as vanilla holds them. `fallDistance` is a float before 1.21.5 and a double from
then (`doubleFrom` in `fields.ts`). `-0` and `0` are different values (the recorder's `replay/test/detects.test.ts`).

## Event and damage order

In one tick: rockets and entities that tick before the player, the player's tick (landing, then contact), its spin
attack check, later rockets and entities, then the moving pistons. `tick_local_player_scene` in
`native/src/simulation.rs` holds that order.

## Unsupported, unobserved and zero

| Value | Reads as | Where |
| --- | --- | --- |
| Actual zero | `0`, compared | `compare.ts`: an expected `0` with a missing output diverges as `<missing>` |
| Not returned | a divergence, never a match | the recorder's `replay/test/detects.test.ts` |
| Not recorded for this release | not compared, counted | `versions` in `fields.ts`, `unavailable` in `engine.ts` |
| A tick the engine has no model for | `unsupported`, not a pass | the recorder's `replay/verdict.ts` |
| Nothing compared | `untested`, which fails a selection alone | `tallyFails` |

## Failing conformance

The recorder's `replay/test/detects.test.ts` changes one recorded value and requires the replay to report it at its tick
and field, for a one-ulp position change and for a flag, with the position otherwise matching. Fall damage is held to
the server's health in `fd_*`, not to position.

## Not covered

- `eyeInWater` and other derived flags are compared where recorded, and not all interaction events have a recorded
  counterpart. Events without one are in the contract's outputs but unproven.
- The round-trip table reads a tick in empty air on 26.3. A field that a class says is kept is not lost; that says
  nothing of its value being vanilla's, and the structured fields have their tests elsewhere.
