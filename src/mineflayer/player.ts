// The local player's tick for mineflayer-physics-utils' EPhysicsCtx, the engine interface mineflayer's physics plugin
// drives (IPhysics.simulate, as BotcraftPhysics). The native engine moves the player (simulateVersionedTick); around it
// the adapter makes the client's own decisions that the engine leaves to its caller, as LocalPlayer.aiStep makes them
// (a port of BotcraftPhysics' localPlayerAIStep): crouching, sprinting, flying and the elytra's start, swimming, the
// pose and, before 1.14, the player's height.
import type { IPhysics } from '@nxg-org/mineflayer-physics-util/dist/physics/engines/IPhysics';
import {
    NATIVE_PLAYER_ENTITY_LENGTH,
    NativePlayerEntityFlag,
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
import { EFFECT, FLUID_LAVA, GLIDE_THROUGH, type DescribedBlock } from './blocks';
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

/** A moving piston block as mineflayer-physics-utils' BotcraftPhysics.pushByMovingPiston takes it. */
export interface MovingPiston {
    pos: [number, number, number];
    direction: [number, number, number];
    extending: boolean;
    source: boolean;
    progress: number;
    shapes: number[][];
    /** The outline boxes of a block it pushes, which push on 1.9 - 1.10 (absent: the full cube). */
    outlineShapes?: number[][];
    /** The name of the block it moves: a slime block bounces the player, a honey block carries it. */
    movedBlock?: string;
}

/** The slime block's names: slime before 1.13 (minecraft-data's), slime_block from it. */
const SLIME_BLOCKS = new Set(['slime', 'slime_block']);

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
const ENTITY_KINDS: Record<string, number> = {
    inert: 0, living: 1, boat: 2, shulker: 3, happy_ghast: 4, still_living: 5, happy_ghast_stays_still: 6,
    attached_rocket: 7,
};

/**
 * The LivingEntities that never push the local player: the bat (doPush and pushEntities are empty), the parrot (doPush
 * skips players, 1.12+), the ender dragon (its aiStep never pushes) and the vex (noPhysics through its own tick, which
 * Entity.push(Entity) skips, 1.11+). 1.9.4 BatEntity.java:70, 1.21.11 Bat.java:91-96; 1.12.2 ParrotEntity.java:367-371,
 * 1.21.11 Parrot.java:385-389; 1.9.4 EnderDragonEntity.java:151, 1.21.11 EnderDragon.java:152; 1.11.2
 * VexEntity.java:61-63, 1.21.11 Vex.java:73-75, Entity.java:1752-1754.
 */
const NEVER_PUSHING_LIVING = new Set(['bat', 'parrot', 'ender_dragon', 'vex']);

/** Team.CollisionRule by its id (1.21.5 - 1.21.8 send the id) and by its names (pushOtherTeams; push_other_teams from 1.21.9). */
const COLLISION_RULES = ['always', 'never', 'pushOtherTeams', 'pushOwnTeam'];
const collisionRuleOf = (team: { collisionRule?: unknown } | undefined): string => {
    if (!team) return 'always';
    const rule = team.collisionRule;
    if (typeof rule === 'number') return COLLISION_RULES[rule] ?? 'always';
    if (typeof rule !== 'string') return 'always';
    const name = rule.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
    return COLLISION_RULES.includes(name) ? name : 'always';
};

/** The TamableAnimals (the ocelot to 1.13), by mineflayer's names (1.9 - 1.10's "Wolf", "Ozelot"), lower case. */
const TAMABLE = new Set(['wolf', 'ocelot', 'ozelot', 'cat', 'parrot', 'nautilus', 'zombie_nautilus']);

type TeamEntity = { type?: string; name?: string; username?: string; uuid?: string; metadata?: unknown[] };
type TeamTeam = { collisionRule?: unknown } | undefined;
type TeamMap = Record<string, TeamTeam>;

/** The team of an entity's scoreboard name (Entity.getScoreboardName: a player's name, another entity's UUID). */
const scoreboardTeam = (entity: TeamEntity, teams: TeamMap): TeamTeam => {
    const name = entity.type === 'player' ? entity.username : entity.uuid;
    return name != null ? teams[name] : undefined;
};

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
            outlineShapes: piston.outlineShapes?.map(([minX, minY, minZ, maxX, maxY, maxZ]) => nativeBox(box(minX, minY, minZ, maxX, maxY, maxZ))),
            bounces: SLIME_BLOCKS.has(piston.movedBlock ?? ''),
            stickyForEntities: piston.movedBlock === 'honey_block',
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
        // Entity.collide collides with the border the server set (ClientPacketListener's border packets)
        const border = state.bot?.worldBorder;
        if (border) sync.setWorldBorder(border, state.bot?.game);

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
            | bit(!!state.removed, InFlag.REMOVED) | bit(state.autoSpinAttackTicks > 0, InFlag.AUTO_SPIN_ATTACK)
            | bit(this.isSleeping(state), InFlag.SLEEPING) | bit(this.isDead(state), InFlag.DEAD);
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
        input[In.YAW_DEGREES] = f32(yaw);
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
        // the attached rockets listed among the nearby entities boost in their turns of the tick (each entity runs its
        // whole tick in ClientLevel.tickEntities' order, FireworkRocketEntity.java:124-131 in 1.21.11), the rest at the
        // start of their side of the player
        const listed = (before: boolean) => (state.nearbyEntities ?? []).filter(e => e.attachedRocket && !!e.ticksBeforePlayer === before).length;
        rocketsBefore = Math.max(0, rocketsBefore - listed(true));
        rocketsAfter = Math.max(0, rocketsAfter - listed(false));
        input[In.FIREWORK_ROCKETS] = rocketsBefore + rocketsAfter;
        input[In.FIREWORK_ROCKETS_BEFORE_PLAYER] = rocketsBefore;
        input[In.FLYING_SPEED] = state.flySpeed ?? f32(0.05);
        input[In.ITEM_USE_SPEED_MULTIPLIER] = state.useItemSpeedMultiplier ?? f32(0.2);

        const carry = this.carryFor(state, kept);
        input[In.FLAGS] = flags | bit(samePosition(carry.position, pos), InFlag.UNMOVED);
        const flyingDuringMove = state.flying;
        carry.size = size;
        const out = carry.player.tick(sync.world, input, this.entitiesOf(state));
        this.takeOutput(state, carry, out);
        // isInWeb: the move was slowed by a cobweb, berry bush or powder snow (Entity.isInWeb before 1.14); the lift of a
        // riptide launch from the ground takes the slowdown, and to 1.21.1 its own checkInsideBlocks may set it again
        if (state.gameMode !== 'spectator') state.isInWeb = (out[Out.FLAGS] & OutFlag.MOVE_SLOWED) !== 0;
        state.pushedEntities = this.pushedEntities(state, carry);

        // LivingEntity.aiStep after the move: the spin attack runs down, and ends on an entity or a wall (the engine's
        // checkAutoSpinAttack, which bounces the player off a living entity)
        if (state.autoSpinAttackTicks > 0) {
            state.autoSpinAttackTicks -= 1;
            if ((out[Out.FLAGS] & OutFlag.AUTO_SPIN_ATTACK_ENDED) !== 0) state.autoSpinAttackTicks = 0;
        }
        // Player.travel's flight clears the glide flag on 1.9 - 1.21.1; from 1.21.2 only the server does
        // (LivingEntity.updateFallFlying is server side, 1.21.2 Player.java:1399-1421, LivingEntity.java:2864-2869)
        if (flyingDuringMove && !this.lt('1.9') && this.lt('1.21.2')) state.fallFlying = false;
        state.onClimbable = this.isInClimbable(state, sync);
        if (state.onGround && state.flying && state.gameMode !== 'spectator') state.flying = false;
        if (this.gt('1.13.2')) this.updatePoses(state, sync);
        else this.updateLegacySize(state, sync);
        if (state.fireworkRocketDuration > 0) state.fireworkRocketDuration -= 1;

        state.lastOnGround = state.onGround;
        state.prevHeading.forward = heading.forward;
        state.prevHeading.strafe = heading.strafe;
        // wasJumping: the input's jump, which this tick's auto-jump pressed (1.21.11 LocalPlayer.java:738, 753)
        state.prevControl.jump = control.jump || state.autoJumping;
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

    /** The engine's inputs that rarely change: the synced attributes, the effects, the enchantments and the abilities. */
    private slowInputs(state: PlayerState): Partial<NativeSimulationState> {
        return {
            ...this.attributesAndEffects(state),
            depthStrider: state.depthStrider,
            soulSpeed: state.soulSpeed,
            powderSnowWalkable: state.canWalkOnPowderSnow,
            // MagmaBlock.stepOn and CampfireBlock.entityInside spare Frost Walker's wearer before 1.21 (1.20.5
            // MagmaBlock.java:29, CampfireBlock.java:104), the enchantment's damage immunity from 1.21 (1.21.11
            // Enchantments.java:389-394)
            frostWalker: this.bootsEnchantmentLevel(state, 'frost_walker') > 0,
            // ProtectionEnchantment.transformFireDuration takes the highest Fire Protection level worn off the burning before
            // 1.21 (1.17 ProtectionEnchantment.java:59-66, EnchantmentHelper.getEquipmentLevel); from 1.21 the
            // enchantment is in the BURNING_TIME attribute
            fireProtection: this.lt('1.21') ? this.armorEnchantmentLevel(state, 'fire_protection') : 0,
            // Player.causeFallDamage deals nothing where the abilities allow flight (1.21.5 Player.java:1470-1472), and
            // calculateFallDamage nothing for a type in #fall_damage_immune (1.20.5 LivingEntity.java:1603-1605, 1.21.5
            // :1673-1675): the state's fallDamageImmune
            fallDamageImmune: state.mayFly || !!state.fallDamageImmune,
            // the client's own player moves itself (Entity.isLogicalSideForUpdatingMovement through
            // ClientPlayerEntity.canMoveVoluntarily, 1.21.3 Entity.java:3198-3200, ClientPlayerEntity.java:477); a
            // state may say otherwise
            controlledByLocalInstance: state.controlledByLocalInstance ?? true,
            // Entity.updateFluidHeightAndDoFluidPushing pushes by lava at 0.007 in an ultrawarm dimension (FAST_LAVA from
            // 1.21.11), 0.0023333333333333335 elsewhere (1.16 Entity.java:1049, 1.21.11 :1519); of the vanilla dimension
            // types only the nether's is (1.16 DimensionType.java:80-95, 1.21.11 DimensionTypes.java:81)
            ultraWarm: state.bot?.game?.dimension === 'the_nether',
            noGravity: this.noGravityOf(state, state.bot?.entity),
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
        // LivingEntity.stopFallFlying: from 1.21.5 a climbable block ends the glide in travel
        if (!flag(OutFlag.FALL_FLYING)) state.fallFlying = false;
        state.fallDistance = out[Out.FALL_DISTANCE];
        // LivingEntity.causeFallDamage's amount on this tick's landing, 0 without one; the boundary values the state
        // carries (safeFallDistance, fallDamageImmune, fallDamageMultiplier, the bounciness, controlledByLocalInstance)
        // are its own and stay as they are
        state.fallDamage = out[Out.FALL_DAMAGE];
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

    /** The highest level of an enchantment on the armor the bot wears, 0 without a bot. */
    private armorEnchantmentLevel(state: PlayerState, name: string): number {
        const bot = state.bot;
        if (!bot?.inventory || !bot.getEquipmentDestSlot) return 0;
        let highest = 0;
        for (const slot of ['head', 'torso', 'legs', 'feet']) {
            const enchants = bot.inventory.slots[bot.getEquipmentDestSlot(slot)]?.enchants;
            const enchantments = Array.isArray(enchants)
                ? enchants.map(({ name: id, lvl }) => ({ id, lvl }))
                : (enchants?.enchantments ?? []).map(({ id, level }) => ({ id, lvl: level }));
            highest = Math.max(highest, this.enchantmentLevelByName(name, enchantments));
        }
        return highest;
    }

    /** The level of an enchantment on the boots the bot wears (its inventory's feet slot), 0 without a bot. */
    private bootsEnchantmentLevel(state: PlayerState, name: string): number {
        const bot = state.bot;
        if (!bot?.inventory || !bot.getEquipmentDestSlot) return 0;
        const enchants = bot.inventory.slots[bot.getEquipmentDestSlot('feet')]?.enchants;
        const enchantments = Array.isArray(enchants)
            ? enchants.map(({ name: id, lvl }) => ({ id, lvl }))
            : (enchants?.enchantments ?? []).map(({ id, level }) => ({ id, lvl: level }));
        return this.enchantmentLevelByName(name, enchantments);
    }

    /** The engine's inputs from the synced attributes and the effects. */
    private attributesAndEffects(state: PlayerState): Partial<NativeSimulationState> {
        const attributes = state.attributes;
        const levitation = this.effectAmplifier(state, state.levitation, 'Levitation');
        const jumpBoost = this.effectAmplifier(state, state.jumpBoost, 'JumpBoost');
        const out: Partial<NativeSimulationState> = {
            movementSpeed: f32(0.1),
            // the state's own boundary values first (a caller's runtime state), else the synced attributes
            safeFallDistance: state.safeFallDistance ?? this.attributeValueOf(attributes, 'safeFallDistance') ?? 3,
            slowFalling: this.effectAmplifier(state, state.slowFalling, 'SlowFalling') !== undefined,
            levitation: levitation !== undefined,
            // WebBlock.entityInside's (0.5, 0.25, 0.5) under Weaving (1.20.5+, 1.21.11 WebBlock.java:27-31)
            weaving: this.effectLevelByName('Weaving', state.effects) > 0,
        };
        const sneakingSpeed = this.lt('1.21')
            ? (state.swiftSneak > 0 && !this.lt('1.19') ? Math.min(Math.max(f32(f32(0.3) + f32(state.swiftSneak * f32(0.15))), 0), 1) : undefined)
            : this.attributeValueOf(attributes, 'playerSneakingSpeed') ?? this.attributeValueOf(attributes, 'sneakingSpeed');
        const optional: Partial<NativeSimulationState> = {
            // LivingEntity.getSpeed and getJumpPower read the attributes with every modifier they hold, the speed and
            // slowness effects' and the sprint's among them (1.21.11 LivingEntity.java:2224-2225, 26.3
            // AttributeInstance.java:151-169)
            movementSpeedValue: this.attributeValueOf(attributes, 'movementSpeed'),
            gravity: this.attributeValueOf(attributes, 'gravity'),
            stepHeight: this.attributeValueOf(attributes, 'stepHeight'),
            jumpStrength: this.attributeValueOf(attributes, 'jumpStrength'),
            sneakingSpeed,
            movementEfficiency: this.attributeValueOf(attributes, 'movementEfficiency'),
            waterMovementEfficiency: this.attributeValueOf(attributes, 'waterMovementEfficiency'),
            burningTime: this.attributeValueOf(attributes, 'burningTime'),
            airDragModifier: this.attributeValueOf(attributes, 'airDragModifier'),
            frictionModifier: this.attributeValueOf(attributes, 'frictionModifier'),
            entityBounciness: state.entityBounciness ?? this.attributeValueOf(attributes, 'bounciness'),
            blockBounciness: state.blockBounciness,
            fallDamageMultiplier: state.fallDamageMultiplier ?? this.attributeValueOf(attributes, 'fallDamageMultiplier'),
            jumpBoostAmplifier: jumpBoost,
            levitationAmplifier: levitation,
            dolphinsGrace: this.effectAmplifier(state, state.dolphinsGrace, 'DolphinsGrace') !== undefined ? 1 : undefined,
        };
        for (const [key, value] of Object.entries(optional)) if (value !== undefined) (out as Record<string, unknown>)[key] = value;
        return out;
    }

    /**
     * The amplifier of an effect the state has, undefined without it: its effects' (where before 1.20.5 a negative one,
     * `/effect ... 255`, still counts), else the level mineflayer-physics-utils keeps for it.
     */
    private effectAmplifier(state: PlayerState, level: number, name: string): number | undefined {
        return this.effectAmplifierByName(name, state.effects) ?? (level > 0 ? level - 1 : undefined);
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
        return !!this.attribute(state.attributes, 'movementSpeed')?.modifiers?.some(entry => String(entry.uuid ?? entry.id).toLowerCase() === id);
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
            const { boundingBox: moveBox, position: p } = entity.ticksBeforePlayer ? ticked : entity;
            const { boundingBox: tickBox, position: tp } = ticked;
            const b = this.armorStandBox(state, entity, moveBox, p);
            const tb = this.armorStandBox(state, entity, tickBox, tp);
            out.set([
                ENTITY_KINDS[this.entityKind(state, entity)], entity.ticksBeforePlayer ? 1 : 0,
                p.x, p.y, p.z, b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ,
                tp.x, tp.y, tp.z, tb.minX, tb.minY, tb.minZ, tb.maxX, tb.maxY, tb.maxZ,
                this.teamPushes(state, entity) ? 0 : NativePlayerEntityFlag.NO_TEAM_PUSH,
            ], i * NATIVE_PLAYER_ENTITY_LENGTH);
        });
        return out;
    }

    /**
     * EntitySelector.pushableBy(entity).test(player) for its team rules: the teams of the bot's scoreboard (bot.teamMap)
     * by scoreboard name, a player's name, another entity's UUID (Entity.getScoreboardName). No push when the pusher's
     * rule or the player's is never, when one is pushOwnTeam between allies, or one is pushOtherTeams between others
     * (1.9.4 EntityPredicate.java:57-85, 1.16.5 EntityPredicates.java:30-57, 1.21.11 EntitySelector.java:27-54, 26.3
     * :39-70).
     */
    private teamPushes(state: PlayerState, entity: PlayerState['nearbyEntities'][number]): boolean {
        const bot = state.bot;
        const teams = bot?.teamMap;
        if (!teams) return true;
        const other = entity.id != null ? bot?.entities?.[entity.id] : undefined;
        const ownTeam = other && this.teamOf(state, other, teams);
        const ownRule = collisionRuleOf(ownTeam);
        if (ownRule === 'never') return false;
        const playerName = bot?.username ?? bot?.entity?.username;
        const theirTeam = playerName != null ? teams[playerName] : undefined;
        const theirRule = collisionRuleOf(theirTeam);
        if (theirRule === 'never') return false;
        const allied = ownTeam != null && ownTeam === theirTeam;
        if ((ownRule === 'pushOwnTeam' || theirRule === 'pushOwnTeam') && allied) return false;
        return (ownRule !== 'pushOtherTeams' && theirRule !== 'pushOtherTeams') || allied;
    }

    /**
     * Entity.getTeam: the team of the scoreboard name (a player's name, another entity's UUID); a tamed animal's whose
     * owner the client holds is its owner's team, before its own to 1.21.4 (1.9.4 TameableEntity.java:181-190, 1.21.4
     * :199-208), after it from 1.21.5, the root owner's (1.21.5 TamableAnimal.java:196-210, 1.21.11 :179-191,
     * OwnableEntity.java:15-37).
     */
    private teamOf(state: PlayerState, entity: TeamEntity, teams: TeamMap): TeamTeam {
        const own = scoreboardTeam(entity, teams);
        let owner = this.tamedOwnerOf(state, entity);
        if (owner == null) return own;
        // (to 1.21.4 the owner is a player, whose team is its name's)
        if (this.lt('1.21.5')) return scoreboardTeam(owner, teams);
        if (own != null) return own;
        // getRootOwner: up the owners while one is tamed, none on a loop
        const seen = new Set<TeamEntity>([entity]);
        for (let next = this.tamedOwnerOf(state, owner); next != null; next = this.tamedOwnerOf(state, owner)) {
            if (seen.has(next)) return undefined;
            seen.add(owner);
            owner = next;
        }
        return scoreboardTeam(owner, teams);
    }

    /**
     * The owner of a tamed TamableAnimal (its synced flags' tame bit, 4, and owner UUID, AbstractHorse's flags' index
     * before minecraft-data names them) among what the client holds: a player to 1.21.4 (TameableEntity.getOwner:
     * world.getPlayerByUuid, 1.9.4 TameableEntity.java:159-166), any living entity from 1.21.5 (EntityReference.get).
     */
    private tamedOwnerOf(state: PlayerState, entity: TeamEntity): TeamEntity | undefined {
        const name = entity.name?.toLowerCase();
        if (name == null || !TAMABLE.has(name) || this.lt('1.9') || ((name === 'ocelot' || name === 'ozelot') && !this.lt('1.14'))) return undefined;
        const keys = (this.data as unknown as { entitiesByName?: Record<string, { metadataKeys?: string[] } | undefined> }).entitiesByName?.[entity.name!]?.metadataKeys;
        const named = keys?.indexOf('flags') ?? -1;
        const flagsAt = named >= 0 ? named : this.lt('1.10') ? 12 : this.lt('1.14') ? 13 : this.lt('1.15') ? 15 : this.lt('1.17') ? 16 : 17;
        const ownerAt = named >= 0 ? keys!.indexOf('owneruuid') : flagsAt + 1;
        const flags = entity.metadata?.[flagsAt];
        const ownerData = entity.metadata?.[ownerAt];
        const uuid = typeof ownerData === 'string' ? ownerData : (ownerData as { uuid?: unknown } | undefined)?.uuid;
        if (typeof flags !== 'number' || (flags & 0x04) === 0 || typeof uuid !== 'string') return undefined;
        const bot = state.bot;
        const players = this.lt('1.21.5');
        const self = bot?.entity as TeamEntity | undefined;
        const selfUuid = self?.uuid ?? bot?.player?.uuid;
        if (self && selfUuid?.toLowerCase() === uuid.toLowerCase()) return { ...self, type: 'player', username: self.username ?? bot?.username };
        return Object.values(bot?.entities ?? {}).find(e => e != null && e.uuid?.toLowerCase() === uuid.toLowerCase() && (!players || e.type === 'player'));
    }

    /** LivingEntity.isSleeping: in a bed (the sleeping pose from 1.14; mineflayer's bot.isSleeping). */
    private isSleeping(state: PlayerState): boolean {
        return state.pose === Pose.SLEEPING || !!state.bot?.isSleeping;
    }

    /** LivingEntity.isAlive is false: no health left (mineflayer's bot.health). */
    private isDead(state: PlayerState): boolean {
        const health = state.bot?.health;
        return typeof health === 'number' && health <= 0;
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

    private entityKind(state: PlayerState, entity: PlayerState['nearbyEntities'][number]): string {
        if (entity.attachedRocket) return 'attached_rocket';
        if (entity.sameVehicle) return 'inert';
        if (isBoatName(entity.name)) return 'boat';
        if (entity.name === 'shulker') return 'shulker';
        // an adult on its still timeout collides from every side (HappyGhast.canBeCollidedWith: isOnStillTimeout, the
        // synced STAYS_STILL; 1.21.6 HappyGhast.java:544-553, 1.21.11 :549-558, :523-524)
        if (entity.name === 'happy_ghast') return entity.baby ? 'living' : entity.staysStill ? 'happy_ghast_stays_still' : 'happy_ghast';
        // the armor stand pushes no one and is no collider, but is a LivingEntity: it bounces a riptide spin
        // (LivingEntity.checkAutoSpinAttack, 1.13.2 LivingEntity.java:2022; ArmorStandEntity.java:40)
        if (entity.name === 'armor_stand') return 'still_living';
        if (NEVER_PUSHING_LIVING.has(entity.name)) return 'still_living';
        // a spectator (Player.tick: noPhysics = isSpectator(), which Entity.push(Entity) skips; 1.9.4
        // PlayerEntity.java:248, 1.21.11 Player.java:239) neither pushes nor meets the spin (getEntities: NO_SPECTATORS,
        // 1.13.2 class_3593.java:18-19, 1.21.11 EntityGetter.java:29-31)
        if (entity.name === 'player' && this.isSpectatorPlayer(state, entity)) return 'inert';
        return entity.living ? 'living' : 'inert';
    }

    /**
     * The box of an armor stand by its synced client flags (1.13+, where only the riptide spin meets it): a marker's
     * (16) is 0 x 0 and a small one's (1) half the type's 0.5F x 1.975F, around its position (1.13.2
     * ArmorStandEntity.java:121-127, 624-630, 669, 694; 1.14 :798-799; 1.21.11 ArmorStand.java:59-70). The flags are
     * the armor stand's first synced data: at 11 in 1.13, 13 in 1.14 (the pose, the sleeping position), 14 in 1.15 -
     * 1.16 (the stingers), 15 from 1.17 (the frozen ticks).
     */
    private armorStandBox(state: PlayerState, entity: PlayerState['nearbyEntities'][number], b: Box, p: Vec3): Box {
        if (entity.name !== 'armor_stand' || entity.id == null || this.lt('1.13')) return b;
        const index = this.lt('1.14') ? 11 : this.lt('1.15') ? 13 : this.lt('1.17') ? 14 : 15;
        const flags = state.bot?.entities?.[entity.id]?.metadata?.[index];
        if (typeof flags !== 'number') return b;
        const scale = (flags & 16) !== 0 ? 0 : (flags & 1) !== 0 ? f32(0.5) : 1;
        if (scale === 1) return b;
        return this.boxAround(p, f32(f32(0.5) * scale), f32(f32(1.975) * scale));
    }

    /** Whether a player entity near the bot plays in spectator mode (mineflayer's bot.players[name].gamemode, 3). */
    private isSpectatorPlayer(state: PlayerState, entity: PlayerState['nearbyEntities'][number]): boolean {
        const bot = state.bot;
        const other = entity.id != null ? bot?.entities?.[entity.id] : undefined;
        const name = other?.username;
        return name != null && bot?.players?.[name]?.gamemode === 3;
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
        // EntityFluidInteraction.hasFluidAndLoaded: from 26.1 no fluid is touched while a chunk under the cells widened by
        // one is not loaded (26.1 EntityFluidInteraction.java:44, 94-110)
        let loaded = true;
        if (!this.lt('26.1')) {
            for (let cx = (Math.floor(deflated.minX) - 1) >> 4; cx <= Math.ceil(deflated.maxX) >> 4; cx += 1) {
                for (let cz = (Math.floor(deflated.minZ) - 1) >> 4; cz <= Math.ceil(deflated.maxZ) >> 4; cz += 1) {
                    if (!sync.chunkLoaded(cx, cz)) loaded = false;
                }
            }
        }
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
        state.isInWater = inWater && loaded;
        // Entity.updateFluidOnEyes: 1.13 - 1.15 compare the eyes with y + (the water height + 0.11111111F) in float and
        // take it at once (1.13.2 and 1.15.2 Entity.isUnderLiquid bytecode: fadd, fadd); 1.16 - 1.20.4 look
        // 0.1111111119389534 below the eyes, 1.20.5+ at them (1.20.4 Entity.java:1222, 1.20.5 :1237), and from 1.16
        // isUnderWater is the eyes of the tick before
        const wasEyeInWater = state.eyeInWater ?? state.isUnderWater;
        let eyeY = state.pos.y + f32(this.eyeHeight(state));
        if (!this.lt('1.16') && this.lt('1.20.5')) eyeY -= 0.1111111119389534;
        const eyeX = Math.floor(state.pos.x);
        const eyeZ = Math.floor(state.pos.z);
        const heightAt = (block: DescribedBlock, y: number) => !this.lt('1.14') && isWaterish(sync.blockAt(eyeX, y + 1, eyeZ))
            ? 1
            : f32((8 - this.renderedDepth(block)) / 9);
        let eyeInWater = false;
        if (!this.lt('26.1')) {
            // EntityFluidInteraction.update's eyesInside: a water of the scan in the eyes' column with
            // bottom <= eyeY <= top in double; 26.3 tops a source under a block sturdy below at 1
            // (getHeightForCamera; 26.1 EntityFluidInteraction.java:61, 70, 26.3 :84-87, FluidState.java:53-60)
            for (let y = Math.floor(deflated.minY); y < Math.ceil(deflated.maxY) && loaded && !eyeInWater; y += 1) {
                const block = sync.blockAt(eyeX, y, eyeZ);
                if (!isWaterish(block)) continue;
                const top = y + heightAt(block, y);
                if (top < deflated.minY || eyeY < y) continue;
                const camera = !this.lt('26.3') && block.fluid?.source && sync.bottomFaceSturdyAt(eyeX, y + 1, eyeZ) ? y + 1 : top;
                eyeInWater = eyeY <= camera;
            }
        } else {
            const y = Math.floor(eyeY);
            const block = sync.blockAt(eyeX, y, eyeZ);
            if (isWaterish(block)) {
                const height = heightAt(block, y);
                if (this.lt('1.16')) eyeInWater = eyeY < f32(y + f32(height + f32(0.11111111)));
                else eyeInWater = f32(y + height) > eyeY;
            }
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

    /**
     * Entity.updateSwimming (1.13+): sprinting under water starts swimming, from 1.17.1 only with water at the feet
     * block (1.17 Entity.java:1052; 1.17.1 Entity.updateSwimming bytecode 62-79; 1.18 :1032-1034), and sprinting in
     * water keeps it.
     */
    private updateSwimming(state: PlayerState, sync: NativeWorldSync): void {
        if (this.lt('1.13') || state.flying) {
            state.swimming = false;
        } else if (state.swimming) {
            state.swimming = state.sprinting && state.isInWater;
        } else {
            const feet = () => sync.blockAt(Math.floor(state.pos.x), Math.floor(state.pos.y), Math.floor(state.pos.z));
            state.swimming = state.sprinting && state.isUnderWater && (this.lt('1.17.1') || isWaterish(feet()));
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
            // !abilities.flying: a flying (or spectating) player does not crouch (1.14 ClientPlayerEntity.java:582-586,
            // 1.16.4 LocalPlayer.java:603, 1.21.11 :715, 26.3 :764)
            const flying = state.flying || state.gameMode === 'spectator';
            state.crouching = !flying && !this.swimmingAndNotFlying(state) && this.isFree(sync, sneakBox, state)
                && (state.prevControl.sneak || !this.isFree(sync, standBox, state));
        } else {
            state.crouching = !this.swimmingAndNotFlying(state) && state.prevControl.sneak;
        }
        const movingSlowly = this.isMovingSlowly(state);
        // 1.21.4 only: LocalPlayer.shouldStopSprinting right after the input's tick (isFallFlying() || hasBlindness() ||
        // isMovingSlowly() || ..., 1.21.4 ClientPlayerEntity.java:815-820; the passenger term has no place here)
        if (this.gt('1.21.3') && this.lt('1.21.5')) {
            const blind = this.effectAmplifier(state, state.blindness, 'Blindness') !== undefined;
            if (state.fallFlying || blind || movingSlowly || (state.isUsingItem && !state.isUnderWater)) this.setSprinting(state, false);
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
        // 1.8: a sprint lasts 600 ticks at most (EntityPlayerSP.onLivingUpdate, 1.8.9 ClientPlayerEntity.java:487-492)
        const sprintingTicksLeft = state.sprintingTicksLeft ?? 0;
        if (this.lt('1.9') && sprintingTicksLeft > 0) {
            state.sprintingTicksLeft = sprintingTicksLeft - 1;
            if (sprintingTicksLeft === 1) this.setSprinting(state, false);
        }
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
            // the sprint key alone is held back in shallow water (1.13.2 EntityPlayerSP.java:707, 1.21.4
            // ClientPlayerEntity.java:707-709)
            const shallow = !this.lt('1.13') && state.isInWater && !state.isUnderWater;
            if (canStart && !shallow && state.control.sprint) this.setSprinting(state, true);
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

    /**
     * LocalPlayer.canStartSprinting, and before 1.19.4 the terms both sprint starts share: no glide term before 1.19.4,
     * a strict one to 1.21.4 (1.19.3 ClientPlayerEntity.java:689-712, 1.19.4 :1020-1027), the crouch from 1.21.4
     * (1.21.4 :1054-1062); the glide and shallow water spare a player under water from 1.21.5 (1.21.5
     * LocalPlayer.java:1065-1074), and from 1.21.9 a flying player is not held back in shallow water (isSprintingPossible,
     * 1.21.9 LocalPlayer.java:1055-1066). Before 1.21.5 the shallow water holds back only the sprint key (inputsToSprint).
     */
    private canStartSprinting(state: PlayerState, heading: { forward: number }): boolean {
        const shallow = state.isInWater && !state.isUnderWater;
        let terms = true;
        if (this.gt('1.21.4')) {
            terms = (!state.fallFlying || state.isUnderWater) && (!this.isMovingSlowly(state) || state.isUnderWater)
                && (!shallow || (this.gt('1.21.8') && state.flying));
        } else if (this.gt('1.19.3')) {
            terms = !state.fallFlying && (this.lt('1.21.4') || !this.isMovingSlowly(state) || state.isUnderWater);
        }
        return !state.sprinting
            && this.hasEnoughImpulseToStartSprinting(state, heading)
            && this.hasEnoughFoodToSprint(state)
            && !this.slowDueToUsingItem(state)
            && this.effectAmplifier(state, state.blindness, 'Blindness') === undefined
            && terms;
    }

    /** isWalking: 0.8 forward, from 1.14 any forward under water (1.14.4 LocalPlayer.java:925-928). */
    private hasEnoughImpulseToStartSprinting(state: PlayerState, heading: { forward: number }): boolean {
        if (this.gt('1.21.4')) return heading.forward > 1e-5;
        if (this.lt('1.14')) return heading.forward >= 0.8;
        return heading.forward >= (state.isUnderWater ? 1e-5 : 0.8);
    }

    private hasEnoughFoodToSprint(state: PlayerState): boolean {
        return state.food > 6 || state.mayFly;
    }

    private hasForwardImpulseToKeepSprinting(heading: { forward: number }): boolean {
        return this.lt('1.14') ? heading.forward >= 0.8 : heading.forward > 1e-5;
    }

    /** From 1.21.9 a flying player keeps the sprint in shallow water (isSprintingPossible, 1.21.9 LocalPlayer.java:841-845). */
    private shouldStopRunSprinting(state: PlayerState, heading: { forward: number }): boolean {
        return (this.gt('1.21.3') && this.effectAmplifier(state, state.blindness, 'Blindness') !== undefined)
            || !this.hasForwardImpulseToKeepSprinting(heading)
            || !this.hasEnoughFoodToSprint(state)
            || (state.isCollidedHorizontally && !state.isCollidedHorizontallyMinor)
            || (state.isInWater && !state.isUnderWater && !(this.gt('1.21.8') && state.flying));
    }

    /**
     * Before 1.21.5 the food ends a swim only off the ground without the sneak key, and blindness not at all
     * (!onGround && !sneak && (!forward || !food) || !isInWater, 1.14.4 LocalPlayer.java:632-640, 1.21.4
     * ClientPlayerEntity.java:713-718); from 1.21.5 shouldStopSwimSprinting (1.21.5 LocalPlayer.java:846-852).
     */
    private shouldStopSwimSprinting(state: PlayerState, heading: { forward: number }): boolean {
        const forward = this.hasForwardImpulseToKeepSprinting(heading);
        if (this.lt('1.21.5')) {
            return (!state.onGround && !state.control.sneak && (!forward || !this.hasEnoughFoodToSprint(state))) || !state.isInWater;
        }
        return this.effectAmplifier(state, state.blindness, 'Blindness') !== undefined
            || !state.isInWater
            || (!forward && !state.onGround && !state.control.sneak)
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
        // 1.8's EntityPlayerSP.setSprinting (1.8.9 ClientPlayerEntity.java:349-352)
        if (this.lt('1.9')) state.sprintingTicksLeft = value ? 600 : 0;
        state.sprinting = value;
    }

    /**
     * LocalPlayer.aiStep's flight: a second press of jump within 7 ticks toggles flying where the player may fly (since
     * 1.20.5 a start on the ground jumps); since 1.15 a fresh jump in the air with a usable elytra starts gliding. The jump
     * is the input's: an auto-jump presses it (makeJump) for its tick and the wasJumping of the next, and does not toggle
     * flying (!wasJumping && jumping && !autoJumping: 1.10.2 ClientPlayerEntity.java:658-716, 1.15
     * ClientPlayerEntity.java:628-711, 1.21.11 LocalPlayer.java:738-753, 799, 815).
     */
    private inputsToFly(state: PlayerState, sync: NativeWorldSync): void {
        let flyChanged = false;
        const wasJumping = state.prevControl.jump;
        const jumping = state.control.jump || state.autoJumping;
        if (state.mayFly) {
            if (state.gameMode === 'spectator') {
                state.flying = true;
                flyChanged = true;
            } else if (!wasJumping && jumping && !state.autoJumping) {
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
        // (Player.tryToStartFallFlying does not start under levitation from 1.16, nor in lava from 26.3: !isInLiquid(),
        // 26.3 Player.java:1351-1352; 26.2 :1366 has !isInWater()); aiStep tests the climbable from 1.15.2 (!onLadder();
        // none in 1.15 ClientPlayerEntity.java:711-713 nor 1.15.1, 1.16 :742)
        const levitating = this.effectAmplifier(state, state.levitation, 'Levitation') !== undefined && !this.lt('1.16');
        const inLava = state.isInLava && !this.lt('26.3');
        const climbing = state.onClimbable && !this.lt('1.15.2');
        if (!this.lt('1.15') && jumping && !wasJumping && !flyChanged && !state.flying && !climbing && !state.onGround
            && !state.isInWater && !inLava && !state.fallFlying && !levitating && state.validElytraEquipped) {
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
     * LivingEntity.isAutoSpinAttack (1.13+): bit 4 of the living entity flags the server syncs (the client never sets it,
     * 1.16.5 LivingEntity.java:2603-2611, 1.21.11 Player.java:520-527): the synced data at 6 in 1.13, 7 from 1.14 (the
     * pose), 8 from 1.17 (the frozen ticks).
     */
    private isAutoSpinAttack(state: PlayerState): boolean {
        if (this.lt('1.13')) return false;
        const index = this.lt('1.14') ? 6 : this.lt('1.17') ? 7 : 8;
        const flags = state.bot?.entity?.metadata?.[index];
        return typeof flags === 'number' && (flags & 4) !== 0;
    }

    /**
     * Player.updatePlayerPose (1.14+): nothing changes without room to swim; else the desired pose when the player is a
     * spectator, a passenger or fits in it, else crouching where that fits, else swimming (1.14 PlayerEntity.java:401-428,
     * 1.20.4 :373-401, 1.21.5 Player.java:449-463, 1.21.11 :352-366).
     */
    private updatePoses(state: PlayerState, sync: NativeWorldSync): void {
        const fits = (pose: number) => this.isFree(sync, shrink(this.collider(pose, state.pos, state.scale), 1e-7), state);
        if (!fits(Pose.SWIMMING)) return;
        const wanted = this.desiredPose(state);
        if (state.gameMode === 'spectator' || this.isPassenger(state) || fits(wanted)) state.pose = wanted;
        else if (fits(Pose.SNEAKING)) state.pose = Pose.SNEAKING;
        else state.pose = Pose.SWIMMING;
    }

    /**
     * The pose updatePlayerPose wants: up to 1.21.4 gliding, sleeping, swimming, the riptide spin, crouching (not
     * while flying), standing (1.14 PlayerEntity.java:403-416, 1.21.4 :437-450); from 1.21.5 getDesiredPose's
     * sleeping, swimming, gliding, then the rest in the same order (1.21.5 Player.java:465-477, 1.21.11 :368-380).
     */
    private desiredPose(state: PlayerState): number {
        const gliding = state.fallFlying;
        if (gliding && this.lt('1.21.5')) return Pose.FALL_FLYING;
        if (this.isSleeping(state)) return Pose.SLEEPING;
        if (this.swimmingAndNotFlying(state)) return Pose.SWIMMING;
        if (gliding) return Pose.FALL_FLYING;
        if (this.isAutoSpinAttack(state)) return Pose.SPIN_ATTACK;
        return state.control.sneak && !state.flying ? Pose.SNEAKING : Pose.STANDING;
    }

    /** Entity.isPassenger: the state's passenger flag, else whether mineflayer has the bot riding a vehicle. */
    private isPassenger(state: PlayerState): boolean {
        if (typeof state.passenger === 'boolean') return state.passenger;
        return state.bot?.vehicle != null || state.bot?.entity?.vehicle != null;
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

    /** The player's height before 1.14 (EntityPlayer.updateSize; 1.8 keeps 1.8F): 0.6F gliding (1.13: or swimming, or
     * in the riptide spin, 1.13.2 PlayerEntity.java:419-437), 1.65F sneaking, else 1.8F. */
    private legacyPlayerHeight(state: PlayerState, sneaking: boolean): number {
        if (this.lt('1.9')) return f32(1.8);
        if (state.fallFlying || (!this.lt('1.13') && (state.swimming || this.isAutoSpinAttack(state)))) return f32(0.6);
        return f32(sneaking ? 1.65 : 1.8);
    }

    /** LivingEntity.onClimbable: a climbable block at the feet, or (1.9+) an open trapdoor over a ladder facing its way. */
    private isInClimbable(state: PlayerState, sync: NativeWorldSync): boolean {
        if (state.gameMode === 'spectator') return false;
        // Player.onClimbable: never while flying from 1.21.4 (1.21.4 PlayerEntity.java:2219-2221)
        if (state.flying && !this.lt('1.21.4')) return false;
        const x = Math.floor(state.pos.x);
        const y = Math.floor(state.pos.y);
        const z = Math.floor(state.pos.z);
        const feet = sync.blockAt(x, y, z);
        if (!feet) return false;
        // a fall-flying player is not on a #can_glide_through block from 1.21.11 (1.21.11 LivingEntity.java:1675-1676)
        if (state.fallFlying && !this.lt('1.21.11') && GLIDE_THROUGH.has(feet.name)) return false;
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
        // (before 1.17 the synced pose 6 is DYING, LONG_JUMPING coming in at 6 with 1.17: 1.14 EntityPose.java:4-10,
        // 1.16.5 :4-10, 1.17 :4-11)
        if (pose === Pose.LONG_JUMPING && this.lt('1.17')) pose = Pose.DYING;
        const size = POSE_SIZES[pose] ?? POSE_SIZES[Pose.STANDING];
        // the sleeping and dying dimensions are fixed, which the scale leaves (1.20.5 LivingEntity.java:3251,
        // PlayerEntity.java:147, EntityDimensions.java:29-38; 1.21.11 Avatar.java:55)
        if (scale === 1 || pose === Pose.SLEEPING || pose === Pose.DYING) return size;
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
        // 1.13: 0.4F swimming, gliding or in a 0.6F box, the riptide spin's (1.13.2 PlayerEntity.java:1871-1878)
        const lowBox = !this.lt('1.13') && this.lt('1.14') && state.pose !== Pose.SLEEPING
            && (state.swimming || state.fallFlying || state.keptBoundingBox?.height === f32(0.6));
        if (state.pose === Pose.SLEEPING) {
            // 0.2F asleep, its fixed dimensions unscaled (1.8.9 PlayerEntity.java:1643-1653, which still takes the
            // 0.08F off sneaking; 1.12.2 :1769-1780; 1.14 LivingEntity.java:2749; 1.20.5 LivingEntity.java:170, 3251)
            return this.lt('1.9') && state.prevControl.sneak ? f32(f32(0.2) - f32(0.08)) : f32(0.2);
        }
        if (lowBox) {
            eyeHeight = f32(0.4);
        } else if (this.lt('1.14') && (state.prevControl.sneak || sneakBox)) {
            eyeHeight = f32(f32(1.62) - f32(0.08));
        } else if (state.pose === Pose.SNEAKING) {
            eyeHeight = 1.27;
        } else if (state.pose === Pose.DYING) {
            // 1.62F dying: the default case before 1.20.5 (1.14 PlayerEntity.java:1792-1802), then the fixed
            // dimensions' eye, which the scale leaves (1.20.5 PlayerEntity.java:147, EntityDimensions.java:29-38;
            // 1.21.11 Avatar.java:55)
            return 1.62;
        } else if (state.pose === Pose.STANDING || state.pose === Pose.LONG_JUMPING || !(state.pose in POSE_SIZES)) {
            eyeHeight = 1.62;
        } else {
            eyeHeight = 0.4;
        }
        return state.scale === 1 ? eyeHeight : f32(f32(eyeHeight) * state.scale);
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
