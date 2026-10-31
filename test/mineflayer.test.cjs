// The mineflayer adapter (dist/mineflayer) on small prismarine worlds: the NativeWorld it keeps in step, and the player
// ticks it runs. Run after `pnpm build` and the native build: node --test test/
const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const adapter = require('..');

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

/** The registry of `base` taken for the later release `release`, which minecraft-data does not know yet. */
function registryAs(base, release) {
    const registry = require('prismarine-registry')(base);
    const parts = version => version.split('.').map(Number);
    const compare = other => {
        const [a, b] = [parts(release), parts(other)];
        for (let i = 0; i < Math.max(a.length, b.length); i += 1) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) - (b[i] ?? 0);
        return 0;
    };
    const version = {
        ...registry.version, minecraftVersion: release, majorVersion: release.split('.').slice(0, 2).join('.'),
        '<': other => compare(other) < 0, '<=': other => compare(other) <= 0, '>': other => compare(other) > 0,
        '>=': other => compare(other) >= 0, '==': other => compare(other) === 0,
    };
    return new Proxy(registry, { get: (target, key) => (key === 'version' ? version : target[key]) });
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
        pistonDeltas: [0, 0, 0], pistonDeltasTick: -1, jumpTicks: 0, sprintTriggerTime: 0, prevForwardImpulse: 0,
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
        // empty; 1.14 - 1.15 the #leaves, Block.java:407-409), eight layers of snow from 1.16 (SnowBlock.getSidesShape);
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
        assert.equal(downBeside('1.14.4', 'oak_leaves', { distance: 7, persistent: false }), 0);
        assert.equal(downBeside('1.14.4', 'snow', { layers: 8 }), 0);
        assert.equal(downBeside('1.13.2', 'soul_sand'), -1);
        assert.equal(downBeside('1.13.2', 'glass'), 0);
    });
});

describe('NativePhysics', () => {
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
        ctx.state.effects = { [weaving]: { id: weaving, amplifier: 0 } };
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
            const { world } = worldOf('1.21.4', ({ Block: B, set }) => set(Math.floor(x), 0, 0, B.fromProperties('water', { level: 0 }, 0).stateId));
            if (!loadedFromX16) unloadedFromX16(world);
            const physics = new adapter.NativePhysics(registryAs('1.21.4', release));
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

    it('tops a water source under a block sturdy below at 1 for the eyes in 26.3', () => {
        // FluidState.getHeightForCamera (26.3 FluidState.java:53-60, EntityFluidInteraction.java:84-87)
        const eyes = (release, above) => {
            const { world } = worldOf('1.21.4', ({ registry: r, Block: B, set }) => {
                set(0, 0, 0, B.fromProperties('water', { level: 0 }, 0).stateId);
                set(0, 1, 0, r.blocksByName[above].defaultState);
            });
            const physics = new adapter.NativePhysics(registryAs('1.21.4', release));
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
        assert.deepEqual(glides('1.21.10', 'vine', vine), [false, adapter.Pose.STANDING]);
        assert.deepEqual(glides('1.21.11', 'vine', vine), [true, adapter.Pose.FALL_FLYING]);
        assert.deepEqual(glides('1.21.11', 'ladder', ladder), [false, adapter.Pose.STANDING]);
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
            const { world } = worldOf('1.21.4', () => {});
            const physics = new adapter.NativePhysics(registryAs('1.21.4', release));
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
});

describe('NativeRiddenMobPhysics', () => {
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
});

describe('BlockMapper', () => {
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
});
