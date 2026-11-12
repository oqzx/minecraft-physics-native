// A bot's carts from packets alone, with a fake bot: an emitter of what minecraft-protocol's client emits, and a tick. Each
// packet's effect is checked against what the client's handler does with it; the recorder package's replay/cli.ts
// --bot-carts (@minecraft-physics-native/recorder) holds the same class to a real client's cart through the recordings.
const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { EventEmitter } = require('node:events');
const adapter = require('../dist/index.js');

function fakeBot(version, options) {
    const registry = require('prismarine-registry')(version);
    const physics = new adapter.NativeMinecartPhysics(registry);
    const client = new EventEmitter();
    return { client, registry, tracker: new adapter.MinecartTracker(physics, client, options) };
}

const ID = 7;
const rideable = (bot, position = { x: 0, y: 64, z: 0 }) => bot.tracker.adopt({ id: ID, variant: 'minecart', position, yRot: 0, xRot: 0 });

describe('MinecartTracker', () => {
    it('finds carts in the spawn packets of a release', () => {
        const modern = fakeBot('26.3');
        const type = modern.registry.entitiesByName.tnt_minecart.internalId ?? modern.registry.entitiesByName.tnt_minecart.id;
        modern.client.emit('spawn_entity', { entityId: 1, type, x: 1, y: 2, z: 3, yaw: 64, pitch: 0, velocity: { x: 0.5, y: 0, z: 0 } });
        const tnt = modern.tracker.carts.get(1);
        assert.equal(tnt.variant, 'tnt_minecart');
        assert.deepEqual(tnt.position, { x: 1, y: 2, z: 3 });
        assert.equal(tnt.yRot, 90);
        assert.equal(tnt.velocity.x, 0.5);
        modern.client.emit('spawn_entity', { entityId: 2, type: modern.registry.entitiesByName.pig.id, x: 0, y: 0, z: 0, yaw: 0, pitch: 0 });
        assert.equal(modern.tracker.carts.has(2), false);
        // before 1.14 a cart is object 10, its kind in the data, and 1.8's position is in 32nds
        const old = fakeBot('1.8.8');
        old.client.emit('spawn_entity', { entityId: 3, type: 10, objectData: 5, x: 32, y: 64, z: 96, yaw: 0, pitch: 0, velocity: { x: 800, y: 0, z: 0 } });
        const hopper = old.tracker.carts.get(3);
        assert.equal(hopper.variant, 'hopper_minecart');
        assert.deepEqual(hopper.position, { x: 1, y: 2, z: 3 });
        assert.equal(hopper.velocity.x, 0.1);
        old.client.emit('spawn_entity', { entityId: 4, type: 2, objectData: 0, x: 0, y: 0, z: 0, yaw: 0, pitch: 0 });
        assert.equal(old.tracker.carts.has(4), false);
    });

    it('interpolates to a relative move over the steps of its release', () => {
        for (const [version, steps] of [['1.20.4', 5], ['26.3', 3]]) {
            const bot = fakeBot(version);
            rideable(bot);
            bot.client.emit('rel_entity_move', version === '26.3'
                ? { entityId: ID, delta: { dX: 4096, dY: 0, dZ: 0, onGround: true } }
                : { entityId: ID, dX: 4096, dY: 0, dZ: 0, onGround: true });
            const cart = bot.tracker.carts.get(ID);
            assert.equal(cart.lerp.steps, steps, version);
            assert.equal(cart.position.x, 0, 'a packet alone moves nothing');
            assert.equal(cart.onGround, true);
            bot.tracker.tick();
            assert.ok(Math.abs(cart.position.x - 1 / steps) < 1e-9, version);
            for (let i = 1; i < steps; i += 1) bot.tracker.tick();
            assert.equal(cart.position.x, 1, version);
            bot.tracker.tick();
            assert.equal(cart.position.x, 1, 'and then it stays');
        }
    });

    it('adds the deltas of a move on what the server said, not on where the cart is', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        const move = { entityId: ID, dX: 2048, dY: 0, dZ: 0, onGround: false };
        bot.client.emit('rel_entity_move', move);
        bot.client.emit('rel_entity_move', move);
        assert.equal(bot.tracker.carts.get(ID).lerp.target.x, 1);
        assert.equal(bot.tracker.carts.get(ID).position.x, 0);
    });

    it('takes the fixed point of 1.8', () => {
        const bot = fakeBot('1.8.8');
        rideable(bot);
        bot.client.emit('rel_entity_move', { entityId: ID, dX: 32, dY: -16, dZ: 0, onGround: false });
        assert.deepEqual(bot.tracker.carts.get(ID).lerp.target, { x: 1, y: 63.5, z: 0 });
        bot.client.emit('entity_teleport', { entityId: ID, x: 320, y: 2048, z: 0, yaw: 0, pitch: 0, onGround: true });
        assert.deepEqual(bot.tracker.carts.get(ID).lerp.target, { x: 10, y: 64, z: 0 });
    });

    it('takes a teleport up to where the deltas add from', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        bot.client.emit('entity_teleport', { entityId: ID, x: 10, y: 64, z: 0, yaw: -128, pitch: 0, onGround: true });
        const cart = bot.tracker.carts.get(ID);
        assert.equal(cart.lerp.target.x, 10);
        assert.equal(cart.lerp.targetYRot, -180);
        bot.client.emit('rel_entity_move', { entityId: ID, dX: 4096, dY: 0, dZ: 0, onGround: true });
        assert.equal(cart.lerp.target.x, 11);
    });

    it('turns a near teleport in place up to 1.16.1, and not from 1.16.2', () => {
        const near = { entityId: ID, x: 0.01, y: 64, z: 0, yaw: 64, pitch: 0, onGround: true };
        const before = fakeBot('1.14.4');
        rideable(before);
        before.client.emit('entity_teleport', near);
        assert.equal(before.tracker.carts.get(ID).lerp.target.x, 0);
        assert.equal(before.tracker.carts.get(ID).lerp.steps, 2);
        // (over the usual 3 steps on 1.15 - 1.16.1: 1.15 ClientPacketListener.java:581-582)
        for (const version of ['1.15', '1.16.1']) {
            const turned = fakeBot(version);
            rideable(turned);
            turned.client.emit('entity_teleport', near);
            assert.equal(turned.tracker.carts.get(ID).lerp.target.x, 0, version);
            assert.equal(turned.tracker.carts.get(ID).lerp.steps, 5, version);
        }
        const after = fakeBot('1.20.4');
        rideable(after);
        after.client.emit('entity_teleport', near);
        assert.equal(after.tracker.carts.get(ID).lerp.target.x, 0.01);
        assert.equal(after.tracker.carts.get(ID).lerp.steps, 5);
    });

    it('takes a position sync, and snaps past 64 blocks', () => {
        const bot = fakeBot('1.21.4');
        rideable(bot);
        bot.client.emit('sync_entity_position', { entityId: ID, x: 3, y: 64, z: 0, yaw: 90, pitch: 0, onGround: true });
        assert.equal(bot.tracker.carts.get(ID).lerp.target.x, 3);
        bot.client.emit('sync_entity_position', { entityId: ID, x: 300, y: 64, z: 0, yaw: 90, pitch: 0, onGround: true });
        assert.equal(bot.tracker.carts.get(ID).position.x, 300);
        // 26.3's path ends at its last step
        const late = fakeBot('26.3');
        rideable(late);
        late.client.emit('sync_entity_position', { entityId: ID, path: { steps: [{ x: 1, y: 64, z: 0 }, { x: 2, y: 64, z: 0 }] }, yaw: 0, pitch: 0, onGround: true });
        assert.equal(late.tracker.carts.get(ID).lerp.target.x, 2);
    });

    it('takes velocity for now and for the next interpolation', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        bot.client.emit('entity_velocity', { entityId: ID, velocity: { x: 4000, y: 0, z: 0 } });
        const cart = bot.tracker.carts.get(ID);
        assert.equal(cart.velocity.x, 0.5);
        cart.velocity = { x: 9, y: 9, z: 9 };
        bot.client.emit('rel_entity_move', { entityId: ID, dX: 1, dY: 0, dZ: 0, onGround: true });
        assert.equal(cart.velocity.x, 0.5);
    });

    it('reads the synced hurt at the index of its release', () => {
        for (const [version, index] of [['1.8.8', 17], ['1.9.4', 5], ['1.13.2', 6], ['1.14.4', 7], ['1.16.5', 7], ['1.20.4', 8], ['26.3', 8]]) {
            const bot = fakeBot(version);
            rideable(bot);
            bot.client.emit('entity_metadata', { entityId: ID, metadata: [{ key: index, value: 10 }, { key: index + 1, value: -1 }, { key: index + 2, value: 50 }, { key: index + 5, value: 99 }] });
            const cart = bot.tracker.carts.get(ID);
            assert.deepEqual([cart.effects.hurtTime, cart.hurtDir, cart.effects.damage], [10, -1, 50], version);
        }
    });

    it('runs the hurt down between packets', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        bot.client.emit('entity_metadata', { entityId: ID, metadata: [{ key: 8, value: 2 }, { key: 10, value: 1.5 }] });
        bot.tracker.tick();
        const cart = bot.tracker.carts.get(ID);
        assert.deepEqual([cart.effects.hurtTime, cart.effects.damage], [1, 0.5]);
    });

    it('primes a client fuse at 80 by the event of its release and goes no further than 0', () => {
        for (const [version, event] of [['1.20.4', 10], ['26.1', 10], ['26.2', 70], ['26.3', 70]]) {
            const bot = fakeBot(version);
            bot.tracker.adopt({ id: ID, variant: 'tnt_minecart', position: { x: 0, y: 0, z: 0 }, yRot: 0, xRot: 0 });
            bot.client.emit('entity_status', { entityId: ID, entityStatus: event === 10 ? 70 : 10 });
            assert.equal(bot.tracker.carts.get(ID).effects.fuse, -1, version);
            bot.client.emit('entity_status', { entityId: ID, entityStatus: event });
            assert.equal(bot.tracker.carts.get(ID).effects.fuse, 80, version);
            for (let i = 0; i < 90; i += 1) bot.tracker.tick();
            // and blows nothing up: the cart goes when the server says so
            assert.equal(bot.tracker.carts.get(ID).effects.fuse, 0, version);
            assert.equal(bot.tracker.carts.has(ID), true, version);
        }
    });

    it('takes no fuse event for what is no TNT cart', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        bot.client.emit('entity_status', { entityId: ID, entityStatus: 10 });
        assert.equal(bot.tracker.carts.get(ID).effects.fuse, -1);
    });

    it('forgets a cart the server removes, and knows nothing of what it never saw', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        bot.client.emit('rel_entity_move', { entityId: 99, dX: 1, dY: 0, dZ: 0, onGround: true });
        bot.client.emit('entity_destroy', { entityIds: [ID] });
        assert.equal(bot.tracker.carts.size, 0);
    });

    it('knows who a cart carries', () => {
        const bot = fakeBot('1.20.4');
        rideable(bot);
        bot.client.emit('set_passengers', { entityId: ID, passengers: [3] });
        assert.deepEqual(bot.tracker.carts.get(ID).passengers, [3]);
    });

    it('walks the steps of the improved path, and takes the feature from the login', () => {
        const bot = fakeBot('26.3');
        bot.client.emit('feature_flags', { features: ['minecraft:minecart_improvements'] });
        assert.equal(bot.tracker.improvements, true);
        const cart = rideable(bot);
        const origin = { x: 0, y: 64, z: 0 };
        bot.client.emit('move_minecart', { entityId: ID, steps: [{ position: { x: 3, y: 64, z: 0 }, velocity: origin, yaw: 64, pitch: 0, weight: 3 }] });
        assert.equal(cart.steps.pending.length, 1);
        assert.equal(cart.steps.pending[0].yRot, 90);
        bot.tracker.tick();
        const after = bot.tracker.carts.get(ID);
        assert.ok(Math.abs(after.position.x - 1) < 1e-6);
        assert.equal(after.steps.pending.length, 0);
        // the improved path has no interpolation of its own: a position goes in at once
        bot.client.emit('entity_teleport', { entityId: ID, x: 9, y: 64, z: 0, dx: 0, dy: 0, dz: 0, yaw: 0, pitch: 0, flags: {}, onGround: true });
        assert.equal(after.position.x, 9);
    });
});

describe('NativeMinecartPhysics side effects', () => {
    const world = () => {
        const registry = require('prismarine-registry')('1.20.4');
        const Block = require('prismarine-block')(registry);
        const air = Block.fromStateId(registry.blocksByName.air.defaultState, 0);
        return { registry, world: Object.assign(new EventEmitter(), { getBlock: () => air, getBlockStateId: () => registry.blocksByName.air.defaultState }) };
    };
    const cart = fields => ({ variant: 'minecart', position: { x: 0.5, y: 10, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yRot: 0, flipped: false, onGround: false, inWater: false, ...fields });

    it('says what a client cannot give it', () => {
        const { registry, world: level } = world();
        const physics = new adapter.NativeMinecartPhysics(registry);
        assert.throws(() => physics.simulate(cart({ variant: 'tnt_minecart' }), level, { improvements: false }), /randomSeed/);
        assert.throws(() => physics.simulate(cart({ variant: 'hopper_minecart', comparatorSignal: 0 }), level, { improvements: false }), /inventory/);
        assert.throws(() => physics.simulate(cart({ position: { x: 0.5, y: -70, z: 0.5 } }), level, { improvements: false }), /minY/);
    });

    it('refuses an inventory on a cart whose tick would drop it', () => {
        // (only MinecartHopper takes and holds items in a tick; a chest cart's slots reach it as comparatorSignal)
        const { registry, world: level } = world();
        const physics = new adapter.NativeMinecartPhysics(registry);
        for (const fields of [{ variant: 'minecart' }, { variant: 'chest_minecart', comparatorSignal: 0 }]) {
            assert.throws(() => physics.simulate(cart({ ...fields, inventory: [] }), level, { improvements: false }), /only a hopper_minecart/, fields.variant);
        }
        assert.doesNotThrow(() => physics.simulate(cart({ variant: 'hopper_minecart', comparatorSignal: 0, inventory: [] }), level, { improvements: false }));
    });

    it('gives the blast of a lit cart back with its random', () => {
        const { registry, world: level } = world();
        const physics = new adapter.NativeMinecartPhysics(registry);
        const tick = physics.simulate(cart({ variant: 'tnt_minecart', randomSeed: 42, fuse: 0 }), level, { improvements: false });
        assert.equal(tick.state.removed, 'exploded');
        assert.equal(tick.explosions.length, 1);
        assert.equal(tick.explosions[0].power, 4);
        assert.equal(tick.state.randomSeed, 197491923327988);
    });

    it('hurts a cart from outside its tick', () => {
        const { registry, world: level } = world();
        const physics = new adapter.NativeMinecartPhysics(registry);
        const tick = physics.hurt(cart({ damage: 35 }), { kind: 'plain', amount: 1 }, level, { improvements: false });
        assert.equal(tick.state.removed, 'destroyed');
        assert.throws(() => physics.hurt(cart(), { kind: 'magic', amount: 1 }, level, { improvements: false }), /no kind of hurt/);
    });

    it('takes an item into a hopper cart, by name', () => {
        const { registry, world: level } = world();
        const physics = new adapter.NativeMinecartPhysics(registry);
        const stack = { item: 'stone', count: 3, maxStack: 64 };
        const box = { min: { x: 0.4, y: 10, z: 0.4 }, max: { x: 0.6, y: 10.2, z: 0.6 } };
        const scene = physics.simulateScene([cart({ variant: 'hopper_minecart', comparatorSignal: 0, inventory: [] })], [], level, { improvements: false }, [{ boundingBox: box, stack }]);
        assert.deepEqual(scene.carts[0].state.inventory[0], stack);
        assert.equal(scene.items[0].stack.count, 0);
        assert.equal(scene.carts[0].state.comparatorSignal, 1);
    });
});
