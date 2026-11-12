'use strict';

// The real integration contract: mineflayer-physics-utils' own EPhysicsCtx.FROM_BOT and PlayerState (not plain states)
// around a bot shaped as mineflayer's is, through the package's public exports. Which plugin ticks is shown, not assumed.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
const { PlayerState } = require('@nxg-org/mineflayer-physics-util/dist/physics/states/playerState');
const adapter = require('..');
const manifest = require('../package.json');
const { botOf } = require('./bot-fixture.cjs');

const RELEASES = ['1.8.8', '1.12.2', '1.16.5', '1.20.4', '1.20.5', '1.21.4', '1.21.5', '1.21.11'];

/** All of a bot a prediction must leave alone. */
const snapshot = bot => structuredClone({
    position: bot.entity.position.toArray(), velocity: bot.entity.velocity.toArray(), onGround: bot.entity.onGround,
    attributes: bot.entity.attributes, effects: bot.entity.effects, metadata: bot.entity.metadata, yaw: bot.entity.yaw,
    pitch: bot.entity.pitch, height: bot.entity.height, width: bot.entity.width, game: bot.game, slots: bot.inventory.slots,
    controlState: bot.controlState, fireworkRocketDuration: bot.fireworkRocketDuration,
});

const ticks = (physics, ctx, world, count) => {
    for (let i = 0; i < count; i += 1) physics.simulate(ctx, world);
};

describe('EPhysicsCtx.FROM_BOT over the native engine', () => {
    it('runs on the dependency the package declares', () => {
        const dependency = require('@nxg-org/mineflayer-physics-util/package.json');
        assert.equal(dependency.version, manifest.devDependencies['@nxg-org/mineflayer-physics-util']);
    });

    for (const version of RELEASES) {
        it(`ticks and applies a real PlayerState of ${version}`, () => {
            const bot = botOf(version);
            const physics = adapter.createNativePhysics(bot.registry);
            assert.equal(physics.version, version);
            const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
            assert.ok(ctx.state instanceof PlayerState);
            ticks(physics, ctx, bot.world, 5);
            assert.ok(ctx.state.pos.distanceTo(bot.entity.position) > 0.1, 'walked');
            assert.equal(ctx.state.pos.y, 64);
            assert.equal(ctx.state.onGround, true);
            ctx.state.apply(bot);
            assert.deepEqual(bot.entity.position.toArray(), ctx.state.pos.toArray());
            assert.deepEqual(bot.entity.velocity.toArray(), ctx.state.vel.toArray());
        });
    }

    it('runs the engine of the exact release on each side of a boundary', () => {
        for (const version of ['1.21.4', '1.21.5']) {
            const bot = botOf(version);
            const { physics } = adapter.createBotPhysics(bot);
            assert.ok(physics instanceof adapter.NATIVE_PHYSICS_ENGINES[version]);
            assert.equal(physics.version, version);
        }
    });

    it('reads synced attributes under each release\'s key, across 1.20.5', () => {
        const pace = (version, key) => {
            const faster = botOf(version);
            if (key) faster.entity.attributes[key] = { value: 0.2, modifiers: [] };
            const physics = adapter.createNativePhysics(faster.registry);
            const ctx = EPhysicsCtx.FROM_BOT(physics, faster);
            ticks(physics, ctx, faster.world, 5);
            return ctx.state.pos.z - 0.5 || ctx.state.pos.x - 0.5;
        };
        for (const version of ['1.20.4', '1.20.5']) {
            const key = adapter.createNativePhysics(botOf(version).registry).attributeKey('movementSpeed');
            assert.ok(key, version);
            assert.ok(Math.abs(pace(version, key)) > 1.5 * Math.abs(pace(version)), version);
        }
    });

    it('takes the effects of the bot', () => {
        const bot = botOf('1.20.4');
        const physics = adapter.createNativePhysics(bot.registry);
        const walked = entity => {
            const ctx = EPhysicsCtx.FROM_BOT(physics, entity);
            ticks(physics, ctx, entity.world, 5);
            return Math.abs(ctx.state.pos.z - 0.5) + Math.abs(ctx.state.pos.x - 0.5);
        };
        const fast = botOf('1.20.4');
        fast.entity.effects[fast.registry.effectsByName.Speed.id] = { amplifier: 2, duration: 100 };
        assert.ok(walked(fast) > walked(bot));
    });

    it('leaves the ticks to whoever owns them: the loader installs no loop', () => {
        const bot = botOf('1.20.4', { physicsEngine: { simulate() { throw new Error('the plugin\'s own'); } } });
        const own = bot.physicsEngine.simulate;
        const listeners = () => bot.eventNames().length;
        const before = listeners();
        adapter.default(bot);
        assert.ok(bot.NativePhysics.NativeWorld);
        assert.equal(listeners(), before);
        assert.equal(bot.physicsEngine.simulate, own);
    });
});

describe('createBotPhysics', () => {
    /** mineflayer's tick (lib/plugins/physics.js): what its loop does with its engine. */
    const pluginTick = (bot, ctx) => bot.physicsEngine.simulate(ctx, bot.world).apply(bot);

    it('makes a context of the bot over its release\'s engine', () => {
        const bot = botOf('1.21.11');
        const setup = adapter.createBotPhysics(bot);
        const ctx = setup.context(EPhysicsCtx);
        assert.ok(ctx.state instanceof PlayerState);
        assert.equal(ctx.ctx, setup.physics);
        ticks(setup.physics, ctx, bot.world, 3);
        assert.ok(ctx.state.pos.distanceTo(bot.entity.position) > 0.05);
        setup.dispose();
    });

    it('refuses what it cannot run before it listens to anything', () => {
        const bot = botOf('1.20.4');
        assert.throws(() => adapter.createBotPhysics(bot, { version: '1.20.2' }), /1\.20\.2/);
        assert.throws(() => adapter.createBotPhysics(botOf('1.20.4', { version: '1.20.2' })), /the bot runs 1\.20\.2, and the engine asked for is 1\.20\.4/);
        assert.throws(() => adapter.createBotPhysics(botOf('1.20.4', { registry: undefined })), /no registry yet/);
        const unknown = botOf('1.20.4');
        unknown.registry = { ...unknown.registry, version: { ...unknown.registry.version, minecraftVersion: '1.20.7' } };
        assert.throws(() => adapter.createBotPhysics(unknown));
        assert.deepEqual(bot.eventNames(), []);
    });

    it('stays one setup a bot until it is disposed', () => {
        const bot = botOf('1.20.4');
        const setup = adapter.createBotPhysics(bot);
        assert.throws(() => adapter.createBotPhysics(bot), /a setup already/);
        setup.dispose();
        setup.dispose();
        adapter.createBotPhysics(bot).dispose();
        assert.throws(() => setup.context(EPhysicsCtx), /disposed/);
        assert.throws(() => setup.installTicks(), /disposed/);
    });

    it('ticks once a tick through the plugin\'s own loop, and hands it back', () => {
        // the loop this relies on: the plugin's engine stays one object, and it calls its simulate
        const plugin = fs.readFileSync(path.join(path.dirname(require.resolve('mineflayer/package.json')), 'lib', 'plugins', 'physics.js'), 'utf8');
        assert.match(plugin, /physics\.simulate\(ectx, world\)\.apply\(bot\)/);
        assert.match(plugin, /bot\.physicsEngine = physics/);

        let own = 0;
        const engine = { simulate(ctx) { own += 1; return ctx.state; } };
        const bot = botOf('1.20.4', { physicsEngine: engine });
        const baseline = bot.eventNames().length;
        const setup = adapter.createBotPhysics(bot);
        assert.throws(() => adapter.createBotPhysics(botOf('1.20.4')).installTicks(), /physics plugin is not loaded/);
        const ctx = setup.context(EPhysicsCtx);
        setup.installTicks();
        setup.installTicks();
        pluginTick(bot, ctx);
        assert.equal(own, 0);
        assert.equal(ctx.state.age, 1);
        assert.deepEqual(bot.entity.position.toArray(), ctx.state.pos.toArray());
        const moved = ctx.state.pos.clone();
        setup.dispose();
        assert.equal(Object.hasOwn(engine, 'simulate'), true);
        pluginTick(bot, ctx);
        assert.equal(own, 1);
        assert.deepEqual(ctx.state.pos.toArray(), moved.toArray());
        assert.equal(bot.eventNames().length, baseline);
    });

    it('takes a simulate of the engine\'s class back to the class', () => {
        class Engine { simulate(ctx) { return ctx.state; } }
        const engine = new Engine();
        const bot = botOf('1.20.4', { physicsEngine: engine });
        const setup = adapter.createBotPhysics(bot);
        setup.installTicks();
        assert.equal(Object.hasOwn(engine, 'simulate'), true);
        setup.dispose();
        assert.equal(Object.hasOwn(engine, 'simulate'), false);
        assert.equal(engine.simulate, Engine.prototype.simulate);
    });

    it('lets go of the worlds and the listeners when the bot ends, however often it is set up', () => {
        const bot = botOf('1.20.4', { physicsEngine: { simulate: ctx => ctx.state } });
        const listeners = emitter => emitter.eventNames().length;
        for (let i = 0; i < 4; i += 1) {
            const setup = adapter.createBotPhysics(bot);
            setup.installTicks();
            const ctx = setup.context(EPhysicsCtx);
            pluginTick(bot, ctx);
            assert.equal(bot.world.listenerCount('blockUpdate'), 1);
            assert.equal(bot.listenerCount('physicsTick'), 1);
            if (i % 2 === 0) bot.emit('end');
            else setup.dispose();
            assert.equal(listeners(bot), 0);
            assert.equal(listeners(bot.world), 0);
        }
    });
});

describe('what a prediction leaves alone', () => {
    it('runs on a clone without touching the bot or the state it came from', () => {
        for (const version of ['1.8.8', '1.16.5', '1.21.11']) {
            const bot = botOf(version);
            bot.entity.attributes[version === '1.8.8' ? 'generic.movementSpeed' : version === '1.16.5' ? 'minecraft:generic.movement_speed' : 'minecraft:movement_speed'] = {
                value: 0.1, modifiers: [{ uuid: '91aeaa56-376b-4498-935b-2f7f68070635', amount: 0.2, operation: 2 }],
            };
            bot.entity.effects[bot.registry.effectsByName.Speed.id] = { amplifier: 0, duration: 200 };
            bot.controlState.sprint = true;
            const physics = adapter.createNativePhysics(bot.registry);
            const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
            const before = snapshot(bot);
            const start = ctx.state.pos.clone();
            const look = EPhysicsCtx.FROM_BOT(physics, bot);
            look.state = ctx.state.clone();
            ticks(physics, look, bot.world, 20);
            assert.ok(look.state.pos.distanceTo(start) > 1, version);
            assert.deepEqual(snapshot(bot), before, version);
            assert.deepEqual(ctx.state.pos.toArray(), start.toArray(), version);
            // what clones share with the bot stays the bot's: a sprint's modifier goes on the clone's own map
            assert.equal(ctx.state.attributes, bot.entity.attributes, version);
            assert.deepEqual(ctx.state.attributes, before.attributes, version);
        }
    });

    it('goes on from cloneState as the state it copies would', () => {
        // PlayerState.clone leaves out the fall, the input's history and the engine's copy of the player (B104, B097)
        const bot = botOf('1.21.11');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        ctx.state.control.forward = true;
        ctx.state.pos.y += 6;
        ctx.state.onGround = false;
        ticks(physics, ctx, bot.world, 12);
        assert.ok(ctx.state.fallDistance > 2);
        ctx.state.attackHit = { strength: 1 };
        const copy = EPhysicsCtx.FROM_BOT(physics, bot);
        copy.state = physics.cloneState(ctx.state);
        assert.equal(copy.state.attackHit ?? null, null);
        ctx.state.attackHit = null;
        assert.equal(copy.state.fallDistance, ctx.state.fallDistance);
        assert.equal(copy.state.prevForwardImpulse, ctx.state.prevForwardImpulse);
        for (let tick = 0; tick < 30; tick += 1) {
            physics.simulate(ctx, bot.world);
            physics.simulate(copy, bot.world);
            assert.deepEqual(copy.state.pos.toArray(), ctx.state.pos.toArray(), `tick ${tick}`);
            assert.deepEqual(copy.state.vel.toArray(), ctx.state.vel.toArray(), `tick ${tick}`);
            assert.equal(copy.state.fallDistance, ctx.state.fallDistance, `tick ${tick}`);
            assert.equal(copy.state.fallDamage, ctx.state.fallDamage, `tick ${tick}`);
        }
    });

    it('hands the library\'s own clone callers cloneState once the engine has ticked the state', () => {
        // EPhysicsCtx.clone and the library's simulators call state.clone(): what PlayerState.clone leaves out must survive it
        const bot = botOf('1.21.11');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        ctx.state.control.forward = true;
        ctx.state.pos.y += 6;
        ctx.state.onGround = false;
        ticks(physics, ctx, bot.world, 12);
        const copy = ctx.clone();
        assert.equal(copy.state.fallDistance, ctx.state.fallDistance);
        assert.equal(copy.state.prevForwardImpulse, ctx.state.prevForwardImpulse);
        // the clone of a clone too, and a key of the state's own never shows the method
        const second = copy.clone();
        assert.ok(!Object.keys(second.state).includes('clone'));
        for (let tick = 0; tick < 30; tick += 1) {
            physics.simulate(ctx, bot.world);
            physics.simulate(copy, bot.world);
            physics.simulate(second, bot.world);
            assert.deepEqual(copy.state.pos.toArray(), ctx.state.pos.toArray(), `tick ${tick}`);
            assert.deepEqual(second.state.vel.toArray(), ctx.state.vel.toArray(), `tick ${tick}`);
            assert.equal(second.state.fallDamage, ctx.state.fallDamage, `tick ${tick}`);
        }
    });

    it('copies a state the engine has never ticked as cloneState does, through the library\'s own clone (B104)', () => {
        // PlayerState.clone builds a new state from the bot and copies its own fields: what a caller set on a fresh state
        // before its first tick (the fall, the input's history, attributes) went missing, and the copy went its own way
        const bot = botOf('1.21.11');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        ctx.state.pos.y += 6;
        ctx.state.onGround = false;
        ctx.state.control.forward = true;
        ctx.state.fallDistance = 3.5;
        ctx.state.prevForwardImpulse = 0.98;
        ctx.state.sprintTriggerTime = 4;
        const key = physics.attributeKey('movementSpeed');
        ctx.state.attributes = { [key]: { value: 0.1, modifiers: [{ id: 'minecraft:test', amount: 0.5, operation: 0 }] } };
        const copy = ctx.clone();
        assert.ok(copy.state instanceof PlayerState);
        assert.equal(copy.state.fallDistance, 3.5);
        assert.equal(copy.state.prevForwardImpulse, 0.98);
        assert.equal(copy.state.sprintTriggerTime, 4);
        copy.state.attributes[key].modifiers[0].amount = 9;
        assert.equal(ctx.state.attributes[key].modifiers[0].amount, 0.5, 'the fork shares no attribute with its state');
        copy.state.attributes[key].modifiers[0].amount = 0.5;
        assert.ok(!Object.keys(copy.state).includes('clone'));
        for (let tick = 0; tick < 20; tick += 1) {
            physics.simulate(ctx, bot.world);
            physics.simulate(copy, bot.world);
            assert.deepEqual(copy.state.pos.toArray(), ctx.state.pos.toArray(), `tick ${tick}`);
            assert.deepEqual(copy.state.vel.toArray(), ctx.state.vel.toArray(), `tick ${tick}`);
            assert.equal(copy.state.fallDistance, ctx.state.fallDistance, `tick ${tick}`);
        }
    });

    it('leaves the library\'s clone to a state of another engine', () => {
        const bot = botOf('1.21.11');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        ctx.state.fallDistance = 2;
        // the same class with an engine that is not this package's: its clone is the library's, which has no fall to copy
        const other = { data: physics.data, supportFeature: name => physics.supportFeature(name), getEffectLevel: () => 0, getEnchantmentLevel: () => 0 };
        ctx.state.ctx = other;
        const copy = ctx.state.clone();
        assert.equal(copy.ctx, other);
        assert.equal(copy.fallDistance, undefined);
    });

    it('keeps a fork apart from its state through nested attributes, effects and controls', () => {
        const bot = botOf('1.21.11');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        const key = physics.attributeKey('movementSpeed');
        ctx.state.attributes = { [key]: { value: 0.1, modifiers: [{ id: 'minecraft:test', amount: 0.5, operation: 0 }] } };
        ctx.state.effects = { 1: { amplifier: 0, duration: 100 } };
        const fork = physics.cloneState(ctx.state);
        fork.attributes[key].modifiers[0].amount = 9;
        fork.attributes[key].value = 7;
        fork.effects[1].amplifier = 3;
        fork.control.forward = !ctx.state.control.forward;
        assert.equal(ctx.state.attributes[key].modifiers[0].amount, 0.5);
        assert.equal(ctx.state.attributes[key].value, 0.1);
        assert.equal(ctx.state.effects[1].amplifier, 0);
        assert.notEqual(fork.control.forward, ctx.state.control.forward);
        // an absent attribute stays absent in the fork, not a zero
        assert.equal(fork.attributes[physics.attributeKey('stepHeight')], undefined);
    });

    it('leaves the bot as it was when a tick fails', () => {
        const bot = botOf('1.21.11');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        const before = snapshot(bot);
        ctx.state.vel.x = 1e9;
        assert.throws(() => physics.simulate(ctx, bot.world), /NativeWorldSync\.sync: the blocks around the entity/);
        assert.deepEqual(snapshot(bot), before);
    });

    it('writes only what apply writes, and the same nested objects stay the bot\'s', () => {
        const bot = botOf('1.20.4');
        const { attributes, effects, metadata, position, velocity } = bot.entity;
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        ticks(physics, ctx, bot.world, 5);
        ctx.state.apply(bot);
        assert.equal(bot.entity.attributes, attributes);
        assert.equal(bot.entity.effects, effects);
        assert.equal(bot.entity.metadata, metadata);
        assert.equal(bot.entity.position, position);
        assert.equal(bot.entity.velocity, velocity);
    });

    it('tells the pushes the eligible entities took, each once a tick, in the order they came', () => {
        const bot = botOf('1.20.4');
        const physics = adapter.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(physics, bot);
        const box = (x, z) => ({ minX: x - 0.3, minY: 64, minZ: z - 0.3, maxX: x + 0.3, maxY: 65.95, maxZ: z + 0.3 });
        const near = (id, name, x, fields = {}) => ({ id, name, type: 'mob', living: true, position: bot.entity.position.offset(x, 0, 0.1), boundingBox: box(0.5 + x, 0.6), ...fields });
        const entities = [near(1, 'zombie', 0.2), near(2, 'zombie', -0.2), near(3, 'horse', 0.1, { isVehicle: true }), near(4, 'zombie', 30)];
        const metadata = structuredClone(entities.map(({ id, position }) => [id, position.toArray()]));
        ctx.state.nearbyEntities = entities;
        physics.simulate(ctx, bot.world);
        const pushed = ctx.state.pushedEntities;
        assert.deepEqual(pushed.map(push => push.id), [1, 2]);
        physics.simulate(ctx, bot.world);
        assert.equal(new Set(ctx.state.pushedEntities.map(push => push.id)).size, ctx.state.pushedEntities.length);
        // the entities themselves are the caller's to move
        assert.deepEqual(entities.map(({ id, position }) => [id, position.toArray()]), metadata);
    });
});
