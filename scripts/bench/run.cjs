'use strict';

// node scripts/bench/run.cjs --suite ticks|overhead|optimizations [--releases a,b] [--out file] [--min-cpu-ms n]
// Every timed path first proves it gives the same outcome as its baseline, to the bit: a faster wrong answer stops the run.
// See docs/benchmarks.md for what each number means and which of them a loaded machine moves.

const assert = require('node:assert/strict');
const { join } = require('node:path');
const {
    ROOT, RELEASES, binding, physics, REGION, arg, cpuMs, fingerprint, identity, latency, measure, playerState, registryFor,
    rounded, sceneWorld, sourceOf, SCENES, writeJson,
} = require('./lib.cjs');

const { NativePlayerInput: In, NativePlayerInputFlag: InFlag, NativePlayerOutput: Out, NativePlayerOutputFlag: OutFlag } = physics;

const suite = arg('suite', 'ticks');
const minCpuMs = Number(arg('min-cpu-ms', 120));
const options = { minCpuMs };
const REPRESENTATIVE = ['1.12.2', '1.16.5', '1.20.4', '26.3'];
const releases = arg('releases', suite === 'ticks' ? RELEASES.join(',') : REPRESENTATIVE.join(',')).split(',');

const numbersOf = state => [state.position.x, state.position.y, state.position.z, state.velocity.x, state.velocity.y, state.velocity.z, state.onGround ? 1 : 0];
const clone = value => structuredClone(value);

function scalarRun(version, world, start, ticks) {
    let state = clone(start);
    for (let tick = 0; tick < ticks; tick += 1) {
        const jump = state.jumpRequested;
        state = binding.simulateVersionedTick(version, world, { ...state, input: start.input, jumpRequested: start.jumpRequested || jump }, {});
    }
    return state;
}

/** What a kept player reads of the scene's state, the rest of its input a walking player's constants. */
function firstInput(start) {
    const input = new Float64Array(In.LENGTH);
    input.set([start.position.x, start.position.y, start.position.z], In.POSITION);
    input[In.YAW] = 0;
    input[In.PITCH] = 0;
    input.set([start.look.x, start.look.y, start.look.z], In.LOOK);
    input[In.STRAFE] = start.input.x;
    input[In.FORWARD] = start.input.z;
    input[In.HALF_WIDTH] = start.halfWidth;
    input[In.HEIGHT] = start.height;
    input[In.EYE_HEIGHT] = start.eyeHeight;
    input[In.FLYING_SPEED] = Math.fround(0.05);
    input[In.ITEM_USE_SPEED_MULTIPLIER] = Math.fround(0.2);
    input[In.FLAGS] = (start.onGround ? InFlag.ON_GROUND : 0) | (start.sprinting ? InFlag.SPRINTING | InFlag.SPRINT_MODIFIER : 0)
        | (start.inWater ? InFlag.IN_WATER : 0) | (start.jumpRequested ? InFlag.JUMP_REQUESTED : 0);
    return input;
}

function persistentRunner(version, start) {
    const player = new binding.NativePlayer(version, clone(start), {});
    const input = firstInput(start);
    const none = new Float64Array(0);
    const carried = (input[In.FLAGS] & ~(InFlag.ON_GROUND | InFlag.IN_WATER | InFlag.COLLIDED_HORIZONTALLY | InFlag.COLLIDED_VERTICALLY))
        | InFlag.UNMOVED | (start.sprinting ? InFlag.WAS_SPRINTING : 0);
    return {
        player,
        tick(world) {
            const out = player.tick(world, input, none);
            input.set(out.subarray(Out.POSITION, Out.POSITION + 6), In.POSITION);
            const flags = out[Out.FLAGS];
            input[In.FLAGS] = carried | (flags & OutFlag.ON_GROUND ? InFlag.ON_GROUND : 0) | (flags & OutFlag.IN_WATER ? InFlag.IN_WATER : 0)
                | (flags & OutFlag.COLLIDED_HORIZONTALLY ? InFlag.COLLIDED_HORIZONTALLY : 0) | (flags & OutFlag.COLLIDED_VERTICALLY ? InFlag.COLLIDED_VERTICALLY : 0)
                | (flags & OutFlag.VERTICAL_COLLISION_BELOW ? InFlag.VERTICAL_COLLISION_BELOW : 0) | (flags & OutFlag.MINOR_HORIZONTAL_COLLISION ? InFlag.MINOR_HORIZONTAL_COLLISION : 0);
            input[In.FALL_DISTANCE] = out[Out.FALL_DISTANCE];
            input[In.JUMP_TICKS] = out[Out.JUMP_TICKS];
            input[In.AUTO_JUMP_TIME] = out[Out.AUTO_JUMP_TIME];
            input.set(out.subarray(Out.STUCK_SPEED_MULTIPLIER, Out.STUCK_SPEED_MULTIPLIER + 3), In.STUCK_SPEED_MULTIPLIER);
            return out;
        },
    };
}

function persistentRun(version, world, start, ticks) {
    const runner = persistentRunner(version, start);
    let out;
    for (let tick = 0; tick < ticks; tick += 1) out = runner.tick(world);
    return [...out.subarray(0, 6), out[Out.FLAGS] & OutFlag.ON_GROUND ? 1 : 0];
}

/** One player over the same ticks by each path: their outcomes must be one. */
function agreeingPaths(version, sceneName, ticks) {
    const { world } = sceneWorld(version, sceneName);
    const start = playerState(sceneName);
    const scalar = fingerprint(numbersOf(scalarRun(version, world, start, ticks)));
    const batch = fingerprint(numbersOf(binding.simulateVersionedTicks(version, world, [{ ...clone(start) }], ticks, {})[0]));
    const persistent = fingerprint(persistentRun(version, world, start, ticks));
    return { scalar, batch, persistent, world, start };
}

function boatOf(z = 0.5) {
    return {
        position: { x: 0.5, y: 64 - 0.4, z }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0,
        forward: true, left: false, right: false, inWater: true,
    };
}

function entityOf(sceneName, id) {
    return { simulation: { ...playerState(sceneName), isPlayer: false, sprinting: false }, noPhysics: false, id, pushable: true, pushthrough: 0 };
}

function ticksSuite() {
    const results = {};
    const TICKS = 200;
    for (const version of releases) {
        const row = (results[version] = {});
        for (const sceneName of Object.keys(SCENES)) {
            const { scalar, batch, persistent, world, start } = agreeingPaths(version, sceneName, TICKS);
            assert.equal(batch, scalar, `${version} ${sceneName}: a batch differs from single ticks`);
            assert.equal(persistent, scalar, `${version} ${sceneName}: a kept player differs from single ticks`);
            row[sceneName] = {
                outcome: scalar,
                scalar: measure(() => scalarRun(version, world, start, TICKS), { ...options, units: TICKS }),
                batch: measure(() => binding.simulateVersionedTicks(version, world, [start], TICKS, {}), { ...options, units: TICKS }),
                persistent: measure(() => persistentRun(version, world, start, TICKS), { ...options, units: TICKS }),
            };
        }
        const crowd = sceneWorld(version, 'empty');
        const pile = Array.from({ length: 8 }, (_, i) => entityOf('empty', i + 1));
        const piled = binding.simulateVersionedEntityTicks(version, crowd.world, pile, 20, {});
        const reversed = binding.simulateVersionedEntityTicks(version, crowd.world, [...pile].reverse(), 20, {}).reverse();
        row.interaction = {
            entities: 8,
            outcome: fingerprint(piled.flatMap(state => numbersOf(state.simulation))),
            orderMatters: fingerprint(reversed.flatMap(state => numbersOf(state.simulation))) !== fingerprint(piled.flatMap(state => numbersOf(state.simulation))),
            batch: measure(() => binding.simulateVersionedEntityTicks(version, crowd.world, pile, 20, {}), { ...options, units: 8 * 20 }),
        };
        const water = sceneWorld(version, 'fluid');
        try {
            const boats = [boatOf()];
            const [single] = binding.simulateVersionedBoatTicks(version, water.world, boats, 100);
            let stepped = boats[0];
            for (let tick = 0; tick < 100; tick += 1) [stepped] = binding.simulateVersionedBoatTicks(version, water.world, [stepped], 1);
            row.vehicle = { kind: 'boat', outcome: fingerprint(numbersOf({ ...single, onGround: single.onGround })), chunkedSame: fingerprint(numbersOf({ ...stepped, onGround: stepped.onGround })) === fingerprint(numbersOf({ ...single, onGround: single.onGround })) };
            row.vehicle.batch = measure(() => binding.simulateVersionedBoatTicks(version, water.world, boats, 100), { ...options, units: 100 });
        } catch (error) {
            row.vehicle = { unsupported: error.message };
        }
        process.stderr.write(`${version} `);
    }
    const scaling = {};
    for (const version of REPRESENTATIVE) {
        const { world } = sceneWorld(version, 'dense');
        const out = (scaling[version] = {});
        for (const entities of [1, 8, 64]) {
            const states = Array.from({ length: entities }, (_, i) => playerState('dense', (i % 8) * 1.1));
            for (const ticks of [0, 1, 20, 200]) {
                const units = entities * Math.max(ticks, 1);
                out[`${entities}x${ticks}`] = measure(() => binding.simulateVersionedTicks(version, world, states, ticks, {}), { ...options, units, reps: 3 });
            }
        }
        const crowd = sceneWorld(version, 'empty');
        out.interaction = {};
        for (const entities of [1, 4, 16, 64, 256]) {
            const pile = Array.from({ length: entities }, (_, i) => entityOf('empty', i + 1));
            out.interaction[entities] = measure(() => binding.simulateVersionedEntityTicks(version, crowd.world, pile, 4, {}), { ...options, units: entities * 4, reps: 3 });
        }
        const start = playerState('dense');
        out.startup = {
            nativePlayerNew: measure(() => new binding.NativePlayer(version, start, {}), { ...options, reps: 3 }),
            sceneWorldCold: measure(() => sceneWorld(version, 'dense').sync.dispose(), { ...options, reps: 3 }),
        };
        const runner = persistentRunner(version, start);
        out.latency = { persistentTick: latency(() => runner.tick(world)), scalarTick: latency(() => binding.simulateVersionedTick(version, world, start, {})) };
    }
    return { releases: results, scaling };
}

function overheadSuite() {
    const results = {};
    for (const version of releases) {
        const row = (results[version] = {});
        const { world, source, sync, registry } = sceneWorld(version, 'dense');
        row.napi = {
            emptyCall: measure(() => binding.isSupportedVersion(version), options),
            worldScalarCall: measure(() => world.isInsideBuildHeight(64), options),
            blockTagsAt: measure(() => world.blockTagsAt(1, 64, 1), options),
            blockAt: measure(() => world.blockAt(1, 64, 1), options),
            fluidAt: measure(() => world.fluidAt(1, 64, 1), options),
        };
        const start = playerState('dense');
        const player = new binding.NativePlayer(version, start, {});
        const objectState = player.state();
        const input = firstInput(start);
        const empty = new Float64Array(0);
        const bytes = { objectFields: Object.keys(objectState).length, typedBytesIn: input.byteLength, typedBytesOut: Out.LENGTH * 8 };
        row.conversion = {
            bytes,
            // both carry the whole state across the boundary and run no tick
            objectIn: measure(() => player.setState(objectState), options),
            objectOut: measure(() => player.state(), options),
            objectRoundTripNoTicks: measure(() => binding.simulateVersionedTicks(version, world, [start], 0, {}), options),
            typedCopyIn: measure(() => { new Float64Array(input); }, options),
            typedTickInEmptyWorld: (() => {
                const air = new binding.NativeWorld();
                const kept = new binding.NativePlayer(version, playerState('empty'), {});
                return measure(() => kept.tick(air, input, empty), options);
            })(),
        };
        row.shapes = {};
        for (const [name, radius] of [['player', 1], ['large', 8], ['huge', 24]]) {
            const min = { x: 10 - radius, y: 62, z: 0 - radius };
            const max = { x: 10 + radius, y: 66, z: 0 + radius };
            const found = world.collisionShapes(min, max, version);
            row.shapes[name] = {
                cells: (2 * radius + 1) ** 2 * 5, shapeCandidates: found.length, transferredBytes: found.length * 48,
                query: measure(() => world.collisionShapes(min, max, version), options),
            };
        }
        // a swept move: the same collision, far enough to walk many cells
        row.sweep = {};
        for (const [name, speed] of [['slow', 0.2], ['fast', 8], ['large', 40]]) {
            const state = { ...start, input: { x: 0, y: 0, z: 0 }, velocity: { x: speed, y: -0.08, z: 0 }, onGround: false, jumpRequested: false };
            row.sweep[name] = { blocksPerTick: speed, tick: measure(() => binding.simulateVersionedTick(version, world, state, {}), options) };
        }
        const cells = (REGION.maxX - REGION.minX + 1) * (REGION.maxY - REGION.minY + 1) * (REGION.maxZ - REGION.minZ + 1);
        const syncRun = () => {
            const fresh = new physics.NativeWorldSync(binding, registry, version);
            fresh.sync(source, REGION, 0);
            fresh.dispose();
        };
        const block = { x: 1.5, y: 64.5, z: 1.5 };
        row.sync = {
            cells,
            cold: measure(syncRun, { ...options, reps: 3 }),
            warmWatched: measure(() => sync.sync(source, REGION, 0), options),
            incrementalBlockChange: measure(() => {
                source.emit('blockUpdate', null, { position: block });
                sync.blockChanged(block);
                sync.sync(source, REGION, 0);
            }, options),
        };
        const Block = require('prismarine-block')(registry);
        const blocks = [...new Set(registry.blocksArray.map(entry => entry.defaultState ?? entry.id << 4))].map(id => Block.fromStateId(id, 0));
        row.sync.blockMapping = { blocks: blocks.length, describe: measure(() => { for (const one of blocks) sync.mapper.describe(one); }, { ...options, units: blocks.length }) };
        let shift = 0;
        row.sync.chunkChurn = measure(() => {
            // a column of a new chunk at the edge of what the player sees
            shift += 16;
            const minX = REGION.minX + (shift % 1600);
            sync.sync(source, { ...REGION, minX, maxX: minX + 15, maxZ: REGION.minZ + 15 }, 0);
        }, { ...options, reps: 3 });
        row.stages = stageTimes(version);
        sync.dispose();
        process.stderr.write(`${version} `);
    }
    return { releases: results };
}

/** The adapter's whole tick, and the same ticks with its sync and its native call timed from inside. */
function stageTimes(version) {
    const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
    const { botOf } = require(join(ROOT, 'test/bot-fixture.cjs'));
    const registry = registryFor(version);
    const source = sourceOf(version, SCENES.dense(registry));
    const bot = botOf(registry.version.minecraftVersion, { world: source });
    bot.entity.position.set(0.5, 64, 0.5);
    const engine = new physics.NativePhysics(registry, { version });
    const ctx = EPhysicsCtx.FROM_BOT(engine, bot);
    const syncProto = physics.NativeWorldSync.prototype;
    const playerProto = binding.NativePlayer.prototype;
    const original = { sync: syncProto.sync, tickMove: playerProto.tickMove, finishTick: playerProto.finishTick };
    const spent = { sync: 0, native: 0, syncCalls: 0, nativeCalls: 0 };
    const whole = () => {
        ctx.state.pos.set(0.5, 64, 0.5);
        ctx.state.vel.set(0, 0, 0);
        engine.simulate(ctx, source);
    };
    whole();
    const total = measure(whole, options);
    syncProto.sync = function timedSync(...args) {
        const begin = process.hrtime.bigint();
        const out = original.sync.apply(this, args);
        spent.sync += Number(process.hrtime.bigint() - begin) / 1000;
        spent.syncCalls += 1;
        return out;
    };
    for (const name of ['tickMove', 'finishTick']) {
        playerProto[name] = function timedNative(...args) {
            const begin = process.hrtime.bigint();
            const out = original[name].apply(this, args);
            spent.native += Number(process.hrtime.bigint() - begin) / 1000;
            spent.nativeCalls += 1;
            return out;
        };
    }
    const ticks = 4000;
    const begin = process.hrtime.bigint();
    for (let i = 0; i < ticks; i += 1) whole();
    const wall = Number(process.hrtime.bigint() - begin) / 1000;
    syncProto.sync = original.sync;
    playerProto.tickMove = original.tickMove;
    playerProto.finishTick = original.finishTick;
    return {
        wholeTickCpu: total,
        // wall clock inside one run: their sum is the run's by construction, and the shares are what to read
        sharesOfWall: { syncUs: rounded(spent.sync / ticks), nativeUs: rounded(spent.native / ticks), adapterUs: rounded((wall - spent.sync - spent.native) / ticks), totalUs: rounded(wall / ticks) },
        calls: { sync: spent.syncCalls / ticks, native: spent.nativeCalls / ticks },
        loadSensitive: true,
    };
}

function optimizationsSuite() {
    const results = {};
    for (const version of releases) {
        const row = (results[version] = {});
        for (const [sceneName, label] of [['empty', 'plain'], ['dense', 'legacyOrModernCollision'], ['fluid', 'fluid']]) {
            const TICKS = 200;
            const { scalar, batch, persistent, world, start } = agreeingPaths(version, sceneName, TICKS);
            assert.equal(batch, scalar);
            assert.equal(persistent, scalar);
            const baseline = measure(() => scalarRun(version, world, start, TICKS), { ...options, units: TICKS });
            const persistentTimes = measure(() => persistentRun(version, world, start, TICKS), { ...options, units: TICKS });
            const batchTimes = measure(() => binding.simulateVersionedTicks(version, world, [start], TICKS, {}), { ...options, units: TICKS });
            row[label] = {
                outcome: scalar,
                baselineObjectTick: baseline,
                persistentTypedTick: persistentTimes,
                batchTick: batchTimes,
                persistentOverBaseline: rounded(persistentTimes.cpuUs / baseline.cpuUs),
                batchOverBaseline: rounded(batchTimes.cpuUs / baseline.cpuUs),
            };
        }
        // the watched world skips the rescan a world without events needs
        const registry = registryFor(version);
        const cells = SCENES.dense(registry);
        const watched = sourceOf(version, cells);
        const unwatched = { getBlockStateId: watched.getBlockStateId, getBlock: watched.getBlock };
        const box = { minX: -8, minY: 60, minZ: -8, maxX: 8, maxY: 68, maxZ: 8 };
        const withEvents = new physics.NativeWorldSync(binding, registry, version);
        const withoutEvents = new physics.NativeWorldSync(binding, registry, version);
        withEvents.sync(watched, box, 0);
        withoutEvents.sync(unwatched, box, 0);
        assert.equal(withEvents.world.contentDigest(), withoutEvents.world.contentDigest(), `${version}: the worlds differ`);
        const eventDriven = measure(() => withEvents.sync(watched, box, 0), options);
        const rescan = measure(() => withoutEvents.sync(unwatched, box, 0), options);
        row.worldSync = { cells: 17 * 9 * 17, eventDriven, rescan, eventDrivenOverRescan: rounded(eventDriven.cpuUs / rescan.cpuUs) };
        withEvents.dispose();
        withoutEvents.dispose();
        process.stderr.write(`${version} `);
    }
    return { releases: results };
}

const started = cpuMs();
const body = { ticks: ticksSuite, overhead: overheadSuite, optimizations: optimizationsSuite }[suite]();
const out = arg('out', join(ROOT, `docs/benchmarks/${suite}-${process.platform}.json`));
writeJson(out, { suite, identity: identity(), minCpuMs, ...body, suiteCpuSeconds: rounded((cpuMs() - started) / 1000) });
process.stderr.write(`\nwrote ${out}\n`);
