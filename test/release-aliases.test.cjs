'use strict';

// B021, B024: 1.8.8 is served from 1.8.9 only on a recorded comparison of the two clients, and an
// alias that does not hold cannot be cited.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { describe, it } = require('node:test');
const { ALIASES, REQUIRED_SCOPE, aliasRecord, aliasDefect, evidenceHash, javap } = require('../scripts/generate-release-aliases.cjs');
const { clientJarRoot } = require('../scripts/source-pins.cjs');
const { registeredReleases } = require('../scripts/registered-releases.cjs');
const { LEGACY_RELEASES } = require('../scripts/legacy-release-data.cjs');

const root = path.resolve(__dirname, '..');
const read = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const { aliases } = read('release-aliases.json');
const manifest = read('release-manifest.json').releases;
const jarsAtHand = fs.existsSync(path.join(clientJarRoot(), '1.8.9')) && javap() !== null;

describe('the 1.8.8 alias', () => {
    const alias = aliases['1.8.8'];

    it('is the only alias on record, and no release is served from another any more', () => {
        // (1.8.8 is read from its own decompiled jar; the alias stays as the proof that 1.8.9's code is 1.8.8's)
        assert.deepEqual(LEGACY_RELEASES.filter(release => release.source !== release.version), []);
        assert.deepEqual(Object.keys(aliases), ['1.8.8']);
        assert.equal(aliasDefect(alias), null);
    });

    it('accounts for every class of the client', () => {
        assert.equal(alias.classes, 2541);
        assert.equal(alias.codeIdentical + Object.keys(alias.differing).length, alias.classes);
        assert.ok(alias.byteIdentical <= alias.codeIdentical);
    });

    it('leaves no differing class without a review', () => {
        assert.deepEqual(Object.keys(alias.differing).sort(), Object.keys(ALIASES['1.8.8'].reviewed).sort());
        for (const reason of Object.values(alias.differing)) assert.ok(reason.length > 0);
    });

    it('covers the scope a served alias needs, though the release manifest no longer cites it', () => {
        assert.equal(manifest['1.8.8'].source.status, 'embedded');
        assert.equal(manifest['1.8.8'].source.embedded, '1.8.8');
        assert.deepEqual(alias.scope, REQUIRED_SCOPE);
    });

    it('repeats on the very jars', { skip: !jarsAtHand }, () => {
        assert.deepEqual(aliasRecord('1.8.8', ALIASES['1.8.8']), alias);
    });
});

describe('an alias that does not hold', () => {
    const base = aliases['1.8.8'];
    const resealed = (change) => {
        const record = { ...structuredClone(base), ...change };
        return { ...record, evidence: evidenceHash(record) };
    };

    it('fails with no evidence at all', () => {
        assert.equal(aliasDefect(undefined), 'no alias evidence');
    });

    it('fails with a scope that leaves something out', () => {
        assert.match(aliasDefect(resealed({ scope: REQUIRED_SCOPE.filter(entry => entry !== 'collision') })), /leaves out collision/);
    });

    it('fails without the hash of either end', () => {
        assert.match(aliasDefect(resealed({ clients: { ...base.clients, source: null } })), /no client hash for the source/);
        assert.match(aliasDefect(resealed({ remapped: { ...base.remapped, requested: 'abc' } })), /no remapped hash for the requested/);
    });

    it('fails when any part of the record was changed after it was sealed', () => {
        assert.match(aliasDefect({ ...structuredClone(base), byteIdentical: 2541 }), /evidence hash/);
    });

    it('is refused when either client is not the pinned one', () => {
        const fake = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-alias-'));
        for (const version of ['1.8.8', '1.8.9']) {
            fs.mkdirSync(path.join(fake, version));
            fs.writeFileSync(path.join(fake, version, `minecraft-${version}-client.jar`), 'another jar');
        }
        assert.throws(() => aliasRecord('1.8.8', ALIASES['1.8.8'], fake, 'javap'), /not the pinned one/);
        fs.rmSync(fake, { recursive: true, force: true });
    });

    it('never makes another release an alias', () => {
        const others = registeredReleases().map(({ version }) => version).filter(version => version !== '1.8.8');
        for (const version of others) assert.notEqual(manifest[version].source.status, 'alias', version);
    });
});
