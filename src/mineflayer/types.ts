// The shapes of mineflayer-physics-utils' states the adapter reads and writes (its replay-parity PlayerState, BoatState
// and RiddenMobState): structural, so the adapter works with the copy of mineflayer-physics-utils its caller has.

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
}

/** The box vanilla keeps between ticks before 1.17, with the position and size it was left with. */
export interface KeptBoundingBox {
    bb: Box;
    pos: Vec3;
    width: number;
    height: number;
}

export interface Effect {
    id: number;
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
export interface PlayerState {
    /**
     * The bot the state was made from: its world border (bot.worldBorder), its level's height and dimension (bot.game),
     * the boots it wears (bot.inventory's slot for "feet") and its entity's synced data by index (bot.entity.metadata).
     */
    bot?: {
        entity?: { metadata?: unknown[]; username?: string; uuid?: string };
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
        inventory?: { slots: ({ enchants?: ItemEnchants } | null | undefined)[] };
        getEquipmentDestSlot?(destination: string): number;
    };
    age: number;
    pos: Vec3;
    vel: Vec3;
    onGround: boolean;
    lastOnGround: boolean;
    onClimbable: boolean;
    isInWater: boolean;
    isUnderWater: boolean;
    eyeInWater?: boolean;
    isInLava: boolean;
    isInWeb: boolean;
    isCollidedHorizontally: boolean;
    isCollidedHorizontallyMinor: boolean;
    isCollidedVertically: boolean;
    fallDistance: number;
    scale: number;
    isInPowderSnow: boolean;
    wasInPowderSnow: boolean;
    keptBoundingBox: KeptBoundingBox | null;
    stuckSpeedMultiplier: Vec3;
    nearbyEntities: NearbyEntity[];
    /** The entities the player pushed back this tick, and how far (Entity.push: the other way). */
    pushedEntities?: { id: number; x: number; z: number }[];
    pistonDeltas: number[];
    pistonDeltasTick: number;
    jumpTicks: number;
    sprintTriggerTime: number;
    /** 1.8's EntityPlayerSP.sprintingTicksLeft: 600 when a sprint starts, which ends it at 0. */
    sprintingTicksLeft?: number;
    prevForwardImpulse: number;
    autoJumpEnabled: boolean;
    autoJumpTime: number;
    autoJumping: boolean;
    flyJumpTriggerTime: number;
    attributes: Attributes;
    yaw: number;
    pitch: number;
    yawDegrees?: number;
    control: Controls;
    prevControl: Controls;
    heading: { forward: number; strafe: number };
    prevHeading: { forward: number; strafe: number };
    isUsingItem: boolean;
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
    depthStrider: number;
    swiftSneak: number;
    riptide: number;
    itemUseTicks: number;
    autoSpinAttackTicks: number;
    soulSpeed: number;
    pose: number;
    gameMode: string;
    food: number;
    mayFly: boolean;
    flying: boolean;
    swimming: boolean;
    sprinting: boolean;
    crouching: boolean;
    fallFlying: boolean;
    flySpeed: number;
    validElytraEquipped: boolean;
    canWalkOnPowderSnow: boolean;
    fireworkRocketDuration: number;
    fireworkRockets: number;
    fireworkRocketsBeforePlayer: number;
    supportingBlockPos: Vec3 | null;
    /** Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+). */
    removed?: boolean;
    getBB(): Box & { clone(): Box };
}

/** mineflayer-physics-utils' EPhysicsCtx, the fields the adapter uses. */
export interface PhysicsCtx<State = unknown> {
    state: State;
}
