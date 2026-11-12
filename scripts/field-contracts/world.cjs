// The field contracts of the world, its pistons, the remaining binding results and the adapter's own interfaces (scripts/field-contracts.cjs has the format).
const box = (axis, end) => `${end === "min" ? "lower" : "upper"} ${axis} face of the box, relative to its block's cell corner`;
const generatedBox = (axis, end) => `${end === "min" ? "lower" : "upper"} ${axis} face of one box of the shape table, in the block's cell`;

module.exports = {
  NativeAabb: {
    family: "player",
    fields: {
      min: ["corner with the lowest x, y and z of an absolute or cell-relative box, as the holding field says", "", "AABB.minX"],
      max: ["corner with the highest x, y and z of the box; From<NativeAabb> copies both corners unchecked", "", "AABB.maxX"],
    },
  },
  NativeAttributeModifier: {
    family: "player",
    fields: {
      amount: ["amount the modifier adds, or scales by, in MovementSpeedAttribute.value", "attribute value", "AttributeModifier.amount|getAmount"],
      operation: ["add_value, add_multiplied_base or add_multiplied_total; TryFrom refuses any other name", "", "AttributeModifier.operation|getOperation"],
    },
  },
  NativeCollisionContext: {
    family: "world",
    fields: {
      bottom: ["height of the entity's feet the blocks compare with their tops in isAbove", "blocks", "EntityCollisionContext.entityBottom"],
      descending: ["whether the entity holds the sneak key, which lets scaffolding and powder snow drop it through", "", "binding: entity_collision_shapes copies it to EntityCollisionContext.descending"],
      fallDistance: ["distance the entity has fallen; over 2.5 powder snow catches it in a lower box", "blocks", "Entity.fallDistance"],
      walksOnPowderSnow: ["whether the entity may stand on powder snow: a tagged mob or leather boots", "", "binding: entity_collision_shapes copies it to EntityCollisionContext.walks_on_powder_snow"],
      fallingBlock: ["whether the entity is a falling block, which powder snow holds up wherever its feet are; absent: false", "", "binding: entity_collision_shapes copies it to EntityCollisionContext.falling_block"],
    },
  },
  NativeBlockPosition: {
    family: "world",
    fields: {
      x: ["block x of the cell a state names, such as its supporting block", "blocks", "Vec3i.getX"],
      y: ["block y of the cell a state names, such as its supporting block", "blocks", "Vec3i.getY"],
      z: ["block z of the cell a state names, such as its supporting block", "blocks", "Vec3i.getZ"],
    },
  },
  NativeBlockProperty: {
    family: "world",
    fields: {
      name: ["property name of a block state, unique and non-empty in an update (validate_block_properties)", "", "Property.getName"],
      value: ["property value as text; minecart rails and scaffolding read it back from the cell", "", "StateHolder|AbstractStateHolder.getValue"],
    },
  },
  NativeBlockShape: {
    family: "world",
    fields: {
      minX: [box("x", "min"), "blocks", "AABB.minX", { range: "-1..2" }],
      minY: [box("y", "min"), "blocks", "AABB.minY", { range: "-1..2" }],
      minZ: [box("z", "min"), "blocks", "AABB.minZ", { range: "-1..2" }],
      maxX: [box("x", "max"), "blocks", "AABB.maxX", { range: "-1..2" }],
      maxY: [box("y", "max"), "blocks", "AABB.maxY", { range: "-1..2" }],
      maxZ: [box("z", "max"), "blocks", "AABB.maxZ", { range: "-1..2" }],
      slipperiness: ["block friction the box carries, finite and not below 0 (valid_block_shape)", "ratio", "Block.getFriction"],
      effect: ["BlockEffect code of the box, which the engine's contact and inside-block effects read; absent: 0, no effect", "id", "binding: BlockEffect::from_code numbers the effects, refusing anything above 30", { range: "0..30" }],
      bounceRestitution: ["landing restitution, which only setBlockShape and the first box of setBlockShapes copy; absent: 0", "ratio", "Block.getBounceRestitution"],
      suppressesBounce: ["block cancels a landing's bounce, which only setBlockShape and the first box of setBlockShapes copy; absent: false", "", "BlockTags.SUPPRESSES_BOUNCE"],
      fallDamageResetting: ["block is a full cube for the fall-damage clip, which only setBlockShape and the first box of setBlockShapes copy; absent: false", "", "BlockTags.FALL_DAMAGE_RESETTING"],
    },
  },
  NativeBlockState: {
    family: "world",
    fields: {
      present: ["the world holds a cell here in a loaded chunk (contains_block), air included if it was written", "", "binding: NativeWorld.block_at reads WorldSnapshot::contains_block"],
      id: ["id stored for the cell: the adapter's block type id, or a state id when stateVersion names its release; 0 for nothing stored", "id", "binding: NativeWorld.block_at hands back BlockState.id"],
      stateVersion: ["release whose block state table `id` belongs to, absent when none was stored", "", "binding: NativeWorld.block_at reads WorldSnapshot::block_state_id"],
      properties: ["property pairs stored for the cell, empty when none were sent", "", "binding: NativeWorld.block_at reads WorldSnapshot::block_properties"],
      tags: ["caller's tag bits: rail 1, powered rail 2, activator rail 4, redstone conductor 8, FENCE_LIKE 16, WALL_OR_GATE 32", "bits", "binding: BlockState.tags, read by minecart.rs and Entity.getOnPos's port in simulation.rs"],
      fluid: ["fluid of the cell: 0 none, 1 water, 2 lava, 3 waterlogged, a water state held in another block", "id", "binding: fluid_code numbers FluidState", { range: "0..3" }],
      fluidTags: ["caller's fluid tag bits, stored and handed back; no engine path reads them", "bits", "binding: NativeWorld.block_at hands back BlockState.fluid_tags"],
      fluidLevel: ["LiquidBlock level as sent, which the height reads as 8 minus it for 1 to 7 on a fluid neither source nor falling", "count", "LiquidBlock.LEVEL", { range: "0..15" }],
      fluidSource: ["fluid is a source: its own height is 8/9, and a block sturdy below raises its camera height to 1", "", "FluidState.isSource"],
      fluidFlow: ["flow as the caller computed it, added to an entity's current by the fluid push", "", "FluidState.getFlow"],
      fluidFalling: ["fluid is falling, which gives it the 8/9 own height", "", "FlowingFluid.FALLING"],
      fallDamageResetting: ["cell is a full cube for the fall-damage clip of a move or projectile", "", "BlockTags.FALL_DAMAGE_RESETTING"],
      effect: ["BlockEffect code of the cell's first collision box, 0 with no box", "id", "binding: block_effect_code numbers the first box's BlockEffect", { range: "0..30" }],
      slipperiness: ["friction of the first collision box; 0.6, the block default, with no box", "ratio", "Block.getFriction"],
      bounceRestitution: ["restitution a landing on the cell takes from 26.2 (Entity.bounce's port)", "ratio", "Block.getBounceRestitution"],
      suppressesBounce: ["cell cancels a landing's bounce (in vanilla, the #suppresses_bounce tag)", "", "BlockTags.SUPPRESSES_BOUNCE"],
      shape: ["first collision box in the cell, absent for no collision box", "", "BlockBehaviour|Block.getCollisionShape"],
      outlineShape: ["first outline box in the cell, read by outline clips", "", "BlockBehaviour|Block.getShape"],
      interactionShape: ["first interaction box in the cell, absent when the cell holds none", "", "BlockBehaviour|Block.getInteractionShape"],
      blockEntity: ["block entity kind stored for the cell, such as minecraft:campfire; handed back only, no engine path reads it", "", "binding: NativeWorld.block_at reads WorldSnapshot::block_entity"],
    },
  },
  NativeBlockUpdate: {
    family: "world",
    fields: {
      x: ["block x of the cell the update writes", "blocks", "Vec3i.getX"],
      y: ["block y of the cell the update writes", "blocks", "Vec3i.getY"],
      z: ["block z of the cell the update writes", "blocks", "Vec3i.getZ"],
      id: ["block id stored for the cell, a state id if stateVersion is sent; absent: 0, or the kept id on a metadata-only update; present without `shape`: a full cube", "id", "binding: block_update_state turns it into BlockState.id, which is_air_at compares with 0"],
      stateVersion: ["registered release whose state table `id` belongs to; others are refused, and the batch's last update of a cell decides it", "", "binding: VersionedBlockStateId::new refuses a release outside the registry"],
      blockEntity: ["block entity kind stored for the cell, non-empty; the batch's last update of a cell decides it", "", "binding: apply_block_updates calls WorldSnapshot::set_block_entity"],
      removeBlockEntity: ["drops the cell's block entity when the batch's last update of the cell sends no blockEntity; absent: false", "", "binding: apply_block_updates calls WorldSnapshot::remove_block_entity"],
      properties: ["property pairs stored for the cell, read by rails and scaffolding; only the batch's last update of a cell counts, and one without them writes none", "", "binding: apply_block_updates calls WorldSnapshot::set_block_properties"],
      tags: ["caller's tag bits: rail 1, powered rail 2, activator rail 4, redstone conductor 8, FENCE_LIKE 16, WALL_OR_GATE 32; absent: 0", "bits", "binding: block_update_state stores BlockState.tags for minecart.rs and simulation.rs"],
      shape: ["collision box of this update; several updates of a cell in a batch add one box each", "", "BlockBehaviour|Block.getCollisionShape"],
      outlineShape: ["outline box of this update; absent: the collision box, or a full cube when only id came", "", "BlockBehaviour|Block.getShape"],
      interactionShape: ["interaction box of this update; absent: the outline box, or a full cube when only id came", "", "BlockBehaviour|Block.getInteractionShape"],
      fluid: ["fluid of the cell: 0 none, 1 water, 2 lava, 3 waterlogged; anything else is refused; absent: 0", "id", "binding: block_update_state decodes it into FluidState", { range: "0..3" }],
      fluidTags: ["caller's fluid tag bits, stored and handed back; no engine path reads them; absent: 0", "bits", "binding: block_update_state stores BlockState.fluid_tags"],
      fluidLevel: ["LiquidBlock level as sent, read by the fluid height for 1 to 7; absent: 0", "count", "LiquidBlock.LEVEL", { range: "0..15" }],
      fluidSource: ["fluid is a source, refused with no fluid or when falling; absent: true for a fluid sent without fluidLevel", "", "FluidState.isSource"],
      fluidFlow: ["flow the caller computed, added to an entity's current by the fluid push; absent: zero", "", "FluidState.getFlow"],
      fluidFalling: ["fluid is falling, which gives it the 8/9 own height; refused on a source or with no fluid; absent: false", "", "FlowingFluid.FALLING"],
      waterBlock: ["Blocks.WATER itself, whose speed factor Entity.getBlockSpeedFactor keeps without reading the block below; absent: false", "", "Entity.getBlockSpeedFactor"],
      fallDamageResetting: ["cell is a full cube for the fall-damage clip of a move or projectile; absent: false", "", "BlockTags.FALL_DAMAGE_RESETTING"],
      playerFallDamageResetting: ["end gateway or end portal, full cubes for a player's fall-damage clip from 1.21.9; absent: false", "", "ClipContext.FALLDAMAGE_RESETTING"],
      bounceRestitution: ["restitution a landing on the cell takes from 26.2, refused if negative or not finite; absent: 0", "ratio", "Block.getBounceRestitution"],
      suppressesBounce: ["cell cancels a landing's bounce; the adapter sets it for the honey block; absent: false", "", "BlockTags.SUPPRESSES_BOUNCE"],
      suffocating: ["overrides BlockState.isSuffocating of the cell; absent: a single full collision cube suffocates", "", "BlockBehaviour|BlockState.isSuffocating|isViewBlocking"],
      inWall: ["overrides the block test of Entity.isInsideWall before 1.14 where it is not `suffocating`; absent: `suffocating`", "", "binding: WorldSnapshot::is_in_wall_block reads it, else is_suffocating"],
      bottomFaceSturdy: ["overrides isFaceSturdy(DOWN) of the cell, which a source's camera height reads; absent: from the collision boxes", "", "BlockBehaviour|BlockState.isFaceSturdy"],
      explosionResistance: ["Block.getExplosionResistance of the block, which a blast's ray loses 0.3 of and 0.3 more in each block it steps through; from 1.13 a fluid's 100 is the more where it is greater; refused if negative or not finite; absent: air", "explosion resistance", "Block|BlockBehaviour.getExplosionResistance|explosionResistance"],
      remove: ["clears the cell, ending any earlier update of it in the batch; every other field is then ignored; absent: false", "", "binding: apply_block_updates calls WorldSnapshot::remove_block"],
    },
  },
  NativeBuildIdentity: {
    family: "world",
    fields: {
      target: ["Rust target triple the addon was compiled for, which native/artifacts/provenance records too", "", "binding: build_identity reads MCPN_BUILD_TARGET from native/build.rs"],
      sourceHash: ["SHA-256 over the hashed listing of Cargo.toml, Cargo.lock, build.rs, data and src, line endings normalized", "", "binding: build_identity reads MCPN_SOURCE_HASH from native/build.rs"],
      crateVersion: ["version of the native crate at compile time", "", "binding: build_identity reads CARGO_PKG_VERSION"],
    },
  },
  NativeEntityPushResult: {
    family: "player",
    fields: {
      leftVelocity: ["left entity's velocity plus the impulse Entity.push gives it", "", "Entity.push"],
      rightVelocity: ["right entity's velocity plus the impulse Entity.push gives it", "", "Entity.push"],
    },
  },
  NativeEntityPushState: {
    family: "player",
    fields: {
      position: ["position of this side, whose x and z differences give the push; must be finite", "", "Entity.push"],
      velocity: ["velocity the impulse adds to; must be finite", "", "Entity.getDeltaMovement|velocityX"],
      noPhysics: ["either side having it cancels the whole push", "", "Entity.noPhysics"],
      vehicle: ["side has passengers (Entity.isVehicle), so it takes no push", "", "Entity.isVehicle"],
      pushable: ["from 1.19 a side that is not Entity.isPushable takes no push", "", "Entity.isPushable"],
      pushthrough: ["Entity.pushthrough before 1.17: the left side's scales both pushes by 1 minus it, the right side's is unused; refused outside 0..1", "ratio", "Entity.pushthrough", { range: "0..1" }],
    },
  },
  NativeEntityTravelSupport: {
    family: "player",
    fields: {
      family: ["path that moves the kind: player, boat, ridden, mob or unsupported (the game has it, the engine no path); a kind outside the inventory gets the fallback name of entity_travel_support", "", "binding: entity_travel_support names TravelFamily from entity_travel.rs"],
      available: ["registry of the release holds the kind", "", "binding: travel_support compares the release with the kind's first one"],
      since: ["first release with the kind, as major.minor.patch; absent outside the inventory", "", "binding: entity_travel_support formats the INVENTORY release"],
    },
  },
  NativeGeneratedShapeBox: {
    family: "world",
    fields: {
      minX: [generatedBox("x", "min"), "blocks", "AABB.minX"],
      minY: [generatedBox("y", "min"), "blocks", "AABB.minY"],
      minZ: [generatedBox("z", "min"), "blocks", "AABB.minZ"],
      maxX: [generatedBox("x", "max"), "blocks", "AABB.maxX"],
      maxY: [generatedBox("y", "max"), "blocks", "AABB.maxY"],
      maxZ: [generatedBox("z", "max"), "blocks", "AABB.maxZ"],
    },
  },
  NativeGeneratedShapePredicate: {
    family: "world",
    fields: {
      property: ["Java name of the property a shape method compares, such as HINGE (scripts/legacy-release-data.cjs)", "", "binding: legacy-release-data.cjs reads the getProperty comparison into generated data"],
      operator: ["== or !=, as the method's comparison has it; generated_method_shape_predicates_match_state treats others as false", "", "binding: legacy-release-data.cjs reads the getProperty comparison into generated data"],
      value: ["right side of the comparison, such as DoorHingeSide.RIGHT; the match compares its last dotted part, ignoring case", "", "binding: legacy-release-data.cjs reads the getProperty comparison into generated data"],
    },
  },
  NativeGeneratedShapeProvenance: {
    family: "world",
    fields: {
      sourceSha256: ["SHA-256 of the shape method's signature and body in the release's class, absent with no method of its own", "", "binding: legacy-release-data.cjs hashes the method text into generated data"],
      hasMethod: ["block's own class declares the method of this channel", "", "binding: legacy-release-data.cjs records whether the class declares it"],
      stateDependent: ["method's body reads a property or fluid state, or branches by a switch or a ternary, a textual test", "", "binding: legacy-release-data.cjs tests the method body by pattern"],
      propertyDependencies: ["properties the method body reads through getProperty, sorted", "", "binding: legacy-release-data.cjs collects the getProperty arguments"],
      candidateBoxCount: ["boxes the generated table lists for this channel of the block", "count", "binding: generate-native-data.cjs takes the length of the channel's boxes"],
    },
  },
  NativeGeneratedStateShape: {
    family: "world",
    fields: {
      status: ["exact (the boxes are the answer, none included), contextual (needs dependsOn, no boxes) or absent (no such method)", "", "binding: native_state_shape names the StateShape from scripts/java-oracle/ShapeDump.java"],
      dependsOn: ["what a contextual answer also needs: position, level or entity; empty otherwise", "", "binding: native_state_shape reads the DEPENDS_ mask bits"],
      boxes: ["boxes of the channel for an exact status, empty otherwise", "", "binding: native_state_shape copies the exact boxes"],
    },
  },
  NativeMovementSpeedAttribute: {
    family: "player",
    fields: {
      base: ["attribute's base value, the start of MovementSpeedAttribute.value", "attribute value", "AttributeInstance.getBaseValue"],
      modifiers: ["every modifier but the powder snow one; apply_frost recomputes the value through them when frost changes", "", "AttributeInstance.getModifiers"],
      frost: ["amount of the powder snow modifier, -0.05 times the freezing scale, added with add_value; absent: not held", "attribute value", "LivingEntity.tryAddFrost"],
    },
  },
  NativeMovement: {
    family: "player",
    fields: {
      from: ["where the move started", "blocks", "Entity.Movement"],
      to: ["where the move ended", "blocks", "Entity.Movement"],
      axisOrder: ["the movement whose axis order splits the move one axis at a time; absent: the move as a line", "blocks", "Entity.Movement"],
    },
  },
  NativeMovingPiston: {
    family: "piston",
    fields: {
      x: ["block x of the moving block's cell", "blocks", "BlockEntity.getBlockPos"],
      y: ["block y of the moving block's cell", "blocks", "BlockEntity.getBlockPos"],
      z: ["block z of the moving block's cell", "blocks", "BlockEntity.getBlockPos"],
      facing: ["piston's facing as Direction.from3DDataValue: down, up, north, south, west, east; refused above 5", "id", "PistonMovingBlockEntity.direction", { range: "0..5" }],
      extending: ["block moves along facing, otherwise against it (getMovementDirection)", "", "PistonMovingBlockEntity.extending"],
      source: ["piston's own head or base, not a block it pushes", "", "PistonMovingBlockEntity.isSourcePiston"],
      progress: ["how far it has moved before this tick, cut to a float; each tick adds 0.5 up to 1; refused if not finite", "ratio", "PistonMovingBlockEntity.progress"],
      shapes: ["collision boxes of the block it moves in its cell: they push from 1.11, and on 1.8 those of a block it pushes", "", "PistonMovingBlockEntity.getCollisionRelatedBlockState"],
      nextShapes: ["`shapes` once it has gone on this tick, for the pistons that tick after it, a head being shorter by its progress; absent: the same", "", "PistonMovingBlockEntity.getCollisionRelatedBlockState"],
      outlineShapes: ["outline boxes of a block it pushes in its cell, which push on 1.9 - 1.10; absent or empty: the full cube", "", "binding: MovingPiston::outline_bounds joins them for legacy_moved_box"],
      bounces: ["block it moves is a slime block: a player it meets takes the piston's motion on that axis; absent: false", "", "PistonMovingBlockEntity.moveCollidedEntities"],
      stickyForEntities: ["block it moves is a honey block, carrying what stands on it on a horizontal move from 1.15; absent: false", "", "PistonMovingBlockEntity.isStickyForEntities"],
    },
  },
  NativeNearbyEntity: {
    family: "player",
    fields: {
      kind: ["living, boat, shulker, happy_ghast, happy_ghast_stays_still, still_living, attached_rocket or minecart; anything else neither pushes nor collides", "", "binding: NearbyEntityKind::parse maps other names to Inert"],
      position: ["where the player's move meets it", "", "Entity.position|x"],
      boundingBox: ["box the player's move meets", "", "Entity.getBoundingBox"],
      ticksBeforePlayer: ["client ticks it before the player, as the hash map of 1.14 - 1.16 can, so it pushes and collides from tickBoundingBox; absent: false", "", "binding: push_by_nearby_entities filters its turn by NearbyEntity.ticks_before_player"],
      tickPosition: ["where its own tick leaves it, from where it pushes the player; absent: position", "", "binding: entity.rs pushes from NearbyEntity.tick_position"],
      tickBoundingBox: ["box its own tick leaves it with, which the push tests; absent: boundingBox", "", "binding: entity.rs pushes by NearbyEntity.tick_bounding_box"],
      teamPushes: ["false when the team collision rules keep it from pushing the player; absent: true", "", "EntitySelector.pushableBy"],
    },
  },
  NativeNearbyEntityPush: {
    family: "player",
    fields: {
      index: ["index of the entity among the state's nearbyEntities", "count", "binding: entity.rs records EntityPush.index in the tick"],
      x: ["x velocity the entity took back from the player's tick", "blocks a tick", "Entity.push"],
      z: ["z velocity the entity took back from the player's tick", "blocks a tick", "Entity.push"],
    },
  },
  NativePistonPushedEntity: {
    family: "piston",
    fields: {
      position: ["position its own tick left it at; must be finite", "", "Entity.position|x"],
      velocity: ["velocity its own tick left it with; must be finite", "", "Entity.getDeltaMovement|velocityX"],
      boundingBox: ["its kept box before 1.17; absent: from the position, halfWidth and height", "", "Entity.getBoundingBox"],
      halfWidth: ["half its width, which sizes the box; must be finite and above 0", "blocks", "Entity.getBbWidth"],
      height: ["its height, which sizes the box; must be finite and above 0", "blocks", "Entity.getBbHeight"],
      onGround: ["Entity.onGround before the push; absent: false", "", "Entity.onGround"],
      collidedHorizontally: ["Entity.horizontalCollision before the push; absent: false", "", "Entity.horizontalCollision"],
      collidedVertically: ["Entity.verticalCollision before the push; absent: false", "", "Entity.verticalCollision"],
      verticalCollisionBelow: ["Entity.verticalCollisionBelow before the push; absent: false", "", "Entity.verticalCollisionBelow"],
      fallDistance: ["distance fallen so far, carried through its Entity.move; absent: 0", "blocks", "Entity.fallDistance"],
      stuckSpeedMultiplier: ["stuck speed multiplier its Entity.move slows by and then uses up; absent: zero", "", "Entity.stuckSpeedMultiplier"],
      living: ["a LivingEntity: it bounces off slime and beds fully, others by 0.8 of it; absent: false", "", "LivingEntity"],
      stepHeight: ["step height of its move, cut to a float: Entity.maxUpStep, with the STEP_HEIGHT attribute of a LivingEntity; absent: 0", "blocks", "Entity.maxUpStep"],
      gravity: ["its getGravity, for the 26.2 landing's bounce; absent: 0.04", "blocks a tick per tick", "Entity.getGravity"],
      airDrag: ["its air drag, for the 26.2 landing's bounce; absent: 0.98F", "ratio", "Entity.getAirDrag"],
      noPhysics: ["Entity.noPhysics: the whole of the push, through what is in the way; absent: false", "", "Entity.noPhysics"],
      clientSide: ["client's view of it, read by the authority rules of docs/piston-authority.md; absent: false", "", "Level.isClientSide"],
      controlledByLocalInstance: ["Entity.isLocalInstanceAuthoritative, which keeps vertical collision and ground state on a horizontal push when false; absent: true", "", "Entity.isLocalInstanceAuthoritative|isControlledByLocalInstance"],
      canSimulateMovement: ["Entity.canSimulateMovement, gating slime bounce and restitution from 26.3 (docs/piston-authority.md); absent: from controlledByLocalInstance", "", "Entity.canSimulateMovement"],
      pistonDeltas: ["what earlier calls of this game tick moved it (their pistonDeltas), which the 0.51 limit counts; absent: nothing", "", "Entity.pistonDeltas"],
    },
  },
  NativePistonPushedResult: {
    family: "piston",
    fields: {
      position: ["position after the tick's pistons", "", "Entity.position|x"],
      velocity: ["velocity after the tick's pistons, a slime block's bounce included", "", "Entity.getDeltaMovement|velocityX"],
      boundingBox: ["box kept by Entity.move, absent when none was kept", "", "Entity.getBoundingBox"],
      onGround: ["Entity.onGround after the push", "", "Entity.onGround"],
      collidedHorizontally: ["Entity.horizontalCollision after the push", "", "Entity.horizontalCollision"],
      collidedVertically: ["Entity.verticalCollision after the push", "", "Entity.verticalCollision"],
      verticalCollisionBelow: ["Entity.verticalCollisionBelow after the push", "", "Entity.verticalCollisionBelow"],
      fallDistance: ["distance fallen after the push's moves", "blocks", "Entity.fallDistance"],
      stuckSpeedMultiplier: ["stuck speed multiplier as the moves left it", "", "Entity.stuckSpeedMultiplier"],
      pistonDeltas: ["how far they moved it on each axis, limited to 0.51 a game tick; pass it to the tick's next call", "", "Entity.pistonDeltas"],
    },
  },
  NativePistonPushResult: {
    family: "piston",
    fields: {
      state: ["player state after the pushes of the state's movingPistons", "", "binding: simulate_versioned_piston_push hands it back"],
      pistonDeltas: ["how far the pistons of the tick have moved the player on each axis, its input included", "", "Entity.pistonDeltas"],
    },
  },
  NativePlayerLayout: {
    family: "world",
    fields: {
      version: ["layout number of the packed arrays, raised when an index moves or changes meaning; a checkpoint of another is refused", "id", "binding: native_player_layout hands back LAYOUT_VERSION"],
      inputLength: ["numbers in the packed input array", "count", "binding: native_player_layout hands back input::LENGTH"],
      outputLength: ["numbers in the packed output array", "count", "binding: native_player_layout hands back output::LENGTH"],
      entityLength: ["numbers of one entity in the packed entity array", "count", "binding: native_player_layout hands back ENTITY_LENGTH"],
    },
  },
  NativePoseRefreshResult: {
    family: "player",
    fields: {
      state: ["state with the half width, height and eye height of the chosen pose", "", "binding: refresh_versioned_pose hands it back"],
      pose: ["pose chosen, as vanilla's Pose id: 0 standing, 3 swimming, 5 crouching and so on", "id", "Entity.getPose"],
      changed: ["pose differs from the current_pose argument", "", "binding: refresh_versioned_pose compares the two poses"],
    },
  },
  NativeTickOutcome: {
    family: "player",
    fields: {
      state: ["state after the tick, with the scene the next tick reads, fit to pass back", "", "binding: simulate_versioned_tick_outcome hands it back through state_with_scene"],
      entityPushes: ["pushes the nearby entities took back from this tick", "", "binding: simulate_versioned_tick_outcome hands back the outcome's pushes"],
      autoSpinAttackEnded: ["riptide spin attack ended this tick (checkAutoSpinAttack)", "", "LivingEntity.checkAutoSpinAttack"],
      pistonDeltas: ["how far the moving pistons moved the player this tick", "", "Entity.pistonDeltas"],
    },
  },
  NativeTickPhase: {
    family: "player",
    fields: {
      name: ["phase name, tickStart to clientTickEnd, as the recorder's schema/phases.ts has them", "", "binding: Phase::name in native/src/phase.rs"],
      occurrence: ["times the tick had passed that point before", "count", "binding: phase.rs counts the point's earlier events"],
      state: ["copy of the state at that point", "", "binding: phase.rs emit copies the state"],
      travelVector: ["LivingEntity's xxa, yya and zza at beforeTravel, absent elsewhere", "", "LivingEntity.xxa"],
    },
  },
  NativeTracedBoatTick: {
    family: "player",
    fields: {
      state: ["what simulateVersionedBoatTick gives for the same input", "", "binding: simulate_versioned_boat_tick_traced hands it back"],
      moves: ["heads and tails of the moves in the tick, in order", "", "binding: phase.rs traced_vehicle collects them"],
    },
  },
  NativeTracedHorseTick: {
    family: "player",
    fields: {
      state: ["what simulateVersionedHorseTick gives for the same input", "", "binding: simulate_versioned_horse_tick_traced hands it back"],
      moves: ["heads and tails of the moves in the tick, in order", "", "binding: phase.rs traced_vehicle collects them"],
    },
  },
  NativeTracedTick: {
    family: "player",
    fields: {
      state: ["what simulateVersionedTick gives for the same input, the trace only reading", "", "binding: simulate_versioned_tick_traced hands it back"],
      phases: ["points the tick passed, in order; a tick that does not travel passes fewer", "", "binding: phase.rs traced collects them"],
    },
  },
  NativeVec3: {
    family: "player",
    fields: {
      x: ["x component of a position, motion, multiplier or input, per the field holding it", "blocks, blocks a tick or ratio", "Vec3.x"],
      y: ["y component of a position, motion, multiplier or input, per the field holding it", "blocks, blocks a tick or ratio", "Vec3.y"],
      z: ["z component of a position, motion, multiplier or input, per the field holding it", "blocks, blocks a tick or ratio", "Vec3.z"],
    },
  },
  NativeVehicleMove: {
    family: "player",
    fields: {
      name: ["vehicleBeforeMove or vehicleAfterMove, the head or the tail of the move", "", "binding: move_vehicle_among's emit_vehicle_move names the two"],
      position: ["vehicle's position at that point", "", "Entity.position|x"],
      velocity: ["vehicle's velocity at that point", "", "Entity.getDeltaMovement|velocityX"],
    },
  },
  NativeVersionCapabilities: {
    family: "world",
    fields: {
      fluidPhysics: ["release is 1.13 or later, answered for the caller; the tick asks the release itself", "", "binding: MinecraftVersion::capabilities tests at_least(1, 13, 0)"],
      honeyBlocks: ["release is 1.15 or later, the first with the honey block", "", "binding: MinecraftVersion::capabilities tests at_least(1, 15, 0)"],
      fallFlying: ["release is 1.9 or later, the first with elytra flight", "", "binding: MinecraftVersion::capabilities tests at_least(1, 9, 0)"],
      waterMovementEfficiency: ["release is 1.21 or later, whose LivingEntity registers the attribute", "", "binding: MinecraftVersion::capabilities tests at_least(1, 21, 0)"],
      minorHorizontalCollision: ["release is 1.18 or later, with Entity.minorHorizontalCollision", "", "Entity.minorHorizontalCollision"],
    },
  },
  NativeVersionedEntityState: {
    family: "player",
    fields: {
      simulation: ["simulation state of the entity, moved and handed back by EntityBatch", "", "binding: EntityBatch::states hands it back"],
      noPhysics: ["cancels every Entity.push with this entity", "", "Entity.noPhysics"],
      id: ["id other entities' vehicleId name, which marks passengers of one vehicle", "id", "Entity.getId"],
      vehicleId: ["id of the entity it rides, which keeps the two from pushing each other", "id", "Entity.getVehicle"],
      pushable: ["Entity.isPushable: a push reaches it only if true, and from 1.19 it takes one only if true", "", "Entity.isPushable"],
      pushthrough: ["Entity.pushthrough before 1.17: the pusher's scales the push by 1 minus it; refused outside 0..1", "ratio", "Entity.pushthrough", { range: "0..1" }],
    },
  },
  NativeWorldBounds: {
    family: "world",
    fields: {
      minBuildHeight: ["lowest block y inside the build height, 0 until setWorldBounds", "blocks", "LevelHeightAccessor.getMinBuildHeight|getMinY"],
      maxBuildHeight: ["first block y above the build height, exclusive unlike getMaxY, 0 until setWorldBounds", "blocks", "LevelHeightAccessor|BlockGetter.getMaxBuildHeight|getMaxY"],
      borderCenterX: ["x of the world border's centre", "blocks", "WorldBorder.getCenterX"],
      borderCenterZ: ["z of the world border's centre", "blocks", "WorldBorder.getCenterZ"],
      borderSize: ["world border's side length, not negative", "blocks", "WorldBorder.getSize"],
    },
  },
  BulkRunOptions: {
    family: "adapter",
    fields: {
      chunkTicks: ["ticks of a chunk, the most a cancellation waits for times the entities; at least 1, DEFAULT_CHUNK_TICKS (20) by default", "ticks", "binding: runBulk in src/bulk.ts shrinks it to fit maxChunkWork"],
      signal: ["stops the run between two chunks; a run already stopped runs nothing", "", "binding: runBulk in src/bulk.ts polls signal.aborted before each chunk"],
      onProgress: ["called after each chunk with the ticks the run has done in all and those of this call", "", "binding: runBulk in src/bulk.ts calls it after run.advance"],
    },
  },
  BulkRunOutcome: {
    family: "adapter",
    fields: {
      ticksDone: ["whole ticks of the run, this call's and those before", "ticks", "binding: runBulk in src/bulk.ts reads run.ticksDone"],
      ticksRun: ["whole ticks of this call", "ticks", "binding: runBulk in src/bulk.ts adds up the chunks"],
      cancelled: ["signal stopped this call short of its ticks, leaving the run ready to go on", "", "binding: runBulk in src/bulk.ts sets it at a chunk boundary"],
    },
  },
  NativeCheckpointParts: {
    family: "adapter",
    fields: {
      version: ["exact release of the run; decodeCheckpoint refuses another", "", "binding: encodeCheckpoint writes it as `release`"],
      kind: ["family `state` is of, player when not given; decodeCheckpoint refuses another", "", "binding: encodeCheckpoint in src/checkpoint.ts writes it"],
      state: ["whole state, numbers kept by their bits where JSON would lose them", "", "binding: encodeCheckpoint in src/checkpoint.ts runs it through exact"],
      world: ["world stored as bytes with their digest; decoding builds a new one", "", "binding: encodeCheckpoint in src/checkpoint.ts calls world.checkpoint"],
      config: ["simulation config written as JSON, {} when not given", "", "binding: encodeCheckpoint in src/checkpoint.ts runs it through exact"],
    },
  },
  ReleaseDimension: {
    family: "adapter",
    fields: {
      status: ["what scripts/release-admission.cjs found; exact or alias pass for source, complete for data and implementation", "", "binding: assertReleaseAdmitted reads src/release-status.json"],
      reasons: ["what keeps the dimension short of passing, named in the refusal", "", "binding: assertReleaseAdmitted joins them into its error"],
    },
  },
  ReleaseStatus: {
    family: "adapter",
    fields: {
      source: ["whether the release's sources are pinned exactly or by alias", "", "binding: assertReleaseAdmitted tests it for exact or alias"],
      data: ["whether the release's data is complete", "", "binding: assertReleaseAdmitted tests it for complete"],
      implementation: ["whether the release's implementation is complete", "", "binding: assertReleaseAdmitted tests it for complete"],
      admitted: ["every dimension passed, conformance among them: only a pinned, current and complete trace does that", "", "binding: assertReleaseAdmitted lets only this through"],
    },
  },
  NativeEngineOptions: {
    family: "adapter",
    fields: {
      binding: ["native binding used; LoadNativePhysics() by default", "", "binding: NativeEngine constructor in src/mineflayer/base.ts loads it"],
      version: ["exact release to run, the registry's by default; it must match registry.version.minecraftVersion", "", "binding: resolveRegistryRelease in registry-release.ts refuses a mismatch"],
      requireParity: ["refuses a release not admitted for parity, rather than run what is registered", "", "binding: NativeEngine constructor calls assertReleaseAdmitted"],
    },
  },
  PistonBot: {
    family: "adapter",
    fields: {
      on: ["subscribes to pistonMove, blockUpdate, physicsTick and end, which feed the tracker, the lids and the disposal", "", "binding: NativeEngine in src/mineflayer/base.ts listens through it"],
      removeListener: ["takes those listeners off again when the link or setup is released", "", "binding: WorldLink.release in src/mineflayer/base.ts calls it"],
    },
  },
  PrismarineBlock: {
    family: "adapter",
    fields: {
      type: ["block id of the registry, sent as the update's id", "id", "binding: describeBlock in blocks.ts reads it as DescribedBlock.id"],
      name: ["registry name, which picks friction, effect, fluid and overrides", "", "binding: describeBlock in blocks.ts reads it"],
      metadata: ["data value before the flattening, also the legacy liquid level", "id", "binding: describeBlock in blocks.ts reads it as meta"],
      stateId: ["state id, which the piston tracker writes back; before the flattening it uses type << 4 | metadata instead", "id", "binding: stateIdOf in moving.ts reads it"],
      shapes: ["collision boxes as six or more numbers, cut to six", "blocks", "binding: describeBlock in blocks.ts slices each to a Box"],
      getProperties: ["block state properties by name, stringified for DescribedBlock.props", "", "binding: describeBlock in blocks.ts calls it"],
    },
  },
  DescribedFluid: {
    family: "adapter",
    fields: {
      type: ["FLUID_WATER 1, FLUID_LAVA 2 or FLUID_WATERLOGGED 3, sent as the update's fluid", "id", "binding: describeBlock in blocks.ts picks it from the block"],
      level: ["liquid's level property or legacy data value, 0 for water plants and waterlogged blocks", "count", "LiquidBlock.LEVEL"],
      source: ["level is 0, or the block is a water plant or waterlogged", "", "binding: describeBlock in blocks.ts derives it"],
      falling: ["level is 8 or more, or the block is a copper grate's falling source", "", "binding: describeBlock in blocks.ts derives it"],
    },
  },
  DescribedBlock: {
    family: "adapter",
    fields: {
      name: ["registry name of the block", "", "binding: describeBlock copies PrismarineBlock.name"],
      props: ["block properties as strings", "", "binding: describeBlock stringifies getProperties"],
      id: ["block id, PrismarineBlock.type", "id", "binding: describeBlock copies PrismarineBlock.type"],
      meta: ["data value, PrismarineBlock.metadata", "id", "binding: describeBlock copies PrismarineBlock.metadata"],
      boxes: ["collision boxes before neighbours are applied", "", "binding: describeBlock cuts PrismarineBlock.shapes to Boxes"],
      friction: ["Block.friction by name, else 0.6", "ratio", "Block.getFriction"],
      effect: ["BlockEffect code of the block, from effectOf", "id", "binding: effectOf in blocks.ts maps name and props to it"],
      bounceRestitution: ["1 for the slime effect, 0.75 for a bed, else 0", "ratio", "Block.getBounceRestitution"],
      fluid: ["fluid held by the block, null for none", "", "binding: describeBlock in blocks.ts builds a DescribedFluid"],
    },
  },
  VersionCheck: {
    family: "adapter",
    fields: {
      "'<'": ["release is older than the given one, for the adapter's release gates", "", "binding: releaseCheck in blocks.ts compares exact releases"],
      "'>='": ["release is the given one or newer, for the adapter's release gates", "", "binding: releaseCheck in blocks.ts compares exact releases"],
      majorVersion: ["major line, such as 1.12, that keys the legacy tables", "", "binding: releaseCheck in blocks.ts derives it"],
      minecraftVersion: ["exact release string, named in refusals and data lookups", "", "binding: releaseCheck in blocks.ts holds it"],
    },
  },
  LiquidCollisionContext: {
    family: "adapter",
    fields: {
      bottom: ["EntityCollisionContext.entityBottom, tested against the stable fluid top", "blocks", "EntityCollisionContext.entityBottom"],
      canStandOn: ["LivingEntity.canStandOnFluid: a strider's on lava, false for any other body and for no entity", "", "LivingEntity.canStandOnFluid"],
      withFluidCollisions: ["CollisionGetter.getBlockAndLiquidCollisions, which only the server's spawn search asks; refused before 1.21.2", "", "CollisionGetter.getBlockAndLiquidCollisions"],
    },
  },
  LegacyBlock: {
    family: "adapter",
    fields: {
      type: ["block id of a neighbour before the flattening", "id", "binding: legacyShapes in legacy.ts combines it with the data value"],
      metadata: ["data value of a neighbour before the flattening", "id", "binding: legacyShapes in legacy.ts combines it with the block id"],
    },
  },
  LegacyShapes: {
    family: "adapter",
    fields: {
      chestOutline: ["box a ray meets of a chest in 1.8, null for any other block or release", "", "binding: legacyShapes in legacy.ts answers it from chestShapes for 1.8"],
      outline: ["box a ray meets of redstone wire or a vine among its neighbours before 1.13, null for any other block", "", "binding: legacyShapes in legacy.ts answers it from legacy-outline-neighbours.json"],
      shapes: ["block's boxes among its neighbours, or null when minecraft-data's shape is the block's", "", "binding: legacyShapes in legacy.ts builds it from the legacy tables"],
      isNormalCube: ["whether pushOutOfBlocks takes the block for a normal cube: the block's own property, not its collision boxes", "", "binding: legacyShapes in legacy.ts looks it up by id and data value"],
      reach: ["how far from a block the blocks its shape depends on lie: 2 in 1.12, whose fences read the stairs' corners", "blocks", "binding: legacyShapes sets it, and NativeWorldSync reads that much further"],
    },
  },
  LegacyFlowBlock: {
    family: "adapter",
    fields: {
      fluid: ["its liquid, lava or not with its level, null for none, for BlockLiquid.getFlow's port", "count", "binding: fluidUpdate in blocks.ts builds it for legacyFlowAt"],
    },
  },
  MovableWorld: {
    family: "adapter",
    fields: {
      setBlockStateId: ["writes a state id into the prismarine world as the tracker moves blocks; a world without it stays as it is", "", "binding: place in moving.ts calls it"],
    },
  },
  MovingRegistry: {
    family: "adapter",
    fields: {
      blocksByName: ["registry entries by name: their block id and default state, for the moving_piston and air cells", "id", "binding: the moving.ts tracker reads it"],
    },
  },
  Mth: {
    family: "adapter",
    fields: {
      sin: ["vanilla's table sine of an angle in radians: float argument scaled by 10430.378F, double from 1.21.11", "ratio", "Mth.sin"],
      cos: ["vanilla's table cosine of an angle in radians, the table entry 16384 further on", "ratio", "Mth.cos"],
    },
  },
  BiomeData: {
    family: "adapter",
    fields: {
      name: ["biome name; a biome without one has no rain, and the frozen oceans take the adjusted temperature", "", "binding: rainingAt and biomeTemperature in src/mineflayer read it"],
      temperature: ["base temperature of the biome; absent: 0.5", "biome temperature", "Biome.getBaseTemperature|getTemperature"],
      precipitation: ["rain, snow or none, to 1.19.3", "", "binding: rainsInBiome in rain.ts reads minecraft-data's field"],
      has_precipitation: ["whether the biome has precipitation, from 1.19.4; wins over `precipitation`", "", "Biome.hasPrecipitation"],
    },
  },
  PhysicsBot: {
    family: "adapter",
    fields: {
      version: ["release the bot talks, which its registry must be of; another engine release is refused", "", "binding: NativeBotPhysics constructor in setup.ts compares it"],
      registry: ["data the engine is made from; a bot without one is refused", "", "binding: createBotPhysics in setup.ts requires it"],
      physicsEngine: ["engine of mineflayer's physics plugin, whose `simulate` installTicks replaces and dispose restores", "", "binding: NativeBotPhysics.installTicks in setup.ts swaps it"],
    },
  },
  PhysicsCtxFactory: {
    family: "adapter",
    fields: {
      FROM_BOT: ["mineflayer-physics-utils' EPhysicsCtx.FROM_BOT, which makes a context over this engine", "", "binding: NativeBotPhysics.context in setup.ts calls it"],
    },
  },
  VersionedNativePhysicsClass: {
    family: "adapter",
    fields: {
      version: ["exact release the engine class is bound to", "", "binding: generate-version-exports.cjs writes it into versions.ts"],
    },
  },
  Vec3Like: {
    family: "adapter",
    fields: {
      x: ["x coordinate of a position handed to a prismarine world", "blocks", "binding: Cursor in src/mineflayer/world.ts provides it"],
      y: ["y coordinate of a position handed to a prismarine world", "blocks", "binding: Cursor in src/mineflayer/world.ts provides it"],
      z: ["z coordinate of a position handed to a prismarine world", "blocks", "binding: Cursor in src/mineflayer/world.ts provides it"],
    },
  },
  PrismarineWorld: {
    family: "adapter",
    fields: {
      getBlock: ["prismarine block at a position, read when a state id does not answer", "", "binding: NativeWorldSync in src/mineflayer/world.ts calls it"],
      getSkyLight: ["sky light at a position; rain needs 15, so a world without it has no rain", "count", "binding: rainingAt in src/mineflayer/world.ts calls it", { range: "0..15" }],
      getBlockStateId: ["state id at a position, read without building a block", "id", "binding: NativeWorldSync in src/mineflayer/world.ts calls it"],
      getColumnAt: ["chunk column at a position: where it has none, its state id is 0 and no block", "", "binding: NativeWorldSync in src/mineflayer/world.ts calls it"],
      on: ["a world that has it gets listeners for blockUpdate and chunk columns, and only changed blocks are read again; others are read whole each tick", "", "binding: NativeWorldSync in src/mineflayer/world.ts attaches them"],
      off: ["takes those listeners off, preferred over removeListener; a world with neither keeps them, which then do nothing", "", "binding: NativeWorldSync.detach in src/mineflayer/world.ts calls it"],
      removeListener: ["takes those listeners off in a world with no `off`", "", "binding: NativeWorldSync.detach in src/mineflayer/world.ts calls it"],
    },
  },
  SyncRegistry: {
    family: "adapter",
    fields: {
      version: ["registry's version object, the BlockMapper constructor's first parameter", "", "binding: NativeWorldSync constructor types it from BlockMapper"],
      blocksArray: ["registry's blocks with id and name, from which legacyShapes matches block families", "id", "binding: legacyShapes in legacy.ts reads it"],
      blocksByName: ["registry's blocks by name, with their ids", "id", "binding: NativeWorldSync passes it to BlockMapper"],
    },
  },
  BoxLike: {
    family: "adapter",
    fields: {
      minX: ["lower x of the region a sync or collision query reads", "blocks", "binding: regionOf in src/mineflayer/world.ts floors it"],
      minY: ["lower y of the region a sync or collision query reads", "blocks", "binding: regionOf in src/mineflayer/world.ts floors it"],
      minZ: ["lower z of the region a sync or collision query reads", "blocks", "binding: regionOf in src/mineflayer/world.ts floors it"],
      maxX: ["upper x of the region a sync or collision query reads", "blocks", "binding: regionOf in src/mineflayer/world.ts floors it"],
      maxY: ["upper y of the region a sync or collision query reads", "blocks", "binding: regionOf in src/mineflayer/world.ts floors it"],
      maxZ: ["upper z of the region a sync or collision query reads", "blocks", "binding: regionOf in src/mineflayer/world.ts floors it"],
    },
  },
  MovingBlock: {
    family: "adapter",
    fields: {
      x: ["block x of the moving block's cell", "blocks", "BlockEntity.getBlockPos"],
      y: ["block y of the moving block's cell", "blocks", "BlockEntity.getBlockPos"],
      z: ["block z of the moving block's cell", "blocks", "BlockEntity.getBlockPos"],
      facing: ["piston's facing as Direction.from3DDataValue", "id", "PistonMovingBlockEntity.direction", { range: "0..5" }],
      extending: ["block moves along facing, otherwise against it", "", "PistonMovingBlockEntity.extending"],
      source: ["piston's own head or base, not a block it pushes", "", "PistonMovingBlockEntity.isSourcePiston"],
      progress: ["how far it has moved before this tick, which the tracker raises by 0.5 up to 1", "ratio", "PistonMovingBlockEntity.progress"],
      block: ["block whose shape it moves at this progress: the head a source piston shows, else the block it moves", "", "binding: shown in moving.ts picks it"],
      next: ["block it shows once it has gone on this tick, if that is another one; gives nextShapes", "", "binding: shown in moving.ts picks it for movingPistonsOf"],
      base: ["extended base a source piston leaves as it retracts, from 1.11", "", "binding: base in moving.ts builds it for setMovingBlocks"],
    },
  },
};
