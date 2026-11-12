# What an update from the server does to the engine's state

The engine does no networking. The caller reads a packet and picks one of the events below; each states what the engine
guarantees, so that nothing is left to a generic state replacement by accident. Compare with `Entity.java`, `LocalPlayer.java`
and `ClientPacketListener.java` of the release: 26.3 is the directly checked checkpoint.

## The local player (adapter)

The adapter's carry remembers where the engine left the player. When `state.pos` differs at the next tick, the change is an
external one, and the tick starts from the new place.

| Event | The caller does | Next tick | Kept | Dropped |
| --- | --- | --- | --- | --- |
| Teleport, position packet | Sets `state.pos`; `state.vel` when the packet carries it | Starts at the new place, and `previousPosition` is that place | Counters, effects, abilities, the supporting block until the next move | The start position of the last tick |
| Velocity correction | Sets `state.vel` | That motion is the tick's: applied once | Everything else | Nothing |
| Rotation only | Sets `yaw`, `yawDegrees`, `pitch` | The look of the tick | Everything | Nothing |
| Kept box before 1.17 | Sets `keptBoundingBox` | `NativePlayer.setBoundingBox` is called at the next tick | Everything else | The box the engine carried |
| New world | Calls `simulate` with the new world | The carry moves by `setState` | State | The last tick's report and pending finish |

None of these runs a tick, moves `state.age`, or repeats a one-shot. `test/sync-contract.test.cjs` holds them. A velocity of 0
moves nothing, so a teleport onto a floor shows `onGround` only after the next tick's gravity meets it, as vanilla's does.

## NativePlayer

| Call | Effect |
| --- | --- |
| `setState(state)` | Replaces the whole state. What the last tick reported, a tick `tickMove` left unfinished, pending pushes, piston deltas, the auto spin ending and the rockets after the player go with the old state; the player is free to tick in any world. A refused state changes nothing |
| `state()` | A whole state. A new `NativePlayer` of it continues as the original does through the same updates |
| `setBoundingBox(box)` | Only the kept box (`Entity.setBoundingBox`) |

The supporting block stays through a teleport, as `Entity.mainSupportingBlockPos` does: only a move sets it (26.3 `Entity.java` `checkSupportingBlock`), so the
first tick after a teleport reads friction through the old one's column, as vanilla's. A state whose `supportingBlockKept` is false finds it again from its
position. `tickCount` and `age` move only in a tick.

## Vehicles and ridden mobs

| Path | Owner | Event |
| --- | --- | --- |
| A boat or mount the player controls | The client's engine | As the player: a changed `pos` drops the carry, whose fields then come from the state |
| A vehicle the server drives | The server, by `ClientboundMoveEntity`/`TeleportEntity` | Not simulated. `interpolate` for carts, `lerpPosition` for mobs: one step of `Mth.lerp` by `1 / lerpSteps`. `compareClientLerp` compares them |
| A minecart's own tick | The server | `NativeMinecartPhysics.simulate`; `MinecartTracker` follows packets on the client |

Pistons do not stop for a vehicle: after its tick the caller gives each moving block to `pushByMovingPiston`, on `NativeBoatPhysics`
and `NativeRiddenMobPhysics` as on `NativePhysics`. Getting off is the server's: it picks the place by
`getDismountLocationForPassenger` and sends a position packet, so a dismount is a teleport here, and nothing about it needs
predicting. `v_dismount_*` and `c_ride_*_dismount` show that packet arrive on the tick it takes effect.

## The world

The engine reads only its `NativeWorld`, and never a caller's world. It keeps no connection to a server. Two kinds of caller
fill a `NativeWorld`: a direct caller with its own writes, and `NativeWorldSync` (`src/mineflayer/world.ts`), which copies a
prismarine world into one. The code blocks below run in `test/doc-examples.test.cjs`.

### Who owns it

| Thing | Owner | Ends |
| --- | --- | --- |
| A direct `NativeWorld` | the caller | With its last reference. It has no `free` or `dispose` of its own |
| `sync.world` | the sync | A new source or the cache limit replaces it, so do not keep a reference across ticks |
| A sync of an engine | the engine's `WorldLink` | `releaseWorld(world)`, `dispose()`, a bot's `end`, or the bot moving to another world |
| A sync you made yourself | you | `sync.dispose()`. It takes the listeners off the source. `sync.world` stays readable, and `sync` then throws |

Cached blocks are held per sync, up to 2^18 cells. Past that the sync forgets them all and starts a new `NativeWorld`.

### The order of a tick

An adapter tick runs these before the move (`NativePhysics.tick`):

1. `sync.sync(world, reachOf(state), 0)`: the blocks.
2. `syncMovingBlocks`: the moving pistons and open shulker lids, whole lists, every tick.
3. `sync.setWorldBorder(bot.worldBorder, bot.game)`: the border and the level's height.

A block update must reach the world before the tick that reads it, in packet order (`docs/boundary-inputs.md`).

### What must be synced

A tick reads every cell its box sweeps. The adapter's regions:

| Path | Box | Margin |
| --- | --- | --- |
| Player (`reachOf`) | The standing box, joined with the box after `vel`, grown by 1, plus 4 for a riptide launch, plus 2 with rockets, plus 8 times the horizontal speed with auto-jump. Upward it also adds the step height | 0 |
| Boat, ridden mob, minecart (`syncAround`) | The entity's box | 3 + the largest axis of its motion, rounded up |
| Mob travel, moving entity | The entity's box | as above |
| Projectile | The box from `position` to `position + velocity` | 2 |

A sync reads one cell more around the region, and two on 1.12 (`BlockMapper.reach`), since the shapes of the outer cells come
from their neighbours. Only the region itself is written to the `NativeWorld`.

A region of more than 2^22 cells is refused. `syncReads(box, margin)` says so beforehand. A player's tick then skips and
sets `state.moveRefused` (`docs/numeric-contract.md`). Other paths throw.

### Unknown is not air, until it reaches the engine

`NativeWorldSync` tells a cell it could not read from air. The `NativeWorld` then holds nothing for that cell, and to the engine an
absent cell reads as air with no fluid, as vanilla's `EmptyLevelChunk` does. What differs is the chunk's loaded state:

| The source says | The sync | `isChunkLoaded` | Reads as |
| --- | --- | --- | --- |
| `getColumnAt` gives nothing | Unknown. Marks a known column unloaded | false | Air, no fluid, no block entity, no section |
| A column, `getBlock` gives nothing | Unknown. Writes nothing | true if another cell of it was read | Air |
| A column, air | Written as nothing, or a removal | true | Air |
| A column, a block | Written | true | The block |
| No `getColumnAt` at all | Treats a block as loaded, and a null block as unknown | as above | It cannot tell a gone column |

In a raw `NativeWorld`:

| State | `isChunkLoaded` | Writes |
| --- | --- | --- |
| Never touched | false | The first block written loads it |
| `setChunkLoaded(x, z, true)` | true | Stored |
| `setChunkLoaded(x, z, false)` | false | Ignored until it loads, as a write to `EmptyLevelChunk` is. Blocks already stored stay, read as air, and are back when it loads |

Loaded state changes behaviour in places, so a caller must set it to reproduce a run:

- from 26.1, fluids are not touched while a chunk near the box is not loaded (`fluid_interaction_loaded`, `native/src/world.rs`);
- before 1.14 the border's stone cells exist only in loaded chunks, and a player over an unloaded chunk sinks
  (`test/mineflayer.test.cjs`, `lets a player in a chunk column the world has not loaded sink as 1.8 does`).

Not modelled: 1.8.x's `EmptyChunk`, which keeps what is set in it, and 1.13.x's, which gives back the fluid set in it.

A section is apart from this. `setSectionPresent` says whether a section exists, and an all-air section the column keeps is present
where one it left out is not. The sync reads that from the column's `sections` when it has them.

### Worlds with events and without

`typeof source.on === 'function'` decides, once, when a sync meets a source.

| | Events | Polling |
| --- | --- | --- |
| Reads each sync | Cells new to the region, and those the world said changed | Every cell of the region and its neighbours |
| A sync with nothing new | Returns at once | Reads it all again |
| Listens to | `blockUpdate`, `chunkColumnLoad`, `chunkColumnUnload` | Nothing |
| A column gone | The event | Found by `getColumnAt` on the next read |

A world that has `on` must really emit. One that does not goes stale without an error. Such a caller must call
`sync.blockChanged(pos)` and `sync.columnChanged(chunkX, chunkZ, loaded)` itself.

A world for these examples, with its events on or off, on 1.21.1:

```js doctest
const registry = require('prismarine-registry')('1.21.1');
const Block = require('prismarine-block')(registry);
const stone = registry.blocksByName.stone.defaultState;
const air = registry.blocksByName.air.defaultState;
function makeWorld(events) {
    const blocks = new Map();
    const gone = new Set();
    const cell = pos => `${Math.floor(pos.x)},${Math.floor(pos.y)},${Math.floor(pos.z)}`;
    const column = pos => `${Math.floor(pos.x) >> 4},${Math.floor(pos.z) >> 4}`;
    const base = {
        getColumnAt: pos => (gone.has(column(pos)) ? null : {}),
        getBlock: pos => (gone.has(column(pos)) ? null : Block.fromStateId(blocks.get(cell(pos)) ?? air, 0)),
        getBlockStateId: pos => (gone.has(column(pos)) ? 0 : blocks.get(cell(pos)) ?? air),
    };
    const world = events ? Object.assign(new EventEmitter(), base) : base;
    world.place = (x, y, z, state) => {
        blocks.set(`${x},${y},${z}`, state);
        if (events) world.emit('blockUpdate', null, { position: { x, y, z } });
    };
    world.unload = (cx, cz) => {
        gone.add(`${cx},${cz}`);
        if (events) world.emit('chunkColumnUnload', { x: cx * 16, y: 0, z: cz * 16 });
    };
    world.load = (cx, cz) => {
        gone.delete(`${cx},${cz}`);
        if (events) world.emit('chunkColumnLoad', { x: cx * 16, y: 0, z: cz * 16 });
    };
    return world;
}
const box = { minX: 0.2, minY: 1, minZ: 0.2, maxX: 0.8, maxY: 2.8, maxZ: 0.8 };
const inCell = { minX: 0.1, minY: 0.1, minZ: 0.1, maxX: 0.9, maxY: 0.9, maxZ: 0.9 };
```

A block change. With events the world says so. Polled, the next sync finds it:

```js doctest
for (const events of [true, false]) {
    const world = makeWorld(events);
    const sync = new NativeWorldSync(binding, registry);
    sync.sync(world, box, 3);
    assert.equal(sync.collisions(inCell).length, 0);
    world.place(0, 0, 0, stone);
    sync.sync(world, box, 3);
    assert.equal(sync.collisions(inCell).length, 1);
    sync.dispose();
}
```

A chunk unloads and comes back changed. While it is gone it reads as air and is not loaded. What comes back is the new column:

```js doctest
for (const events of [true, false]) {
    const world = makeWorld(events);
    world.place(0, 0, 0, stone);
    const sync = new NativeWorldSync(binding, registry);
    sync.sync(world, box, 3);
    assert.equal(sync.world.isChunkLoaded(0, 0), true);

    world.unload(0, 0);
    sync.sync(world, box, 3);
    assert.equal(sync.world.isChunkLoaded(0, 0), false);
    assert.equal(sync.collisions(inCell).length, 0);

    world.place(0, 0, 0, air);
    world.load(0, 0);
    sync.sync(world, box, 3);
    assert.equal(sync.world.isChunkLoaded(0, 0), true);
    assert.equal(sync.collisions(inCell).length, 0);
    sync.dispose();
}
```

A new source. `sync` forgets the old one, takes its listeners off, and makes a new `NativeWorld`. A change to the old source is
then ignored, and a state ticked in the old world needs `setState` to move (`docs/state-contract.md`):

```js doctest
const first = makeWorld(true);
const second = makeWorld(true);
first.place(0, 0, 0, stone);
const sync = new NativeWorldSync(binding, registry);
sync.sync(first, box, 3);
const before = sync.world;
assert.equal(sync.collisions(inCell).length, 1);

sync.sync(second, box, 3);
assert.notEqual(sync.world, before);
assert.equal(sync.collisions(inCell).length, 0);
assert.equal(first.listenerCount('blockUpdate'), 0);
sync.dispose();
assert.equal(second.listenerCount('blockUpdate'), 0);
```

### Changes that spread

A change makes the cells around it out of date, one cell away, two on 1.12. They are written again on the next sync. Their fluid
flow, a trapdoor over a ladder, and before 1.13 their fence, door and chest shapes depend on it. Pre-1.13 blocks arrive as
numeric id and metadata, and `src/mineflayer/legacy.ts` works out their connections. From 1.13 a state id says it all, but for
those neighbour rules. A direct caller must rewrite neighbours itself.

A block the engine refuses stays out, and `sync.refused` says which and why. The cells around it are unaffected
(`test/chunk-lifecycle.test.cjs`).

### Border and height

`setWorldBorder(border, game)` writes `setWorldBounds` when something changed. A border not usable leaves the last one, and
none known leaves the default (centre 0, 0, size 59999968). The level's height comes from `game.minY` and `game.height` when
the server told them. Otherwise it is the release's default: 0 and 256, or -64 and 384 in the overworld from 1.18. The
engine's move does not read the height as a wall. A cell outside it is air.

`previousSize`, where finite, stands for the size. The comment cites a moving border's size before its last tick from 1.21.11. The
code does not gate it by release, so a caller on an older release must not pass it. Its behaviour has not been checked against
each release.

### To reproduce a tick's world

A direct caller gets the same world as a sync when it does all of this before the tick:

1. Writes every cell of the region above with `applyBlockUpdates`, in packet order. A batch applies whole or not at all.
2. Gives each cell the shape its neighbours make, and rewrites the neighbours of a change.
3. Calls `setChunkLoaded` for each chunk around the region, loaded or not, and `setSectionPresent` for present empty sections.
4. Calls `setWorldBounds`.
5. Gives moving pistons, shulker lids, entities and rockets for that tick. These are not kept (`docs/boundary-inputs.md`).

The recorder's `replay/world.ts` makes a prismarine world of a recording and goes through `NativeWorldSync`, so the replay holds to the
same path. `world.contentDigest()` compares two worlds. Whether a direct caller and the sync give the same digest for one
world has not been tested.

## Not guaranteed

Moving entities other than the player's vehicle follow `simulateVersionedMovingEntityTick`, whose caller keeps the state. No universal
synchronization boundary was found across releases.
