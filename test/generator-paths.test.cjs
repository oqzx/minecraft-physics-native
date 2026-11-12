'use strict';

// Manifests record portable paths (B039), and a source directory cannot make a release (B023).

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { describe, it } = require('node:test');
const { serializePath, resolveRecordedPath } = require('../scripts/portable-paths.cjs');

const root = path.resolve(__dirname, '..');
const required = [
    'net/minecraft/world/entity/Entity.java',
    'net/minecraft/world/entity/LivingEntity.java',
    'net/minecraft/world/entity/player/Player.java',
    'net/minecraft/world/phys/shapes/VoxelShape.java',
    'net/minecraft/world/phys/AABB.java',
];
const writeTree = (directory) => {
    for (const file of required) {
        const target = path.join(directory, file);
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.writeFileSync(target, 'class Fixture {\n}\n');
    }
};

/** Runs the version generator on a source tree of `directories`, the manifest beside it. */
function generate(directories) {
    const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc source paths '));
    const sources = path.join(base, 'decompiled sources');
    for (const directory of directories) writeTree(path.join(sources, directory));
    const output = path.join(base, 'out dir', 'minecraft-versions.json');
    execFileSync(process.execPath, [path.join(root, 'scripts', 'generate-version-manifest.cjs')], {
        cwd: base,
        env: { ...process.env, MC_DECOMP_SRC: sources, MC_PHYSICS_VERSION_MANIFEST: output },
        stdio: 'pipe',
    });
    return { base, sources, output, manifest: JSON.parse(fs.readFileSync(output, 'utf8')) };
}

describe('recorded paths', () => {
    it('serializes with forward slashes under either host', () => {
        assert.equal(serializePath('E:\\repo\\generated', 'E:\\pr\\mc decomp\\src', path.win32), '../../pr/mc decomp/src');
        assert.equal(serializePath('/repo/generated', '/pr/mc decomp/src', path.posix), '../../pr/mc decomp/src');
    });

    it('keeps a root on another Windows drive absolute, and resolves it there', () => {
        const recorded = serializePath('E:\\repo\\generated', 'D:\\sources', path.win32);
        assert.equal(recorded, 'D:/sources');
        assert.equal(resolveRecordedPath('E:\\repo\\generated', recorded, path.win32), 'D:\\sources');
    });

    it('reads a manifest written with backslashes as the same directory on either host', () => {
        assert.equal(resolveRecordedPath('/repo/generated', '..\\..\\mc-decomp\\src', path.posix), '/mc-decomp/src');
        assert.equal(resolveRecordedPath('E:\\repo\\generated', '..\\..\\mc-decomp\\src', path.win32), 'E:\\mc-decomp\\src');
    });

    it('refuses a Windows root on a POSIX host instead of naming a file after it', () => {
        assert.throws(() => resolveRecordedPath('/repo/generated', 'D:/sources', path.posix), /Windows path/);
        assert.throws(() => resolveRecordedPath('/repo/generated', '', path.posix), /records no path/);
    });

    it('resolves the POSIX absolute root it was given', () => {
        assert.equal(resolveRecordedPath('/repo/generated', '/opt/mc sources', path.posix), '/opt/mc sources');
    });

    it('checks in no backslash in a recorded root or file path', () => {
        for (const file of ['minecraft-versions.json', 'physics-methods.json']) {
            const manifest = JSON.parse(fs.readFileSync(path.join(root, 'generated', file), 'utf8'));
            assert.doesNotMatch(manifest.sourceRoot ?? '', /\\/, file);
            for (const release of Object.values(manifest.releases ?? {})) {
                for (const entry of Object.values(release.files)) assert.doesNotMatch(entry.path, /\\/);
            }
        }
    });

    it('records a legacy source relative to its root, not to the depth of the checkout', () => {
        const legacy = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'legacy-release-data.json'), 'utf8'));
        for (const [version, release] of Object.entries(legacy.releases)) {
            assert.doesNotMatch(release.registry.source, /^\.\./, version);
            for (const block of Object.values(release.blocks)) if (block.source) assert.doesNotMatch(block.source, /^\.\./, version);
        }
    });

    it('generates through spaces, from another working directory, a portable sourceRoot', () => {
        const { base, sources, output, manifest } = generate(['1.21/1.21.11']);
        assert.equal(manifest.sourceRoot, '../decompiled sources');
        assert.equal(resolveRecordedPath(path.dirname(output), manifest.sourceRoot), sources);
        fs.rmSync(base, { recursive: true, force: true });
    });
});

describe('source directories and releases', () => {
    it('lists a registered release it finds, and no snapshot, candidate or unregistered release', () => {
        const { base, manifest } = generate([
            '1.21/1.21.11', '1.21/1.21.11-rc1', '1.21/1.21.11-pre3', '1.21/1.21.12', '26.1/26.1.2-rc-1', '26.4/26.4',
        ]);
        assert.deepEqual(manifest.versions, ['1.21.11']);
        assert.deepEqual(Object.keys(manifest.sourceFiles), ['1.21.11']);
        assert.deepEqual(manifest.methodDiffs, {});
        fs.rmSync(base, { recursive: true, force: true });
    });
});
