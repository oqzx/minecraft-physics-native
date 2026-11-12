import type { OpenShulkerBox } from './shulkers';
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
    type NativeBoatVehicle,
    type NativeMovementSpeedAttribute,
    type NativeMovingPiston,
    type NativePistonPushedEntity,
    type NativePistonPushedResult,
    type NativePlayer,
    type NativeSimulationState,
    type NativeVec3,
} from '../native';
import { NativeEngine, type NativeEngineOptions, type PhysicsRegistry } from './base';
import { EFFECT, FLUID_LAVA, GLIDE_THROUGH, type DescribedBlock } from './blocks';
import { livingFlagsSlot } from './physics-features';
import { floatRadians, mthFor, viewVector, type Mth } from './mth';
import type { Attribute, AttributeModifier, Box, EquippedItem, ItemEnchants, PhysicsCtx, PlayerState, Vec3 } from './types';
import { bodyReach, syncReads, type NativeWorldSync, type PrismarineWorld } from './world';

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
    /** The collision boxes it has once it has gone on this tick, for the blocks after it, if they are other ones. */
    nextShapes?: number[][];
    /** The name of the block it moves: a slime block bounces the player, a honey block carries it. */
    movedBlock?: string;
}

/** The slime block's names: slime before 1.13 (minecraft-data's), slime_block from it. */
const SLIME_BLOCKS = new Set(['slime', 'slime_block']);

/** The engine's form of a moving piston block. */
export function nativeMovingPiston(piston: MovingPiston): NativeMovingPiston {
    const [dx, dy, dz] = piston.direction;
    return {
        x: piston.pos[0],
        y: piston.pos[1],
        z: piston.pos[2],
        facing: DIRECTIONS.findIndex(([x, y, z]) => x === dx && y === dy && z === dz),
        extending: piston.extending,
        source: piston.source,
        progress: piston.progress,
        shapes: piston.shapes.map(([minX, minY, minZ, maxX, maxY, maxZ]) => nativeBox(box(minX, minY, minZ, maxX, maxY, maxZ))),
        outlineShapes: piston.outlineShapes?.map(([minX, minY, minZ, maxX, maxY, maxZ]) => nativeBox(box(minX, minY, minZ, maxX, maxY, maxZ))),
        nextShapes: piston.nextShapes?.map(([minX, minY, minZ, maxX, maxY, maxZ]) => nativeBox(box(minX, minY, minZ, maxX, maxY, maxZ))),
        bounces: SLIME_BLOCKS.has(piston.movedBlock ?? ''),
        stickyForEntities: piston.movedBlock === 'honey_block',
    };
}

/** What a tick reads back of the state that mineflayer-physics-utils' PlayerState.clone leaves out. */
const CLONE_DROPS = [
    'fallDistance', 'prevForwardImpulse', 'sprintingTicksLeft', 'autoJumpEnabled', 'autoJumpTime', 'autoJumping', 'autoSpinAttackTicks', 'itemUseTicks',
    'usedItemRiptide', 'riptide', 'eyeInWater', 'isInPowderSnow', 'wasInPowderSnow', 'scale', 'sizedPose', 'passenger', 'isVehicle', 'boatVehicle', 'invulnerable',
    'noGravity', 'flySpeed', 'mayFly', 'useItemCanSprint', 'useItemSpeedMultiplier', 'fireworkRockets', 'fireworkRocketsBeforePlayer', 'nearbyEntities',
    'vehicleCanSprint', 'vehicleJumpable', 'vehicleOnGround', 'sprintWindow', 'seaLevel', 'safeFallDistance', 'fallDamageImmune', 'fallDamageMultiplier',
    'entityBounciness', 'blockBounciness', 'controlledByLocalInstance', 'canSimulateMovement', 'canWalkOnPowderSnow', 'removed',
] as const;

/** Direction.from3DDataValue: down, up, north, south, west, east. */
const DIRECTIONS = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/** LivingEntity.onClimbable's blocks: the #climbable tag from 1.16, ladder, vine and scaffolding before it. */
const CLIMBABLE = new Set(['ladder', 'vine', 'scaffolding', 'weeping_vines', 'weeping_vines_plant', 'twisting_vines', 'twisting_vines_plant', 'cave_vines', 'cave_vines_plant']);

type Size = { width: number; height: number };

/** The slots mineflayer's getEquipmentDestSlot names for the worn armor, head first (EquipmentSlot's ALL_ARMOR). */
const ARMOR_SLOTS = ['head', 'torso', 'legs', 'feet'] as const;
type ArmorSlot = typeof ARMOR_SLOTS[number];

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
    /** The frozen ticks last synced: the engine counts on from them where the client does (from 1.21.5) until the next sync. */
    frozen: number | undefined;
    /** The box the engine keeps before 1.17 (null without one), the size the player moved with and the one its tick left. */
    kept: Box | null;
    size: Size;
    keptSize: Size;
    /** The native world it last ticked in. */
    world?: object;
    /** Entity.verticalCollisionBelow as the last tick left it: no PlayerState field holds it. */
    verticalCollisionBelow: boolean;
    /** The yaw the last tick's aiStep left in yHeadRot (Player.serverAiStep), which a 1.11 rocket before the player reads. */
    headYaw?: number;
}

type Vec3Like = { x: number; y: number; z: number };

const In = NativePlayerInput;
const InFlag = NativePlayerInputFlag;
const Out = NativePlayerOutput;
const OutFlag = NativePlayerOutputFlag;

/** AttributeModifier.Operation by its id in the attributes packet, as the engine names them. */
const SPEED_OPERATIONS = ['add_value', 'add_multiplied_base', 'add_multiplied_total'];

/** The kinds of the nearby entities in a NativePlayer's entities array. */
const ENTITY_KINDS: Record<string, number> = {
    inert: 0, living: 1, boat: 2, shulker: 3, happy_ghast: 4, still_living: 5, happy_ghast_stays_still: 6,
    attached_rocket: 7, minecart: 8,
};

/**
 * The LivingEntities that never push the local player: the bat (doPush and pushEntities are empty), the parrot (doPush
 * skips players, 1.12+), the ender dragon (its aiStep never pushes) and the vex (noPhysics through its own tick, which
 * Entity.push(Entity) skips, 1.11+). 1.9.4 Bat.java:69, 1.21.11 Bat.java:83-87; 1.12.2 Parrot.java:375-379,
 * 1.21.11 Parrot.java:306-310; 1.9.4 EnderDragon.java:151, 1.21.11 EnderDragon.java:145; 1.11.2
 * Vex.java:65-67, 1.21.11 Vex.java:70-72, Entity.java:1811-1813.
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

const sameBoatVehicle = (a: NativeBoatVehicle | null | undefined, b: NativeBoatVehicle | null | undefined) => {
    if (a == null || b == null) return a == null && b == null;
    return a.underwater === b.underwater
        && (['min', 'max'] as const).every(side => (['x', 'y', 'z'] as const)
            .every(axis => Object.is(a.boundingBox[side][axis], b.boundingBox[side][axis])));
};
const sameSpeedAttribute = (a?: NativeMovementSpeedAttribute, b?: NativeMovementSpeedAttribute) => a === b || (!!a && !!b
    && a.base === b.base && a.frost === b.frost && a.modifiers.length === b.modifiers.length
    && a.modifiers.every((modifier, i) => modifier.amount === b.modifiers[i].amount && modifier.operation === b.modifiers[i].operation));
const sameInputs = (a: Partial<NativeSimulationState>, b: Partial<NativeSimulationState>) => {
    const keys = Object.keys(a) as (keyof NativeSimulationState)[];
    return keys.length === Object.keys(b).length && keys.every(key => (key === 'boatVehicle'
        ? sameBoatVehicle(a.boatVehicle, b.boatVehicle)
        : key === 'movementSpeedAttribute' ? sameSpeedAttribute(a[key], b[key]) : Object.is(a[key], b[key])));
};
const sameBox = (a: Box | null, b: Box | null) => a === b || (!!a && !!b && a.minX === b.minX && a.minY === b.minY
    && a.minZ === b.minZ && a.maxX === b.maxX && a.maxY === b.maxY && a.maxZ === b.maxZ);

const box = (minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): Box => ({ minX, minY, minZ, maxX, maxY, maxZ });
const shrink = (b: Box, by: number): Box => box(b.minX + by, b.minY + by, b.minZ + by, b.maxX - by, b.maxY - by, b.maxZ - by);
const nativeBox = (b: Box) => ({ min: { x: b.minX, y: b.minY, z: b.minZ }, max: { x: b.maxX, y: b.maxY, z: b.maxZ } });
const vec = (v: { x: number; y: number; z: number }): NativeVec3 => ({ x: v.x, y: v.y, z: v.z });
const validateBoatVehicle = (boat: NativeBoatVehicle | null | undefined): void => {
    if (boat == null) return;
    const min = boat.boundingBox?.min;
    const max = boat.boundingBox?.max;
    if (typeof boat.underwater !== 'boolean' || !min || !max || !(['x', 'y', 'z'] as const)
        .every(axis => Number.isFinite(min[axis]) && Number.isFinite(max[axis]) && min[axis] <= max[axis])) {
        throw new Error('boatVehicle must contain finite, ordered boundingBox coordinates and an underwater boolean');
    }
};
const copyBoatVehicle = (boat: NativeBoatVehicle | null | undefined): NativeBoatVehicle | undefined => {
    validateBoatVehicle(boat);
    return boat == null ? undefined : {
        underwater: boat.underwater,
        boundingBox: { min: vec(boat.boundingBox.min), max: vec(boat.boundingBox.max) },
    };
};
const addedComponent = (item: EquippedItem, type: string) => item.componentMap?.get(type);
const removedComponent = (item: EquippedItem, type: string) => !!item.removedComponents?.some(removed => removed.type === type);
/** ItemStack.getMaxDamage: the stack's max_damage component (1.20.5+), else the registry's. */
const maxDamageOf = (item: EquippedItem) => (addedComponent(item, 'max_damage')?.data as number | undefined) ?? item.maxDurability ?? 0;
/** The slot of an item's own equippable component (Items.java: `.humanoidArmor` and the elytra's, 26.3 :2105), for a stack that adds no other. */
const defaultEquippableSlot = (name: string) => (name === 'elytra' ? 'chest' : /_helmet$/.test(name) ? 'head' : /_chestplate$/.test(name) ? 'chest'
    : /_leggings$/.test(name) ? 'legs' : /_boots$/.test(name) ? 'feet' : undefined);
/** ItemStack.nextDamageWillBreak (client1.21.2 ItemStack.java:412-414, 436-438). */
const nextDamageWillBreak = (item: EquippedItem) => {
    const damageable = (!!addedComponent(item, 'max_damage') || (!!item.maxDurability && !removedComponent(item, 'max_damage')))
        && !addedComponent(item, 'unbreakable') && !removedComponent(item, 'damage');
    return damageable && (item.durabilityUsed ?? 0) >= maxDamageOf(item) - 1;
};
/** LivingEntity.canGlideUsing: the stack has GLIDER, its EQUIPPABLE names the slot, and it survives the next damage. */
const canGlideUsing = (item: EquippedItem | null | undefined, slot: string) => {
    const name = item?.name;
    if (!item || name == null) return false;
    if (!addedComponent(item, 'glider') && (name !== 'elytra' || removedComponent(item, 'glider'))) return false;
    const equippable = addedComponent(item, 'equippable')?.data as { slot?: string } | undefined;
    const equippedIn = equippable?.slot ?? (removedComponent(item, 'equippable') ? undefined : defaultEquippableSlot(name));
    return equippedIn === slot && !nextDamageWillBreak(item);
};
const samePosition = (a: { x: number; y: number; z: number }, b: { x: number; y: number; z: number }) => a.x === b.x && a.y === b.y && a.z === b.z;
const isBoatName = (name: string) => /(^|_)(boat|raft)$/.test(name);
const isWaterish = (block: DescribedBlock | null): block is DescribedBlock => !!block?.fluid && block.fluid.type !== FLUID_LAVA;

/** Each state class's own clone(), kept when the adapter takes the class's prototype over (adoptPrototype). */
const PROTOTYPE_CLONES = new WeakMap<object, () => PlayerState>();

/** The prototype of a state's chain that declares its clone(), or null. */
function cloneOwner(state: object): object | null {
    for (let at = Object.getPrototypeOf(state); at && at !== Object.prototype; at = Object.getPrototypeOf(at)) {
        if (Object.prototype.hasOwnProperty.call(at, 'clone')) return at;
    }
    return null;
}

/** The clone() a state's library gave it, past any the adapter put in front of it. */
function libraryCloneOf(state: object): () => PlayerState {
    const owner = cloneOwner(state);
    return (owner && PROTOTYPE_CLONES.get(owner)) ?? (state as { clone(): PlayerState }).clone;
}

/**
 * mineflayer-physics-utils' PlayerState.clone drops the adapter's counters, and the state carries the engine it was made for
 * (PlayerState.ctx, which its clone hands on). So the class's clone() is taken over once, for every state of it: one whose
 * engine is a NativePhysics goes to that engine's cloneState, any other to the library's own clone. A state the engine has
 * never ticked is copied whole so (B104); one it has ticked also takes over its own clone() (adoptClone). The library's
 * clone stays the base of cloneState.
 */
function adoptPrototype(state: object): void {
    const owner = cloneOwner(state);
    if (!owner || PROTOTYPE_CLONES.has(owner)) return;
    const original = (owner as { clone?: () => PlayerState }).clone;
    if (typeof original !== 'function') return;
    PROTOTYPE_CLONES.set(owner, original);
    Object.defineProperty(owner, 'clone', {
        value: function clone(this: PlayerState & { ctx?: unknown }) {
            return this.ctx instanceof NativePhysics ? this.ctx.cloneState(this) : original.call(this);
        },
        configurable: true, writable: true, enumerable: false,
    });
}

/** The PlayerState class of the mineflayer-physics-utils this package resolves, when it resolves one. */
function adoptLibraryPrototype(): void {
    try {
        const library = module.require('@nxg-org/mineflayer-physics-util/dist/physics/states/playerState') as { PlayerState?: { prototype: object } };
        if (library.PlayerState) adoptPrototype(Object.create(library.PlayerState.prototype));
    } catch {
        // no such dependency here: a caller's own copy is taken over by the first of its states the engine sees
    }
}

/**
 * The player physics of one release, backed by the native engine. Make the states with mineflayer-physics-utils
 * (EPhysicsCtx.FROM_BOT(physics, bot)) and tick them with simulate(ctx, world), as with BotcraftPhysics.
 */
export class NativePhysics extends NativeEngine implements IPhysics {
    readonly mth: Mth;
    private readonly carries = new WeakMap<object, Carry>();
    private readonly libraryClones = new WeakMap<object, () => PlayerState>();
    private readonly input = new Float64Array(In.LENGTH);

    constructor(registry: PhysicsRegistry, options: NativeEngineOptions = {}) {
        super(registry, options);
        this.mth = mthFor(!this.lt('1.21.11'));
        adoptLibraryPrototype();
    }

    /**
     * One client tick of the player in ctx (a PlayerState), its world read through world.getBlock. A passenger's tick is
     * Entity.rideTick and never ordinary travel: its position is the seat the caller hands in (see PlayerState.passenger).
     */
    simulate<Ctx extends PhysicsCtx>(ctx: Ctx, world: PrismarineWorld): Ctx['state'] {
        const state = ctx.state as PlayerState;
        validateBoatVehicle(state.boatVehicle);
        this.adoptClone(state);
        state.attributes ??= {};
        // (mineflayer-physics-utils' PlayerState has no such counters: they start with the adapter's first tick)
        state.fallDistance ??= 0;
        state.itemUseTicks ??= 0;
        state.autoSpinAttackTicks ??= 0;
        state.autoJumpTime ??= 0;
        // A velocity packet is the server's word and takes no check: from 1.21.9 its LpVec3 carries up to 1.7179869183E10
        // blocks a tick (26.3 LpVec3.java:17, 70). The motion past what a sync reads, which comes well before the engine's
        // MAX_WALKED_CELLS, is dropped: nothing of the tick ran, and the next tick starts from rest where the player is.
        // (a riptide launch reaches further, and can come only of a use that ended)
        state.moveRefused = !syncReads(this.reachOf(state, !state.isUsingItem && state.itemUseTicks > 0), 0);
        if (state.moveRefused) state.vel.set(0, 0, 0);
        else this.tick(state, world);
        state.age += 1;
        return ctx.state;
    }

    /**
     * A copy of a state to predict on that goes on as the state would: mineflayer-physics-utils' clone() and what it drops,
     * the adapter's counters (the fall, the input's history, the frost) and the engine's own copy of the player. A one-shot
     * event still to be taken (attackHit) stays with the state it came to, so a prediction does not take it twice.
     */
    cloneState<State extends PlayerState>(state: State): State {
        const copy = (this.libraryClones.get(state) ?? libraryCloneOf(state)).call(state) as State;
        for (const key of CLONE_DROPS) (copy as Record<string, unknown>)[key] = state[key];
        copy.pos.set(state.pos.x, state.pos.y, state.pos.z);
        copy.stuckSpeedMultiplier = state.stuckSpeedMultiplier.clone();
        // PlayerState.clone shares these by reference: a fork's edit would reach the state it came from
        copy.attributes = structuredClone(state.attributes);
        copy.effects = structuredClone(state.effects);
        const controls = <Held extends object>(held: Held) => (held as Held & { clone?(): Held }).clone?.() ?? { ...held };
        copy.control = controls(state.control);
        copy.prevControl = controls(state.prevControl);
        if (state.keptBoundingBox) copy.keptBoundingBox = { ...state.keptBoundingBox, pos: state.keptBoundingBox.pos.clone() };
        const carry = this.carries.get(state);
        if (carry) {
            this.carries.set(copy, {
                ...carry, player: new this.binding.NativePlayer(this.version, carry.player.state(), {}),
                position: { ...carry.position }, slow: { ...carry.slow }, size: { ...carry.size }, keptSize: { ...carry.keptSize },
            });
        }
        this.adoptClone(copy);
        return copy;
    }

    /**
     * Who holds a state the engine has ticked and calls its clone() (mineflayer-physics-utils' EPhysicsCtx.clone and its
     * simulators, which this repository cannot change) gets cloneState. The state keeps the library's clone for cloneState
     * itself. A state never ticked reaches cloneState through its class's clone (adoptPrototype).
     */
    private adoptClone(state: PlayerState): void {
        if (this.libraryClones.has(state)) return;
        adoptPrototype(state);
        if (typeof (state as PlayerState & { clone?(): PlayerState }).clone !== 'function') return;
        const original = libraryCloneOf(state);
        this.libraryClones.set(state, original);
        Object.defineProperty(state, 'clone', { value: () => this.cloneState(state), configurable: true, writable: true, enumerable: false });
    }

    /** An opening shulker box's lid pushes the player after its tick, as the block entities tick after the entities. */
    pushByShulkerBox(ctx: PhysicsCtx, world: PrismarineWorld, open: OpenShulkerBox): void {
        const state = ctx.state as PlayerState;
        const carry = this.carries.get(state);
        if (!carry || !open.pushes || !samePosition(carry.position, state.pos)) return;
        this.takeOutput(state, carry, carry.player.pushByShulkerBox(this.syncOf(world).world, open.state, open.x, open.y, open.z));
    }

    /**
     * A moving piston block pushes the player after its tick (PistonMovingBlockEntity.tick: the client ticks them after
     * its entities), each in turn: the engine's push of simulateVersionedTick's movingPistons.
     */
    pushByMovingPiston(ctx: PhysicsCtx, world: PrismarineWorld, piston: MovingPiston): void {
        this.pushByMovingPistons(ctx, world, [piston]);
    }

    /**
     * All the moving blocks of a tick, in the order they tick. One that has ticked stands at its new progress for the ones
     * after it, which pushes given one at a time cannot show (movingPistonsOf has them so).
     */
    pushByMovingPistons(ctx: PhysicsCtx, world: PrismarineWorld, pistons: readonly MovingPiston[]): void {
        const state = ctx.state as PlayerState;
        const carry = this.carries.get(state);
        if (!carry || !samePosition(carry.position, state.pos)) return;
        this.takeOutput(state, carry, carry.player.pushByMovingPistons(this.syncOf(world).world, pistons.map(nativeMovingPiston)));
    }

    /**
     * The moving blocks of a tick pushing an entity the engine does not tick, where it stands after its own tick: a
     * passenger after its vehicle seated it, a cart the client shows. A moving block asks every entity of the level and moves
     * each by its own Entity.move, a passenger too (PistonMovingBlockEntity.moveCollidedEntities, 1.20.4
     * PistonMovingBlockEntity.java:85-150, 26.3 :110-170): docs/piston-authority.md.
     */
    pushEntityByMovingPistons(world: PrismarineWorld, entity: NativePistonPushedEntity, pistons: readonly MovingPiston[]): NativePistonPushedResult {
        const { box, margin } = bodyReach(entity.position, entity.velocity, entity.halfWidth, entity.height);
        const sync = this.syncOf(world);
        sync.sync(world, box, margin);
        return this.binding.simulateVersionedEntityPistonPush(this.version, sync.world, entity, pistons.map(nativeMovingPiston));
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

    /** Entity.isInRain of the tick under way. */
    private rainNow = false;

    /**
     * Entity.isInRain now, between ticks: what TridentItem.use asks of a riptide trident before the use starts (1.20.4
     * TridentItem.java:124, Entity.java:1156-1160, 26.3 TridentItem.java:121). The caller's inRain when it gives one.
     */
    isInRain(ctx: PhysicsCtx, world: PrismarineWorld): boolean {
        const state = ctx.state as PlayerState;
        return state.inRain ?? this.rainExposed(state, world, this.syncOf(world));
    }

    /** LivingEntity.getScale of the tick under way, as the crouch's fit tests read it; state.scale catches up at the end of aiStep. */
    private scaleNow = 1;

    /**
     * Entity.isInRain derived from the bot: Level.isRainingAt at the block position and at the box's top (26.3
     * Entity.java:1638-1641, 1.8.8 Entity.java:827-829). isRaining is the rain level above 0.2 (26.3 Level.java:716-718),
     * and canHaveWeather needs no test here: the nether and the end have no rain level, sky or precipitation.
     */
    private rainExposed(state: PlayerState, world: PrismarineWorld, sync: NativeWorldSync): boolean {
        const bot = state.bot;
        if (!bot || !((bot.rainState ?? 0) > 0.2)) return false;
        const seaLevel = state.seaLevel ?? 63;
        const top = (bot.game?.minY ?? 0) + (bot.game?.height ?? 256);
        const x = Math.floor(state.pos.x);
        const y = Math.floor(state.pos.y);
        const z = Math.floor(state.pos.z);
        const { height } = this.poseSize(state.pose, state.scale);
        return sync.rainingAt(world, x, y, z, top, seaLevel) || sync.rainingAt(world, x, Math.floor(state.pos.y + height), z, top, seaLevel);
    }

    private tick(state: PlayerState, world: PrismarineWorld): void {
        // Reject invalid external geometry before changing the caller's state or the native carry.
        validateBoatVehicle(state.boatVehicle);
        const sync = this.syncOf(world);
        const control = state.control;
        this.rainNow = state.inRain ?? this.rainExposed(state, world, sync);
        this.takeAttackHit(state);
        // The attribute is read live (LivingEntity.getScale: Player's fit tests), the dimensions keep the scale of their last
        // refresh: a synced change waits for the end of LivingEntity.aiStep (1.20.5 LivingEntity.java:2463-2468, 26.3
        // :2786, 1097-1109), so the tick's box, eyes and fluids are the old ones. A new pose refreshes them at once
        // (Entity.onSyncedDataUpdated, 26.3 Entity.java:3490).
        this.scaleNow = this.lt('1.20.5') ? 1 : f32(this.attributeValueOf(state.attributes, 'scale') ?? 1);
        if (state.sizedPose !== state.pose) {
            state.scale = this.scaleNow;
            // a pose the server synced since the last tick refreshes the dimensions as it arrives (1.15 Entity.java:2236, 1.16.5 :449-451)
            if (state.sizedPose !== undefined) this.refreshDimensions(state);
        }
        // Player.tick takes a passenger off the ground from 1.21.4, before LocalPlayer.aiStep reads it (1.21.3
        // Player.java:231, 1.21.4 :234-235, 26.3 Player.java:219-221)
        if (!this.lt('1.21.4') && this.isPassenger(state)) state.onGround = false;

        // the use key is handled before the entities tick: releasing a riptide trident launches the player
        let riptideLaunch: number | undefined;
        if (state.isUsingItem) {
            state.itemUseTicks += 1;
            state.usedItemRiptide = this.itemInUseRiptide(state);
        } else {
            const used = state.itemUseTicks;
            state.itemUseTicks = 0;
            // TridentItem.releaseUsing: isInWaterOrRain (1.13.2 class_3564.java:62, 1.20.4 TridentItem.java:66), and from
            // 26.1 not a passenger (26.3 TridentItem.java:64)
            const wet = state.isInWater || this.rainNow;
            const riding = !this.lt('26.1') && this.isPassenger(state);
            // (the item that was in use, which may be the off hand's, TridentItem.releaseUsing's stack; the fork's state.riptide
            // is always a number, and only the main hand's: the caller's word when no use was seen)
            const riptide = state.usedItemRiptide ?? state.riptide ?? 0;
            state.usedItemRiptide = undefined;
            if (!this.lt('1.13') && used >= 10 && riptide > 0 && wet && !riding) {
                riptideLaunch = riptide;
                state.autoSpinAttackTicks = 20;
            }
        }
        // the attached rockets the client ticks before the player, which boost it only while it glides
        let rocketsBefore = 0;
        // (a rocket outlives the glide: it boosts no one until its lifetime ends, and a new glide meets it, 1.11.2
        // FireworkRocketEntity.java:113-126, 26.3 :119-133, 172-178)
        if (state.fireworkRocketsBeforePlayer > 0 && state.fireworkRocketDuration > 0 && state.fallFlying) rocketsBefore = state.fireworkRocketsBeforePlayer;

        sync.sync(world, this.reachOf(state, riptideLaunch !== undefined), 0);
        // (the block entities tick after the entities: the move meets them as the last tick left them)
        this.syncMovingBlocks(sync, world, state.bot);
        // Entity.collide collides with the border the server set (ClientPacketListener's border packets)
        sync.setWorldBorder(state.bot?.worldBorder, state.bot?.game);

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
        if (state.fireworkRocketDuration > 0 && state.fallFlying) rocketsAfter = Math.max(0, (state.fireworkRockets > 0 ? state.fireworkRockets : 1) - state.fireworkRocketsBeforePlayer);

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
        // (EnchantmentHelper clamps a level to 255, 1.20.4 EnchantmentHelper.java:56: an item a server sends is no error)
        input[In.RIPTIDE_LAUNCH] = Math.min(riptideLaunch ?? 0, 255);
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
        // a state ticked on in another world goes there by setState, which the native player asks of it
        if (carry.world !== sync.world) {
            if (carry.world) carry.player.setState(carry.player.state());
            carry.world = sync.world;
        }
        const unmoved = samePosition(carry.position, pos);
        // no PlayerState field holds it: what the last tick left, or for a player put elsewhere what its flags say
        const below = unmoved ? carry.verticalCollisionBelow : state.isCollidedVertically && state.onGround;
        input[In.FLAGS] = flags | bit(unmoved, InFlag.UNMOVED) | bit(below, InFlag.VERTICAL_COLLISION_BELOW);
        const flyingDuringMove = state.flying;
        carry.size = size;
        carry.keptSize = size;
        // a 1.11 rocket that ticks before the player boosts along yHeadRot, the yaw of the last tick's aiStep, with this
        // tick's pitch (1.11.2 LivingEntity.java:1917-1925, Player.java:468-472, FireworkRocketEntity.java:120; D-222)
        const headYaw = state.headYawDegrees ?? carry.headYaw;
        const headLook = headYaw !== undefined && !this.lt('1.11') && this.lt('1.12') ? viewVector(this.mth, true, pitch, headYaw) : undefined;
        input[In.HEAD_LOOK] = headLook?.x ?? NaN;
        input[In.HEAD_LOOK + 1] = headLook?.y ?? NaN;
        input[In.HEAD_LOOK + 2] = headLook?.z ?? NaN;
        carry.headYaw = yaw;
        const out = carry.player.tickMove(sync.world, input, this.entitiesOf(state));
        this.takeOutput(state, carry, out);
        // isInWeb: the move was slowed by a cobweb, berry bush or powder snow (Entity.isInWeb before 1.14); the lift of a
        // riptide launch from the ground takes the slowdown, and to 1.21.1 its own checkInsideBlocks may set it again
        if (state.gameMode !== 'spectator') state.isInWeb = (out[Out.FLAGS] & OutFlag.MOVE_SLOWED) !== 0;

        // LivingEntity.aiStep after the move: the spin attack runs down, and ends on an entity or a wall (the engine's
        // checkAutoSpinAttack, which bounces the player off a living entity)
        if (state.autoSpinAttackTicks > 0) {
            state.autoSpinAttackTicks -= 1;
            if ((out[Out.FLAGS] & OutFlag.AUTO_SPIN_ATTACK_ENDED) !== 0) state.autoSpinAttackTicks = 0;
        }
        // Player.travel's flight clears the glide flag on 1.9 - 1.21.1; from 1.21.2 only the server does
        // (LivingEntity.updateFallFlying is server side, 1.21.2 Player.java:1399-1421, LivingEntity.java:2864-2869)
        if (flyingDuringMove && !this.lt('1.9') && this.lt('1.21.2')) state.fallFlying = false;
        state.scale = this.scaleNow;
        state.onClimbable = this.isInClimbable(state, sync);
        if (state.onGround && state.flying && state.gameMode !== 'spectator') state.flying = false;
        if (this.gt('1.13.2')) {
            const posed = state.pose;
            this.updatePoses(state, sync);
            // setPose is a synced value: its change runs refreshDimensions at once on this side too (1.15
            // Entity.java:2246, 1.16.5 :449-451, :2310-2333), which refits the box the entity keeps
            if (state.pose !== posed) this.refreshDimensions(state);
        } else {
            this.updateLegacySize(state, sync);
        }
        // Player.tick ends with the pose update, and what ticks after the player meets the box of the new pose (26.2
        // Player.java:266; before 1.14 updateSize, 1.13.2 Player.java:292, 338)
        carry.keptSize = this.sizeAfterTick(state, size);
        // the box the size update refitted is the entity's from here: Player.tick's updateSize sets the bounds, a new box
        // from minX to minX + width rather than the moved box's maxX (1.12.2 Player.java:324, 386-392, Entity.java:382-394
        // in legacy-src; 1.16.5 Entity.refreshDimensions). What ticks after the player meets it, and a moving piston's
        // push after the entities (pushByMovingPistons, even of no piston) leaves it there
        this.keepBox(carry, this.keptFor(state));
        const finished = carry.player.finishTick(sync.world, carry.keptSize.height);
        state.vel.set(finished[Out.VELOCITY], finished[Out.VELOCITY + 1], finished[Out.VELOCITY + 2]);
        state.pushedEntities = this.pushedEntities(state, carry);
        if (state.fireworkRocketDuration > 0) state.fireworkRocketDuration -= 1;

        state.sizedPose = state.pose;
        // to 1.15 the eyes are a query, not a field: after the tick they ask from the height it left (1.13.2, 1.15.2 Entity.isUnderLiquid;
        // 1.9 - 1.12 Entity.isInsideOfMaterial, the same bounds: 1.12.2 Entity.java isInsideOfMaterial, BlockLiquid.getLiquidHeightPercent)
        if (!this.lt('1.9') && this.lt('1.16') && !this.isPassenger(state)) {
            const low = this.lt('1.14') ? state.keptBoundingBox?.height === f32(0.6) : false;
            const eyeY = state.pos.y + (low ? f32(0.4) : f32(this.eyeHeight(state)));
            const y = Math.floor(eyeY);
            const block = sync.blockAt(Math.floor(state.pos.x), y, Math.floor(state.pos.z));
            const above = !this.lt('1.14') && isWaterish(sync.blockAt(Math.floor(state.pos.x), y + 1, Math.floor(state.pos.z)));
            state.eyeInWater = isWaterish(block) && eyeY < f32(y + f32((above ? 1 : f32((8 - this.renderedDepth(block)) / 9)) + f32(0.11111111)));
            if (this.lt('1.13')) state.isUnderWater = state.eyeInWater;
        }
        state.lastOnGround = state.onGround;
        state.prevHeading.forward = heading.forward;
        state.prevHeading.strafe = heading.strafe;
        // wasJumping: the input's jump, which this tick's auto-jump pressed (1.21.11 LocalPlayer.java:711, 722)
        state.prevControl.jump = control.jump || state.autoJumping;
        state.prevControl.sneak = control.sneak;
    }

    /**
     * The engine's copy of the player, brought up to date with what the adapter does not send every tick: the whole
     * state when the attributes, effects or enchantments changed, the kept box when it is not the engine's.
     */
    private carryFor(state: PlayerState, kept: Box | null): Carry {
        const slow = this.slowInputs(state);
        const frozen = this.ticksFrozenOf(state);
        const carry = this.carries.get(state);
        if (!carry) {
            const created: NativeSimulationState = {
                ...slow,
                ticksFrozen: frozen,
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
            const fresh = { verticalCollisionBelow: state.isCollidedVertically && state.onGround, player, position: vec(state.pos), slow, frozen, kept: kept && box(kept.minX, kept.minY, kept.minZ, kept.maxX, kept.maxY, kept.maxZ), size: { width: 0.6, height: 1.8 }, keptSize: { width: 0.6, height: 1.8 } };
            this.carries.set(state, fresh);
            return fresh;
        }
        const refrozen = frozen !== carry.frozen;
        if (refrozen || !sameInputs(slow, carry.slow)) {
            const dropped = Object.fromEntries(Object.keys(carry.slow).map(key => [key, undefined]));
            carry.player.setState({ ...carry.player.state(), ...dropped, ...slow, ...(refrozen ? { ticksFrozen: frozen } : {}) });
            carry.slow = slow;
            carry.frozen = frozen;
        }
        this.keepBox(carry, kept);
        return carry;
    }

    /** The engine's kept box made the state's, when it differs (a copy: the state's box is an object its caller may change). */
    private keepBox(carry: Carry, kept: Box | null): void {
        if (sameBox(kept, carry.kept)) return;
        carry.player.setBoundingBox(kept ? nativeBox(kept) : null);
        carry.kept = kept && box(kept.minX, kept.minY, kept.minZ, kept.maxX, kept.maxY, kept.maxZ);
    }

    /** The engine's inputs that rarely change: the synced attributes, the effects, the enchantments and the abilities. */
    private slowInputs(state: PlayerState): Partial<NativeSimulationState> {
        return {
            ...this.attributesAndEffects(state),
            // getDepthStrider: the highest level on any armor piece (1.8.8 EnchantmentHelper.java:88-103, 182-183; 1.9.4
            // Enchantment.java:118, 129; 1.20.5 Enchantments.java:72-75); from 1.21 the WATER_MOVEMENT_EFFICIENCY attribute
            depthStrider: this.lt('1.21') ? this.wornEnchantmentLevel(state, 'depth_strider', ARMOR_SLOTS) ?? state.depthStrider : 0,
            // the boots alone (1.16.5 Enchantments.java:29, 1.20.5 :72-73); from 1.21 the MOVEMENT_EFFICIENCY attribute
            soulSpeed: this.lt('1.21') ? this.wornEnchantmentLevel(state, 'soul_speed', ['feet']) ?? state.soulSpeed : 0,
            powderSnowWalkable: this.canWalkOnPowderSnow(state),
            // MagmaBlock.stepOn and CampfireBlock.entityInside spare Frost Walker's wearer before 1.21 (1.20.5
            // MagmaBlock.java:29, CampfireBlock.java:102), the enchantment's damage immunity from 1.21 (1.21.11
            // Enchantments.java:139-139)
            frostWalker: (this.wornEnchantmentLevel(state, 'frost_walker', ['feet']) ?? 0) > 0,
            // ProtectionEnchantment.transformFireDuration takes the highest Fire Protection level worn off the burning before
            // 1.21 (1.17 ProtectionEnchantment.java:59-66, EnchantmentHelper.getEnchantmentLevel); from 1.21 the
            // enchantment is in the BURNING_TIME attribute
            fireProtection: this.lt('1.21') ? this.wornEnchantmentLevel(state, 'fire_protection', ARMOR_SLOTS) ?? 0 : 0,
            // Player.causeFallDamage deals nothing where the abilities allow flight (1.21.5 Player.java:1470-1472), and
            // calculateFallDamage nothing for a type in #fall_damage_immune (1.20.5 LivingEntity.java:1594-1596, 1.21.5
            // :1644-1646): the state's fallDamageImmune
            fallDamageImmune: state.mayFly || !!state.fallDamageImmune,
            // the bot's own player, as its client ticks it (the client's side of the fire and freeze ticks)
            clientSide: true,
            invulnerable: !!state.invulnerable,
            inRain: this.rainNow,
            passenger: this.isPassenger(state),
            isVehicle: state.isVehicle ?? (state.bot?.entity?.passengers?.length ?? 0) > 0,
            // Snapshot nested coordinates: callers may mutate the same context object between ticks.
            boatVehicle: copyBoatVehicle(state.boatVehicle),
            // the client's own player moves itself (Entity.isLogicalSideForUpdatingMovement through
            // LocalPlayer.isEffectiveAi, 1.21.3 Entity.java:3307-3312, LocalPlayer.java:471); a
            // state may say otherwise
            controlledByLocalInstance: state.controlledByLocalInstance ?? true,
            // Keep omission distinct from false for the native piston bounce/restitution authority fallback.
            canSimulateMovement: state.canSimulateMovement,
            // Entity.updateFluidHeightAndDoFluidPushing pushes by lava at 0.007 in an ultrawarm dimension (FAST_LAVA from
            // 1.21.11), 0.0023333333333333335 elsewhere (1.16 Entity.java:1046, 1.21.11 :1561); of the vanilla dimension
            // types only the nether's is (1.16 DimensionType.java:80-95, 1.21.11 DimensionTypes.java:84)
            ultraWarm: state.bot?.game?.dimension === 'the_nether',
            noGravity: this.noGravityOf(state, state.bot?.entity),
            // the burning the server says, which Entity.isOnFire reads on a client (1.8.8 Entity.java:1450-1453, 26.3 :2779-2782)
            onFireFlag: this.onFireFlagOf(state.bot?.entity),
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
        carry.verticalCollisionBelow = flag(OutFlag.VERTICAL_COLLISION_BELOW);
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
        // the amounts the engine hands hurt, not changes of the health: the adapter runs the client's side, where a block's
        // hurt, a flight into a wall and the freeze ticks are the server's, so they stay 0 unless a state of the server's
        // side is given (26.3 Entity.java:1969-1975, LivingEntity.java:2505-2510, 3082)
        state.contactDamage = out[Out.CONTACT_DAMAGE];
        state.wallDamage = out[Out.WALL_DAMAGE];
        state.freezeDamage = out[Out.FREEZE_DAMAGE];
        this.keepFrost(state, carry, out[Out.FROST]);
        state.blockFallCallback = flag(OutFlag.BLOCK_FALL_CALLBACK);
        // from 26.1 a move into water refreshes the eyes as well (LivingEntity.checkFallDamage's updateFluidInteraction, 26.1
        // EntityFluidInteraction.update): the next tick's isUnderWater is those; a seat's fluids are the vehicle's (updateFluids)
        // (before 1.16 no flag is kept, and each read asks the world: the state says what the eyes meet where the move left them)
        if ((!this.lt('26.1') || this.lt('1.16')) && !this.isPassenger(state)) state.eyeInWater = flag(OutFlag.EYE_IN_WATER);
        if (this.lt('1.13') && !this.isPassenger(state)) state.isUnderWater = !!state.eyeInWater;
        state.jumpTicks = out[Out.JUMP_TICKS];
        state.autoJumpTime = out[Out.AUTO_JUMP_TIME];
        state.stuckSpeedMultiplier.set(out[Out.STUCK_SPEED_MULTIPLIER], out[Out.STUCK_SPEED_MULTIPLIER + 1], out[Out.STUCK_SPEED_MULTIPLIER + 2]);
        carry.position = vec(state.pos);
        const at = Out.BOUNDING_BOX;
        carry.kept = Number.isNaN(out[at]) ? null : box(out[at], out[at + 1], out[at + 2], out[at + 3], out[at + 4], out[at + 5]);
        state.keptBoundingBox = carry.kept && this.lt('1.17')
            ? { bb: this.stateBox(state, carry.kept), pos: state.pos.clone(), ...carry.keptSize }
            : null;
    }

    /** The enchantments of an item as enchantmentLevelByName reads them, through the item format of its release. */
    private itemEnchantments(item: { enchants?: ItemEnchants } | null | undefined): { id: string | number; lvl: number }[] {
        const enchants = item?.enchants;
        return Array.isArray(enchants)
            ? enchants.map(({ name: id, lvl }) => ({ id, lvl }))
            : (enchants?.enchantments ?? []).map(({ id, level }) => ({ id, lvl: level }));
    }

    /**
     * EnchantmentHelper.getEquipmentLevel on the bot's own armor: the highest level on the slots the enchantment counts
     * (1.20.5 EnchantmentHelper.java:177-194), undefined when the state has no bot inventory to read. The fields
     * mineflayer-physics-utils' PlayerState keeps are not that: it reads the helmet for the boots (equipment[5]) and only
     * the nbt, which items have no more from 1.20.5.
     */
    private wornEnchantmentLevel(state: PlayerState, name: string, slots: readonly ArmorSlot[]): number | undefined {
        const bot = state.bot;
        if (!bot?.inventory || !bot.getEquipmentDestSlot) return undefined;
        let highest = 0;
        for (const slot of slots) {
            const enchantments = this.itemEnchantments(bot.inventory.slots[bot.getEquipmentDestSlot(slot)]);
            highest = Math.max(highest, this.enchantmentLevelByName(name, enchantments));
        }
        return highest;
    }

    /** The Swift Sneak on the leggings (1.19 Enchantments.java:30, 1.20.5 :77-77), which LocalPlayer.aiStep reads to 1.20.6. */
    private swiftSneakLevel(state: PlayerState): number {
        return this.wornEnchantmentLevel(state, 'swift_sneak', ['legs']) ?? state.swiftSneak;
    }

    /**
     * PowderSnowBlock.canEntityWalkOnPowderSnow for a player: leather boots on its feet, the item alone (1.17
     * PowderSnowBlock.java:109-115, 26.3 PowderSnowBlock.java). The state's own value first: PlayerState has none.
     */
    private canWalkOnPowderSnow(state: PlayerState): boolean {
        if (state.canWalkOnPowderSnow !== undefined) return state.canWalkOnPowderSnow;
        const bot = state.bot;
        if (!bot?.inventory || !bot.getEquipmentDestSlot) return false;
        return bot.inventory.slots[bot.getEquipmentDestSlot('feet')]?.name === 'leather_boots';
    }

    /**
     * LivingEntity.getUsedItemHand from the bot's tracked flags, bit 2 (1.14.4 LivingEntity.java:2427): the dependency reads
     * slot 6 or 8 for `> 2`, health where the field is at 7 (1.14-1.16), and counts a spin attack's bit 4 as the off hand.
     */
    private usingOffHand(state: PlayerState): boolean {
        const slot = livingFlagsSlot(this.version);
        const flags = slot === null ? undefined : state.bot?.entity?.metadata?.[slot];
        return typeof flags === 'number' ? (flags & 2) !== 0 : !!state.isUsingOffHand;
    }

    /** The Riptide of the item in use: the off hand's when it is that hand, else the main hand's. Undefined with no such item to read. */
    private itemInUseRiptide(state: PlayerState): number | undefined {
        const bot = state.bot;
        if (!bot) return undefined;
        const offHand = this.usingOffHand(state) && bot.getEquipmentDestSlot ? bot.inventory?.slots[bot.getEquipmentDestSlot('off-hand')] : undefined;
        const item = offHand ?? bot.heldItem;
        return item ? this.enchantmentLevelByName('riptide', this.itemEnchantments(item)) : undefined;
    }

    /** The engine's inputs from the synced attributes and the effects. */
    private attributesAndEffects(state: PlayerState): Partial<NativeSimulationState> {
        const attributes = state.attributes;
        const levitation = this.effectAmplifier(state, state.levitation, 'Levitation');
        const jumpBoost = this.effectAmplifier(state, state.jumpBoost, 'JumpBoost');
        // an attribute the server synced holds the effects' modifiers; without one (or with only setSprinting's) the
        // engine's fallback applies them (1.21.11 LivingEntity.java:2224-2225, 26.3 AttributeInstance.java:151-169)
        const movementSpeed = this.attribute(attributes, 'movementSpeed');
        const syncedSpeed = movementSpeed !== undefined && !movementSpeed.unsynced;
        const out: Partial<NativeSimulationState> = {
            movementSpeed: f32(0.1),
            // the state's own boundary values first (a caller's runtime state), else the synced attributes
            // (Jump Boost's ADD_VALUE modifier of it, 1 a level from 1.20.5, a synced attribute already holds: 1.20.5
            // MobEffects.java:77-77, 26.3 MobEffects.java:76, MobEffect.java:212)
            safeFallDistance: state.safeFallDistance ?? this.attributeValueOf(attributes, 'safeFallDistance')
                ?? 3 + (jumpBoost !== undefined && !this.lt('1.20.5') ? jumpBoost + 1 : 0),
            slowFalling: this.effectAmplifier(state, state.slowFalling, 'SlowFalling') !== undefined,
            levitation: levitation !== undefined,
            // WebBlock.entityInside's (0.5, 0.25, 0.5) under Weaving (1.20.5+, 1.21.11 WebBlock.java:25-30)
            weaving: this.effectLevelByName('Weaving', state.effects) > 0,
            // LivingEntity.hurt refuses fire damage under it (1.8.8 LivingEntity.java:683, 26.3 :1154)
            fireResistance: this.effectLevelByName('FireResistance', state.effects) > 0,
        };
        const sneakingSpeed = this.lt('1.21')
            ? (this.swiftSneakLevel(state) > 0 && !this.lt('1.19') ? this.sneakingSpeedFactor(state) : undefined)
            : this.attributeValueOf(attributes, 'playerSneakingSpeed') ?? this.attributeValueOf(attributes, 'sneakingSpeed');
        const optional: Partial<NativeSimulationState> = {
            movementSpeedAttribute: movementSpeed && syncedSpeed && this.clientHoldsFrost() ? this.speedAttributeParts(movementSpeed) : undefined,
            // LivingEntity.getSpeed and getJumpPower read the attributes with every modifier they hold, the speed and
            // slowness effects' and the sprint's among them (1.21.11 LivingEntity.java:2224-2225, 26.3
            // AttributeInstance.java:151-169)
            movementSpeedValue: syncedSpeed ? this.attributeValueOf(attributes, 'movementSpeed') : undefined,
            speedEffectAmplifier: syncedSpeed ? undefined : this.effectAmplifier(state, state.speed, 'Speed'),
            slownessEffectAmplifier: syncedSpeed ? undefined : this.effectAmplifier(state, state.slowness, 'Slowness'),
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

    /** LivingEntity.aiStep makes the powder snow modifier on the client too on 1.17 - 1.21.4 (1.17 LivingEntity.java:2555-2556, 1.21.4 :2785-2786), on the server alone from 1.21.5 (1.21.5 :2763-2772). */
    private clientHoldsFrost(): boolean {
        return !this.lt('1.17') && this.lt('1.21.5');
    }

    /** The modifier's id, a UUID before 1.21 (1.17 LivingEntity.java:120, 1.21 :139). */
    private get frostModifierId(): string {
        return this.lt('1.21') ? '1eaf83ff-7207-4596-b37a-d7a07b3ec4ce' : 'minecraft:powder_snow';
    }

    private isFrostModifier(modifier: AttributeModifier): boolean {
        return String(modifier.uuid ?? modifier.id).toLowerCase() === this.frostModifierId;
    }

    /** The movement speed's base, its modifiers but the powder snow one, each operation in the order its container iterates, and that one's amount. */
    private speedAttributeParts(attribute: Attribute): NativeMovementSpeedAttribute {
        const held = attribute.modifiers ?? [];
        const others = held.filter(modifier => !this.isFrostModifier(modifier));
        return {
            base: attribute.value,
            modifiers: [0, 1, 2].flatMap(operation => this.iterationOrder(others.filter(modifier => modifier.operation === operation))
                .map(({ amount }) => ({ amount, operation: SPEED_OPERATIONS[operation] }))),
            frost: held.find(modifier => this.isFrostModifier(modifier))?.amount,
        };
    }

    /**
     * What the tick's frost update left on the client's attribute (removePowderSnowSlow, addPowderSnowSlowIfNeeded): a
     * copy of the synced one, as with setSprinting, until the server's next attribute packet replaces it. The engine's
     * value moved with it: the next tick sends the attribute whatever it holds, a packet taking the modifier off included.
     */
    private keepFrost(state: PlayerState, carry: Carry, frost: number): void {
        const key = this.attributeKey('movementSpeed');
        const held = key === null ? undefined : state.attributes[key];
        if (key === null || !held || held.unsynced || !this.clientHoldsFrost()) return;
        const modifiers = held.modifiers ?? [];
        const amount = Number.isNaN(frost) ? undefined : frost;
        if (modifiers.find(modifier => this.isFrostModifier(modifier))?.amount === amount) return;
        const others = modifiers.filter(modifier => !this.isFrostModifier(modifier));
        if (amount !== undefined) others.push({ uuid: this.frostModifierId, amount, operation: 0 });
        state.attributes = { ...state.attributes, [key]: { ...held, modifiers: others } };
        carry.slow = { ...carry.slow, movementSpeedValue: NaN };
    }

    /**
     * Entity.getTicksFrozen, the synced data at 7 from 1.17 (1.17 Entity.java:201, 241): the count the server keeps. A
     * negative one (a command's) counts as none wherever it is read (1.17 LivingEntity.java:510-511, Entity.java:2055).
     */
    private ticksFrozenOf(state: PlayerState): number | undefined {
        if (this.lt('1.17')) return undefined;
        const ticks = state.bot?.entity?.metadata?.[7];
        return typeof ticks === 'number' ? Math.max(0, ticks) : undefined;
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
                (this.teamPushes(state, entity) ? 0 : NativePlayerEntityFlag.NO_TEAM_PUSH)
                    | (entity.name === 'player' && this.isSpectatorPlayer(state, entity) ? NativePlayerEntityFlag.SPECTATOR : 0),
            ], i * NATIVE_PLAYER_ENTITY_LENGTH);
        });
        return out;
    }

    /**
     * EntitySelector.pushableBy(entity).test(player) for its team rules: the teams of the bot's scoreboard (bot.teamMap)
     * by scoreboard name, a player's name, another entity's UUID (Entity.getScoreboardName). No push when the pusher's
     * rule or the player's is never, when one is pushOwnTeam between allies, or one is pushOtherTeams between others
     * (1.9.4 EntitySelector.java:54-82, 1.16.5 EntitySelector.java:27-54, 1.21.11 EntitySelector.java:40-72, 26.3
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
     * owner the client holds is its owner's team, before its own to 1.21.4 (1.9.4 TamableAnimal.java:182-191, 1.21.4
     * :193-202), after it from 1.21.5, the root owner's (1.21.5 TamableAnimal.java:190-203, 1.21.11 :174-185,
     * OwnableEntity.java:14-36).
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
     * before minecraft-data names them) among what the client holds: a player to 1.21.4 (TamableAnimal.getOwner:
     * world.getPlayerByUuid, 1.9.4 TamableAnimal.java:160-167), any living entity from 1.21.5 (EntityReference.get).
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
        // a dying mob (no health left) is no collider: Shulker and HappyGhast.canBeCollidedWith need isAlive (1.9.4
        // ShulkerEntity.java:420-421, 1.16.5 :395, 26.3 Shulker.java:439-441, HappyGhast.java:522-523). It pushes until
        // 1.11, where LivingEntity.isPushable starts to want isAlive too (1.10.2 LivingEntity.java:1891-1892, 1.11.2
        // :1952-1953), and still meets a riptide spin as any LivingEntity
        if (entity.alive === false && entity.living) return this.lt('1.11') ? 'living' : 'still_living';
        if (entity.name === 'shulker') return 'shulker';
        if (/(^|_)minecart$/.test(entity.name ?? '')) return 'minecart';
        // an adult on its still timeout collides from every side (HappyGhast.canBeCollidedWith: isOnStillTimeout, the
        // synced STAYS_STILL; 1.21.6 HappyGhast.java:504-513, 1.21.11 :511-520, :487-488)
        if (entity.name === 'happy_ghast') return entity.baby ? 'living' : entity.staysStill ? 'happy_ghast_stays_still' : 'happy_ghast';
        // the armor stand pushes no one and is no collider, but is a LivingEntity: it bounces a riptide spin
        // (LivingEntity.checkAutoSpinAttack, 1.13.2 LivingEntity.java:2022; ArmorStand.java:40)
        if (entity.name === 'armor_stand') return 'still_living';
        if (NEVER_PUSHING_LIVING.has(entity.name)) return 'still_living';
        // a spectator (Player.tick: noPhysics = isSpectator(), which Entity.push(Entity) skips; 1.9.4
        // Player.java:249, 1.21.11 Player.java:224) neither pushes nor meets the spin (getEntities: NO_SPECTATORS,
        // 1.13.2 class_3593.java:18-19, 1.21.11 EntityGetter.java:29-31)
        if (entity.name === 'player' && this.isSpectatorPlayer(state, entity)) return 'inert';
        return entity.living ? 'living' : 'inert';
    }

    /**
     * The box of an armor stand by its synced client flags (1.13+, where only the riptide spin meets it): a marker's
     * (16) is 0 x 0 and a small one's (1) half the type's 0.5F x 1.975F, around its position (1.13.2
     * ArmorStand.java:121-127, 624-630, 669, 694; 1.14.4 :750-751; 1.21.11 ArmorStand.java:59-70). The flags are
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

    /** Explicit context only: ordinary boat IN_WATER cannot stand in for the underwater status. */
    private boatForFluids(state: PlayerState): NativeBoatVehicle | undefined {
        validateBoatVehicle(state.boatVehicle);
        return this.isPassenger(state) ? state.boatVehicle ?? undefined : undefined;
    }

    /** Entity's boat exclusion in the eye query before the shared 26.1 interaction scan. */
    private boatBlocksEyes(state: PlayerState, boat: NativeBoatVehicle | undefined, eyeY: number): boolean {
        if (!boat || this.lt('1.9') || !this.lt('26.1')) return false;
        if (this.lt('1.16')) return true;
        if (boat.underwater) return false;
        const { min, max } = boat.boundingBox;
        // 1.16/1.16.1 use AABB.contains(eye sample); 1.16.2 switches to inclusive Y bounds alone.
        if (this.lt('1.16.2')) return state.pos.x >= min.x && state.pos.x < max.x
            && eyeY >= min.y && eyeY < max.y && state.pos.z >= min.z && state.pos.z < max.z;
        return eyeY >= min.y && eyeY <= max.y;
    }

    /**
     * Whether the player is in water (Entity.updateInWaterStateAndDoFluidPushing) and its eyes are (updateFluidOnEyes,
     * isUnderWater), where the tick starts; the engine pushes it and keeps its own.
     */
    private updateFluids(state: PlayerState, sync: NativeWorldSync): void {
        if (this.lt('1.13')) {
            this.updateLegacyFluids(state, sync);
            return;
        }
        // (1.13 has no pose: the box is the one updateSize left, 0.6 high in a swim, so a swim out of knee-deep air ends at once)
        const kept = this.lt('1.14') ? state.keptBoundingBox : null;
        const collider = kept && samePosition(kept.pos, state.pos) ? kept.bb : this.collider(state.pose, state.pos, state.scale);
        const deflated = shrink(collider, 1e-3);
        const boat = this.boatForFluids(state);
        const interaction = !this.lt('26.1');
        if (interaction && boat && !boat.underwater) {
            // Entity deflates first; AbstractBoat then returns null or raises minY to its top.
            // Collision dimensions and the original depth origin remain the passenger's own.
            if (boat.boundingBox.max.y >= deflated.maxY) {
                state.isInWater = false;
                state.isInLava = false;
                state.eyeInWater = false;
                state.isUnderWater = false;
                return;
            }
            deflated.minY = Math.max(deflated.minY, boat.boundingBox.max.y);
        }
        // EntityFluidInteraction.hasFluidAndLoaded: from 26.1 no fluid is touched while a chunk under the cells widened by
        // one is not loaded (26.1 EntityFluidInteraction.java:44, 94-110)
        let loaded = true;
        if (interaction) {
            for (let cx = (Math.floor(deflated.minX) - 1) >> 4; cx <= Math.ceil(deflated.maxX) >> 4; cx += 1) {
                for (let cz = (Math.floor(deflated.minZ) - 1) >> 4; cz <= Math.ceil(deflated.maxZ) >> 4; cz += 1) {
                    if (!sync.chunkLoaded(cx, cz)) loaded = false;
                }
            }
        }
        const inFluid = (water: boolean): boolean => {
            if (!loaded) return false;
            const matches = (block: DescribedBlock | null): block is DescribedBlock => water
                ? isWaterish(block) : block?.fluid?.type === FLUID_LAVA;
            // up to ceil(max), exclusive, on every release: a box whose deflated top is a whole number leaves the cell over it
            // out (1.13.2 Entity.java:2512-2517, 1.20.5 :3084-3090, 1.21.11 :3599-3605)
            for (let x = Math.floor(deflated.minX); x < Math.ceil(deflated.maxX); x += 1) {
                for (let y = Math.floor(deflated.minY); y < Math.ceil(deflated.maxY); y += 1) {
                    for (let z = Math.floor(deflated.minZ); z < Math.ceil(deflated.maxZ); z += 1) {
                        const block = sync.blockAt(x, y, z);
                        if (!matches(block)) continue;
                        const height = matches(sync.blockAt(x, y + 1, z)) && !this.lt('1.14') ? 1 : this.ownFluidHeight(block);
                        const top = interaction ? y + height : f32(y + height);
                        if (top >= deflated.minY && (!interaction || top > collider.minY)) return true;
                    }
                }
            }
            return false;
        };
        // Before 1.19.3 even an underwater boat excludes water. Lava is independent until 26.1.
        const excludesWater = !!boat && !interaction && (this.lt('1.19.3') || !boat.underwater);
        state.isInWater = !excludesWater && inFluid(true);
        // Modern fluid interaction updates both before LocalPlayer makes its decisions (including 26.3 glide eligibility).
        if (interaction) state.isInLava = inFluid(false);
        // Entity.updateFluidOnEyes: 1.13 - 1.15 compare the eyes with y + (the water height + 0.11111111F) in float and
        // take it at once (1.13.2 and 1.15.2 Entity.isUnderLiquid bytecode: fadd, fadd); 1.16 - 1.20.4 look
        // 0.1111111119389534 below the eyes, 1.20.5+ at them (1.20.4 Entity.java:141, 1227, 1.20.5 :1276), and from 1.16
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
        if (interaction) {
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
        } else if (!this.boatBlocksEyes(state, boat, eyeY)) {
            const y = Math.floor(eyeY);
            const block = sync.blockAt(eyeX, y, eyeZ);
            if (isWaterish(block)) {
                const height = heightAt(block, y);
                if (this.lt('1.16')) eyeInWater = eyeY < f32(y + f32(height + f32(0.11111111)));
                else eyeInWater = f32(y + height) > eyeY;
            }
        }
        state.eyeInWater = eyeInWater;
        // LocalPlayer.isUnderWater from 1.16 is the field Player.tick set before this base tick (1.16
        // LocalPlayer.java:1023, 1.16.5 Player.java:316, 26.3 LocalPlayer.java:1171, Player.java:249-250):
        // the eyes of the tick before, whatever the water does now, so a fast rise out of it is still under water
        state.isUnderWater = this.lt('1.16') ? eyeInWater && state.isInWater : wasEyeInWater;
    }

    /**
     * Entity.handleWaterMovement before 1.13: a water block in the box shrunk by 0.4F vertically and 0.001 all round,
     * when ceil(maxY) reaches its float top; the eyes are in water (isInsideOfMaterial) below its top plus 0.11111111F.
     */
    private updateLegacyFluids(state: PlayerState, sync: NativeWorldSync): void {
        if (!this.lt('1.9') && this.boatForFluids(state)) {
            state.isInWater = false;
            state.eyeInWater = false;
            state.isUnderWater = false;
            return;
        }
        const isWater = (block: DescribedBlock | null): block is DescribedBlock => block?.fluid?.type === 1;
        const heightPercent = (block: DescribedBlock) => f32(((block.fluid!.level >= 8 ? 0 : block.fluid!.level) + 1) / 9);
        // the entity's own box, as updateSize left it (1.65 high in a sneak, 0.6 in a glide from 1.9), not a pose's: there
        // is none (1.8.9 Entity.java handleWaterMovement, 1.12.2 Entity.java:1051 getEntityBoundingBox)
        const kept = state.keptBoundingBox;
        const collider = kept && samePosition(kept.pos, state.pos) ? kept.bb : this.collider(state.pose, state.pos, 1);
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
        // (getEyeHeight is a float, widened: 1.12.2 Entity.java:1114, Player.java:1753-1754)
        const eyeY = state.pos.y + f32(this.eyeHeight(state));
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
     * block (1.17 Entity.java:1046; 1.17.1 Entity.updateSwimming bytecode 62-79; 1.18 :1032-1034), and sprinting in
     * water keeps it. A passenger does neither (1.13.2 Entity.java:1060-1062, 1.20.4 :1187-1189, 26.3 :1679-1681).
     */
    private updateSwimming(state: PlayerState, sync: NativeWorldSync): void {
        if (this.lt('1.13') || state.flying || this.isPassenger(state)) {
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
            // 1.14's KeyboardInput.tick(isVisuallySneaking || crawling, isSpectator()) slows by the sneak key, but never a
            // spectator's (1.14.4 KeyboardInput.java:22, LocalPlayer.java:593); aiStep's flight then divides the sneak's
            // impulse by 0.3D all the same (:682-688), so a sneaking spectator goes faster, not as fast (D-192)
            if (this.lt('1.15') && state.gameMode === 'spectator') return false;
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
            // !abilities.flying: a flying (or spectating) player does not crouch (1.14.4 LocalPlayer.java:556-561,
            // 1.16.4 LocalPlayer.java:603, 1.21.11 :715, 26.3 :764)
            const flying = state.flying || state.gameMode === 'spectator';
            // !isPassenger from 1.20.2 (origin/client1.20.1 LocalPlayer.java:666-669, client1.20.2 :664-668), and a sleeper
            // is not forced down from 1.15 (origin/client1.14.4 LocalPlayer.java:557-558, 1.15
            // LocalPlayer.java:593-594)
            const riding = !this.lt('1.20.2') && this.isPassenger(state);
            const sleeping = !this.lt('1.15') && this.isSleeping(state);
            state.crouching = !flying && !this.swimmingAndNotFlying(state) && !riding && this.poseFits(sync, Pose.SNEAKING, state, this.scaleNow)
                && (state.prevControl.sneak || (!sleeping && !this.poseFits(sync, Pose.STANDING, state, this.scaleNow)));
        } else {
            state.crouching = !this.swimmingAndNotFlying(state) && state.prevControl.sneak;
        }
        const movingSlowly = this.isMovingSlowly(state);
        // 1.21.4 only: LocalPlayer.shouldStopSprinting right after the input's tick (isFallFlying() || hasBlindness() ||
        // isMovingSlowly() || ..., 1.21.4 LocalPlayer.java:795-798; the vehicle's term is shouldStopRunSprinting's
        // here, and the item's spares a passenger, :798)
        const itemSlows = state.isUsingItem && !this.isPassenger(state);
        if (this.gt('1.21.3') && this.lt('1.21.5')) {
            const blind = this.effectAmplifier(state, state.blindness, 'Blindness') !== undefined;
            if (state.fallFlying || blind || movingSlowly || (itemSlows && !state.isUnderWater)) this.setSprinting(state, false);
        }
        // (1.21.5 Vec2.java:49 normalized, 1.0E-4F)
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
        // the item spares a passenger in every release: isUsingItem() && !hasVehicle() (1.8.8 LocalPlayer.java:538,
        // 1.13.2 :698, 1.14.4 :596, 1.20.4 :672, 1.21.4 :642)
        const usingItemFirst = this.gt('1.21.3');
        if (itemSlows && usingItemFirst) slow(f32(0.2));
        if (movingSlowly) slow(this.sneakingSpeedFactor(state));
        if (itemSlows && !usingItemFirst) slow(f32(0.2));
        return movingSlowly;
    }

    /** The sneak slowdown: 0.3 before 1.19, swift sneak's float clamp to 1.20.6, the sneaking speed attribute since. */
    private sneakingSpeedFactor(state: PlayerState): number {
        if (this.lt('1.19')) return 0.3;
        if (this.lt('1.21')) return Math.min(Math.max(f32(f32(0.3) + f32(this.swiftSneakLevel(state) * f32(0.15))), 0), 1);
        return f32(this.attributeValueOf(state.attributes, 'playerSneakingSpeed') ?? this.attributeValueOf(state.attributes, 'sneakingSpeed') ?? 0.3);
    }

    private slowDueToUsingItem(state: PlayerState): boolean {
        return state.isUsingItem && !state.useItemCanSprint;
    }

    /**
     * LocalPlayer.aiStep's sprint: the sprint key starts one, and so does a double tap of forward (a fresh press opens the
     * sprint window); a sprint stops when the player no longer moves forward enough, hits a wall, hungers, ...
     */
    private inputsToSprint(state: PlayerState, heading: { forward: number; strafe: number }): void {
        // 1.8: a sprint lasts 600 ticks at most (EntityPlayerSP.onLivingUpdate, 1.8.8 LocalPlayer.java:485-490)
        const sprintingTicksLeft = state.sprintingTicksLeft ?? 0;
        if (this.lt('1.9') && sprintingTicksLeft > 0) {
            state.sprintingTicksLeft = sprintingTicksLeft - 1;
            if (sprintingTicksLeft === 1) this.setSprinting(state, false);
        }
        if (state.sprintTriggerTime > 0) state.sprintTriggerTime -= 1;
        const prevSneakKey = state.prevControl.sneak;
        // the item spares a passenger (the slowdown's own test before 1.21.5, 1.20.4 LocalPlayer.java:672-675; 1.21.5
        // LocalPlayer.java:695, 26.3 :781)
        const itemHoldsSprint = this.slowDueToUsingItem(state) && !this.isPassenger(state);
        if (itemHoldsSprint || (this.gt('1.15.2') && prevSneakKey) || (this.gt('1.21.4') && state.control.back)) {
            state.sprintTriggerTime = 0;
        }
        const canStart = this.canStartSprinting(state, heading);
        const prevImpulse = state.prevForwardImpulse;
        state.prevForwardImpulse = heading.forward;
        if (this.gt('1.21.4')) {
            if (canStart) {
                if (!(prevImpulse > f32(1e-5))) {
                    if (state.sprintTriggerTime > 0) this.setSprinting(state, true);
                    else state.sprintTriggerTime = this.sprintWindow(state);
                }
                if (state.control.sprint) this.setSprinting(state, true);
            }
        } else {
            const prevEnough = this.gt('1.13.2') && state.isUnderWater ? prevImpulse > f32(1e-5) : prevImpulse >= 0.8;
            const onGroundOrUnderWater = this.sprintGround(state) || (this.gt('1.12.2') && state.isUnderWater);
            if (canStart && onGroundOrUnderWater && !prevSneakKey && !prevEnough) {
                if (state.sprintTriggerTime <= 0 && !state.control.sprint) state.sprintTriggerTime = 7;
                else this.setSprinting(state, true);
            }
            // the sprint key alone is held back in shallow water (1.13.2 LocalPlayer.java:732-734, 1.21.4
            // LocalPlayer.java:686-688)
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
     * LivingEntity.knockback on the player's velocity, for a caller that decides hits itself: a client's own knockback comes as
     * a velocity packet, which needs nothing here. `strength` is what the attacker's knockback passes, (x, z) its direction,
     * and the resistance the player's KNOCKBACK_RESISTANCE. `roll` is the random.nextDouble() it is rolled against to 1.15.
     * False, and no change, when vanilla would draw a random direction (from 1.21, a direction under 1.0E-5F squared).
     */
    knockback(state: PlayerState, hit: { strength: number; x: number; z: number; roll?: number }): boolean {
        const resistance = this.attributeValueOf(state.attributes, 'knockbackResistance') ?? 0;
        const out = this.binding.livingKnockback(this.version, { x: state.vel.x, y: state.vel.y, z: state.vel.z }, state.onGround, hit.strength, hit.x, hit.z, resistance, hit.roll);
        if (!out) return false;
        state.vel.set(out.x, out.y, out.z);
        return true;
    }

    /**
     * A landed hit's knockback on the attacker, once: Player.attack runs from the keybinds, before the entities tick (26.3
     * Minecraft.java:1786, :1797; 1.20.4 Minecraft.java:1847, :1866). Any knockback above 0 takes 0.6 of the horizontal
     * motion and ends the sprint (1.8.8 Player.java:1023-1027, 1.20.4 :1168, 1.21.10 Player.java:968-977, 26.3
     * :1052-1053). A sprint adds 1, 0.5 from 1.21.11 (1.21.10 :1011, 1.21.11 :949, 26.3 :922), and from 1.9 only at full
     * strength, above 0.9 (1.8.8 :994-995, 1.9.4 :996 and :1002, 26.3 :898).
     */
    private takeAttackHit(state: PlayerState): void {
        const hit = state.attackHit;
        if (hit == null) return;
        state.attackHit = null;
        const sprintKnockback = state.sprinting && (this.lt('1.9') || hit.strength > f32(0.9));
        const sprintBonus = this.lt('1.21.11') ? 1 : 0.5;
        if ((hit.knockback ?? 0) + (sprintKnockback ? sprintBonus : 0) <= 0) return;
        state.vel.x *= 0.6;
        state.vel.z *= 0.6;
        this.setSprinting(state, false);
    }

    /**
     * The double tap's window: the sprintWindow option from 1.21.9 (1.21.9 LocalPlayer.java:731, Options.java:783-795, IntRange
     * 0 - 10 and 7 by default; 26.3 LocalPlayer.java:790), 7 before (1.21.8 LocalPlayer.java:700). OptionInstance.set takes
     * an illegal value for the default (26.3 OptionInstance.java:117-121, IntRange.validateValue :434-436).
     */
    private sprintWindow(state: PlayerState): number {
        const value = state.sprintWindow ?? 7;
        return this.gt('1.21.8') && Number.isInteger(value) && value >= 0 && value <= 10 ? value : 7;
    }

    /**
     * LocalPlayer.canStartSprinting, and before 1.19.4 the terms both sprint starts share: no glide term before 1.19.4,
     * a strict one to 1.21.4 (1.19.3 LocalPlayer.java:698-721, 1.19.4 :1029-1036), the crouch from 1.21.4
     * (1.21.4 :1012-1015); the glide and shallow water spare a player under water from 1.21.5 (1.21.5
     * LocalPlayer.java:1019-1022), and from 1.21.9 a flying player is not held back in shallow water (isSprintingPossible,
     * 1.21.9 LocalPlayer.java:1014-1020). Before 1.21.5 the shallow water holds back only the sprint key (inputsToSprint).
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
            && !this.vehicleHoldsSprint(state)
            && terms;
    }

    /** isWalking: 0.8 forward, from 1.14 any forward under water (1.14.4 LocalPlayer.java:925-928). */
    private hasEnoughImpulseToStartSprinting(state: PlayerState, heading: { forward: number }): boolean {
        if (this.gt('1.21.4')) return heading.forward > f32(1e-5); // (1.21.5 ClientInput.java:24, 1.0E-5F)
        if (this.lt('1.14')) return heading.forward >= 0.8;
        return heading.forward >= (state.isUnderWater ? f32(1e-5) : 0.8);
    }

    /** A passenger needs no food from 1.19.3 (1.19.2 LocalPlayer.java:768, 1.19.3 :1052-1053). */
    private hasEnoughFoodToSprint(state: PlayerState): boolean {
        return (this.gt('1.19.2') && this.isPassenger(state)) || state.food > 6 || state.mayFly;
    }

    /** A passenger's vehicle that cannot sprint, from 1.19.4 (canVehicleSprint, 1.19.4 LocalPlayer.java:1029-1040). */
    private vehicleHoldsSprint(state: PlayerState): boolean {
        return this.gt('1.19.3') && this.isPassenger(state) && !state.vehicleCanSprint;
    }

    /**
     * The ground a double tap starts from: the vehicle's also in 1.19.3, only the vehicle's for a passenger from 1.19.4
     * (1.19.3 LocalPlayer.java:699, 1.19.4 :691).
     */
    private sprintGround(state: PlayerState): boolean {
        if (!this.gt('1.19.2') || !this.isPassenger(state)) return state.onGround;
        const vehicle = state.vehicleOnGround ?? state.onGround;
        return this.gt('1.19.3') ? vehicle : state.onGround || vehicle;
    }

    private hasForwardImpulseToKeepSprinting(heading: { forward: number }): boolean {
        return this.lt('1.14') ? heading.forward >= 0.8 : heading.forward > f32(1e-5);
    }

    /** From 1.21.9 a flying player keeps the sprint in shallow water (isSprintingPossible, 1.21.9 LocalPlayer.java:814-817). */
    private shouldStopRunSprinting(state: PlayerState, heading: { forward: number }): boolean {
        return (this.gt('1.21.3') && this.effectAmplifier(state, state.blindness, 'Blindness') !== undefined)
            || (this.gt('1.21.3') && this.vehicleHoldsSprint(state))
            || !this.hasForwardImpulseToKeepSprinting(heading)
            || !this.hasEnoughFoodToSprint(state)
            || (state.isCollidedHorizontally && !state.isCollidedHorizontallyMinor)
            || (state.isInWater && !state.isUnderWater && !(this.gt('1.21.8') && state.flying));
    }

    /**
     * Before 1.21.5 the food ends a swim only off the ground without the sneak key, and blindness not at all
     * (!onGround && !sneak && (!forward || !food) || !isInWater, 1.14.4 LocalPlayer.java:632-640, 1.21.4
     * LocalPlayer.java:692-697); from 1.21.5 shouldStopSwimSprinting (1.21.5 LocalPlayer.java:819-822).
     */
    private shouldStopSwimSprinting(state: PlayerState, heading: { forward: number }): boolean {
        const forward = this.hasForwardImpulseToKeepSprinting(heading);
        if (this.lt('1.21.5')) {
            return (!state.onGround && !state.control.sneak && (!forward || !this.hasEnoughFoodToSprint(state))) || !state.isInWater;
        }
        return this.effectAmplifier(state, state.blindness, 'Blindness') !== undefined
            || this.vehicleHoldsSprint(state)
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
            const unsynced = synced === undefined || synced.unsynced === true;
            state.attributes = { ...state.attributes, [key]: { value: synced?.value ?? f32(0.1), modifiers, unsynced } };
        }
        // 1.8's EntityPlayerSP.setSprinting (1.8.8 LocalPlayer.java:347-350)
        if (this.lt('1.9')) state.sprintingTicksLeft = value ? 600 : 0;
        state.sprinting = value;
    }

    /**
     * LocalPlayer.aiStep's flight: a second press of jump within 7 ticks toggles flying where the player may fly (since
     * 1.20.5 a start on the ground jumps); since 1.15 a fresh jump in the air with a usable elytra starts gliding. The jump
     * is the input's: an auto-jump presses it (makeJump) for its tick and the wasJumping of the next, and does not toggle
     * flying (!wasJumping && jumping && !autoJumping: 1.10.2 LocalPlayer.java:656-714, 1.15
     * LocalPlayer.java:628-711, 1.21.11 LocalPlayer.java:711-722, 768, 784).
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
                } else if ((this.lt('1.13') || !state.swimming) && !this.vehicleHoldsFlight(state)) {
                    state.flying = !state.flying;
                    if (state.flying && state.onGround && !this.lt('1.20.5')) this.jumpFromGround(state, sync);
                    flyChanged = true;
                    state.flyJumpTriggerTime = 0;
                }
            }
        }
        // (Player.tryToStartFallFlying does not start under levitation from 1.16, nor in lava from 26.3: !isInLiquid(),
        // 26.3 Player.java:1351-1352; 26.2 :1366 has !isInWater()); aiStep tests the climbable from 1.15.2 (!onLadder();
        // none in 1.15 LocalPlayer.java:711-713 nor 1.15.1, 1.16 :740), and no passenger starts a glide (1.15 :711,
        // 1.16.5 :715, 1.20.4 :743; 26.3 LivingEntity.canGlide :3143)
        const levitating = this.effectAmplifier(state, state.levitation, 'Levitation') !== undefined && !this.lt('1.16');
        const inLava = state.isInLava && !this.lt('26.3');
        const climbing = state.onClimbable && !this.lt('1.15.2');
        state.validElytraEquipped = this.gliderEquipped(state);
        if (!this.lt('1.15') && jumping && !wasJumping && !flyChanged && !state.flying && !climbing && !state.onGround
            && !state.isInWater && !inLava && !state.fallFlying && !levitating && state.validElytraEquipped && !this.isPassenger(state)) {
            state.fallFlying = true;
        }
    }

    /**
     * Whether the equipment lets a glide start, for a state with a bot's inventory; else the state's own validElytraEquipped.
     * Before 1.21.2 an elytra in the chest slot with a use left (ElytraItem.isUsable, 1.9.4 ElytraItem.java:29-31, 1.20.4
     * :18-20, client1.21.1 :19-21), which an unbreakable one does not change. From 1.21.2 any slot holding a GLIDER item
     * equippable to that slot that the next damage would not break (client1.21.2 LivingEntity.java:2889-2900, :3681-3687,
     * 26.3 :3142-3148, :3999-4006).
     *
     * This is the client's prediction. The durability a glide costs and the end of a glide on a glider that stops being
     * usable are the server's (LivingEntity.updateFallFlying: `!isClientSide`, client1.21.1 :2759-2777, 26.3 :3119-3127,
     * and 1.9.4 :1738-1753, which only sets the flag on the server): a client tick neither damages the item nor clears
     * fallFlying, which goes when the server's flags say so.
     */
    private gliderEquipped(state: PlayerState): boolean {
        const bot = state.bot;
        const inventory = bot?.inventory;
        if (!inventory || !bot.getEquipmentDestSlot) return state.validElytraEquipped;
        const worn = (destination: string) => inventory.slots[bot.getEquipmentDestSlot!(destination)];
        if (this.lt('1.21.2')) {
            const chest = worn('torso');
            return chest?.name === 'elytra' && (chest.durabilityUsed ?? 0) < maxDamageOf(chest) - 1;
        }
        return canGlideUsing(bot.heldItem, 'main_hand') || canGlideUsing(worn('off-hand'), 'off_hand') || canGlideUsing(worn('feet'), 'feet')
            || canGlideUsing(worn('legs'), 'legs') || canGlideUsing(worn('torso'), 'chest') || canGlideUsing(worn('head'), 'head');
    }

    /**
     * From 1.21.11 a passenger toggles flight only on a vehicle it can jump: getVehicle() == null || jumpableVehicle() != null
     * (1.21.11 LocalPlayer.java:771, 26.3 :820; no vehicle term in 1.21.10). The press that fails keeps the trigger.
     */
    private vehicleHoldsFlight(state: PlayerState): boolean {
        return !this.lt('1.21.11') && this.isPassenger(state) && !state.vehicleJumpable;
    }

    /**
     * LivingEntity.jumpFromGround for the flight a player starts on the ground (the engine jumps only on foot; this path
     * opens at 1.20.5): getJumpPower, Jump Boost by its amplifier, and from 1.21.2 a faster rise stays (1.20.5
     * LivingEntity.java:2063-2086, 1.21.1 :2081-2085, 1.21.2 :2160-2164, 26.3 :2294-2315). The block under it is the one
     * below that affects its movement, at the x and z of the block that supports it (getOnPos).
     */
    private jumpFromGround(state: PlayerState, sync: NativeWorldSync): void {
        const honey = (x: number, y: number, z: number) => sync.blockAt(Math.floor(x), Math.floor(y), Math.floor(z))?.effect === EFFECT.honey;
        const carry = this.carries.get(state);
        const kept = carry && samePosition(carry.position, state.pos) ? carry.player.state() : undefined;
        const support = state.supportingBlockPos ?? (kept?.supportingBlockKept ? kept.supportingBlock : undefined) ?? state.pos;
        // Entity.getBlockPosBelowThatAffectsMyMovement: 1.19.4 Entity.java:771 0.5000001, 1.20 Entity.java:798 getOnPos(0.500001F)
        const below = this.lt('1.20') ? 0.5000001 : f32(0.500001);
        const factor = honey(state.pos.x, state.pos.y, state.pos.z) || honey(support.x, state.pos.y - below, support.z) ? 0.5 : 1;
        const amplifier = this.effectAmplifier(state, state.jumpBoost, 'JumpBoost');
        const jumpBoost = amplifier === undefined ? 0 : f32(f32(0.1) * f32(amplifier + 1));
        const strength = f32(this.attributeValueOf(state.attributes, 'jumpStrength') ?? f32(0.42));
        const power = f32(f32(strength * factor) + jumpBoost);
        if (power <= f32(1.0e-5)) return;
        state.vel.y = this.lt('1.21.2') ? power : Math.max(power, state.vel.y);
        if (state.sprinting) {
            const yaw = floatRadians(this.yawDegrees(state));
            state.vel.x += -this.mth.sin(yaw) * 0.2;
            state.vel.z += this.mth.cos(yaw) * 0.2;
        }
    }

    /**
     * LivingEntity.isAutoSpinAttack (1.13+): bit 4 of the living entity flags the server syncs (the client never sets it,
     * 1.16.5 LivingEntity.java:2603-2611, 1.21.11 Player.java:497-504): the synced data at 6 in 1.13, 7 from 1.14 (the
     * pose), 8 from 1.17 (the frozen ticks).
     */
    private isAutoSpinAttack(state: PlayerState): boolean {
        if (this.lt('1.13')) return false;
        const flags = state.bot?.entity?.metadata?.[livingFlagsSlot(this.version)!];
        return typeof flags === 'number' && (flags & 4) !== 0;
    }

    /**
     * Player.updatePlayerPose (1.14+): nothing changes without room to swim; else the desired pose when the player is a
     * spectator, a passenger or fits in it, else crouching where that fits, else swimming (1.14.4 Player.java:333-362,
     * 1.20.4 :371-399, 1.21.5 Player.java:423-439, 1.21.11 :336-352).
     */
    private updatePoses(state: PlayerState, sync: NativeWorldSync): void {
        const fits = (pose: number) => this.poseFits(sync, pose, state);
        if (!fits(Pose.SWIMMING)) return;
        const wanted = this.desiredPose(state);
        if (state.gameMode === 'spectator' || this.isPassenger(state) || fits(wanted)) state.pose = wanted;
        else if (fits(Pose.SNEAKING)) state.pose = Pose.SNEAKING;
        else state.pose = Pose.SWIMMING;
    }

    /** The size the end of Player.tick leaves: its pose's from 1.14, before the box updateSize kept. */
    private sizeAfterTick(state: PlayerState, moved: Size): Size {
        if (!this.lt('1.14')) return this.poseSize(state.pose, state.scale);
        return state.keptBoundingBox ? { width: state.keptBoundingBox.width, height: state.keptBoundingBox.height } : moved;
    }

    /**
     * The pose updatePlayerPose wants: up to 1.21.4 gliding, sleeping, swimming, the riptide spin, crouching (not
     * while flying), standing (1.14.4 Player.java:335-348, 1.21.4 :412-425); from 1.21.5 getDesiredPose's
     * sleeping, swimming, gliding, then the rest in the same order (1.21.5 Player.java:441-453, 1.21.11 :354-366).
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

    /**
     * Entity.isPassenger: the state's passenger flag, else whether mineflayer has the bot riding a vehicle. A bot's own
     * riding ticks go through mineflayer's vehicle paths (the boat and mount physics in vehicles.ts) and never reach
     * simulate; bot.physics.simulatePlayer can, on a riding bot, and gets the ride tick (PlayerState.passenger), not a throw.
     */
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
     * in the riptide spin, 1.13.2 Player.java:418-436), 1.65F sneaking, else 1.8F. */
    private legacyPlayerHeight(state: PlayerState, sneaking: boolean): number {
        if (this.lt('1.9')) return f32(1.8);
        if (state.fallFlying || (!this.lt('1.13') && (state.swimming || this.isAutoSpinAttack(state)))) return f32(0.6);
        return f32(sneaking ? 1.65 : 1.8);
    }

    /** LivingEntity.onClimbable: a climbable block at the feet, or (1.9+) an open trapdoor over a ladder facing its way. */
    private isInClimbable(state: PlayerState, sync: NativeWorldSync): boolean {
        if (state.gameMode === 'spectator') return false;
        // Player.onClimbable: never while flying from 1.21.4 (1.21.4 Player.java:2053-2058)
        if (state.flying && !this.lt('1.21.4')) return false;
        const x = Math.floor(state.pos.x);
        const y = Math.floor(state.pos.y);
        const z = Math.floor(state.pos.z);
        const feet = sync.blockAt(x, y, z);
        if (!feet) return false;
        // a fall-flying player is not on a #can_glide_through block from 1.21.11 (1.21.11 LivingEntity.java:1646-1647)
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
        // (before 1.17 the synced pose 6 is DYING, LONG_JUMPING coming in at 6 with 1.17: 1.14.4 Pose.java:4-10,
        // 1.16.5 :4-10, 1.17 :4-11)
        if (pose === Pose.LONG_JUMPING && this.lt('1.17')) pose = Pose.DYING;
        const size = POSE_SIZES[pose] ?? POSE_SIZES[Pose.STANDING];
        // the sleeping and dying dimensions are fixed, which the scale leaves (1.20.5 LivingEntity.java:3265,
        // Player.java:2049, EntityDimensions.java:38-41; 1.21.11 Avatar.java:55)
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
        // 1.13: 0.4F swimming, gliding or in a 0.6F box, the riptide spin's (1.13.2 Player.java:1855-1862)
        const lowBox = !this.lt('1.13') && this.lt('1.14') && state.pose !== Pose.SLEEPING
            && (state.swimming || state.fallFlying || state.keptBoundingBox?.height === f32(0.6));
        if (state.pose === Pose.SLEEPING) {
            // 0.2F asleep, its fixed dimensions unscaled (1.8.8 Player.java:1633-1643, which still takes the
            // 0.08F off sneaking; 1.12.2 :1753-1764; 1.14.4 LivingEntity.java:2750; 1.20.5 LivingEntity.java:164, 3265)
            return this.lt('1.9') && state.prevControl.sneak ? f32(f32(0.2) - f32(0.08)) : f32(0.2);
        }
        if (lowBox) {
            eyeHeight = f32(0.4);
        } else if (this.lt('1.14') && (state.prevControl.sneak || sneakBox)) {
            eyeHeight = f32(f32(1.62) - f32(0.08));
        } else if (!this.lt('1.9') && this.lt('1.13') && (state.fallFlying || state.keptBoundingBox?.height === f32(0.6))) {
            // 1.9 - 1.12: 0.4F gliding or in a 0.6F box, after the sneak's (1.9.4 PlayerEntity.java:1750-1761, 1.12.2 :1769-1780)
            eyeHeight = f32(0.4);
        } else if (state.pose === Pose.SNEAKING) {
            eyeHeight = 1.27;
        } else if (state.pose === Pose.DYING) {
            // 1.62F dying: the default case before 1.20.5 (1.14.4 Player.java:1741-1751), then the fixed
            // dimensions' eye, which the scale leaves (1.20.5 Player.java:2049, EntityDimensions.java:38-41;
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
     * Entity.wouldPoseNotCollide (canPlayerFitWithinBlocksAndEntitiesWhen in 26.3): noCollision for the pose's box at the
     * player's position, shrunk by 1e-7 from 1.16 (1.14.4 Entity.java:1769-1771, 1.15 :1651-1653, 1.16 :1687-1689, 26.3
     * Player.java:364-366), which sees the blocks, the entities that collide and the world border (26.3
     * CollisionGetter.java:52-54, 74-81).
     */
    private poseFits(sync: NativeWorldSync, pose: number, state: PlayerState, scale = state.scale): boolean {
        const size = this.collider(pose, state.pos, scale);
        const query = this.lt('1.16') ? size : shrink(size, 1e-7);
        return this.isFree(sync, query, state) && !this.meetsEntityCollider(state, query) && this.borderLeavesFree(sync, query, state);
    }

    /** The border's term of poseFits: for 1.14 - 1.17 it reads the box the player has, not the box it asks about. */
    private borderLeavesFree(sync: NativeWorldSync, query: Box, state: PlayerState): boolean {
        const now = this.collider(state.pose, state.pos, state.scale);
        const corners = (b: Box) => [{ x: b.minX, y: b.minY, z: b.minZ }, { x: b.maxX, y: b.maxY, z: b.maxZ }] as const;
        return sync.world.noBorderCollision(...corners(query), ...corners(now), state.pos.x, state.pos.z, this.version);
    }

    /**
     * Whether an entity that collides meets the box grown by 1e-7, as the entities' collisions take it (client1.14.4
     * EntityGetter.java:54-68, 1.15 EntityGetter.java:53-67, 1.16.5 :59-76, 26.3 CollisionGetter.java:70-72), where the
     * player's turn finds it: its own tick done when the client ticks it first. The size update before 1.14 passes no
     * entity to its query and has no such term (1.12.2 Level.java:1206-1208, 1.13.2 Player.java:439-442).
     */
    private meetsEntityCollider(state: PlayerState, query: Box): boolean {
        const reach = shrink(query, -1e-7);
        return (state.nearbyEntities ?? []).some(entity => {
            const { boundingBox: b } = entity.ticksBeforePlayer ? entity.afterTick ?? entity : entity;
            const kind = this.entityKind(state, entity);
            // (HappyGhast.canBeCollidedWith: a player at or above the top of an adult, 26.3 HappyGhast.java:522-532)
            const collides = kind === 'boat' || kind === 'shulker' || kind === 'happy_ghast_stays_still'
                || (kind === 'happy_ghast' && state.pos.y >= b.maxY);
            return collides && b.minX < reach.maxX && b.maxX > reach.minX && b.minY < reach.maxY && b.maxY > reach.minY
                && b.minZ < reach.maxZ && b.maxZ > reach.minZ;
        });
    }

    /**
     * Whether no collision box of the blocks in the box's cells (and the row under them when `underlying`) meets it,
     * the scaffolding and powder snow shaped for the player (EntityCollisionContext). A cell one past the box on one axis
     * counts with a shape that reaches out of its cell: an open shulker box's lid (BlockCollisions' edge cells take a block
     * that hasLargeCollisionShape, which a dynamic shape has, 1.14 and 26.3 BlockBehaviour.hasLargeCollisionShape).
     */
    private isFree(sync: NativeWorldSync, query: Box, state: PlayerState, underlying = false): boolean {
        const [x0, x1] = [Math.floor(query.minX), Math.floor(query.maxX)];
        const [y0, y1] = [Math.floor(query.minY) - Number(underlying), Math.floor(query.maxY)];
        const [z0, z1] = [Math.floor(query.minZ), Math.floor(query.maxZ)];
        for (let y = y0 - 1; y <= y1 + 1; y += 1) {
            for (let z = z0 - 1; z <= z1 + 1; z += 1) {
                for (let x = x0 - 1; x <= x1 + 1; x += 1) {
                    const edges = Number(x < x0 || x > x1) + Number(y < y0 || y > y1) + Number(z < z0 || z > z1);
                    if (edges > 1) continue;
                    // (an edge cell counts for a shape that leaves its cell: an open shulker box's lid, and from 1.14.4 a moving
                    // piston block's, BlockState.hasLargeCollisionShape; the engine's reads_edge_cells)
                    const lid = edges === 1 ? sync.shulkerLidAt(x, y, z) : undefined;
                    const moved = edges === 1 && !lid && !this.lt('1.14.4') ? sync.movingBoxesAt(x, y, z) : undefined;
                    if (edges === 1 && !lid && !moved) continue;
                    const boxes = lid ? [lid] : moved ?? this.collisionBoxes(sync, state, x, y, z);
                    for (const [minX, minY, minZ, maxX, maxY, maxZ] of boxes) {
                        if (!(x + minX < query.maxX && x + maxX > query.minX && y + minY < query.maxY && y + maxY > query.minY
                            && z + minZ < query.maxZ && z + maxZ > query.minZ)) continue;
                        // (from 1.13 an overlap the shapes' merger drops does not count, 1.14.4 LevelReader.java, 26.3
                        // BlockCollisions.java:91-104: a thin wall the box lies flush with fits)
                        const cube = boxes.length === 1 && minX === 0 && minY === 0 && minZ === 0 && maxX === 1 && maxY === 1 && maxZ === 1;
                        const meets = this.binding.collisionBoxMeetsQuery(
                            this.version, { x: x + minX, y: y + minY, z: z + minZ }, { x: x + maxX, y: y + maxY, z: z + maxZ }, cube,
                            { x: query.minX, y: query.minY, z: query.minZ }, { x: query.maxX, y: query.maxY, z: query.maxZ },
                        );
                        if (meets) return false;
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
            return this.canWalkOnPowderSnow(state) && above && !state.control.sneak ? [[0, 0, 0, 1, 1, 1]] : [];
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
