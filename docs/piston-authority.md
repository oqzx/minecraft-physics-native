# Piston movement authority

`NativeSimulationState`, entity descriptors and the Mineflayer adapter accept an
optional `canSimulateMovement` boolean. Supply the result of vanilla's 26.3 method
when the entity's simulation mode cannot be inferred from the existing flags.
The value controls piston slime bounce and piston collision restitution from
26.3. Earlier releases ignore it and retain their own rules.

When omitted, players use `!clientSide || controlledByLocalInstance`; generic
entities use `controlledByLocalInstance`, the default `AUTHORITATIVE_SIDE` mode.
`controlledByLocalInstance` represents the resolved local authority at this phase
(`Entity.isLocalInstanceAuthoritative`). It is not equivalent to server side:
a server player or a server mount controlled by a client can have it false.
For `SERVER_AND_CLIENT` entities, such as items and projectiles, pass
`canSimulateMovement: true` explicitly. Supply the resolved result for custom
entity modes as well; this API does not infer a Java subclass from dimensions.

The override remains optional on output, so changing side/control flags later
recomputes the fallback instead of retaining an old computed permission. Packed
players keep it via `setState`/`state`; whole-state replacement without it clears
it. The adapter forwards true, false and absence without adding a packed index.

Simulation permission and local authority have separate jobs. Permission gates
the slime impulse and collision restitution. A horizontal piston move by a
non-authoritative entity preserves its vertical collision, ground and supporting
block state. Its actual displacement and applicable inside-block effects still
run. This input does not redefine the engine's other movement-authority gates or
provide complete entity ticking for items/projectiles.
In particular, the shared collision restitution helper still follows living
entity block bounciness; the nonliving `0.8F` block-restitution factor in 26.3
`Entity.java:905` is outside this authority change.

## What is not a player

A moving block asks every entity of the level but those of `PushReaction.IGNORE_ENTITY`, so an item, TNT, a falling block, a boat, a
cart and a mob are moved as the player is, by the same boxes, by their own `Entity.move` (26.3
`PistonMovingBlockEntity.java:110-170`). `simulateVersionedEntityPistonPush` runs the pistons of a game tick for one of them, after
its own tick, and `NativeBoatPhysics` and `NativeRiddenMobPhysics` have `pushByMovingPiston`. Leave out what vanilla does not push.
`controlledByLocalInstance` and `canSimulateMovement` work as above: an item or TNT simulates on both sides and the server owns it, so a client
passes `controlledByLocalInstance: false` and `canSimulateMovement: true`. `me_*_piston` and `pm_ride_*` hold both sides.

The world's lifecycle of a moving block stays the caller's: `NativeWorld.setMovingBlock` and `moving.ts`'s tracker make the `moving_piston`
cells from block events, the tick that ends one puts its block back, and nothing here finds that a piston should fire.

## Historical behavior

| Release | Server-player slime behavior |
| --- | --- |
| 1.9.4–1.16.1 | Bounce and displacement are permitted. |
| 1.16.2–26.2 | Skip both bounce and displacement for this slime piston. |
| 26.3 | Gate bounce by simulation permission; displacement still runs. |

`isPlayer && !clientSide` identifies a vanilla server player in this contract.
The historical skip does not suppress ordinary pistons, honey carry or subsequent
pistons. The registered 1.8.8 path is unchanged; its available 1.8.9 stand-in uses
an older extending-slime rule and does not establish exact 1.8.8 source identity.
The 1.16.1 velocity packet sent to server players is outside the physics API.

Collision restitution in 26.2 already has a separate simulation-permission
guard. It uses that release's player/generic fallback; the new override applies
only from 26.3. Ground/support ownership follows the existing 1.21.4 boundary.

## References

- `E:/pr/mc-decompiled/1.16/net/minecraft/block/entity/PistonBlockEntity.java:102`
  and `1.16.5/net/minecraft/block/entity/PistonBlockEntity.java:108` establish
  the surrounding historical behavior. Release-identified cached 1.16.1–1.16.4
  jars pin the whole-entity skip to 1.16.2; complete bytecode and hashes are saved.
- `E:/pr/mc-source/26.2/client/net/minecraft/world/level/block/piston/PistonMovingBlockEntity.java:111`
  and the 26.3 file at line 109 show the old skip versus the new bounce guard.
  `26.3/client/net/minecraft/world/entity/Entity.java:818` and `:839` distinguish
  ground ownership from restitution; `:3721` defines the three simulation modes.
  The 26.2 `Entity.java:815` also gates restitution.
- `D:/projects/mc-zuri/mc-data/extracted_minecraft_data`: the same piston file is
  pinned at 26.1 `250494672ff7a6ef66607c785c91a56a5c32c9f3`, 26.1.1
  `2322018b5712b13eacbbb324b7f9dd359091cdc7`, 26.2
  `912ae031240da395051acae238a3f60d733353de` and 26.3
  `7c0695f918f2b96b09b66169ba54b7aa6e85244f`.
- `D:/minecraft/java/physics-data-generator/mc/26.3/src/main/java/dev/physrec/mc/BridgeImpl.java:331`
  captures the local player; other-entity observations at line 416 omit movement
  authority. This is supporting capture tooling, not a server-player oracle run.

Evidence is retained in `audit/physics-backlog-20261104-dc4f918/b151-evidence`
in the main checkout, alongside `b151-reference.txt`. These are source-derived
regressions; no new Minecraft runtime recording is claimed.
