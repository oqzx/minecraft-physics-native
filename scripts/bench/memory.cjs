'use strict';

// node --expose-gc scripts/bench/memory.cjs [--releases a,b] [--workloads a,b] [--scale n] [--out file]
// Each workload runs in rounds with a full GC between them, and reports the JavaScript heap, external memory, RSS and the
// sync's caches at every round. `slopeKbPerRound` is over the rounds after warmup: 0 within RSS noise is a stable workload.

const { spawnSync } = require('node:child_process');
const { join } = require('node:path');
const inspector = require('node:inspector');
const { ROOT, RELEASES, REGION, SCENES, arg, binding, cpuMs, identity, physics, playerState, registryFor, rounded, sceneWorld, sourceOf, writeJson } = require('./lib.cjs');

if (!global.gc) throw new Error('run with --expose-gc: a heap between GCs says nothing of what is kept');

const scale = Number(arg('scale', 1));
const releases = arg('releases', '1.12.2,1.20.4,26.3').split(',');
const ROUNDS = 12;
const WARMUP = 4;

/**
 * A round's end. The turn of the event loop comes first and matters: a WeakRef made in a job holds its target to the end of
 * that job, so a workload that never yields sees every world it dropped as retained, which no bot's ticks do.
 */
async function snapshot(extra) {
    await new Promise(resolve => setImmediate(resolve));
    global.gc();
    await new Promise(resolve => setImmediate(resolve));
    global.gc();
    const kb = bytes => Math.round(bytes / 1024);
    const { heapUsed, external, arrayBuffers, rss } = process.memoryUsage();
    return { heapKb: kb(heapUsed), externalKb: kb(external), arrayBuffersKb: kb(arrayBuffers), rssKb: kb(rss), ...extra };
}

function slope(rows, field) {
    const kept = rows.slice(WARMUP);
    const count = kept.length;
    const meanX = (count - 1) / 2;
    const meanY = kept.reduce((sum, row) => sum + row[field], 0) / count;
    const top = kept.reduce((sum, row, index) => sum + (index - meanX) * (row[field] - meanY), 0);
    const bottom = kept.reduce((sum, _, index) => sum + (index - meanX) ** 2, 0);
    return rounded(top / bottom);
}

function report(rows) {
    return {
        rounds: rows,
        slopeKbPerRound: { heap: slope(rows, 'heapKb'), external: slope(rows, 'externalKb'), rss: slope(rows, 'rssKb') },
        rangeAfterWarmupKb: Object.fromEntries(['heapKb', 'externalKb', 'rssKb'].map(field => {
            const values = rows.slice(WARMUP).map(row => row[field]);
            return [field, [Math.min(...values), Math.max(...values)]];
        })),
    };
}

const cachesOf = sync => ({ ids: sync.ids.size, present: sync.present.size, current: sync.current.size, loadedChunks: sync.loadedChunks.size, described: sync.described.size });

const WORKLOADS = {
    /** One bot ticking in place: nothing grows that a tick does not replace. */
    async stationary(version) {
        const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
        const { botOf } = require(join(ROOT, 'test/bot-fixture.cjs'));
        const registry = registryFor(version);
        const source = sourceOf(version, SCENES.dense(registry));
        const bot = botOf(registry.version.minecraftVersion, { world: source });
        const engine = new physics.NativePhysics(registry, { version });
        const ctx = EPhysicsCtx.FROM_BOT(engine, bot);
        const rows = [];
        for (let round = 0; round < ROUNDS; round += 1) {
            for (let tick = 0; tick < 2000 * scale; tick += 1) {
                ctx.state.pos.set(0.5, 64, 0.5);
                ctx.state.vel.set(0, 0, 0);
                engine.simulate(ctx, source);
            }
            rows.push(await snapshot({ listeners: source.eventNames().reduce((sum, name) => sum + source.listenerCount(name), 0) }));
        }
        return report(rows);
    },
    /** A window walking chunk after chunk: the cache limit is what stops this one. */
    async traversal(version) {
        const registry = registryFor(version);
        const source = sourceOf(version, SCENES.dense(registry));
        const sync = new physics.NativeWorldSync(binding, registry, version);
        const rows = [];
        let x = 0;
        for (let round = 0; round < ROUNDS; round += 1) {
            for (let step = 0; step < 200 * scale; step += 1) {
                x += 16;
                sync.sync(source, { minX: x, minY: 60, minZ: -16, maxX: x + 47, maxY: 68, maxZ: 31 }, 0);
            }
            rows.push(await snapshot({ ...cachesOf(sync), walkedBlocks: x, worldCheckpointKb: sync.world.checkpoint().length >> 10 }));
        }
        sync.dispose();
        return report(rows);
    },
    /** One cell changing over and over. */
    async churn(version) {
        const { source, sync } = sceneWorld(version, 'dense');
        const rows = [];
        const position = { x: 1, y: 64, z: 1 };
        for (let round = 0; round < ROUNDS; round += 1) {
            for (let step = 0; step < 20000 * scale; step += 1) {
                sync.blockChanged(position);
                sync.sync(source, REGION, 0);
            }
            rows.push(await snapshot({ ...cachesOf(sync), changed: sync.changed.size }));
        }
        sync.dispose();
        return report(rows);
    },
    /** A bot leaving one world for another, as a dimension change does. */
    async replacement(version) {
        const registry = registryFor(version);
        const cells = SCENES.empty(registry);
        const engine = new physics.NativePhysics(registry, { version });
        const box = { minX: -8, minY: 60, minZ: -8, maxX: 8, maxY: 68, maxZ: 8 };
        const rows = [];
        let last;
        for (let round = 0; round < ROUNDS; round += 1) {
            for (let step = 0; step < 100 * scale; step += 1) {
                last = sourceOf(version, cells);
                engine.syncOf(last).sync(last, box, 0);
            }
            rows.push(await snapshot({ listenersOnLastWorld: last.eventNames().reduce((sum, name) => sum + last.listenerCount(name), 0) }));
        }
        engine.dispose();
        return report(rows);
    },
    /** A crowd in batches: what a batch allocates ends with it. */
    async entities(version) {
        const { world } = sceneWorld(version, 'empty');
        const states = Array.from({ length: 256 }, (_, i) => playerState('empty', (i % 16) * 0.7));
        const rows = [];
        for (let round = 0; round < ROUNDS; round += 1) {
            for (let step = 0; step < 50 * scale; step += 1) binding.simulateVersionedTicks(version, world, states, 20, {});
            rows.push(await snapshot({}));
        }
        return report(rows);
    },
    /** Engines and syncs made and disposed. */
    async construction(version) {
        const registry = registryFor(version);
        const source = sourceOf(version, SCENES.empty(registry));
        const box = { minX: -8, minY: 60, minZ: -8, maxX: 8, maxY: 68, maxZ: 8 };
        const rows = [];
        for (let round = 0; round < ROUNDS; round += 1) {
            for (let step = 0; step < 100 * scale; step += 1) {
                const engine = new physics.NativePhysics(registry, { version });
                engine.syncOf(source).sync(source, box, 0);
                engine.dispose();
            }
            rows.push(await snapshot({ listeners: source.eventNames().reduce((sum, name) => sum + source.listenerCount(name), 0) }));
        }
        return report(rows);
    },
};

/** The functions that allocate most JavaScript memory in a warm adapter tick. */
function allocationProfile(version) {
    const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
    const { botOf } = require(join(ROOT, 'test/bot-fixture.cjs'));
    const registry = registryFor(version);
    const source = sourceOf(version, SCENES.dense(registry));
    const bot = botOf(registry.version.minecraftVersion, { world: source });
    const engine = new physics.NativePhysics(registry, { version });
    const ctx = EPhysicsCtx.FROM_BOT(engine, bot);
    const tick = () => {
        ctx.state.pos.set(0.5, 64, 0.5);
        ctx.state.vel.set(0, 0, 0);
        engine.simulate(ctx, source);
    };
    for (let i = 0; i < 500; i += 1) tick();
    const session = new inspector.Session();
    session.connect();
    return new Promise((resolve, reject) => {
        session.post('HeapProfiler.startSampling', { samplingInterval: 4096, includeObjectsCollectedByMajorGC: true, includeObjectsCollectedByMinorGC: true }, startError => {
            if (startError) return reject(startError);
            const ticks = 5000;
            for (let i = 0; i < ticks; i += 1) tick();
            return session.post('HeapProfiler.stopSampling', (stopError, { profile }) => {
                session.disconnect();
                if (stopError) return reject(stopError);
                const byFunction = new Map();
                const walk = node => {
                    const name = `${node.callFrame.functionName || '(anonymous)'} ${node.callFrame.url.split(/[\\/]/).pop()}:${node.callFrame.lineNumber + 1}`;
                    byFunction.set(name, (byFunction.get(name) ?? 0) + node.selfSize);
                    node.children.forEach(walk);
                };
                walk(profile.head);
                const total = [...byFunction.values()].reduce((sum, value) => sum + value, 0);
                return resolve({
                    ticks,
                    bytesPerTick: rounded(total / ticks),
                    top: [...byFunction].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([name, bytes]) => ({ name, bytesPerTick: rounded(bytes / ticks) })),
                });
            });
        });
    });
}

(async () => {
    const one = arg('one');
    if (one) {
        const [version, name] = one.split('/');
        const result = name === 'allocationProfile' ? await allocationProfile(version) : await WORKLOADS[name](version);
        process.stdout.write(JSON.stringify(result));
        return;
    }
    // a process for each: what a workload leaves in the heap and the allocator is no part of the next one's numbers
    const started = cpuMs();
    const names = arg('workloads', Object.keys(WORKLOADS).join(',')).split(',');
    const results = {};
    for (const version of releases) {
        if (!RELEASES.includes(version)) throw new Error(`${version} is not registered`);
        results[version] = {};
        for (const name of [...names, 'allocationProfile']) {
            const child = spawnSync(process.execPath, ['--expose-gc', __filename, '--one', `${version}/${name}`, '--scale', String(scale)], { encoding: 'utf8', maxBuffer: 1 << 26 });
            if (child.status !== 0) throw new Error(child.stderr);
            results[version][name] = JSON.parse(child.stdout);
            process.stderr.write(`${version} ${name}
`);
        }
    }
    const out = arg('out', join(ROOT, `docs/benchmarks/memory-${process.platform}.json`));
    writeJson(out, { suite: 'memory', identity: identity(), scale, flags: '--expose-gc', allocator: 'system (no global allocator is set in the addon)', results, suiteCpuSeconds: rounded((cpuMs() - started) / 1000) });
    process.stderr.write(`wrote ${out}
`);
})();
