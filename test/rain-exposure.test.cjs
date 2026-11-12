'use strict';

// B297: Entity.isInRain derived for a bot, from its rain level, sky light, blocks and biome. What vanilla does at these places
// is recorded in physrec/q_rain_* (1.20.4 and 26.3: open launches; roofed, clear_sky, snowy_biome, desert do not).

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { describe, it } = require('node:test');
const Vec3 = require('vec3').Vec3;
const adapter = require('..');
const rain = require('../dist/mineflayer/rain.js');

const check = version => require('../dist/mineflayer/blocks.js').releaseCheck(version);

/** A column world: the floor at y 63, `blocks` by y, one biome, and full sky light above `roof`. */
function worldOf(version, biomeName, { blocks = {}, skyBelow = -Infinity } = {}) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const biome = registry.biomesArray.find(entry => entry.name === biomeName);
    assert.ok(biome, biomeName);
    const air = registry.blocksByName.air.defaultState ?? 0;
    const stateAt = pos => {
        const y = Math.floor(pos.y);
        if (y === 63) return registry.blocksByName.stone.defaultState ?? registry.blocksByName.stone.id << 4;
        const name = blocks[y];
        return name ? registry.blocksByName[name].defaultState ?? registry.blocksByName[name].id << 4 : air;
    };
    const world = Object.assign(new EventEmitter(), {
        getBlock: pos => Object.assign(Block.fromStateId(stateAt(pos), 0), { biome }),
        getBlockStateId: stateAt,
        getSkyLight: pos => (Math.floor(pos.y) < skyBelow ? 0 : 15),
    });
    return { registry, world };
}

function player(y, fields = {}) {
    return { pos: new Vec3(0.5, y, 0.5), pose: 0, scale: 1, bot: { rainState: 1, game: { minY: 0, height: 256 } }, ...fields };
}

const controls = () => ({ forward: false, back: false, left: false, right: false, jump: false, sprint: false, sneak: false });

/** The fields of a mineflayer-physics-utils state the adapter reads. */
function playerState(x, y, z, fields = {}) {
    return {
        state: {
            age: 0, pos: new Vec3(x, y, z), vel: new Vec3(0, 0, 0), onGround: false, lastOnGround: false, onClimbable: false,
            isInWater: false, isUnderWater: false, isInLava: false, isInWeb: false, isCollidedHorizontally: false,
            isCollidedHorizontallyMinor: false, isCollidedVertically: false, fallDistance: 0, scale: 1, isInPowderSnow: false,
            wasInPowderSnow: false, keptBoundingBox: null, stuckSpeedMultiplier: new Vec3(0, 0, 0), nearbyEntities: [],
            jumpTicks: 0, sprintTriggerTime: 0, prevForwardImpulse: 0,
            autoJumpEnabled: false, autoJumpTime: 0, autoJumping: false, flyJumpTriggerTime: 0, attributes: {}, yaw: Math.PI,
            yawDegrees: 0, pitch: 0, control: controls(), prevControl: controls(), heading: { forward: 0, strafe: 0 },
            prevHeading: { forward: 0, strafe: 0 }, isUsingItem: false, effects: {}, jumpBoost: 0, speed: 0, slowness: 0,
            dolphinsGrace: 0, slowFalling: 0, levitation: 0, blindness: 0, depthStrider: 0, swiftSneak: 0, riptide: 0,
            itemUseTicks: 0, autoSpinAttackTicks: 0, soulSpeed: 0, pose: 0, gameMode: 'survival', food: 20, mayFly: false,
            flying: false, swimming: false, sprinting: false, crouching: false, fallFlying: false, flySpeed: Math.fround(0.05),
            validElytraEquipped: false, canWalkOnPowderSnow: false, fireworkRocketDuration: 0, fireworkRockets: 0,
            fireworkRocketsBeforePlayer: 0, supportingBlockPos: null,
            getBB() {
                return { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 };
            },
            ...fields,
        },
    };
}

const exposed = (version, biome, y, options, fields) => {
    const { registry, world } = worldOf(version, biome, options);
    const physics = new adapter.NativePhysics(registry);
    const state = player(y, fields);
    const result = physics.rainExposed(state, world, physics.syncOf(world));
    physics.dispose();
    return result;
};

const roof = { blocks: { 70: 'stone' }, skyBelow: 70 };

describe('rain at a place', () => {
    for (const version of ['1.20.4', '1.20.5', '26.3']) {
        it(`falls as recorded on ${version}`, () => {
            assert.equal(exposed(version, 'plains', 64), true);
            assert.equal(exposed(version, 'plains', 64, roof), false);
            assert.equal(exposed(version, 'snowy_plains', 64), false);
            assert.equal(exposed(version, 'desert', 64), false);
            assert.equal(exposed(version, 'plains', 64, {}, { bot: { rainState: 0 } }), false);
        });
    }

    it('needs the level above 0.2, as isRaining does', () => {
        assert.equal(exposed('26.3', 'plains', 64, {}, { bot: { rainState: 0.2 } }), false);
        assert.equal(exposed('26.3', 'plains', 64, {}, { bot: { rainState: 0.21 } }), true);
    });

    it('stops at what blocks motion above, with the sky still open', () => {
        // glass lets the sky light through and is in the heightmap; a torch is not (26.3 Heightmap.java:166, 1.20.4 by blocksMotion)
        for (const version of ['1.20.4', '26.3']) {
            assert.equal(exposed(version, 'plains', 64, { blocks: { 80: 'glass' } }), false, version);
            assert.equal(exposed(version, 'plains', 64, { blocks: { 80: 'oak_leaves' } }), false, version);
            assert.equal(exposed(version, 'plains', 64, { blocks: { 66: 'torch' } }), true, version);
            // water counts, by its fluid
            assert.equal(exposed(version, 'plains', 64, { blocks: { 70: 'water' } }), false, version);
        }
    });

    it('reaches the box top', () => {
        // the feet are under a slab of nothing, the head above: isRainingAt tries both (26.3 Entity.java:1638-1641)
        assert.equal(exposed('26.3', 'plains', 64, { skyBelow: 65 }), true);
        assert.equal(exposed('26.3', 'plains', 64, { skyBelow: 67 }), false);
    });

    it('cools with the height from 1.16 alone, on each release\'s own scale', () => {
        // taiga 0.25: y 170 is under 0.15 by /30 above 64 and by /40 above 80, and 1.13 - 1.15 ask the biome alone
        for (const [version, rains] of [['1.14.4', true], ['1.15.2', true], ['1.16.5', false], ['1.17.1', false], ['1.18.2', false], ['26.3', false]]) {
            assert.equal(exposed(version, 'taiga', 170), rains, version);
        }
        assert.equal(exposed('1.16.5', 'taiga', 64), true);
        assert.equal(exposed('26.3', 'taiga', 64), true);
    });

    it('follows a release\'s rule before the flattening', () => {
        // 1.8.9 World.java:2546-2561: no snow biome, not cold, a biome with rain
        for (const version of ['1.8.8', '1.12.2']) {
            assert.equal(exposed(version, 'plains', 64), true, version);
            assert.equal(exposed(version, 'desert', 64), false, version);
        }
    });

    it('needs a bot', () => {
        assert.equal(exposed('26.3', 'plains', 64, {}, { bot: undefined }), false);
    });
});

describe('the noise', () => {
    // the 26.3 jar's SimplexNoise over LegacyRandomSource, and java.util.Random
    const points = [[0, 0], [0.125, 0.25], [1000.5, -2000.25], [-17.5, 31.25], [123456, 654321], [-0.03125, -0.0625]];
    const close = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-6, `${actual} ${expected}`);

    it('is the jar\'s', () => {
        const simplex = [0, -0.85696477, 0.54227513, -0.26771593, 0.3189391, 0.3943677];
        const frozen = [0, 0.23149377, -0.3021298, 0.5396739, 0.25034806, -0.07666845];
        const info = [0, 0.24510935, -0.627057, -0.506546, -0.3189393, -0.1314559];
        points.forEach(([x, z], i) => {
            close(rain.temperatureNoiseAt(false, x, z), simplex[i]);
            close(rain.frozenNoiseAt(x, z), frozen[i]);
            close(rain.biomeInfoNoiseAt(x, z), info[i]);
        });
    });

    it('draws what java.util.Random draws', () => {
        const random = new rain.JavaRandom(1234);
        assert.equal(random.nextDouble(), 0.6465821602909256);
        assert.equal(random.nextInt(256), 243);
        assert.equal(random.nextInt(255), 180);
        assert.equal(random.nextDouble(), 0.8575884598068334);
    });

    it('takes a frozen ocean\'s ice patches for 0.2', () => {
        const temperature = (x, z) => rain.biomeTemperature(check('26.3'), { name: 'frozen_ocean', temperature: 0 }, x, 64, z, 63);
        const values = new Set();
        for (let x = 0; x < 256; x += 8) for (let z = 0; z < 256; z += 8) values.add(temperature(x, z));
        assert.deepEqual([...values].sort(), [0, Math.fround(0.2)]);
    });
});

describe('a live bot', () => {
    const released = (version, biome, options, fields) => {
        const { registry, world } = worldOf(version, biome, options);
        const physics = new adapter.NativePhysics(registry);
        const base = playerState(0.5, 64, 0.5, { riptide: 3, itemUseTicks: 12, yaw: 0, bot: { rainState: 1, game: { minY: 0, height: 256 } }, ...fields });
        physics.simulate(base, world);
        const launched = base.state.autoSpinAttackTicks > 0;
        physics.dispose();
        return { launched, state: base.state };
    };

    it('launches a riptide in the rain under open sky and not under a roof', () => {
        for (const version of ['1.20.4', '26.3']) {
            assert.equal(released(version, 'plains').launched, true, version);
            assert.equal(released(version, 'plains', roof).launched, false, version);
            assert.equal(released(version, 'desert').launched, false, version);
        }
    });

    it('leaves the value to a caller that gives it, and writes none of its own', () => {
        assert.equal(released('26.3', 'desert', {}, { inRain: true }).launched, true);
        const refused = released('26.3', 'plains', {}, { inRain: false });
        assert.equal(refused.launched, false);
        assert.equal(refused.state.inRain, false);
        assert.equal('inRain' in released('26.3', 'plains').state, false);
    });
});
