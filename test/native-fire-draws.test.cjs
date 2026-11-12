'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

function fireWorld() {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{
        x: 0, y: 1, z: 0,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 16 },
    }]); // Fire with an empty collision shape.
    return world;
}

const serverPlayer = (serverFireIncrements) => ({
    position: { x: 0.5, y: 1, z: 0.5 },
    velocity: { x: 0, y: 0, z: 0 },
    halfWidth: 0.3, height: 1.8,
    onGround: false, collidedHorizontally: false, collidedVertically: false,
    isPlayer: true, noGravity: true, clientSide: false,
    remainingFireTicks: 200, serverFireIncrements,
});

it('reads one ordered fire draw per ignition and returns those not read', () => {
    const world = fireWorld();
    let state = serverPlayer([1, 2, 2]);
    state = native.simulateVersionedTick('26.3', world, state, {});
    assert.deepEqual(state.serverFireIncrements, [2, 2]);
    state = native.simulateVersionedTick('26.3', world, state, {});
    assert.deepEqual(state.serverFireIncrements, [2]);
    assert.equal(state.remainingFireTicks, 201); // 200 - 2 + 1 + 2
    const [batched] = native.simulateVersionedTicks('26.3', world, [serverPlayer([1, 2, 2])], 2, {});
    assert.deepEqual(batched, state);
});

it('refuses a stream it cannot read and one that runs out', () => {
    const world = fireWorld();
    const tick = (state) => native.simulateVersionedTick('26.3', world, state, {});
    assert.throws(() => tick(serverPlayer([3])), /each 1 or 2/);
    assert.throws(() => tick({ ...serverPlayer([1]), serverFireIncrement: 2 }), /not both/);
    assert.throws(() => tick(serverPlayer([])), /ran out/);
});
