// Projectiles: the flight states and results of napi_api.rs, the hit calls of napi_projectile.rs and the adapter's scene.
const ULTRA_WARM = "DimensionType|EnvironmentAttributes.ultraWarm|FAST_LAVA";
const LAST_POSITION_UNTIL = { until: "1.13.2", why: "1.13.2 AbstractArrow.java:46,165 reads blockX; 1.14.4 AbstractArrow has no such field, and native/src/projectile.rs:928,956 stops reading it at 1.14" };
const NO_PHYSICS_SINCE = { since: "1.13", why: "1.13.2 AbstractArrow.java:156,596 reads it in tick; 1.12.2 AbstractArrow.java tick has no such read; native/src/projectile.rs:934" };
const HOOK_STATES_SINCE = { since: "1.11", why: "1.11.2 FishingHook.java:45 has the State machine, 1.10.2 FishingHook.java has no enum; native/src/projectile.rs:1688 sends earlier releases to tick_fishing_hook_before_1_11" };
const ATTACHED_SINCE = { since: "1.11", why: "1.11.2 FireworkRocketEntity.java:29 SHOOTER_ENTITY_ID; 1.10.2 FireworkRocketEntity.java:22 tracks only the item; native/src/projectile.rs:1615" };
const SHULKER_SINCE = { since: "1.9", why: "1.9.4 ShulkerBullet.java; the 1.8.8 tree has no such class" };
const PIERCE_SINCE = { since: "1.14", why: "1.14.4 AbstractArrow.java:53 PIERCE_LEVEL; 1.13.2 AbstractArrow.java has no pierce; native/src/projectile_hits.rs:373-379" };

/** What the arrow and the trident share of their state. */
const embeddedState = (body, waterInertia, waterSource) => ({
  position: [`Where the ${body} stands as the tick starts: its ray and move start here`, "blocks", "Entity.position|x"],
  velocity: ["Motion the tick's ray and move spend, which inertia and gravity then change", "blocks a tick", "Entity.deltaMovement"],
  gravity: ["Taken off the y motion at the end of a flying tick; absent, 0.05, the float's widening before 1.20.5. A noPhysics flight reads none", "blocks a tick squared", "AbstractArrow.getDefaultGravity|tick"],
  airDrag: ["Scales the motion on a flying tick out of water; 0.99F when absent", "ratio", "AbstractArrow.INERTIA|tick"],
  waterInertia: [`Scales the motion in place of airDrag while inWater; ${waterInertia} when absent`, "ratio", waterSource],
  inWater: ["Picks waterInertia over airDrag; a versioned tick recomputes it at the base tick, before the flight until 1.21.1 and after it from 1.21.2", "", "Entity.isInWater"],
});

const embeddedRest = (body) => ({
  inGround: [`The ${body} is embedded and takes the in-ground branch, no flight: a block hit or a shape around it sets it, a fall clears it`, "", "AbstractArrow.isInGround|inGround"],
  life: ["Ticks towards the despawn: the in-ground branch counts it, 1200 discards, a fall resets it to 0", "ticks", "AbstractArrow.life"],
  inGroundTime: ["Ticks in a row in the ground: the in-ground branch counts it, a flying tick resets it to 0", "ticks", "AbstractArrow.inGroundTime"],
  shakeTime: ["Counts down at the top of every tick; a block hit sets 7", "ticks", "AbstractArrow.shakeTime"],
  fallImpulse: ["Per-axis factor on the motion when the block goes and it falls: random.nextFloat() * 0.2F, the server's draw. Absent stops the motion", "ratio", "AbstractArrow.startFalling|tick", { range: "0..0.2" }],
  lastBlockId: ["State id of the block it hit; a block read in the ground that differs lets it fall", "id", "AbstractArrow.lastState"],
  lastBlockPosition: ["The block it hit, which the containment and in-ground tests read until 1.13.2; from 1.14 the tick only writes it", "blocks", "binding: arrow_tick writes it at a block hit", LAST_POSITION_UNTIL],
  noPhysics: ["Skips the containment test, the in-ground branch, the ray and gravity: it moves by its motion through blocks", "", "AbstractArrow.isNoPhysics", NO_PHYSICS_SINCE],
  ultraWarm: ["Sets lava's current at the base tick to 0.007, else 0.0023333", "", ULTRA_WARM],
});

/** What the results of an arrow and a trident share. */
const embeddedResult = (body) => ({
  position: [`Where the ${body} stands after the tick`, "blocks", "Entity.position|x"],
  velocity: ["Motion for the next tick, after the flight and the base tick", "blocks a tick", "Entity.deltaMovement"],
  blockHit: ["The tick's flight stopped at a block: a border hit that bounces does not count", "", "binding: arrow_tick reports a hit that did not bounce"],
  inWater: ["What the tick left of the state's inWater, for the next tick", "", "Entity.isInWater"],
  inGround: ["It is embedded after the tick, for the next state", "", "AbstractArrow.isInGround|inGround"],
  discarded: ["It is gone: a hit removed it, 1200 ticks in the ground passed, or it fell 64 blocks below the world", "", "binding: arrow_tick and arrow_tick_in_world hand it back"],
  life: ["The despawn count after the tick, for the next state", "ticks", "AbstractArrow.life"],
  inGroundTime: ["The in-ground count after the tick, for the next state", "ticks", "AbstractArrow.inGroundTime"],
  shakeTime: ["The shake count after the tick, for the next state", "ticks", "AbstractArrow.shakeTime"],
  startedFalling: ["The block went this tick and it left the ground, its motion scaled by fallImpulse", "", "AbstractArrow.startFalling|tick"],
});

const embeddedResultEnd = {
  lastBlockId: ["State id of the block hit, kept for the next state", "id", "AbstractArrow.lastState"],
  lastBlockPosition: ["The block hit, kept for the next state", "blocks", "binding: native_arrow_tick_result hands back what arrow_tick wrote", LAST_POSITION_UNTIL],
};

const hits = {
  hitState: ["What the tick left of the hit state, to pass to the next tick", "", "binding: native_hit_state hands back the state the tick changed"],
  entityHits: ["The hits of the tick, in the order vanilla made them", "", "binding: native_report hands back the tick's ProjectileHitReport"],
  blockHit: ["The block or border the tick hit; absent when none", "", "binding: native_report hands back the tick's ProjectileHitReport"],
};

module.exports = {
  NativeArrowHitTick: {
    family: "arrow",
    fields: {
      arrow: ["The flight result of this server tick", "", "binding: simulate_versioned_arrow_hit_tick hands it back"],
      ...hits,
    },
  },
  NativeArrowState: {
    family: "arrow",
    fields: {
      ...embeddedState("arrow", "0.6F", "AbstractArrow.getWaterInertia"),
      ...embeddedRest("arrow"),
    },
  },
  NativeArrowTickResult: {
    family: "arrow",
    fields: {
      ...embeddedResult("arrow"),
      ...embeddedResultEnd,
    },
  },
  NativeProjectileBlockHit: {
    family: "projectile",
    fields: {
      block: ["The block cell the ray hit, written to the report by BlockHit.report", "blocks", "BlockHitResult.getBlockPos"],
      face: ["Direction.getName of the face the ray entered by", "", "Direction.getName", { range: "down, up, north, south, west, east" }],
      location: ["The point on the ray where it met the block or border", "blocks", "HitResult.getLocation"],
      border: ["The hit is the world border's, not a block's: an arrow bounces off it", "", "BlockHitResult.isWorldBorderHit"],
    },
  },
  NativeProjectileEntityHit: {
    family: "projectile",
    fields: {
      target: ["Entity.getId of what was hit, from the target list", "id", "EntityHitResult.getEntity"],
      location: ["Where the hit is: the entity's position before 1.21.2, the point on the ray from then, and from 26.3 on the inflated box when the ray misses the box", "blocks", "HitResult.getLocation"],
      reaction: ["How the hit ended for the projectile: ProjectileReaction::name of what the family's hit code recorded", "", "binding: ProjectileReaction::name gives it", { range: "hurt, refused, evaded, deflected, spent, passed, hooked" }],
      damage: ["An arrow's damage before a critical roll: Mth.ceil of speed times baseDamage. Absent for a trident and the other families", "health points", "AbstractArrow.onHitEntity"],
    },
  },
  NativeProjectileHitState: {
    family: "projectile",
    fields: {
      tickCount: ["Entity.tickCount, which the tick counts up first; the target margin from 1.21.6 and a thrown item's first-ticks test read it", "ticks", "Entity.tickCount"],
      flyingTicks: ["Ticks in flight: the owner is safe from an arrow, thrown item or hook until 5, from a hurting projectile until 25. Read until 1.15", "ticks", "AbstractArrow|AbstractHurtingProjectile|FishingHook.flightTime"],
      leftOwner: ["The projectile is clear of what the owner rides, so it may hit them: checkLeftOwner sets it once", "", "Projectile.leftOwner"],
      pierceLevel: ["Entities an arrow may pierce past the first; a block hit clears it, and before 1.14 it reads as 0", "count", "AbstractArrow.getPierceLevel", PIERCE_SINCE],
      pierced: ["The entity ids the arrow already pierced, which it may not hit again; a block hit clears them", "id", "AbstractArrow.piercingIgnoreEntityIds", PIERCE_SINCE],
      lastDeflectedBy: ["The id that last deflected it: the same entity cannot deflect it again", "id", "Projectile.lastDeflectedBy"],
      baseDamage: ["The arrow's damage factor, with its weapon's enchantments: hit_as_arrow multiplies it by the speed. 2 when absent", "health points", "AbstractArrow.baseDamage"],
      loyalty: ["A trident's loyalty level: above 0 it returns to its owner after a hit, the pull scaled by it", "level", "ThrownTrident.ID_LOYALTY", { capability: "projectile trident" }],
      pickupAllowed: ["pickup == ALLOWED: with loyalty the trident does not age in the ground", "", "AbstractArrow.pickup"],
      ignored: ["The id a thrown item passes through while it stays near: the first ticks set it, ignoreTime running out clears it", "id", "ThrowableProjectile.entityToIgnore", { since: "1.9", why: "1.9.4 ThrowableProjectile.java:33 field_14820; 1.8.8 ThrowableProjectile.java has only the thrower" }],
      ignoreTime: ["Ticks left of `ignored` once the item is clear of it; 0 when absent", "ticks", "ThrowableProjectile.timeToIgnore", { since: "1.9", why: "1.9.4 ThrowableProjectile.java:34 field_14819; 1.8.8 ThrowableProjectile.java has no such count" }],
      life: ["A rocket's age, counted on a live tick and compared with lifeTime; a hook's ticks on the ground, 1200 of which discard it", "ticks", "FireworkRocketEntity|FishingHook.life"],
      lifeTime: ["The age past which a rocket bursts, the server's random; absent it never bursts by age", "ticks", "FireworkRocketEntity.lifetime"],
      explodes: ["The rocket has explosions, so a block hit removes it; without them it stays", "", "FireworkRocketEntity.hasExplosion"],
      attached: ["The rocket stays attached and never flies: with a holder it follows it, without one it stands still", "", "FireworkRocketEntity.isAttachedToEntity", ATTACHED_SINCE],
      holder: ["The living entity an attached rocket stands at and takes its motion from, after the player's engine ran the boost", "", "FireworkRocketEntity.attachedToEntity", ATTACHED_SINCE],
      inGround: ["A hook that hit a block: stops it, frees it the next tick before 1.11, and is what its ground count reads until 1.15", "", "FishingHook.inGround"],
      groundRelease: ["Per-axis factor on a hook's motion the tick a block frees it: random.nextFloat() * 0.2F, the server's. Absent keeps none of the motion", "ratio", "binding: tick_fishing_hook_before_1_11 reads it", { until: "1.10.2", why: "1.10.2 FishingHook.java:219-221 draws it; 1.11.2 FishingHook.java has no such draw", range: "0..0.2" }],
      hooked: ["The id the hook caught; the hook rides that target until the id leaves the list", "id", "FishingHook.hookedIn"],
      hookTracked: ["The id last set on a hook's tracked data in 1.9 and 1.10: only a second hit on the same id holds the catch", "id", "binding: family_reaction sets it", { since: "1.9", until: "1.10.2", why: "1.10.2 FishingHook.java:135-144 clears the catch when the id is set, 1.11.2 :129-136 keeps it, 1.8.8 FishingHook.java has no tracked id" }],
      hookSettled: ["The hook's first tick after a catch has passed and it rides `hooked`: HOOKED_IN_ENTITY", "", "FishingHook.currentState", HOOK_STATES_SINCE],
      stopFishing: ["The server's answer that the owner put the rod away or went too far: the hook is discarded at the top of the tick", "", "FishingHook.shouldStopFishing"],
      bulletDelta: ["A shulker bullet's steering target: the tick grows it by 1.025, clamps each axis to 1 and moves a fifth of the way; absent it falls", "blocks a tick", "ShulkerBullet.targetDeltaX", SHULKER_SINCE],
      chunkUnloaded: ["The server has no chunk here: a fireball is discarded on its tick", "", "LevelReader|Level.hasChunkAt"],
      outsideBorder: ["Entity.isOutsideWorldBorder as the box's noCollision left it: inside, the cells past the border are full blocks that a fireball's box meets. Pass back the one returned", "", "Entity.isOutsideWorldBorder|setOutsideWorldBorder", { since: "1.13", until: "1.13.2", why: "1.13.2 LevelReader.java:172-200 keeps it in the collision query; 1.14.4 LevelReader.java:147-157 asks the border's shape instead, and 1.12.2 ProjectileUtil asks no collision of the box" }],
      peaceful: ["The difficulty is peaceful: a shulker bullet is discarded at the top of the tick", "", "ShulkerBullet.checkDespawn|tick", SHULKER_SINCE],
      pearlOutlivesOwner: ["The game rule enderPearlsVanishOnDeath is off: a pearl flies on when its player owner is dead", "", "GameRules.RULE_ENDER_PEARLS_VANISH_ON_DEATH|ENDER_PEARLS_VANISH_ON_DEATH"],
    },
  },
  NativeProjectileHitTick: {
    family: "projectile",
    fields: {
      projectile: ["The flight result of this server tick", "", "binding: simulate_versioned_projectile_hit_tick hands it back"],
      ...hits,
      accelerationPower: ["The state's value after the tick, which a deflection halves; pass it in the next state", "blocks a tick squared", "AbstractHurtingProjectile.accelerationPower"],
      power: ["The state's value after the tick, which a deflection of 1.20.5 or 1.20.6 sets from the turned motion; pass it in the next state", "blocks a tick squared", "AbstractHurtingProjectile.xPower"],
    },
  },
  NativeProjectileOwner: {
    family: "projectile",
    fields: {
      id: ["Entity.getId of the owner: an owner's immunity and a family's own tests compare it with a target's", "id", "Entity.getId"],
      rootVehicle: ["The owner's root vehicle id, which leftOwner and the immunity from 1.16 compare with a target's; its own id when absent", "id", "Entity.getRootVehicle"],
      eyePosition: ["Where a loyal trident returns to", "blocks", "Entity.getEyePosition"],
      width: ["The owner's width: from 1.21.2 a trident within width + 1 of a non-player owner's eyes is gone", "blocks", "Entity.getBbWidth"],
      alive: ["A dead owner loses its trident, and a dead player its pearl", "", "Entity.isAlive"],
      player: ["The owner is a player: a hook without one is gone, a dead one loses a pearl, and an owner that is none takes its trident back at width + 1", "", "Player"],
      spectator: ["A spectating owner loses its returning trident", "", "Entity.isSpectator"],
      removed: ["The owner is removed from the level: a fireball is discarded on its tick", "", "Entity.isRemoved|removed"],
      wonGame: ["The player is in the end credits: removed, so not alive, yet from 1.21.9 its pearl flies on; absent: false", "", "ServerPlayer.wonGame"],
    },
  },
  NativeProjectileState: {
    family: "projectile",
    fields: {
      position: ["Where the projectile stands as the tick starts", "blocks", "Entity.position|x"],
      velocity: ["Motion the family's tick starts from", "blocks a tick", "Entity.deltaMovement"],
      gravity: ["Taken off the y motion by a thrown item, spit, a hook from 1.11 and a bullet with no bulletDelta; a rocket and a hurting projectile do not read it. Absent, the family's default of the release", "blocks a tick squared", "ThrowableProjectile.getDefaultGravity|getGravity"],
      airDrag: ["Inertia of a thrown item or hurting projectile out of water, of spit, and of a hook from 1.11; a rocket and a bullet do not read it. Absent, the family's default, a dangerous skull's 0.73F", "ratio", "AbstractHurtingProjectile.getInertia"],
      power: ["Added to a hurting projectile's motion each tick before 1.21; zero when absent", "blocks a tick squared", "AbstractHurtingProjectile.xPower"],
      accelerationPower: ["Added along a hurting projectile's motion from 1.21; absent 0.1, a wind charge's 0", "blocks a tick squared", "AbstractHurtingProjectile.accelerationPower"],
      dangerous: ["A blue wither skull: its default airDrag is 0.73F, and an explicit airDrag wins", "", "WitherSkull.isDangerous"],
      inWater: ["A thrown item or fireball takes 0.8F inertia, spit is discarded; a versioned tick recomputes it at the family's base tick", "", "Entity.isInWater"],
      ultraWarm: ["Sets lava's current at the base tick to 0.007, else 0.0023333", "", ULTRA_WARM],
      firstTick: ["A thrown item's first tick runs the bubble column scan from 1.21.2; true when absent, and any tick clears it", "", "Entity.firstTick"],
      kind: ["The family's tick: ProjectileKind::from_name_checked of the name; a name it does not know or an arrow's is refused, absent is the thrown item's tick", "", "binding: projectile_state takes it"],
      horizontalCollision: ["The last move was cut on x or z: a rocket's boost stops from 1.16.2, a flying hook stops", "", "Entity.horizontalCollision"],
      onGround: ["The last move ended on something below: a flying hook stops, and its ground count reads it from 1.16", "", "Entity.onGround"],
      shotAtAngle: ["A rocket shot from a crossbow takes no boost, from 1.14", "", "FireworkRocketEntity.isShotAtAngle", { since: "1.14", why: "1.13.2 FireworkRocketEntity.java:27-28 tracks only the item and the shooter; native/src/projectile.rs:1623" }],
      bobbing: ["The hook is in FishingHook's BOBBING state: the tick pulls it to the surface instead of the flying branch", "", "FishingHook.currentState", HOOK_STATES_SINCE],
      bobRandom: ["The bobbing hook's random.nextFloat() for this tick's pull; 0 when absent, which pulls nothing", "ratio", "FishingHook.tick", { ...HOOK_STATES_SINCE, range: "0..1" }],
      noPhysics: ["Entity.move passes through blocks, for a hook or rocket, and a rocket acts on no hit", "", "Entity.noPhysics"],
      stuckSpeedMultiplier: ["What a cobweb or powder snow the last move ended in left on a hook or rocket: its next Entity.move multiplies its movement by it and clears it; zero when absent", "ratio", "Entity.stuckSpeedMultiplier"],
      boundingBox: ["The box a rocket's or hook's Entity.move goes by before 1.17, which its position follows; absent, made from the position", "", "Entity.getBoundingBox", { until: "1.16.5", why: "source-cache 1.16.5 Entity.java:611-612 moves the box and places the entity from it, 1.17 Entity.java:539 sets the position; native/src/projectile.rs:2061" }],
    },
  },
  NativeProjectileTarget: {
    family: "projectile",
    fields: {
      id: ["Entity.getId: what the hits, `pierced`, `ignored` and `hooked` name", "id", "Entity.getId"],
      rootVehicle: ["Entity.getRootVehicle's id, compared with the owner's by leftOwner and the immunity from 1.16; its own id when absent", "id", "Entity.getRootVehicle"],
      position: ["Entity.position: a hit's location before 1.21.2, a hooked hook's x and z, the order of one step's hits from 1.21.11, and the base of a box made from width and height", "blocks", "Entity.position|x"],
      boundingBox: ["The box the search and the ray meet, as it is; absent, width and height make it and must both be there", "", "Entity.getBoundingBox"],
      width: ["The pose's width before scale, for target_hitbox when there is no boundingBox", "blocks", "EntityDimensions.width"],
      height: ["The pose's height before scale, for target_hitbox when there is no boundingBox", "blocks", "EntityDimensions.height"],
      scale: ["LivingEntity.getScale, multiplied into width and height when there is no boundingBox; 1 when absent", "ratio", "LivingEntity.getScale"],
      pickable: ["Entity.isPickable: false rules it out of every family's query; true when absent", "", "Entity.isPickable"],
      alive: ["Entity.isAlive: false rules it out from 1.9 for an arrow, always for a rocket, from 1.16 for the others, and ends a hook's ride from 1.21.9; true when absent", "", "Entity.isAlive"],
      spectator: ["A spectator: ruled out from 1.9 for an arrow, from 1.14 for the others", "", "Entity.isSpectator"],
      friendly: ["A player the owning player may not harm, for an arrow or trident: from 1.21.2 no candidate, before that a hit on it drops the whole hit", "", "Player.canHarmPlayer"],
      invulnerablePlayer: ["1.8's creative player: an arrow's hit on it drops the whole hit", "", "Abilities.invulnerable", { until: "1.8.9", why: "1.8.8 AbstractArrow.java:224 tests abilities.invulnerable, 1.9.4 AbstractArrow.java:211 no longer does" }],
      hurt: ["What Entity.hurt answers: an arrow or trident turns back when false and the reaction reads refused; true when absent", "", "Entity.hurtServer|hurt"],
      enderman: ["An enderman: an arrow or trident's hit reads evaded, by the release's rule of what it answered", "", "EntityType|EntityTypes.ENDERMAN"],
      deflects: ["The type deflects the projectile: the tag on refusal in 1.20.3 and 1.20.4, ProjectileDeflection first from 1.20.5", "", "EntityTypeTags|Entity.DEFLECTS_ARROWS|deflection"],
      deflectRandom: ["random.nextFloat() that turns a deflected arrow or trident by up to 360 degrees; 0 when absent", "ratio", "AbstractArrow.deflect", { until: "1.20.4", why: "source-cache 1.20.3 AbstractArrow.java:412-413 draws it, 1.20.5 AbstractArrow.java:401 deflects by REVERSE; native/src/projectile_hits.rs:537", range: "0..1" }],
      noPhysics: ["Entity.noPhysics: a fireball, wind charge or shulker bullet passes through it", "", "Entity.noPhysics"],
      item: ["An ItemEntity: a fishing hook takes it though it is not pickable", "", "FishingHook.canHitEntity|checkCollision"],
      windCharge: ["A wind charge: wind charges pass through it", "", "WindCharge|AbstractWindCharge.canHitEntity"],
      endCrystal: ["An end crystal: wind charges pass through it from 1.20.5", "", "AbstractWindCharge.canHitEntity"],
      dragonBox: ["A dragon part's dragon box, which the search must also meet for in_search to give the part; ProjectileScene.dragonParts fills it", "", "binding: ProjectileTarget::in_search reads it", { until: "1.21.3", why: "source-cache 1.21.3 Level.java:579-584 walks a part through its dragon, 1.21.4 Level.java:583 through dragonParts()" }],
    },
  },
  NativeProjectileTickResult: {
    family: "projectile",
    fields: {
      position: ["Where the projectile stands after the tick", "blocks", "Entity.position|x"],
      velocity: ["Motion for the next tick, after the family's tick and base tick", "blocks a tick", "Entity.deltaMovement"],
      blockHit: ["The family's hit test found a block or the border this tick; a hook's border hit does not count, and a noPhysics rocket's neither", "", "binding: tick_family hands it back"],
      inWater: ["Entity.isInWater after the tick: the state's own on an unversioned call, the base tick's on a versioned one", "", "Entity.isInWater"],
      firstTick: ["False after any tick, for the next state", "", "Entity.firstTick"],
      horizontalCollision: ["The move was cut on x or z, for the next state", "", "Entity.horizontalCollision"],
      onGround: ["The move ended on something below, for the next state", "", "Entity.onGround"],
      bobbing: ["The hook is bobbing after the tick, for the next state", "", "FishingHook.currentState", HOOK_STATES_SINCE],
      discarded: ["It is gone: a hit, its family's end (spit in water, a rocket past lifeTime, a lost hook) or 64 blocks below the world", "", "binding: tick_family and tick_projectile_with_targets hand it back"],
      stuckSpeedMultiplier: ["The multiplier a cobweb or powder snow the hook's or rocket's move ended in left, for the next state; zero otherwise", "ratio", "Entity.stuckSpeedMultiplier"],
      boundingBox: ["The box move_projectile left, for the next state; absent from 1.17 and for families that do not use it", "", "Entity.getBoundingBox", { until: "1.16.5", why: "source-cache 1.16.5 Entity.java:611-612 moves the box and places the entity from it, 1.17 Entity.java:539 sets the position; native/src/projectile.rs:2061" }],
    },
  },
  NativeRocketHolder: {
    family: "projectile",
    fields: {
      position: ["Where the holder stands: an attached rocket stands here, at the hand when gliding", "blocks", "Entity.position|x"],
      velocity: ["The holder's motion after the boost, which the rocket takes", "blocks a tick", "Entity.deltaMovement"],
      gliding: ["LivingEntity.isFallFlying: a gliding player's rocket stands at the hand from 1.18.2", "", "LivingEntity.isFallFlying"],
      player: ["Only a player holds the rocket off its centre; true when absent", "", "Entity.getHandHoldingItemAngle"],
      yawDegrees: ["The holder's yaw, which places the hand 80 degrees to the side, half a block out; 0 when absent", "degrees", "Entity.yRot"],
      leftHand: ["The hand with the rocket is the left: the hand's side turns to -80 degrees", "", "Entity.getHandHoldingItemAngle"],
    },
  },
  NativeStabHit: {
    family: "player",
    fields: {
      knockback: ["The weapon's getKnockback on a server state: only above 0 counts, which from 26.2 takes the recoil's 0.6 of the horizontal motion a second time", "attribute value", "LivingEntity.stabAttack"],
    },
  },
  NativeTridentHitTick: {
    family: "trident",
    fields: {
      trident: ["The flight result of this server tick", "", "binding: simulate_versioned_trident_hit_tick hands it back"],
      noPhysics: ["The state's value after the tick, which a return to the owner sets; pass it in the next state", "", "AbstractArrow.isNoPhysics"],
      ...hits,
    },
  },
  NativeTridentState: {
    family: "trident",
    fields: {
      ...embeddedState("trident", "0.99F", "ThrownTrident.getWaterInertia"),
      dealtDamage: ["The trident hit something, or stayed over 4 ticks in the ground: it finds no more targets, and with loyalty it returns", "", "ThrownTrident.dealtDamage"],
      ...embeddedRest("trident"),
    },
  },
  NativeTridentTickResult: {
    family: "trident",
    fields: {
      ...embeddedResult("trident"),
      dealtDamage: ["The trident's dealtDamage after the tick, for the next state", "", "ThrownTrident.dealtDamage"],
      ...embeddedResultEnd,
    },
  },
  TargetEntity: {
    family: "adapter",
    fields: {
      id: ["The entity's id: targetOf's target id, and what rootOf ends on", "id", "binding: NativeProjectilePhysics.targetOf reads it"],
      type: ["\"player\" gives the pose sizes of poseSize and makes ownerOf's owner a player", "", "binding: NativeProjectilePhysics.poseSize and ownerOf read it"],
      name: ["The registry name targetOf tests for enderman, deflects, item, windCharge and endCrystal", "", "binding: NativeProjectilePhysics.targetOf reads it"],
      position: ["Copied as the target's position, and raised to the eyes for an owner", "blocks", "binding: NativeProjectilePhysics.targetOf and ownerOf read it"],
      width: ["minecraft-data's width, which poseSize keeps as a float unless the pose changes it; 0 when absent", "blocks", "binding: NativeProjectilePhysics.poseSize reads it"],
      height: ["minecraft-data's height, which poseSize keeps as a float unless the pose changes it; 0 when absent", "blocks", "binding: NativeProjectilePhysics.poseSize reads it"],
      metadata: ["Synced data by index: poseSize and ownerOf read index 6, the pose, from 1.14", "", "binding: NativeProjectilePhysics.poseSize and ownerOf read it"],
      vehicle: ["What it rides: rootOf follows it up to the root vehicle's id", "", "binding: NativeProjectilePhysics.rootOf reads it"],
      attributes: ["scaleOf takes the value of minecraft:scale, scale or minecraft:generic.scale from 1.20.5", "attribute value", "binding: NativeProjectilePhysics.scaleOf reads it"],
      isValid: ["False makes the target not alive, and the owner too unless ownerAlive says", "", "binding: NativeProjectilePhysics.targetOf and ownerOf read it"],
    },
  },
  ProjectileScene: {
    family: "adapter",
    fields: {
      entities: ["The candidates targetsOf turns into targets, in this order, with the owner among them", "", "binding: NativeProjectilePhysics.targetsOf reads it"],
      owner: ["The entity ownerOf turns into the owner; left out, the tick has no owner", "", "binding: NativeProjectilePhysics.ownerOf reads it"],
      ownerAlive: ["The owner's alive; absent, `owner.isValid !== false`", "", "binding: NativeProjectilePhysics.ownerOf reads it"],
      ownerSpectator: ["The owner's spectator flag; false when absent", "", "binding: NativeProjectilePhysics.ownerOf reads it"],
      targetOverrides: ["Per entity id, fields spread over what targetOf derived: the server's say, such as hurt or spectator", "id", "binding: NativeProjectilePhysics.targetOf and targetsOf read it"],
      dragonParts: ["Parts of each dragon id: targetsOf makes the dragon unpickable and lists the parts, after it until 1.21.3 and last from 1.21.4", "id", "binding: NativeProjectilePhysics.targetsOf reads it"],
    },
  },
};
