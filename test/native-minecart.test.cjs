// The minecart engine through its N-API face and the mineflayer adapter: which path a level's features select, the state
// it gives back, the diagnostics of what it cannot run, and the client's interpolation, which reads no world.
const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const adapter = require('../dist/index.js');

function worldOf(version, place) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const blocks = new Map();
    place({ registry, Block, set: (x, y, z, id) => blocks.set(`${x},${y},${z}`, id) });
    const air = registry.blocksByName.air.defaultState ?? 0;
    const stateAt = pos => blocks.get(`${Math.floor(pos.x)},${Math.floor(pos.y)},${Math.floor(pos.z)}`) ?? air;
    const world = Object.assign(new EventEmitter(), {
        getBlock: pos => Block.fromStateId(stateAt(pos), 0),
        getBlockStateId: stateAt,
    });
    return { registry, world };
}

/** A straight east-west rail over stone, in the release's own terms. */
function track(version) {
    return worldOf(version, ({ registry, Block, set }) => {
        const flat = registry.version['>=']('1.13');
        const rail = flat ? Block.fromProperties('rail', { shape: 'east_west', waterlogged: false }, 0).stateId : (registry.blocksByName.rail.id << 4) | 1;
        const stone = registry.blocksByName.stone.defaultState ?? (registry.blocksByName.stone.id << 4);
        for (let x = -2; x < 30; x += 1) {
            set(x, 0, 0, rail);
            set(x, -1, 0, stone);
        }
    });
}

const cart = fields => ({
    variant: 'minecart', position: { x: 2.5, y: 0.0625, z: 0.5 }, velocity: { x: 0.1, y: 0, z: 0 }, yRot: 0, flipped: false,
    onGround: false, inWater: false, ...fields,
});

describe('NativeMinecartPhysics', () => {
    it('goes along a rail on the old path in every kind of release', () => {
        for (const version of ['1.8.8', '1.13.2', '1.20.4', '26.3']) {
            const { registry, world } = track(version);
            const physics = new adapter.NativeMinecartPhysics(registry);
            const { state } = physics.simulate(cart(), world, { improvements: false });
            assert.ok(Math.abs(state.position.x - 2.6) < 1e-9, version);
            assert.equal(state.onRails, true, version);
            assert.ok(state.velocity.x > 0.09 && state.velocity.x < 0.1, version);
        }
    });

    it('takes the improved path by the flag and not by the release', () => {
        const { registry, world } = track('1.21.4');
        const physics = new adapter.NativeMinecartPhysics(registry);
        const old = physics.simulate(cart(), world, { improvements: false });
        const improved = physics.simulate(cart({ position: { x: 2.5, y: 0.1, z: 0.5 }, onRails: true }), world, { improvements: true, maxMinecartSpeed: 8 });
        assert.equal(old.state.velocity.x, 0.1 * 0.96);
        assert.equal(old.steps.length, 0);
        assert.equal(improved.state.velocity.x, 0.1 * 0.975);
        assert.equal(improved.steps.length, 1);
    });

    it('says what it cannot run', () => {
        const { registry, world } = track('1.21.4');
        const physics = new adapter.NativeMinecartPhysics(registry);
        assert.throws(() => physics.simulate(cart(), world, { improvements: true }), /maxMinecartSpeed/);
        assert.throws(() => physics.simulate(cart({ variant: 'boat' }), world, { improvements: false }), /boat is no minecart/);
        assert.throws(() => physics.simulate(cart({ variant: 'furnace_minecart' }), world, { improvements: false }), /fuel and push/);
        assert.throws(() => physics.simulate(cart({ variant: 'chest_minecart' }), world, { improvements: false }), /comparatorSignal/);
        const older = track('1.20.4');
        const before = new adapter.NativeMinecartPhysics(older.registry);
        assert.throws(() => before.simulate(cart(), older.world, { improvements: true, maxMinecartSpeed: 8 }), /does not exist in 1\.20\.4/);
    });

    it('gives its state back to carry on from', () => {
        const { registry, world } = track('1.20.4');
        const physics = new adapter.NativeMinecartPhysics(registry);
        let state = cart({ variant: 'furnace_minecart', fuel: 5, push: { x: 1, y: 0, z: 0 } });
        for (let tick = 0; tick < 3; tick += 1) ({ state } = physics.simulate(state, world, { improvements: false }));
        assert.equal(state.variant, 'furnace_minecart');
        assert.equal(state.fuel, 2);
        assert.ok(state.position.x > 2.5);
    });

    it('interpolates a client cart with no world, by its release', () => {
        const registry = require('prismarine-registry')('1.20.4');
        const physics = new adapter.NativeMinecartPhysics(registry);
        let lerp = physics.lerpTo({ position: { x: 0, y: 0, z: 0 }, yRot: 0, xRot: 0 }, { x: 5, y: 0, z: 0 }, 0, 0);
        assert.equal(lerp.steps, 5);
        lerp = physics.interpolate(lerp);
        assert.equal(lerp.position.x, 1);
        assert.equal(new adapter.NativeMinecartPhysics(require('prismarine-registry')('26.3')).lerpTo(lerp, lerp.target, 0, 0).steps, 3);
    });

    it('takes the push of a boat at its turn in the level, from 1.9', () => {
        const boat = fields => ({
            position: { x: 1.15, y: 0.0625, z: 0.5 },
            boundingBox: { min: { x: 0.4625, y: 0.0625, z: -0.1875 }, max: { x: 1.8375, y: 0.625, z: 1.1875 } },
            pushable: true, solid: true, boat: true, hasPassengers: true, ...fields,
        });
        const still = cart({ velocity: { x: 0, y: 0, z: 0 } });
        const pushed = (version, fields) => {
            const { registry, world } = track(version);
            return new adapter.NativeMinecartPhysics(registry).simulateScene([still], [boat(fields)], world, { improvements: false }).carts[0].state;
        };
        for (const version of ['1.9.4', '1.20.4', '26.3']) {
            const after = pushed(version, {});
            assert.equal(after.position.x, 2.5, version);
            assert.ok(Math.abs(after.velocity.x - 0.05) < 1e-6, version);
            assert.ok(pushed(version, { ticksAfter: 0 }).position.x > 2.5, version);
        }
        assert.equal(pushed('1.8.8', {}).velocity.x, 0);
        assert.throws(() => pushed('1.20.4', { tickPosition: { x: 1, y: 0, z: 0 } }), /go together/);
    });

    it('walks a client cart through the steps the server sent', () => {
        const physics = new adapter.NativeMinecartPhysics(require('prismarine-registry')('26.3'));
        const origin = { x: 0, y: 0, z: 0 };
        const step = { position: { x: 3, y: 0, z: 0 }, movement: origin, yRot: 0, xRot: 0, weight: 3 };
        let client = { position: origin, velocity: origin, yRot: 0, xRot: 0, delay: 0, pending: [step], current: [], totalWeight: 0, old: { ...step, position: origin, weight: 0 } };
        client = physics.interpolateSteps(client);
        assert.ok(Math.abs(client.position.x - 1) < 1e-6);
        assert.equal(client.delay, 3);
        assert.equal(client.pending.length, 0);
    });
    it('pushes the carts and entities about a TNT cart\'s blast, and hurts the carts', () => {
        const { registry, world } = track('26.3');
        const physics = new adapter.NativeMinecartPhysics(registry);
        const still = { x: 0, y: 0, z: 0 };
        const near = cart({ position: { x: 9.1, y: 0.0625, z: 0.5 }, velocity: still });
        const lit = cart({ variant: 'tnt_minecart', position: { x: 2.5, y: 0.0625, z: 0.5 }, velocity: still, fuse: 0, randomSeed: 42 });
        const pig = { position: { x: 7.5, y: 0.0625, z: 0.5 }, boundingBox: { min: { x: 7.05, y: 0.0625, z: 0.05 }, max: { x: 7.95, y: 0.9625, z: 0.95 } }, living: true, pushable: true, eyeHeight: 0.765 };
        const scene = physics.simulateScene([near, lit], [pig], world, { improvements: false });
        assert.equal(scene.carts[1].explosions.length, 1);
        assert.ok(scene.carts[0].state.velocity.x > 0, 'pushed away');
        assert.ok(scene.carts[0].state.damage > 0 && scene.carts[0].state.hurtTime === 10);
        assert.ok(scene.others[0].velocity.x > 0 && scene.others[0].eyeHeight === Math.fround(0.765));
    });

    it('breaks the blocks a TNT cart\'s rays reach, as far as their resistance lets them, and burns up the items near it', () => {
        const { registry, world } = worldOf('26.3', ({ registry, Block, set }) => {
            const rail = Block.fromProperties('rail', { shape: 'east_west', waterlogged: false }, 0).stateId;
            for (let x = -2; x < 12; x += 1) {
                set(x, 0, 0, rail);
                set(x, -1, 0, registry.blocksByName.stone.defaultState);
            }
            set(2, 0, 1, registry.blocksByName.obsidian.defaultState);
            set(3, 0, -1, registry.blocksByName.dirt.defaultState);
        });
        const physics = new adapter.NativeMinecartPhysics(registry);
        const lit = cart({ variant: 'tnt_minecart', velocity: { x: 0, y: 0, z: 0 }, fuse: 0, randomSeed: 42 });
        const item = x => ({ boundingBox: { min: { x: x - 0.125, y: 0.0625, z: 0.375 }, max: { x: x + 0.125, y: 0.3125, z: 0.625 } }, stack: { item: 'stone', count: 1, maxStack: 64 } });
        const has = (list, x, y, z) => list.some(p => p.x === x && p.y === y && p.z === z);
        const scene = physics.simulateScene([lit], [], world, { improvements: false, levelRandom: 12345 }, [item(3.5), item(9.9)]);
        const [blast] = scene.carts[0].explosions;
        // obsidian takes all of a ray, dirt hardly any; a primed cart spares the rail and the stone under it
        assert.ok(!has(blast.possible, 2, 0, 1));
        assert.ok(has(blast.certain, 3, 0, -1));
        assert.ok(!has(blast.possible, 2, 0, 0) && !has(blast.possible, 2, -1, 0));
        assert.ok(blast.certain.every(p => has(blast.blown, p.x, p.y, p.z)) && blast.blown.every(p => has(blast.possible, p.x, p.y, p.z)));
        assert.equal(typeof blast.levelRandom, 'number');
        assert.equal(scene.items[0].removed, true);
        assert.equal(scene.items[1].removed, false);
        assert.ok(scene.items[1].velocity.x > 0 && scene.items[1].health > 0 && scene.items[1].health < 5);
        // without the level's random, the bounds alone
        const unseeded = physics.simulateScene([lit], [], world, { improvements: false }).carts[0].explosions[0];
        assert.equal(unseeded.blown, undefined);
        assert.deepEqual(unseeded.certain, blast.certain);
        assert.throws(() => physics.simulateScene([lit], [], world, { improvements: false, levelRandom: 0.5 }), /levelRandom/);
    });

    it('reads the world as far as a TNT cart\'s strongest rays reach', () => {
        // (stone 5.5 blocks off, past the 3 a cart's own tick reads: no ray breaks through it)
        const { registry, world } = worldOf('26.3', ({ registry, Block, set }) => {
            const rail = Block.fromProperties('rail', { shape: 'east_west', waterlogged: false }, 0).stateId;
            for (let x = -2; x < 8; x += 1) {
                set(x, 0, 0, rail);
                set(x, -1, 0, registry.blocksByName.stone.defaultState);
            }
            for (let y = -7; y <= 7; y += 1) for (let z = -7; z <= 7; z += 1) set(8, y, z, registry.blocksByName.stone.defaultState);
        });
        const lit = cart({ variant: 'tnt_minecart', velocity: { x: 0, y: 0, z: 0 }, fuse: 0, randomSeed: 42 });
        const [blast] = new adapter.NativeMinecartPhysics(registry).simulateScene([lit], [], world, { improvements: false }).carts[0].explosions;
        assert.ok(blast.possible.some(p => p.x === 7));
        assert.deepEqual(blast.possible.filter(p => p.x >= 9), []);
    });

    it('lets the team rules from 1.9, an armor stand and a bat push a cart their own ways', () => {
        const { registry, world } = track('1.20.4');
        const physics = new adapter.NativeMinecartPhysics(registry);
        const still = cart({ variant: 'chest_minecart', velocity: { x: 0, y: 0, z: 0 }, comparatorSignal: 0 });
        const golem = team => ({ position: { x: 3.0, y: 0.0625, z: 0.5 }, boundingBox: { min: { x: 2.3, y: 0.0625, z: -0.2 }, max: { x: 3.7, y: 2.7625, z: 1.2 } }, living: true, pushable: true, ironGolem: true, ...(team ? { team } : {}) });
        const pushed = others => physics.simulateScene([still], others, world, { improvements: false }).carts[0].state.velocity.x;
        assert.ok(pushed([golem()]) < 0);
        assert.equal(pushed([golem({ name: 'tn', collisionRule: 'never' })]), 0);
        assert.throws(() => pushed([golem({ name: 'tn', collisionRule: 'sometimes' })]), /collision rule/);
        // (a bat pushes nothing; an armor stand only a rideable cart at its own place)
        const stand = (x, extra) => ({ position: { x, y: 0.0625, z: 0.5 }, boundingBox: { min: { x: x - 0.25, y: 0.0625, z: 0.25 }, max: { x: x + 0.25, y: 2.0375, z: 0.75 } }, living: true, ...extra });
        assert.equal(pushed([stand(2.8, { bat: true })]), 0);
        assert.equal(pushed([stand(2.8, { armorStand: true })]), 0, 'a chest cart is no rideable cart');
        const rideable = physics.simulateScene([cart({ velocity: { x: 0, y: 0, z: 0 } })], [stand(2.8, { armorStand: true })], world, { improvements: false });
        assert.ok(rideable.carts[0].state.velocity.x < 0);
        assert.equal(rideable.others[0].armorStand, true);
    });

    it('sends a cart through an end gateway by the release\'s way, and counts a spawner cart down', () => {
        const gateway = { position: { x: 2, y: 0, z: 0 }, exit: { x: 20, y: 0, z: 0 }, cooldown: 0, age: 0 };
        const resting = cart({ velocity: { x: 0, y: 0, z: 0 } });
        const through = version => {
            const { registry, world } = track(version);
            return new adapter.NativeMinecartPhysics(registry).simulateScene([resting], [], world, { improvements: false }, [], [], [gateway]);
        };
        const old = through('1.20.4');
        assert.equal(old.carts[0].state.position.x, 20.5);
        assert.equal(old.gateways[0].cooldown, 40);
        const modern = through('26.3');
        assert.equal(modern.carts[0].state.portal, 'gateway');
        assert.deepEqual(modern.carts[0].state.portalEntry, { x: 2, y: 0, z: 0 });
        // (on its next tick it goes through, and moves on from the exit's rail, which the sync reads too)
        const { registry: modernRegistry, world: modernWorld } = track('26.3');
        const sent = new adapter.NativeMinecartPhysics(modernRegistry).simulateScene([modern.carts[0].state], [], modernWorld, { improvements: false }, [], [], modern.gateways).carts[0].state;
        assert.ok(sent.position.x > 20 && sent.position.x < 21, JSON.stringify(sent.position));
        assert.equal(sent.onRails, true);
        const { registry, world } = track('26.3');
        const player = { position: { x: 6.5, y: 0, z: 0.5 }, boundingBox: { min: { x: 6.2, y: 0, z: 0.2 }, max: { x: 6.8, y: 1.8, z: 0.8 } }, player: true, living: true };
        const spawner = physics => physics.simulateScene([cart({ variant: 'spawner_minecart', velocity: { x: 0, y: 0, z: 0 }, spawnDelay: 1 })], [player], world, { improvements: false }).carts[0];
        const physics = new adapter.NativeMinecartPhysics(registry);
        const first = spawner(physics);
        assert.equal(first.state.spawnDelay, 0);
        assert.equal(first.spawner, undefined);
        const tried = physics.simulateScene([first.state], [player], world, { improvements: false }).carts[0];
        assert.equal(tried.spawner, 'attempt');
        assert.deepEqual(tried.spawnAt, { x: 2, y: 0, z: 0 });
        assert.throws(() => physics.simulateScene([cart({ portal: 'gateway' })], [], world, { improvements: false }), /portalEntry/);
    });
});
