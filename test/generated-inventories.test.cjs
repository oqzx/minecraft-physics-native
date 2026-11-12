'use strict';

// B043: which vanilla declarations each collision primitive ports. B046: every decimal the engine and the adapter move
// things by, looked up in each release's sources. Both are reviewed tables held to the source: a new primitive or a
// constant nobody can place fails here.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { PRIMITIVES, nativePrimitives, declaredMethods, decisionsOf } = require('../scripts/generate-primitive-map.cjs');
const { keysOf, trivial, moved, cited, usesOf } = require('../scripts/generate-movement-constants.cjs');
const { registeredReleases, compareReleases } = require('../scripts/registered-releases.cjs');
const { scanRust, citationsOf } = require('../scripts/movement-sites.cjs');
const { moduleSites, applying, typing } = require('../scripts/generate-movement-sites.cjs');

const root = path.resolve(__dirname, '..');
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const map = generated('primitive-map.json');
const inventory = generated('movement-constants.json');
const constants = Object.values(inventory.constants);

describe('the primitive map (B043)', () => {
    it('has an entry for every public function of the collision modules', () => {
        assert.deepEqual(nativePrimitives().filter(name => !PRIMITIVES[name]), []);
        assert.ok(nativePrimitives().length >= 29);
    });

    it('has no entry for a function that is gone', () => {
        assert.deepEqual(Object.keys(PRIMITIVES).filter(name => !nativePrimitives().includes(name)), []);
    });

    it('says why a function ports nothing, when it does not', () => {
        for (const [name, entry] of Object.entries(PRIMITIVES)) {
            if (!Array.isArray(entry)) assert.ok(entry.own.length > 10, name);
        }
    });

    it('finds each declaration it names in the source of some release', () => {
        assert.deepEqual(Object.entries(map.declarations).filter(([, row]) => row.eras.every(era => era.sha256 === null)).map(([name]) => name), []);
        for (const entry of Object.values(PRIMITIVES)) {
            if (Array.isArray(entry)) for (const reference of entry) assert.ok(map.declarations[reference], reference);
        }
    });

    it('covers the releases of Mojang\'s names and names the others', () => {
        assert.equal(map.mapped.length, 44);
        assert.deepEqual(map.legacy.releases, ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']);
        assert.deepEqual(map.unavailable, []);
    });

    it('maps the releases before 1.14 by their own classes, each name found in that release', () => {
        const primitives = new Set(Object.keys(PRIMITIVES));
        for (const release of map.legacy.releases) {
            const table = map.legacy.primitives[release];
            assert.ok(Object.keys(table).length >= 5, release);
            for (const [primitive, references] of Object.entries(table)) {
                assert.ok(primitives.has(primitive), primitive);
                for (const reference of references) assert.ok(map.legacy.declarations[reference], `${release} ${reference}`);
            }
        }
        // the voxel shapes came with 1.13, and with them the walk over a world
        assert.ok(map.legacy.primitives['1.13.2']['voxel_shape::union']);
        assert.equal(map.legacy.primitives['1.12.2']['voxel_shape::union'], undefined);
        assert.deepEqual(map.legacy.primitives['1.12.2']['collision::from_world'], ['Level#doesBoxCollide', 'Level#method_13689']);
    });

    const eraOf = (rows, release) => [...rows].reverse().find(({ from }) => compareReleases(from, release) <= 0);

    it('keeps each release\'s walk of the blocks, decision by decision', () => {
        const legacyWalk = release => (compareReleases(release, '1.13') >= 0 ? 'LevelReader#method_16378' : 'Level#doesBoxCollide');
        for (const release of map.legacy.releases) assert.ok(eraOf(map.legacy.decisions[legacyWalk(release)], release).decisions[0].length > 5, release);
        for (const release of map.mapped) {
            const walk = compareReleases(release, '1.18') >= 0 ? 'BlockCollisions#computeNext' : compareReleases(release, '1.16') >= 0 ? 'CollisionSpliterator#collisionCheck' : compareReleases(release, '1.15') >= 0 ? 'CollisionGetter#getBlockCollisions' : 'LevelReader#getCollisions';
            assert.ok(eraOf(map.decisions[walk], release)?.decisions[0].length > 0, `${release} ${walk}`);
        }
        // what the walk filters by, in order: the corner cells, and then the shape compared with Shapes.block() by identity
        const newest = map.decisions['BlockCollisions#computeNext'].at(-1).decisions[0];
        const corner = newest.findIndex(text => text.includes('== 3'));
        const identity = newest.findIndex(text => text.includes('== Shapes.block()'));
        assert.ok(corner >= 0 && identity > corner, newest.join(' | '));
    });

    it('says where a release simplifies: Shapes.join alone, none before the voxel shapes', () => {
        assert.deepEqual(map.legacy.simplification.map(({ from, sites }) => [from, sites]), [['1.8.8', []], ['1.13.2', ['Shapes#join']]]);
        assert.deepEqual(map.simplification.map(({ sites }) => sites), [['Shapes#join']]);
        assert.deepEqual(map.unread, { '1.18.1': ['Shapes'] });
    });

    it('reads decisions in order, with the names of locals erased', () => {
        const body = 'void f(int a) {\n if (a == 3) {\n continue;\n }\n VoxelShape s = Shapes.block().optimize();\n return s;\n}';
        assert.deepEqual(decisionsOf(body), ['if (_ == 3)', 'continue', 'Shapes.block', '.optimize()', 'return _;']);
    });

    it('shows where a declaration came and changed: BlockCollisions is 1.18\'s', () => {
        const blocks = map.declarations['BlockCollisions#computeNext'].eras;
        assert.equal(blocks[0].sha256, null);
        assert.equal(blocks[0].from, '1.14.4');
        assert.equal(blocks.find(era => era.sha256).from, '1.18');
        const merged = map.declarations['Entity#collideWithShapes'].eras;
        assert.equal(merged.find(era => era.sha256).from, '1.18');
    });

    it('follows helpers through the shape classes, and not into Entity', () => {
        const { helpers } = map.declarations['VoxelShape#optimize'];
        assert.ok(helpers.includes('Shapes#joinUnoptimized'), helpers.join());
        assert.ok(helpers.includes('VoxelShape#forAllBoxes'), helpers.join());
        for (const row of Object.values(map.declarations)) assert.ok(!row.helpers.some(helper => helper.startsWith('Entity#')));
    });

    it('reads a class\'s declared methods, not its calls', () => {
        const source = 'class A {\n   public double f(int a) {\n      return this.g(a);\n   }\n\n   private static void h() {\n   }\n}';
        assert.deepEqual([...declaredMethods(source)].sort(), ['f', 'h']);
    });
});

describe('the movement constants (B046)', () => {
    it('reads every release, and every module but the bindings and the tests', () => {
        assert.equal(inventory.releases, 50);
        assert.deepEqual(inventory.unavailable, []);
        assert.equal(moved('simulation.rs'), true);
        assert.equal(moved('trig.rs'), true);
        assert.equal(moved('napi_api.rs'), false);
        assert.equal(moved('oracle_tests.rs'), false);
        assert.equal(moved('lib.rs'), false);
    });

    it('accounts for each one, and none is left unplaced', () => {
        assert.equal(inventory.totals.unresolved ?? 0, 0);
        for (const row of constants) {
            assert.ok(['source', 'jdk', 'cited'].includes(row.status), String(row.value));
            assert.ok(row.files.length > 0, String(row.value));
        }
        assert.ok(constants.length >= 130);
    });

    it('says the release range and the Java type of a source constant', () => {
        for (const row of constants.filter(({ status }) => status === 'source')) {
            assert.ok(row.first && row.last && row.releases > 0, String(row.value));
            assert.ok(row.types.every(type => type === 'float' || type === 'double'), String(row.value));
            assert.ok(compareReleases(row.first, row.last) <= 0, String(row.value));
        }
    });

    it('holds each number to its release: a constant that came late or went is not called universal', () => {
        const held = value => constants.find(row => row.value === value);
        assert.equal(held(0.0325).first, '1.21.11');
        assert.equal(held(0.0325).last, '26.3');
        assert.equal(compareReleases(held(0.06153846016296973).last, '1.20.4'), 0);
    });

    it('keeps the JDK\'s numbers apart: trig.rs holds them as bits, so none reads as a source of Minecraft\'s', () => {
        assert.deepEqual(constants.filter(({ status }) => status === 'jdk'), []);
        for (const row of constants) assert.ok(!row.files.includes('native/trig.rs'), String(row.value));
    });

    it('holds legal zeros and thresholds of the movement tick', () => {
        for (const value of [0.91, 0.98, 0.08, 0.02, 0.003, 1.0E-7, 0.21600002, 0.6]) {
            const row = constants.find(entry => keysOf(entry.value).includes(String(value)) || keysOf(value).includes(String(entry.value)));
            assert.ok(row, String(value));
            assert.equal(row.status, 'source', String(value));
        }
    });

    it('leaves out what is no constant of anything, and reads a literal as it is written', () => {
        assert.equal(trivial(0.5), true);
        assert.equal(trivial(2), true);
        assert.equal(trivial(0.25), true);
        assert.equal(trivial(0.3), false);
        assert.deepEqual(keysOf(0.1), ['0.1', String(Math.fround(0.1))]);
    });

    it('does not read a version, a test or a string as a number', () => {
        const directory = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'mc-constants-'));
        fs.writeFileSync(path.join(directory, 'a.rs'), [
            '// 1.11.2 FireworkRocketEntity.java:113',
            'fn f() { let a = 0.98_f32; let v = "1.21.5"; g(1.0e-7); h(2.0); }',
            '#[cfg(test)]',
            'fn helper() {}',
            'fn g() { let b = 0.31; }',
            '#[cfg(test)]',
            'mod tests { fn t() { assert_eq!(0.77, 0.77); } }',
        ].join('\n'));
        const uses = usesOf(directory, ['a.rs'], true);
        assert.deepEqual([...uses.keys()].sort(), ['0.31', '0.98', '1e-7']);
        assert.equal(uses.get('0.98').cited, true);
    });

    it('calls a line cited by the source named above it', () => {
        assert.equal(cited(['// 26.3 Entity.java:12', 'let a = 0.3;'], 1), true);
        assert.equal(cited(['let b = 1;', 'let a = 0.3;'], 1), false);
    });
});

describe('the movement sites (B046)', () => {
    const inventoried = generated('movement-sites.json');
    const { sites } = inventoried;

    it('take their types from a record of the native code as it is now', () => {
        // (a type rustc gave a literal in code that has changed since is stale, whatever the function and text still say)
        const { codeHash } = require('../scripts/movement-literal-types.cjs');
        const record = generated('movement-literal-types.json');
        assert.match(record.rustc ?? '', /^rustc \d+\.\d+\.\d+/);
        assert.equal(record.source, codeHash(path.join(root, 'native', 'src')), 'run node scripts/movement-literal-types.cjs --work DIR');
    });

    it('reads all fifty releases from their own trees', () => {
        assert.equal(inventoried.trees, 50);
        assert.deepEqual(inventoried.unavailable, []);
        const trees = Object.values(inventoried.byRelease).map(({ tree }) => tree);
        assert.equal(trees.filter(tree => tree === 'legacy').length, 6);
        assert.equal(trees.filter(tree => tree === 'modern').length, 44);
    });

    it('has a row for every numeric literal of the code, whole ones too, so a new one fails until it is generated', () => {
        const rows = new Set(sites.map(site => `${site.file}#${site.fn}|${site.literal}`));
        const missing = [];
        for (const { name, scan } of moduleSites()) {
            if (name.startsWith('native/versioned/')) continue;
            for (const site of scan.sites) {
                if (!rows.has(`${name}#${site.fn}|${site.text}`)) missing.push(`${name}#${site.fn} ${site.text}`);
            }
        }
        assert.deepEqual(missing, [], 'run node scripts/generate-movement-sites.cjs');
    });

    it('knows the compiler\'s type of each literal', () => {
        assert.deepEqual(sites.filter(({ type }) => !['f32', 'f64', 'number', 'int'].includes(type)).map(({ id }) => id), []);
    });

    it('calls extracted only what a citation holds at the release and the type', () => {
        for (const site of sites.filter(({ status }) => status === 'extracted')) {
            assert.ok(['line', 'method', 'class', 'only'].includes(site.proof), site.id);
            assert.ok(['same', 'widened'].includes(site.typing), site.id);
            assert.match(site.java.at, /\.java:\d+$/, site.id);
            assert.ok(site.java.text.length > 0, site.id);
        }
    });

    it('says why of each review, and keeps no review of nothing', () => {
        for (const site of sites.filter(({ status }) => status === 'reviewed')) assert.ok(site.review.length > 20, site.id);
        assert.deepEqual(inventoried.staleReviews, []);
        assert.deepEqual(inventoried.staleSiteReviews, []);
        assert.deepEqual(inventoried.badSiteReviews, [], 'a reviewed line must hold the number at the site\'s type');
    });

    it('keeps the unproved in view: a new site no one has placed fails', () => {
        const open = sites.filter(({ status }) => status === 'open').map(({ id }) => id);
        assert.ok(open.length <= 3, open.join('\n'));
    });

    it('keeps a legal zero as a value, apart from the constants', () => {
        const zeros = sites.filter(({ value }) => value === 0);
        assert.ok(zeros.length > 50);
        for (const zero of zeros) assert.equal(zero.status, 'structural', zero.id);
    });

    it('counts each release by what backs it', () => {
        for (const [release, row] of Object.entries(inventoried.byRelease)) {
            assert.ok(row.extracted > 400, release);
            assert.ok(row.reviewed > 50, release);
            assert.ok(row.open <= 3, release);
            assert.ok(row.notInTree < 120, release);
        }
    });

    it('holds each profile\'s value to the classes of its own release', () => {
        // (vanilla has no such number yet in these, and the code that reads the field does not run: swimming comes in
        // 1.13 and its downward steering's 0.085 in 1.13 too, the 1.9 0.06 with the look-down climb; lava pushes from 1.16)
        const inert = { ultra_warm_lava_flow_scale: ['1.8.8..1.15.2'], swimming_vertical_steering: ['1.8.8'], swimming_downward_steering: ['1.8.8..1.12.2'] };
        for (const [field, rows] of Object.entries(inventoried.profiles)) {
            assert.ok(rows.length > 0, field);
            for (const row of rows) assert.deepEqual(row.missing ?? [], inert[field] ?? [], field);
        }
    });

    it('justifies every static number that stands in for a runtime value', () => {
        const unjustified = inventoried.defaults.filter(({ neutral, why }) => !neutral && !why).map(({ id }) => id);
        assert.deepEqual(unjustified, []);
        assert.ok(inventoried.defaults.length > 100);
    });

    it('lists the release gates and the tables written by hand', () => {
        assert.ok(inventoried.gates.length > 200);
        assert.ok(inventoried.gates.some(({ fn, gates }) => fn === 'native/projectile.rs#pearl_lost' && gates.includes('1.21.9')));
        assert.ok(inventoried.tables.includes('native/entity_travel.rs#INVENTORY'));
    });

    it('reads a gate around a literal, an else and a named bool among them', () => {
        const { sites: read } = scanRust([
            'fn a(release: V) {',
            '    let modern = release.at_least(1, 21, 5);',
            '    let x = if modern { 0.1 } else if !release.at_least(1, 14, 0) { 0.2 } else { 0.3_f32 };',
            '    if release.at_least(1, 16, 0) && !release.at_least(1, 17, 0) { g(0.4); }',
            '    h(java_float(0.6));',
            '}',
        ].join('\n'));
        const gates = text => read.find(site => site.text === text).gates;
        assert.deepEqual(gates('0.1'), [{ at: [1, 21, 5], from: true }]);
        assert.deepEqual(gates('0.2'), [{ at: [1, 21, 5], from: false }, { at: [1, 14, 0], from: false }]);
        assert.deepEqual(gates('0.3_f32'), [{ at: [1, 21, 5], from: false }, { at: [1, 14, 0], from: true }]);
        assert.deepEqual(gates('0.4'), [{ at: [1, 16, 0], from: true }, { at: [1, 17, 0], from: false }]);
        assert.equal(read.find(site => site.text === '0.6').narrowed, true);
        assert.deepEqual(applying(gates('0.4'), ['1.15.2', '1.16', '1.16.5', '1.17'], null), ['1.16', '1.16.5']);
    });

    it('reads a citation with the release before it', () => {
        assert.deepEqual(citationsOf('1.21.3 TntEntity.java:92, 1.21 :143-150').filter(({ from }) => from !== null), [
            { release: '1.21.3', file: 'TntEntity', from: 92, to: 92 },
            { release: '1.21', file: 'TntEntity', from: 143, to: 150 },
        ]);
    });

    it('tells a float widened from one written as a double', () => {
        assert.equal(typing('f32', 0.98, 'float'), 'same');
        assert.equal(typing('f64', 0.98, 'double'), 'same');
        assert.equal(typing('f64', Math.fround(0.98), 'float'), 'widened');
        assert.equal(typing('f64', 0.98, 'float'), 'differs');
        assert.equal(typing('f32', 0.98, 'double'), 'differs');
    });
});

describe('releases', () => {
    it('are the registered ones', () => {
        assert.equal(registeredReleases().length, 50);
    });
});
