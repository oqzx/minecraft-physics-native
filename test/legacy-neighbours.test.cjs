'use strict';

// B152, B073: the shapes neighbours give a block before 1.14, against the releases' own code. test/fixtures/legacy-neighbours
// is scripts/java-oracle/LegacyNeighbourDump.java, which found by itself which blocks depend on what around them: every
// stored state of a release at each cell around them, then those cells together, then the chains through a neighbour, then
// random stored states at all the cells a block reads at once (B161: not one of each kind alone). Redstone wire's cells are those
// it climbs to as well. What is compared is what the mapper hands the engine.

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { describe, it } = require('node:test');
const { Vec3 } = require('vec3');
const adapter = require('..');
const { RELEASES, differences, dependents, aloneOutlines, combos } = require('../scripts/java-oracle/legacy-neighbours.cjs');

const READS_NEIGHBOURS = /stairs$|_door$|^iron_door$|fence|pane$|^iron_bars$|_wall$|chest$|^chorus_plant$|^redstone_wire$|^vine$/;

describe('neighbours of the releases before 1.14, every stored state at each cell and the cells together', () => {
    for (const version of RELEASES) {
        it(version, () => {
            const { compared, collision, outline } = differences(version);
            if (process.env.NEIGHBOUR_REPORT) {
                console.log(version, compared);
                for (const [name, { count, first }] of [...collision].slice(0, 30)) console.log('  collision', name, count, first.slice(0, 300));
                for (const [name, { count, first }] of [...outline].slice(0, 30)) console.log('  outline', name, count, first.slice(0, 300));
            }
            assert.deepEqual([...collision.keys()], []);
            assert.deepEqual([...outline.keys()], []);
            if (version !== '1.13.2') assert.ok(compared > 1000000, `${compared}`);
        });
    }
});

describe('the outline of every stored state with air around it', () => {
    for (const version of RELEASES) {
        it(version, () => {
            const { compared, differ } = aloneOutlines(version);
            assert.deepEqual([...differ], []);
            assert.ok(compared > 1000, `${compared}`);
        });
    }
});

describe('random neighbours at every cell at once', () => {
    it('are drawn for each block that reads its neighbours, the wire with the cells it climbs to', () => {
        for (const version of RELEASES.filter(release => release !== '1.13.2')) {
            const drawn = combos(version);
            const homes = new Set(drawn.map(({ home }) => home));
            assert.ok(drawn.reduce((sum, { count }) => sum + count, 0) > 40000, version);
            assert.ok(homes.size > 100, version);
            if (version !== '1.8.8') {
                const wire = drawn.find(({ cells }) => cells.includes('0,1,0') && cells.includes('1,-1,0'));
                assert.ok(wire && ['-1,1,0', '1,1,0', '0,1,-1', '0,1,1'].every(cell => wire.cells.includes(cell)), version);
            }
            // (a fence reads a stair beside it, which reads the cells around it: the ring two away)
            assert.ok(drawn.some(({ cells }) => cells.includes('2,0,0') && cells.includes('1,0,1')), version);
        }
    });
});

describe('which blocks read their neighbours', () => {
    it('are those the mapper reads them for, found by the jars and not listed by hand', () => {
        for (const version of RELEASES) {
            const { names } = dependents(version);
            assert.deepEqual(names.filter(name => !READS_NEIGHBOURS.test(name)), [], version);
        }
        assert.ok(dependents('1.12.2').names.includes('chest'));
        assert.ok(!dependents('1.8.8').names.includes('chorus_plant'));
    });

    it('a block has its state as its shape from the flattening', () => {
        assert.deepEqual(dependents('1.13.2').names, []);
    });

    it('leaves out only what needs a block entity', () => {
        for (const version of RELEASES) {
            assert.deepEqual(dependents(version).refused.filter(name => !/piston|shulker/.test(name)), [], version);
        }
    });
});

describe('what the world keeps of them', () => {
    function worldOf(version, placed) {
        const registry = require('prismarine-registry')(version);
        const Block = require('prismarine-block')(registry);
        const blocks = new Map(placed(registry));
        const stateAt = pos => blocks.get(`${Math.floor(pos.x)},${Math.floor(pos.y)},${Math.floor(pos.z)}`) ?? 0;
        const world = Object.assign(new EventEmitter(), {
            getBlock: pos => Block.fromStateId(stateAt(pos), 0),
            getBlockStateId: stateAt,
            setBlock(x, y, z, id) {
                blocks.set(`${x},${y},${z}`, id);
                world.emit('blockUpdate', null, { position: new Vec3(x, y, z) });
            },
        });
        return { registry, world };
    }
    const around = { minX: -2, minY: 0, minZ: -2, maxX: 3, maxY: 3, maxZ: 3 };
    const fence = (registry) => [['0,1,0', registry.blocksByName.fence.id << 4], ['1,1,0', registry.blocksByName.stone.id << 4]];

    it('a fence lets go of a neighbour broken and takes it back', () => {
        const { registry, world } = worldOf('1.12.2', fence);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, around, 1);
        assert.equal(Math.max(...sync.boxesAt(0, 1, 0).map(box => box[3])), 1);
        world.setBlock(1, 1, 0, 0);
        sync.sync(world, around, 1);
        assert.equal(Math.max(...sync.boxesAt(0, 1, 0).map(box => box[3])), 0.625);
        world.setBlock(1, 1, 0, registry.blocksByName.stone.id << 4);
        sync.sync(world, around, 1);
        assert.equal(Math.max(...sync.boxesAt(0, 1, 0).map(box => box[3])), 1);
    });

    it('the outline of a wire follows a wire placed beside it, below it and above it, and lets go when it goes', () => {
        const wire = registry => registry.blocksByName.redstone_wire.id << 4;
        const { registry, world } = worldOf('1.12.2', registry => [['0,1,0', wire(registry)], ['0,0,0', registry.blocksByName.stone.id << 4]]);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        const width = () => {
            sync.sync(world, around, 1);
            const boxes = sync.world.outlineShapes({ x: -1, y: 1, z: -1 }, { x: 2, y: 2, z: 2 }, '1.12.2').filter(box => box.min.y === 1 && box.max.y === 1.0625);
            return boxes.map(box => [box.min.x, box.max.x, box.min.z, box.max.z]);
        };
        assert.deepEqual(width(), [[0.1875, 0.8125, 0.1875, 0.8125]]);
        // a wire to the east: the run goes through west to east (1.12.2 RedStoneWireBlock.java:117-139)
        world.setBlock(1, 1, 0, wire(registry));
        assert.deepEqual(width()[0], [0, 1, 0.1875, 0.8125]);
        world.setBlock(1, 1, 0, 0);
        // a wire down the side to the north, beside no normal cube
        world.setBlock(0, 0, -1, wire(registry));
        assert.deepEqual(width()[0], [0.1875, 0.8125, 0, 1]);
        world.setBlock(0, 0, -1, 0);
        // up a stone to the south onto a wire on it, none above this one
        world.setBlock(0, 1, 1, registry.blocksByName.stone.id << 4);
        world.setBlock(0, 2, 1, wire(registry));
        assert.deepEqual(width()[0], [0.1875, 0.8125, 0, 1]);
        // and a stone above it stops the climb
        world.setBlock(0, 2, 0, registry.blocksByName.stone.id << 4);
        assert.deepEqual(width()[0], [0.1875, 0.8125, 0.1875, 0.8125]);
    });

    it('a vine takes the top plate under a block that holds it, and lets go of it', () => {
        const { registry, world } = worldOf('1.12.2', registry => [['0,1,0', registry.blocksByName.vine.id << 4]]);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        const outline = () => {
            sync.sync(world, around, 1);
            return sync.world.outlineShapes({ x: -1, y: 0, z: -1 }, { x: 2, y: 2, z: 2 }, '1.12.2').filter(box => box.min.y >= 1 && box.max.y <= 2).map(box => box.min.y);
        };
        assert.deepEqual(outline(), [1]);
        world.setBlock(0, 2, 0, registry.blocksByName.stone.id << 4);
        assert.deepEqual(outline(), [1.9375]);
        world.setBlock(0, 2, 0, 0);
        assert.deepEqual(outline(), [1]);
    });

    it('a ray meets a fence no higher than its block, where an entity meets it at 1.5', () => {
        const { registry, world } = worldOf('1.12.2', fence);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, around, 1);
        const min = { x: -1, y: 0, z: -1 };
        const max = { x: 2, y: 3, z: 2 };
        const collision = sync.world.collisionShapes(min, max, '1.12.2').filter(box => box.max.x <= 1);
        const outline = sync.world.outlineShapes(min, max, '1.12.2').filter(box => box.max.x <= 1);
        assert.equal(Math.max(...collision.map(box => box.max.y)), 2.5);
        assert.equal(Math.max(...outline.map(box => box.max.y)), 2);
    });
});
