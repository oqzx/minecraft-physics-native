// The local player's tick for mineflayer-physics-utils' EPhysicsCtx, the engine interface mineflayer's physics plugin
// drives (IPhysics.simulate, as BotcraftPhysics). The native engine moves the player (simulateVersionedTick); around it
// the adapter makes the client's own decisions that the engine leaves to its caller, as LocalPlayer.aiStep makes them
// (a port of BotcraftPhysics' localPlayerAIStep): crouching, sprinting, flying and the elytra's start, swimming, the
// pose and, before 1.14, the player's height.
import type { IPhysics } from '@nxg-org/mineflayer-physics-util/dist/physics/engines/IPhysics';
import {
    NATIVE_PLAYER_ENTITY_LENGTH,
    NativePlayerInput,
    NativePlayerInputFlag,
    NativePlayerOutput,
    NativePlayerOutputFlag,
    type NativeMovingPiston,
    type NativePlayer,
    type NativeSimulationState,
    type NativeVec3,
} from '../native';
import { NativeEngine, type NativeEngineOptions, type PhysicsRegistry } from './base';
import { EFFECT, FLUID_LAVA, type DescribedBlock } from './blocks';
import { floatRadians, mthFor, viewVector, type Mth } from './mth';
import type { Box, PhysicsCtx, PlayerState, Vec3 } from './types';
import type { NativeWorldSync, PrismarineWorld } from './world';

const f32 = Math.fround;

/** mineflayer-physics-utils' PlayerPoses. */
export const Pose = { STANDING: 0, FALL_FLYING: 1, SLEEPING: 2, SWIMMING: 3, SPIN_ATTACK: 4, SNEAKING: 5, LONG_JUMPING: 6, DYING: 7 } as const;

/** Player.getDimensions per pose, in float. */
const POSE_SIZES: Record<number, { width: number; height: number }> = {
    0: { width: f32(0.6), height: f32(1.8) },
    1: { width: f32(0.6), height: f32(0.6) },
    2: { width: f32(0.2), height: f32(0.2) },
    3: { width: f32(0.6), height: f32(0.6) },
    4: { width: f32(0.6), height: f32(0.6) },
    5: { width: f32(0.6), height: f32(1.5) },
    6: { width: f32(0.6), height: f32(1.8) },
    7: { width: f32(0.2), height: f32(0.2) },
};

const BLINDNESS = 6;

/** The movement speed modifiers of the speed and slowness effects (MobEffect.addAttributeModifier), by their id (a UUID
 * before 1.21), and the amount per level. */
const EFFECT_SPEED_MODIFIERS = {
    speed: { ids: ['minecraft:effect.speed', '91aeaa56-376b-4498-935b-2f7f68070635'], perLevel: 0.20000000298023224 },
    slowness: { ids: ['minecraft:effect.slowness', '7107de5e-7ce8-4030-940e-514c1f160890'], perLevel: -0.15000000596046448 },
};

/** A moving piston block as mineflayer-physics-utils' BotcraftPhysics.pushByMovingPiston takes it. */
export interface MovingPiston {
    pos: [number, number, number];
    direction: [number, number, number];
    extending: boolean;
    source: boolean;
    progress: number;
    shapes: number[][];
}

/** Direction.from3DDataValue: down, up, north, south, west, east. */
const DIRECTIONS = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/** LivingEntity.onClimbable's blocks: the #climbable tag from 1.16, ladder, vine and scaffolding before it. */
const CLIMBABLE = new Set(['ladder', 'vine', 'scaffolding', 'weeping_vines', 'weeping_vines_plant', 'twisting_vines', 'twisting_vines_plant', 'cave_vines', 'cave_vines_plant']);

type Size = { width: number; height: number };

/**
 * The engine's own copy of a player (a NativePlayer, which keeps what mineflayer-physics-utils' state has no field for:
 * the position the tick started from, its counters), with what the adapter sent it.
 */
interface Carry {
    player: NativePlayer;
    /** Where the engine left the player: a position set since (a teleport) drops the position the tick started from. */
    position: Vec3Like;
    /** The inputs that rarely change (attributes, effects, enchantments), sent with the whole state when they do. */
    slow: Partial<NativeSimulationState>;
    /** The box the engine keeps before 1.17 (null without one), and the size the player moved with. */
    kept: Box | null;
    size: Size;
}

type Vec3Like = { x: number; y: number; z: number };

const In = NativePlayerInput;
const InFlag = NativePlayerInputFlag;
const Out = NativePlayerOutput;
const OutFlag = NativePlayerOutputFlag;

/** The kinds of the nearby entities in a NativePlayer's entities array. */
const ENTITY_KINDS: Record<string, number> = { inert: 0, living: 1, boat: 2, shulker: 3, happy_ghast: 4 };

const sameInputs = (a: Partial<NativeSimulationState>, b: Partial<NativeSimulationState>) => {
    const keys = Object.keys(a) as (keyof NativeSimulationState)[];
    return keys.length === Object.keys(b).length && keys.every(key => Object.is(a[key], b[key]));
};
const sameBox = (a: Box | null, b: Box | null) => a === b || (!!a && !!b && a.minX === b.minX && a.minY === b.minY
    && a.minZ === b.minZ && a.maxX === b.maxX && a.maxY === b.maxY && a.maxZ === b.maxZ);

const box = (minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): Box => ({ minX, minY, minZ, maxX, maxY, maxZ });
const shrink = (b: Box, by: number): Box => box(b.minX + by, b.minY + by, b.minZ + by, b.maxX - by, b.maxY - by, b.maxZ - by);
const nativeBox = (b: Box) => ({ min: { x: b.minX, y: b.minY, z: b.minZ }, max: { x: b.maxX, y: b.maxY, z: b.maxZ } });
const vec = (v: { x: number; y: number; z: number }): NativeVec3 => ({ x: v.x, y: v.y, z: v.z });
const samePosition = (a: { x: number; y: number; z: number }, b: { x: number; y: number; z: number }) => a.x === b.x && a.y === b.y && a.z === b.z;
const isBoatName = (name: string) => /(^|_)(boat|raft)$/.test(name);
const isWaterish = (block: DescribedBlock | null): block is DescribedBlock => !!block?.fluid && block.fluid.type !== FLUID_LAVA;

/**
 * The player physics of one release, backed by the native engine. Make the states with mineflayer-physics-utils
 * (EPhysicsCtx.FROM_BOT(physics, bot)) and tick them with simulate(ctx, world), as with BotcraftPhysics.
 */
export class NativePhysics extends NativeEngine implements IPhysics {
    readonly mth: Mth;
    private readonly carries = new WeakMap<object, Carry>();
    private readonly input = new Float64Array(In.LENGTH);

    constructor(registry: PhysicsRegistry, options: NativeEngineOptions = {}) {
        super(registry, options);
        this.mth = mthFor(!this.lt('1.21.11'));
    }

    /** One client tick of the player in ctx (a PlayerState), its world read through world.getBlock. */
    simulate<Ctx extends PhysicsCtx>(ctx: Ctx, world: PrismarineWorld): Ctx['state'] {
        const state = ctx.state as PlayerState;
        state.attributes ??= {};
        this.tick(state, world);
        state.age += 1;
        return ctx.state;
    }

    /**
     * A moving piston block pushes the player after its tick (PistonMovingBlockEntity.tick: the client ticks them after
     * its entities), each in turn: the engine's push of simulateVersionedTick's movingPistons.
     */
    pushByMovingPiston(ctx: PhysicsCtx, world: PrismarineWorld, piston: MovingPiston): void {
        const state = ctx.state as PlayerState;
        const carry = this.carries.get(state);
        if (!carry || !samePosition(carry.position, state.pos)) return;
        const [dx, dy, dz] = piston.direction;
        const movingPiston: NativeMovingPiston = {
            x: piston.pos[0],
            y: piston.pos[1],
            z: piston.pos[2],
            facing: DIRECTIONS.findIndex(([x, y, z]) => x === dx && y === dy && z === dz),
            extending: piston.extending,
            source: piston.source,
            progress: piston.progress,
            shapes: piston.shapes.map(([minX, minY, minZ, maxX, maxY, maxZ]) => nativeBox(box(minX, minY, minZ, maxX, maxY, maxZ))),
        };
        this.takeOutput(state, carry, carry.player.pushByMovingPistons(this.syncOf(world).world, [movingPiston]));
    }

    /**
     * Entity.refreshDimensions on 1.14 - 1.16, which runs whenever the pose is set from synced entity data: the kept
     * bounding box is refitted to the pose's size on its lower corner (around the position when narrower).
     */
    refreshDimensions(state: PlayerState): void {
        const kept = state.keptBoundingBox;
        if (this.lt('1.14') || !this.lt('1.17') || kept == null || !samePosition(kept.pos, state.pos)) return;
        const { width, height } = this.poseSize(state.pose, state.scale);
        const { bb } = kept;
        const refitted = width < kept.width
            ? this.boxAround(state.pos, width, height)
            : box(bb.minX, bb.minY, bb.minZ, bb.minX + width, bb.minY + height, bb.minZ + width);
        state.keptBoundingBox = { bb: this.stateBox(state, refitted), pos: kept.pos, width, height };
    }

    private tick(state: PlayerState, world: PrismarineWorld): void {
        const sync = this.syncOf(world);
        const control = state.control;
        state.scale = this.lt('1.20.5') ? 1 : f32(this.attributeValueOf(state.attributes, 'scale') ?? 1);

        // the use key is handled before the entities tick: releasing a riptide trident launches the player
        let riptideLaunch: number | undefined;
        if (state.isUsingItem) {
            state.itemUseTicks += 1;
        } else {
            const used = state.itemUseTicks;
            state.itemUseTicks = 0;
            if (!this.lt('1.13') && used >= 10 && state.riptide > 0 && state.isInWater) {
                riptideLaunch = state.riptide;
                state.autoSpinAttackTicks = 20;
            }
        }
        // the attached rockets the client ticks before the player, which boost it only while it glides
        let rocketsBefore = 0;
        if (state.fireworkRocketsBeforePlayer > 0 && state.fireworkRocketDuration > 0) {
            if (state.fallFlying) rocketsBefore = state.fireworkRocketsBeforePlayer;
            else state.fireworkRocketDuration = 0;
        }

        sync.sync(world, this.reachOf(state, riptideLaunch !== undefined), 0);

        const wasInWater = state.isInWater;
        const wasInLava = state.isInLava;
        this.updateFluids(state, sync);

        // LocalPlayer.aiStep
        const sprintedBefore = state.sprinting;
        const forwardKey = Number(control.forward) - Number(control.back);
        const strafeKey = Number(control.left) - Number(control.right);
        const heading = { forward: forwardKey, strafe: strafeKey };
        state.heading = heading;
        state.autoJumping = state.autoJumpTime > 0;
        this.updateSwimming(state, sync);
        state.wasInPowderSnow = state.isInPowderSnow;
        const movingSlowly = this.inputsToCrouch(state, heading, sync);
        this.inputsToSprint(state, heading);
        this.inputsToFly(state, sync);
        state.flyJumpTriggerTime = Math.max(0, state.flyJumpTriggerTime - 1);
        // (from 1.17 the pose is refreshed before the move, so the first tick of a glide moves its own box)
        if (this.gt('1.13.2') && !this.lt('1.17')) this.updatePoses(state, sync);

        let rocketsAfter = 0;
        if (state.fireworkRocketDuration > 0) {
            if (state.fallFlying) rocketsAfter = Math.max(0, (state.fireworkRockets > 0 ? state.fireworkRockets : 1) - state.fireworkRocketsBeforePlayer);
            else state.fireworkRocketDuration = 0;
        }

        const kept = this.keptFor(state);
        const size = this.moveSize(state, kept !== null);
        const bit = (set: boolean, flag: number) => (set ? flag : 0);
        const flags = bit(control.jump, InFlag.JUMP_REQUESTED) | bit(state.sprinting, InFlag.SPRINTING)
            | bit(sprintedBefore, InFlag.WAS_SPRINTING) | bit(this.hasSprintModifier(state), InFlag.SPRINT_MODIFIER)
            | bit(control.sneak, InFlag.SNEAKING) | bit(movingSlowly, InFlag.MOVING_SLOWLY)
            | bit(state.swimming, InFlag.SWIMMING) | bit(state.flying, InFlag.FLYING)
            | bit(state.fallFlying, InFlag.FALL_FLYING) | bit(state.isUsingItem, InFlag.USING_ITEM)
            | bit(state.autoJumpEnabled, InFlag.AUTO_JUMP) | bit(state.gameMode === 'spectator', InFlag.NO_PHYSICS)
            | bit(state.onGround, InFlag.ON_GROUND) | bit(state.isCollidedHorizontally, InFlag.COLLIDED_HORIZONTALLY)
            | bit(!!state.isCollidedHorizontallyMinor, InFlag.MINOR_HORIZONTAL_COLLISION)
            | bit(state.isCollidedVertically, InFlag.COLLIDED_VERTICALLY)
            | bit(state.isCollidedVertically && state.onGround, InFlag.VERTICAL_COLLISION_BELOW)
            | bit(wasInWater, InFlag.IN_WATER) | bit(wasInLava, InFlag.IN_LAVA) | bit(state.isInPowderSnow, InFlag.IN_POWDER_SNOW)
            | bit(!!state.removed, InFlag.REMOVED);
        const yaw = this.yawDegrees(state);
        const pitch = this.pitchDegrees(state);
        const look = viewVector(this.mth, this.lt('1.13'), pitch, yaw);
        const { pos, vel, stuckSpeedMultiplier: stuck } = state;
        const input = this.input;
        input[In.POSITION] = pos.x;
        input[In.POSITION + 1] = pos.y;
        input[In.POSITION + 2] = pos.z;
        input[In.VELOCITY] = vel.x;
        input[In.VELOCITY + 1] = vel.y;
        input[In.VELOCITY + 2] = vel.z;
        input[In.YAW] = floatRadians(yaw);
        input[In.PITCH] = floatRadians(pitch);
        input[In.LOOK] = look.x;
        input[In.LOOK + 1] = look.y;
        input[In.LOOK + 2] = look.z;
        input[In.STRAFE] = strafeKey;
        input[In.FORWARD] = forwardKey;
        input[In.HALF_WIDTH] = size.width / 2;
        input[In.HEIGHT] = size.height;
        input[In.EYE_HEIGHT] = f32(this.eyeHeight(state));
        input[In.FALL_DISTANCE] = state.fallDistance;
        input[In.JUMP_TICKS] = state.jumpTicks;
        input[In.AUTO_JUMP_TIME] = state.autoJumpTime;
        input[In.STUCK_SPEED_MULTIPLIER] = stuck.x;
        input[In.STUCK_SPEED_MULTIPLIER + 1] = stuck.y;
        input[In.STUCK_SPEED_MULTIPLIER + 2] = stuck.z;
        input[In.RIPTIDE_LAUNCH] = riptideLaunch ?? 0;
        input[In.FIREWORK_ROCKETS] = rocketsBefore + rocketsAfter;
        input[In.FIREWORK_ROCKETS_BEFORE_PLAYER] = rocketsBefore;
        input[In.FLYING_SPEED] = state.flySpeed ?? f32(0.05);
        input[In.ITEM_USE_SPEED_MULTIPLIER] = state.useItemSpeedMultiplier ?? f32(0.2);

        const carry = this.carryFor(state, kept);
        input[In.FLAGS] = flags | bit(samePosition(carry.position, pos), InFlag.UNMOVED);
        const flyingDuringMove = state.flying;
        // isInWeb: the move is slowed by a cobweb, berry bush or powder snow (Entity.isInWeb before 1.14), unless the lift
        // of a riptide launch from the ground took the slowdown
        const slowed = stuck.x * stuck.x + stuck.y * stuck.y + stuck.z * stuck.z > 1e-7 && !(riptideLaunch && wasInWater && state.onGround);
        carry.size = size;
        this.takeOutput(state, carry, carry.player.tick(sync.world, input, this.entitiesOf(state)));
        if (state.gameMode !== 'spectator') state.isInWeb = slowed;
        state.pushedEntities = this.pushedEntities(state, carry);

        // LivingEntity.aiStep after the move: the spin attack runs down, and ends on hitting a wall
        if (state.autoSpinAttackTicks > 0) {
            state.autoSpinAttackTicks -= 1;
            if (state.isCollidedHorizontally) state.autoSpinAttackTicks = 0;
        }
        if (flyingDuringMove) state.fallFlying = false;
        state.onClimbable = this.isInClimbable(state, sync);
        if (state.onGround && state.flying && state.gameMode !== 'spectator') state.flying = false;
        if (this.gt('1.13.2')) this.updatePoses(state, sync);
        else this.updateLegacySize(state, sync);
        if (state.fireworkRocketDuration > 0) state.fireworkRocketDuration -= 1;

        state.lastOnGround = state.onGround;
        state.prevHeading.forward = heading.forward;
        state.prevHeading.strafe = heading.strafe;
        state.prevControl.jump = control.jump;
        state.prevControl.sneak = control.sneak;
    }

    /**
     * The engine's copy of the player, brought up to date with what the adapter does not send every tick: the whole
     * state when the attributes, effects or enchantments changed, the kept box when it is not the engine's.
     */
    private carryFor(state: PlayerState, kept: Box | null): Carry {
        const slow = this.slowInputs(state);
        const carry = this.carries.get(state);
        if (!carry) {
            const created: NativeSimulationState = {
                ...slow,
                position: vec(state.pos),
                velocity: vec(state.vel),
                halfWidth: 0.3,
                height: 1.8,
                onGround: state.onGround,
                collidedHorizontally: false,
                collidedVertically: false,
                isPlayer: true,
                controlledByLocalInstance: true,
                boundingBox: kept ? nativeBox(kept) : undefined,
            };
            const player = new this.binding.NativePlayer(this.version, created, {});
            const fresh = { player, position: vec(state.pos), slow, kept, size: { width: 0.6, height: 1.8 } };
            this.carries.set(state, fresh);
            return fresh;
        }
        if (!sameInputs(slow, carry.slow)) {
            const dropped = Object.fromEntries(Object.keys(carry.slow).map(key => [key, undefined]));
            carry.player.setState({ ...carry.player.state(), ...dropped, ...slow });
            carry.slow = slow;
        }
        if (!sameBox(kept, carry.kept)) {
            carry.player.setBoundingBox(kept ? nativeBox(kept) : null);
            carry.kept = kept;
        }
        return carry;
    }

    /** The engine's inputs that rarely change: the synced attributes, the effects and the enchantments. */
    private slowInputs(state: PlayerState): Partial<NativeSimulationState> {
        return {
            ...this.attributesAndEffects(state),
            depthStrider: state.depthStrider,
            soulSpeed: state.soulSpeed,
            powderSnowWalkable: state.canWalkOnPowderSnow,
        };
    }

    /**
     * The result of the engine's tick or push on the state: position, motion, contacts, fluids, the fall, the jump
     * counters, the stuck multiplier and (before 1.17) the box it keeps.
     */
    private takeOutput(state: PlayerState, carry: Carry, out: Float64Array): void {
        const flags = out[Out.FLAGS];
        const flag = (bit: number) => (flags & bit) !== 0;
        state.pos.set(out[Out.POSITION], out[Out.POSITION + 1], out[Out.POSITION + 2]);
        state.vel.set(out[Out.VELOCITY], out[Out.VELOCITY + 1], out[Out.VELOCITY + 2]);
        state.onGround = flag(OutFlag.ON_GROUND);
        state.isCollidedHorizontally = flag(OutFlag.COLLIDED_HORIZONTALLY);
        state.isCollidedHorizontallyMinor = flag(OutFlag.MINOR_HORIZONTAL_COLLISION);
        state.isCollidedVertically = flag(OutFlag.COLLIDED_VERTICALLY);
        state.isInWater = flag(OutFlag.IN_WATER);
        state.isInLava = flag(OutFlag.IN_LAVA);
        state.isInPowderSnow = flag(OutFlag.IN_POWDER_SNOW);
        state.fallDistance = out[Out.FALL_DISTANCE];
        state.jumpTicks = out[Out.JUMP_TICKS];
        state.autoJumpTime = out[Out.AUTO_JUMP_TIME];
        state.stuckSpeedMultiplier.set(out[Out.STUCK_SPEED_MULTIPLIER], out[Out.STUCK_SPEED_MULTIPLIER + 1], out[Out.STUCK_SPEED_MULTIPLIER + 2]);
        carry.position = vec(state.pos);
        const at = Out.BOUNDING_BOX;
        carry.kept = Number.isNaN(out[at]) ? null : box(out[at], out[at + 1], out[at + 2], out[at + 3], out[at + 4], out[at + 5]);
        state.keptBoundingBox = carry.kept && this.lt('1.17')
            ? { bb: this.stateBox(state, carry.kept), pos: state.pos.clone(), ...carry.size }
            : null;
    }

    /** The engine's inputs from the synced attributes and the effects. */
    private attributesAndEffects(state: PlayerState): Partial<NativeSimulationState> {
        const attributes = state.attributes;
        const out: Partial<NativeSimulationState> = {
            movementSpeed: this.attributeBaseOf(attributes, 'movementSpeed') ?? f32(0.1),
            safeFallDistance: this.attributeValueOf(attributes, 'safeFallDistance') ?? 3,
            slowFalling: state.slowFalling > 0,
            levitation: state.levitation > 0,
        };
        const sneakingSpeed = this.lt('1.21')
            ? (state.swiftSneak > 0 && !this.lt('1.19') ? Math.min(Math.max(f32(f32(0.3) + f32(state.swiftSneak * f32(0.15))), 0), 1) : undefined)
            : this.attributeValueOf(attributes, 'playerSneakingSpeed') ?? this.attributeValueOf(attributes, 'sneakingSpeed');
        const optional: Partial<NativeSimulationState> = {
            gravity: this.attributeValueOf(attributes, 'gravity'),
            stepHeight: this.attributeValueOf(attributes, 'stepHeight'),
            jumpStrength: this.attributeBaseOf(attributes, 'jumpStrength'),
            sneakingSpeed,
            movementEfficiency: this.attributeValueOf(attributes, 'movementEfficiency'),
            waterMovementEfficiency: this.attributeValueOf(attributes, 'waterMovementEfficiency'),
            airDragModifier: this.attributeValueOf(attributes, 'airDragModifier'),
            frictionModifier: this.attributeValueOf(attributes, 'frictionModifier'),
            entityBounciness: this.attributeValueOf(attributes, 'bounciness'),
            speedEffectAmplifier: this.effectModifierAmplifier(state, 'speed'),
            slownessEffectAmplifier: this.effectModifierAmplifier(state, 'slowness'),
            jumpBoostAmplifier: state.jumpBoost > 0 ? state.jumpBoost - 1 : undefined,
            levitationAmplifier: state.levitation > 0 ? state.levitation - 1 : undefined,
            dolphinsGrace: state.dolphinsGrace > 0 ? state.dolphinsGrace : undefined,
        };
        for (const [key, value] of Object.entries(optional)) if (value !== undefined) (out as Record<string, unknown>)[key] = value;
        return out;
    }

    /**
     * The amplifier of the speed or slowness effect whose modifier the synced movement speed holds: the client takes
     * an effect's modifiers with the attributes the server syncs (LivingEntity.onEffectAdded leaves them to it).
     */
    private effectModifierAmplifier(state: PlayerState, effect: keyof typeof EFFECT_SPEED_MODIFIERS): number | undefined {
        const { ids, perLevel } = EFFECT_SPEED_MODIFIERS[effect];
        const modifier = this.attribute(state.attributes, 'movementSpeed')?.modifiers.find(entry => ids.includes(String(entry.uuid ?? entry.id).toLowerCase()));
        return modifier ? Math.round(modifier.amount / perLevel) - 1 : undefined;
    }

    /** LivingEntity.SPEED_MODIFIER_SPRINTING's id (a UUID before 1.21). */
    private get sprintModifierId(): string {
        return this.lt('1.21') ? '662a6b8d-da3e-4c1c-8813-96ea6097278d' : 'minecraft:sprinting';
    }

    /**
     * Whether the movement speed holds the sprint modifier: setSprinting puts it there and takes it off, and a sync from
     * the server without it drops it until the next setSprinting (LivingEntity.getSpeed reads the attribute).
     */
    private hasSprintModifier(state: PlayerState): boolean {
        const id = this.sprintModifierId;
        return !!this.attribute(state.attributes, 'movementSpeed')?.modifiers.some(entry => String(entry.uuid ?? entry.id).toLowerCase() === id);
    }

    /**
     * The entities near the player as the engine's entities array: how each pushes and whether it collides, where the
     * player's move meets it (where its tick left it when the client ticks it before the player) and where its own tick
     * leaves it.
     */
    private entitiesOf(state: PlayerState): Float64Array {
        const entities = state.nearbyEntities ?? [];
        const out = new Float64Array(entities.length * NATIVE_PLAYER_ENTITY_LENGTH);
        entities.forEach((entity, i) => {
            const ticked = entity.afterTick ?? entity;
            const { boundingBox: b, position: p } = entity.ticksBeforePlayer ? ticked : entity;
            const { boundingBox: tb, position: tp } = ticked;
            out.set([
                ENTITY_KINDS[this.entityKind(entity)], entity.ticksBeforePlayer ? 1 : 0,
                p.x, p.y, p.z, b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ,
                tp.x, tp.y, tp.z, tb.minX, tb.minY, tb.minZ, tb.maxX, tb.maxY, tb.maxZ,
            ], i * NATIVE_PLAYER_ENTITY_LENGTH);
        });
        return out;
    }

    /** The entities the player pushed back in its tick, as the engine tells them, but those that carry passengers. */
    private pushedEntities(state: PlayerState, carry: Carry): { id: number; x: number; z: number }[] {
        const entities = state.nearbyEntities ?? [];
        if (entities.length === 0) return [];
        const pushes = carry.player.entityPushes();
        const pushed = [];
        for (let at = 0; at < pushes.length; at += 3) {
            const { id, isVehicle } = entities[pushes[at]];
            if (id != null && !isVehicle) pushed.push({ id, x: pushes[at + 1], z: pushes[at + 2] });
        }
        return pushed;
    }

    private entityKind(entity: PlayerState['nearbyEntities'][number]): string {
        if (entity.sameVehicle) return 'inert';
        if (isBoatName(entity.name)) return 'boat';
        if (entity.name === 'shulker') return 'shulker';
        if (entity.name === 'happy_ghast') return entity.baby ? 'living' : 'happy_ghast';
        return entity.living ? 'living' : 'inert';
    }

    // ---- the fluids at the tick start (Entity.baseTick), which the client's decisions read ----

    /**
     * Whether the player is in water (Entity.updateInWaterStateAndDoFluidPushing) and its eyes are (updateFluidOnEyes,
     * isUnderWater), where the tick starts; the engine pushes it and keeps its own.
     */
    private updateFluids(state: PlayerState, sync: NativeWorldSync): void {
        if (this.lt('1.13')) {
            this.updateLegacyFluids(state, sync);
            return;
        }
        const collider = this.collider(state.pose, state.pos, state.scale);
        const deflated = shrink(collider, 1e-3);
        let inWater = false;
        for (let x = Math.floor(deflated.minX); x <= Math.floor(deflated.maxX) && !inWater; x += 1) {
            for (let y = Math.floor(deflated.minY); y <= Math.floor(deflated.maxY) && !inWater; y += 1) {
                for (let z = Math.floor(deflated.minZ); z <= Math.floor(deflated.maxZ) && !inWater; z += 1) {
                    const block = sync.blockAt(x, y, z);
                    if (!isWaterish(block)) continue;
                    const height = isWaterish(sync.blockAt(x, y + 1, z)) && !this.lt('1.14') ? 1 : this.ownFluidHeight(block);
                    const top = this.lt('26.1') ? f32(y + height) : y + height;
                    if (top >= deflated.minY) inWater = true;
                }
            }
        }
        state.isInWater = inWater;
        // Entity.updateFluidOnEyes: 1.13 - 1.15 compare the eyes with the water top plus 0.11111111F and take it at once;
        // 1.16+ look 0.1111111119389534 below the eyes (not from 1.21) and isUnderWater is the eyes of the tick before
        const wasEyeInWater = state.eyeInWater ?? state.isUnderWater;
        let eyeY = state.pos.y + f32(this.eyeHeight(state));
        if (!this.lt('1.16') && this.lt('1.21')) eyeY -= 0.1111111119389534;
        const y = Math.floor(eyeY);
        const block = sync.blockAt(Math.floor(state.pos.x), y, Math.floor(state.pos.z));
        let eyeInWater = false;
        if (isWaterish(block)) {
            const height = !this.lt('1.14') && isWaterish(sync.blockAt(Math.floor(state.pos.x), y + 1, Math.floor(state.pos.z)))
                ? 1
                : f32((8 - this.renderedDepth(block)) / 9);
            if (this.lt('1.14')) eyeInWater = eyeY < f32(y + f32(height + f32(0.11111111)));
            else if (this.lt('1.16')) eyeInWater = eyeY < f32(f32(y + height) + f32(0.11111111));
            else eyeInWater = f32(y + height) > eyeY;
        }
        state.eyeInWater = eyeInWater;
        state.isUnderWater = (this.lt('1.16') ? eyeInWater : wasEyeInWater) && state.isInWater;
    }

    /**
     * Entity.handleWaterMovement before 1.13: a water block in the box shrunk by 0.4F vertically and 0.001 all round,
     * when ceil(maxY) reaches its float top; the eyes are in water (isInsideOfMaterial) below its top plus 0.11111111F.
     */
    private updateLegacyFluids(state: PlayerState, sync: NativeWorldSync): void {
        const isWater = (block: DescribedBlock | null): block is DescribedBlock => block?.fluid?.type === 1;
        const heightPercent = (block: DescribedBlock) => f32(((block.fluid!.level >= 8 ? 0 : block.fluid!.level) + 1) / 9);
        const collider = this.collider(state.pose, state.pos, 1);
        const inset = f32(0.4) + 0.001;
        const bb = box(collider.minX + 0.001, collider.minY + inset, collider.minZ + 0.001, collider.maxX - 0.001, collider.maxY - inset, collider.maxZ - 0.001);
        const maxY = Math.ceil(bb.maxY);
        let inWater = false;
        for (let x = Math.floor(bb.minX); x < Math.ceil(bb.maxX); x += 1) {
            for (let y = Math.floor(bb.minY); y < maxY; y += 1) {
                for (let z = Math.floor(bb.minZ); z < Math.ceil(bb.maxZ); z += 1) {
                    const block = sync.blockAt(x, y, z);
                    if (isWater(block) && maxY >= f32(y + 1 - heightPercent(block))) inWater = true;
                }
            }
        }
        state.isInWater = inWater;
        const eyeY = state.pos.y + this.eyeHeight(state);
        const y = Math.floor(eyeY);
        const block = sync.blockAt(Math.floor(state.pos.x), y, Math.floor(state.pos.z));
        state.isUnderWater = isWater(block) && eyeY < f32(y + 1 - f32(heightPercent(block) - f32(0.11111111)));
    }

    /** FlowingFluid.getOwnHeight: amount / 9F, a source, waterlogged block or falling fluid holding 8. */
    private ownFluidHeight(block: DescribedBlock): number {
        const liquid = block.name === 'water' || block.name === 'lava';
        const level = block.fluid?.level ?? 0;
        return f32((!liquid || level >= 8 ? 8 : 8 - level) / 9);
    }

    /** The water's depth as the eyes read it: its level, 0 when falling, a source or waterlogged. */
    private renderedDepth(block: DescribedBlock): number {
        if (block.name !== 'water' && block.name !== 'flowing_water') return 0;
        const level = block.fluid?.level ?? 0;
        return level >= 8 ? 0 : level;
    }

    // ---- LocalPlayer.aiStep's decisions ----

    /** Player.updateSwimming (1.13+): sprinting under water starts swimming, sprinting in water keeps it. */
    private updateSwimming(state: PlayerState, sync: NativeWorldSync): void {
        if (this.lt('1.13') || state.flying) {
            state.swimming = false;
        } else if (state.swimming) {
            state.swimming = state.sprinting && state.isInWater;
        } else {
            const feet = sync.blockAt(Math.floor(state.pos.x), Math.floor(state.pos.y), Math.floor(state.pos.z));
            state.swimming = state.sprinting && state.isUnderWater && isWaterish(feet);
        }
    }

    private swimmingAndNotFlying(state: PlayerState): boolean {
        return !state.flying && state.gameMode !== 'spectator' && state.swimming;
    }

    /**
     * Whether the sneak slowdown applies to this tick's input. 1.15+: LocalPlayer.isMovingSlowly, crouching or
     * crawling; 1.14's KeyboardInput also slows by the sneak key; before 1.14 the sneak key alone.
     */
    private isMovingSlowly(state: PlayerState): boolean {
        if (this.gt('1.13.2')) {
            const crawling = (state.pose === Pose.SWIMMING || (!state.fallFlying && state.pose === Pose.FALL_FLYING)) && !state.isInWater;
            if (this.lt('1.15') && state.control.sneak) return true;
            return state.crouching || crawling;
        }
        return state.control.sneak;
    }

    /**
     * Crouching (the sneak key of the tick before, or no room to stand from 1.14) and the input as the sprint reads it:
     * slowed by the sneak and the item in use before 1.21.5, normalized since. Returns whether the input moves slowly.
     */
    private inputsToCrouch(state: PlayerState, heading: { forward: number; strafe: number }, sync: NativeWorldSync): boolean {
        if (this.gt('1.13.2')) {
            const sneakBox = shrink(this.collider(Pose.SNEAKING, state.pos, state.scale), 1e-7);
            const standBox = shrink(this.collider(Pose.STANDING, state.pos, state.scale), 1e-7);
            state.crouching = !this.swimmingAndNotFlying(state) && this.isFree(sync, sneakBox, state)
                && (state.prevControl.sneak || !this.isFree(sync, standBox, state));
        } else {
            state.crouching = !this.swimmingAndNotFlying(state) && state.prevControl.sneak;
        }
        const movingSlowly = this.isMovingSlowly(state);
        // 1.21.4 only: LocalPlayer.shouldStopSprinting right after the input's tick
        if (this.gt('1.21.3') && this.lt('1.21.5')) {
            const blind = this.getEffectLevel(BLINDNESS, state.effects) > 0;
            if (blind || movingSlowly || (state.isUsingItem && !state.isUnderWater)) this.setSprinting(state, false);
        }
        if (this.gt('1.21.4')) {
            const length = f32(Math.sqrt(f32(f32(heading.strafe * heading.strafe) + f32(heading.forward * heading.forward))));
            heading.strafe = length < f32(1.0e-4) ? 0 : f32(heading.strafe / length);
            heading.forward = length < f32(1.0e-4) ? 0 : f32(heading.forward / length);
            return movingSlowly;
        }
        const slow = (factor: number) => {
            heading.forward = f32(heading.forward * factor);
            heading.strafe = f32(heading.strafe * factor);
        };
        const usingItemFirst = this.gt('1.21.3');
        if (state.isUsingItem && usingItemFirst) slow(f32(0.2));
        if (movingSlowly) slow(this.sneakingSpeedFactor(state));
        if (state.isUsingItem && !usingItemFirst) slow(f32(0.2));
        return movingSlowly;
    }

    /** The sneak slowdown: 0.3 before 1.19, swift sneak's float clamp to 1.20.6, the sneaking speed attribute since. */
    private sneakingSpeedFactor(state: PlayerState): number {
        if (this.lt('1.19')) return 0.3;
        if (this.lt('1.21')) return Math.min(Math.max(f32(f32(0.3) + f32(state.swiftSneak * f32(0.15))), 0), 1);
        return f32(this.attributeValueOf(state.attributes, 'playerSneakingSpeed') ?? this.attributeValueOf(state.attributes, 'sneakingSpeed') ?? 0.3);
    }

    private slowDueToUsingItem(state: PlayerState): boolean {
        return state.isUsingItem && !state.useItemCanSprint;
    }

    /**
     * LocalPlayer.aiStep's sprint: the sprint key starts one, and so does a double tap of forward (a fresh press opens a
     * 7-tick window); a sprint stops when the player no longer moves forward enough, hits a wall, hungers, ...
     */
    private inputsToSprint(state: PlayerState, heading: { forward: number; strafe: number }): void {
        if (state.sprintTriggerTime > 0) state.sprintTriggerTime -= 1;
        const prevSneakKey = state.prevControl.sneak;
        if (this.slowDueToUsingItem(state) || (this.gt('1.15.2') && prevSneakKey) || (this.gt('1.21.4') && state.control.back)) {
            state.sprintTriggerTime = 0;
        }
        const canStart = this.canStartSprinting(state, heading);
        const prevImpulse = state.prevForwardImpulse;
        state.prevForwardImpulse = heading.forward;
        if (this.gt('1.21.4')) {
            if (canStart) {
                if (!(prevImpulse > 1e-5)) {
                    if (state.sprintTriggerTime > 0) this.setSprinting(state, true);
                    else state.sprintTriggerTime = 7;
                }
                if (state.control.sprint) this.setSprinting(state, true);
            }
        } else {
            const prevEnough = this.gt('1.13.2') && state.isUnderWater ? prevImpulse > 1e-5 : prevImpulse >= 0.8;
            const onGroundOrUnderWater = state.onGround || (this.gt('1.12.2') && state.isUnderWater);
            if (canStart && onGroundOrUnderWater && !prevSneakKey && !prevEnough) {
                if (state.sprintTriggerTime <= 0 && !state.control.sprint) state.sprintTriggerTime = 7;
                else this.setSprinting(state, true);
            }
            if (canStart && state.control.sprint) this.setSprinting(state, true);
        }
        if (!state.sprinting) return;
        if (this.lt('1.13')) {
            if (heading.forward < 0.8 || state.isCollidedHorizontally || !this.hasEnoughFoodToSprint(state)) this.setSprinting(state, false);
        } else if (state.swimming) {
            if (this.shouldStopSwimSprinting(state, heading)) this.setSprinting(state, false);
        } else if (this.shouldStopRunSprinting(state, heading)) {
            this.setSprinting(state, false);
        }
    }

    private canStartSprinting(state: PlayerState, heading: { forward: number }): boolean {
        const movingSlowly = this.gt('1.21.4') ? this.isMovingSlowly(state) : state.crouching;
        return !state.sprinting
            && this.hasEnoughImpulseToStartSprinting(state, heading)
            && this.hasEnoughFoodToSprint(state)
            && !this.slowDueToUsingItem(state)
            && !state.blindness
            && (!movingSlowly || state.isUnderWater)
            && (this.lt('1.13') || ((!state.fallFlying || state.isUnderWater) && (!state.isInWater || state.isUnderWater)));
    }

    private hasEnoughImpulseToStartSprinting(state: PlayerState, heading: { forward: number }): boolean {
        if (this.gt('1.21.4')) return heading.forward > 1e-5;
        if (this.lt('1.14')) return heading.forward >= 0.8;
        return heading.forward >= (state.isInWater ? 1e-5 : 0.8);
    }

    private hasEnoughFoodToSprint(state: PlayerState): boolean {
        return state.food > 6 || state.mayFly;
    }

    private hasForwardImpulseToKeepSprinting(heading: { forward: number }): boolean {
        return this.lt('1.14') ? heading.forward >= 0.8 : heading.forward > 1e-5;
    }

    private shouldStopRunSprinting(state: PlayerState, heading: { forward: number }): boolean {
        return (this.gt('1.21.3') && state.blindness > 0)
            || !this.hasForwardImpulseToKeepSprinting(heading)
            || !this.hasEnoughFoodToSprint(state)
            || (state.isCollidedHorizontally && !state.isCollidedHorizontallyMinor)
            || (state.isInWater && !state.isUnderWater);
    }

    private shouldStopSwimSprinting(state: PlayerState, heading: { forward: number }): boolean {
        return state.blindness > 0
            || !state.isInWater
            || (!this.hasForwardImpulseToKeepSprinting(heading) && !state.onGround && !state.control.sneak)
            || !this.hasEnoughFoodToSprint(state);
    }

    /**
     * LivingEntity.setSprinting: the sprint modifier comes and goes on a copy of the synced movement speed (the server's
     * stays as it sent it).
     */
    setSprinting(state: PlayerState, value: boolean): void {
        const key = this.attributeKey('movementSpeed');
        if (key !== null) {
            const sprintId = this.sprintModifierId;
            const synced = state.attributes[key];
            const modifiers = (synced?.modifiers ?? []).filter(modifier => String(modifier.uuid ?? modifier.id).toLowerCase() !== sprintId);
            if (value) modifiers.push({ uuid: sprintId, amount: f32(0.3), operation: 2 });
            state.attributes = { ...state.attributes, [key]: { value: synced?.value ?? f32(0.1), modifiers } };
        }
        state.sprinting = value;
    }

    /**
     * LocalPlayer.aiStep's flight: a second press of jump within 7 ticks toggles flying where the player may fly (since
     * 1.20.5 a start on the ground jumps); since 1.15 a fresh jump in the air with a usable elytra starts gliding.
     */
    private inputsToFly(state: PlayerState, sync: NativeWorldSync): void {
        let flyChanged = false;
        const freshJump = !state.prevControl.jump && state.control.jump;
        if (state.mayFly) {
            if (state.gameMode === 'spectator') {
                state.flying = true;
                flyChanged = true;
            } else if (freshJump) {
                if (state.flyJumpTriggerTime === 0) {
                    state.flyJumpTriggerTime = 7;
                } else if (this.lt('1.13') || !state.swimming) {
                    state.flying = !state.flying;
                    if (state.flying && state.onGround && !this.lt('1.20.5')) this.jumpFromGround(state, sync);
                    flyChanged = true;
                    state.flyJumpTriggerTime = 0;
                }
            }
        }
        // (Player.tryToStartFallFlying does not start under levitation from 1.16)
        const levitating = state.levitation > 0 && !this.lt('1.16');
        if (!this.lt('1.15') && freshJump && !flyChanged && !state.flying && !state.onClimbable && !state.onGround
            && !state.isInWater && !state.fallFlying && !levitating && state.validElytraEquipped) {
            state.fallFlying = true;
        }
    }

    /** LivingEntity.jumpFromGround for the flight a player starts on the ground (the engine jumps only on foot). */
    private jumpFromGround(state: PlayerState, sync: NativeWorldSync): void {
        const honey = (x: number, y: number, z: number) => sync.blockAt(Math.floor(x), Math.floor(y), Math.floor(z))?.effect === EFFECT.honey;
        const below = state.supportingBlockPos ?? { x: state.pos.x, y: state.pos.y - 0.500001, z: state.pos.z };
        const factor = honey(state.pos.x, state.pos.y, state.pos.z) || honey(below.x, below.y, below.z) ? 0.5 : 1;
        const jumpBoost = f32(f32(0.1) * state.jumpBoost);
        const strength = f32(this.attributeValueOf(state.attributes, 'jumpStrength') ?? f32(0.42));
        const power = f32(strength * f32(factor) + jumpBoost);
        if (!(power > 1e-5)) return;
        state.vel.y = Math.max(power, state.vel.y);
        if (state.sprinting) {
            const yaw = floatRadians(this.yawDegrees(state));
            state.vel.x += -this.mth.sin(yaw) * 0.2;
            state.vel.z += this.mth.cos(yaw) * 0.2;
        }
    }

    /**
     * Player.updatePlayerPose (1.14+): the pose the player wants (gliding, sleeping, swimming, crouching, standing)
     * when it fits, else crouching or swimming; nothing changes without room to swim.
     */
    private updatePoses(state: PlayerState, sync: NativeWorldSync): void {
        if (!this.isFree(sync, shrink(this.collider(Pose.SWIMMING, state.pos, state.scale), 1e-7), state)) return;
        let wanted: number;
        if (state.fallFlying) wanted = Pose.FALL_FLYING;
        else if (state.pose === Pose.SLEEPING) wanted = Pose.SLEEPING;
        else if (this.swimmingAndNotFlying(state)) wanted = Pose.SWIMMING;
        else if (state.control.sneak && !state.flying) wanted = Pose.SNEAKING;
        else wanted = Pose.STANDING;
        if (state.gameMode === 'spectator' || this.isFree(sync, shrink(this.collider(wanted, state.pos, state.scale), 1e-7), state)) state.pose = wanted;
        else if (this.isFree(sync, shrink(this.collider(Pose.SNEAKING, state.pos, state.scale), 1e-7), state)) state.pose = Pose.SNEAKING;
        else state.pose = Pose.SWIMMING;
    }

    /**
     * EntityPlayer.updateSize at the end of the tick (1.9 - 1.13): a new height refits the kept box on its lower corner
     * (Entity.setSize), unless the refitted box would collide with a block.
     */
    private updateLegacySize(state: PlayerState, sync: NativeWorldSync): void {
        const kept = state.keptBoundingBox;
        if (kept == null || this.lt('1.9')) return;
        const height = this.legacyPlayerHeight(state, state.control.sneak);
        if (height === kept.height) return;
        const { bb } = kept;
        const resized = box(bb.minX, bb.minY, bb.minZ, bb.minX + kept.width, bb.minY + height, bb.minZ + kept.width);
        if (!this.isFree(sync, this.lt('1.13') ? resized : shrink(resized, 1e-7), state, true)) return;
        state.keptBoundingBox = { ...kept, bb: this.stateBox(state, resized), height };
    }

    /** The player's height before 1.14 (EntityPlayer.updateSize; 1.8 keeps 1.8F): 0.6F gliding (1.13: or swimming),
     * 1.65F sneaking, else 1.8F. */
    private legacyPlayerHeight(state: PlayerState, sneaking: boolean): number {
        if (this.lt('1.9')) return f32(1.8);
        if (state.fallFlying || (!this.lt('1.13') && state.swimming)) return f32(0.6);
        return f32(sneaking ? 1.65 : 1.8);
    }

    /** LivingEntity.onClimbable: a climbable block at the feet, or (1.9+) an open trapdoor over a ladder facing its way. */
    private isInClimbable(state: PlayerState, sync: NativeWorldSync): boolean {
        if (state.gameMode === 'spectator') return false;
        const x = Math.floor(state.pos.x);
        const y = Math.floor(state.pos.y);
        const z = Math.floor(state.pos.z);
        const feet = sync.blockAt(x, y, z);
        if (!feet) return false;
        if (CLIMBABLE.has(feet.name)) return true;
        if (this.lt('1.9') || !feet.name.endsWith('trapdoor') || feet.props.open !== 'true') return false;
        const below = sync.blockAt(x, y - 1, z);
        return below?.name === 'ladder' && below.props.facing === feet.props.facing;
    }

    /**
     * The blocks the tick can reach: the player's box and where its motion takes it, with a block around (the block that
     * supports it, its poses, what its input adds), its step above, and with auto-jump the 7 ticks of speed it looks
     * ahead; a riptide launch or rockets take it further.
     */
    private reachOf(state: PlayerState, launching: boolean): Box {
        const { width, height } = this.poseSize(Pose.STANDING, state.scale);
        const b = this.boxAround(state.pos, width, height);
        const v = state.vel;
        const boost = (launching ? 4 : 0) + (state.fireworkRocketDuration > 0 ? 2 : 0);
        const ahead = 1 + boost + (state.autoJumpEnabled ? 8 * Math.sqrt(v.x * v.x + v.z * v.z) : 0);
        const step = this.attributeValueOf(state.attributes, 'stepHeight') ?? 0.6;
        return box(
            Math.min(b.minX, b.minX + v.x) - ahead,
            Math.min(b.minY, b.minY + v.y) - 1 - boost,
            Math.min(b.minZ, b.minZ + v.z) - ahead,
            Math.max(b.maxX, b.maxX + v.x) + ahead,
            Math.max(b.maxY, b.maxY + v.y) + 1 + step + boost,
            Math.max(b.maxZ, b.maxZ + v.z) + ahead,
        );
    }

    // ---- sizes ----

    private poseSize(pose: number, scale: number): Size {
        const size = POSE_SIZES[pose] ?? POSE_SIZES[Pose.STANDING];
        if (scale === 1 || pose === Pose.SLEEPING) return size;
        return { width: f32(size.width * scale), height: f32(size.height * scale) };
    }

    private collider(pose: number, pos: Vec3, scale: number): Box {
        const { width, height } = this.poseSize(pose, scale);
        return this.boxAround(pos, width, height);
    }

    private boxAround(pos: { x: number; y: number; z: number }, width: number, height: number): Box {
        const half = width / 2;
        return box(pos.x - half, pos.y, pos.z - half, pos.x + half, pos.y + height, pos.z + half);
    }

    /** The box the player keeps before 1.17, while it is still where the engine left it. */
    private keptFor(state: PlayerState): Box | null {
        const kept = state.keptBoundingBox;
        if (!this.lt('1.17') || kept == null || !samePosition(kept.pos, state.pos)) return null;
        return kept.bb;
    }

    /** The size the player moves with: its pose's from 1.14; before, the kept box's or 1.8F (1.65F sneaking). */
    private moveSize(state: PlayerState, keptValid: boolean): Size {
        if (!this.lt('1.14')) return this.poseSize(state.pose, state.scale);
        const height = keptValid && state.keptBoundingBox ? state.keptBoundingBox.height : this.legacyPlayerHeight(state, state.prevControl.sneak);
        return { width: f32(0.6), height };
    }

    /** Player.getStandingEyeHeight of the pose; before 1.14 0.08F lower sneaking (or in the kept 1.65F box). */
    private eyeHeight(state: PlayerState): number {
        let eyeHeight: number;
        const sneakBox = !this.lt('1.9') && state.keptBoundingBox?.height === f32(1.65);
        if (this.lt('1.14') && state.pose !== Pose.SLEEPING && (state.prevControl.sneak || sneakBox)) {
            eyeHeight = f32(f32(1.62) - f32(0.08));
        } else if (state.pose === Pose.SNEAKING) {
            eyeHeight = 1.27;
        } else if (state.pose === Pose.STANDING || state.pose === Pose.LONG_JUMPING || !(state.pose in POSE_SIZES)) {
            eyeHeight = 1.62;
        } else {
            eyeHeight = 0.4;
        }
        return state.scale === 1 || state.pose === Pose.SLEEPING ? eyeHeight : f32(f32(eyeHeight) * state.scale);
    }

    /** The player's yaw as vanilla holds it: float degrees, whole turns kept when the state has them. */
    private yawDegrees(state: PlayerState): number {
        if (state.yawDegrees != null) return state.yawDegrees;
        return f32((Math.PI - state.yaw) * (180 / Math.PI));
    }

    /** The player's pitch in float degrees, positive looking down (mineflayer's radians look up). */
    private pitchDegrees(state: PlayerState): number {
        return f32(-state.pitch * (180 / Math.PI));
    }

    /**
     * Whether no collision box of the blocks in the box's cells (and the row under them when `underlying`) meets it,
     * the scaffolding and powder snow shaped for the player (EntityCollisionContext).
     */
    private isFree(sync: NativeWorldSync, query: Box, state: PlayerState, underlying = false): boolean {
        for (let y = Math.floor(query.minY) - Number(underlying); y <= Math.floor(query.maxY); y += 1) {
            for (let z = Math.floor(query.minZ); z <= Math.floor(query.maxZ); z += 1) {
                for (let x = Math.floor(query.minX); x <= Math.floor(query.maxX); x += 1) {
                    for (const [minX, minY, minZ, maxX, maxY, maxZ] of this.collisionBoxes(sync, state, x, y, z)) {
                        if (x + minX < query.maxX && x + maxX > query.minX && y + minY < query.maxY && y + maxY > query.minY
                            && z + minZ < query.maxZ && z + maxZ > query.minZ) return false;
                    }
                }
            }
        }
        return true;
    }

    /**
     * A block's collision boxes for the player: ScaffoldingBlock.getCollisionShape (the stable shape to a player above
     * it that is not descending, a hanging bottom's slab to one above the block below) and PowderSnowBlock's (0.9F tall
     * after a fall of more than 2.5, a full block to a player in leather boots above it that is not descending).
     */
    private collisionBoxes(sync: NativeWorldSync, state: PlayerState, x: number, y: number, z: number): number[][] {
        const block = sync.blockAt(x, y, z);
        if (!block) return [];
        const above = state.pos.y > y + 1 - 9.999999747378752e-6;
        if (block.name === 'scaffolding') {
            if (above && !state.control.sneak) return sync.boxesAt(x, y, z);
            const hangingBottom = Number(block.props.distance) !== 0 && block.props.bottom === 'true';
            return hangingBottom && state.pos.y > y - 9.999999747378752e-6 ? [[0, 0, 0, 1, 0.125, 1]] : [];
        }
        if (block.name === 'powder_snow') {
            if (state.fallDistance > 2.5) return [[0, 0, 0, 1, 0.8999999761581421, 1]];
            return state.canWalkOnPowderSnow && above && !state.control.sneak ? [[0, 0, 0, 1, 1, 1]] : [];
        }
        return sync.boxesAt(x, y, z);
    }

    /** A box of the state's own AABB class (its getBB's), for the fields mineflayer-physics-utils keeps. */
    private stateBox(state: PlayerState, b: Box): Box {
        const out = state.getBB();
        Object.assign(out, b);
        return out;
    }
}
