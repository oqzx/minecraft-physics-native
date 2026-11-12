# Bulk simulation, a chunk at a time

A native call cannot be interrupted, and an `AbortSignal` cannot reach into one. So a long run is a sequence of bounded
synchronous chunks with the event loop turning between them. There is no worker and no thread.

```ts
const run = binding.NativeBulkRun.players('26.3', states, config);
const { ticksDone, cancelled } = await runBulk(run, world, 72000, { chunkTicks: 20, signal });
const reached = run.playerStates();
run.dispose();
```

`simulateVersioned*Ticks` stay for short runs. They hold the thread for `ticks × states` entity ticks and cannot be stopped.
Use a `NativeBulkRun` for anything a caller may want to cancel.

## The contract

**Families.** Every batch family has a factory: `players`, `entities`, `boats`, `horses`, `projectiles`, `arrows` and
`tridents`. Each `simulateVersioned*Ticks` runs on the same batch type (`native/src/napi_bulk.rs`), so a run and a long
call are one loop.

**Checkpoints.** A chunk ends on a whole tick of every entity, in the order given. Nothing is observable mid-tick, and
`ticksDone` counts whole ticks.

**Work bound.** A chunk takes at most `maxChunkWork` entity ticks (16384). `advance` refuses more and runs nothing,
except that a single tick is always allowed, since a tick cannot be split. `runBulk` shrinks a chunk to fit.

A cancellation waits for the chunk in hand at most: `chunkTicks × entityCount` entity ticks, or one tick if that is the
larger. An abort from `onProgress` costs no further tick. An already aborted signal runs none. The bound is in entity ticks
and not in time. `docs/benchmarks.md` gives what an entity tick costs.

**Partial results.** A cancelled `runBulk` resolves with `{ ticksDone, ticksRun, cancelled: true }` and leaves the run at
its last chunk. Read it, go on with another `runBulk`, or dispose it.

**Resuming.** The run owns what a returned state cannot carry: a player's scene (piston progress, rocket lifetimes, the
movement speed attribute) and a projectile's fluid and base-tick state. Chunks therefore cannot drop context, whatever
their size. `test/bulk-simulation.test.cjs` compares one long call, chunks of 1, 7, 20 and all ticks, and a stopped and
resumed run, by deep equality on all seven families.

`playerStates()` also gives each player its scene, fit to start a new run or a tick. The other families read back what the
matching call returns, which for projectiles, arrows and tridents is the last tick's result and not a whole state.
Resume those through the run itself.

**Errors.** An error in a chunk finalizes the run: the failed tick stopped partway through its entities, so no consistent
state remains, and everything after it refuses, saying so. The world keeps no entity colliders from it. A refused
chunk, a wrong world or an error from `onProgress` leaves the run alive at its last finished chunk.

**The world.** A run keeps no copy. What a caller changes between chunks, the next chunk sees, as with ticks run one by one.
For a run isolated from the live world, give it `binding.nativeWorldFromCheckpoint(world.checkpoint())`. A run stays in
the world of its first chunk.

**Cleanup.** `dispose()` frees the states at once; without it the garbage collector does, when the run is dropped.
`runBulk` adds no listener to the signal, so an abort is polled at chunk boundaries and a long-lived signal holds nothing.
The test repeats a cancellation 3500 times and checks the listeners and the RSS.
