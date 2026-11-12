'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { Vec3 } = require('vec3');
const physics = require('..');
const registryOf = require('prismarine-registry');

const constructors = [
    (registry, options) => new physics.NativePhysics(registry, options),
    (registry, options) => physics.createNativePhysics(registry, options),
    (registry, options) => new physics.NATIVE_PHYSICS_ENGINES[options.version](registry, options),
    (registry, options) => new physics.NativeBoatPhysics(registry, options),
    (registry, options) => new physics.NativeRiddenMobPhysics(registry, options),
];

it('rejects unverified registry/release pairs before reading the binding across every engine constructor', () => {
    for (const [source, target] of [
        ['1.12.2', '1.13.2'], ['1.15.2', '1.16.5'], ['1.20.4', '1.20.5'],
        ['1.21.4', '26.3'], ['26.3', '1.13.2'],
        ['1.16.4', '1.16.5'], ['1.20.3', '1.20.4'], ['1.20.5', '1.20.6'],
    ]) {
        for (const [given, selected] of [[source, target], [target, source]]) {
            const registry = registryOf(given);
            for (const construct of constructors) {
                let touches = 0;
                const options = { version: selected, get binding() { touches++; throw new Error('binding touched'); } };
                assert.throws(() => construct(registry, options), error => {
                    assert.match(error.message, /cannot serve Minecraft/);
                    assert.ok(error.message.includes(given) && error.message.includes(selected), error.message);
                    return true;
                });
                assert.equal(touches, 0, `${given} -> ${selected}`);
            }
        }
    }
});

it('rejects missing declarations and unsupported spellings without guessing a release', () => {
    for (const value of [undefined, null, '', 765, { name: '1.20.4' }]) {
        const registry = { version: { minecraftVersion: value, majorVersion: '1.20', version: 765 } };
        for (const construct of constructors) {
            const options = { version: '1.20.4', get binding() { throw new Error('binding touched'); } };
            assert.throws(() => construct(registry, options), /registry.*version\.minecraftVersion/i);
        }
    }
    for (const registry of [{}, { version: undefined }]) {
        assert.throws(() => new physics.NativePhysics(registry, {
            version: '1.20.4', get binding() { throw new Error('binding touched'); },
        }), /registry.*version\.minecraftVersion/i);
    }
    for (const version of ['1.20.0', '26.4', '26w01a']) {
        assert.throws(() => physics.createNativePhysics(registryOf('1.20'), {
            version, get binding() { throw new Error('binding touched'); },
        }), /unsupported decompiled Minecraft version/);
    }
});

it('validates standalone world synchronization before allocating a native world', () => {
    let allocations = 0;
    const binding = { get NativeWorld() { allocations++; throw new Error('world allocated'); } };
    assert.throws(() => new physics.NativeWorldSync(binding, registryOf('1.20.4'), '1.20.5'), /1\.20\.4.*1\.20\.5/);
    assert.throws(() => new physics.NativeWorldSync(binding, { version: {} }, '1.20.4'), /registry.*version\.minecraftVersion/i);
    assert.equal(allocations, 0);
    assert.throws(() => new physics.BlockMapper(registryOf('1.20.5').version, registryOf('1.20.4')), /1\.20\.4.*1\.20\.5/);
});

it('rejects contradictory registry identities even when selecting the declared default', () => {
    const exact = registryOf('1.20.4');
    const contradictory = new Proxy(exact, { get: (target, key) => key === 'version'
        ? { ...exact.version, '==': () => false } : target[key] });
    assert.throws(() => new physics.NativePhysics(contradictory, {
        get binding() { throw new Error('binding touched'); },
    }), /registry.*(inconsistent|disagree|contradict|identity)/i);
    // Installed minecraft-data fallbacks/aliases retain the requested version in
    // their comparison closure but declare the older data table's release.
    for (const requested of ['1.19.1', '1.21.2', '1.21.7', '1.21.10', '26.1.1', '26.1.2']) {
        const registry = registryOf(requested);
        if (registry.version.minecraftVersion !== requested) {
            // (1.21.10's declaration is its verified data release, whose contradiction is what refuses it)
            assert.throws(() => new physics.NativePhysics(registry, {
                version: requested, get binding() { throw new Error('binding touched'); },
            }), /cannot serve Minecraft|inconsistent/);
        }
        if (registry.version['=='](registry.version.minecraftVersion) === false) {
            assert.throws(() => new physics.NativePhysics(registry, {
                get binding() { throw new Error('binding touched'); },
            }), /registry.*(inconsistent|disagree|contradict|identity)/i);
        } else {
            // A future provider may supply a coherent declaration; default
            // construction promises that declared release, not a different request.
            assert.doesNotThrow(() => new physics.NativePhysics(registry));
        }
    }
});

const controls = (forward = false) => ({ forward, back: false, left: false, right: false, jump: false, sprint: false, sneak: false });

function player(attributes, effects) {
    return {
        age: 0, pos: new Vec3(0.5, 1, 0.5), vel: new Vec3(0, 0, 0), onGround: true, lastOnGround: true,
        isInWater: false, isUnderWater: false, isInLava: false, isInWeb: false, onClimbable: false,
        isCollidedHorizontally: false, isCollidedHorizontallyMinor: false, isCollidedVertically: false,
        fallDistance: 0, scale: 1, isInPowderSnow: false, wasInPowderSnow: false, keptBoundingBox: null,
        stuckSpeedMultiplier: new Vec3(0, 0, 0), nearbyEntities: [], pistonDeltas: [0, 0, 0], pistonDeltasTick: -1,
        jumpTicks: 0, sprintTriggerTime: 0, prevForwardImpulse: 0, autoJumpEnabled: false, autoJumpTime: 0,
        autoJumping: false, flyJumpTriggerTime: 0, attributes, effects, yaw: Math.PI, yawDegrees: 0, pitch: 0,
        control: controls(true), prevControl: controls(), heading: { forward: 0, strafe: 0 }, prevHeading: { forward: 0, strafe: 0 },
        isUsingItem: false, jumpBoost: 0, speed: 0, slowness: 0, dolphinsGrace: 0, slowFalling: 0, levitation: 0,
        blindness: 0, depthStrider: 0, swiftSneak: 0, riptide: 0, itemUseTicks: 0, autoSpinAttackTicks: 0,
        soulSpeed: 0, pose: 0, gameMode: 'survival', food: 20, mayFly: false, flying: false, swimming: false,
        sprinting: false, crouching: false, fallFlying: false, flySpeed: Math.fround(0.05), validElytraEquipped: false,
        canWalkOnPowderSnow: false, fireworkRocketDuration: 0, fireworkRockets: 0, fireworkRocketsBeforePlayer: 0,
        supportingBlockPos: null, getBB() { return { minX: 0.2, minY: 1, minZ: 0.2, maxX: 0.8, maxY: 2.8, maxZ: 0.8 }; },
    };
}

function floorWorld(registry) {
    const Block = require('prismarine-block')(registry);
    const air = registry.blocksByName.air.defaultState;
    const stone = registry.blocksByName.stone.defaultState;
    const id = pos => Math.floor(pos.y) === 0 ? stone : air;
    return { getBlockStateId: id, getBlock: pos => Block.fromStateId(id(pos), 0) };
}

it('matched generic, fixed and factory engines retain the same attribute-sensitive ticks and registry lookups', () => {
    for (const version of ['1.20.4', '1.20.5', '1.21.4', '1.21.5', '26.3']) {
        const registry = registryOf(version);
        const engines = [new physics.NativePhysics(registry), new physics.NATIVE_PHYSICS_ENGINES[version](registry), physics.createNativePhysics(registry)];
        const resource = registry.attributesByName.movementSpeed.resource;
        const key = version === '1.20.4' && !resource.startsWith('minecraft:') ? `minecraft:${resource}` : resource;
        const effects = { [registry.effectsByName.Speed.id]: { amplifier: 1 } };
        const outcomes = [];
        for (const engine of engines) {
            assert.equal(engine.attributeKey('movementSpeed'), key, version);
            assert.equal(engine.attributeValueOf({ [key]: { value: 0.2, modifiers: [] } }, 'movementSpeed'), 0.2);
            assert.equal(engine.getEffectLevel(1, effects), 2);
            const state = player({ [key]: { value: 0.2, modifiers: [] } }, effects);
            const world = floorWorld(registry);
            engine.simulate({ state }, world);
            engine.simulate({ state }, world);
            assert.ok(state.pos.z > 0.5, version);
            outcomes.push({ pos: state.pos, vel: state.vel, onGround: state.onGround, speed: state.speed });
            assert.equal(engine.getEnchantmentLevel(0, [{ id: 'minecraft:depth_strider', lvl: 2 }]), 2);
            assert.equal(engine.supportFeature('doesNotExist'), false);
            if (version === '26.3') {
                assert.ok(engine.attributeKey('bounciness'));
                assert.ok(engine.attributeKey('airDragModifier'));
                assert.ok(engine.attributeKey('frictionModifier'));
            }
        }
        assert.deepEqual(outcomes[1], outcomes[0], version);
        assert.deepEqual(outcomes[2], outcomes[0], version);
        const ordinary = player({}, {});
        engines[0].simulate({ state: ordinary }, floorWorld(registry));
        engines[0].simulate({ state: ordinary }, floorWorld(registry));
        assert.ok(outcomes[0].pos.z > ordinary.pos.z, `${version}: supplied movement attribute affects travel`);
    }
});
