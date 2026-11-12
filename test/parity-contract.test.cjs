'use strict';

// B001, B008, B016, B022, B030: the contract covers every release of every interaction, a cell's status is computed
// from pinned evidence, and no release is complete while any required cell is not verified.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const {
    CONTEXTS, FAMILIES, INTERACTIONS, STATUSES,
    cell, contractCells, contractProblems, scopeHash, sealOf,
} = require('../scripts/parity-contract.cjs');
const { canonicalReleases } = require('../scripts/release-inventory.cjs');
const { compareReleases } = require('../scripts/registered-releases.cjs');
const { admission, admitted, allAdmissions, exportedNames } = require('../scripts/release-admission.cjs');
const { sha256OfModule } = require('../scripts/generate-release-manifest.cjs');
const crypto = require('node:crypto');

const root = path.resolve(__dirname, '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'release-manifest.json'), 'utf8')).releases;
const versions = canonicalReleases();
const ground = INTERACTIONS.find(interaction => interaction.id === 'player.ground');
const identity = { source: 'source', tables: 'tables', engine: 'engine' };
const sha256 = text => crypto.createHash('sha256').update(text).digest('hex');

/** A pin of one interaction, as the recorder package's scripts/pin-conformance.cjs would seal it. */
function pin(interaction, facts = {}, ids = identity) {
    const unsealed = {
        scope: scopeHash(interaction), source: ids.source, tables: ids.tables, engine: ids.engine,
        corpus: 3, selected: 3, matching: 3, exit: 0, ...facts,
    };
    return { ...unsealed, seal: sealOf(unsealed) };
}

describe('the parity contract', () => {
    it('is well formed, with every family the ticket lists', () => {
        assert.deepEqual(contractProblems(), []);
        assert.deepEqual(FAMILIES, ['player', 'living', 'boat', 'mount', 'minecart', 'projectile', 'environment']);
    });

    it('states accepted inputs, owned state, outputs, authority and context of every interaction', () => {
        for (const interaction of INTERACTIONS) {
            for (const field of ['inputs', 'state', 'outputs', 'authority']) assert.ok(interaction[field], `${interaction.id} ${field}`);
            assert.ok(interaction.requires.length > 0, interaction.id);
            for (const context of interaction.requires) assert.ok(CONTEXTS.includes(context), interaction.id);
        }
    });

    it('fails on a repeated interaction, a family with none, and a start that is no release', () => {
        assert.match(contractProblems([...INTERACTIONS, ground]).join(), /player\.ground is declared twice/);
        assert.match(contractProblems(INTERACTIONS.filter(({ family }) => family !== 'boat')).join(), /no interaction of boat/);
        assert.match(contractProblems([{ ...ground, id: 'x', since: '1.20.7' }]).join(), /starts at 1\.20\.7/);
        assert.match(contractProblems([{ ...ground, id: 'x', scenarios: [] }]).join(), /no scenario inventory/);
    });

    it('names entry points the package has', () => {
        const names = exportedNames();
        for (const interaction of INTERACTIONS) {
            for (const name of interaction.api) assert.ok(names.has(name), `${interaction.id}: ${name}`);
        }
    });


    it('holds a release to what the game has, unavailable before and required after', () => {
        const mcData = require('minecraft-data');
        const experimental = { camel: '1.19.3', wind_charge: '1.20.3' };
        for (const interaction of INTERACTIONS.filter(({ entity, item }) => entity || item)) {
            const name = interaction.entity ?? interaction.item;
            for (const version of versions) {
                const data = mcData(version);
                if (!data) continue;
                const has = !!(interaction.entity ? data.entitiesByName : data.itemsByName)[name];
                const key = `${interaction.id} ${version}`;
                if (compareReleases(version, interaction.since) >= 0) assert.ok(has, `${key}: required and absent`);
                else if (!experimental[name] || compareReleases(version, experimental[name]) < 0) assert.ok(!has, `${key}: in the game and unavailable`);
            }
        }
        const trident = INTERACTIONS.find(({ id }) => id === 'projectile.trident');
        assert.equal(cell('1.12.2', trident, {}, identity).status, 'unavailable');
        assert.equal(cell('1.13.2', trident, {}, identity).status, 'implemented');
        assert.equal(cell('1.21.1', INTERACTIONS.find(({ id }) => id === 'minecart.experimental'), {}, identity).status, 'unavailable');
    });
});

describe('a cell of the contract', () => {
    it('has one of the four statuses for every release of every interaction', () => {
        for (const version of versions) {
            const cells = contractCells(version, {}, identity);
            assert.deepEqual(Object.keys(cells), INTERACTIONS.map(({ id }) => id));
            for (const { status } of Object.values(cells)) assert.ok(STATUSES.includes(status));
        }
    });

    it('is verified only by a current, complete and matching pin', () => {
        const pins = version => ({ [version]: { [ground.id]: pin(ground) } });
        assert.equal(cell('26.3', ground, pins('26.3'), identity).status, 'verified');
        assert.equal(cell('26.2', ground, pins('26.3'), identity).status, 'implemented');
    });

    it('is partial when the pin covers part of the scenarios or does not all match', () => {
        for (const facts of [{ selected: 2, matching: 2 }, { matching: 2 }, { exit: 1 }, { corpus: 0, selected: 0, matching: 0 }]) {
            const pins = { '26.3': { [ground.id]: pin(ground, facts) } };
            assert.equal(cell('26.3', ground, pins, identity).status, 'partial', JSON.stringify(facts));
        }
    });

    it('falls back when anything the pin was made of changes, saying what', () => {
        const pins = { '26.3': { [ground.id]: pin(ground) } };
        for (const [change, reason] of [[{ source: 'other' }, /source record/], [{ tables: 'other' }, /tables/], [{ engine: 'other' }, /engine/]]) {
            const stale = cell('26.3', ground, pins, { ...identity, ...change });
            assert.equal(stale.status, 'implemented');
            assert.match(stale.reasons.join(), reason);
        }
    });

    it('loses its claim when the interaction\'s scope changes', () => {
        const pins = { '26.3': { [ground.id]: pin(ground) } };
        const wider = { ...ground, scenarios: [...ground.scenarios, 'physrec/x_*'] };
        const stale = cell('26.3', wider, pins, identity);
        assert.equal(stale.status, 'implemented');
        assert.match(stale.reasons.join(), /contract of the interaction changed/);
    });

    it('refuses a pin edited by hand', () => {
        const forged = { ...pin(ground, { matching: 2 }), matching: 3 };
        const stale = cell('26.3', ground, { '26.3': { [ground.id]: forged } }, identity);
        assert.equal(stale.status, 'implemented');
        assert.match(stale.reasons.join(), /seal/);
    });

    it('stays partial for a declared gap, whatever the traces say', () => {
        const gapped = { ...ground, gaps: [{ ticket: 'B000', reason: 'a known hole', from: '26.1' }] };
        const pins = { '26.3': { [gapped.id]: pin(gapped) }, '1.21.11': { [gapped.id]: pin(gapped) } };
        assert.equal(cell('26.3', gapped, pins, identity).status, 'partial');
        assert.equal(cell('1.21.11', gapped, pins, identity).status, 'verified');
    });
});

describe('admission from the contract', () => {
    const version = '26.3';
    const record = manifest[version];
    const source = sha256(JSON.stringify(record.source));
    const ids = { source, tables: record.tables.sha256, engine: 'engine' };
    const required = INTERACTIONS.filter(({ since }) => compareReleases(version, since) >= 0);
    const complete = { [version]: Object.fromEntries(required.map(interaction => [interaction.id, pin(interaction, {}, ids)])) };
    const inputs = { moduleSha256: sha256OfModule(version), legacy: true, pins: complete, engine: 'engine' };

    it('passes only with every required cell verified', () => {
        const status = admission(version, record, inputs);
        assert.equal(status.conformance.status, 'verified');
        assert.equal(admitted(status), true);
    });

    it('is kept out by removing source, data or conformance evidence, each on its own', () => {
        const noSource = admission(version, { ...record, source: { status: 'unavailable', embedded: null } }, inputs);
        assert.equal(noSource.source.status, 'missing');
        assert.equal(noSource.data.status, 'complete');
        assert.equal(admitted(noSource), false);

        const noData = admission(version, record, { ...inputs, moduleSha256: 'changed' });
        assert.equal(noData.source.status, 'exact');
        assert.equal(noData.data.status, 'incomplete');
        assert.equal(noData.conformance.status, 'verified');
        assert.equal(admitted(noData), false);

        const pins = { [version]: { ...complete[version] } };
        delete pins[version]['boat.control'];
        const noTrace = admission(version, record, { ...inputs, pins });
        assert.equal(noTrace.source.status, 'exact');
        assert.equal(noTrace.data.status, 'complete');
        assert.equal(noTrace.conformance.status, 'partial');
        assert.equal(admitted(noTrace), false);
    });

    it('is kept out by a partial or unserved cell, naming it', () => {
        const pins = { [version]: { ...complete[version], 'player.fluid': pin(INTERACTIONS.find(({ id }) => id === 'player.fluid'), { matching: 2 }, ids) } };
        const partial = admission(version, record, { ...inputs, pins });
        assert.equal(admitted(partial), false);
        assert.match(partial.conformance.reasons.join(), /player\.fluid: 2 of 3 scenarios match/);
        const unserved = admission(version, record, { ...inputs, entryPoints: new Set(['simulateVersionedTick']) });
        assert.equal(unserved.implementation.status, 'incomplete');
        assert.equal(admitted(unserved), false);
        const gapped = INTERACTIONS.map(interaction => (interaction.id === 'boat.control' ? { ...interaction, gaps: [{ ticket: 'B000', reason: 'a hole' }] } : interaction));
        const gap = admission(version, record, { ...inputs, interactions: gapped, pins: {} });
        assert.match(gap.implementation.reasons.join(), /boat\.control: gap \(B000\): a hole/);
    });

    it('does not count a passing pin of another engine', () => {
        const status = admission(version, record, { ...inputs, engine: 'rebuilt' });
        assert.notEqual(status.conformance.status, 'verified');
        assert.match(status.conformance.reasons.join(), /the engine changed/);
    });

    it('certifies no release today', () => {
        for (const [release, status] of Object.entries(allAdmissions())) assert.equal(admitted(status), false, release);
    });
});
