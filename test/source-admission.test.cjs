'use strict';

// B002, B021: a source record is held to its pins, each failure names the release, and every registration is either
// reproducible or visibly blocked.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { sourceBlocker, sourceRecordProblems } = require('../scripts/source-admission.cjs');
const { allAdmissions } = require('../scripts/release-admission.cjs');
const { LEGACY_RELEASES } = require('../scripts/legacy-release-data.cjs');

const root = path.resolve(__dirname, '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'release-manifest.json'), 'utf8')).releases;
const legacy = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'legacy-release-data.json'), 'utf8')).releases;
const withSource = (version, change) => ({ ...manifest[version], source: { ...manifest[version].source, ...change } });

describe('source records', () => {
    it('admits every checked-in record', () => {
        for (const [version, record] of Object.entries(manifest)) assert.deepEqual(sourceRecordProblems(version, record), [], version);
    });

    it('fails a wrong embedded identity, naming the exact release', () => {
        assert.deepEqual(sourceRecordProblems('1.16.4', withSource('1.16.4', { embedded: '20w45a' })), ['1.16.4: the source embeds 20w45a']);
    });

    it('fails a source that is not the release, however it is labelled', () => {
        for (const status of ['rejected', 'directory-only', 'unavailable', 'stand-in']) {
            const problems = sourceRecordProblems('1.21.5', withSource('1.21.5', { status, embedded: null }));
            assert.match(problems.join(), new RegExp(`^1\\.21\\.5: the source is ${status}`));
        }
    });

    it('fails missing mappings', () => {
        assert.match(sourceRecordProblems('1.20.4', { ...manifest['1.20.4'], mappings: { kind: 'mojang', sha1: null } }).join(), /1\.20\.4: Mojang's mappings have no sha1/);
        assert.match(sourceRecordProblems('1.20.4', { ...manifest['1.20.4'], mappings: undefined }).join(), /1\.20\.4: the mappings are missing/);
        assert.match(sourceRecordProblems('1.20.4', { ...manifest['1.20.4'], mappings: { kind: 'unobfuscated', sha1: null } }).join(), /only 26\.x ships unobfuscated/);
        assert.match(sourceRecordProblems('1.20.4', withSource('1.20.4', { mapping: 'yarn' })).join(), /1\.20\.4: the source is in yarn names/);
    });

    it('fails each changed source hash', () => {
        assert.match(sourceRecordProblems('1.20.4', withSource('1.20.4', { commit: 'f'.repeat(40) })).join(), /1\.20\.4: the source commit f+ is not the pinned 6cc5ed/);
        assert.match(sourceRecordProblems('1.16.5', withSource('1.16.5', { clientJar: { sha1: 'f'.repeat(40) } })).join(), /1\.16\.5: the client jar f+ is not the pinned 37fd3c/);
        const remapped = { clientJar: { ...manifest['1.16.5'].source.clientJar, remappedSha256: 'f'.repeat(64) } };
        assert.match(sourceRecordProblems('1.16.5', withSource('1.16.5', remapped)).join(), /1\.16\.5: the remapped jar/);
        assert.match(sourceRecordProblems('26.3', withSource('26.3', { clientJar: { sha1: 'f'.repeat(40) } })).join(), /26\.3: the client jar/);
    });

    it('fails a record with nothing that identifies its source', () => {
        const { tree, clientJar, ...bare } = manifest['1.15'].source;
        assert.match(sourceRecordProblems('1.15', { ...manifest['1.15'], source: bare }).join(), /1\.15: the client jar undefined/);
        const { tree: gone, clientJar: alsoGone, ...nothing } = manifest['26.3'].source;
        assert.match(sourceRecordProblems('26.3', { ...manifest['26.3'], source: nothing }).join(), /26\.3: no commit, client jar or tree hash/);
    });

    it('keeps the snapshot branch of 1.16.4 out', () => {
        assert.equal(manifest['1.16.4'].source.rejectedRef.ref, 'origin/client1.16.4');
        assert.notEqual(manifest['1.16.4'].source.commit, manifest['1.16.4'].source.rejectedRef.commit);
    });

    it('gives all registrations a reproducible record', () => {
        const all = allAdmissions();
        for (const [version, status] of Object.entries(all)) assert.equal(status.source.status, 'exact', version);
        for (const version of Object.keys(manifest)) assert.equal(sourceBlocker(manifest[version]), null, version);
    });

    it('still blocks a yarn tree no record can make again', () => {
        const yarn = { ...manifest['1.12.2'], source: { status: 'embedded', embedded: '1.12.2', mapping: 'yarn', clientJar: { sha1: manifest['1.12.2'].source.clientJar.sha1 }, tree: manifest['1.12.2'].source.tree } };
        assert.match(sourceBlocker(yarn), /decompiler and yarn build are not recorded/);
    });

    it('holds a legacy release to the jar the recorder mapping remapped', () => {
        const record = structuredClone(manifest['1.12.2']);
        assert.deepEqual(sourceRecordProblems('1.12.2', record), []);
        record.source.clientJar.remappedSha256 = '0'.repeat(64);
        assert.match(sourceRecordProblems('1.12.2', record).join(), /the remapped jar 0{64} is not the pinned/);
    });
});

describe('legacy data', () => {
    it('tells numeric metadata from flattened states, release by release', () => {
        for (const { version } of LEGACY_RELEASES) assert.equal(legacy[version].flattened, version === '1.13.2', version);
        assert.equal(Object.keys(legacy['1.12.2'].fluidTags ?? {}).length, 0);
        assert.ok(Object.keys(legacy['1.13.2'].fluidTags).length > 0);
    });
});
