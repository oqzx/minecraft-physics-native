'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

function wallWorld() {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{
        x: 2, y: 10, z: 0,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 },
    }]);
    return world;
}

function glider(speed, clientSide) {
    return {
        position: { x: 1.6, y: 10, z: 0.5 },
        velocity: { x: speed, y: 0, z: 0 },
        halfWidth: 0.3, height: 0.6,
        onGround: false, collidedHorizontally: false, collidedVertically: false,
        isPlayer: true, noGravity: true, clientSide,
        controlledByLocalInstance: true,
        fallFlying: true,
        look: { x: 1, y: 0, z: 0 },
    };
}

it('reports a flight into a wall on the server alone, for the tick that hit it', () => {
    // LivingEntity.travelFallFlying (26.3 LivingEntity.java:2505-2510, 2546-2553; 1.9.4 :1363-1371).
    const world = wallWorld();
    for (const version of ['1.9.4', '1.20.4', '1.21.11', '26.3']) {
        const server = native.simulateVersionedTick(version, world, glider(1, false), {});
        const client = native.simulateVersionedTick(version, world, glider(1, true), {});
        assert.equal(server.wallDamage, 7, version);
        assert.equal(client.wallDamage, 0, version);
        assert.equal(server.collidedHorizontally, true, version);
        assert.equal(client.collidedHorizontally, true, version);
        assert.deepEqual(client.velocity, server.velocity, version);
        assert.equal(native.simulateVersionedTick(version, world, server, {}).wallDamage, 0, version);
        assert.equal(native.simulateVersionedTick(version, world, glider(0.3, false), {}).wallDamage, 0, version);
    }
});
