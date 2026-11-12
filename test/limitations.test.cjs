'use strict';

// B272, B269, B295: the documents that state limits are held to the files they cite. Every path a page names in backticks
// exists, the audit findings still open are all listed, and nothing says a release is done while none is admitted.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { SUPPORTED_VERSIONS } = require('..');

const root = path.resolve(__dirname, '..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8').replace(/\r\n/g, '\n');
const limitations = read('docs/limitations.md');
const notes = read('docs/release-notes.md');
const dispositions = require('../docs/parity-dispositions.json');
const status = require('../src/release-status.json').releases;

const named = disposition => dispositions.filter(entry => entry.disposition === disposition).map(entry => entry.id).sort();
const tableOf = (heading) => {
    const section = limitations.slice(limitations.indexOf(`### ${heading}`));
    return [...section.slice(0, section.indexOf('\n#', 5)).matchAll(/^\| (D-\d+) /gm)].map(match => match[1]).sort();
};

describe('the limitation documents', () => {
    it('list every audit finding still open, and no other', () => {
        assert.deepEqual(tableOf('Active'), named('active'));
        assert.deepEqual(tableOf('Fixed, not validated'), named('fixed-unvalidated'));
    });

    it('name only what exists', () => {
        for (const page of ['engine-architecture', 'limitations', 'release-notes', 'release-status', 'synchronization']) {
            const paths = [...read(`docs/${page}.md`).matchAll(/`((?:native|src|scripts|test|docs|generated|legacy-mappings)\/[\w./-]+\.\w+)`/g)].map(match => match[1]);
            assert.ok(paths.length > 0, page);
            for (const file of paths) assert.ok(fs.existsSync(path.join(root, file)), `docs/${page}.md names ${file}`);
        }
    });

    it('list the registered releases in the notes exactly', () => {
        const list = notes.slice(notes.indexOf('### Versions'), notes.indexOf('Registered means'));
        const releases = [...list.matchAll(/(?<![\w.])\d+\.\d+(?:\.\d+)?(?![\w.]*\d)/g)].map(match => match[0]).filter(release => release !== '50');
        assert.deepEqual(releases, [...SUPPORTED_VERSIONS]);
    });

    it('keep the notes unreleased, their pass result generated from the records, while nothing is admitted', () => {
        assert.ok(notes.includes('## Unreleased'));
        assert.ok(Object.values(status).every(entry => !entry.admitted));
        // (scripts/release-notes.cjs writes the pass result between its markers; with nothing admitted it supports nothing)
        assert.match(notes, /<!-- release-notes\.cjs: begin\.[\s\S]*Releases supported by this candidate, each with a complete evidence row: none\.[\s\S]*<!-- release-notes\.cjs: end -->/);
        assert.match(notes, /\| Admitted \| 0 \|/);
    });
});
