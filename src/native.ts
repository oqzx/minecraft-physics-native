import { guardWholeNumbers } from './whole-numbers';

export interface NativeVec3 {
    x: number;
    y: number;
    z: number;
}

/** Entity.Movement: a move, checked one axis at a time in the order of `axisOrder` where it has one (1.21.5+). */
export interface NativeMovement {
    from: NativeVec3;
    to: NativeVec3;
    axisOrder?: NativeVec3;
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

/**
 * The boat at the passenger's fluid-query phase. Bounds are absolute world coordinates;
 * underwater means boat status UNDER_WATER or UNDER_FLOWING_WATER, not ordinary IN_WATER.
 * The caller updates this context as the boat moves. Keeping it in a batch does not predict boat/seat movement.
 */
export interface NativeBoatPlacement {
    position: NativeVec3;
    yawDegrees: number;
}

export interface NativeBoatVehicle {
    boundingBox: NativeAabb;
    underwater: boolean;
}

/** getKnockback of the stab's weapon, 0 when not given. */
export interface NativeStabHit {
    knockback?: number;
}

export interface NativeSimulationState {
    position: NativeVec3;
    velocity: NativeVec3;
    yaw?: number;
    /** The float degrees of the yaw, which moveFlying turns into its own radians before 1.9: `yaw` when not given. */
    yawDegrees?: number;
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
    /**
     * The shulker boxes' pushes since the last tick, in order (Entity.movementThisTick, 1.21.5+): a push returns them, and
     * the next tick's block effects take them first. Pass back the list returned.
     */
    pendingMovements?: NativeMovement[];
    sprinting?: boolean;
    /** Sprinting at the end of the previous tick: the airborne speed before 1.19.4 follows it. */
    wasSprinting?: boolean;
    /**
     * Whether the movement speed attribute holds the sprint modifier (LivingEntity.setSprinting adds it, a sync from the
     * server without it drops it); `sprinting` when left out.
     */
    sprintModifier?: boolean;
    inWater?: boolean;
    inLava?: boolean;
    ultraWarm?: boolean;
    isPlayer?: boolean;
    controlledByLocalInstance?: boolean;
    /**
     * Explicit vanilla Entity.canSimulateMovement result for piston bounce/restitution and bubble column pushes in 26.3+; ignored earlier.
     * When absent, players use !clientSide || controlledByLocalInstance and generic AUTHORITATIVE_SIDE entities
     * use controlledByLocalInstance (isLocalInstanceAuthoritative). Supply the result explicitly for
     * SERVER_AND_CLIENT and other movement modes.
     * Other movement-authority decisions are unchanged.
     */
    canSimulateMovement?: boolean;
    /** The entity is no LivingEntity: a slime block or bed returns 0.8 of the bounce (from 1.9), from 26.2 0.8F of the block's restitution. */
    nonLiving?: boolean;
    passenger?: boolean;
    /** Entity.isVehicle: it carries passengers (a server's, as vanilla seats none on a player); from 1.9 nothing pushes it. */
    isVehicle?: boolean;
    /** Boat fluid-query context, used only while passenger is true. Omit or pass null to clear it. */
    boatVehicle?: NativeBoatVehicle | null;
    inWeb?: boolean;
    weaving?: boolean;
    /** Frost Walker on the boots: magma and lit campfires deal no contact damage. */
    frostWalker?: boolean;
    sweetBerryBushImmune?: boolean;
    powderSnowImmune?: boolean;
    powderSnowWalkable?: boolean;
    /** Entity.isInPowderSnow after the tick; given back, it is the tick before's (wasInPowderSnow, 1.21.5+ climbing). */
    inPowderSnow?: boolean;
    /** EntityType.fireImmune: fire never ignites it and it takes no fire contact damage. */
    fireImmune?: boolean;
    /** MobEffects.FIRE_RESISTANCE: LivingEntity.hurt refuses fire damage while the burning goes on. */
    fireResistance?: boolean;
    /** Entity.getSharedFlag(0): burning, as the server last said (Entity.isOnFire reads it on a client). */
    onFireFlag?: boolean;
    /**
     * A spear's stab landed with knockback since the last tick, a hit of the server's copy of the player. The tick takes
     * Player.stabAttack's recoil once, from 1.21.11: 0.6 of the horizontal motion and the sprint, twice over from 26.2 with a
     * weapon `knockback` above 0. A client state ignores it: a client sends STAB and has no recoil of its own, its motion
     * and sprint untouched in recordings (physrec/m_click_spear_stab_*). The caller modelling the server sets it, for
     * either way a stab lands (PiercingWeapon.attack, KineticWeapon.damageEntities) when dealsKnockback held and the
     * target was not deflected; neither a client nor a packet can tell that.
     */
    stabHit?: NativeStabHit;
    /** Entity.isInRain, which the world does not hold: rain at the block position or the box's top puts the fire out. */
    inRain?: boolean;
    /** The highest Fire Protection level worn, which shortens the burning before 1.21. */
    fireProtection?: number;
    /** The BURNING_TIME attribute (1.21+), 1 when not given. */
    burningTime?: number;
    /** Entity.remainingFireTicks; when not given -20 for a player, -1 otherwise up to 1.21.5, 0 from 1.21.6. */
    remainingFireTicks?: number;
    /** The nextInt(1, 3) of every ServerPlayer ignition (1.21.2+), 1 when not given: a fixed value, an approximation of the level's random. */
    serverFireIncrement?: number;
    /** Each ServerPlayer ignition's own nextInt(1, 3) in order, at most 128; the state returned holds those not read. Running out is an error. Not with serverFireIncrement. */
    serverFireIncrements?: number[];
    ticksFrozen?: number;
    ticksRequiredToFreeze?: number;
    tickCount?: number;
    freezeDamage?: number;
    contactDamage?: number;
    /** What this tick's flight into a wall hands hurt, the server's alone: 0 on a client state. */
    wallDamage?: number;
    insideWall?: boolean;
    /** LivingEntity.isSleeping: a sleeping player is never inside a wall. */
    sleeping?: boolean;
    eyeHeight?: number;
    eyeInWater?: boolean;
    eyeInLava?: boolean;
    slowFalling?: boolean;
    levitation?: boolean;
    levitationAmplifier?: number;
    fallDistance?: number;
    safeFallDistance?: number;
    fallDamageImmune?: boolean;
    /** The client's own state: its rules where vanilla's sides differ (fire cleared in baseTick up to 1.21.8, freeze ticks the server's). */
    clientSide?: boolean;
    /** The height the pose update leaves at the end of the player's tick: what ticks after the player meets that box. */
    heightAfterTick?: number;
    /** The kind the state is of: a tick refuses any but "player" (entityTravelSupport). Absent: not named. */
    entityType?: string;
    /** Abilities.invulnerable: a player's fire ticks are at most 1 from 1.16. */
    invulnerable?: boolean;
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
    /** Abilities.flyingSpeed; 0.05F when absent. */
    flyingSpeed?: number;
    climbing?: boolean;
    noPhysics?: boolean;
    noGravity?: boolean;
    /** Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+). */
    removed?: boolean;
    /** Its health is 0 (LivingEntity.isAlive is false): from 1.11 no entity pushes it. */
    dead?: boolean;
    gravity?: number;
    stepHeight?: number;
    /** The movement speed attribute with its additive modifiers; the engine applies sprint, speed and slowness. */
    movementSpeed?: number;
    /** The movement speed attribute's value with every modifier it holds, folded by the caller (movementSpeed unused). */
    movementSpeedValue?: number;
    /** The attribute's parts, where the caller has them: the tick applies its frost modifier and movementSpeedValue is the value after it (1.17 to 1.21.4 on a client). */
    movementSpeedAttribute?: NativeMovementSpeedAttribute;
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
    /**
     * The tick's landing: the amount LivingEntity.causeFallDamage hands hurt, 0 without a landing. Unlike contactDamage and
     * wallDamage this has a value on a client state, a prediction of the server's: the client runs causeFallDamage and
     * computes the same amount for its sounds, and only its hurt does nothing there (26.3 LivingEntity.java:1737-1742,
     * Entity.java:1970-1973; 1.21.3 Entity.java:1605-1608; 1.8.9 LivingEntity.java:864-867, :673). The health is the server's:
     * it counts the fall from the moves it receives, and its hurt then applies armor, Resistance, Feather Falling, the
     * fallDamage game rule and invulnerability, which this amount leaves out. An explosion's or wind charge's impulse
     * context (26.3 LivingEntity.java:1724-1731, from 1.20.5) is not modelled either: a state in one reports the plain fall.
     *
     * One release is the exception, by source: 1.21.4 runs no checkFallDamage for a player in Entity.move on either side
     * (1.21.4 Entity.java:697, PlayerEntity.java:2214), so its client has no landing and reports 0, and only a state with
     * clientSide false reports it, as the server's handleFall of the same move does (Entity.java:1250-1256).
     */
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
     * Entity.mainSupportingBlockPos as the last move left it (1.20+), when supportingBlockKept (absent: no block).
     * Pass back the ones the previous tick returned; without them the block is taken from the position and the
     * previous position.
     */
    supportingBlock?: NativeBlockPosition;
    supportingBlockKept?: boolean;
    /** Entity.onGroundNoBlocks: the last move left the entity on the ground with no block under it. */
    onGroundNoBlocks?: boolean;
    /**
     * Entity.isOutsideBorder before 1.14: the last collision found the entity outside the world border, which it then
     * has to be a block further in to meet. Pass back the one the previous tick returned.
     */
    outsideBorder?: boolean;
    /**
     * The entities near the player this tick, in the order the client ticks them (simulateVersionedTick only): those
     * it ticks before the player push it before its move, the colliders among them stop its move, the others push it
     * after its move.
     */
    nearbyEntities?: NativeNearbyEntity[];
    /**
     * The firework rockets attached to the player that tick this tick (simulateVersionedTick only, 1.11+): each pulls a
     * gliding player toward 1.5 times its look vector (FireworkRocketEntity.tick). These boost at the start of their side
     * of the player; an attached rocket given among nearbyEntities ("attached_rocket") boosts in its turn instead, and is
     * not counted here.
     */
    fireworkRockets?: number;
    /** How many of `fireworkRockets` the client ticks before the player: they boost it before its move. */
    fireworkRocketsBeforePlayer?: number;
    /**
     * How many more client ticks each attached rocket boosts, those before the player first (simulateVersionedTicks:
     * FireworkRocketEntity.tick boosts, then life++, and the rocket ends once life > lifetime, which the server alone
     * knows). One per rocket; it gives `fireworkRockets` when that is left out. Absent: the rockets boost every tick.
     */
    fireworkRocketTicks?: number[];
    /**
     * The blocks pistons move, as they are before this tick (simulateVersionedTick only): the client ticks them after
     * its entities, and each moves the player out of its way. The caller advances them (0.5F a tick).
     */
    movingPistons?: NativeMovingPiston[];
}

export interface NativeMovingPiston {
    /** The boxes of `shapes` once it has gone on this tick, for the pistons that tick after it (absent: the same). */
    nextShapes?: NativeAabb[];
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
    /**
     * The collision boxes of the block it moves (getCollisionRelatedBlockState) in its cell: used from 1.11, and on 1.8
     * those of a block it pushes (none: it pushes nothing).
     */
    shapes: NativeAabb[];
    /**
     * The outline boxes of a block it pushes (not its source) in its cell, whose bounds push on 1.9 - 1.10
     * (Block.getCollisionBox(BlockView), the bounding box); absent or empty: the full cube.
     */
    outlineShapes?: NativeAabb[];
    /** The block it moves is a slime block, which sets the player's motion along the way it moves (causeBounce). */
    bounces?: boolean;
    /** The block it moves is a honey block, which carries the player standing on it sideways (1.15+). */
    stickyForEntities?: boolean;
}

export interface NativeNearbyEntity {
    /**
     * "living" (LivingEntity.pushEntities), "boat" (Boat.tick, a collider), "shulker" (a living collider),
     * "happy_ghast" (a living collider to a player whose feet are at or above its top), "happy_ghast_stays_still" (on
     * its still timeout, a living collider from every side), "still_living" (a living entity that neither pushes nor
     * collides), "minecart" or "attached_rocket" (a firework rocket attached to the player: it boosts a gliding player in its turn of
     * the client's tick, between the pushes of the entities before and after it); other kinds neither push nor collide.
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
    /** False when the team collision rules keep it from pushing the player (EntitySelector.pushableBy). */
    teamPushes?: boolean;
}

/** Which path moves a kind in a release (entityTravelSupport). */
export interface NativeEntityTravelSupport {
    /** "player", "boat", "ridden", "mob" (simulateVersionedMobTravel), "dragon" (simulateVersionedDragonTick), "entity" (no LivingEntity), "unsupported" (the game has it, the engine has no path for it), or "unknown". */
    family: string;
    /** The release's registry holds the kind. */
    available: boolean;
    /** The first release that has it. */
    since?: string;
}

/** A push a nearby entity took back from the player: its index in the state's nearbyEntities, then x and z. */
export interface NativeNearbyEntityPush {
    index: number;
    x: number;
    z: number;
}

export interface NativeTickPhase {
    name: string;
    occurrence: number;
    state: NativeSimulationState;
    /** LivingEntity's xxa, yya and zza at beforeTravel. */
    travelVector?: NativeVec3;
}

/** The head or the tail of a vehicle's move inside its tick. */
export interface NativeVehicleMove {
    name: string;
    position: NativeVec3;
    velocity: NativeVec3;
}

export interface NativeTracedBoatTick {
    state: NativeBoatState;
    moves: NativeVehicleMove[];
}

export interface NativeTracedHorseTick {
    state: NativeHorseState;
    moves: NativeVehicleMove[];
}

export interface NativeTracedTick {
    state: NativeSimulationState;
    phases: NativeTickPhase[];
}

export interface NativeTickOutcome {
    state: NativeSimulationState;
    entityPushes: NativeNearbyEntityPush[];
    autoSpinAttackEnded: boolean;
    pistonDeltas: NativeVec3;
}

export interface NativePistonPushResult {
    state: NativeSimulationState;
    /** How far the pistons of the tick have moved the player on each axis (Entity.pistonDeltas). */
    pistonDeltas: NativeVec3;
}

/** One side of simulateVersionedEntityPush: the left one runs Entity.push(Entity) (`this`), the right one is its argument. */
export interface NativeEntityPushState {
    position: NativeVec3;
    velocity: NativeVec3;
    noPhysics: boolean;
    /** It has passengers (Entity.isVehicle): it takes no push. */
    vehicle: boolean;
    /** Entity.isPushable: from 1.19 a side that is not takes no push. */
    pushable: boolean;
    /** Entity.pushthrough before 1.17: the left side's scales both pushes; the right side's is unused. */
    pushthrough: number;
}

export interface NativeEntityPushResult {
    leftVelocity: NativeVec3;
    rightVelocity: NativeVec3;
}

/**
 * A living entity of simulateVersionedEntityTicks. Each runs its whole tick in the order given: its move, then the push
 * of every other pushable entity whose box meets its own (LivingEntity.pushEntities -> Entity.push).
 */
export interface NativeVersionedEntityState {
    simulation: NativeSimulationState;
    noPhysics: boolean;
    /** Its id, which other entities' vehicleId name. */
    id?: number;
    /** The id of the entity it rides. */
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
    boatVehicle?: NativeBoatVehicle | null;
    /** Explicit vanilla 26.3+ canSimulateMovement result for piston bounce/restitution and bubble column pushes; see NativeSimulationState. */
    canSimulateMovement?: boolean;
    /** The entity is no LivingEntity; see NativeSimulationState. */
    nonLiving?: boolean;
    flying?: boolean;
    /** Abilities.flyingSpeed; 0.05F when absent. */
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
    jumpRequested?: boolean;
    jumpTicks?: number;
    noPhysics?: boolean;
    waterMovementEfficiency?: number;
    movementEfficiency?: number;
    airDragModifier?: number;
    frictionModifier?: number;
}

/** What vanilla's EntityCollisionContext tells a block about the entity asking. */
export interface NativeCollisionContext {
    bottom: number;
    descending: boolean;
    fallDistance: number;
    walksOnPowderSnow: boolean;
    /** The entity is a falling block, which powder snow holds up wherever its feet are; absent: false. */
    fallingBlock?: boolean;
}

export interface NativeWorld {
    /** The whole world as bytes, for nativeWorldFromCheckpoint: equal worlds give equal bytes. */
    checkpoint(): Buffer;
    /** What names this world's content in a checkpoint. */
    contentDigest(): string;
    /** The release whose client this world is: what a chunk it does not hold reads of the shared empty chunk (1.8 its blocks, 1.13 their fluids). */
    setRelease(version: string): void;
    setChunkLoaded(chunkX: number, chunkZ: number, loaded: boolean): void;
    isChunkLoaded(chunkX: number, chunkZ: number): boolean;
    /** MovingPistonBlock.getCollisionShape: the piston's cell takes the shape its block entity gives at its progress. */
    /** Returns the boxes the cell collides with, relative to it (free to leave it). */
    setMovingBlock(version: string, piston: NativeMovingPiston, baseShapes?: NativeAabb[] | null): NativeAabb[];
    /** The moving block's block entity ended: the block has no shape until it is written anew. */
    clearMovingBlock(x: number, y: number, z: number): void;
    setWorldBounds(minBuildHeight: number, maxBuildHeight: number, borderCenterX: number, borderCenterZ: number, borderSize: number, borderAbsoluteMaxSize?: number): void;
    worldBounds(): NativeWorldBounds;
    /** WorldBorder.isWithinBounds(x, z) of the release (the newest when none is given). */
    isInsideWorldBorder(x: number, z: number, version?: string): boolean;
    /** CollisionGetter.noBorderCollision from 1.14: whether the border leaves the box free for an entity at x, z with box `bounds`. */
    noBorderCollision(min: NativeVec3, max: NativeVec3, boundsMin: NativeVec3, boundsMax: NativeVec3, x: number, z: number, version?: string): boolean;
    isInsideBuildHeight(y: number): boolean;
    isSectionPresent(chunkX: number, sectionY: number, chunkZ: number): boolean;
    setSectionPresent(chunkX: number, sectionY: number, chunkZ: number, present: boolean): void;
    sectionBlockCount(chunkX: number, sectionY: number, chunkZ: number): number;
    /** WorldBorder.clampVec3ToBound from 1.21.2, before it clampToBounds (a block corner), of the release (the newest when none is given). */
    clampToWorldBorder(position: NativeVec3, version?: string): NativeVec3;
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
    /** The boxes in the query, its cells by the Mth.floor and Mth.ceil of the release (the newest when none is given). */
    collisionShapes(min: NativeVec3, max: NativeVec3, version?: string): NativeAabb[];
    entityCollisionShapes(min: NativeVec3, max: NativeVec3, context: NativeCollisionContext, version?: string): NativeAabb[];
    outlineShapes(min: NativeVec3, max: NativeVec3, version?: string): NativeAabb[];
    interactionShapes(min: NativeVec3, max: NativeVec3, version?: string): NativeAabb[];
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

export type NativeShapeChannel = 'collision' | 'outline' | 'interaction' | 'support';

/**
 * A channel of a block state as the release's own code answers it. `exact`: the boxes are the answer, no box being an
 * answer too. `contextual`: it also needs what dependsOn lists, so there are no boxes to use. `absent`: the release has
 * no such method.
 */
export interface NativeGeneratedStateShape {
    status: 'exact' | 'contextual' | 'absent';
    dependsOn: Array<'position' | 'level' | 'entity'>;
    boxes: NativeGeneratedShapeBox[];
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

export interface NativeAttributeModifier {
    amount: number;
    /** add_value, add_multiplied_base or add_multiplied_total. */
    operation: string;
}

/** The movement speed attribute: its base, its modifiers but the powder snow one, and that one's amount if it holds it. */
export interface NativeMovementSpeedAttribute {
    base: number;
    modifiers: NativeAttributeModifier[];
    frost?: number;
}

export interface NativeBlockUpdate {
    x: number;
    y: number;
    z: number;
    id?: number;
    stateVersion?: string;
    blockEntity?: string;
    removeBlockEntity?: boolean;
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
    /** Blocks.WATER itself (not a waterlogged block or a water plant): its speed factor stands, the block below is not read. */
    waterBlock?: boolean;
    bounceRestitution?: number;
    suppressesBounce?: boolean;
    fallDamageResetting?: boolean;
    /** The end gateway and end portal: full cubes for a player's fall-damage-resetting clip from 1.21.9. */
    playerFallDamageResetting?: boolean;
    /** BlockState.isSuffocating when it is not "a full collision cube" (glass, leaves: false). */
    suffocating?: boolean;
    /** Entity.isInsideWall's test of the block before 1.13 when it is not `suffocating` (isNormalCube). */
    inWall?: boolean;
    /** BlockState.isFaceSturdy(DOWN), by the support shape; the collision boxes' bottom face when not given. */
    bottomFaceSturdy?: boolean;
    /** Block.getExplosionResistance of the block, absent for air: what a blast's rays lose in it. */
    explosionResistance?: number;
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
    /**
     * `tick` up to Player.tick's pose update: the entities before the player and its move. The caller then chooses the
     * pose where the move left the player, and `finishTick` runs what ticks after it with that pose's box.
     */
    tickMove(world: NativeWorld, input: Float64Array, entities: Float64Array): Float64Array;
    /** The rockets and entities after the player in a tick `tickMove` started, with the box `height` tall the pose left. */
    finishTick(world: NativeWorld, height: number): Float64Array;
    /** What an opening shulker box's lid at a cell does to the player, after its tick. */
    pushByShulkerBox(world: NativeWorld, shulker: NativeShulkerBox, x: number, y: number, z: number): Float64Array;
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
    /** The float yaw in degrees that YAW holds in radians (moveFlying turns it into radians itself before 1.9). */
    YAW_DEGREES: 28,
    /** The look along the yaw the last aiStep left in yHeadRot, with this tick's pitch, three numbers: what a 1.11
     * rocket that ticks before the player boosts along. NaN: LOOK's. */
    HEAD_LOOK: 29,
    LENGTH: 32,
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
    /** The riptide spin attack runs this tick (LivingEntity.autoSpinAttackTicks > 0, 1.13+). */
    AUTO_SPIN_ATTACK: 1 << 22,
    /** In a bed (LivingEntity.isSleeping; Player.isSleeping before 1.14). */
    SLEEPING: 1 << 23,
    /** Its health is 0 (LivingEntity.isAlive is false). */
    DEAD: 1 << 24,
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
    /** The damage of the tick's landing (LivingEntity.causeFallDamage's amount), 0 without one. */
    FALL_DAMAGE: 19,
    /** What the tick's blocks hand hurt, added up over its contacts: the server's alone, 0 on a client state and on the next tick. */
    CONTACT_DAMAGE: 20,
    /** What the tick's flight into a wall hands hurt: the server's alone, 0 on a client state and on the next tick. */
    WALL_DAMAGE: 21,
    /** The freeze damage dealt so far, a running count, the server's alone: the amount of a tick is its rise. */
    FREEZE_DAMAGE: 22,
    /** The powder snow modifier the movement speed attribute holds after the tick's frost update (the client's too up to 1.21.4), NaN with none or with no attribute in the state. */
    FROST: 23,
    LENGTH: 24,
} as const;

/** What a build's packed arrays are; a caller that keeps them apart from it compares this (nativePlayerLayout). */
export const NATIVE_PLAYER_LAYOUT: NativePlayerLayout = { version: 3, inputLength: 32, outputLength: 24, entityLength: 21 };

export interface NativePlayerLayout {
    version: number;
    inputLength: number;
    outputLength: number;
    entityLength: number;
}

export const NativePlayerOutputFlag = {
    ON_GROUND: 1 << 0,
    COLLIDED_HORIZONTALLY: 1 << 1,
    MINOR_HORIZONTAL_COLLISION: 1 << 2,
    COLLIDED_VERTICALLY: 1 << 3,
    IN_WATER: 1 << 4,
    IN_LAVA: 1 << 5,
    IN_POWDER_SNOW: 1 << 6,
    IN_WEB: 1 << 7,
    /** Still fall flying after the tick: flight and, from 1.21.5, a climbable block stop a glide. */
    FALL_FLYING: 1 << 8,
    /** The riptide spin attack ended in the tick: it hit a living entity, or touching none a wall. */
    AUTO_SPIN_ATTACK_ENDED: 1 << 9,
    /** The tick's move was slowed by the stuck speed multiplier (Entity.isInWeb before 1.14). */
    MOVE_SLOWED: 1 << 10,
    /** The tick's move ran Block.updateEntityMovementAfterFallOn, once a move, whatever it hit. */
    BLOCK_FALL_CALLBACK: 1 << 11,
    EYE_IN_WATER: 1 << 12,
    /** Entity.verticalCollisionBelow after the move, which the input array takes back. */
    VERTICAL_COLLISION_BELOW: 1 << 13,
} as const;

/**
 * The numbers of one nearby entity in a NativePlayer's entities array: its kind (0 inert, 1 living, 2 boat, 3 shulker,
 * 4 happy ghast, 5 a living entity that neither pushes nor collides, 6 a happy ghast on its still timeout, 7 a firework
 * rocket attached to the player, boosting it in its turn), whether it
 * ticks before the player, where the player's move meets it (position, box: min, max), where its own tick leaves it
 * (position, box), and its flags (NativePlayerEntityFlag).
 */
export const NATIVE_PLAYER_ENTITY_LENGTH = 21;

/** The bits of a nearby entity's flags, the last number of its entry in the entities array. */
export const NativePlayerEntityFlag = {
    /** The team collision rules keep it from pushing the player (EntitySelector.pushableBy). */
    NO_TEAM_PUSH: 1 << 0,
    /** A spectating player, which the riptide spin's sweep leaves out (getEntities: NO_SPECTATORS). */
    SPECTATOR: 1 << 1,
} as const;

/**
 * A bulk simulation the engine holds between calls, so that it can run a chunk at a time (runBulk, src/bulk.ts). It keeps
 * what a returned state cannot carry, a player's scene and a projectile's base-tick state, so chunks and one long call
 * are the same ticks. Each chunk ends on a whole tick of every entity.
 */
export interface NativeBulkRun {
    /** The entity ticks a chunk may take; a chunk of one tick is always allowed. */
    readonly maxChunkWork: number;
    readonly entityCount: number;
    /** The whole ticks run so far. */
    readonly ticksDone: number;
    /** Whether the run holds its states: dispose and a failed chunk both end that. */
    readonly alive: boolean;
    /** `ticks` more whole ticks in `world`, the same world each time. An error in them finalizes the run. */
    advance(world: NativeWorld, ticks: number): void;
    /** Frees the states; anything after this refuses. */
    dispose(): void;
    /** Each player with the scene it has reached, fit to start a new run or a tick. */
    playerStates(): NativeSimulationState[];
    entityStates(): NativeVersionedEntityState[];
    boatStates(): NativeBoatState[];
    horseStates(): NativeHorseState[];
    /** The last tick's results, empty before one, as the simulateVersioned*Ticks of these families give. */
    projectileResults(): NativeProjectileTickResult[];
    arrowResults(): NativeArrowTickResult[];
    tridentResults(): NativeTridentTickResult[];
}

export interface NativeBulkRunFactory {
    players(version: string, states: NativeSimulationState[], config: NativeSimulationConfig): NativeBulkRun;
    entities(version: string, states: NativeVersionedEntityState[], config: NativeSimulationConfig): NativeBulkRun;
    boats(version: string, states: NativeBoatState[]): NativeBulkRun;
    horses(version: string, states: NativeHorseState[]): NativeBulkRun;
    projectiles(version: string, states: NativeProjectileState[]): NativeBulkRun;
    arrows(version: string, states: NativeArrowState[]): NativeBulkRun;
    tridents(version: string, states: NativeTridentState[]): NativeBulkRun;
}

/** What a loaded binary says it is (native/build.rs). */
export interface NativeBuildIdentity {
    /** The Rust target it was compiled for. */
    target: string;
    /** The hash of the sources it was compiled from, as native/artifacts/provenance.*.json records it. */
    sourceHash: string;
    crateVersion: string;
}

/** A shulker box's lid: `status` is 0 closed, 1 opening, 2 opened, 3 closing; `facing` a Direction's ordinal. */
export interface NativeShulkerBox {
    facing: number;
    status: number;
    progress: number;
    progressOld: number;
    /** The progress a push reads before 1.17: `progress` when not given. */
    pushProgress?: number;
}

export interface NativeShulkerBoxTick {
    state: NativeShulkerBox;
    /** The cell's collision box, at the cell's origin. */
    bounds: NativeAabb;
    /** Whether the lid pushes what it meets this tick (simulateVersionedShulkerBoxPush). */
    pushes: boolean;
}

export interface NativePhysicsBinding {
    buildIdentity(): NativeBuildIdentity;
    /** Every release on one Java major's Math.sin and Math.cos, or with null each on the one it declares (Java 8 up to 1.16.5). Process-wide. */
    setMathJava?(major: number | null): void;
    /** The Java major whose Math a release follows now. */
    mathJava?(version: string): number;
    /** A shulker box's block event, the server's (triggerEvent(1, count)). */
    shulkerBoxBlockEvent(version: string, state: NativeShulkerBox, count: number): NativeShulkerBox;
    /** One tick of a shulker box's block entity. */
    simulateVersionedShulkerBoxTick(version: string, state: NativeShulkerBox): NativeShulkerBoxTick;
    /** What an opening lid does to a state at the cell's edge. */
    simulateVersionedShulkerBoxPush(version: string, world: NativeWorld, state: NativeSimulationState, shulker: NativeShulkerBox, x: number, y: number, z: number): NativeSimulationState;
    NativeWorld: new () => NativeWorld;
    NativeBulkRun: NativeBulkRunFactory;
    /** A world from the bytes of NativeWorld.checkpoint: a new world. */
    nativeWorldFromCheckpoint(bytes: Buffer): NativeWorld;
    nativePlayerLayout(): NativePlayerLayout;
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
     * simulateVersionedTick with its phase trace: the same tick, and a copy of the state at each point it passed, by the
     * names of the recorder's schema/phases.ts. A trace only reads: the state it gives is simulateVersionedTick's, to the bit.
     */
    simulateVersionedTickTraced(
        version: string,
        world: NativeWorld,
        state: NativeSimulationState,
        config: NativeSimulationConfig,
    ): NativeTracedTick;
    /**
     * simulateVersionedTick's tick with all it writes: the state carrying the scene the next tick reads (so it can be
     * passed back as the next tick's state), the pushes the nearby entities took back, whether the spin attack ended and
     * the piston deltas.
     */
    simulateVersionedTickOutcome(
        version: string,
        world: NativeWorld,
        state: NativeSimulationState,
        config: NativeSimulationConfig,
    ): NativeTickOutcome;
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
    /**
     * `ticks` simulateVersionedTick calls for each state, every tick in the scene the state brings: its nearbyEntities,
     * fireworkRockets and movingPistons. The pistons' progress goes on 0.5 a tick up to 1, and before 1.11 a piston
     * leaves after its ending tick; the rockets given fireworkRocketTicks end when their ticks run out, the others and
     * the entities stay as they came.
     */
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
    /** The engine's inventory of the kinds that move through fluids or are steered through them. */
    entityTravelSupport(version: string, entityType: string): NativeEntityTravelSupport;
    simulateVersionedEntityPush(
        version: string,
        left: NativeEntityPushState,
        right: NativeEntityPushState,
        passengerOfSameVehicle: boolean,
    ): NativeEntityPushResult;
    /**
     * Player.updatePlayerPose (1.14+): the requested pose when it fits, else crouching or swimming (id 3), and no change
     * without room to swim. The fits meet the blocks in the player's collision context and the state's colliding
     * nearbyEntities. Pose id 6 is DYING before 1.17, LONG_JUMPING from 1.17 (DYING 7); `scale` (the SCALE attribute)
     * sizes the pose from 1.20.5.
     */
    /** Whether a block's collision box meets a query as noCollision takes it; `block`: the box is the block's whole shape, a full cube. */
    livingKnockback(version: string, velocity: NativeVec3, onGround: boolean, strength: number, x: number, z: number, resistance: number, roll?: number): NativeVec3 | null;
    collisionBoxMeetsQuery(version: string, min: NativeVec3, max: NativeVec3, block: boolean, queryMin: NativeVec3, queryMax: NativeVec3): boolean;
    refreshVersionedPose(
        version: string,
        world: NativeWorld,
        state: NativeSimulationState,
        currentPose: number,
        requestedPose: number,
        spectator: boolean,
        scale?: number,
    ): NativePoseRefreshResult;
    versionCapabilities(major: number, minor: number, patch: number): NativeVersionCapabilities;
    /**
     * Whether the engine has a module for the release (simulateVersioned* and NativePlayer refuse the others). Absent
     * from bindings built before it.
     */
    isSupportedVersion?(version: string): boolean;
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
    /** A channel of a block state as the release's own code answers it, by the state's index among the block's. */
    generatedStateShape(version: string, block: string, kind: NativeShapeChannel, index: number): NativeGeneratedStateShape | null;
    /** The same by the state's properties: null for a tuple that is not exactly one value of each. */
    generatedStateShapeForProperties(version: string, block: string, kind: NativeShapeChannel, properties: NativeBlockProperty[]): NativeGeneratedStateShape | null;
    generatedShapePropertyDependencies(version: string, block: string): string[] | null;
    generatedMethodShapePropertyDependencies(version: string, block: string, kind: "collision" | "outline" | "interaction"): string[] | null;
    generatedMethodShapePredicates(version: string, block: string, kind: "collision" | "outline" | "interaction"): NativeGeneratedShapePredicate[] | null;
    generatedMethodShapePredicatesMatchState(version: string, block: string, kind: "collision" | "outline" | "interaction", properties: NativeBlockProperty[]): boolean | null;
    generatedMethodShapeIsStateDependent(version: string, block: string, kind: "collision" | "outline" | "interaction"): boolean | null;
    generatedMethodShapeProvenance(version: string, block: string, kind: "collision" | "outline" | "interaction"): NativeGeneratedShapeProvenance | null;
    simulateBoatTick(state: NativeBoatState): NativeBoatState;
    simulateVersionedBoatTick(version: string, world: NativeWorld, state: NativeBoatState): NativeBoatState;
    /** The tick with the head and tail of each move inside it. */
    simulateVersionedBoatTickTraced(version: string, world: NativeWorld, state: NativeBoatState): NativeTracedBoatTick;
    simulateVersionedBoatTicks(version: string, world: NativeWorld, states: NativeBoatState[], ticks: number): NativeBoatState[];
    /** A server tick of one cart. The rules say which path: a release alone does not. */
    simulateVersionedMinecartTick(version: string, world: NativeWorld, state: NativeMinecartState, rules: NativeMinecartRules): NativeMinecartTick;
    /** A server tick of a level's carts in its tick order, among the other entities near them. */
    simulateVersionedMinecartScene(version: string, world: NativeWorld, carts: NativeMinecartState[], others: NativeMinecartOther[], rules: NativeMinecartRules, items?: NativeMinecartItem[] | null, containers?: NativeMinecartContainer[] | null, gateways?: NativeMinecartGateway[] | null): NativeMinecartScene;
    /** VehicleEntity.hurtServer of what comes from outside a cart's tick. */
    simulateVersionedMinecartHurt(version: string, world: NativeWorld, cart: NativeMinecartState, hurt: NativeMinecartHurt, rules: NativeMinecartRules): NativeMinecartTick;
    /** One client tick of what a client counts down of a cart: never what the server decides. */
    simulateMinecartClientEffects(state: NativeMinecartClientEffects): NativeMinecartClientEffects;
    /** AbstractMinecart.push(Entity): a cart an entity's own tick pushes, and what that entity takes. */
    simulateVersionedMinecartPush(version: string, cart: NativeMinecartState, other: NativeMinecartOther): NativeMinecartScene;
    /** The steps a client's interpolation of a cart starts with under the old path. */
    minecartClientLerpSteps(version: string): number;
    /** One client tick of the old path's interpolation: it reads no world. */
    simulateMinecartClientLerp(version: string, state: NativeMinecartLerp): NativeMinecartLerp;
    /** One client tick of the improvements' interpolation: it reads no world. */
    simulateMinecartClientSteps(state: NativeMinecartClientSteps): NativeMinecartClientSteps;
    simulateHorseTick(state: NativeHorseState): NativeHorseState;
    simulateVersionedHorseTick(version: string, world: NativeWorld, state: NativeHorseState): NativeHorseState;
    /** The tick with the head and tail of each move inside it. */
    simulateVersionedHorseTickTraced(version: string, world: NativeWorld, state: NativeHorseState): NativeTracedHorseTick;
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
    /**
     * One tick of a primed TNT, falling block, item or experience orb. It changes neither the world nor any other entity: the
     * result says what the server would do, and a client's says nothing.
     */
    /**
     * The moving blocks of one game tick against what is no player, after its own tick. `pistons` in the order they tick; leave out
     * what vanilla does not push (PushReaction.IGNORE_ENTITY: a marker armor stand, displays and the like).
     */
    simulateVersionedEntityPistonPush(
        version: string,
        world: NativeWorld,
        entity: NativePistonPushedEntity,
        pistons: readonly NativeMovingPiston[],
    ): NativePistonPushedResult;
    /** Where a boat as its tick left it seats the player in front, of `passengers` (absent: alone), and the yaw it leaves it with. */
    boatPassengerPlacement(version: string, state: NativeBoatState, passengerYawDegrees: number, chest?: boolean | null, raft?: boolean | null, passengers?: number | null): { position: NativeVec3; yawDegrees: number };
    simulateVersionedMovingEntityTick(
        version: string,
        world: NativeWorld,
        state: NativeMovingEntityState,
        inputs?: NativeMovingEntityInputs | null,
    ): NativeMovingEntityTickResult;
    /** One LivingEntity.travel of a squid, fish, guardian, ghast, phantom, allay or vex. Its AI, before and after, is the caller's. */
    simulateVersionedMobTravel(version: string, world: NativeWorld, state: NativeMobTravelState): NativeMobTravelState;
    simulateVersionedDragonTick(version: string, state: NativeDragonState): NativeDragonTickResult;
    dragonWingPushesForVersion(version: string, parts: NativeDragonPart[], targets: NativeDragonPushTarget[]): NativeDragonPush[];
    simulateArrowTick(world: NativeWorld, state: NativeArrowState): NativeArrowTickResult;
    simulateVersionedArrowTicks(version: string, world: NativeWorld, states: NativeArrowState[], ticks: number): NativeArrowTickResult[];
    simulateTridentTick(world: NativeWorld, state: NativeTridentState): NativeTridentTickResult;
    simulateVersionedTridentTicks(version: string, world: NativeWorld, states: NativeTridentState[], ticks: number): NativeTridentTickResult[];
    /** One server tick of an arrow among targets, in Level.getEntities order; owner is what Projectile.getOwner gives. */
    /** One server tick of a family other than the arrows among `targetsInOrder`. */
    simulateVersionedProjectileHitTick(
        version: string,
        world: NativeWorld,
        state: NativeProjectileState,
        hits: NativeProjectileHitState | undefined | null,
        targetsInOrder: NativeProjectileTarget[],
        owner?: NativeProjectileOwner | null,
    ): NativeProjectileHitTick;
    simulateVersionedArrowHitTick(
        version: string,
        world: NativeWorld,
        state: NativeArrowState,
        hits: NativeProjectileHitState | undefined | null,
        targetsInOrder: NativeProjectileTarget[],
        projectileOwner?: NativeProjectileOwner | null,
    ): NativeArrowHitTick;
    /** One server tick of a trident among targets, its return to the owner included. */
    simulateVersionedTridentHitTick(
        version: string,
        world: NativeWorld,
        state: NativeTridentState,
        hits: NativeProjectileHitState | undefined | null,
        targetsInOrder: NativeProjectileTarget[],
        projectileOwner?: NativeProjectileOwner | null,
    ): NativeTridentHitTick;
    /** EntityDimensions.makeBoundingBox of a pose's width and height, scaled. */
    projectileTargetHitbox(position: NativeVec3, width: number, height: number, scale?: number | null): NativeAabb;
}

export interface NativeVersionCapabilities {
    fluidPhysics: boolean;
    honeyBlocks: boolean;
    fallFlying: boolean;
    waterMovementEfficiency: boolean;
    minorHorizontalCollision: boolean;
}

/** What a level's enabled features select: required, as a release alone does not say. */
export interface NativeMinecartRules {
    /** The minecart_improvements feature, which 1.21.2+ can have enabled. */
    improvements: boolean;
    /** The game rule, in blocks a second: required with the improvements. */
    maxMinecartSpeed?: number | null;
    /** Level.getMinY: required of a cart under -64 from 1.18, where it decides whether the cart is gone. */
    minY?: number | null;
    /** The game rule of 1.21.5+. */
    tntExplodes?: boolean | null;
    /** Whether a nether portal has a portal of the level's beyond it: only a player's makes one. True when not given. */
    netherExit?: boolean | null;
    /** The spawnerBlocksEnabled game rule of 1.21.9+, which a spawner cart's spawner runs under. True when not given. */
    spawnerBlocksEnabled?: boolean | null;
    /** Level.random's raw 48 bits, where the caller knows them: a TNT cart's blast then gives the exact blocks it breaks. */
    levelRandom?: number | null;
}

/**
 * An end gateway's block entity, which no client can see into: its block, the block it sends to (the exit portal under
 * ExactTeleport, else findExitPosition's; none where it has none), its teleportCooldown and age. Before 1.21 its own tick
 * takes what is in it, from 1.21 a cart's move does. `undecided` comes back set where the level's random picked among
 * entities on different carts, and none was moved.
 */
export interface NativeMinecartGateway {
    position: NativeBlockPosition;
    exit?: NativeBlockPosition | null;
    cooldown?: number | null;
    age?: number | null;
    undecided?: boolean | null;
}

/** A stack in a hopper cart, an item entity or a container: `item` is any name the same for stacks that merge. */
export interface NativeMinecartStack {
    item: string;
    count: number;
    maxStack: number;
}

/** An item entity a hopper cart can take. */
export interface NativeMinecartItem {
    boundingBox: NativeAabb;
    stack: NativeMinecartStack;
    /** Entity.position: its box's bottom centre when not given. */
    position?: NativeVec3 | null;
    velocity?: NativeVec3 | null;
    /** ItemEntity.health: 5 when not given. */
    health?: number | null;
    /** A blast's hurt is kept off it: an invulnerable item, a nether star, a stack resistant to explosions. */
    blastProof?: boolean | null;
    /** Comes back set where a blast's hurt took its health. */
    removed?: boolean | null;
}

/** A block's container a hopper cart can take from: the slots open to it, in order. */
export interface NativeMinecartContainer {
    x: number;
    y: number;
    z: number;
    slots: NativeMinecartStack[];
}

/** A hurt from outside a cart's tick. */
export interface NativeMinecartHurt {
    /** `plain`, `fire` or `explosion`. */
    kind: string;
    amount: number;
    creative?: boolean | null;
    /** The burning arrow's velocity's lengthSqr, when one is the direct entity. */
    burningArrow?: number | null;
}

export interface NativeMinecartExplosion {
    position: NativeVec3;
    power: number;
    /** The blocks the rays break under any of the level's draws, air included, as the explosion's toBlow has them. */
    certain: NativeBlockPosition[];
    /** The blocks they break under some draw. */
    possible: NativeBlockPosition[];
    /** The exact list, where the rules gave levelRandom. */
    blown?: NativeBlockPosition[] | null;
    /** Level.random after the rays' draws, with blown. */
    levelRandom?: number | null;
}

/** What a client's tick counts down of a cart: the server's numbers, between two of its packets. */
export interface NativeMinecartClientEffects {
    hurtTime: number;
    damage: number;
    fuse: number;
}

/** A cart's first passenger: which field counts depends on the release. */
export interface NativeMinecartRider {
    player: boolean;
    living?: boolean | null;
    /** Its forward input, read before 1.14. */
    forward?: number | null;
    /** Its float yaw in degrees, read before 1.14. */
    yaw?: number | null;
    /** Its velocity on the server, read 1.14 - 1.21.1. */
    velocity?: NativeVec3 | null;
    /** ServerPlayer.getLastClientMoveIntent, read from 1.21.2. */
    moveIntent?: NativeVec3 | null;
}

/** A cart as the server keeps it. `variant` is its entity name. */
export interface NativeMinecartState {
    variant: string;
    position: NativeVec3;
    velocity: NativeVec3;
    yRot: number;
    xRot?: number | null;
    flipped: boolean;
    onRails?: boolean | null;
    onGround: boolean;
    firstTick?: boolean | null;
    inWater: boolean;
    collidedHorizontally?: boolean | null;
    collidedVertically?: boolean | null;
    stuckSpeedMultiplier?: NativeVec3 | null;
    noGravity?: boolean | null;
    ultraWarm?: boolean | null;
    fallDistance?: number | null;
    boundingBox?: NativeAabb | null;
    rider?: NativeMinecartRider | null;
    fuel?: number | null;
    push?: NativeVec3 | null;
    comparatorSignal?: number | null;
    hasLootTable?: boolean | null;
    /** Entity.hasCustomName: before 1.20.3 a creative hit destroys a named cart instead of discarding it. */
    customName?: boolean | null;
    tickCount?: number | null;
    hurtTime?: number | null;
    hurtDir?: number | null;
    damage?: number | null;
    invulnerable?: boolean | null;
    /** MinecartTNT.fuse, -1 until it is primed. */
    fuse?: number | null;
    /** From 1.21.5, MinecartTNT.ignitionSource has a creative player for its attacker: its blast is that player's. */
    creativeIgnition?: boolean | null;
    explosionPowerBase?: number | null;
    explosionSpeedFactor?: number | null;
    /** The raw 48 bits of Entity.random, required of a TNT cart: no client knows it, so it is given, and comes back as the tick left it. */
    randomSeed?: number | null;
    enabled?: boolean | null;
    /** A hopper cart's five slots, required of one and refused of any other: the empty ones null. */
    inventory?: (NativeMinecartStack | undefined | null)[] | null;
    lastActivated?: number | null;
    /** A spawner cart's BaseSpawner: Delay (20 without it), MinSpawnDelay (200), MaxSpawnDelay (800), RequiredPlayerRange (16). */
    spawnDelay?: number | null;
    minSpawnDelay?: number | null;
    maxSpawnDelay?: number | null;
    requiredPlayerRange?: number | null;
    /** `nether`, `end` or `gateway`, the portal it is in. */
    portal?: string | null;
    /** The block of the gateway it went into, required of a `gateway` portal. */
    portalEntry?: NativeBlockPosition | null;
    insidePortal?: boolean | null;
    portalTime?: number | null;
    portalCooldown?: number | null;
    /** Set by the tick: `destroyed`, `discarded`, `exploded`, `below_world`, `nether` or `end`. */
    removed?: string | null;
    /** The scoreboard team it is on, whose collision rule the pushes ask from 1.9. */
    team?: NativeMinecartTeam | null;
}

/** A scoreboard team: its name (the same name is the same team) and Team.CollisionRule: always, never, pushOtherTeams or pushOwnTeam. */
export interface NativeMinecartTeam {
    name: string;
    collisionRule: string;
    /** PlayerTeam.allowFriendlyFire, true without it: from 26.3 a TNT cart's blast hurts its allies only with it. */
    allowFriendlyFire?: boolean | null;
}

/** An entity near the carts that is not one of them. */
export interface NativeMinecartOther {
    position: NativeVec3;
    boundingBox: NativeAabb;
    velocity?: NativeVec3 | null;
    pushable?: boolean | null;
    solid?: boolean | null;
    player?: boolean | null;
    ironGolem?: boolean | null;
    living?: boolean | null;
    passenger?: boolean | null;
    hasPassengers?: boolean | null;
    /** Set by the tick: the index of the cart that picked it up. */
    rides?: number | null;
    /** The index among the scene's others of the entity it rides, which rides a cart (a jockey): from 1.9 nothing of that cart's pushes or collides with it (isPassengerOfSameVehicle). */
    ridesOther?: number | null;
    /** AbstractBoat: its tick from 1.9 pushes the carts it reaches. */
    boat?: boolean | null;
    /** Where its own tick leaves it, from where it pushes: with tickBoundingBox, for what a cart ticking ahead of it finds elsewhere. */
    tickPosition?: NativeVec3 | null;
    tickBoundingBox?: NativeAabb | null;
    /** How many of the scene's carts the level ticks ahead of it (its spawn order). Without it, all of them. */
    ticksAfter?: number | null;
    getsOff?: boolean | null;
    /** Its eye height, from which a TNT cart's blast pushes it: without it 0.85 of its box's height. */
    eyeHeight?: number | null;
    /** An armor stand: it pushes only the rideable carts at its own place, by its own way. */
    armorStand?: boolean | null;
    /** A bat, which pushes nothing. */
    bat?: boolean | null;
    /** The scoreboard team it is on. */
    team?: NativeMinecartTeam | null;
    /** Entity.boardingCooldown, from 1.9: while above 0 no cart picks it up. Getting off a cart sets 60. */
    boardingCooldown?: number | null;
    /** Set by the tick: it left the level with the cart it rides, through a nether or end portal (from 1.21). */
    removed?: boolean | null;
}

export interface NativeMinecartStep {
    position: NativeVec3;
    movement: NativeVec3;
    yRot: number;
    xRot: number;
    weight: number;
}

/** What a tick left of one cart. */
export interface NativeMinecartTick {
    state: NativeMinecartState;
    /** The powered state of the activator rail it passed. */
    activator?: boolean | null;
    /** What the improved path sends its clients. */
    steps: NativeMinecartStep[];
    /** Its fuse was primed: the entity event the clients hear. */
    primed: boolean;
    /** The blasts of Level.explode: the scene's other carts and entities take their hurt and push; the blocks are the server's. */
    explosions: NativeMinecartExplosion[];
    /** The activator rail ran a command block cart's command. */
    commandRan: boolean;
    /** A spawner cart's turn that the level's random decides: `attempt` at `spawnAt` (what came of it is the server's) or `delay`. */
    spawner?: string | null;
    spawnAt?: NativeBlockPosition | null;
}

export interface NativeMinecartScene {
    carts: NativeMinecartTick[];
    others: NativeMinecartOther[];
    items: NativeMinecartItem[];
    containers: NativeMinecartContainer[];
    gateways: NativeMinecartGateway[];
}

/** A client's cart under the old path. */
export interface NativeMinecartLerp {
    position: NativeVec3;
    yRot: number;
    xRot: number;
    steps: number;
    target: NativeVec3;
    targetYRot: number;
    targetXRot: number;
}

/** A client's cart under the improvements. */
export interface NativeMinecartClientSteps {
    position: NativeVec3;
    velocity: NativeVec3;
    yRot: number;
    xRot: number;
    delay: number;
    pending: NativeMinecartStep[];
    current: NativeMinecartStep[];
    totalWeight: number;
    old: NativeMinecartStep;
}

/** A mob whose travel is the engine's, where its AI left it. */
export interface NativeMobTravelState {
    /**
     * The entity's registry name: squid, glow_squid, cod, salmon, tropical_fish, pufferfish, tadpole, guardian,
     * elder_guardian, ghast, phantom, allay, vex, blaze, zombie, cow, bat, bee, parrot, dolphin, turtle, axolotl, frog,
     * drowned, spider, cave_spider, polar_bear, wither, armor_stand, strider, skeleton_horse or sulfur_cube; "living" for
     * any mob with no travel of its own.
     */
    kind: string;
    position: NativeVec3;
    /** The motion after its AI. */
    velocity: NativeVec3;
    /** Entity.getYRot, in degrees. */
    yawDegrees: number;
    /** xxa, yya, zza. */
    input: NativeVec3;
    /** LivingEntity.getSpeed. */
    speed: number;
    halfWidth: number;
    height: number;
    maxStep?: number | null;
    boundingBox?: NativeAabb | null;
    onGround?: boolean | null;
    collidedHorizontally?: boolean | null;
    collidedVertically?: boolean | null;
    inWater?: boolean | null;
    inLava?: boolean | null;
    noGravity?: boolean | null;
    stuckSpeedMultiplier?: NativeVec3 | null;
    fallDistance?: number | null;
    /** Mob.getTarget is set: a fish or guardian then does not sink. */
    hasTarget?: boolean | null;
    /** Guardian.isMoving. */
    guardianMoving?: boolean | null;
    /** Spider.isClimbing. */
    climbing?: boolean | null;
    /** Entity.isPushedByFluid; the kind's when absent. */
    pushedByFluid?: boolean | null;
    /** Entity.getFluidJumpThreshold; 0.4 when absent. */
    fluidJumpThreshold?: number | null;
    /** Entity.isUnderWater. */
    underWater?: boolean | null;
    /** Drowned.wantsToSwim. */
    wantsToSwim?: boolean | null;
    /** Turtle.goingHome, and within 20 blocks of it. */
    nearHome?: boolean | null;
    /** ArmorStand.isMarker. */
    marker?: boolean | null;
    /** SulfurCube.hasBodyItem. */
    bodyItem?: boolean | null;
    /** SulfurCube.floatsInLiquids. */
    floatsInLiquids?: boolean | null;
    /** Entity.tickCount. */
    tickCount?: number | null;
    /** MobEffects.SLOW_FALLING. */
    slowFalling?: boolean | null;
    /** MobEffects.LEVITATION's amplifier. */
    levitationAmplifier?: number | null;
    /** MobEffects.DOLPHINS_GRACE. */
    dolphinsGrace?: boolean | null;
    /** Attributes.GRAVITY, from 1.20.5. */
    gravity?: number | null;
    /** Attributes.WATER_MOVEMENT_EFFICIENCY, from 1.21. */
    waterMovementEfficiency?: number | null;
    /** Attributes.AIR_DRAG_MODIFIER, from 26.2; absent is 1. */
    airDragModifier?: number | null;
    /** LivingEntity.shouldDiscardFriction (a goat's or frog's long jump, a breeze's jump), from 1.17. */
    discardFriction?: boolean | null;
    /** Mob.isEffectiveAi, false for a NoAI mob, whose travel then does nothing; true when absent. */
    effectiveAi?: boolean | null;
}

/** A sample of a dragon's flight history. */
export interface NativeDragonFlightSample {
    y: number;
    yawDegrees: number;
}

/** A dragon as its AI leaves it, on the server. What the AI decides is the caller's. */
export interface NativeDragonState {
    position: NativeVec3;
    velocity: NativeVec3;
    /** Entity.getYRot, in degrees. */
    yawDegrees: number;
    /** EnderDragon.yRotA. */
    yawAcceleration?: number | null;
    /** EnderDragon.inWall: the last tick's. */
    inWall?: boolean | null;
    /** Entity's kept box, before 1.17. */
    boundingBox?: NativeAabb | null;
    /** DragonPhaseInstance.getFlyTargetLocation; absent flies nowhere. In 1.8 the target after the AI. */
    target?: NativeVec3 | null;
    /** 1.8: the target the tick began with. */
    steeringTarget?: NativeVec3 | null;
    /** DragonPhaseInstance.getFlySpeed. */
    flySpeed?: number | null;
    /** DragonPhaseInstance.getTurnSpeed. */
    turnSpeed?: number | null;
    /** DragonPhaseInstance.isSitting. */
    sitting?: boolean | null;
    /** Where it was, newest first. */
    history?: NativeDragonFlightSample[] | null;
    /** `history` already has this tick's sample: the flight alone runs. */
    historyRecorded?: boolean | null;
}

/** One of the boxes a dragon is hit in: the dragon itself is not pickable. */
export interface NativeDragonPart {
    /** "head", "neck", "body", "tail" or "wing". */
    name: string;
    /** Its entity id less the dragon's. */
    idOffset: number;
    position: NativeVec3;
    boundingBox: NativeAabb;
}

export interface NativeDragonTickResult {
    state: NativeDragonState;
    /** In the order of the entity ids. */
    parts: NativeDragonPart[];
    /** The parts enter an entity list right after the dragon, seen only when its own box is; else after the level's other entities. */
    partsFollowDragon: boolean;
}

export interface NativeDragonPushTarget {
    id: number;
    position: NativeVec3;
    boundingBox: NativeAabb;
}

export interface NativeDragonPush {
    id: number;
    impulse: NativeVec3;
}

export interface NativeMovingEntityMovement {
    from: NativeVec3;
    to: NativeVec3;
    axisOrder?: NativeVec3 | null;
}

/** A primed TNT, falling block, item or experience orb. A field left out takes the value a new entity has. */
export interface NativeMovingEntityState {
    kind: 'tnt' | 'falling_block' | 'item' | 'experience_orb' | (string & {});
    /** "client" (the default: a prediction, with no event) or "server". */
    authority?: 'client' | 'server' | (string & {}) | null;
    id?: number | null;
    /** Entity.tickCount, which the level counts before the tick. */
    tickCount?: number | null;
    position: NativeVec3;
    velocity: NativeVec3;
    /** Entity's kept box, before 1.17. */
    boundingBox?: NativeAabb | null;
    onGround?: boolean | null;
    collidedHorizontally?: boolean | null;
    collidedVertically?: boolean | null;
    verticalCollisionBelow?: boolean | null;
    inWater?: boolean | null;
    inLava?: boolean | null;
    noGravity?: boolean | null;
    noPhysics?: boolean | null;
    stuckSpeedMultiplier?: NativeVec3 | null;
    fallDistance?: number | null;
    ultraWarm?: boolean | null;
    firstTick?: boolean | null;
    /** Entity.finalMovementsThisTick: a resting item's blocks from 26.1. */
    lastMovements?: NativeMovingEntityMovement[] | null;
    removed?: boolean | null;
    /** PrimedTnt.getFuse. */
    fuse?: number | null;
    /** FallingBlockEntity.time. */
    time?: number | null;
    /** The carried block's id, as the world's blocks say it. */
    carriedBlock?: number | null;
    carriedAir?: boolean | null;
    concretePowder?: boolean | null;
    /** 1.18 and 1.18.1: the client froze the landed block until its removal. */
    discarding?: boolean | null;
    /** ItemEntity.age, ExperienceOrb.age. */
    age?: number | null;
    /** ItemEntity.pickupDelay; ExperienceOrb.throwTime before 1.17. */
    pickupDelay?: number | null;
    itemEmpty?: boolean | null;
    /** The item is fire immune (a netherite item or ancient debris): no burn draw in lava on 1.16. */
    fireImmune?: boolean | null;
    /** ExperienceOrb.followingPlayer, by its entity id. */
    followingPlayer?: number | null;
    /** ExperienceOrb.followingTime before 1.17. */
    followingTime?: number | null;
    /** ExperienceOrb.tickCount before 1.17, its own and not Entity's. */
    orbTicks?: number | null;
}

export interface NativeMovingEntityPlayer {
    id: number;
    position: NativeVec3;
    eyeHeight: number;
    spectator?: boolean | null;
    dead?: boolean | null;
}

/** What a client cannot see, which the caller decides. */
export interface NativeMovingEntityInputs {
    /** The entity's next random.nextFloat() draws, in order. */
    random?: number[] | null;
    /** The players of the level, in its order. */
    players?: NativeMovingEntityPlayer[] | null;
    /** A falling block on a server before 1.14: FallingBlock.isFree under its feet. */
    fallsThroughBelow?: boolean | null;
}

/** What a tick leaves for others to do. Nothing here has happened, and a client's tick says none of it. */
/** The living entity a firework rocket is attached to. */
export interface NativeRocketHolder {
    position: NativeVec3;
    velocity: NativeVec3;
    /** LivingEntity.isFallFlying. */
    gliding: boolean;
    /** A player: only it holds the rocket in a hand off its centre. Absent: true. */
    player?: boolean;
    yawDegrees?: number;
    /** The hand with the rocket is the left. */
    leftHand?: boolean;
}

/** What a piston pushes that is no player: its body as its own tick left it, and what its Entity.move depends on. */
export interface NativePistonPushedEntity {
    position: NativeVec3;
    velocity: NativeVec3;
    /** The box, kept before 1.17 (absent: from the position, halfWidth and height). */
    boundingBox?: NativeAabb;
    halfWidth: number;
    height: number;
    onGround?: boolean;
    collidedHorizontally?: boolean;
    collidedVertically?: boolean;
    verticalCollisionBelow?: boolean;
    fallDistance?: number;
    stuckSpeedMultiplier?: NativeVec3;
    /** A LivingEntity: it bounces off slime and beds fully. */
    living?: boolean;
    /** Entity.maxUpStep, with the STEP_HEIGHT attribute of a LivingEntity. Absent: 0. */
    stepHeight?: number;
    /** The entity's getGravity, for the 26.2 landing's bounce. Absent: 0.04. */
    gravity?: number;
    /** The entity's air drag, for the 26.2 landing's bounce. Absent: 0.98F. */
    airDrag?: number;
    /** Entity.noPhysics: the whole of the push, through what is in the way. */
    noPhysics?: boolean;
    /** The client's view of it. Absent: false. */
    clientSide?: boolean;
    /** Entity.isLocalInstanceAuthoritative. Absent: true. */
    controlledByLocalInstance?: boolean;
    /** Entity.canSimulateMovement, from 26.3 (docs/piston-authority.md). */
    canSimulateMovement?: boolean;
    /** What earlier calls of this game tick moved it (their pistonDeltas). Absent: nothing. */
    pistonDeltas?: NativeVec3;
}

/** The entity after the tick's pistons. */
export interface NativePistonPushedResult {
    position: NativeVec3;
    velocity: NativeVec3;
    boundingBox?: NativeAabb;
    onGround: boolean;
    collidedHorizontally: boolean;
    collidedVertically: boolean;
    verticalCollisionBelow: boolean;
    fallDistance: number;
    stuckSpeedMultiplier: NativeVec3;
    /** How far they moved it on each axis (Entity.pistonDeltas), which is its limit of 0.51 a tick. */
    pistonDeltas: NativeVec3;
}

export interface NativeMovingEntityTickResult {
    state: NativeMovingEntityState;
    /** PrimedTnt.explode. */
    explodes: boolean;
    /** A falling block ended here: its block goes in, breaks or drops. */
    landedAt?: NativeBlockPosition | null;
    /** A falling block fell too long. */
    expired: boolean;
    /** Before 1.18.2 a falling block's first tick takes its own block out of the world, on both sides. */
    clearsBlock?: NativeBlockPosition | null;
    /** The tick looks for items or orbs to merge with. */
    mergeCheck: boolean;
    /** How many of inputs.random the tick drew. */
    randomUsed: number;
    /** The tick drew more than it was given. */
    randomMissing: boolean;
}

export interface NativeBoatState {
    /** The entities around the boat, it and those sharing it left out: its move collides with them as vanilla's does. */
    nearbyEntities?: NativeNearbyEntity[] | null;
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
    /** The boat ticks on the server, which runs the column's timer. */
    serverSide?: boolean;
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
    /** Entity.fallDistance of the boat (absent: 0). */
    fallDistance?: number;
    /** The box the boat keeps before 1.17. */
    boundingBox?: NativeAabb;
    /** 1.8's EntityBoat.speedMultiplier: how hard its rider's input pushes it (0.07 to 0.35). */
    speedMultiplier?: number;
    /** 1.8: the rider's yaw (degrees), moveForward and moveStrafing, which push the boat. */
    riderYaw?: number;
    riderForward?: number;
    riderStrafe?: number;
    /** Entity.stuckSpeedMultiplier: what a cobweb or powder snow the boat was inside slows its next move by. */
    stuckSpeedMultiplier?: NativeVec3;
    /** The boat's dimension is ultrawarm (the nether): lava pushes it at 0.007 (FAST_LAVA), else 0.0023333333333333335. */
    ultraWarm?: boolean;
    /** The boat's synced NoGravity flag (Entity.isNoGravity, 1.10+): floatBoat takes no gravity. */
    noGravity?: boolean;
    /** Boat.paddlePositions (1.9+): the left and right paddles' rowing phases. From 1.12 a paddle at the start of its stroke plays its sound, which takes the status (land friction, water level) again. */
    paddlePhases?: number[];
    /** Entity.isSilent: a silent boat plays no paddle sound. */
    silent?: boolean;
    /** AbstractBoat.lerpSteps: the steps left of the interpolation a position packet started (1.21.2 - 1.21.4 run it on a controlled boat). */
    lerpSteps?: number;
    /** AbstractBoat.lerpX, lerpY and lerpZ: where the interpolation goes. */
    lerpPosition?: NativeVec3;
    /** AbstractBoat.lerpYRot: the yaw it turns to, in degrees. */
    lerpYRot?: number;
}

export interface NativeHorseState {
    /** The entities around the mob, it and those sharing it left out: its move collides with them as vanilla's does. */
    nearbyEntities?: NativeNearbyEntity[] | null;
    position: NativeVec3;
    velocity: NativeVec3;
    yaw: number;
    forward: number;
    strafe: number;
    jumpRequested: boolean;
    onGround: boolean;
    inWater: boolean;
    /** The MOVEMENT_SPEED attribute (0 is a legal one); the mob's base when not given. */
    movementSpeed?: number;
    /** The JUMP_STRENGTH attribute (0 is a legal one); the mob's base when not given. */
    jumpStrength?: number;
    /** A ridden mob's measured size in its pose (SCALE, age); its type's when not given. */
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
    /** A ridden mob's getFluidHeight(LAVA) > 0 of its last base tick (`inLava` when not given). */
    lavaHeightPositive?: boolean;
    /**
     * The world-less horse's getFluidHeight(WATER) while `inWater`, what floatInWaterWhileRidden lifts it by (0 when not
     * given; a ridden mob in its world measures its own).
     */
    waterHeight?: number;
    fallDistance?: number;
    /** The box the mob keeps before 1.17. */
    boundingBox?: NativeAabb;
    /** Entity.stuckSpeedMultiplier, for the next move. */
    stuckSpeedMultiplier?: NativeVec3;
    slowFalling?: boolean;
    levitationAmplifier?: number;
    dolphinsGrace?: boolean;
    /** Weaving (1.20.5+), which changes what a cobweb leaves the mob. */
    weaving?: boolean;
    /** The GRAVITY attribute (1.20.5+), 0.08 when not given. */
    gravity?: number;
    /** The STEP_HEIGHT attribute (1.20.5+), the mob's base when not given. */
    stepHeight?: number;
    /** The WATER_MOVEMENT_EFFICIENCY attribute (1.21+). */
    waterMovementEfficiency?: number;
    /** The AIR_DRAG_MODIFIER and FRICTION_MODIFIER attributes (26.2+). */
    airDragModifier?: number;
    frictionModifier?: number;
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
    /** Its MOVEMENT_SPEED with the suffocating modifier, when other modifiers keep it from being `movementSpeed` less 34%. */
    suffocatingSpeed?: number;
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
    /** The mob's dimension is ultrawarm (the nether): lava pushes it at 0.007 (FAST_LAVA), else 0.0023333333333333335. */
    ultraWarm?: boolean;
    /** The amplifier of the mob's Jump Boost, which its riding jump adds 0.1F a level to. */
    jumpBoostAmplifier?: number;
    /** The mob's synced NoGravity flag (Entity.isNoGravity, 1.10+): its travel takes no gravity. */
    noGravity?: boolean;
}

export interface NativeProjectileState {
    /** The box a rocket or hook keeps before 1.17 (absent: from the position). */
    boundingBox?: NativeAabb;
    position: NativeVec3;
    velocity: NativeVec3;
    gravity?: number;
    airDrag?: number;
    /** AbstractHurtingProjectile's power vector, added to the motion before 1.21. */
    power?: NativeVec3;
    /** AbstractHurtingProjectile.accelerationPower (1.21+, 0.1 when absent), added along the motion. */
    accelerationPower?: number;
    /** A dangerous wither skull, whose air inertia is 0.73F. */
    dangerous?: boolean;
    inWater?: boolean;
    /** The dimension is ultrawarm: lava pushes at 0.007. From 1.16. */
    ultraWarm?: boolean;
    firstTick?: boolean;
    /** Optional projectile family tag for source-specialized dispatch. */
    kind?: string;
    /** The horizontal collision of the last move (firework rocket, fishing hook). */
    horizontalCollision?: boolean;
    /** The ground contact of the last move (fishing hook). */
    onGround?: boolean;
    /** Source-derived firework shot-at-angle flag. */
    shotAtAngle?: boolean;
    /** A fishing hook in its BOBBING state. */
    bobbing?: boolean;
    /** The bobbing fishing hook's random.nextFloat() for this tick. */
    bobRandom?: number;
    /** Entity.noPhysics: Entity.move passes through blocks and a firework rocket acts on no hit. */
    noPhysics?: boolean;
    /** Entity.stuckSpeedMultiplier a cobweb or powder snow left on a hook or rocket, which its next move takes (absent: none). */
    stuckSpeedMultiplier?: NativeVec3;
}

export interface NativeProjectileTickResult {
    position: NativeVec3;
    velocity: NativeVec3;
    blockHit: boolean;
    /** Entity.isInWater after the tick: the versioned batch's own, read from the world at the family's base tick. */
    inWater: boolean;
    firstTick: boolean;
    horizontalCollision: boolean;
    onGround: boolean;
    bobbing: boolean;
    /** The client removed the projectile this tick (llama spit in water or inside blocks). */
    discarded: boolean;
    /** The box a rocket or hook keeps before 1.17. */
    boundingBox?: NativeAabb;
    /** Entity.stuckSpeedMultiplier the tick left on a hook or rocket, for the next tick's state. */
    stuckSpeedMultiplier: NativeVec3;
}

/** An entity a projectile may hit. Vanilla's order of candidates, Level.getEntities, decides ties: keep it. */
export interface NativeProjectileTarget {
    /** A dragon's part before 1.21.4: the dragon's box, which a search has to meet to find it. */
    dragonBox?: NativeAabb | null;
    /** Entity.getId. */
    id: number;
    /** Entity.getRootVehicle().getId(); its own id when absent. */
    rootVehicle?: number;
    position: NativeVec3;
    /** The box as it is; without one, width, height and scale make it. */
    boundingBox?: NativeAabb;
    /** The width of its pose, before its scale. */
    width?: number;
    /** The height of its pose, before its scale. */
    height?: number;
    /** LivingEntity.getScale; 1 when absent. */
    scale?: number;
    /** Entity.isPickable; true when absent. */
    pickable?: boolean;
    /** Entity.isAlive; true when absent. */
    alive?: boolean;
    spectator?: boolean;
    /** A player the owning player may not harm. */
    friendly?: boolean;
    /** 1.8: PlayerAbilities.invulnerable. */
    invulnerablePlayer?: boolean;
    /** What Entity.hurt answers; true when absent. False turns an arrow back. */
    hurt?: boolean;
    enderman?: boolean;
    /** The type deflects projectiles (a breeze). */
    deflects?: boolean;
    /** random.nextFloat() of the 1.20.3 deflection. */
    deflectRandom?: number;
    /** Entity.noPhysics: fireballs and shulker bullets pass through it. */
    noPhysics?: boolean;
    /** An ItemEntity, which a fishing hook catches. */
    item?: boolean;
    /** A wind charge, which wind charges pass through. */
    windCharge?: boolean;
    /** An end crystal, which wind charges pass through. */
    endCrystal?: boolean;
}

/** Projectile.getOwner, when it gives an entity: leave it out for an owner that is gone or not loaded. */
export interface NativeProjectileOwner {
    id: number;
    /** Its own id when absent. */
    rootVehicle?: number;
    eyePosition: NativeVec3;
    width: number;
    alive: boolean;
    player: boolean;
    spectator?: boolean;
    /** Entity.isRemoved: a fireball is gone with it. */
    removed?: boolean;
    /** ServerPlayer.wonGame: in the end credits, which an ender pearl waits out from 1.21.9. Absent: false. */
    wonGame?: boolean;
}

/** What a projectile keeps between ticks beside its motion: pass back the hitState a tick returns. */
export interface NativeProjectileHitState {
    /** Entity.tickCount. */
    tickCount?: number;
    /** The ticks in flight, which end the owner's immunity until 1.15. */
    flyingTicks?: number;
    /** Projectile.leftOwner, from 1.16. */
    leftOwner?: boolean;
    /** From 1.14. */
    pierceLevel?: number;
    /** The ids already pierced. */
    pierced?: number[];
    /** From 1.21. */
    lastDeflectedBy?: number;
    /** AbstractArrow.baseDamage; 2 when absent. */
    baseDamage?: number;
    /** ThrownTrident.ID_LOYALTY. */
    loyalty?: number;
    /** pickup == ALLOWED. */
    pickupAllowed?: boolean;
    /** What a thrown item ignores, from 1.9 until 1.15. */
    ignored?: number;
    ignoreTime?: number;
    /** A rocket's age, or a hook's time on the ground. */
    life?: number;
    /** A rocket's lifetime, a random of the server's: without one it never bursts by age. */
    lifeTime?: number;
    /** The rocket has explosions, which a block hit sets off. */
    explodes?: boolean;
    /** FireworkRocketEntity.isAttachedToEntity, from 1.11: such a rocket never flies. Without a holder its id found no living entity, and it stays where it is. */
    attached?: boolean;
    /** What it is attached to, after its boost, which the player's engine runs. Vanilla keeps one it has resolved even when that is gone. */
    holder?: NativeRocketHolder;
    /** A hook's inGround, until 1.15. */
    inGround?: boolean;
    /** What frees a hook from a block before 1.11: per axis random.nextFloat() * 0.2F, the server's. Without one it keeps none of its motion. */
    groundRelease?: NativeVec3;
    /** The id a hook hooked. */
    hooked?: number;
    /** The id a hook of 1.9 or 1.10 tracks, which a hit must meet twice. */
    hookTracked?: number;
    /** The hook rides `hooked` (HOOKED_IN_ENTITY). */
    hookSettled?: boolean;
    /** FishingHook.shouldStopFishing, the server's say on the owner's rod. */
    stopFishing?: boolean;
    /** A shulker bullet's targetDelta while its target lives; without one it falls. The server draws it and sends none of it. */
    bulletDelta?: NativeVec3;
    /** The server has no chunk here: a fireball is gone. */
    chunkUnloaded?: boolean;
    /** Entity.isOutsideWorldBorder as 1.13's collision query of the box leaves it; pass back the one returned. */
    outsideBorder?: boolean;
    /** A shulker bullet is gone on peaceful. */
    peaceful?: boolean;
    /** enderPearlsVanishOnDeath is off. */
    pearlOutlivesOwner?: boolean;
}

export interface NativeProjectileHitTick {
    projectile: NativeProjectileTickResult;
    hitState: NativeProjectileHitState;
    /** In the order vanilla hit them. */
    entityHits: NativeProjectileEntityHit[];
    blockHit?: NativeProjectileBlockHit | null;
    /** A deflection halves it, for the next tick's state. */
    accelerationPower: number;
    /** A deflection sets it in 1.20.5 and 1.20.6, for the next tick's state. */
    power: NativeVec3;
}

export interface NativeProjectileEntityHit {
    target: number;
    /** EntityHitResult.getLocation: the entity's position before 1.21.2, the point on the ray from then. */
    location: NativeVec3;
    reaction: 'hurt' | 'refused' | 'evaded' | 'deflected' | 'spent';
    /** An arrow's damage before a critical roll. */
    damage?: number;
}

export interface NativeProjectileBlockHit {
    block: NativeBlockPosition;
    face: 'down' | 'up' | 'north' | 'south' | 'west' | 'east';
    location: NativeVec3;
    border: boolean;
}

export interface NativeArrowHitTick {
    arrow: NativeArrowTickResult;
    hitState: NativeProjectileHitState;
    /** In the order vanilla hit them. */
    entityHits: NativeProjectileEntityHit[];
    blockHit?: NativeProjectileBlockHit;
}

export interface NativeTridentHitTick {
    trident: NativeTridentTickResult;
    /** AbstractArrow.isNoPhysics: a returning trident stays so, for the next tick's state. */
    noPhysics: boolean;
    hitState: NativeProjectileHitState;
    entityHits: NativeProjectileEntityHit[];
    blockHit?: NativeProjectileBlockHit;
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
    /** The state id of the block the arrow hit. */
    lastBlockId?: number;
    /** The block the arrow hit, whose state the in-ground test reads until 1.13.2. */
    lastBlockPosition?: NativeBlockPosition;
    /** AbstractArrow.isNoPhysics, from 1.13: no containment, block ray or gravity. */
    noPhysics?: boolean;
    /** The dimension is ultrawarm: lava pushes at 0.007. From 1.16. */
    ultraWarm?: boolean;
}

export interface NativeBlockPosition {
    x: number;
    y: number;
    z: number;
}

export interface NativeArrowTickResult {
    position: NativeVec3;
    velocity: NativeVec3;
    blockHit: boolean;
    inWater: boolean;
    inGround: boolean;
    discarded: boolean;
    /** The count towards the 1200-tick despawn, for the next call's state. */
    life: number;
    inGroundTime: number;
    shakeTime: number;
    startedFalling: boolean;
    lastBlockId?: number;
    lastBlockPosition?: NativeBlockPosition;
}

export interface NativeTridentState {
    position: NativeVec3;
    velocity: NativeVec3;
    gravity?: number;
    airDrag?: number;
    waterInertia?: number;
    inWater?: boolean;
    dealtDamage?: boolean;
    /** AbstractArrow's embedded state, as in NativeArrowState. */
    inGround?: boolean;
    life?: number;
    inGroundTime?: number;
    shakeTime?: number;
    fallImpulse?: NativeVec3;
    lastBlockId?: number;
    lastBlockPosition?: NativeBlockPosition;
    /** AbstractArrow.isNoPhysics, from 1.13: a returning trident. */
    noPhysics?: boolean;
    /** The dimension is ultrawarm: lava pushes at 0.007. From 1.16. */
    ultraWarm?: boolean;
}

export interface NativeTridentTickResult {
    position: NativeVec3;
    velocity: NativeVec3;
    blockHit: boolean;
    inWater: boolean;
    inGround: boolean;
    discarded: boolean;
    /** The count towards the 1200-tick despawn, for the next call's state. */
    life: number;
    inGroundTime: number;
    shakeTime: number;
    startedFalling: boolean;
    dealtDamage: boolean;
    lastBlockId?: number;
    lastBlockPosition?: NativeBlockPosition;
}

/** The addon a host loads when it is a declared native target (package.json napi.targets). */
function declaredAddon(): string | null {
    const host = `${process.platform}-${process.arch}`;
    if (host === 'win32-x64') return 'index.win32-x64-msvc.node';
    if (host !== 'linux-x64') return null;
    // musl is not a declared target, and has no glibc to report
    const report = process.report?.getReport() as { header?: { glibcVersionRuntime?: string } } | undefined;
    return report?.header?.glibcVersionRuntime ? 'index.linux-x64-gnu.node' : null;
}

const WASM_REQUESTED = (): boolean => ['true', 'error'].includes(process.env.NAPI_RS_FORCE_WASI ?? '')
    || Boolean(process.env.NAPI_RS_WASI_FLAVOR);

/** Which build a binding is, by what it says of itself and not by what was asked for. */
export function nativeBackend(binding: NativePhysicsBinding): NativeBuildIdentity & { backend: 'native' | 'wasm' } {
    const identity = binding.buildIdentity();
    return { ...identity, backend: identity.target.startsWith('wasm32-') ? 'wasm' : 'native' };
}

/** What writes to a cell: its first three arguments are the cell's. */
const CELL_WRITERS = [
    'setFullBlock', 'setBlockStateId', 'setBlockEntity', 'removeBlockEntity', 'setBlockProperties', 'setBlockShape',
    'setBlockOutlineShape', 'setBlockShapes', 'removeBlock', 'setFluid', 'setFluidState', 'setVersionedFluidState',
] as const;

/** A Java int: N-API's own conversion would truncate 0.5 to 0 and wrap 2^33 to 0 without a word. */
function checkCell(operation: string, x: number, y: number, z: number): void {
    const cell = (value: number) => Number.isInteger(value) && value >= -2147483648 && value <= 2147483647;
    if (!cell(x) || !cell(y) || !cell(z)) throw new RangeError(`${operation}: (${x}, ${y}, ${z}) is no block cell`);
}

const guarded = new WeakSet<object>();

function guardCells(binding: NativePhysicsBinding): void {
    const world = binding.NativeWorld?.prototype as unknown as Record<string, (...args: unknown[]) => unknown> | undefined;
    if (!world || guarded.has(world)) return;
    guarded.add(world);
    for (const name of CELL_WRITERS) {
        const write = world[name];
        if (!write) continue;
        world[name] = function guardedWrite(this: unknown, ...args: unknown[]) {
            checkCell(name, args[0] as number, args[1] as number, args[2] as number);
            return write.apply(this, args);
        };
    }
    const apply = world.applyBlockUpdates;
    if (apply) {
        world.applyBlockUpdates = function guardedUpdates(this: unknown, updates: unknown) {
            for (const { x, y, z } of updates as NativeBlockUpdate[]) checkCell('applyBlockUpdates', x, y, z);
            return apply.call(this, updates);
        };
    }
}

/**
 * The checks LoadNativePhysics puts on its addon (whole numbers, cells), put on a binding a caller gives the adapter
 * instead: an addon loaded another way, or another copy of it, gets the same refusals by field name (B209). A binding
 * is guarded once, in place, and returned.
 */
export function guardNativeBinding<T extends object>(binding: T): T {
    guardWholeNumbers(binding);
    guardCells(binding as unknown as NativePhysicsBinding);
    return binding;
}

export function LoadNativePhysics(): NativePhysicsBinding {
    const override = process.env.NAPI_RS_NATIVE_LIBRARY_PATH;
    const modulePath = override ?? '../native/artifacts/native.js';
    let binding: NativePhysicsBinding;
    try {
        binding = guardNativeBinding(module.require(modulePath) as NativePhysicsBinding);
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`Native physics addon is unavailable at ${modulePath}: ${message}`);
    }
    if (override) return binding;
    if (typeof binding.buildIdentity !== 'function') {
        throw new Error(`The addon at ${modulePath} predates this package: it has no buildIdentity. Rebuild it (pnpm native:build) or reinstall the package.`);
    }
    // the loader falls back to WebAssembly when an addon does not load: on a declared native target that is a broken install
    const addon = declaredAddon();
    if (addon && !WASM_REQUESTED() && nativeBackend(binding).backend === 'wasm') {
        throw new Error(`The native addon ${addon} of this platform did not load, and WebAssembly was not asked for: reinstall the package, or set NAPI_RS_FORCE_WASI=true to run WebAssembly.`);
    }
    // =true keeps native as the fallback: say so, or a run meant to measure WebAssembly measures the addon
    if (process.env.NAPI_RS_FORCE_WASI === 'true' && nativeBackend(binding).backend === 'native') {
        process.emitWarning('NAPI_RS_FORCE_WASI=true fell back to the native addon: no WebAssembly build loaded. NAPI_RS_FORCE_WASI=error refuses instead.');
    }
    return binding;
}
