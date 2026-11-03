'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { verifyLandingPolicies, sourceLocator } = require('../scripts/verify-landing-policy.cjs');

const versioned = path.join(__dirname, '..', 'native', 'src', 'versioned');
const modules = fs.readdirSync(versioned)
    .filter(entry => /^v-.*\.rs$/.test(entry))
    .map(entry => ({ file: entry, source: fs.readFileSync(path.join(versioned, entry), 'utf8') }));
const registered = [...fs.readFileSync(path.join(versioned, 'mod.rs'), 'utf8').matchAll(/#\[path = "(v-[^"]+\.rs)"\]/g)].map(match => match[1]);

// LivingEntity's fall-damage computations as the releases write them (1.20.4 LivingEntity.java:1557-1564,
// origin/client1.21.4 :1622-1628, 1.21.5 :1672-1683)
const LEGACY = 'int computeFallDamage(float fallDistance, float damageMultiplier) {\n'
    + '    return MathHelper.ceil((fallDistance - 3.0F - h) * damageMultiplier);\n}';
const ATTRIBUTE = 'return Mth.ceil((double)(var4 * var2) * this.getAttributeValue(Attributes.FALL_DAMAGE_MULTIPLIER));';
const MODERN = 'return Mth.floor(e * f * this.getAttributeValue(Attributes.FALL_DAMAGE_MULTIPLIER));\n'
    + 'private double calculateFallPower(double d) {\n    return d + 1.0E-6 - this.getAttributeValue(Attributes.SAFE_FALL_DISTANCE);\n}';

function stubLocator(sources) {
    return {
        versions: Object.keys(sources),
        livingEntity: version => (sources[version] ? { java: sources[version], from: `stub ${version}` } : undefined),
    };
}

function module(version) {
    return modules.find(entry => entry.file === `v-${version.replaceAll('.', '_')}.rs`);
}

describe('verify-landing-policy', () => {
    it('reads the policy of every registered profile from its Rust module', () => {
        assert.deepEqual(modules.map(entry => entry.file).sort(), [...registered].sort());
        const sources = { '1.8.8': LEGACY, '26.3': MODERN };
        const results = verifyLandingPolicies({ modules, locator: stubLocator(sources) });
        assert.equal(results.length, registered.length);
        const policy = version => results.find(result => result.version === version).native;
        assert.deepEqual(policy('1.20.4'), { version: '1.20.4', formula: 'legacy', rounding: 'ceil', epsilon: false });
        assert.deepEqual(policy('1.20.5'), { version: '1.20.5', formula: 'attribute', rounding: 'ceil', epsilon: false });
        assert.deepEqual(policy('1.21'), { version: '1.21', formula: 'attribute', rounding: 'ceil', epsilon: false });
        assert.deepEqual(policy('1.21.5'), { version: '1.21.5', formula: 'attribute', rounding: 'floor', epsilon: true });
        assert.deepEqual(policy('26.1.2'), { version: '26.1.2', formula: 'attribute', rounding: 'floor', epsilon: true });
    });

    it('accepts a profile matching its source and rejects one that rounds otherwise', () => {
        const locator = stubLocator({ '1.20.4': LEGACY, '1.21.4': ATTRIBUTE, '1.21.5': MODERN });
        const run = files => verifyLandingPolicies({ modules: files, locator });
        assert.deepEqual(run([module('1.20.4'), module('1.21.4'), module('1.21.5')]).map(result => result.ok), [true, true, true]);
        const ceiling = { ...module('1.21.5'), source: module('1.21.5').source.replace('LandingDamageRounding::Floor', 'LandingDamageRounding::Ceil') };
        const [rejected] = run([ceiling]);
        assert.equal(rejected.ok, false);
        assert.equal(rejected.source.rounding, 'floor');
        // the 1.21.4 profile against a 1.21.5-style source: the epsilon and the floor both differ
        const [mismatch] = verifyLandingPolicies({ modules: [module('1.21.4')], locator: stubLocator({ '1.21.4': MODERN }) });
        assert.equal(mismatch.ok, false);
    });

    it('validates a release without a snapshot only between neighbours that agree', () => {
        const agreeing = verifyLandingPolicies({ modules: [module('1.15.1')], locator: stubLocator({ '1.15': LEGACY, '1.16': LEGACY }) });
        assert.equal(agreeing[0].ok, true);
        assert.match(agreeing[0].basis, /bracketed by 1\.15 and 1\.16/);
        const split = verifyLandingPolicies({ modules: [module('1.21.4')], locator: stubLocator({ '1.21.3': ATTRIBUTE, '1.21.5': MODERN }) });
        assert.equal(split[0].ok, false);
        assert.equal(split[0].source, undefined);
    });

    const repo = process.env.MC_EXTRACTED_DATA_REPO ?? 'D:/projects/mc-zuri/mc-data/extracted_minecraft_data';
    const locator = sourceLocator({ extractedRepo: fs.existsSync(repo) ? repo : undefined });
    it('validates every registered profile against the decompiled LivingEntity sources', { skip: locator.versions.length === 0 && 'no decompiled sources on this machine' }, () => {
        const results = verifyLandingPolicies({ modules, locator });
        const checked = results.filter(result => result.source);
        for (const result of checked) assert.ok(result.ok, `${result.version}: ${JSON.stringify(result.native)} vs ${JSON.stringify(result.source)}`);
        if (fs.existsSync(repo)) assert.deepEqual(results.filter(result => !result.source).map(result => result.version), []);
    });
});
