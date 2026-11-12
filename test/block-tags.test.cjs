'use strict';

// B034, B077, B164: each release's block tags by the pack that holds them, what became of the audit's differences, and
// the handwritten sets the adapter decides by, against the memberships the release ships.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { parseTagFile, mergePacks, resolveTags, membershipHash } = require('../scripts/generate-block-tags.cjs');
const { judge } = require('../scripts/classify-tag-audit.cjs');

const root = path.resolve(__dirname, '..');
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const tags = generated('block-tags.json').releases;
const audit = generated('tag-audit-dispositions.json');
const modern = Object.entries(tags).filter(([, release]) => release.tags);

/** tag id -> { blocks, includes } of a release's generated Rust table. */
function moduleTags(version) {
    const text = fs.readFileSync(path.join(root, 'native', 'data', 'releases', `v${version.replaceAll('.', '_')}.rs`), 'utf8');
    const lists = new Map();
    for (const match of text.matchAll(/pub static (\w+): &\[&str\] = &\[(.*?)\];/g)) lists.set(match[1], [...match[2].matchAll(/"([^"]*)"/g)].map(name => name[1]));
    const found = new Map();
    for (const match of text.matchAll(/GeneratedBlockTag \{ id: "([^"]+)".*?blocks: (\w+), includes: (\w+)/g)) {
        found.set(match[1], { blocks: lists.get(match[2]) ?? [], includes: lists.get(match[3]) ?? [] });
    }
    return found;
}

const tagOf = (blocks, includes = []) => ({ blocks, includes, replace: false });

describe('the tag files', () => {
    it('reads object entries, namespaces, repeats and nested tags', () => {
        const file = parseTagFile(JSON.stringify({ values: ['minecraft:b', 'minecraft:a', 'minecraft:a', { id: 'minecraft:c', required: false }, '#minecraft:d'] }), 'x');
        assert.deepEqual(file, { blocks: ['a', 'b', 'c'], includes: ['d'], replace: false });
        assert.equal(parseTagFile(JSON.stringify({ replace: true, values: [] }), 'x').replace, true);
    });

    it('refuses a file that is not a tag', () => {
        assert.throws(() => parseTagFile('{}', 'x.json'), /x\.json has no values/);
        assert.throws(() => parseTagFile(JSON.stringify({ values: [1] }), 'x.json'), /no name/);
    });

    it('adds a pack to a tag, and replaces it only when the file says so', () => {
        const base = new Map([['t', tagOf(['a'])]]);
        assert.deepEqual(mergePacks([base, new Map([['t', tagOf(['b'])]])]).get('t').blocks, ['a', 'b']);
        assert.deepEqual(mergePacks([base, new Map([['t', { ...tagOf(['b']), replace: true }]])]).get('t').blocks, ['b']);
    });

    it('resolves nested tags', () => {
        const merged = new Map([['outer', tagOf(['a'], ['inner'])], ['inner', tagOf(['b'])]]);
        assert.deepEqual([...resolveTags(merged, null, 'x').get('outer')].sort(), ['a', 'b']);
    });

    it('fails on a cycle, on a reference to no tag and on a block the release lacks', () => {
        assert.throws(() => resolveTags(new Map([['a', tagOf([], ['b'])], ['b', tagOf([], ['a'])]]), null, 'x'), /tag cycle a -> b -> a/);
        assert.throws(() => resolveTags(new Map([['a', tagOf([], ['gone'])]]), null, 'x'), /a includes #gone, which no file defines/);
        assert.throws(() => resolveTags(new Map([['a', tagOf(['stone'])]]), new Set(['dirt']), 'x'), /#a holds stone/);
    });
});

describe('the generated tags', () => {
    it('has tags from 1.13, and says so of the releases before', () => {
        assert.deepEqual(Object.entries(tags).filter(([, release]) => !release.tags).map(([version]) => version), ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2']);
        assert.equal(modern.length, 45);
    });

    it('binds every Rust table to the default pack, by hash and with nothing left over', () => {
        for (const [version, release] of modern) {
            const table = moduleTags(version);
            for (const [id, entry] of Object.entries(release.default)) {
                assert.ok(table.has(id), `${version} lacks #${id}`);
                assert.equal(membershipHash(table.get(id)), entry.membership, `${version} #${id}`);
            }
            for (const [id, members] of table) {
                if (release.default[id]) continue;
                assert.ok(release.declaredEmpty[id], `${version} #${id} comes from no file`);
                assert.deepEqual(members, { blocks: [], includes: [] }, `${version} #${id} is empty by the data`);
            }
        }
    });

    it('keeps an experimental pack out of the default tables', () => {
        // bamboo blocks are the update_1_20 pack's on 1.19.4: declared in BlockTags, with no default file
        assert.deepEqual(tags['1.19.4'].declaredEmpty.bamboo_blocks, ['update_1_20']);
        assert.deepEqual(moduleTags('1.19.4').get('bamboo_blocks'), { blocks: [], includes: [] });
        assert.deepEqual(tags['1.19.4'].experimental.update_1_20.bamboo_blocks.blocks, ['bamboo_block', 'stripped_bamboo_block']);
        assert.ok(!moduleTags('1.19.4').get('planks').blocks.includes('cherry_planks'));
        assert.ok(tags['1.19.4'].experimental.update_1_20.planks.blocks.includes('cherry_planks'));
        // the pale oak is winter_drop's until 1.21.4 makes it default
        assert.deepEqual(tags['1.21.3'].declaredEmpty.pale_oak_logs, ['winter_drop']);
        assert.equal(tags['1.21.4'].declaredEmpty.pale_oak_logs, undefined);
    });

    it('tells a tag no pack fills from one a pack does', () => {
        assert.deepEqual(tags['26.3'].declaredEmpty.consumable_does_not_teleport_to, []);
        assert.equal(tags['26.3'].default.consumable_does_not_teleport_to, undefined);
    });

    it('has every membership at the hash of the file it came from', () => {
        for (const [, release] of modern) {
            for (const entry of Object.values(release.default)) assert.match(entry.file, /^[0-9a-f]{64}$/);
            assert.match(release.client, /^[0-9a-f]{40}$/);
        }
    });
});

/**
 * Every tag a movement rule of the adapter or the engine asks, the release its tag starts, and what decides before it:
 * the rule is a predicate there, so a modern tag's members must not be carried back. `absent-tag` is a release with no
 * such tag at all, which is not the same as an empty one.
 */
const CONSUMERS = [
    { tag: 'climbable', from: '1.16', consumer: 'blocks.ts effectOf, player.ts onClimbable', before: 'ladder, vine, scaffolding by name' },
    { tag: 'fall_damage_resetting', from: '1.18.2', consumer: 'blocks.ts isFallDamageResetting', before: 'absent-tag: the clip starts there' },
    { tag: 'can_glide_through', from: '1.21.11', consumer: 'blocks.ts GLIDE_THROUGH', before: 'absent-tag: a glider climbs' },
    { tag: 'blocks_fluid_flow', from: '26.3', consumer: 'blocks.ts flowBlocker', before: 'blocksMotion, flow-blockers.json' },
    { tag: 'blocks_motion_in_heightmap', from: '26.3', consumer: 'blocks.ts motionBlocker', before: 'blocksMotion' },
    { tag: 'suppresses_bounce', from: '26.2', consumer: 'blocks.ts suppressesBounce', before: 'absent-tag: nothing suppresses' },
    { tag: 'fences', from: '1.14.4', consumer: 'blocks.ts isFenceLike, simulation.rs getLandingPos', before: 'by name' },
    { tag: 'walls', from: '1.14.4', consumer: 'blocks.ts isWallOrGate, simulation.rs getOnPos', before: 'by name' },
    { tag: 'leaves', from: '1.13.2', consumer: 'blocks.ts supportShape', before: 'by name' },
    { tag: 'rails', from: '1.13.2', consumer: 'blocks.ts railFields', before: 'by name' },
];

describe('the tags movement asks', () => {
    const versions = Object.keys(tags);
    const firstWith = tag => versions.find(version => tags[version].default?.[tag]);

    for (const { tag, from, before } of CONSUMERS) {
        it(`#${tag} starts at ${from} and stays, never empty`, () => {
            assert.equal(firstWith(tag), from);
            for (const version of versions.slice(versions.indexOf(from))) {
                assert.ok(tags[version].default[tag], `${tag} is gone from ${version}`);
                assert.ok(tags[version].default[tag].resolved > 0, `${tag} is empty on ${version}`);
            }
            assert.ok(before.length > 0 && !before.includes('undefined'));
        });
    }

    it('lists every tag the adapter\'s source asks', () => {
        const source = fs.readFileSync(path.join(root, 'src', 'mineflayer', 'blocks.ts'), 'utf8');
        const asked = [...source.matchAll(/(?:hasTag\(\w+, |, tag = |hasTag, )'([a-z_]+)'/g)].map(match => match[1]);
        assert.ok(asked.length >= 3, asked.join());
        for (const tag of asked) assert.ok(CONSUMERS.some(consumer => consumer.tag === tag), `#${tag} has no entry here`);
    });

    it('keeps a modern membership out of the releases before its tag', () => {
        assert.equal(tags['1.20.4'].default.blocks_fluid_flow, undefined);
        assert.equal(tags['1.18.1'].default.fall_damage_resetting, undefined);
        assert.equal(tags['1.15.2'].default.climbable, undefined);
        assert.equal(tags['1.13.2'].default.fences, undefined);
        assert.equal(tags['1.12.2'].tags, false);
    });
});

describe('the audit\'s 808 differences', () => {
    it('gives every one a disposition and a source, and leaves none unresolved', () => {
        assert.equal(audit.rows.length, 808);
        assert.deepEqual(audit.totals, { 'wrong-identifier': 18, configuration: 148, 'missing-data': 642 });
        for (const row of audit.rows) assert.match(row.source, /^client [0-9a-f]{40}/, `${row.version} #${row.tag}`);
    });

    it('judges a name by the jar: default pack, experimental pack, not a block, else unresolved', () => {
        const jar = { base: new Map([['t', new Set(['in_default'])]]), packs: new Map([['update_1_20', new Map([['t', new Set(['in_pack'])]])]]) };
        const known = new Set(['in_default', 'in_pack', 'known_elsewhere']);
        assert.equal(judge('in_default', 't', jar, known), 'missing-data');
        assert.equal(judge('in_pack', 't', jar, known), 'configuration:update_1_20');
        assert.equal(judge('misnamed', 't', jar, known), 'wrong-identifier');
        assert.equal(judge('known_elsewhere', 't', jar, known), 'unresolved');
    });

    it('holds no difference from the jar any more', () => {
        const tables = new Map();
        for (const row of audit.rows) {
            const table = tables.get(row.version) ?? tables.set(row.version, moduleTags(row.version)).get(row.version);
            const entry = tags[row.version].default[row.tag];
            if (entry) assert.equal(membershipHash(table.get(row.tag)), entry.membership, `${row.version} #${row.tag}`);
            else assert.deepEqual(table.get(row.tag), { blocks: [], includes: [] }, `${row.version} #${row.tag}`);
        }
    });
});

describe('the tag audit the dispositions classify', () => {
    const { auditProblem, AUDIT } = require('../scripts/classify-tag-audit.cjs');
    const record = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'generated', 'tag-audit-dispositions.json'), 'utf8')).audit;

    it('is the pinned audit', () => {
        assert.deepEqual(record, AUDIT);
    });

    it('refuses an audit of other bytes before classifying it, unless that digest is accepted', () => {
        const other = Buffer.from('[]');
        assert.match(auditProblem(other), /^the audit is [0-9a-f]{64}, pinned 35811f16[0-9a-f]+: review it/);
        assert.equal(auditProblem(other, require('node:crypto').createHash('sha256').update(other).digest('hex')), null);
    });
});
