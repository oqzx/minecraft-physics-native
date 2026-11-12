// The mineflayer adapter (dist/mineflayer) on small prismarine worlds: the NativeWorld it keeps in step, and the player
// ticks it runs. Run after `pnpm build` and the native build: node --test test/
const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const adapter = require('..');
const { LoadNativePhysics } = require('../dist/native');

const f32 = Math.fround;

/** A prismarine world of the given blocks ("x,y,z" -> state id), air elsewhere. */
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
        setBlock(x, y, z, id) {
            blocks.set(`${x},${y},${z}`, id);
            world.emit('blockUpdate', null, { position: new Vec3(x, y, z) });
        },
    });
    return { registry, Block, world };
}

/** prismarine-world's view of a world whose chunk columns from x 16 on are not loaded: no block there, state id 0. */
function unloadedFromX16(world) {
    const { getBlock, getBlockStateId } = world;
    const loaded = pos => Math.floor(pos.x) < 16;
    return Object.assign(world, {
        getColumnAt: pos => (loaded(pos) ? {} : null),
        getBlock: pos => (loaded(pos) ? getBlock(pos) : null),
        getBlockStateId: pos => (loaded(pos) ? getBlockStateId(pos) : 0),
    });
}

/** A stone floor at y 0 from -4 to 4. */
const floor = ({ registry, set }) => {
    const stone = registry.blocksByName.stone.defaultState ?? registry.blocksByName.stone.id << 4;
    for (let x = -4; x <= 4; x += 1) for (let z = -4; z <= 4; z += 1) set(x, 0, z, stone);
};

const controls = (keys = {}) => ({ forward: false, back: false, left: false, right: false, jump: false, sprint: false, sneak: false, ...keys });

/** The fields of mineflayer-physics-utils' PlayerState the adapter reads, for a player standing at (x, y, z). */
function playerState(x, y, z, fields = {}) {
    const state = {
        age: 0, pos: new Vec3(x, y, z), vel: new Vec3(0, 0, 0), onGround: false, lastOnGround: false, onClimbable: false,
        isInWater: false, isUnderWater: false, isInLava: false, isInWeb: false, isCollidedHorizontally: false,
        isCollidedHorizontallyMinor: false, isCollidedVertically: false, fallDistance: 0, scale: 1, isInPowderSnow: false,
        wasInPowderSnow: false, keptBoundingBox: null, stuckSpeedMultiplier: new Vec3(0, 0, 0), nearbyEntities: [],
        jumpTicks: 0, sprintTriggerTime: 0, prevForwardImpulse: 0,
        autoJumpEnabled: false, autoJumpTime: 0, autoJumping: false, flyJumpTriggerTime: 0, attributes: {}, yaw: Math.PI,
        yawDegrees: 0, pitch: 0, control: controls(), prevControl: controls(), heading: { forward: 0, strafe: 0 },
        prevHeading: { forward: 0, strafe: 0 }, isUsingItem: false, effects: {}, jumpBoost: 0, speed: 0, slowness: 0,
        dolphinsGrace: 0, slowFalling: 0, levitation: 0, blindness: 0, depthStrider: 0, swiftSneak: 0, riptide: 0,
        itemUseTicks: 0, autoSpinAttackTicks: 0, soulSpeed: 0, pose: 0, gameMode: 'survival', food: 20, mayFly: false,
        flying: false, swimming: false, sprinting: false, crouching: false, fallFlying: false, flySpeed: f32(0.05),
        validElytraEquipped: false, canWalkOnPowderSnow: false, fireworkRocketDuration: 0, fireworkRockets: 0,
        fireworkRocketsBeforePlayer: 0, supportingBlockPos: null,
        getBB() {
            return { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 };
        },
        ...fields,
    };
    return { state };
}

/** The fields of mineflayer-physics-utils' RiddenMobState the adapter reads, for a mob of that type at (x, y, z). */
function riddenState(type, x, y, z, fields = {}) {
    const state = {
        age: 0, type, pos: new Vec3(x, y, z), vel: new Vec3(0, 0, 0), onGround: false, lastOnGround: false, isInWater: false,
        isInLava: false, isCollidedHorizontally: false, isCollidedVertically: false, fallDistance: 0, yawDegrees: 0,
        pitchDegrees: 0, movementSpeed: 0.22499999403953552, jumpStrength: 0.7, flyingSpeed: 0.05, riddenSpeed: 0,
        boosting: false, boostTime: 0, boostTimeTotal: 0, suffocating: false, noAi: false, jumpPendingScale: 0,
        dashCooldown: 0, dashing: false, camelSitting: false, poseTime: 1000, horseJumping: false, standing: false,
        allowStandSliding: false, saddled: true, ownForward: 0, standCounter: 0, standAnim: 0, standAnimO: 0,
        keptBoundingBox: null, dimensions: { width: f32(1.3964844), height: f32(1.6) }, pitch: 0, lerpTarget: null,
        lerpSteps: 0, lerpPrevious: null, lerpStart: null, lerpTicks: 0, lerpYaw: null, lerpPitch: 0,
        lerpPreviousRotation: null, lerpStartRotation: null,
        getBB() {
            return { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 };
        },
        ...fields,
    };
    return { state };
}

/** A world of one chunk column made of `sections` (null where one is absent) from `minY` on, all air. */
function sectionedWorld(version, sections, minY) {
    const { registry, world } = worldOf(version, () => {});
    const column = { sections, minY };
    return { registry, world: Object.assign(world, { getColumnAt: pos => (Math.floor(pos.x) >> 4 === 0 && Math.floor(pos.z) >> 4 === 0 ? column : null) }), column };
}

describe('NativeWorldSync section presence and world height', () => {
    const inColumn = { minX: 4, minY: 4, minZ: 4, maxX: 5, maxY: 5, maxZ: 5 };

    it('tells an all-air section from an absent one and from an unloaded column', () => {
        const { registry, world } = sectionedWorld('1.16.5', [{}, null, {}], undefined);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, inColumn, 0);
        assert.equal(sync.world.isChunkLoaded(0, 0), true);
        assert.equal(sync.world.isSectionPresent(0, 0, 0), true);
        assert.equal(sync.world.isSectionPresent(0, 1, 0), false);
        assert.equal(sync.world.isSectionPresent(0, 2, 0), true);
        assert.equal(sync.world.isSectionPresent(0, 3, 0), false);
        assert.equal(sync.world.isSectionPresent(3, 0, 3), false);
    });

    it('counts the sections from the bottom of the level, below zero', () => {
        const { registry, world } = sectionedWorld('1.21.1', [{}, null, {}], -64);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: 4, minY: -60, minZ: 4, maxX: 5, maxY: -59, maxZ: 5 }, 0);
        assert.equal(sync.world.isSectionPresent(0, -4, 0), true);
        assert.equal(sync.world.isSectionPresent(0, -3, 0), false);
        assert.equal(sync.world.isSectionPresent(0, -2, 0), true);
    });

    it('reads the sections of a column loaded anew, and the one a block change made', () => {
        const { registry, world, column } = sectionedWorld('1.16.5', [{}, null], undefined);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, inColumn, 0);
        column.sections = [null, null];
        world.emit('chunkColumnLoad', new Vec3(0, 0, 0));
        sync.sync(world, inColumn, 0);
        assert.equal(sync.world.isSectionPresent(0, 0, 0), false);
        column.sections = [null, {}];
        world.setBlock(8, 20, 8, registry.blocksByName.stone.defaultState);
        sync.sync(world, inColumn, 0);
        assert.equal(sync.world.isSectionPresent(0, 1, 0), true);
    });

    it('takes a new height with the border unchanged, and a height without a border', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        const heights = () => {
            const bounds = sync.world.worldBounds();
            return [bounds.minBuildHeight, bounds.maxBuildHeight, bounds.borderSize];
        };
        const border = { centerX: 0, centerZ: 0, size: 100 };
        sync.setWorldBorder(border, { minY: -64, height: 384 });
        assert.deepEqual(heights(), [-64, 320, 100]);
        sync.setWorldBorder(border, { minY: 0, height: 256 });
        assert.deepEqual(heights(), [0, 256, 100]);
        // no border heard of: the height still follows the dimension and the last border stays
        sync.setWorldBorder(undefined, { minY: -32, height: 96 });
        assert.deepEqual(heights(), [-32, 64, 100]);
        sync.setWorldBorder({ centerX: NaN, centerZ: 0, size: 5 }, { minY: 16, height: 32 });
        assert.deepEqual(heights(), [16, 48, 100]);
    });

    it('falls back to the height of the dimension in the release where the server told none', () => {
        const bounds = (version, game) => {
            const { registry } = worldOf(version, () => {});
            const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
            sync.setWorldBorder(undefined, game);
            const { minBuildHeight, maxBuildHeight, borderSize } = sync.world.worldBounds();
            return [minBuildHeight, maxBuildHeight, borderSize];
        };
        assert.deepEqual(bounds('1.8.8', undefined), [0, 256, 59999968]);
        assert.deepEqual(bounds('1.17.1', { dimension: 'overworld' }), [0, 256, 59999968]);
        assert.deepEqual(bounds('1.18.2', { dimension: 'overworld' }), [-64, 320, 59999968]);
        assert.deepEqual(bounds('1.18.2', { dimension: 'the_nether' }), [0, 256, 59999968]);
        assert.deepEqual(bounds('1.21.1', { dimension: 'the_end' }), [0, 256, 59999968]);
        // a server's own dimension keeps its numbers, in any release
        assert.deepEqual(bounds('1.21.1', { dimension: 'custom:deep', minY: -2032, height: 4064 }), [-2032, 2032, 59999968]);
        // a height of 0 was never told: its minY alone says nothing
        assert.deepEqual(bounds('1.21.1', { dimension: 'overworld', minY: 16, height: 0 }), [-64, 320, 59999968]);
    });
});

describe('NativeWorldSync', () => {
    it('writes the blocks around an entity and the ones a world changes', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        const around = { minX: -0.3, minY: 1, minZ: -0.3, maxX: 0.3, maxY: 2.8, maxZ: 0.3 };
        sync.sync(world, around, 2);
        assert.deepEqual(sync.collisions({ minX: -0.5, minY: 0.5, minZ: -0.5, maxX: 0.5, maxY: 1.5, maxZ: 0.5 }).length, 4);
        world.setBlock(0, 1, 0, registry.blocksByName.stone.defaultState);
        sync.sync(world, around, 2);
        const box = sync.collisions({ minX: 0.1, minY: 1.1, minZ: 0.1, maxX: 0.9, maxY: 1.9, maxZ: 0.9 });
        assert.deepEqual(box.map(({ min, max }) => [min.y, max.y]), [[1, 2]]);
    });

    it('gives a moving_piston cell the shape of its block entity, until it ends', () => {
        // MovingPistonBlock.getCollisionShape (26.3 :86-89): the block has no shape of its own
        const { registry, world, Block } = worldOf('1.21.1', ({ registry: r, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, r.blocksByName.moving_piston.defaultState);
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        const around = { minX: -0.3, minY: 1, minZ: -0.3, maxX: 0.3, maxY: 2.8, maxZ: 0.3 };
        const query = { minX: 0.1, minY: 0.4, minZ: 0.1, maxX: 0.9, maxY: 1.9, maxZ: 0.9 };
        const tops = () => sync.collisions(query).map(({ min, max }) => [min.y, max.y]);
        sync.sync(world, around, 2);
        assert.deepEqual(tops(), [[0, 1]]);
        const stone = Block.fromStateId(registry.blocksByName.stone.defaultState, 0);
        const rising = { x: 0, y: 1, z: 0, facing: 1, extending: true, source: false, block: stone };
        sync.setMovingBlocks([{ ...rising, progress: 0.5 }]);
        assert.deepEqual(tops(), [[0, 1], [0.5, 1.5]]);
        // the cell's own boxes, for the adapter's own queries (a pose's fit): in the cell, and out of it
        assert.deepEqual(sync.movingBoxesAt(0, 1, 0), [[0, -0.5, 0, 1, 0.5, 1]]);
        assert.deepEqual(sync.boxesAt(0, 1, 0), [[0, -0.5, 0, 1, 0.5, 1]]);
        // the next tick's progress replaces it, and the block entity's end leaves the cell empty
        sync.setMovingBlocks([{ ...rising, progress: 1 }]);
        assert.deepEqual(tops(), [[0, 1], [1, 2]]);
        sync.setMovingBlocks([]);
        assert.deepEqual(tops(), [[0, 1]]);
        assert.equal(sync.movingBoxesAt(0, 1, 0), undefined);
        // a cell with another block keeps that block's shape
        world.setBlock(0, 1, 0, registry.blocksByName.stone.defaultState);
        sync.sync(world, around, 2);
        sync.setMovingBlocks([{ ...rising, progress: 0.5 }]);
        assert.deepEqual(tops(), [[0, 1], [1, 2]]);
        assert.equal(sync.movingBoxesAt(0, 1, 0), undefined);
    });

    it('follows a piston from its block event, and leaves no moving block when it settles', () => {
        // the server sends no moving_piston block: the client moves them (26.3 PistonBaseBlock.triggerEvent)
        const { registry, world, Block } = worldOf('1.21.1', ({ registry: r, Block: B, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, B.fromProperties('piston', { facing: 'east', extended: false }, 0).stateId);
            set(1, 1, 0, r.blocksByName.stone.defaultState);
        });
        world.setBlockStateId = (pos, id) => world.setBlock(pos.x, pos.y, pos.z, id);
        const physics = new adapter.NativePhysics(registry);
        const bot = new EventEmitter();
        const sync = physics.syncOf(world);
        const around = { minX: 1, minY: 1, minZ: 0, maxX: 3, maxY: 2, maxZ: 1 };
        const query = { minX: 1.01, minY: 1.1, minZ: 0.1, maxX: 3.99, maxY: 1.9, maxZ: 0.9 };
        const spans = () => {
            sync.sync(world, around, 2);
            physics.syncMovingBlocks(sync, world, bot);
            return sync.collisions(query).map(({ min, max }) => [min.x, max.x]).sort((a, b) => a[0] - b[0]);
        };
        assert.deepEqual(spans(), [[1, 2]]);
        bot.emit('pistonMove', Object.assign(Block.fromStateId(world.getBlockStateId({ x: 0, y: 1, z: 0 }), 0), { position: new Vec3(0, 1, 0) }), 0, 5);
        // the stone from its old cell, and the head's plate and arm behind it
        assert.deepEqual(spans().at(-1), [1, 2]);
        bot.emit('physicsTick');
        assert.deepEqual(spans().at(-1), [1.5, 2.5]);
        bot.emit('physicsTick');
        assert.deepEqual(spans().at(-1), [2, 3]);
        bot.emit('physicsTick');
        assert.equal(world.getBlock({ x: 2, y: 1, z: 0 }).name, 'stone');
        assert.deepEqual(spans().at(-1), [2, 3]);
        // what the server sets over a moving block ends it: the air is not given a moved shape
        bot.emit('pistonMove', Object.assign(Block.fromStateId(world.getBlockStateId({ x: 0, y: 1, z: 0 }), 0), { position: new Vec3(0, 1, 0) }), 1, 5);
        spans();
        world.setBlock(0, 1, 0, registry.blocksByName.stone.defaultState);
        bot.emit('blockUpdate', null, { name: 'stone', position: new Vec3(0, 1, 0) });
        assert.deepEqual(spans().filter(([min]) => min < 1.5), []);
    });

    describe('what a piston moves', () => {
        // PistonStructureResolver (26.3 PistonStructureResolver.java, 1.8.9 PistonHandler.java); clear of the floor, which a slime block holds
        const LIFT = 4;
        const moved = (version, blocks, name = 'piston', event = 0) => {
            const { registry, world, Block } = worldOf(version, ({ registry: r, Block: B, set }) => {
                floor({ registry: r, set });
                set(0, LIFT + 1, 0, B.fromProperties(name, { facing: 'east', extended: event !== 0 }, 0).stateId);
                for (const [x, y, z, block] of blocks) set(x, LIFT + y, z, r.blocksByName[block].defaultState);
            });
            assert.ok(Block);
            world.setBlockStateId = (pos, id) => world.setBlock(pos.x, pos.y, pos.z, id);
            const tracker = new adapter.MovingBlockTracker(registry, version, world);
            tracker.trigger(name, [0, LIFT + 1, 0], event, 5);
            return tracker.blocks().filter(entry => !entry.source && entry.block.name !== 'piston_head' && entry.block.name !== name)
                .map(({ x, y, z, block }) => `${block.name} ${x},${y - LIFT},${z}`).sort();
        };

        it('moves the line in front of it alone without a sticky block', () => {
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'stone'], [2, 1, 0, 'dirt'], [1, 2, 0, 'stone']]), ['dirt 3,1,0', 'stone 2,1,0']);
        });

        it('takes what a slime block holds, on every side but its faces along the push', () => {
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'slime_block'], [1, 2, 0, 'stone'], [1, 1, 1, 'dirt']]),
                ['dirt 2,1,1', 'slime_block 2,1,0', 'stone 2,2,0']);
        });

        it('leaves honey and slime apart, from 1.15', () => {
            // canStickToEachOther (1.15 PistonHandler.java:68-73, 26.3 :64-72)
            const glued = [[1, 1, 0, 'slime_block'], [1, 2, 0, 'honey_block'], [1, 3, 0, 'stone']];
            assert.deepEqual(moved('1.21.1', glued), ['slime_block 2,1,0']);
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'honey_block'], [1, 2, 0, 'dirt']]), ['dirt 2,2,0', 'honey_block 2,1,0']);
        });

        it('follows a line a branch starts, and refuses a thirteenth block', () => {
            const column = [[1, 1, 0, 'slime_block'], ...Array.from({ length: 4 }, (_, i) => [1, 2 + i, 0, 'slime_block'])];
            assert.equal(moved('1.21.1', column).length, 5);
            const long = Array.from({ length: 13 }, (_, i) => [1 + i, 1, 0, 'stone']);
            assert.deepEqual(moved('1.21.1', long), []);
            assert.equal(moved('1.21.1', long.slice(0, 12)).length, 12);
        });

        it('asks the table of the release: a rail moves, a block entity stops the line', () => {
            // no collision box says nothing of a piston (26.3 Blocks.java RAIL has no pushReaction), and hasBlockEntity refuses
            // what is PUSH_PULL (PistonBaseBlock.isPushable, 26.3 :235)
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'rail']]), ['rail 2,1,0']);
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'stone'], [2, 1, 0, 'barrel']]), []);
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'stone'], [2, 1, 0, 'oak_sign']]), []);
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'stone'], [2, 1, 0, 'bell']]), ['stone 2,1,0']);
        });

        it('stops at what cannot move, for the line and for the branch', () => {
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'stone'], [2, 1, 0, 'obsidian']]), []);
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'slime_block'], [2, 2, 0, 'stone'], [2, 1, 0, 'stone'], [3, 1, 0, 'bedrock']]), []);
            // a branch's obsidian is left behind (addBlockLine returns true)
            assert.deepEqual(moved('1.21.1', [[1, 1, 0, 'slime_block'], [1, 2, 0, 'obsidian']]), ['slime_block 2,1,0']);
        });

        it('drags with a sticky piston what it holds', () => {
            assert.deepEqual(moved('1.21.1', [[2, 1, 0, 'slime_block'], [2, 2, 0, 'stone']], 'sticky_piston', 1), ['slime_block 1,1,0', 'stone 1,2,0']);
            assert.deepEqual(moved('1.21.1', [[2, 1, 0, 'obsidian']], 'sticky_piston', 1), []);
        });

        it('branches from a slime block on the oldest release, where no honey exists', () => {
            assert.deepEqual(moved('1.14.4', [[1, 1, 0, 'slime_block'], [1, 2, 0, 'stone']]), ['slime_block 2,1,0', 'stone 2,2,0']);
        });
    });

    describe('its lifetime', () => {
        const listeners = emitter => Object.fromEntries(emitter.eventNames().map(name => [name, emitter.listenerCount(name)]));
        const box = { minX: 0, minY: 1, minZ: 0, maxX: 1, maxY: 2, maxZ: 1 };
        const stoneWorld = version => worldOf(version, ({ registry, set }) => floor({ registry, set }));
        const syncOf = (registry) => new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);

        it('takes its listeners off a world it leaves or is disposed of, however often', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const other = stoneWorld('1.21.1').world;
            const baseline = listeners(world);
            const sync = syncOf(registry);
            for (let i = 0; i < 5; i += 1) {
                sync.sync(world, box, 1);
                assert.equal(world.listenerCount('blockUpdate'), 1);
                sync.sync(other, box, 1);
                assert.deepEqual(listeners(world), baseline);
                assert.equal(other.listenerCount('blockUpdate'), 1);
            }
            sync.dispose();
            sync.dispose();
            assert.deepEqual(listeners(other), baseline);
        });

        it('falls back on removeListener and survives a world with no way to take a listener off', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const held = [];
            const noOff = { getBlock: world.getBlock, getBlockStateId: world.getBlockStateId, on: (name, listener) => held.push(listener) };
            const noRemoval = syncOf(registry);
            noRemoval.sync(noOff, box, 1);
            noRemoval.dispose();
            for (const listener of held) assert.doesNotThrow(() => listener(null, { position: new Vec3(0, 1, 0) }));

            const removed = [];
            const onlyRemove = { ...noOff, on: () => {}, removeListener: (name, listener) => removed.push(name) };
            const sync = syncOf(registry);
            sync.sync(onlyRemove, box, 1);
            sync.dispose();
            assert.deepEqual(removed, ['blockUpdate', 'chunkColumnLoad', 'chunkColumnUnload']);
        });

        it('disposes of a world without events too, and refuses to run after', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const sync = syncOf(registry);
            sync.sync({ getBlock: world.getBlock, getBlockStateId: world.getBlockStateId }, box, 1);
            sync.dispose();
            assert.throws(() => sync.sync(world, box, 1), /NativeWorldSync\.sync: the sync was disposed/);
        });

        it('hears no change of an old or disposed world, and every change of its own', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const other = stoneWorld('1.21.1').world;
            const stone = registry.blocksByName.stone.defaultState;
            const shapes = sync => sync.collisions({ minX: 1, minY: 1, minZ: 1, maxX: 2, maxY: 2, maxZ: 2 }).length;
            const near = { minX: 1, minY: 1, minZ: 1, maxX: 2, maxY: 2, maxZ: 2 };
            const sync = syncOf(registry);
            sync.sync(world, near, 1);
            assert.equal(shapes(sync), 0);
            // its own: a block and a chunk column
            world.setBlock(1, 1, 1, stone);
            sync.sync(world, near, 1);
            assert.equal(shapes(sync), 1);
            sync.sync(other, near, 1);
            // the old one's change is not this world's
            world.setBlock(1, 1, 1, registry.blocksByName.air.defaultState);
            sync.sync(other, near, 1);
            assert.equal(shapes(sync), 0);
            other.setBlock(1, 1, 1, stone);
            sync.sync(other, near, 1);
            assert.equal(shapes(sync), 1);
            const read = sync.world;
            sync.dispose();
            other.setBlock(1, 1, 1, registry.blocksByName.air.defaultState);
            other.emit('chunkColumnUnload', { x: 0, z: 0 });
            sync.blockChanged({ x: 1, y: 1, z: 1 });
            sync.columnChanged(0, 0, false);
            assert.equal(read.isChunkLoaded(0, 0), true);
        });

        it('leaves no listener on a world or a bot the engine let go of', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const physics = new adapter.NativePhysics(registry);
            const bot = new EventEmitter();
            const baseline = [listeners(world), listeners(bot)];
            for (let i = 0; i < 4; i += 1) {
                const sync = physics.syncOf(world);
                sync.sync(world, box, 1);
                physics.syncMovingBlocks(sync, world, bot);
                assert.equal(bot.listenerCount('physicsTick'), 1);
                if (i % 2 === 0) physics.releaseWorld(world);
                else physics.dispose();
                assert.deepEqual([listeners(world), listeners(bot)], baseline);
            }
            physics.dispose();
            physics.releaseWorld(world);
            // the engine runs on after: a new sync of the world
            const sync = physics.syncOf(world);
            sync.sync(world, box, 1);
            assert.equal(world.listenerCount('blockUpdate'), 1);
        });

        it('lets go of the world a bot left, once it moves to another or ends', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const next = stoneWorld('1.21.1').world;
            const physics = new adapter.NativePhysics(registry);
            const bot = new EventEmitter();
            const tick = at => {
                const sync = physics.syncOf(at);
                sync.sync(at, box, 1);
                physics.syncMovingBlocks(sync, at, bot);
            };
            const baseline = listeners(world);
            tick(world);
            tick(world);
            assert.equal(bot.listenerCount('pistonMove'), 1);
            tick(next);
            // one tracker takes the bot's events, of the world it is in
            assert.equal(bot.listenerCount('pistonMove'), 1);
            assert.equal(bot.listenerCount('physicsTick'), 1);
            assert.deepEqual(listeners(world), baseline);
            bot.emit('end');
            assert.deepEqual(bot.eventNames(), []);
            assert.deepEqual(listeners(next), baseline);
        });

        it('refuses a box it cannot read, and reads a large one', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const sync = syncOf(registry);
            const reach = extent => ({ minX: 0, minY: 0, minZ: 0, maxX: extent, maxY: 1, maxZ: 1 });
            assert.throws(() => sync.sync(world, reach(1e9), 1), /NativeWorldSync\.sync: the blocks around the entity/);
            assert.doesNotThrow(() => sync.sync(world, reach(NaN), 1));
            sync.sync(world, reach(1000), 1);
            assert.equal(sync.collisions({ minX: 0, minY: 0, minZ: 0, maxX: 4, maxY: 1, maxZ: 1 }).length, 4);
        });

        it('keeps a second engine on the same world when the first lets go', () => {
            const { registry, world } = stoneWorld('1.21.1');
            const first = new adapter.NativePhysics(registry);
            const second = new adapter.NativePhysics(registry);
            first.syncOf(world).sync(world, box, 1);
            second.syncOf(world).sync(world, box, 1);
            assert.equal(world.listenerCount('blockUpdate'), 2);
            first.dispose();
            assert.equal(world.listenerCount('blockUpdate'), 1);
        });
    });

    it('draws a moving block back on 1.8, where it is not moved', () => {
        // 1.8.9 PistonExtensionBlock.java:122-134, 166-197: progress 0.5F has lastProgress 0.0F, so a stone is flat
        const { registry, world, Block } = worldOf('1.8.8', ({ registry: r, set }) => {
            set(0, 1, 0, r.blocksByName.piston_extension.id << 4);
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: -0.3, minY: 1, minZ: -0.3, maxX: 0.3, maxY: 2.8, maxZ: 0.3 }, 2);
        const stone = Block.fromStateId(registry.blocksByName.stone.id << 4, 0);
        sync.setMovingBlocks([{ x: 0, y: 1, z: 0, facing: 1, extending: true, source: false, progress: 1, block: stone }]);
        const box = sync.collisions({ minX: 0.1, minY: 1.1, minZ: 0.1, maxX: 0.9, maxY: 1.9, maxZ: 0.9 });
        assert.deepEqual(box.map(({ min, max }) => [min.y, max.y]), [[1, 1.5]]);
    });

    it('marks the #fall_damage_resetting blocks for the clip of Entity.move', () => {
        // ClipContext.Block.FALLDAMAGE_RESETTING (1.19 RaycastContext.java:72, 1.21.4 :76): the blocks of the release's
        // #fall_damage_resetting tag (#climbable, cobweb, sweet_berry_bush), boxes or none
        const { registry, world } = worldOf('1.21.4', ({ registry: r, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, r.blocksByName.vine.defaultState);
            set(1, 1, 0, r.blocksByName.ladder.defaultState);
            set(-1, 1, 0, r.blocksByName.cobweb.defaultState);
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: -0.3, minY: 1, minZ: -0.3, maxX: 0.3, maxY: 2.8, maxZ: 0.3 }, 2);
        assert.equal(sync.world.blockAt(0, 1, 0).fallDamageResetting, true);
        assert.equal(sync.world.blockAt(1, 1, 0).fallDamageResetting, true);
        assert.equal(sync.world.blockAt(-1, 1, 0).fallDamageResetting, true);
        assert.equal(sync.world.blockAt(0, 0, 0).fallDamageResetting, false);
    });

    it('joins a fence to its neighbours before 1.13', () => {
        const { registry, world } = worldOf('1.12.2', ({ registry: r, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, r.blocksByName.fence.id << 4);
            set(1, 1, 0, r.blocksByName.stone.id << 4);
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 1);
        const fence = sync.collisions({ minX: 0.3, minY: 1.1, minZ: 0.3, maxX: 0.99, maxY: 2.4, maxZ: 0.7 });
        assert.equal(Math.max(...fence.map(({ max }) => max.x)), 1);
        assert.deepEqual([...new Set(fence.map(({ max }) => max.y))], [2.5]);
        const west = sync.collisions({ minX: 0.01, minY: 1.1, minZ: 0.3, maxX: 0.37, maxY: 2.4, maxZ: 0.7 });
        assert.deepEqual(west, []);
    });

    it('joins a 1.12 fence to the solid sides of a corner stair', () => {
        // 1.12.2 BlockFence.canConnectTo (yarn FenceBlock.java:132-137, 161-164) wants the neighbour's face toward the
        // fence SOLID by getBlockFaceShape; BlockStairs' (yarn StairsBlock.java:165-186) resolves the SHAPE first: a
        // straight stair is solid at its facing, an inner corner at two sides, an outer corner at none.
        const stair = (r, facing) => (r.blocksByName.oak_stairs.id << 4) | { east: 0, west: 1, south: 2, north: 3 }[facing];
        const fenceArm = (version, place) => {
            const { registry, world } = worldOf(version, ({ registry: r, set }) => {
                floor({ registry: r, set });
                set(0, 1, 0, r.blocksByName.fence.id << 4);
                place(r, set);
            });
            const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
            const around = { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 };
            sync.sync(world, around, 1);
            const east = () => Math.max(...sync.boxesAt(0, 1, 0).map(box => box[3]));
            return { registry, world, sync, around, east };
        };
        // straight stairs east of the fence: solid toward it (west) only when facing west
        assert.equal(fenceArm('1.12.2', (r, set) => set(1, 1, 0, stair(r, 'west'))).east(), 1);
        assert.equal(fenceArm('1.12.2', (r, set) => set(1, 1, 0, stair(r, 'south'))).east(), 0.625);
        // a south-facing stair with a west-facing one behind it is inner_right, solid at south and west
        const innerRight = (r, set) => {
            set(1, 1, 0, stair(r, 'south'));
            set(1, 1, -1, stair(r, 'west'));
        };
        assert.equal(fenceArm('1.12.2', innerRight).east(), 1);
        // a north-facing stair with a west-facing one behind it is inner_left, solid at north and west
        assert.equal(fenceArm('1.12.2', (r, set) => {
            set(1, 1, 0, stair(r, 'north'));
            set(1, 1, 1, stair(r, 'west'));
        }).east(), 1);
        // with a west-facing stair ahead of it too, the stair is an outer corner, solid nowhere, until a stair beside that
        // one's back, two blocks from the fence, continues it and leaves the inner corner
        const continued = fenceArm('1.12.2', (r, set) => {
            innerRight(r, set);
            set(1, 1, 1, stair(r, 'west'));
        });
        assert.equal(continued.east(), 0.625);
        continued.world.setBlock(2, 1, 0, stair(continued.registry, 'south'));
        continued.sync.sync(continued.world, continued.around, 1);
        assert.equal(continued.east(), 1);
        // before 1.12 a fence joins no stairs (BlockFence.canConnectTo: an opaque full cube)
        assert.equal(fenceArm('1.11.2', innerRight).east(), 0.625);
    });

    it('gives one layer of snow before 1.13 its box of no height', () => {
        // 1.12.2 SnowLayerBlock.java:75-79 (BlockSnow.getCollisionBoundingBox): (layers - 1) * 0.125 high
        for (const version of ['1.8.8', '1.12.2']) {
            const { registry, world } = worldOf(version, ({ registry: r, set }) => {
                set(0, 1, 0, (r.blocksByName.snow_layer.id << 4) | 0);
                set(1, 1, 0, (r.blocksByName.snow_layer.id << 4) | 8);
                set(2, 1, 0, (r.blocksByName.snow_layer.id << 4) | 1);
            });
            const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
            sync.sync(world, { minX: 0.5, minY: 1, minZ: -0.5, maxX: 1.5, maxY: 2, maxZ: 0.5 }, 1);
            assert.deepEqual(sync.boxesAt(0, 1, 0), [[0, 0, 0, 1, 0, 1]], version);
            assert.deepEqual(sync.boxesAt(1, 1, 0), [[0, 0, 0, 1, 0, 1]], version);
            assert.deepEqual(sync.boxesAt(2, 1, 0).map(box => box[4]), [0.125], version);
        }
    });

    it('leaves out the chunk columns a prismarine world has not loaded', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        unloadedFromX16(world);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: 15.2, minY: 1, minZ: 0.2, maxX: 15.8, maxY: 2.8, maxZ: 0.8 }, 2);
        assert.equal(sync.world.isChunkLoaded(0, 0), true);
        assert.equal(sync.world.isChunkLoaded(1, 0), false);
        assert.equal(sync.blockAt(16, 1, 0), null);
    });

    it('reads an unloaded chunk column as empty and the blocks of the column loaded again anew', () => {
        // ClientChunkCache.drop and the EmptyLevelChunk of a column it has not got (client1.18.1 ClientChunkCache.java
        // getChunk, EmptyLevelChunk.java:21-31): air, no fluid, no collision, the block state and entity unset; the column
        // loaded again brings its own blocks, not the ones from before
        const { registry, world } = worldOf('1.21.1', ({ registry: r, set }) => {
            floor({ registry: r, set });
            set(16, 1, 0, r.blocksByName.stone.defaultState);
            set(17, 1, 0, r.blocksByName.water.defaultState);
        });
        let columnLoaded = true;
        const { getBlock, getBlockStateId } = world;
        const loaded = pos => columnLoaded || Math.floor(pos.x) < 16;
        Object.assign(world, {
            getColumnAt: pos => (loaded(pos) ? {} : null),
            getBlock: pos => (loaded(pos) ? getBlock(pos) : null),
            getBlockStateId: pos => (loaded(pos) ? getBlockStateId(pos) : 0),
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        const around = { minX: 15.2, minY: 1, minZ: 0.2, maxX: 15.8, maxY: 2.8, maxZ: 0.8 };
        const cell = { minX: 16.1, minY: 1.1, minZ: 0.1, maxX: 16.9, maxY: 1.9, maxZ: 0.9 };
        sync.sync(world, around, 2);
        assert.equal(sync.collisions(cell).length, 1);
        assert.equal(sync.world.blockAt(16, 1, 0).present, true);
        assert.notEqual(sync.world.fluidAt(17, 1, 0), 0);

        columnLoaded = false;
        world.emit('chunkColumnUnload', new Vec3(16, 0, 0));
        sync.sync(world, around, 2);
        assert.equal(sync.world.isChunkLoaded(1, 0), false);
        assert.equal(sync.collisions(cell).length, 0);
        assert.equal(sync.world.blockAt(16, 1, 0).present, false);
        assert.equal(sync.world.fluidAt(17, 1, 0), 0);
        assert.equal(sync.blockAt(16, 1, 0), null);
        // the chunk at x 0 is untouched
        assert.equal(sync.world.isChunkLoaded(0, 0), true);

        // while unloaded the column changes: the stone is gone, a slab stands beside it
        world.setBlock(16, 1, 0, registry.blocksByName.air.defaultState);
        world.setBlock(16, 1, 1, registry.blocksByName.oak_slab.defaultState);
        columnLoaded = true;
        world.emit('chunkColumnLoad', new Vec3(16, 0, 0));
        sync.sync(world, around, 2);
        assert.equal(sync.world.isChunkLoaded(1, 0), true);
        assert.equal(sync.collisions(cell).length, 0);
        assert.equal(sync.world.blockAt(16, 1, 0).present, false);
        const slab = sync.collisions({ minX: 16.1, minY: 1.1, minZ: 1.1, maxX: 16.9, maxY: 1.9, maxZ: 1.9 });
        assert.deepEqual(slab.map(({ min, max }) => [min.y, max.y]), [[1, 1.5]]);
        assert.notEqual(sync.world.fluidAt(17, 1, 0), 0);
    });

    it('gives the 1.16 and 1.16.1 chain and lanterns their own boxes', () => {
        // 1.16 ChainBlock.java:18 (upright, no axis yet) and LanternBlock.java:20-25, 49; minecraft-data's 1.16.1 shapes
        // have 1.16.2's states, which would make the chain an x bar and a standing lantern hang.
        const boxes = (version, name, properties) => {
            const { registry, world } = worldOf(version, ({ Block, set }) => set(0, 1, 0, Block.fromProperties(name, properties, 0).stateId));
            const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
            sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 1);
            return sync.boxesAt(0, 1, 0);
        };
        for (const version of ['1.16', '1.16.1']) {
            assert.deepEqual(boxes(version, 'chain', { waterlogged: false }), [[0.40625, 0, 0.40625, 0.59375, 1, 0.59375]], version);
            assert.deepEqual(boxes(version, 'lantern', { hanging: false }).map(b => b[4]), [0.4375, 0.5625], version);
            assert.deepEqual(boxes(version, 'soul_lantern', { hanging: true }).map(b => b[4]), [0.5, 0.625], version);
        }
        assert.deepEqual(boxes('1.16.2', 'chain', { axis: 'x', waterlogged: false }), [[0, 0.40625, 0.40625, 1, 0.59375, 0.59375]]);
    });

    it('gives a fluid the flow toward its lower neighbour', () => {
        const { registry, world } = worldOf('1.21.1', ({ registry: r, Block, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, Block.fromProperties('water', { level: 0 }, 0).stateId);
            set(1, 1, 0, Block.fromProperties('water', { level: 2 }, 0).stateId);
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 2);
        const flow = sync.world.fluidFlowAt(0, 1, 0);
        assert.ok(flow.x > 0 && flow.z === 0, `flow ${JSON.stringify(flow)}`);
    });

    it('turns a falling fluid down beside a block whose support face is full', () => {
        // FlowingFluid.isSolidFace: isFaceSturdy, Block.isFaceFull of the support shape (1.16 Block.java:232-239,
        // 1.20.4 FlowableFluid.java:104-114): the union of a stair's boxes, no leaves (LeavesBlock.getSidesShape
        // empty; 1.15 the #leaves, Block.java:431-433, which 1.14's cache missed), eight layers of snow from 1.16 (SnowBlock.getSidesShape);
        // 1.13's getBlockFaceShape: soul sand solid, glass an exception (1.13.2 FlowingFluid.java:131-143)
        const downBeside = (version, name, props = {}) => {
            const { registry, world } = worldOf(version, ({ registry: r, Block, set }) => {
                floor({ registry: r, set });
                set(0, 1, 0, Block.fromProperties('water', { level: 8 }, 0).stateId);
                set(1, 1, 0, Block.fromProperties(name, props, 0).stateId);
            });
            const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
            sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 2);
            return sync.world.fluidFlowAt(0, 1, 0).y;
        };
        const stairs = { facing: 'east', half: 'bottom', shape: 'straight', waterlogged: false };
        assert.equal(downBeside('1.21.1', 'stone'), -1);
        assert.equal(downBeside('1.21.1', 'oak_stairs', stairs), -1);
        assert.equal(downBeside('1.21.1', 'oak_leaves', { distance: 7, persistent: false, waterlogged: false }), 0);
        assert.equal(downBeside('1.21.1', 'snow', { layers: 8 }), -1);
        assert.equal(downBeside('1.21.1', 'snow', { layers: 7 }), 0);
        assert.equal(downBeside('1.14.4', 'oak_leaves', { distance: 7, persistent: false }), -1);
        assert.equal(downBeside('1.15.2', 'oak_leaves', { distance: 7, persistent: false }), 0);
        assert.equal(downBeside('1.16', 'oak_leaves', { distance: 7, persistent: false, }), 0);
        assert.equal(downBeside('1.14.4', 'snow', { layers: 8 }), 0);
        assert.equal(downBeside('1.13.2', 'soul_sand'), -1);
        assert.equal(downBeside('1.13.2', 'glass'), 0);
    });
});

describe('NativePhysics', () => {
    it('refuses a release the engine has no module for when it is constructed', () => {
        const registry = require('prismarine-registry')('1.21.4');
        for (const Engine of [adapter.NativePhysics, adapter.NativeBoatPhysics, adapter.NativeRiddenMobPhysics]) {
            assert.throws(() => new Engine(registry, { version: '1.22.1' }), /unsupported decompiled Minecraft version: 1\.22\.1/);
            assert.doesNotThrow(() => new Engine(registry));
            // a binding without isSupportedVersion (the prebuilt wasm one) still constructs
            const older = Object.create(adapter.LoadNativePhysics());
            older.isSupportedVersion = undefined;
            assert.doesNotThrow(() => new Engine(registry, { binding: older }));
        }
    });

    it('carries a state on into another world, which the native player asks setState for', () => {
        const fall = worlds => {
            const physics = new adapter.NativePhysics(worlds[0].registry);
            const ctx = playerState(0.5, 20, 0.5);
            const ticks = [];
            for (const { world } of worlds) {
                physics.simulate(ctx, world);
                ticks.push(ctx.state.pos.y, ctx.state.vel.y);
            }
            return ticks;
        };
        const worlds = () => worldOf('1.21.1', floor);
        const first = worlds();
        const same = fall([first, first, first]);
        assert.deepEqual(fall([worlds(), worlds(), worlds()]), same);
    });

    it('skips a move no tick can walk, as a velocity packet can ask from 1.21.9 (26.3 LpVec3.java:17)', () => {
        const most = 1.7179869183e10;
        for (const version of ['1.21.9', '1.21.11', '26.3'].filter(v => adapter.LoadNativePhysics().isSupportedVersion(v))) {
            for (const [x, y, z] of [[most, 0, 0], [0, most, 0], [0, -most, 0], [0, 0, -most], [most, most, most], [Infinity, 0, 0]]) {
                const { registry, world } = worldOf(version, floor);
                const physics = new adapter.NativePhysics(registry);
                const ctx = playerState(0.5, 3, 0.5);
                physics.simulate(ctx, world);
                const { x: atX, y: atY, z: atZ } = ctx.state.pos;
                ctx.state.vel.set(x, y, z);
                assert.doesNotThrow(() => physics.simulate(ctx, world), `${version} ${x} ${y} ${z}`);
                assert.equal(ctx.state.moveRefused, true);
                assert.deepEqual([ctx.state.pos.x, ctx.state.pos.y, ctx.state.pos.z], [atX, atY, atZ]);
                assert.deepEqual([ctx.state.vel.x, ctx.state.vel.y, ctx.state.vel.z], [0, 0, 0]);
                // the next tick goes on from there
                physics.simulate(ctx, world);
                assert.equal(ctx.state.moveRefused, false);
                assert.equal(ctx.state.pos.y, atY);
                assert.equal(ctx.state.vel.y, -0.0784000015258789);
            }
        }
    });

    it('still walks the largest move a sync reads', () => {
        const { registry, world } = worldOf('1.21.11', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 3, 0.5);
        ctx.state.vel.set(0, 4e5, 0);
        physics.simulate(ctx, world);
        assert.equal(ctx.state.moveRefused, false);
        assert.ok(ctx.state.pos.y > 3e5);
    });

    it('lets a player fall and land as vanilla does', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 3, 0.5);
        physics.simulate(ctx, world);
        assert.equal(ctx.state.pos.y, 3);
        assert.equal(ctx.state.vel.y, -0.0784000015258789);
        physics.simulate(ctx, world);
        assert.equal(ctx.state.pos.y, 3 - 0.0784000015258789);
        for (let tick = 0; tick < 20; tick += 1) physics.simulate(ctx, world);
        assert.equal(ctx.state.pos.y, 1);
        assert.ok(ctx.state.onGround);
        assert.equal(ctx.state.age, 22);
    });

    it('bounces a player off a pushed slime block and carries one on a pushed honey block', () => {
        // moveCollidedEntities sets the motion along the way a slime block moves before the push, moveStuckEntities
        // carries an on-ground entity over a honey block moving sideways by deltaProgress (1.21.11
        // PistonMovingBlockEntity.java:122-175, 194-213)
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const cube = [[0, 0, 0, 1, 1, 1]];
        const honeyShape = [[0.0625, 0, 0.0625, 0.9375, 0.9375, 0.9375]];
        const pushed = (x, movedBlock, pos, shapes = cube) => {
            const ctx = playerState(x, 1, 0.5);
            // the second tick lands the player on the floor
            for (let tick = 0; tick < 2; tick += 1) physics.simulate(ctx, world);
            physics.pushByMovingPiston(ctx, world, { pos, direction: [1, 0, 0], extending: true, source: false, progress: 0, shapes, movedBlock });
            return ctx.state;
        };
        const slime = pushed(1.8, 'slime_block', [2, 1, 0]);
        assert.equal(slime.vel.x, 1);
        assert.equal(slime.pos.x, 1.8 + 0.51);
        const stone = pushed(1.8, 'stone', [2, 1, 0]);
        assert.equal(stone.vel.x, 0);
        assert.equal(stone.pos.x, 1.8 + 0.51);
        assert.equal(pushed(0.5, 'honey_block', [1, 0, 0], honeyShape).pos.x, 1);
        assert.equal(pushed(0.5, 'stone', [1, 0, 0], honeyShape).pos.x, 0.5);
    });

    it('forwards optional movement authority through packed ticks and clears replacements', () => {
        for (const version of ['26.2', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5);
            let player;
            for (const value of [undefined, false, true, false, undefined]) {
                if (value === undefined) delete ctx.state.canSimulateMovement;
                else ctx.state.canSimulateMovement = value;
                for (let tick = 0; tick < 2; tick += 1) {
                    physics.simulate(ctx, world);
                    const current = physics.carries.get(ctx.state).player;
                    if (player) assert.equal(current, player);
                    player = current;
                    assert.equal(current.state().canSimulateMovement, value, `${version}, ${value}, tick ${tick}`);
                    assert.equal(ctx.state.canSimulateMovement, value);
                    assert.equal(Object.hasOwn(ctx.state, 'canSimulateMovement'), value !== undefined);
                }
            }
        }
    });

    it('uses the 26.3 movement authority for slime bounce while preserving displacement', () => {
        // PistonMovingBlockEntity.moveCollidedEntities gates bounce with canSimulateMovement from 26.3;
        // MoverType.PISTON displacement still reaches players without authority.
        for (const version of ['26.2', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            for (const fields of [
                {},
                { controlledByLocalInstance: false },
                { controlledByLocalInstance: true, canSimulateMovement: false },
                { controlledByLocalInstance: false, canSimulateMovement: true },
            ]) {
                const ctx = playerState(1.8, 1, 0.5, fields);
                for (let tick = 0; tick < 2; tick += 1) physics.simulate(ctx, world);
                physics.pushByMovingPiston(ctx, world, {
                    pos: [2, 1, 0], direction: [1, 0, 0], extending: true, source: false, progress: 0,
                    shapes: [[0, 0, 0, 1, 1, 1]], movedBlock: 'slime_block',
                });
                const eligible = version === '26.2' || (fields.canSimulateMovement ?? fields.controlledByLocalInstance ?? true);
                assert.equal(ctx.state.vel.x, eligible ? 1 : 0, `${version}, ${JSON.stringify(fields)}`);
                assert.equal(ctx.state.pos.x, 1.8 + 0.51);
            }
        }
    });

    it('moves a pushed block on 1.9 - 1.10 by its outline boxes, the full cube without them', () => {
        // method_11702 takes pushedBlock.getCollisionBox(BlockView, pos), the bounding box, not the collision boxes
        // (1.9.4 PistonBlockEntity.java:83-88, Block.java:47, 352): a fence's outline is 1.0 high (FenceBlock.java:28-31)
        const { registry, world } = worldOf('1.10.2', floor);
        const physics = new adapter.NativePhysics(registry);
        const pushed = (pos, shapes, outlineShapes) => {
            const ctx = playerState(1.2, 1, 0.5);
            // the second tick lands the player on the floor
            for (let tick = 0; tick < 2; tick += 1) physics.simulate(ctx, world);
            physics.pushByMovingPiston(ctx, world, { pos, direction: [1, 0, 0], extending: true, source: false, progress: 0, shapes, outlineShapes });
            return ctx.state.pos.x;
        };
        // a fence in the floor's row: its collision boxes reach the player's feet, its outline does not
        const fence = [[0.375, 0, 0.375, 0.625, 1.5, 0.625]];
        assert.equal(pushed([1, 0, 0], fence, [[0.375, 0, 0.375, 0.625, 1, 0.625]]), 1.2);
        // a block without collision boxes (a rail) given no outline: the full cube
        assert.equal(pushed([1, 1, 0], [], undefined), 1.8100000119209287);
    });

    it('sends the engine no fall damage for a player whose abilities allow flight', () => {
        // Player.causeFallDamage returns early on abilities.mayfly (1.8.9 PlayerEntity.java:1387-1388, 1.21.5
        // Player.java:1470-1472)
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 3, 0.5, { mayFly: true });
        physics.simulate(ctx, world);
        assert.equal(physics.carries.get(ctx.state).player.state().fallDamageImmune, true);
        ctx.state.mayFly = false;
        physics.simulate(ctx, world);
        assert.equal(physics.carries.get(ctx.state).player.state().fallDamageImmune, false);
    });

    it("writes the landing tick's fall damage and the fall distance back to the state", () => {
        // LivingEntity.causeFallDamage's amount, floor((fd + 1.0E-6 - 3.0) * 1.0 * 1.0) from 1.21.5 (1.21.5
        // LivingEntity.java:1672-1683) and ceil((fd - 3.0F - 0) * 1.0F) before 1.20.5 (1.20.4 LivingEntity.java:1557-1564),
        // with the landing tick's drop counted from 1.21.5 (1.21.5 Entity.java:1286)
        for (const [version, damage] of [['1.21.11', 2], ['1.20.4', 2]]) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 6, 0.5);
            const seen = [];
            for (let tick = 0; tick < 40 && !ctx.state.onGround; tick += 1) {
                const before = ctx.state.fallDistance;
                physics.simulate(ctx, world);
                seen.push(ctx.state.fallDamage);
                if (!ctx.state.onGround) assert.ok(ctx.state.fallDistance >= before && (tick === 0 || ctx.state.fallDistance > before), `${version} tick ${tick}`);
            }
            assert.ok(ctx.state.onGround, version);
            assert.equal(ctx.state.fallDistance, 0, version);
            assert.equal(seen.at(-1), damage, version);
            assert.deepEqual(seen.slice(0, -1).filter(value => value !== 0), [], version);
            physics.simulate(ctx, world);
            assert.equal(ctx.state.fallDamage, 0, version);
        }
    });

    it("sends the state's own fall-damage boundary and leaves it on the state", () => {
        // the SAFE_FALL_DISTANCE and FALL_DAMAGE_MULTIPLIER attributes and the #fall_damage_immune type tag
        // calculateFallDamage reads (1.21.5 LivingEntity.java:1672-1683), the bounciness and local control
        const { registry, world } = worldOf('1.21.11', floor);
        const physics = new adapter.NativePhysics(registry);
        const fields = {
            safeFallDistance: 1, fallDamageMultiplier: 2, fallDamageImmune: false, entityBounciness: 0.25,
            blockBounciness: 0.5, controlledByLocalInstance: true,
        };
        const ctx = playerState(0.5, 6, 0.5, { ...fields });
        for (let tick = 0; tick < 40 && !ctx.state.onGround; tick += 1) physics.simulate(ctx, world);
        const engine = physics.carries.get(ctx.state).player.state();
        for (const [key, value] of Object.entries(fields)) {
            assert.equal(engine[key], value, key);
            assert.equal(ctx.state[key], value, key);
        }
        // floor((5 + 1.0E-6 - 1) * 1.0 * 2)
        assert.equal(ctx.state.fallDamage, 8);
        // the type's immunity: no damage
        const immune = playerState(0.5, 6, 0.5, { fallDamageImmune: true });
        for (let tick = 0; tick < 40 && !immune.state.onGround; tick += 1) physics.simulate(immune, world);
        assert.equal(immune.state.fallDamage, 0);
        assert.equal(physics.carries.get(immune.state).player.state().fallDamageImmune, true);
        // a state without them: the defaults (3 blocks, no immunity, the client's own player)
        const plain = playerState(0.5, 6, 0.5);
        physics.simulate(plain, world);
        const defaults = physics.carries.get(plain.state).player.state();
        assert.equal(defaults.safeFallDistance, 3);
        assert.equal(defaults.fallDamageImmune, false);
        assert.equal(defaults.controlledByLocalInstance, true);
        assert.equal('safeFallDistance' in plain.state, false);
    });

    it('spares a passenger the double-tap reset of an item in use', () => {
        // isUsingItem() && !hasVehicle() clears the double tap (1.8.9 ClientPlayerEntity.java:540-543, 1.13.2 :703-706,
        // 1.20.4 :662-665, 1.21.4 :667-670; 1.21.5 LocalPlayer.java:718, 26.3 :781)
        const trigger = (version, fields) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, isUsingItem: true, sprintTriggerTime: 5, ...fields });
            physics.simulate({ state }, world);
            return state.sprintTriggerTime;
        };
        for (const version of ['1.8.8', '1.13.2', '1.20.4', '1.21.4', '1.21.5', '26.3']) {
            assert.equal(trigger(version, { passenger: false }), 0, version);
            assert.equal(trigger(version, { passenger: true }), 4, version);
            assert.equal(trigger(version, { passenger: true, isUsingItem: false }), 4, version);
        }
    });

    it('spares a passenger the input scaling of an item in use before 1.21.5', () => {
        // the same term scales the impulses aiStep reads: forward 1 is a full impulse for the isWalking test of a double tap
        // (1.20.4 ClientPlayerEntity.java:662-664, :1012), which 0.2 on foot is not. canStartSprinting's own
        // !isUsingItem has no passenger term (1.20.4 :1015), so the use may sprint here (useItemCanSprint) or nothing starts
        const armed = (version, passenger) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, {
                onGround: true, passenger, isUsingItem: true, useItemCanSprint: true, vehicleOnGround: true, vehicleCanSprint: true,
                control: controls({ forward: true }),
            });
            physics.simulate({ state }, world);
            return state.sprintTriggerTime;
        };
        for (const version of ['1.13.2', '1.20.4', '1.21.4']) {
            assert.equal(armed(version, true), 7, version);
            assert.equal(armed(version, false), 0, version);
        }
    });

    it('stops a 1.21.4 sprint on an item in use only off the seat', () => {
        // shouldStopSprinting: isUsingItem() && !hasVehicle() && !isSubmergedInWater() (1.21.4 ClientPlayerEntity.java:820)
        const sprinting = (passenger) => {
            const { registry, world } = worldOf('1.21.4', floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, {
                onGround: true, sprinting: true, isUsingItem: true, passenger, vehicleCanSprint: true, control: controls({ forward: true }),
            });
            physics.simulate({ state }, world);
            return state.sprinting;
        };
        assert.equal(sprinting(false), false);
        assert.equal(sprinting(true), true);
    });

    it('lets a passenger toggle flight only on a jumpable vehicle from 1.21.11', () => {
        // !isSwimming() && (getVehicle() == null || jumpableVehicle() != null) (1.21.11 LocalPlayer.java:802, 26.3 :820); no
        // vehicle term in 1.21.9 or 1.21.10; the second press that fails leaves jumpTriggerTime running, Player.aiStep counts it down
        const toggled = (version, fields) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 3, 0.5, { mayFly: true, flyJumpTriggerTime: 3, control: controls({ jump: true }), ...fields });
            physics.simulate({ state }, world);
            return [state.flying, state.flyJumpTriggerTime];
        };
        for (const version of ['1.20.4', '1.21.9']) {
            assert.deepEqual(toggled(version, { passenger: true }), [true, 0], version);
        }
        for (const version of ['1.21.11', '26.1', '26.3']) {
            assert.deepEqual(toggled(version, { passenger: true }), [false, 2], version);
            assert.deepEqual(toggled(version, { passenger: true, vehicleJumpable: false }), [false, 2], version);
            assert.deepEqual(toggled(version, { passenger: true, vehicleJumpable: true }), [true, 0], version);
            assert.deepEqual(toggled(version, { passenger: false }), [true, 0], version);
        }
    });

    it('opens the double tap for the sprintWindow option from 1.21.9', () => {
        // sprintTriggerTime = options.sprintWindow().get() (1.21.9 LocalPlayer.java:731, 26.3 :790), 7 in 1.21.8 (:727); the
        // option is an IntRange(0, 10) with 7 by default (1.21.9 Options.java:502-503), and set() turns an illegal
        // value into the default (26.3 OptionInstance.java:117-121)
        const opened = (version, sprintWindow) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, sprintWindow, control: controls({ forward: true }) });
            physics.simulate({ state }, world);
            return state.sprintTriggerTime;
        };
        for (const version of ['1.21.5', '1.21.8']) {
            assert.equal(opened(version, 3), 7, version);
            assert.equal(opened(version, undefined), 7, version);
        }
        for (const version of ['1.21.9', '1.21.11', '26.3']) {
            assert.equal(opened(version, undefined), 7, version);
            assert.equal(opened(version, 3), 3, version);
            assert.equal(opened(version, 0), 0, version);
            assert.equal(opened(version, 10), 10, version);
            assert.equal(opened(version, 11), 7, version);
            assert.equal(opened(version, -1), 7, version);
            assert.equal(opened(version, 2.5), 7, version);
        }
    });

    it('takes a landed hit\'s recoil once, at the start of the tick', () => {
        // knockback > 0: motion * (0.6, 1, 0.6) and no more sprint (1.8.9 PlayerEntity.java:1026-1034, 1.14 :1081-1082, 26.3
        // Player.java:1052-1053); the keybinds run before the entities tick (26.3 Minecraft.java:1786, :1797), so the tick
        // equals one from the reduced motion without the sprint
        const run = (version, fields) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, {
                onGround: true, sprinting: true, prevForwardImpulse: 1, control: controls({ forward: true }), vel: new Vec3(0.2, 0, -0.1), ...fields,
            });
            physics.simulate({ state }, world);
            return { physics, world, state };
        };
        const moved = ({ state }) => [state.pos.x, state.pos.y, state.pos.z, state.vel.x, state.vel.y, state.vel.z, state.sprinting];
        const recoiled = () => ({ sprinting: false, vel: new Vec3(0.2 * 0.6, 0, -0.1 * 0.6) });
        const unhit = (version, fields) => assert.deepEqual(moved(run(version, fields)), moved(run(version, {})), version);
        const hit = (version, fields) => assert.deepEqual(moved(run(version, fields)), moved(run(version, recoiled())), version);
        for (const version of ['1.8.8', '1.9.4', '1.20.4', '1.21.11', '26.3']) {
            hit(version, { attackHit: { strength: 1 } });
            // a weapon's knockback does it without a sprint: nothing else tells this tick from a walk at that motion
            const walking = { sprinting: false, attackHit: { strength: 1, knockback: 1 } };
            assert.deepEqual(moved(run(version, walking)), moved(run(version, recoiled())), version);
            // no sprint, no weapon: no knockback, and the motion stays
            const plain = { sprinting: false };
            assert.deepEqual(moved(run(version, { ...plain, attackHit: { strength: 1 } })), moved(run(version, plain)), version);
            assert.deepEqual(moved(run(version, { attackHit: null })), moved(run(version, {})), version);
            // once: a held button is no second hit
            const done = run(version, { attackHit: { strength: 1 } });
            assert.equal(done.state.attackHit, null, version);
            done.state.sprinting = true;
            done.physics.simulate({ state: done.state }, done.world);
            assert.equal(done.state.sprinting, true, version);
        }
        // a charged swing from 1.9: bl = h > 0.9F (1.9.4 PlayerEntity.java:998, :1004, 1.20.4 :1125, :1129, 26.3
        // Player.java:897-898); 1.8 has no cooldown (1.8.9 PlayerEntity.java:997-998)
        hit('1.8.8', { attackHit: { strength: 0.5 } });
        for (const version of ['1.9.4', '1.20.4', '26.3']) {
            unhit(version, { attackHit: { strength: 0.5 } });
            unhit(version, { attackHit: { strength: f32(0.9) } });
            hit(version, { attackHit: { strength: 0.91 } });
            // the weapon's knockback does not wait for the charge (1.20.4 :1128, :1175)
            hit(version, { attackHit: { strength: 0.5, knockback: 1 } });
        }
        // the sprint's share halves in 1.21.11 (1.21.9 and 1.21.10 Player.java:1011, 1.21.11 :986, 26.3 :922): a weapon's -0.75 leaves
        // 0.25 above 0 before and -0.25 after
        hit('1.21.9', { attackHit: { strength: 1, knockback: -0.75 } });
        unhit('1.21.11', { attackHit: { strength: 1, knockback: -0.75 } });
        unhit('26.3', { attackHit: { strength: 1, knockback: -0.75 } });
    });

    it('agrees with the object tick on landings, a teleport and support, and takes no contact damage as a client', () => {
        // the adapter runs the client's side: a block's hurt is the server's (26.3 Entity.java:1969-1975)
        const version = '26.3';
        const { registry, world } = worldOf(version, ({ registry: known, set }) => {
            floor({ registry: known, set });
            set(2, 1, 0, known.blocksByName.cactus.defaultState);
        });
        const physics = new adapter.NativePhysics(registry);
        const { state } = playerState(0.5, 4, 0.5, { fallDistance: 6, control: controls({ forward: true }), yaw: -Math.PI / 2, yawDegrees: -90 });
        const ctx = { state };
        let landed = 0;
        let touched = 0;
        let supported = 0;
        for (let tick = 0; tick < 50; tick += 1) {
            if (tick === 40) state.pos.set(-2.5, 3, -2.5); // a position set from outside
            physics.simulate(ctx, world);
            const carry = physics.carries.get(state);
            const held = carry.player.state();
            const where = `tick ${tick}`;
            assert.deepEqual([held.position.x, held.position.y, held.position.z], [state.pos.x, state.pos.y, state.pos.z], where);
            assert.deepEqual([held.velocity.x, held.velocity.y, held.velocity.z], [state.vel.x, state.vel.y, state.vel.z], where);
            assert.equal(held.onGround, state.onGround, where);
            assert.deepEqual(held.supportingBlock ?? null, physics.carries.get(state).player.state().supportingBlock ?? null, where);
            supported += held.supportingBlock ? 1 : 0;
            assert.equal(held.fallDamage ?? 0, state.fallDamage ?? 0, `${where} fall`);
            assert.equal(held.contactDamage ?? 0, state.contactDamage ?? 0, `${where} contact`);
            assert.equal(state.contactDamage ?? 0, 0, `${where}: the client deals none`);
            landed += (state.fallDamage ?? 0) > 0 ? 1 : 0;
            // what the same state deals on the server's side, where the hurt is: the engine answers, the adapter stays out
            const server = physics.binding.simulateVersionedTick(version, physics.syncOf(world).world, { ...held, clientSide: false }, {});
            touched += (server.contactDamage ?? 0) > 0 ? 1 : 0;
        }
        assert.ok(landed >= 1, 'it landed');
        assert.ok(supported >= 1, 'it stood on something');
        assert.ok(touched >= 1, 'it touched the cactus');
    });

    it('follows a shulker box from its block event: the cell grows, and a replaced box keeps no lid', () => {
        const version = '26.3';
        const { registry, world } = worldOf(version, ({ registry: known, Block, set }) => {
            floor({ registry: known, set });
            const up = Block.fromProperties('shulker_box', { facing: 'up' }, 0).stateId;
            set(2, 1, 2, up);
        });
        const physics = new adapter.NativePhysics(registry);
        const bot = Object.assign(new EventEmitter(), { _client: new EventEmitter() });
        const { state } = playerState(0.5, 1, 0.5, { onGround: true, bot });
        physics.simulate({ state }, world);
        class Box {
            constructor(minX, minY, minZ, maxX, maxY, maxZ) {
                Object.assign(this, { minX, minY, minZ, maxX, maxY, maxZ });
            }
        }
        const top = () => Math.max(...physics.getSurroundingBBs(new Box(2.2, 1.2, 2.2, 2.8, 2.4, 2.8), world).map(box => box.maxY));
        assert.equal(top(), 2, 'shut, a cube');
        bot._client.emit('block_action', { location: { x: 2, y: 1, z: 2 }, byte1: 1, byte2: 1 });
        for (let tick = 0; tick < 5; tick += 1) bot.emit('physicsTick');
        physics.simulate({ state }, world);
        assert.ok(top() > 2.2, 'the open lid is the collision shape');
        world.setBlock(2, 1, 2, registry.blocksByName.stone.defaultState);
        bot.emit('blockUpdate', null, { position: new Vec3(2, 1, 2), name: 'stone' });
        physics.simulate({ state }, world);
        assert.equal(top(), 2, 'the stone that replaced it is a cube, and no lid is left');
        world.setBlock(2, 1, 2, registry.blocksByName.shulker_box.defaultState);
        physics.simulate({ state }, world);
        assert.equal(top(), 2, 'a new box starts shut');
        bot._client.emit('block_action', { location: { x: 2, y: 1, z: 2 }, byte1: 1, byte2: 1 });
        for (let tick = 0; tick < 5; tick += 1) bot.emit('physicsTick');
        physics.simulate({ state }, world);
        assert.ok(top() > 2.2, 'open again');
        world.emit('chunkColumnUnload', new Vec3(0, 0, 0));
        physics.simulate({ state }, world);
        assert.equal(top(), 2, 'an unloaded column keeps no lid');
        physics.releaseWorld(world);
        assert.equal(world.listenerCount('chunkColumnUnload'), 0);
        assert.equal(bot._client.listenerCount('block_action'), 0);
        assert.equal(bot.listenerCount('physicsTick'), 0);
    });

    it('takes food from the server: sprint reads it, a tick never spends it, and an update counts at once', () => {
        // Player.causeFoodExhaustion does nothing on a client (26.3 Player.java:1471-1477), FoodData ticks in ServerPlayer
        const version = '26.3';
        const { registry, world } = worldOf(version, floor);
        const physics = new adapter.NativePhysics(registry);
        const sprints = fields => {
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, control: controls({ forward: true, sprint: true }), ...fields });
            physics.simulate({ state }, world);
            return { sprinting: state.sprinting, food: state.food, state };
        };
        assert.equal(sprints({ food: 6 }).sprinting, false);
        assert.equal(sprints({ food: 7 }).sprinting, true);
        assert.equal(sprints({ food: 0, mayFly: true }).sprinting, true);
        const { state } = sprints({ food: 7 });
        state.clone = function clone() { return { ...this, pos: this.pos.clone(), vel: this.vel.clone() }; };
        const clone = physics.cloneState(state);
        for (let tick = 0; tick < 40; tick += 1) physics.simulate({ state }, world);
        assert.equal(state.food, 7, 'sprinting spent none');
        assert.equal(state.sprinting, true);
        state.food = 6; // the set_health packet, between ticks
        state.sprinting = false;
        physics.simulate({ state }, world);
        assert.equal(state.sprinting, false, 'the new value counts on the next tick');
        physics.simulate({ state: clone }, world);
        assert.equal(clone.food, 7, 'a fork keeps the value it was made with');
        assert.equal(clone.sprinting, true);
    });

    it('holds a passenger to what its vehicle can sprint from 1.19.4, and needs no food from 1.19.3', () => {
        // canStartSprinting (1.19.3 ClientPlayerEntity.java:689, 1.19.4 :1020-1031, 26.3 LocalPlayer.java:1117-1146)
        const sprints = (version, fields) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, passenger: true, control: controls({ forward: true, sprint: true }), ...fields });
            physics.simulate({ state }, world);
            return state.sprinting;
        };
        assert.equal(sprints('1.19.3', {}), true);
        assert.equal(sprints('1.19.2', { food: 6 }), false);
        assert.equal(sprints('1.19.3', { food: 6 }), true);
        for (const version of ['1.19.4', '1.20.4', '1.21.4', '1.21.11', '26.3']) {
            assert.equal(sprints(version, {}), false, version);
            assert.equal(sprints(version, { vehicleCanSprint: false }), false, version);
            assert.equal(sprints(version, { vehicleCanSprint: true }), true, version);
            assert.equal(sprints(version, { vehicleCanSprint: true, food: 0 }), true, version);
            // no vehicle, so the flag means nothing
            assert.equal(sprints(version, { passenger: false }), true, version);
            assert.equal(sprints(version, { passenger: false, food: 6 }), false, version);
        }
    });

    it('stops a passenger sprint when its vehicle cannot, from 1.21.4', () => {
        // 1.21.4 ClientPlayerEntity.java:815-821, 1.21.8 LocalPlayer.java:837-852, 26.3 :901-907
        const keeps = (version, fields) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, passenger: true, sprinting: true, control: controls({ forward: true }), ...fields });
            physics.simulate({ state }, world);
            return state.sprinting;
        };
        assert.equal(keeps('1.20.4', { vehicleCanSprint: false }), true);
        for (const version of ['1.21.4', '1.21.8', '1.21.9', '26.3']) {
            assert.equal(keeps(version, { vehicleCanSprint: false }), false, version);
            assert.equal(keeps(version, { vehicleCanSprint: true }), true, version);
        }
    });

    it('starts a sprint with the sprint key and moves along the yaw', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 1, 0.5, { onGround: true });
        ctx.state.control = controls({ forward: true, sprint: true });
        for (let tick = 0; tick < 5; tick += 1) physics.simulate(ctx, world);
        assert.ok(ctx.state.sprinting);
        assert.ok(ctx.state.pos.z > 1, `z ${ctx.state.pos.z}`);
        assert.equal(ctx.state.pos.x, 0.5);
        assert.equal(ctx.state.pos.y, 1);
    });

    it('crouches where the player has no room to stand', () => {
        const { registry, world } = worldOf('1.21.1', ({ registry: r, Block, set }) => {
            floor({ registry: r, set });
            const slab = Block.fromProperties('stone_slab', { type: 'top', waterlogged: false }, 0).stateId;
            for (let x = -1; x <= 1; x += 1) for (let z = -1; z <= 1; z += 1) set(x, 2, z, slab);
        });
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 1, 0.5, { onGround: true });
        physics.simulate(ctx, world);
        assert.equal(ctx.state.pose, adapter.Pose.SNEAKING);
    });

    it('lets a player in a chunk column the world has not loaded sink as 1.8 does', () => {
        const { registry, world } = worldOf('1.8.8', floor);
        unloadedFromX16(world);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(20.5, 5, 0.5);
        physics.simulate(ctx, world);
        assert.equal(ctx.state.vel.y, -0.09800000190734864);
    });

    it('turns the yaw into radians for moveFlying as 1.8 does', () => {
        // 1.8.9 Entity.java:942-943: MathHelper.sin(this.yaw * (float) Math.PI / 180.0F), two float roundings; at 28.927
        // degrees that is sine table entry 5266 where yaw * 0.017453292F (1.9.4 Entity.java:1050-1051) gives 5265.
        const sine = index => f32(Math.sin((index * Math.PI * 2) / 65536));
        const moved = (version, index) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 3, 0.5, { yawDegrees: f32(28.927) });
            ctx.state.control = controls({ forward: true });
            physics.simulate(ctx, world);
            const forward = f32(f32(0.98) * f32(0.02));
            assert.equal(ctx.state.pos.x, 0.5 + f32(-f32(forward * sine(index))), version);
        };
        moved('1.8.8', 5266);
        moved('1.9.4', 5265);
    });

    it('sends the engine the Weaving effect for the cobweb', () => {
        // WebBlock.entityInside: (0.5, 0.25, 0.5) for a LivingEntity with WEAVING (1.20.5 CobwebBlock.java:26-28,
        // 1.21.11 WebBlock.java:27-31)
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 1, 0.5, { onGround: true });
        physics.simulate(ctx, world);
        assert.equal(physics.carries.get(ctx.state).player.state().weaving, false);
        const weaving = registry.effectsByName.Weaving.id;
        ctx.state.effects = { [weaving]: { amplifier: 0 } };
        physics.simulate(ctx, world);
        assert.equal(physics.carries.get(ctx.state).player.state().weaving, true);
    });

    it('sends the engine the Frost Walker of the boots for magma and campfires', () => {
        // MagmaBlock.stepOn and CampfireBlock.entityInside spare its wearer before 1.21 (1.20.5 MagmaBlock.java:29,
        // CampfireBlock.java:104), its damage immunity to #burn_from_stepping from 1.21 (1.21.11 Enchantments.java:389-394)
        const frostWalkerOf = (version, enchants) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const slots = [];
            slots[8] = enchants === undefined ? null : { enchants: enchants(registry.enchantmentsByName.frost_walker.id) };
            const bot = { inventory: { slots }, getEquipmentDestSlot: destination => (destination === 'feet' ? 8 : 5) };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, bot });
            physics.simulate(ctx, world);
            return physics.carries.get(ctx.state).player.state().frostWalker;
        };
        assert.equal(frostWalkerOf('1.21.1', undefined), false);
        assert.equal(frostWalkerOf('1.21.1', id => ({ enchantments: [{ id, level: 2 }] })), true);
        assert.equal(frostWalkerOf('1.20.4', () => [{ name: 'frost_walker', lvl: 1 }]), true);
        assert.equal(frostWalkerOf('1.20.4', () => [{ name: 'depth_strider', lvl: 1 }]), false);
    });

    it('keeps the running results of the client side at nothing, and tells a landing once', () => {
        // a block's hurt, a flight into a wall and the freeze ticks are the server's (26.3 Entity.java:1969-1975,
        // LivingEntity.java:2505-2510, 3082); Block.updateEntityMovementAfterFallOn runs on the client's own landing too
        const { registry, world } = worldOf('1.20.4', ({ registry: r, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, r.blocksByName.cactus.defaultState);
        });
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(1.2, 1.5, 0.5, { vel: new Vec3(0, -1, 0) });
        physics.simulate(ctx, world);
        assert.deepEqual([ctx.state.onGround, ctx.state.blockFallCallback], [true, true]);
        assert.deepEqual([ctx.state.contactDamage, ctx.state.wallDamage, ctx.state.freezeDamage], [0, 0, 0]);
        ctx.state.vel.set(0, 0, 0);
        physics.simulate(ctx, world);
        physics.simulate(ctx, world);
        assert.equal(ctx.state.contactDamage, 0);
    });

    it('raises the safe fall distance by Jump Boost where no attribute was synced', () => {
        // Jump Boost is an ADD_VALUE modifier of SAFE_FALL_DISTANCE from 1.20.5, a term of the damage before (1.20.5
        // StatusEffects.java:57-62, 26.3 MobEffects.java:76; 1.20.4 LivingEntity.java:1562): a synced attribute holds it
        const inputs = (version, fields) => {
            const { registry } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const effects = { [registry.effectsByName.JumpBoost.id]: { amplifier: 1 } };
            return physics.attributesAndEffects(playerState(0.5, 1, 0.5, { effects, ...fields(physics) }).state).safeFallDistance;
        };
        assert.equal(inputs('26.3', () => ({})), 5);
        assert.equal(inputs('1.20.5', () => ({})), 5);
        assert.equal(inputs('1.20.4', () => ({})), 3);
        const key = physics => physics.attributeKey('safeFallDistance');
        assert.equal(inputs('26.3', physics => ({ attributes: { [key(physics)]: { value: 3, modifiers: [{ amount: 2, operation: 0, id: 'minecraft:effect.jump_boost' }] } } })), 5);
        assert.equal(inputs('26.3', () => ({ safeFallDistance: 4 })), 4);
    });

    it('holds an effect past its duration until the server takes it away', () => {
        // The client's tick never removes one: MobEffectInstance.tickClient only counts down, the removal is the server's
        // (26.3 LivingEntity.java tickEffects, MobEffectInstance.java:213-220; 1.20.4 LivingEntity.java:769-780; 1.8.9
        // LivingEntity.java tickStatusEffects), so the state's effects are a snapshot of the packets and nothing runs them down
        const { registry, world } = worldOf('1.20.4', floor);
        const physics = new adapter.NativePhysics(registry);
        const id = registry.effectsByName.Levitation.id;
        const effect = { amplifier: 0, duration: 0 };
        const ctx = playerState(0.5, 5, 0.5, { effects: { [id]: effect } });
        for (let tick = 0; tick < 3; tick += 1) {
            physics.simulate(ctx, world);
            assert.equal(physics.carries.get(ctx.state).player.state().levitation, true);
        }
        assert.equal(ctx.state.effects[id], effect);
        assert.equal(effect.duration, 0);
        ctx.state.effects = {};
        physics.simulate(ctx, world);
        const engine = physics.carries.get(ctx.state).player.state();
        assert.equal(engine.levitation, false);
        assert.equal(engine.levitationAmplifier, undefined);
    });

    it('reads an amplifier as a signed byte before 1.20.5 and clamps it from there', () => {
        // 1.20.4 EntityStatusEffectS2CPacket.java:49, StatusEffectInstance.java:73; 1.20.5 EntityStatusEffectS2CPacket.java:57,
        // StatusEffectInstance.java:81
        const amplifierOf = (version, amplifier) => {
            const { registry } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const effects = { [registry.effectsByName.Levitation.id]: { amplifier } };
            const inputs = physics.attributesAndEffects(playerState(0.5, 1, 0.5, { effects }).state);
            return [inputs.levitation, inputs.levitationAmplifier];
        };
        for (const [amplifier, legacy, modern] of [[0, 0, 0], [127, 127, 127], [128, -128, 128], [255, -1, 255]]) {
            assert.deepEqual(amplifierOf('1.20.4', amplifier), [true, legacy]);
            assert.deepEqual(amplifierOf('1.20.5', amplifier), [true, modern]);
        }
    });

    it('moves no differently under the effects with no movement consumer', () => {
        // Darkness and Conduit Power hold no attribute modifier and nothing in travel reads them (26.3 MobEffects.java;
        // the MobEffects of LivingEntity, Player and LocalPlayer are Jump Boost, Slow Falling, Levitation, Dolphin's Grace,
        // Blindness; Speed and Slowness through the attribute; Weaving in WebBlock)
        for (const name of ['Darkness', 'ConduitPower']) {
            const run = effects => {
                const { registry, world } = worldOf('26.3', floor);
                const physics = new adapter.NativePhysics(registry);
                const ctx = playerState(0.5, 1, 0.5, { onGround: true, effects: effects(registry), control: controls({ forward: true, sprint: true, jump: true }) });
                for (let tick = 0; tick < 6; tick += 1) physics.simulate(ctx, world);
                return [ctx.state.pos.toArray(), ctx.state.vel.toArray(), ctx.state.sprinting];
            };
            assert.deepEqual(run(registry => ({ [registry.effectsByName[name].id]: { amplifier: 3 } })), run(() => ({})), name);
        }
    });

    it('folds Speed and Slowness once, through the synced attribute', () => {
        const { registry, world } = worldOf('26.3', floor);
        const physics = new adapter.NativePhysics(registry);
        const key = physics.attributeKey('movementSpeed');
        const id = registry.effectsByName.Speed.id;
        const attributes = { [key]: { value: f32(0.1), modifiers: [{ id: 'minecraft:effect.speed', amount: f32(0.2) * 2, operation: 2 }] } };
        const ctx = playerState(0.5, 1, 0.5, { onGround: true, attributes, effects: { [id]: { amplifier: 1 } } });
        physics.simulate(ctx, world);
        const engine = physics.carries.get(ctx.state).player.state();
        assert.equal(engine.speedEffectAmplifier, undefined);
        assert.ok(Math.abs(engine.movementSpeedValue - f32(0.1) * (1 + f32(0.2) * 2)) < 1e-12);
    });

    describe('what the bot wears', () => {
        const slotsOf = { head: 5, torso: 6, legs: 7, feet: 8, 'off-hand': 45 };
        /** A bot wearing { slot: item } and the slow inputs the engine gets for it after a tick. */
        const worn = (version, items, fields = {}) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const slots = [];
            for (const [slot, item] of Object.entries(items(registry))) slots[slotsOf[slot]] = item;
            const bot = { inventory: { slots }, getEquipmentDestSlot: destination => slotsOf[destination] };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, bot, ...fields });
            physics.simulate(ctx, world);
            return physics.carries.get(ctx.state).player.state();
        };
        const nbtItem = (name, lvl) => ({ enchants: [{ name, lvl }] });
        const componentItem = (registry, name, level) => ({ enchants: { enchantments: [{ id: registry.enchantmentsByName[name].id, level }] } });

        it('takes Depth Strider from the highest armor piece, not from the fields of mineflayer-physics-utils', () => {
            // getDepthStrider: the highest level on any armor piece (1.8.9 EnchantmentHelper.java:88-103, 182-183; 1.9.4
            // Enchantment.java:118, 137; 1.20.5 Enchantments.java:72-75). PlayerState reads equipment[5], the helmet, and an
            // nbt that items have no more from 1.20.5
            for (const version of ['1.8.8', '1.9.4', '1.13.2', '1.20.4']) {
                assert.equal(worn(version, () => ({ feet: nbtItem('depth_strider', 3) })).depthStrider, 3, version);
                assert.equal(worn(version, () => ({ legs: nbtItem('depth_strider', 1), feet: nbtItem('depth_strider', 2) })).depthStrider, 2, version);
                assert.equal(worn(version, () => ({}), { depthStrider: 3 }).depthStrider, 0, version);
            }
            // the first entry of a duplicated enchantment counts (EnchantmentHelper.getLevel, 1.20.4 :63-74)
            assert.equal(worn('1.20.4', () => ({ feet: { enchants: [{ name: 'depth_strider', lvl: 1 }, { name: 'depth_strider', lvl: 3 }] } })).depthStrider, 1);
            assert.equal(worn('1.20.6', registry => ({ feet: componentItem(registry, 'depth_strider', 3) })).depthStrider, 3);
            // from 1.21 the WATER_MOVEMENT_EFFICIENCY attribute carries it, and a raw level adds nothing to it
            assert.equal(worn('1.21.1', registry => ({ feet: componentItem(registry, 'depth_strider', 3) }), { depthStrider: 3 }).depthStrider, 0);
        });

        it('takes Soul Speed from the boots and Swift Sneak from the leggings alone', () => {
            // 1.16.5 Enchantments.java:29, 1.19 :30, 1.20.5 :92-106: EquipmentSlot.FEET and LEGS
            assert.equal(worn('1.16.5', () => ({ feet: nbtItem('soul_speed', 2) })).soulSpeed, 2);
            assert.equal(worn('1.16.5', () => ({ head: nbtItem('soul_speed', 2) }), { soulSpeed: 2 }).soulSpeed, 0);
            assert.equal(worn('1.20.5', registry => ({ feet: componentItem(registry, 'soul_speed', 1) })).soulSpeed, 1);
            // 0.3F + level * 0.15F, the attribute from 1.21 (1.20.5 ClientPlayerEntity.java:662)
            assert.equal(worn('1.20.4', () => ({ legs: nbtItem('swift_sneak', 2) })).sneakingSpeed, f32(f32(0.3) + f32(2 * f32(0.15))));
            assert.equal(worn('1.20.5', registry => ({ legs: componentItem(registry, 'swift_sneak', 1) })).sneakingSpeed, f32(f32(0.3) + f32(0.15)));
            assert.equal(worn('1.20.4', () => ({ feet: nbtItem('swift_sneak', 2) })).sneakingSpeed, undefined);
        });

        it('keeps the fields of a state with no bot inventory', () => {
            const { registry, world } = worldOf('1.20.4', floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, depthStrider: 2, soulSpeed: 1, swiftSneak: 3, bot: { game: {} } });
            physics.simulate(ctx, world);
            const state = physics.carries.get(ctx.state).player.state();
            assert.deepEqual([state.depthStrider, state.soulSpeed], [2, 1]);
            assert.equal(state.sneakingSpeed, f32(f32(0.3) + f32(3 * f32(0.15))));
        });

        it('follows the armor from tick to tick, as it changes or breaks', () => {
            const { registry, world } = worldOf('1.20.4', floor);
            const physics = new adapter.NativePhysics(registry);
            const slots = [];
            slots[8] = nbtItem('depth_strider', 3);
            const bot = { inventory: { slots }, getEquipmentDestSlot: destination => slotsOf[destination] };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, bot });
            physics.simulate(ctx, world);
            assert.equal(physics.carries.get(ctx.state).player.state().depthStrider, 3);
            slots[8] = null;
            physics.simulate(ctx, world);
            assert.equal(physics.carries.get(ctx.state).player.state().depthStrider, 0);
        });

        it('walks on powder snow in leather boots, whatever their enchantments', () => {
            // PowderSnowBlock.canEntityWalkOnPowderSnow: the feet's item is leather boots (1.17 PowderSnowBlock.java:107-113)
            const walkable = (items, fields) => worn('1.20.4', () => items, { canWalkOnPowderSnow: undefined, ...fields }).powderSnowWalkable;
            assert.equal(walkable({ feet: { name: 'leather_boots', enchants: [] } }), true);
            assert.equal(walkable({ feet: { name: 'iron_boots', enchants: [] } }), false);
            assert.equal(walkable({ head: { name: 'leather_boots', enchants: [] } }), false);
            assert.equal(walkable({ feet: { name: 'iron_boots', enchants: [] } }, { canWalkOnPowderSnow: true }), true);
        });

        it('launches on the Riptide of the item in use when the state does not say', () => {
            const { registry, world } = worldOf('1.20.4', ({ registry: r, set }) => {
                for (let y = 1; y < 5; y += 1) set(0, y, 0, r.blocksByName.water.defaultState);
            });
            const launch = (bot, fields = {}) => {
                const physics = new adapter.NativePhysics(registry);
                const ctx = playerState(0.5, 2, 0.5, { riptide: undefined, isInWater: true, bot, ...fields });
                for (let tick = 0; tick < 11; tick += 1) {
                    ctx.state.isUsingItem = true;
                    physics.simulate(ctx, world);
                }
                ctx.state.isUsingItem = false;
                ctx.state.isUsingOffHand = false;
                physics.simulate(ctx, world);
                return ctx.state.autoSpinAttackTicks;
            };
            assert.ok(launch({ heldItem: nbtItem('riptide', 3) }) > 0);
            assert.equal(launch({ heldItem: nbtItem('loyalty', 3) }), 0);
            const slots = [];
            slots[45] = nbtItem('riptide', 1);
            const offHand = { heldItem: null, inventory: { slots }, getEquipmentDestSlot: destination => slotsOf[destination] };
            assert.ok(launch(offHand, { isUsingOffHand: true }) > 0);
            assert.equal(launch(offHand), 0);
            // the fork's state always says the main hand's level, a number (playerState.ts riptide), and that is not the item in use
            assert.ok(launch(offHand, { isUsingOffHand: true, riptide: 0 }) > 0);
            assert.ok(launch({ heldItem: nbtItem('riptide', 3) }, { riptide: 3 }) > 0);
            assert.equal(launch({ heldItem: nbtItem('loyalty', 3) }, { riptide: 0 }), 0);
            const plain = [];
            plain[45] = nbtItem('loyalty', 1);
            const riptideInMainHand = { heldItem: nbtItem('riptide', 3), inventory: { slots: plain }, getEquipmentDestSlot: destination => slotsOf[destination] };
            assert.equal(launch(riptideInMainHand, { isUsingOffHand: true, riptide: 3 }), 0);
            assert.ok(launch(riptideInMainHand, { riptide: 3 }) > 0);
        });

        it('reads the eyes of a legacy release through the float getEyeHeight', () => {
            const { registry } = worldOf('1.12.2', () => {});
            const physics = new adapter.NativePhysics(registry);
            // the surface of a source block at y 9 is y 10 (Entity.isSubmergedIn, 1.12.2 Entity.java:1120-1135)
            const sync = { blockAt: (x, y) => (y === 9 ? { fluid: { type: 1, level: 0 } } : null) };
            const under = y => {
                const { state } = playerState(0.5, y, 0.5, {});
                physics.updateLegacyFluids(state, sync);
                return state.isUnderWater;
            };
            const eye = Math.fround(1.62);
            assert.equal(eye, 1.6200000047683716);
            assert.equal(under(10 - 1.62 - 2e-9), false);
            assert.equal(under(10 - eye - 2e-9), true);
        });
    });

    describe('the jump of a flight started on the ground', () => {
        const jump = (version, fields = {}, place = floor) => {
            const { registry, world } = worldOf(version, place);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, ...fields });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: 0, minZ: -1, maxX: 2, maxY: 3, maxZ: 2 }, 1);
            physics.jumpFromGround(ctx.state, sync);
            return ctx.state.vel.y;
        };

        it('sets the rise before 1.21.2, and keeps a faster one from it', () => {
            // setDeltaMovement(x, jumpPower, z), Math.max from 1.21.2 (1.20.5 LivingEntity.java:2078, 1.21.1 :2085, 1.21.2 :2164)
            for (const version of ['1.20.5', '1.20.6', '1.21', '1.21.1']) {
                assert.equal(jump(version, { vel: new Vec3(0, 0.6, 0) }), f32(0.42), version);
            }
            for (const version of ['1.21.3', '1.21.11', '26.3']) {
                assert.equal(jump(version, { vel: new Vec3(0, 0.6, 0) }), 0.6, version);
                assert.equal(jump(version), f32(0.42), version);
            }
        });

        it('adds the Jump Boost of the effects as they are, as well as of the level a state keeps', () => {
            const expected = f32(f32(0.42) + f32(f32(0.1) * 2));
            const { registry } = worldOf('26.3', floor);
            assert.equal(jump('26.3', { effects: { [registry.effectsByName.JumpBoost.id]: { amplifier: 1 } } }), expected);
            assert.equal(jump('26.3', { jumpBoost: 2 }), expected);
        });

        it('stops at no power, and halves on honey at the x and z of the block that supports it', () => {
            const { registry } = worldOf('26.3', floor);
            const jumpStrength = registry.attributesByName.jumpStrength.resource;
            assert.equal(jump('26.3', { vel: new Vec3(0, 0.25, 0), attributes: { [jumpStrength]: { value: 0, modifiers: [] } } }), 0.25);
            const edge = ({ registry: r, set }) => set(1, 0, 0, r.blocksByName.honey_block.defaultState);
            assert.equal(jump('26.3', { supportingBlockPos: new Vec3(1, 0, 0) }, edge), f32(f32(0.42) * 0.5));
            assert.equal(jump('26.3', { supportingBlockPos: new Vec3(0, 0, 0) }, edge), f32(0.42));
        });

        it('reads the block under it by the offset of its release, a double before 1.20 and getOnPos(0.500001F) from it', () => {
            // (1.19.4 Entity.java:771 0.5000001, 1.20 Entity.java:798): at this height the two fall in different cells
            const edge = ({ registry: r, set }) => set(1, 0, 0, r.blocksByName.honey_block.defaultState);
            const fields = { pos: new Vec3(0.5, 1.50000095, 0.5), supportingBlockPos: new Vec3(1, 0, 0) };
            assert.equal(jump('1.19.4', fields, edge), f32(0.42));
            assert.equal(jump('1.20', fields, edge), f32(f32(0.42) * 0.5));
        });

        it('reads the supporting block the engine keeps when the state has none', () => {
            const { world, registry } = worldOf('26.3', ({ registry: r, set }) => set(1, 0, 0, r.blocksByName.honey_block.defaultState));
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.8, 0.9375, 0.5, { onGround: true });
            physics.simulate(ctx, world);
            physics.simulate(ctx, world);
            assert.equal(ctx.state.onGround, true);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: 0, minZ: -1, maxX: 3, maxY: 3, maxZ: 2 }, 1);
            physics.jumpFromGround(ctx.state, sync);
            assert.equal(ctx.state.vel.y, f32(f32(0.42) * 0.5));
        });
    });

    it('starts the counters mineflayer-physics-utils does not keep, so a first fall counts', () => {
        const { registry, world } = worldOf('1.20.4', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 30, 0.5);
        delete ctx.state.fallDistance;
        delete ctx.state.itemUseTicks;
        delete ctx.state.autoSpinAttackTicks;
        delete ctx.state.autoJumpTime;
        physics.simulate(ctx, world);
        physics.simulate(ctx, world);
        assert.ok(ctx.state.fallDistance > 0);
        assert.equal(ctx.state.itemUseTicks, 0);
    });

    it('sends the engine the highest Fire Protection of the armor before 1.21', () => {
        // ProtectionEnchantment.transformFireDuration takes EnchantmentHelper.getEquipmentLevel, the highest level worn
        // (1.17 ProtectionEnchantment.java:59-66); from 1.21 the BURNING_TIME attribute carries it
        const fireProtectionOf = (version, levels) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const slots = [];
            const destinations = { head: 5, torso: 6, legs: 7, feet: 8 };
            for (const [slot, lvl] of Object.entries(levels)) slots[destinations[slot]] = { enchants: [{ name: 'fire_protection', lvl }] };
            const bot = { inventory: { slots }, getEquipmentDestSlot: destination => destinations[destination] };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, bot });
            physics.simulate(ctx, world);
            return physics.carries.get(ctx.state).player.state().fireProtection;
        };
        assert.equal(fireProtectionOf('1.20.4', {}), 0);
        assert.equal(fireProtectionOf('1.20.4', { head: 2, legs: 4 }), 4);
        assert.equal(fireProtectionOf('1.21.1', { head: 2, legs: 4 }), 0);
    });

    it('takes a flying player for off the ladder from 1.21.4', () => {
        // Player.onClimbable: abilities.flying ? false : super.onClimbable() (1.21.4 PlayerEntity.java:2219-2221; 1.21.3
        // has no override)
        const onClimbable = (version, flying) => {
            const { registry, world } = worldOf(version, ({ registry: r, Block, set }) => {
                floor({ registry: r, set });
                set(0, 2, 0, Block.fromProperties('ladder', { facing: 'north', waterlogged: false }, 0).stateId);
            });
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 2.2, 0.5, { flying, mayFly: true });
            physics.simulate(ctx, world);
            return ctx.state.onClimbable;
        };
        assert.equal(onClimbable('1.21.3', true), true);
        assert.equal(onClimbable('1.21.4', true), false);
        assert.equal(onClimbable('1.21.4', false), true);
    });

    it('lets the ground not stop the fall of a player removed at its 20th tick of death', () => {
        const { registry, world } = worldOf('1.18.2', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 1, 0.5, { onGround: true, vel: new Vec3(0, -0.0784000015258789, 0), removed: true });
        physics.simulate(ctx, world);
        assert.equal(ctx.state.pos.y, 1);
        assert.equal(ctx.state.vel.y, -0.1552320045166016);
    });

    it('tells the entities the player pushed back, but not those that carry passengers', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const cow = fields => ({
            id: 7, name: 'cow', living: true, position: new Vec3(1, 1, 0.5),
            boundingBox: { minX: 0.55, minY: 1, minZ: 0.05, maxX: 1.45, maxY: 2.4, maxZ: 0.95 }, ...fields,
        });
        const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [cow()] });
        physics.simulate(ctx, world);
        const push = (0.5 / Math.sqrt(0.5)) * f32(0.05);
        assert.deepEqual(ctx.state.pushedEntities, [{ id: 7, x: push, z: 0 }]);
        assert.equal(ctx.state.vel.x, -push);
        const carrying = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [cow({ isVehicle: true })] });
        physics.simulate(carrying, world);
        assert.deepEqual(carrying.state.pushedEntities, []);
        assert.equal(carrying.state.vel.x, -push);
    });

    it('lets the team collision rules keep an entity from pushing the player', () => {
        // EntitySelector.pushableBy: the pusher's rule never, either rule pushOwnTeam between allies, either rule
        // pushOtherTeams between others (1.9.4 EntityPredicate.java:57-85, 1.21.11 EntitySelector.java:27-54); the
        // scoreboard name is a player's name, another entity's UUID; 1.21.5 sends the rule's id, 1.21.9 push_own_team
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const pushed = (cowRule, botRule, sameTeam) => {
            const cowTeam = cowRule && { collisionRule: cowRule };
            const botTeam = sameTeam ? cowTeam : botRule && { collisionRule: botRule };
            const teamMap = { ...(cowTeam && { 'cow-uuid': cowTeam }), ...(botTeam && { bot: botTeam }) };
            const bot = { username: 'bot', teamMap, entities: { 7: { type: 'mob', uuid: 'cow-uuid' } } };
            const cow = {
                id: 7, name: 'cow', living: true, position: new Vec3(1, 1, 0.5),
                boundingBox: { minX: 0.55, minY: 1, minZ: 0.05, maxX: 1.45, maxY: 2.4, maxZ: 0.95 },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [cow], bot });
            physics.simulate(ctx, world);
            return ctx.state.pushedEntities.length === 1;
        };
        assert.equal(pushed(undefined, undefined, false), true);
        assert.equal(pushed('always', 'always', false), true);
        assert.equal(pushed('never', undefined, false), false);
        assert.equal(pushed(undefined, 'never', false), false);
        assert.equal(pushed(1, undefined, false), false);
        assert.equal(pushed('pushOwnTeam', undefined, true), false);
        assert.equal(pushed('pushOwnTeam', 'always', false), true);
        assert.equal(pushed('push_own_team', undefined, true), false);
        assert.equal(pushed(3, undefined, true), false);
        assert.equal(pushed('pushOtherTeams', 'always', false), false);
        assert.equal(pushed(undefined, 'pushOtherTeams', false), false);
        assert.equal(pushed('pushOtherTeams', undefined, true), true);
    });

    it("takes a tamed animal's team from its owner", () => {
        // TamableAnimal.getTeam: tamed with an owner the client holds, the owner's team before its own to 1.21.4
        // (1.9.4 TameableEntity.java:181-190, 1.21.4 :199-208), its own first from 1.21.5 and then the root owner's
        // (1.21.11 TamableAnimal.java:179-191, 26.3 :186-200); the synced flags' tame bit 4 and the owner UUID at 17 and
        // 18; pushableBy then (1.21.11 EntitySelector.java:27-54)
        const pushed = (version, { tame = true, owner = 'bot-uuid', wolfTeam, botTeam = { collisionRule: 'pushOwnTeam' }, otherTeam } = {}) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const metadata = [];
            metadata[17] = tame ? 4 : 0;
            metadata[18] = owner;
            const teamMap = { bot: botTeam, ...(wolfTeam && { 'wolf-uuid': wolfTeam }), ...(otherTeam && { alex: otherTeam }) };
            const bot = {
                username: 'bot', teamMap, entity: { uuid: 'bot-uuid', username: 'bot' },
                entities: {
                    7: { type: 'mob', name: 'wolf', uuid: 'wolf-uuid', metadata },
                    8: { type: 'player', username: 'alex', uuid: 'alex-uuid' },
                },
            };
            const wolf = {
                id: 7, name: 'wolf', living: true, position: new Vec3(1, 1, 0.5),
                boundingBox: { minX: 0.7, minY: 1, minZ: 0.2, maxX: 1.3, maxY: 1.85, maxZ: 0.8 },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [wolf], bot });
            physics.simulate(ctx, world);
            return ctx.state.pushedEntities.length === 1;
        };
        for (const version of ['1.21.1', '1.21.11']) {
            // the bot's own wolf on no team: allied with the bot's pushOwnTeam
            assert.equal(pushed(version), false, version);
            assert.equal(pushed(version, { tame: false }), true, version);
            // another loaded player's wolf, the player's team never
            assert.equal(pushed(version, { owner: 'alex-uuid', otherTeam: { collisionRule: 'never' } }), false, version);
            // an owner the client does not hold
            assert.equal(pushed(version, { owner: 'gone-uuid' }), true, version);
        }
        // the wolf's own team, another than its owner's: to 1.21.4 the owner's counts
        assert.equal(pushed('1.21.1', { wolfTeam: { collisionRule: 'always' } }), false);
        assert.equal(pushed('1.21.11', { wolfTeam: { collisionRule: 'always' } }), true);
    });

    it('never slows a 1.14 spectator by the sneak key, which its flight then divides by 0.3D', () => {
        // D-192: 1.14.4 KeyboardInput.tick(var1, isSpectator()) slows by the sneak key unless spectating (:22-25); a
        // survival player's sneak slows on 1.14.4, and from 1.15 only crouching does (LocalPlayer.isMovingSlowly)
        const slowly = (version, fields) => {
            const physics = new adapter.NativePhysics(worldOf(version, () => {}).registry);
            const { state } = playerState(0.5, 10, 0.5, { control: controls({ sneak: true, forward: true }), ...fields });
            return physics.isMovingSlowly(state);
        };
        assert.equal(slowly('1.14.4', { gameMode: 'spectator', flying: true, mayFly: true }), false);
        assert.equal(slowly('1.14.4', {}), true);
        assert.equal(slowly('1.13.2', { gameMode: 'spectator', flying: true, mayFly: true }), true);
    });

    it('is pushed by no bat, parrot, ender dragon, vex or spectator', () => {
        // Bat.doPush/pushEntities are empty (1.21.11 Bat.java:91-96), Parrot.doPush skips players (Parrot.java:385-389),
        // EnderDragon.aiStep never pushes (EnderDragon.java:152), the vex has noPhysics through its tick (Vex.java:73-75)
        // and a spectator through its own (Player.java:239), which Entity.push(Entity) skips (Entity.java:1752-1754)
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const bot = { entities: { 7: { type: 'player', username: 'watcher' } }, players: { watcher: { gamemode: 3 } } };
        const pushedBy = (name, playerBot) => {
            const entity = {
                id: 7, name, living: true, position: new Vec3(1, 1, 0.5),
                boundingBox: { minX: 0.55, minY: 1, minZ: 0.05, maxX: 1.45, maxY: 2.4, maxZ: 0.95 },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [entity], ...(playerBot && { bot: playerBot }) });
            physics.simulate(ctx, world);
            return ctx.state.pushedEntities.length === 1;
        };
        assert.equal(pushedBy('cow'), true);
        for (const name of ['bat', 'parrot', 'ender_dragon', 'vex']) assert.equal(pushedBy(name), false, name);
        assert.equal(pushedBy('player', bot), false);
        assert.equal(pushedBy('player', { ...bot, players: { watcher: { gamemode: 0 } } }), true);
    });

    it('sends a spectating player as one the riptide spin\'s sweep leaves out', () => {
        // D-197: getEntities(this, minmax) is NO_SPECTATORS (26.3 LivingEntity.java:3188-3195, EntityGetter.java:29-31)
        const physics = new adapter.NativePhysics(worldOf('1.21.1', floor).registry);
        const bot = { entities: { 7: { type: 'player', username: 'watcher' } }, players: { watcher: { gamemode: 3 } } };
        const flags = (name, playerBot) => {
            const entity = {
                id: 7, name, living: true, position: new Vec3(1, 1, 0.5),
                boundingBox: { minX: 0.55, minY: 1, minZ: 0.05, maxX: 1.45, maxY: 2.4, maxZ: 0.95 },
            };
            const { state } = playerState(0.5, 1, 0.5, { nearbyEntities: [entity], bot: playerBot });
            return physics.entitiesOf(state)[adapter.NATIVE_PLAYER_ENTITY_LENGTH - 1];
        };
        assert.equal(flags('player', bot) & adapter.NativePlayerEntityFlag.SPECTATOR, adapter.NativePlayerEntityFlag.SPECTATOR);
        assert.equal(flags('player', { ...bot, players: { watcher: { gamemode: 0 } } }) & adapter.NativePlayerEntityFlag.SPECTATOR, 0);
        assert.equal(flags('cow', bot) & adapter.NativePlayerEntityFlag.SPECTATOR, 0);
    });

    it('is not pushed by a mob while it sleeps or once dead', () => {
        // LivingEntity.push(Entity): if (!isSleeping()) (1.21.11 LivingEntity.java:2219-2223); isPushable: isAlive()
        // (:3201-3203), getHealth() > 0
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const pushed = bot => {
            const cow = {
                id: 7, name: 'cow', living: true, position: new Vec3(1, 1, 0.5),
                boundingBox: { minX: 0.55, minY: 1, minZ: 0.05, maxX: 1.45, maxY: 2.4, maxZ: 0.95 },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [cow], bot });
            physics.simulate(ctx, world);
            return ctx.state.pushedEntities.length === 1;
        };
        assert.equal(pushed({ health: 20 }), true);
        assert.equal(pushed({ health: 20, isSleeping: true }), false);
        assert.equal(pushed({ health: 0 }), false);
    });

    it('meets a happy ghast on its still timeout from the side', () => {
        // HappyGhast.canBeCollidedWith: from below its top only on the still timeout, the synced STAYS_STILL
        // (1.21.6 HappyGhast.java:544-553, 1.21.11 :549-558, :523-524)
        const { registry, world } = worldOf('1.21.4', floor);
        const physics = new adapter.NativePhysics(registry);
        const walkInto = staysStill => {
            const ghast = {
                id: 9, name: 'happy_ghast', living: true, baby: false, staysStill, position: new Vec3(3, 1, 0.5),
                boundingBox: { minX: 1, minY: 1, minZ: -1.5, maxX: 5, maxY: 5, maxZ: 2.5 },
                afterTick: { position: new Vec3(3, 1, 0.5), boundingBox: { minX: 1, minY: 1, minZ: -1.5, maxX: 5, maxY: 5, maxZ: 2.5 } },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, vel: new Vec3(0.5, 0, 0), nearbyEntities: [ghast] });
            physics.simulate(ctx, world);
            return ctx.state;
        };
        const still = walkInto(true);
        assert.equal(still.isCollidedHorizontally, true);
        assert.equal(still.pos.x, 1 - f32(0.3));
        assert.ok(walkInto(false).pos.x > 1 - f32(0.3));
    });

    it('is not pushed while it carries passengers, which it still pushes, from 1.9 (D-206)', () => {
        // Entity.push moves each side only where it is no vehicle (1.9.4 Entity.java:1164-1169, 26.3 :1931-1937)
        const pushed = (version, fields) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const cow = {
                id: 7, name: 'cow', living: true, position: new Vec3(1, 1, 0.5),
                boundingBox: { minX: 0.55, minY: 1, minZ: 0.05, maxX: 1.45, maxY: 2.4, maxZ: 0.95 },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [cow], ...fields });
            physics.simulate(ctx, world);
            return [ctx.state.vel.x < 0, ctx.state.pushedEntities.length];
        };
        assert.deepEqual(pushed('1.21.11', {}), [true, 1]);
        assert.deepEqual(pushed('1.21.11', { isVehicle: true }), [false, 1]);
        assert.deepEqual(pushed('1.21.11', { bot: { entity: { passengers: [{ id: 9 }] } } }), [false, 1]);
        assert.deepEqual(pushed('1.9.4', { isVehicle: true }), [false, 1]);
    });

    it('walks through a dying shulker or happy ghast, which a dying mob pushes only before 1.11 (D-204)', () => {
        // Shulker and HappyGhast.canBeCollidedWith need isAlive (1.9.4 ShulkerEntity.java:420-421, 26.3
        // Shulker.java:439-441, HappyGhast.java:522-523); LivingEntity.isPushable wants it from 1.11 (1.10.2
        // LivingEntity.java:1891-1892, 1.11.2 :1952-1953)
        const walkInto = (version, name, alive) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const box = name === 'shulker'
                ? { minX: 1, minY: 1, minZ: 0, maxX: 2, maxY: 2, maxZ: 1 }
                : { minX: 1, minY: 1, minZ: -1.5, maxX: 5, maxY: 5, maxZ: 2.5 };
            const position = new Vec3((box.minX + box.maxX) / 2, 1, (box.minZ + box.maxZ) / 2);
            const mob = {
                id: 9, name, living: true, baby: false, staysStill: true, ...(alive === undefined ? {} : { alive }), position,
                boundingBox: box, afterTick: { position, boundingBox: box },
            };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, vel: new Vec3(0.5, 0, 0), nearbyEntities: [mob] });
            physics.simulate(ctx, world);
            return ctx.state;
        };
        for (const [version, name] of [['1.10.2', 'shulker'], ['1.21.4', 'shulker'], ['1.21.11', 'happy_ghast']]) {
            assert.equal(walkInto(version, name, undefined).pos.x, 1 - f32(0.3), `${version} ${name} alive`);
            assert.equal(walkInto(version, name, true).pos.x, 1 - f32(0.3), `${version} ${name} alive`);
            const dying = walkInto(version, name, false);
            assert.equal(dying.isCollidedHorizontally, false, `${version} ${name} dying`);
            assert.ok(dying.pos.x > 1 - f32(0.3), `${version} ${name} dying`);
        }
        // the player's box meets the dying shulker's after the move: before 1.11 it is pushed back by it
        const pushes = version => walkInto(version, 'shulker', false).pushedEntities.length;
        assert.equal(pushes('1.10.2'), 1);
        assert.equal(pushes('1.11.2'), 0);
    });

    it('meets a boat the client ticks before the player where its tick left it', () => {
        const { registry, world } = worldOf('1.16.5', () => {});
        const physics = new adapter.NativePhysics(registry);
        const boatAt = y => ({ position: new Vec3(0.5, y, 0.5), boundingBox: { minX: -0.1875, minY: y, minZ: -0.1875, maxX: 1.1875, maxY: y + 0.5625, maxZ: 1.1875 } });
        const standing = ticksBeforePlayer => {
            const boat = { id: 3, name: 'boat', living: false, ...boatAt(9.4375), ticksBeforePlayer, afterTick: boatAt(9.3375) };
            const ctx = playerState(0.5, 10, 0.5, { onGround: true, vel: new Vec3(0, -0.0784000015258789, 0), nearbyEntities: [boat] });
            physics.simulate(ctx, world);
            return ctx.state.onGround;
        };
        assert.equal(standing(false), true);
        assert.equal(standing(true), false);
    });

    it('collides with the world border the bot knows', () => {
        // Entity.collide collects the border's shape (1.21.11 Entity.java:1146-1160) as the server set it
        // (ClientPacketListener: the border packets, mineflayer's bot.worldBorder)
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativePhysics(registry);
        const bot = { worldBorder: { centerX: 0, centerZ: 0, size: 3 }, game: { minY: -64, height: 384 } };
        const ctx = playerState(1.1, 1, 0.5, { onGround: true, vel: new Vec3(1, 0, 0), bot });
        physics.simulate(ctx, world);
        assert.equal(ctx.state.pos.x, 2 - 0.30000001192092896);
        assert.ok(ctx.state.isCollidedHorizontally);
        const bounds = physics.syncOf(world).world.worldBounds();
        assert.deepEqual([bounds.borderSize, bounds.minBuildHeight, bounds.maxBuildHeight], [3, -64, 320]);
    });

    it('collides with a moving border where its edges were before its last tick, held to the absolute max size', () => {
        // from 1.21.11 Entity.collide's WorldBorder.getCollisionShape puts the edges at getMinX(0.0F), a moving border's
        // previous size (1.21.11 WorldBorder.java:350-356, 441-449), each clamped to +-absoluteMaxSize (:353-381), which
        // the init packet sets (ClientPacketListener.java:1805)
        const { registry, world } = worldOf('1.21.11', floor);
        const physics = new adapter.NativePhysics(registry);
        const bot = { worldBorder: { centerX: 0, centerZ: 0, size: 2, previousSize: 3, absoluteMaxSize: 29999984 }, game: { minY: -64, height: 384 } };
        const ctx = playerState(1.1, 1, 0.5, { onGround: true, vel: new Vec3(1, 0, 0), bot });
        physics.simulate(ctx, world);
        assert.equal(ctx.state.pos.x, 2 - 0.30000001192092896);
        const clamped = { worldBorder: { centerX: 0, centerZ: 0, size: 100, absoluteMaxSize: 1 }, game: { minY: -64, height: 384 } };
        const held = playerState(0.5, 1, 0.5, { onGround: true, vel: new Vec3(1, 0, 0), bot: clamped });
        physics.simulate(held, world);
        assert.equal(held.state.pos.x, 1 - 0.30000001192092896);
    });

    it('keeps the box vanilla keeps before 1.17 while the player stays where it left it', () => {
        const { registry, world } = worldOf('1.16.5', floor);
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 1, 0.5, { onGround: true });
        physics.simulate(ctx, world);
        const kept = ctx.state.keptBoundingBox;
        assert.equal(kept.bb.minY, 1);
        assert.equal(kept.bb.maxY, 1 + f32(1.8));
        ctx.state.pos.set(2.5, 1, 2.5);
        physics.simulate(ctx, world);
        assert.equal(ctx.state.keptBoundingBox.bb.minX, 2.5 - f32(0.6) / 2);
    });

    it('refits the kept box to the synced pose, 6 dying before 1.17', () => {
        // Entity.refreshDimensions on the pose the server syncs: before 1.17 the pose ordinal 6 is DYING, fixed 0.2F by
        // 0.2F (1.14 EntityPose.java:4-10, PlayerEntity.java:117; 1.16.5 EntityPose.java:4-10, PlayerEntity.java:120),
        // LONG_JUMPING taking 6 from 1.17 (1.17 EntityPose.java:4-11)
        const refitted = (version, pose) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true });
            physics.simulate(ctx, world);
            ctx.state.pose = pose;
            physics.refreshDimensions(ctx.state);
            return [ctx.state.keptBoundingBox.width, ctx.state.keptBoundingBox.height];
        };
        assert.deepEqual(refitted('1.16.5', 6), [f32(0.2), f32(0.2)]);
        assert.deepEqual(refitted('1.14.4', 6), [f32(0.2), f32(0.2)]);
        assert.deepEqual(refitted('1.16.5', 5), [f32(0.6), f32(1.5)]);
    });

    it('refits the kept box in the tick the player changes its own pose', () => {
        // setPose goes through the synced data, whose change runs calculateDimensions on this side too (1.15 Entity.java:2246,
        // 1.16.5 :446-448, :2323-2346): the box is 1.5 high when the tick that crouched ends. From 1.17 no box is kept.
        for (const version of ['1.14.4', '1.16.5']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, control: controls({ sneak: true }) });
            physics.simulate(ctx, world);
            assert.equal(ctx.state.pose, adapter.Pose.SNEAKING, version);
            assert.equal(ctx.state.keptBoundingBox.height, f32(1.5), version);
            assert.equal(ctx.state.keptBoundingBox.bb.maxY - ctx.state.keptBoundingBox.bb.minY, f32(1.5), version);
        }
    });

    it('sends the engine an ultrawarm dimension in the nether', () => {
        // Entity.updateFluidHeightAndDoFluidPushing: ultraWarm() (FAST_LAVA from 1.21.11) ? 0.007 : 0.0023333333333333335
        // (1.16 Entity.java:1049, 1.21.11 :1519); only the nether's dimension type is (1.16 DimensionType.java:80-95)
        const ultraWarmIn = dimension => {
            const { registry, world } = worldOf('1.21.1', floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, bot: { game: { dimension } } });
            physics.simulate(ctx, world);
            return physics.carries.get(ctx.state).player.state().ultraWarm;
        };
        assert.equal(ultraWarmIn('the_nether'), true);
        assert.equal(ultraWarmIn('overworld'), false);
        assert.equal(ultraWarmIn('the_end'), false);
    });

    it('imports the on-fire flag of the synced data at 0, and Fire Resistance from the effects', () => {
        // Entity.isOnFire reads flag 0 on a client (1.20.4 Entity.java:2121-2124); LivingEntity.hurt refuses fire
        // under the effect (1.20.4 LivingEntity.java:1104)
        const sent = (flags, effects) => {
            const { registry, world } = worldOf('1.20.4', () => {});
            const physics = new adapter.NativePhysics(registry);
            const ids = Object.fromEntries(effects.map(name => [registry.effectsByName[name].id, { amplifier: 0 }]));
            const ctx = playerState(0.5, 10, 0.5, { effects: ids, bot: { entity: { metadata: [flags] } } });
            physics.simulate(ctx, world);
            const state = physics.carries.get(ctx.state).player.state();
            return [state.onFireFlag, state.fireResistance];
        };
        assert.deepEqual(sent(0, []), [false, false]);
        assert.deepEqual(sent(1, []), [true, false]);
        assert.deepEqual(sent(2, ['FireResistance']), [false, true]);
    });

    it('tells a sweet berry bush of age 0 from a grown one', () => {
        // age 0 deals nothing (1.14 SweetBerryBushBlock.java:62, 26.3 :80)
        const { registry } = worldOf('1.20.4', () => {});
        const Block = require('prismarine-block')(registry);
        const bush = registry.blocksByName.sweet_berry_bush;
        const effect = age => adapter.describeBlock(Block.fromStateId(bush.minStateId + age, 0)).effect;
        assert.equal(effect(0), 28);
        assert.equal(effect(1), 9);
        assert.equal(effect(3), 9);
    });

    it('imports the no-gravity flag the server syncs at 5 from 1.10', () => {
        // Entity.NO_GRAVITY, the sixth synced data (1.10.2 Entity.java:134-139, 1.21.11 Entity.java:245-259);
        // getGravity is 0 with it (1.21.11 Entity.java:1443-1445), LivingEntity.travel takes no 0.08 (1.10.2)
        const falls = (version, noGravity) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 10, 0.5, { bot: { entity: { metadata: [0, 300, '', false, false, noGravity] } } });
            physics.simulate(ctx, world);
            return [physics.carries.get(ctx.state).player.state().noGravity, ctx.state.vel.y];
        };
        // with no gravity and no input the player stays still: no 0.08, then 0 * 0.98
        assert.deepEqual(falls('1.21.4', true), [true, 0]);
        assert.deepEqual(falls('1.10.2', true), [true, 0]);
        const [, fell] = falls('1.21.4', false);
        assert.ok(fell < 0);
        const [ignored, oldFell] = falls('1.9.4', true);
        assert.equal(ignored, false);
        assert.ok(oldFell < 0);
    });

    it('holds the eyes at 0.2F asleep and 1.62F dying, with the dying box unscaled', () => {
        // LivingEntity.getEyeHeight: 0.2F sleeping (1.14 LivingEntity.java:2749, 1.20.4 :3267); the default 1.62F
        // dying (1.14 PlayerEntity.java:1792-1802); from 1.20.5 the fixed sleeping and dying dimensions the scale
        // leaves, 0.2F and 1.62F eyes (1.20.5 LivingEntity.java:170, 3251, PlayerEntity.java:147,
        // EntityDimensions.java:29-38; 1.21.11 Avatar.java:55); before 1.14 0.2F asleep (1.12.2 PlayerEntity.java:1769-1780)
        const eye = (version, pose, scale = 1) => {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            return physics.eyeHeight(playerState(0.5, 1, 0.5, { pose, scale }).state);
        };
        for (const version of ['1.14.4', '1.20.4', '1.21.4']) {
            assert.equal(eye(version, adapter.Pose.SLEEPING), f32(0.2), version);
            assert.equal(eye(version, adapter.Pose.DYING), 1.62, version);
        }
        assert.equal(eye('1.12.2', adapter.Pose.SLEEPING), f32(0.2));
        // D-218: 1.9 - 1.12 put a glider's eyes at 0.4F, after the sneak's 1.54F (1.9.4 Player.java:1733-1744,
        // 1.12.2 :1753-1764); 1.8.8 has no glide
        const glider = (version, fields) => new adapter.NativePhysics(worldOf(version, () => {}).registry)
            .eyeHeight(playerState(0.5, 10, 0.5, { fallFlying: true, ...fields }).state);
        for (const version of ['1.9.4', '1.10.2', '1.11.2', '1.12.2']) {
            assert.equal(glider(version), f32(0.4), version);
            assert.equal(glider(version, { control: controls({ sneak: true }), prevControl: controls({ sneak: true }) }), f32(f32(1.62) - f32(0.08)), version);
        }
        assert.equal(glider('1.8.8'), 1.62);
        assert.equal(eye('1.21.4', adapter.Pose.DYING, 2), 1.62);
        assert.equal(eye('1.21.4', adapter.Pose.SLEEPING, 2), f32(0.2));
        assert.equal(eye('1.21.4', adapter.Pose.STANDING, 2), f32(f32(1.62) * 2));
        const physics = new adapter.NativePhysics(worldOf('1.21.4', () => {}).registry);
        assert.deepEqual(physics.poseSize(adapter.Pose.DYING, 2), { width: f32(0.2), height: f32(0.2) });
        assert.deepEqual(physics.poseSize(adapter.Pose.STANDING, 2), { width: f32(1.2), height: f32(3.6) });
    });

    it('reads the eyes 0.1111111119389534 below them from 1.16 to 1.20.4, at them from 1.20.5', () => {
        // Entity.updateFluidOnEyes: getEyeY() - 0.11111111F (1.20.4 Entity.java:1222), getEyeY() (1.20.5 :1237)
        const eyesInWater = version => {
            const { registry, world } = worldOf(version, ({ Block: B, set }) => {
                set(0, 0, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
                set(0, 1, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
            });
            const physics = new adapter.NativePhysics(registry);
            // the eyes at 1.95, above the source's top of 1 + 8 / 9F
            const { state } = playerState(0.5, 0.33, 0.5);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: -1, minZ: -1, maxX: 2, maxY: 3, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            return state.eyeInWater;
        };
        assert.equal(eyesInWater('1.20.4'), true);
        assert.equal(eyesInWater('1.20.5'), false);
        assert.equal(eyesInWater('1.21.1'), false);
    });

    it('adds the water height and 0.11111111F before the cell y in 1.13 - 1.15', () => {
        // Entity.isUnderLiquid: y + (getHeight + 0.11111111F) in float (1.13.2 and 1.15.2 bytecode: getHeight, ldc
        // 0.11111111f, fadd, fadd): a level 7 water at y -16 tops at f32(-16 + f32(1/9F + 0.11111111F)) = -15.777777671813965,
        // f32(f32(-16 + 1/9F) + 0.11111111F) would be -15.777778625488281
        const { registry, world } = worldOf('1.14.4', ({ Block: B, set }) => set(0, -16, 0, B.fromProperties('water', { level: 7 }, 0).stateId));
        const physics = new adapter.NativePhysics(registry);
        const { state } = playerState(0.5, -15.777778 - f32(1.62), 0.5);
        const sync = physics.syncOf(world);
        sync.sync(world, { minX: -1, minY: -18, minZ: -1, maxX: 2, maxY: -13, maxZ: 2 }, 2);
        physics.updateFluids(state, sync);
        assert.equal(state.eyeInWater, true);
    });

    it('takes the eyes in water at its top, and not beside an unloaded chunk, from 26.1', () => {
        // EntityFluidInteraction.update: eyeY >= fluidBottom && eyeY <= fluidTop in double, and no fluid at all while a
        // chunk under the cells widened by one is not loaded (26.1 EntityFluidInteraction.java:44, 70, 94-110)
        const eyes = (release, x, eyeY, loadedFromX16 = true) => {
            const { registry, world } = worldOf(release, ({ Block: B, set }) => set(Math.floor(x), 0, 0, B.fromProperties('water', { level: 0 }, 0).stateId));
            if (!loadedFromX16) unloadedFromX16(world);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(x, eyeY - f32(1.62), 0.5);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: x - 2, minY: -3, minZ: -2, maxX: x + 2, maxY: 3, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            return [state.isInWater, state.eyeInWater];
        };
        const top = f32(8 / 9);
        assert.deepEqual(eyes('1.21.4', 0.5, top), [true, false]);
        assert.deepEqual(eyes('26.1', 0.5, top), [true, true]);
        assert.deepEqual(eyes('26.1', 0.5, top + 1e-9), [true, false]);
        assert.deepEqual(eyes('1.21.4', 15.5, 0.5, false), [true, true]);
        assert.deepEqual(eyes('26.1', 15.5, 0.5, false), [false, false]);
    });

    it('leaves out the cell over a box whose deflated top is a whole number (D-220)', () => {
        // Entity.updateFluidHeightAndDoFluidPushing walks floor(min) to ceil(max), exclusive (1.13.2 Entity.java:2512-2517,
        // 1.20.5 :3084-3090, 1.21.11 :3599-3605): water over a head whose box less 0.001 ends on 64 is not touched
        const y = 62.20100004768371;
        assert.equal((y + f32(1.8)) - 0.001, 64);
        const touches = (release, at) => {
            const { registry, world } = worldOf(release, ({ Block: B, set }) => set(0, 64, 0, B.fromProperties('water', { level: 0 }, 0).stateId));
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, at, 0.5);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -2, minY: 60, minZ: -2, maxX: 2, maxY: 66, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            return state.isInWater;
        };
        for (const release of ['1.13.2', '1.16.5', '1.20.4', '1.21.11', '26.1']) {
            assert.equal(touches(release, y), false, release);
            assert.equal(touches(release, y + 1e-6), true, release);
        }
    });

    it('takes the water before 1.13 with the box updateSize left, 1.65 high in a sneak (D-219)', () => {
        // Entity.handleWaterMovement reads getEntityBoundingBox (1.12.2 Entity.java:1051), the box a sneak's updateSize
        // made 1.65 high, not one sized from a pose: the standing box would reach the row of water over the head
        const touches = kept => {
            const { registry, world } = worldOf('1.12.2', ({ registry: r, set }) => set(0, 64, 0, r.blocksByName.water.id << 4));
            const physics = new adapter.NativePhysics(registry);
            const pos = new Vec3(0.5, 62.7, 0.5);
            const half = f32(0.6) / 2;
            const keptBoundingBox = kept
                ? { bb: { minX: 0.5 - half, minY: 62.7, minZ: 0.5 - half, maxX: 0.5 + half, maxY: 62.7 + f32(1.65), maxZ: 0.5 + half }, pos: pos.clone(), width: f32(0.6), height: f32(1.65) }
                : null;
            const { state } = playerState(pos.x, pos.y, pos.z, { keptBoundingBox });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -2, minY: 60, minZ: -2, maxX: 2, maxY: 66, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            return state.isInWater;
        };
        assert.equal(touches(false), true);
        assert.equal(touches(true), false);
    });

    it('takes a knockback with its resistance, and refuses the direction vanilla redraws', () => {
        // LivingEntity.knockback by the jars from 1.17.1 (native/fixtures/java-oracle-knockback.txt): power * (1 - resistance)
        const { registry, world } = worldOf('1.21.11', () => {});
        const physics = new adapter.NativePhysics(registry);
        const { state } = playerState(0.5, 1, 0.5);
        void world;
        state.onGround = true;
        state.vel.set(0.2, -0.08, 0);
        state.attributes = { [physics.attributeKey('knockbackResistance')]: { value: 0.5, modifiers: [] } };
        assert.equal(physics.knockback(state, { strength: 0.4, x: 1, z: 0 }), true);
        assert.deepEqual([state.vel.x, state.vel.y, state.vel.z], [0.1 - 0.2, Math.min(0.4, -0.04 + 0.2), 0]);
        const before = state.vel.clone();
        assert.equal(physics.knockback(state, { strength: 0.4, x: 1e-4, z: 0 }), false);
        assert.deepEqual(state.vel.toArray(), before.toArray());
    });

    it('lags isUnderWater a tick behind the eyes, beside an unloaded chunk too', () => {
        // Player.updateIsUnderwater reads the eyes the last tick left (26.3 Entity.java:1677-1681, Player.java:367-370): the
        // first tick in the water has the eyes under and isUnderWater false
        for (const [release, loaded, under] of [['1.21.4', true, true], ['26.1', true, true], ['26.1', false, false]]) {
            const { registry, world } = worldOf(release, ({ Block: B, set }) => {
                for (let y = 0; y < 4; y += 1) set(15, y, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
            });
            if (!loaded) unloadedFromX16(world);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(15.5, 0.5, 0.5);
            state.isUnderWater = false;
            state.eyeInWater = false;
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: 13, minY: -3, minZ: -2, maxX: 18, maxY: 6, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            assert.equal(state.eyeInWater, under, `${release} ${loaded}`);
            assert.equal(state.isUnderWater, false, `${release} ${loaded}`);
        }
    });

    it('tops a water source under a block sturdy below at 1 for the eyes in 26.3', () => {
        // FluidState.getHeightForCamera (26.3 FluidState.java:53-60, EntityFluidInteraction.java:84-87)
        const eyes = (release, above) => {
            const { registry, world } = worldOf(release, ({ registry: r, Block: B, set }) => {
                set(0, 0, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
                set(0, 1, 0, r.blocksByName[above].defaultState);
            });
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 0.95 - f32(1.62), 0.5);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -2, minY: -3, minZ: -2, maxX: 2, maxY: 3, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            return state.eyeInWater;
        };
        assert.equal(eyes('26.2', 'stone'), false);
        assert.equal(eyes('26.3', 'stone'), true);
        // leaves have an empty support shape, a bottom slab a full bottom face
        assert.equal(eyes('26.3', 'oak_leaves'), false);
        assert.equal(eyes('26.3', 'oak_slab'), true);
    });

    it('uses explicit boat context for passenger water and eye decisions at the historical boundaries', () => {
        const sample = (version, boatVehicle, fields = {}) => {
            const { registry, world } = worldOf(version, ({ registry: r, set }) => {
                const water = r.blocksByName.water.defaultState ?? r.blocksByName.water.id << 4;
                for (let y = 0; y <= 3; y++) set(0, y, 0, water);
            });
            const physics = new adapter.NativePhysics(registry, { version });
            const { state } = playerState(0.5, 0, 0.5, { passenger: true, boatVehicle, ...fields });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: -1, minZ: -1, maxX: 2, maxY: 4, maxZ: 2 }, 2);
            physics.updateFluids(state, sync);
            return [state.isInWater, state.eyeInWater ?? state.isUnderWater];
        };
        const boat = (underwater = false, min = { x: 0, y: 0, z: 0 }, max = { x: 1, y: 3, z: 1 }) => ({
            underwater, boundingBox: { min, max },
        });
        assert.deepEqual(sample('1.8.8', boat()), [true, true]); // Source stand-in: 1.8.9.
        for (const version of ['1.9.4', '1.12.2', '1.13.2', '1.15.2']) {
            assert.deepEqual(sample(version, boat(true)), [false, false], version);
        }
        assert.deepEqual(sample('1.19.2', boat(true)), [false, true]);
        assert.deepEqual(sample('1.19.3', boat(true)), [true, true]);
        assert.deepEqual(sample('1.19.3', boat()), [false, false]);

        // 1.16.1 checks the full boat AABB; 1.16.2 only its inclusive Y interval.
        const elsewhere = boat(false, { x: 10, y: 0, z: 10 }, { x: 11, y: 3, z: 11 });
        assert.deepEqual(sample('1.16.1', elsewhere), [false, true]);
        assert.deepEqual(sample('1.16.2', elsewhere), [false, false]);
        const eyeSample = f32(1.62) - f32(0.11111111);
        const maximum = boat(false, { x: 0, y: 0, z: 0 }, { x: 1, y: eyeSample, z: 1 });
        assert.deepEqual(sample('1.16.1', maximum), [false, true]);
        assert.deepEqual(sample('1.16.2', maximum), [false, false]);

        const offsetOnly = boat(false, { x: 0, y: 1.5, z: 0 }, { x: 1, y: 1.55, z: 1 });
        assert.deepEqual(sample('1.20.4', offsetOnly), [false, false]);
        assert.deepEqual(sample('1.20.5', offsetOnly), [false, true]);
        assert.deepEqual(sample('1.20.4', boat(), { passenger: false }), [true, true]);
        assert.deepEqual(sample('1.20.4', undefined, { bot: { vehicle: { inWater: true } } }), [true, true]);
    });

    it('adjusts the modern passenger fluid scan without moving the eyes or collision dimensions', () => {
        for (const fluid of ['water', 'lava']) {
            const { registry, world } = worldOf('26.3', ({ Block: B, set }) => set(0, 1, 0, B.fromProperties(fluid, { level: 0 }, 0).stateId));
            const physics = new adapter.NativePhysics(registry);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: -1, minZ: -1, maxX: 2, maxY: 4, maxZ: 2 }, 2);
            const boatVehicle = { underwater: false, boundingBox: { min: { x: 10, y: 0, z: 10 }, max: { x: 11, y: 2, z: 11 } } };
            const { state } = playerState(0.5, 0, 0.5, {
                passenger: true, boatVehicle, isInWater: true, isInLava: true, eyeInWater: true, isUnderWater: true,
            });
            const before = physics.collider(state.pose, state.pos, state.scale);
            physics.updateFluids(state, sync);
            assert.deepEqual([state.isInWater, state.isInLava, state.eyeInWater, state.isUnderWater], [false, false, false, false]);

            // A box trimmed at 1.7 still scans the fluid cell at Y=1. Eyes at 1.62
            // remain inside that cell even though below the scan's new minimum.
            boatVehicle.boundingBox.max.y = 1.7;
            physics.updateFluids(state, sync);
            assert.equal(state.isInWater, fluid === 'water');
            assert.equal(state.isInLava, fluid === 'lava');
            assert.equal(state.eyeInWater, fluid === 'water');
            assert.deepEqual(physics.collider(state.pose, state.pos, state.scale), before);

            boatVehicle.boundingBox.max.y = 2;
            boatVehicle.underwater = true;
            physics.updateFluids(state, sync);
            assert.equal(state.isInWater, fluid === 'water');
            assert.equal(state.isInLava, fluid === 'lava');
            boatVehicle.underwater = false;
            state.passenger = false;
            physics.updateFluids(state, sync);
            assert.equal(state.isInWater, fluid === 'water');
            assert.equal(state.isInLava, fluid === 'lava');
        }
    });

    it('carries mutable boat geometry by value and clears it on replacement without corrupting native state', () => {
        const { registry, world } = worldOf('26.3', () => {});
        const physics = new adapter.NativePhysics(registry);
        const boatVehicle = { underwater: false, boundingBox: { min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 2, z: 1 } } };
        const { state } = playerState(0.5, 0, 0.5, { passenger: true, boatVehicle });
        const carry = physics.carryFor(state, null);
        assert.deepEqual(carry.player.state().boatVehicle, boatVehicle);
        assert.notEqual(carry.slow.boatVehicle, boatVehicle);
        boatVehicle.boundingBox.max.y = 1.7;
        assert.equal(physics.carryFor(state, null).player, carry.player);
        assert.equal(carry.player.state().boatVehicle.boundingBox.max.y, 1.7);
        boatVehicle.underwater = true;
        physics.carryFor(state, null);
        assert.equal(carry.player.state().boatVehicle.underwater, true);
        const prior = carry.player.state();
        boatVehicle.boundingBox.min.x = NaN;
        assert.throws(() => physics.simulate({ state }, world), /boatVehicle.*finite/);
        assert.equal(state.age, 0);
        assert.deepEqual(carry.player.state(), prior);
        delete state.attributes;
        assert.throws(() => physics.simulate({ state }, world), /boatVehicle.*finite/);
        assert.equal(Object.hasOwn(state, 'attributes'), false);
        assert.equal(state.age, 0);
        assert.deepEqual(carry.player.state(), prior);
        state.attributes = {};
        boatVehicle.boundingBox.min.x = 0;
        state.passenger = false;
        physics.carryFor(state, null);
        assert.equal(carry.player.state().passenger, false);
        assert.deepEqual(carry.player.state().boatVehicle, boatVehicle);
        delete state.boatVehicle;
        physics.carryFor(state, null);
        assert.equal(carry.player.state().boatVehicle, undefined);
        state.boatVehicle = boatVehicle;
        physics.carryFor(state, null);
        state.boatVehicle = null;
        physics.carryFor(state, null);
        assert.equal(carry.player.state().boatVehicle, undefined);
    });

    it('starts swimming under water without water at the feet before 1.17.1', () => {
        // Entity.updateSwimming: isSprinting() && isUnderWater() && !isPassenger(), from 1.17.1 also
        // getFluidState(blockPosition()).is(WATER) (1.17 Entity.java:1052; 1.17.1 bytecode 62-79; 1.18 :1032-1034)
        const swims = version => {
            const { registry, world } = worldOf(version, ({ Block: B, set }) => {
                set(0, 2, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
            });
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { sprinting: true, isInWater: true, isUnderWater: true });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: 0, minZ: -1, maxX: 2, maxY: 3, maxZ: 2 }, 2);
            physics.updateSwimming(state, sync);
            return state.swimming;
        };
        assert.equal(swims('1.16.5'), true);
        assert.equal(swims('1.17'), true);
        assert.equal(swims('1.17.1'), false);
        assert.equal(swims('1.18.2'), false);
    });

    it('does not crouch a flying player, whose sneak key keeps the flight speed', () => {
        // LocalPlayer.aiStep: crouching = !abilities.flying && ... (1.14 ClientPlayerEntity.java:582-586, 1.16.4
        // LocalPlayer.java:603, 1.21.11 :715), so isMovingSlowly is false; 1.14 slows by the key and undoes it
        // (ClientPlayerEntity.java:717-721)
        for (const version of ['1.14.4', '1.16.5', '1.21.1', '1.21.5']) {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const fly = sneak => {
                const ctx = playerState(0.5, 10, 0.5, { flying: true, mayFly: true, gameMode: 'creative' });
                ctx.state.control = controls({ forward: true, sneak });
                ctx.state.prevControl = controls({ sneak });
                physics.simulate(ctx, world);
                return ctx.state;
            };
            const [sneaking, plain] = [fly(true), fly(false)];
            assert.equal(sneaking.crouching, false, version);
            assert.equal(sneaking.vel.z, plain.vel.z, version);
            assert.ok(sneaking.vel.z > 0, version);
        }
    });

    it('moves with the pose of the tick before, which updatePlayerPose changes after the move', () => {
        // Player.tick ends with updatePlayerPose, after the move (1.18.1 Player.java:233, 276; 1.21.11 :249-274)
        for (const version of ['1.17', '1.20.4', '1.21.4']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true });
            ctx.state.control = controls({ sneak: true });
            physics.simulate(ctx, world);
            assert.equal(physics.carries.get(ctx.state).size.height, f32(1.8), version);
            assert.equal(ctx.state.pose, adapter.Pose.SNEAKING, version);
        }
    });

    it('ends a glide on a climbable block from 1.21.5, but not on vines from 1.21.11', () => {
        // LivingEntity.travelFallFlying: onClimbable() -> travelInAir, stopFallFlying (1.21.5 LivingEntity.java:2299-2302);
        // from 1.21.11 a fall-flying entity is not on a #can_glide_through block (LivingEntity.java:1675-1676)
        const glides = (version, name, props) => {
            const { registry, world } = worldOf(version, ({ Block, set }) => set(0, 2, 0, Block.fromProperties(name, props, 0).stateId));
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 2.2, 0.5, {
                fallFlying: true, pose: adapter.Pose.FALL_FLYING, validElytraEquipped: true, vel: new Vec3(0, 0, 0.3),
            });
            physics.simulate(ctx, world);
            return [ctx.state.fallFlying, ctx.state.pose];
        };
        const ladder = { facing: 'north', waterlogged: false };
        const vine = { east: false, north: true, south: false, up: false, west: false };
        assert.deepEqual(glides('1.21.4', 'ladder', ladder), [true, adapter.Pose.FALL_FLYING]);
        assert.deepEqual(glides('1.21.5', 'ladder', ladder), [false, adapter.Pose.STANDING]);
        // The installed registry aliases 1.21.10 to 1.21.9; use the exact earlier control.
        assert.deepEqual(glides('1.21.9', 'vine', vine), [false, adapter.Pose.STANDING]);
        assert.deepEqual(glides('1.21.11', 'vine', vine), [true, adapter.Pose.FALL_FLYING]);
        assert.deepEqual(glides('1.21.11', 'ladder', ladder), [false, adapter.Pose.STANDING]);
    });

    it('boosts with an attached rocket listed among the entities in its turn, and only once', () => {
        // each entity runs its whole tick in ClientLevel.tickEntities' order: a cow ticking before the rocket pushes the
        // glider first and the boost then halves that push (1.21.11 FireworkRocketEntity.java:124-131); a listed
        // rocket is not counted again in fireworkRockets
        const { registry, world } = worldOf('1.21.1', () => {});
        const physics = new adapter.NativePhysics(registry);
        const cow = {
            id: 7, name: 'cow', living: true, position: new Vec3(1.2, 10, 0.5),
            boundingBox: { minX: -1, minY: 8, minZ: -1, maxX: 2, maxY: 12, maxZ: 3 },
        };
        const rocket = {
            id: 8, name: 'firework_rocket', living: false, attachedRocket: true, position: new Vec3(0.5, 10, 0.5),
            boundingBox: { minX: 0.375, minY: 10, minZ: 0.375, maxX: 0.625, maxY: 10.25, maxZ: 0.625 },
        };
        const glide = nearbyEntities => {
            const ctx = playerState(0.5, 10, 0.5, {
                fallFlying: true, pose: adapter.Pose.FALL_FLYING, validElytraEquipped: true, vel: new Vec3(0, 0, 0.3),
                fireworkRocketDuration: 1, fireworkRockets: 1, nearbyEntities,
            });
            physics.simulate(ctx, world);
            return ctx.state.vel.clone();
        };
        const pushFirst = glide([cow, rocket]);
        const boostFirst = glide([rocket, cow]);
        assert.deepEqual(boostFirst, glide([cow]));
        assert.notEqual(pushFirst.x, boostFirst.x);
    });

    it('boosts a 1.11 glider by a rocket before it along the yaw of its last tick (yHeadRot)', () => {
        // D-222: 1.11.2 LivingEntity.getLookAngle is getViewVector(1.0F) of yHeadRot (LivingEntity.java:1917-1925), which the
        // last aiStep set to that tick's yaw (Player.java:468-472); 1.12.2 Entity.getLookAngle reads the yaw (:1729-1731)
        const glide = (version, headYawDegrees, turns) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 30, 0.5, {
                fallFlying: true, validElytraEquipped: true, vel: new Vec3(0, 0, 0.3), yaw: Math.PI, yawDegrees: 0,
                fireworkRocketDuration: 2, fireworkRockets: 1, fireworkRocketsBeforePlayer: 1, headYawDegrees,
            });
            if (turns) {
                // the last tick at yaw 0, this one at 90: the engine remembers the yaw of the last
                physics.simulate(ctx, world);
                ctx.state.vel = new Vec3(0, 0, 0.3);
                ctx.state.pos = new Vec3(0.5, 30, 0.5);
            }
            ctx.state.yawDegrees = 90;
            ctx.state.yaw = Math.PI / 2;
            physics.simulate(ctx, world);
            return ctx.state.vel.clone();
        };
        // the head yaw of the last tick, 0, given or remembered, is not this tick's 90
        assert.notDeepEqual(glide('1.11.2', 0), glide('1.11.2', 90));
        assert.deepEqual(glide('1.11.2', undefined, true).x, glide('1.11.2', 0).x);
        // 1.12.2 boosts along this tick's yaw whatever the head yaw
        assert.deepEqual(glide('1.12.2', 0), glide('1.12.2', 90));
    });

    it('keeps an attached rocket through a pause in the glide, for the glide after it', () => {
        // FireworkRocketEntity.tick: not gliding, the rocket follows and boosts nothing, and only its lifetime ends it
        // (26.3 FireworkRocketEntity.java:119-133, 172-178)
        const { registry, world } = worldOf('1.21.1', () => {});
        const physics = new adapter.NativePhysics(registry);
        const ctx = playerState(0.5, 10, 0.5, { fallFlying: false, fireworkRocketDuration: 5, fireworkRockets: 1 });
        physics.simulate(ctx, world);
        assert.equal(ctx.state.fireworkRocketDuration, 4);
        assert.equal(ctx.state.vel.z, 0);
    });

    it('starts a sprint with the terms of its release: the glide, the crouch and shallow water', () => {
        // 1.13.2 - 1.19.3: no glide term, the sprint key alone is held back in shallow water and the double tap arms
        // there (1.13.2 EntityPlayerSP.java:699-708, 1.19.3 ClientPlayerEntity.java:689-712); 1.19.4 - 1.21.4 a strict
        // !isFallFlying() (1.19.4 :1020-1027), 1.21.4 the crouch (1.21.4 :1054-1062); 1.21.5 spares a player under
        // water from both and moves the shallow water into canStartSprinting (1.21.5 LocalPlayer.java:1065-1074);
        // isWalking takes any forward under water from 1.14 (1.14.4 LocalPlayer.java:925-928)
        const start = (version, fields, heading = { forward: 1, strafe: 0 }) => {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, ...fields });
            return [physics.canStartSprinting(state, heading), physics, state];
        };
        const gliding = { onGround: false, fallFlying: true };
        assert.equal(start('1.19.3', gliding)[0], true);
        assert.equal(start('1.19.4', gliding)[0], false);
        assert.equal(start('1.21.4', { ...gliding, isInWater: true, isUnderWater: true })[0], false);
        assert.equal(start('1.21.5', { ...gliding, isInWater: true, isUnderWater: true })[0], true);
        assert.equal(start('1.21.3', { crouching: true })[0], true);
        assert.equal(start('1.21.4', { crouching: true })[0], false);
        // shouldSlowDown is crouching or crawling: the swimming pose (or a gliding one, not gliding) out of water
        // (1.21.4 ClientPlayerEntity.java:601-602, 1062; Entity.java:2400-2401; LivingEntity.java:3301-3302)
        const crawling = { pose: adapter.Pose.SWIMMING };
        assert.equal(start('1.21.3', crawling)[0], true);
        assert.equal(start('1.21.4', crawling)[0], false);
        assert.equal(start('1.21.4', { pose: adapter.Pose.FALL_FLYING })[0], false);
        const shallow = { isInWater: true };
        assert.equal(start('1.16.5', shallow, { forward: 0.5, strafe: 0 })[0], false);
        assert.equal(start('1.16.5', { ...shallow, isUnderWater: true }, { forward: 0.5, strafe: 0 })[0], true);
        assert.equal(start('1.21.5', shallow)[0], false);
        for (const version of ['1.13.2', '1.16.5', '1.21.4']) {
            const [, physics, state] = start(version, shallow);
            physics.inputsToSprint(state, { forward: 1, strafe: 0 });
            assert.equal(state.sprintTriggerTime, 7, version);
            const [, keyed, held] = start(version, { ...shallow, prevForwardImpulse: 1, control: controls({ forward: true, sprint: true }) });
            keyed.inputsToSprint(held, { forward: 1, strafe: 0 });
            assert.equal(held.sprinting, false, version);
        }
    });

    it('stops a sprint on 1.21.4 when the player glides', () => {
        // LocalPlayer.shouldStopSprinting: isFallFlying() || hasBlindness() || ... (1.21.4 ClientPlayerEntity.java:815-820)
        const stops = version => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, { fallFlying: true, sprinting: true });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: 9, minZ: -1, maxX: 2, maxY: 12, maxZ: 2 }, 2);
            physics.inputsToCrouch(state, { forward: 1, strafe: 0 }, sync);
            return !state.sprinting;
        };
        assert.equal(stops('1.21.3'), false);
        assert.equal(stops('1.21.4'), true);
    });

    it('stops a swim for hunger only off the ground and for blindness from 1.21.5', () => {
        // before 1.21.5: !onGround && !sneak && (!forward || !food) || !isInWater (1.14.4 LocalPlayer.java:632-640,
        // 1.21.4 ClientPlayerEntity.java:713-718); 1.21.5 shouldStopSwimSprinting (LocalPlayer.java:846-852)
        const stops = (version, fields) => {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { isInWater: true, isUnderWater: true, swimming: true, sprinting: true, ...fields });
            return physics.shouldStopSwimSprinting(state, { forward: 1, strafe: 0 });
        };
        for (const version of ['1.14.4', '1.21.4']) {
            assert.equal(stops(version, { onGround: true, food: 3 }), false, version);
            assert.equal(stops(version, { onGround: false, food: 3 }), true, version);
            assert.equal(stops(version, { blindness: 1 }), false, version);
        }
        assert.equal(stops('1.21.5', { onGround: true, food: 3 }), true);
        assert.equal(stops('1.21.5', { blindness: 1 }), true);
    });

    it('keeps a flying player sprinting in shallow water from 1.21.9', () => {
        // isSprintingPossible(getAbilities().flying): (allowedInShallowWater || !isInShallowWater()) (1.21.9
        // LocalPlayer.java:841-845, 1055-1066; 1.21.8 still stops on isInWater() && !isUnderWater())
        const sprints = version => {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const fields = { isInWater: true, flying: true, mayFly: true, gameMode: 'creative' };
            const { state } = playerState(0.5, 1, 0.5, fields);
            const { state: running } = playerState(0.5, 1, 0.5, { ...fields, sprinting: true });
            const heading = { forward: 1, strafe: 0 };
            return [physics.canStartSprinting(state, heading), physics.shouldStopRunSprinting(running, heading)];
        };
        assert.deepEqual(sprints('1.21.8'), [false, true]);
        assert.deepEqual(sprints('1.21.9'), [true, false]);
    });

    it('keeps the glide of a flying player from 1.21.2, where only the server clears it', () => {
        // Player.travel's flight: setSharedFlag(7, false) to 1.21.1 (1.21.1 Player.java:1421-1427); from 1.21.2 only
        // the server-side LivingEntity.updateFallFlying (1.21.2 Player.java:1399-1421, LivingEntity.java:2864-2869);
        // minecraft-data reads 1.21.2 as 1.21, so 1.21.3 stands for it
        const glides = version => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 10, 0.5, {
                fallFlying: true, flying: true, mayFly: true, gameMode: 'creative', validElytraEquipped: true,
            });
            physics.simulate(ctx, world);
            return ctx.state.fallFlying;
        };
        assert.equal(glides('1.21.1'), false);
        assert.equal(glides('1.21.3'), true);
    });

    it('does not start a glide in lava from 26.3', () => {
        // Player.tryToStartFallFlying: !isInLiquid() in 26.3 (Player.java:1351-1352), !isInWater() in 26.2 (:1366)
        const starts = release => {
            const { registry, world } = worldOf(release, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, { isInLava: true, validElytraEquipped: true });
            state.control = controls({ jump: true });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: 9, minZ: -1, maxX: 2, maxY: 12, maxZ: 2 }, 2);
            physics.inputsToFly(state, sync);
            return state.fallFlying;
        };
        assert.equal(starts('26.2'), true);
        assert.equal(starts('26.3'), false);
    });

    describe('the start of a glide', () => {
        // LocalPlayer.aiStep: a fresh press with no flight toggled and no climbable, then tryToStartFallFlying (26.3
        // LocalPlayer.java:833, Player.java:1351-1352, LivingEntity.java:3142-3143; 1.15 ClientPlayerEntity.java:711-713 and
        // PlayerEntity.java:1623-1631; 1.16.5 PlayerEntity.java:1595-1605; 1.21.1 Player.java:1481-1491)
        const start = (version, fields = {}, { held = false, keys = { jump: true } } = {}) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, { validElytraEquipped: true, ...(typeof fields === 'function' ? fields(registry) : fields) });
            state.control = controls(keys);
            state.prevControl = controls({ jump: held });
            physics.inputsToFly(state, physics.syncOf(world));
            return state;
        };
        const levitating = registry => {
            const { id } = registry.effectsByName.Levitation;
            return { effects: { [id]: { id, amplifier: 0 } } };
        };
        const versions = ['1.15.2', '1.16.5', '1.20.4', '1.21.1', '1.21.3', '1.21.4', '26.2', '26.3'];

        it('takes a fresh press, not a held jump', () => {
            for (const version of versions) {
                assert.equal(start(version).fallFlying, true, version);
                assert.equal(start(version, {}, { held: true }).fallFlying, false, version);
                assert.equal(start(version, {}, { keys: {} }).fallFlying, false, version);
            }
        });

        it('does not start on the client before 1.15, which only asks the server', () => {
            // 1.14 ClientPlayerEntity.java:697-702 sends START_FALL_FLYING and leaves the flag to the server
            assert.equal(start('1.14.4', { vel: new Vec3(0, -0.5, 0) }).fallFlying, false);
            assert.equal(start('1.15').fallFlying, true);
        });

        it('wants the player off the ground, out of the water and out of a vehicle', () => {
            for (const version of versions) {
                for (const held of [false, true]) {
                    assert.equal(start(version, { onGround: true }, { held }).fallFlying, false, version);
                    assert.equal(start(version, { isInWater: true }, { held }).fallFlying, false, version);
                    assert.equal(start(version, { passenger: true }, { held }).fallFlying, false, version);
                    assert.equal(start(version, { onClimbable: true }, { held }).fallFlying, false, version);
                }
            }
        });

        it('starts under levitation only before 1.16', () => {
            // no levitation term in 1.15 PlayerEntity.java:1624; 1.16.5 :1596, 1.21.1 Player.java:1482, 26.3 LivingEntity.java:3143
            assert.equal(start('1.15', levitating).fallFlying, true);
            assert.equal(start('1.15.2', levitating).fallFlying, true);
            for (const version of ['1.16', '1.16.5', '1.21.1', '1.21.3', '26.2', '26.3']) {
                assert.equal(start(version, levitating).fallFlying, false, version);
            }
        });

        it('starts in lava only before 26.3', () => {
            // 26.2 Player.java:1366 !isInWater(), 26.3 :1352 !isInLiquid()
            for (const held of [false, true]) {
                assert.equal(start('26.2', { isInLava: true }, { held }).fallFlying, !held);
                assert.equal(start('26.3', { isInLava: true }, { held }).fallFlying, false);
            }
        });

        it('asks the lava of this tick, not the flag the last one left (26.3)', () => {
            // D-229: aiStep's tryToStartFallFlying reads isInLiquid() after baseTick refreshed the fluid heights of this
            // tick (26.3 LocalPlayer.java:833, Player.java:1351-1352, Entity.java baseTick updateInWaterStateAndDoFluidPushing),
            // so a player that came into lava starts no glide however the last tick left isInLava, and one that left it does
            const glides = (release, lava, stale) => {
                const { registry, world } = worldOf(release, ({ Block, set }) => {
                    if (lava) set(0, 10, 0, Block.fromProperties('lava', { level: 0 }, 0).stateId);
                });
                const physics = new adapter.NativePhysics(registry);
                const ctx = playerState(0.5, 10.2, 0.5, { validElytraEquipped: true, isInLava: stale, vel: new Vec3(0, -0.1, 0) });
                ctx.state.control = controls({ jump: true });
                physics.simulate(ctx, world);
                return ctx.state.fallFlying;
            };
            assert.equal(glides('26.3', true, false), false);
            assert.equal(glides('26.3', false, true), true);
            // 26.2 asks the water only (Player.java:1366)
            assert.equal(glides('26.2', true, false), true);
        });

        it('leaves a press that toggled creative flight, and a flying player, alone', () => {
            // justToggledCreativeFlight (26.3 LocalPlayer.java:820-833), Player.canGlide's !abilities.flying (1.21.3
            // PlayerEntity.java:1519-1520) and the 1.15 test (ClientPlayerEntity.java:711)
            for (const version of versions) {
                const creative = { mayFly: true, gameMode: 'creative' };
                // the first tap only arms the double tap, and a glide may start on it
                const first = start(version, creative);
                assert.deepEqual([first.flyJumpTriggerTime, first.fallFlying], [7, true], version);
                const second = start(version, { ...creative, flyJumpTriggerTime: 5 });
                assert.deepEqual([second.flying, second.fallFlying], [true, false], version);
                const leaving = start(version, { ...creative, flying: true, flyJumpTriggerTime: 5 });
                assert.deepEqual([leaving.flying, leaving.fallFlying], [false, false], version);
                assert.equal(start(version, { ...creative, flying: true }).fallFlying, false, version);
                // a spectator's flight toggles every tick
                assert.equal(start(version, { ...creative, gameMode: 'spectator' }).fallFlying, false, version);
            }
        });

        it('wants a usable glider, and starts nothing already gliding', () => {
            for (const version of versions) {
                assert.equal(start(version, { validElytraEquipped: false }).fallFlying, false, version);
                const gliding = start(version, { fallFlying: true }, { held: true });
                assert.equal(gliding.fallFlying, true, version);
            }
        });

        it('does not start again on a held jump after the server refuses the glide', () => {
            // the client's flag goes on the press; the server clears it (LivingEntity.updateFallFlying), and the held key is
            // no new press, a released and pressed one is
            for (const version of ['1.16.5', '1.21.4', '26.3']) {
                const { registry, world } = worldOf(version, () => {});
                const physics = new adapter.NativePhysics(registry);
                const ctx = playerState(0.5, 20, 0.5, { validElytraEquipped: true, control: controls({ jump: true }) });
                physics.simulate(ctx, world);
                assert.equal(ctx.state.fallFlying, true, version);
                ctx.state.fallFlying = false;
                physics.simulate(ctx, world);
                assert.equal(ctx.state.fallFlying, false, version);
                ctx.state.control = controls();
                physics.simulate(ctx, world);
                ctx.state.control = controls({ jump: true });
                physics.simulate(ctx, world);
                assert.equal(ctx.state.fallFlying, true, version);
            }
        });

        it('leaves activation to the adapter: the native contract takes the flag as authorized', () => {
            // native code never starts a glide and holds no equipment: simulateVersionedTick and a NativePlayer read
            // fallFlying as the caller's decision, and glide on it with nothing else to say the player may
            const binding = adapter.LoadNativePhysics();
            for (const version of ['1.16.5', '1.21.4', '26.3']) {
                const { registry, world } = worldOf(version, () => {});
                const physics = new adapter.NativePhysics(registry);
                const native = physics.syncOf(world).world;
                const object = fallFlying => binding.simulateVersionedTick(version, native, {
                    position: { x: 0.5, y: 20, z: 0.5 }, velocity: { x: 0, y: -0.5, z: 0.5 }, halfWidth: 0.3, height: 1.8,
                    onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true,
                    jumpRequested: true, fallFlying,
                }, {});
                const plain = object(false);
                const gliding = object(true);
                assert.equal(plain.fallFlying, false, version);
                assert.equal(gliding.fallFlying, true, version);
                assert.notDeepEqual(plain.velocity, gliding.velocity, version);
                // a kept player, without the adapter's decision
                const player = new binding.NativePlayer(version, {
                    position: { x: 0.5, y: 20, z: 0.5 }, velocity: { x: 0, y: -0.5, z: 0.5 }, halfWidth: 0.3, height: 1.8,
                    onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true,
                }, {});
                const { NativePlayerInput: In, NativePlayerInputFlag: InFlag, NativePlayerOutput: Out, NativePlayerOutputFlag: OutFlag } = adapter;
                const tick = flags => {
                    const input = new Float64Array(In.LENGTH);
                    input.set([0.5, 20, 0.5, 0, -0.5, 0.5]);
                    input[In.HALF_WIDTH] = 0.3;
                    input[In.HEIGHT] = 1.8;
                    input[In.FLAGS] = flags;
                    return player.tick(native, input, new Float64Array(0));
                };
                assert.equal(tick(InFlag.JUMP_REQUESTED)[Out.FLAGS] & OutFlag.FALL_FLYING, 0, version);
                assert.notEqual(tick(InFlag.JUMP_REQUESTED | InFlag.FALL_FLYING)[Out.FLAGS] & OutFlag.FALL_FLYING, 0, version);
            }
        });
    });

    describe('the glider a glide takes', () => {
        const slots = { head: 5, torso: 6, legs: 7, feet: 8, 'off-hand': 45 };
        const elytra = (fields = {}) => ({ name: 'elytra', durabilityUsed: 0, maxDurability: 432, ...fields });
        const botWearing = (worn, heldItem) => {
            const inventory = { slots: [] };
            for (const [slot, item] of Object.entries(worn)) inventory.slots[slots[slot]] = item;
            return { inventory, heldItem, getEquipmentDestSlot: destination => slots[destination] };
        };
        const components = (...list) => new Map(list.map(([type, data]) => [type, { type, data }]));
        const starts = (version, worn, heldItem) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, { bot: botWearing(worn, heldItem), control: controls({ jump: true }) });
            physics.inputsToFly(state, physics.syncOf(world));
            assert.equal(state.validElytraEquipped, state.fallFlying, version);
            return state.fallFlying;
        };
        const versions = ['1.16.5', '1.20.4', '1.21.1', '1.21.3', '26.2', '26.3'];

        it('goes by the last point of durability on both sides', () => {
            // isUsable: damage < max - 1 (1.9.4 ElytraItem.java:30-32, client1.21.1 :19-21); nextDamageWillBreak: damage >= max - 1
            // (client1.21.2 ItemStack.java:436-438, 26.3 :412-414): 430 of 432 flies, 431 does not
            for (const version of versions) {
                assert.equal(starts(version, { torso: elytra() }), true, version);
                assert.equal(starts(version, { torso: elytra({ durabilityUsed: 430 }) }), true, version);
                assert.equal(starts(version, { torso: elytra({ durabilityUsed: 431 }) }), false, version);
                assert.equal(starts(version, { torso: elytra({ durabilityUsed: 432 }) }), false, version);
            }
        });

        it('wants the glider worn', () => {
            for (const version of versions) {
                assert.equal(starts(version, {}), false, version);
                assert.equal(starts(version, { torso: { name: 'iron_chestplate', durabilityUsed: 0, maxDurability: 240 } }), false, version);
                // an elytra in the hand or another slot is not equipped for its chest slot
                assert.equal(starts(version, { legs: elytra(), 'off-hand': elytra() }, elytra()), false, version);
            }
        });

        it('reads the components from 1.21.2 and the chest elytra before', () => {
            // client1.21.1 LivingEntity.java:2762-2763 vs client1.21.2 :3681-3687
            const leather = (...list) => ({ name: 'leather_chestplate', durabilityUsed: 0, maxDurability: 80, componentMap: components(...list) });
            assert.equal(starts('1.21.1', { torso: leather(['glider']) }), false);
            assert.equal(starts('1.21.3', { torso: leather(['glider']) }), true);
            assert.equal(starts('26.3', { torso: leather(['glider']) }), true);
            // a pair of boots made a glider goes in the slot of its equippable, an elytra made a helmet in the head
            assert.equal(starts('26.3', { feet: { name: 'iron_boots', componentMap: components(['glider']) } }), true);
            assert.equal(starts('26.3', { head: elytra({ componentMap: components(['equippable', { slot: 'head' }]) }) }), true);
            assert.equal(starts('26.3', { torso: elytra({ componentMap: components(['equippable', { slot: 'head' }]) }) }), false);
            assert.equal(starts('26.3', { torso: { name: 'stick', componentMap: components(['glider'], ['equippable', { slot: 'chest' }]) } }), true);
            assert.equal(starts('26.3', { torso: { name: 'stick', componentMap: components(['glider']) } }), false);
            // the stack can take them away
            assert.equal(starts('26.3', { torso: elytra({ removedComponents: [{ type: 'glider' }] }) }), false);
            assert.equal(starts('26.3', { torso: elytra({ removedComponents: [{ type: 'equippable' }] }) }), false);
            assert.equal(starts('1.21.1', { torso: elytra({ removedComponents: [{ type: 'glider' }] }) }), true);
        });

        it('lets an unbreakable glider fly on its last point only from 1.21.2', () => {
            // nextDamageWillBreak wants isDamageableItem (client1.21.2 ItemStack.java:412-414); ElytraItem.isFlyEnabled does not
            const last = elytra({ durabilityUsed: 431, componentMap: components(['unbreakable', {}]) });
            assert.equal(starts('1.21.1', { torso: last }), false);
            assert.equal(starts('1.21.3', { torso: last }), true);
            assert.equal(starts('26.3', { torso: last }), true);
            const stretched = elytra({ durabilityUsed: 431, componentMap: components(['max_damage', 433]) });
            for (const version of ['1.21.1', '1.21.3', '26.3']) assert.equal(starts(version, { torso: stretched }), true, version);
        });

        it('keeps the caller\'s snapshot without an inventory', () => {
            // (a bot's derivation does not apply to a state with none: that is the explicit validity update)
            const { registry, world } = worldOf('26.3', () => {});
            const physics = new adapter.NativePhysics(registry);
            for (const validElytraEquipped of [true, false]) {
                const { state } = playerState(0.5, 10, 0.5, { validElytraEquipped, control: controls({ jump: true }) });
                physics.inputsToFly(state, physics.syncOf(world));
                assert.equal(state.fallFlying, validElytraEquipped);
            }
        });

        it('glides on after the glider goes until the server says otherwise, and takes no durability', () => {
            // updateFallFlying's check and damage are the server's (1.9.4 LivingEntity.java:1741-1756, client1.21.1 :2759-2777,
            // 26.3 :3119-3127): every 20 ticks there, and on no client tick, so an unequipped or spent glider changes
            // nothing here until the flags arrive. The move is the glide's up to that tick, and the fall's from it.
            for (const version of ['1.16.5', '1.20.4', '1.21.1', '1.21.3', '26.3']) {
                const { registry, world } = worldOf(version, () => {});
                const run = change => {
                    const physics = new adapter.NativePhysics(registry);
                    const item = elytra({ durabilityUsed: 100 });
                    const bot = botWearing({ torso: item });
                    const ctx = playerState(0.5, 100, 0.5, {
                        bot, fallFlying: true, pose: adapter.Pose.FALL_FLYING, validElytraEquipped: true, vel: new Vec3(0, -0.2, 0.6),
                    });
                    const path = [];
                    for (let tick = 0; tick < 45; tick += 1) {
                        change?.(tick, bot, ctx.state);
                        physics.simulate(ctx, world);
                        path.push([ctx.state.pos.y, ctx.state.vel.y, ctx.state.vel.z, ctx.state.fallFlying]);
                    }
                    assert.equal(item.durabilityUsed, 100, version);
                    return { path, state: ctx.state };
                };
                const kept = run();
                assert.equal(kept.path[44][3], true, version);
                // unequipped on tick 5, nearly broken on tick 7: the same path, and no restart is possible meanwhile
                const lost = run((tick, bot) => {
                    if (tick === 5) bot.inventory.slots[6] = null;
                    if (tick === 7) bot.inventory.slots[6] = elytra({ durabilityUsed: 431 });
                });
                assert.deepEqual(lost.path, kept.path, version);
                assert.equal(lost.state.validElytraEquipped, false, version);
                // the server's flags clear it on tick 10: the path holds up to there, and goes on as a fall
                const ended = run((tick, bot, state) => {
                    if (tick === 10) state.fallFlying = false;
                });
                assert.deepEqual(ended.path.slice(0, 10), kept.path.slice(0, 10), version);
                assert.equal(ended.path[10][3], false, version);
                assert.notEqual(ended.path[10][1], kept.path[10][1], version);
            }
        });
    });

    it('starts a glide on a ladder in 1.15 and 1.15.1, where aiStep has no climbable test', () => {
        // 1.15 ClientPlayerEntity.java:711-713 (and 1.15.1) test no onLadder(); 1.15.2 adds !onLadder(), 1.16 :742
        const starts = version => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, { onClimbable: true, validElytraEquipped: true });
            state.control = controls({ jump: true });
            physics.inputsToFly(state, physics.syncOf(world));
            return state.fallFlying;
        };
        assert.equal(starts('1.15'), true);
        assert.equal(starts('1.15.1'), true);
        assert.equal(starts('1.15.2'), false);
        assert.equal(starts('1.16.5'), false);
    });

    it('counts the jump of an auto-jump in wasJumping and toggles no flight with it', () => {
        // LocalPlayer.aiStep: wasJumping is the input's jump before input.tick, which makeJump pressed on the auto-jump's
        // tick; the flight toggle wants !wasJumping && jumping && !autoJump, the glide jumping && !wasJumping (1.10.2
        // ClientPlayerEntity.java:658-716, 1.15 :628-711, 1.21.11 LocalPlayer.java:738-753, 799, 815)
        for (const version of ['1.12.2', '1.16.5', '1.21.4']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, autoJumpTime: 1, mayFly: true, gameMode: 'creative' });
            physics.simulate(ctx, world);
            assert.equal(ctx.state.prevControl.jump, true, version);
            // the key pressed on the tick after the auto-jump is no fresh press: no second tap of a double tap
            const { state } = playerState(0.5, 10, 0.5, { mayFly: true, gameMode: 'creative', flyJumpTriggerTime: 5 });
            state.prevControl = controls({ jump: true });
            state.control = controls({ jump: true });
            physics.inputsToFly(state, physics.syncOf(world));
            assert.equal(state.flying, false, version);
            // the auto-jump's own press toggles nothing either
            const { state: auto } = playerState(0.5, 10, 0.5, { mayFly: true, gameMode: 'creative', flyJumpTriggerTime: 5, autoJumping: true });
            physics.inputsToFly(auto, physics.syncOf(world));
            assert.equal(auto.flying, false, version);
            assert.equal(auto.flyJumpTriggerTime, 5, version);
        }
        // an auto-jump's press in the air starts a glide like the key's
        const { registry, world } = worldOf('1.16.5', () => {});
        const physics = new adapter.NativePhysics(registry);
        const { state } = playerState(0.5, 10, 0.5, { autoJumping: true, validElytraEquipped: true });
        physics.inputsToFly(state, physics.syncOf(world));
        assert.equal(state.fallFlying, true);
    });

    it('ends a sprint after 600 ticks in 1.8', () => {
        // EntityPlayerSP.setSprinting: sprintingTicksLeft = sprinting ? 600 : 0, counted down at the start of
        // onLivingUpdate, setSprinting(false) at 0 (1.8.9 ClientPlayerEntity.java:349-352, 487-492); gone in 1.9.4
        const sprints = (version, ticks) => {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, prevForwardImpulse: 1 });
            physics.setSprinting(state, true);
            for (let i = 0; i < ticks; i += 1) physics.inputsToSprint(state, { forward: 1, strafe: 0 });
            return [state.sprinting, state.sprintingTicksLeft];
        };
        assert.deepEqual(sprints('1.8.8', 0), [true, 600]);
        assert.deepEqual(sprints('1.8.8', 599), [true, 1]);
        assert.deepEqual(sprints('1.8.8', 600), [false, 0]);
        assert.equal(sprints('1.9.4', 600)[0], true);
    });

    it('starts a sprint with the sprint key after a sneak before 1.14; only the double tap tests the sneak', () => {
        // the sprint key's start has no sneak term, the double tap's !wasSneaking (1.8.9 ClientPlayerEntity.java:551-573,
        // 1.12.2 :715-736, 1.13.2 :722-743)
        for (const version of ['1.8.8', '1.12.2', '1.13.2']) {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const start = sprint => {
                const { state } = playerState(0.5, 1, 0.5, { onGround: true, crouching: true });
                state.prevControl = controls({ sneak: true });
                state.control = controls({ forward: true, sprint });
                physics.inputsToSprint(state, { forward: 1, strafe: 0 });
                return [state.sprinting, state.sprintTriggerTime];
            };
            assert.deepEqual(start(true), [true, 0], version);
            assert.deepEqual(start(false), [false, 0], version);
        }
    });

    it('takes the riptide spin from the synced living entity flags for the pose', () => {
        // Player.updatePlayerPose: SPIN_ATTACK while isAutoSpinAttack, bit 4 of the living entity flags (1.14.4
        // Player.java:333-361); from 1.21.5 sleeping and swimming come before gliding (1.21.5 Player.java:464-476);
        // 1.13's updateSize: 0.6F (1.13.2 PlayerEntity.java:419-437)
        const spinning = index => {
            const metadata = [];
            metadata[index] = 4;
            return { entity: { metadata } };
        };
        for (const [version, index] of [['1.14.4', 7], ['1.16.5', 7], ['1.17', 8], ['1.21.4', 8]]) {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 10, 0.5, { bot: spinning(index) });
            physics.simulate(ctx, world);
            assert.equal(ctx.state.pose, adapter.Pose.SPIN_ATTACK, version);
            const other = playerState(0.5, 10, 0.5, { bot: spinning(index + 1) });
            physics.simulate(other, world);
            assert.equal(other.state.pose, adapter.Pose.STANDING, version);
        }
        const { registry, world } = worldOf('1.13.2', () => {});
        const legacy = playerState(0.5, 10, 0.5, { bot: spinning(6) });
        new adapter.NativePhysics(registry).simulate(legacy, world);
        assert.equal(legacy.state.keptBoundingBox.height, f32(0.6));
        const wanted = (version, fields) => {
            const { registry: r, world: w } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(r);
            const { state } = playerState(0.5, 10, 0.5, fields);
            const sync = physics.syncOf(w);
            sync.sync(w, { minX: -1, minY: 9, minZ: -1, maxX: 2, maxY: 12, maxZ: 2 }, 2);
            physics.updatePoses(state, sync);
            return state.pose;
        };
        assert.equal(wanted('1.21.4', { swimming: true, fallFlying: true }), adapter.Pose.FALL_FLYING);
        assert.equal(wanted('1.21.5', { swimming: true, fallFlying: true }), adapter.Pose.SWIMMING);
        assert.equal(wanted('1.21.5', { fallFlying: true, bot: spinning(8) }), adapter.Pose.FALL_FLYING);
    });

    it('bounces a riptide spin off the first living entity it meets', () => {
        // LivingEntity.checkAutoSpinAttack: a LivingEntity in the box swept by the move ends the spin and scales the
        // velocity by -0.2; another entity there keeps the spin (1.13.2 LivingEntity.java:2018-2035,
        // 1.21.11 :3090-3104)
        const { registry, world } = worldOf('1.21.1', () => {});
        const physics = new adapter.NativePhysics(registry);
        const spin = (name, living) => {
            // met in the move; its own tick leaves it far away, where it pushes no one
            const entity = {
                id: 5, name, living, position: new Vec3(0.5, 10, 1),
                boundingBox: { minX: 0.2, minY: 10, minZ: 0.9, maxX: 0.8, maxY: 11.95, maxZ: 1.5 },
                afterTick: { position: new Vec3(20.5, 10, 1), boundingBox: { minX: 20.2, minY: 10, minZ: 0.9, maxX: 20.8, maxY: 11.95, maxZ: 1.5 } },
            };
            const ctx = playerState(0.5, 10, 0.5, { vel: new Vec3(0, 0, 0.5), autoSpinAttackTicks: 20, nearbyEntities: [entity] });
            physics.simulate(ctx, world);
            return ctx.state;
        };
        const [hit, missed] = [spin('zombie', true), spin('item', false)];
        assert.equal(hit.autoSpinAttackTicks, 0);
        assert.equal(missed.autoSpinAttackTicks, 19);
        assert.equal(hit.vel.z, missed.vel.z * -0.2);
        assert.equal(hit.vel.y, missed.vel.y * -0.2);
        // the armor stand, which mineflayer-physics-utils counts as not living (it pushes no one), is a LivingEntity
        // (1.13.2 LivingEntity.java:2022, entity/decoration/ArmorStandEntity.java:40)
        const stand = spin('armor_stand', false);
        assert.equal(stand.autoSpinAttackTicks, 0);
        assert.equal(stand.vel.z, missed.vel.z * -0.2);
    });

    it('sizes an armor stand by its synced client flags for the riptide spin: a marker 0 x 0, a small one half', () => {
        // ArmorStand: MARKER_DIMENSIONS 0 x 0 (flag 16), the small one the type's 0.5F x 1.975F scaled by 0.5F (flag 1),
        // the client flags its first synced data, 15 from 1.17 (1.13.2 ArmorStandEntity.java:121-127, 624-630, 669, 694;
        // 1.16.5 :850-860; 1.21.11 ArmorStand.java:59-71)
        const { registry, world } = worldOf('1.21.1', () => {});
        const physics = new adapter.NativePhysics(registry);
        const spin = (flags, y) => {
            const position = new Vec3(0.5, y, 1);
            const boundingBox = { minX: 0.25, minY: y, minZ: 0.75, maxX: 0.75, maxY: y + f32(1.975), maxZ: 1.25 };
            const far = { position: new Vec3(20.5, y, 1), boundingBox: { ...boundingBox, minX: 20.25, maxX: 20.75 } };
            const metadata = [];
            metadata[15] = flags;
            const ctx = playerState(0.5, 10, 0.5, {
                vel: new Vec3(0, 0, 0.5), autoSpinAttackTicks: 20,
                nearbyEntities: [{ id: 5, name: 'armor_stand', living: false, position, boundingBox, afterTick: far }],
                bot: { entities: { 5: { metadata } } },
            });
            physics.simulate(ctx, world);
            return ctx.state.autoSpinAttackTicks;
        };
        // at the player's feet a full stand and a small one are met, the marker's point is not
        assert.equal(spin(0, 10), 0);
        assert.equal(spin(1, 10), 0);
        assert.equal(spin(16, 10), 19);
        // reaching up to the feet from below, only the full stand's 1.975F is met
        assert.equal(spin(0, 8.1), 0);
        assert.equal(spin(1, 8.1), 19);
    });

    it('lets a passenger take the pose it wants without the room for it', () => {
        // Player.updatePlayerPose: a spectator or a passenger takes the desired pose, whose fit is tested only for the
        // others (1.14 PlayerEntity.java:418-425, 1.21.11 Player.java:355-362); without room to swim nothing changes
        for (const version of ['1.14.4', '1.21.1']) {
            const { registry, world } = worldOf(version, ({ registry: r, Block, set }) => {
                floor({ registry: r, set });
                const slab = Block.fromProperties('stone_slab', { type: 'top', waterlogged: false }, 0).stateId;
                for (let x = -1; x <= 1; x += 1) for (let z = -1; z <= 1; z += 1) set(x, 2, z, slab);
            });
            const physics = new adapter.NativePhysics(registry);
            const posed = fields => {
                const { state } = playerState(0.5, 1, 0.5, { onGround: true, ...fields });
                const sync = physics.syncOf(world);
                sync.sync(world, { minX: -1, minY: 0, minZ: -1, maxX: 2, maxY: 3, maxZ: 2 }, 2);
                physics.updatePoses(state, sync);
                return state.pose;
            };
            assert.equal(posed({}), adapter.Pose.SNEAKING, version);
            assert.equal(posed({ passenger: true }), adapter.Pose.STANDING, version);
            assert.equal(posed({ bot: { vehicle: { id: 7 } } }), adapter.Pose.STANDING, version);
            assert.equal(posed({ bot: { entity: { vehicle: { id: 7 } } } }), adapter.Pose.STANDING, version);
            assert.equal(posed({ passenger: false, bot: { vehicle: { id: 7 } } }), adapter.Pose.SNEAKING, version);
        }
    });

    it('takes matched release behaviour and rejects registries from a different release', () => {
        const before = require('prismarine-registry')('1.21.4');
        const registry = require('prismarine-registry')('1.21.5');
        assert.throws(() => new adapter.NativePhysics_1_21_5(before), /1\.21\.4.*1\.21\.5/);
        const physics = new adapter.NativePhysics_1_21_5(registry);
        assert.equal(physics.version, '1.21.5');
        assert.equal(physics.lt('1.21.5'), false);
        assert.equal(physics.gt('1.21.4'), true);
        assert.equal(new adapter.NativePhysics(before).lt('1.21.5'), true);
        // the world sync's block rules too
        assert.throws(() => new adapter.NativePhysics_26_3(before), /1\.21\.4.*26\.3/);
        const later = new adapter.NativePhysics_26_3(require('prismarine-registry')('26.3'));
        assert.equal(later.lt('26.2'), false);
        assert.equal(later.syncOf({}).mapper.minimumFlowLength, Math.fround(1.0e-5));
    });

    it('launches a riptide in the rain, and from 26.1 no passenger', () => {
        // TridentItem.releaseUsing (1.20.4 TridentItem.java:70, 26.3 :64)
        const released = (version, fields) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, { riptide: 3, itemUseTicks: 12, yaw: 0, ...fields });
            physics.simulate({ state }, world);
            return state.autoSpinAttackTicks > 0;
        };
        for (const version of ['1.20.4', '1.21.11']) {
            assert.equal(released(version, {}), false, version);
            assert.equal(released(version, { inRain: true }), true, version);
            assert.equal(released(version, { isInWater: true }), true, version);
            // a passenger keeps the launch until 26.1
            assert.equal(released(version, { inRain: true, passenger: true }), true, version);
            // the charge of ten ticks, and a trident with no riptide, are still needed
            assert.equal(released(version, { inRain: true, itemUseTicks: 9 }), false, version);
            assert.equal(released(version, { inRain: true, riptide: 0 }), false, version);
        }
        for (const version of ['26.1', '26.3']) {
            assert.equal(released(version, { inRain: true }), true, version);
            assert.equal(released(version, { inRain: true, passenger: true }), false, version);
            assert.equal(released(version, { isInWater: true, passenger: true }), false, version);
        }
        // no riptide before 1.13
        assert.equal(released('1.12.2', { inRain: true }), false);
        // the launch itself, not only the spin: at zero rotation it pushes the player forward
        const { registry, world } = worldOf('1.21.11', () => {});
        const physics = new adapter.NativePhysics(registry);
        const { state } = playerState(0.5, 10, 0.5, { riptide: 3, itemUseTicks: 12, yaw: 0, inRain: true });
        physics.simulate({ state }, world);
        assert.ok(Math.abs(state.pos.x - 0.5) + Math.abs(state.pos.z - 0.5) > 1, 'it moves');
    });

    it('applies the effects of a state without a synced movement speed, and only then', () => {
        const { registry, world } = worldOf('1.20.4', floor);
        const moved = fields => {
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, control: controls({ forward: true }), ...fields });
            if (fields.sprint) physics.setSprinting(state, true);
            physics.simulate({ state }, world);
            return Math.abs(state.pos.z - 0.5);
        };
        const plain = moved({});
        assert.ok(moved({ speed: 2 }) > plain);
        assert.ok(moved({ slowness: 2 }) < plain);
        // setSprinting's attribute is no synced one: the effect stays
        assert.ok(moved({ speed: 2, sprint: true }) > moved({ sprint: true }));
        // a synced attribute holds the modifiers already
        const attributes = { [new adapter.NativePhysics(registry).attributeKey('movementSpeed')]: { value: f32(0.1), modifiers: [] } };
        assert.equal(moved({ speed: 2, attributes }), moved({ attributes }));
    });

    it('moves by the source speed of a state without a movement speed, through sprints, clones and removed effects', () => {
        // getSpeed is the attribute: 0.1F, times one plus 0.2F * (amplifier + 1) for Speed, one minus 0.15F * (amplifier + 1)
        // for Slowness, one plus 0.3F sprinting, every one an ADD_MULTIPLIED_TOTAL (26.3 MobEffects.java:69-70,
        // AttributeInstance.java:151-169; 1.20.4 StatusEffects.java:10-21). The first move on stone from rest is the input
        // 0.98F (LivingEntity.applyInput, 26.3 :3106-3109) by getSpeed * (0.21600002F / 0.6F^3) (getFrictionInfluencedSpeed,
        // 26.3 LivingEntity.java:2640-2642).
        const friction = f32(0.6);
        const cube = f32(f32(friction * friction) * friction);
        const influence = f32(0.21600002 / cube);
        const expected = (speedLevels, slownessLevels, sprinting) => {
            let value = f32(0.1);
            if (speedLevels > 0) value *= 1 + f32(0.2) * speedLevels;
            if (slownessLevels > 0) value *= 1 - f32(0.15) * slownessLevels;
            if (sprinting) value *= 1 + f32(0.3);
            return f32(0.98) * f32(f32(value) * influence);
        };
        const close = (actual, wanted, message) => assert.ok(Math.abs(actual - wanted) < 1e-7, `${message}: ${actual} against ${wanted}`);
        for (const version of ['1.8.8', '1.20.4', '1.21.11', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const start = fields => playerState(0.5, 1, 0.5, { onGround: true, control: controls({ forward: true }), ...fields });
            const moved = (fields, sprint) => {
                const physics = new adapter.NativePhysics(registry);
                const ctx = start(fields);
                if (sprint) physics.setSprinting(ctx.state, true);
                physics.simulate(ctx, world);
                return { physics, state: ctx.state, moved: ctx.state.pos.z - 0.5, sent: physics.carries.get(ctx.state).player.state() };
            };
            for (const sprint of [false, true]) {
                const cases = [[{}, 0, 0], [{ speed: 2 }, 2, 0], [{ slowness: 2 }, 0, 2], [{ speed: 2, slowness: 2 }, 2, 2]];
                for (const [fields, speedLevels, slownessLevels] of cases) {
                    const { moved: z, sent } = moved(fields, sprint);
                    close(z, expected(speedLevels, slownessLevels, sprint), `${version} ${JSON.stringify(fields)} sprint ${sprint}`);
                    // the fallback's inputs: no effective value, the amplifiers, and setSprinting's attribute hides neither
                    assert.equal(sent.movementSpeedValue, undefined, version);
                    assert.equal(sent.speedEffectAmplifier, speedLevels ? speedLevels - 1 : undefined, version);
                    assert.equal(sent.slownessEffectAmplifier, slownessLevels ? slownessLevels - 1 : undefined, version);
                }
            }
            // the effects by registry, as a bot's are, say the same
            const { Speed: speedEffect, Slowness: slownessEffect } = registry.effectsByName;
            const effects = { [speedEffect.id]: { amplifier: 1 }, [slownessEffect.id]: { amplifier: 1 } };
            close(moved({ effects }).moved, expected(2, 2, false), `${version} effects`);

            // the sprint on and off again: the same fallback, not a synced attribute
            {
                const physics = new adapter.NativePhysics(registry);
                const ctx = start({ speed: 2 });
                physics.setSprinting(ctx.state, true);
                physics.setSprinting(ctx.state, false);
                physics.simulate(ctx, world);
                close(ctx.state.pos.z - 0.5, expected(2, 0, false), `${version} sprint off`);
            }

            // a clone takes the attributes and the effects as they are (PlayerState.clone, physics-utils playerState.js:377,
            // 386-390) and a new engine player, and goes on as the original does
            {
                const physics = new adapter.NativePhysics(registry);
                const ctx = start({ speed: 2, slowness: 2 });
                physics.setSprinting(ctx.state, true);
                physics.simulate(ctx, world);
                const clone = { state: { ...ctx.state, pos: ctx.state.pos.clone(), vel: ctx.state.vel.clone(), control: controls({ forward: true }), prevControl: controls({ forward: true }) } };
                for (let tick = 0; tick < 4; tick += 1) {
                    physics.simulate(ctx, world);
                    physics.simulate(clone, world);
                    assert.deepEqual([clone.state.pos.x, clone.state.pos.y, clone.state.pos.z], [ctx.state.pos.x, ctx.state.pos.y, ctx.state.pos.z], `${version} tick ${tick}`);
                    assert.deepEqual([clone.state.vel.x, clone.state.vel.y, clone.state.vel.z], [ctx.state.vel.x, ctx.state.vel.y, ctx.state.vel.z], `${version} tick ${tick}`);
                }
                assert.equal(physics.carries.get(clone.state).player.state().speedEffectAmplifier, 1, version);
                // a sprint a clone ends leaves the original's attribute alone (setSprinting replaces it)
                physics.setSprinting(clone.state, false);
                assert.equal(physics.hasSprintModifier(ctx.state), true, version);
            }

            // an effect that ends leaves the plain walk, on the engine player that had it
            {
                const physics = new adapter.NativePhysics(registry);
                const ctx = start({ speed: 2, slowness: 2 });
                physics.simulate(ctx, world);
                const { player } = physics.carries.get(ctx.state);
                ctx.state.speed = 0;
                ctx.state.slowness = 0;
                ctx.state.pos.set(0.5, 1, 0.5);
                ctx.state.vel.set(0, 0, 0);
                ctx.state.onGround = true;
                physics.simulate(ctx, world);
                assert.equal(physics.carries.get(ctx.state).player, player);
                assert.equal(player.state().speedEffectAmplifier, undefined, version);
                assert.equal(player.state().slownessEffectAmplifier, undefined, version);
                close(ctx.state.pos.z - 0.5, expected(0, 0, false), `${version} removed`);
            }

            // a synced attribute holds the modifiers already: the effects are not applied again, in either form it comes
            const key = new adapter.NativePhysics(registry).attributeKey('movementSpeed');
            const speedModifier = { uuid: 'speed', amount: f32(0.2) * 2, operation: 2 };
            const slownessModifier = { uuid: 'slowness', amount: f32(-0.15) * 2, operation: 2 };
            const synced = { [key]: { value: f32(0.1), modifiers: [speedModifier, slownessModifier] } };
            const once = moved({ speed: 2, slowness: 2, attributes: synced });
            close(once.moved, expected(2, 2, false), `${version} synced`);
            assert.equal(once.sent.speedEffectAmplifier, undefined, version);
            assert.equal(once.sent.slownessEffectAmplifier, undefined, version);
            const onceSprinting = moved({ speed: 2, slowness: 2, attributes: synced }, true);
            close(onceSprinting.moved, expected(2, 2, true), `${version} synced sprint`);
            // an attribute without a modifiers array is as synced as one with: its value is the base the server sent
            const compact = { [key]: { value: f32(0.1) } };
            const plain = moved({ speed: 2, slowness: 2, attributes: compact });
            close(plain.moved, expected(0, 0, false), `${version} compact`);
            assert.equal(plain.sent.speedEffectAmplifier, undefined, version);
            close(moved({ speed: 2, attributes: compact }, true).moved, expected(0, 0, true), `${version} compact sprint`);
        }
    });

    it('holds a sleeping or dead player to the motion it has, and agrees with the object API through the sleep', () => {
        // LivingEntity.aiStep clears jumping, xxa and zza of an immobile entity, a dead one or a sleeping player, and the
        // cooldown with them; the keys stay the input's (1.8.9 LivingEntity.java:1438-1443, PlayerEntity.java:441-442; 1.20.4
        // LivingEntity.java:1971-1972, 2552-2555; 26.3 :2254-2255, 3000-3003, 3035, Player.java:823-824)
        for (const version of ['1.8.8', '1.20.4', '1.21.11', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const binding = adapter.LoadNativePhysics();
            const keys = { forward: true, left: true, jump: true };
            const standing = (0 - 0.08) * f32(0.98);
            const ctx = playerState(0.5, 1, 0.5, {
                onGround: true, vel: new Vec3(0.1, standing, 0), jumpTicks: 4, control: controls(keys), prevControl: controls(keys), bot: { health: 20 },
            });
            const { state } = ctx;
            let object;
            for (const [tick, immobile] of ['', 'sleeping', 'sleeping', '', 'dead', 'dead', ''].entries()) {
                state.bot.isSleeping = immobile === 'sleeping';
                // (LivingEntity.stopSleeping stands the player up: the pose updatePoses kept is the caller's, as the sync's live)
                if (immobile !== 'sleeping') state.pose = adapter.Pose.STANDING;
                state.bot.health = immobile === 'dead' ? 0 : 20;
                const before = state.pos.clone();
                const held = state.vel.clone();
                physics.simulate(ctx, world);
                const packed = physics.carries.get(state).player.state();
                if (object) {
                    object = binding.simulateVersionedTick(version, physics.syncOf(world).world, {
                        ...object, sleeping: immobile === 'sleeping', dead: immobile === 'dead',
                    }, {});
                    assert.deepEqual([object.position, object.velocity, object.jumpTicks, object.onGround],
                        [packed.position, packed.velocity, packed.jumpTicks, packed.onGround], `${version} tick ${tick}`);
                    // the object API's state holds the keys for the tick after, as the adapter's control does
                    assert.equal(object.jumpRequested, true, `${version} tick ${tick}`);
                    assert.deepEqual(object.input, packed.input, `${version} tick ${tick}`);
                } else {
                    object = packed;
                }
                if (immobile) {
                    // no key adds anything: the move is the motion it had, and the held jump's cooldown is gone
                    assert.equal(state.pos.y, 1, `${version} tick ${tick}`);
                    assert.equal(state.jumpTicks, 0, `${version} tick ${tick}`);
                    assert.ok(Math.abs((state.pos.x - before.x) - held.x) < 1e-12, `${version} tick ${tick}`);
                    assert.ok(Math.abs((state.pos.z - before.z) - held.z) < 1e-12, `${version} tick ${tick}`);
                    assert.equal(state.control.jump, true);
                    assert.equal(state.control.forward, true);
                } else if (tick > 0) {
                    // the first tick awake jumps at once and walks
                    assert.ok(state.pos.y > 1.4, `${version} tick ${tick}`);
                    assert.ok(state.pos.z > before.z, `${version} tick ${tick}`);
                    // (down on the floor again for the next immobile tick)
                    state.pos.y = 1;
                    state.vel.y = standing;
                    state.onGround = true;
                    state.jumpTicks = 4;
                    object.position = { ...object.position, y: 1 };
                    object.velocity = { ...object.velocity, y: standing };
                    object.onGround = true;
                    object.jumpTicks = 4;
                    object.boundingBox = undefined;
                }
            }
        }
    });

    it('keeps a passenger from crouching from 1.20.2 and tells the engine it rides', () => {
        // origin/client1.20.1 LocalPlayer.java:666-669, client1.20.2 :664-668
        const crouches = version => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 1, 0.5, { onGround: true, passenger: true, prevControl: controls({ sneak: true }) });
            physics.updatePoses(state, physics.syncOf(world));
            physics.simulate({ state }, world);
            return [state.crouching, physics.carries.get(state).player.state().passenger];
        };
        assert.deepEqual(crouches('1.20.1'), [true, true]);
        assert.deepEqual(crouches('1.20.2'), [false, true]);
        assert.deepEqual(crouches('1.21.11'), [false, true]);
    });

    it('ticks a standalone passenger as Entity.rideTick: no crouch, no travel, on its seat until it dismounts', () => {
        // Entity.rideTick: motion zero, the tick, the vehicle's positionRider, and the fall distance reset (1.8.9
        // Entity.java:1321-1330, LivingEntity.java:1519-1524; 1.20.4 Entity.java:1843-1847; 26.3 Entity.java:2465-2470,
        // LivingEntity.java:3233-3236)
        for (const version of ['1.8.8', '1.12.2', '1.20.4', '1.21.11', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const keys = { forward: true, left: true, jump: true, sneak: true };
            for (const riding of [{ passenger: true }, { bot: { vehicle: { id: 7 } } }, { bot: { entity: { vehicle: { id: 7 } } } },
                { passenger: true, boatVehicle: { underwater: false, boundingBox: { min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } } } }]) {
                const physics = new adapter.NativePhysics(registry);
                const ctx = playerState(0.5, 3, 0.5, {
                    vel: new Vec3(0.6, 0.5, -0.3), fallDistance: 6, control: controls(keys), prevControl: controls(keys), ...riding,
                });
                const { state } = ctx;
                // (no throw: mineflayer's simulatePlayer can reach this on a riding bot)
                physics.simulate(ctx, world);
                assert.deepEqual([state.pos.x, state.pos.y, state.pos.z], [0.5, 3, 0.5], version);
                assert.equal(state.fallDistance, 0, version);
                assert.equal(state.crouching, !version.startsWith('1.2') && version !== '26.3', version);
                // the engine player holds the flag, and the seat follows the caller
                assert.equal(physics.carries.get(state).player.state().passenger, true, version);
                state.pos.set(1.5, 2.5, 0.5);
                physics.simulate(ctx, world);
                assert.deepEqual([state.pos.x, state.pos.y, state.pos.z], [1.5, 2.5, 0.5], version);
                // rideTick's zero leaves the rider's own air travel from rest, not the 0.5 it was handed
                assert.ok(Math.abs(state.vel.y - (-0.08 * f32(0.98))) < 1e-9, `${version}: ${state.vel.y}`);
            }
        }
    });

    it('takes the dismounted state back to ordinary travel, in the same engine player', () => {
        for (const version of ['1.8.8', '1.20.4', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const keys = { forward: true };
            const ctx = playerState(0.5, 3, 0.5, { passenger: true, control: controls(keys), prevControl: controls(keys) });
            const { state } = ctx;
            physics.simulate(ctx, world);
            const { player } = physics.carries.get(state);
            const left = state.vel.clone();
            state.passenger = false;
            physics.simulate(ctx, world);
            assert.equal(physics.carries.get(state).player, player);
            assert.equal(player.state().passenger, false);
            // Entity.removeVehicle leaves the motion: the first tick off moves by what the last ride tick left and its keys
            assert.ok(Math.abs((state.pos.y - 3) - left.y) < 1e-9, version);
            assert.ok(state.pos.z > 0.5, version);
            state.passenger = true;
            const seat = state.pos.clone();
            physics.simulate(ctx, world);
            assert.equal(state.pos.z, seat.z, version);
            assert.equal(state.pos.y, seat.y, version);
        }
    });

    it('keeps a passenger from flight, gliding and swimming as the source does', () => {
        // Player.travel's flight is !isPassenger, but the flying flag stays and aiStep's impulse runs (1.20.4
        // PlayerEntity.java:1488, 26.3 Player.java:1294-1296, 1295); no passenger starts a glide (1.20.4
        // ClientPlayerEntity.java:733, 26.3 LivingEntity.java:3143) or swims (26.3 Entity.java:1679-1681)
        for (const version of ['1.16.5', '1.20.4', '1.21.11', '26.3']) {
            const { registry, world } = worldOf(version, floor);
            const flying = new adapter.NativePhysics(registry);
            const flier = playerState(0.5, 3, 0.5, { passenger: true, flying: true, mayFly: true, control: controls({ jump: true }), prevControl: controls({ jump: true }) });
            flying.simulate(flier, world);
            assert.equal(flier.state.flying, true, version);
            assert.deepEqual([flier.state.pos.x, flier.state.pos.y], [0.5, 3], version);
            assert.ok(Math.abs(flier.state.vel.y - (f32(0.05 * 3) - 0.08) * f32(0.98)) < 1e-7, `${version}: ${flier.state.vel.y}`);

            const glider = new adapter.NativePhysics(registry);
            const jump = controls({ jump: true });
            const rider = playerState(0.5, 3, 0.5, { passenger: true, validElytraEquipped: true, control: jump });
            glider.simulate(rider, world);
            assert.equal(rider.state.fallFlying, false, version);
            const walker = playerState(0.5, 3, 0.5, { validElytraEquipped: true, control: jump });
            glider.simulate(walker, world);
            assert.equal(walker.state.fallFlying, true, version);

            const { registry: wet, world: pool } = worldOf(version, ({ registry: r, set }) => {
                const water = r.blocksByName.water.defaultState ?? r.blocksByName.water.id << 4;
                for (let x = -2; x <= 2; x += 1) for (let z = -2; z <= 2; z += 1) for (let y = 0; y <= 4; y += 1) set(x, y, z, water);
            });
            const swimmer = new adapter.NativePhysics(wet);
            const swimming = fields => {
                const ctx = playerState(0.5, 1, 0.5, {
                    sprinting: true, isInWater: true, isUnderWater: true, control: controls({ forward: true, sprint: true }), ...fields,
                });
                swimmer.simulate(ctx, pool);
                return ctx.state.swimming;
            };
            assert.equal(swimming({}), true, version);
            assert.equal(swimming({ passenger: true }), false, version);
            assert.equal(swimming({ swimming: true, passenger: true }), false, version);
        }
    });

    it('takes a passenger off the ground from 1.21.4, before the jump it would take', () => {
        // Player.tick: isSpectator() || isPassenger from 1.21.4 (1.21.3 PlayerEntity.java:256, 1.21.4 :259-260)
        const jumps = version => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1, 0.5, { passenger: true, onGround: true, control: controls({ jump: true }) });
            physics.simulate(ctx, world);
            return ctx.state.jumpTicks;
        };
        assert.equal(jumps('1.21.3'), 10);
        assert.equal(jumps('1.21.4'), 0);
        assert.equal(jumps('26.3'), 0);
    });

    it('launches or holds back a passenger riptide without leaving the seat', () => {
        // the launch's gate is TridentItem.releaseUsing's, the seat is rideTick's: the push is motion it zeroes (26.3
        // TridentItem.java:64, Entity.java:2465-2470)
        for (const [version, spins] of [['1.20.4', true], ['1.21.11', true], ['26.1', false], ['26.3', false]]) {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 10, 0.5, { riptide: 3, itemUseTicks: 12, yaw: 0, inRain: true, passenger: true });
            physics.simulate(ctx, world);
            assert.equal(ctx.state.autoSpinAttackTicks > 0, spins, version);
            assert.deepEqual([ctx.state.pos.x, ctx.state.pos.y, ctx.state.pos.z], [0.5, 10, 0.5], version);
        }
    });

    it("wants sleeping before gliding only from 1.21.5, with the bed plugin's sleep counted", () => {
        // Player.updatePlayerPose up to 1.21.4: gliding, sleeping, swimming, spin, crouching (1.21.4
        // PlayerEntity.java:437-450); getDesiredPose from 1.21.5: sleeping, swimming, gliding (1.21.5 Player.java:465-477)
        const wanted = (version, fields) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const { state } = playerState(0.5, 10, 0.5, fields);
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -1, minY: 9, minZ: -1, maxX: 2, maxY: 12, maxZ: 2 }, 2);
            physics.updatePoses(state, sync);
            return state.pose;
        };
        const asleep = { fallFlying: true, bot: { isSleeping: true } };
        assert.equal(wanted('1.21.4', asleep), adapter.Pose.FALL_FLYING);
        assert.equal(wanted('1.21.5', asleep), adapter.Pose.SLEEPING);
        assert.equal(wanted('1.21.5', { bot: { isSleeping: true }, swimming: true }), adapter.Pose.SLEEPING);
        assert.equal(wanted('1.21.4', { swimming: true, control: controls({ sneak: true }) }), adapter.Pose.SWIMMING);
        assert.equal(wanted('1.21.4', { flying: true, control: controls({ sneak: true }) }), adapter.Pose.STANDING);
    });

    it("takes the state's own no-gravity flag before the synced one", () => {
        // Entity.isNoGravity (1.10.2 Entity.java:902, 1.21.11 Entity.java:1443-1445): a state that carries the flag
        // sends it; one without it keeps reading bot.entity's synced data at 5
        const sent = (version, fields) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 10, 0.5, fields);
            physics.simulate(ctx, world);
            return [physics.carries.get(ctx.state).player.state().noGravity, ctx.state.vel.y];
        };
        const synced = { entity: { metadata: [0, 300, '', false, false, true] } };
        assert.deepEqual(sent('1.21.4', { noGravity: true }), [true, 0]);
        assert.equal(sent('1.21.4', { noGravity: false, bot: synced })[0], false);
        assert.equal(sent('1.21.4', { bot: synced })[0], true);
        assert.equal(sent('1.9.4', { noGravity: true })[0], false);
    });

    it('keeps an effect with a negative byte amplifier before 1.20.5', () => {
        // the effect packet reads a signed byte and MobEffectInstance keeps it before 1.20.5, so 255 is -1 and the
        // effect stays (1.20.4 EntityStatusEffectS2CPacket.java:49, StatusEffectInstance.java:73); from 1.20.5 a varint
        // the instance clamps to 0 - 255 (1.20.5 EntityStatusEffectS2CPacket.java:57, StatusEffectInstance.java:81)
        const inputsOf = (version, amplifier) => {
            const { registry } = worldOf(version, () => {});
            const physics = new adapter.NativePhysics(registry);
            const effects = {};
            for (const name of ['Levitation', 'SlowFalling', 'DolphinsGrace', 'JumpBoost', 'Blindness']) {
                const { id } = registry.effectsByName[name];
                effects[id] = { id, amplifier };
            }
            const { state } = playerState(0.5, 1, 0.5, { effects });
            return { inputs: physics.attributesAndEffects(state), state, physics, effects };
        };
        for (const amplifier of [-1, 255]) {
            const { inputs, state, physics, effects } = inputsOf('1.20.4', amplifier);
            assert.equal(inputs.levitation, true);
            assert.equal(inputs.levitationAmplifier, -1);
            assert.equal(inputs.slowFalling, true);
            assert.equal(inputs.dolphinsGrace, 1);
            assert.equal(inputs.jumpBoostAmplifier, -1);
            // LocalPlayer.aiStep starts no sprint under blindness (1.20.4 ClientPlayerEntity.java canStartSprinting)
            assert.equal(physics.canStartSprinting({ ...state, food: 20 }, { forward: 1, strafe: 0 }), false);
            assert.equal(physics.getEffectLevel(5, effects), 0);
        }
        assert.equal(inputsOf('1.20.4', 128).inputs.jumpBoostAmplifier, -128);
        assert.equal(inputsOf('1.21.4', 300).inputs.jumpBoostAmplifier, 255);
        assert.equal(inputsOf('1.21.4', -1).inputs.levitationAmplifier, 0);
    });

    it('folds every movement speed and jump strength modifier, in the order vanilla iterates them', () => {
        // AttributeInstance.calculateValue: base + ADD_VALUE, + base * ADD_MULTIPLIED_BASE, *= 1 + ADD_MULTIPLIED_TOTAL
        // (1.21.11 AttributeInstance.java:150-172), each operation in its Object2ObjectOpenHashMap's order: by the
        // Identifier from 1.21 (mix(hash) slots 29, 23 and 19 for custom, sprinting and effect.speed, iterated down)
        const { registry } = worldOf('1.21.4', () => {});
        const physics = new adapter.NativePhysics(registry);
        const speed = physics.attributeKey('movementSpeed');
        const jump = physics.attributeKey('jumpStrength');
        const attributes = {
            [speed]: { value: f32(0.1), modifiers: [
                { uuid: 'minecraft:sprinting', amount: f32(0.3), operation: 2 },
                { uuid: 'minecraft:effect.speed', amount: 0.4000000059604645, operation: 2 },
                { uuid: 'minecraft:custom', amount: 0.25, operation: 2 },
                { uuid: 'minecraft:boots', amount: 0.5, operation: 1 },
            ] },
            [jump]: { value: f32(0.42), modifiers: [{ uuid: 'minecraft:boots', amount: 0.5, operation: 1 }] },
        };
        const base = f32(0.1) + f32(0.1) * 0.5;
        const expected = base * (1 + 0.25) * (1 + f32(0.3)) * (1 + 0.4000000059604645);
        assert.notEqual(expected, base * (1 + f32(0.3)) * (1 + 0.4000000059604645) * (1 + 0.25));
        const { state } = playerState(0.5, 1, 0.5, { attributes });
        const inputs = physics.attributesAndEffects(state);
        assert.equal(inputs.movementSpeedValue, expected);
        assert.equal(inputs.jumpStrength, f32(0.42) + f32(0.42) * 0.5);
        // before 1.20.5 the UUIDs' HashSet buckets: slowness, sprinting, speed (as the engine folds them)
        const legacy = new adapter.NativePhysics(worldOf('1.20.4', () => {}).registry);
        const ids = { sprint: '662a6b8d-da3e-4c1c-8813-96ea6097278d', speed: '91aeaa56-376b-4498-935b-2f7f68070635', slowness: '7107de5e-7ce8-4030-940e-514c1f160890' };
        const amounts = { sprint: f32(0.3), speed: 0.4000000059604645, slowness: -0.15000000596046448 };
        const ordered = legacy.iterationOrder(['speed', 'sprint', 'slowness'].map(name => ({ uuid: ids[name], amount: amounts[name], operation: 2 })));
        assert.deepEqual(ordered.map(({ uuid }) => Object.keys(ids).find(name => ids[name] === uuid)), ['slowness', 'sprint', 'speed']);
        // 1.20.5's map by the UUIDs: sprinting, slowness, speed
        const v1205 = new adapter.NativePhysics(worldOf('1.20.5', () => {}).registry);
        const ordered1205 = v1205.iterationOrder(['speed', 'sprint', 'slowness'].map(name => ({ uuid: ids[name], amount: amounts[name], operation: 2 })));
        assert.deepEqual(ordered1205.map(({ uuid }) => Object.keys(ids).find(name => ids[name] === uuid)), ['sprint', 'slowness', 'speed']);
    });

    it('takes a compact attribute without modifiers for its clamped base', () => {
        // AttributeInstance.calculateValue with no modifier is sanitizeValue(base): RangedAttribute clamps step height to
        // 0..10 and gravity to -1..1 (1.21.11 AttributeInstance.java:150-172, RangedAttribute.java:35-37, Attributes.java:46, 86)
        const { registry, world } = worldOf('1.21.4', floor);
        const physics = new adapter.NativePhysics(registry);
        const attributes = {
            [physics.attributeKey('movementSpeed')]: { value: f32(0.1) },
            [physics.attributeKey('stepHeight')]: { value: 20 },
            [physics.attributeKey('gravity')]: { value: -0.04 },
        };
        const ctx = playerState(0.5, 3, 0.5, { attributes });
        const inputs = physics.attributesAndEffects(ctx.state);
        assert.equal(inputs.movementSpeedValue, f32(0.1));
        assert.equal(inputs.stepHeight, 10);
        assert.equal(inputs.gravity, -0.04);
        // NaN is the minimum (1.21.11 RangedAttribute.java:36)
        assert.equal(physics.attributesAndEffects({ ...ctx.state, attributes: { [physics.attributeKey('gravity')]: { value: NaN } } }).gravity, -1);
        assert.doesNotThrow(() => physics.simulate(ctx, world));
    });
});

describe('the flow lookup', () => {
    // FlowingFluid.getFlow looks below a neighbour only where it does not block motion. A wall sign (no collision) blocks
    // it, a carpet (collision) does not (1.16.5 FlowableFluid.java:52-75, 1.20.4 :65, 26.3 FlowingFluid.java:61)
    it('tells the blocks by material, forced solidity and tag, not by a collision box', () => {
        const described = (name, boxes) => ({ name, boxes, props: {}, meta: 0 });
        const blocks = (version, tag) => {
            const { registry } = worldOf(version, () => {});
            return new adapter.NativePhysics(registry).syncOf({}).mapper.flowBlocks.blocksFlow;
        };
        for (const version of ['1.14.4', '1.16.5', '1.19.4']) {
            const blocksFlow = blocks(version);
            assert.equal(blocksFlow(described('oak_wall_sign', [])), true, version);
            assert.equal(blocksFlow(described('white_carpet', [[0, 0, 0, 1, 1 / 16, 1]])), false, version);
            assert.equal(blocksFlow(described('stone', [[0, 0, 0, 1, 1, 1]])), true, version);
            assert.equal(blocksFlow(described('air', [])), false, version);
            assert.equal(blocksFlow(null), false, version);
        }
        for (const version of ['1.20.4', '1.21.11']) {
            const blocksFlow = blocks(version);
            assert.equal(blocksFlow(described('oak_wall_sign', [])), true, version);
            assert.equal(blocksFlow(described('white_carpet', [[0, 0, 0, 1, 1 / 16, 1]])), false, version);
            assert.equal(blocksFlow(described('snow', [[0, 0, 0, 1, 2 / 16, 1]])), false, version);
            assert.equal(blocksFlow(described('stone', [[0, 0, 0, 1, 1, 1]])), true, version);
            // the size test: a slab's average 0.833, a full-height post's 1
            assert.equal(blocksFlow(described('oak_slab', [[0, 0, 0, 1, 0.5, 1]])), true, version);
            assert.equal(blocksFlow(described('cobweb', [])), false, version);
        }
    });

    it('takes #blocks_fluid_flow from 26.3', () => {
        const physics = new adapter.NativePhysics(require('prismarine-registry')('26.3'));
        const { blocksFlow } = physics.syncOf({}).mapper.flowBlocks;
        assert.equal(blocksFlow({ name: 'oak_wall_sign', boxes: [] }), true);
        assert.equal(blocksFlow({ name: 'white_carpet', boxes: [[0, 0, 0, 1, 1 / 16, 1]] }), false);
    });

    /**
     * The audit's dry fixture: water at (0, 1, 0), its east neighbour holding no fluid over more water at (1, 0, 0), a
     * player in the water. FlowingFluid.getFlow reads the water below a neighbour that does not block its flow: that gives
     * the distance own - (own - 0.8888889F), the flow east; a neighbour that blocks leaves the flow zero (1.13.2
     * FlowableFluid.java:60-70, 1.16.5 :52-75, 1.20.4 :65, 26.3 FlowingFluid.java:58-70). The one cell pushes the player by
     * its unit flow times 0.014 before the move.
     */
    const beside = (version, name, props) => {
        const { registry, world, Block } = worldOf(version, ({ registry: r, Block: B, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
            set(1, 0, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
            set(1, 1, 0, B.fromProperties(name, props, 0).stateId);
        });
        const physics = new adapter.NativePhysics(registry);
        const sync = physics.syncOf(world);
        sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 2);
        const flow = sync.world.fluidFlowAt(0, 1, 0);
        const ctx = playerState(0.5, 1, 0.5);
        physics.simulate(ctx, world);
        const block = Block.fromStateId(Block.fromProperties(name, props, 0).stateId, 0);
        return { flow, x: ctx.state.pos.x, z: ctx.state.pos.z, described: sync.mapper.describe(block), blocksFlow: sync.mapper.flowBlocks.blocksFlow };
    };
    const blocked = (version, name, props) => {
        const { flow, x, z, described, blocksFlow } = beside(version, name, props);
        assert.equal(blocksFlow(described), true, `${version} ${name}`);
        assert.deepEqual(flow, { x: 0, y: 0, z: 0 }, `${version} ${name}`);
        assert.deepEqual([x, z], [0.5, 0.5], `${version} ${name}`);
        return described;
    };
    const flowing = (version, name, props) => {
        const { flow, x, z, described, blocksFlow } = beside(version, name, props);
        assert.equal(blocksFlow(described), false, `${version} ${name}`);
        assert.deepEqual(flow, { x: 1, y: 0, z: 0 }, `${version} ${name}`);
        assert.deepEqual([x, z], [0.5 + 0.014, 0.5], `${version} ${name}`);
        return described;
    };

    it('holds the current back at a wall sign with no collision, and lets it by a carpet and two layers of snow with one', () => {
        // a sign's WOOD blocks motion to 1.19.4 (1.16.5 Material.java:102), forceSolidOn makes it solid to 26.2 (1.20.4
        // AbstractBlock.java:466-488), #all_signs is in #blocks_fluid_flow from 26.3 (26.3 VanillaBlockTagsProvider.java:249).
        // CARPET and SNOW_LAYER allow motion (1.13.2 Material.java:19, 60; 1.16.5 :175-200), the carpet's bounds are too
        // small from 1.20, snow is forceSolidOff, and neither is in the 26.3 tag
        for (const version of ['1.13.2', '1.14.4', '1.16.5', '1.19.4', '1.20.4', '1.21.11', '26.2', '26.3']) {
            const sign = version === '1.13.2' ? 'wall_sign' : 'oak_wall_sign';
            assert.deepEqual(blocked(version, sign, { facing: 'east', waterlogged: false }).boxes, [], version);
            assert.equal(flowing(version, 'white_carpet', {}).boxes.length, 1, version);
            assert.deepEqual(flowing(version, 'snow', { layers: 2 }).boxes.map(box => box[4]), [0.125], version);
        }
    });

    it('lets the current by the air of 1.13.2, whose material allows motion', () => {
        // Block.setup registers air by add(default id, lv) and types some of its variables by their class: tripwire_hook
        // (DECORATION), chorus_plant (PLANT; 1.13.2 Block.java:798-799, 2005-2006, 2968-2971, Material.java:7,
        // 23, 71, where method_17829 is allowsMovement)
        flowing('1.13.2', 'air', {});
        flowing('1.13.2', 'tripwire_hook', { attached: false, facing: 'north', powered: false });
        flowing('1.13.2', 'chorus_plant', { down: false, east: false, north: false, south: false, up: false, west: false });
    });

    it('moves the blocks over the 26.2 - 26.3 line by the tag, not the shape', () => {
        // The stable scaffolding bounds a whole cube: solid by size to 26.2 (26.2 BlockBehaviour.java:824-842), in no tag of
        // #blocks_fluid_flow from 26.3. A bud's box is too small to 26.2; #blocks_motion_no_leaves names it from 26.3 (26.3
        // VanillaBlockTagsProvider.java:240, 249)
        const scaffolding = { bottom: false, distance: 0, waterlogged: false };
        const bud = { facing: 'up', waterlogged: false };
        blocked('26.2', 'scaffolding', scaffolding);
        flowing('26.3', 'scaffolding', scaffolding);
        flowing('26.2', 'large_amethyst_bud', bud);
        blocked('26.3', 'large_amethyst_bud', bud);
    });

    it('resolves the nested tags of #blocks_fluid_flow whatever the shape says', () => {
        // #blocks_motion (#blocks_motion_no_leaves, #leaves) and #all_signs (26.3 VanillaBlockTagsProvider.java:240-241, 249)
        const physics = new adapter.NativePhysics(require('prismarine-registry')('26.3'));
        const { blocksFlow } = physics.syncOf({}).mapper.flowBlocks;
        for (const name of ['oak_wall_sign', 'oak_sign', 'oak_hanging_sign', 'oak_wall_hanging_sign', 'oak_leaves', 'stone', 'snow_block']) {
            assert.equal(blocksFlow({ name, boxes: [] }), true, name);
        }
        for (const name of ['white_carpet', 'snow', 'cobweb', 'scaffolding', 'air']) {
            assert.equal(blocksFlow({ name, boxes: [[0, 0, 0, 1, 1, 1]] }), false, name);
        }
    });

    it('takes no collision box for the tag where none is told', () => {
        // a BlockMapper of 26.3 without a tag test keeps the last generated predicate, 26.2's
        const registry = require('prismarine-registry')('26.3');
        const { blocksFlow } = new adapter.BlockMapper(registry.version, registry).flowBlocks;
        assert.equal(blocksFlow({ name: 'oak_wall_sign', boxes: [] }), true);
        assert.equal(blocksFlow({ name: 'snow', boxes: [[0, 0, 0, 1, 0.125, 1]] }), false);
    });
});

describe('projectile families', () => {
    it('refuses an arrow or trident in the generic batch, whose motion it would not be', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        const shot = kind => [{ kind, position: { x: 0, y: 100, z: 0 }, velocity: { x: 1, y: 1, z: 0 } }];
        assert.throws(() => binding.simulateVersionedProjectileTicks('1.21.4', world, shot('arrow'), 1), /simulateVersionedArrowTicks/);
        assert.throws(() => binding.simulateVersionedProjectileTicks('1.21.4', world, shot('spectral_arrow'), 1), /simulateVersionedArrowTicks/);
        assert.throws(() => binding.simulateVersionedProjectileTicks('1.21.4', world, shot('trident'), 1), /simulateVersionedTridentTicks/);
        assert.equal(binding.simulateVersionedProjectileTicks('1.21.4', world, shot('snowball'), 1).length, 1);
    });

    it('keeps a trident in the block it hit across the batch', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        for (let y = 98; y < 103; y += 1) world.setFullBlock(1, y, 0, 1);
        const shot = [{ position: { x: 0.5, y: 100.5, z: 0.5 }, velocity: { x: 2, y: 0, z: 0 } }];
        const [first] = binding.simulateVersionedTridentTicks('1.21.4', world, shot, 1);
        const [later] = binding.simulateVersionedTridentTicks('1.21.4', world, shot, 5);
        assert.equal(later.inGround, true);
        assert.equal(later.position.y, first.position.y);
        assert.equal(later.inGroundTime, 4);
    });

    // Every name of the AbstractArrow families, refused by both generic calls. Their ticks differ from ThrowableProjectile's
    // from the first tick (26.3 AbstractArrow.java:164-250 against ThrowableProjectile.java:41-60; 1.20.4
    // PersistentProjectileEntity.java:153-280 against ThrownEntity.java:46-96), and a thrown item has no place for their
    // embedded state.
    const arrowNames = ['arrow', 'Arrow', 'minecraft:arrow', 'thrown_arrow', 'ThrownArrow', 'arrow_entity', 'spectral_arrow',
        'SpectralArrow', 'minecraft:spectral_arrow', 'tipped_arrow', 'TippedArrow', 'minecraft:tipped_arrow', 'entity:tipped_arrow'];
    const tridentNames = ['trident', 'minecraft:trident', 'thrown_trident', 'ThrownTrident', 'trident_entity'];
    const fired = kind => ({ kind, position: { x: 0, y: 100, z: 0 }, velocity: { x: 1, y: 1, z: 0 } });

    it('refuses every arrow and trident alias in both generic calls, either side of 1.21.2', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        for (const [names, call] of [[arrowNames, /simulateVersionedArrowTicks/], [tridentNames, /simulateVersionedTridentTicks/]]) {
            for (const name of names) {
                assert.throws(() => binding.simulateProjectileTick(world, fired(name)), call, name);
                for (const version of ['1.9.4', '1.12.2', '1.20.4', '1.21.1', '1.21.2', '1.21.4', '26.3']) {
                    assert.throws(() => binding.simulateVersionedProjectileTicks(version, world, [fired(name)], 1), call, `${version} ${name}`);
                }
            }
        }
        // one such name refuses the whole batch
        assert.throws(() => binding.simulateVersionedProjectileTicks('1.21.4', world, [fired('snowball'), fired('tipped_arrow')], 1), /simulateVersionedArrowTicks/);
    });

    it('never takes an unknown family for a snowball', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        for (const name of ['not_a_projectile', 'generic', '', 'evoker_fangs', 'minecraft:fishing_rod', 'entity.minecraft.arrow']) {
            assert.throws(() => binding.simulateProjectileTick(world, fired(name)), /unsupported projectile family/, name);
            for (const version of ['1.20.4', '1.21.4']) {
                assert.throws(() => binding.simulateVersionedProjectileTicks(version, world, [fired(name)], 1), /unsupported projectile family/, name);
            }
        }
    });

    it('keeps the thrown-item motion of the genuine throwables and of an unnamed projectile', () => {
        // 1.21.4 ThrownEntity.java:47-62: gravity, 0.99F, the move; 1.20.4 :46-96: the move, 0.99F, 0.03F
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        const drag = Math.fround(0.99);
        const modern = { x: drag, y: (1 - 0.03) * drag, z: 0 };
        for (const kind of ['snowball', 'egg', 'ender_pearl', undefined]) {
            const shot = fired(kind);
            for (const result of [binding.simulateVersionedProjectileTicks('1.21.4', world, [shot], 1)[0], binding.simulateProjectileTick(world, shot)]) {
                assert.deepEqual(result.velocity, modern, kind);
                assert.deepEqual(result.position, { x: modern.x, y: 100 + modern.y, z: 0 }, kind);
            }
            const [old] = binding.simulateVersionedProjectileTicks('1.20.4', world, [shot], 1);
            assert.deepEqual(old.position, { x: 1, y: 101, z: 0 }, kind);
            assert.deepEqual(old.velocity, { x: drag, y: drag - Math.fround(0.03), z: 0 }, kind);
        }
    });

    it('takes the air and water motion of an arrow and a trident from their own calls', () => {
        // the whole old motion moves them, then the inertia and gravity (1.21.4 PersistentProjectileEntity.java:166-250,
        // 1.21 :261-277): 0.6F water for an arrow, 0.99F for a trident (1.13.2 TridentEntity.java:193)
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        const shot = { position: { x: 0, y: 100, z: 0 }, velocity: { x: 1, y: 1, z: 0 } };
        assert.deepEqual(binding.simulateVersionedArrowTicks('1.21.4', world, [shot], 1)[0].position, { x: 1, y: 101, z: 0 });
        for (let x = -1; x <= 2; x += 1) for (let y = 99; y <= 102; y += 1) for (let z = -1; z <= 1; z += 1) world.setFluid(x, y, z, 1);
        const inWater = { ...shot, inWater: true, position: { x: 0.5, y: 100.5, z: 0.5 } };
        for (const version of ['1.20.4', '1.21.4']) {
            for (const [call, inertia] of [['simulateVersionedArrowTicks', 0.6], ['simulateVersionedTridentTicks', 0.99]]) {
                const [result] = binding[call](version, world, [inWater], 1);
                assert.deepEqual(result.position, { x: 1.5, y: 101.5, z: 0.5 }, `${call} ${version}`);
                assert.equal(result.velocity.x, Math.fround(inertia), `${call} ${version}`);
                assert.equal(result.inWater, true, `${call} ${version}`);
            }
        }
    });

    it('continues an embedded arrow and trident through the unversioned calls', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        world.setFullBlock(1, 100, 0, 7);
        const shot = { position: { x: 0.5, y: 100.5, z: 0.5 }, velocity: { x: 2, y: 0, z: 0 } };
        for (const call of ['simulateArrowTick', 'simulateTridentTick']) {
            const first = binding[call](world, shot);
            assert.equal(first.inGround, true);
            const next = binding[call](world, { ...shot, ...first });
            assert.equal(next.inGround, true);
            assert.deepEqual(next.position, first.position);
            assert.equal(next.life, 1);
            assert.equal(next.inGroundTime, 1);
        }
    });

    it('matches a batch to repeated single ticks as a projectile enters and leaves the water', () => {
        // the base tick's isInWater goes on from tick to tick (26.3 ThrowableProjectile.java:41-60, AbstractArrow.java:164-251,
        // 1.21.4 ThrownEntity.java:47-62, 1.20.4 :46-96): each call's result must carry it, in both orders
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        for (let chunkX = -1; chunkX <= 1; chunkX += 1) for (let chunkZ = -1; chunkZ <= 1; chunkZ += 1) world.setChunkLoaded(chunkX, chunkZ, true);
        for (let y = 99; y <= 102; y += 1) for (let z = -1; z <= 1; z += 1) world.setFluid(1, y, z, 1);
        const shot = { position: { x: 0.5, y: 100.5, z: 0.5 }, velocity: { x: 1, y: 0, z: 0 }, gravity: 0 };
        const families = [
            ['simulateVersionedProjectileTicks', { ...shot, kind: 'snowball' }],
            ['simulateVersionedProjectileTicks', { ...shot, kind: 'small_fireball' }],
            ['simulateVersionedArrowTicks', shot],
            ['simulateVersionedTridentTicks', shot],
        ];
        for (const version of ['1.20.4', '1.21.1', '1.21.2', '1.21.4', '26.3']) {
            for (const [call, launched] of families) {
                let state = launched;
                const seen = [];
                for (let ticks = 1; ticks <= 6; ticks += 1) {
                    const [single] = binding[call](version, world, [state], 1);
                    assert.deepEqual(single, binding[call](version, world, [launched], ticks)[0], `${call} ${launched.kind} ${version} ${ticks}`);
                    seen.push(single.inWater);
                    state = { ...state, ...single };
                }
                // the base tick after the move finds the water on the tick that enters it, the one before on the next
                assert.equal(seen[0], !['1.20.4', '1.21.1'].includes(version), `${call} ${version}`);
                assert.ok(seen.includes(true), `${call} ${version}`);
                assert.equal(seen[5], false, `${call} ${version}`);
            }
        }
    });

    // A one-tick call carries what the batch keeps, the embedded state and the despawn count included (26.3
    // AbstractArrow.java:192-210, 356-362, 1.21.4 PersistentProjectileEntity.java:195-199, 1.13.2 TridentEntity.java:57-92)
    const embeddedKinds = [['simulateVersionedTridentTicks', '1.13.2'], ['simulateVersionedTridentTicks', '1.20.4'],
        ['simulateVersionedTridentTicks', '1.21.4'], ['simulateVersionedTridentTicks', '26.3'],
        ['simulateVersionedArrowTicks', '1.21.4']];
    const carried = (state, { position, velocity, inWater, inGround, life, inGroundTime, shakeTime, dealtDamage, lastBlockId, lastBlockPosition }) => ({
        ...state, position, velocity, inWater, inGround, life, inGroundTime, shakeTime, dealtDamage, lastBlockId, lastBlockPosition,
    });
    const launched = { position: { x: 0.5, y: 100.5, z: 0.5 }, velocity: { x: 2, y: 0, z: 0 }, fallImpulse: { x: 0.5, y: 0.5, z: 0.5 } };

    it('matches repeated single ticks to the batch for an embedded trident and its arrow control', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        world.setFullBlock(1, 100, 0, 7);
        for (const [call, version] of embeddedKinds) {
            let state = launched;
            for (let ticks = 1; ticks <= 8; ticks += 1) {
                const [single] = binding[call](version, world, [state], 1);
                assert.deepEqual(single, binding[call](version, world, [launched], ticks)[0], `${call} ${version} ${ticks}`);
                assert.equal(single.inGround, true);
                assert.equal(single.life, ticks - 1);
                assert.equal(single.lastBlockId, 7);
                assert.deepEqual(single.lastBlockPosition, { x: 1, y: 100, z: 0 });
                state = carried(state, single);
            }
        }
    });

    it('resumes the despawn count of an embedded trident from the result it returned', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        const world = new binding.NativeWorld();
        world.setFullBlock(1, 100, 0, 7);
        for (const [call, version] of embeddedKinds) {
            const [embedded] = binding[call](version, world, [launched], 1);
            const [waiting] = binding[call](version, world, [carried(launched, { ...embedded, life: 1198 })], 1);
            assert.equal(waiting.life, 1199);
            assert.equal(waiting.discarded, false);
            assert.equal(binding[call](version, world, [carried(launched, waiting)], 1)[0].discarded, true, version);
        }
    });

    it('holds an embedded trident through a replaced block and drops it when the block goes', () => {
        const binding = require('../dist/native').LoadNativePhysics();
        for (const [call, version] of embeddedKinds) {
            const world = new binding.NativeWorld();
            world.setFullBlock(1, 100, 0, 7);
            let state = launched;
            const tick = () => {
                const [result] = binding[call](version, world, [state], 1);
                state = carried(state, result);
                return result;
            };
            tick();
            world.setFullBlock(1, 100, 0, 8);
            const held = tick();
            assert.equal(held.inGround, true, version);
            assert.equal(held.startedFalling, false, version);
            assert.equal(held.lastBlockId, 7, version);
            world.removeBlock(1, 100, 0);
            const dropped = tick();
            assert.equal(dropped.startedFalling, true, version);
            assert.equal(dropped.inGround, false, version);
            assert.equal(dropped.life, 0, version);
        }
    });
});

describe('NativeRiddenMobPhysics', () => {
    it("imports the mob's no-gravity flag, its own or its entity's synced one, from 1.10", () => {
        // LivingEntity.travel takes no gravity under NoGravity (1.12.2 LivingEntity.java:1549-1550, 1.16.5 :2084; 1.21.11
        // Entity.java:1443-1445, LivingEntity.java:2351)
        const fell = (version, fields) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativeRiddenMobPhysics(registry);
            const ctx = riddenState('horse', 0.5, 10, 0.5, fields);
            physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
            return ctx.state.vel.y;
        };
        const synced = { metadata: [0, 300, '', false, false, true] };
        assert.equal(fell('1.21.4', { noGravity: true }), 0);
        assert.equal(fell('1.12.2', { entity: synced }), 0);
        assert.ok(fell('1.21.4', {}) < 0);
        assert.ok(fell('1.21.4', { noGravity: false, entity: synced }) < 0);
        assert.ok(fell('1.9.4', { noGravity: true }) < 0);
    });

    it('carries the stuck multiplier of a web from tick to tick', () => {
        const webs = ({ registry: r, set }) => {
            floor({ registry: r, set });
            const web = r.blocksByName.cobweb.defaultState;
            for (let x = -3; x <= 3; x += 1) for (let z = -3; z <= 3; z += 1) set(x, 1, z, web);
        };
        const { registry, world } = worldOf('1.20.4', webs);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, stuckSpeedMultiplier: new Vec3(0, 0, 0) });
        const rider = { yawDegrees: 0, pitchDegrees: 0, zza: 1 };
        physics.simulateRidden(ctx, world, rider);
        assert.equal(ctx.state.stuckSpeedMultiplier.x, 0.25);
        const moved = ctx.state.pos.z;
        physics.simulateRidden(ctx, world, rider);
        // the slowed second step is a quarter of a free one
        const free = riddenState('horse', 0.5, 1, 0.5, { onGround: true });
        const open = worldOf('1.20.4', floor);
        physics.simulateRidden(free, open.world, rider);
        physics.simulateRidden(free, open.world, rider);
        assert.ok(ctx.state.pos.z - moved < (free.state.pos.z - 0.5) / 2);
    });

    /** A floor with a wall of the block at z 1, three blocks high; a mob walking along +z meets it. */
    const wallOf = name => ({ registry: r, set }) => {
        floor({ registry: r, set });
        const block = r.blocksByName[name].defaultState;
        for (let x = -4; x <= 4; x += 1) for (let y = 1; y <= 3; y += 1) set(x, y, 1, block);
    };
    const walking = { yawDegrees: 0, pitchDegrees: 0, zza: 1 };
    // the horse's MOVEMENT_SPEED on a friction of 0.6F (1.20.4 LivingEntity.java:2248-2249)
    const groundStep = f32(f32(0.22499999403953552) * f32(f32(0.21600002) / f32(f32(f32(0.6) * f32(0.6)) * f32(0.6))));

    it('slows a mount through a web until the move after the last contact, without a field of its own', () => {
        // Entity.move spends the multiplier and zeroes the motion, the web leaves it again while the box is in it (1.20.4
        // Entity.java:611-615, 2371; 26.3 :783-790, 3072-3075): a slowed step of a quarter, one more after the box has left
        const { registry, world } = worldOf('1.20.4', wallOf('cobweb'));
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const ctx = riddenState('horse', 0.5, 1, -1.5, { onGround: true, vel: new Vec3(0, -0.0784, 0) });
        assert.equal('stuckSpeedMultiplier' in ctx.state, false);
        const moves = [];
        for (let tick = 0; tick < 200 && ctx.state.pos.z < 3; tick += 1) {
            const stuck = ctx.state.stuckSpeedMultiplier?.x ?? 0;
            const z = ctx.state.pos.z;
            physics.simulateRidden(ctx, world, walking);
            moves.push({ stuck, step: ctx.state.pos.z - z, left: ctx.state.stuckSpeedMultiplier.x });
        }
        const slowed = moves.filter(move => move.stuck > 0);
        assert.ok(slowed.length > 3);
        for (const move of slowed.slice(1)) assert.ok(Math.abs(move.step - groundStep * 0.25) < 1e-7);
        const last = moves.findLastIndex(move => move.stuck > 0);
        assert.equal(moves[last].left, 0);
        assert.ok(Math.abs(moves[last + 1].step - groundStep) < 1e-7);
        assert.equal(moves[last + 1].left, 0);
        assert.ok(moves[last + 2].step > moves[last + 1].step);
    });

    it('keeps the stuck multiplier through a correction of the position', () => {
        // nothing but Entity.move and makeStuckInBlock writes it (26.3 Entity.java:237, 311, 783-790, 3072-3075), not the
        // teleport that moves the mob out of the web
        const { registry, world } = worldOf('1.20.4', wallOf('cobweb'));
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const ctx = riddenState('horse', 0.5, 1, 1.5, { onGround: true, vel: new Vec3(0, -0.0784, 0) });
        physics.simulateRidden(ctx, world, walking);
        assert.equal(ctx.state.stuckSpeedMultiplier.x, 0.25);
        const asked = ctx.state.vel.z * 0.98 + groundStep;
        ctx.state.pos.set(0.5, 1, -2.5);
        physics.simulateRidden(ctx, world, walking);
        assert.ok(Math.abs(ctx.state.pos.z + 2.5 - asked * 0.25) < 1e-7);
        assert.equal(ctx.state.stuckSpeedMultiplier.x, 0);
    });

    it('takes the multiplier of berries and powder snow only where the block has it', () => {
        // (0.8F, 0.75, 0.8F) and (0.9F, 1.5, 0.9F) for a living entity, by its feet in powder snow (1.20.4
        // SweetBerryBushBlock.java:78, PowderSnowBlock.java:61-62; 26.3 :76-77, :55-56)
        for (const [version, block, x, y] of [
            ['1.14.4', 'sweet_berry_bush', f32(0.8), 0.75],
            ['1.20.4', 'sweet_berry_bush', f32(0.8), 0.75],
            ['1.17.1', 'powder_snow', f32(0.9), 1.5],
            ['1.20.4', 'powder_snow', f32(0.9), 1.5],
        ]) {
            const { registry, world } = worldOf(version, wallOf(block));
            const physics = new adapter.NativeRiddenMobPhysics(registry);
            const ctx = riddenState('horse', 0.5, 1, 1.5, { onGround: true, vel: new Vec3(0, -0.0784, 0) });
            physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
            assert.deepEqual([ctx.state.stuckSpeedMultiplier.x, ctx.state.stuckSpeedMultiplier.y], [x, y], `${version} ${block}`);
        }
        assert.equal(worldOf('1.13.2', floor).registry.blocksByName.sweet_berry_bush, undefined);
        assert.equal(worldOf('1.16.5', floor).registry.blocksByName.powder_snow, undefined);
    });

    it('takes the Weaving of the mob to the web', () => {
        // WebBlock.entityInside: (0.5, 0.25, 0.5) under Weaving (1.20.5 CobwebBlock.java:25-28, 26.3 WebBlock.java:19-24)
        const { registry, world } = worldOf('1.21.1', wallOf('cobweb'));
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const effects = { [registry.effectsByName.Weaving.id]: { amplifier: 0 } };
        const ctx = riddenState('horse', 0.5, 1, 1.5, { onGround: true, vel: new Vec3(0, -0.0784, 0), effects });
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
        assert.equal(ctx.state.stuckSpeedMultiplier.x, 0.5);
    });

    it('walks through a web in repeated ticks as in a batch', () => {
        const { registry, world } = worldOf('1.20.4', wallOf('cobweb'));
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const calls = [];
        const binding = { ...physics.binding, simulateVersionedHorseTick: (version, nativeWorld, input) => {
            calls.push({ nativeWorld, input });
            return physics.binding.simulateVersionedHorseTick(version, nativeWorld, input);
        } };
        const spying = new adapter.NativeRiddenMobPhysics(registry, { binding });
        const ctx = riddenState('horse', 0.5, 1, -1.5, { onGround: true, vel: new Vec3(0, -0.0784, 0) });
        const ticks = 60;
        for (let tick = 0; tick < ticks; tick += 1) spying.simulateRidden(ctx, world, walking);
        const [{ nativeWorld, input }] = calls;
        const [batch] = physics.binding.simulateVersionedHorseTicks('1.20.4', nativeWorld, [input], ticks);
        assert.deepEqual(batch.position, { x: ctx.state.pos.x, y: ctx.state.pos.y, z: ctx.state.pos.z });
        assert.equal(batch.stuckSpeedMultiplier.x, ctx.state.stuckSpeedMultiplier.x);
    });

    it("takes the mob's measured size, zero attributes, effects and travel attributes to the engine", () => {
        const { registry, world } = worldOf('1.21.1', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const calls = [];
        const binding = { ...physics.binding, simulateVersionedHorseTick: (version, nativeWorld, input) => {
            calls.push(input);
            return physics.binding.simulateVersionedHorseTick(version, nativeWorld, input);
        } };
        const spying = new adapter.NativeRiddenMobPhysics(registry, { binding });
        const gravity = spying.attributeKey('gravity');
        const ctx = riddenState('horse', 0.5, 1, 0.5, {
            onGround: true,
            movementSpeed: 0,
            dimensions: { width: f32(2.8), height: f32(3.2) },
            attributes: { [gravity]: { value: 0.04 } },
            effects: { [registry.effectsByName.SlowFalling.id]: { amplifier: 0 }, [registry.effectsByName.Levitation.id]: { amplifier: 2 } },
        });
        spying.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0, zza: 1 });
        const [input] = calls;
        assert.equal(input.movementSpeed, 0);
        assert.equal(input.halfWidth, f32(2.8) / 2);
        assert.equal(input.height, f32(3.2));
        assert.equal(input.gravity, 0.04);
        assert.equal(input.slowFalling, true);
        assert.equal(input.levitationAmplifier, 2);
    });

    it('steers and jumps a mount by its zero attributes, sends them every tick and rounds them through a batch', () => {
        // MOVEMENT_SPEED and JUMP_STRENGTH go down to 0 (1.20.4 EntityAttributes.java:29, 45; 26.3 Attributes.java:73, 80).
        // executeRidersJump's impulse is the attribute's, its push forward 0.4F of the charge (1.20.4
        // AbstractHorseEntity.java:809-820): the mob leaves the floor by nothing but the push
        for (const version of ['1.20.4', '1.21.4', '1.21.9']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativeRiddenMobPhysics(registry);
            const calls = [];
            const binding = { ...physics.binding, simulateVersionedHorseTick: (v, nativeWorld, input) => {
                calls.push({ nativeWorld, input });
                return physics.binding.simulateVersionedHorseTick(v, nativeWorld, input);
            } };
            const spying = new adapter.NativeRiddenMobPhysics(registry, { binding });
            const ctx = riddenState('horse', 0.5, 1, -1.5, { onGround: true, movementSpeed: 0, jumpStrength: 0 });
            for (let tick = 0; tick < 3; tick += 1) spying.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0, zza: 1 });
            assert.equal(ctx.state.pos.z, -1.5, version);
            assert.equal(calls[2].input.movementSpeed, 0, version);
            assert.equal(calls[2].input.jumpStrength, 0, version);
            spying.riderJump(ctx.state, 100);
            spying.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0, zza: 1 });
            assert.equal(ctx.state.pos.y, 1, version);
            assert.ok(Math.abs(ctx.state.pos.z + 1.5 - f32(0.4)) < 1e-6, version);
            assert.equal(ctx.state.jumpPendingScale, 0, version);
            const [batch] = physics.binding.simulateVersionedHorseTicks(version, calls[0].nativeWorld, [calls[0].input], 3);
            assert.equal(batch.position.z, -1.5, version);
            assert.equal(batch.movementSpeed, 0, version);
            assert.equal(batch.jumpStrength, 0, version);
        }
    });

    it('takes Jump Boost alone to a mount of no jump strength', () => {
        // the float 0.1F a level (1.20.4 AbstractHorseEntity.java:810-811, 1.20.5 LivingEntity.java:2065-2068)
        const { registry, world } = worldOf('1.21.4', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const effects = { [registry.effectsByName.JumpBoost.id]: { amplifier: 1 } };
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, jumpStrength: 0, effects });
        physics.riderJump(ctx.state, 100);
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
        assert.equal(ctx.state.pos.y, 1 + f32(f32(0.1) * 2));
    });

    it("takes the mob's base for an attribute the state has not, and a tiny one for itself", () => {
        // an attribute not given is the engine's mob base, its 0.225 and the 0.42 of JUMP_STRENGTH (26.3 Attributes.java:73)
        const { registry, world } = worldOf('1.20.4', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const settled = fields => riddenState('horse', 0.5, 1, -1.5, { onGround: true, vel: new Vec3(0, -0.0784, 0), ...fields });
        const omitted = settled({ movementSpeed: undefined, jumpStrength: undefined });
        physics.simulateRidden(omitted, world, walking);
        assert.ok(Math.abs(omitted.state.pos.z + 1.5 - groundStep) < 1e-7);
        const jumping = riddenState('horse', 0.5, 1, 0.5, { onGround: true, movementSpeed: undefined, jumpStrength: undefined });
        physics.riderJump(jumping.state, 100);
        physics.simulateRidden(jumping, world, { yawDegrees: 0, pitchDegrees: 0 });
        assert.equal(jumping.state.pos.y, 1 + 0.41999998688697815);
        const tiny = settled({ movementSpeed: 0.001 });
        physics.simulateRidden(tiny, world, walking);
        const step = f32(f32(0.001) * f32(f32(0.21600002) / f32(f32(f32(0.6) * f32(0.6)) * f32(0.6))));
        assert.ok(Math.abs(tiny.state.pos.z + 1.5 - step) < 1e-9);
    });

    it('refuses an attribute no RangedAttribute keeps', () => {
        // sanitizeValue keeps a value in range and not NaN (1.20.5 ClampedEntityAttribute.java:35-36, 26.3
        // RangedAttribute.java:29-31): the engine is not asked to guess what such a value was
        const { registry, world } = worldOf('1.20.4', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        for (const fields of [{ movementSpeed: -0.1 }, { movementSpeed: NaN }, { jumpStrength: -0.1 }, { jumpStrength: Infinity }]) {
            const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, ...fields });
            assert.throws(() => physics.simulateRidden(ctx, world, walking), /invalid values/);
        }
    });

    /** The engine as the adapter calls it, with every input and output of its ticks. */
    const spied = registry => {
        const direct = new adapter.NativeRiddenMobPhysics(registry);
        const calls = [];
        const binding = { ...direct.binding, simulateVersionedHorseTick: (version, nativeWorld, input) => {
            const output = direct.binding.simulateVersionedHorseTick(version, nativeWorld, input);
            calls.push({ nativeWorld, input, output });
            return output;
        } };
        return { direct, calls, physics: new adapter.NativeRiddenMobPhysics(registry, { binding }) };
    };

    it('keeps a measured size through the engine, tick to tick and in a batch', () => {
        // SCALE multiplies the dimensions from 1.20.5 (1.20.5 LivingEntity.java:3250-3255, 26.3 :3684-3689)
        const { registry, world } = worldOf('1.21.1', floor);
        const { direct, calls, physics } = spied(registry);
        const dimensions = { width: f32(2.7929688), height: f32(3.2) };
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, vel: new Vec3(0, -0.0784, 0), dimensions });
        for (let tick = 0; tick < 3; tick += 1) physics.simulateRidden(ctx, world, walking);
        for (const { input, output } of calls) {
            assert.equal(input.halfWidth, dimensions.width / 2);
            assert.equal(input.height, dimensions.height);
            assert.ok(Math.abs(output.halfWidth - dimensions.width / 2) < 1e-6);
            assert.ok(Math.abs(output.height - dimensions.height) < 1e-6);
        }
        assert.deepEqual(ctx.state.dimensions, dimensions);
        const [batch] = direct.binding.simulateVersionedHorseTicks('1.21.1', calls[0].nativeWorld, [calls[0].input], 3);
        assert.deepEqual(batch.position, { x: ctx.state.pos.x, y: ctx.state.pos.y, z: ctx.state.pos.z });
    });

    it('stands a scaled camel up to its scaled size and keeps it there', () => {
        // Camel.standUp refreshes the dimensions at once (1.21.11 Camel.java:244-249, 579-584), getDimensions scaling the
        // pose's (26.3 LivingEntity.java:3684-3689); the state's size is the one in its pose (RiddenMobState.dimensions)
        const { registry, world } = worldOf('1.21.1', floor);
        const { calls, physics } = spied(registry);
        const sittingHeight = f32(f32(2.375) - f32(1.43));
        const dimensions = { width: f32(f32(1.7) * 2), height: f32(sittingHeight * 2) };
        const ctx = riddenState('camel', 0.5, 1, 0.5, { onGround: true, camelSitting: true, dimensions });
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0, zza: 1 });
        assert.equal(ctx.state.camelSitting, false);
        assert.equal(ctx.state.dimensions.height, f32(f32(2.375) * 2));
        assert.equal(ctx.state.dimensions.width, f32(f32(1.7) * 2));
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0, zza: 1 });
        assert.equal(calls[1].input.height, f32(f32(2.375) * 2));
        assert.equal(calls[1].output.height, f32(f32(2.375) * 2));
    });

    it('round-trips the kept box of a release that keeps it, at the size of the mob', () => {
        // the box moves and the position is read from it before 1.17 (1.16.5 Entity.java:608-609, 911); from 1.17 the box is
        // rebuilt (1.17 Entity.java:365-367), so the state keeps none. A box of other dimensions is a fixture, not gameplay
        const { registry, world } = worldOf('1.16.5', floor);
        const { calls, physics } = spied(registry);
        const dimensions = { width: f32(2.7929688), height: f32(3.2) };
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, vel: new Vec3(0, -0.0784, 0), dimensions });
        physics.simulateRidden(ctx, world, walking);
        assert.equal(calls[0].input.boundingBox, undefined);
        const kept = ctx.state.keptBoundingBox;
        assert.ok(Math.abs(kept.bb.maxX - kept.bb.minX - dimensions.width) < 1e-6);
        assert.ok(Math.abs(kept.bb.maxY - kept.bb.minY - dimensions.height) < 1e-6);
        assert.equal(kept.bb.minY, ctx.state.pos.y);
        physics.simulateRidden(ctx, world, walking);
        assert.deepEqual(calls[1].input.boundingBox.min, { x: kept.bb.minX, y: kept.bb.minY, z: kept.bb.minZ });
        // a mob moved by something else starts a box of its own
        ctx.state.pos.z += 1;
        physics.simulateRidden(ctx, world, walking);
        assert.equal(calls[2].input.boundingBox, undefined);
        const modern = worldOf('1.17.1', floor);
        const rebuilt = riddenState('horse', 0.5, 1, 0.5, { onGround: true, dimensions, keptBoundingBox: { bb: kept.bb, pos: new Vec3(0.5, 1, 0.5) } });
        new adapter.NativeRiddenMobPhysics(modern.registry).simulateRidden(rebuilt, modern.world, walking);
        assert.equal(rebuilt.state.keptBoundingBox, null);
    });

    it('syncs the world a scaled mob reaches', () => {
        // the box of the measured size, not of the type: a wall a plain horse is far from stops it
        const walled = ({ registry: r, set }) => {
            floor({ registry: r, set });
            const stone = r.blocksByName.stone.defaultState;
            for (let x = -4; x <= 4; x += 1) for (let y = 1; y <= 5; y += 1) set(x, y, 3, stone);
        };
        const { registry, world } = worldOf('1.21.1', walled);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const walk = dimensions => {
            const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, vel: new Vec3(0, -0.0784, 0), dimensions });
            for (let tick = 0; tick < 30; tick += 1) physics.simulateRidden(ctx, world, walking);
            return ctx.state.pos.z;
        };
        const width = f32(2.7929688);
        assert.ok(Math.abs(walk({ width, height: f32(3.2) }) - (3 - width / 2)) < 1e-6);
        assert.ok(Math.abs(walk({ width: f32(1.3964844), height: f32(1.6) }) - (3 - f32(1.3964844) / 2)) < 1e-6);
    });

    it('forwards the travel attributes of the release to the engine, by what they do', () => {
        // the effective values: STEP_HEIGHT and GRAVITY from 1.20.5, WATER_MOVEMENT_EFFICIENCY from 1.21, the modifiers from
        // 26.2 (26.3 LivingEntity.java:2382-2407, 2429-2441, 3974-3977)
        const version = '26.3';
        const { registry, world } = worldOf(version, floor);
        const { calls, physics } = spied(registry);
        const key = name => physics.attributeKey(name);
        const attributes = {
            [key('gravity')]: { value: -0.04 },
            [key('stepHeight')]: { value: 2.5 },
            [key('waterMovementEfficiency')]: { value: 0.5 },
            [key('airDragModifier')]: { value: 0.25 },
            [key('frictionModifier')]: { value: 2 },
        };
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, attributes });
        physics.simulateRidden(ctx, world, walking);
        const { input, output } = calls[0];
        assert.equal(input.gravity, -0.04);
        assert.equal(input.stepHeight, 2.5);
        assert.equal(input.waterMovementEfficiency, 0.5);
        assert.equal(input.airDragModifier, 0.25);
        assert.equal(input.frictionModifier, 2);
        assert.equal(output.airDragModifier, 0.25);
        assert.equal(output.frictionModifier, 2);
        // a friction of 1 - (1 - 0.6F) * 2 is 0.2F or so, so a plain speed and the air's own 0.91F modified by 0.25
        const friction = f32(f32(1) - f32(f32(f32(1) - f32(0.6)) * f32(2)));
        const airDrag = f32(f32(1) - f32(f32(f32(1) - f32(0.91)) * f32(0.25)));
        const step = f32(0.22499999403953552);
        assert.ok(Math.abs(ctx.state.pos.z - 0.5 - step) < 1e-6);
        assert.ok(Math.abs(ctx.state.vel.z - step * f32(friction * airDrag)) < 1e-6);
    });

    it('sanitizes a synced travel attribute as the attribute does, and leaves out what the release has not', () => {
        // RangedAttribute: NaN is the minimum, the rest clamped (26.3 RangedAttribute.java:29-31, Attributes.java:10, 72)
        const { registry, world } = worldOf('26.3', floor);
        const { calls, physics } = spied(registry);
        const attributes = {
            [physics.attributeKey('gravity')]: { value: NaN },
            [physics.attributeKey('airDragModifier')]: { value: 5000 },
        };
        physics.simulateRidden(riddenState('horse', 0.5, 5, 0.5, { attributes }), world, walking);
        assert.equal(calls[0].input.gravity, -1);
        assert.equal(calls[0].input.airDragModifier, 2048);
        const old = worldOf('1.20.4', floor);
        const before = spied(old.registry);
        before.physics.simulateRidden(riddenState('horse', 0.5, 5, 0.5), old.world, walking);
        assert.equal(before.calls[0].input.gravity, undefined);
        assert.equal(before.calls[0].input.stepHeight, undefined);
    });

    it('ends an effect with the tick it is gone from the state', () => {
        // the effects are read in the tick (26.3 LivingEntity.java:2337-2340, 2387-2397); the carried output does not keep one.
        // (1.21.9: from 1.21.5 a ridden mob's motion does not fade before its travel)
        const { registry, world } = worldOf('1.21.9', floor);
        const { calls, physics } = spied(registry);
        const ctx = riddenState('horse', 0.5, 50, 0.5, {
            effects: { [registry.effectsByName.Levitation.id]: { amplifier: 1 }, [registry.effectsByName.SlowFalling.id]: { amplifier: 0 } },
        });
        physics.simulateRidden(ctx, world, walking);
        assert.equal(calls[0].input.levitationAmplifier, 1);
        assert.equal(calls[0].input.slowFalling, true);
        const risen = ctx.state.vel.y;
        assert.ok(risen > 0);
        ctx.state.effects = {};
        physics.simulateRidden(ctx, world, walking);
        assert.equal(calls[1].input.levitationAmplifier, undefined);
        assert.equal(calls[1].input.slowFalling, false);
        assert.ok(Math.abs(ctx.state.vel.y - (risen - 0.08) * f32(0.98)) < 1e-9);
    });

    it('flies a happy ghast and swims a nautilus past the terms of ordinary travel', () => {
        // HappyGhast.travel is travelFlying, AbstractNautilus.travelInWater the speed and 0.9 (26.3 HappyGhast.java:154-157,
        // AbstractNautilus.java:203-208)
        const { registry, world } = worldOf('26.3', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const effects = { [registry.effectsByName.Levitation.id]: { amplifier: 3 } };
        const attributes = { [physics.attributeKey('gravity')]: { value: 0.5 }, [physics.attributeKey('airDragModifier')]: { value: 0 } };
        const flown = fields => {
            const ctx = riddenState('happy_ghast', 0.5, 20, 0.5, { vel: new Vec3(0, -0.1, 0), ...fields });
            physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
            return ctx.state.vel.y;
        };
        assert.equal(flown({}), -0.1 * 0.9100000262260437);
        assert.equal(flown({ effects, attributes }), flown({}));
    });

    it('animates the stand of a rearing horse, which seats its rider back and up', () => {
        const { registry, world } = worldOf('1.20.4', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, standing: true });
        const rider = { yawDegrees: 0, pitchDegrees: 0 };
        const rearing = anim => Math.min(f32(anim + f32(f32(f32(1 - anim) * f32(0.4)) + f32(0.05))), 1);
        physics.simulateRidden(ctx, world, rider);
        assert.equal(ctx.state.standAnimO, 0);
        assert.equal(ctx.state.standAnim, rearing(0));
        physics.simulateRidden(ctx, world, rider);
        assert.equal(ctx.state.standAnimO, rearing(0));
        assert.equal(ctx.state.standAnim, rearing(rearing(0)));
        ctx.state.standing = false;
        const settling = ctx.state.standAnim;
        physics.simulateRidden(ctx, world, rider);
        const cube = f32(f32(f32(f32(0.8) * settling) * settling) * settling);
        assert.equal(ctx.state.standAnim, f32(settling + f32(f32(f32(cube - settling) * f32(0.6)) - f32(0.05))));
        for (let tick = 0; tick < 10; tick += 1) physics.simulateRidden(ctx, world, rider);
        assert.equal(ctx.state.standAnim, 0);
        assert.equal(ctx.state.standAnimO, 0);
    });

    it('eases a ridden mob toward a server teleport over the steps left, or puts it there from afar', () => {
        const { registry, world } = worldOf('1.21.4', floor);
        const physics = new adapter.NativeRiddenMobPhysics(registry);
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true });
        const rider = { yawDegrees: 0, pitchDegrees: 0 };
        physics.teleportRidden(ctx.state, new Vec3(1.5, 1, 0.5));
        assert.deepEqual(physics.lerpTargetOf(ctx.state).toArray(), [1.5, 1, 0.5]);
        let x = 0.5;
        for (const steps of [3, 2, 1]) {
            physics.simulateRidden(ctx, world, rider);
            x += (1 / steps) * (1.5 - x);
            assert.equal(ctx.state.pos.x, x);
        }
        assert.equal(ctx.state.lerpSteps, 0);
        assert.deepEqual(physics.lerpTargetOf(ctx.state).toArray(), ctx.state.pos.toArray());
        physics.teleportRidden(ctx.state, new Vec3(100.5, 1, 0.5));
        assert.equal(ctx.state.pos.x, 100.5);
        assert.equal(ctx.state.lerpSteps, 0);
    });

    it('reports the rotation a ridden mob\'s interpolation heads to from 1.21.5', () => {
        const turned = version => {
            const physics = new adapter.NativeRiddenMobPhysics(require('prismarine-registry')(version));
            const { state } = riddenState('horse', 0.5, 1, 0.5, { yawDegrees: 10, pitchDegrees: 5 });
            physics.interpolateRidden(state, new Vec3(1.5, 1, 0.5), 3, { yaw: 90, pitch: 20 });
            return physics.lerpTargetRotationOf(state);
        };
        assert.deepEqual(turned('1.21.5'), { yaw: 90, pitch: 20 });
        assert.deepEqual(turned('1.21.4'), { yaw: 10, pitch: 5 });
    });

    it('gives a camel no riding jump while it sits, sits down or stands up', () => {
        // LocalPlayer.jumpableVehicle holds the camel only while Camel.canJump: !refuseToMove() && isSaddled()
        // (1.21.11 Camel.java:251-253, 272-274, LocalPlayer.java:563-564, 847-858; 1.19.3 CamelEntity.java:251-253)
        const physics = new adapter.NativeRiddenMobPhysics(require('prismarine-registry')('1.21.4'));
        const jumped = fields => {
            const { state } = riddenState('camel', 0.5, 1, 0.5, { onGround: true, ...fields });
            physics.riderJump(state, 90);
            return state.jumpPendingScale;
        };
        assert.equal(jumped({}), 1);
        // (after the camel's tick, whose step the pose time already holds: canJump reads the game time that tick read,
        // 1.21.11 LocalPlayer.java:847, Minecraft.java:1805, 1830, ClientLevel.java:323-324)
        assert.equal(jumped({ poseTime: 51 }), 0);
        assert.equal(jumped({ poseTime: 52 }), 0);
        assert.equal(jumped({ poseTime: 53 }), 1);
        assert.equal(jumped({ camelSitting: true, poseTime: 1000 }), 0);
        assert.equal(jumped({ saddled: false }), 0);
    });

    it('tells the engine a ridden mob is in an ultrawarm dimension', () => {
        // Entity's lava current is 0.007 where the dimension is ultrawarm (1.16 Entity.java:1049, 1.21.11 :1519)
        const { registry, world } = worldOf('1.21.4', floor);
        const binding = adapter.LoadNativePhysics();
        const sent = [];
        const spy = Object.create(binding);
        spy.simulateVersionedHorseTick = (version, w, input) => (sent.push(input.ultraWarm), binding.simulateVersionedHorseTick(version, w, input));
        const physics = new adapter.NativeRiddenMobPhysics(registry, { binding: spy });
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, ultraWarm: true });
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
        ctx.state.ultraWarm = false;
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
        assert.deepEqual(sent, [true, false]);
    });

    it('starts the 55-tick dash cooldown of a camel, the 40 of a nautilus, when the server syncs its DASH', () => {
        // onSyncedDataUpdated(DASH): dashCooldown = dashCooldown == 0 ? 55 : dashCooldown (1.19.3 CamelEntity.java:601-604,
        // 1.21.11 Camel.java:625-628, 26.3 Camel.java:580-583); the nautilus's 40 (26.3 AbstractNautilus.java:333-336)
        const physics = new adapter.NativeRiddenMobPhysics(require('prismarine-registry')('1.21.4'));
        const synced = (type, dashCooldown) => {
            const { state } = riddenState(type, 0.5, 1, 0.5, { dashCooldown });
            physics.syncDash(state, true);
            return [state.dashing, state.dashCooldown];
        };
        assert.deepEqual(synced('camel', 0), [true, 55]);
        assert.deepEqual(synced('camel', 12), [true, 12]);
        assert.deepEqual(synced('nautilus', 0), [true, 40]);
        assert.deepEqual(synced('horse', 0), [true, 0]);
    });

    it('tells the engine the Jump Boost of the mob its rider jumps', () => {
        // the riding jump adds the mob's own Jump Boost, 0.1F a level (1.21.11 AbstractHorse.java:767-768,
        // LivingEntity.java:2258-2264)
        const registry = require('prismarine-registry')('1.21.4');
        const { world } = worldOf('1.21.4', floor);
        const binding = adapter.LoadNativePhysics();
        const sent = [];
        const spy = Object.create(binding);
        spy.simulateVersionedHorseTick = (version, w, input) => (sent.push(input.jumpBoostAmplifier), binding.simulateVersionedHorseTick(version, w, input));
        const physics = new adapter.NativeRiddenMobPhysics(registry, { binding: spy });
        const id = registry.effectsByName.JumpBoost.id;
        const ctx = riddenState('horse', 0.5, 1, 0.5, { onGround: true, effects: { [id]: { id, amplifier: 1, duration: 100 } } });
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
        ctx.state.effects = {};
        physics.simulateRidden(ctx, world, { yawDegrees: 0, pitchDegrees: 0 });
        assert.deepEqual(sent, [1, undefined]);
    });
});

describe('BlockMapper', () => {
    it('gives a block before the flattening the shape of its data value, without variations', () => {
        // minecraft-data has no variations for these before 1.12, and prismarine-block looks a shape up in them alone
        for (const version of ['1.9.4', '1.10.2', '1.11.2', '1.12.2']) {
            const registry = require('prismarine-registry')(version);
            const Block = require('prismarine-block')(registry);
            const mapper = new adapter.BlockMapper(registry.version, registry);
            const boxes = (name, meta) => mapper.describe(Block.fromStateId(registry.blocksByName[name].id << 4 | meta, 0)).boxes;
            assert.deepStrictEqual(boxes('snow_layer', 3), [[0, 0, 0, 1, 0.375, 1]], version);
            assert.deepStrictEqual(boxes('ladder', 5), [[0, 0, 0, 0.1875, 1, 1]], version);
            assert.deepStrictEqual(boxes('trapdoor', 8), [[0, 0.8125, 0, 1, 1, 1]], version);
        }
    });

    const updatesOf = (version, name, props = {}, hasTag = undefined) => {
        const registry = require('prismarine-registry')(version);
        const Block = require('prismarine-block')(registry);
        const block = registry.blocksByName[name];
        const state = registry.version['>=']('1.13') ? Block.fromProperties(name, props, 0).stateId : block.id << 4;
        const described = adapter.describeBlock(Block.fromStateId(state, 0));
        const mapper = new adapter.BlockMapper(registry.version, registry, hasTag);
        return mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
    };

    it('gives the blocks before 1.13 the in-wall test of Entity.isInsideWall', () => {
        // 1.8.9 Entity.java:1304 Block.isLeafBlock (blocksMovement && renderAsNormalBlock, Block.java:330-332) and 1.12.2
        // Entity.java:1619 causesSuffocation (Block.java:323-325), not pushOutOfBlocks' isNormalCube: glowstone, ice
        // and a redstone block are walls; glass (GlassBlock.renderAsNormalBlock), leaves (LeavesBlock.isLeafBlock)
        // and a slab are not; soul sand, a lower box, is
        for (const version of ['1.8.8', '1.12.2']) {
            const inWall = name => updatesOf(version, name)[0]?.inWall;
            assert.deepEqual(
                ['glowstone', 'ice', 'redstone_block', 'stone', 'soul_sand', 'glass', 'leaves', 'stone_slab'].map(inWall),
                [true, true, true, true, true, false, false, false], version);
            assert.equal(updatesOf(version, 'glowstone')[0].suffocating, false);
        }
        assert.equal(updatesOf('1.21.11', 'glowstone')[0].inWall, undefined);
    });

    it('never takes the chorus flower for a wall or a suffocating block', () => {
        // A full collision cube until 1.20.3 whose material does not block movement: Material.PLANT under
        // `blocksMovement() && isFullCube` (1.9.4 Block.java:297-299 and ChorusFlowerBlock.java:27, 1.12.2 Block.java:323-325
        // and ChorusFlowerBlock.java:30, PlantMaterial.java:19-22; 1.13.2 Block.java:362-364, 2976; 1.16.5
        // AbstractBlock.java:795, Blocks.java:1824), notSolid from 1.20 (1.20 AbstractBlock.java:465-467, Blocks.java:4288)
        for (const version of ['1.9.4', '1.12.2']) {
            const [update] = updatesOf(version, 'chorus_flower');
            assert.equal(update.inWall, false, version);
            assert.equal(update.suffocating, false, version);
        }
        const [modern13] = updatesOf('1.13.2', 'chorus_flower', { age: 0 });
        assert.equal(modern13.suffocating, false);
        assert.equal(modern13.inWall, false);
        for (const version of ['1.14.4', '1.16.5', '1.19.4', '1.20.1']) {
            const [update] = updatesOf(version, 'chorus_flower', { age: 0 });
            assert.equal(update.suffocating, false, version);
            assert.equal(update.inWall, undefined, version);
        }
        // a stone block beside it stays both
        assert.equal(updatesOf('1.12.2', 'stone')[0].inWall, true);
        assert.equal(updatesOf('1.16.5', 'stone')[0].suffocating, undefined);
    });

    it('takes soul sand for suffocating and the beacon for not in 1.13', () => {
        // 1.13.2 Block.causesSuffocation: blocksMovement && isFullCube (Block.java:362-369), which SoulSandBlock keeps
        // and BeaconBlock answers false; 1.14's canSuffocate is a full collision cube (Block.java:324-326)
        assert.equal(updatesOf('1.13.2', 'soul_sand')[0].suffocating, true);
        assert.equal(updatesOf('1.13.2', 'beacon')[0].suffocating, false);
        assert.equal(updatesOf('1.13.2', 'soul_sand')[0].inWall, true);
        assert.equal(updatesOf('1.13.2', 'beacon')[0].inWall, false);
        assert.equal(updatesOf('1.14.4', 'soul_sand')[0].suffocating, undefined);
        assert.equal(updatesOf('1.14.4', 'beacon')[0].suffocating, undefined);
    });

    it("gives 1.13's pushOutOfBlocks the isNormalCube of the blocks, and isInsideWall their full cube", () => {
        // 1.13.2 EntityPlayerSP.java:386-444 and EntityPlayer.java:1454-1460 take a block for open unless it is a normal
        // cube: an opaque material, Block.isFullCube and no redstone power (Block.java:278-280). Ice and glowstone (a
        // material that is not opaque), the redstone block and the observer (canProvidePower), the piston base and the
        // shulker box (isFullCube false) are full collision cubes that are not
        const of = name => updatesOf('1.13.2', name, name === 'piston' ? { extended: false } : {})[0];
        for (const name of ['ice', 'glowstone', 'sea_lantern', 'tnt', 'redstone_block', 'observer', 'piston', 'white_shulker_box']) {
            assert.equal(of(name).suffocating, false, name);
            assert.equal(of(name).inWall, true, name);
        }
        assert.equal(of('stone').suffocating, undefined);
        assert.equal(of('stone').inWall, true);
        assert.equal(of('oak_slab').inWall, false);
        assert.equal(updatesOf('1.14.4', 'ice')[0].suffocating, undefined);
    });

    it('marks the fences, walls and gates Entity.getLandingPos looks through to', () => {
        // BlockTags.FENCES, BlockTags.WALLS, FenceGateBlock (1.17 Entity.java:686), an open gate with no box among them
        for (const name of ['oak_fence', 'nether_brick_fence', 'cobblestone_wall', 'oak_fence_gate']) {
            assert.equal(updatesOf('1.20.4', name)[0]?.tags & 16, 16, name);
        }
        // (a wall or gate also carries WALL_OR_GATE, which Entity.getOnPos stops at for any offset)
        assert.equal(updatesOf('1.20.4', 'oak_fence_gate', { open: 'true' })[0]?.tags, 48);
        assert.equal(updatesOf('1.20.4', 'cobblestone_wall')[0]?.tags, 48);
        // (and before 1.13 LEGACY_PROBE_OUTLINE, 64: its getCollisionBoundingBox is its outline, D-196)
        assert.equal(updatesOf('1.12.2', 'fence')[0]?.tags, 16 | 64);
        assert.equal(updatesOf('1.12.2', 'fence_gate')[0]?.tags, 48);
        for (const name of ['stone', 'oak_sign', 'oak_wall_sign', 'rail']) assert.equal((updatesOf('1.20.4', name)[0]?.tags ?? 0) & 16, 0, name);
        // (the bit is the fence's alone: minecart.rs reads 1, 2, 4 and 8 as rails and conductors)
        assert.equal(updatesOf('1.20.4', 'oak_fence')[0]?.tags, 16);
    });

    it('gives the fall-damage-resetting clip its tagged blocks, and the end portals for a player', () => {
        const binding = adapter.LoadNativePhysics();
        const updates = (version, name) => updatesOf(version, name, {}, (block, tag) => binding.generatedBlockHasTag(version, block, tag));
        // the 1.21.9 tag: #climbable, cobweb, sweet_berry_bush; END_GATEWAY and END_PORTAL full cubes for a player
        // (1.21.9 ClipContext.java:59-68), sent although they have no box
        for (const name of ['vine', 'ladder', 'scaffolding', 'cobweb', 'sweet_berry_bush']) {
            assert.equal(updates('1.21.9', name)[0]?.fallDamageResetting, true, name);
        }
        assert.equal(updates('1.21.9', 'stone')[0].fallDamageResetting, undefined);
        for (const name of ['end_portal', 'end_gateway']) {
            const [update] = updates('1.21.9', name);
            assert.equal(update?.playerFallDamageResetting, true, name);
            assert.deepEqual([update.shape.maxX, update.shape.maxY, update.shape.maxZ], [0, 0, 0], name);
        }
    });

    it('sends a block with no box and no effect, which BlockState.isAir does not take for air', () => {
        // BubbleColumnBlock.entityInside wants getBlockState(above).isAir() for the surface push before 1.21.5 (1.13.2
        // BlockBubbleColumn.java:40-41, 1.21.4 BubbleColumnBlock.java:48-49): a torch or a sign is not air (only the
        // Settings.air() blocks are, 1.16 Blocks.java:31, 2398-2399)
        for (const [name, props] of [['torch', {}], ['oak_sign', { waterlogged: false }]]) {
            const [update, ...rest] = updatesOf('1.21.4', name, props);
            assert.equal(rest.length, 0, name);
            assert.ok(update.id > 0, name);
            assert.deepEqual([update.shape.minX, update.shape.maxX, update.shape.maxY, update.shape.maxZ], [0, 0, 0, 0], name);
        }
        for (const name of ['air', 'cave_air', 'void_air']) assert.deepEqual(updatesOf('1.21.4', name), [], name);
    });

    it('marks the blocks whose fallOn changes the fall damage: hay and an upward dripstone tip', () => {
        // HayBlock.fallOn causeFallDamage(fd, 0.2F) (1.9.4 HayBlock.java:19-20, 1.21.4 :23-24); PointedDripstoneBlock.fallOn
        // only for VERTICAL_DIRECTION up and THICKNESS tip (1.17 PointedDripstoneBlock.java:128-133)
        // (the native BlockEffect codes: Hay 19, PointedDripstoneTip 20)
        const effect = (version, name, props) => updatesOf(version, name, props)[0].shape.effect;
        assert.equal(effect('1.12.2', 'hay_block'), 19);
        assert.equal(effect('1.21.1', 'hay_block', { axis: 'y' }), 19);
        const dripstone = (vertical_direction, thickness) => effect('1.21.1', 'pointed_dripstone', { vertical_direction, thickness, waterlogged: false });
        assert.equal(dripstone('up', 'tip'), 20);
        assert.equal(dripstone('down', 'tip'), 0);
        assert.equal(dripstone('up', 'frustum'), 0);
    });

    it('marks the water block, not a waterlogged block or a water plant, for the block speed factor', () => {
        // Entity.getBlockSpeedFactor leaves the block below unread in Blocks.WATER and Blocks.BUBBLE_COLUMN alone
        // (1.15 Entity.java:676-684, 1.21.11 :996-1004)
        assert.equal(updatesOf('1.21.11', 'water', { level: 0 })[0].waterBlock, true);
        assert.equal(updatesOf('1.21.11', 'water', { level: 3 })[0].waterBlock, true);
        assert.equal(updatesOf('1.21.11', 'seagrass')[0].waterBlock, undefined);
        assert.equal(updatesOf('1.21.11', 'kelp', { age: 0 })[0].waterBlock, undefined);
        assert.equal(updatesOf('1.21.11', 'oak_slab', { type: 'bottom', waterlogged: true })[0].waterBlock, undefined);
    });

    it('marks the honey block as suppressing the bounce', () => {
        // #suppresses_bounce holds the honey block (26.3 VanillaBlockTagsProvider.java:224)
        assert.equal(updatesOf('1.21.11', 'honey_block')[0].suppressesBounce, true);
        assert.equal(updatesOf('1.21.11', 'slime_block')[0].suppressesBounce, undefined);
    });

    it('marks the climbable blocks of #can_glide_through apart from the ladder', () => {
        // #can_glide_through: the vines, #cave_vines (1.21.11 data/minecraft/tags/block/can_glide_through.json)
        // (the native BlockEffect codes: Climbable 5, GlideThroughClimbable 21)
        const effect = (name, props) => updatesOf('1.21.11', name, props)[0].shape.effect;
        assert.equal(effect('vine', { east: false, north: true, south: false, up: false, west: false }), 21);
        assert.equal(effect('twisting_vines', { age: 0 }), 21);
        assert.equal(effect('cave_vines_plant', { berries: false }), 21);
        assert.equal(effect('ladder', { facing: 'north', waterlogged: false }), 5);
    });

    it('takes the soils for suffocating from 1.16.2', () => {
        // AbstractBlock.Settings.suffocates(Blocks::always) on farmland, the grass path and soul sand arrives in 1.16.2
        // (1.16.5 Blocks.java:1843-1846; 1.16 Blocks.java:503-504, 1833 give them blockVision alone)
        for (const name of ['farmland', 'grass_path', 'soul_sand']) {
            assert.equal(updatesOf('1.16.1', name)[0].suffocating, undefined, name);
            assert.equal(updatesOf('1.16.2', name)[0].suffocating, true, name);
        }
    });

    it('marks the lava cauldron and the water and powder snow cauldrons by level', () => {
        // LavaCauldronBlock and LayeredCauldronBlock.entityInside: the lava sets on fire, the water and powder snow put
        // out, in a content (6 + 3 * level) / 16 high (1.17 LeveledCauldronBlock.java:49-56, 1.21.9
        // LayeredCauldronBlock.java:68-93) (the native BlockEffect codes: LavaCauldron 24, LayeredCauldron 25 - 27)
        const effect = (name, props) => updatesOf('1.21.11', name, props)[0].shape.effect;
        assert.equal(effect('lava_cauldron'), 24);
        assert.equal(effect('water_cauldron', { level: 1 }), 25);
        assert.equal(effect('powder_snow_cauldron', { level: 3 }), 27);
        assert.equal(effect('cauldron'), 0);
    });

    it('marks the lily pad and the 1.8 snow layer for the boat', () => {
        // a boat leaves a lily pad out of its ground friction (1.21.11 AbstractBoat.java:453), which gives it no box before
        // 1.13 (1.12.2 LilyPadBlock.java:22-26), and 1.8's boat clears snow layers (1.8.9 BoatEntity.java:298), not the snow
        // block ('snow' before the flattening)
        const effect = (version, name) => updatesOf(version, name)[0].shape.effect;
        assert.equal(effect('1.8.8', 'waterlily'), 22);
        assert.equal(effect('1.12.2', 'waterlily'), 22);
        assert.equal(effect('1.21.11', 'lily_pad'), 22);
        assert.equal(effect('1.8.8', 'snow_layer'), 23);
        assert.equal(effect('1.8.8', 'snow'), 0);
    });
});

describe('the entities through fluids', () => {
    const binding = adapter.LoadNativePhysics();
    const support = (version, type) => binding.entityTravelSupport(version, type);
    // what the game has that no path here moves: WaterAnimal and its fish, squid and dolphin, the tadpole (26.3
    // entity/animal/fish, squid, dolphin, frog), the axolotl, turtle, frog, guardians and the drowned, each with its own
    // move control or travel
    const travelling = ['squid', 'glow_squid', 'guardian', 'elder_guardian', 'cod', 'salmon', 'pufferfish', 'tropical_fish',
        'ghast', 'phantom', 'allay', 'vex', 'dolphin', 'turtle', 'axolotl', 'tadpole', 'frog', 'drowned', 'blaze', 'bat', 'bee',
        'parrot', 'spider', 'polar_bear', 'wither', 'armor_stand', 'sulfur_cube'];
    const ridden = ['pig', 'horse', 'donkey', 'mule', 'skeleton_horse', 'zombie_horse', 'llama', 'trader_llama', 'strider', 'camel',
        'camel_husk', 'happy_ghast', 'nautilus', 'zombie_nautilus'];
    const boats = ['boat', 'chest_boat', 'oak_boat', 'spruce_chest_boat', 'pale_oak_boat', 'bamboo_raft', 'bamboo_chest_raft'];

    it('sorts every kind into its family', () => {
        for (const type of travelling) assert.equal(support('26.3', type).family, 'mob', type);
        for (const type of ridden) assert.equal(support('26.3', type).family, 'ridden', type);
        for (const type of boats) assert.equal(support('26.3', type).family, 'boat', type);
        assert.equal(support('26.3', 'player').family, 'player');
        assert.equal(support('26.3', 'minecart').family, 'entity');
        for (const version of ['1.8.8', '1.16.5', '26.3']) assert.equal(support(version, 'ender_dragon').family, 'dragon', version);
        for (const version of ['1.9.4', '1.16.5', '26.3']) assert.equal(support(version, 'shulker').family, 'unsupported', version);
        assert.equal(support('26.3', 'no_such_kind').family, 'unknown');
    });

    it('names every kind of every release: nothing the game has goes as unknown', () => {
        const entityData = require('../generated/entity-data.json').releases;
        for (const version of require('../scripts/release-inventory.cjs').canonicalReleases()) {
            // (a client's own dump where there is one, the registries for the legacy six)
            const names = entityData[version]?.entities ? Object.keys(entityData[version].entities) : Object.keys(require('prismarine-registry')(version).entitiesByName);
            // (Mob and Monster are EntityList's abstract classes of 1.8 to 1.10: nothing spawns as them)
            const wrong = names.filter(name => { if (name === 'Mob' || name === 'Monster') return false; const found = support(version, name); return found.family === 'unknown' || !found.available; });
            assert.deepEqual(wrong, [], version);
        }
    });

    it('knows the releases from their registries, on both sides of each first release', () => {
        // the names the registries use from 1.11; before, the horses and the guardians are variants of one type
        const has = (version, name) => !!require('prismarine-registry')(version).entitiesByName[name];
        for (const [before, after, names] of [
            ['1.12.2', '1.13.2', ['cod', 'salmon', 'pufferfish', 'tropical_fish', 'dolphin', 'turtle', 'drowned']],
            ['1.13.2', '1.14.4', ['trader_llama']],
            ['1.15', '1.16', ['strider']],
            ['1.16.5', '1.17', ['glow_squid', 'axolotl']],
            ['1.18.1', '1.19', ['tadpole', 'frog', 'chest_boat']],
            ['1.19.2', '1.19.3', ['camel']],
            ['1.21.5', '1.21.6', ['happy_ghast']],
            ['1.21.9', '1.21.11', ['camel_husk', 'nautilus', 'zombie_nautilus']],
            ['1.21.1', '1.21.3', ['oak_boat', 'pale_oak_chest_boat', 'bamboo_raft']],
        ]) {
            for (const name of names) {
                assert.deepEqual([has(before, name), has(after, name)], [false, true], `${name} in the registries`);
                assert.deepEqual([support(before, name).available, support(after, name).available], [false, true], `${name} in the engine`);
            }
        }
        // the single boat goes at 1.21.2 (client1.21.1 and client1.21.2 EntityType.java)
        assert.deepEqual([has('1.21.1', 'boat'), has('1.21.3', 'boat')], [true, false]);
        assert.deepEqual([support('1.21.1', 'boat').available, support('1.21.2', 'boat').available], [true, false]);
        for (const name of ['elder_guardian', 'donkey', 'llama', 'squid', 'guardian']) assert.equal(has('1.11.2', name), true, name);
    });

    it('refuses a fish for the player\'s tick on every entry point, and leaves the world and the state alone', () => {
        const { registry, world } = worldOf('26.3', floor);
        const physics = new adapter.NativePhysics(registry);
        const native = physics.syncOf(world).world;
        const state = entityType => ({
            position: { x: 0.5, y: 5, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: false,
            collidedHorizontally: false, collidedVertically: false, isPlayer: false, entityType,
        });
        for (const type of ['cod', 'tropical_fish', 'squid', 'axolotl']) {
            assert.throws(() => binding.simulateVersionedTick('26.3', native, state(type), {}), /no travel simulation/, type);
            assert.throws(() => binding.simulateVersionedTicks('26.3', native, [state(type)], 1, {}), /no travel simulation/, type);
            assert.throws(() => new binding.NativePlayer('26.3', state(type), {}), /no travel simulation/, type);
            assert.throws(() => binding.simulateVersionedEntityTicks('26.3', native, [{ simulation: state(type), noPhysics: false, pushable: true, pushthrough: 0 }], 1, {}), /no travel simulation/, type);
        }
        assert.throws(() => binding.simulateVersionedTick('26.3', native, state('strider'), {}), /simulateVersionedHorseTick/);
        assert.throws(() => binding.simulateVersionedTick('26.3', native, state('oak_boat'), {}), /simulateVersionedBoatTick/);
        assert.throws(() => binding.simulateVersionedTick('26.3', native, state('minecart'), {}), /inventory/);
        assert.throws(() => binding.simulateVersionedTick('26.3', native, state('no_such_kind'), {}), /inventory/);
        // a named player and a state that names nothing go on as they did
        const named = binding.simulateVersionedTick('26.3', native, state('player'), {});
        const unnamed = binding.simulateVersionedTick('26.3', native, state(undefined), {});
        assert.deepEqual(named.velocity, unnamed.velocity);
    });

    it('refuses a ridden tick for a fish, and for a mob its release does not have', () => {
        const ride = (version, type) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativeRiddenMobPhysics(registry);
            const ctx = riddenState(type, 0.5, 1, 0.5, { onGround: true });
            const before = ctx.state.pos.clone();
            try {
                physics.simulateRidden(ctx, world, {});
            } catch (error) {
                assert.deepEqual(ctx.state.pos, before);
                return error.message;
            }
            return undefined;
        };
        assert.match(ride('26.3', 'cod'), /no ridden mob simulation for cod on 26.3 \(mob\)/);
        assert.match(ride('26.3', 'oak_boat'), /\(boat\)/);
        assert.match(ride('1.15', 'strider'), /no ridden mob simulation for strider on 1.15/);
        assert.equal(ride('1.16.5', 'strider'), undefined);
        assert.equal(ride('26.3', 'nautilus'), undefined);
    });
});

describe('NativeMovingEntityPhysics', () => {
    const stoneFloor = release => worldOf(release, ({ registry, set }) => {
        for (let x = -4; x <= 4; x += 1) for (let z = -4; z <= 4; z += 1) set(x, 63, z, registry.blocksByName.stone.defaultState ?? 16);
    });

    it('names the entities as the releases do, and leaves out the others', () => {
        assert.equal(adapter.movingEntityKind('tnt'), 'tnt');
        assert.equal(adapter.movingEntityKind('minecraft:falling_block'), 'falling_block');
        assert.equal(adapter.movingEntityKind('XPOrb'), 'experience_orb');
        assert.equal(adapter.movingEntityKind('zombie'), undefined);
    });

    it('predicts a primed TNT, whose end is the client\'s discard and no explosion', () => {
        const { registry, world } = stoneFloor('1.20.4');
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        const state = physics.track({ id: 5, name: 'tnt', position: new Vec3(0.5, 66, 0.5), velocity: new Vec3(0, 0, 0) }, { fuse: 2 });
        assert.equal(state.kind, 'tnt');
        const first = physics.tick(5, world);
        assert.equal(first.state.position.y, 66 - 0.04);
        assert.equal(first.state.fuse, 1);
        const last = physics.tick(5, world);
        assert.equal(last.state.removed, true);
        assert.equal(last.explodes, false);
        assert.equal(physics.state(5), undefined);
    });

    it('takes a landed block no further than the ground, and leaves the block to the server', () => {
        const { registry, world } = stoneFloor('1.20.4');
        const before = world.getBlock(new Vec3(0, 64, 0)).type;
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        physics.track({ id: 6, name: 'falling_block', position: new Vec3(0.5, 64, 0.5), velocity: new Vec3(0, -0.1, 0) }, { carriedBlock: 12, carriedName: 'sand' });
        const landed = physics.tick(6, world);
        assert.equal(landed.state.onGround, true);
        assert.equal(landed.state.removed, false);
        assert.equal(landed.landedAt ?? null, null);
        assert.equal(world.getBlock(new Vec3(0, 64, 0)).type, before);
    });

    it('takes the server\'s word for where the entity is, and keeps what its tick keeps', () => {
        const { registry, world } = stoneFloor('1.20.4');
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        physics.track({ id: 7, name: 'item', position: new Vec3(0.5, 70, 0.5), velocity: new Vec3(0, 0, 0) });
        physics.tick(7, world);
        assert.equal(physics.state(7).age, 1);
        physics.serverUpdate(7, { position: { x: 1.5, y: 68, z: 1.5 }, velocity: { x: 0, y: 0, z: 0 } });
        const next = physics.tick(7, world);
        assert.equal(next.state.position.x, 1.5);
        assert.equal(next.state.position.y, 68 - 0.04);
        assert.equal(next.state.age, 2);
    });

    it('pulls an orb toward the player it is given', () => {
        const { registry, world } = stoneFloor('1.20.4');
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        physics.track({ id: 8, name: 'experience_orb', position: new Vec3(0.5, 66, 0.5), velocity: new Vec3(0, 0, 0) });
        const result = physics.tick(8, world, { players: [{ id: 1, position: { x: 3.5, y: 64, z: 0.5 }, eyeHeight: 1.62 }] });
        assert.equal(result.state.followingPlayer, 1);
        assert.ok(result.state.velocity.x > 0);
    });

    it('moves a mob by the control and motion it is given', () => {
        const { registry, world } = stoneFloor('1.20.4');
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        const moved = physics.travelMob({
            kind: 'squid', position: { x: 0.5, y: 70, z: 0.5 }, velocity: { x: 0.25, y: 0, z: 0 }, yawDegrees: 0,
            input: { x: 0, y: 0, z: 1 }, speed: 0.1, halfWidth: 0.4, height: 0.8,
        }, world);
        assert.equal(moved.position.x, 0.75);
        assert.equal(moved.kind, 'squid');
        assert.throws(() => physics.travelMob({ ...moved, kind: 'minecart' }, world), /no travel of its own/);
        // (EnderDragon.aiStep never calls travel, 26.3 EnderDragon.java:146-231; a shulker holds no motion, 26.3
        // Shulker.java:295-300, and before that its tick puts it back on its block, 1.16.5 Shulker.java:257)
        assert.throws(() => physics.travelMob({ ...moved, kind: 'ender_dragon' }, world), /simulateVersionedDragonTick/);
        assert.throws(() => physics.travelMob({ ...moved, kind: 'shulker' }, world), /no travel simulation for shulker/);
        assert.equal(physics.travelMob({ ...moved, kind: 'creeper' }, world).kind, 'creeper');
    });

    it('flies a dragon from what its phase decided, and gives its parts to the wings and to arrows', () => {
        const { registry } = stoneFloor('26.3');
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        const { state, parts, partsFollowDragon } = physics.tickDragon({
            position: { x: 0.5, y: 70, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yawDegrees: 0,
            target: { x: 0.5, y: 80, z: 40 }, flySpeed: 0.6, turnSpeed: 0.7,
        });
        assert.notEqual(state.position.z, 0.5);
        assert.equal(state.history.length, 64);
        assert.deepEqual(parts.map(part => part.name), ['head', 'neck', 'body', 'tail', 'tail', 'tail', 'wing', 'wing']);
        assert.equal(parts[7].idOffset, 8);
        assert.equal(partsFollowDragon, false);
        const at = { x: parts[6].position.x, y: parts[6].position.y - 2, z: parts[6].position.z };
        const box = { min: at, max: { x: at.x + 0.6, y: at.y + 1.8, z: at.z + 0.6 } };
        const pushes = physics.dragonWingPushes(parts, [{ id: 9, position: at, boundingBox: box }]);
        assert.equal(pushes[0].id, 9);
        assert.equal(pushes[0].impulse.y, Math.fround(0.2));
    });

    it('rejects a kind and a draw the engine cannot take', () => {
        const { registry, world } = stoneFloor('1.20.4');
        const physics = new adapter.NativeMovingEntityPhysics(registry);
        const sync = physics.syncOf(world);
        assert.throws(() => physics.binding.simulateVersionedMovingEntityTick('1.20.4', sync.world, { kind: 'cow', position: { x: 0, y: 0, z: 0 }, velocity: { x: 0, y: 0, z: 0 } }), /unknown moving entity kind/);
        assert.throws(() => physics.binding.simulateVersionedMovingEntityTick('1.20.4', sync.world, { kind: 'item', position: { x: 0, y: 0, z: 0 }, velocity: { x: 0, y: 0, z: 0 } }, { random: [1] }), /random draw/);
    });
});

describe('NativeBoatPhysics', () => {
    it('lets a piston push the boat and a ridden mob as it does the player, up to the limit of a tick', () => {
        // every entity of the level is asked (26.3 PistonMovingBlockEntity.java:110-150), and 0.51 a tick is one for all
        // of its pushes (Entity.limitPistonMovement); a boat's checkFallDamage keeps the motion, as lastYd
        const { registry, world } = worldOf('26.3', floor);
        const boats = new adapter.NativeBoatPhysics(registry);
        const pos = new Vec3(1.2, 1, 0.5);
        const state = {
            age: 3, pos, vel: new Vec3(0, 0, 0), onGround: true, isCollidedHorizontally: false, isCollidedVertically: true,
            halfWidth: 0.6875, height: 0.5625, lastVerticalVelocity: -0.04,
        };
        boats.carries.set(state, { position: { x: 1.2, y: 1, z: 0.5 } });
        const piston = { pos: [1, 1, 0], direction: [1, 0, 0], extending: true, source: false, progress: 0, shapes: [[0, 0, 0, 1, 1, 1]] };
        boats.pushByMovingPiston({ state }, world, piston);
        assert.equal(state.pos.x, 1.2 + 0.51);
        assert.equal(state.lastVerticalVelocity, 0);
        boats.pushByMovingPiston({ state }, world, { ...piston, pos: [2, 1, 0] });
        assert.equal(state.pos.x, 1.2 + 0.51);
        state.age += 1;
        boats.pushByMovingPiston({ state }, world, { ...piston, pos: [2, 1, 0], progress: 0 });
        assert.ok(state.pos.x > 1.2 + 0.51);
        const mobs = new adapter.NativeRiddenMobPhysics(registry);
        const mob = {
            age: 0, type: 'horse', pos: new Vec3(1.2, 1, 0.5), vel: new Vec3(0, 0, 0), onGround: true,
            isCollidedHorizontally: false, isCollidedVertically: true, dimensions: { width: 1.3964844, height: 1.6 }, attributes: {},
        };
        mobs.pushByMovingPiston({ state: mob }, world, piston);
        assert.ok(mob.pos.x > 1.2);
        assert.equal(mob.fallDistance, 0);
    });

    it("imports the boat's no-gravity flag, its own or its entity's synced one, from 1.10", () => {
        // Boat.floatBoat: hasNoGravity() ? 0.0 : -0.04F (1.10.2 BoatEntity.java:499), getFinalGravity() from 1.20.5
        // (1.20.5 BoatEntity.java:582)
        const fell = (version, fields) => {
            const { registry, world } = worldOf(version, () => {});
            const physics = new adapter.NativeBoatPhysics(registry);
            const state = {
                age: 0, pos: new Vec3(0.5, 10, 0.5), vel: new Vec3(0, 0, 0), onGround: false, isInWater: false,
                isCollidedHorizontally: false, isCollidedVertically: false, control: controls(), status: 4,
                previousStatus: 4, waterLevel: NaN, landFriction: 0, yawVelocity: 0, yaw: 0, yawDegrees: 0,
                lastVerticalVelocity: 0, controllingPlayer: true, speedMultiplier: 0.07, worldReady: true,
                halfWidth: 0.6875, height: f32(0.5625),
                getBB() {
                    return { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 };
                },
                ...fields,
            };
            physics.simulate({ state }, world);
            return state.vel.y;
        };
        assert.equal(fell('1.21.4', { noGravity: true }), 0);
        assert.equal(fell('1.16.5', { entity: { metadata: [0, 300, '', false, false, true] } }), 0);
        assert.equal(fell('1.21.4', {}), -0.04);
        assert.equal(fell('1.16.5', {}), f32(-0.04));
    });

    it('takes the bubble column drag and launch the server announces from 26.3', () => {
        // AbstractBoat.handleEntityEvent 71 and 72: handleBubbleColumnEffect adds -0.7 to the vertical motion or sets it
        // to 0.6 (26.3 AbstractBoat.java:316-335); before 26.3 the server alone runs it (26.2 AbstractBoat.java:274-310)
        const handled = (release, dragDown) => {
            const physics = new adapter.NativeBoatPhysics(require('prismarine-registry')(release));
            const ctx = { state: { vel: new Vec3(0.1, -0.2, 0.3) } };
            physics.handleBubbleColumnEffect(ctx, dragDown);
            return ctx.state.vel.toArray();
        };
        assert.deepEqual(handled('26.3', true), [0.1, -0.2 + -0.7, 0.3]);
        assert.deepEqual(handled('26.3', false), [0.1, 0.6, 0.3]);
        assert.deepEqual(handled('26.2', true), [0.1, -0.2, 0.3]);
        assert.deepEqual(handled('26.2', false), [0.1, -0.2, 0.3]);
    });
});

describe('falls, jumps, poses and borders through each surface', () => {
    /** A fall or a held jump through the adapter, the object API reseeded from a clone every tick beside it. */
    function bothWays(version, place, fields, ticks, each) {
        const { registry, world } = worldOf(version, place);
        const physics = new adapter.NativePhysics(registry);
        const binding = adapter.LoadNativePhysics();
        const ctx = playerState(0.5, 1, 0.5, fields);
        const { state } = ctx;
        let object;
        for (let tick = 0; tick < ticks; tick += 1) {
            physics.simulate(ctx, world);
            const packed = physics.carries.get(state).player.state();
            object = object === undefined
                ? packed
                : binding.simulateVersionedTick(version, physics.syncOf(world).world, structuredClone(object), {});
            const at = `${version} tick ${tick}`;
            assert.ok(Object.is(object.fallDistance, packed.fallDistance), `${at}: ${object.fallDistance} ${packed.fallDistance}`);
            assert.deepEqual(
                [object.position, object.velocity, object.jumpTicks, object.onGround, object.fallDamage],
                [packed.position, packed.velocity, packed.jumpTicks, packed.onGround, state.fallDamage], at);
            each(tick, state, at);
        }
    }

    const bed = ({ registry, Block, set }) => {
        floor({ registry, set });
        set(0, 1, 0, Block.fromProperties('white_bed', { part: 'foot', facing: 'east', occupied: false }, 0).stateId);
    };

    it('keeps one fall history through clones of the state, and reports the landing on its tick alone', () => {
        // Entity.checkFallDamage -> LivingEntity.causeFallDamage on the landing tick (1.20.4 Entity.java:1116-1130,
        // 26.3 :1596-1610): 5 blocks deal 2 by ceil before 1.21.5 and by floor with the epsilon after (1.20.4
        // LivingEntity.java:1564, 26.3 :1783-1790)
        for (const version of ['1.18.1', '1.20.4', '26.3']) {
            const damages = [];
            bothWays(version, floor, { pos: new Vec3(0.5, 6, 0.5) }, 30, (tick, state) => {
                damages.push(state.fallDamage);
            });
            assert.deepEqual(damages.filter(d => d !== 0), [2], version);
        }
    });

    it('reports the landing on a bed once and nothing in the air of the bounce', () => {
        // BedBlock halves the distance and bounces (updateEntityMovementAfterFallOn); the next tick is in the air
        for (const version of ['1.20.4', '26.3']) {
            let landed = -1;
            bothWays(version, bed, { pos: new Vec3(0.5, 12, 0.5) }, 40, (tick, state, at) => {
                if (landed < 0 && state.fallDamage > 0) {
                    landed = tick;
                    assert.ok(state.vel.y > 0, at);
                } else {
                    assert.equal(state.fallDamage, 0, at);
                }
            });
            assert.ok(landed > 0, version);
        }
    });

    it('holds fall damage off a player that may fly', () => {
        // Player.causeFallDamage deals nothing with mayfly (1.8.9 PlayerEntity.java:1387-1388, 1.21.5 Player.java:1470-1472);
        // the fall still counts and ends
        for (const version of ['1.20.4', '26.3']) {
            let most = 0;
            bothWays(version, floor, { pos: new Vec3(0.5, 8, 0.5), mayFly: true }, 30, (tick, state, at) => {
                assert.equal(state.fallDamage, 0, at);
                most = Math.max(most, state.fallDistance);
            });
            assert.ok(most > 6, version);
        }
    });

    it('traces a held jump alike through the adapter and the object API', () => {
        // noJumpDelay: 10 at the launch, counted down at the top of aiStep (1.8.9 LivingEntity.java:1405-1406, 1456-1458;
        // 26.3 :2959-2960, 3024-3026)
        for (const version of ['1.8.8', '1.20.4', '26.3']) {
            const keys = controls({ jump: true });
            const delays = [];
            bothWays(version, floor, { onGround: true, vel: new Vec3(0, (0 - 0.08) * f32(0.98), 0), control: keys, prevControl: keys }, 12, (tick, state) => {
                delays.push(state.jumpTicks);
            });
            assert.deepEqual(delays.slice(0, 11), [10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0], version);
        }
    });

    it('fits the pose among the colliders and pushes with the box of the new pose', () => {
        // es_slab_under_shulker: on a bottom slab (feet at 1.5) under a shulker at 3, Player.tick's pose update crouches the
        // player (1.5 tall, 3.0 at the top) before the shulker ticks (26.2 Player.java:266), and noCollision sees the shulker
        // (26.3 Player.java:364-366, CollisionGetter.java:52-54, 70-72)
        const slabbed = version => worldOf(version, ({ registry: r, set }) => {
            floor({ registry: r, set });
            const bottom = r.blocksByName.smooth_stone_slab.defaultState;
            for (let x = -1; x <= 4; x += 1) for (let z = -1; z <= 2; z += 1) set(x, 1, z, bottom);
        });
        const shulker = (minY, fields = {}) => {
            const boundingBox = { minX: 0, minY, minZ: 0, maxX: 1, maxY: minY + 1, maxZ: 1 };
            return { id: 5, name: 'shulker', living: true, position: new Vec3(0.5, minY, 0.5), boundingBox, ...fields };
        };
        const tick = (version, entities, fields = {}) => {
            const { registry, world } = slabbed(version);
            const physics = new adapter.NativePhysics(registry);
            const ctx = playerState(0.5, 1.5, 0.5, { onGround: true, pose: adapter.Pose.SNEAKING, nearbyEntities: entities, ...fields });
            physics.simulate(ctx, world);
            return ctx.state;
        };
        for (const version of ['1.16.5', '1.21.1', '26.2', '26.3']) {
            const alone = tick(version, []);
            assert.equal(alone.pose, adapter.Pose.STANDING, version);
            // the standing box (3.3) meets it, the crouching one (3.0) only touches it: no push, the same tick as alone
            const under = tick(version, [shulker(3)]);
            assert.equal(under.pose, adapter.Pose.SNEAKING, version);
            assert.deepEqual(under.pushedEntities, [], version);
            assert.deepEqual([under.pos, under.vel], [alone.pos, alone.vel], version);
            // just over the standing box it stays out of the way
            const over = tick(version, [shulker(1.5 + f32(1.8) + 1e-3)]);
            assert.equal(over.pose, adapter.Pose.STANDING, version);
            assert.deepEqual(over.pushedEntities, [], version);
            // into the crouching box: down to swimming (2.1), which it misses
            const low = tick(version, [shulker(2.9)]);
            assert.equal(low.pose, adapter.Pose.SWIMMING, version);
            assert.deepEqual(low.pushedEntities, [], version);
            // into the swimming box: no pose fits, the pose stays, and its box takes the push
            const lower = tick(version, [shulker(2, { position: new Vec3(0.6, 2, 0.5) })]);
            assert.equal(lower.pose, adapter.Pose.SNEAKING, version);
            assert.equal(lower.pushedEntities.length, 1, version);
        }
        // a spectator or a passenger takes the pose it wants whatever the colliders (26.3 Player.java:336)
        assert.equal(tick('26.3', [shulker(3)], { passenger: true }).pose, adapter.Pose.STANDING);
    });

    it('walks onto the slab under the shulker as es_slab_under_shulker recorded', () => {
        // the floor at y 0 and the recording's y 100 at feet level y 1: slabs from x 3, the shulker's box at x 3 - 4, 3 - 4 high.
        // Up to 26.2 the step takes the player onto the slab on tick 11, whose pose update crouches it; from then on
        // the server sets it back (recordings 1.20.4, 1.21.1, 26.2). In 26.3 the step meets the shulker: it stops at the slab
        const walk = (version, ticks) => {
            const { registry, world } = worldOf(version, ({ registry: r, set }) => {
                floor({ registry: r, set });
                const bottom = r.blocksByName.stone_slab.defaultState;
                for (let x = 3; x <= 4; x += 1) for (let z = -1; z <= 1; z += 1) set(x, 1, z, bottom);
            });
            const physics = new adapter.NativePhysics(registry);
            const entity = {
                id: 5, name: 'shulker', living: true, position: new Vec3(3.5, 3, 0.5),
                boundingBox: { minX: 3, minY: 3, minZ: 0, maxX: 4, maxY: 4, maxZ: 1 },
            };
            const ctx = playerState(0.5, 1, 0.5, {
                onGround: true, vel: new Vec3(0, -0.0784000015258789, 0), yawDegrees: 270, nearbyEntities: [entity],
                control: controls({ forward: true }),
            });
            const pushes = [];
            for (let tick = 0; tick < ticks; tick += 1) {
                physics.simulate(ctx, world);
                pushes.push(...ctx.state.pushedEntities);
            }
            assert.deepEqual(pushes, [], version);
            return ctx.state;
        };
        for (const version of ['1.20.4', '1.21.1', '26.2']) {
            const state = walk(version, 12);
            assert.deepEqual([state.pos.x, state.pos.y], [2.830889549636968, 1.5], version);
            assert.equal(state.pose, adapter.Pose.SNEAKING, version);
            assert.equal(state.vel.x, 0.11777633270024773, version);
        }
        const state = walk('26.3', 30);
        assert.deepEqual([state.pos.x, state.pos.y], [2.699999988079071, 1]);
        assert.equal(state.pose, adapter.Pose.STANDING);
    });

    it('pushes with the old box before the player and the new one after it', () => {
        // the entities the client ticks before the player push before its pose update, those after it after (26.2
        // Player.java:266; a hash map of entities on 1.14 - 1.16)
        const { registry, world } = worldOf('1.16.5', ({ registry: r, set }) => {
            floor({ registry: r, set });
        });
        const physics = new adapter.NativePhysics(registry);
        const pushed = ticksBeforePlayer => {
            const boundingBox = { minX: 0.1, minY: 2.6, minZ: 0, maxX: 1.1, maxY: 3.6, maxZ: 1 };
            const entity = { id: 5, name: 'shulker', living: true, position: new Vec3(0.6, 2.6, 0.5), boundingBox, ticksBeforePlayer };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [entity] });
            physics.simulate(ctx, world);
            assert.equal(ctx.state.pose, adapter.Pose.SNEAKING);
            return ctx.state.pushedEntities.length;
        };
        assert.equal(pushed(true), 1);
        assert.equal(pushed(false), 0);
    });

    it('shrinks a pose by 1e-7 only from 1.16 and grows the entity query by 1e-7 from 1.14.4', () => {
        // wouldPoseNotCollide: the box itself to 1.15 (1.15 Entity.java:1657-1659), contract(1.0E-7) from 1.16 (1.16 :1695-1697);
        // the entities meet the box grown by 1e-7 (client1.14.4 EntityGetter.java:58, 1.16.5 EntityView.java:64)
        const standsUnder = (version, minY) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const boundingBox = { minX: 0, minY, minZ: 0, maxX: 1, maxY: minY + 1, maxZ: 1 };
            const entity = { id: 5, name: 'shulker', living: true, position: new Vec3(0.5, minY, 0.5), boundingBox };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [entity] });
            physics.simulate(ctx, world);
            return ctx.state.pose === adapter.Pose.STANDING;
        };
        const top = 1 + f32(1.8);
        for (const version of ['1.14.4', '1.15']) {
            assert.equal(standsUnder(version, top + 5e-8), false, version);
            assert.equal(standsUnder(version, top + 2e-7), true, version);
        }
        for (const version of ['1.16', '1.21.1', '26.3']) {
            assert.equal(standsUnder(version, top + 5e-8), true, version);
            assert.equal(standsUnder(version, top - 2e-7), false, version);
        }
    });

    it('crouches under a collider without the sneak key, from 1.14', () => {
        // LocalPlayer.aiStep: crouching when the crouching pose fits and the standing one does not (26.3 LocalPlayer.java:764),
        // through canPlayerFitWithinBlocksAndEntitiesWhen, which sees entities
        const crouching = (version, minY, name = 'shulker', fields = {}) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const boundingBox = { minX: -1, minY, minZ: -1, maxX: 2, maxY: minY + 1, maxZ: 2 };
            const entity = { id: 5, name, living: name !== 'oak_boat', position: new Vec3(0.5, minY, 0.5), boundingBox, ...fields };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [entity] });
            physics.simulate(ctx, world);
            return ctx.state.crouching;
        };
        assert.equal(crouching('26.3', 2.6), true);
        assert.equal(crouching('26.3', 2.9), false);
        assert.equal(crouching('1.14.4', 2.6), true);
        assert.equal(crouching('26.3', 2.6, 'oak_boat'), true);
        // an adult happy ghast is a collider from below only on its still timeout (26.3 HappyGhast.java:522-532)
        assert.equal(crouching('26.3', 2.6, 'happy_ghast', { baby: false, staysStill: true }), true);
        assert.equal(crouching('26.3', 2.6, 'happy_ghast', { baby: false, staysStill: false }), false);
        assert.equal(crouching('26.3', 2.6, 'cow'), false);
    });

    it('keeps the box the size update refitted through the pushes after the tick', () => {
        // Player.tick's updateSize sets the bounds to a new box from minX to minX + width (1.12.2 Player.java:324, 386-392,
        // Entity.java:382-394 in legacy-src), not the moved box's maxX: the moving pistons that push after the entities
        // (mineflayer's tickMovingPistons, every tick, with no piston too) meet that box and leave it. The tick that
        // stands up from s_sneak_ice's sneak on 1.12.2 (its box and motion then), where minX + 0.6F is one ulp under the
        // moved maxX: mineflayer-replay's bot layer was an ulp off on the tick after
        for (const version of ['1.9.4', '1.12.2', '1.13.2']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const standing = () => {
                const pos = new Vec3(1.8733437433633933, 1, 0.5);
                const bb = { minX: 1.573343731442464, minY: 1, minZ: 0.5 - f32(0.6) / 2, maxX: 2.173343755284322, maxY: 1 + f32(1.65), maxZ: 0.5 + f32(0.6) / 2 };
                return playerState(pos.x, 1, 0.5, {
                    onGround: true, vel: new Vec3(0.05381995340346649, 0, 0), prevControl: controls({ sneak: true }),
                    keptBoundingBox: { bb, pos: pos.clone(), width: f32(0.6), height: f32(1.65) },
                });
            };
            const pushed = standing();
            const plain = standing();
            for (let tick = 0; tick < 3; tick++) {
                for (const ctx of [pushed, plain]) physics.simulate(ctx, world);
                physics.pushByMovingPistons(pushed, world, []);
                const at = `${version} tick ${tick}`;
                const { bb } = pushed.state.keptBoundingBox;
                if (tick === 0) {
                    assert.equal(pushed.state.keptBoundingBox.height, f32(1.8), at);
                    assert.equal(bb.maxX, bb.minX + f32(0.6), at);
                }
                assert.deepEqual({ ...bb }, { ...plain.state.keptBoundingBox.bb }, at);
                assert.deepEqual(pushed.state.pos.toArray(), plain.state.pos.toArray(), at);
            }
        }
    });

    it('updates the size before 1.14 whatever the entities', () => {
        // EntityPlayer.updateSize asks the world about blocks alone (1.12.2 PlayerEntity.java:386-389, World.java:1225-1227;
        // 1.13.2 PlayerEntity.java:440-443 passes no entity): the box stands up into the shulker, which then pushes it
        for (const version of ['1.12.2', '1.13.2']) {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const boundingBox = { minX: 0.1, minY: 2.7, minZ: 0, maxX: 1.1, maxY: 3.7, maxZ: 1 };
            const entity = { id: 5, name: 'shulker', living: true, position: new Vec3(0.6, 2.7, 0.5), boundingBox };
            const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [entity], prevControl: controls({ sneak: true }) });
            physics.simulate(ctx, world);
            assert.equal(physics.carries.get(ctx.state).size.height, f32(1.65), version);
            assert.equal(ctx.state.keptBoundingBox.height, f32(1.8), version);
            assert.equal(ctx.state.pushedEntities.length, 1, version);
        }
    });

    it('picks the pose by the scaled sizes', () => {
        // Player.updatePlayerPose tests the desired pose, then crouching, then swimming, in getDimensions(pose), which
        // scales all but the fixed sleeping and dying dimensions (26.3 Player.java:344-367, LivingEntity.java:3684-3686,
        // Avatar.java:55-58, EntityDimensions.java:29-38)
        const version = '26.3';
        const { registry, world } = worldOf(version, ({ registry: r, set }) => {
            floor({ registry: r, set });
            const stone = r.blocksByName.stone.defaultState;
            // a ceiling 1 high over x -1, one 3 high over x 3, a beam at y 2 at x 1
            for (let z = -1; z <= 1; z += 1) {
                set(-1, 2, z, stone);
                set(3, 4, z, stone);
                set(1, 2, z, stone);
            }
        });
        const physics = new adapter.NativePhysics(registry);
        const posed = (x, scale, fields = {}) => {
            const { state } = playerState(x, 1, 0.5, { onGround: true, scale, ...fields });
            const sync = physics.syncOf(world);
            sync.sync(world, { minX: -4, minY: 0, minZ: -4, maxX: 5, maxY: 8, maxZ: 5 }, 2);
            physics.updatePoses(state, sync);
            return state.pose;
        };
        assert.equal(posed(-0.5, 1), adapter.Pose.SWIMMING);
        assert.equal(posed(-0.5, 0.5), adapter.Pose.STANDING);
        assert.equal(posed(-0.5, 0.5, { control: controls({ sneak: true }) }), adapter.Pose.SNEAKING);
        assert.equal(posed(3.5, 1), adapter.Pose.STANDING);
        assert.equal(posed(3.5, 2), adapter.Pose.SNEAKING);
        assert.deepEqual(physics.poseSize(adapter.Pose.SWIMMING, 2), { width: f32(1.2), height: f32(1.2) });
        assert.deepEqual(physics.poseSize(adapter.Pose.SLEEPING, 2), { width: f32(0.2), height: f32(0.2) });
        assert.equal(physics.eyeHeight(playerState(0.5, 1, 0.5, { pose: adapter.Pose.SWIMMING, scale: 2 }).state), f32(f32(0.4) * 2));
    });

    it('stays under water by the last tick\'s eyes, out of the water as well', () => {
        // LocalPlayer.isUnderWater is what Player.tick set before the base tick: physrec/bt_cross_up_fast on 1.21.11 has
        // isInWater false and isUnderWater true at t 35. Before 1.16 it is the entity's own test.
        const { registry, world } = worldOf('1.21.11', floor);
        const tick = version => {
            const { registry: r, world: w } = version === '1.21.11' ? { registry, world } : worldOf(version, floor);
            const ctx = playerState(0.5, 5, 0.5, { eyeInWater: true, isUnderWater: false, isInWater: true });
            new adapter.NativePhysics(r).simulate(ctx, w);
            return ctx.state;
        };
        assert.equal(tick('1.21.11').isInWater, false);
        assert.equal(tick('1.21.11').isUnderWater, true);
        assert.equal(tick('1.16.5').isUnderWater, true);
        assert.equal(tick('1.15.2').isUnderWater, false);
    });

    it('moves with the old dimensions in the tick a synced scale arrives', () => {
        // The attribute packet only marks the scale dirty: the end of LivingEntity.aiStep refreshes the dimensions (1.20.5
        // LivingEntity.java:2461-2466, 26.3 :2786, :1097-1109), as physrec/ps_crawl_scale_changes records: from x 2.598 the
        // tick still moves at 0.15 wide, and the next box is the 0.3 one. So a wall the new box would reach stops nothing.
        const stoneWall = ({ registry: r, set }) => {
            floor({ registry: r, set });
            for (let y = 1; y <= 2; y += 1) for (let z = -1; z <= 1; z += 1) set(3, y, z, r.blocksByName.stone.defaultState);
        };
        const { registry, world } = worldOf('26.3', stoneWall);
        const physics = new adapter.NativePhysics(registry);
        const scale = value => ({ scale: { value, modifiers: [] }, 'minecraft:scale': { value, modifiers: [] } });
        const ctx = playerState(2.5, 1, 0.5, { onGround: true, attributes: scale(0.5) });
        physics.simulate(ctx, world);
        assert.equal(ctx.state.scale, 0.5);
        ctx.state.attributes = scale(1);
        ctx.state.pos.set(2.7, 1, 0.5);
        ctx.state.vel.set(0.1, 0, 0);
        physics.simulate(ctx, world);
        assert.ok(ctx.state.pos.x > 2.75 && ctx.state.pos.x < 2.85, String(ctx.state.pos.x));
        assert.equal(ctx.state.scale, 1);
    });

    it('takes a minecart among the entities around the player', () => {
        // Entity kind 8: a player beside a cart ticks on, and the cart pushes it as any entity that pushes
        const { registry, world } = worldOf('26.3', floor);
        const physics = new adapter.NativePhysics(registry);
        const cart = {
            id: 7, name: 'minecart', living: false, position: new Vec3(1, 1, 0.5),
            boundingBox: { minX: 0.51, minY: 1, minZ: 0.01, maxX: 1.49, maxY: 1.7, maxZ: 0.99 },
        };
        const ctx = playerState(0.5, 1, 0.5, { onGround: true, nearbyEntities: [cart] });
        assert.doesNotThrow(() => physics.simulate(ctx, world));
    });

    it('collides each tick with the border snapshot it is given', () => {
        // The edges are those of the moment (Entity.collide, 26.3 Entity.java:1262-1263), from 1.21.11 a moving border's
        // size before its last tick (26.3 WorldBorder.java:346-357); an edge past the player stops nothing
        const { registry, world } = worldOf('26.3', floor);
        const physics = new adapter.NativePhysics(registry);
        const game = { minY: -64, height: 384 };
        const east = worldBorder => {
            const ctx = playerState(2.5, 1, 0.5, { onGround: true, vel: new Vec3(0.5, 0, 0), bot: { worldBorder, game } });
            physics.simulate(ctx, world);
            return ctx.state;
        };
        assert.equal(east({ centerX: 0, centerZ: 0, size: 6 }).pos.x, 3 - 0.30000001192092896);
        assert.equal(east({ centerX: 0, centerZ: 0, size: 3, previousSize: 6 }).pos.x, 3 - 0.30000001192092896);
        assert.ok(east({ centerX: 0, centerZ: 0, size: 3 }).pos.x > 2.5);
    });

    it('leaves a thin wall the merger drops out of the pose fit', () => {
        // BlockCollisions joins a shape that is not the whole cube with the box, and the merger drops an overlap of 1e-7
        // (1.14.4 VoxelShapes.matchesAnywhere, 26.3 BlockCollisions.java:91-104). 1.14 and 1.15 ask with the box as it is
        const open = ({ registry, Block, set }) => {
            floor({ registry, set });
            set(1, 1, 0, Block.fromProperties('oak_trapdoor', { facing: 'east', half: 'bottom', open: true, powered: false, waterlogged: false }, 0).stateId);
        };
        const { registry, world } = worldOf('1.14.4', open);
        const physics = new adapter.NativePhysics(registry);
        const crouch = x => {
            const ctx = playerState(x, 1, 0.5, { onGround: true, control: controls({ sneak: true }), prevControl: controls({ sneak: true }) });
            physics.simulate(ctx, world);
            return ctx.state.crouching;
        };
        // (the plate of a trapdoor open to the east is the west side of its block, from x 1 on)
        const flush = 1 - f32(0.6) / 2;
        assert.equal(crouch(flush + 5e-8), true);
        assert.equal(crouch(flush + 1e-3), false);
    });

    it('meets the world border in the pose fit from 1.14', () => {
        // noCollision's noBorderCollision (26.3 CollisionGetter.java:52-54, 74-81): from 1.18 the shape where it meets the
        // query, for an entity inside and close to the border, so a box over the edge fits nothing. In 1.14 - 1.17
        // getBlockCollisions offers the shape, whatever the query, only for an entity whose box shrunk by 1e-7 misses it
        // and grown by 1e-7 meets it past the merger's 1e-7 (client1.14.4 LevelReader.java:146-155, 1.16.5
        // BlockCollisionSpliterator.java:115-127): a box a hair over the edge, not one flush with it or well over it.
        const sneak = (version, x) => {
            const { registry, world } = worldOf(version, floor);
            const physics = new adapter.NativePhysics(registry);
            const bot = { worldBorder: { centerX: 0, centerZ: 0, size: 6 }, game: { minY: -64, height: 384 } };
            const ctx = playerState(x, 1, 0.5, { onGround: true, control: controls({ sneak: true }), prevControl: controls({ sneak: true }), bot });
            physics.simulate(ctx, world);
            return [ctx.state.crouching, ctx.state.pose === adapter.Pose.SNEAKING];
        };
        const flush = 3 - f32(0.6) / 2;
        for (const version of ['1.14.4', '1.15.2', '1.16.5', '1.17.1']) {
            assert.deepEqual(sneak(version, flush), [true, true], version);
            assert.deepEqual(sneak(version, flush + 1e-7), [false, false], version);
            assert.deepEqual(sneak(version, 2.9), [true, true], version);
        }
        for (const version of ['1.18', '1.20.4', '26.3']) {
            assert.deepEqual(sneak(version, flush), [true, true], version);
            assert.deepEqual(sneak(version, 2.9), [false, false], version);
            assert.deepEqual(sneak(version, 0.5), [true, true], version);
        }
    });
});

describe('the fluid of a waterlogged block', () => {
    // WaterloggedTransparentBlock.getFluidState is the one falling source of all that hold water (26.3 :43, 1.20.4
    // GrateBlock.java:49): FlowingFluid.getFlow turns it down beside a sturdy face, as it does no slab's
    const flowOf = (version, name, props, neighbour) => {
        const { registry, world } = worldOf(version, ({ registry: r, Block, set }) => {
            floor({ registry: r, set });
            set(0, 1, 0, Block.fromProperties(name, props, 0).stateId);
            set(1, 1, 0, Block.fromProperties(neighbour, {}, 0).stateId);
        });
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 2);
        return { flow: sync.world.fluidFlowAt(0, 1, 0), block: sync.world.blockAt(0, 1, 0) };
    };
    for (const version of ['1.20.4', '1.21.1', '26.3']) {
        it(`takes a copper grate for a falling source, ${version}`, () => {
            const grate = flowOf(version, 'copper_grate', { waterlogged: true }, 'stone');
            assert.equal(grate.block.fluidSource, true);
            assert.equal(grate.block.fluid, 3, 'the update reached the world');
            assert.equal(grate.flow.y, -1);
            assert.equal(flowOf(version, 'copper_grate', { waterlogged: true }, 'air').flow.y, 0);
            const slab = flowOf(version, 'stone_slab', { type: 'bottom', waterlogged: true }, 'stone');
            assert.equal(slab.flow.y, 0);
            assert.equal(flowOf(version, 'waxed_oxidized_copper_grate', { waterlogged: true }, 'stone').flow.y, -1);
        });
    }
});

describe('the anvil before and after 1.9', () => {
    // 1.8 reads the bounds on the block object, a full cube for any facing (1.8.9 Block.java:290, recorded by
    // pa_anvil_*); 1.9 gets getCollisionBox by the facing's axis (1.9.4 AnvilBlock.java:102-105)
    const boxes = (version, metadata) => {
        const { registry, world } = worldOf(version, ({ registry: r, set }) => set(0, 1, 0, r.blocksByName.anvil.id << 4 | metadata));
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: -0.5, minY: 1, minZ: -0.5, maxX: 0.5, maxY: 2, maxZ: 0.5 }, 2);
        return sync.boxesAt(0, 1, 0);
    };
    it('collides as a whole cube whichever way it faces on 1.8.8', () => {
        for (const metadata of [0, 1, 2, 3, 4, 8]) assert.deepEqual(boxes('1.8.8', metadata), [[0, 0, 0, 1, 1, 1]], String(metadata));
    });
    it('goes by the axis of its facing on 1.9.4', () => {
        assert.deepEqual(boxes('1.9.4', 0), [[0.125, 0, 0, 0.875, 1, 1]]);
        assert.deepEqual(boxes('1.9.4', 1), [[0, 0, 0.125, 1, 1, 0.875]]);
    });
});

describe('the frost modifier of a live bot', () => {
    // LivingEntity.aiStep removes and adds the powder snow modifier on the client too up to 1.21.4 (1.17
    // LivingEntity.java:2555-2556, 505-527; 1.21.4 :2812-2813), and the attributes packet brings the server's a tick
    // later. bot.entity.attributes is the client's AttributeInstance: the adapter keeps the modifier there, as with the
    // sprint, and the bot's metadata at 7 holds the frozen ticks.
    const frostId = version => (version.startsWith('1.21') ? 'minecraft:powder_snow' : '1eaf83ff-7207-4596-b37a-d7a07b3ec4ce');
    const start = (version, ticksFrozen, { modifiers = [], y = 1, keys = { forward: true }, onGround = y === 1 } = {}) => {
        const { registry, world } = worldOf(version, floor);
        const physics = new adapter.NativePhysics(registry);
        const key = physics.attributeKey('movementSpeed');
        const metadata = [0, 300, '', false, false, false, 0, ticksFrozen];
        const ctx = playerState(0.5, y, 0.5, {
            onGround, control: controls(keys), attributes: { [key]: { value: f32(0.1), modifiers } }, bot: { entity: { metadata } },
        });
        return { physics, world, ctx, key, metadata, held: () => ctx.state.attributes[key].modifiers };
    };
    const frostOf = (version, held) => held.find(modifier => modifier.uuid === frostId(version))?.amount;
    const half = f32(-0.05 * f32(0.5));
    const settle = bot => {
        bot.physics.simulate(bot.ctx, bot.world);
        bot.physics.simulate(bot.ctx, bot.world);
    };

    for (const version of ['1.17', '1.20.4', '1.21.1', '1.21.4']) {
        it(`makes the client's own modifier from the synced ticks, and the next tick walks with it, ${version}`, () => {
            // (the first tick settles from y 1 in the air's speed: the walk that reads the attribute is on the ground)
            const bot = start(version, 70);
            settle(bot);
            assert.equal(frostOf(version, bot.held()), half, 'the end of aiStep holds it');
            assert.equal(bot.held()[0].operation, 0);
            // a client that had the server's packet with the same modifier before this tick walks the same
            const synced = start(version, 0);
            settle(synced);
            synced.metadata[7] = 70;
            synced.ctx.state.attributes[synced.key] = { value: f32(0.1), modifiers: bot.held().map(modifier => ({ ...modifier })) };
            bot.physics.simulate(bot.ctx, bot.world);
            synced.physics.simulate(synced.ctx, synced.world);
            assert.deepEqual([bot.ctx.state.pos.x, bot.ctx.state.vel.x, bot.ctx.state.pos.z, bot.ctx.state.vel.z],
                [synced.ctx.state.pos.x, synced.ctx.state.vel.x, synced.ctx.state.pos.z, synced.ctx.state.vel.z]);
            const plain = start(version, 0);
            settle(plain);
            plain.physics.simulate(plain.ctx, plain.world);
            assert.notEqual(bot.ctx.state.pos.x + bot.ctx.state.pos.z, plain.ctx.state.pos.x + plain.ctx.state.pos.z, 'the frost slows the walk');
        });

        it(`follows the synced ticks as they change, keeping the sprint beside it, ${version}`, () => {
            const bot = start(version, 70, { keys: { forward: true, sprint: true } });
            bot.physics.simulate(bot.ctx, bot.world);
            assert.equal(frostOf(version, bot.held()), half);
            bot.metadata[7] = 140;
            bot.physics.simulate(bot.ctx, bot.world);
            assert.equal(frostOf(version, bot.held()), f32(-0.05), 'the whole of it at the limit');
            assert.equal(bot.held().length, 2, 'the sprint stays');
            bot.metadata[7] = 0;
            bot.physics.simulate(bot.ctx, bot.world);
            assert.equal(frostOf(version, bot.held()), undefined, 'gone with the ticks, before the packet says so');
            assert.equal(bot.held().length, 1);
        });

        it(`takes the modifier off in the air, and the server's packet replaces the client's, ${version}`, () => {
            // (addPowderSnowSlowIfNeeded: only with a landing block that is not air)
            const air = start(version, 70, { y: 10, modifiers: [{ uuid: frostId(version), amount: half, operation: 0 }] });
            air.physics.simulate(air.ctx, air.world);
            assert.equal(frostOf(version, air.held()), undefined);

            // the packet on the tick right after the client made it, with what the adapter last sent
            const bot = start(version, 0);
            settle(bot);
            bot.metadata[7] = 70;
            bot.physics.simulate(bot.ctx, bot.world);
            assert.equal(frostOf(version, bot.held()), half);
            // mineflayer's update_attributes handler: the whole entry goes
            bot.ctx.state.attributes[bot.key] = { value: f32(0.1), modifiers: [] };
            const plain = start(version, 0);
            settle(plain);
            plain.physics.simulate(plain.ctx, plain.world);
            bot.physics.simulate(bot.ctx, bot.world);
            plain.physics.simulate(plain.ctx, plain.world);
            assert.equal(bot.ctx.state.pos.x, plain.ctx.state.pos.x);
            assert.equal(bot.ctx.state.pos.z, plain.ctx.state.pos.z);
            assert.equal(frostOf(version, bot.held()), half, 'the end of that aiStep makes it again');
        });
    }

    it('leaves the attribute to the server from 1.21.5 and before 1.17', () => {
        for (const version of ['1.16.5', '1.21.5', '26.3']) {
            const bot = start(version, 70);
            bot.physics.simulate(bot.ctx, bot.world);
            assert.deepEqual(bot.held(), [], version);
        }
    });

    it('sends the frozen ticks to the engine when they sync', () => {
        const bot = start('1.20.4', 70);
        bot.physics.simulate(bot.ctx, bot.world);
        const carry = () => bot.physics.carries.get(bot.ctx.state).player.state().ticksFrozen;
        assert.equal(carry(), 70);
        bot.metadata[7] = 72;
        bot.physics.simulate(bot.ctx, bot.world);
        assert.equal(carry(), 72);
        bot.metadata[7] = -5;
        bot.physics.simulate(bot.ctx, bot.world);
        assert.equal(carry(), 0, 'a negative count is none');
    });

    it('leaves the count the client made alone until a sync changes it', () => {
        // from 1.21.5 the client counts in powder snow itself (InsideBlockEffectType.FREEZE)
        const bot = start('1.21.5', 70);
        bot.physics.simulate(bot.ctx, bot.world);
        const { player } = bot.physics.carries.get(bot.ctx.state);
        player.setState({ ...player.state(), ticksFrozen: 71 });
        // an input that changes, and no sync of the ticks
        bot.ctx.state.attributes[bot.key] = { value: f32(0.2), modifiers: [] };
        bot.physics.simulate(bot.ctx, bot.world);
        assert.equal(player.state().ticksFrozen, 71);
        bot.metadata[7] = 60;
        bot.physics.simulate(bot.ctx, bot.world);
        assert.equal(player.state().ticksFrozen, 60);
    });
});
