'use strict';

// node scripts/bench/table-digest.cjs <out.json>: every generated shape lookup of every release, by digest: each block, each
// state, each channel, the boxes and masks as the binding gives them. Two builds with one digest answer every lookup alike.

const { createHash } = require('node:crypto');
const { readFileSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');
const zlib = require('node:zlib');
const { ROOT, RELEASES, binding, registryFor } = require('./lib.cjs');

const CHANNELS = ['collision', 'outline', 'interaction', 'support'];
const digests = {};
let lookups = 0;
for (const version of RELEASES) {
    const hash = createHash('sha256');
    const names = new Set(registryFor(version).blocksArray.map(block => block.name));
    let table = null;
    try {
        table = JSON.parse(zlib.gunzipSync(readFileSync(join(ROOT, 'generated/block-shapes', `${version}.json.gz`))));
    } catch {
        // the releases before 1.14.4 have no such table
    }
    if (table) for (const id of Object.keys(table.blocks)) names.add(id);
    for (const name of [...names].sort()) {
        const count = binding.generatedBlockStateCount(version, name) ?? 0;
        const states = Math.max(count, 1);
        for (let index = 0; index < states; index += 1) {
            for (const channel of CHANNELS) {
                hash.update(JSON.stringify([name, index, channel, binding.generatedStateShape(version, name, channel, index)]));
                lookups += 1;
            }
        }
    }
    digests[version] = hash.digest('hex').slice(0, 24);
}
writeFileSync(process.argv[2], `${JSON.stringify({ lookups, digests }, null, 1)}\n`);
process.stdout.write(`${RELEASES.length} releases, ${lookups} lookups\n`);
