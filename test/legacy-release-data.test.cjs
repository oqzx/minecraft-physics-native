'use strict';

// The 1.8.8 - 1.13.2 block, fluid and tag data (generated/legacy-release-data.json, emitted into
// native/data/releases by generate-native-data.cjs): complete, reproducible from the decompiled
// trees, and for 1.13.2 the same block states minecraft-data lists.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { LEGACY_RELEASES, extractLegacyRelease } = require('../scripts/legacy-release-data.cjs');

const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'generated', 'legacy-release-data.json'), 'utf8'));
const decompiledRoot = process.env.MC_DECOMPILED_ROOT ?? path.join(__dirname, '..', '..', '..', 'pr', 'mc-decompiled');
const haveTrees = LEGACY_RELEASES.every(release => fs.existsSync(path.join(decompiledRoot, release.source, 'net')));

describe('legacy release data', () => {
    it('covers every pre-1.14 profile with fully resolved block states', () => {
        assert.deepEqual(Object.keys(manifest.releases), LEGACY_RELEASES.map(release => release.version));
        for (const [version, release] of Object.entries(manifest.releases)) {
            for (const id of release.registry.blocks) {
                const block = release.blocks[release.registry.classes[id]];
                assert.ok(block, `${version} ${id}`);
                assert.notEqual(block.stateCount, null, `${version} ${id} has an unresolved state`);
                assert.equal(block.stateCount, block.properties.reduce((count, property) => count * property.values.length, 1));
            }
            assert.deepEqual(release.fluids.map(fluid => fluid.id), ['water', 'flowing_water', 'lava', 'flowing_lava']);
            assert.equal(release.fluidTags.length > 0, release.flattened, version);
            assert.equal(release.blockTags.length > 0, release.flattened, version);
        }
    });

    it('reproduces the manifest from the decompiled trees', { skip: !haveTrees && 'no decompiled trees on this machine' }, () => {
        for (const release of LEGACY_RELEASES) {
            const data = extractLegacyRelease(release, decompiledRoot);
            const kept = manifest.releases[release.version];
            assert.deepEqual(data.unresolved, []);
            assert.equal(data.registry.sha256, kept.registry.sha256, release.version);
            assert.deepEqual(data.registry.classes, kept.registry.classes, release.version);
            assert.deepEqual(data.fluids, kept.fluids, release.version);
            for (const [className, block] of Object.entries(data.blocks)) {
                assert.deepEqual(block.properties, kept.blocks[className].properties, `${release.version} ${className}`);
            }
        }
    });

    it('matches the 1.13.2 block states minecraft-data lists', () => {
        const blocks = require('minecraft-data')('1.13.2').blocksArray;
        const release = manifest.releases['1.13.2'];
        assert.equal(release.registry.blocks.length, blocks.length);
        for (const expected of blocks) {
            const block = release.blocks[release.registry.classes[expected.name]];
            assert.ok(block, expected.name);
            assert.equal(block.stateCount, expected.maxStateId - expected.minStateId + 1, expected.name);
            for (const state of expected.states ?? []) {
                const property = block.properties.find(candidate => candidate.name === state.name.toUpperCase());
                assert.ok(property, `${expected.name} ${state.name}`);
                assert.equal(property.values.length, state.num_values, `${expected.name} ${state.name}`);
                if (state.type === 'enum') assert.deepEqual(property.values, state.values, `${expected.name} ${state.name}`);
            }
        }
    });
});
