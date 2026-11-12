use crate::legacy_fluid;
use crate::math::{Aabb, Vec3};
use crate::simulation::{
    applies_effects_from_blocks, apply_block_effects, apply_inside_block_effects, block_below_affecting_movement,
    block_position, block_speed_factor, blocks_inside, blocks_inside_moves, climbable,
    collision_query_entity, entity_collision_context, fluid_falling_adjusted_y,
    accumulated_fall_distance, compute_modified_friction, position_of_bounding_box, friction_influenced_speed, landing_position, main_supporting_block, movements_this_tick,
    mth_cos, mth_sin, push_by_fluid, relative_movement, water_contact, yaw_radians,
    SimulationState,
};
use crate::vehicle::{lava_flow_scale, move_vehicle, MoveRules, VehicleMove};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, FluidContact, FluidState, WorldSnapshot};

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

    /// The kind of a mob by its entity name in a release that has it. Donkeys, mules and the undead
    /// horses are types from 1.11 and HorseType variants of the horse before it (1.10.2
    /// EntityType.java:310, HorseType.java:10-21, 1.11.2 EntityType.java:200-204), so all of them
    /// ride from the first release. Llama: 1.11.2 EntityType.java:270, not in 1.10.2. Trader
    /// llama: 1.14.4 EntityType.java, not in 1.13.2. Strider: 1.16, not in 1.15. Camel: 1.19.3
    /// EntityType.java:203 (behind UPDATE_1_20), not in 1.19.2. Happy ghast: 1.21.6, not in
    /// 1.21.5. Nautilus, zombie nautilus and camel husk: 1.21.11, not in 1.21.10.
    pub fn parse_for(name: &str, release: MinecraftVersion) -> Result<Self, String> {
        let kind = Self::parse(name).ok_or_else(|| format!("no ridden mob simulation for {name}"))?;
        let first = match name {
            "llama" => MinecraftVersion::new(1, 11, 0),
            "trader_llama" => MinecraftVersion::new(1, 14, 0),
            "strider" => MinecraftVersion::new(1, 16, 0),
            "camel" => MinecraftVersion::new(1, 19, 3),
            "happy_ghast" => MinecraftVersion::new(1, 21, 6),
            "nautilus" | "zombie_nautilus" | "camel_husk" => MinecraftVersion::new(1, 21, 11),
            _ => return Ok(kind),
        };
        if release < first {
            return Err(format!("{name} is not in {release}: it arrives in {first}"));
        }
        Ok(kind)
    }

    /// EntityType's width and height: before 1.14 every horse has AbstractHorse's setSize, 1.4F
    /// wide before 1.9 (1.8.8 AbstractHorse.java:209; 1.9.4 :242, 1.13.2
    /// AbstractHorse.java:148), and a donkey is 1.5 high from 1.14 (1.14.4
    /// EntityType.java:514).
    #[must_use]
    pub fn dimensions(self, release: MinecraftVersion) -> (f32, f32) {
        match self {
            Self::Pig => (0.9, 0.9),
            Self::Strider => (0.9, 1.7),
            Self::Camel => (1.7, 2.375),
            Self::HappyGhast => (4.0, 4.0),
            Self::Nautilus | Self::ZombieNautilus => (0.875, 0.95),
            Self::Horse | Self::Donkey | Self::Mule | Self::SkeletonHorse | Self::ZombieHorse
                if !release.at_least(1, 9, 0) =>
            {
                (1.4, 1.6)
            }
            // (1.13.2 AbstractHorse.java:148; 1.14.4 EntityType.java:514)
            Self::Donkey if release.at_least(1, 14, 0) => (1.3964844, 1.5),
            Self::Horse | Self::Donkey | Self::Mule | Self::SkeletonHorse | Self::ZombieHorse => {
                (1.3964844, 1.6)
            }
            Self::Llama | Self::TraderLlama => (0.9, 1.87),
        }
    }

    /// EntityType's eye height, 0.85F of the height where the type gives none (1.20.5
    /// EntityType.java:664, 227, 239, 26.3 EntityTypes.java:357, 397, 405, 427,
    /// EntityDimensions.java:15).
    fn eye_height(self) -> f32 {
        match self {
            Self::Pig => 0.9 * 0.85,
            Self::Strider => 1.7 * 0.85,
            Self::Camel => 2.275,
            Self::HappyGhast => 2.6,
            Self::Nautilus | Self::ZombieNautilus => 0.2751,
            Self::Donkey => 1.425,
            Self::Llama | Self::TraderLlama => 1.7765,
            Self::Horse | Self::Mule | Self::SkeletonHorse | Self::ZombieHorse => 1.52,
        }
    }

    /// The mob's MOVEMENT_SPEED attribute base (createAttributes: Pig#createAttributes,
    /// Strider#createAttributes, Camel#createAttributes, HappyGhast#createAttributes,
    /// AbstractNautilus#createAttributes, ZombieNautilus#createAttributes,
    /// AbstractHorse#createBaseHorseAttributes, AbstractChestedHorse#createBaseChestedHorseAttributes,
    /// SkeletonHorse#createAttributes, ZombieHorse#createAttributes).
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

    /// The base MOVEMENT_SPEED of the release: from 1.21.11 ZombieHorse.createAttributes adds none of
    /// its own, so it keeps AbstractHorse.createBaseHorseAttributes' 0.225F (1.21.10
    /// ZombieHorse.java:37 adds 0.2F; 1.21.11 ZombieHorse.java:56, 26.3 :54 do not)
    pub fn movement_speed_in(self, release: MinecraftVersion) -> f64 {
        if self == Self::ZombieHorse && release.at_least(1, 21, 11) {
            Self::Horse.movement_speed()
        } else {
            self.movement_speed()
        }
    }

    pub(crate) fn is_horse(self) -> bool {
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

    /// Whether LivingEntity.onClimbable can hold the mob: AbstractHorse's is false, for every
    /// horse, donkey, mule, llama and camel (1.8.8 AbstractHorse.java:1390, 1.12.2
    /// AbstractHorse.java:963, 1.21.4 :984, 1.21.11 AbstractHorse.java:940, 26.3 :920;
    /// Camel and AbstractChestedHorse inherit it).
    fn climbs(self) -> bool {
        !self.is_horse() && self != Self::Camel
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
/// dash). `saddled` false is a mob its rider controls without steering it (before 1.19 a horse
/// without its saddle, a pig or strider without its stick): it goes on its own input.
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
    /// Entity.getFluidHeight(LAVA) > 0 of the last base tick, which 1.16 - 1.16.1 do not measure
    /// in water (and whose inLava is the flag of the last move).
    pub lava_height_positive: bool,
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
    /// MOVEMENT_SPEED with the suffocating modifier, where other modifiers keep it from being
    /// `movement_speed` less 34% of it.
    pub suffocating_speed: Option<f64>,
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
    /// The mob's dimension is ultrawarm: lava pushes it at 0.007 (FAST_LAVA).
    pub ultra_warm: bool,
    /// The amplifier of the mob's Jump Boost, which its riding jump adds 0.1F a level to.
    pub jump_boost_amplifier: Option<i32>,
    /// The mob's synced NoGravity flag (Entity.isNoGravity, 1.10+): its travel takes no gravity.
    pub no_gravity: bool,
    /// Entity.stuckSpeedMultiplier, which a cobweb, berry bush or powder snow leaves for the next
    /// move (1.9.4 Entity.java:550-552, 26.3 :783-788, 3072-3074).
    pub stuck_speed_multiplier: Vec3,
    /// Width and height over the type's, of a mob that is not its type's size: the SCALE attribute
    /// and age (1.20.5 LivingEntity.java:3264-3269, 26.3 :3684-3689).
    pub scale: (f32, f32),
    /// Slow Falling (1.13+): at most 0.01 of gravity while it falls.
    pub slow_falling: bool,
    /// The amplifier of Levitation (1.9+), which replaces its gravity in the air.
    pub levitation_amplifier: Option<i32>,
    /// Dolphin's Grace (1.13+): its water slow-down is 0.96F.
    pub dolphins_grace: bool,
    /// Weaving (1.20.5+): a cobweb leaves it (0.5, 0.25, 0.5), not the (0.25, 0.05F, 0.25) of a
    /// mob without it (1.20.5 WebBlock.java:24-31, 26.3 WebBlock.java:18-26).
    pub weaving: bool,
    /// The GRAVITY attribute (1.20.5+), 0.08 when not given.
    pub gravity: Option<f64>,
    /// The STEP_HEIGHT attribute (1.20.5+), the mob's base when not given.
    pub step_height: Option<f64>,
    /// The WATER_MOVEMENT_EFFICIENCY attribute (1.21+).
    pub water_movement_efficiency: f64,
    /// The AIR_DRAG_MODIFIER attribute (26.2+), 1 when not given.
    pub air_drag_modifier: Option<f64>,
    /// The FRICTION_MODIFIER attribute (26.2+), 1 when not given.
    pub friction_modifier: Option<f64>,
    /// A move past MAX_WALKED_CELLS did not run, and the bindings refuse the tick (`move_vehicle`).
    pub move_out_of_range: bool,
    /// Set for a mob that travels on its own (`plain_living_travel`), which no kind describes.
    pub(crate) plain: Option<PlainMob>,
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
            lava_height_positive: false,
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
            suffocating_speed: None,
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
            ultra_warm: false,
            jump_boost_amplifier: None,
            no_gravity: false,
            stuck_speed_multiplier: Vec3::ZERO,
            move_out_of_range: false,
            scale: (1.0, 1.0),
            slow_falling: false,
            levitation_amplifier: None,
            dolphins_grace: false,
            weaving: false,
            gravity: None,
            step_height: None,
            water_movement_efficiency: 0.0,
            air_drag_modifier: None,
            friction_modifier: None,
            plain: None,
        }
    }

    /// The type's dimensions in the mob's pose: a camel in the SITTING pose (while it sits or sits
    /// down, Camel.isCamelSitting) is 1.43F lower (1.19.3 Camel.java:71, 138-139; 1.21.11
    /// Camel.java:70-79, 138-139; 26.3 Camel.java:146, 609).
    fn pose_dimensions(&self, release: MinecraftVersion) -> (f32, f32) {
        let (width, height) = self.kind.dimensions(release);
        if self.kind == RiddenMobKind::Camel && self.sitting {
            (width, height - 1.43)
        } else {
            (width, height)
        }
    }

    /// The mob's dimensions in its pose, at its scale.
    #[must_use]
    pub fn dimensions(&self, release: MinecraftVersion) -> (f32, f32) {
        let (width, height) = self.pose_dimensions(release);
        (width * self.scale.0, height * self.scale.1)
    }

    /// The scale that gives dimensions measured in the mob's pose.
    pub fn measure(&mut self, width: Option<f32>, height: Option<f32>, release: MinecraftVersion) {
        let (type_width, type_height) = self.pose_dimensions(release);
        self.scale = (
            width.map_or(1.0, |width| width / type_width),
            height.map_or(1.0, |height| height / type_height),
        );
    }

    /// Entity.getFluidJumpThreshold: the 0.4 of a mob whose scaled eyes are that high, else 0 (26.3
    /// Entity.java:3803-3805, 1.20.5 LivingEntity.java:3264-3269, EntityDimensions.java:40 scales the
    /// eyes by the height's factor; a sitting camel's are 0.845F, 26.3 Camel.java:609). It is 1.16's
    /// (1.16 Entity.java:2664-2666, 1.20.4 :3148-3150), and before 1.20.5 a baby's eyes are scaled
    /// the same way: its dimensions are the type's times getScale, 0.5F of a baby (1.20.4
    /// LivingEntity.java:561-563; 1.16 LivingEntity.java:2955,
    /// 1.20.4 :3185-3186), and its eyes 0.85F (1.20.4 Entity.java:2802-2804), 0.95F of a horse
    /// (1.20.4 AbstractHorse.java:1005-1007) or a camel's height less 0.1F * getScale (1.20.4
    /// Camel.java:149-151) of those, as the type's eyes times the scale give them. A baby pig
    /// saddled by a command and steered has its 0.765 eyes at 0.3825. Before 1.16 the jump in a
    /// fluid compares with 0.4 (1.15.2 LivingEntity.java:2295).
    fn fluid_jump_threshold(&self, release: MinecraftVersion) -> f64 {
        if let Some(plain) = self.plain {
            return plain.fluid_jump_threshold;
        }
        if !release.at_least(1, 16, 0) {
            return 0.4;
        }
        let eyes = if self.kind == RiddenMobKind::Camel && self.sitting {
            0.845
        } else {
            self.kind.eye_height()
        };
        if f64::from(eyes * self.scale.1) < 0.4 {
            0.0
        } else {
            0.4
        }
    }

    /// EntityDimensions#makeBoundingBox of the mob's dimensions at `position`.
    fn box_at(&self, position: Vec3, release: MinecraftVersion) -> Aabb {
        let (width, height) = self.dimensions(release);
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
    let (width, height) = mob.dimensions(release);
    // (1.16.5 Entity.java:631-632; 1.17 Entity.java:561)
    let kept_box = (!release.at_least(1, 17, 0)).then(|| {
        mob.bounding_box
            .unwrap_or_else(|| mob.box_at(mob.position, release))
    });
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
        no_gravity: mob.no_gravity,
        bounding_box: kept_box,
        previous_position: mob.previous_position,
        stuck_speed_multiplier: mob.stuck_speed_multiplier,
        weaving: mob.weaving,
        ..SimulationState::default()
    };
    let old_position = body.position;
    let previous_movement = mob
        .previous_position
        .map(|previous| body.position - previous);
    let support = release
        // (1.19.4 Entity.java:770-772; 1.20 Entity.java:797-799)
        .at_least(1, 20, 0)
        .then(|| main_supporting_block(world, &body, previous_movement, release))
        .flatten();
    // (1.18.2 Strider.java:306; 1.19 Strider.java:306)
    if mob.kind == RiddenMobKind::Strider && (!mob.no_ai || !release.at_least(1, 19, 0)) {
        mob.suffocating = !strider_is_warm(mob, &body, world, support, release);
    }
    let (water_height, lava_height) = push_by_fluids(mob, &mut body, world, release);
    mob.lava_height_positive = lava_height > 0.0;
    if let Some(position) = mob.lerp_position.take() {
        body.position = position;
        if body.bounding_box.is_some() {
            body.bounding_box = Some(mob.box_at(position, release));
        }
    } else if !release.at_least(1, 21, 5) {
        body.velocity = body.velocity.scale(0.98);
    }
    // (1.8.8 LivingEntity.java:1414; 1.9.4 LivingEntity.java:1689)
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
    // (1.8.8 LivingEntity.java:1133-1134; 1.9.4 LivingEntity.java:1323-1324)
    if !release.at_least(1, 9, 0) {
        // 1.8 moves a mob only where canMoveVoluntarily, off the client (1.8.8
        // LivingEntity.java:1134, 1408, 1556-1558): a horse's travel turns it to its rider and
        // drops its jump on the ground but moves it on the server alone (AbstractHorse.java:
        // 1064-1111), and the rider's jump goes to the server (LocalPlayer.java:264). The
        // mob keeps the server's position, by its interpolation.
        if mob.kind.is_horse() && mob.saddled {
            // AbstractHorse.travel's setRotation (1.8.8 AbstractHorse.java:1063-1065) takes
            // both % 360.0F (1.8.8 Entity.java:323-325)
            mob.yaw = mob.rider.yaw % 360.0;
            mob.pitch = (mob.rider.pitch * 0.5) % 360.0;
            if body.on_ground {
                mob.jump_pending_scale = 0.0;
                mob.horse_jumping = false;
            }
        }
        mob.rider.released_jump = None;
        mob.position = body.position;
        mob.velocity = body.velocity;
        mob.in_water = body.in_water;
        // (Entity.isInLava asks the world, at where the interpolation left it: 1.8.8 Entity.java:922-924)
        mob.in_lava = legacy_fluid::touching_lava(world, body.bounds(), release);
        mob.fall_distance = body.fall_distance;
        mob.bounding_box = body.bounding_box;
        mob.previous_position = Some(old_position);
        mob.stuck_speed_multiplier = body.stuck_speed_multiplier;
        mob.move_out_of_range |= body.move_out_of_range;
        return;
    }
    let speed_before = mob.ridden_speed;
    let forward_before = mob.own_forward;
    let input = tick_ridden(mob, &mut body, world, support, release);
    let mut moved = None;
    // (1.21.5 Mob.java:1191-1193; 1.21.6 Entity.java:3426-3428)
    if mob.no_ai && release.at_least(1, 21, 5) && !release.at_least(1, 21, 6) {
        // 1.21.5 Mob.canSimulateMovement is false without AI: travelRidden stops at its velocity,
        // before the setSpeed that writes both the speed and zza (LivingEntity.java:2335-2340)
        if mob.own_forward == mob.ridden_speed {
            mob.own_forward = forward_before;
        }
        mob.ridden_speed = speed_before;
        body.velocity = Vec3::ZERO;
    } else {
        let unsaddled = ((mob.kind.is_horse()
            || matches!(mob.kind, RiddenMobKind::Pig | RiddenMobKind::Strider))
            && !mob.saddled)
            || camel_ignores_rider_1_19_3(mob, release);
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
            water_height,
            lava_height,
        };
        moved = travel(mob, &mut body, around, input, air_speed);
    }
    // (1.21.1 Entity.java:738; 1.21.2 LivingEntity.java:2817-2819): the ridden mob is the local
    // player's to move, so its client applies them
    if release.at_least(1, 21, 2) && applies_effects_from_blocks(&body, release) {
        let (requested, displacement) = moved.map_or((Vec3::ZERO, Vec3::ZERO), |moved| {
            (moved.requested, moved.moved)
        });
        let movements = movements_this_tick(&body, old_position, requested, displacement, release);
        let (inside, _) = blocks_inside_moves(&body, world, &movements, release);
        let support = moved.and_then(|moved| moved.support);
        apply_block_effects(&mut body, world, support, release, &inside);
    }
    // 1.17 - 1.19.3 ItemSteerable.travel checks the blocks inside again after a steered travel
    // (1.17 ItemSteerable.java:48, 1.18.1 :46, 1.19.3 :48; none in 1.16.5 or 1.19.4)
    let steered = matches!(mob.kind, RiddenMobKind::Pig | RiddenMobKind::Strider) && mob.saddled;
    if steered && release.at_least(1, 17, 0) && !release.at_least(1, 19, 4) {
        check_inside_blocks(&mut body, world, release);
    }
    // and so does AbstractHorse.travel's steered branch, a saddled horse's (a llama has no
    // controlling passenger) or a 1.19.3 camel's that does not ignore its rider, by
    // tryCheckBlockCollision (1.17 HorseBaseEntity.java:765, 1.19 AbstractHorseEntity.java:763,
    // 1.19.3 :783; none in 1.16.5 :748-800 or 1.19.4)
    let horse_steered = (mob.kind.is_horse() && mob.kind.rears() && mob.saddled)
        || (mob.kind == RiddenMobKind::Camel && !camel_ignores_rider_1_19_3(mob, release));
    if horse_steered && release.at_least(1, 17, 0) && !release.at_least(1, 19, 4) {
        check_inside_blocks(&mut body, world, release);
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
    mob.stuck_speed_multiplier = body.stuck_speed_multiplier;
    mob.move_out_of_range |= body.move_out_of_range;
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
) -> (f64, f64) {
    let pushed = !mob.kind.is_nautilus();
    let bounds = body.bounds();
    let mut water_height = 0.0;
    if legacy_fluid::applies(release) {
        // Entity.handleWaterMovement before 1.13: the box shrunk by 0.4F vertically and 0.001 all
        // round, its water's flows summed and normalized (1.12.2 Entity.java:1040-1055,
        // Level.java:1616-1655; 1.9.4 Entity.java:948)
        legacy_fluid::handle_water_movement(body, world, pushed, release);
    } else {
        // (1.13: the fluid's own height, 1.13.2 Entity.java:2511-2551)
        let water = water_contact(world, bounds, release);
        water_height = water.height;
        body.in_water = water.touching;
        if water.touching {
            body.fall_distance = 0.0;
            if pushed {
                push_by_fluid(body, water, 0.014, release);
            }
        }
    }
    // 1.16 - 1.16.1 measure no lava in water and keep the inLava of the last move (1.16
    // Entity.java:1039-1048, 1161-1163; 1.16.1 Entity.java:1039-1047 returns in water before the
    // lava, :1161-1162 isTouchingLava); 1.16.2 measures the lava in water too and its inLava is that
    // height (1.16.2 Entity.java:1064-1070, 1178-1179)
    let measures_lava = release.at_least(1, 16, 2) || !release.at_least(1, 16, 0) || !body.in_water;
    let lava = if measures_lava {
        world.fluid_contact(FluidState::Lava, bounds, release)
    } else {
        FluidContact::default()
    };
    if release.at_least(1, 16, 0) {
        if pushed && measures_lava {
            push_by_fluid(body, lava, lava_flow_scale(mob.ultra_warm), release);
        }
        if release.at_least(1, 16, 2) {
            body.in_lava = lava.height > 0.0;
        }
    // (1.13.2 Entity.java:1143-1145; 1.14.4 Entity.java:568-569, 1171-1173)
    } else if !release.at_least(1, 14, 0) {
        // Entity.isInLava before 1.14: lava in the box shrunk by 0.1F sideways and 0.4F
        // vertically (1.9.4 Entity.java:1032, 1.12.2 :1129, 1.13.2 :1144)
        body.in_lava = legacy_fluid::touching_lava(world, bounds, release);
    }
    // Entity.baseTick: every tick in lava halves the fall distance (1.21.4 Entity.java:514)
    if body.in_lava {
        body.fall_distance *= 0.5;
    }
    (water_height, lava.height)
}

/// Strider.tick: warm on the blocks at its feet or under it that are lava, or touching lava in
/// the tick before (getFluidHeight(LAVA) > 0, 1.16 Strider.java:314; from 1.16.2 the
/// inLava of the base tick). The test is the same in 1.16.1 and 1.16.2 (1.16.1 Strider.java:314,
/// 1.16.2 Strider.java:309); 1.16.1 measures no lava height in water (1.16.1 Entity.java:1039-1047),
/// and from 1.16.2 isInLava is that height above 0 (1.16.2 Entity.java:1178-1179).
fn strider_is_warm(
    mob: &RiddenMobState,
    body: &SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> bool {
    let lava_before = if release.at_least(1, 16, 2) {
        body.in_lava
    } else {
        mob.lava_height_positive
    };
    world.fluid_at(block_position(body.position, release)) == FluidState::Lava
        || world.fluid_at(landing_position(world, body, support, release)) == FluidState::Lava
        || lava_before
}

/// The speed the air speed comes from before 1.19.4 (ItemSteerable.travel, AbstractHorse.travel:
/// flyingSpeed = getSpeed() * 0.1F before the ridden speed is set): the speed of the tick before,
/// but a strider's Strider.travel sets it to its move speed first.
fn legacy_flying_base(mob: &RiddenMobState, speed_before: f32) -> f32 {
    if mob.kind != RiddenMobKind::Strider {
        return speed_before;
    }
    strider_move_speed(mob)
}

/// Strider.getMoveSpeed before 1.19.4: its MOVEMENT_SPEED, 0.66F of it suffocating.
fn strider_move_speed(mob: &RiddenMobState) -> f32 {
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
            let input = Vec3::new(0.0, 0.0, f64::from(mob.own_forward));
            // Strider.travel's setSpeed(getMoveSpeed()) before ItemSteerable.travel, which a
            // Mob's setSpeed makes its zza too (1.16 Strider.java:260-266, Mob.java:488-491;
            // 1.18.1 Strider.java:249-252, Mob.java:507-510; 1.19.3 Strider.java:254-257). 1.19.4
            // drops the override for tickRidden and getRiddenSpeed, which only a controlling
            // rider runs (1.19.4 Strider.java:263-278): out of a saddle a strider travels at the
            // speed its own goals set, as a pig does
            if mob.kind == RiddenMobKind::Strider && !release.at_least(1, 19, 4) {
                mob.ridden_speed = strider_move_speed(mob);
                mob.own_forward = mob.ridden_speed;
            }
            input
        }
        RiddenMobKind::Pig | RiddenMobKind::Strider => {
            face_rider(mob);
            let mut boost_ended = false;
            if mob.boosting {
                mob.boost_time = mob.boost_time.wrapping_add(1);
                if mob.boost_time.wrapping_sub(1) > mob.boost_time_total {
                    mob.boosting = false;
                    boost_ended = true;
                }
            }
            // 1.9.4 - 1.10.2 end the boost inside the boosted speed's block, which adds the boost
            // in the tick it ends too (1.9.4 Pig.java:211-217, 1.10.2 :219-225; 1.11.2
            // :238-246 ends it first)
            let boosted = mob.boosting || (boost_ended && !release.at_least(1, 11, 0));
            mob.ridden_speed = steering_speed(mob, boosted, release);
            mob.own_forward = mob.ridden_speed;
            Vec3::new(0.0, 0.0, 1.0)
        }
        RiddenMobKind::Camel => {
            let refuses = camel_refuses_to_move(mob);
            if refuses && !release.at_least(1, 19, 4) {
                // 1.19.3: Camel.ignoresMovementInput stands a sitting camel up for a forward
                // input and says it ignores the rider while stationary, so AbstractHorse.travel
                // takes its other branch: no turn, jump or speed of the rider's, the camel's own
                // input after LivingEntity.tickMovement's 0.98 (1.19.3 AbstractHorse.java:746-789,
                // Camel.java:219-249, LivingEntity.java:2594-2598); from 1.19.4 getRiddenInput
                // gives a camel that refuses to move none (1.19.4 Camel.java:262-263)
                if rider.forward > 0.0 && mob.sitting && !camel_in_pose_transition(mob) {
                    mob.sitting = false;
                    mob.pose_time = 0;
                    body.height = f64::from(mob.dimensions(release).1);
                }
                mob.own_forward *= 0.98;
                return Vec3::new(0.0, 0.0, f64::from(mob.own_forward));
            }
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
                // Camel.standUp's setPose(STANDING) refreshes its dimensions at once
                // (1.21.11 Camel.java:228-234, 529-534, Entity.java:3305-3309)
                body.height = f64::from(mob.dimensions(release).1);
            }
            let bonus = if rider.sprinting && mob.dash_cooldown == 0 {
                0.1
            } else {
                0.0
            };
            mob.ridden_speed = mob.movement_speed as f32 + bonus;
            // LivingEntity.travelRidden's setSpeed, which a Mob's makes its zza too
            mob.own_forward = mob.ridden_speed;
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
            // no getRiddenSpeed of its own: travelRidden's setSpeed gives getSpeed() back, and
            // a Mob's setSpeed makes it its zza too
            mob.own_forward = mob.ridden_speed;
            Vec3::new(f64::from(strafe), f64::from(up), f64::from(forward)).scale(scale)
        }
        RiddenMobKind::Nautilus | RiddenMobKind::ZombieNautilus => {
            let (strafe, up, forward) = look_input(rider, release);
            turn_toward_rider(mob, 0.5);
            // (horse_jumping is LivingEntity.jumping from 1.21.6: AbstractNautilus.tickRidden)
            if mob.jump_pending_scale > 0.0 && !mob.horse_jumping {
                nautilus_dash(mob, body, world, support, release);
            }
            mob.jump_pending_scale = 0.0;
            let share = if body.in_water { 0.0325 } else { 0.02 };
            mob.ridden_speed = share * mob.movement_speed as f32;
            mob.own_forward = mob.ridden_speed;
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

/// A happy ghast's or nautilus's turn: a share of the way to its rider's yaw, half its pitch
/// (HappyGhast#tickRidden, AbstractNautilus#tickRidden).
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
/// ItemSteerable.travel's float steering speed, 0.23 suffocating, plus its boost (1.19.3 Pig.java:253
/// 0.225F; 1.19.4 Pig.java:258 0.225).
fn steering_speed(mob: &RiddenMobState, boosted: bool, release: MinecraftVersion) -> f32 {
    let strider = mob.kind == RiddenMobKind::Strider;
    let boost_sin = || {
        let share = mob.boost_time as f32 / mob.boost_time_total as f32 * 3.1415927;
        mth_sin(f64::from(share), release) as f32
    };
    // (1.19.3 Strider.java:265; 1.19.4 Strider.java:277)
    if !release.at_least(1, 19, 4) {
        let share = match (strider, mob.suffocating) {
            (false, _) => 0.225,
            (true, false) => 0.55,
            (true, true) => 0.23,
        };
        let speed = mob.movement_speed as f32 * share;
        return if boosted {
            speed + speed * 1.15 * boost_sin()
        } else {
            speed
        };
    }
    let factor = if boosted {
        1.0 + 1.15 * boost_sin()
    } else {
        1.0
    };
    if !strider {
        return (mob.movement_speed * 0.225 * f64::from(factor)) as f32;
    }
    let (speed, share) = if mob.suffocating {
        (
            mob.suffocating_speed
                .unwrap_or(mob.movement_speed + mob.movement_speed * -0.3400000035762787),
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
    // (1.19.3 AbstractHorse.java:757; 1.19.4 AbstractHorse.java:763)
    if !release.at_least(1, 19, 4) {
        if mob.jump_pending_scale > 0.0 && !mob.horse_jumping && body.on_ground {
            horse_jump(mob, body, world, support, release, input.z);
            mob.jump_pending_scale = 0.0;
        }
    } else if body.on_ground {
        // from 1.21.6 the check reads LivingEntity.jumping (its AbstractHorse.tickRidden); the
        // horse's own flag, cleared here and set by the jump, is gone (1.21.5
        // AbstractHorse.java:711-713, 749; 1.21.6 AbstractHorse.java:717-719, 750-754)
        if !release.at_least(1, 21, 6) {
            mob.horse_jumping = false;
        }
        if mob.jump_pending_scale > 0.0 && !mob.horse_jumping {
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
    let at_feet = factor(block_position(body.position, release));
    if at_feet == 1.0 {
        factor(block_below_affecting_movement(world, body, support, release))
    } else {
        at_feet
    }
}

/// Jump Boost's part of a riding jump: (amplifier + 1) * 0.1F, a float the double jump takes before
/// 1.20.5 (1.9.4 AbstractHorse.java:1047-1050, 1.14.4 :660-667, 1.18 :726-727, LivingEntity.java:
/// 1975-1977), getJumpBoostPower's 0.1F * (amplifier + 1.0F) in getJumpPower's float sum from
/// 1.20.5 (1.20.5 LivingEntity.java:2065-2072, 1.21.11 :2258-2264).
fn jump_boost_power(mob: &RiddenMobState) -> f32 {
    mob.jump_boost_amplifier
        .map_or(0.0, |amplifier| 0.1 * (amplifier as f32 + 1.0))
}

/// AbstractHorse.executeRidersJump: up by JUMP_STRENGTH times the charge (and the block's jump
/// factor from 1.15, in float from 1.20.5) plus Jump Boost's float 0.1F a level, and moving
/// forward 0.4F of the charge along its yaw. The double sum until 1.20.4 (1.20.4
/// AbstractHorse.java:800-801), getJumpPower's float from 1.20.5 (1.20.5 AbstractHorse.java:789,
/// LivingEntity.java:2063-2064); setIsJumping(true) until 1.21.5 (1.21.5 AbstractHorse.java:749,
/// none in 1.21.6 AbstractHorse.java:750-754).
fn horse_jump(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
    forward: f64,
) {
    let scale = mob.jump_pending_scale;
    let boost = jump_boost_power(mob);
    // (1.14.4 AbstractHorse.java:664; 1.15 AbstractHorse.java:744)
    body.velocity.y = if !release.at_least(1, 15, 0) {
        mob.jump_strength * f64::from(scale) + f64::from(boost)
    } else if !release.at_least(1, 20, 5) {
        let factor = block_jump_factor(body, world, support, release);
        mob.jump_strength * f64::from(scale) * f64::from(factor) + f64::from(boost)
    } else {
        let factor = block_jump_factor(body, world, support, release);
        f64::from(mob.jump_strength as f32 * scale * factor + boost)
    };
    if !release.at_least(1, 21, 6) {
        mob.horse_jumping = true;
    }
    if forward > 0.0 {
        let yaw = yaw_radians(mob.yaw, release);
        let sin = mth_sin(yaw, release) as f32;
        let cos = mth_cos(yaw, release) as f32;
        body.velocity.x += f64::from(-0.4 * sin * scale);
        body.velocity.z += f64::from(0.4 * cos * scale);
    }
}

/// A 1.19.3 camel that is stationary, whose Camel.ignoresMovementInput sends AbstractHorse.travel
/// to the branch of a mob nobody steers (1.19.3 Camel.java:241-248, AbstractHorse.java:749); from
/// 1.19.4 travelRidden takes the ridden input, which getRiddenInput zeroes (1.19.4
/// Camel.java:262-263).
fn camel_ignores_rider_1_19_3(mob: &RiddenMobState, release: MinecraftVersion) -> bool {
    mob.kind == RiddenMobKind::Camel
        && !release.at_least(1, 19, 4)
        && camel_refuses_to_move(mob)
}

/// Camel#refuseToMove: sitting, or sitting down or standing up (1.19.3 Camel.java:233-235, 26.1
/// :240-242). `sitting` is the camel's sitting state, which 1.19.3 reads as isPoseSitting and
/// 1.19.4 on as isCamelSitting.
fn camel_refuses_to_move(mob: &RiddenMobState) -> bool {
    mob.sitting || camel_in_pose_transition(mob)
}

/// Camel#isInPoseTransition: under 40 ticks in the pose while sitting, 52 while standing.
fn camel_in_pose_transition(mob: &RiddenMobState) -> bool {
    mob.pose_time < if mob.sitting { 40 } else { 52 }
}

/// Camel.executeRidersJump: a dash along its look, 22.2222F * the charge * MOVEMENT_SPEED far and
/// 1.4285F * the charge * its jump power high (in double before 1.20.5, LivingEntity.getJumpPower
/// in float since), Jump Boost's 0.1F a level in it (1.19.3 Camel.java:266-267, 1.21.11
/// Camel.java:267-267). The double jump power until 1.20.4 (1.20.4 Camel.java:287), the float
/// getJumpPower() from 1.20.5 (1.20.5 Camel.java:256, LivingEntity.java:2059-2064).
fn camel_dash(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) {
    let scale = mob.jump_pending_scale;
    let jump_factor = block_jump_factor(body, world, support, release);
    let boost = jump_boost_power(mob);
    let jump_power = if release.at_least(1, 20, 5) {
        f64::from(mob.jump_strength as f32 * jump_factor + boost)
    } else {
        mob.jump_strength * f64::from(jump_factor) + f64::from(boost)
    };
    let look = view_vector(mob.pitch, mob.yaw, release);
    let speed_factor = f64::from(block_speed_factor(world, body, support, release));
    let distance = f64::from(22.2222_f32 * scale) * mob.movement_speed * speed_factor;
    let along = look.horizontal().normalized(release).scale(distance);
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
pub(crate) fn view_vector(pitch: f32, yaw: f32, release: MinecraftVersion) -> Vec3 {
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

/// RangedAttribute.sanitizeValue: NaN is the minimum, the rest is clamped (1.20.5
/// RangedAttribute.java:30-31, 26.3 RangedAttribute.java:29-31). What a caller gives for an
/// attribute a mob's travel reads is taken through it, so no value reaches the motion unsanitized.
fn sanitized(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        min
    } else {
        value.clamp(min, max)
    }
}

/// LivingEntity.maxUpStep with a player in control: at least 1 (a camel's 1.5); a pig or strider
/// its rider does not steer steps 0.5 (1.16 ItemSteerable.java:50). From 1.20.5 the STEP_HEIGHT
/// attribute, at least 1 with a player in control (1.20.5 LivingEntity.java:3507-3510, 26.3
/// :3974-3977), sanitized as RangedAttribute does (Attributes.java:86).
fn step_height(mob: &RiddenMobState, release: MinecraftVersion) -> f32 {
    if let Some(plain) = mob.plain {
        return plain.max_step;
    }
    let controlled = !matches!(mob.kind, RiddenMobKind::Pig | RiddenMobKind::Strider) || mob.saddled;
    if let Some(step) = mob.step_height.filter(|_| release.at_least(1, 20, 5)) {
        let step = sanitized(step, 0.0, 10.0) as f32;
        return if controlled { step.max(1.0) } else { step };
    }
    match mob.kind {
        RiddenMobKind::Pig | RiddenMobKind::Strider if !mob.saddled => 0.5,
        RiddenMobKind::Camel => 1.5,
        _ => 1.0,
    }
}

/// LivingEntity.getEffectiveGravity: the GRAVITY attribute from 1.20.5, sanitized (Attributes.java:
/// 46), 0 under NoGravity, at most 0.01 falling under Slow Falling (1.13.2 LivingEntity.java:1511,
/// 1.20.5 :2107-2111, 26.3 :2333-2339).
pub(crate) fn effective_gravity(
    attribute: Option<f64>,
    no_gravity: bool,
    slow_falling: bool,
    velocity_y: f64,
    release: MinecraftVersion,
) -> f64 {
    let gravity = attribute
        .filter(|_| release.at_least(1, 20, 5))
        .map_or(0.08, |gravity| sanitized(gravity, -1.0, 1.0));
    let gravity = if no_gravity { 0.0 } else { gravity };
    if slow_falling && velocity_y <= 0.0 && release.at_least(1, 13, 0) {
        gravity.min(0.01)
    } else {
        gravity
    }
}

/// Where a ridden mob travels: the world, the release, the block that supported it as the tick
/// began and the height of the lava in its box (Entity.getFluidHeight, from Entity.baseTick).
#[derive(Clone, Copy)]
struct Surroundings<'a> {
    world: &'a WorldSnapshot,
    release: MinecraftVersion,
    support: Option<BlockPosition>,
    water_height: f64,
    lava_height: f64,
}

/// What a mob out of a saddle keeps of its own in LivingEntity.travel. A mount's rules, which go by
/// its kind, say nothing about these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlainMob {
    pub(crate) yaw: f32,
    /// Entity.maxUpStep.
    pub(crate) max_step: f32,
    /// Entity.isPushedByFluid: a water creature's flow does not move it.
    pub(crate) pushed_by_fluid: bool,
    /// Entity.getFluidJumpThreshold.
    pub(crate) fluid_jump_threshold: f64,
    /// A mob's own LivingEntity.onClimbable: a spider's flag in place of the block it is in.
    pub(crate) climbing: Option<bool>,
    /// LivingEntity.getWaterSlowDown, where it is not the 0.8F of every mob.
    pub(crate) water_slow_down: Option<f32>,
    /// FlyingAnimal (Entity.omnidirectionalAirMover from 26.2): the air slows its climb as it does
    /// the rest of its motion.
    pub(crate) omnidirectional: bool,
    /// The effects and attributes of LivingEntity.travel.
    pub(crate) slow_falling: bool,
    pub(crate) levitation_amplifier: Option<i32>,
    pub(crate) dolphins_grace: bool,
    pub(crate) gravity: Option<f64>,
    pub(crate) water_movement_efficiency: f64,
    /// An empty checkFallDamage: no fall is counted, and LivingEntity's water refresh there is
    /// skipped with it.
    pub(crate) no_fall_check: bool,
    /// Vex.move before 1.21.2: the blocks inside after a move that does not look for them.
    pub(crate) inside_after_move: bool,
    /// LivingEntity.shouldDiscardFriction from 1.17: a goat's or frog's long jump, a breeze's
    /// jump, keep their motion through the air with no drag.
    pub(crate) discard_friction: bool,
}

/// Entity.move of a living entity with nothing of a mount about it: LivingEntity.checkFallDamage
/// refreshes its water where the move left it, and counts the fall.
pub(crate) fn plain_living_move(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    rules: MoveRules,
    plain: PlainMob,
) -> VehicleMove {
    let mut mob = RiddenMobState::new(RiddenMobKind::Pig, body.position);
    mob.plain = Some(plain);
    move_mob(&mob, body, world, release, rules)
}

/// LivingEntity.travel of a mob with nothing of a mount about it, as its own control moves it: a
/// fish or guardian out of water, a vex. `body` holds the rest of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plain_living_travel(
    kind: RiddenMobKind,
    position: Vec3,
    speed: f32,
    input: Vec3,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    plain: PlainMob,
) -> Option<VehicleMove> {
    // (a pig stands for any mob that is not a mount: nothing else of its kind counts out of a saddle)
    let mut mob = RiddenMobState::new(kind, position);
    mob.ridden_speed = speed;
    mob.movement_speed = f64::from(speed);
    mob.yaw = plain.yaw;
    mob.no_gravity = body.no_gravity;
    mob.slow_falling = plain.slow_falling;
    mob.levitation_amplifier = plain.levitation_amplifier;
    mob.dolphins_grace = plain.dolphins_grace;
    mob.gravity = plain.gravity;
    mob.water_movement_efficiency = plain.water_movement_efficiency;
    mob.plain = Some(plain);
    let bounds = body.bounds();
    let contact = |fluid| world.fluid_contact(fluid, bounds, release).height;
    let support = release
        // (1.19.4 Entity.java:770-772; 1.20 Entity.java:797-799)
        .at_least(1, 20, 0)
        .then(|| main_supporting_block(world, body, None, release))
        .flatten();
    let around = Surroundings {
        world,
        release,
        support,
        water_height: contact(FluidState::Water),
        lava_height: contact(FluidState::Lava),
    };
    travel(&mob, body, around, input, 0.02)
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
    // aiStep resets the fall distance under Slow Falling or Levitation before travel from 1.20 (1.20
    // LivingEntity.java:2578-2579); before, travel does under a Slow Falling that falls (1.13.2
    // :1511-1513, 1.19.4 :1986-1990), and under Levitation after the air's move (below)
    let levitation = mob.levitation_amplifier.filter(|_| release.at_least(1, 9, 0));
    let falling = body.velocity.y <= 0.0;
    let slow_falling = mob.slow_falling && release.at_least(1, 13, 0);
    if release.at_least(1, 20, 0) {
        if slow_falling || levitation.is_some() {
            body.fall_distance = 0.0;
        }
    } else if slow_falling && falling {
        body.fall_distance = 0.0;
    }
    let gravity = effective_gravity(mob.gravity, mob.no_gravity, mob.slow_falling, body.velocity.y, release);
    // AIR_DRAG_MODIFIER and FRICTION_MODIFIER of 26.2 (Attributes.java:10, 71: 0 to 2048)
    let modified = release.at_least(26, 2, 0);
    let attribute_modifier =
        |modifier: Option<f64>| modifier.map_or(1.0, |modifier| sanitized(modifier, 0.0, 2048.0));
    let air_drag = attribute_modifier(mob.air_drag_modifier);
    let rules = MoveRules {
        living: true,
        max_step: step_height(mob, release),
        // getEffectiveGravity (26.2 Entity.java:887-889, 1532-1533)
        gravity,
        // LivingEntity.getAirDrag: the modified 0.98F (26.2 LivingEntity.java:2414-2416, 26.3
        // :2412-2414), which the bounce of 26.2 compensates
        air_drag: if modified {
            compute_modified_friction(f64::from(0.98_f32), air_drag)
        } else {
            f64::from(0.98_f32)
        },
        bounciness: 0.0,
        stands_on_lava: mob.kind == RiddenMobKind::Strider,
        // Strider.checkFallDamage checks the blocks inside first from 1.16 (the strider's first
        // release) until 1.21.1 (1.16 Strider.java:295-296, 1.21.1 :288-289; 1.21.2 :288-289 has
        // none)
        checks_inside_at_fall: mob.kind == RiddenMobKind::Strider
            && release.at_least(1, 16, 0)
            && !release.at_least(1, 21, 2),
        rails_keep_speed: false,
        ignored: [None, None],
        skips_restitution: false,
        counts_fall: false,
    };
    let yaw = yaw_radians(mob.yaw, release);
    let speed = f64::from(mob.ridden_speed);
    let mut input = input;
    // Camel.travel stops a camel that refuses to move on the ground before LivingEntity.travel
    // picks its fluid (1.19.3 Camel.java:222-231, 1.21.11 Camel.java:219-226, 26.3 :226-233)
    if mob.kind == RiddenMobKind::Camel && camel_refuses_to_move(mob) && body.on_ground {
        body.velocity = body.velocity.multiply(Vec3::new(0.0, 1.0, 0.0));
        input = input.multiply(Vec3::new(0.0, 1.0, 0.0));
    }
    if mob.kind == RiddenMobKind::HappyGhast {
        // HappyGhast.travel: travelFlying's own speed, not the mob's
        let flying = f64::from(mob.flying_speed as f32 * 5.0 / 3.0);
        body.velocity = body.velocity + relative_movement(input, flying, yaw, release);
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
        && world.fluid_at(block_position(body.position, release)) == FluidState::Lava;
    if (body.in_water || body.in_lava) && !stands_on_fluid {
        return Some(travel_in_fluid(mob, body, around, input, yaw, rules));
    }
    let friction = world.slipperiness_at(block_below_affecting_movement(world, body, support, release));
    // travelInAir's modified friction and drag of 26.2 (26.2 LivingEntity.java, 26.3 :2382-2407)
    let friction = if modified {
        compute_modified_friction(friction, attribute_modifier(mob.friction_modifier))
    } else {
        friction
    };
    let (horizontal_drag, vertical_drag) = if modified {
        (
            compute_modified_friction(f64::from(0.91_f32), air_drag),
            compute_modified_friction(f64::from(0.98_f32), air_drag),
        )
    } else {
        (f64::from(0.91_f32), f64::from(0.98_f32))
    };
    let inertia = if body.on_ground {
        f64::from(friction as f32 * horizontal_drag as f32)
    } else {
        horizontal_drag
    };
    // a FlyingAnimal keeps its climb as the rest of its motion from 1.20.5, by the block under it
    // as well until 26.1; from 26.2 by the air alone, on the ground too (1.20.4 LivingEntity.java:
    // 2161, 1.20.5 :2234, 1.21.11 :2330, 26.1 :2374, 26.2 :2408, 26.3 :2406)
    let vertical_drag = if !mob.plain.is_some_and(|plain| plain.omnidirectional) {
        vertical_drag
    } else if release.at_least(26, 2, 0) {
        horizontal_drag
    } else if release.at_least(1, 20, 5) {
        inertia
    } else {
        vertical_drag
    };
    let input_speed = if body.on_ground {
        friction_influenced_speed(speed, friction, release)
    } else {
        f64::from(air_speed)
    };
    body.velocity = body.velocity + relative_movement(input, input_speed, yaw, release);
    if on_climbable(mob, world, body, release) {
        let limit = f64::from(0.15_f32);
        body.fall_distance = 0.0;
        body.velocity.x = body.velocity.x.clamp(-limit, limit);
        body.velocity.z = body.velocity.z.clamp(-limit, limit);
        // (1.13.2 LivingEntity.java:1594-1596; 1.14.4 LivingEntity.java:1921-1923)
        let fall_limit = if release.at_least(1, 14, 0) {
            limit
        } else {
            0.15
        };
        body.velocity.y = body.velocity.y.max(-fall_limit);
    }
    let moved = move_mob(mob, body, world, release, rules);
    // from 1.21.2 travelInAir takes getEffectiveGravity after the move, so Slow Falling tests the
    // motion the move left, before the climb (1.21.3 LivingEntity.java:2194-2196, 2217-2222, 26.3
    // :2337-2340, 2382-2397); 1.21 the motion it began with (1.21 LivingEntity.java:2117-2121,
    // 1.21.1 LivingEntity.java:2117-2121, 2234; 1.21.2 :2194-2197, 2229)
    let gravity = if release.at_least(1, 21, 2) {
        effective_gravity(mob.gravity, mob.no_gravity, mob.slow_falling, body.velocity.y, release)
    } else {
        gravity
    };
    if body.collided_horizontally && on_climbable(mob, world, body, release) {
        body.velocity.y = 0.2;
    }
    // Levitation in place of gravity (1.9.4 LivingEntity.java:1417-1418, 1.13.2 :1610-1611, 26.3
    // :2387-2397), which resets the fall distance 1.13 - 1.19.4 (1.13.2 :1612, 1.19.4 :2105); no
    // gravity under NoGravity (1.12.2 LivingEntity.java:1547-1548, 1.16.5 :2082; from 1.20.5
    // getEffectiveGravity() of getGravity() 0, 1.21.11 LivingEntity.java:2351, Entity.java:1443-1445)
    if let Some(amplifier) = levitation {
        let target = 0.05 * (f64::from(amplifier) + 1.0);
        body.velocity.y += (target - body.velocity.y) * 0.2;
        if release.at_least(1, 13, 0) && !release.at_least(1, 20, 0) {
            body.fall_distance = 0.0;
        }
    } else if !body.no_gravity {
        body.velocity.y -= gravity;
    }
    // shouldDiscardFriction keeps the motion as it is from 1.17 (1.17 LivingEntity.java:2123-2124,
    // 636-638; 1.21.11 :2327-2328; 26.2 travelInAir)
    if mob.plain.is_some_and(|plain| plain.discard_friction) && release.at_least(1, 17, 0) {
        return Some(moved);
    }
    body.velocity.x *= inertia;
    body.velocity.z *= inertia;
    body.velocity.y *= vertical_drag;
    Some(moved)
}

/// LivingEntity.onClimbable for the mob: never for a horse or camel.
fn on_climbable(
    mob: &RiddenMobState,
    world: &WorldSnapshot,
    body: &SimulationState,
    release: MinecraftVersion,
) -> bool {
    if let Some(climbing) = mob.plain.and_then(|plain| plain.climbing) {
        return climbing;
    }
    mob.kind.climbs() && climbable(world, body, release)
}

/// LivingEntity.travel in water (a mob's water slow-down, 0.96F a skeleton horse's from 1.13) or
/// lava, with the jump out of it against a wall.
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
        water_height,
        lava_height,
        ..
    } = around;
    let old_y = body.position.y;
    let going_down = body.velocity.y <= 0.0;
    let in_water = body.in_water;
    let gravity = rules.gravity;
    // the skeleton horse's own 0.96F from 1.13 (1.12.2 LivingEntity.java:1445-1447 for every mob;
    // 1.13.2 SkeletonHorse.java:143-145), WATER_MOVEMENT_EFFICIENCY from 1.21 (halved off the
    // ground, 1.21 LivingEntity.java:2130-2137, 26.3 :2432-2443), Dolphin's Grace last from 1.13
    let own_slow_down = mob.plain.and_then(|plain| plain.water_slow_down);
    let mut slow_down = if let Some(own) = own_slow_down {
        own
    } else if mob.kind == RiddenMobKind::SkeletonHorse && release.at_least(1, 13, 0) {
        0.96_f32
    } else {
        0.8
    };
    let mut speed = 0.02_f32;
    if in_water {
        let efficiency = if release.at_least(1, 21, 0) {
            // (0 to 1, 26.3 Attributes.java:91)
            let attribute = sanitized(mob.water_movement_efficiency, 0.0, 1.0) as f32;
            attribute * if body.on_ground { 1.0 } else { 0.5 }
        } else {
            0.0
        };
        if efficiency > 0.0 {
            slow_down += (0.54600006 - slow_down) * efficiency;
            speed += (mob.ridden_speed - speed) * efficiency;
        }
        if mob.dolphins_grace && release.at_least(1, 13, 0) {
            slow_down = 0.96;
        }
    }
    body.velocity = body.velocity + relative_movement(input, f64::from(speed), yaw, release);
    let moved = move_mob(mob, body, world, release, rules);
    let (water_height, lava_height) = moved.fluid_heights.unwrap_or((water_height, lava_height));
    if in_water {
        // the climb out of water from 1.14 (1.13.2 LivingEntity.java:1646-1684 has none; 1.14.4
        // :1880-1882)
        if release.at_least(1, 14, 0)
            && body.collided_horizontally
            && on_climbable(mob, world, body, release)
        {
            body.velocity.y = 0.2;
        }
        let slow_down = f64::from(slow_down);
        body.velocity = body
            .velocity
            .multiply(Vec3::new(slow_down, 0.800000011920929, slow_down));
        // NoGravity keeps the vertical motion (1.12.2 LivingEntity.java:1571-1572, 1.13.2 :1673,
        // 1.16.5 :2120), as any gravity of 0 from 1.20.5 (1.21.11 getFluidFallingAdjustedMovement,
        // :2521-2533, 26.3 :2608-2620)
        if gravity != 0.0 {
            // 1.13 tests the motion after the move and the drag (1.13.2 LivingEntity.java:1672), from
            // 1.14 the motion before travel (`bl`, 1.14.4 LivingEntity.java:1760)
            let going_down = if release.at_least(1, 14, 0) { going_down } else { body.velocity.y <= 0.0 };
            body.velocity.y = if release.at_least(1, 13, 0) {
                fluid_falling_adjusted_y(gravity, 16.0, going_down, body.velocity.y)
            } else {
                body.velocity.y - 0.02
            };
        }
    } else {
        // lava halves the motion until 1.15.2 (1.15.2 LivingEntity.java:1944); from 1.16 a mob
        // the lava is no deeper on than its jump threshold falls as in water (1.16
        // LivingEntity.java:1996-2001)
        if release.at_least(1, 16, 0) && lava_height <= mob.fluid_jump_threshold(release) {
            body.velocity = body
                .velocity
                .multiply(Vec3::new(0.5, 0.800000011920929, 0.5));
            if gravity != 0.0 {
                body.velocity.y =
                    fluid_falling_adjusted_y(gravity, 16.0, going_down, body.velocity.y);
            }
        } else {
            body.velocity = body.velocity.scale(0.5);
        }
        // (1.12.2 LivingEntity.java:1602-1603, 1.16.5 :2018; 1.21.11 `d != 0.0`, :2388-2390; the
        // whole of Vec3.add: its 0.0 clears a wall's -0.0, 26.3 :2474)
        if gravity != 0.0 {
            body.velocity.y -= gravity / 4.0;
            // Vec3.add(0.0, -gravity / 4.0, 0.0) from 1.14: its 0.0 turns the -0.0 a wall's
            // restitution leaves from 26.2 into 0.0 (1.14.4 LivingEntity.java:1907-1908, 26.3 :2473-2474,
            // Entity.java:867-872)
            if release.at_least(1, 14, 0) {
                body.velocity.x += 0.0;
                body.velocity.z += 0.0;
            }
        }
    }
    let lifted = body.bounds().translated(Vec3::new(
        body.velocity.x,
        body.velocity.y + 0.6000000238418579 - body.position.y + old_y,
        body.velocity.z,
    ));
    // (Entity.isFree for the mob: its CollisionContext, colliders and the border)
    let context = entity_collision_context(body);
    let entity = collision_query_entity(body, &context);
    if body.collided_horizontally && world.is_free_for_entity(lifted, entity, release) {
        body.velocity.y = 0.30000001192092896;
    }
    // floatInWaterWhileRidden from 1.21.11: a horse, donkey, mule, zombie horse or camel
    // (EntityTypeTags.CAN_FLOAT_WHILE_RIDDEN) that is ridden rises 0.04F in water deeper than its
    // getFluidJumpThreshold (1.21.11 LivingEntity.java:2336-2342, 2404-2410,
    // EntityTypeTagsProvider.java:61-62; 1.21.10 has none); 26.3 runs it after the lava's
    // travel too, by the water's height (26.3 LivingEntity.java:2416-2427, 2488-2493)
    let floats = matches!(
        mob.kind,
        RiddenMobKind::Horse
            | RiddenMobKind::Donkey
            | RiddenMobKind::Mule
            | RiddenMobKind::ZombieHorse
            | RiddenMobKind::Camel
    );
    let after_this_fluid = in_water || release.at_least(26, 3, 0);
    if floats
        && after_this_fluid
        && release.at_least(1, 21, 11)
        && water_height > mob.fluid_jump_threshold(release)
    {
        // Vec3.add(0.0, 0.04F, 0.0): its 0.0 turns the -0.0 a wall's restitution leaves from 26.2
        // into 0.0 (26.3 LivingEntity.java:2490, Entity.java:867-872)
        body.velocity = body.velocity + Vec3::new(0.0, f64::from(0.04_f32), 0.0);
    }
    moved
}

/// Entity.checkInsideBlocks run outside a move: the entityInside effects of the blocks in the box.
fn check_inside_blocks(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let inside: Vec<_> = blocks_inside(body, release)
        .into_iter()
        .map(|position| (position, true))
        .collect();
    apply_inside_block_effects(body, world, release, &inside);
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
    if body.no_physics {
        // (Entity.move's way out, for a vex: 1.20.4 Entity.java:606-607)
        match body.bounding_box {
            Some(bounds) => {
                let moved = bounds.translated(velocity);
                body.bounding_box = Some(moved);
                body.position = position_of_bounding_box(moved);
            }
            None => body.position = body.position + velocity,
        }
        // (the collision flags go from 1.21.9: 1.21.8 Entity.java:706-707, 1.21.9 :713-718)
        if release.at_least(1, 21, 9) {
            body.collided_horizontally = false;
            body.collided_vertically = false;
            body.vertical_collision_below = false;
        }
        // (1.14.4 Vex.java, 1.21 :move; the override goes at 1.21.2: 1.21.1 Vex.java:66-69,
        // none in 1.21.2 Vex.java:68-74)
        if mob.plain.is_some_and(|plain| plain.inside_after_move) && !release.at_least(1, 21, 2) {
            let inside: Vec<_> = blocks_inside(body, release)
                .into_iter()
                .map(|position| (position, true))
                .collect();
            body.no_physics = false;
            apply_inside_block_effects(body, world, release, &inside);
            body.no_physics = true;
        }
        return VehicleMove {
            requested: velocity,
            moved: velocity,
            support: None,
            motion_at_landing: velocity,
            fluid_heights: None,
        };
    }
    // (a mob's fall counts before the blocks inside are checked, and a cobweb, bubble column or honey
    // there ends it: 1.20.4 Entity.java:759-766 checkFallDamage, then checkBlockCollision; 26.3 has no
    // such check in the move)
    let mut ended_inside = false;
    let mut moved = move_vehicle(body, world, release, velocity, rules, |body, support| {
        let inside: Vec<_> = blocks_inside(body, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        let fall = body.fall_distance;
        body.fall_distance = 1.0;
        apply_block_effects(body, world, support, release, &inside);
        ended_inside = body.fall_distance == 0.0;
        body.fall_distance = if body.fall_distance == 0.0 { 0.0 } else { fall };
    });
    // 1.16 - 1.16.1 Strider.fall runs checkBlockCollision first, which sets the inLava the move
    // has not yet reset (1.16 Strider.java:295-299, Entity.java:611, 660-662; 1.16.1
    // Strider.java:295-297, Entity.java:660-661, 1157-1162); from 1.16.2 isInLava is the base
    // tick's lava height (1.16.2 Strider.java:290-292, Entity.java:1178-1179)
    let strider_in_lava = || {
        body.in_lava
            || (release.at_least(1, 16, 0)
                && !release.at_least(1, 16, 2)
                && world.has_lava_above_feet_inside(body.bounds()))
    };
    let strider_lava = mob.kind == RiddenMobKind::Strider && strider_in_lava();
    // (1.21.4 alone: Entity.move skips checkFallDamage, and so its water refresh, for what a player
    // controls, 1.21.4 Entity.java:721; 1.21.5 tests isLocalInstanceAuthoritative, true for the
    // local rider's mount)
    let controlled_1_21_4 = mob.plain.is_none() && release.at_least(1, 21, 4) && !release.at_least(1, 21, 5);
    let no_fall_check = mob.plain.is_some_and(|plain| plain.no_fall_check);
    let skips = mob.kind == RiddenMobKind::HappyGhast || strider_lava || controlled_1_21_4 || no_fall_check;
    if !skips && !body.in_water && legacy_fluid::applies(release) {
        // LivingEntity.fall's updateWaterState before 1.13 (1.9.4 LivingEntity.java:247-249,
        // 1.12.2 :274-276)
        legacy_fluid::handle_water_movement(body, world, true, release);
    } else if !skips && !body.in_water {
        let pushed = mob
            .plain
            .map_or(!mob.kind.is_nautilus(), |plain| plain.pushed_by_fluid);
        let water = water_contact(world, body.bounds(), release);
        if water.touching {
            body.in_water = true;
            body.fall_distance = 0.0;
            if pushed {
                push_by_fluid(body, water, 0.014, release);
            }
        }
        // (1.21.11 LivingEntity.java:321-323; 26.1 LivingEntity.java:311-313)
        if release.at_least(26, 1, 0) {
            // LivingEntity.checkFallDamage's updateFluidInteraction: the heights the travel reads
            // after the move are those of where the move left the mob (26.1 LivingEntity.java:
            // 310-313, 26.3 :326-328; 1.21.11 :320-324 refreshes only the water state)
            let lava = world.fluid_contact(FluidState::Lava, body.bounds(), release);
            moved.fluid_heights = Some((water.height, lava.height));
            body.in_lava = lava.touching;
            if pushed && lava.touching {
                push_by_fluid(body, lava, lava_flow_scale(mob.ultra_warm), release);
            }
        }
    }
    // Entity.checkFallDamage after LivingEntity's water refresh: a mob's fall distance grows by the
    // drop while it is in the air and is reset when it lands (1.17 Entity.java:1062-1070, 1.20.4
    // Entity.java:1121-1132; 1.21.5 counts the drop first, then lands: Entity.java:1321-1331)
    // (a strider in lava lands instead, Strider.checkFallDamage -> resetFallDistance, 1.20.4
    // Strider.java:297-304; a happy ghast, a flying mob, keeps no fall at all)
    if controlled_1_21_4 {
    } else if strider_lava {
        body.fall_distance = 0.0;
    } else if mob.kind != RiddenMobKind::HappyGhast && !body.no_physics && !no_fall_check {
        let dy = moved.moved.y;
        // (not in water: 1.21.5 Entity.java:1322, 1.21.11 :1473, as the player's)
        if release.at_least(1, 21, 5) && dy < 0.0 && !body.in_water {
            body.fall_distance = accumulated_fall_distance(body.fall_distance, dy, release);
        }
        if body.on_ground {
            body.fall_distance = 0.0;
        } else if !release.at_least(1, 21, 5) && dy < 0.0 {
            body.fall_distance = accumulated_fall_distance(body.fall_distance, dy, release);
        }
    }
    if ended_inside {
        body.fall_distance = 0.0;
    }
    // (1.13.2 Entity.java:1143-1145; 1.14.4 Entity.java:568-569, 826-848; LiquidBlock.java:158-161)
    // the move's isTouchingLava until 1.16.1 (1.16.1 Entity.java:660-661, 1161-1162), the base
    // tick's lava height from 1.16.2 (1.16.2 Entity.java:1178-1179)
    if release.at_least(1, 14, 0) && !release.at_least(1, 16, 2) {
        body.in_lava = if release.at_least(1, 16, 0) {
            world.has_lava_above_feet_inside(body.bounds())
        } else {
            world.has_fluid_block_inside(FluidState::Lava, body.bounds())
        };
    // (1.13.2 Entity.java:1143-1145; 1.14.4 Entity.java:568-569, 1171-1173)
    } else if !release.at_least(1, 14, 0) {
        // (no field before 1.14: Entity.isInLava asks the world, 1.9.4 Entity.java:1031-1033, 1.13.2
        // :1143-1145, 1.14.4 :1171-1173, so the tick ends with the lava of where the move left it)
        body.in_lava = legacy_fluid::touching_lava(world, body.bounds(), release);
    }
    moved
}

/// The mob's own tick after its travel: a horse that landed drops its pending jump before 1.19.4,
/// its rearing ends after 20 ticks where the client runs it (counted up until 1.21.6, down
/// since), and a horse that does not rear no longer lets the rider's jump move it; a nautilus's
/// and camel's dash ends and its cooldown runs down; a strider in lava stands on its surface,
/// still or flowing, or floats up (Strider.floatStrider), and checks the blocks inside before
/// 1.21.2.
fn after_ridden_travel(
    mob: &mut RiddenMobState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    if mob.kind.is_horse() {
        // (1.19.3 AbstractHorse.java:771; 1.19.4 AbstractHorse.java:767)
        if !release.at_least(1, 19, 4) && body.on_ground {
            mob.jump_pending_scale = 0.0;
            mob.horse_jumping = false;
        }
        // (1.21.5 AbstractHorse.java:562-565 counts up to 20; 1.21.6 AbstractHorse.java:567-569
        // counts down to 0)
        if release.at_least(1, 21, 6) {
            if mob.stand_counter > 0 {
                mob.stand_counter = mob.stand_counter.wrapping_sub(1);
                if mob.stand_counter <= 0 {
                    mob.stand_counter = 0;
                    mob.standing = false;
                }
            }
        } else if horse_stands_locally(release) && mob.stand_counter > 0 {
            mob.stand_counter = mob.stand_counter.wrapping_add(1);
            if mob.stand_counter > 20 {
                mob.stand_counter = 0;
                mob.standing = false;
            }
        }
        if !mob.standing {
            mob.allow_stand_sliding = false;
        }
    }
    // a camel is an AbstractHorse too, whose tick drops the stand sliding of one not rearing (a
    // camel never rears): 1.20.4 AbstractHorse.java:645-654
    if mob.kind == RiddenMobKind::Camel && !mob.standing {
        mob.allow_stand_sliding = false;
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
        // Camel.tick stands a sitting camel in water up at once, its pose time that of a full
        // stand: getGameTime() - (getGameTime() - 52 - 1) (1.20 Camel.java:190-191, CamelEntity.java:
        // 187-189; 26.3 Camel.java:182-184, 553-566; none in 1.19.4 Camel.java:185-187)
        if release.at_least(1, 20, 0) && mob.sitting && body.in_water {
            mob.sitting = false;
            mob.pose_time = 53;
            body.height = f64::from(mob.dimensions(release).1);
        }
        mob.pose_time += 1;
    }
    if mob.kind == RiddenMobKind::Strider && body.in_lava {
        // CollisionContext.isAbove(the half-high stable shape) and no lava above: flowing lava
        // holds it as a source does, the source test being LiquidBlock.getCollisionShape's
        // (1.16 Strider.java:334-343, LiquidBlock.java:36; 1.21.11 Strider.java:313-323;
        // 26.3 :322-336, EntityCollisionContext.java:72-74)
        let (x, y, z) = block_position(body.position, release);
        let above_surface = body.position.y > f64::from(y) + 0.5 - 9.999999747378752e-6;
        if above_surface && world.fluid_at((x, y + 1, z)) != FluidState::Lava {
            body.on_ground = true;
        } else {
            body.velocity = body.velocity.scale(0.5) + Vec3::new(0.0, 0.05, 0.0);
        }
    }
    if mob.kind == RiddenMobKind::Strider
        && release.at_least(1, 16, 0)
        && !release.at_least(1, 21, 2)
    {
        // Strider.tick checks the blocks inside once more after floatStrider, until 1.21.2 (1.16
        // Strider.java:318, 1.20.4 :336, 1.21.1 Strider.java:329; 1.21.2 has none)
        check_inside_blocks(body, world, release);
    }
}

/// Whether the client runs the ridden horse's rearing (AbstractHorse.standIfPossible and its
/// counter): before 1.19.4 and from 1.21.5, where the client runs the AI of the mob it moves
/// (1.19.3 AbstractHorse.java:601 isControlledByLocalInstance; 1.19.4 :613 isEffectiveAi alone,
/// !isClientSide, Entity.java:2831-2833; 1.21.4 Entity.java:3244-3246; 1.21.5 Entity.java:3242-3244
/// isLocalInstanceAuthoritative, AbstractHorse.java:562).
fn horse_stands_locally(release: MinecraftVersion) -> bool {
    !release.at_least(1, 19, 4) || release.at_least(1, 21, 5)
}

/// The riding jump the rider released (onPlayerJump, in percent): a horse lets the jump move it
/// rearing and rears where the client runs it, and jumps at its next tick on the ground; a
/// nautilus or camel with its cooldown run out (a camel on the ground, saddled and neither sitting
/// nor changing pose) dashes at its next tick.
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
                // (isEffectiveAi: a mob without AI does not rear, 1.21.5 Entity.java:3242-3244,
                // AbstractHorse.java:674-680, 26.3 :680-684)
                if horse_stands_locally(release)
                    && kind.rears()
                    && !(mob.no_ai && release.at_least(1, 21, 5))
                {
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
            // LocalPlayer.jumpableVehicle holds a camel only while Camel.canJump: not sitting or
            // changing pose, and saddled (1.19.3 Camel.java:254-256, 1.20 :272-274,
            // 1.21.11 Camel.java:253-255, LocalPlayer.java:539-543, 816-827)
            // It runs in the client tick the camel's tickRidden ran in, on the same game time
            // (Minecraft.java:1798, 1823; ClientLevel.tickTime :298-299 steps it after): the
            // pose time before after_ridden_travel's step
            let this_tick = mob.pose_time - 1;
            let in_transition = this_tick < if mob.sitting { 40 } else { 52 };
            let can_jump = mob.saddled && !mob.sitting && !in_transition;
            if can_jump && mob.dash_cooldown <= 0 && mob.on_ground {
                // Camel.setJumpStrength / onPlayerJump goes on to AbstractHorse's, which lets the
                // jump slide a standing horse (1.20.4 Camel.java:274-278,
                // AbstractHorse.java:924-939; 1.21.5 Camel.java:256-260)
                if power >= 0 {
                    mob.allow_stand_sliding = true;
                }
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
    use crate::simulation::{friction_influenced_speed, mth_sin};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

    const V1_16_5: MinecraftVersion = MinecraftVersion::new(1, 16, 5);
    const V1_21_1: MinecraftVersion = MinecraftVersion::new(1, 21, 1);
    const V26_2: MinecraftVersion = MinecraftVersion::new(26, 2, 0);

    #[test]
    fn a_zombie_horse_keeps_the_horse_base_speed_from_1_21_11() {
        let zombie = RiddenMobKind::ZombieHorse;
        assert_eq!(zombie.movement_speed_in(MinecraftVersion::new(1, 21, 10)), f64::from(0.2_f32));
        assert_eq!(zombie.movement_speed_in(MinecraftVersion::new(1, 21, 11)), f64::from(0.225_f32));
        assert_eq!(zombie.movement_speed_in(MinecraftVersion::new(26, 3, 0)), f64::from(0.225_f32));
        let skeleton = RiddenMobKind::SkeletonHorse;
        assert_eq!(skeleton.movement_speed_in(MinecraftVersion::new(26, 3, 0)), f64::from(0.2_f32));
    }

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
    fn a_horse_jump_reads_living_entity_jumping_from_1_21_6() {
        const V1_21_5: MinecraftVersion = MinecraftVersion::new(1, 21, 5);
        const V1_21_6: MinecraftVersion = MinecraftVersion::new(1, 21, 6);
        let world = stone_floor((-3, 4), 100, (2170, 2177));
        let charged = |jumping: bool| {
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
            horse.jump_pending_scale = 1.0;
            horse.horse_jumping = jumping;
            horse
        };

        // the horse's own flag: cleared on the ground, then set by the jump
        let mut horse = charged(true);
        tick_ridden_mob(&mut horse, &world, V1_21_5);
        assert!(horse.position.y > 101.0);
        assert!(horse.horse_jumping);

        // LivingEntity.jumping: the jump leaves it alone
        let mut horse = charged(false);
        tick_ridden_mob(&mut horse, &world, V1_21_6);
        assert!(horse.position.y > 101.0);
        assert!(!horse.horse_jumping);

        // and it holds the charged jump back, which is still spent
        let mut horse = charged(true);
        tick_ridden_mob(&mut horse, &world, V1_21_6);
        assert_eq!(horse.position.y, 101.0);
        assert!(horse.horse_jumping);
        assert_eq!(horse.jump_pending_scale, 0.0);
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
    fn lava_beside_the_water_a_mob_is_in_does_not_push_it_in_1_16_and_1_16_1() {
        // 1.16 Entity.updateInWaterStateAndDoFluidPushing returns once touching water, before the lava (1.16
        // Entity.java:1039-1048); 1.16.2 pushes by both (1.16.5 Entity.java:1043-1049)
        let pushed = |release| {
            let mut world = fluid_pool(FluidState::Water, (-3, 4), (98, 100), (1934, 1941));
            world.set_block(
                (1, 99, 1937),
                BlockState {
                    fluid: FluidState::Lava,
                    fluid_level: 2,
                    fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                    ..BlockState::default()
                },
            );
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 99.0, 1937.5),
                Vec3::ZERO,
                270.0,
            );
            tick_ridden_mob(&mut horse, &world, release);
            assert!(horse.in_water, "{release}");
            horse.velocity.x
        };

        assert_eq!(pushed(MinecraftVersion::new(1, 16, 1)), 0.0);
        assert!(pushed(MinecraftVersion::new(1, 16, 2)) > 0.0);
    }

    #[test]
    fn a_strider_stays_warm_the_tick_after_its_box_left_the_lava_beside_it_in_1_16_1() {
        // Strider.tick reads getFluidHeight(LAVA) of the base tick before (1.16
        // Strider.java:312-315), not the inLava its move clears off the lava (1.16
        // Entity.java:660-662)
        let release = MinecraftVersion::new(1, 16, 1);
        let mut world = stone_floor((-3, 4), 98, (1934, 1941));
        world.set_block(
            (1, 99, 1937),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let mut strider = ridden(
            RiddenMobKind::Strider,
            Vec3::new(0.6, 99.0, 1937.5),
            Vec3::new(-0.3, 0.0, 0.0),
            270.0,
        );
        strider.on_ground = true;

        tick_ridden_mob(&mut strider, &world, release);
        assert!(strider.position.x + 0.45 < 1.0);
        assert!(!strider.in_lava);
        assert!(strider.lava_height_positive);
        tick_ridden_mob(&mut strider, &world, release);
        assert!(!strider.suffocating);
        assert!(!strider.lava_height_positive);
        tick_ridden_mob(&mut strider, &world, release);
        assert!(strider.suffocating);
    }

    #[test]
    fn a_striders_lava_beside_the_water_it_is_in_is_not_measured_in_1_16_1() {
        // in water 1.16 Entity.updateInWaterStateAndDoFluidPushing returns before the lava, whose height stays 0
        // (1.16 Entity.java:1039-1048), so the strider is cold the next tick
        let mut world = fluid_pool(FluidState::Water, (-3, 4), (99, 99), (1934, 1941));
        for x in -3..=4 {
            for z in 1934..=1941 {
                world.set_block(
                    (x, 98, z),
                    BlockState {
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
            }
        }
        world.set_block(
            (1, 99, 1937),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let warm_after = |release| {
            let mut strider = ridden(
                RiddenMobKind::Strider,
                Vec3::new(0.6, 99.0, 1937.5),
                Vec3::ZERO,
                270.0,
            );
            strider.on_ground = true;
            tick_ridden_mob(&mut strider, &world, release);
            assert!(strider.in_water, "{release}");
            tick_ridden_mob(&mut strider, &world, release);
            !strider.suffocating
        };

        assert!(!warm_after(MinecraftVersion::new(1, 16, 1)));
        assert!(warm_after(MinecraftVersion::new(1, 16, 2)));
    }

    #[test]
    fn a_strider_entering_lava_skips_the_water_check_in_1_16_1() {
        // Strider.fall runs checkBlockCollision, which sets inLava for the lava over its feet,
        // before Entity.move resets it, and skips the fall's water check in lava (1.16
        // Strider.java:295-299, Entity.java:611, 660-662, LiquidBlock.java:173-180)
        let release = MinecraftVersion::new(1, 16, 1);
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 99, 1937),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        world.set_block(
            (1, 99, 1937),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let after = |kind| {
            let mut mob = ridden(
                kind,
                Vec3::new(1.0, 100.2, 1937.5),
                Vec3::new(0.0, -0.5, 0.0),
                270.0,
            );
            tick_ridden_mob(&mut mob, &world, release);
            assert!(mob.position.y < 99.8, "{kind:?}");
            (mob.in_water, mob.in_lava)
        };

        assert_eq!(after(RiddenMobKind::Strider), (false, true));
        assert_eq!(after(RiddenMobKind::Pig), (true, true));
    }

    #[test]
    fn a_mob_dropping_into_lava_meets_the_depth_its_move_reached_from_26_1() {
        // LivingEntity.checkFallDamage refreshes every fluid from 26.1 (26.1 LivingEntity.java:
        // 310-313), the water alone before (1.21.11 :320-324), and travelInLava then reads
        // isInShallowFluid (26.3 LivingEntity.java:2465): recorded in physrec/v_horse_into_lava
        let mut world = stone_floor((-4, 4), 95, (-4, 4));
        for x in -4..=4 {
            for z in -4..=4 {
                for y in 96..=99 {
                    world.set_block(
                        (x, y, z),
                        BlockState {
                            fluid: FluidState::Lava,
                            ..BlockState::default()
                        },
                    );
                }
            }
        }
        let fall = |release: MinecraftVersion| {
            let mut horse = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 99.8, 0.5));
            horse.in_lava = true;
            horse.velocity.y = -0.7;
            tick_ridden_mob(&mut horse, &world, release);
            assert!(horse.position.y < 99.2, "{release}");
            horse.velocity.y
        };
        let deep = -0.7 * 0.5 - 0.08 / 4.0;
        assert!((fall(MinecraftVersion::new(26, 1, 0)) - deep).abs() < 1.0e-9);
        assert!((fall(MinecraftVersion::new(26, 3, 0)) - deep).abs() < 1.0e-9);
        // (the shallow branch, by the height the tick began with)
        assert!(fall(MinecraftVersion::new(1, 21, 11)) < -0.5);
    }

    #[test]
    fn lava_halves_a_ridden_mobs_fall_distance() {
        let world = fluid_pool(FluidState::Lava, (-3, 4), (98, 100), (1934, 1941));
        let mut horse = ridden(
            RiddenMobKind::Horse,
            Vec3::new(0.5, 99.0, 1937.5),
            Vec3::ZERO,
            270.0,
        );
        horse.fall_distance = 4.0;

        tick_ridden_mob(&mut horse, &world, V1_21_1);

        // Entity.baseTick halves it in lava (1.21.4 Entity.java:514); the still horse adds no drop
        assert!(horse.in_lava);
        assert_eq!(horse.position.y, 99.0);
        assert_eq!(horse.fall_distance, 2.0);
        tick_ridden_mob(&mut horse, &world, V1_21_1);
        // halved again, then the move's drop added while the horse sinks in the air of the pool
        // (Entity.checkFallDamage: fallDistance -= dy off the ground, float before 1.21.5)
        let drop = horse.position.y - 99.0;
        assert!(drop < 0.0 && !horse.on_ground);
        assert_eq!(horse.fall_distance, f64::from(1.0_f32 - drop as f32));
    }

    #[test]
    fn a_landed_ridden_mob_has_no_fall_distance() {
        let world = stone_floor((-3, 4), 100, (1934, 1941));
        let mut horse = ridden(
            RiddenMobKind::Horse,
            Vec3::new(0.5, 101.0, 1937.5),
            // (resting, as a mob on the ground does: the last tick's gravity)
            Vec3::new(0.0, -0.0784000015258789, 0.0),
            270.0,
        );
        horse.fall_distance = 4.0;

        tick_ridden_mob(&mut horse, &world, V1_21_1);

        // on the floor the move lands it: Entity.checkFallDamage -> resetFallDistance
        assert!(horse.on_ground);
        assert_eq!(horse.fall_distance, 0.0);
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
    fn a_nautilus_diving_in_water_keeps_no_fall_distance() {
        let world = fluid_pool(FluidState::Water, (-3, 4), (90, 100), (3298, 3305));
        let mut nautilus = ridden(
            RiddenMobKind::Nautilus,
            Vec3::new(0.5, 97.0, 3301.5),
            Vec3::ZERO,
            -90.0,
        );
        nautilus.rider.pitch = 40.0;
        nautilus.rider.forward = 0.98;

        for _ in 0..4 {
            tick_ridden_mob(&mut nautilus, &world, V26_2);
        }
        assert!(nautilus.position.y < 97.0);
        assert_eq!(nautilus.fall_distance, 0.0);
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

    #[test]
    fn a_ladder_holds_neither_a_ridden_horse_nor_a_camel() {
        // AbstractHorse.onClimbable is false (1.12.2 AbstractHorse.java:963, 1.21.11
        // AbstractHorse.java:940, 26.3 :920); a pig's LivingEntity.onClimbable keeps its fall
        // to 0.15F (1.21.1 LivingEntity.java handleOnClimbable)
        use crate::world::BlockEffect;
        let mut ladder = WorldSnapshot::new();
        ladder.set_block(
            (0, 105, 2173),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let empty = WorldSnapshot::new();
        let fallen = |kind, world: &WorldSnapshot, release| {
            let mut mob = ridden(
                kind,
                Vec3::new(0.5, 105.0, 2173.5),
                Vec3::new(0.0, -0.5, 0.0),
                270.0,
            );
            tick_ridden_mob(&mut mob, world, release);
            mob
        };
        for release in [MinecraftVersion::new(1, 12, 2), V1_21_1, V26_2] {
            for kind in [
                RiddenMobKind::Horse,
                RiddenMobKind::Llama,
                RiddenMobKind::Camel,
            ] {
                let on_ladder = fallen(kind, &ladder, release);
                let in_air = fallen(kind, &empty, release);
                assert_eq!(on_ladder.position, in_air.position, "{kind:?} {release}");
                assert_eq!(on_ladder.velocity, in_air.velocity, "{kind:?} {release}");
            }
        }
        let pig = fallen(RiddenMobKind::Pig, &ladder, V1_21_1);
        assert_eq!(pig.position.y, 105.0 - f64::from(0.15_f32));
    }

    #[test]
    fn a_camel_that_sits_or_stands_up_takes_no_riding_jump() {
        // LocalPlayer.jumpableVehicle is null unless Camel.canJump: !refuseToMove() (sitting or in
        // its pose transition) && isSaddled() (1.19.3 Camel.java:254-256, 1.20 :272-274,
        // 1.21.11 Camel.java:236-238, 253-255, LocalPlayer.java:539-543, 816-827)
        let world = stone_floor((0, 12), 100, (2170, 2177));
        let released = |sitting: bool, pose_time: i64, saddled: bool| {
            let mut camel = ridden(
                RiddenMobKind::Camel,
                Vec3::new(3.5, 101.0, 2173.5),
                Vec3::new(0.0, -0.0784000015258789, 0.0),
                270.0,
            );
            camel.on_ground = true;
            camel.sitting = sitting;
            camel.pose_time = pose_time;
            camel.saddled = saddled;
            camel.rider.released_jump = Some(90);
            tick_ridden_mob(&mut camel, &world, V26_2);
            camel.jump_pending_scale
        };
        assert_eq!(released(false, 1000, true), 1.0);
        // standing up for 52 ticks, sitting down for 40, sitting
        assert_eq!(released(false, 10, true), 0.0);
        assert_eq!(released(false, 50, true), 0.0);
        // LocalPlayer.aiStep's canJump reads the getPoseTime the camel's tick read (1.21.11
        // LocalPlayer.java:539-543, 816; Minecraft.java:1798, 1823; ClientLevel.java:298-299):
        // 51 is still standing up (Camel.isInPoseTransition, 51 < 52), 52 is done
        assert_eq!(released(false, 51, true), 0.0);
        assert_eq!(released(false, 52, true), 1.0);
        assert_eq!(released(true, 1000, true), 0.0);
        assert_eq!(released(false, 1000, false), 0.0);
    }

    #[test]
    fn a_ridden_mob_with_no_gravity_takes_no_gravity_in_air_water_or_lava() {
        // LivingEntity.travel skips gravity under NoGravity: in the air (1.12.2
        // LivingEntity.java:1547-1548, 1.16.5 :2082), in water (1.12.2 :1571-1572, 1.16.5 :2120)
        // and in lava (1.12.2 :1600-1601, 1.16.5 :2018); from 1.20.5 getGravity() is 0 (1.21.11
        // Entity.java:1443-1445, LivingEntity.java:2317, 2387-2390, 2521-2533)
        for release in [MinecraftVersion::new(1, 12, 2), MinecraftVersion::new(1, 16, 5), V1_21_1] {
            let world = WorldSnapshot::new();
            let mut horse = ridden(RiddenMobKind::Horse, Vec3::new(0.5, 100.0, 0.5), Vec3::ZERO, 270.0);
            horse.no_gravity = true;
            tick_ridden_mob(&mut horse, &world, release);
            assert_eq!(horse.velocity.y, 0.0, "{release}");
            assert_eq!(horse.position.y, 100.0, "{release}");

            for fluid in [FluidState::Water, FluidState::Lava] {
                let world = fluid_pool(fluid, (-2, 2), (98, 101), (-2, 2));
                let mut mob = ridden(RiddenMobKind::Horse, Vec3::new(0.5, 99.0, 0.5), Vec3::ZERO, 270.0);
                mob.no_gravity = true;
                tick_ridden_mob(&mut mob, &world, release);
                assert_eq!(mob.velocity.y, 0.0, "{release} {fluid:?}");
                let mut falling = ridden(RiddenMobKind::Horse, Vec3::new(0.5, 99.0, 0.5), Vec3::ZERO, 270.0);
                tick_ridden_mob(&mut falling, &world, release);
                assert!(falling.velocity.y < 0.0, "{release} {fluid:?}");
            }
        }
    }

    #[test]
    fn a_no_gravity_mob_bounces_off_a_slow_landing_from_26_2() {
        // Entity.restituteMovementAfterCollisions bounces a landing unless -currentMovement.y <
        // getEffectiveGravity(), which is 0 under NoGravity, and compensates
        // portionWithMovement * getEffectiveGravity() (26.2 Entity.java:848, 855, 887-889,
        // 1532-1533): a NoGravity horse landing at 0.05 a tick bounces, and with no gravity
        // compensation; a falling one lands, its 0.05 under the 0.08 threshold
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 99, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 0.75,
                ..BlockState::default()
            },
        );
        let landing = |no_gravity: bool| {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 100.03, 0.5),
                Vec3::new(0.0, -0.05, 0.0),
                270.0,
            );
            horse.no_gravity = no_gravity;
            tick_ridden_mob(&mut horse, &world, V26_2);
            horse
        };
        let floating = landing(true);
        assert_eq!(floating.position.y, 100.0);
        assert!(floating.velocity.y > 0.0, "{}", floating.velocity.y);
        let falling = landing(false);
        assert_eq!(falling.position.y, 100.0);
        assert!(falling.velocity.y <= 0.0, "{}", falling.velocity.y);
    }

    #[test]
    fn a_strider_above_flowing_lava_stands_on_it() {
        // Strider.floatStrider: CollisionContext.isAbove(the half-high shape) and no lava above,
        // with no test of the lava's level (1.16 Strider.java:334-343, 1.21.11
        // Strider.java:313-323, 26.3 :322-331)
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in 1936..=1938 {
                world.set_block(
                    (x, 100, z),
                    BlockState {
                        fluid: FluidState::Lava,
                        fluid_level: 1,
                        ..BlockState::default()
                    },
                );
            }
        }
        let mut strider = ridden(
            RiddenMobKind::Strider,
            Vec3::new(0.5, 100.6, 1937.5),
            Vec3::ZERO,
            270.0,
        );

        tick_ridden_mob(&mut strider, &world, V1_21_1);

        assert!(strider.in_lava && strider.position.y > 100.5);
        assert!(strider.on_ground);
        assert_eq!(strider.velocity.y, -0.08 * f64::from(0.98_f32));
    }

    #[test]
    fn lava_pushes_a_ridden_mob_at_0_007_in_an_ultrawarm_dimension() {
        // Entity.updateFluidHeightAndDoFluidPushing: 0.007 where the dimension is ultrawarm
        // (FAST_LAVA), else 0.0023333333333333335, lifted to 0.0045 for a still entity (1.16
        // Entity.java:1046-1067, 1.21.4 :1364, 1.21.11 :1561); aiStep then fades it by 0.98
        let mut world = stone_floor((-1, 1), 99, (1936, 1938));
        for x in -1..=1 {
            for z in 1936..=1938 {
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
        let moved = |ultra_warm: bool| {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 100.0, 1937.5),
                Vec3::ZERO,
                270.0,
            );
            horse.on_ground = true;
            horse.ultra_warm = ultra_warm;
            tick_ridden_mob(&mut horse, &world, V1_21_1);
            assert!(horse.in_lava);
            horse.position.x
        };

        assert_eq!(moved(true), 0.5 + 0.007 * 0.98);
        assert_eq!(moved(false), 0.5 + 0.0045000000000000005 * 0.98);
    }

    #[test]
    fn a_ridden_mob_takes_the_legacy_water_scan_before_1_13() {
        // Entity.handleWaterMovement before 1.13: the box shrunk by 0.4F vertically and 0.001 all
        // round meets the layers from y 100, whose water is touched where the cells top (ceil
        // maxY, 102) reaches its surface (1.12.2 Entity.java:1043, Level.java:1616-1655); from 1.13
        // its surface (100.333) must reach the box bottom (1.13.2 Entity.java:2511-2551)
        let mut world = WorldSnapshot::new();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block(
                    (x, 100, z),
                    BlockState {
                        fluid: FluidState::Water,
                        fluid_level: 5,
                        ..BlockState::default()
                    },
                );
            }
        }
        let in_water = |release: MinecraftVersion| {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 100.5, 0.5),
                Vec3::ZERO,
                0.0,
            );
            tick_ridden_mob(&mut horse, &world, release);
            horse.in_water
        };
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 12, 2),
        ] {
            assert!(in_water(release), "{release}");
        }
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 14, 0),
        ] {
            assert!(!in_water(release), "{release}");
        }
    }

    #[test]
    fn a_ridden_mob_moved_into_water_is_not_in_it_in_1_21_4_alone() {
        // the refresh of checkFallDamage, which 1.21.4 skips for what a player controls (1.21.4
        // Entity.java:721, LivingEntity.checkFallDamage): physrec/v_horse_into_water
        let mut world = stone_floor((-4, 4), 95, (-4, 4));
        for x in -4..=4 {
            for z in -4..=4 {
                for y in 96..=99 {
                    world.set_block(
                        (x, y, z),
                        BlockState {
                            fluid: FluidState::Water,
                            fluid_source: true,
                            ..BlockState::default()
                        },
                    );
                }
            }
        }
        let entered = |release: MinecraftVersion| {
            let mut horse = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 100.4, 0.5));
            horse.velocity.y = -0.9;
            tick_ridden_mob(&mut horse, &world, release);
            horse.in_water
        };
        assert!(entered(MinecraftVersion::new(1, 21, 3)));
        assert!(!entered(MinecraftVersion::new(1, 21, 4)));
        assert!(entered(MinecraftVersion::new(1, 21, 5)));
    }

    #[test]
    fn a_mob_moved_into_lava_is_in_it_at_once_before_1_14() {
        // Entity.isInLava asks the world before 1.14 (1.9.4 Entity.java:1031-1033, 1.13.2
        // :1143-1145) and reads the base tick's field from it (1.14.4 :1171-1173): physrec/v_horse_into_lava
        let mut world = stone_floor((-4, 4), 95, (-4, 4));
        for x in -4..=4 {
            for z in -4..=4 {
                for y in 96..=99 {
                    world.set_block(
                        (x, y, z),
                        BlockState {
                            fluid: FluidState::Lava,
                            fluid_source: true,
                            ..BlockState::default()
                        },
                    );
                }
            }
        }
        let entered = |release: MinecraftVersion| {
            let mut horse = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 100.4, 0.5));
            horse.velocity.y = -0.9;
            tick_ridden_mob(&mut horse, &world, release);
            horse.in_lava
        };
        assert!(entered(MinecraftVersion::new(1, 9, 4)));
        assert!(entered(MinecraftVersion::new(1, 13, 2)));
        // (1.8 interpolates the mob the server moves: 1.8.8 Entity.java:922-924)
        let mut interpolated = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 100.4, 0.5));
        interpolated.lerp_position = Some(Vec3::new(0.5, 99.5, 0.5));
        tick_ridden_mob(&mut interpolated, &world, MinecraftVersion::new(1, 8, 8));
        assert!(interpolated.in_lava);
        assert!(!entered(MinecraftVersion::new(1, 16, 5)));
    }

    #[test]
    fn a_ridden_mob_is_in_lava_by_its_box_shrunk_by_0_1_before_1_14() {
        // Entity.isInLava: the box shrunk by 0.1F sideways and 0.4F vertically (1.9.4
        // Entity.java:1032, 1.12.2 :1129, 1.13.2 :1144): lava 0.048 into the box is outside it
        let mut world = stone_floor((-2, 2), 100, (-2, 2));
        for z in -1..=1 {
            world.set_block(
                (1, 101, z),
                BlockState {
                    fluid: FluidState::Lava,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
        }
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 13, 2),
        ] {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.35, 101.0, 0.5),
                Vec3::ZERO,
                0.0,
            );
            horse.on_ground = true;
            tick_ridden_mob(&mut horse, &world, release);
            assert!(!horse.in_lava, "{release}");
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.45, 101.0, 0.5),
                Vec3::ZERO,
                0.0,
            );
            horse.on_ground = true;
            tick_ridden_mob(&mut horse, &world, release);
            assert!(horse.in_lava, "{release}");
        }
    }

    #[test]
    fn a_ridden_mob_is_not_moved_on_the_1_8_client() {
        // LivingEntity.travel moves only where canMoveVoluntarily, not on the client (1.8.8
        // LivingEntity.java:1134, 1408-1412, 1556-1558; AbstractHorse.travel turns the horse to its
        // rider and moves it off the client alone, :1061-1103): its motion fades by 0.98
        let world = stone_floor((-3, 3), 100, (-3, 3));
        let ticked = |release: MinecraftVersion| {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::new(0.1, 0.0, 0.0),
                0.0,
            );
            horse.on_ground = true;
            horse.rider.forward = 0.98;
            horse.rider.pitch = 30.0;
            horse.rider.released_jump = Some(90);
            tick_ridden_mob(&mut horse, &world, release);
            horse
        };
        let horse = ticked(MinecraftVersion::new(1, 8, 8));
        assert_eq!(horse.position, Vec3::new(0.5, 101.0, 0.5));
        assert_eq!(horse.velocity, Vec3::new(0.1 * 0.98, 0.0, 0.0));
        assert_eq!((horse.yaw, horse.pitch), (270.0, 15.0));
        assert_eq!(horse.jump_pending_scale, 0.0);
        let moved = ticked(MinecraftVersion::new(1, 9, 4));
        assert!(moved.position.x > 0.5);
        // setRotation takes the rider's unbounded yaw % 360.0F (1.8.8 Entity.java:323-325,
        // AbstractHorse.java:1063-1065)
        for (rider_yaw, yaw) in [(450.0, 90.0), (-400.0, -40.0)] {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::ZERO,
                0.0,
            );
            horse.on_ground = true;
            horse.rider.yaw = rider_yaw;
            tick_ridden_mob(&mut horse, &world, MinecraftVersion::new(1, 8, 8));
            assert_eq!(horse.yaw, yaw);
        }
    }

    #[test]
    fn a_horse_under_jump_boost_jumps_a_tenth_higher_a_level() {
        // AbstractHorse's riding jump adds (amplifier + 1) * 0.1F: to the double jump before 1.20.5
        // (1.16.5 AbstractHorse.java:766-770), in getJumpPower's float sum from 1.20.5 (1.21
        // AbstractHorse.java:796, 1.20.5 LivingEntity.java:2065-2072)
        let world = stone_floor((-3, 4), 100, (2170, 2177));
        let jumped = |release: MinecraftVersion, boost: Option<i32>| {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 101.0, 2173.5),
                Vec3::ZERO,
                270.0,
            );
            horse.on_ground = true;
            horse.jump_strength = 0.7;
            horse.jump_pending_scale = 1.0;
            horse.jump_boost_amplifier = boost;
            tick_ridden_mob(&mut horse, &world, release);
            horse.position.y
        };
        assert_eq!(jumped(V1_16_5, None), 101.0 + 0.7);
        assert_eq!(jumped(V1_16_5, Some(1)), 101.0 + (0.7 + f64::from(0.2_f32)));
        assert_eq!(jumped(V1_21_1, Some(1)), 101.0 + f64::from(0.7_f32 + 0.2));
        assert_eq!(jumped(V1_21_1, Some(0)), 101.0 + f64::from(0.7_f32 + 0.1));
    }

    #[test]
    fn a_camel_under_jump_boost_dashes_higher() {
        // Camel.executeRidersJump's height 1.4285F * the charge * getJumpPower(), Jump Boost's
        // 0.1F a level in it (1.21.11 Camel.java:267-269, LivingEntity.java:2258-2264)
        let world = stone_floor((-6, 12), 100, (2165, 2182));
        let dashed = |boost: Option<i32>| {
            let mut camel = ridden(
                RiddenMobKind::Camel,
                Vec3::new(3.5, 101.0, 2173.5),
                Vec3::ZERO,
                270.0,
            );
            camel.on_ground = true;
            camel.jump_pending_scale = 1.0;
            camel.jump_boost_amplifier = boost;
            tick_ridden_mob(&mut camel, &world, V26_2);
            camel.position.y
        };
        let power = |boost: f32| 101.0 + f64::from(1.4285_f32) * f64::from(0.42_f32 + boost);
        assert_eq!(dashed(None), power(0.0));
        assert_eq!(dashed(Some(0)), power(0.1));
    }

    #[test]
    fn a_sitting_camel_is_1_43_lower() {
        // Camel.getDefaultDimensions: SITTING is CAMEL's height - 1.43F (1.19.3
        // Camel.java:71, 138-139; 1.21.11 Camel.java:70-79, 138-139): a sitting camel under
        // water 1 block above its feet does not reach it, a standing one does
        let mut world = stone_floor((0, 6), 100, (2170, 2177));
        let water = BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            ..BlockState::default()
        };
        for x in 0..=6 {
            for z in 2170..=2177 {
                world.set_block((x, 102, z), water);
            }
        }
        let in_water = |sitting: bool| {
            let mut camel = ridden(
                RiddenMobKind::Camel,
                Vec3::new(3.5, 101.0, 2173.5),
                Vec3::ZERO,
                270.0,
            );
            camel.on_ground = true;
            camel.sitting = sitting;
            assert_eq!(
                camel.dimensions(V26_2),
                (1.7, if sitting { 2.375_f32 - 1.43 } else { 2.375 })
            );
            tick_ridden_mob(&mut camel, &world, V26_2);
            camel.in_water
        };
        assert!(!in_water(true));
        assert!(in_water(false));
    }

    #[test]
    fn a_camel_that_refuses_to_move_stops_before_it_swims() {
        // Camel.travel zeroes the motion of a camel that refuses to move on the ground before
        // LivingEntity.travel picks the water (1.21.11 Camel.java:219-226, 1.19.3
        // Camel.java:222-231)
        let mut world = fluid_pool(FluidState::Water, (0, 6), (101, 104), (2170, 2177));
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        for x in 0..=6 {
            for z in 2170..=2177 {
                world.set_block((x, 100, z), stone);
            }
        }
        let mut camel = ridden(
            RiddenMobKind::Camel,
            Vec3::new(3.5, 101.0, 2173.5),
            Vec3::new(0.1, 0.0, 0.05),
            270.0,
        );
        camel.on_ground = true;
        camel.in_water = true;
        camel.sitting = true;
        tick_ridden_mob(&mut camel, &world, V26_2);
        assert_eq!(camel.position.x, 3.5);
        assert_eq!(camel.position.z, 2173.5);
        assert_eq!(camel.velocity.x, 0.0);
        assert_eq!(camel.velocity.z, 0.0);
    }

    fn water_with(effect: BlockEffect) -> BlockState {
        BlockState {
            id: 2,
            fluid: FluidState::Water,
            fluid_source: true,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                effect,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    #[test]
    fn horse_sizes_follow_their_release() {
        // 1.8.8 AbstractHorse.java:209 setBounds(1.4F, 1.6F); 1.9.4 :242 and 1.13.2
        // AbstractHorse.java:148 setBounds(1.3964844F, 1.6F) for every horse; the donkey's
        // own 1.5 from 1.14 (1.14.4 EntityType.java:514)
        let v = MinecraftVersion::new;
        assert_eq!(RiddenMobKind::Horse.dimensions(v(1, 8, 8)), (1.4, 1.6));
        assert_eq!(RiddenMobKind::Donkey.dimensions(v(1, 8, 8)), (1.4, 1.6));
        assert_eq!(
            RiddenMobKind::Horse.dimensions(v(1, 9, 4)),
            (1.3964844, 1.6)
        );
        assert_eq!(
            RiddenMobKind::Donkey.dimensions(v(1, 12, 2)),
            (1.3964844, 1.6)
        );
        assert_eq!(
            RiddenMobKind::Donkey.dimensions(v(1, 13, 2)),
            (1.3964844, 1.6)
        );
        assert_eq!(
            RiddenMobKind::Donkey.dimensions(v(1, 14, 0)),
            (1.3964844, 1.5)
        );
        assert_eq!(
            RiddenMobKind::Mule.dimensions(v(1, 14, 0)),
            (1.3964844, 1.6)
        );
        assert_eq!(RiddenMobKind::Donkey.dimensions(V26_2), (1.3964844, 1.5));
    }

    #[test]
    fn skeleton_horse_keeps_the_water_slow_down_of_every_mob_before_1_13() {
        // 1.12.2 LivingEntity.java:1445-1447 0.8F with no override in SkeletonHorseEntity; 1.13.2
        // SkeletonHorse.java:143-145 0.96F
        let world = fluid_pool(FluidState::Water, (-4, 4), (98, 104), (-4, 4));
        let swim = |release: MinecraftVersion| {
            let mut horse = ridden(
                RiddenMobKind::SkeletonHorse,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::new(0.1, 0.0, 0.0),
                270.0,
            );
            tick_ridden_mob(&mut horse, &world, release);
            horse.velocity.x
        };
        assert_eq!(
            swim(MinecraftVersion::new(1, 12, 2)),
            0.1 * 0.98 * f64::from(0.8_f32)
        );
        assert_eq!(
            swim(MinecraftVersion::new(1, 13, 2)),
            0.1 * 0.98 * f64::from(0.96_f32)
        );
    }

    #[test]
    fn the_vec3_add_of_a_mob_at_a_wall_in_a_fluid_clears_the_restitutions_negative_zero() {
        // Entity.restituteMovementAfterCollisions leaves -0.0 on a wall from 26.2 (26.3
        // Entity.java:867-872); the slow-down keeps it, and the 0.0 of the Vec3.add of the ride's
        // float or of the lava's gravity clears it (26.3 LivingEntity.java:2473-2474, 2490). A
        // skeleton horse in water, which adds nothing, keeps it.
        let at_wall = |fluid: FluidState, kind: RiddenMobKind| {
            let mut world = fluid_pool(fluid, (-4, 1), (98, 104), (-4, 4));
            let stone = BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            };
            for y in 98..=104 {
                for z in -4..=4 {
                    world.set_block((2, y, z), stone);
                }
            }
            let mut mob = ridden(kind, Vec3::new(1.2, 101.0, 0.5), Vec3::new(0.3, 0.0, 0.0), 270.0);
            tick_ridden_mob(&mut mob, &world, MinecraftVersion::new(26, 3, 0));
            assert!(mob.collided_horizontally);
            mob.velocity.x
        };
        let positive = 0.0_f64.to_bits();
        assert_eq!(at_wall(FluidState::Water, RiddenMobKind::Horse).to_bits(), positive);
        assert_eq!(at_wall(FluidState::Lava, RiddenMobKind::Horse).to_bits(), positive);
        assert_eq!(
            at_wall(FluidState::Water, RiddenMobKind::SkeletonHorse).to_bits(),
            (-0.0_f64).to_bits()
        );
    }

    #[test]
    fn a_ridden_horse_or_camel_floats_in_deep_water_from_1_21_11() {
        // LivingEntity.floatInWaterWhileRidden after travelInWater: + 0.04F for a ridden
        // CAN_FLOAT_WHILE_RIDDEN mob in water deeper than getFluidJumpThreshold, 0.4 (1.21.11
        // LivingEntity.java:2336-2342, 2404-2410, EntityTypeTagsProvider.java:61-62; none in
        // 1.21.10; 26.2 LivingEntity.java:2420-2424, 2490-2496), not a skeleton horse or llama
        let world = fluid_pool(FluidState::Water, (-4, 4), (98, 104), (-4, 4));
        let swim = |kind: RiddenMobKind, release: MinecraftVersion| {
            let mut mob = ridden(kind, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
            tick_ridden_mob(&mut mob, &world, release);
            mob.velocity.y
        };
        let v1_21_10 = MinecraftVersion::new(1, 21, 10);
        let v1_21_11 = MinecraftVersion::new(1, 21, 11);
        for kind in [RiddenMobKind::Horse, RiddenMobKind::Camel] {
            assert_eq!(
                swim(kind, v1_21_11),
                swim(kind, v1_21_10) + 0.03999999910593033,
                "{kind:?}"
            );
            assert_eq!(swim(kind, V26_2), swim(kind, v1_21_11), "{kind:?}");
        }
        for kind in [RiddenMobKind::SkeletonHorse, RiddenMobKind::Llama] {
            assert_eq!(swim(kind, v1_21_11), swim(kind, v1_21_10), "{kind:?}");
        }
    }

    #[test]
    fn floating_clears_the_negative_zero_a_wall_leaves_from_26_2() {
        // restituteMovementAfterCollisions leaves -x * 0.0 (26.2 Entity.java:848ff, 26.3
        // :863-869); LivingEntity.floatInLiquidWhileRidden's Vec3.add clears it (26.3 :2488-2492)
        // for the mobs that float, and lava's gravity does for all (26.3 :2474)
        let wall = |fluid| {
            let mut world = fluid_pool(fluid, (-4, 4), (98, 104), (-4, 4));
            let stone = BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            };
            for y in 100..104 {
                for z in -2..3 {
                    world.set_block((1, y, z), stone);
                }
            }
            world
        };
        let hit = |kind: RiddenMobKind, world: &WorldSnapshot| {
            let mut mob = ridden(kind, Vec3::new(0.0, 101.0, 0.5), Vec3::new(1.0, 0.0, 0.0), 270.0);
            tick_ridden_mob(&mut mob, world, V26_2);
            mob.velocity.x
        };
        let water = wall(FluidState::Water);
        assert!(hit(RiddenMobKind::Horse, &water).to_bits() == 0, "{}", hit(RiddenMobKind::Horse, &water));
        assert!(hit(RiddenMobKind::SkeletonHorse, &water).to_bits() == (-0.0_f64).to_bits());
        assert!(hit(RiddenMobKind::SkeletonHorse, &wall(FluidState::Lava)).to_bits() == 0);
    }

    #[test]
    fn mob_in_water_climbs_against_a_wall_only_from_1_14() {
        // 1.13.2 LivingEntity.java:1646-1684: no climb in the water branch; 1.14.4 :1880-1882 sets
        // 0.2 against a wall on a climbable block
        let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 104), (-4, 4));
        world.set_block((0, 101, 0), water_with(BlockEffect::Climbable));
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        world.set_block((1, 101, 0), stone);
        world.set_block((1, 102, 0), stone);
        let swim = |release: MinecraftVersion| {
            let mut pig = ridden(
                RiddenMobKind::Pig,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::new(0.2, 0.0, 0.0),
                270.0,
            );
            tick_ridden_mob(&mut pig, &world, release);
            assert!(pig.collided_horizontally);
            pig.velocity.y
        };
        assert_eq!(swim(MinecraftVersion::new(1, 13, 2)), -0.08 / 16.0);
        assert_eq!(
            swim(MinecraftVersion::new(1, 14, 0)),
            0.2 * 0.800000011920929 - 0.08 / 16.0
        );
    }

    #[test]
    fn pig_boost_adds_its_term_in_the_tick_it_ends_before_1_11() {
        // 1.10.2 Pig.java:219-225 ends the boost inside the boosted block and still adds
        // the boost; 1.11.2 :238-246 ends it before
        let world = stone_floor((-4, 4), 100, (-4, 4));
        let speed = |release: MinecraftVersion| {
            let mut pig = ridden(
                RiddenMobKind::Pig,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::ZERO,
                270.0,
            );
            pig.on_ground = true;
            pig.boosting = true;
            pig.boost_time = 141;
            pig.boost_time_total = 140;
            tick_ridden_mob(&mut pig, &world, release);
            assert!(!pig.boosting);
            assert_eq!(pig.boost_time, 142);
            pig.ridden_speed
        };
        let base = 0.25_f32 * 0.225;
        let v1_10_2 = MinecraftVersion::new(1, 10, 2);
        let sin = mth_sin(f64::from(142.0_f32 / 140.0 * 3.1415927), v1_10_2) as f32;
        assert_eq!(
            speed(MinecraftVersion::new(1, 9, 4)),
            base + base * 1.15 * sin
        );
        assert_eq!(speed(v1_10_2), base + base * 1.15 * sin);
        assert_eq!(speed(MinecraftVersion::new(1, 11, 2)), base);
    }

    #[test]
    fn strider_checks_the_blocks_inside_in_its_fall_and_tick_until_1_21_2() {
        // 1.16 Strider.java:295-296 (fall) and :318 (tick), 1.21.1 Strider.java:289, 329,
        // beside Entity.move's own; 1.17 - 1.19.3 ItemSteerable.java:48 once more after a steered
        // travel; 1.21.2 Strider.java has neither
        let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
        world.set_block((0, 101, 0), water_with(BlockEffect::BubbleUp));
        let rise = |kind: RiddenMobKind, release: MinecraftVersion| {
            let mut mob = ridden(kind, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
            tick_ridden_mob(&mut mob, &world, release);
            mob.velocity.y
        };
        let swim = |pushes_in_move: f64| pushes_in_move * 0.06 * 0.800000011920929 - 0.08 / 16.0;
        let strider = RiddenMobKind::Strider;
        let v = MinecraftVersion::new;
        assert_eq!(rise(strider, v(1, 16, 5)), swim(2.0) + 0.06);
        assert_eq!(rise(strider, v(1, 18, 2)), swim(2.0) + 0.06 + 0.06);
        assert_eq!(rise(strider, v(1, 20, 4)), swim(2.0) + 0.06);
        assert_eq!(rise(strider, V1_21_1), swim(2.0) + 0.06);
        assert_eq!(rise(strider, v(1, 21, 2)), swim(0.0) + 0.06);
        assert_eq!(rise(RiddenMobKind::Pig, v(1, 16, 5)), swim(1.0));
        assert_eq!(rise(RiddenMobKind::Pig, v(1, 18, 2)), swim(1.0) + 0.06);
        assert_eq!(rise(RiddenMobKind::Pig, v(1, 19, 4)), swim(1.0));
    }

    #[test]
    fn a_steered_horse_checks_the_blocks_inside_again_1_17_to_1_19_3() {
        // AbstractHorse.travel's steered branch ends with tryCheckBlockCollision, beside
        // Entity.move's own (1.17 HorseBaseEntity.java:765, 1.19 AbstractHorseEntity.java:763,
        // 1.19.3 :783); 1.16.5 :748-800 and 1.19.4 have none. A 1.19.3 camel steered is a horse
        // here; one sitting ignores its rider and takes the other branch, which has none.
        let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
        world.set_block((0, 101, 0), water_with(BlockEffect::BubbleUp));
        let rise = |kind: RiddenMobKind, sitting: bool, release: MinecraftVersion| {
            let mut mob = ridden(kind, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
            mob.sitting = sitting;
            tick_ridden_mob(&mut mob, &world, release);
            mob.velocity.y
        };
        let v = MinecraftVersion::new;
        for kind in [RiddenMobKind::Horse, RiddenMobKind::Donkey, RiddenMobKind::ZombieHorse] {
            let once = rise(kind, false, v(1, 16, 5));
            assert_eq!(rise(kind, false, v(1, 17, 1)), once + 0.06, "{kind:?}");
            assert_eq!(rise(kind, false, v(1, 18, 2)), once + 0.06, "{kind:?}");
            assert_eq!(rise(kind, false, v(1, 19, 3)), once + 0.06, "{kind:?}");
            assert_eq!(rise(kind, false, v(1, 19, 4)), once, "{kind:?}");
        }
        // a llama has no controlling passenger, so it is never steered
        assert_eq!(
            rise(RiddenMobKind::Llama, false, v(1, 18, 2)),
            rise(RiddenMobKind::Llama, false, v(1, 16, 5))
        );
        let camel = RiddenMobKind::Camel;
        assert_eq!(
            rise(camel, false, v(1, 19, 3)),
            rise(camel, false, v(1, 19, 4)) + 0.06
        );
        assert_eq!(rise(camel, true, v(1, 19, 3)), rise(camel, true, v(1, 19, 4)));
    }

    #[test]
    fn a_1_19_3_camel_that_ignores_its_rider_goes_on_its_own_input() {
        // 1.19.3 Camel.ignoresMovementInput is isStationary(): AbstractHorse.travel then takes
        // the branch of a mob nobody steers, flyingSpeed 0.02F and the camel's own input after
        // LivingEntity.tickMovement's 0.98, at the speed the last steered tick set (1.19.3
        // AbstractHorse.java:746-789, Camel.java:219-249, LivingEntity.java:2594-2598); from 1.19.4
        // Camel.getRiddenInput gives a camel that refuses to move none (1.19.4 Camel.java:262-263)
        let world = WorldSnapshot::new();
        let tick = |release: MinecraftVersion| {
            let mut camel = ridden(
                RiddenMobKind::Camel,
                Vec3::new(0.5, 110.0, 0.5),
                Vec3::ZERO,
                270.0,
            );
            camel.sitting = true;
            camel.own_forward = 0.09;
            camel.ridden_speed = 0.09;
            camel.rider.sprinting = true;
            tick_ridden_mob(&mut camel, &world, release);
            camel
        };
        let old = tick(MinecraftVersion::new(1, 19, 3));
        assert_eq!(old.own_forward, 0.09_f32 * 0.98);
        assert_eq!(old.ridden_speed, 0.09);
        assert!(old.velocity.x > 0.0);
        assert_eq!(old.yaw, 270.0);
        for version in [MinecraftVersion::new(1, 19, 4), MinecraftVersion::new(1, 20, 0)] {
            let new = tick(version);
            assert_eq!(new.velocity.x, 0.0, "{version}");
            assert_eq!(new.ridden_speed, 0.09_f32 + 0.1, "{version}");
        }
    }

    #[test]
    fn a_sitting_camel_in_water_stands_up_at_once_from_1_20() {
        // Camel.tick: isCamelSitting() && isInWater() -> standUpInstantly(), its pose time that
        // of a full stand, 52 + 1 (1.20 CamelEntity.java:187-189; 26.3 Camel.java:182-184,
        // 553-566); 1.19.3 Camel.java has none
        let world = fluid_pool(FluidState::Water, (0, 6), (100, 104), (2170, 2177));
        for (release, stands) in [
            (MinecraftVersion::new(1, 19, 3), false),
            (MinecraftVersion::new(1, 20, 0), true),
            (V26_2, true),
            (MinecraftVersion::new(26, 3, 0), true),
        ] {
            let mut camel = ridden(
                RiddenMobKind::Camel,
                Vec3::new(3.5, 101.0, 2173.5),
                Vec3::ZERO,
                270.0,
            );
            camel.in_water = true;
            camel.sitting = true;
            tick_ridden_mob(&mut camel, &world, release);
            assert!(camel.in_water, "{release}");
            assert_eq!(camel.sitting, !stands, "{release}");
            assert_eq!(camel.pose_time, if stands { 54 } else { 1001 }, "{release}");
            assert!(!super::camel_in_pose_transition(&camel), "{release}");
        }
    }

    #[test]
    fn unsteered_strider_goes_on_its_own_input_at_its_move_speed_before_1_19() {
        // 1.16 ItemSteerable.java:50-52: step 0.5 and flyingSpeed 0.02F without the stick;
        // Strider.java:260-266 sets its speed (and, as a Mob, its zza: Mob.java:
        // 494-497) to getSpeed every tick
        let mut world = stone_floor((-4, 4), 100, (-4, 4));
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        world.set_block((1, 101, 0), stone);
        let walk = |saddled: bool| {
            let mut strider = ridden(
                RiddenMobKind::Strider,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::new(0.3, 0.0, 0.0),
                270.0,
            );
            strider.on_ground = true;
            strider.saddled = saddled;
            strider.own_forward = 0.1;
            tick_ridden_mob(&mut strider, &world, V1_16_5);
            strider
        };
        let unsteered = walk(false);
        let move_speed = 0.17499999701976776_f64 as f32 * 0.66;
        assert!(unsteered.suffocating);
        assert_eq!(unsteered.ridden_speed, move_speed);
        assert_eq!(unsteered.own_forward, move_speed);
        assert_eq!(unsteered.position.y, 101.0);
        assert!(unsteered.collided_horizontally);
        assert_eq!(walk(true).position.y, 102.0);
    }

    #[test]
    fn an_unsaddled_strider_keeps_its_own_speed_from_1_19_4() {
        // 1.19.3 Strider.travel sets the speed to getMoveSpeed() every travel (Strider.java:254-261);
        // 1.19.4 has no travel of its own, and the speed is the one its goals set (Strider.java:263-278)
        let world = stone_floor((-4, 4), 100, (-4, 4));
        let walk = |release: MinecraftVersion| {
            let mut strider = ridden(RiddenMobKind::Strider, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
            strider.on_ground = true;
            strider.saddled = false;
            strider.own_forward = 0.1;
            strider.ridden_speed = 0.05;
            tick_ridden_mob(&mut strider, &world, release);
            (strider.ridden_speed, strider.own_forward)
        };
        // (out of lava it is cold, and its getMoveSpeed takes 0.66 of the attribute)
        let move_speed = 0.17499999701976776_f64 as f32 * 0.66;
        assert_eq!(walk(MinecraftVersion::new(1, 19, 3)), (move_speed, move_speed));
        assert_eq!(walk(MinecraftVersion::new(1, 19, 4)), (0.05, 0.1 * 0.98));
    }

    fn web_volume() -> WorldSnapshot {
        let web = BlockState {
            shape: Some(BlockShape {
                effect: BlockEffect::Web,
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let mut world = stone_floor((-2, 4), 9, (-2, 2));
        for x in -2..=4 {
            for z in -2..=2 {
                for y in 10..=12 {
                    world.set_block((x, y, z), web);
                }
            }
        }
        world
    }

    fn walking_horse() -> RiddenMobState {
        let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 10.0, 0.5));
        mob.on_ground = true;
        mob.saddled = true;
        mob.rider.forward = 1.0;
        mob.rider.yaw = -90.0;
        mob.yaw = -90.0;
        mob
    }

    #[test]
    fn a_web_slows_the_next_move_of_a_mount() {
        // the multiplier a cobweb leaves slows the next move and is spent (1.9.4 Entity.java:550-552,
        // 26.3 WebBlock.java:26, Entity.java:783-788, 3072-3074): three ticks in it are no air
        let web = (web_volume(), (0.25, f64::from(0.05_f32), 0.25));
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let air = stone_floor((-2, 4), 9, (-2, 2));
            let (mut free, mut stuck) = (walking_horse(), walking_horse());
            for _ in 0..3 {
                tick_ridden_mob(&mut free, &air, release);
                tick_ridden_mob(&mut stuck, &web.0, release);
            }
            let (x, y, z) = web.1;
            assert_eq!(stuck.stuck_speed_multiplier, Vec3::new(x, y, z), "{release}");
            assert_eq!(free.stuck_speed_multiplier, Vec3::ZERO, "{release}");
            assert!(
                (stuck.position.x - 0.5).abs() < (free.position.x - 0.5).abs() * 0.6,
                "{release}: {} {}",
                stuck.position.x,
                free.position.x
            );
        }
    }

    const RELEASES: [MinecraftVersion; 6] = [
        MinecraftVersion::new(1, 9, 4),
        MinecraftVersion::new(1, 16, 5),
        MinecraftVersion::new(1, 20, 4),
        MinecraftVersion::new(1, 21, 1),
        MinecraftVersion::new(1, 21, 9),
        MinecraftVersion::new(26, 3, 0),
    ];

    /// A mount standing on the floor of web_volume's height, its rider pushing it along +x.
    fn standing(kind: RiddenMobKind) -> RiddenMobState {
        RiddenMobState {
            kind,
            movement_speed: kind.movement_speed(),
            ..walking_horse()
        }
    }

    #[test]
    fn a_mount_without_ai_sets_no_speed_only_in_1_21_5() {
        // Mob overrides canSimulateMovement in 1.21.5 alone (1.21.5 Mob.java:1191), so only there
        // does travelRidden skip its setSpeed
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        for (release, speed) in [
            (MinecraftVersion::new(1, 21, 4), 0.225),
            (MinecraftVersion::new(1, 21, 5), 0.0),
            (MinecraftVersion::new(1, 21, 6), 0.225),
        ] {
            let mut mob = standing(RiddenMobKind::Horse);
            mob.no_ai = true;
            mob.movement_speed = 0.225;
            tick_ridden_mob(&mut mob, &floor, release);
            assert_eq!(mob.ridden_speed, speed as f32, "{release}");
            assert_eq!(mob.own_forward, speed as f32, "{release}");
        }
    }

    #[test]
    fn a_mount_without_movement_speed_takes_no_steering() {
        // MOVEMENT_SPEED goes down to 0 in every release (1.9.4 SharedMonsterAttributes.java:24, 1.20.4
        // :39, 26.3 Attributes.java:80), and the ground's move is that speed times the friction's
        // factor (1.20.4 LivingEntity.java:2259-2260, 26.3 :2640-2642): none, not the mob's base
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        for release in RELEASES {
            let mut kinds = vec![RiddenMobKind::Horse, RiddenMobKind::Pig, RiddenMobKind::Llama];
            if release.at_least(1, 19, 3) {
                kinds.push(RiddenMobKind::Camel);
            }
            for kind in kinds {
                let mut mob = standing(kind);
                mob.movement_speed = 0.0;
                mob.velocity.y = -0.0784;
                for _ in 0..5 {
                    tick_ridden_mob(&mut mob, &floor, release);
                }
                assert_eq!(mob.ridden_speed, 0.0, "{kind:?} {release}");
                assert_eq!(mob.position, Vec3::new(0.5, 10.0, 0.5), "{kind:?} {release}");
                assert_eq!((mob.velocity.x, mob.velocity.z), (0.0, 0.0), "{kind:?} {release}");
            }
        }
    }

    #[test]
    fn a_mount_without_movement_speed_keeps_what_does_not_come_from_it() {
        // a sprinting camel's 0.1F is added to the attribute (1.20.4 Camel.java:253-256, 26.3
        // Camel.java:247-250), and water takes its own 0.02F (1.20.4 LivingEntity.java:2048,
        // 26.3 :2431)
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let release = MinecraftVersion::new(1, 21, 1);
        let mut camel = standing(RiddenMobKind::Camel);
        camel.movement_speed = 0.0;
        camel.rider.sprinting = true;
        tick_ridden_mob(&mut camel, &floor, release);
        assert_eq!(camel.ridden_speed, 0.1);
        let water = fluid_pool(FluidState::Water, (-4, 4), (9, 13), (-4, 4));
        let mut mule = RiddenMobState::new(RiddenMobKind::Mule, Vec3::new(0.5, 10.0, 0.5));
        mule.in_water = true;
        mule.movement_speed = 0.0;
        mule.yaw = -90.0;
        mule.rider.yaw = -90.0;
        mule.rider.forward = 1.0;
        tick_ridden_mob(&mut mule, &water, release);
        assert_eq!(mule.ridden_speed, 0.0);
        assert!((mule.position.x - 0.5 - f64::from(0.02_f32)).abs() < 1.0e-7);
    }

    #[test]
    fn a_mount_in_the_air_without_movement_speed_takes_no_steering() {
        // a tenth of the speed in the air (1.19.3 AbstractHorse.java:762, 1.21.11
        // LivingEntity.java:2557-2558): a tenth of nothing
        for release in RELEASES {
            let mut mob = standing(RiddenMobKind::Horse);
            mob.on_ground = false;
            mob.movement_speed = 0.0;
            for _ in 0..3 {
                tick_ridden_mob(&mut mob, &WorldSnapshot::new(), release);
            }
            assert_eq!(mob.position.x, 0.5, "{release}");
        }
    }

    #[test]
    fn a_mount_without_jump_strength_leaves_the_floor_by_nothing_but_its_push_forward() {
        // executeRidersJump sets the impulse, JUMP_STRENGTH * the charge * the block's factor, and
        // pushes 0.4F of the charge forward whatever the impulse is (1.20.4
        // AbstractHorse.java:799-810, 26.3 AbstractHorse.java:754-763). With no impulse the
        // move asks for no fall, so no collision below keeps it on the ground (1.20.4
        // Entity.java:645-646), and gravity is back for the next tick
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let drag = f64::from(0.98_f32);
        for release in RELEASES {
            let mut mob = standing(RiddenMobKind::Horse);
            mob.movement_speed = 0.0;
            mob.jump_strength = 0.0;
            mob.jump_pending_scale = 1.0;
            tick_ridden_mob(&mut mob, &floor, release);
            assert_eq!(mob.position.y, 10.0, "{release}");
            assert_eq!(mob.velocity.y, -0.08 * drag, "{release}");
            assert!(!mob.on_ground, "{release}");
            assert!(
                (mob.position.x - 0.5 - f64::from(0.4_f32)).abs() < 1.0e-6,
                "{release}: {}",
                mob.position.x
            );
            // and the charge is spent
            assert_eq!(mob.jump_pending_scale, 0.0, "{release}");
        }
    }

    #[test]
    fn jump_boost_is_all_of_the_jump_of_a_mount_without_jump_strength() {
        // the float 0.1F a level on an impulse of nothing (1.9.4 AbstractHorse.java:1047-1050,
        // 1.20.4 AbstractHorse.java:800-801, 26.3 LivingEntity.java:2294-2300)
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let drag = f64::from(0.98_f32);
        for release in RELEASES {
            let mut mob = standing(RiddenMobKind::Horse);
            mob.rider.forward = 0.0;
            mob.jump_strength = 0.0;
            mob.jump_boost_amplifier = Some(1);
            mob.jump_pending_scale = 0.5;
            tick_ridden_mob(&mut mob, &floor, release);
            let impulse = f64::from(0.1_f32 * 2.0);
            assert_eq!(mob.position.y, 10.0 + impulse, "{release}");
            assert_eq!(mob.velocity.y, (impulse - 0.08) * drag, "{release}");
        }
    }

    #[test]
    fn a_camel_without_movement_speed_or_jump_strength_dashes_nowhere() {
        // the dash is the charge times both (1.19.3 Camel.java:266-267, 1.21.11 Camel.java:
        // 289-290)
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let release = MinecraftVersion::new(1, 21, 9);
        let mut camel = standing(RiddenMobKind::Camel);
        camel.rider.forward = 0.0;
        camel.movement_speed = 0.0;
        camel.jump_strength = 0.0;
        camel.jump_pending_scale = 1.0;
        tick_ridden_mob(&mut camel, &floor, release);
        assert_eq!(camel.position, Vec3::new(0.5, 10.0, 0.5));
        assert_eq!(camel.dash_cooldown, 54);
    }

    #[test]
    fn a_tiny_movement_speed_is_a_speed() {
        // Entity.move sets the position only for a movement over 1.0E-7 squared, from 1.14 (1.14.4
        // Entity.java:492, 1.20.4 :620), or from 1.21.2 one the collision took nothing off (1.21.3
        // Entity.java:682, 1.21.4 :691), which a settled mob's fall into the floor does; 1.13 has
        // no such test (1.13.2 Entity.java:569). The motion takes the speed all the same, never
        // the base's
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let inertia = f64::from(0.6_f32 * 0.91_f32);
        for release in RELEASES {
            let speed = 1.0e-4_f64;
            let step = friction_influenced_speed(f64::from(speed as f32), 0.6, release);
            let mut rested = walking_horse();
            rested.movement_speed = speed;
            tick_ridden_mob(&mut rested, &floor, release);
            assert!((rested.velocity.x - step * inertia).abs() < 1.0e-9, "{release}");
            let moved = !release.at_least(1, 14, 0) || release.at_least(1, 21, 2);
            let expected = if moved { 0.5 + step } else { 0.5 };
            assert!((rested.position.x - expected).abs() < 1.0e-9, "{release}");
            let mut settled = walking_horse();
            settled.movement_speed = speed;
            settled.velocity.y = -0.0784;
            tick_ridden_mob(&mut settled, &floor, release);
            let expected = if release.at_least(1, 14, 0) { 0.5 } else { 0.5 + step };
            assert!((settled.position.x - expected).abs() < 1.0e-9, "{release}");
            // over the threshold it walks
            let mut walking = walking_horse();
            walking.movement_speed = 1.0e-3;
            walking.velocity.y = -0.0784;
            tick_ridden_mob(&mut walking, &floor, release);
            assert!(walking.position.x > 0.5, "{release}");
        }
    }

    /// A column of a block without collision at x 2, three blocks high, on a stone floor.
    fn effect_column(effect: BlockEffect) -> WorldSnapshot {
        let inside = BlockState {
            shape: Some(BlockShape {
                effect,
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let mut world = stone_floor((-3, 8), 9, (-3, 3));
        for z in -3..=3 {
            for y in 10..=12 {
                world.set_block((2, y, z), inside);
            }
        }
        world
    }

    /// Walks a mount through the column of `effect` and checks every tick against Entity.move's
    /// stuck speed: a multiplier left by the tick before scales the requested move and zeroes the
    /// motion, the block's makeStuckInBlock leaves it again for the next move, and the move after
    /// the last contact is the first step from rest (1.9.4 Entity.java:550-555, 1.20.4 :619-623,
    /// 26.3 :783-790, 3072-3075). The blocks act in the move on the box where it ends, deflated by
    /// 0.001, by 1.0E-7 from 1.19.3 (1.9.4 Entity.java:835-836, 1.19.2 :891-894, 1.19.3 :904-907);
    /// after travel on the box deflated by 1.0E-5F from 1.21.2, a move under a block long still
    /// taking the target's blocks alone (1.21.4 Entity.java:1096-1106, BlockGetter.java:164-169), and
    /// on the blocks swept from where the tick began from 1.21.5 (26.3 Entity.java:1351-1357,
    /// BlockGetter.java:161-182). Powder snow takes a living entity only by its feet (1.17
    /// PowderSnowBlock.java:54-56, 26.3 :55).
    fn walks_through(
        kind: RiddenMobKind,
        effect: BlockEffect,
        multiplier: Vec3,
        set: &dyn Fn(&mut RiddenMobState),
        release: MinecraftVersion,
    ) {
        let world = effect_column(effect);
        let mut mob = RiddenMobState::new(kind, Vec3::new(0.2, 10.0, 0.5));
        mob.on_ground = true;
        mob.velocity.y = -0.0784;
        mob.rider.forward = 1.0;
        mob.rider.yaw = -90.0;
        mob.yaw = -90.0;
        set(&mut mob);
        let half = f64::from(mob.dimensions(release).0 / 2.0);
        let deflate = if release.at_least(1, 21, 2) {
            f64::from(1.0e-5_f32)
        } else if release.at_least(1, 19, 3) {
            1.0e-7
        } else {
            0.001
        };
        let touches = |x: f64| x + half - deflate > 2.0 && x - half + deflate < 3.0;
        let inertia = f64::from(0.6_f32 * 0.91_f32);
        let (mut motion, mut left) = (0.0_f64, Vec3::ZERO);
        let (mut entered, mut slowed, mut free_after) = (false, 0, 0);
        for tick in 0..600 {
            let label = format!("{kind:?} {effect:?} {release} tick {tick}");
            let start = mob.position.x;
            tick_ridden_mob(&mut mob, &world, release);
            let mut before = if release.at_least(1, 21, 5) {
                motion
            } else {
                motion * 0.98
            };
            if before.abs() < 0.003 {
                before = 0.0;
            }
            let speed = f64::from(mob.ridden_speed);
            let asked = before + friction_influenced_speed(speed, 0.6, release);
            let stuck = left != Vec3::ZERO;
            let step = if stuck { asked * left.x } else { asked };
            assert!((mob.position.x - start - step).abs() < 1.0e-7, "{label}");
            motion = if stuck { 0.0 } else { asked * inertia };
            assert!((mob.velocity.x - motion).abs() < 1.0e-7, "{label}");
            motion = mob.velocity.x;
            assert!(mob.on_ground, "{label}");
            let inside = touches(mob.position.x)
                || (release.at_least(1, 21, 5) && touches(start));
            let by_feet = effect != BlockEffect::PowderSnow
                || (2.0..3.0).contains(&mob.position.x);
            let last = left;
            left = if inside && by_feet { multiplier } else { Vec3::ZERO };
            assert_eq!(mob.stuck_speed_multiplier, left, "{label}");
            entered |= left != Vec3::ZERO;
            if stuck {
                slowed += 1;
            }
            if entered && last == Vec3::ZERO && left == Vec3::ZERO {
                free_after += 1;
            }
            if free_after == 3 {
                break;
            }
        }
        assert!(slowed > 3, "{kind:?} {effect:?} {release}: {slowed}");
        assert_eq!(free_after, 3, "{kind:?} {effect:?} {release}");
    }

    #[test]
    fn a_cobweb_ends_the_fall_of_the_move_that_reaches_it_before_1_21_2() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 100, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let release = MinecraftVersion::new(1, 20, 4);
        let mut mob = ridden(RiddenMobKind::Horse, Vec3::new(0.5, 100.5, 0.5), Vec3::new(0.0, -0.1, 0.0), 0.0);
        mob.fall_distance = 3.0;
        tick_ridden_mob(&mut mob, &world, release);
        assert_ne!(mob.stuck_speed_multiplier, Vec3::ZERO);
        assert_eq!(mob.fall_distance, 0.0);
    }

    #[test]
    fn a_mount_enters_stays_in_and_leaves_a_web() {
        let web = Vec3::new(0.25, f64::from(0.05_f32), 0.25);
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for kind in [RiddenMobKind::Horse, RiddenMobKind::Pig, RiddenMobKind::Llama] {
                walks_through(kind, BlockEffect::Web, web, &|_| {}, release);
            }
            if release.at_least(1, 19, 3) {
                walks_through(RiddenMobKind::Camel, BlockEffect::Web, web, &|_| {}, release);
            }
        }
    }

    #[test]
    fn a_mount_enters_stays_in_and_leaves_a_berry_bush_from_1_14() {
        // (0.8F, 0.75, 0.8F) for what is not a fox or bee (1.14.4 SweetBerryBushBlock.java:58-59,
        // 26.3 :76-77)
        let bush = Vec3::new(f64::from(0.8_f32), 0.75, f64::from(0.8_f32));
        for release in [
            MinecraftVersion::new(1, 14, 0),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for kind in [RiddenMobKind::Horse, RiddenMobKind::Pig] {
                walks_through(kind, BlockEffect::SweetBerryBush, bush, &|_| {}, release);
            }
        }
    }

    #[test]
    fn a_mount_enters_stays_in_and_leaves_powder_snow_from_1_17() {
        // (0.9F, 1.5, 0.9F), once the feet are in it (1.17 PowderSnowBlock.java:54-56, 26.3 :55-56)
        let snow = Vec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32));
        for release in [
            MinecraftVersion::new(1, 17, 0),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for kind in [RiddenMobKind::Horse, RiddenMobKind::Pig] {
                walks_through(kind, BlockEffect::PowderSnow, snow, &|_| {}, release);
            }
        }
    }

    #[test]
    fn weaving_loosens_the_web_a_mount_is_in() {
        // WebBlock.entityInside's (0.5, 0.25, 0.5) for a LivingEntity under Weaving (1.20.5
        // WebBlock.java:25-29, 26.3 WebBlock.java:19-24)
        let release = MinecraftVersion::new(1, 21, 9);
        let weaving = |mob: &mut RiddenMobState| mob.weaving = true;
        let loose = Vec3::new(0.5, 0.25, 0.5);
        walks_through(RiddenMobKind::Horse, BlockEffect::Web, loose, &weaving, release);
    }

    #[test]
    fn a_boat_and_a_mount_keep_the_same_web_multiplier() {
        // the boat control of M01: Boat and LivingEntity share Entity.move and makeStuckInBlock
        // (26.3 Entity.java:783-790, 3072-3075), so both come out of a web with the same multiplier
        // and both spend it in the next move
        use crate::vehicle::{tick_boat_in_world, BoatSimulationState};
        let release = MinecraftVersion::new(1, 21, 9);
        let mut world = web_volume();
        let web = Vec3::new(0.25, f64::from(0.05_f32), 0.25);
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 10.0, 0.5),
            ..BoatSimulationState::default()
        };
        tick_boat_in_world(&mut boat, &mut world, release);
        assert_eq!(boat.stuck_speed_multiplier, web);
        let mut mob = walking_horse();
        tick_ridden_mob(&mut mob, &world, release);
        assert_eq!(mob.stuck_speed_multiplier, boat.stuck_speed_multiplier);
        // out of the web, with nothing there to leave it again
        let mut clear = stone_floor((-2, 4), 9, (-2, 2));
        tick_boat_in_world(&mut boat, &mut clear, release);
        tick_ridden_mob(&mut mob, &clear, release);
        assert_eq!(boat.stuck_speed_multiplier, Vec3::ZERO);
        assert_eq!(mob.stuck_speed_multiplier, Vec3::ZERO);
    }

    #[test]
    fn a_scaled_mount_collides_at_its_measured_size() {
        // SCALE multiplies the dimensions (1.20.5 LivingEntity.java:3264-3269, 26.3 :3684-3689): a
        // ceiling at y 13 clears a horse 1.6 high at y 9.7, and stops one 3.2 high a little up
        let release = MinecraftVersion::new(1, 21, 11);
        let world = stone_floor((-2, 2), 13, (-2, 2));
        let rise = |scale: f32| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 9.7, 0.5));
            mob.scale = (scale, scale);
            mob.velocity = Vec3::new(0.0, 0.3, 0.0);
            tick_ridden_mob(&mut mob, &world, release);
            mob.position.y - 9.7
        };
        assert!((rise(1.0) - 0.3).abs() < 1.0e-9);
        assert!((rise(2.0) - 0.1).abs() < 1.0e-6);
    }

    /// A floor at y 9 with these solid blocks on it.
    fn floor_with(blocks: impl IntoIterator<Item = (i32, i32, i32)>) -> WorldSnapshot {
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        let mut world = stone_floor((-6, 9), 9, (-6, 6));
        for position in blocks {
            world.set_block(position, stone);
        }
        world
    }

    /// A horse of that scale walks along +x from (0.5, 10, 0) for 60 ticks.
    fn walked_at(scale: f32, world: &WorldSnapshot, release: MinecraftVersion) -> RiddenMobState {
        let mut mob = walking_horse();
        mob.position.z = 0.0;
        mob.scale = (scale, scale);
        mob.velocity.y = -0.0784;
        for _ in 0..60 {
            tick_ridden_mob(&mut mob, world, release);
        }
        mob
    }

    #[test]
    fn a_doubled_mount_stops_at_an_opening_and_a_ceiling_a_plain_one_passes() {
        // the SCALE of 1.20.5 scales both dimensions (1.20.5 LivingEntity.java:3264-3269, 26.3
        // :3684-3689, EntityDimensions.java:34): a horse 1.3964844 wide and 1.6 high is 2.7929688
        // by 3.2 at scale 2, an ordinary rideable horse that /attribute has resized
        let wall = -6..=6;
        let opening = floor_with(wall.clone().flat_map(|z| {
            let blocked = !(-1..=0).contains(&z);
            (10..=14).filter(move |_| blocked).map(move |y| (3, y, z))
        }));
        let ceiling = floor_with(wall.flat_map(|z| [(3, 12, z), (3, 13, z)]));
        for release in [
            MinecraftVersion::new(1, 20, 5),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for world in [&opening, &ceiling] {
                let plain = walked_at(1.0, world, release);
                let doubled = walked_at(2.0, world, release);
                let width = f64::from(1.3964844_f32 * 2.0);
                assert_eq!(doubled.dimensions(release), (1.3964844 * 2.0, 3.2), "{release}");
                assert!(plain.position.x > 4.0, "{release}: {}", plain.position.x);
                let stop = 3.0 - width / 2.0;
                assert!((doubled.position.x - stop).abs() < 1.0e-6, "{release}");
                assert!(doubled.collided_horizontally, "{release}");
            }
        }
    }

    #[test]
    fn a_doubled_mount_touches_the_water_beside_and_above_a_plain_one() {
        // the fluid's contact is taken over the entity's box (1.20.5 Entity.java:3103-3107): a doubled horse's reaches
        // the shallow water 1.4 away and the layer 2 above its feet, a plain one's neither
        let release = MinecraftVersion::new(1, 21, 9);
        let beside = fluid_pool(FluidState::Water, (2, 4), (10, 10), (-2, 2));
        let above = fluid_pool(FluidState::Water, (-2, 2), (12, 12), (-2, 2));
        for water in [&beside, &above] {
            let at = |scale: f32| {
                let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.7, 10.0, 0.5));
                mob.scale = (scale, scale);
                mob.no_gravity = true;
                mob.velocity.x = 0.01;
                tick_ridden_mob(&mut mob, water, release);
                mob
            };
            let (plain, doubled) = (at(1.0), at(2.0));
            assert!(!plain.in_water);
            assert!(doubled.in_water);
            // the air's 0.91F against the water's 0.8F of the same motion
            assert!(plain.velocity.x > doubled.velocity.x);
        }
    }

    #[test]
    fn a_mount_of_low_eyes_has_no_shallow_lava_or_water() {
        // getFluidJumpThreshold is 0 below eyes of 0.4 (26.3 Entity.java:3803-3805): a pig at half
        // scale has 0.3825, a nautilus 0.2751 at any (26.3 EntityTypes.java:427). Its lava is
        // never shallow (26.3 LivingEntity.java:2458-2460, 2462-2470): the whole of the 0.5, no
        // 0.8 for the vertical and none of the falling adjustment, and only the quarter of gravity
        let lava = |height: u8| {
            let mut world = stone_floor((-4, 4), 9, (-4, 4));
            let shallow = BlockState {
                fluid: FluidState::Lava,
                fluid_level: height,
                ..BlockState::default()
            };
            for x in -4..=4 {
                for z in -4..=4 {
                    world.set_block((x, 10, z), shallow);
                }
            }
            world
        };
        let release = MinecraftVersion::new(26, 3, 0);
        let world = lava(7);
        let fall = |kind, scale: f32| {
            let mut mob = RiddenMobState::new(kind, Vec3::new(0.5, 10.0, 0.5));
            mob.scale = (scale, scale);
            mob.in_lava = true;
            mob.velocity.y = 0.1;
            tick_ridden_mob(&mut mob, &world, release);
            mob.velocity.y
        };
        let shallow = 0.1 * 0.800000011920929 - 0.08 / 16.0 - 0.08 / 4.0;
        let deep = 0.1 * 0.5 - 0.08 / 4.0;
        assert!((fall(RiddenMobKind::Pig, 1.0) - shallow).abs() < 1.0e-9);
        assert!((fall(RiddenMobKind::Pig, 0.5) - deep).abs() < 1.0e-9);
        assert!((fall(RiddenMobKind::Nautilus, 1.0) - deep).abs() < 1.0e-9);
        // floatInWaterWhileRidden is the threshold too, from 1.21.11: a horse at a quarter (0.38)
        let puddle = {
            let mut world = stone_floor((-4, 4), 9, (-4, 4));
            let shallow = BlockState {
                fluid: FluidState::Water,
                fluid_level: 7,
                ..BlockState::default()
            };
            for x in -4..=4 {
                for z in -4..=4 {
                    world.set_block((x, 10, z), shallow);
                }
            }
            world
        };
        let float = |scale: f32| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 10.5, 0.5));
            mob.scale = (scale, scale);
            mob.position.y = 10.05;
            mob.in_water = true;
            tick_ridden_mob(&mut mob, &puddle, release);
            mob.velocity.y
        };
        assert!((float(0.25) - float(1.0) - f64::from(0.04_f32)).abs() < 1.0e-9);
    }

    #[test]
    fn a_doubled_mount_is_stuck_by_a_block_beside_a_plain_one() {
        // checkInsideBlocks takes the box (26.3 Entity.java:1351-1357)
        let world = effect_column(BlockEffect::Web);
        for release in [
            MinecraftVersion::new(1, 20, 5),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let at = |scale: f32| {
                let mut mob = walking_horse();
                mob.position.x = 0.7;
                mob.rider.forward = 0.0;
                mob.scale = (scale, scale);
                tick_ridden_mob(&mut mob, &world, release);
                mob.stuck_speed_multiplier
            };
            assert_eq!(at(1.0), Vec3::ZERO, "{release}");
            assert_eq!(at(2.0), Vec3::new(0.25, f64::from(0.05_f32), 0.25), "{release}");
        }
    }

    #[test]
    fn a_doubled_mount_stands_on_the_rim_of_a_hole_a_plain_one_falls_in() {
        // Entity.collide takes the box (26.3 Entity.java:1185-1187)
        let mut world = WorldSnapshot::new();
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        for x in -4..=4 {
            for z in -4..=4 {
                if !(0..=1).contains(&x) || !(0..=1).contains(&z) {
                    world.set_block((x, 9, z), stone);
                }
            }
        }
        let release = MinecraftVersion::new(1, 21, 9);
        let at = |scale: f32| {
            let mut mob = walking_horse();
            mob.position = Vec3::new(1.0, 10.0, 1.0);
            mob.rider.forward = 0.0;
            mob.velocity.y = -0.0784;
            mob.scale = (scale, scale);
            tick_ridden_mob(&mut mob, &world, release);
            mob
        };
        assert!(at(1.0).position.y < 10.0);
        let doubled = at(2.0);
        assert_eq!(doubled.position.y, 10.0);
        assert!(doubled.on_ground);
    }

    #[test]
    fn a_scaled_camel_stands_up_to_its_scaled_height_in_the_tick() {
        // Camel.standUp's setPose(STANDING) refreshes the dimensions at once, getDimensions scaling
        // the pose's (1.21.11 Camel.java:228-234, 529-534, 26.3 LivingEntity.java:3684-3689): 4.75
        // high under a ceiling at 15, so a rise of 0.3 stops at 0.25; sitting it is 1.89 high
        let release = MinecraftVersion::new(1, 21, 9);
        let world = stone_floor((-4, 4), 15, (-4, 4));
        let rise = |forward: f32| {
            let mut camel = RiddenMobState::new(RiddenMobKind::Camel, Vec3::new(0.5, 10.0, 0.5));
            camel.sitting = true;
            camel.scale = (2.0, 2.0);
            camel.velocity.y = 0.3;
            camel.rider.forward = forward;
            tick_ridden_mob(&mut camel, &world, release);
            camel
        };
        let stood = rise(1.0);
        assert!(!stood.sitting);
        assert_eq!(stood.dimensions(release), (1.7 * 2.0, 2.375 * 2.0));
        assert!((stood.position.y - (15.0 - 4.75)).abs() < 1.0e-6);
        let sat = rise(0.0);
        assert!(sat.sitting);
        assert_eq!(sat.dimensions(release).1, (2.375_f32 - 1.43) * 2.0);
        assert!((sat.position.y - 10.3).abs() < 1.0e-6);
    }

    #[test]
    fn a_kept_box_is_a_mounts_body_until_1_17() {
        // the box is moved and the position read back from it before 1.17 (1.16.5 Entity.java:611-612,
        // 911); from 1.17
        // the position is set and the box rebuilt from the dimensions (1.17 Entity.java:367-369,
        // 537). A rideable mob's
        // box is its type's size: one of other dimensions is no gameplay, but the fixture that
        // tells the two apart, 2.8 wide on a horse 1.4 wide
        let world = floor_with((-6..=6).flat_map(|z| (10..=14).map(move |y| (3, y, z))));
        let kept = Aabb::new(Vec3::new(-0.4, 10.0, -0.9), Vec3::new(2.4, 11.6, 1.9));
        let walked = |release| {
            let mut mob = walking_horse();
            mob.position = Vec3::new(1.0, 10.0, 0.5);
            mob.velocity.y = -0.0784;
            mob.bounding_box = Some(kept);
            for _ in 0..30 {
                tick_ridden_mob(&mut mob, &world, release);
            }
            mob
        };
        for release in [
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 16, 5),
        ] {
            let mob = walked(release);
            let held = mob.bounding_box.expect("the box stays kept");
            assert!((held.max.x - 3.0).abs() < 1.0e-6, "{release}: {held:?}");
            assert!((held.max.x - held.min.x - 2.8).abs() < 1.0e-9, "{release}");
            assert!((mob.position.x - 1.6).abs() < 1.0e-6, "{release}: {}", mob.position.x);
        }
        let release = MinecraftVersion::new(1, 17, 0);
        let rebuilt = walked(release);
        assert_eq!(rebuilt.bounding_box, None);
        let half = f64::from(1.3964844_f32 / 2.0);
        assert!((rebuilt.position.x - (3.0 - half)).abs() < 1.0e-6);
    }

    #[test]
    fn a_step_toward_a_teleport_rebuilds_a_kept_box_at_the_measured_size() {
        // setPosition builds the box from the dimensions before 1.17 too (1.16.5 Entity.java:471)
        let release = MinecraftVersion::new(1, 16, 5);
        let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 10.0, 0.5));
        mob.scale = (2.0, 2.0);
        mob.no_gravity = true;
        mob.bounding_box = Some(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
        mob.lerp_position = Some(Vec3::new(4.5, 20.0, 4.5));
        tick_ridden_mob(&mut mob, &WorldSnapshot::new(), release);
        let held = mob.bounding_box.unwrap();
        assert_eq!(held.max.y - held.min.y, f64::from(1.6_f32 * 2.0));
        assert_eq!(held.max.x - held.min.x, f64::from(1.3964844_f32 * 2.0 / 2.0) * 2.0);
    }

    #[test]
    fn measured_dimensions_survive_a_camels_pose() {
        let release = MinecraftVersion::new(1, 21, 11);
        let mut camel = RiddenMobState::new(RiddenMobKind::Camel, Vec3::ZERO);
        camel.sitting = true;
        let (width, height) = camel.dimensions(release);
        camel.measure(Some(width * 2.0), Some(height * 2.0), release);
        assert_eq!(camel.dimensions(release), (width * 2.0, height * 2.0));
        // measuring its own output again is the same scale, and standing up keeps it
        camel.measure(Some(width * 2.0), Some(height * 2.0), release);
        camel.sitting = false;
        assert_eq!(camel.dimensions(release), (1.7 * 2.0, 2.375 * 2.0));
        let mut plain = RiddenMobState::new(RiddenMobKind::Horse, Vec3::ZERO);
        plain.measure(None, None, release);
        assert_eq!(plain.dimensions(release), RiddenMobKind::Horse.dimensions(release));
    }

    fn falling(release: MinecraftVersion, mut set: impl FnMut(&mut RiddenMobState)) -> RiddenMobState {
        let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 50.0, 0.5));
        mob.fall_distance = 5.0;
        set(&mut mob);
        tick_ridden_mob(&mut mob, &WorldSnapshot::new(), release);
        mob
    }

    #[test]
    fn a_mount_falls_by_its_effects_and_gravity() {
        // getEffectiveGravity (1.13.2 LivingEntity.java:1511, 26.3 :2333-2339) and Levitation in
        // place of gravity (1.9.4 :1417-1419, 26.3 :2387-2397)
        let drag = f64::from(0.98_f32);
        let release = MinecraftVersion::new(1, 21, 11);
        assert_eq!(falling(release, |_| {}).velocity.y, -0.08 * drag);
        let slow = falling(release, |mob| mob.slow_falling = true);
        assert_eq!(slow.velocity.y, -0.01 * drag);
        assert_eq!(slow.fall_distance, 0.0);
        // Slow Falling is of 1.13, its pull only while falling
        let old = falling(MinecraftVersion::new(1, 12, 2), |mob| mob.slow_falling = true);
        assert_eq!(old.velocity.y, -0.08 * drag);
        let rising = falling(release, |mob| {
            mob.slow_falling = true;
            mob.velocity.y = 0.5;
        });
        assert!((rising.velocity.y - (0.5 - 0.08) * drag).abs() < 1.0e-6);
        let up = falling(release, |mob| mob.levitation_amplifier = Some(1));
        assert_eq!(up.velocity.y, (0.1 * 0.2) * drag);
        assert_eq!(up.fall_distance, 0.0);
        // 1.12 keeps the fall distance under Levitation; 1.13 - 1.19.4 reset it after the move
        let kept = falling(MinecraftVersion::new(1, 12, 2), |mob| {
            mob.levitation_amplifier = Some(0);
        });
        assert_eq!(kept.fall_distance, 5.0);
        // the GRAVITY attribute is of 1.20.5
        let heavy = |mob: &mut RiddenMobState| mob.gravity = Some(0.2);
        assert_eq!(falling(release, heavy).velocity.y, -0.2 * drag);
        assert_eq!(
            falling(MinecraftVersion::new(1, 20, 4), heavy).velocity.y,
            -0.08 * drag
        );
        assert_eq!(
            falling(release, |mob| mob.gravity = Some(-5.0)).velocity.y,
            1.0 * drag
        );
    }

    #[test]
    fn a_mount_steps_by_its_attribute_with_a_player_in_control() {
        // LivingEntity.maxUpStep: the attribute, at least 1 with a player in control (1.20.5
        // LivingEntity.java:3507-3510, 26.3 :3974-3977)
        let new = MinecraftVersion::new(1, 21, 11);
        let steps = |height: f64, attribute: Option<f64>, release| {
            let mut world = stone_floor((-2, 6), 9, (-2, 2));
            let stone = BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            };
            for z in -2..=2 {
                for y in 10..(10 + height as i32) {
                    world.set_block((2, y, z), stone);
                }
            }
            let mut mob = walking_horse();
            mob.position = Vec3::new(1.0, 10.0, 0.5);
            mob.velocity = Vec3::new(0.6, 0.0, 0.0);
            mob.rider.forward = 0.0;
            mob.step_height = attribute;
            tick_ridden_mob(&mut mob, &world, release);
            mob.position.y
        };
        assert_eq!(steps(1.0, None, new), 11.0);
        assert_eq!(steps(2.0, None, new), 10.0);
        assert_eq!(steps(2.0, Some(2.0), new), 12.0);
        assert_eq!(steps(1.0, Some(0.0), new), 11.0);
        assert_eq!(steps(2.0, Some(2.0), MinecraftVersion::new(1, 20, 4)), 10.0);
    }

    #[test]
    fn a_mount_swims_by_its_water_attribute_and_dolphins_grace() {
        let water = fluid_pool(FluidState::Water, (-4, 4), (9, 13), (-4, 4));
        let drifted = |set: &dyn Fn(&mut RiddenMobState), release| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Mule, Vec3::new(0.5, 10.0, 0.5));
            mob.in_water = true;
            mob.velocity = Vec3::new(0.2, 0.0, 0.0);
            set(&mut mob);
            tick_ridden_mob(&mut mob, &water, release);
            mob.velocity.x
        };
        let new = MinecraftVersion::new(1, 21, 11);
        let plain = drifted(&|_| {}, new);
        let fast = drifted(&|mob| mob.dolphins_grace = true, new);
        assert!((fast / plain - f64::from(0.96_f32) / 0.8).abs() < 1.0e-6);
        // the attribute is of 1.21, halved off the ground: 0.8F toward 0.54600006F
        let efficient = |mob: &mut RiddenMobState| mob.water_movement_efficiency = 1.0;
        assert!((drifted(&efficient, new) / plain - 0.673_000_03 / 0.8).abs() < 1.0e-6);
        assert_eq!(
            drifted(&efficient, MinecraftVersion::new(1, 20, 6)),
            drifted(&|_| {}, MinecraftVersion::new(1, 20, 6))
        );
    }

    /// What Mth.clamp(1.0F - (1.0F - friction) * modifier, 0.0F, 1.0F) gives (26.3
    /// LivingEntity.java:483-485).
    fn modified(friction: f32, modifier: f32) -> f32 {
        (1.0 - (1.0 - friction) * modifier).clamp(0.0, 1.0)
    }

    #[test]
    fn a_mounts_friction_modifier_changes_its_step_and_inertia_from_26_2() {
        // travelInAir's friction on the ground (26.3 LivingEntity.java:2384, 2402-2405) and
        // getFrictionInfluencedSpeed's plain speed at a friction of 0.6 or less (:2640-2642)
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let walked = |modifier: Option<f64>, release| {
            let mut mob = walking_horse();
            mob.velocity.y = -0.0784;
            mob.friction_modifier = modifier;
            tick_ridden_mob(&mut mob, &floor, release);
            mob
        };
        let new = MinecraftVersion::new(26, 3, 0);
        let speed = RiddenMobKind::Horse.movement_speed() as f32;
        let case = |modifier: Option<f64>, friction: f32| {
            let mob = walked(modifier, new);
            let step = if f64::from(friction) > 0.6 {
                speed * (0.21600002 / (friction * friction * friction))
            } else {
                speed
            };
            let inertia = friction * 0.91;
            assert!((mob.position.x - 0.5 - f64::from(step)).abs() < 1.0e-7, "{modifier:?}");
            let motion = f64::from(step) * f64::from(inertia);
            assert!((mob.velocity.x - motion).abs() < 1.0e-7, "{modifier:?}");
        };
        let stone = 0.6_f32;
        case(None, stone);
        case(Some(1.0), stone);
        case(Some(0.0), modified(stone, 0.0));
        case(Some(0.5), modified(stone, 0.5));
        case(Some(2.0), modified(stone, 2.0));
        case(Some(3.0), 0.0);
        // not before it, nor in the air (:2384's 1.0F)
        let old = MinecraftVersion::new(26, 1, 0);
        let (given, none) = (walked(Some(0.0), old), walked(None, old));
        assert_eq!((given.position, given.velocity), (none.position, none.velocity));
        let drifted = |modifier: Option<f64>| {
            let mob = falling(new, |mob| {
                mob.velocity.x = 0.5;
                mob.friction_modifier = modifier;
            });
            mob.velocity.x
        };
        assert_eq!(drifted(Some(0.0)), drifted(None));
    }

    #[test]
    fn a_mounts_air_drag_modifier_changes_both_drags_from_26_2() {
        // airDrag on 0.91F and the vertical friction on 0.98F, the block's friction times the first
        // (26.3 LivingEntity.java:2403-2406)
        let new = MinecraftVersion::new(26, 3, 0);
        for modifier in [0.0_f32, 0.5, 2.0, 20.0] {
            let mob = falling(new, |mob| {
                mob.velocity.x = 0.5;
                mob.air_drag_modifier = Some(f64::from(modifier));
            });
            assert_eq!(mob.velocity.x, 0.5 * f64::from(modified(0.91, modifier)));
            assert_eq!(mob.velocity.y, -0.08 * f64::from(modified(0.98, modifier)));
        }
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let mut mob = walking_horse();
        mob.velocity.y = -0.0784;
        mob.air_drag_modifier = Some(0.0);
        tick_ridden_mob(&mut mob, &floor, new);
        let step = mob.position.x - 0.5;
        assert!((mob.velocity.x - step * f64::from(0.6_f32 * 1.0)).abs() < 1.0e-7);
    }

    #[test]
    fn a_mounts_bounce_compensates_the_drag_of_its_modifier() {
        // getAirDrag is the modified 0.98F for all that does not fly (26.3 LivingEntity.java:2412-2414,
        // 26.2 :2414-2416), which restituteMovementAfterCollisions lerps by the part of the fall the
        // move made (26.2 Entity.java:855-856, 26.3 :885-888), then travelInAir takes off gravity
        // and the same drag
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 99, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 1.0,
                ..BlockState::default()
            },
        );
        for modifier in [None, Some(0.0_f64), Some(2.0)] {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 100.1, 0.5),
                Vec3::new(0.0, -0.3, 0.0),
                270.0,
            );
            horse.air_drag_modifier = modifier;
            tick_ridden_mob(&mut horse, &world, V26_2);
            let drag = f64::from(modified(0.98, modifier.unwrap_or(1.0) as f32));
            let portion = -0.1 / -0.3;
            let lerped = 1.0 + portion * (drag - 1.0);
            let expected = (((portion * 0.08) + 0.3) * lerped - 0.08) * drag;
            assert!((horse.velocity.y - expected).abs() < 1.0e-6, "{modifier:?}");
        }
    }

    #[test]
    fn a_landing_at_exactly_its_gravity_rests_from_26_3() {
        // !(-currentMovement.y < gravity) in 26.2, !(-currentMovement.y <= gravity) in 26.3 (26.2
        // Entity.java:848, 26.3 :878): a mount with GRAVITY 0.25 meets it exactly
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 99, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 1.0,
                ..BlockState::default()
            },
        );
        let landing = |release| {
            let mut horse = ridden(
                RiddenMobKind::Horse,
                Vec3::new(0.5, 100.1, 0.5),
                Vec3::new(0.0, -0.25, 0.0),
                270.0,
            );
            horse.gravity = Some(0.25);
            tick_ridden_mob(&mut horse, &world, release);
            horse.velocity.y
        };
        assert!(landing(V26_2) > 0.0);
        assert_eq!(landing(MinecraftVersion::new(26, 3, 0)), -0.25 * f64::from(0.98_f32));
    }

    #[test]
    fn a_mount_falls_by_a_gravity_of_nothing_and_rises_by_a_negative_one() {
        // getGravity is the attribute, whatever its sign (1.20.5 LivingEntity.java:2107, 26.3
        // :2333-2339, 2397; Attributes.java range -1 to 1); the same goes for fluids, whose
        // falling adjustment is skipped for a gravity of 0 (26.3 LivingEntity.java:2608-2620)
        let release = MinecraftVersion::new(26, 3, 0);
        let drag = f64::from(0.98_f32);
        assert_eq!(falling(release, |mob| mob.gravity = Some(0.0)).velocity.y, 0.0);
        let negative = falling(release, |mob| mob.gravity = Some(-0.04));
        assert_eq!(negative.velocity.y, 0.04 * drag);
        // (a llama: a mule's CAN_FLOAT_WHILE_RIDDEN would lift it, 26.3 LivingEntity.java:2488-2493)
        let water = fluid_pool(FluidState::Water, (-4, 4), (9, 13), (-4, 4));
        let swum = |gravity: Option<f64>, y: f64| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Llama, Vec3::new(0.5, 10.0, 0.5));
            mob.in_water = true;
            mob.velocity.y = y;
            mob.gravity = gravity;
            tick_ridden_mob(&mut mob, &water, release);
            mob.velocity.y
        };
        let slowed = 0.800000011920929;
        assert_eq!(swum(Some(0.0), -0.0035), -0.0035 * slowed);
        // near a gravity's sixteenth, the same motion is caught at -0.003 (:2611-2612); further from
        // it the sixteenth comes off
        assert_eq!(swum(Some(0.0008), -0.0035), -0.003);
        assert_eq!(swum(Some(0.04), -0.0035), -0.0035 * slowed - 0.04 / 16.0);
        assert_eq!(swum(None, 0.1), 0.1 * slowed - 0.08 / 16.0);
        assert_eq!(swum(Some(-0.16), 0.1), 0.1 * slowed + 0.16 / 16.0);
        let lava = fluid_pool(FluidState::Lava, (-4, 4), (9, 13), (-4, 4));
        let mut mob = RiddenMobState::new(RiddenMobKind::Llama, Vec3::new(0.5, 10.0, 0.5));
        mob.in_lava = true;
        mob.velocity.y = 0.1;
        mob.gravity = Some(0.0);
        tick_ridden_mob(&mut mob, &lava, release);
        assert_eq!(mob.velocity.y, 0.1 * 0.5);
    }

    #[test]
    fn slow_falling_reads_the_motion_the_move_left_from_1_21_2() {
        // travelInAir takes getEffectiveGravity after the move from 1.21.2 (1.21.3 LivingEntity.java:
        // 2183-2185, 2205-2211; 26.3 :2337-2340, 2382-2397), so a mount that rose into a ceiling has
        // no upward motion left and falls by 0.01; 1.21 reads the motion travel began with (1.21
        // LivingEntity.java:2117-2121)
        let world = stone_floor((-2, 2), 12, (-2, 2));
        let rose = |release| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 10.3, 0.5));
            mob.velocity.y = 0.2;
            mob.slow_falling = true;
            tick_ridden_mob(&mut mob, &world, release);
            mob.velocity.y
        };
        let drag = f64::from(0.98_f32);
        assert_eq!(rose(MinecraftVersion::new(1, 21, 4)), -0.01 * drag);
        assert_eq!(rose(MinecraftVersion::new(26, 3, 0)), -0.01 * drag);
        assert_eq!(rose(MinecraftVersion::new(1, 21, 1)), -0.08 * drag);
    }

    #[test]
    fn a_mount_in_water_is_slowed_by_slow_falling_and_not_lifted_by_levitation() {
        // travelInFluid takes getEffectiveGravity, and Levitation is of travelInAir alone (26.3
        // LivingEntity.java:2382-2397, 2416-2425; 1.13.2 :1646-1684)
        let release = MinecraftVersion::new(1, 21, 9);
        let water = fluid_pool(FluidState::Water, (-4, 4), (9, 13), (-4, 4));
        let swum = |set: &dyn Fn(&mut RiddenMobState)| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Mule, Vec3::new(0.5, 10.0, 0.5));
            mob.in_water = true;
            mob.velocity.y = -0.1;
            set(&mut mob);
            tick_ridden_mob(&mut mob, &water, release);
            mob.velocity.y
        };
        let slowed = -0.1 * 0.800000011920929;
        assert_eq!(swum(&|_| {}), slowed - 0.08 / 16.0);
        assert_eq!(swum(&|mob| mob.slow_falling = true), slowed - 0.01 / 16.0);
        assert_eq!(swum(&|mob| mob.levitation_amplifier = Some(3)), swum(&|_| {}));
    }

    #[test]
    fn a_mounts_effects_stop_with_the_tick_they_are_gone_from() {
        // the client's instance is the mount's while it is on the mob: Levitation's and Slow
        // Falling's terms are read every tick (26.3 LivingEntity.java:2387-2397, 2337-2340)
        let release = MinecraftVersion::new(1, 21, 9);
        let drag = f64::from(0.98_f32);
        let world = WorldSnapshot::new();
        let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 50.0, 0.5));
        mob.levitation_amplifier = Some(1);
        for _ in 0..3 {
            tick_ridden_mob(&mut mob, &world, release);
        }
        let rising = mob.velocity.y;
        assert!(rising > 0.0);
        mob.levitation_amplifier = None;
        tick_ridden_mob(&mut mob, &world, release);
        assert_eq!(mob.velocity.y, (rising - 0.08) * drag);
        mob.velocity.y = -0.3;
        mob.slow_falling = true;
        tick_ridden_mob(&mut mob, &world, release);
        assert_eq!(mob.velocity.y, (-0.3 - 0.01) * drag);
        let slow = mob.velocity.y;
        mob.slow_falling = false;
        tick_ridden_mob(&mut mob, &world, release);
        assert_eq!(mob.velocity.y, (slow - 0.08) * drag);
    }

    #[test]
    fn a_happy_ghast_flies_past_the_terms_of_ordinary_travel() {
        // HappyGhast.travel is travelFlying, its drags fixed and no gravity in them, nor a modifier,
        // an effect or WATER_MOVEMENT_EFFICIENCY (26.3 HappyGhast.java:154-157, LivingEntity.java:
        // 2365-2379)
        let release = MinecraftVersion::new(26, 3, 0);
        let water = fluid_pool(FluidState::Water, (-6, 6), (9, 15), (-6, 6));
        for (world, in_water) in [(&WorldSnapshot::new(), false), (&water, true)] {
            let flown = |set: &dyn Fn(&mut RiddenMobState)| {
                let position = Vec3::new(0.5, 10.0, 0.5);
                let mut mob = RiddenMobState::new(RiddenMobKind::HappyGhast, position);
                mob.in_water = in_water;
                mob.velocity = Vec3::new(0.2, -0.1, 0.0);
                mob.rider.forward = 1.0;
                set(&mut mob);
                tick_ridden_mob(&mut mob, world, release);
                (mob.position, mob.velocity)
            };
            let plain = flown(&|_| {});
            let drag = if in_water { 0.800000011920929 } else { 0.9100000262260437 };
            assert!((plain.1.y - -0.1 * drag).abs() < 1.0e-3, "{}", plain.1.y);
            assert_eq!(flown(&|mob| mob.levitation_amplifier = Some(3)), plain);
            assert_eq!(flown(&|mob| mob.slow_falling = true), plain);
            assert_eq!(flown(&|mob| mob.gravity = Some(0.5)), plain);
            assert_eq!(flown(&|mob| mob.air_drag_modifier = Some(0.0)), plain);
            assert_eq!(flown(&|mob| mob.friction_modifier = Some(0.0)), plain);
            assert_eq!(flown(&|mob| mob.water_movement_efficiency = 1.0), plain);
            assert_eq!(flown(&|mob| mob.dolphins_grace = true), plain);
        }
    }

    #[test]
    fn a_nautilus_swims_past_the_terms_of_ordinary_water_and_falls_by_those_of_the_air() {
        // AbstractNautilus.travelInWater is the speed, the move and 0.9 (26.3 AbstractNautilus.java:
        // 203-208); out of water it takes travelInAir as any mob (LivingEntity.java:2382-2397)
        let release = MinecraftVersion::new(26, 3, 0);
        let water = fluid_pool(FluidState::Water, (-6, 6), (9, 15), (-6, 6));
        for kind in [RiddenMobKind::Nautilus, RiddenMobKind::ZombieNautilus] {
            let swum = |set: &dyn Fn(&mut RiddenMobState)| {
                let mut mob = RiddenMobState::new(kind, Vec3::new(0.5, 10.0, 0.5));
                mob.in_water = true;
                mob.velocity = Vec3::new(0.2, -0.1, 0.0);
                set(&mut mob);
                tick_ridden_mob(&mut mob, &water, release);
                mob.velocity
            };
            let plain = swum(&|_| {});
            assert_eq!(plain.y, -0.1 * 0.9, "{kind:?}");
            assert_eq!(swum(&|mob| mob.levitation_amplifier = Some(3)), plain, "{kind:?}");
            assert_eq!(swum(&|mob| mob.slow_falling = true), plain, "{kind:?}");
            assert_eq!(swum(&|mob| mob.gravity = Some(0.5)), plain, "{kind:?}");
            assert_eq!(swum(&|mob| mob.water_movement_efficiency = 1.0), plain, "{kind:?}");
            assert_eq!(swum(&|mob| mob.dolphins_grace = true), plain, "{kind:?}");
            let aired = |set: &dyn Fn(&mut RiddenMobState)| {
                let mut mob = RiddenMobState::new(kind, Vec3::new(0.5, 50.0, 0.5));
                set(&mut mob);
                tick_ridden_mob(&mut mob, &WorldSnapshot::new(), release);
                mob.velocity.y
            };
            let drag = f64::from(0.98_f32);
            assert_eq!(aired(&|_| {}), -0.08 * drag, "{kind:?}");
            assert_eq!(aired(&|mob| mob.gravity = Some(0.02)), -0.02 * drag, "{kind:?}");
            assert_eq!(
                aired(&|mob| mob.levitation_amplifier = Some(1)),
                0.1 * 0.2 * drag,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_mount_leaving_the_water_takes_the_air_terms_of_its_attributes() {
        // the same state in the water and out of it: Levitation and the drag modifier are of the
        // air alone, the efficiency of the water (26.3 LivingEntity.java:2382-2407, 2429-2453)
        let release = MinecraftVersion::new(26, 3, 0);
        let water = fluid_pool(FluidState::Water, (-4, 4), (9, 10), (-4, 4));
        let mut mob = RiddenMobState::new(RiddenMobKind::Mule, Vec3::new(0.5, 10.5, 0.5));
        mob.in_water = true;
        mob.velocity.y = 1.5;
        mob.levitation_amplifier = Some(1);
        mob.air_drag_modifier = Some(0.0);
        tick_ridden_mob(&mut mob, &water, release);
        assert!(mob.position.y > 11.0);
        let left = mob.velocity.y;
        assert_eq!(left, 1.5 * 0.800000011920929 - 0.08 / 16.0);
        tick_ridden_mob(&mut mob, &water, release);
        assert!(!mob.in_water);
        assert_eq!(mob.velocity.y, left + (0.1 - left) * 0.2);
    }

    #[test]
    fn a_mounts_default_attributes_given_are_the_attributes_not_given() {
        // the defaults of 26.3 Attributes.java: GRAVITY's 0.08, the modifiers' 1 (:10, 71) and no water
        // efficiency; STEP_HEIGHT at least 1 with a rider in control, a camel's own 1.5
        for release in [MinecraftVersion::new(1, 21, 11), MinecraftVersion::new(26, 3, 0)] {
            for (kind, step) in [(RiddenMobKind::Horse, 1.0), (RiddenMobKind::Camel, 1.5)] {
                let run = |given: bool| {
                    let world = fluid_pool(FluidState::Water, (2, 4), (10, 11), (-2, 2));
                    let mut floor = stone_floor((-4, 8), 9, (-4, 4));
                    floor.set_block((3, 10, 0), BlockState::default());
                    let mut mob = standing(kind);
                    mob.velocity.y = 0.1;
                    if given {
                        mob.gravity = Some(0.08);
                        mob.step_height = Some(step);
                        mob.water_movement_efficiency = 0.0;
                        mob.air_drag_modifier = Some(1.0);
                        mob.friction_modifier = Some(1.0);
                    }
                    for _ in 0..6 {
                        tick_ridden_mob(&mut mob, &floor, release);
                    }
                    for _ in 0..6 {
                        tick_ridden_mob(&mut mob, &world, release);
                    }
                    (mob.position, mob.velocity, mob.fall_distance)
                };
                assert_eq!(run(true), run(false), "{kind:?} {release}");
            }
        }
    }

    #[test]
    fn a_mounts_attributes_are_sanitized_as_the_attribute_is() {
        // RangedAttribute.sanitizeValue takes NaN to the minimum and clamps the rest (26.3
        // RangedAttribute.java:29-31): GRAVITY -1 to 1, the modifiers 0 to 2048, the water's 0 to 1
        // (26.3 Attributes.java:10, 71, 72, 91)
        let release = MinecraftVersion::new(26, 3, 0);
        let drag = f64::from(0.98_f32);
        let gravity = |value: f64| falling(release, |mob| mob.gravity = Some(value)).velocity.y;
        assert_eq!(gravity(f64::NAN), 1.0 * drag);
        assert_eq!(gravity(f64::INFINITY), -1.0 * drag);
        assert_eq!(gravity(f64::NEG_INFINITY), 1.0 * drag);
        let dragged = |value: f64| {
            let mob = falling(release, |mob| {
                mob.velocity.x = 0.5;
                mob.air_drag_modifier = Some(value);
            });
            (mob.velocity.x, mob.velocity.y)
        };
        let unmodified = dragged(0.0);
        assert_eq!(unmodified, (0.5, -0.08));
        assert_eq!(dragged(f64::NAN), unmodified);
        assert_eq!(dragged(-5.0), unmodified);
        assert_eq!(dragged(f64::INFINITY), dragged(2048.0));
        let floor = stone_floor((-2, 4), 9, (-2, 2));
        let rubbed = |value: f64| {
            let mut mob = walking_horse();
            mob.velocity.y = -0.0784;
            mob.friction_modifier = Some(value);
            tick_ridden_mob(&mut mob, &floor, release);
            mob.velocity.x
        };
        assert_eq!(rubbed(f64::NAN), rubbed(0.0));
        assert_eq!(rubbed(-1.0), rubbed(0.0));
        assert_eq!(rubbed(f64::INFINITY), rubbed(2048.0));
        let water = fluid_pool(FluidState::Water, (-4, 4), (9, 13), (-4, 4));
        let swum = |value: f64| {
            let mut mob = RiddenMobState::new(RiddenMobKind::Mule, Vec3::new(0.5, 10.0, 0.5));
            mob.in_water = true;
            mob.velocity.x = 0.2;
            mob.water_movement_efficiency = value;
            tick_ridden_mob(&mut mob, &water, release);
            mob.velocity.x
        };
        assert_eq!(swum(f64::NAN), swum(0.0));
        assert_eq!(swum(-1.0), swum(0.0));
        assert_eq!(swum(5.0), swum(1.0));
        assert!(swum(1.0).is_finite());
    }

    #[test]
    fn a_mount_steps_by_a_sanitized_attribute() {
        // 0 to 10 and NaN the minimum (26.3 Attributes.java:86, RangedAttribute.java:29-31),
        // at least 1 under a rider in control, and as it is under a mob's own control (26.3
        // LivingEntity.java:3974-3977)
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        let release = MinecraftVersion::new(1, 21, 9);
        let climbed = |kind, saddled: bool, height: i32, attribute: f64| {
            let mut world = stone_floor((-2, 6), 9, (-2, 2));
            for z in -2..=2 {
                for y in 10..(10 + height) {
                    world.set_block((2, y, z), stone);
                }
            }
            let mut mob = standing(kind);
            mob.saddled = saddled;
            mob.position = Vec3::new(1.0, 10.0, 0.5);
            mob.velocity = Vec3::new(0.6, 0.0, 0.0);
            mob.rider.forward = 0.0;
            mob.step_height = Some(attribute);
            tick_ridden_mob(&mut mob, &world, release);
            mob.position.y
        };
        assert_eq!(climbed(RiddenMobKind::Horse, true, 1, f64::NAN), 11.0);
        assert_eq!(climbed(RiddenMobKind::Horse, true, 2, f64::INFINITY), 12.0);
        assert_eq!(climbed(RiddenMobKind::Horse, true, 2, 1.0), 10.0);
        // an unsteered pig has nobody in control
        assert_eq!(climbed(RiddenMobKind::Pig, false, 1, 0.0), 10.0);
        assert_eq!(climbed(RiddenMobKind::Pig, false, 1, f64::NAN), 10.0);
        assert_eq!(climbed(RiddenMobKind::Pig, false, 1, 1.0), 11.0);
    }

    #[test]
    fn a_mounts_drag_and_friction_modifiers_are_of_26_2() {
        // travelInAir's computeModifiedFriction (26.3 LivingEntity.java:2382-2407); 26.1 has none
        let drifted = |modifier: Option<f64>, release| {
            let mob = falling(release, |mob| {
                mob.velocity.x = 0.5;
                mob.air_drag_modifier = modifier;
            });
            mob.velocity.x
        };
        let (new, old) = (MinecraftVersion::new(26, 2, 0), MinecraftVersion::new(26, 1, 0));
        assert_eq!(drifted(None, new), 0.5 * f64::from(0.91_f32));
        assert_eq!(drifted(Some(0.0), new), 0.5);
        assert_eq!(drifted(Some(0.0), old), 0.5 * f64::from(0.91_f32));
    }

    /// B290: each test runs a gate on the two releases either side of its boundary, so moving the
    /// gate one release either way fails here.
    mod gate_boundaries {
        use super::super::*;
        use super::{fluid_pool, ridden, stone_floor, water_with};
        use crate::world::{BlockShape, BlockState};

        const fn v(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
            MinecraftVersion::new(major, minor, patch)
        }

        fn stone() -> BlockState {
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            }
        }

        #[test]
        fn a_donkey_is_lower_from_1_14_4_and_a_scaled_mob_loses_its_fluid_jump_from_1_20_5() {
            // 1.13.2 AbstractHorse.java:148 setSize(1.3964844F, 1.6F) for every horse; 1.14.4
            // EntityType.java:514 sizes the donkey 1.3964844F x 1.5F
            assert_eq!(RiddenMobKind::Donkey.dimensions(v(1, 13, 2)), (1.3964844, 1.6));
            assert_eq!(RiddenMobKind::Donkey.dimensions(v(1, 14, 4)), (1.3964844, 1.5));
            // Entity.getFluidJumpThreshold: eyes under 0.4 give 0 (1.20.5 Entity.java:3183-3185), and
            // from 1.20.5 SCALE scales the eyes (EntityDimensions.java:40): a pig scaled to 0.4 high
            // has its 0.765 eyes at 0.306
            let mut pig = RiddenMobState::new(RiddenMobKind::Pig, Vec3::new(0.5, 100.0, 0.5));
            pig.scale = (1.0, 0.4);
            assert_eq!(pig.fluid_jump_threshold(v(1, 20, 5)), 0.0);
            pig.scale = (1.0, 1.0);
            assert_eq!(pig.fluid_jump_threshold(v(1, 20, 5)), 0.4);
        }

        #[test]
        fn a_baby_loses_its_fluid_jump_from_1_16_not_from_1_20_5() {
            // 1.15.2 LivingEntity.java:2295 compares the fluid with 0.4 for every mob; from 1.16
            // getFluidJumpThreshold reads the eyes (1.16 Entity.java:2664-2666, 1.20.4 :3148-3150), and
            // a baby's are those of its dimensions scaled by 0.5 (1.20.4 LivingEntity.java:3185-3186,
            // Entity.java:2802-2804): a baby pig's 0.765 eyes are at 0.3825
            let mut pig = RiddenMobState::new(RiddenMobKind::Pig, Vec3::new(0.5, 100.0, 0.5));
            pig.scale = (0.5, 0.5);
            assert_eq!(pig.fluid_jump_threshold(v(1, 15, 2)), 0.4);
            assert_eq!(pig.fluid_jump_threshold(v(1, 16, 0)), 0.0);
            assert_eq!(pig.fluid_jump_threshold(v(1, 20, 4)), 0.0);
            assert_eq!(pig.fluid_jump_threshold(v(1, 20, 5)), 0.0);
            // an adult's eyes are over 0.4 on every release
            pig.scale = (1.0, 1.0);
            assert_eq!(pig.fluid_jump_threshold(v(1, 16, 0)), 0.4);
            assert_eq!(pig.fluid_jump_threshold(v(1, 20, 4)), 0.4);
            // a baby horse's 1.52 eyes at 0.76 keep the 0.4 (1.20.4 AbstractHorse.java:1005-1007)
            let mut foal = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 100.0, 0.5));
            foal.scale = (0.5, 0.5);
            assert_eq!(foal.fluid_jump_threshold(v(1, 20, 4)), 0.4);
        }

        #[test]
        fn a_strider_without_ai_keeps_its_suffocation_from_1_19() {
            // 1.18.2 Strider.java:303-306 sets suffocating every tick; 1.19 Strider.java:306-311
            // only with its AI
            let world = stone_floor((-3, 3), 100, (-3, 3));
            let cold = |release| {
                let mut strider =
                    ridden(RiddenMobKind::Strider, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
                strider.on_ground = true;
                strider.no_ai = true;
                tick_ridden_mob(&mut strider, &world, release);
                strider.suffocating
            };
            assert!(cold(v(1, 18, 2)));
            assert!(!cold(v(1, 19, 0)));
        }

        #[test]
        fn a_steered_horse_checks_the_blocks_inside_again_from_1_17() {
            // AbstractHorse.travel's steered branch calls tryCheckBlockCollision after the move from 1.17
            // (1.17 HorseBaseEntity.java:765; none in 1.16.5 :748-800): a bubble column pushes once more
            let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
            world.set_block((0, 101, 0), water_with(BlockEffect::BubbleUp));
            let rise = |release| {
                let mut mob = ridden(RiddenMobKind::Horse, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
                tick_ridden_mob(&mut mob, &world, release);
                mob.velocity.y
            };
            assert_eq!(rise(v(1, 17, 0)), rise(v(1, 16, 5)) + 0.06);
        }

        #[test]
        fn a_slow_falling_mob_pushed_up_in_its_move_snaps_from_1_14_alone() {
            // getFluidFallingAdjustedMovement's falling flag: 1.13.2 tests the motion after the move and
            // the drag (LivingEntity.java:1672-1676), 1.14 the motion before travel (1.14.4
            // LivingEntity.java:1760). Falling at -0.059 into a bubble column, the move's push leaves a
            // little upward motion: 1.14.4 snaps it to -0.003 (|y - 0.01 / 16| < 0.003), 1.13.2 does not
            let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
            world.set_block((0, 101, 0), water_with(BlockEffect::BubbleUp));
            let after = |release| {
                let mut mob = ridden(RiddenMobKind::Pig, Vec3::new(0.5, 101.0, 0.5), Vec3::new(0.0, -0.059, 0.0), 270.0);
                mob.slow_falling = true;
                tick_ridden_mob(&mut mob, &world, release);
                mob.velocity.y
            };
            assert_eq!(after(v(1, 14, 4)), -0.003);
            assert!(after(v(1, 13, 2)) > 0.0);
        }

        #[test]
        fn a_sitting_camel_in_water_stands_from_1_20() {
            // 1.20 Camel.tick stands a sitting camel in water up at once (Camel.java:190-191); 1.19.4 has
            // no such test (Camel.java:185-187)
            let pool = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
            let sits = |release| {
                let mut camel = ridden(RiddenMobKind::Camel, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
                camel.sitting = true;
                camel.pose_time = 100;
                tick_ridden_mob(&mut camel, &pool, release);
                camel.sitting
            };
            assert!(sits(v(1, 19, 4)));
            assert!(!sits(v(1, 20, 0)));
        }

        #[test]
        fn the_steered_and_strider_checks_of_the_blocks_inside_at_their_boundaries() {
            // the bubble column of strider_checks_the_blocks_inside_in_its_fall_and_tick_until_1_21_2:
            // 1.16 Strider.java:295-296 (fall) and :318 (tick) from the strider's first release;
            // ItemSteerable.travel's own check from 1.17 (1.17 ItemSteerable.java:48) to 1.19.3
            // (1.19.3 :48; none in 1.16.5 or 1.19.4)
            let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
            world.set_block((0, 101, 0), water_with(BlockEffect::BubbleUp));
            let rise = |kind: RiddenMobKind, release: MinecraftVersion| {
                let mut mob = ridden(kind, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
                tick_ridden_mob(&mut mob, &world, release);
                mob.velocity.y
            };
            let swim = |pushes_in_move: f64| pushes_in_move * 0.06 * 0.800000011920929 - 0.08 / 16.0;
            assert_eq!(rise(RiddenMobKind::Strider, v(1, 16, 0)), swim(2.0) + 0.06);
            assert_eq!(rise(RiddenMobKind::Strider, v(1, 17, 0)), swim(2.0) + 0.06 + 0.06);
            assert_eq!(rise(RiddenMobKind::Pig, v(1, 16, 5)), swim(1.0));
            assert_eq!(rise(RiddenMobKind::Pig, v(1, 17, 0)), swim(1.0) + 0.06);
            assert_eq!(rise(RiddenMobKind::Pig, v(1, 19, 3)), swim(1.0) + 0.06);
            assert_eq!(rise(RiddenMobKind::Pig, v(1, 19, 4)), swim(1.0));
        }

        #[test]
        fn a_landed_horse_still_jumping_waits_for_its_flag_until_1_19_4() {
            // 1.19.3 AbstractHorse.java:757-760 jumps only when !isJumping(); 1.19.4
            // AbstractHorse.java:760-764 clears the flag on the ground first, then jumps
            let world = stone_floor((-3, 3), 100, (-3, 3));
            let jumped = |release| {
                let mut horse =
                    ridden(RiddenMobKind::Horse, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
                horse.on_ground = true;
                horse.horse_jumping = true;
                horse.jump_pending_scale = 1.0;
                horse.jump_strength = 0.7;
                tick_ridden_mob(&mut horse, &world, release);
                horse.position.y > 101.0
            };
            assert!(!jumped(v(1, 19, 3)));
            assert!(jumped(v(1, 19, 4)));
        }

        #[test]
        fn a_riding_jump_takes_the_honey_jump_factor_from_1_15_and_a_float_power_from_1_20_5() {
            // 1.14.4 AbstractHorse.java:664 getCustomJump() * scale; 1.15 AbstractHorse.java:744
            // times getBlockJumpFactor(), which honey halves
            let mut world = WorldSnapshot::new();
            world.set_block(
                (0, 100, 0),
                BlockState {
                    shape: Some(BlockShape {
                        effect: BlockEffect::Honey,
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            let body = || SimulationState {
                position: Vec3::new(0.5, 101.0, 0.5),
                half_width: 0.6982421875,
                height: 1.6,
                on_ground: true,
                ..SimulationState::default()
            };
            let horse_jump_y = |release| {
                let mut horse = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 101.0, 0.5));
                horse.jump_strength = 0.7;
                horse.jump_pending_scale = 1.0;
                let mut body = body();
                horse_jump(&mut horse, &mut body, &world, None, release, 0.0);
                body.velocity.y
            };
            assert_eq!(horse_jump_y(v(1, 14, 4)), 0.7);
            assert_eq!(horse_jump_y(v(1, 15, 0)), 0.7 * 0.5);
            // Camel.executeRidersJump: 1.4285F * scale * the double jump power until 1.20.4
            // (1.20.4 Camel.java:287), the float getJumpPower() from 1.20.5 (1.20.5 Camel.java:256,
            // LivingEntity.java:2059-2064)
            let floor = stone_floor((-3, 3), 100, (-3, 3));
            let camel_dash_y = |release| {
                let mut camel = RiddenMobState::new(RiddenMobKind::Camel, Vec3::new(0.5, 101.0, 0.5));
                camel.jump_strength = 0.42;
                camel.jump_pending_scale = 1.0;
                let mut body = SimulationState {
                    half_width: 0.85,
                    height: 2.375,
                    ..body()
                };
                camel_dash(&mut camel, &mut body, &floor, None, release);
                body.velocity.y
            };
            let lift = f64::from(1.4285_f32);
            assert_eq!(camel_dash_y(v(1, 20, 4)), lift * 0.42);
            assert_eq!(camel_dash_y(v(1, 20, 5)), lift * f64::from(0.42_f32));
        }

        #[test]
        fn slow_falling_reads_the_motion_the_move_left_on_1_21_2_itself() {
            // the ceiling of slow_falling_reads_the_motion_the_move_left_from_1_21_2 at the boundary:
            // 1.21.1 LivingEntity.java:2117-2121 the motion travel began with, 1.21.2
            // LivingEntity.java:2194-2197 the motion the move left
            let world = stone_floor((-2, 2), 12, (-2, 2));
            let rose = |release| {
                let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 10.3, 0.5));
                mob.velocity.y = 0.2;
                mob.slow_falling = true;
                tick_ridden_mob(&mut mob, &world, release);
                mob.velocity.y
            };
            let drag = f64::from(0.98_f32);
            assert_eq!(rose(v(1, 21, 1)), -0.08 * drag);
            assert_eq!(rose(v(1, 21, 2)), -0.01 * drag);
        }

        #[test]
        fn the_step_height_attribute_is_read_from_1_20_5() {
            // 1.20.5 LivingEntity.java:3507-3510 maxUpStep is the STEP_HEIGHT attribute, at least 1
            // with a player in control; 1.20.4 AbstractHorse has the fixed 1.0
            let mut horse = RiddenMobState::new(RiddenMobKind::Horse, Vec3::ZERO);
            horse.step_height = Some(2.0);
            assert_eq!(step_height(&horse, v(1, 20, 4)), 1.0);
            assert_eq!(step_height(&horse, v(1, 20, 5)), 2.0);
        }

        fn plain() -> PlainMob {
            PlainMob {
                yaw: 0.0,
                max_step: 0.6,
                pushed_by_fluid: true,
                fluid_jump_threshold: 0.4,
                climbing: None,
                water_slow_down: None,
                omnidirectional: false,
                slow_falling: false,
                levitation_amplifier: None,
                dolphins_grace: false,
                gravity: None,
                water_movement_efficiency: 0.0,
                no_fall_check: false,
                inside_after_move: false,
                discard_friction: false,
            }
        }

        fn plain_body(position: Vec3, velocity: Vec3) -> SimulationState {
            SimulationState {
                position,
                velocity,
                half_width: 0.45,
                height: 0.9,
                ..SimulationState::default()
            }
        }

        #[test]
        fn levitation_lifts_a_mob_from_1_9() {
            // Levitation is of 1.9 (1.9.4 LivingEntity.java:1417-1418 in place of gravity); 1.8.8
            // LivingEntity.java:1556-1560 pulls by 0.08 alone
            let travelled = |release, levitation| {
                let mut body = plain_body(Vec3::new(0.5, 50.0, 0.5), Vec3::ZERO);
                let mob = PlainMob {
                    levitation_amplifier: levitation,
                    ..plain()
                };
                let empty = WorldSnapshot::new();
                plain_living_travel(RiddenMobKind::Pig, body.position, 0.1, Vec3::ZERO, &mut body, &empty, release, mob);
                body.velocity.y
            };
            assert_eq!(travelled(v(1, 8, 8), Some(0)), travelled(v(1, 8, 8), None));
            assert_eq!(travelled(v(1, 9, 4), Some(0)), 0.05 * 0.2 * f64::from(0.98_f32));
        }

        fn falling(release: MinecraftVersion, set: impl FnOnce(&mut RiddenMobState)) -> RiddenMobState {
            let mut mob = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 50.0, 0.5));
            mob.fall_distance = 5.0;
            set(&mut mob);
            tick_ridden_mob(&mut mob, &WorldSnapshot::new(), release);
            mob
        }

        #[test]
        fn the_fall_distance_under_slow_falling_and_levitation_at_their_boundaries() {
            // Slow Falling is of 1.13: 1.13.2 LivingEntity.java:1511-1513 resets a falling mob's fall
            // distance before travel, 1.12.2 has no such effect
            let plain_fall = falling(v(1, 12, 2), |_| {}).fall_distance;
            assert_eq!(falling(v(1, 12, 2), |mob| mob.slow_falling = true).fall_distance, plain_fall);
            assert!(falling(v(1, 13, 2), |mob| mob.slow_falling = true).fall_distance < 1.0);
            // 1.19.4 :1986-1990 only while falling; 1.20 LivingEntity.java:2578-2579 under either
            // effect, rising too
            let rising = |release| {
                falling(release, |mob| {
                    mob.slow_falling = true;
                    mob.velocity.y = 0.5;
                })
                .fall_distance
            };
            assert_eq!(rising(v(1, 19, 4)), 5.0);
            assert_eq!(rising(v(1, 20, 0)), 0.0);
            // Levitation resets the fall after the move 1.13 - 1.19.4 (1.13.2 :1612, 1.19.4 :2105);
            // 1.12.2 never, 1.20 before the move (1.20 :2578-2579), so the drop of the move counts
            let levitating = |release| {
                falling(release, |mob| {
                    mob.levitation_amplifier = Some(0);
                    mob.velocity.y = -0.5;
                })
                .fall_distance
            };
            // (the drop is the 0.98 the tick fades the motion by before 1.21.5, counted in float)
            assert_eq!(levitating(v(1, 12, 2)), f64::from(5.0_f32 + 0.49_f32));
            assert_eq!(levitating(v(1, 13, 2)), 0.0);
            assert_eq!(levitating(v(1, 19, 4)), 0.0);
            assert_eq!(levitating(v(1, 20, 0)), f64::from(0.49_f32));
        }

        #[test]
        fn the_drag_modifier_is_of_26_2() {
            // AIR_DRAG_MODIFIER arrives in 26.2 (26.2 Attributes.java:10; 26.1.2 Attributes.java has
            // none)
            let drifted = |release, modifier| {
                falling(release, |mob| {
                    mob.velocity.x = 0.5;
                    mob.air_drag_modifier = modifier;
                })
                .velocity
            };
            assert_eq!(drifted(v(26, 1, 2), Some(2.0)), drifted(v(26, 1, 2), None));
            assert_ne!(drifted(v(26, 2, 0), Some(2.0)), drifted(v(26, 2, 0), None));
        }

        #[test]
        fn a_ladder_holds_a_fall_to_a_float_from_1_14() {
            // 1.13.2 LivingEntity.java:1594-1596 keeps motionY to -0.15 (a double); 1.14.4
            // LivingEntity.java:1921-1923 to -0.15F
            let mut ladder = WorldSnapshot::new();
            ladder.set_block(
                (0, 105, 2173),
                BlockState {
                    shape: Some(BlockShape {
                        effect: BlockEffect::Climbable,
                        bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            let fallen = |release| {
                let mut pig = ridden(
                    RiddenMobKind::Pig,
                    Vec3::new(0.5, 105.0, 2173.5),
                    Vec3::new(0.0, -0.5, 0.0),
                    270.0,
                );
                tick_ridden_mob(&mut pig, &ladder, release);
                pig.position.y
            };
            assert_eq!(fallen(v(1, 13, 2)), 105.0 - 0.15);
            assert_eq!(fallen(v(1, 14, 4)), 105.0 - f64::from(0.15_f32));
        }

        #[test]
        fn water_movement_efficiency_and_the_climb_out_of_water_at_their_boundaries() {
            // WATER_MOVEMENT_EFFICIENCY is of 1.21 (1.21 LivingEntity.java:2130-2137; 1.20.6 has no
            // such attribute)
            let world = fluid_pool(FluidState::Water, (-4, 4), (98, 106), (-4, 4));
            let swum = |release, efficiency| {
                let mut horse =
                    ridden(RiddenMobKind::Horse, Vec3::new(0.5, 101.0, 0.5), Vec3::ZERO, 270.0);
                horse.rider.forward = 1.0;
                horse.water_movement_efficiency = efficiency;
                tick_ridden_mob(&mut horse, &world, release);
                horse.velocity
            };
            assert_eq!(swum(v(1, 20, 6), 1.0), swum(v(1, 20, 6), 0.0));
            assert_ne!(swum(v(1, 21, 0), 1.0), swum(v(1, 21, 0), 0.0));
            // the climb out of water against a wall on a climbable block: none in 1.13.2
            // LivingEntity.java:1646-1684, 0.2 in 1.14.4 :1880-1882
            let mut world = fluid_pool(FluidState::Water, (-4, 4), (98, 104), (-4, 4));
            world.set_block((0, 101, 0), water_with(BlockEffect::Climbable));
            world.set_block((1, 101, 0), stone());
            world.set_block((1, 102, 0), stone());
            let climbed = |release| {
                let mut pig =
                    ridden(RiddenMobKind::Pig, Vec3::new(0.5, 101.0, 0.5), Vec3::new(0.2, 0.0, 0.0), 270.0);
                tick_ridden_mob(&mut pig, &world, release);
                assert!(pig.collided_horizontally);
                pig.velocity.y
            };
            assert_eq!(climbed(v(1, 13, 2)), -0.08 / 16.0);
            assert_eq!(climbed(v(1, 14, 4)), 0.2 * 0.800000011920929 - 0.08 / 16.0);
        }

        #[test]
        fn the_lava_add_turns_a_negative_zero_positive_from_1_14() {
            // 1.13.2 LivingEntity.java:1602-1603 subtracts from motionY alone; 1.14.4
            // LivingEntity.java:1907-1908 setDeltaMovement(getDeltaMovement().add(0.0, -0.02, 0.0)),
            // whose 0.0 makes x and z +0.0
            let lava = fluid_pool(FluidState::Lava, (-4, 4), (98, 104), (-4, 4));
            let signs = |release| {
                let mut body = plain_body(Vec3::new(0.5, 100.0, 0.5), Vec3::new(-0.0, 0.0, 0.0));
                body.in_lava = true;
                plain_living_travel(
                    RiddenMobKind::Pig,
                    body.position,
                    0.1,
                    Vec3::new(-0.0, 0.0, 1.0),
                    &mut body,
                    &lava,
                    release,
                    plain(),
                );
                body.velocity.x.is_sign_negative()
            };
            assert!(signs(v(1, 13, 2)));
            assert!(!signs(v(1, 14, 4)));
        }

        #[test]
        fn a_horse_floats_by_the_water_after_its_lava_travel_from_26_3() {
            // floatInWaterWhileRidden after the water's travel alone in 26.2, after the lava's too in
            // 26.3 (26.3 LivingEntity.java:2416-2427, 2488-2493), by the water the move left it in
            // (26.3 :326-328)
            let mut beside = fluid_pool(FluidState::Lava, (-6, -1), (98, 104), (-4, 4));
            let water = fluid_pool(FluidState::Water, (0, 4), (98, 104), (-4, 4));
            for x in 0..=4 {
                for y in 98..=104 {
                    for z in -4..=4 {
                        beside.set_block((x, y, z), water.block((x, y, z)));
                    }
                }
            }
            let all_lava = fluid_pool(FluidState::Lava, (-6, 4), (98, 104), (-4, 4));
            let crossed = |world: &WorldSnapshot, release| {
                let mut horse =
                    ridden(RiddenMobKind::Horse, Vec3::new(-2.5, 100.0, 0.5), Vec3::new(3.0, 0.0, 0.0), 270.0);
                tick_ridden_mob(&mut horse, world, release);
                horse.velocity.y
            };
            assert_eq!(crossed(&beside, v(26, 2, 0)), crossed(&all_lava, v(26, 2, 0)));
            assert_ne!(crossed(&beside, v(26, 3, 0)), crossed(&all_lava, v(26, 3, 0)));
        }

        #[test]
        fn a_vex_takes_the_blocks_inside_after_its_move_until_1_21_2() {
            // 1.21.1 Vex.java:66-69 checks the blocks inside after a noPhysics move; 1.21.2
            // Vex.java:68-74 has no such override
            let mut web = WorldSnapshot::new();
            web.set_block(
                (0, 50, 0),
                BlockState {
                    shape: Some(BlockShape {
                        effect: BlockEffect::Web,
                        bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            let stuck = |release| {
                let mut body = plain_body(Vec3::new(0.5, 50.0, 0.5), Vec3::ZERO);
                body.no_physics = true;
                let mob = PlainMob {
                    inside_after_move: true,
                    ..plain()
                };
                plain_living_travel(RiddenMobKind::Pig, body.position, 0.1, Vec3::ZERO, &mut body, &web, release, mob);
                body.stuck_speed_multiplier
            };
            assert_ne!(stuck(v(1, 21, 1)), Vec3::ZERO);
            assert_eq!(stuck(v(1, 21, 2)), Vec3::ZERO);
        }

        #[test]
        fn the_lava_a_mob_is_in_at_the_lava_boundaries() {
            // Entity.isInLava: a world query before 1.14 (1.13.2 Entity.java:1143-1145); the move's
            // field from 1.14 (1.14.4 Entity.java:568-569, 1168-1173), set by any lava block of the
            // box (LiquidBlock.java:158-161) until 1.15.2, by one whose top is above the feet in
            // 1.16 - 1.16.1 (1.16 LiquidBlock.java:173-180); the base tick's lava height from
            // 1.16.2 (1.16.2 Entity.java:1178-1179). A stale inLava of a tick that has no lava:
            let stale = |release| falling(release, |mob| mob.in_lava = true).velocity.y;
            let air = -0.08 * f64::from(0.98_f32);
            assert_eq!(stale(v(1, 13, 2)), air);
            assert_ne!(stale(v(1, 14, 4)), air);
            assert_ne!(stale(v(1, 16, 1)), air);
            assert_eq!(stale(v(1, 16, 2)), air);
            // feet above the top of the lava of their cell
            let mut lava = WorldSnapshot::new();
            lava.set_block(
                (0, 100, 0),
                BlockState {
                    fluid: FluidState::Lava,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
            let hovering = |release| {
                let mut horse =
                    ridden(RiddenMobKind::Horse, Vec3::new(0.5, 100.95, 0.5), Vec3::ZERO, 270.0);
                horse.no_gravity = true;
                tick_ridden_mob(&mut horse, &lava, release);
                assert_eq!(horse.position.y, 100.95);
                horse.in_lava
            };
            assert!(hovering(v(1, 15, 2)));
            assert!(!hovering(v(1, 16, 0)));
        }

        #[test]
        fn the_flow_of_lava_pushes_a_mob_from_1_16_and_in_water_from_1_16_2() {
            // 1.16 Entity.java:1039-1048 pushes by the lava's flow, 1.15.2 Entity.java has no lava
            // push; in water 1.16 returns before the lava, 1.16.2 Entity.java:1064-1070 measures and
            // pushes by it too
            let flowing = |flow: Vec3| BlockState {
                fluid: FluidState::Lava,
                fluid_level: 4,
                fluid_flow: flow,
                ..BlockState::default()
            };
            let world = |flow: Vec3, water: bool| {
                let mut world = WorldSnapshot::new();
                for x in -2..=2 {
                    for z in -2..=2 {
                        world.set_block((x, 99, z), stone());
                        world.set_block((x, 100, z), flowing(flow));
                    }
                }
                if water {
                    world.set_block(
                        (0, 100, 0),
                        BlockState {
                            fluid: FluidState::Water,
                            fluid_source: true,
                            ..BlockState::default()
                        },
                    );
                }
                world
            };
            let pushed = |release, water| {
                let moved = |flow| {
                    // (moving, so the 1.15.2 push is not under the 0.003 a still motion drops)
                    let mut horse = ridden(
                        RiddenMobKind::Horse,
                        Vec3::new(0.9, 100.0, 0.5),
                        Vec3::new(0.1, 0.0, 0.0),
                        270.0,
                    );
                    tick_ridden_mob(&mut horse, &world(flow, water), release);
                    horse.velocity
                };
                moved(Vec3::new(1.0, 0.0, 0.0)) != moved(Vec3::ZERO)
            };
            assert!(!pushed(v(1, 15, 2), false));
            assert!(pushed(v(1, 16, 0), false));
            assert!(!pushed(v(1, 16, 0), true));
            assert!(pushed(v(1, 16, 2), true));
        }
    }
}
