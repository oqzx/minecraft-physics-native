'use strict';

// Every friction the adapter emits against generated/block-friction.json, which scripts/generate-block-friction.cjs
// reads out of each release's own block registry, and the failures that keep that file from defaulting silently.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const adapter = require('..');
const { floatBits, parseDefaultFriction, parseRegistryFriction, parseLegacyFriction, parseLegacyClass } = require('../scripts/block-friction.cjs');
const { frictionOf, treeOf } = require('../scripts/generate-block-friction.cjs');
const { registeredReleases } = require('../scripts/registered-releases.cjs');

const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'generated', 'block-friction.json'), 'utf8'));
const registered = registeredReleases().map(({ version }) => version);

/** The registry of a release the engine accepts it for, undefined where minecraft-data holds no exact one. */
function registryOf(version) {
    try {
        const registry = require('prismarine-registry')(version);
        new adapter.BlockMapper(registry.version, registry);
        return registry;
    } catch {
        return undefined;
    }
}

/** Every block of a registry as the adapter sends it, by name: the friction of its default state. */
function emittedFrictions(registry) {
    const Block = require('prismarine-block')(registry);
    const mapper = new adapter.BlockMapper(registry.version, registry);
    const emitted = {};
    for (const block of registry.blocksArray) {
        const state = block.defaultState ?? block.id << 4;
        const described = adapter.describeBlock(Block.fromStateId(state, 0));
        const [update] = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
        // (air and a bare fluid send no shape: the engine's 0.6, napi_api.rs block.shape.map_or)
        emitted[block.name] = update?.shape?.slipperiness ?? 0.6;
    }
    return emitted;
}

describe('block friction', () => {
    it('lists every registered release, and the ones no exact source covers by name', () => {
        assert.deepEqual([...Object.keys(manifest.releases), ...manifest.unverified].sort(), [...registered].sort());
        // every release has its exact client now; the 1.16.4 branch embeds 20w45a and stays unread
        assert.deepEqual(manifest.unverified, []);
        assert.match(manifest.releases['1.16.4'].source, /^client 1952d94a/);
        assert.match(manifest.releases['26.1.2'].source, /^client 4e618f09/);
    });

    it('records the expression, the resolved bits and the source of each value', () => {
        for (const [version, release] of Object.entries(manifest.releases)) {
            assert.equal(release.default.expression, '0.6F', version);
            assert.match(release.source, /\S/, version);
            assert.match(release.sha256, /^[0-9a-f]{64}$/, version);
            assert.ok(Object.keys(release.frictions).length >= 3, version);
            for (const [name, entry] of Object.entries(release.frictions)) {
                assert.match(entry.expression, /F$/, `${version} ${name}`);
                assert.equal(entry.value, Math.fround(Number(entry.expression.slice(0, -1))), `${version} ${name}`);
                assert.equal(entry.bits, floatBits(entry.value), `${version} ${name}`);
            }
        }
        assert.equal(manifest.releases['26.3'].frictions.blue_ice.bits, '0x3f7d2f1b');
        assert.equal(manifest.releases['26.3'].frictions.ice.bits, '0x3f7ae148');
        assert.equal(manifest.releases['26.3'].frictions.slime_block.bits, '0x3f4ccccd');
    });

    it('emits every block of every release at the registry\'s bits, the defaults too', () => {
        const compared = [];
        for (const [version, release] of Object.entries(manifest.releases)) {
            const registry = registryOf(version);
            if (!registry) continue;
            compared.push(version);
            const emitted = emittedFrictions(registry);
            for (const name of Object.keys(release.frictions)) assert.ok(name in emitted, `${version}: ${name} is no block of the registry`);
            for (const [name, friction] of Object.entries(emitted)) {
                const expected = release.frictions[name]?.value ?? release.default.value;
                // (the engine reads it as vanilla's float: `as f32` where it is used)
                assert.equal(floatBits(Math.fround(friction)), floatBits(expected), `${version} ${name}`);
            }
        }
        assert.ok(compared.includes('1.8.8') && compared.includes('1.12.2') && compared.includes('1.13.2') && compared.includes('26.3'), compared.join(' '));
        // every release, the six whose tables are their pair's (B302) included: none is skipped for its registry
        assert.equal(compared.length, 50, compared.join(' '));
    });

    it('reads the same from the sources at hand', { skip: treeOf('26.3') == null }, () => {
        for (const version of ['1.12.2', '1.13.2', '1.16.5', '26.3']) {
            const tree = treeOf(version);
            if (tree) assert.deepEqual(frictionOf(tree, version), manifest.releases[version], version);
        }
    });

    describe('reading a registry', () => {
        const registry = body => `public class Blocks {\n${body}\n}\n`;
        const ice = 'public static final Block ICE = register("ice", new IceBlock(Properties.of().friction(0.98F).strength(0.5F)));';

        it('takes direct values, legal zeros, and a copy unless it sets its own', () => {
            const frictions = parseRegistryFriction(registry(`${ice}
                public static final Block STILL = register("still", new Block(Properties.of().friction(0.0F)));
                public static final Block COPIED = register("copied", new Block(Properties.ofFullCopy(ICE).strength(1.0F)));
                public static final Block OLD = register("old", new Block(Settings.copy(COPIED)));
                public static final Block OWN = register("own", new Block(Properties.ofLegacyCopy(ICE).friction(0.7F)));
                public static final Block PLAIN = register("plain", new Block(Properties.of()));`), 'test');
            assert.deepEqual(Object.keys(frictions), ['copied', 'ice', 'old', 'own', 'still']);
            assert.equal(frictions.still.value, 0);
            assert.equal(frictions.still.bits, '0x00000000');
            assert.equal(frictions.copied.via, 'ice');
            assert.equal(frictions.old.value, Math.fround(0.98));
            assert.equal(frictions.own.value, Math.fround(0.7));
            assert.equal(frictions.own.via, undefined);
        });

        it('fails on what it cannot resolve instead of taking the default', () => {
            const fails = (body, message) => assert.throws(() => parseRegistryFriction(registry(body), 'test'), message);
            fails(ice.replace('0.98F', 'SLIPPERY'), /is not a float literal/);
            fails(ice.replace('0.98F', '0.98'), /is not a float literal/);
            fails(`${ice}
                private static Properties helper() { return Properties.of().friction(0.9F); }`, /outside a registration/);
            fails(`${ice}
                public static final Block COPIED = register("copied", new Block(Properties.ofFullCopy(UNKNOWN)));`, /copies UNKNOWN/);
            fails(`${ice}
                public static final Block SHEET = register("sheet", new Block(sheetOf(ICE)));`, /sheet builds on ice/);
            assert.throws(() => parseRegistryFriction('class Blocks {}', 'test'), /no block registrations/);
            assert.throws(() => parseDefaultFriction('class Block {}', 'test'), /no default friction/);
        });

        it('takes a mention of another class\'s field by the same name for what it is', () => {
            parseRegistryFriction(registry(`${ice}
                public static final Block SOUND = register("sound", new Block(Properties.of().sound(SoundType.ICE)));`), 'test');
        });

        it('follows a class before 1.13 to the setter it extends, and fails on an unknown class', () => {
            const source = 'register(79, "ice", new IceBlock().setStrength(0.5F));\nregister(\n212,\n"frosted_ice",\nnew FrostedIceBlock());\nregister(1, "stone", new StoneBlock());';
            const classes = {
                IceBlock: parseLegacyClass('public class IceBlock extends Block { IceBlock() { this.slipperiness = 0.98F; } }'),
                FrostedIceBlock: parseLegacyClass('public class FrostedIceBlock extends IceBlock {}'),
            };
            const declared = name => ['IceBlock', 'FrostedIceBlock', 'StoneBlock'].includes(name);
            const frictions = parseLegacyFriction(source, classes, declared, 'test');
            assert.deepEqual(Object.keys(frictions), ['frosted_ice', 'ice']);
            assert.equal(frictions.frosted_ice.via, 'IceBlock');
            assert.throws(() => parseLegacyFriction(source, classes, name => name !== 'StoneBlock', 'test'), /StoneBlock, which no source declares/);
            assert.throws(() => parseLegacyFriction(source, { IceBlock: { expression: 'friction' } }, declared, 'test'), /is not a float literal/);
        });
    });
});
