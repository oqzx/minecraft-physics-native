'use strict';

// generated/release-manifest.json: one record per registered release, each fact exact or visibly
// absent (B020), and a source admitted by the release embedded in it, never by its label (B037).

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { describe, it } = require('node:test');
const { registeredReleases } = require('../scripts/registered-releases.cjs');
const { joinRecord, sha256OfModule } = require('../scripts/generate-release-manifest.cjs');
const { refIsRelease } = require('../scripts/source-identity.cjs');
const { PINNED_SOURCES, extractedRepo } = require('../scripts/source-pins.cjs');

const root = path.resolve(__dirname, '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'release-manifest.json'), 'utf8'));
const runtime = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'runtime-registries.json'), 'utf8')).releases;
const mojang = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'mojang-releases.json'), 'utf8')).releases;
const versions = registeredReleases().map(({ version }) => version);
const repoAtHand = fs.existsSync(extractedRepo());

describe('the joined release records', () => {
    it('has one record per registered release, under its exact name', () => {
        assert.deepEqual(Object.keys(manifest.releases), versions);
        for (const version of versions) assert.equal(manifest.releases[version].release, version);
    });

    it('has each release\'s own protocol and data version', () => {
        const expected = { '1.8.8': [47, 94], '1.13.2': [404, 1631], '1.14.4': [498, 1976], '1.21.5': [770, 4325], '26.1.2': [775, 4790], '26.3': [777, 5023] };
        for (const [version, [protocol, dataVersion]] of Object.entries(expected)) {
            assert.equal(manifest.releases[version].protocol, protocol, version);
            assert.equal(manifest.releases[version].dataVersion, dataVersion, version);
        }
        for (const version of versions) {
            assert.equal(typeof manifest.releases[version].protocol, 'number', version);
            assert.ok(manifest.releases[version].protocolSource, version);
        }
    });

    it('takes a dumped release\'s protocol from its client', () => {
        for (const [version, dump] of Object.entries(runtime)) {
            assert.equal(manifest.releases[version].protocol, dump.runtimeProtocolVersion, version);
            assert.equal(manifest.releases[version].protocolSource, 'client-runtime', version);
        }
    });

    it('names the mappings of a release, or why it has none', () => {
        for (const release of mojang.filter(({ id }) => versions.includes(id))) {
            const { mappings } = manifest.releases[release.id];
            if (release.mappings) assert.deepEqual(mappings, { kind: 'mojang', sha1: release.mappings.sha1 }, release.id);
            else assert.equal(mappings.sha1, null, release.id);
        }
        assert.equal(manifest.releases['26.3'].mappings.kind, 'unobfuscated');
        assert.equal(manifest.releases['1.8.8'].mappings.kind, 'unpublished');
    });

    it('refuses to join two reports that disagree', () => {
        const record = {
            runtime: { runtimeProtocolVersion: 770 },
            protocolRow: { version: 771, dataVersion: 1 },
            mojang: null,
            source: { status: 'unavailable', embedded: null },
            tables: { sha256: '' },
        };
        assert.throws(() => joinRecord('1.21.5', record), /client reports protocol 770, minecraft-data 771/);
    });

    it('is current with every release\'s table', () => {
        for (const version of versions) assert.equal(manifest.releases[version].tables.sha256, sha256OfModule(version), version);
    });

    it('regenerates to the same bytes', () => {
        execFileSync(process.execPath, [path.join(root, 'scripts', 'generate-release-manifest.cjs'), '--check'], { stdio: 'pipe' });
    });
});

describe('source identity', () => {
    const status = version => manifest.releases[version].source;

    it('keeps each evidence class visible', () => {
        const byStatus = {};
        for (const version of versions) (byStatus[status(version).status] ??= []).push(version);
        // (every release embeds its own name: 1.8.8 is read from its own jar, no longer as 1.8.9's alias)
        assert.equal(byStatus.embedded.length, 50);
        assert.deepEqual(Object.keys(byStatus).sort(), ['embedded']);
    });

    it('does not take 20w45a for 1.16.4, whatever its hashes', () => {
        assert.equal(status('1.16.4').embedded, '1.16.4');
        assert.equal(status('1.16.4').rejectedRef.ref, 'origin/client1.16.4');
        assert.equal(PINNED_SOURCES['1.16.4'], undefined);
    });

    it('reads the legacy releases from their own jars, and no snapshot or candidate for final 26.1.2', () => {
        for (const version of ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']) {
            assert.equal(status(version).embedded, version);
            assert.equal(status(version).mapping, 'recorder');
            assert.equal(status(version).decompiler, 'vineflower 1.12.0');
            assert.match(status(version).clientJar.remappedSha256, /^[0-9a-f]{64}$/);
        }
        assert.equal(PINNED_SOURCES['26.1.2'], undefined);
    });

    it('marks a decompiled release with the jar it came from', () => {
        assert.deepEqual(status('26.1.2').clientJar, { sha1: '4e618f09a0c649dde3fdf829df443ce0b8831e65' });
        assert.equal(status('26.1.2').decompiler, 'vineflower 1.12.0');
        assert.match(status('1.15.2').clientJar.remappedSha256, /^[0-9a-f]{64}$/);
    });

    it('records no path of this machine', () => {
        assert.equal(/scratchpad|AppData|\.gradle/.test(JSON.stringify(manifest)), false);
    });

    it('pins an immutable commit for each branch it takes, embedding its release', () => {
        for (const [version, commit] of Object.entries(PINNED_SOURCES)) {
            assert.match(commit, /^[0-9a-f]{40}$/, version);
            assert.equal(status(version).commit, commit, version);
            assert.equal(status(version).embedded, version);
        }
    });

    it('refuses to unpack a commit of another release', { skip: !repoAtHand }, () => {
        const { materialize } = require('../scripts/materialize-sources.cjs');
        const previous = process.env.MC_SOURCE_CACHE;
        const cache = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-source-cache-'));
        process.env.MC_SOURCE_CACHE = cache;
        try {
            assert.throws(() => materialize('1.16.4', '08e782cbdb11cfaec9cdb27240b38b105e5054e6'), /embeds 20w45a/);
            assert.deepEqual(fs.readdirSync(cache), []);
        } finally {
            if (previous === undefined) delete process.env.MC_SOURCE_CACHE;
            else process.env.MC_SOURCE_CACHE = previous;
            fs.rmSync(cache, { recursive: true, force: true });
        }
    });

    it('takes no branch at its label', () => {
        const repository = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-identity-'));
        const git = (...args) => execFileSync('git', ['-C', repository, '-c', 'user.name=t', '-c', 'user.email=t@t', ...args], { stdio: 'pipe' });
        git('init', '-q');
        fs.mkdirSync(path.join(repository, 'client'));
        const commit = (file, text, branch) => {
            fs.writeFileSync(path.join(repository, file), text);
            git('add', '.');
            git('commit', '-q', '-m', branch);
            git('update-ref', `refs/remotes/origin/client${branch}`, 'HEAD');
        };
        commit('client/readme.txt', 'no version here', '9.1');
        commit('client/version.json', '{"id":"9.2-rc-1"}', '9.2');
        commit('client/version.json', '{"id":"9.3"}', '9.3');
        assert.equal(refIsRelease(repository, '9.1'), false);
        assert.equal(refIsRelease(repository, '9.2'), false);
        assert.equal(refIsRelease(repository, '9.3'), true);
        fs.rmSync(repository, { recursive: true, force: true });
    });
});
