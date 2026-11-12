'use strict';

// B007, B018: every version gate of the shared native code is listed by function with its bounded interval, and is
// either backed by the source audit or marked unverified. A gate cannot move without this list changing.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { document, gatesOf, sharedGates } = require('../scripts/shared-gates.cjs');
const { ALLOWANCES, normalised, renameLocals, sameUpToLeftovers, versionJsonDifference } = require('../scripts/compare-gate-sources.cjs');
const { sharedFunctions } = require('../scripts/shared-functions.cjs');

const checkedIn = fs.readFileSync(path.join(__dirname, '..', 'generated', 'shared-gates.json'), 'utf8').replace(/\r\n/g, '\n');

describe('the shared gates', () => {
    const entries = sharedGates();

    it('are the list that is checked in', () => {
        assert.equal(checkedIn, document(entries));
    });

    it('give every gated function a bounded interval and a status', () => {
        assert.ok(entries.length > 100);
        for (const entry of entries) {
            assert.ok(entry.boundaries.length > 0, entry.function);
            assert.ok(['source-compared', 'unverified'].includes(entry.status), entry.function);
        }
    });

    it('cover the subsystems the ticket names', () => {
        const files = new Set(entries.map(entry => entry.file));
        for (const file of ['simulation.rs', 'collision.rs', 'vehicle.rs', 'projectile.rs']) assert.ok(files.has(file), file);
    });

    it('find a gate by its function, and leave out tests', () => {
        const gates = gatesOf([
            'pub(crate) fn outer(release: MinecraftVersion) {',
            '    if release.at_least(1, 21, 5) || release.at_least(1, 20, 0) {}',
            '}',
            '#[cfg(test)]',
            'mod tests {',
            '    fn inner() { release.at_least(1, 9, 0); }',
            '}',
        ].join('\n'));
        assert.deepEqual([...gates.keys()], ['outer']);
        assert.deepEqual([...gates.get('outer')], ['1.21.5', '1.20']);
    });

    it('takes only the test item after a #[cfg(test)], with real code after it', () => {
        const gates = gatesOf([
            '#[cfg(test)]',
            'fn helper(release: MinecraftVersion) {',
            '    release.at_least(1, 9, 0);',
            '}',
            '#[cfg(test)]',
            '#[path = "x_tests.rs"]',
            'mod x_tests;',
            'fn real(release: MinecraftVersion) {',
            '    release.at_least(1, 21, 5);',
            '}',
        ].join('\n'));
        assert.deepEqual([...gates.keys()], ['real']);
        const simulation = fs.readFileSync(path.join(__dirname, '..', 'native', 'src', 'simulation.rs'), 'utf8');
        assert.ok(gatesOf(simulation).has('attribute_landing_damage'));
    });

    it('calls a gate with no difference found unverified, and names the boundary', () => {
        const none = sharedGates(new Set());
        assert.ok(none.every(entry => entry.status === 'unverified' && entry.missing.length === entry.boundaries.length));
        const all = sharedGates(new Set(entries.flatMap(({ file, function: name, boundaries }) => boundaries.map(boundary => `${file}#${name}@${boundary}`))));
        assert.ok(all.every(entry => entry.status === 'source-compared' && !entry.missing));
    });
});

describe('the comparison of a gate\'s method across its boundary', () => {
    const report = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'generated', 'gate-sources.json'), 'utf8'));

    it('lists what it forgives, each with its reason, and no more than that', () => {
        assert.deepEqual(report.allowances.map(({ name }) => name), ALLOWANCES.map(({ name }) => name));
        for (const { why } of report.allowances) assert.ok(why.length > 20);
    });

    it('forgives what a decompile prints differently, and no change of a number, a type or an operator', () => {
        assert.equal(normalised('double a = this.x; // note\nreturn a;'), normalised('@Override double b = x;   return b;'));
        for (const [before, after] of [['return x * 0.5F;', 'return x * 0.5;'], ['return (float)x;', 'return x;'], ['return x + 1.0;', 'return x - 1.0;'], ['return a.foo();', 'return a.bar();']]) {
            assert.notEqual(normalised(before), normalised(after), before);
        }
        assert.equal(renameLocals('int p = q + foo(p);'), 'int l0 = l1 + foo(l0);');
    });

    it('takes no method for a gate unless it differs: what a review named and is equal stays refuted', () => {
        for (const gate of report.gates) {
            for (const { change, class: className, method, versionJson, before, after } of gate.found ?? []) {
                // a value of the release's version json, which differs across the pair, or no finding
                if (versionJson) assert.ok(change === 'value' && JSON.stringify(before) !== JSON.stringify(after), versionJson);
                else assert.ok(['body', 'added', 'removed'].includes(change), `${className}#${method}`);
            }
        }
        assert.deepEqual(report.staleReviews, []);
        assert.deepEqual(report.missingTrees, []);
    });

    it('reads a version json only at its pin, and finds a value only where the pair differs', (context) => {
        const change = versionJsonDifference(['1.16.5', '1.17'], 'javaVersion.majorVersion');
        if (change === undefined) return context.skip('no version json at its pin on this machine');
        assert.deepEqual(change, { before: 8, after: 16 });
        assert.equal(versionJsonDifference(['1.17', '1.17.1'], 'javaVersion.majorVersion'), null);
    });

    it('lets a name the legacy mapping left stand for any name, and nothing else', () => {
        assert.ok(sameUpToLeftovers('return l0.method_8578(l1);', 'return l0.getLightColor(l1);'));
        assert.ok(!sameUpToLeftovers('return l0.method_8578(l1) + 1;', 'return l0.getLightColor(l1) + 2;'));
        assert.ok(!sameUpToLeftovers('return l0;', 'return l1;'));
    });
});

describe('the shared functions', () => {
    const entries = sharedFunctions();
    const named = name => entries.find(entry => entry.function === name);

    it('list every function outside the tests, the ungated too', () => {
        assert.ok(entries.length > 1000);
        assert.ok(entries.some(entry => entry.interval === 'every release'));
        assert.ok(!entries.some(entry => /tests\.rs$/.test(entry.file)));
    });

    it('give each an interval and a status, with the incomplete marker the default', () => {
        for (const entry of entries) {
            assert.ok(entry.interval === 'every release' || entry.interval.length > 0, entry.function);
            assert.ok(['entry', 'source-compared', 'unverified'].includes(entry.status), entry.function);
        }
        assert.equal(entries.filter(entry => entry.interval === 'every release').every(entry => entry.status !== 'source-compared'), true);
    });

    it('map callers and the families that reach a function from their entry points', () => {
        assert.ok(named('attribute_landing_damage').callers.includes('check_fall_damage'));
        assert.ok(named('attribute_landing_damage').families.includes('player'));
        assert.ok(named('simulate_versioned_boat_tick').families.includes('boat'));
        assert.ok(!named('simulate_versioned_boat_tick').families.includes('projectile'));
    });
});

describe('the gate mutation sweep', () => {
    const { sweepProblems } = require('../scripts/gate-mutants.cjs');
    const sweep = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'generated', 'gate-mutation.json'), 'utf8'));
    const { spawnSync } = require('node:child_process');
    const known = spawnSync('git', ['cat-file', '-e', `${sweep.commit}^{commit}`], { cwd: path.join(__dirname, '..') }).status === 0;

    it('is the sweep of the gates the commit it names had', { skip: !known && 'the commit is not in this clone' }, () => {
        assert.match(sweep.commit, /^[0-9a-f]{7,40}$/);
        assert.deepEqual(sweepProblems(sweep.entries, sweep.commit), []);
    });

    it('is refused as the sweep of another commit', { skip: !known && 'the commit is not in this clone' }, () => {
        const first = sweep.entries[0];
        const moved = [{ ...first, boundary: '1.8.9' }, ...sweep.entries.slice(1)];
        const problems = sweepProblems(moved, sweep.commit).join('\n');
        assert.match(problems, new RegExp(`${first.function}@${first.boundary.replaceAll('.', '\.')}: a gate of ${sweep.commit} the sweep has no entry for`));
        assert.match(problems, new RegExp(`${first.function}@1\.8\.9: swept, and no gate of ${sweep.commit}`));
    });
});
