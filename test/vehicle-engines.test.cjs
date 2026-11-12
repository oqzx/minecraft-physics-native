'use strict';

// One boat and one ridden mob engine class per release: the generated file, the exports and the declarations list the
// same releases, a class runs the release it names and no other, and a direct class ticks as the factory's does.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { describe, it } = require('node:test');
const { Vec3 } = require('vec3');
const physics = require('../dist');
const { LoadNativePhysics } = require('../dist/native');
const { registeredReleases } = require('../scripts/registered-releases.cjs');
const { vehicleEnginesSource, outputPath } = require('../scripts/generate-vehicle-engines.cjs');

const versions = registeredReleases().map(({ version }) => version);
const f32 = Math.fround;
const identifier = version => version.replaceAll('.', '_');
const registryOf = version => ({ version: { minecraftVersion: version, '<': () => false, '>': () => false } });
const families = [
    { base: 'NativeBoatPhysics', engines: physics.NATIVE_BOAT_ENGINES, create: physics.createNativeBoatPhysics },
    { base: 'NativeRiddenMobPhysics', engines: physics.NATIVE_RIDDEN_MOB_ENGINES, create: physics.createNativeRiddenMobPhysics },
];

function airWorld(version) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const air = registry.blocksByName.air.defaultState ?? 0;
    const world = Object.assign(new EventEmitter(), { getBlock: () => Block.fromStateId(air, 0), getBlockStateId: () => air });
    return { registry, world };
}

function boat() {
    return {
        state: {
            age: 0, pos: new Vec3(0.5, 10, 0.5), vel: new Vec3(0.1, 0, 0), onGround: false, isInWater: false,
            isCollidedHorizontally: false, isCollidedVertically: false, status: 4, previousStatus: 4, waterLevel: NaN, landFriction: 0,
            yawVelocity: 0, yaw: 0, yawDegrees: 0, lastVerticalVelocity: 0, controllingPlayer: true, speedMultiplier: 0.07, worldReady: true,
            halfWidth: 0.6875, height: f32(0.5625), control: { forward: true, back: false, left: false, right: false, jump: false, sprint: false, sneak: false },
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

describe('the families of the other entities', () => {
    const native = LoadNativePhysics();
    const thrown = fields => ({ position: { x: 0.5, y: 80, z: 0.5 }, velocity: { x: 0.2, y: 0.1, z: 0 }, ...fields });

    it('tick a thrown projectile in the adapter as the binding does, tick after tick', () => {
        for (const [version, kind] of [['1.8.8', 'snowball'], ['1.16.5', 'llama_spit'], ['1.21.4', 'wither_skull'], ['26.3', 'ender_pearl']]) {
            const { registry, world } = airWorld(version);
            const physics_ = new physics.NativeProjectilePhysics(registry);
            let state = thrown({ kind });
            const direct = native.simulateVersionedProjectileTicks(version, new native.NativeWorld(), [thrown({ kind })], 6)[0];
            let last;
            for (let tick = 0; tick < 6; tick += 1) {
                const next = physics_.predictProjectile(world, structuredClone(state));
                ({ state, result: last } = next);
            }
            assert.deepEqual(last, direct, version);
        }
    });

    it('refuse what a call has no meaning for, rather than taking it for something else', () => {
        const boat = status => ({ position: { x: 0.5, y: 80, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: false, left: false, right: false, inWater: false, status });
        assert.throws(() => native.simulateBoatTick(boat('in the air')), /unknown boat status: in the air/);
        assert.throws(() => native.simulateBoatTick({ ...boat(undefined), previousStatus: 'landed' }), /unknown boat status: landed/);
        for (const status of ['in_water', 'under_water', 'under_flowing_water', 'on_land', 'in_air']) native.simulateBoatTick(boat(status));
        assert.equal(native.simulateBoatTick(boat(undefined)).position.x, 0.5);
        const horse = entityType => ({ position: { x: 0, y: 80, z: 0 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 0, strafe: 0, jumpRequested: false, onGround: false, inWater: false, entityType });
        for (const type of ['pig', 'camel', 'strider', 'zombie']) assert.throws(() => native.simulateHorseTick(horse(type)), /a horse's/, type);
        for (const type of [undefined, 'horse', 'donkey', 'mule']) native.simulateHorseTick(horse(type));
    });

    it('refuse a kind in a release that does not have it, in the binding and the adapter', () => {
        const world = new native.NativeWorld();
        const refused = (version, kind) => () => native.simulateVersionedProjectileTicks(version, world, [thrown({ kind })], 1);
        for (const [kind, before, first] of [['shulker_bullet', '1.8.8', '1.9.4'], ['dragon_fireball', '1.8.8', '1.9.4'], ['llama_spit', '1.10.2', '1.11.2'], ['wind_charge', '1.20.2', '1.20.3']]) {
            assert.throws(refused(before, kind), /not supported before/, kind);
            refused(first, kind)();
        }
        const { registry, world: air } = airWorld('1.10.2');
        assert.throws(() => new physics.NativeProjectilePhysics(registry).predictProjectile(air, thrown({ kind: 'llama_spit' })), /not supported before/);
        const mob = kind => ({ kind, position: { x: 0.5, y: 80, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yawDegrees: 0, input: { x: 0, y: 0, z: 0 }, speed: 0.1, halfWidth: 0.3, height: 0.8 });
        for (const [kind, before, first] of [['vex', '1.10.2', '1.11.2'], ['phantom', '1.12.2', '1.13.2'], ['cod', '1.12.2', '1.13.2'], ['glow_squid', '1.16.5', '1.17'], ['allay', '1.18.2', '1.19']]) {
            assert.throws(() => native.simulateVersionedMobTravel(before, world, mob(kind)), /is not in/, kind);
            native.simulateVersionedMobTravel(first, world, mob(kind));
        }
        assert.throws(() => native.simulateVersionedMobTravel('26.3', world, mob('minecart')), /no travel of its own/);
    });
});

describe('the vehicle engines of each release', () => {
    it('are generated from the release registry', () => {
        assert.equal(fs.readFileSync(outputPath, 'utf8').replaceAll('\r\n', '\n'), vehicleEnginesSource());
    });

    it('list the same releases in the exports, the registries and the declarations', () => {
        const declarations = fs.readFileSync(path.join(__dirname, '..', 'dist', 'mineflayer', 'vehicle-versions.d.ts'), 'utf8');
        for (const { base, engines } of families) {
            assert.deepEqual(Object.keys(engines), versions);
            for (const version of versions) {
                const name = `${base}_${identifier(version)}`;
                assert.equal(physics[name], engines[version], name);
                assert.equal(physics[name].version, version, name);
                assert.ok(declarations.includes(`export declare class ${name} extends Versioned${base}<'${version}'>`), name);
            }
        }
    });

    it('run the one release they name, however they are made', () => {
        const binding = LoadNativePhysics();
        for (const { base, engines, create } of families) {
            for (const version of versions) {
                const direct = new engines[version](registryOf(version), { binding });
                assert.equal(direct.version, version);
                assert.ok(direct instanceof physics[base], version);
                const created = create(registryOf(version), { binding });
                assert.equal(created.constructor, engines[version], version);
                assert.throws(() => { 'use strict'; direct.version = '1.8.8x'; }, TypeError);
                assert.equal(direct.version, version);
            }
        }
    });

    it('refuse a registry of another release, and an unknown release, before touching the binding', () => {
        const unloadable = { get isSupportedVersion() { throw new Error('binding touched'); } };
        for (const { engines, create } of families) {
            assert.throws(() => new engines['1.20.4'](registryOf('1.20.3'), { binding: unloadable }), /cannot serve Minecraft 1\.20\.4/);
            assert.throws(() => create(registryOf('1.20.3'), { binding: unloadable, version: '1.20.4' }), /cannot serve Minecraft 1\.20\.4/);
            assert.throws(() => create(registryOf('1.20.7'), { binding: unloadable }), /unsupported decompiled Minecraft version: 1\.20\.7/);
        }
    });

    it('tick as the generic engine and the factory do', () => {
        for (const version of ['1.8.8', '1.12.2', '1.16.5', '1.21.4', '26.3']) {
            const { registry, world } = airWorld(version);
            const boats = [new physics.NATIVE_BOAT_ENGINES[version](registry), physics.createNativeBoatPhysics(registry), new physics.NativeBoatPhysics(registry)];
            const mobs = [new physics.NATIVE_RIDDEN_MOB_ENGINES[version](registry), physics.createNativeRiddenMobPhysics(registry), new physics.NativeRiddenMobPhysics(registry)];
            const riding = { yawDegrees: 10, pitchDegrees: 0, zza: 1, xxa: 0 };
            const boatTicks = boats.map(engine => {
                const ctx = boat();
                for (let tick = 0; tick < 5; tick += 1) engine.simulate(ctx, world);
                return [...ctx.state.pos.toArray(), ...ctx.state.vel.toArray()];
            });
            const mobTicks = mobs.map(engine => {
                const ctx = horse();
                for (let tick = 0; tick < 5; tick += 1) engine.simulateRidden(ctx, world, riding);
                return [...ctx.state.pos.toArray(), ...ctx.state.vel.toArray()];
            });
            assert.notDeepEqual(boatTicks[0], [0.5, 10, 0.5, 0.1, 0, 0], version);
            for (const ticks of [boatTicks, mobTicks]) {
                assert.deepEqual(ticks[1], ticks[0], version);
                assert.deepEqual(ticks[2], ticks[0], version);
            }
        }
    });

    it('say which mobs their release has, and refuse the others as they tick', () => {
        const old = new physics.NATIVE_RIDDEN_MOB_ENGINES['1.16.5'](registryOf('1.16.5'));
        const modern = new physics.NATIVE_RIDDEN_MOB_ENGINES['26.3'](registryOf('26.3'));
        assert.equal(old.canRide('camel'), false);
        assert.equal(modern.canRide('camel'), true);
        assert.equal(old.canRide('pig'), true);
        assert.equal(old.canRide('zombie'), false);
        const ctx = { state: { ...horse().state, type: 'camel' } };
        assert.throws(() => old.simulateRidden(ctx, airWorld('1.16.5').world, {}), /no ridden mob simulation for camel on 1\.16\.5/);
    });
});
