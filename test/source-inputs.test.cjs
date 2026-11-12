'use strict';

// The recorded source inputs: each one is read back and checked, an input not at hand is reported
// instead of assumed (B038, B049), and a client jar is admitted by its hash and its embedded release.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync, spawnSync } = require('node:child_process');
const { describe, it } = require('node:test');
const { readRecorded } = require('../scripts/source-inputs.cjs');
const { PINNED_SOURCES, PINNED_CLIENTS, CLIENT_DATA, sourceCache, clientJarRoot, extractedRepo } = require('../scripts/source-pins.cjs');
const { pinnedJars } = require('../scripts/materialize-sources.cjs');
const { directoryIdentity } = require('../scripts/source-identity.cjs');
const { literalRegistrations } = require('../scripts/block-references.cjs');
const { openZip } = require('../scripts/zip-read.cjs');

const root = path.resolve(__dirname, '..');
const generated = path.join(root, 'generated');
const script = name => path.join(root, 'scripts', name);
const modern = JSON.parse(fs.readFileSync(path.join(generated, 'minecraft-versions.json'), 'utf8')).versions;
const absentEnvironment = (base) => {
    const absent = path.join(base, 'absent');
    return { ...process.env, MC_SOURCE_ROOT: absent, MC_SOURCE_CACHE: absent, MC_EXTRACTED_DATA_REPO: absent, MC_DECOMP_SRC: absent };
};

/** A stored zip of `entries`, enough for what scripts/zip-read.cjs reads. */
function zipOf(entries) {
    const parts = [];
    const directory = [];
    let offset = 0;
    for (const [name, text] of Object.entries(entries)) {
        const [nameBytes, data] = [Buffer.from(name), Buffer.from(text)];
        const local = Buffer.alloc(30);
        local.writeUInt32LE(0x04034b50, 0);
        local.writeUInt32LE(data.length, 18);
        local.writeUInt32LE(data.length, 22);
        local.writeUInt16LE(nameBytes.length, 26);
        parts.push(local, nameBytes, data);
        const central = Buffer.alloc(46);
        central.writeUInt32LE(0x02014b50, 0);
        central.writeUInt32LE(data.length, 20);
        central.writeUInt32LE(data.length, 24);
        central.writeUInt16LE(nameBytes.length, 28);
        central.writeUInt32LE(offset, 42);
        directory.push(central, nameBytes);
        offset += 30 + nameBytes.length + data.length;
    }
    const centralBytes = Buffer.concat(directory);
    const end = Buffer.alloc(22);
    end.writeUInt32LE(0x06054b50, 0);
    end.writeUInt16LE(Object.keys(entries).length, 10);
    end.writeUInt32LE(centralBytes.length, 12);
    end.writeUInt32LE(offset, 16);
    return Buffer.concat([...parts, centralBytes, end]);
}

describe('recorded inputs', () => {
    it('reads a path back, and calls an input that is not there absent', () => {
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-inputs-'));
        fs.writeFileSync(path.join(base, 'present.java'), 'class A {}');
        const found = readRecorded(base, ['present.java', 'missing.java']);
        assert.equal(found.get('present.java').toString(), 'class A {}');
        assert.equal(found.get('missing.java'), null);
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('reads an MC_SOURCE_ROOT source wherever that root is, so a manifest does not depend on the layout', () => {
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-inputs-'));
        fs.mkdirSync(path.join(base, '26.3', 'client'), { recursive: true });
        fs.writeFileSync(path.join(base, '26.3', 'client', 'A.java'), 'class A {}');
        const script = "const { readRecorded } = require(process.argv[1]); process.stdout.write(String(readRecorded('.', ['mc-source:26.3/client/A.java']).get('mc-source:26.3/client/A.java')));";
        const read = execFileSync(process.execPath, ['-e', script, path.join(__dirname, '..', 'scripts', 'source-inputs.cjs')], { encoding: 'utf8', env: { ...process.env, MC_SOURCE_ROOT: base } });
        assert.equal(read, 'class A {}');
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('holds no path of a layout in a generated manifest', () => {
        for (const name of ['block-registries', 'block-state-definitions', 'minecraft-versions', 'physics-methods']) {
            assert.doesNotMatch(fs.readFileSync(path.join(__dirname, '..', 'generated', `${name}.json`), 'utf8'), /"\.\.\/\.\.\/\.\.\/[^"]*\/client\//, name);
        }
    });

    it('refuses a pin nobody admitted', () => {
        assert.throws(() => readRecorded(generated, [`${'0'.repeat(40)}:client/net/minecraft/Foo.java`]), /no pinned commit or client/);
    });

    it('reads a pinned commit as a Git object when it is not unpacked', { skip: !fs.existsSync(extractedRepo()) }, () => {
        const previous = process.env.MC_SOURCE_CACHE;
        process.env.MC_SOURCE_CACHE = path.join(os.tmpdir(), 'mc-no-cache-here');
        try {
            const name = `${PINNED_SOURCES['1.20.4']}:client/version.json`;
            assert.equal(JSON.parse(readRecorded(generated, [name]).get(name).toString()).id, '1.20.4');
        } finally {
            if (previous === undefined) delete process.env.MC_SOURCE_CACHE;
            else process.env.MC_SOURCE_CACHE = previous;
        }
    });
});

describe('verify-version-coverage', () => {
    it('runs with no mc-decomp tree and no sources at all, and calls every release cached', () => {
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-coverage-'));
        const output = execFileSync(process.execPath, [script('verify-version-coverage.cjs')], { env: absentEnvironment(base), encoding: 'utf8' });
        assert.match(output, new RegExp(`Verified [0-9] of ${modern.length} `));
        assert.match(output, /Cached, inputs not at hand: 1\.14\.4/);
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('fails --fresh when an input is not at hand', () => {
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-coverage-'));
        const run = spawnSync(process.execPath, [script('verify-version-coverage.cjs'), '--fresh'], { env: absentEnvironment(base), encoding: 'utf8' });
        assert.equal(run.status, 1);
        assert.match(run.stderr, /every release must be read from its sources/);
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('fails on a source that is not the one recorded', { skip: !fs.existsSync(path.join(sourceCache(), '26.1.2', 'client', 'net')) }, () => {
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-coverage-'));
        const environment = absentEnvironment(base);
        environment.MC_SOURCE_CACHE = base;
        const blocks = path.join(base, '26.1.2', 'client', 'net', 'minecraft', 'world', 'level', 'block');
        fs.mkdirSync(blocks, { recursive: true });
        fs.writeFileSync(path.join(base, '26.1.2', 'client', 'version.json'), '{"id":"26.1.2"}');
        fs.writeFileSync(path.join(blocks, 'Blocks.java'), 'tampered');
        const run = spawnSync(process.execPath, [script('verify-version-coverage.cjs')], { env: environment, encoding: 'utf8' });
        assert.equal(run.status, 1);
        assert.match(run.stderr, /26\.1\.2 block registry fingerprint mismatch/);
        fs.rmSync(base, { recursive: true, force: true });
    });
});

describe('a pinned client', () => {
    const fixture = (version, embedded) => {
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-client-'));
        const jar = zipOf({ 'version.json': JSON.stringify({ id: embedded }) });
        fs.mkdirSync(path.join(base, version));
        fs.writeFileSync(path.join(base, version, `minecraft-${version}-client.jar`), jar);
        return { base, sha1: crypto.createHash('sha1').update(jar).digest('hex') };
    };

    it('is taken at its sha1 and its embedded name', () => {
        const { base, sha1 } = fixture('26.1.2', '26.1.2');
        assert.equal(pinnedJars('26.1.2', { client: sha1 }, base).client.endsWith('minecraft-26.1.2-client.jar'), true);
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('is refused at another sha1', () => {
        const { base } = fixture('26.1.2', '26.1.2');
        assert.throws(() => pinnedJars('26.1.2', PINNED_CLIENTS['26.1.2'], base), /pinned 4e618f09/);
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('is refused when it embeds another release', () => {
        const { base, sha1 } = fixture('26.1.2', '26.1.2-rc-1');
        assert.throws(() => pinnedJars('26.1.2', { client: sha1 }, base), /embeds 26\.1\.2-rc-1/);
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('covers every release no commit or tree does, and each client once', () => {
        const pinned = new Set([...Object.keys(PINNED_SOURCES), ...Object.keys(PINNED_CLIENTS), '26.2', '26.3']);
        assert.deepEqual(modern.filter(version => !pinned.has(version)), []);
        assert.deepEqual(Object.keys(PINNED_CLIENTS).filter(version => PINNED_SOURCES[version]), []);
        assert.deepEqual(Object.keys(CLIENT_DATA).sort(), [...Object.keys(PINNED_SOURCES), '26.2', '26.3'].sort());
    });

    it('holds the very jars pinned, where they are at hand', { skip: !fs.existsSync(clientJarRoot()) }, () => {
        for (const [version, pin] of Object.entries(PINNED_CLIENTS)) {
            const { client } = pinnedJars(version, pin);
            assert.equal(JSON.parse(openZip(client).read('version.json')).id, version);
        }
    });
});

describe('what the exact clients changed', () => {
    it('reads the copper factories however the decompiler prints their field', () => {
        const source = [
            'public class Blocks {',
            '   public static final WeatheringCopperBlocks COPPER_BARS = WeatheringCopperBlocks.create(',
            '      "copper_bars", Blocks::register, WaxedThing::new, WeatheringThing::new',
            '   );',
            '}',
        ].join('\n');
        assert.ok(literalRegistrations(source).waxed_oxidized_copper_bars);
    });

    it('widens a float box literal as the JVM does', () => {
        // Shapes.box(..., 0.9F, ...): (double) 0.9F, in the tables at sixteenths
        for (const version of ['26.1.2', '1.17.1', '1.20.3']) {
            const table = fs.readFileSync(path.join(root, 'native', 'data', 'releases', `v${version.replaceAll('.', '_')}.rs`), 'utf8');
            assert.equal(table.includes('0.05624999850988388'), true, version);
            assert.equal(table.includes('max: [0.0625, 0.05625,'), false, version);
        }
    });

    it('reads the identity of the oldest trees from what they embed', { skip: !fs.existsSync(require('../scripts/source-roots.cjs').legacySourcesRoot()) }, () => {
        const decompiled = require('../scripts/source-roots.cjs').legacySourcesRoot();
        for (const version of ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']) {
            assert.equal(directoryIdentity(path.join(decompiled, version)), version);
        }
    });
});
