# Teleports, portals and new dimensions

What moves a player without a tick's move, and who says so. The engine has no networking and no level: it takes the
result of each event from the caller, and emits only movement consequences. `test/lifecycle.test.cjs` covers the rows
below. `physrec/m_rotate_teleport`, `cu_tp_away_and_back`, `mc_nether_portal`, `mc_end_portal` and `m_dimension` replay
clean on 26.3 from real clients.

| event | the caller gives | the engine does | stays | resets |
|---|---|---|---|---|
| ordinary teleport (position packet) | the new position; the velocity and rotation the packet's relative flags leave | drops the previous position; the next tick finds its own support | velocity as given, rotation, pose, counters, attributes | the support, and before 1.17 the kept box if the caller drops it |
| portal entry | nothing | nothing: a portal block has no player movement effect | everything | nothing |
| portal exit, same state | as a teleport | as a teleport | as a teleport | as a teleport |
| dimension change | a new world and a new state, or `setState` | refuses to tick the old state in the new world until `setState` | what the caller seeds | what the last tick reported, a pending `tickMove`, the world |
| respawn | a new state: vanilla builds a new LocalPlayer | as a dimension change | what the caller seeds | everything else |

## A position set alone

The object API takes a state as it is. Setting `position` while keeping `previousPosition`, `supportingBlock` or, before
1.17, `boundingBox` from before is no vanilla teleport: the engine reads what it is given. The packed path and the
adapter make this hard to get wrong. `UNMOVED` is what keeps the previous position, and the adapter sends it only when
the position is where the engine left it. The adapter's `keptFor` drops a kept box at any other position.

## Portal timers

`Entity.portalProcess`, `PortalProcessor` and the portal cooldown decide on the server when an entity changes dimension.
The local player only hears the result, so these are no player state and nothing can be retained from a previous world.
A minecart has its own portal lifecycle in `native/src/minecart.rs`, which the cart's tick does own: `recorder` covers it
with `mc_*`.

## Velocity, rotation and pose

The engine never changes these on a teleport, as it never sees one. The rules are the position packet's relative flags,
applied by the caller. `ClientPacketListener.handleMovePlayer` keeps what a flag marks relative and sets the rest.
