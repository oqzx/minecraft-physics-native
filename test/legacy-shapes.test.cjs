'use strict';

// B152: the releases before 1.14, state by state, against their own code in a level of their own: what the mapper hands
// the engine for a block alone. What neighbours do is for the recordings (pa_ on the old releases).

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const { describe, it } = require('node:test');
const adapter = require('..');
const { parseDump } = require('../scripts/generate-legacy-shapes.cjs');

const root = path.resolve(__dirname, '..');
const { releases } = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'block-shapes-legacy.json'), 'utf8'));
const table = version => JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, 'generated', 'block-shapes-legacy', `${version}.json.gz`))).toString('utf8'));
const flattened = version => version === '1.13.2';

/** Prismarine's state id of the id the jar gave: `id | meta << 12` before the flattening. */
const stateId = (version, id) => (flattened(version) ? id : ((id & 4095) << 4) | (id >> 12));

function sorted(boxes) {
    return [...boxes].map(box => box.join()).sort();
}

describe('the report', () => {
    it('has the six releases before 1.14, with what each state is alone', () => {
        assert.deepEqual(Object.keys(releases), ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']);
        for (const entry of Object.values(releases)) {
            assert.equal(entry.status, 'alone');
            assert.ok(entry.states > 5000);
            assert.match(entry.sha256, /^[0-9a-f]{64}$/);
        }
    });

    it('refuses only what a block entity holds, and says so', () => {
        for (const [version, entry] of Object.entries(releases)) {
            const names = Object.keys(entry.refused);
            for (const name of names) assert.match(name, /piston|shulker/, `${version} ${name}`);
            assert.ok(names.length > 0, version);
            const { refused } = table(version);
            for (const channels of Object.values(refused)) {
                for (const reasons of Object.values(channels)) for (const reason of Object.keys(reasons)) assert.match(reason, /getBlockEntity/, version);
            }
        }
    });

    it('reads a dump, and fails on a line it does not know or a dump that is short', () => {
        const rows = Array.from({ length: 101 }, (_, index) => ['state', `b${index}`, index, '', '0.0,0.0,0.0,1.0,1.0,1.0', '-'].join('\t'));
        const parsed = parseDump(rows.join('\n'), 'x');
        // (no `full` column: a release from 1.13)
        assert.deepEqual(parsed.blocks.b1, [[1, '', 0, null, null]]);
        const full = parseDump([...rows.slice(1), ['state', 'stairs', 7, '', '0.0,0.0,0.0,1.0,0.5,1.0', '-', '1'].join('\t')].join('\n'), 'x');
        assert.deepEqual(full.blocks.stairs, [[7, '', 1, null, 1]]);
        assert.throws(() => parseDump(['nonsense\tx', ...rows].join('\n'), 'x'), /unknown dump line/);
        assert.throws(() => parseDump(rows.slice(0, 5).join('\n'), 'x'), /only 5 blocks/);
    });
});

describe('every state through the adapter, against its own release', () => {
    for (const version of Object.keys(releases)) {
        it(`${version}`, () => {
            const registry = require('prismarine-registry')(version);
            const Block = require('prismarine-block')(registry);
            const mapper = new adapter.BlockMapper(registry.version, registry);
            const { shapes, blocks } = table(version);
            const seen = new Set();
            let compared = 0;
            for (const [name, rows] of Object.entries(blocks)) {
                // a door's upper half reads the lower one: both sides have the block's default below
                const lower = rows.find(([, props]) => /half=lower/.test(props) && /facing=north/.test(props) && /open=false/.test(props) && !/powered=true/.test(props) && /hinge=left/.test(props));
                for (const [id, props, collision] of rows) {
                    // `short` and the like are no metadata: a block as placed has the default
                    if (collision === null || /short=true/.test(props) || seen.has(id)) continue;
                    seen.add(id);
                    const described = mapper.describe(Block.fromStateId(stateId(version, id), 0));
                    const below = name.endsWith('_door') && /half=upper/.test(props) && lower ? mapper.describe(Block.fromStateId(stateId(version, lower[0]), 0)) : null;
                    const at = (x, y, z) => (x === 0 && z === 0 ? (y === 0 ? described : y === -1 ? below : null) : null);
                    assert.deepEqual(sorted(mapper.boxesAt(at, 0, 0, 0, described)), sorted(shapes[collision]), `${version} ${name} ${props}`);
                    compared += 1;
                }
            }
            assert.ok(compared > 1000, `${compared}`);
        });
    }
});

describe('what the jar settles', () => {
    it('1.13.2\'s bed is a cuboid and its cauldron a tub, not the later shapes of minecraft-data', () => {
        const registry = require('prismarine-registry')('1.13.2');
        const Block = require('prismarine-block')(registry);
        const mapper = new adapter.BlockMapper(registry.version, registry);
        const { shapes, blocks } = table('1.13.2');
        const [id, , collision] = blocks.white_bed[0];
        assert.deepEqual(shapes[collision], [[0, 0, 0, 1, 0.5625, 1]]);
        assert.deepEqual(mapper.describe(Block.fromStateId(id, 0)).boxes, [[0, 0, 0, 1, 0.5625, 1]]);
        const data = Block.fromStateId(id, 0).shapes;
        assert.ok(data.length > 1, 'the data has the legs of a later bed');
        const [cauldron, , tub] = blocks.cauldron[0];
        assert.equal(shapes[tub].length, 5);
        assert.deepEqual(mapper.describe(Block.fromStateId(cauldron, 0)).boxes, shapes[tub]);
    });

    it('a piston head as placed reaches 1.25, what `short` is not stored for', () => {
        const { shapes, blocks } = table('1.12.2');
        const row = blocks.piston_head.find(([, props]) => /facing=down/.test(props) && /short=false/.test(props));
        assert.equal(shapes[row[2]][1][4], 1.25);
    });

    it('1.8.8 has no outline method on a block\'s state, and says so with null', () => {
        const { blocks } = table('1.8.8');
        assert.equal(blocks.stone[0][3], null);
        const { blocks: later, shapes } = table('1.12.2');
        assert.deepEqual(shapes[later.stone[0][3]], [[0, 0, 0, 1, 1, 1]]);
    });
});
