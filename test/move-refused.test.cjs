'use strict';

// A velocity packet is the server's word and takes no check: from 1.21.9 its LpVec3 carries up to 1.7179869183E10 blocks a
// tick (26.3 LpVec3.java:17, 70). What no sync can read drops, as NativePhysics.simulate's: no tick throws, and each says so
// (docs/numeric-contract.md). The player's own cases are in mineflayer.test.cjs.

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { describe, it } = require('node:test');
const { Vec3 } = require('vec3');
const adapter = require('..');

const f32 = Math.fround;
const VERSION = '26.3';
const MOST = 1.7179869183e10;
const MOTIONS = [[MOST, 0, 0], [0, MOST, 0], [0, -MOST, 0], [0, 0, -MOST], [MOST, MOST, MOST], [Infinity, 0, 0], [1e6, 0, 0]];
/** Well inside what a sync reads around a body, and around a path. */
const WALKED = 50;
const AT_REST = { x: 0, y: 0, z: 0 };
const vector = ([x, y, z]) => ({ x, y, z });

function airWorld() {
    const registry = require('prismarine-registry')(VERSION);
    const Block = require('prismarine-block')(registry);
    const air = registry.blocksByName.air.defaultState ?? 0;
    return { registry, world: Object.assign(new EventEmitter(), { getBlock: () => Block.fromStateId(air, 0), getBlockStateId: () => air }) };
}

function boat() {
    return {
        state: {
            age: 0, pos: new Vec3(0.5, 10, 0.5), vel: new Vec3(0, 0, 0), onGround: false, isInWater: false,
            isCollidedHorizontally: false, isCollidedVertically: false, status: 4, previousStatus: 4, waterLevel: NaN, landFriction: 0,
            yawVelocity: 0, yaw: 0, yawDegrees: 0, lastVerticalVelocity: 0, controllingPlayer: true, speedMultiplier: 0.07, worldReady: true,
            halfWidth: 0.6875, height: f32(0.5625), control: { forward: false, back: false, left: false, right: false, jump: false, sprint: false, sneak: false },
            getBB: () => ({ minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 }),
        },
    };
}

function horse() {
    return {
        state: {
            age: 0, type: 'horse', pos: new Vec3(0.5, 10, 0.5), vel: new Vec3(0, 0, 0), onGround: false, lastOnGround: false, isInWater: false,
            isInLava: false, isCollidedHorizontally: false, isCollidedVertically: false, fallDistance: 0, yawDegrees: 0, pitchDegrees: 0,
            movementSpeed: 0.22499999403953552, jumpStrength: 0.7, flyingSpeed: 0.05, riddenSpeed: 0, boosting: false, boostTime: 0, boostTimeTotal: 0,
            suffocating: false, noAi: false, jumpPendingScale: 0, dashCooldown: 0, dashing: false, camelSitting: false, poseTime: 1000,
            horseJumping: false, standing: false, allowStandSliding: false, saddled: true, ownForward: 0, standCounter: 0, standAnim: 0, standAnimO: 0,
            keptBoundingBox: null, dimensions: { width: f32(1.3964844), height: f32(1.6) }, pitch: 0, lerpTarget: null, lerpSteps: 0, lerpPrevious: null,
            lerpStart: null, lerpTicks: 0, lerpYaw: null, lerpPitch: 0, lerpPreviousRotation: null, lerpStartRotation: null,
            getBB: () => ({ minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 }),
        },
    };
}

const cart = motion => ({ variant: 'minecart', position: { x: 0.5, y: 10, z: 0.5 }, velocity: vector(motion), yRot: 0, flipped: false, onGround: false, inWater: false });
const rules = { improvements: false };
const scene = { entities: [] };
const piston = { pos: [1, 10, 0], direction: [1, 0, 0], extending: true, source: false, progress: 0, shapes: [[0, 0, 0, 1, 1, 1]] };

/** Each in-place tick: it drops the motion, stays, counts the tick, says so; and the next tick goes on from rest. */
for (const [name, make, Engine, tick] of [
    ['a boat', boat, adapter.NativeBoatPhysics, (engine, ctx, world) => engine.simulate(ctx, world)],
    ['a 1.8 style boat tick', boat, adapter.NativeBoatPhysics, (engine, ctx, world) => engine.simulateLegacy(ctx, world, null)],
    ['a ridden mob', horse, adapter.NativeRiddenMobPhysics, (engine, ctx, world) => engine.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 })],
]) {
    describe(`the refusal of ${name}`, () => {
        it('skips a move no sync reads, and goes on from rest', () => {
            const { registry, world } = airWorld();
            for (const motion of MOTIONS) {
                const engine = new Engine(registry);
                const ctx = make();
                ctx.state.vel.set(...motion);
                assert.doesNotThrow(() => tick(engine, ctx, world), motion.join());
                assert.equal(ctx.state.moveRefused, true);
                assert.deepEqual([ctx.state.pos.x, ctx.state.pos.y, ctx.state.pos.z], [0.5, 10, 0.5]);
                assert.deepEqual([ctx.state.vel.x, ctx.state.vel.y, ctx.state.vel.z], [0, 0, 0]);
                assert.equal(ctx.state.age, 1);
                tick(engine, ctx, world);
                assert.equal(ctx.state.moveRefused, false);
                assert.equal(ctx.state.age, 2);
            }
        });

        it('walks the motion a sync reads', () => {
            const { registry, world } = airWorld();
            const ctx = make();
            ctx.state.vel.set(WALKED, 0, 0);
            tick(new Engine(registry), ctx, world);
            assert.equal(ctx.state.moveRefused, false);
            assert.ok(ctx.state.pos.x > 40);
        });
    });
}

describe('the piston push of a state whose motion no sync reads', () => {
    for (const [name, make, Engine] of [['boat', boat, adapter.NativeBoatPhysics], ['ridden mob', horse, adapter.NativeRiddenMobPhysics]]) {
        it(`leaves a ${name} where it is`, () => {
            const { registry, world } = airWorld();
            const ctx = make();
            ctx.state.vel.set(MOST, 0, 0);
            assert.doesNotThrow(() => new Engine(registry).pushByMovingPiston(ctx, world, piston));
            assert.equal(ctx.state.moveRefused, true);
            assert.equal(ctx.state.pos.x, 0.5);
            assert.equal(ctx.state.vel.x, 0);
        });
    }
});

describe('the refusal of a minecart', () => {
    it('skips the tick', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeMinecartPhysics(registry);
        for (const motion of MOTIONS) {
            const tick = physics.simulate(cart(motion), world, rules);
            assert.equal(tick.moveRefused, true);
            assert.deepEqual(tick.state.position, cart(motion).position);
            assert.deepEqual(tick.state.velocity, AT_REST);
            assert.deepEqual(tick.steps, []);
            assert.deepEqual(tick.explosions, []);
            assert.equal(physics.simulate(tick.state, world, rules).moveRefused, undefined);
        }
    });

    it('sits a cart of a scene out, in its place', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeMinecartPhysics(registry);
        const other = { ...cart([0, 0, 0]), position: { x: 8.5, y: 10, z: 0.5 } };
        const { carts } = physics.simulateScene([cart([MOST, 0, 0]), other, cart([0, MOST, 0])], [], world, rules);
        assert.deepEqual(carts.map(tick => tick.moveRefused), [true, undefined, true]);
        assert.equal(carts[1].state.position.x, 8.5);
        assert.ok(carts[1].state.velocity.y < 0, 'the third falls on');
        assert.deepEqual(physics.simulateScene([cart([MOST, 0, 0])], [], world, rules).carts.map(tick => tick.moveRefused), [true]);
        assert.deepEqual(physics.simulateScene([], [], world, rules).carts, []);
    });

    it('takes a hit at rest, the hit being no move', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeMinecartPhysics(registry);
        const tick = physics.hurt({ ...cart([MOST, 0, 0]), damage: 0 }, { kind: 'plain', amount: 1 }, world, rules);
        assert.equal(tick.moveRefused, true);
        assert.deepEqual(tick.state.velocity, AT_REST);
        assert.ok(tick.state.damage > 0);
    });
});

describe('the refusal of a moving entity', () => {
    it('skips the tick and keeps the entity, at rest', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        for (const motion of MOTIONS) {
            physics.track({ id: 5, name: 'item', position: new Vec3(0.5, 10, 0.5), velocity: new Vec3(...motion) });
            const result = physics.tick(5, world);
            assert.equal(result.moveRefused, true);
            assert.equal(result.state.position.y, 10);
            assert.deepEqual(physics.state(5).velocity, AT_REST);
            assert.equal(physics.tick(5, world).moveRefused, undefined);
        }
    });

    it('skips a travel', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        const squid = motion => ({
            kind: 'squid', position: { x: 0.5, y: 10, z: 0.5 }, velocity: vector(motion), yawDegrees: 0,
            input: { x: 0, y: 0, z: 1 }, speed: 0.1, halfWidth: 0.4, height: 0.8,
        });
        for (const motion of MOTIONS) {
            const moved = physics.travelMob(squid(motion), world);
            assert.equal(moved.moveRefused, true);
            assert.deepEqual(moved.position, squid(motion).position);
            assert.deepEqual(moved.velocity, AT_REST);
            assert.equal(physics.travelMob(moved, world).moveRefused, undefined);
        }
    });
});

describe('the refusal of a projectile', () => {
    const at = motion => ({ position: { x: 0.5, y: 10, z: 0.5 }, velocity: vector(motion) });

    it('skips a client tick, and the state goes on', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeProjectilePhysics(registry);
        for (const motion of MOTIONS) {
            const { state, result } = physics.predictProjectile(world, { ...at(motion), kind: 'snowball' });
            assert.equal(result.moveRefused, true);
            assert.equal(state.moveRefused, true);
            assert.deepEqual(result.position, at(motion).position);
            assert.deepEqual(state.velocity, AT_REST);
            assert.equal(physics.predictProjectile(world, state).state.moveRefused, false);
        }
    });

    it('skips an arrow, keeping what it had', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeProjectilePhysics(registry);
        const hits = { tickCount: 3 };
        for (const motion of MOTIONS) {
            const tick = physics.tickArrow(world, { ...at(motion), life: 7 }, hits, scene);
            assert.equal(tick.moveRefused, true);
            assert.deepEqual(tick.arrow.position, at(motion).position);
            assert.deepEqual(tick.arrow.velocity, AT_REST);
            assert.equal(tick.arrow.life, 7);
            assert.equal(tick.arrow.discarded, false);
            assert.equal(tick.hitState, hits);
            assert.deepEqual(tick.entityHits, []);
        }
    });

    it('skips a server tick of any other family', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeProjectilePhysics(registry);
        for (const motion of MOTIONS) {
            const tick = physics.tickProjectile(world, { ...at(motion), kind: 'snowball', accelerationPower: 0.25 }, undefined, scene);
            assert.equal(tick.moveRefused, true);
            assert.deepEqual(tick.projectile.velocity, AT_REST);
            assert.equal(tick.accelerationPower, 0.25);
            assert.deepEqual(tick.hitState, {});
        }
    });

    it('skips a trident', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeProjectilePhysics(registry);
        for (const motion of MOTIONS) {
            const tick = physics.tickTrident(world, { ...at(motion), dealtDamage: true }, undefined, scene);
            assert.equal(tick.moveRefused, true);
            assert.deepEqual(tick.trident.velocity, AT_REST);
            assert.equal(tick.trident.dealtDamage, true);
        }
    });

    it('walks the motion a sync reads', () => {
        const { registry, world } = airWorld();
        const physics = new adapter.NativeProjectilePhysics(registry);
        assert.equal(physics.tickArrow(world, at([WALKED, 0, 0]), undefined, scene).moveRefused, undefined);
    });
});
