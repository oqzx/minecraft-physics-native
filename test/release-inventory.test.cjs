'use strict';

// B009, B025: one canonical inventory, and every registry surface held to it by exact name.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { canonicalReleases, inventoryProblems, inventorySources, readSurfaces } = require('../scripts/release-inventory.cjs');
const { generatedFiles, normalized } = require('../scripts/generate-version-exports.cjs');
const { vehicleEnginesSource } = require('../scripts/generate-vehicle-engines.cjs');

const sources = inventorySources();
const surfaces = readSurfaces();
const clone = value => structuredClone(value);

describe('the release inventory', () => {
    it('passes the current checkout, the legacy and modern lists kept apart', () => {
        assert.deepEqual(inventoryProblems(sources, surfaces), []);
        assert.equal(sources.legacy.includes('1.14.4'), false);
        assert.equal(sources.modern.includes('1.13.2'), false);
        assert.equal(sources.modern[0], '1.14.4');
        assert.equal(sources.legacy[0], '1.8.8');
    });

    it('fails a same-count swap of two modules, naming both', () => {
        const swapped = clone(surfaces);
        swapped.modules = swapped.modules.map(module => (module.name === 'v1_20_4' ? { ...module, name: 'v1_99_0', file: 'v-1_99_0.rs' } : module));
        const problems = inventoryProblems(sources, swapped);
        assert.equal(swapped.modules.length, surfaces.modules.length);
        assert.ok(problems.includes('native/src/versioned/mod.rs modules has no v1_20_4 for 1.20.4'), problems.join('\n'));
        assert.ok(problems.includes('native/src/versioned/mod.rs modules has v1_99_0, which is no canonical release'), problems.join('\n'));
    });

    it('fails a same-count swap of generated tables, naming both', () => {
        const swapped = clone(surfaces);
        swapped.dataFiles = swapped.dataFiles.map(entry => (entry.file === 'v26_3.rs' ? { ...entry, file: 'v26_4.rs' } : entry));
        const problems = inventoryProblems(sources, swapped);
        assert.ok(problems.includes('native/data/releases has no v26_3 for 26.3'), problems.join('\n'));
        assert.ok(problems.includes('native/data/releases has v26_4, which is no canonical release'), problems.join('\n'));
    });

    it('fails a module that declares another release, and tables that hold another', () => {
        const wrong = clone(surfaces);
        wrong.modules.find(module => module.name === 'v1_21_5').label = '1.21.6';
        wrong.dataFiles.find(entry => entry.file === 'v1_21_5.rs').prefix = 'V1_21_6';
        const problems = inventoryProblems(sources, wrong);
        assert.ok(problems.includes('v-1_21_5.rs declares VERSION 1.21.6, not 1.21.5'), problems.join('\n'));
        assert.ok(problems.includes('native/data/releases/v1_21_5.rs holds V1_21_6, not the tables of 1.21.5'), problems.join('\n'));
    });

    it('fails missing, duplicate and unexpected identities on each side', () => {
        const missingLegacy = { ...sources, legacy: sources.legacy.filter(version => version !== '1.9.4') };
        assert.ok(inventoryProblems(missingLegacy, surfaces).some(problem => /has v1_9_4, which is no canonical release/.test(problem)));
        const duplicateModern = { ...sources, modern: [...sources.modern, '26.3'] };
        assert.ok(inventoryProblems(duplicateModern, surfaces).includes('26.3 is listed twice in generated/minecraft-versions.json'));
        const duplicateLegacy = { ...sources, legacy: [...sources.legacy, '1.8.8'] };
        assert.ok(inventoryProblems(duplicateLegacy, surfaces).includes('1.8.8 is listed twice in LEGACY_RELEASES'));
        const both = { ...sources, modern: [...sources.modern, '1.12.2'] };
        assert.ok(inventoryProblems(both, surfaces).includes('1.12.2 is both legacy and modern'));
        const repeated = clone(surfaces);
        repeated.supported.push('v1_8_8');
        assert.ok(inventoryProblems(sources, repeated).includes('SUPPORTED_VERSIONS repeats v1_8_8'));
    });

    it('detects the missing implementation of a release added to the inventory, on every surface', () => {
        const added = { ...sources, modern: [...sources.modern, '26.4'] };
        const problems = inventoryProblems(added, surfaces);
        for (const surface of ['native/src/versioned/mod.rs modules', 'SUPPORTED_VERSIONS', 'rules_for_version', 'the tick dispatch', 'native/data/releases']) {
            assert.ok(problems.includes(`${surface} has no v26_4 for 26.4`), surface);
        }
    });

    it('takes every count from the lists themselves', () => {
        assert.equal(canonicalReleases(sources).length, sources.legacy.length + sources.modern.length);
        const verifier = fs.readFileSync(path.join(__dirname, '..', 'scripts', 'verify-native-package.cjs'), 'utf8');
        assert.match(verifier, /inventoryProblems\(inventorySources\(\), readSurfaces\(\)\)/);
        assert.doesNotMatch(fs.readFileSync(path.join(__dirname, '..', 'scripts', 'release-inventory.cjs'), 'utf8'), /\b(44|50)\b/);
    });
});

describe('the generated registries', () => {
    it('are what the checked-in sources say, and stable', () => {
        for (const [file, text] of generatedFiles()) assert.equal(normalized(fs.readFileSync(file, 'utf8')), text, file);
        assert.deepEqual(generatedFiles(), generatedFiles());
    });

    it('keep each named constructor on its exact literal release', () => {
        const engines = generatedFiles()[1][1];
        for (const version of ['1.15', '1.15.1', '1.21.10', '26.1.2']) {
            const name = version.replaceAll('.', '_');
            assert.ok(engines.includes(`class NativePhysics_${name} extends VersionedNativePhysics<'${version}'>`), version);
            assert.ok(engines.includes(`'${version}': NativePhysics_${name},`), version);
        }
    });

    it('gain and lose a release on every surface when the inventory does', () => {
        const added = [...canonicalReleases(), '26.4'];
        const [versions, engines] = generatedFiles(added).map(([, text]) => text);
        assert.ok(versions.includes(`'26.4',`));
        assert.ok(engines.includes('class NativePhysics_26_4'));
        assert.ok(engines.includes(`'26.4': NativePhysics_26_4`));
        assert.ok(vehicleEnginesSource(added).includes('class NativeBoatPhysics_26_4'));
        const removed = canonicalReleases().filter(version => version !== '1.15.1');
        const [fewer, fewerEngines] = generatedFiles(removed).map(([, text]) => text);
        assert.ok(!fewer.includes(`'1.15.1'`));
        assert.ok(!fewerEngines.includes('NativePhysics_1_15_1'));
    });
});
