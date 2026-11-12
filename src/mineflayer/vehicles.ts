// The vehicles the local player steers, for mineflayer-physics-utils' BoatState and RiddenMobState, as its BoatPhysics
// and RiddenMobPhysics tick them: the native engine's versioned boat tick (Boat.tick; 1.8's EntityBoat.onUpdate) and
// ridden mob tick (LivingEntity.travelRidden of the mob named by its type). The engine's own fields that the states have
// no place for are carried from one tick to the next while the vehicle stays where the engine left it.
import type { NativeAabb, NativeBoatState, NativeHorseState, NativeMinecartClientEffects, NativeMinecartClientSteps, NativeMinecartContainer, NativeMinecartGateway, NativeMinecartHurt, NativeMinecartItem, NativeMinecartLerp, NativeMinecartOther, NativeMinecartRules, NativeMinecartScene, NativeMinecartState, NativeMinecartTick, NativeNearbyEntity, NativePistonPushedEntity, NativePistonPushedResult, NativeVec3, NativeWorld } from '../native';
import { NativeEngine } from './base';
import { nativeMovingPiston, type MovingPiston } from './player';
import type { Attributes, Box, Controls, KeptBoundingBox, MoveRefusable, PhysicsCtx, Vec3 } from './types';
import { bodyReach, syncReads, type PrismarineWorld } from './world';

const f32 = Math.fround;

/** mineflayer-physics-utils' BoatStatus, by the engine's names. */
const BOAT_STATUSES = ['in_water', 'under_water', 'under_flowing_water', 'on_land', 'in_air'] as const;
type BoatStatusName = typeof BOAT_STATUSES[number];

/** mineflayer-physics-utils' BoatState, the fields the adapter uses. */
export interface BoatState {
    age: number;
    pos: Vec3;
    vel: Vec3;
    onGround: boolean;
    isInWater: boolean;
    isCollidedHorizontally: boolean;
    isCollidedVertically: boolean;
    control: Controls;
    status: number;
    previousStatus: number;
    waterLevel: number;
    landFriction: number;
    yawVelocity: number;
    yaw: number;
    yawDegrees: number;
    box?: Box;
    lastVerticalVelocity: number;
    /** Entity.fallDistance of the boat, which powder snow's collision reads (absent: 0, then the engine's). */
    fallDistance?: number;
    controllingPlayer: boolean;
    speedMultiplier: number;
    worldReady: boolean;
    halfWidth: number;
    height: number;
    /** The last tick's motion was too large to walk, and the tick dropped it and did nothing else (docs/numeric-contract.md). */
    moveRefused?: boolean;
    /** Set by the caller: the boat's dimension is ultrawarm (the nether), where lava pushes at 0.007 (FAST_LAVA). */
    ultraWarm?: boolean;
    /** The boat's NoGravity flag (Entity.isNoGravity), when the state has it; else its entity's synced data at 5. */
    noGravity?: boolean;
    /** The boat's entity (prismarine-entity), whose synced data holds the NoGravity flag. */
    entity?: { metadata?: unknown[] };
    /** Set by the caller: the entities around the boat, it and those sharing it left out, which its move collides with. */
    nearbyEntities?: NativeNearbyEntity[];
    getBB(): Box;
}

/** What the rider gives its mount each tick (mineflayer-physics-utils' RiderInput). */
export interface RiderInput {
    yawDegrees: number;
    pitchDegrees: number;
    xxa?: number;
    zza?: number;
    sprinting?: boolean;
    jumping?: boolean;
}

/** A rotation in float degrees. */
export interface Rotation {
    yaw: number;
    pitch: number;
}

/** mineflayer-physics-utils' RiddenMobState, the fields the adapter uses. */
export interface RiddenMobState {
    /** Set by the caller: the entities around the mob, it and those sharing it left out, which its move collides with. */
    nearbyEntities?: NativeNearbyEntity[];
    age: number;
    type: string;
    pos: Vec3;
    vel: Vec3;
    onGround: boolean;
    lastOnGround: boolean;
    isInWater: boolean;
    isInLava: boolean;
    isCollidedHorizontally: boolean;
    isCollidedVertically: boolean;
    fallDistance: number;
    yawDegrees?: number;
    pitchDegrees: number;
    pitch: number;
    movementSpeed: number;
    jumpStrength: number;
    flyingSpeed: number;
    riddenSpeed: number;
    boosting: boolean;
    boostTime: number;
    boostTimeTotal: number;
    suffocating: boolean;
    noAi: boolean;
    jumpPendingScale: number;
    dashCooldown: number;
    dashing: boolean;
    camelSitting: boolean;
    poseTime: number;
    horseJumping: boolean;
    standing: boolean;
    allowStandSliding: boolean;
    saddled: boolean;
    ownForward: number;
    standCounter: number;
    standAnim: number;
    standAnimO: number;
    keptBoundingBox: KeptBoundingBox | null;
    /** The last tick's motion was too large to walk, and the tick dropped it and did nothing else (docs/numeric-contract.md). */
    moveRefused?: boolean;
    /** A strider's MOVEMENT_SPEED with its suffocating modifier among its others; less 34% of movementSpeed when not given. */
    suffocatingSpeed?: number;
    /** The mob's measured size in its pose (its SCALE and age); its type's when not given. */
    dimensions?: { width: number; height: number };
    /** Entity.stuckSpeedMultiplier, which a cobweb, berry bush or powder snow leaves for the next move. */
    stuckSpeedMultiplier?: Vec3;
    /** The mob's synced attributes, those of its travel when it has them. */
    attributes?: Attributes;
    /**
     * Where a server teleport has the mob interpolate to and the steps left (LivingEntity.lerpTo, InterpolationHandler),
     * where the last step left it (1.21.5+), where the step started from and the ticks into it (26.3+); the rotation it
     * heads to (null: none), its rotation after the last step and the one the step started from.
     */
    lerpTarget: Vec3 | null;
    lerpSteps: number;
    lerpPrevious: Vec3 | null;
    lerpStart: Vec3 | null;
    lerpTicks: number;
    lerpYaw: number | null;
    lerpPitch: number;
    lerpPreviousRotation: Rotation | null;
    lerpStartRotation: Rotation | null;
    /** Set by the caller: the mob's dimension is ultrawarm (the nether), where lava pushes at 0.007 (FAST_LAVA). */
    ultraWarm?: boolean;
    /** The mob's effects as mineflayer keeps an entity's (by effect id), its Jump Boost among them. */
    effects?: object;
    /** The mob's NoGravity flag (Entity.isNoGravity), when the state has it; else its entity's synced data at 5. */
    noGravity?: boolean;
    /** The mob's entity (prismarine-entity), whose synced data holds the NoGravity flag. */
    entity?: { metadata?: unknown[] };
    getBB(): Box;
}

const HORSES = new Set(['horse', 'donkey', 'mule', 'skeleton_horse', 'zombie_horse', 'llama', 'trader_llama']);
const NAUTILUSES = new Set(['nautilus', 'zombie_nautilus']);

const vec = (v: { x: number; y: number; z: number }): NativeVec3 => ({ x: v.x, y: v.y, z: v.z });
const nativeBox = (b: Box): NativeAabb => ({ min: { x: b.minX, y: b.minY, z: b.minZ }, max: { x: b.maxX, y: b.maxY, z: b.maxZ } });
const samePosition = (a: { x: number; y: number; z: number }, b: { x: number; y: number; z: number }) => a.x === b.x && a.y === b.y && a.z === b.z;

/** Mth.wrapDegrees of a float: within [-180, 180). */
function wrapDegreesF(degrees: number): number {
    let wrapped = f32(degrees % 360);
    if (wrapped >= 180) wrapped = f32(wrapped - 360);
    if (wrapped < -180) wrapped = f32(wrapped + 360);
    return wrapped;
}

/** Mth.wrapDegrees of a double: within [-180, 180). */
function wrapDegreesD(degrees: number): number {
    let wrapped = degrees % 360;
    if (wrapped >= 180) wrapped -= 360;
    if (wrapped < -180) wrapped += 360;
    return wrapped;
}

/**
 * Camel.canJump, without which LocalPlayer.jumpableVehicle is null and the rider's jump never reaches the camel: saddled,
 * and not refusing to move (sitting, or within the 40 ticks of sitting down or the 52 of standing up) (1.19.3
 * Camel.java:254-256, 1.20 :272-274, 598-601; 1.21.11 Camel.java:236-238, 253-255, LocalPlayer.java:539-543).
 * The rider's aiStep reads it after the camel's tick, on the game time that tick read (Minecraft.java:1798, 1823;
 * ClientLevel.java:298-299): the pose time before the tick's step.
 */
function canCamelJump(mob: Pick<RiddenMobState, 'saddled' | 'camelSitting' | 'poseTime'>): boolean {
    const inPoseTransition = mob.poseTime - 1 < (mob.camelSitting ? 40 : 52);
    return mob.saddled !== false && !mob.camelSitting && !inPoseTransition;
}

/** Entity.setRot: the yaw and pitch, each kept within 360. */
function setRot(mob: RiddenMobState, yaw: number, pitch: number): void {
    mob.yawDegrees = f32(yaw % 360);
    mob.pitchDegrees = f32(pitch % 360);
    mob.pitch = -mob.pitchDegrees * (Math.PI / 180);
}

/** A box of the state's own AABB class (its getBB's). */
function stateBox(state: { getBB(): Box }, b: NativeAabb): Box {
    const out = state.getBB();
    Object.assign(out, { minX: b.min.x, minY: b.min.y, minZ: b.min.z, maxX: b.max.x, maxY: b.max.y, maxZ: b.max.z });
    return out;
}

/** The world around a vehicle, synced before its tick: its box and its reach this tick, with room to step and fall. */
/** The pushes each state has had this game tick, whose limit of 0.51 is one for all of them (Entity.pistonDeltas). */
const pistonDeltas = new WeakMap<object, { age: number; deltas: NativeVec3 }>();

interface PushedState { pos: Vec3; vel: Vec3; onGround: boolean; age: number; isCollidedHorizontally: boolean; isCollidedVertically: boolean; canSimulateMovement?: boolean }

/** Entity.move(PISTON) of a state that is no player, whose fields it updates. */
function pushEntity(engine: NativeEngine, state: PushedState, world: NativeWorld, entity: Omit<NativePistonPushedEntity, 'position' | 'velocity'>, pistons: readonly MovingPiston[]): NativePistonPushedResult {
    const earlier = pistonDeltas.get(state);
    const output = engine.binding.simulateVersionedEntityPistonPush(engine.version, world, {
        ...entity, position: vec(state.pos), velocity: vec(state.vel), onGround: state.onGround,
        collidedHorizontally: state.isCollidedHorizontally, collidedVertically: state.isCollidedVertically, clientSide: true,
        ...(earlier?.age === state.age ? { pistonDeltas: earlier.deltas } : {}),
        ...(state.canSimulateMovement === undefined ? {} : { canSimulateMovement: state.canSimulateMovement }),
    }, pistons.map(nativeMovingPiston));
    pistonDeltas.set(state, { age: state.age, deltas: output.pistonDeltas });
    state.pos.set(output.position.x, output.position.y, output.position.z);
    state.vel.set(output.velocity.x, output.velocity.y, output.velocity.z);
    state.onGround = output.onGround;
    state.isCollidedHorizontally = output.collidedHorizontally;
    state.isCollidedVertically = output.collidedVertically;
    return output;
}

function syncAround(engine: NativeEngine, world: PrismarineWorld, pos: Vec3, vel: Vec3, halfWidth: number, height: number) {
    const sync = engine.syncOf(world);
    const { box, margin } = bodyReach(pos, vel, halfWidth, height);
    sync.sync(world, box, margin);
    // (the blocks pistons move, which its bot's engine tracks: NativeEngine.movingBlocksFrom)
    const tracker = engine.movingBlockTrackerOf(world);
    if (tracker) sync.setMovingBlocks(tracker.blocks());
    return sync;
}

/** Whether a sync reads a tick of this motion. One past it drops, as NativePhysics.simulate's: docs/numeric-contract.md. */
function motionReads(pos: { x: number; y: number; z: number }, vel: { x: number; y: number; z: number }, halfWidth: number, height: number): boolean {
    const { box, margin } = bodyReach(pos, vel, halfWidth, height);
    return syncReads(box, margin);
}

/** Says whether the state's motion is past what a sync reads, and drops such a motion. */
function refusesMove(state: { pos: Vec3; vel: Vec3; moveRefused?: boolean }, halfWidth: number, height: number): boolean {
    state.moveRefused = !motionReads(state.pos, state.vel, halfWidth, height);
    if (state.moveRefused) state.vel.set(0, 0, 0);
    return state.moveRefused;
}

const cartReads = (cart: NativeMinecartState) => motionReads(cart.position, cart.velocity, 0.5, 0.7);

/**
 * How far from `from` the rays of the scene's TNT carts can reach: a blast's radius by 1.3 over steps of 0.3 that each
 * take 0.225 of it (ServerExplosion.calculateExplodedPositions), at the most power a cart's speed of 5 gives
 * (MinecartTNT.explode); 0 with none.
 */
function blastReach(carts: readonly NativeMinecartState[], from: { x: number; y: number; z: number }): number {
    return Math.max(0, ...carts.filter(cart => cart.variant === 'tnt_minecart').map(cart => {
        const power = (cart.explosionPowerBase ?? 4) + (cart.explosionSpeedFactor ?? 1) * 1.5 * 5;
        const away = Math.max(Math.abs(cart.position.x - from.x), Math.abs(cart.position.y - from.y), Math.abs(cart.position.z - from.z));
        return Math.ceil(power * 1.3 / 0.225 * 0.3) + 1 + Math.ceil(away);
    }));
}
/** A cart whose motion dropped: nothing of its tick ran. */
const cartRefused = (cart: NativeMinecartState): MoveRefusable<NativeMinecartTick> => ({
    state: { ...cart, velocity: { x: 0, y: 0, z: 0 } }, steps: [], primed: false, explosions: [], commandRan: false, moveRefused: true,
});

/**
 * Minecarts, which a client does not simulate: `simulate` is the server's tick (for a bot that predicts one, or a server
 * of its own), and it needs the level's enabled features in `rules`, as a release alone does not say which path a cart
 * takes. What a client does with a cart is `interpolate` (the old path) and `interpolateSteps` (the improvements): those
 * take what the server sent and read no world.
 */
export class NativeMinecartPhysics extends NativeEngine {
    /** One server tick of a cart (AbstractMinecart.tick). */
    simulate(state: NativeMinecartState, world: PrismarineWorld, rules: NativeMinecartRules): MoveRefusable<NativeMinecartTick> {
        if (!cartReads(state)) return cartRefused(state);
        const { position, velocity } = state;
        const sync = syncAround(this, world, position as unknown as Vec3, velocity as unknown as Vec3, 0.5, 0.7);
        return this.binding.simulateVersionedMinecartTick(this.version, sync.world, state, rules);
    }

    /**
     * One server tick of a level's carts in its tick order, among what they meet and what a hopper cart can take. The
     * world is read around the first cart, so a caller with carts far apart runs a scene for each group. What no client
     * can see (a TNT cart's randomSeed, a hopper cart's inventory, a container's slots, an end gateway's exit) is given. A
     * cart whose motion is past what a sync reads takes no part in the tick, and keeps its place in the result.
     */
    simulateScene(carts: NativeMinecartState[], others: NativeMinecartOther[], world: PrismarineWorld, rules: NativeMinecartRules, items: NativeMinecartItem[] = [], containers: NativeMinecartContainer[] = [], gateways: NativeMinecartGateway[] = []): NativeMinecartScene {
        const ticking = carts.filter(cartReads);
        if (ticking.length === 0) return { carts: carts.map(cartRefused), others, items, containers, gateways };
        const { position, velocity } = ticking[0]!;
        const sync = this.syncOf(world);
        const reach = bodyReach(position, velocity, 0.5, 0.7);
        const { margin } = reach;
        // (a cart an end gateway sends on moves from its exit in the same tick, Entity.handlePortal ahead of the move: the
        // box takes in the exits, where a sync can read that far)
        const exits = gateways.flatMap(gateway => (gateway.exit ? [gateway.exit] : []));
        const withExits = {
            minX: Math.min(reach.box.minX, ...exits.map(at => at.x)), minY: Math.min(reach.box.minY, ...exits.map(at => at.y)), minZ: Math.min(reach.box.minZ, ...exits.map(at => at.z)),
            maxX: Math.max(reach.box.maxX, ...exits.map(at => at.x + 1)), maxY: Math.max(reach.box.maxY, ...exits.map(at => at.y + 1)), maxZ: Math.max(reach.box.maxZ, ...exits.map(at => at.z + 1)),
        };
        const box = syncReads(withExits, margin) ? withExits : reach.box;
        // (a TNT cart's rays read the blocks as far as its blast reaches, where a sync can read that far)
        const wide = Math.max(margin, blastReach(ticking, position));
        sync.sync(world, box, syncReads(box, wide) ? wide : margin);
        const scene = this.binding.simulateVersionedMinecartScene(this.version, sync.world, ticking, others, rules, items, containers, gateways);
        let next = 0;
        return { ...scene, carts: carts.map(cart => (cartReads(cart) ? scene.carts[next++]! : cartRefused(cart))) };
    }

    /** A hit from outside the cart's tick (an attack, an arrow, a blast): VehicleEntity.hurtServer. */
    hurt(state: NativeMinecartState, hurt: NativeMinecartHurt, world: PrismarineWorld, rules: NativeMinecartRules): MoveRefusable<NativeMinecartTick> {
        // (the hit is no move: it lands, on a cart at rest)
        const reads = cartReads(state);
        const cart = reads ? state : cartRefused(state).state;
        const sync = syncAround(this, world, cart.position as unknown as Vec3, cart.velocity as unknown as Vec3, 0.5, 0.7);
        const tick = this.binding.simulateVersionedMinecartHurt(this.version, sync.world, cart, hurt, rules);
        return reads ? tick : { ...tick, moveRefused: true };
    }

    /** The interpolation a position packet starts under the old path, from where the client's cart is. */
    lerpTo(cart: { position: NativeVec3; yRot: number; xRot: number }, target: NativeVec3, targetYRot: number, targetXRot: number): NativeMinecartLerp {
        return { ...cart, steps: this.binding.minecartClientLerpSteps(this.version), target, targetYRot, targetXRot };
    }

    /** One client tick of the old path. */
    interpolate(cart: NativeMinecartLerp): NativeMinecartLerp {
        return this.binding.simulateMinecartClientLerp(this.version, cart);
    }

    /** One client tick of the improvements, whose steps ClientboundMoveMinecartPacket brings. */
    interpolateSteps(cart: NativeMinecartClientSteps): NativeMinecartClientSteps {
        return this.binding.simulateMinecartClientSteps(cart);
    }

    /** One client tick of what a client counts down of a cart: never what the server decides. */
    tickEffects(cart: NativeMinecartClientEffects): NativeMinecartClientEffects {
        return this.binding.simulateMinecartClientEffects(cart);
    }

    /** Whether the engine's release is before `version`. */
    olderThan(version: string): boolean {
        return this.lt(version);
    }
}

/**
 * The boat the local player steers, backed by the native engine: make its state with mineflayer-physics-utils
 * (BoatState.CREATE_FROM_ENTITY(physics, entity), EPhysicsCtx.FROM_ENTITY_STATE) and tick it as with BoatPhysics.
 */
export class NativeBoatPhysics extends NativeEngine {
    private readonly carries = new WeakMap<object, NativeBoatState>();
    private readonly unsettled = new WeakSet<object>();

    /** Takes the boat's status from where it is now at its next tick, as the status of the tick before. */
    settleStatus(ctx: PhysicsCtx): void {
        this.unsettled.add(ctx.state as object);
    }

    /** One client tick of the boat in ctx (Boat.tick), its controls the keys its rider gave it. */
    simulate<Ctx extends PhysicsCtx>(ctx: Ctx, world: PrismarineWorld): Ctx['state'] {
        const state = ctx.state as BoatState;
        const { left, right, forward, back } = state.control;
        this.tick(state, world, { forward, inputDown: back, left, right });
        return ctx.state;
    }

    /**
     * 1.8's EntityBoat.onUpdate for the boat the local player rides, pushed by its rider's yaw (degrees) and the
     * moveForward and moveStrafing its last tick left.
     */
    simulateLegacy<Ctx extends PhysicsCtx>(ctx: Ctx, world: PrismarineWorld, rider: { yaw: number; forward: number; strafe: number } | null): Ctx['state'] {
        const state = ctx.state as BoatState;
        const riderFields = { riderYaw: rider?.yaw, riderForward: rider?.forward, riderStrafe: rider?.strafe };
        this.tick(state, world, { forward: false, left: false, right: false, ...riderFields });
        return ctx.state;
    }

    /**
     * The boat's entity event 71 or 72 (AbstractBoat.handleEntityEvent, 26.3+): the server's bubble column dragged the
     * boat down (71) or launched it (72) after its 60 ticks above it, and the boat the player steers takes it too,
     * -0.7 on its vertical motion or its vertical motion set to 0.6 (handleBubbleColumnEffect, canSimulateMovement:
     * 26.3 AbstractBoat.java:316-335). Before 26.3 the server alone runs the effect (26.2 AbstractBoat.java:274-310).
     */
    handleBubbleColumnEffect(ctx: PhysicsCtx, dragDown: boolean): void {
        if (this.lt('26.3')) return;
        const { vel } = ctx.state as BoatState;
        if (dragDown) vel.set(vel.x, vel.y + -0.7, vel.z);
        else vel.set(vel.x, 0.6, vel.z);
    }

    /**
     * A moving piston block pushes the boat after its tick, as it does the player (PlayerPhysics.pushByMovingPiston): every
     * entity of the level is asked (26.3 PistonMovingBlockEntity.java:110-150), and a boat's checkFallDamage keeps its motion.
     */
    pushByMovingPiston(ctx: PhysicsCtx, world: PrismarineWorld, piston: MovingPiston): void {
        this.pushByMovingPistons(ctx, world, [piston]);
    }

    /** All the moving blocks of a tick, in the order they tick (NativePhysics.pushByMovingPistons). */
    pushByMovingPistons(ctx: PhysicsCtx, world: PrismarineWorld, pistons: readonly MovingPiston[]): void {
        const state = ctx.state as BoatState;
        if (refusesMove(state, state.halfWidth, state.height)) return;
        const sync = syncAround(this, world, state.pos, state.vel, state.halfWidth, state.height);
        const [x, y, z] = [state.pos.x, state.pos.y, state.pos.z];
        const output = pushEntity(this, state, sync.world, {
            halfWidth: state.halfWidth, height: state.height, living: false,
            ...(state.box && this.lt('1.17') ? { boundingBox: nativeBox(state.box) } : {}),
        }, pistons);
        const carry = this.carries.get(state);
        if (carry) {
            carry.position = output.position;
            // (1.21.4 alone skips the check for what a player controls: 1.21.4 Entity.java:721)
            if (!samePosition({ x, y, z }, output.position) && this.version !== '1.21.4' && !this.lt('1.9')) state.lastVerticalVelocity = output.velocity.y;
        }
        state.box = output.boundingBox && this.lt('1.17') ? stateBox(state, output.boundingBox) : undefined;
    }

    /** The paddles the boat's controls move (ServerboundPaddleBoatPacket). */
    getPaddleState(state: BoatState): { leftPaddle: boolean; rightPaddle: boolean } {
        const { left, right, forward } = state.control;
        return { leftPaddle: (right && !left) || forward, rightPaddle: (left && !right) || forward };
    }

    private tick(state: BoatState, world: PrismarineWorld, controls: Partial<NativeBoatState>): void {
        if (refusesMove(state, state.halfWidth, state.height)) {
            state.age += 1;
            return;
        }
        const sync = syncAround(this, world, state.pos, state.vel, state.halfWidth, state.height);
        const carry = this.carries.get(state);
        const fresh = carry && samePosition(carry.position, state.pos) ? carry : {};
        const settle = this.unsettled.has(state);
        this.unsettled.delete(state);
        const input: NativeBoatState = {
            ...fresh,
            position: vec(state.pos),
            velocity: vec(state.vel),
            yaw: 0,
            yawDegrees: state.yawDegrees,
            forward: false,
            left: false,
            right: false,
            ...controls,
            inWater: !!state.isInWater,
            onGround: state.onGround,
            collidedHorizontally: state.isCollidedHorizontally,
            collidedVertically: state.isCollidedVertically,
            deltaRotation: state.yawVelocity,
            landFriction: state.landFriction,
            lastVerticalVelocity: state.lastVerticalVelocity,
            ...(state.fallDistance === undefined ? {} : { fallDistance: state.fallDistance }),
            speedMultiplier: state.speedMultiplier,
            playerPassenger: state.controllingPlayer,
            ultraWarm: !!state.ultraWarm,
            noGravity: this.noGravityOf(state),
            status: settle ? undefined : BOAT_STATUSES[state.status],
            previousStatus: settle ? undefined : BOAT_STATUSES[state.previousStatus],
            boundingBox: state.box && this.lt('1.17') ? nativeBox(state.box) : undefined,
            nearbyEntities: state.nearbyEntities,
        };
        if (Number.isFinite(state.waterLevel)) input.waterLevel = state.waterLevel;
        else delete input.waterLevel;
        const output = this.binding.simulateVersionedBoatTick(this.version, sync.world, input);
        this.carries.set(state, output);
        state.pos.set(output.position.x, output.position.y, output.position.z);
        state.vel.set(output.velocity.x, output.velocity.y, output.velocity.z);
        state.onGround = !!output.onGround;
        state.isInWater = !!output.inWater;
        state.isCollidedHorizontally = !!output.collidedHorizontally;
        state.isCollidedVertically = !!output.collidedVertically;
        if (output.yawDegrees !== undefined) {
            state.yawDegrees = f32(output.yawDegrees);
            state.yaw = Math.PI - (state.yawDegrees * Math.PI) / 180;
        }
        if (output.deltaRotation !== undefined) state.yawVelocity = output.deltaRotation;
        if (output.landFriction !== undefined) state.landFriction = output.landFriction;
        if (output.lastVerticalVelocity !== undefined) state.lastVerticalVelocity = output.lastVerticalVelocity;
        if (output.fallDistance !== undefined) state.fallDistance = output.fallDistance;
        if (output.waterLevel !== undefined) state.waterLevel = output.waterLevel;
        if (output.speedMultiplier !== undefined) state.speedMultiplier = output.speedMultiplier;
        if (output.status) state.status = BOAT_STATUSES.indexOf(output.status as BoatStatusName);
        if (output.previousStatus) state.previousStatus = BOAT_STATUSES.indexOf(output.previousStatus as BoatStatusName);
        state.box = output.boundingBox && this.lt('1.17') ? stateBox(state, output.boundingBox) : undefined;
        state.worldReady = true;
        state.age += 1;
    }
}

/**
 * The mob the local player rides and steers (pig, strider, camel, horses, happy ghast, nautiluses), backed by the native
 * engine: make its state with mineflayer-physics-utils (new RiddenMobState(physics, init), EPhysicsCtx.FROM_ENTITY_STATE)
 * and tick it with simulateRidden, as with RiddenMobPhysics.
 */
export class NativeRiddenMobPhysics extends NativeEngine {
    /** A moving piston block pushes the mob after its tick (NativeBoatPhysics.pushByMovingPiston), by its own step height. */
    pushByMovingPiston(ctx: PhysicsCtx, world: PrismarineWorld, piston: MovingPiston): void {
        this.pushByMovingPistons(ctx, world, [piston]);
    }

    /** All the moving blocks of a tick, in the order they tick (NativePhysics.pushByMovingPistons). */
    pushByMovingPistons(ctx: PhysicsCtx, world: PrismarineWorld, pistons: readonly MovingPiston[]): void {
        const mob = ctx.state as RiddenMobState;
        // the size the mob last ticked with stands in for a state with none; a push is no place for a guess at a size
        const last = this.carries.get(mob);
        const halfWidth = mob.dimensions ? mob.dimensions.width / 2 : last?.halfWidth;
        const height = mob.dimensions?.height ?? last?.height;
        if (halfWidth === undefined || height === undefined) throw new Error(`a piston's push of a ${mob.type} needs its dimensions: the state has none, and it has not ticked`);
        if (refusesMove(mob, halfWidth, height)) return;
        const sync = syncAround(this, world, mob.pos, mob.vel, halfWidth, height);
        const kept = mob.keptBoundingBox && this.lt('1.17') ? mob.keptBoundingBox.bb : null;
        const { stepHeight } = this.travelAttributes(mob);
        const output = pushEntity(this, mob, sync.world, {
            halfWidth, height, living: true, fallDistance: mob.fallDistance ?? 0, ...(kept ? { boundingBox: nativeBox(kept) } : {}),
            ...(stepHeight === undefined ? {} : { stepHeight }),
        }, pistons);
        mob.fallDistance = output.fallDistance;
        const carry = this.carries.get(mob);
        if (carry) carry.position = output.position;
    }

    private readonly carries = new WeakMap<object, NativeHorseState>();

    /** Whether this engine's release has the mob and the engine steers it: what simulateRidden refuses otherwise. */
    canRide(type: string): boolean {
        const support = this.binding.entityTravelSupport(this.version, type);
        return support.family === 'ridden' && support.available;
    }

    /** One client tick of the ridden mob in ctx (a RiddenMobState), with its rider's look and input. */
    simulateRidden<Ctx extends PhysicsCtx>(ctx: Ctx, world: PrismarineWorld, rider: RiderInput): Ctx['state'] {
        const mob = ctx.state as RiddenMobState;
        // a kind the engine does not steer, or that its release does not have, stops here, before any state changes
        if (!this.canRide(mob.type)) {
            const { family } = this.binding.entityTravelSupport(this.version, mob.type);
            throw new Error(`no ridden mob simulation for ${mob.type} on ${this.version} (${family})`);
        }
        const halfWidth = (mob.dimensions?.width ?? 0.9) / 2;
        if (refusesMove(mob, halfWidth, mob.dimensions?.height ?? 1.8)) {
            mob.age += 1;
            return ctx.state;
        }
        const sync = syncAround(this, world, mob.pos, mob.vel, halfWidth, mob.dimensions?.height ?? 1.8);
        const carry = this.carries.get(mob);
        const fresh: Partial<NativeHorseState> = carry && samePosition(carry.position, mob.pos) ? carry : { previousPosition: undefined };
        const lerpPosition = this.lerpStep(mob);
        const kept = mob.keptBoundingBox && this.lt('1.17') && samePosition(mob.keptBoundingBox.pos, mob.pos) ? mob.keptBoundingBox.bb : null;
        const input: NativeHorseState = {
            ...fresh,
            entityType: mob.type,
            position: vec(mob.pos),
            velocity: vec(mob.vel),
            yaw: 0,
            yawDegrees: mob.yawDegrees ?? 0,
            pitchDegrees: mob.pitchDegrees ?? 0,
            forward: rider.zza ?? 0,
            strafe: rider.xxa ?? 0,
            jumpRequested: !!rider.jumping,
            riderYaw: rider.yawDegrees,
            riderPitch: rider.pitchDegrees,
            riderSprinting: !!rider.sprinting,
            riderJump: undefined,
            onGround: mob.onGround,
            inWater: !!mob.isInWater,
            inLava: !!mob.isInLava,
            collidedHorizontally: mob.isCollidedHorizontally,
            collidedVertically: mob.isCollidedVertically,
            fallDistance: mob.fallDistance ?? 0,
            movementSpeed: mob.movementSpeed,
            jumpStrength: mob.jumpStrength,
            flyingSpeed: mob.flyingSpeed,
            saddled: mob.saddled,
            noAi: mob.noAi,
            riddenSpeed: mob.riddenSpeed,
            ownForward: mob.ownForward,
            boosting: mob.boosting,
            boostTime: mob.boostTime,
            boostTimeTotal: mob.boostTimeTotal,
            suffocating: mob.suffocating,
            suffocatingSpeed: mob.suffocatingSpeed,
            jumpPendingScale: mob.jumpPendingScale,
            horseJumping: mob.horseJumping,
            standing: mob.standing,
            allowStandSliding: mob.allowStandSliding,
            standCounter: mob.standCounter,
            dashCooldown: mob.dashCooldown,
            dashing: mob.dashing,
            sitting: mob.camelSitting,
            poseTime: mob.poseTime,
            boundingBox: kept ? nativeBox(kept) : undefined,
            lerpPosition: lerpPosition && vec(lerpPosition),
            ultraWarm: !!mob.ultraWarm,
            noGravity: this.noGravityOf(mob),
            nearbyEntities: mob.nearbyEntities,
            halfWidth: mob.dimensions ? mob.dimensions.width / 2 : undefined,
            height: mob.dimensions?.height,
            stuckSpeedMultiplier: mob.stuckSpeedMultiplier ? vec(mob.stuckSpeedMultiplier) : fresh.stuckSpeedMultiplier,
            slowFalling: this.effectAmplifierByName('SlowFalling', mob.effects ?? {}) !== undefined,
            levitationAmplifier: this.effectAmplifierByName('Levitation', mob.effects ?? {}),
            dolphinsGrace: this.effectAmplifierByName('DolphinsGrace', mob.effects ?? {}) !== undefined,
            weaving: this.effectAmplifierByName('Weaving', mob.effects ?? {}) !== undefined,
            ...this.travelAttributes(mob),
            // the mob's Jump Boost, which its riding jump adds 0.1F a level to (1.21.11 AbstractHorse.java:751-752,
            // LivingEntity.java:2258-2264; 1.9.4 AbstractHorse.java:1047-1050)
            jumpBoostAmplifier: this.effectAmplifierByName('JumpBoost', mob.effects ?? {}),
        };
        const output = this.binding.simulateVersionedHorseTick(this.version, sync.world, input);
        this.carries.set(mob, output);
        // Camel.standUp's setPose refreshes the dimensions in the tick (1.21.11 Camel.java:228-234, 529-534, Entity.java:
        // 3230-3233), and the next tick measures the size it is given in the pose it then has
        if (mob.dimensions && output.sitting !== undefined && output.sitting !== mob.camelSitting
            && output.halfWidth !== undefined && output.height !== undefined) {
            mob.dimensions = { width: output.halfWidth * 2, height: output.height };
        }
        mob.pos.set(output.position.x, output.position.y, output.position.z);
        mob.vel.set(output.velocity.x, output.velocity.y, output.velocity.z);
        mob.onGround = output.onGround;
        mob.isInWater = output.inWater;
        mob.isInLava = !!output.inLava;
        mob.isCollidedHorizontally = !!output.collidedHorizontally;
        mob.isCollidedVertically = !!output.collidedVertically;
        mob.fallDistance = output.fallDistance ?? 0;
        // on the state it outlives a move the engine did not make: only Entity.move and makeStuckInBlock touch it, so a
        // correction of the mob's position keeps it (26.3 Entity.java:783-790, 3072-3075)
        if (output.stuckSpeedMultiplier) {
            const { x, y, z } = output.stuckSpeedMultiplier;
            if (mob.stuckSpeedMultiplier) mob.stuckSpeedMultiplier.set(x, y, z);
            else mob.stuckSpeedMultiplier = mob.vel.clone().set(x, y, z);
        }
        if (output.yawDegrees !== undefined) mob.yawDegrees = output.yawDegrees;
        if (output.pitchDegrees !== undefined) mob.pitchDegrees = output.pitchDegrees;
        mob.riddenSpeed = output.riddenSpeed ?? mob.riddenSpeed;
        mob.ownForward = output.ownForward ?? mob.ownForward;
        mob.boosting = output.boosting ?? mob.boosting;
        mob.boostTime = output.boostTime ?? mob.boostTime;
        mob.boostTimeTotal = output.boostTimeTotal ?? mob.boostTimeTotal;
        mob.suffocating = output.suffocating ?? mob.suffocating;
        mob.jumpPendingScale = output.jumpPendingScale ?? mob.jumpPendingScale;
        mob.horseJumping = output.horseJumping ?? mob.horseJumping;
        mob.standing = output.standing ?? mob.standing;
        mob.allowStandSliding = output.allowStandSliding ?? mob.allowStandSliding;
        mob.standCounter = output.standCounter ?? mob.standCounter;
        mob.dashCooldown = output.dashCooldown ?? mob.dashCooldown;
        mob.dashing = output.dashing ?? mob.dashing;
        mob.camelSitting = output.sitting ?? mob.camelSitting;
        mob.poseTime = output.poseTime ?? mob.poseTime;
        if (this.lt('1.17') && output.boundingBox) {
            const { width, height } = mob.dimensions ?? { width: halfWidth * 2, height: 1.8 };
            mob.keptBoundingBox = { bb: stateBox(mob, output.boundingBox), pos: mob.pos.clone(), width, height };
        } else {
            mob.keptBoundingBox = null;
        }
        if (HORSES.has(mob.type)) this.animateStand(mob);
        mob.lastOnGround = mob.onGround;
        mob.age += 1;
        return ctx.state;
    }

    /** The attributes of LivingEntity.travel the mob's state has; the engine takes the mob's base for the others. */
    private travelAttributes(mob: RiddenMobState): Partial<NativeHorseState> {
        const attributes = mob.attributes ?? {};
        return {
            gravity: this.attributeValueOf(attributes, 'gravity'),
            stepHeight: this.attributeValueOf(attributes, 'stepHeight'),
            waterMovementEfficiency: this.attributeValueOf(attributes, 'waterMovementEfficiency'),
            airDragModifier: this.attributeValueOf(attributes, 'airDragModifier'),
            frictionModifier: this.attributeValueOf(attributes, 'frictionModifier'),
        };
    }

    /**
     * AbstractHorse.tick's stand animation after the move, which seats the rider back and up: toward 1 while the horse
     * rears, back toward 0 after.
     */
    private animateStand(mob: RiddenMobState): void {
        const anim = mob.standAnim;
        mob.standAnimO = anim;
        if (mob.standing) {
            mob.standAnim = Math.min(f32(anim + f32(f32(f32(1 - anim) * f32(0.4)) + f32(0.05))), 1);
        } else {
            const cube = f32(f32(f32(f32(0.8) * anim) * anim) * anim);
            mob.standAnim = Math.max(f32(anim + f32(f32(f32(cube - anim) * f32(0.6)) - f32(0.05))), 0);
        }
    }

    /**
     * LivingEntity.aiStep's step toward a teleport's target (Mth.lerp of 1 / the steps left from 1.20.2, a division
     * before; from 26.3 SteppedInterpolationHandler's float share of the step's ticks), with the mob's own move and turn
     * since the last step carried into the target from 1.21.5 (InterpolationHandler.interpolate). The mob turns now; the
     * engine puts it where the step leads after the fluids of its tick start.
     */
    private lerpStep(mob: RiddenMobState): Vec3 | undefined {
        const t = mob.lerpTarget;
        if (!(mob.lerpSteps > 0) || !t) return undefined;
        const n = mob.lerpSteps;
        const p = mob.pos;
        if (!this.lt('1.21.5') && mob.lerpPrevious) {
            const dx = p.x - mob.lerpPrevious.x;
            const dy = p.y - mob.lerpPrevious.y;
            const dz = p.z - mob.lerpPrevious.z;
            t.set(t.x + dx, t.y + dy, t.z + dz);
            mob.lerpStart?.set(mob.lerpStart.x + dx, mob.lerpStart.y + dy, mob.lerpStart.z + dz);
        }
        const turning = mob.lerpYaw !== null;
        if (turning && !this.lt('1.21.5') && mob.lerpPreviousRotation) {
            const yawTurn = f32((mob.yawDegrees ?? 0) - mob.lerpPreviousRotation.yaw);
            const pitchTurn = f32(mob.pitchDegrees - mob.lerpPreviousRotation.pitch);
            mob.lerpYaw = f32(mob.lerpYaw! + yawTurn);
            mob.lerpPitch = f32(mob.lerpPitch + pitchTurn);
            if (mob.lerpStartRotation) {
                mob.lerpStartRotation.yaw = f32(mob.lerpStartRotation.yaw + yawTurn);
                mob.lerpStartRotation.pitch = f32(mob.lerpStartRotation.pitch + pitchTurn);
            }
        }
        const yaw = mob.yawDegrees ?? 0;
        const pitch = mob.pitchDegrees;
        const stepped = p.clone();
        if (!this.lt('26.3') && mob.lerpStart) {
            const s = mob.lerpStart;
            const a = f32(mob.lerpTicks / 3);
            const from = mob.lerpStartRotation;
            if (mob.lerpTicks < 3) {
                stepped.set(s.x + (t.x - s.x) * a, s.y + (t.y - s.y) * a, s.z + (t.z - s.z) * a);
                if (turning && from) setRot(mob, f32(from.yaw + f32(a * wrapDegreesF(f32(mob.lerpYaw! - from.yaw)))), f32(from.pitch + f32(a * f32(mob.lerpPitch - from.pitch))));
            } else {
                stepped.set(t.x, t.y, t.z);
                if (turning) setRot(mob, mob.lerpYaw!, mob.lerpPitch);
            }
            mob.lerpTicks += 1;
        } else if (this.lt('1.20.2')) {
            stepped.set(p.x + (t.x - p.x) / n, p.y + (t.y - p.y) / n, p.z + (t.z - p.z) / n);
            if (turning) setRot(mob, f32(yaw + f32(f32(wrapDegreesD(mob.lerpYaw! - yaw)) / n)), f32(pitch + f32((mob.lerpPitch - pitch) / n)));
        } else {
            stepped.set(p.x + (1 / n) * (t.x - p.x), p.y + (1 / n) * (t.y - p.y), p.z + (1 / n) * (t.z - p.z));
            if (turning) setRot(mob, f32(yaw + (1 / n) * wrapDegreesD(mob.lerpYaw! - yaw)), f32(pitch + (1 / n) * (mob.lerpPitch - pitch)));
        }
        mob.lerpSteps -= 1;
        mob.lerpPrevious = stepped.clone();
        mob.lerpPreviousRotation = { yaw: mob.yawDegrees ?? 0, pitch: mob.pitchDegrees };
        if (mob.lerpSteps <= 0) mob.lerpYaw = null;
        mob.keptBoundingBox = null;
        return stepped;
    }

    /**
     * A server teleport of the mob its rider controls (ClientPacketListener.handleTeleportEntity, which applies it to a
     * controlled vehicle from 1.21.2): the mob interpolates there over 3 ticks, or is put there at once when it is more
     * than 64 blocks away.
     */
    teleportRidden(mob: RiddenMobState, target: Vec3, rotation?: Rotation): void {
        const dx = mob.pos.x - target.x;
        const dy = mob.pos.y - target.y;
        const dz = mob.pos.z - target.z;
        if (dx * dx + dy * dy + dz * dz <= 4096) {
            this.interpolateRidden(mob, target, 3, rotation);
            return;
        }
        mob.pos.set(target.x, target.y, target.z);
        if (rotation) setRot(mob, rotation.yaw, rotation.pitch);
        mob.lerpSteps = 0;
        mob.lerpTarget = null;
        mob.lerpYaw = null;
        mob.keptBoundingBox = null;
    }

    /**
     * The mob eases to target over the steps left (LivingEntity.lerpTo, InterpolationHandler.interpolateTo), turning to
     * the rotation given; without one, from 1.21.5, to the rotation it already heads to, else its own.
     */
    interpolateRidden(mob: RiddenMobState, target: Vec3, steps: number, rotation?: Rotation): void {
        const heading = mob.lerpSteps > 0 && mob.lerpYaw !== null ? { yaw: mob.lerpYaw, pitch: mob.lerpPitch } : null;
        const to = rotation ?? (this.lt('1.21.5') ? null : heading ?? { yaw: mob.yawDegrees ?? 0, pitch: mob.pitchDegrees });
        mob.lerpTarget = target.clone();
        mob.lerpSteps = steps;
        mob.lerpPrevious = mob.pos.clone();
        mob.lerpYaw = to ? f32(to.yaw) : null;
        mob.lerpPitch = to ? f32(to.pitch) : 0;
        mob.lerpPreviousRotation = { yaw: mob.yawDegrees ?? 0, pitch: mob.pitchDegrees };
        // 26.3: one step of the update interval (3 ticks) from where the mob is, its first tick a third of the way
        if (!this.lt('26.3')) {
            mob.lerpStart = mob.pos.clone();
            mob.lerpStartRotation = { yaw: mob.yawDegrees ?? 0, pitch: mob.pitchDegrees };
            mob.lerpTicks = 1 + (3 - steps);
        }
    }

    /** Where the mob is headed (Entity.lerpTargetX, what the vehicle move reports): the teleport's target while it runs. */
    lerpTargetOf(mob: RiddenMobState): Vec3 {
        return mob.lerpSteps > 0 && mob.lerpTarget ? mob.lerpTarget.clone() : mob.pos.clone();
    }

    /**
     * The rotation the vehicle move reports: from 1.21.5 the one the interpolation heads to while it runs
     * (Entity.lerpTargetYRot / XRot), else the mob's own.
     */
    lerpTargetRotationOf(mob: RiddenMobState): Rotation {
        if (!this.lt('1.21.5') && mob.lerpSteps > 0 && mob.lerpYaw !== null) return { yaw: mob.lerpYaw, pitch: mob.lerpPitch };
        return { yaw: mob.yawDegrees ?? 0, pitch: mob.pitchDegrees };
    }

    /**
     * The rider's jump released (LocalPlayer.aiStep calls onPlayerJump with the charge in percent, after the mob's
     * tick): a horse's jump at its next tick on the ground (and its rearing where the client runs it), a camel's or
     * nautilus's dash.
     */
    riderJump(mob: RiddenMobState, power: number): void {
        const percent = Math.max(0, power);
        const scale = percent >= 90 ? 1 : f32(f32(0.4) + f32(f32(f32(0.4) * percent) / f32(90)));
        if (HORSES.has(mob.type)) {
            if (power >= 0) {
                mob.allowStandSliding = true;
                // AbstractHorse.standIfPossible where the client runs the horse's rearing (not a llama's)
                const standsLocally = this.lt('1.19.4') || !this.lt('1.21.5');
                // (not one without AI from 1.21.5: standIfPossible asks isEffectiveAi, 1.21.5 Entity.java:3242-3244,
                // AbstractHorse.java:674-680; 26.3 :680-684)
                const effectiveAi = !(mob.noAi && !this.lt('1.21.5'));
                if (standsLocally && effectiveAi && mob.type !== 'llama' && mob.type !== 'trader_llama') {
                    mob.standing = true;
                    mob.standCounter = this.lt('1.21.6') ? 1 : 20;
                }
            }
            mob.jumpPendingScale = scale;
            return;
        }
        if (NAUTILUSES.has(mob.type)) {
            if (mob.dashCooldown <= 0) mob.jumpPendingScale = scale;
            return;
        }
        if (mob.type === 'camel' && canCamelJump(mob) && mob.dashCooldown <= 0 && mob.onGround) mob.jumpPendingScale = scale;
    }

    /**
     * The DASH flag the server synced: a nautilus or camel whose cooldown had run out starts it again, 40 ticks or 55
     * (onSyncedDataUpdated: 1.19.3 Camel.java:608-611, 1.21.11 Camel.java:573-576, 26.3 Camel.java:580-583,
     * AbstractNautilus.java:333-336).
     */
    syncDash(mob: RiddenMobState, dashing: boolean): void {
        mob.dashing = dashing;
        if (mob.dashCooldown !== 0) return;
        if (NAUTILUSES.has(mob.type)) mob.dashCooldown = 40;
        else if (mob.type === 'camel') mob.dashCooldown = 55;
    }
}
