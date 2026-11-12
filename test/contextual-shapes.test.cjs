'use strict';

// B073, B152: the channels generated/block-shapes marks contextual, which test/adapter-block-states.test.cjs leaves out because no
// boxes stand for them: the plants at several cells, scaffolding, powder snow, the liquids and the light block in an
// EntityCollisionContext, a real PistonMovingBlockEntity at each progress. test/fixtures/contexts is the jar's own code in those
// contexts (scripts/java-oracle/OracleContext.java).

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { compare } = require('../scripts/java-oracle/contexts.cjs');

const report = JSON.parse(fs.readFileSync(path.resolve(__dirname, '..', 'generated', 'block-shapes.json'), 'utf8')).releases;
const RELEASES = Object.keys(report).filter(version => report[version].status === 'exact');

describe('contextual channels, the jar in the context against the engine through the adapter', () => {
    for (const version of RELEASES) {
        it(version, () => {
            const { compared, differ } = compare(version);
            if (process.env.CONTEXT_REPORT) {
                console.log(version, compared);
                for (const [what, { count, first }] of differ) console.log(' ', what, count, first.slice(0, 260));
            }
            assert.ok(compared > 10000, `${compared}`);
            assert.deepEqual([...differ.keys()], []);
        });
    }
});
