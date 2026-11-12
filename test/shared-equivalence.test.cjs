'use strict';

// B280: the equivalence claims of the ungated shared code and the profile rules (scripts/shared-equivalence.cjs) cover
// exactly what native/src holds now. Making the claims needs the compare trees; holding the record to the code does not,
// so a function or a rule added, removed or renamed without regenerating it fails here. Every review of
// audit/shared-equivalence-reviews/ must account for a change the record has, by that change's hash, so a review lapses
// when a text it read changes; where the compare trees are on this machine, each reviewed change's two texts are read
// again and hashed, so a tree that changes under a review fails here even before the record is made again.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const {
    ungatedFunctions, profileRules, readReviews, changeHash, overloads, mechanicalClass, helperProblems, rustTests, REASONS, scopedLocals,
} = require('../scripts/shared-equivalence.cjs');
const { canonicalReleases } = require('../scripts/release-inventory.cjs');
const { readReach } = require('../scripts/reach-probe.cjs');
const record = require('../generated/shared-equivalence.json');

const key = ({ file, function: name }) => `${file}:${name}`;
const STATUSES = ['source-unchanged', 'mechanical', 'reviewed', 'review', 'unresolved', 'jdk', 'helper', 'glue', 'uncited'];

describe('the shared equivalence record', () => {
    it('lists every ungated function of native/src, once, and no other', () => {
        const now = [...new Set(ungatedFunctions().map(key))].sort();
        const listed = record.functions.map(key);
        assert.equal(new Set(listed).size, listed.length);
        assert.deepEqual([...listed].sort(), now);
    });

    it('holds every profile rule at the values the profiles give now, release by release', () => {
        const values = profileRules();
        const releases = canonicalReleases();
        assert.equal(record.releases, releases.length);
        const fields = [...new Set(Object.values(values).flatMap(Object.keys))].sort();
        assert.deepEqual(record.rules.map(rule => rule.rule), fields);
        for (const rule of record.rules) {
            const now = releases.map(release => values[release][rule.rule]);
            assert.deepEqual(rule.values, [...new Set(now)], rule.rule);
            const boundaries = releases.filter((release, index) => index > 0 && now[index] !== now[index - 1]);
            assert.deepEqual(rule.boundaries.map(entry => entry.boundary), boundaries, rule.rule);
        }
    });

    it('gives each claim a status that follows from what it found', () => {
        for (const entry of record.functions) {
            assert.ok(STATUSES.includes(entry.status), key(entry));
            const changes = entry.changes ?? [];
            if (entry.status === 'source-unchanged') assert.ok(entry.methods.length > 0 && changes.length === 0, key(entry));
            if (['mechanical', 'reviewed', 'review'].includes(entry.status)) assert.ok(changes.length > 0, key(entry));
            if (['mechanical', 'reviewed'].includes(entry.status)) assert.ok(changes.every(change => change.by) && !entry.open, key(entry));
            if (entry.status === 'mechanical') assert.ok(changes.every(change => change.by === change.mechanical || change.by.startsWith('unreached: ')), key(entry));
            if (entry.status === 'review') {
                const open = [...new Set(changes.filter(change => !change.by).map(change => change.boundary))];
                assert.deepEqual(entry.open, open, key(entry));
            }
            if (['uncited', 'glue', 'helper'].includes(entry.status)) assert.ok(!entry.methods, key(entry));
            if (entry.status === 'glue') assert.match(entry.file, /^napi_/, key(entry));
        }
        for (const rule of record.rules) if (rule.status === 'checked') assert.ok(rule.by, rule.rule);
        const totals = {};
        for (const entry of record.functions) totals[entry.status] = (totals[entry.status] ?? 0) + 1;
        for (const [status, count] of Object.entries(totals)) assert.equal(record.totals[status], count, status);
    });

    it('has no review that accounts for nothing, and each review names a change by the hash the record has', () => {
        assert.deepEqual(record.staleReviews, []);
        const reviews = readReviews();
        const changes = record.functions.flatMap(entry => (entry.changes ?? []).map(change => ({ ...change, entry })));
        for (const review of reviews.methods) {
            assert.ok(review.note && review.note.length > 20, `${review.from}: ${review.method}@${review.boundary} has no note`);
            const hit = changes.filter(change => change.method === review.method && change.boundary === review.boundary);
            assert.ok(hit.some(change => change.hash === review.hash), `${review.from}: ${review.method}@${review.boundary} names ${review.hash}, the record ${hit.map(change => change.hash)}`);
        }
        for (const review of reviews.functions) {
            assert.ok(typeof review.file === 'string' && typeof review.function === 'string' && typeof review.boundary === 'string', `${review.from}: ${JSON.stringify(review).slice(0, 120)}`);
            assert.ok(REASONS[review.reason], `${review.from}: ${review.file}:${review.function}@${review.boundary}: no reason ${review.reason}`);
            assert.ok(review.note && review.note.length > 20, `${review.from}: ${review.file}:${review.function}@${review.boundary} has no note`);
            const entry = record.functions.find(item => item.file === review.file && item.function === review.function);
            assert.ok(entry, `${review.from}: no function ${review.file}:${review.function}`);
            for (const [method, hash] of Object.entries(review.hashes ?? {})) {
                const change = (entry.changes ?? []).find(item => item.method === method && item.boundary === review.boundary);
                assert.equal(change?.hash, hash, `${review.from}: ${review.file}:${review.function} ${method}@${review.boundary}`);
            }
        }
        const tests = rustTests();
        const reach = readReach();
        // (a helper whose tests do not hold leaves its function open, `uncited` with the problems listed)
        for (const helper of reviews.helpers) {
            const problems = helperProblems(helper, tests, reach);
            const entry = record.functions.find(item => item.file === helper.file && item.function === helper.function);
            assert.ok(entry, `${helper.from}: no function ${helper.file}:${helper.function}`);
            if (problems.length === 0) assert.equal(entry.status, 'helper', `${helper.from}: ${helper.file}:${helper.function}`);
            else assert.deepEqual([entry.status, entry.problems], ['uncited', problems], `${helper.from}: ${helper.file}:${helper.function}`);
        }
    });

    it('puts a change down to reach only where the reach record says that side never ran the function', () => {
        const reach = readReach();
        const releases = canonicalReleases();
        for (const entry of record.functions) {
            for (const change of entry.changes ?? []) {
                if (!change.by?.startsWith('unreached: ')) continue;
                const held = reach?.byFunction.get(key(entry));
                assert.ok(held && !held.unprobed, `${key(entry)}: no reach record`);
                const at = releases.indexOf(change.boundary);
                const sides = [releases.slice(0, at), releases.slice(at)].map(side => side.some(release => held.releases.has(release)));
                assert.ok(!(sides[0] && sides[1]), `${key(entry)}@${change.boundary}: reached on both sides`);
            }
        }
    });

    it('takes renamed locals for what they are', () => {
        const local = (text) => [{ raw: text }];
        assert.equal(mechanicalClass(local('int f(int a) { int b = a; return b; }'), local('int f(int x) { int y = x; return y; }')), 'renamed-locals');
        // a field and a local of one name are not the same thing
        assert.equal(mechanicalClass(local('void f(double a) { double minX = this.minX - a; }'), local('void f(double a) { double minX = minX - a; }')), null);
        // two fields are not one because neither is declared here
        assert.equal(mechanicalClass(local('boolean f() { return a > b; }'), local('boolean f() { return a > c; }')), null);
        assert.equal(scopedLocals('{ double a = 1; } { double a = 2; } return a;'), scopedLocals('{ double p = 1; } { double q = 2; } return q;'));
    });
});

// The texts under each review and each mechanical class, read again from the trees: a tree whose text changes under them
// fails here, whether or not the record was made again.
const { treeOf } = require('../scripts/compare-gate-sources.cjs');
const trees = canonicalReleases().every(release => treeOf(release));
describe('the reviewed changes, read again from the compare trees', { skip: trees ? false : 'no compare trees on this machine' }, () => {
    it('are the texts each review and mechanical class read', () => {
        const releases = canonicalReleases();
        const seen = new Set();
        for (const entry of record.functions) {
            for (const change of entry.changes ?? []) {
                if (!change.by || change.by.startsWith('unreached: ')) continue;
                const id = `${change.method}@${change.boundary}`;
                if (seen.has(id)) continue;
                seen.add(id);
                const [className, method] = change.method.split('#');
                const pair = [releases[releases.indexOf(change.boundary) - 1], change.boundary];
                const [before, after] = pair.map(release => overloads(release, className, method));
                const texts = items => items.map(({ text }) => text).sort().join('\n');
                assert.equal(changeHash(texts(before), texts(after)), change.hash, `${id}: the trees changed under ${change.by}`);
                if (change.by === 'renamed-locals') assert.equal(mechanicalClass(before, after), 'renamed-locals', id);
            }
        }
    });
});
