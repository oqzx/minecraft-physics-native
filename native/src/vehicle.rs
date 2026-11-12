use crate::block_traversal::Movement;
use crate::collision::{
    collide_legacy, collide_shapes, sort_in_cursor_order, vanilla_equal, Axis, BlockVoxelShape,
    Colliders, CollisionRules, StepUp,
};
use crate::entity::EntityState;
use crate::legacy_fluid;
use crate::math::{
    atan2_vanilla, ceil_i32_for, floor_i32, floor_i32_for, math_cos, math_sin, wrap_degrees_f32, Aabb, Vec3,
};
use crate::moving_entity::count_fall;
use crate::simulation::{
    apply_block_effects, apply_inside_block_effects, block_position, block_speed_factor, blocks_inside,
    can_simulate_movement,
    blocks_inside_moves, fluid_falling_adjusted_y, friction_influenced_speed, landing_position,
    entity_collision_query, main_supporting_block, movements_this_tick_from, moves_position, mth_cos, mth_sin,
    position_of_bounding_box, push_by_fluid, relative_movement, water_contact, yaw_radians,
    kept_supporting_block, refreshes_vertical_collision, reset_fall_on_clip, SimulationState,
    VersionPhysicsRules,
};
use crate::version::MinecraftVersion;
use crate::world::{
    shapes_join, BlockEffect, BlockState, CollisionQueryEntity, EntityCollisionContext, FluidState,
    WorldPatches, WorldSnapshot,
};

type BlockPosition = (i32, i32, i32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VehicleKind {
    Boat,
    Horse,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoatStatus {
    #[default]
    InAir,
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleState {
    pub kind: VehicleKind,
    pub entity: EntityState,
    pub passenger: Option<EntityState>,
}

/// A boat as Boat.tick keeps it between ticks. `yaw_degrees` is vanilla's float yRot and
/// `delta_rotation` its float deltaRotation; `bounding_box` is the box an entity keeps before
/// 1.17. Without a known status of the tick before (`settle_status`) the status is taken where
/// the boat is. `stuck_speed_multiplier` is Entity.stuckSpeedMultiplier, which a cobweb or powder
/// snow the boat is inside sets for its next move; `ultra_warm` its dimension's (the fast lava);
/// `no_gravity` its synced NoGravity flag (Entity.isNoGravity, 1.10+).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoatSimulationState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f64,
    pub yaw_degrees: f32,
    pub forward: bool,
    pub left: bool,
    pub right: bool,
    pub input_down: bool,
    pub delta_rotation: f64,
    pub in_water: bool,
    pub water_level: f64,
    pub land_friction: f32,
    pub status: BoatStatus,
    pub previous_status: BoatStatus,
    pub settle_status: bool,
    pub last_yd: f64,
    /// Entity.fallDistance: what Boat.checkFallDamage counts of a drop that is not over water, and
    /// powder snow's collision reads (FALLING_COLLISION_SHAPE above 2.5).
    pub fall_distance: f64,
    pub bubble_time: i32,
    pub bubble_column_direction_is_down: bool,
    pub above_bubble_column: bool,
    pub server_side: bool,
    pub player_passenger: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub on_ground: bool,
    pub bounding_box: Option<Aabb>,
    pub stuck_speed_multiplier: Vec3,
    pub ultra_warm: bool,
    pub no_gravity: bool,
    /// Boat.paddlePositions (1.9+): each paddle's rowing phase, which grows while that paddle
    /// moves and is 0 while it rests.
    pub paddle_phases: [f32; 2],
    /// Entity.isSilent: a silent boat plays no paddle sound, so takes no second status.
    pub silent: bool,
    /// A move past MAX_WALKED_CELLS did not run, and the bindings refuse the tick (`move_vehicle`).
    pub move_out_of_range: bool,
    pub legacy: LegacyBoatState,
    /// The interpolation the server's position packets started (AbstractBoat.lerpSteps and its
    /// target), which 1.21.2 to 1.21.4 run even on a boat the client controls (`tick_lerp`).
    pub lerp: BoatLerp,
}

/// AbstractBoat.lerpSteps, lerpX/Y/Z, lerpYRot and lerpXRot (1.21.2 AbstractBoat.java:65-70).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoatLerp {
    pub steps: i32,
    pub target: Vec3,
    pub y_rot: f64,
    pub x_rot: f64,
}

/// What 1.8's EntityBoat.onUpdate keeps and reads besides the boat: how hard its rider's input
/// pushes it (speedMultiplier, 0.07 to 0.35) and its rider's yaw, moveForward and moveStrafing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyBoatState {
    pub speed_multiplier: f64,
    pub rider_yaw: f32,
    pub rider_forward: f32,
    pub rider_strafe: f32,
}

impl Default for LegacyBoatState {
    fn default() -> Self {
        Self {
            speed_multiplier: 0.07,
            rider_yaw: 0.0,
            rider_forward: 0.0,
            rider_strafe: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HorseSimulationState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f64,
    pub forward: f64,
    pub strafe: f64,
    pub jump_requested: bool,
    pub on_ground: bool,
    pub in_water: bool,
    pub movement_speed: f64,
    pub jump_strength: f64,
    pub half_width: f64,
    pub height: f64,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    /// getFluidHeight(WATER) while `in_water`, which the world-less horse cannot measure.
    pub water_height: f64,
    /// The synced NoGravity flag (Entity.isNoGravity): travel takes no gravity.
    pub no_gravity: bool,
}

/// The seat a boat gives one of its passengers: the passenger's index among the boat's passengers
/// and their count, whether it is an Animal (0.2F further forward with two aboard), whether the
/// boat is a chest boat (its one passenger 0.15F forward, from 1.19) or a raft (seated higher,
/// from 1.19.3), and the passenger's own offset under the seat as the release takes it:
/// getHeightOffset before 1.20.2 (a player's -0.35), getRidingOffset in 1.20.2 - 1.20.4 (a
/// player's -0.6F), less its vehicle attachment's height from 1.20.5 (a player's 0.6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatSeat {
    pub index: usize,
    pub passengers: usize,
    pub animal: bool,
    pub chest: bool,
    pub raft: bool,
    pub passenger_offset: f64,
}

impl BoatSeat {
    /// A player alone in a plain boat (1.8.8 Player.java:977-979, 1.20.2
    /// Player.java:1022-1024, 1.20.5 Player.DEFAULT_VEHICLE_ATTACHMENT). (1.20.4 Player.java:1025
    /// returns -0.6F; 1.20.5 Player.java:2047 attaches it at 0.6.)
    #[must_use]
    pub fn player(release: MinecraftVersion) -> Self {
        let passenger_offset = if !release.at_least(1, 20, 2) {
            -0.35
        } else if !release.at_least(1, 20, 5) {
            f64::from(-0.6_f32)
        } else {
            -0.6
        };
        Self {
            index: 0,
            passengers: 1,
            animal: false,
            chest: false,
            raft: false,
            passenger_offset,
        }
    }
}

/// Where a boat puts a passenger after its move (Entity.positionRider, Boat.positionRider):
/// - 1.8: EntityBoat.updateRiderPosition, 0.4 along its yaw by Math.cos and sin, at its mounted
///   offset -0.3 plus the rider's (1.8.8 Boat.java:136-138, 370-376);
/// - 1.9 - 1.20.1: the seat's forward offset (0.2F or -0.6F with two aboard) as Vec3(f, 0, 0)
///   turned by the float -yRot * 0.017453292F - 1.5707964F with Mth's cos and sin, at the float
///   sum of the boat's mounted offset (-0.1; a raft's 0.3, from 1.20 0.25) and the passenger's
///   (1.9.4 Boat.java:570-588, 1.17 :650-668, 1.19.3 :153-155, 1.20 :153-155, 671-694);
/// - 1.20.2 - 1.20.4: getPassengerAttachmentPos, JOML's float Vector3f(0, height / 3.0F (a raft's
///   height * 0.8888889F), offset).rotateY(-yRot * 0.017453292F), plus the passenger's float
///   riding offset (1.20.2 Boat.java:155-171, Entity.java:1856-1877);
/// - from 1.20.5: Vec3(0, rideHeight, offset).yRot(-yRot * 0.017453292F) with Mth's cos and sin,
///   less the passenger's vehicle attachment (1.20.5 Boat.java:139-155, Entity.java:
///   1876-1891, Vec3.java:204-211; 1.21.11 AbstractBoat.java:132-147, Boat.java:14-16,
///   Raft.java:14-16).
#[must_use]
pub fn boat_passenger_position(
    boat: &BoatSimulationState,
    seat: BoatSeat,
    release: MinecraftVersion,
) -> Vec3 {
    let position = boat.position;
    if !release.at_least(1, 9, 0) {
        let radians = f64::from(boat.yaw_degrees) * std::f64::consts::PI / 180.0;
        return Vec3::new(
            position.x + math_cos(radians, release) * 0.4,
            position.y + -0.3 + seat.passenger_offset,
            position.z + math_sin(radians, release) * 0.4,
        );
    }
    // (1.18.2 Boat.java:652; 1.19 ChestBoat.java:47)
    let mut offset = if seat.chest && release.at_least(1, 19, 0) {
        0.15_f32
    } else {
        0.0
    };
    if seat.passengers > 1 {
        offset = if seat.index == 0 { 0.2 } else { -0.6 };
        if seat.animal {
            // (1.18.1 Boat.java:640 adds a double, 1.18.2 :663 a float)
            offset = if release.at_least(1, 18, 2) {
                offset + 0.2
            } else {
                (f64::from(offset) + 0.2) as f32
            };
        }
    }
    let yaw = -boat.yaw_degrees * 0.017453292;
    if !release.at_least(1, 20, 2) {
        // (1.19.2 Boat.java:151; 1.19.3 Boat.java:154)
        let mounted = if !seat.raft || !release.at_least(1, 19, 3) {
            -0.1
        } else if release.at_least(1, 20, 0) {
            0.25
        } else {
            0.3
        };
        let height = (mounted + seat.passenger_offset) as f32;
        let angle = yaw - 1.5707964;
        let cos = mth_cos(f64::from(angle), release);
        let sin = mth_sin(f64::from(angle), release);
        let forward = f64::from(offset);
        return Vec3::new(
            position.x + (forward * cos + 0.0 * sin),
            position.y + f64::from(height),
            position.z + (0.0 * cos - forward * sin),
        );
    }
    let ride_height = if seat.raft {
        BOAT_HEIGHT * 0.8888889
    } else {
        BOAT_HEIGHT / 3.0
    };
    // (1.20.4 Entity.java:1880-1881; 1.20.5 Boat.java:154)
    if !release.at_least(1, 20, 5) {
        let sin = math_sin(f64::from(yaw), release) as f32;
        let cos = joml_cos_from_sin(sin, yaw);
        let x = 0.0 * cos + offset * sin;
        let z = -0.0 * sin + offset * cos;
        return Vec3::new(
            f64::from(x) + position.x,
            f64::from(ride_height) + position.y + seat.passenger_offset,
            f64::from(z) + position.z,
        );
    }
    let cos = mth_cos(f64::from(yaw), release);
    let sin = mth_sin(f64::from(yaw), release);
    let forward = f64::from(offset);
    Vec3::new(
        position.x + (0.0 * cos + forward * sin),
        position.y + f64::from(ride_height) + seat.passenger_offset,
        position.z + (forward * cos - 0.0 * sin),
    )
}

/// JOML's cosFromSin of a float angle (1.20.2 - 1.20.4's JOML Vector3f rotateY), a library's and no
/// release's own code: the float root of
/// 1 - sin * sin, negative where the angle's cosine is.
fn joml_cos_from_sin(sin: f32, angle: f32) -> f32 {
    const PI_HALF: f32 = std::f32::consts::FRAC_PI_2;
    const PI2: f32 = std::f32::consts::TAU;
    const PI: f32 = std::f32::consts::PI;
    let cos = f64::from(1.0 - sin * sin).sqrt() as f32;
    let a = angle + PI_HALF;
    let mut b = a - (a / PI2) as i32 as f32 * PI2;
    if b < 0.0 {
        b += PI2;
    }
    if b >= PI {
        -cos
    } else {
        cos
    }
}

/// The yaw Boat.positionRider leaves a passenger outside CAN_TURN_IN_BOATS (a player) with from
/// 1.9: turned by the boat's deltaRotation, then held within 105 degrees of the boat's yaw
/// (clampRotation: yRot += clamped - wrapped before 1.17, setYRot(getYRot() + clamped - wrapped)
/// from 1.17; 1.9.4 Boat.java:589-607, 1.16.5 :673-675, 715-722, 1.17 :669-671, 716-723,
/// 1.21.11 AbstractBoat.java:595-607, 645-652). 1.8's boat does not turn its rider.
#[must_use]
pub fn boat_passenger_yaw(
    passenger_yaw: f32,
    boat: &BoatSimulationState,
    release: MinecraftVersion,
) -> f32 {
    if !release.at_least(1, 9, 0) {
        return passenger_yaw;
    }
    let yaw = passenger_yaw + boat.delta_rotation as f32;
    let wrapped = wrap_degrees_f32(yaw - boat.yaw_degrees);
    let clamped = wrapped.clamp(-105.0, 105.0);
    // (1.16.5 Boat.java:720; 1.17 Boat.java:721)
    if release.at_least(1, 17, 0) {
        yaw + clamped - wrapped
    } else {
        yaw + (clamped - wrapped)
    }
}

/// Ticks a boat in its world first, then seats its passenger at the boat's new position (the
/// passenger's Entity#rideTick, which has the vehicle's Boat#positionRider place it).
pub fn tick_boat_with_passenger(
    vehicle: &mut BoatSimulationState,
    world: &mut WorldSnapshot,
    release: MinecraftVersion,
    passenger: &mut EntityState,
    seat: BoatSeat,
) {
    tick_boat_in_world(vehicle, world, release);
    passenger.position = boat_passenger_position(vehicle, seat, release);
}

/// A boat without a world (simulateBoatTick), at the default release: Boat.floatBoat in the water
/// of `water_level` while `in_water`, else on land of `land_friction` while `on_ground` and that
/// friction is positive, else in the air (not set onto a water surface, which is the world's),
/// then controlBoat with its keys and a move nothing stops.
pub fn tick_boat(state: &mut BoatSimulationState) {
    let release = VersionPhysicsRules::default().release;
    state.status = if state.in_water {
        BoatStatus::InWater
    } else if state.on_ground && state.land_friction > 0.0 {
        BoatStatus::OnLand
    } else {
        BoatStatus::InAir
    };
    state.previous_status = state.status;
    let ground_friction = state.land_friction;
    float_boat_by_status(state, release);
    // floatBoat halves landFriction for a player's boat, but getStatus takes it from the ground
    // again before every floatBoat (26.2 AbstractBoat.java:197, 370): the ground's friction is
    // what the next tick is given
    state.land_friction = ground_friction;
    control_boat(state, release);
    state.position = state.position + state.velocity;
    state.yaw = f64::from(state.yaw_degrees).to_radians();
}

/// The boat's width and height: EntityType.BOAT's 1.375 by 0.5625.
const BOAT_HALF_WIDTH: f64 = 0.6875;
const BOAT_HEIGHT: f32 = 0.5625;

/// One client tick of a boat its player rides and steers (Boat.tick, AbstractBoat.tick from
/// 1.21.2): its status where it is, the fluids' push, floatBoat, controlBoat with the keys its
/// rider gave it, then Entity.move. What the move clears before 1.13 (legacy_boat_world) is
/// written into `world` for the move and taken back after it.
pub fn tick_boat_in_world(
    state: &mut BoatSimulationState,
    world: &mut WorldSnapshot,
    release: MinecraftVersion,
) {
    // (1.8.8 Boat.java:180-367; 1.9.4 Boat.java:223-297)
    if !release.at_least(1, 9, 0) {
        tick_legacy_boat(state, world, release);
        return;
    }
    // (1.16.5 Entity.java:631-632; 1.17 Entity.java:561)
    if !release.at_least(1, 17, 0) && state.bounding_box.is_none() {
        state.bounding_box = Some(boat_box_at(state.position));
    }
    let old_position = state.position;
    if state.settle_status {
        state.status = boat_status(state, world, release);
        state.settle_status = false;
    }
    state.previous_status = state.status;
    state.status = boat_status(state, world, release);
    push_boat_by_fluids(state, world, release);
    tick_lerp(state, release);
    float_boat(state, world, release);
    // (controlBoat is the client's: 26.3 AbstractBoat.java:222-226)
    if !state.server_side {
        control_boat(state, release);
    }
    let mut body = boat_body(state, release);
    let move_start = body.position;
    let velocity = state.velocity;
    state.move_out_of_range |= !lily_pad_reach(body.bounds(), velocity).is_walkable();
    let mut cleared = WorldPatches::default();
    legacy_boat_world(world, &mut cleared, release, body.bounds(), velocity, None);
    let move_world: &WorldSnapshot = world;
    // the blocks inside are checked after the fall in the move before 1.21.2, and a cobweb or
    // powder snow there ends it (1.20.4 Entity.java:759-766, makeStuckInBlock)
    let mut ended_inside = false;
    let outcome = move_vehicle(
        &mut body,
        move_world,
        release,
        velocity,
        MoveRules::with_gravity_of(MoveRules::BOAT, state.no_gravity),
        |body, support| {
            let fall = body.fall_distance;
            body.fall_distance = 1.0;
            apply_effects_inside(body, move_world, support, release);
            ended_inside = body.fall_distance == 0.0;
            body.fall_distance = fall;
        },
    );
    world.restore(&mut cleared);
    // the client checks the fall of a boat it controls from 1.21.2, and 1.21.4 not of one a player
    // controls (1.21.2 Entity.java:721-724, 1.21.4 :721)
    let checks_fall = !(state.player_passenger && release.at_least(1, 21, 4) && !release.at_least(1, 21, 5))
        && (!release.at_least(1, 21, 2) || state.server_side || state.player_passenger);
    if checks_fall {
        count_boat_fall(&mut body, world, outcome.moved.y, release);
    }
    if ended_inside {
        body.fall_distance = 0.0;
    }
    apply_boat_block_effects(&mut body, world, release, old_position, move_start, outcome);
    if let (true, Some(drag_down)) = (state.server_side, body.above_bubble_column) {
        state.above_bubble_column = true;
        state.bubble_column_direction_is_down = drag_down;
        if state.bubble_time == 0 {
            state.bubble_time = 60;
        }
    }
    // (1.21.4 alone: Entity.move skips checkFallDamage, which keeps lastYd, for a boat a player
    // controls; 1.21.4 Entity.java:721)
    if !(state.player_passenger && release.at_least(1, 21, 4) && !release.at_least(1, 21, 5)) {
        state.last_yd = outcome.motion_at_landing.y;
    }
    state.stuck_speed_multiplier = body.stuck_speed_multiplier;
    state.fall_distance = body.fall_distance;
    state.move_out_of_range |= body.move_out_of_range;
    state.position = body.position;
    state.velocity = body.velocity;
    state.bounding_box = body.bounding_box;
    state.on_ground = body.on_ground;
    state.collided_horizontally = body.collided_horizontally;
    state.collided_vertically = body.collided_vertically;
    state.yaw = f64::from(state.yaw_degrees).to_radians();
    if state.server_side {
        tick_bubble_column(state);
    }
    advance_paddles(state, world, release);
}

/// AbstractBoat.tickLerp after super.tick: one step of the interpolation a position packet
/// started, Entity.lerpPositionAndRotationStep (1.21.4 AbstractBoat.java:361-366, Entity.java:3657-3666).
/// 1.21.2 to 1.21.4 run it whether or not the boat is controlled here; 1.21.1 Boat.tickLerp
/// zeroes the steps of a boat the client controls first (Boat.java:392-396), and 1.21.5 hands it
/// to an InterpolationHandler that does the same. The replay seeds the steps where the
/// recording has them, so a release outside the range never has a step to take.
fn tick_lerp(state: &mut BoatSimulationState, release: MinecraftVersion) {
    if !(release.at_least(1, 21, 2) && !release.at_least(1, 21, 5)) || state.lerp.steps <= 0 {
        return;
    }
    let delta = 1.0 / f64::from(state.lerp.steps);
    let lerp = |from: f64, to: f64| from + delta * (to - from);
    let target = state.lerp.target;
    state.position = Vec3::new(lerp(state.position.x, target.x), lerp(state.position.y, target.y), lerp(state.position.z, target.z));
    // (Mth.rotLerp: the shorter way round)
    let y_rot = f64::from(state.yaw_degrees) + delta * crate::math::wrap_degrees(state.lerp.y_rot - f64::from(state.yaw_degrees));
    state.yaw_degrees = y_rot as f32;
    state.yaw = f64::from(state.yaw_degrees).to_radians();
    state.lerp.steps -= 1;
}

/// AbstractBoat.tickBubbleColumn on the server: 60 ticks over a column end with a push, down or up,
/// and the timer stops on the tick it is not over one. A boat with no player (26.3
/// AbstractBoat.java:276-336, 1.13.2 Boat.java:355-377): the push a player's boat takes is
/// not the server's to make, as that boat ticks on the client.
fn tick_bubble_column(state: &mut BoatSimulationState) {
    if !state.above_bubble_column {
        state.bubble_time = 0;
    }
    if state.bubble_time > 0 {
        state.bubble_time -= 1;
        if state.bubble_time == 0 {
            if state.bubble_column_direction_is_down {
                state.velocity.y -= 0.7;
            } else {
                state.velocity.y = 0.6;
            }
        }
        state.above_bubble_column = false;
    }
}

/// The paddle loop at the end of Boat.tick, after the move. The client set the paddles from its
/// rider's keys (Boat.controlBoat: the right paddle rows for right without left or forward, the
/// left for left without right or forward). From 1.12 a paddle at the start of its stroke plays
/// its sound, and getPaddleSound takes the boat's status again where it now is: the land
/// friction and water level are taken anew there, so a player's boat keeps the unhalved ground
/// friction on such a tick (1.12.2 Boat.java, 1.20.4 Boat.java:317-337,
/// 1.21.11 AbstractBoat.java:255-270, 26.3 AbstractBoat.java:240-256). The phase grows by
/// PI/8 a tick from 1.12; 1.9-1.11 add 0.01 and play no sound (1.11.2 Boat.java:269; 1.12.2
/// Boat.java:271-283).
fn advance_paddles(state: &mut BoatSimulationState, world: &WorldSnapshot, release: MinecraftVersion) {
    // (a boat whose first passenger is not a player has its paddles stopped)
    let rows = |a: bool, b: bool| state.player_passenger && (a && !b || state.forward);
    let moving = [rows(state.right, state.left), rows(state.left, state.right)];
    let modern = release.at_least(1, 12, 0);
    for (i, moving) in moving.into_iter().enumerate() {
        if !moving {
            state.paddle_phases[i] = 0.0;
            continue;
        }
        if !modern {
            state.paddle_phases[i] = (f64::from(state.paddle_phases[i]) + 0.01) as f32;
            continue;
        }
        let phase = state.paddle_phases[i];
        let turn = std::f64::consts::TAU as f32;
        let quarter = std::f64::consts::FRAC_PI_4 as f32;
        let step = (std::f64::consts::PI / 8.0) as f32;
        if !state.silent && phase % turn <= quarter && (phase + step) % turn >= quarter {
            // the status itself is not kept: getPaddleSound only reads it
            let _ = boat_status(state, world, release);
        }
        state.paddle_phases[i] = phase + step;
    }
}

/// EntityDimensions#makeBoundingBox of the boat's dimensions at `position`.
fn boat_box_at(position: Vec3) -> Aabb {
    Aabb::new(
        Vec3::new(
            position.x - BOAT_HALF_WIDTH,
            position.y,
            position.z - BOAT_HALF_WIDTH,
        ),
        Vec3::new(
            position.x + BOAT_HALF_WIDTH,
            position.y + f64::from(BOAT_HEIGHT),
            position.z + BOAT_HALF_WIDTH,
        ),
    )
}

/// Entity#getBoundingBox: the box the boat keeps, else the one its position makes.
fn boat_bounds(state: &BoatSimulationState) -> Aabb {
    state
        .bounding_box
        .unwrap_or_else(|| boat_box_at(state.position))
}

/// Boat.checkFallDamage (Boat.fall before 1.21.2) after the move: on the ground the fall ends
/// (on 1.8 only past 3, where the boat breaks), and in the air a drop counts unless the block
/// under the boat holds water (1.8.9 BoatEntity.java:400-421; 1.9.4 :632-662; 1.14.4 Boat.java:
/// 750-774; 1.18.2 :764-787 in float; 1.21.5 AbstractBoat.java:685-692 in double; 26.3 :720-730).
fn count_boat_fall(body: &mut SimulationState, world: &WorldSnapshot, dy: f64, release: MinecraftVersion) {
    if body.on_ground {
        if release.at_least(1, 9, 0) || body.fall_distance > 3.0 {
            body.fall_distance = 0.0;
        }
    } else if dy < 0.0 {
        let (x, y, z) = crate::simulation::block_position(body.position, release);
        if !is_water(world.fluid_at((x, y.wrapping_sub(1), z))) {
            body.fall_distance = crate::simulation::accumulated_fall_distance(body.fall_distance, dy, release);
        }
    }
}

fn boat_body(state: &BoatSimulationState, release: MinecraftVersion) -> SimulationState {
    SimulationState {
        position: state.position,
        velocity: state.velocity,
        half_width: BOAT_HALF_WIDTH,
        height: f64::from(BOAT_HEIGHT),
        on_ground: state.on_ground,
        collided_horizontally: state.collided_horizontally,
        collided_vertically: state.collided_vertically,
        controlled_by_local_instance: true,
        // (1.16.5 Entity.java:631-632; 1.17 Entity.java:559)
        bounding_box: (!release.at_least(1, 17, 0)).then(|| boat_bounds(state)),
        stuck_speed_multiplier: state.stuck_speed_multiplier,
        fall_distance: state.fall_distance,
        sweet_berry_bush_immune: true,
        boat: true,
        ..SimulationState::default()
    }
}

/// Entity.move's checkInsideBlocks (before 1.21.2): the stepOn of the block under the entity and
/// the blocks inside its box. Before 1.15 Entity.move runs onSteppedOn only for an entity that
/// canClimb (1.8.8 Entity.java:667, 676; 1.12.2 :830, 839; 1.14.4 :533, 542), which a boat does not
/// (1.8.8 Boat.java:98; 1.12.2 :117; 1.13.2 :130); from 1.15 it is the onGround entity's
/// that does not bypass stepping effects (1.15 Entity.java:590).
pub(crate) fn apply_effects_inside(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) {
    let inside: Vec<_> = blocks_inside(body, release)
        .into_iter()
        .map(|position| (position, true))
        .collect();
    if release.at_least(1, 15, 0) {
        apply_block_effects(body, world, support, release, &inside);
    } else {
        apply_inside_block_effects(body, world, release, &inside);
    }
}

/// The blocks Boat.tick applies after its move, again (1.9 - 1.21.1, after the move's own:
/// checkBlockCollision, 1.9.4 Boat.java:260, 275; 1.16 :295, 323; 1.20.5 :279, 305) or
/// twice from 1.21.2, which applies none in the move (applyEffectsFromBlocks, the tick's
/// recorded moves then the whole of it: 1.21.11 AbstractBoat.java:236-237, Entity.java:859-870;
/// 26.3 AbstractBoat.java:236-237). A sweet berry bush does not hold a boat (no LivingEntity).
/// (1.21.1 Boat.java:308 checkInsideBlocks once; 1.21.2 AbstractBoat.java:268-269 applyEffectsFromBlocks
/// twice.)
fn apply_boat_block_effects(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    old_position: Vec3,
    move_start: Vec3,
    outcome: VehicleMove,
) {
    if !release.at_least(1, 21, 2) {
        let inside: Vec<_> = blocks_inside(body, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_inside_block_effects(body, world, release, &inside);
        return;
    }
    let whole_tick = Movement {
        from: old_position,
        to: body.position,
        axis_order: None,
    };
    // the move records from where floatBoat's snap and the interpolation left the boat, the
    // second pass's fallback from the tick's start (1.21.5 AbstractBoat.java:216-239, 520-527,
    // Entity.java:707-721, 809-822; 26.3 AbstractBoat.java:236-237)
    let recorded = movements_this_tick_from(
        body,
        old_position,
        move_start,
        outcome.requested,
        outcome.moved,
        release,
    );
    for movements in [recorded, vec![whole_tick]] {
        let (inside, _) = blocks_inside_moves(body, world, &movements, release);
        apply_block_effects(body, world, outcome.support, release, &inside);
    }
}

pub(crate) fn is_water(fluid: FluidState) -> bool {
    matches!(fluid, FluidState::Water | FluidState::Waterlogged)
}

/// FluidState.getHeight of the water at a position; in 1.13 its own amount over 9, with no look at
/// the block above (method_17810: 1.13.2 Boat.java:451, 524, 554, FishingHook.java:161,
/// FlowingFluid.java:433); before 1.13 BlockLiquid's getBlockLiquidHeight: 1 under water only for
/// a level whose low bits are 0.
pub(crate) fn water_height(world: &WorldSnapshot, position: BlockPosition, release: MinecraftVersion) -> f32 {
    if release.at_least(1, 14, 0) {
        return world.fluid_height_at(position) as f32;
    }
    let block = world.block_cow(position);
    if release.at_least(1, 13, 0) {
        let amount = if block.fluid_source || block.fluid_falling || !(1..8).contains(&block.fluid_level) {
            8
        } else {
            8 - block.fluid_level
        };
        return f32::from(amount) / 9.0;
    }
    let level = block.fluid_level;
    let (x, y, z) = position;
    if level & 7 == 0 && is_water(world.fluid_at((x, y + 1, z))) {
        return 1.0;
    }
    1.0 - f32::from(if level >= 8 { 0 } else { level } + 1) / 9.0
}

/// Boat.getStatus: under water, in water, on land (its ground friction) or in the air.
fn boat_status(
    state: &mut BoatSimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> BoatStatus {
    let bounds = boat_bounds(state);
    if let Some(status) = underwater_status(bounds, world, release) {
        state.water_level = bounds.max.y;
        return status;
    }
    let (in_water, water_level) = water_level_in(bounds, world, release);
    state.water_level = water_level;
    if in_water {
        return BoatStatus::InWater;
    }
    let friction = ground_friction(bounds, world, release);
    if friction > 0.0 {
        state.land_friction = friction;
        BoatStatus::OnLand
    } else {
        BoatStatus::InAir
    }
}

/// Boat.isUnderwater: water whose surface is above the box's top, flowing water first.
fn underwater_status(
    bounds: Aabb,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> Option<BoatStatus> {
    let top = bounds.max.y + 0.001;
    let mut under_water = false;
    for x in floor_i32_for(release, bounds.min.x)..ceil_i32_for(release, bounds.max.x) {
        for y in floor_i32_for(release, bounds.max.y)..ceil_i32_for(release, top) {
            for z in floor_i32_for(release, bounds.min.z)..ceil_i32_for(release, bounds.max.z) {
                let block = world.block_cow((x, y, z));
                if !is_water(block.fluid) {
                    continue;
                }
                let surface = y as f32 + water_height(world, (x, y, z), release);
                if top < f64::from(surface) {
                    if !block.fluid_source {
                        return Some(BoatStatus::UnderFlowingWater);
                    }
                    under_water = true;
                }
            }
        }
    }
    under_water.then_some(BoatStatus::UnderWater)
}

/// Boat.checkInWater: whether water reaches above the box's bottom, and the highest surface
/// in its bottom layer (the smallest double before 1.13, the lowest after, when none).
fn water_level_in(bounds: Aabb, world: &WorldSnapshot, release: MinecraftVersion) -> (bool, f64) {
    // Double.MIN_VALUE, the smallest positive double, until 1.17 (1.15 and 1.16.5 Boat.java
    // checkBoatInWater; -Double.MAX_VALUE in 1.17 :526)
    let mut water_level = if release.at_least(1, 17, 0) {
        -f64::MAX
    } else {
        f64::from_bits(1)
    };
    let mut in_water = false;
    for x in floor_i32_for(release, bounds.min.x)..ceil_i32_for(release, bounds.max.x) {
        for y in floor_i32_for(release, bounds.min.y)..ceil_i32_for(release, bounds.min.y + 0.001) {
            for z in floor_i32_for(release, bounds.min.z)..ceil_i32_for(release, bounds.max.z) {
                if !is_water(world.fluid_at((x, y, z))) {
                    continue;
                }
                let surface = f64::from(y as f32 + water_height(world, (x, y, z), release));
                water_level = water_level.max(surface);
                in_water |= bounds.min.y < surface;
            }
        }
    }
    (in_water, water_level)
}

/// Boat.getWaterLevelAbove: from the box's top, the first layer whose water does not fill a
/// block, as far as the boat fell in the tick before.
fn water_level_above(
    bounds: Aabb,
    last_yd: f64,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> f32 {
    let top = ceil_i32_for(release, bounds.max.y - last_yd);
    // 1.13 takes max(f, y + height), so any water of a layer at y >= 1 fills it, and returns y + f
    // (1.13.2 BoatEntity.java:452, 458; the height alone in 1.12.2 :388 and 1.14 :448)
    let adds_layer = release.at_least(1, 13, 0) && !release.at_least(1, 14, 0);
    'layers: for y in floor_i32_for(release, bounds.max.y)..top {
        let mut height = 0.0_f32;
        for x in floor_i32_for(release, bounds.min.x)..ceil_i32_for(release, bounds.max.x) {
            for z in floor_i32_for(release, bounds.min.z)..ceil_i32_for(release, bounds.max.z) {
                if is_water(world.fluid_at((x, y, z))) {
                    let own = water_height(world, (x, y, z), release);
                    height = height.max(if adds_layer { y as f32 + own } else { own });
                }
                if height >= 1.0 {
                    continue 'layers;
                }
            }
        }
        return y as f32 + height;
    }
    top.wrapping_add(1) as f32
}

/// Boat.getGroundFriction: the float average friction of the blocks whose collision shape meets
/// the slice under the box (NaN, so no land, when none does). A lily pad is left out of both: by
/// `instanceof LilyPadBlock` from 1.13 (1.13.2 Boat.java:491, 1.20.4 :513; 1.21.5
/// AbstractBoat.java:455, 1.21.11 :453), by the box it gives no boat before (1.9.4 - 1.12.2
/// WaterlilyBlock.java:23-25, which the friction reads with the boat: 1.12.2 Boat.java:428).
fn ground_friction(bounds: Aabb, world: &WorldSnapshot, release: MinecraftVersion) -> f32 {
    let slice = Aabb::new(
        Vec3::new(bounds.min.x, bounds.min.y - 0.001, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
    );
    let cells = |min: f64, max: f64| {
        (
            floor_i32_for(release, min).wrapping_sub(1),
            ceil_i32_for(release, max).wrapping_add(1),
        )
    };
    let (min_x, max_x) = cells(slice.min.x, slice.max.x);
    let (min_y, max_y) = cells(slice.min.y, slice.max.y);
    let (min_z, max_z) = cells(slice.min.z, slice.max.z);
    let mut friction = 0.0_f32;
    let mut blocks = 0;
    for x in min_x..max_x {
        for z in min_z..max_z {
            let edges =
                i32::from(x == min_x || x == max_x - 1) + i32::from(z == min_z || z == max_z - 1);
            if edges == 2 {
                continue;
            }
            for y in min_y..max_y {
                if edges > 0 && (y == min_y || y == max_y - 1) {
                    continue;
                }
                let meets = world.effect_at((x, y, z)) != BlockEffect::LilyPad
                    && world
                        .block_collision_boxes((x, y, z))
                        .iter()
                        .any(|shape| shape.intersects(slice));
                if meets {
                    friction += world.slipperiness_at((x, y, z)) as f32;
                    blocks += 1;
                }
            }
        }
    }
    friction / blocks as f32
}

/// Entity.baseTick's fluid push: the water's current, and from 1.16 the lava's (1.15.2
/// Entity.java:1028-1043 the water only; 1.16 Entity.java:1036-1044 the lava after it, though
/// not for what is in water until 1.16.2, 1.16.2 Entity.java:1064-1070).
fn push_boat_by_fluids(
    state: &mut BoatSimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let bounds = boat_bounds(state);
    let mut body = SimulationState {
        velocity: state.velocity,
        bounding_box: Some(bounds),
        ..SimulationState::default()
    };
    if legacy_fluid::applies(release) {
        // Entity.handleWaterMovement before 1.13: the box shrunk by 0.4F vertically and 0.001 all
        // round, its water's flows summed and normalized (1.8.8 Entity.java:837, Level.java:1533;
        // 1.12.2 Entity.java:1040-1055, Level.java:1616-1655)
        legacy_fluid::handle_water_movement(&mut body, world, true, release);
        state.in_water = body.in_water;
    } else {
        // (1.13: the fluid's own height, 1.13.2 Entity.java:2511-2551)
        let water = water_contact(world, bounds, release);
        state.in_water = water.touching;
        if water.touching {
            push_by_fluid(&mut body, water, 0.014, release);
        }
    }
    // the base tick's water ends the fall before the move counts any (1.8.9 Entity.java
    // handleWaterMovement; 1.13.2 Entity.java:1053-1062; 26.3 updateInWaterStateAndDoWaterCurrentPushing)
    if state.in_water {
        state.fall_distance = 0.0;
    }
    // (1.16 Entity.java:1041-1044 returns before the lava for a boat in water; 1.16.2 :1064-1070 does not)
    if release.at_least(1, 16, 2) || (release.at_least(1, 16, 0) && !state.in_water) {
        let lava = world.fluid_contact(FluidState::Lava, bounds, release);
        push_by_fluid(&mut body, lava, lava_flow_scale(state.ultra_warm), release);
    }
    state.velocity = body.velocity;
}

/// Entity's lava current scale (1.16+): 0.007 in an ultrawarm dimension (FAST_LAVA from
/// 1.21.11), 0.0023333333333333335 elsewhere, for every entity it pushes (1.16 Entity.java:1046,
/// 1.19.2 :1063, 1.21.4 :1364, 1.21.11 :1561, 26.3 :1704).
pub(crate) fn lava_flow_scale(ultra_warm: bool) -> f64 {
    if ultra_warm {
        0.007
    } else {
        0.0023333333333333335
    }
}

/// Boat.floatBoat: a boat that came out of the air onto water is set on its surface; otherwise
/// gravity, the status's friction and buoyancy toward the water level. Gravity is the float
/// 0.04F before 1.20.5; the boat is set on the water only where it fits from 1.21.
fn float_boat(state: &mut BoatSimulationState, world: &WorldSnapshot, release: MinecraftVersion) {
    let entering = state.previous_status == BoatStatus::InAir
        && !matches!(state.status, BoatStatus::InAir | BoatStatus::OnLand);
    if entering {
        state.water_level = state.position.y + f64::from(BOAT_HEIGHT);
        let bounds = boat_bounds(state);
        let surface = water_level_above(bounds, state.last_yd, world, release);
        let target_y = f64::from(surface - BOAT_HEIGHT) + 0.101;
        let target = bounds.translated(Vec3::new(0.0, target_y - state.position.y, 0.0));
        // Level.noCollision(boat, box) (client1.21.11 AbstractBoat.java:521): the boat's
        // CollisionContext (scaffolding is empty to a boat not above it) and the border
        let context = EntityCollisionContext {
            bottom: state.position.y,
            ..EntityCollisionContext::default()
        };
        let entity = CollisionQueryEntity {
            context: &context,
            bounds,
            position: state.position,
            outside_border: false,
        };
        let fits = world.no_collision_for_entity(target, entity, release);
        // (1.20.6 Boat.java:570; 1.21 Boat.java:594)
        if fits || !release.at_least(1, 21, 0) {
            state.position.y = target_y;
            if state.bounding_box.is_some() {
                state.bounding_box = Some(boat_box_at(state.position));
            }
            // (1.13.2 Boat.java:574-577; 1.14.4 Boat.java:614-618)
            state.velocity.y = if release.at_least(1, 14, 0) {
                state.velocity.y * 0.0
            } else {
                0.0
            };
            state.last_yd = 0.0;
        }
        state.status = BoatStatus::InWater;
        return;
    }
    float_boat_by_status(state, release);
}

/// Boat.floatBoat for a boat that does not come onto water: gravity, its status's friction and
/// the buoyancy toward the water level. NoGravity takes the gravity away (`hasNoGravity() ? 0.0 :
/// -0.04F`, 1.10.2 Boat.java:498, 1.20.4 :592; from 1.20.5 getFinalGravity(), 1.20.5
/// Boat.java:565, Entity.java:1143-1145), not the 0.04 / 0.65 of the buoyancy, which reads
/// getGravity() (1.20.5 Boat.java:598).
fn float_boat_by_status(state: &mut BoatSimulationState, release: MinecraftVersion) {
    let double_gravity = release.at_least(1, 20, 5);
    let mut vertical = if state.no_gravity {
        0.0
    } else if double_gravity {
        -0.04
    } else {
        f64::from(-0.04_f32)
    };
    let mut buoyancy = 0.0;
    let friction = match state.status {
        BoatStatus::InWater => {
            buoyancy = (state.water_level - state.position.y) / f64::from(BOAT_HEIGHT);
            0.9
        }
        BoatStatus::UnderFlowingWater => {
            vertical = -7.0e-4;
            0.9
        }
        BoatStatus::UnderWater => {
            buoyancy = 0.009999999776482582;
            0.45
        }
        BoatStatus::InAir => 0.9,
        BoatStatus::OnLand => {
            let friction = state.land_friction;
            if state.player_passenger {
                state.land_friction /= 2.0;
            }
            friction
        }
    };
    state.velocity.x *= f64::from(friction);
    state.velocity.y += vertical;
    state.velocity.z *= f64::from(friction);
    state.delta_rotation = f64::from(state.delta_rotation as f32 * friction);
    if buoyancy > 0.0 {
        let lift = if double_gravity {
            0.04 / 0.65
        } else {
            0.06153846016296973
        };
        state.velocity.y = (state.velocity.y + buoyancy * lift) * 0.75;
    }
}

/// Boat.controlBoat: the turn keys change deltaRotation by a degree, the yaw turns by it and the
/// keys push the boat along it (float arithmetic).
fn control_boat(state: &mut BoatSimulationState, release: MinecraftVersion) {
    let mut delta_rotation = state.delta_rotation as f32;
    let mut acceleration = 0.0_f32;
    if state.left {
        delta_rotation -= 1.0;
    }
    if state.right {
        delta_rotation += 1.0;
    }
    if state.right != state.left && !state.forward && !state.input_down {
        acceleration += 0.005;
    }
    state.yaw_degrees += delta_rotation;
    state.delta_rotation = f64::from(delta_rotation);
    if state.forward {
        acceleration += 0.04;
    }
    if state.input_down {
        acceleration -= 0.005;
    }
    let radians = state.yaw_degrees * 0.017453292;
    let sin = mth_sin(f64::from(-radians), release) as f32;
    let cos = mth_cos(f64::from(radians), release) as f32;
    state.velocity.x += f64::from(sin * acceleration);
    // Vec3.add(sin * acceleration, 0.0, cos * acceleration) from 1.14: its 0.0 turns the -0.0 that
    // floatBoat's multiply(1.0, 0.0, 1.0) leaves on a boat falling onto water into 0.0, which
    // checkFallDamage then keeps as lastYd (1.14.4 Boat.java:617, 673-674, 1.20.4 :598, 656,
    // 26.3 AbstractBoat.java:547, 610, 721). 1.13.2 sets velocityY to 0.0 and its paddles add to
    // the other two alone (1.13.2 Boat.java:577, 612-638).
    if release.at_least(1, 14, 0) {
        state.velocity.y += 0.0;
    }
    state.velocity.z += f64::from(cos * acceleration);
}

/// 1.8's boat box: EntityBoat.setSize(1.5F, 0.6F).
const LEGACY_BOAT_HALF_WIDTH: f64 = 0.75;
const LEGACY_BOAT_HEIGHT: f32 = 0.6;

/// 1.8's EntityBoat.onUpdate, which the client runs itself for the boat its player rides: the
/// share of five slices of its box in water floats it, its rider's input pushes it along the
/// rider's yaw by its speed multiplier, its speed is held to 0.35 while the multiplier grows
/// toward 0.35 or shrinks toward 0.07, on the ground its motion halves, it moves, keeps 0.99F of
/// its horizontal motion and 0.95F of its vertical (unless it ran into a wall fast) and turns
/// toward where it went, by at most 20 degrees (1.8.8 Boat.java:260-281).
fn tick_legacy_boat(
    state: &mut BoatSimulationState,
    world: &mut WorldSnapshot,
    release: MinecraftVersion,
) {
    let bounds = state.bounding_box.unwrap_or_else(|| {
        Aabb::new(
            Vec3::new(
                state.position.x - LEGACY_BOAT_HALF_WIDTH,
                state.position.y,
                state.position.z - LEGACY_BOAT_HALF_WIDTH,
            ),
            Vec3::new(
                state.position.x + LEGACY_BOAT_HALF_WIDTH,
                state.position.y + f64::from(LEGACY_BOAT_HEIGHT),
                state.position.z + LEGACY_BOAT_HALF_WIDTH,
            ),
        )
    });
    state.bounding_box = Some(bounds);
    push_boat_by_fluids(state, world, release);
    let start = state.position;
    let mut in_water = 0.0;
    for slice in 0..5 {
        let at = |share: f64| bounds.min.y + (bounds.max.y - bounds.min.y) * share / 5.0 - 0.125;
        let part = Aabb::new(
            Vec3::new(bounds.min.x, at(f64::from(slice)), bounds.min.z),
            Vec3::new(bounds.max.x, at(f64::from(slice + 1)), bounds.max.z),
        );
        if legacy_water_reaches(world, part) {
            in_water += 1.0 / 5.0;
        }
    }
    let velocity = &mut state.velocity;
    let speed_before = velocity.horizontal_length();
    if in_water < 1.0 {
        velocity.y += f64::from(0.04_f32) * (in_water * 2.0 - 1.0);
    } else {
        if velocity.y < 0.0 {
            velocity.y /= 2.0;
        }
        velocity.y += f64::from(0.007_f32);
    }
    let rider = state.legacy;
    let rider_yaw = rider.rider_yaw + -rider.rider_strafe * 90.0;
    let radians = f64::from(rider_yaw * std::f32::consts::PI / 180.0);
    let (multiplier, forward) = (rider.speed_multiplier, f64::from(rider.rider_forward));
    velocity.x += -math_sin(radians, release) * multiplier * forward * f64::from(0.05_f32);
    velocity.z += math_cos(radians, release) * multiplier * forward * f64::from(0.05_f32);
    let mut speed = velocity.horizontal_length();
    if speed > 0.35 {
        let scale = 0.35 / speed;
        velocity.x *= scale;
        velocity.z *= scale;
        speed = 0.35;
    }
    let multiplier = &mut state.legacy.speed_multiplier;
    if speed > speed_before && *multiplier < 0.35 {
        *multiplier = (*multiplier + (0.35 - *multiplier) / 35.0).min(0.35);
    } else {
        *multiplier = (*multiplier - (*multiplier - 0.07) / 35.0).max(0.07);
    }
    if state.on_ground {
        state.velocity = state.velocity.scale(0.5);
    }
    let mut body = SimulationState {
        position: state.position,
        velocity: state.velocity,
        half_width: LEGACY_BOAT_HALF_WIDTH,
        height: f64::from(LEGACY_BOAT_HEIGHT),
        on_ground: state.on_ground,
        bounding_box: state.bounding_box,
        controlled_by_local_instance: true,
        stuck_speed_multiplier: state.stuck_speed_multiplier,
        fall_distance: state.fall_distance,
        sweet_berry_bush_immune: true,
        boat: true,
        ..SimulationState::default()
    };
    let velocity = state.velocity;
    state.move_out_of_range |= !lily_pad_reach(bounds, velocity).is_walkable();
    let mut cleared = WorldPatches::default();
    legacy_boat_world(world, &mut cleared, release, bounds, velocity, Some(state.position));
    // Entity.moveEntity's doBlockCollisions, the only one of 1.8's boat (1.8.8
    // Boat.java:314; a cobweb's setInWeb slows its next move)
    // updateFallState before doBlockCollisions, whose cobweb (setInWeb) ends the fall (1.8.8
    // Entity.java moveEntity)
    let mut ended_inside = false;
    let moved = move_vehicle(
        &mut body,
        world,
        release,
        velocity,
        MoveRules::BOAT,
        |body, support| {
            let fall = body.fall_distance;
            body.fall_distance = 1.0;
            apply_effects_inside(body, world, support, release);
            ended_inside = body.fall_distance == 0.0;
            body.fall_distance = fall;
        },
    );
    count_boat_fall(&mut body, world, moved.moved.y, release);
    world.restore(&mut cleared);
    if ended_inside {
        body.fall_distance = 0.0;
    }
    state.stuck_speed_multiplier = body.stuck_speed_multiplier;
    state.fall_distance = body.fall_distance;
    state.move_out_of_range |= body.move_out_of_range;
    state.position = body.position;
    state.velocity = body.velocity;
    state.bounding_box = body.bounding_box;
    state.on_ground = body.on_ground;
    state.collided_horizontally = body.collided_horizontally;
    state.collided_vertically = body.collided_vertically;
    if !state.collided_horizontally || speed_before <= 0.2975 {
        state.velocity.x *= f64::from(0.99_f32);
        state.velocity.y *= f64::from(0.95_f32);
        state.velocity.z *= f64::from(0.99_f32);
    }
    let held = f64::from(state.yaw_degrees);
    let (dx, dz) = (start.x - state.position.x, start.z - state.position.z);
    let heading = if dx * dx + dz * dz > 0.001 {
        f64::from((atan2_vanilla(dz, dx, release) * 180.0 / std::f64::consts::PI) as f32)
    } else {
        held
    };
    let turn = wrap_degrees(heading - held).clamp(-20.0, 20.0);
    state.yaw_degrees = (held + turn) as f32 % 360.0;
    state.yaw = f64::from(state.yaw_degrees).to_radians();
}

/// The cells a boat's move looks for lily pads in.
fn lily_pad_reach(bounds: Aabb, movement: Vec3) -> Aabb {
    bounds.expand_towards(movement).inflate(Vec3::new(1.0, 1.0, 1.0))
}

/// The world a boat moves in before 1.13, where a lily pad gives it no collision box (1.8.8
/// WaterlilyBlock.java:38-42; 1.9.4 - 1.12.2 :23-25; 1.13 drops the override): the lily pads the
/// move can reach removed. 1.8's boat (`corners_of`, where it is) also clears the snow layers and
/// lily pads at its four corners, in the block of its feet and the one above, before it moves
/// (1.8.8 Boat.java:290-306). Written into `world` through `patches`, which the caller restores.
fn legacy_boat_world(
    world: &mut WorldSnapshot,
    patches: &mut WorldPatches,
    release: MinecraftVersion,
    bounds: Aabb,
    movement: Vec3,
    corners_of: Option<Vec3>,
) {
    if release.at_least(1, 13, 0) {
        return;
    }
    let mut cleared = Vec::new();
    if let Some(position) = corners_of {
        for corner in 0..4 {
            let x = floor_i32_for(release, position.x + (f64::from(corner % 2) - 0.5) * 0.8);
            let z = floor_i32_for(release, position.z + (f64::from(corner / 2) - 0.5) * 0.8);
            for layer in 0..2 {
                let at = (x, floor_i32_for(release, position.y) + layer, z);
                if matches!(
                    world.effect_at(at),
                    BlockEffect::SnowLayer | BlockEffect::LilyPad
                ) {
                    cleared.push(at);
                }
            }
        }
    }
    let reach = lily_pad_reach(bounds, movement);
    // (a move past what a call walks is refused, by the caller)
    if !reach.is_walkable() {
        return;
    }
    for x in floor_i32_for(release, reach.min.x)..=floor_i32_for(release, reach.max.x) {
        for y in floor_i32_for(release, reach.min.y)..=floor_i32_for(release, reach.max.y) {
            for z in floor_i32_for(release, reach.min.z)..=floor_i32_for(release, reach.max.z) {
                if world.effect_at((x, y, z)) == BlockEffect::LilyPad {
                    cleared.push((x, y, z));
                }
            }
        }
    }
    for at in cleared {
        world.patch_block(patches, at, BlockState::default());
    }
}

/// 1.8's Level.containsBlockWithMaterial for water: a water block in the cells from the box's corner to
/// one past it whose surface (its top, less level / 8 under level 8) reaches the box's bottom.
fn legacy_water_reaches(world: &WorldSnapshot, bounds: Aabb) -> bool {
    let cells = |min: f64, max: f64| floor_i32(min)..floor_i32(max + 1.0);
    cells(bounds.min.x, bounds.max.x).any(|x| {
        cells(bounds.min.y, bounds.max.y).any(|y| {
            cells(bounds.min.z, bounds.max.z).any(|z| {
                let block = world.block_cow((x, y, z));
                if block.fluid != FluidState::Water {
                    return false;
                }
                let level = block.fluid_level;
                let surface = if level < 8 {
                    f64::from(y + 1) - f64::from(level) / 8.0
                } else {
                    f64::from(y + 1)
                };
                surface >= bounds.min.y
            })
        })
    })
}

/// Mth.wrapDegrees of a double: within [-180, 180) (1.8.8 Mth.java:249-260).
fn wrap_degrees(degrees: f64) -> f64 {
    let mut wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

/// What a vehicle's Entity.move depends on besides its body: a living entity bounces off slime
/// and beds fully, its step height, and from 26.2 the gravity and air drag its bounce compensates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MoveRules {
    pub living: bool,
    pub max_step: f32,
    pub gravity: f64,
    pub air_drag: f64,
    pub bounciness: f64,
    pub stands_on_lava: bool,
    /// The entity's checkFallDamage runs checkInsideBlocks itself (a strider's, 1.16 - 1.21.1).
    pub checks_inside_at_fall: bool,
    /// AbstractMinecart.getBlockSpeedFactor: 1 on a rail, whatever lies under it (1.16+).
    pub rails_keep_speed: bool,
    /// The cells whose blocks do not collide (MinecartCollisionContext).
    pub ignored: [Option<BlockPosition>; 2],
    /// The caller knows this entity does not simulate its movement, from 26.2: no restitution.
    pub skips_restitution: bool,
    /// Entity.checkFallDamage as Entity itself has it (`count_fall`), at its place in the move: after
    /// the flags, before the motion stops and the blocks inside (1.20.4 Entity.java:649-664, 26.3
    /// :811-815). The callers with their own fall leave it off.
    pub counts_fall: bool,
}

impl MoveRules {
    /// The rules with the entity's getEffectiveGravity, which is 0 under NoGravity (26.2
    /// Entity.java:887-889, 1532-1533; LivingEntity.getEffectiveGravity returns getGravity too),
    /// for the landing's bounce threshold and gravity compensation (26.2 Entity.java:848, 855).
    pub(crate) fn with_gravity_of(rules: Self, no_gravity: bool) -> Self {
        Self {
            gravity: if no_gravity { 0.0 } else { rules.gravity },
            ..rules
        }
    }

    const BOAT: Self = Self {
        living: false,
        max_step: 0.0,
        gravity: 0.04,
        air_drag: 1.0,
        bounciness: 0.0,
        stands_on_lava: false,
        checks_inside_at_fall: false,
        rails_keep_speed: false,
        ignored: [None, None],
        skips_restitution: false,
        counts_fall: false,
    };

    pub(crate) const MINECART: Self = Self::BOAT;
}

/// What Entity.move left besides the body: the movement asked for and made, the block that
/// supports the entity, and the motion checkFallDamage saw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VehicleMove {
    pub requested: Vec3,
    pub moved: Vec3,
    pub support: Option<BlockPosition>,
    pub motion_at_landing: Vec3,
    /// The water and lava heights a mob's refresh within the move left, from 26.1.
    pub fluid_heights: Option<(f64, f64)>,
}

/// Entity.move for an entity that is not the player: the stuck speed, the collision (stepping
/// up by `max_step`), the collision flags, the motion stopped on the axes it collided on and the
/// landing, the blocks inside (`inside`, before 1.21.2) and the block's speed factor from 1.15.
/// Before 1.17 the body keeps its box (`bounding_box`) and is placed from it.
pub(crate) fn move_vehicle(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    movement: Vec3,
    rules: MoveRules,
    inside: impl FnOnce(&mut SimulationState, Option<BlockPosition>),
) -> VehicleMove {
    move_vehicle_among(body, world, release, movement, rules, &[], inside)
}

/// `move_vehicle` of an entity the boxes of `entities` stop (a minecart's canCollideWith).
pub(crate) fn move_vehicle_among(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    movement: Vec3,
    rules: MoveRules,
    entities: &[Aabb],
    inside: impl FnOnce(&mut SimulationState, Option<BlockPosition>),
) -> VehicleMove {
    crate::phase::emit_vehicle_move("vehicleBeforeMove", body);
    let moved = run_vehicle_move(body, world, release, movement, rules, entities, inside);
    crate::phase::emit_vehicle_move("vehicleAfterMove", body);
    moved
}

fn run_vehicle_move(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    movement: Vec3,
    mut rules: MoveRules,
    entities: &[Aabb],
    inside: impl FnOnce(&mut SimulationState, Option<BlockPosition>),
) -> VehicleMove {
    let mut requested = movement;
    if body.stuck_speed_multiplier.length_squared() > 1.0e-7 {
        requested = requested.multiply(body.stuck_speed_multiplier);
        body.stuck_speed_multiplier = Vec3::ZERO;
        body.velocity = Vec3::ZERO;
    }
    let bounds = body.bounds();
    // (every entity's move ends here, and what it walks is a cell at a time)
    let step = Vec3::new(0.0, f64::from(rules.max_step.max(0.0)), 0.0);
    if !bounds.expand_towards(requested).expand_towards(step).is_walkable() {
        body.move_out_of_range = true;
        requested = Vec3::ZERO;
        rules.max_step = 0.0;
    }
    // (1.13.2 Entity.java:674-773; 1.14.4 Entity.java:491-500, 691-719)
    let modern = release.at_least(1, 14, 0);
    let moved = if modern {
        let moved = collide_vehicle(body, world, release, bounds, requested, rules, entities);
        if moves_position(requested, moved, release) {
            reset_fall_on_clip(body, world, body.position, moved, release);
            place_body(body, moved);
        }
        moved
    } else {
        let (moved_box, moved) = legacy_collide(
            world,
            release,
            bounds,
            requested,
            rules.max_step,
            body.on_ground,
            entities,
        );
        body.bounding_box = Some(moved_box);
        body.position = position_of_bounding_box(moved_box);
        moved
    };
    let (x_collision, z_collision) = if modern {
        (
            !vanilla_equal(requested.x, moved.x),
            !vanilla_equal(requested.z, moved.z),
        )
    } else {
        (requested.x != moved.x, requested.z != moved.z)
    };
    let y_collision = requested.y != moved.y;
    body.collided_horizontally = x_collision || z_collision;
    // from 1.21.4 a level move of what the instance does not control keeps the vertical flags, the
    // ground and the supporting block (1.21.4 Entity.java:706-710, 26.3 :798-803)
    let support = if refreshes_vertical_collision(body, requested.y, release) {
        body.collided_vertically = y_collision;
        body.vertical_collision_below = y_collision && requested.y < 0.0;
        body.on_ground = body.vertical_collision_below;
        release
            // (1.19.4 Entity.java:605; 1.20 Entity.java:652)
            .at_least(1, 20, 0)
            .then(|| main_supporting_block(world, body, Some(moved), release))
            .flatten()
    } else {
        release
            .at_least(1, 20, 0)
            .then(|| kept_supporting_block(world, body, None, release))
            .flatten()
    };
    if rules.counts_fall {
        count_fall(body, moved.y, release);
    }
    if rules.checks_inside_at_fall {
        // Strider.checkFallDamage's checkInsideBlocks, before the motion stops on the axes it
        // collided on (1.16 Strider.java:295-296, Entity.java:608-623; 1.21.1
        // Strider.java:289)
        let inside: Vec<_> = blocks_inside(body, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_inside_block_effects(body, world, release, &inside);
    }
    let motion_at_landing = body.velocity;
    // a landing's block stops or bounces what the side simulates alone: the client leaves an item it does not own moving
    // until 26.3 (1.21.2 Entity.java:721-724, 1.21.3 :721-726, 1.21.5 :755-760, 26.2 :815; before, on both sides: 1.21 :692)
    let simulates = !release.at_least(1, 21, 2) || !body.client_side || can_simulate_movement(body, release);
    if release.at_least(26, 2, 0) {
        if simulates && !rules.skips_restitution {
            restitute_vehicle_motion(
            body,
            world,
            support,
            release,
            [x_collision, y_collision, z_collision],
            moved.y,
            rules,
            );
        }
    } else {
        // (1.18.1 Entity.java:618; 1.18.2 Entity.java:602)
        let (stops_x, stops_z) = if release.at_least(1, 18, 2) {
            (x_collision, z_collision)
        } else if modern {
            // until 1.18.2 the z reset puts back the x motion it read before the x reset
            let stops_z = requested.z != moved.z;
            (requested.x != moved.x && !stops_z, stops_z)
        } else {
            (requested.x != moved.x, requested.z != moved.z)
        };
        if stops_x {
            body.velocity.x = 0.0;
        }
        if stops_z {
            body.velocity.z = 0.0;
        }
        if y_collision && simulates {
            fall_on(body, world, support, release, rules.living);
        }
    }
    // (1.21.1 Entity.java:738; 1.21.2 Entity.java:776-778)
    if !release.at_least(1, 21, 2) {
        inside(body, support);
    }
    // (1.14.4 Entity.java:569; 1.15 Entity.java:634)
    if release.at_least(1, 15, 0) {
        let on_rail = rules.rails_keep_speed
            && crate::minecart::is_rail(world, block_position(body.position, release));
        let factor = if on_rail {
            1.0
        } else {
            f64::from(block_speed_factor(world, body, support, release))
        };
        body.velocity.x *= factor;
        body.velocity.z *= factor;
    }
    VehicleMove {
        requested,
        moved,
        support,
        motion_at_landing,
        fluid_heights: None,
    }
}

/// Where Entity#move leaves the body: a kept box moved and the position taken from it
/// (Entity#setLocationFromBoundingbox, before 1.17), else the position moved.
fn place_body(body: &mut SimulationState, moved: Vec3) {
    match body.bounding_box {
        Some(bounds) => {
            let moved = bounds.translated(moved);
            body.bounding_box = Some(moved);
            body.position = position_of_bounding_box(moved);
        }
        None => body.position = body.position + moved,
    }
}

/// Entity.collide from 1.14, with the world border's boxes where the entity is near it, and the
/// still lava an entity that stands on it is above (LiquidBlock.getCollisionShape).
fn collide_vehicle(
    body: &SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    bounds: Aabb,
    movement: Vec3,
    rules: MoveRules,
    entities: &[Aabb],
) -> Vec3 {
    let max_step = rules.max_step;
    let context = EntityCollisionContext {
        bottom: body.position.y,
        descending: false,
        fall_distance: body.fall_distance,
        walks_on_powder_snow: body.powder_snow_walkable,
        falling_block: false,
    };
    let swept = bounds.expand_towards(movement);
    let query = Aabb::new(
        Vec3::new(swept.min.x, swept.min.y - 1.0e-5, swept.min.z),
        Vec3::new(
            swept.max.x,
            swept.max.y.max(bounds.max.y + f64::from(max_step)),
            swept.max.z,
        ),
    );
    let mut blocks = world.collision_block_shapes_for_entity(query, &context, release);
    for (x, y, z) in rules.ignored.into_iter().flatten() {
        let origin = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
        blocks.retain(|shape| shape.origin != origin);
    }
    if rules.stands_on_lava {
        // LiquidBlock.getCollisionShape: the lava's STABLE_SHAPE is the block's own shape
        for surface in lava_surfaces(world, query, body.position.y, release) {
            let origin = Vec3::new(surface.min.x.floor(), surface.min.y.floor(), surface.min.z.floor());
            blocks.retain(|shape| shape.origin != origin);
            blocks.push(BlockVoxelShape::from_world(origin, vec![surface]));
        }
        sort_in_cursor_order(&mut blocks);
    }
    let border = world.world_border_collision_boxes();
    // (1.17.1 Entity.java:755; 1.18 Entity.java:801)
    let collides_with_border = if release.at_least(1, 18, 0) {
        world.is_inside_close_to_world_border(body.position, swept, release)
    } else {
        // (as collide_with_world, whose release is 1.14 or later here: run_vehicle_move takes legacy_collide before)
        let inside = bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !border.iter().any(|outside| shapes_join(*outside, inside))
    };
    let collision = CollisionRules {
        // (1.20.6 Entity.java:908; 1.21 Entity.java:926)
        step_up: if release.at_least(1, 21, 0) {
            StepUp::CandidateHeights
        } else {
            StepUp::Lifted
        },
        // (1.17.1 Entity.java:787; 1.18 Entity.java:794)
        single_axis_walk: !release.at_least(1, 18, 0),
    };
    // (collectColliders asks the level for the entities first, whatever the entity; a cart's own come after)
    let mut colliding = world.entity_collisions(
        entity_collision_query(swept, query, max_step, release),
        &context,
    );
    colliding.extend_from_slice(entities);
    let colliders = Colliders {
        entities: &colliding,
        border: collides_with_border.then_some(&border[..]),
        // (a vehicle's move decides the border once; Colliders::border_near would ask each attempt)
        border_near: None,
        boxes: &[],
        block_border: world
            .block_collisions_offer_world_border(bounds, release)
            .then_some(&border[..]),
        blocks: &blocks,
        release: Some(release),
    };
    collide_shapes(bounds, movement, colliders, body.on_ground, max_step, collision)
}

/// The top half of each still lava block in the cells of `query` under no lava, for an entity whose
/// feet are above it (LiquidBlock.STABLE_SHAPE, EntityCollisionContext.isAbove).
pub(crate) fn lava_surfaces(
    world: &WorldSnapshot,
    query: Aabb,
    bottom: f64,
    release: MinecraftVersion,
) -> Vec<Aabb> {
    let cells = |min: f64, max: f64| floor_i32_for(release, min)..ceil_i32_for(release, max);
    let mut surfaces = Vec::new();
    for x in cells(query.min.x, query.max.x) {
        for y in cells(query.min.y, query.max.y) {
            for z in cells(query.min.z, query.max.z) {
                if is_lava_surface(world, (x, y, z), bottom) {
                    let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    surfaces.push(Aabb::new(min, min + Vec3::new(1.0, 0.5, 1.0)));
                }
            }
        }
    }
    surfaces
}

/// LiquidBlock#getCollisionShape of lava for a mob that stands on it: a source with no lava
/// above, and the entity above its stable shape (EntityCollisionContext#isAbove).
fn is_lava_surface(world: &WorldSnapshot, (x, y, z): BlockPosition, bottom: f64) -> bool {
    let block = world.block_cow((x, y, z));
    block.fluid == FluidState::Lava
        && block.fluid_source
        && world.fluid_at((x, y + 1, z)) != FluidState::Lava
        && bottom > f64::from(y) + 0.5 - 9.999999747378752e-6
}

/// Entity.move's collision before 1.14: the box moves on y, then x, then z against the boxes the
/// move sweeps; blocked horizontally on the ground (or landing) it also tries the move raised by
/// the step height and keeps the one that goes farther. Returns the moved box and the movement
/// vanilla takes the collision flags from (its y the settling after a step). On 1.13 it is the
/// player's collide_legacy with VoxelShapes: each block's boxes one shape, met in the order the
/// cells are walked (x fastest), the full cubes joined in one shape after them, and the 1.0E-7
/// zero check once before each shape (1.13.2 LevelReader.java:113-155,
/// Shapes.java:151-160).
fn legacy_collide(
    world: &WorldSnapshot,
    release: MinecraftVersion,
    bounds: Aabb,
    requested: Vec3,
    max_step: f32,
    on_ground: bool,
    entities: &[Aabb],
) -> (Aabb, Vec3) {
    if release.at_least(1, 13, 0) {
        let obstacles = legacy_obstacles(world, bounds, requested, max_step);
        let mut colliding = legacy_entity_colliders(world, bounds, requested, max_step);
        colliding.extend_from_slice(entities);
        let legacy = collide_legacy(bounds, requested, &obstacles, &colliding, on_ground, max_step, true);
        return (legacy.bounds, legacy.movement);
    }
    let mut colliders = legacy_colliders(world, bounds, requested, release);
    colliders.extend(legacy_entity_colliders(world, bounds, requested, max_step));
    colliders.extend_from_slice(entities);
    let offset = |axis, bounds, colliders: &[Aabb], distance| {
        legacy_offset(axis, bounds, colliders, distance)
    };
    let mut moved_box = bounds;
    let Vec3 {
        mut x,
        mut y,
        mut z,
    } = requested;
    if y != 0.0 {
        y = offset(Axis::Y, moved_box, &colliders, y);
        moved_box = moved_box.translated(Vec3::new(0.0, y, 0.0));
    }
    if x != 0.0 {
        x = offset(Axis::X, moved_box, &colliders, x);
        moved_box = moved_box.translated(Vec3::new(x, 0.0, 0.0));
    }
    if z != 0.0 {
        z = offset(Axis::Z, moved_box, &colliders, z);
        moved_box = moved_box.translated(Vec3::new(0.0, 0.0, z));
    }
    let landing = on_ground || (requested.y != y && requested.y < 0.0);
    let blocked = requested.x != x || requested.z != z;
    if max_step <= 0.0 || !landing || !blocked {
        return (moved_box, Vec3::new(x, y, z));
    }
    let step = f64::from(max_step);
    let mut step_colliders = legacy_colliders(
        world,
        bounds,
        Vec3::new(requested.x, step, requested.z),
        release,
    );
    step_colliders.extend(legacy_entity_colliders(
        world,
        bounds,
        Vec3::new(requested.x, step, requested.z),
        max_step,
    ));
    let across = |rise: f64| {
        let raised = bounds.translated(Vec3::new(0.0, rise, 0.0));
        let dx = offset(Axis::X, raised, &step_colliders, requested.x);
        let moved_x = raised.translated(Vec3::new(dx, 0.0, 0.0));
        let dz = offset(Axis::Z, moved_x, &step_colliders, requested.z);
        (
            moved_x.translated(Vec3::new(0.0, 0.0, dz)),
            Vec3::new(dx, rise, dz),
        )
    };
    let horizontal = Vec3::new(requested.x, 0.0, requested.z);
    let swept = across(offset(
        Axis::Y,
        bounds.expand_towards(horizontal),
        &step_colliders,
        step,
    ));
    let plain = across(offset(Axis::Y, bounds, &step_colliders, step));
    let (stepped_box, stepped) =
        if swept.1.horizontal_length_squared() > plain.1.horizontal_length_squared() {
            swept
        } else {
            plain
        };
    let down = offset(Axis::Y, stepped_box, &step_colliders, -stepped.y);
    if x * x + z * z >= stepped.x * stepped.x + stepped.z * stepped.z {
        return (moved_box, Vec3::new(x, y, z));
    }
    (
        stepped_box.translated(Vec3::new(0.0, down, 0.0)),
        Vec3::new(stepped.x, down, stepped.z),
    )
}

/// The block boxes a move can meet before 1.13 (Level.doesBoxCollide): those meeting the swept
/// box.
fn legacy_colliders(
    world: &WorldSnapshot,
    bounds: Aabb,
    movement: Vec3,
    release: MinecraftVersion,
) -> Vec<Aabb> {
    let swept = bounds.expand_towards(movement);
    // (a list of boxes: one of no height stops a vehicle too, as a head at the start of its way on 1.8)
    let mut colliders = world.legacy_collision_shapes(swept, release);
    border_as_stone(world, bounds, swept, release, &mut colliders);
    colliders.retain(|shape| shape.intersects(swept));
    colliders
}

/// Level.doesBoxCollide before 1.14 for a vehicle inside the world border: each cell outside it
/// collides as stone in place of its block (1.8.8 Level.java:1053-1072, 1.12.2 Level.java:1115-1140
/// `if (!bl && !lv.isWithinBounds(lv3) && bl3) lv4 = lv2` with lv2 STONE; 1.13.2
/// LevelReader.java:120-136 Shapes.block()). Inside is Level#isInsideWorld of the vehicle's
/// position (1.8.8 Level.java:1100-1117, 1.13.2 LevelReader.java:183-201), a block past the
/// border's edges while isOutsideWorldBorder is unset, which a vehicle the engine moves never
/// sets: it is inside the border while it moves there. The player's own path is simulation.rs's
/// legacy branch of collide_with_world, which this mirrors.
fn border_as_stone(world: &WorldSnapshot, bounds: Aabb, query: Aabb, release: MinecraftVersion, colliders: &mut Vec<Aabb>) {
    if !world.is_inside_legacy_world_border(position_of_bounding_box(bounds), false) {
        return;
    }
    colliders.retain(|collider| !world.is_outside_world_border_cell(collider.min.x.floor(), collider.min.z.floor()));
    colliders.extend(world.legacy_world_border_cells(query, release));
}

/// The entity colliders a move can meet before 1.14 (Level#doesBoxCollide, MCP's getCollisionBoxes, after
/// the blocks).
fn legacy_entity_colliders(
    world: &WorldSnapshot,
    bounds: Aabb,
    movement: Vec3,
    max_step: f32,
) -> Vec<Aabb> {
    let swept = bounds.expand_towards(movement);
    let query = Aabb::new(
        Vec3::new(swept.min.x, swept.min.y - 1.0e-5, swept.min.z),
        Vec3::new(
            swept.max.x,
            swept.max.y.max(bounds.max.y + f64::from(max_step.max(0.0))),
            swept.max.z,
        ),
    );
    let context = EntityCollisionContext {
        bottom: bounds.min.y,
        ..EntityCollisionContext::default()
    };
    world.entity_collisions(query, &context)
}

/// The block boxes of every cell 1.13's getCollisionBoxes can walk for the move and its step (the
/// swept box grown by 1.0E-7, and a cell past it on every side: method_16378 walks
/// floor(min) - 1 to ceil(max) on each axis, 1.13.2 LevelReader.java:114-119, where a moving
/// block beside or over the box reaches into it), each block's boxes together, which
/// collide_legacy groups, filters to that range less the cells on two of its faces, and orders as
/// 1.13.2 LevelReader.java:113-155 does.
fn legacy_obstacles(world: &WorldSnapshot, bounds: Aabb, movement: Vec3, max_step: f32) -> Vec<Aabb> {
    let swept = bounds.expand_towards(movement);
    let query = Aabb::new(
        swept.min,
        Vec3::new(
            swept.max.x,
            swept.max.y.max(bounds.max.y + f64::from(max_step.max(0.0))),
            swept.max.z,
        ),
    )
    .inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
    let mut obstacles = Vec::new();
    let range = |min: f64, max: f64| floor_i32(min).wrapping_sub(1)..=floor_i32(max.ceil());
    // a cell outside the border is Shapes.block() to a vehicle inside it (border_as_stone; 1.13.2
    // LevelReader.java:133-134)
    let inside = world.is_inside_legacy_world_border(position_of_bounding_box(bounds), false);
    for z in range(query.min.z, query.max.z) {
        for y in range(query.min.y, query.max.y) {
            for x in range(query.min.x, query.max.x) {
                if inside
                    && world.is_outside_world_border_cell(f64::from(x), f64::from(z))
                    // (every column on 1.13's client: hasChunkAt, 1.13.2 LevelReader.java:136, 277-283,
                    // MultiPlayerLevel.java:110-112)
                {
                    let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    obstacles.push(Aabb::new(min, min + Vec3::new(1.0, 1.0, 1.0)));
                    continue;
                }
                obstacles.extend(world.block_collision_boxes((x, y, z)));
            }
        }
    }
    obstacles
}

/// How far a box moves along an axis before it meets the colliders: AxisAlignedBB's
/// calculateOffset before 1.13 (AABB#method_583, AABB#method_589 and AABB#method_594 for x, y, z).
fn legacy_offset(axis: Axis, bounds: Aabb, colliders: &[Aabb], mut distance: f64) -> f64 {
    let get = |value: Vec3, axis: Axis| match axis {
        Axis::X => value.x,
        Axis::Y => value.y,
        Axis::Z => value.z,
    };
    let others = match axis {
        Axis::X => [Axis::Y, Axis::Z],
        Axis::Y => [Axis::Z, Axis::X],
        Axis::Z => [Axis::X, Axis::Y],
    };
    for collider in colliders {
        let overlaps = others.iter().all(|&other| {
            get(bounds.max, other) > get(collider.min, other)
                && get(bounds.min, other) < get(collider.max, other)
        });
        if !overlaps {
            continue;
        }
        if distance > 0.0 && get(bounds.max, axis) <= get(collider.min, axis) {
            distance = distance.min(get(collider.min, axis) - get(bounds.max, axis));
        } else if distance < 0.0 && get(bounds.min, axis) >= get(collider.max, axis) {
            distance = distance.max(get(collider.max, axis) - get(bounds.min, axis));
        }
    }
    distance
}

/// Block.updateEntityAfterFallOn (onLanded before 1.14): the vertical motion stops, or bounces
/// off slime (and a bed from 1.12), fully for a living entity and from 1.9 at 0.8 of it otherwise
/// (1.9.4 SlimeBlock.java:34-44; 1.8.8 SlimeBlock.java:33-39 has no scale).
fn fall_on(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
    living: bool,
) {
    let bounce = if living || !release.at_least(1, 9, 0) {
        1.0
    } else {
        0.8
    };
    match world.effect_at(landing_position(world, body, support, release)) {
        BlockEffect::Slime if !body.sneaking => {
            if body.velocity.y < 0.0 {
                body.velocity.y = -body.velocity.y * bounce;
            }
        }
        // (1.11.2 Block.java:674; 1.12.2 BedBlock.java:152-158)
        BlockEffect::Bed if release.at_least(1, 12, 0) && !body.sneaking => {
            if body.velocity.y < 0.0 {
                body.velocity.y = -body.velocity.y * 0.6600000262260437 * bounce;
            }
        }
        _ => {
            // (1.13.2 Block.java:680-682; 1.14.4 Block.java:687-689)
            body.velocity.y = if release.at_least(1, 14, 0) {
                body.velocity.y * 0.0
            } else {
                0.0
            };
        }
    }
}

/// Entity.restituteMovementAfterCollisions (26.2+): a collided axis bounces back by the entity's
/// bounciness, a landing by the block's (0.8 of it for an entity that is not living).
fn restitute_vehicle_motion(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
    collisions: [bool; 3],
    moved_y: f64,
    rules: MoveRules,
) {
    let current = body.velocity;
    let mut restitution = rules.bounciness;
    if collisions[0] {
        body.velocity.x = -current.x * restitution;
    }
    if collisions[2] {
        body.velocity.z = -current.z * restitution;
    }
    if !collisions[1] {
        return;
    }
    if body.vertical_collision_below {
        let landed_on = world.block_cow(landing_position(world, body, support, release));
        // (26.3 Entity.java:878 rests at the gravity itself, 26.2 :848 only under it)
        let resting = if release.at_least(26, 3, 0) {
            -current.y <= rules.gravity
        } else {
            -current.y < rules.gravity
        };
        restitution = if resting || landed_on.suppresses_bounce {
            0.0
        } else if rules.living {
            restitution.max(landed_on.bounce_restitution)
        } else {
            restitution.max(f64::from(landed_on.bounce_restitution as f32 * 0.8))
        };
    }
    let (compensation, drag) = if restitution > 0.0 {
        let portion = moved_y / current.y;
        (
            portion * rules.gravity,
            1.0 + portion * (rules.air_drag - 1.0),
        )
    } else {
        (0.0, 1.0)
    };
    body.velocity.y = (compensation - current.y) * drag * restitution;
}

/// A horse its player rides, without a world (simulateHorseTick), at the default release: the
/// negligible motion LivingEntity.aiStep drops, AbstractHorse's ridden input (half the strafe, a
/// quarter of a backward input) at its MOVEMENT_SPEED, the riding jump `jump_requested` gives at
/// full charge on the ground (JUMP_STRENGTH up, 0.4F forward along its yaw), then
/// LivingEntity.travel: in water the 0.02F input and the 0.8F slow-down (and the 0.04F lift of
/// floatInWaterWhileRidden in water deeper than 0.4, its `water_height`), else on a ground of the
/// default friction (0.6F) or in the air at a tenth of its speed, with gravity 0.08 and the 0.98F
/// drag, and a move nothing stops. `on_ground` is the ground under it, which a jump leaves; `yaw`
/// is its yRot in radians (1.21.11 AbstractHorse.java:733-759, LivingEntity.java:2220-2230,
/// :2256, :2348-2351, :2471-2481).
pub fn tick_horse(state: &mut HorseSimulationState) {
    let release = VersionPhysicsRules::default().release;
    for axis in [
        &mut state.velocity.x,
        &mut state.velocity.y,
        &mut state.velocity.z,
    ] {
        if axis.abs() < 0.003 {
            *axis = 0.0;
        }
    }
    let yaw_degrees = state.yaw.to_degrees() as f32;
    let yaw = yaw_radians(yaw_degrees, release);
    let speed = if state.movement_speed > 0.0 {
        state.movement_speed
    } else {
        0.1
    } as f32;
    let strafe = state.strafe as f32 * 0.5;
    let mut forward = state.forward as f32;
    if forward <= 0.0 {
        forward *= 0.25;
    }
    let input = Vec3::new(f64::from(strafe), 0.0, f64::from(forward));
    if state.on_ground && state.jump_requested {
        let strength = if state.jump_strength > 0.0 {
            state.jump_strength
        } else {
            0.42
        };
        state.velocity.y = f64::from(strength as f32);
        if input.z > 0.0 {
            let sin = mth_sin(yaw, release) as f32;
            let cos = mth_cos(yaw, release) as f32;
            state.velocity.x += f64::from(-0.4 * sin);
            state.velocity.z += f64::from(0.4 * cos);
        }
    }
    let on_ground = state.on_ground;
    if state.jump_requested {
        state.on_ground = false;
    }
    if state.in_water {
        let going_down = state.velocity.y <= 0.0;
        state.velocity =
            state.velocity + relative_movement(input, f64::from(0.02_f32), yaw, release);
        state.position = state.position + state.velocity;
        let slow_down = f64::from(0.8_f32);
        state.velocity =
            state
                .velocity
                .multiply(Vec3::new(slow_down, 0.800000011920929, slow_down));
        // getFluidFallingAdjustedMovement takes no gravity of 0 (NoGravity, 1.21.11
        // LivingEntity.java:2521-2533)
        if !state.no_gravity {
            state.velocity.y = fluid_falling_adjusted_y(0.08, 16.0, going_down, state.velocity.y);
        }
        // floatInWaterWhileRidden: a ridden horse (EntityTypeTags.CAN_FLOAT_WHILE_RIDDEN) in
        // water deeper than its getFluidJumpThreshold (0.4) rises 0.04F (26.2
        // LivingEntity.java:2420-2424, 2490-2496; 1.21.11 :2336-2342, 2404-2410)
        if state.water_height > 0.4 {
            state.velocity.y += f64::from(0.04_f32);
        }
    } else {
        let (inertia, input_speed) = if on_ground {
            (
                f64::from(0.6_f32 * 0.91),
                friction_influenced_speed(f64::from(speed), 0.6, release),
            )
        } else {
            (f64::from(0.91_f32), f64::from(speed * 0.1))
        };
        state.velocity = state.velocity + relative_movement(input, input_speed, yaw, release);
        state.position = state.position + state.velocity;
        if !state.no_gravity {
            state.velocity.y -= 0.08;
        }
        state.velocity.x *= inertia;
        state.velocity.z *= inertia;
        state.velocity.y *= f64::from(0.98_f32);
    }
    state.jump_requested = false;
}

#[cfg(test)]
mod tests {
    use super::{
        boat_passenger_position, boat_passenger_yaw, tick_boat, tick_boat_in_world,
        float_boat_by_status, legacy_collide, tick_boat_with_passenger, tick_horse, BoatSeat,
        BoatSimulationState,
        BoatStatus, BOAT_HEIGHT,
        HorseSimulationState,
    };
    use crate::entity::{Dimensions, EntityFlags, EntityState};
    use crate::math::{Aabb, Vec3};
    use crate::simulation::{friction_influenced_speed, mth_cos, mth_sin, yaw_radians};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockShape, BlockState, FluidState, WorldSnapshot};

    const V1_12_2: MinecraftVersion = MinecraftVersion::new(1, 12, 2);
    const V1_20_4: MinecraftVersion = MinecraftVersion::new(1, 20, 4);
    const V1_21_1: MinecraftVersion = MinecraftVersion::new(1, 21, 1);
    const V26_2: MinecraftVersion = MinecraftVersion::new(26, 2, 0);

    #[test]
    fn a_1_13_boat_skips_a_water_layer_above_the_first_by_its_y() {
        // 1.13.2 Boat.java:451 adds the layer's y to the height, so water at y 1 fills the layer
        // (1.889 >= 1) and the level is the next layer up; 1.12.2 and 1.14.4 read the height alone
        let mut world = WorldSnapshot::new();
        world.set_block((0, 1, 0), water_source());
        let bounds = Aabb::new(Vec3::new(0.2, 0.5, 0.2), Vec3::new(0.8, 1.5, 0.8));
        let level = |release| super::water_level_above(bounds, -1.0, &world, release);
        assert_eq!(level(MinecraftVersion::new(1, 13, 2)), 2.0);
        assert_eq!(level(MinecraftVersion::new(1, 12, 2)) < 2.0, true);
    }

    fn water_source() -> BlockState {
        BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            ..BlockState::default()
        }
    }

    fn solid(slipperiness: f64) -> BlockState {
        BlockState {
            shape: Some(BlockShape {
                slipperiness,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    /// A pond of still water one block deep at y 100 over x 0..=1, z 2124..=2126, over stone.
    fn pond() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -2..=3 {
            for z in 2122..=2128 {
                world.set_block((x, 99, z), solid(0.6));
                world.set_block((x, 100, z), water_source());
            }
        }
        world
    }

    #[test]
    fn a_controlled_boat_steps_its_interpolation_on_1_21_2_to_1_21_4_alone() {
        for (release, steps) in [(MinecraftVersion::new(1, 21, 1), 0), (MinecraftVersion::new(1, 21, 3), 3), (MinecraftVersion::new(1, 21, 5), 0)] {
            let mut boat = BoatSimulationState {
                player_passenger: true,
                lerp: super::BoatLerp { steps: 3, target: Vec3::new(0.5, 70.0, 3.5), y_rot: 30.0, x_rot: 0.0 },
                ..boat_on_pond()
            };
            let start = boat.position;
            super::tick_lerp(&mut boat, release);
            if steps == 0 {
                assert_eq!((boat.position, boat.lerp.steps), (start, 3), "{release:?}");
            } else {
                // (a third of the way, Entity.lerpPositionAndRotationStep, and a step fewer left)
                assert_eq!(boat.position.z, start.z + (1.0 / 3.0) * (3.5 - start.z), "{release:?}");
                assert_eq!(boat.lerp.steps, 2);
                assert_eq!(boat.yaw_degrees, (f64::from(boat_on_pond().yaw_degrees) + (1.0 / 3.0) * crate::math::wrap_degrees(30.0 - f64::from(boat_on_pond().yaw_degrees))) as f32);
            }
        }
    }

    fn boat_on_pond() -> BoatSimulationState {
        BoatSimulationState {
            position: Vec3::new(0.5, 100.52334181630349, 2125.5),
            velocity: Vec3::new(0.0, -0.000026689068836262958, 0.0),
            yaw_degrees: -90.0,
            status: BoatStatus::InWater,
            water_level: 100.88888549804688,
            player_passenger: true,
            ..BoatSimulationState::default()
        }
    }

    fn flowing_water(level: u8) -> BlockState {
        BlockState {
            fluid: FluidState::Water,
            fluid_level: level,
            ..BlockState::default()
        }
    }

    /// Water two blocks deep over x -2..=3, z -2..=2 (y 101 and 102), for a boat at y 100.5 whose
    /// box top (101.0625) is in the y 101 layer.
    fn submerged(cell: BlockState) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -2..=3 {
            for z in -2..=2 {
                world.set_block((x, 100, z), solid(0.6));
                world.set_block((x, 101, z), water_source());
                world.set_block((x, 102, z), water_source());
            }
        }
        world.set_block((0, 101, 0), cell);
        world
    }

    fn submerged_boat() -> BoatSimulationState {
        BoatSimulationState {
            position: Vec3::new(0.5, 100.5, 0.5),
            status: BoatStatus::UnderWater,
            ..BoatSimulationState::default()
        }
    }

    #[test]
    fn the_water_over_a_boat_is_its_own_height_in_1_13() {
        // method_17810 has no look at the block above: the top of a source under water is 8/9 in
        // 1.13, 1 from 1.14 (1.13.2 Boat.java:554, 1.14.4 :420): physrec/bf_under_stone
        let status = |release| {
            let mut boat = submerged_boat();
            boat.position.y = 101.95 - f64::from(BOAT_HEIGHT);
            boat.bounding_box = None;
            super::boat_status(&mut boat, &submerged(water_source()), release)
        };
        assert_ne!(status(MinecraftVersion::new(1, 13, 2)), BoatStatus::UnderWater);
        assert_eq!(status(MinecraftVersion::new(1, 14, 4)), BoatStatus::UnderWater);
    }

    #[test]
    fn a_boat_counts_its_fall_out_of_water_and_ends_it_on_the_ground() {
        // Boat.checkFallDamage: in the air a drop counts unless the block under the boat holds
        // water, on the ground the fall ends, on 1.8 only past 3 (1.8.9 BoatEntity.java:400-421,
        // 1.9.4 :632-662, 1.20.4 Boat.java:767-790, 26.3 AbstractBoat.java:720-730); 1.21.4 skips
        // the check for what a player controls (1.21.4 Entity.java:721) (D-183)
        let mut ground = WorldSnapshot::new();
        for x in -2..=2 {
            for z in -2..=2 {
                ground.set_block((x, 63, z), solid(0.6));
            }
        }
        let mut over_water = ground.clone();
        over_water.set_block((0, 68, 0), water_source());
        let tick = |world: &WorldSnapshot, release, y: f64, fall: f64| {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, y, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                player_passenger: true,
                status: BoatStatus::InAir,
                previous_status: BoatStatus::InAir,
                fall_distance: fall,
                ..BoatSimulationState::default()
            };
            tick_boat_in_world(&mut boat, &mut world.clone(), release);
            boat
        };
        let v = MinecraftVersion::new;
        for release in [v(1, 8, 8), V1_12_2, V1_20_4, v(1, 21, 5), V26_2] {
            let air = tick(&ground, release, 69.9, 1.0);
            assert!(!air.on_ground, "{release:?}");
            assert!(air.fall_distance > 1.4, "{release:?} {}", air.fall_distance);
            assert_eq!(tick(&over_water, release, 69.9, 1.0).fall_distance, 1.0, "{release:?}");
            let landed = tick(&ground, release, 64.1, 2.0);
            assert!(landed.on_ground, "{release:?}");
            assert_eq!(landed.fall_distance, if release == v(1, 8, 8) { 2.0 } else { 0.0 }, "{release:?}");
            assert_eq!(tick(&ground, release, 64.1, 5.0).fall_distance, 0.0, "{release:?}");
        }
        assert_eq!(tick(&ground, v(1, 21, 4), 69.5, 1.0).fall_distance, 1.0);
        assert_eq!(tick(&ground, v(1, 21, 4), 64.1, 2.0).fall_distance, 2.0);
        // the base tick's water ends the fall first, so a boat sinking toward stone holds only this
        // tick's drop (Entity.updateWaterState, 1.13.2 Entity.java:1053-1062): physrec/bf_under_stone
        let mut sunk = ground.clone();
        for y in 64..=70 {
            sunk.set_block((0, y, 0), water_source());
        }
        for release in [V1_12_2, MinecraftVersion::new(1, 13, 2), V1_20_4, V26_2] {
            let under = tick(&sunk, release, 66.0, 3.0);
            assert!(under.fall_distance < 1.0, "{release:?} {}", under.fall_distance);
        }
    }

    #[test]
    fn the_level_above_a_boat_counts_any_water_as_full_in_1_13() {
        // Boat.getWaterLevelAbove: max(f, y + height) in 1.13, which any water of a layer at y >= 1
        // fills, so the boat goes on to the layer over it (1.13.2 BoatEntity.java:452, 458); the
        // height alone before and after (1.12.2 :388, 1.14 :448)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 62, 0), water_source());
        let bounds = Aabb::new(
            Vec3::new(-0.1875, 62.95 - f64::from(BOAT_HEIGHT), -0.1875),
            Vec3::new(1.1875, 62.95, 1.1875),
        );
        let above = |release| super::water_level_above(bounds, -0.3, &world, release);
        let source = 62.0 + 8.0_f32 / 9.0;
        assert_eq!(above(V1_12_2), 62.0 + (1.0 - 1.0_f32 / 9.0));
        assert_eq!(above(MinecraftVersion::new(1, 13, 2)), 63.0);
        assert_eq!(above(MinecraftVersion::new(1, 14, 4)), source);
        assert_eq!(above(V1_20_4), source);
        // a layer with no water ends the walk where it is, the same on every release
        let dry = WorldSnapshot::new();
        for release in [V1_12_2, MinecraftVersion::new(1, 13, 2), V1_20_4] {
            assert_eq!(super::water_level_above(bounds, -0.3, &dry, release), 62.0);
        }
    }

    #[test]
    fn submerged_boat_status_separates_source_and_flowing_water() {
        // Boat.isUnderwater: any water cell in the top layer whose surface is above maxY + 0.001
        // makes it UNDER_WATER, and the first that is not a source returns UNDER_FLOWING_WATER
        // (1.20.4 Boat.java:isUnderwater, 1.12.2 Boat.java:481); under
        // water the 1.13+ height is 1.0, and before 1.13 a level-1 cell stands 1 - 2/9 high, still
        // above the top (101.0625)
        for release in [V1_12_2, V1_20_4, V1_21_1, V26_2] {
            let mut boat = submerged_boat();
            let world = submerged(water_source());
            assert_eq!(super::boat_status(&mut boat, &world, release), BoatStatus::UnderWater);
            assert_eq!(boat.water_level, 100.5 + f64::from(BOAT_HEIGHT));

            let mut boat = submerged_boat();
            let world = submerged(flowing_water(1));
            assert_eq!(
                super::boat_status(&mut boat, &world, release),
                BoatStatus::UnderFlowingWater,
                "{release:?}"
            );
        }
        // only the layers from floor(maxY) to ceil(maxY + 0.001) are scanned: flowing water in
        // the layer above the top leaves the boat UNDER_WATER
        let mut world = submerged(water_source());
        world.set_block((0, 102, 0), flowing_water(4));
        let mut boat = submerged_boat();
        assert_eq!(super::boat_status(&mut boat, &world, V1_21_1), BoatStatus::UnderWater);
        // the status picks the buoyancy: UNDER_FLOWING_WATER replaces gravity with -7.0E-4 at 0.9
        // friction, UNDER_WATER keeps gravity with a 0.01F lift at 0.45 (1.20.4 Boat.java:floatBoat)
        let mut flowing = BoatSimulationState {
            status: BoatStatus::UnderFlowingWater,
            velocity: Vec3::new(1.0, 0.0, 0.0),
            ..BoatSimulationState::default()
        };
        float_boat_by_status(&mut flowing, V1_20_4);
        assert_eq!(flowing.velocity.y, -7.0e-4);
        assert_eq!(flowing.velocity.x, f64::from(0.9_f32));
        let mut under = BoatSimulationState {
            status: BoatStatus::UnderWater,
            velocity: Vec3::new(1.0, 0.0, 0.0),
            ..BoatSimulationState::default()
        };
        float_boat_by_status(&mut under, V1_20_4);
        assert_eq!(under.velocity.x, f64::from(0.45_f32));
        assert_eq!(
            under.velocity.y,
            (f64::from(-0.04_f32) + 0.009999999776482582 * 0.06153846016296973) * 0.75
        );
    }

    #[test]
    fn boat_accelerates_and_turns() {
        // the world-less boat at the default release (26.2): floatBoat IN_WATER keeps 0.9F of the
        // turn and falls by -0.04, controlBoat's right key turns it a degree (deltaRotation++) and
        // its forward key pushes it 0.04F along -sin and cos of yRot (1.21.11
        // AbstractBoat.java:514-556, 558-584)
        let release = MinecraftVersion::new(26, 2, 0);
        let mut state = BoatSimulationState {
            forward: true,
            right: true,
            in_water: true,
            ..BoatSimulationState::default()
        };
        tick_boat(&mut state);
        let radians = 1.0_f32 * 0.017453292;
        let sin = mth_sin(f64::from(-radians), release) as f32;
        let cos = mth_cos(f64::from(radians), release) as f32;
        assert_eq!(state.status, BoatStatus::InWater);
        assert_eq!(state.delta_rotation, 1.0);
        assert_eq!(state.yaw_degrees, 1.0);
        assert_eq!(state.yaw, 1.0_f64.to_radians());
        assert_eq!(state.velocity.y, -0.04);
        assert_eq!(state.position.x, f64::from(sin * 0.04));
        assert_eq!(state.position.z, f64::from(cos * 0.04));
        assert!(state.position.x < 0.0 && state.position.z > 0.0);
    }

    #[test]
    fn boat_control_persists_rotation_and_applies_backward_acceleration() {
        // controlBoat: the back key alone pushes it -0.005F, and the turn it keeps 0.9F of in the
        // water (floatBoat) grows by a degree a tick (1.21.11 AbstractBoat.java:575-604)
        let release = MinecraftVersion::new(26, 2, 0);
        let mut state = BoatSimulationState {
            input_down: true,
            right: true,
            in_water: true,
            ..BoatSimulationState::default()
        };

        tick_boat(&mut state);

        assert_eq!(state.delta_rotation, 1.0);
        assert!((state.yaw - 1.0_f64.to_radians()).abs() < 1.0e-12);
        let radians = 1.0_f32 * 0.017453292;
        let sin = mth_sin(f64::from(-radians), release) as f32;
        let cos = mth_cos(f64::from(radians), release) as f32;
        assert_eq!(state.velocity.x, f64::from(sin * -0.005));
        assert_eq!(state.velocity.z, f64::from(cos * -0.005));
        assert!(state.velocity.x > 0.0 && state.velocity.z < 0.0);

        tick_boat(&mut state);
        assert_eq!(state.delta_rotation, f64::from(1.0_f32 * 0.9 + 1.0));
    }

    #[test]
    fn floating_boat_keeps_still_and_its_forward_key_pushes_it_along_its_yaw() {
        let mut world = pond();
        let mut boat = boat_on_pond();

        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        assert_eq!(boat.status, BoatStatus::InWater);
        assert_eq!(boat.water_level, 100.88888549804688);
        assert_eq!(boat.velocity, Vec3::new(0.0, -0.000026689068836262958, 0.0));
        assert_eq!(boat.position, Vec3::new(0.5, 100.52334181630349, 2125.5));

        boat.forward = true;
        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        assert_eq!(boat.velocity.x, 0.03999999910593033);
        assert_eq!(boat.velocity.y, -0.000026689068836262958);
        assert_eq!(boat.position.x, 0.5399999991059303);
        assert_eq!(boat.position.y, 100.52331512723465);
    }

    #[test]
    fn boat_turns_by_float_degrees_and_keeps_the_turn_damped_by_the_water() {
        let mut world = pond();
        let mut boat = BoatSimulationState {
            left: true,
            ..boat_on_pond()
        };

        tick_boat_in_world(&mut boat, &mut world, V1_21_1);
        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        assert_eq!(boat.delta_rotation, f64::from(-1.0_f32 * 0.9 - 1.0));
        assert_eq!(boat.yaw_degrees, -90.0 - 1.0 + (-1.0_f32 * 0.9 - 1.0));
        assert!(boat.velocity.x > 0.0 && boat.velocity.z < 0.0);
    }

    #[test]
    fn steering_clears_the_negative_zero_of_a_boat_set_on_the_water() {
        let mut boat = BoatSimulationState {
            velocity: Vec3::new(0.1, -0.0, 0.0),
            ..BoatSimulationState::default()
        };
        super::control_boat(&mut boat, V1_21_1);
        assert!(boat.velocity.y.is_sign_positive());
    }

    #[test]
    fn a_boat_stops_at_an_entity_it_collides_with() {
        // Entity.collide takes the entity colliders first (26.3 Entity.java:1262-1263) on every
        // path: the modern, the 1.13 and the older one
        for release in [V1_21_1, MinecraftVersion::parse("1.13.2").unwrap(), MinecraftVersion::parse("1.12.2").unwrap()] {
            let mut world = WorldSnapshot::new();
            world.set_entity_colliders(vec![crate::world::EntityCollider {
                bounds: Aabb::new(Vec3::new(2.0, 9.0, 0.0), Vec3::new(3.0, 11.0, 1.0)),
                only_from_above: false,
            }]);
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                velocity: Vec3::new(3.0, 0.0, 0.0),
                no_gravity: true,
                ..BoatSimulationState::default()
            };
            tick_boat_in_world(&mut boat, &mut world, release);
            assert!(boat.position.x < 2.0 - 0.6, "{release:?}: {}", boat.position.x);
            assert!(boat.collided_horizontally, "{release:?}");
        }
    }

    #[test]
    fn a_boat_with_no_gravity_does_not_fall_but_keeps_its_buoyancy() {
        // Boat.floatBoat: `hasNoGravity() ? 0.0 : -0.04F` (1.10.2 Boat.java:498, 1.20.4
        // :592), getFinalGravity() from 1.20.5 (1.20.5 Boat.java:565, Entity.java:1143-1145);
        // the buoyancy's getGravity() / 0.65 stays (1.20.5 Boat.java:598)
        let mut world = WorldSnapshot::new();
        for release in [V1_20_4, V1_21_1] {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                no_gravity: true,
                ..BoatSimulationState::default()
            };
            tick_boat_in_world(&mut boat, &mut world, release);
            assert_eq!(boat.status, BoatStatus::InAir);
            assert_eq!(boat.velocity.y, 0.0, "{release}");
            assert_eq!(boat.position.y, 10.0, "{release}");
        }
        let mut floating = BoatSimulationState {
            status: BoatStatus::InWater,
            previous_status: BoatStatus::InWater,
            water_level: 1.0,
            position: Vec3::new(0.5, 0.5, 0.5),
            no_gravity: true,
            ..BoatSimulationState::default()
        };
        float_boat_by_status(&mut floating, V1_21_1);
        assert_eq!(floating.velocity.y, (0.0 + 0.5 / f64::from(BOAT_HEIGHT) * (0.04 / 0.65)) * 0.75);
    }

    #[test]
    fn a_1_13_vehicle_checks_the_sub_epsilon_move_once_a_block_shape() {
        // 1.13's getCollisionBoxes streams each block's VoxelShape whole and Shapes.collide
        // drops a move under 1.0E-7 before each shape, not between a shape's boxes (1.13.2
        // LevelReader.java:113-155, Shapes.java:151-160): pressing 5e-8 short of a lone
        // stair keeps the 5e-8 its first box leaves, though the stair has a second box
        let mut world = WorldSnapshot::new();
        world.set_block_shapes(
            (1, 0, 0),
            BlockState::default(),
            vec![
                BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                },
                BlockShape {
                    bounds: Aabb::new(Vec3::new(0.5, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                    ..BlockShape::full_block()
                },
            ],
        );
        let bounds = Aabb::new(Vec3::new(0.4, 0.0, 0.0), Vec3::new(1.0 - 5.0e-8, 1.0, 1.0));
        let push = Vec3::new(0.7, 0.0, 0.0);
        let gap = 1.0 - (1.0 - 5.0e-8);
        let (_, moved) = legacy_collide(&world, MinecraftVersion::new(1, 13, 2), bounds, push, 0.0, false, &[]);
        assert_eq!(moved.x, gap);
        // a full cube of the next row after it is a later shape, which drops the 5e-8
        world.set_block(
            (1, 0, 1),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let across = Aabb::new(Vec3::new(0.4, 0.0, 0.5), Vec3::new(1.0 - 5.0e-8, 1.0, 1.5));
        let (_, moved) = legacy_collide(&world, MinecraftVersion::new(1, 13, 2), across, push, 0.0, false, &[]);
        assert_eq!(moved.x, 0.0);
    }

    #[test]
    fn a_1_13_vehicle_meets_a_moving_block_in_the_cell_past_its_sweep() {
        // 1.13.2 method_16378 walks floor(min) - 1 to ceil(max) on each axis (LevelReader.java:114-119):
        // a block pushed east at half its way stands in the cell past the swept box, its shape
        // reaching half a block back into it
        let mut world = WorldSnapshot::new();
        world.set_block((2, 0, 0), BlockState::default());
        world.set_moving_block((2, 0, 0), &[Aabb::new(Vec3::new(-0.5, 0.0, 0.0), Vec3::new(0.5, 1.0, 1.0))], false);
        let bounds = Aabb::new(Vec3::new(0.0, 0.0, 0.2), Vec3::new(0.9, 0.5, 0.8));
        let (_, moved) = legacy_collide(&world, MinecraftVersion::new(1, 13, 2), bounds, Vec3::new(1.0, 0.0, 0.0), 0.0, false, &[]);
        assert_eq!(moved.x, 1.5 - 0.9);
    }

    #[test]
    fn a_legacy_vehicle_inside_the_world_border_meets_stone_past_it() {
        // Level.doesBoxCollide before 1.14: to an entity inside the border a cell outside it is
        // stone (1.8.8 Level.java:1068-1071, 1.12.2 Level.java:1127-1132, 1.13.2
        // LevelReader.java:133-134); the border of size 10 at 0, 0 ends at x 5
        let mut world = WorldSnapshot::new();
        world.set_chunk_loaded(0, 0, true);
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        let bounds = Aabb::new(Vec3::new(3.5, 0.0, 0.2), Vec3::new(4.4, 0.5, 0.8));
        for release in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 12, 2), MinecraftVersion::new(1, 13, 2)] {
            let (_, moved) = legacy_collide(&world, release, bounds, Vec3::new(2.0, 0.0, 0.0), 0.0, false, &[]);
            assert_eq!(moved.x, 5.0 - 4.4, "{release:?}");
        }
        // one already outside it meets no stone (`|| !bl2`, 1.8.8 Level.java:1069)
        let outside = bounds.translated(Vec3::new(4.0, 0.0, 0.0));
        let (_, moved) = legacy_collide(&world, MinecraftVersion::new(1, 12, 2), outside, Vec3::new(2.0, 0.0, 0.0), 0.0, false, &[]);
        assert_eq!(moved.x, 2.0);
    }

    #[test]
    fn a_legacy_vehicle_meets_the_border_stone_where_the_client_holds_no_chunk() {
        // the stone stands behind blockExists / hasChunkAt, which the client answers true for every
        // chunk before 1.14 (1.8.8 ClientChunkCache.java:31-33, 1.12.2 MultiPlayerLevel.java:103-105,
        // 1.13.2 LevelReader.java:136, MultiPlayerLevel.java:110-112): no chunk is loaded here
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        let bounds = Aabb::new(Vec3::new(3.5, 0.0, 0.2), Vec3::new(4.4, 0.5, 0.8));
        for release in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 12, 2), MinecraftVersion::new(1, 13, 2)] {
            let (_, moved) = legacy_collide(&world, release, bounds, Vec3::new(2.0, 0.0, 0.0), 0.0, false, &[]);
            assert_eq!(moved.x, 5.0 - 4.4, "{release:?}");
        }
    }

    #[test]
    fn a_no_gravity_boat_bounces_with_no_gravity_compensation_from_26_2() {
        // Entity.restituteMovementAfterCollisions compensates portionWithMovement *
        // getEffectiveGravity(), 0 under NoGravity (26.2 Entity.java:848, 855, 887-889,
        // 1532-1533); a boat is not living, so the block's restitution counts 0.8 of it
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 0.75,
                ..BlockState::default()
            },
        );
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 1.02, 0.5),
            velocity: Vec3::new(0.0, -0.03, 0.0),
            no_gravity: true,
            ..BoatSimulationState::default()
        };
        tick_boat_in_world(&mut boat, &mut world, MinecraftVersion::new(26, 2, 0));
        assert_eq!(boat.position.y, 1.0);
        assert_eq!(boat.velocity.y, 0.03 * f64::from(0.75_f32 * 0.8));
    }

    #[test]
    fn boat_falls_by_float_gravity_before_1_20_5() {
        let mut world = WorldSnapshot::new();
        let mut modern = BoatSimulationState {
            position: Vec3::new(0.5, 10.0, 0.5),
            ..BoatSimulationState::default()
        };
        let mut legacy = modern;

        tick_boat_in_world(&mut modern, &mut world, V1_21_1);
        tick_boat_in_world(&mut legacy, &mut world, V1_20_4);

        assert_eq!(modern.status, BoatStatus::InAir);
        assert_eq!(modern.velocity.y, -0.04);
        assert_eq!(legacy.velocity.y, -0.03999999910593033);
        assert_eq!(legacy.position.y, 10.0 - 0.03999999910593033);
    }

    #[test]
    fn a_world_less_boat_on_land_keeps_its_grounds_friction_from_tick_to_tick() {
        // getStatus takes landFriction from the ground before every floatBoat, whose halving for a
        // player's boat lasts the tick alone (26.2 AbstractBoat.java:197, 370, 540-542; 1.21.11 :371, 541-543)
        let mut boat = BoatSimulationState {
            velocity: Vec3::new(0.1, 0.0, 0.0),
            on_ground: true,
            land_friction: 0.6,
            player_passenger: true,
            ..BoatSimulationState::default()
        };
        tick_boat(&mut boat);
        tick_boat(&mut boat);
        assert_eq!(boat.status, BoatStatus::OnLand);
        assert_eq!(boat.land_friction, 0.6);
        assert_eq!(
            boat.velocity.x,
            0.1 * f64::from(0.6_f32) * f64::from(0.6_f32)
        );
    }

    #[test]
    fn boat_on_land_takes_the_average_friction_and_halves_it_for_its_rider() {
        let mut world = WorldSnapshot::new();
        for x in -2..=2 {
            for z in -2..=2 {
                world.set_block((x, -1, z), solid(if x < 0 { 0.98 } else { 0.6 }));
            }
        }
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.1, 0.0, 0.0),
            player_passenger: true,
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        let friction = [0.98_f32, 0.98, 0.98, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6]
            .iter()
            .sum::<f32>()
            / 9.0;
        assert_eq!(boat.status, BoatStatus::OnLand);
        assert_eq!(boat.land_friction, friction / 2.0);
        assert_eq!(boat.velocity.x, 0.1 * f64::from(friction));
        assert!(boat.on_ground);
        assert_eq!(boat.velocity.y, -0.0);
        assert!(boat.velocity.y.is_sign_negative());
    }

    #[test]
    fn a_paddle_stroke_takes_the_boats_status_again_after_the_move() {
        // getPaddleSound in the paddle loop after the move: a stroke's start gives back the ground's
        // friction, unhalved (1.20.4 Boat.java:317-337, 402-413), which a silent boat does not;
        // the phase grows by PI/8 from 1.12, by 0.01 before and with no sound (1.9.4 :268-269)
        let mut world = WorldSnapshot::new();
        for x in -3..=3 {
            for z in -3..=3 {
                world.set_block((x, -1, z), solid(0.5));
            }
        }
        let start = BoatSimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            forward: true,
            player_passenger: true,
            ..BoatSimulationState::default()
        };
        let step = (std::f64::consts::PI / 8.0) as f32;

        let mut boat = start;
        tick_boat_in_world(&mut boat, &mut world, V1_20_4);
        assert_eq!(boat.paddle_phases, [step, step]);
        assert_eq!(boat.land_friction, 0.25);
        tick_boat_in_world(&mut boat, &mut world, V1_20_4);
        assert_eq!(boat.paddle_phases, [step * 2.0, step * 2.0]);
        assert_eq!(boat.land_friction, 0.5);
        // the window holds two strokes' starts: PI/4 itself is within it
        tick_boat_in_world(&mut boat, &mut world, V1_20_4);
        assert_eq!(boat.land_friction, 0.5);
        tick_boat_in_world(&mut boat, &mut world, V1_20_4);
        assert_eq!(boat.land_friction, 0.25);

        let mut silent = BoatSimulationState { silent: true, ..start };
        tick_boat_in_world(&mut silent, &mut world, V1_20_4);
        tick_boat_in_world(&mut silent, &mut world, V1_20_4);
        assert_eq!(silent.land_friction, 0.25);

        let mut old = start;
        tick_boat_in_world(&mut old, &mut world, MinecraftVersion::new(1, 9, 4));
        tick_boat_in_world(&mut old, &mut world, MinecraftVersion::new(1, 9, 4));
        assert_eq!(old.land_friction, 0.25);
        assert_eq!(old.paddle_phases[0], (0.01_f64 as f32 as f64 + 0.01) as f32);

        // a paddle at rest has no phase, and a boat without a player does not row
        let mut resting = BoatSimulationState { paddle_phases: [1.0, 1.0], forward: false, ..start };
        tick_boat_in_world(&mut resting, &mut world, V1_20_4);
        assert_eq!(resting.paddle_phases, [0.0, 0.0]);
        let mut empty = BoatSimulationState { player_passenger: false, ..start };
        tick_boat_in_world(&mut empty, &mut world, V1_20_4);
        assert_eq!(empty.paddle_phases, [0.0, 0.0]);
    }

    #[test]
    fn boat_landing_keeps_a_positive_zero_from_26_2() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, -1, 0), solid(0.6));
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 0.01, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &mut world, V26_2);

        assert!(boat.on_ground);
        assert_eq!(boat.position.y, 0.0);
        assert!(boat.velocity.y == 0.0 && boat.velocity.y.is_sign_positive());
    }

    #[test]
    fn boat_entering_water_from_the_air_is_set_on_its_surface() {
        let mut world = pond();
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 100.7, 2125.5),
            velocity: Vec3::new(0.0, -0.3, 0.0),
            status: BoatStatus::InAir,
            last_yd: -0.3,
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        assert_eq!(boat.status, BoatStatus::InWater);
        assert_eq!(boat.position.y, f64::from(101.0_f32 - 0.5625) + 0.101);
        assert_eq!(boat.velocity.y.to_bits(), 0.0_f64.to_bits());
        assert_eq!(boat.last_yd.to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn a_boat_set_on_the_water_keeps_no_negative_zero_through_its_paddles_from_1_14() {
        // floatBoat's multiply(1.0, 0.0, 1.0) leaves -0.0 and controlBoat's Vec3.add(.., 0.0, ..)
        // clears it (1.14.4 Boat.java:617, 673-674, 26.3 AbstractBoat.java:547, 610); before
        // 1.14 floatBoat writes 0.0 itself and nothing adds to it
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 14, 0),
            V1_20_4,
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.7, 2125.5),
                velocity: Vec3::new(0.19, -0.3, 0.0),
                status: BoatStatus::InAir,
                last_yd: -0.3,
                ..BoatSimulationState::default()
            };

            tick_boat_in_world(&mut boat, &mut pond(), release);

            assert_eq!(boat.velocity.y.to_bits(), 0.0_f64.to_bits(), "{release:?}");
            assert_eq!(boat.last_yd.to_bits(), 0.0_f64.to_bits(), "{release:?}");
        }
    }

    #[test]
    fn settled_boat_does_not_enter_the_water_it_floats_on() {
        let mut world = pond();
        let mut boat = BoatSimulationState {
            status: BoatStatus::InAir,
            settle_status: true,
            ..boat_on_pond()
        };

        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        assert_eq!(boat.previous_status, BoatStatus::InWater);
        assert_eq!(boat.position.y, 100.52334181630349);
    }

    #[test]
    fn boat_keeps_its_box_and_takes_the_legacy_water_height_before_1_13() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 1,
                ..BlockState::default()
            },
        );
        world.set_block((0, 1, 0), water_source());
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 0.3, 0.5),
            status: BoatStatus::InWater,
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &mut world, V1_12_2);

        assert_eq!(boat.water_level, f64::from(1.0_f32 - 2.0 / 9.0));
        assert_eq!(boat.status, BoatStatus::InWater);
        let kept = boat.bounding_box.expect("a boat keeps its box before 1.17");
        assert_eq!(kept.min.y, boat.position.y);
        assert_eq!(kept.max.y - kept.min.y, 0.5625);
    }

    #[test]
    fn a_box_of_no_height_stops_a_vehicle_before_1_13() {
        // the head at the start of its way on 1.8, drawn back flat (1.8.8 MovingPistonBlock.java:125-137,
        // 166-197): Level.getCollisionBoxes keeps what a block lists, whatever its height
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), crate::world::BlockState::default());
        let flat = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0));
        world.set_moving_block((0, 100, 0), &[flat], false);
        let bounds = Aabb::new(Vec3::new(0.1, 100.0, 0.1), Vec3::new(0.9, 100.7, 0.9));
        let (_, moved) = legacy_collide(&world, MinecraftVersion::new(1, 8, 8), bounds, Vec3::new(0.0, -0.02, 0.0), 0.0, false, &[]);
        assert_eq!(moved.y, 0.0);
    }

    #[test]
    fn legacy_boat_floats_by_its_slices_in_water_and_goes_by_its_riders_input() {
        let mut world = WorldSnapshot::new();
        for x in -3..=3 {
            for z in 2122..=2128 {
                world.set_block((x, 99, z), water_source());
                world.set_block((x, 100, z), water_source());
            }
        }
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 100.89859999937713, 2125.5),
            velocity: Vec3::new(0.0, 0.014819999392181656, 0.0),
            yaw_degrees: -90.0,
            bounding_box: Some(Aabb::new(
                Vec3::new(-0.25, 100.89859999937713, 2124.75),
                Vec3::new(1.25, 101.49860002321898, 2126.25),
            )),
            ..BoatSimulationState::default()
        };
        let v1_8_8 = MinecraftVersion::new(1, 8, 8);

        tick_boat_in_world(&mut boat, &mut world, v1_8_8);

        assert_eq!(boat.position, Vec3::new(0.5, 100.90541999894812, 2125.5));
        assert_eq!(boat.velocity, Vec3::new(0.0, 0.006478999511145081, 0.0));

        boat.legacy.rider_yaw = 270.0;
        boat.legacy.rider_forward = 0.98;
        tick_boat_in_world(&mut boat, &mut world, v1_8_8);

        assert_eq!(
            boat.position,
            Vec3::new(0.5034300001178682, 100.90389899863808, 2125.500000000041)
        );
        assert_eq!(
            boat.velocity,
            Vec3::new(
                0.003395700149400534,
                -0.0014449502764071972,
                4.0493318341968365e-11
            )
        );
        assert_eq!(boat.yaw_degrees, -90.0);
        assert!(boat.legacy.speed_multiplier > 0.07);
    }

    #[test]
    fn boat_stops_on_the_wall_it_runs_into() {
        let mut world = WorldSnapshot::new();
        world.set_block((2, 0, 0), solid(0.6));
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &mut world, V1_21_1);

        assert!(boat.collided_horizontally);
        assert_eq!(boat.position.x, 2.0 - 0.6875);
        assert_eq!(boat.velocity.x, 0.0);
    }

    #[test]
    fn horse_jumps_and_moves_forward() {
        // the world-less horse at the default release (26.2): the full riding jump goes up by
        // JUMP_STRENGTH and 0.4F forward along its yaw, then LivingEntity.travel on the ground
        // (gravity 0.08, the 0.98F drag) (1.21.11 AbstractHorse.java:751-759, LivingEntity.java)
        let release = MinecraftVersion::new(26, 2, 0);
        let mut state = HorseSimulationState {
            forward: 1.0,
            jump_requested: true,
            on_ground: true,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut state);
        let walk = friction_influenced_speed(f64::from(0.1_f32), 0.6, release);
        assert_eq!(state.position.y, f64::from(0.42_f32));
        assert_eq!(state.position.z, f64::from(0.4_f32) + walk);
        assert_eq!(state.position.x, 0.0);
        assert_eq!(
            state.velocity.y,
            (f64::from(0.42_f32) - 0.08) * f64::from(0.98_f32)
        );
        assert_eq!(
            state.velocity.z,
            (f64::from(0.4_f32) + walk) * f64::from(0.6_f32 * 0.91)
        );
        assert!(!state.on_ground);
    }

    #[test]
    fn horse_strafe_changes_lateral_motion() {
        // AbstractHorse.getRiddenInput halves the strafe; in the air the ridden mob moves at a
        // tenth of its speed (getFlyingSpeed) and keeps 0.91F of it (1.21.11
        // AbstractHorse.java:733-740, LivingEntity.java)
        let release = MinecraftVersion::new(26, 2, 0);
        let mut state = HorseSimulationState {
            strafe: 1.0,
            yaw: 0.25,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut state);
        let yaw = yaw_radians(0.25_f64.to_degrees() as f32, release);
        let speed = 0.5 * f64::from(0.1_f32 * 0.1);
        assert_eq!(state.position.x, speed * mth_cos(yaw, release));
        assert_eq!(state.position.z, speed * mth_sin(yaw, release));
        assert_eq!(state.velocity.x, state.position.x * f64::from(0.91_f32));
        assert_eq!(state.yaw, 0.25);
    }

    #[test]
    fn horse_diagonal_input_is_normalized_before_move_relative() {
        // the input (half the strafe, the forward) longer than 1 is normalized by moveRelative
        let mut state = HorseSimulationState {
            forward: 1.0,
            strafe: 1.0,
            movement_speed: 1.0,
            ..HorseSimulationState::default()
        };

        tick_horse(&mut state);

        let speed = f64::from(1.0_f32 * 0.1);
        assert!((state.position.x / state.position.z - 0.5).abs() < 1.0e-12);
        assert!((state.position.x.hypot(state.position.z) - speed).abs() < 1.0e-12);
    }

    #[test]
    fn a_world_less_horse_with_no_gravity_takes_no_gravity() {
        // LivingEntity.travel: no gravity in the air, and no falling adjustment in water, under
        // NoGravity (1.21.11 LivingEntity.java:2317, 2521-2533, Entity.java:1443-1445)
        let mut air = HorseSimulationState {
            velocity: Vec3::new(0.0, 0.1, 0.0),
            no_gravity: true,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut air);
        assert_eq!(air.velocity.y, 0.1 * f64::from(0.98_f32));
        let mut water = HorseSimulationState {
            velocity: Vec3::new(0.0, 0.1, 0.0),
            in_water: true,
            no_gravity: true,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut water);
        assert_eq!(water.velocity.y, 0.1 * 0.800000011920929);
    }

    #[test]
    fn a_horse_in_water_takes_the_water_input_and_slow_down() {
        // LivingEntity.travel in water: 0.02F of the input, then 0.8F of the motion and the
        // falling adjustment of gravity / 16 (1.21.11 LivingEntity.java)
        let mut state = HorseSimulationState {
            forward: 1.0,
            in_water: true,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut state);
        assert_eq!(state.position.z, f64::from(0.02_f32));
        assert_eq!(state.velocity.z, f64::from(0.02_f32) * f64::from(0.8_f32));
        assert_eq!(state.velocity.y, -0.08 / 16.0);
        // floatInWaterWhileRidden: deeper than getFluidJumpThreshold (0.4) the ridden horse rises
        // 0.03999999910593033 more (26.2 LivingEntity.java:2420-2424, 2490-2496; 1.21.11
        // :2336-2342, 2404-2410)
        let floated = |water_height: f64| {
            let mut state = HorseSimulationState {
                in_water: true,
                water_height,
                ..HorseSimulationState::default()
            };
            tick_horse(&mut state);
            state.velocity.y
        };
        assert_eq!(floated(0.4), -0.08 / 16.0);
        assert_eq!(floated(0.9), -0.08 / 16.0 + 0.03999999910593033);
    }

    #[test]
    fn a_boat_seats_its_passenger_by_the_release() {
        let boat = |release_yaw: f32| BoatSimulationState {
            position: Vec3::new(10.0, 64.0, 20.0),
            yaw_degrees: release_yaw,
            ..BoatSimulationState::default()
        };
        let seat = |release| BoatSeat::player(release);
        // 1.8: 0.4 along its yaw by Math.cos and sin, at -0.3 and the player's -0.35 (1.8.8
        // Boat.java:136-138, 370-376), Java 8's, the JVM 1.8.8 declares (B315)
        let v1_8_8 = MinecraftVersion::new(1, 8, 8);
        let radians = 90.0 * std::f64::consts::PI / 180.0;
        assert_eq!(
            boat_passenger_position(&boat(90.0), seat(v1_8_8), v1_8_8),
            Vec3::new(
                10.0 + crate::trig::java8::cos(radians) * 0.4,
                64.0 + -0.3 + -0.35,
                20.0 + crate::trig::java8::sin(radians) * 0.4
            )
        );
        // 1.9 - 1.20.1: the float sum of the boat's -0.1 and the player's -0.35; with two aboard
        // the first sits 0.2F forward, Vec3(0.2F, 0, 0) turned by -yRot * 0.017453292F -
        // 1.5707964F with Mth's tables (1.12.2 Boat.java:590-608)
        assert_eq!(
            boat_passenger_position(&boat(90.0), seat(V1_12_2), V1_12_2),
            Vec3::new(10.0, 64.0 + f64::from(-0.45_f32), 20.0)
        );
        let first = BoatSeat {
            passengers: 2,
            ..seat(V1_12_2)
        };
        let angle = -90.0_f32 * 0.017453292 - 1.5707964;
        let (cos, sin) = (
            mth_cos(f64::from(angle), V1_12_2),
            mth_sin(f64::from(angle), V1_12_2),
        );
        assert_eq!(
            boat_passenger_position(&boat(90.0), first, V1_12_2),
            Vec3::new(
                10.0 + (f64::from(0.2_f32) * cos + 0.0 * sin),
                64.0 + f64::from(-0.45_f32),
                20.0 + (0.0 * cos - f64::from(0.2_f32) * sin)
            )
        );
        // a raft of 1.19.4 seats at its 0.3 (1.19.4 Boat.java:153-155)
        let v1_19_4 = MinecraftVersion::new(1, 19, 4);
        let raft = BoatSeat {
            raft: true,
            ..seat(v1_19_4)
        };
        assert_eq!(
            boat_passenger_position(&boat(0.0), raft, v1_19_4).y,
            64.0 + f64::from((0.3 - 0.35) as f32)
        );
        // 1.20.2 - 1.20.4: a chest boat's 0.15F forward and height / 3.0F, JOML's float rotateY,
        // the player's -0.6F (1.20.2 Boat.java:155-171, ChestBoat.java:46-48)
        let v1_20_4 = MinecraftVersion::new(1, 20, 4);
        let chest = BoatSeat {
            chest: true,
            ..seat(v1_20_4)
        };
        let seated = boat_passenger_position(&boat(90.0), chest, v1_20_4);
        assert_eq!(seated.x, f64::from(-0.15_f32) + 10.0);
        assert_eq!(
            seated.y,
            f64::from(0.5625_f32 / 3.0) + 64.0 + f64::from(-0.6_f32)
        );
        assert!((seated.z - 20.0).abs() < 1.0e-7);
        // from 1.20.5: Vec3(0, rideHeight, 0.15F).yRot with Mth, less the player's 0.6; a raft
        // rides at height * 0.8888889F (1.21.11 AbstractBoat.java:132-147, Raft.java:14-16)
        let v1_21_11 = MinecraftVersion::new(1, 21, 11);
        let chest = BoatSeat {
            chest: true,
            ..seat(v1_21_11)
        };
        let yaw = f64::from(-90.0_f32 * 0.017453292);
        let (cos, sin) = (mth_cos(yaw, v1_21_11), mth_sin(yaw, v1_21_11));
        assert_eq!(
            boat_passenger_position(&boat(90.0), chest, v1_21_11),
            Vec3::new(
                10.0 + (0.0 * cos + f64::from(0.15_f32) * sin),
                64.0 + f64::from(0.5625_f32 / 3.0) + -0.6,
                20.0 + (f64::from(0.15_f32) * cos - 0.0 * sin)
            )
        );
        let raft = BoatSeat {
            raft: true,
            ..seat(v1_21_11)
        };
        assert_eq!(
            boat_passenger_position(&boat(0.0), raft, v1_21_11).y,
            64.0 + f64::from(0.5625_f32 * 0.8888889) + -0.6
        );
    }

    #[test]
    fn a_boats_player_turns_with_it_and_stays_within_105_degrees() {
        // Boat.positionRider turns the passenger by deltaRotation, clampRotation holds it within
        // 105 degrees of the boat: yRot += clamped - wrapped before 1.17, setYRot(yRot + clamped
        // - wrapped) from 1.17 (1.16.5 Boat.java:673-675, 715-722; 1.17 :669-671, 716-723)
        let boat = BoatSimulationState {
            yaw_degrees: 0.0,
            delta_rotation: 2.0,
            ..BoatSimulationState::default()
        };
        let v1_16_5 = MinecraftVersion::new(1, 16, 5);
        let v1_17 = MinecraftVersion::new(1, 17, 0);
        assert_eq!(boat_passenger_yaw(100.0, &boat, v1_16_5), 102.0);
        assert_eq!(boat_passenger_yaw(110.0, &boat, v1_16_5), 105.0);
        assert_eq!(boat_passenger_yaw(-110.0, &boat, v1_17), -105.0);
        let still = BoatSimulationState {
            delta_rotation: 0.0,
            ..boat
        };
        let wrapped = 250.7_f32 % 360.0 - 360.0;
        assert_eq!(
            boat_passenger_yaw(250.7, &still, v1_16_5),
            250.7 + (-105.0 - wrapped)
        );
        assert_eq!(
            boat_passenger_yaw(250.7, &still, v1_17),
            250.7 + -105.0 - wrapped
        );
        // 1.8 does not turn its rider (1.8.8 Boat.java:370-376)
        assert_eq!(
            boat_passenger_yaw(110.0, &boat, MinecraftVersion::new(1, 8, 8)),
            110.0
        );
    }

    #[test]
    fn vehicle_moves_before_passenger_is_attached() {
        let mut world = pond();
        let mut vehicle = BoatSimulationState {
            forward: true,
            ..boat_on_pond()
        };
        let mut passenger = EntityState {
            position: Vec3::new(50.0, 50.0, 50.0),
            velocity: Vec3::ZERO,
            dimensions: Dimensions::new(0.3, 1.8),
            flags: EntityFlags::default(),
            no_gravity: false,
            gravity: None,
            step_height: None,
        };
        let start = vehicle.position;
        let seat = BoatSeat::player(V1_21_1);

        tick_boat_with_passenger(&mut vehicle, &mut world, V1_21_1, &mut passenger, seat);

        assert_ne!(vehicle.position, start);
        assert_eq!(
            passenger.position,
            boat_passenger_position(&vehicle, seat, V1_21_1)
        );
    }

    #[test]
    fn a_boat_seats_its_rider_where_it_ended_on_every_release() {
        // Entity#rideTick: the passenger's tick, then the vehicle's positionRider at the boat's new
        // place (1.8.8 Entity.java rideTick and Boat.positionRider cos and sin of the yaw times 0.4;
        // 1.9.4 Boat.positionRider, 1.21.2 AbstractBoat), for each release
        let mut world = pond();
        for (name, release) in crate::reach_fixtures_e2_tests::releases_from("1.8.8") {
            let mut vehicle = BoatSimulationState { forward: true, ..boat_on_pond() };
            let mut passenger = EntityState {
                position: Vec3::new(50.0, 50.0, 50.0),
                velocity: Vec3::ZERO,
                dimensions: Dimensions::new(0.3, 1.8),
                flags: EntityFlags::default(),
                no_gravity: false,
                gravity: None,
                step_height: None,
            };
            let seat = BoatSeat::player(release);
            tick_boat_with_passenger(&mut vehicle, &mut world, release, &mut passenger, seat);
            assert_eq!(passenger.position, boat_passenger_position(&vehicle, seat, release), "{name}");
            if !release.at_least(1, 9, 0) {
                let radians = f64::from(vehicle.yaw_degrees) * std::f64::consts::PI / 180.0;
                assert!((passenger.position.x - (vehicle.position.x + radians.cos() * 0.4)).abs() < 1.0e-6, "{name}");
            } else {
                // a lone rider sits on the boat's centre line from 1.9
                let offset = passenger.position - vehicle.position;
                assert!(offset.x.abs() < 1.0e-9 && offset.z.abs() < 1.0e-9, "{name}: {offset:?}");
            }
        }
    }

    #[test]
    fn a_vehicle_bounces_off_slime_at_0_8_only_from_1_9() {
        // SlimeBlock.onLanded: 1.8.9 sets motionY = -motionY for every entity (1.8.8 SlimeBlock.java:33-39);
        // from 1.9 an entity that is not living keeps 0.8 of it (1.9.4 SlimeBlock.java:34-44)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let landed = |release: MinecraftVersion, living: bool| {
            let mut body = crate::simulation::SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                half_width: 0.5,
                height: 0.6,
                ..crate::simulation::SimulationState::default()
            };
            super::fall_on(&mut body, &world, None, release, living);
            body.velocity.y
        };
        assert_eq!(landed(MinecraftVersion::new(1, 8, 8), false), 0.5);
        assert_eq!(landed(MinecraftVersion::new(1, 9, 4), false), 0.5 * 0.8);
        assert_eq!(landed(MinecraftVersion::new(1, 21, 11), false), 0.5 * 0.8);
        assert_eq!(landed(MinecraftVersion::new(1, 21, 11), true), 0.5);
    }

    #[test]
    fn a_boat_takes_the_slime_step_on_only_from_1_15() {
        // Entity.move runs onSteppedOn before 1.15 only for an entity that canClimb (1.12.2
        // Entity.java:830, 839; 1.14.4 :533, 542), and Boat.makeStepSound is false (1.12.2
        // Boat.java:117; 1.13.2 :130); from 1.15 the onGround entity steps on it (1.15
        // Entity.java:590), and SlimeBlock.stepOn scales x and z by 0.4 + |y| * 0.2
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let stepped = |release: MinecraftVersion| {
            let mut body = crate::simulation::SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.3, 0.0, -0.2),
                half_width: 0.6875,
                height: 0.5625,
                on_ground: true,
                boat: true,
                ..crate::simulation::SimulationState::default()
            };
            super::apply_effects_inside(&mut body, &world, Some((0, 0, 0)), release);
            body.velocity
        };
        for release in [V1_12_2, MinecraftVersion::new(1, 14, 4)] {
            assert_eq!(stepped(release), Vec3::new(0.3, 0.0, -0.2));
        }
        assert_eq!(
            stepped(MinecraftVersion::new(1, 16, 5)),
            Vec3::new(0.3 * 0.4, 0.0, -0.2 * 0.4)
        );
    }

    /// A block that only holds an entity inside it (a cobweb, powder snow, a bush, a bubble column
    /// with water): no collision box.
    fn inside_block(effect: crate::world::BlockEffect, fluid: FluidState) -> BlockState {
        BlockState {
            fluid,
            fluid_source: fluid != FluidState::Empty,
            shape: Some(BlockShape {
                effect,
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    /// Two ticks of a boat falling from rest at y 10 into the blocks at y 10 around it.
    fn falling_boat_by(block: BlockState, release: MinecraftVersion) -> [BoatSimulationState; 2] {
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block((x, 10, z), block);
            }
        }
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 10.0, 0.5),
            ..BoatSimulationState::default()
        };
        tick_boat_in_world(&mut boat, &mut world, release);
        let first = boat;
        tick_boat_in_world(&mut boat, &mut world, release);
        [first, boat]
    }

    #[test]
    fn a_cobweb_the_boat_is_inside_slows_its_next_move() {
        // Entity.move's checkInsideBlocks and Boat.tick's own (1.21.1 Entity.java:738,
        // Boat.java:308; applyEffectsFromBlocks from 1.21.2, 1.21.11 AbstractBoat.java:236-237)
        // run WebBlock.entityInside's makeStuckInBlock, which the next Entity.move multiplies
        // in and clears with the motion (1.21.11 Entity.java:765-771)
        use crate::world::BlockEffect;
        let web = inside_block(BlockEffect::Web, FluidState::Empty);
        let stuck = Vec3::new(0.25, f64::from(0.05_f32), 0.25);
        for release in [V1_21_1, MinecraftVersion::new(1, 21, 11), V26_2] {
            let [first, second] = falling_boat_by(web, release);
            assert_eq!(first.position.y, 10.0 - 0.04, "{release}");
            assert_eq!(first.stuck_speed_multiplier, stuck, "{release}");
            assert_eq!(
                second.position.y,
                first.position.y + -0.08 * stuck.y,
                "{release}"
            );
            assert_eq!(second.velocity, Vec3::ZERO, "{release}");
            assert_eq!(second.stuck_speed_multiplier, stuck, "{release}");
        }
        // the kept box of 1.9 - 1.16 (float gravity before 1.20.5)
        let [first, second] = falling_boat_by(web, V1_12_2);
        assert_eq!(first.stuck_speed_multiplier, stuck);
        let falling = first.velocity.y + f64::from(-0.04_f32);
        assert_eq!(second.position.y, first.position.y + falling * stuck.y);
        assert_eq!(second.velocity, Vec3::ZERO);
        // 1.8's EntityBoat: Entity.moveEntity's doBlockCollisions has BlockWeb setInWeb, which
        // the next moveEntity slows by and stops the motion (1.8.8 Boat.java:314)
        let [first, second] = falling_boat_by(web, MinecraftVersion::new(1, 8, 8));
        assert_eq!(first.stuck_speed_multiplier, stuck);
        assert!(first.position.y - second.position.y < 0.01);
        assert_eq!(second.velocity, Vec3::ZERO);
    }

    #[test]
    fn powder_snow_holds_a_boat_wherever_its_feet_are_and_a_berry_bush_does_not() {
        // PowderSnowBlock.entityInside: !(entity instanceof LivingEntity) || its feet are in it
        // (1.17 PowderSnowBlock.java:54-56, 1.21.11 :59-61); SweetBerryBushBlock.entityInside
        // holds a LivingEntity alone
        use crate::world::BlockEffect;
        let powder_snow = inside_block(BlockEffect::PowderSnow, FluidState::Empty);
        let [first, _] = falling_boat_by(powder_snow, MinecraftVersion::new(1, 17, 1));
        // the boat's feet are in the air under the snow
        assert_eq!(first.position.y.floor(), 9.0);
        assert_eq!(
            first.stuck_speed_multiplier,
            Vec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32))
        );
        let bush = inside_block(BlockEffect::SweetBerryBush, FluidState::Empty);
        let [first, _] = falling_boat_by(bush, V1_21_1);
        assert_eq!(first.stuck_speed_multiplier, Vec3::ZERO);
    }

    /// One tick of a boat under water in the blocks at x -1..=1 z -1..=1: stone at y 99, water at
    /// y 100 (`block` at x 0 z 0, one of the nine blocks the boat is inside), `above` at y 101.
    fn submerged_boat_by(
        block: BlockState,
        above: BlockState,
        release: MinecraftVersion,
    ) -> BoatSimulationState {
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block((x, 99, z), solid(0.6));
                world.set_block((x, 100, z), water_source());
                world.set_block((x, 101, z), above);
            }
        }
        world.set_block((0, 100, 0), block);
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 100.2, 0.5),
            status: BoatStatus::UnderWater,
            previous_status: BoatStatus::UnderWater,
            ..BoatSimulationState::default()
        };
        tick_boat_in_world(&mut boat, &mut world, release);
        boat
    }

    #[test]
    fn a_submerged_bubble_column_pushes_a_boat_twice_a_tick() {
        // BubbleColumnBlock.entityInside -> Entity.onInsideBubbleColumn, from Entity.move's
        // checkInsideBlocks and Boat.tick's own (1.16 Boat.java:295, 323; 1.20.5 :279, 305),
        // from 1.21.2 from both of Boat.tick's applyEffectsFromBlocks (1.21.11
        // AbstractBoat.java:236-237, BubbleColumnBlock.java:50-59)
        use crate::world::BlockEffect;
        let water = inside_block(BlockEffect::None, FluidState::Water);
        let column = inside_block(BlockEffect::BubbleUp, FluidState::Water);
        for release in [
            MinecraftVersion::new(1, 16, 5),
            V1_21_1,
            MinecraftVersion::new(1, 21, 11),
        ] {
            let plain = submerged_boat_by(water, water, release);
            let pushed = submerged_boat_by(column, water, release);
            assert_eq!(pushed.position, plain.position, "{release}");
            assert_eq!(
                pushed.velocity.y,
                plain.velocity.y + 0.06 + 0.06,
                "{release}"
            );
        }
    }

    #[test]
    fn a_boat_with_no_water_under_it_reports_the_start_of_the_search() {
        // Boat.checkInWater starts waterLevel at Double.MIN_VALUE, the smallest positive double,
        // until 1.16.5 and at -Double.MAX_VALUE from 1.17, and keeps it when no water is found
        // (1.9.4 BoatEntity.java:428, 1.12.2 :459, 1.15 :524, 1.16.5 :535; 1.17 :531)
        let world = WorldSnapshot::new();
        for (release, level) in [
            (MinecraftVersion::new(1, 9, 4), f64::from_bits(1)),
            (MinecraftVersion::new(1, 12, 2), f64::from_bits(1)),
            (MinecraftVersion::new(1, 15, 2), f64::from_bits(1)),
            (MinecraftVersion::new(1, 16, 5), f64::from_bits(1)),
            (MinecraftVersion::new(1, 17, 0), -f64::MAX),
            (MinecraftVersion::new(26, 3, 0), -f64::MAX),
        ] {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                water_level: 7.0,
                ..BoatSimulationState::default()
            };
            assert_eq!(super::boat_status(&mut boat, &world, release), BoatStatus::InAir, "{release}");
            assert_eq!(boat.water_level.to_bits(), level.to_bits(), "{release}");
        }
    }

    #[test]
    fn the_boats_recorded_move_starts_after_the_float_snap_from_1_21_5() {
        // floatBoat's setPos records no movement, so the move's own Movement starts at the
        // snapped height and only the second applyEffectsFromBlocks, with nothing recorded, goes
        // from oldPosition (1.21.5 AbstractBoat.java:216-239, 520-527, Entity.java:707-721,
        // 809-822): a column under where the tick began pushes once, not in both passes
        use crate::world::BlockEffect;
        let mut world = WorldSnapshot::new();
        world.set_block((0, 98, 0), inside_block(BlockEffect::None, FluidState::Water));
        world.set_block((0, 99, 0), inside_block(BlockEffect::BubbleUp, FluidState::Water));
        world.set_block((0, 100, 0), inside_block(BlockEffect::None, FluidState::Water));
        let pushed = |release: MinecraftVersion| {
            let old_position = Vec3::new(0.5, 98.7, 0.5);
            let snapped = Vec3::new(0.5, 100.3, 0.5);
            let mut body = super::boat_body(
                &BoatSimulationState {
                    position: snapped,
                    ..BoatSimulationState::default()
                },
                release,
            );
            let outcome = super::VehicleMove {
                requested: Vec3::ZERO,
                moved: Vec3::ZERO,
                support: None,
                motion_at_landing: Vec3::ZERO,
                fluid_heights: None,
            };
            super::apply_boat_block_effects(&mut body, &world, release, old_position, snapped, outcome);
            body.velocity.y
        };
        // before 1.21.5 both passes go from oldPosition (1.21.4 Entity.java:788-802)
        assert_eq!(pushed(MinecraftVersion::new(1, 21, 4)), 0.06 + 0.06);
        assert_eq!(pushed(MinecraftVersion::new(1, 21, 5)), 0.06);
        assert_eq!(pushed(MinecraftVersion::new(26, 3, 0)), 0.06);
    }

    #[test]
    fn sixty_ticks_over_a_column_end_in_the_server_s_push() {
        let tick = |down: bool, velocity_y: f64, time: i32| {
            let mut boat = BoatSimulationState {
                velocity: Vec3::new(0.1, velocity_y, 0.0),
                bubble_time: time,
                above_bubble_column: true,
                bubble_column_direction_is_down: down,
                ..BoatSimulationState::default()
            };
            super::tick_bubble_column(&mut boat);
            boat
        };
        assert_eq!(tick(false, 0.25, 1).velocity.y, 0.6);
        assert_eq!(tick(true, 0.25, 1).velocity.y, 0.25 - 0.7);
        let waiting = tick(false, 0.25, 2);
        assert_eq!((waiting.velocity.y, waiting.bubble_time, waiting.above_bubble_column), (0.25, 1, false));
        // (off the column the timer is gone with no push)
        let mut left = BoatSimulationState { bubble_time: 1, velocity: Vec3::new(0.0, 0.25, 0.0), ..BoatSimulationState::default() };
        super::tick_bubble_column(&mut left);
        assert_eq!((left.bubble_time, left.velocity.y), (0, 0.25));
    }

    #[test]
    fn a_bubble_column_under_the_air_does_not_move_the_boat_its_player_steers() {
        // AbstractBoat.onAboveBubbleColumn moves the boat on the server alone (1.16
        // Boat.java:202-212, 1.21.11 AbstractBoat.java:155-162, 26.3 :151-158)
        use crate::world::BlockEffect;
        let water = inside_block(BlockEffect::None, FluidState::Water);
        let column = inside_block(BlockEffect::BubbleUp, FluidState::Water);
        let air = BlockState::default();
        let plain = submerged_boat_by(water, air, V1_21_1);
        let above_column = submerged_boat_by(column, air, V1_21_1);
        assert_eq!(above_column.velocity, plain.velocity);
        assert_eq!(above_column.position, plain.position);
    }

    #[test]
    fn lava_pushes_a_boat_at_0_007_in_an_ultrawarm_dimension() {
        // Entity.updateFluidHeightAndDoFluidPushing: 0.007 where the dimension is ultrawarm
        // (FAST_LAVA), else 0.0023333333333333335 (1.16 Entity.java:1046, 1.21.11 :1561,
        // 26.3 :1704)
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block(
                    (x, 100, z),
                    BlockState {
                        fluid: FluidState::Lava,
                        fluid_level: 2,
                        fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                        ..BlockState::default()
                    },
                );
            }
        }
        let pushed = |ultra_warm: bool, release: MinecraftVersion| {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                velocity: Vec3::new(0.1, 0.0, 0.0),
                ultra_warm,
                ..BoatSimulationState::default()
            };
            super::push_boat_by_fluids(&mut boat, &world, release);
            boat.velocity.x
        };
        for release in [MinecraftVersion::new(1, 16, 5), V1_21_1, V26_2] {
            assert_eq!(pushed(true, release), 0.1 + 0.007, "{release}");
            assert_eq!(
                pushed(false, release),
                0.1 + 0.0023333333333333335,
                "{release}"
            );
        }
    }

    #[test]
    fn lava_pushes_a_boat_in_water_only_from_1_16_2() {
        // (1.16 Entity.java:1041-1044 returns before the lava while in water; 1.16.2 :1064-1070 pushes it too)
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block((x, 100, z), BlockState { fluid: FluidState::Water, ..BlockState::default() });
                world.set_block(
                    (x, 101, z),
                    BlockState {
                        fluid: FluidState::Lava,
                        fluid_level: 2,
                        fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                        ..BlockState::default()
                    },
                );
            }
        }
        let pushed = |release: MinecraftVersion| {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.5, 0.5),
                ..BoatSimulationState::default()
            };
            super::push_boat_by_fluids(&mut boat, &world, release);
            (boat.in_water, boat.velocity.x)
        };
        for release in [MinecraftVersion::new(1, 16, 0), MinecraftVersion::new(1, 16, 1)] {
            assert_eq!(pushed(release), (true, 0.0), "{release}");
        }
        for release in [MinecraftVersion::new(1, 16, 2), MinecraftVersion::new(1, 16, 5)] {
            let (in_water, x) = pushed(release);
            assert!(in_water && x > 0.0, "{release}: {x}");
        }
    }

    #[test]
    fn a_boat_takes_the_legacy_water_scan_before_1_13() {
        // Entity.handleWaterMovement before 1.13: the box shrunk by 0.4F vertically and 0.001 all
        // round reaches the layer at y 100, and water is touched where the cells top (ceil maxY)
        // reaches its surface (1.12.2 Entity.java:1043, Level.java:1616-1655; 1.8.8
        // Entity.java:837, Level.java:1533); its flows are normalized and pushed by 0.014. From
        // 1.13 the surface (here 100.333) must reach the box bottom (1.13.2 Entity.java:2511-2551).
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block(
                    (x, 100, z),
                    BlockState {
                        fluid: FluidState::Water,
                        fluid_level: 5,
                        fluid_flow: Vec3::new(0.5, 0.0, 0.0),
                        ..BlockState::default()
                    },
                );
            }
        }
        let pushed = |release: MinecraftVersion, y: f64| {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, y, 0.5),
                ..BoatSimulationState::default()
            };
            super::push_boat_by_fluids(&mut boat, &world, release);
            (boat.in_water, boat.velocity.x)
        };
        for release in [MinecraftVersion::new(1, 8, 8), V1_12_2] {
            assert_eq!(pushed(release, 100.5), (true, 0.014), "{release}");
            // the boat (0.5625 high, 0.6 in 1.8) is lower than the 0.8 the box loses, so its
            // corners cross over and the AxisAlignedBB constructor orders them again (1.12.2
            // AABB.java:39-45; 1.8.8 AABB.java:23-29): from y 100.7 the box is 100.8635 - 101.099
            // (1.8: 100.901 - 101.099), whose cells reach the layers at 100 and 101 with top 102
            assert_eq!(pushed(release, 100.7), (true, 0.014), "{release}");
        }
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 14, 0),
        ] {
            assert_eq!(pushed(release, 100.5), (false, 0.0), "{release}");
        }
    }

    /// A lily pad: its box (1.13+ LilyPadBlock.SHAPE, 1.8 - 1.12 a 0.015625 slab of the block).
    fn lily_pad(release: MinecraftVersion) -> BlockState {
        let (inset, height) = if release.at_least(1, 13, 0) {
            (0.0625, 0.09375)
        } else {
            (0.0, 0.015625)
        };
        BlockState {
            shape: Some(BlockShape {
                effect: crate::world::BlockEffect::LilyPad,
                bounds: Aabb::new(
                    Vec3::new(inset, 0.0, inset),
                    Vec3::new(1.0 - inset, height, 1.0 - inset),
                ),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    #[test]
    fn a_lily_pad_is_no_ground_for_a_boats_friction() {
        // Boat.getGroundFriction leaves a lily pad out of the sum and the count (1.13.2
        // Boat.java:491, 1.21.11 AbstractBoat.java:437; before 1.13 it gives a boat no box,
        // 1.12.2 WaterlilyBlock.java:23-25): on it alone the boat is in the air (NaN)
        let release = V1_21_1;
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), lily_pad(release));
        let bounds = super::boat_box_at(Vec3::new(0.5, 100.09375, 0.5));
        assert!(super::ground_friction(bounds, &world, MinecraftVersion::new(26, 3, 0)).is_nan());
        world.set_block((1, 100, 0), solid(0.98));
        assert_eq!(super::ground_friction(bounds, &world, MinecraftVersion::new(26, 3, 0)), 0.98);
    }

    #[test]
    fn a_boats_ground_friction_is_the_float_average_of_what_it_spans() {
        // Boat.getGroundFriction sums the floats by x, then z, and divides by the count as a float
        // (1.13.2 Boat.java:476-494, 26.3 AbstractBoat.java:449-470): the blocks meeting the
        // slice under the box, no block whose edge it only touches
        let mut world = WorldSnapshot::new();
        let (ice, stone, blue_ice, slime) = (0.98_f32, 0.6_f32, 0.989_f32, 0.8_f32);
        for (position, friction) in [
            ((0, 99, 0), ice),
            ((0, 99, 1), stone),
            ((1, 99, 0), blue_ice),
            ((1, 99, 1), slime),
        ] {
            world.set_block(position, solid(f64::from(friction)));
        }
        let spanning = super::boat_box_at(Vec3::new(1.0, 100.0, 1.0));
        assert_eq!(
            super::ground_friction(spanning, &world, MinecraftVersion::new(26, 3, 0)).to_bits(),
            ((ice + stone + blue_ice + slime) / 4.0).to_bits(),
        );
        // an edge on x 1 leaves the blocks past it out, and a hair past it brings them in
        let touching = super::boat_box_at(Vec3::new(1.0 - super::BOAT_HALF_WIDTH, 100.0, 0.5));
        assert_eq!(touching.max.x, 1.0);
        assert_eq!(
            super::ground_friction(touching, &world, MinecraftVersion::new(26, 3, 0)).to_bits(),
            ((ice + stone) / 2.0).to_bits(),
        );
        let over = super::boat_box_at(Vec3::new(1.0 - super::BOAT_HALF_WIDTH + 1.0e-6, 100.0, 0.5));
        assert_eq!(
            super::ground_friction(over, &world, MinecraftVersion::new(26, 3, 0)).to_bits(),
            ((ice + stone + blue_ice + slime) / 4.0).to_bits(),
        );
    }

    #[test]
    fn a_boat_falls_through_a_lily_pad_before_1_13() {
        // BlockLilyPad.addCollisionBoxesToList adds no box for a boat (1.8.8 WaterlilyBlock.java:
        // 37-41, 1.9.4 - 1.12.2 :23-25); 1.13 drops the override (1.13.2 WaterlilyBlock.java)
        let fall = |release: MinecraftVersion| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 100, 0), lily_pad(release));
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.15, 0.5),
                velocity: Vec3::new(0.0, -0.1, 0.0),
                ..BoatSimulationState::default()
            };
            tick_boat_in_world(&mut boat, &mut world, release);
            boat
        };
        for release in [MinecraftVersion::new(1, 8, 8), V1_12_2] {
            let boat = fall(release);
            assert!(boat.position.y < 100.015625 && !boat.on_ground, "{release}");
        }
        let boat = fall(MinecraftVersion::new(1, 13, 2));
        assert_eq!(boat.position.y, 100.09375);
        assert!(boat.on_ground);
    }

    #[test]
    fn a_legacy_boat_clears_the_snow_layers_at_its_corners() {
        // 1.8 EntityBoat.onUpdate sets air at the snow layers (and lily pads) at its four corners,
        // at its feet and the block above, before it moves (1.8.8 Boat.java:290-306); a snow
        // layer the boat reaches elsewhere still holds it (three layers: a 0.25 box)
        let snow = BlockState {
            shape: Some(BlockShape {
                effect: crate::world::BlockEffect::SnowLayer,
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.25, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let fall = |at: (i32, i32, i32)| {
            let mut world = WorldSnapshot::new();
            world.set_block(at, snow);
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.3, 0.5),
                velocity: Vec3::new(0.0, -0.1, 0.0),
                ..BoatSimulationState::default()
            };
            tick_boat_in_world(&mut boat, &mut world, MinecraftVersion::new(1, 8, 8));
            boat
        };
        let cleared = fall((0, 100, 0));
        assert!(cleared.position.y < 100.25 && !cleared.on_ground);
        let held = fall((1, 100, 0));
        assert_eq!(held.position.y, 100.25);
        assert!(held.on_ground);
    }

    // B290: fixtures that move each of these gates one release either way.

    #[test]
    fn a_boat_turns_its_player_from_1_9_4_and_not_on_1_8_8() {
        // 1.8.8 Boat.java:370-376 leaves the rider's yaw; 1.9.4 Boat.java:589-607 adds deltaRotation
        let boat = BoatSimulationState {
            yaw_degrees: 0.0,
            delta_rotation: 2.0,
            ..BoatSimulationState::default()
        };
        assert_eq!(boat_passenger_yaw(100.0, &boat, MinecraftVersion::new(1, 8, 8)), 100.0);
        assert_eq!(boat_passenger_yaw(100.0, &boat, MinecraftVersion::new(1, 9, 4)), 102.0);
    }

    #[test]
    fn a_boats_clamp_adds_in_its_releases_float_order() {
        // clampRotation: `yRot += clamped - wrapped` in 1.16.5 (Boat.java:720), setYRot(getYRot()
        // + clamped - wrapped) from 1.17 (Boat.java:721): float sums in another order
        let boat = BoatSimulationState {
            yaw_degrees: 291.246_215_820_312_5,
            delta_rotation: 0.0,
            ..BoatSimulationState::default()
        };
        let yaw = 90.021_507_263_183_59_f32;
        let wrapped = super::wrap_degrees_f32(yaw - boat.yaw_degrees);
        let clamped = wrapped.clamp(-105.0, 105.0);
        let old = yaw + (clamped - wrapped);
        let new = yaw + clamped - wrapped;
        assert_ne!(old, new);
        assert_eq!(boat_passenger_yaw(yaw, &boat, MinecraftVersion::new(1, 16, 5)), old);
        assert_eq!(boat_passenger_yaw(yaw, &boat, MinecraftVersion::new(1, 17, 0)), new);
    }

    #[test]
    fn lava_pushes_a_boat_out_of_water_from_1_16_and_not_on_1_15_2() {
        // 1.15.2 Entity.java has no lava current; 1.16 Entity.java:1041-1046 pushes a boat not in water
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block(
                    (x, 100, z),
                    BlockState {
                        fluid: FluidState::Lava,
                        fluid_level: 2,
                        fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                        ..BlockState::default()
                    },
                );
            }
        }
        let pushed = |release: MinecraftVersion| {
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                velocity: Vec3::new(0.1, 0.0, 0.0),
                ..BoatSimulationState::default()
            };
            super::push_boat_by_fluids(&mut boat, &world, release);
            boat.velocity.x
        };
        assert_eq!(pushed(MinecraftVersion::new(1, 15, 2)), 0.1);
        assert_eq!(pushed(MinecraftVersion::new(1, 16, 0)), 0.1 + 0.0023333333333333335);
    }

    #[test]
    fn a_client_stops_a_landing_it_does_not_own_on_1_21_1_and_not_from_1_21_2() {
        // 1.21.1 Entity.java:692 stops every landing; 1.21.2 Entity.java:721-724 only where the side simulates
        use crate::moving_entity::{tick_moving_entity, MovingEntityInputs, MovingEntityKind, MovingEntityState};
        let mut world = WorldSnapshot::new();
        for x in -6..=6 {
            for z in -6..=6 {
                world.set_block((x, 99, z), solid(0.6));
            }
        }
        let mut landed = |release| {
            let mut client = MovingEntityState::new(
                MovingEntityKind::PrimedTnt,
                Vec3::new(0.5, 100.1, 0.5),
                Vec3::new(0.0, -0.3, 0.0),
            );
            tick_moving_entity(&mut client, &mut world, &MovingEntityInputs::default(), release);
            client.velocity.y
        };
        assert_eq!(landed(MinecraftVersion::new(1, 21, 1)).abs(), 0.0);
        assert!(landed(MinecraftVersion::new(1, 21, 2)) > 0.1);
    }

    fn stepping_body() -> crate::simulation::SimulationState {
        crate::simulation::SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.5,
            height: 1.0,
            on_ground: true,
            ..crate::simulation::SimulationState::default()
        }
    }

    #[test]
    fn a_vehicles_step_takes_the_lowest_face_that_clears_the_way_from_1_21() {
        // 1.20.6 Entity.java:908 lifts by maxUpStep and comes down; 1.21 Entity.java:926 tries
        // the candidate heights from the lowest
        let shaped = |bounds: Aabb| BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let mut world = WorldSnapshot::new();
        world.set_block((0, -1, 0), solid(0.6));
        world.set_block((1, -1, 0), solid(0.6));
        world.set_block((1, 0, 0), shaped(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.125, 1.0))));
        world.set_block((1, 1, 0), shaped(Aabb::new(Vec3::new(0.0, 0.2, 0.0), Vec3::new(1.0, 1.0, 1.0))));
        let rules = super::MoveRules {
            max_step: 0.6,
            ..super::MoveRules::MINECART
        };
        let moved = |release| {
            let mut body = stepping_body();
            super::move_vehicle(&mut body, &world, release, Vec3::new(0.3, 0.42, 0.0), rules, |_, _| {})
                .moved
        };
        assert_eq!(moved(MinecraftVersion::new(1, 21, 0)), Vec3::new(0.3, 0.125, 0.0));
        let lifted = moved(MinecraftVersion::new(1, 20, 6));
        assert!(lifted.y > 0.19, "{lifted:?}");
    }

    #[test]
    fn a_vehicles_tiny_move_along_one_axis_is_dropped_until_1_17_1() {
        // 1.17.1 Entity.java:828-833 walks a single-axis move through Shapes.collide, which drops
        // one under 1.0E-7 (Shapes.java:229-231) and so collides vertically (Entity.java:565);
        // 1.18 Entity.java:771 collides it with the shapes listed, none here
        let world = WorldSnapshot::new();
        let moved = |release| {
            let mut body = crate::simulation::SimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                half_width: 0.3,
                height: 0.6,
                ..crate::simulation::SimulationState::default()
            };
            let moved = super::move_vehicle(
                &mut body,
                &world,
                release,
                Vec3::new(0.0, -1.0e-8, 0.0),
                super::MoveRules::MINECART,
                |_, _| {},
            )
            .moved;
            (moved.y, body.collided_vertically)
        };
        assert_eq!(moved(MinecraftVersion::new(1, 17, 1)), (0.0, true));
        assert_eq!(moved(MinecraftVersion::new(1, 18, 0)), (-1.0e-8, false));
    }

    #[test]
    fn a_vehicle_stopped_short_of_a_wall_by_a_hair_stays_put_until_1_17_1() {
        // 1.17.1's walk drops what a block leaves under 1.0E-7 at once (Shapes.java:270-272); 1.18
        // drops it only before the next shape (Shapes.java:199-209), and the blocks are listed last,
        // after the entities and the border (Entity.java:795-807)
        let mut world = WorldSnapshot::new();
        world.set_block((1, 100, 0), BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() });
        let moved = |release| {
            let mut body = crate::simulation::SimulationState {
                position: Vec3::new(0.7 - 5.0e-8, 100.0, 0.5),
                half_width: 0.3,
                height: 0.6,
                ..crate::simulation::SimulationState::default()
            };
            super::move_vehicle(&mut body, &world, release, Vec3::new(0.5, 0.0, 0.0), super::MoveRules::MINECART, |_, _| {}).moved.x
        };
        assert_eq!(moved(MinecraftVersion::new(1, 17, 1)), 0.0);
        let left = moved(MinecraftVersion::new(1, 18, 0));
        assert!(left > 0.0 && left < 1.0e-7, "{left}");
    }

    #[test]
    fn a_vehicle_bounces_off_a_bed_from_1_12_and_lands_on_one_before() {
        // 1.11.2 Block.java:674 onLanded sets motionY 0; 1.12.2 BedBlock.java:152-158 bounces
        // what is not sneaking by 0.66, and 0.8 of that for what is not living
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Bed,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let landed = |release: MinecraftVersion| {
            let mut body = crate::simulation::SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                half_width: 0.5,
                height: 0.6,
                ..crate::simulation::SimulationState::default()
            };
            super::fall_on(&mut body, &world, None, release, false);
            body.velocity.y
        };
        assert_eq!(landed(MinecraftVersion::new(1, 11, 2)), 0.0);
        assert_eq!(landed(MinecraftVersion::new(1, 12, 2)), 0.5 * 0.6600000262260437 * 0.8);
    }
}
#[cfg(test)]
#[path = "vehicle_gate_tests.rs"]
mod gate_fixtures;
