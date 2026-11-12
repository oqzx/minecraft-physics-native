'use strict';

// The addon across worker threads: each worker loads it and makes its own objects, independent simulations reproduce a
// serial run bit for bit, what ends a worker leaves the others running, and a native object does not cross. Then the
// ownership of what crosses a call: results are the caller's, and input is not kept.

const assert = require('node:assert/strict');
const path = require('node:path');
const { describe, it } = require('node:test');
const { Worker, isMainThread, parentPort, workerData } = require('node:worker_threads');

const binding = require('../native/artifacts/native.js');

/** Ticks a player over a floor; the numbers it ends with, as their bits. */
function simulate(version, seed, ticks) {
    const world = new binding.NativeWorld();
    for (let x = -8; x < 8; x += 1) for (let z = -8; z < 8; z += 1) world.setFullBlock(x, 0, z, 1);
    let state = {
        position: { x: 0.5, y: 1 + seed / 100, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: false,
        collidedHorizontally: false, collidedVertically: false, input: { x: 0, y: 0, z: 1 }, yaw: seed / 10, isPlayer: true,
        movementSpeed: 0.1,
    };
    for (let i = 0; i < ticks; i += 1) state = binding.simulateVersionedTick(version, world, state, {});
    const { position: { x, y, z }, velocity } = state;
    return [...new Uint8Array(new Float64Array([x, y, z, velocity.x, velocity.y, velocity.z]).buffer)].join();
}

if (!isMainThread) {
    parentPort.postMessage(simulate(workerData.version, workerData.seed, workerData.ticks));
} else {
    const run = data => new Promise((resolve, reject) => {
        const worker = new Worker(__filename, { workerData: data });
        let result;
        worker.once('message', message => { result = message; });
        worker.once('error', reject);
        worker.once('exit', code => (code === 0 ? resolve(result) : reject(new Error(`worker exit ${code}`))));
    });

    describe('the addon across workers', () => {
        const cases = ['1.8.8', '1.12.2', '1.20.4', '1.21.11', '26.3'].flatMap((version, at) => [1, 2, 3].map(seed => ({ version, seed: seed + at * 7, ticks: 40 })));

        it('reproduces a serial run in independent workers, and on the main thread beside them', async () => {
            const serial = cases.map(({ version, seed, ticks }) => simulate(version, seed, ticks));
            const parallel = await Promise.all(cases.map(run));
            assert.deepEqual(parallel, serial);
        });

        it('survives workers that come and go', async () => {
            const expected = simulate('1.21.11', 1, 5);
            for (let round = 0; round < 3; round += 1) {
                const workers = Array.from({ length: 4 }, () => new Worker(__filename, { workerData: { version: '1.21.11', seed: 1, ticks: 1000000 } }));
                await Promise.all(workers.map(worker => worker.terminate()));
                assert.equal(await run({ version: '1.21.11', seed: 1, ticks: 5 }), expected);
            }
            assert.equal(simulate('1.21.11', 1, 5), expected);
        });

        it('fails a worker\'s bad call in that worker alone', async () => {
            await assert.rejects(run({ version: '9.9.9', seed: 0, ticks: 1 }), /9\.9\.9/);
            assert.equal(typeof await run({ version: '1.21.11', seed: 0, ticks: 1 }), 'string');
        });

        it('does not let a native object cross to another worker', () => {
            const world = new binding.NativeWorld();
            world.setFullBlock(0, 0, 0, 1);
            const { port1, port2 } = new MessageChannel();
            let crossed;
            try {
                port1.postMessage(world);
                crossed = require('node:worker_threads').receiveMessageOnPort(port2)?.message;
            } catch {
                crossed = undefined;
            }
            port1.close();
            port2.close();
            // refused, or arrives as a plain copy that holds no world
            if (crossed !== undefined) {
                assert.equal(crossed instanceof binding.NativeWorld, false);
                assert.throws(() => binding.simulateVersionedTick('1.21.11', crossed, {
                    position: { x: 0, y: 1, z: 0 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: false,
                    collidedHorizontally: false, collidedVertically: false,
                }, {}));
            }
        });
    });

    describe('what crosses a call', () => {
        const { NativePlayerInput: In, NativePlayerOutput: Out } = require(path.join('..', 'dist', 'native'));
        const walker = () => ({
            position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: true,
            collidedHorizontally: false, collidedVertically: false, isPlayer: true, movementSpeed: 0.1,
        });
        const inputOf = () => {
            const input = new Float64Array(In.LENGTH);
            input.set([0.5, 1, 0.5], In.POSITION);
            input[In.FORWARD] = 1;
            input[In.HALF_WIDTH] = 0.3;
            input[In.HEIGHT] = 1.8;
            input[In.EYE_HEIGHT] = 1.62;
            return input;
        };
        const floor = () => {
            const world = new binding.NativeWorld();
            for (let x = -8; x < 8; x += 1) for (let z = -8; z < 8; z += 1) world.setFullBlock(x, 0, z, 1);
            return world;
        };

        it('gives each tick an array of its own, which later ticks leave alone', () => {
            const world = floor();
            const player = new binding.NativePlayer('1.21.11', walker(), {});
            const input = inputOf();
            const first = player.tick(world, input, new Float64Array());
            const kept = Float64Array.from(first);
            const second = player.tick(world, input, new Float64Array());
            assert.notEqual(first.buffer, second.buffer);
            player.setState(walker());
            player.tick(world, input, new Float64Array());
            assert.deepEqual(first, kept);
        });

        it('reads the input of a call and keeps none of it', () => {
            const world = floor();
            const input = inputOf();
            const entities = new Float64Array(21);
            const copy = (make) => {
                const player = new binding.NativePlayer('1.21.11', walker(), {});
                const array = make();
                player.tick(world, array, new Float64Array());
                array.fill(1234);
                return player.state();
            };
            assert.deepEqual(copy(() => Float64Array.from(input)), copy(() => inputOf()));
            const untouched = Float64Array.from(input);
            const player = new binding.NativePlayer('1.21.11', walker(), {});
            player.tick(world, input, entities);
            assert.deepEqual(input, untouched);
            assert.deepEqual(entities, new Float64Array(21));
        });

        it('changes the kept state by its setters alone', () => {
            const world = floor();
            const player = new binding.NativePlayer('1.21.11', walker(), {});
            const out = player.tick(world, inputOf(), new Float64Array());
            const state = player.state();
            const position = { ...state.position };
            out[Out.POSITION] = 99;
            state.position.x = 99;
            state.input = { x: 9, y: 9, z: 9 };
            assert.deepEqual(player.state().position, position);
            assert.notDeepEqual(player.state().input, state.input);
            player.setState(state);
            assert.equal(player.state().position.x, 99);
        });

        it('keeps a player to its world until setState moves it', () => {
            const [first, second] = [floor(), floor()];
            const player = new binding.NativePlayer('1.21.11', walker(), {});
            const before = player.tick(first, inputOf(), new Float64Array());
            const kept = player.state();
            assert.throws(() => player.tick(second, inputOf(), new Float64Array()), /another NativeWorld/);
            assert.throws(() => player.pushByMovingPistons(second, []), /another NativeWorld/);
            assert.deepEqual(player.state(), kept);
            // a refused tick moved nothing, and the world it knows still runs
            assert.equal(player.tick(first, inputOf(), new Float64Array()).length, before.length);
            player.setState(player.state());
            player.tick(second, inputOf(), new Float64Array());
            assert.throws(() => player.tick(first, inputOf(), new Float64Array()), /another NativeWorld/);
        });

        it('ends a tick in the world it began in', () => {
            const [first, second] = [floor(), floor()];
            const player = new binding.NativePlayer('1.21.11', walker(), {});
            player.tickMove(first, inputOf(), new Float64Array());
            assert.throws(() => player.finishTick(second, 1.8), /another NativeWorld/);
            player.finishTick(first, 1.8);
        });

        it('lets any number of players and releases share a world', () => {
            const world = floor();
            const players = ['1.8.8', '1.20.4', '26.3'].map(version => new binding.NativePlayer(version, walker(), {}));
            for (let tick = 0; tick < 3; tick += 1) for (const player of players) player.tick(world, inputOf(), new Float64Array());
            assert.deepEqual(new binding.NativePlayer('1.21.11', walker(), {}).state().position, walker().position);
        });

        it('keeps no input object: a state or a block update edited after the call changes nothing', () => {
            const state = walker();
            const player = new binding.NativePlayer('1.21.11', state, {});
            state.position.x = 99;
            assert.notEqual(player.state().position.x, 99);
            const update = { x: 0, y: 3, z: 0, id: 1, tags: 1 };
            const world = floor();
            world.applyBlockUpdates([update]);
            update.tags = 9;
            assert.equal(world.blockAt(0, 3, 0).tags, 1);
        });

        it('refuses an unknown release at construction, and takes no other in its place', () => {
            for (const version of ['1.8.9', '1.20.7', '1.21', '9.9.9', '']) {
                if (binding.isSupportedVersion(version)) continue;
                assert.throws(() => new binding.NativePlayer(version, walker(), {}), /unsupported decompiled Minecraft version/);
            }
        });

        it('stands repeated making, reuse and letting go', () => {
            const expected = (() => {
                const player = new binding.NativePlayer('1.21.11', walker(), {});
                return player.tick(floor(), inputOf(), new Float64Array());
            })();
            for (let i = 0; i < 200; i += 1) {
                const world = floor();
                const player = new binding.NativePlayer('1.21.11', walker(), {});
                const ran = player.tick(world, inputOf(), new Float64Array());
                player.setState(walker());
                assert.deepEqual(player.tick(world, inputOf(), new Float64Array()), ran);
                assert.deepEqual(ran, expected);
            }
        });
    });
}
