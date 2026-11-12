// A world's columns through their life: gone, changed while gone and back, around the origin where a floor and a truncation
// differ, event-driven and rescanned. What is checked is what the engine then holds: shapes, fluids, loaded flags and the
// ticks it runs over them, not the cache. Run after `pnpm build` and the native build: node --test test/
const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const adapter = require('..');

const VERSION = '1.21.1';
const registry = require('prismarine-registry')(VERSION);
const Block = require('prismarine-block')(registry);
const id = name => registry.blocksByName[name].defaultState;
const key = (x, y, z) => `${x},${y},${z}`;
const column = pos => `${Math.floor(pos.x) >> 4},${Math.floor(pos.z) >> 4}`;

/** A prismarine world of columns: events off, it says nothing and is read again at each sync (the rescan). */
function columnWorld(events = true, blocks = new Map()) {
    const gone = new Set();
    const air = id('air');
    const stateAt = pos => blocks.get(key(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z))) ?? air;
    const base = {
        blocks, gone,
        getColumnAt: pos => (gone.has(column(pos)) ? null : {}),
        getBlock: pos => (gone.has(column(pos)) ? null : Block.fromStateId(stateAt(pos), 0)),
        getBlockStateId: pos => (gone.has(column(pos)) ? 0 : stateAt(pos)),
    };
    const world = events ? Object.assign(new EventEmitter(), base) : base;
    world.place = (x, y, z, state) => {
        blocks.set(key(x, y, z), state);
        if (events && !gone.has(column({ x, z }))) world.emit('blockUpdate', null, { position: new Vec3(x, y, z) });
    };
    world.unload = (cx, cz) => {
        gone.add(`${cx},${cz}`);
        if (events) world.emit('chunkColumnUnload', new Vec3(cx * 16, 0, cz * 16));
    };
    world.load = (cx, cz) => {
        gone.delete(`${cx},${cz}`);
        if (events) world.emit('chunkColumnLoad', new Vec3(cx * 16, 0, cz * 16));
    };
    return world;
}

const newSync = () => new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
const around = (x, z) => ({ minX: x - 0.3, minY: 1, minZ: z - 0.3, maxX: x + 0.3, maxY: 2.8, maxZ: z + 0.3 });
const inCell = (x, z) => ({ minX: x + 0.1, minY: 1.1, minZ: z + 0.1, maxX: x + 0.9, maxY: 1.9, maxZ: z + 0.9 });

describe('a block the engine refuses', () => {
    it('leaves a gap the sync reports, and spoils nothing around it', () => {
        const world = columnWorld();
        world.place(0, 1, 0, id('stone'));
        world.place(1, 1, 0, id('stone'));
        // (a class, as the sync makes a new NativeWorld for a new source; one cell, refused on every try that writes it)
        const native = adapter.LoadNativePhysics();
        class Refusing extends native.NativeWorld {
            applyBlockUpdates(updates) {
                if (updates.some(update => update.x === 0 && update.y === 1 && update.z === 0 && !update.remove)) throw new Error('refused');
                super.applyBlockUpdates(updates);
            }
        }
        const sync = new adapter.NativeWorldSync(Object.assign(Object.create(native), { NativeWorld: Refusing }), registry);
        sync.sync(world, around(0.5, 0.5), 3);
        assert.deepEqual([...sync.refused], [['0,1,0', 'refused']]);
        assert.equal(sync.collisions(inCell(0, 0)).length, 0);
        assert.equal(sync.collisions(inCell(1, 0)).length, 1);
    });
});

describe('chunk columns through unload and reload', () => {
    // (a cell and its column: -1 and -16 are column -1, -17 is column -2, 15 column 0 and 16 column 1)
    for (const [x, z, cx, cz] of [[-1, -1, -1, -1], [-16, -16, -1, -1], [-17, 5, -2, 0], [15, -1, 0, -1], [16, 16, 1, 1]]) {
        it(`leaves no shape, fluid or loaded flag of a column replaced at ${x}, ${z}`, () => {
            const world = columnWorld();
            world.place(x, 1, z, id('stone'));
            const waterZ = z + 1 === 0 ? z - 1 : z + 1;
            world.place(x, 1, waterZ, id('water'));
            const sync = newSync();
            sync.sync(world, around(x + 0.5, z + 0.5), 3);
            assert.equal(sync.world.isChunkLoaded(cx, cz), true);
            assert.equal(sync.collisions(inCell(x, z)).length, 1);
            assert.notEqual(sync.world.fluidAt(x, 1, waterZ), 0);

            world.unload(cx, cz);
            sync.sync(world, around(x + 0.5, z + 0.5), 3);
            assert.equal(sync.world.isChunkLoaded(cx, cz), false);
            assert.equal(sync.collisions(inCell(x, z)).length, 0);
            assert.equal(sync.world.blockAt(x, 1, z).present, false);
            assert.equal(sync.world.fluidAt(x, 1, waterZ), 0);

            // while it is gone the column changes: what comes back is the new column
            world.blocks.delete(key(x, 1, z));
            world.blocks.delete(key(x, 1, waterZ));
            world.blocks.set(key(x, 2, z), id('oak_slab'));
            world.load(cx, cz);
            sync.sync(world, around(x + 0.5, z + 0.5), 3);
            assert.equal(sync.world.isChunkLoaded(cx, cz), true);
            assert.equal(sync.collisions(inCell(x, z)).length, 0);
            assert.equal(sync.world.fluidAt(x, 1, waterZ), 0);
            const slab = sync.collisions({ ...inCell(x, z), minY: 2.1, maxY: 2.4 });
            assert.deepEqual(slab.map(box => [box.min.y, box.max.y]), [[2, 2.5]]);
        });
    }

    it('keeps its neighbours through repeated cycles', () => {
        const world = columnWorld();
        world.place(-1, 1, 0, id('stone'));
        world.place(0, 1, 0, id('stone'));
        const sync = newSync();
        const box = around(0, 0.5);
        sync.sync(world, box, 3);
        for (let cycle = 0; cycle < 5; cycle += 1) {
            world.unload(-1, 0);
            sync.sync(world, box, 3);
            assert.equal(sync.world.isChunkLoaded(-1, 0), false, `cycle ${cycle}`);
            assert.equal(sync.collisions(inCell(-1, 0)).length, 0, `cycle ${cycle}`);
            assert.equal(sync.collisions(inCell(0, 0)).length, 1, `the column beside it, cycle ${cycle}`);
            world.load(-1, 0);
            sync.sync(world, box, 3);
            assert.equal(sync.collisions(inCell(-1, 0)).length, 1, `cycle ${cycle}`);
        }
    });

    it('takes no block change of a column while it is gone, and finds it when it is back', () => {
        const world = columnWorld();
        const sync = newSync();
        const box = around(16.5, 0.5);
        sync.sync(world, box, 3);
        world.unload(1, 0);
        world.place(16, 1, 0, id('stone'));
        sync.sync(world, box, 3);
        assert.equal(sync.collisions(inCell(16, 0)).length, 0);
        world.load(1, 0);
        sync.sync(world, box, 3);
        assert.equal(sync.collisions(inCell(16, 0)).length, 1);
    });

    it('forgets the blocks of the old provider for a new one', () => {
        const first = columnWorld();
        first.place(0, 1, 0, id('stone'));
        const second = columnWorld();
        second.place(1, 1, 0, id('water'));
        const sync = newSync();
        const box = around(0.5, 0.5);
        sync.sync(first, box, 3);
        assert.equal(sync.collisions(inCell(0, 0)).length, 1);
        sync.sync(second, box, 3);
        assert.equal(sync.collisions(inCell(0, 0)).length, 0);
        assert.notEqual(sync.world.fluidAt(1, 1, 0), 0);
    });
});

describe('an event-driven world and a rescanned one', () => {
    it('hold the same shapes, fluids and flags at every step across a column edge', () => {
        const worlds = [columnWorld(true), columnWorld(false)];
        const syncs = [newSync(), newSync()];
        const seen = [[], []];
        const steps = [
            w => w.place(-1, 1, 0, id('stone')),
            w => w.place(0, 1, 1, id('water')),
            w => w.unload(-1, 0),
            w => w.place(-1, 1, 0, id('air')),
            w => w.place(0, 2, 0, id('oak_slab')),
            w => w.load(-1, 0),
            w => w.unload(0, 0),
            w => w.load(0, 0),
        ];
        worlds.forEach((world, side) => {
            for (const step of steps) {
                step(world);
                syncs[side].sync(world, around(0, 0.5), 3);
                const sync = syncs[side];
                seen[side].push({
                    west: sync.world.isChunkLoaded(-1, 0),
                    east: sync.world.isChunkLoaded(0, 0),
                    shapes: sync.collisions({ minX: -2, minY: 0, minZ: -2, maxX: 2, maxY: 3, maxZ: 2 }).map(b => [b.min.x, b.min.y, b.max.y]).sort(),
                    fluid: sync.world.fluidAt(0, 1, 1),
                });
            }
        });
        assert.deepEqual(seen[0], seen[1]);
        assert.equal(seen[0].length, steps.length);
        // and the steps did something: the stone was there, then not, the slab came
        assert.notDeepEqual(seen[0][0].shapes, seen[0][7].shapes);
        assert.equal(seen[0][2].west, false);
        assert.equal(seen[0][5].west, true);
    });

    const walkOff = (version, reg) => {
        const native = adapter.LoadNativePhysics();
        const air = reg.blocksByName.air.defaultState ?? 0;
        const world = Object.assign(new EventEmitter(), {
            getColumnAt: () => null,
            getBlock: () => null,
            getBlockStateId: () => air,
        });
        const sync = new adapter.NativeWorldSync(native, reg);
        const at = { x: 0.5, y: 80, z: 0.5 };
        sync.sync(world, { minX: 0.2, minY: 80, minZ: 0.2, maxX: 0.8, maxY: 81.8, maxZ: 0.8 }, 3);
        const state = {
            ...native.createSimulationState({ position: at, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: false, yaw: 0, inWater: false, inLava: false }),
            isPlayer: true, clientSide: true, controlledByLocalInstance: true,
        };
        return native.simulateVersionedTick(version, sync.world, state, {}).velocity.y;
    };

    // (only the legacy client sinks in a column it has not got: 1.8.9 LivingEntity.java:1186-1190, 1.12.2 :1548; from 1.14
    // ClientWorld.isChunkLoaded answers true for every chunk, 1.14 ClientWorld.java:212)
    it('sinks a player at -0.1 in a column the client has not got only before 1.14', () => {
        assert.equal(walkOff('1.12.2', require('prismarine-registry')('1.12.2')), -0.1 * Math.fround(0.98));
        assert.notEqual(walkOff('1.21.1', registry), -0.1 * Math.fround(0.98));
    });
});
