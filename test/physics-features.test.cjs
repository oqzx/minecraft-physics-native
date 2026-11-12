'use strict';

// mineflayer-physics-utils' feature catalog over the engine's selected release: the dependency's own EPhysicsCtx and states
// with matched registries, both sides of every breakpoint, every registered release, and the catalog's sources read again
// where this machine has them.

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { existsSync, readFileSync } = require('node:fs');
const { join } = require('node:path');
const { describe, it } = require('node:test');
const { Vec3 } = require('vec3');
const registryOf = require('prismarine-registry');
const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
const { PlayerState } = require('@nxg-org/mineflayer-physics-util/dist/physics/states/playerState');
const dependencyCatalog = require('@nxg-org/mineflayer-physics-util/dist/physics/info/features.json');
const minecraftDataFeatures = require('minecraft-data/minecraft-data/data/pc/common/features.json');
const physics = require('..');
const { PHYSICS_FEATURES, livingFlagsSlot, physicsFeature } = require('../dist/mineflayer/physics-features');
const { compareReleases } = require('../dist/mineflayer/blocks');
const { botOf } = require('./bot-fixture.cjs');

const RELEASES = Object.keys(physics.NATIVE_PHYSICS_ENGINES).sort(compareReleases);
const atLeast = (release, version) => compareReleases(release, version) >= 0;

/** What each name must answer: the breakpoints, written out again rather than read from the code they test. */
const EXPECTED = {
    independentLiquidGravity: release => !atLeast(release, '1.13'),
    proportionalLiquidGravity: release => atLeast(release, '1.13'),
    velocityBlocksOnCollision: release => !atLeast(release, '1.15'),
    velocityBlocksOnTop: release => atLeast(release, '1.15'),
    climbUsingJump: release => atLeast(release, '1.14'),
    effectNamesAreRegistryNames: () => false,
    itemUsageMetadata6: release => atLeast(release, '1.10') && !atLeast(release, '1.14'),
    attributesPrefixedByMinecraft: release => atLeast(release, '1.16') && !atLeast(release, '1.20.5'),
};

const engineOf = version => new physics.NativePhysics(registryOf(version));

describe('the physics feature catalog', () => {
    it('is every name the dependency asks, and the registered releases are the fifty', () => {
        assert.deepEqual([...PHYSICS_FEATURES].sort(), dependencyCatalog.map(feature => feature.name).sort());
        assert.equal(RELEASES.length, 50);
    });

    it('answers every name for every registered release', () => {
        for (const release of RELEASES) {
            for (const feature of PHYSICS_FEATURES) assert.equal(physicsFeature(release, feature), EXPECTED[feature](release), `${feature} ${release}`);
        }
    });

    it('answers the same through an engine on both sides of every breakpoint', () => {
        for (const release of ['1.8.8', '1.9.4', '1.10.2', '1.12.2', '1.13.2', '1.14.4', '1.15', '1.15.2', '1.16', '1.16.5', '1.17', '1.20.4', '1.20.5', '1.21.11', '26.1', '26.2', '26.3']) {
            const engine = engineOf(release);
            for (const feature of PHYSICS_FEATURES) assert.equal(engine.supportFeature(feature), EXPECTED[feature](release), `${feature} ${release}`);
        }
    });

    it('answers the same when the function leaves the engine, as the dependency passes it', () => {
        const { supportFeature } = engineOf('1.12.2');
        assert.equal(supportFeature('independentLiquidGravity'), true);
    });

    it('keeps 26.x and the releases the dependency lists wrongly or not at all', () => {
        for (const version of ['26.1', '26.2', '26.3']) {
            const engine = engineOf(version);
            assert.equal(engine.supportFeature('proportionalLiquidGravity'), true, version);
            assert.equal(engine.supportFeature('velocityBlocksOnTop'), true, version);
            assert.equal(engine.supportFeature('climbUsingJump'), true, version);
        }
        // features.json has no 1.16 for these, and 1.12 alone for the slot
        assert.equal(engineOf('1.16.5').supportFeature('velocityBlocksOnTop'), true);
        assert.equal(engineOf('1.16.5').supportFeature('climbUsingJump'), true);
        assert.equal(engineOf('1.13.2').supportFeature('itemUsageMetadata6'), true);
    });

    it('calls no effect by a registry name in any release: minecraft-data has only its own names', () => {
        for (const release of RELEASES) {
            for (const effect of registryOf(release).effectsArray) assert.match(effect.name, /^[A-Z][A-Za-z]*$/, release);
        }
    });
});

describe('what is not the catalog', () => {
    it('shares no name with minecraft-data, whose answers pass through for every release', () => {
        const names = minecraftDataFeatures.map(feature => feature.name);
        for (const name of PHYSICS_FEATURES) assert.ok(!names.includes(name), name);
        for (const release of ['1.8.8', '1.12.2', '1.13.2', '1.16.5', '1.20.5', '1.21.11', '26.3']) {
            const registry = registryOf(release);
            const engine = new physics.NativePhysics(registry);
            for (const name of names) assert.equal(engine.supportFeature(name), !!registry.supportFeature(name), `${name} ${release}`);
        }
    });

    it('is a yes or no for a feature with a value, which stays the registry\'s', () => {
        const registry = registryOf('1.20.4');
        const engine = new physics.NativePhysics(registry);
        assert.equal(typeof registry.supportFeature('metadataIxOfItem'), 'number');
        assert.equal(engine.supportFeature('metadataIxOfItem'), true);
    });

    it('answers no to a name neither catalog has, and to names of Object', () => {
        const engine = engineOf('26.3');
        assert.equal(engine.supportFeature('doesNotExist'), false);
        for (const name of ['constructor', 'toString', '__proto__']) assert.equal(physicsFeature('26.3', name), undefined, name);
    });

    it('answers the catalog without a registry that has supportFeature, and no to the rest', () => {
        const registry = Object.create(registryOf('1.13.2'));
        registry.supportFeature = undefined;
        const engine = new physics.NativePhysics(registry);
        assert.equal(engine.supportFeature('proportionalLiquidGravity'), true);
        assert.equal(engine.supportFeature('metadataIxOfItem'), false);
    });

    it('never answers for a release it does not run: a registry outside the fifty builds no engine', () => {
        assert.throws(() => new physics.NativePhysics(registryOf('1.7.10')), /1\.7/);
        assert.throws(() => new physics.NativePhysics(registryOf('1.16.5'), { version: '1.17' }), /cannot serve Minecraft/);
    });
});

describe('the dependency\'s EPhysicsCtx over the engine', () => {
    const liquid = version => {
        const ctx = EPhysicsCtx.FROM_BOT(engineOf(version), botOf(version));
        return [ctx.waterGravity, ctx.lavaGravity];
    };

    it('takes the constant on the old side of 1.13 and the proportion from it, 26.x included', () => {
        for (const version of ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2']) assert.deepEqual(liquid(version), [0.02, 0.02], version);
        for (const version of ['1.13.2', '1.14.4', '1.21.11', '26.1', '26.2', '26.3']) assert.deepEqual(liquid(version), [0.08 / 16, 0.08 / 4], version);
    });

    it('scales the proportion with an entity\'s own gravity, which the fallback would not', () => {
        for (const version of ['1.13.2', '26.3']) {
            const engine = engineOf(version);
            EPhysicsCtx.loadData(engine.data);
            const ctx = EPhysicsCtx.FROM_ENTITY_TYPE(engine, { id: -1, type: 'projectile', name: 'snowball', height: 0.25, width: 0.25 });
            assert.equal(ctx.gravity, 0.03, version);
            assert.deepEqual([ctx.waterGravity, ctx.lavaGravity], [0.03 / 16, 0.03 / 4], version);
        }
    });
});

describe('the item-use slot', () => {
    /** A bot of the release using an item, its tracked data with the living flags at their slot and health after them. */
    const usingBot = (version, flags, fields = {}) => {
        const bot = botOf(version, { usingHeldItem: true, ...fields });
        const slot = livingFlagsSlot(version);
        bot.entity.metadata = [];
        if (slot !== null) {
            bot.entity.metadata[slot] = flags;
            bot.entity.metadata[slot + 1] = 20;
        }
        return bot;
    };

    const stateOf = (version, flags) => new PlayerState(engineOf(version), usingBot(version, flags));

    it('is the tracked field\'s slot in every release', () => {
        const expected = { '1.8.8': null, '1.9.4': 5, '1.10.2': 6, '1.12.2': 6, '1.13.2': 6, '1.14.4': 7, '1.16.5': 7, '1.17': 8, '1.20.6': 8, '1.21.11': 8, '26.3': 8 };
        for (const [version, slot] of Object.entries(expected)) assert.equal(livingFlagsSlot(version), slot, version);
    });

    it('reads the hand through the dependency where one of its two slots holds the field', () => {
        for (const version of ['1.10.2', '1.12.2', '1.13.2', '1.17', '26.3']) {
            assert.equal(stateOf(version, 3).isUsingOffHand, true, version);
            assert.equal(stateOf(version, 3).isUsingMainHand, false, version);
            assert.equal(stateOf(version, 1).isUsingOffHand, false, version);
            assert.equal(stateOf(version, 1).isUsingMainHand, true, version);
        }
    });

    it('cannot be right through the dependency where the field is at neither slot, so the adapter does not ask it', () => {
        for (const version of ['1.14.4', '1.15.2', '1.16.5']) {
            // 1 is the main hand, but the dependency reads health at 8 for `> 2`
            const engine = engineOf(version);
            const state = new PlayerState(engine, usingBot(version, 1));
            assert.equal(state.isUsingOffHand, true, version);
            assert.equal(engine.usingOffHand(state), false, version);
        }
    });

    it('takes the hand from the tracked flags on both sides of each move of the slot', () => {
        const hand = (version, flags) => {
            const engine = engineOf(version);
            return engine.usingOffHand(new PlayerState(engine, usingBot(version, flags)));
        };
        for (const version of ['1.13.2', '1.14.4', '1.16.5', '1.17', '26.3']) {
            assert.equal(hand(version, 3), true, version);
            assert.equal(hand(version, 1), false, version);
            // a spin attack's bit is no hand, though the dependency's `> 2` calls it the off hand
            assert.equal(hand(version, 5), false, version);
        }
    });

    it('keeps the state\'s word where there are no tracked flags to read', () => {
        const engine = engineOf('1.8.8');
        assert.equal(engine.usingOffHand({ isUsingOffHand: true, bot: { entity: { metadata: [] } } }), true);
        assert.equal(engine.usingOffHand({ isUsingOffHand: false }), false);
    });
});

describe('the native engine\'s own liquid gravity', () => {
    const waterWorld = (registry, Block) => {
        const stateOf = block => block.defaultState ?? block.id << 4;
        const water = stateOf(registry.blocksByName.water);
        const air = stateOf(registry.blocksByName.air);
        const stateAt = pos => (Math.floor(pos.y) >= 60 && Math.floor(pos.y) <= 70 ? water : air);
        return Object.assign(new EventEmitter(), {
            getBlock: pos => Object.assign(Block.fromStateId(stateAt(pos), 0), { position: new Vec3(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)) }),
            getBlockStateId: stateAt,
        });
    };

    /** The velocity after a tick in deep water from rest, with the dependency's settings left or overwritten. */
    const fallInWater = (version, overwrite) => {
        const registry = registryOf(version);
        const world = waterWorld(registry, require('prismarine-block')(registry));
        const engine = new physics.NativePhysics(registry);
        const bot = botOf(version, { world, controlState: { forward: false, back: false, left: false, right: false, jump: false, sprint: false, sneak: false } });
        bot.entity.onGround = false;
        const ctx = EPhysicsCtx.FROM_BOT(engine, bot);
        const settings = [ctx.waterGravity, ctx.lavaGravity];
        if (overwrite) Object.assign(ctx, { waterGravity: 7, lavaGravity: 9 });
        engine.simulate(ctx, world);
        if (!overwrite) assert.deepEqual([ctx.waterGravity, ctx.lavaGravity], settings, 'a tick leaves the settings alone');
        return ctx.state.vel.y;
    };

    it('applies vanilla\'s gravity once, whatever the dependency\'s settings say', () => {
        for (const [version, gravity] of [['1.12.2', 0.02], ['1.13.2', 0.005], ['26.3', 0.005]]) {
            const vy = fallInWater(version, false);
            assert.ok(Math.abs(vy + gravity) < 1e-9, `${version} ${vy}`);
            assert.equal(fallInWater(version, true), vy, version);
        }
    });
});

describe('the sources of the breakpoints', () => {
    const CACHE = 'E:/mc-physics-demo-wt/source-cache';
    const LEGACY = require('../scripts/source-roots.cjs').legacySourcesRoot();
    const SOURCE = 'E:/pr/mc-source';
    /** The tree of a release, Mojang-named: a legacy release's is its own jar decompiled (scripts/materialize-sources.cjs). */
    const treeOf = release => {
        for (const root of [join(SOURCE, release, 'client'), join(CACHE, release, 'client'), join(LEGACY, release)]) {
            if (existsSync(join(root, 'net/minecraft/world/entity/Entity.java'))) return { root, modern: true };
        }
        return undefined;
    };
    const skip = !treeOf('26.3');
    const read = (tree, path) => readFileSync(join(tree.root, path), 'utf8');

    it('has a tree for every registered release', { skip }, () => {
        assert.deepEqual(RELEASES.filter(release => !treeOf(release)), []);
    });

    it('agrees with every release\'s LivingEntity and Entity', { skip }, () => {
        for (const release of RELEASES) {
            const tree = treeOf(release);
            const dir = tree.modern ? 'net/minecraft/world/entity' : 'net/minecraft/entity';
            const living = read(tree, `${dir}/LivingEntity.java`);
            const entity = read(tree, `${dir}/Entity.java`);
            assert.equal(/\/ 16\.0D?\b/.test(living), EXPECTED.proportionalLiquidGravity(release), `water gravity ${release}`);
            assert.equal(/\/ 4\.0D?\b/.test(living), EXPECTED.proportionalLiquidGravity(release), `lava gravity ${release}`);
            assert.equal(/\(this\.horizontalCollision \|\| this\.jumping\)/.test(living), EXPECTED.climbUsingJump(release), `climb ${release}`);
            const speedFactor = tree.modern ? /getBlockSpeedFactor/.test(entity) : /getVelocityMultiplier/.test(entity);
            assert.equal(speedFactor, EXPECTED.velocityBlocksOnTop(release), `block speed factor ${release}`);
            if (release === '1.8.8') continue; // 1.8's tracker is by number
            const tracked = (entity.match(tree.modern ? /SynchedEntityData\.(<[^(]*>)?defineId\(/g : /DataTracker\.registerData\(Entity\.class/g) ?? []).length;
            assert.equal(tracked, livingFlagsSlot(release), `tracked data of Entity ${release}`);
        }
    });

    it('has 1.8 track no living flags, and soul sand stop slowing in its own collision at 1.15', { skip }, () => {
        assert.doesNotMatch(read(treeOf('1.8.8'), 'net/minecraft/world/entity/LivingEntity.java'), /track\(5,/);
        const inside = release => /entityInside/.test(read(treeOf(release), 'net/minecraft/world/level/block/SoulsandBlock.java'));
        assert.equal(inside('1.14.4'), true);
        assert.equal(inside('1.15'), false);
        // (the recorder's mapping keeps Yarn's velocityX where 1.14.4 has no such field)
        assert.match(read(treeOf('1.12.2'), 'net/minecraft/world/level/block/SoulsandBlock.java'), /\.velocityX \*= 0\.4/);
    });

    it('has the attribute key change form at 1.16 and at 1.20.5', { skip }, () => {
        const packet = 'net/minecraft/network/protocol/game/ClientboundUpdateAttributesPacket.java';
        assert.match(read(treeOf('1.15.2'), packet), /writeUtf\(/);
        assert.match(read(treeOf('1.16'), packet), /writeResourceLocation\(Registry\.ATTRIBUTE\.getKey/);
        assert.match(read(treeOf('1.20.4'), packet), /writeResourceLocation\(BuiltInRegistries\.ATTRIBUTE\.getKey/);
        assert.match(read(treeOf('1.20.5'), packet), /Holder<Attribute>/);
        assert.match(read(treeOf('1.13.2'), packet), /\.writeUtf\(/);
    });
});
