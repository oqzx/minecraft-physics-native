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

    it('leaves out the chunk columns a prismarine world has not loaded', () => {
        const { registry, world } = worldOf('1.21.1', floor);
        unloadedFromX16(world);
        const sync = new adapter.NativeWorldSync(adapter.LoadNativePhysics(), registry);
        sync.sync(world, { minX: 15.2, minY: 1, minZ: 0.2, maxX: 15.8, maxY: 2.8, maxZ: 0.8 }, 2);
        assert.equal(sync.world.isChunkLoaded(0, 0), true);
        assert.equal(sync.world.isChunkLoaded(1, 0), false);
        assert.equal(sync.blockAt(16, 1, 0), null);
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
