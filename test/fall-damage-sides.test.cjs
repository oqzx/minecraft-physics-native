'use strict';

// B304: fallDamage on the two sides. The client computes causeFallDamage's amount as the server does and only its hurt
// does nothing, so a client state reports a prediction, where contact and wall damage are 0 (NativeSimulationState.fallDamage).

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

function floorWorld() {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{
        x: 0, y: 10, z: 0,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 },
    }]);
    return world;
}

/** A player a tick from the floor, 5 blocks into its fall. */
const falling = (clientSide, fields = {}) => ({
    position: { x: 0.5, y: 11.2, z: 0.5 },
    velocity: { x: 0, y: -0.5, z: 0 },
    halfWidth: Math.fround(0.3), height: Math.fround(1.8),
    onGround: false, collidedHorizontally: false, collidedVertically: false,
    isPlayer: true, clientSide, controlledByLocalInstance: true, fallDistance: 5,
    ...fields,
});

const tick = (version, state) => native.simulateVersionedTick(version, floorWorld(), state, {});

it('predicts the landing on a client state as the server deals it, either side of each rounding', () => {
    // ceil before 1.21.5 (1.20.4 LivingEntity.java:1564), floor with the epsilon after (1.21.5 :1672-1683), the drop of
    // the landing tick counted from 1.21.5 (1.21.5 Entity.java:1286): the two sides run the same check
    for (const version of ['1.8.8', '1.12.2', '1.20.4', '1.20.5', '1.21.1', '1.21.2', '1.21.3', '1.21.5', '1.21.11', '26.3']) {
        const client = tick(version, falling(true));
        const server = tick(version, falling(false));
        assert.ok(client.fallDamage > 0, version);
        assert.equal(client.fallDamage, server.fallDamage, version);
        assert.equal(client.onGround, true, version);
    }
});

it('has no landing on 1.21.4\'s client, and the server has it', () => {
    // Entity.move skips a player on both sides there (1.21.4 Entity.java:697, PlayerEntity.java:2214), and the client
    // has no handleFall; the server's moves run it (ServerPlayNetworkHandler.java:999, Entity.java:1250-1256)
    const client = tick('1.21.4', falling(true));
    const server = tick('1.21.4', falling(false));
    assert.equal(client.fallDamage, 0);
    assert.equal(client.fallDistance, 5);
    assert.ok(server.fallDamage > 0);
    assert.equal(server.fallDistance, 0);
    assert.equal(server.fallDamage, tick('1.21.3', falling(false)).fallDamage);
    // either side of the release: no such skip
    assert.ok(tick('1.21.3', falling(true)).fallDamage > 0);
    assert.ok(tick('1.21.5', falling(true)).fallDamage > 0);
});

it('leaves a remote entity to its own side from 1.21.2', () => {
    // 1.21.1 runs fall on every side; 1.21.2 - 1.21.4 `!isClient || isLogicalSideForUpdatingMovement` (1.21.3
    // Entity.java:686); 1.21.5 isLocalInstanceAuthoritative (1.21.5 Entity.java:708)
    const remote = { isPlayer: false, controlledByLocalInstance: false };
    assert.ok(tick('1.21.1', falling(true, remote)).fallDamage > 0);
    assert.equal(tick('1.21.2', falling(true, remote)).fallDamage, 0);
    assert.equal(tick('1.21.3', falling(true, remote)).fallDamage, 0);
    assert.ok(tick('1.21.3', falling(false, remote)).fallDamage > 0);
    assert.ok(tick('1.21.4', falling(false, remote)).fallDamage > 0);
    assert.equal(tick('1.21.5', falling(false, remote)).fallDamage, 0);
    assert.equal(tick('26.3', falling(true, remote)).fallDamage, 0);
});

it('holds it off a player that may fly on both sides', () => {
    // Player.causeFallDamage returns before computing (26.3 Player.java:1340, 1.8.9 PlayerEntity.java:1387-1388)
    for (const version of ['1.8.8', '1.20.4', '26.3']) {
        for (const side of [true, false]) assert.equal(tick(version, falling(side, { fallDamageImmune: true })).fallDamage, 0, version);
    }
});
