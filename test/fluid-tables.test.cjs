'use strict';

// generated/fluid-tables.json against the exact-release registry data, through the mapper and into the engine's own
// fluid queries, and the failures that keep scripts/generate-fluid-tables.cjs from defaulting silently.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const adapter = require('..');
const { parseFluids, parseTagFields, parseProvider, resolveTags, parseLevelStates, classifyFluidState } = require('../scripts/generate-fluid-tables.cjs');
const { registeredReleases } = require('../scripts/registered-releases.cjs');

const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'generated', 'fluid-tables.json'), 'utf8'));
const registered = registeredReleases().map(({ version }) => version);
const { FLUID_WATER, FLUID_LAVA, FLUID_WATERLOGGED } = require('../dist/mineflayer/blocks.js');

/** The exact data of a release: a table that minecraft-data answers with another release's name is not it. */
function exactData(version) {
    const data = require('minecraft-data')(version);
    return data?.blocksArray && data.version.minecraftVersion === version ? data : undefined;
}

const stateOf = (name, props, metadata = 0) => ({
    name,
    type: 0,
    metadata,
    shapes: [],
    getProperties: () => props,
});

describe('fluid tables', () => {
    it('lists every registered release, each from its exact source', () => {
        assert.deepEqual([...Object.keys(manifest.releases), ...manifest.unverified].sort(), [...registered].sort());
        // (1.8.8 too, since its own jar is decompiled: it was the one without)
        assert.deepEqual(manifest.unverified, []);
        for (const [version, release] of Object.entries(manifest.releases)) {
            assert.match(release.source, /\S/, version);
            assert.match(release.sha256, /^[0-9a-f]{64}$/, version);
        }
    });

    it('takes no provider-only tag for a release before it existed, and nests them as the release does', () => {
        const tagsOf = (version) => Object.keys(manifest.releases[version].tags);
        assert.deepEqual(tagsOf('1.21.11'), ['lava', 'water']);
        assert.equal(tagsOf('26.2').length, 6);
        assert.equal(tagsOf('26.3').length, 10);
        const last = manifest.releases['26.3'].tags;
        assert.deepEqual(last.entity_floatable, { values: ['#water'], members: ['flowing_water', 'water'] });
        assert.deepEqual(last.supports_lily_pad.members, ['water']);
    });

    it('has the five fluids of the registry from 1.13, and the four liquid blocks before', () => {
        for (const [version, release] of Object.entries(manifest.releases)) {
            if (release.legacy) assert.deepEqual(release.liquidBlocks, ['flowing_lava', 'flowing_water', 'lava', 'water'], version);
            else assert.deepEqual(release.fluids, ['empty', 'flowing_water', 'water', 'flowing_lava', 'lava'], version);
        }
        assert.equal(manifest.releases['1.13.2'].partial?.length > 0, true);
    });

    it('names exactly the blocks of the registry that hold water', () => {
        let compared = 0;
        for (const [version, release] of Object.entries(manifest.releases)) {
            const data = release.holders ? exactData(version) : undefined;
            if (!data) continue;
            compared += 1;
            const names = new Set(data.blocksArray.map((block) => block.name));
            const logged = data.blocksArray.filter((block) => block.states?.some((state) => state.name === 'waterlogged')).map((block) => block.name);
            assert.deepEqual([...release.holders.waterlogged, ...release.holders.fallingWaterlogged].sort(), logged.sort(), version);
            for (const name of release.holders.inherent) assert.ok(names.has(name), `${version} ${name}`);
        }
        assert.ok(compared >= 35, `${compared} releases compared`);
    });

    it('maps every holder as the release holds it', () => {
        for (const [version, release] of Object.entries(manifest.releases)) {
            if (!release.holders) continue;
            for (const name of release.holders.inherent) {
                assert.deepEqual(adapter.describeBlock(stateOf(name, {})).fluid, { type: FLUID_WATER, level: 0, source: true, falling: false }, `${version} ${name}`);
            }
            for (const name of release.holders.waterlogged) {
                assert.deepEqual(adapter.describeBlock(stateOf(name, { waterlogged: true })).fluid, { type: FLUID_WATERLOGGED, level: 0, source: true, falling: false }, `${version} ${name}`);
                assert.equal(adapter.describeBlock(stateOf(name, { waterlogged: false })).fluid, null, `${version} ${name}`);
            }
            for (const name of release.holders.fallingWaterlogged) {
                assert.deepEqual(adapter.describeBlock(stateOf(name, { waterlogged: true })).fluid, { type: FLUID_WATERLOGGED, level: 0, source: true, falling: true }, `${version} ${name}`);
            }
        }
    });

    it('round-trips each level of a fluid block through the mapper and the native height', () => {
        const native = require('../native/artifacts/native.js');
        for (const [version, release] of Object.entries(manifest.releases)) {
            if (!release.states) continue;
            assert.deepEqual(release.levels, { block: [0, 15], flowing: [1, 8] }, version);
            assert.equal(release.states.length, 16, version);
            for (const [name, type] of [['water', FLUID_WATER], ['flowing_water', FLUID_WATER], ['lava', FLUID_LAVA], ['flowing_lava', FLUID_LAVA]]) {
                for (const state of release.states) {
                    const { fluid } = adapter.describeBlock(stateOf(name, { level: state.level }));
                    assert.deepEqual(fluid, { type, level: state.level, source: state.source, falling: state.falling }, `${version} ${name} ${state.level}`);
                    const world = new native.NativeWorld();
                    world.setFluidState(0, 1, 0, fluid.type, 0, fluid.level, fluid.source, fluid.falling, { x: 0, y: 0, z: 0 });
                    // FluidState.getOwnHeight: amount / 9, as a float
                    assert.equal(world.fluidHeightAt(0, 1, 0), Math.fround(state.amount / 9), `${version} ${name} ${state.level}`);
                }
            }
        }
    });

    it('fails on what it cannot resolve instead of leaving it out', () => {
        assert.throws(() => parseFluids('register("water", a); register("water", b);', 'x'), /registered twice/);
        assert.throws(() => parseFluids('class Fluids {}', 'x'), /no fluid registrations/);
        assert.throws(() => parseTagFields('A = create("water"); B = create("water");', 'x'), /declared twice/);
        assert.throws(() => parseTagFields('class FluidTags {}', 'x'), /no fluid tags/);
        assert.throws(() => resolveTags({ water: ['#missing'] }, ['water'], 'x'), /unresolved fluid tag #missing/);
        assert.throws(() => resolveTags({ water: ['gas'] }, ['water'], 'x'), /unknown fluid gas/);
        assert.throws(() => resolveTags({ a: ['#b'], b: ['#a'] }, ['water'], 'x'), /cycle/);
        assert.throws(() => parseProvider('this.tag(FluidTags.GAS).add(Fluids.WATER);', { WATER: 'water' }, 'x'), /unresolved fluid tag GAS/);
        assert.throws(() => parseProvider('this.tag(FluidTags.WATER).add(Other.WATER);', { WATER: 'water' }, 'x'), /unresolved member/);
        assert.throws(() => parseLevelStates('class LiquidBlock {}', 'x'), /stateCache/);
        assert.throws(() => classifyFluidState('Odd.java', 'FluidState getFluidState(BlockState s) { return odd(); }', 'x'), /unknown getFluidState/);
        assert.throws(() => classifyFluidState('Odd.java', 'FluidState getFluidState(BlockState s) { return Fluids.LAVA.getSource(false); }', 'x'), /holds lava/);
    });

    it('tells the three ways of holding water apart', () => {
        const body = (text) => `FluidState getFluidState(BlockState s) { ${text} }`;
        assert.equal(classifyFluidState('Kelp.java', body('return Fluids.WATER.getSource(false);'), 'x'), 'inherent');
        assert.equal(classifyFluidState('Slab.java', body('return s.getValue(WATERLOGGED) ? Fluids.WATER.getSource(false) : super.getFluidState(s);'), 'x'), 'waterlogged');
        assert.equal(classifyFluidState('Grate.java', body('return s.getValue(WATERLOGGED) ? Fluids.WATER.getSource(true) : super.getFluidState(s);'), 'x'), 'fallingWaterlogged');
        assert.equal(classifyFluidState('Plain.java', 'class Plain {}', 'x'), undefined);
    });
});
