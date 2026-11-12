'use strict';

// B302, B029: six releases are exact releases of minecraft-data that read their tables, by path, from the release of
// the same protocol, on the evidence in generated/data-aliases.json. The adapter serves each on its own registry, and
// a registry of another release, the pair's included, is refused as for every release.

const assert = require('node:assert/strict');
const path = require('node:path');
const { describe, it } = require('node:test');
const physics = require('..');
const { ALIASES } = require('../scripts/generate-data-aliases.cjs');
const record = require(path.join(__dirname, '..', 'generated', 'data-aliases.json'));

const registryOf = require('prismarine-registry');
const minecraftData = require('minecraft-data');
const noBinding = (version) => ({ ...(version ? { version } : {}), get binding() { throw new Error('binding touched'); } });
const SIX = ['1.19.1', '1.21.10', '1.21.2', '1.21.7', '26.1.1', '26.1.2'];

describe('the six releases on their pair\'s tables', () => {
    it('declare themselves in minecraft-data, with the protocol and data version of their jars', () => {
        assert.deepEqual(Object.keys(record.aliases).sort(), SIX);
        const protocols = require('minecraft-data/minecraft-data/data/pc/common/protocolVersions.json');
        const entry = release => protocols.find(row => row.minecraftVersion === release);
        const manifest = require('../generated/release-manifest.json').releases;
        const jars = { '1.19.1': [760, 3117], '1.21.2': [768, 4080], '1.21.7': [772, 4438], '1.21.10': [773, 4556], '26.1.1': [775, 4788], '26.1.2': [775, 4790] };
        for (const [requested, alias] of Object.entries(record.aliases)) {
            const { version } = minecraftData(requested);
            assert.equal(version.minecraftVersion, requested);
            assert.equal(version.version, alias.protocol, requested);
            assert.equal(version['=='](requested), true, requested);
            assert.deepEqual([entry(requested).version, entry(requested).dataVersion], jars[requested], requested);
            assert.deepEqual([manifest[requested].protocol, manifest[requested].dataVersion], jars[requested], requested);
            assert.equal(entry(alias.data).version, alias.protocol, requested);
        }
    });

    it('read every table of their pair but the version, and a language of their own only where the jars differ', () => {
        const paths = require('minecraft-data/minecraft-data/data/dataPaths.json').pc;
        for (const [requested, alias] of Object.entries(record.aliases)) {
            const own = Object.keys({ ...paths[requested], ...paths[alias.data] }).filter(table => paths[requested][table] !== paths[alias.data][table]).sort();
            assert.deepEqual(own, alias.ownTables, requested);
            assert.deepEqual(own, alias.languageDiffering.length > 0 ? ['language', 'version'] : ['version'], requested);
            const [mine, pair] = [minecraftData(requested), minecraftData(alias.data)];
            for (const table of ['blocksArray', 'blockCollisionShapes', 'itemsArray', 'entitiesArray', 'effectsArray', 'biomesArray', 'enchantmentsArray', 'attributesArray', 'protocol']) {
                assert.deepEqual(mine[table], pair[table], `${requested} ${table}`);
            }
        }
        // 1.19.1 lacks 1.19.2's two messages of an expired key and has its own invalid one
        assert.deepEqual(record.aliases['1.19.1'].languageDiffering, ['chat.disabled.expiredProfileKey', 'multiplayer.disconnect.expired_public_key', 'multiplayer.disconnect.invalid_public_key']);
        assert.equal(minecraftData('1.19.1').language['multiplayer.disconnect.invalid_public_key'], 'Unable to parse profile public key.');
        assert.equal(minecraftData('1.19.1').language['multiplayer.disconnect.expired_public_key'], undefined);
    });

    it('rest on the vanilla data of both jars and a reviewed reason for every class', () => {
        for (const [requested, alias] of Object.entries(record.aliases)) {
            assert.ok(alias.vanillaDataFiles > 4000, requested);
            assert.match(alias.vanillaData, /^[0-9a-f]{64}$/, requested);
            assert.ok(alias.classes > 6000, requested);
            assert.ok(alias.languageKeys > 5000, requested);
            assert.deepEqual(Object.keys(alias.differing).sort(), Object.keys(ALIASES[requested].reviewed).sort(), requested);
            for (const reason of Object.values(alias.differing)) assert.ok(reason.length > 0, requested);
        }
    });

    it('take 26.1\'s tables for 26.1.1 and 26.1.2 only on the same 8594 files, with their different data versions kept', () => {
        const [one, two] = [record.aliases['26.1.1'], record.aliases['26.1.2']];
        assert.equal(one.vanillaData, two.vanillaData);
        assert.equal(one.vanillaDataFiles, 8594);
        assert.ok(!Object.keys(two.differing).some(name => /\.(world|core|tags)\./.test(name)), 'no game class but the listed server handler');
        assert.match(two.differing['net.minecraft.server.network.ServerGamePacketListenerImpl'], /handleAttack/);
        const manifest = require('../generated/release-manifest.json').releases;
        assert.deepEqual(['26.1', '26.1.1', '26.1.2'].map(release => manifest[release].dataVersion), [4786, 4788, 4790]);
    });

    it('are served on their own registry, which selects its own release', () => {
        for (const requested of SIX) {
            const engine = physics.createNativePhysics(registryOf(requested), { version: requested, binding: physics.LoadNativePhysics() });
            assert.equal(engine.version, requested);
            engine.dispose();
            const own = physics.createNativePhysics(registryOf(requested), { binding: physics.LoadNativePhysics() });
            assert.equal(own.version, requested);
            own.dispose();
        }
        assert.equal(physics.dataReleaseOf, undefined, 'no alias at run time');
    });

    it('refuse the registry of any other release, their pair\'s included', () => {
        for (const [requested, { data }] of Object.entries(record.aliases)) {
            assert.throws(() => new physics.NativePhysics(registryOf(data), noBinding(requested)), new RegExp(`registry release ${data.replaceAll('.', '\\.')} cannot serve Minecraft ${requested.replaceAll('.', '\\.')}`));
            assert.throws(() => new physics.NativePhysics(registryOf(requested), noBinding(data)), /cannot serve Minecraft/);
        }
        assert.throws(() => new physics.NativePhysics(registryOf('1.21.4'), noBinding('1.21.2')), /cannot serve Minecraft 1\.21\.2/);
        assert.throws(() => new physics.NativePhysics(registryOf('1.21.6'), noBinding('1.21.7')), /cannot serve Minecraft 1\.21\.7/);
        assert.throws(() => new physics.NativePhysics(registryOf('26.1'), noBinding('26.2')), /cannot serve Minecraft 26\.2/);
    });

    it('let a bot run its release on its own registry, and hold it to that', () => {
        const bot = { version: '1.21.2', registry: registryOf('1.21.2'), on() {}, removeListener() {} };
        const setup = physics.createBotPhysics(bot, { binding: physics.LoadNativePhysics() });
        assert.equal(setup.physics.version, '1.21.2');
        setup.dispose();
        assert.throws(() => physics.createBotPhysics({ version: '1.21.2', registry: registryOf('1.21.3'), on() {} }), /the bot runs 1\.21\.2/);
        assert.throws(() => physics.createBotPhysics({ version: '1.21.4', registry: registryOf('1.21.3'), on() {} }), /the bot runs 1\.21\.4/);
    });
});

describe('the jars the data aliases were proved on', () => {
    const { clientPin } = require('../scripts/java-oracle/classpath.cjs');
    const { jarIdentity } = require('../scripts/generate-data-aliases.cjs');

    it('are each release pair at its pins, recorded', () => {
        for (const [requested, alias] of Object.entries(record.aliases)) {
            assert.deepEqual(Object.keys(alias.jars).sort(), [requested, alias.data].sort(), requested);
            for (const [release, jars] of Object.entries(alias.jars)) {
                assert.equal(jars.client, clientPin(release), `${release} client`);
                assert.match(jars.server, /^[0-9a-f]{40}$/, `${release} server`);
                if (!/^\d\d\./.test(release)) assert.match(jars.intermediary, /^[0-9a-f]{64}$/, `${release} intermediary`);
            }
        }
    });

    it('refuses a jar that is not at hand or not at its pin, before it compares anything', () => {
        const before = process.env.MC_CLIENT_JAR_ROOT;
        process.env.MC_CLIENT_JAR_ROOT = path.join(__dirname, 'no-such-jars');
        try {
            assert.throws(() => jarIdentity('1.19.1'), /ENOENT|no such file/);
        } finally {
            if (before === undefined) delete process.env.MC_CLIENT_JAR_ROOT;
            else process.env.MC_CLIENT_JAR_ROOT = before;
        }
    });
});
