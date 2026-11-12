// What an authoritative update does to the state the engine carries (docs/synchronization.md): the player's teleports,
// velocity corrections and rotations through the adapter, and whole-state replacement of NativePlayer. Run after
// `pnpm build` and the native build: node --test test/
const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const adapter = require('..');
const { LoadNativePhysics } = require('../dist/native');

const f32 = Math.fround;
const binding = LoadNativePhysics();

function worldOf(version, place) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const blocks = new Map();
    place({ registry, set: (x, y, z, id) => blocks.set(`${x},${y},${z}`, id) });
    const air = registry.blocksByName.air.defaultState ?? 0;
    const stateAt = pos => blocks.get(`${Math.floor(pos.x)},${Math.floor(pos.y)},${Math.floor(pos.z)}`) ?? air;
    const world = Object.assign(new EventEmitter(), {
        getBlock: pos => Block.fromStateId(stateAt(pos), 0),
        getBlockStateId: stateAt,
    });
    return { registry, world };
}

/** Stone at y 0 from -8 to 8, and a ledge of stone at y 4 over x 4 to 8. */
const ground = ({ registry, set }) => {
    const stone = registry.blocksByName.stone.defaultState ?? registry.blocksByName.stone.id << 4;
    for (let x = -8; x <= 8; x += 1) for (let z = -8; z <= 8; z += 1) set(x, 0, z, stone);
    for (let x = 4; x <= 8; x += 1) for (let z = -8; z <= 8; z += 1) set(x, 4, z, stone);
};

const controls = (keys = {}) => ({ forward: false, back: false, left: false, right: false, jump: false, sprint: false, sneak: false, ...keys });

function playerState(x, y, z) {
    return {
        state: {
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
            getBB: () => ({ minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0 }),
        },
    };
}

describe('authoritative updates of the adapter', () => {
    const { registry, world } = worldOf('26.3', ground);
    const physics = new adapter.NativePhysics(registry);
    const settled = () => {
        const ctx = playerState(0.5, 1, 0.5);
        for (let i = 0; i < 3; i += 1) physics.simulate(ctx, world);
        return ctx;
    };

    it('takes a teleport as the place the next tick starts from, and the ground goes at the next move', () => {
        const ctx = settled();
        const { state } = ctx;
        assert.equal(state.onGround, true);
        const age = state.age;
        // a position packet: the caller sets the place, and nothing runs until the next tick
        state.pos.set(6.5, 5, 0.5);
        state.vel.set(0, 0, 0);
        assert.equal(state.age, age);
        physics.simulate(ctx, world);
        assert.equal(state.age, age + 1);
        assert.equal(state.pos.x, 6.5);
        assert.equal(state.pos.y, 5);
        // (a motion of 0 moves nothing, and only the next tick's gravity meets the ledge, as vanilla's)
        physics.simulate(ctx, world);
        assert.equal(state.pos.y, 5);
        assert.equal(state.onGround, true);
        // off the ledge the ground goes at the first move
        state.pos.set(0.5, 3, 0.5);
        physics.simulate(ctx, world);
        assert.equal(state.onGround, false);
        assert.ok(state.pos.y < 3);
    });

    it('starts the falling of a corrected velocity from what was sent, once', () => {
        const ctx = settled();
        const { state } = ctx;
        state.vel.set(0.25, 0.42, 0);
        physics.simulate(ctx, world);
        const risen = state.pos.y;
        assert.ok(Math.abs(risen - 1.42) < 1e-9);
        // the second tick owes nothing to the correction but its motion
        physics.simulate(ctx, world);
        assert.ok(state.pos.y - risen < 0.42 - 0.05);
    });

    it('moves nothing on a rotation-only update', () => {
        const ctx = settled();
        const { state } = ctx;
        const [x, y, z, age] = [state.pos.x, state.pos.y, state.pos.z, state.age];
        state.yaw = 1;
        state.yawDegrees = 90;
        state.pitch = 0.5;
        assert.deepEqual([state.pos.x, state.pos.y, state.pos.z, state.age], [x, y, z, age]);
        physics.simulate(ctx, world);
        assert.deepEqual([state.pos.x, state.pos.y, state.pos.z], [x, y, z]);
    });
});

describe('flags and support across the two APIs', () => {
    const { registry, world } = worldOf('26.3', ground);
    const physics = new adapter.NativePhysics(registry);

    it('carries the landing out of the packed tick into a next tick of the object API, and neither sees a stale flag', () => {
        const ctx = playerState(0.5, 1.3, 0.5);
        const { state } = ctx;
        while (!state.onGround) physics.simulate(ctx, world);
        const carried = physics.carries.get(state).player.state();
        assert.equal(carried.verticalCollisionBelow, true);
        assert.equal(carried.collidedVertically, true);
        assert.deepEqual(carried.supportingBlock, { x: 0, y: 0, z: 0 });
        const copy = new binding.NativePlayer('26.3', carried, {});
        assert.deepEqual(copy.state(), carried);
        const sync = physics.syncOf(world);
        const object = binding.simulateVersionedTick('26.3', sync.world, carried, {});
        physics.simulate(ctx, world);
        assert.equal(object.position.y, state.pos.y);
        assert.equal(object.onGround, state.onGround);
        assert.equal(object.verticalCollisionBelow, physics.carries.get(state).player.state().verticalCollisionBelow);
        assert.deepEqual(object.supportingBlock, physics.carries.get(state).player.state().supportingBlock);
        // and off the ledge the flags go on the first move down
        state.pos.set(0.5, 3, 0.5);
        physics.simulate(ctx, world);
        physics.simulate(ctx, world);
        const next = physics.carries.get(state).player.state();
        assert.equal(next.verticalCollisionBelow, false);
        assert.equal(next.onGround, false);
        assert.equal(next.supportingBlock ?? null, null);
    });

    it('refits a kept box to a pose the server synced, before the tick, on 1.14 to 1.16', () => {
        const { registry: old, world: oldWorld } = worldOf('1.16.5', ground);
        const engine = new adapter.NativePhysics(old);
        const ctx = playerState(0.5, 1, 0.5);
        const { state } = ctx;
        const boxOf = (minY, maxY) => ({ minX: 0.2, minY, minZ: 0.2, maxX: 0.8, maxY, maxZ: 0.8 });
        for (let i = 0; i < 3; i += 1) engine.simulate(ctx, oldWorld);
        assert.equal(state.sizedPose, 0);
        state.keptBoundingBox = { bb: boxOf(1, 2.8), pos: state.pos.clone(), width: 0.6, height: 1.8 };
        let refits = 0;
        const refresh = engine.refreshDimensions.bind(engine);
        engine.refreshDimensions = target => {
            refits += 1;
            refresh(target);
        };
        state.pose = 5;
        engine.simulate(ctx, oldWorld);
        assert.ok(refits >= 1);
    });
});

describe('NativePlayer state replacement', () => {
    const { registry, world } = worldOf('26.3', ground);
    const engine = new adapter.NativePhysics(registry);
    const native = engine.syncOf(world);
    native.sync(world, { minX: -8, minY: 0, minZ: -8, maxX: 8, maxY: 8, maxZ: 8 }, 2);
    const walker = () => ({
        position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: true,
        collidedHorizontally: false, collidedVertically: true, isPlayer: true, forward: 1, strafe: 0, yaw: 0,
    });
    const step = (player, n = 1) => {
        for (let i = 0; i < n; i += 1) player.setState(binding.simulateVersionedTick('26.3', native.world, player.state(), {}));
    };

    it('continues a clone as the original, after the same correction', () => {
        const first = new binding.NativePlayer('26.3', walker(), {});
        step(first, 4);
        const copy = new binding.NativePlayer('26.3', first.state(), {});
        const correct = player => player.setState({ ...player.state(), position: { x: 1.5, y: 1, z: 1.5 }, velocity: { x: 0, y: 0, z: 0.1 } });
        correct(first);
        correct(copy);
        step(first, 3);
        step(copy, 3);
        assert.deepEqual(copy.state(), first.state());
    });

    it('does not age the state or run anything on a replacement', () => {
        const player = new binding.NativePlayer('26.3', { ...walker(), tickCount: 7, fireworkRockets: 1 }, {});
        const before = player.state();
        player.setState(before);
        player.setState(player.state());
        assert.deepEqual(player.state(), before);
    });

    it('refuses a state that is not whole and keeps the one it had', () => {
        const player = new binding.NativePlayer('26.3', walker(), {});
        const before = player.state();
        assert.throws(() => player.setState({ ...before, halfWidth: -1 }));
        assert.deepEqual(player.state(), before);
    });
});
