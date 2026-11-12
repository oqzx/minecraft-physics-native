'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

function shoot(version, world, position, velocity) {
    return native.simulateVersionedProjectileTicks(version, world, [{
        kind: 'snowball', position, velocity, gravity: 0, airDrag: 1, firstTick: false,
    }], 1)[0];
}

it('uses the legacy exit face for a projectile starting inside a block before 1.13', () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 0, y: 0, z: 0, id: 1 }]);
    // Box.raycast tests all six faces in 1.9.4–1.12.2. The 1.8.8 registration
    // shares that path; its available source reference is the 1.8.9 stand-in.
    for (const version of ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2', '26.3']) {
        for (const axis of ['x', 'y', 'z']) {
            for (const direction of [-1, 1]) {
                const position = { x: 0.5, y: 0.5, z: 0.5 };
                const velocity = { x: 0, y: 0, z: 0, [axis]: direction };
                const result = shoot(version, world, position, velocity);
                // Older ThrowableEntity ticks report the collision and still perform
                // the full move. Only the 1.21.2+ path stops at the hit point.
                const expected = { ...position, [axis]: 0.5 + direction * (version === '26.3' ? 0.001 : 1) };
                assert.equal(result.blockHit, true, `${version}, ${axis}, ${direction}`);
                assert.deepEqual(result.position, expected, `${version}, ${axis}, ${direction}`);
            }
        }
    }
});

it('keeps the legacy per-axis tiny-ray cutoff distinct from the modern length cutoff', () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 1, y: 0, z: 0, id: 1 }]);
    const position = { x: 0.9999, y: 0.5, z: 0.5 };
    const velocity = { x: 0.00025, y: 0.00025, z: 0 };
    const legacy = shoot('1.12.2', world, position, velocity);
    const modern = shoot('1.13.2', world, position, velocity);
    assert.equal(legacy.blockHit, false);
    assert.deepEqual(legacy.position, { x: position.x + velocity.x, y: position.y + velocity.y, z: 0.5 });
    assert.equal(modern.blockHit, true);
    assert.deepEqual(modern.position, legacy.position); // Both versions still perform the full move.
});
