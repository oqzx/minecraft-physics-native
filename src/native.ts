export interface NativeVec3 {
    x: number;
    y: number;
    z: number;
}

export interface NativeSimulationConfig {
    gravity?: number;
    drag?: number;
    movementSpeed?: number;
    sprintMultiplier?: number;
    jumpVelocity?: number;
    stepHeight?: number;
    negligibleVelocity?: number;
}

export interface NativeSimulationState {
    position: NativeVec3;
    velocity: NativeVec3;
    yaw?: number;
    pitch?: number;
    halfWidth: number;
    height: number;
    onGround: boolean;
    collidedHorizontally: boolean;
    minorHorizontalCollision?: boolean;
    collidedVertically: boolean;
    verticalCollisionBelow?: boolean;
    blockFallCallback?: boolean;
    entityBounciness?: number;
    blockBounciness?: number;
    /** The movement keys held: x is left (1) or right (-1), z forward (1) or back (-1). */
    input?: NativeVec3;
    jumpRequested?: boolean;
    jumpTicks?: number;
    /** The auto-jump option (1.10+): walking into a step jumps it the next tick. */
    autoJump?: boolean;
    /** LocalPlayer.autoJumpTime: a jump auto-jump queued for the next tick; pass back the one returned. */
    autoJumpTime?: number;
    /**
     * The riptide level of a trident the player let go of before this tick after using it 10 ticks or more
     * (TridentItem.releaseUsing, 1.13+): in water it launches the player along its yaw and pitch.
     */
    riptideLaunch?: number;
    sprinting?: boolean;
    /** Sprinting at the end of the previous tick: the airborne speed before 1.19.4 follows it. */
    wasSprinting?: boolean;
    /**
     * Whether the movement speed attribute holds the sprint modifier (LivingEntity.setSprinting adds it, a sync from the
     * server without it drops it); `sprinting` when left out.
     */
    sprintModifier?: boolean;
    inWater?: boolean;
    firstTick?: boolean;
    inLava?: boolean;
    ultraWarm?: boolean;
    isPlayer?: boolean;
    controlledByLocalInstance?: boolean;
    passenger?: boolean;
    inWeb?: boolean;
    weaving?: boolean;
    sweetBerryBushImmune?: boolean;
    powderSnowImmune?: boolean;
    powderSnowWalkable?: boolean;
    /** Entity.isInPowderSnow after the tick; given back, it is the tick before's (wasInPowderSnow, 1.21.5+ climbing). */
    inPowderSnow?: boolean;
    ticksFrozen?: number;
    ticksRequiredToFreeze?: number;
    tickCount?: number;
    freezeDamage?: number;
    contactDamage?: number;
    insideWall?: boolean;
    eyeHeight?: number;
    eyeInWater?: boolean;
    eyeInLava?: boolean;
    slowFalling?: boolean;
    levitation?: boolean;
    levitationAmplifier?: number;
    fallDistance?: number;
    safeFallDistance?: number;
    fallDamageImmune?: boolean;
    fallDamageMultiplier?: number;
    sneaking?: boolean;
    /** LocalPlayer.isMovingSlowly: crouching or crawling slows the input; defaults to `sneaking`. */
    movingSlowly?: boolean;
    steppingCarefully?: boolean;
    usingItem?: boolean;
    /** UseEffects.speedMultiplier of the item in use (1.21.11+, a spear's 1.0); 0.2 when absent. */
    itemUseSpeedMultiplier?: number;
    swimming?: boolean;
    flying?: boolean;
    flyingSpeed?: number;
    climbing?: boolean;
    noPhysics?: boolean;
    noGravity?: boolean;
    /** Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+). */
    removed?: boolean;
    gravity?: number;
    stepHeight?: number;
    /** The movement speed attribute with its additive modifiers; the engine applies sprint, speed and slowness. */
    movementSpeed?: number;
    jumpStrength?: number;
    speedEffectAmplifier?: number;
    slownessEffectAmplifier?: number;
    jumpBoostAmplifier?: number;
    dolphinsGrace?: number;
    depthStrider?: number;
    soulSpeed?: number;
    waterMovementEfficiency?: number;
    movementEfficiency?: number;
    airDragModifier?: number;
    frictionModifier?: number;
    sneakingSpeed?: number;
    fallDamage?: number;
    fallFlying?: boolean;
    look?: NativeVec3;
    fireworkBoost?: NativeVec3;
    /** Entity.stuckSpeedMultiplier: what the next move is scaled by, set inside cobwebs, berry bushes and powder snow. */
    stuckSpeedMultiplier?: NativeVec3;
    /**
     * The position the last tick started from: its move finds the block supporting an entity that walked off
     * an edge under the box where the move began (Entity.mainSupportingBlockPos, 1.20+).
     */
    previousPosition?: NativeVec3;
    /**
     * Before 1.17 the entity's bounding box, kept between ticks: the move starts from it and the position is taken
     * from it. Pass back the one the previous tick returned; leave it out after setting the position.
     */
    boundingBox?: NativeAabb;
    /**
     * The entities near the player this tick, in the order the client ticks them (simulateVersionedTick only): those
     * it ticks before the player push it before its move, the colliders among them stop its move, the others push it
     * after its move.
     */
    nearbyEntities?: NativeNearbyEntity[];
    /**
     * The firework rockets attached to the player that tick this tick (simulateVersionedTick only, 1.11+): each pulls a
     * gliding player toward 1.5 times its look vector (FireworkRocketEntity.tick).
     */
    fireworkRockets?: number;
    /** How many of `fireworkRockets` the client ticks before the player: they boost it before its move. */
    fireworkRocketsBeforePlayer?: number;
    /**
     * The blocks pistons move, as they are before this tick (simulateVersionedTick only): the client ticks them after
     * its entities, and each moves the player out of its way. The caller advances them (0.5F a tick).
     */
    movingPistons?: NativeMovingPiston[];
}

export interface NativeMovingPiston {
    x: number;
    y: number;
    z: number;
    /** The piston's facing as Direction.from3DDataValue: down, up, north, south, west, east (0 - 5). */
    facing: number;
    extending: boolean;
    /** The piston's own head or base rather than a block it pushes. */
    source: boolean;
    /** How far it has moved before this tick, a float. */
    progress: number;
    /** The collision boxes of the block it moves (getCollisionRelatedBlockState) in its cell; used from 1.11. */
    shapes: NativeAabb[];
}

export interface NativeNearbyEntity {
    /**
     * "living" (LivingEntity.pushEntities), "boat" (Boat.tick, a collider), "shulker" (a living collider) or
     * "happy_ghast" (a living collider to a player whose feet are at or above its top); other kinds neither push nor
     * collide.
     */
    kind: string;
    /** Where the player's move meets it. */
    position: NativeVec3;
    boundingBox: NativeAabb;
    /** The client ticks it before the player (1.14 - 1.16 tick a hash map of entities). */
    ticksBeforePlayer?: boolean;
    /** Where its own tick leaves it, from where it pushes the player; defaults to `position` and `boundingBox`. */
    tickPosition?: NativeVec3;
    tickBoundingBox?: NativeAabb;
}

export interface NativePistonPushResult {
    state: NativeSimulationState;
    /** How far the pistons of the tick have moved the player on each axis (Entity.pistonDeltas). */
    pistonDeltas: NativeVec3;
}

export interface NativeEntityPushState {
    position: NativeVec3;
    velocity: NativeVec3;
    noPhysics: boolean;
    vehicle: boolean;
    pushable: boolean;
    pushthrough: number;
}

export interface NativeEntityPushResult {
    leftVelocity: NativeVec3;
    rightVelocity: NativeVec3;
}

export interface NativeVersionedEntityState {
    simulation: NativeSimulationState;
    noPhysics: boolean;
    vehicleId?: number;
    pushable: boolean;
    pushthrough: number;
}

export interface NativePoseRefreshResult {
    state: NativeSimulationState;
    pose: number;
    changed: boolean;
}

export interface NativeEntityDescriptor {
    position: NativeVec3;
    velocity: NativeVec3;
    halfWidth: number;
    height: number;
    onGround: boolean;
    yaw: number;
    inWater: boolean;
    inLava: boolean;
    version?: string;
    pose?: number;
    pitch?: number;
    input?: NativeVec3;
    sprinting?: boolean;
    sneaking?: boolean;
    usingItem?: boolean;
    swimming?: boolean;
    passenger?: boolean;
    flying?: boolean;
    flyingSpeed?: number;
    climbing?: boolean;
    noGravity?: boolean;
    fallFlying?: boolean;
    fallDamageImmune?: boolean;
    slowFalling?: boolean;
    levitation?: boolean;
    levitationAmplifier?: number;
    fallDamageMultiplier?: number;
    movementSpeed?: number;
    jumpStrength?: number;
    speedEffectAmplifier?: number;
    slownessEffectAmplifier?: number;
    jumpBoostAmplifier?: number;
    dolphinsGrace?: number;
    depthStrider?: number;
    soulSpeed?: number;
    sneakingSpeed?: number;
    look?: NativeVec3;
    fireworkBoost?: NativeVec3;
}

export interface NativeWorld {
    setChunkLoaded(chunkX: number, chunkZ: number, loaded: boolean): void;
    isChunkLoaded(chunkX: number, chunkZ: number): boolean;
    setWorldBounds(minBuildHeight: number, maxBuildHeight: number, borderCenterX: number, borderCenterZ: number, borderSize: number): void;
    worldBounds(): NativeWorldBounds;
    isInsideWorldBorder(x: number, z: number): boolean;
    isInsideBuildHeight(y: number): boolean;
    isSectionPresent(chunkX: number, sectionY: number, chunkZ: number): boolean;
    setSectionPresent(chunkX: number, sectionY: number, chunkZ: number, present: boolean): void;
    sectionBlockCount(chunkX: number, sectionY: number, chunkZ: number): number;
    clampToWorldBorder(position: NativeVec3): NativeVec3;
    setFullBlock(x: number, y: number, z: number, id: number): void;
    setBlockStateId(x: number, y: number, z: number, version: string, id: number): void;
    setBlockEntity(x: number, y: number, z: number, kind: string): void;
    removeBlockEntity(x: number, y: number, z: number): void;
    setBlockProperties(x: number, y: number, z: number, properties: NativeBlockProperty[]): void;
    setBlockShape(x: number, y: number, z: number, id: number, shape: NativeBlockShape): void;
    setBlockOutlineShape(x: number, y: number, z: number, shape: NativeBlockShape): void;
    setBlockShapes(x: number, y: number, z: number, id: number, shapes: NativeBlockShape[], outlineShapes?: NativeBlockShape[]): void;
    removeBlock(x: number, y: number, z: number): void;
    setFluid(x: number, y: number, z: number, fluid: number): void;
    setFluidState(x: number, y: number, z: number, fluid: number, fluidTags: number, fluidLevel: number, fluidSource: boolean, fluidFalling: boolean, fluidFlow: NativeVec3): void;
    setVersionedFluidState(x: number, y: number, z: number, version: string, fluid: number, fluidTags: number, fluidLevel: number, fluidSource: boolean, fluidFalling: boolean, fluidFlow: NativeVec3): void;
    applyBlockUpdates(updates: NativeBlockUpdate[]): void;
    collisionShapes(min: NativeVec3, max: NativeVec3): NativeAabb[];
    outlineShapes(min: NativeVec3, max: NativeVec3): NativeAabb[];
    interactionShapes(min: NativeVec3, max: NativeVec3): NativeAabb[];
    fluidAt(x: number, y: number, z: number): number;
    fluidTagsAt(x: number, y: number, z: number): number;
    fluidHeightAt(x: number, y: number, z: number): number;
    fluidFlowAt(x: number, y: number, z: number): NativeVec3;
    blockEffectAt(x: number, y: number, z: number): number;
    blockTagsAt(x: number, y: number, z: number): number;
    blockAt(x: number, y: number, z: number): NativeBlockState;
}

export interface NativeAabb {
    min: NativeVec3;
    max: NativeVec3;
}

export interface NativeWorldBounds {
    minBuildHeight: number;
    maxBuildHeight: number;
    borderCenterX: number;
    borderCenterZ: number;
    borderSize: number;
}

export interface NativeBlockState {
    present: boolean;
    id: number;
    stateVersion?: string;
    properties: NativeBlockProperty[];
    tags: number;
    fluid: number;
    fluidTags: number;
    fluidLevel: number;
    fluidSource: boolean;
    fluidFlow: NativeVec3;
    fluidFalling: boolean;
    fallDamageResetting: boolean;
    effect: number;
    slipperiness: number;
    bounceRestitution: number;
    suppressesBounce: boolean;
    shape?: NativeAabb;
    outlineShape?: NativeAabb;
    interactionShape?: NativeAabb;
    blockEntity?: string;
    removeBlockEntity?: boolean;
}

export interface NativeBlockProperty {
    name: string;
    value: string;
}

export interface NativeBlockShape {
    minX: number;
    minY: number;
    minZ: number;
    maxX: number;
    maxY: number;
    maxZ: number;
    slipperiness: number;
    effect?: number;
    bounceRestitution?: number;
    suppressesBounce?: boolean;
    fallDamageResetting?: boolean;
}

export interface NativeGeneratedShapeBox {
    minX: number;
    minY: number;
    minZ: number;
    maxX: number;
    maxY: number;
    maxZ: number;
}

export interface NativeGeneratedShapeProvenance {
    sourceSha256?: string;
    hasMethod: boolean;
    stateDependent: boolean;
    propertyDependencies: string[];
    candidateBoxCount: number;
}

export interface NativeGeneratedShapePredicate {
    property: string;
    operator: string;
    value: string;
}

export interface NativeBlockUpdate {
    x: number;
    y: number;
    z: number;
    id?: number;
    stateVersion?: string;
    blockEntity?: string;
    properties?: NativeBlockProperty[];
    tags?: number;
    shape?: NativeBlockShape;
    outlineShape?: NativeBlockShape;
    interactionShape?: NativeBlockShape;
    fluid?: number;
    fluidTags?: number;
    fluidLevel?: number;
    fluidSource?: boolean;
    fluidFlow?: NativeVec3;
    fluidFalling?: boolean;
    bounceRestitution?: number;
    suppressesBounce?: boolean;
    fallDamageResetting?: boolean;
    /** BlockState.isSuffocating when it is not "a full collision cube" (glass, leaves: false). */
    suffocating?: boolean;
    remove?: boolean;
}

/**
 * A local player whose state the engine keeps between ticks (simulateVersionedTick's tick): each tick sends only what
 * changes, in a Float64Array laid out as NativePlayerInput, and gets the result back laid out as NativePlayerOutput.
 * The rest of the state (attributes, effects, the counters the engine keeps) stays until setState replaces it.
 */
export interface NativePlayer {
    /** Replaces the whole state, its nearby entities too. */
    setState(state: NativeSimulationState): void;
    /** The whole state after the last tick or push. */
    state(): NativeSimulationState;
    /** Replaces the bounding box kept before 1.17 (a refitted size). */
    setBoundingBox(boundingBox?: NativeAabb | null): void;
    /** One tick; `entities` holds this tick's nearby entities, NATIVE_PLAYER_ENTITY_LENGTH numbers each. */
    tick(world: NativeWorld, input: Float64Array, entities: Float64Array): Float64Array;
    /** The pushes of moving pistons after the tick, each call after the others of the same tick. */
    pushByMovingPistons(world: NativeWorld, pistons: NativeMovingPiston[]): Float64Array;
    /**
     * The pushes the nearby entities took back from the player in the last tick (Entity.push: the other way), three
     * numbers each: the entity's index in the entities array, then x and z.
     */
    entityPushes(): Float64Array;
}

/** The indices of a NativePlayer's input array (native/src/napi_player.rs). */
export const NativePlayerInput = {
    POSITION: 0,
    VELOCITY: 3,
    YAW: 6,
    PITCH: 7,
    LOOK: 8,
    STRAFE: 11,
    FORWARD: 12,
    HALF_WIDTH: 13,
    HEIGHT: 14,
    EYE_HEIGHT: 15,
    FLAGS: 16,
    FALL_DISTANCE: 17,
    JUMP_TICKS: 18,
    AUTO_JUMP_TIME: 19,
    STUCK_SPEED_MULTIPLIER: 20,
    RIPTIDE_LAUNCH: 23,
    FIREWORK_ROCKETS: 24,
    FIREWORK_ROCKETS_BEFORE_PLAYER: 25,
    FLYING_SPEED: 26,
    ITEM_USE_SPEED_MULTIPLIER: 27,
    LENGTH: 28,
} as const;

/** The bits of the input array's flags. UNMOVED: the player is where the last tick left it (else the position that
 * tick started from and the kept box go, as after a teleport). */
export const NativePlayerInputFlag = {
    JUMP_REQUESTED: 1 << 0,
    SPRINTING: 1 << 1,
    WAS_SPRINTING: 1 << 2,
    SNEAKING: 1 << 3,
    MOVING_SLOWLY: 1 << 4,
    SWIMMING: 1 << 5,
    FLYING: 1 << 6,
    FALL_FLYING: 1 << 7,
    USING_ITEM: 1 << 8,
    AUTO_JUMP: 1 << 9,
    NO_PHYSICS: 1 << 10,
    ON_GROUND: 1 << 11,
    COLLIDED_HORIZONTALLY: 1 << 12,
    MINOR_HORIZONTAL_COLLISION: 1 << 13,
    COLLIDED_VERTICALLY: 1 << 14,
    VERTICAL_COLLISION_BELOW: 1 << 15,
    IN_WATER: 1 << 16,
    IN_LAVA: 1 << 17,
    IN_POWDER_SNOW: 1 << 18,
    UNMOVED: 1 << 19,
    SPRINT_MODIFIER: 1 << 20,
    REMOVED: 1 << 21,
} as const;

/** The indices of a NativePlayer's output array; the kept box is NaN without one. */
export const NativePlayerOutput = {
    POSITION: 0,
    VELOCITY: 3,
    FLAGS: 6,
    FALL_DISTANCE: 7,
    JUMP_TICKS: 8,
    AUTO_JUMP_TIME: 9,
    STUCK_SPEED_MULTIPLIER: 10,
    BOUNDING_BOX: 13,
    LENGTH: 19,
} as const;

export const NativePlayerOutputFlag = {
    ON_GROUND: 1 << 0,
    COLLIDED_HORIZONTALLY: 1 << 1,
    MINOR_HORIZONTAL_COLLISION: 1 << 2,
    COLLIDED_VERTICALLY: 1 << 3,
    IN_WATER: 1 << 4,
    IN_LAVA: 1 << 5,
    IN_POWDER_SNOW: 1 << 6,
    IN_WEB: 1 << 7,
} as const;

/**
 * The numbers of one nearby entity in a NativePlayer's entities array: its kind (0 inert, 1 living, 2 boat, 3 shulker,
 * 4 happy ghast), whether it ticks before the player, where the player's move meets it (position, box: min, max) and
 * where its own tick leaves it (position, box).
 */
export const NATIVE_PLAYER_ENTITY_LENGTH = 20;

export interface NativePhysicsBinding {
    NativeWorld: new () => NativeWorld;
    NativePlayer: new (version: string, state: NativeSimulationState, config: NativeSimulationConfig) => NativePlayer;
    simulateTick(
        world: NativeWorld,
        state: NativeSimulationState,
        config: NativeSimulationConfig,
    ): NativeSimulationState;
    createSimulationState(descriptor: NativeEntityDescriptor): NativeSimulationState;
    simulateTicks(
        world: NativeWorld,
        states: NativeSimulationState[],
        ticks: number,
        config: NativeSimulationConfig,
    ): NativeSimulationState[];
    simulateVersionedTick(
        version: string,
        world: NativeWorld,
        state: NativeSimulationState,
        config: NativeSimulationConfig,
    ): NativeSimulationState;
    /**
     * The pushes of the state's movingPistons alone, as simulateVersionedTick applies them after the player's tick: for
     * a caller that learns of the moving blocks only after the tick. The pistons pushed earlier in the same tick moved
     * the player by `pistonDeltas`; pass on the ones returned.
     */
    simulateVersionedPistonPush(
        version: string,
        world: NativeWorld,
        state: NativeSimulationState,
        config: NativeSimulationConfig,
        pistonDeltas?: NativeVec3,
    ): NativePistonPushResult;
    simulateVersionedTicks(
        version: string,
        world: NativeWorld,
        states: NativeSimulationState[],
        ticks: number,
        config: NativeSimulationConfig,
    ): NativeSimulationState[];
    simulateVersionedEntityTicks(
        version: string,
        world: NativeWorld,
        states: NativeVersionedEntityState[],
        ticks: number,
        config: NativeSimulationConfig,
    ): NativeVersionedEntityState[];
    simulateVersionedEntityPush(
        version: string,
        left: NativeEntityPushState,
        right: NativeEntityPushState,
        passengerOfSameVehicle: boolean,
    ): NativeEntityPushResult;
    refreshVersionedPose(
        version: string,
        world: NativeWorld,
        state: NativeSimulationState,
        currentPose: number,
        requestedPose: number,
        spectator: boolean,
    ): NativePoseRefreshResult;
    versionCapabilities(major: number, minor: number, patch: number): NativeVersionCapabilities;
    generatedBlockHasTag(version: string, block: string, tag: string): boolean;
    generatedStateIndex(
        version: string,
        block: string,
        properties: [string, string][],
    ): number | null;
    generatedBlockStateCount(version: string, block: string): number | null;
    generatedFluidStateCount(version: string, fluid: string): number | null;
    generatedFluidStateIndex(
        version: string,
        fluid: string,
        properties: Array<[string, string]>,
    ): number | null;
    generatedBlockDefaults(version: string, block: string): NativeBlockProperty[] | null;
    generatedSourceShapeBoxes(version: string, block: string): NativeGeneratedShapeBox[] | null;
    generatedMethodShapeBoxes(version: string, block: string, kind: "collision" | "outline" | "interaction"): NativeGeneratedShapeBox[] | null;
    generatedExactMethodShapeBoxes(version: string, block: string, kind: "collision" | "outline" | "interaction"): NativeGeneratedShapeBox[] | null;
    generatedExactMethodShapeBoxesForState(version: string, block: string, kind: "collision" | "outline" | "interaction", properties: NativeBlockProperty[]): NativeGeneratedShapeBox[] | null;
    generatedShapePropertyDependencies(version: string, block: string): string[] | null;
    generatedMethodShapePropertyDependencies(version: string, block: string, kind: "collision" | "outline" | "interaction"): string[] | null;
    generatedMethodShapePredicates(version: string, block: string, kind: "collision" | "outline" | "interaction"): NativeGeneratedShapePredicate[] | null;
    generatedMethodShapePredicatesMatchState(version: string, block: string, kind: "collision" | "outline" | "interaction", properties: NativeBlockProperty[]): boolean | null;
    generatedMethodShapeIsStateDependent(version: string, block: string, kind: "collision" | "outline" | "interaction"): boolean | null;
    generatedMethodShapeProvenance(version: string, block: string, kind: "collision" | "outline" | "interaction"): NativeGeneratedShapeProvenance | null;
    simulateBoatTick(state: NativeBoatState): NativeBoatState;
    simulateVersionedBoatTick(version: string, world: NativeWorld, state: NativeBoatState): NativeBoatState;
    simulateVersionedBoatTicks(version: string, world: NativeWorld, states: NativeBoatState[], ticks: number): NativeBoatState[];
    simulateHorseTick(state: NativeHorseState): NativeHorseState;
    simulateVersionedHorseTick(version: string, world: NativeWorld, state: NativeHorseState): NativeHorseState;
    simulateVersionedHorseTicks(version: string, world: NativeWorld, states: NativeHorseState[], ticks: number): NativeHorseState[];
    simulateProjectileTick(
        world: NativeWorld,
        state: NativeProjectileState,
    ): NativeProjectileTickResult;
    simulateVersionedProjectileTicks(
        version: string,
        world: NativeWorld,
        states: NativeProjectileState[],
        ticks: number,
    ): NativeProjectileTickResult[];
    simulateArrowTick(world: NativeWorld, state: NativeArrowState): NativeArrowTickResult;
    simulateVersionedArrowTicks(version: string, world: NativeWorld, states: NativeArrowState[], ticks: number): NativeArrowTickResult[];
    simulateTridentTick(world: NativeWorld, state: NativeTridentState): NativeTridentTickResult;
    simulateVersionedTridentTicks(version: string, world: NativeWorld, states: NativeTridentState[], ticks: number): NativeTridentTickResult[];
}

export interface NativeVersionCapabilities {
    fluidPhysics: boolean;
    honeyBlocks: boolean;
    fallFlying: boolean;
    waterMovementEfficiency: boolean;
    minorHorizontalCollision: boolean;
}

export interface NativeBoatState {
    position: NativeVec3;
    velocity: NativeVec3;
    yaw: number;
    forward: boolean;
    left: boolean;
    right: boolean;
    /** Vanilla `inputDown`; optional for compatibility with older callers. */
    inputDown?: boolean;
    /** Persistent vanilla deltaRotation in degrees. */
    deltaRotation?: number;
    inWater: boolean;
    waterLevel?: number;
    /** The status of the tick before; without one the versioned tick takes it where the boat is. */
    status?: 'in_water' | 'under_water' | 'under_flowing_water' | 'on_land' | 'in_air';
    previousStatus?: 'in_water' | 'under_water' | 'under_flowing_water' | 'on_land' | 'in_air';
    bubbleTime?: number;
    bubbleColumnDirectionIsDown?: boolean;
    aboveBubbleColumn?: boolean;
    /** A player rides and controls the boat (its land friction halves each tick). */
    playerPassenger?: boolean;
    collidedHorizontally?: boolean;
    collidedVertically?: boolean;
    onGround?: boolean;
    /** Vanilla's float yRot in degrees (the versioned tick turns it); `yaw` in radians otherwise. */
    yawDegrees?: number;
    /** Boat.landFriction: the ground friction it last found. */
    landFriction?: number;
    /** Boat.lastYd: the vertical motion its last move saw. */
    lastVerticalVelocity?: number;
    /** The box the boat keeps before 1.17. */
    boundingBox?: NativeAabb;
    /** 1.8's EntityBoat.speedMultiplier: how hard its rider's input pushes it (0.07 to 0.35). */
    speedMultiplier?: number;
    /** 1.8: the rider's yaw (degrees), moveForward and moveStrafing, which push the boat. */
    riderYaw?: number;
    riderForward?: number;
    riderStrafe?: number;
}

export interface NativeHorseState {
    position: NativeVec3;
    velocity: NativeVec3;
    yaw: number;
    forward: number;
    strafe: number;
    jumpRequested: boolean;
    onGround: boolean;
    inWater: boolean;
    movementSpeed: number;
    jumpStrength: number;
    halfWidth?: number;
    height?: number;
    collidedHorizontally?: boolean;
    collidedVertically?: boolean;
    /**
     * The mob a player rides and steers, which the versioned tick moves as vanilla's
     * LivingEntity.travelRidden (a horse when unnamed): pig, strider, camel, camel_husk,
     * happy_ghast, nautilus, zombie_nautilus, horse, donkey, mule, skeleton_horse,
     * zombie_horse, llama, trader_llama. Its rider's xxa and zza after its aiStep of the tick
     * before are `strafe` and `forward`, its jumping `jumpRequested`.
     */
    entityType?: string;
    /** The mob's float yaw and pitch in degrees (the versioned tick turns them). */
    yawDegrees?: number;
    pitchDegrees?: number;
    /** The rider's look this tick, in degrees, and its sprinting. */
    riderYaw?: number;
    riderPitch?: number;
    riderSprinting?: boolean;
    /** The riding jump the rider releases after the mob's tick, in percent of its charge. */
    riderJump?: number;
    inLava?: boolean;
    fallDistance?: number;
    /** The box the mob keeps before 1.17. */
    boundingBox?: NativeAabb;
    /** Where the last tick started, for the block that supports the mob (1.20+). */
    previousPosition?: NativeVec3;
    /** A happy ghast's FLYING_SPEED. */
    flyingSpeed?: number;
    /** Whether the rider steers the mob; one it does not goes on its own input. */
    saddled?: boolean;
    noAi?: boolean;
    /** LivingEntity.speed: the ridden speed of the last tick. */
    riddenSpeed?: number;
    /** The mob's own forward input (LivingEntity.zza). */
    ownForward?: number;
    /** A pig's or strider's boost (ItemBasedSteering). */
    boosting?: boolean;
    boostTime?: number;
    boostTimeTotal?: number;
    /** A strider off the warm blocks. */
    suffocating?: boolean;
    /** A horse's, camel's or nautilus's pending riding jump, and a horse's jump and rearing. */
    jumpPendingScale?: number;
    horseJumping?: boolean;
    standing?: boolean;
    allowStandSliding?: boolean;
    standCounter?: number;
    /** A camel's or nautilus's dash and its cooldown. */
    dashCooldown?: number;
    dashing?: boolean;
    /** A camel sitting, and the ticks since it last sat down or stood up. */
    sitting?: boolean;
    poseTime?: number;
    /** Where this tick's step toward a server teleport's target puts the mob, in place of the fade of its motion. */
    lerpPosition?: NativeVec3;
}

export interface NativeProjectileState {
    position: NativeVec3;
    velocity: NativeVec3;
    gravity?: number;
    airDrag?: number;
    /** Persistent AbstractHurtingProjectile power acceleration. */
    power?: NativeVec3;
    inWater?: boolean;
    firstTick?: boolean;
    /** Optional projectile family tag for source-specialized dispatch. */
    kind?: string;
    /** Source-derived firework horizontal collision flag. */
    horizontalCollision?: boolean;
    /** Source-derived firework shot-at-angle flag. */
    shotAtAngle?: boolean;
}

export interface NativeProjectileTickResult {
    position: NativeVec3;
    velocity: NativeVec3;
    blockHit: boolean;
    firstTick: boolean;
}

export interface NativeArrowState {
    position: NativeVec3;
    velocity: NativeVec3;
    gravity?: number;
    airDrag?: number;
    waterInertia?: number;
    inWater?: boolean;
    inGround?: boolean;
    life?: number;
    inGroundTime?: number;
    shakeTime?: number;
    fallImpulse?: NativeVec3;
    lastBlockId?: number;
}

export interface NativeArrowTickResult {
    position: NativeVec3;
    velocity: NativeVec3;
    blockHit: boolean;
    inGround: boolean;
    discarded: boolean;
    inGroundTime: number;
    shakeTime: number;
    startedFalling: boolean;
}

export interface NativeTridentState {
    position: NativeVec3;
    velocity: NativeVec3;
    gravity?: number;
    airDrag?: number;
    waterInertia?: number;
    inWater?: boolean;
    dealtDamage?: boolean;
}

export interface NativeTridentTickResult {
    position: NativeVec3;
    velocity: NativeVec3;
    blockHit: boolean;
}

export function LoadNativePhysics(): NativePhysicsBinding {
    const modulePath = process.env.NAPI_RS_NATIVE_LIBRARY_PATH ?? '../native/artifacts/native.js';
    try {
        return module.require(modulePath) as NativePhysicsBinding;
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`Native physics addon is unavailable at ${modulePath}: ${message}`);
    }
}
