'use strict';

// B209, B271: every exported field has a contract with a unit, an owner, releases and a source, and a new one fails until it does.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { FLAGS, OUTPUTS, OUTPUT_FLAGS, PACKED } = require('../scripts/state-bridge.cjs');
const { MAPPED, current, parse, render, renderMaps } = require('../scripts/field-catalogue.cjs');
const { isAnchor, rowProblem } = require('../scripts/field-contracts.cjs');

const root = path.resolve(__dirname, '..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8').replace(/\r\n/g, '\n');
const made = current();
const doc = read('docs/field-catalogue.md');

describe('the field catalogue', () => {
    it('is what the declarations and layouts say', () => {
        assert.equal(doc, render(made), 'docs/field-catalogue.md differs: node scripts/field-catalogue.cjs');
    });

    it('fails when a field is exported with no entry', () => {
        const added = parse(read('native/artifacts/native.d.ts').replace('export interface NativeSimulationConfig {', 'export interface NativeSimulationConfig {\n  brandNewField?: number'));
        assert.notEqual(render({ ...made, declarations: added }), doc);
    });

    it('is the field maps the declarations and the readers say', () => {
        assert.equal(read('docs/field-maps.md'), renderMaps(made), 'docs/field-maps.md differs: node scripts/field-catalogue.cjs');
        for (const name of MAPPED) assert.ok(read('docs/field-maps.md').includes(`## ${name}\n`), name);
    });

    const all = [...made.declarations.interfaces, ...made.adapters.flatMap(({ interfaces }) => interfaces)];

    it('gives every member of every interface a contract that says something', () => {
        const problems = [];
        for (const item of all) {
            const contract = made.contracts[item.name];
            if (!contract) { problems.push(`${item.name}: no contract`); continue; }
            for (const member of item.members) {
                const row = contract.fields[member.name];
                const problem = row ? rowProblem(member, row) : 'has no row';
                if (problem) problems.push(`${item.name}.${member.name} ${problem}`);
            }
            for (const name of Object.keys(contract.fields)) {
                if (!item.members.some(member => member.name === name)) problems.push(`${item.name}.${name} is no member`);
            }
        }
        assert.deepEqual(problems, []);
    });

    it('has no contract of what is not exported', () => {
        const names = new Set(all.map(item => item.name));
        assert.deepEqual(Object.keys(made.contracts).filter(name => !names.has(name)), []);
    });

    it('fails a row that names itself, is vague, or cites nothing', () => {
        const member = { type: 'number', note: '' };
        assert.ok(rowProblem(member, ['as vanilla holds it, in its unit', 'blocks', 'Entity.fallDistance']));
        assert.ok(rowProblem(member, ['fall distance', 'blocks', 'Entity.fallDistance']));
        assert.ok(rowProblem(member, ['The fall so far, which fall damage reads', '', 'Entity.fallDistance']));
        assert.ok(rowProblem(member, ['The fall so far, which fall damage reads', 'blocks', '']));
        assert.ok(rowProblem(member, ['The fall so far, which fall damage reads', 'blocks', 'binding: x']));
        assert.equal(rowProblem(member, ['The fall so far, which fall damage reads', 'blocks', 'Entity.fallDistance']), null);
    });

    it('holds every vanilla anchor to a release of the source record', () => {
        const missing = [];
        for (const { fields } of Object.values(made.contracts)) {
            for (const [name, [, , source]] of Object.entries(fields)) {
                if (isAnchor(source) && !made.evidence.records[source]) missing.push(`${name}: ${source}`);
            }
        }
        assert.deepEqual(missing, [], 'node scripts/field-evidence.cjs');
        assert.ok(made.evidence.sources.includes('26.3') && made.evidence.sources.includes('1.14.4'));
    });

    it('fills each column of each binding row', () => {
        const rows = doc.slice(doc.indexOf('## Interfaces of the binding'), doc.indexOf('## Packed layouts')).split('\n').filter(line => /^\| `\w+` \| /.test(line));
        assert.equal(rows.length, made.declarations.interfaces.reduce((sum, item) => sum + item.members.length, 0));
        for (const row of rows) {
            const cells = row.split(/(?<!\\)\|/).slice(1, -1).map(text => text.trim());
            assert.equal(cells.length, 10, row);
            const [name, , rust, absent, , , owner, releases, meaning, source] = cells;
            for (const [column, value] of Object.entries({ rust, absent, owner, releases, meaning, source })) assert.ok(value, `${name}: ${column}`);
            assert.match(releases, /^\d[\d.]*\.\.\d[\d.]*/, name);
        }
    });

    it('says what an absent field reads as, from the reader, wherever a state takes one', () => {
        const unread = [];
        for (const name of MAPPED) {
            const { members } = made.declarations.interfaces.find(item => item.name === name);
            for (const member of members) if (member.optional && !made.read[name][member.name]) unread.push(`${name}.${member.name}`);
        }
        assert.deepEqual(unread, ['NativeBlockUpdate.removeBlockEntity'], 'read in apply_block_updates\' own loop');
    });

    it('keeps the hand-written interfaces of src/native.ts to the generated ones', () => {
        const hand = new Map(parse(read('src/native.ts')).interfaces.map(item => [item.name, item.members.map(member => member.name).sort()]));
        for (const { name, members } of made.declarations.interfaces) {
            assert.deepEqual(hand.get(name), members.map(member => member.name).sort(), name);
        }
    });

    it('holds every packed mapping to a name of the layout', () => {
        const { NativePlayerInput, NativePlayerInputFlag, NativePlayerOutput, NativePlayerOutputFlag } = made.layouts;
        for (const [table, layout] of [[PACKED, NativePlayerInput], [FLAGS, NativePlayerInputFlag], [OUTPUTS, NativePlayerOutput], [OUTPUT_FLAGS, NativePlayerOutputFlag]]) {
            for (const [field, name] of Object.entries(table)) assert.ok(name in layout, `${field}: ${name}`);
        }
    });
});

describe('what the surface says of itself (B316)', () => {
    const read = file => fs.readFileSync(path.join(__dirname, '..', file), 'utf8');

    it('lists every mob kind the travel parses, in the binding and in the TypeScript', () => {
        const parse = read('native/src/moving_entity.rs').split('impl MobTravelKind {')[1].split('_ => None')[0];
        const names = [...parse.matchAll(/"([a-z_]+)"/g)].map(match => match[1]);
        assert.ok(names.length >= 33, String(names.length));
        const flat = text => text.replace(/\s*(\/\/\/|\*)\s*/g, ' ');
        const binding = flat(read('native/src/napi_moving_entity.rs').split('pub struct NativeMobTravelState')[1].split('pub kind')[0]);
        const declared = flat(read('src/native.ts').split('export interface NativeMobTravelState')[1].split('kind: string')[0]);
        for (const name of names) {
            assert.match(binding, new RegExp(`\\b${name}\\b`), name);
            assert.match(declared, new RegExp(`\\b${name}\\b`), name);
        }
    });

    it('names the families and the kinds of a nearby entity', () => {
        const api = read('native/src/napi_api.rs');
        const family = api.split('pub struct NativeEntityTravelSupport')[1].split('pub family')[0];
        for (const name of ['player', 'boat', 'ridden', 'mob', 'unsupported', 'unknown']) assert.ok(family.includes(`"${name}"`), name);
        const nearby = api.split('pub struct NativeNearbyEntity {')[1].split('pub kind')[0];
        const parsed = read('native/src/entity.rs').split('"living" => Self::Living')[0].length;
        const kinds = [...read('native/src/entity.rs').slice(parsed).split('_ => Self::Inert')[0].matchAll(/"([a-z_]+)" =>/g)].map(match => match[1]);
        assert.ok(kinds.includes('minecart'));
        for (const name of kinds) assert.ok(nearby.includes(`"${name}"`), name);
    });

    it('asks a caller for nothing the adapter does not read', () => {
        const types = read('src/mineflayer/types.ts');
        assert.doesNotMatch(types, /pistonDeltas/);
        assert.doesNotMatch(types.split('export interface Effect')[1].split('}')[0], /\bid\b/);
        assert.doesNotMatch(read('src/mineflayer/shulkers.ts'), /blockId/);
    });
});
