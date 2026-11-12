// The shapes of mineflayer-physics-utils' states the adapter reads and writes (its replay-parity PlayerState, BoatState
// and RiddenMobState): structural, so the adapter works with the copy of mineflayer-physics-utils its caller has.
import type { NativeBoatVehicle } from '../native';
import type { PistonBot } from './base';

export interface Vec3 {
    x: number;
    y: number;
    z: number;
    set(x: number, y: number, z: number): Vec3;
    clone(): Vec3;
}

export interface Box {
    minX: number;
    minY: number;
    minZ: number;
    maxX: number;
    maxY: number;
    maxZ: number;
}

export interface Controls {
    forward: boolean;
    back: boolean;
    left: boolean;
    right: boolean;
    jump: boolean;
    sprint: boolean;
    sneak: boolean;
}

export interface AttributeModifier {
    uuid?: string;
    id?: string;
    amount: number;
    operation: number;
}

export interface Attribute {
    value: number;
    /** Absent in a compact attribute: no modifiers, so the value is the clamped base. */
    modifiers?: AttributeModifier[];
    /** Made by setSprinting with no attribute from the server: the base and the sprint, not the effects' modifiers. */
    unsynced?: boolean;
}

export type Attributes = Record<string, Attribute | undefined>;

/** An entity near the player (mineflayer-physics-utils' NearbyEntity). */
export interface NearbyEntity {
    /** Its entity id, to tell whom a push went to (pushedEntities). */
    id?: number;
    /** It carries passengers: a push from the player does not move it (Entity.push). */
    isVehicle?: boolean;
    name: string;
    living: boolean;
    position: Vec3;
    boundingBox: Box;
    ticksBeforePlayer?: boolean;
    /**
     * A firework rocket attached to the player, listed in the client's tick order: it boosts the gliding player in its
     * turn, between the pushes of the entities before and after it, and is not counted in fireworkRockets.
     */
    attachedRocket?: boolean;
    afterTick?: { position: Vec3; boundingBox: Box };
    sameVehicle?: boolean;
    baby?: boolean;
    staysStill?: boolean;
    /**
     * LivingEntity.isAlive: false for a mob whose synced health is 0, in the ticks of its death before it is removed.
     * Absent: alive. A dying shulker or happy ghast is no collider, and from 1.11 no living entity pushes while dying.
     */
    alive?: boolean;
}

/** The box vanilla keeps between ticks before 1.17, with the position and size it was left with. */
export interface KeptBoundingBox {
    bb: Box;
    pos: Vec3;
    width: number;
    height: number;
}

export interface Effect {
    amplifier: number;
}

/** The world border as the client knows it (mineflayer's bot.worldBorder): its centre and its size now. */
export interface WorldBorder {
    centerX: number;
    centerZ: number;
    size: number;
    /**
     * The size its edges have now (getMinX(0.0F)): from 1.21.11 a moving border's size before its last tick
     * (WorldBorder.MovingBorderExtent.previousSize), else its size.
     */
    previousSize?: number;
    /** The bound its edges are clamped to (WorldBorder.absoluteMaxSize, the server's max-world-size). */
    absoluteMaxSize?: number;
}

/**
 * prismarine-item's Item.enchants: { name, lvl } from the nbt, from 1.20.5 the enchantments component as sent,
 * { enchantments: [{ id, level }] }.
 */
export type ItemEnchants = { name: string; lvl: number }[] | { enchantments?: { id: number; level: number }[] };

/** mineflayer-physics-utils' PlayerState, the fields the adapter uses. */
/** What an item in a slot holds that the glider rules read (prismarine-item's Item). */
export interface EquippedItem {
    name?: string;
    enchants?: ItemEnchants;
    /** The stack's damage; its components' damage from 1.20.5. */
    durabilityUsed?: number | null;
    /** The registry's maximum damage: 0 or absent on an item that takes none. */
    maxDurability?: number;
    /** The stack's added components (1.20.5+) by type. */
    componentMap?: Map<string, { type: string; data?: unknown }>;
    /** The components the stack removes from its item's defaults (1.20.5+). */
    removedComponents?: { type: string }[];
}

export interface PlayerState {
    /**
     * The bot the state was made from: its world border (bot.worldBorder), its level's height and dimension (bot.game),
     * the boots it wears (bot.inventory's slot for "feet") and its entity's synced data by index (bot.entity.metadata).
     */
    bot?: PistonBot & {
        entity?: { metadata?: unknown[]; username?: string; uuid?: string; vehicle?: unknown; passengers?: unknown[] };
        /** mineflayer's vehicle plugin: the entity the bot rides, null when it rides none. */
        vehicle?: unknown;
        /** The bot's player entry: its UUID (a tamed animal's owner). */
        player?: { uuid?: string };
        /** The bot's name, its scoreboard name (Entity.getScoreboardName). */
        username?: string;
        /** mineflayer's health plugin: 0 once dead. */
        health?: number;
        /** mineflayer's bed plugin: in a bed. */
        isSleeping?: boolean;
        /** The entities by id: a player's name, another's UUID (its scoreboard name); a tamed animal's synced data. */
        entities?: Record<number, { type?: string; name?: string; username?: string; uuid?: string; metadata?: unknown[] } | undefined>;
        /** The players by name: their game mode (3 spectator). */
        players?: Record<string, { gamemode?: number } | undefined>;
        /** The teams by member scoreboard name: a team's collisionRule ("pushOtherTeams" style, or its id). */
        teamMap?: Record<string, { collisionRule?: unknown } | undefined>;
        worldBorder?: WorldBorder;
        game?: { minY?: number; height?: number; dimension?: string };
        /** mineflayer's rain plugin: the level of the last RAIN_LEVEL_CHANGE. */
        rainState?: number;
        inventory?: { slots: (EquippedItem | null | undefined)[] };
        /** The item in the selected hotbar slot (mineflayer's bot.heldItem). */
        heldItem?: EquippedItem | null;
        getEquipmentDestSlot?(destination: string): number;
    };
    age: number;
    pos: Vec3;
    vel: Vec3;
    onGround: boolean;
    lastOnGround: boolean;
    onClimbable: boolean;
    isInWater: boolean;
    /**
     * Entity.isInRain: a riptide launches, a fire goes out. Not given, the adapter derives it each tick from the bot's rain
     * level, sky light, blocks and biome (NativePhysics.rainExposed) and does not write it here; given, true or false, it is
     * the caller's and stays. A state without a bot has no rain to derive.
     */
    inRain?: boolean;
    isUnderWater: boolean;
    eyeInWater?: boolean;
    isInLava: boolean;
    isInWeb: boolean;
    isCollidedHorizontally: boolean;
    isCollidedHorizontallyMinor: boolean;
    isCollidedVertically: boolean;
    fallDistance: number;
    /** The amount of the last tick's landing (LivingEntity.causeFallDamage's, a prediction of the server's: NativeSimulationState.fallDamage), 0 without one, and always 0 on 1.21.4; written by the adapter. */
    fallDamage?: number;
    /** What the tick's blocks hand hurt, added up over its contacts: the server's alone, so 0 on the bot's own state; written by the adapter. */
    contactDamage?: number;
    /** What the tick's flight into a wall hands hurt, the server's alone as well; written by the adapter. */
    wallDamage?: number;
    /** The freeze damage dealt so far, a running count, the server's alone as well; written by the adapter. */
    freezeDamage?: number;
    /** The tick's move ran Block.updateEntityMovementAfterFallOn, once a move; written by the adapter. */
    blockFallCallback?: boolean;
    /**
     * The entity's own fall-damage boundary, sent as they are when set and left on the state after the tick:
     * SAFE_FALL_DISTANCE (else the synced attribute, else 3), the type's #fall_damage_immune (mayFly is immune
     * anyway), FALL_DAMAGE_MULTIPLIER (else the synced attribute), the BOUNCINESS attribute and a landing block's
     * bounciness, and Entity.isControlledByLocalInstance (true when absent: the client's own player).
     */
    safeFallDistance?: number;
    fallDamageImmune?: boolean;
    fallDamageMultiplier?: number;
    entityBounciness?: number;
    blockBounciness?: number;
    controlledByLocalInstance?: boolean;
    /**
     * Optional vanilla canSimulateMovement result for piston bounce/restitution in 26.3+; ignored earlier.
     * Omission uses controlledByLocalInstance for this client's player (true when omitted).
     */
    canSimulateMovement?: boolean;
    /** The scale the dimensions were last refreshed with (the attribute's, at the end of the tick that got it); written by the adapter. */
    scale: number;
    /** The pose those dimensions are of: another one on the state refreshes them; written by the adapter. */
    sizedPose?: number;
    isInPowderSnow: boolean;
    wasInPowderSnow: boolean;
    keptBoundingBox: KeptBoundingBox | null;
    stuckSpeedMultiplier: Vec3;
    nearbyEntities: NearbyEntity[];
    /** The entities the player pushed back this tick, and how far (Entity.push: the other way). */
    pushedEntities?: { id: number; x: number; z: number }[];
    jumpTicks: number;
    sprintTriggerTime: number;
    /** 1.8's EntityPlayerSP.sprintingTicksLeft: 600 when a sprint starts, which ends it at 0. */
    sprintingTicksLeft?: number;
    prevForwardImpulse: number;
    /** Options.sprintWindow from 1.21.9, the ticks a double tap of forward has to land (0 - 10, 0 turns it off); 7 when not given. */
    sprintWindow?: number;
    autoJumpEnabled: boolean;
    autoJumpTime: number;
    /** The last tick's motion was too large to walk, and the tick dropped it and did nothing else (docs/numeric-contract.md). */
    moveRefused?: boolean;
    autoJumping: boolean;
    flyJumpTriggerTime: number;
    attributes: Attributes;
    yaw: number;
    pitch: number;
    yawDegrees?: number;
    /** LivingEntity.yHeadRot in degrees as the last aiStep left it (the yaw then), which a 1.11 rocket that ticks before the
     * player boosts along; the engine remembers the last tick's yaw when not given. */
    headYawDegrees?: number;
    control: Controls;
    prevControl: Controls;
    heading: { forward: number; strafe: number };
    prevHeading: { forward: number; strafe: number };
    isUsingItem: boolean;
    /** The item in use is in the off hand (mineflayer-physics-utils' isUsingOffHand): where its Riptide is read. */
    isUsingOffHand?: boolean;
    useItemCanSprint?: boolean;
    useItemSpeedMultiplier?: number;
    effects: Record<number, Effect | undefined> | Effect[];
    jumpBoost: number;
    speed: number;
    slowness: number;
    dolphinsGrace: number;
    slowFalling: number;
    levitation: number;
    blindness: number;
    /** Depth Strider, Swift Sneak and Soul Speed where no bot inventory says: the bot's worn armor decides when it has one. */
    depthStrider: number;
    swiftSneak: number;
    /** The Riptide of the item in use, when the caller knows it: what the adapter reads of the item in use on the bot goes first. */
    riptide?: number;
    /** The Riptide of the item in use while it is, kept for its release; written by the adapter. Undefined with no use seen. */
    usedItemRiptide?: number;
    itemUseTicks: number;
    autoSpinAttackTicks: number;
    /**
     * A melee attack that landed since the last tick; the next tick takes it once (Player.attack's own recoil, not the
     * target's knockback). Set it only when the target's client side hurt returned true: on a client that is a remote player
     * (1.8.8 RemotePlayer.java:30, 1.20.4 :33, 26.3 RemotePlayer.java:31), never a mob (LivingEntity.damage is
     * false on a client, 1.20.4 LivingEntity.java:1104; Entity.hurtClient, 26.3 Entity.java:1990). A spear's stab is not this
     * event: a client has no recoil of its own from one, which is the server's (NativeSimulationState.stabHit).
     */
    attackHit?: {
        /** getAttackStrengthScale(0.5F) at the attack, read from 1.9. */
        strength: number;
        /**
         * The weapon's knockback without the sprint's, as the client reads it: EnchantmentHelper.getKnockback to 1.20.6; from 1.21
         * the attack_knockback attribute alone, as the enchantment is only asked in a ServerLevel (1.21 LivingEntity.java:1441-1447,
         * 26.3 :1531-1535). 0 when not given.
         */
        knockback?: number;
    } | null;
    soulSpeed: number;
    pose: number;
    gameMode: string;
    food: number;
    /** Level.getSeaLevel from 1.21.2, which the join packet says and mineflayer does not keep: 63 when not given. */
    seaLevel?: number;
    mayFly: boolean;
    /** Abilities.invulnerable (creative, or a command): caps a player's fire ticks at 1 from 1.16. */
    invulnerable?: boolean;
    flying: boolean;
    swimming: boolean;
    sprinting: boolean;
    crouching: boolean;
    fallFlying: boolean;
    flySpeed: number;
    /**
     * A glider the player can start with. The adapter derives it from a state's bot.inventory (NativePhysics.gliderEquipped);
     * without one it is the caller's own snapshot, sent when the equipment changes.
     */
    validElytraEquipped: boolean;
    /** PowderSnowBlock.canEntityWalkOnPowderSnow; else the adapter looks for the bot's leather boots. */
    canWalkOnPowderSnow?: boolean;
    fireworkRocketDuration: number;
    fireworkRockets: number;
    fireworkRocketsBeforePlayer: number;
    supportingBlockPos: Vec3 | null;
    /** Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+). */
    removed?: boolean;
    /** The player's NoGravity flag (Entity.isNoGravity) when the state has it; else bot.entity's synced data at 5. */
    noGravity?: boolean;
    /**
     * Riding a vehicle (Entity.isPassenger); else bot.vehicle or bot.entity.vehicle. A passenger's tick is Entity.rideTick
     * and never ordinary travel: its motion is zeroed, the rider's own tick runs (input, crouch, flight and the riptide
     * rules for a passenger; travel from rest, whose result stays in vel), and the tick ends on the position handed in with
     * the fall distance reset. The engine knows no vehicle: that position is the seat, Entity.positionRider's, and the
     * caller moves it with the vehicle each tick. Vehicles themselves tick elsewhere (NativeBoatPhysics,
     * NativeRiddenMobPhysics); sprint predicates read no vehicle (LocalPlayer.vehicleCanSprint), and the auto-spin's
     * sweep sees the seat, not the rider's discarded move. Dismounting is clearing the flag: the next tick walks from the
     * position with the vel the last ride tick left, and the dismount offset is the caller's.
     */
    passenger?: boolean;
    /**
     * Entity.isVehicle: the player carries passengers, which only a server seats on it (vanilla's /ride refuses a player
     * as a vehicle). From 1.9 nothing pushes it. Absent: whether the bot's entity lists passengers (mineflayer's
     * set_passengers).
     */
    isVehicle?: boolean;
    /** Explicit boat geometry/status at the fluid-query phase; the caller carries it when cloning a state. */
    boatVehicle?: NativeBoatVehicle | null;
    /** A passenger's vehicle can sprint and this client moves it (LocalPlayer.vehicleCanSprint: a camel, or a player); false when not given. */
    vehicleCanSprint?: boolean;
    /** A passenger's vehicle takes its jump (LocalPlayer.jumpableVehicle: a controlled PlayerRideableJumping whose canJump() holds); false when not given. */
    vehicleJumpable?: boolean;
    /** A passenger's vehicle is on the ground, for a double-tapped sprint before 1.21.5; the player's own onGround when not given. */
    vehicleOnGround?: boolean;
    getBB(): Box & { clone(): Box };
}

/** mineflayer-physics-utils' EPhysicsCtx, the fields the adapter uses. */
export interface PhysicsCtx<State = unknown> {
    state: State;
}

/** A tick's result, with whether it dropped a motion too large to walk and did nothing else (docs/numeric-contract.md). */
export type MoveRefusable<T> = T & { moveRefused?: boolean };
