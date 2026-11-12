'use strict';

// One build, recorded: the hash a binary carries is the hash of its sources, a package is exactly what its records say, and
// a binary of another platform or another build, a stale declaration or an unrecorded file is refused. The loader refuses
// an unasked WebAssembly fallback on a declared target. Clean consumers are scripts/consumer-check.cjs, which needs a
// registry; this runs anywhere.

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync, spawnSync } = require('node:child_process');
const { describe, it, before, after } = require('node:test');
const { TARGETS, addonProblem, binaryFormat, hostTarget, provenanceFile } = require('../scripts/native-artifacts.cjs');
const { packageProblems, sourceHash } = require('../scripts/build-provenance.cjs');
const manifest = require('../package.json');

const root = path.resolve(__dirname, '..');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const LINUX = 'x86_64-unknown-linux-gnu';
const WINDOWS = 'x86_64-pc-windows-msvc';
const THREADS = 'wasm32-wasip1-threads';

const elf = (machine = 62) => {
    const bytes = Buffer.alloc(64);
    bytes.writeUInt32BE(0x7f454c46, 0);
    bytes.writeUInt16LE(machine, 18);
    return bytes;
};
const pe = (machine = 0x8664) => {
    const bytes = Buffer.alloc(128);
    bytes.write('MZ', 0, 'latin1');
    bytes.writeUInt32LE(64, 60);
    bytes.write('PE\0\0', 64, 'latin1');
    bytes.writeUInt16LE(machine, 68);
    return bytes;
};

describe('a built binary', () => {
    it('names the sources it was built from', () => {
        const addon = require(path.join(root, 'native', 'artifacts', TARGETS[hostTarget()].outputs[0]));
        const identity = addon.buildIdentity();
        assert.equal(identity.target, hostTarget());
        assert.equal(identity.sourceHash, sourceHash(path.join(root, 'native')));
    });

    it('hashes a source the same whatever its line endings', () => {
        const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-physics-hash-'));
        for (const input of ['Cargo.toml', 'Cargo.lock', 'build.rs']) fs.writeFileSync(path.join(directory, input), 'a\n');
        fs.mkdirSync(path.join(directory, 'data'));
        fs.mkdirSync(path.join(directory, 'src'));
        fs.writeFileSync(path.join(directory, 'src', 'lib.rs'), 'one\ntwo\n');
        const hash = sourceHash(directory);
        fs.writeFileSync(path.join(directory, 'src', 'lib.rs'), 'one\r\ntwo\r\n');
        assert.equal(sourceHash(directory), hash);
        fs.writeFileSync(path.join(directory, 'src', 'lib.rs'), 'one\ntwo!\n');
        assert.notEqual(sourceHash(directory), hash);
        fs.rmSync(directory, { recursive: true, force: true });
    });
});

describe('the declared targets', () => {
    it('are the ones package.json builds, and the loader knows each addon', () => {
        assert.deepEqual([...manifest.napi.targets].sort(), Object.keys(TARGETS).sort());
        const loader = fs.readFileSync(path.join(root, 'src', 'native.ts'), 'utf8');
        for (const target of Object.values(TARGETS).filter(entry => !entry.wasm)) assert.ok(loader.includes(target.outputs[0]), target.outputs[0]);
    });

    it('tell a binary by its bytes, not its name', () => {
        assert.deepEqual(binaryFormat(elf()), { format: 'elf', machine: 62 });
        assert.deepEqual(binaryFormat(pe()), { format: 'pe', machine: 0x8664 });
        assert.equal(binaryFormat(Buffer.from('not a binary')), null);
        assert.equal(addonProblem('index.linux-x64-gnu.node', elf()), null);
        assert.equal(addonProblem('index.win32-x64-msvc.node', pe()), null);
        assert.match(addonProblem('index.linux-x64-gnu.node', pe()), /holds a pe binary/);
        assert.match(addonProblem('index.win32-x64-msvc.node', elf()), /holds a elf binary/);
        assert.match(addonProblem('index.linux-x64-gnu.node', elf(183)), /machine 183/);
        assert.match(addonProblem('index.linux-x64-gnu.node', Buffer.alloc(64)), /no native binary/);
        // a loader branch is no target
        assert.match(addonProblem('index.darwin-arm64.node', elf()), /no declared target/);
        assert.match(addonProblem('index.linux-x64-musl.node', elf()), /no declared target/);
    });
});

describe('a package and its records', () => {
    let directory;
    let counter = 0;
    const hash = 'a'.repeat(64);

    /** A package of the targets, by hand: each file hashed as a real build's would be. */
    function packageOf(targets, { sourceHashes = {} } = {}) {
        counter += 1;
        const package_ = path.join(directory, `package-${counter}`);
        const artifacts = path.join(package_, 'native', 'artifacts');
        fs.mkdirSync(artifacts, { recursive: true });
        fs.mkdirSync(path.join(package_, 'dist'));
        const write = (name, bytes) => fs.writeFileSync(path.join(package_, name), bytes);
        write('dist/index.js', 'exports.ok = true;\n');
        write('native/artifacts/native.js', 'module.exports = {};\n');
        write('native/artifacts/native.d.ts', 'export declare function ok(): void\n');
        for (const target of targets) {
            const name = TARGETS[target].outputs[0];
            const header = target === LINUX ? elf() : target === WINDOWS ? pe() : Buffer.alloc(8);
            write(`native/artifacts/${name}`, Buffer.concat([header, Buffer.from(`${sourceHashes[target] ?? hash} ${target}`)]));
            const files = {};
            for (const file of ['dist/index.js', 'native/artifacts/native.js', 'native/artifacts/native.d.ts', `native/artifacts/${name}`]) {
                files[file] = sha256(fs.readFileSync(path.join(package_, file)));
            }
            write(`native/artifacts/${provenanceFile(target)}`, JSON.stringify({
                schema: 1, target, artifact: TARGETS[target].artifact, commit: 'c'.repeat(40), dirty: false,
                inputs: { sourceHash: sourceHashes[target] ?? hash, generatedHash: 'b' }, files,
            }));
        }
        return package_;
    }

    before(() => {
        directory = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-physics-provenance-'));
    });
    after(() => fs.rmSync(directory, { recursive: true, force: true }));

    it('accepts a package that is its records', () => {
        assert.deepEqual(packageProblems(packageOf([LINUX]), { targets: [LINUX] }), []);
        assert.deepEqual(packageProblems(packageOf([LINUX, WINDOWS])), []);
    });

    it('needs a record', () => {
        const package_ = packageOf([LINUX]);
        fs.rmSync(path.join(package_, 'native', 'artifacts', provenanceFile(LINUX)));
        assert.deepEqual(packageProblems(package_), ['the package holds no provenance record']);
    });

    it('refuses a declaration, a loader or a compiled file that changed', () => {
        for (const file of ['native/artifacts/native.d.ts', 'native/artifacts/native.js', 'dist/index.js']) {
            const package_ = packageOf([LINUX]);
            fs.appendFileSync(path.join(package_, file), '// stale\n');
            assert.match(packageProblems(package_).join('\n'), new RegExp(`${file.replaceAll('.', '\\.')} is not the file`), file);
        }
    });

    it('refuses a missing file and an unrecorded one', () => {
        const missing = packageOf([LINUX]);
        fs.rmSync(path.join(missing, 'native', 'artifacts', 'index.linux-x64-gnu.node'));
        assert.match(packageProblems(missing).join('\n'), /index\.linux-x64-gnu\.node of provenance\.linux-x64-gnu\.json is missing/);
        const extra = packageOf([LINUX]);
        fs.writeFileSync(path.join(extra, 'dist', 'leftover.js'), '');
        assert.match(packageProblems(extra).join('\n'), /dist\/leftover\.js is not in any provenance record/);
        const stray = packageOf([LINUX]);
        fs.writeFileSync(path.join(stray, 'native', 'artifacts', 'index.win32-x64-msvc.node'), pe());
        assert.match(packageProblems(stray).join('\n'), /index\.win32-x64-msvc\.node is not in any provenance record/);
    });

    it('refuses a binary of another build or of another platform, even with a matching hash on record', () => {
        const other = packageOf([LINUX]);
        const addon = path.join(other, 'native', 'artifacts', 'index.linux-x64-gnu.node');
        const bytes = Buffer.concat([elf(), Buffer.from(`${'d'.repeat(64)} ${LINUX}`)]);
        fs.writeFileSync(addon, bytes);
        const record = path.join(other, 'native', 'artifacts', provenanceFile(LINUX));
        const parsed = JSON.parse(fs.readFileSync(record, 'utf8'));
        parsed.files['native/artifacts/index.linux-x64-gnu.node'] = sha256(bytes);
        fs.writeFileSync(record, JSON.stringify(parsed));
        assert.match(packageProblems(other).join('\n'), /was not built from sources aaaaaaaaaaaa/);
        const foreign = packageOf([LINUX]);
        const foreignBytes = Buffer.concat([pe(), Buffer.from(`${hash} ${LINUX}`)]);
        fs.writeFileSync(path.join(foreign, 'native', 'artifacts', 'index.linux-x64-gnu.node'), foreignBytes);
        const foreignRecord = path.join(foreign, 'native', 'artifacts', provenanceFile(LINUX));
        const foreignParsed = JSON.parse(fs.readFileSync(foreignRecord, 'utf8'));
        foreignParsed.files['native/artifacts/index.linux-x64-gnu.node'] = sha256(foreignBytes);
        fs.writeFileSync(foreignRecord, JSON.stringify(foreignParsed));
        assert.match(packageProblems(foreign).join('\n'), /holds a pe binary/);
    });

    it('refuses records of two builds in one package', () => {
        const mixed = packageOf([LINUX, WINDOWS], { sourceHashes: { [WINDOWS]: 'e'.repeat(64) } });
        assert.match(packageProblems(mixed).join('\n'), /another sourceHash .*: a mixed build/);
    });

    it('refuses a package that is not the intended distribution', () => {
        assert.match(packageProblems(packageOf([LINUX]), { targets: [LINUX, THREADS] }).join('\n'), /intended/);
    });
});

describe('LoadNativePhysics', () => {
    let directory;
    const script = "const p = require(process.cwd() + '/dist/native'); const b = p.LoadNativePhysics(); process.stdout.write(JSON.stringify(p.nativeBackend(b)));";

    /** The compiled loader over a stand-in binding that says it is `target`. */
    function loaderOf(target) {
        const package_ = fs.mkdtempSync(path.join(directory, 'p-'));
        fs.mkdirSync(path.join(package_, 'dist'));
        fs.mkdirSync(path.join(package_, 'native', 'artifacts'), { recursive: true });
        // dist/native.js and what it requires of its own folder, however that grows
        const copied = new Set();
        const copy = (file) => {
            if (copied.has(file)) return;
            copied.add(file);
            fs.copyFileSync(path.join(root, 'dist', file), path.join(package_, 'dist', file));
            const text = fs.readFileSync(path.join(root, 'dist', file), 'utf8');
            for (const [, neighbour] of text.matchAll(/require\("\.\/([^"]+)"\)/g)) copy(`${neighbour}.js`);
        };
        copy('native.js');
        fs.writeFileSync(path.join(package_, 'native', 'artifacts', 'native.js'),
            `module.exports = { buildIdentity: () => (${JSON.stringify({ target, sourceHash: 'h', crateVersion: '0' })}) };`);
        return package_;
    }
    const run = (package_, environment = {}) => {
        const env = { ...process.env, ...environment };
        for (const name of ['NAPI_RS_FORCE_WASI', 'NAPI_RS_WASI_FLAVOR', 'NAPI_RS_NATIVE_LIBRARY_PATH']) if (!(name in environment)) delete env[name];
        return spawnSync(process.execPath, ['-e', script], { cwd: package_, encoding: 'utf8', env });
    };

    before(() => {
        directory = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-physics-loader-'));
    });
    after(() => fs.rmSync(directory, { recursive: true, force: true }));

    it('says which build answered', () => {
        const result = run(loaderOf(hostTarget()));
        assert.equal(result.status, 0, result.stderr);
        assert.equal(JSON.parse(result.stdout).backend, 'native');
    });

    it('refuses WebAssembly nobody asked for on a declared native target', () => {
        const package_ = loaderOf(THREADS);
        const refused = run(package_);
        assert.notEqual(refused.status, 0);
        assert.match(refused.stderr, /did not load, and WebAssembly was not asked for/);
        for (const environment of [{ NAPI_RS_FORCE_WASI: 'true' }, { NAPI_RS_FORCE_WASI: 'error' }, { NAPI_RS_WASI_FLAVOR: 'wasm32-wasi' }]) {
            const asked = run(package_, environment);
            assert.equal(asked.status, 0, asked.stderr);
            assert.equal(JSON.parse(asked.stdout).backend, 'wasm');
        }
    });

    it('warns when a preference for WebAssembly fell back to the addon', () => {
        const result = run(loaderOf(hostTarget()), { NAPI_RS_FORCE_WASI: 'true' });
        assert.equal(result.status, 0, result.stderr);
        assert.match(result.stderr, /fell back to the native addon/);
    });

    it('refuses an addon from before the identity', () => {
        const package_ = loaderOf(hostTarget());
        fs.writeFileSync(path.join(package_, 'native', 'artifacts', 'native.js'), 'module.exports = {};');
        assert.match(run(package_).stderr, /predates this package/);
    });
});

describe('the dependency contract', () => {
    const importsOf = (directory, extension, pattern) => fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
        const file = path.join(directory, entry.name);
        if (entry.isDirectory()) return importsOf(file, extension, pattern);
        return entry.name.endsWith(extension) ? [...fs.readFileSync(file, 'utf8').matchAll(pattern)].map(match => match[1]) : [];
    });
    const packageName = specifier => specifier.split('/').slice(0, specifier.startsWith('@') ? 2 : 1).join('/');

    it('loads nothing undeclared at run time, and no development tool', () => {
        const required = new Set(importsOf(path.join(root, 'dist'), '.js', /require\("([^".][^"]*)"\)/g).map(packageName));
        const builtin = name => require('node:module').builtinModules.includes(name.replace(/^node:/, ''));
        for (const name of required) assert.ok(builtin(name) || manifest.dependencies[name], `${name} is required at run time and not a dependency`);
    });

    it('names in its public types only the optional peers', () => {
        const imported = new Set(importsOf(path.join(root, 'dist'), '.d.ts', /from '([^'.][^']*)'/g).map(packageName));
        assert.deepEqual([...imported].sort(), Object.keys(manifest.peerDependencies).sort());
        for (const name of imported) assert.equal(manifest.peerDependenciesMeta[name].optional, true, name);
    });

    it('leaves the low-level entry free of them', () => {
        const text = fs.readFileSync(path.join(root, 'dist', 'native.d.ts'), 'utf8');
        assert.doesNotMatch(text, /mineflayer/);
    });

    it('keeps the wasm runtime and pins the toolchains the build refuses to run without', () => {
        for (const name of ['@emnapi/core', '@emnapi/runtime', '@napi-rs/wasm-runtime']) assert.ok(manifest.dependencies[name], name);
        assert.match(manifest.packageManager, /^pnpm@\d+\.\d+\.\d+$/);
        assert.match(fs.readFileSync(path.join(root, 'rust-toolchain.toml'), 'utf8'), /channel = "\d+\.\d+\.\d+"/);
        execFileSync('git', ['-C', root, 'ls-files', '--error-unmatch', 'native/Cargo.lock', 'pnpm-lock.yaml'], { stdio: 'ignore' });
    });
});
