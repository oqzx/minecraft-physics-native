'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

const piston = {
    x: 1, y: 101, z: 0, facing: 5, extending: true, source: false, progress: 0,
    shapes: [{ min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } }],
    bounces: true,
};

function state(fields = {}) {
    return {
        position: { x: 1.2, y: 101, z: 0.5 }, velocity: { x: 0.125, y: 0, z: 0 },
        halfWidth: Math.fround(0.3), height: Math.fround(1.8), stepHeight: 0,
        onGround: true, collidedHorizontally: false, collidedVertically: true,
        verticalCollisionBelow: true, isPlayer: true, clientSide: false,
        controlledByLocalInstance: false, movingPistons: [piston], ...fields,
    };
}

function push(version, input, world = new native.NativeWorld()) {
    return native.simulateVersionedPistonPush(version, world, input, {});
}

it('skips server-player slime bounce and displacement only from 1.16.2 through 26.2', () => {
    for (const version of ['1.16.1', '1.16.2', '1.16.5', '1.21.11', '26.2', '26.3']) {
        const input = state();
        const result = push(version, input);
        const skipped = !['1.16.1', '26.3'].includes(version);
        assert.equal(result.state.velocity.x, skipped ? 0.125 : 1, version);
        assert.equal(result.state.position.x, skipped ? 1.2 : 1.2 + 0.51, version);
        assert.equal(result.pistonDeltas.x, skipped ? 0 : 0.51, version);
        const ordinary = push(version, state({ movingPistons: [{ ...piston, bounces: false }] }));
        assert.equal(ordinary.state.position.x, 1.2 + 0.51, `${version}: ordinary push`);
        assert.equal(ordinary.state.velocity.x, 0.125, `${version}: ordinary motion`);
    }
});

it('uses explicit 26.3 simulation permission without suppressing piston displacement', () => {
    const cases = [
        { isPlayer: true, clientSide: true, controlledByLocalInstance: true, expected: 1 },
        { isPlayer: true, clientSide: true, controlledByLocalInstance: false, expected: 0.125 },
        { isPlayer: true, clientSide: false, controlledByLocalInstance: false, expected: 1 },
        { isPlayer: false, clientSide: false, controlledByLocalInstance: false, expected: 0.125 },
        { isPlayer: false, clientSide: false, controlledByLocalInstance: true, expected: 1 },
        { isPlayer: false, clientSide: true, controlledByLocalInstance: false, canSimulateMovement: true, expected: 1 },
        { isPlayer: false, clientSide: false, controlledByLocalInstance: true, canSimulateMovement: false, expected: 0.125 },
    ];
    for (const { expected, ...fields } of cases) {
        const result = push('26.3', state(fields));
        assert.equal(result.state.velocity.x, expected, JSON.stringify(fields));
        assert.equal(result.state.position.x, 1.2 + 0.51, JSON.stringify(fields));
        assert.equal(result.pistonDeltas.x, 0.51);
        assert.equal(result.state.canSimulateMovement, fields.canSimulateMovement);
    }
    const old = push('26.2', state({ clientSide: true, controlledByLocalInstance: true, canSimulateMovement: false }));
    assert.equal(old.state.velocity.x, 1, 'the new override does not change older slime rules');
});

it('keeps remote vertical flags and gates collision restitution when a piston presses into a wall', () => {
    const world = new native.NativeWorld();
    world.setFullBlock(2, 101, 0, 1);
    world.setFullBlock(2, 102, 0, 1);
    for (const version of ['26.2', '26.3']) {
        const result = push(version, state({ clientSide: true, movingPistons: [{ ...piston, bounces: false }] }), world);
        assert.equal(result.state.collidedHorizontally, true, version);
        assert.equal(result.state.velocity.x, 0.125, version);
        assert.equal(result.state.onGround, true, version);
        assert.equal(result.state.collidedVertically, true, version);
        assert.equal(result.state.verticalCollisionBelow, true, version);
        assert.ok(result.state.position.x > 1.2 && result.state.position.x < 1.71, version);
    }
    const simulated = push('26.3', state({
        clientSide: true, canSimulateMovement: true, movingPistons: [{ ...piston, bounces: false }],
    }), world);
    assert.ok(simulated.state.velocity.x === 0);
    // Simulation permission is distinct from local ownership of horizontal-move ground flags.
    assert.equal(simulated.state.onGround, true);
});
