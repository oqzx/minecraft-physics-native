# Tick order

The order vanilla runs a local player's tick in, and how much of that order is checked today. A formula that gives
the right final position can still run in the wrong order and hide a pair of errors, so this states plainly what
is and is not compared.

The recorder's `schema/phases.ts` is the shared definition, and the recorder mod snapshots vanilla at each of these points.
The engine emits the ones the table gives a native point for (`native/src/phase.rs`, `simulateVersionedTickTraced`),
and the recorder's `replay/cli.ts --phases` compares them by name and occurrence. A trace only reads: a traced and an untraced
tick are the same tick.

| Phase | Vanilla (1.20.4) | Native point |
| --- | --- | --- |
| `tickStart` | `LocalPlayer.tick` head, after the round's server packets | `tick_traced`, after the riptide launch |
| `afterBaseTick` | `LivingEntity.baseTick` tail: fluid state, fluid push, effects | `tick_traced`, after the lava fall-distance halving |
| `afterInput` | `LocalPlayer.aiStep` after `Input.tick` | adapter only |
| `beforeLivingAiStep` | `LivingEntity.aiStep` head: sprint, fly and elytra decided | `tick_traced`, before the 0.003 cut |
| `beforeTravel` | `Player.travel` head, with the travel vector | `tick_traced`, after jump and fall-flying start |
| `beforeMove` | `Entity.move` head, `MoverType.SELF` | `tick_traced`, before `move_slowed` |
| `afterMove` | `LocalPlayer.move` tail: flags, ground, supporting block, speed factor | `tick_traced`, after the block speed factor |
| `afterTravel` | `Player.travel` tail: gravity, drag, fluid falling | `tick_traced`, after `glide_stopped` |
| `afterAiStep` | `LocalPlayer.aiStep` tail | `tick_local_player_scene`, after the spin attack check |
| `beforeSend` | `LocalPlayer.tick` after `AbstractClientPlayer.tick` | adapter only |
| `tickEnd` | `LocalPlayer.tick` tail | adapter only |
| `clientTickEnd` | `Minecraft.tick` tail: later entities, rockets, pistons | `tick_scene`, after the moving pistons |

## What a replay compares

For the player, the recorder's `replay/engine.ts` seeds a tick from the recorded `tickStart` and `beforeLivingAiStep`
states, runs the engine, and compares every mapped field at `afterAiStep`. A field with a `comparedAt` in
the recorder's `schema/fields.ts` is compared with vanilla's value at that phase instead.

With `--phases`, `PHASE_FIELDS` in the recorder's `replay/phases.ts` is also compared at each phase, field by field: position
and velocity throughout, fluid flags up to `beforeMove`, contact flags and `fallDistance` after the move. A compensating
error inside one tick that nets out by `afterAiStep` is caught where those fields show it. An intermediate value outside
`PHASE_FIELDS` is not compared, and neither is the `afterInput`, `beforeSend` or `tickEnd` phase, which are the adapter's.
A phase one side passed and the other did not is a problem, not an empty comparison.

Damage is not a compared field: `fallDamage`, `contactDamage` and `freezeDamage` are held to the server's health
in the `fd_*` and `hz_*` cases (`compareLandings`, `compareContact`, `compareFreezing`).

Vehicles and projectiles compare per tick, in the recorder's `replay/vehicle.ts`, `projectile.ts` and `moving-entities.ts`.

## Numeric conversions

These are the float and double boundaries that later phases read. Each is a source-backed gate in the engine. Line
numbers are of `E:/pr/mc-decompiled/<release>`:

| Boundary | Change | Source |
| --- | --- | --- |
| 1.20.5 | `isFalling` checks the gravity argument, not `!hasNoGravity()` | 1.20.4 `LivingEntity.java:2217`, 1.20.5 `:2288` |
| 1.21.2 | travel splits into fluid paths, with block effects moved | `native/src/versioned/mod.rs`, `fluid_travel_split` |
| 1.21.5 | `calculateFallDamage` in double with `1.0E-6`, floor not ceil | 1.21.4 `LivingEntity.java:1657-1664`, 1.21.5 `:1672-1682` |
| 1.21.5 | `fallDistance` is a double | 1.21.5 `Entity.java:1286` |
| 1.21.11 | `Mth.sin` indexes through `long` from a double | 1.21.10 `Mth.java:51-56` (float, commit `bd23da11`), 1.21.11 `Mth.java:50-55` (double) |

## Unit results and trace results

These are reported apart, and only one of them certifies anything:

- **Unit and formula tests** (`cargo test`, `test/*.test.cjs`) show that a rule does what the source says. They
  raise no cell of the contract.
- **Vanilla traces** (the recorder's `replay/cli.ts`) show that each tick's end state matches vanilla in every compared
  field.
- **Phase-aligned traces** (`--phases`) add the intermediate fields. The recorder's `scripts/pin-conformance.cjs` pins player, boat and
  mount cells with them on, and only a pin raises a cell (`docs/parity-contract.md`).

A replay's first divergence names its tick, comparison point and field, which is what a correction of order cites.

## Boundary traces

See `docs/release-boundaries.md`. Replaying the two releases either side of a boundary with `--phases` compares velocity,
flags and fall distance at each phase, and a landing's damage against the server's health.
