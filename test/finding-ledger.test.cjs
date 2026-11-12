'use strict';

// B011, B012: every finding of the review is listed once, with its own evidence, and an open one stays open.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { describe, it } = require('node:test');

const root = path.resolve(__dirname, '..');
const ledger = fs.readFileSync(path.join(root, 'docs', 'finding-ledger.md'), 'utf8');
const rows = [...ledger.matchAll(/^\| (M\d\d) \| (B\d+) \|.*\| (fixed|\*\*open\*\*) \| (.*) \|$/gm)]
    .map(([, finding, ticket, state, evidence]) => ({ finding, ticket, state, evidence }));

describe('the finding ledger', () => {
    it('lists each of the 32 findings once', () => {
        assert.deepEqual(rows.map(row => row.finding), Array.from({ length: 32 }, (_, index) => `M${String(index + 1).padStart(2, '0')}`));
    });

    it('gives each fixed finding its own commit and a test that exists', () => {
        for (const { finding, state, evidence } of rows.filter(row => row.state === 'fixed')) {
            const cited = [...evidence.matchAll(/`([^`]+)`/g)].map(match => match[1]);
            const files = cited.filter(name => name.includes('/'));
            assert.ok(files.length > 0, `${finding} names no test`);
            for (const file of files) assert.ok(fs.existsSync(path.join(root, file)), `${finding}: ${file}`);
            for (const commit of cited.filter(name => /^[0-9a-f]{7,12}$/.test(name))) {
                execFileSync('git', ['cat-file', '-e', `${commit}^{commit}`], { cwd: root });
            }
        }
    });

    it('leaves no finding open', () => {
        assert.deepEqual(rows.filter(row => row.state !== 'fixed').map(row => row.finding), []);
    });
});
