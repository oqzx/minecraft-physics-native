use crate::block_traversal::{blocks_visited, blocks_visited_where, Movement, Visit};
use crate::collision::{collide, collide_legacy, vanilla_equal, CollisionRules, StepUp};
use crate::entity::{Dimensions, EntityFlags, EntityState};
use crate::legacy_fluid;
use crate::projectile::{clip_hits, ClipShape};
use crate::math::{
    fast_inv_sqrt, fast_inv_sqrt_f32, java_cos, java_cos_legacy, java_float, java_sin,
    java_sin_legacy, lerp, Aabb, Vec3,
};
use crate::trig;
use crate::version::MinecraftVersion;
use crate::world::WorldSnapshot;
use crate::world::{
    BlockEffect, CollisionQueryEntity, EntityCollisionContext, FluidContact, FluidState,
};

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

/// LivingEntity.computeFallDamage before 1.20.5, in float: `ceil((fallDistance - 3.0F -
/// jumpBoost) * damageMultiplier)`, where `safe_fall_distance` stands for the 3.0F (1.8.9
/// LivingEntity.java:864, 1.20.4 :1557-1564). Nothing below one point.
pub(crate) fn landing_damage(
    fall_distance: f64,
    safe_fall_distance: f64,
    multiplier: f64,
    jump_boost_amplifier: Option<u8>,
    rounding: LandingDamageRounding,
) -> f64 {
    let jump_boost =
        jump_boost_amplifier.map_or(0.0, |amplifier| (i32::from(amplifier) + 1) as f32);
    let damage =
        (fall_distance as f32 - safe_fall_distance as f32 - jump_boost) * multiplier as f32;
    let damage = match rounding {
        LandingDamageRounding::Ceil => damage.ceil(),
        LandingDamageRounding::Floor => damage.floor(),
    };
    f64::from(damage.max(0.0))
}

/// LivingEntity.computeFallDamage from 1.20.5, with the SAFE_FALL_DISTANCE and
/// FALL_DAMAGE_MULTIPLIER attributes (Jump Boost raises the first) and the landing block's
/// damage multiplier (1.0F but where Block.fallOn says otherwise): 1.20.5 - 1.21.4 in float,
/// `ceil((double)((fallDistance - (float)safe) * damageMultiplier) * multiplier)` (1.20.5
/// LivingEntity.java:1602-1609, 1.21.4 :1657-1664); from 1.21.5 in double with an epsilon,
/// `floor((fallDistance + 1.0E-6 - safe) * damageMultiplier * multiplier)` (1.21.5
/// LivingEntity.java:1672-1682). Nothing below one point.
pub(crate) fn attribute_landing_damage(
    fall_distance: f64,
    safe_fall_distance: f64,
    damage_multiplier: f32,
    multiplier: f64,
    rounding: LandingDamageRounding,
    release: MinecraftVersion,
) -> f64 {
    let damage = if release.at_least(1, 21, 5) {
        (fall_distance + 1.0e-6 - safe_fall_distance) * f64::from(damage_multiplier) * multiplier
    } else {
        f64::from((fall_distance as f32 - safe_fall_distance as f32) * damage_multiplier)
            * multiplier
    };
    let damage = match rounding {
        LandingDamageRounding::Ceil => damage.ceil(),
        LandingDamageRounding::Floor => damage.floor(),
    };
    damage.max(0.0)
}

/// Entity.fall's (checkFallDamage's) count of a downward move: in float before 1.21.5,
/// `(float)((double)fallDistance - dy)` up to 1.18.x and `fallDistance -= (float)dy` from 1.19
/// (1.18.1 Entity.java:1019, 1.19 :1005; 1.18.2 has no source and is taken as 1.18.1); a double
/// field from 1.21.5, `fallDistance -= (double)(float)dy` (1.21.5 Entity.java:1286).
pub(crate) fn accumulated_fall_distance(
    fall_distance: f64,
    dy: f64,
    release: MinecraftVersion,
) -> f64 {
    if release.at_least(1, 21, 5) {
        fall_distance - f64::from(dy as f32)
    } else if release.at_least(1, 19, 0) {
        f64::from(fall_distance as f32 - dy as f32)
    } else {
        f64::from((fall_distance - dy) as f32)
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
    /// The float yaw in degrees (Entity.rotationYaw), for the radians a release turns it into
    /// itself: moveFlying before 1.9 (see `yaw_radians`). None uses `yaw`.
    pub yaw_degrees: Option<f32>,
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
    /// Frost Walker on the boots: magma and lit campfires deal no contact damage.
    pub frost_walker: bool,
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
    /// LivingEntity.isSleeping (Player.inBed before 1.14): a sleeper is never in a wall.
    pub sleeping: bool,
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
    /// The tick's move was slowed by the stuck speed multiplier (Entity.isInWeb before 1.14), the
    /// one a riptide launch's lift left included.
    pub move_slowed: bool,
    pub previous_position: Option<Vec3>,
    pub bounding_box: Option<Aabb>,
    /// Entity.mainSupportingBlockPos as the last move left it (1.20+), when a move of this state
    /// set it (`supporting_block_kept`); a state without one takes it from its position and its
    /// last move.
    pub supporting_block: Option<BlockPosition>,
    pub supporting_block_kept: bool,
    /// Entity.onGroundNoBlocks: the last move left the entity on the ground with no block under it.
    pub on_ground_no_blocks: bool,
    /// Entity.isOutsideBorder before 1.14: the last collision found the entity outside the world
    /// border, which then has to be a block further in to count as inside it.
    pub outside_border: bool,
}

impl From<EntityState> for SimulationState {
    fn from(value: EntityState) -> Self {
        Self {
            position: value.position,
            velocity: value.velocity,
            yaw: 0.0,
            yaw_degrees: None,
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
            frost_walker: false,
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
            sleeping: false,
            eye_height: None,
            eye_in_water: false,
            eye_in_lava: false,
            slow_falling: false,
            levitation: false,
            levitation_amplifier: None,
            fall_distance: 0.0,
            safe_fall_distance: 3.0,
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
            move_slowed: false,
            previous_position: None,
            bounding_box: None,
            supporting_block: None,
            supporting_block_kept: false,
            on_ground_no_blocks: false,
            outside_border: false,
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

/// Entity.move's `inLava = false; checkBlockCollision()` 1.14 - 1.16.1: a lava block of the box
/// (above its foot in 1.16 - 1.16.1, FluidBlock.onEntityCollision) sets it (1.14
/// Entity.java:636-638, 1.16 :657-659); from 1.16.2 the base tick's lava height does.
pub(crate) fn update_in_lava_after_move(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    if release.at_least(1, 14, 0) && !release.at_least(1, 16, 2) {
        state.in_lava = if release.at_least(1, 16, 0) {
            world.has_lava_above_feet_inside(state.bounds())
        } else {
            world.has_fluid_block_inside(FluidState::Lava, state.bounds())
        };
    }
}

/// Vec3.normalize: a vector shorter than 1.0E-4 (1.0E-5F from 1.21.2) is zero (1.21.1
/// Vec3.java:63-65, 1.21.2 :71-73).
fn normalize(vector: Vec3, release: MinecraftVersion) -> Vec3 {
    let length = vector_length(vector, release);
    let minimum = if release.at_least(1, 21, 2) {
        f64::from(1.0e-5_f32)
    } else {
        1.0e-4
    };
    if length < minimum {
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

/// The last makeStuckInBlock of the blocks the entity is inside, in the order Entity.checkInsideBlocks
/// visits them: the deflated box before 1.21.2, every block the tick's moves sweep from 1.21.2
/// (cobwebs and bushes have the full inside shape: 1.21.4 Entity.java:1072-1100, 1.21.11
/// BlockBehaviour.java:316-318; powder snow its collision shape for the entity from 1.21.5, which
/// blocks_inside_moves tests).
fn stuck_speed_multiplier_inside(
    state: &SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
) -> Option<Vec3> {
    let in_powder_snow = world.effect_at(block_position(state.position)) == BlockEffect::PowderSnow;
    inside
        .iter()
        .filter_map(|&(position, _)| match world.effect_at(position) {
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
    let feet = block_position(state.position);
    let at_position = factor_at(feet);
    // the water block and the bubble column keep their own factor: the block below is not read
    // (1.15 Entity.java:676-684, 1.21.11 :996-1004, 26.3 :1131-1139)
    let feet_in_water = world.block(feet).water_block
        || matches!(
            world.effect_at(feet),
            BlockEffect::BubbleUp | BlockEffect::BubbleDown
        );
    let factor = if at_position == 1.0 && !feet_in_water {
        factor_at(below)
    } else {
        at_position
    };
    if release.at_least(1, 21, 0) {
        factor + state.movement_efficiency as f32 * (1.0 - factor)
    } else if release.at_least(1, 16, 0)
        && state.soul_speed > 0
        && matches!(
            world.effect_at(soul_speed_position(state, below, release)),
            BlockEffect::SoulSand | BlockEffect::SoulSoil
        )
    {
        1.0
    } else {
        factor
    }
}

/// The block LivingEntity.onSoulSpeedBlock reads: in 1.16 and 1.16.1 Entity.getBlockStateOn,
/// the block at floor(y - 0.2F) (1.16 LivingEntity.java:653-655, Entity.java:683-698; its fence
/// fallback finds a fence, wall or gate, never a soul speed block), from 1.16.2 the block below
/// that affects the movement (1.16.5 LivingEntity.java:664-666).
fn soul_speed_position(
    state: &SimulationState,
    below: BlockPosition,
    release: MinecraftVersion,
) -> BlockPosition {
    if release.at_least(1, 16, 2) {
        below
    } else {
        block_position(state.position - Vec3::new(0.0, f64::from(0.2_f32), 0.0))
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

/// Entity.mainSupportingBlockPos (1.20+, checkSupportingBlock) of an entity that keeps none: the
/// block under an entity on the ground that supports it, else, after a move that leaves the box
/// over nothing, the one under the box where the move started.
pub(crate) fn main_supporting_block(
    world: &WorldSnapshot,
    state: &SimulationState,
    movement: Option<Vec3>,
) -> Option<BlockPosition> {
    if !state.on_ground {
        return None;
    }
    let area = supporting_block_area(state);
    let context = entity_collision_context(state);
    world
        .supporting_block_position(area, state.position, &context)
        .or_else(|| {
            let movement = movement?;
            let start = area.translated(Vec3::new(-movement.x, 0.0, -movement.z));
            world.supporting_block_position(start, state.position, &context)
        })
}

/// The test area of Entity.checkSupportingBlock: the 1e-6 under the box.
fn supporting_block_area(state: &SimulationState) -> Aabb {
    let bounds = state.bounds();
    Aabb::new(
        Vec3::new(bounds.min.x, bounds.min.y - 1.0e-6, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
    )
}

/// Entity.mainSupportingBlockPos before a move: as the last move left it, else (a state no move
/// kept it in) from the position and the last move.
pub(crate) fn kept_supporting_block(
    world: &WorldSnapshot,
    state: &SimulationState,
    movement: Option<Vec3>,
) -> Option<BlockPosition> {
    if state.supporting_block_kept {
        state.supporting_block
    } else {
        main_supporting_block(world, state, movement)
    }
}

/// Entity.checkSupportingBlock (1.20+) after a move: the block found under the box; with none,
/// unless the last check found none either (onGroundNoBlocks), the one under the box where the
/// move started; with no move to retry it, the block kept before. Off the ground, none.
pub(crate) fn check_supporting_block(
    world: &WorldSnapshot,
    state: &mut SimulationState,
    movement: Option<Vec3>,
) -> Option<BlockPosition> {
    if state.on_ground {
        let area = supporting_block_area(state);
        let context = entity_collision_context(state);
        let mut found = world.supporting_block_position(area, state.position, &context);
        if found.is_some() || state.on_ground_no_blocks {
            state.supporting_block = found;
        } else if let Some(movement) = movement {
            let start = area.translated(Vec3::new(-movement.x, 0.0, -movement.z));
            found = world.supporting_block_position(start, state.position, &context);
            state.supporting_block = found;
        } else if !state.supporting_block_kept {
            state.supporting_block = None;
        }
        state.on_ground_no_blocks = found.is_none();
    } else {
        state.on_ground_no_blocks = false;
        state.supporting_block = None;
    }
    state.supporting_block_kept = true;
    state.supporting_block
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
            // MagmaBlock.stepOn: no fall-flying test; Frost Walker spares the boots' wearer, in
            // the block before 1.21 (1.20.5 MagmaBlock.java:29) and by the enchantment's
            // damage immunity to #burn_from_stepping from 1.21 (1.21.11 Enchantments.java:389-394)
            BlockEffect::Magma if !stepping_carefully && !state.frost_walker => {
                state.contact_damage += 1.0;
            }
            _ => {}
        }
    }
    if !(state.is_player && state.flying) {
        if let Some(multiplier) = stuck_speed_multiplier_inside(state, world, inside) {
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

/// The signals of the blocks the move leaves the entity inside, the list apply_block_effects
/// takes (Entity.checkInsideBlocks on the box after the move, deflated by 0.001, 1e-7 from 1.19.3,
/// the swept blocks from 1.21.2: 1.19.2 Entity.java:891-894, 1.19.3 :899-902, 1.21.4 :1072-1100),
/// and LivingEntity.aiStep's freezing after it (1.17 LivingEntity.java:2543-2559, 1.21.11
/// :2985-2995): Entity.isInPowderSnow, the cobweb, the contact damage of cactus, fire and lit
/// campfires (none under Frost Walker: 1.20.5 CampfireBlock.java:104, the enchantment's damage
/// immunity from 1.21) and the freeze ticks, counted up in powder snow and down by two outside it.
/// From 1.21.5 the freezing is the steps' InsideBlockEffectType.FREEZE, one tick a step (1.21.5
/// InsideBlockEffectType.java:7-12, InsideBlockEffectApplier.java advanceStep), and fire and lava
/// clear the ticks: CLEAR_FREEZE from 1.21.9 (1.21.11 BaseFireBlock.java:132, LavaFluid.java:117),
/// igniteForTicks from 1.21.5 (1.21.5 Entity.java:565-571), then aiStep takes two off outside
/// powder snow (1.21.5 LivingEntity.java:2778-2779). Before 1.21.5 the burning Entity.baseTick
/// clears them (1.21.4 Entity.java:460-479); the engine keeps no fire ticks, so this takes the
/// contact with fire or lava for it.
fn apply_inside_block_signals(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    freeze_steps: &[FreezeStep],
    release: MinecraftVersion,
) {
    let effects: Vec<BlockEffect> = inside
        .iter()
        .map(|&(position, _)| world.effect_at(position))
        .collect();
    state.in_web = effects.contains(&BlockEffect::Web);
    state.in_powder_snow = effects.contains(&BlockEffect::PowderSnow);
    if effects.contains(&BlockEffect::Cactus) {
        state.contact_damage += 1.0;
    }
    if effects.contains(&BlockEffect::Campfire) && !state.frost_walker {
        state.contact_damage += 1.0;
    }
    if effects.contains(&BlockEffect::SoulCampfire) && !state.frost_walker {
        state.contact_damage += 2.0;
    }
    if effects.contains(&BlockEffect::Fire) {
        state.contact_damage += 1.0;
    }
    if effects.contains(&BlockEffect::SoulFire) {
        state.contact_damage += 2.0;
    }
    let required_to_freeze = state.ticks_required_to_freeze.max(140);
    if release.at_least(1, 21, 5) {
        for step in freeze_steps {
            if step.freeze && !state.powder_snow_immune {
                state.ticks_frozen = state.ticks_frozen.saturating_add(1).min(required_to_freeze);
            }
            if step.clear {
                state.ticks_frozen = 0;
            }
        }
        if !state.in_powder_snow || state.powder_snow_immune {
            state.ticks_frozen = state.ticks_frozen.saturating_sub(2);
        }
    } else {
        if state.in_powder_snow && !state.powder_snow_immune {
            state.ticks_frozen = state.ticks_frozen.saturating_add(1).min(required_to_freeze);
        } else {
            state.ticks_frozen = state.ticks_frozen.saturating_sub(2);
        }
        if effects.contains(&BlockEffect::Fire)
            || effects.contains(&BlockEffect::SoulFire)
            || state.in_lava
        {
            state.ticks_frozen = 0;
        }
    }
    state.ticks_frozen = state.ticks_frozen.min(required_to_freeze);
    if state.tick_count % 40 == 0
        && state.ticks_frozen >= required_to_freeze
        && !state.powder_snow_immune
    {
        state.freeze_damage += 1.0;
    }
}

/// One InsideBlockEffectApplier.StepBasedCollector step's freezing effects: FREEZE, then
/// CLEAR_FREEZE / LAVA_IGNITE / FIRE_IGNITE, in InsideBlockEffectType's order (1.21.5
/// InsideBlockEffectType.java, 1.21.11 :6-13).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FreezeStep {
    pub freeze: bool,
    pub clear: bool,
}

/// The blocks Entity.checkInsideBlocks gives entityInside from 1.21.2 and, from 1.21.5, the
/// freezing effects of each step. From 1.21.5 a block passes when its getEntityInsideCollisionShape
/// is Shapes.block() or the move collides along with it, and its fluid's when the move collides
/// along with its AABB (1.21.5 Entity.java:1096-1124, 1.21.11 :1222-1255): powder snow's inside
/// shape is its collision shape for the entity, FALLING_COLLISION_SHAPE (0.9F high) past a fall
/// of 2.5 (1.21.5 - 1.21.11 PowderSnowBlock.java:108-123, 26.3 :89-104), and a makeStuckInBlock
/// on the way ends the fall (1.21.11 Entity.java:2824-2827).
pub(crate) fn blocks_inside_moves(
    state: &SimulationState,
    world: &WorldSnapshot,
    movements: &[Movement],
    release: MinecraftVersion,
) -> (Vec<((i32, i32, i32), bool)>, Vec<FreezeStep>) {
    if !release.at_least(1, 21, 5) {
        let inside = blocks_visited(
            movements,
            state.bounds(),
            state.half_width,
            state.height,
            release,
        );
        return (inside, Vec::new());
    }
    let box_at = |position: Vec3| {
        Aabb::new(
            Vec3::new(
                position.x - state.half_width,
                position.y,
                position.z - state.half_width,
            ),
            Vec3::new(
                position.x + state.half_width,
                position.y + state.height,
                position.z + state.half_width,
            ),
        )
    };
    let feet_in_powder_snow =
        world.effect_at(block_position(state.position)) == BlockEffect::PowderSnow;
    let sticks = !(state.is_player && state.flying);
    let mut fall_distance = state.fall_distance;
    let mut steps: Vec<FreezeStep> = Vec::new();
    let mut last_step = None;
    let inside = blocks_visited_where(
        movements,
        state.bounds(),
        state.half_width,
        state.height,
        release,
        |visit: &Visit| {
            let (x, y, z) = visit.position;
            if world.is_air_at(visit.position) {
                return false;
            }
            let corner = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
            let moving = box_at(visit.from);
            let travel = visit.to - visit.from;
            let effect = world.effect_at(visit.position);
            let shape_hit = effect != BlockEffect::PowderSnow
                || fall_distance <= 2.5
                || moving.collided_along_vector(
                    travel,
                    &[Aabb::new(
                        corner,
                        corner + Vec3::new(1.0, java_float(0.9), 1.0),
                    )],
                );
            // a block whose inside shape the move misses is powder snow, which holds no fluid
            if !shape_hit {
                return false;
            }
            let lava_hit = world.block(visit.position).fluid == FluidState::Lava
                && moving.collided_along_vector(
                    travel,
                    &[Aabb::new(
                        corner,
                        corner + Vec3::new(1.0, world.fluid_height_at(visit.position), 1.0),
                    )],
                );
            if last_step != Some(visit.step) {
                last_step = Some(visit.step);
                steps.push(FreezeStep::default());
            }
            let step = steps.last_mut().expect("a step was just pushed");
            match effect {
                BlockEffect::PowderSnow => {
                    step.freeze = true;
                    if feet_in_powder_snow && sticks {
                        fall_distance = 0.0;
                    }
                }
                BlockEffect::Web if sticks => fall_distance = 0.0,
                BlockEffect::SweetBerryBush if sticks && !state.sweet_berry_bush_immune => {
                    fall_distance = 0.0;
                }
                // BaseFireBlock's CLEAR_FREEZE from 1.21.9 (1.21.11 BaseFireBlock.java:132);
                // before it fireIgnite clears only once the fire ticks reach 0 (1.21.5
                // BaseFireBlock.java:136-147), which the engine does not track: it clears at once
                BlockEffect::Fire | BlockEffect::SoulFire => {
                    step.clear = true;
                }
                _ => {}
            }
            // LavaFluid.entityInside: LAVA_IGNITE (igniteForTicks clears the freeze, 1.21.5
            // Entity.java:565-571) and CLEAR_FREEZE from 1.21.9 (1.21.11 LavaFluid.java:116-119)
            if lava_hit {
                step.clear = true;
            }
            true
        },
    );
    (inside, steps)
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
/// (Level.getEntityCollisions), since 26.3 first stretched up by the step height
/// (Entity.collide: `expandTowards(movement).expandTowards(0, maxUpStep(), 0)`); before, the
/// blocks' query (World.getCollisionBoxes).
fn entity_collision_query(
    swept: Aabb,
    query: Aabb,
    max_step: f32,
    release: MinecraftVersion,
) -> Aabb {
    if release.at_least(26, 3, 0) {
        swept
            .expand_towards(Vec3::new(0.0, f64::from(max_step), 0.0))
            .inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    } else if release.at_least(1, 14, 0) {
        swept.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    } else {
        query
    }
}

/// LivingEntity.onClimbable; from 1.21.4 Player.onClimbable is false while flying (1.21.4
/// PlayerEntity.java:2219-2221, 26.3 Player.java:1876-1878; 1.21.3 has no override). From
/// 1.21.11 a fall-flying entity is not on a #can_glide_through block (1.21.11
/// LivingEntity.java:1675-1676, 26.3 :1671).
pub(crate) fn climbable(
    world: &WorldSnapshot,
    state: &SimulationState,
    release: MinecraftVersion,
) -> bool {
    !state.no_physics
        && !(state.is_player && state.flying && release.at_least(1, 21, 4))
        && (state.climbing
            || match world.effect_at(block_position(state.position)) {
                BlockEffect::Climbable | BlockEffect::Scaffolding => true,
                BlockEffect::GlideThroughClimbable => {
                    !(state.fall_flying && release.at_least(1, 21, 11))
                }
                _ => false,
            })
}

/// Before 1.17 an entity keeps its bounding box between ticks: Entity.move moves the box and takes
/// the position from it. A pose of another size refits it as Entity.refreshDimensions does.
pub(crate) fn kept_bounding_box(state: &SimulationState) -> Aabb {
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
    // LocalPlayer.aiStep undoes the sneak slowdown of a flying player, 1.9 - 1.14 (1.9.4
    // ClientPlayerEntity.java:712-715, 1.14 :717-721); 1.8 only moves it down (1.8.9
    // ClientPlayerEntity.java:596-603), and from 1.15 a flying player does not crouch
    if state.flying && state.sneaking && release.at_least(1, 9, 0) && !release.at_least(1, 15, 0) {
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

/// A float yaw in degrees as radians the way moveFlying turns it: `yaw * (float) Math.PI /
/// 180.0F` (two float roundings) before 1.9 (1.8.9 Entity.java:942-943, HorseBaseEntity.java:1091),
/// `yaw * (float) (Math.PI / 180.0)` from 1.9 (1.9.4 Entity.java:1050-1051).
pub(crate) fn yaw_radians(degrees: f32, release: MinecraftVersion) -> f64 {
    if release.at_least(1, 9, 0) {
        f64::from(degrees * 0.017453292)
    } else {
        f64::from(degrees * 3.1415927 / 180.0)
    }
}

/// The radians the player's moveFlying turns its yaw into: `yaw_radians` of the float degrees
/// when the state has them, else the radians it holds.
fn move_relative_yaw(state: &SimulationState, release: MinecraftVersion) -> f64 {
    state
        .yaw_degrees
        .map_or(state.yaw, |degrees| yaw_radians(degrees, release))
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

/// Player.maybeBackOffFromEdge from 1.14 (Entity.move's sneak edge check before): a sneaking
/// player's move backs off while it would leave the ground below by the step height (the float
/// maxUpStep).
/// - 1.14 - 1.16.1: only on the ground, the whole box moved by the step down meets no collision
///   (client1.14.4 Entity.java:600-640, 1.16 PlayerEntity.java:1035-1081).
/// - 1.16.2 - 1.18.2: also not flying, and above the ground: on it, or the whole box moved down by
///   what is left of the step after the fall distance (both floats) meets a collision (1.16.5
///   PlayerEntity.java:1116-1120, 1.18 :1068-1072).
/// - 1.19 - 1.20.4: also not while the move rises (1.19 PlayerEntity.java:1059-1064).
/// - 1.20.5 - 1.21.4: the boxes are the feet's slab from minY less the step and the float 1.0E-5F
///   (1.20.5 PlayerEntity.java:1121-1128, 1.21.4 :1124-1132); the fall distance a float yet.
/// - 1.21.5 on: the slab is 1e-7 shorter than 1e-5 below and 1e-7 in all round, the fall distance
///   a double (1.21.5 Player.java:1077-1085, client26.3 :876-883).
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
    let step = max_down_step as f32;
    let checks_above_ground = release.at_least(1, 16, 2);
    if state.no_physics
        || !state.is_player
        || !state.sneaking
        || (checks_above_ground && state.flying)
        || (release.at_least(1, 19, 0) && delta.y > 0.0)
        || (!checks_above_ground && !state.on_ground)
    {
        return delta;
    }
    let bounds = state.bounds();
    let entity = collision_query_entity(state, context);
    let feet_slab = release.at_least(1, 20, 5);
    // the box moved by (x, -fall, z): the whole box before 1.20.5, else the feet's slab
    let can_fall = |x: f64, z: f64, fall: f64| {
        let lowered = if !feet_slab {
            bounds.translated(Vec3::new(x, -fall, z))
        } else if release.at_least(1, 21, 5) {
            Aabb::new(
                Vec3::new(
                    bounds.min.x + 1.0e-7 + x,
                    bounds.min.y - fall - 1.0e-7,
                    bounds.min.z + 1.0e-7 + z,
                ),
                Vec3::new(
                    bounds.max.x - 1.0e-7 + x,
                    bounds.min.y,
                    bounds.max.z - 1.0e-7 + z,
                ),
            )
        } else {
            Aabb::new(
                Vec3::new(
                    bounds.min.x + x,
                    bounds.min.y - fall - f64::from(1.0e-5_f32),
                    bounds.min.z + z,
                ),
                Vec3::new(bounds.max.x + x, bounds.min.y, bounds.max.z + z),
            )
        };
        world.no_collision_for_entity(lowered, entity, release)
    };
    if checks_above_ground {
        let (falls_short, rest_of_step) = if release.at_least(1, 21, 5) {
            let fall_distance = state.fall_distance;
            (
                fall_distance < f64::from(step),
                f64::from(step) - fall_distance,
            )
        } else {
            let fall_distance = state.fall_distance as f32;
            (fall_distance < step, f64::from(step - fall_distance))
        };
        let above_ground = state.on_ground || (falls_short && !can_fall(0.0, 0.0, rest_of_step));
        if !above_ground {
            return delta;
        }
    }
    back_off_while(delta, |x, z| can_fall(x, z, f64::from(step)))
}

/// The entity a no-collision query of the entity's own is made for.
pub(crate) fn collision_query_entity<'a>(
    state: &SimulationState,
    context: &'a EntityCollisionContext,
) -> CollisionQueryEntity<'a> {
    CollisionQueryEntity {
        context,
        bounds: state.bounds(),
        position: state.position,
        outside_border: state.outside_border,
    }
}

/// Entity.move's sneak edge check before 1.14: whole box moved down by 1, from 1.11 by the step
/// height, must meet no collision box for a grounded sneaking player's move to back off.
/// World.getCollisionBoxes(entity, box) gives the blocks' boxes, the cells outside the border as
/// stone and the boxes of the entities that meet the query (1.8.9 Entity.java:487, 1.12.2
/// Entity.java:616 and World.java:1180-1203, 1.13.2 Entity.java:627 through World.java:1805-1808).
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
    let entity = collision_query_entity(state, context);
    let can_fall = |x: f64, z: f64| {
        let mut lowered = bounds.translated(Vec3::new(x, -drop, z));
        if release.at_least(1, 13, 0) {
            lowered = lowered.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        }
        // (World.getCollisionBoxes before 1.13: a box of no height, one layer of snow, too)
        world.no_collision_for_entity(lowered, entity, release)
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

/// Entity.isInWall, which LivingEntity (Player before 1.14) answers false for while sleeping.
/// Before 1.16 a suffocating block in a cell under any of 8 points 0.1 tall and 0.8 of the width
/// around the eye (Entity.java:1291-1311 in 1.8.9, :1572-1592 in 1.15); in 1.16 a suffocating
/// collision shape meeting a box 0.1 tall around the eye (:1640-1649 in 1.16.5), 1e-6 tall from
/// 1.17 (:1662-1670), or the world border the entity's box touches from inside; 1.18 tests each
/// block's shape moved to the eye instead of to its cell (client1.18.1 Entity.java:1698-1710);
/// from 1.19 each non-air suffocating block's shape at its
/// cell (:1676-1694 in 1.19, client1.21.11 :2192-2203).
fn is_in_wall(state: &SimulationState, world: &WorldSnapshot, release: MinecraftVersion) -> bool {
    if state.no_physics || state.sleeping {
        return false;
    }
    let eye_height = state.eye_height.unwrap_or(state.height * 0.85) as f32;
    let width = (state.half_width * 2.0) as f32;
    if !release.at_least(1, 16, 0) {
        return (0..8).any(|corner: i32| {
            let offset = |bit: i32| ((corner >> bit) % 2) as f32 - 0.5;
            let y = (state.position.y + f64::from(offset(0) * 0.1) + f64::from(eye_height)).floor();
            let x = (state.position.x + f64::from(offset(1) * width * 0.8)).floor();
            let z = (state.position.z + f64::from(offset(2) * width * 0.8)).floor();
            world.is_in_wall_block((x as i32, y as i32, z as i32))
        });
    }
    let half_width = f64::from(width * 0.8) / 2.0;
    let half_height = if release.at_least(1, 17, 0) {
        1.0e-6 / 2.0
    } else {
        f64::from(0.1_f32) / 2.0
    };
    let eye = Vec3::new(
        state.position.x,
        state.position.y + f64::from(eye_height),
        state.position.z,
    );
    let eye_box = Aabb::new(
        eye - Vec3::new(half_width, half_height, half_width),
        eye + Vec3::new(half_width, half_height, half_width),
    );
    if release.at_least(1, 18, 0) && !release.at_least(1, 19, 0) {
        return world.collides_with_suffocating_block_moved_to(eye_box, eye);
    }
    // (1.16 - 1.17.1: getBlockCollisions with the entity offers the world border's shape first,
    // which the suffocation predicate does not filter)
    if !release.at_least(1, 18, 0) && world.entity_touches_world_border(state.bounds()) {
        return true;
    }
    world.collides_with_suffocating_block(eye_box)
}

/// LocalPlayer.moveTowardsClosestSpace (checkInBlock before 1.16.2): from each corner of the box
/// shrunk to 0.35 of its width, a player inside a suffocating block moves at 0.1 toward the
/// nearest side of that block that is open. Until 1.16.1 a cell of the column is suffocating
/// whatever its collision shape (1.16 ClientPlayerEntity.java:406-464, blocked() in the 1.16.1
/// bytecode); from 1.16.2 a suffocating collision shape meets the column (suffocatesAt, 1.16.5
/// :406-439).
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
        if release.at_least(1, 16, 2) {
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
        let mut closest = if release.at_least(1, 16, 2) {
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
            // Entity.getBlockBounciness: the whole restitution for a LivingEntity, which every
            // entity this travel moves is (26.3 Entity.java:905-911)
            entity_restitution
                .max(effect_state.bounce_restitution)
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
    /// Entity.isOutsideBorder as the collision left it.
    outside_border: bool,
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
    let (mut obstacles, entered) = if rules.release.at_least(1, 13, 0) {
        world.collision_shapes_with_entered_for_entity(query, bounds, context)
    } else {
        (
            world.legacy_collision_shapes_for_entity(query, context),
            Vec::new(),
        )
    };
    let entities = world.entity_collisions(
        entity_collision_query(swept, query, max_step, rules.release),
        context,
    );
    if !rules.release.at_least(1, 14, 0) {
        let inside = world.is_inside_legacy_world_border(state.position, state.outside_border);
        let outside_border = if updates_outside_border(world, swept, rules.release) {
            !inside
        } else {
            state.outside_border
        };
        if inside {
            let outside_cell = |bounds: &Aabb| {
                world.is_outside_world_border_cell(bounds.min.x.floor(), bounds.min.z.floor())
            };
            obstacles.retain(|obstacle| !outside_cell(obstacle));
            obstacles.extend(world.legacy_world_border_cells(query));
        }
        let voxel_shapes = rules.release.at_least(1, 13, 0);
        let legacy = collide_legacy(
            bounds,
            desired,
            &obstacles,
            &entities,
            state.on_ground,
            max_step,
            voxel_shapes,
        );
        return WorldCollision {
            movement: legacy.movement,
            moved_bounds: Some(legacy.bounds),
            outside_border,
        };
    }
    let border = world.world_border_collision_boxes();
    let collides_with_border = if rules.release.at_least(1, 18, 0) {
        world.is_inside_close_to_world_border(
            state.position,
            bounds.expand_towards(desired),
            rules.release,
        )
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
    let movement = collide(
        bounds,
        desired,
        &obstacles,
        &entities,
        &entered,
        state.on_ground,
        max_step,
        collision,
    );
    WorldCollision {
        movement,
        moved_bounds: None,
        outside_border: state.outside_border,
    }
}

/// Whether World.getCollisionBoxes sets Entity.isOutsideBorder for a move sweeping `swept`: from
/// 1.13 always (IWorldReaderBase.getCollisionBoxes), before only once its walk over the columns
/// reaches one in a loaded chunk, the columns of the box before 1.9 and, from 1.9, those around it
/// too but the corners (World.getCollisionBoxes).
fn updates_outside_border(world: &WorldSnapshot, swept: Aabb, release: MinecraftVersion) -> bool {
    if release.at_least(1, 13, 0) {
        return true;
    }
    let (columns_x, columns_z) = if release.at_least(1, 9, 0) {
        (
            swept.min.x.floor() as i32 - 1..swept.max.x.ceil() as i32 + 1,
            swept.min.z.floor() as i32 - 1..swept.max.z.ceil() as i32 + 1,
        )
    } else {
        (
            swept.min.x.floor() as i32..(swept.max.x + 1.0).floor() as i32,
            swept.min.z.floor() as i32..(swept.max.z + 1.0).floor() as i32,
        )
    };
    let corners = release.at_least(1, 9, 0);
    columns_x.clone().any(|x| {
        columns_z.clone().any(|z| {
            let edge_x = x == columns_x.start || x == columns_x.end - 1;
            let edge_z = z == columns_z.start || z == columns_z.end - 1;
            !(corners && edge_x && edge_z) && world.is_chunk_loaded(x >> 4, z >> 4)
        })
    })
}

/// Moves the entity by the collided movement, and before 1.17 the bounding box it keeps, which the
/// position follows: before 1.14 to where the collision moved the box.
pub(crate) fn displace(
    state: &mut SimulationState,
    desired: Vec3,
    collision: WorldCollision,
    release: MinecraftVersion,
) {
    state.outside_border = collision.outside_border;
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
    let support = check_supporting_block(world, state, Some(displacement));
    if state.collided_vertically {
        state.velocity.y = if release.at_least(1, 14, 0) {
            state.velocity.y * 0.0
        } else {
            0.0
        };
    }
    // the lift is an Entity.move, whose checkInsideBlocks pushes, slows and sticks as the tick's
    // own (1.13.2 Entity.java:841, 1.20.4 :713, 1.21 :713); from 1.21.2 the move only queues its
    // path, which the tick's check leaves out: it runs from where the tick started (1.21.3
    // Entity.java:752-767, 1057, LivingEntity.java:2785)
    if !release.at_least(1, 21, 2) {
        let inside: Vec<_> = blocks_inside(state, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_block_effects(state, world, support, release, &inside);
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
pub(crate) fn update_auto_jump(
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
    // 1.15 starts the probe at position() (1.15 ClientPlayerEntity.java:860), 1.14 at the
    // bounding box minY (1.14 ClientPlayerEntity.java:851).
    let feet_y = if release.at_least(1, 15, 0) {
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
    // Level.getCollisions: the boxes of the entities the player collides with (boats, shulkers)
    // after the block shapes before 1.18, before them from 1.18 (1.12.2 World.getCollisionBoxes,
    // 1.16.5 CollisionView.java:58-59, 1.18 CollisionView.java:67-70, 1.21.11
    // CollisionGetter.java:85-89); from 1.14 the entities meeting the area grown by 1.0E-7
    // (1.14.4 EntityGetter.java:54-58, 1.21.11 EntityGetter.java:54-60)
    let entity_area = if release.at_least(1, 14, 0) {
        area.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    } else {
        area
    };
    let entities = world.entity_collisions(entity_area, &context);
    // 1.13 - 1.14.0 defer the full cubes into one merged shape after the partial ones (1.13.2
    // RenderBlockView.java:113-153, 1.14 ViewableWorld.java:145-195); 1.14.4 and 1.15 walk every
    // block in order (1.14.4 LevelReader.java:130-145, 1.15 CollisionView.java:67-106)
    let blocks = if release.at_least(1, 13, 0) && !release.at_least(1, 14, 1) {
        world.block_collision_boxes_merging_full_cubes(area, &context)
    } else {
        world.block_collision_boxes_in_order(area, &context)
    };
    let obstacles: Vec<Aabb> = if release.at_least(1, 18, 0) {
        entities.into_iter().chain(blocks).collect()
    } else {
        blocks.into_iter().chain(entities).collect()
    };
    let unset = f32::from_bits(1);
    let mut step_top = unset;
    for obstacle in obstacles {
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
    // Entity.setOnGround(false) empties mainSupportingBlockPos and onGroundNoBlocks (1.21.11
    // Entity.java:680-684), and nothing else takes an entity off the ground: a state handed in off
    // it (a player Player.tick kept off it while it rode, :238-241, or a new one after a respawn)
    // keeps no block from before.
    if !state.on_ground {
        state.supporting_block = None;
        state.on_ground_no_blocks = false;
    }
    if state.is_player && state.no_physics {
        state.on_ground = false;
        check_supporting_block(world, state, None);
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
    // LivingEntity.aiStep (1.20+) resets the fall distance under Slow Falling or Levitation before
    // travel (1.20 LivingEntity.java:2566-2567); before, travel resets it itself (below).
    let effects_reset_fall_before_travel = rules.release.at_least(1, 20, 0);
    if ((state.slow_falling || state.levitation) && effects_reset_fall_before_travel)
        || (state.flying && flight_resets_fall_before_travel)
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
    let mut support = kept_supporting_block(world, state, previous_movement);
    let below_position = block_below_affecting_movement(state, support, rules.release);
    let bounds = state.bounds();
    let eye_height = state.eye_height.unwrap_or(state.height * 0.85);
    state.inside_wall = is_in_wall(state, world, rules.release);
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
    // 1.16 - 1.16.1 Entity.updateWaterState returns in water before it measures the lava (its
    // height stays 0), and isInLava is the flag of the last move (1.16 Entity.java:1042-1051,
    // 1164-1166); 1.16.2 always measures it (1.16.5 Entity.java:1046-1052, 1181-1183)
    let measures_lava =
        rules.release.at_least(1, 16, 2) || !rules.release.at_least(1, 16, 0) || !state.in_water;
    let lava = if measures_lava {
        world.fluid_contact(FluidState::Lava, bounds, rules.release)
    } else {
        FluidContact::default()
    };
    if rules.release.at_least(1, 16, 0) {
        if affected_by_fluids && measures_lava {
            let lava_flow_scale = if state.ultra_warm {
                rules.ultra_warm_lava_flow_scale
            } else {
                rules.lava_flow_scale
            };
            push_by_fluid(state, lava, lava_flow_scale, rules.release);
        }
        if rules.release.at_least(1, 16, 2) {
            state.in_lava = lava.height > 0.0;
        }
    } else if !rules.release.at_least(1, 14, 0) {
        state.in_lava = legacy_fluid::touching_lava(world, bounds, rules.release);
    }
    // Entity.baseTick halves the fall distance every tick in lava, on the client too (1.8.9
    // Entity.java:410, 1.14 :481, 1.21.4 :487, 1.21.5 :490 `*= 0.5`; 1.14 - 1.15 read the inLava
    // of the last move). Halving is exact in float and double alike.
    if state.in_lava {
        state.fall_distance *= 0.5;
    }
    if state.is_player && !state.no_physics {
        move_towards_closest_space(state, world, rules.release);
    }
    let sinks_in_water =
        !legacy_fluids && (affected_by_fluids || !rules.release.at_least(1, 16, 0));
    if state.is_player && state.in_water && state.sneaking && sinks_in_water {
        state.velocity.y -= 0.03999999910593033;
    }
    if state.flying && !rules.release.at_least(1, 14, 0) {
        // Before 1.14 sneak and jump are two separate double steps, so holding both is not
        // a no-op in double (1.8.9 ClientPlayerEntity.java:596-603, 1.13.2 :797-805).
        let impulse = f64::from(state.flying_speed as f32 * 3.0);
        if state.sneaking {
            state.velocity.y -= impulse;
        }
        if state.jump_requested {
            state.velocity.y += impulse;
        }
    } else if state.flying {
        // 1.14 nets the input into one impulse (1.14 ClientPlayerEntity.java:719-731).
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
    let body_effect = world.effect_at(body_position);
    let was_in_powder_snow = state.in_powder_snow;
    let climbing = climbable(world, state, rules.release);
    let on_scaffolding = body_effect == BlockEffect::Scaffolding;
    let eyes = (
        state.position.x,
        state.position.y + java_float(eye_height),
        state.position.z,
    );
    // Player.travel steers a swimmer while any fluid, lava too, is at (y + 1.0) - 0.1 (1.13.2
    // PlayerEntity.java:1489, 1.21.4 :1503, 1.21.11 Player.java:1364, 26.3 :1301)
    let swimming_head_position = (
        state.position.x.floor() as i32,
        (state.position.y + 1.0 - 0.1).floor() as i32,
        state.position.z.floor() as i32,
    );
    let swimming_head_in_fluid = world.block(swimming_head_position).fluid != FluidState::Empty;
    state.eye_in_water = world.eyes_in_fluid(FluidState::Water, bounds, eyes, rules.release);
    state.eye_in_lava = world.eyes_in_fluid(FluidState::Lava, bounds, eyes, rules.release);
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
    // LivingEntity.aiStep (1.21.2+) checks the fall distance every tick the fall-flying flag is set,
    // before travel and in any fluid (1.21.2 LivingEntity.java:2796, 2865); before, only the glide
    // in travel does (below). The rocket's boost (FireworkRocketEntity.tick) has already run when
    // aiStep starts, so the check sees the boosted velocity; the boost itself is added below.
    let fall_flying_checks_before_travel = rules.release.at_least(1, 21, 2);
    if state.fall_flying && fall_flying_checks_before_travel {
        let velocity = state.velocity;
        state.add_delta_movement(state.firework_boost);
        state.check_fall_distance_accumulation();
        state.velocity = velocity;
    }
    if state.is_player && !state.passenger && state.swimming && state.in_water {
        let steering = if state.look.y < -0.2 {
            rules.swimming_downward_steering
        } else {
            rules.swimming_vertical_steering
        };
        if state.look.y <= 0.0 || state.jump_requested || swimming_head_in_fluid {
            state.velocity.y += (state.look.y - state.velocity.y) * steering;
        }
    }
    let flying_vertical_velocity = state.velocity.y;
    let going_down = state.velocity.y <= 0.0;
    // LivingEntity.travel before 1.20 resets the fall distance under Slow Falling only while not
    // rising (1.13.2 LivingEntity.java:1511-1513, 1.19.4 :1986-1990)
    if state.slow_falling && going_down && !effects_reset_fall_before_travel {
        state.fall_distance = 0.0;
    }
    // from 1.20.5 travel's gravity is getFinalGravity(), 0 with NoGravity, which the glide takes
    // too (1.20.5 LivingEntity.java:2111, Entity.java:1112-1114; 1.21.11 Entity.java:1398-1400);
    // before, the glide's fixed 0.08 ignores NoGravity (1.16.5 LivingEntity.java:1963)
    let travel_gravity = if state.no_gravity && rules.release.at_least(1, 20, 5) {
        0.0
    } else if state.slow_falling && going_down {
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
    // LivingEntity.travelFallFlying from 1.21.5: on a climbable block the glide travels as in the
    // air and stops (1.21.5 LivingEntity.java:2299-2302, 1.21.11 :2439-2442, 26.3 :2499-2502)
    let glide_stopped = state.fall_flying
        && matches!(fluid, FluidState::Empty)
        && climbing
        && rules.release.at_least(1, 21, 5);
    let gliding = state.fall_flying && matches!(fluid, FluidState::Empty) && !glide_stopped;
    if !gliding {
        let input = movement_input(state, rules.release);
        let yaw = move_relative_yaw(state, rules.release);
        state.velocity = state.velocity + relative_movement(input, input_speed, yaw, rules.release);
    }
    state.add_delta_movement(state.firework_boost);
    if gliding {
        // LivingEntity.travel's glide: from 1.19.3 Entity.checkSlowFallDistance lowers the fall
        // distance to 1.0 (1.19.3 Entity.java:2156-2158, LivingEntity.java:2117); 1.9 - 1.19.2 set
        // it to 1.0 outright, raising a smaller one (1.9.4 LivingEntity.java:1328-1329, 1.19.2
        // :2107-2109)
        if !fall_flying_checks_before_travel {
            if rules.release.at_least(1, 19, 3) {
                state.check_fall_distance_accumulation();
            } else if state.velocity.y > -0.5 {
                state.fall_distance = 1.0;
            }
        }
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
    state.move_slowed =
        !state.no_physics && state.stuck_speed_multiplier.length_squared() > 1.0e-7;
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
            outside_border: state.outside_border,
        }
    } else {
        collide_with_world(state, world, &collision_context, desired, rules)
    };
    let displacement = collision.movement;
    let move_start = state.position;
    // Entity.move (1.18.2+): a collided move of a block or more clips with FALLDAMAGE_RESETTING and
    // Fluid.WATER and resets the fall distance on a hit; from 1.21.9 the clip covers at most 8
    // blocks (1.19 Entity.java:570-580, 1.21.9 :703-711; 1.18.1 has none, 1.18.2 has the
    // #fall_damage_resetting tag). checkFallDamage still counts this move's drop afterwards.
    if !state.no_physics
        && rules.release.at_least(1, 18, 2)
        && state.fall_distance != 0.0
        && displacement.length_squared() >= 1.0
    {
        let clip_end = if rules.release.at_least(1, 21, 9) {
            // Vec3.normalize divides by the length (never under 1.0E-5F here)
            let length = displacement.length();
            let normal = Vec3::new(
                displacement.x / length,
                displacement.y / length,
                displacement.z / length,
            );
            move_start + normal.scale(length.min(8.0))
        } else {
            move_start + displacement
        };
        let shape = ClipShape::FallDamageResetting {
            player: state.is_player,
        };
        if clip_hits(world, move_start, clip_end, rules.release, shape) {
            state.fall_distance = 0.0;
        }
    }
    displace(state, desired, collision, rules.release);
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
        // Entity.move (1.21.4+) refreshes the vertical collision only on a vertical move or for
        // a locally controlled entity; earlier releases refresh it on every move.
        if !rules.release.at_least(1, 21, 4)
            || desired.y != 0.0
            || state.controlled_by_local_instance
        {
            state.collided_vertically = y_collision;
            state.vertical_collision_below = state.collided_vertically && desired.y < 0.0;
            state.on_ground = state.collided_vertically && desired.y < 0.0;
            support = check_supporting_block(world, state, Some(displacement));
        }
    } else if rules.release.at_least(1, 21, 9) {
        state.collided_horizontally = false;
        state.minor_horizontal_collision = false;
        state.collided_vertically = false;
        state.vertical_collision_below = false;
    }
    // Entity.move (1.21.2+) runs checkFallDamage on the client only for a locally controlled
    // entity (isLogicalSideForUpdatingMovement, isLocalInstanceAuthoritative from 1.21.5).
    let checks_fall_damage = !state.no_physics
        && !(state.is_player && rules.release == MinecraftVersion::new(1, 21, 4))
        && (!rules.release.at_least(1, 21, 2) || state.controlled_by_local_instance);
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
    // Entity.checkFallDamage (1.21.5+) counts this move's drop out of the water first, after
    // LivingEntity's water refresh, and lands with it (1.21.5 Entity.java:1284-1297,
    // LivingEntity.java:328-331); before, Entity.fall counts it only when not landing.
    let counts_drop_before_landing = rules.release.at_least(1, 21, 5);
    if checks_fall_damage && counts_drop_before_landing && !state.in_water && displacement.y < 0.0
    {
        state.fall_distance =
            accumulated_fall_distance(state.fall_distance, displacement.y, rules.release);
    }
    let landed_on = world.effect_at(landing_position(state, support, rules.release));
    let bounced =
        !restitution_enabled && y_collision && landed_on == BlockEffect::Slime && !state.sneaking;
    if bounced {
        if state.velocity.y < 0.0 {
            // SlimeBlock.bounceUp: a LivingEntity bounces back fully (1.21.11 SlimeBlock.java:44)
            state.velocity.y = -state.velocity.y;
        }
        state.fall_distance = 0.0;
        state.fall_damage = 0.0;
    }
    // Block.fallOn: the fall distance and damage multiplier the landing block hands
    // causeFallDamage. The bed halves the distance from 1.12 (BedBlock); an upward pointed
    // dripstone tip adds 2.0F to it and doubles the damage, 2.5 in double from 1.21.5 (1.17
    // PointedDripstoneBlock.java:128-130, 1.21.5 :147-149); hay deals a fifth from 1.9 (1.9.4
    // HayBlock.java:19-20; 1.8.9 has no fallOn), honey a fifth (1.15 HoneyBlock.java:55-61).
    let landed_on_bed = landed_on == BlockEffect::Bed && rules.release.at_least(1, 12, 0);
    let landing_fall_distance = match landed_on {
        _ if landed_on_bed => state.fall_distance * 0.5,
        BlockEffect::PointedDripstoneTip if rules.release.at_least(1, 21, 5) => {
            state.fall_distance + 2.5
        }
        BlockEffect::PointedDripstoneTip => f64::from(state.fall_distance as f32 + 2.0),
        _ => state.fall_distance,
    };
    let block_damage_multiplier = match landed_on {
        BlockEffect::Hay if rules.release.at_least(1, 9, 0) => 0.2_f32,
        BlockEffect::Honey => 0.2,
        BlockEffect::PointedDripstoneTip => 2.0,
        _ => 1.0,
    };
    // A fallOn that never calls causeFallDamage: powder snow's from 1.18 (1.18
    // PowderSnowBlock.java:85-91; 1.17.1's jar declares none) and, from 1.21.2, the slime
    // block's for an entity suppressing the bounce (1.21.2 SlimeBlock.java:24-28; 1.21.1 hands it
    // to Block.fallOn). Entity.checkFallDamage still resets the fall distance.
    let landing_deals_no_damage = match landed_on {
        BlockEffect::PowderSnow => rules.release.at_least(1, 18, 0),
        BlockEffect::Slime => restitution_enabled || rules.release.at_least(1, 21, 2),
        _ => false,
    };
    if checks_fall_damage && state.on_ground && !bounced {
        if state.on_ground {
            // Player.causeFallDamage deals nothing where the player may fly (1.8.9
            // PlayerEntity.java:1387-1388, 1.21.5 Player.java:1470-1472)
            state.fall_damage = if state.fall_damage_immune || landing_deals_no_damage {
                0.0
            } else if modern_fall_damage {
                attribute_landing_damage(
                    landing_fall_distance,
                    state.safe_fall_distance,
                    block_damage_multiplier,
                    state.fall_damage_multiplier.unwrap_or(1.0),
                    landing_damage_rounding,
                    rules.release,
                )
            } else {
                landing_damage(
                    landing_fall_distance,
                    state.safe_fall_distance,
                    f64::from(
                        block_damage_multiplier
                            * state.fall_damage_multiplier.unwrap_or(1.0) as f32,
                    ),
                    state.jump_boost_amplifier,
                    landing_damage_rounding,
                )
            };
            state.fall_distance = 0.0;
        }
    } else if checks_fall_damage
        && !counts_drop_before_landing
        && !bounced
        && displacement.y < 0.0
    {
        state.fall_distance =
            accumulated_fall_distance(state.fall_distance, displacement.y, rules.release);
    }

    // Entity.move (1.17 - 1.19) stops after checkFallDamage for a removed entity
    let removed_in_move = state.removed && rules.release.at_least(1, 17, 0);
    // Block.updateEntityMovementAfterFallOn (onLanded, setEntityVelocity before 1.14) runs on any
    // vertical collision, a ceiling's too, for a locally controlled entity only from 1.21.2 (1.21
    // Entity.java:665-668, 1.21.3 :698-703, 1.21.11 :768-773; 1.12.2 :826-828). 26.2 has no such
    // hook: restituteMovementAfterCollisions takes the vertical collision of a vertical move only,
    // `movedVertically = Math.abs(delta.y) > 0.0`, so a step-up from a move with no y does not
    // count (26.2 Entity.java:793, 815).
    state.block_fall_callback = !state.no_physics
        && !removed_in_move
        && y_collision
        && (!rules.release.at_least(1, 21, 2) || state.controlled_by_local_instance)
        && (!rules.release.at_least(26, 2, 0) || desired.y != 0.0);
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
                    // BedBlock.bounceUp, a LivingEntity's (1.21.11 BedBlock.java:149-150)
                    state.velocity.y = -state.velocity.y * 0.6600000262260437;
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
    if !state.no_physics {
        update_in_lava_after_move(state, world, rules.release);
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
    let mut inside_after_move = Vec::new();
    if !rules.release.at_least(1, 21, 2) && !removed_in_move {
        let inside: Vec<_> = blocks_inside(state, rules.release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_block_effects(state, world, support, rules.release, &inside);
        inside_after_move = inside;
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
        && (climbable(world, state, rules.release) || climbs_powder_snow)
    {
        state.velocity.y = 0.2;
    }
    if matches!(fluid, FluidState::Empty) && !gliding && !state.flying {
        if state.levitation {
            let target = 0.05 * (f64::from(state.levitation_amplifier.unwrap_or(0)) + 1.0);
            state.velocity.y += (target - state.velocity.y) * 0.2;
            // LivingEntity.travel's air move resets the fall distance under Levitation after the
            // move, 1.13 - 1.19.4 (1.13.2 LivingEntity.java:1609-1612, 1.19.4 :2093-2095); 1.9 -
            // 1.12 never reset it (1.12.2 LivingEntity.java:1543-1545)
            if rules.release.at_least(1, 13, 0) && !effects_reset_fall_before_travel {
                state.fall_distance = 0.0;
            }
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
        && climbable(world, state, rules.release)
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
        let context = entity_collision_context(state);
        let entity = collision_query_entity(state, &context);
        let free = if legacy_fluids {
            legacy_fluid::is_free(world, entity, hop_bounds, rules.release)
        } else {
            world.is_free_for_entity(hop_bounds, entity, rules.release)
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
        // Player.travel's flight resets the fall distance and the glide from 1.9 (1.9.4
        // PlayerEntity.java:1410-1411); 1.8 only scales the vertical motion (1.8.9
        // PlayerEntity.java:1303-1309)
        if !flight_resets_fall_before_travel && rules.release.at_least(1, 9, 0) {
            state.fall_distance = 0.0;
            state.fall_flying = false;
        }
    }
    // LivingEntity.stopFallFlying, after the air travel
    if glide_stopped {
        state.fall_flying = false;
    }

    let mut freeze_steps = Vec::new();
    if rules.release.at_least(1, 21, 2) {
        let movements =
            movements_this_tick(state, old_position, desired, displacement, rules.release);
        let (inside, steps) = blocks_inside_moves(state, world, &movements, rules.release);
        apply_block_effects(state, world, support, rules.release, &inside);
        inside_after_move = inside;
        freeze_steps = steps;
    }
    if state.no_physics {
        inside_after_move.clear();
        freeze_steps.clear();
    }
    apply_inside_block_signals(
        state,
        world,
        &inside_after_move,
        &freeze_steps,
        rules.release,
    );
    if state.is_player && state.flying && state.on_ground && !state.no_physics {
        state.flying = false;
    }
    if !rules.release.at_least(1, 14, 0) {
        state.in_lava = legacy_fluid::touching_lava(world, state.bounds(), rules.release);
    }
    state.was_sprinting = state.sprinting;
    state.previous_position = Some(old_position);

    TickResult {
        displacement,
        collided_horizontally: state.collided_horizontally,
        collided_vertically: state.collided_vertically,
        block_fall_callback: state.block_fall_callback,
    }
}
