use crate::block_traversal::blocks_visited;
use crate::math::{Aabb, Vec3};
use crate::simulation::{
    apply_block_effects, block_below_affecting_movement, block_position, block_speed_factor,
    blocks_inside, climbable, fluid_falling_adjusted_y, friction_influenced_speed,
    landing_position, main_supporting_block, movements_this_tick, mth_cos, mth_sin, push_by_fluid,
    relative_movement, SimulationState,
};
use crate::vehicle::{is_lava_surface, move_vehicle, MoveRules, VehicleMove};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, FluidState, WorldSnapshot};

type BlockPosition = (i32, i32, i32);

/// The mobs a player rides and steers itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiddenMobKind {
    Pig,
    Strider,
    Camel,
    HappyGhast,
    Nautilus,
    ZombieNautilus,
    Horse,
    Donkey,
    Mule,
    SkeletonHorse,
    ZombieHorse,
    Llama,
    TraderLlama,
}

impl RiddenMobKind {
    /// The kind of a mob by its entity name; a camel husk rides as a camel (CamelHusk extends
    /// Camel).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "pig" => Self::Pig,
            "strider" => Self::Strider,
            "camel" | "camel_husk" => Self::Camel,
            "happy_ghast" => Self::HappyGhast,
            "nautilus" => Self::Nautilus,
            "zombie_nautilus" => Self::ZombieNautilus,
            "horse" => Self::Horse,
            "donkey" => Self::Donkey,
            "mule" => Self::Mule,
            "skeleton_horse" => Self::SkeletonHorse,
            "zombie_horse" => Self::ZombieHorse,
            "llama" => Self::Llama,
            "trader_llama" => Self::TraderLlama,
            _ => return None,
        })
    }

    /// EntityType's width and height.
    #[must_use]
    pub fn dimensions(self) -> (f32, f32) {
        match self {
            Self::Pig => (0.9, 0.9),
            Self::Strider => (0.9, 1.7),
            Self::Camel => (1.7, 2.375),
            Self::HappyGhast => (4.0, 4.0),
            Self::Nautilus | Self::ZombieNautilus => (0.875, 0.95),
            Self::Horse | Self::Mule | Self::SkeletonHorse | Self::ZombieHorse => (1.3964844, 1.6),
            Self::Donkey => (1.3964844, 1.5),
            Self::Llama | Self::TraderLlama => (0.9, 1.87),
        }
    }

    /// The mob's MOVEMENT_SPEED attribute base (createAttributes).
    #[must_use]
    pub fn movement_speed(self) -> f64 {
        match self {
            Self::Pig => 0.25,
            Self::Strider | Self::Donkey | Self::Mule | Self::Llama | Self::TraderLlama => {
                0.17499999701976776
            }
            Self::Camel => 0.09000000357627869,
            Self::HappyGhast => 0.05,
            Self::Nautilus => 1.0,
            Self::ZombieNautilus => 1.100000023841858,
            Self::Horse => 0.22499999403953552,
            Self::SkeletonHorse | Self::ZombieHorse => 0.20000000298023224,
        }
    }

    fn is_horse(self) -> bool {
        matches!(
            self,
            Self::Horse
                | Self::Donkey
                | Self::Mule
                | Self::SkeletonHorse
                | Self::ZombieHorse
                | Self::Llama
                | Self::TraderLlama
        )
    }

    fn is_nautilus(self) -> bool {
        matches!(self, Self::Nautilus | Self::ZombieNautilus)
    }

    /// AbstractHorse.canPerformRearing: a llama never rears.
    fn rears(self) -> bool {
        !matches!(self, Self::Llama | Self::TraderLlama)
    }
}

/// What the rider gives its mount: its look this tick (float degrees), its xxa and zza and
/// sprinting after its aiStep of the tick before, its jumping (LivingEntity.jumping), and the
/// charge of the riding jump it releases in its own tick, after the mob's (in percent).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RiderInput {
    pub yaw: f32,
    pub pitch: f32,
    pub strafe: f32,
    pub forward: f32,
    pub sprinting: bool,
    pub jumping: bool,
    pub released_jump: Option<i32>,
}

/// A mob its player rides and steers, as the client keeps it between ticks: its body, float
/// rotation and attributes, and the state its ridden travel reads (the pig's and strider's boost,
/// the strider's suffocating, the horse's pending jump and rearing, the camel's and nautilus's
/// dash). `saddled` false is a mob its rider controls without steering it (a horse without its
/// saddle before 1.19, a pig without its stick before 1.11): it goes on its own input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RiddenMobState {
    pub kind: RiddenMobKind,
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub in_water: bool,
    pub in_lava: bool,
    pub fall_distance: f64,
    pub bounding_box: Option<Aabb>,
    pub previous_position: Option<Vec3>,
    pub movement_speed: f64,
    pub jump_strength: f64,
    pub flying_speed: f64,
    pub saddled: bool,
    pub no_ai: bool,
    pub ridden_speed: f32,
    pub own_forward: f32,
    pub boosting: bool,
    pub boost_time: i32,
    pub boost_time_total: i32,
    pub suffocating: bool,
    pub jump_pending_scale: f32,
    pub horse_jumping: bool,
    pub standing: bool,
    pub allow_stand_sliding: bool,
    pub stand_counter: i32,
    pub dash_cooldown: i32,
    pub dashing: bool,
    pub sitting: bool,
    pub pose_time: i64,
    pub rider: RiderInput,
    /// Where this tick's step toward a server teleport's target puts the mob (LivingEntity.aiStep's
    /// interpolation, in place of the fade of its motion); its turn is the caller's.
    pub lerp_position: Option<Vec3>,
}

impl RiddenMobState {
    #[must_use]
    pub fn new(kind: RiddenMobKind, position: Vec3) -> Self {
        Self {
            kind,
            position,
            velocity: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            collided_horizontally: false,
            collided_vertically: false,
            in_water: false,
            in_lava: false,
            fall_distance: 0.0,
            bounding_box: None,
            previous_position: None,
            movement_speed: kind.movement_speed(),
            jump_strength: 0.41999998688697815,
            flying_speed: 0.05,
            saddled: true,
            no_ai: false,
            ridden_speed: 0.0,
            own_forward: 0.0,
            boosting: false,
            boost_time: 0,
            boost_time_total: 0,
            suffocating: false,
            jump_pending_scale: 0.0,
            horse_jumping: false,
            standing: false,
            allow_stand_sliding: false,
            stand_counter: 0,
            dash_cooldown: 0,
            dashing: false,
            sitting: false,
            pose_time: 1000,
            rider: RiderInput::default(),
            lerp_position: None,
        }
    }

    fn box_at(&self, position: Vec3) -> Aabb {
        let (width, height) = self.kind.dimensions();
        let half = f64::from(width / 2.0);
        Aabb::new(
            Vec3::new(position.x - half, position.y, position.z - half),
            Vec3::new(
                position.x + half,
                position.y + f64::from(height),
                position.z + half,
            ),
        )
    }
}

/// One client tick of a mob its player rides and steers, which ticks before its rider: Entity's
/// fluid push, LivingEntity.aiStep (a step toward a server teleport's target, else the motion it
/// does not run the AI of fades before 1.21.5; a negligible one is dropped), travelRidden
/// (tickRidden with its rider, then travel at the ridden speed with the ridden input), the mob's
/// own tick after its move, and the riding jump its rider released.
pub fn tick_ridden_mob(mob: &mut RiddenMobState, world: &WorldSnapshot, release: MinecraftVersion) {
    let (width, height) = mob.kind.dimensions();
    let kept_box = (!release.at_least(1, 17, 0))
        .then(|| mob.bounding_box.unwrap_or_else(|| mob.box_at(mob.position)));
    let mut body = SimulationState {
        position: mob.position,
        velocity: mob.velocity,
        half_width: f64::from(width / 2.0),
        height: f64::from(height),
        on_ground: mob.on_ground,
        collided_horizontally: mob.collided_horizontally,
        collided_vertically: mob.collided_vertically,
        in_water: mob.in_water,
        in_lava: mob.in_lava,
        fall_distance: mob.fall_distance,
        controlled_by_local_instance: true,
        bounding_box: kept_box,
        previous_position: mob.previous_position,
        ..SimulationState::default()
    };
    let old_position = body.position;
    let previous_movement = mob
        .previous_position
        .map(|previous| body.position - previous);
    let support = release
        .at_least(1, 20, 0)
        .then(|| main_supporting_block(world, &body, previous_movement))
        .flatten();
    if mob.kind == RiddenMobKind::Strider && (!mob.no_ai || !release.at_least(1, 19, 0)) {
        mob.suffocating = !strider_is_warm(&body, world, support, release);
    }
    let lava_height = push_by_fluids(mob, &mut body, world, release);
    if let Some(position) = mob.lerp_position.take() {
        body.position = position;
        if body.bounding_box.is_some() {
            body.bounding_box = Some(mob.box_at(position));
        }
    } else if !release.at_least(1, 21, 5) {
        body.velocity = body.velocity.scale(0.98);
    }
    let negligible = if release.at_least(1, 9, 0) {
        0.003
    } else {
        0.005
    };
    for axis in [
        &mut body.velocity.x,
        &mut body.velocity.y,
        &mut body.velocity.z,
    ] {
        if axis.abs() < negligible {
            *axis = 0.0;
        }
    }
    let speed_before = mob.ridden_speed;
    let input = tick_ridden(mob, &mut body, world, support, release);
    let mut moved = None;
    if mob.no_ai && release.at_least(1, 21, 5) && !release.at_least(1, 21, 6) {
        body.velocity = Vec3::ZERO;
    } else {
        let unsaddled = (mob.kind.is_horse() || mob.kind == RiddenMobKind::Pig) && !mob.saddled;
        let air_speed = if unsaddled {
            0.02
        } else if release.at_least(1, 19, 4) {
            mob.ridden_speed * 0.1
        } else {
            legacy_flying_base(mob, speed_before) * 0.1
        };
        let around = Surroundings {
            world,
            release,
            support,
            lava_height,
        };
        moved = travel(mob, &mut body, around, input, air_speed);
    }
    if release.at_least(1, 21, 2) {
        let (requested, displacement) = moved.map_or((Vec3::ZERO, Vec3::ZERO), |moved| {
            (moved.requested, moved.moved)
        });
        let movements = movements_this_tick(&body, old_position, requested, displacement, release);
        let inside = blocks_visited(
            &movements,
            body.bounds(),
            body.half_width,
            body.height,
            release,
        );
        let support = moved.and_then(|moved| moved.support);
        apply_block_effects(&mut body, world, support, release, &inside);
    }
    after_ridden_travel(mob, &mut body, world, release);
    mob.position = body.position;
    mob.velocity = body.velocity;
    mob.on_ground = body.on_ground;
    mob.collided_horizontally = body.collided_horizontally;
    mob.collided_vertically = body.collided_vertically;
    mob.in_water = body.in_water;
    mob.in_lava = body.in_lava;
    mob.fall_distance = body.fall_distance;
    mob.bounding_box = body.bounding_box;
    mob.previous_position = Some(old_position);
    if let Some(power) = mob.rider.released_jump.take() {
        rider_jump(mob, power, release);
    }
}

/// Entity.baseTick's fluids: the water's contact and push, and the lava's (pushed from 1.16; a
/// nautilus is not pushed). Returns the lava's height in the box.
fn push_by_fluids(
    mob: &RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> f64 {
    let pushed = !mob.kind.is_nautilus();
    let bounds = body.bounds();
    let water = world.fluid_contact(FluidState::Water, bounds, release);
    body.in_water = water.touching;
    if water.touching {
        body.fall_distance = 0.0;
        if pushed {
            push_by_fluid(body, water, 0.014, release);
        }
    }
    let lava = world.fluid_contact(FluidState::Lava, bounds, release);
    if release.at_least(1, 16, 0) {
        if pushed {
            push_by_fluid(body, lava, 0.0023333333333333335, release);
        }
        body.in_lava = lava.height > 0.0;
    } else if !release.at_least(1, 14, 0) {
        body.in_lava = world.has_fluid_block_inside(FluidState::Lava, bounds);
    }
    lava.height
}

/// Strider.tick: warm on the blocks at its feet or under it that are lava, or touching lava in
/// the tick before.
fn strider_is_warm(
    body: &SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> bool {
    world.fluid_at(block_position(body.position)) == FluidState::Lava
        || world.fluid_at(landing_position(body, support, release)) == FluidState::Lava
        || body.in_lava
}

/// The speed the air speed comes from before 1.19.4 (ItemSteerable.travel, AbstractHorse.travel:
/// flyingSpeed = getSpeed() * 0.1F before the ridden speed is set): the speed of the tick before,
/// but a strider's Strider.travel sets it to its move speed first.
fn legacy_flying_base(mob: &RiddenMobState, speed_before: f32) -> f32 {
    if mob.kind != RiddenMobKind::Strider {
        return speed_before;
    }
    mob.movement_speed as f32 * if mob.suffocating { 0.66 } else { 1.0 }
}

/// Mth.wrapDegrees of a float: within [-180, 180).
fn wrap_degrees(degrees: f32) -> f32 {
    let mut wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

fn radians(degrees: f32) -> f64 {
    f64::from(degrees * 0.017453292)
}

/// The mob's part of travelRidden (tickRidden, getRiddenInput, getRiddenSpeed; ItemSteerable
/// and AbstractHorse.travel before 1.19.4): it turns to its rider, takes the rider's jump or
/// dash and its speed, and returns the input it travels with.
fn tick_ridden(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> Vec3 {
    let rider = mob.rider;
    match mob.kind {
        RiddenMobKind::Pig | RiddenMobKind::Strider if !mob.saddled => {
            mob.own_forward *= 0.98;
            Vec3::new(0.0, 0.0, f64::from(mob.own_forward))
        }
        RiddenMobKind::Pig | RiddenMobKind::Strider => {
            face_rider(mob);
            if mob.boosting {
                mob.boost_time += 1;
                if mob.boost_time - 1 > mob.boost_time_total {
                    mob.boosting = false;
                }
            }
            mob.ridden_speed = steering_speed(mob, release);
            mob.own_forward = mob.ridden_speed;
            Vec3::new(0.0, 0.0, 1.0)
        }
        RiddenMobKind::Camel => {
            let refuses = camel_refuses_to_move(mob);
            if !refuses {
                face_rider(mob);
            }
            if body.on_ground {
                if mob.jump_pending_scale > 0.0 {
                    camel_dash(mob, body, world, support, release);
                }
                mob.jump_pending_scale = 0.0;
            }
            if rider.forward > 0.0 && mob.sitting && !camel_in_pose_transition(mob) {
                mob.sitting = false;
                mob.pose_time = 0;
            }
            let bonus = if rider.sprinting && mob.dash_cooldown == 0 {
                0.1
            } else {
                0.0
            };
            mob.ridden_speed = mob.movement_speed as f32 + bonus;
            if refuses {
                Vec3::ZERO
            } else {
                horse_input(rider)
            }
        }
        RiddenMobKind::HappyGhast => {
            let (strafe, mut up, forward) = look_input(rider, release);
            if rider.jumping {
                up += 0.5;
            }
            let scale = 3.9000000953674316 * mob.flying_speed;
            turn_toward_rider(mob, 0.08);
            mob.ridden_speed = mob.flying_speed as f32 * 5.0 / 3.0;
            Vec3::new(f64::from(strafe), f64::from(up), f64::from(forward)).scale(scale)
        }
        RiddenMobKind::Nautilus | RiddenMobKind::ZombieNautilus => {
            let (strafe, up, forward) = look_input(rider, release);
            turn_toward_rider(mob, 0.5);
            if mob.jump_pending_scale > 0.0 {
                nautilus_dash(mob, body, world, support, release);
            }
            mob.jump_pending_scale = 0.0;
            let share = if body.in_water { 0.0325 } else { 0.02 };
            mob.ridden_speed = share * mob.movement_speed as f32;
            Vec3::new(f64::from(strafe), f64::from(up), f64::from(forward))
        }
        _ => tick_ridden_horse(mob, body, world, support, release),
    }
}

/// Entity.setRot of the rider's yaw and half its pitch, each kept within 360.
fn face_rider(mob: &mut RiddenMobState) {
    mob.yaw = mob.rider.yaw % 360.0;
    mob.pitch = (mob.rider.pitch * 0.5) % 360.0;
}

/// A happy ghast's or nautilus's turn: a share of the way to its rider's yaw, half its pitch.
fn turn_toward_rider(mob: &mut RiddenMobState, share: f32) {
    let yaw = mob.yaw + wrap_degrees(mob.rider.yaw - mob.yaw) * share;
    mob.yaw = yaw % 360.0;
    mob.pitch = (mob.rider.pitch * 0.5) % 360.0;
}

/// The rider's input along its look (HappyGhast, AbstractNautilus.getRiddenInput): its strafe as
/// it is, and with a forward or back input, forward by the cosine of its pitch and up by minus
/// its sine, half of it and reversed backwards.
fn look_input(rider: RiderInput, release: MinecraftVersion) -> (f32, f32, f32) {
    if rider.forward == 0.0 {
        return (rider.strafe, 0.0, 0.0);
    }
    let pitch = radians(rider.pitch);
    let mut forward = mth_cos(pitch, release) as f32;
    let mut up = -(mth_sin(pitch, release) as f32);
    if rider.forward < 0.0 {
        forward *= -0.5;
        up *= -0.5;
    }
    (rider.strafe, up, forward)
}

/// AbstractHorse.getRiddenInput: half the rider's strafe, its forward input, a quarter of it
/// backwards.
fn horse_input(rider: RiderInput) -> Vec3 {
    let strafe = rider.strafe * 0.5;
    let mut forward = rider.forward;
    if forward <= 0.0 {
        forward *= 0.25;
    }
    Vec3::new(f64::from(strafe), 0.0, f64::from(forward))
}

/// The pig's and strider's ridden speed: MOVEMENT_SPEED * 0.225 (the strider's 0.55, while
/// suffocating 0.35 of its speed less the suffocating 34%) times the boost factor; before 1.19.4
/// ItemSteerable.travel's float steering speed, 0.23 suffocating, plus its boost.
fn steering_speed(mob: &RiddenMobState, release: MinecraftVersion) -> f32 {
    let strider = mob.kind == RiddenMobKind::Strider;
    let boost_sin = || {
        let share = mob.boost_time as f32 / mob.boost_time_total as f32 * 3.1415927;
        mth_sin(f64::from(share), release) as f32
    };
    if !release.at_least(1, 19, 4) {
        let share = match (strider, mob.suffocating) {
            (false, _) => 0.225,
            (true, false) => 0.55,
            (true, true) => 0.23,
        };
        let speed = mob.movement_speed as f32 * share;
        return if mob.boosting {
            speed + speed * 1.15 * boost_sin()
        } else {
            speed
        };
    }
    let factor = if mob.boosting {
        1.0 + 1.15 * boost_sin()
    } else {
        1.0
    };
    if !strider {
        return (mob.movement_speed * 0.225 * f64::from(factor)) as f32;
    }
    let (speed, share) = if mob.suffocating {
        (
            mob.movement_speed + mob.movement_speed * -0.3400000035762787,
            0.35_f32,
        )
    } else {
        (mob.movement_speed, 0.55)
    };
    (speed * f64::from(share) * f64::from(factor)) as f32
}

/// A horse's part of travelRidden (AbstractHorse.travel before 1.19.4): its rider's input, none
/// while it rears on the ground unless the rider's jump lets it; the rider's rotation; the jump
/// the rider charged, on the ground; MOVEMENT_SPEED. A horse its rider does not steer goes on its
/// own input.
fn tick_ridden_horse(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> Vec3 {
    if !mob.saddled {
        if mob.standing {
            mob.own_forward = 0.0;
        }
        mob.own_forward *= 0.98;
        return Vec3::new(0.0, 0.0, f64::from(mob.own_forward));
    }
    let input = horse_input(mob.rider);
    let rearing =
        body.on_ground && mob.jump_pending_scale == 0.0 && mob.standing && !mob.allow_stand_sliding;
    face_rider(mob);
    if !release.at_least(1, 19, 4) {
        if mob.jump_pending_scale > 0.0 && !mob.horse_jumping && body.on_ground {
            horse_jump(mob, body, world, support, release, input.z);
            mob.jump_pending_scale = 0.0;
        }
    } else if body.on_ground {
        mob.horse_jumping = false;
        if mob.jump_pending_scale > 0.0 {
            horse_jump(mob, body, world, support, release, input.z);
        }
        mob.jump_pending_scale = 0.0;
    }
    mob.ridden_speed = mob.movement_speed as f32;
    mob.own_forward = mob.ridden_speed;
    if rearing {
        Vec3::ZERO
    } else {
        input
    }
}

/// Block.getJumpFactor of the block at the feet, else of the one under them (honey halves it).
fn block_jump_factor(
    body: &SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> f32 {
    let factor = |position| {
        if world.effect_at(position) == BlockEffect::Honey {
            0.5
        } else {
            1.0
        }
    };
    let at_feet = factor(block_position(body.position));
    if at_feet == 1.0 {
        factor(block_below_affecting_movement(body, support, release))
    } else {
        at_feet
    }
}

/// AbstractHorse.executeRidersJump: up by JUMP_STRENGTH times the charge (and the block's jump
/// factor from 1.15, in float from 1.20.5), and moving forward 0.4F of the charge along its yaw.
fn horse_jump(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
    forward: f64,
) {
    let scale = mob.jump_pending_scale;
    body.velocity.y = if !release.at_least(1, 15, 0) {
        mob.jump_strength * f64::from(scale)
    } else if !release.at_least(1, 20, 5) {
        let factor = block_jump_factor(body, world, support, release);
        mob.jump_strength * f64::from(scale) * f64::from(factor)
    } else {
        let factor = block_jump_factor(body, world, support, release);
        f64::from(mob.jump_strength as f32 * scale * factor)
    };
    mob.horse_jumping = true;
    if forward > 0.0 {
        let yaw = radians(mob.yaw);
        let sin = mth_sin(yaw, release) as f32;
        let cos = mth_cos(yaw, release) as f32;
        body.velocity.x += f64::from(-0.4 * sin * scale);
        body.velocity.z += f64::from(0.4 * cos * scale);
    }
}

/// Camel.refuseToMove: sitting, or sitting down or standing up.
fn camel_refuses_to_move(mob: &RiddenMobState) -> bool {
    mob.sitting || camel_in_pose_transition(mob)
}

fn camel_in_pose_transition(mob: &RiddenMobState) -> bool {
    mob.pose_time < if mob.sitting { 40 } else { 52 }
}

/// Camel.executeRidersJump: a dash along its look, 22.2222F * the charge * MOVEMENT_SPEED far and
/// 1.4285F * the charge * its jump power high (in double before 1.20.5, LivingEntity.getJumpPower
/// in float since).
fn camel_dash(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) {
    let scale = mob.jump_pending_scale;
    let jump_factor = block_jump_factor(body, world, support, release);
    let jump_power = if release.at_least(1, 20, 5) {
        f64::from(mob.jump_strength as f32 * jump_factor)
    } else {
        mob.jump_strength * f64::from(jump_factor)
    };
    let look = view_vector(mob.pitch, mob.yaw, release);
    let length = (look.x * look.x + look.z * look.z).sqrt();
    let speed_factor = f64::from(block_speed_factor(world, body, support, release));
    let distance = f64::from(22.2222_f32 * scale) * mob.movement_speed * speed_factor;
    let along = if length < 1.0e-5 {
        Vec3::ZERO
    } else {
        Vec3::new(look.x / length, 0.0, look.z / length).scale(distance)
    };
    let dash = along + Vec3::new(0.0, f64::from(1.4285_f32 * scale) * jump_power, 0.0);
    body.velocity = body.velocity + dash;
    mob.dash_cooldown = 55;
    mob.dashing = true;
}

/// AbstractNautilus.executeRidersJump: a push along its rider's look, (1.2F in water, 0.5F out of
/// it) * the charge * MOVEMENT_SPEED * the block's speed factor.
fn nautilus_dash(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) {
    let momentum = if body.in_water { 1.2_f32 } else { 0.5 } * mob.jump_pending_scale;
    let speed_factor = f64::from(block_speed_factor(world, body, support, release));
    let look = view_vector(mob.rider.pitch, mob.rider.yaw, release);
    body.velocity =
        body.velocity + look.scale(f64::from(momentum) * mob.movement_speed * speed_factor);
    mob.dash_cooldown = 40;
    mob.dashing = true;
}

/// Entity.calculateViewVector of a float pitch and yaw.
fn view_vector(pitch: f32, yaw: f32, release: MinecraftVersion) -> Vec3 {
    let x_rot = pitch * 0.017453292;
    let y_rot = -yaw * 0.017453292;
    let y_cos = mth_cos(f64::from(y_rot), release) as f32;
    let y_sin = mth_sin(f64::from(y_rot), release) as f32;
    let x_cos = mth_cos(f64::from(x_rot), release) as f32;
    let x_sin = mth_sin(f64::from(x_rot), release) as f32;
    Vec3::new(
        f64::from(y_sin * x_cos),
        f64::from(-x_sin),
        f64::from(y_cos * x_cos),
    )
}

/// LivingEntity.maxUpStep with a player in control: at least 1 (a camel's 1.5); a pig its rider
/// does not steer steps 0.5.
fn step_height(mob: &RiddenMobState) -> f32 {
    match mob.kind {
        RiddenMobKind::Pig if !mob.saddled => 0.5,
        RiddenMobKind::Camel => 1.5,
        _ => 1.0,
    }
}

/// Where a ridden mob travels: the world, the release, the block that supported it as the tick
/// began and the height of the lava in its box (Entity.getFluidHeight, from Entity.baseTick).
#[derive(Clone, Copy)]
struct Surroundings<'a> {
    world: &'a WorldSnapshot,
    release: MinecraftVersion,
    support: Option<BlockPosition>,
    lava_height: f64,
}

/// LivingEntity.travel with the ridden input: a happy ghast flies, a nautilus swims at its
/// speed, any other mob travels in the water or lava it is in (not the lava a strider stands
/// on), else on the ground at its ridden speed and in the air at `air_speed`.
fn travel(
    mob: &RiddenMobState,
    body: &mut SimulationState,
    around: Surroundings,
    input: Vec3,
    air_speed: f32,
) -> Option<VehicleMove> {
    let Surroundings {
        world,
        release,
        support,
        ..
    } = around;
    let rules = MoveRules {
        living: true,
        max_step: step_height(mob),
        gravity: 0.08,
        air_drag: f64::from(0.98_f32),
        bounciness: 0.0,
        stands_on_lava: mob.kind == RiddenMobKind::Strider,
    };
    let yaw = radians(mob.yaw);
    let speed = f64::from(mob.ridden_speed);
    if mob.kind == RiddenMobKind::HappyGhast {
        body.velocity = body.velocity + relative_movement(input, speed, yaw, release);
        let moved = move_mob(mob, body, world, release, rules);
        let drag = if body.in_water {
            0.800000011920929
        } else if body.in_lava {
            0.5
        } else {
            0.9100000262260437
        };
        body.velocity = body.velocity.scale(drag);
        return Some(moved);
    }
    if mob.kind.is_nautilus() && body.in_water {
        body.velocity = body.velocity + relative_movement(input, speed, yaw, release);
        let moved = move_mob(mob, body, world, release, rules);
        body.velocity = body.velocity.scale(0.9);
        return Some(moved);
    }
    let stands_on_fluid = mob.kind == RiddenMobKind::Strider
        && world.fluid_at(block_position(body.position)) == FluidState::Lava;
    if (body.in_water || body.in_lava) && !stands_on_fluid {
        return Some(travel_in_fluid(mob, body, around, input, yaw, rules));
    }
    let mut input = input;
    if mob.kind == RiddenMobKind::Camel && camel_refuses_to_move(mob) && body.on_ground {
        body.velocity = body.velocity.multiply(Vec3::new(0.0, 1.0, 0.0));
        input = input.multiply(Vec3::new(0.0, 1.0, 0.0));
    }
    let friction = world.slipperiness_at(block_below_affecting_movement(body, support, release));
    let inertia = if body.on_ground {
        f64::from(friction as f32 * 0.91)
    } else {
        f64::from(0.91_f32)
    };
    let input_speed = if body.on_ground {
        friction_influenced_speed(speed, friction, release)
    } else {
        f64::from(air_speed)
    };
    body.velocity = body.velocity + relative_movement(input, input_speed, yaw, release);
    if climbable(world, body) {
        let limit = f64::from(0.15_f32);
        body.fall_distance = 0.0;
        body.velocity.x = body.velocity.x.clamp(-limit, limit);
        body.velocity.z = body.velocity.z.clamp(-limit, limit);
        let fall_limit = if release.at_least(1, 14, 0) {
            limit
        } else {
            0.15
        };
        body.velocity.y = body.velocity.y.max(-fall_limit);
    }
    let moved = move_mob(mob, body, world, release, rules);
    if body.collided_horizontally && climbable(world, body) {
        body.velocity.y = 0.2;
    }
    body.velocity.y -= 0.08;
    body.velocity.x *= inertia;
    body.velocity.z *= inertia;
    body.velocity.y *= f64::from(0.98_f32);
    Some(moved)
}

/// LivingEntity.travel in water (a mob's water slow-down, 0.96F a skeleton horse's) or lava,
/// with the jump out of it against a wall.
fn travel_in_fluid(
    mob: &RiddenMobState,
    body: &mut SimulationState,
    around: Surroundings,
    input: Vec3,
    yaw: f64,
    rules: MoveRules,
) -> VehicleMove {
    let Surroundings {
        world,
        release,
        lava_height,
        ..
    } = around;
    let old_y = body.position.y;
    let going_down = body.velocity.y <= 0.0;
    let in_water = body.in_water;
    body.velocity = body.velocity + relative_movement(input, f64::from(0.02_f32), yaw, release);
    let moved = move_mob(mob, body, world, release, rules);
    if in_water {
        if body.collided_horizontally && climbable(world, body) {
            body.velocity.y = 0.2;
        }
        let slow_down = f64::from(if mob.kind == RiddenMobKind::SkeletonHorse {
            0.96_f32
        } else {
            0.8
        });
        body.velocity = body
            .velocity
            .multiply(Vec3::new(slow_down, 0.800000011920929, slow_down));
        body.velocity.y = if release.at_least(1, 13, 0) {
            fluid_falling_adjusted_y(0.08, going_down, body.velocity.y)
        } else {
            body.velocity.y - 0.02
        };
    } else {
        if release.at_least(1, 16, 0) && lava_height <= 0.4 {
            body.velocity = body
                .velocity
                .multiply(Vec3::new(0.5, 0.800000011920929, 0.5));
            body.velocity.y = fluid_falling_adjusted_y(0.08, going_down, body.velocity.y);
        } else {
            body.velocity = body.velocity.scale(0.5);
        }
        body.velocity.y -= 0.08 / 4.0;
    }
    let lifted = body.bounds().translated(Vec3::new(
        body.velocity.x,
        body.velocity.y + 0.6000000238418579 - body.position.y + old_y,
        body.velocity.z,
    ));
    if body.collided_horizontally && world.is_free(lifted) {
        body.velocity.y = 0.30000001192092896;
    }
    moved
}

/// Entity.move for the mob, its blocks inside applied in the move before 1.21.2, and
/// Entity.checkFallDamage's look at the water it did not touch (and the lava from 26.1); a
/// strider in lava (Strider.checkFallDamage) and a happy ghast skip it.
fn move_mob(
    mob: &RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    rules: MoveRules,
) -> VehicleMove {
    let velocity = body.velocity;
    let moved = move_vehicle(body, world, release, velocity, rules, |body, support| {
        let inside: Vec<_> = blocks_inside(body, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_block_effects(body, world, support, release, &inside);
    });
    let skips = mob.kind == RiddenMobKind::HappyGhast
        || (mob.kind == RiddenMobKind::Strider && body.in_lava);
    if !skips && !body.in_water {
        let pushed = !mob.kind.is_nautilus();
        let water = world.fluid_contact(FluidState::Water, body.bounds(), release);
        if water.touching {
            body.in_water = true;
            body.fall_distance = 0.0;
            if pushed {
                push_by_fluid(body, water, 0.014, release);
            }
        }
        if release.at_least(26, 1, 0) {
            let lava = world.fluid_contact(FluidState::Lava, body.bounds(), release);
            body.in_lava = lava.touching;
            if pushed && lava.touching {
                push_by_fluid(body, lava, 0.0023333333333333335, release);
            }
        }
    }
    if release.at_least(1, 14, 0) && !release.at_least(1, 16, 0) {
        body.in_lava = world.has_fluid_block_inside(FluidState::Lava, body.bounds());
    }
    moved
}

/// The mob's own tick after its travel: a horse that landed drops its pending jump before 1.19.4,
/// its rearing ends after 20 ticks where the client runs it (counted up until 1.21.6, down
/// since), and a horse that does not rear no longer lets the rider's jump move it; a nautilus's
/// and camel's dash ends and its cooldown runs down; a strider in lava stands on its still
/// surface or floats up (Strider.floatStrider).
fn after_ridden_travel(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    if mob.kind.is_horse() {
        if !release.at_least(1, 19, 4) && body.on_ground {
            mob.jump_pending_scale = 0.0;
            mob.horse_jumping = false;
        }
        if release.at_least(1, 21, 6) {
            if mob.stand_counter > 0 {
                mob.stand_counter -= 1;
                if mob.stand_counter <= 0 {
                    mob.stand_counter = 0;
                    mob.standing = false;
                }
            }
        } else if horse_stands_locally(release) && mob.stand_counter > 0 {
            mob.stand_counter += 1;
            if mob.stand_counter > 20 {
                mob.stand_counter = 0;
                mob.standing = false;
            }
        }
        if !mob.standing {
            mob.allow_stand_sliding = false;
        }
    }
    if mob.kind.is_nautilus() {
        if mob.dashing && mob.dash_cooldown < 35 {
            mob.dashing = false;
        }
        if mob.dash_cooldown > 0 {
            mob.dash_cooldown -= 1;
        }
    }
    if mob.kind == RiddenMobKind::Camel {
        let landed = body.on_ground || body.in_water || body.in_lava;
        if mob.dashing && mob.dash_cooldown < 50 && landed {
            mob.dashing = false;
        }
        if mob.dash_cooldown > 0 {
            mob.dash_cooldown -= 1;
        }
        mob.pose_time += 1;
    }
    if mob.kind == RiddenMobKind::Strider && body.in_lava {
        let feet = block_position(body.position);
        if is_lava_surface(world, feet, body.position.y) {
            body.on_ground = true;
        } else {
            body.velocity = body.velocity.scale(0.5) + Vec3::new(0.0, 0.05, 0.0);
        }
    }
}

/// Whether the client runs the ridden horse's rearing (AbstractHorse.standIfPossible and its
/// counter): before 1.19.4 and from 1.21.5, where the client runs the AI of the mob it moves.
fn horse_stands_locally(release: MinecraftVersion) -> bool {
    !release.at_least(1, 19, 4) || release.at_least(1, 21, 5)
}

/// The riding jump the rider released (onPlayerJump, in percent): a horse lets the jump move it
/// rearing and rears where the client runs it, and jumps at its next tick on the ground; a
/// nautilus or camel with its cooldown run out (a camel on the ground) dashes at its next tick.
fn rider_jump(mob: &mut RiddenMobState, power: i32, release: MinecraftVersion) {
    let pending = |power: i32| {
        let percent = power.max(0);
        if percent >= 90 {
            1.0
        } else {
            0.4 + 0.4 * percent as f32 / 90.0
        }
    };
    match mob.kind {
        kind if kind.is_horse() => {
            if !mob.saddled {
                return;
            }
            if power >= 0 {
                mob.allow_stand_sliding = true;
                if horse_stands_locally(release) && kind.rears() {
                    mob.standing = true;
                    mob.stand_counter = if release.at_least(1, 21, 6) { 20 } else { 1 };
                }
            }
            mob.jump_pending_scale = pending(power);
        }
        RiddenMobKind::Nautilus | RiddenMobKind::ZombieNautilus => {
            if mob.dash_cooldown <= 0 {
                mob.jump_pending_scale = pending(power);
            }
        }
        RiddenMobKind::Camel => {
            if mob.dash_cooldown <= 0 && mob.on_ground {
                mob.jump_pending_scale = pending(power);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{tick_ridden_mob, RiddenMobKind, RiddenMobState};
    use crate::math::{Aabb, Vec3};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockShape, BlockState, FluidState, WorldSnapshot};

    const V1_16_5: MinecraftVersion = MinecraftVersion::new(1, 16, 5);
    const V1_21_1: MinecraftVersion = MinecraftVersion::new(1, 21, 1);
    const V26_2: MinecraftVersion = MinecraftVersion::new(26, 2, 0);

    fn stone_floor(x: (i32, i32), y: i32, z: (i32, i32)) -> WorldSnapshot {
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        let mut world = WorldSnapshot::new();
        for bx in x.0..=x.1 {
            for bz in z.0..=z.1 {
                world.set_block((bx, y, bz), stone);
            }
        }
        world
    }

    fn fluid_pool(fluid: FluidState, x: (i32, i32), y: (i32, i32), z: (i32, i32)) -> WorldSnapshot {
        let source = BlockState {
            fluid,
            fluid_source: true,
            ..BlockState::default()
        };
        let mut world = WorldSnapshot::new();
        for bx in x.0..=x.1 {
            for by in y.0..=y.1 {
                for bz in z.0..=z.1 {
                    world.set_block((bx, by, bz), source);
                }
            }
        }
        world
    }

    fn ridden(kind: RiddenMobKind, position: Vec3, velocity: Vec3, yaw: f32) -> RiddenMobState {
        let mut mob = RiddenMobState::new(kind, position);
        mob.velocity = velocity;
        mob.yaw = yaw;
        mob.rider.yaw = 270.0;
        mob
    }

    #[test]
    fn horse_walks_at_its_movement_speed_with_its_riders_input() {
        let world = stone_floor((-3, 6), 100, (2170, 2177));
        let mut horse = ridden(
            RiddenMobKind::Horse,
            Vec3::new(0.5, 101.0, 2173.5),
            Vec3::new(0.0, -0.0784000015258789, 0.0),
            270.0,
        );
        horse.on_ground = true;
        horse.movement_speed = 0.225;
        horse.jump_strength = 0.7;

        tick_ridden_mob(&mut horse, &world, V1_21_1);
        assert_eq!(horse.position, Vec3::new(0.5, 101.0, 2173.5));
        assert_eq!(horse.velocity, Vec3::new(0.0, -0.0784000015258789, 0.0));

        horse.rider.forward = 0.98;
        tick_ridden_mob(&mut horse, &world, V1_21_1);
        assert_eq!(horse.ridden_speed, 0.225);
        assert_eq!(horse.position, Vec3::new(0.7204999984502791, 101.0, 2173.5));
        assert_eq!(horse.velocity.x, 0.12039301313781722);

        tick_ridden_mob(&mut horse, &world, V1_21_1);
        assert_eq!(horse.position.x, 1.058985149775619);
        assert_eq!(horse.velocity.x, 0.18481291409014125);
        assert!(horse.on_ground);
    }

    #[test]
    fn mob_easing_toward_a_server_teleport_steps_there_in_place_of_the_fade() {
        let world = stone_floor((-3, 6), 100, (2170, 2177));
        let start = Vec3::new(0.5, 101.0, 2173.5);
        let step = Vec3::new(1.0, 101.0, 2173.5);
        let velocity = Vec3::new(0.1, 0.0, 0.0);
        let release = MinecraftVersion::new(1, 20, 4);
        let mut faded = ridden(RiddenMobKind::Horse, step, velocity, 270.0);
        faded.on_ground = true;
        tick_ridden_mob(&mut faded, &world, release);
        let mut stepped = ridden(RiddenMobKind::Horse, start, velocity, 270.0);
        stepped.on_ground = true;
        stepped.lerp_position = Some(step);
        tick_ridden_mob(&mut stepped, &world, release);
        assert_eq!(faded.position.x, 1.0 + 0.1 * 0.98);
        assert_eq!(stepped.position.x, 1.0 + 0.1);
        assert_eq!(stepped.lerp_position, None);

        let mut kept = ridden(RiddenMobKind::Horse, step, Vec3::ZERO, 270.0);
        kept.on_ground = true;
        tick_ridden_mob(&mut kept, &world, V1_16_5);
        let mut stepped = ridden(RiddenMobKind::Horse, start, Vec3::ZERO, 270.0);
        stepped.on_ground = true;
        stepped.lerp_position = Some(step);
        tick_ridden_mob(&mut stepped, &world, V1_16_5);
        assert_eq!(stepped.position, kept.position);
        assert_eq!(stepped.bounding_box, kept.bounding_box);
    }

    #[test]
    fn horse_jumps_with_the_charge_its_rider_released_before_1_19_4() {
        let world = stone_floor((-3, 4), 100, (2170, 2177));
        let mut horse = ridden(
            RiddenMobKind::Horse,
            Vec3::new(0.5, 101.0, 2173.5),
            Vec3::new(0.0, -0.0784000015258789, 0.0),
            270.0,
        );
        horse.on_ground = true;
        horse.movement_speed = 0.225;
        horse.jump_strength = 0.7;
        horse.bounding_box = Some(Aabb::new(
            Vec3::new(-0.1982421875, 101.0, 2172.8017578125),
            Vec3::new(1.1982421875, 102.60000002384186, 2174.1982421875),
        ));
        horse.rider.released_jump = Some(90);

        tick_ridden_mob(&mut horse, &world, V1_16_5);
        assert_eq!(horse.position.y, 101.0);
        assert_eq!(horse.jump_pending_scale, 1.0);
        assert!(horse.standing && horse.allow_stand_sliding);

        tick_ridden_mob(&mut horse, &world, V1_16_5);
        assert_eq!(horse.position.y, 101.7);
        assert_eq!(horse.velocity.y, 0.6076000118255616);
        assert_eq!(horse.jump_pending_scale, 0.0);
        assert!(!horse.on_ground);
    }

    #[test]
    fn pig_speeds_up_by_its_boost() {
        let world = stone_floor((0, 6), 100, (3242, 3249));
        let mut pig = ridden(
            RiddenMobKind::Pig,
            Vec3::new(2.9381919428134062, 101.0, 3245.5),
            Vec3::new(0.06605965531153825, -0.0784000015258789, 0.0),
            270.0,
        );
        pig.on_ground = true;
        pig.boosting = true;
        pig.boost_time_total = 309;

        tick_ridden_mob(&mut pig, &world, V1_21_1);

        assert_eq!(pig.boost_time, 1);
        assert_eq!(f64::from(pig.ridden_speed), 0.05690738186240196);
        assert_eq!(pig.position.x, 3.0598377868811157);
        assert_eq!(pig.velocity.x, 0.06641863857566875);
    }

    #[test]
    fn camel_dashes_along_its_look_at_the_tick_after_its_rider_released_the_jump() {
        let world = stone_floor((0, 12), 100, (2170, 2177));
        let mut camel = ridden(
            RiddenMobKind::Camel,
            Vec3::new(3.957551177174496, 101.0, 2173.5),
            Vec3::new(0.1060720840443712, -0.0784000015258789, 0.0),
            270.0,
        );
        camel.on_ground = true;
        camel.rider.forward = 0.98;
        camel.rider.released_jump = Some(90);

        tick_ridden_mob(&mut camel, &world, V26_2);
        assert_eq!(camel.position.x, 4.151823266440234);
        assert_eq!(camel.jump_pending_scale, 1.0);

        tick_ridden_mob(&mut camel, &world, V26_2);
        assert_eq!(
            camel.position,
            Vec3::new(6.346093959624992, 101.52157000337422, 2173.5)
        );
        assert_eq!(
            camel.velocity,
            Vec3::new(
                1.1980719376380815,
                0.43273861172901323,
                1.3373132578722665e-16
            )
        );
        assert!(camel.dashing);
        assert_eq!(camel.dash_cooldown, 54);
    }

    #[test]
    fn strider_settles_on_the_still_lava_it_falls_onto() {
        let world = fluid_pool(FluidState::Lava, (-3, 4), (98, 100), (1934, 1941));
        let mut strider = ridden(
            RiddenMobKind::Strider,
            Vec3::new(0.5280087501006574, 100.92159999847412, 1937.5),
            Vec3::new(0.016729213121191414, -0.1552320045166016, 0.0),
            270.0,
        );
        strider.ridden_speed = 0.09625;

        tick_ridden_mob(&mut strider, &world, V26_2);
        assert_eq!(
            strider.position,
            Vec3::new(0.5543629631696948, 100.76636799395752, 1937.5)
        );
        assert_eq!(
            strider.velocity,
            Vec3::new(0.023982334583990732, -0.230527368912964, 0.0)
        );
        assert!(strider.on_ground && !strider.suffocating);

        tick_ridden_mob(&mut strider, &world, V26_2);
        tick_ridden_mob(&mut strider, &world, V26_2);
        assert_eq!(
            strider.position,
            Vec3::new(0.8364921539914629, 100.5, 1937.5)
        );
        assert_eq!(
            strider.velocity,
            Vec3::new(0.08839569507498411, -0.0784000015258789, 0.0)
        );
    }

    #[test]
    fn happy_ghast_flies_along_its_riders_look_without_gravity() {
        let world = stone_floor((-3, 4), 100, (3242, 3249));
        let mut ghast = ridden(
            RiddenMobKind::HappyGhast,
            Vec3::new(0.5, 101.0, 3245.5),
            Vec3::ZERO,
            -90.0,
        );

        tick_ridden_mob(&mut ghast, &world, V26_2);
        assert_eq!(ghast.position, Vec3::new(0.5, 101.0, 3245.5));

        ghast.rider.forward = 0.98;
        tick_ridden_mob(&mut ghast, &world, V26_2);
        assert_eq!(ghast.position, Vec3::new(0.5162500008816521, 101.0, 3245.5));
        assert_eq!(ghast.velocity, Vec3::new(0.014787501228476599, 0.0, 0.0));

        tick_ridden_mob(&mut ghast, &world, V26_2);
        assert_eq!(ghast.position.x, 0.5472875029917807);
        assert_eq!(ghast.velocity.x, 0.02824412773420796);
    }

    #[test]
    fn nautilus_swims_at_its_ridden_speed_in_water() {
        let world = fluid_pool(FluidState::Water, (-3, 4), (95, 100), (3298, 3305));
        let mut nautilus = ridden(
            RiddenMobKind::Nautilus,
            Vec3::new(0.5, 97.0, 3301.5),
            Vec3::ZERO,
            -90.0,
        );

        tick_ridden_mob(&mut nautilus, &world, V26_2);
        assert_eq!(nautilus.position, Vec3::new(0.5, 97.0, 3301.5));

        nautilus.rider.forward = 0.98;
        tick_ridden_mob(&mut nautilus, &world, V26_2);
        assert_eq!(f64::from(nautilus.ridden_speed), 0.032499998807907104);
        assert_eq!(
            nautilus.position,
            Vec3::new(0.5324999988079071, 97.0, 3301.5)
        );
        assert_eq!(nautilus.velocity, Vec3::new(0.029249998927116393, 0.0, 0.0));

        tick_ridden_mob(&mut nautilus, &world, V26_2);
        assert_eq!(nautilus.position.x, 0.5942499965429306);
        assert_eq!(nautilus.velocity.x, 0.055574997961521155);
    }
}
