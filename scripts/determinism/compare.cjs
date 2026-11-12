'use strict';

// node scripts/determinism/compare.cjs <a.json> <b.json>: where two builds' replays part, to the 100 ticks, with the command
// that prints both sides at that tick. Exit 1 on any difference, 0 with the counts of what was compared.

const { readFileSync } = require('node:fs');

const [left, right] = process.argv.slice(2, 4).map(path => JSON.parse(readFileSync(path, 'utf8')));
let runs = 0;
let ticks = 0;
let errors = 0;
const differences = [];
for (const version of new Set([...Object.keys(left.results), ...Object.keys(right.results)])) {
    const [a, b] = [left.results[version], right.results[version]];
    if (!a || !b) {
        differences.push(`${version}: on one side only`);
        continue;
    }
    for (const run of new Set([...Object.keys(a), ...Object.keys(b)])) {
        runs += 1;
        ticks += a[run]?.ticks ?? 0;
        if (a[run]?.error) errors += 1;
        if (JSON.stringify(a[run]) === JSON.stringify(b[run])) continue;
        const parted = (a[run]?.marks ?? []).findIndex((mark, index) => mark !== b[run]?.marks?.[index]);
        const from = parted < 0 ? a[run]?.marks?.length * 100 : parted * 100;
        differences.push(`${version} ${run}: parts in the 100 ticks from ${from} (errors: ${a[run]?.error ?? '-'} / ${b[run]?.error ?? '-'}); --dump ${version}/${run}/<tick>`);
    }
}
process.stdout.write(`${left.target} (${left.addon.slice(0, 12)}) against ${right.target} (${right.addon.slice(0, 12)}): ${runs} runs, ${ticks} ticks, ${errors} runs ended by the same refusal, ${differences.length} differences\n`);
for (const line of differences.slice(0, 40)) process.stdout.write(`${line}\n`);
process.exit(differences.length > 0 ? 1 : 0);
