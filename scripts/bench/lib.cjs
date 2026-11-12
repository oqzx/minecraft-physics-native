'use strict';

// What every benchmark here shares: the run's identity, CPU-time sampling and the scenes. CPU time and counts are the
// numbers to read: this machine is shared, and the wall clock moves with whatever else it runs.

const { execFileSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } = require('node:fs');
const os = require('node:os');
const { dirname, join } = require('node:path');

const ROOT = join(__dirname, '..', '..');
const physics = require(join(ROOT, 'dist'));
const { compareReleases } = require(join(ROOT, 'dist/mineflayer/blocks'));
const registryOf = require('prismarine-registry');

const binding = physics.LoadNativePhysics();
const RELEASES = Object.keys(physics.NATIVE_PHYSICS_ENGINES).sort(compareReleases);

function sha256(data) {
    return createHash('sha256').update(data).digest('hex');
}

function tool(command, args) {
    try {
        return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], cwd: ROOT }).trim();
    } catch {
        return null;
    }
}

function filesUnder(dir) {
    return readdirSync(dir, { withFileTypes: true })
        .flatMap(entry => (entry.isDirectory() ? filesUnder(join(dir, entry.name)) : [join(dir, entry.name)]))
        .sort();
}

/** What a result was measured on and of: the same hashes mean the same engine and data. */
function identity() {
    const dataHash = createHash('sha256');
    for (const file of filesUnder(join(ROOT, 'native/data'))) dataHash.update(readFileSync(file));
    const artifacts = join(ROOT, 'native/artifacts');
    const addon = join(artifacts, readdirSync(artifacts).find(name => name.endsWith('.node')));
    const cargo = readFileSync(join(ROOT, 'native/Cargo.toml'), 'utf8');
    return {
        date: new Date().toISOString(),
        commit: tool('git', ['rev-parse', 'HEAD']),
        dirty: (tool('git', ['status', '--porcelain', '--', 'src', 'native/src', 'native/data']) ?? '') !== '',
        platform: `${process.platform}-${process.arch}`,
        cpu: os.cpus()[0]?.model ?? null,
        logicalCpus: os.cpus().length,
        totalMemoryMb: Math.round(os.totalmem() / 1048576),
        node: process.version,
        rustc: tool('rustc', ['-V']),
        buildProfile: cargo.slice(cargo.indexOf('[profile.release]')).trim().replace(/\r?\n/g, '; '),
        addonBytes: statSync(addon).size,
        addonSha256: sha256(readFileSync(addon)),
        generatedDataSha256: dataHash.digest('hex'),
        clock: 'process.cpuUsage (user + system, all threads; about 15.6 ms granularity on Windows) and process.hrtime',
    };
}

function cpuMs() {
    const { user, system } = process.cpuUsage();
    return (user + system) / 1000;
}

function wallMs() {
    return Number(process.hrtime.bigint()) / 1e6;
}

function sample(fn, loops) {
    const [cpu, wall] = [cpuMs(), wallMs()];
    for (let i = 0; i < loops; i += 1) fn();
    return { cpuMs: cpuMs() - cpu, wallMs: wallMs() - wall };
}

function median(values) {
    const sorted = [...values].sort((a, b) => a - b);
    const middle = sorted.length >> 1;
    return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

const rounded = value => Number(value.toPrecision(5));

/**
 * The CPU time of `fn`: enough loops to fill `minCpuMs` a repetition, far above the clock's grain, `warmup` repetitions
 * thrown away, then `reps` kept, all of them in the result. `spread` is (max - min) / median of the kept CPU times.
 */
function measure(fn, { minCpuMs = 120, reps = 5, warmup = 2, units = 1 } = {}) {
    // the wall clock sizes the loops: it is fine-grained, where the CPU clock of a short sample reads 0 or a whole 15.6 ms
    let loops = 1;
    for (;;) {
        const { wallMs: spent } = sample(fn, loops);
        if (spent >= minCpuMs / 4 || loops > 1e8) {
            loops = Math.ceil(loops * (minCpuMs / spent));
            break;
        }
        loops *= Math.max(2, Math.min(16, Math.ceil(minCpuMs / 4 / Math.max(spent, 0.001))));
    }
    for (let i = 0; i < warmup; i += 1) sample(fn, loops);
    const samples = [];
    for (let i = 0; i < reps; i += 1) samples.push(sample(fn, loops));
    const perUnit = samples.map(run => (run.cpuMs * 1000) / (loops * units));
    const wallPerUnit = samples.map(run => (run.wallMs * 1000) / (loops * units));
    const cpuMedian = median(perUnit);
    return {
        cpuUs: rounded(cpuMedian),
        minCpuUs: rounded(Math.min(...perUnit)),
        maxCpuUs: rounded(Math.max(...perUnit)),
        spread: rounded(cpuMedian > 0 ? (Math.max(...perUnit) - Math.min(...perUnit)) / cpuMedian : 0),
        // wall over CPU above 1 is a thread that waited: a sign of the machine's load, and no part of the result
        wallPerCpu: rounded(median(wallPerUnit) / cpuMedian),
        loops,
        rawCpuUs: perUnit.map(rounded),
    };
}

/** The wall clock of each of `calls` single calls: latency, which a loaded machine moves, so it is labelled so. */
function latency(fn, calls = 2000) {
    const times = new Float64Array(calls);
    for (let i = 0; i < calls; i += 1) {
        const start = process.hrtime.bigint();
        fn(i);
        times[i] = Number(process.hrtime.bigint() - start) / 1000;
    }
    times.sort();
    const at = fraction => rounded(times[Math.min(calls - 1, Math.floor(calls * fraction))]);
    return { wallUsP50: at(0.5), wallUsP99: at(0.99), wallUsMax: rounded(times[calls - 1]), calls, loadSensitive: true };
}

function writeJson(path, value) {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, `${JSON.stringify(value, null, 1)}\n`);
}

function arg(name, fallback) {
    const at = process.argv.indexOf(`--${name}`);
    return at < 0 ? fallback : process.argv[at + 1];
}

/** The bits of the numbers of a value, in the order they are met: a fingerprint of an outcome. */
function fingerprint(values) {
    const hash = createHash('sha256');
    const number = new Float64Array(1);
    for (const value of values) {
        number[0] = value;
        hash.update(Buffer.from(number.buffer));
    }
    return hash.digest('hex').slice(0, 16);
}

const registries = new Map();
function registryFor(version) {
    if (!registries.has(version)) {
        const found = registryOf(version);
        // minecraft-data answers a release it lacks with the nearest table, under that table's name but this one's comparison
        registries.set(version, found.version.minecraftVersion === version ? found : registryOf(found.version.minecraftVersion));
    }
    return registries.get(version);
}

const stateOf = block => block.defaultState ?? block.id << 4;

/** The first of the names the registry has: names that change between releases, one block in each. */
function blockOf(registry, ...names) {
    for (const name of names) if (registry.blocksByName[name]) return stateOf(registry.blocksByName[name]);
    throw new Error(`no block of ${names.join(', ')} in ${registry.version.minecraftVersion}`);
}

const FLOOR_Y = 63;

/** The block of each cell of a scene, a pure function of the cell. */
const SCENES = {
    empty: registry => {
        const stone = blockOf(registry, 'stone');
        const air = blockOf(registry, 'air');
        return (x, y) => (y === FLOOR_Y ? stone : air);
    },
    dense: registry => {
        const stone = blockOf(registry, 'stone');
        const air = blockOf(registry, 'air');
        const shapes = [
            blockOf(registry, 'oak_stairs'), blockOf(registry, 'oak_slab', 'wooden_slab'), blockOf(registry, 'oak_fence', 'fence'),
            blockOf(registry, 'cobblestone_wall'), blockOf(registry, 'iron_bars'), blockOf(registry, 'stone_slab'),
        ];
        return (x, y, z) => {
            if (y === FLOOR_Y) return stone;
            if (y > FLOOR_Y && y <= FLOOR_Y + 2 && ((x + z * 3) & 3) !== 0) return shapes[(((x * 7 + z * 13 + y) % 6) + 6) % 6];
            return air;
        };
    },
    fluid: registry => {
        const stone = blockOf(registry, 'stone');
        const air = blockOf(registry, 'air');
        const water = blockOf(registry, 'water');
        return (x, y) => (y === FLOOR_Y - 8 ? stone : y > FLOOR_Y - 8 && y <= FLOOR_Y + 1 ? water : air);
    },
};

/** A prismarine world over a scene's cells, as much of one as NativeWorldSync reads. */
function sourceOf(version, cells) {
    const { EventEmitter } = require('node:events');
    const registry = registryFor(version);
    const Block = require('prismarine-block')(registry);
    return Object.assign(new EventEmitter(), {
        getBlockStateId: pos => cells(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)),
        getBlock: pos => Object.assign(Block.fromStateId(cells(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)), 0), { position: pos }),
    });
}

/** The box a run of a scene stays in. */
const REGION = { minX: -24, minY: 54, minZ: -24, maxX: 72, maxY: 72, maxZ: 24 };

function sceneWorld(version, scene, region = REGION) {
    const registry = registryFor(version);
    const source = sourceOf(version, SCENES[scene](registry));
    const sync = new physics.NativeWorldSync(binding, registry, worldRelease(version));
    sync.sync(source, region, 0);
    return { registry, source, sync, world: sync.world };
}

/** The release whose data builds a scene: its own, or the nearest table minecraft-data has for a release it lacks. */
function worldRelease(version) {
    return registryFor(version).version.minecraftVersion;
}

/** A walking player at the scene's start: `offset` keeps a crowd apart or, at 0, in one place. */
function playerState(scene, offset = 0) {
    const y = scene === 'fluid' ? FLOOR_Y : FLOOR_Y + 1;
    return {
        position: { x: 0.5 + offset, y, z: 0.5 },
        velocity: { x: 0, y: 0, z: 0 },
        yaw: 0, pitch: 0,
        look: { x: 0, y: 0, z: 1 },
        halfWidth: 0.3, height: 1.8,
        eyeHeight: 1.62,
        onGround: scene !== 'fluid',
        collidedHorizontally: false, collidedVertically: false,
        input: { x: 0, y: 0, z: 1 },
        sprinting: scene === 'empty',
        sprintModifier: scene === 'empty',
        isPlayer: true,
        clientSide: true,
        inWater: scene === 'fluid',
        jumpRequested: scene === 'dense',
    };
}

const isMain = module === require.main;
if (isMain) throw new Error('scripts/bench/lib.cjs is a library: run scripts/bench/run.cjs');

module.exports = {
    ROOT, RELEASES, binding, physics, FLOOR_Y, REGION, SCENES,
    arg, blockOf, compareReleases, worldRelease, cpuMs, fingerprint, identity, latency, measure, median, playerState, registryFor, rounded, sceneWorld, sha256, sourceOf, wallMs, writeJson,
};
