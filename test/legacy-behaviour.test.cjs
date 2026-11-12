'use strict';

// B030: a legacy release runs on its own numeric world data and its own fluid path, not on modern data with an old name.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const physics = require('..');
const { LEGACY_RELEASES } = require('../scripts/legacy-release-data.cjs');

const native = physics.LoadNativePhysics();
const NUMERIC = LEGACY_RELEASES.map(({ version }) => version).filter(version => version !== '1.13.2');
const WATER = 1;

const legacyBlock = (version, id, metadata) => {
    const registry = require('prismarine-registry')(version);
    return new (require('prismarine-block')(registry))(id, 0, metadata);
};

describe('numeric releases', () => {
    for (const version of NUMERIC) {
        it(`${version} has still and flowing water as two blocks, the level in the metadata`, () => {
            const registry = require('prismarine-registry')(version);
            assert.equal(registry.blocksByName.water.id, 9);
            assert.equal(registry.blocksByName.flowing_water.id, 8);
            assert.equal(registry.blocksByName.lava.id, 11);
            assert.deepEqual(physics.describeBlock(legacyBlock(version, 9, 0)).fluid, { type: WATER, level: 0, source: true, falling: false });
            assert.deepEqual(physics.describeBlock(legacyBlock(version, 8, 3)).fluid, { type: WATER, level: 3, source: false, falling: false });
            assert.deepEqual(physics.describeBlock(legacyBlock(version, 8, 8)).fluid, { type: WATER, level: 8, source: false, falling: true });
        });
    }

    it('have no fluid tags and no flattened states in their tables', () => {
        for (const version of NUMERIC) assert.equal(native.generatedBlockHasTag(version, 'water', 'water'), false, version);
    });
});

describe('the flattening', () => {
    it('starts at 1.13.2, where one block holds every level as a property', () => {
        const registry = require('prismarine-registry')('1.13.2');
        assert.equal(registry.blocksByName.flowing_water, undefined);
        assert.equal(registry.blocksByName.water.maxStateId - registry.blocksByName.water.minStateId + 1, 16);
        assert.equal(native.generatedBlockStateCount('1.13.2', 'water'), 16);
        assert.ok(native.generatedFluidStateCount('1.13.2', 'water') > 0);
    });
});

describe('the legacy fluid path', () => {
    function waterWorld() {
        const world = new native.NativeWorld();
        const empty = { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 0 };
        const updates = [];
        for (const y of [0, 1, 2, 3]) updates.push({ x: 0, y, z: 0, shape: empty, fluid: WATER, fluidLevel: 0, fluidSource: true });
        world.applyBlockUpdates(updates);
        return world;
    }
    const body = {
        position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0.1, y: -0.1, z: 0 }, halfWidth: 0.3, height: 1.8, eyeHeight: 1.62,
        onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true, clientSide: true,
    };
    const tick = (version, world) => native.simulateVersionedTick(version, world, body, {});

    it('wets the player on every legacy release, on its own terms', () => {
        const air = new native.NativeWorld();
        for (const { version } of LEGACY_RELEASES) {
            const wet = tick(version, waterWorld());
            assert.equal(wet.inWater, true, version);
            assert.notDeepEqual(wet.velocity, tick(version, air).velocity, version);
        }
    });

    it('is not the modern path under an old name', () => {
        const legacy = tick('1.12.2', waterWorld());
        const modern = tick('1.20.4', waterWorld());
        assert.notDeepEqual([legacy.position, legacy.velocity], [modern.position, modern.velocity]);
    });
});
