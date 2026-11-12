'use strict';

// Collision data crosses the N-API boundary without loss: the numbers of a shape come back by their
// bits, in order, on every channel. native/src/napi_api.rs's `collision_boundary_matches_the_direct_query`
// runs the same scene against the WorldSnapshot directly.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');

const native = LoadNativePhysics();

const thin = 1 / 3;
const near = 1e-7;
const shape = (minX, minY, minZ, maxX, maxY, maxZ) => ({ minX, minY, minZ, maxX, maxY, maxZ, slipperiness: 0.6 });
// a double no float holds, at a face
const first = shape(0, 0, 0, thin, 0.1 + 0.2, 1);
const second = shape(0.5 + near, 0, 0, 1, 1 - near, 1);

const cells = [[3, 4, 5], [-1, -60, -1], [-29999984, 70, 29999983], [1000000, 300, -1000000]];

function scene() {
    const world = new native.NativeWorld();
    world.setWorldBounds(-64, 320, 0, 0, 6e7);
    for (const [x, y, z] of cells) {
        world.setBlockShapes(x, y, z, 1, [first, second], [shape(0.25, 0, 0.25, 0.75, 1, 0.75)]);
    }
    return world;
}

const box = (x, y, z, s) => ({
    min: { x: x + s.minX, y: y + s.minY, z: z + s.minZ },
    max: { x: x + s.maxX, y: y + s.maxY, z: z + s.maxZ },
});

const around = ([x, y, z]) => [{ x: x - 0.5, y: y - 0.5, z: z - 0.5 }, { x: x + 1.5, y: y + 1.5, z: z + 1.5 }];

it('two boxes come back in order by their bits, at negative and large cells, on their own channel', () => {
    const world = scene();
    for (const cell of cells) {
        const [x, y, z] = cell;
        assert.deepEqual(world.collisionShapes(...around(cell)), [box(x, y, z, first), box(x, y, z, second)], `${cell}`);
        assert.deepEqual(world.outlineShapes(...around(cell)), [box(x, y, z, shape(0.25, 0, 0.25, 0.75, 1, 0.75))], `${cell}`);
    }
});

it('the same query twice answers alike, and a near-contact face keeps its gap', () => {
    const world = scene();
    const [x, y, z] = cells[0];
    const once = world.collisionShapes(...around(cells[0]));
    assert.deepEqual(world.collisionShapes(...around(cells[0])), once);
    // a query in the gap between the boxes of one cell has the whole shape, as BlockCollisions does
    const gap = world.collisionShapes({ x: x + 0.4, y, z }, { x: x + 0.5, y: y + 0.2, z: z + 1 });
    assert.deepEqual(gap, [box(x, y, z, first), box(x, y, z, second)]);
    assert.ok(gap[1].min.x - (x + 0.5) > 0.9 * near, 'the 1e-7 is still there');
});

it('the interaction channel and the support answer by their own shapes', () => {
    const world = new native.NativeWorld();
    const interaction = shape(0.25, 0, 0.25, 0.75, 0.5, 0.75);
    world.applyBlockUpdates([{
        x: -3, y: 4, z: 5, id: 1, shape: first, outlineShape: second, interactionShape: interaction,
    }]);
    const [min, max] = around([-3, 4, 5]);
    assert.deepEqual(world.interactionShapes(min, max), [box(-3, 4, 5, interaction)]);
    assert.deepEqual(world.outlineShapes(min, max), [box(-3, 4, 5, second)]);
    assert.deepEqual(world.collisionShapes(min, max), [box(-3, 4, 5, first)]);
    const standing = {
        position: { x: -2.8, y: 4 + first.maxY, z: 5.5 }, velocity: { x: 0, y: -0.1, z: 0 }, halfWidth: 0.1, height: 1.8,
        onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true, controlledByLocalInstance: true,
    };
    const after = native.simulateVersionedTickOutcome('26.3', world, standing, {}).state;
    assert.deepEqual(after.supportingBlock, { x: -3, y: 4, z: 5 });
    assert.equal(after.position.y, 4 + first.maxY, 'the support is the double, not a float of it');
});

it('a zero stays a zero and a bad number is refused, not replaced', () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{
        x: 0, y: 1, z: 0, id: 0, tags: 0, fluidTags: 0, fluidLevel: 0, bounceRestitution: 0,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0 },
    }]);
    const [only] = world.collisionShapes({ x: 0, y: 1, z: 0 }, { x: 1, y: 2, z: 1 });
    assert.deepEqual(only, box(0, 1, 0, shape(0, 0, 0, 1, 1, 1)));
    assert.throws(() => world.collisionShapes({ x: NaN, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }));
    assert.throws(() => world.collisionShapes({ x: 2, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }));
    assert.throws(() => world.setBlockShapes(0, 0, 0, 1, [{ ...first, maxX: Infinity }]));
    assert.throws(() => world.setBlockShapes(0.5, 0, 0, 1, [first]), /no block cell/);
    assert.throws(() => world.setBlockShapes(2 ** 33, 0, 0, 1, [first]), /no block cell/);
    assert.throws(() => world.applyBlockUpdates([{ x: 1.5, y: 0, z: 0 }]), /no block cell/);
});
