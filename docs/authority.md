# Authority

Vanilla splits a player's tick between the client that predicts it and the server that decides. One
`isClientSide` flag does not describe the split. This table is what the engine models, and
`test/authority-boundaries.test.cjs` holds the behaviour to it.

## The states

| Case | State | Moves itself | Server-only effects |
| --- | --- | --- | --- |
| Local player | `clientSide: true`, `controlledByLocalInstance` default | yes, predicted | not applied |
| Remote player or entity on a client | `clientSide: true`, `canSimulateMovement: false` from 26.3 | no: interpolated from packets | not applied |
| ServerPlayer | `clientSide: false`, `isPlayer: true` | no: the client's moves are validated | applied, with its own fire draws |
| Server entity | `clientSide: false`, `isPlayer: false` | yes | applied |
| Locally controlled vehicle | the vehicle's state with `controlledByLocalInstance` | yes, predicted | not applied |
| Server-driven vehicle | not simulated | no | `interpolated` in a replay, never compared as a match |

`clientSide` is `Level.isClientSide`. `controlledByLocalInstance` is `Entity.isLocalInstanceForMovement` and its
earlier forms. `canSimulateMovement` is 26.3's `Entity.canSimulateMovement` (26.3 `Entity.java`). The player's
kind is `isPlayer`, and a `ServerPlayer` is `isPlayer` with `clientSide: false`.

## Which side owns what

| Effect | Owner | Boundary | Source |
| --- | --- | --- | --- |
| Periodic freeze damage | server | 1.17 on | 1.17 `LivingEntity.java:2543-2559`, 26.3 `:3074` |
| Freeze counter from the inside-block effect | both | from 1.21.5 | 26.3 `InsideBlockEffectType.java:7` |
| Post-move fire reset | both before 1.21.6, server from | 1.21.6 | `audit` M23, B094 |
| Positive ignition with a random draw | `ServerPlayer` | 1.21.2 on | M24, B095 |
| Fall damage | server deals it, client predicts the landing | rounding at 1.21.5 | 1.21.4 `LivingEntity.java:1657-1664`, 1.21.5 `:1678` |
| Contact and wall damage | server | all | `MagmaBlock`, 26.3 `Entity.java:1969-1975` |
| Slime bounce under a piston | `canSimulateMovement` | 26.3; `ServerPlayer` skip from 1.16.2 to 26.2 | M21, B151 |

## Adapter and replay

`src/mineflayer/player.ts` runs local prediction. The recorder's `replay/seed.ts` seeds the same fields from the
recording, and a server's view is compared as its own row of a recording, not mixed with the client's.

## Corrections

A position packet replaces the predicted position, velocity and rotation at the start of the next tick, before
`tickStart`. A server-only count is not in that packet: the adapter never adds a client's damage to it, because a
client state has none. So a correction cannot apply a server-only effect twice, and the test runs exactly that case:
a client predicts 45 ticks in powder snow, takes the server's position and carries on with no damage of its own, while
the server's count rises once per 40 ticks.

## Not covered

- The legacy releases' side split rests on their unit tests and on the current replays of both sides on them: the server's and
  the client's moving entities, on every release (`generated/subsystem-matrix.json`, rendered by the recorder's docs/subsystem-matrix.md).
- A `ServerPlayer` is modelled as far as damage and draws go. Its move validation (`ServerGamePacketListenerImpl`) is
  not simulated: the recorder's server view of the player is the reference for that.
