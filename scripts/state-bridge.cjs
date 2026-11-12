// How each field of NativeSimulationState reaches a NativePlayer (B015, docs/state-bridge.md). A
// packed tick carries what changes every tick; everything else is handed over whole by setState and
// kept by the player between ticks. test/state-bridge.test.cjs fails on a field left out of here, and
// on a channel the packed layout does not have.

/** Fields with a slot of NativePlayerInput. */
const PACKED = {
  position: "POSITION", velocity: "VELOCITY", yaw: "YAW", yawDegrees: "YAW_DEGREES", pitch: "PITCH", look: "LOOK", input: "STRAFE",
  halfWidth: "HALF_WIDTH", height: "HEIGHT", eyeHeight: "EYE_HEIGHT", fallDistance: "FALL_DISTANCE", jumpTicks: "JUMP_TICKS",
  autoJumpTime: "AUTO_JUMP_TIME", stuckSpeedMultiplier: "STUCK_SPEED_MULTIPLIER", riptideLaunch: "RIPTIDE_LAUNCH",
  fireworkRockets: "FIREWORK_ROCKETS", fireworkRocketsBeforePlayer: "FIREWORK_ROCKETS_BEFORE_PLAYER",
  flyingSpeed: "FLYING_SPEED", itemUseSpeedMultiplier: "ITEM_USE_SPEED_MULTIPLIER",
};

/** Fields with a bit of NativePlayerInputFlag. */
const FLAGS = {
  jumpRequested: "JUMP_REQUESTED", sprinting: "SPRINTING", wasSprinting: "WAS_SPRINTING", sneaking: "SNEAKING",
  movingSlowly: "MOVING_SLOWLY", swimming: "SWIMMING", flying: "FLYING", fallFlying: "FALL_FLYING", usingItem: "USING_ITEM",
  autoJump: "AUTO_JUMP", noPhysics: "NO_PHYSICS", onGround: "ON_GROUND", collidedHorizontally: "COLLIDED_HORIZONTALLY",
  minorHorizontalCollision: "MINOR_HORIZONTAL_COLLISION", collidedVertically: "COLLIDED_VERTICALLY",
  verticalCollisionBelow: "VERTICAL_COLLISION_BELOW", inWater: "IN_WATER", inLava: "IN_LAVA", inPowderSnow: "IN_POWDER_SNOW",
  previousPosition: "UNMOVED", sprintModifier: "SPRINT_MODIFIER", removed: "REMOVED", sleeping: "SLEEPING", dead: "DEAD",
};

/** Fields a NativePlayer keeps from setState and hands back from state(). */
const KEPT = [
  "entityBounciness", "blockBounciness", "blockFallCallback", "ultraWarm", "isPlayer", "controlledByLocalInstance", "canSimulateMovement",
  "nonLiving", "passenger", "isVehicle", "boatVehicle", "inWeb", "weaving", "frostWalker", "sweetBerryBushImmune", "powderSnowImmune",
  "powderSnowWalkable", "fireImmune", "fireResistance", "onFireFlag", "inRain", "stabHit", "fireProtection", "burningTime",
  "remainingFireTicks", "serverFireIncrement", "serverFireIncrements", "ticksFrozen", "ticksRequiredToFreeze", "tickCount",
  "freezeDamage", "contactDamage", "wallDamage", "insideWall", "eyeInWater", "eyeInLava", "slowFalling", "levitation",
  "levitationAmplifier", "safeFallDistance", "fallDamageImmune", "clientSide", "heightAfterTick", "entityType", "invulnerable",
  "fallDamageMultiplier", "steppingCarefully", "climbing", "noGravity", "gravity", "stepHeight", "movementSpeed",
  "movementSpeedValue", "jumpStrength", "speedEffectAmplifier", "slownessEffectAmplifier", "jumpBoostAmplifier", "dolphinsGrace",
  "depthStrider", "soulSpeed", "waterMovementEfficiency", "movementEfficiency", "airDragModifier", "frictionModifier",
  "sneakingSpeed", "fallDamage", "fireworkBoost", "boundingBox", "supportingBlock", "supportingBlockKept", "onGroundNoBlocks",
  "outsideBorder", "movementSpeedAttribute", "fireworkRocketTicks", "pendingMovements",
];

/** Fields of a tick's scene: passed with that tick (entities as packed numbers), never kept. */
const SCENE = { nearbyEntities: "the tick's entities array", movingPistons: "pushByMovingPistons" };

/** What a packed tick reports, by the state field it updates (NativePlayerOutput and its flags). */
const OUTPUTS = {
  position: "POSITION", velocity: "VELOCITY", fallDistance: "FALL_DISTANCE", jumpTicks: "JUMP_TICKS", autoJumpTime: "AUTO_JUMP_TIME",
  stuckSpeedMultiplier: "STUCK_SPEED_MULTIPLIER", boundingBox: "BOUNDING_BOX", fallDamage: "FALL_DAMAGE", contactDamage: "CONTACT_DAMAGE",
  wallDamage: "WALL_DAMAGE", freezeDamage: "FREEZE_DAMAGE",
};
const OUTPUT_FLAGS = {
  onGround: "ON_GROUND", collidedHorizontally: "COLLIDED_HORIZONTALLY", minorHorizontalCollision: "MINOR_HORIZONTAL_COLLISION",
  collidedVertically: "COLLIDED_VERTICALLY", inWater: "IN_WATER", inLava: "IN_LAVA", inPowderSnow: "IN_POWDER_SNOW", inWeb: "IN_WEB",
  fallFlying: "FALL_FLYING", eyeInWater: "EYE_IN_WATER", blockFallCallback: "BLOCK_FALL_CALLBACK",
};

module.exports = { FLAGS, KEPT, OUTPUTS, OUTPUT_FLAGS, PACKED, SCENE };
