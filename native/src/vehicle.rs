use crate::block_traversal::Movement;
use crate::collision::{clip_axis, collide, vanilla_equal, Axis, CollisionRules, StepUp};
use crate::entity::EntityState;
use crate::legacy_fluid;
use crate::math::{atan2_vanilla, wrap_degrees_f32, Aabb, Vec3};
use crate::simulation::{
    apply_block_effects, apply_inside_block_effects, block_speed_factor, blocks_inside,
    blocks_inside_moves, fluid_falling_adjusted_y, friction_influenced_speed, landing_position,
    main_supporting_block, movements_this_tick, moves_position, mth_cos, mth_sin,
    position_of_bounding_box, push_by_fluid, relative_movement, water_contact, yaw_radians,
    SimulationState, VersionPhysicsRules,
};
use crate::trig;
use crate::version::MinecraftVersion;
use crate::world::{
    BlockEffect, BlockState, CollisionQueryEntity, EntityCollisionContext, FluidState,
    WorldSnapshot,
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
/// snow the boat is inside sets for its next move; `ultra_warm` its dimension's (the fast lava).
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
    pub bubble_time: i32,
    pub bubble_column_direction_is_down: bool,
    pub above_bubble_column: bool,
    pub player_passenger: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub on_ground: bool,
    pub bounding_box: Option<Aabb>,
    pub stuck_speed_multiplier: Vec3,
    pub ultra_warm: bool,
    pub legacy: LegacyBoatState,
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
    /// A player alone in a plain boat (1.8.9 PlayerEntity.java:980-982, 1.20.2
    /// PlayerEntity.java:1027-1029, 1.20.5 Player.DEFAULT_VEHICLE_ATTACHMENT).
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
///   offset -0.3 plus the rider's (1.8.9 BoatEntity.java:136-138, 370-376);
/// - 1.9 - 1.20.1: the seat's forward offset (0.2F or -0.6F with two aboard) as Vec3(f, 0, 0)
///   turned by the float -yRot * 0.017453292F - 1.5707964F with Mth's cos and sin, at the float
///   sum of the boat's mounted offset (-0.1; a raft's 0.3, from 1.20 0.25) and the passenger's
///   (1.9.4 BoatEntity.java:571-589, 1.17 :658-676, 1.19.3 :153-155, 1.20 :153-155, 666-684);
/// - 1.20.2 - 1.20.4: getPassengerAttachmentPos, JOML's float Vector3f(0, height / 3.0F (a raft's
///   height * 0.8888889F), offset).rotateY(-yRot * 0.017453292F), plus the passenger's float
///   riding offset (1.20.2 BoatEntity.java:155-171, Entity.java:1856-1877);
/// - from 1.20.5: Vec3(0, rideHeight, offset).yRot(-yRot * 0.017453292F) with Mth's cos and sin,
///   less the passenger's vehicle attachment (1.20.5 BoatEntity.java:144-161, Entity.java:
///   1876-1891, Vec3d.java:202-209; 1.21.11 AbstractBoat.java:136-152, Boat.java:15-17,
///   Raft.java:15-17).
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
            position.x + trig::cos(radians) * 0.4,
            position.y + -0.3 + seat.passenger_offset,
            position.z + trig::sin(radians) * 0.4,
        );
    }
    let mut offset = if seat.chest && release.at_least(1, 19, 0) {
        0.15_f32
    } else {
        0.0
    };
    if seat.passengers > 1 {
        offset = if seat.index == 0 { 0.2 } else { -0.6 };
        if seat.animal {
            offset = if release.at_least(1, 19, 0) {
                offset + 0.2
            } else {
                (f64::from(offset) + 0.2) as f32
            };
        }
    }
    let yaw = -boat.yaw_degrees * 0.017453292;
    if !release.at_least(1, 20, 2) {
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
    if !release.at_least(1, 20, 5) {
        let sin = trig::sin(f64::from(yaw)) as f32;
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

/// JOML's Math.cosFromSin of a float angle (1.20.2 - 1.20.4's Vector3f.rotateY): the float root of
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
/// from 1.17; 1.9.4 BoatEntity.java:590-608, 1.16.5 :681-683, 723-730, 1.17 :677-679, 724-731,
/// 1.21.11 AbstractBoat.java:616-628, 666-673). 1.8's boat does not turn its rider.
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
    if release.at_least(1, 17, 0) {
        yaw + clamped - wrapped
    } else {
        yaw + (clamped - wrapped)
    }
}

/// Ticks a boat in its world first, then seats its passenger at the boat's new position.
pub fn tick_boat_with_passenger(
    vehicle: &mut BoatSimulationState,
    world: &WorldSnapshot,
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
/// rider gave it, then Entity.move.
pub fn tick_boat_in_world(
    state: &mut BoatSimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    if !release.at_least(1, 9, 0) {
        tick_legacy_boat(state, world, release);
        return;
    }
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
    float_boat(state, world, release);
    control_boat(state, release);
    let mut body = boat_body(state, release);
    let velocity = state.velocity;
    let cleared = legacy_boat_world(world, release, body.bounds(), velocity, None);
    let move_world = cleared.as_ref().unwrap_or(world);
    let outcome = move_vehicle(
        &mut body,
        move_world,
        release,
        velocity,
        MoveRules::BOAT,
        |body, support| apply_effects_inside(body, move_world, support, release),
    );
    apply_boat_block_effects(&mut body, world, release, old_position, outcome);
    state.last_yd = outcome.motion_at_landing.y;
    state.stuck_speed_multiplier = body.stuck_speed_multiplier;
    state.position = body.position;
    state.velocity = body.velocity;
    state.bounding_box = body.bounding_box;
    state.on_ground = body.on_ground;
    state.collided_horizontally = body.collided_horizontally;
    state.collided_vertically = body.collided_vertically;
    state.yaw = f64::from(state.yaw_degrees).to_radians();
}

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

fn boat_bounds(state: &BoatSimulationState) -> Aabb {
    state
        .bounding_box
        .unwrap_or_else(|| boat_box_at(state.position))
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
        bounding_box: (!release.at_least(1, 17, 0)).then(|| boat_bounds(state)),
        stuck_speed_multiplier: state.stuck_speed_multiplier,
        sweet_berry_bush_immune: true,
        boat: true,
        ..SimulationState::default()
    }
}

/// Entity.move's checkInsideBlocks (before 1.21.2): the stepOn of the block under the entity and
/// the blocks inside its box. Before 1.15 Entity.move runs onSteppedOn only for an entity that
/// canClimb (1.8.9 Entity.java:668, 677; 1.12.2 :830, 839; 1.14 :602, 611), which a boat does not
/// (1.8.9 BoatEntity.java:98; 1.12.2 :120; 1.13.2 :132); from 1.15 it is the onGround entity's
/// that does not bypass stepping effects (1.15 Entity.java:585).
fn apply_effects_inside(
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
/// checkBlockCollision, 1.9.4 BoatEntity.java:261, 276; 1.16 :298, 328; 1.20.5 :297, 327) or
/// twice from 1.21.2, which applies none in the move (applyEffectsFromBlocks, the tick's
/// recorded moves then the whole of it: 1.21.11 AbstractBoat.java:236-237, Entity.java:822-833;
/// 26.3 AbstractBoat.java:236-237). A sweet berry bush does not hold a boat (no LivingEntity).
fn apply_boat_block_effects(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    old_position: Vec3,
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
    let recorded = movements_this_tick(
        body,
        old_position,
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

/// FluidState.getHeight of the water at a position; before 1.13 BlockLiquid's
/// getBlockLiquidHeight: 1 under water only for a level whose low bits are 0.
pub(crate) fn water_height(world: &WorldSnapshot, position: BlockPosition, release: MinecraftVersion) -> f32 {
    if release.at_least(1, 13, 0) {
        return world.fluid_height_at(position) as f32;
    }
    let level = world.block(position).fluid_level;
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
    let friction = ground_friction(bounds, world);
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
    for x in bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32 {
        for y in bounds.max.y.floor() as i32..top.ceil() as i32 {
            for z in bounds.min.z.floor() as i32..bounds.max.z.ceil() as i32 {
                let block = world.block((x, y, z));
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
    let mut water_level = if release.at_least(1, 13, 0) {
        -f64::MAX
    } else {
        f64::from_bits(1)
    };
    let mut in_water = false;
    for x in bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32 {
        for y in bounds.min.y.floor() as i32..(bounds.min.y + 0.001).ceil() as i32 {
            for z in bounds.min.z.floor() as i32..bounds.max.z.ceil() as i32 {
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
    let top = (bounds.max.y - last_yd).ceil() as i32;
    'layers: for y in bounds.max.y.floor() as i32..top {
        let mut height = 0.0_f32;
        for x in bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32 {
            for z in bounds.min.z.floor() as i32..bounds.max.z.ceil() as i32 {
                if is_water(world.fluid_at((x, y, z))) {
                    height = height.max(water_height(world, (x, y, z), release));
                }
                if height >= 1.0 {
                    continue 'layers;
                }
            }
        }
        return y as f32 + height;
    }
    (top + 1) as f32
}

/// Boat.getGroundFriction: the float average friction of the blocks whose collision shape meets
/// the slice under the box (NaN, so no land, when none does). A lily pad is left out of both: by
/// `instanceof LilyPadBlock` from 1.13 (1.13.2 BoatEntity.java:492, 1.20.4 :506; 1.21.5
/// AbstractBoat.java:455, 1.21.11 :453), by the box it gives no boat before (1.9.4 - 1.12.2
/// LilyPadBlock.java:22-26, which the friction reads with the boat: 1.12.2 BoatEntity.java:431).
fn ground_friction(bounds: Aabb, world: &WorldSnapshot) -> f32 {
    let slice = Aabb::new(
        Vec3::new(bounds.min.x, bounds.min.y - 0.001, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
    );
    let (min_x, max_x) = (
        slice.min.x.floor() as i32 - 1,
        slice.max.x.ceil() as i32 + 1,
    );
    let (min_y, max_y) = (
        slice.min.y.floor() as i32 - 1,
        slice.max.y.ceil() as i32 + 1,
    );
    let (min_z, max_z) = (
        slice.min.z.floor() as i32 - 1,
        slice.max.z.ceil() as i32 + 1,
    );
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

/// Entity.baseTick's fluid push: the water's current, and from 1.16 the lava's.
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
        // round, its water's flows summed and normalized (1.8.9 Entity.java:840, World.java:1545;
        // 1.12.2 Entity.java:1048-1063, World.java:1635-1674)
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
    if release.at_least(1, 16, 0) {
        let lava = world.fluid_contact(FluidState::Lava, bounds, release);
        push_by_fluid(&mut body, lava, lava_flow_scale(state.ultra_warm), release);
    }
    state.velocity = body.velocity;
}

/// Entity's lava current scale (1.16+): 0.007 in an ultrawarm dimension (FAST_LAVA from
/// 1.21.11), 0.0023333333333333335 elsewhere, for every entity it pushes (1.16 Entity.java:1049,
/// 1.19.2 :1069, 1.21.4 :1341, 1.21.11 :1519, 26.3 :1704).
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
        // Level.noCollision(boat, box) (client1.21.11 AbstractBoat.java:539): the boat's
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
        if fits || !release.at_least(1, 21, 0) {
            state.position.y = target_y;
            if state.bounding_box.is_some() {
                state.bounding_box = Some(boat_box_at(state.position));
            }
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
/// the buoyancy toward the water level.
fn float_boat_by_status(state: &mut BoatSimulationState, release: MinecraftVersion) {
    let double_gravity = release.at_least(1, 20, 5);
    let mut vertical = if double_gravity {
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
/// toward where it went, by at most 20 degrees.
fn tick_legacy_boat(
    state: &mut BoatSimulationState,
    world: &WorldSnapshot,
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
    velocity.x += -trig::sin(radians) * multiplier * forward * f64::from(0.05_f32);
    velocity.z += trig::cos(radians) * multiplier * forward * f64::from(0.05_f32);
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
        sweet_berry_bush_immune: true,
        boat: true,
        ..SimulationState::default()
    };
    let velocity = state.velocity;
    let cleared = legacy_boat_world(world, release, bounds, velocity, Some(state.position));
    let world = cleared.as_ref().unwrap_or(world);
    // Entity.moveEntity's doBlockCollisions, the only one of 1.8's boat (1.8.9
    // BoatEntity.java:314; a cobweb's setInWeb slows its next move)
    move_vehicle(
        &mut body,
        world,
        release,
        velocity,
        MoveRules::BOAT,
        |body, support| apply_effects_inside(body, world, support, release),
    );
    state.stuck_speed_multiplier = body.stuck_speed_multiplier;
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
        f64::from((atan2_vanilla(dz, dx) * 180.0 / std::f64::consts::PI) as f32)
    } else {
        held
    };
    let turn = wrap_degrees(heading - held).clamp(-20.0, 20.0);
    state.yaw_degrees = (held + turn) as f32 % 360.0;
    state.yaw = f64::from(state.yaw_degrees).to_radians();
}

/// The world a boat moves in before 1.13, where a lily pad gives it no collision box (1.8.9
/// LilyPadBlock.java:37-41; 1.9.4 - 1.12.2 :22-26; 1.13 drops the override): the lily pads the
/// move can reach removed. 1.8's boat (`corners_of`, where it is) also clears the snow layers and
/// lily pads at its four corners, in the block of its feet and the one above, before it moves
/// (1.8.9 BoatEntity.java:290-306). None when there is nothing to remove.
fn legacy_boat_world(
    world: &WorldSnapshot,
    release: MinecraftVersion,
    bounds: Aabb,
    movement: Vec3,
    corners_of: Option<Vec3>,
) -> Option<WorldSnapshot> {
    if release.at_least(1, 13, 0) {
        return None;
    }
    let mut cleared = Vec::new();
    if let Some(position) = corners_of {
        for corner in 0..4 {
            let x = (position.x + (f64::from(corner % 2) - 0.5) * 0.8).floor() as i32;
            let z = (position.z + (f64::from(corner / 2) - 0.5) * 0.8).floor() as i32;
            for layer in 0..2 {
                let at = (x, position.y.floor() as i32 + layer, z);
                if matches!(
                    world.effect_at(at),
                    BlockEffect::SnowLayer | BlockEffect::LilyPad
                ) {
                    cleared.push(at);
                }
            }
        }
    }
    let reach = bounds
        .expand_towards(movement)
        .inflate(Vec3::new(1.0, 1.0, 1.0));
    for x in reach.min.x.floor() as i32..=reach.max.x.floor() as i32 {
        for y in reach.min.y.floor() as i32..=reach.max.y.floor() as i32 {
            for z in reach.min.z.floor() as i32..=reach.max.z.floor() as i32 {
                if world.effect_at((x, y, z)) == BlockEffect::LilyPad {
                    cleared.push((x, y, z));
                }
            }
        }
    }
    if cleared.is_empty() {
        return None;
    }
    let mut world = world.clone();
    for at in cleared {
        world.set_block(at, BlockState::default());
    }
    Some(world)
}

/// 1.8's World.isAABBInMaterial for water: a water block in the cells from the box's corner to
/// one past it whose surface (its top, less level / 8 under level 8) reaches the box's bottom.
fn legacy_water_reaches(world: &WorldSnapshot, bounds: Aabb) -> bool {
    let cells = |min: f64, max: f64| min.floor() as i32..(max + 1.0).floor() as i32;
    cells(bounds.min.x, bounds.max.x).any(|x| {
        cells(bounds.min.y, bounds.max.y).any(|y| {
            cells(bounds.min.z, bounds.max.z).any(|z| {
                let block = world.block((x, y, z));
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

/// MathHelper.wrapDegrees of a double: within [-180, 180).
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
}

impl MoveRules {
    const BOAT: Self = Self {
        living: false,
        max_step: 0.0,
        gravity: 0.04,
        air_drag: 1.0,
        bounciness: 0.0,
        stands_on_lava: false,
        checks_inside_at_fall: false,
    };
}

/// What Entity.move left besides the body: the movement asked for and made, the block that
/// supports the entity, and the motion checkFallDamage saw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VehicleMove {
    pub requested: Vec3,
    pub moved: Vec3,
    pub support: Option<BlockPosition>,
    pub motion_at_landing: Vec3,
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
    let mut requested = movement;
    if body.stuck_speed_multiplier.length_squared() > 1.0e-7 {
        requested = requested.multiply(body.stuck_speed_multiplier);
        body.stuck_speed_multiplier = Vec3::ZERO;
        body.velocity = Vec3::ZERO;
    }
    let bounds = body.bounds();
    let modern = release.at_least(1, 14, 0);
    let moved = if modern {
        let moved = collide_vehicle(body, world, release, bounds, requested, rules);
        if moves_position(requested, moved, release) {
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
    body.collided_vertically = y_collision;
    body.vertical_collision_below = y_collision && requested.y < 0.0;
    body.on_ground = body.vertical_collision_below;
    let support = release
        .at_least(1, 20, 0)
        .then(|| main_supporting_block(world, body, Some(moved)))
        .flatten();
    if rules.checks_inside_at_fall {
        // Strider.checkFallDamage's checkInsideBlocks, before the motion stops on the axes it
        // collided on (1.16 StriderEntity.java:295-296, Entity.java:605-620; 1.21.1
        // Strider.java:289)
        let inside: Vec<_> = blocks_inside(body, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_inside_block_effects(body, world, release, &inside);
    }
    let motion_at_landing = body.velocity;
    if release.at_least(26, 2, 0) {
        restitute_vehicle_motion(
            body,
            world,
            support,
            release,
            [x_collision, y_collision, z_collision],
            moved.y,
            rules,
        );
    } else {
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
        if y_collision {
            fall_on(body, world, support, release, rules.living);
        }
    }
    if !release.at_least(1, 21, 2) {
        inside(body, support);
    }
    if release.at_least(1, 15, 0) {
        let factor = f64::from(block_speed_factor(world, body, support, release));
        body.velocity.x *= factor;
        body.velocity.z *= factor;
    }
    VehicleMove {
        requested,
        moved,
        support,
        motion_at_landing,
    }
}

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
) -> Vec3 {
    let max_step = rules.max_step;
    let context = EntityCollisionContext {
        bottom: body.position.y,
        descending: false,
        fall_distance: body.fall_distance,
        walks_on_powder_snow: false,
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
    let (mut obstacles, entered) =
        world.collision_shapes_with_entered_for_entity(query, bounds, &context);
    if rules.stands_on_lava {
        obstacles.extend(lava_surfaces(world, query, body.position.y));
    }
    let border = world.world_border_collision_boxes();
    let collides_with_border = if release.at_least(1, 18, 0) {
        world.is_inside_close_to_world_border(body.position, swept, release)
    } else {
        let inside = bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !border.iter().any(|outside| outside.intersects(inside))
    };
    if collides_with_border {
        obstacles.extend(border);
    }
    let rules = CollisionRules {
        step_up: if release.at_least(1, 21, 0) {
            StepUp::CandidateHeights
        } else {
            StepUp::Lifted
        },
        border_always_collides: collides_with_border
            && release.at_least(1, 13, 0)
            && !release.at_least(1, 18, 0),
    };
    collide(
        bounds,
        movement,
        &obstacles,
        &[],
        &entered,
        body.on_ground,
        max_step,
        rules,
    )
}

/// The top half of each still lava block in the cells of `query` under no lava, for an entity whose
/// feet are above it (LiquidBlock.STABLE_SHAPE, EntityCollisionContext.isAbove).
pub(crate) fn lava_surfaces(world: &WorldSnapshot, query: Aabb, bottom: f64) -> Vec<Aabb> {
    let mut surfaces = Vec::new();
    for x in query.min.x.floor() as i32..query.max.x.ceil() as i32 {
        for y in query.min.y.floor() as i32..query.max.y.ceil() as i32 {
            for z in query.min.z.floor() as i32..query.max.z.ceil() as i32 {
                if is_lava_surface(world, (x, y, z), bottom) {
                    let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    surfaces.push(Aabb::new(min, min + Vec3::new(1.0, 0.5, 1.0)));
                }
            }
        }
    }
    surfaces
}

fn is_lava_surface(world: &WorldSnapshot, (x, y, z): BlockPosition, bottom: f64) -> bool {
    let block = world.block((x, y, z));
    block.fluid == FluidState::Lava
        && block.fluid_source
        && world.fluid_at((x, y + 1, z)) != FluidState::Lava
        && bottom > f64::from(y) + 0.5 - 9.999999747378752e-6
}

/// Entity.move's collision before 1.14: the box moves on y, then x, then z against the boxes the
/// move sweeps; blocked horizontally on the ground (or landing) it also tries the move raised by
/// the step height and keeps the one that goes farther. Returns the moved box and the movement
/// vanilla takes the collision flags from (its y the settling after a step).
fn legacy_collide(
    world: &WorldSnapshot,
    release: MinecraftVersion,
    bounds: Aabb,
    requested: Vec3,
    max_step: f32,
    on_ground: bool,
) -> (Aabb, Vec3) {
    let colliders = legacy_colliders(world, release, bounds, requested);
    let offset = |axis, bounds, colliders: &[Aabb], distance| {
        legacy_offset(axis, bounds, colliders, distance, release)
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
    let step_colliders = legacy_colliders(
        world,
        release,
        bounds,
        Vec3::new(requested.x, step, requested.z),
    );
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

/// The block boxes a move can meet before 1.14 (World.getCollisionBoxes): those meeting the swept
/// box. 1.13 sweeps a box grown by 1.0E-7, leaves out the blocks the box already reaches into and
/// lists the full cubes after the other shapes.
fn legacy_colliders(
    world: &WorldSnapshot,
    release: MinecraftVersion,
    bounds: Aabb,
    movement: Vec3,
) -> Vec<Aabb> {
    let swept = bounds.expand_towards(movement);
    if !release.at_least(1, 13, 0) {
        let mut colliders = world.collision_shapes(swept);
        colliders.retain(|shape| shape.intersects(swept));
        return colliders;
    }
    let query = swept.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
    let inside = bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
    let full_cube = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    let mut partial = Vec::new();
    let mut full = Vec::new();
    for y in query.min.y.floor() as i32 - 1..=query.max.y.floor() as i32 {
        for z in query.min.z.floor() as i32..=query.max.z.floor() as i32 {
            for x in query.min.x.floor() as i32..=query.max.x.floor() as i32 {
                let boxes = world.block_collision_boxes((x, y, z));
                let reached = boxes.iter().any(|shape| shape.intersects(inside));
                if reached || !boxes.iter().any(|shape| shape.intersects(query)) {
                    continue;
                }
                let origin = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                if boxes.len() == 1 && boxes[0] == full_cube.translated(origin) {
                    full.extend(boxes);
                } else {
                    partial.extend(boxes);
                }
            }
        }
    }
    partial.extend(full);
    partial
}

/// How far a box moves along an axis before it meets the colliders: AxisAlignedBB's
/// calculateOffset before 1.13, VoxelShapes' with its 1.0E-7 tolerances on 1.13.
fn legacy_offset(
    axis: Axis,
    bounds: Aabb,
    colliders: &[Aabb],
    mut distance: f64,
    release: MinecraftVersion,
) -> f64 {
    if release.at_least(1, 13, 0) {
        for collider in colliders {
            if distance.abs() < 1.0e-7 {
                return 0.0;
            }
            distance = clip_axis(bounds, *collider, distance, axis);
        }
        return distance;
    }
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
/// (1.9.4 SlimeBlock.java:34-44; 1.8.9 SlimeBlock.java:33-39 has no scale).
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
    match world.effect_at(landing_position(body, support, release)) {
        BlockEffect::Slime if !body.sneaking => {
            if body.velocity.y < 0.0 {
                body.velocity.y = -body.velocity.y * bounce;
            }
        }
        BlockEffect::Bed if release.at_least(1, 12, 0) && !body.sneaking => {
            if body.velocity.y < 0.0 {
                body.velocity.y = -body.velocity.y * 0.6600000262260437 * bounce;
            }
        }
        _ => {
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
        let landed_on = world.block(landing_position(body, support, release));
        restitution = if -current.y < rules.gravity || landed_on.suppresses_bounce {
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
/// is its yRot in radians (1.21.11 AbstractHorse.java:748-775, LivingEntity.java:2254-2264,
/// 2496-2505).
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
        state.velocity.y = fluid_falling_adjusted_y(0.08, going_down, state.velocity.y);
        // floatInWaterWhileRidden: a ridden horse (EntityTypeTags.CAN_FLOAT_WHILE_RIDDEN) in
        // water deeper than its getFluidJumpThreshold (0.4) rises 0.04F (26.2
        // LivingEntity.java:2420-2424, 2490-2496; 1.21.11 :2366-2372, 2432-2437)
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
        state.velocity.y -= 0.08;
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
        tick_boat_with_passenger, tick_horse, BoatSeat, BoatSimulationState, BoatStatus,
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

    #[test]
    fn boat_accelerates_and_turns() {
        // the world-less boat at the default release (26.2): floatBoat IN_WATER keeps 0.9F of the
        // turn and falls by -0.04, controlBoat's right key turns it a degree (deltaRotation++) and
        // its forward key pushes it 0.04F along -sin and cos of yRot (1.21.11
        // AbstractBoat.java:532-573, 575-604)
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
        let world = pond();
        let mut boat = boat_on_pond();

        tick_boat_in_world(&mut boat, &world, V1_21_1);

        assert_eq!(boat.status, BoatStatus::InWater);
        assert_eq!(boat.water_level, 100.88888549804688);
        assert_eq!(boat.velocity, Vec3::new(0.0, -0.000026689068836262958, 0.0));
        assert_eq!(boat.position, Vec3::new(0.5, 100.52334181630349, 2125.5));

        boat.forward = true;
        tick_boat_in_world(&mut boat, &world, V1_21_1);

        assert_eq!(boat.velocity.x, 0.03999999910593033);
        assert_eq!(boat.velocity.y, -0.000026689068836262958);
        assert_eq!(boat.position.x, 0.5399999991059303);
        assert_eq!(boat.position.y, 100.52331512723465);
    }

    #[test]
    fn boat_turns_by_float_degrees_and_keeps_the_turn_damped_by_the_water() {
        let world = pond();
        let mut boat = BoatSimulationState {
            left: true,
            ..boat_on_pond()
        };

        tick_boat_in_world(&mut boat, &world, V1_21_1);
        tick_boat_in_world(&mut boat, &world, V1_21_1);

        assert_eq!(boat.delta_rotation, f64::from(-1.0_f32 * 0.9 - 1.0));
        assert_eq!(boat.yaw_degrees, -90.0 - 1.0 + (-1.0_f32 * 0.9 - 1.0));
        assert!(boat.velocity.x > 0.0 && boat.velocity.z < 0.0);
    }

    #[test]
    fn boat_falls_by_float_gravity_before_1_20_5() {
        let world = WorldSnapshot::new();
        let mut modern = BoatSimulationState {
            position: Vec3::new(0.5, 10.0, 0.5),
            ..BoatSimulationState::default()
        };
        let mut legacy = modern;

        tick_boat_in_world(&mut modern, &world, V1_21_1);
        tick_boat_in_world(&mut legacy, &world, V1_20_4);

        assert_eq!(modern.status, BoatStatus::InAir);
        assert_eq!(modern.velocity.y, -0.04);
        assert_eq!(legacy.velocity.y, -0.03999999910593033);
        assert_eq!(legacy.position.y, 10.0 - 0.03999999910593033);
    }

    #[test]
    fn a_world_less_boat_on_land_keeps_its_grounds_friction_from_tick_to_tick() {
        // getStatus takes landFriction from the ground before every floatBoat, whose halving for a
        // player's boat lasts the tick alone (26.2 AbstractBoat.java:197, 370, 540-542; 1.21.11 :387, 559-561)
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

        tick_boat_in_world(&mut boat, &world, V1_21_1);

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
    fn boat_landing_keeps_a_positive_zero_from_26_2() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, -1, 0), solid(0.6));
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 0.01, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &world, V26_2);

        assert!(boat.on_ground);
        assert_eq!(boat.position.y, 0.0);
        assert!(boat.velocity.y == 0.0 && boat.velocity.y.is_sign_positive());
    }

    #[test]
    fn boat_entering_water_from_the_air_is_set_on_its_surface() {
        let world = pond();
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 100.7, 2125.5),
            velocity: Vec3::new(0.0, -0.3, 0.0),
            status: BoatStatus::InAir,
            last_yd: -0.3,
            ..BoatSimulationState::default()
        };

        tick_boat_in_world(&mut boat, &world, V1_21_1);

        assert_eq!(boat.status, BoatStatus::InWater);
        assert_eq!(boat.position.y, f64::from(101.0_f32 - 0.5625) + 0.101);
        assert!(boat.velocity.y == 0.0 && boat.velocity.y.is_sign_negative());
        assert_eq!(boat.last_yd, 0.0);
    }

    #[test]
    fn settled_boat_does_not_enter_the_water_it_floats_on() {
        let world = pond();
        let mut boat = BoatSimulationState {
            status: BoatStatus::InAir,
            settle_status: true,
            ..boat_on_pond()
        };

        tick_boat_in_world(&mut boat, &world, V1_21_1);

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

        tick_boat_in_world(&mut boat, &world, V1_12_2);

        assert_eq!(boat.water_level, f64::from(1.0_f32 - 2.0 / 9.0));
        assert_eq!(boat.status, BoatStatus::InWater);
        let kept = boat.bounding_box.expect("a boat keeps its box before 1.17");
        assert_eq!(kept.min.y, boat.position.y);
        assert_eq!(kept.max.y - kept.min.y, 0.5625);
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

        tick_boat_in_world(&mut boat, &world, v1_8_8);

        assert_eq!(boat.position, Vec3::new(0.5, 100.90541999894812, 2125.5));
        assert_eq!(boat.velocity, Vec3::new(0.0, 0.006478999511145081, 0.0));

        boat.legacy.rider_yaw = 270.0;
        boat.legacy.rider_forward = 0.98;
        tick_boat_in_world(&mut boat, &world, v1_8_8);

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

        tick_boat_in_world(&mut boat, &world, V1_21_1);

        assert!(boat.collided_horizontally);
        assert_eq!(boat.position.x, 2.0 - 0.6875);
        assert_eq!(boat.velocity.x, 0.0);
    }

    #[test]
    fn horse_jumps_and_moves_forward() {
        // the world-less horse at the default release (26.2): the full riding jump goes up by
        // JUMP_STRENGTH and 0.4F forward along its yaw, then LivingEntity.travel on the ground
        // (gravity 0.08, the 0.98F drag) (1.21.11 AbstractHorse.java:767-775, LivingEntity.java)
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
        // AbstractHorse.java:748-756, LivingEntity.java)
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
        // :2366-2372, 2432-2437)
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
        // 1.8: 0.4 along its yaw by Math.cos and sin, at -0.3 and the player's -0.35 (1.8.9
        // BoatEntity.java:136-138, 370-376)
        let v1_8_8 = MinecraftVersion::new(1, 8, 8);
        let radians = 90.0 * std::f64::consts::PI / 180.0;
        assert_eq!(
            boat_passenger_position(&boat(90.0), seat(v1_8_8), v1_8_8),
            Vec3::new(
                10.0 + crate::trig::cos(radians) * 0.4,
                64.0 + -0.3 + -0.35,
                20.0 + crate::trig::sin(radians) * 0.4
            )
        );
        // 1.9 - 1.20.1: the float sum of the boat's -0.1 and the player's -0.35; with two aboard
        // the first sits 0.2F forward, Vec3(0.2F, 0, 0) turned by -yRot * 0.017453292F -
        // 1.5707964F with Mth's tables (1.12.2 BoatEntity.java:595-613)
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
        // a raft of 1.19.4 seats at its 0.3 (1.19.4 BoatEntity.java:153-155)
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
        // the player's -0.6F (1.20.2 BoatEntity.java:155-171, ChestBoatEntity.java:46-48)
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
        // rides at height * 0.8888889F (1.21.11 AbstractBoat.java:136-152, Raft.java:15-17)
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
        // - wrapped) from 1.17 (1.16.5 BoatEntity.java:681-683, 723-730; 1.17 :677-679, 724-731)
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
        // 1.8 does not turn its rider (1.8.9 BoatEntity.java:370-376)
        assert_eq!(
            boat_passenger_yaw(110.0, &boat, MinecraftVersion::new(1, 8, 8)),
            110.0
        );
    }

    #[test]
    fn vehicle_moves_before_passenger_is_attached() {
        let world = pond();
        let mut vehicle = BoatSimulationState {
            forward: true,
            ..boat_on_pond()
        };
        let mut passenger = EntityState {
            position: Vec3::new(50.0, 50.0, 50.0),
            velocity: Vec3::ZERO,
            dimensions: Dimensions::new(0.3, 1.8),
            flags: EntityFlags::default(),
        };
        let start = vehicle.position;
        let seat = BoatSeat::player(V1_21_1);

        tick_boat_with_passenger(&mut vehicle, &world, V1_21_1, &mut passenger, seat);

        assert_ne!(vehicle.position, start);
        assert_eq!(
            passenger.position,
            boat_passenger_position(&vehicle, seat, V1_21_1)
        );
    }

    #[test]
    fn a_vehicle_bounces_off_slime_at_0_8_only_from_1_9() {
        // SlimeBlock.onLanded: 1.8.9 sets motionY = -motionY for every entity (SlimeBlock.java:33-39);
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
        // Entity.java:830, 839; 1.14 :602, 611), and BoatEntity.canClimb is false (1.12.2
        // BoatEntity.java:120; 1.13.2 :132); from 1.15 the onGround entity steps on it (1.15
        // Entity.java:585), and SlimeBlock.onSteppedOn scales x and z by 0.4 + |y| * 0.2
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
        tick_boat_in_world(&mut boat, &world, release);
        let first = boat;
        tick_boat_in_world(&mut boat, &world, release);
        [first, boat]
    }

    #[test]
    fn a_cobweb_the_boat_is_inside_slows_its_next_move() {
        // Entity.move's checkInsideBlocks and Boat.tick's own (1.21.1 Entity.java:738,
        // Boat.java:308; applyEffectsFromBlocks from 1.21.2, 1.21.11 AbstractBoat.java:236-237)
        // run CobwebBlock.entityInside's makeStuckInBlock, which the next Entity.move multiplies
        // in and clears with the motion (1.21.11 Entity.java:729-735)
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
        // the next moveEntity slows by and stops the motion (1.8.9 BoatEntity.java:314)
        let [first, second] = falling_boat_by(web, MinecraftVersion::new(1, 8, 8));
        assert_eq!(first.stuck_speed_multiplier, stuck);
        assert!(first.position.y - second.position.y < 0.01);
        assert_eq!(second.velocity, Vec3::ZERO);
    }

    #[test]
    fn powder_snow_holds_a_boat_wherever_its_feet_are_and_a_berry_bush_does_not() {
        // PowderSnowBlock.entityInside: !(entity instanceof LivingEntity) || its feet are in it
        // (1.17 PowderSnowBlock.java:50-52, 1.21.11 :62-64); SweetBerryBushBlock.entityInside
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
        tick_boat_in_world(&mut boat, &world, release);
        boat
    }

    #[test]
    fn a_submerged_bubble_column_pushes_a_boat_twice_a_tick() {
        // BubbleColumnBlock.entityInside -> Entity.onInsideBubbleColumn, from Entity.move's
        // checkInsideBlocks and Boat.tick's own (1.16 BoatEntity.java:298, 328; 1.20.5 :297, 327),
        // from 1.21.2 from both of Boat.tick's applyEffectsFromBlocks (1.21.11
        // AbstractBoat.java:236-237, BubbleColumnBlock.java:51-60)
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
    fn a_bubble_column_under_the_air_does_not_move_the_boat_its_player_steers() {
        // AbstractBoat.onAboveBubbleColumn moves the boat on the server alone (1.16
        // BoatEntity.java:205-215, 1.21.11 AbstractBoat.java:155-162, 26.3 :151-158)
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
        // (FAST_LAVA), else 0.0023333333333333335 (1.16 Entity.java:1049, 1.21.11 :1519,
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
    fn a_boat_takes_the_legacy_water_scan_before_1_13() {
        // Entity.handleWaterMovement before 1.13: the box shrunk by 0.4F vertically and 0.001 all
        // round reaches the layer at y 100, and water is touched where the cells top (ceil maxY)
        // reaches its surface (1.12.2 Entity.java:1051, World.java:1635-1674; 1.8.9
        // Entity.java:840, World.java:1545); its flows are normalized and pushed by 0.014. From
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
            // Box.java:43-49; 1.8.9 Box.java:22-28): from y 100.7 the box is 100.8635 - 101.099
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
        // BoatEntity.java:492, 1.21.11 AbstractBoat.java:453; before 1.13 it gives a boat no box,
        // 1.12.2 LilyPadBlock.java:22-26): on it alone the boat is in the air (NaN)
        let release = V1_21_1;
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), lily_pad(release));
        let bounds = super::boat_box_at(Vec3::new(0.5, 100.09375, 0.5));
        assert!(super::ground_friction(bounds, &world).is_nan());
        world.set_block((1, 100, 0), solid(0.98));
        assert_eq!(super::ground_friction(bounds, &world), 0.98);
    }

    #[test]
    fn a_boat_falls_through_a_lily_pad_before_1_13() {
        // BlockLilyPad.addCollisionBoxesToList adds no box for a boat (1.8.9 LilyPadBlock.java:
        // 37-41, 1.9.4 - 1.12.2 :22-26); 1.13 drops the override (1.13.2 LilyPadBlock.java)
        let fall = |release: MinecraftVersion| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 100, 0), lily_pad(release));
            let mut boat = BoatSimulationState {
                position: Vec3::new(0.5, 100.15, 0.5),
                velocity: Vec3::new(0.0, -0.1, 0.0),
                ..BoatSimulationState::default()
            };
            tick_boat_in_world(&mut boat, &world, release);
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
        // at its feet and the block above, before it moves (1.8.9 BoatEntity.java:290-306); a snow
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
            tick_boat_in_world(&mut boat, &world, MinecraftVersion::new(1, 8, 8));
            boat
        };
        let cleared = fall((0, 100, 0));
        assert!(cleared.position.y < 100.25 && !cleared.on_ground);
        let held = fall((1, 100, 0));
        assert_eq!(held.position.y, 100.25);
        assert!(held.on_ground);
    }
}
