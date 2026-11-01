'use strict';

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const audit = require('../generated/physics-methods.json');
const report = require('../generated/physics-breakpoints.json');

const releases = Object.keys(audit.releases);
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
    it('covers the releases and every method of the source audit', () => {
        assert.deepEqual(report.releases, releases);
        const methods = new Set(releases.flatMap(release => [...methodHashes(release).keys()]));
        assert.deepEqual(new Set(report.comparedMethods), methods);
    });

    it('lists each adjacent pair of releases whose tracked methods differ, and only adjacent pairs', () => {
        const listed = new Map(report.breakpoints.map(entry => [`${entry.previous}>${entry.current}`, entry]));
        for (const entry of report.breakpoints) {
            assert.equal(releases.indexOf(entry.current), releases.indexOf(entry.previous) + 1, `${entry.previous} -> ${entry.current}`);
        }
        for (let index = 1; index < releases.length; index += 1) {
            const previous = methodHashes(releases[index - 1]);
            const current = methodHashes(releases[index]);
            const files = (release) => new Set(Object.keys(audit.releases[release].files));
            const both = [...files(releases[index - 1])].filter(file => files(releases[index]).has(file));
            const changed = report.comparedMethods
                .filter(method => both.includes(method.split(':')[0]))
                .filter(method => (previous.get(method) ?? '') !== (current.get(method) ?? ''));
            const entry = listed.get(`${releases[index - 1]}>${releases[index]}`);
            assert.deepEqual(entry?.changed ?? [], changed, `${releases[index - 1]} -> ${releases[index]}`);
        }
    });
});
