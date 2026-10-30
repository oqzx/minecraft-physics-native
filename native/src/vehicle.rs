use crate::collision::{clip_axis, collide, vanilla_equal, Axis, CollisionRules, StepUp};
use crate::entity::EntityState;
use crate::math::{atan2_vanilla, Aabb, Vec3};
use crate::simulation::{
    block_speed_factor, landing_position, main_supporting_block, moves_position, mth_cos, mth_sin,
    position_of_bounding_box, push_by_fluid, SimulationState,
};
use crate::trig;
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, EntityCollisionContext, FluidState, WorldSnapshot};

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
/// the boat is.
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
}

/// Returns the passenger position used after a vehicle has moved for the tick.
///
/// This follows vanilla's `Entity.positionRider`/boat riding-position order:
/// the local passenger offset is rotated by the vehicle yaw, then translated
/// from the vehicle's current position.
#[must_use]
pub fn passenger_position(vehicle_position: Vec3, vehicle_yaw: f64, local_offset: Vec3) -> Vec3 {
    let (sin, cos) = vehicle_yaw.sin_cos();
    vehicle_position.add_components(
        local_offset.x * cos + local_offset.z * sin,
        local_offset.y,
        local_offset.z * cos - local_offset.x * sin,
    )
}

/// Ticks a boat first and attaches its passenger to the boat's new position.
pub fn tick_boat_with_passenger(
    vehicle: &mut BoatSimulationState,
    passenger: &mut EntityState,
    local_offset: Vec3,
) {
    tick_boat(vehicle);
    passenger.position = passenger_position(vehicle.position, vehicle.yaw, local_offset);
}

pub fn tick_boat(state: &mut BoatSimulationState) {
    apply_boat_controls(state);
    let drag = if state.in_water { 0.92 } else { 0.8 };
    state.velocity = state.velocity.scale(drag);
    state.position = state.position + state.velocity;
}

fn apply_boat_controls(state: &mut BoatSimulationState) {
    if state.left {
        state.delta_rotation += 1.0;
    }

    if state.right {
        state.delta_rotation -= 1.0;
    }
    let mut acceleration = 0.0_f64;
    if state.right != state.left && !state.forward && !state.input_down {
        acceleration += 0.005;
    }
    state.yaw += state.delta_rotation.to_radians();
    if state.forward {
        acceleration += 0.04;
    }
    if state.input_down {
        acceleration -= 0.005;
    }
    state.velocity.x += trig::sin(state.yaw) * acceleration;
    state.velocity.z -= trig::cos(state.yaw) * acceleration;
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
    let outcome = move_vehicle(
        &mut body,
        world,
        release,
        velocity,
        MoveRules::BOAT,
        |_, _| {},
    );
    state.last_yd = outcome.motion_at_landing.y;
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
        ..SimulationState::default()
    }
}

fn is_water(fluid: FluidState) -> bool {
    matches!(fluid, FluidState::Water | FluidState::Waterlogged)
}

/// FluidState.getHeight of the water at a position; before 1.13 BlockLiquid's
/// getBlockLiquidHeight: 1 under water only for a level whose low bits are 0.
fn water_height(world: &WorldSnapshot, position: BlockPosition, release: MinecraftVersion) -> f32 {
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
/// the slice under the box (NaN, so no land, when none does).
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
                let meets = world
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
        ..SimulationState::default()
    };
    let water = world.fluid_contact(FluidState::Water, bounds, release);
    state.in_water = water.touching;
    if water.touching {
        push_by_fluid(&mut body, water, 0.014, release);
    }
    if release.at_least(1, 16, 0) {
        let lava = world.fluid_contact(FluidState::Lava, bounds, release);
        push_by_fluid(&mut body, lava, 0.0023333333333333335, release);
    }
    state.velocity = body.velocity;
}

/// Boat.floatBoat: a boat that came out of the air onto water is set on its surface; otherwise
/// gravity, the status's friction and buoyancy toward the water level. Gravity is the float
/// 0.04F before 1.20.5; the boat is set on the water only where it fits from 1.21.
fn float_boat(state: &mut BoatSimulationState, world: &WorldSnapshot, release: MinecraftVersion) {
    let double_gravity = release.at_least(1, 20, 5);
    let mut vertical = if double_gravity {
        -0.04
    } else {
        f64::from(-0.04_f32)
    };
    let mut buoyancy = 0.0;
    let entering = state.previous_status == BoatStatus::InAir
        && !matches!(state.status, BoatStatus::InAir | BoatStatus::OnLand);
    if entering {
        state.water_level = state.position.y + f64::from(BOAT_HEIGHT);
        let bounds = boat_bounds(state);
        let surface = water_level_above(bounds, state.last_yd, world, release);
        let target_y = f64::from(surface - BOAT_HEIGHT) + 0.101;
        let target = bounds.translated(Vec3::new(0.0, target_y - state.position.y, 0.0));
        let fits = !world
            .collision_shapes(target)
            .iter()
            .any(|shape| shape.intersects(target));
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
        ..SimulationState::default()
    };
    let velocity = state.velocity;
    move_vehicle(
        &mut body,
        world,
        release,
        velocity,
        MoveRules::BOAT,
        |_, _| {},
    );
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
}

impl MoveRules {
    const BOAT: Self = Self {
        living: false,
        max_step: 0.0,
        gravity: 0.04,
        air_drag: 1.0,
        bounciness: 0.0,
        stands_on_lava: false,
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
        world.is_inside_close_to_world_border(body.position, swept)
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

pub(crate) fn is_lava_surface(
    world: &WorldSnapshot,
    (x, y, z): BlockPosition,
    bottom: f64,
) -> bool {
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
/// off slime (and a bed from 1.12), fully for a living entity and at 0.8 of it otherwise.
fn fall_on(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
    living: bool,
) {
    let bounce = if living { 1.0 } else { 0.8 };
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

pub fn tick_horse(state: &mut HorseSimulationState) {
    if state.on_ground && state.jump_requested {
        state.velocity.y = if state.jump_strength > 0.0 {
            state.jump_strength
        } else {
            0.42
        };
        state.on_ground = false;
    }

    let speed = if state.movement_speed > 0.0 {
        state.movement_speed
    } else {
        0.1
    };
    let forward = state.forward.clamp(-1.0, 1.0);
    let strafe = state.strafe.clamp(-1.0, 1.0);
    let input_length = (forward * forward + strafe * strafe).sqrt();
    let input_scale = if input_length > 1.0 {
        1.0 / input_length
    } else {
        1.0
    };
    let forward = forward * input_scale;
    let strafe = strafe * input_scale;
    let (sin, cos) = (trig::sin(state.yaw), trig::cos(state.yaw));
    state.velocity.x += (sin * forward + cos * strafe) * speed;
    state.velocity.z += (cos * forward - sin * strafe) * speed;
    if state.in_water {
        state.velocity.y -= 0.01;
        state.velocity.x *= 0.8;
        state.velocity.z *= 0.8;
    } else {
        state.velocity.y -= 0.08;
        state.velocity.x *= 0.91;
        state.velocity.z *= 0.91;
    }
    state.position = state.position + state.velocity;
    state.jump_requested = false;
}

#[cfg(test)]
mod tests {
    use super::{
        passenger_position, tick_boat, tick_boat_in_world, tick_boat_with_passenger, tick_horse,
        BoatSimulationState, BoatStatus, HorseSimulationState,
    };
    use crate::entity::{Dimensions, EntityFlags, EntityState};
    use crate::math::{Aabb, Vec3};
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
        let mut state = BoatSimulationState {
            forward: true,
            right: true,
            in_water: true,
            ..BoatSimulationState::default()
        };
        tick_boat(&mut state);
        assert!(state.position.z < 0.0);
        assert!(state.yaw < 0.0);
    }

    #[test]
    fn boat_control_persists_rotation_and_applies_backward_acceleration() {
        let mut state = BoatSimulationState {
            input_down: true,
            right: true,
            in_water: true,
            ..BoatSimulationState::default()
        };

        tick_boat(&mut state);

        assert_eq!(state.delta_rotation, -1.0);
        assert!((state.yaw + 1.0_f64.to_radians()).abs() < 1.0e-12);
        assert!(state.velocity.z > 0.0);
        assert!(state.velocity.x > 0.0);
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
        let mut state = HorseSimulationState {
            forward: 1.0,
            jump_requested: true,
            on_ground: true,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut state);
        assert!(state.position.z > 0.0);
        assert!(state.position.y > 0.0);
        assert!(!state.on_ground);
    }

    #[test]
    fn horse_strafe_changes_lateral_motion() {
        let mut state = HorseSimulationState {
            strafe: 1.0,
            yaw: 0.25,
            ..HorseSimulationState::default()
        };
        tick_horse(&mut state);
        assert!(
            (state.position.x - 0.08817103037566869).abs() < 1.0e-12,
            "x={:?}",
            state.position.x
        );
        assert!(
            (state.position.z + 0.02251376029266159).abs() < 1.0e-9,
            "z={:?}",
            state.position.z
        );
        assert_eq!(state.yaw, 0.25);
    }

    #[test]
    fn horse_diagonal_input_is_normalized_before_move_relative() {
        let mut state = HorseSimulationState {
            forward: 1.0,
            strafe: 1.0,
            movement_speed: 1.0,
            ..HorseSimulationState::default()
        };

        tick_horse(&mut state);

        assert!((state.position.x - 0.7071067811865475 * 0.91).abs() < 1.0e-12);
        assert!((state.position.z - 0.7071067811865475 * 0.91).abs() < 1.0e-12);
    }

    #[test]
    fn passenger_offset_rotates_with_vehicle_yaw() {
        let position = passenger_position(
            Vec3::new(10.0, 2.0, 10.0),
            std::f64::consts::FRAC_PI_2,
            Vec3::new(0.5, 1.0, 1.0),
        );

        assert!((position.x - 11.0).abs() < 1e-12);
        assert!((position.y - 3.0).abs() < 1e-12);
        assert!((position.z - 9.5).abs() < 1e-12);
    }

    #[test]
    fn vehicle_moves_before_passenger_is_attached() {
        let mut vehicle = BoatSimulationState {
            forward: true,
            in_water: true,
            ..BoatSimulationState::default()
        };
        let mut passenger = EntityState {
            position: Vec3::new(50.0, 50.0, 50.0),
            velocity: Vec3::ZERO,
            dimensions: Dimensions::new(0.3, 1.8),
            flags: EntityFlags::default(),
        };

        tick_boat_with_passenger(&mut vehicle, &mut passenger, Vec3::new(0.0, 1.0, 0.0));

        assert_eq!(
            passenger.position,
            vehicle.position.add_components(0.0, 1.0, 0.0)
        );
        assert!(passenger.position.z < 0.0);
    }
}
