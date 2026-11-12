'use strict';

// B049, B050: each generated-data defect the audit found (M26 to M32), put back on purpose, and the check that catches
// it; and that a generation which fails publishes nothing. test/block-shapes.test.cjs and test/block-tags.test.cjs hold
// the same for their own tables.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { after, describe, it } = require('node:test');
const { admission, admitted } = require('../scripts/release-admission.cjs');
const { sha256OfModule } = require('../scripts/generate-release-manifest.cjs');
const { extractMethod } = require('../scripts/generate-version-manifest.cjs');
const { directoryIdentity } = require('../scripts/source-identity.cjs');
const { mergePacks, resolveTags, membershipHash } = require('../scripts/generate-block-tags.cjs');
const { againstClient, buildTable, parseDump } = require('../scripts/generate-block-shapes.cjs');
const { runStages, recover } = require('../scripts/generate-all.cjs');
const { isDeclaration } = require('../scripts/verify-generation-invariants.cjs');
const { explain, inventory, OUTPUTS, ORACLES } = require('../scripts/provenance.cjs');
const { integrityReport } = require('../scripts/report-generated-integrity.cjs');

const root = path.resolve(__dirname, '..');
// Every scratch folder these tests make is removed when the file's tests end: each gate run of every lane made ~40 MB
// of them under the system temp folder and kept them, until the drive filled.
const scratch = [];
const scratchDirectory = prefix => { const directory = fs.mkdtempSync(path.join(os.tmpdir(), prefix)); scratch.push(directory); return directory; };
after(() => { for (const directory of scratch) fs.rmSync(directory, { recursive: true, force: true }); });
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const manifest = generated('release-manifest.json').releases;
const runtime = generated('runtime-registries.json').releases;
const registries = generated('block-registries.json').versions;

/** 26.3's admission, with the generated registry changed by `change`. */
function admittedWith(change) {
    const registry = structuredClone(registries['26.3']);
    change(registry);
    return admission('26.3', manifest['26.3'], { moduleSha256: sha256OfModule('26.3'), runtime: runtime['26.3'], registry });
}

const table = (...blocks) => buildTable('x', parseDump(['tags\tholders', ...blocks.flatMap(([id, states]) => [
    ['block', id, 'net.minecraft.world.level.block.Block', '0.6', '1.0', '1.0', '0'].join('\t'),
    ...Array.from({ length: states }, (_, index) => ['state', id, index, index, states > 1 ? `age=${index}` : '', '', '', '', '', '000000', '0000000:EmptyFluid', ''].join('\t')),
])].join('\n'), 'x'), { client: 'c' });

describe('each audited defect, put back', () => {
    it('stays out of the fixtures when nothing is wrong', () => {
        assert.equal(admittedWith(() => {}).data.status, 'complete');
    });

    it('M26: a block family registered through a collection, lost', () => {
        const status = admittedWith(registry => {
            registry.blocks = registry.blocks.filter(id => !/copper/.test(id));
        });
        assert.equal(status.data.status, 'incomplete');
        assert.match(status.data.reasons.join(), /blocks of the client are not in the table/);
        assert.equal(admitted(status), false);
    });

    it('M27: a symbolic id taken for a block', () => {
        const status = admittedWith(registry => {
            registry.blocks = [...registry.blocks, 'blockitemids.oak_door'];
        });
        assert.deepEqual(status.data.reasons, ['1 blocks in the table are not the client\'s']);
    });

    it('M28: an inherited property dropped from a block', () => {
        assert.deepEqual(againstClient(table(['oak_door', 1]), [['oak_door', 2]]), ['oak_door has 1 states, the client 2']);
    });

    it('M29: a tag with an experimental pack merged in, or left empty', () => {
        const base = new Map([['planks', { blocks: ['oak_planks'], includes: [], replace: false }]]);
        const pack = new Map([['planks', { blocks: ['cherry_planks'], includes: [], replace: false }]]);
        const shipped = membershipHash({ blocks: ['oak_planks'], includes: [] });
        assert.notEqual(membershipHash(mergePacks([base, pack]).get('planks')), shipped);
        assert.notEqual(membershipHash({ blocks: [], includes: [] }), shipped);
        assert.throws(() => resolveTags(mergePacks([base, pack]), new Set(['oak_planks']), '1.19.4'), /holds cherry_planks/);
    });

    /** Runs generate-all's last stage over a copy of the audit changed by `change`: its status and what it said. */
    function generationOver(change) {
        const directory = scratchDirectory('mc-invariants-');
        const audit = generated('physics-methods.json');
        const sources = generated('minecraft-versions.json');
        change(audit, sources);
        fs.writeFileSync(path.join(directory, 'physics-methods.json'), JSON.stringify(audit));
        fs.writeFileSync(path.join(directory, 'minecraft-versions.json'), JSON.stringify(sources));
        return spawnSync(process.execPath, [path.join(root, 'scripts', 'verify-generation-invariants.cjs')], { env: { ...process.env, MC_GENERATED_ROOT: directory }, encoding: 'utf8' });
    }

    it('passes the last stage of generation on the real audit', () => {
        assert.equal(generationOver(() => {}).status, 0);
    });

    it('M30: a method fingerprint that hashes a call site fails generation', () => {
        const source = ['class Entity {', '    void caller() {', '        this.move(delta);', '    }', '', '    public void move(Vec3 delta) {', '        real();', '    }', '}'].join('\n');
        assert.equal(source.slice(extractMethod(source, 'move').start).startsWith('    public void move'), true);
        const run = generationOver(audit => {
            audit.releases['26.3'].files['Entity.java'].methods.move[0].source = '        this.move(MoverType.SELF, delta);\n';
        });
        assert.equal(run.status, 1);
        assert.match(run.stderr, /M30 26\.3 Entity\.java#move line \d+: the fingerprint is not of a declaration: this\.move/);
    });

    it('tells a declaration from a call', () => {
        assert.equal(isDeclaration('   public void move(final MoverType type, Vec3 delta) {', 'move'), true);
        assert.equal(isDeclaration('   protected abstract double getGravity() {', 'getGravity'), true);
        assert.equal(isDeclaration('   public <T> List<T> find(Class<T> type) {', 'find'), true);
        assert.equal(isDeclaration('   this.move(a, b);', 'move'), false);
        assert.equal(isDeclaration('   entity.move(a);\n   if (x) {', 'move'), false);
        assert.equal(isDeclaration('   return move(a);', 'move'), false);
        assert.equal(isDeclaration('   void other() {', 'move'), false);
    });

    it('M31: LocalPlayer lost from a release, or read from the wrong package, fails generation', () => {
        const moved = generationOver((_, sources) => {
            sources.sourceFiles['26.3'].files.find(file => file.path.endsWith('LocalPlayer.java')).path = 'net/minecraft/world/entity/player/LocalPlayer.java';
        });
        assert.equal(moved.status, 1);
        assert.match(moved.stderr, /M31 26\.3: LocalPlayer is at net\/minecraft\/world\/entity\/player/);
        const absent = generationOver((_, sources) => {
            sources.sourceFiles['1.20.4'].files.find(file => file.path.endsWith('LocalPlayer.java')).present = false;
        });
        assert.equal(absent.status, 1);
        assert.match(absent.stderr, /M31 1\.20\.4: LocalPlayer is not in the source read/);
        const noTick = generationOver(audit => {
            delete audit.releases['1.21.4'].files['LocalPlayer.java'].methods.aiStep;
        });
        assert.equal(noTick.status, 1);
        assert.match(noTick.stderr, /M31 1\.21\.4: the audit has no LocalPlayer\.aiStep/);
    });

    it('M31: LocalPlayer read from anywhere but its client package', () => {
        const { optionalFiles, sourceFiles, versions } = generated('minecraft-versions.json');
        assert.ok(optionalFiles.includes('net/minecraft/client/player/LocalPlayer.java'));
        assert.ok(!optionalFiles.includes('net/minecraft/world/entity/player/LocalPlayer.java'));
        for (const version of versions) {
            const local = sourceFiles[version].files.find(file => file.path.endsWith('LocalPlayer.java'));
            assert.equal(local.present, true, version);
        }
    });

    it('M32: a snapshot taken for the release its branch is named after', () => {
        const tree = scratchDirectory('mc-identity-');
        fs.mkdirSync(path.join(tree, 'client'));
        fs.writeFileSync(path.join(tree, 'client', 'version.json'), JSON.stringify({ id: '20w45a' }));
        assert.equal(directoryIdentity(tree), '20w45a');
        assert.notEqual(directoryIdentity(tree), '1.16.4');
        assert.equal(manifest['1.16.4'].source.embedded, '1.16.4');
    });
});

describe('a failed generation', () => {
    function workspace() {
        const directory = scratchDirectory('mc-stage-');
        const folder = path.join(directory, 'artifacts');
        fs.mkdirSync(folder);
        fs.writeFileSync(path.join(folder, 'table.json'), '{"complete":true}\n');
        const stage = (name, body) => fs.writeFileSync(path.join(directory, name), body);
        stage('writes.cjs', `require('fs').writeFileSync(${JSON.stringify(path.join(folder, 'table.json'))}, '{"partial":true}\\n');`);
        stage('adds.cjs', `require('fs').writeFileSync(${JSON.stringify(path.join(folder, 'stray.json'))}, '{}');`);
        stage('fails.cjs', 'process.exit(3);');
        stage('passes.cjs', '');
        return { directory, folder, read: () => fs.readFileSync(path.join(folder, 'table.json'), 'utf8') };
    }

    it('leaves the previous artifacts in place when a later stage fails', () => {
        const { directory, folder, read } = workspace();
        const { failed } = runStages({ stages: ['writes.cjs', 'adds.cjs', 'fails.cjs'], folders: [folder], directory, cwd: directory });
        assert.equal(failed, 'fails.cjs');
        assert.equal(read(), '{"complete":true}\n');
        assert.deepEqual(fs.readdirSync(folder), ['table.json']);
    });

    it('publishes all of it when every stage passes', () => {
        const { directory, folder, read } = workspace();
        assert.equal(runStages({ stages: ['writes.cjs', 'passes.cjs'], folders: [folder], directory, cwd: directory }).failed, null);
        assert.equal(read(), '{"partial":true}\n');
    });

    it('a run killed outright leaves a journal, and the next run puts the previous artifacts back first', () => {
        const { directory, folder, read } = workspace();
        const journal = path.join(directory, 'journal.json');
        // a stage that writes half a table and then kills the run itself, TerminateProcess on Windows: nothing in the
        // run lives to restore anything
        fs.writeFileSync(path.join(directory, 'killer.cjs'), `require('fs').writeFileSync(${JSON.stringify(path.join(folder, 'table.json'))}, '{"partial":true}\\n'); process.kill(process.ppid, 'SIGKILL');`);
        const runner = path.join(directory, 'runner.cjs');
        fs.writeFileSync(runner, `require(${JSON.stringify(path.join(__dirname, '..', 'scripts', 'generate-all.cjs'))}).runStages(${JSON.stringify({ stages: ['adds.cjs', 'killer.cjs'], folders: [folder], directory, cwd: directory, journal })});`);
        const killed = spawnSync(process.execPath, [runner]);
        assert.notEqual(killed.status, 0);
        assert.equal(read(), '{"partial":true}\n');
        assert.ok(fs.existsSync(journal), 'the killed run left its journal');
        const { failed, recovered } = runStages({ stages: ['fails.cjs'], folders: [folder], directory, cwd: directory, journal });
        assert.equal(recovered, true);
        assert.equal(failed, 'fails.cjs');
        assert.equal(read(), '{"complete":true}\n');
        assert.deepEqual(fs.readdirSync(folder), ['table.json']);
        assert.equal(fs.existsSync(journal), false);
    });

    it('recover alone restores a killed run and says when there was none', () => {
        const { folder, directory, read } = workspace();
        const journal = path.join(directory, 'journal.json');
        const snapshot = scratchDirectory('mc-generated-');
        fs.cpSync(folder, path.join(snapshot, '0'), { recursive: true });
        fs.writeFileSync(path.join(folder, 'table.json'), '{"partial":true}\n');
        fs.writeFileSync(journal, JSON.stringify({ snapshot, copies: [[folder, path.join(snapshot, '0')]] }));
        assert.equal(recover(journal), true);
        assert.equal(read(), '{"complete":true}\n');
        assert.equal(fs.existsSync(snapshot), false);
        assert.equal(recover(journal), false);
    });

    it('a check keeps nothing, and names what drifted, added files too', () => {
        const { directory, folder, read } = workspace();
        const { drift } = runStages({ stages: ['writes.cjs', 'adds.cjs'], folders: [folder], check: true, directory, cwd: directory });
        assert.deepEqual(drift.sort(), ['stray.json', 'table.json']);
        assert.equal(read(), '{"complete":true}\n');
    });

    it('fails a fresh legacy run with no tree rather than keeping old entries, and keeps them only if told to', () => {
        const directory = scratchDirectory('mc-legacy-');
        const manifest = path.join(directory, 'legacy.json');
        fs.copyFileSync(path.join(root, 'generated', 'legacy-release-data.json'), manifest);
        const environment = { ...process.env, MC_LEGACY_SOURCES: path.join(directory, 'no-such-tree'), MC_LEGACY_RELEASE_MANIFEST: manifest };
        const script = path.join(root, 'scripts', 'generate-legacy-release-manifest.cjs');
        const fresh = spawnSync(process.execPath, [script], { env: environment, encoding: 'utf8' });
        assert.notEqual(fresh.status, 0);
        assert.match(fresh.stderr, /Missing legacy source tree/);
        assert.equal(spawnSync(process.execPath, [script, '--cached'], { env: environment, encoding: 'utf8' }).status, 0);
    });

    it('fails a fresh tag or shape run with no jar rather than keeping old entries', () => {
        const environment = { ...process.env, MC_CLIENT_JAR_ROOT: path.join(os.tmpdir(), 'no-such-jars') };
        for (const script of ['generate-block-tags.cjs', 'generate-block-shapes.cjs']) {
            const before = fs.readFileSync(path.join(root, 'generated', 'block-tags.json'));
            const run = spawnSync(process.execPath, [path.join(root, 'scripts', script), '--check'], { env: environment, encoding: 'utf8' });
            assert.notEqual(run.status, 0, script);
            assert.match(run.stderr, /No client jar/, script);
            assert.deepEqual(fs.readFileSync(path.join(root, 'generated', 'block-tags.json')), before, script);
        }
    });
});

describe('where an entry comes from (B047)', () => {
    const generatedRoot = path.join(root, 'generated');
    const chain = (kind, release, key) => explain(kind, release, key, generatedRoot);
    const hashed = /^[0-9a-f]{40,64}$/;

    it('traces each kind of entry to a hashed input and the generator that read it', () => {
        for (const [kind, key, release] of [['block', 'oak_door', '26.3'], ['property', 'oak_door|OPEN', '26.3'], ['tag', 'doors', '26.3'], ['shape', 'snow|outline|3', '26.3'], ['friction', 'blue_ice', '26.3'], ['block', 'acacia_door', '1.12.2']]) {
            const { steps } = chain(kind, release, key);
            assert.ok(steps.length > 0, kind);
            assert.ok(steps.some(({ sha256 }) => hashed.test(sha256)), kind);
            for (const { generator, input } of steps) assert.ok(generator && input, kind);
        }
    });

    it('follows a tag through the tags it includes, to the block', () => {
        const direct = chain('tag', '26.3', 'doors|iron_door');
        assert.ok(direct.steps.at(-1).detail.includes('iron_door through doors'));
        const nested = chain('tag', '26.3', 'doors|oak_door');
        assert.ok(nested.steps.length >= 3, 'wooden_doors, then doors');
        assert.ok(nested.steps.some(step => step.input.includes('#wooden_doors')));
        assert.throws(() => chain('tag', '26.3', 'doors|stone'), /stone is not in #doors/);
    });

    it('says a tag no pack fills is empty by the data, and that an old release has none', () => {
        assert.match(chain('tag', '26.3', 'consumable_does_not_teleport_to').steps[0].detail, /empty by the data/);
        assert.throws(() => chain('tag', '1.12.2', 'doors'), /has no tags/);
    });

    it('gives a constant its releases and its type', () => {
        assert.match(chain('constant', '26.3', '0.21600002').steps[0].detail, /float in the sources of 1\.14\.4 to 26\.3/);
    });

    it('keeps whether the source is the release\'s apart from whether the engine matches it', () => {
        const { identity, behaviour } = chain('block', '26.3', 'oak_door');
        assert.deepEqual([identity.status, identity.embedded], ['embedded', '26.3']);
        assert.match(behaviour, /not judged here/);
        // (1.8.8 is read from its own jar since its decompile is pinned: no longer 1.8.9's alias)
        assert.deepEqual([chain('block', '1.8.8', 'acacia_door').identity.status, chain('block', '1.8.8', 'acacia_door').identity.embedded], ['embedded', '1.8.8']);
    });

    it('refuses an entry that does not exist and a release that is not registered', () => {
        assert.throws(() => chain('block', '26.3', 'no_such_block'), /not a block/);
        assert.throws(() => chain('block', '20w45a', 'stone'), /not a registered release/);
        assert.throws(() => chain('nonsense', '26.3', 'x'), /Unknown kind/);
    });

    /** A copy of the generated folder and the scripts, with `change` applied: what `--check` says of it. */
    function checkOver(change) {
        const directory = scratchDirectory('mc-provenance-');
        const copied = path.join(directory, 'generated');
        const scripts = path.join(directory, 'scripts');
        fs.mkdirSync(scripts);
        fs.mkdirSync(path.join(copied, 'block-shapes'), { recursive: true });
        for (const name of [...Object.keys(OUTPUTS), 'provenance.json']) fs.copyFileSync(path.join(generatedRoot, name), path.join(copied, name));
        for (const name of ['26.3.json.gz', '1.14.4.json.gz']) fs.copyFileSync(path.join(generatedRoot, 'block-shapes', name), path.join(copied, 'block-shapes', name));
        const record = JSON.parse(fs.readFileSync(path.join(copied, 'provenance.json'), 'utf8'));
        record.shapeTables = { '26.3.json.gz': record.shapeTables['26.3.json.gz'], '1.14.4.json.gz': record.shapeTables['1.14.4.json.gz'] };
        fs.writeFileSync(path.join(copied, 'provenance.json'), JSON.stringify(record));
        for (const name of new Set(Object.values(OUTPUTS))) fs.copyFileSync(path.join(root, 'scripts', name), path.join(scripts, name));
        fs.mkdirSync(path.join(scripts, 'java-oracle'));
        for (const name of ORACLES) fs.copyFileSync(path.join(root, 'scripts', name), path.join(scripts, name));
        change(copied, scripts);
        // (a stalled child fails this test: without the timeout it holds the whole gate; the check takes well under a second)
        const run = spawnSync(process.execPath, [path.join(root, 'scripts', 'provenance.cjs'), '--check'], { env: { ...process.env, MC_GENERATED_ROOT: copied, MC_GENERATOR_ROOT: scripts }, encoding: 'utf8', timeout: 60_000 });
        if (run.error) throw new Error(`provenance --check did not finish: ${run.error.message}`);
        return run;
    }

    it('passes the checked-in files', () => {
        const run = checkOver(() => {});
        assert.equal(run.status, 0, run.stderr);
    });

    it('finds an edited table, without a timestamp', () => {
        const run = checkOver((copied) => {
            const file = path.join(copied, 'block-tags.json');
            const stamp = fs.statSync(file).mtime;
            fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('"tags": true', '"tags": false'));
            fs.utimesSync(file, stamp, stamp);
        });
        assert.equal(run.status, 1);
        assert.match(run.stderr, /block-tags\.json is not what provenance recorded/);
    });

    it('finds a generator changed since its output, and a lost table', () => {
        const changed = checkOver((_, scripts) => fs.appendFileSync(path.join(scripts, 'generate-block-tags.cjs'), '\n// changed\n'));
        assert.equal(changed.status, 1);
        assert.match(changed.stderr, /generate-block-tags\.cjs changed since its output was generated/);
        const lost = checkOver(copied => fs.rmSync(path.join(copied, 'block-shapes', '1.14.4.json.gz')));
        assert.equal(lost.status, 1);
        assert.match(lost.stderr, /block-shapes\/1\.14\.4\.json\.gz is gone/);
    });

    it('finds an oracle changed since its table: its answers are the table\'s', () => {
        const run = checkOver((_, scripts) => fs.appendFileSync(path.join(scripts, 'java-oracle', 'ShapeDump.java'), '\n// changed\n'));
        assert.equal(run.status, 1);
        assert.match(run.stderr, /java-oracle\/ShapeDump\.java changed since its output was generated/);
    });

    it('takes no notice of line endings, which git decides', () => {
        const run = checkOver((copied) => {
            const file = path.join(copied, 'block-tags.json');
            fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replaceAll('\n', '\r\n'));
        });
        assert.equal(run.status, 0, run.stderr);
    });

    it('regenerates to the same bytes', () => {
        assert.deepEqual(inventory(generatedRoot, path.join(root, 'scripts')), JSON.parse(fs.readFileSync(path.join(generatedRoot, 'provenance.json'), 'utf8')));
    });
});

describe('the integrity report', () => {
    const report = integrityReport();

    it('keeps structure, provenance and behaviour apart', () => {
        assert.deepEqual(Object.keys(report).sort(), ['behaviour', 'provenance', 'releases', 'structure', 'unresolved']);
    });

    it('accounts for all 50 releases, cached ones named as cached', () => {
        assert.equal(report.releases, 50);
        assert.ok(report.provenance.shapes.exact.length >= 44);
        assert.deepEqual(report.provenance.shapes.alone, ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']);
        assert.deepEqual(report.provenance.shapes.unavailable, []);
    });

    it('never calls a release behaviourally verified from generated data alone', () => {
        assert.equal(report.behaviour.status, 'not-judged-here');
        assert.equal(report.behaviour.verified.length, 0);
    });

    it('lists what is unresolved, no release by leaving it out', () => {
        const named = report.unresolved.flatMap(entry => entry.releases);
        assert.ok(named.includes('1.8.8'));
        for (const entry of report.unresolved) assert.ok(entry.what && entry.releases.length > 0);
    });
});
