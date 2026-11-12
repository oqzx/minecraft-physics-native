// What a reviewer read of the sites of generated/movement-sites.json no citation proves (B046). A rule's `match` is on a
// site's id; `why` says what the number is: `derived` (a value vanilla computes or names, with the expression), `engine`
// (the engine's own, no number of vanilla's), or the source. A rule nothing matches any more shows in `staleReviews` and
// fails test/generated-inventories.test.cjs. A site left out stays `open`: no one has shown where it comes from.

const DISPOSITIONS = [
  { rule: "degrees", match: /\|(0\.017_?453_?292(_f32)?|57\.295_?776(_367_187_5)?|3\.1415927|1\.5707964)\|/, why: "derived: Mth.DEG_TO_RAD, Mth.RAD_TO_DEG and (float) Math.PI; a tree that prints the name or the expression holds no literal of it" },
  { rule: "sine scale", match: /\|(10430\.378(350470453)?|65536\.0)\|/, why: "derived: Mth's table index, the float 10430.378F to 1.21.10 and 65536.0 / (2 * Math.PI) from 1.21.11, by the gate in math.rs and mth.ts" },
  { rule: "liquid height", match: /legacy_fluid\.rs#(height_percent|own_height)\|f32\|9\.0\|/, why: "1.12.2 LiquidBlock.java:158 (height + 1) / 9.0F" },
  { rule: "java seed", match: /napi_minecart\.rs#whole_seed/, why: "engine: 2^53 - 1, the largest whole number a JavaScript number holds, for a raw seed's check" },
  { rule: "held rocket", match: /projectile\.rs#stay_with_holder\|\|80\.0/, why: "Entity.getHandHoldingItemAngle, cited at the function (1.19 Entity.java:1862-1871, 26.3 :2691-2699)" },
  { rule: "zombie nautilus", match: /ridden\.rs#movement_speed\|\|1\.100000023841858/, why: "26.3 ZombieNautilus.java: Attributes.MOVEMENT_SPEED, 1.100000023841858" },
  { rule: "strider", match: /ridden\.rs#(steering_speed\|\|0\.23|strider_move_speed\|\|0\.66)/, why: "1.19.3 Strider.java:72-73, the suffocating constants, read at :260-265" },
  { rule: "eye heights", match: /(player\.rs#eye_height|adapter\/player\.ts#(eyeHeight|legacyPlayerHeight)|adapter\/projectiles\.ts#ownerOf)\|\|(0\.4|1\.27|1\.62|1\.65)\|/, why: "Avatar's float poses (1.21.11 Avatar.java:32, 54-55): every reader narrows them but adapter/player.ts:1189, reported on B316's lane" },
  { rule: "profile fields", match: /simulation\.rs#default\||versioned\/\*\.rs#rules\|/, why: "a VersionPhysicsRules field, held as f64 and narrowed at use by java_float; each release's value is checked in `profiles`" },
  { rule: "collision query", match: /#(collide_with_world|collide_vehicle|legacy_entity_colliders)\|\|1\.0e-5\|f64/, why: "engine: a margin below the swept box when asking the world for candidates; each candidate is then clipped by vanilla's own axis steps, so a box it adds moves nothing" },
  { rule: "jump strength", match: /vehicle\.rs#tick_horse\|\|0\.42\|f64/, why: "26.3 LivingEntity.java:171 BASE_JUMP_POWER 0.42F: the f64 default of a jump strength, narrowed by `strength as f32` on the next line" },
  { rule: "friction table", match: /adapter\/blocks\.ts#const FRICTION\|(blue_ice\|0\.989|(ice|packed_ice|frosted_ice)\|0\.98|(slime_block|slime)\|0\.8)\|number/, why: "Blocks.java friction(0.989F) at 1.14.4 :1316, the ices' friction(0.98F) at 1.21.11 :1598, 1862, 1976, slime's friction(0.8F) at 26.2 :1493: the adapter's double, narrowed where the engine reads it (`slipperiness_at(..) as f32`, java_float, compute_modified_friction)" },
  { rule: "default friction", match: /(native\/world\.rs#(full_block|set_block_voxel_shape|set_moving_block)\|slipperiness|native\/world\.rs#slipperiness_at\||native\/napi_api\.rs#block_at\|slipperiness|adapter\/blocks\.ts#\(file\)\|friction|adapter\/blocks\.ts#updatesAt\|slipperiness)\|0\.6\|(f64|number)$/, why: "BlockBehaviour's default friction 0.6F (1.21.11 BlockBehaviour.java:458, 1.12.2 Block.java:113), held as the f64 a shape carries and narrowed at every read as the table's are" },
  { rule: "dragon fly speed", match: /native\/napi_moving_entity\.rs#dragon_state\|fly_speed\|0\.6\|f64/, why: "AbstractDragonPhaseInstance.getFlySpeed's 0.6F (1.14.4 AbstractDragonPhaseInstance.java:41), the default of a field the caller gives, narrowed by `as f32` beside it" },
  { rule: "hash load factor", match: /adapter\/base\.ts#iterationOrder\|\|0\.75\|/, why: "jdk: java.util.HashMap's DEFAULT_LOAD_FACTOR 0.75f and fastutil's Hash.DEFAULT_LOAD_FACTOR, the containers of an attribute's modifiers; no literal of vanilla's" },
  { rule: "strict math", match: /native\/trig\.rs#.*\|int$/, why: "jdk: HotSpot's Math.sin and cos on x86_64, Intel LIBM, ported in trig.rs: its bit masks, shifts and exponents, no number of vanilla's" },
  { rule: "java arithmetic", match: /adapter\/mth\.ts#(javaInt|javaLongLow16|\(file\)|legacy|modern)\|.*\|int$/, why: "jdk: a Java int's range and a long's low bits in a JavaScript number, and the 65536 cells of Mth's sine table, 1.21.11 Mth.java" },
  { rule: "java random", match: /adapter\/rain\.ts#(next|nextInt|nextDouble|\(file\))\|.*\|int$/, why: "jdk: java.util.Random's 48-bit generator and nextDouble's 26 and 27 bits" },
  { rule: "shared buffer", match: /native\/napi_player\.rs#(const [A-Z_]+|nearby_entity)\|.*\|int$/, why: "engine: a field's slot in the buffers the adapter and NativePlayer share, no number of vanilla's" },
  { rule: "checkpoint", match: /native\/world\/checkpoint\.rs#.*\|int$/, why: "engine: the byte layout of a world checkpoint" },
  { rule: "engine limits", match: /adapter\/world\.ts#(\(file\)|const CACHE_LIMIT|const MAX_SYNCED_CELLS)\|.*\|int$/, why: "engine: the size of the adapter's own caches, in powers of two" },
  { rule: "release numbers", match: /native\/entity_travel\.rs#.*\|int$/, why: "engine: a release's own number, the first release of a kind, each checked by the registry test in test/mineflayer.test.cjs" },
  { rule: "attribute hashes", match: /adapter\/base\.ts#(iterationOrder|modifierIdHash)\|.*\|int$/, why: "jdk: HashMap's 16 buckets and its shift, fastutil's 32 slots, a UUID's hashCode, as the method's own comment says" },
  { rule: "registry ids", match: /adapter\/base\.ts#(const EFFECT_NAMES|const ENCHANTMENT_NAMES|effectAmplifierByName)\|.*\|int$/, why: "engine: the index the binding gives each effect and enchantment it reads by name, or the shifts that sign-extend the byte an amplifier was before 1.20.5" },
  { rule: "chunk coordinates", match: /adapter\/(base|shulkers)\.ts#(syncMovingBlocks|forgetColumn)\|.*\|int$/, why: "derived: a chunk's 16 blocks, the >> 4 of SectionPos.blockToSectionCoord" },
  { rule: "legacy metadata", match: /adapter\/legacy\.ts#.*\|int$/, why: "the bits of a block's metadata before the flattening and the sides they name, read at each block's getStateFromMeta in the 1.8.8 and 1.12.2 trees, cited at the shapes; a shape's own values are in generated/block-shapes-legacy" },
  { rule: "modifier ids", match: /adapter\/player\.ts#(frostModifierId|sprintModifierId)\|.*\|int$/, why: "digits of a UUID in a string: 1.17 LivingEntity.java SPEED_MODIFIER_POWDER_SNOW_UUID, 1.16.5 LivingEntity.java:112 SPEED_MODIFIER_SPRINTING_UUID" },
  { rule: "tracker index", match: /adapter\/minecarts\.ts#hurtIndex\|.*\|int$/, why: "the entity-data index of a minecart's hurt time before 1.9, 1.8.8 AbstractMinecart.java:151 track(17, ..)" },
  { rule: "legacy random", match: /native\/minecart\.rs#next\|\|25_214_903_917\|int$/, why: "26.3 LegacyRandomSource.java:40 oldSeed * 25214903917L + 11L, java.util.Random's multiplier" },
  { rule: "strongest draw", match: /native\/minecart\.rs#explosion_rays\|\|16_777_215\.0\|/, why: "derived: 2^24 - 1, the most next(24) gives, by 26.3 BitRandomSource.java:43-44 nextFloat's 5.9604645E-8F: the strongest of a blast's rays" },
  { rule: "block effect codes", match: /native\/world\.rs#(code|from_code)\|\|\d+\|int$/, why: "engine: the number the binding gives each BlockEffect, no number of vanilla's" },
  { rule: "blast reach", match: /adapter\/vehicles\.ts#\(file\)\|\|(0\.225|1\.3)\|number$/, why: "derived: a ray's 0.22500001F a step and its 0.7F + 0.6F of the radius at most (26.3 ServerExplosion.java:124-152), rounded up for the reach of the world sync alone" },
  { rule: "freeze default", match: /napi_api\.rs#from\|ticks_required_to_freeze\|140\|int/, why: "Entity.getTicksRequiredToFreeze, 26.3 Entity.java; the caller's wins" },
];

/** The static stand-in for a value a caller or the server has, by `module#field=value`: what makes it vanilla's own. */
const DEFAULTS = {
  "native/napi_api.rs#ticks_required_to_freeze=140": "Entity.getTicksRequiredToFreeze, 26.3 Entity.java; the caller's wins",
  "native/napi_api.rs#safe_fall_distance=3": "26.3 Attributes.java:83 SAFE_FALL_DISTANCE's 3.0; the synced attribute wins",
  "native/napi_api.rs#border_absolute_max_size=29999984": "26.3 WorldBorder.java:50; the packet's wins",
  "native/napi_api.rs#half_width=0.7": "the release-less simulateHorseTick reads no box: 1.8.8 AbstractHorse.java:209's 1.4F wide, echoed back; the versioned ticks take RiddenMobKind::dimensions",
  "native/napi_api.rs#height=1.6": "the release-less simulateHorseTick reads no box: AbstractHorse's 1.6F, echoed back",
  "native/napi_moving_entity.rs#gravity=0.04": "MoveRules::MINECART's own, the rules it extends; a pushed entity's caller gives its own",
  "native/napi_moving_entity.rs#max_step=0.6": "26.3 Attributes.java:87 STEP_HEIGHT's 0.6; the state's wins",
  "native/napi_moving_entity.rs#fluid_jump_threshold=0.4": "Entity.getFluidJumpThreshold's 0.4 of an eye at least that high; the state's wins",
  "native/napi_moving_entity.rs#fly_speed=0.6": "26.3 AbstractDragonPhaseInstance.java:40; the state's wins",
  "native/napi_player.rs#item_use_speed_multiplier=0.2": "26.3 UseEffects.java:10 DEFAULT's 0.2F; the item's wins",
  "adapter/blocks.ts#FRICTION[name]=0.6": "1.12.2 Block.java:113 slipperiness 0.6F, every block outside the table",
  "adapter/blocks.ts#slipperiness=0.6": "1.12.2 Block.java:113 slipperiness 0.6F, an update with no shape",
  "adapter/player.ts#seaLevel=63": "1.20.1 Level.java:695-697; a state's wins",
  "adapter/blocks.ts#=0.6": "1.12.2 Block.java:113 slipperiness 0.6F, an update with no shape",
  "adapter/player.ts#=0.3": "26.3 Attributes.java:85 SNEAKING_SPEED's 0.3; the synced attribute wins",
  "adapter/player.ts#=0.6": "26.3 Attributes.java:87 STEP_HEIGHT's 0.6; the synced attribute wins",
  "adapter/player.ts#=3": "26.3 Attributes.java:83 SAFE_FALL_DISTANCE's 3.0; a state's and the synced attribute win",
  "adapter/player.ts#height=256": "engine: a bot without a game height, the old level's 256 for the rain's top; the server's login wins",
  "adapter/player.ts#sprintWindow=7": "26.3 Options.java:1032 IntRange(0, 10), 7; a client's option wins",
  "adapter/player.ts#stepHeight=0.6": "26.3 Attributes.java:87 STEP_HEIGHT's 0.6; the synced attribute wins",
  "adapter/rain.ts#temperature=0.5": "engine: a biome the data gives no temperature; 0.5 is above every freezing height's threshold",
  "adapter/vehicles.ts#width=0.9": "engine: a mob with no dimensions, for the reach of the world sync alone",
  "adapter/vehicles.ts#height=1.8": "engine: a mob with no dimensions, for the reach of the world sync alone",
  "adapter/vehicles.ts#explosionPowerBase=4": "26.3 MinecartTNT.java:181 explosion_power's default 4.0F, for the reach of the world sync alone; the cart's wins",
  "native/napi_minecart.rs#health=5": "1.8.8 ItemEntity.java:24 health = 5, 1.21.2 ItemEntity.java:57 the same; the caller's wins",
};

/** The classes each field of a VersionPhysicsRules comes from, by their Mojang names (the legacy trees take them too). */
const PROFILE_SOURCES = {
  gravity: ["LivingEntity", "Attributes"],
  water_drag: ["LivingEntity"],
  water_sprint_drag: ["LivingEntity"],
  air_drag: ["LivingEntity"],
  vertical_drag: ["LivingEntity", "Entity"],
  fluid_vertical_drag: ["LivingEntity"],
  water_flow_scale: ["Entity", "Level"],
  lava_flow_scale: ["Entity", "Level"],
  ultra_warm_lava_flow_scale: ["Entity", "Level", "LavaFluid"],
  water_input_speed: ["LivingEntity"],
  lava_input_speed: ["LivingEntity"],
  step_height: ["Entity", "LivingEntity", "Player", "Attributes"],
  swimming_vertical_steering: ["Player", "LivingEntity"],
  swimming_downward_steering: ["Player", "LivingEntity"],
};

module.exports = { DISPOSITIONS, DEFAULTS, PROFILE_SOURCES };
