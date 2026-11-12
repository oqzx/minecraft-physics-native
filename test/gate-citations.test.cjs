'use strict';

// B007, B046: every release gate of the engine names the vanilla line it rests on, in a release the gate divides. A gate added
// without one fails here; scripts/gate-citations.cjs lists the gates and docs/version-gates.md counts them.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { gateCitations, citesGate } = require('../scripts/gate-citations.cjs');

describe('the release gates\' citations (B007, B046)', () => {
    it('cite a line of a release either side of every gate', () => {
        const gates = gateCitations();
        assert.ok(gates.length > 700, `${gates.length} gates`);
        const uncited = gates.filter(({ cited }) => !cited).map(({ file, line, holder, boundary }) => `${file}:${line} ${holder} ${boundary}`);
        assert.deepEqual(uncited, [], 'run node scripts/gate-citations.cjs');
    });

    it('takes only a line of the gate\'s own release or of the registered release on either side', () => {
        const gate = { boundary: '1.16.2', lastBefore: '1.16.1', firstFrom: '1.16.2' };
        assert.equal(citesGate('(1.16.2 Entity.java:1064-1070 pushes it in water too)', gate), true);
        assert.equal(citesGate('(1.16.1 Entity.java:1039-1048 returns first)', gate), true);
        // another release, a class with no line, and a decimal that takes the release's place say nothing of this boundary
        assert.equal(citesGate('(1.16.5 Entity.java:1067-1072)', gate), false);
        assert.equal(citesGate('(1.16.2 Entity.java)', gate), false);
        assert.equal(citesGate('(1.16.2 by 1.0 Entity.java:1064)', gate), false);
        assert.equal(citesGate('(26.3 Entity.java:1698, 1.16.1 :1039)', gate), true);
    });
});
