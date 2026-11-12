'use strict';

// The published package: what `npm pack` ships (the compiled adapter and the native loader,
// declarations and builds, never decompiled source, the source audits or the engine's sources), a
// packed copy that loads and ticks on its own, the paths package.json and the loader name, and the
// package and smoke verifications of the built addon.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { describe, it, before, after } = require('node:test');

const root = path.resolve(__dirname, '..');
const manifest = require('../package.json');
const { isNapiOutput, loaderOutputs, wasmOutputs, verifyArtifactDirectory } = require('../scripts/native-artifacts.cjs');
const npm = (args, cwd = root) => execFileSync(process.platform === 'win32' ? 'npm.cmd' : 'npm', args, {
    cwd,
    encoding: 'utf8',
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
});

/** The headers of an x86-64 PE, all the artifact check reads of an addon. */
function stubAddon() {
    const bytes = Buffer.alloc(128);
    bytes.write('MZ', 0, 'latin1');
    bytes.writeUInt32LE(64, 60);
    bytes.write('PE\0\0', 64, 'latin1');
    bytes.writeUInt16LE(0x8664, 68);
    return bytes;
}

describe('the published package', () => {
    let packed;
    let scratch;

    before(() => {
        scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-physics-pack-'));
        packed = JSON.parse(npm(['pack', '--json', '--ignore-scripts', '--pack-destination', scratch]))[0];
    });

    after(() => {
        if (scratch) fs.rmSync(scratch, { recursive: true, force: true });
    });

    it('ships the adapter and the native builds only', () => {
        const files = packed.files.map(file => file.path.replaceAll('\\', '/'));
        for (const file of ['package.json', 'THIRD_PARTY_NOTICES', 'dist/index.js', 'dist/index.d.ts', 'dist/whole-numbers.js', 'dist/whole-fields.js', 'dist/native.js', 'dist/versions.js', 'native/artifacts/native.js', 'native/artifacts/native.d.ts']) {
            assert.ok(files.includes(file), file);
        }
        assert.ok(files.some(file => /^native\/artifacts\/index\.[\w-]+\.node$/.test(file)));
        const allowed = file => /^(?:package\.json|README\.md|THIRD_PARTY_NOTICES|dist\/.+)$/.test(file)
            || (file.startsWith('native/artifacts/') && isNapiOutput(file.slice('native/artifacts/'.length)));
        for (const file of files) assert.ok(allowed(file), file);
        // no decompiled source, source audit or engine source
        for (const file of files) {
            assert.doesNotMatch(file, /\.java$|^generated\/|^native\/(?:src|data)\/|^scripts\/|^src\/|^test\/|todo\.md|parity-deviations\.md/, file);
        }
    });

    // tar is given relative names: GNU tar reads a drive letter as a remote host
    it('holds no decompiled vanilla code', () => {
        const unpacked = path.join(scratch, 'unpacked');
        fs.mkdirSync(unpacked);
        execFileSync('tar', ['-xzf', packed.filename, '-C', 'unpacked'], { cwd: scratch, stdio: 'ignore' });
        const visit = directory => fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
            const entryPath = path.join(directory, entry.name);
            return entry.isDirectory() ? visit(entryPath) : [entryPath];
        });
        for (const file of visit(path.join(unpacked, 'package'))) {
            if (!/\.(?:js|cjs|mjs|ts|json|md)$/.test(file)) continue;
            const text = fs.readFileSync(file, 'utf8');
            // Java declarations as the decompilers write them (the audits' `source` texts)
            assert.doesNotMatch(text, /^\s*(?:public|protected|private) (?:static )?(?:final )?(?:void|boolean|double|float|int|Vec3d?) \w+\([^)]*\) \{$/m, file);
            assert.doesNotMatch(text, /this\.level\(\)\.|this\.world\.isClient|net\/minecraft\/world\/entity\/Entity\.java/, file);
        }
        fs.rmSync(unpacked, { recursive: true, force: true });
    });

    it('loads from the packed copy and runs a tick of every release family', () => {
        const unpacked = path.join(scratch, 'run');
        fs.mkdirSync(unpacked);
        execFileSync('tar', ['-xzf', packed.filename, '-C', 'run'], { cwd: scratch, stdio: 'ignore' });
        const script = `
            const physics = require(${JSON.stringify(path.join(unpacked, 'package'))});
            const binding = physics.LoadNativePhysics();
            const world = new binding.NativeWorld();
            world.setFullBlock(0, 0, 0, 1);
            const out = {};
            for (const version of ['1.8.8', '1.12.2', '1.20.2', '1.21.11', '26.3']) {
                const state = binding.simulateVersionedTick(version, world, {
                    position: { x: 0.5, y: 1.5, z: 0.5 }, velocity: { x: 0, y: -0.1, z: 0 },
                    halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false,
                }, {});
                out[version] = [binding.isSupportedVersion(version), state.position.y < 1.5];
            }
            out.classes = physics.SUPPORTED_VERSIONS.every(v => physics.NATIVE_PHYSICS_ENGINES[v].version === v);
            process.stdout.write(JSON.stringify(out));
        `;
        // the packed loader, not one the environment points to
        const environment = { ...process.env };
        delete environment.NAPI_RS_NATIVE_LIBRARY_PATH;
        const result = JSON.parse(execFileSync(process.execPath, ['-e', script], { cwd: unpacked, encoding: 'utf8', env: environment }));
        for (const version of ['1.8.8', '1.12.2', '1.20.2', '1.21.11', '26.3']) assert.deepEqual(result[version], [true, true], version);
        assert.equal(result.classes, true);
        fs.rmSync(unpacked, { recursive: true, force: true });
    });

    // the consumer has mineflayer-physics-utils, which the package only names in types: the context is the dependency's
    it('makes, clones, ticks and applies a real context from the packed copy', () => {
        const unpacked = path.join(scratch, 'consumer');
        fs.mkdirSync(unpacked);
        execFileSync('tar', ['-xzf', packed.filename, '-C', 'consumer'], { cwd: scratch, stdio: 'ignore' });
        const script = `
            const physics = require(${JSON.stringify(path.join(unpacked, 'package'))});
            const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
            const { botOf } = require(${JSON.stringify(path.join(root, 'test', 'bot-fixture.cjs'))});
            const out = {};
            for (const version of ['1.8.8', '1.20.4', '1.20.5', '1.21.11']) {
                const bot = botOf(version);
                const setup = physics.createBotPhysics(bot);
                const ctx = setup.context(EPhysicsCtx);
                ctx.state = ctx.state.clone();
                for (let i = 0; i < 5; i += 1) setup.physics.simulate(ctx, bot.world);
                ctx.state.apply(bot);
                setup.dispose();
                out[version] = [setup.physics.version, bot.entity.position.z !== 0.5 || bot.entity.position.x !== 0.5, bot.eventNames().length];
            }
            process.stdout.write(JSON.stringify(out));
        `;
        const environment = { ...process.env, NODE_PATH: path.join(root, 'node_modules') };
        delete environment.NAPI_RS_NATIVE_LIBRARY_PATH;
        const result = JSON.parse(execFileSync(process.execPath, ['-e', script], { cwd: unpacked, encoding: 'utf8', env: environment }));
        for (const version of ['1.8.8', '1.20.4', '1.20.5', '1.21.11']) assert.deepEqual(result[version], [version, true, 0], version);
        fs.rmSync(unpacked, { recursive: true, force: true });
    });

    it('names only files that exist', () => {
        for (const file of [manifest.main, manifest.types]) assert.ok(fs.existsSync(path.join(root, file)), file);
        for (const [name, command] of Object.entries(manifest.scripts)) {
            for (const [, script] of command.matchAll(/\bnode (scripts\/[\w.-]+\.cjs)/g)) {
                assert.ok(fs.existsSync(path.join(root, script)), `${name}: ${script}`);
            }
        }
        // LoadNativePhysics loads ../native/artifacts/native.js from dist
        const loader = fs.readFileSync(path.join(root, 'src', 'native.ts'), 'utf8');
        const modulePath = /'(\.\.\/native\/artifacts\/native\.js)'/.exec(loader)?.[1];
        assert.ok(modulePath);
        assert.ok(fs.existsSync(path.join(root, 'dist', modulePath)));
        // every relative require of the scripts and tests resolves
        for (const directory of ['scripts', 'test']) {
            for (const file of fs.readdirSync(path.join(root, directory)).filter(name => name.endsWith('.cjs'))) {
                const text = fs.readFileSync(path.join(root, directory, file), 'utf8');
                for (const [, target] of text.matchAll(/require\((?:"|')(\.{1,2}\/[^"']+)(?:"|')\)/g)) {
                    const resolved = path.resolve(root, directory, target);
                    assert.ok(['', '.js', '.cjs', '.json'].some(extension => fs.existsSync(resolved + extension))
                        || fs.existsSync(path.join(resolved, 'index.js')), `${directory}/${file}: ${target}`);
                }
            }
        }
    });

    // what the wasm builds (native:build:wasm, native:build:wasip1) write: every loader, worker,
    // module and declaration ships, and the artifact check takes them as napi-rs outputs
    it('packs a WebAssembly distribution with its loaders and declarations', () => {
        const copy = path.join(scratch, 'wasm');
        const artifacts = path.join(copy, 'native', 'artifacts');
        fs.mkdirSync(artifacts, { recursive: true });
        fs.mkdirSync(path.join(copy, 'dist'));
        fs.writeFileSync(path.join(copy, 'dist', 'index.js'), '');
        fs.writeFileSync(path.join(copy, 'package.json'), JSON.stringify({ ...manifest, scripts: {} }));
        const outputs = [...loaderOutputs, ...wasmOutputs, 'index.win32-x64-msvc.node'];
        const loaded = outputs.filter(name => name.endsWith('.cjs') || name.endsWith('.node'));
        for (const name of outputs) {
            // a loader naming each addon and wasm loader as napi-rs's does ('./<file>')
            const text = name === 'native.js' ? loaded.map(file => `load('./${file}')\n`).join('') : '';
            fs.writeFileSync(path.join(artifacts, name), name.endsWith('.node') ? stubAddon() : text);
        }
        verifyArtifactDirectory(artifacts);
        const files = JSON.parse(npm(['pack', '--dry-run', '--json', '--ignore-scripts'], copy))[0]
            .files.map(file => file.path.replaceAll('\\', '/'));
        for (const name of outputs) assert.ok(files.includes(`native/artifacts/${name}`), name);
        // a retired build is refused
        fs.writeFileSync(path.join(artifacts, 'index.win-32-x-64-msvc.node'), '');
        assert.throws(() => verifyArtifactDirectory(artifacts), /index\.win-32-x-64-msvc\.node/);
        fs.rmSync(path.join(artifacts, 'index.win-32-x-64-msvc.node'));
        // and so is an addon that is not what its name says
        fs.writeFileSync(path.join(artifacts, 'index.win32-x64-msvc.node'), 'not a binary');
        assert.throws(() => verifyArtifactDirectory(artifacts), /no native binary/);
        fs.rmSync(copy, { recursive: true, force: true });
    });

    it('passes the package verification and the native smoke suite', () => {
        execFileSync(process.execPath, [path.join(root, 'scripts', 'verify-native-package.cjs')], { cwd: root, stdio: 'pipe' });
        execFileSync(process.execPath, [path.join(root, 'scripts', 'native-smoke.cjs')], { cwd: root, stdio: 'pipe' });
    });
});
