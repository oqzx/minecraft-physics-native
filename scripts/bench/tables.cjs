'use strict';

// node scripts/bench/tables.cjs [--out file]: what the generated tables cost. Source and section sizes are exact; the load
// and lookup times are CPU time, and a loaded machine moves them. Six legacy and 44 modern tables are kept apart throughout.

const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const { readFileSync, readdirSync, statSync } = require('node:fs');
const { join } = require('node:path');
const zlib = require('node:zlib');
const { ROOT, RELEASES, arg, binding, compareReleases, identity, measure, median, registryFor, rounded, sha256, writeJson } = require('./lib.cjs');

const LEGACY = new Set(['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']);

const fileOf = (dir, version) => join(ROOT, 'native/data', dir, `v${version.replace(/\./g, '_')}.rs`);

function sizes(dir) {
    const files = readdirSync(join(ROOT, 'native/data', dir));
    const rows = {};
    const byHash = new Map();
    const lines = new Map();
    let total = 0;
    let lineBytes = 0;
    for (const version of RELEASES) {
        const path = fileOf(dir, version);
        if (!files.includes(path.split(/[\\/]/).pop())) continue;
        const text = readFileSync(path);
        const hash = sha256(text);
        rows[version] = { bytes: text.length, sha256: hash.slice(0, 16), identicalTo: byHash.get(hash) ?? null };
        if (!byHash.has(hash)) byHash.set(hash, version);
        total += text.length;
        for (const line of text.toString('utf8').split('\n')) {
            lineBytes += line.length + 1;
            lines.set(line, line.length + 1);
        }
    }
    const of = legacy => Object.entries(rows).filter(([version]) => LEGACY.has(version) === legacy).reduce((sum, [, row]) => sum + row.bytes, 0);
    const distinctFiles = byHash.size;
    const distinctLineBytes = [...lines.values()].reduce((sum, value) => sum + value, 0);
    return {
        files: Object.keys(rows).length, distinctFiles, bytes: total,
        legacyBytes: of(true), modernBytes: of(false),
        bytesInFilesIdenticalToAnEarlierOne: Object.values(rows).filter(row => row.identicalTo).reduce((sum, row) => sum + row.bytes, 0),
        // lines shared by releases are what a cross-release pool could share; provenance and order are not counted out here
        distinctLineBytes, lineBytes, rows,
    };
}

/** The sections of the addon: what the tables and the code take on disk and in memory. */
function sections(path) {
    const bytes = readFileSync(path);
    if (bytes.toString('latin1', 0, 2) !== 'MZ') return { elf: true };
    const pe = bytes.readUInt32LE(0x3c);
    const count = bytes.readUInt16LE(pe + 6);
    const optional = bytes.readUInt16LE(pe + 20);
    const first = pe + 24 + optional;
    const rows = {};
    for (let i = 0; i < count; i += 1) {
        const at = first + i * 40;
        const name = bytes.toString('latin1', at, at + 8).replace(/\0.*$/, '');
        rows[name] = { virtualBytes: bytes.readUInt32LE(at + 8), fileBytes: bytes.readUInt32LE(at + 16) };
    }
    return rows;
}

/** A new process's load of the addon, with no data touched and with one release's tables walked. */
function loadCost(addon) {
    const script = `
        const before = process.cpuUsage(); const rss = process.memoryUsage().rss;
        const b = require(${JSON.stringify(addon)});
        const loaded = process.cpuUsage(before); const rssLoaded = process.memoryUsage().rss;
        const walk = process.cpuUsage();
        for (const name of ['stone', 'oak_stairs', 'water']) b.generatedBlockStateCount('26.3', name);
        const first = process.cpuUsage(walk);
        console.log(JSON.stringify({ loadCpuMs: (loaded.user + loaded.system) / 1000, rssGrowthKb: (rssLoaded - rss) >> 10, firstLookupCpuMs: (first.user + first.system) / 1000, rssAfterLookupKb: (process.memoryUsage().rss - rss) >> 10 }));`;
    const runs = [];
    for (let i = 0; i < 12; i += 1) {
        const child = spawnSync(process.execPath, ['-e', script], { encoding: 'utf8' });
        if (child.status !== 0) throw new Error(child.stderr);
        runs.push(JSON.parse(child.stdout));
    }
    const field = name => ({ median: rounded(median(runs.map(run => run[name]))), min: Math.min(...runs.map(run => run[name])), max: Math.max(...runs.map(run => run[name])) });
    return { runs: runs.length, loadCpuMs: field('loadCpuMs'), rssGrowthKb: field('rssGrowthKb'), firstLookupCpuMs: field('firstLookupCpuMs'), rssAfterLookupKb: field('rssAfterLookupKb'), loadSensitive: true };
}

/** The lookups the adapter makes of the tables, by the releases at both ends and between. */
function lookups() {
    const rows = {};
    for (const version of ['1.12.2', '1.13.2', '1.20.4', '26.3']) {
        const registry = registryFor(version);
        const names = registry.blocksArray.map(block => block.name);
        const middle = names[names.length >> 1];
        rows[version] = {
            blocks: names.length,
            stateCountFirstBlock: measure(() => binding.generatedBlockStateCount(version, names[0])),
            stateCountMiddleBlock: measure(() => binding.generatedBlockStateCount(version, middle)),
            stateCountLastBlock: measure(() => binding.generatedBlockStateCount(version, names[names.length - 1])),
            blockHasTag: measure(() => binding.generatedBlockHasTag(version, middle, 'minecraft:climbable')),
        };
        if (binding.generatedMethodShapeBoxes) rows[version].methodShapeBoxes = measure(() => binding.generatedMethodShapeBoxes(version, 'stone', 'collision'));
    }
    return rows;
}

/** What the pool of distinct shapes saves: the boxes a table holds, and those it would hold with each state's written out. */
function pooling() {
    const rows = {};
    for (const version of RELEASES) {
        let table;
        try {
            table = JSON.parse(zlib.gunzipSync(readFileSync(join(ROOT, 'generated/block-shapes', `${version}.json.gz`))));
        } catch {
            continue;
        }
        const pooled = table.shapes.reduce((sum, boxes) => sum + boxes.length, 0);
        let written = 0;
        let references = 0;
        for (const entry of Object.values(table.blocks)) {
            for (const channel of ['collision', 'outline', 'interaction', 'support']) {
                for (const value of [entry[channel]].flat()) {
                    references += 1;
                    if (value !== null) written += table.shapes[value]?.length ?? 0;
                }
            }
        }
        rows[version] = { distinctShapes: table.shapes.length, pooledBoxes: pooled, boxesWithoutThePool: written, references };
    }
    return rows;
}

const addon = join(ROOT, 'native/artifacts', readdirSync(join(ROOT, 'native/artifacts')).find(name => name.endsWith('.node')));
const releases = sizes('releases');
const shapes = sizes('shapes');
const out = arg('out', join(ROOT, 'docs/benchmarks/tables.json'));
writeJson(out, {
    suite: 'tables',
    identity: identity(),
    source: { releases, shapes, sharedRustBytes: ['mod.rs', 'block_shapes.rs'].reduce((sum, name) => sum + statSync(join(ROOT, 'native/data', name)).size, 0) },
    addon: { bytes: statSync(addon).size, sections: sections(addon) },
    variants: JSON.parse(arg('variants', '{}')),
    pooling: pooling(),
    load: loadCost(addon),
    lookups: lookups(),
    releasesChecked: RELEASES.slice().sort(compareReleases).length,
    digest: createHash('sha256').update(readFileSync(join(ROOT, 'native/data/mod.rs'))).digest('hex').slice(0, 16),
});
process.stderr.write(`wrote ${out}\n`);
