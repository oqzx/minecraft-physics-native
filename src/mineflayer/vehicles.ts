// The vehicles the local player steers, for mineflayer-physics-utils' BoatState and RiddenMobState, as its BoatPhysics
// and RiddenMobPhysics tick them: the native engine's versioned boat tick (Boat.tick; 1.8's EntityBoat.onUpdate) and
// ridden mob tick (LivingEntity.travelRidden of the mob named by its type). The engine's own fields that the states have
// no place for are carried from one tick to the next while the vehicle stays where the engine left it.
import type { NativeAabb, NativeBoatState, NativeHorseState, NativeVec3 } from '../native';
import { NativeEngine } from './base';
import type { Box, Controls, KeptBoundingBox, PhysicsCtx, Vec3 } from './types';
import type { PrismarineWorld } from './world';

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
    controllingPlayer: boolean;
    speedMultiplier: number;
    worldReady: boolean;
    halfWidth: number;
    height: number;
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
    dimensions?: { width: number; height: number };
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
function syncAround(engine: NativeEngine, world: PrismarineWorld, pos: Vec3, vel: Vec3, halfWidth: number, height: number) {
    const sync = engine.syncOf(world);
    const reach = Math.max(Math.abs(vel.x), Math.abs(vel.y), Math.abs(vel.z));
    sync.sync(world, { minX: pos.x - halfWidth, minY: pos.y, minZ: pos.z - halfWidth, maxX: pos.x + halfWidth, maxY: pos.y + height, maxZ: pos.z + halfWidth }, 3 + Math.ceil(reach));
    return sync;
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

    /** The paddles the boat's controls move (ServerboundPaddleBoatPacket). */
    getPaddleState(state: BoatState): { leftPaddle: boolean; rightPaddle: boolean } {
        const { left, right, forward } = state.control;
        return { leftPaddle: (right && !left) || forward, rightPaddle: (left && !right) || forward };
    }

    private tick(state: BoatState, world: PrismarineWorld, controls: Partial<NativeBoatState>): void {
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
            speedMultiplier: state.speedMultiplier,
            playerPassenger: state.controllingPlayer,
            status: settle ? undefined : BOAT_STATUSES[state.status],
            previousStatus: settle ? undefined : BOAT_STATUSES[state.previousStatus],
            boundingBox: state.box && this.lt('1.17') ? nativeBox(state.box) : undefined,
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
    private readonly carries = new WeakMap<object, NativeHorseState>();

    /** One client tick of the ridden mob in ctx (a RiddenMobState), with its rider's look and input. */
    simulateRidden<Ctx extends PhysicsCtx>(ctx: Ctx, world: PrismarineWorld, rider: RiderInput): Ctx['state'] {
        const mob = ctx.state as RiddenMobState;
        const halfWidth = (mob.dimensions?.width ?? 0.9) / 2;
        const sync = syncAround(this, world, mob.pos, mob.vel, halfWidth, mob.dimensions?.height ?? 1.8);
        const carry = this.carries.get(mob);
        const fresh = carry && samePosition(carry.position, mob.pos) ? carry : { previousPosition: undefined };
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
        };
        const output = this.binding.simulateVersionedHorseTick(this.version, sync.world, input);
        this.carries.set(mob, output);
        mob.pos.set(output.position.x, output.position.y, output.position.z);
        mob.vel.set(output.velocity.x, output.velocity.y, output.velocity.z);
        mob.onGround = output.onGround;
        mob.isInWater = output.inWater;
        mob.isInLava = !!output.inLava;
        mob.isCollidedHorizontally = !!output.collidedHorizontally;
        mob.isCollidedVertically = !!output.collidedVertically;
        mob.fallDistance = output.fallDistance ?? 0;
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
                if (standsLocally && mob.type !== 'llama' && mob.type !== 'trader_llama') {
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
        if (mob.type === 'camel' && mob.dashCooldown <= 0 && mob.onGround) mob.jumpPendingScale = scale;
    }

    /** The DASH flag the server synced: a nautilus whose cooldown had run out starts it again. */
    syncDash(mob: RiddenMobState, dashing: boolean): void {
        mob.dashing = dashing;
        if (NAUTILUSES.has(mob.type) && mob.dashCooldown === 0) mob.dashCooldown = 40;
    }
}
