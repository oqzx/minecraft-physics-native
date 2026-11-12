'use strict';

// B008, B022: registration, source, data, implementation and conformance are separate answers, each with its reasons.
// test/parity-contract.test.cjs has the cells that conformance reads.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { describe, it } = require('node:test');
const { admission, admitted, allAdmissions, statusDocument, statusProblems } = require('../scripts/release-admission.cjs');
const { sha256OfModule } = require('../scripts/generate-release-manifest.cjs');
const { evidenceHash } = require('../scripts/generate-release-aliases.cjs');

const root = path.resolve(__dirname, '..');
const read = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const manifest = read('release-manifest.json').releases;
const aliases = read('release-aliases.json').aliases;
const all = allAdmissions();

describe('release admission', () => {
    it('reports every registered release, none of them finished by being registered', () => {
        assert.deepEqual(Object.keys(all), Object.keys(manifest));
        for (const status of Object.values(all)) {
            assert.notEqual(status.conformance.status, 'verified');
            assert.equal(admitted(status), false);
        }
    });

    it('reads every release from its exact source, the legacy ones from their own decompiled jars', () => {
        for (const [version, status] of Object.entries(all)) assert.equal(status.source.status, 'exact', version);
    });

    it('holds the modern tables to their client', () => {
        for (const version of ['1.15.2', '1.20.3', '26.1.2', '26.3']) {
            assert.deepEqual(all[version].data.reasons.filter(reason => /client|class\b/.test(reason)), [], version);
        }
    });

    it('fails only the data dimension when a table changes', () => {
        const stale = admission('26.3', manifest['26.3'], { moduleSha256: 'changed', registry: undefined, legacy: true });
        assert.equal(stale.source.status, 'exact');
        assert.equal(stale.data.status, 'incomplete');
        assert.match(stale.data.reasons.join(), /not the one the manifest joined/);
    });

    it('names what the table lacks against the client', () => {
        const runtime = { blocks: [['stone', 1, 0], ['dirt', 1, 0]] };
        const status = admission('26.3', manifest['26.3'], {
            moduleSha256: sha256OfModule('26.3'), runtime, registry: { blocks: ['stone', 'marble'], unresolvedClassCount: 1 },
        });
        assert.deepEqual(status.data.reasons, ['1 blocks of the client are not in the table', '1 blocks in the table are not the client\'s', '1 blocks have no class']);
    });

    it('fails only the source dimension when an alias changes', () => {
        // (no release is served as an alias now; 1.8.8's record as it was, on its evidence, still is one)
        const record = { ...manifest['1.8.8'], source: { status: 'alias', embedded: '1.8.9', sourceRelease: '1.8.9', scope: aliases['1.8.8'].scope, evidence: evidenceHash(aliases['1.8.8']), mapping: 'yarn' } };
        const alias = { ...structuredClone(aliases['1.8.8']), byteIdentical: 1 };
        const inputs = { moduleSha256: sha256OfModule('1.8.8'), legacy: true };
        assert.equal(admission('1.8.8', record, { ...inputs, alias }).source.status, 'rejected');
        const resealed = { ...alias, evidence: evidenceHash(alias) };
        const status = admission('1.8.8', record, { ...inputs, alias: resealed });
        assert.match(status.source.reasons.join(), /changed since the manifest was joined/);
        assert.equal(status.data.status, 'complete');
    });

    it('says so on the command line, with a failing status for a release not admitted', () => {
        const run = spawnSync(process.execPath, [path.join(root, 'scripts', 'release-admission.cjs'), '1.16.5'], { encoding: 'utf8' });
        assert.equal(run.status, 1);
        assert.match(run.stdout, /1\.16\.5: source exact, data \w+, implementation \w+, conformance unverified/);
        assert.match(run.stdout, /no pinned trace evidence/);
        assert.equal(spawnSync(process.execPath, [path.join(root, 'scripts', 'release-admission.cjs'), '1.20.7']).status, 2);
    });

    it('ships the status the evidence supports, and fails one that claims more', () => {
        const shipped = JSON.parse(fs.readFileSync(path.join(root, 'src', 'release-status.json'), 'utf8'));
        assert.deepEqual(statusProblems(shipped, all), []);
        const forged = structuredClone(shipped);
        forged.releases['26.3'].admitted = true;
        assert.deepEqual(statusProblems(forged, all), ['src/release-status.json is stale for 26.3']);
        const hidden = structuredClone(shipped);
        hidden.releases['1.12.2'].source = { status: 'blocked', reasons: ['a reason the evidence does not give'] };
        assert.deepEqual(statusProblems(hidden, all), ['src/release-status.json is stale for 1.12.2']);
        assert.deepEqual(Object.keys(statusDocument(all).releases), Object.keys(manifest));
    });
});
