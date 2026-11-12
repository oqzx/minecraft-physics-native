// What the mapper hands the engine for the blocks that read their neighbours before 1.14, against LegacyNeighbourDump.java's fixtures:
// test/legacy-neighbours.test.cjs and scripts/audit-adapter-world.cjs.
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const adapter = require('../..');

const RELEASES = ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2'];
const folder = path.resolve(__dirname, '..', '..', 'test', 'fixtures', 'legacy-neighbours');
const fixture = version => zlib.gunzipSync(fs.readFileSync(path.join(folder, `${version}.tsv.gz`))).toString('utf8').split(/\r?\n/);
const flattened = version => version === '1.13.2';
/** Prismarine's state id of the id the jar gave: `id | meta << 12` before the flattening. */
const stateId = (version, id) => (flattened(version) ? id : ((id & 4095) << 4) | (id >> 12));
const parseBoxes = text => (text === '' ? [] : text.split(';').map(box => box.split(',').map(Number)));
const sorted = boxes => boxes.map(box => box.join()).sort().join(' ');
// (1.8's vine with no side and nothing above sets its bounds the wrong way round, and collisionRayTrace finds no point inside
// such bounds: 1.8.9 Block.java:513, 603-620, each test min <= v <= max. A ray meets none of it, as of a box the adapter leaves out)
const met = boxes => boxes.filter(([x0, y0, z0, x1, y1, z1]) => x1 > x0 && y1 > y0 && z1 > z0);
// (a row of `x` is a state whose block entity the blank level does not have)
const pair = text => (text === 'x' ? null : text.split(':').map(Number));

function setup(version) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const mapper = new adapter.BlockMapper(registry.version, registry);
    const described = new Map();
    const state = (id) => {
        const known = described.get(id);
        if (known) return known;
        const made = mapper.describe(Block.fromStateId(stateId(version, id), 0));
        described.set(id, made);
        return made;
    };
    return { registry, mapper, state };
}

/** Walks a dump: `check(home, cells, collision, outline, label)`, cells the neighbours by `x,y,z`. */
function walk(version, lines, state, ids, check) {
    const shapes = [];
    let pool = [];
    let compared = 0;
    for (const line of lines) {
        const fields = line.split('\t');
        if (fields[0] === 'shape') shapes[Number(fields[1])] = fields[2] === '-' ? null : parseBoxes(fields[2] ?? '');
        else if (fields[0] === 'pool') pool = fields[1].split(',').map(Number);
        else if (fields[0] === 'combo') {
            // (LegacyNeighbourDump.combos: xorshift32 from the seed, air 1 in 8, any stored state 3 in 8, the pool 4 in 8)
            const [, home, start, head, rows] = fields;
            const cells = head.split(';');
            let seed = Number(start) | 0;
            const next = () => {
                seed ^= seed << 13;
                seed ^= seed >>> 17;
                seed ^= seed << 5;
                return seed;
            };
            const pick = count => (next() >>> 0) % count;
            const homeState = state(Number(home));
            for (const row of rows.split(',')) {
                const around = new Map(cells.map((cell) => {
                    const mode = pick(8);
                    const id = mode === 0 ? ids[0] : mode < 4 ? ids[pick(ids.length)] : pool[pick(pool.length)];
                    return [cell, state(id)];
                }));
                const answer = pair(row);
                if (!answer) continue;
                check(homeState, around, shapes[answer[0]], shapes[answer[1]], `${version} ${homeState.name} ${home} among ${[...around].map(([cell, block]) => `${cell}=${block.name}:${block.meta}`).join(' ')}`);
                compared += 1;
            }
        }
        else if (fields[0] === 'side') {
            const [, home, cell, collision, outline, differ] = fields;
            const others = new Map((differ ? differ.split(',') : []).map((entry) => {
                const at = entry.indexOf(':');
                return [Number(entry.slice(0, at)), pair(entry.slice(at + 1))];
            }));
            const homeState = state(Number(home));
            for (const id of ids) {
                const answer = others.has(id) ? others.get(id) : [Number(collision), Number(outline)];
                if (!answer) continue;
                check(homeState, new Map([[cell, state(id)]]), shapes[answer[0]], shapes[answer[1]], `${version} ${homeState.name} ${home} with ${id} at ${cell}`);
                compared += 1;
            }
        } else if (fields[0] === 'grid') {
            const [, home, head, rows] = fields;
            const cells = head.split(';').map((part) => {
                const [cell, palette] = part.split('=');
                return [cell, palette.split('|').map(id => state(Number(id)))];
            });
            const homeState = state(Number(home));
            const turn = new Array(cells.length).fill(0);
            for (const row of rows.split(',')) {
                const answer = pair(row);
                if (answer) {
                    const around = new Map(cells.map(([cell, palette], i) => [cell, palette[turn[i]]]));
                    check(homeState, around, shapes[answer[0]], shapes[answer[1]], `${version} ${homeState.name} ${home} among ${[...around].map(([cell, block]) => `${cell}=${block.name}:${block.meta}`).join(' ')}`);
                    compared += 1;
                }
                for (let i = cells.length - 1; i >= 0; i -= 1) {
                    turn[i] += 1;
                    if (turn[i] < cells[i][1].length) break;
                    turn[i] = 0;
                }
            }
        }
    }
    return compared;
}

/** The random combinations the dump drew, all of a block's cells at once, by the block's stored id. */
function combos(version) {
    return fixture(version).filter(line => line.startsWith('combo\t')).map((line) => {
        const fields = line.split('\t');
        return { home: Number(fields[1]), cells: fields[3].split(';'), count: fields[4].split(',').length };
    });
}

/** The blocks a release's jar found reading their neighbours, and those it could not ask. */
function dependents(version) {
    const lines = fixture(version);
    const names = lines.filter(line => line.startsWith('depends\t')).map(line => line.split('\t')[1]);
    const refused = new Set(lines.filter(line => line.startsWith('refused\t')).map(line => line.split('\t')[1]));
    return { names, refused: [...refused] };
}

/** The outline each stored state sends with air around it, by block where the jar's differs. */
function aloneOutlines(version) {
    const lines = fixture(version);
    const { mapper, state } = setup(version);
    const shapes = [];
    const differ = new Map();
    let compared = 0;
    for (const line of lines) {
        const fields = line.split('\t');
        if (fields[0] === 'shape') shapes[Number(fields[1])] = fields[2] === '-' ? null : parseBoxes(fields[2] ?? '');
        if (fields[0] !== 'alone') continue;
        const [, name, id, , outline] = fields;
        // (no ray stops at these: Block.canCollide)
        // (1.8's torch sets its bounds in its rayTrace alone: generate-legacy-outlines.cjs has that box)
        if (!shapes[Number(outline)] || /^(air|fire|cave_air|void_air)$/.test(name) || (version === '1.8.8' && /torch$/.test(name))) continue;
        const home = state(Number(id));
        // (at the dump's cell: a plant's offset is of its column)
        const sent = mapper.updatesAt((x, y, z) => (x === 8 && y === 72 && z === 8 ? home : null), 8, 72, 8).flatMap(({ outlineShape, shape }) => {
            const box = outlineShape ?? shape;
            return box ? [[box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ]] : [];
        }).filter(([x0, y0, z0, x1, y1, z1]) => x1 > x0 && y1 > y0 && z1 > z0);
        compared += 1;
        if (sorted(sent) !== sorted(met(shapes[Number(outline)])) && !differ.has(name)) differ.set(name, `${id}: adapter ${sorted(sent)} jar ${sorted(shapes[Number(outline)])}`);
    }
    return { compared, differ };
}

/** The differences by block, with the first of each, as the mapper's boxes and the updates' outlines meet the jar's. */
function differences(version) {
    const lines = fixture(version);
    const { mapper, state } = setup(version);
    const found = { collision: new Map(), outline: new Map() };
    const note = (map, name, text) => {
        const entry = map.get(name) ?? { count: 0, first: text };
        entry.count += 1;
        map.set(name, entry);
    };
    const check = (home, around, collision, outline, label) => {
        const at = (x, y, z) => (x === 0 && y === 0 && z === 0 ? home : around.get(`${x},${y},${z}`) ?? state(0));
        const got = mapper.boxesAt(at, 0, 0, 0, home);
        if (sorted(got) !== sorted(collision)) note(found.collision, home.name, `${label}: adapter ${sorted(got)} jar ${sorted(collision)}`);
        if (outline) {
            // (the engine's clips filter the empty boxes the shorter list is padded with)
            const sent = mapper.updatesAt(at, 0, 0, 0).flatMap(({ outlineShape, shape }) => {
                const box = outlineShape ?? shape;
                return box ? [[box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ]] : [];
            }).filter(([x0, y0, z0, x1, y1, z1]) => x1 > x0 && y1 > y0 && z1 > z0);
            if (sorted(sent) !== sorted(met(outline))) note(found.outline, home.name, `${label}: adapter ${sorted(sent)} jar ${sorted(outline)}`);
        }
    };
    const ids = lines.filter(line => line.startsWith('stored\t')).flatMap(line => line.split('\t')[1].split(',')).map(Number);
    return { compared: walk(version, lines, state, ids, check), ...found };
}

module.exports = { RELEASES, differences, dependents, aloneOutlines, combos, setup };
