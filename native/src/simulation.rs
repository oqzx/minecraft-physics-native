use crate::block_traversal::{blocks_visited, blocks_visited_where, Movement, Visit};
use crate::collision::{
    collide_legacy, collide_shapes, vanilla_equal, BorderNear, Colliders, CollisionRules, StepUp,
};
use crate::entity::{
    boost_by_attached_rockets, boost_by_attached_rockets_before_player, check_auto_spin_attack, nearby_entity_colliders,
    push_by_nearby_entities, Dimensions, EntityFlags, EntityPush, EntityState, NearbyEntity,
};
use crate::phase::{self, Phase};
use crate::piston::{push_by_moving_pistons, MovingPiston};
use crate::legacy_fluid;
use crate::projectile::{clip_hits, ClipShape};
use crate::math::{
    MAX_WALKED_CELLS,
    ceil_f32, ceil_i32_for, clamp_f64_for, fast_inv_sqrt, fast_inv_sqrt_f32, floor_f32,
    floor_f32_for, floor_i32_for, java_cos, java_cos_legacy, java_float, java_sin,
    java_sin_legacy, lerp, Aabb, Vec3,
};
use crate::version::MinecraftVersion;
use crate::world::{WorldSnapshot, FENCE_LIKE, WALL_OR_GATE};
use crate::world::{
    shapes_join, BlockEffect, CollisionQueryEntity, EntityCollisionContext, FluidContact, FluidState,
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
    /// getFluidFallingAdjustedMovement's divisor of the gravity in water and shallow lava (1.13+;
    /// before, the water branch takes a literal 0.02, legacy_fluid::sink_in_water).
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

    /// Whether a landing bounces by the restitution of the block it lands on (Entity.move from
    /// 26.2); the profiles of those releases say so.
    fn restitution_enabled() -> bool {
        false
    }

    /// Whether the fall damage takes the SAFE_FALL_DISTANCE and FALL_DAMAGE_MULTIPLIER attributes
    /// (from 1.20.5); by default with the WATER_MOVEMENT_EFFICIENCY attribute, and the profiles of
    /// 1.20.5-1.20.6 and 26.1-26.3 say so themselves.
    fn modern_fall_damage() -> bool {
        Self::rules().water_movement_efficiency_attribute
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandingDamageRounding {
    Ceil,
    Floor,
}

/// LivingEntity.computeFallDamage before 1.20.5, in float: `ceil((fallDistance - 3.0F -
/// jumpBoost) * damageMultiplier)`, where `safe_fall_distance` stands for the 3.0F (1.8.8
/// LivingEntity.java:865, 1.20.4 :1566-1572). Nothing below one point.
pub(crate) fn landing_damage(
    fall_distance: f64,
    safe_fall_distance: f64,
    multiplier: f64,
    jump_boost_amplifier: Option<i32>,
    rounding: LandingDamageRounding,
) -> f64 {
    let jump_boost = jump_boost_amplifier.map_or(0.0, |amplifier| (amplifier + 1) as f32);
    let damage =
        (fall_distance as f32 - safe_fall_distance as f32 - jump_boost) * multiplier as f32;
    // Mth.ceil and Mth.floor of a float, an int; this formula ends before 26.1, so a huge fall wraps
    let damage = match rounding {
        LandingDamageRounding::Ceil => ceil_f32(damage),
        LandingDamageRounding::Floor => floor_f32(damage),
    };
    f64::from(damage.max(0))
}

/// LivingEntity.computeFallDamage from 1.20.5, with the SAFE_FALL_DISTANCE and
/// FALL_DAMAGE_MULTIPLIER attributes (Jump Boost raises the first) and the landing block's
/// damage multiplier (1.0F but where Block.fallOn says otherwise): 1.20.5 - 1.21.4 in float,
/// `ceil((double)((fallDistance - (float)safe) * damageMultiplier) * multiplier)` (1.20.5
/// LivingEntity.java:1593-1599, 1.21.4 :1621-1627); from 1.21.5 in double with an epsilon,
/// `floor((fallDistance + 1.0E-6 - safe) * damageMultiplier * multiplier)` (1.21.5
/// LivingEntity.java:1643-1653). Nothing below one point.
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
    // an int: past its range Mth.ceil wraps to a negative, so to no damage, until 26.1 saturates
    let damage = match rounding {
        LandingDamageRounding::Ceil => ceil_i32_for(release, damage),
        LandingDamageRounding::Floor => floor_i32_for(release, damage),
    };
    f64::from(damage.max(0))
}

/// Entity.fall's (checkFallDamage's) count of a downward move: in float before 1.21.5,
/// `(float)((double)fallDistance - dy)` up to 1.18.1 and `fallDistance -= (float)dy` from 1.18.2
/// (1.18.1 Entity.java:1019, 1.18.2 :991); a double
/// field from 1.21.5, `fallDistance -= (double)(float)dy` (1.21.5 Entity.java:1323).
pub(crate) fn accumulated_fall_distance(
    fall_distance: f64,
    dy: f64,
    release: MinecraftVersion,
) -> f64 {
    if release.at_least(1, 21, 5) {
        fall_distance - f64::from(dy as f32)
    } else if release.at_least(1, 18, 2) {
        // from 1.18.2 (its Entity.checkFallDamage: d2f, fsub; 1.18.1 subtracts the doubles, then rounds)
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
        P::restitution_enabled(),
        P::modern_fall_damage(),
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

impl Default for VersionPhysicsRules {
    fn default() -> Self {
        Self {
            release: MinecraftVersion::new(26, 2, 0),
            gravity: 0.08,
            water_gravity_divisor: 16.0,
            lava_gravity_divisor: 4.0,
            water_drag: 0.8,
            water_sprint_drag: 0.9,
            lava_drag: 0.5,
            air_drag: 0.91,
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
            movement_speed: 0.1f32 as f64,
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
    speed_effect_amplifier: Option<i32>,
    slowness_effect_amplifier: Option<i32>,
    sprint_active: bool,
    sprint_multiplier: f64,
    release: MinecraftVersion,
) -> f64 {
    let sprint = sprint_active.then(|| java_float(sprint_multiplier - 1.0));
    let speed =
        speed_effect_amplifier.map(|amplifier| 0.20000000298023224 * (f64::from(amplifier) + 1.0));
    let slowness = slowness_effect_amplifier
        .map(|amplifier| -0.15000000596046448 * (f64::from(amplifier) + 1.0));
    // (1.20.6 AttributeInstance.java:22; 1.21 AttributeInstance.java:25)
    let modifiers = if release.at_least(1, 21, 0) {
        [sprint, speed, slowness]
    // (1.20.4 AttributeInstance.java:52; 1.20.5 AttributeInstance.java:55)
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

/// The boat's observed state when its passenger checks fluids. Its ordinary IN_WATER status
/// is not underwater: only UNDER_WATER and UNDER_FLOWING_WATER set `underwater`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatVehicle {
    pub bounding_box: Aabb,
    pub underwater: bool,
}

/// The ordered nextInt(1, 3) draws a ServerPlayer's ignitions read from the level's random
/// (BaseFireBlock.fireIgnite, 1.21.2+), which the engine cannot draw itself. One bit each, so the
/// state stays Copy and the draws follow it through every tick path; the rest of a stream goes
/// back to the caller with the state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerFireDraws {
    bits: u128,
    remaining: u8,
}

impl ServerFireDraws {
    pub const CAPACITY: usize = 128;

    /// None for more draws than a stream holds or a draw that is not 1 or 2.
    pub fn new(draws: &[i32]) -> Option<Self> {
        if draws.len() > Self::CAPACITY {
            return None;
        }
        let mut bits = 0_u128;
        for (index, draw) in draws.iter().enumerate() {
            match draw {
                1 => {}
                2 => bits |= 1 << index,
                _ => return None,
            }
        }
        Some(Self {
            bits,
            remaining: draws.len() as u8,
        })
    }

    fn next(&mut self) -> Option<i32> {
        if self.remaining == 0 {
            return None;
        }
        let draw = 1 + (self.bits & 1) as i32;
        self.bits = self.bits.checked_shr(1).unwrap_or(0);
        self.remaining -= 1;
        Some(draw)
    }

    /// The draws not read yet, in order.
    pub fn to_vec(self) -> Vec<i32> {
        let mut rest = self;
        std::iter::from_fn(|| rest.next()).collect()
    }
}

/// One tick of one entity. Its counters, all a Java int in vanilla, starting at 0 (26.3 lines):
///
/// - `tick_count`: Entity.tickCount. Advanced before the tick, for every tick the level admits
///   (Entity.commonTick, :550, from ClientLevel.tickNonPassenger); kept as the int's bits, it wraps.
/// - `jump_ticks`: LivingEntity.noJumpDelay. Down at the top of aiStep (:2959), 10 on a jump (:3026),
///   0 on a tick without the jump key (:3035).
/// - `auto_jump_time`: LocalPlayer.autoJumpTime. Down in aiStep, holding the jump key (:768); set by
///   the move's updateAutoJump (:1083).
/// - `ticks_frozen`: Entity.TicksFrozen, tracked data. The server's to count, up 1 to the limit in
///   powder snow and down 2 to 0 outside; the client reads it.
/// - `remaining_fire_ticks`: Entity.remainingFireTicks, the server's; the client clears its own.
///
/// The adapter owns what the engine does not read or that lives in the caller's tick: `age`
/// (mineflayer-physics-utils' count, no input here), itemUseTicks, autoSpinAttackTicks,
/// fireworkRocketDuration, sprintTriggerTime and flyJumpTriggerTime. A projectile's tick count,
/// flying ticks, life, inGroundTime and shakeTime are in projectile.rs and projectile_hits.rs; a
/// vehicle's boost, stand, dash and bubble counters in ridden.rs and vehicle.rs.
/// The moves others' ticks recorded in an entity's movementThisTick since its own last
/// applyEffectsFromBlocks, in order: up to four, past which the first two are joined into one as
/// 26.3's list does past its 100 (26.3 Entity.java:977-985; 1.21.5 keeps every one, Entity.java:720).
/// Kept in place so that the state stays Copy; four shulker boxes pushing one entity between two
/// of its ticks is more than a level holds in practice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HeldMovements {
    movements: [Option<Movement>; 4],
}

impl HeldMovements {
    pub(crate) fn push(&mut self, movement: Movement) {
        if self.movements.iter().all(Option::is_some) {
            let (first, second) = (self.movements[0].unwrap(), self.movements[1].unwrap());
            self.movements.rotate_left(1);
            self.movements[0] = Some(Movement { from: first.from, to: second.to, axis_order: None });
            self.movements[3] = None;
        }
        let free = self.movements.iter().position(Option::is_none).unwrap_or(3);
        self.movements[free] = Some(movement);
    }

    pub(crate) fn as_slice(&self) -> Vec<Movement> {
        self.movements.iter().flatten().copied().collect()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.movements[0].is_none()
    }
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
    /// How far this tick's riptide launch lifted the entity off the ground (the 1.2 move of
    /// TridentItem.releaseUsing), zero without one: the box aiStep takes before travel is past it.
    pub riptide_lift: Vec3,
    /// A move another's tick made that stays in the entity's movementThisTick until its own
    /// applyEffectsFromBlocks takes it: a shulker box's push from 1.21.5, which applies nothing
    /// itself (26.3 ShulkerBoxBlockEntity.java:133, Entity.java:807, 960-963), where a piston's takes
    /// its own back (1.21.5 PistonMovingBlockEntity.java:182-183, 26.3 :167-168). Each push is a
    /// batch of its own in the list, in the order they came (1.21.5 Entity.java:259, 720, 811-815).
    pub(crate) pending_movements: HeldMovements,
    /// The riptide release already ran this tick, where the client handles the use key before
    /// its entities tick (versioned::release_riptide): the tick does not run it again (D-214).
    pub(crate) riptide_released: bool,
    /// The Movement the released lift recorded for this tick's movementThisTick (1.21.5+).
    pub(crate) riptide_lift_movement: Option<Movement>,
    pub sprinting: bool,
    pub was_sprinting: bool,
    pub sprint_modifier: Option<bool>,
    pub in_water: bool,
    pub in_lava: bool,
    pub ultra_warm: bool,
    pub is_player: bool,
    pub controlled_by_local_instance: bool,
    /// Exact Entity.canSimulateMovement result for 26.3+ piston bounce/restitution and bubble
    /// column pushes. Without it, players use their side/local authority and other entities use local authority.
    pub can_simulate_movement: Option<bool>,
    /// The entity is no LivingEntity: a slime block or bed returns 0.8 of the bounce (from 1.9,
    /// the bed's from 1.12), from 26.2 0.8F of the block's restitution. `block_bounciness` is
    /// taken as resolved.
    pub non_living: bool,
    pub passenger: bool,
    /// Entity.isVehicle: it carries passengers, which a server can seat on a player (vanilla's /ride
    /// refuses to). From 1.9 nothing pushes it (Entity.push).
    pub is_vehicle: bool,
    /// Absolute boat geometry at the fluid-query phase; ignored when not a passenger.
    pub boat_vehicle: Option<BoatVehicle>,
    pub in_web: bool,
    pub weaving: bool,
    /// Frost Walker on the boots: magma and lit campfires deal no contact damage.
    pub frost_walker: bool,
    pub sweet_berry_bush_immune: bool,
    /// A boat, which is no LivingEntity: powder snow holds it wherever its feet are
    /// (PowderSnowBlock.entityInside), and a bubble column with nothing above it does not move
    /// the boat its player controls (AbstractBoat.onAboveBubbleColumn).
    pub boat: bool,
    /// A boat's onAboveBubbleColumn on the last block effects: the column's drag.
    pub above_bubble_column: Option<bool>,
    pub powder_snow_immune: bool,
    pub powder_snow_walkable: bool,
    pub in_powder_snow: bool,
    /// EntityType.fireImmune: fire never ignites it and it takes no fire damage.
    pub fire_immune: bool,
    /// MobEffects.FIRE_RESISTANCE: LivingEntity.hurt refuses all fire damage (1.8.8
    /// LivingEntity.java:683, 1.20.4 :1108, 26.3 :1154), while the burning itself goes on.
    pub fire_resistance: bool,
    /// Entity.getSharedFlag(0) as the server last sent it, which isOnFire reads on a client.
    pub on_fire_flag: bool,
    /// Entity.isInRain (isBeingRainedOn before 1.21.5), which the world does not know: rain at
    /// the block position or at the box's top, which puts the fire out.
    pub in_rain: bool,
    /// A spear's stab that dealt knockback since the last tick, with the weapon's own knockback
    /// (take_stab_hit). Taken once, and only on a server state.
    pub stab_hit: Option<f32>,
    /// The highest Fire Protection level the entity wears, which shortens its burning before 1.21.
    pub fire_protection: u32,
    /// The BURNING_TIME attribute (1.21+), 1.0 when not given.
    pub burning_time: Option<f64>,
    /// Entity.remainingFireTicks; the field's initial value when not given.
    pub remaining_fire_ticks: Option<i32>,
    /// The nextInt(1, 3) of a ServerPlayer's ignition (BaseFireBlock.fireIgnite, 1.21.2+), which
    /// the engine cannot draw; 1 when not given. A fixed value for every ignition, an
    /// approximation of the level's random; server_fire_draws is the exact form.
    pub server_fire_increment: Option<i32>,
    /// Each ignition's own draw, read one per ignition that reaches nextInt, in order.
    pub server_fire_draws: Option<ServerFireDraws>,
    /// An ignition found server_fire_draws empty: the result is no exact replay.
    pub server_fire_draws_exhausted: bool,
    /// A tick needed a ServerPlayer's fire draw and the state gave neither input.
    pub server_fire_draw_missing: bool,
    /// The tick's move swept more cells than a call may walk (MAX_WALKED_CELLS): it did not run, and
    /// no result of the tick is published.
    pub move_out_of_range: bool,
    pub ticks_frozen: u32,
    pub ticks_required_to_freeze: u32,
    /// Entity.tickCount, the bits of a Java int.
    pub tick_count: u32,
    pub freeze_damage: f64,
    pub contact_damage: f64,
    /// What this tick's flight into a wall hands hurt, the server's alone (flyIntoWall).
    pub wall_damage: f64,
    pub inside_wall: bool,
    /// LivingEntity.isSleeping (Player.inBed before 1.14): a sleeper is never in a wall.
    pub sleeping: bool,
    pub eye_height: Option<f64>,
    pub eye_in_water: bool,
    pub eye_in_lava: bool,
    pub slow_falling: bool,
    pub levitation: bool,
    pub levitation_amplifier: Option<i32>,
    pub fall_distance: f64,
    pub safe_fall_distance: f64,
    pub fall_damage_immune: bool,
    /// The state is the client's (the local player as the client ticks it): the client's rules where vanilla's sides
    /// differ (a living entity's fire cleared in baseTick up to 1.21.8, freeze ticks counted by the server alone).
    pub client_side: bool,
    /// The height the pose update leaves (Player.tick's updatePlayerPose, updateSize before 1.14),
    /// when the caller knows it: what ticks after the player meets the box of that height.
    pub height_after_tick: Option<f64>,
    /// Abilities.invulnerable (creative, or set by a command): a player's fire ticks are at most 1 from 1.16
    /// (Player.setRemainingFireTicks).
    pub invulnerable: bool,
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
    /// Spider.makeStuckInBlock: a cobweb does not hold it.
    pub web_immune: bool,
    /// WitherBoss.makeStuckInBlock: nothing holds it.
    pub stuck_immune: bool,
    pub no_gravity: bool,
    /// Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+).
    pub removed: bool,
    /// Its health is 0 (LivingEntity.isAlive is false): from 1.11 no entity pushes it.
    pub dead: bool,
    pub gravity: Option<f64>,
    pub step_height: Option<f64>,
    pub movement_speed: Option<f64>,
    /// AttributeInstance.getValue of MOVEMENT_SPEED as the caller folded it, with every modifier it
    /// holds (the sprint, speed and slowness ones too). None folds `movement_speed` with the
    /// sprint, speed and slowness modifiers alone.
    pub movement_speed_value: Option<f64>,
    pub jump_strength: Option<f64>,
    pub speed_effect_amplifier: Option<i32>,
    pub slowness_effect_amplifier: Option<i32>,
    pub jump_boost_amplifier: Option<i32>,
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
    /// The look along the yaw the player's last aiStep left in yHeadRot, with this tick's pitch:
    /// what a 1.11 rocket that ticks before the player boosts along (D-222); None takes `look`.
    pub head_look: Option<Vec3>,
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
            riptide_lift: Vec3::ZERO,
            pending_movements: HeldMovements::default(),
            riptide_released: false,
            riptide_lift_movement: None,
            sprinting: false,
            was_sprinting: false,
            sprint_modifier: None,
            in_water: value.flags.in_water,
            in_lava: value.flags.in_lava,
            ultra_warm: false,
            is_player: false,
            controlled_by_local_instance: false,
            can_simulate_movement: None,
            non_living: false,
            passenger: false,
            is_vehicle: false,
            boat_vehicle: None,
            in_web: false,
            weaving: false,
            frost_walker: false,
            sweet_berry_bush_immune: false,
            boat: false,
            above_bubble_column: None,
            powder_snow_immune: false,
            powder_snow_walkable: false,
            in_powder_snow: false,
            fire_immune: false,
            fire_resistance: false,
            on_fire_flag: false,
            in_rain: false,
            stab_hit: None,
            fire_protection: 0,
            burning_time: None,
            remaining_fire_ticks: None,
            server_fire_increment: None,
            server_fire_draws: None,
            server_fire_draws_exhausted: false,
            server_fire_draw_missing: false,
            move_out_of_range: false,
            ticks_frozen: 0,
            ticks_required_to_freeze: 140,
            tick_count: 0,
            freeze_damage: 0.0,
            contact_damage: 0.0,
            wall_damage: 0.0,
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
            client_side: false,
            height_after_tick: None,
            invulnerable: false,
            fall_damage_multiplier: Some(1.0),
            sneaking: false,
            moving_slowly: None,
            stepping_carefully: false,
            using_item: false,
            item_use_speed_multiplier: None,
            swimming: false,
            flying: false,
            // Abilities.flyingSpeed (1.21.11 Abilities.java:19)
            flying_speed: f64::from(0.05_f32),
            climbing: false,
            no_physics: false,
            web_immune: false,
            stuck_immune: false,
            no_gravity: value.no_gravity,
            removed: false,
            dead: false,
            gravity: value.gravity,
            step_height: value.step_height,
            movement_speed: None,
            movement_speed_value: None,
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
            head_look: None,
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
            no_gravity: value.no_gravity,
            gravity: value.gravity,
            step_height: value.step_height,
        }
    }
}

impl SimulationState {
    /// Merges an entity state (a prediction's result, or the server's correction of it) into
    /// this one: its motion, size, contact flags and its own gravity replace this state's, and
    /// everything EntityState has no place for (the input, effects, counters) stays.
    pub fn merge_entity_state(&mut self, entity: EntityState) {
        self.position = entity.position;
        self.velocity = entity.velocity;
        self.half_width = entity.dimensions.half_width;
        self.height = entity.dimensions.height;
        self.on_ground = entity.flags.on_ground;
        self.collided_horizontally = entity.flags.horizontal_collision;
        self.collided_vertically = entity.flags.vertical_collision;
        self.vertical_collision_below = entity.flags.vertical_collision_below;
        self.in_water = entity.flags.in_water;
        self.in_lava = entity.flags.in_lava;
        self.no_gravity = entity.no_gravity;
        self.gravity = entity.gravity;
        self.step_height = entity.step_height;
    }
}

impl SimulationState {
    /// Entity.updateInWaterStateAndDoWaterCurrentPushing skips a boat passenger from 1.9;
    /// 1.19.3 lets an underwater boat's passenger touch water (1.19.2 Entity.java:1069,
    /// 1.19.3 :1084). From 26.1 the adjusted interaction box replaces this exclusion.
    /// (1.8.8 Entity.java:836-837 has no boat test; 1.9.4 Entity.java:944-946 clears it in a
    /// boat; 1.21.11 Entity.java:1566-1572 still returns early; 26.1 Entity.java:1572-1583 has
    /// no vehicle test, the boat trims the fluid box instead)
    pub(crate) fn boat_excludes_water(self, release: MinecraftVersion) -> bool {
        release.at_least(1, 9, 0)
            && !release.at_least(26, 1, 0)
            && self.boat_vehicle.filter(|_| self.passenger).is_some_and(|boat| {
                !release.at_least(1, 19, 3) || !boat.underwater
            })
    }

    /// Entity.getFluidInteractionBox, then AbstractBoat.modifyPassengerFluidInteractionBox
    /// (26.1+, 26.3 Entity.java:4233, AbstractBoat.java:787): deflate first, then trim. None
    /// means the boat covers the whole scan, not that the physical entity has no bounds.
    pub(crate) fn fluid_interaction_area(self) -> Option<Aabb> {
        let mut area = self.bounds().deflate(Vec3::new(0.001, 0.001, 0.001));
        if let Some(boat) = self.boat_vehicle.filter(|boat| self.passenger && !boat.underwater) {
            if boat.bounding_box.max.y >= area.max.y {
                return None;
            }
            area.min.y = area.min.y.max(boat.bounding_box.max.y);
        }
        Some(area)
    }

    /// The entity's box (vanilla: Entity#getBoundingBox): the one it keeps, else the one around
    /// its position.
    #[must_use]
    pub fn bounds(self) -> Aabb {
        self.bounding_box
            .unwrap_or_else(|| self.bounds_around_position())
    }

    /// The box the entity's size makes around its position (vanilla:
    /// EntityDimensions#makeBoundingBox).
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

    /// Entity#checkFallDistanceAccumulation: a fall of more than 1 that is no longer falling
    /// faster than 0.5 is cut to 1.
    pub fn check_fall_distance_accumulation(&mut self) {
        if self.velocity.y > -0.5 && self.fall_distance > 1.0 {
            self.fall_distance = 1.0;
        }
    }

    /// Entity.addDeltaMovement: from 1.21.11 an impulse that is not finite, or a sum that is not,
    /// leaves the motion as it was (1.21.11 Entity.java:3727-3739, setDeltaMovement's guard; 1.21.10
    /// :3709-3715 has none, nor do the setVelocity/addVelocity of 1.20.4 - 1.21.4).
    pub fn add_delta_movement(&mut self, impulse: Vec3, release: MinecraftVersion) {
        if !release.at_least(1, 21, 11) {
            self.velocity = self.velocity + impulse;
            return;
        }
        if impulse.is_finite() {
            self.set_delta_movement(self.velocity + impulse, release);
        }
    }

    /// Entity.setDeltaMovement: from 1.21.11 a motion that is not finite is refused and the motion
    /// stays as it was (1.21.11 Entity.java:3727-3732); before, any motion is taken.
    pub fn set_delta_movement(&mut self, velocity: Vec3, release: MinecraftVersion) {
        if velocity.is_finite() || !release.at_least(1, 21, 11) {
            self.velocity = velocity;
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

/// AttributeModifier.Operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeOperation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttributeModifier {
    pub amount: f64,
    pub operation: AttributeOperation,
}

/// The movement speed attribute: its base, its modifiers but the powder snow one, and that one's
/// amount if it holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MovementSpeedAttribute {
    pub base: f64,
    pub modifiers: Vec<AttributeModifier>,
    pub frost: Option<f64>,
}

impl MovementSpeedAttribute {
    /// AttributeInstance.calculateValue, sanitized as the clamped attribute is, 0 to 1024.
    fn value(&self) -> f64 {
        let held = |operation| {
            self.modifiers
                .iter()
                .filter(move |modifier| modifier.operation == operation)
                .map(|modifier| modifier.amount)
        };
        let mut added = self.base;
        for amount in held(AttributeOperation::AddValue).chain(self.frost) {
            added += amount;
        }
        let mut scaled = added;
        for amount in held(AttributeOperation::AddMultipliedBase) {
            scaled += added * amount;
        }
        for amount in held(AttributeOperation::AddMultipliedTotal) {
            scaled *= 1.0 + amount;
        }
        scaled.clamp(0.0, 1024.0)
    }
}

/// What one client tick of the local player reads beside its state and world: the entities near
/// it in the order the client ticks them, the firework rockets attached to it (all of them and
/// those the client ticks before it), whether its riptide spin attack runs this tick, the moving
/// piston blocks that push it after every entity ticked, and how far the pistons of this tick
/// already moved it (Entity.pistonDeltas). `rocket_ticks` lists the ticks each attached rocket
/// has left, those before the player first; None: the rockets do not end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickScene {
    /// The movement speed attribute as the tick begins, where a caller has its parts: only the
    /// frost modifier changes it in the tick (apply_frost).
    pub movement_speed: Option<MovementSpeedAttribute>,
    pub nearby_entities: Vec<NearbyEntity>,
    pub firework_rockets: u32,
    pub firework_rockets_before_player: u32,
    pub rocket_ticks: Option<Vec<u32>>,
    pub auto_spin_attack: bool,
    pub moving_pistons: Vec<MovingPiston>,
    pub piston_deltas: [f64; 3],
}

impl TickScene {
    /// Takes `spent` ticks off each attached rocket's and drops those left with none, keeping
    /// the count of those before the player (vanilla: FireworkRocketEntity#tick, its life counted
    /// up to its lifetime).
    pub fn end_rockets(&mut self, spent: u32) {
        let Some(ticks) = &mut self.rocket_ticks else {
            return;
        };
        let before = self.firework_rockets_before_player as usize;
        let mut kept_before = 0;
        let mut index = 0;
        ticks.retain_mut(|left| {
            *left = left.saturating_sub(spent);
            let keep = *left > 0;
            if keep && index < before {
                kept_before += 1;
            }
            index += 1;
            keep
        });
        self.firework_rockets = ticks.len() as u32;
        self.firework_rockets_before_player = kept_before;
    }

    /// The scene's own part of the tick, after the player's, which leaves the scene the next tick
    /// reads: the moving pistons' progressO = progress, and below 1 progress = min(progress +
    /// 0.5F, 1), the block ending (pushing nothing from 1.11) the tick after it reaches 1 (1.8.8
    /// PistonMovingBlockEntity.java:123-140, 1.12.2 :277-292, 1.21.11 PistonMovingBlockEntity.java:
    /// 294-326). Before 1.11 that tick pushes a last time, pushEntities(1.0F, 0.25F), and removes
    /// the block entity, so the piston leaves the scene (1.8.8 PistonMovingBlockEntity.java:123-131,
    /// 1.10.2 :167-172); from 1.11 the ended block pushes nothing, removed (1.12.2 :279-285) or
    /// kept 5 client death ticks (1.16.4 PistonMovingBlockEntity.java:277-279), so it stays. The
    /// attached rockets count the tick off (FireworkRocketEntity.tick, 1.21.11
    /// FireworkRocketEntity.java:154-165), an ended spin attack runs no more, and the piston
    /// deltas start over, as the next game time clears them (1.21.11 Entity.java:1045-1048).
    pub fn advance(&mut self, auto_spin_attack_ended: bool, release: MinecraftVersion) {
        let ended_removed = !release.at_least(1, 11, 0);
        self.moving_pistons.retain_mut(|piston| {
            if piston.progress >= 1.0 {
                return !ended_removed;
            }
            piston.progress = (piston.progress + 0.5).min(1.0);
            true
        });
        self.end_rockets(1);
        self.auto_spin_attack &= !auto_spin_attack_ended;
        self.piston_deltas = [0.0; 3];
    }
}

/// One client tick of the local player in a scene (tick_local_player_scene), then the scene's
/// own (TickScene::advance): what simulateVersionedTick and a TickInputSnapshot both run.
pub(crate) fn tick_scene(
    world: &mut WorldSnapshot,
    state: &mut SimulationState,
    scene: &mut TickScene,
    config: PhysicsConfig,
    release: MinecraftVersion,
    release_use_key: impl FnOnce(&mut SimulationState, &WorldSnapshot) -> Result<(), String>,
    tick: impl FnOnce(&mut SimulationState, &WorldSnapshot) -> Result<TickResult, String>,
) -> Result<TickSceneOutcome, String> {
    let outcome = tick_local_player_scene(
        world,
        state,
        scene.movement_speed.as_mut(),
        &scene.nearby_entities,
        (scene.firework_rockets, scene.firework_rockets_before_player),
        scene.auto_spin_attack,
        &scene.moving_pistons,
        &mut scene.piston_deltas,
        config,
        release,
        release_use_key,
        tick,
    )?;
    scene.advance(outcome.auto_spin_attack_ended, release);
    Ok(outcome)
}

/// What one client tick of the local player did beside its state: the move's result, the pushes
/// the nearby entities took back from it, whether its spin attack ended, and the piston deltas
/// the moving pistons left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickSceneOutcome {
    pub result: TickResult,
    pub entity_pushes: Vec<EntityPush>,
    pub auto_spin_attack_ended: bool,
    pub piston_deltas: [f64; 3],
}

/// One client tick of the local player in its scene: the attached rockets and the entities the
/// client ticks before it boost and push it, `tick` moves it (colliding with the entities that are
/// colliders), the spin attack is checked (LivingEntity.aiStep after travel), the rockets and
/// entities after it boost and push it, then the moving pistons push it with the colliders where
/// the entities' ticks left them. The world's entity colliders are left empty.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick_local_player_scene(
    world: &mut WorldSnapshot,
    state: &mut SimulationState,
    movement_speed: Option<&mut MovementSpeedAttribute>,
    nearby_entities: &[NearbyEntity],
    (rockets, rockets_before_player): (u32, u32),
    auto_spin_attack: bool,
    moving_pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
    release_use_key: impl FnOnce(&mut SimulationState, &WorldSnapshot) -> Result<(), String>,
    tick: impl FnOnce(&mut SimulationState, &WorldSnapshot) -> Result<TickResult, String>,
) -> Result<TickSceneOutcome, String> {
    let moved = move_local_player(
        world,
        state,
        movement_speed,
        nearby_entities,
        (rockets, rockets_before_player),
        auto_spin_attack,
        release,
        release_use_key,
        tick,
    )?;
    // Player.tick ends with the pose update, after aiStep and before the level ticks the next
    // entity (1.13.2 Player.java:292, 338; 1.14.4 :224-225 ff.; 26.2 Player.java:242, 266), so
    // those meet the box of the new pose, as the block entities do. The player's own tick's height
    // comes back for what follows, which is the caller's pose update.
    let height = state.height;
    let bounding_box = state.bounding_box;
    let after_pose = state.height_after_tick.filter(|after| *after != height);
    if let Some(after) = after_pose {
        set_pose_height(state, after);
    }
    let refitted = state.bounding_box;
    let mut pushes = moved.entity_pushes;
    pushes.extend(finish_local_player(
        world,
        state,
        nearby_entities,
        rockets - rockets_before_player.min(rockets),
        moving_pistons,
        piston_deltas,
        config,
        release,
    ));
    if after_pose.is_some() {
        state.height = height;
        // the box as aiStep left it, which refreshDimensions (setBounds before 1.14) then rebuilt
        // from its corner: the caller's pose update rebuilds it again. A box the entities after the
        // player moved keeps that move, with the kept box's own size.
        if let (Some(bounds), Some(kept)) = (state.bounding_box.as_mut(), bounding_box) {
            if Some(*bounds) == refitted {
                *bounds = kept;
            } else {
                bounds.max = bounds.min + (kept.max - kept.min);
            }
        }
    }
    phase::emit(Phase::ClientTickEnd, state);
    Ok(TickSceneOutcome {
        result: moved.result,
        entity_pushes: pushes,
        auto_spin_attack_ended: moved.auto_spin_attack_ended,
        piston_deltas: *piston_deltas,
    })
}

/// The height the pose update leaves, on the state and the box kept before 1.17. A changed size
/// rebuilds the kept box from its corner with the float width and the new height
/// (Entity#refreshDimensions, 1.14.4 Entity.java:2260-2261, 1.16.5 :2326-2327; Entity#setBounds
/// before 1.14, 1.12.2 Entity.java:392-393): the player's poses never narrow it, so the centred
/// branch never runs. An unchanged size leaves the box (setBounds and the pose's data only call
/// it on a change). The recordings' end-of-tick box is this rebuilt one (1.16.5 walk/sneak_br t=20:
/// maxX = minX + 0.6F exactly, where aiStep's box was 1.1e-16 narrower); their afterAiStep box is
/// the one before it, as the pose update follows aiStep (tick_local_player_scene).
pub(crate) fn set_pose_height(state: &mut SimulationState, height: f64) {
    let changed = height != state.height;
    state.height = height;
    if !changed {
        return;
    }
    if let Some(bounds) = state.bounding_box.as_mut() {
        let width = f64::from((state.half_width * 2.0) as f32);
        *bounds = Aabb::new(bounds.min, bounds.min + Vec3::new(width, height, width));
    }
}

/// tick_local_player_scene up to Player.tick's pose update: the rockets and entities before the
/// player, its move and the spin attack's check. A caller that decides the pose from where the
/// move left the player sets it, then runs finish_local_player.
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_local_player(
    world: &mut WorldSnapshot,
    state: &mut SimulationState,
    movement_speed: Option<&mut MovementSpeedAttribute>,
    nearby_entities: &[NearbyEntity],
    (rockets, rockets_before_player): (u32, u32),
    auto_spin_attack: bool,
    release: MinecraftVersion,
    release_use_key: impl FnOnce(&mut SimulationState, &WorldSnapshot) -> Result<(), String>,
    tick: impl FnOnce(&mut SimulationState, &WorldSnapshot) -> Result<TickResult, String>,
) -> Result<TickSceneOutcome, String> {
    let rockets_before_player = rockets_before_player.min(rockets);
    state.minor_horizontal_collision_supported = release.supports_minor_horizontal_collision();
    // the use key's release comes first: Minecraft.tick handles the keys before the level ticks
    // its entities, the rockets and pushers before the player among them (D-214: 1.13.2
    // Minecraft.java:1115, 1146; 1.16.5 :1420, 1443; 26.3 :1786, 1797, 1930)
    if state.riptide_launch.is_some() {
        release_use_key(state, world)?;
    }
    let released_early = state.riptide_released;
    boost_by_attached_rockets_before_player(state, rockets_before_player, release);
    let pushes = push_by_nearby_entities(state, world, nearby_entities, true, release);
    world.set_entity_colliders(nearby_entity_colliders(nearby_entities, false));
    // the box where the move starts: before 1.17 the kept box refitted to this tick's pose, as
    // Entity.setPose -> refreshDimensions already refitted it when the pose changed (1.16.5
    // LivingEntity.java:2502 takes getBoundingBox() of the new size), and past the lift of a
    // riptide launch, which TridentItem.releaseUsing moved before aiStep took it (the use key is
    // handled before the level ticks: 1.16.5 Minecraft.java:1420, 1587, TridentItem.java:95)
    let before = kept_bounding_box(state);
    let result = tick(state, world);
    world.set_entity_colliders(Vec::new());
    let result = result?;
    // (a release in the tick lifts the player after the box was taken)
    let before = if released_early { before } else { before.translated(state.riptide_lift) };
    if let Some(attribute) = movement_speed {
        apply_frost(state, world, attribute, release);
    }
    // LivingEntity.aiStep checks the spin attack after travel, before the pushes (1.12.2
    // LivingEntity.java:1865-1868 has none; 1.13.2 LivingEntity.java:1955-1963 adds it)
    let auto_spin_attack_ended = auto_spin_attack
        && release.at_least(1, 13, 0)
        && check_auto_spin_attack(state, before, nearby_entities);
    phase::emit(Phase::AfterAiStep, state);
    Ok(TickSceneOutcome {
        result,
        entity_pushes: pushes,
        auto_spin_attack_ended,
        piston_deltas: [0.0; 3],
    })
}

/// LivingEntity.aiStep's removeFrost and tryAddFrost after travel: on every side before 1.21.5,
/// which makes them the server's (1.17 LivingEntity.java:2555-2556, 505-527; 1.21.4 :2785-2786,
/// 509-531; 1.21.5 :2763-2772). The attribute packet carries the server's modifier a tick after
/// the client made its own where the frozen ticks start or the landing block turns to air, so the
/// value comes from the parts. A modifier that did not change leaves the value the caller gave.
fn apply_frost(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    attribute: &mut MovementSpeedAttribute,
    release: MinecraftVersion,
) {
    if !release.at_least(1, 17, 0) || (state.client_side && release.at_least(1, 21, 5)) {
        return;
    }
    let frost = (state.ticks_frozen > 0 && !landing_block_is_air(state, world, release)).then(|| {
        // Entity.getFreezingScale, float ticks over the required int, and -0.05F of it
        // (1.17 Entity.java:2064-2067, LivingEntity.java:517)
        let required = state.ticks_required_to_freeze.max(140);
        let scale = state.ticks_frozen.min(required) as f32 / required as f32;
        f64::from(-0.05_f32 * scale)
    });
    if frost != attribute.frost {
        attribute.frost = frost;
        state.movement_speed_value = Some(attribute.value());
    }
}

/// Entity.getLandingBlockState().isAir() (1.17 Entity.java:1123-1125, 26.3 getBlockStateOnLegacy).
fn landing_block_is_air(
    state: &SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> bool {
    let support = kept_supporting_block(world, state, None, release);
    world.is_air_at(landing_position(world, state, support, release))
}

/// tick_local_player_scene after Player.tick's pose update: the rockets and entities after the
/// player, then the moving pistons with the colliders where the entities' ticks left them.
/// Returns the pushes the entities took back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_local_player(
    world: &mut WorldSnapshot,
    state: &mut SimulationState,
    nearby_entities: &[NearbyEntity],
    rockets_after_player: u32,
    moving_pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) -> Vec<EntityPush> {
    boost_by_attached_rockets(state, rockets_after_player, release);
    let pushes = push_by_nearby_entities(state, world, nearby_entities, false, release);
    if !moving_pistons.is_empty() {
        world.set_entity_colliders(nearby_entity_colliders(nearby_entities, true));
        push_by_moving_pistons(state, world, moving_pistons, piston_deltas, config, release);
        world.set_entity_colliders(Vec::new());
    }
    pushes
}

/// Everything one client tick of the local player reads, which executing it leaves as it was: the
/// state, the world, the runtime config, the release's rules and fall-damage policy, and the
/// scene around the player (nearby entities, attached rockets, the spin attack, moving pistons).
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
    pub scene: TickScene,
}

/// Everything one client tick of the local player writes: the state after it, the move's result,
/// the pushes the nearby entities took back, whether the spin attack ended, the piston deltas the
/// moving pistons left, and the scene the next tick reads (the pistons moved on, the rockets'
/// tick counted off: TickScene::advance).
#[derive(Clone, Debug, PartialEq)]
pub struct TickOutputSnapshot {
    pub state: SimulationState,
    pub result: TickResult,
    pub entity_pushes: Vec<EntityPush>,
    pub auto_spin_attack_ended: bool,
    pub piston_deltas: [f64; 3],
    pub scene: TickScene,
}

impl TickInputSnapshot {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
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
            scene: TickScene::default(),
        }
    }

    /// The snapshot of a release's tick as its versioned engine runs it: tick_with_profile, with
    /// the profile's own restitution and fall-damage policy.
    #[must_use]
    pub fn for_profile<P: VersionPhysicsProfile>(
        state: SimulationState,
        world: WorldSnapshot,
        config: PhysicsConfig,
    ) -> Self {
        let rules = P::rules();
        Self::new(
            state,
            world,
            config,
            rules,
            P::landing_damage_rounding(),
            P::restitution_enabled(),
            P::modern_fall_damage(),
            rules.water_movement_efficiency_attribute,
        )
    }

    #[must_use]
    pub fn with_scene(mut self, scene: TickScene) -> Self {
        self.scene = scene;
        self
    }

    /// The snapshot of the tick after the one `output` came from: its state and scene, in the
    /// same world, config and release.
    #[must_use]
    pub fn next(&self, output: &TickOutputSnapshot) -> Self {
        Self {
            state: output.state,
            scene: output.scene.clone(),
            ..self.clone()
        }
    }

    #[must_use]
    pub fn execute(&self) -> TickOutputSnapshot {
        let mut state = self.state;
        let mut world = self.world.clone();
        let mut rules = self.rules;
        rules.water_movement_efficiency_attribute = self.water_movement_efficiency_attribute;
        let mut scene = self.scene.clone();
        let outcome = tick_scene(
            &mut world,
            &mut state,
            &mut scene,
            self.config,
            rules.release,
            |state, world| {
                let fall_rules = FallDamageRules {
                    landing_damage_rounding: self.landing_damage_rounding,
                    restitution_enabled: self.restitution_enabled,
                    modern_fall_damage: self.modern_fall_damage,
                };
                release_riptide(state, world, rules, fall_rules);
                state.riptide_released = true;
                Ok(())
            },
            |state, world| {
                Ok(tick_with_rules_and_rounding_and_restitution(
                    state,
                    world,
                    self.config,
                    rules,
                    self.landing_damage_rounding,
                    self.restitution_enabled,
                    self.modern_fall_damage,
                ))
            },
        )
        .expect("the snapshot's tick does not fail");
        TickOutputSnapshot {
            state,
            result: outcome.result,
            entity_pushes: outcome.entity_pushes,
            auto_spin_attack_ended: outcome.auto_spin_attack_ended,
            piston_deltas: outcome.piston_deltas,
            scene,
        }
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
    // (1.21.11 Entity.java:3638-3645; 26.1 EntityFluidInteraction.java:180-187)
    let current = if release.at_least(26, 1, 0) {
        if contact.flowing_blocks == 0 || contact.flow.length_squared() < 9.999999747378752e-6 {
            return;
        }
        if state.is_player {
            average()
        } else {
            contact.flow.normalized(release)
        }
    } else {
        if !(contact.flow.length_in(release) > 0.0) {
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
            current.normalized(release)
        }
    };
    let mut push = current.scale(scale);
    // (1.15.2 Entity.java:2610; 1.16 Entity.java:2649)
    if release.at_least(1, 16, 0)
        && state.velocity.x.abs() < 0.003
        && state.velocity.z.abs() < 0.003
        && push.length_in(release) < 0.0045000000000000005
    {
        push = push.normalized(release).scale(0.0045000000000000005);
    }
    state.velocity = state.velocity + push;
}

pub(crate) fn water_contact(
    world: &WorldSnapshot,
    bounds: Aabb,
    release: MinecraftVersion,
) -> FluidContact {
    // (1.13.2 Entity.java:2511-2554; 1.14.4 Entity.java:2705-2748)
    if release.at_least(1, 14, 0) {
        world.fluid_contact(FluidState::Water, bounds, release)
    } else {
        legacy_fluid::fluid_contact_1_13(world, FluidState::Water, bounds)
    }
}

/// Entity's fluid query with its current vehicle context. Before 26.1 a boat excludes water
/// independently of lava; the modern shared scan adjusts both, retaining the original depth
/// origin (26.3 EntityFluidInteraction.java:57, 91). (1.21.11 Entity.java:1566-1575 drops water
/// in a boat before its own scan; 26.1 Entity.java:1572-1575 scans through fluidInteraction)
pub(crate) fn entity_fluid_contact(
    state: &SimulationState,
    world: &WorldSnapshot,
    fluid: FluidState,
    release: MinecraftVersion,
) -> FluidContact {
    if release.at_least(26, 1, 0) {
        return state.fluid_interaction_area().map_or_else(FluidContact::default, |area| {
            world.fluid_contact_in_area(fluid, area, state.bounds().min.y, release)
        });
    }
    if fluid == FluidState::Water {
        if state.boat_excludes_water(release) {
            FluidContact::default()
        } else {
            water_contact(world, state.bounds(), release)
        }
    } else {
        world.fluid_contact(fluid, state.bounds(), release)
    }
}

/// Eye-fluid queries use the original eye position even when the boat changes the scan. The eye is
/// at 0.85 of the height where the caller gives none (1.8.8 Entity.java:1817, 26.3 EntityDimensions.java:16).
/// Before 1.16 any boat excludes the query; 1.16/1.16.1 use AABB.contains, and 1.16.2+
/// only inclusive vertical bounds (exact cached Entity.updateFluidOnEyes bytecode).
/// (1.8.8 Entity.java:907-912 has no boat test; 1.9.4 Entity.java:1013-1015 returns false in
/// any boat, as 1.15.2 Entity.java:1111-1113 still does; 1.16 Entity.java:1069-1075 tests the
/// boat's box at the eye less 0.11111111F; 1.20.4 Entity.java:1227-1229 still lowers it, 1.20.5
/// Entity.java:1276-1279 tests the eye itself)
pub(crate) fn entity_eyes_in_fluid(
    state: &SimulationState,
    world: &WorldSnapshot,
    fluid: FluidState,
    release: MinecraftVersion,
) -> bool {
    let eye_height = state.eye_height.unwrap_or(state.height * 0.85);
    let eyes = (
        state.position.x,
        state.position.y + java_float(eye_height),
        state.position.z,
    );
    // (1.21.11 Entity.java:1602; 26.1 EntityFluidInteraction.java:71)
    if release.at_least(26, 1, 0) {
        return state.fluid_interaction_area().is_some_and(|area| {
            world.eyes_in_fluid_in_area(fluid, area, eyes, release)
        });
    }
    if release.at_least(1, 9, 0) {
        if let Some(boat) = state.boat_vehicle.filter(|_| state.passenger) {
            if !release.at_least(1, 16, 0) {
                return false;
            }
            let sample_y = if release.at_least(1, 20, 5) {
                eyes.1
            } else {
                eyes.1 - f64::from(0.11111111_f32)
            };
            // (1.16.1 Entity.java:1074; 1.16.2 Entity.java:1095)
            let inside_boat = if release.at_least(1, 16, 2) {
                boat.bounding_box.min.y <= sample_y && boat.bounding_box.max.y >= sample_y
            } else {
                boat.bounding_box.contains(Vec3::new(eyes.0, sample_y, eyes.2))
            };
            if !boat.underwater && inside_boat {
                return false;
            }
        }
    }
    world.eyes_in_fluid(fluid, state.bounds(), eyes, release)
}

/// Entity.move's `inLava = false; checkBlockCollision()` 1.14 - 1.16.1: a lava block of the box
/// (above its foot in 1.16 - 1.16.1, FluidBlock.onEntityCollision) sets it (1.14.4
/// Entity.java:567-569, 1.16 :660-662); from 1.16.2 the base tick's lava height does.
/// (1.16.1 Entity.java:660-662 still clears it before checkInsideBlocks; 1.16.2 Entity.java:691-692
/// no longer does, and Entity.java:1178-1179 reads the lava height)
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

/// What the fluid travel handlers need from the tick once the move is done: the fluid the
/// entity travels in, its lava height against getFluidJumpThreshold, the water slow-down picked
/// before the move, the pre-move gravity and falling flag, and the Y the travel started at.
pub(crate) struct FluidTravelAfterMove {
    pub fluid: FluidState,
    pub fluid_height: f64,
    pub fluid_jump_threshold: f64,
    pub water_slowdown: f32,
    pub gravity: f64,
    pub going_down: bool,
    pub start_y: f64,
}

/// The fluid travel handler a profile selects with `fluid_travel_split`: LivingEntity.travel's
/// inline water and lava branches until 1.21.1 (1.21.1 LivingEntity.java:2126-2177), and the
/// travelInFluid method travel calls from 1.21.2 (1.21.2 LivingEntity.java:2202-2203, 2241-2293).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidTravelHandler {
    InlineTravel,
    TravelInFluid,
}

impl FluidTravelHandler {
    #[must_use]
    pub fn for_rules(rules: VersionPhysicsRules) -> Self {
        if rules.fluid_travel_split {
            Self::TravelInFluid
        } else {
            Self::InlineTravel
        }
    }

    pub(crate) fn after_move(
        self,
        state: &mut SimulationState,
        world: &WorldSnapshot,
        rules: VersionPhysicsRules,
        after_move: &FluidTravelAfterMove,
    ) {
        match self {
            Self::InlineTravel => legacy_fluid_travel_after_move(state, world, rules, after_move),
            Self::TravelInFluid => travel_in_fluid_after_move(state, world, rules, after_move),
        }
    }
}

/// LivingEntity.travel's inline fluid branches after the move, through 1.21.1: in water the
/// 0.2 climb on a horizontal collision (from 1.14), the water slow-down and 0.8F vertical drag
/// with getFluidFallingAdjustedMovement (before 1.13 the literal 0.02 sink), in lava 0.5 with the
/// shallow-lava branch from 1.16 and gravity / 4, then the 0.3 hop (1.21.1
/// LivingEntity.java:2146-2177, 1.12.2 LivingEntity.java:1448).
pub(crate) fn legacy_fluid_travel_after_move(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    rules: VersionPhysicsRules,
    after_move: &FluidTravelAfterMove,
) {
    let legacy_fluids = legacy_fluid::applies(rules.release);
    let in_water = matches!(after_move.fluid, FluidState::Water | FluidState::Waterlogged);
    // (1.13.2 LivingEntity.java:1669-1672 moves and drags with no climb; 1.14.4
    // LivingEntity.java:1880-1882 sets 0.2 on a ladder after a horizontal collision)
    if in_water
        && state.collided_horizontally
        && rules.release.at_least(1, 14, 0)
        && climbable(world, state, rules.release)
    {
        state.velocity.y = 0.2;
    }
    fluid_horizontal_drag(state, rules, after_move);
    if legacy_fluids && after_move.fluid == FluidState::Water {
        legacy_fluid::sink_in_water(state, rules.release);
    } else {
        fluid_travel_vertical(state, rules, after_move);
    }
    fluid_travel_hop(state, world, rules, after_move, legacy_fluids);
}

/// LivingEntity.travelInFluid after the move, from 1.21.2: in water the 0.2 climb, the water
/// slow-down and 0.8 vertical drag with getFluidFallingAdjustedMovement, otherwise (lava) 0.5 with
/// the shallow-lava branch and -gravity / 4 while the effective gravity is not 0, then the 0.3 hop
/// out of the fluid (1.21.2 LivingEntity.java:2264-2291).
pub(crate) fn travel_in_fluid_after_move(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    rules: VersionPhysicsRules,
    after_move: &FluidTravelAfterMove,
) {
    if matches!(after_move.fluid, FluidState::Water | FluidState::Waterlogged)
        && state.collided_horizontally
        && climbable(world, state, rules.release)
    {
        state.velocity.y = 0.2;
    }
    fluid_horizontal_drag(state, rules, after_move);
    fluid_travel_vertical(state, rules, after_move);
    fluid_travel_hop(state, world, rules, after_move, false);
}

/// The horizontal part of the fluid drag after the move: the water slow-down, or lava's 0.5
/// (vanilla: LivingEntity#travel, LivingEntity#travelInFluid).
fn fluid_horizontal_drag(
    state: &mut SimulationState,
    rules: VersionPhysicsRules,
    after_move: &FluidTravelAfterMove,
) {
    let drag = if after_move.fluid == FluidState::Lava {
        java_float(rules.lava_drag)
    } else {
        f64::from(after_move.water_slowdown)
    };
    state.velocity.x *= drag;
    state.velocity.z *= drag;
}

/// The vertical part of the fluid drag after the move, with the falling adjustment and lava's
/// gravity (vanilla: LivingEntity#travel, LivingEntity#travelInFluid).
fn fluid_travel_vertical(
    state: &mut SimulationState,
    rules: VersionPhysicsRules,
    after_move: &FluidTravelAfterMove,
) {
    apply_fluid_travel_vertical(
        state,
        after_move.fluid,
        after_move.fluid_height,
        after_move.fluid_jump_threshold,
        rules,
        after_move.gravity,
        after_move.going_down,
    );
}

/// The 0.3 hop out of a fluid: isFree(dx, dy + 0.6 - getY() + startY, dz) after a horizontal
/// collision (before 1.13 Level.doesBoxCollide and isAnyLiquid, legacy_fluid::is_free).
fn fluid_travel_hop(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    rules: VersionPhysicsRules,
    after_move: &FluidTravelAfterMove,
    legacy_fluids: bool,
) {
    if !state.collided_horizontally {
        return;
    }
    let hop_bounds = state.bounds().translated(Vec3::new(
        state.velocity.x,
        state.velocity.y + 0.6000000238418579 - state.position.y + after_move.start_y,
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
            // 1.13 tests velocityY <= 0.0 here, after the move and the drag (1.13.2
            // LivingEntity.java:1674); from 1.14 the flag is taken before travel (1.14
            // LivingEntity.java:1935)
            let falling = if rules.release.at_least(1, 13, 0) && !rules.release.at_least(1, 14, 0) {
                state.velocity.y <= 0.0
            } else {
                going_down
            };
            if fluid_falling_gravity_enabled(state, rules, state.sprinting) {
                state.velocity.y = fluid_falling_adjusted_y(
                    gravity,
                    rules.water_gravity_divisor,
                    falling,
                    state.velocity.y,
                );
            }
        }
        FluidState::Lava => {
            // (1.15.2 LivingEntity.java:1945; 1.16 LivingEntity.java:1996-2002)
            if rules.release.at_least(1, 16, 0) && fluid_height <= fluid_jump_threshold {
                state.velocity.y *= java_float(rules.fluid_vertical_drag);
                if fluid_falling_gravity_enabled(state, rules, state.sprinting) {
                    state.velocity.y = fluid_falling_adjusted_y(
                        gravity,
                        rules.water_gravity_divisor,
                        going_down,
                        state.velocity.y,
                    );
                }
            } else {
                state.velocity.y *= 0.5;
            }
            // while the effective gravity is not 0 from 1.20.5 (1.20.4 LivingEntity.java:2103,
            // 1.20.5 :2164, 26.3 :2473)
            // (NoGravity from 1.10: 1.9.4 LivingEntity.java:1444 takes 0.02 off whatever the flag,
            // 1.10.2 tests isNoGravity first)
            let gravity_adds = (!state.no_gravity || !rules.release.at_least(1, 10, 0))
                && (gravity != 0.0 || !rules.release.at_least(1, 20, 5));
            if gravity_adds {
                state.velocity.y -= gravity / rules.lava_gravity_divisor;
                // Vec3.add(0.0, -gravity / 4.0, 0.0): its 0.0 turns the -0.0 a wall's restitution
                // leaves from 26.2 into 0.0 (26.3 LivingEntity.java:2474, Entity.java:867-872)
                state.velocity.x += 0.0;
                state.velocity.z += 0.0;
            }
        }
        FluidState::Empty => {}
    }
}

/// From 1.20.2 Entity.checkInsideBlocks visits no block once the entity is dead (`if
/// (!this.isAlive()) return;` per block, 1.20.2 Entity.java:982-984; the same test in the swept form,
/// 1.21.2 - 1.21.5, and `return false` from 1.21.6, 1.21.6 Entity.java:1154-1156); before, a dead
/// entity is walked like any other (1.20.1 Entity.java checkInsideBlocks has no such test).
fn visits_no_blocks_dead(state: &SimulationState, release: MinecraftVersion) -> bool {
    state.dead && release.at_least(1, 20, 2)
}

pub(crate) fn blocks_inside(
    state: &SimulationState,
    release: MinecraftVersion,
) -> Vec<(i32, i32, i32)> {
    if visits_no_blocks_dead(state, release) {
        return Vec::new();
    }
    // (1.19.2 Entity.java:889; 1.19.3 Entity.java:906)
    let inset = if !release.at_least(1, 19, 3) {
        0.001
    } else if !release.at_least(1, 21, 2) {
        1.0e-7
    } else {
        f64::from(1.0e-5_f32)
    };
    let inside = state.bounds().deflate(Vec3::new(inset, inset, inset));
    let min = block_position(inside.min, release);
    let max = block_position(inside.max, release);
    let mut blocks = Vec::new();
    // (1.21.1 Entity.java:1046-1053 insets 1.0E-7 and walks x, y, z outermost first; 1.21.2
    // Entity.java:1093 deflates 9.999999747378752E-6, BlockGetter.java:169 BlockPos.betweenClosed,
    // BlockPos.java:396-401 walks x innermost)
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
/// (cobwebs and bushes have the full inside shape: 1.21.4 Entity.java:1096-1124, 1.21.11
/// BlockBehaviour.java:312-314; powder snow its collision shape for the entity from 1.21.5, which
/// blocks_inside_moves tests).
pub(crate) fn stuck_speed_multiplier_inside(
    state: &SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    release: MinecraftVersion,
) -> Option<Vec3> {
    let in_powder_snow = world.effect_at(block_position(state.position, release)) == BlockEffect::PowderSnow;
    inside
        .iter()
        .filter_map(|&(position, _)| match world.effect_at(position) {
            _ if state.stuck_immune => None,
            BlockEffect::Web if state.web_immune => None,
            BlockEffect::Web if state.weaving => Some(Vec3::new(0.5, 0.25, 0.5)),
            BlockEffect::Web => Some(Vec3::new(0.25, java_float(0.05), 0.25)),
            BlockEffect::SweetBerryBush | BlockEffect::SweetBerrySapling
                if !state.sweet_berry_bush_immune =>
            {
                Some(Vec3::new(java_float(0.8), 0.75, java_float(0.8)))
            }
            // (1.17 PowderSnowBlock.java:54-56, 1.21.11 :59-61)
            BlockEffect::PowderSnow if in_powder_snow || state.boat || state.non_living => {
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
    let below = block_below_affecting_movement(world, state, support, release);
    let feet = block_position(state.position, release);
    let at_position = factor_at(feet);
    // the water block and the bubble column keep their own factor: the block below is not read
    // (1.15 Entity.java:681-689, 1.21.11 :1026-1034, 26.3 :1131-1139)
    let feet_in_water = world.block_cow(feet).water_block
        || matches!(
            world.effect_at(feet),
            BlockEffect::BubbleUp | BlockEffect::BubbleDown
        );
    let factor = if at_position == 1.0 && !feet_in_water {
        factor_at(below)
    } else {
        at_position
    };
    // (1.20.6 LivingEntity.java:469-470 returns 1 on a soul speed block; 1.21
    // LivingEntity.java:475-476 lerps by the movement efficiency attribute)
    if release.at_least(1, 21, 0) {
        factor + state.movement_efficiency as f32 * (1.0 - factor)
    // (1.15.2 Entity.java:681-689; 1.16 LivingEntity.java:647-654)
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
/// the block at floor(y - 0.2F) (1.16 LivingEntity.java:650-652, Entity.java:683-698; its fence
/// fallback finds a fence, wall or gate, never a soul speed block), from 1.16.2 the block below
/// that affects the movement (1.16.5 LivingEntity.java:661-663). (1.16.1 LivingEntity.java:650-651
/// reads getBlockStateOn; 1.16.2 LivingEntity.java:661-662 the block below)
fn soul_speed_position(
    state: &SimulationState,
    below: BlockPosition,
    release: MinecraftVersion,
) -> BlockPosition {
    if release.at_least(1, 16, 2) {
        below
    } else {
        block_position(state.position - Vec3::new(0.0, f64::from(0.2_f32), 0.0), release)
    }
}

pub(crate) fn block_below_affecting_movement(
    world: &WorldSnapshot,
    state: &SimulationState,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> BlockPosition {
    let below = |offset: f64| {
        (
            floor_i32_for(release, state.position.x),
            floor_i32_for(release, state.position.y - offset),
            floor_i32_for(release, state.position.z),
        )
    };
    // (1.14.4 LivingEntity.java:1828; 1.15 Entity.java:692)
    if !release.at_least(1, 15, 0) {
        return below(1.0);
    }
    // (1.19.4 Entity.java:771 floors minY - 0.5000001; 1.20 Entity.java:798 getOnPos(0.500001F))
    if !release.at_least(1, 20, 0) {
        return below(0.5000001);
    }
    on_position(world, state, support, 0.500001, release)
}

/// Entity.getLandingPos and getOnPosLegacy, getOnPos(0.2F) from 1.20: before, the block at
/// floor(y - 0.2F), or the fence, wall or gate under it when that is air (1.8.8 Entity.java:644-652,
/// 1.14.4 :508-515, 1.19.4 :738-750; up to 1.13.2 by class, then by tag).
pub(crate) fn landing_position(
    world: &WorldSnapshot,
    state: &SimulationState,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> BlockPosition {
    if release.at_least(1, 20, 0) {
        return on_position(world, state, support, 0.2, release);
    }
    let landing = block_position(state.position - Vec3::new(0.0, f64::from(0.2_f32), 0.0), release);
    let under = (landing.0, landing.1 - 1, landing.2);
    if world.is_air_at(landing) && world.block_cow(under).tags & FENCE_LIKE != 0 {
        under
    } else {
        landing
    }
}

/// Entity.getOnPos(offset) (getPosWithYOffset before 1.21.5) from 1.20: the height in the supporting
/// block's column, but the supporting block itself when it is a wall or gate, or a fence for an
/// offset up to 0.5 (1.20 Entity.java:804-821, 1.21.4 :892-907, 26.3 :1109-1124).
pub(crate) fn on_position(
    world: &WorldSnapshot,
    state: &SimulationState,
    support: Option<BlockPosition>,
    offset: f32,
    release: MinecraftVersion,
) -> BlockPosition {
    let below = block_position(state.position - Vec3::new(0.0, f64::from(offset), 0.0), release);
    let Some(block) = support else {
        return below;
    };
    let tags = world.block_cow(block).tags;
    if tags & WALL_OR_GATE != 0 || (f64::from(offset) <= 0.5 && tags & FENCE_LIKE != 0) {
        block
    } else {
        (block.0, below.1, block.2)
    }
}

/// Entity.mainSupportingBlockPos (1.20+, checkSupportingBlock) of an entity that keeps none: the
/// block under an entity on the ground that supports it, else, after a move that leaves the box
/// over nothing, the one under the box where the move started.
pub(crate) fn main_supporting_block(
    world: &WorldSnapshot,
    state: &SimulationState,
    movement: Option<Vec3>,
    release: MinecraftVersion,
) -> Option<BlockPosition> {
    if !state.on_ground {
        return None;
    }
    let area = supporting_block_area(state);
    let context = entity_collision_context(state);
    world
        .supporting_block_position(area, state.position, &context, release)
        .or_else(|| {
            let movement = movement?;
            let start = area.translated(Vec3::new(-movement.x, 0.0, -movement.z));
            world.supporting_block_position(start, state.position, &context, release)
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
    release: MinecraftVersion,
) -> Option<BlockPosition> {
    if state.supporting_block_kept {
        state.supporting_block
    } else {
        main_supporting_block(world, state, movement, release)
    }
}

/// Entity.checkSupportingBlock (1.20+) after a move: the block found under the box; with none,
/// unless the last check found none either (onGroundNoBlocks), the one under the box where the
/// move started; with no move to retry it, the block kept before. Off the ground, none.
pub(crate) fn check_supporting_block(
    world: &WorldSnapshot,
    state: &mut SimulationState,
    movement: Option<Vec3>,
    release: MinecraftVersion,
) -> Option<BlockPosition> {
    if state.on_ground {
        let area = supporting_block_area(state);
        let context = entity_collision_context(state);
        let mut found = world.supporting_block_position(area, state.position, &context, release);
        if found.is_some() || state.on_ground_no_blocks {
            state.supporting_block = found;
        } else if let Some(movement) = movement {
            let start = area.translated(Vec3::new(-movement.x, 0.0, -movement.z));
            found = world.supporting_block_position(start, state.position, &context, release);
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
    // before 1.15 stepOn sits inside canClimb(), which is !flying for a player (1.8.9
    // Entity.java:668, 677, Player.java:1578-1580; 1.12.2 Entity.java:830, 839, Player.java:1649-1651;
    // 1.14 Entity.java:602, 610); 1.15 asks onGround alone (1.15 Entity.java:589-591)
    let steps_on = release.at_least(1, 15, 0) || !(state.is_player && state.flying);
    if state.on_ground && steps_on {
        match world.effect_at(landing_position(world, state, support, release)) {
            BlockEffect::Slime if !stepping_carefully && state.velocity.y.abs() < 0.1 => {
                let step_factor = 0.4 + state.velocity.y.abs() * 0.2;
                state.velocity.x *= step_factor;
                state.velocity.z *= step_factor;
            }
            // MagmaBlock.stepOn: no fall-flying test; Frost Walker spares the boots' wearer, in
            // the block before 1.21 (1.20.5 MagmaBlock.java:29) and by the enchantment's
            // damage immunity to #burn_from_stepping from 1.21 (1.21.11 Enchantments.java:139-139);
            // a hot floor is fire to what is immune to it (1.10.2 MagmaBlock.java, 26.3 hurtServer)
            BlockEffect::Magma
                if !stepping_carefully && !state.frost_walker && takes_fire_damage(state) =>
            {
                deal_contact_damage(state, 1.0);
            }
            _ => {}
        }
    }
    apply_inside_block_effects(state, world, release, inside);
}

/// The entityInside part of apply_block_effects (Entity.checkInsideBlocks without the stepOn of
/// the block under the entity): the stuck speed, bubble columns, soul sand before 1.15 and honey.
pub(crate) fn apply_inside_block_effects(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    inside: &[((i32, i32, i32), bool)],
) {
    if state.no_physics {
        return;
    }
    if !(state.is_player && state.flying) {
        if let Some(multiplier) = stuck_speed_multiplier_inside(state, world, inside, release) {
            state.stuck_speed_multiplier = multiplier;
            state.fall_distance = 0.0;
        }
    }
    // (1.21.1 Entity.java:2435, 2447; 1.21.2 Player.java:330-342)
    let pushed_by_bubbles = !(release.at_least(1, 21, 2) && state.is_player && state.flying);
    for &(position, precise) in inside {
        match world.effect_at(position) {
            effect @ (BlockEffect::BubbleUp | BlockEffect::BubbleDown)
                if pushed_by_bubbles && precise =>
            {
                let drag_down = effect == BlockEffect::BubbleDown;
                push_by_bubble_column(state, world, position, drag_down, release);
            }
            // (1.14.4 SoulsandBlock.java:26; 1.15 Blocks.java:597)
            BlockEffect::SoulSand if !release.at_least(1, 15, 0) => {
                state.velocity.x *= 0.4;
                state.velocity.z *= 0.4;
            }
            BlockEffect::Honey => {
                slide_down_honey(state, position, release);
            }
            _ => {}
        }
    }
}

/// HoneyBlock.entityInside: an entity sliding down the side of the honey block at `position`
/// slides at -0.05 a tick (doSlideMovement), its fall reset. Any entity the block's effects reach,
/// not only a living one (1.15 HoneyBlock.java:68-75, 26.3 HoneyBlock.java:55-62, 73-97); the slide
/// keeps -0.05 until 1.21.1 and from 1.21.2 is the old delta, -0.05 less 0.08 times 0.98F (1.21.1
/// HoneyBlock.java:91-95, 1.21.2 HoneyBlock.java:69-71, 99). Whether it
/// slid.
pub(crate) fn slide_down_honey(
    state: &mut SimulationState,
    position: (i32, i32, i32),
    release: MinecraftVersion,
) -> bool {
    if !sliding_down_honey(state, position, release) {
        return false;
    }
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
    state.velocity = Vec3::new(state.velocity.x * scale, slide, state.velocity.z * scale);
    state.fall_distance = 0.0;
    true
}

/// The signals of the blocks the move leaves the entity inside, the list apply_block_effects
/// takes (Entity.checkInsideBlocks on the box after the move, deflated by 0.001, 1e-7 from 1.19.3,
/// the swept blocks from 1.21.2: 1.19.2 Entity.java:891-894, 1.19.3 :904-907, 1.21.4 :1072-1100),
/// and LivingEntity.aiStep's freezing after it (1.17 LivingEntity.java:2543-2559, 1.21.11
/// :2977-2987): Entity.isInPowderSnow, the cobweb, the contact damage of cactus, fire and lit
/// campfires (none under Frost Walker: 1.20.5 CampfireBlock.java:102, the enchantment's damage
/// immunity from 1.21; none of the fire ones to a fire-immune entity: 1.17 CampfireBlock.java:89,
/// BaseFireBlock.java:125-131, 1.21.11 Entity.java:2973) and the freeze ticks, counted up in
/// powder snow and down by two outside it. From 1.21.5 the steps' InsideBlockEffectType.FREEZE
/// counts them up (apply_inside_fire_effects) and aiStep only takes two off outside powder snow
/// (1.21.5 LivingEntity.java:2767-2768). Fire and lava clear them through the fire ticks:
/// base_tick_fire before 1.21.5, the ignitions and CLEAR_FREEZE from 1.21.5.
/// A block's hurt is the server's: the local player's returns false (1.8.8
/// LocalPlayer.java:88-90, 1.20.4 :149-151, 1.21 :152-154), any living entity's does on a
/// client (1.8.8 LivingEntity.java:673-675, 1.20.4 :1096-1098), and from 1.21.2 the blocks call a
/// server-only damage (1.21.3 MagmaBlock.java:30, 26.3 Entity.java:1969-1975).
fn deal_contact_damage(state: &mut SimulationState, amount: f64) {
    if !state.client_side {
        state.contact_damage += amount;
    }
}

/// Whether fire damage hurts the entity: EntityType.fireImmune and Fire Resistance refuse it in
/// LivingEntity.hurt (1.8.8 LivingEntity.java:683, 26.3 :1154).
fn takes_fire_damage(state: &SimulationState) -> bool {
    !state.fire_immune && !state.fire_resistance
}

/// The horizontal length of a motion (Vec3.horizontalDistance).
fn horizontal_speed(velocity: Vec3) -> f64 {
    (velocity.x * velocity.x + velocity.z * velocity.z).sqrt()
}

/// LivingEntity's flight into a wall on the server, from 1.9: `speed`, the motion's before the
/// glide changed it, less the motion Entity.move left, ten times over less 3, as a float, when
/// the move ended in a horizontal collision and that is above 0 (1.9.4 LivingEntity.java:1335,
/// 1363-1371; 1.20.4 :2144; 1.21.3 :2295-2303, 2333-2341; 26.3 :2504-2510, 2546-2553). The client
/// computes none of it, and 1.8.9 has no such damage source.
fn deal_wall_damage(state: &mut SimulationState, speed: f64, release: MinecraftVersion) {
    if state.client_side || !state.collided_horizontally || !release.at_least(1, 9, 0) {
        return;
    }
    let damage = ((speed - horizontal_speed(state.velocity)) * 10.0 - 3.0) as f32;
    if damage > 0.0 {
        state.wall_damage = f64::from(damage);
    }
}

fn apply_inside_block_signals(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    moved: Vec3,
    release: MinecraftVersion,
) {
    let effects: Vec<BlockEffect> = inside
        .iter()
        .map(|&(position, _)| world.effect_at(position))
        .collect();
    state.in_web = effects.contains(&BlockEffect::Web);
    state.in_powder_snow = effects.contains(&BlockEffect::PowderSnow);
    if effects.contains(&BlockEffect::Cactus) {
        deal_contact_damage(state, 1.0);
    }
    // SweetBerryBushBlock.entityInside: a grown bush hurts a living entity that moved 0.003F or
    // more along x or z (1.14.4 SweetBerryBushBlock.java:60-65, 1.21 :78-83; the known movement
    // from 1.21.5, 1.21.5 :83-91, 26.3 :80-88)
    let pricked = f64::from(0.003_f32);
    if effects.contains(&BlockEffect::SweetBerryBush)
        && !state.sweet_berry_bush_immune
        && (moved.x.abs() >= pricked || moved.z.abs() >= pricked)
    {
        deal_contact_damage(state, 1.0);
    }
    let burns = takes_fire_damage(state);
    if effects.contains(&BlockEffect::Campfire) && !state.frost_walker && burns {
        deal_contact_damage(state, 1.0);
    }
    if effects.contains(&BlockEffect::SoulCampfire) && !state.frost_walker && burns {
        deal_contact_damage(state, 2.0);
    }
    if effects.contains(&BlockEffect::Fire) && burns {
        deal_contact_damage(state, 1.0);
    }
    if effects.contains(&BlockEffect::SoulFire) && burns {
        deal_contact_damage(state, 2.0);
    }
    let required_to_freeze = state.ticks_required_to_freeze.max(140);
    if state.client_side {
        // the client keeps the count the server's entity data gives it (LivingEntity.aiStep's "freezing" is the
        // server's: 1.17 LivingEntity.java:2543, 1.20.4 :2624, 1.21.5 :2765)
    } else if release.at_least(1, 21, 5) {
        if !state.in_powder_snow || state.powder_snow_immune {
            state.ticks_frozen = state.ticks_frozen.saturating_sub(2);
        }
    } else if state.in_powder_snow && !state.powder_snow_immune {
        state.ticks_frozen = state.ticks_frozen.saturating_add(1).min(required_to_freeze);
    } else {
        // only the count up is clamped: the count down is max(0, i - 2), so ticks over the limit
        // (a command's, the server's) fall by two (1.17 LivingEntity.java:2546-2552, 1.21.5
        // :2767-2768)
        state.ticks_frozen = state.ticks_frozen.saturating_sub(2);
    }
    // the server's alone (1.17 LivingEntity.java:2554, 1.21.11 :2981-2988, 26.3 :3074-3087)
    if !state.client_side
        && (state.tick_count as i32) % 40 == 0
        && state.ticks_frozen >= required_to_freeze
        && !state.powder_snow_immune
    {
        state.freeze_damage += 1.0;
    }
}

/// Entity.getFireImmuneTicks: 20 for a player, for any other entity 1 up to 1.21.5 and 0 from
/// 1.21.6 (1.17 Player.java:433-435, Entity.java:176; 1.21.5 Player.java:486-488,
/// Entity.java:3276-3278; 1.21.6 Entity.java:3464-3466).
pub(crate) fn fire_immune_ticks(is_player: bool, release: MinecraftVersion) -> i32 {
    if is_player {
        20
    } else if release.at_least(1, 21, 6) {
        0
    } else {
        1
    }
}

/// Entity.remainingFireTicks until a state sets it: -getFireImmuneTicks up to 1.21.5 (1.17
/// Entity.java:176, 1.21.5 :212), the field's 0 from 1.21.6 (1.21.6 Entity.java:234).
fn fire_ticks(state: &SimulationState, release: MinecraftVersion) -> i32 {
    state
        .remaining_fire_ticks
        .unwrap_or(if release.at_least(1, 21, 6) {
            0
        } else {
            -fire_immune_ticks(state.is_player, release)
        })
}

/// Entity.setRemainingFireTicks; a player's is at most 1 while invulnerable from 1.16
/// (1.16 Player.java:1910, 1.20.4 :1870-1871, 1.21.5 Player.java, 1.21.11 Player.java).
fn set_fire_ticks(state: &mut SimulationState, ticks: i32, release: MinecraftVersion) {
    let ticks = if state.is_player && state.invulnerable && release.at_least(1, 16, 0) {
        ticks.min(1)
    } else {
        ticks
    };
    state.remaining_fire_ticks = Some(ticks);
}

/// Entity.isOnFire: not fire-immune, with burning ticks left or, on a client, the server's flag
/// (1.8.8 Entity.java:1450-1453, 1.20.4 :2127-2130, 26.3 :2779-2782): a client that cleared its
/// ticks in lava still burns for the wet test after the move.
fn on_fire(state: &SimulationState, release: MinecraftVersion) -> bool {
    !state.fire_immune
        && (fire_ticks(state, release) > 0 || state.client_side && state.on_fire_flag)
}

/// Entity.extinguish / clearFire: setRemainingFireTicks(0) up to 1.21.8, so a fire-free entity's
/// negative ticks go to 0 too (1.21.4 Entity.java:604-606, 1.21.5 :581-583, 1.21.8 :641-643),
/// and min(0, ticks) from 1.21.9 (1.21.9 Entity.java:648-650).
fn clear_fire(state: &mut SimulationState, release: MinecraftVersion) {
    let ticks = if release.at_least(1, 21, 9) {
        fire_ticks(state, release).min(0)
    } else {
        0
    };
    set_fire_ticks(state, ticks, release);
}

/// Entity.setOnFireForTicks / igniteForTicks of a LivingEntity: the duration less 15% a level of
/// Fire Protection before 1.21 (1.17 ProtectionEnchantment.java:59-66, 1.20.5
/// LivingEntity.java:3520-3522), times the BURNING_TIME attribute from 1.21 (1.21
/// LivingEntity.java:3555-3557, 1.21.11 :3818-3820), kept when the ticks left are more; from
/// 1.21.5 the ignition clears the freeze ticks (1.21.5 Entity.java:598-604).
fn ignite_for_ticks(state: &mut SimulationState, ticks: i32, release: MinecraftVersion) {
    let ticks = if release.at_least(1, 21, 0) {
        ceil_i32_for(release, f64::from(ticks) * state.burning_time.unwrap_or(1.0))
    } else if state.fire_protection > 0 {
        ticks - floor_f32_for(release, ticks as f32 * (state.fire_protection as f32 * 0.15_f32))
    } else {
        ticks
    };
    if fire_ticks(state, release) < ticks {
        set_fire_ticks(state, ticks, release);
    }
    if release.at_least(1, 21, 5) {
        state.ticks_frozen = 0;
    }
}

/// A fire block's ignition (BaseFireBlock.entityInside, BaseFireBlock.fireIgnite from
/// 1.21.5) of an entity that is not fire-immune: before 1.21.2 a tick more, and eight seconds of
/// fire when that reaches 0 (1.17 BaseFireBlock.java:124-132, 1.21 :121-127); from 1.21.2 a
/// tick more while the ticks are negative, else for a ServerPlayer alone nextInt(1, 3) more (the
/// state's server_fire_draws or server_fire_increment), and eight seconds of fire from 0 on (1.21.3
/// BaseFireBlock.java:121-133, 1.21.11 BaseFireBlock.java:123-137, 26.3 :122-136).
/// (1.21.1 BaseFireBlock.java:119-123 adds a tick, igniting at 0; 1.21.2 BaseFireBlock.java:122-131
/// draws for a ServerPlayer; the 160 is igniteForSeconds(8.0F) at 1.21.1 Entity.java:554-555)
fn fire_ignite(state: &mut SimulationState, release: MinecraftVersion) {
    if state.fire_immune {
        return;
    }
    if !release.at_least(1, 21, 2) {
        set_fire_ticks(state, fire_ticks(state, release) + 1, release);
        if fire_ticks(state, release) == 0 {
            ignite_for_ticks(state, 160, release);
        }
        return;
    }
    if fire_ticks(state, release) < 0 {
        set_fire_ticks(state, fire_ticks(state, release) + 1, release);
    } else if state.is_player && !state.client_side {
        let increment = match state.server_fire_draws.as_mut() {
            Some(draws) => draws.next().unwrap_or_else(|| {
                state.server_fire_draws_exhausted = true;
                1
            }),
            None => match state.server_fire_increment {
                Some(increment) => increment.clamp(1, 2),
                // vanilla draws here: with no draw given there is nothing to read, and a 1 would pass for one
                None => {
                    state.server_fire_draw_missing = true;
                    1
                }
            },
        };
        set_fire_ticks(state, fire_ticks(state, release) + increment, release);
    }
    if fire_ticks(state, release) >= 0 {
        ignite_for_ticks(state, 160, release);
    }
}

/// Entity.setOnFireFromLava / lavaIgnite: fifteen seconds of fire to an entity that is not
/// fire-immune (1.17 Entity.java:485-492, 1.21.5 :571-575).
fn lava_ignite(state: &mut SimulationState, release: MinecraftVersion) {
    if !state.fire_immune {
        ignite_for_ticks(state, 300, release);
    }
}

/// Entity.isWet (isInWaterRainOrBubble): in water, in the rain or in a bubble column at the block
/// position (1.17 Entity.java:1017-1032); the rain is the caller's in_rain.
fn is_wet(state: &SimulationState, world: &WorldSnapshot, release: MinecraftVersion) -> bool {
    state.in_water
        || state.in_rain
        || matches!(
            world.effect_at(block_position(state.position, release)),
            BlockEffect::BubbleUp | BlockEffect::BubbleDown
        )
}

/// The fire part of the server's Entity.baseTick and LivingEntity.baseTick, from 1.17 where the
/// fire ticks first reach a signal (the freeze ticks), with the fluid flags this tick's base tick
/// measured: water extinguishes before 1.21.5 (Entity.checkWaterState, 1.17 Entity.java:1059-1070);
/// burning ticks count down, four at a time for a fire-immune entity until 1.21.9 and cleared at
/// once from it (1.17 Entity.java:419-439, 1.21.8 :497-510, 1.21.9 / 26.3 :551-563), and before
/// 1.21.5 clear the freeze ticks (1.21.4 Entity.java:502-505); lava sets the entity on fire before
/// 1.21.5 (setOnFireFromLava, 1.21.4 Entity.java:512-515), its inside effect from 1.21.5; then
/// LivingEntity.baseTick puts a fire-immune entity out, and before 1.21.5 a wet one (1.17
/// LivingEntity.java:345-347, 385-391; 1.21.4 :383, 442-448; 1.21.5 :362-365).
pub(crate) fn base_tick_fire(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let modern = release.at_least(1, 21, 5);
    if !modern && release.at_least(1, 17, 0) && state.in_water {
        clear_fire(state, release);
    }
    if state.client_side {
        // Entity.baseTick on the client: no burning down, the ticks are cleared (1.17 Entity.java:419-420, 1.21.4
        // and 1.21.5 the else of `level() instanceof ServerLevel`, 1.21.9 the same)
        clear_fire(state, release);
    } else if fire_ticks(state, release) > 0 {
        if !state.fire_immune {
            set_fire_ticks(state, fire_ticks(state, release) - 1, release);
        } else if release.at_least(1, 21, 9) {
            clear_fire(state, release);
        } else {
            set_fire_ticks(state, fire_ticks(state, release) - 4, release);
            if fire_ticks(state, release) < 0 {
                clear_fire(state, release);
            }
        }
        if !modern {
            state.ticks_frozen = 0;
        }
    }
    if !modern && state.in_lava {
        lava_ignite(state, release);
    }
    if state.boat {
        return;
    }
    // LivingEntity.baseTick after Entity's: a fire-immune entity, and up to 1.21.8 any on the client, is cleared
    // (1.17 LivingEntity.java:350-351, 1.21.5 :362-363, 1.21.8 the same; gone from 1.21.9)
    let client_clears = state.client_side && !release.at_least(1, 21, 9);
    if state.fire_immune || client_clears || (!modern && is_wet(state, world, release)) {
        clear_fire(state, release);
    }
}

/// One InsideBlockEffectApplier.StepBasedCollector step's effects that reach the fire and freeze
/// ticks, applied in InsideBlockEffectType's order: FREEZE, CLEAR_FREEZE (1.21.9+), FIRE_IGNITE,
/// LAVA_IGNITE, EXTINGUISH (1.21.5 InsideBlockEffectType.java:6-16, 1.21.9 :6-17, 26.3 the same).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InsideEffectStep {
    pub freeze: bool,
    pub clear_freeze: bool,
    pub fire_ignite: bool,
    pub lava_ignite: bool,
    pub extinguish: bool,
}

/// What a cauldron's entityInside does at once in 1.21.5 - 1.21.8, while the blocks are visited
/// and before any step's effects: the lava cauldron's lavaIgnite and a water or powder snow
/// cauldron's clearFire of a burning entity, each with the entity inside its content (1.21.5
/// LavaCauldronBlock.java:31-36, LayeredCauldronBlock.java:64-71, AbstractCauldronBlock.java:49-51).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectInsideEffect {
    LavaIgnite,
    ExtinguishBurning,
}

/// The inside-block effects of a move's visit from 1.21.5: the direct ones in visit order, then
/// the steps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InsideEffects {
    pub direct: Vec<DirectInsideEffect>,
    pub steps: Vec<InsideEffectStep>,
    /// How many of the steps' lava cells run Entity.lavaHurt after LAVA_IGNITE, one each
    /// (LavaFluid.entityInside's runAfter, 1.21.5 LavaFluid.java:107-111, 26.3 :the same).
    pub lava_hurts: u32,
}

/// The content height of a cauldron: 0.9375 of lava, (6 + 3 * level) / 16 of a layered (water or
/// powder snow) cauldron (1.17 LavaCauldronBlock.java, LayeredCauldronBlock.java; 1.21.9
/// LavaCauldronBlock.java:32-34, LayeredCauldronBlock.java:55-61).
fn cauldron_content(effect: BlockEffect) -> Option<f64> {
    match effect {
        BlockEffect::LavaCauldron => Some(0.9375),
        BlockEffect::LayeredCauldron(level) => Some((6.0 + f64::from(level) * 3.0) / 16.0),
        _ => None,
    }
}

/// AbstractCauldronBlock.isEntityTouchingFluid / isEntityInsideContent: the entity's feet below the
/// content and its box above a quarter of the block (1.17 AbstractCauldronBlock.java:52-54, 1.21.5
/// :49-51).
fn inside_cauldron_content(
    state: &SimulationState,
    world: &WorldSnapshot,
    position: (i32, i32, i32),
) -> bool {
    cauldron_content(world.effect_at(position)).is_some_and(|content| {
        let floor = f64::from(position.1);
        state.position.y < floor + content && state.bounds().max.y > floor + 0.25
    })
}

/// A block of #fire or Blocks.LAVA, the blocks that keep the fire ticks after a move before 1.21.5
/// (1.17 Entity.java:633-635, 1.21.4 :804).
pub(crate) fn fire_or_lava_block(world: &WorldSnapshot, position: (i32, i32, i32)) -> bool {
    matches!(
        world.effect_at(position),
        BlockEffect::Fire | BlockEffect::SoulFire
    ) || world.block_cow(position).fluid == FluidState::Lava
}

/// The fire test after Entity.move before 1.17, in the box shrunk by 0.001. 1.16: each fire block
/// ignites (fire_ignite), then with no fire and no lava there ticks not above 0 go back to
/// -getFireImmuneTicks, and a wet burning entity does too (1.16.5 Entity.java:698-707). Before:
/// fire or lava in the box raises the ticks unless the entity is in water (eight seconds of fire
/// on reaching 0), else the same reset (1.8.8 Entity.java:706-722, 1.12.2 :875-890, 1.15 :630-646).
pub(crate) fn legacy_inside_fire(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    reset_ticks: i32,
    release: MinecraftVersion,
) {
    let area = state.bounds().deflate(Vec3::new(0.001, 0.001, 0.001));
    let (min, max) = (block_position(area.min, release), block_position(area.max, release));
    let touches_fire = (min.0..=max.0).any(|x| {
        (min.1..=max.1).any(|y| (min.2..=max.2).any(|z| fire_or_lava_block(world, (x, y, z))))
    });
    // (1.15.2 Entity.java:635-653; 1.16 Entity.java:672-680, BaseFireBlock.java:126-137)
    if release.at_least(1, 16, 0) {
        for &(position, _) in inside {
            if matches!(world.effect_at(position), BlockEffect::Fire | BlockEffect::SoulFire) {
                fire_ignite(state, release);
            }
        }
        if !touches_fire && fire_ticks(state, release) <= 0 {
            set_fire_ticks(state, reset_ticks, release);
        }
        if on_fire(state, release) && is_wet(state, world, release) {
            set_fire_ticks(state, reset_ticks, release);
        }
        return;
    }
    // (isInWaterRainOrBubble: 1.15.2 Entity.java:635, 1.13.2 :849; a fire-immune entity's ticks rise
    // as well, 1.15.2 :636-641)
    let wet = is_wet(state, world, release);
    if touches_fire {
        if !wet {
            set_fire_ticks(state, fire_ticks(state, release) + 1, release);
            if fire_ticks(state, release) == 0 {
                ignite_for_ticks(state, 160, release);
            }
        }
    } else if fire_ticks(state, release) <= 0 {
        set_fire_ticks(state, reset_ticks, release);
    }
    // Entity.move asks for the ticks through 1.10.2 and for isOnFire from 1.11 (1.10.2
    // Entity.java:822, 1.11.2 :875)
    let burning = if release.at_least(1, 11, 0) {
        on_fire(state, release)
    } else {
        fire_ticks(state, release) > 0
    };
    if wet && burning {
        set_fire_ticks(state, reset_ticks, release);
    }
}

/// The fire and freeze effects of the blocks a move leaves the entity inside, from 1.17.
///
/// Before 1.21.5 (Entity.move's checkBlockCollision and the fire test after it, 1.17
/// Entity.java:630-648; tickBlockCollision from 1.21.2, 1.21.4 :788-823): each fire block ignites
/// (fire_ignite), a lava cauldron with the entity in its lava sets it on fire, a water or powder
/// snow cauldron puts a burning entity in it out (1.17 LavaCauldronBlock.java:26-30,
/// LayeredCauldronBlock.java:51-58); with no fire and no lava in the box shrunk by 1e-6 (the
/// blocks visited from 1.21.2) ticks not above 0 go back to -getFireImmuneTicks, and a burning
/// entity in powder snow or wet does too.
///
/// From 1.21.5 (Entity.applyEffectsFromBlocks, 1.21.5 Entity.java:837-862, 1.21.6 :867-891, 26.3
/// :915-941): the direct effects, then each step's (FREEZE, CLEAR_FREEZE, FIRE_IGNITE, LAVA_IGNITE,
/// EXTINGUISH), the rain's clearFire, and the reset to -getFireImmuneTicks: of an entity that
/// burned before and no longer does with no ticks left in 1.21.5, of any entity not burning that
/// no ignition raised this tick from 1.21.6.
pub(crate) fn apply_inside_fire_effects(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    effects: &InsideEffects,
    release: MinecraftVersion,
) {
    if state.no_physics {
        return;
    }
    let reset_ticks = -fire_immune_ticks(state.is_player, release);
    // (1.16.5 Entity.java:702; 1.17 Entity.java:635)
    if !release.at_least(1, 17, 0) {
        legacy_inside_fire(state, world, inside, reset_ticks, release);
        return;
    }
    if !release.at_least(1, 21, 5) {
        let mut in_powder_snow = false;
        let mut touches_fire = false;
        for &(position, _) in inside {
            match world.effect_at(position) {
                BlockEffect::Fire | BlockEffect::SoulFire => fire_ignite(state, release),
                BlockEffect::PowderSnow => in_powder_snow = true,
                BlockEffect::LavaCauldron if inside_cauldron_content(state, world, position) => {
                    lava_ignite(state, release);
                }
                BlockEffect::LayeredCauldron(_)
                    if on_fire(state, release) && inside_cauldron_content(state, world, position) =>
                {
                    clear_fire(state, release);
                }
                _ => {}
            }
            // (1.21.1 Entity.java:741; 1.21.2 Entity.java:790)
            if release.at_least(1, 21, 2) {
                touches_fire |= fire_or_lava_block(world, position);
            }
        }
        // a dead entity keeps its count from 1.21.2 (1.21.3 Entity.java:795, 1.21.4 :803)
        let alive = !(release.at_least(1, 21, 2) && state.dead);
        if !release.at_least(1, 21, 2) {
            // LevelReader.getBlockStatesIfLoaded: the cells from floor(min) to floor(max) (1.17
            // LevelReader.java:37-45)
            let area = state.bounds().deflate(Vec3::new(1.0e-6, 1.0e-6, 1.0e-6));
            let (min, max) = (block_position(area.min, release), block_position(area.max, release));
            touches_fire = (min.0..=max.0).any(|x| {
                (min.1..=max.1)
                    .any(|y| (min.2..=max.2).any(|z| fire_or_lava_block(world, (x, y, z))))
            });
        }
        if !touches_fire && alive && fire_ticks(state, release) <= 0 {
            set_fire_ticks(state, reset_ticks, release);
        }
        if on_fire(state, release) && (in_powder_snow || is_wet(state, world, release)) {
            set_fire_ticks(state, reset_ticks, release);
        }
        return;
    }
    let was_on_fire = on_fire(state, release);
    let previous_ticks = fire_ticks(state, release);
    for effect in &effects.direct {
        match effect {
            DirectInsideEffect::LavaIgnite => lava_ignite(state, release),
            DirectInsideEffect::ExtinguishBurning => {
                if on_fire(state, release) {
                    clear_fire(state, release);
                }
            }
        }
    }
    let required_to_freeze = state.ticks_required_to_freeze.max(140);
    for step in &effects.steps {
        if step.freeze {
            state.in_powder_snow = true;
            if !state.powder_snow_immune {
                state.ticks_frozen = state.ticks_frozen.saturating_add(1).min(required_to_freeze);
            }
        }
        if step.clear_freeze {
            state.ticks_frozen = 0;
        }
        if step.fire_ignite {
            fire_ignite(state, release);
        }
        if step.lava_ignite {
            lava_ignite(state, release);
        }
        if step.extinguish {
            clear_fire(state, release);
        }
    }
    if state.in_rain {
        clear_fire(state, release);
    }
    // (1.21.5 Entity.java:898; 1.21.6 Entity.java:925-926)
    let reset = if release.at_least(1, 21, 6) {
        // the server's alone from 1.21.6 (1.21.6 Entity.java:888, 26.3 :1028-1030)
        !state.client_side
            && !on_fire(state, release)
            && fire_ticks(state, release) <= previous_ticks
    } else {
        was_on_fire && !on_fire(state, release) && fire_ticks(state, release) <= 0
    };
    if reset {
        set_fire_ticks(state, reset_ticks, release);
    }
}

/// The blocks Entity.checkInsideBlocks gives entityInside from 1.21.2 and, from 1.21.5, the
/// effects of each step. From 1.21.5 a block passes when its getEntityInsideCollisionShape is
/// Shapes.block() or the move collides along with it, and its fluid's when the move collides
/// along with its AABB (1.21.5 Entity.java:1128-1156, 1.21.11 :1222-1255), with the release's
/// AABB.collidedAlongVector inset: powder snow's inside shape is its collision shape for the
/// entity, FALLING_COLLISION_SHAPE (0.9F high) past a fall of 2.5 (1.21.5 - 1.21.11
/// PowderSnowBlock.java:92-106, 26.3 :89-104), and from 1.21.6 a cauldron's is the cauldron with
/// its content (1.21.6 LavaCauldronBlock.java:37-39, 51; LayeredCauldronBlock.java:62-64, 123;
/// 1.21.5 has no override and tests isEntityInsideContent in entityInside, LavaCauldronBlock and
/// LayeredCauldronBlock.java:42-50). Each block's entityInside changes the entity on the way: a makeStuckInBlock, a bubble
/// column it is inside and a honey slide end the fall (1.21.11 Entity.java:2856-2871, 2920-2923,
/// HoneyBlock.java:106-115), which a later powder snow's shape reads.
pub(crate) fn blocks_inside_moves(
    state: &SimulationState,
    world: &WorldSnapshot,
    movements: &[Movement],
    release: MinecraftVersion,
) -> (Vec<((i32, i32, i32), bool)>, InsideEffects) {
    if visits_no_blocks_dead(state, release) {
        return (Vec::new(), InsideEffects::default());
    }
    if !release.at_least(1, 21, 5) {
        let inside = blocks_visited(
            movements,
            state.bounds(),
            state.half_width,
            state.height,
            release,
        );
        return (inside, InsideEffects::default());
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
    let inset = if release.at_least(1, 21, 9) {
        1.0e-7
    } else {
        0.0
    };
    let filled_cauldrons = release.at_least(1, 21, 6);
    // CLEAR_FREEZE of fire and a lava cauldron (1.21.8 BaseFireBlock and LavaCauldronBlock
    // entityInside have none; 1.21.9 BaseFireBlock.java:118-122, LavaCauldronBlock.java:38-41)
    let clears_freeze = release.at_least(1, 21, 9);
    // the entity as the blocks visited so far leave it, for the fall a powder snow shape reads
    let mut running = *state;
    let mut effects = InsideEffects::default();
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
            let collides = |boxes: &[Aabb]| moving.collided_along_vector(travel, boxes, inset);
            let effect = world.effect_at(visit.position);
            let shape_hit = match effect {
                BlockEffect::PowderSnow => {
                    running.fall_distance <= 2.5
                        || collides(&[Aabb::new(
                            corner,
                            corner + Vec3::new(1.0, java_float(0.9), 1.0),
                        )])
                }
                BlockEffect::LavaCauldron | BlockEffect::LayeredCauldron(_) if filled_cauldrons => {
                    // Block.column(12, 4, top): the content, to 15/16 of lava
                    let top = match effect {
                        BlockEffect::LayeredCauldron(level) => {
                            (6.0 + f64::from(level) * 3.0) / 16.0
                        }
                        _ => 15.0 / 16.0,
                    };
                    let mut boxes = world.block_collision_boxes(visit.position);
                    boxes.push(Aabb::new(
                        corner + Vec3::new(2.0 / 16.0, 4.0 / 16.0, 2.0 / 16.0),
                        corner + Vec3::new(14.0 / 16.0, top, 14.0 / 16.0),
                    ));
                    collides(&boxes)
                }
                _ => true,
            };
            // a block whose inside shape the move can miss (powder snow, a cauldron) holds no fluid
            if !shape_hit {
                return false;
            }
            let fluid = world.block_cow(visit.position).fluid;
            let fluid_hit = fluid != FluidState::Empty
                && collides(&[Aabb::new(
                    corner,
                    corner + Vec3::new(1.0, world.fluid_height_at(visit.position), 1.0),
                )]);
            if last_step != Some(visit.step) {
                last_step = Some(visit.step);
                effects.steps.push(InsideEffectStep::default());
            }
            let step = effects.steps.last_mut().expect("a step was just pushed");
            match effect {
                // PowderSnowBlock.entityInside: FREEZE and EXTINGUISH (1.21.5
                // PowderSnowBlock.java:79-93, 1.21.11 :86-100)
                BlockEffect::PowderSnow => {
                    step.freeze = true;
                    step.extinguish = true;
                }
                // BaseFireBlock: CLEAR_FREEZE from 1.21.9 and FIRE_IGNITE (1.21.5
                // BaseFireBlock.java:117-121, 1.21.9 :118-123)
                BlockEffect::Fire | BlockEffect::SoulFire => {
                    step.clear_freeze |= clears_freeze;
                    step.fire_ignite = true;
                }
                // LAVA_IGNITE and EXTINGUISH from 1.21.6 (1.21.6 LavaCauldronBlock.java:41-44,
                // LayeredCauldronBlock.java:66-78), CLEAR_FREEZE from 1.21.9 (1.21.9
                // LavaCauldronBlock.java:42-46)
                BlockEffect::LavaCauldron if filled_cauldrons => {
                    step.clear_freeze |= clears_freeze;
                    step.lava_ignite = true;
                }
                BlockEffect::LayeredCauldron(_) if filled_cauldrons => step.extinguish = true,
                BlockEffect::LavaCauldron
                    if inside_cauldron_content(state, world, visit.position) =>
                {
                    effects.direct.push(DirectInsideEffect::LavaIgnite);
                }
                BlockEffect::LayeredCauldron(_)
                    if inside_cauldron_content(state, world, visit.position) =>
                {
                    effects.direct.push(DirectInsideEffect::ExtinguishBurning);
                }
                _ => {}
            }
            // LavaFluid.entityInside: CLEAR_FREEZE from 1.21.9 and LAVA_IGNITE (1.21.5
            // LavaFluid.java:107-111, 1.21.9 :107-112); WaterFluid.entityInside: EXTINGUISH
            // (1.21.5 WaterFluid.java:73-76)
            if fluid_hit {
                if fluid == FluidState::Lava {
                    step.clear_freeze |= clears_freeze;
                    step.lava_ignite = true;
                    effects.lava_hurts += 1;
                } else {
                    step.extinguish = true;
                }
            }
            apply_inside_block_effects(
                &mut running,
                world,
                release,
                &[(visit.position, visit.precise)],
            );
            true
        },
    );
    (inside, effects)
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
    movements_this_tick_from(state, old_position, old_position, requested, moved, release)
}

/// movements_this_tick of a tick whose move started at `move_start`, where the tick set its
/// position before the move (a boat's floatBoat snap, an interpolation step): the recorded move
/// starts there, and only the tick's fallback from oldPosition (26.3 Entity.java:1003-1015,
/// 1.21.5 :809-822) starts at `old_position`.
pub(crate) fn movements_this_tick_from(
    state: &SimulationState,
    old_position: Vec3,
    move_start: Vec3,
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
    // (1.21.4 Entity.java:788-802; 1.21.5 Entity.java:707-720, 811-823)
    if !release.at_least(1, 21, 5) || state.no_physics || !recorded {
        return vec![whole_tick];
    }
    let to = move_start + moved;
    // (1.21.8 Entity.java:737, 1160; 1.21.9 Entity.java:750, 1183-1184)
    let axis_order = if release.at_least(1, 21, 9) {
        requested
    } else {
        moved
    };
    let mut movements = vec![Movement {
        from: move_start,
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

/// The Movement an Entity.move adds to movementThisTick from 1.21.5, when it moved or was not cut
/// short: from where it started by the collided movement, its axis order from the collided
/// movement, from 1.21.9 from the requested one (1.21.5 Entity.java:720-721, 1.21.9 :750-751, 26.3
/// :775-788). None before 1.21.5, under noPhysics or when the move did not record.
pub(crate) fn recorded_movement(
    state: &SimulationState,
    from: Vec3,
    requested: Vec3,
    moved: Vec3,
    release: MinecraftVersion,
) -> Option<Movement> {
    let moved_length = moved.length_squared();
    let recorded = moved_length > 1.0e-7 || requested.length_squared() - moved_length < 1.0e-7;
    if !release.at_least(1, 21, 5) || state.no_physics || !recorded {
        return None;
    }
    Some(Movement {
        from,
        to: from + moved,
        axis_order: Some(if release.at_least(1, 21, 9) { requested } else { moved }),
    })
}

/// Player simulates on its server as well as its authoritative client; Entity's default
/// AUTHORITATIVE_SIDE uses isLocalInstanceAuthoritative (26.2 Player.java:1200, Entity.java:3599;
/// 26.3 Player.java:1190, Entity.java:3721). Other 26.3 modes require the caller's exact result.
pub(crate) fn can_simulate_movement(state: &SimulationState, release: MinecraftVersion) -> bool {
    let inferred = (state.is_player && !state.client_side) || state.controlled_by_local_instance;
    if release.at_least(26, 3, 0) {
        state.can_simulate_movement.unwrap_or(inferred)
    } else {
        inferred
    }
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
    // (1.21.4 BubbleColumnBlock.java:53; 1.21.5 BubbleColumnBlock.java:52)
    let nothing_above = if release.at_least(1, 21, 5) {
        !world.has_collision_shape(above) && world.fluid_at(above) == FluidState::Empty
    } else {
        world.is_air_at(above)
    };
    if nothing_above && state.boat {
        // the boat's own onAboveBubbleColumn moves it on the server alone (1.16
        // Boat.java:202-212, 1.21.11 AbstractBoat.java:155-162, 26.3 :151-158)
        state.above_bubble_column = Some(drag_down);
        return;
    }
    // 26.3 pushes only an entity that simulates its movement, and the fall still ends in the
    // column (26.3 Entity.java:2974, 3006; 26.2 Entity.java:2883-2891, 2912-2921 has no test)
    if !release.at_least(26, 3, 0) || can_simulate_movement(state, release) {
        state.velocity.y = match (nothing_above, drag_down) {
            (true, true) => (state.velocity.y - 0.03).max(-0.9),
            (true, false) => (state.velocity.y + 0.1).min(1.8),
            (false, true) => (state.velocity.y - 0.03).max(-0.3),
            (false, false) => (state.velocity.y + 0.06).min(0.7),
        };
    }
    if !nothing_above {
        state.fall_distance = 0.0;
    }
}

fn honey_slide_falling(velocity_y: f64, release: MinecraftVersion) -> f64 {
    // (1.21.1 HoneyBlock.java:93; 1.21.2 HoneyBlock.java:69-71, 101-102)
    if release.at_least(1, 21, 2) {
        velocity_y / java_float(0.98) + 0.08
    } else {
        velocity_y
    }
}

/// HoneyBlock#isSlidingDown: off the ground, below the block's top, falling (from 1.21.2 by the
/// motion before this tick's gravity and drag) and over the block's edge.
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
/// blocks' query (Level.doesBoxCollide).
pub(crate) fn entity_collision_query(
    swept: Aabb,
    query: Aabb,
    max_step: f32,
    release: MinecraftVersion,
) -> Aabb {
    // (26.2 Entity.java:1156; 26.3 Entity.java:1187)
    if release.at_least(26, 3, 0) {
        swept
            .expand_towards(Vec3::new(0.0, f64::from(max_step), 0.0))
            .inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    // (1.13.2 Entity.java:676, 708; 1.14.4 Entity.java:696)
    } else if release.at_least(1, 14, 0) {
        swept.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    } else {
        query
    }
}

/// LivingEntity.onClimbable; from 1.21.4 Player.onClimbable is false while flying (1.21.4
/// Player.java:2053-2058, 26.3 Player.java:1876-1878; 1.21.3 has no override). From
/// 1.21.11 a fall-flying entity is not on a #can_glide_through block (1.21.11
/// LivingEntity.java:1646-1647, 26.3 :1671).
pub(crate) fn climbable(
    world: &WorldSnapshot,
    state: &SimulationState,
    release: MinecraftVersion,
) -> bool {
    !state.no_physics
        && !(state.is_player && state.flying && release.at_least(1, 21, 4))
        && (state.climbing
            || match world.effect_at(block_position(state.position, release)) {
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

/// Entity#setLocationFromBoundingbox: the box's horizontal centre at its bottom.
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
        // (1.8.8 LivingEntity.java:1180-1181; 1.9.4 LivingEntity.java:1419-1420)
        if release.at_least(1, 9, 0) {
            floor_i32_for(release, value)
        } else {
            value as i32
        }
    };
    world.is_chunk_loaded(block(position.x) >> 4, block(position.z) >> 4)
}

/// BlockPos.containing (the BlockPos of doubles before 1.19.4, through Vec3i's Mth.floor).
pub(crate) fn block_position(position: Vec3, release: MinecraftVersion) -> (i32, i32, i32) {
    (
        floor_i32_for(release, position.x),
        floor_i32_for(release, position.y),
        floor_i32_for(release, position.z),
    )
}

/// LivingEntity.getFluidFallingAdjustedMovement: `divisor` is the 16.0 of every 1.13+ release
/// (1.13.2 LivingEntity.java:1674-1677, 1.20.4 :2230-2233, 1.21.11 :2524-2527, 26.3 :2611-2614).
pub(crate) fn fluid_falling_adjusted_y(
    gravity: f64,
    divisor: f64,
    falling: bool,
    current_y: f64,
) -> f64 {
    if falling
        && (current_y - 0.005).abs() >= 0.003
        && (current_y - gravity / divisor).abs() < 0.003
    {
        -0.003
    } else {
        current_y - gravity / divisor
    }
}

/// The test LivingEntity#getFluidFallingAdjustedMovement makes before it adjusts: not sprinting,
/// and `!isNoGravity()` before 1.20.5, a gravity argument that is not 0 from 1.20.5.
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
    // (1.19.3 Player.java:513; 1.19.4 Player.java:2102)
    let sprinting = if release.at_least(1, 19, 4) {
        state.sprinting
    } else {
        state.was_sprinting
    };
    if !sprinting {
        java_float(0.02)
    // (1.19.3 Player.java:513; 1.19.4 Player.java:2102)
    } else if release.at_least(1, 19, 4) {
        f64::from(0.025999999_f32)
    // (1.18.1 Player.java:488; 1.18.2 Player.java:511)
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
    // (1.15.2 LivingEntity.java:2294-2307; 1.16 LivingEntity.java:2460-2483)
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

/// LivingEntity.jumpFromGround with the power of the jump: from 1.20.5 nothing up to 1.0E-5F jumps
/// (26.3 LivingEntity.java:2303-2312), and the sprint push multiplies a double, before a float.
/// LivingEntity.knockback on a delta movement. It runs on the side that decides a hit: a client hears of its own
/// player's by a velocity packet, so nothing in a tick calls this and a caller asks at the hit, with the
/// strength, the direction and the KNOCKBACK_RESISTANCE of the moment. `roll` is the random.nextDouble()
/// the resistance is rolled against up to 1.15 (none reads as 0). None when vanilla would draw a direction of
/// its own: from 1.21 one shorter than 1.0E-5F squared is replaced at random (1.21 LivingEntity.java:1463-1476,
/// 26.3 :1588-1600).
///
/// Six families by their sources: 1.8 pushes 0.4F whatever it is given, on all three axes (1.8.8
/// LivingEntity.java:816-831); 1.9 the strength, Y only on the ground, over a float length (1.9.4 :935-950,
/// 1.13.2 :1101-1117); 1.14 a Vec3 normalized, the Y capped by min (1.14.4 :1178-1183); 1.16 the resistance
/// scales a float strength instead of a roll (1.16 :1512-1518); 1.17 a double (1.17 :1376-1383, 1.20.5
/// :1473-1481); 1.21 the re-draw.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn living_knockback(
    velocity: Vec3,
    on_ground: bool,
    strength: f64,
    x: f64,
    z: f64,
    resistance: f64,
    roll: f64,
    release: MinecraftVersion,
) -> Option<Vec3> {
    // (1.15.2 LivingEntity.java:1380; 1.16 LivingEntity.java:1511)
    if !release.at_least(1, 16, 0) && roll < resistance {
        return Some(velocity);
    }
    // (1.13.2 LivingEntity.java:1101-1117; 1.14.4 LivingEntity.java:1178-1185)
    if !release.at_least(1, 14, 0) {
        let length = f64::from((x * x + z * z).sqrt() as f32);
        let legacy_cap = f64::from(java_float(0.4));
        let strength = if release.at_least(1, 9, 0) { f64::from(strength as f32) } else { legacy_cap };
        let mut out = Vec3::new(velocity.x / 2.0 - x / length * strength, velocity.y, velocity.z / 2.0 - z / length * strength);
        if on_ground || !release.at_least(1, 9, 0) {
            out.y = velocity.y / 2.0 + strength;
            if out.y > legacy_cap {
                out.y = legacy_cap;
            }
        }
        return Some(out);
    }
    // (1.16.5 LivingEntity.java:1522-1530; 1.17 LivingEntity.java:1376-1384)
    let power = if release.at_least(1, 17, 0) {
        strength * (1.0 - resistance)
    // (1.15.2 LivingEntity.java:1383; 1.16 LivingEntity.java:1511)
    } else if release.at_least(1, 16, 0) {
        f64::from((f64::from(strength as f32) * (1.0 - resistance)) as f32)
    } else {
        f64::from(strength as f32)
    };
    // (1.15.2 LivingEntity.java:1380; 1.16 LivingEntity.java:1510)
    if power <= 0.0 && release.at_least(1, 16, 0) {
        return Some(velocity);
    }
    if release.at_least(1, 21, 0) && x * x + z * z < f64::from(java_float(1.0e-5)) {
        return None;
    }
    // (Vec3.normalize: Mth.sqrt, a float, to 1.16.5 Vec3.java:79, Math.sqrt from 1.17 :54. Its 1.0E-5 of
    // 1.21.2 decides nothing here: 1.21 has returned for that already)
    let length = (x * x + z * z).sqrt();
    let length = if release.at_least(1, 17, 0) { length } else { f64::from(length as f32) };
    let (push_x, push_z) = if length < 1.0e-4 { (0.0, 0.0) } else { (x / length * power, z / length * power) };
    let y = if on_ground { (velocity.y / 2.0 + power).min(0.4) } else { velocity.y };
    Some(Vec3::new(velocity.x / 2.0 - push_x, y, velocity.z / 2.0 - push_z))
}

pub(crate) fn jump_from_ground(state: &mut SimulationState, power: f64, release: MinecraftVersion) {
    // (1.20.4 LivingEntity.java:2031-2033 always sets the power; 1.20.5 LivingEntity.java:2072-2073
    // skips a power at most 1.0E-5F)
    if release.at_least(1, 20, 5) && power as f32 <= 1.0e-5 {
        return;
    }
    // (1.21.1 LivingEntity.java:2081-2093; 1.21.2 LivingEntity.java:2160-2172)
    state.velocity.y = if release.at_least(1, 21, 2) {
        power.max(state.velocity.y)
    } else {
        power
    };
    if state.sprinting {
        let (x, z) = sprint_jump_push(state.yaw, release);
        state.velocity.x += x;
        state.velocity.z += z;
    }
}

pub(crate) use crate::math::{friction_influenced_speed, sprint_jump_push};

pub(crate) fn jump_power(
    state: &SimulationState,
    config: PhysicsConfig,
    release: MinecraftVersion,
    block_jump_factor: f32,
) -> f64 {
    let strength = state.jump_strength.unwrap_or(config.jump_velocity);
    let power = strength as f32 * block_jump_factor;
    let boost = state
        .jump_boost_amplifier
        .map_or(0.0, |amplifier| 0.1_f32 * (amplifier as f32 + 1.0));
    let boost_added_in_double =
        // (1.13.2 LivingEntity.java:1485-1489; 1.14.4 LivingEntity.java:1725-1731)
        // (1.17 LivingEntity.java:1965-1967 adds the boost in float; 1.17.1 LivingEntity.java:1973
        // adds getJumpBoostPower in double, as 1.19.4 LivingEntity.java:1975 still does; 1.20
        // LivingEntity.java:1994-1995 adds it in float inside getJumpPower)
        !release.at_least(1, 14, 0) || (release.at_least(1, 17, 1) && !release.at_least(1, 20, 0));
    if boost_added_in_double {
        f64::from(power) + f64::from(boost)
    } else {
        f64::from(power + boost)
    }
}

fn glide(state: &mut SimulationState, gravity: f64, release: MinecraftVersion) {
    let look = state.look;
    let look_horizontal = (look.x * look.x + look.z * look.z).sqrt();
    let horizontal_speed =
        (state.velocity.x * state.velocity.x + state.velocity.z * state.velocity.z).sqrt();
    // (1.21.1 LivingEntity.java:2187; 1.21.2 LivingEntity.java:2313)
    let look_factor = if release.at_least(1, 21, 2) {
        1.0
    } else {
        (look.length_in(release) / 0.4).min(1.0)
    };
    // (1.18.1 LivingEntity.java:2062; 1.18.2 LivingEntity.java:2082)
    let lift = if release.at_least(1, 18, 2) {
        let cos = crate::math::math_cos(state.pitch, release);
        cos * cos * look_factor
    } else {
        let cos = mth_cos(state.pitch, release);
        java_float(cos * cos * look_factor)
    };
    let mut velocity = state.velocity;
    // (1.12.2 LivingEntity.java:1464 the literal -0.08 + lift * 0.06; 1.13.2 LivingEntity.java:1532
    // the gravity d * (-1.0 + lift * 0.75))
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
    // (1.21.4 LocalPlayer.java:585-596, LivingEntity.java:2745-2746; 1.21.5 LocalPlayer.java:589-621)
    } else if release.at_least(1, 21, 5) {
        modified_input(input_move_vector(state, release), state)
    } else {
        scale_input(input_move_vector(state, release), 0.98)
    };
    Vec3::new(f64::from(strafe), state.input.y, f64::from(forward))
}

/// The player's Input.getMoveVector during the move: the keys normalized from 1.21.5, before it
/// the impulses LocalPlayer scaled for sneaking and using an item.
pub(crate) fn input_move_vector(state: &SimulationState, release: MinecraftVersion) -> [f32; 2] {
    let keys = [state.input.x as f32, state.input.z as f32];
    // (1.21.5 Vec2.java:49 normalized, 1.0E-4F)
    if release.at_least(1, 21, 5) {
        let length = (keys[0] * keys[0] + keys[1] * keys[1]).sqrt();
        return if length < 1.0e-4 {
            [0.0, 0.0]
        } else {
            [keys[0] / length, keys[1] / length]
        };
    }
    // (1.21.3 LocalPlayer.java:643; 1.21.4 LocalPlayer.java:642)
    let item_slows_first = release.at_least(1, 21, 4);
    let slowed_by_item = slowed_by_using_item(state);
    let mut input = keys;
    if slowed_by_item && item_slows_first {
        input = scale_input(input, 0.2);
    }
    if state.moving_slowly.unwrap_or(state.sneaking) {
        // (1.18.1 KeyboardInput.java:24 in double, 1.18.2 :32 in float, 1.19 LocalPlayer.java:738 by
        // the sneaking speed)
        input = if release.at_least(1, 19, 0) {
            scale_input(input, sneaking_speed(state))
        } else if release.at_least(1, 18, 2) {
            scale_input(input, 0.3)
        } else {
            input.map(|impulse| (f64::from(impulse) * 0.3) as f32)
        };
    }
    if slowed_by_item && !item_slows_first {
        input = scale_input(input, 0.2);
    }
    // LocalPlayer.aiStep undoes the sneak slowdown of a flying player, 1.9 - 1.14 (1.9.4
    // LocalPlayer.java:710-713, 1.14.4 :682-687); 1.8 only moves it down (1.8.8
    // LocalPlayer.java:594-601), and from 1.15 a flying player does not crouch (1.14.4
    // LocalPlayer.java:682-688 divides by 0.3D; 1.15 LocalPlayer.java:592-593 has no crouch in
    // flight, LocalPlayer.java:731-735 no division)
    if state.flying && state.sneaking && release.at_least(1, 9, 0) && !release.at_least(1, 15, 0) {
        input = input.map(|impulse| (f64::from(impulse) / 0.3) as f32);
    }
    input
}

/// LocalPlayer's item-use slowdown spares a passenger in every release: isUsingItem() && !hasVehicle()
/// (1.8.8 LocalPlayer.java:538, 1.13.2 :698, 1.14.4 :596, 1.20.4 :672, 1.21.4 :642), in
/// modifyInput from 1.21.5 (1.21.5 LocalPlayer.java:610, 26.3 :695).
fn slowed_by_using_item(state: &SimulationState) -> bool {
    state.using_item && !state.passenger
}

/// LocalPlayer#modifyInput with LocalPlayer#modifyInputSpeedForSquareMovement: 0.98F of the
/// move vector, the item-use and sneaking slow-downs, then the length stretched toward the
/// unit square, at most 1.
pub(crate) fn modified_input(move_vector: [f32; 2], state: &SimulationState) -> [f32; 2] {
    if move_vector == [0.0, 0.0] {
        return move_vector;
    }
    let mut input = scale_input(move_vector, 0.98);
    if slowed_by_using_item(state) {
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

/// Vec2#scale, in float.
fn scale_input(input: [f32; 2], factor: f32) -> [f32; 2] {
    [input[0] * factor, input[1] * factor]
}

/// The SNEAKING_SPEED attribute's value (vanilla: Attributes#SNEAKING_SPEED), its default 0.3
/// where the state holds none.
fn sneaking_speed(state: &SimulationState) -> f32 {
    state.sneaking_speed.map_or(0.3, |speed| speed as f32)
}

pub(crate) fn mth_sin(radians: f64, release: MinecraftVersion) -> f64 {
    // (1.21.10 Mth.java:51-53; 1.21.11 Mth.java:53-55)
    if release.at_least(1, 21, 11) {
        java_sin(radians)
    } else {
        java_sin_legacy(radians as f32)
    }
}

pub(crate) fn mth_cos(radians: f64, release: MinecraftVersion) -> f64 {
    // (1.21.10 Mth.java:55-57; 1.21.11 Mth.java:57-59)
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
    // (1.13.2 Entity.java:1147-1165; 1.14.4 Entity.java:1180-1190)
    if !release.at_least(1, 14, 0) {
        return relative_movement_in_float(input, speed as f32, yaw as f32);
    }
    let length_squared = input.x * input.x + input.y * input.y + input.z * input.z;
    if length_squared < 1.0e-7 {
        return Vec3::ZERO;
    }
    let direction = if length_squared > 1.0 {
        input.normalized(release)
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
/// 180.0F` (two float roundings) before 1.9 (1.8.8 Entity.java:937-938, AbstractHorse.java:1088),
/// `yaw * (float) (Math.PI / 180.0)` from 1.9 (1.9.4 Entity.java:1046-1047).
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

/// Entity.moveRelative's float form (1.8.8 Entity.java:928, 1.13.2 Entity.java:1149: 1.0E-4F).
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
    // (1.8.8 LivingEntity.java:1414-1424; 1.9.4 LivingEntity.java:1686-1696)
    if customized || release.at_least(1, 9, 0) {
        config.negligible_velocity
    } else {
        0.005
    }
}

/// LivingEntity#computeModifiedFriction: `clamp(1.0F - (1.0F - friction) * modifier, 0, 1)` in float.
pub(crate) fn compute_modified_friction(friction: f64, modifier: f64) -> f64 {
    f64::from((1.0 - (1.0 - friction as f32) * modifier as f32).clamp(0.0, 1.0))
}

/// The modifier attribute's value, its default 1.0 where the state holds none (vanilla:
/// Attributes#AIR_DRAG_MODIFIER, Attributes#FRICTION_MODIFIER).
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
        // (1.21.1 Entity.java:654; 1.21.2 Entity.java:682)
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
    libm::acos(dot / (input_length_squared * movement_length_squared).sqrt()) < 0.13962633907794952
}

/// Player.maybeBackOffFromEdge from 1.14 (Entity.move's sneak edge check before): a sneaking
/// player's move backs off while it would leave the ground below by the step height (the float
/// maxUpStep).
/// - 1.14 - 1.16.1: only on the ground, the whole box moved by the step down meets no collision
///   (client1.14.4 Entity.java:600-640, 1.16 Player.java:1030-1076).
/// - 1.16.2 - 1.18.2: also not flying, and above the ground: on it, or the whole box moved down by
///   what is left of the step after the fall distance (both floats) meets a collision (1.16.5
///   Player.java:1111-1114, 1.18 :1067-1070).
/// - 1.19 - 1.20.4: also not while the move rises (1.19 Player.java:1056-1061).
/// - 1.20.5 - 1.21.4: the boxes are the feet's slab from minY less the step and the float 1.0E-5F
///   (1.20.5 Player.java:1072-1079, 1.21.4 :1066-1073); the fall distance a float yet.
/// - 1.21.5 on: the slab is 1e-7 shorter than 1e-5 below and 1e-7 in all round, the fall distance
///   a double (1.21.5 Player.java:1019-1026, client26.3 :876-883).
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
    // (1.16.1 Player.java:1030-1031; 1.16.2 Player.java:1061-1062, 1111-1114)
    let checks_above_ground = release.at_least(1, 16, 2);
    if state.no_physics
        || !state.is_player
        || !state.sneaking
        || (checks_above_ground && state.flying)
        // (1.18.2 Player.java:1031; 1.19 Player.java:1058) `movement.y <= 0.0` to 1.20.4, which a NaN
        // fails, then `!(movement.y > 0.0)`, which it passes (1.20.4 Player.java:1047; 1.20.5 :1029)
        || (release.at_least(1, 19, 0) && delta.y > 0.0)
        || (release.at_least(1, 19, 0) && !release.at_least(1, 20, 5) && delta.y.is_nan())
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
        // (1.21.4 Player.java:1070-1073; 1.21.5 Player.java:1023-1027)
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
        // (1.21.4 Player.java:1067; 1.21.5 Player.java:1020)
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
/// Level.doesBoxCollide(entity, box) gives the blocks' boxes, the cells outside the border as
/// stone and the boxes of the entities that meet the query (1.8.8 Entity.java:489, 1.12.2
/// Entity.java:619 and Level.java:1161-1184, 1.13.2 Entity.java:632 through Level.java:1794-1797).
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
    // (1.10.2 Entity.java:578 moves the box by -1.0; 1.11.2 Entity.java:617 by -this.maxUpStep)
    let drop = if release.at_least(1, 11, 0) {
        f64::from(step_height as f32)
    } else {
        1.0
    };
    let bounds = state.bounds();
    let entity = collision_query_entity(state, context);
    // the box moved as it is, 1.13.x included: `getBoundingBox().offset(x, -stepHeight, z)` with
    // no deflate (1.13.2 Entity.java:632, 644, 653)
    let can_fall = |x: f64, z: f64| {
        let lowered = bounds.translated(Vec3::new(x, -drop, z));
        // (Level.doesBoxCollide before 1.13: a box of no height, one layer of snow, too)
        world.no_collision_for_entity(lowered, entity, release)
    };
    back_off_while(delta, can_fall)
}

/// The 0.05 steps by which a sneaking move backs off while it would leave the ground: on x, on z,
/// then on both while the diagonal move would (vanilla: Player#maybeBackOffFromEdge,
/// Entity#applySneaking in 1.14, Entity#move before).
fn back_off_while(delta: Vec3, can_fall: impl Fn(f64, f64) -> bool) -> Vec3 {
    // (no step takes a NaN to zero, and the walks below would not end)
    if delta.x.is_nan() || delta.z.is_nan() {
        return delta;
    }
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
/// The eye is at 0.85 of the height where the caller gives none (1.8.8 Entity.java:1817, 26.3 EntityDimensions.java:16).
/// Before 1.16 a suffocating block in a cell under any of 8 points 0.1 tall and 0.8 of the width
/// around the eye (Entity.java:1288-1308 in 1.8.9, :1563-1583 in 1.15); in 1.16 a suffocating
/// collision shape meeting a box 0.1 tall around the eye (:1632-1641 in 1.16.5), 1e-6 tall from
/// 1.17 (:1651-1659), or the world border the entity's box touches from inside; 1.18 tests each
/// block's shape moved to the eye instead of to its cell (client1.18.1 Entity.java:1698-1710);
/// from 1.18.2 each non-air suffocating block's shape at its
/// cell (1.18.2 Entity.java:1640, :1676-1694 in 1.19, client1.21.11 :2192-2203).
/// (1.17.1 Entity.java:1656-1658 walks getBlockCollisions with the entity; 1.18 Entity.java:1642-1653
/// reads the block states, each shape moved to the eye)
fn is_in_wall(state: &SimulationState, world: &WorldSnapshot, release: MinecraftVersion) -> bool {
    if state.no_physics || state.sleeping {
        return false;
    }
    let eye_height = state.eye_height.unwrap_or(state.height * 0.85) as f32;
    let width = (state.half_width * 2.0) as f32;
    if !release.at_least(1, 16, 0) {
        return (0..8).any(|corner: i32| {
            let offset = |bit: i32| ((corner >> bit) % 2) as f32 - 0.5;
            let y = state.position.y + f64::from(offset(0) * 0.1) + f64::from(eye_height);
            let x = state.position.x + f64::from(offset(1) * width * 0.8);
            let z = state.position.z + f64::from(offset(2) * width * 0.8);
            world.is_in_wall_block((
                floor_i32_for(release, x),
                floor_i32_for(release, y),
                floor_i32_for(release, z),
            ))
        });
    }
    let half_width = f64::from(width * 0.8) / 2.0;
    // (1.16.5 Entity.java:1639; 1.17 Entity.java:1657)
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
    // (the shape moved to the eye to 1.18.1, Entity.java:1707; to its cell from 1.18.2 :1640, 1.19 :1687)
    if release.at_least(1, 18, 0) && !release.at_least(1, 18, 2) {
        return world.collides_with_suffocating_block_moved_to(eye_box, eye, release);
    }
    // (1.16 - 1.17.1: getBlockCollisions with the entity offers the world border's shape first,
    // which the suffocation predicate does not filter)
    if !release.at_least(1, 18, 0) && world.entity_touches_world_border(state.bounds()) {
        return true;
    }
    // (1.16 - 1.17.1 walk getBlockCollisions, with its Shapes.block() shortcut; from 1.19 every
    // shape is joined with the eye box)
    world.collides_with_suffocating_block(eye_box, release, !release.at_least(1, 18, 0))
}

/// LocalPlayer.moveTowardsClosestSpace (checkInBlock before 1.16.2): from each corner of the box
/// shrunk to 0.35 of its width, a player inside a suffocating block moves at 0.1 toward the
/// nearest side of that block that is open. Until 1.16.1 a cell of the column is suffocating
/// whatever its collision shape (1.16 LocalPlayer.java:404-462, blocked() in the 1.16.1
/// bytecode); from 1.16.2 a suffocating collision shape meets the column (suffocatesAt, 1.16.5
/// :404-437). (1.13.2 LocalPlayer.java:395-447 checkInBlock, ported by
/// push_out_of_blocks_before_1_14; 1.14.4 LocalPlayer.java:366-413 tests each side with blocked.
/// 1.17.1 LocalPlayer.java:437-440 asks hasBlockCollision with the entity; 1.18
/// LocalPlayer.java:443-446 collidesWithSuffocatingBlock)
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
    // 1.16.2 - 1.17.1 ask with the entity, so the walk offers the world border's shape first, which the
    // suffocation predicate does not filter, as in is_in_wall: every column suffocates and no side
    // is open (1.16.5 LocalPlayer.java:433-437, CollisionSpliterator.java:101-114; 1.18 asks
    // canCollide). 1.16 - 1.16.1 read the cells alone.
    let at_border = release.at_least(1, 16, 2)
        && !release.at_least(1, 18, 0)
        && world.entity_touches_world_border(bounds);
    let suffocates_at = |x: i32, z: i32| {
        if at_border {
            true
        } else if release.at_least(1, 16, 2) {
            let column = Aabb::new(
                Vec3::new(f64::from(x), bounds.min.y, f64::from(z)),
                Vec3::new(f64::from(x) + 1.0, bounds.max.y, f64::from(z) + 1.0),
            );
            let column = column.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
            world.collides_with_suffocating_block(column, release, true)
        } else {
            (floor_i32_for(release, bounds.min.y)..ceil_i32_for(release, bounds.max.y))
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
        let (cell_x, cell_z) = (floor_i32_for(release, x), floor_i32_for(release, z));
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
        // (1.16.1 LocalPlayer.java:410; 1.16.2 LocalPlayer.java:410)
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
        let cell = (floor_i32_for(release, x), floor_i32_for(release, feet_y), floor_i32_for(release, z));
        // (1.12.2 LocalPlayer.java:390; 1.13.2 LocalPlayer.java:403, 448)
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

pub(crate) fn restitute_movement_after_collisions(
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
    let mut after = current;
    if collisions[0] {
        after.x = -current.x * entity_restitution;
    }
    if collisions[2] {
        after.z = -current.z * entity_restitution;
    }
    if !collisions[1] {
        state.velocity = after;
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
        let effect_state = world.block_cow(landing_position(world, state, support, rules.release));
        // (26.2 Entity.java:848; 26.3 Entity.java:878)
        let resting = if rules.release.at_least(26, 3, 0) {
            -current.y <= gravity
        } else {
            -current.y < gravity
        };
        if resting || suppressing_bounce || effect_state.suppresses_bounce {
            0.0
        } else {
            // Entity.getBlockBounciness: the whole restitution for a LivingEntity, 0.8F of it in
            // float for any other (26.2 Entity.java:874-881, 26.3 :905-911)
            let block_restitution = if state.non_living {
                f64::from(effect_state.bounce_restitution as f32 * 0.8)
            } else {
                effect_state.bounce_restitution
            };
            entity_restitution
                .max(block_restitution)
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
    after.y = (gravity_compensation - current.y) * effective_drag * restitution;
    // (0 / 0 of a bounce with no motion, which setDeltaMovement refuses as a whole, 26.2 Entity.java:
    // the last line of the method)
    state.set_delta_movement(after, rules.release);
}

pub(crate) fn entity_collision_context(state: &SimulationState) -> EntityCollisionContext {
    EntityCollisionContext {
        bottom: state.position.y,
        descending: state.sneaking,
        fall_distance: state.fall_distance,
        walks_on_powder_snow: state.powder_snow_walkable,
        // (canEntityWalkOnPowderSnow holds only of a living entity, a tagged mob or leather boots: 1.17
        // PowderSnowBlock.java:111-116, 26.3 :116-124. A body no living entity that walks on it is the
        // falling block moving_entity.rs makes)
        falling_block: state.non_living && state.powder_snow_walkable,
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
    let entities = world.entity_collisions(
        entity_collision_query(swept, query, max_step, rules.release),
        context,
    );
    // (1.13.2 Entity.java:672-766; 1.14.4 Entity.java:691-719)
    if !rules.release.at_least(1, 14, 0) {
        let mut obstacles = if rules.release.at_least(1, 13, 0) {
            // (past the query's 1.0E-7, whose shapes legacy_colliders tells)
            let reach = query.inflate(Vec3::new(1.0e-6, 1.0e-6, 1.0e-6));
            world.collision_shapes_for_entity(reach, context, rules.release)
        } else {
            world.legacy_collision_shapes_for_entity(query, context, rules.release)
        };
        let inside = world.is_inside_legacy_world_border(state.position, state.outside_border);
        // (1.13.2 Entity.java:675 asks nothing of a move of nothing; 1.12.2 always asks)
        let asked = !rules.release.at_least(1, 13, 0) || desired != Vec3::ZERO;
        let outside_border = if asked && updates_outside_border(world, swept, rules.release) {
            !inside
        } else {
            state.outside_border
        };
        if inside {
            let outside_cell = |bounds: &Aabb| {
                world.is_outside_world_border_cell(bounds.min.x.floor(), bounds.min.z.floor())
            };
            obstacles.retain(|obstacle| !outside_cell(obstacle));
            obstacles.extend(world.legacy_world_border_cells(query, rules.release));
        }
        // (1.12.2 Entity.java:662-695; 1.13.2 Entity.java:678-697)
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
    // (1.17.1 Entity.java:754-755; 1.18 Entity.java:800-801, WorldBorder.java:76-79)
    let collides_with_border = if rules.release.at_least(1, 18, 0) {
        world.is_inside_close_to_world_border(
            state.position,
            bounds.expand_towards(desired),
            rules.release,
        )
    } else {
        // Shapes.joinIsNotEmpty of the border and the box less 1.0E-7, whose merge takes another
        // 1.0E-7 off (1.16.5 Entity.java:794, 1.17 :755)
        let inside = bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !border.iter().any(|outside| shapes_join(*outside, inside))
    };
    // the blocks as whole shapes with their cells, in BlockCollisions' order; the border's shape
    // between the entity colliders and them (Entity.collectColliders)
    let blocks = world.collision_block_shapes_for_entity(query, context, rules.release);
    let collision = CollisionRules {
        // (1.20.6 Entity.java:916-919 collides the lifted move; 1.21 Entity.java:917-926 tries the
        // candidate step heights)
        step_up: if rules.release.at_least(1, 21, 0) {
            StepUp::CandidateHeights
        } else {
            StepUp::Lifted
        },
        // (1.17.1 Entity.java:787; 1.18 Entity.java:794)
        single_axis_walk: !rules.release.at_least(1, 18, 0),
    };
    // from 1.18 each attempt asks again of its own box (Colliders::border_near)
    let near = |query: Aabb| world.is_inside_close_to_world_border(state.position, query, rules.release);
    let modern_border = rules.release.at_least(1, 18, 0);
    let colliders = Colliders {
        entities: &entities,
        border: (collides_with_border || modern_border).then_some(&border[..]),
        border_near: modern_border.then_some(&near as BorderNear<'_>),
        boxes: &[],
        block_border: world
            .block_collisions_offer_world_border(bounds, rules.release)
            .then_some(&border[..]),
        blocks: &blocks,
        release: Some(rules.release),
    };
    let movement = collide_shapes(
        bounds,
        desired,
        colliders,
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

/// Whether Level.doesBoxCollide sets Entity.isOutsideBorder for a move sweeping `swept`: from
/// 1.13 always (IWorldReaderBase.getCollisionBoxes), before only once its walk over the columns
/// reaches one in a loaded chunk, the columns of the box before 1.9 and, from 1.9, those around it
/// too but the corners (Level.doesBoxCollide).
fn updates_outside_border(world: &WorldSnapshot, swept: Aabb, release: MinecraftVersion) -> bool {
    // (1.12.2 Level.java:1133-1134; 1.13.2 LevelReader.java:176-177)
    if release.at_least(1, 13, 0) {
        return true;
    }
    // (1.8.8 Level.java:1047; 1.9.4 Level.java:1058)
    let (columns_x, columns_z) = if release.at_least(1, 9, 0) {
        (
            floor_i32_for(release, swept.min.x).wrapping_sub(1)..ceil_i32_for(release, swept.max.x).wrapping_add(1),
            floor_i32_for(release, swept.min.z).wrapping_sub(1)..ceil_i32_for(release, swept.max.z).wrapping_add(1),
        )
    } else {
        (
            floor_i32_for(release, swept.min.x)..floor_i32_for(release, swept.max.x + 1.0),
            floor_i32_for(release, swept.min.z)..floor_i32_for(release, swept.max.z + 1.0),
        )
    };
    // (1.8.8 Level.java:1061; 1.9.4 Level.java:1073)
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
/// position follows: before 1.14 to where the collision moved the box (vanilla: Entity#move,
/// with Entity#setLocationFromBoundingbox).
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

/// The MoverType an Entity.move of the player takes. Before 1.11 moveEntity takes none and every
/// call backs off from edges as SELF does (1.10.2 Entity.java:574, 1.11.2 :614).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoverType {
    /// SELF or PLAYER: the tick's own move, a riptide lift, a pre-1.11 piston's moveEntity.
    SelfOrPlayer,
    Piston,
    ShulkerBox,
}

/// What Entity.move left for its caller.
#[derive(Clone, Debug)]
pub(crate) struct EntityMove {
    /// The movement after the stuck speed, the size guard and the edge back-off (`delta`).
    pub desired: Vec3,
    /// The collided movement (`movement`).
    pub displacement: Vec3,
    /// The supporting block after the move (kept where the move did not refresh it).
    pub support: Option<BlockPosition>,
    /// checkFallDamage's result; None under noPhysics.
    pub fall: Option<FallCheck>,
    /// The move stopped after checkFallDamage for a removed entity (1.17+).
    pub removed: bool,
    /// The blocks the move's checkInsideBlocks visited, before 1.21.2.
    pub inside: Vec<((i32, i32, i32), bool)>,
}

/// Entity.move's fall-reset clip, from 1.18.2: a move of a block or more by an entity with a fall
/// distance clips FALLDAMAGE_RESETTING and Fluid.WATER from where it starts, at most 8 blocks from
/// 1.21.9, and resets the fall on a hit (1.18.1 Entity.java:595-598 only moves; 1.18.2
/// Entity.java:569-574 clips first; 1.19 :570-580; 1.21.9 :739-746; 26.3 :776-784). Every move
/// takes it, the player's and every other entity's.
pub(crate) fn reset_fall_on_clip(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    start: Vec3,
    displacement: Vec3,
    release: MinecraftVersion,
) {
    if state.no_physics
        || !release.at_least(1, 18, 2)
        || state.fall_distance == 0.0
        || !(displacement.length_squared() >= 1.0)
    {
        return;
    }
    let clip_end = if release.at_least(1, 21, 9) {
        // Vec3.normalize divides by the length (never under 1.0E-5F here)
        let length = displacement.length();
        let normal = Vec3::new(
            displacement.x / length,
            displacement.y / length,
            displacement.z / length,
        );
        start + normal.scale(length.min(8.0))
    } else {
        start + displacement
    };
    let shape = ClipShape::FallDamageResetting {
        player: state.is_player,
    };
    if clip_hits(world, start, clip_end, release, shape) {
        state.fall_distance = 0.0;
    }
}

/// Entity.move's noPhysics branch: the whole movement through what is in its way, and from 1.21.9
/// no collision flag left over (1.8.8 Entity.java:464-466; 1.21.8 :706-708; 1.21.9 :713-720; 26.3
/// :746-752).
pub(crate) fn move_without_physics(state: &mut SimulationState, movement: Vec3, release: MinecraftVersion) {
    match state.bounding_box {
        Some(bounds) => {
            let moved = bounds.translated(movement);
            state.bounding_box = Some(moved);
            state.position = position_of_bounding_box(moved);
        }
        None => state.position = state.position + movement,
    }
    if release.at_least(1, 21, 9) {
        state.collided_horizontally = false;
        state.minor_horizontal_collision = false;
        state.collided_vertically = false;
        state.vertical_collision_below = false;
    }
}

/// Whether Entity.move refreshes verticalCollision, onGround and the supporting block: always
/// before 1.21.4, then only for a move with a vertical part (`Math.abs(y) > 0.0`, false for NaN) or
/// an entity its instance controls (1.21.3 Entity.java:697-706; 1.21.4 :706-710; 26.3 :798-803).
pub(crate) fn refreshes_vertical_collision(
    state: &SimulationState,
    requested_y: f64,
    release: MinecraftVersion,
) -> bool {
    !release.at_least(1, 21, 4) || requested_y.abs() > 0.0 || state.controlled_by_local_instance
}

/// Entity.isHorizontalCollisionMinor: false but for LocalPlayer, which overrides it from 1.18
/// (1.18 Entity.java, LocalPlayer.java; 26.3 Entity.java:805-809, LocalPlayer.java). A server
/// player and a remote player keep false.
fn minor_horizontal_collision(state: &SimulationState, displacement: Vec3, release: MinecraftVersion) -> bool {
    state.minor_horizontal_collision_supported
        && state.is_player
        && state.client_side
        && state.controlled_by_local_instance
        && state.collided_horizontally
        && horizontal_collision_is_minor(state, displacement, release)
}

/// Entity.move of the player's body, one implementation for the tick's own move, a riptide lift,
/// a piston's and a shulker box's push: noPhysics, the stuck speed (not a PISTON move's from
/// 1.21.10, 1.21.10 Entity.java:729-735), the edge back-off of SELF and PLAYER moves (1.11.2
/// Entity.java:614, 26.3 Player.java:831-833), the collision, the fall-reset clip, the flags, the
/// ground and support, checkFallDamage, the isRemoved exit (1.17 Entity.java:568, 26.3 :817), the
/// motion stop and landing callback before 26.2 or restituteMovementAfterCollisions from it, and
/// before 1.21.2 checkInsideBlocks (with inLava 1.14 - 1.16.1), then the speed factor from 1.15.
/// `support` is the supporting block before the move. The engine refuses a move past
/// MAX_WALKED_CELLS (`move_out_of_range`), which vanilla has no counterpart for.
pub(crate) fn entity_move(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    mover: MoverType,
    rules: VersionPhysicsRules,
    fall_rules: FallDamageRules,
    support: Option<BlockPosition>,
) -> EntityMove {
    let release = rules.release;
    let slowed = slowed_by_stuck_speed(state, movement);
    let mut movement = if mover == MoverType::Piston && release.at_least(1, 21, 10) {
        movement
    } else {
        slowed
    };
    // (every source of motion ends here: the walks below take a step or a cell at a time)
    if state.bounds().expand_towards(movement).cell_count() > MAX_WALKED_CELLS {
        state.move_out_of_range = true;
        movement = Vec3::ZERO;
    }
    let collision_context = entity_collision_context(state);
    let desired = if mover == MoverType::SelfOrPlayer {
        back_off_from_edge(state, world, &collision_context, movement, rules.step_height, release)
    } else {
        movement
    };
    if state.no_physics {
        move_without_physics(state, desired, release);
        refresh_legacy_eyes(state, world, release);
        return EntityMove {
            desired,
            displacement: desired,
            support,
            fall: None,
            removed: false,
            inside: Vec::new(),
        };
    }
    let collision = collide_with_world(state, world, &collision_context, desired, rules);
    let displacement = collision.movement;
    reset_fall_on_clip(state, world, state.position, displacement, release);
    displace(state, desired, collision, release);
    let horizontal_collision = |requested: f64, moved: f64| {
        // (1.13.2 Entity.java:773; 1.14.4 Entity.java:499)
        if release.at_least(1, 14, 0) {
            !vanilla_equal(requested, moved)
        } else {
            requested != moved
        }
    };
    let x_collision = horizontal_collision(desired.x, displacement.x);
    let y_collision = desired.y != displacement.y;
    let z_collision = horizontal_collision(desired.z, displacement.z);
    state.collided_horizontally = x_collision || z_collision;
    state.minor_horizontal_collision = minor_horizontal_collision(state, displacement, release);
    let mut support = support;
    if refreshes_vertical_collision(state, desired.y, release) {
        state.collided_vertically = y_collision;
        state.vertical_collision_below = y_collision && desired.y < 0.0;
        state.on_ground = state.vertical_collision_below;
        support = check_supporting_block(world, state, Some(displacement), release);
    }
    let fall = check_fall_damage(
        state,
        world,
        rules,
        support,
        displacement.y,
        y_collision,
        fall_rules,
    );
    // Entity.move (1.17+) stops after checkFallDamage for a removed entity (1.16.5 Entity.java:643;
    // 1.17 Entity.java:570; 26.3 :817)
    let removed = state.removed && release.at_least(1, 17, 0);
    // Block.updateEntityMovementAfterFallOn runs on any vertical collision, a ceiling's too; the
    // flag says whether any move of the tick ended in it
    state.block_fall_callback |=
        block_fall_callback_runs(state, y_collision, x_collision || z_collision, desired.y, release);
    let mut outcome = EntityMove {
        desired,
        displacement,
        support,
        fall: Some(fall),
        removed,
        inside: Vec::new(),
    };
    if removed {
        return outcome;
    }
    if fall_rules.restitution_enabled {
        // restituteMovementAfterCollisions where the instance simulates the entity, for a move that
        // went vertically into the field verticalCollision or hit something sideways; it reads the
        // field, which a level move of an entity the instance does not control keeps (26.2
        // Entity.java:815, 846; 26.3 :820, 857)
        if can_simulate_movement(state, release)
            && ((desired.y.abs() > 0.0 && state.collided_vertically) || x_collision || z_collision)
        {
            restitute_movement_after_collisions(
                state,
                world,
                support,
                rules,
                [x_collision, state.collided_vertically, z_collision],
                displacement.y,
            );
        }
    } else {
        stop_blocked_motion(state, desired, displacement, [x_collision, y_collision, z_collision], fall, release);
    }
    update_in_lava_after_move(state, world, release);
    // (1.21.1 Entity.java:738 checkInsideBlocks in move; 1.21.2 Entity.java:776-810 applyEffectsFromBlocks after it)
    if !release.at_least(1, 21, 2) {
        let inside: Vec<_> = blocks_inside(state, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_block_effects(state, world, support, release, &inside);
        outcome.inside = inside;
    }
    // (1.14.4 Entity.java:569; 1.15 Entity.java:634)
    if release.at_least(1, 15, 0) {
        let speed_factor = f64::from(block_speed_factor(world, state, support, release));
        state.velocity.x *= speed_factor;
        state.velocity.z *= speed_factor;
    }
    outcome
}

/// The motion after a move that hit something before 26.2: it stops (1.14 - 1.18.1 put back the
/// x motion when z stops too), and landing multiplies the vertical motion by 0 (zeroes it before
/// 1.14) but where the slime block bounced it (check_fall_damage) or a bed bounces it back by
/// 0.66 (BedBlock.bounceUp, from 1.12; 1.21.11 BedBlock.java:143), as the tick's own move.
pub(crate) fn stop_blocked_motion(
    state: &mut SimulationState,
    movement: Vec3,
    displacement: Vec3,
    [x_collision, y_collision, z_collision]: [bool; 3],
    landing: FallCheck,
    release: MinecraftVersion,
) {
    let current = state.velocity;
    // (1.18.1 Entity.java:618-624 (x then z reset from one snapshot, exact !=); 1.18.2 Entity.java:582-583,600-602 (both by !Mth.equal in one call))
    let (stops_x, stops_z) = if release.at_least(1, 18, 2) {
        (x_collision, z_collision)
    // (1.13.2 Entity.java:793-799; 1.14.4 Entity.java:520-525)
    } else if release.at_least(1, 14, 0) {
        let stops_z = movement.z != displacement.z;
        (movement.x != displacement.x && !stops_z, stops_z)
    } else {
        (movement.x != displacement.x, movement.z != displacement.z)
    };
    if stops_x {
        state.velocity.x = 0.0;
    }
    if stops_z {
        state.velocity.z = 0.0;
    }
    if y_collision && simulates_landing(state, release) && !landing.bounced {
        if landing.landed_on_bed && !state.sneaking {
            if current.y < 0.0 {
                state.velocity.y = -current.y * 0.6600000262260437 * landing_bounce(state, release);
            }
        } else {
            // (1.13.2 Block.java:680-682; 1.14.4 Block.java:687-689)
            state.velocity.y = if release.at_least(1, 14, 0) {
                current.y * 0.0
            } else {
                0.0
            };
        }
    }
}

/// Entity.baseTick's fluids and fire: the water and lava contact with their pushes, the fire's
/// ticks and the fall distance halved in lava. Returns the water and lava contact.
fn base_tick_fluids_and_fire(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    rules: &VersionPhysicsRules,
) -> (FluidContact, FluidContact) {
    let affected_by_fluids = !(state.is_player && state.flying);
    let legacy_fluids = legacy_fluid::applies(rules.release);
    let bounds = state.bounds();
    let water = if legacy_fluids {
        legacy_fluid::handle_water_movement(state, world, affected_by_fluids, rules.release);
        FluidContact::default()
    } else {
        let water = entity_fluid_contact(state, world, FluidState::Water, rules.release);
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
    // height stays 0), and isInLava is the flag of the last move (1.16 Entity.java:1039-1048,
    // 1164-1166); 1.16.2 always measures it (1.16.5 Entity.java:1043-1049, 1178-1180)
    let measures_lava =
        rules.release.at_least(1, 16, 2) || !rules.release.at_least(1, 16, 0) || !state.in_water;
    let lava = if measures_lava {
        entity_fluid_contact(state, world, FluidState::Lava, rules.release)
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
        // (1.16.1 Entity.java:1161-1163; 1.16.2 Entity.java:1178-1180)
        if rules.release.at_least(1, 16, 2) {
            state.in_lava = lava.height > 0.0;
        }
    // (1.13.2 Entity.java:508-511, 1143-1145; 1.14.4 Entity.java:400-403, 1171-1173)
    } else if !rules.release.at_least(1, 14, 0) {
        state.in_lava = legacy_fluid::touching_lava(world, bounds, rules.release);
    }
    base_tick_fire(state, world, rules.release);
    // Entity.baseTick halves the fall distance every tick in lava, on the client too (1.8.8
    // Entity.java:412, 1.14.4 :402, 1.21.4 :514, 1.21.5 :518 `*= 0.5`; 1.14 - 1.15 read the inLava
    // of the last move). Halving is exact in float and double alike.
    if state.in_lava {
        state.fall_distance *= 0.5;
    }
    (water, lava)
}

/// The release of a riptide trident: its launch (launch_riptide), what the lift moved
/// (riptide_lift) and the Movement it recorded (riptide_lift_movement).
fn release_riptide(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    rules: VersionPhysicsRules,
    fall_rules: FallDamageRules,
) {
    state.riptide_lift = Vec3::ZERO;
    state.riptide_lift_movement = None;
    if let Some(level) = state.riptide_launch.take() {
        let launched_from = state.position;
        if let Some((requested, moved)) = launch_riptide(state, world, level, rules, fall_rules) {
            state.riptide_lift_movement =
                recorded_movement(state, launched_from, requested, moved, rules.release);
        }
        state.riptide_lift = state.position - launched_from;
    }
}

/// The release of a riptide trident in the client's handleKeybinds, with the release's profile,
/// before the entities that tick before the player (D-214); the tick then leaves it.
pub fn release_riptide_with_profile<P: VersionPhysicsProfile>(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    _config: PhysicsConfig,
) {
    let fall_rules = FallDamageRules {
        landing_damage_rounding: P::landing_damage_rounding(),
        restitution_enabled: P::restitution_enabled(),
        modern_fall_damage: P::modern_fall_damage(),
    };
    release_riptide(state, world, P::rules(), fall_rules);
    state.riptide_released = true;
}

/// TridentItem.releaseUsing (1.13+), before the tick: a riptide trident let go of in water pushes
/// the player along its look by the spin attack strength, and a player on the ground moves up by
/// 1.2 at once.
fn launch_riptide(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    level: u8,
    rules: VersionPhysicsRules,
    fall_rules: FallDamageRules,
) -> Option<(Vec3, Vec3)> {
    let release = rules.release;
    // (1.21.11 TridentItem.java:64; 26.1 TridentItem.java:64)
    let riding = state.passenger && release.at_least(26, 1, 0);
    // Entity.isInWaterOrRain (1.13.2 class_3564.java:62 through Entity.java:1022-1023, 1.20.4
    // TridentItem.java:66, 26.3 :64)
    let wet = state.in_water || state.in_rain;
    if level == 0 || !release.at_least(1, 13, 0) || !wet || riding {
        return None;
    }
    let (yaw, pitch) = (f64::from(state.yaw as f32), f64::from(state.pitch as f32));
    let cos_pitch = mth_cos(pitch, release) as f32;
    let x = -(mth_sin(yaw, release) as f32) * cos_pitch;
    let y = -(mth_sin(pitch, release) as f32);
    let z = mth_cos(yaw, release) as f32 * cos_pitch;
    let length = (x * x + y * y + z * z).sqrt();
    // the spin attack strength: 3.0F * ((1.0F + level) / 4.0F) to 1.20.6 (1.20.6 TridentItem.java:106),
    // the enchantment's value from 1.21 (1.21 TridentItem.java:73)
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
        return None;
    }
    // Player.move(SELF, (0, 1.2, 0)): an Entity.move as the tick's own, its stuck speed, collision,
    // flags, checkFallDamage, ceiling stop and, before 1.21.2, its checkInsideBlocks (1.13.2
    // class_3564.java:104, 1.20.4 TridentItem.java:94, 26.3 TridentItem.java:93); from 1.21.2 the move
    // only queues its path for the tick's applyEffectsFromBlocks (1.21.3 Entity.java:776-793)
    let support = kept_supporting_block(world, state, None, release);
    let lift = entity_move(
        state,
        world,
        Vec3::new(0.0, 1.1999999284744263, 0.0),
        MoverType::SelfOrPlayer,
        rules,
        fall_rules,
        support,
    );
    // the lift's own fire test (before 1.17), and its powder snow and fire from 1.17, after its
    // isRemoved exit, as a piston's move (D-228: 1.17 Entity.java:568, 630-648; 1.21.1 :738-760);
    // from 1.21.2 the tick's applyEffectsFromBlocks takes the lift's path
    if !release.at_least(1, 21, 2) && !lift.removed {
        crate::piston::apply_powder_snow(state, world, &lift.inside, &InsideEffects::default(), release);
    }
    Some((lift.desired, lift.displacement))
}

/// Block.getJumpFactor of the block at the feet, else of the block below that affects movement.
pub(crate) fn block_jump_factor(
    world: &WorldSnapshot,
    body: BlockPosition,
    below: BlockPosition,
) -> f32 {
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
    // (1.19.3 LocalPlayer.java:937; 1.19.4 LocalPlayer.java:919)
    if release.at_least(1, 19, 4) {
        1.0 / length_squared.sqrt()
    // (1.14.4 LocalPlayer.java:838; 1.15 LocalPlayer.java:878)
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
        // (1.9.4 LocalPlayer.java:1-782; 1.10.2 LocalPlayer.java:807-818)
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
    // 1.15 starts the probe at position() (1.15 LocalPlayer.java:860), 1.14 at the
    // bounding box minY (1.14.4 LocalPlayer.java:817).
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
    // before 1.13 the probes read getCollisionBoundingBox, one box (D-196: 1.12.2
    // LocalPlayer.java:879-882, 911-923, method_11726), not the block's collision boxes
    let legacy = !release.at_least(1, 13, 0);
    let probe = |position| {
        if legacy {
            world.legacy_collision_bounding_boxes(position, &context)
        } else {
            world.block_collision_boxes_for_entity(position, &context)
        }
    };
    let empty = |position| probe(position).is_empty();
    let mut ceiling = block_position(Vec3::new(state.position.x, bounds.max.y, state.position.z), release);
    if !empty(ceiling) {
        return;
    }
    ceiling.1 += 1;
    if !empty(ceiling) {
        return;
    }
    let mut jump_height = 1.2_f32;
    if let Some(amplifier) = state.jump_boost_amplifier {
        jump_height += (amplifier + 1) as f32 * 0.75;
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
    // after the block shapes before 1.18, before them from 1.18 (1.12.2 Level.getCollisionBoxes,
    // 1.16.5 CollisionGetter.java:58-59, 1.18 CollisionGetter.java:67-70, 1.21.11
    // CollisionGetter.java:85-89); from 1.14 the entities meeting the area grown by 1.0E-7
    // (1.14.4 EntityGetter.java:54-58, 1.21.11 EntityGetter.java:51-63)
    let entity_area = if release.at_least(1, 14, 0) {
        area.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
    } else {
        area
    };
    let entities = world.entity_collisions(entity_area, &context);
    // 1.13 - 1.14.0 defer the full cubes into one merged shape after the partial ones (1.13.2
    // LevelReader.java:113-155, 1.14.4 LevelReader.java:179-201); 1.14.4 and 1.15 walk every
    // block in order (1.14.4 LevelReader.java:130-145, 1.15 CollisionGetter.java:67-106)
    // before 1.13 Level.getCollisionBoxes' walk, x, then z, then y (D-195: 1.12.2 Level.method_13689)
    let blocks = if legacy {
        world.legacy_collision_boxes_in_order(area, collision_query_entity(state, &context), release)
    } else if !release.at_least(1, 14, 1) {
        world.block_collision_boxes_merging_full_cubes(area, &context)
    } else {
        world.block_collision_boxes_in_order(area, &context, release)
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
        ), release);
        let mut steps = 1;
        while (steps as f32) < jump_height {
            let above = (center.0, center.1 + steps, center.2);
            let shape = probe(above);
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

/// How a release lands an entity: LivingEntity.computeFallDamage's rounding, the attributes'
/// damage from 1.20.5 and the restitution from 26.2 in place of the landing block's bounce.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FallDamageRules {
    pub landing_damage_rounding: LandingDamageRounding,
    pub restitution_enabled: bool,
    pub modern_fall_damage: bool,
}

/// What check_fall_damage left for the rest of Entity.move: a slime block's bounce, a bed's
/// landing, and the lava height its water refresh measured (26.1+).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FallCheck {
    pub bounced: bool,
    pub landed_on_bed: bool,
    pub lava_height: Option<f64>,
}

/// The share of a slime block's or bed's bounce an entity keeps: a LivingEntity all of it, any
/// other 0.8 from 1.9, where the slime block's test arrives (1.8.8 SlimeBlock.java:33-39; 1.9.4
/// :34-44, 1.21.11 :43). The bed bounces only from 1.12 and has the test from there (1.12.2
/// BedBlock.java:155-159, 1.21.11 :142-143).
pub(crate) fn landing_bounce(state: &SimulationState, release: MinecraftVersion) -> f64 {
    if state.non_living && release.at_least(1, 9, 0) {
        0.8
    } else {
        1.0
    }
}

/// Entity.checkFallDamage (Entity.fall, updateFallState before 1.14) after a move that went
/// `displacement_y` and collided vertically or not: the water refresh, the drop counted into the
/// fall distance or the landing that deals the damage and ends the fall, and the slime block's
/// bounce in place of it before 26.2.
pub(crate) fn check_fall_damage(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    rules: VersionPhysicsRules,
    support: Option<BlockPosition>,
    displacement_y: f64,
    y_collision: bool,
    fall: FallDamageRules,
) -> FallCheck {
    let affected_by_fluids = !(state.is_player && state.flying);
    let legacy_fluids = legacy_fluid::applies(rules.release);
    let mut lava_height = None;
    // Entity.move (1.21.2+) runs checkFallDamage on the client only for a locally controlled
    // entity (isLogicalSideForUpdatingMovement, isLocalInstanceAuthoritative from 1.21.5). 1.21.4
    // leaves a player out of it on both sides (1.21.4 Entity.java:721, Player.java:2049),
    // where the client then has no landing at all and the server has the one the moves it receives
    // make (handleFall, Entity.java:1278-1284, ServerGamePacketListenerImpl.java:975, :992).
    let checks_fall_damage = !state.no_physics
        && !(state.is_player && state.client_side && rules.release == MinecraftVersion::new(1, 21, 4))
        && (!rules.release.at_least(1, 21, 2)
            || state.controlled_by_local_instance
            || (!state.client_side && !rules.release.at_least(1, 21, 5)));
    if checks_fall_damage && !state.in_water && legacy_fluids {
        legacy_fluid::handle_water_movement(state, world, affected_by_fluids, rules.release);
    } else if checks_fall_damage && !state.in_water {
        let water = entity_fluid_contact(state, world, FluidState::Water, rules.release);
        if water.touching {
            state.in_water = true;
            state.fall_distance = 0.0;
            if affected_by_fluids {
                push_by_fluid(state, water, rules.water_flow_scale, rules.release);
            }
        }
        // (1.21.11 LivingEntity.java:321-323; 26.1 LivingEntity.java:311-313, Entity.java:1572)
        if rules.release.at_least(26, 1, 0) {
            // LivingEntity.checkFallDamage's updateFluidInteraction refreshes the eyes as well,
            // at where the move left them (26.1 EntityFluidInteraction.update)
            state.eye_in_water =
                entity_eyes_in_fluid(state, world, FluidState::Water, rules.release);
            state.eye_in_lava =
                entity_eyes_in_fluid(state, world, FluidState::Lava, rules.release);
            let lava = entity_fluid_contact(state, world, FluidState::Lava, rules.release);
            state.in_lava = lava.touching;
            lava_height = Some(lava.height);
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
    // LivingEntity's water refresh, and lands with it (1.21.5 Entity.java:1321-1331,
    // LivingEntity.java:299-303); before, Entity.fall counts it only when not landing.
    let counts_drop_before_landing = rules.release.at_least(1, 21, 5);
    if checks_fall_damage && counts_drop_before_landing && !state.in_water && displacement_y < 0.0 {
        state.fall_distance =
            accumulated_fall_distance(state.fall_distance, displacement_y, rules.release);
    }
    let landed_on = world.effect_at(landing_position(world, state, support, rules.release));
    // SlimeBlock.updateEntityMovementAfterFallOn, the block's callback. The fall ends in
    // checkFallDamage, on the ground and for an entity that runs it, as any other landing
    // (1.21.11 Entity.java:790-805, 1.20.4 :660-663).
    let bounced = !fall.restitution_enabled
        && y_collision
        && simulates_landing(state, rules.release)
        && landed_on == BlockEffect::Slime
        && !state.sneaking;
    if bounced {
        // the bounce itself is updateEntityAfterFallOn, after Entity.move's isRemoved exit; the
        // fall's end is checkFallDamage's, before it (1.17 Entity.java:567-582, 1.21.11 :793-807)
        let removed = state.removed && rules.release.at_least(1, 17, 0);
        if state.velocity.y < 0.0 && !removed {
            state.velocity.y = -state.velocity.y * landing_bounce(state, rules.release);
        }
        if state.on_ground
            && (checks_fall_damage || !state.client_side)
            && resets_landed_fall(state.fall_distance, rules.release)
        {
            state.fall_distance = 0.0;
        }
    }
    // Block.fallOn: the fall distance and damage multiplier the landing block hands
    // causeFallDamage. The bed halves the distance from 1.12 (BedBlock); an upward pointed
    // dripstone tip adds 2.0F to it and doubles the damage, 2.5 in double from 1.21.5 (1.17
    // PointedDripstoneBlock.java:131-133, 1.21.5 :135-137); hay deals a fifth from 1.9 (1.9.4
    // HayBlock.java:19-20; 1.8.9 has no fallOn), honey a fifth (1.15 HoneyBlock.java:55-61).
    // (1.11.2 BedBlock.java has no fallOn; 1.12.2 BedBlock.java:147-148 halves the distance)
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
    // PowderSnowBlock.java:87-93) and, from 1.21.2, the slime block's for an entity suppressing the
    // bounce (1.21.2 SlimeBlock.java:24-28; 1.21.1 hands it to Block.fallOn). Entity.checkFallDamage
    // still resets the fall distance. 1.17 and 1.17.1 declare no fallOn, but a player's landing is
    // the server's: ServerPlayNetworkHandler moves it first, and that move's powder snow
    // (Entity.slowMovement, 1.17 Entity.java) has zeroed the fall before ServerPlayer.doCheckFallDamage
    // reads it, so its health does not move (fd_on_powder_snow_h6 on both); the client works out
    // a hurt it cannot deal. A mob lands in its own move, before the block.
    let landing_deals_no_damage = match landed_on {
        BlockEffect::PowderSnow => rules.release.at_least(1, 18, 0) || state.is_player,
        BlockEffect::Slime => fall.restitution_enabled || rules.release.at_least(1, 21, 2),
        _ => false,
    };
    if checks_fall_damage && state.on_ground && !bounced {
        // Block.fallOn runs for a fall distance above 0 only (1.8.8 Entity.java, 1.20.4
        // Entity.java:1118, 26.3 :1602): a move on the ground leaves what a landing of the same tick
        // reported, as a piston's after the player's own
        if state.fall_distance > 0.0 {
            // Player.causeFallDamage deals nothing where the player may fly (1.8.8
            // Player.java:1380-1381, 1.21.5 Player.java:1392-1394)
            state.fall_damage = if state.fall_damage_immune || landing_deals_no_damage {
                0.0
            } else if fall.modern_fall_damage {
                attribute_landing_damage(
                    landing_fall_distance,
                    state.safe_fall_distance,
                    block_damage_multiplier,
                    state.fall_damage_multiplier.unwrap_or(1.0),
                    fall.landing_damage_rounding,
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
                    fall.landing_damage_rounding,
                )
            };
        }
        if resets_landed_fall(state.fall_distance, rules.release) {
            state.fall_distance = 0.0;
        }
    } else if checks_fall_damage && !counts_drop_before_landing && !bounced && displacement_y < 0.0
    {
        state.fall_distance =
            accumulated_fall_distance(state.fall_distance, displacement_y, rules.release);
    }
    refresh_legacy_eyes(state, world, rules.release);
    FallCheck {
        bounced,
        landed_on_bed,
        lava_height,
    }
}

/// Whether a landing ends the fall: before 1.9 only a fall distance above 0 is reset (1.8.8
/// Entity.java:790-800, inside its `fallDistance > 0.0F`), from 1.9 any (1.9.4 Entity.java:891-897).
fn resets_landed_fall(fall_distance: f64, release: MinecraftVersion) -> bool {
    release.at_least(1, 9, 0) || fall_distance > 0.0
}

/// Before 1.16 no eye flag is kept: Entity.isUnderLiquid asks the world where the entity is now
/// (1.15 Entity.java isSubmergedIn), so what a move leaves is where it left the eyes (1.15.2
/// Entity.java:1111-1124; 1.16 Entity.java:1153-1154 reads the fluidOnEyes field the base tick set).
fn refresh_legacy_eyes(state: &mut SimulationState, world: &WorldSnapshot, release: MinecraftVersion) {
    if !release.at_least(1, 16, 0) {
        state.eye_in_water = entity_eyes_in_fluid(state, world, FluidState::Water, release);
        state.eye_in_lava = entity_eyes_in_fluid(state, world, FluidState::Lava, release);
    }
}

/// Whether LivingEntity.aiStep's applyEffectsFromBlocks after travel runs for this instance: from
/// 1.21.2 on the server, or on a client that controls the entity (1.21.2 LivingEntity.java:2817
/// and 1.21.4 :2769 isControlledByLocalInstance; 1.21.5 :2755, 1.21.11 :2966 and 26.3 :3064
/// isLocalInstanceAuthoritative, which is the same on a client: the local player, or what it
/// controls, 1.21.5 Entity.java:3220-3236).
pub(crate) fn applies_effects_from_blocks(state: &SimulationState, release: MinecraftVersion) -> bool {
    !release.at_least(1, 21, 2) || !state.client_side || state.controlled_by_local_instance
}

/// Whether the block's fall callback, which stops or bounces the motion, runs for this instance of
/// the entity: from 1.21.2 not on a client that does not own it (1.21.3 Entity.java:698-703, 1.21.11
/// :799-804, canSimulateMovement). (1.21.1 Entity.java:690-692 runs it on every instance; 1.21.2
/// Entity.java:709-710, 721-724 skips a client that does not control the entity)
pub(crate) fn simulates_landing(state: &SimulationState, release: MinecraftVersion) -> bool {
    !release.at_least(1, 21, 2) || state.controlled_by_local_instance || !state.client_side
}

/// Whether a move, the tick's own or a piston's, ends in the block's fall callback (see its call in
/// the tick): `requested_y` is the vertical part of the move as asked. (1.21.1 Entity.java:690-692
/// on every instance; 1.21.2 Entity.java:721-724 only when controlled by the local instance;
/// 26.1.2 Entity.java:788-791 canSimulateMovement alone; 26.2 Entity.java:793, 815-816 also wants
/// the move to have asked for a vertical part, or a horizontal collision)
pub(crate) fn block_fall_callback_runs(
    state: &SimulationState,
    y_collision: bool,
    horizontal_collision: bool,
    requested_y: f64,
    release: MinecraftVersion,
) -> bool {
    !state.no_physics
        // (1.16.5 Entity.java:643; 1.17 Entity.java:570)
        && !(state.removed && release.at_least(1, 17, 0))
        && y_collision
        && (!release.at_least(1, 21, 2) || state.controlled_by_local_instance)
        // (Math.abs(delta.y) > 0.0, false for NaN: 26.2 Entity.java:798, 820)
        && (!release.at_least(26, 2, 0) || requested_y.abs() > 0.0 || horizontal_collision)
}

/// Player.stabAttack's recoil on the attacker, from 1.21.11, where spears come: causeExtraKnockback
/// takes 0.6 of the horizontal motion and ends the sprint (1.21.11 Player.java:1070-1080). It runs
/// once with 0.4F + the weapon's knockback (1.21.11 :1167; 26.1, 26.1.1 and 26.1.2 by their jars),
/// from 26.2 once with 0.4F and again with the knockback when that is above 0 (26.1.2
/// Player.java:1219 calls it once; 26.2 Player.java:1152-1153 twice, 26.3 :1142-1143). Only the
/// server runs it: a client sends STAB and stops there (26.3 MultiPlayerGameMode.java:513-516; 1.21.11 :490-495) and LivingEntity.stabAttack is the server's
/// (26.3 LivingEntity.java:2815-2818, ItemStack.java:1093).
fn take_stab_hit(state: &mut SimulationState, release: MinecraftVersion) {
    let Some(knockback) = state.stab_hit.take() else {
        return;
    };
    if state.client_side || !release.at_least(1, 21, 11) {
        return;
    }
    let takes = if release.at_least(26, 2, 0) && knockback > 0.0 {
        2
    } else {
        1
    };
    for _ in 0..takes {
        state.velocity.x *= 0.6;
        state.velocity.z *= 0.6;
    }
    state.sprinting = false;
}

/// An impulse straight up or down. From 1.14 it is Vec3.add(0, y, 0), which clears the sign of a
/// horizontal -0.0 (26.3 LivingEntity.java:2317-2323, LocalPlayer.java:861; 1.14.4
/// LocalPlayer.java:684-698); before, it is a sum on motionY alone.
fn add_vertical_impulse(state: &mut SimulationState, y: f64, release: MinecraftVersion) {
    if release.at_least(1, 14, 0) {
        state.velocity = state.velocity + Vec3::new(0.0, y, 0.0);
    } else {
        state.velocity.y += y;
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
    // Entity.tickCount is a Java int: `++this.tickCount` wraps (26.3 Entity.java:250, 550)
    state.tick_count = state.tick_count.wrapping_add(1);
    // the landing damage is this tick's: check_fall_damage sets it only on a landing
    state.fall_damage = 0.0;
    take_stab_hit(state, rules.release);
    // a removed player's tick runs Entity.baseTick, its fluid pushes, fire and lava halving, and skips
    // only aiStep (D-215: 1.21.4 LivingEntity.java tick `if (!this.isRemoved()) this.aiStep()`,
    // LocalPlayer.java:836-840; 1.19.1 the same)
    if state.removed && rules.release.at_least(1, 19, 1) {
        state.inside_wall = is_in_wall(state, world, rules.release);
        base_tick_fluids_and_fire(state, world, &rules);
        phase::emit(Phase::AfterBaseTick, state);
        return TickResult {
            displacement: Vec3::ZERO,
            collided_horizontally: state.collided_horizontally,
            collided_vertically: state.collided_vertically,
            block_fall_callback: false,
        };
    }
    // (1.16.5 Entity.java:631-632; 1.17 Entity.java:559)
    state.bounding_box = (!rules.release.at_least(1, 17, 0)).then(|| kept_bounding_box(state));
    state.contact_damage = 0.0;
    state.wall_damage = 0.0;
    state.block_fall_callback = false;
    // A passenger's tick is Entity.rideTick: the motion goes, the whole tick runs (LocalPlayer.aiStep,
    // travel and its move included) and the vehicle's positionRider then overwrites where the move
    // went, with the fall distance reset (1.8.8 Entity.java:1316-1325, LivingEntity.java:1508-1513;
    // 1.12.2 Entity.java:1629-1639, 1.20.4 :1850-1854; 26.3 Entity.java:2465-2470,
    // LivingEntity.java:3233-3236). The engine has no vehicle: the position a caller hands in is the
    // seat, positionRider's result for this tick, and the tick ends on it. Moving the seat with the
    // vehicle is the caller's, as is leaving it (clear `passenger`).
    let seat = state.passenger.then_some(state.position);
    // Entity.setOnGround(false) empties mainSupportingBlockPos and onGroundNoBlocks (1.21.11
    // Entity.java:680-684), and nothing else takes an entity off the ground: a state handed in off
    // it (a player Player.tick kept off it while it rode, :223-226, or a new one after a respawn)
    // keeps no block from before.
    if !state.on_ground {
        state.supporting_block = None;
        state.on_ground_no_blocks = false;
    }
    if state.is_player && state.no_physics {
        state.on_ground = false;
        check_supporting_block(world, state, None, rules.release);
    }
    // a motion handed in that is not finite: from 1.21.11 setDeltaMovement refuses it, and the
    // entity keeps the motion it had, which a state without a tick before knows only as
    // Entity.deltaMovement's initial Vec3.ZERO (1.21.11 Entity.java:3727-3732); before, it is
    // taken as it is (1.21.10 Entity.java:3709-3711)
    if !state.velocity.is_finite() && rules.release.at_least(1, 21, 11) {
        state.velocity = Vec3::ZERO;
    }
    // Keep the profile's gravity, drag and step height when callers use the default config; an
    // explicitly customized runtime value remains an override.
    let default_config = PhysicsConfig::default();
    if config.gravity != default_config.gravity {
        rules.gravity = config.gravity;
    }
    if config.drag != default_config.drag {
        rules.air_drag = config.drag;
    }
    if config.step_height != default_config.step_height {
        rules.step_height = config.step_height;
    }
    // The GRAVITY and STEP_HEIGHT attributes read through RangedAttribute.sanitizeValue: NaN is the
    // minimum, the rest is clamped to -1..1 and 0..10 (1.21.11 RangedAttribute.java:30-32,
    // Attributes.java:26-66, 42-80; NaN maps to the minimum from 1.19, 1.19 RangedAttribute.java:32,
    // and both attributes exist from 1.20.5).
    let sanitize = |value: f64, min: f64, max: f64| {
        if value.is_nan() {
            min
        } else {
            value.clamp(min, max)
        }
    };
    if let Some(gravity) = state.gravity {
        rules.gravity = sanitize(gravity, -1.0, 1.0);
    }
    if let Some(step_height) = state.step_height {
        rules.step_height = sanitize(step_height, 0.0, 10.0);
    }
    // the release of the use key, unless the scene ran it where the client does, before the
    // entities' ticks (D-214, versioned::release_riptide). The lift is a move of this tick's
    // movementThisTick: TridentItem.releaseUsing runs before the tick sets its old position, and
    // the list is cleared only by the tick's applyEffectsFromBlocks (26.3 Entity.java:788, 946-957)
    if !std::mem::take(&mut state.riptide_released) {
        let fall_rules = FallDamageRules {
            landing_damage_rounding,
            restitution_enabled,
            modern_fall_damage,
        };
        release_riptide(state, world, rules, fall_rules);
    }
    let lift_movement = state.riptide_lift_movement.take();
    // firework_boost is a rocket's FireworkRocketEntity.tick, which runs before the player's and after the use key's release (D-214): all of
    // the player's tick reads the boosted motion (1.20.4 LivingEntity.java:2036-2043, 26.3
    // FireworkRocketEntity.java:123-128, LivingEntity.java:2337-2339)
    // (no rocket adds nothing: adding a zero would turn a motion of -0.0 into 0.0)
    if state.firework_boost != Vec3::ZERO {
        state.add_delta_movement(state.firework_boost, rules.release);
    }
    phase::emit(Phase::TickStart, state);
    // the rockets and the release of the use key come before the rider's tick, whose rideTick
    // takes what they added away; Player.tick then takes a passenger off the ground from 1.21.4
    // (1.21.3 Player.java:231, 1.21.4 :234-235, 26.3 Player.java:219-221)
    if seat.is_some() {
        state.velocity = Vec3::ZERO;
        if state.is_player && rules.release.at_least(1, 21, 4) {
            state.on_ground = false;
            state.supporting_block = None;
            state.on_ground_no_blocks = false;
        }
    }
    let old_position = state.position;
    let flight_resets_fall_before_travel = rules.release.at_least(1, 21, 2);
    let effects_reset_fall_before_travel = rules.release.at_least(1, 20, 0);
    state.jump_ticks = state.jump_ticks.saturating_sub(1);
    let requested_jump = state.jump_requested;
    if state.auto_jump_time > 0 {
        state.auto_jump_time -= 1;
        state.jump_requested = true;
    }
    let body_position = (
        floor_i32_for(rules.release, state.position.x),
        floor_i32_for(rules.release, state.position.y),
        floor_i32_for(rules.release, state.position.z),
    );
    let previous_movement = state
        .previous_position
        .map(|previous| state.position - previous);
    let mut support = kept_supporting_block(world, state, previous_movement, rules.release);
    let below_position = block_below_affecting_movement(world, state, support, rules.release);
    let eye_height = state.eye_height.unwrap_or(state.height * 0.85);
    state.inside_wall = is_in_wall(state, world, rules.release);
    let affected_by_fluids = !(state.is_player && state.flying);
    let legacy_fluids = legacy_fluid::applies(rules.release);
    let (water, lava) = base_tick_fluids_and_fire(state, world, &rules);
    phase::emit(Phase::AfterBaseTick, state);
    // !isPassenger from 26.3 (26.2 LocalPlayer.java:753, 26.3 :774)
    if state.is_player
        && !state.no_physics
        && !(state.passenger && rules.release.at_least(26, 3, 0))
    {
        move_towards_closest_space(state, world, rules.release);
    }
    let sinks_in_water =
        !legacy_fluids && (affected_by_fluids || !rules.release.at_least(1, 16, 0));
    if state.is_player && state.in_water && state.sneaking && sinks_in_water {
        add_vertical_impulse(state, -0.03999999910593033, rules.release);
    }
    if state.flying && !rules.release.at_least(1, 14, 0) {
        // Before 1.14 sneak and jump are two separate double steps, so holding both is not
        // a no-op in double (1.8.8 LocalPlayer.java:594-601, 1.13.2 :792-800).
        let impulse = f64::from(state.flying_speed as f32 * 3.0);
        if state.sneaking {
            state.velocity.y -= impulse;
        }
        if state.jump_requested {
            state.velocity.y += impulse;
        }
    } else if state.flying {
        // 1.14 nets the input into one impulse (1.14.4 LocalPlayer.java:684-698).
        let direction = i8::from(state.jump_requested) - i8::from(state.sneaking);
        if direction != 0 {
            let impulse = f64::from(f32::from(direction) * state.flying_speed as f32 * 3.0);
            add_vertical_impulse(state, impulse, rules.release);
        }
    }
    phase::emit(Phase::BeforeLivingAiStep, state);
    // LivingEntity.aiStep clears jumping, xxa and zza of an immobile entity: dead, or a sleeping
    // player (1.8.8 LivingEntity.java:1427-1432, Player.java:441-442; 1.20.4
    // LivingEntity.java:1982-1983, 2563-2566; 1.21.5 :2711; 26.3 :2254-2255, 3000-3003, Player.java:823-824)
    let immobile = state.dead || (state.is_player && state.sleeping);
    let requested_input = state.input;
    if immobile {
        state.jump_requested = false;
        state.input.x = 0.0;
        state.input.z = 0.0;
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
    // Player.travel steers a swimmer while any fluid, lava too, is at (y + 1.0) - 0.1 (1.13.2
    // Player.java:1473, 1.21.4 :1377, 1.21.11 Player.java:1326, 26.3 :1301)
    let swimming_head_position = (
        floor_i32_for(rules.release, state.position.x),
        floor_i32_for(rules.release, state.position.y + 1.0 - 0.1),
        floor_i32_for(rules.release, state.position.z),
    );
    let swimming_head_in_fluid = world.block_cow(swimming_head_position).fluid != FluidState::Empty;
    state.eye_in_water = entity_eyes_in_fluid(state, world, FluidState::Water, rules.release);
    state.eye_in_lava = entity_eyes_in_fluid(state, world, FluidState::Lava, rules.release);
    let vertical_drag = match fluid {
        FluidState::Water => rules.water_drag,
        FluidState::Lava => rules.lava_drag,
        FluidState::Waterlogged => rules.water_drag,
        FluidState::Empty => rules.vertical_drag,
    };
    let jumping = state.jump_requested
        // (1.15.2 LivingEntity.java:2294; 1.16 LivingEntity.java:2458, Player.java:1025-1027)
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
        add_vertical_impulse(state, 0.03999999910593033, rules.release);
    } else if jump == FluidJump::Ground && state.jump_ticks == 0 {
        let jump_factor = block_jump_factor(world, body_position, below_position);
        let power = jump_power(state, config, rules.release, jump_factor);
        jump_from_ground(state, power, rules.release);
        state.jump_ticks = 10;
    }
    if !jumping {
        state.jump_ticks = 0;
    }
    // LivingEntity.aiStep (1.21.2+) checks the fall distance every tick the fall-flying flag is set,
    // before travel and in any fluid (1.21.2 LivingEntity.java:2796, 2865); before, only the glide
    // in travel does (below).
    let fall_flying_checks_before_travel = rules.release.at_least(1, 21, 2);
    if state.fall_flying && fall_flying_checks_before_travel {
        state.check_fall_distance_accumulation();
    }
    // LivingEntity.aiStep (1.20+) resets the fall distance under Slow Falling or Levitation just
    // before travel, after the base tick and the glide's check (1.20 LivingEntity.java:2578-2579,
    // 26.3 :3040-3048); before, travel resets it itself (below).
    if ((state.slow_falling || state.levitation) && effects_reset_fall_before_travel)
        || (state.flying && flight_resets_fall_before_travel)
    {
        state.fall_distance = 0.0;
    }
    phase::emit_before_travel(state, || movement_input(state, rules.release));
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
    // too (1.20.5 LivingEntity.java:2107, Entity.java:1143-1145; 1.21.11 Entity.java:1443-1445);
    // before, the glide's fixed 0.08 ignores NoGravity (1.16.5 LivingEntity.java:1963)
    let travel_gravity = if state.no_gravity && rules.release.at_least(1, 20, 5) {
        0.0
    } else if state.slow_falling && going_down {
        rules.gravity.min(0.01)
    } else {
        rules.gravity
    };
    // LivingEntity.getSpeed reads the attribute with every modifier it holds (1.21.11
    // AttributeInstance.java:150-172), which a caller that has them folds itself
    let movement_speed = state.movement_speed_value.unwrap_or_else(|| {
        movement_speed_attribute(
            state.movement_speed.unwrap_or(config.movement_speed),
            state.speed_effect_amplifier,
            state.slowness_effect_amplifier,
            state.sprint_modifier.unwrap_or(state.sprinting),
            config.sprint_multiplier,
            rules.release,
        )
    });
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
    let mut glide_speed = None;
    if gliding {
        // LivingEntity.travel's glide: from 1.19.3 Entity.checkSlowFallDistance lowers the fall
        // distance to 1.0 (1.19.3 Entity.java:2169-2171, LivingEntity.java:2120); 1.9 - 1.19.2 set
        // it to 1.0 outright, raising a smaller one (1.9.4 LivingEntity.java:1328-1329, 1.19.2
        // :2105-2107)
        if !fall_flying_checks_before_travel {
            if rules.release.at_least(1, 19, 3) {
                state.check_fall_distance_accumulation();
            } else if state.velocity.y > -0.5 {
                state.fall_distance = 1.0;
            }
        }
        glide_speed = Some(horizontal_speed(state.velocity));
        glide(state, travel_gravity, rules.release);
    } else if state.flying {
        state.velocity.y = flying_vertical_velocity;
    }
    if climbing && matches!(fluid, FluidState::Empty) && !gliding {
        let limit = java_float(0.15);
        state.fall_distance = 0.0;
        // Mth.clamp(double) (LivingEntity.handleOnClimbable, 1.21.11 Mth.java:115-117).
        state.velocity.x = clamp_f64_for(rules.release, state.velocity.x, -limit, limit);
        state.velocity.z = clamp_f64_for(rules.release, state.velocity.z, -limit, limit);
        let fall_limit = if rules.release.at_least(1, 14, 0) {
            limit
        } else {
            // a double before 1.14 (1.8.8 LivingEntity.java:1163-1164, 1.13.2 :1595-1596)
            0.15
        };
        state.velocity.y = state.velocity.y.max(-fall_limit);
        let suppresses_sliding_down =
            state.sneaking && !(state.flying && rules.release.at_least(1, 16, 0));
        if state.is_player && suppresses_sliding_down && !on_scaffolding && state.velocity.y < 0.0 {
            state.velocity.y = 0.0;
        }
    }
    phase::emit(Phase::BeforeMove, state);
    state.move_slowed =
        !state.no_physics && state.stuck_speed_multiplier.length_squared() > 1.0e-7;
    // the tick's Entity.move(SELF); the flag tells whether any move of this tick ended in the block's
    // fall callback
    state.block_fall_callback = false;
    let own_move = entity_move(
        state,
        world,
        state.velocity,
        MoverType::SelfOrPlayer,
        rules,
        FallDamageRules {
            landing_damage_rounding,
            restitution_enabled,
            modern_fall_damage,
        },
        support,
    );
    let desired = own_move.desired;
    let displacement = own_move.displacement;
    support = own_move.support;
    if let Some(lava_height) = own_move.fall.and_then(|fall| fall.lava_height) {
        fluid_height = lava_height;
    }
    let removed_in_move = own_move.removed;
    let mut inside_after_move = own_move.inside;
    let jump_factor = block_jump_factor(
        world,
        block_position(state.position, rules.release),
        block_below_affecting_movement(world, state, support, rules.release),
    );
    // only xxa, zza and jumping are cleared: the keys stay in LocalPlayer.input, which the auto-jump
    // reads after the move (1.20.4 LocalPlayer.java:884, 1006; 26.3 LocalPlayer.java:983, 1003,
    // 1110-1114), so a pushed sleeper can arm the jump its first tick awake takes
    if immobile {
        state.input = requested_input;
    }
    // LocalPlayer.move runs updateAutoJump after super.move, the block effects and speed factor
    // included (1.20.4 LocalPlayer.java:883-884, 26.3 LocalPlayer.java:980-983)
    update_auto_jump(
        state,
        world,
        state.position - old_position,
        movement_speed as f32,
        jump_factor,
        rules.release,
    );
    phase::emit(Phase::AfterMove, state);
    if let Some(speed) = glide_speed {
        deal_wall_damage(state, speed, rules.release);
    }
    let climbs_powder_snow = state.powder_snow_walkable
        && if rules.release.at_least(1, 21, 5) {
            was_in_powder_snow
        } else {
            world.effect_at(block_position(state.position, rules.release)) == BlockEffect::PowderSnow
        };
    // From 1.21.2 travelInAir takes getEffectiveGravity after the move: Slow Falling tests the delta
    // the move left, before handleRelativeFrictionAndCalculateMovement's climb (1.21.2
    // LivingEntity.java:2194-2197, 2213-2229, 2383-2393; 26.3 :2337-2340, 2382-2397); before, travel
    // tests the delta it started with (1.21 LivingEntity.java:2115-2121)
    let air_gravity = if rules.release.at_least(1, 21, 2) && !state.no_gravity {
        if state.slow_falling && state.velocity.y <= 0.0 {
            rules.gravity.min(0.01)
        } else {
            rules.gravity
        }
    } else {
        travel_gravity
    };
    if matches!(fluid, FluidState::Empty)
        && !gliding
        && (state.collided_horizontally
            // (1.13.2 LivingEntity.java:1606-1608; 1.14.4 LivingEntity.java:1835-1837)
            || (state.jump_requested && rules.release.at_least(1, 14, 0)))
        && (climbable(world, state, rules.release) || climbs_powder_snow)
    {
        state.velocity.y = 0.2;
    }
    // a passenger's flight is no flight to Player.travel, which passes it to LivingEntity.travel
    // as it is (1.8.8 Player.java:1296, 1.12.2 :1406, 1.20.4 :1452, 26.3 Player.java:1294-1296)
    let flight_travel = state.flying && !state.passenger;
    if matches!(fluid, FluidState::Empty) && !gliding && !flight_travel {
        if state.levitation {
            let target = 0.05 * (f64::from(state.levitation_amplifier.unwrap_or(0)) + 1.0);
            state.velocity.y += (target - state.velocity.y) * 0.2;
            // LivingEntity.travel's air move resets the fall distance under Levitation after the
            // move, 1.13 - 1.19.4 (1.13.2 LivingEntity.java:1610-1612, 1.19.4 :2103-2105); 1.9 -
            // 1.12 never reset it (1.12.2 LivingEntity.java:1543-1545)
            if rules.release.at_least(1, 13, 0) && !effects_reset_fall_before_travel {
                state.fall_distance = 0.0;
            }
        } else if !rules.release.at_least(1, 14, 0)
            && state.client_side
            && !column_loaded(world, state.position, rules.release)
        {
            // only the client sinks, at world.isClient's test (1.8.8 LivingEntity.java:1177-1181,
            // 1.12.2 :1546); from 1.14.4 MultiPlayerLevel.hasChunk answers true for every chunk (1.14.4
            // MultiPlayerLevel.java:232, LevelReader.java:258; 26.3 ClientLevel.java:490), which also
            // admits every LocalPlayer.tick (1.14.4 LocalPlayer.java:168, 1.20.4 :189), and
            // from 1.21.2 no column is asked there at all (1.21.3 LocalPlayer.java:190)
            state.velocity.y = if state.position.y > 0.0 { -0.1 } else { 0.0 };
        } else if !state.no_gravity {
            state.velocity.y -= air_gravity;
        }
    }
    let horizontal_drag = if gliding { 1.0 } else { ground_inertia };
    if matches!(fluid, FluidState::Empty) {
        state.velocity.x *= horizontal_drag;
        state.velocity.z *= horizontal_drag;
    } else {
        let after_move = FluidTravelAfterMove {
            fluid,
            fluid_height,
            fluid_jump_threshold,
            water_slowdown,
            gravity: travel_gravity,
            going_down,
            start_y: old_position.y,
        };
        FluidTravelHandler::for_rules(rules).after_move(state, world, rules, &after_move);
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
    if flight_travel {
        state.velocity.y = flying_vertical_velocity * 0.6;
        // Player.travel's flight resets the fall distance and the glide from 1.9 (1.9.4
        // Player.java:1396-1397); 1.8 only scales the vertical motion (1.8.8
        // Player.java:1296-1302)
        if !flight_resets_fall_before_travel && rules.release.at_least(1, 9, 0) {
            state.fall_distance = 0.0;
            state.fall_flying = false;
        }
    }
    // LivingEntity.stopFallFlying, after the air travel
    if glide_stopped {
        state.fall_flying = false;
    }
    phase::emit(Phase::AfterTravel, state);

    let mut inside_effects = InsideEffects::default();
    let block_effects = applies_effects_from_blocks(state, rules.release);
    if rules.release.at_least(1, 21, 2) && block_effects {
        let mut movements =
            movements_this_tick(state, old_position, desired, displacement, rules.release);
        if let Some(lift) = lift_movement {
            movements.insert(0, lift);
        }
        // (the shulker boxes' pushes since the last tick are first, in their order: the list holds
        // them until this applyEffectsFromBlocks, 26.3 Entity.java:960-963)
        let pending = std::mem::take(&mut state.pending_movements);
        movements.splice(0..0, pending.as_slice().iter().copied());
        let (inside, effects) = blocks_inside_moves(state, world, &movements, rules.release);
        apply_block_effects(state, world, support, rules.release, &inside);
        inside_after_move = inside;
        inside_effects = effects;
    }
    if state.no_physics || !block_effects {
        inside_after_move.clear();
        inside_effects = InsideEffects::default();
    }
    if !removed_in_move && block_effects {
        apply_inside_fire_effects(
            state,
            world,
            &inside_after_move,
            &inside_effects,
            rules.release,
        );
    }
    let moved = state.position - old_position;
    apply_inside_block_signals(state, world, &inside_after_move, moved, rules.release);
    if state.is_player && state.flying && state.on_ground && !state.no_physics {
        state.flying = false;
    }
    // (1.13.2 Entity.java:1143-1145; 1.14.4 Entity.java:568-569, 1171-1173)
    if !rules.release.at_least(1, 14, 0) {
        state.in_lava = legacy_fluid::touching_lava(world, state.bounds(), rules.release);
    }
    state.was_sprinting = state.sprinting;
    state.previous_position = Some(old_position);
    // the state keeps what the caller asks for, as LocalPlayer.input does: the tick after waking
    // reads the keys, whichever way the state came back
    if immobile {
        state.jump_requested = requested_jump;
    }
    let displacement = if let Some(seat) = seat {
        state.position = seat;
        state.fall_distance = 0.0;
        // Entity.setPos fits the box to the position again (1.12.2 Entity.java:406, 1.16.5 :469)
        state.bounding_box = None;
        Vec3::ZERO
    } else {
        displacement
    };

    TickResult {
        displacement,
        collided_horizontally: state.collided_horizontally,
        collided_vertically: state.collided_vertically,
        block_fall_callback: state.block_fall_callback,
    }
}
#[cfg(test)]
#[path = "simulation_gate_tests.rs"]
mod gate_fixtures;
