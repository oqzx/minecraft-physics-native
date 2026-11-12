// Compares the jar's collision shape of every block state (OracleStates.java) with the boxes the adapter hands the
// engine, as a set of boxes and in toAabbs order, and lists the distinct shapes with their grid lines:
//
//   node scripts/java-oracle/compare-states.cjs RELEASE states.txt [distinct.txt]
//
// `distinct.txt`: `shape boxes... x-lines y-lines z-lines`, for native/fixtures/java-oracle-shapes-RELEASE.txt.
const { readFileSync, writeFileSync } = require('node:fs');
const path = require('node:path');

const [release, statesFile, distinctFile] = process.argv.slice(2);
const { BlockMapper, releaseCheck } = require(path.resolve(__dirname, '../../dist/mineflayer/blocks.js'));
const registry = require('prismarine-registry')(release);
const Block = require('prismarine-block')(registry);
// BlockMapper's answers to FlowingFluid and the camera, from the same state
const native = require(path.resolve(__dirname, '../../native/artifacts/native.js'));
const mapper = new BlockMapper(releaseCheck(release), registry, undefined, undefined,
    (name, index, kind) => native.generatedStateShape(release, name, kind, index));
const STEPS = { 2: [0, -1], 3: [0, 1], 4: [-1, 0], 5: [1, 0] };
const faces = { checked: 0, differ: new Map() };

const list = (words, at, each) => {
    const count = Number(words[at]);
    const out = words.slice(at + 1, at + 1 + count * each).map(Number);
    return [out, at + 1 + count * each];
};
const boxesOf = flat => Array.from({ length: flat.length / 6 }, (_, i) => flat.slice(i * 6, i * 6 + 6));
const key = box => box.join(' ');
const value = text => (text === 'true' ? true : text === 'false' ? false : /^-?\d+$/.test(text) ? Number(text) : text);

// the same solid in other boxes: every cell of the lines of both is in both or neither
function sameUnion(left, right) {
    const lines = axis => [...new Set([...left, ...right].flatMap(box => [box[axis], box[axis + 3]]))].sort((a, b) => a - b);
    const inside = (boxes, x, y, z) => boxes.some(b => b[0] < x && x < b[3] && b[1] < y && y < b[4] && b[2] < z && z < b[5]);
    const [xs, ys, zs] = [lines(0), lines(1), lines(2)];
    const middles = list => list.slice(1).map((value, i) => (value + list[i]) / 2);
    for (const x of middles(xs)) for (const y of middles(ys)) for (const z of middles(zs)) {
        if (inside(left, x, y, z) !== inside(right, x, y, z)) return false;
    }
    return true;
}

const tally = { states: 0, same: 0, order: 0, merged: 0, differ: 0, absent: 0 };
const differ = new Map();
const absent = new Set();
const distinct = new Map();
for (const line of readFileSync(statesFile, 'utf8').split('\n')) {
    if (!line.startsWith('state ')) continue;
    tally.states += 1;
    const words = line.split(' ');
    const [, name, propText] = words;
    const [flat, xAt] = list(words, 4, 6);
    const [xs, yAt] = list(words, xAt, 1);
    const [ys, zAt] = list(words, yAt, 1);
    const [zs, facesAt] = list(words, zAt, 1);
    distinct.set(`${flat.length / 6} ${flat.join(' ')} ${xs.length} ${xs.join(' ')} ${ys.length} ${ys.join(' ')} ${zs.length} ${zs.join(' ')}`, true);
    if (!registry.blocksByName[name]) {
        absent.add(name);
        tally.absent += 1;
        continue;
    }
    const props = {};
    if (propText !== '-') for (const pair of propText.split(',')) { const [k, v] = pair.split('='); props[k] = value(v); }
    const block = Block.fromProperties(name, props, 0);
    // (the mapper's, which corrects what minecraft-data holds of a release: the states of 1.16 and 1.16.1's chains)
    const described = mapper.describe(block);
    const adapter = described.boxes;
    if (words.length >= facesAt + 6) {
        const say = (face, actual) => {
            faces.checked += 1;
            if ((words[facesAt + face] === '1') !== actual) faces.differ.set(`${name} face ${face} jar ${words[facesAt + face]}`, propText);
        };
        say(0, mapper.bottomFaceSturdy(described));
        for (const [face, step] of Object.entries(STEPS)) say(Number(face), mapper.flowBlocks.faceSturdy(described, step));
    }
    const java = boxesOf(flat);
    // (the table's boxes for an offset block are the shape at its own zero offset, boxesAt moves them)
    const a = adapter.map(key);
    const j = java.map(key);
    if (a.join('|') === j.join('|')) { tally.same += 1; continue; }
    if ([...a].sort().join('|') === [...j].sort().join('|')) { tally.order += 1; continue; }
    if (sameUnion(adapter, java)) { tally.merged += 1; continue; }
    tally.differ += 1;
    const entry = differ.get(name) ?? { count: 0, first: `${propText}\n    java    ${j.join(' | ')}\n    adapter ${a.join(' | ')}` };
    entry.count += 1;
    differ.set(name, entry);
}
console.log(release, tally, `distinct ${distinct.size}`);
if (absent.size) console.log('absent from the registry:', [...absent].join(' '));
for (const [name, entry] of differ) console.log(`${name} x${entry.count} ${entry.first}`);
console.log(`faces ${faces.checked}, ${faces.differ.size} block faces differ`);
for (const [what, props] of [...faces.differ].slice(0, 40)) console.log(`  ${what} ${props}`);
if (distinctFile) writeFileSync(distinctFile, [...distinct.keys()].map(text => `shape ${text}`).join('\n') + '\n');
