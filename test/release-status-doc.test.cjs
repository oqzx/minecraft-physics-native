'use strict';

// B268: the status page lists every registered release, and nothing it shows outruns the evidence.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { collect, passRows, render, stateOf, withoutGeneratedLine } = require('../scripts/release-status-doc.cjs');
const { SUPPORTED_VERSIONS } = require('..');

const root = path.resolve(__dirname, '..');
const doc = fs.readFileSync(path.join(root, 'docs', 'release-status.md'), 'utf8').replace(/\r\n/g, '\n');
const data = collect();
const rows = [...doc.matchAll(/^\| ((?:\d+\.)+\d+) \| (.*) \|$/gm)].filter(([, , rest]) => rest.split(' | ').length === 8);

describe('the release status page', () => {
    it('is what the evidence says', () => {
        assert.equal(withoutGeneratedLine(doc), withoutGeneratedLine(render(data, 'Generated')), 'docs/release-status.md differs: node scripts/release-status-doc.cjs');
    });

    it('lists the registered releases exactly, in order', () => {
        assert.deepEqual(rows.map(([, release]) => release), [...SUPPORTED_VERSIONS]);
        assert.deepEqual(data.releases, [...SUPPORTED_VERSIONS]);
        assert.deepEqual(Object.keys(data.status), [...SUPPORTED_VERSIONS]);
    });

    it('names when it was made, on which engine', () => {
        assert.match(doc, /^Generated \d{4}-\d{2}-\d{2} on commit `\w+`, engine `\w{8}`\. \d+ of \d+ pins named that engine then\.$/m);
    });

    it('never shows a release as admitted that the status does not', () => {
        for (const [, release, rest] of rows) {
            const admitted = rest.split(' | ')[6];
            assert.equal(admitted, data.status[release].admitted ? 'yes' : 'no', release);
        }
    });

    it('takes missing evidence for what it is', () => {
        const pass = { notOk: 'none' };
        const passing = { source: { status: 'exact' }, data: { status: 'complete' }, implementation: { status: 'complete' }, admitted: false };
        assert.equal(stateOf(passing, pass), 'unpinned: no conformance pin');
        assert.equal(stateOf(passing, { notOk: '1 error' }), 'unpinned: no conformance pin, recordings short');
        assert.equal(stateOf({ ...passing, source: { status: 'blocked' } }, pass), 'blocked: source');
        assert.equal(stateOf({ ...passing, data: { status: 'incomplete' } }, pass), 'incomplete');
        assert.equal(stateOf({ ...passing, admitted: true }, pass), 'admitted');
    });

    it('refuses a pass table that misses or repeats a release', () => {
        const table = '| 1.8.8 | 10 | none | here |\n| 1.9.4 | 20 | none | here |\n';
        assert.equal(passRows(table, ['1.8.8', '1.9.4']).get('1.9.4').scenarios, '20');
        assert.throws(() => passRows(table, ['1.8.8', '1.9.4', '1.10.2']), /no row for 1\.10\.2/);
        assert.throws(() => passRows(`${table}| 1.9.4 | 20 | none | here |\n`, ['1.8.8', '1.9.4']), /in two rows/);
    });

    it('reads every kind of range in the pass table', () => {
        const releases = ['1.15', '1.15.1', '1.15.2', '1.16', '1.16.1'];
        const parsed = passRows('| 1.15 to 1.15.2 | 2 | none | remote |\n| 1.16, 1.16.1 | 3 | none | remote |\n', releases);
        assert.equal(parsed.get('1.15.1').scenarios, '2');
        assert.equal(parsed.get('1.16.1').scenarios, '3');
    });
});
