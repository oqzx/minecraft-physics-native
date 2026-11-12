'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

it('takes a rocket\'s noPhysics: it passes through and acts on no hit', () => {
    // 26.3 FireworkRocketEntity.java:146-152, Entity.java:766-772.
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{
        x: 1, y: 0, z: 0,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 },
    }]);
    const rocket = {
        kind: 'firework_rocket', firstTick: false,
        position: { x: 0.5, y: 0.25, z: 0.5 }, velocity: { x: 1, y: 0, z: 0 },
    };
    const [solid] = native.simulateVersionedProjectileTicks('26.3', world, [rocket], 1);
    assert.equal(solid.blockHit, true);
    assert.equal(solid.horizontalCollision, true);
    const [ghost] = native.simulateVersionedProjectileTicks('26.3', world, [{ ...rocket, noPhysics: true }], 1);
    assert.equal(ghost.blockHit, false);
    assert.equal(ghost.horizontalCollision, false);
    assert.equal(ghost.position.x, 0.5 + 1.15);
});
