'use strict';

// B058: the engine's draws against what every release's own source draws. test/fixtures/random-draws.tsv comes from
// scripts/audit-random-draws.cjs, test/fixtures/random-draw-dispositions.json says what each consumer reaches, and
// docs/random-consumers.md reads both.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { LoadNativePhysics, encodeCheckpoint, decodeCheckpoint } = require('..');
const { parseClasses, drawsOf, callsOf, reach } = require('../scripts/audit-random-draws.cjs');

const native = LoadNativePhysics();
const rows = readFileSync(join(__dirname, 'fixtures/random-draws.tsv'), 'utf8').trim().split(/\r?\n/).map((line) => {
    const [release, kind, consumer, draw, effect, trail] = line.split('\t');
    return { release, kind, consumer, draw, effect, trail };
});
const dispositions = require('./fixtures/random-draw-dispositions.json');
const manifest = require('../generated/release-manifest.json').releases;
const releases = Array.isArray(manifest) ? manifest.map(entry => entry.release) : Object.keys(manifest);

const fire = () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 0, y: 1, z: 0, id: 9, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 16 } }]);
    return world;
};
const burning = (fields = {}) => ({
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8,
    onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true, noGravity: true,
    controlledByLocalInstance: true, clientSide: false, remainingFireTicks: 200, ...fields,
});
const tick = (version, world, state) => native.simulateVersionedTick(version, world, state, {});
const fireDraws = release => rows.some(row => row.release === release && row.consumer.startsWith('BaseFireBlock.') && row.draw.endsWith('nextInt'));

describe('the audit reads what it says', () => {
    it('walks every registered release', () => {
        assert.deepEqual(new Set(rows.map(row => row.release)), new Set(releases));
        assert.equal(releases.length, 50);
    });

    it('finds a draw through a local, a parameter and a helper, in a tree without local names', () => {
        const source = `class A extends B {
   void direct() { float var1 = this.random.nextFloat(); }
   void local() { RandomSource var1 = this.getRandom(); int var2 = var1.nextInt(3); }
   void parameter(RandomSource var1) { var1.nextDouble(); }
   void handed() { Util.getRandom(this.list, this.random); }
   void particle() { this.level.addParticle(null, this.random.nextDouble(), 0, 0); }
   void none() { this.motion = 1; }
   void calls() { this.direct(); none(); this.level.other(); }
}`;
        const [owner] = parseClasses(source);
        const of = name => [...drawsOf(owner.methods.get(name)[0])];
        assert.deepEqual(of('direct'), [['random.nextFloat', false]]);
        assert.deepEqual(of('local').map(([what]) => what), ['random.nextInt']);
        assert.deepEqual(of('parameter').map(([what]) => what), ['random.nextDouble']);
        assert.ok(of('handed').some(([what]) => what === 'hands its random to Util.getRandom'));
        assert.deepEqual(of('particle'), [['random.nextDouble', true]]);
        assert.deepEqual(of('none'), []);
        const calls = callsOf(owner.methods.get('calls')[0]);
        assert.ok(calls.has('direct') && calls.has('none') && !calls.has('other'));
    });

    it('follows a helper through a superclass, and names the way there', () => {
        const parsed = parseClasses(`class LocalPlayer extends Entity { void tick() { super.tick(); this.helper(); } }
class Entity { void tick() { } void helper() { deeper(); } void deeper() { this.random.nextInt(2); } void unused() { this.random.nextFloat(); } }`);
        const found = reach(new Map(parsed.map(owner => [owner.name, owner])), 'LocalPlayer');
        assert.deepEqual(found.map(row => row.key), ['Entity.deeper']);
        assert.deepEqual(found[0].trail, ['LocalPlayer.tick', 'Entity.helper', 'Entity.deeper']);
    });

    it('has every state consumer of every release said for, and no saying left over', () => {
        const rules = dispositions.consumers.map(rule => ({ ...rule, pattern: new RegExp(rule.match) }));
        const used = new Set();
        const unsaid = new Set();
        for (const row of rows.filter(entry => entry.kind === 'consumer' && entry.effect === 'state')) {
            const rule = rules.find(entry => entry.pattern.test(`${row.consumer} ${row.draw}`));
            if (rule) used.add(rule.match);
            else unsaid.add(`${row.consumer} ${row.draw}`);
        }
        assert.deepEqual([...unsaid].sort(), []);
        assert.deepEqual(rules.filter(rule => !used.has(rule.match)).map(rule => rule.match), []);
    });

    it('has two streams of draws in the engine, and every disposition names how a value arrives', () => {
        const streams = dispositions.consumers.filter(rule => rule.engine === 'draws').map(rule => rule.by);
        assert.deepEqual([...new Set(streams)], ['serverFireIncrements', 'MovingEntityInputs.random']);
        for (const rule of dispositions.consumers) {
            assert.ok(['draws', 'input', 'handed', 'none'].includes(rule.engine), rule.match);
            assert.ok(rule.authority && rule.reaches && rule.by, rule.match);
        }
    });

    it('reaches no state from a LocalPlayer\'s own tick, in any release', () => {
        const rules = dispositions.reach.map(rule => ({ ...rule, pattern: new RegExp(rule.match) }));
        const unsaid = new Set();
        for (const row of rows.filter(entry => entry.kind === 'reach' && entry.effect === 'state')) {
            if (!rules.some(rule => rule.pattern.test(`${row.consumer} ${row.draw}`))) unsaid.add(`${row.consumer} ${row.draw}`);
        }
        assert.deepEqual([...unsaid].sort(), []);
        for (const rule of dispositions.reach) assert.equal(rule.moves, false, rule.match);
    });
});

describe('the engine against the audit', () => {
    it('reads a fire draw in exactly the releases whose source draws one, of either value', () => {
        const world = fire();
        const first = releases.filter(fireDraws)[0];
        assert.equal(first, '1.21.2', 'the first release with the draw, by its own tree');
        for (const release of releases) {
            if (fireDraws(release)) {
                const [one, two] = [1, 2].map(value => tick(release, world, burning({ serverFireIncrements: [value, 2] })));
                assert.deepEqual(one.serverFireIncrements, [2], release);
                assert.deepEqual(two.serverFireIncrements, [2], release);
                assert.equal(two.remainingFireTicks - one.remainingFireTicks, 1, `${release}: the draw is the increment`);
            } else {
                assert.deepEqual(tick(release, world, burning({ serverFireIncrements: [2, 1] })).serverFireIncrements, [2, 1], `${release} has no draw`);
            }
        }
    });

    it('leaves a LocalPlayer\'s draws unread in every release', () => {
        const world = fire();
        for (const release of releases) {
            const state = tick(release, world, burning({ clientSide: true, serverFireIncrements: [2, 1] }));
            assert.deepEqual(state.serverFireIncrements, [2, 1], release);
        }
    });

    it('gives one result through the object, batch and packed surfaces, with the same unread draws', () => {
        const world = fire();
        const draws = [1, 2, 2, 1];
        let single = burning({ serverFireIncrements: draws });
        for (let at = 0; at < 3; at++) single = tick('26.3', world, single);
        const [batched] = native.simulateVersionedTicks('26.3', world, [burning({ serverFireIncrements: draws })], 3, {});
        assert.deepEqual(batched, single);
        assert.deepEqual(single.serverFireIncrements, [1]);
        const again = native.simulateVersionedTickOutcome('26.3', world, burning({ serverFireIncrements: draws }), {}).state;
        assert.deepEqual(again.serverFireIncrements, [2, 2, 1]);
        const kept = new native.NativePlayer('26.3', again, {});
        assert.deepEqual(kept.state().serverFireIncrements, [2, 2, 1], 'the player keeps the unread draws');
    });

    it('takes no draw for what is cosmetic: world edits elsewhere and client ticks between leave the stream where it was', () => {
        const draws = [2, 1, 2, 2, 1, 1];
        const run = (between) => {
            const world = fire();
            const seen = [];
            let state = burning({ serverFireIncrements: draws });
            for (let at = 0; at < 5; at++) {
                state = tick('26.3', world, state);
                seen.push([state.remainingFireTicks, state.serverFireIncrements]);
                between(world, state, at);
            }
            return seen;
        };
        const plain = run(() => {});
        // an unrelated block, as a particle's or a sound's packet never reaches the engine at all
        const edited = run((world, state, at) => {
            world.applyBlockUpdates([{ x: 40 + at, y: 90, z: 40, id: 2, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 } }]);
        });
        assert.deepEqual(edited, plain);
        // a client's prediction of the same state reads none
        const predicted = run((world, state) => {
            assert.deepEqual(tick('26.3', world, { ...state, clientSide: true }).serverFireIncrements, state.serverFireIncrements);
        });
        assert.deepEqual(predicted, plain);
    });

    it('carries the unread draws through a clone and a restore, and refuses a restore into another release', () => {
        const world = fire();
        const state = tick('26.3', world, burning({ serverFireIncrements: [2, 1, 2] }));
        assert.deepEqual({ ...state }.serverFireIncrements, [1, 2], 'a copy has them');
        const bytes = encodeCheckpoint(native, { version: '26.3', state, world });
        const restored = decodeCheckpoint(native, bytes, '26.3');
        assert.deepEqual(restored.state.serverFireIncrements, [1, 2]);
        assert.deepEqual(tick('26.3', restored.world, restored.state), tick('26.3', world, state));
        assert.throws(() => decodeCheckpoint(native, bytes, '1.21.1'));
    });

    it('says when a moving entity ran short of its draws, and gives the same tick for the same draws', () => {
        const world = new native.NativeWorld();
        world.applyBlockUpdates([{ x: 0, y: 5, z: 0, id: 1, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 } }]);
        const item = { kind: 'item', position: { x: 0.5, y: 5.5, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, onGround: false, inWater: false, age: 3, pickupDelay: 10, authority: 'server' };
        const stuck = random => native.simulateVersionedMovingEntityTick('1.20.4', world, item, { random });
        assert.equal(stuck([]).randomMissing, true);
        const drawn = stuck([0.5]);
        assert.equal(drawn.randomMissing, false);
        assert.equal(drawn.randomUsed, 1);
        assert.deepEqual(stuck([0.5]), drawn);
        assert.notDeepEqual(stuck([0.25]).state.velocity, drawn.state.velocity);
    });

    it('refuses a missing or an impossible draw, and takes an old release\'s stream as none', () => {
        const world = fire();
        assert.throws(() => tick('26.3', world, burning({ serverFireIncrements: [] })), /ran out/);
        assert.throws(() => tick('26.3', world, burning({})), /needs its fire draw/);
        assert.throws(() => tick('26.3', world, burning({ serverFireIncrements: [3] })), /1 or 2/);
        assert.throws(() => tick('26.3', world, burning({ serverFireIncrements: [0] })), /1 or 2/);
        assert.doesNotThrow(() => tick('1.21.1', world, burning({})));
    });
});
