'use strict';

// Counters, recorded draws and what a teleport or a new dimension leaves of a player:
// docs/portals-and-teleports.md and docs/determinism.md.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics, encodeCheckpoint, decodeCheckpoint } = require('..');

const native = LoadNativePhysics();

function floorWorld(top = 1) {
    const world = new native.NativeWorld();
    const updates = [];
    for (let x = -8; x < 8; x++) {
        for (let z = -8; z < 8; z++) {
            updates.push({ x, y: top - 1, z, id: 1, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 } });
        }
    }
    world.applyBlockUpdates(updates);
    return world;
}

const stander = (fields = {}) => ({
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8,
    onGround: true, collidedHorizontally: false, collidedVertically: true, isPlayer: true,
    controlledByLocalInstance: true, clientSide: true, ...fields,
});

const outcome = (version, world, state) => native.simulateVersionedTickOutcome(version, world, state, {}).state;

it('every counter crosses one to zero in the tick that counts it, together when they expire together', () => {
    const world = floorWorld();
    for (const version of ['1.12.2', '1.20.4', '26.3']) {
        const after = outcome(version, world, stander({ jumpTicks: 1, autoJumpTime: 1, tickCount: 7 }));
        assert.equal(after.autoJumpTime, 0, version);
        assert.equal(after.tickCount, 8, version);
        // noJumpDelay goes to 0 at the top of aiStep, so autoJumpTime's jump key finds it free this very tick
        assert.equal(after.jumpTicks, 10, version);
        assert.ok(after.velocity.y > 0, `${version}: it jumps the tick both expire`);
        const held = outcome(version, world, stander({ jumpTicks: 2, jumpRequested: true }));
        assert.equal(held.jumpTicks, 1, `${version}: one tick is one count`);
        assert.ok(held.velocity.y <= 0, `${version}: still waiting`);
        // the jump key let go clears the wait
        assert.equal(outcome(version, world, stander({ jumpTicks: 9 })).jumpTicks, 0, version);
    }
});

it('tickCount wraps as a Java int, near the top, and a periodic effect keeps its place', () => {
    const world = new native.NativeWorld();
    const around = stander({ onGround: false, noGravity: true, clientSide: false, ticksFrozen: 200, freezeDamage: 0 });
    let state = { ...around, tickCount: 2 ** 31 - 2 };
    state = native.simulateVersionedTick('1.20.4', world, state, {});
    assert.equal(state.tickCount, 2 ** 31 - 1);
    state = native.simulateVersionedTick('1.20.4', world, state, {});
    assert.equal(state.tickCount, 2 ** 31, 'the bits of Integer.MIN_VALUE');
    state = native.simulateVersionedTick('1.20.4', world, { ...state, tickCount: 2 ** 32 - 1 }, {});
    assert.equal(state.tickCount, 0, 'minus one and one is nought');
});

it('a removed player ends its tick at the door from 1.19.1 and holds every other counter', () => {
    const world = floorWorld();
    const removed = stander({ removed: true, jumpTicks: 5, autoJumpTime: 3, tickCount: 2 });
    const after = outcome('26.3', world, removed);
    assert.equal(after.tickCount, 3, 'admitted once');
    assert.equal(after.jumpTicks, 5);
    assert.equal(after.autoJumpTime, 3);
    assert.deepEqual(after.position, removed.position);
});

it('a clone counts on from the same tick: periodic damage falls on the same ticks', () => {
    const world = floorWorld(-20);
    const frozen = stander({ onGround: false, noGravity: true, clientSide: false, ticksFrozen: 200, freezeDamage: 0, tickCount: 35, position: { x: 0.5, y: 50, z: 0.5 } });
    const original = new native.NativePlayer('1.20.4', frozen, {});
    const steps = player => {
        const seen = [];
        for (let tick = 0; tick < 10; tick++) {
            const state = outcome('1.20.4', world, { ...player.state() });
            player.setState(state);
            seen.push([state.tickCount, state.freezeDamage]);
        }
        return seen;
    };
    const clone = new native.NativePlayer('1.20.4', original.state(), {});
    assert.deepEqual(steps(clone), steps(original));
});

it('a teleport keeps motion and counters, drops the old support, and the next tick looks again', () => {
    const world = floorWorld();
    const high = floorWorld(30);
    for (const version of ['1.16.5', '1.20.4', '26.3']) {
        const landed = outcome(version, world, stander({ tickCount: 5, jumpTicks: 3, velocity: { x: 0.01, y: 0, z: 0 } }));
        // the caller's assignment: where the packet says, and no previous position
        const moved = { ...landed, position: { x: 3.5, y: 40, z: 3.5 }, previousPosition: undefined, boundingBox: undefined };
        const next = outcome(version, world, moved);
        assert.equal(next.onGround, false, `${version}: in the air now`);
        assert.equal(next.supportingBlock ?? null, null, `${version}: no block from before`);
        assert.equal(next.tickCount, landed.tickCount + 1, version);
        assert.equal(next.position.y < 40, true, `${version}: it falls from where it was put`);
        // a teleport onto another floor of the same level finds that floor
        const onto = outcome(version, high, { ...landed, position: { x: 3.5, y: 30, z: 3.5 }, previousPosition: undefined, boundingBox: undefined });
        assert.equal(onto.onGround, true, version);
    }
});

it('before 1.17 the kept box goes on until a teleport lets go of it', () => {
    const world = floorWorld();
    // a box that drifted off the position by rounding (Entity.setPosition's refit keeps none of this from 1.17)
    const drift = 1e-9;
    const kept = { min: { x: 0.2 + drift, y: 1, z: 0.2 }, max: { x: 0.8 + drift, y: 2.8, z: 0.8 } };
    const stay = outcome('1.16.5', world, { ...stander(), boundingBox: kept });
    assert.equal(stay.boundingBox.min.x, kept.min.x, 'the kept box is what it stands in');
    assert.equal(outcome('1.17', world, { ...stander(), boundingBox: undefined }).boundingBox ?? null, null, 'none from 1.17');
    // the caller drops it with the teleport (the adapter: keptFor at another position), and the box is the new place's
    const moved = outcome('1.16.5', world, { ...stander(), position: { x: 3.5, y: 40, z: 3.5 }, boundingBox: undefined });
    assert.ok(Math.abs(moved.boundingBox.min.x - 3.2) < 1e-6, 'around the new position');
});

it('a new dimension is a new world: the old one refuses the state and setState starts clean', () => {
    const overworld = floorWorld();
    const nether = floorWorld(32);
    const player = new native.NativePlayer('26.3', stander(), {});
    const input = new Float64Array(require('..').NATIVE_PLAYER_LAYOUT.inputLength);
    input[13] = 0.3;
    input[14] = 1.8;
    input[15] = 1.62;
    input[0] = 0.5;
    input[1] = 1;
    input[2] = 0.5;
    player.tick(overworld, input, new Float64Array(0));
    assert.throws(() => player.tick(nether, input, new Float64Array(0)), /another NativeWorld/);
    // LocalPlayer is replaced on a respawn: the caller seeds the new one, and nothing of the old is kept
    player.setState(stander({ position: { x: 0.5, y: 32, z: 0.5 } }));
    assert.equal(player.state().supportingBlock ?? null, null);
    const moved = input.slice();
    moved[1] = 32;
    player.tick(nether, moved, new Float64Array(0));
    assert.equal(player.state().position.y, 32);
});

it('standing in a portal is no state: repeated contact and leaving it change nothing of the player', () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 0, y: 1, z: 0, id: 9, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 29 } }]);
    const start = stander({ onGround: false, noGravity: true });
    let inside = start;
    for (let tick = 0; tick < 100; tick++) inside = native.simulateVersionedTick('26.3', world, inside, {});
    let outside = { ...start, tickCount: 0 };
    const empty = new native.NativeWorld();
    for (let tick = 0; tick < 100; tick++) outside = native.simulateVersionedTick('26.3', empty, outside, {});
    assert.deepEqual(inside, outside);
});

const fire = () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 0, y: 1, z: 0, id: 9, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 16 } }]);
    return world;
};

it('a server ignition reads one draw of either value, a client reads none, an older release has none', () => {
    const world = fire();
    const burning = (fields) => stander({ onGround: false, noGravity: true, remainingFireTicks: 200, ...fields });
    const server = serverFireIncrements => native.simulateVersionedTick('26.3', world, burning({ clientSide: false, serverFireIncrements }), {});
    const one = server([1, 2]);
    const two = server([2, 1]);
    assert.deepEqual(one.serverFireIncrements, [2]);
    assert.deepEqual(two.serverFireIncrements, [1]);
    assert.equal(two.remainingFireTicks - one.remainingFireTicks, 1, 'the draw is the increment');
    const client = native.simulateVersionedTick('26.3', world, burning({ clientSide: true, serverFireIncrements: [2, 2] }), {});
    assert.deepEqual(client.serverFireIncrements, [2, 2], 'LocalPlayer reads no draw');
    const old = native.simulateVersionedTick('1.20.4', world, burning({ clientSide: false, serverFireIncrements: [2, 2] }), {});
    assert.deepEqual(old.serverFireIncrements, [2, 2], 'no such draw before 1.21.2');
});

it('the same draws give the same run, through a clone and a checkpoint', () => {
    const world = fire();
    const start = stander({ onGround: false, noGravity: true, clientSide: false, remainingFireTicks: 200, serverFireIncrements: [1, 2, 2, 1, 2, 1, 1, 2] });
    const run = (state, target, ticks) => {
        const seen = [];
        let now = state;
        for (let tick = 0; tick < ticks; tick++) {
            now = outcome('26.3', target, now);
            seen.push(now.remainingFireTicks);
        }
        return { seen, now };
    };
    const first = run(start, world, 3);
    const rest = run(first.now, world, 3);
    assert.deepEqual(run(start, world, 3).seen, first.seen, 'again, the same');
    const clone = new native.NativePlayer('26.3', first.now, {}).state();
    assert.deepEqual(run(clone, world, 3).seen, rest.seen, 'a clone reads on');
    const restored = decodeCheckpoint(native, encodeCheckpoint(native, { version: '26.3', state: first.now, world }), '26.3');
    assert.deepEqual(run(restored.state, restored.world, 3).seen, rest.seen, 'a checkpoint reads on');
});

it('draws run out loudly, and more than a state holds are refused', () => {
    const world = fire();
    const state = stander({ onGround: false, noGravity: true, clientSide: false, remainingFireTicks: 200, serverFireIncrements: [] });
    assert.throws(() => native.simulateVersionedTick('26.3', world, state, {}), /draw|fire/i);
    assert.throws(() => native.simulateVersionedTick('26.3', world, { ...state, serverFireIncrements: [3] }, {}), /1 or 2|draw/i);
});
