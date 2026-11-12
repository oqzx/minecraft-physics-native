'use strict';

// LiquidBlock.getCollisionShape's two ways: an entity's, which asks for a source, a capability and the feet, and the query with
// fluid collisions, which from 1.21.9 answers the whole block before it asks anything.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const adapter = require('..');
const { releaseCheck } = require('../dist/mineflayer/blocks.js');

const state = (name, props = {}) => adapter.describeBlock({ name, type: 0, metadata: 0, shapes: [], getProperties: () => props });
const SOURCE = state('lava', { level: 0 });
const FLOWING = state('lava', { level: 3 });
const FALLING = state('lava', { level: 8 });
const WATER = state('water', { level: 0 });
const KELP = state('kelp', {});
const LOGGED = state('oak_fence', { waterlogged: true });
const AIR = null;
const FULL = [0, 0, 0, 1, 1, 1];
const STABLE = [0, 0, 0, 1, 0.5, 1];
const strider = { bottom: 64.5, canStandOn: (fluid) => fluid === 'lava' };
const player = { bottom: 64.5, canStandOn: () => false };
const shape = (block, above, context, release = '26.3') => adapter.liquidCollisionBox(block, above, 64, context, releaseCheck(release));

describe('liquid collision', () => {
    it('forces the whole block from 1.21.9, whatever the state and whoever asks', () => {
        for (const release of ['1.21.9', '1.21.11', '26.1', '26.2', '26.3']) {
            for (const block of [SOURCE, FLOWING, FALLING, WATER]) {
                for (const above of [AIR, SOURCE, WATER]) {
                    assert.deepEqual(shape(block, above, { ...player, bottom: 0, withFluidCollisions: true }, release), FULL);
                }
            }
        }
    });

    it('between 1.21.2 and 1.21.8 only turns the capability on', () => {
        for (const release of ['1.21.2', '1.21.5', '1.21.8']) {
            assert.deepEqual(shape(SOURCE, AIR, { ...player, withFluidCollisions: true }, release), STABLE);
            assert.deepEqual(shape(WATER, AIR, { ...player, withFluidCollisions: true }, release), STABLE);
            assert.equal(shape(FLOWING, AIR, { ...player, withFluidCollisions: true }, release), null);
            assert.equal(shape(SOURCE, SOURCE, { ...player, withFluidCollisions: true }, release), null);
            assert.equal(shape(SOURCE, AIR, { ...player, bottom: 64.4, withFluidCollisions: true }, release), null);
        }
    });

    it('has no such query before 1.21.2', () => {
        assert.throws(() => shape(SOURCE, AIR, { ...player, withFluidCollisions: true }, '1.21.1'), /no query with fluid collisions/);
    });

    it('never answers for a block that only holds water', () => {
        for (const block of [KELP, LOGGED]) assert.equal(shape(block, AIR, { ...strider, withFluidCollisions: true }), null);
    });

    it('holds an entity that can stand on it above a source with no same fluid over it', () => {
        assert.deepEqual(shape(SOURCE, AIR, strider), STABLE);
        assert.deepEqual(shape(SOURCE, WATER, strider), STABLE);
        assert.equal(shape(SOURCE, SOURCE, strider), null);
        assert.equal(shape(SOURCE, FLOWING, strider), null);
        assert.equal(shape(FLOWING, AIR, strider), null);
        assert.equal(shape(FALLING, AIR, strider), null);
        assert.equal(shape(WATER, AIR, strider), null);
        assert.equal(shape(SOURCE, AIR, player), null);
    });

    it('asks for the feet above the surface, by the same 1.0E-5 as a float', () => {
        const at = (bottom) => shape(SOURCE, AIR, { ...strider, bottom });
        assert.deepEqual(at(64.5), STABLE);
        assert.deepEqual(at(64.499995), STABLE);
        assert.equal(at(64.4999), null);
    });

    it('holds nothing up before the strider', () => {
        assert.equal(shape(SOURCE, AIR, strider, '1.15.2'), null);
        assert.deepEqual(shape(SOURCE, AIR, strider, '1.16'), STABLE);
    });
});
