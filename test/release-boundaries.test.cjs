'use strict';

// B028: fixtures that tell the two sides of a boundary apart. A replay that matches on both sides shows nothing if no
// scenario reaches the branch that differs: moving the 1.21.5 fall-damage gate passed all 41 physrec/fd_* replays, so that
// arithmetic is pinned here, at a distance where the formulas give different answers.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { LoadNativePhysics } = require('..');

const native = LoadNativePhysics();

function floorWorld() {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 0, y: 10, z: 0, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 } }]);
    return world;
}
// A player resting on the floor's face: the tick's move is zero, so nothing is added to the distance it lands with.
const landing = (version, fallDistance) => native.simulateVersionedTick(version, floorWorld(), {
    position: { x: 0.5, y: 11, z: 0.5 }, velocity: { x: 0, y: -0.5, z: 0 }, halfWidth: Math.fround(0.3), height: Math.fround(1.8),
    onGround: false, collidedHorizontally: false, collidedVertically: false,
    isPlayer: true, clientSide: false, controlledByLocalInstance: true, fallDistance,
    safeFallDistance: 3, fallDamageMultiplier: 1,
}, {}).fallDamage;

describe('1.21.4 to 1.21.5: LivingEntity.calculateFallDamage', () => {
    // 1.21.4 LivingEntity.java:1657-1664: ceil((double)((fallDistance - (float)safe) * multiplier) * multiplier), in float
    // 1.21.5 LivingEntity.java:1672-1682: floor((fallDistance + 1.0E-6 - safe) * multiplier * multiplier), in double
    it('rounds up before and down after', () => {
        assert.equal(landing('1.21.4', 5.5), 3);
        assert.equal(landing('1.21.5', 5.5), 2);
    });

    it('takes the epsilon of the double form from 1.21.5, where the float form falls short of a point', () => {
        // 3.9999995 plus the epsilon, less 3, is 1.0000005: floor 1. In float the same sum is 0.9999995: floor 0.
        assert.equal(landing('1.21.5', 3.9999995), 1);
        assert.equal(landing('26.3', 3.9999995), 1);
    });

    it('rounds a sliver of a point up to one before 1.21.5', () => {
        assert.equal(landing('1.21.4', 3.0001), 1);
    });

    it('deals nothing at or below the safe distance on either side', () => {
        for (const version of ['1.21.4', '1.21.5']) assert.ok(!landing(version, 3), version);
    });
});

describe('1.21.1 to 1.21.2: LivingEntity.jumpFromGround', () => {
    // 1.21.1 LivingEntity.java:2085 setDeltaMovement(x, power, z); 1.21.2 :2164 Math.max(power, y), lines of the pinned commits
    const jump = version => native.simulateVersionedTick(version, floorWorld(), {
        position: { x: 0.5, y: 11, z: 0.5 }, velocity: { x: 0, y: 0.8, z: 0 }, halfWidth: Math.fround(0.3), height: Math.fround(1.8),
        onGround: true, collidedHorizontally: false, collidedVertically: true, isPlayer: true, clientSide: true,
        controlledByLocalInstance: true, jumpRequested: true,
    }, {}).position.y;

    it('takes the power over a faster rise before, and keeps the faster rise after', () => {
        assert.ok(jump('1.21.1') < 11.5, 'the rise is replaced by the jump power');
        assert.ok(jump('1.21.2') > 11.75, 'the faster rise stands');
        assert.equal(jump('1.21.3'), jump('1.21.2'));
        assert.equal(jump('1.20.6'), jump('1.21.1'));
    });
});
