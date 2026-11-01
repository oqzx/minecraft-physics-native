'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { verifyFluidGravityDivisors, sourceDivisors, sourceLocator } = (() => {
    const verifier = require('../scripts/verify-fluid-gravity-divisors.cjs');
    return { ...verifier, sourceLocator: require('../scripts/verify-landing-policy.cjs').sourceLocator };
})();

const versioned = path.join(__dirname, '..', 'native', 'src', 'versioned');
const modules = fs.readdirSync(versioned)
    .filter(entry => /^v-.*\.rs$/.test(entry))
    .map(entry => ({ file: entry, source: fs.readFileSync(path.join(versioned, entry), 'utf8') }));

// travel's fluid branches as the releases write them (1.12.2 EntityLivingBase.travel's literal
// sinks; 1.13.2 LivingEntity.java:1639, 1674-1677 inline; 1.21.5 LivingEntity.java:2289, 2407)
const LEGACY = 'this.velocityY *= 0.5;\n this.velocityZ *= 0.5;\n if (!this.hasNoGravity()) {\n this.velocityY -= 0.02;\n }\n'
    + 'this.velocityY *= 0.8F;\n this.velocityZ *= i;\n if (!this.hasNoGravity()) {\n this.velocityY -= 0.02;\n }\n'
    + 'this.velocityY -= 0.08;\n';
const INLINE = 'this.velocityY -= d / 4.0;\n'
    + 'if (this.velocityY <= 0.0 && Math.abs(this.velocityY - 0.005) >= 0.003 && Math.abs(this.velocityY - d / 16.0) < 0.003) {\n'
    + 'this.velocityY = -0.003;\n } else {\n this.velocityY -= d / 16.0;\n }\n';
const MODERN = 'this.setDeltaMovement(this.getDeltaMovement().add(0.0, -e / 4.0, 0.0));\n'
    + 'int i = (int)Math.max(1.0F, f / 4.0F);\n'
    + 'if (bl && Math.abs(arg.y - 0.005) >= 0.003 && Math.abs(arg.y - d / 16.0) < 0.003) {\n e = -0.003;\n } else {\n e = arg.y - d / 16.0;\n }\n';

function stubLocator(sources) {
    return {
        versions: Object.keys(sources),
        livingEntity: version => (sources[version] ? { java: sources[version], from: `stub ${version}` } : undefined),
    };
}

function module(version) {
    return modules.find(entry => entry.file === `v-${version.replaceAll('.', '_')}.rs`);
}

describe('verify-fluid-gravity-divisors', () => {
    it('extracts the divisors in every source form and rejects ambiguous ones', () => {
        assert.deepEqual(sourceDivisors(LEGACY), { form: 'literal', water: 0.02, lava: 0.02 });
        assert.deepEqual(sourceDivisors(INLINE), { form: 'divisor', water: 16, lava: 4 });
        assert.deepEqual(sourceDivisors(MODERN), { form: 'divisor', water: 16, lava: 4 });
        assert.equal(sourceDivisors('return 0;'), undefined);
        assert.throws(() => sourceDivisors(`${MODERN}Math.abs(arg.y - d / 12.0) < 0.003`), /more than one water/);
        assert.throws(() => sourceDivisors(`${MODERN}this.setDeltaMovement(v.add(0.0, -e / 2.0, 0.0));`), /more than one lava/);
        assert.throws(() => sourceDivisors('Math.abs(arg.y - d / 16.0) < 0.003'), /no lava gravity divisor/);
    });

    it('accepts profiles matching their source and rejects a changed divisor', () => {
        const locator = stubLocator({ '1.12.2': LEGACY, '1.13.2': INLINE, '1.21.5': MODERN });
        const run = files => verifyFluidGravityDivisors({ modules: files, locator });
        assert.deepEqual(run([module('1.12.2'), module('1.13.2'), module('1.21.5')]).map(result => result.ok), [true, true, true]);
        for (const [version, field] of [['1.21.5', 'water_gravity_divisor'], ['1.13.2', 'lava_gravity_divisor'], ['1.12.2', 'lava_gravity_divisor']]) {
            const changed = { ...module(version), source: module(version).source.replace(new RegExp(`${field}: [0-9.]+`), `${field}: 8.0`) };
            const [rejected] = run([changed]);
            assert.equal(rejected.ok, false, `${version} ${field}`);
        }
        const broken = { ...module('1.21.5'), source: module('1.21.5').source.replace(/water_gravity_divisor: [0-9.]+/, 'water_gravity_divisor: 0.0') };
        assert.throws(() => run([broken]), /not a positive finite number/);
    });

    const repo = process.env.MC_EXTRACTED_DATA_REPO ?? 'D:/projects/mc-zuri/mc-data/extracted_minecraft_data';
    const locator = sourceLocator({ extractedRepo: fs.existsSync(repo) ? repo : undefined });
    it('validates every registered profile against the decompiled LivingEntity sources', { skip: locator.versions.length === 0 && 'no decompiled sources on this machine' }, () => {
        const results = verifyFluidGravityDivisors({ modules, locator });
        assert.equal(results.length, modules.length);
        for (const result of results.filter(entry => entry.source)) {
            assert.ok(result.ok, `${result.version}: ${JSON.stringify(result.native)} vs ${JSON.stringify(result.source)}`);
        }
        if (fs.existsSync(repo)) assert.deepEqual(results.filter(result => !result.source).map(result => result.version), []);
    });
});
