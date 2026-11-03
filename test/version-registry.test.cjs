'use strict';

// The release registry: the native one (native/src/versioned/mod.rs), the TypeScript one
// (src/versions.ts, src/mineflayer/versions.ts) and the binding agree release for release; Mojang's
// release list (generated/mojang-releases.json) from 1.15 on is registered; every registered
// release has an audited Entity.move, LivingEntity.travel and Player.travel
// (generated/physics-methods.json); and every 1.15 - 1.19 release whose source is at hand has the
// Entity, LivingEntity and Player sources.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { describe, it } = require('node:test');
const { registeredReleases, compareReleases } = require('../scripts/registered-releases.cjs');
const { LEGACY_RELEASES } = require('../scripts/legacy-release-data.cjs');
const physics = require('../dist');
const { LoadNativePhysics } = require('../dist/native');

const root = path.resolve(__dirname, '..');
const native = registeredReleases();
const versions = native.map(({ version }) => version);
const registry = fs.readFileSync(path.join(root, 'native', 'src', 'versioned', 'mod.rs'), 'utf8');
const audit = require('../generated/physics-methods.json');
const mojang = require('../generated/mojang-releases.json');
const sourceManifest = require('../generated/minecraft-versions.json');
const className = version => `NativePhysics_${version.replaceAll('.', '_')}`;
const registryOf = version => ({ version: { minecraftVersion: version, '<': () => false, '>': () => false } });

describe('the release registry', () => {
    it('gives every native module its release, profile and dispatch arm', () => {
        assert.equal(new Set(versions).size, versions.length);
        assert.deepEqual([...versions].sort(compareReleases), versions);
        for (const { version, module, file } of native) {
            assert.equal(file, `v-${version.replaceAll('.', '_')}.rs`);
            assert.equal(module, `v${version.replaceAll('.', '_')}`);
            assert.match(registry, new RegExp(`${module}::Engine::VERSION => Some\\(${module}::Profile::rules\\(\\)\\)`), version);
            assert.match(registry, new RegExp(`${module}::Engine::VERSION => \\{\\s*tick_conforming::<${module}::Engine, ${module}::Profile>`), version);
        }
    });

    it('keeps the TypeScript registry equal to the native one', () => {
        assert.deepEqual([...physics.SUPPORTED_VERSIONS], versions);
        assert.deepEqual(Object.keys(physics.NATIVE_PHYSICS_ENGINES), versions);
        for (const version of versions) {
            const Engine = physics.NATIVE_PHYSICS_ENGINES[version];
            assert.equal(Engine, physics[className(version)], version);
            assert.equal(Engine.name, className(version));
            assert.equal(Engine.version, version);
            assert.ok(Engine.prototype instanceof physics.NativePhysics);
            assert.equal(physics.isSupportedVersion(version), true);
        }
        for (const version of ['1.8', '1.14', '1.20.7', '26.4', '1.21.99']) {
            assert.equal(physics.isSupportedVersion(version), false, version);
        }
    });

    it('agrees with the binding, which refuses every other release', () => {
        const binding = LoadNativePhysics();
        for (const version of versions) assert.equal(binding.isSupportedVersion(version), true, version);
        for (const release of mojang.releases.filter(release => !versions.includes(release.id))) {
            assert.equal(binding.isSupportedVersion(release.id), false, release.id);
        }
    });

    it('constructs each release in its own class, and refuses the others before loading anything', () => {
        const binding = LoadNativePhysics();
        for (const version of ['1.8.8', '1.20', '1.20.2', '1.20.3', '26.3']) {
            const engine = new physics.NATIVE_PHYSICS_ENGINES[version](registryOf(version), { binding });
            assert.equal(engine.version, version);
            const created = physics.createNativePhysics(registryOf(version), { binding });
            assert.ok(created instanceof physics[className(version)], version);
            // the release given beats the registry's
            assert.ok(physics.createNativePhysics(registryOf('1.12.2'), { binding, version }) instanceof physics[className(version)]);
        }
        const unloadable = { get isSupportedVersion() { throw new Error('binding touched'); } };
        assert.throws(() => physics.createNativePhysics(registryOf('1.20.7'), { binding: unloadable }), /unsupported decompiled Minecraft version: 1\.20\.7/);
    });

    it('registers every Mojang release from 1.15 on, and only Mojang releases', () => {
        const ids = mojang.releases.map(release => release.id);
        assert.equal(mojang.latestRelease, ids.at(-1));
        const modern = ids.filter(id => compareReleases(id, '1.15') >= 0);
        for (const id of ['1.15', '1.20', '1.20.2', '1.20.3', '1.21.11', '26.1', '26.3']) assert.ok(modern.includes(id), id);
        assert.deepEqual(modern.filter(id => !versions.includes(id)), []);
        assert.deepEqual(versions.filter(version => !ids.includes(version)), []);
        // the releases before 1.15 the engine registers: the last of each minor line
        const older = versions.filter(version => compareReleases(version, '1.15') < 0);
        assert.deepEqual(older, ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2', '1.14.4']);
    });

    it('audits Entity.move, LivingEntity.travel and Player.travel for every registered release', () => {
        assert.deepEqual(Object.keys(audit.releases), versions);
        for (const version of versions) {
            for (const [file, method] of [['Entity.java', 'move'], ['LivingEntity.java', 'travel'], ['Player.java', 'travel']]) {
                const candidates = audit.releases[version].files[file]?.methods?.[method];
                assert.ok(candidates?.length > 0, `${version} ${file}#${method}`);
                for (const candidate of candidates) assert.match(candidate.sha256, /^[0-9a-f]{64}$/);
            }
        }
        // the releases before the mc-decomp trees come from their Yarn-named decompiles
        for (const { version, source } of LEGACY_RELEASES) {
            const entity = audit.releases[version].files['Entity.java'];
            assert.equal(entity.mapping, 'yarn', version);
            assert.equal(entity.sourceRelease ?? version, source, version);
        }
    });

    it('inventories the vehicle movement overrides', () => {
        for (const version of versions) {
            const release = audit.releases[version];
            const tracked = ['Boat.java', 'AbstractBoat.java', 'AbstractMinecart.java', 'AbstractHorse.java'];
            for (const file of tracked) {
                assert.equal(Boolean(release.files[file]) || release.absentFiles.includes(file), true, `${version} ${file}`);
            }
            const boat = compareReleases(version, '1.21.2') >= 0 ? release.files['AbstractBoat.java'] : release.files['Boat.java'];
            if (boat) assert.ok(boat.methods.tick?.length > 0, `${version} boat tick`);
            const horse = release.files['AbstractHorse.java'];
            if (horse) {
                const ridden = compareReleases(version, '1.19.4') >= 0 ? 'tickRidden' : 'travel';
                assert.ok(horse.methods[ridden]?.length > 0, `${version} AbstractHorse.${ridden}`);
            }
            assert.equal(release.behaviorFeatures.horseSource, Boolean(horse));
        }
        // AbstractBoat holds the boat's movement from 1.21.2 (1.21.11 AbstractBoat.java)
        assert.deepEqual(Object.keys(audit.releases['1.21.11'].files['AbstractBoat.java'].methods), ['floatBoat', 'controlBoat', 'tick']);
    });

    it('finds the Entity, LivingEntity and Player sources of every 1.15 - 1.19 release at hand', () => {
        const modern = versions.filter(version => compareReleases(version, '1.15') >= 0 && compareReleases(version, '1.20') < 0);
        const decompiled = process.env.MC_DECOMPILED_ROOT ?? path.join(root, '..', '..', 'pr', 'mc-decompiled');
        const extracted = process.env.MC_EXTRACTED_DATA_REPO ?? 'D:/projects/mc-zuri/mc-data/extracted_minecraft_data';
        const yarn = ['net/minecraft/entity/Entity.java', 'net/minecraft/entity/LivingEntity.java', 'net/minecraft/entity/player/PlayerEntity.java'];
        const mojangPaths = ['net/minecraft/world/entity/Entity.java', 'net/minecraft/world/entity/LivingEntity.java', 'net/minecraft/world/entity/player/Player.java'];
        for (const version of modern) {
            // the mc-decomp tree the source manifest recorded
            const recorded = sourceManifest.sourceFiles[version]?.files ?? [];
            for (const file of mojangPaths) {
                assert.ok(recorded.some(entry => entry.path === file && entry.present && entry.required), `${version} ${file}`);
            }
            const local = path.join(decompiled, version);
            if (fs.existsSync(local)) {
                const paths = fs.existsSync(path.join(local, mojangPaths[0])) ? mojangPaths : yarn;
                for (const file of paths) assert.ok(fs.existsSync(path.join(local, file)), `${local} ${file}`);
            }
            if (fs.existsSync(extracted)) {
                let tree = null;
                try {
                    tree = execFileSync('git', ['-C', extracted, 'ls-tree', '-r', '--name-only', `origin/client${version}`, '--', 'client/net/minecraft/world/entity'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
                } catch {
                    tree = null;
                }
                if (tree) for (const file of mojangPaths) assert.ok(tree.includes(`client/${file}`), `origin/client${version} ${file}`);
            }
        }
    });
});
