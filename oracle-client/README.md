# Vanilla Client Capture

This client-only Fabric mod captures `ClientPlayerEntity` state around actual Minecraft client ticks for 1.20.4. It does not use Mineflayer or the project's native implementation to produce the vanilla trace.

The mod opens a line-oriented TCP control server on `127.0.0.1:25566` (configurable with `-Doracle.client.port`). After a player joins a world and an oracle process connects, it emits a `ready` record. Each subsequent `START_CLIENT_TICK` waits for one JSON control record, applies those key states, and emits one post-tick `tick` record from `END_CLIENT_TICK`.

This is the authoritative client-trace component only. The oracle still needs to launch/configure a matching vanilla client and server, synchronize the world and player state, and compare the captured trace with the same native tick inputs before any result can certify parity.