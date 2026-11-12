'use strict';

// B152: the collision of a block entity's block before 1.14, against the releases' own code. test/fixtures/legacy-block-entities is
// scripts/java-oracle/LegacyNeighbourDump.java with block entities in its level: a real PistonMovingBlockEntity in a piston_extension
// cell, moving each block MovingBlockTracker would make it move, at each progress its tick gives; and a real ShulkerBoxBlockEntity at
// each step of its lid, opening and closing, from 1.11. What is compared is what the adapter's trackers and NativeWorldSync give the
// engine.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { RELEASES, compare } = require('../scripts/java-oracle/legacy-block-entities.cjs');

describe('block entities of the releases before 1.14, the jar against the engine through the adapter', () => {
    for (const version of RELEASES) {
        it(version, () => {
            const { compared, differ } = compare(version);
            if (process.env.CONTEXT_REPORT) {
                console.log(version, compared);
                for (const [what, { count, first }] of differ) console.log(' ', what, count, first.slice(0, 300));
            }
            assert.deepEqual([...differ.keys()], []);
            assert.ok(compared > 1000, `${compared}`);
        });
    }
});
