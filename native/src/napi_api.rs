use napi::bindgen_prelude::Result;
use napi::Error;
use napi_derive::napi;

use crate::entity::{
    boost_by_attached_rockets, entity_push_impulses_with_pushthrough, nearby_entity_colliders,
    push_by_nearby_entities, Dimensions, EntityFlags, EntityPush, EntityState, NearbyEntity,
    NearbyEntityKind,
};
use crate::math::{Aabb, Vec3};
use crate::piston::{push_by_moving_pistons, MovingPiston};
use crate::projectile::{
    tick_arrow, tick_projectile, tick_projectile_with_bubble_column, tick_trident, ArrowState,
    ProjectileKind, ProjectileState, TridentState,
};
use crate::ridden::{tick_ridden_mob, RiddenMobKind, RiddenMobState, RiderInput};
use crate::simulation::{PhysicsConfig, SimulationState};
use crate::trig;
use crate::vehicle::{
    tick_boat, tick_boat_in_world, tick_horse, BoatSimulationState, BoatStatus,
    HorseSimulationState, LegacyBoatState,
};
use crate::version::MinecraftVersion;
use crate::versioned;
use crate::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

#[napi(object)]
#[derive(Clone, Copy)]
pub struct NativeVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[napi(object)]
pub struct NativeAabb {
    pub min: NativeVec3,
    pub max: NativeVec3,
}

#[napi(object)]
pub struct NativeWorldBounds {
    pub min_build_height: i32,
    pub max_build_height: i32,
    pub border_center_x: f64,
    pub border_center_z: f64,
    pub border_size: f64,
}

#[napi(object)]
pub struct NativeBlockState {
    pub present: bool,
    pub id: u32,
    pub state_version: Option<String>,
    pub properties: Vec<NativeBlockProperty>,
    pub tags: u32,
    pub fluid: u8,
    pub fluid_tags: u32,
    pub fluid_level: u8,
    pub fluid_source: bool,
    pub fluid_flow: NativeVec3,
    pub fluid_falling: bool,
    pub fall_damage_resetting: bool,
    pub effect: u8,
    pub slipperiness: f64,
    pub bounce_restitution: f64,
    pub suppresses_bounce: bool,
    pub shape: Option<NativeAabb>,
    pub outline_shape: Option<NativeAabb>,
    pub interaction_shape: Option<NativeAabb>,
    pub block_entity: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeBlockProperty {
    pub name: String,
    pub value: String,
}

#[napi(object)]
pub struct NativeEntityDescriptor {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub half_width: f64,
    pub height: f64,
    pub on_ground: bool,
    pub yaw: f64,
    pub in_water: bool,
    pub in_lava: bool,
    pub version: Option<String>,
    pub pose: Option<u8>,
    pub pitch: Option<f64>,
    pub input: Option<NativeVec3>,
    pub jump_requested: Option<bool>,
    pub jump_ticks: Option<u32>,
    pub sprinting: Option<bool>,
    pub sneaking: Option<bool>,
    pub using_item: Option<bool>,
    pub swimming: Option<bool>,
    pub passenger: Option<bool>,
    pub flying: Option<bool>,
    pub flying_speed: Option<f64>,
    pub climbing: Option<bool>,
    pub no_physics: Option<bool>,
    pub no_gravity: Option<bool>,
    pub fall_flying: Option<bool>,
    pub fall_damage_immune: Option<bool>,
    pub fall_damage_multiplier: Option<f64>,
    pub slow_falling: Option<bool>,
    pub levitation: Option<bool>,
    pub levitation_amplifier: Option<u8>,
    pub movement_speed: Option<f64>,
    pub jump_strength: Option<f64>,
    pub speed_effect_amplifier: Option<u8>,
    pub slowness_effect_amplifier: Option<u8>,
    pub jump_boost_amplifier: Option<u8>,
    pub dolphins_grace: Option<u8>,
    pub depth_strider: Option<u8>,
    pub soul_speed: Option<u8>,
    pub water_movement_efficiency: Option<f64>,
    pub movement_efficiency: Option<f64>,
    pub air_drag_modifier: Option<f64>,
    pub friction_modifier: Option<f64>,
    pub sneaking_speed: Option<f64>,
    pub look: Option<NativeVec3>,
    pub firework_boost: Option<NativeVec3>,
}

impl From<NativeVec3> for Vec3 {
    fn from(value: NativeVec3) -> Self {
        Self::new(value.x, value.y, value.z)
    }
}

impl From<Vec3> for NativeVec3 {
    fn from(value: Vec3) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

impl From<NativeAabb> for Aabb {
    fn from(value: NativeAabb) -> Self {
        Self::new(value.min.into(), value.max.into())
    }
}

impl From<Aabb> for NativeAabb {
    fn from(value: Aabb) -> Self {
        Self {
            min: value.min.into(),
            max: value.max.into(),
        }
    }
}

#[napi(object)]
pub struct NativeSimulationConfig {
    pub gravity: Option<f64>,
    pub drag: Option<f64>,
    pub movement_speed: Option<f64>,
    pub sprint_multiplier: Option<f64>,
    pub jump_velocity: Option<f64>,
    pub step_height: Option<f64>,
    pub negligible_velocity: Option<f64>,
}

impl From<NativeSimulationConfig> for PhysicsConfig {
    fn from(value: NativeSimulationConfig) -> Self {
        let defaults = Self::default();
        Self {
            gravity: value.gravity.unwrap_or(defaults.gravity),
            drag: value.drag.unwrap_or(defaults.drag),
            movement_speed: value.movement_speed.unwrap_or(defaults.movement_speed),
            sprint_multiplier: value
                .sprint_multiplier
                .unwrap_or(defaults.sprint_multiplier),
            jump_velocity: value.jump_velocity.unwrap_or(defaults.jump_velocity),
            step_height: value.step_height.unwrap_or(defaults.step_height),
            negligible_velocity: value
                .negligible_velocity
                .unwrap_or(defaults.negligible_velocity),
        }
    }
}

#[napi(object)]
pub struct NativeSimulationState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub half_width: f64,
    pub height: f64,
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub minor_horizontal_collision: Option<bool>,
    pub collided_vertically: bool,
    pub vertical_collision_below: Option<bool>,
    pub block_fall_callback: Option<bool>,
    pub entity_bounciness: Option<f64>,
    pub block_bounciness: Option<f64>,
    pub input: Option<NativeVec3>,
    pub jump_requested: Option<bool>,
    pub jump_ticks: Option<u32>,
    pub auto_jump: Option<bool>,
    pub auto_jump_time: Option<u32>,
    pub riptide_launch: Option<u8>,
    pub sprinting: Option<bool>,
    pub was_sprinting: Option<bool>,
    /// Whether the movement speed attribute holds the sprint modifier (LivingEntity.setSprinting
    /// adds it, a sync from the server without it drops it); `sprinting` when left out.
    pub sprint_modifier: Option<bool>,
    pub in_water: Option<bool>,
    pub in_lava: Option<bool>,
    pub ultra_warm: Option<bool>,
    pub is_player: Option<bool>,
    pub controlled_by_local_instance: Option<bool>,
    pub passenger: Option<bool>,
    pub in_web: Option<bool>,
    pub weaving: Option<bool>,
    pub sweet_berry_bush_immune: Option<bool>,
    pub powder_snow_immune: Option<bool>,
    pub powder_snow_walkable: Option<bool>,
    pub in_powder_snow: Option<bool>,
    pub ticks_frozen: Option<u32>,
    pub ticks_required_to_freeze: Option<u32>,
    pub tick_count: Option<u32>,
    pub freeze_damage: Option<f64>,
    pub contact_damage: Option<f64>,
    pub inside_wall: Option<bool>,
    pub eye_height: Option<f64>,
    pub eye_in_water: Option<bool>,
    pub eye_in_lava: Option<bool>,
    pub slow_falling: Option<bool>,
    pub levitation: Option<bool>,
    pub levitation_amplifier: Option<u8>,
    pub fall_distance: Option<f64>,
    pub safe_fall_distance: Option<f64>,
    pub fall_damage_immune: Option<bool>,
    pub fall_damage_multiplier: Option<f64>,
    pub sneaking: Option<bool>,
    pub moving_slowly: Option<bool>,
    pub stepping_carefully: Option<bool>,
    pub using_item: Option<bool>,
    pub item_use_speed_multiplier: Option<f64>,
    pub swimming: Option<bool>,
    pub flying: Option<bool>,
    pub flying_speed: Option<f64>,
    pub climbing: Option<bool>,
    pub no_physics: Option<bool>,
    pub no_gravity: Option<bool>,
    pub removed: Option<bool>,
    pub gravity: Option<f64>,
    pub step_height: Option<f64>,
    pub movement_speed: Option<f64>,
    pub jump_strength: Option<f64>,
    pub speed_effect_amplifier: Option<u8>,
    pub slowness_effect_amplifier: Option<u8>,
    pub jump_boost_amplifier: Option<u8>,
    pub dolphins_grace: Option<u8>,
    pub depth_strider: Option<u8>,
    pub soul_speed: Option<u8>,
    pub water_movement_efficiency: Option<f64>,
    pub movement_efficiency: Option<f64>,
    pub air_drag_modifier: Option<f64>,
    pub friction_modifier: Option<f64>,
    pub sneaking_speed: Option<f64>,
    pub fall_damage: Option<f64>,
    pub fall_flying: Option<bool>,
    pub look: Option<NativeVec3>,
    pub firework_boost: Option<NativeVec3>,
    pub stuck_speed_multiplier: Option<NativeVec3>,
    pub previous_position: Option<NativeVec3>,
    pub bounding_box: Option<NativeAabb>,
    pub nearby_entities: Option<Vec<NativeNearbyEntity>>,
    pub firework_rockets: Option<u32>,
    pub firework_rockets_before_player: Option<u32>,
    pub moving_pistons: Option<Vec<NativeMovingPiston>>,
}

#[napi(object)]
pub struct NativeMovingPiston {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// The piston's facing as Direction.from3DDataValue: down, up, north, south, west, east.
    pub facing: u8,
    pub extending: bool,
    pub source: bool,
    pub progress: f64,
    pub shapes: Vec<NativeAabb>,
}

impl TryFrom<NativeMovingPiston> for MovingPiston {
    type Error = Error;

    fn try_from(value: NativeMovingPiston) -> Result<Self> {
        if value.facing > 5 || !value.progress.is_finite() {
            return Err(Error::from_reason("moving piston contains invalid values"));
        }
        Ok(Self {
            position: (value.x, value.y, value.z),
            facing: value.facing,
            extending: value.extending,
            source: value.source,
            progress: value.progress as f32,
            shapes: value.shapes.into_iter().map(Into::into).collect(),
        })
    }
}

#[napi(object)]
pub struct NativePistonPushResult {
    pub state: NativeSimulationState,
    /// How far the pistons of the tick have moved the player on each axis (Entity.pistonDeltas).
    pub piston_deltas: NativeVec3,
}

#[napi(object)]
pub struct NativeNearbyEntity {
    /// "living", "boat", "shulker" or "happy_ghast"; other entities neither push nor collide.
    pub kind: String,
    pub position: NativeVec3,
    pub bounding_box: NativeAabb,
    pub ticks_before_player: Option<bool>,
    pub tick_position: Option<NativeVec3>,
    pub tick_bounding_box: Option<NativeAabb>,
}

impl From<NativeNearbyEntity> for NearbyEntity {
    fn from(value: NativeNearbyEntity) -> Self {
        let position: Vec3 = value.position.into();
        let bounding_box: Aabb = value.bounding_box.into();
        Self {
            kind: NearbyEntityKind::parse(&value.kind),
            position,
            bounding_box,
            tick_position: value.tick_position.map_or(position, Into::into),
            tick_bounding_box: value.tick_bounding_box.map_or(bounding_box, Into::into),
            ticks_before_player: value.ticks_before_player.unwrap_or(false),
        }
    }
}

#[napi(object)]
pub struct NativeEntityPushState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub no_physics: bool,
    pub vehicle: bool,
    pub pushable: bool,
    pub pushthrough: f64,
}

#[napi(object)]
pub struct NativeEntityPushResult {
    pub left_velocity: NativeVec3,
    pub right_velocity: NativeVec3,
}

#[napi(object)]
pub struct NativeVersionedEntityState {
    pub simulation: NativeSimulationState,
    pub no_physics: bool,
    pub vehicle_id: Option<u32>,
    pub pushable: bool,
    pub pushthrough: f64,
}

#[napi(object)]
pub struct NativePoseRefreshResult {
    pub state: NativeSimulationState,
    pub pose: u8,
    pub changed: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeBlockShape {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
    pub slipperiness: f64,
    pub effect: Option<u8>,
    pub bounce_restitution: Option<f64>,
    pub suppresses_bounce: Option<bool>,
    pub fall_damage_resetting: Option<bool>,
}

#[napi(object)]
pub struct NativeGeneratedShapeBox {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

#[napi(object)]
pub struct NativeGeneratedShapeProvenance {
    pub source_sha256: Option<String>,
    pub has_method: bool,
    pub state_dependent: bool,
    pub property_dependencies: Vec<String>,
    pub candidate_box_count: i64,
}

#[napi(object)]
pub struct NativeGeneratedShapePredicate {
    pub property: String,
    pub operator: String,
    pub value: String,
}

#[napi(object)]
pub struct NativeBlockUpdate {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub id: Option<u32>,
    pub state_version: Option<String>,
    pub block_entity: Option<String>,
    pub remove_block_entity: Option<bool>,
    pub properties: Option<Vec<NativeBlockProperty>>,
    pub tags: Option<u32>,
    pub shape: Option<NativeBlockShape>,
    pub outline_shape: Option<NativeBlockShape>,
    pub interaction_shape: Option<NativeBlockShape>,
    pub fluid: Option<u8>,
    pub fluid_tags: Option<u32>,
    pub fluid_level: Option<u8>,
    pub fluid_source: Option<bool>,
    pub fluid_flow: Option<NativeVec3>,
    pub fluid_falling: Option<bool>,
    pub fall_damage_resetting: Option<bool>,
    pub bounce_restitution: Option<f64>,
    pub suppresses_bounce: Option<bool>,
    pub suffocating: Option<bool>,
    pub remove: Option<bool>,
}

#[napi(object)]
pub struct NativeVersionCapabilities {
    pub fluid_physics: bool,
    pub honey_blocks: bool,
    pub fall_flying: bool,
    pub water_movement_efficiency: bool,
    pub minor_horizontal_collision: bool,
}

#[napi(object)]
pub struct NativeBoatState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub yaw: f64,
    pub forward: bool,
    pub left: bool,
    pub right: bool,
    pub input_down: Option<bool>,
    pub delta_rotation: Option<f64>,
    pub in_water: bool,
    pub water_level: Option<f64>,
    pub status: Option<String>,
    pub previous_status: Option<String>,
    pub bubble_time: Option<i32>,
    pub bubble_column_direction_is_down: Option<bool>,
    pub above_bubble_column: Option<bool>,
    pub player_passenger: Option<bool>,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub on_ground: Option<bool>,
    pub yaw_degrees: Option<f64>,
    pub land_friction: Option<f64>,
    pub last_vertical_velocity: Option<f64>,
    pub bounding_box: Option<NativeAabb>,
    pub speed_multiplier: Option<f64>,
    pub rider_yaw: Option<f64>,
    pub rider_forward: Option<f64>,
    pub rider_strafe: Option<f64>,
}

#[napi(object)]
pub struct NativeHorseState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub yaw: f64,
    pub forward: f64,
    pub strafe: f64,
    pub jump_requested: bool,
    pub on_ground: bool,
    pub in_water: bool,
    pub movement_speed: f64,
    pub jump_strength: f64,
    pub half_width: Option<f64>,
    pub height: Option<f64>,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub entity_type: Option<String>,
    pub yaw_degrees: Option<f64>,
    pub pitch_degrees: Option<f64>,
    pub rider_yaw: Option<f64>,
    pub rider_pitch: Option<f64>,
    pub rider_sprinting: Option<bool>,
    pub rider_jump: Option<i32>,
    pub in_lava: Option<bool>,
    pub fall_distance: Option<f64>,
    pub bounding_box: Option<NativeAabb>,
    pub previous_position: Option<NativeVec3>,
    pub flying_speed: Option<f64>,
    pub saddled: Option<bool>,
    pub no_ai: Option<bool>,
    pub ridden_speed: Option<f64>,
    pub own_forward: Option<f64>,
    pub boosting: Option<bool>,
    pub boost_time: Option<i32>,
    pub boost_time_total: Option<i32>,
    pub suffocating: Option<bool>,
    pub jump_pending_scale: Option<f64>,
    pub horse_jumping: Option<bool>,
    pub standing: Option<bool>,
    pub allow_stand_sliding: Option<bool>,
    pub stand_counter: Option<i32>,
    pub dash_cooldown: Option<i32>,
    pub dashing: Option<bool>,
    pub sitting: Option<bool>,
    pub pose_time: Option<i64>,
    /// Where this tick's step toward a server teleport's target puts a ridden mob, in place of the
    /// fade of its motion.
    pub lerp_position: Option<NativeVec3>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeProjectileState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub gravity: Option<f64>,
    pub air_drag: Option<f64>,
    pub power: Option<NativeVec3>,
    pub in_water: Option<bool>,
    pub first_tick: Option<bool>,
    pub kind: Option<String>,
    pub horizontal_collision: Option<bool>,
    pub shot_at_angle: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeProjectileTickResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub block_hit: bool,
    pub first_tick: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeArrowState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub gravity: Option<f64>,
    pub air_drag: Option<f64>,
    pub water_inertia: Option<f64>,
    pub in_water: Option<bool>,
    pub in_ground: Option<bool>,
    pub life: Option<u16>,
    pub in_ground_time: Option<u32>,
    pub shake_time: Option<u8>,
    pub fall_impulse: Option<NativeVec3>,
    pub last_block_id: Option<u32>,
}

#[napi(object)]
pub struct NativeArrowTickResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub block_hit: bool,
    pub in_ground: bool,
    pub discarded: bool,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub started_falling: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeTridentState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub gravity: Option<f64>,
    pub air_drag: Option<f64>,
    pub water_inertia: Option<f64>,
    pub in_water: Option<bool>,
    pub dealt_damage: Option<bool>,
}

#[napi(object)]
pub struct NativeTridentTickResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub block_hit: bool,
}

impl From<NativeSimulationState> for SimulationState {
    fn from(value: NativeSimulationState) -> Self {
        Self {
            position: value.position.into(),
            velocity: value.velocity.into(),
            yaw: value.yaw.unwrap_or(0.0),
            pitch: value.pitch.unwrap_or(0.0),
            half_width: value.half_width,
            height: value.height,
            on_ground: value.on_ground,
            collided_horizontally: value.collided_horizontally,
            minor_horizontal_collision: value.minor_horizontal_collision.unwrap_or(false),
            minor_horizontal_collision_supported: false,
            collided_vertically: value.collided_vertically,
            vertical_collision_below: value.vertical_collision_below.unwrap_or(false),
            block_fall_callback: value.block_fall_callback.unwrap_or(false),
            entity_bounciness: value.entity_bounciness.unwrap_or(0.0),
            block_bounciness: value.block_bounciness.unwrap_or(0.0),
            input: value.input.map(Into::into).unwrap_or(Vec3::ZERO),
            jump_requested: value.jump_requested.unwrap_or(false),
            jump_ticks: value.jump_ticks.unwrap_or(0),
            auto_jump: value.auto_jump.unwrap_or(false),
            auto_jump_time: value.auto_jump_time.unwrap_or(0),
            riptide_launch: value.riptide_launch,
            sprinting: value.sprinting.unwrap_or(false),
            was_sprinting: value.was_sprinting.unwrap_or(false),
            sprint_modifier: value.sprint_modifier,
            in_water: value.in_water.unwrap_or(false),
            in_lava: value.in_lava.unwrap_or(false),
            ultra_warm: value.ultra_warm.unwrap_or(false),
            is_player: value.is_player.unwrap_or(false),
            controlled_by_local_instance: value.controlled_by_local_instance.unwrap_or(false),
            passenger: value.passenger.unwrap_or(false),
            in_web: value.in_web.unwrap_or(false),
            weaving: value.weaving.unwrap_or(false),
            sweet_berry_bush_immune: value.sweet_berry_bush_immune.unwrap_or(false),
            powder_snow_immune: value.powder_snow_immune.unwrap_or(false),
            powder_snow_walkable: value.powder_snow_walkable.unwrap_or(false),
            in_powder_snow: value.in_powder_snow.unwrap_or(false),
            ticks_frozen: value.ticks_frozen.unwrap_or(0),
            ticks_required_to_freeze: value.ticks_required_to_freeze.unwrap_or(140),
            tick_count: value.tick_count.unwrap_or(0),
            freeze_damage: value.freeze_damage.unwrap_or(0.0),
            contact_damage: value.contact_damage.unwrap_or(0.0),
            inside_wall: value.inside_wall.unwrap_or(false),
            eye_height: value.eye_height,
            eye_in_water: value.eye_in_water.unwrap_or(false),
            eye_in_lava: value.eye_in_lava.unwrap_or(false),
            slow_falling: value.slow_falling.unwrap_or(false),
            levitation: value.levitation.unwrap_or(false),
            levitation_amplifier: value.levitation_amplifier,
            fall_distance: value.fall_distance.unwrap_or(0.0),
            safe_fall_distance: value.safe_fall_distance.unwrap_or(0.0),
            fall_damage_immune: value.fall_damage_immune.unwrap_or(false),
            fall_damage_multiplier: Some(value.fall_damage_multiplier.unwrap_or(1.0)),
            sneaking: value.sneaking.unwrap_or(false),
            moving_slowly: value.moving_slowly,
            stepping_carefully: value.stepping_carefully.unwrap_or(false),
            using_item: value.using_item.unwrap_or(false),
            item_use_speed_multiplier: value.item_use_speed_multiplier.map(|speed| speed as f32),
            swimming: value.swimming.unwrap_or(false),
            flying: value.flying.unwrap_or(false),
            flying_speed: value.flying_speed.unwrap_or(0.02),
            climbing: value.climbing.unwrap_or(false),
            no_physics: value.no_physics.unwrap_or(false),
            no_gravity: value.no_gravity.unwrap_or(false),
            removed: value.removed.unwrap_or(false),
            gravity: value.gravity,
            step_height: value.step_height,
            movement_speed: value.movement_speed,
            jump_strength: value.jump_strength,
            speed_effect_amplifier: value.speed_effect_amplifier,
            slowness_effect_amplifier: value.slowness_effect_amplifier,
            jump_boost_amplifier: value.jump_boost_amplifier,
            dolphins_grace: value.dolphins_grace.unwrap_or(0),
            depth_strider: value.depth_strider.unwrap_or(0),
            soul_speed: value.soul_speed.unwrap_or(0),
            water_movement_efficiency: value.water_movement_efficiency.unwrap_or(0.0),
            movement_efficiency: value.movement_efficiency.unwrap_or(0.0),
            air_drag_modifier: value.air_drag_modifier.unwrap_or(0.0),
            friction_modifier: value.friction_modifier.unwrap_or(0.0),
            air_drag_modifier_present: value.air_drag_modifier.is_some(),
            friction_modifier_present: value.friction_modifier.is_some(),
            sneaking_speed: value.sneaking_speed,
            fall_damage: value.fall_damage.unwrap_or(0.0),
            fall_flying: value.fall_flying.unwrap_or(false),
            look: value
                .look
                .map(Into::into)
                .unwrap_or(Vec3::new(0.0, 0.0, 1.0)),
            firework_boost: value.firework_boost.map(Into::into).unwrap_or(Vec3::ZERO),
            stuck_speed_multiplier: value
                .stuck_speed_multiplier
                .map(Into::into)
                .unwrap_or(Vec3::ZERO),
            previous_position: value.previous_position.map(Into::into),
            bounding_box: value.bounding_box.map(Into::into),
        }
    }
}

impl From<SimulationState> for NativeSimulationState {
    fn from(value: SimulationState) -> Self {
        Self {
            position: value.position.into(),
            velocity: value.velocity.into(),
            yaw: Some(value.yaw),
            pitch: Some(value.pitch),
            half_width: value.half_width,
            height: value.height,
            on_ground: value.on_ground,
            collided_horizontally: value.collided_horizontally,
            minor_horizontal_collision: Some(value.minor_horizontal_collision),
            collided_vertically: value.collided_vertically,
            vertical_collision_below: Some(value.vertical_collision_below),
            block_fall_callback: Some(value.block_fall_callback),
            entity_bounciness: Some(value.entity_bounciness),
            block_bounciness: Some(value.block_bounciness),
            input: Some(value.input.into()),
            jump_requested: Some(value.jump_requested),
            jump_ticks: Some(value.jump_ticks),
            auto_jump: Some(value.auto_jump),
            auto_jump_time: Some(value.auto_jump_time),
            riptide_launch: value.riptide_launch,
            sprinting: Some(value.sprinting),
            was_sprinting: Some(value.was_sprinting),
            sprint_modifier: value.sprint_modifier,
            in_water: Some(value.in_water),
            in_lava: Some(value.in_lava),
            ultra_warm: Some(value.ultra_warm),
            is_player: Some(value.is_player),
            controlled_by_local_instance: Some(value.controlled_by_local_instance),
            passenger: Some(value.passenger),
            in_web: Some(value.in_web),
            weaving: Some(value.weaving),
            sweet_berry_bush_immune: Some(value.sweet_berry_bush_immune),
            powder_snow_immune: Some(value.powder_snow_immune),
            powder_snow_walkable: Some(value.powder_snow_walkable),
            in_powder_snow: Some(value.in_powder_snow),
            ticks_frozen: Some(value.ticks_frozen),
            ticks_required_to_freeze: Some(value.ticks_required_to_freeze),
            tick_count: Some(value.tick_count),
            freeze_damage: Some(value.freeze_damage),
            contact_damage: Some(value.contact_damage),
            inside_wall: Some(value.inside_wall),
            eye_height: value.eye_height,
            eye_in_water: Some(value.eye_in_water),
            eye_in_lava: Some(value.eye_in_lava),
            slow_falling: Some(value.slow_falling),
            levitation: Some(value.levitation),
            levitation_amplifier: value.levitation_amplifier,
            fall_distance: Some(value.fall_distance),
            safe_fall_distance: Some(value.safe_fall_distance),
            fall_damage_immune: Some(value.fall_damage_immune),
            fall_damage_multiplier: value.fall_damage_multiplier,
            sneaking: Some(value.sneaking),
            moving_slowly: value.moving_slowly,
            stepping_carefully: Some(value.stepping_carefully),
            using_item: Some(value.using_item),
            item_use_speed_multiplier: value.item_use_speed_multiplier.map(f64::from),
            swimming: Some(value.swimming),
            flying: Some(value.flying),
            flying_speed: Some(value.flying_speed),
            climbing: Some(value.climbing),
            no_physics: Some(value.no_physics),
            no_gravity: Some(value.no_gravity),
            removed: Some(value.removed),
            gravity: value.gravity,
            step_height: value.step_height,
            movement_speed: value.movement_speed,
            jump_strength: value.jump_strength,
            speed_effect_amplifier: value.speed_effect_amplifier,
            slowness_effect_amplifier: value.slowness_effect_amplifier,
            jump_boost_amplifier: value.jump_boost_amplifier,
            dolphins_grace: Some(value.dolphins_grace),
            depth_strider: Some(value.depth_strider),
            soul_speed: Some(value.soul_speed),
            water_movement_efficiency: Some(value.water_movement_efficiency),
            movement_efficiency: Some(value.movement_efficiency),
            air_drag_modifier: value
                .air_drag_modifier_present
                .then_some(value.air_drag_modifier),
            friction_modifier: value
                .friction_modifier_present
                .then_some(value.friction_modifier),
            sneaking_speed: value.sneaking_speed,
            fall_damage: Some(value.fall_damage),
            fall_flying: Some(value.fall_flying),
            look: Some(value.look.into()),
            firework_boost: Some(value.firework_boost.into()),
            stuck_speed_multiplier: Some(value.stuck_speed_multiplier.into()),
            previous_position: value.previous_position.map(Into::into),
            bounding_box: value.bounding_box.map(Into::into),
            nearby_entities: None,
            firework_rockets: None,
            firework_rockets_before_player: None,
            moving_pistons: None,
        }
    }
}

#[napi]
pub struct NativeWorld {
    inner: WorldSnapshot,
}

impl Default for NativeWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeWorld {
    pub(crate) fn snapshot(&self) -> &WorldSnapshot {
        &self.inner
    }
}

#[napi]
impl NativeWorld {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: WorldSnapshot::new(),
        }
    }

    #[napi]
    pub fn set_chunk_loaded(&mut self, chunk_x: i32, chunk_z: i32, loaded: bool) {
        self.inner.set_chunk_loaded(chunk_x, chunk_z, loaded);
    }

    #[napi]
    pub fn is_chunk_loaded(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.inner.is_chunk_loaded(chunk_x, chunk_z)
    }

    #[napi]
    pub fn set_world_bounds(
        &mut self,
        min_build_height: i32,
        max_build_height: i32,
        border_center_x: f64,
        border_center_z: f64,
        border_size: f64,
    ) -> Result<()> {
        if min_build_height >= max_build_height
            || !border_center_x.is_finite()
            || !border_center_z.is_finite()
            || !border_size.is_finite()
            || border_size < 0.0
        {
            return Err(Error::from_reason("world bounds are invalid"));
        }
        self.inner.set_world_bounds(
            min_build_height,
            max_build_height,
            border_center_x,
            border_center_z,
            border_size,
        );
        Ok(())
    }

    #[napi]
    pub fn is_inside_world_border(&self, x: f64, z: f64) -> bool {
        self.inner
            .is_inside_world_border(Vec3::new(x, 0.0, z))
    }

    #[napi]
    pub fn is_inside_build_height(&self, y: i32) -> bool {
        self.inner.is_inside_build_height(y)
    }

    #[napi]
    pub fn is_section_present(&self, chunk_x: i32, section_y: i32, chunk_z: i32) -> bool {
        self.inner
            .is_section_present(chunk_x, section_y, chunk_z)
    }

    #[napi]
    pub fn set_section_present(
        &mut self,
        chunk_x: i32,
        section_y: i32,
        chunk_z: i32,
        present: bool,
    ) {
        self.inner
            .set_section_present(chunk_x, section_y, chunk_z, present);
    }

    #[napi]
    pub fn section_block_count(&self, chunk_x: i32, section_y: i32, chunk_z: i32) -> u32 {
        self.inner
            .section_block_count(chunk_x, section_y, chunk_z)
            .try_into()
            .unwrap_or(u32::MAX)
    }

    #[napi]
    pub fn world_bounds(&self) -> NativeWorldBounds {
        let (min_build_height, max_build_height, border_center_x, border_center_z, border_size) =
            self.inner.world_bounds();
        NativeWorldBounds {
            min_build_height,
            max_build_height,
            border_center_x,
            border_center_z,
            border_size,
        }
    }

    #[napi]
    pub fn clamp_to_world_border(&self, position: NativeVec3) -> NativeVec3 {
        self.inner.clamp_to_world_border(position.into()).into()
    }

    #[napi]
    pub fn set_full_block(&mut self, x: i32, y: i32, z: i32, id: u32) {
        self.inner.set_block(
            (x, y, z),
            BlockState {
                id,
                tags: 0,
                shape: Some(BlockShape::full_block()),
                outline_shape: Some(BlockShape::full_block()),
                interaction_shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
    }

    #[napi]
    pub fn set_block_state_id(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        version: String,
        id: u32,
    ) -> Result<()> {
        let version = MinecraftVersion::parse(&version)
            .ok_or_else(|| Error::from_reason("block state version must be a release name"))?;
        if !versioned::is_supported(&version.to_string()) {
            return Err(Error::from_reason(
                "block state version is not present in the source-backed release registry",
            ));
        }
        if !self.inner.contains_block((x, y, z)) {
            return Err(Error::from_reason(
                "block state IDs require an explicitly stored block state",
            ));
        }
        self.inner
            .set_block_state_id((x, y, z), crate::world::VersionedBlockStateId { version, id });
        Ok(())
    }

    #[napi]
    pub fn set_block_entity(&mut self, x: i32, y: i32, z: i32, kind: String) -> Result<()> {
        if kind.is_empty() {
            return Err(Error::from_reason("block entity kind must not be empty"));
        }
        if !self.inner.contains_block((x, y, z)) {
            return Err(Error::from_reason(
                "block entities require an explicitly stored block state",
            ));
        }
        self.inner.set_block_entity((x, y, z), kind);
        Ok(())
    }

    #[napi]
    pub fn remove_block_entity(&mut self, x: i32, y: i32, z: i32) {
        self.inner.remove_block_entity((x, y, z));
    }

    #[napi]
    pub fn set_block_properties(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        properties: Vec<NativeBlockProperty>,
    ) -> Result<()> {
        if !self.inner.contains_block((x, y, z)) {
            return Err(Error::from_reason(
                "block properties require an explicitly stored block state",
            ));
        }
        validate_block_properties(&properties)?;
        self.inner.set_block_properties(
            (x, y, z),
            properties
                .into_iter()
                .map(|property| (property.name, property.value))
                .collect(),
        );
        Ok(())
    }

    #[napi]
    pub fn set_block_shape(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        id: u32,
        shape: NativeBlockShape,
    ) -> Result<()> {
        let bounds = crate::math::Aabb::new(
            Vec3::new(shape.min_x, shape.min_y, shape.min_z),
            Vec3::new(shape.max_x, shape.max_y, shape.max_z),
        );
        if !valid_collision_shape(bounds, shape.slipperiness) {
            return Err(Error::from_reason(
                "block shape bounds must be finite, ordered, and reach at most one block out of their own",
            ));
        }
        self.inner.set_block(
            (x, y, z),
            BlockState {
                id,
                tags: 0,
                fluid_tags: 0,
                shape: Some(BlockShape {
                    bounds,
                    slipperiness: shape.slipperiness,
                    fluid: FluidState::Empty,
                    effect: block_effect(shape.effect.unwrap_or(0))?,
                }),
                outline_shape: Some(BlockShape {
                    bounds,
                    slipperiness: shape.slipperiness,
                    fluid: FluidState::Empty,
                    effect: block_effect(shape.effect.unwrap_or(0))?,
                }),
                interaction_shape: Some(BlockShape {
                    bounds,
                    slipperiness: shape.slipperiness,
                    fluid: FluidState::Empty,
                    effect: block_effect(shape.effect.unwrap_or(0))?,
                }),
                fluid: FluidState::Empty,
                fluid_level: 0,
                fluid_source: false,
                fluid_flow: Vec3::ZERO,
                fluid_falling: false,
                fall_damage_resetting: shape.fall_damage_resetting.unwrap_or(false),
                bounce_restitution: shape.bounce_restitution.unwrap_or(0.0),
                suppresses_bounce: shape.suppresses_bounce.unwrap_or(false),
                suffocating: None,
            },
        );
        Ok(())
    }

    #[napi]
    pub fn set_block_outline_shape(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        shape: NativeBlockShape,
    ) -> Result<()> {
        let bounds = crate::math::Aabb::new(
            Vec3::new(shape.min_x, shape.min_y, shape.min_z),
            Vec3::new(shape.max_x, shape.max_y, shape.max_z),
        );
        if !valid_shape(bounds, shape.slipperiness) {
            return Err(Error::from_reason(
                "outline shape bounds must be finite, ordered, and within one block",
            ));
        }
        let mut state = self.inner.block((x, y, z));
        state.outline_shape = Some(BlockShape {
            bounds,
            slipperiness: shape.slipperiness,
            fluid: FluidState::Empty,
            effect: block_effect(shape.effect.unwrap_or(0))?,
        });
        self.inner.set_block((x, y, z), state);
        Ok(())
    }

    #[napi]
    pub fn set_block_shapes(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        id: u32,
        shapes: Vec<NativeBlockShape>,
        outline_shapes: Option<Vec<NativeBlockShape>>,
    ) -> Result<()> {
        let mut converted = Vec::with_capacity(shapes.len());
        let bounce_restitution = shapes
            .first()
            .and_then(|shape| shape.bounce_restitution)
            .unwrap_or(0.0);
        let suppresses_bounce = shapes
            .first()
            .and_then(|shape| shape.suppresses_bounce)
            .unwrap_or(false);
        let fall_damage_resetting = shapes
            .first()
            .and_then(|shape| shape.fall_damage_resetting)
            .unwrap_or(false);
        for shape in shapes {
            let bounds = crate::math::Aabb::new(
                Vec3::new(shape.min_x, shape.min_y, shape.min_z),
                Vec3::new(shape.max_x, shape.max_y, shape.max_z),
            );
            if !valid_collision_shape(bounds, shape.slipperiness) {
                return Err(Error::from_reason(
                    "block shape bounds must be finite, ordered, and reach at most one block out of their own",
                ));
            }
            converted.push(BlockShape {
                bounds,
                slipperiness: shape.slipperiness,
                fluid: FluidState::Empty,
                effect: block_effect(shape.effect.unwrap_or(0))?,
            });
        }
        let outline_shapes = match outline_shapes {
            Some(shapes) => shapes
                .into_iter()
                .map(native_block_shape)
                .collect::<Result<Vec<_>>>()?,
            None => converted.clone(),
        };
        self.inner.set_block_shapes_with_outline(
            (x, y, z),
            BlockState {
                id,
                interaction_shape: None,
                bounce_restitution,
                suppresses_bounce,
                fall_damage_resetting,
                ..BlockState::default()
            },
            converted,
            outline_shapes,
        );
        Ok(())
    }

    #[napi]
    pub fn remove_block(&mut self, x: i32, y: i32, z: i32) {
        self.inner.remove_block((x, y, z));
    }

    #[napi]
    pub fn apply_block_updates(&mut self, updates: Vec<NativeBlockUpdate>) -> Result<()> {
        let mut grouped = std::collections::HashMap::new();
        for update in &updates {
            let position = (update.x, update.y, update.z);
            if update.remove.unwrap_or(false) {
                grouped.remove(&position);
                self.inner.remove_block(position);
                continue;
            }
            if let Some(properties) = update.properties.as_ref() {
                validate_block_properties(properties)?;
            }
            let state = if update.id.is_none()
                && update.tags.is_none()
                && update.shape.is_none()
                && update.outline_shape.is_none()
                && update.interaction_shape.is_none()
                && update.fluid.is_none()
                && update.fluid_tags.is_none()
                && update.fluid_level.is_none()
                && update.fluid_source.is_none()
                && update.fluid_flow.is_none()
                && update.fluid_falling.is_none()
                && update.fall_damage_resetting.is_none()
                && update.bounce_restitution.is_none()
                && update.suppresses_bounce.is_none()
                && update.suffocating.is_none()
            {
                self.inner.block(position)
            } else {
                block_update_state(update)?
            };
            let entry = grouped
                .entry(position)
                .or_insert_with(|| (state, Vec::new(), Vec::new(), Vec::new(), None, None, None, false));
            if let Some(shape) = state.shape {
                entry.1.push(shape);
            }
            if let Some(shape) = state.outline_shape {
                entry.2.push(shape);
            }
            if let Some(shape) = state.interaction_shape {
                entry.3.push(shape);
            }
            entry.0 = state;
            entry.4 = update.properties.as_ref().map(|properties| {
                    properties
                        .iter()
                        .map(|property| (property.name.clone(), property.value.clone()))
                        .collect()
                });
            entry.5 = match update.state_version.as_deref() {
                Some(value) => Some(crate::world::VersionedBlockStateId {
                    version: MinecraftVersion::parse(value).ok_or_else(|| {
                        Error::from_reason("block state version must be a release name")
                    })?,
                    id: update.id.unwrap_or(state.id),
                }),
                None => None,
            };
            entry.6 = update.block_entity.clone();
            entry.7 = update.remove_block_entity.unwrap_or(false);
        }
        for (position, (state, shapes, outline_shapes, interaction_shapes, properties, state_id, block_entity, remove_block_entity)) in grouped {
            if shapes.is_empty() {
                self.inner.set_block(position, state);
            } else {
                self.inner.set_block_shapes_with_geometry(
                    position,
                    state,
                    shapes,
                    outline_shapes,
                    interaction_shapes,
                );
            }
            if let Some(properties) = properties {
                self.inner.set_block_properties(position, properties);
            }
            if let Some(state_id) = state_id {
                self.inner.set_block_state_id(position, state_id);
            }
            if let Some(kind) = block_entity {
                if kind.is_empty() {
                    return Err(Error::from_reason("block entity kind must not be empty"));
                }
                self.inner.set_block_entity(position, kind);
            } else if remove_block_entity {
                self.inner.remove_block_entity(position);
            }
        }
        Ok(())
    }

    #[napi]
    pub fn set_fluid(&mut self, x: i32, y: i32, z: i32, fluid: u8) -> Result<()> {
        let fluid = match fluid {
            0 => FluidState::Empty,
            1 => FluidState::Water,
            2 => FluidState::Lava,
            3 => FluidState::Waterlogged,
            _ => return Err(Error::from_reason("fluid must be 0, 1, 2, or 3")),
        };
        self.inner.set_block(
            (x, y, z),
            BlockState {
                id: 0,
                tags: 0,
                shape: None,
                outline_shape: None,
                interaction_shape: None,
                fluid,
                fluid_tags: 0,
                fluid_level: 0,
                fluid_source: true,
                fluid_flow: Vec3::ZERO,
                fluid_falling: false,
                fall_damage_resetting: false,
                bounce_restitution: 0.0,
                suppresses_bounce: false,
                suffocating: None,
            },
        );
        Ok(())
    }

    #[napi]
    pub fn set_fluid_state(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        fluid: u8,
        fluid_tags: u32,
        fluid_level: u8,
        fluid_source: bool,
        fluid_falling: bool,
        fluid_flow: NativeVec3,
    ) -> Result<()> {
        let fluid = decode_fluid(fluid)?;
        let flow = fluid_flow.into();
        validate_fluid_state(fluid, fluid_level, fluid_source, fluid_falling, flow)?;
        self.inner.set_block(
            (x, y, z),
            BlockState {
                id: 0,
                tags: 0,
                fluid_tags,
                shape: None,
                outline_shape: None,
                interaction_shape: None,
                fluid,
                fluid_level,
                fluid_source,
                fluid_flow: flow,
                fluid_falling,
                ..BlockState::default()
            },
        );
        Ok(())
    }

    #[napi]
    pub fn set_versioned_fluid_state(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        version: String,
        fluid: u8,
        fluid_tags: u32,
        fluid_level: u8,
        fluid_source: bool,
        fluid_falling: bool,
        fluid_flow: NativeVec3,
    ) -> Result<()> {
        let fluid_state = decode_fluid(fluid)?;
        let fluid_id = generated_fluid_id(fluid_state)
            .ok_or_else(|| Error::from_reason("versioned fluid state does not support this fluid"))?;
        let mut properties = vec![(
            String::from("FALLING"),
            if fluid_falling { "true" } else { "false" }.to_owned(),
        )];
        if !fluid_source {
            properties.insert(0, (String::from("LEVEL"), fluid_level.to_string()));
        }
        let property_refs = properties
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        if crate::generated_data::fluid_state_index(&version, fluid_id, &property_refs).is_none() {
            return Err(Error::from_reason(
                "fluid state is not present in the generated source definition",
            ));
        }
        let flow = fluid_flow.into();
        validate_fluid_state(fluid_state, fluid_level, fluid_source, fluid_falling, flow)?;
        self.inner.set_block(
            (x, y, z),
            BlockState {
                id: 0,
                tags: 0,
                shape: None,
                outline_shape: None,
                interaction_shape: None,
                fluid: fluid_state,
                fluid_tags,
                fluid_level,
                fluid_source,
                fluid_flow: flow,
                fluid_falling,
                ..BlockState::default()
            },
        );
        Ok(())
    }

    #[napi]
    pub fn collision_shapes(&self, min: NativeVec3, max: NativeVec3) -> Result<Vec<NativeAabb>> {
        let query = crate::math::Aabb::new(min.into(), max.into());
        if !valid_bounds(query) {
            return Err(Error::from_reason(
                "collision query bounds must be finite and ordered",
            ));
        }

        Ok(self
            .inner
            .collision_shapes(query)
            .into_iter()
            .map(|bounds| NativeAabb {
                min: bounds.min.into(),
                max: bounds.max.into(),
            })
            .collect())
    }

    #[napi]
    pub fn outline_shapes(&self, min: NativeVec3, max: NativeVec3) -> Result<Vec<NativeAabb>> {
        let query = crate::math::Aabb::new(min.into(), max.into());
        if !valid_bounds(query) {
            return Err(Error::from_reason(
                "outline query bounds must be finite and ordered",
            ));
        }

        Ok(self.inner.outline_shapes(query).into_iter().map(|bounds| NativeAabb {
            min: bounds.min.into(),
            max: bounds.max.into(),
        }).collect())
    }

    #[napi]
    pub fn interaction_shapes(&self, min: NativeVec3, max: NativeVec3) -> Result<Vec<NativeAabb>> {
        let query = crate::math::Aabb::new(min.into(), max.into());
        if !valid_bounds(query) {
            return Err(Error::from_reason(
                "interaction query bounds must be finite and ordered",
            ));
        }
        Ok(self
            .inner
            .interaction_shapes(query)
            .into_iter()
            .map(|bounds| NativeAabb {
                min: bounds.min.into(),
                max: bounds.max.into(),
            })
            .collect())
    }

    #[napi]
    pub fn fluid_at(&self, x: i32, y: i32, z: i32) -> u8 {
        fluid_code(self.inner.fluid_at((x, y, z)))
    }

    #[napi]
    pub fn fluid_tags_at(&self, x: i32, y: i32, z: i32) -> u32 {
        self.inner.block((x, y, z)).fluid_tags
    }

    #[napi]
    pub fn fluid_height_at(&self, x: i32, y: i32, z: i32) -> f64 {
        self.inner.fluid_height_at((x, y, z))
    }

    #[napi]
    pub fn fluid_flow_at(&self, x: i32, y: i32, z: i32) -> NativeVec3 {
        self.inner.fluid_flow_at((x, y, z)).into()
    }

    #[napi]
    pub fn block_effect_at(&self, x: i32, y: i32, z: i32) -> u8 {
        block_effect_code(self.inner.effect_at((x, y, z)))
    }

    #[napi]
    pub fn block_tags_at(&self, x: i32, y: i32, z: i32) -> u32 {
        self.inner.block((x, y, z)).tags
    }

    #[napi]
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> NativeBlockState {
        let block = self.inner.block((x, y, z));
        let effect = block.shape.map_or(BlockEffect::None, |shape| shape.effect);
        NativeBlockState {
            present: self.inner.contains_block((x, y, z)),
            id: block.id,
            state_version: self
                .inner
                .block_state_id((x, y, z))
                .map(|state| state.version.to_string()),
            block_entity: self.inner.block_entity((x, y, z)).map(str::to_owned),
            properties: self.inner
                .block_properties((x, y, z))
                .into_iter()
                .map(|(name, value)| NativeBlockProperty { name, value })
                .collect(),
            tags: block.tags,
            fluid: fluid_code(block.fluid),
            fluid_tags: block.fluid_tags,
            fluid_level: block.fluid_level,
            fluid_source: block.fluid_source,
            fluid_flow: block.fluid_flow.into(),
            fluid_falling: block.fluid_falling,
            fall_damage_resetting: block.fall_damage_resetting,
            effect: block_effect_code(effect),
            slipperiness: block.shape.map_or(0.6, |shape| shape.slipperiness),
            shape: block.shape.map(|shape| NativeAabb {
                min: shape.bounds.min.into(),
                max: shape.bounds.max.into(),
            }),
            outline_shape: block.outline_shape.map(|shape| NativeAabb {
                min: shape.bounds.min.into(),
                max: shape.bounds.max.into(),
            }),
            interaction_shape: block.interaction_shape.map(|shape| NativeAabb {
                min: shape.bounds.min.into(),
                max: shape.bounds.max.into(),
            }),
            bounce_restitution: block.bounce_restitution,
            suppresses_bounce: block.suppresses_bounce,
        }
    }
}

#[napi]
pub fn simulate_tick(
    world: &NativeWorld,
    state: NativeSimulationState,
    config: NativeSimulationConfig,
) -> Result<NativeSimulationState> {
    let _ = (world, state, config);
    Err(Error::from_reason(
        "a concrete Minecraft version is required; use simulateVersionedTick",
    ))
}

/// One client tick of the local player: the attached rockets and nearby entities the client ticks
/// before it boost and push it, it ticks (colliding with the entities that are colliders), the
/// other rockets and entities boost and push it, then the moving pistons push it.
#[napi]
pub fn simulate_versioned_tick(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeSimulationState,
    config: NativeSimulationConfig,
) -> Result<NativeSimulationState> {
    let nearby_entities: Vec<NearbyEntity> = state
        .nearby_entities
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect();
    let moving_pistons = state
        .moving_pistons
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(MovingPiston::try_from)
        .collect::<Result<Vec<_>>>()?;
    let rockets = state.firework_rockets.unwrap_or(0);
    let rockets_before_player = state
        .firework_rockets_before_player
        .unwrap_or(0)
        .min(rockets);
    let mut simulation_state: SimulationState = state.into();
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let config = validated_simulation_config(config)?;
    tick_local_player(
        &version,
        minecraft_version,
        world,
        &mut simulation_state,
        &nearby_entities,
        (rockets, rockets_before_player),
        config,
    )?;
    if !moving_pistons.is_empty() {
        push_local_player_by_pistons(
            world,
            &mut simulation_state,
            &nearby_entities,
            &moving_pistons,
            &mut [0.0; 3],
            config,
            minecraft_version,
        );
    }
    Ok(simulation_state.into())
}

/// simulateVersionedTick's tick before the pistons: the rockets and entities the client ticks
/// before the player, its tick with the colliders among the entities in the world, then the
/// rockets and entities after it. `rockets` counts all the attached rockets and those before.
/// Returns the pushes the entities took back from the player.
pub(crate) fn tick_local_player(
    version: &str,
    release: MinecraftVersion,
    world: &mut NativeWorld,
    state: &mut SimulationState,
    nearby_entities: &[NearbyEntity],
    (rockets, rockets_before_player): (u32, u32),
    config: PhysicsConfig,
) -> Result<Vec<EntityPush>> {
    state.minor_horizontal_collision_supported = release.supports_minor_horizontal_collision();
    boost_by_attached_rockets(state, rockets_before_player, release);
    let mut pushes =
        push_by_nearby_entities(state, world.snapshot(), nearby_entities, true, release);
    world
        .inner
        .set_entity_colliders(nearby_entity_colliders(nearby_entities, false));
    let result = versioned::dispatch(version, state, world.snapshot(), config);
    world.inner.set_entity_colliders(Vec::new());
    result.map_err(Error::from_reason)?;
    boost_by_attached_rockets(state, rockets - rockets_before_player, release);
    pushes.extend(push_by_nearby_entities(
        state,
        world.snapshot(),
        nearby_entities,
        false,
        release,
    ));
    Ok(pushes)
}

/// The moving pistons' pushes after the player's tick, with the colliders among the entities
/// where their ticks left them.
pub(crate) fn push_local_player_by_pistons(
    world: &mut NativeWorld,
    state: &mut SimulationState,
    nearby_entities: &[NearbyEntity],
    moving_pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    world
        .inner
        .set_entity_colliders(nearby_entity_colliders(nearby_entities, true));
    push_by_moving_pistons(
        state,
        world.snapshot(),
        moving_pistons,
        piston_deltas,
        config,
        release,
    );
    world.inner.set_entity_colliders(Vec::new());
}

/// The pushes of the state's movingPistons alone, as simulateVersionedTick applies them after the
/// player's tick: for a caller that learns of the moving blocks only after the tick. The pistons
/// pushed earlier in the same tick moved the player by `piston_deltas`.
#[napi]
pub fn simulate_versioned_piston_push(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeSimulationState,
    config: NativeSimulationConfig,
    piston_deltas: Option<NativeVec3>,
) -> Result<NativePistonPushResult> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    let nearby_entities: Vec<NearbyEntity> = state
        .nearby_entities
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect();
    let moving_pistons = state
        .moving_pistons
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(MovingPiston::try_from)
        .collect::<Result<Vec<_>>>()?;
    let mut simulation_state: SimulationState = state.into();
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    simulation_state.minor_horizontal_collision_supported =
        minecraft_version.supports_minor_horizontal_collision();
    let config = validated_simulation_config(config)?;
    let mut deltas = piston_deltas.map_or([0.0; 3], |delta| [delta.x, delta.y, delta.z]);
    push_local_player_by_pistons(
        world,
        &mut simulation_state,
        &nearby_entities,
        &moving_pistons,
        &mut deltas,
        config,
        minecraft_version,
    );
    Ok(NativePistonPushResult {
        state: simulation_state.into(),
        piston_deltas: NativeVec3 {
            x: deltas[0],
            y: deltas[1],
            z: deltas[2],
        },
    })
}

#[napi]
pub fn simulate_versioned_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeSimulationState>,
    ticks: u32,
    config: NativeSimulationConfig,
) -> Result<Vec<NativeSimulationState>> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    let mut simulation_states: Vec<SimulationState> = states.into_iter().map(Into::into).collect();
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let config = validated_simulation_config(config)?;
    for _ in 0..ticks {
        for state in &mut simulation_states {
            state.minor_horizontal_collision_supported =
                minecraft_version.supports_minor_horizontal_collision();
            let result = versioned::dispatch(&version, state, world.snapshot(), config)
                .map_err(Error::from_reason)?;
            state.collided_horizontally = result.collided_horizontally;
            state.collided_vertically = result.collided_vertically;
        }
    }
    Ok(simulation_states.into_iter().map(Into::into).collect())
}

#[napi]
pub fn simulate_versioned_entity_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeVersionedEntityState>,
    ticks: u32,
    config: NativeSimulationConfig,
) -> Result<Vec<NativeVersionedEntityState>> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    let pushthrough_enabled =
        versioned::entity_pushthrough_enabled(&version).map_err(Error::from_reason)?;
    let mut entities: Vec<(SimulationState, bool, Option<u32>, bool, f64)> = states
        .into_iter()
        .map(
            |NativeVersionedEntityState {
                 simulation,
                 no_physics,
                 vehicle_id,
                 pushable,
                 pushthrough,
             }| {
                (
                    simulation.into(),
                    no_physics,
                    vehicle_id,
                    pushable,
                    pushthrough,
                )
            },
        )
        .collect();
    for (_, _, _, _, pushthrough) in &entities {
        if !pushthrough.is_finite() || !(0.0..=1.0).contains(pushthrough) {
            return Err(Error::from_reason(
                "entity push state contains invalid values",
            ));
        }
    }
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let config = validated_simulation_config(config)?;

    for _ in 0..ticks {
        for (simulation, _, _, _, _) in &mut entities {
            simulation.minor_horizontal_collision_supported =
                minecraft_version.supports_minor_horizontal_collision();
            let result = versioned::dispatch(&version, simulation, world.snapshot(), config)
                .map_err(Error::from_reason)?;
            simulation.collided_horizontally = result.collided_horizontally;
            simulation.collided_vertically = result.collided_vertically;
        }

        for left_index in 0..entities.len() {
            for right_index in (left_index + 1)..entities.len() {
                let (left, right) = {
                    let (before_right, after_right) = entities.split_at_mut(right_index);
                    (&mut before_right[left_index], &mut after_right[0])
                };
                let left_bounds = left.0.bounds();
                let right_bounds = right.0.bounds();
                if left_bounds.max.x <= right_bounds.min.x
                    || right_bounds.max.x <= left_bounds.min.x
                    || left_bounds.max.y <= right_bounds.min.y
                    || right_bounds.max.y <= left_bounds.min.y
                    || left_bounds.max.z <= right_bounds.min.z
                    || right_bounds.max.z <= left_bounds.min.z
                {
                    continue;
                }
                let (left_impulse, right_impulse) = entity_push_impulses_with_pushthrough(
                    left.0.position,
                    right.0.position,
                    left.1,
                    right.1,
                    left.2.is_some() && left.2 == right.2,
                    left.2.is_some(),
                    right.2.is_some(),
                    left.3,
                    right.3,
                    if pushthrough_enabled { left.4 } else { 0.0 },
                    if pushthrough_enabled { right.4 } else { 0.0 },
                );
                left.0.velocity = left.0.velocity + left_impulse;
                right.0.velocity = right.0.velocity + right_impulse;
            }
        }
    }

    Ok(entities
        .into_iter()
        .map(
            |(simulation, no_physics, vehicle_id, pushable, pushthrough)| {
                NativeVersionedEntityState {
                    simulation: simulation.into(),
                    no_physics,
                    vehicle_id,
                    pushable,
                    pushthrough,
                }
            },
        )
        .collect())
}

#[napi]
pub fn simulate_versioned_entity_push(
    version: String,
    left: NativeEntityPushState,
    right: NativeEntityPushState,
    passenger_of_same_vehicle: bool,
) -> Result<NativeEntityPushResult> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    let pushthrough_enabled =
        versioned::entity_pushthrough_enabled(&version).map_err(Error::from_reason)?;
    let left_position: Vec3 = left.position.into();
    let right_position: Vec3 = right.position.into();
    let left_velocity: Vec3 = left.velocity.into();
    let right_velocity: Vec3 = right.velocity.into();
    if !left_position.is_finite()
        || !right_position.is_finite()
        || !left_velocity.is_finite()
        || !right_velocity.is_finite()
        || !left.pushthrough.is_finite()
        || !(0.0..=1.0).contains(&left.pushthrough)
        || !right.pushthrough.is_finite()
        || !(0.0..=1.0).contains(&right.pushthrough)
    {
        return Err(Error::from_reason(
            "entity push state contains invalid values",
        ));
    }
    let (left_impulse, right_impulse) = entity_push_impulses_with_pushthrough(
        left_position,
        right_position,
        left.no_physics,
        right.no_physics,
        passenger_of_same_vehicle,
        left.vehicle,
        right.vehicle,
        left.pushable,
        right.pushable,
        if pushthrough_enabled {
            left.pushthrough
        } else {
            0.0
        },
        if pushthrough_enabled {
            right.pushthrough
        } else {
            0.0
        },
    );
    Ok(NativeEntityPushResult {
        left_velocity: (left_velocity + left_impulse).into(),
        right_velocity: (right_velocity + right_impulse).into(),
    })
}

#[napi]
pub fn refresh_versioned_pose(
    version: String,
    world: &NativeWorld,
    state: NativeSimulationState,
    current_pose: u8,
    requested_pose: u8,
    spectator: bool,
) -> Result<NativePoseRefreshResult> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    let _current_dimensions = dimensions_for_pose(0.3, 1.8, Some(current_pose), Some(&version))?;
    let requested_dimensions = dimensions_for_pose(0.3, 1.8, Some(requested_pose), Some(&version))?;
    let simulation_state: SimulationState = state.into();
    let candidate_is_free = |dimensions: Dimensions| {
        if spectator || simulation_state.no_physics {
            return true;
        }
        let bounds = Aabb::new(
            Vec3::new(
                simulation_state.position.x - dimensions.half_width,
                simulation_state.position.y,
                simulation_state.position.z - dimensions.half_width,
            ),
            Vec3::new(
                simulation_state.position.x + dimensions.half_width,
                simulation_state.position.y + dimensions.height,
                simulation_state.position.z + dimensions.half_width,
            ),
        )
        .deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !world
            .snapshot()
            .collision_shapes(bounds)
            .iter()
            .any(|shape| shape.intersects(bounds))
    };
    // Player.updatePlayerPose only changes the pose when the swimming
    // bounding box is available. A passenger or spectator may then select
    // the desired pose without a fit check.
    let swimming_is_free =
        candidate_is_free(dimensions_for_pose(0.3, 1.8, Some(1), Some(&version))?);
    let selected_pose = if !swimming_is_free {
        current_pose
    } else if spectator || simulation_state.passenger || candidate_is_free(requested_dimensions) {
        requested_pose
    } else if candidate_is_free(dimensions_for_pose(0.3, 1.8, Some(5), Some(&version))?) {
        5
    } else {
        1
    };
    let selected_dimensions = dimensions_for_pose(0.3, 1.8, Some(selected_pose), Some(&version))?;
    let mut output_state = simulation_state;
    output_state.half_width = selected_dimensions.half_width;
    output_state.height = selected_dimensions.height;
    output_state.eye_height = Some(pose_eye_height(selected_pose));
    Ok(NativePoseRefreshResult {
        state: output_state.into(),
        pose: selected_pose,
        changed: selected_pose != current_pose,
    })
}

#[napi]
pub fn create_simulation_state(
    descriptor: NativeEntityDescriptor,
) -> Result<NativeSimulationState> {
    if !descriptor.position.x.is_finite()
        || !descriptor.position.y.is_finite()
        || !descriptor.position.z.is_finite()
        || !descriptor.velocity.x.is_finite()
        || !descriptor.velocity.y.is_finite()
        || !descriptor.velocity.z.is_finite()
        || !descriptor.half_width.is_finite()
        || !descriptor.height.is_finite()
        || descriptor.half_width <= 0.0
        || descriptor.height <= 0.0
        || !descriptor.yaw.is_finite()
    {
        return Err(Error::from_reason(
            "entity descriptor contains invalid values",
        ));
    }
    let entity = EntityState {
        position: descriptor.position.into(),
        velocity: descriptor.velocity.into(),
        dimensions: dimensions_for_pose(
            descriptor.half_width,
            descriptor.height,
            descriptor.pose,
            descriptor.version.as_deref(),
        )?,
        flags: EntityFlags {
            on_ground: descriptor.on_ground,
            in_water: descriptor.in_water,
            in_lava: descriptor.in_lava,
            ..EntityFlags::default()
        },
    };
    let mut state = SimulationState::from(entity);
    state.yaw = descriptor.yaw;
    state.input = descriptor.input.map(Into::into).unwrap_or(Vec3::ZERO);
    state.jump_requested = descriptor.jump_requested.unwrap_or(false);
    state.sprinting = descriptor.sprinting.unwrap_or(false);
    state.sneaking = descriptor.sneaking.unwrap_or(false);
    state.swimming = descriptor.swimming.unwrap_or(false);
    state.passenger = descriptor.passenger.unwrap_or(false);
    state.climbing = descriptor.climbing.unwrap_or(false);
    state.no_gravity = descriptor.no_gravity.unwrap_or(false);
    state.fall_flying = descriptor.fall_flying.unwrap_or(false);
    state.fall_damage_immune = descriptor.fall_damage_immune.unwrap_or(false);
    state.fall_damage_multiplier = Some(descriptor.fall_damage_multiplier.unwrap_or(1.0));
    state.slow_falling = descriptor.slow_falling.unwrap_or(false);
    state.levitation = descriptor.levitation.unwrap_or(false);
    state.levitation_amplifier = descriptor.levitation_amplifier;
    state.movement_speed = descriptor.movement_speed;
    state.jump_strength = descriptor.jump_strength;
    state.speed_effect_amplifier = descriptor.speed_effect_amplifier;
    state.slowness_effect_amplifier = descriptor.slowness_effect_amplifier;
    state.jump_boost_amplifier = descriptor.jump_boost_amplifier;
    state.dolphins_grace = descriptor.dolphins_grace.unwrap_or(0);
    state.depth_strider = descriptor.depth_strider.unwrap_or(0);
    state.soul_speed = descriptor.soul_speed.unwrap_or(0);
    state.water_movement_efficiency = descriptor.water_movement_efficiency.unwrap_or(0.0);
    state.pitch = descriptor.pitch.unwrap_or(0.0);
    state.look = descriptor.look.map(Into::into).unwrap_or_else(|| {
        let pitch = descriptor.pitch.unwrap_or(0.0);
        let horizontal = trig::cos(pitch);
        Vec3::new(
            -trig::sin(descriptor.yaw) * horizontal,
            -trig::sin(pitch),
            trig::cos(descriptor.yaw) * horizontal,
        )
    });
    state.firework_boost = descriptor
        .firework_boost
        .map(Into::into)
        .unwrap_or(Vec3::ZERO);
    if !valid_state(state) {
        return Err(Error::from_reason(
            "entity descriptor contains invalid values",
        ));
    }

    Ok(state.into())
}

fn dimensions_for_pose(
    half_width: f64,
    height: f64,
    pose: Option<u8>,
    version: Option<&str>,
) -> Result<Dimensions> {
    match pose {
        None => Ok(Dimensions::new(half_width, height)),
        Some(0) => Ok(Dimensions::new(0.3, 1.8)),
        Some(1) | Some(3) | Some(4) => Ok(Dimensions::new(0.3, 0.6)),
        Some(2) => Ok(Dimensions::new(0.1, 0.2)),
        Some(5) => Ok(Dimensions::new(0.3, 1.5)),
        Some(6) if is_pre_long_jumping_version(version) => Ok(Dimensions::new(0.1, 0.2)),
        Some(6) if has_long_jumping_pose(version) => Ok(Dimensions::new(0.3, 1.8)),
        Some(6) => Err(Error::from_reason(
            "long-jumping pose is unavailable for this version",
        )),
        Some(7) if has_long_jumping_pose(version) => Ok(Dimensions::new(0.1, 0.2)),
        Some(7) => Err(Error::from_reason(
            "dying pose id 7 is unavailable for this version",
        )),
        Some(_) => Err(Error::from_reason("entity pose must be between 0 and 7")),
    }

}

pub(crate) fn validated_simulation_config(config: NativeSimulationConfig) -> Result<PhysicsConfig> {
    let config: PhysicsConfig = config.into();
    config.validate().map_err(Error::from_reason)?;
    Ok(config)
}

fn is_pre_long_jumping_version(version: Option<&str>) -> bool {
    matches!(version, Some("1.14.4" | "1.15.2" | "1.16.5"))
}

fn has_long_jumping_pose(version: Option<&str>) -> bool {
    version.is_some_and(|value| !is_pre_long_jumping_version(Some(value)))
}

fn pose_eye_height(pose: u8) -> f64 {
    match pose {
        1 | 3 | 4 => 0.4,
        5 => 1.27,
        2 | 7 => 1.62,
        _ => 1.62,
    }
}

#[napi]
pub fn version_capabilities(major: u16, minor: u16, patch: u16) -> NativeVersionCapabilities {
    let capabilities = MinecraftVersion::new(major, minor, patch).capabilities();
    NativeVersionCapabilities {
        fluid_physics: capabilities.fluid_physics,
        honey_blocks: capabilities.honey_blocks,
        fall_flying: capabilities.fall_flying,
        water_movement_efficiency: capabilities.water_movement_efficiency,
        minor_horizontal_collision: capabilities.minor_horizontal_collision,
    }
}

#[napi]
pub fn generated_block_has_tag(version: String, block: String, tag: String) -> bool {
    crate::generated_data::block_has_tag(&version, &block, &tag)
}

#[napi]
pub fn generated_state_index(
    version: String,
    block: String,
    properties: Vec<(String, String)>,
) -> Option<u32> {
    let values = properties
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    crate::generated_data::state_index(&version, &block, &values)
        .and_then(|index| u32::try_from(index).ok())
}

#[napi]
pub fn generated_block_state_count(version: String, block: String) -> Option<u32> {
    crate::generated_data::block(&version, &block)
        .and_then(|definition| definition.state_count)
        .and_then(|count| u32::try_from(count).ok())
}

#[napi]
pub fn generated_fluid_state_count(version: String, fluid: String) -> Option<u32> {
    crate::generated_data::fluid(&version, &fluid)
        .and_then(|definition| u32::try_from(definition.state_count).ok())
}

#[napi]
pub fn generated_fluid_state_index(
    version: String,
    fluid: String,
    properties: Vec<(String, String)>,
) -> Option<u32> {
    let values = properties
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    crate::generated_data::fluid_state_index(&version, &fluid, &values)
        .and_then(|index| u32::try_from(index).ok())
}

#[napi]
pub fn generated_block_defaults(
    version: String,
    block: String,
) -> Option<Vec<NativeBlockProperty>> {
    let definition = crate::generated_data::block(&version, &block)?;
    if definition
        .properties
        .iter()
        .any(|property| property.defaults.is_empty())
    {
        return if definition.properties.is_empty() {
            Some(Vec::new())
        } else {
            None
        };
    }

    Some(
        definition
            .properties
            .iter()
            .flat_map(|property| {
                property.defaults.iter().map(|value| NativeBlockProperty {
                    name: property.name.to_owned(),
                    value: (*value).to_owned(),
                })
            })
            .collect(),
    )
}

#[napi]
pub fn generated_source_shape_boxes(
    version: String,
    block: String,
) -> Option<Vec<NativeGeneratedShapeBox>> {
    crate::generated_data::source_shape_boxes(&version, &block).map(|boxes| {
        boxes
            .iter()
            .map(|shape| NativeGeneratedShapeBox {
                min_x: shape.min[0],
                min_y: shape.min[1],
                min_z: shape.min[2],
                max_x: shape.max[0],
                max_y: shape.max[1],
                max_z: shape.max[2],
            })
            .collect()
    })
}

#[napi]
pub fn generated_method_shape_boxes(
    version: String,
    block: String,
    kind: String,
) -> Option<Vec<NativeGeneratedShapeBox>> {
    crate::generated_data::method_shape_boxes(&version, &block, &kind).map(|boxes| {
        boxes
            .iter()
            .map(|shape| NativeGeneratedShapeBox {
                min_x: shape.min[0],
                min_y: shape.min[1],
                min_z: shape.min[2],
                max_x: shape.max[0],
                max_y: shape.max[1],
                max_z: shape.max[2],
            })
            .collect()
    })
}

#[napi]
pub fn generated_exact_method_shape_boxes(
    version: String,
    block: String,
    kind: String,
) -> Option<Vec<NativeGeneratedShapeBox>> {
    crate::generated_data::exact_method_shape_boxes(&version, &block, &kind).map(|boxes| {
        boxes
            .iter()
            .map(|shape| NativeGeneratedShapeBox {
                min_x: shape.min[0],
                min_y: shape.min[1],
                min_z: shape.min[2],
                max_x: shape.max[0],
                max_y: shape.max[1],
                max_z: shape.max[2],
            })
            .collect()
    })
}

#[napi]
pub fn generated_exact_method_shape_boxes_for_state(
    version: String,
    block: String,
    kind: String,
    properties: Vec<NativeBlockProperty>,
) -> Option<Vec<NativeGeneratedShapeBox>> {
    let values = properties
        .iter()
        .map(|property| (property.name.as_str(), property.value.as_str()))
        .collect::<Vec<_>>();
    crate::generated_data::state_index(&version, &block, &values)?;
    crate::generated_data::exact_method_shape_boxes(&version, &block, &kind).map(|boxes| {
        boxes
            .iter()
            .map(|shape| NativeGeneratedShapeBox {
                min_x: shape.min[0],
                min_y: shape.min[1],
                min_z: shape.min[2],
                max_x: shape.max[0],
                max_y: shape.max[1],
                max_z: shape.max[2],
            })
            .collect()
    })
}

#[napi]
pub fn generated_shape_property_dependencies(
    version: String,
    block: String,
) -> Option<Vec<String>> {
    crate::generated_data::shape_property_dependencies(&version, &block)
        .map(|properties| properties.iter().map(|property| (*property).to_owned()).collect())
}

#[napi]
pub fn generated_method_shape_property_dependencies(
    version: String,
    block: String,
    kind: String,
) -> Option<Vec<String>> {
    crate::generated_data::method_shape_property_dependencies(&version, &block, &kind)
        .map(|properties| properties.iter().map(|property| (*property).to_owned()).collect())
}

#[napi]
pub fn generated_method_shape_predicates(
    version: String,
    block: String,
    kind: String,
) -> Option<Vec<NativeGeneratedShapePredicate>> {
    crate::generated_data::method_shape_predicates(&version, &block, &kind).map(|predicates| {
        predicates
            .iter()
            .map(|predicate| NativeGeneratedShapePredicate {
                property: predicate.property.to_owned(),
                operator: predicate.operator.to_owned(),
                value: predicate.value.to_owned(),
            })
            .collect()
    })
}

#[napi]
pub fn generated_method_shape_predicates_match_state(
    version: String,
    block: String,
    kind: String,
    properties: Vec<NativeBlockProperty>,
) -> Option<bool> {
    let values = properties
        .iter()
        .map(|property| (property.name.as_str(), property.value.as_str()))
        .collect::<Vec<_>>();
    crate::generated_data::method_shape_predicates_match_state(&version, &block, &kind, &values)
}

#[napi]
pub fn generated_method_shape_is_state_dependent(
    version: String,
    block: String,
    kind: String,
) -> Option<bool> {
    crate::generated_data::method_shape_is_state_dependent(&version, &block, &kind)
}

#[napi]
pub fn generated_method_shape_provenance(
    version: String,
    block: String,
    kind: String,
) -> Option<NativeGeneratedShapeProvenance> {
    crate::generated_data::method_shape_provenance(&version, &block, &kind).map(|provenance| {
        NativeGeneratedShapeProvenance {
            source_sha256: provenance.source_sha256.map(str::to_owned),
            has_method: provenance.has_method,
            state_dependent: provenance.state_dependent,
            property_dependencies: provenance
                .property_dependencies
                .iter()
                .map(|property| (*property).to_owned())
                .collect(),
            candidate_box_count: provenance.candidate_box_count as i64,
        }
    })
}

#[napi]
pub fn simulate_boat_tick(state: NativeBoatState) -> Result<NativeBoatState> {
    let mut state = BoatSimulationState {
        position: state.position.into(),
        velocity: state.velocity.into(),
        yaw: state.yaw,
        forward: state.forward,
        left: state.left,
        right: state.right,
        input_down: state.input_down.unwrap_or(false),
        delta_rotation: state.delta_rotation.unwrap_or(0.0),
        in_water: state.in_water,
        water_level: state.water_level.unwrap_or(f64::NEG_INFINITY),
        status: BoatStatus::default(),
        previous_status: BoatStatus::default(),
        bubble_time: state.bubble_time.unwrap_or(0),
        bubble_column_direction_is_down: state
            .bubble_column_direction_is_down
            .unwrap_or(false),
        above_bubble_column: state.above_bubble_column.unwrap_or(false),
        player_passenger: state.player_passenger.unwrap_or(false),
        collided_horizontally: state.collided_horizontally.unwrap_or(false),
        collided_vertically: state.collided_vertically.unwrap_or(false),
        on_ground: state.on_ground.unwrap_or(false),
        ..BoatSimulationState::default()
    };
    if !valid_boat_state(state) {
        return Err(Error::from_reason(
            "boat simulation state contains invalid values",
        ));
    }
    tick_boat(&mut state);
    Ok(NativeBoatState {
        position: state.position.into(),
        velocity: state.velocity.into(),
        yaw: state.yaw,
        forward: state.forward,
        left: state.left,
        right: state.right,
        input_down: Some(state.input_down),
        delta_rotation: Some(state.delta_rotation),
        in_water: state.in_water,
        water_level: Some(state.water_level),
        status: Some(if state.in_water {
            "in_water".to_string()
        } else {
            "in_air".to_string()
        }),
        previous_status: Some("in_air".to_string()),
        bubble_time: Some(state.bubble_time),
        bubble_column_direction_is_down: Some(state.bubble_column_direction_is_down),
        above_bubble_column: Some(state.above_bubble_column),
        player_passenger: Some(state.player_passenger),
        collided_horizontally: Some(state.collided_horizontally),
        collided_vertically: Some(state.collided_vertically),
        on_ground: Some(state.on_ground),
        yaw_degrees: None,
        land_friction: None,
        last_vertical_velocity: None,
        bounding_box: None,
        speed_multiplier: None,
        rider_yaw: None,
        rider_forward: None,
        rider_strafe: None,
    })
}

fn vehicle_release(version: &str, kind: &str) -> Result<MinecraftVersion> {
    if !versioned::is_supported(version) {
        return Err(Error::from_reason(format!(
            "unsupported Minecraft version for {kind} simulation: {version}"
        )));
    }
    MinecraftVersion::parse(version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))
}

#[napi]
pub fn simulate_versioned_boat_tick(
    version: String,
    world: &NativeWorld,
    state: NativeBoatState,
) -> Result<NativeBoatState> {
    let release = vehicle_release(&version, "boat")?;
    let mut state = boat_state(state)?;
    tick_boat_in_world(&mut state, world.snapshot(), release);
    Ok(native_boat_state(state))
}

#[napi]
pub fn simulate_versioned_boat_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeBoatState>,
    ticks: u32,
) -> Result<Vec<NativeBoatState>> {
    let release = vehicle_release(&version, "boat")?;
    let snapshot = world.snapshot();
    let mut states: Vec<BoatSimulationState> = states
        .into_iter()
        .map(boat_state)
        .collect::<Result<Vec<_>>>()?;
    for _ in 0..ticks {
        for state in &mut states {
            tick_boat_in_world(state, snapshot, release);
        }
    }
    Ok(states.into_iter().map(native_boat_state).collect())
}

#[napi]
pub fn simulate_horse_tick(state: NativeHorseState) -> Result<NativeHorseState> {
    let mut state = horse_state(state)?;
    tick_horse(&mut state);
    Ok(native_horse_state(state))
}

/// The tick of a mob a player rides and steers (`entityType`: a horse when unnamed).
#[napi]
pub fn simulate_versioned_horse_tick(
    version: String,
    world: &NativeWorld,
    state: NativeHorseState,
) -> Result<NativeHorseState> {
    let release = vehicle_release(&version, "ridden mob")?;
    let mut mob = ridden_mob_state(state)?;
    tick_ridden_mob(&mut mob, world.snapshot(), release);
    Ok(native_ridden_mob_state(mob))
}

#[napi]
pub fn simulate_versioned_horse_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeHorseState>,
    ticks: u32,
) -> Result<Vec<NativeHorseState>> {
    let release = vehicle_release(&version, "ridden mob")?;
    let snapshot = world.snapshot();
    let mut states: Vec<RiddenMobState> = states
        .into_iter()
        .map(ridden_mob_state)
        .collect::<Result<Vec<_>>>()?;
    for _ in 0..ticks {
        for state in &mut states {
            tick_ridden_mob(state, snapshot, release);
        }
    }
    Ok(states.into_iter().map(native_ridden_mob_state).collect())
}

#[napi]
pub fn simulate_projectile_tick(
    world: &NativeWorld,
    state: NativeProjectileState,
) -> Result<NativeProjectileTickResult> {
    let mut state = projectile_state(state)?;
    let result = tick_projectile(&mut state, world.snapshot());
    Ok(native_projectile_tick_result(result, &state))
}

#[napi]
pub fn simulate_versioned_projectile_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeProjectileState>,
    ticks: u32,
) -> Result<Vec<NativeProjectileTickResult>> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported Minecraft version for projectile simulation: {version}"
        )));
    }
    let mut native_states = states
        .into_iter()
        .map(projectile_state)
        .collect::<Result<Vec<_>>>()?;
    let mut results = Vec::with_capacity(native_states.len());
    let snapshot = world.snapshot();
    for _ in 0..ticks {
        results.clear();
        for state in &mut native_states {
            let result = tick_projectile_with_bubble_column(
                state,
                snapshot,
                version.starts_with("26.1") || version == "26.2" || version == "26.3",
            );
            results.push(native_projectile_tick_result(result, state));
        }
    }
    Ok(results)
}

#[napi]
pub fn simulate_arrow_tick(
    world: &NativeWorld,
    state: NativeArrowState,
) -> Result<NativeArrowTickResult> {
    let mut state = arrow_state(state)?;
    let result = tick_arrow(&mut state, world.snapshot());
    Ok(native_arrow_tick_result(result))
}

#[napi]
pub fn simulate_versioned_arrow_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeArrowState>,
    ticks: u32,
) -> Result<Vec<NativeArrowTickResult>> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported Minecraft version for arrow simulation: {version}"
        )));
    }
    let mut native_states = states
        .into_iter()
        .map(arrow_state)
        .collect::<Result<Vec<_>>>()?;
    let mut results = Vec::with_capacity(native_states.len());
    let snapshot = world.snapshot();
    for _ in 0..ticks {
        results.clear();
        for state in &mut native_states {
            let result = tick_arrow(state, snapshot);
            results.push(native_arrow_tick_result(result));
        }
    }
    Ok(results)
}

#[napi]
pub fn simulate_trident_tick(
    world: &NativeWorld,
    state: NativeTridentState,
) -> Result<NativeTridentTickResult> {
    let mut state = trident_state(state)?;
    let result = tick_trident(&mut state, world.snapshot());
    Ok(native_trident_tick_result(result))
}

#[napi]
pub fn simulate_versioned_trident_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeTridentState>,
    ticks: u32,
) -> Result<Vec<NativeTridentTickResult>> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported Minecraft version for trident simulation: {version}"
        )));
    }
    let mut native_states = states
        .into_iter()
        .map(trident_state)
        .collect::<Result<Vec<_>>>()?;
    let mut results = Vec::with_capacity(native_states.len());
    let snapshot = world.snapshot();
    for _ in 0..ticks {
        results.clear();
        for state in &mut native_states {
            let result = tick_trident(state, snapshot);
            results.push(native_trident_tick_result(result));
        }
    }
    Ok(results)
}

#[napi]
pub fn simulate_ticks(
    world: &NativeWorld,
    states: Vec<NativeSimulationState>,
    ticks: u32,
    config: NativeSimulationConfig,
) -> Result<Vec<NativeSimulationState>> {
    let _ = (world, states, ticks, config);
    Err(Error::from_reason(
        "a concrete Minecraft version is required; use versioned tick calls",
    ))
}

fn projectile_state(value: NativeProjectileState) -> Result<ProjectileState> {
    let kind = match value.kind.as_deref() {
        Some(name) => ProjectileKind::from_name_checked(name).ok_or_else(|| {
            Error::from_reason(format!("unsupported projectile family: {name}"))
        })?,
        None => ProjectileKind::Generic,
    };
    let defaults = ProjectileState::for_kind(kind, Vec3::ZERO, Vec3::ZERO);
    let state = ProjectileState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        gravity: value.gravity.unwrap_or(defaults.gravity),
        air_drag: value.air_drag.unwrap_or(defaults.air_drag),
        power: value.power.map(Into::into).unwrap_or(Vec3::ZERO),
        in_water: value.in_water.unwrap_or(false),
        first_tick: value.first_tick.unwrap_or(true),
        kind,
        horizontal_collision: value.horizontal_collision.unwrap_or(false),
        shot_at_angle: value.shot_at_angle.unwrap_or(false),
    };
    if !state.position.is_finite()
        || !state.velocity.is_finite()
        || !state.gravity.is_finite()
        || !state.air_drag.is_finite()
        || !state.power.is_finite()
    {
        return Err(Error::from_reason(
            "projectile simulation state contains invalid values",
        ));
    }
    Ok(state)
}

fn native_projectile_tick_result(
    result: crate::projectile::ProjectileTickResult,
    state: &ProjectileState,
) -> NativeProjectileTickResult {
    NativeProjectileTickResult {
        position: result.position.into(),
        velocity: result.velocity.into(),
        block_hit: result.block_hit,
        first_tick: state.first_tick,
    }
}

fn arrow_state(value: NativeArrowState) -> Result<ArrowState> {
    let defaults = ArrowState::new(Vec3::ZERO, Vec3::ZERO);
    let state = ArrowState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        gravity: value.gravity.unwrap_or(defaults.gravity),
        air_drag: value.air_drag.unwrap_or(defaults.air_drag),
        water_inertia: value.water_inertia.unwrap_or(defaults.water_inertia),
        in_water: value.in_water.unwrap_or(false),
        in_ground: value.in_ground.unwrap_or(false),
        life: value.life.unwrap_or(0),
        in_ground_time: value.in_ground_time.unwrap_or(0),
        shake_time: value.shake_time.unwrap_or(0),
        fall_impulse: value.fall_impulse.map(Into::into).unwrap_or(Vec3::ZERO),
        last_block_id: value.last_block_id,
    };
    if !state.position.is_finite()
        || !state.velocity.is_finite()
        || !state.gravity.is_finite()
        || !state.air_drag.is_finite()
        || !state.water_inertia.is_finite()
    {
        return Err(Error::from_reason(
            "arrow simulation state contains invalid values",
        ));
    }
    Ok(state)
}

fn native_arrow_tick_result(
    result: crate::projectile::ArrowTickResult,
) -> NativeArrowTickResult {
    NativeArrowTickResult {
        position: result.position.into(),
        velocity: result.velocity.into(),
        block_hit: result.block_hit,
        in_ground: result.in_ground,
        discarded: result.discarded,
        in_ground_time: result.in_ground_time,
        shake_time: result.shake_time,
        started_falling: result.started_falling,
    }
}

fn trident_state(value: NativeTridentState) -> Result<TridentState> {
    let defaults = TridentState::new(Vec3::ZERO, Vec3::ZERO);
    let state = TridentState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        gravity: value.gravity.unwrap_or(defaults.gravity),
        air_drag: value.air_drag.unwrap_or(defaults.air_drag),
        water_inertia: value.water_inertia.unwrap_or(defaults.water_inertia),
        in_water: value.in_water.unwrap_or(false),
        dealt_damage: value.dealt_damage.unwrap_or(false),
    };
    if !state.position.is_finite()
        || !state.velocity.is_finite()
        || !state.gravity.is_finite()
        || !state.air_drag.is_finite()
        || !state.water_inertia.is_finite()
    {
        return Err(Error::from_reason(
            "trident simulation state contains invalid values",
        ));
    }
    Ok(state)
}

fn native_trident_tick_result(
    result: crate::projectile::TridentTickResult,
) -> NativeTridentTickResult {
    NativeTridentTickResult {
        position: result.position.into(),
        velocity: result.velocity.into(),
        block_hit: result.block_hit,
    }
}

fn valid_state(state: SimulationState) -> bool {
    state.position.x.is_finite()
        && state.position.y.is_finite()
        && state.position.z.is_finite()
        && state.velocity.x.is_finite()
        && state.velocity.y.is_finite()
        && state.velocity.z.is_finite()
        && state.yaw.is_finite()
        && state.input.x.is_finite()
        && state.input.y.is_finite()
        && state.input.z.is_finite()
        && state.half_width.is_finite()
        && state.height.is_finite()
        && state.half_width > 0.0
        && state.height > 0.0
        && state
            .gravity
            .map_or(true, |value| value.is_finite() && value >= 0.0)
        && state.step_height.map_or(true, |value| {
            value.is_finite() && (0.0..=10.0).contains(&value)
        })
        && state.movement_speed.map_or(true, f64::is_finite)
        && state.jump_strength.map_or(true, f64::is_finite)
        && state.flying_speed.is_finite()
        && state.firework_boost.is_finite()
}

#[cfg(test)]
mod tests {
    use super::{
        create_simulation_state, refresh_versioned_pose, simulate_versioned_entity_push,
        simulate_versioned_entity_ticks, simulate_versioned_piston_push, simulate_versioned_tick,
        simulate_versioned_ticks, NativeAabb, NativeBlockShape, NativeBlockUpdate,
        NativeEntityDescriptor, NativeEntityPushState, NativeMovingPiston, NativeNearbyEntity,
        NativeSimulationConfig, NativeSimulationState, NativeVec3, NativeVersionedEntityState,
        NativeWorld,
    };

    fn block_shape(min: (f64, f64, f64), max: (f64, f64, f64), effect: u8) -> NativeBlockShape {
        NativeBlockShape {
            min_x: min.0,
            min_y: min.1,
            min_z: min.2,
            max_x: max.0,
            max_y: max.1,
            max_z: max.2,
            slipperiness: 0.6,
            effect: Some(effect),
            bounce_restitution: None,
            suppresses_bounce: None,
            fall_damage_resetting: None,
        }
    }

    fn shape_update(position: (i32, i32, i32), shape: NativeBlockShape) -> NativeBlockUpdate {
        NativeBlockUpdate {
            x: position.0,
            y: position.1,
            z: position.2,
            id: Some(1),
            state_version: None,
            block_entity: None,
            remove_block_entity: None,
            properties: None,
            tags: None,
            shape: Some(shape),
            outline_shape: None,
            interaction_shape: None,
            fluid: None,
            fluid_tags: None,
            fluid_level: None,
            fluid_source: None,
            fluid_flow: None,
            fluid_falling: None,
            fall_damage_resetting: None,
            bounce_restitution: None,
            suppresses_bounce: None,
            suffocating: None,
            remove: None,
        }
    }

    fn vec3(x: f64, y: f64, z: f64) -> NativeVec3 {
        NativeVec3 { x, y, z }
    }

    fn default_config() -> NativeSimulationConfig {
        NativeSimulationConfig {
            gravity: None,
            drag: None,
            movement_speed: None,
            sprint_multiplier: None,
            jump_velocity: None,
            step_height: None,
            negligible_velocity: None,
        }
    }

    fn pose_state() -> NativeSimulationState {
        create_simulation_state(NativeEntityDescriptor {
            position: NativeVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            velocity: NativeVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            yaw: 0.0,
            in_water: false,
            in_lava: false,
            version: Some("1.21.1".to_string()),
            pose: Some(0),
            pitch: None,
            input: None,
            jump_requested: None,
            jump_ticks: None,
            sprinting: None,
            sneaking: None,
            using_item: None,
            swimming: None,
            passenger: None,
            flying: None,
            flying_speed: None,
            climbing: None,
            no_physics: None,
            no_gravity: None,
            fall_flying: None,
            fall_damage_immune: None,
            fall_damage_multiplier: None,
            slow_falling: None,
            levitation: None,
            levitation_amplifier: None,
            movement_speed: None,
            jump_strength: None,
            speed_effect_amplifier: None,
            slowness_effect_amplifier: None,
            jump_boost_amplifier: None,
            dolphins_grace: None,
            depth_strider: None,
            soul_speed: None,
            water_movement_efficiency: None,
            movement_efficiency: None,
            air_drag_modifier: None,
            friction_modifier: None,
            sneaking_speed: None,
            look: None,
            firework_boost: None,
        })
        .expect("pose fixture should be valid")
    }

    fn push_state(x: f64, velocity_x: f64, pushthrough: f64) -> NativeEntityPushState {
        NativeEntityPushState {
            position: NativeVec3 { x, y: 0.0, z: 0.0 },
            velocity: NativeVec3 {
                x: velocity_x,
                y: 0.0,
                z: 0.0,
            },
            no_physics: false,
            vehicle: false,
            pushable: true,
            pushthrough,
        }
    }

    #[test]
    fn versioned_tick_accepts_every_release_name() {
        for release in crate::generated_data::RELEASES {
            let result = simulate_versioned_tick(
                release.version.to_string(),
                &mut NativeWorld::new(),
                pose_state(),
                default_config(),
            );
            assert!(result.is_ok(), "{} should be accepted", release.version);
        }
    }

    #[test]
    fn ticking_the_output_again_keeps_the_default_drag() {
        let mut world = NativeWorld::new();
        let mut state = pose_state();
        state.velocity = vec3(1.0, 0.0, 0.0);
        state.on_ground = false;

        let once = simulate_versioned_tick("26.2".to_string(), &mut world, state, default_config())
            .expect("26.2 is supported");
        let moved_once = once.velocity.x;
        assert_eq!(once.air_drag_modifier, None);
        assert_eq!(once.friction_modifier, None);
        let twice = simulate_versioned_tick("26.2".to_string(), &mut world, once, default_config())
            .expect("26.2 is supported");

        assert_eq!(twice.velocity.x, moved_once * crate::math::java_float(0.91));
    }

    fn nearby_entity(
        kind: &str,
        min: NativeVec3,
        max: NativeVec3,
        before: bool,
    ) -> NativeNearbyEntity {
        NativeNearbyEntity {
            kind: kind.to_string(),
            position: vec3((min.x + max.x) / 2.0, min.y, (min.z + max.z) / 2.0),
            bounding_box: NativeAabb { min, max },
            ticks_before_player: Some(before),
            tick_position: None,
            tick_bounding_box: None,
        }
    }

    #[test]
    fn nearby_entities_push_the_player_before_or_after_its_tick() {
        let mut world = NativeWorld::new();
        let cow = |before| {
            nearby_entity(
                "living",
                vec3(0.25, 0.0, -0.45),
                vec3(1.15, 1.4, 0.45),
                before,
            )
        };
        let impulse = 0.7 / 0.7_f64.sqrt() * 1.0 * f64::from(0.05_f32);

        let mut state = pose_state();
        state.nearby_entities = Some(vec![cow(false)]);
        let after =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .expect("1.21.1 is supported");
        assert_eq!(after.position.x, 0.0);
        assert_eq!(after.velocity.x, -impulse);

        let mut state = pose_state();
        state.nearby_entities = Some(vec![cow(true)]);
        let before =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .expect("1.21.1 is supported");
        assert_eq!(before.position.x, -impulse);
    }

    #[test]
    fn attached_rockets_boost_the_player_before_or_after_its_tick() {
        let mut world = NativeWorld::new();
        let gliding = |rockets: u32, before: u32| {
            let mut state = pose_state();
            state.on_ground = false;
            state.position = vec3(0.0, 10.0, 0.0);
            state.fall_flying = Some(true);
            state.look = Some(vec3(0.0, 0.0, 1.0));
            state.firework_rockets = Some(rockets);
            state.firework_rockets_before_player = Some(before);
            state
        };
        let mut tick = |state| {
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .expect("1.21.1 is supported")
        };

        let plain = tick(gliding(0, 0));
        let after = tick(gliding(1, 0));
        assert_eq!(after.position.z, plain.position.z);
        let boosted = plain.velocity.z + (1.0 * 0.1 + (1.0 * 1.5 - plain.velocity.z) * 0.5);
        assert_eq!(after.velocity.z, boosted);
        let before = tick(gliding(1, 1));
        assert!(before.position.z > plain.position.z);
    }

    fn moving_head(facing: u8) -> NativeMovingPiston {
        NativeMovingPiston {
            x: 0,
            y: 0,
            z: 0,
            facing,
            extending: true,
            source: true,
            progress: 0.0,
            shapes: vec![NativeAabb {
                min: vec3(0.75, 0.0, 0.0),
                max: vec3(1.0, 1.0, 1.0),
            }],
        }
    }

    #[test]
    fn moving_pistons_push_the_player_after_its_tick() {
        let mut world = NativeWorld::new();
        let mut state = pose_state();
        state.moving_pistons = Some(vec![moving_head(5)]);
        let pushed =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .expect("1.21.1 is supported");
        assert_eq!(pushed.position.x, 0.5 + 0.01);
        assert_eq!(pushed.velocity.x, 0.0);

        let mut state = pose_state();
        state.moving_pistons = Some(vec![moving_head(6)]);
        assert!(
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .is_err()
        );
    }

    #[test]
    fn a_piston_push_after_the_tick_pushes_as_the_tick_does() {
        let mut world = NativeWorld::new();
        let mut state = pose_state();
        state.moving_pistons = Some(vec![moving_head(5)]);
        let in_tick =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .expect("1.21.1 is supported");

        let mut ticked = simulate_versioned_tick(
            "1.21.1".to_string(),
            &mut world,
            pose_state(),
            default_config(),
        )
        .expect("1.21.1 is supported");
        ticked.moving_pistons = Some(vec![moving_head(5)]);
        let pushed = simulate_versioned_piston_push(
            "1.21.1".to_string(),
            &mut world,
            ticked,
            default_config(),
            None,
        )
        .expect("1.21.1 is supported");
        assert_eq!(pushed.state.position.x, in_tick.position.x);
        assert_eq!(pushed.state.velocity.x, in_tick.velocity.x);
        assert_eq!(pushed.piston_deltas.x, 0.51);

        let mut again = pushed.state;
        again.moving_pistons = Some(vec![moving_head(5)]);
        let limited = simulate_versioned_piston_push(
            "1.21.1".to_string(),
            &mut world,
            again,
            default_config(),
            Some(pushed.piston_deltas),
        )
        .expect("1.21.1 is supported");
        assert_eq!(limited.state.position.x, in_tick.position.x);
    }

    #[test]
    fn colliding_entities_stop_the_move_of_their_tick_only() {
        let mut world = NativeWorld::new();
        let shulker = nearby_entity("shulker", vec3(-0.5, -1.0, -0.5), vec3(0.5, 0.0, 0.5), true);
        let mut state = pose_state();
        state.on_ground = false;
        state.velocity = vec3(0.0, -0.5, 0.0);
        state.position = vec3(0.0, 0.2, 0.0);
        state.nearby_entities = Some(vec![shulker]);
        let landed =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, state, default_config())
                .expect("1.21.1 is supported");
        assert_eq!(landed.position.y, 0.0);
        assert!(landed.on_ground);

        let falls =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, landed, default_config())
                .expect("1.21.1 is supported");
        assert!(falls.position.y < 0.0);
    }

    #[test]
    fn the_kept_bounding_box_goes_in_and_out_before_1_17() {
        let mut world = NativeWorld::new();
        let mut state = pose_state();
        state.velocity = vec3(0.2, 0.0, 0.0);
        state.bounding_box = Some(NativeAabb {
            min: vec3(-0.25, 0.0, -0.3),
            max: vec3(0.35, 1.8, 0.3),
        });

        let old =
            simulate_versioned_tick("1.16.5".to_string(), &mut world, state, default_config())
                .expect("1.16.5 is supported");
        let bounding_box = old.bounding_box.expect("1.16.5 keeps the box");
        let (min_x, max_x) = (-0.25 + 0.2, 0.35 + 0.2);
        assert_eq!((bounding_box.min.x, bounding_box.max.x), (min_x, max_x));
        assert_eq!(old.position.x, (min_x + max_x) / 2.0);

        let mut state = pose_state();
        state.bounding_box = Some(bounding_box);
        let modern =
            simulate_versioned_tick("1.17".to_string(), &mut world, state, default_config())
                .expect("1.17 is supported");
        assert!(modern.bounding_box.is_none());
    }

    #[test]
    fn block_state_ids_keep_two_part_release_names() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 0, 0, 1);
        world
            .set_block_state_id(0, 0, 0, "1.21".to_string(), 7)
            .expect("1.21 is a release");
        assert_eq!(
            world.block_at(0, 0, 0).state_version.as_deref(),
            Some("1.21")
        );
        assert!(world
            .set_block_state_id(0, 0, 0, "1.22".to_string(), 7)
            .is_err());
    }

    #[test]
    fn effect_blocks_without_collision_box_keep_their_effect() {
        let mut world = NativeWorld::new();
        world
            .apply_block_updates(vec![shape_update(
                (0, 0, 0),
                block_shape((0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 4),
            )])
            .expect("a cobweb has no collision box");
        assert_eq!(world.block_at(0, 0, 0).effect, 4);
        assert!(world
            .collision_shapes(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0))
            .expect("valid query")
            .is_empty());
    }

    #[test]
    fn collision_boxes_may_reach_one_block_out_of_their_own() {
        let mut world = NativeWorld::new();
        world
            .apply_block_updates(vec![
                shape_update((0, 0, 0), block_shape((0.375, 0.0, 0.375), (0.625, 1.5, 0.625), 0)),
                shape_update((0, 0, 0), block_shape((0.375, 0.375, 0.0), (0.625, 1.5, 0.375), 0)),
            ])
            .expect("a fence is 1.5 blocks tall");
        assert_eq!(
            world
                .collision_shapes(vec3(0.0, 1.2, 0.0), vec3(1.0, 2.0, 1.0))
                .expect("valid query")
                .len(),
            2
        );
        assert!(world
            .apply_block_updates(vec![shape_update(
                (0, 0, 0),
                block_shape((0.0, 0.0, 0.0), (1.0, 2.5, 1.0), 0),
            )])
            .is_err());
    }

    #[test]
    fn versioned_entity_push_updates_both_velocities() {
        let result = simulate_versioned_entity_push(
            "1.21.1".to_string(),
            push_state(0.0, 0.0, 0.4),
            push_state(1.0, 0.0, 0.0),
            false,
        )
        .expect("supported entity push should succeed");
        assert!(result.left_velocity.x < 0.0);
        assert!(result.right_velocity.x > 0.0);
    }

    #[test]
    fn versioned_entity_push_ignores_removed_pushthrough_behavior() {
        let result = simulate_versioned_entity_push(
            "1.21.4".to_string(),
            push_state(0.0, 0.0, 1.0),
            push_state(1.0, 0.0, 0.0),
            false,
        )
        .expect("supported entity push should succeed");
        assert_eq!(result.left_velocity.x, -0.05_f32 as f64);
        assert_eq!(result.right_velocity.x, 0.05_f32 as f64);
    }

    #[test]
    fn versioned_entity_push_rejects_unsupported_versions() {
        let result = simulate_versioned_entity_push(
            "1.22.1".to_string(),
            push_state(0.0, 0.0, 0.0),
            push_state(1.0, 0.0, 0.0),
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn versioned_entity_push_rejects_invalid_pushthrough() {
        let result = simulate_versioned_entity_push(
            "1.21.1".to_string(),
            push_state(0.0, 0.0, 1.1),
            push_state(1.0, 0.0, 0.0),
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn versioned_pose_refresh_falls_back_to_swimming_when_taller_poses_are_blocked() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 1, 0, 1);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &world, pose_state(), 0, 0, false)
                .expect("pose refresh should succeed");
        assert_eq!(result.pose, 1);
        assert!(result.changed);
        assert_eq!(result.state.height, 0.6);
        assert_eq!(result.state.eye_height, Some(0.4));
    }

    #[test]
    fn versioned_pose_refresh_allows_spectator_to_select_requested_pose() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 1, 0, 1);
        let result = refresh_versioned_pose("1.21.1".to_string(), &world, pose_state(), 1, 0, true)
            .expect("spectator pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert_eq!(result.state.height, 1.8);
    }

    #[test]
    fn versioned_pose_refresh_preserves_pose_when_swimming_fit_is_blocked() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 0, 0, 1);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &world, pose_state(), 0, 5, false)
                .expect("pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert!(!result.changed);
        assert_eq!(result.state.height, 1.8);
    }

    #[test]
    fn versioned_pose_refresh_allows_passenger_to_select_requested_pose() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 1, 0, 1);
        let mut state = pose_state();
        state.passenger = Some(true);
        let result = refresh_versioned_pose("1.21.1".to_string(), &world, state, 1, 0, false)
            .expect("pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert_eq!(result.state.height, 1.8);
    }

    #[test]
    fn versioned_pose_refresh_rejects_unavailable_long_jumping_pose() {
        let result = refresh_versioned_pose(
            "1.14.4".to_string(),
            &NativeWorld::new(),
            pose_state(),
            0,
            7,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn versioned_bulk_ticks_run_concrete_dispatch_for_each_state() {
        let descriptor = NativeEntityDescriptor {
            position: NativeVec3 {
                x: 0.0,
                y: 2.0,
                z: 0.0,
            },
            velocity: NativeVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            half_width: 0.3,
            height: 1.8,
            on_ground: false,
            yaw: 0.0,
            in_water: false,
            in_lava: false,
            version: Some("1.21.1".to_string()),
            pose: None,
            pitch: None,
            input: None,
            jump_requested: None,
            jump_ticks: None,
            sprinting: None,
            sneaking: None,
            using_item: None,
            swimming: None,
            passenger: None,
            flying: None,
            flying_speed: None,
            climbing: None,
            no_physics: None,
            no_gravity: None,
            fall_flying: None,
            fall_damage_immune: None,
            fall_damage_multiplier: None,
            slow_falling: None,
            levitation: None,
            levitation_amplifier: None,
            movement_speed: None,
            jump_strength: None,
            speed_effect_amplifier: None,
            slowness_effect_amplifier: None,
            jump_boost_amplifier: None,
            dolphins_grace: None,
            depth_strider: None,
            soul_speed: None,
            water_movement_efficiency: None,
            movement_efficiency: None,
            air_drag_modifier: None,
            friction_modifier: None,
            sneaking_speed: None,
            look: None,
            firework_boost: None,
        };
        let state = create_simulation_state(descriptor).expect("descriptor should be valid");
        let result = simulate_versioned_ticks(
            "1.21.1".to_string(),
            &NativeWorld::new(),
            vec![state],
            2,
            NativeSimulationConfig {
                gravity: None,
                drag: None,
                movement_speed: None,
                sprint_multiplier: None,
                jump_velocity: None,
                step_height: None,
                negligible_velocity: None,
            },
        )
        .expect("bulk versioned simulation should succeed");
        assert_eq!(result.len(), 1);
        assert!(result[0].position.y < 2.0);
    }

    #[test]
    fn versioned_entity_ticks_apply_pairwise_push_after_movement() {
        let descriptor = |x: f64| NativeEntityDescriptor {
            position: NativeVec3 { x, y: 2.0, z: 0.0 },
            velocity: NativeVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            half_width: 0.3,
            height: 1.8,
            on_ground: false,
            yaw: 0.0,
            in_water: false,
            in_lava: false,
            version: Some("1.21.1".to_string()),
            pose: None,
            pitch: None,
            input: None,
            jump_requested: None,
            jump_ticks: None,
            sprinting: None,
            sneaking: None,
            using_item: None,
            swimming: None,
            passenger: None,
            flying: None,
            flying_speed: None,
            climbing: None,
            no_physics: None,
            no_gravity: None,
            fall_flying: None,
            fall_damage_immune: None,
            fall_damage_multiplier: None,
            slow_falling: None,
            levitation: None,
            levitation_amplifier: None,
            movement_speed: None,
            jump_strength: None,
            speed_effect_amplifier: None,
            slowness_effect_amplifier: None,
            jump_boost_amplifier: None,
            dolphins_grace: None,
            depth_strider: None,
            soul_speed: None,
            water_movement_efficiency: None,
            movement_efficiency: None,
            air_drag_modifier: None,
            friction_modifier: None,
            sneaking_speed: None,
            look: None,
            firework_boost: None,
        };
        let left = NativeVersionedEntityState {
            simulation: create_simulation_state(descriptor(0.0)).unwrap(),
            no_physics: false,
            vehicle_id: None,
            pushable: true,
            pushthrough: 0.0,
        };
        let right = NativeVersionedEntityState {
            simulation: create_simulation_state(descriptor(0.4)).unwrap(),
            no_physics: false,
            vehicle_id: None,
            pushable: true,
            pushthrough: 0.0,
        };
        let result = simulate_versioned_entity_ticks(
            "1.21.1".to_string(),
            &NativeWorld::new(),
            vec![left, right],
            1,
            NativeSimulationConfig {
                gravity: None,
                drag: None,
                movement_speed: None,
                sprint_multiplier: None,
                jump_velocity: None,
                step_height: None,
                negligible_velocity: None,
            },
        )
        .unwrap();
        assert!(result[0].simulation.velocity.x < 0.0);
        assert!(result[1].simulation.velocity.x > 0.0);
    }

    #[test]
    fn versioned_entity_ticks_do_not_push_non_colliding_entities() {
        let state = |x: f64| NativeVersionedEntityState {
            simulation: NativeSimulationState {
                position: NativeVec3 { x, y: 2.0, z: 0.0 },
                velocity: NativeVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                yaw: Some(0.0),
                pitch: None,
                half_width: 0.3,
                height: 1.8,
                on_ground: false,
                collided_horizontally: false,
                minor_horizontal_collision: None,
                collided_vertically: false,
                vertical_collision_below: None,
                block_fall_callback: None,
                entity_bounciness: None,
                block_bounciness: None,
                input: None,
                jump_requested: None,
                jump_ticks: None,
                auto_jump: None,
                auto_jump_time: None,
                riptide_launch: None,
                sprinting: None,
                was_sprinting: None,
                sprint_modifier: None,
                in_water: None,
                in_lava: None,
                ultra_warm: None,
                is_player: None,
                controlled_by_local_instance: None,
                passenger: None,
                in_web: None,
                weaving: None,
                sweet_berry_bush_immune: None,
                powder_snow_immune: None,
                powder_snow_walkable: None,
                in_powder_snow: None,
                ticks_frozen: None,
                ticks_required_to_freeze: None,
                tick_count: None,
                freeze_damage: None,
                contact_damage: None,
                inside_wall: None,
                eye_height: None,
                eye_in_water: None,
                eye_in_lava: None,
                slow_falling: None,
                levitation: None,
                levitation_amplifier: None,
                fall_distance: None,
                safe_fall_distance: None,
                fall_damage_immune: None,
                fall_damage_multiplier: None,
                sneaking: None,
                moving_slowly: None,
                stepping_carefully: None,
                using_item: None,
                item_use_speed_multiplier: None,
                swimming: None,
                flying: None,
                flying_speed: None,
                climbing: None,
                no_physics: None,
                no_gravity: None,
                removed: None,
                gravity: None,
                step_height: None,
                movement_speed: None,
                jump_strength: None,
                speed_effect_amplifier: None,
                slowness_effect_amplifier: None,
                jump_boost_amplifier: None,
                dolphins_grace: None,
                depth_strider: None,
                soul_speed: None,
                water_movement_efficiency: None,
                movement_efficiency: None,
                air_drag_modifier: None,
                friction_modifier: None,
                sneaking_speed: None,
                fall_damage: None,
                fall_flying: None,
                look: None,
                firework_boost: None,
                stuck_speed_multiplier: None,
                previous_position: None,
                bounding_box: None,
                nearby_entities: None,
                firework_rockets: None,
                firework_rockets_before_player: None,
                moving_pistons: None,
            },
            no_physics: false,
            vehicle_id: None,
            pushable: true,
            pushthrough: 0.0,
        };
        let result = simulate_versioned_entity_ticks(
            "1.21.1".to_string(),
            &NativeWorld::new(),
            vec![state(0.0), state(2.0)],
            0,
            NativeSimulationConfig {
                gravity: None,
                drag: None,
                movement_speed: None,
                sprint_multiplier: None,
                jump_velocity: None,
                step_height: None,
                negligible_velocity: None,
            },
        )
        .unwrap();
        assert_eq!(result[0].simulation.velocity.x, 0.0);
        assert_eq!(result[0].simulation.velocity.y, 0.0);
        assert_eq!(result[0].simulation.velocity.z, 0.0);
        assert_eq!(result[1].simulation.velocity.x, 0.0);
        assert_eq!(result[1].simulation.velocity.y, 0.0);
        assert_eq!(result[1].simulation.velocity.z, 0.0);
    }
}

fn valid_horse_state(state: HorseSimulationState) -> bool {
    state.position.x.is_finite()
        && state.position.y.is_finite()
        && state.position.z.is_finite()
        && state.velocity.x.is_finite()
        && state.velocity.y.is_finite()
        && state.velocity.z.is_finite()
        && state.yaw.is_finite()
        && state.forward.is_finite()
        && state.strafe.is_finite()
        && state.movement_speed.is_finite()
        && state.jump_strength.is_finite()
        && state.movement_speed >= 0.0
        && state.jump_strength >= 0.0
        && state.half_width > 0.0
        && state.height > 0.0
}

fn horse_state(value: NativeHorseState) -> Result<HorseSimulationState> {
    let state = HorseSimulationState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        yaw: value.yaw,
        forward: value.forward,
        strafe: value.strafe,
        jump_requested: value.jump_requested,
        on_ground: value.on_ground,
        in_water: value.in_water,
        movement_speed: value.movement_speed,
        jump_strength: value.jump_strength,
        half_width: value.half_width.unwrap_or(0.7),
        height: value.height.unwrap_or(1.6),
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
    };
    if !valid_horse_state(state) {
        return Err(Error::from_reason(
            "horse simulation state contains invalid values",
        ));
    }
    Ok(state)
}

fn native_horse_state(state: HorseSimulationState) -> NativeHorseState {
    NativeHorseState {
        position: state.position.into(),
        velocity: state.velocity.into(),
        yaw: state.yaw,
        forward: state.forward,
        strafe: state.strafe,
        jump_requested: state.jump_requested,
        on_ground: state.on_ground,
        in_water: state.in_water,
        movement_speed: state.movement_speed,
        jump_strength: state.jump_strength,
        half_width: Some(state.half_width),
        height: Some(state.height),
        collided_horizontally: Some(state.collided_horizontally),
        collided_vertically: Some(state.collided_vertically),
        entity_type: None,
        yaw_degrees: None,
        pitch_degrees: None,
        rider_yaw: None,
        rider_pitch: None,
        rider_sprinting: None,
        rider_jump: None,
        in_lava: None,
        fall_distance: None,
        bounding_box: None,
        previous_position: None,
        flying_speed: None,
        saddled: None,
        no_ai: None,
        ridden_speed: None,
        own_forward: None,
        boosting: None,
        boost_time: None,
        boost_time_total: None,
        suffocating: None,
        jump_pending_scale: None,
        horse_jumping: None,
        standing: None,
        allow_stand_sliding: None,
        stand_counter: None,
        dash_cooldown: None,
        dashing: None,
        sitting: None,
        pose_time: None,
        lerp_position: None,
    }
}

/// A ridden mob from the horse state: the rider's xxa and zza are `strafe` and `forward`, its
/// jumping `jumpRequested`; an attribute given as 0 takes the mob's base.
fn ridden_mob_state(value: NativeHorseState) -> Result<RiddenMobState> {
    let name = value.entity_type.as_deref().unwrap_or("horse");
    let kind = RiddenMobKind::parse(name)
        .ok_or_else(|| Error::from_reason(format!("no ridden mob simulation for {name}")))?;
    let defaults = RiddenMobState::new(kind, value.position.into());
    let positive = |value: f64, default: f64| if value > 0.0 { value } else { default };
    let mob = RiddenMobState {
        velocity: value.velocity.into(),
        yaw: value.yaw_degrees.unwrap_or_else(|| value.yaw.to_degrees()) as f32,
        pitch: value.pitch_degrees.unwrap_or(0.0) as f32,
        on_ground: value.on_ground,
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
        in_water: value.in_water,
        in_lava: value.in_lava.unwrap_or(false),
        fall_distance: value.fall_distance.unwrap_or(0.0),
        bounding_box: value.bounding_box.map(Into::into),
        previous_position: value.previous_position.map(Into::into),
        movement_speed: positive(value.movement_speed, defaults.movement_speed),
        jump_strength: positive(value.jump_strength, defaults.jump_strength),
        flying_speed: value.flying_speed.unwrap_or(defaults.flying_speed),
        saddled: value.saddled.unwrap_or(true),
        no_ai: value.no_ai.unwrap_or(false),
        ridden_speed: value.ridden_speed.unwrap_or(0.0) as f32,
        own_forward: value.own_forward.unwrap_or(0.0) as f32,
        boosting: value.boosting.unwrap_or(false),
        boost_time: value.boost_time.unwrap_or(0),
        boost_time_total: value.boost_time_total.unwrap_or(0),
        suffocating: value.suffocating.unwrap_or(false),
        jump_pending_scale: value.jump_pending_scale.unwrap_or(0.0) as f32,
        horse_jumping: value.horse_jumping.unwrap_or(false),
        standing: value.standing.unwrap_or(false),
        allow_stand_sliding: value.allow_stand_sliding.unwrap_or(false),
        stand_counter: value.stand_counter.unwrap_or(0),
        dash_cooldown: value.dash_cooldown.unwrap_or(0),
        dashing: value.dashing.unwrap_or(false),
        sitting: value.sitting.unwrap_or(false),
        pose_time: value.pose_time.unwrap_or(defaults.pose_time),
        rider: RiderInput {
            yaw: value.rider_yaw.unwrap_or(0.0) as f32,
            pitch: value.rider_pitch.unwrap_or(0.0) as f32,
            strafe: value.strafe as f32,
            forward: value.forward as f32,
            sprinting: value.rider_sprinting.unwrap_or(false),
            jumping: value.jump_requested,
            released_jump: value.rider_jump,
        },
        lerp_position: value.lerp_position.map(Into::into),
        ..defaults
    };
    let finite = mob.position.is_finite()
        && mob.velocity.is_finite()
        && mob.yaw.is_finite()
        && mob.movement_speed.is_finite()
        && mob.jump_strength.is_finite();
    if !finite {
        return Err(Error::from_reason(
            "ridden mob simulation state contains invalid values",
        ));
    }
    Ok(mob)
}

fn ridden_mob_name(kind: RiddenMobKind) -> &'static str {
    match kind {
        RiddenMobKind::Pig => "pig",
        RiddenMobKind::Strider => "strider",
        RiddenMobKind::Camel => "camel",
        RiddenMobKind::HappyGhast => "happy_ghast",
        RiddenMobKind::Nautilus => "nautilus",
        RiddenMobKind::ZombieNautilus => "zombie_nautilus",
        RiddenMobKind::Horse => "horse",
        RiddenMobKind::Donkey => "donkey",
        RiddenMobKind::Mule => "mule",
        RiddenMobKind::SkeletonHorse => "skeleton_horse",
        RiddenMobKind::ZombieHorse => "zombie_horse",
        RiddenMobKind::Llama => "llama",
        RiddenMobKind::TraderLlama => "trader_llama",
    }
}

fn native_ridden_mob_state(mob: RiddenMobState) -> NativeHorseState {
    let (width, height) = mob.kind.dimensions();
    NativeHorseState {
        position: mob.position.into(),
        velocity: mob.velocity.into(),
        yaw: f64::from(mob.yaw).to_radians(),
        forward: f64::from(mob.rider.forward),
        strafe: f64::from(mob.rider.strafe),
        jump_requested: mob.rider.jumping,
        on_ground: mob.on_ground,
        in_water: mob.in_water,
        movement_speed: mob.movement_speed,
        jump_strength: mob.jump_strength,
        half_width: Some(f64::from(width / 2.0)),
        height: Some(f64::from(height)),
        collided_horizontally: Some(mob.collided_horizontally),
        collided_vertically: Some(mob.collided_vertically),
        entity_type: Some(ridden_mob_name(mob.kind).to_string()),
        yaw_degrees: Some(f64::from(mob.yaw)),
        pitch_degrees: Some(f64::from(mob.pitch)),
        rider_yaw: Some(f64::from(mob.rider.yaw)),
        rider_pitch: Some(f64::from(mob.rider.pitch)),
        rider_sprinting: Some(mob.rider.sprinting),
        rider_jump: None,
        in_lava: Some(mob.in_lava),
        fall_distance: Some(mob.fall_distance),
        bounding_box: mob.bounding_box.map(Into::into),
        previous_position: mob.previous_position.map(Into::into),
        flying_speed: Some(mob.flying_speed),
        saddled: Some(mob.saddled),
        no_ai: Some(mob.no_ai),
        ridden_speed: Some(f64::from(mob.ridden_speed)),
        own_forward: Some(f64::from(mob.own_forward)),
        boosting: Some(mob.boosting),
        boost_time: Some(mob.boost_time),
        boost_time_total: Some(mob.boost_time_total),
        suffocating: Some(mob.suffocating),
        jump_pending_scale: Some(f64::from(mob.jump_pending_scale)),
        horse_jumping: Some(mob.horse_jumping),
        standing: Some(mob.standing),
        allow_stand_sliding: Some(mob.allow_stand_sliding),
        stand_counter: Some(mob.stand_counter),
        dash_cooldown: Some(mob.dash_cooldown),
        dashing: Some(mob.dashing),
        sitting: Some(mob.sitting),
        pose_time: Some(mob.pose_time),
        lerp_position: None,
    }
}

fn valid_boat_state(state: BoatSimulationState) -> bool {
    state.position.x.is_finite()
        && state.position.y.is_finite()
        && state.position.z.is_finite()
        && state.velocity.x.is_finite()
        && state.velocity.y.is_finite()
        && state.velocity.z.is_finite()
        && state.yaw.is_finite()
        && state.delta_rotation.is_finite()
}

fn parse_boat_status(value: Option<&str>) -> BoatStatus {
    match value {
        Some("in_water") => BoatStatus::InWater,
        Some("under_water") => BoatStatus::UnderWater,
        Some("under_flowing_water") => BoatStatus::UnderFlowingWater,
        Some("on_land") => BoatStatus::OnLand,
        _ => BoatStatus::InAir,
    }
}

fn boat_state(value: NativeBoatState) -> Result<BoatSimulationState> {
    let state = BoatSimulationState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        yaw: value.yaw,
        yaw_degrees: value.yaw_degrees.unwrap_or_else(|| value.yaw.to_degrees()) as f32,
        forward: value.forward,
        left: value.left,
        right: value.right,
        input_down: value.input_down.unwrap_or(false),
        delta_rotation: value.delta_rotation.unwrap_or(0.0),
        in_water: value.in_water,
        water_level: value.water_level.unwrap_or(f64::NEG_INFINITY),
        land_friction: value.land_friction.unwrap_or(0.0) as f32,
        status: parse_boat_status(value.status.as_deref()),
        previous_status: parse_boat_status(value.previous_status.as_deref()),
        settle_status: value.status.is_none(),
        last_yd: value.last_vertical_velocity.unwrap_or(0.0),
        bubble_time: value.bubble_time.unwrap_or(0),
        bubble_column_direction_is_down: value
            .bubble_column_direction_is_down
            .unwrap_or(false),
        above_bubble_column: value.above_bubble_column.unwrap_or(false),
        player_passenger: value.player_passenger.unwrap_or(false),
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
        on_ground: value.on_ground.unwrap_or(false),
        bounding_box: value.bounding_box.map(Into::into),
        legacy: LegacyBoatState {
            speed_multiplier: value
                .speed_multiplier
                .unwrap_or(LegacyBoatState::default().speed_multiplier),
            rider_yaw: value.rider_yaw.unwrap_or(0.0) as f32,
            rider_forward: value.rider_forward.unwrap_or(0.0) as f32,
            rider_strafe: value.rider_strafe.unwrap_or(0.0) as f32,
        },
    };
    if !valid_boat_state(state) {
        return Err(Error::from_reason(
            "boat simulation state contains invalid values",
        ));
    }
    Ok(state)
}

fn boat_status_name(status: BoatStatus) -> String {
    match status {
        BoatStatus::InWater => "in_water",
        BoatStatus::UnderWater => "under_water",
        BoatStatus::UnderFlowingWater => "under_flowing_water",
        BoatStatus::OnLand => "on_land",
        BoatStatus::InAir => "in_air",
    }
    .to_string()
}

fn native_boat_state(state: BoatSimulationState) -> NativeBoatState {
    NativeBoatState {
        position: state.position.into(),
        velocity: state.velocity.into(),
        yaw: state.yaw,
        forward: state.forward,
        left: state.left,
        right: state.right,
        input_down: Some(state.input_down),
        delta_rotation: Some(state.delta_rotation),
        in_water: state.in_water,
        water_level: Some(state.water_level),
        status: Some(boat_status_name(state.status)),
        previous_status: Some(boat_status_name(state.previous_status)),
        bubble_time: Some(state.bubble_time),
        bubble_column_direction_is_down: Some(state.bubble_column_direction_is_down),
        above_bubble_column: Some(state.above_bubble_column),
        player_passenger: Some(state.player_passenger),
        collided_horizontally: Some(state.collided_horizontally),
        collided_vertically: Some(state.collided_vertically),
        on_ground: Some(state.on_ground),
        yaw_degrees: Some(f64::from(state.yaw_degrees)),
        land_friction: Some(f64::from(state.land_friction)),
        last_vertical_velocity: Some(state.last_yd),
        bounding_box: state.bounding_box.map(Into::into),
        speed_multiplier: Some(state.legacy.speed_multiplier),
        rider_yaw: Some(f64::from(state.legacy.rider_yaw)),
        rider_forward: Some(f64::from(state.legacy.rider_forward)),
        rider_strafe: Some(f64::from(state.legacy.rider_strafe)),
    }
}

fn valid_shape(bounds: crate::math::Aabb, slipperiness: f64) -> bool {
    valid_shape_within(bounds, slipperiness, 0.0, 1.0)
}

fn valid_collision_shape(bounds: crate::math::Aabb, slipperiness: f64) -> bool {
    valid_shape_within(bounds, slipperiness, -1.0, 2.0)
}

fn valid_shape_within(bounds: crate::math::Aabb, slipperiness: f64, min: f64, max: f64) -> bool {
    valid_bounds(bounds)
        && bounds.min.x >= min
        && bounds.min.y >= min
        && bounds.min.z >= min
        && bounds.max.x <= max
        && bounds.max.y <= max
        && bounds.max.z <= max
        && slipperiness.is_finite()
        && slipperiness >= 0.0
}

fn native_block_shape(value: NativeBlockShape) -> Result<BlockShape> {
    let bounds = crate::math::Aabb::new(
        Vec3::new(value.min_x, value.min_y, value.min_z),
        Vec3::new(value.max_x, value.max_y, value.max_z),
    );
    if !valid_shape(bounds, value.slipperiness) {
        return Err(Error::from_reason(
            "outline shape bounds must be finite, ordered, and within one block",
        ));
    }

    Ok(BlockShape {
        bounds,
        slipperiness: value.slipperiness,
        fluid: FluidState::Empty,
        effect: block_effect(value.effect.unwrap_or(0))?,
    })
}

fn validate_block_properties(properties: &[NativeBlockProperty]) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    for property in properties {
        if property.name.is_empty() || property.value.is_empty() {
            return Err(Error::from_reason(
                "block property names and values must not be empty",
            ));
        }
        if !names.insert(property.name.as_str()) {
            return Err(Error::from_reason(
                "block property names must be unique within a state",
            ));
        }
    }
    Ok(())
}

fn valid_bounds(bounds: crate::math::Aabb) -> bool {
    bounds.min.x.is_finite()
        && bounds.min.y.is_finite()
        && bounds.min.z.is_finite()
        && bounds.max.x.is_finite()
        && bounds.max.y.is_finite()
        && bounds.max.z.is_finite()
        && bounds.min.x <= bounds.max.x
        && bounds.min.y <= bounds.max.y
        && bounds.min.z <= bounds.max.z
}

fn fluid_code(fluid: FluidState) -> u8 {
    match fluid {
        FluidState::Empty => 0,
        FluidState::Water => 1,
        FluidState::Lava => 2,
        FluidState::Waterlogged => 3,
    }
}

fn decode_fluid(value: u8) -> Result<FluidState> {
        match value {
            0 => Ok(FluidState::Empty),
            1 => Ok(FluidState::Water),
            2 => Ok(FluidState::Lava),
            3 => Ok(FluidState::Waterlogged),
            _ => Err(Error::from_reason("fluid must be 0, 1, 2, or 3")),
        }
    }

fn generated_fluid_id(fluid: FluidState) -> Option<&'static str> {
    match fluid {
        FluidState::Water => Some("water"),
        FluidState::Lava => Some("lava"),
        FluidState::Empty | FluidState::Waterlogged => None,
    }
}

fn validate_fluid_state(
        fluid: FluidState,
        level: u8,
        source: bool,
        falling: bool,
        flow: Vec3,
    ) -> Result<()> {
        if level > 15 {
            return Err(Error::from_reason("fluid level must be between 0 and 15"));
        }
        if !flow.is_finite() {
            return Err(Error::from_reason("fluid flow must contain finite values"));
        }
        if fluid == FluidState::Empty && (source || falling) {
            return Err(Error::from_reason(
                "empty fluid states cannot be source or falling fluids",
            ));
        }
        if source && falling {
            return Err(Error::from_reason(
                "source fluid states cannot be marked as falling",
            ));
        }
        Ok(())
}

fn block_effect_code(effect: BlockEffect) -> u8 {
    match effect {
        BlockEffect::None => 0,
        BlockEffect::SoulSand => 1,
        BlockEffect::Honey => 2,
        BlockEffect::Slime => 3,
        BlockEffect::Web => 4,
        BlockEffect::Climbable => 5,
        BlockEffect::BubbleUp => 6,
        BlockEffect::BubbleDown => 7,
        BlockEffect::PowderSnow => 8,
        BlockEffect::SweetBerryBush => 9,
        BlockEffect::SoulSoil => 10,
        BlockEffect::Scaffolding => 11,
        BlockEffect::Cactus => 12,
        BlockEffect::Magma => 13,
        BlockEffect::Campfire => 14,
        BlockEffect::SoulCampfire => 15,
        BlockEffect::Fire => 16,
        BlockEffect::SoulFire => 17,
        BlockEffect::Bed => 18,
    }
}

fn block_update_state(update: &NativeBlockUpdate) -> Result<BlockState> {
    if update.remove.unwrap_or(false) {
        return Ok(BlockState::default());
    }
    let fluid = match update.fluid.unwrap_or(0) {
        0 => FluidState::Empty,
        1 => FluidState::Water,
        2 => FluidState::Lava,
        3 => FluidState::Waterlogged,
        _ => return Err(Error::from_reason("fluid must be 0, 1, 2, or 3")),
    };
    let fluid_level = update.fluid_level.unwrap_or(0);
    if fluid_level > 15 {
        return Err(Error::from_reason("fluid level must be between 0 and 15"));
    }
    let fluid_falling = update.fluid_falling.unwrap_or(false);
    let fall_damage_resetting = update.fall_damage_resetting.unwrap_or(false);
    let fluid_source = update
        .fluid_source
        .unwrap_or(fluid != FluidState::Empty && update.fluid_level.is_none());
    if fluid == FluidState::Empty && fluid_falling {
        return Err(Error::from_reason(
            "empty fluid states cannot be marked as falling",
        ));
    }
    if fluid == FluidState::Empty && fluid_source {
        return Err(Error::from_reason(
            "empty fluid states cannot be marked as source fluids",
        ));
    }
    if fluid_source && fluid_falling {
        return Err(Error::from_reason(
            "source fluid states cannot be marked as falling",
        ));
    }
    let bounce_restitution = update.bounce_restitution.unwrap_or(0.0);
    let suppresses_bounce = update.suppresses_bounce.unwrap_or(false);
    if !bounce_restitution.is_finite() || bounce_restitution < 0.0 {
        return Err(Error::from_reason(
            "bounce restitution must be finite and non-negative",
        ));
    }
    let shape = update.shape.as_ref().map(|value| {
        let bounds = crate::math::Aabb::new(
            Vec3::new(value.min_x, value.min_y, value.min_z),
            Vec3::new(value.max_x, value.max_y, value.max_z),
        );
        (bounds, value.slipperiness, value.effect.unwrap_or(0))
    });
    if let Some((bounds, slipperiness, effect)) = shape {
        if !valid_collision_shape(bounds, slipperiness) {
            return Err(Error::from_reason(
                "block shape bounds must be finite, ordered, and reach at most one block out of their own",
            ));
        }
        let outline_shape = if let Some(value) = update.outline_shape.as_ref() {
            let outline_bounds = crate::math::Aabb::new(
                Vec3::new(value.min_x, value.min_y, value.min_z),
                Vec3::new(value.max_x, value.max_y, value.max_z),
            );
            if !valid_shape(outline_bounds, value.slipperiness) {
                return Err(Error::from_reason(
                    "outline shape bounds must be finite, ordered, and within one block",
                ));
            }
            Some(BlockShape {
                bounds: outline_bounds,
                slipperiness: value.slipperiness,
                fluid,
                effect: block_effect(value.effect.unwrap_or(0))?,
            })
        } else {
            Some(BlockShape {
                bounds,
                slipperiness,
                fluid,
                effect: block_effect(effect)?,
            })
        };
        return Ok(BlockState {
            id: update.id.unwrap_or(0),
            tags: update.tags.unwrap_or(0),
            shape: Some(BlockShape {
                bounds,
                slipperiness,
                fluid,
                effect: block_effect(effect)?,
            }),
            outline_shape,
            interaction_shape: if let Some(value) = update.interaction_shape.as_ref() {
                Some(BlockShape {
                    fluid,
                    ..native_block_shape(value.clone())?
                })
            } else {
                outline_shape
            },
            bounce_restitution,
            suppresses_bounce,
            fluid,
            fluid_tags: update.fluid_tags.unwrap_or(0),
            fluid_level,
            fluid_source,
            fluid_flow: update
                .fluid_flow
                .as_ref()
                .map_or(Vec3::ZERO, |flow| Vec3::new(flow.x, flow.y, flow.z)),
            fluid_falling,
            fall_damage_resetting,
            suffocating: update.suffocating,
        });
    }
    Ok(BlockState {
        id: update.id.unwrap_or(0),
        tags: update.tags.unwrap_or(0),
        shape: (update.id.is_some()).then(BlockShape::full_block),
        outline_shape: if let Some(value) = update.outline_shape.as_ref() {
            let bounds = crate::math::Aabb::new(
                Vec3::new(value.min_x, value.min_y, value.min_z),
                Vec3::new(value.max_x, value.max_y, value.max_z),
            );
            if !valid_shape(bounds, value.slipperiness) {
                return Err(Error::from_reason(
                    "outline shape bounds must be finite, ordered, and within one block",
                ));
            }
            Some(BlockShape {
                bounds,
                slipperiness: value.slipperiness,
                fluid,
                effect: block_effect(value.effect.unwrap_or(0))?,
            })
        } else {
            (update.id.is_some()).then(BlockShape::full_block)
        },
        interaction_shape: if let Some(value) = update.interaction_shape.as_ref() {
            let bounds = crate::math::Aabb::new(
                Vec3::new(value.min_x, value.min_y, value.min_z),
                Vec3::new(value.max_x, value.max_y, value.max_z),
            );
            if !valid_shape(bounds, value.slipperiness) {
                return Err(Error::from_reason(
                    "interaction shape bounds must be finite, ordered, and within one block",
                ));
            }
            Some(BlockShape {
                bounds,
                slipperiness: value.slipperiness,
                fluid,
                effect: block_effect(value.effect.unwrap_or(0))?,
            })
        } else {
            (update.id.is_some()).then(BlockShape::full_block)
        },
        bounce_restitution,
        suppresses_bounce,
        fluid,
        fluid_tags: update.fluid_tags.unwrap_or(0),
        fluid_level,
        fluid_source,
        fluid_flow: update
            .fluid_flow
            .as_ref()
            .map_or(Vec3::ZERO, |flow| Vec3::new(flow.x, flow.y, flow.z)),
        fluid_falling,
        fall_damage_resetting,
        suffocating: update.suffocating,
    })
}

fn block_effect(value: u8) -> Result<BlockEffect> {
    match value {
        0 => Ok(BlockEffect::None),
        1 => Ok(BlockEffect::SoulSand),
        2 => Ok(BlockEffect::Honey),
        3 => Ok(BlockEffect::Slime),
        4 => Ok(BlockEffect::Web),
        5 => Ok(BlockEffect::Climbable),
        6 => Ok(BlockEffect::BubbleUp),
        7 => Ok(BlockEffect::BubbleDown),
        8 => Ok(BlockEffect::PowderSnow),
        9 => Ok(BlockEffect::SweetBerryBush),
        10 => Ok(BlockEffect::SoulSoil),
        11 => Ok(BlockEffect::Scaffolding),
        12 => Ok(BlockEffect::Cactus),
        13 => Ok(BlockEffect::Magma),
        14 => Ok(BlockEffect::Campfire),
        15 => Ok(BlockEffect::SoulCampfire),
        16 => Ok(BlockEffect::Fire),
        17 => Ok(BlockEffect::SoulFire),
        18 => Ok(BlockEffect::Bed),
        _ => Err(Error::from_reason("block effect must be between 0 and 18")),
    }
}
