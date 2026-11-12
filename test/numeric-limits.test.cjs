'use strict';

// docs/numeric-contract.md's exceptional values through the object and packed paths alike.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics, NativePlayerInput: In } = require('..');

const native = LoadNativePhysics();

const world = () => {
    const made = new native.NativeWorld();
    const updates = [];
    for (let x = -4; x < 4; x++) {
        for (let z = -4; z < 4; z++) updates.push({ x, y: 0, z, id: 1, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 } });
    }
    made.applyBlockUpdates(updates);
    return made;
};

const state = (fields = {}) => ({
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, eyeHeight: 1.62,
    onGround: true, collidedHorizontally: false, collidedVertically: false, isPlayer: true, look: { x: 0, y: 0, z: 1 },
    input: { x: 0, y: 0, z: 0 }, ...fields,
});

function packed(of) {
    const values = new Float64Array(In.LENGTH);
    values.set([of.position.x, of.position.y, of.position.z], In.POSITION);
    values.set([of.velocity.x, of.velocity.y, of.velocity.z], In.VELOCITY);
    values[In.HALF_WIDTH] = of.halfWidth;
    values[In.HEIGHT] = of.height;
    values[In.EYE_HEIGHT] = of.eyeHeight;
    values[In.FLYING_SPEED] = 0.05;
    return values;
}

const outcome = (call) => {
    try {
        call();
        return null;
    } catch (error) {
        return error.message;
    }
};

const none = new Float64Array(0);

it('object and packed paths refuse the same thing, name it, and keep what they had', () => {
    for (const change of [
        [{ position: { x: Infinity, y: 1, z: 0.5 } }, /position\.x/],
        [{ position: { x: 1e300, y: 1, z: 0.5 } }, /position\.x/],
        [{ halfWidth: -1 }, /halfWidth/],
        [{ height: NaN }, /height/],
        [{ velocity: { x: 1e9, y: 0, z: 0 } }, /the tick's move/],
    ]) {
        const [fields, why] = change;
        const object = outcome(() => native.simulateVersionedTick('26.3', world(), state(fields), {}));
        assert.match(object ?? '', why, 'object');
        const player = new native.NativePlayer('26.3', state(), {});
        const before = player.state();
        const persistent = outcome(() => player.tick(world(), packed(state(fields)), none));
        assert.match(persistent ?? '', why, 'packed');
        assert.deepEqual(player.state(), before, 'the player is as it was');
    }
});

it('a finite motion that overflows inside the tick runs as Java would and leaves no half state', () => {
    // 1e308 * 1e308 is infinite in Java as here; the sweep then refuses a move it cannot walk
    const huge = state({ velocity: { x: 1e308, y: 1e308, z: 1e308 } });
    const refused = outcome(() => native.simulateVersionedTick('26.3', world(), huge, {}));
    assert.match(refused ?? '', /the tick's move/);
});

it('a motion that is not finite follows its release: refused from 1.21.11, taken before', () => {
    const notFinite = state({ velocity: { x: NaN, y: 0, z: 0 } });
    const modern = native.simulateVersionedTick('1.21.11', world(), notFinite, {});
    assert.equal(modern.velocity.x, 0, 'setDeltaMovement kept the initial zero');
    // before, no setter refuses it: the move drops a NaN x and its collision leaves 0.0, a NaN y stays NaN and does
    // not move the entity, as the real aiStep does on 1.21.10 (native/fixtures/java-oracle-living-1.21.10.txt, scene 900)
    const older = native.simulateVersionedTick('1.21.10', world(), notFinite, {});
    assert.ok(Object.is(older.velocity.x, 0));
    assert.equal(older.position.x, 0.5);
    const up = native.simulateVersionedTick('1.21.10', world(), state({ velocity: { x: 0, y: NaN, z: 0 } }), {});
    assert.ok(Number.isNaN(up.velocity.y));
    assert.equal(up.position.y, 1);
});

it('signed zero is a value, not a fault', () => {
    const negative = state({ velocity: { x: -0, y: 0, z: -0 }, onGround: false, noGravity: true });
    const after = native.simulateVersionedTick('26.3', new native.NativeWorld(), negative, {});
    assert.ok(Object.is(after.position.x, 0.5));
    assert.equal(after.velocity.x, 0);
});
