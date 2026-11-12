use std::sync::atomic::{AtomicU64, Ordering};

use napi::bindgen_prelude::Result;
use napi::Error;
use napi_derive::napi;

use crate::entity_travel::{
    refuse_for_player_tick, refuse_for_ridden_tick, travel_support, TravelFamily,
};
use crate::entity::{
    entity_push_impulses_with_pushthrough, nearby_entity_colliders, vehicle_entity_colliders,
    Dimensions, EntityFlags,
    EntityPush,
    EntityState, NearbyEntity, NearbyEntityKind,
};
use crate::math::{Aabb, Vec3, MAX_COORDINATE, MAX_WALKED_CELLS};
use crate::napi_bulk::{
    ArrowBatch, BoatBatch, EntityBatch, HorseBatch, PlayerBatch, ProjectileBatch, TridentBatch,
};
use crate::piston::{push_by_moving_pistons, set_moving_block, MovingPiston};
use crate::projectile::{
    tick_arrow, tick_projectile, tick_trident,
    ArrowState, ProjectileKind, ProjectileState, TridentState,
};
use crate::ridden::{tick_ridden_mob, RiddenMobKind, RiddenMobState, RiderInput};
use crate::simulation::{
    AttributeModifier, AttributeOperation, MovementSpeedAttribute,
    collision_query_entity, entity_collision_context, mth_cos, mth_sin,
    finish_local_player, move_local_player, tick_scene, BoatVehicle, PhysicsConfig, ServerFireDraws,
    SimulationState, TickScene,
    TickSceneOutcome, VersionPhysicsRules,
};
use crate::vehicle::{
    boat_passenger_position, boat_passenger_yaw, tick_boat, tick_boat_in_world, tick_horse, BoatSeat,
    BoatSimulationState, BoatStatus,
    HorseSimulationState, LegacyBoatState,
};
use crate::version::MinecraftVersion;
use crate::versioned;
use crate::shulker_box::{ShulkerBox, ShulkerBoxStatus};
use crate::world::{
    checkpoint_digest,
    shape_meets_query, BlockEffect, BlockShape, BlockState, EntityCollider, FluidState,
    WorldSnapshot,
};

/// A stab's recoil on the attacker: `knockback` is getKnockback, 0 when not given.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeStabHit {
    pub knockback: Option<f64>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Entity.Movement: a move from `from` to `to`, checked one axis at a time in the order of
/// `axisOrder` where it has one (Direction.axisStepOrder, from 1.21.5).
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeMovement {
    pub from: NativeVec3,
    pub to: NativeVec3,
    pub axis_order: Option<NativeVec3>,
}

impl From<NativeMovement> for crate::block_traversal::Movement {
    fn from(value: NativeMovement) -> Self {
        Self { from: value.from.into(), to: value.to.into(), axis_order: value.axis_order.map(Into::into) }
    }
}

impl From<crate::block_traversal::Movement> for NativeMovement {
    fn from(value: crate::block_traversal::Movement) -> Self {
        Self { from: value.from.into(), to: value.to.into(), axis_order: value.axis_order.map(Into::into) }
    }
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeAabb {
    pub min: NativeVec3,
    pub max: NativeVec3,
}

/// The boat's observed geometry and status at its passenger's fluid-query phase.
/// This input affects fluid queries only while `passenger` is true. Callers own
/// updates between ticks; a batch does not predict the boat's movement.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeBoatVehicle {
    /// The boat's absolute world-space bounding box, not the passenger's box.
    pub bounding_box: NativeAabb,
    /// Boat status UNDER_WATER or UNDER_FLOWING_WATER, not ordinary IN_WATER.
    pub underwater: bool,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeWorldBounds {
    pub min_build_height: i32,
    pub max_build_height: i32,
    pub border_center_x: f64,
    pub border_center_z: f64,
    pub border_size: f64,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
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
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeBlockProperty {
    pub name: String,
    pub value: String,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
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
    pub boat_vehicle: Option<NativeBoatVehicle>,
    /// Vanilla 26.3 canSimulateMovement result for piston bounce, collision restitution and
    /// bubble column pushes. This does not redefine general tick authority.
    /// Ignored before 26.3. Absent: Player uses !clientSide || controlledByLocalInstance;
    /// generic AUTHORITATIVE_SIDE uses controlledByLocalInstance (isLocalInstanceAuthoritative,
    /// not the logical side). Other modes, including SERVER_AND_CLIENT, must supply their result explicitly.
    pub can_simulate_movement: Option<bool>,
    /// The entity is no LivingEntity: a slime block or bed returns 0.8 of the bounce (from 1.9),
    /// from 26.2 0.8F of the block's restitution. False when absent.
    pub non_living: Option<bool>,
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
    pub levitation_amplifier: Option<i32>,
    pub movement_speed: Option<f64>,
    pub jump_strength: Option<f64>,
    pub speed_effect_amplifier: Option<i32>,
    pub slowness_effect_amplifier: Option<i32>,
    pub jump_boost_amplifier: Option<i32>,
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

impl From<NativeBoatVehicle> for BoatVehicle {
    fn from(value: NativeBoatVehicle) -> Self {
        Self {
            bounding_box: value.bounding_box.into(),
            underwater: value.underwater,
        }
    }
}

impl From<BoatVehicle> for NativeBoatVehicle {
    fn from(value: BoatVehicle) -> Self {
        Self {
            bounding_box: value.bounding_box.into(),
            underwater: value.underwater,
        }
    }
}

/// A state named for a kind the player's tick does not move is refused (entity_travel).
pub(crate) fn validate_entity_type(state: &NativeSimulationState) -> Result<()> {
    state
        .entity_type
        .as_deref()
        .map_or(Ok(()), refuse_for_player_tick)
        .map_err(Error::from_reason)
}

/// An exact draw stream must be readable before the infallible state conversion takes it, and
/// cannot stand beside the fixed value it replaces.
pub(crate) fn validate_fire_draws(state: &NativeSimulationState) -> Result<()> {
    let Some(draws) = state.server_fire_increments.as_deref() else {
        return Ok(());
    };
    if state.server_fire_increment.is_some() {
        return Err(Error::from_reason(
            "give serverFireIncrements or serverFireIncrement, not both",
        ));
    }
    if ServerFireDraws::new(draws).is_none() {
        return Err(Error::from_reason(format!(
            "serverFireIncrements holds at most {} draws, each 1 or 2",
            ServerFireDraws::CAPACITY
        )));
    }
    Ok(())
}

/// A tick that needed a draw the stream did not have is not an exact replay: no result of it is
/// published.
pub(crate) fn check_fire_draws(state: &SimulationState) -> Result<()> {
    if state.server_fire_draws_exhausted {
        return Err(Error::from_reason(
            "serverFireIncrements ran out: an ignition needed another draw",
        ));
    }
    if state.server_fire_draw_missing {
        return Err(Error::from_reason(
            "a burning ServerPlayer needs its fire draw: give serverFireIncrement or serverFireIncrements",
        ));
    }
    if state.move_out_of_range {
        return Err(Error::from_reason(format!(
            "the tick's move sweeps more than {MAX_WALKED_CELLS} block cells: velocity {:?}, half width {}, height {}",
            state.velocity, state.half_width, state.height,
        )));
    }
    Ok(())
}

/// A tick of an entity that is not the player has no result when a move of it could not run.
pub(crate) fn check_move(operation: &str, out_of_range: bool) -> Result<()> {
    if out_of_range {
        return Err(Error::from_reason(format!(
            "{operation}: the tick's move sweeps more than {MAX_WALKED_CELLS} block cells"
        )));
    }
    Ok(())
}

/// What a flight of `reach` from `position` walks, a cell at a time along the ray. Beside the motion,
/// `reach` holds what the tick adds to it before the ray, and the drag it may take first. The most any of
/// them can make of the motion is what counts: a release's own order of them changes no real flight.
pub(crate) fn validate_flight(operation: &str, position: Vec3, reach: Vec3) -> Result<()> {
    let cells = reach.x.abs() + reach.y.abs() + reach.z.abs();
    let end = position + reach;
    if !(cells <= MAX_WALKED_CELLS) || [end.x, end.y, end.z].into_iter().any(out_of_range) {
        return Err(Error::from_reason(format!(
            "{operation}: the flight from {position:?} by {reach:?} walks more than {MAX_WALKED_CELLS} block cells or leaves the coordinates"
        )));
    }
    Ok(())
}

/// The most attached rockets a tick counts, each a boost it runs in turn.
pub(crate) const MAX_ATTACHED_ROCKETS: u32 = 1 << 16;

fn out_of_range(value: f64) -> bool {
    value.abs() > MAX_COORDINATE
}

/// A position a tick can run from: NaN stays what the engine makes of it (a motion that is not
/// finite leaves one before 1.21.11), and an infinite one, or one past the i32 cells, has no cell.
pub(crate) fn validate_position(operation: &str, position: Vec3) -> Result<()> {
    for (axis, value) in [("x", position.x), ("y", position.y), ("z", position.z)] {
        if out_of_range(value) {
            return Err(Error::from_reason(format!(
                "{operation}: position.{axis} is {value}, past the {MAX_COORDINATE} a block cell has"
            )));
        }
    }
    Ok(())
}

/// The size of a box a tick can walk: finite, not negative (zero is legal) and within the cells a
/// call may walk.
pub(crate) fn validate_size(operation: &str, half_width: f64, height: f64) -> Result<()> {
    for (name, value) in [("halfWidth", half_width), ("height", height)] {
        if !value.is_finite() || value < 0.0 {
            return Err(Error::from_reason(format!(
                "{operation}: {name} is {value}, it must be finite and not negative"
            )));
        }
    }
    let cells = (2.0 * half_width + 2.0) * (2.0 * half_width + 2.0) * (height + 2.0);
    if cells > MAX_WALKED_CELLS {
        return Err(Error::from_reason(format!(
            "{operation}: a box {half_width} half wide and {height} tall reaches more than {MAX_WALKED_CELLS} block cells"
        )));
    }
    Ok(())
}

/// The box kept before 1.17: ordered and within reach. NaN stays, as in validate_position.
pub(crate) fn validate_kept_box(operation: &str, kept: Option<Aabb>) -> Result<()> {
    let Some(kept) = kept else {
        return Ok(());
    };
    let (min, max) = (kept.min, kept.max);
    if min.x > max.x || min.y > max.y || min.z > max.z {
        return Err(Error::from_reason(format!(
            "{operation}: boundingBox has its minimum above its maximum: {min:?}, {max:?}"
        )));
    }
    validate_position(operation, min)?;
    validate_position(operation, max)?;
    if kept.cell_count() > MAX_WALKED_CELLS {
        return Err(Error::from_reason(format!(
            "{operation}: boundingBox reaches more than {MAX_WALKED_CELLS} block cells: {min:?}, {max:?}"
        )));
    }
    Ok(())
}

/// All a tick reads of a state before the infallible conversion takes it: nothing has changed when
/// this fails.
pub(crate) fn validate_tick_state(operation: &str, state: &NativeSimulationState) -> Result<()> {
    validate_boat_vehicle(state.boat_vehicle.as_ref())?;
    validate_fire_draws(state)?;
    validate_entity_type(state)?;
    validate_position(operation, state.position.into())?;
    validate_size(operation, state.half_width, state.height)?;
    validate_kept_box(operation, state.bounding_box.clone().map(Into::into))
}

/// What a field of a state a call takes must be. A refusal names the first field that is not, with
/// its value, after the state's kind (B209).
#[derive(Clone, Copy)]
pub(crate) enum FieldRule {
    Finite,
    NotNegative,
    Positive,
    Within(f64, f64),
}

impl FieldRule {
    fn holds(self, value: f64) -> bool {
        match self {
            FieldRule::Finite => value.is_finite(),
            FieldRule::NotNegative => value.is_finite() && value >= 0.0,
            FieldRule::Positive => value.is_finite() && value > 0.0,
            FieldRule::Within(low, high) => value.is_finite() && (low..=high).contains(&value),
        }
    }

    fn says(self) -> String {
        match self {
            FieldRule::Finite => "finite".to_owned(),
            FieldRule::NotNegative => "finite and not negative".to_owned(),
            FieldRule::Positive => "finite and above 0".to_owned(),
            FieldRule::Within(low, high) => format!("from {low} to {high}"),
        }
    }
}

/// The fields of one state checked in order: `done` refuses with the first that breaks its rule,
/// as "<kind> contains invalid values: <field> is <value>, it must be <rule>".
pub(crate) struct FieldCheck<'a> {
    kind: &'a str,
    refused: Option<String>,
}

impl<'a> FieldCheck<'a> {
    pub(crate) fn new(kind: &'a str) -> Self {
        Self { kind, refused: None }
    }

    pub(crate) fn number(mut self, name: &str, value: f64, rule: FieldRule) -> Self {
        if self.refused.is_none() && !rule.holds(value) {
            // (as JavaScript spells it, which the caller wrote: NaN and Infinity, not Rust's NaN and inf)
            let spelled = if value.is_infinite() { if value > 0.0 { "Infinity".to_owned() } else { "-Infinity".to_owned() } } else { value.to_string() };
            self.refused = Some(format!("{name} is {spelled}, it must be {}", rule.says()));
        }
        self
    }

    /// A field the caller may leave out: absent is the engine's default, and only a given value is checked.
    pub(crate) fn optional(self, name: &str, value: Option<f64>, rule: FieldRule) -> Self {
        match value {
            Some(value) => self.number(name, value, rule),
            None => self,
        }
    }

    pub(crate) fn vector(self, name: &str, value: Vec3, rule: FieldRule) -> Self {
        if self.refused.is_some() {
            return self;
        }
        self.number(&format!("{name}.x"), value.x, rule)
            .number(&format!("{name}.y"), value.y, rule)
            .number(&format!("{name}.z"), value.z, rule)
    }

    pub(crate) fn done(self) -> Result<()> {
        match self.refused {
            Some(reason) => Err(Error::from_reason(format!("{} contains invalid values: {reason}", self.kind))),
            None => Ok(()),
        }
    }
}

/// What vanilla's EntityCollisionContext tells a block about the entity asking.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy, Debug)]
pub struct NativeCollisionContext {
    /// The entity's feet: entityBottom.
    pub bottom: f64,
    /// Entity.isDescending, the sneak key.
    pub descending: bool,
    /// Entity.fallDistance.
    pub fall_distance: f64,
    /// PowderSnowBlock.canEntityWalkOnPowderSnow.
    pub walks_on_powder_snow: bool,
    /// The entity is a FallingBlockEntity; absent: false.
    pub falling_block: Option<bool>,
}

/// A world query the walks of `collect_collision_shapes` and the outline and interaction queries can
/// answer: finite, ordered and within the cells a call may walk.
fn validate_query(kind: &str, query: Aabb) -> Result<()> {
    if !valid_bounds(query) {
        return Err(Error::from_reason(format!(
            "{kind} query bounds must be finite and ordered: {:?}, {:?}",
            query.min, query.max
        )));
    }
    validate_position(kind, query.min)?;
    validate_position(kind, query.max)?;
    if query.cell_count() > MAX_WALKED_CELLS {
        return Err(Error::from_reason(format!(
            "{kind} query reaches more than {MAX_WALKED_CELLS} block cells: {:?}, {:?}",
            query.min, query.max
        )));
    }
    Ok(())
}

/// Validate the new external geometry before any caller-owned state is replaced
/// or a world query runs. The general state conversion deliberately remains
/// infallible and preserves release-specific handling of other numeric inputs.
pub(crate) fn validate_boat_vehicle(value: Option<&NativeBoatVehicle>) -> Result<()> {
    if let Some(value) = value {
        let bounds = &value.bounding_box;
        let min = Vec3::from(bounds.min);
        let max = Vec3::from(bounds.max);
        if !min.is_finite() || !max.is_finite()
            || min.x > max.x || min.y > max.y || min.z > max.z
        {
            return Err(Error::from_reason(
                "boatVehicle boundingBox must contain finite, ordered bounds",
            ));
        }
    }
    Ok(())
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
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

/// One modifier of an attribute: its amount and operation (add_value, add_multiplied_base or
/// add_multiplied_total).
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeAttributeModifier {
    pub amount: f64,
    pub operation: String,
}

/// The movement speed attribute: its base, its modifiers but the powder snow one, and that one's
/// amount if it holds it.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMovementSpeedAttribute {
    pub base: f64,
    pub modifiers: Vec<NativeAttributeModifier>,
    pub frost: Option<f64>,
}

impl TryFrom<NativeMovementSpeedAttribute> for MovementSpeedAttribute {
    type Error = Error;

    fn try_from(value: NativeMovementSpeedAttribute) -> Result<Self> {
        let modifiers = value
            .modifiers
            .into_iter()
            .map(|modifier| {
                let operation = match modifier.operation.as_str() {
                    "add_value" => AttributeOperation::AddValue,
                    "add_multiplied_base" => AttributeOperation::AddMultipliedBase,
                    "add_multiplied_total" => AttributeOperation::AddMultipliedTotal,
                    other => {
                        return Err(Error::from_reason(format!(
                            "unknown attribute operation: {other}"
                        )))
                    }
                };
                Ok(AttributeModifier { amount: modifier.amount, operation })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { base: value.base, modifiers, frost: value.frost })
    }
}

impl From<&MovementSpeedAttribute> for NativeMovementSpeedAttribute {
    fn from(value: &MovementSpeedAttribute) -> Self {
        let modifiers = value
            .modifiers
            .iter()
            .map(|modifier| NativeAttributeModifier {
                amount: modifier.amount,
                operation: match modifier.operation {
                    AttributeOperation::AddValue => "add_value",
                    AttributeOperation::AddMultipliedBase => "add_multiplied_base",
                    AttributeOperation::AddMultipliedTotal => "add_multiplied_total",
                }
                .to_string(),
            })
            .collect();
        Self { base: value.base, modifiers, frost: value.frost }
    }
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeSimulationState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub yaw: Option<f64>,
    /// The float degrees of the yaw, which moveFlying turns into its own radians before 1.9: `yaw` when
    /// not given.
    pub yaw_degrees: Option<f64>,
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
    /// The shulker boxes' pushes since the last tick, in order, which the next tick's block effects
    /// take first (from 1.21.5; the engine's own output, to hand back with the state).
    pub pending_movements: Option<Vec<NativeMovement>>,
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
    /// Vanilla 26.3 canSimulateMovement result for piston bounce, collision restitution and
    /// bubble column pushes. This does not redefine general tick authority.
    /// Ignored before 26.3. Absent: Player uses !clientSide || controlledByLocalInstance;
    /// generic AUTHORITATIVE_SIDE uses controlledByLocalInstance (isLocalInstanceAuthoritative,
    /// not the logical side). Other modes, including SERVER_AND_CLIENT, must supply their result explicitly.
    pub can_simulate_movement: Option<bool>,
    /// The entity is no LivingEntity: a slime block or bed returns 0.8 of the bounce (from 1.9),
    /// from 26.2 0.8F of the block's restitution. False when absent.
    pub non_living: Option<bool>,
    /// Entity.isPassenger: the tick is Entity.rideTick, never ordinary travel. The motion is zeroed and
    /// the tick ends on the position handed in, the seat the caller keeps with the vehicle.
    pub passenger: Option<bool>,
    /// Entity.isVehicle: it carries passengers (a server's, as vanilla seats none on a player). From
    /// 1.9 nothing pushes it (1.9.4 Entity.java:1164-1169, 26.3 :1931-1937). False when absent.
    pub is_vehicle: Option<bool>,
    /// Optional boat geometry/status for passenger fluid queries; omission clears it.
    pub boat_vehicle: Option<NativeBoatVehicle>,
    pub in_web: Option<bool>,
    pub weaving: Option<bool>,
    pub frost_walker: Option<bool>,
    pub sweet_berry_bush_immune: Option<bool>,
    pub powder_snow_immune: Option<bool>,
    pub powder_snow_walkable: Option<bool>,
    pub in_powder_snow: Option<bool>,
    pub fire_immune: Option<bool>,
    pub fire_resistance: Option<bool>,
    pub on_fire_flag: Option<bool>,
    pub in_rain: Option<bool>,
    /// A spear's stab that dealt knockback since the last tick (Player.stabAttack): a server's.
    pub stab_hit: Option<NativeStabHit>,
    pub fire_protection: Option<u32>,
    pub burning_time: Option<f64>,
    pub remaining_fire_ticks: Option<i32>,
    pub server_fire_increment: Option<i32>,
    pub server_fire_increments: Option<Vec<i32>>,
    pub ticks_frozen: Option<u32>,
    pub ticks_required_to_freeze: Option<u32>,
    pub tick_count: Option<u32>,
    pub freeze_damage: Option<f64>,
    pub contact_damage: Option<f64>,
    /// What this tick's flight into a wall hands hurt, the server's alone: 0 on a client state.
    pub wall_damage: Option<f64>,
    pub inside_wall: Option<bool>,
    pub sleeping: Option<bool>,
    pub eye_height: Option<f64>,
    pub eye_in_water: Option<bool>,
    pub eye_in_lava: Option<bool>,
    pub slow_falling: Option<bool>,
    pub levitation: Option<bool>,
    pub levitation_amplifier: Option<i32>,
    pub fall_distance: Option<f64>,
    pub safe_fall_distance: Option<f64>,
    pub fall_damage_immune: Option<bool>,
    pub client_side: Option<bool>,
    /// The height the pose update leaves at the end of the player's tick (Player.tick): what ticks
    /// after the player meets that box. Absent: the height the tick ran with.
    pub height_after_tick: Option<f64>,
    /// The kind the state is of: a tick refuses any but "player" (entityTravelSupport), so a kind
    /// with another path or none does not run on the player's equations. Absent: not named.
    pub entity_type: Option<String>,
    pub invulnerable: Option<bool>,
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
    /// Its health is 0 (LivingEntity.isAlive is false): from 1.11 no entity pushes it.
    pub dead: Option<bool>,
    pub gravity: Option<f64>,
    pub step_height: Option<f64>,
    pub movement_speed: Option<f64>,
    pub movement_speed_value: Option<f64>,
    pub jump_strength: Option<f64>,
    pub speed_effect_amplifier: Option<i32>,
    pub slowness_effect_amplifier: Option<i32>,
    pub jump_boost_amplifier: Option<i32>,
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
    /// One raw rocket impulse, added once at the start of every tick, before the player's tick
    /// reads its motion (the place FireworkRocketEntity.tick has); between the scene's rockets
    /// before and after the player. It stays on the state for the next tick: the caller clears it.
    pub firework_boost: Option<NativeVec3>,
    pub stuck_speed_multiplier: Option<NativeVec3>,
    pub previous_position: Option<NativeVec3>,
    pub bounding_box: Option<NativeAabb>,
    /// Entity.mainSupportingBlockPos as the last move left it, when `supporting_block_kept`
    /// (absent: no block); without it the engine takes it from the position and the last move.
    pub supporting_block: Option<NativeBlockPosition>,
    pub supporting_block_kept: Option<bool>,
    /// Entity.onGroundNoBlocks: the last move left the entity on the ground with no block under it.
    pub on_ground_no_blocks: Option<bool>,
    /// Entity.isOutsideBorder before 1.14: the last collision found the entity outside the world
    /// border.
    pub outside_border: Option<bool>,
    /// The movement speed attribute's parts: with them the tick applies its frost modifier and
    /// movement_speed_value is the value after it. 1.17 to 1.21.4 on a client.
    pub movement_speed_attribute: Option<NativeMovementSpeedAttribute>,
    pub nearby_entities: Option<Vec<NativeNearbyEntity>>,
    pub firework_rockets: Option<u32>,
    pub firework_rockets_before_player: Option<u32>,
    /// How many more client ticks each attached rocket boosts, those before the player first
    /// (simulateVersionedTicks): FireworkRocketEntity.tick boosts, then life++, and the rocket
    /// ends once life > lifetime. Absent: the rockets boost every tick.
    pub firework_rocket_ticks: Option<Vec<u32>>,
    pub moving_pistons: Option<Vec<NativeMovingPiston>>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeMovingPiston {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// The piston's facing as Direction.from3DDataValue: down, up, north, south, west, east.
    pub facing: u8,
    pub extending: bool,
    pub source: bool,
    pub progress: f64,
    /// The collision boxes of the block it moves in its cell: they push from 1.11, and on 1.8
    /// those of a block it pushes.
    pub shapes: Vec<NativeAabb>,
    /// `shapes` once it has gone on this tick, for the pistons that tick after it (absent: the same).
    pub next_shapes: Option<Vec<NativeAabb>>,
    /// The outline boxes of a block it pushes in its cell, which push on 1.9 - 1.10 (absent or
    /// empty: the full cube).
    pub outline_shapes: Option<Vec<NativeAabb>>,
    /// The block it moves is a slime block, which bounces the player (absent: false).
    pub bounces: Option<bool>,
    /// The block it moves is a honey block, which carries the player on it (absent: false).
    pub sticky_for_entities: Option<bool>,
}

impl TryFrom<NativeMovingPiston> for MovingPiston {
    type Error = Error;

    fn try_from(value: NativeMovingPiston) -> Result<Self> {
        FieldCheck::new("moving piston")
            .number("facing", f64::from(value.facing), FieldRule::Within(0.0, 5.0))
            .number("progress", value.progress, FieldRule::Finite)
            .done()?;
        Ok(Self {
            position: (value.x, value.y, value.z),
            facing: value.facing,
            extending: value.extending,
            source: value.source,
            progress: value.progress as f32,
            shapes: value.shapes.into_iter().map(Into::into).collect(),
            next_shapes: value
                .next_shapes
                .map(|shapes| shapes.into_iter().map(Into::into).collect()),
            outline_shapes: value
                .outline_shapes
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            bounces: value.bounces.unwrap_or(false),
            sticky_for_entities: value.sticky_for_entities.unwrap_or(false),
        })
    }
}

/// The push one nearby entity took back from the local player's tick: its index among the state's
/// nearbyEntities and the velocity it was pushed by on x and z.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeNearbyEntityPush {
    pub index: u32,
    pub x: f64,
    pub z: f64,
}

/// Everything simulateVersionedTickOutcome's tick writes: the state after it, carrying the scene
/// the next tick reads (its nearbyEntities as they came, the moving pistons moved on and, before
/// 1.11, without those that ended, the attached rockets with this tick counted off), the pushes
/// the nearby entities took back, whether the spin attack ended, and how far the moving pistons
/// moved the player (Entity.pistonDeltas).
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeTickOutcome {
    pub state: NativeSimulationState,
    pub entity_pushes: Vec<NativeNearbyEntityPush>,
    pub auto_spin_attack_ended: bool,
    pub piston_deltas: NativeVec3,
}

/// One point of a traced tick: its phase name (the recorder's schema/phases.ts), how many times the tick
/// had passed that point before, and the state there.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeTickPhase {
    pub name: String,
    pub occurrence: u32,
    pub state: NativeSimulationState,
    /// LivingEntity's xxa, yya and zza at beforeTravel.
    pub travel_vector: Option<NativeVec3>,
}

/// simulateVersionedTick's state, and the points the tick passed on the way to it.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeTracedTick {
    pub state: NativeSimulationState,
    pub phases: Vec<NativeTickPhase>,
}

impl From<MovingPiston> for NativeMovingPiston {
    fn from(value: MovingPiston) -> Self {
        Self {
            x: value.position.0,
            y: value.position.1,
            z: value.position.2,
            facing: value.facing,
            extending: value.extending,
            source: value.source,
            progress: f64::from(value.progress),
            shapes: value.shapes.into_iter().map(Into::into).collect(),
            next_shapes: value
                .next_shapes
                .map(|shapes| shapes.into_iter().map(Into::into).collect()),
            outline_shapes: (!value.outline_shapes.is_empty())
                .then(|| value.outline_shapes.into_iter().map(Into::into).collect()),
            bounces: Some(value.bounces),
            sticky_for_entities: Some(value.sticky_for_entities),
        }
    }
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativePistonPushResult {
    pub state: NativeSimulationState,
    /// How far the pistons of the tick have moved the player on each axis (Entity.pistonDeltas).
    pub piston_deltas: NativeVec3,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeNearbyEntity {
    /// "living", "boat", "shulker", "happy_ghast", "happy_ghast_stays_still" (on its still
    /// timeout), "still_living" (a living entity that neither pushes nor collides, the armor
    /// stand), "minecart" or "attached_rocket" (a firework rocket attached to the player, boosting
    /// it in its turn of the tick) or "spectator" (a spectating player, which the riptide spin's
    /// sweep leaves out); other entities neither push nor collide.
    pub kind: String,
    pub position: NativeVec3,
    pub bounding_box: NativeAabb,
    pub ticks_before_player: Option<bool>,
    pub tick_position: Option<NativeVec3>,
    pub tick_bounding_box: Option<NativeAabb>,
    /// False when the team collision rules keep it from pushing the player
    /// (EntitySelector.pushableBy); true by default.
    pub team_pushes: Option<bool>,
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
            team_pushes: value.team_pushes.unwrap_or(true),
            spectator: value.kind == "spectator",
        }
    }
}

/// One side of simulateVersionedEntityPush: the left one is the entity whose Entity.push(Entity)
/// runs (`this`), the right one its argument.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeEntityPushState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub no_physics: bool,
    /// It has passengers (Entity.isVehicle): it takes no push.
    pub vehicle: bool,
    /// Entity.isPushable: from 1.19 a side that is not takes no push.
    pub pushable: bool,
    /// Entity.pushthrough before 1.17: the left side's scales both pushes; the right side's is
    /// unused.
    pub pushthrough: f64,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeEntityPushResult {
    pub left_velocity: NativeVec3,
    pub right_velocity: NativeVec3,
}

/// A living entity of simulateVersionedEntityTicks.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeVersionedEntityState {
    pub simulation: NativeSimulationState,
    pub no_physics: bool,
    /// Its id, which other entities' vehicleId name.
    pub id: Option<u32>,
    /// The id of the entity it rides.
    pub vehicle_id: Option<u32>,
    pub pushable: bool,
    pub pushthrough: f64,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativePoseRefreshResult {
    pub state: NativeSimulationState,
    pub pose: u8,
    pub changed: bool,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
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
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeGeneratedShapeBox {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

/// A channel of a block state as the release's own code answers it (scripts/java-oracle/ShapeDump.java).
/// `status` is `exact` (the boxes are the answer, none being an answer too), `contextual` (the answer also
/// needs what `depends_on` lists, so there are no boxes) or `absent` (the release has no such method).
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeGeneratedStateShape {
    pub status: String,
    pub depends_on: Vec<String>,
    pub boxes: Vec<NativeGeneratedShapeBox>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeGeneratedShapeProvenance {
    pub source_sha256: Option<String>,
    pub has_method: bool,
    pub state_dependent: bool,
    pub property_dependencies: Vec<String>,
    pub candidate_box_count: i64,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeGeneratedShapePredicate {
    pub property: String,
    pub operator: String,
    pub value: String,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
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
    /// Blocks.WATER itself, whose speed factor Entity.getBlockSpeedFactor keeps without reading
    /// the block below (not a waterlogged block or a water plant).
    pub water_block: Option<bool>,
    pub fall_damage_resetting: Option<bool>,
    /// The end gateway and end portal: full cubes for a player's fall-damage-resetting clip from
    /// 1.21.9.
    pub player_fall_damage_resetting: Option<bool>,
    pub bounce_restitution: Option<f64>,
    pub suppresses_bounce: Option<bool>,
    pub suffocating: Option<bool>,
    pub in_wall: Option<bool>,
    pub bottom_face_sturdy: Option<bool>,
    /// Block.getExplosionResistance of the block, absent for air: what a blast's rays lose in it.
    pub explosion_resistance: Option<f64>,
    pub remove: Option<bool>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeVersionCapabilities {
    pub fluid_physics: bool,
    pub honey_blocks: bool,
    pub fall_flying: bool,
    pub water_movement_efficiency: bool,
    pub minor_horizontal_collision: bool,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeBoatState {
    /// The entities around the boat, it and those sharing it left out: its move collides with
    /// those vanilla says (vehicle_entity_colliders).
    pub nearby_entities: Option<Vec<NativeNearbyEntity>>,
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
    /// The boat ticks on the server, which runs the column's timer (AbstractBoat.tickBubbleColumn).
    pub server_side: Option<bool>,
    pub player_passenger: Option<bool>,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub on_ground: Option<bool>,
    pub yaw_degrees: Option<f64>,
    pub land_friction: Option<f64>,
    pub last_vertical_velocity: Option<f64>,
    /// Entity.fallDistance of the boat (absent: 0).
    pub fall_distance: Option<f64>,
    pub bounding_box: Option<NativeAabb>,
    pub speed_multiplier: Option<f64>,
    pub rider_yaw: Option<f64>,
    pub rider_forward: Option<f64>,
    pub rider_strafe: Option<f64>,
    /// Entity.stuckSpeedMultiplier, set by a cobweb or powder snow the boat was inside for its
    /// next move.
    pub stuck_speed_multiplier: Option<NativeVec3>,
    /// The boat's dimension is ultrawarm (the nether): lava pushes it at 0.007 (FAST_LAVA).
    pub ultra_warm: Option<bool>,
    /// The boat's synced NoGravity flag (Entity.isNoGravity, 1.10+): floatBoat takes no gravity.
    pub no_gravity: Option<bool>,
    /// Boat.paddlePositions (1.9+): the left and right paddles' rowing phases.
    pub paddle_phases: Option<Vec<f64>>,
    /// Entity.isSilent: a silent boat plays no paddle sound (which takes its status again).
    pub silent: Option<bool>,
    /// AbstractBoat.lerpSteps: the steps left of the interpolation a position packet started,
    /// which 1.21.2 to 1.21.4 run on a boat the client controls too.
    pub lerp_steps: Option<i32>,
    /// AbstractBoat.lerpX, lerpY and lerpZ: where the interpolation goes.
    pub lerp_position: Option<NativeVec3>,
    /// AbstractBoat.lerpYRot: the yaw it turns to, in degrees.
    pub lerp_y_rot: Option<f64>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeHorseState {
    /// The entities around the mob, it and those sharing it left out: its move collides with
    /// those vanilla says (vehicle_entity_colliders).
    pub nearby_entities: Option<Vec<NativeNearbyEntity>>,
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub yaw: f64,
    pub forward: f64,
    pub strafe: f64,
    pub jump_requested: bool,
    pub on_ground: bool,
    pub in_water: bool,
    /// The MOVEMENT_SPEED attribute, 0 a legal one; the mob's base when not given.
    pub movement_speed: Option<f64>,
    /// The JUMP_STRENGTH attribute, 0 a legal one; the mob's base when not given.
    pub jump_strength: Option<f64>,
    /// A ridden mob's measured size in its pose (SCALE, age); its type's when not given.
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
    /// A ridden mob's getFluidHeight(LAVA) > 0 of its last base tick (`in_lava` when not given).
    pub lava_height_positive: Option<bool>,
    /// The world-less horse's getFluidHeight(WATER) while `in_water`, what
    /// floatInWaterWhileRidden lifts it by (0 when not given; a ridden mob in its world measures
    /// its own).
    pub water_height: Option<f64>,
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
    pub suffocating_speed: Option<f64>,
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
    /// A ridden mob's dimension is ultrawarm (the nether): lava pushes it at 0.007 (FAST_LAVA).
    pub ultra_warm: Option<bool>,
    /// The amplifier of a ridden mob's Jump Boost, which its riding jump adds 0.1F a level to.
    pub jump_boost_amplifier: Option<i32>,
    /// The mob's synced NoGravity flag (Entity.isNoGravity, 1.10+): its travel takes no gravity.
    pub no_gravity: Option<bool>,
    /// Entity.stuckSpeedMultiplier, for the next move of a ridden mob.
    pub stuck_speed_multiplier: Option<NativeVec3>,
    pub slow_falling: Option<bool>,
    pub levitation_amplifier: Option<i32>,
    pub dolphins_grace: Option<bool>,
    /// Weaving (1.20.5+), which changes what a cobweb leaves the ridden mob.
    pub weaving: Option<bool>,
    /// The ridden mob's GRAVITY attribute (1.20.5+), STEP_HEIGHT (1.20.5+),
    /// WATER_MOVEMENT_EFFICIENCY (1.21+), AIR_DRAG_MODIFIER and FRICTION_MODIFIER (26.2+).
    pub gravity: Option<f64>,
    pub step_height: Option<f64>,
    pub water_movement_efficiency: Option<f64>,
    pub air_drag_modifier: Option<f64>,
    pub friction_modifier: Option<f64>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeProjectileState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub gravity: Option<f64>,
    pub air_drag: Option<f64>,
    pub power: Option<NativeVec3>,
    /// AbstractHurtingProjectile.accelerationPower (1.21+; when absent the kind's, 0.1 and 0 for a wind charge).
    pub acceleration_power: Option<f64>,
    /// A dangerous (blue) wither skull, whose air inertia is 0.73F.
    pub dangerous: Option<bool>,
    pub in_water: Option<bool>,
    /// The dimension is ultrawarm: lava pushes at 0.007.
    pub ultra_warm: Option<bool>,
    pub first_tick: Option<bool>,
    pub kind: Option<String>,
    pub horizontal_collision: Option<bool>,
    pub on_ground: Option<bool>,
    pub shot_at_angle: Option<bool>,
    /// A fishing hook in its BOBBING state.
    pub bobbing: Option<bool>,
    /// The bobbing fishing hook's random.nextFloat() for this tick.
    pub bob_random: Option<f64>,
    /// Entity.noPhysics: Entity.move passes through blocks and a firework rocket acts on no hit.
    pub no_physics: Option<bool>,
    /// The box a rocket or hook keeps before 1.17 (absent: from the position).
    pub bounding_box: Option<NativeAabb>,
    /// Entity.stuckSpeedMultiplier a cobweb or powder snow left on a hook or rocket, which its next
    /// move takes (absent: none).
    pub stuck_speed_multiplier: Option<NativeVec3>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeProjectileTickResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub block_hit: bool,
    /// Entity.isInWater after the tick (the batch's own, from the world).
    pub in_water: bool,
    pub first_tick: bool,
    pub horizontal_collision: bool,
    pub on_ground: bool,
    pub bobbing: bool,
    pub discarded: bool,
    /// The box a rocket or hook keeps before 1.17.
    pub bounding_box: Option<NativeAabb>,
    /// Entity.stuckSpeedMultiplier the tick left on a hook or rocket, for the next tick's state.
    pub stuck_speed_multiplier: NativeVec3,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
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
    /// The state id of the block the arrow hit.
    pub last_block_id: Option<u32>,
    /// The block the arrow hit, whose state the in-ground test reads until 1.13.2.
    pub last_block_position: Option<NativeBlockPosition>,
    /// AbstractArrow.isNoPhysics, from 1.13.
    pub no_physics: Option<bool>,
    /// The dimension is ultrawarm: lava pushes at 0.007.
    pub ultra_warm: Option<bool>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeBlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeArrowTickResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub block_hit: bool,
    pub in_water: bool,
    pub in_ground: bool,
    pub discarded: bool,
    pub life: u16,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub started_falling: bool,
    pub last_block_id: Option<u32>,
    pub last_block_position: Option<NativeBlockPosition>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeTridentState {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub gravity: Option<f64>,
    pub air_drag: Option<f64>,
    pub water_inertia: Option<f64>,
    pub in_water: Option<bool>,
    pub dealt_damage: Option<bool>,
    /// AbstractArrow's embedded state, as in `NativeArrowState`.
    pub in_ground: Option<bool>,
    pub life: Option<u16>,
    pub in_ground_time: Option<u32>,
    pub shake_time: Option<u8>,
    pub fall_impulse: Option<NativeVec3>,
    pub last_block_id: Option<u32>,
    pub last_block_position: Option<NativeBlockPosition>,
    pub no_physics: Option<bool>,
    pub ultra_warm: Option<bool>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeTridentTickResult {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub block_hit: bool,
    pub in_water: bool,
    pub in_ground: bool,
    pub discarded: bool,
    pub life: u16,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub started_falling: bool,
    pub dealt_damage: bool,
    pub last_block_id: Option<u32>,
    pub last_block_position: Option<NativeBlockPosition>,
}

impl From<NativeSimulationState> for SimulationState {
    fn from(value: NativeSimulationState) -> Self {
        Self {
            position: value.position.into(),
            velocity: value.velocity.into(),
            yaw: value.yaw.unwrap_or(0.0),
            yaw_degrees: value.yaw_degrees.map(|degrees| degrees as f32),
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
            riptide_lift: Vec3::ZERO,
            pending_movements: {
                let mut held = crate::simulation::HeldMovements::default();
                for movement in value.pending_movements.unwrap_or_default() {
                    held.push(movement.into());
                }
                held
            },
            riptide_released: false,
            riptide_lift_movement: None,
            sprinting: value.sprinting.unwrap_or(false),
            was_sprinting: value.was_sprinting.unwrap_or(false),
            sprint_modifier: value.sprint_modifier,
            in_water: value.in_water.unwrap_or(false),
            in_lava: value.in_lava.unwrap_or(false),
            ultra_warm: value.ultra_warm.unwrap_or(false),
            is_player: value.is_player.unwrap_or(false),
            controlled_by_local_instance: value.controlled_by_local_instance.unwrap_or(false),
            can_simulate_movement: value.can_simulate_movement,
            non_living: value.non_living.unwrap_or(false),
            passenger: value.passenger.unwrap_or(false),
            is_vehicle: value.is_vehicle.unwrap_or(false),
            boat_vehicle: value.boat_vehicle.map(Into::into),
            in_web: value.in_web.unwrap_or(false),
            weaving: value.weaving.unwrap_or(false),
            frost_walker: value.frost_walker.unwrap_or(false),
            sweet_berry_bush_immune: value.sweet_berry_bush_immune.unwrap_or(false),
            web_immune: false,
            stuck_immune: false,
            boat: false,
            above_bubble_column: None,
            powder_snow_immune: value.powder_snow_immune.unwrap_or(false),
            powder_snow_walkable: value.powder_snow_walkable.unwrap_or(false),
            in_powder_snow: value.in_powder_snow.unwrap_or(false),
            fire_immune: value.fire_immune.unwrap_or(false),
            fire_resistance: value.fire_resistance.unwrap_or(false),
            on_fire_flag: value.on_fire_flag.unwrap_or(false),
            in_rain: value.in_rain.unwrap_or(false),
            stab_hit: value.stab_hit.map(|hit| hit.knockback.unwrap_or(0.0) as f32),
            fire_protection: value.fire_protection.unwrap_or(0),
            burning_time: value.burning_time,
            remaining_fire_ticks: value.remaining_fire_ticks,
            server_fire_increment: value.server_fire_increment,
            server_fire_draws: value
                .server_fire_increments
                .as_deref()
                .and_then(ServerFireDraws::new),
            server_fire_draws_exhausted: false,
            server_fire_draw_missing: false,
            move_out_of_range: false,
            ticks_frozen: value.ticks_frozen.unwrap_or(0),
            ticks_required_to_freeze: value.ticks_required_to_freeze.unwrap_or(140),
            tick_count: value.tick_count.unwrap_or(0),
            freeze_damage: value.freeze_damage.unwrap_or(0.0),
            contact_damage: value.contact_damage.unwrap_or(0.0),
            wall_damage: value.wall_damage.unwrap_or(0.0),
            inside_wall: value.inside_wall.unwrap_or(false),
            sleeping: value.sleeping.unwrap_or(false),
            eye_height: value.eye_height,
            eye_in_water: value.eye_in_water.unwrap_or(false),
            eye_in_lava: value.eye_in_lava.unwrap_or(false),
            slow_falling: value.slow_falling.unwrap_or(false),
            levitation: value.levitation.unwrap_or(false),
            levitation_amplifier: value.levitation_amplifier,
            fall_distance: value.fall_distance.unwrap_or(0.0),
            // the SAFE_FALL_DISTANCE attribute's default, and the 3.0F of the fall damage before it
            safe_fall_distance: value.safe_fall_distance.unwrap_or(3.0),
            fall_damage_immune: value.fall_damage_immune.unwrap_or(false),
            client_side: value.client_side.unwrap_or(false),
            height_after_tick: value.height_after_tick,
            invulnerable: value.invulnerable.unwrap_or(false),
            fall_damage_multiplier: Some(value.fall_damage_multiplier.unwrap_or(1.0)),
            sneaking: value.sneaking.unwrap_or(false),
            moving_slowly: value.moving_slowly,
            stepping_carefully: value.stepping_carefully.unwrap_or(false),
            using_item: value.using_item.unwrap_or(false),
            item_use_speed_multiplier: value.item_use_speed_multiplier.map(|speed| speed as f32),
            swimming: value.swimming.unwrap_or(false),
            flying: value.flying.unwrap_or(false),
            // Abilities.flyingSpeed starts at 0.05F (1.21.11 Abilities.java:19)
            flying_speed: value.flying_speed.unwrap_or(f64::from(0.05_f32)),
            climbing: value.climbing.unwrap_or(false),
            no_physics: value.no_physics.unwrap_or(false),
            no_gravity: value.no_gravity.unwrap_or(false),
            removed: value.removed.unwrap_or(false),
            dead: value.dead.unwrap_or(false),
            gravity: value.gravity,
            step_height: value.step_height,
            movement_speed: value.movement_speed,
            movement_speed_value: value.movement_speed_value,
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
            head_look: None,
            firework_boost: value.firework_boost.map(Into::into).unwrap_or(Vec3::ZERO),
            stuck_speed_multiplier: value
                .stuck_speed_multiplier
                .map(Into::into)
                .unwrap_or(Vec3::ZERO),
            move_slowed: false,
            previous_position: value.previous_position.map(Into::into),
            bounding_box: value.bounding_box.map(Into::into),
            supporting_block: value
                .supporting_block
                .map(|position| (position.x, position.y, position.z)),
            supporting_block_kept: value.supporting_block_kept.unwrap_or(false),
            on_ground_no_blocks: value.on_ground_no_blocks.unwrap_or(false),
            outside_border: value.outside_border.unwrap_or(false),
        }
    }
}

impl From<SimulationState> for NativeSimulationState {
    fn from(value: SimulationState) -> Self {
        Self {
            position: value.position.into(),
            velocity: value.velocity.into(),
            yaw: Some(value.yaw),
            yaw_degrees: value.yaw_degrees.map(f64::from),
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
            pending_movements: Some(value.pending_movements.as_slice().into_iter().map(Into::into).collect()),
            sprinting: Some(value.sprinting),
            was_sprinting: Some(value.was_sprinting),
            sprint_modifier: value.sprint_modifier,
            in_water: Some(value.in_water),
            in_lava: Some(value.in_lava),
            ultra_warm: Some(value.ultra_warm),
            is_player: Some(value.is_player),
            controlled_by_local_instance: Some(value.controlled_by_local_instance),
            can_simulate_movement: value.can_simulate_movement,
            non_living: Some(value.non_living),
            passenger: Some(value.passenger),
            is_vehicle: Some(value.is_vehicle),
            boat_vehicle: value.boat_vehicle.map(Into::into),
            in_web: Some(value.in_web),
            weaving: Some(value.weaving),
            frost_walker: Some(value.frost_walker),
            sweet_berry_bush_immune: Some(value.sweet_berry_bush_immune),
            powder_snow_immune: Some(value.powder_snow_immune),
            powder_snow_walkable: Some(value.powder_snow_walkable),
            in_powder_snow: Some(value.in_powder_snow),
            fire_immune: Some(value.fire_immune),
            fire_resistance: Some(value.fire_resistance),
            on_fire_flag: Some(value.on_fire_flag),
            in_rain: Some(value.in_rain),
            stab_hit: value.stab_hit.map(|knockback| NativeStabHit {
                knockback: Some(f64::from(knockback)),
            }),
            fire_protection: Some(value.fire_protection),
            burning_time: value.burning_time,
            remaining_fire_ticks: value.remaining_fire_ticks,
            server_fire_increment: value.server_fire_increment,
            server_fire_increments: value.server_fire_draws.map(ServerFireDraws::to_vec),
            ticks_frozen: Some(value.ticks_frozen),
            ticks_required_to_freeze: Some(value.ticks_required_to_freeze),
            tick_count: Some(value.tick_count),
            freeze_damage: Some(value.freeze_damage),
            contact_damage: Some(value.contact_damage),
            wall_damage: Some(value.wall_damage),
            inside_wall: Some(value.inside_wall),
            sleeping: Some(value.sleeping),
            eye_height: value.eye_height,
            eye_in_water: Some(value.eye_in_water),
            eye_in_lava: Some(value.eye_in_lava),
            slow_falling: Some(value.slow_falling),
            levitation: Some(value.levitation),
            levitation_amplifier: value.levitation_amplifier,
            fall_distance: Some(value.fall_distance),
            safe_fall_distance: Some(value.safe_fall_distance),
            fall_damage_immune: Some(value.fall_damage_immune),
            client_side: Some(value.client_side),
            height_after_tick: None,
            entity_type: None,
            invulnerable: Some(value.invulnerable),
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
            dead: Some(value.dead),
            gravity: value.gravity,
            step_height: value.step_height,
            movement_speed: value.movement_speed,
            movement_speed_value: value.movement_speed_value,
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
            supporting_block: value.supporting_block.map(|(x, y, z)| NativeBlockPosition {
                x,
                y,
                z,
            }),
            supporting_block_kept: Some(value.supporting_block_kept),
            on_ground_no_blocks: Some(value.on_ground_no_blocks),
            outside_border: Some(value.outside_border),
            movement_speed_attribute: None,
            nearby_entities: None,
            firework_rockets: None,
            firework_rockets_before_player: None,
            firework_rocket_ticks: None,
            moving_pistons: None,
        }
    }
}

#[napi]
pub struct NativeWorld {
    pub(crate) inner: WorldSnapshot,
    /// Which world this is, for what keeps state of the world it last ran in. Never reused.
    pub(crate) identity: u64,
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

/// A world from the bytes of NativeWorld.checkpoint. It is a new world: a NativePlayer that ticked
/// in another takes it through setState.
#[napi]
pub fn native_world_from_checkpoint(bytes: napi::bindgen_prelude::Buffer) -> Result<NativeWorld> {
    let mut world = NativeWorld::new();
    world.inner = WorldSnapshot::decode(&bytes).map_err(Error::from_reason)?;
    Ok(world)
}

#[napi]
impl NativeWorld {
    #[napi(constructor)]
    pub fn new() -> Self {
        static WORLDS: AtomicU64 = AtomicU64::new(0);
        Self {
            inner: WorldSnapshot::new(),
            identity: WORLDS.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The whole world as bytes, for nativeWorldFromCheckpoint: equal worlds give equal bytes.
    #[napi]
    pub fn checkpoint(&self) -> napi::bindgen_prelude::Buffer {
        self.inner.encode().into()
    }

    /// What names this world's content in a checkpoint.
    #[napi]
    pub fn content_digest(&self) -> String {
        checkpoint_digest(&self.inner.encode())
    }

    /// The release whose client this world is: what it reads in a chunk it does not hold of the
    /// shared empty chunk, which keeps what is written there (WorldSnapshot::set_release; 1.8 its
    /// blocks, 1.13 their fluids, other releases nothing). A world given none reads nothing there.
    #[napi]
    pub fn set_release(&mut self, version: String) -> Result<()> {
        self.inner.set_release(registered_release(&version)?);
        Ok(())
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
        border_absolute_max_size: Option<i32>,
    ) -> Result<()> {
        if min_build_height >= max_build_height {
            return Err(Error::from_reason(format!(
                "setWorldBounds: minBuildHeight {min_build_height} must be below maxBuildHeight {max_build_height}"
            )));
        }
        for (name, value) in [("borderCenterX", border_center_x), ("borderCenterZ", border_center_z), ("borderSize", border_size)] {
            if !value.is_finite() {
                return Err(Error::from_reason(format!("setWorldBounds: {name} is {value}, it must be finite")));
            }
        }
        if border_size < 0.0 {
            return Err(Error::from_reason(format!("setWorldBounds: borderSize is {border_size}, it must not be negative")));
        }
        self.inner.set_world_bounds(
            min_build_height,
            max_build_height,
            border_center_x,
            border_center_z,
            border_size,
        );
        // (WorldBorder.absoluteMaxSize, 29999984 when none is given)
        self.inner
            .set_world_border_absolute_max_size(border_absolute_max_size.unwrap_or(29_999_984));
        Ok(())
    }

    /// MovingPistonBlock.getCollisionShape: the block at the piston's cell takes the shape its block
    /// entity gives at its progress, which a move before the block entity's tick meets.
    /// `base_shapes`: the extended base a source piston leaves as it retracts. Returns the boxes the
    /// cell collides with, relative to it (free to leave it), for the caller's own collision queries.
    #[napi]
    pub fn set_moving_block(
        &mut self,
        version: String,
        piston: NativeMovingPiston,
        base_shapes: Option<Vec<NativeAabb>>,
    ) -> Result<Vec<NativeAabb>> {
        let release = world_border_release(Some(version))?;
        let piston = MovingPiston::try_from(piston)?;
        let base: Vec<Aabb> = base_shapes.unwrap_or_default().into_iter().map(Into::into).collect();
        let boxes = set_moving_block(&mut self.inner, &piston, &base, release);
        Ok(boxes
            .into_iter()
            .map(|bounds| NativeAabb {
                min: bounds.min.into(),
                max: bounds.max.into(),
            })
            .collect())
    }

    /// The moving block's block entity ended: the block has no shape until it is written anew.
    #[napi]
    pub fn clear_moving_block(&mut self, x: i32, y: i32, z: i32) {
        self.inner.clear_moving_block((x, y, z));
    }

    /// WorldBorder.isWithinBounds(x, z) of the release (the newest one when none is given).
    #[napi]
    pub fn is_inside_world_border(&self, x: f64, z: f64, version: Option<String>) -> Result<bool> {
        Ok(self
            .inner
            .is_inside_world_border(Vec3::new(x, 0.0, z), world_border_release(version)?))
    }

    /// CollisionGetter.noBorderCollision of the release from 1.14 (the newest one when none is
    /// given): whether the world border leaves `query` free for an entity at x, z with box
    /// `bounds`. A release before 1.14 has no such term: its border cells are in the block query.
    #[napi]
    pub fn no_border_collision(
        &self,
        min: NativeVec3,
        max: NativeVec3,
        bounds_min: NativeVec3,
        bounds_max: NativeVec3,
        x: f64,
        z: f64,
        version: Option<String>,
    ) -> Result<bool> {
        Ok(self.inner.no_border_collision_for_entity(
            Aabb::new(min.into(), max.into()),
            Vec3::new(x, 0.0, z),
            Aabb::new(bounds_min.into(), bounds_max.into()),
            world_border_release(version)?,
        ))
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

    /// WorldBorder.clampVec3ToBound from 1.21.2, clampToBounds (a block corner) before, of the
    /// release (the newest one when none is given).
    #[napi]
    pub fn clamp_to_world_border(
        &self,
        position: NativeVec3,
        version: Option<String>,
    ) -> Result<NativeVec3> {
        Ok(self
            .inner
            .clamp_to_world_border(position.into(), world_border_release(version)?)
            .into())
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
        let version = registered_release(&version)?;
        let identity =
            crate::world::VersionedBlockStateId::new(version, id).map_err(Error::from_reason)?;
        if !self.inner.contains_block((x, y, z)) {
            return Err(Error::from_reason(
                "block state IDs require an explicitly stored block state",
            ));
        }
        self.inner.set_block_state_id((x, y, z), identity);
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
        if !valid_block_shape(bounds, shape.slipperiness) {
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
                water_block: false,
                fall_damage_resetting: shape.fall_damage_resetting.unwrap_or(false),
                player_fall_damage_resetting: false,
                bounce_restitution: validated_bounce(shape.bounce_restitution)?,
                suppresses_bounce: shape.suppresses_bounce.unwrap_or(false),
                suffocating: None,
                in_wall: None,
                bottom_face_sturdy: None,
                explosion_resistance: None,
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
        refuse_collision_fields("an outline", &shape)?;
        let bounds = crate::math::Aabb::new(
            Vec3::new(shape.min_x, shape.min_y, shape.min_z),
            Vec3::new(shape.max_x, shape.max_y, shape.max_z),
        );
        if !valid_block_shape(bounds, shape.slipperiness) {
            return Err(Error::from_reason(
                "outline shape bounds must be finite, ordered, and reach at most one block out of their own",
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
        let bounce_restitution = validated_bounce(shapes.first().and_then(|shape| shape.bounce_restitution))?;
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
            if !valid_block_shape(bounds, shape.slipperiness) {
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
        // a batch applies whole or not at all: nothing below fails after this
        for (index, update) in updates.iter().enumerate() {
            validate_block_update(update).map_err(|error| {
                Error::from_reason(format!(
                    "applyBlockUpdates: update {index} at ({}, {}, {}): {}",
                    update.x, update.y, update.z, error.reason
                ))
            })?;
        }
        let mut grouped = std::collections::HashMap::new();
        for update in &updates {
            let position = (update.x, update.y, update.z);
            if update.remove.unwrap_or(false) {
                grouped.remove(&position);
                self.inner.remove_block(position);
                continue;
            }
            // A block's geometry is fixed per state: an update of its metadata alone keeps every
            // box the block (or an earlier update of it in this batch) has.
            let entry = if metadata_only(update) {
                grouped.entry(position).or_insert_with(|| {
                    let (shapes, outline_shapes, interaction_shapes) =
                        self.inner.block_geometry(position);
                    let state = self.inner.block(position);
                    (state, shapes, outline_shapes, interaction_shapes, None, None, None, false)
                })
            } else {
                let state = block_update_state(update)?;
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
                entry
            };
            let state = entry.0;
            // the updates of a cell in a batch are the boxes of one block: one that says nothing of these
            // leaves what an earlier one said
            if let Some(properties) = update.properties.as_ref() {
                entry.4 = Some(
                    properties
                        .iter()
                        .map(|property| (property.name.clone(), property.value.clone()))
                        .collect(),
                );
            }
            if let Some(value) = update.state_version.as_deref() {
                entry.5 = Some(
                    crate::world::VersionedBlockStateId::new(
                        registered_release(value)?,
                        update.id.unwrap_or(state.id),
                    )
                    .map_err(Error::from_reason)?,
                );
            }
            if update.block_entity.is_some() {
                entry.6 = update.block_entity.clone();
            }
            entry.7 = update.remove_block_entity.unwrap_or(entry.7);
        }
        for (position, (state, shapes, outline_shapes, interaction_shapes, properties, state_id, block_entity, remove_block_entity)) in grouped {
            if shapes.is_empty() && outline_shapes.is_empty() && interaction_shapes.is_empty() {
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
                // a fluid alone is its own block
                water_block: fluid == FluidState::Water,
                fall_damage_resetting: false,
                player_fall_damage_resetting: false,
                bounce_restitution: 0.0,
                suppresses_bounce: false,
                suffocating: None,
                in_wall: None,
                bottom_face_sturdy: None,
                explosion_resistance: None,
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
                water_block: fluid == FluidState::Water,
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
        let flow = fluid_flow.into();
        validate_fluid_state(fluid_state, fluid_level, fluid_source, fluid_falling, flow)?;
        if generated_fluid_state_index_for(
            &version,
            fluid_state,
            fluid_level,
            fluid_source,
            fluid_falling,
        )?
        .is_none()
        {
            return Err(Error::from_reason(
                "fluid state is not present in the generated source definition",
            ));
        }
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
                water_block: fluid_state == FluidState::Water,
                ..BlockState::default()
            },
        );
        Ok(())
    }

    /// The collision boxes in the query, its cells by the release's Mth.floor and Mth.ceil (the
    /// newest release when none is given). Before 1.13 a box of no height is among them, as the
    /// release's entities meet it (legacy_collision_shapes): 1.12.2 SnowLayerBlock.java:27 keeps the
    /// one-layer box as an AABB of no height, while 1.13.2 Shapes.java:60-66 makes a box that covers
    /// no voxel an empty shape.
    #[napi]
    pub fn collision_shapes(
        &self,
        min: NativeVec3,
        max: NativeVec3,
        version: Option<String>,
    ) -> Result<Vec<NativeAabb>> {
        let release = world_border_release(version)?;
        let query = crate::math::Aabb::new(min.into(), max.into());
        validate_query("collision", query)?;
        let shapes = if release.at_least(1, 13, 0) {
            self.inner.collision_shapes(query, release)
        } else {
            self.inner.legacy_collision_shapes(query, release)
        };

        Ok(shapes
            .into_iter()
            .map(|bounds| NativeAabb {
                min: bounds.min.into(),
                max: bounds.max.into(),
            })
            .collect())
    }

    /// The collision boxes in the query as an entity is given them, the blocks that ask its
    /// CollisionContext (scaffolding, powder snow) answering for this one.
    #[napi]
    pub fn entity_collision_shapes(
        &self,
        min: NativeVec3,
        max: NativeVec3,
        context: NativeCollisionContext,
        version: Option<String>,
    ) -> Result<Vec<NativeAabb>> {
        let release = world_border_release(version)?;
        let query = crate::math::Aabb::new(min.into(), max.into());
        validate_query("collision", query)?;
        let context = crate::world::EntityCollisionContext {
            bottom: context.bottom,
            descending: context.descending,
            fall_distance: context.fall_distance,
            walks_on_powder_snow: context.walks_on_powder_snow,
            falling_block: context.falling_block.unwrap_or(false),
        };
        Ok(self
            .inner
            .collision_shapes_for_entity(query, &context, release)
            .into_iter()
            .map(|bounds| NativeAabb {
                min: bounds.min.into(),
                max: bounds.max.into(),
            })
            .collect())
    }

    /// The outline boxes in the query, as collision_shapes takes its cells.
    #[napi]
    pub fn outline_shapes(
        &self,
        min: NativeVec3,
        max: NativeVec3,
        version: Option<String>,
    ) -> Result<Vec<NativeAabb>> {
        let release = world_border_release(version)?;
        let query = crate::math::Aabb::new(min.into(), max.into());
        validate_query("outline", query)?;

        Ok(self.inner.outline_shapes(query, release).into_iter().map(|bounds| NativeAabb {
            min: bounds.min.into(),
            max: bounds.max.into(),
        }).collect())
    }

    /// The interaction boxes in the query, as collision_shapes takes its cells.
    #[napi]
    pub fn interaction_shapes(
        &self,
        min: NativeVec3,
        max: NativeVec3,
        version: Option<String>,
    ) -> Result<Vec<NativeAabb>> {
        let release = world_border_release(version)?;
        let query = crate::math::Aabb::new(min.into(), max.into());
        validate_query("interaction", query)?;
        Ok(self
            .inner
            .interaction_shapes(query, release)
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
                .map(|state| state.version().to_string()),
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

/// What entityTravelSupport says of a kind.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Debug)]
pub struct NativeEntityTravelSupport {
    /// "player", "boat", "ridden", "mob", "dragon" (simulateVersionedDragonTick), "entity", which is
    /// no LivingEntity and has its own tick,
    /// "unsupported", which the game has and the engine has no path for, or "unknown", no kind of the game.
    pub family: String,
    /// The release's registry holds the kind.
    pub available: bool,
    /// The first release that has it.
    pub since: Option<String>,
}

/// The engine's inventory of the kinds that move through fluids or are steered through them
/// (entity_travel.rs): which path moves a kind in a release, if any.
#[napi]
pub fn entity_travel_support(version: String, entity_type: String) -> Result<NativeEntityTravelSupport> {
    let release = registered_release(&version)?;
    let support = travel_support(&entity_type, release);
    Ok(NativeEntityTravelSupport {
        family: support.family.map_or("unknown", TravelFamily::name).to_string(),
        available: support.available,
        since: support
            .since
            .map(|(major, minor, patch)| format!("{major}.{minor}.{patch}")),
    })
}

/// One client tick of the local player: the attached rockets and nearby entities the client ticks
/// before it boost and push it, it ticks (colliding with the entities that are colliders), the
/// other rockets and entities boost and push it, then the moving pistons push it. The rockets
/// counted by fireworkRockets boost at the start of their side of the player; the
/// "attached_rocket" nearby entities boost in their turns among the others.
#[napi]
pub fn simulate_versioned_tick(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeSimulationState,
    config: NativeSimulationConfig,
) -> Result<NativeSimulationState> {
    let mut scene = LocalPlayerScene::take(&mut state)?;
    let mut simulation_state: SimulationState = state.into();
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let config = validated_simulation_config(config)?;
    scene.tick(&version, minecraft_version, world, &mut simulation_state, config)?;
    Ok(simulation_state.into())
}

/// simulateVersionedTick with its phase trace: the same tick, which a trace only reads, and a copy of
/// the state at each point it passed. A tick that does not travel (a removed entity) passes fewer.
#[napi]
pub fn simulate_versioned_tick_traced(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeSimulationState,
    config: NativeSimulationConfig,
) -> Result<NativeTracedTick> {
    let mut scene = LocalPlayerScene::take(&mut state)?;
    let mut simulation_state: SimulationState = state.into();
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let config = validated_simulation_config(config)?;
    let (outcome, events) = crate::phase::traced(|| {
        scene.tick(&version, minecraft_version, world, &mut simulation_state, config)
    });
    outcome?;
    Ok(NativeTracedTick {
        state: simulation_state.into(),
        phases: events
            .into_iter()
            .map(|event| NativeTickPhase {
                name: event.phase.name().to_string(),
                occurrence: event.occurrence,
                state: event.state.into(),
                travel_vector: event.travel_vector.map(Into::into),
            })
            .collect(),
    })
}

/// simulateVersionedTick's tick with all it writes (the TickOutputSnapshot of the tick): the state
/// carrying the scene the next tick reads, so it can be passed back as the next tick's state,
/// the pushes the nearby entities took back, whether the spin attack ended, and the piston deltas.
#[napi]
pub fn simulate_versioned_tick_outcome(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeSimulationState,
    config: NativeSimulationConfig,
) -> Result<NativeTickOutcome> {
    let nearby_entities = state.nearby_entities.clone();
    let had_pistons = state.moving_pistons.is_some();
    let mut scene = LocalPlayerScene::take(&mut state)?;
    let mut simulation_state: SimulationState = state.into();
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let config = validated_simulation_config(config)?;
    let outcome = scene.tick(&version, minecraft_version, world, &mut simulation_state, config)?;
    let next = state_with_scene(simulation_state.into(), &scene, nearby_entities, had_pistons);
    let [x, y, z] = outcome.piston_deltas;
    Ok(NativeTickOutcome {
        state: next,
        entity_pushes: outcome
            .entity_pushes
            .into_iter()
            .map(|push| NativeNearbyEntityPush {
                index: push.index as u32,
                x: push.x,
                z: push.z,
            })
            .collect(),
        auto_spin_attack_ended: outcome.auto_spin_attack_ended,
        piston_deltas: NativeVec3 { x, y, z },
    })
}

/// What a state brings to simulateVersionedTick beside the player: the nearby entities, the
/// attached rockets and the moving pistons, as the TickScene a TickInputSnapshot also runs.
pub(crate) struct LocalPlayerScene {
    scene: TickScene,
}

/// The state with the scene it reached, which the next tick reads: `nearby_entities` as they came,
/// since the scene's are the engine's own form.
pub(crate) fn state_with_scene(
    mut next: NativeSimulationState,
    scene: &LocalPlayerScene,
    nearby_entities: Option<Vec<NativeNearbyEntity>>,
    had_pistons: bool,
) -> NativeSimulationState {
    let scene = &scene.scene;
    next.movement_speed_attribute = scene.movement_speed.as_ref().map(Into::into);
    next.nearby_entities = nearby_entities;
    next.firework_rockets = Some(scene.firework_rockets);
    next.firework_rockets_before_player = Some(scene.firework_rockets_before_player);
    next.firework_rocket_ticks = scene.rocket_ticks.clone();
    next.moving_pistons = (had_pistons || !scene.moving_pistons.is_empty())
        .then(|| scene.moving_pistons.iter().cloned().map(Into::into).collect());
    next
}

impl LocalPlayerScene {
    pub(crate) fn take(state: &mut NativeSimulationState) -> Result<Self> {
        validate_tick_state("simulateVersionedTick", state)?;
        let nearby_entities = state
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
        let rocket_ticks = state.firework_rocket_ticks.take();
        let rockets = match (&rocket_ticks, state.firework_rockets) {
            (Some(ticks), Some(rockets)) if ticks.len() != rockets as usize => {
                return Err(Error::from_reason(
                    "fireworkRocketTicks must list one count per attached rocket",
                ))
            }
            (Some(ticks), _) => ticks.len() as u32,
            (None, rockets) => rockets.unwrap_or(0),
        };
        if rockets > MAX_ATTACHED_ROCKETS {
            return Err(Error::from_reason(format!(
                "simulateVersionedTick: fireworkRockets is {rockets}, past the {MAX_ATTACHED_ROCKETS} a tick counts"
            )));
        }
        let rockets_before_player = state
            .firework_rockets_before_player
            .unwrap_or(0)
            .min(rockets);
        let movement_speed = state
            .movement_speed_attribute
            .take()
            .map(MovementSpeedAttribute::try_from)
            .transpose()?;
        let mut scene = TickScene {
            movement_speed,
            nearby_entities,
            firework_rockets: rockets,
            firework_rockets_before_player: rockets_before_player,
            rocket_ticks,
            auto_spin_attack: false,
            moving_pistons,
            piston_deltas: [0.0; 3],
        };
        // a rocket with no tick left has ended: it boosts no more
        scene.end_rockets(0);
        Ok(Self { scene })
    }

    /// One client tick of the local player in this scene (simulateVersionedTick), then the
    /// scene's own: the moving pistons move on and the attached rockets count the tick off
    /// (TickScene::advance).
    pub(crate) fn tick(
        &mut self,
        version: &str,
        release: MinecraftVersion,
        world: &mut NativeWorld,
        state: &mut SimulationState,
        config: PhysicsConfig,
    ) -> Result<TickSceneOutcome> {
        let outcome = tick_scene(
            &mut world.inner,
            state,
            &mut self.scene,
            config,
            release,
            |state, world| versioned::release_riptide(version, state, world, config),
            |state, world| versioned::dispatch(version, state, world, config),
        )
        .map_err(Error::from_reason)?;
        check_fire_draws(state)?;
        Ok(outcome)
    }
}

/// simulateVersionedTick's tick up to Player.tick's pose update: the rockets and entities the
/// client ticks before the player, and its tick with the colliders among the entities in the
/// world. `rockets` counts all the attached rockets and those before. Returns the pushes the
/// entities took back from the player and whether the spin attack ended;
/// finish_local_player_tick runs the rest.
pub(crate) fn move_local_player_tick(
    version: &str,
    release: MinecraftVersion,
    world: &mut NativeWorld,
    state: &mut SimulationState,
    movement_speed: Option<&mut MovementSpeedAttribute>,
    nearby_entities: &[NearbyEntity],
    rockets: (u32, u32),
    auto_spin_attack: bool,
    config: PhysicsConfig,
) -> Result<(Vec<EntityPush>, bool)> {
    let outcome = move_local_player(
        &mut world.inner,
        state,
        movement_speed,
        nearby_entities,
        rockets,
        auto_spin_attack,
        release,
        |state, world| versioned::release_riptide(version, state, world, config),
        |state, world| versioned::dispatch(version, state, world, config),
    )
    .map_err(Error::from_reason)?;
    check_fire_draws(state)?;
    Ok((outcome.entity_pushes, outcome.auto_spin_attack_ended))
}

/// The rockets and entities after the player, in the height the pose update left. Returns the
/// pushes the entities took back.
pub(crate) fn finish_local_player_tick(
    release: MinecraftVersion,
    world: &mut NativeWorld,
    state: &mut SimulationState,
    nearby_entities: &[NearbyEntity],
    rockets_after_player: u32,
    config: PhysicsConfig,
) -> Vec<EntityPush> {
    finish_local_player(
        &mut world.inner,
        state,
        nearby_entities,
        rockets_after_player,
        &[],
        &mut [0.0; 3],
        config,
        release,
    )
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
        &mut world.inner,
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
    validate_tick_state("simulateVersionedPistonPush", &state)?;
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

/// `ticks` simulateVersionedTick calls for each state, every tick in the scene its state brings
/// (its nearby entities, attached rockets and moving pistons, which the result drops as
/// simulateVersionedTick's does). The pistons' progress goes on 0.5 a tick up to 1, and before
/// 1.11 a piston leaves after its ending tick; the rockets given fireworkRocketTicks end when
/// their ticks run out, the others and the entities stay as they came.
#[napi]
pub fn simulate_versioned_ticks(
    version: String,
    world: &mut NativeWorld,
    states: Vec<NativeSimulationState>,
    ticks: u32,
    config: NativeSimulationConfig,
) -> Result<Vec<NativeSimulationState>> {
    let mut batch = PlayerBatch::new(version, states, config)?;
    batch.advance(world, ticks)?;
    Ok(batch.into_states())
}

#[napi]
pub fn simulate_versioned_entity_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeVersionedEntityState>,
    ticks: u32,
    config: NativeSimulationConfig,
) -> Result<Vec<NativeVersionedEntityState>> {
    let mut batch = EntityBatch::new(version, states, config)?;
    batch.advance(world.snapshot(), ticks)?;
    Ok(batch.states())
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
    let minecraft_version = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let left_position: Vec3 = left.position.into();
    let right_position: Vec3 = right.position.into();
    let left_velocity: Vec3 = left.velocity.into();
    let right_velocity: Vec3 = right.velocity.into();
    FieldCheck::new("entity push state")
        .vector("left.position", left_position, FieldRule::Finite)
        .vector("left.velocity", left_velocity, FieldRule::Finite)
        .number("left.pushthrough", left.pushthrough, FieldRule::Within(0.0, 1.0))
        .vector("right.position", right_position, FieldRule::Finite)
        .vector("right.velocity", right_velocity, FieldRule::Finite)
        .number("right.pushthrough", right.pushthrough, FieldRule::Within(0.0, 1.0))
        .done()?;
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
        left.pushthrough,
        minecraft_version,
    );
    Ok(NativeEntityPushResult {
        left_velocity: (left_velocity + left_impulse).into(),
        right_velocity: (right_velocity + right_impulse).into(),
    })
}

/// LivingEntity.knockback on a delta movement (simulation::living_knockback): none when vanilla would
/// draw a random direction, the velocity as it was when the resistance stops the hit. `roll` is the
/// random.nextDouble() the resistance is rolled against to 1.15.
#[napi]
#[allow(clippy::too_many_arguments)]
pub fn living_knockback(
    version: String,
    velocity: NativeVec3,
    on_ground: bool,
    strength: f64,
    x: f64,
    z: f64,
    resistance: f64,
    roll: Option<f64>,
) -> Result<Option<NativeVec3>> {
    let release = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    Ok(crate::simulation::living_knockback(velocity.into(), on_ground, strength, x, z, resistance, roll.unwrap_or(0.0), release)
        .map(Into::into))
}

/// Whether a block's collision box meets a query as vanilla's noCollision takes it
/// (world::shape_meets_query): `block` when the box is the block's whole shape, a full cube.
#[napi]
pub fn collision_box_meets_query(
    version: String,
    min: NativeVec3,
    max: NativeVec3,
    block: bool,
    query_min: NativeVec3,
    query_max: NativeVec3,
) -> Result<bool> {
    let release = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    Ok(shape_meets_query(
        Aabb::new(min.into(), max.into()),
        block,
        Aabb::new(query_min.into(), query_max.into()),
        release,
    ))
}

/// Player.updatePlayerPose (updateSize before 1.17): the requested pose when it fits, else
/// crouching or swimming, and no change without room to swim; a spectator or passenger takes the
/// requested pose unchecked (1.14.4 Player.java:333-363, 1.21.11 Player.java:336-352). Each fit
/// is CollisionGetter.noCollision for the player (its block context and the state's
/// nearbyEntities that collide) of the pose's box, shrunk by 1e-7 from 1.16 (1.14.4 Entity.java:1769-
/// 1643, 1.15 :1651-1653, 1.16 :1687-1689, 1.21.11 Player.java:368-370). `scale` is the SCALE
/// attribute, read from 1.20.5.
#[napi]
pub fn refresh_versioned_pose(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeSimulationState,
    current_pose: u8,
    requested_pose: u8,
    spectator: bool,
    scale: Option<f64>,
) -> Result<NativePoseRefreshResult> {
    if !versioned::is_supported(&version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    let release = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
    let scale = scale.unwrap_or(1.0);
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Error::from_reason("the scale must be positive"));
    }
    let scale = scale as f32;
    player_pose_size(current_pose, release, scale)?;
    let requested_size = player_pose_size(requested_pose, release, scale)?;
    let swimming_size = player_pose_size(POSE_SWIMMING, release, scale)?;
    let crouching_size = player_pose_size(POSE_CROUCHING, release, scale)?;
    validate_tick_state("refreshVersionedPose", &state)?;
    let nearby_entities: Vec<NearbyEntity> = state
        .nearby_entities
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect();
    let simulation_state: SimulationState = state.into();
    world
        .inner
        .set_entity_colliders(nearby_entity_colliders(&nearby_entities, false));
    let context = entity_collision_context(&simulation_state);
    let entity = collision_query_entity(&simulation_state, &context);
    let fits = |size: PoseSize| {
        let half_width = f64::from(size.width / 2.0);
        let position = simulation_state.position;
        let bounds = Aabb::new(
            Vec3::new(position.x - half_width, position.y, position.z - half_width),
            Vec3::new(
                position.x + half_width,
                position.y + f64::from(size.height),
                position.z + half_width,
            ),
        );
        // (1.15.2 Entity.java:1651-1653; 1.16 Entity.java:1687-1689 (deflate(1.0E-7)))
        let bounds = if release.at_least(1, 16, 0) {
            bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
        } else {
            bounds
        };
        world
            .inner
            .no_collision_for_entity(bounds, entity, release)
    };
    let selected_pose = if !fits(swimming_size) {
        current_pose
    } else if spectator || simulation_state.passenger || fits(requested_size) {
        requested_pose
    } else if fits(crouching_size) {
        POSE_CROUCHING
    } else {
        POSE_SWIMMING
    };
    world.inner.set_entity_colliders(Vec::new());
    let selected_size = player_pose_size(selected_pose, release, scale)?;
    let mut output_state = simulation_state;
    output_state.half_width = f64::from(selected_size.width / 2.0);
    output_state.height = f64::from(selected_size.height);
    output_state.eye_height = Some(f64::from(selected_size.eye_height));
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
    validate_boat_vehicle(descriptor.boat_vehicle.as_ref())?;
    FieldCheck::new("entity descriptor")
        .vector("position", descriptor.position.clone().into(), FieldRule::Finite)
        .vector("velocity", descriptor.velocity.clone().into(), FieldRule::Finite)
        .number("halfWidth", descriptor.half_width, FieldRule::Positive)
        .number("height", descriptor.height, FieldRule::Positive)
        .number("yaw", descriptor.yaw, FieldRule::Finite)
        .done()?;
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
        no_gravity: descriptor.no_gravity.unwrap_or(false),
        gravity: None,
        step_height: None,
    };
    let mut state = SimulationState::from(entity);
    state.yaw = descriptor.yaw;
    state.input = descriptor.input.map(Into::into).unwrap_or(Vec3::ZERO);
    state.jump_requested = descriptor.jump_requested.unwrap_or(false);
    state.sprinting = descriptor.sprinting.unwrap_or(false);
    state.sneaking = descriptor.sneaking.unwrap_or(false);
    state.swimming = descriptor.swimming.unwrap_or(false);
    state.passenger = descriptor.passenger.unwrap_or(false);
    state.boat_vehicle = descriptor.boat_vehicle.map(Into::into);
    state.can_simulate_movement = descriptor.can_simulate_movement;
    state.non_living = descriptor.non_living.unwrap_or(false);
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
    // every LivingEntity reads MOVEMENT_EFFICIENCY (1.21) and the FRICTION and AIR_DRAG modifiers
    // (26.2), as the full state's conversion forwards them
    state.movement_efficiency = descriptor.movement_efficiency.unwrap_or(0.0);
    state.air_drag_modifier = descriptor.air_drag_modifier.unwrap_or(0.0);
    state.friction_modifier = descriptor.friction_modifier.unwrap_or(0.0);
    state.air_drag_modifier_present = descriptor.air_drag_modifier.is_some();
    state.friction_modifier_present = descriptor.friction_modifier.is_some();
    state.sneaking_speed = descriptor.sneaking_speed;
    state.jump_ticks = descriptor.jump_ticks.unwrap_or(0);
    state.using_item = descriptor.using_item.unwrap_or(false);
    state.flying = descriptor.flying.unwrap_or(false);
    // Abilities.flyingSpeed starts at 0.05F (1.21.11 Abilities.java:19)
    state.flying_speed = descriptor.flying_speed.unwrap_or(f64::from(0.05_f32));
    state.no_physics = descriptor.no_physics.unwrap_or(false);
    state.pitch = descriptor.pitch.unwrap_or(0.0);
    let release = match descriptor.version.as_deref() {
        Some(version) => registered_release(version)?,
        None => VersionPhysicsRules::default().release,
    };
    state.look = descriptor.look.map(Into::into).unwrap_or_else(|| {
        view_vector(state.pitch, state.yaw, release)
    });
    state.firework_boost = descriptor
        .firework_boost
        .map(Into::into)
        .unwrap_or(Vec3::ZERO);
    check_state("entity descriptor", state)?;

    Ok(state.into())
}

/// Entity.calculateViewVector of the float radians of a pitch and yaw: Mth table lookups with float
/// products (1.13.2 Entity.java:1336-1344, 1.21.11 Entity.java:1913-1921). Before 1.13
/// Entity.getVectorForRotation turns the yaw half a turn and negates the pitch's cosine instead
/// (1.8.8 Entity.java:1088-1092, 1.12.2 Entity.java:1312-1318).
fn view_vector(pitch: f64, yaw: f64, release: MinecraftVersion) -> Vec3 {
    let table_sin = |radians: f32| mth_sin(f64::from(radians), release) as f32;
    let table_cos = |radians: f32| mth_cos(f64::from(radians), release) as f32;
    if !release.at_least(1, 13, 0) {
        let yaw = -(yaw as f32) - 3.1415927;
        let pitch = -(pitch as f32);
        let pitch_cos = -table_cos(pitch);
        return Vec3::new(
            f64::from(table_sin(yaw) * pitch_cos),
            f64::from(table_sin(pitch)),
            f64::from(table_cos(yaw) * pitch_cos),
        );
    }
    let (pitch, yaw) = (pitch as f32, -(yaw as f32));
    let pitch_cos = table_cos(pitch);
    Vec3::new(
        f64::from(table_sin(yaw) * pitch_cos),
        f64::from(-table_sin(pitch)),
        f64::from(table_cos(yaw) * pitch_cos),
    )
}

fn dimensions_for_pose(
    half_width: f64,
    height: f64,
    pose: Option<u8>,
    version: Option<&str>,
) -> Result<Dimensions> {
    let Some(pose) = pose else {
        return Ok(Dimensions::new(half_width, height));
    };
    let release = version.map(registered_release).transpose()?;
    // ids 6 and 7 name different poses before and from 1.17: no release, no telling which
    match (release, pose) {
        (None, 6) => {
            return Err(Error::from_reason(
                "long-jumping pose is unavailable for this version",
            ))
        }
        (None, 7) => {
            return Err(Error::from_reason(
                "dying pose id 7 is unavailable for this version",
            ))
        }
        _ => {}
    }
    let release = release.unwrap_or(VersionPhysicsRules::default().release);
    let size = player_pose_size(pose, release, 1.0)?;
    Ok(Dimensions::new(
        f64::from(size.width / 2.0),
        f64::from(size.height),
    ))
}

const POSE_SWIMMING: u8 = 3;
const POSE_CROUCHING: u8 = 5;

/// A player pose's EntityDimensions (EntitySize before 1.17): float width, height and eye height.
#[derive(Clone, Copy)]
struct PoseSize {
    width: f32,
    height: f32,
    eye_height: f32,
}

/// Player.getDimensions(pose) and its eye height for a synced pose id. The poses come with 1.14;
/// id 6 is DYING until LONG_JUMPING takes it in 1.17, DYING moving to 7 (1.14.4 Pose.java,
/// 1.16.5 Pose.java, 1.17 Pose.java). The sizes are 1.14.4 Player.java:111-116
/// and 1.21.11 Avatar.java:20-55 (LONG_JUMPING has none, so the standing size); the eyes 0.2F
/// asleep (1.14.4 LivingEntity.java:2750), else 1.14.4 Player.java:1741-1752, then the
/// dimensions' (1.20.5 Player.java:2048-2049). From 1.20.5 the scale multiplies all but the
/// fixed sleeping and dying sizes (1.20.5 LivingEntity.java:3264-3266, EntityDimensions.java:38-41).
fn player_pose_size(pose: u8, release: MinecraftVersion, scale: f32) -> Result<PoseSize> {
    if !release.at_least(1, 14, 0) {
        return Err(Error::from_reason(
            "entity poses are unavailable before 1.14",
        ));
    }
    // (1.16.5 Pose.java:9-10 CROUCHING, DYING; 1.17 Pose.java:9-11 puts LONG_JUMPING before DYING)
    let long_jumping = release.at_least(1, 17, 0);
    let size = |width: f32, height: f32, eye_height: f32| PoseSize {
        width,
        height,
        eye_height,
    };
    let (base, fixed) = match pose {
        0 => (size(0.6, 1.8, 1.62), false),
        1 | 3 | 4 => (size(0.6, 0.6, 0.4), false),
        2 => (size(0.2, 0.2, 0.2), true),
        5 => (size(0.6, 1.5, 1.27), false),
        6 if long_jumping => (size(0.6, 1.8, 1.62), false),
        6 => (size(0.2, 0.2, 1.62), true),
        7 if long_jumping => (size(0.2, 0.2, 1.62), true),
        7 => {
            return Err(Error::from_reason(
                "dying pose id 7 is unavailable for this version",
            ))
        }
        _ => {
            return Err(Error::from_reason(if long_jumping {
                "entity pose must be between 0 and 7"
            } else {
                "entity pose must be between 0 and 6"
            }))
        }
    };
    // (1.20.4 Player.java:1935-1937 (Player overrides getDimensions, unscaled); 1.20.5 LivingEntity.java:3264-3266, Player.java:1899-1901, EntityDimensions.java:38-40 (scaled unless fixed))
    if fixed || scale == 1.0 || !release.at_least(1, 20, 5) {
        return Ok(base);
    }
    Ok(size(
        base.width * scale,
        base.height * scale,
        base.eye_height * scale,
    ))
}

pub(crate) fn validated_simulation_config(config: NativeSimulationConfig) -> Result<PhysicsConfig> {
    let config: PhysicsConfig = config.into();
    config.validate().map_err(Error::from_reason)?;
    Ok(config)
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

/// What this binary is, as it was compiled: the Rust target and the hash of the sources it was built
/// from (native/build.rs). native/artifacts/provenance.*.json records the same, so a binary from
/// another build cannot pass as this one, and a WebAssembly fallback cannot pass as a native addon.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeBuildIdentity {
    pub target: String,
    pub source_hash: String,
    pub crate_version: String,
}

#[napi]
pub fn build_identity() -> NativeBuildIdentity {
    NativeBuildIdentity {
        target: env!("MCPN_BUILD_TARGET").to_string(),
        source_hash: env!("MCPN_SOURCE_HASH").to_string(),
        crate_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Makes every release follow one Java major's Math.sin and Math.cos (8, or 9 and up, which agree),
/// or with null each the one its version json declares: Java 8 up to 1.16.5 (crate::math::math_java).
/// For the whole process; the recorder runs every release on Java 21 or 25, so its replays set 21.
#[napi]
pub fn set_math_java(major: Option<u32>) -> Result<()> {
    match major {
        None => crate::math::set_math_java(None),
        Some(major @ 8..=255) => crate::math::set_math_java(Some(major as u8)),
        Some(major) => return Err(Error::from_reason(format!("no Java major {major}: 8 or more"))),
    }
    Ok(())
}

/// The Java major whose Math a release follows now (setMathJava, else the declared one).
#[napi]
pub fn math_java(version: String) -> Result<u32> {
    let release = MinecraftVersion::parse(&version)
        .ok_or_else(|| Error::from_reason(format!("unsupported Minecraft version {version}")))?;
    Ok(u32::from(crate::math::math_java(release)))
}

/// Whether the engine has a module for `version` (the releases every simulateVersioned* call and
/// NativePlayer accept), so a caller can refuse one before it builds an engine.
#[napi]
pub fn is_supported_version(version: String) -> bool {
    versioned::is_supported(&version)
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

fn native_state_shape(shape: crate::block_shapes::StateShape) -> NativeGeneratedStateShape {
    use crate::block_shapes::{StateShape, DEPENDS_ENTITY, DEPENDS_LEVEL, DEPENDS_POSITION};
    match shape {
        StateShape::Exact(boxes) => NativeGeneratedStateShape {
            status: "exact".to_string(),
            depends_on: Vec::new(),
            boxes: boxes
                .iter()
                .map(|[min_x, min_y, min_z, max_x, max_y, max_z]| NativeGeneratedShapeBox {
                    min_x: *min_x,
                    min_y: *min_y,
                    min_z: *min_z,
                    max_x: *max_x,
                    max_y: *max_y,
                    max_z: *max_z,
                })
                .collect(),
        },
        StateShape::Contextual(mask) => NativeGeneratedStateShape {
            status: "contextual".to_string(),
            depends_on: [(DEPENDS_POSITION, "position"), (DEPENDS_LEVEL, "level"), (DEPENDS_ENTITY, "entity")]
                .into_iter()
                .filter(|(bit, _)| mask & bit != 0)
                .map(|(_, name)| name.to_string())
                .collect(),
            boxes: Vec::new(),
        },
        StateShape::Absent => NativeGeneratedStateShape {
            status: "absent".to_string(),
            depends_on: Vec::new(),
            boxes: Vec::new(),
        },
    }
}

/// A channel of a block's state by its index among the block's states; null for a release with no table, an
/// unknown block or channel, or an index past its states.
#[napi]
pub fn generated_state_shape(
    version: String,
    block: String,
    kind: String,
    index: u32,
) -> Option<NativeGeneratedStateShape> {
    let (shapes, block) = crate::block_shapes::block(&version, &block)?;
    block.shape(shapes, &kind, index as usize).map(native_state_shape)
}

/// A channel of a block's state by its properties, vanilla's names and values: null as above, and for a tuple
/// that is not exactly one value of each of the block's properties.
#[napi]
pub fn generated_state_shape_for_properties(
    version: String,
    block: String,
    kind: String,
    properties: Vec<NativeBlockProperty>,
) -> Option<NativeGeneratedStateShape> {
    let (shapes, block) = crate::block_shapes::block(&version, &block)?;
    let values = properties
        .iter()
        .map(|property| (property.name.as_str(), property.value.as_str()))
        .collect::<Vec<_>>();
    block.shape(shapes, &kind, block.state_index(&values)?).map(native_state_shape)
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

/// A boat without a world, at the default release (vehicle::tick_boat): its yaw is its yRot in
/// radians, `yawDegrees` the float degrees when given.
#[napi]
pub fn simulate_boat_tick(state: NativeBoatState) -> Result<NativeBoatState> {
    let mut state = boat_state(state)?;
    tick_boat(&mut state);
    check_move("simulateBoatTick", state.move_out_of_range)?;
    Ok(native_boat_state(state))
}

/// The release a caller named, which must be one the engine registers by that exact spelling: an
/// unknown patch, a prerelease or "1.21.0" for 1.21 is refused, never read as the nearest release.
pub(crate) fn registered_release(version: &str) -> Result<MinecraftVersion> {
    if !versioned::is_supported(version) {
        return Err(Error::from_reason(format!(
            "unsupported Minecraft version: {version}"
        )));
    }
    MinecraftVersion::parse(version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))
}

fn world_border_release(version: Option<String>) -> Result<MinecraftVersion> {
    match version {
        Some(version) => registered_release(&version),
        None => Ok(VersionPhysicsRules::default().release),
    }
}

pub(crate) fn vehicle_release(version: &str, kind: &str) -> Result<MinecraftVersion> {
    if !versioned::is_supported(version) {
        return Err(Error::from_reason(format!(
            "unsupported Minecraft version for {kind} simulation: {version}"
        )));
    }
    MinecraftVersion::parse(version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))
}

/// Where a boat seats a player alone in it, and the yaw it leaves it with (Entity.positionRider,
/// Boat.positionRider: `boat_passenger_position`, `boat_passenger_yaw`).
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeBoatPlacement {
    pub position: NativeVec3,
    pub yaw_degrees: f64,
}

/// The seat of the player a boat in `state`, as its tick left it, carries in front: a chest boat's
/// and a raft's differ (`chest`, `raft`), and so does one of `passengers` (absent: alone).
#[napi]
pub fn boat_passenger_placement(
    version: String,
    state: NativeBoatState,
    passenger_yaw_degrees: f64,
    chest: Option<bool>,
    raft: Option<bool>,
    passengers: Option<u32>,
) -> Result<NativeBoatPlacement> {
    let release = vehicle_release(&version, "boat")?;
    let boat = boat_state(state)?;
    let seat = BoatSeat {
        chest: chest.unwrap_or(false),
        raft: raft.unwrap_or(false),
        passengers: passengers.unwrap_or(1) as _,
        ..BoatSeat::player(release)
    };
    Ok(NativeBoatPlacement {
        position: boat_passenger_position(&boat, seat, release).into(),
        yaw_degrees: f64::from(boat_passenger_yaw(passenger_yaw_degrees as f32, &boat, release)),
    })
}

#[napi]
pub fn simulate_versioned_boat_tick(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeBoatState,
) -> Result<NativeBoatState> {
    let release = vehicle_release(&version, "boat")?;
    let colliders = vehicle_colliders(state.nearby_entities.take(), true, release);
    let mut state = boat_state(state)?;
    world.inner.set_entity_colliders(colliders);
    tick_boat_in_world(&mut state, &mut world.inner, release);
    world.inner.set_entity_colliders(Vec::new());
    check_move("simulateVersionedBoatTick", state.move_out_of_range)?;
    Ok(native_boat_state(state))
}

/// The head or the tail of a vehicle's move inside its tick.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeVehicleMove {
    pub name: String,
    pub position: NativeVec3,
    pub velocity: NativeVec3,
}

/// simulateVersionedBoatTick's state, and the moves of the tick on the way to it.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeTracedBoatTick {
    pub state: NativeBoatState,
    pub moves: Vec<NativeVehicleMove>,
}

/// simulateVersionedHorseTick's state, and the moves of the tick on the way to it.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeTracedHorseTick {
    pub state: NativeHorseState,
    pub moves: Vec<NativeVehicleMove>,
}

fn native_vehicle_moves(events: Vec<crate::phase::VehicleMoveEvent>) -> Vec<NativeVehicleMove> {
    events.into_iter().map(|event| NativeVehicleMove { name: event.name.to_string(), position: event.position.into(), velocity: event.velocity.into() }).collect()
}

/// simulateVersionedBoatTick with the head and tail of each move inside the tick, which only a trace pays for.
#[napi]
pub fn simulate_versioned_boat_tick_traced(
    version: String,
    world: &mut NativeWorld,
    state: NativeBoatState,
) -> Result<NativeTracedBoatTick> {
    let (state, events) = crate::phase::traced_vehicle(|| simulate_versioned_boat_tick(version, world, state));
    Ok(NativeTracedBoatTick { state: state?, moves: native_vehicle_moves(events) })
}

/// simulateVersionedHorseTick with the head and tail of each move inside the tick.
#[napi]
pub fn simulate_versioned_horse_tick_traced(
    version: String,
    world: &mut NativeWorld,
    state: NativeHorseState,
) -> Result<NativeTracedHorseTick> {
    let (state, events) = crate::phase::traced_vehicle(|| simulate_versioned_horse_tick(version, world, state));
    Ok(NativeTracedHorseTick { state: state?, moves: native_vehicle_moves(events) })
}

/// The colliders of a vehicle's `nearbyEntities` (vehicle_entity_colliders).
pub(crate) fn vehicle_colliders(
    nearby: Option<Vec<NativeNearbyEntity>>,
    boat: bool,
    release: MinecraftVersion,
) -> Vec<EntityCollider> {
    let entities: Vec<NearbyEntity> = nearby
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect();
    vehicle_entity_colliders(&entities, boat, release)
}

#[napi]
pub fn simulate_versioned_boat_ticks(
    version: String,
    world: &mut NativeWorld,
    states: Vec<NativeBoatState>,
    ticks: u32,
) -> Result<Vec<NativeBoatState>> {
    let mut batch = BoatBatch::new(&version, states)?;
    batch.advance(world, ticks)?;
    Ok(batch.states())
}

#[napi]
pub fn simulate_horse_tick(state: NativeHorseState) -> Result<NativeHorseState> {
    // (this is AbstractHorse's tick: the other mobs have their own, which take a world)
    if let Some(name) = state.entity_type.as_deref() {
        if !RiddenMobKind::parse(name).is_some_and(RiddenMobKind::is_horse) {
            return Err(Error::from_reason(format!(
                "simulateHorseTick is a horse's: {name} ticks through simulateVersionedHorseTick"
            )));
        }
    }
    let mut state = horse_state(state)?;
    tick_horse(&mut state);
    Ok(native_horse_state(state))
}

/// The tick of a mob a player rides and steers (`entityType`: a horse when unnamed).
#[napi]
pub fn simulate_versioned_horse_tick(
    version: String,
    world: &mut NativeWorld,
    mut state: NativeHorseState,
) -> Result<NativeHorseState> {
    let release = vehicle_release(&version, "ridden mob")?;
    let colliders = vehicle_colliders(state.nearby_entities.take(), false, release);
    let mut mob = ridden_mob_state(state, release)?;
    world.inner.set_entity_colliders(colliders);
    tick_ridden_mob(&mut mob, world.snapshot(), release);
    world.inner.set_entity_colliders(Vec::new());
    check_move("simulateVersionedHorseTick", mob.move_out_of_range)?;
    Ok(native_ridden_mob_state(mob, release))
}

#[napi]
pub fn simulate_versioned_horse_ticks(
    version: String,
    world: &mut NativeWorld,
    states: Vec<NativeHorseState>,
    ticks: u32,
) -> Result<Vec<NativeHorseState>> {
    let mut batch = HorseBatch::new(&version, states)?;
    batch.advance(world, ticks)?;
    Ok(batch.states())
}

#[napi]
pub fn simulate_projectile_tick(
    world: &NativeWorld,
    state: NativeProjectileState,
) -> Result<NativeProjectileTickResult> {
    let mut state = projectile_state(state, None)?;
    validate_projectile_flight(&state)?;
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
    let mut batch = ProjectileBatch::new(&version, states)?;
    batch.advance(world.snapshot(), ticks)?;
    Ok(batch.into_results())
}

#[napi]
pub fn simulate_arrow_tick(
    world: &NativeWorld,
    state: NativeArrowState,
) -> Result<NativeArrowTickResult> {
    let mut state = arrow_state(state, None)?;
    validate_arrow_flight(&state)?;
    let result = tick_arrow(&mut state, world.snapshot());
    Ok(native_arrow_tick_result(result, &state))
}

#[napi]
pub fn simulate_versioned_arrow_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeArrowState>,
    ticks: u32,
) -> Result<Vec<NativeArrowTickResult>> {
    let mut batch = ArrowBatch::new(&version, states)?;
    batch.advance(world.snapshot(), ticks)?;
    Ok(batch.into_results())
}

#[napi]
pub fn simulate_trident_tick(
    world: &NativeWorld,
    state: NativeTridentState,
) -> Result<NativeTridentTickResult> {
    let mut state = trident_state(state, None)?;
    validate_trident_flight(&state)?;
    let result = tick_trident(&mut state, world.snapshot());
    Ok(native_trident_tick_result(result, &state))
}

#[napi]
pub fn simulate_versioned_trident_ticks(
    version: String,
    world: &NativeWorld,
    states: Vec<NativeTridentState>,
    ticks: u32,
) -> Result<Vec<NativeTridentTickResult>> {
    let mut batch = TridentBatch::new(&version, states)?;
    batch.advance(world.snapshot(), ticks)?;
    Ok(batch.into_results())
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

/// The projectile of a call; `release` gives the family's default gravity (the unversioned call
/// takes 26.2's).
/// Whether the release has the family, as the engine models it.
pub(crate) fn check_projectile_family(
    state: &ProjectileState,
    release: MinecraftVersion,
    version: &str,
) -> Result<()> {
    if let Some((name, first)) = state.kind.first_release().filter(|(_, first)| release < *first) {
        return Err(Error::from_reason(format!(
            "{name} simulation is not supported before {first}: {version}"
        )));
    }
    Ok(())
}

pub(crate) fn projectile_state(
    value: NativeProjectileState,
    release: Option<MinecraftVersion>,
) -> Result<ProjectileState> {
    let kind = match value.kind.as_deref() {
        Some(name) => {
            let kind = ProjectileKind::from_name_checked(name).ok_or_else(|| {
                Error::from_reason(format!("unsupported projectile family: {name}"))
            })?;
            // their embedded state has no place here, and thrown-item motion is another motion
            if kind.is_arrow() {
                let call = if kind == ProjectileKind::Trident {
                    "simulateVersionedTridentTicks"
                } else {
                    "simulateVersionedArrowTicks"
                };
                return Err(Error::from_reason(format!(
                    "{name} is an AbstractArrow: use {call}"
                )));
            }
            kind
        }
        None => ProjectileKind::Generic,
    };
    let defaults = match release {
        Some(release) => {
            ProjectileState::for_kind_in_release(kind, Vec3::ZERO, Vec3::ZERO, release)
        }
        None => ProjectileState::for_kind(kind, Vec3::ZERO, Vec3::ZERO),
    };
    // WitherSkull.getInertia (getDrag before 1.21.2): 0.73F for a dangerous skull
    let default_air_drag = if kind == ProjectileKind::WitherSkull && value.dangerous == Some(true) {
        f64::from(0.73_f32)
    } else {
        defaults.air_drag
    };
    let state = ProjectileState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        gravity: value.gravity.unwrap_or(defaults.gravity),
        air_drag: value.air_drag.unwrap_or(default_air_drag),
        power: value.power.map(Into::into).unwrap_or(Vec3::ZERO),
        acceleration_power: value
            .acceleration_power
            .unwrap_or(defaults.acceleration_power),
        in_water: value.in_water.unwrap_or(false),
        ultra_warm: value.ultra_warm.unwrap_or(false),
        first_tick: value.first_tick.unwrap_or(true),
        kind,
        horizontal_collision: value.horizontal_collision.unwrap_or(false),
        on_ground: value.on_ground.unwrap_or(false),
        shot_at_angle: value.shot_at_angle.unwrap_or(false),
        bobbing: value.bobbing.unwrap_or(false),
        bob_random: value.bob_random.unwrap_or(0.0) as f32,
        no_physics: value.no_physics.unwrap_or(false),
        bounding_box: value.bounding_box.map(Into::into),
        stuck_speed_multiplier: value.stuck_speed_multiplier.map(Into::into).unwrap_or(Vec3::ZERO),
    };
    FieldCheck::new("projectile simulation state")
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .number("gravity", state.gravity, FieldRule::Finite)
        .number("airDrag", state.air_drag, FieldRule::Finite)
        .vector("power", state.power, FieldRule::Finite)
        .number("accelerationPower", state.acceleration_power, FieldRule::Finite)
        .number("bobRandom", f64::from(state.bob_random), FieldRule::Finite)
        .vector("stuckSpeedMultiplier", state.stuck_speed_multiplier, FieldRule::Finite)
        .done()?;
    validate_position("projectile", state.position)?;
    Ok(state)
}

/// The ray of a projectile's tick, which is its motion with what the tick adds first.
pub(crate) fn validate_projectile_flight(state: &ProjectileState) -> Result<()> {
    let adds = state.power + Vec3::new(0.0, state.gravity.abs(), 0.0);
    let along = state.velocity.length() * state.acceleration_power.abs();
    let slowed = state.velocity.scale(state.air_drag.abs().max(1.0));
    validate_flight("projectile", state.position, slowed + adds + Vec3::new(along, along, along))
}

pub(crate) fn validate_arrow_flight(state: &ArrowState) -> Result<()> {
    let adds = Vec3::new(0.0, state.gravity.abs(), 0.0) + state.fall_impulse;
    let slowed = state.velocity.scale(state.air_drag.abs().max(state.water_inertia.abs()).max(1.0));
    validate_flight("arrow", state.position, slowed + adds)
}

pub(crate) fn validate_trident_flight(state: &TridentState) -> Result<()> {
    let adds = Vec3::new(0.0, state.gravity.abs(), 0.0) + state.fall_impulse;
    let slowed = state.velocity.scale(state.air_drag.abs().max(state.water_inertia.abs()).max(1.0));
    validate_flight("trident", state.position, slowed + adds)
}

pub(crate) fn native_projectile_tick_result(
    result: crate::projectile::ProjectileTickResult,
    state: &ProjectileState,
) -> NativeProjectileTickResult {
    NativeProjectileTickResult {
        position: result.position.into(),
        velocity: result.velocity.into(),
        block_hit: result.block_hit,
        in_water: state.in_water,
        first_tick: state.first_tick,
        horizontal_collision: result.horizontal_collision,
        on_ground: result.on_ground,
        bobbing: result.bobbing,
        discarded: result.discarded,
        bounding_box: state.bounding_box.map(Into::into),
        stuck_speed_multiplier: state.stuck_speed_multiplier.into(),
    }
}

/// The arrow of a call; `release` gives the default gravity (the unversioned call takes 26.2's).
pub(crate) fn arrow_state(value: NativeArrowState, release: Option<MinecraftVersion>) -> Result<ArrowState> {
    let defaults = match release {
        Some(release) => ArrowState::new_in_release(Vec3::ZERO, Vec3::ZERO, release),
        None => ArrowState::new(Vec3::ZERO, Vec3::ZERO),
    };
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
        last_block_position: value
            .last_block_position
            .map(|position| (position.x, position.y, position.z)),
        no_physics: value.no_physics.unwrap_or(false),
        ultra_warm: value.ultra_warm.unwrap_or(false),
    };
    FieldCheck::new("arrow simulation state")
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .number("gravity", state.gravity, FieldRule::Finite)
        .number("airDrag", state.air_drag, FieldRule::Finite)
        .number("waterInertia", state.water_inertia, FieldRule::Finite)
        .done()?;
    validate_position("arrow", state.position)?;
    Ok(state)
}

pub(crate) fn native_arrow_tick_result(
    result: crate::projectile::ArrowTickResult,
    state: &ArrowState,
) -> NativeArrowTickResult {
    NativeArrowTickResult {
        position: result.position.into(),
        velocity: result.velocity.into(),
        block_hit: result.block_hit,
        in_water: state.in_water,
        in_ground: result.in_ground,
        discarded: result.discarded,
        life: state.life,
        in_ground_time: result.in_ground_time,
        shake_time: result.shake_time,
        started_falling: result.started_falling,
        last_block_id: state.last_block_id,
        last_block_position: state
            .last_block_position
            .map(|(x, y, z)| NativeBlockPosition { x, y, z }),
    }
}

/// The trident of a call; `release` gives the default gravity (the unversioned call takes 26.2's).
pub(crate) fn trident_state(
    value: NativeTridentState,
    release: Option<MinecraftVersion>,
) -> Result<TridentState> {
    let defaults = match release {
        Some(release) => TridentState::new_in_release(Vec3::ZERO, Vec3::ZERO, release),
        None => TridentState::new(Vec3::ZERO, Vec3::ZERO),
    };
    let state = TridentState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        gravity: value.gravity.unwrap_or(defaults.gravity),
        air_drag: value.air_drag.unwrap_or(defaults.air_drag),
        water_inertia: value.water_inertia.unwrap_or(defaults.water_inertia),
        in_water: value.in_water.unwrap_or(false),
        dealt_damage: value.dealt_damage.unwrap_or(false),
        in_ground: value.in_ground.unwrap_or(false),
        life: value.life.unwrap_or(0),
        in_ground_time: value.in_ground_time.unwrap_or(0),
        shake_time: value.shake_time.unwrap_or(0),
        fall_impulse: value.fall_impulse.map(Into::into).unwrap_or(Vec3::ZERO),
        last_block_id: value.last_block_id,
        last_block_position: value
            .last_block_position
            .map(|position| (position.x, position.y, position.z)),
        no_physics: value.no_physics.unwrap_or(false),
        ultra_warm: value.ultra_warm.unwrap_or(false),
    };
    FieldCheck::new("trident simulation state")
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .number("gravity", state.gravity, FieldRule::Finite)
        .number("airDrag", state.air_drag, FieldRule::Finite)
        .number("waterInertia", state.water_inertia, FieldRule::Finite)
        .done()?;
    validate_position("trident", state.position)?;
    Ok(state)
}

pub(crate) fn native_trident_tick_result(
    result: crate::projectile::TridentTickResult,
    state: &TridentState,
) -> NativeTridentTickResult {
    NativeTridentTickResult {
        position: result.position.into(),
        velocity: result.velocity.into(),
        block_hit: result.block_hit,
        in_water: state.in_water,
        in_ground: result.in_ground,
        discarded: result.discarded,
        life: state.life,
        in_ground_time: result.in_ground_time,
        shake_time: result.shake_time,
        started_falling: result.started_falling,
        dealt_damage: result.dealt_damage,
        last_block_id: state.last_block_id,
        last_block_position: state
            .last_block_position
            .map(|(x, y, z)| NativeBlockPosition { x, y, z }),
    }
}

fn check_state(kind: &str, state: SimulationState) -> Result<()> {
    FieldCheck::new(kind)
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .number("yaw", state.yaw, FieldRule::Finite)
        .vector("input", state.input, FieldRule::Finite)
        .number("halfWidth", state.half_width, FieldRule::Positive)
        .number("height", state.height, FieldRule::Positive)
        // the GRAVITY attribute's range is -1..1 and STEP_HEIGHT's 0..10 (1.21.11 Attributes.java:26-66, 42-80)
        .optional("gravity", state.gravity, FieldRule::Within(-1.0, 1.0))
        .optional("stepHeight", state.step_height, FieldRule::Within(0.0, 10.0))
        .optional("movementSpeed", state.movement_speed, FieldRule::Finite)
        .optional("jumpStrength", state.jump_strength, FieldRule::Finite)
        .number("flyingSpeed", state.flying_speed, FieldRule::Finite)
        .vector("fireworkBoost", state.firework_boost, FieldRule::Finite)
        .done()
}

#[cfg(test)]
mod tests {
    use super::{
        create_simulation_state, generated_fluid_state_index_for, is_supported_version, refresh_versioned_pose,
        simulate_versioned_entity_push, simulate_versioned_entity_ticks,
        simulate_versioned_piston_push, simulate_versioned_tick, simulate_versioned_tick_outcome,
        simulate_versioned_ticks,
        NativeAabb, NativeBlockShape, NativeBlockUpdate, NativeBoatVehicle,
        NativeEntityDescriptor, NativeEntityPushState, NativeMovingPiston, NativeNearbyEntity,
        NativeSimulationConfig, NativeSimulationState, NativeVec3, NativeVersionedEntityState,
        NativeWorld,
    };
    use crate::entity::{entity_push_impulses, NearbyEntity, NearbyEntityKind};
    use crate::math::{Aabb, Vec3};
    use crate::piston::MovingPiston;
    use crate::simulation::{PhysicsConfig, SimulationState};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockShape, BlockState, FluidState, WorldSnapshot};

    #[test]
    fn a_fluid_state_is_the_liquid_block_level_before_1_13_and_a_falling_source_from_it() {
        // before 1.13 the fluid is the still or flowing liquid block, LEVEL 0-15 its only property
        // (1.12.2 LiquidBlock.java:28), so the state is the level; from 1.13 a source is
        // FlowingFluid#getSource(falling) (1.13.2 FlowingFluid.java), a state of the source fluid
        for level in [0_u8, 2, 7] {
            let index = generated_fluid_state_index_for("1.12.2", FluidState::Water, level, false, false).unwrap();
            assert_eq!(index, Some(usize::from(level)));
        }
        assert_eq!(generated_fluid_state_index_for("1.12.2", FluidState::Water, 0, true, false).unwrap(), Some(0));
        for version in ["1.13.2", "1.14.4", "26.3"] {
            let still = generated_fluid_state_index_for(version, FluidState::Water, 0, true, false).unwrap();
            let falling = generated_fluid_state_index_for(version, FluidState::Water, 0, true, true).unwrap();
            assert!(still.is_some() && falling.is_some() && still != falling, "{version}");
        }
    }

    #[test]
    fn the_knockback_binding_hands_each_release_to_the_engine() {
        // LivingEntity#knockback of a resting entity on the ground hit along +x: x/2 less the
        // strength, the Y capped at 0.4 (1.9.4 LivingEntity.java:935-950, 1.14.4 :1178-1183, 1.21
        // :1463-1476, 26.3 :1588-1600); 1.8 pushes 0.4F whatever it is given (1.8.8 :816-831)
        for (name, release) in crate::reach_fixtures_e2_tests::releases_from("1.8.8") {
            let bound = super::living_knockback(name.to_string(), NativeVec3 { x: 0.0, y: 0.0, z: 0.0 }, true, 0.4, 1.0, 0.0, 0.0, None)
                .unwrap()
                .expect("a direction of its own");
            let engine = crate::simulation::living_knockback(Vec3::ZERO, true, 0.4, 1.0, 0.0, 0.0, 0.0, release).unwrap();
            assert_eq!((bound.x, bound.y, bound.z), (engine.x, engine.y, engine.z), "{name}");
            let strength = if release.at_least(1, 17, 0) { 0.4 } else { f64::from(0.4_f32) };
            assert_eq!(bound.x, -strength, "{name}");
            // (min(0.4, y / 2 + strength) from 1.14, the float cap before)
            assert_eq!(bound.y, if release.at_least(1, 14, 0) { 0.4 } else { f64::from(0.4_f32) }, "{name}");
        }
    }

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
            water_block: None,
            fall_damage_resetting: None,
            player_fall_damage_resetting: None,
            bounce_restitution: None,
            suppresses_bounce: None,
            suffocating: None,
            in_wall: None,
            bottom_face_sturdy: None,
            explosion_resistance: None,
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
        create_simulation_state(pose_descriptor(Some("1.21.1"), Some(0)))
            .expect("pose fixture should be valid")
    }

    fn boat_vehicle() -> NativeBoatVehicle {
        NativeBoatVehicle {
            bounding_box: NativeAabb {
                min: vec3(-1.0, 0.0, -1.0),
                max: vec3(2.0, 2.0, 2.0),
            },
            underwater: false,
        }
    }

    // D-194: a caller that omits the movement speed gets the player's base, 0.1F widened
    // (26.3 Player.java:206, :613 setBaseValue(abilities.getWalkingSpeed())), not the double 0.1
    #[test]
    fn omitted_movement_speed_is_the_player_base_float_0_1() {
        let config: crate::simulation::PhysicsConfig = NativeSimulationConfig {
            gravity: None, drag: None, movement_speed: None, sprint_multiplier: None,
            jump_velocity: None, step_height: None, negligible_velocity: None,
        }
        .into();
        assert_eq!(config.movement_speed, 0.10000000149011612);
    }

    #[test]
    fn boat_context_round_trips_through_descriptor_and_simulation_state() {
        let mut descriptor = pose_descriptor(Some("26.3"), Some(0));
        descriptor.passenger = Some(true);
        descriptor.boat_vehicle = Some(boat_vehicle());
        let state = create_simulation_state(descriptor).expect("valid boat descriptor");
        let internal = SimulationState::from(state);
        let boat = internal.boat_vehicle.expect("boat context is retained");
        assert_eq!(boat.bounding_box.min, Vec3::new(-1.0, 0.0, -1.0));
        assert_eq!(boat.bounding_box.max, Vec3::new(2.0, 2.0, 2.0));
        assert!(!boat.underwater);
        let state: NativeSimulationState = internal.into();
        assert!(state.passenger.unwrap());
        let mut world = NativeWorld::new();
        let next = simulate_versioned_ticks("26.3".to_string(), &mut world, vec![state], 0, default_config())
            .expect("a zero-tick batch preserves its input").pop().unwrap();
        let returned = next.boat_vehicle.expect("boat context returns through the public batch");
        assert_eq!(Vec3::from(returned.bounding_box.min), boat.bounding_box.min);
        assert_eq!(Vec3::from(returned.bounding_box.max), boat.bounding_box.max);
        assert_eq!(returned.underwater, boat.underwater);
    }

    // D-216: the generic state carries the float degrees of the yaw, which 1.8's moveFlying turns
    // into its own radians (1.8.8 Entity.java:928 `rotationYaw * (float)Math.PI / 180.0F`), so a
    // caller of simulateVersionedTick gets 1.8.8's float rounding, not the radians it rounded itself
    #[test]
    fn the_generic_tick_turns_the_yaw_degrees_into_1_8_radians() {
        // (a yaw whose two radians fall in different entries of the sine table: 1141 and 1140)
        let degrees = 6.2677_f32;
        let run = |yaw: f64, yaw_degrees: Option<f64>| {
            let mut state = create_simulation_state(pose_descriptor(Some("1.8.8"), None)).expect("valid descriptor");
            state.yaw = Some(yaw);
            state.yaw_degrees = yaw_degrees;
            state.on_ground = false;
            state.input = Some(NativeVec3 { x: 0.0, y: 0.0, z: 1.0 });
            let mut world = NativeWorld::new();
            let next = simulate_versioned_tick("1.8.8".to_string(), &mut world, state, default_config()).expect("tick");
            (next.velocity.x, next.velocity.z)
        };
        let double_radians = f64::from(degrees).to_radians();
        let float_radians = f64::from(degrees * 3.1415927 / 180.0);
        assert_ne!(double_radians, float_radians);
        assert_eq!(run(double_radians, Some(f64::from(degrees))), run(float_radians, None));
        assert_ne!(run(double_radians, Some(f64::from(degrees))), run(double_radians, None));
    }

    #[test]
    fn a_non_living_entity_stays_one_through_public_state() {
        for (given, kept) in [(None, false), (Some(false), false), (Some(true), true)] {
            let mut descriptor = pose_descriptor(Some("26.3"), Some(0));
            descriptor.non_living = given;
            let state = create_simulation_state(descriptor).expect("valid descriptor");
            let internal = SimulationState::from(state);
            assert_eq!(internal.non_living, kept);
            let state: NativeSimulationState = internal.into();
            assert_eq!(state.non_living, Some(kept));
            let mut world = NativeWorld::new();
            let next = simulate_versioned_tick(
                "26.3".to_string(), &mut world, state, default_config(),
            ).expect("the tick keeps the entity's family");
            assert_eq!(next.non_living, Some(kept));
        }
    }

    #[test]
    fn movement_authority_preserves_absent_false_and_true_through_public_state() {
        for version in ["26.2", "26.3"] {
            for authority in [None, Some(false), Some(true)] {
                let mut descriptor = pose_descriptor(Some(version), Some(0));
                descriptor.can_simulate_movement = authority;
                let state = create_simulation_state(descriptor).expect("valid descriptor");
                assert_eq!(state.can_simulate_movement, authority);
                let internal = SimulationState::from(state);
                assert_eq!(internal.can_simulate_movement, authority);
                let state: NativeSimulationState = internal.into();
                assert_eq!(state.can_simulate_movement, authority);
                let mut world = NativeWorld::new();
                let mut batch = simulate_versioned_ticks(
                    version.to_string(), &mut world, vec![state], 0, default_config(),
                ).expect("a batch preserves the caller's authority context");
                let state = batch.pop().unwrap();
                assert_eq!(state.can_simulate_movement, authority);
                let next = simulate_versioned_tick(
                    version.to_string(), &mut world, state, default_config(),
                ).expect("tick preserves authority context independently of fluid and travel state");
                assert_eq!(next.can_simulate_movement, authority);
            }
        }
    }

    #[test]
    fn all_simulation_entry_points_reject_malformed_boat_geometry() {
        fn boat_error<T>(result: napi::Result<T>) {
            let error = result.err().expect("malformed boat context must be rejected");
            assert!(error.reason.contains("boatVehicle"), "{}", error.reason);
        }
        for invalid in 0..3 {
            let mut boat = boat_vehicle();
            match invalid {
                0 => boat.bounding_box.min.x = f64::NAN,
                1 => boat.bounding_box.max.y = f64::INFINITY,
                _ => boat.bounding_box.min.z = boat.bounding_box.max.z + 1.0,
            }
            let state = || {
                let mut state = pose_state();
                state.passenger = Some(true);
                state.boat_vehicle = Some(boat.clone());
                state
            };
            let version = || "26.3".to_string();
            let mut world = NativeWorld::new();
            boat_error(simulate_versioned_tick(version(), &mut world, state(), default_config()));
            boat_error(simulate_versioned_tick_outcome(version(), &mut world, state(), default_config()));
            boat_error(simulate_versioned_ticks(version(), &mut world, vec![state()], 0, default_config()));
            boat_error(simulate_versioned_piston_push(version(), &mut world, state(), default_config(), None));
            boat_error(refresh_versioned_pose(version(), &mut world, state(), 0, 0, false, None));
            boat_error(simulate_versioned_entity_ticks(version(), &world, vec![NativeVersionedEntityState {
                simulation: state(), id: None, no_physics: false, vehicle_id: None,
                pushable: true, pushthrough: 0.0,
            }], 0, default_config()));
            let mut descriptor = pose_descriptor(Some("26.3"), Some(0));
            descriptor.boat_vehicle = Some(boat);
            boat_error(create_simulation_state(descriptor));
        }
    }

    fn pose_descriptor(version: Option<&str>, pose: Option<u8>) -> NativeEntityDescriptor {
        NativeEntityDescriptor {
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
            version: version.map(str::to_string),
            pose,
            pitch: None,
            input: None,
            jump_requested: None,
            jump_ticks: None,
            sprinting: None,
            sneaking: None,
            using_item: None,
            swimming: None,
            passenger: None,
            boat_vehicle: None,
            can_simulate_movement: None,
            non_living: None,
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
        }
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

    fn saddled_horse(movement_speed: Option<f64>, jump_strength: Option<f64>) -> super::NativeHorseState {
        super::NativeHorseState {
            movement_speed,
            jump_strength,
            entity_type: Some("horse".to_string()),
            ..super::native_ridden_mob_state(
                crate::ridden::RiddenMobState::new(
                    crate::ridden::RiddenMobKind::Horse,
                    crate::Vec3::new(0.5, 1.0, 0.5),
                ),
                crate::version::MinecraftVersion::new(1, 20, 4),
            )
        }
    }

    #[test]
    fn a_ridden_mob_exists_from_the_release_its_entity_type_arrives() {
        let mob = |name: &str| super::NativeHorseState {
            entity_type: Some(name.to_string()),
            ..saddled_horse(None, None)
        };
        let version = crate::version::MinecraftVersion::new;
        // the name, the last release without it, the first with it
        for (name, last, first) in [
            ("llama", version(1, 10, 2), version(1, 11, 0)),
            ("trader_llama", version(1, 13, 2), version(1, 14, 0)),
            ("strider", version(1, 15, 2), version(1, 16, 0)),
            ("camel", version(1, 19, 2), version(1, 19, 3)),
            ("happy_ghast", version(1, 21, 5), version(1, 21, 6)),
            ("nautilus", version(1, 21, 10), version(1, 21, 11)),
            ("zombie_nautilus", version(1, 21, 10), version(1, 21, 11)),
            ("camel_husk", version(1, 21, 10), version(1, 21, 11)),
        ] {
            let refused = super::ridden_mob_state(mob(name), last).err().expect(name);
            assert_eq!(refused.reason, format!("{name} is not in {last}: it arrives in {first}"));
            assert!(super::ridden_mob_state(mob(name), first).is_ok(), "{name}");
            assert!(super::ridden_mob_state(mob(name), version(26, 3, 0)).is_ok(), "{name}");
        }
        // (a variant of the horse before it is a type of its own)
        for name in ["pig", "horse", "donkey", "mule", "skeleton_horse", "zombie_horse"] {
            assert!(super::ridden_mob_state(mob(name), version(1, 8, 8)).is_ok(), "{name}");
        }
        let unknown = super::ridden_mob_state(mob("cow"), version(26, 3, 0)).err().unwrap();
        assert_eq!(unknown.reason, "no ridden mob simulation for cow");
    }

    #[test]
    fn a_ridden_mobs_zero_attributes_are_attributes() {
        // MOVEMENT_SPEED and JUMP_STRENGTH go down to 0 (26.3 Attributes.java:73, 80); only an
        // attribute not given takes the base
        let release = crate::version::MinecraftVersion::new(1, 20, 4);
        let zero = super::ridden_mob_state(saddled_horse(Some(0.0), Some(0.0)), release).unwrap();
        assert_eq!((zero.movement_speed, zero.jump_strength), (0.0, 0.0));
        let base = crate::ridden::RiddenMobState::new(
            crate::ridden::RiddenMobKind::Horse,
            crate::Vec3::ZERO,
        );
        let unset = super::ridden_mob_state(saddled_horse(None, None), release).unwrap();
        assert_eq!(unset.movement_speed, base.movement_speed);
        assert_eq!(unset.jump_strength, base.jump_strength);
        for (speed, jump) in [(-0.1, 0.5), (0.1, -0.5), (f64::NAN, 0.5), (0.1, f64::INFINITY)] {
            assert!(super::ridden_mob_state(saddled_horse(Some(speed), Some(jump)), release).is_err());
        }
    }

    /// A stone floor at y 0 and a column of cobweb at x 2, for a ridden mob at y 1.
    fn web_column_world() -> NativeWorld {
        let mut world = NativeWorld::new();
        for x in -3..=8 {
            for z in -3..=3 {
                world.set_full_block(x, 0, z, 1);
            }
        }
        for z in -3..=3 {
            for y in 1..=3 {
                world
                    .set_block_shape(2, y, z, 99, block_shape((0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 4))
                    .unwrap();
            }
        }
        world
    }

    fn walking_horse(release: crate::version::MinecraftVersion) -> super::NativeHorseState {
        let mut mob = crate::ridden::RiddenMobState::new(
            crate::ridden::RiddenMobKind::Horse,
            crate::Vec3::new(0.2, 1.0, 0.5),
        );
        mob.on_ground = true;
        mob.velocity.y = -0.0784;
        mob.yaw = -90.0;
        mob.rider.yaw = -90.0;
        mob.rider.forward = 1.0;
        super::native_ridden_mob_state(mob, release)
    }

    #[test]
    fn a_ridden_mobs_stuck_speed_crosses_public_calls_as_it_crosses_a_batch() {
        // a multiplier that lived only inside one call would pass on the batch and fail here: the
        // output of each call is the input of the next (26.3 Entity.java:783-790)
        for version in ["1.9.4", "1.20.4", "1.21.9", "26.3"] {
            let release = super::vehicle_release(version, "ridden mob").unwrap();
            let mut world = web_column_world();
            let mut repeated = walking_horse(release);
            let mut stuck_ticks = 0;
            for ticks in 1..=60_u32 {
                repeated =
                    super::simulate_versioned_horse_tick(version.to_string(), &mut world, repeated)
                        .unwrap();
                let batch = super::simulate_versioned_horse_ticks(
                    version.to_string(),
                    &mut world,
                    vec![walking_horse(release)],
                    ticks,
                )
                .unwrap();
                let (batch, repeated) = (&batch[0], &repeated);
                let (a, b) = (&batch.position, &repeated.position);
                assert_eq!((a.x, a.y, a.z), (b.x, b.y, b.z), "{version} tick {ticks}");
                let left = |state: &super::NativeHorseState| {
                    let vec = state.stuck_speed_multiplier.as_ref().unwrap();
                    (vec.x, vec.y, vec.z)
                };
                assert_eq!(left(batch), left(repeated), "{version} tick {ticks}");
                if left(repeated).0 != 0.0 {
                    stuck_ticks += 1;
                }
            }
            assert!(stuck_ticks > 3, "{version}: {stuck_ticks}");
        }
    }

    #[test]
    fn a_ridden_mobs_weaving_and_stuck_speed_come_back() {
        let release = crate::version::MinecraftVersion::new(1, 21, 9);
        let mob = super::ridden_mob_state(
            super::NativeHorseState {
                weaving: Some(true),
                stuck_speed_multiplier: Some(vec3(0.5, 0.25, 0.5)),
                ..saddled_horse(None, None)
            },
            release,
        )
        .unwrap();
        assert!(mob.weaving);
        let back = super::native_ridden_mob_state(mob, release);
        assert_eq!(back.weaving, Some(true));
        assert_eq!(back.stuck_speed_multiplier.unwrap().x, 0.5);
    }

    fn stone_floor_world() -> NativeWorld {
        let mut world = NativeWorld::new();
        for x in -3..=8 {
            for z in -3..=3 {
                world.set_full_block(x, 0, z, 1);
            }
        }
        world
    }

    #[test]
    fn a_batch_of_ridden_mobs_keeps_their_zero_attributes() {
        // MOVEMENT_SPEED and JUMP_STRENGTH go down to 0 in every release that has them (1.9.4
        // SharedMonsterAttributes.java:24, AbstractHorse.java:66; 1.20.4 Attributes.java:25, 39;
        // 1.20.5 :21-24, 26-30; 26.3 Attributes.java:73, 80), and the batch is the public call repeated
        for version in ["1.9.4", "1.20.4", "1.21.9", "26.3"] {
            let release = super::vehicle_release(version, "ridden mob").unwrap();
            let mut world = stone_floor_world();
            let with = |speed, jump| super::NativeHorseState {
                movement_speed: speed,
                jump_strength: jump,
                jump_pending_scale: Some(1.0),
                ..walking_horse(release)
            };
            let ticks = super::simulate_versioned_horse_ticks(
                version.to_string(),
                &mut world,
                vec![with(Some(0.0), Some(0.0)), with(None, None), with(Some(0.001), Some(0.001))],
                1,
            )
            .unwrap();
            let [zero, default, tiny] = [&ticks[0], &ticks[1], &ticks[2]];
            assert_eq!(zero.position.y, 1.0, "{version}");
            assert_eq!(zero.movement_speed, Some(0.0), "{version}");
            assert_eq!(zero.jump_strength, Some(0.0), "{version}");
            let base = crate::ridden::RiddenMobKind::Horse.movement_speed();
            assert_eq!(default.movement_speed, Some(base), "{version}");
            assert_eq!(default.jump_strength, Some(0.41999998688697815), "{version}");
            assert!(default.position.y > 1.4, "{version}: {}", default.position.y);
            // (the third by f32 from 1.20.5: 1.20.5 LivingEntity.java:2065-2068)
            assert!((tiny.position.y - 1.001).abs() < 1.0e-6, "{version}: {}", tiny.position.y);
            assert!(tiny.position.y < default.position.y, "{version}");
            let single = super::simulate_versioned_horse_tick(
                version.to_string(),
                &mut world,
                with(Some(0.0), Some(0.0)),
            )
            .unwrap();
            assert_eq!(single.position.x, zero.position.x, "{version}");
            assert_eq!(single.velocity.y, zero.velocity.y, "{version}");
        }
    }

    #[test]
    fn a_ridden_mobs_attributes_above_their_maximum_are_refused() {
        // no RangedAttribute keeps more than its maximum (26.3 RangedAttribute.java:29-31):
        // MOVEMENT_SPEED 1024; JUMP_STRENGTH 2 to 1.20.4, 32 from 1.20.5
        let mut world = stone_floor_world();
        for (version, major, minor, patch, top_jump) in
            [("1.20.4", 1, 20, 4, 2.0), ("1.20.5", 1, 20, 5, 32.0), ("26.3", 26, 3, 0, 32.0)]
        {
            let release = crate::version::MinecraftVersion::new(major, minor, patch);
            let mut tick = |speed: f64, jump: f64| {
                let state = super::NativeHorseState {
                    movement_speed: Some(speed),
                    jump_strength: Some(jump),
                    ..walking_horse(release)
                };
                super::simulate_versioned_horse_tick(version.to_string(), &mut world, state)
            };
            assert!(tick(1024.0, top_jump).is_ok(), "{version}");
            assert!(tick(1024.0000001, 0.5).is_err(), "{version}");
            assert!(tick(1.0e12, 0.5).is_err(), "{version}");
            assert!(tick(0.1, top_jump + 1.0e-9).is_err(), "{version}");
            let batch = super::simulate_versioned_horse_ticks(
                version.to_string(),
                &mut world,
                vec![super::NativeHorseState {
                    movement_speed: Some(2048.0),
                    ..walking_horse(release)
                }],
                1,
            );
            assert!(batch.is_err(), "{version}");
        }
    }

    #[test]
    fn a_ridden_mobs_invalid_attributes_are_refused_alone_and_in_a_batch() {
        // nothing a RangedAttribute keeps can be negative or not finite, so the effective value
        // given here is not one (1.20.5 RangedAttribute.java:30-31, 26.3
        // RangedAttribute.java:29-31)
        let mut world = stone_floor_world();
        let release = crate::version::MinecraftVersion::new(1, 21, 9);
        for (speed, jump) in [
            (-1.0e-9, 0.5),
            (0.1, -1.0e-9),
            (f64::NAN, 0.5),
            (0.1, f64::NAN),
            (f64::INFINITY, 0.5),
            (0.1, f64::NEG_INFINITY),
        ] {
            let state = || super::NativeHorseState {
                movement_speed: Some(speed),
                jump_strength: Some(jump),
                ..walking_horse(release)
            };
            let single =
                super::simulate_versioned_horse_tick("1.21.9".to_string(), &mut world, state());
            assert!(single.is_err(), "{speed} {jump}");
            let batch = super::simulate_versioned_horse_ticks(
                "1.21.9".to_string(),
                &mut world,
                vec![walking_horse(release), state()],
                1,
            );
            assert!(batch.is_err(), "{speed} {jump}");
            assert!(super::simulate_horse_tick(state()).is_err(), "{speed} {jump}");
        }
    }

    #[test]
    fn the_horse_without_a_world_keeps_its_zero_attributes() {
        // tick_horse's speed and jump are the attributes too (1.21.11 AbstractHorse.java:733-759)
        let state = super::NativeHorseState {
            movement_speed: Some(0.0),
            jump_strength: Some(0.0),
            forward: 1.0,
            jump_requested: true,
            ..saddled_horse(None, None)
        };
        let out = super::simulate_horse_tick(state).unwrap();
        assert_eq!(out.movement_speed, Some(0.0));
        assert_eq!(out.jump_strength, Some(0.0));
        assert_eq!(out.position.y, 1.0);
    }

    #[test]
    fn a_ridden_mobs_size_crosses_public_calls_as_it_crosses_a_batch() {
        // a measured size of no round factor, whose output is the input of the next call: the box
        // of every tick is the same as the batch's (1.20.5 LivingEntity.java:3264-3269)
        for version in ["1.20.5", "1.21.9", "26.3"] {
            let release = super::vehicle_release(version, "ridden mob").unwrap();
            let mut world = stone_floor_world();
            let start = || super::NativeHorseState {
                half_width: Some(f64::from(1.3964844_f32 * 1.37 / 2.0)),
                height: Some(f64::from(1.6_f32 * 1.37)),
                ..walking_horse(release)
            };
            let (first_half, first_height) = (start().half_width, start().height);
            let mut repeated = start();
            for ticks in 1..=20_u32 {
                repeated =
                    super::simulate_versioned_horse_tick(version.to_string(), &mut world, repeated)
                        .unwrap();
                let batch =
                    super::simulate_versioned_horse_ticks(version.to_string(), &mut world, vec![start()], ticks)
                        .unwrap();
                assert_eq!(repeated.half_width, batch[0].half_width, "{version} {ticks}");
                assert_eq!(repeated.height, batch[0].height, "{version} {ticks}");
                assert_eq!(repeated.position.x, batch[0].position.x, "{version} {ticks}");
                assert_eq!(repeated.half_width, first_half, "{version} {ticks}");
                assert_eq!(repeated.height, first_height, "{version} {ticks}");
            }
        }
    }

    #[test]
    fn a_ridden_mobs_effects_and_travel_attributes_come_back() {
        let release = crate::version::MinecraftVersion::new(26, 3, 0);
        let mob = super::ridden_mob_state(
            super::NativeHorseState {
                slow_falling: Some(true),
                levitation_amplifier: Some(2),
                dolphins_grace: Some(true),
                gravity: Some(-0.25),
                step_height: Some(2.5),
                water_movement_efficiency: Some(0.5),
                air_drag_modifier: Some(0.0),
                friction_modifier: Some(0.25),
                ..saddled_horse(None, None)
            },
            release,
        )
        .unwrap();
        let back = super::native_ridden_mob_state(mob, release);
        assert_eq!(back.slow_falling, Some(true));
        assert_eq!(back.levitation_amplifier, Some(2));
        assert_eq!(back.dolphins_grace, Some(true));
        assert_eq!(back.gravity, Some(-0.25));
        assert_eq!(back.step_height, Some(2.5));
        assert_eq!(back.water_movement_efficiency, Some(0.5));
        assert_eq!(back.air_drag_modifier, Some(0.0));
        assert_eq!(back.friction_modifier, Some(0.25));
        let plain = super::ridden_mob_state(saddled_horse(None, None), release).unwrap();
        let plain = super::native_ridden_mob_state(plain, release);
        assert_eq!(plain.gravity, None);
        assert_eq!(plain.air_drag_modifier, None);
    }

    #[test]
    fn a_ridden_mobs_attributes_of_no_number_stay_out_of_its_motion() {
        // GRAVITY, STEP_HEIGHT and the modifiers are sanitized as RangedAttribute does (26.3
        // RangedAttribute.java:29-31), so a batch of them gives finite motion
        let mut world = stone_floor_world();
        let release = crate::version::MinecraftVersion::new(26, 3, 0);
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let state = super::NativeHorseState {
                gravity: Some(value),
                step_height: Some(value),
                water_movement_efficiency: Some(value),
                air_drag_modifier: Some(value),
                friction_modifier: Some(value),
                ..walking_horse(release)
            };
            let out =
                super::simulate_versioned_horse_ticks("26.3".to_string(), &mut world, vec![state], 5)
                    .unwrap();
            let (position, velocity) = (&out[0].position, &out[0].velocity);
            for axis in [position.x, position.y, position.z, velocity.x, velocity.y, velocity.z] {
                assert!(axis.is_finite(), "{value}");
            }
        }
    }

    #[test]
    fn a_ridden_mobs_flying_speed_is_an_attribute_too() {
        // FLYING_SPEED keeps 0 to 1024 (26.3 Attributes.java:69): 0 flies nowhere, and what no
        // RangedAttribute keeps is refused with the others
        let release = crate::version::MinecraftVersion::new(26, 3, 0);
        let ghast = |speed| super::NativeHorseState {
            entity_type: Some("happy_ghast".to_string()),
            flying_speed: Some(speed),
            ..saddled_horse(None, None)
        };
        assert_eq!(super::ridden_mob_state(ghast(0.0), release).unwrap().flying_speed, 0.0);
        for speed in [-0.1, f64::NAN, f64::INFINITY] {
            assert!(super::ridden_mob_state(ghast(speed), release).is_err(), "{speed}");
        }
    }

    #[test]
    fn a_ridden_mobs_measured_dimensions_come_back() {
        let release = crate::version::MinecraftVersion::new(1, 21, 9);
        let mob = super::ridden_mob_state(
            super::NativeHorseState {
                half_width: Some(1.396484375),
                height: Some(f64::from(3.2_f32)),
                ..saddled_horse(None, None)
            },
            release,
        )
        .unwrap();
        let back = super::native_ridden_mob_state(mob, release);
        assert_eq!(back.half_width, Some(1.396484375));
        assert_eq!(back.height, Some(f64::from(3.2_f32)));
        let plain = super::ridden_mob_state(saddled_horse(None, None), release).unwrap();
        let plain = super::native_ridden_mob_state(plain, release);
        assert_eq!(plain.half_width, Some(0.6982421875));
    }

    #[test]
    fn versioned_fluid_states_round_trip_source_and_flowing_cells() {
        let mut world = NativeWorld::new();
        let still = vec3(0.0, 0.0, 0.0);
        // 1.20.6 flowing_water: LEVEL = amount 1-8 and FALLING, sorted FALLING, LEVEL
        // (StateDefinition), so legacy level 2 (amount 6) is index 5 and a falling cell index 15.
        assert_eq!(
            generated_fluid_state_index_for("1.20.6", FluidState::Water, 2, false, false).unwrap(),
            Some(5)
        );
        assert_eq!(
            generated_fluid_state_index_for("1.20.6", FluidState::Water, 9, false, true).unwrap(),
            Some(15)
        );
        assert_eq!(
            generated_fluid_state_index_for("1.20.6", FluidState::Lava, 0, true, false).unwrap(),
            Some(0)
        );
        world
            .set_versioned_fluid_state(0, 0, 0, "1.20.6".into(), 1, 0, 2, false, false, still)
            .expect("flowing water is a generated 1.20.6 state");
        let block = world.inner.block((0, 0, 0));
        assert!(!block.fluid_source);
        assert_eq!(block.fluid_level, 2);
        world
            .set_versioned_fluid_state(0, 0, 0, "1.20.6".into(), 1, 0, 0, true, false, still)
            .expect("source water is a generated 1.20.6 state");
        assert!(world.inner.block((0, 0, 0)).fluid_source);
        // Empty cells never carry the source or falling bit, on any write path.
        assert!(world.set_fluid_state(1, 0, 0, 0, 0, 0, true, false, still).is_err());
        assert!(world.set_fluid_state(1, 0, 0, 0, 0, 0, false, true, still).is_err());
        assert!(world
            .set_versioned_fluid_state(1, 0, 0, "1.20.6".into(), 0, 0, 0, true, false, still)
            .is_err());
        assert!(!world.inner.contains_block((1, 0, 0)));
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
            team_pushes: None,
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
            next_shapes: None,
            outline_shapes: None,
            bounces: None,
            sticky_for_entities: None,
        }
    }

    #[test]
    fn tick_snapshots_carry_the_whole_scene_of_a_client_tick() {
        use crate::simulation::{TickInputSnapshot, TickScene};
        use crate::versioned::v1_21_1;
        // the scene simulateVersionedTick reads beside the state: a cow before the player and one
        // after it, an attached rocket and a moving piston head (as in the tests above)
        let cow = |x: f64, before| {
            nearby_entity(
                "living",
                vec3(x, 0.0, -0.45),
                vec3(x + 0.9, 1.4, 0.45),
                before,
            )
        };
        let scene_state = || {
            let mut state = pose_state();
            state.nearby_entities = Some(vec![cow(0.25, true), cow(-1.15, false)]);
            state.moving_pistons = Some(vec![moving_head(5)]);
            state.fall_flying = Some(true);
            state.on_ground = false;
            state.position = vec3(0.0, 0.0, 0.0);
            state.look = Some(vec3(0.0, 0.0, 1.0));
            state.firework_rockets = Some(2);
            state.firework_rockets_before_player = Some(1);
            // the rocket before the player has its last tick left
            state.firework_rocket_ticks = Some(vec![1, 3]);
            state
        };
        let mut world = NativeWorld::new();
        let ticked =
            simulate_versioned_tick("1.21.1".to_string(), &mut world, scene_state(), default_config())
                .expect("1.21.1 is supported");
        let twice = simulate_versioned_ticks(
            "1.21.1".to_string(),
            &mut world,
            vec![scene_state()],
            2,
            default_config(),
        )
        .expect("1.21.1 is supported")
        .remove(0);

        let mut native = scene_state();
        let nearby_entities: Vec<NearbyEntity> = native
            .nearby_entities
            .take()
            .unwrap()
            .into_iter()
            .map(Into::into)
            .collect();
        let moving_pistons: Vec<MovingPiston> = native
            .moving_pistons
            .take()
            .unwrap()
            .into_iter()
            .map(|piston| MovingPiston::try_from(piston).unwrap())
            .collect();
        let state: SimulationState = native.into();
        let input = TickInputSnapshot::for_profile::<v1_21_1::Profile>(
            state,
            world.snapshot().clone(),
            PhysicsConfig::default(),
        )
        .with_scene(TickScene {
            movement_speed: None,
            nearby_entities,
            firework_rockets: 2,
            firework_rockets_before_player: 1,
            rocket_ticks: Some(vec![1, 3]),
            auto_spin_attack: false,
            moving_pistons: moving_pistons.clone(),
            piston_deltas: [0.0; 3],
        });
        let output = input.execute();

        // the snapshot's tick is the client's: the same motion as simulateVersionedTick's
        assert_eq!(output.state.position.x, ticked.position.x);
        assert_eq!(output.state.position.y, ticked.position.y);
        assert_eq!(output.state.position.z, ticked.position.z);
        assert_eq!(output.state.velocity.x, ticked.velocity.x);
        assert_eq!(output.state.velocity.y, ticked.velocity.y);
        assert_eq!(output.state.velocity.z, ticked.velocity.z);
        // and it writes what that tick leaves beside the state: the cow before the player took a
        // push back, and the piston's push is in the deltas (Entity.pistonDeltas)
        assert_eq!(output.entity_pushes.first().map(|push| push.index), Some(0));
        assert!(output.piston_deltas[0] > 0.0);
        assert!(!output.auto_spin_attack_ended);
        // and the scene the next tick reads: the piston moved on by min(progress + 0.5F, 1)
        // (1.21.1 PistonMovingBlockEntity.java tick), the rocket before the player ended
        // (FireworkRocketEntity.tick), so one is left, after the player, and the piston deltas
        // start over with the next game time (1.21.1 Entity.java limitPistonMovement)
        assert_eq!(
            output.scene.moving_pistons[0].progress,
            (moving_pistons[0].progress + 0.5).min(1.0)
        );
        assert_eq!(output.scene.rocket_ticks, Some(vec![2]));
        assert_eq!(
            (
                output.scene.firework_rockets,
                output.scene.firework_rockets_before_player
            ),
            (1, 0)
        );
        assert_eq!(output.scene.piston_deltas, [0.0; 3]);
        // so a second tick runs from the first's output as simulateVersionedTicks runs it
        let second = input.next(&output).execute();
        assert_eq!(second.state.position.x, twice.position.x);
        assert_eq!(second.state.position.y, twice.position.y);
        assert_eq!(second.state.position.z, twice.position.z);
        assert_eq!(second.state.velocity.x, twice.velocity.x);
        assert_eq!(second.state.velocity.y, twice.velocity.y);
        assert_eq!(second.state.velocity.z, twice.velocity.z);
        // simulateVersionedTickOutcome writes the same over N-API, its state the next tick's
        let outcome = simulate_versioned_tick_outcome(
            "1.21.1".to_string(),
            &mut world,
            scene_state(),
            default_config(),
        )
        .expect("1.21.1 is supported");
        assert_eq!(outcome.state.position.y, output.state.position.y);
        assert_eq!(outcome.state.velocity.x, output.state.velocity.x);
        assert_eq!(
            outcome
                .entity_pushes
                .iter()
                .map(|push| (push.index as usize, push.x, push.z))
                .collect::<Vec<_>>(),
            output
                .entity_pushes
                .iter()
                .map(|push| (push.index, push.x, push.z))
                .collect::<Vec<_>>()
        );
        assert_eq!(outcome.piston_deltas.x, output.piston_deltas[0]);
        assert!(!outcome.auto_spin_attack_ended);
        assert_eq!(outcome.state.firework_rocket_ticks, Some(vec![2]));
        assert_eq!(
            outcome.state.moving_pistons.as_ref().map(|pistons| pistons[0].progress),
            Some(f64::from(output.scene.moving_pistons[0].progress))
        );
        let again = simulate_versioned_tick_outcome(
            "1.21.1".to_string(),
            &mut world,
            outcome.state,
            default_config(),
        )
        .expect("1.21.1 is supported");
        assert_eq!(again.state.position.x, twice.position.x);
        assert_eq!(again.state.position.y, twice.position.y);
        assert_eq!(again.state.velocity.y, twice.velocity.y);
        // the input is left as it was, so the snapshot replays
        assert_eq!(input.execute(), output);
        assert_eq!(input.state, state);
    }

    #[test]
    fn the_spin_attack_sweeps_from_the_box_past_the_riptide_lift() {
        use crate::simulation::{TickInputSnapshot, TickScene};
        use crate::versioned::{v1_16_5, v1_21_1};
        // TridentItem.releaseUsing lifts a grounded player 1.2 before the level ticks (1.16.5
        // Minecraft.java:1420, 1587, TridentItem.java:95), and aiStep takes the box the
        // spin attack sweeps from after it (1.16.5 LivingEntity.java:2504-2510): a living entity
        // at the feet the launch left is not met
        let at_feet = Aabb::new(Vec3::new(-0.2, 0.1, -0.2), Vec3::new(0.2, 0.5, 0.2));
        let stand = NearbyEntity {
            kind: NearbyEntityKind::StillLiving,
            position: Vec3::new(0.0, 0.1, 0.0),
            bounding_box: at_feet,
            tick_position: Vec3::new(0.0, 0.1, 0.0),
            tick_bounding_box: at_feet,
            ticks_before_player: false,
            team_pushes: true,
            spectator: false,
        };
        let mut world = WorldSnapshot::new();
        world.set_block((0, -1, 0), BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        });
        let launched = |riptide: Option<u8>| SimulationState {
            position: Vec3::ZERO,
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            in_water: true,
            is_player: true,
            pitch: -std::f64::consts::FRAC_PI_2,
            look: Vec3::new(0.0, 1.0, 0.0),
            riptide_launch: riptide,
            ..SimulationState::default()
        };
        let scene = TickScene {
            nearby_entities: vec![stand],
            auto_spin_attack: true,
            ..TickScene::default()
        };
        let spin = |state: SimulationState, kept: bool| {
            let mut state = state;
            if kept {
                state.bounding_box = Some(state.bounds_around_position());
                TickInputSnapshot::for_profile::<v1_16_5::Profile>(state, world.clone(), PhysicsConfig::default())
            } else {
                TickInputSnapshot::for_profile::<v1_21_1::Profile>(state, world.clone(), PhysicsConfig::default())
            }
            .with_scene(scene.clone())
            .execute()
        };
        for kept in [true, false] {
            let lifted = spin(launched(Some(1)), kept);
            assert_eq!(lifted.state.riptide_lift.y, 1.1999999284744263, "kept {kept}");
            assert!(!lifted.auto_spin_attack_ended, "kept {kept}");
            assert!(lifted.state.velocity.y > 0.0, "kept {kept}");
            // without the launch the swept box starts at the feet, and the stand bounces it
            let grounded = spin(launched(None), kept);
            assert_eq!(grounded.state.riptide_lift, Vec3::ZERO);
            assert!(grounded.auto_spin_attack_ended, "kept {kept}");
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
    fn bulk_ticks_are_single_ticks_in_the_states_scene() {
        // every client tick the attached rockets boost the player (FireworkRocketEntity.tick,
        // 1.21.11 FireworkRocketEntity.java:116-128), the nearby entities push it
        // (LivingEntity.pushEntities, 1.21.11 LivingEntity.java:3528-3558) and the moving pistons
        // push it (PistonMovingBlockEntity.tick, 1.21.11 PistonMovingBlockEntity.java:274-290),
        // their progress 0.5 further each tick up to 1 (:294-326)
        let gliding = || {
            let mut state = pose_state();
            state.on_ground = false;
            state.position = vec3(0.0, 10.0, 0.0);
            state.fall_flying = Some(true);
            state.look = Some(vec3(0.0, 0.0, 1.0));
            state.firework_rockets = Some(1);
            state.firework_rockets_before_player = Some(1);
            state.nearby_entities = Some(vec![nearby_entity(
                "living",
                vec3(0.25, 10.0, -0.45),
                vec3(1.15, 11.4, 0.45),
                false,
            )]);
            state
        };
        let pushed = || {
            let mut state = pose_state();
            state.moving_pistons = Some(vec![moving_head(5)]);
            state
        };
        for scene in [&gliding as &dyn Fn() -> NativeSimulationState, &pushed] {
            let mut world = NativeWorld::new();
            let mut single = scene();
            for tick in 1..=3 {
                let mut next =
                    simulate_versioned_tick("1.21.1".to_string(), &mut world, single, default_config())
                        .expect("1.21.1 is supported");
                let again = scene();
                next.nearby_entities = again.nearby_entities;
                next.firework_rockets = again.firework_rockets;
                next.firework_rockets_before_player = again.firework_rockets_before_player;
                next.moving_pistons = again.moving_pistons.map(|pistons| {
                    pistons
                        .into_iter()
                        .map(|mut piston| {
                            piston.progress = (0.5 * f64::from(tick)).min(1.0);
                            piston
                        })
                        .collect()
                });
                single = next;
            }
            let bulk = simulate_versioned_ticks(
                "1.21.1".to_string(),
                &mut world,
                vec![scene()],
                3,
                default_config(),
            )
            .expect("1.21.1 is supported")
            .remove(0);
            let unscened = simulate_versioned_ticks(
                "1.21.1".to_string(),
                &mut world,
                vec![{
                    let mut state = scene();
                    state.nearby_entities = None;
                    state.firework_rockets = None;
                    state.moving_pistons = None;
                    state
                }],
                3,
                default_config(),
            )
            .expect("1.21.1 is supported")
            .remove(0);
            assert_eq!(bulk.position.x, single.position.x);
            assert_eq!(bulk.position.y, single.position.y);
            assert_eq!(bulk.position.z, single.position.z);
            assert_eq!(bulk.velocity.x, single.velocity.x);
            assert_eq!(bulk.velocity.z, single.velocity.z);
            assert!(
                bulk.position.x != unscened.position.x || bulk.position.z != unscened.position.z
            );
        }
    }

    #[test]
    fn bulk_ticks_move_the_pistons_on_two_half_steps_then_end_them() {
        // PistonMovingBlockEntity.tick: progressO = progress; at 1 the block ends, pushing
        // nothing; else it pushes over progress .. progress + 0.5F and keeps that, at most 1
        // (1.21.11 PistonMovingBlockEntity.java:286-319)
        let positions = (1..=4)
            .map(|ticks| {
                let mut state = pose_state();
                state.moving_pistons = Some(vec![moving_head(5)]);
                simulate_versioned_ticks(
                    "1.21.1".to_string(),
                    &mut NativeWorld::new(),
                    vec![state],
                    ticks,
                    default_config(),
                )
                .expect("1.21.1 is supported")
                .remove(0)
                .position
                .x
            })
            .collect::<Vec<_>>();
        // the head's plate sweeps x -0.25 .. 0.25 and moves the player's box (x -0.3 .. 0.3) out
        // by 0.51; then 0.25 .. 1, Entity.move(PISTON) limiting the 0.8 out to 0.51 again; then it
        // ends (the frozen progress pushed 0.05 more and stopped at 0.56)
        let pushed = 0.5 + 0.01;
        assert_eq!(positions, [pushed, pushed + 0.51, pushed + 0.51, pushed + 0.51]);
    }

    /// `ticks` single ticks of `scene`, the scene of tick n given by `scene_of(n, state)`.
    fn single_ticks(
        version: &str,
        ticks: u32,
        start: NativeSimulationState,
        scene_of: impl Fn(u32, &mut NativeSimulationState),
    ) -> NativeSimulationState {
        let mut world = NativeWorld::new();
        let mut state = start;
        for tick in 1..=ticks {
            scene_of(tick, &mut state);
            state = simulate_versioned_tick(version.to_string(), &mut world, state, default_config())
                .expect("supported release");
        }
        state
    }

    #[test]
    fn bulk_ticks_drop_a_piston_after_its_ending_tick_before_1_11() {
        // 1.8.8 PistonMovingBlockEntity.tick: at progress 1 it pushes a last time, pushEntities(1.0F,
        // 0.25F), and removes the block entity (PistonMovingBlockEntity.java:123-131; 1.10.2 :167-172),
        // so the tick after pushes nothing; from 1.11 the ended block pushes nothing at all
        // (1.12.2 :277-285)
        let start = || {
            let mut state = pose_state();
            state.position = vec3(0.9, 0.0, 0.5);
            state
        };
        let ended = || {
            let mut piston = moving_head(5);
            piston.progress = 1.0;
            piston
        };
        for version in ["1.8.8", "1.9.4", "1.10.2", "1.12.2"] {
            let bulk = simulate_versioned_ticks(
                version.to_string(),
                &mut NativeWorld::new(),
                vec![{
                    let mut state = start();
                    state.moving_pistons = Some(vec![ended()]);
                    state
                }],
                3,
                default_config(),
            )
            .expect("supported release")
            .remove(0);
            let single = single_ticks(version, 3, start(), |tick, state| {
                state.moving_pistons = (tick == 1).then(|| vec![ended()]);
            });
            assert_eq!(bulk.position.x, single.position.x, "{version}");
            assert_eq!(bulk.position.y, single.position.y, "{version}");
            assert_eq!(bulk.velocity.x, single.velocity.x, "{version}");
            let kept = single_ticks(version, 3, start(), |_, state| {
                state.moving_pistons = Some(vec![ended()]);
            });
            if version == "1.12.2" {
                assert_eq!(bulk.position.x, kept.position.x, "{version}");
            } else {
                assert!(bulk.position.x > start().position.x, "{version}");
            }
            if version == "1.8.8" {
                // kept at 1, the 0.25 ending push of the whole cell would repeat every tick (from
                // 1.9 it moves the player out of the head's box, which then stays clear)
                assert!(kept.position.x > bulk.position.x, "{version}");
            }
        }
    }

    #[test]
    fn bulk_ticks_end_the_attached_rockets_after_their_ticks() {
        // FireworkRocketEntity.tick boosts the gliding player, then life++, and the rocket ends
        // once life > lifetime (1.21.11 FireworkRocketEntity.java:119-126, 154-165)
        let gliding = || {
            let mut state = pose_state();
            state.on_ground = false;
            state.position = vec3(0.0, 10.0, 0.0);
            state.fall_flying = Some(true);
            state.look = Some(vec3(0.0, 0.0, 1.0));
            state
        };
        let bulk = |rocket_ticks: Vec<u32>, before: u32| {
            simulate_versioned_ticks(
                "1.21.1".to_string(),
                &mut NativeWorld::new(),
                vec![{
                    let mut state = gliding();
                    state.firework_rockets_before_player = Some(before);
                    state.firework_rocket_ticks = Some(rocket_ticks);
                    state
                }],
                4,
                default_config(),
            )
            .expect("1.21.1 is supported")
            .remove(0)
        };
        let ended = bulk(vec![2, 3], 1);
        let single = single_ticks("1.21.1", 4, gliding(), |tick, state| {
            state.firework_rockets = Some(match tick {
                1 | 2 => 2,
                3 => 1,
                _ => 0,
            });
            // the rocket before the player ends after tick 2
            state.firework_rockets_before_player = Some(u32::from(tick <= 2));
        });
        assert_eq!(ended.position.z, single.position.z);
        assert_eq!(ended.velocity.z, single.velocity.z);
        assert_eq!(ended.position.y, single.position.y);
        let unending = single_ticks("1.21.1", 4, gliding(), |_, state| {
            state.firework_rockets = Some(2);
            state.firework_rockets_before_player = Some(1);
        });
        assert_ne!(ended.velocity.z, unending.velocity.z);
        // a rocket with no tick left boosts no more
        assert_eq!(bulk(vec![0], 0).velocity.z, single_ticks("1.21.1", 4, gliding(), |_, _| {}).velocity.z);
        let mut mismatched = gliding();
        mismatched.firework_rockets = Some(2);
        mismatched.firework_rocket_ticks = Some(vec![1]);
        assert!(simulate_versioned_ticks(
            "1.21.1".to_string(),
            &mut NativeWorld::new(),
            vec![mismatched],
            1,
            default_config(),
        )
        .is_err());
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
    fn an_entity_after_the_player_meets_the_box_of_its_new_pose() {
        // the step into a shulker's box the pose update leaves (26.2 Player.java:266): a shulker
        // at 1.6 pushes a standing player (1.8 tall) from 0.1 and misses a crouching one (1.5)
        for (release, height_after, pushed) in [
            ("1.21.1", None, true),
            ("1.21.1", Some(f64::from(1.5_f32)), false),
            ("1.16.5", Some(f64::from(1.5_f32)), false),
            ("26.2", Some(1.8), true),
        ] {
            let mut world = NativeWorld::new();
            let mut state = pose_state();
            state.no_gravity = Some(true);
            state.position = vec3(0.0, 0.1, 0.0);
            state.height = 1.8;
            state.height_after_tick = height_after;
            state.nearby_entities = Some(vec![nearby_entity(
                "shulker",
                vec3(0.1, 1.61, -0.5),
                vec3(1.1, 2.61, 0.5),
                false,
            )]);
            let after = simulate_versioned_tick(
                release.to_string(),
                &mut world,
                state,
                default_config(),
            )
            .expect("supported");
            assert_eq!(after.velocity.x < 0.0, pushed, "{release} {height_after:?}");
            assert_eq!(after.height, 1.8, "{release}: the tick's own height stays");
        }
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
    fn batched_block_state_identities_outside_the_registry_fail_closed() {
        // the batched update path checks the release against the source-backed registry as the
        // direct setter does, and stores nothing of a rejected identity
        let mut world = NativeWorld::new();
        let update = |version: &str| NativeBlockUpdate {
            id: Some(7),
            state_version: Some(version.to_string()),
            ..shape_update((0, 0, 0), block_shape((0.0, 0.0, 0.0), (1.0, 1.0, 1.0), 0))
        };
        assert!(world.apply_block_updates(vec![update("1.21.12")]).is_err());
        assert!(world.apply_block_updates(vec![update("1.7.10")]).is_err());
        assert_eq!(world.block_at(0, 0, 0).state_version, None);
        world
            .apply_block_updates(vec![update("1.21.11")])
            .expect("1.21.11 is registered");
        assert_eq!(world.block_at(0, 0, 0).state_version.as_deref(), Some("1.21.11"));
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
            .collision_shapes(vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0), None)
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
                .collision_shapes(vec3(0.0, 1.2, 0.0), vec3(1.0, 2.0, 1.0), None)
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

    /// A lectern's updates (26.3 LecternBlock.java:230-231): the boxes of its collision, then empty ones carrying the
    /// rest of its outline, whose top plate reaches 18/16 of its cell. The refusal of one box loses the whole block.
    #[test]
    fn outline_boxes_may_reach_one_block_out_of_their_own() {
        let outlined = |shape: NativeBlockShape, outline: NativeBlockShape| NativeBlockUpdate {
            outline_shape: Some(outline),
            ..shape_update((0, 0, 0), shape)
        };
        let empty = || block_shape((0.0, 0.0, 0.0), (0.0, 0.0, 0.0), 0);
        let base = block_shape((0.0, 0.0, 0.0), (1.0, 0.125, 1.0), 0);
        let post = block_shape((0.25, 0.125, 0.25), (0.75, 0.875, 0.75), 0);
        let plate = block_shape((0.0, 0.875, 0.0), (1.0, 1.125, 0.25), 0);
        let mut world = NativeWorld::new();
        world
            .apply_block_updates(vec![
                outlined(base.clone(), base.clone()),
                outlined(post.clone(), post.clone()),
                outlined(empty(), plate),
            ])
            .expect("a lectern's plate leaves its cell");
        let cell = || (vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0));
        let collision = world.collision_shapes(cell().0, cell().1, None).expect("valid query");
        assert_eq!(collision.len(), 2);
        let outline = world.outline_shapes(cell().0, cell().1, None).expect("valid query");
        assert_eq!(outline.len(), 3);
        assert!(outline.iter().any(|bounds| bounds.max.y == 1.125));

        // the lower half of a pitcher crop, from -1/16 (26.3 PitcherCropBlock.java:53)
        let crop = block_shape((0.1875, -0.0625, 0.1875), (0.8125, 1.0, 0.8125), 0);
        world
            .apply_block_updates(vec![outlined(empty(), crop)])
            .expect("a pitcher crop leaves its cell");

        let far = block_shape((0.0, 0.0, 0.0), (1.0, 2.5, 1.0), 0);
        assert!(world.apply_block_updates(vec![outlined(empty(), far)]).is_err());
    }

    #[test]
    fn a_metadata_only_update_keeps_every_box_of_the_block() {
        // Block geometry is fixed per state (BlockBehaviour.BlockStateBase caches its shapes), so
        // new properties alone leave a fence's two posts, outlines and interaction boxes as they are.
        let mut world = NativeWorld::new();
        let post = |min: (f64, f64, f64), max: (f64, f64, f64)| NativeBlockUpdate {
            outline_shape: Some(block_shape(min, (max.0, 1.0, max.2), 0)),
            interaction_shape: Some(block_shape(min, (max.0, 1.0, max.2), 0)),
            ..shape_update((0, 0, 0), block_shape(min, max, 0))
        };
        world
            .apply_block_updates(vec![
                post((0.375, 0.0, 0.375), (0.625, 1.5, 0.625)),
                post((0.375, 0.375, 0.0), (0.625, 1.5, 0.375)),
            ])
            .expect("a fence with two boxes");
        let metadata = NativeBlockUpdate {
            id: None,
            shape: None,
            properties: Some(vec![super::NativeBlockProperty {
                name: "north".to_string(),
                value: "true".to_string(),
            }]),
            ..shape_update((0, 0, 0), block_shape((0.0, 0.0, 0.0), (1.0, 1.0, 1.0), 0))
        };
        world
            .apply_block_updates(vec![metadata])
            .expect("properties alone");
        let cell = crate::math::Aabb::new(
            crate::math::Vec3::ZERO,
            crate::math::Vec3::new(1.0, 1.0, 1.0),
        );
        assert_eq!(world.inner.collision_shapes(cell, MinecraftVersion::new(26, 3, 0)).len(), 2);
        assert_eq!(world.inner.outline_shapes(cell, MinecraftVersion::new(26, 3, 0)).len(), 2);
        assert_eq!(world.inner.interaction_shapes(cell, MinecraftVersion::new(26, 3, 0)).len(), 2);
        assert_eq!(
            world.inner.block_properties((0, 0, 0)),
            vec![("north".to_string(), "true".to_string())]
        );
    }

    #[test]
    fn a_batch_in_an_unloaded_chunk_writes_nothing() {
        // EmptyLevelChunk.setBlockState returns null (1.14.4 EmptyLevelChunk.java:36-39, client26.3
        // EmptyLevelChunk.java:29-31): the whole update is dropped, its properties, state id and
        // block entity with it, and a removal (air set) too; loaded again, the cell is as it was.
        let mut world = NativeWorld::new();
        let block = |id: u32| NativeBlockUpdate {
            id: Some(id),
            properties: Some(vec![super::NativeBlockProperty {
                name: "lit".to_string(),
                value: id.to_string(),
            }]),
            state_version: Some("1.21".to_string()),
            block_entity: Some(format!("minecraft:entity{id}")),
            ..shape_update((0, 0, 0), block_shape((0.0, 0.0, 0.0), (1.0, 1.0, 1.0), 0))
        };
        world.apply_block_updates(vec![block(1)]).expect("a block");
        world.set_chunk_loaded(0, 0, false);
        world.apply_block_updates(vec![block(2)]).expect("dropped");
        world
            .apply_block_updates(vec![NativeBlockUpdate {
                remove: Some(true),
                ..shape_update((0, 0, 0), block_shape((0.0, 0.0, 0.0), (1.0, 1.0, 1.0), 0))
            }])
            .expect("dropped");
        world.set_chunk_loaded(0, 0, true);
        let cell = world.block_at(0, 0, 0);
        assert_eq!(cell.id, 1);
        assert_eq!(cell.properties[0].value, "1");
        assert_eq!(cell.block_entity.as_deref(), Some("minecraft:entity1"));
        assert_eq!(
            world.inner.block_state_id((0, 0, 0)).map(|id| id.id()),
            Some(1)
        );
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
    fn versioned_entity_push_scales_both_sides_by_the_callers_pushthrough() {
        // `d *= 1.0F - this.pushSpeedReduction` before 1.17 (1.16.5 Entity.java:1306-1307): the
        // calling (left) entity's float scales both pushes; the right entity's is unused
        let result = simulate_versioned_entity_push(
            "1.16.5".to_string(),
            push_state(0.0, 0.0, 0.4),
            push_state(1.0, 0.0, 0.2),
            false,
        )
        .expect("supported entity push should succeed");
        let impulse = 0.05_f32 as f64 * f64::from(1.0_f32 - 0.4_f32);
        assert_eq!(result.left_velocity.x, -impulse);
        assert_eq!(result.right_velocity.x, impulse);
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
    fn is_supported_version_matches_the_versions_the_engine_ticks() {
        for version in ["1.8.8", "1.12.2", "1.13.2", "1.14.4", "1.21.11", "26.3"] {
            assert!(is_supported_version(version.to_string()), "{version}");
        }
        for version in ["1.8.9", "1.14", "1.22.1", "", "latest"] {
            assert!(!is_supported_version(version.to_string()), "{version}");
        }
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
        // the fallback is Pose.SWIMMING, id 3 (1.21.11 Player.java:344, Pose.java), of the float
        // 0.6F x 0.6F size and 0.4F eyes (1.21.11 Avatar.java:20-55)
        let mut world = NativeWorld::new();
        world.set_full_block(0, 1, 0, 1);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, pose_state(), 0, 0, false, None)
                .expect("pose refresh should succeed");
        assert_eq!(result.pose, 3);
        assert!(result.changed);
        assert_eq!(result.state.height, f64::from(0.6_f32));
        assert_eq!(result.state.half_width, f64::from(0.6_f32 / 2.0));
        assert_eq!(result.state.eye_height, Some(f64::from(0.4_f32)));
    }

    #[test]
    fn versioned_pose_refresh_allows_spectator_to_select_requested_pose() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 1, 0, 1);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, pose_state(), 1, 0, true, None)
                .expect("spectator pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert_eq!(result.state.height, f64::from(1.8_f32));
    }

    #[test]
    fn versioned_pose_refresh_checks_the_swimming_fit_for_a_spectator_too() {
        // canPlayerFitWithinBlocksAndEntitiesWhen(SWIMMING) comes before the spectator's free
        // choice (1.14.4 Player.java:334, 1.21.11 Player.java:337-340)
        let mut world = NativeWorld::new();
        world.set_full_block(0, 0, 0, 1);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, pose_state(), 0, 5, true, None)
                .expect("spectator pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert!(!result.changed);
    }

    #[test]
    fn versioned_pose_refresh_puts_a_sleepers_eyes_at_0_2f_and_the_dying_at_1_62f() {
        // LivingEntity.getEyeHeight: 0.2F sleeping (1.14.4 LivingEntity.java:2750, 1.20.4 :3287,
        // 1.21.11 LivingEntity.java:3878); dying the 1.62F default (1.14.4 Player.java:1741-1751,
        // 1.21.11 Avatar.java:55)
        let mut world = NativeWorld::new();
        let mut eyes = |pose| {
            refresh_versioned_pose("1.21.1".to_string(), &mut world, pose_state(), 0, pose, false, None)
                .expect("pose refresh should succeed")
                .state
                .eye_height
        };
        assert_eq!(eyes(2), Some(f64::from(0.2_f32)));
        assert_eq!(eyes(7), Some(f64::from(1.62_f32)));
        assert_eq!(eyes(5), Some(f64::from(1.27_f32)));
    }

    #[test]
    fn versioned_pose_refresh_reads_id_6_as_dying_before_1_17() {
        // EntityPose: STANDING .. CROUCHING, DYING = 6 through 1.16.5; LONG_JUMPING = 6 and
        // DYING = 7 from 1.17 (1.16.5 Pose.java:3-11, 1.17 Pose.java:3-12); DYING is
        // the fixed 0.2F x 0.2F size (1.14.4 Player.java:115, 1.17 Player.java:132)
        let mut world = NativeWorld::new();
        let mut refresh = |version: &str, pose| {
            refresh_versioned_pose(version.to_string(), &mut world, pose_state(), 0, pose, false, None)
        };
        for version in ["1.14.4", "1.15", "1.15.1", "1.15.2", "1.16", "1.16.4", "1.16.5"] {
            let dying = refresh(version, 6).expect("id 6 is DYING");
            assert_eq!(dying.state.height, f64::from(0.2_f32), "{version}");
            assert!(refresh(version, 7).is_err(), "{version}");
        }
        for version in ["1.17", "1.18.2", "1.19.2", "1.21.1", "26.3"] {
            let long_jumping = refresh(version, 6).expect("id 6 is LONG_JUMPING");
            assert_eq!(long_jumping.state.height, f64::from(1.8_f32), "{version}");
            let dying = refresh(version, 7).expect("id 7 is DYING");
            assert_eq!(dying.state.height, f64::from(0.2_f32), "{version}");
        }
    }

    #[test]
    fn versioned_pose_refresh_has_no_poses_before_1_14() {
        // EntityPose and the pose refresh arrive with 1.14 (1.13.2 Player.java updateSize
        // sets heights alone)
        for version in ["1.8.8", "1.12.2", "1.13.2"] {
            let result = refresh_versioned_pose(
                version.to_string(),
                &mut NativeWorld::new(),
                pose_state(),
                0,
                0,
                false,
                None,
            );
            assert!(result.is_err(), "{version}");
        }
    }

    #[test]
    fn versioned_pose_refresh_shrinks_the_box_from_1_16() {
        // canEnterPose: noCollision of the pose's box as it is in 1.14 and 1.15 (client1.14.4
        // Entity.java:1769-1771, 1.15 Entity.java:1651-1653), shrunk by 1e-7 from 1.16 (1.16
        // Entity.java:1687-1689). A slab 1.5e-7 into the standing box's top: joinIsNotEmpty
        // ignores an overlap within 1e-7 (client1.21.11 Shapes.java:179-185).
        let mut world = NativeWorld::new();
        world.set_block_shape(
            0,
            1,
            0,
            1,
            NativeBlockShape {
                min_x: 0.0,
                min_y: f64::from(1.8_f32) - 1.0 - 1.5e-7,
                min_z: 0.0,
                max_x: 1.0,
                max_y: 1.0,
                max_z: 1.0,
                slipperiness: 0.6,
                effect: None,
                bounce_restitution: None,
                suppresses_bounce: None,
                fall_damage_resetting: None,
            },
        )
        .unwrap();
        let mut refresh = |version: &str| {
            refresh_versioned_pose(version.to_string(), &mut world, pose_state(), 5, 0, false, None)
                .expect("pose refresh should succeed")
                .pose
        };
        assert_eq!(refresh("1.15.2"), 5);
        assert_eq!(refresh("1.16"), 0);
    }

    #[test]
    fn versioned_pose_refresh_meets_the_colliding_entities() {
        // noCollision takes the entity colliders too (client1.21.11 CollisionGetter.java:54-72,
        // EntityGetter.java:51-77): a boat over the head keeps the player crouching
        let mut world = NativeWorld::new();
        let mut state = pose_state();
        state.nearby_entities = Some(vec![nearby_entity(
            "boat",
            vec3(-0.5, 1.6, -0.5),
            vec3(0.5, 2.2, 0.5),
            false,
        )]);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, state, 5, 0, false, None)
                .expect("pose refresh should succeed");
        assert_eq!(result.pose, 5);
        assert_eq!(result.state.height, f64::from(1.5_f32));
        let unblocked =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, pose_state(), 5, 0, false, None)
                .expect("pose refresh should succeed");
        assert_eq!(unblocked.pose, 0);
    }

    #[test]
    fn versioned_pose_refresh_scales_the_pose_from_1_20_5() {
        // LivingEntity.getDimensions: the base size times the SCALE attribute but the fixed
        // sleeping and dying sizes, from 1.20.5 (1.20.5 LivingEntity.java:3264-3266,
        // EntityDimensions.java:29-38)
        let mut world = NativeWorld::new();
        let mut refresh = |version: &str, pose| {
            refresh_versioned_pose(
                version.to_string(),
                &mut world,
                pose_state(),
                0,
                pose,
                false,
                Some(0.5),
            )
            .expect("pose refresh should succeed")
            .state
        };
        let standing = refresh("1.20.5", 0);
        assert_eq!(standing.height, f64::from(1.8_f32 * 0.5));
        assert_eq!(standing.half_width, f64::from(0.6_f32 * 0.5 / 2.0));
        assert_eq!(standing.eye_height, Some(f64::from(1.62_f32 * 0.5)));
        assert_eq!(refresh("1.20.5", 2).height, f64::from(0.2_f32));
        assert_eq!(refresh("1.20.4", 0).height, f64::from(1.8_f32));
    }

    #[test]
    fn versioned_pose_refresh_preserves_pose_when_swimming_fit_is_blocked() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 0, 0, 1);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, pose_state(), 0, 5, false, None)
                .expect("pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert!(!result.changed);
        assert_eq!(result.state.height, f64::from(1.8_f32));
    }

    #[test]
    fn versioned_pose_refresh_allows_passenger_to_select_requested_pose() {
        let mut world = NativeWorld::new();
        world.set_full_block(0, 1, 0, 1);
        let mut state = pose_state();
        state.passenger = Some(true);
        let result =
            refresh_versioned_pose("1.21.1".to_string(), &mut world, state, 1, 0, false, None)
                .expect("pose refresh should succeed");
        assert_eq!(result.pose, 0);
        assert_eq!(result.state.height, f64::from(1.8_f32));
    }

    #[test]
    fn a_described_pose_takes_the_players_float_size() {
        // Player.getDimensions(pose): float sizes (1.14.4 Player.java:111-116, 1.21.11
        // Avatar.java:20-55); id 6 is DYING before 1.17, LONG_JUMPING (the standing size) from
        // it, DYING moving to 7 (1.16.5 Pose.java, 1.17 Pose.java)
        let height = |version: Option<&str>, pose: u8| {
            create_simulation_state(pose_descriptor(version, Some(pose))).map(|state| state.height)
        };
        assert_eq!(height(Some("1.14.4"), 6).unwrap(), f64::from(0.2_f32));
        assert_eq!(height(Some("1.20.6"), 7).unwrap(), f64::from(0.2_f32));
        assert_eq!(height(Some("1.17.1"), 6).unwrap(), f64::from(1.8_f32));
        assert_eq!(height(Some("1.14.4"), 5).unwrap(), 1.5);
        assert!(height(Some("1.14.4"), 7).is_err());
        // without a release ids 6 and 7 are ambiguous; the others take the latest sizes
        assert!(height(None, 6).is_err());
        assert!(height(None, 7).is_err());
        assert_eq!(height(None, 0).unwrap(), f64::from(1.8_f32));
        // poses come with 1.14 (1.14.4 Pose.java; 1.13.2 has none)
        assert!(height(Some("1.13.2"), 0).is_err());
        assert!(height(Some("1.8.8"), 5).is_err());
        assert_eq!(
            create_simulation_state(pose_descriptor(Some("1.13.2"), None))
                .unwrap()
                .height,
            1.8
        );
    }

    #[test]
    fn versioned_pose_refresh_rejects_unavailable_long_jumping_pose() {
        let result = refresh_versioned_pose(
            "1.14.4".to_string(),
            &mut NativeWorld::new(),
            pose_state(),
            0,
            7,
            false,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn the_fallback_look_is_the_vanilla_view_vector() {
        // Entity.calculateViewVector: Mth table lookups with float products (1.13.2
        // Entity.java:1336-1344, 1.21.11 Entity.java:1913-1921); before 1.13
        // Entity.getVectorForRotation (1.12.2 Entity.java:1312-1318). Yaw 30, pitch -20 degrees.
        let look = |version: Option<&str>| {
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
                yaw: f64::from(30.0_f32 * 0.017453292),
                in_water: false,
                in_lava: false,
                version: version.map(str::to_string),
                pose: None,
                pitch: Some(f64::from(-20.0_f32 * 0.017453292)),
                input: None,
                jump_requested: None,
                jump_ticks: None,
                sprinting: None,
                sneaking: None,
                using_item: None,
                swimming: None,
                passenger: None,
                boat_vehicle: None,
                can_simulate_movement: None,
                non_living: None,
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
            let look = create_simulation_state(descriptor).unwrap().look.unwrap();
            (look.x, look.y, look.z)
        };
        let modern = (
            f64::from(-0.469_818_47_f32),
            f64::from(0.341_940_08_f32),
            f64::from(0.813_764_5_f32),
        );
        assert_eq!(look(Some("1.21.1")), modern);
        assert_eq!(look(Some("1.21.11")), modern);
        assert_eq!(look(None), modern);
        assert_eq!(
            look(Some("1.12.2")),
            (
                f64::from(-0.469_834_86_f32),
                f64::from(0.341_940_08_f32),
                f64::from(0.813_837_95_f32),
            )
        );
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
            boat_vehicle: None,
            can_simulate_movement: None,
            non_living: None,
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
            &mut NativeWorld::new(),
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
    fn entity_descriptor_forwards_every_living_entity_input() {
        // every LivingEntity reads MOVEMENT_EFFICIENCY (1.21 LivingEntity.java getMovementSpeed /
        // 1.21.11 LivingEntity.java:2224-2225) and the FRICTION and AIR_DRAG modifiers (26.2), and the
        // descriptor carries them as the full state does
        let descriptor = NativeEntityDescriptor {
            position: vec3(0.5, 2.0, 0.5),
            velocity: vec3(0.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: false,
            yaw: 0.0,
            in_water: false,
            in_lava: false,
            version: Some("26.3".to_string()),
            pose: None,
            pitch: None,
            input: None,
            jump_requested: None,
            jump_ticks: Some(7),
            sprinting: None,
            sneaking: None,
            using_item: Some(true),
            swimming: None,
            passenger: None,
            boat_vehicle: None,
            can_simulate_movement: None,
            non_living: None,
            flying: Some(true),
            flying_speed: Some(0.1),
            climbing: None,
            no_physics: Some(true),
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
            water_movement_efficiency: Some(0.25),
            movement_efficiency: Some(0.5),
            air_drag_modifier: Some(0.75),
            friction_modifier: Some(0.125),
            sneaking_speed: Some(0.6),
            look: None,
            firework_boost: None,
        };
        let state = create_simulation_state(descriptor).expect("descriptor should be valid");
        assert_eq!(state.water_movement_efficiency, Some(0.25));
        assert_eq!(state.movement_efficiency, Some(0.5));
        assert_eq!(state.air_drag_modifier, Some(0.75));
        assert_eq!(state.friction_modifier, Some(0.125));
        assert_eq!(state.sneaking_speed, Some(0.6));
        assert_eq!(state.jump_ticks, Some(7));
        assert_eq!(state.using_item, Some(true));
        assert_eq!(state.flying, Some(true));
        assert_eq!(state.flying_speed, Some(0.1));
        assert_eq!(state.no_physics, Some(true));
    }

    #[test]
    fn omitted_flying_speed_is_the_abilities_default() {
        // Abilities.flyingSpeed = 0.05F in every release (1.8.8 Abilities.java:11,
        // 1.21.11 Abilities.java:19), not LivingEntity's 0.02F air speed
        let mut full = create_simulation_state(entity_descriptor(0.0)).unwrap();
        assert_eq!(full.flying_speed, Some(f64::from(0.05_f32)));
        full.flying_speed = None;
        assert_eq!(
            crate::simulation::SimulationState::from(full).flying_speed,
            f64::from(0.05_f32)
        );
    }

    fn entity_descriptor(x: f64) -> NativeEntityDescriptor {
        NativeEntityDescriptor {
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
            boat_vehicle: None,
            can_simulate_movement: None,
            non_living: None,
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
        }
    }

    fn ticked_entity(
        x: f64,
        id: Option<u32>,
        vehicle_id: Option<u32>,
    ) -> NativeVersionedEntityState {
        NativeVersionedEntityState {
            simulation: create_simulation_state(entity_descriptor(x)).unwrap(),
            no_physics: false,
            id,
            vehicle_id,
            pushable: true,
            pushthrough: 0.0,
        }
    }

    fn entity_ticks_config() -> NativeSimulationConfig {
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

    #[test]
    fn versioned_entity_ticks_push_in_each_entitys_turn() {
        // each living entity's tick moves it and then pushes from where it moved: every other
        // pushable entity meeting it runs Entity.push(it) (1.16.5 LivingEntity.java:2512,
        // 2549-2598): the first entity takes a push from where the second stood before its move
        // and another from where both moved
        let result = simulate_versioned_entity_ticks(
            "1.21.1".to_string(),
            &NativeWorld::new(),
            vec![
                ticked_entity(0.0, None, None),
                ticked_entity(0.4, None, None),
            ],
            1,
            entity_ticks_config(),
        )
        .unwrap();
        let push_x = |from: f64, to: f64| {
            entity_push_impulses(
                Vec3::new(from, 0.0, 0.0),
                Vec3::new(to, 0.0, 0.0),
                false,
                false,
                false,
                false,
                false,
                true,
                true,
                MinecraftVersion::new(1, 21, 1),
            )
            .1
            .x
        };
        let first = result[0].simulation.position.x;
        let second = result[1].simulation.position.x;
        assert_eq!(first, 0.0);
        assert_ne!(second, 0.4);
        assert_eq!(
            result[0].simulation.velocity.x,
            push_x(0.4, first) + push_x(second, first)
        );
    }

    #[test]
    fn versioned_entity_ticks_spare_vehicles_not_riders() {
        // Entity.push: `!isVehicle()` (has passengers: 1.16.5 Entity.java:1308-1314, 1.21.11
        // :1931-1937) takes no push; a rider is pushed, except by an entity of the same root
        // vehicle (isPassengerOfSameVehicle, 1.16.5 Entity.java:2526)
        let result = simulate_versioned_entity_ticks(
            "1.21.1".to_string(),
            &NativeWorld::new(),
            vec![
                // a rider of a vehicle outside the list and a free entity
                ticked_entity(0.0, Some(1), Some(99)),
                ticked_entity(0.4, Some(2), None),
                // a vehicle (its rider far away) and a free entity
                ticked_entity(10.0, Some(3), None),
                ticked_entity(10.4, Some(4), None),
                ticked_entity(30.0, Some(5), Some(3)),
                // two riders of one vehicle
                ticked_entity(50.0, Some(6), Some(98)),
                ticked_entity(50.4, Some(7), Some(98)),
            ],
            1,
            entity_ticks_config(),
        )
        .unwrap();
        let velocity_x = |index: usize| result[index].simulation.velocity.x;
        assert!(velocity_x(0) < 0.0);
        assert!(velocity_x(1) > 0.0);
        assert_eq!(velocity_x(2), 0.0);
        assert!(velocity_x(3) > 0.0);
        assert_eq!(velocity_x(5), 0.0);
        assert_eq!(velocity_x(6), 0.0);
        assert_eq!(result[0].id, Some(1));
    }

    #[test]
    fn versioned_entity_push_drops_pushthrough_exactly_at_1_17() {
        let right_x = |version: &str| {
            simulate_versioned_entity_push(
                version.to_string(),
                push_state(0.0, 0.0, 1.0),
                push_state(1.0, 0.0, 0.0),
                false,
            )
            .unwrap()
            .right_velocity
            .x
        };
        assert_eq!(right_x("1.16.5"), 0.0);
        assert_eq!(right_x("1.17"), 0.05_f32 as f64);
    }

    #[test]
    fn versioned_entity_ticks_skip_a_sleeper_being_pushed() {
        // the first's tick runs the sleeper's Entity.push(first), which LivingEntity.push cancels
        // for both; the sleeper's own tick runs first.push(sleeper), which pushes both. So the
        // first entity takes one push instead of two.
        let first_x = |version: &str, sleeping: bool, is_player: bool| {
            let mut sleeper = ticked_entity(0.0, None, None);
            sleeper.simulation.sleeping = Some(sleeping);
            sleeper.simulation.is_player = Some(is_player);
            simulate_versioned_entity_ticks(
                version.to_string(),
                &NativeWorld::new(),
                vec![ticked_entity(0.4, None, None), sleeper],
                1,
                entity_ticks_config(),
            )
            .unwrap()[0]
                .simulation
                .velocity
                .x
        };
        let one_push = |version: &str| {
            let release = MinecraftVersion::parse(version).unwrap();
            let (_, pushed) = crate::entity::entity_push_impulses_with_pushthrough(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.4, 0.0, 0.0),
                false,
                false,
                false,
                false,
                false,
                true,
                true,
                0.0,
                release,
            );
            pushed.x
        };
        assert_eq!(first_x("1.21.1", true, true), one_push("1.21.1"));
        assert_eq!(first_x("1.21.1", true, false), one_push("1.21.1"));
        assert_eq!(first_x("1.14.4", true, false), one_push("1.14.4"));
        assert_eq!(first_x("1.13.2", true, true), one_push("1.13.2"));
        assert_ne!(first_x("1.21.1", false, false), one_push("1.21.1"));
        // before 1.14 only Player overrides it, in 1.8 nothing does
        assert_ne!(first_x("1.13.2", true, false), one_push("1.13.2"));
        assert_ne!(first_x("1.8.8", true, true), one_push("1.8.8"));
    }

    #[test]
    fn versioned_entity_ticks_push_from_a_grown_box_on_1_8() {
        // 1.8.8 LivingEntity.collideWithNearbyEntities takes the entities in its box grown by
        // 0.2F, 0, 0.2F (LivingEntity.java:1471-1474); 1.9.4 :1762-1763 its box alone
        let apart = |version: &str| {
            simulate_versioned_entity_ticks(
                version.to_string(),
                &NativeWorld::new(),
                vec![
                    ticked_entity(0.0, None, None),
                    ticked_entity(0.7, None, None),
                ],
                1,
                entity_ticks_config(),
            )
            .unwrap()[0]
                .simulation
                .velocity
                .x
        };
        assert!(apart("1.8.8") < 0.0);
        assert_eq!(apart("1.9.4"), 0.0);
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
            boat_vehicle: None,
            can_simulate_movement: None,
            non_living: None,
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
            id: None,
            vehicle_id: None,
            pushable: true,
            pushthrough: 0.0,
        };
        let right = NativeVersionedEntityState {
            simulation: create_simulation_state(descriptor(0.4)).unwrap(),
            no_physics: false,
            id: None,
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
                yaw_degrees: None,
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
                pending_movements: None,
                sprinting: None,
                was_sprinting: None,
                sprint_modifier: None,
                in_water: None,
                in_lava: None,
                ultra_warm: None,
                is_player: None,
                controlled_by_local_instance: None,
                passenger: None,
                is_vehicle: None,
                boat_vehicle: None,
                can_simulate_movement: None,
                non_living: None,
                in_web: None,
                weaving: None,
                frost_walker: None,
                sweet_berry_bush_immune: None,
                powder_snow_immune: None,
                powder_snow_walkable: None,
                in_powder_snow: None,
                fire_immune: None,
                fire_resistance: None,
                on_fire_flag: None,
                in_rain: None,
                stab_hit: None,
                fire_protection: None,
                burning_time: None,
                remaining_fire_ticks: None,
                server_fire_increment: None,
                server_fire_increments: None,
                ticks_frozen: None,
                ticks_required_to_freeze: None,
                tick_count: None,
                freeze_damage: None,
                contact_damage: None,
                wall_damage: None,
                inside_wall: None,
                sleeping: None,
                eye_height: None,
                eye_in_water: None,
                eye_in_lava: None,
                slow_falling: None,
                levitation: None,
                levitation_amplifier: None,
                fall_distance: None,
                safe_fall_distance: None,
                fall_damage_immune: None,
                client_side: None,
                height_after_tick: None,
                entity_type: None,
                invulnerable: None,
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
                dead: None,
                gravity: None,
                step_height: None,
                movement_speed: None,
                movement_speed_value: None,
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
                supporting_block: None,
                supporting_block_kept: None,
                on_ground_no_blocks: None,
                outside_border: None,
                movement_speed_attribute: None,
                nearby_entities: None,
                firework_rockets: None,
                firework_rockets_before_player: None,
                firework_rocket_ticks: None,
                moving_pistons: None,
            },
            no_physics: false,
            id: None,
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

fn check_horse_state(state: HorseSimulationState) -> Result<()> {
    FieldCheck::new("horse simulation state")
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .number("yaw", state.yaw, FieldRule::Finite)
        .number("forward", state.forward, FieldRule::Finite)
        .number("strafe", state.strafe, FieldRule::Finite)
        .number("movementSpeed", state.movement_speed, FieldRule::NotNegative)
        .number("jumpStrength", state.jump_strength, FieldRule::NotNegative)
        .number("halfWidth", state.half_width, FieldRule::Positive)
        .number("height", state.height, FieldRule::Positive)
        .number("waterHeight", state.water_height, FieldRule::Finite)
        .done()
}

fn horse_state(value: NativeHorseState) -> Result<HorseSimulationState> {
    let base = RiddenMobState::new(RiddenMobKind::Horse, value.position.clone().into());
    let state = HorseSimulationState {
        position: value.position.into(),
        velocity: value.velocity.into(),
        yaw: value.yaw,
        forward: value.forward,
        strafe: value.strafe,
        jump_requested: value.jump_requested,
        on_ground: value.on_ground,
        in_water: value.in_water,
        movement_speed: value.movement_speed.unwrap_or(base.movement_speed),
        jump_strength: value.jump_strength.unwrap_or(base.jump_strength),
        half_width: value.half_width.unwrap_or(0.7),
        height: value.height.unwrap_or(1.6),
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
        water_height: value.water_height.unwrap_or(0.0),
        no_gravity: value.no_gravity.unwrap_or(false),
    };
    check_horse_state(state)?;
    validate_position("horse", state.position)?;
    Ok(state)
}

fn native_horse_state(state: HorseSimulationState) -> NativeHorseState {
    NativeHorseState {
        nearby_entities: None,
        position: state.position.into(),
        velocity: state.velocity.into(),
        yaw: state.yaw,
        forward: state.forward,
        strafe: state.strafe,
        jump_requested: state.jump_requested,
        on_ground: state.on_ground,
        in_water: state.in_water,
        movement_speed: Some(state.movement_speed),
        jump_strength: Some(state.jump_strength),
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
        lava_height_positive: None,
        water_height: Some(state.water_height),
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
        suffocating_speed: None,
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
        ultra_warm: None,
        jump_boost_amplifier: None,
        no_gravity: Some(state.no_gravity),
        stuck_speed_multiplier: None,
        slow_falling: None,
        levitation_amplifier: None,
        dolphins_grace: None,
        weaving: None,
        gravity: None,
        step_height: None,
        water_movement_efficiency: None,
        air_drag_modifier: None,
        friction_modifier: None,
    }
}

/// A ridden mob from the horse state: the rider's xxa and zza are `strafe` and `forward`, its
/// jumping `jumpRequested`; an attribute not given takes the mob's base.
pub(crate) fn ridden_mob_state(value: NativeHorseState, release: MinecraftVersion) -> Result<RiddenMobState> {
    let name = value.entity_type.as_deref().unwrap_or("horse");
    let kind = RiddenMobKind::parse_for(name, release).map_err(Error::from_reason)?;
    refuse_for_ridden_tick(name, release).map_err(Error::from_reason)?;
    let defaults = RiddenMobState::new(kind, value.position.into());
    let mut mob = RiddenMobState {
        velocity: value.velocity.into(),
        yaw: value.yaw_degrees.unwrap_or_else(|| value.yaw.to_degrees()) as f32,
        pitch: value.pitch_degrees.unwrap_or(0.0) as f32,
        on_ground: value.on_ground,
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
        in_water: value.in_water,
        in_lava: value.in_lava.unwrap_or(false),
        lava_height_positive: value
            .lava_height_positive
            .unwrap_or_else(|| value.in_lava.unwrap_or(false)),
        fall_distance: value.fall_distance.unwrap_or(0.0),
        bounding_box: value.bounding_box.map(Into::into),
        previous_position: value.previous_position.map(Into::into),
        movement_speed: value.movement_speed.unwrap_or_else(|| kind.movement_speed_in(release)),
        jump_strength: value.jump_strength.unwrap_or(defaults.jump_strength),
        flying_speed: value.flying_speed.unwrap_or(defaults.flying_speed),
        saddled: value.saddled.unwrap_or(true),
        no_ai: value.no_ai.unwrap_or(false),
        ridden_speed: value.ridden_speed.unwrap_or(0.0) as f32,
        own_forward: value.own_forward.unwrap_or(0.0) as f32,
        boosting: value.boosting.unwrap_or(false),
        boost_time: value.boost_time.unwrap_or(0),
        boost_time_total: value.boost_time_total.unwrap_or(0),
        suffocating: value.suffocating.unwrap_or(false),
        suffocating_speed: value.suffocating_speed,
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
        ultra_warm: value.ultra_warm.unwrap_or(false),
        jump_boost_amplifier: value.jump_boost_amplifier,
        no_gravity: value.no_gravity.unwrap_or(false),
        stuck_speed_multiplier: value
            .stuck_speed_multiplier
            .map_or(Vec3::ZERO, Into::into),
        slow_falling: value.slow_falling.unwrap_or(false),
        levitation_amplifier: value.levitation_amplifier,
        dolphins_grace: value.dolphins_grace.unwrap_or(false),
        weaving: value.weaving.unwrap_or(false),
        gravity: value.gravity,
        step_height: value.step_height,
        water_movement_efficiency: value.water_movement_efficiency.unwrap_or(0.0),
        air_drag_modifier: value.air_drag_modifier,
        plain: None,
        friction_modifier: value.friction_modifier,
        ..defaults
    };
    // (the attributes are the effective values: RangedAttribute keeps these at 0 or above; GRAVITY,
    // STEP_HEIGHT, the efficiency and the modifiers are sanitized where ridden.rs reads them, as
    // their signed or clamped ranges are in the attribute's own contract)
    FieldCheck::new("ridden mob simulation state")
        .vector("position", mob.position, FieldRule::Finite)
        .vector("velocity", mob.velocity, FieldRule::Finite)
        .number("yaw", f64::from(mob.yaw), FieldRule::Finite)
        // MOVEMENT_SPEED tops out at 1024, JUMP_STRENGTH at 2 until 1.20.4 and 32 from 1.20.5 (1.20.4
        // Attributes.java:25, 39; 1.20.5 :21-24; 26.3 Attributes.java:73, 80)
        .number("movementSpeed", mob.movement_speed, FieldRule::Within(0.0, 1024.0))
        .number("jumpStrength", mob.jump_strength, FieldRule::Within(0.0, if release.at_least(1, 20, 5) { 32.0 } else { 2.0 }))
        .number("flyingSpeed", mob.flying_speed, FieldRule::NotNegative)
        .vector("stuckSpeedMultiplier", mob.stuck_speed_multiplier, FieldRule::Finite)
        .optional("halfWidth", value.half_width, FieldRule::Positive)
        .optional("height", value.height, FieldRule::Positive)
        .done()?;
    validate_position("ridden mob", mob.position)?;
    for position in [mob.lerp_position, mob.previous_position].into_iter().flatten() {
        validate_position("ridden mob", position)?;
    }
    validate_kept_box("ridden mob", mob.bounding_box)?;
    validate_size("ridden mob", value.half_width.unwrap_or(0.0), value.height.unwrap_or(0.0))?;
    mob.measure(
        value.half_width.map(|half| (half * 2.0) as f32),
        value.height.map(|height| height as f32),
        release,
    );
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

pub(crate) fn native_ridden_mob_state(mob: RiddenMobState, release: MinecraftVersion) -> NativeHorseState {
    let (width, height) = mob.dimensions(release);
    NativeHorseState {
        nearby_entities: None,
        position: mob.position.into(),
        velocity: mob.velocity.into(),
        yaw: f64::from(mob.yaw).to_radians(),
        forward: f64::from(mob.rider.forward),
        strafe: f64::from(mob.rider.strafe),
        jump_requested: mob.rider.jumping,
        on_ground: mob.on_ground,
        in_water: mob.in_water,
        movement_speed: Some(mob.movement_speed),
        jump_strength: Some(mob.jump_strength),
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
        lava_height_positive: Some(mob.lava_height_positive),
        water_height: None,
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
        suffocating_speed: mob.suffocating_speed,
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
        ultra_warm: Some(mob.ultra_warm),
        jump_boost_amplifier: mob.jump_boost_amplifier,
        no_gravity: Some(mob.no_gravity),
        stuck_speed_multiplier: Some(mob.stuck_speed_multiplier.into()),
        slow_falling: Some(mob.slow_falling),
        levitation_amplifier: mob.levitation_amplifier,
        dolphins_grace: Some(mob.dolphins_grace),
        weaving: Some(mob.weaving),
        gravity: mob.gravity,
        step_height: mob.step_height,
        water_movement_efficiency: Some(mob.water_movement_efficiency),
        air_drag_modifier: mob.air_drag_modifier,
        friction_modifier: mob.friction_modifier,
    }
}

fn check_boat_state(state: BoatSimulationState) -> Result<()> {
    FieldCheck::new("boat simulation state")
        .vector("position", state.position, FieldRule::Finite)
        .vector("velocity", state.velocity, FieldRule::Finite)
        .number("yaw", state.yaw, FieldRule::Finite)
        .number("deltaRotation", f64::from(state.delta_rotation), FieldRule::Finite)
        .done()
}

/// A boat's status by its name, absent being in the air: a name it does not know stands for no status.
fn parse_boat_status(value: Option<&str>) -> Result<BoatStatus> {
    match value {
        Some("in_water") => Ok(BoatStatus::InWater),
        Some("under_water") => Ok(BoatStatus::UnderWater),
        Some("under_flowing_water") => Ok(BoatStatus::UnderFlowingWater),
        Some("on_land") => Ok(BoatStatus::OnLand),
        Some("in_air") | None => Ok(BoatStatus::InAir),
        Some(other) => Err(Error::from_reason(format!("unknown boat status: {other}"))),
    }
}

pub(crate) fn boat_state(value: NativeBoatState) -> Result<BoatSimulationState> {
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
        status: parse_boat_status(value.status.as_deref())?,
        previous_status: parse_boat_status(value.previous_status.as_deref())?,
        settle_status: value.status.is_none(),
        last_yd: value.last_vertical_velocity.unwrap_or(0.0),
        fall_distance: value.fall_distance.unwrap_or(0.0),
        bubble_time: value.bubble_time.unwrap_or(0),
        bubble_column_direction_is_down: value
            .bubble_column_direction_is_down
            .unwrap_or(false),
        above_bubble_column: value.above_bubble_column.unwrap_or(false),
        server_side: value.server_side.unwrap_or(false),
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
        stuck_speed_multiplier: value.stuck_speed_multiplier.map_or(Vec3::ZERO, Into::into),
        ultra_warm: value.ultra_warm.unwrap_or(false),
        no_gravity: value.no_gravity.unwrap_or(false),
        paddle_phases: match value.paddle_phases.as_deref() {
            None => [0.0; 2],
            Some([left, right]) => [*left as f32, *right as f32],
            Some(_) => {
                return Err(Error::from_reason(
                    "boat paddlePhases must hold the left and right phases",
                ))
            }
        },
        silent: value.silent.unwrap_or(false),
        move_out_of_range: false,
        lerp: crate::vehicle::BoatLerp {
            steps: value.lerp_steps.unwrap_or(0).max(0),
            target: value.lerp_position.map_or(value.position.into(), Into::into),
            y_rot: value.lerp_y_rot.unwrap_or(value.yaw_degrees.unwrap_or(0.0)),
            x_rot: 0.0,
        },
    };
    check_boat_state(state)?;
    validate_position("boat", state.position)?;
    validate_kept_box("boat", state.bounding_box)?;
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

pub(crate) fn native_boat_state(state: BoatSimulationState) -> NativeBoatState {
    NativeBoatState {
        nearby_entities: None,
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
        server_side: Some(state.server_side),
        player_passenger: Some(state.player_passenger),
        collided_horizontally: Some(state.collided_horizontally),
        collided_vertically: Some(state.collided_vertically),
        on_ground: Some(state.on_ground),
        yaw_degrees: Some(f64::from(state.yaw_degrees)),
        land_friction: Some(f64::from(state.land_friction)),
        last_vertical_velocity: Some(state.last_yd),
        fall_distance: Some(state.fall_distance),
        bounding_box: state.bounding_box.map(Into::into),
        speed_multiplier: Some(state.legacy.speed_multiplier),
        rider_yaw: Some(f64::from(state.legacy.rider_yaw)),
        rider_forward: Some(f64::from(state.legacy.rider_forward)),
        rider_strafe: Some(f64::from(state.legacy.rider_strafe)),
        stuck_speed_multiplier: Some(state.stuck_speed_multiplier.into()),
        ultra_warm: Some(state.ultra_warm),
        no_gravity: Some(state.no_gravity),
        paddle_phases: Some(state.paddle_phases.iter().map(|&phase| f64::from(phase)).collect()),
        silent: Some(state.silent),
        lerp_steps: Some(state.lerp.steps),
        lerp_position: Some(state.lerp.target.into()),
        lerp_y_rot: Some(state.lerp.y_rot),
    }
}

/// A box of a block's shape, in any channel. No VoxelShape is held to its cell: collision has fences at 1.5, and the
/// outline too leaves it, a lectern's top plate reaching 18/16 (26.3 LecternBlock.java:231) and a pitcher crop's lower
/// half starting at -1/16 (26.3 PitcherCropBlock.java:53). The limit is the reach of a query's cells, one block out.
fn valid_block_shape(bounds: crate::math::Aabb, slipperiness: f64) -> bool {
    valid_bounds(bounds)
        && bounds.min.x >= -1.0
        && bounds.min.y >= -1.0
        && bounds.min.z >= -1.0
        && bounds.max.x <= 2.0
        && bounds.max.y <= 2.0
        && bounds.max.z <= 2.0
        && slipperiness.is_finite()
        && slipperiness >= 0.0
}

fn native_block_shape(value: NativeBlockShape) -> Result<BlockShape> {
    let bounds = crate::math::Aabb::new(
        Vec3::new(value.min_x, value.min_y, value.min_z),
        Vec3::new(value.max_x, value.max_y, value.max_z),
    );
    if !valid_block_shape(bounds, value.slipperiness) {
        return Err(Error::from_reason(
            "outline shape bounds must be finite, ordered, and reach at most one block out of their own",
        ));
    }

    Ok(BlockShape {
        bounds,
        slipperiness: value.slipperiness,
        fluid: FluidState::Empty,
        effect: block_effect(value.effect.unwrap_or(0))?,
    })
}

/// A block's restitution as a landing reads it: absent is none, and anything but a finite number from 0 is refused.
fn validated_bounce(value: Option<f64>) -> Result<f64> {
    let bounce = value.unwrap_or(0.0);
    if !bounce.is_finite() || bounce < 0.0 {
        return Err(Error::from_reason(
            "bounce restitution must be finite and non-negative",
        ));
    }
    Ok(bounce)
}

/// What only a collision box carries of a block, refused on a box that would drop it.
fn refuse_collision_fields(kind: &str, shape: &NativeBlockShape) -> Result<()> {
    if shape.bounce_restitution.is_some() || shape.suppresses_bounce.is_some() || shape.fall_damage_resetting.is_some() {
        return Err(Error::from_reason(format!(
            "{kind} shape carries no bounceRestitution, suppressesBounce or fallDamageResetting: give them on the collision shape or the update"
        )));
    }
    Ok(())
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

/// The generated state index of a native fluid cell (legacy `level`, source and falling bits) in
/// `version`'s source-derived fluid table, or `None` when the table has no such state.
///
/// From 1.13 a source is `FlowingFluid#getSource(falling)` (FALLING only) and anything else is
/// `getFlowing(amount, falling)` with LEVEL = amount; LiquidBlock maps legacy level 0 to the
/// source, 1-7 to amount `8 - level` and 8-15 to falling amount 8 (`origin/client1.13.2
/// FlowingFluid.java`, `1.20.6 LiquidBlock.java` stateCache). Before 1.13 the fluid is the
/// still/flowing liquid block itself, whose only property is LEVEL 0-15 with bit 8 for falling
/// (`1.12.2 LiquidBlock.java:28`).
fn generated_fluid_state_index_for(
    version: &str,
    fluid: FluidState,
    level: u8,
    source: bool,
    falling: bool,
) -> Result<Option<usize>> {
    let source_id = generated_fluid_id(fluid)
        .ok_or_else(|| Error::from_reason("versioned fluid state does not support this fluid"))?;
    let flowing_id = match fluid {
        FluidState::Water => "flowing_water",
        _ => "flowing_lava",
    };
    let Some(definition) = crate::generated_data::fluid(version, source_id) else {
        return Ok(None);
    };
    let falling_value = if falling { "true" } else { "false" };
    if definition.property_names.contains(&"FALLING") {
        if source {
            return Ok(crate::generated_data::fluid_state_index(
                version,
                source_id,
                &[("FALLING", falling_value)],
            ));
        }
        let amount = if falling || level == 0 || level >= 8 { 8 } else { 8 - level };
        let amount = amount.to_string();
        return Ok(crate::generated_data::fluid_state_index(
            version,
            flowing_id,
            &[("LEVEL", amount.as_str()), ("FALLING", falling_value)],
        ));
    }
    let legacy_level = if source {
        0
    } else if falling && level < 8 {
        level + 8
    } else {
        level
    };
    let legacy_level = legacy_level.to_string();
    Ok(crate::generated_data::fluid_state_index(
        version,
        if source { source_id } else { flowing_id },
        &[("LEVEL", legacy_level.as_str())],
    ))
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

pub(crate) fn block_effect_code(effect: BlockEffect) -> u8 {
    effect.code()
}

/// Whether an update says nothing of the block's state or geometry, only its metadata.
fn metadata_only(update: &NativeBlockUpdate) -> bool {
    update.id.is_none()
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
        && update.water_block.is_none()
        && update.fall_damage_resetting.is_none()
        && update.player_fall_damage_resetting.is_none()
        && update.bounce_restitution.is_none()
        && update.suppresses_bounce.is_none()
        && update.suffocating.is_none()
        && update.in_wall.is_none()
        && update.bottom_face_sturdy.is_none()
}

/// All applyBlockUpdates can refuse of one update, which it asks before it changes the world.
fn validate_block_update(update: &NativeBlockUpdate) -> Result<()> {
    if update.remove.unwrap_or(false) {
        return Ok(());
    }
    if let Some(properties) = update.properties.as_ref() {
        validate_block_properties(properties)?;
    }
    if !metadata_only(update) {
        block_update_state(update)?;
    }
    if let Some(shape) = update.outline_shape.as_ref() {
        refuse_collision_fields("an outline", shape)?;
    }
    if let Some(shape) = update.interaction_shape.as_ref() {
        refuse_collision_fields("an interaction", shape)?;
    }
    if let Some(value) = update.state_version.as_deref() {
        let version = registered_release(value)?;
        crate::world::VersionedBlockStateId::new(version, 0).map_err(Error::from_reason)?;
    }
    if update.block_entity.as_deref() == Some("") {
        return Err(Error::from_reason("block entity kind must not be empty"));
    }
    Ok(())
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
    let water_block = update.water_block.unwrap_or(false);
    let collision = update.shape.as_ref();
    let fall_damage_resetting = update
        .fall_damage_resetting
        .or(collision.and_then(|shape| shape.fall_damage_resetting))
        .unwrap_or(false);
    let player_fall_damage_resetting = update.player_fall_damage_resetting.unwrap_or(false);
    let explosion_resistance = match update.explosion_resistance {
        Some(value) if !value.is_finite() || value < 0.0 => {
            return Err(Error::from_reason("explosion resistance must be finite and not negative"));
        }
        value => value.map(|value| value as f32),
    };
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
    let bounce_restitution = validated_bounce(
        update.bounce_restitution.or(collision.and_then(|shape| shape.bounce_restitution)),
    )?;
    let suppresses_bounce = update
        .suppresses_bounce
        .or(collision.and_then(|shape| shape.suppresses_bounce))
        .unwrap_or(false);
    let shape = update.shape.as_ref().map(|value| {
        let bounds = crate::math::Aabb::new(
            Vec3::new(value.min_x, value.min_y, value.min_z),
            Vec3::new(value.max_x, value.max_y, value.max_z),
        );
        (bounds, value.slipperiness, value.effect.unwrap_or(0))
    });
    if let Some((bounds, slipperiness, effect)) = shape {
        if !valid_block_shape(bounds, slipperiness) {
            return Err(Error::from_reason(
                "block shape bounds must be finite, ordered, and reach at most one block out of their own",
            ));
        }
        let outline_shape = if let Some(value) = update.outline_shape.as_ref() {
            let outline_bounds = crate::math::Aabb::new(
                Vec3::new(value.min_x, value.min_y, value.min_z),
                Vec3::new(value.max_x, value.max_y, value.max_z),
            );
            if !valid_block_shape(outline_bounds, value.slipperiness) {
                return Err(Error::from_reason(
                    "outline shape bounds must be finite, ordered, and reach at most one block out of their own",
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
            water_block,
            fall_damage_resetting,
            player_fall_damage_resetting,
            suffocating: update.suffocating,
            in_wall: update.in_wall,
            bottom_face_sturdy: update.bottom_face_sturdy,
            explosion_resistance,
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
            if !valid_block_shape(bounds, value.slipperiness) {
                return Err(Error::from_reason(
                    "outline shape bounds must be finite, ordered, and reach at most one block out of their own",
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
            if !valid_block_shape(bounds, value.slipperiness) {
                return Err(Error::from_reason(
                    "interaction shape bounds must be finite, ordered, and reach at most one block out of their own",
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
        water_block,
        fall_damage_resetting,
        player_fall_damage_resetting,
        suffocating: update.suffocating,
        in_wall: update.in_wall,
        bottom_face_sturdy: update.bottom_face_sturdy,
        explosion_resistance,
    })
}

pub(crate) fn block_effect(value: u8) -> Result<BlockEffect> {
    BlockEffect::from_code(value).map_err(Error::from_reason)
}

/// A shulker box's lid at the bindings: `status` is 0 closed, 1 opening, 2 opened, 3 closing.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeShulkerBox {
    /// FACING as a Direction's ordinal: down, up, north, south, west, east.
    pub facing: u8,
    pub status: u8,
    pub progress: f64,
    pub progress_old: f64,
    /// The progress a push reads before 1.17: `progress` when not given.
    pub push_progress: Option<f64>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeShulkerBoxTick {
    pub state: NativeShulkerBox,
    /// The cell's collision box, at the cell's origin.
    pub bounds: NativeAabb,
    /// Whether the lid pushes what it meets this tick (simulateVersionedShulkerBoxPush).
    pub pushes: bool,
}

pub(crate) fn shulker_box(value: NativeShulkerBox) -> Result<ShulkerBox> {
    let status = match value.status {
        0 => ShulkerBoxStatus::Closed,
        1 => ShulkerBoxStatus::Opening,
        2 => ShulkerBoxStatus::Opened,
        3 => ShulkerBoxStatus::Closing,
        other => return Err(Error::from_reason(format!("shulker box status {other} is none of 0 to 3"))),
    };
    if value.facing > 5 || !value.progress.is_finite() || !value.progress_old.is_finite() {
        return Err(Error::from_reason("shulker box facing must be 0 to 5, and its progress finite"));
    }
    Ok(ShulkerBox {
        facing: value.facing,
        status,
        progress: value.progress as f32,
        progress_old: value.progress_old as f32,
        push_progress: value.push_progress.unwrap_or(value.progress) as f32,
    })
}

fn native_shulker_box(value: ShulkerBox) -> NativeShulkerBox {
    NativeShulkerBox {
        facing: value.facing,
        status: value.status as u8,
        progress: f64::from(value.progress),
        progress_old: f64::from(value.progress_old),
        push_progress: Some(f64::from(value.push_progress)),
    }
}

/// A shulker box's block event, the server's (triggerEvent(1, count)), in the releases that have
/// the box (from 1.11).
#[napi]
pub fn shulker_box_block_event(version: String, state: NativeShulkerBox, count: i32) -> Result<NativeShulkerBox> {
    let release = vehicle_release(&version, "shulker box")?;
    // (1.10.2 BlockEntity.java:243-263 (no shulker box registered); 1.11.2 BlockEntity.java:263, ShulkerBoxBlockEntity.java:58-63)
    if !release.at_least(1, 11, 0) {
        return Err(Error::from_reason(format!("{version} has no shulker box")));
    }
    let mut shulker = shulker_box(state)?;
    shulker.block_event(count);
    Ok(native_shulker_box(shulker))
}

/// One tick of a shulker box's block entity, in the releases that have it (from 1.11).
#[napi]
pub fn simulate_versioned_shulker_box_tick(
    version: String,
    state: NativeShulkerBox,
) -> Result<NativeShulkerBoxTick> {
    let release = vehicle_release(&version, "shulker box")?;
    // (1.10.2 BlockEntity.java:243-263 (no shulker box registered); 1.11.2 BlockEntity.java:263, ShulkerBoxBlockEntity.java:58-63)
    if !release.at_least(1, 11, 0) {
        return Err(Error::from_reason(format!("{version} has no shulker box")));
    }
    let mut shulker = shulker_box(state)?;
    let pushes = shulker.tick(release);
    let bounds = shulker.bounds();
    Ok(NativeShulkerBoxTick {
        state: native_shulker_box(shulker),
        bounds: NativeAabb { min: bounds.min.into(), max: bounds.max.into() },
        pushes,
    })
}

/// What an opening lid does to a state at the cell's edge: its state after the push.
#[napi]
pub fn simulate_versioned_shulker_box_push(
    version: String,
    world: &NativeWorld,
    state: NativeSimulationState,
    shulker: NativeShulkerBox,
    x: i32,
    y: i32,
    z: i32,
) -> Result<NativeSimulationState> {
    let release = vehicle_release(&version, "shulker box")?;
    // as simulateVersionedShulkerBoxTick: no shulker box before 1.11 (1.10.2 BlockEntity.java:243-263, 1.11.2 :263)
    if !release.at_least(1, 11, 0) {
        return Err(Error::from_reason(format!("{version} has no shulker box")));
    }
    validate_tick_state("simulateVersionedShulkerBoxPush", &state)?;
    let shulker = shulker_box(shulker)?;
    let mut moved: SimulationState = state.into();
    crate::shulker_box::push_entity(
        &mut moved,
        world.snapshot(),
        shulker,
        (x, y, z),
        PhysicsConfig::default(),
        release,
    );
    Ok(moved.into())
}
#[cfg(test)]
#[path = "napi_api_gate_tests.rs"]
mod gate_fixtures;
#[cfg(test)]
#[path = "recorded_fixtures_tests.rs"]
mod recorded_fixtures;
