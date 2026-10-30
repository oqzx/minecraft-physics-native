use crate::block_traversal::{blocks_visited, Movement};
use crate::collision::{collide, collide_legacy, vanilla_equal, CollisionRules, StepUp};
use crate::entity::{Dimensions, EntityFlags, EntityState};
use crate::legacy_fluid;
use crate::math::{
    fast_inv_sqrt, fast_inv_sqrt_f32, java_cos, java_cos_legacy, java_float, java_sin,
    java_sin_legacy, lerp, Aabb, Vec3,
};
use crate::trig;
use crate::version::MinecraftVersion;
use crate::world::WorldSnapshot;
use crate::world::{BlockEffect, EntityCollisionContext, FluidContact, FluidState};

type BlockPosition = (i32, i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsConfig {
    pub gravity: f64,
    pub drag: f64,
    pub movement_speed: f64,
    pub sprint_multiplier: f64,
    pub jump_velocity: f64,
    pub step_height: f64,
    pub negligible_velocity: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VersionPhysicsRules {
    pub release: MinecraftVersion,
    pub gravity: f64,
    pub water_gravity_divisor: f64,
    pub lava_gravity_divisor: f64,
    pub water_drag: f64,
    pub water_sprint_drag: f64,
    pub lava_drag: f64,
    pub air_drag: f64,
    pub vertical_drag: f64,
    pub fluid_vertical_drag: f64,
    pub water_flow_scale: f64,
    pub lava_flow_scale: f64,
    pub ultra_warm_lava_flow_scale: f64,
    pub fluid_travel_split: bool,
    pub fluid_falling_checks_gravity_argument: bool,
    pub water_input_speed: f64,
    pub lava_input_speed: f64,
    pub step_height: f64,
    pub swimming_vertical_steering: f64,
    pub swimming_downward_steering: f64,
    pub water_movement_efficiency_divisor: f64,
    pub water_movement_efficiency_attribute: bool,
}

pub trait VersionPhysicsProfile {
    fn rules() -> VersionPhysicsRules;
    fn landing_damage_rounding() -> LandingDamageRounding;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandingDamageRounding {
    Ceil,
    Floor,
}

pub(crate) fn landing_damage(
    fall_distance: f64,
    safe_fall_distance: f64,
    multiplier: f64,
    jump_boost_amplifier: Option<u8>,
    rounding: LandingDamageRounding,
) -> f64 {
    let jump_boost = jump_boost_amplifier.map_or(0.0, |amplifier| f64::from(amplifier) + 1.0);
    if fall_distance <= 3.0 + safe_fall_distance + jump_boost {
        return 0.0;
    }
    let damage = (fall_distance - 3.0 - safe_fall_distance - jump_boost) * multiplier;
    match rounding {
        LandingDamageRounding::Ceil => damage.ceil(),
        LandingDamageRounding::Floor => damage.floor(),
    }
}

pub fn tick_with_profile<P: VersionPhysicsProfile>(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> TickResult {
    tick_with_rules_and_rounding_and_restitution(
        state,
        world,
        config,
        P::rules(),
        P::landing_damage_rounding(),
        false,
        P::rules().water_movement_efficiency_attribute,
    )
}

pub fn tick_with_profile_and_restitution<P: VersionPhysicsProfile>(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> TickResult {
    tick_with_rules_and_rounding_and_restitution(
        state,
        world,
        config,
        P::rules(),
        P::landing_damage_rounding(),
        true,
        true,
    )
}

pub fn tick_with_profile_and_modern_fall_damage<P: VersionPhysicsProfile>(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> TickResult {
    tick_with_rules_and_rounding_and_restitution(
        state,
        world,
        config,
        P::rules(),
        P::landing_damage_rounding(),
        false,
        true,
    )
}

impl Default for VersionPhysicsRules {
    fn default() -> Self {
        Self {
            release: MinecraftVersion::new(26, 2, 0),
            gravity: 0.08,
            water_gravity_divisor: 5.0,
            lava_gravity_divisor: 4.0,
            water_drag: 0.8,
            water_sprint_drag: 0.9,
            lava_drag: 0.5,
            air_drag: 0.98,
            vertical_drag: 0.98,
            fluid_vertical_drag: 0.8,
            water_flow_scale: 0.014,
            lava_flow_scale: 0.0023333333333333335,
            ultra_warm_lava_flow_scale: 0.007,
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: true,
            water_input_speed: 0.02,
            lava_input_speed: 0.02,
            step_height: 0.6,
            swimming_vertical_steering: 0.06,
            swimming_downward_steering: 0.085,
            water_movement_efficiency_divisor: 3.0,
            water_movement_efficiency_attribute: false,
        }
    }
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 0.08,
            drag: 0.98,
            movement_speed: 0.1,
            sprint_multiplier: 1.3,
            jump_velocity: 0.42,
            step_height: 0.6,
            negligible_velocity: 0.003,
        }
    }
}

impl PhysicsConfig {
    pub fn validate(self) -> std::result::Result<(), &'static str> {
        let finite = self.gravity.is_finite()
            && self.drag.is_finite()
            && self.movement_speed.is_finite()
            && self.sprint_multiplier.is_finite()
            && self.jump_velocity.is_finite()
            && self.step_height.is_finite()
            && self.negligible_velocity.is_finite();
        if !finite {
            return Err("simulation config contains non-finite values");
        }
        if !(0.0..=10.0).contains(&self.gravity)
            || !(0.0..=2.0).contains(&self.drag)
            || !(0.0..=100.0).contains(&self.movement_speed)
            || !(0.0..=10.0).contains(&self.sprint_multiplier)
            || !(0.0..=10.0).contains(&self.jump_velocity)
            || !(0.0..=10.0).contains(&self.step_height)
            || !(0.0..=1.0).contains(&self.negligible_velocity)
        {
            return Err("simulation config contains out-of-range values");
        }
        Ok(())
    }
}

/// The movement speed attribute with the sprint, speed and slowness
/// modifiers. They are all `ADD_MULTIPLIED_TOTAL`: each multiplies the value
/// by one plus its amount, in the order vanilla's modifier map iterates them.
#[must_use]
pub(crate) fn movement_speed_attribute(
    base: f64,
    speed_effect_amplifier: Option<u8>,
    slowness_effect_amplifier: Option<u8>,
    sprint_active: bool,
    sprint_multiplier: f64,
    release: MinecraftVersion,
) -> f64 {
    let sprint = sprint_active.then(|| java_float(sprint_multiplier - 1.0));
    let speed =
        speed_effect_amplifier.map(|amplifier| 0.20000000298023224 * (f64::from(amplifier) + 1.0));
    let slowness = slowness_effect_amplifier
        .map(|amplifier| -0.15000000596046448 * (f64::from(amplifier) + 1.0));
    let modifiers = if release.at_least(1, 21, 0) {
        [sprint, speed, slowness]
    } else if release.at_least(1, 20, 5) {
        [sprint, slowness, speed]
    } else {
        [slowness, sprint, speed]
    };
    modifiers
        .into_iter()
        .flatten()
        .fold(base, |value, amount| value * (1.0 + amount))
        .clamp(0.0, 1024.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimulationState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub half_width: f64,
    pub height: f64,
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub minor_horizontal_collision: bool,
    pub minor_horizontal_collision_supported: bool,
    pub collided_vertically: bool,
    pub vertical_collision_below: bool,
    pub block_fall_callback: bool,
    pub entity_bounciness: f64,
    pub block_bounciness: f64,
    pub input: Vec3,
    pub jump_requested: bool,
    pub jump_ticks: u32,
    pub auto_jump: bool,
    pub auto_jump_time: u32,
    pub riptide_launch: Option<u8>,
    pub sprinting: bool,
    pub was_sprinting: bool,
    pub sprint_modifier: Option<bool>,
    pub in_water: bool,
    pub in_lava: bool,
    pub ultra_warm: bool,
    pub is_player: bool,
    pub controlled_by_local_instance: bool,
    pub passenger: bool,
    pub in_web: bool,
    pub weaving: bool,
    pub sweet_berry_bush_immune: bool,
    pub powder_snow_immune: bool,
    pub powder_snow_walkable: bool,
    pub in_powder_snow: bool,
    pub ticks_frozen: u32,
    pub ticks_required_to_freeze: u32,
    pub tick_count: u32,
    pub freeze_damage: f64,
    pub contact_damage: f64,
    pub inside_wall: bool,
    pub eye_height: Option<f64>,
    pub eye_in_water: bool,
    pub eye_in_lava: bool,
    pub slow_falling: bool,
    pub levitation: bool,
    pub levitation_amplifier: Option<u8>,
    pub fall_distance: f64,
    pub safe_fall_distance: f64,
    pub fall_damage_immune: bool,
    pub fall_damage_multiplier: Option<f64>,
    pub sneaking: bool,
    pub moving_slowly: Option<bool>,
    pub stepping_carefully: bool,
    pub using_item: bool,
    /// UseEffects.speedMultiplier of the item in use (1.21.11+); 0.2 when absent.
    pub item_use_speed_multiplier: Option<f32>,
    pub swimming: bool,
    pub flying: bool,
    pub flying_speed: f64,
    pub climbing: bool,
    pub no_physics: bool,
    pub no_gravity: bool,
    /// Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+).
    pub removed: bool,
    pub gravity: Option<f64>,
    pub step_height: Option<f64>,
    pub movement_speed: Option<f64>,
    pub jump_strength: Option<f64>,
    pub speed_effect_amplifier: Option<u8>,
    pub slowness_effect_amplifier: Option<u8>,
    pub jump_boost_amplifier: Option<u8>,
    pub dolphins_grace: u8,
    pub depth_strider: u8,
    pub soul_speed: u8,
    pub water_movement_efficiency: f64,
    pub movement_efficiency: f64,
    pub air_drag_modifier: f64,
    pub friction_modifier: f64,
    pub air_drag_modifier_present: bool,
    pub friction_modifier_present: bool,
    pub sneaking_speed: Option<f64>,
    pub fall_damage: f64,
    pub fall_flying: bool,
    pub look: Vec3,
    pub firework_boost: Vec3,
    pub stuck_speed_multiplier: Vec3,
    pub previous_position: Option<Vec3>,
    pub bounding_box: Option<Aabb>,
}

impl From<EntityState> for SimulationState {
    fn from(value: EntityState) -> Self {
        Self {
            position: value.position,
            velocity: value.velocity,
            yaw: 0.0,
            pitch: 0.0,
            half_width: value.dimensions.half_width,
            height: value.dimensions.height,
            on_ground: value.flags.on_ground,
            collided_horizontally: value.flags.horizontal_collision,
            minor_horizontal_collision: false,
            minor_horizontal_collision_supported: false,
            collided_vertically: value.flags.vertical_collision,
            vertical_collision_below: value.flags.vertical_collision_below,
            block_fall_callback: false,
            entity_bounciness: 0.0,
            block_bounciness: 0.0,
            input: Vec3::ZERO,
            jump_requested: false,
            jump_ticks: 0,
            auto_jump: false,
            auto_jump_time: 0,
            riptide_launch: None,
            sprinting: false,
            was_sprinting: false,
            sprint_modifier: None,
            in_water: value.flags.in_water,
            in_lava: value.flags.in_lava,
            ultra_warm: false,
            is_player: false,
            controlled_by_local_instance: false,
            passenger: false,
            in_web: false,
            weaving: false,
            sweet_berry_bush_immune: false,
            powder_snow_immune: false,
            powder_snow_walkable: false,
            in_powder_snow: false,
            ticks_frozen: 0,
            ticks_required_to_freeze: 140,
            tick_count: 0,
            freeze_damage: 0.0,
            contact_damage: 0.0,
            inside_wall: false,
            eye_height: None,
            eye_in_water: false,
            eye_in_lava: false,
            slow_falling: false,
            levitation: false,
            levitation_amplifier: None,
            fall_distance: 0.0,
            safe_fall_distance: 0.0,
            fall_damage_immune: false,
            fall_damage_multiplier: Some(1.0),
            sneaking: false,
            moving_slowly: None,
            stepping_carefully: false,
            using_item: false,
            item_use_speed_multiplier: None,
            swimming: false,
            flying: false,
            flying_speed: 0.02,
            climbing: false,
            no_physics: false,
            no_gravity: false,
            removed: false,
            gravity: None,
            step_height: None,
            movement_speed: None,
            jump_strength: None,
            speed_effect_amplifier: None,
            slowness_effect_amplifier: None,
            jump_boost_amplifier: None,
            dolphins_grace: 0,
            depth_strider: 0,
            soul_speed: 0,
            water_movement_efficiency: 0.0,
            movement_efficiency: 0.0,
            air_drag_modifier: 0.0,
            friction_modifier: 0.0,
            air_drag_modifier_present: false,
            friction_modifier_present: false,
            sneaking_speed: None,
            fall_damage: 0.0,
            fall_flying: false,
            look: Vec3::new(0.0, 0.0, 1.0),
            firework_boost: Vec3::ZERO,
            stuck_speed_multiplier: Vec3::ZERO,
            previous_position: None,
            bounding_box: None,
        }
    }
}

impl From<SimulationState> for EntityState {
    fn from(value: SimulationState) -> Self {
        Self {
            position: value.position,
            velocity: value.velocity,
            dimensions: Dimensions::new(value.half_width, value.height),
            flags: EntityFlags {
                on_ground: value.on_ground,
                horizontal_collision: value.collided_horizontally,
                vertical_collision: value.collided_vertically,
                vertical_collision_below: value.vertical_collision_below,
                in_water: value.in_water,
                in_lava: value.in_lava,
                ..EntityFlags::default()
            },
        }
    }
}

impl SimulationState {
    #[must_use]
    pub fn bounds(self) -> Aabb {
        self.bounding_box
            .unwrap_or_else(|| self.bounds_around_position())
    }

    #[must_use]
    pub fn bounds_around_position(self) -> Aabb {
        Aabb::new(
            Vec3::new(
                self.position.x - self.half_width,
                self.position.y,
                self.position.z - self.half_width,
            ),
            Vec3::new(
                self.position.x + self.half_width,
                self.position.y + self.height,
                self.position.z + self.half_width,
            ),
        )
    }

    pub fn check_fall_distance_accumulation(&mut self) {
        if self.velocity.y > -0.5 && self.fall_distance > 1.0 {
            self.fall_distance = 1.0;
        }
    }

    pub fn add_delta_movement(&mut self, impulse: Vec3) {
        if impulse.is_finite() {
            let candidate = self.velocity + impulse;
            if candidate.is_finite() {
                self.velocity = candidate;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TickResult {
    pub displacement: Vec3,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub block_fall_callback: bool,
}

#[derive(Clone, Debug)]
pub struct TickInputSnapshot {
    pub state: SimulationState,
    pub world: WorldSnapshot,
    pub config: PhysicsConfig,
    pub rules: VersionPhysicsRules,
    pub landing_damage_rounding: LandingDamageRounding,
    pub restitution_enabled: bool,
    pub modern_fall_damage: bool,
    pub water_movement_efficiency_attribute: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TickOutputSnapshot {
    pub state: SimulationState,
    pub result: TickResult,
}

impl TickInputSnapshot {
    #[must_use]
    pub fn new(
        state: SimulationState,
        world: WorldSnapshot,
        config: PhysicsConfig,
        rules: VersionPhysicsRules,
        landing_damage_rounding: LandingDamageRounding,
        restitution_enabled: bool,
        modern_fall_damage: bool,
        water_movement_efficiency_attribute: bool,
    ) -> Self {
        Self {
            state,
            world,
            config,
            rules,
            landing_damage_rounding,
            restitution_enabled,
            modern_fall_damage,
            water_movement_efficiency_attribute,
        }
    }

    #[must_use]
    pub fn execute(&self) -> TickOutputSnapshot {
        let mut state = self.state;
        let result = tick_with_rules_and_rounding_and_restitution(
            &mut state,
            &self.world,
            self.config,
            self.rules,
            self.landing_damage_rounding,
            self.restitution_enabled,
            self.modern_fall_damage,
        );
        TickOutputSnapshot { state, result }
    }
}

#[cfg(test)]
pub fn tick(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> TickResult {
    tick_with_rules(state, world, config, VersionPhysicsRules::default())
}

#[cfg(test)]
pub fn tick_with_rules(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
    rules: VersionPhysicsRules,
) -> TickResult {
    tick_with_rules_and_rounding_and_restitution(
        state,
        world,
        config,
        rules,
        LandingDamageRounding::Ceil,
        false,
        false,
    )
}

pub(crate) fn push_by_fluid(
    state: &mut SimulationState,
    contact: FluidContact,
    scale: f64,
    release: MinecraftVersion,
) {
    let average = || contact.flow.scale(1.0 / f64::from(contact.flowing_blocks));
    let current = if release.at_least(26, 1, 0) {
        if contact.flowing_blocks == 0 || contact.flow.length_squared() < 9.999999747378752e-6 {
            return;
        }
        if state.is_player {
            average()
        } else {
            normalize(contact.flow, release)
        }
    } else {
        if !(vector_length(contact.flow, release) > 0.0) {
            return;
        }
        let current = if contact.flowing_blocks > 0 {
            average()
        } else {
            contact.flow
        };
        if state.is_player {
            current
        } else {
            normalize(current, release)
        }
    };
    let mut push = current.scale(scale);
    if release.at_least(1, 16, 0)
        && state.velocity.x.abs() < 0.003
        && state.velocity.z.abs() < 0.003
        && vector_length(push, release) < 0.0045000000000000005
    {
        push = normalize(push, release).scale(0.0045000000000000005);
    }
    state.velocity = state.velocity + push;
}

fn water_contact(world: &WorldSnapshot, bounds: Aabb, release: MinecraftVersion) -> FluidContact {
    if release.at_least(1, 14, 0) {
        world.fluid_contact(FluidState::Water, bounds, release)
    } else {
        legacy_fluid::fluid_contact_1_13(world, FluidState::Water, bounds)
    }
}

fn vector_length(vector: Vec3, release: MinecraftVersion) -> f64 {
    let length = vector.length_squared().sqrt();
    if release.at_least(1, 17, 0) {
        length
    } else {
        java_float(length)
    }
}

fn normalize(vector: Vec3, release: MinecraftVersion) -> Vec3 {
    let length = vector_length(vector, release);
    if length < 1.0e-4 {
        Vec3::ZERO
    } else {
        Vec3::new(vector.x / length, vector.y / length, vector.z / length)
    }
}

fn apply_fluid_travel_vertical(
    state: &mut SimulationState,
    fluid: FluidState,
    fluid_height: f64,
    fluid_jump_threshold: f64,
    rules: VersionPhysicsRules,
    gravity: f64,
    going_down: bool,
) {
    match fluid {
        FluidState::Water | FluidState::Waterlogged => {
            state.velocity.y *= java_float(rules.fluid_vertical_drag);
            if fluid_falling_gravity_enabled(state, rules, state.sprinting) {
                state.velocity.y = fluid_falling_adjusted_y(gravity, going_down, state.velocity.y);
            }
        }
        FluidState::Lava => {
            if rules.release.at_least(1, 16, 0) && fluid_height <= fluid_jump_threshold {
                state.velocity.y *= java_float(rules.fluid_vertical_drag);
                if fluid_falling_gravity_enabled(state, rules, state.sprinting) {
                    state.velocity.y =
                        fluid_falling_adjusted_y(gravity, going_down, state.velocity.y);
                }
            } else {
                state.velocity.y *= 0.5;
            }
            if !state.no_gravity {
                state.velocity.y -= gravity / rules.lava_gravity_divisor;
            }
        }
        FluidState::Empty => {}
    }
}

pub(crate) fn blocks_inside(
    state: &SimulationState,
    release: MinecraftVersion,
) -> Vec<(i32, i32, i32)> {
    let inset = if !release.at_least(1, 19, 3) {
        0.001
    } else if !release.at_least(1, 21, 2) {
        1.0e-7
    } else {
        f64::from(1.0e-5_f32)
    };
    let inside = state.bounds().deflate(Vec3::new(inset, inset, inset));
    let min = block_position(inside.min);
    let max = block_position(inside.max);
    let mut blocks = Vec::new();
    if release.at_least(1, 21, 2) {
        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    blocks.push((x, y, z));
                }
            }
        }
    } else {
        for x in min.0..=max.0 {
            for y in min.1..=max.1 {
                for z in min.2..=max.2 {
                    blocks.push((x, y, z));
                }
            }
        }
    }
    blocks
}

fn stuck_speed_multiplier_inside(
    state: &SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> Option<Vec3> {
    let in_powder_snow = world.effect_at(block_position(state.position)) == BlockEffect::PowderSnow;
    blocks_inside(state, release)
        .into_iter()
        .filter_map(|position| match world.effect_at(position) {
            BlockEffect::Web if state.weaving => Some(Vec3::new(0.5, 0.25, 0.5)),
            BlockEffect::Web => Some(Vec3::new(0.25, java_float(0.05), 0.25)),
            BlockEffect::SweetBerryBush if !state.sweet_berry_bush_immune => {
                Some(Vec3::new(java_float(0.8), 0.75, java_float(0.8)))
            }
            BlockEffect::PowderSnow if in_powder_snow => {
                Some(Vec3::new(java_float(0.9), 1.5, java_float(0.9)))
            }
            _ => None,
        })
        .last()
}

pub(crate) fn block_speed_factor(
    world: &WorldSnapshot,
    state: &SimulationState,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> f32 {
    if state.is_player && (state.flying || state.fall_flying) {
        return 1.0;
    }
    let factor_at = |position| match world.effect_at(position) {
        BlockEffect::SoulSand | BlockEffect::Honey => 0.4_f32,
        _ => 1.0,
    };
    let below = block_below_affecting_movement(state, support, release);
    let at_position = factor_at(block_position(state.position));
    let factor = if at_position == 1.0 {
        factor_at(below)
    } else {
        at_position
    };
    if release.at_least(1, 21, 0) {
        factor + state.movement_efficiency as f32 * (1.0 - factor)
    } else if release.at_least(1, 16, 0)
        && state.soul_speed > 0
        && matches!(
            world.effect_at(below),
            BlockEffect::SoulSand | BlockEffect::SoulSoil
        )
    {
        1.0
    } else {
        factor
    }
}

pub(crate) fn block_below_affecting_movement(
    state: &SimulationState,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> BlockPosition {
    let below = |offset: f64| {
        (
            state.position.x.floor() as i32,
            (state.position.y - offset).floor() as i32,
            state.position.z.floor() as i32,
        )
    };
    if !release.at_least(1, 15, 0) {
        return below(1.0);
    }
    if !release.at_least(1, 20, 0) {
        return below(0.5000001);
    }
    on_position(state, support, 0.500001)
}

pub(crate) fn landing_position(
    state: &SimulationState,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> BlockPosition {
    if release.at_least(1, 20, 0) {
        on_position(state, support, 0.2)
    } else {
        block_position(state.position - Vec3::new(0.0, f64::from(0.2_f32), 0.0))
    }
}

fn on_position(
    state: &SimulationState,
    support: Option<BlockPosition>,
    offset: f32,
) -> BlockPosition {
    let below = block_position(state.position - Vec3::new(0.0, f64::from(offset), 0.0));
    match support {
        Some((x, _, z)) => (x, below.1, z),
        None => below,
    }
}

/// Entity.mainSupportingBlockPos (1.20+, checkSupportingBlock): the block under an entity on the
/// ground that supports it, else, after a move that leaves the box over nothing, the one under
/// the box where the move started.
pub(crate) fn main_supporting_block(
    world: &WorldSnapshot,
    state: &SimulationState,
    movement: Option<Vec3>,
) -> Option<BlockPosition> {
    if !state.on_ground {
        return None;
    }
    let bounds = state.bounds();
    let area = Aabb::new(
        Vec3::new(bounds.min.x, bounds.min.y - 1.0e-6, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
    );
    world
        .supporting_block_position(area, state.position)
        .or_else(|| {
            let movement = movement?;
            let start = area.translated(Vec3::new(-movement.x, 0.0, -movement.z));
            world.supporting_block_position(start, state.position)
        })
}

pub(crate) fn apply_block_effects(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
    inside: &[((i32, i32, i32), bool)],
) {
    if state.no_physics {
        return;
    }
    let stepping_carefully = state.stepping_carefully || state.sneaking;
    if state.on_ground {
        match world.effect_at(landing_position(state, support, release)) {
            BlockEffect::Slime if !stepping_carefully && state.velocity.y.abs() < 0.1 => {
                let step_factor = 0.4 + state.velocity.y.abs() * 0.2;
                state.velocity.x *= step_factor;
                state.velocity.z *= step_factor;
            }
            BlockEffect::Magma if !stepping_carefully && !state.fall_flying => {
                state.contact_damage += 1.0;
            }
            _ => {}
        }
    }
    if !(state.is_player && state.flying) {
        if let Some(multiplier) = stuck_speed_multiplier_inside(state, world, release) {
            state.stuck_speed_multiplier = multiplier;
            state.fall_distance = 0.0;
        }
    }
    let pushed_by_bubbles = !(release.at_least(1, 21, 2) && state.is_player && state.flying);
    for &(position, precise) in inside {
        match world.effect_at(position) {
            effect @ (BlockEffect::BubbleUp | BlockEffect::BubbleDown)
                if pushed_by_bubbles && precise =>
            {
                let drag_down = effect == BlockEffect::BubbleDown;
                push_by_bubble_column(state, world, position, drag_down, release);
            }
            BlockEffect::SoulSand if !release.at_least(1, 15, 0) => {
                state.velocity.x *= 0.4;
                state.velocity.z *= 0.4;
            }
            BlockEffect::Honey if sliding_down_honey(state, position, release) => {
                let falling = honey_slide_falling(state.velocity.y, release);
                let scale = if falling < -0.13 {
                    -0.05 / falling
                } else {
                    1.0
                };
                let slide = if release.at_least(1, 21, 2) {
                    (-0.05 - 0.08) * java_float(0.98)
                } else {
                    -0.05
                };
                state.velocity =
                    Vec3::new(state.velocity.x * scale, slide, state.velocity.z * scale);
                state.fall_distance = 0.0;
            }
            _ => {}
        }
    }
}

/// The moves Entity.applyEffectsFromBlocks checks from 1.21.2: the tick's move, recorded by
/// Entity.move from 1.21.5 with the movement whose axis order splits it (the collided one, the
/// requested one from 1.21.9), and the rest of the way to the position.
pub(crate) fn movements_this_tick(
    state: &SimulationState,
    old_position: Vec3,
    requested: Vec3,
    moved: Vec3,
    release: MinecraftVersion,
) -> Vec<Movement> {
    let whole_tick = Movement {
        from: old_position,
        to: state.position,
        axis_order: None,
    };
    let moved_length = moved.length_squared();
    let recorded = moved_length > 1.0e-7 || requested.length_squared() - moved_length < 1.0e-7;
    if !release.at_least(1, 21, 5) || state.no_physics || !recorded {
        return vec![whole_tick];
    }
    let to = old_position + moved;
    let axis_order = if release.at_least(1, 21, 9) {
        requested
    } else {
        moved
    };
    let mut movements = vec![Movement {
        from: old_position,
        to,
        axis_order: Some(axis_order),
    }];
    if to.distance_squared(state.position) > 9.999999439624929e-11 {
        movements.push(Movement {
            from: to,
            to: state.position,
            axis_order: None,
        });
    }
    movements
}

/// BubbleColumnBlock.entityInside: a column with nothing above it
/// (Entity.onAboveBubbleCol) pushes harder and does not end a fall.
fn push_by_bubble_column(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    (x, y, z): (i32, i32, i32),
    drag_down: bool,
    release: MinecraftVersion,
) {
    let above = (x, y + 1, z);
    let nothing_above = if release.at_least(1, 21, 5) {
        !world.has_collision_shape(above) && world.fluid_at(above) == FluidState::Empty
    } else {
        world.is_air_at(above)
    };
    state.velocity.y = match (nothing_above, drag_down) {
        (true, true) => (state.velocity.y - 0.03).max(-0.9),
        (true, false) => (state.velocity.y + 0.1).min(1.8),
        (false, true) => (state.velocity.y - 0.03).max(-0.3),
        (false, false) => (state.velocity.y + 0.06).min(0.7),
    };
    if !nothing_above {
        state.fall_distance = 0.0;
    }
}

fn honey_slide_falling(velocity_y: f64, release: MinecraftVersion) -> f64 {
    if release.at_least(1, 21, 2) {
        velocity_y / java_float(0.98) + 0.08
    } else {
        velocity_y
    }
}

fn sliding_down_honey(
    state: &SimulationState,
    (x, y, z): (i32, i32, i32),
    release: MinecraftVersion,
) -> bool {
    let reach = 0.4375 + state.half_width;
    !state.on_ground
        && state.position.y <= f64::from(y) + 0.9375 - 1.0e-7
        && honey_slide_falling(state.velocity.y, release) < -0.08
        && ((f64::from(x) + 0.5 - state.position.x).abs() + 1.0e-7 > reach
            || (f64::from(z) + 0.5 - state.position.z).abs() + 1.0e-7 > reach)
}

/// Where a move meets the entity colliders: since 1.14 the swept box grown by 1.0E-7
/// (Level.getEntityCollisions); before, the blocks' query (World.getCollisionBoxes).
fn entity_collision_query(swept: Aabb, query: Aabb, release: MinecraftVersion) -> Aabb {
    if release.at_least(1, 14, 0) {
        swept.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    } else {
        query
    }
}

pub(crate) fn climbable(world: &WorldSnapshot, state: &SimulationState) -> bool {
    !state.no_physics
        && (state.climbing
            || matches!(
                world.effect_at(block_position(state.position)),
                BlockEffect::Climbable | BlockEffect::Scaffolding
            ))
}

/// Before 1.17 an entity keeps its bounding box between ticks: Entity.move moves the box and takes
/// the position from it. A pose of another size refits it as Entity.refreshDimensions does.
fn kept_bounding_box(state: &SimulationState) -> Aabb {
    let around_position = state.bounds_around_position();
    let Some(kept) = state.bounding_box else {
        return around_position;
    };
    let kept_half_width = (kept.max.x - kept.min.x) / 2.0;
    // a moved box keeps its size up to rounding
    let same_size = |box_size: f64, size: f64| (box_size - size).abs() < 1.0e-4;
    let same_width = same_size(kept_half_width, state.half_width);
    if same_width && same_size(kept.max.y - kept.min.y, state.height) {
        kept
    } else if !same_width && state.half_width < kept_half_width {
        around_position
    } else {
        let width = state.half_width * 2.0;
        Aabb::new(kept.min, kept.min + Vec3::new(width, state.height, width))
    }
}

pub(crate) fn position_of_bounding_box(bounds: Aabb) -> Vec3 {
    Vec3::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        bounds.min.y,
        (bounds.min.z + bounds.max.z) / 2.0,
    )
}

/// Whether the chunk column under the entity is loaded, as LivingEntity.travel asks before 1.14
/// (a client entity over one that is not sinks at 0.1): its x and z cast to int on 1.8, floored
/// since.
fn column_loaded(world: &WorldSnapshot, position: Vec3, release: MinecraftVersion) -> bool {
    let block = |value: f64| {
        if release.at_least(1, 9, 0) {
            value.floor() as i32
        } else {
            value as i32
        }
    };
    world.is_chunk_loaded(block(position.x) >> 4, block(position.z) >> 4)
}

pub(crate) fn block_position(position: Vec3) -> (i32, i32, i32) {
    (
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

pub(crate) fn fluid_falling_adjusted_y(gravity: f64, falling: bool, current_y: f64) -> f64 {
    if falling && (current_y - 0.005).abs() >= 0.003 && (current_y - gravity / 16.0).abs() < 0.003 {
        -0.003
    } else {
        current_y - gravity / 16.0
    }
}

fn fluid_falling_gravity_enabled(
    state: &SimulationState,
    rules: VersionPhysicsRules,
    sprint_active: bool,
) -> bool {
    !sprint_active
        && !state.no_gravity
        && (!rules.fluid_falling_checks_gravity_argument || rules.gravity != 0.0)
}

fn flying_speed(state: &SimulationState, release: MinecraftVersion) -> f64 {
    if state.flying && !state.passenger {
        let speed = state.flying_speed as f32;
        return f64::from(if state.sprinting { speed * 2.0 } else { speed });
    }
    if !state.is_player {
        return java_float(0.02);
    }
    let sprinting = if release.at_least(1, 19, 4) {
        state.sprinting
    } else {
        state.was_sprinting
    };
    if !sprinting {
        java_float(0.02)
    } else if release.at_least(1, 19, 4) {
        f64::from(0.025999999_f32)
    } else if release.at_least(1, 18, 2) {
        f64::from(0.02_f32 + 0.006_f32)
    } else {
        f64::from((f64::from(0.02_f32) + 0.005999999865889549) as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FluidJump {
    Swim,
    Ground,
    None,
}

/// LivingEntity.aiStep's jump: swim up (jumpInLiquid) in fluid deeper than
/// the jump threshold or off the ground, else jump from the ground. Before
/// 1.16 the water height is held against 0.4 and any lava swims up; before
/// 1.13 any water or lava the entity is in does.
fn fluid_jump(
    state: &SimulationState,
    water_height: f64,
    lava_height: f64,
    threshold: f64,
    release: MinecraftVersion,
) -> FluidJump {
    if legacy_fluid::applies(release) {
        return if state.in_water || state.in_lava {
            FluidJump::Swim
        } else if state.on_ground {
            FluidJump::Ground
        } else {
            FluidJump::None
        };
    }
    if !release.at_least(1, 16, 0) {
        let in_water = water_height > 0.0;
        return if in_water && (!state.on_ground || water_height > 0.4) || state.in_lava {
            FluidJump::Swim
        } else if state.on_ground || in_water && water_height <= 0.4 {
            FluidJump::Ground
        } else {
            FluidJump::None
        };
    }
    let height = if state.in_lava {
        lava_height
    } else {
        water_height
    };
    let in_water = state.in_water && height > 0.0;
    let swims = !state.on_ground || height > threshold;
    if (in_water || state.in_lava) && swims {
        FluidJump::Swim
    } else if state.on_ground || in_water && height <= threshold {
        FluidJump::Ground
    } else {
        FluidJump::None
    }
}

fn jump_power(
    state: &SimulationState,
    config: PhysicsConfig,
    release: MinecraftVersion,
    block_jump_factor: f32,
) -> f64 {
    let strength = state.jump_strength.unwrap_or(config.jump_velocity);
    let power = strength as f32 * block_jump_factor;
    let boost = state
        .jump_boost_amplifier
        .map_or(0.0, |amplifier| 0.1_f32 * (f32::from(amplifier) + 1.0));
    let boost_added_in_double =
        !release.at_least(1, 14, 0) || (release.at_least(1, 17, 1) && !release.at_least(1, 20, 0));
    if boost_added_in_double {
        f64::from(power) + f64::from(boost)
    } else {
        f64::from(power + boost)
    }
}

pub(crate) fn friction_influenced_speed(
    speed: f64,
    friction: f64,
    release: MinecraftVersion,
) -> f64 {
    if !release.at_least(1, 14, 0) {
        let inertia = friction as f32 * 0.91;
        let acceleration = if release.at_least(1, 13, 0) {
            0.16277137_f32
        } else {
            0.16277136_f32
        };
        return f64::from(speed as f32 * (acceleration / (inertia * inertia * inertia)));
    }
    let friction = friction as f32;
    if release.at_least(26, 2, 0) && f64::from(friction) <= 0.6 {
        return f64::from(speed as f32);
    }
    f64::from(speed as f32 * (0.21600002_f32 / (friction * friction * friction)))
}

fn glide(state: &mut SimulationState, gravity: f64, release: MinecraftVersion) {
    let look = state.look;
    let look_horizontal = (look.x * look.x + look.z * look.z).sqrt();
    let horizontal_speed =
        (state.velocity.x * state.velocity.x + state.velocity.z * state.velocity.z).sqrt();
    let look_factor = if release.at_least(1, 21, 2) {
        1.0
    } else {
        (vector_length(look, release) / 0.4).min(1.0)
    };
    let lift = if release.at_least(1, 18, 2) {
        let cos = trig::cos(state.pitch);
        cos * cos * look_factor
    } else {
        let cos = mth_cos(state.pitch, release);
        java_float(cos * cos * look_factor)
    };
    let mut velocity = state.velocity;
    velocity.y += if release.at_least(1, 13, 0) {
        gravity * (-1.0 + lift * 0.75)
    } else {
        -0.08 + lift * 0.06
    };
    if velocity.y < 0.0 && look_horizontal > 0.0 {
        let dive = velocity.y * -0.1 * lift;
        velocity = velocity
            + Vec3::new(
                look.x * dive / look_horizontal,
                dive,
                look.z * dive / look_horizontal,
            );
    }
    if state.pitch < 0.0 && look_horizontal > 0.0 {
        let climb = horizontal_speed * -mth_sin(state.pitch, release) * 0.04;
        velocity = velocity
            + Vec3::new(
                -look.x * climb / look_horizontal,
                climb * 3.2,
                -look.z * climb / look_horizontal,
            );
    }
    if look_horizontal > 0.0 {
        velocity = velocity
            + Vec3::new(
                (look.x / look_horizontal * horizontal_speed - velocity.x) * 0.1,
                0.0,
                (look.z / look_horizontal * horizontal_speed - velocity.z) * 0.1,
            );
    }
    state.velocity = Vec3::new(
        velocity.x * java_float(0.99),
        velocity.y * java_float(0.98),
        velocity.z * java_float(0.99),
    );
}

fn movement_input(state: &SimulationState, release: MinecraftVersion) -> Vec3 {
    let [strafe, forward] = if !state.is_player {
        scale_input([state.input.x as f32, state.input.z as f32], 0.98)
    } else if release.at_least(1, 21, 5) {
        modified_input(input_move_vector(state, release), state)
    } else {
        scale_input(input_move_vector(state, release), 0.98)
    };
    Vec3::new(f64::from(strafe), state.input.y, f64::from(forward))
}

/// The player's Input.getMoveVector during the move: the keys normalized from 1.21.5, before it
/// the impulses LocalPlayer scaled for sneaking and using an item.
fn input_move_vector(state: &SimulationState, release: MinecraftVersion) -> [f32; 2] {
    let keys = [state.input.x as f32, state.input.z as f32];
    if release.at_least(1, 21, 5) {
        let length = (keys[0] * keys[0] + keys[1] * keys[1]).sqrt();
        return if length < 1.0e-4 {
            [0.0, 0.0]
        } else {
            [keys[0] / length, keys[1] / length]
        };
    }
    let item_slows_first = release.at_least(1, 21, 4);
    let mut input = keys;
    if state.using_item && item_slows_first {
        input = scale_input(input, 0.2);
    }
    if state.moving_slowly.unwrap_or(state.sneaking) {
        input = if release.at_least(1, 19, 0) {
            scale_input(input, sneaking_speed(state))
        } else {
            input.map(|impulse| (f64::from(impulse) * 0.3) as f32)
        };
    }
    if state.using_item && !item_slows_first {
        input = scale_input(input, 0.2);
    }
    if state.flying && state.sneaking && !release.at_least(1, 15, 0) {
        input = input.map(|impulse| (f64::from(impulse) / 0.3) as f32);
    }
    input
}

fn modified_input(move_vector: [f32; 2], state: &SimulationState) -> [f32; 2] {
    if move_vector == [0.0, 0.0] {
        return move_vector;
    }
    let mut input = scale_input(move_vector, 0.98);
    if state.using_item {
        input = scale_input(input, state.item_use_speed_multiplier.unwrap_or(0.2));
    }
    if state.moving_slowly.unwrap_or(state.sneaking) {
        input = scale_input(input, sneaking_speed(state));
    }
    let length = (input[0] * input[0] + input[1] * input[1]).sqrt();
    if length <= 0.0 {
        return input;
    }
    let direction = scale_input(input, 1.0 / length);
    let (x, y) = (direction[0].abs(), direction[1].abs());
    let ratio = if y > x { x / y } else { y / x };
    scale_input(direction, (length * (1.0 + ratio * ratio).sqrt()).min(1.0))
}

fn scale_input(input: [f32; 2], factor: f32) -> [f32; 2] {
    [input[0] * factor, input[1] * factor]
}

fn sneaking_speed(state: &SimulationState) -> f32 {
    state.sneaking_speed.map_or(0.3, |speed| speed as f32)
}

pub(crate) fn mth_sin(radians: f64, release: MinecraftVersion) -> f64 {
    if release.at_least(1, 21, 11) {
        java_sin(radians)
    } else {
        java_sin_legacy(radians as f32)
    }
}

pub(crate) fn mth_cos(radians: f64, release: MinecraftVersion) -> f64 {
    if release.at_least(1, 21, 11) {
        java_cos(radians)
    } else {
        java_cos_legacy(radians as f32)
    }
}

pub(crate) fn relative_movement(
    input: Vec3,
    speed: f64,
    yaw: f64,
    release: MinecraftVersion,
) -> Vec3 {
    if !release.at_least(1, 14, 0) {
        return relative_movement_in_float(input, speed as f32, yaw as f32);
    }
    let length_squared = input.x * input.x + input.y * input.y + input.z * input.z;
    if length_squared < 1.0e-7 {
        return Vec3::ZERO;
    }
    let direction = if length_squared > 1.0 {
        normalize(input, release)
    } else {
        input
    };
    let scaled = direction.scale(speed);
    let sin = mth_sin(yaw, release);
    let cos = mth_cos(yaw, release);
    Vec3::new(
        scaled.x * cos - scaled.z * sin,
        scaled.y,
        scaled.z * cos + scaled.x * sin,
    )
}

fn relative_movement_in_float(input: Vec3, speed: f32, yaw: f32) -> Vec3 {
    let (strafe, up, forward) = (input.x as f32, input.y as f32, input.z as f32);
    let length_squared = strafe * strafe + up * up + forward * forward;
    if length_squared < 1.0e-4 {
        return Vec3::ZERO;
    }
    let scale = speed / length_squared.sqrt().max(1.0);
    let (strafe, up, forward) = (strafe * scale, up * scale, forward * scale);
    let sin = java_sin_legacy(yaw) as f32;
    let cos = java_cos_legacy(yaw) as f32;
    Vec3::new(
        f64::from(strafe * cos - forward * sin),
        f64::from(up),
        f64::from(forward * cos + strafe * sin),
    )
}

fn negligible_velocity(config: PhysicsConfig, release: MinecraftVersion) -> f64 {
    let customized = config.negligible_velocity != PhysicsConfig::default().negligible_velocity;
    if customized || release.at_least(1, 9, 0) {
        config.negligible_velocity
    } else {
        0.005
    }
}

fn compute_modified_friction(friction: f64, modifier: f64) -> f64 {
    f64::from((1.0 - (1.0 - friction as f32) * modifier as f32).clamp(0.0, 1.0))
}

fn effective_modifier(modifier: f64, present: bool) -> f64 {
    if present || modifier != 0.0 {
        modifier
    } else {
        1.0
    }
}

pub(crate) fn moves_position(requested: Vec3, movement: Vec3, release: MinecraftVersion) -> bool {
    let length_squared = movement.length_squared();
    length_squared > 1.0e-7
        || (release.at_least(1, 21, 2) && requested.length_squared() - length_squared < 1.0e-7)
}

pub(crate) fn horizontal_collision_is_minor(
    state: &SimulationState,
    movement: Vec3,
    release: MinecraftVersion,
) -> bool {
    let input = movement_input(state, release);
    let sin = mth_sin(state.yaw, release);
    let cos = mth_cos(state.yaw, release);
    let input_x = input.x * cos - input.z * sin;
    let input_z = input.z * cos + input.x * sin;
    let input_length_squared = input_x * input_x + input_z * input_z;
    let movement_length_squared = movement.x * movement.x + movement.z * movement.z;
    if input_length_squared < 9.999999747378752e-6 || movement_length_squared < 9.999999747378752e-6
    {
        return false;
    }
    let dot = input_x * movement.x + input_z * movement.z;
    (dot / (input_length_squared * movement_length_squared).sqrt()).acos() < 0.13962633907794952
}

pub(crate) fn back_off_from_edge(
    state: &SimulationState,
    world: &WorldSnapshot,
    context: &EntityCollisionContext,
    delta: Vec3,
    max_down_step: f64,
    release: MinecraftVersion,
) -> Vec3 {
    if !release.at_least(1, 14, 0) {
        return back_off_from_edge_before_1_14(
            state,
            world,
            context,
            delta,
            max_down_step,
            release,
        );
    }
    if state.no_physics
        || !state.is_player
        || !state.sneaking
        || state.flying
        || delta.y > 0.0
        || max_down_step <= 0.0
    {
        return delta;
    }
    let bounds = state.bounds();
    let above_ground = state.on_ground
        || (state.fall_distance < max_down_step
            && !world.no_collision_for_entity(
                bounds.translated(Vec3::new(0.0, -max_down_step + state.fall_distance, 0.0)),
                context,
            ));
    if !above_ground {
        return delta;
    }
    let can_fall = |x: f64, z: f64| {
        let lowered = Aabb::new(
            Vec3::new(
                bounds.min.x + 1.0e-7 + x,
                bounds.min.y - max_down_step - 1.0e-7,
                bounds.min.z + 1.0e-7 + z,
            ),
            Vec3::new(
                bounds.max.x - 1.0e-7 + x,
                bounds.min.y,
                bounds.max.z - 1.0e-7 + z,
            ),
        );
        world.no_collision_for_entity(lowered, context)
    };
    back_off_while(delta, can_fall)
}

/// Entity.move's sneak edge check before 1.14: whole box moved down by 1, from 1.11 by the step
/// height, must meet no block for a grounded sneaking player's move to back off.
fn back_off_from_edge_before_1_14(
    state: &SimulationState,
    world: &WorldSnapshot,
    context: &EntityCollisionContext,
    delta: Vec3,
    step_height: f64,
    release: MinecraftVersion,
) -> Vec3 {
    if state.no_physics || !state.is_player || !state.sneaking || !state.on_ground {
        return delta;
    }
    let drop = if release.at_least(1, 11, 0) {
        f64::from(step_height as f32)
    } else {
        1.0
    };
    let bounds = state.bounds();
    let can_fall = |x: f64, z: f64| {
        let mut lowered = bounds.translated(Vec3::new(x, -drop, z));
        if release.at_least(1, 13, 0) {
            lowered = lowered.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        }
        !world
            .collision_shapes_for_entity(lowered, context)
            .iter()
            .any(|shape| shape.intersects(lowered))
    };
    back_off_while(delta, can_fall)
}

/// The 0.05 steps by which a sneaking move backs off while it would leave the ground: on x, on z,
/// then on both while the diagonal move would.
fn back_off_while(delta: Vec3, can_fall: impl Fn(f64, f64) -> bool) -> Vec3 {
    let mut x = delta.x;
    let mut z = delta.z;
    let step_x = x.signum() * 0.05;
    let step_z = z.signum() * 0.05;
    while x != 0.0 && can_fall(x, 0.0) {
        if x.abs() <= 0.05 {
            x = 0.0;
        } else {
            x -= step_x;
        }
    }
    while z != 0.0 && can_fall(0.0, z) {
        if z.abs() <= 0.05 {
            z = 0.0;
        } else {
            z -= step_z;
        }
    }
    while x != 0.0 && z != 0.0 && can_fall(x, z) {
        if x.abs() <= 0.05 {
            x = 0.0;
        } else {
            x -= step_x;
        }
        if z.abs() <= 0.05 {
            z = 0.0;
        } else {
            z -= step_z;
        }
    }
    Vec3::new(x, delta.y, z)
}

/// LocalPlayer.moveTowardsClosestSpace (checkInBlock before 1.16): from each corner of the box
/// shrunk to 0.35 of its width, a player inside a suffocating block moves at 0.1 toward the
/// nearest side of that block that is open.
fn move_towards_closest_space(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    if !release.at_least(1, 14, 0) {
        push_out_of_blocks_before_1_14(state, world, release);
        return;
    }
    let bounds = state.bounds();
    let suffocates_at = |x: i32, z: i32| {
        if release.at_least(1, 16, 0) {
            let column = Aabb::new(
                Vec3::new(f64::from(x), bounds.min.y, f64::from(z)),
                Vec3::new(f64::from(x) + 1.0, bounds.max.y, f64::from(z) + 1.0),
            );
            world.collides_with_suffocating_block(column.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7)))
        } else {
            (bounds.min.y.floor() as i32..bounds.max.y.ceil() as i32)
                .any(|y| world.is_suffocating((x, y, z)))
        }
    };
    let offset = state.half_width * 2.0 * 0.35;
    let corners = [
        (-offset, offset),
        (-offset, -offset),
        (offset, -offset),
        (offset, offset),
    ];
    for (offset_x, offset_z) in corners {
        let x = state.position.x + offset_x;
        let z = state.position.z + offset_z;
        let (cell_x, cell_z) = (x.floor() as i32, z.floor() as i32);
        if !suffocates_at(cell_x, cell_z) {
            continue;
        }
        let (inside_x, inside_z) = (x - f64::from(cell_x), z - f64::from(cell_z));
        let sides = [
            (-1, 0, inside_x),
            (1, 0, 1.0 - inside_x),
            (0, -1, inside_z),
            (0, 1, 1.0 - inside_z),
        ];
        let mut closest = if release.at_least(1, 16, 0) {
            f64::MAX
        } else {
            9999.0
        };
        let mut direction = None;
        for (step_x, step_z, distance) in sides {
            if distance < closest && !suffocates_at(cell_x + step_x, cell_z + step_z) {
                closest = distance;
                direction = Some((step_x, step_z));
            }
        }
        match direction {
            Some((0, step_z)) => state.velocity.z = 0.1 * f64::from(step_z),
            Some((step_x, _)) => state.velocity.x = 0.1 * f64::from(step_x),
            None => {}
        }
    }
}

fn push_out_of_blocks_before_1_14(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let open = |(x, y, z): BlockPosition| {
        !world.is_suffocating((x, y, z)) && !world.is_suffocating((x, y + 1, z))
    };
    let feet_y = state.bounds().min.y + 0.5;
    let offset = state.half_width * 2.0 * 0.35;
    let corners = [
        (-offset, offset),
        (-offset, -offset),
        (offset, -offset),
        (offset, offset),
    ];
    for (offset_x, offset_z) in corners {
        let (x, z) = (state.position.x + offset_x, state.position.z + offset_z);
        let cell = (x.floor() as i32, feet_y.floor() as i32, z.floor() as i32);
        let blocked = if release.at_least(1, 13, 0) && state.swimming {
            world.is_suffocating(cell)
        } else {
            !open(cell)
        };
        if !blocked {
            continue;
        }
        let (inside_x, inside_z) = (x - f64::from(cell.0), z - f64::from(cell.2));
        let sides = [
            (-1, 0, inside_x),
            (1, 0, 1.0 - inside_x),
            (0, -1, inside_z),
            (0, 1, 1.0 - inside_z),
        ];
        let mut closest = 9999.0;
        let mut direction = None;
        for (step_x, step_z, distance) in sides {
            if open((cell.0 + step_x, cell.1, cell.2 + step_z)) && distance < closest {
                closest = distance;
                direction = Some((step_x, step_z));
            }
        }
        let push = f64::from(0.1_f32);
        match direction {
            Some((0, step_z)) => state.velocity.z = push * f64::from(step_z),
            Some((step_x, _)) => state.velocity.x = push * f64::from(step_x),
            None => {}
        }
    }
}

fn restitute_movement_after_collisions(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    rules: VersionPhysicsRules,
    collisions: [bool; 3],
    moved_y: f64,
) {
    let current = state.velocity;
    let suppressing_bounce = state.sneaking;
    let entity_restitution = if suppressing_bounce {
        0.0
    } else {
        state.entity_bounciness
    };
    if collisions[0] {
        state.velocity.x = -current.x * entity_restitution;
    }
    if collisions[2] {
        state.velocity.z = -current.z * entity_restitution;
    }
    if !collisions[1] {
        return;
    }
    let gravity = if state.no_gravity {
        0.0
    } else if state.slow_falling && current.y <= 0.0 {
        rules.gravity.min(0.01)
    } else {
        rules.gravity
    };
    let restitution = if !state.vertical_collision_below {
        entity_restitution
    } else {
        let effect_state = world.block(landing_position(state, support, rules.release));
        let resting = if rules.release.at_least(26, 3, 0) {
            -current.y <= gravity
        } else {
            -current.y < gravity
        };
        if resting || suppressing_bounce || effect_state.suppresses_bounce {
            0.0
        } else {
            let block_bounciness = if state.is_player {
                effect_state.bounce_restitution
            } else {
                f64::from(effect_state.bounce_restitution as f32 * 0.8_f32)
            };
            entity_restitution
                .max(block_bounciness)
                .max(state.block_bounciness)
        }
    };
    let (gravity_compensation, effective_drag) = if restitution > 0.0 {
        let portion_with_movement = moved_y / current.y;
        let air_drag = compute_modified_friction(
            rules.vertical_drag,
            effective_modifier(state.air_drag_modifier, state.air_drag_modifier_present),
        );
        (
            portion_with_movement * gravity,
            1.0 + portion_with_movement * (air_drag - 1.0),
        )
    } else {
        (0.0, 1.0)
    };
    state.velocity.y = (gravity_compensation - current.y) * effective_drag * restitution;
}

pub(crate) fn entity_collision_context(state: &SimulationState) -> EntityCollisionContext {
    EntityCollisionContext {
        bottom: state.position.y,
        descending: state.sneaking,
        fall_distance: state.fall_distance,
        walks_on_powder_snow: state.powder_snow_walkable,
    }
}

/// Entity.move inside cobwebs, berry bushes and powder snow: the movement scaled by the stuck
/// speed multiplier, which the move uses up along with the motion.
pub(crate) fn slowed_by_stuck_speed(state: &mut SimulationState, movement: Vec3) -> Vec3 {
    if state.no_physics || state.stuck_speed_multiplier.length_squared() <= 1.0e-7 {
        return movement;
    }
    let slowed = movement.multiply(state.stuck_speed_multiplier);
    state.stuck_speed_multiplier = Vec3::ZERO;
    state.velocity = Vec3::ZERO;
    slowed
}

/// Entity.collide: how much of `desired` the blocks and the world border let the entity move.
pub(crate) struct WorldCollision {
    pub(crate) movement: Vec3,
    moved_bounds: Option<Aabb>,
}

pub(crate) fn collide_with_world(
    state: &SimulationState,
    world: &WorldSnapshot,
    context: &EntityCollisionContext,
    desired: Vec3,
    rules: VersionPhysicsRules,
) -> WorldCollision {
    let bounds = state.bounds();
    let max_step = rules.step_height as f32;
    let swept = bounds.expand_towards(desired);
    let query = Aabb::new(
        Vec3::new(swept.min.x, swept.min.y - 1.0e-5, swept.min.z),
        Vec3::new(
            swept.max.x,
            swept.max.y.max(bounds.max.y + f64::from(max_step)),
            swept.max.z,
        ),
    );
    let (mut obstacles, entered) =
        world.collision_shapes_with_entered_for_entity(query, bounds, context);
    obstacles.extend(
        world.entity_collisions(entity_collision_query(swept, query, rules.release), context),
    );
    let border = world.world_border_collision_boxes();
    let collides_with_border = if rules.release.at_least(1, 18, 0) {
        world.is_inside_close_to_world_border(state.position, bounds.expand_towards(desired))
    } else {
        let inside = bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !border.iter().any(|outside| outside.intersects(inside))
    };
    if collides_with_border {
        obstacles.extend(border);
    }
    let collision = CollisionRules {
        step_up: if rules.release.at_least(1, 21, 0) {
            StepUp::CandidateHeights
        } else {
            StepUp::Lifted
        },
        border_always_collides: collides_with_border && !rules.release.at_least(1, 18, 0),
    };
    if rules.release.at_least(1, 14, 0) {
        let movement = collide(
            bounds,
            desired,
            &obstacles,
            &entered,
            state.on_ground,
            max_step,
            collision,
        );
        return WorldCollision {
            movement,
            moved_bounds: None,
        };
    }
    let voxel_shapes = rules.release.at_least(1, 13, 0);
    let legacy = collide_legacy(
        bounds,
        desired,
        &obstacles,
        state.on_ground,
        max_step,
        voxel_shapes,
    );
    WorldCollision {
        movement: legacy.movement,
        moved_bounds: Some(legacy.bounds),
    }
}

/// Moves the entity by the collided movement, and before 1.17 the bounding box it keeps, which the
/// position follows: before 1.14 to where the collision moved the box.
pub(crate) fn displace(
    state: &mut SimulationState,
    desired: Vec3,
    collision: WorldCollision,
    release: MinecraftVersion,
) {
    let moved = match (collision.moved_bounds, state.bounding_box) {
        (Some(moved), _) => moved,
        _ if !state.no_physics && !moves_position(desired, collision.movement, release) => return,
        (None, Some(bounds)) => bounds.translated(collision.movement),
        (None, None) => {
            state.position = state.position + collision.movement;
            return;
        }
    };
    state.bounding_box = Some(moved);
    state.position = position_of_bounding_box(moved);
}

/// TridentItem.releaseUsing (1.13+), before the tick: a riptide trident let go of in water pushes
/// the player along its look by the spin attack strength, and a player on the ground moves up by
/// 1.2 at once.
fn launch_riptide(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    level: u8,
    rules: VersionPhysicsRules,
) {
    let release = rules.release;
    let riding = state.passenger && release.at_least(26, 1, 0);
    if level == 0 || !release.at_least(1, 13, 0) || !state.in_water || riding {
        return;
    }
    let (yaw, pitch) = (f64::from(state.yaw as f32), f64::from(state.pitch as f32));
    let cos_pitch = mth_cos(pitch, release) as f32;
    let x = -(mth_sin(yaw, release) as f32) * cos_pitch;
    let y = -(mth_sin(pitch, release) as f32);
    let z = mth_cos(yaw, release) as f32 * cos_pitch;
    let length = (x * x + y * y + z * z).sqrt();
    let strength = if release.at_least(1, 21, 0) {
        1.5 + 0.75 * f32::from(level - 1)
    } else {
        3.0 * ((1.0 + f32::from(level)) / 4.0)
    };
    let push = Vec3::new(
        f64::from(x * (strength / length)),
        f64::from(y * (strength / length)),
        f64::from(z * (strength / length)),
    );
    state.velocity = state.velocity + push;
    if !state.on_ground {
        return;
    }
    let lift = slowed_by_stuck_speed(state, Vec3::new(0.0, 1.1999999284744263, 0.0));
    let context = entity_collision_context(state);
    let collision = collide_with_world(state, world, &context, lift, rules);
    let displacement = collision.movement;
    displace(state, lift, collision, release);
    state.collided_horizontally = false;
    state.minor_horizontal_collision = false;
    state.collided_vertically = lift.y != displacement.y;
    state.vertical_collision_below = false;
    state.on_ground = false;
    if state.collided_vertically {
        state.velocity.y = if release.at_least(1, 14, 0) {
            state.velocity.y * 0.0
        } else {
            0.0
        };
    }
    if release.at_least(1, 15, 0) {
        let speed_factor = f64::from(block_speed_factor(world, state, None, release));
        state.velocity.x *= speed_factor;
        state.velocity.z *= speed_factor;
    }
}

/// Block.getJumpFactor of the block at the feet, else of the block below that affects movement.
fn block_jump_factor(world: &WorldSnapshot, body: BlockPosition, below: BlockPosition) -> f32 {
    if world.effect_at(body) == BlockEffect::Honey || world.effect_at(below) == BlockEffect::Honey {
        0.5
    } else {
        1.0
    }
}

/// Entity.getForward, horizontally: Vec3.directionFromRotation of the pitch and yaw.
fn forward_direction(state: &SimulationState, release: MinecraftVersion) -> (f32, f32) {
    let yaw = -(state.yaw as f32) - 3.1415927;
    let pitch = -(state.pitch as f32);
    let horizontal = -(mth_cos(f64::from(pitch), release) as f32);
    (
        mth_sin(f64::from(yaw), release) as f32 * horizontal,
        mth_cos(f64::from(yaw), release) as f32 * horizontal,
    )
}

/// The inverse square root updateAutoJump normalizes the move with: Mth.fastInvSqrt in double
/// before 1.15, in float until 1.19.4, then Mth.invSqrt.
fn auto_jump_inverse_length(length_squared: f32, release: MinecraftVersion) -> f32 {
    if release.at_least(1, 19, 4) {
        1.0 / length_squared.sqrt()
    } else if release.at_least(1, 15, 0) {
        fast_inv_sqrt_f32(length_squared)
    } else {
        fast_inv_sqrt(f64::from(length_squared)) as f32
    }
}

/// LocalPlayer.updateAutoJump (1.10+, with the auto-jump option on), after a move by `moved`: a
/// player walking on the ground towards a step 0.5 to 1.2 high (more with jump boost) with room
/// above jumps the next tick. The step is the first block box that one of the two lines half a
/// width to each side, 0.51 above the feet, crosses within 7 ticks of speed (at least the move).
fn update_auto_jump(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    moved: Vec3,
    speed: f32,
    jump_factor: f32,
    release: MinecraftVersion,
) {
    if !state.auto_jump
        || !state.is_player
        || !release.at_least(1, 10, 0)
        || state.auto_jump_time > 0
        || !state.on_ground
        || state.sneaking
        || state.passenger
        || jump_factor < 1.0
    {
        return;
    }
    let move_vector = input_move_vector(state, release);
    if move_vector == [0.0, 0.0] {
        return;
    }
    let bounds = state.bounds();
    let feet_y = if release.at_least(1, 16, 0) {
        state.position.y
    } else {
        bounds.min.y
    };
    let begin = Vec3::new(state.position.x, feet_y, state.position.z);
    let (moved_x, moved_z) = (moved.x as f32, moved.z as f32);
    let mut direction = Vec3::new(f64::from(moved_x), 0.0, f64::from(moved_z));
    let mut length_squared = direction.length_squared() as f32;
    if length_squared <= 0.001 {
        let input_x = speed * move_vector[0];
        let input_z = speed * move_vector[1];
        let sin = mth_sin(state.yaw, release) as f32;
        let cos = mth_cos(state.yaw, release) as f32;
        direction = Vec3::new(
            f64::from(input_x * cos - input_z * sin),
            0.0,
            f64::from(input_z * cos + input_x * sin),
        );
        length_squared = direction.length_squared() as f32;
        if length_squared <= 0.001 {
            return;
        }
    }
    let inverse_length = auto_jump_inverse_length(length_squared, release);
    let direction = direction.scale(f64::from(inverse_length));
    let (forward_x, forward_z) = forward_direction(state, release);
    if ((f64::from(forward_x) * direction.x + f64::from(forward_z) * direction.z) as f32) < -0.15 {
        return;
    }
    let context = entity_collision_context(state);
    let empty = |position| {
        world
            .block_collision_boxes_for_entity(position, &context)
            .is_empty()
    };
    let mut ceiling = block_position(Vec3::new(state.position.x, bounds.max.y, state.position.z));
    if !empty(ceiling) {
        return;
    }
    ceiling.1 += 1;
    if !empty(ceiling) {
        return;
    }
    let mut jump_height = 1.2_f32;
    if let Some(amplifier) = state.jump_boost_amplifier {
        jump_height += (f32::from(amplifier) + 1.0) * 0.75;
    }
    let look_ahead = (speed * 7.0).max(1.0 / inverse_length);
    let end = begin + Vec3::new(f64::from(moved_x), 0.0, f64::from(moved_z));
    let end = end + direction.scale(f64::from(look_ahead));
    let width = (state.half_width * 2.0) as f32;
    let top = end + Vec3::new(0.0, f64::from(state.height as f32), 0.0);
    let area = Aabb::new(
        Vec3::new(begin.x.min(top.x), begin.y.min(top.y), begin.z.min(top.z)),
        Vec3::new(begin.x.max(top.x), begin.y.max(top.y), begin.z.max(top.z)),
    )
    .inflate(Vec3::new(f64::from(width), 0.0, f64::from(width)));
    let lift = Vec3::new(0.0, 0.5099999904632568, 0.0);
    let (line_begin, line_end) = (begin + lift, end + lift);
    let side = direction
        .cross(Vec3::new(0.0, 1.0, 0.0))
        .scale(f64::from(width * 0.5));
    let lines = [
        (line_begin - side, line_end - side),
        (line_begin + side, line_end + side),
    ];
    let crosses = |bounds: Aabb, (from, to): (Vec3, Vec3)| {
        bounds.min.x < from.x.max(to.x)
            && bounds.max.x > from.x.min(to.x)
            && bounds.min.y < from.y.max(to.y)
            && bounds.max.y > from.y.min(to.y)
            && bounds.min.z < from.z.max(to.z)
            && bounds.max.z > from.z.min(to.z)
    };
    let unset = f32::from_bits(1);
    let mut step_top = unset;
    for obstacle in world.block_collision_boxes_in_order(area, &context) {
        if !lines.iter().any(|&line| crosses(obstacle, line)) {
            continue;
        }
        step_top = obstacle.max.y as f32;
        let center = block_position(Vec3::new(
            lerp(0.5, obstacle.min.x, obstacle.max.x),
            lerp(0.5, obstacle.min.y, obstacle.max.y),
            lerp(0.5, obstacle.min.z, obstacle.max.z),
        ));
        let mut steps = 1;
        while (steps as f32) < jump_height {
            let above = (center.0, center.1 + steps, center.2);
            let shape = world.block_collision_boxes_for_entity(above, &context);
            if let Some(shape_top) = shape.iter().map(|bounds| bounds.max.y).reduce(f64::max) {
                step_top = shape_top as f32 + above.1 as f32;
                if f64::from(step_top) - feet_y > f64::from(jump_height) {
                    return;
                }
            }
            if steps > 1 {
                ceiling.1 += 1;
                if !empty(ceiling) {
                    return;
                }
            }
            steps += 1;
        }
        break;
    }
    if step_top != unset {
        let rise = (f64::from(step_top) - feet_y) as f32;
        if !(rise <= 0.5) && !(rise > jump_height) {
            state.auto_jump_time = 1;
        }
    }
}

pub(crate) fn tick_with_rules_and_rounding_and_restitution(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
    rules: VersionPhysicsRules,
    landing_damage_rounding: LandingDamageRounding,
    restitution_enabled: bool,
    modern_fall_damage: bool,
) -> TickResult {
    let mut rules = rules;
    state.tick_count = state.tick_count.saturating_add(1);
    if state.removed && rules.release.at_least(1, 19, 1) {
        return TickResult {
            displacement: Vec3::ZERO,
            collided_horizontally: state.collided_horizontally,
            collided_vertically: state.collided_vertically,
            block_fall_callback: false,
        };
    }
    state.bounding_box = (!rules.release.at_least(1, 17, 0)).then(|| kept_bounding_box(state));
    state.contact_damage = 0.0;
    state.block_fall_callback = false;
    if state.is_player && state.no_physics {
        state.on_ground = false;
    }
    if !state.velocity.is_finite() {
        state.velocity = Vec3::ZERO;
    }
    rules.gravity = config.gravity;
    // Keep source-generated version drag when callers use the default config;
    // an explicitly customized runtime drag remains an override.
    if config.drag != PhysicsConfig::default().drag {
        rules.air_drag = config.drag;
    }
    rules.step_height = config.step_height;
    if let Some(gravity) = state.gravity {
        rules.gravity = gravity;
    }
    if let Some(step_height) = state.step_height {
        rules.step_height = step_height;
    }
    if let Some(level) = state.riptide_launch.take() {
        launch_riptide(state, world, level, rules);
    }
    let old_position = state.position;
    let flight_resets_fall_before_travel = rules.release.at_least(1, 21, 2);
    if state.slow_falling || state.levitation || (state.flying && flight_resets_fall_before_travel)
    {
        state.fall_distance = 0.0;
    }
    state.jump_ticks = state.jump_ticks.saturating_sub(1);
    if state.auto_jump_time > 0 {
        state.auto_jump_time -= 1;
        state.jump_requested = true;
    }
    let body_position = (
        state.position.x.floor() as i32,
        state.position.y.floor() as i32,
        state.position.z.floor() as i32,
    );
    let previous_movement = state
        .previous_position
        .map(|previous| state.position - previous);
    let mut support = main_supporting_block(world, state, previous_movement);
    let below_position = block_below_affecting_movement(state, support, rules.release);
    let bounds = state.bounds();
    let eye_height = state.eye_height.unwrap_or(state.height * 0.85);
    let eye_half_width = state.half_width * 0.8;
    let eye_box = Aabb::new(
        Vec3::new(
            state.position.x - eye_half_width,
            state.position.y + eye_height - 0.5e-6,
            state.position.z - eye_half_width,
        ),
        Vec3::new(
            state.position.x + eye_half_width,
            state.position.y + eye_height + 0.5e-6,
            state.position.z + eye_half_width,
        ),
    );
    state.inside_wall = !state.no_physics
        && world
            .collision_shapes(eye_box)
            .iter()
            .any(|shape| shape.intersects(eye_box));
    let affected_by_fluids = !(state.is_player && state.flying);
    let legacy_fluids = legacy_fluid::applies(rules.release);
    let water = if legacy_fluids {
        legacy_fluid::handle_water_movement(state, world, affected_by_fluids, rules.release);
        FluidContact::default()
    } else {
        let water = water_contact(world, bounds, rules.release);
        state.in_water = water.touching;
        if state.in_water {
            state.fall_distance = 0.0;
            if affected_by_fluids {
                push_by_fluid(state, water, rules.water_flow_scale, rules.release);
            }
        }
        water
    };
    let lava = world.fluid_contact(FluidState::Lava, bounds, rules.release);
    if rules.release.at_least(1, 16, 0) {
        if affected_by_fluids {
            let lava_flow_scale = if state.ultra_warm {
                rules.ultra_warm_lava_flow_scale
            } else {
                rules.lava_flow_scale
            };
            push_by_fluid(state, lava, lava_flow_scale, rules.release);
        }
        state.in_lava = lava.height > 0.0;
    } else if !rules.release.at_least(1, 14, 0) {
        state.in_lava = legacy_fluid::touching_lava(world, bounds, rules.release);
    }
    if state.is_player && !state.no_physics {
        move_towards_closest_space(state, world, rules.release);
    }
    let sinks_in_water =
        !legacy_fluids && (affected_by_fluids || !rules.release.at_least(1, 16, 0));
    if state.is_player && state.in_water && state.sneaking && sinks_in_water {
        state.velocity.y -= 0.03999999910593033;
    }
    if state.flying {
        let direction = i8::from(state.jump_requested) - i8::from(state.sneaking);
        if direction != 0 {
            state.velocity.y += f64::from(f32::from(direction) * state.flying_speed as f32 * 3.0);
        }
    }
    let negligible_velocity = negligible_velocity(config, rules.release);
    if state.is_player && rules.release.at_least(1, 21, 5) {
        let horizontal_squared =
            state.velocity.x * state.velocity.x + state.velocity.z * state.velocity.z;
        if horizontal_squared < negligible_velocity * negligible_velocity {
            state.velocity.x = 0.0;
            state.velocity.z = 0.0;
        }
    } else {
        if state.velocity.x.abs() < negligible_velocity {
            state.velocity.x = 0.0;
        }
        if state.velocity.z.abs() < negligible_velocity {
            state.velocity.z = 0.0;
        }
    }
    if state.velocity.y.abs() < negligible_velocity {
        state.velocity.y = 0.0;
    }
    let fluid = if !affected_by_fluids {
        FluidState::Empty
    } else if state.in_water {
        FluidState::Water
    } else if state.in_lava {
        FluidState::Lava
    } else {
        FluidState::Empty
    };
    let mut fluid_height = if state.in_water {
        water.height
    } else {
        lava.height
    };
    let fluid_jump_threshold = if java_float(eye_height) < 0.4 {
        0.0
    } else {
        0.4
    };
    let head_position = (
        state.position.x.floor() as i32,
        (state.position.y + state.height - 0.1).floor() as i32,
        state.position.z.floor() as i32,
    );
    let body_effect = world.effect_at(body_position);
    let inside_effects = if state.no_physics {
        Vec::new()
    } else {
        world.effects_in_bounds(bounds)
    };
    let in_web = inside_effects.contains(&BlockEffect::Web);
    state.in_web = in_web;
    let was_in_powder_snow = state.in_powder_snow;
    let in_powder_snow = inside_effects.contains(&BlockEffect::PowderSnow);
    if in_powder_snow && !state.powder_snow_immune {
        state.ticks_frozen = state.ticks_frozen.saturating_add(1);
    } else {
        state.ticks_frozen = state.ticks_frozen.saturating_sub(2);
    }

    let required_to_freeze = state.ticks_required_to_freeze.max(140);
    state.ticks_frozen = state.ticks_frozen.min(required_to_freeze);
    if state.tick_count % 40 == 0
        && state.ticks_frozen >= required_to_freeze
        && !state.powder_snow_immune
    {
        state.freeze_damage += 1.0;
    }
    if inside_effects.contains(&BlockEffect::Cactus)
        || inside_effects.contains(&BlockEffect::Campfire)
    {
        state.contact_damage += 1.0;
    }
    if inside_effects.contains(&BlockEffect::SoulCampfire) {
        state.contact_damage += 2.0;
    }
    if inside_effects.contains(&BlockEffect::Fire) {
        state.ticks_frozen = 0;
        state.contact_damage += 1.0;
    }
    if inside_effects.contains(&BlockEffect::SoulFire) {
        state.ticks_frozen = 0;
        state.contact_damage += 2.0;
    }
    let climbing = climbable(world, state);
    let on_scaffolding = body_effect == BlockEffect::Scaffolding;
    let eye_y = state.position.y + state.height - 0.1;
    let eye_fluid_height = world.fluid_height_at(head_position);
    let eye_fluid_top = head_position.1 as f64 + eye_fluid_height;
    let head_fluid = world.block(head_position);
    let swimming_head_position = (
        state.position.x.floor() as i32,
        (state.position.y + 0.9).floor() as i32,
        state.position.z.floor() as i32,
    );
    let swimming_head_in_water = matches!(
        world.block(swimming_head_position).fluid,
        FluidState::Water | FluidState::Waterlogged
    );
    state.eye_in_water = matches!(
        head_fluid.fluid,
        FluidState::Water | FluidState::Waterlogged
    ) && eye_fluid_top > eye_y;
    state.eye_in_lava = head_fluid.fluid == FluidState::Lava && eye_fluid_top > eye_y;
    let vertical_drag = match fluid {
        FluidState::Water => rules.water_drag,
        FluidState::Lava => rules.lava_drag,
        FluidState::Waterlogged => rules.water_drag,
        FluidState::Empty => rules.vertical_drag,
    };
    let jumping = state.jump_requested
        && !(state.is_player && state.flying && rules.release.at_least(1, 16, 0));
    let jump = if jumping {
        fluid_jump(
            state,
            water.height,
            lava.height,
            fluid_jump_threshold,
            rules.release,
        )
    } else {
        FluidJump::None
    };
    if jump == FluidJump::Swim {
        state.velocity.y += 0.03999999910593033;
    } else if jump == FluidJump::Ground && state.jump_ticks == 0 {
        let jump_factor = block_jump_factor(world, body_position, below_position);
        let power = jump_power(state, config, rules.release, jump_factor);
        if !rules.release.at_least(1, 20, 5) || power as f32 > 1.0e-5 {
            state.velocity.y = if rules.release.at_least(1, 21, 2) {
                power.max(state.velocity.y)
            } else {
                power
            };
            if state.sprinting {
                let sin = mth_sin(state.yaw, rules.release);
                let cos = mth_cos(state.yaw, rules.release);
                if rules.release.at_least(1, 20, 5) {
                    state.velocity.x += -sin * 0.2;
                    state.velocity.z += cos * 0.2;
                } else {
                    state.velocity.x += f64::from(-(sin as f32) * 0.2);
                    state.velocity.z += f64::from(cos as f32 * 0.2);
                }
            }
        }
        state.fall_distance = 0.0;
        state.jump_ticks = 10;
    }
    if !jumping {
        state.jump_ticks = 0;
    }
    if state.is_player && !state.passenger && state.swimming && state.in_water {
        let steering = if state.look.y < -0.2 {
            rules.swimming_downward_steering
        } else {
            rules.swimming_vertical_steering
        };
        if state.look.y <= 0.0 || state.jump_requested || swimming_head_in_water {
            state.velocity.y += (state.look.y - state.velocity.y) * steering;
        }
    }
    let flying_vertical_velocity = state.velocity.y;
    let going_down = state.velocity.y <= 0.0;
    let travel_gravity = if state.slow_falling && going_down {
        rules.gravity.min(0.01)
    } else {
        rules.gravity
    };
    let movement_speed = movement_speed_attribute(
        state.movement_speed.unwrap_or(config.movement_speed),
        state.speed_effect_amplifier,
        state.slowness_effect_amplifier,
        state.sprint_modifier.unwrap_or(state.sprinting),
        config.sprint_multiplier,
        rules.release,
    );
    let water_efficiency = {
        let efficiency = if rules.water_movement_efficiency_attribute {
            state.water_movement_efficiency as f32
        } else {
            f32::from(state.depth_strider.min(3))
        };
        if state.on_ground {
            efficiency
        } else {
            efficiency * 0.5
        }
    };
    let toward_water_efficiency = |value: f32, target: f32| {
        if water_efficiency > 0.0 {
            value
                + (target - value) * water_efficiency
                    / rules.water_movement_efficiency_divisor as f32
        } else {
            value
        }
    };
    let water_slowdown = if state.dolphins_grace > 0 {
        0.96
    } else if state.sprinting && !legacy_fluids {
        toward_water_efficiency(rules.water_sprint_drag as f32, 0.54600006)
    } else {
        toward_water_efficiency(rules.water_drag as f32, 0.54600006)
    };
    let fluid_input_speed = match fluid {
        FluidState::Water | FluidState::Waterlogged => f64::from(toward_water_efficiency(
            rules.water_input_speed as f32,
            movement_speed as f32,
        )),
        FluidState::Lava => java_float(rules.lava_input_speed),
        FluidState::Empty => movement_speed,
    };
    let block_friction = {
        let friction = world.slipperiness_at(below_position);
        if rules.fluid_travel_split {
            compute_modified_friction(
                friction,
                effective_modifier(state.friction_modifier, state.friction_modifier_present),
            )
        } else {
            friction
        }
    };
    let air_inertia = if rules.fluid_travel_split {
        compute_modified_friction(
            rules.air_drag,
            effective_modifier(state.air_drag_modifier, state.air_drag_modifier_present),
        )
    } else {
        rules.air_drag
    };
    let ground_inertia = if state.on_ground {
        java_float(java_float(block_friction) * java_float(air_inertia))
    } else {
        java_float(air_inertia)
    };
    let input_speed = if matches!(fluid, FluidState::Empty) {
        if state.on_ground {
            friction_influenced_speed(movement_speed, block_friction, rules.release)
        } else {
            flying_speed(state, rules.release)
        }
    } else {
        fluid_input_speed
    };
    let gliding = state.fall_flying && matches!(fluid, FluidState::Empty);
    if !gliding {
        let input = movement_input(state, rules.release);
        state.velocity =
            state.velocity + relative_movement(input, input_speed, state.yaw, rules.release);
    }
    state.add_delta_movement(state.firework_boost);
    if gliding {
        state.check_fall_distance_accumulation();
        glide(state, travel_gravity, rules.release);
    } else if state.flying {
        state.velocity.y = flying_vertical_velocity;
    }
    if climbing && matches!(fluid, FluidState::Empty) && !gliding {
        let limit = java_float(0.15);
        state.fall_distance = 0.0;
        state.velocity.x = state.velocity.x.clamp(-limit, limit);
        state.velocity.z = state.velocity.z.clamp(-limit, limit);
        let fall_limit = if rules.release.at_least(1, 14, 0) {
            limit
        } else {
            0.15
        };
        state.velocity.y = state.velocity.y.max(-fall_limit);
        let suppresses_sliding_down =
            state.sneaking && !(state.flying && rules.release.at_least(1, 16, 0));
        if state.is_player && suppresses_sliding_down && !on_scaffolding && state.velocity.y < 0.0 {
            state.velocity.y = 0.0;
        }
    }
    let movement = slowed_by_stuck_speed(state, state.velocity);
    let collision_context = entity_collision_context(state);
    let desired = back_off_from_edge(
        state,
        world,
        &collision_context,
        movement,
        rules.step_height,
        rules.release,
    );
    let collision = if state.no_physics {
        WorldCollision {
            movement: desired,
            moved_bounds: None,
        }
    } else {
        collide_with_world(state, world, &collision_context, desired, rules)
    };
    let displacement = collision.movement;
    displace(state, desired, collision, rules.release);
    let mut reset_fall_distance = false;
    if !state.no_physics
        && state.fall_distance != 0.0
        && displacement.length_squared() >= 1.0
        && world.intersects_fall_damage_resetting_block(old_position, displacement)
    {
        state.fall_distance = 0.0;
        reset_fall_distance = true;
    }
    let horizontal_collision = |requested: f64, moved: f64| {
        if rules.release.at_least(1, 14, 0) {
            !vanilla_equal(requested, moved)
        } else {
            requested != moved
        }
    };
    let x_collision = horizontal_collision(desired.x, displacement.x);
    let y_collision = desired.y != displacement.y;
    let z_collision = horizontal_collision(desired.z, displacement.z);
    if !state.no_physics {
        state.collided_horizontally = x_collision || z_collision;
        state.minor_horizontal_collision = state.minor_horizontal_collision_supported
            && state.is_player
            && state.collided_horizontally
            && horizontal_collision_is_minor(state, displacement, rules.release);
        if desired.y != 0.0 || state.controlled_by_local_instance {
            state.collided_vertically = y_collision;
            state.vertical_collision_below = state.collided_vertically && desired.y < 0.0;
            state.block_fall_callback =
                state.vertical_collision_below && state.controlled_by_local_instance;
            state.on_ground = state.collided_vertically && desired.y < 0.0;
            support = main_supporting_block(world, state, Some(displacement));
        }
    } else if rules.release.at_least(1, 21, 9) {
        state.collided_horizontally = false;
        state.minor_horizontal_collision = false;
        state.collided_vertically = false;
        state.vertical_collision_below = false;
    }
    let checks_fall_damage =
        !state.no_physics && !(state.is_player && rules.release == MinecraftVersion::new(1, 21, 4));
    if checks_fall_damage && !state.in_water && legacy_fluids {
        legacy_fluid::handle_water_movement(state, world, affected_by_fluids, rules.release);
    } else if checks_fall_damage && !state.in_water {
        let water = water_contact(world, state.bounds(), rules.release);
        if water.touching {
            state.in_water = true;
            state.fall_distance = 0.0;
            if affected_by_fluids {
                push_by_fluid(state, water, rules.water_flow_scale, rules.release);
            }
        }
        if rules.release.at_least(26, 1, 0) {
            let lava = world.fluid_contact(FluidState::Lava, state.bounds(), rules.release);
            state.in_lava = lava.touching;
            fluid_height = lava.height;
            if affected_by_fluids && lava.touching {
                let lava_flow_scale = if state.ultra_warm {
                    rules.ultra_warm_lava_flow_scale
                } else {
                    rules.lava_flow_scale
                };
                push_by_fluid(state, lava, lava_flow_scale, rules.release);
            }
        }
    }
    let landed_on = world.effect_at(landing_position(state, support, rules.release));
    let bounce_scale = if state.is_player { 1.0 } else { 0.8 };
    let bounced =
        !restitution_enabled && y_collision && landed_on == BlockEffect::Slime && !state.sneaking;
    if bounced {
        if state.velocity.y < 0.0 {
            state.velocity.y = -state.velocity.y * bounce_scale;
        }
        state.fall_distance = 0.0;
        state.fall_damage = 0.0;
    }
    let landed_on_bed = landed_on == BlockEffect::Bed && rules.release.at_least(1, 12, 0);
    let landing_fall_distance = if landed_on_bed {
        state.fall_distance * 0.5
    } else {
        state.fall_distance
    };
    if checks_fall_damage && state.on_ground && !bounced {
        if state.on_ground {
            let cushioned = restitution_enabled && landed_on == BlockEffect::Slime;
            state.fall_damage = if state.fall_damage_immune || cushioned {
                0.0
            } else if modern_fall_damage {
                let fall_power = landing_fall_distance + 1.0e-6 - state.safe_fall_distance;
                if fall_power > 0.0 {
                    let damage = fall_power * state.fall_damage_multiplier.unwrap_or(1.0);
                    match landing_damage_rounding {
                        LandingDamageRounding::Ceil => damage.ceil(),
                        LandingDamageRounding::Floor => damage.floor(),
                    }
                } else {
                    0.0
                }
            } else if landing_fall_distance > 3.0 + state.safe_fall_distance {
                landing_damage(
                    landing_fall_distance,
                    state.safe_fall_distance,
                    state.fall_damage_multiplier.unwrap_or(1.0),
                    state.jump_boost_amplifier,
                    landing_damage_rounding,
                )
            } else {
                0.0
            };
            state.fall_distance = 0.0;
        }
    } else if checks_fall_damage && !bounced && !reset_fall_distance && displacement.y < 0.0 {
        state.fall_distance += -displacement.y;
    }

    // Entity.move (1.17 - 1.19) stops after checkFallDamage for a removed entity
    let removed_in_move = state.removed && rules.release.at_least(1, 17, 0);
    if restitution_enabled && !removed_in_move {
        restitute_movement_after_collisions(
            state,
            world,
            support,
            rules,
            [x_collision, y_collision, z_collision],
            displacement.y,
        );
    } else if !removed_in_move {
        let (stops_x, stops_z) = if rules.release.at_least(1, 18, 2) {
            (x_collision, z_collision)
        } else if rules.release.at_least(1, 14, 0) {
            // Until 1.18.2 the z reset puts back the x motion it read before the x reset.
            let stops_z = desired.z != displacement.z;
            (desired.x != displacement.x && !stops_z, stops_z)
        } else {
            (desired.x != displacement.x, desired.z != displacement.z)
        };
        if stops_x {
            state.velocity.x = 0.0;
        }
        if y_collision && !bounced {
            if landed_on_bed && !state.sneaking {
                if state.velocity.y < 0.0 {
                    state.velocity.y = -state.velocity.y * 0.6600000262260437 * bounce_scale;
                }
            } else {
                // Block.updateEntityAfterFallOn multiplies by zero: a landing leaves -0.0.
                // Before 1.14 Block.onLanded sets 0.0.
                state.velocity.y = if rules.release.at_least(1, 14, 0) {
                    state.velocity.y * 0.0
                } else {
                    0.0
                };
            }
        }
        if stops_z {
            state.velocity.z = 0.0;
        }
    }
    if rules.release.at_least(1, 14, 0) && !rules.release.at_least(1, 16, 0) && !state.no_physics {
        state.in_lava = world.has_fluid_block_inside(FluidState::Lava, state.bounds());
    }
    let jump_factor = block_jump_factor(
        world,
        block_position(state.position),
        block_below_affecting_movement(state, support, rules.release),
    );
    update_auto_jump(
        state,
        world,
        state.position - old_position,
        movement_speed as f32,
        jump_factor,
        rules.release,
    );
    if !rules.release.at_least(1, 21, 2) && !removed_in_move {
        let inside: Vec<_> = blocks_inside(state, rules.release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_block_effects(state, world, support, rules.release, &inside);
    }
    if rules.release.at_least(1, 15, 0) && !state.no_physics && !removed_in_move {
        let speed_factor = f64::from(block_speed_factor(world, state, support, rules.release));
        state.velocity.x *= speed_factor;
        state.velocity.z *= speed_factor;
    }
    let climbs_powder_snow = state.powder_snow_walkable
        && if rules.release.at_least(1, 21, 5) {
            was_in_powder_snow
        } else {
            world.effect_at(block_position(state.position)) == BlockEffect::PowderSnow
        };
    if matches!(fluid, FluidState::Empty)
        && !gliding
        && (state.collided_horizontally
            || (state.jump_requested && rules.release.at_least(1, 14, 0)))
        && (climbable(world, state) || climbs_powder_snow)
    {
        state.velocity.y = 0.2;
    }
    if matches!(fluid, FluidState::Empty) && !state.fall_flying && !state.flying {
        if state.levitation {
            let target = 0.05 * (f64::from(state.levitation_amplifier.unwrap_or(0)) + 1.0);
            state.velocity.y += (target - state.velocity.y) * 0.2;
        } else if !rules.release.at_least(1, 14, 0)
            && !column_loaded(world, state.position, rules.release)
        {
            state.velocity.y = if state.position.y > 0.0 { -0.1 } else { 0.0 };
        } else if !state.no_gravity {
            state.velocity.y -= travel_gravity;
        }
    }
    let horizontal_drag = if gliding { 1.0 } else { ground_inertia };
    if matches!(fluid, FluidState::Water | FluidState::Waterlogged)
        && state.collided_horizontally
        && rules.release.at_least(1, 14, 0)
        && climbable(world, state)
    {
        state.velocity.y = 0.2;
    }
    let liquid_horizontal_drag = if matches!(fluid, FluidState::Water | FluidState::Waterlogged) {
        f64::from(water_slowdown)
    } else if fluid == FluidState::Lava {
        java_float(rules.lava_drag)
    } else {
        horizontal_drag
    };
    state.velocity.x *= liquid_horizontal_drag;
    state.velocity.z *= liquid_horizontal_drag;
    if legacy_fluids && fluid == FluidState::Water {
        legacy_fluid::sink_in_water(state);
    } else {
        apply_fluid_travel_vertical(
            state,
            fluid,
            fluid_height,
            fluid_jump_threshold,
            rules,
            travel_gravity,
            going_down,
        );
    }
    if !matches!(fluid, FluidState::Empty) && state.collided_horizontally {
        let hop_bounds = state.bounds().translated(Vec3::new(
            state.velocity.x,
            state.velocity.y + 0.6000000238418579 - state.position.y + old_position.y,
            state.velocity.z,
        ));
        let free = if legacy_fluids {
            legacy_fluid::is_free(world, hop_bounds, rules.release)
        } else {
            world.is_free(hop_bounds)
        };
        if free {
            state.velocity.y = 0.30000001192092896;
        }
    }
    if matches!(fluid, FluidState::Empty) {
        let air_drag = if rules.fluid_travel_split {
            compute_modified_friction(
                rules.vertical_drag,
                effective_modifier(state.air_drag_modifier, state.air_drag_modifier_present),
            )
        } else {
            vertical_drag
        };
        state.velocity.y *= if gliding { 1.0 } else { java_float(air_drag) };
    }
    if state.flying {
        state.velocity.y = flying_vertical_velocity * 0.6;
        if !flight_resets_fall_before_travel {
            state.fall_distance = 0.0;
            state.fall_flying = false;
        }
    }

    let mut visited_powder_snow = false;
    if rules.release.at_least(1, 21, 2) {
        let movements =
            movements_this_tick(state, old_position, desired, displacement, rules.release);
        let inside = blocks_visited(
            &movements,
            state.bounds(),
            state.half_width,
            state.height,
            rules.release,
        );
        visited_powder_snow = !state.no_physics
            && inside
                .iter()
                .any(|&(position, _)| world.effect_at(position) == BlockEffect::PowderSnow);
        apply_block_effects(state, world, support, rules.release, &inside);
    }
    if state.is_player && state.flying && state.on_ground && !state.no_physics {
        state.flying = false;
    }
    if !rules.release.at_least(1, 14, 0) {
        state.in_lava = legacy_fluid::touching_lava(world, state.bounds(), rules.release);
    }
    state.was_sprinting = state.sprinting;
    state.previous_position = Some(old_position);
    state.in_powder_snow = if rules.release.at_least(1, 21, 2) {
        visited_powder_snow
    } else {
        world
            .effects_in_bounds(state.bounds())
            .contains(&BlockEffect::PowderSnow)
    };

    TickResult {
        displacement,
        collided_horizontally: state.collided_horizontally,
        collided_vertically: state.collided_vertically,
        block_fall_callback: state.block_fall_callback,
    }
}
