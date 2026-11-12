'use strict';

// A shulker box's lid from the block event to the cell's collision and the player's push. The lid's numbers are compared
// with the real block entity in native/src/oracle_move_tests.rs; this is the path through the public entry points.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics, ShulkerBoxTracker, encodeCheckpoint, decodeCheckpoint } = require('..');

const native = LoadNativePhysics();
const UP = 1;

it('a block event opens the lid, the box grows towards FACING, and shutting it ends the record', () => {
    const lids = new ShulkerBoxTracker(native, '26.3');
    lids.event(3, 64, 3, UP, 1);
    for (let tick = 0; tick < 5; tick++) lids.tick();
    let [open] = lids.boxes();
    assert.ok(open.bounds.max.y > 1.2, 'half the progress above the cell');
    assert.equal(open.bounds.min.y, 0);
    for (let tick = 0; tick < 6; tick++) lids.tick();
    [open] = lids.boxes();
    assert.equal(open.bounds.max.y, 1.5, 'fully open');
    assert.equal(open.state.status, 2);
    lids.event(3, 64, 3, UP, 0);
    for (let tick = 0; tick < 11; tick++) lids.tick();
    assert.deepEqual(lids.boxes(), [], 'shut is a plain cube again: no state is left');
});

it('a count above one changes nothing, and a replaced block keeps no lid', () => {
    const lids = new ShulkerBoxTracker(native, '26.3');
    lids.event(0, 64, 0, UP, 1);
    lids.tick();
    const before = lids.boxes()[0].state;
    lids.event(0, 64, 0, UP, 2);
    assert.equal(lids.boxes()[0].state.status, before.status, 'a second viewer does not restart it');
    lids.forget(0, 64, 0);
    assert.deepEqual(lids.boxes(), []);
});

it('the lid lifts a player standing on it, on both paths, and walls it in the open cell', () => {
    const world = new native.NativeWorld();
    world.setBlockShapes(0, 63, 0, 1, [{ minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 }]);
    const start = {
        position: { x: 0.5, y: 64, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8,
        onGround: true, collidedHorizontally: false, collidedVertically: false, isPlayer: true,
    };
    const lids = new ShulkerBoxTracker(native, '26.3');
    lids.event(0, 63, 0, UP, 1);
    lids.tick();
    const [open] = lids.boxes();
    assert.equal(open.pushes, true);
    const object = native.simulateVersionedShulkerBoxPush('26.3', world, start, open.state, 0, 63, 0);
    assert.ok(object.position.y > 64, 'lifted');
    const player = new native.NativePlayer('26.3', start, {});
    const out = player.pushByShulkerBox(world, open.state, 0, 63, 0);
    assert.equal(out[1], object.position.y, 'the same on the packed path');
    // no shulker box before 1.11 (1.10.2 BlockEntity.java:243-263), on both paths
    assert.throws(() => native.simulateVersionedShulkerBoxPush('1.10.2', world, start, open.state, 0, 63, 0), /no shulker box/);
    assert.throws(() => new native.NativePlayer('1.10.2', start, {}).pushByShulkerBox(world, open.state, 0, 63, 0), /no shulker box/);
    // and 1.11.2 has it (BlockEntity.java:263, ShulkerBoxBlockEntity.java:58-63): its lid lifts the player on both paths
    const first = native.simulateVersionedShulkerBoxPush('1.11.2', world, start, open.state, 0, 63, 0);
    assert.ok(first.position.y > 64, 'lifted on 1.11.2');
    assert.equal(new native.NativePlayer('1.11.2', start, {}).pushByShulkerBox(world, open.state, 0, 63, 0)[1], first.position.y);
    const elsewhere = native.simulateVersionedShulkerBoxPush('26.3', world, { ...start, position: { x: 5.5, y: 64, z: 0.5 } }, open.state, 0, 63, 0);
    assert.equal(elsewhere.position.y, 64, 'a player out of its reach stays');
});

it('a column that unloads takes its lids with it, and only those', () => {
    const lids = new ShulkerBoxTracker(native, '26.3');
    lids.event(3, 64, 3, UP, 1);
    lids.event(-1, 64, 3, UP, 1);
    lids.event(40, 64, 3, UP, 1);
    lids.tick();
    lids.forgetColumn(0, 0);
    assert.deepEqual(lids.boxes().map(box => box.x).sort((a, b) => a - b), [-1, 40], 'the chunk at 0, 0 alone');
    lids.forgetColumn(-1, 0);
    assert.deepEqual(lids.boxes().map(box => box.x), [40], 'a negative cell is in the chunk below it');
    lids.event(3, 64, 3, UP, 0);
    assert.equal(lids.boxes().find(box => box.x === 3)?.state.progress, 0, 'it loads again shut');
});

it('a lid goes through a checkpoint, and bad states are refused', () => {
    const lids = new ShulkerBoxTracker(native, '26.3');
    lids.event(0, 64, 0, UP, 1);
    lids.tick();
    lids.tick();
    const [open] = lids.boxes();
    const world = new native.NativeWorld();
    const restored = decodeCheckpoint(native, encodeCheckpoint(native, { version: '26.3', kind: 'shulker_box', state: open.state, world }), '26.3', 'shulker_box');
    assert.deepEqual(restored.state, open.state);
    assert.deepEqual(native.simulateVersionedShulkerBoxTick('26.3', restored.state), native.simulateVersionedShulkerBoxTick('26.3', open.state));
    assert.throws(() => native.simulateVersionedShulkerBoxTick('26.3', { ...open.state, facing: 9 }), /facing/);
    assert.throws(() => native.simulateVersionedShulkerBoxTick('26.3', { ...open.state, status: 9 }), /status/);
    assert.throws(() => native.simulateVersionedShulkerBoxTick('1.10.2', open.state), /no shulker box/);
    assert.ok(native.simulateVersionedShulkerBoxTick('1.11.2', open.state).state.progress > open.state.progress, 'it ticks on 1.11.2');
    // the block event too: 1.10.2 registers no shulker box (BlockEntity.java:243-263), 1.11.2 does (:263)
    assert.throws(() => native.shulkerBoxBlockEvent('1.10.2', open.state, 1), /no shulker box/);
    assert.deepEqual(native.shulkerBoxBlockEvent('1.11.2', open.state, 1), native.shulkerBoxBlockEvent('26.3', open.state, 1));
});

it('an open lid is its cell\'s box on the JS side too, which the pose fit reads', () => {
    const { NativeWorldSync } = require('..');
    const registry = require('prismarine-registry')('26.3');
    const Block = require('prismarine-block')(registry);
    const box = registry.blocksByName.shulker_box.minStateId;
    const air = registry.blocksByName.air.defaultState;
    const stateAt = (x, y, z) => (x === 0 && y === 66 && z === 0 ? box : air);
    const source = { getBlock: pos => Block.fromStateId(stateAt(pos.x, pos.y, pos.z), 0), getBlockStateId: pos => stateAt(pos.x, pos.y, pos.z) };
    const sync = new NativeWorldSync(native, registry, '26.3');
    sync.sync(source, { minX: -2, minY: 63, minZ: -2, maxX: 2, maxY: 68, maxZ: 2 }, 1);
    assert.deepEqual(sync.boxesAt(0, 66, 0), [[0, 0, 0, 1, 1, 1]], 'shut, a cube');
    assert.equal(sync.shulkerLidAt(0, 66, 0), undefined);
    const bounds = { min: { x: 0, y: -0.25, z: 0 }, max: { x: 1, y: 1, z: 1 } };
    sync.setShulkerBoxes([{ x: 0, y: 66, z: 0, bounds }]);
    assert.deepEqual(sync.boxesAt(0, 66, 0), [[0, -0.25, 0, 1, 1, 1]], 'open downwards, out of its cell');
    assert.deepEqual(sync.shulkerLidAt(0, 66, 0), [0, -0.25, 0, 1, 1, 1]);
    sync.setShulkerBoxes([]);
    assert.equal(sync.shulkerLidAt(0, 66, 0), undefined, 'shut again');
});
