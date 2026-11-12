//! The N-API surface of moving_entity.rs. A state goes in and comes out whole, so it can be carried
//! from tick to tick; a field left out takes the value a new entity has.

use napi::bindgen_prelude::Result;
use napi::Error;
use napi_derive::napi;

use crate::block_traversal::Movement;
use crate::entity_travel::refuse_for_mob_travel;
use crate::math::{Aabb, Vec3};
use crate::moving_entity::{
    tick_mob_travel, tick_moving_entity, MobTravelKind, MobTravelState, MovingEntityAuthority,
    MovingEntityInputs, MovingEntityKind, MovingEntityOutcome, MovingEntityPlayer,
    MovingEntityState,
};
use crate::dragon::{
    dragon_parts, dragon_wing_pushes, parts_follow_dragon, tick_dragon, tick_dragon_flight,
    DragonFlightHistory, DragonFlightSample, DragonPart, DragonPushTarget, DragonState,
};
use crate::napi_api::{
    check_move, validate_kept_box, FieldCheck, FieldRule, validate_position, validate_size, vehicle_release, NativeAabb,
    NativeBlockPosition, NativeMovingPiston, NativeVec3, NativeWorld,
};
use crate::piston::{push_body_by_moving_pistons, MovingPiston};
use crate::simulation::SimulationState;
use crate::vehicle::MoveRules;

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMovingEntityMovement {
    pub from: NativeVec3,
    pub to: NativeVec3,
    pub axis_order: Option<NativeVec3>,
}

/// A primed TNT, falling block, item or experience orb.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMovingEntityState {
    /// "tnt", "falling_block", "item" or "experience_orb".
    pub kind: String,
    /// "client" (the default: a prediction, with no event) or "server".
    pub authority: Option<String>,
    pub id: Option<i32>,
    pub tick_count: Option<i32>,
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    /// Entity's kept box, before 1.17.
    pub bounding_box: Option<NativeAabb>,
    pub on_ground: Option<bool>,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub vertical_collision_below: Option<bool>,
    pub in_water: Option<bool>,
    pub in_lava: Option<bool>,
    pub no_gravity: Option<bool>,
    pub no_physics: Option<bool>,
    pub stuck_speed_multiplier: Option<NativeVec3>,
    pub fall_distance: Option<f64>,
    pub ultra_warm: Option<bool>,
    pub first_tick: Option<bool>,
    pub last_movements: Option<Vec<NativeMovingEntityMovement>>,
    pub removed: Option<bool>,
    pub fuse: Option<i32>,
    pub time: Option<i32>,
    /// The carried block's id, as the world's blocks say it.
    pub carried_block: Option<u32>,
    pub carried_air: Option<bool>,
    pub concrete_powder: Option<bool>,
    /// 1.18 and 1.18.1: the client froze the landed block until its removal.
    pub discarding: Option<bool>,
    pub age: Option<i32>,
    pub pickup_delay: Option<i32>,
    pub item_empty: Option<bool>,
    /// The item is fire immune (a netherite item or ancient debris): no burn draw in lava on 1.16.
    pub fire_immune: Option<bool>,
    pub following_player: Option<i32>,
    pub following_time: Option<i32>,
    pub orb_ticks: Option<i32>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMovingEntityPlayer {
    pub id: i32,
    pub position: NativeVec3,
    pub eye_height: f64,
    pub spectator: Option<bool>,
    pub dead: Option<bool>,
}

/// What a client cannot see, which the caller decides.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Default)]
pub struct NativeMovingEntityInputs {
    /// The entity's next random.nextFloat() draws, in order.
    pub random: Option<Vec<f64>>,
    pub players: Option<Vec<NativeMovingEntityPlayer>>,
    /// A falling block on a server before 1.14: FallingBlock.isFree under its feet.
    pub falls_through_below: Option<bool>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeMovingEntityTickResult {
    pub state: NativeMovingEntityState,
    pub explodes: bool,
    pub landed_at: Option<NativeBlockPosition>,
    pub expired: bool,
    pub clears_block: Option<NativeBlockPosition>,
    pub merge_check: bool,
    pub random_used: u32,
    pub random_missing: bool,
}

fn parse_authority(value: Option<&str>) -> Result<MovingEntityAuthority> {
    match value {
        None | Some("client") => Ok(MovingEntityAuthority::Client),
        Some("server") => Ok(MovingEntityAuthority::Server),
        Some(other) => Err(Error::from_reason(format!(
            "unknown moving entity authority: {other}"
        ))),
    }
}

fn entity_state(value: NativeMovingEntityState) -> Result<MovingEntityState> {
    let kind = MovingEntityKind::parse(&value.kind).ok_or_else(|| {
        Error::from_reason(format!("unknown moving entity kind: {}", value.kind))
    })?;
    let fresh = MovingEntityState::new(kind, value.position.into(), value.velocity.into());
    let state = MovingEntityState {
        authority: parse_authority(value.authority.as_deref())?,
        id: value.id.unwrap_or(fresh.id),
        tick_count: value.tick_count.unwrap_or(fresh.tick_count),
        bounding_box: value.bounding_box.map(Into::into),
        on_ground: value.on_ground.unwrap_or(false),
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
        vertical_collision_below: value.vertical_collision_below.unwrap_or(false),
        in_water: value.in_water.unwrap_or(false),
        in_lava: value.in_lava.unwrap_or(false),
        no_gravity: value.no_gravity.unwrap_or(false),
        no_physics: value.no_physics.unwrap_or(false),
        stuck_speed_multiplier: value.stuck_speed_multiplier.map_or(Vec3::ZERO, Into::into),
        fall_distance: value.fall_distance.unwrap_or(0.0),
        ultra_warm: value.ultra_warm.unwrap_or(false),
        first_tick: value.first_tick.unwrap_or(fresh.first_tick),
        last_movements: value
            .last_movements
            .unwrap_or_default()
            .into_iter()
            .map(|movement| Movement {
                from: movement.from.into(),
                to: movement.to.into(),
                axis_order: movement.axis_order.map(Into::into),
            })
            .collect(),
        removed: value.removed.unwrap_or(false),
        fuse: value.fuse.unwrap_or(fresh.fuse),
        time: value.time.unwrap_or(0),
        carried_block: value.carried_block.unwrap_or(0),
        carried_air: value.carried_air.unwrap_or(false),
        concrete_powder: value.concrete_powder.unwrap_or(false),
        discarding: value.discarding.unwrap_or(false),
        age: value.age.unwrap_or(0),
        pickup_delay: value.pickup_delay.unwrap_or(0),
        item_empty: value.item_empty.unwrap_or(false),
        fire_immune: value.fire_immune.unwrap_or(false),
        following_player: value.following_player,
        following_time: value.following_time.unwrap_or(0),
        orb_ticks: value.orb_ticks.unwrap_or(0),
        ..fresh
    };
    FieldCheck::new("moving entity state")
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .vector("stuckSpeedMultiplier", state.stuck_speed_multiplier, FieldRule::Finite)
        .number("fallDistance", state.fall_distance, FieldRule::Finite)
        .done()?;
    validate_position("moving entity", state.position)?;
    validate_kept_box("moving entity", state.bounding_box)?;
    for movement in &state.last_movements {
        // (a recorded move is walked as the box it swept)
        let swept = Aabb::ordered(movement.from, movement.to);
        if !swept.is_walkable() || !movement.from.is_finite() || !movement.to.is_finite() {
            return Err(Error::from_reason(format!(
                "moving entity: a last movement from {:?} to {:?} is past what a call walks",
                movement.from, movement.to
            )));
        }
    }
    Ok(state)
}

fn native_entity_state(state: MovingEntityState) -> NativeMovingEntityState {
    NativeMovingEntityState {
        kind: state.kind.name().to_string(),
        authority: Some(
            match state.authority {
                MovingEntityAuthority::Client => "client",
                MovingEntityAuthority::Server => "server",
            }
            .to_string(),
        ),
        id: Some(state.id),
        tick_count: Some(state.tick_count),
        position: state.position.into(),
        velocity: state.velocity.into(),
        bounding_box: state.bounding_box.map(Into::into),
        on_ground: Some(state.on_ground),
        collided_horizontally: Some(state.collided_horizontally),
        collided_vertically: Some(state.collided_vertically),
        vertical_collision_below: Some(state.vertical_collision_below),
        in_water: Some(state.in_water),
        in_lava: Some(state.in_lava),
        no_gravity: Some(state.no_gravity),
        no_physics: Some(state.no_physics),
        stuck_speed_multiplier: Some(state.stuck_speed_multiplier.into()),
        fall_distance: Some(state.fall_distance),
        ultra_warm: Some(state.ultra_warm),
        first_tick: Some(state.first_tick),
        last_movements: Some(
            state
                .last_movements
                .iter()
                .map(|movement| NativeMovingEntityMovement {
                    from: movement.from.into(),
                    to: movement.to.into(),
                    axis_order: movement.axis_order.map(Into::into),
                })
                .collect(),
        ),
        removed: Some(state.removed),
        fuse: Some(state.fuse),
        time: Some(state.time),
        carried_block: Some(state.carried_block),
        carried_air: Some(state.carried_air),
        concrete_powder: Some(state.concrete_powder),
        discarding: Some(state.discarding),
        age: Some(state.age),
        pickup_delay: Some(state.pickup_delay),
        item_empty: Some(state.item_empty),
        fire_immune: Some(state.fire_immune),
        following_player: state.following_player,
        following_time: Some(state.following_time),
        orb_ticks: Some(state.orb_ticks),
    }
}

fn entity_inputs(value: Option<NativeMovingEntityInputs>) -> Result<MovingEntityInputs> {
    let value = value.unwrap_or_default();
    let random = value.random.unwrap_or_default();
    // a draw is a float: a double that narrows to 1.0 is no value nextFloat gives
    if random.iter().any(|draw| !(0.0..1.0).contains(&f64::from(*draw as f32))) {
        return Err(Error::from_reason(
            "a random draw of nextFloat must be in [0, 1)",
        ));
    }
    Ok(MovingEntityInputs {
        random: random.into_iter().map(|draw| draw as f32).collect(),
        players: value
            .players
            .unwrap_or_default()
            .into_iter()
            .map(|player| MovingEntityPlayer {
                id: player.id,
                position: player.position.into(),
                eye_height: player.eye_height as f32,
                spectator: player.spectator.unwrap_or(false),
                dead: player.dead.unwrap_or(false),
            })
            .collect(),
        falls_through_below: value.falls_through_below.unwrap_or(false),
    })
}

fn tick_result(
    state: MovingEntityState,
    outcome: MovingEntityOutcome,
) -> NativeMovingEntityTickResult {
    let position = |(x, y, z): (i32, i32, i32)| NativeBlockPosition { x, y, z };
    NativeMovingEntityTickResult {
        state: native_entity_state(state),
        explodes: outcome.explodes,
        landed_at: outcome.landed_at.map(position),
        expired: outcome.expired,
        clears_block: outcome.clears_block.map(position),
        merge_check: outcome.merge_check,
        random_used: outcome.random_used,
        random_missing: outcome.random_missing,
    }
}

/// One tick of a primed TNT, falling block, item or experience orb. It changes neither the world
/// nor any other entity: the result says what the server would do, and a client's says nothing.
#[napi]
pub fn simulate_versioned_moving_entity_tick(
    version: String,
    world: &mut NativeWorld,
    state: NativeMovingEntityState,
    inputs: Option<NativeMovingEntityInputs>,
) -> Result<NativeMovingEntityTickResult> {
    let release = vehicle_release(&version, "moving entity")?;
    let mut state = entity_state(state)?;
    let inputs = entity_inputs(inputs)?;
    let outcome = tick_moving_entity(&mut state, &mut world.inner, &inputs, release);
    check_move("simulateVersionedMovingEntityTick", state.move_out_of_range)?;
    Ok(tick_result(state, outcome))
}

/// What a piston pushes that is no player: its body as its own tick left it, and what its
/// Entity.move depends on.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativePistonPushedEntity {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    /// The box, kept before 1.17 (absent: from the position, `halfWidth` and `height`).
    pub bounding_box: Option<NativeAabb>,
    pub half_width: f64,
    pub height: f64,
    pub on_ground: Option<bool>,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub vertical_collision_below: Option<bool>,
    pub fall_distance: Option<f64>,
    pub stuck_speed_multiplier: Option<NativeVec3>,
    /// A LivingEntity: it bounces off slime and beds fully. Absent: false.
    pub living: Option<bool>,
    /// Entity.maxUpStep, with the STEP_HEIGHT attribute of a LivingEntity. Absent: 0.
    pub step_height: Option<f64>,
    /// The entity's getGravity, for the 26.2 landing's bounce. Absent: 0.04.
    pub gravity: Option<f64>,
    /// The entity's air drag, for the 26.2 landing's bounce. Absent: 0.98F.
    pub air_drag: Option<f64>,
    /// Entity.noPhysics: the whole of the push, through what is in the way.
    pub no_physics: Option<bool>,
    /// The client's view of it. Absent: false.
    pub client_side: Option<bool>,
    /// Entity.isLocalInstanceAuthoritative. Absent: true.
    pub controlled_by_local_instance: Option<bool>,
    /// Entity.canSimulateMovement, from 26.3 (docs/piston-authority.md).
    pub can_simulate_movement: Option<bool>,
    /// What earlier calls of this game tick moved it (their `pistonDeltas`). Absent: nothing.
    pub piston_deltas: Option<NativeVec3>,
}

/// The entity after the tick's pistons.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativePistonPushedResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub bounding_box: Option<NativeAabb>,
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub vertical_collision_below: bool,
    pub fall_distance: f64,
    pub stuck_speed_multiplier: NativeVec3,
    /// How far they moved it on each axis (Entity.pistonDeltas), which is its limit of 0.51 a tick.
    pub piston_deltas: NativeVec3,
}

/// The moving blocks of one game tick against an entity that is no player: an item, TNT, a falling
/// block, a boat, a minecart or a mob, after its own tick (the block entities tick last, on
/// either side). `pistons` in the order they tick. The caller leaves out what vanilla does not push
/// (PushReaction.IGNORE_ENTITY: a marker armor stand, displays and the like).
#[napi]
pub fn simulate_versioned_entity_piston_push(
    version: String,
    world: &mut NativeWorld,
    entity: NativePistonPushedEntity,
    pistons: Vec<NativeMovingPiston>,
) -> Result<NativePistonPushedResult> {
    let release = vehicle_release(&version, "piston push")?;
    let finite = |v: &NativeVec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
    if !finite(&entity.position)
        || !finite(&entity.velocity)
        || !entity.half_width.is_finite()
        || !entity.height.is_finite()
        || entity.half_width <= 0.0
        || entity.height <= 0.0
    {
        return Err(Error::from_reason(
            "simulateVersionedEntityPistonPush: a pushed entity needs a finite position, motion and size",
        ));
    }
    let pistons = pistons
        .into_iter()
        .map(MovingPiston::try_from)
        .collect::<Result<Vec<_>>>()?;
    let mut body = SimulationState {
        position: entity.position.into(),
        velocity: entity.velocity.into(),
        bounding_box: entity.bounding_box.map(Into::into),
        half_width: entity.half_width,
        height: entity.height,
        on_ground: entity.on_ground.unwrap_or(false),
        collided_horizontally: entity.collided_horizontally.unwrap_or(false),
        collided_vertically: entity.collided_vertically.unwrap_or(false),
        vertical_collision_below: entity.vertical_collision_below.unwrap_or(false),
        fall_distance: entity.fall_distance.unwrap_or(0.0),
        stuck_speed_multiplier: entity
            .stuck_speed_multiplier
            .map_or(Vec3::ZERO, Into::into),
        no_physics: entity.no_physics.unwrap_or(false),
        client_side: entity.client_side.unwrap_or(false),
        controlled_by_local_instance: entity.controlled_by_local_instance.unwrap_or(true),
        can_simulate_movement: entity.can_simulate_movement,
        ..SimulationState::default()
    };
    let rules = MoveRules {
        living: entity.living.unwrap_or(false),
        max_step: entity.step_height.unwrap_or(0.0) as f32,
        gravity: entity.gravity.unwrap_or(0.04),
        air_drag: entity.air_drag.unwrap_or(f64::from(0.98_f32)),
        ..MoveRules::MINECART
    };
    let mut deltas = entity.piston_deltas.map_or([0.0; 3], |d| [d.x, d.y, d.z]);
    push_body_by_moving_pistons(&mut body, &mut world.inner, &pistons, &mut deltas, rules, release);
    check_move("simulateVersionedEntityPistonPush", body.move_out_of_range)?;
    Ok(NativePistonPushedResult {
        position: body.position.into(),
        velocity: body.velocity.into(),
        bounding_box: body.bounding_box.map(Into::into),
        on_ground: body.on_ground,
        collided_horizontally: body.collided_horizontally,
        collided_vertically: body.collided_vertically,
        vertical_collision_below: body.vertical_collision_below,
        fall_distance: body.fall_distance,
        stuck_speed_multiplier: body.stuck_speed_multiplier.into(),
        piston_deltas: NativeVec3 {
            x: deltas[0],
            y: deltas[1],
            z: deltas[2],
        },
    })
}

/// A mob whose travel is the engine's, where its AI left it.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMobTravelState {
    /// The entity's registry name: squid, glow_squid, cod, salmon, tropical_fish, pufferfish,
    /// tadpole, guardian, elder_guardian, ghast, phantom, allay, vex, blaze, zombie, cow, bat, bee,
    /// parrot, dolphin, turtle, axolotl, frog, drowned, spider, cave_spider, polar_bear, wither,
    /// armor_stand, strider, skeleton_horse or sulfur_cube; "living" for any mob with no travel of
    /// its own.
    pub kind: String,
    pub position: NativeVec3,
    /// The motion after its AI.
    pub velocity: NativeVec3,
    /// Entity.getYRot, in degrees.
    pub yaw_degrees: f64,
    /// xxa, yya, zza.
    pub input: NativeVec3,
    /// LivingEntity.getSpeed.
    pub speed: f64,
    pub half_width: f64,
    pub height: f64,
    pub max_step: Option<f64>,
    pub bounding_box: Option<NativeAabb>,
    pub on_ground: Option<bool>,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub in_water: Option<bool>,
    pub in_lava: Option<bool>,
    pub no_gravity: Option<bool>,
    pub stuck_speed_multiplier: Option<NativeVec3>,
    pub fall_distance: Option<f64>,
    pub has_target: Option<bool>,
    pub guardian_moving: Option<bool>,
    /// Spider.isClimbing.
    pub climbing: Option<bool>,
    /// Entity.isPushedByFluid; the kind's when absent.
    pub pushed_by_fluid: Option<bool>,
    /// Entity.getFluidJumpThreshold; 0.4 when absent.
    pub fluid_jump_threshold: Option<f64>,
    /// Entity.isUnderWater.
    pub under_water: Option<bool>,
    /// Drowned.wantsToSwim.
    pub wants_to_swim: Option<bool>,
    /// Turtle.goingHome, and within 20 blocks of it.
    pub near_home: Option<bool>,
    /// ArmorStand.isMarker.
    pub marker: Option<bool>,
    /// SulfurCube.hasBodyItem.
    pub body_item: Option<bool>,
    /// SulfurCube.floatsInLiquids.
    pub floats_in_liquids: Option<bool>,
    /// Entity.tickCount.
    pub tick_count: Option<i32>,
    /// MobEffects.SLOW_FALLING.
    pub slow_falling: Option<bool>,
    /// MobEffects.LEVITATION's amplifier.
    pub levitation_amplifier: Option<i32>,
    /// MobEffects.DOLPHINS_GRACE.
    pub dolphins_grace: Option<bool>,
    /// Attributes.GRAVITY, from 1.20.5.
    pub gravity: Option<f64>,
    /// Attributes.WATER_MOVEMENT_EFFICIENCY, from 1.21.
    pub water_movement_efficiency: Option<f64>,
    /// Attributes.AIR_DRAG_MODIFIER, from 26.2; absent is 1.
    pub air_drag_modifier: Option<f64>,
    /// LivingEntity.shouldDiscardFriction (a goat's or frog's long jump, a breeze's jump), from 1.17.
    pub discard_friction: Option<bool>,
    /// Mob.isEffectiveAi, false for a NoAI mob, whose travel then does nothing; true when absent.
    pub effective_ai: Option<bool>,
}

/// One LivingEntity.travel of the mob. Its AI, before and after, is the caller's.
#[napi]
pub fn simulate_versioned_mob_travel(
    version: String,
    world: &NativeWorld,
    state: NativeMobTravelState,
) -> Result<NativeMobTravelState> {
    let release = vehicle_release(&version, "mob travel")?;
    refuse_for_mob_travel(&state.kind, release).map_err(Error::from_reason)?;
    let kind = MobTravelKind::parse(&state.kind)
        .ok_or_else(|| Error::from_reason(format!("no travel of its own: {}", state.kind)))?;
    validate_position("mob travel", state.position.into())?;
    validate_size("mob travel", state.half_width, state.height)?;
    validate_kept_box("mob travel", state.bounding_box.clone().map(Into::into))?;
    let mut mob = MobTravelState {
        kind,
        position: state.position.into(),
        velocity: state.velocity.into(),
        yaw: state.yaw_degrees as f32,
        input: state.input.into(),
        speed: state.speed as f32,
        half_width: state.half_width,
        height: state.height,
        max_step: state.max_step.unwrap_or(0.6) as f32,
        bounding_box: state.bounding_box.clone().map(Into::into),
        on_ground: state.on_ground.unwrap_or(false),
        collided_horizontally: state.collided_horizontally.unwrap_or(false),
        collided_vertically: state.collided_vertically.unwrap_or(false),
        in_water: state.in_water.unwrap_or(false),
        in_lava: state.in_lava.unwrap_or(false),
        no_gravity: state.no_gravity.unwrap_or(false),
        stuck_speed_multiplier: state.stuck_speed_multiplier.map_or(Vec3::ZERO, Into::into),
        fall_distance: state.fall_distance.unwrap_or(0.0),
        has_target: state.has_target.unwrap_or(false),
        guardian_moving: state.guardian_moving.unwrap_or(false),
        move_out_of_range: false,
        climbing: state.climbing.unwrap_or(false),
        pushed_by_fluid: state.pushed_by_fluid.unwrap_or(kind.pushed_by_fluid()),
        fluid_jump_threshold: state.fluid_jump_threshold.unwrap_or(0.4),
        under_water: state.under_water.unwrap_or(false),
        wants_to_swim: state.wants_to_swim.unwrap_or(false),
        near_home: state.near_home.unwrap_or(false),
        marker: state.marker.unwrap_or(false),
        sweet_berry_bush_immune: matches!(state.kind.as_str(), "fox" | "bee"),
        body_item: state.body_item.unwrap_or(false),
        floats_in_liquids: state.floats_in_liquids.unwrap_or(false),
        tick_count: state.tick_count.unwrap_or(0),
        slow_falling: state.slow_falling.unwrap_or(false),
        levitation_amplifier: state.levitation_amplifier,
        dolphins_grace: state.dolphins_grace.unwrap_or(false),
        gravity: state.gravity,
        water_movement_efficiency: state.water_movement_efficiency.unwrap_or(0.0),
        air_drag_modifier: state.air_drag_modifier,
        discard_friction: state.discard_friction.unwrap_or(false),
    };
    // LivingEntity.travel runs only for a mob with its AI or one its rider steers here: a NoAI mob nobody rides
    // moves not at all (1.8.8 EntityLivingBase.moveEntityWithHeading's isServerWorld, EntityLiving.java isAIDisabled;
    // 1.16.5 LivingEntity.java:1959-1960; from 1.19.4 isControlledByLocalInstance, 1.19.4 LivingEntity.java:1992-1993,
    // which is isEffectiveAi for a mob with no player steering it). In 1.9 and 1.10 the other term, canPassengerSteer, is
    // Entity's own, true on the server for a mob nobody rides (1.9.4 Entity.java:2253-2256, LivingEntity.java:1323-1324),
    // so a NoAI mob travels there; 1.11 MobEntity asks canBeControlledByRider as well (1.11.2 MobEntity.java:1087-1089).
    let travels_anyway = release.at_least(1, 9, 0) && !release.at_least(1, 11, 0);
    if state.effective_ai == Some(false) && !travels_anyway {
        return Ok(state);
    }
    tick_mob_travel(&mut mob, world.snapshot(), release);
    check_move("simulateVersionedMobTravel", mob.move_out_of_range)?;
    Ok(NativeMobTravelState {
        position: mob.position.into(),
        velocity: mob.velocity.into(),
        bounding_box: mob.bounding_box.map(Into::into),
        on_ground: Some(mob.on_ground),
        collided_horizontally: Some(mob.collided_horizontally),
        collided_vertically: Some(mob.collided_vertically),
        stuck_speed_multiplier: Some(mob.stuck_speed_multiplier.into()),
        fall_distance: Some(mob.fall_distance),
        ..state
    })
}

/// A sample of a dragon's flight history.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeDragonFlightSample {
    pub y: f64,
    pub yaw_degrees: f64,
}

/// A dragon as its AI leaves it, on the server. What the AI decides is the caller's.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeDragonState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    /// Entity.getYRot, in degrees.
    pub yaw_degrees: f64,
    /// EnderDragon.yRotA.
    pub yaw_acceleration: Option<f64>,
    /// EnderDragon.inWall: the last tick's.
    pub in_wall: Option<bool>,
    /// Entity's kept box, before 1.17.
    pub bounding_box: Option<NativeAabb>,
    /// DragonPhaseInstance.getFlyTargetLocation; absent flies nowhere. In 1.8 the target after the AI.
    pub target: Option<NativeVec3>,
    /// 1.8: the target the tick began with.
    pub steering_target: Option<NativeVec3>,
    /// DragonPhaseInstance.getFlySpeed.
    pub fly_speed: Option<f64>,
    /// DragonPhaseInstance.getTurnSpeed.
    pub turn_speed: Option<f64>,
    /// DragonPhaseInstance.isSitting.
    pub sitting: Option<bool>,
    /// Where it was, newest first.
    pub history: Option<Vec<NativeDragonFlightSample>>,
    /// `history` already has this tick's sample: the flight alone runs.
    pub history_recorded: Option<bool>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeDragonPart {
    /// "head", "neck", "body", "tail" or "wing".
    pub name: String,
    /// Its entity id less the dragon's.
    pub id_offset: i32,
    pub position: NativeVec3,
    pub bounding_box: NativeAabb,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeDragonTickResult {
    pub state: NativeDragonState,
    /// In the order of the entity ids: as Level.getEntities gives them.
    pub parts: Vec<NativeDragonPart>,
    /// The parts enter an entity list right after the dragon, seen only when its own box is; else
    /// after all of the level's other entities.
    pub parts_follow_dragon: bool,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeDragonPushTarget {
    pub id: i32,
    pub position: NativeVec3,
    pub bounding_box: NativeAabb,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeDragonPush {
    pub id: i32,
    pub impulse: NativeVec3,
}

fn native_part(part: DragonPart) -> NativeDragonPart {
    NativeDragonPart {
        name: part.name.to_owned(),
        id_offset: part.id_offset,
        position: part.position.into(),
        bounding_box: part.bounding_box.into(),
    }
}

fn dragon_state(value: &NativeDragonState) -> DragonState {
    DragonState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        yaw: value.yaw_degrees as f32,
        yaw_acceleration: value.yaw_acceleration.unwrap_or(0.0) as f32,
        in_wall: value.in_wall.unwrap_or(false),
        bounding_box: value.bounding_box.clone().map(Into::into),
        target: value.target.map(Into::into),
        steering_target: value.steering_target.map(Into::into),
        fly_speed: value.fly_speed.unwrap_or(0.6) as f32,
        turn_speed: value.turn_speed.unwrap_or(0.0) as f32,
        sitting: value.sitting.unwrap_or(false),
        history: DragonFlightHistory::new(
            value
                .history
                .iter()
                .flatten()
                .map(|sample| DragonFlightSample {
                    y: sample.y,
                    yaw: sample.yaw_degrees as f32,
                })
                .collect(),
        ),
    }
}

/// One tick of a dragon's flight, and where it leaves the parts: the boxes projectiles hit.
#[napi]
pub fn simulate_versioned_dragon_tick(
    version: String,
    state: NativeDragonState,
) -> Result<NativeDragonTickResult> {
    let release = vehicle_release(&version, "dragon flight")?;
    let mut dragon = dragon_state(&state);
    let parts = if state.history_recorded.unwrap_or(false) {
        tick_dragon_flight(&mut dragon, release);
        dragon_parts(&dragon, release)
    } else {
        tick_dragon(&mut dragon, release)
    };
    Ok(NativeDragonTickResult {
        parts: parts.into_iter().map(native_part).collect(),
        parts_follow_dragon: parts_follow_dragon(release),
        state: NativeDragonState {
            position: dragon.position.into(),
            velocity: dragon.velocity.into(),
            yaw_degrees: f64::from(dragon.yaw),
            yaw_acceleration: Some(f64::from(dragon.yaw_acceleration)),
            bounding_box: dragon.bounding_box.map(Into::into),
            history: Some(
                dragon
                    .history
                    .samples()
                    .iter()
                    .map(|sample| NativeDragonFlightSample {
                        y: sample.y,
                        yaw_degrees: f64::from(sample.yaw),
                    })
                    .collect(),
            ),
            history_recorded: None,
            ..state
        },
    })
}

/// EnderDragon.knockBack of both wings, from where the parts are. `targets` are the living entities
/// around that are not in creative or spectator mode.
#[napi]
pub fn dragon_wing_pushes_for_version(
    version: String,
    parts: Vec<NativeDragonPart>,
    targets: Vec<NativeDragonPushTarget>,
) -> Result<Vec<NativeDragonPush>> {
    let release = vehicle_release(&version, "dragon wings")?;
    let parts: Vec<DragonPart> = parts
        .into_iter()
        .map(|part| DragonPart {
            name: match part.name.as_str() {
                "body" => "body",
                "wing" => "wing",
                _ => "other",
            },
            id_offset: part.id_offset,
            position: part.position.into(),
            bounding_box: part.bounding_box.into(),
        })
        .collect();
    let targets: Vec<DragonPushTarget> = targets
        .into_iter()
        .map(|target| DragonPushTarget {
            id: target.id,
            position: target.position.into(),
            bounding_box: target.bounding_box.into(),
        })
        .collect();
    Ok(dragon_wing_pushes(&parts, &targets, release)
        .into_iter()
        .map(|push| NativeDragonPush {
            id: push.id,
            impulse: push.impulse.into(),
        })
        .collect())
}
