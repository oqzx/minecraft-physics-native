'use strict';

// B004 and B019: the same state on each side gives each side's outputs, a correction does not apply a server effect
// twice, the same ordered events give the same trace, their order matters, and context that is missing stays visible.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { LoadNativePhysics } = require('..');

const native = LoadNativePhysics();

const air = (effect) => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 0, y: 1, z: 0, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect } }]);
    return world;
};
const player = extra => ({
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8,
    onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true, noGravity: true, ...extra,
});
const ticks = (version, world, state, count) => {
    let current = state;
    for (let tick = 0; tick < count; tick += 1) current = native.simulateVersionedTick(version, world, current, {});
    return current;
};

describe('authority', () => {
    it('deals periodic freeze damage on the server alone, from equal states', () => {
        const world = air(8);
        const state = player({ ticksFrozen: 140, tickCount: 39 });
        for (const version of ['1.17', '1.21.5', '26.3']) {
            assert.equal(native.simulateVersionedTick(version, world, { ...state, clientSide: false }, {}).freezeDamage, 1, version);
            assert.ok(!native.simulateVersionedTick(version, world, { ...state, clientSide: true }, {}).freezeDamage, version);
        }
    });

    it('takes the post-move fire reset to the server from 1.21.6, and keeps it on both sides of 1.21.5', () => {
        const world = new native.NativeWorld();
        const state = player({ position: { x: 0.5, y: 10, z: 0.5 }, remainingFireTicks: -20, fireImmune: true, inRain: true });
        const reset = (version, clientSide) => native.simulateVersionedTick(version, world, { ...state, clientSide }, {}).remainingFireTicks;
        assert.equal(reset('1.21.5', true), reset('1.21.5', false));
        assert.notEqual(reset('1.21.6', true), reset('1.21.6', false));
    });

    it('reads a ServerPlayer\'s fire draws, and asks none of a client', () => {
        const world = air(16);
        const client = native.simulateVersionedTick('26.3', world, player({ clientSide: true, remainingFireTicks: 200 }), {});
        const server = native.simulateVersionedTick('26.3', world, player({ clientSide: false, remainingFireTicks: 200, serverFireIncrements: [1, 2] }), {});
        assert.notEqual(client.remainingFireTicks, server.remainingFireTicks);
        assert.throws(() => native.simulateVersionedTick('26.3', world, player({ clientSide: false, remainingFireTicks: 200, serverFireIncrements: [] }), {}), /ran out/);
    });

    it('does not apply a server-only effect twice when a prediction is corrected', () => {
        const world = air(8);
        // the client predicts 45 ticks, the server's state corrects it, and the client carries on: no damage is its own
        const predicted = ticks('26.3', world, player({ clientSide: true, ticksFrozen: 140, tickCount: 0, freezeDamage: 0 }), 45);
        assert.ok(!predicted.freezeDamage);
        const authoritative = ticks('26.3', world, player({ clientSide: false, ticksFrozen: 140, tickCount: 0, freezeDamage: 0 }), 45);
        assert.equal(authoritative.freezeDamage, 1);
        const corrected = { ...predicted, position: authoritative.position, velocity: authoritative.velocity, ticksFrozen: authoritative.ticksFrozen };
        const onward = ticks('26.3', world, corrected, 30);
        assert.ok(!onward.freezeDamage);
        // and the server's own count, carried over a correction of its own, rises once per its period
        const server = ticks('26.3', world, authoritative, 35);
        assert.equal(server.freezeDamage, 2);
    });
});

describe('deterministic boundary inputs', () => {
    it('refuses a burning ServerPlayer that was given no draw, and leaves one that was given a draw as it was', () => {
        const world = air(16);
        const burning = extra => player({ clientSide: false, remainingFireTicks: 200, ...extra });
        for (const version of ['1.21.2', '26.3']) {
            assert.throws(() => native.simulateVersionedTick(version, world, burning({}), {}), /needs its fire draw/, version);
            assert.throws(() => native.simulateVersionedTicks(version, world, [burning({})], 1, {}), /needs its fire draw/, version);
            assert.equal(native.simulateVersionedTick(version, world, burning({ serverFireIncrement: 1 }), {}).remainingFireTicks, 200 - 1 + 1, version);
            assert.equal(native.simulateVersionedTick(version, world, burning({ serverFireIncrement: 2 }), {}).remainingFireTicks, 200 - 1 + 2, version);
            assert.equal(native.simulateVersionedTick(version, world, burning({ serverFireIncrements: [2] }), {}).remainingFireTicks, 200 - 1 + 2, version);
        }
        // no draw is read before 1.21.2, by a client, or by what is not a player, so none is asked for
        assert.doesNotThrow(() => native.simulateVersionedTick('1.21.1', world, burning({}), {}));
        assert.doesNotThrow(() => native.simulateVersionedTick('26.3', world, burning({ clientSide: true }), {}));
        assert.doesNotThrow(() => native.simulateVersionedTick('26.3', world, burning({ isPlayer: false }), {}));
    });

    const box = (x, y, z) => ({ min: { x: x - 0.4, y, z: z - 0.4 }, max: { x: x + 0.4, y: y + 1, z: z + 0.4 } });
    const rocket = { kind: 'attached_rocket', position: { x: 0.5, y: 10, z: 0.5 }, boundingBox: box(0.5, 10, 0.5) };
    const cow = { kind: 'living', position: { x: 0.6, y: 10, z: 0.5 }, boundingBox: box(0.6, 10, 0.5) };
    const gliding = nearbyEntities => ({
        position: { x: 0.5, y: 10, z: 0.5 }, velocity: { x: 0.4, y: -0.1, z: 0.1 }, yaw: 0.4, pitch: -0.3, halfWidth: 0.3, height: 0.6,
        onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true, fallFlying: true, clientSide: true,
        nearbyEntities,
    });

    it('gives the same trace for the same ordered events', () => {
        const world = new native.NativeWorld();
        const run = () => ticks('26.3', world, gliding([cow, rocket]), 1);
        assert.deepEqual(run(), run());
        assert.deepEqual(
            ticks('26.3', world, { ...player({ clientSide: false, remainingFireTicks: 200 }), serverFireIncrements: [1, 2, 2] }, 2),
            ticks('26.3', world, { ...player({ clientSide: false, remainingFireTicks: 200 }), serverFireIncrements: [1, 2, 2] }, 2),
        );
    });

    it('gives a different trace when the order of the scene changes', () => {
        const world = new native.NativeWorld();
        const first = native.simulateVersionedTick('26.3', world, gliding([cow, rocket]), {});
        const second = native.simulateVersionedTick('26.3', world, gliding([rocket, cow]), {});
        assert.notDeepEqual(first.velocity, second.velocity);
    });

    it('gives a different trace when the random draws change', () => {
        const world = air(16);
        const drawn = draws => native.simulateVersionedTick('26.3', world, player({ clientSide: false, remainingFireTicks: 200, serverFireIncrements: draws }), {}).remainingFireTicks;
        assert.notEqual(drawn([1]), drawn([2]));
    });

    it('refuses a stream of draws that runs out, and takes an empty scene as no scene', () => {
        const world = air(16);
        assert.throws(() => native.simulateVersionedTick('26.3', world, player({ clientSide: false, remainingFireTicks: 200, serverFireIncrements: [] }), {}), /ran out/);
        const open = new native.NativeWorld();
        assert.deepEqual(native.simulateVersionedTick('26.3', open, gliding([]), {}).velocity, native.simulateVersionedTick('26.3', open, gliding(undefined), {}).velocity);
    });
});
