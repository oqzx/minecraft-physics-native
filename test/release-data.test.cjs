'use strict';

// scripts/verify-release-data.cjs: the inventories are compared name for name and each release's table is read back. The
// real data passes, and each check is shown to refuse what it is there for, naming the release.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { dispatchProblems, inventoryProblems, registrationProblems, releaseDataProblems, tableProblems } = require('../scripts/verify-release-data.cjs');
const { GENERATED, inventoryGaps } = require('../scripts/generated-inventory.cjs');
const physics = require('..');
const { LoadNativePhysics } = require('../dist/native');

const root = path.resolve(__dirname, '..');
const table = fs.readFileSync(path.join(root, 'native', 'data', 'releases', 'v1_20_6.rs'), 'utf8').replaceAll('\r\n', '\n');
const registry = require('../generated/block-registries.json').versions['1.20.6'].blocks;

const changed = (text, from, to) => {
    const result = text.replace(from, to);
    assert.notEqual(result, text, `nothing to replace: ${from}`);
    return result;
};
const problemsOf = text => tableProblems('1.20.6', text, { registry });

describe('the release data', () => {
    it('is sound for every release', () => assert.deepEqual(releaseDataProblems(root), []));

    it('agrees with what the binding and the public classes dispatch', () => {
        const versions = [...physics.SUPPORTED_VERSIONS];
        assert.equal(versions.length, 50);
        assert.deepEqual(dispatchProblems(LoadNativePhysics(), physics, versions), []);
        assert.match(dispatchProblems(LoadNativePhysics(), physics, [...versions.slice(1), '1.20.7']).join('\n'), /1\.20\.7: the binding refuses it/);
    });

    it('refuses equal counts of other names', () => {
        const modern = ['1.14.4', '1.15'];
        const legacy = ['1.8.8'];
        const all = [...legacy, ...modern];
        const lists = { modern, legacy, registered: all, tables: all, tableRegistrations: all, typescript: all, manifest: all, blockStates: modern, blockRegistries: modern };
        assert.deepEqual(inventoryProblems(lists), []);
        for (const [list, wrong] of [
            ['registered', ['1.8.8', '1.14.4', '1.15.1']],
            ['tables', ['1.8.9', '1.14.4', '1.15']],
            ['tableRegistrations', ['1.8.8', '1.14', '1.15']],
            ['typescript', ['1.8.8', '1.14.4', '1.16']],
            ['manifest', ['1.9.4', '1.14.4', '1.15']],
            ['blockStates', ['1.14.4', '1.15.2']],
            ['blockRegistries', ['1.14', '1.15']],
        ]) {
            assert.equal(wrong.length, lists[list].length);
            assert.notDeepEqual(inventoryProblems({ ...lists, [list]: wrong }), [], list);
        }
        assert.match(inventoryProblems({ ...lists, typescript: ['1.8.8', '1.14.4', '1.14.4'] }).join('\n'), /1\.14\.4 is twice in src\/versions\.ts/);
        assert.match(inventoryProblems({ ...lists, legacy: ['1.14.4'], tables: modern, registered: modern, tableRegistrations: modern, typescript: modern, manifest: modern }).join('\n'), /1\.14\.4 is both legacy and modern/);
    });

    it('refuses a table labelled for another release', () => {
        assert.deepEqual(problemsOf(table), []);
        const labelled = table.replaceAll('V1_20_6_', 'V1_20_5_');
        assert.match(problemsOf(labelled).join('\n'), /^1\.20\.6: the table holds V1_20_5_\w+, labelled for another release/m);
        const entry = 'GeneratedRelease { version: "1.20.6", blocks: V1_20_6_BLOCKS, fluids: V1_20_6_FLUIDS, fluid_tags: V1_20_6_FLUID_TAGS, block_tags: V1_20_6_BLOCK_TAGS },';
        const loaded = '#[path = "releases/v1_20_6.rs"]';
        assert.deepEqual(registrationProblems(`${loaded}\n    ${entry}\n`).problems, []);
        assert.match(registrationProblems(`${loaded}\n    ${entry.replace('blocks: V1_20_6', 'blocks: V1_20_5')}\n`).problems.join(), /1\.20\.6: its blocks are V1_20_5_BLOCKS/);
    });

    it('refuses a bad state domain, default or count, by release and block', () => {
        const count = /GeneratedBlock \{ id: "acacia_door".*?state_count: Some\(64\)/.exec(table)[0];
        assert.match(problemsOf(changed(table, count, count.replace('Some(64)', 'Some(63)'))).join('\n'), /1\.20\.6: block acacia_door has 63 states over a domain of 64/);
        const facing = 'V1_20_6_ACACIA_DOOR_FACING_DEFAULTS: &[&str] = &["Direction.NORTH"]';
        assert.match(problemsOf(changed(table, facing, facing.replace('NORTH', 'UP'))).join('\n'), /1\.20\.6: block acacia_door\.FACING defaults to Direction\.UP/);
        const values = 'V1_20_6_ACACIA_DOOR_FACING_VALUES: &[&str] = &["north", "east", "south", "west"]';
        assert.match(problemsOf(changed(table, values, values.replace('"east"', '"north"'))).join('\n'), /1\.20\.6: block acacia_door\.FACING repeats a value/);
        assert.match(problemsOf(changed(table, values, 'V1_20_6_ACACIA_DOOR_FACING_VALUES: &[&str] = &[]')).join('\n'), /1\.20\.6: block acacia_door\.FACING has an empty domain/);
        assert.match(problemsOf(changed(table, 'id: "acacia_door"', 'id: "acacia_doors"')).join('\n'), /1\.20\.6: block acacia_door of the source registry is not in the table/);
        const missing = changed(table, 'id: "acacia_fence"', 'id: "acacia_door"');
        assert.match(problemsOf(missing).join('\n'), /1\.20\.6: block acacia_door is listed twice/);
    });

    it('refuses absent or inconsistent fluid data', () => {
        const flowing = /^ *GeneratedFluid \{ id: "flowing_lava".*\n/m.exec(table)[0];
        assert.match(problemsOf(table.replace(flowing, '')).join('\n'), /1\.20\.6: required fluid flowing_lava is absent/);
        assert.match(problemsOf(changed(table, 'V1_20_6_WATER_STATE_COUNT: usize = 2', 'V1_20_6_WATER_STATE_COUNT: usize = 3')).join('\n'), /1\.20\.6: fluid water has 3 states over a domain of 2/);
        const water = /GeneratedFluid \{ id: "water".*?tick_delay: Some\(5\)/.exec(table)[0];
        assert.match(problemsOf(changed(table, water, water.replace('Some(5)', 'None'))).join('\n'), /1\.20\.6: fluid water has no tick_delay/);
    });

    it('refuses a tag naming a block the release does not have', () => {
        assert.match(problemsOf(changed(table, 'V1_20_6_WOOL_BLOCKS: &[&str] = &["black_wool"', 'V1_20_6_WOOL_BLOCKS: &[&str] = &["blacker_wool"')).join('\n'), /1\.20\.6: tag wool names block blacker_wool/);
    });
});

describe('the generated inventory', () => {
    it('accounts for every generated file', () => {
        const tracked = fs.readdirSync(path.join(root, 'generated')).map(name => `generated/${name}`);
        assert.deepEqual(inventoryGaps(tracked, file => fs.existsSync(path.join(root, file))), []);
        assert.match(inventoryGaps(['generated/stray.json'], () => true).join(), /generated\/stray\.json has no generator on record/);
        assert.match(inventoryGaps([], file => !file.includes('generate-version-manifest')).join(), /names scripts\/generate-version-manifest\.cjs, which does not exist/);
        for (const { output, step, pass, inputs } of GENERATED) assert.ok(output && step && pass && inputs, output);
    });
});
