'use strict';

// B073: docs/adapter-audit.json, what the adapter hands the engine against each release's own code, channel by channel, with what
// minecraft-data got wrong that the adapter corrects kept apart from what reaches the engine wrong.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { registeredReleases, compareReleases } = require('../scripts/registered-releases.cjs');
const { modern } = require('../scripts/audit-adapter-world.cjs');
const audit = require('../docs/adapter-audit.json');

const CHANNELS = ['collision', 'outline', 'interaction', 'support', 'friction', 'fluid'];

describe('the audit of the adapter', () => {
    it('covers all fifty releases, each by its own data', () => {
        assert.deepEqual(Object.keys(audit.releases), registeredReleases().map(({ version }) => version));
        for (const name of ['minecraft-data', 'prismarine-block', 'prismarine-registry']) assert.match(audit.packages[name], /^\d+\.\d+\.\d+/);
    });

    it('finds nothing that reaches the engine wrong, on any channel of any modern state', () => {
        for (const [version, entry] of Object.entries(audit.releases)) {
            if (compareReleases(version, '1.14') < 0) continue;
            assert.ok(entry.states > 5000, version);
            assert.ok(entry.contexts > 10000, version);
            for (const name of CHANNELS) assert.equal(entry.channels[name].actual, 0, `${version} ${name}`);
        }
    });

    it('tells a data defect the adapter masks from a difference', () => {
        // (minecraft-data's 1.16 chain has no axis in its shape; what the engine gets is the jar's)
        assert.ok(audit.releases['1.16'].channels.collision.masked > 0);
        for (const entry of Object.values(audit.releases)) if (entry.channels) assert.ok(entry.channels.collision.masked <= entry.channels.collision.compared);
    });

    it('holds the releases before 1.14 by their neighbours, nothing colliding wrong', () => {
        for (const [version, entry] of Object.entries(audit.releases)) {
            if (compareReleases(version, '1.14') >= 0) continue;
            assert.equal(entry.neighbours.collisionActual, 0, version);
            assert.deepEqual(entry.neighbours.outlineBlocks, [], version);
            assert.equal(entry.blockEntities.actual, 0, version);
            assert.ok(entry.blockEntities.compared > 1000, version);
        }
    });

    it('is what a fresh audit of a release says', () => {
        const fresh = JSON.parse(JSON.stringify(modern('1.16.1')));
        assert.deepEqual(fresh, audit.releases['1.16.1']);
    });
});
