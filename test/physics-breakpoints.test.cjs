'use strict';

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const audit = require('../generated/physics-methods.json');
const mojang = require('../generated/mojang-releases.json');
const report = require('../generated/physics-breakpoints.json');
const { registeredReleases } = require('../scripts/registered-releases.cjs');

const releases = registeredReleases().map(({ version }) => version);
const decompile = entry => `${entry.origin ?? 'mc-decomp'}/${entry.mapping ?? 'mojang'}`;
const methodHashes = (release) => {
    const hashes = new Map();
    for (const [file, entry] of Object.entries(audit.releases[release].files)) {
        for (const [method, candidates] of Object.entries(entry.methods)) {
            hashes.set(`${file}:${method}`, candidates.map(candidate => candidate.sha256).join(','));
        }
    }
    return hashes;
};

describe('physics-breakpoints.json', () => {
    it('covers every registered release and every method of the source audit', () => {
        assert.deepEqual(report.releases, releases);
        assert.deepEqual(Object.keys(audit.releases), releases);
        const methods = new Set(releases.flatMap(release => [...methodHashes(release).keys()]));
        assert.deepEqual(new Set(report.comparedMethods), methods);
        assert.deepEqual(report.comparedClasses, mojang.mappedClasses);
    });

    it('compares the methods of adjacent releases only within one decompile', () => {
        const listed = new Map(report.breakpoints.map(entry => [`${entry.previous}>${entry.current}`, entry]));
        for (const entry of report.breakpoints) {
            assert.equal(releases.indexOf(entry.current), releases.indexOf(entry.previous) + 1, `${entry.previous} -> ${entry.current}`);
        }
        for (let index = 1; index < releases.length; index += 1) {
            const [left, right] = [releases[index - 1], releases[index]];
            const previous = methodHashes(left);
            const current = methodHashes(right);
            const comparable = report.comparedMethods.filter((method) => {
                const file = method.split(':')[0];
                const leftFile = audit.releases[left].files[file];
                const rightFile = audit.releases[right].files[file];
                return leftFile && rightFile && decompile(leftFile) === decompile(rightFile);
            });
            const changed = comparable.filter(method => (previous.get(method) ?? '') !== (current.get(method) ?? ''));
            const unavailable = report.comparedMethods.filter(method => !comparable.includes(method));
            const entry = listed.get(`${left}>${right}`);
            assert.deepEqual(entry?.changed ?? [], changed, `${left} -> ${right}`);
            assert.deepEqual(entry?.unavailable ?? [], unavailable, `${left} -> ${right}`);
            if (changed.length > 0) assert.ok(entry.categories.includes('source'), `${left} -> ${right}`);
        }
    });

    it('lists the mapping, registry, data-pack and protocol differences under their categories', () => {
        const mappings = new Map(mojang.releases.map(release => [release.id, release.mappings]));
        for (const entry of report.breakpoints) {
            for (const category of ['mappings', 'registries', 'dataPacks']) {
                assert.equal(entry.categories.includes(category), entry[category] !== undefined, `${entry.previous} -> ${entry.current} ${category}`);
            }
            assert.equal(entry.categories.includes('protocol'), entry.protocol?.packets !== undefined);
            const left = mappings.get(entry.previous);
            const right = mappings.get(entry.current);
            if (left && right) {
                const differing = Object.keys(right.classes)
                    .filter(name => left.classes[name] && left.classes[name].sha256 !== right.classes[name].sha256);
                assert.deepEqual(entry.mappings?.changed ?? [], differing, `${entry.previous} -> ${entry.current}`);
            } else {
                assert.ok(entry.unavailableSources.includes('mappings'));
            }
        }
        // the block registry and protocol of known transitions: 1.12.2 -> 1.13.2 flattens the blocks
        // and renumbers the protocol, 1.21.4 -> 1.21.5 changes the movement packets
        const flattening = report.breakpoints.find(entry => entry.previous === '1.12.2');
        assert.ok(flattening.registries.blocks.added.length > 0);
        assert.deepEqual([flattening.protocol.previous, flattening.protocol.current], [340, 404]);
        assert.ok(report.breakpoints.find(entry => entry.previous === '1.21.4').categories.includes('protocol'));
    });
});
