'use strict';

// Bulk simulation a chunk at a time: every family agrees with its one long call and with a run stopped and resumed, in the
// scene a state brings; a cancellation meets its bound and leaves nothing behind (docs/bulk-simulation.md).

const assert = require('node:assert/strict');
const { getEventListeners } = require('node:events');
const { describe, it } = require('node:test');
const physics = require('..');

const binding = physics.LoadNativePhysics();
const { NativeBulkRun } = binding;

function floorWorld(fluid = false) {
    const world = new binding.NativeWorld();
    for (let x = -16; x < 96; x += 1) {
        for (let z = -16; z < 16; z += 1) {
            world.setFullBlock(x, 0, z, 1);
            if (fluid) for (let y = 1; y < 6; y += 1) world.setFluid(x, y, z, 1);
        }
    }
    return world;
}

const walker = (fields = {}) => ({
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, pitch: 0,
    look: { x: 0, y: 0, z: 1 }, halfWidth: 0.3, height: 1.8, eyeHeight: 1.62, onGround: true,
    collidedHorizontally: false, collidedVertically: false, input: { x: 0, y: 0, z: 1 }, isPlayer: true, clientSide: true,
    ...fields,
});

const box = (x, y, z) => ({ min: { x: x - 0.3, y, z: z - 0.3 }, max: { x: x + 0.3, y: y + 1.8, z: z + 0.3 } });
const entity = (x, z) => ({ kind: 'living', position: { x, y: 1, z }, boundingBox: box(x, 1, z) });
const piston = progress => ({
    x: 2, y: 1, z: 0, facing: 5, extending: true, source: false, progress,
    shapes: [{ min: { x: 2, y: 1, z: 0 }, max: { x: 3, y: 2, z: 1 } }],
});

/** A family: how to start a run, read it, and run it in one call, with what the call and a run both give at the end. */
const families = {
    player: (version, states) => ({
        world: floorWorld(),
        run: () => NativeBulkRun.players(version, states(), {}),
        read: run => run.playerStates(),
        restart: read => NativeBulkRun.players(version, read, {}),
        once: (world, ticks) => binding.simulateVersionedTicks(version, world, states(), ticks, {}),
        comparable: result => ({ ...result, nearbyEntities: null, movementSpeedAttribute: null, fireworkRockets: null, fireworkRocketsBeforePlayer: null, fireworkRocketTicks: null, movingPistons: null }),
    }),
    entity: (version, states) => ({
        world: floorWorld(),
        run: () => NativeBulkRun.entities(version, states(), {}),
        read: run => run.entityStates(),
        restart: read => NativeBulkRun.entities(version, read, {}),
        once: (world, ticks) => binding.simulateVersionedEntityTicks(version, world, states(), ticks, {}),
    }),
    boat: (version, states) => ({
        world: floorWorld(true),
        run: () => NativeBulkRun.boats(version, states()),
        read: run => run.boatStates(),
        once: (world, ticks) => binding.simulateVersionedBoatTicks(version, world, states(), ticks),
    }),
    horse: (version, states) => ({
        world: floorWorld(),
        run: () => NativeBulkRun.horses(version, states()),
        read: run => run.horseStates(),
        once: (world, ticks) => binding.simulateVersionedHorseTicks(version, world, states(), ticks),
    }),
    projectile: (version, states) => ({
        world: floorWorld(true),
        run: () => NativeBulkRun.projectiles(version, states()),
        read: run => run.projectileResults(),
        once: (world, ticks) => binding.simulateVersionedProjectileTicks(version, world, states(), ticks),
    }),
    arrow: (version, states) => ({
        world: floorWorld(true),
        run: () => NativeBulkRun.arrows(version, states()),
        read: run => run.arrowResults(),
        once: (world, ticks) => binding.simulateVersionedArrowTicks(version, world, states(), ticks),
    }),
    trident: (version, states) => ({
        world: floorWorld(true),
        run: () => NativeBulkRun.tridents(version, states()),
        read: run => run.tridentResults(),
        once: (world, ticks) => binding.simulateVersionedTridentTicks(version, world, states(), ticks),
    }),
};

const shot = (fields = {}) => ({ position: { x: 0.5, y: 9, z: 0.5 }, velocity: { x: 0.4, y: -0.6, z: 0 }, ...fields });

const cases = [
    ['a walk', 'player', '1.12.2', () => [walker(), walker({ position: { x: 3, y: 1, z: 3 } })]],
    ['entities in the way', 'player', '26.3', () => [walker({ nearbyEntities: [entity(1.1, 0.5), entity(2, 0.5)] })]],
    ['rockets that end', 'player', '26.3', () => [walker({
        fallFlying: true, onGround: false, position: { x: 0.5, y: 4, z: 0.5 }, fireworkRockets: 2, fireworkRocketsBeforePlayer: 1, fireworkRocketTicks: [5, 9],
    })]],
    ['pistons going on, gone after their end before 1.11', 'player', '1.10.2', () => [walker({ movingPistons: [piston(0)] })]],
    ['pistons going on, kept from 1.11', 'player', '1.21.11', () => [walker({ movingPistons: [piston(0)] })]],
    ['a crowd in its order', 'entity', '1.16.5', () => [1, 2, 3].map(id => ({ simulation: walker(), noPhysics: false, id, pushable: true, pushthrough: 0 }))],
    ['a boat among entities', 'boat', '26.3', () => [{
        position: { x: 0.5, y: 5.6, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: true, left: false, right: false, inWater: true,
        nearbyEntities: [{ kind: 'boat', position: { x: 6, y: 5.6, z: 0.5 }, boundingBox: box(6, 5.6, 0.5) }],
    }]],
    ['a ridden horse', 'horse', '1.20.4', () => [{
        position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 1, strafe: 0, jumpRequested: false, onGround: true, inWater: false,
    }]],
    ['a snowball into water', 'projectile', '1.21.4', () => [shot({ kind: 'snowball', firstTick: true })]],
    ['a snowball into water, before 1.13', 'projectile', '1.12.2', () => [shot({ kind: 'snowball', firstTick: true })]],
    ['an arrow into water', 'arrow', '26.3', () => [shot({ velocity: { x: 1.5, y: -0.5, z: 0 } })]],
    ['a trident into water', 'trident', '1.16.5', () => [shot({ velocity: { x: 1.5, y: -0.5, z: 0 } })]],
];

const TICKS = 47;

describe('a run in chunks', () => {
    for (const [name, family, version, states] of cases) {
        const make = () => families[family](version, states);
        const comparable = make().comparable ?? (result => result);

        it(`is the one long call: ${name}, ${version}`, async () => {
            const { world, once } = make();
            const whole = once(world, TICKS).map(comparable);
            for (const chunkTicks of [1, 7, 20, TICKS]) {
                const fresh = make();
                const run = fresh.run();
                const outcome = await physics.runBulk(run, fresh.world, TICKS, { chunkTicks });
                assert.deepEqual(outcome, { ticksDone: TICKS, ticksRun: TICKS, cancelled: false });
                assert.deepEqual(fresh.read(run).map(comparable), whole, `chunks of ${chunkTicks}`);
                run.dispose();
            }
        });

        it(`goes on from a cancellation as if never stopped: ${name}, ${version}`, async () => {
            const { world, once } = make();
            const whole = once(world, TICKS).map(comparable);
            const fresh = make();
            const run = fresh.run();
            const controller = new AbortController();
            const stopped = await physics.runBulk(run, fresh.world, TICKS, {
                chunkTicks: 5,
                signal: controller.signal,
                onProgress: done => { if (done === 15) controller.abort(); },
            });
            assert.deepEqual(stopped, { ticksDone: 15, ticksRun: 15, cancelled: true });
            const rest = await physics.runBulk(run, fresh.world, TICKS - 15, { chunkTicks: 8 });
            assert.equal(rest.ticksDone, TICKS);
            assert.deepEqual(fresh.read(run).map(comparable), whole);
            run.dispose();
        });
    }

    it('gives a player a state that starts a new run where the first stopped, scene and all', async () => {
        for (const [, family, version, states] of cases.filter(([, kind]) => kind === 'player' || kind === 'entity')) {
            const fresh = families[family](version, states);
            const comparable = fresh.comparable ?? (result => result);
            const whole = fresh.once(fresh.world, TICKS).map(comparable);
            const first = fresh.run();
            first.advance(fresh.world, 11);
            const second = fresh.restart(fresh.read(first));
            first.dispose();
            await physics.runBulk(second, fresh.world, TICKS - 11);
            assert.deepEqual(fresh.read(second).map(comparable), whole, version);
            second.dispose();
        }
    });
});

describe('the bound of a cancellation', () => {
    const world = floorWorld();
    const crowd = count => Array.from({ length: count }, () => walker());

    it('runs nothing for a signal already aborted', async () => {
        const run = NativeBulkRun.players('26.3', crowd(2), {});
        const outcome = await physics.runBulk(run, world, 100, { signal: AbortSignal.abort() });
        assert.deepEqual(outcome, { ticksDone: 0, ticksRun: 0, cancelled: true });
        run.dispose();
    });

    it('waits for no more than the chunk in hand, and the event loop turns between chunks', async () => {
        const run = NativeBulkRun.players('26.3', crowd(3), {});
        const controller = new AbortController();
        const chunkTicks = 4;
        let turns = 0;
        let doneAtAbort;
        const turning = () => {
            turns += 1;
            if (turns === 3) {
                doneAtAbort = run.ticksDone;
                controller.abort();
            } else setImmediate(turning);
        };
        setImmediate(turning);
        const outcome = await physics.runBulk(run, world, 400, { chunkTicks, signal: controller.signal });
        assert.equal(outcome.cancelled, true);
        assert.ok(outcome.ticksDone - doneAtAbort <= chunkTicks, `${outcome.ticksDone} after ${doneAtAbort}`);
        assert.ok(outcome.ticksDone < 400);
        run.dispose();
    });

    it('holds a chunk to the entity ticks of the limit, and a single tick to none', async () => {
        const run = NativeBulkRun.players('26.3', crowd(3), {});
        const limit = run.maxChunkWork;
        assert.throws(() => run.advance(world, Math.ceil(limit / 3) + 1), /entity ticks a chunk takes/);
        assert.equal(run.alive, true, 'a refused chunk ran nothing');
        assert.equal(run.ticksDone, 0);
        const largest = [];
        await physics.runBulk(run, world, 20, {
            chunkTicks: 100000,
            onProgress: (done) => { largest.push(done); },
        });
        assert.equal(run.ticksDone, 20);
        run.dispose();
        const many = limit + 1;
        const huge = NativeBulkRun.players('26.3', crowd(many), {});
        huge.advance(new binding.NativeWorld(), 1);
        assert.equal(huge.ticksDone, 1);
        huge.dispose();
    });

    it('splits a chunk that would pass the limit', async () => {
        const count = 4000;
        const run = NativeBulkRun.players('26.3', crowd(count), {});
        const steps = [];
        let before = 0;
        await physics.runBulk(run, new binding.NativeWorld(), 12, {
            chunkTicks: 12,
            onProgress: done => { steps.push(done - before); before = done; },
        });
        assert.ok(Math.max(...steps) * count <= run.maxChunkWork, String(steps));
        run.dispose();
    });
});

describe('what a run leaves', () => {
    const world = floorWorld();

    it('refuses everything after dispose, and frees the states', () => {
        const run = NativeBulkRun.players('26.3', [walker()], {});
        run.advance(world, 1);
        run.dispose();
        assert.equal(run.alive, false);
        assert.throws(() => run.advance(world, 1), /it was disposed/);
        assert.throws(() => run.playerStates(), /it was disposed/);
        run.dispose();
    });

    it('runs in the one world it started in', () => {
        const refused = NativeBulkRun.players('26.3', [walker()], {});
        refused.advance(new binding.NativeWorld(), 1);
        assert.throws(() => refused.advance(world, 1), /another NativeWorld/);
        assert.equal(refused.alive, true, 'the wrong world ran nothing');
        refused.dispose();
    });

    it('takes a chunk of an impossible move to an error that ends the run', async () => {
        const run = NativeBulkRun.boats('26.3', [{
            position: { x: 0.5, y: 5, z: 0.5 }, velocity: { x: 1e7, y: 0, z: 1e7 }, yaw: 0, forward: false, left: false, right: false, inWater: false,
        }]);
        await assert.rejects(physics.runBulk(run, world, 10, { chunkTicks: 2 }), /simulateVersionedBoatTicks/);
        assert.equal(run.alive, false);
        assert.throws(() => run.boatStates(), /a chunk of it failed/);
        assert.deepEqual(world.collisionShapes({ x: 0, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }).length, 1, 'the world keeps no entity colliders of it');
    });

    it('rejects an onProgress that throws with the run still at its chunk', async () => {
        const run = NativeBulkRun.players('26.3', [walker()], {});
        await assert.rejects(physics.runBulk(run, world, 100, { chunkTicks: 3, onProgress() { throw new Error('caller'); } }), /caller/);
        assert.equal(run.alive, true);
        assert.equal(run.ticksDone, 3);
        run.dispose();
    });

    it('refuses what is no count of ticks', async () => {
        const run = NativeBulkRun.players('26.3', [walker()], {});
        await assert.rejects(physics.runBulk(run, world, 1.5), /whole number/);
        await assert.rejects(physics.runBulk(run, world, 5, { chunkTicks: 0 }), /chunkTicks/);
        run.dispose();
    });

    it('holds no listener on the signal and grows no memory over repeated cancellation', async () => {
        const rounds = 3000;
        const round = async () => {
            const controller = new AbortController();
            const run = NativeBulkRun.players('26.3', [walker({ nearbyEntities: [entity(1, 0.5)] })], {});
            await physics.runBulk(run, world, 10, {
                chunkTicks: 2,
                signal: controller.signal,
                onProgress: () => controller.abort(),
            });
            assert.equal(getEventListeners(controller.signal, 'abort').length, 0);
            run.dispose();
        };
        for (let i = 0; i < 500; i += 1) await round();
        if (global.gc) global.gc();
        const before = process.memoryUsage();
        for (let i = 0; i < rounds; i += 1) await round();
        if (global.gc) global.gc();
        const after = process.memoryUsage();
        // the bound is a lot looser than a leak of a run a round would pass: a state is over 1 kB in native memory
        assert.ok(after.rss - before.rss < 32 * 1048576, `rss grew by ${after.rss - before.rss}`);
    });
});
