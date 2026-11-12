'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

it('round-trips server-only periodic freeze damage through public tick and batch APIs', () => {
    // LivingEntity.aiStep owns periodic damage on the server. The separate
    // InsideBlockEffectType.FREEZE can update a client's counter from 1.21.5.
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{
        x: 0, y: 1, z: 0,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 8 },
    }]); // Powder snow with an empty collision shape.
    for (const version of ['1.17', '1.20.4', '1.21.4', '1.21.5', '1.21.11', '26.3']) {
        for (const clientSide of [false, true]) {
            const initial = {
                position: { x: 0.5, y: 1, z: 0.5 },
                velocity: { x: 0, y: 0, z: 0 },
                halfWidth: 0.3, height: 1.8,
                onGround: false, collidedHorizontally: false, collidedVertically: false,
                isPlayer: true, noGravity: true, clientSide,
                ticksFrozen: 140, tickCount: 38, freezeDamage: 7,
            };
            let state = initial;
            for (let tick = 39; tick <= 81; tick++) {
                state = native.simulateVersionedTick(version, world, state, {});
                assert.equal(state.tickCount, tick, version);
                assert.equal(state.freezeDamage, 7 + (clientSide ? 0 : Math.floor(tick / 40)), version);
                assert.equal(state.ticksFrozen, 140, version);
                assert.equal(state.clientSide, clientSide, version);
                assert.deepEqual(state.position, initial.position, version);
                assert.deepEqual(state.velocity, initial.velocity, version);
            }
            const [batched] = native.simulateVersionedTicks(version, world, [initial], 43, {});
            assert.deepEqual(batched, state, `${version}: batch continuation, client=${clientSide}`);
        }
    }
});
