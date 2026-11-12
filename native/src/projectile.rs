use crate::block_traversal::{blocks_visited, Movement};
use crate::math::{ceil_i32_for, floor_i32_for, frac_f64_for, java_float, Aabb, HitFace, Vec3};
use crate::legacy_fluid;
use crate::projectile_hits::{
    return_to_owner, Found, HitScene, ProjectileBlockHit, ProjectileHitReport, ProjectileHitState,
    ProjectileOwner, ProjectileTarget, RocketHolder,
};
use crate::ridden::view_vector;
use crate::vehicle::lava_flow_scale;
use crate::simulation::{
    blocks_inside, entity_fluid_contact, position_of_bounding_box, push_by_fluid, water_contact,
    SimulationState,
};
use crate::vehicle::{is_water, move_vehicle, water_height, MoveRules};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, FluidState, WorldSnapshot};

/// The release the unversioned projectile ticks follow.
const UNVERSIONED_RELEASE: MinecraftVersion = MinecraftVersion::new(26, 2, 0);

/// EntityType's 0.25F x 0.25F box of the thrown items, the fishing hook, llama spit and the
/// firework rocket.
const PROJECTILE_SIZE: f32 = 0.25;

/// What Entity.move collides a fishing hook or a firework rocket with: no step, and from 26.2
/// Entity's restitution (0.98F air drag, no bounciness; the gravity move_projectile gives).
const PROJECTILE_MOVE: MoveRules = MoveRules {
    living: false,
    max_step: 0.0,
    gravity: 0.0,
    air_drag: 0.98_f32 as f64,
    bounciness: 0.0,
    stands_on_lava: false,
    checks_inside_at_fall: false,
    rails_keep_speed: false,
    ignored: [None, None],
    skips_restitution: false,
    counts_fall: false,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileKind {
    Generic,
    Snowball,
    Egg,
    EnderPearl,
    Potion,
    ExperienceBottle,
    Firework,
    FishingHook,
    SpectralArrow,
    TippedArrow,
    Arrow,
    Trident,
    SmallFireball,
    Fireball,
    WitherSkull,
    DragonFireball,
    /// AbstractWindCharge (WindCharge from 1.20.3, BreezeWindCharge from 1.20.5).
    WindCharge,
    LlamaSpit,
    ShulkerBullet,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub gravity: f64,
    pub air_drag: f64,
    /// AbstractHurtingProjectile's xPower/yPower/zPower, added to the motion before 1.21.
    pub power: Vec3,
    /// AbstractHurtingProjectile.accelerationPower (1.21+), added along the motion's direction.
    pub acceleration_power: f64,
    pub in_water: bool,
    /// The dimension is ultrawarm: lava pushes at 0.007.
    pub ultra_warm: bool,
    pub first_tick: bool,
    pub kind: ProjectileKind,
    pub horizontal_collision: bool,
    pub on_ground: bool,
    pub shot_at_angle: bool,
    /// A fishing hook in FishingHook's BOBBING state (FLYING otherwise).
    pub bobbing: bool,
    /// The fishing hook's random.nextFloat() for this tick's bobbing pull.
    pub bob_random: f32,
    /// Entity.noPhysics, as Entity.move and the firework rocket's hit test read it.
    pub no_physics: bool,
    /// The box a rocket or hook keeps from tick to tick before 1.17, which its Entity.move goes by
    /// and its position follows (None: from the position).
    pub bounding_box: Option<Aabb>,
    /// Entity.stuckSpeedMultiplier a cobweb, a sweet berry bush or powder snow left on a hook or a
    /// rocket, which its next Entity.move takes and clears (26.3 Entity.java:783-789).
    pub stuck_speed_multiplier: Vec3,
}

impl ProjectileState {
    #[must_use]
    pub const fn new(position: Vec3, velocity: Vec3) -> Self {
        Self::for_kind(ProjectileKind::Generic, position, velocity)
    }

    #[must_use]
    pub const fn for_kind(kind: ProjectileKind, position: Vec3, velocity: Vec3) -> Self {
        Self {
            position,
            velocity,
            gravity: kind.default_gravity(),
            air_drag: kind.default_air_drag(),
            power: Vec3::ZERO,
            acceleration_power: kind.default_acceleration_power(),
            in_water: false,
            ultra_warm: false,
            first_tick: true,
            kind,
            horizontal_collision: false,
            on_ground: false,
            shot_at_angle: false,
            bobbing: false,
            bob_random: 0.0,
            no_physics: false,
            bounding_box: None,
            stuck_speed_multiplier: Vec3::ZERO,
        }
    }

    /// `for_kind` with the gravity `release` gives the family.
    #[must_use]
    pub fn for_kind_in_release(
        kind: ProjectileKind,
        position: Vec3,
        velocity: Vec3,
        release: MinecraftVersion,
    ) -> Self {
        Self {
            gravity: kind.default_gravity_in(release),
            ..Self::for_kind(kind, position, velocity)
        }
    }
}

impl ProjectileKind {
    #[must_use]
    /// (1.20.5 ThrownExperienceBottle.java:32 0.07; a float before, 1.20.4 :34, by default_gravity_in)
    pub const fn default_gravity(self) -> f64 {
        match self {
            Self::Generic | Self::Snowball | Self::Egg | Self::EnderPearl => 0.03,
            Self::Potion => 0.05,
            Self::ExperienceBottle => 0.07,
            Self::Firework => 0.0,
            Self::FishingHook => 0.03,
            Self::SpectralArrow | Self::TippedArrow | Self::Arrow | Self::Trident => 0.05,
            Self::SmallFireball
            | Self::Fireball
            | Self::WitherSkull
            | Self::DragonFireball
            | Self::WindCharge => 0.0,
            Self::LlamaSpit => 0.06,
            Self::ShulkerBullet => 0.04,
        }
    }

    /// The gravity of `release`: before 1.20.5 getGravity returns a float (0.03F, the potion's
    /// 0.05F, the experience bottle's 0.07F, LlamaSpit's 0.06F) widened to double
    /// (ThrowableProjectile.java:91 in 1.20.4, :91 in 1.20.5); the shulker bullet subtracts a double
    /// literal in every release, and the fishing hook until 26.2 (26.2 FishingHook.java:227), when
    /// 26.3's getDefaultGravity gives the float's value (26.3 FishingHook.java:468-470).
    #[must_use]
    pub fn default_gravity_in(self, release: MinecraftVersion) -> f64 {
        let gravity = self.default_gravity();
        let double = match self {
            Self::FishingHook => !release.at_least(26, 3, 0),
            Self::ShulkerBullet => true,
            _ => release.at_least(1, 20, 5),
        };
        if double {
            gravity
        } else {
            java_float(gravity)
        }
    }

    #[must_use]
    pub const fn default_air_drag(self) -> f64 {
        match self {
            // AbstractWindCharge.getInertia: 1.0F
            Self::Firework | Self::ShulkerBullet | Self::WindCharge => 1.0,
            // AbstractHurtingProjectile.getInertia (getDrag before 1.21.2): 0.95F
            Self::SmallFireball
            | Self::Fireball
            | Self::WitherSkull
            | Self::DragonFireball => 0.95_f32 as f64,
            Self::FishingHook => 0.92,
            _ => 0.99_f32 as f64,
        }
    }

    /// AbstractHurtingProjectile.accelerationPower (1.21+): 0.1, and 0.0 for the wind charges
    /// (1.21 AbstractWindCharge.java:34-45).
    #[must_use]
    pub const fn default_acceleration_power(self) -> f64 {
        match self {
            Self::WindCharge => 0.0,
            _ => 0.1,
        }
    }

    #[must_use]
    pub const fn is_arrow(self) -> bool {
        matches!(
            self,
            Self::Arrow | Self::SpectralArrow | Self::TippedArrow | Self::Trident
        )
    }

    /// Whether the family's tick moves before its base tick from 1.21.2 (`base_tick_opens`): the
    /// throwables, the hurting projectiles and the arrows. FireworkRocketEntity, FishingHook,
    /// LlamaSpit and ShulkerBullet open with `super.tick()` in every release (1.8.8
    /// FireworkRocketEntity.java:87, FishingHook.java:177, 1.16.5 LlamaSpit.java:48,
    /// ShulkerBullet.java:200; 26.3 FireworkRocketEntity.java:108, FishingHook.java:146,
    /// LlamaSpit.java:38, ShulkerBullet.java:185).
    const fn moves_before_base_tick(self) -> bool {
        !matches!(
            self,
            Self::Firework | Self::FishingHook | Self::LlamaSpit | Self::ShulkerBullet
        )
    }

    /// The thrown items, on ThrowableProjectile's tick.
    #[must_use]
    pub(crate) const fn is_thrown(self) -> bool {
        matches!(
            self,
            Self::Generic
                | Self::Snowball
                | Self::Egg
                | Self::EnderPearl
                | Self::Potion
                | Self::ExperienceBottle
        )
    }

    /// Entity.getBoundingBox at `position`: EntityDimensions.makeBoundingBox of `size`, and for a
    /// wind charge 0.15F below its position (1.20.4 WindCharge.java:36-45, 26.3
    /// AbstractWindCharge.java:47-52).
    pub(crate) fn bounding_box(self, position: Vec3) -> Aabb {
        let size = self.size();
        let half_width = f64::from(size / 2.0);
        let bottom = if self == Self::WindCharge {
            position.y - f64::from(0.15_f32)
        } else {
            position.y
        };
        Aabb::new(
            Vec3::new(position.x - half_width, bottom, position.z - half_width),
            Vec3::new(position.x + half_width, bottom + f64::from(size), position.z + half_width),
        )
    }

    /// EntityType.sized: width and height (1.21.5 EntityType.java).
    fn size(self) -> f32 {
        match self {
            Self::Arrow | Self::SpectralArrow | Self::TippedArrow | Self::Trident => 0.5,
            Self::Fireball | Self::DragonFireball => 1.0,
            Self::SmallFireball | Self::WitherSkull | Self::WindCharge | Self::ShulkerBullet => 0.3125,
            _ => PROJECTILE_SIZE,
        }
    }

    #[must_use]
    pub const fn is_hurting(self) -> bool {
        matches!(
            self,
            Self::SmallFireball
                | Self::Fireball
                | Self::WitherSkull
                | Self::DragonFireball
                | Self::WindCharge
        )
    }

    #[must_use]
    pub fn normalize_name(name: &str) -> String {
        let mut normalized = name.trim().to_ascii_lowercase();
        for prefix in ["minecraft:", "entity:", "minecraft.", "entity."] {
            if normalized.starts_with(prefix) {
                normalized = normalized[prefix.len()..].to_owned();
            }
        }
        normalized = normalized
            .replace(' ', "_")
            .replace('-', "_")
            .replace('.', "_")
            .replace('/', "_")
            .replace('\\', "_")
            .replace(':', "_");
        while normalized.contains("__") {
            normalized = normalized.replace("__", "_");
        }
        normalized = normalized.trim_matches('_').to_owned();
        if normalized.starts_with("entity_") {
            normalized = normalized.strip_prefix("entity_").unwrap().to_owned();
        }
        if normalized.ends_with("_entity") {
            normalized = normalized.strip_suffix("_entity").unwrap().to_owned();
        }
        normalized
    }

    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let key = Self::normalize_name(name);
        match key.as_str() {
            "snowball" | "thrown_snowball" | "thrownsnowball" => Self::Snowball,
            "egg" | "thrown_egg" | "thrownegg" => Self::Egg,
            "ender_pearl" | "enderpearl" | "thrown_ender_pearl" | "thrownenderpearl"
            | "ender_pearl_entity" => Self::EnderPearl,
            // splash_potion and lingering_potion from 1.21.5 (1.21.5 EntityType.java:284, 286)
            "potion" | "thrown_potion" | "thrownpotion" | "splash_potion" | "lingering_potion" => {
                Self::Potion
            }
            // 1.8.8 EntityType ThrownExpBottle (EntityType.java:243)
            "experience_bottle" | "experiencebottle" | "xp_bottle" | "xpbottle"
            | "exp_bottle" | "expbottle" | "thrown_experience" | "thrownexperience"
            | "thrownexpbottle" | "experience_bottle_entity" => Self::ExperienceBottle,
            // fireworks_rocket in 1.11-1.12 (1.12.2 EntityType.java:232), 1.8.9
            // FireworksRocketEntity (EntityType.java:248)
            "firework" | "firework_rocket" | "fireworkrocket" | "firework_rocket_entity"
            | "fireworkrocketentity" | "fireworks_rocket" | "fireworksrocketentity" => {
                Self::Firework
            }
            // fishing_bobber from 1.13 (1.21.11 EntityType.java)
            "fishing_hook" | "fishinghook" | "fishing_rod_hook" | "fishingrodhook"
            | "fishing_bobber" | "fishingbobber" => Self::FishingHook,
            "spectral_arrow" | "spectralarrow" | "spectral_arrow_entity" => Self::SpectralArrow,
            "tipped_arrow" | "tippedarrow" | "tipped_arrow_entity" => Self::TippedArrow,
            "arrow" | "thrown_arrow" | "thrownarrow" | "arrow_entity" => Self::Arrow,
            "trident" | "thrown_trident" | "throwntrident" | "trident_entity" => Self::Trident,
            "small_fireball" | "smallfireball" | "fire_charge" | "firecharge"
            | "small_fireball_entity" | "fire_charge_entity" => Self::SmallFireball,
            "fireball" | "large_fireball" | "largefireball" | "big_fireball" | "bigfireball"
            | "large_fireball_entity" | "fireball_entity" => Self::Fireball,
            "wither_skull" | "witherskull" | "wither_skull_entity" => Self::WitherSkull,
            "dragon_fireball" | "dragonfireball" | "dragon_fireball_entity" => Self::DragonFireball,
            // wind_charge from 1.20.3, breeze_wind_charge from 1.20.5 (1.20.5 EntityType.java:187, 291)
            "wind_charge" | "windcharge" | "breeze_wind_charge" | "breezewindcharge" => {
                Self::WindCharge
            }
            "llama_spit" | "llamaspit" | "llama_spit_entity" => Self::LlamaSpit,
            "shulker_bullet" | "shulkerbullet" | "shulker_bullet_entity" => Self::ShulkerBullet,
            _ => Self::Generic,
        }
    }

    #[must_use]
    pub fn from_name_checked(name: &str) -> Option<Self> {
        let kind = Self::from_name(name);
        (kind != Self::Generic).then_some(kind)
    }

    /// The name and first release of a kind that is not of 1.8's (1.8.8 EntityType.java:233-248): the shulker
    /// bullet and dragon fireball are 1.9's (1.9.4 EntityType.java:261-262), the llama's spit 1.11's (1.11.2
    /// EntityType.java:271), and the wind charge 1.20.3's (1.20.3 EntityType "wind_charge").
    #[must_use]
    pub fn first_release(self) -> Option<(&'static str, MinecraftVersion)> {
        match self {
            Self::ShulkerBullet => Some(("shulker bullet", MinecraftVersion::new(1, 9, 0))),
            Self::DragonFireball => Some(("dragon fireball", MinecraftVersion::new(1, 9, 0))),
            Self::LlamaSpit => Some(("llama spit", MinecraftVersion::new(1, 11, 0))),
            Self::WindCharge => Some(("wind charge", MinecraftVersion::new(1, 20, 3))),
            _ => None,
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileTickResult {
    pub position: Vec3,
    pub velocity: Vec3,
    pub block_hit: bool,
    pub horizontal_collision: bool,
    pub on_ground: bool,
    pub bobbing: bool,
    /// The client removed the projectile this tick (llama spit in water or inside blocks).
    pub discarded: bool,
}

impl ProjectileTickResult {
    fn of(state: &ProjectileState, block_hit: bool, discarded: bool) -> Self {
        Self {
            position: state.position,
            velocity: state.velocity,
            block_hit,
            horizontal_collision: state.horizontal_collision,
            on_ground: state.on_ground,
            bobbing: state.bobbing,
            discarded,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub gravity: f64,
    pub air_drag: f64,
    pub water_inertia: f64,
    pub in_water: bool,
    pub in_ground: bool,
    pub life: u16,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub fall_impulse: Vec3,
    /// The state id of the block the arrow hit (lastState, inBlockState).
    pub last_block_id: Option<u32>,
    /// The block the arrow hit (xTile/yTile/zTile), whose state the in-ground test reads until
    /// 1.13.2.
    pub last_block_position: Option<(i32, i32, i32)>,
    /// AbstractArrow.isNoPhysics (a returning trident), from 1.13.
    pub no_physics: bool,
    /// The dimension is ultrawarm: lava pushes at 0.007.
    pub ultra_warm: bool,
}

impl ArrowState {
    /// A flying arrow at rest in no block: AbstractArrow's 0.05 gravity (AbstractArrow#getDefaultGravity),
    /// 0.99F air drag and 0.6F water inertia (AbstractArrow#getWaterInertia).
    #[must_use]
    pub const fn new(position: Vec3, velocity: Vec3) -> Self {
        Self {
            position,
            velocity,
            gravity: 0.05,
            air_drag: 0.99_f32 as f64,
            water_inertia: 0.6_f32 as f64,
            in_water: false,
            in_ground: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            fall_impulse: Vec3::new(0.0, 0.0, 0.0),
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        }
    }

    /// `new` with the gravity of `release`: AbstractArrow's 0.05F before 1.20.5 (1.20.4
    /// AbstractArrow.java:258), 0.05 from 1.20.5 (1.20.5 :266-267).
    #[must_use]
    pub fn new_in_release(position: Vec3, velocity: Vec3, release: MinecraftVersion) -> Self {
        Self {
            gravity: ProjectileKind::Arrow.default_gravity_in(release),
            ..Self::new(position, velocity)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowTickResult {
    pub position: Vec3,
    pub velocity: Vec3,
    pub block_hit: bool,
    pub in_ground: bool,
    pub discarded: bool,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub started_falling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TridentState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub gravity: f64,
    pub air_drag: f64,
    pub water_inertia: f64,
    pub in_water: bool,
    pub dealt_damage: bool,
    /// AbstractArrow's embedded state, which a trident shares (`ArrowState`).
    pub in_ground: bool,
    pub life: u16,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub fall_impulse: Vec3,
    pub last_block_id: Option<u32>,
    pub last_block_position: Option<(i32, i32, i32)>,
    pub no_physics: bool,
    pub ultra_warm: bool,
}

impl TridentState {
    /// A flying trident: AbstractArrow's gravity and air drag with ThrownTrident#getWaterInertia's
    /// 0.99F.
    #[must_use]
    pub const fn new(position: Vec3, velocity: Vec3) -> Self {
        Self {
            position,
            velocity,
            gravity: 0.05,
            air_drag: 0.99_f32 as f64,
            water_inertia: 0.99_f32 as f64,
            in_water: false,
            dealt_damage: false,
            in_ground: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            fall_impulse: Vec3::ZERO,
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        }
    }

    fn arrow(&self) -> ArrowState {
        ArrowState {
            position: self.position,
            velocity: self.velocity,
            gravity: self.gravity,
            air_drag: self.air_drag,
            water_inertia: self.water_inertia,
            in_water: self.in_water,
            in_ground: self.in_ground,
            life: self.life,
            in_ground_time: self.in_ground_time,
            shake_time: self.shake_time,
            fall_impulse: self.fall_impulse,
            last_block_id: self.last_block_id,
            last_block_position: self.last_block_position,
            no_physics: self.no_physics,
            ultra_warm: self.ultra_warm,
        }
    }

    fn take_arrow(&mut self, arrow: ArrowState) {
        self.position = arrow.position;
        self.velocity = arrow.velocity;
        self.in_water = arrow.in_water;
        self.in_ground = arrow.in_ground;
        self.life = arrow.life;
        self.in_ground_time = arrow.in_ground_time;
        self.shake_time = arrow.shake_time;
        self.last_block_id = arrow.last_block_id;
        self.last_block_position = arrow.last_block_position;
    }

    /// `new` with the AbstractArrow gravity of `release` (0.05F before 1.20.5).
    #[must_use]
    pub fn new_in_release(position: Vec3, velocity: Vec3, release: MinecraftVersion) -> Self {
        Self {
            gravity: ProjectileKind::Trident.default_gravity_in(release),
            ..Self::new(position, velocity)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TridentTickResult {
    pub position: Vec3,
    pub velocity: Vec3,
    pub block_hit: bool,
    pub in_ground: bool,
    pub discarded: bool,
    pub in_ground_time: u32,
    pub shake_time: u8,
    pub started_falling: bool,
    pub dealt_damage: bool,
}

/// AbstractArrow.tick for a ThrownTrident as the unversioned release (26.2) runs it.
#[must_use]
pub fn tick_trident(state: &mut TridentState, world: &WorldSnapshot) -> TridentTickResult {
    tick_trident_in_release(state, world, UNVERSIONED_RELEASE)
}

/// ThrownTrident.tick of `release` (1.13+): AbstractArrow.tick, the embedded state included, with
/// the trident's 0.99F water inertia (1.13.2 ThrownTrident.java:57-92, 193; 26.3
/// ThrownTrident.java:62-99), after a stay of more than four ticks in the ground setting
/// dealtDamage. Loyalty's return needs an owner: `tick_trident_with_targets`.
#[must_use]
pub fn tick_trident_in_release(
    state: &mut TridentState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> TridentTickResult {
    if state.in_ground_time > 4 {
        state.dealt_damage = true;
    }
    let mut arrow = state.arrow();
    let mut hits = ProjectileHitState::default();
    let mut scene = HitScene::new(&[], None, &mut hits);
    scene.trident = true;
    let result = arrow_tick(&mut arrow, world, release, &mut scene);
    state.take_arrow(arrow);
    trident_result(result, state)
}

fn trident_result(result: ArrowTickResult, state: &TridentState) -> TridentTickResult {
    TridentTickResult {
        position: state.position,
        velocity: state.velocity,
        block_hit: result.block_hit,
        in_ground: result.in_ground,
        discarded: result.discarded,
        in_ground_time: result.in_ground_time,
        shake_time: result.shake_time,
        started_falling: result.started_falling,
        dealt_damage: state.dealt_damage,
    }
}

/// Entity.baseTick's fluids: whether the box is in water, and, when `pushed`, the currents' push
/// (updateInWaterStateAndDoFluidPushing; handleWaterMovement before 1.13, 1.12.2
/// Entity.java:1040-1055). Lava's follows water's from 1.16, 0.007 in an ultrawarm dimension
/// (1.16.5 Entity.java:1064-1069, 26.3 :1698-1707); before it no current of lava moves them. In
/// 1.16 and 1.16.1 an entity in water returns before lava's (1.16 Entity.java:1036-1045, 1.16.1
/// :1039-1048); from 1.16.2 lava pushes it in water too (1.16.2 Entity.java:1064-1070).
#[allow(clippy::too_many_arguments)]
fn update_in_water(
    position: Vec3,
    velocity: &mut Vec3,
    in_water: &mut bool,
    size: f32,
    pushed: bool,
    ultra_warm: bool,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let mut body = SimulationState {
        position,
        velocity: *velocity,
        half_width: f64::from(size / 2.0),
        height: f64::from(size),
        in_water: *in_water,
        ..SimulationState::default()
    };
    if legacy_fluid::applies(release) {
        legacy_fluid::handle_water_movement(&mut body, world, pushed, release);
    } else {
        let water = water_contact(world, body.bounds(), release);
        body.in_water = water.touching;
        if water.touching && pushed {
            push_by_fluid(&mut body, water, 0.014, release);
        }
        let lava_reads = release.at_least(1, 16, 2) || (release.at_least(1, 16, 0) && !body.in_water);
        if pushed && lava_reads {
            let lava = entity_fluid_contact(&body, world, FluidState::Lava, release);
            if lava.touching {
                push_by_fluid(&mut body, lava, lava_flow_scale(ultra_warm), release);
            }
        }
    }
    *velocity = body.velocity;
    *in_water = body.in_water;
}

/// Whether Entity.tick's base tick comes first in the projectile's tick: `super.tick()` opens it up
/// to 1.21.1 (1.8.8 AbstractArrow.java:149, 1.20.4 ThrowableProjectile.java:44, 1.21.1
/// ThrowableProjectile.java:39-41) and follows the move from 1.21.2 (1.21.3 ThrowableProjectile.java:58,
/// AbstractArrow.java:242; 26.3 ThrowableProjectile.java:56, AbstractArrow.java:251).
const fn base_tick_opens(release: MinecraftVersion) -> bool {
    !release.at_least(1, 21, 2)
}

/// Entity.checkBelowWorld, in the base tick on either side: 64 blocks below the world the entity
/// discards itself, and the tick goes on to its end. The bottom is 0 until 1.16.5, written as
/// `y < -64.0` (1.8.8 Entity.java:415, 1.16.5 :529), and the level's from 1.17 (1.17 Entity.java:461-463,
/// 26.3 :626-629). LivingEntity overrides it to hurt instead (26.3 LivingEntity.java:2118).
fn below_world(y: f64, world: &WorldSnapshot, release: MinecraftVersion) -> bool {
    let bottom = if release.at_least(1, 17, 0) { world.min_build_height() } else { 0 };
    y < f64::from(bottom - 64)
}

/// `tick_projectile_in_release` of a projectile in its world from tick to tick: isInWater and the
/// current's push are the base tick's (Entity#baseTick), at the family's place for it in
/// ThrowableProjectile#tick and AbstractHurtingProjectile#tick.
pub fn tick_projectile_in_world(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let opens = base_tick_opens(release) || !state.kind.moves_before_base_tick();
    let size = state.kind.size();
    let mut void = false;
    if opens {
        let position = state.position;
        update_in_water(position, &mut state.velocity, &mut state.in_water, size, true, state.ultra_warm, world, release);
        void = below_world(position.y, world, release);
    }
    let mut result = tick_projectile_in_release(state, world, release);
    if !opens {
        let position = state.position;
        update_in_water(position, &mut state.velocity, &mut state.in_water, size, true, state.ultra_warm, world, release);
        result.velocity = state.velocity;
        void = below_world(position.y, world, release);
    }
    result.discarded |= void;
    result
}

/// `tick_projectile_in_world` as the server runs it, among `targets` in the order Level.getEntities
/// gives them: the family's query, reaction and removal. `owner` is what Projectile.getOwner gives,
/// and the owner and what it rides belong in `targets` while they are near. Projectile.tick's
/// left-owner test keeps the base tick's place: at the end of the move for the thrown items and the
/// hurting projectiles from 1.21.2 (1.21.3 ThrownEntity tick, 26.3 ThrowableProjectile.java:56),
/// otherwise first (1.21 ThrownEntity tick, 26.3 FireworkRocketEntity.java:108, FishingHook.java:146,
/// LlamaSpit.java:38, ShulkerBullet.java:185). The hit of those moved first comes last, after the
/// base tick (26.3 ThrowableProjectile.java:57-59, AbstractHurtingProjectile.java:89-91).
pub fn tick_projectile_with_targets(
    state: &mut ProjectileState,
    hits: &mut ProjectileHitState,
    targets: &[ProjectileTarget],
    owner: Option<ProjectileOwner>,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (ProjectileTickResult, ProjectileHitReport) {
    let mut scene = HitScene::new(targets, owner, hits);
    scene.kind = state.kind;
    scene.live = true;
    scene.hits.tick_count = scene.hits.tick_count.wrapping_add(1);
    let gone_first = pearl_lost(state, &scene, release) || wind_charge_too_high(state, world, release);
    if gone_first {
        return (ProjectileTickResult::of(state, false, true), scene.report);
    }
    let opens = base_tick_opens(release) || !state.kind.moves_before_base_tick();
    let size = state.kind.size();
    if opens {
        scene.check_left_owner(state.position, state.velocity, release);
        let position = state.position;
        update_in_water(position, &mut state.velocity, &mut state.in_water, size, true, state.ultra_warm, world, release);
    }
    let mut result = tick_family(state, world, release, &mut scene);
    if !opens {
        scene.check_left_owner(state.position, state.velocity, release);
        let position = state.position;
        update_in_water(position, &mut state.velocity, &mut state.in_water, size, true, state.ultra_warm, world, release);
    }
    if let Some(hit) = scene.pending.take() {
        scene.hit_flight(hit, state, release);
    }
    result.velocity = state.velocity;
    result.discarded |= scene.removed;
    (result, scene.report)
}

/// ThrownEnderpearl.tick: a pearl does not outlive a player who threw it (1.8.8
/// ThrownEnderpearl.java tick, 26.3 ThrownEnderpearl.java:177), unless the game rule says so, which
/// comes in 1.20.2 (1.20.2 ThrownEnderpearl.java:90), or from 1.21.9 the player left through the
/// End's exit: ServerPlayer.showEndCredits removes it, which reads as not alive, and sets wonGame
/// (1.21.8 ThrownEnderpearl.java has no wonGame, 1.21.9 has; 26.3 ServerPlayer.java:1135-1143).
fn pearl_lost(state: &ProjectileState, scene: &HitScene<'_>, release: MinecraftVersion) -> bool {
    state.kind == ProjectileKind::EnderPearl
        && !(release.at_least(1, 20, 2) && scene.hits.pearl_outlives_owner)
        && scene.owner.is_some_and(|owner| {
            // (1.21.8 ThrownEnderpearl.java:184 asks isAlive alone; 1.21.9 ThrownEnderpearl.java:172 also wonGame)
            owner.player && !owner.alive && !(release.at_least(1, 21, 9) && owner.won_game)
        })
}

/// AbstractWindCharge.tick, from 1.20.5: too far above the level, it bursts. The top is getTopY,
/// the first height past the level, until 1.21 and the last height in it from 1.21.2 (1.20.5
/// AbstractWindCharge.java:126-134, 1.21 :141-148, 1.21.3 :140-147, 26.3 :161-168).
fn wind_charge_too_high(
    state: &ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> bool {
    // (a world told no height has no such limit to go by)
    if state.kind != ProjectileKind::WindCharge || !release.at_least(1, 20, 5) || world.top_y() == 0 {
        return false;
    }
    // (1.21.1 AbstractWindCharge.java:142 getMaxBuildHeight; 1.21.2 AbstractWindCharge.java:141 getMaxY, one less)
    let top = world.top_y() - i32::from(release.at_least(1, 21, 2));
    state.position.y.floor() as i32 > top + 30
}

/// `tick_arrow_in_release` of an arrow in its world from tick to tick. Until 1.21.1 the base tick
/// opens every tick, in the ground too (1.21 AbstractArrow.java:154). From 1.21.2 it
/// closes the flying branch only, the tick of a hit included, and the in-ground branch has none
/// (26.3 AbstractArrow.java:192-212, 251; 1.21.3 AbstractArrow.java:192-206, 242).
/// From 1.21.5 an embedded arrow is not pushed (1.21.5 AbstractArrow.java:327-329, 26.3 :344-346);
/// before, Entity's isPushedByFluid holds (1.21.4 Entity.java:2907).
pub fn tick_arrow_in_world(
    state: &mut ArrowState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ArrowTickResult {
    let mut hits = ProjectileHitState::default();
    arrow_tick_in_world(state, world, release, &mut HitScene::new(&[], None, &mut hits))
}

/// `tick_arrow_in_world` among `targets`, in the order Level.getEntities gives them: the tick also
/// meets them, and reports what it hit. `owner` is what Projectile.getOwner gives, and the owner
/// and what it rides belong in `targets` for as long as they are near.
pub fn tick_arrow_with_targets(
    state: &mut ArrowState,
    hits: &mut ProjectileHitState,
    targets: &[ProjectileTarget],
    owner: Option<ProjectileOwner>,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (ArrowTickResult, ProjectileHitReport) {
    let mut scene = HitScene::new(targets, owner, hits);
    let result = arrow_tick_in_world(state, world, release, &mut scene);
    (result, scene.report)
}

/// ServerLevel.tickNonPassenger counts the entity's tick before Entity.tick runs it.
fn arrow_tick_in_world(
    state: &mut ArrowState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ArrowTickResult {
    scene.hits.tick_count = scene.hits.tick_count.wrapping_add(1);
    // Projectile.tick opens AbstractArrow.tick, before Entity.tick's base tick, until 1.21.1
    // (1.16.5 Projectile.java:88-94, 1.21 AbstractArrow.java:154; 1.21.1 AbstractArrow.java:154,
    // Projectile.java:117; 1.21.2 AbstractArrow.java:242 calls it at the end of the flying branch)
    if !release.at_least(1, 21, 2) {
        scene.check_left_owner(state.position, state.velocity, release);
    }
    let size = ProjectileKind::Arrow.size();
    let mut void = false;
    if base_tick_opens(release) {
        let position = state.position;
        update_in_water(position, &mut state.velocity, &mut state.in_water, size, true, state.ultra_warm, world, release);
        void = below_world(position.y, world, release);
    }
    let mut result = arrow_tick(state, world, release, scene);
    // only the in-ground branch counts inGroundTime, and the flying one resets it
    if !base_tick_opens(release) && result.in_ground_time == 0 {
        let position = state.position;
        // (1.21.4 Entity.java:2907 (isPushedByFluid true, no arrow override); 1.21.5 AbstractArrow.java:327-329 (!isInGround()))
        let pushed = !(state.in_ground && release.at_least(1, 21, 5));
        update_in_water(position, &mut state.velocity, &mut state.in_water, size, pushed, state.ultra_warm, world, release);
        result.velocity = state.velocity;
        void = below_world(position.y, world, release);
    }
    result.discarded |= void;
    result
}

/// `tick_trident_in_release` of a trident in its world from tick to tick, with ThrownTrident#tick's
/// dealtDamage after more than four ticks in the ground.
pub fn tick_trident_in_world(
    state: &mut TridentState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> TridentTickResult {
    if state.in_ground_time > 4 {
        state.dealt_damage = true;
    }
    tick_trident_with_targets(state, &mut ProjectileHitState::default(), &[], None, world, release).0
}

/// `tick_trident_in_world` among `targets`, as `tick_arrow_with_targets` (ThrownTrident#tick). A trident with
/// `hits.loyalty` returns to `owner` once it has dealt its damage or is returning: with no owner it
/// stays where it is, and a dead or spectating owner loses it.
pub fn tick_trident_with_targets(
    state: &mut TridentState,
    hits: &mut ProjectileHitState,
    targets: &[ProjectileTarget],
    owner: Option<ProjectileOwner>,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (TridentTickResult, ProjectileHitReport) {
    if state.in_ground_time > 4 {
        state.dealt_damage = true;
    }
    let mut scene = HitScene::new(targets, owner, hits);
    scene.trident = true;
    scene.dealt_damage = state.dealt_damage;
    let (mut position, mut velocity, mut no_physics) =
        (state.position, state.velocity, state.no_physics);
    let gone = return_to_owner(
        &mut position,
        &mut velocity,
        &mut no_physics,
        state.dealt_damage,
        &scene,
        release,
    );
    if gone {
        scene.hits.tick_count = scene.hits.tick_count.wrapping_add(1);
        let result = ArrowTickResult {
            position,
            velocity,
            block_hit: false,
            in_ground: state.in_ground,
            discarded: true,
            in_ground_time: state.in_ground_time,
            shake_time: state.shake_time,
            started_falling: false,
        };
        return (trident_result(result, state), scene.report);
    }
    (state.position, state.velocity, state.no_physics) = (position, velocity, no_physics);
    let mut arrow = state.arrow();
    let result = arrow_tick_in_world(&mut arrow, world, release, &mut scene);
    state.take_arrow(arrow);
    state.dealt_damage = scene.dealt_damage;
    (trident_result(result, state), scene.report)
}

/// AbstractArrow.tick as the unversioned release (26.2) runs it.
#[must_use]
pub fn tick_arrow(state: &mut ArrowState, world: &WorldSnapshot) -> ArrowTickResult {
    tick_arrow_in_release(state, world, UNVERSIONED_RELEASE)
}

/// AbstractArrow.tick of `release` (1.21.11 AbstractArrow.java:165-254, 1.21
/// AbstractArrow.java:153-281, 1.15 AbstractArrow.java, 1.13.2
/// AbstractArrow.java, 1.9.4 AbstractArrow.java:165-195, 1.8.8 AbstractArrow.java:148-380). shakeTime counts down at the top of every tick. An arrow in the
/// ground counts inGroundTime up, and falls (its motion scaled by `fall_impulse`, the
/// random.nextFloat() * 0.2F of each axis) when the block state it reads is not the one it hit
/// and, from 1.11, no block collides with the test box: until 1.13.2 the hit block's own state and
/// the arrow's box inflated by 0.05, in 1.14-1.15 the state at the arrow's block and its box
/// inflated by 0.06, from 1.16 the arrow's point inflated by 0.06. Otherwise it ages towards the
/// 1200-tick despawn. From 1.21.2 the fall and the despawn run on the server only; this models the
/// server. A flying arrow resets inGroundTime and flies by `arrow_flight`; a block hit sets the
/// seven-tick shake and remembers the hit block.
#[must_use]
pub fn tick_arrow_in_release(
    state: &mut ArrowState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ArrowTickResult {
    let mut hits = ProjectileHitState::default();
    arrow_tick(state, world, release, &mut HitScene::new(&[], None, &mut hits))
}

fn arrow_tick(
    state: &mut ArrowState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ArrowTickResult {
    let arrow_block = (
        floor_i32_for(release, state.position.x),
        floor_i32_for(release, state.position.y),
        floor_i32_for(release, state.position.z),
    );
    // the shape around the arrow embeds it before anything else: the block at the arrow from 1.14
    // (1.14.4 AbstractArrow.java:160-173), before it the block of the last hit, -1, -1, -1 at first
    // (1.8.8 AbstractArrow.java:157-165, 1.13.2 :165-176); from 1.21.5 the motion stops too
    // (1.21.5 AbstractArrow.java:172-175, 1.21.4 AbstractArrow.java:175-176, 26.3
    // AbstractArrow.java:176). AABB.contains takes the lower faces in from 1.13 (1.12.2 AABB.java:270-274,
    // 1.13.2 :213-218).
    let contained_in = if release.at_least(1, 14, 0) {
        arrow_block
    } else {
        state.last_block_position.unwrap_or((-1, -1, -1))
    };
    // isNoPhysics, from 1.13 (1.13.2 AbstractArrow.java:156, 26.3 AbstractArrow.java:165)
    let no_physics = state.no_physics && release.at_least(1, 13, 0);
    let contained = !no_physics && world.block_collision_boxes(contained_in).iter().any(|shape| {
        let at = state.position;
        if release.at_least(1, 13, 0) {
            shape.contains(at)
        } else {
            at.x > shape.min.x
                && at.x < shape.max.x
                && at.y > shape.min.y
                && at.y < shape.max.y
                && at.z > shape.min.z
                && at.z < shape.max.z
        }
    });
    if contained {
        state.in_ground = true;
        // (1.21.4 AbstractArrow.java:174-178; 1.21.5 AbstractArrow.java:175-179 (setDeltaMovement(Vec3.ZERO) before setInGround))
        if release.at_least(1, 21, 5) {
            state.velocity = Vec3::ZERO;
        }
    }
    state.shake_time = state.shake_time.saturating_sub(1);
    if state.in_ground && !no_physics {
        // (1.13.2 AbstractArrow.java:165-166, 188; 1.14.4 AbstractArrow.java:160-161, 186)
        let read_block = if release.at_least(1, 14, 0) {
            arrow_block
        } else {
            state.last_block_position.unwrap_or(arrow_block)
        };
        let changed = state.last_block_id != Some(world.block_cow(read_block).id);
        // (1.10.2 AbstractArrow.java:182-188 falls on any change; 1.11.2 AbstractArrow.java:179 only with no block in
        // its box inflated by 0.05; 1.15.2 AbstractArrow.java:189 by 0.06; 1.16 AbstractArrow.java:244-245 a point's)
        let falls = changed
            && if !release.at_least(1, 11, 0) {
                true
            // (1.13.2 AbstractArrow.java:188; 1.14.4 AbstractArrow.java:186)
            } else if !release.at_least(1, 14, 0) {
                no_block_collision(
                    world,
                    arrow_box(state.position).inflate(Vec3::new(0.05, 0.05, 0.05)),
                    release,
                )
            } else if !release.at_least(1, 16, 0) {
                no_block_collision(
                    world,
                    arrow_box(state.position).inflate(Vec3::new(0.06, 0.06, 0.06)),
                    release,
                )
            } else {
                no_block_collision(
                    world,
                    Aabb::new(state.position, state.position).inflate(Vec3::new(0.06, 0.06, 0.06)),
                    release,
                )
            };
        // a loyal trident that may be picked up never ages (1.13.2 ThrownTrident.java:185-190,
        // 26.3 ThrownTrident.java:221-226)
        let ages = !(scene.trident && scene.hits.pickup_allowed && scene.hits.loyalty > 0);
        if falls {
            state.in_ground = false;
            state.velocity = state.velocity.multiply(state.fall_impulse);
            state.life = 0;
            scene.hits.flying_ticks = 0;
        } else if ages {
            state.life = state.life.saturating_add(1);
        }
        state.in_ground_time = state.in_ground_time.saturating_add(1);
        // from 1.21.2 the blocks the arrow is in act on it in the ground too (1.21.2
        // AbstractArrow.java:201-203; 1.21.1 AbstractArrow.java:192-199 has none; 1.21.3
        // PersistentProjectileEntity.java:200-202 tickBlockCollision, 1.21.5
        // AbstractArrow.java:198-200, 26.3 AbstractArrow.java:200-203; none in 1.21
        // PersistentProjectileEntity.java:187-196), but the bubble column's pushes skip an arrow
        // in the ground: none of them in 1.21.2 - 1.21.3, the inside one in 1.21.4, both from
        // 1.21.5 (1.21.4 AbstractArrow.java:352-356, 1.21.5 :362-372, 26.3 :379-389)
        let discarded = !falls && ages && state.life >= 1200;
        let pushes = if release.at_least(1, 21, 5) {
            None
        } else if release.at_least(1, 21, 4) {
            Some(BubblePushes::SurfaceOnly)
        } else {
            Some(BubblePushes::Both)
        };
        // (fall() has cleared inGround before the effects, so a falling arrow takes both)
        let pushes = if falls { Some(BubblePushes::Both) } else { pushes };
        if let Some(pushes) = pushes.filter(|_| release.at_least(1, 21, 2) && !discarded) {
            let at = state.position;
            push_by_bubble_columns_along_where(
                &mut state.velocity,
                at,
                at,
                ARROW_SIZE,
                world,
                release,
                pushes,
            );
        }
        return ArrowTickResult {
            position: state.position,
            velocity: state.velocity,
            block_hit: false,
            in_ground: state.in_ground,
            discarded,
            in_ground_time: state.in_ground_time,
            shake_time: state.shake_time,
            started_falling: falls,
        };
    }

    state.in_ground_time = 0;
    scene.hits.flying_ticks = scene.hits.flying_ticks.wrapping_add(1);
    // inside the flying branch from 1.21.9, before the ray (1.21.9 AbstractArrow.java:233)
    if release.at_least(1, 21, 9) {
        scene.check_left_owner(state.position, state.velocity, release);
    }
    if no_physics {
        // no ray and no gravity; the move is always the motion the tick began with (1.16.5
        // AbstractArrow.java:210-239, 26.3 AbstractArrow.java:212-239)
        let inertia = if state.in_water {
            state.water_inertia
        } else {
            state.air_drag
        };
        state.position = state.position + state.velocity;
        state.velocity = state.velocity.scale(inertia);
        // checkBlockCollision closes the flight of a noClip arrow too until 1.21.1; from 1.21.2
        // shouldTickBlockCollision / isAffectedByBlocks leave it out (1.21.1
        // AbstractArrow.java:277-278, 1.21.2 AbstractArrow.java:233-234, 1.21.3
        // PersistentProjectileEntity.java:232-233, Entity.java:785-787)
        if !release.at_least(1, 21, 2) {
            let at = state.position;
            push_by_bubble_columns_inside(&mut state.velocity, at, ARROW_SIZE, world, release);
        }
        end_of_flight(state, scene, release);
        return ArrowTickResult {
            position: state.position,
            velocity: state.velocity,
            block_hit: false,
            in_ground: state.in_ground,
            discarded: false,
            in_ground_time: state.in_ground_time,
            shake_time: state.shake_time,
            started_falling: false,
        };
    }
    let hit = arrow_flight(
        &mut state.position,
        &mut state.velocity,
        ArrowMotion {
            gravity: state.gravity,
            air_drag: state.air_drag,
            water_inertia: state.water_inertia,
            in_water: state.in_water,
        },
        world,
        release,
        scene,
    );
    if let Some(hit) = hit {
        scene.report.block_hit = Some(hit.report());
    }
    if let Some(hit) = hit.filter(|hit| !hit.bounced) {
        state.in_ground = true;
        state.shake_time = 7;
        state.last_block_id = Some(world.block_cow(hit.block).id);
        state.last_block_position = Some(hit.block);
        // onHitBlock clears the pierce (1.14.4 AbstractArrow.java, 26.3 AbstractArrow.java:530-532)
        scene.hits.pierce_level = 0;
        scene.hits.pierced.clear();
    }
    end_of_flight(state, scene, release);
    ArrowTickResult {
        position: state.position,
        velocity: state.velocity,
        block_hit: hit.is_some_and(|hit| !hit.bounced),
        in_ground: state.in_ground,
        discarded: scene.removed,
        in_ground_time: state.in_ground_time,
        shake_time: state.shake_time,
        started_falling: false,
    }
}

/// Projectile.tick closes the flying branch from 1.21.2; until 1.21.8 that is the left-owner test,
/// at the moved arrow (1.21.3 AbstractArrow.java:242, Projectile.java:133-136;
/// 1.21.2 AbstractArrow.java:242, Projectile.java:134-136; 1.21.1 Projectile.java:117 before the arrow's tick). From
/// 1.21.9 the flying branch checks first and Projectile.tick's check finds it done (1.21.9 AbstractArrow.java:237,
/// Projectile.java:104, 110-112).
fn end_of_flight(state: &ArrowState, scene: &mut HitScene<'_>, release: MinecraftVersion) {
    if release.at_least(1, 21, 2) && !release.at_least(1, 21, 9) {
        scene.check_left_owner(state.position, state.velocity, release);
    }
}

/// The inertia and gravity of an AbstractArrow's flight.
#[derive(Clone, Copy, Debug)]
struct ArrowMotion {
    gravity: f64,
    air_drag: f64,
    water_inertia: f64,
    in_water: bool,
}

impl ArrowMotion {
    const WATER_INERTIA: f64 = 0.6_f32 as f64;
    const TRIDENT_WATER_INERTIA: f64 = 0.99_f32 as f64;
}

/// The flight of an AbstractArrow tick, returning its block hit.
///
/// From 1.21.2 (1.21.11 AbstractArrow.java:211-251, onHitBlock :531-553; 1.21.3
/// AbstractArrow.java tick, onBlockHit :498-503): in water the water inertia first
/// (from 1.21.4; 1.21.2-1.21.3 apply it after the move, which comes to the same), then the clip and
/// the move by the motion from the start of the tick; a hit moves the arrow to the hit, back by
/// 0.05F against the sign of each axis of its motion, and stops it; otherwise the air inertia out
/// of water and the gravity.
///
/// Before (1.21 AbstractArrow.java:201-279, onBlockHit :454-475): a hit sets the
/// motion to the hit's offset and moves the arrow back by 0.05F along it (from 1.14 by
/// Vec3.normalize; until 1.13.2 the offset is cast to float, 1.13.2 divides by Mth.sqrt *
/// 20.0F, 1.8.9-1.12.2 divide by Mth.sqrt and multiply by 0.05F: 1.13.2
/// AbstractArrow.java:324-330, 1.12.2 :370-376, 1.8.8 AbstractArrow.java:295-298); then the
/// arrow moves by that motion, which takes the water or the air inertia and the gravity, also on
/// the hit tick.
fn arrow_flight(
    position: &mut Vec3,
    velocity: &mut Vec3,
    motion: ArrowMotion,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> Option<BlockHit> {
    let start = *position;
    // (1.21.1 AbstractArrow.java:204-277 clips, then moves by the motion; 1.21.2 AbstractArrow.java:229-230
    // clipIncludingBorder and stepMoveAndHit)
    if release.at_least(1, 21, 2) {
        return arrow_step_flight(position, velocity, motion, world, release, scene);
    }

    let mut hit = clip_blocks(world, start, *velocity, release, ClipShape::Collider);
    let to = hit.map_or(start + *velocity, |hit| hit.location);
    // The entity on the ray replaces the block, and is hit where the arrow is: an arrow does not
    // move to it. With pierce it looks again, and the block is forgotten. A target the owner may
    // not harm drops the hit, block included (1.8.8 AbstractArrow.java:222-227, 1.14.4
    // AbstractArrow.java:213-242, 1.20.5 AbstractArrow.java:191-208).
    while let Some(found) = scene.find_hits(start, to, *velocity, world, release).first().copied() {
        hit = None;
        let target = scene.targets[found.0];
        // (1.8.8 AbstractArrow.java:222-224 also spares an invulnerable player; 1.9.4 AbstractArrow.java:209-211 not)
        if target.friendly || (target.invulnerable_player && !release.at_least(1, 9, 0)) {
            break;
        }
        let deflected = scene.hit(found, velocity, release);
        if scene.removed || deflected || scene.pierce_level(release) == 0 {
            break;
        }
    }
    if scene.removed {
        return None;
    }
    if let Some(hit) = hit {
        // (1.13.2 AbstractArrow.java:324-330; 1.14.4 AbstractArrow.java:308-313)
        if release.at_least(1, 14, 0) {
            let offset = hit.location - start;
            *velocity = offset;
            let back = offset.normalized(release).scale(f64::from(0.05_f32));
            *position = start - back;
        } else {
            let offset = Vec3::new(
                java_float(hit.location.x - start.x),
                java_float(hit.location.y - start.y),
                java_float(hit.location.z - start.z),
            );
            *velocity = offset;
            let length =
                (offset.x * offset.x + offset.y * offset.y + offset.z * offset.z).sqrt() as f32;
            // (1.12.2 AbstractArrow.java:370-376 (x - v / h * 0.05F); 1.13.2 AbstractArrow.java:324-330 (x - v / (h * 20.0F)))
            *position = if release.at_least(1, 13, 0) {
                let divisor = f64::from(length * 20.0_f32);
                Vec3::new(
                    start.x - offset.x / divisor,
                    start.y - offset.y / divisor,
                    start.z - offset.z / divisor,
                )
            } else {
                let length = f64::from(length);
                let back = f64::from(0.05_f32);
                Vec3::new(
                    start.x - offset.x / length * back,
                    start.y - offset.y / length * back,
                    start.z - offset.z / length * back,
                )
            };
            // and the block hit takes the arrow in, its onEntityCollision on it, before the move:
            // soul sand slows that motion to 0.4 (1.8.9 AbstractArrowEntity.java:306-308,
            // SoulSandBlock.java:25-26; 1.12.2 AbstractArrowEntity.java:381-383; 1.13.2 :335-337,
            // SoulSandBlock.java:26-27; from 1.14 the hit calls onProjectileHit instead, 1.14.4
            // AbstractArrow.java:322)
            if world.effect_at(hit.block) == BlockEffect::SoulSand {
                velocity.x *= 0.4;
                velocity.z *= 0.4;
            }
        }
    }
    *position = *position + *velocity;
    let inertia = if motion.in_water {
        motion.water_inertia
    } else {
        motion.air_drag
    };
    *velocity = velocity.scale(inertia);
    apply_arrow_gravity(velocity, motion.gravity, release);
    // checkBlockCollision at the moved arrow: soul sand's slowing until 1.14.4, the bubble
    // columns' pushes from 1.13 (1.8.9 AbstractArrowEntity.java:381, 1.13.2
    // AbstractArrowEntity.java:302-303, 1.21 PersistentProjectileEntity.java:275-276)
    push_by_bubble_columns_inside(velocity, *position, ARROW_SIZE, world, release);
    hit
}

/// EntityType.ARROW's and TRIDENT's 0.5F.
const ARROW_SIZE: f32 = 0.5;

/// The gravity of an arrow, a thrown item, a spit or a shulker bullet: y - 0.05F until 1.20.4 (1.20.4 AbstractArrow.java:258); from
/// 1.20.5 Entity.applyGravity adds (0, -gravity, 0) when there is gravity, which makes a -0 of the
/// other axes +0 (1.20.5 Entity.java:1147-1152, 26.3 :1567-1573).
fn apply_arrow_gravity(velocity: &mut Vec3, gravity: f64, release: MinecraftVersion) {
    if !release.at_least(1, 20, 5) {
        velocity.y -= gravity;
    } else if gravity != 0.0 {
        *velocity = *velocity + Vec3::new(0.0, -gravity, 0.0);
    }
}

/// The gravity of a spit or a shulker bullet: a Vec3 add from 1.14, which makes a -0 of the other
/// axes +0 (`getDeltaMovement().add(0.0, -0.06F, 0.0)`, 1.14.4 LlamaSpit.java:94, 1.20.4 :51-52;
/// `add(0.0, -0.04, 0.0)`, 1.14.4 ShulkerBullet.java:257, 1.20.4 :208), where 1.13.2 takes it off the y
/// alone (1.13.2 LlamaSpit.java:128, ShulkerBullet.java:254); NoGravity adds nothing.
fn apply_vector_gravity_from_1_14(velocity: &mut Vec3, gravity: f64, release: MinecraftVersion) {
    if release.at_least(1, 14, 0) {
        if gravity != 0.0 {
            *velocity = *velocity + Vec3::new(0.0, -gravity, 0.0);
        }
    } else {
        apply_arrow_gravity(velocity, gravity, release);
    }
}

/// AbstractArrow.stepMoveAndHit, from 1.21.2: the arrow moves to each hit in turn and ends on the
/// block's hit or at the end of the ray (client1.21.2 AbstractArrow.java:185-215, 1.21.5 :257-288,
/// 1.21.11 :258-295, 26.3 :258-292). Before 1.21.4 the water's inertia follows the move, which a
/// deflection's and the search box's motion can see.
fn arrow_step_flight(
    position: &mut Vec3,
    velocity: &mut Vec3,
    motion: ArrowMotion,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> Option<BlockHit> {
    let start = *position;
    let movement = *velocity;
    // (1.21.3 AbstractArrow.java:231-237,280-288 (water or 0.99 inertia after stepMoveAndHit); 1.21.4 AbstractArrow.java:209,231-238 (water inertia before the clip, 0.99 after out of water))
    let water_first = release.at_least(1, 21, 4);
    if motion.in_water && water_first {
        *velocity = velocity.scale(motion.water_inertia);
    }
    let block = clip_blocks(world, start, movement, release, ClipShape::Collider);
    let end = block.map_or(start + movement, |hit| hit.location);
    let mut result = None;
    loop {
        let here = *position;
        let found = scene.find_hits(here, end, *velocity, world, release);
        *position = found.first().map_or(end, |hit| hit.1);
        // applyEffectsFromBlocks(initialPosition, nextLocation) of the step, before its hits
        // (1.21.3 PersistentProjectileEntity.java:248-254 tickBlockCollision; 1.21.5
        // AbstractArrow.java:258; 26.3 AbstractArrow.java:265)
        push_by_bubble_columns_along(velocity, here, *position, ARROW_SIZE, world, release);
        if found.is_empty() {
            let Some(hit) = block else { break };
            // a border hit reverses the arrow, which flies on (client1.21.2 Projectile.java:232-237,
            // AbstractArrow.java:765-767; 1.21.4 Projectile.java:225-231,
            // AbstractArrow.java:773-777; 26.3 Projectile.java:262-265,
            // AbstractArrow.java:776-778)
            if hit.border {
                *velocity = border_bounce(*velocity, release);
                result = Some(BlockHit {
                    bounced: true,
                    ..hit
                });
                break;
            }
            let offset_direction = Vec3::new(
                java_signum(velocity.x),
                java_signum(velocity.y),
                java_signum(velocity.z),
            );
            *position = hit.location - offset_direction.scale(f64::from(0.05_f32));
            *velocity = Vec3::ZERO;
            return Some(hit);
        }
        // hitTargetsOrDeflectSelf: every hit of the step, until one removes or deflects the arrow
        let mut deflected = false;
        for hit in found {
            deflected = scene.hit(hit, velocity, release);
            if scene.removed || deflected {
                break;
            }
        }
        if scene.removed {
            return None;
        }
        if scene.pierce_level(release) == 0 || deflected {
            break;
        }
    }
    if !motion.in_water {
        *velocity = velocity.scale(motion.air_drag);
    } else if !water_first {
        *velocity = velocity.scale(motion.water_inertia);
    }
    apply_arrow_gravity(velocity, motion.gravity, release);
    result
}

/// ProjectileDeflection.REVERSE by the border's 0.2: -0.5, then 0.2 (1.21.5 ProjectileDeflection.java:
/// 11-13, Projectile.java:235-238), and -power * 0.5 at once in 26.3 (26.3
/// ProjectileDeflection.java:12-14, Projectile.java:262-265).
pub(crate) fn border_bounce(velocity: Vec3, release: MinecraftVersion) -> Vec3 {
    if release.at_least(26, 3, 0) {
        velocity.scale(-0.2 * 0.5)
    } else {
        velocity.scale(-0.5).scale(0.2)
    }
}

/// EntityType.ARROW's 0.5F x 0.5F box (EntityDimensions#makeBoundingBox), which the in-ground test inflates until 1.15 (1.15
/// ProjectileEntity tick, 1.13.2 AbstractArrowEntity tick).
pub(crate) fn arrow_box(position: Vec3) -> Aabb {
    let half_width = f64::from(0.5_f32 / 2.0);
    Aabb::new(
        Vec3::new(position.x - half_width, position.y, position.z - half_width),
        Vec3::new(
            position.x + half_width,
            position.y + f64::from(0.5_f32),
            position.z + half_width,
        ),
    )
}

/// Advances one projectile tick as the unversioned release (26.2) runs it.
#[must_use]
pub fn tick_projectile(state: &mut ProjectileState, world: &WorldSnapshot) -> ProjectileTickResult {
    tick_projectile_in_release(state, world, UNVERSIONED_RELEASE)
}

/// Advances one projectile tick of `release` by its family's own tick: ThrowableProjectile's for
/// the thrown items (and an unknown family), AbstractHurtingProjectile's for the fireballs and
/// the wither skull, and FireworkRocketEntity's, FishingHook's (from 1.11), LlamaSpit's and
/// ShulkerBullet's as the client runs them (ThrowableProjectile#tick, AbstractHurtingProjectile#tick,
/// FireworkRocketEntity#tick, FishingHook#tick, LlamaSpit#tick, ShulkerBullet#tick).
#[must_use]
pub fn tick_projectile_in_release(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let mut hits = ProjectileHitState::default();
    tick_family(state, world, release, &mut HitScene::new(&[], None, &mut hits))
}

/// The family's tick. A live scene makes it the server's (`tick_projectile_with_targets`). After it
/// the tick is no longer the first (Entity#firstTick, cleared at the end of Entity#baseTick).
fn tick_family(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    let result = match state.kind {
        kind if kind.is_hurting() => tick_hurting_projectile(state, world, release, scene),
        ProjectileKind::Firework => tick_firework(state, world, release, scene),
        ProjectileKind::FishingHook => tick_fishing_hook(state, world, release, scene),
        ProjectileKind::LlamaSpit => tick_llama_spit(state, world, release, scene),
        ProjectileKind::ShulkerBullet => tick_shulker_bullet(state, world, release, scene),
        kind if kind.is_arrow() => tick_arrow_flight(state, world, release),
        _ => tick_throwable(state, world, release, scene),
    };
    state.first_tick = false;
    result
}

/// An arrow or trident without its embedded state, which `tick_arrow_in_release` keeps: the flight
/// alone, by AbstractArrow's water inertia (0.6F, a trident's 0.99F): AbstractArrow#tick's flight,
/// AbstractArrow#getWaterInertia, ThrownTrident#getWaterInertia.
fn tick_arrow_flight(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let water_inertia = if state.kind == ProjectileKind::Trident {
        ArrowMotion::TRIDENT_WATER_INERTIA
    } else {
        ArrowMotion::WATER_INERTIA
    };
    let hit = arrow_flight(
        &mut state.position,
        &mut state.velocity,
        ArrowMotion {
            gravity: state.gravity,
            air_drag: state.air_drag,
            water_inertia,
            in_water: state.in_water,
        },
        world,
        release,
        &mut HitScene::new(&[], None, &mut ProjectileHitState::default()),
    );
    ProjectileTickResult::of(state, hit.is_some_and(|hit| !hit.bounced), false)
}

/// ThrowableProjectile.tick. From 1.21.2 (1.21.2 ThrowableProjectile.java:43-62): the first
/// tick's bubble columns, gravity, inertia, then the hit test and the move by the new motion,
/// stopping at the hit, and the bubble columns along that move (applyEffectsFromBlocks). Before
/// (1.21.1 :39-66, 1.20.4 ThrowableProjectile.java:64-93, 1.8.8 ThrowableProjectile.java:147-190): the hit
/// test, from 1.16.2 the bubble columns of the box (checkBlockCollision/checkInsideBlocks, 1.16.5
/// ThrowableProjectile.java:69, 1.21.1 ThrowableProjectile.java:45; not in 1.16.1's), and the move by
/// the motion they leave, the full move even on a hit, then inertia and gravity.
fn tick_throwable(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    let inertia = if state.in_water {
        f64::from(0.8_f32)
    } else {
        state.air_drag
    };
    let shape = flight_clip_shape(state.kind, release);
    // (1.21.1 ThrowableProjectile.java:39-67 (base tick, hit, move, inertia, gravity); 1.21.2 ThrowableProjectile.java:43-63 (first-tick bubble column, gravity, inertia, clip, move to the hit, effects, base tick, hit))
    if release.at_least(1, 21, 2) {
        if state.first_tick {
            handle_first_tick_bubble_column(state, world, release);
        }
        apply_arrow_gravity(&mut state.velocity, state.gravity, release);
        state.velocity = state.velocity.scale(inertia);
        let start = state.position;
        let block = clip_blocks(world, start, state.velocity, release, shape);
        let hit = scene.flight_hit(start, state.velocity, block, false, release);
        state.position = hit.map_or(start + state.velocity, FlightHit::location);
        let body = projectile_body(state.position);
        let movements = [Movement {
            from: start,
            to: state.position,
            axis_order: None,
        }];
        let inside = blocks_visited(
            &movements,
            body.bounds(),
            body.half_width,
            body.height,
            release,
        );
        let clamped = state.kind == ProjectileKind::EnderPearl;
        let at = state.position;
        apply_projectile_cells(&mut state.velocity, at, PROJECTILE_SIZE, false, &inside, world, release, BubblePushes::Both, clamped);
        scene.pending = hit;
        return ProjectileTickResult::of(state, hit.is_some_and(FlightHit::is_block), false);
    }
    scene.hits.flying_ticks = scene.hits.flying_ticks.wrapping_add(1);
    let block = clip_blocks(world, state.position, state.velocity, release, shape);
    let hit = scene.flight_hit(state.position, state.velocity, block, false, release);
    if let Some(hit) = hit {
        scene.hit_flight(hit, state, release);
    }
    // (1.16.1 ThrowableProjectile.java:46-94 (no checkInsideBlocks); 1.16.2 ThrowableProjectile.java:69 (checkInsideBlocks after the hit))
    if release.at_least(1, 16, 2) {
        let cells: Vec<_> = blocks_inside(&projectile_body(state.position), release)
            .into_iter()
            .map(|cell| (cell, true))
            .collect();
        let clamped = state.kind == ProjectileKind::EnderPearl;
        let at = state.position;
        apply_projectile_cells(&mut state.velocity, at, PROJECTILE_SIZE, false, &cells, world, release, BubblePushes::Both, clamped);
    }
    let movement = state.velocity;
    state.position = state.position + movement;
    state.velocity = movement.scale(inertia);
    apply_arrow_gravity(&mut state.velocity, state.gravity, release);
    ProjectileTickResult::of(state, hit.is_some_and(FlightHit::is_block), false)
}

/// ThrowableProjectile.handleFirstTickBubbleColumn (1.21.2+, 1.21.2 ThrowableProjectile.java:83-95):
/// each bubble column cell of the box pushes the projectile (BlockPos.betweenClosed, z outer and
/// x fastest; from 26.3 Level.findBlocksIn, chunk sections x then z then y, and in a section y
/// outer and x fastest, 26.3 ThrowableProjectile.java:85-87, BlockScanUtils.java:27-101).
fn handle_first_tick_bubble_column(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let bounds = projectile_box(state.position);
    let min = (
        floor_i32_for(release, bounds.min.x),
        floor_i32_for(release, bounds.min.y),
        floor_i32_for(release, bounds.min.z),
    );
    let max = (
        floor_i32_for(release, bounds.max.x),
        floor_i32_for(release, bounds.max.y),
        floor_i32_for(release, bounds.max.z),
    );
    let mut cells = Vec::new();
    // (26.2 ThrowableProjectile.java:87-97 (BlockPos.betweenClosed); 26.3 ThrowableProjectile.java:85-90, BlockScanUtils.java:27-101 (sections x, z, y; in a section y, z, x))
    if release.at_least(26, 3, 0) {
        let section = |value: i32| value >> 4;
        let low = |section: i32, min: i32| (section << 4).max(min);
        let high = |section: i32, max: i32| ((section << 4) + 15).min(max);
        for section_x in section(min.0)..=section(max.0) {
            for section_z in section(min.2)..=section(max.2) {
                for section_y in section(min.1)..=section(max.1) {
                    for y in low(section_y, min.1)..=high(section_y, max.1) {
                        for z in low(section_z, min.2)..=high(section_z, max.2) {
                            for x in low(section_x, min.0)..=high(section_x, max.0) {
                                cells.push((x, y, z));
                            }
                        }
                    }
                }
            }
        }
    } else {
        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    cells.push((x, y, z));
                }
            }
        }
    }
    for cell in cells {
        push_by_bubble_column(state, world, cell, release);
    }
}

/// BubbleColumnBlock.entityInside for a thrown item when `cell` is a bubble column: with nothing
/// above (air until 1.21.4; no collision shape and no fluid from 1.21.5) the surface push,
/// otherwise the inside push. Until 1.21.4 these are Entity's clamped
/// onBubbleColumnSurfaceCollision/onBubbleColumnCollision (1.21.4 Entity.java:2552-2575); from
/// 1.21.5 Projectile's unclamped onAboveBubbleColumn and onInsideBubbleColumn (1.21.5
/// Projectile.java:172-182), except the ender pearl's, which call Entity's clamped
/// handleOnAboveBubbleColumn/handleOnInsideBubbleColumn (1.21.5 ThrownEnderpearl.java:246-252,
/// Entity.java:2536-2547, 2563-2574; 26.3 ThrownEnderpearl.java:244-250).
fn push_by_bubble_column(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    cell: (i32, i32, i32),
    release: MinecraftVersion,
) {
    let clamped = state.kind == ProjectileKind::EnderPearl;
    push_motion_by_bubble_column(&mut state.velocity, clamped, world, cell, release);
}

/// The box of a projectile of `size` at `position`, for the blocks it is inside.
fn sized_body(position: Vec3, size: f32) -> SimulationState {
    SimulationState {
        position,
        half_width: f64::from(size / 2.0),
        height: f64::from(size),
        ..SimulationState::default()
    }
}

/// Entity.checkInsideBlocks (checkBlockCollision) of the box at `position` before 1.21.2: each
/// bubble column cell in it pushes the motion, and before 1.15 each soul sand cell slows its
/// horizontal motion to 0.4 (1.8.9 SoulSandBlock.java:25-26, 1.14.4 SoulSandBlock.java:26-27; 1.15
/// has the speed factor instead and no entityInside), and from 1.15 honey slides it down its side.
fn push_by_bubble_columns_inside(
    velocity: &mut Vec3,
    position: Vec3,
    size: f32,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    let cells: Vec<_> = blocks_inside(&sized_body(position, size), release)
        .into_iter()
        .map(|cell| (cell, true))
        .collect();
    apply_projectile_cells(velocity, position, size, false, &cells, world, release, BubblePushes::Both, false);
}

/// The entityInside effects a projectile's motion takes from `cells` (each with isPrecise): a
/// bubble column's push of a precise cell, before 1.15 soul sand's 0.4 of x and z, from 1.15 the
/// slide down a honey block's side (1.15 HoneyBlock.java:68-75, 26.3 :55-62, 73-97;
/// simulation::slide_down_honey). `on_ground` is the projectile's, which a slide needs off.
#[allow(clippy::too_many_arguments)]
fn apply_projectile_cells(
    velocity: &mut Vec3,
    position: Vec3,
    size: f32,
    on_ground: bool,
    cells: &[((i32, i32, i32), bool)],
    world: &WorldSnapshot,
    release: MinecraftVersion,
    pushes: BubblePushes,
    clamped: bool,
) {
    for &(cell, precise) in cells {
        match world.effect_at(cell) {
            BlockEffect::SoulSand if !release.at_least(1, 15, 0) => {
                velocity.x *= 0.4;
                velocity.z *= 0.4;
            }
            BlockEffect::Honey if release.at_least(1, 15, 0) => {
                let mut body = SimulationState {
                    velocity: *velocity,
                    on_ground,
                    ..sized_body(position, size)
                };
                if crate::simulation::slide_down_honey(&mut body, cell, release) {
                    *velocity = body.velocity;
                }
            }
            BlockEffect::BubbleUp | BlockEffect::BubbleDown if precise => {
                push_motion_by_bubble_column_where(velocity, clamped, world, cell, release, pushes);
            }
            _ => {}
        }
    }
}

/// Entity.applyEffectsFromBlocks(from, to) from 1.21.2 (tickBlockCollision until 1.21.4): each
/// bubble column cell the move from `from` to `to` precisely went through pushes the motion.
fn push_by_bubble_columns_along(
    velocity: &mut Vec3,
    from: Vec3,
    to: Vec3,
    size: f32,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) {
    push_by_bubble_columns_along_where(velocity, from, to, size, world, release, BubblePushes::Both);
}

fn push_by_bubble_columns_along_where(
    velocity: &mut Vec3,
    from: Vec3,
    to: Vec3,
    size: f32,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    pushes: BubblePushes,
) {
    let body = sized_body(to, size);
    let movements = [Movement {
        from,
        to,
        axis_order: None,
    }];
    let cells = blocks_visited(
        &movements,
        body.bounds(),
        body.half_width,
        body.height,
        release,
    );
    apply_projectile_cells(velocity, to, size, false, &cells, world, release, pushes, false);
}

/// push_by_bubble_column on a motion; `clamped` is the ender pearl's Entity push from 1.21.5.
fn push_motion_by_bubble_column(
    velocity: &mut Vec3,
    clamped: bool,
    world: &WorldSnapshot,
    cell: (i32, i32, i32),
    release: MinecraftVersion,
) {
    push_motion_by_bubble_column_where(velocity, clamped, world, cell, release, BubblePushes::Both);
}

/// Which of a bubble column's pushes reach the entity: an arrow in the ground takes both in
/// 1.21.2 - 1.21.3, the surface's alone in 1.21.4 and neither from 1.21.5.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BubblePushes {
    Both,
    SurfaceOnly,
}

fn push_motion_by_bubble_column_where(
    velocity: &mut Vec3,
    clamped: bool,
    world: &WorldSnapshot,
    (x, y, z): (i32, i32, i32),
    release: MinecraftVersion,
    pushes: BubblePushes,
) {
    let drag_down = match world.effect_at((x, y, z)) {
        BlockEffect::BubbleDown => true,
        BlockEffect::BubbleUp => false,
        _ => return,
    };
    let above = (x, y + 1, z);
    // (1.21.4 BubbleColumnBlock.java:51; 1.21.5 BubbleColumnBlock.java:52)
    let nothing_above = if release.at_least(1, 21, 5) {
        !world.has_collision_shape(above) && world.fluid_at(above) == FluidState::Empty
    } else {
        world.is_air_at(above)
    };
    if pushes == BubblePushes::SurfaceOnly && !nothing_above {
        return;
    }
    let unclamped = release.at_least(1, 21, 5) && !clamped;
    // Projectile's pushes are Vec3.add(0, d, 0), which makes a -0 of x and z +0 (1.21.5
    // Projectile.java:172-182, Vec3.java:104-106; 26.3 Projectile.java:179-189); Entity's clamped
    // ones set x and z back as they were (1.21.5 Entity.java:2536-2547, 2563-2574)
    if unclamped {
        velocity.x += 0.0;
        velocity.z += 0.0;
    }
    let velocity_y = velocity.y;
    velocity.y = match (unclamped, nothing_above, drag_down) {
        (true, _, true) => velocity_y + -0.03,
        (true, true, false) => velocity_y + 0.1,
        (true, false, false) => velocity_y + 0.06,
        (false, true, true) => (velocity_y - 0.03).max(-0.9),
        (false, true, false) => (velocity_y + 0.1).min(1.8),
        (false, false, true) => (velocity_y - 0.03).max(-0.3),
        (false, false, false) => (velocity_y + 0.06).min(0.7),
    };
}

/// AbstractHurtingProjectile.tick. From 1.21.2 the inertia comes first and the hit test and the
/// move take the new motion, stopping at the hit (1.21.2 AbstractHurtingProjectile.java:69-113);
/// before, the hit test and the full move take the old motion and the inertia follows (1.21.1
/// :66-108, 1.8.9 EntityFireball). The inertia adds the power, from 1.21 accelerationPower along
/// the motion's direction (1.21 AbstractHurtingProjectile.java:99), before it the xPower vector
/// (1.20.6 :100), and scales by 0.8F in water or the air inertia (0.95F, WitherSkull's dangerous
/// 0.73F). A wind charge's liquid inertia is its 1.0F air inertia, and in 1.20.3-1.20.4 it clips
/// the outline shapes (1.21.11 AbstractWindCharge.java:127-133, 1.20.4 WindCharge.java:120-138).
fn tick_hurting_projectile(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    // gone with its owner or out of the loaded level (1.8.9 ExplosiveProjectileEntity tick, 26.3
    // AbstractHurtingProjectile.java:69), after the inertia from 1.21.2
    let lost = scene.live
        && (scene.hits.chunk_unloaded || scene.owner.is_some_and(|owner| owner.removed));
    let wind_charge = state.kind == ProjectileKind::WindCharge;
    let inertia = if state.in_water && !wind_charge {
        f64::from(0.8_f32)
    } else {
        state.air_drag
    };
    let clip_shape = flight_clip_shape(state.kind, release);
    let accelerated = |state: &ProjectileState| {
        let push = if release.at_least(1, 21, 0) {
            state.velocity.normalized(release).scale(state.acceleration_power)
        } else {
            state.power
        };
        (state.velocity + push).scale(inertia)
    };
    // (1.21.1 AbstractHurtingProjectile.java:66-107 (base tick, hit, move, then acceleration); 1.21.2 AbstractHurtingProjectile.java:67-95 (applyInertia first, clip, move to the hit, effects, base tick, hit))
    if release.at_least(1, 21, 2) {
        state.velocity = accelerated(state);
        if lost {
            return ProjectileTickResult::of(state, false, true);
        }
        let (block, stuck) = flight_block(state, world, release, clip_shape, scene);
        let hit = scene.flight_hit(state.position, state.velocity, block, stuck, release);
        let start = state.position;
        state.position = hit.map_or(state.position + state.velocity, FlightHit::location);
        // applyEffectsFromBlocks after setPos, the bubble columns' pushes in it (1.21.3
        // ExplosiveProjectileEntity.java:83 tickBlockCollision, 1.21.5
        // AbstractHurtingProjectile.java:79, 26.3 :80)
        let size = state.kind.size();
        push_by_bubble_columns_along(&mut state.velocity, start, state.position, size, world, release);
        scene.pending = hit;
        return ProjectileTickResult::of(state, hit.is_some_and(FlightHit::is_block), false);
    }
    if lost {
        return ProjectileTickResult::of(state, false, true);
    }
    scene.hits.flying_ticks = scene.hits.flying_ticks.wrapping_add(1);
    let (block, stuck) = flight_block(state, world, release, clip_shape, scene);
    let hit = scene.flight_hit(state.position, state.velocity, block, stuck, release);
    if let Some(hit) = hit {
        scene.hit_flight(hit, state, release);
    }
    // checkBlockCollision after the hit, before the move takes the motion, from 1.16.2 (1.16.1
    // AbstractHurtingProjectile.java has none; 1.16.2 :74, 1.16.5 ExplosiveProjectileEntity.java:76,
    // 1.21 :81)
    if release.at_least(1, 16, 2) && !scene.removed {
        let at = state.position;
        push_by_bubble_columns_inside(&mut state.velocity, at, state.kind.size(), world, release);
    }
    state.position = state.position + state.velocity;
    state.velocity = accelerated(state);
    ProjectileTickResult::of(state, hit.is_some_and(FlightHit::is_block), false)
}

/// FireworkRocketEntity.tick for a rocket attached to a holder (from 1.11): it stands at it, at
/// the hand of a gliding player from 1.18.2 (Entity.getHandHoldingItemAngle, 1.18.1 has none, 1.19
/// Entity.java:1862-1871, 26.3 :2691-2699), and takes its motion (26.3 FireworkRocketEntity.java:119-137;
/// 1.18.1 FireworkRocketEntity.java:117 at the holder; 1.18.2 FireworkRocketEntity.java:122-127 at its hand).
fn stay_with_holder(state: &mut ProjectileState, holder: RocketHolder, release: MinecraftVersion) {
    let hand = if holder.gliding && holder.player && release.at_least(1, 18, 2) {
        let side = if holder.left_hand { -80.0 } else { 80.0 };
        view_vector(0.0, holder.yaw_degrees + side, release).scale(0.5)
    } else {
        Vec3::ZERO
    };
    state.position = holder.position + hand;
    state.velocity = holder.velocity;
}

/// FireworkRocketEntity.tick for a rocket attached to no one: the boost (from 1.14 not for a
/// rocket shot at an angle, from 1.16.2 none along a horizontal collision), then Entity.move with
/// its collisions; from 1.16 the motion is put back as it was before the move (1.21.11
/// FireworkRocketEntity.java:139-149, 1.16 :29-33, 1.14 FireworkEntity, 1.8.9). The hit test
/// runs before the move from 1.21.2 and after it from 1.14, and moves nothing; a noPhysics
/// rocket acts on no hit (1.14.4 FireworkRocketEntity.java:136, 1.20.4 :140, 1.21.3 :151, 26.3 :152).
/// A rocket has no hit test until 1.14 (1.13.2 FireworkRocketEntity tick). On a block in 1.14 and
/// 1.15 it acts only when the move collided, and at its own cell if the ray missed (1.14.4
/// FireworkRocketEntity.java:182-199); from 1.16 on any block hit. It bursts at the end of its life
/// (26.3 FireworkRocketEntity.java:174-180, 1.8.9 tick).
fn tick_firework(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    // (an attached rocket only follows, and never flies again: 26.3 FireworkRocketEntity.java:108-138)
    // (1.10.2 FireworkRocketEntity.java:101-102 has no holder; 1.11.2 FireworkRocketEntity.java:118-131 follows it)
    let attached = scene.hits.attached && release.at_least(1, 11, 0);
    if attached {
        if let Some(holder) = scene.hits.holder {
            stay_with_holder(state, holder, release);
        }
    } else if !(state.shot_at_angle && release.at_least(1, 14, 0)) {
        // (1.16.3 FireworkRocketEntity.java:155 still boosts by 1.15 along a collision, as do 1.16.1 and 1.16.2 at the
        // same line; the test of horizontalCollision is first in 1.16.4 FireworkRocketEntity.java:155-156)
        let acceleration = if state.horizontal_collision && release.at_least(1, 16, 4) {
            1.0
        } else {
            1.15
        };
        state.velocity = state
            .velocity
            .multiply(Vec3::new(acceleration, 1.0, acceleration))
            + Vec3::new(0.0, 0.04, 0.0);
    }
    let movement = state.velocity;
    let query = |state: &ProjectileState, scene: &mut HitScene<'_>| {
        let block = clip_blocks(world, state.position, state.velocity, release, ClipShape::Collider);
        scene.flight_hit(state.position, state.velocity, block, false, release)
    };
    let mut hit = None;
    // (1.21.1 FireworkRocketEntity.java:135,139 (query after the move); 1.21.2 FireworkRocketEntity.java:131,139-140 (query before the move; attached: after following the holder))
    if release.at_least(1, 21, 2) {
        hit = query(state, scene);
    }
    let mut collided = false;
    if !attached {
        move_projectile(state, world, release, scene.live);
        collided = state.horizontal_collision || state.velocity.y != movement.y;
        // (1.15.2 FireworkRocketEntity.java:159; 1.16 FireworkRocketEntity.java:154-155)
        if release.at_least(1, 16, 0) {
            state.velocity = movement;
        }
    }
    // (1.13.2 FireworkRocketEntity.java:102-182; 1.14.4 FireworkRocketEntity.java:132-139)
    if release.at_least(1, 14, 0) && !release.at_least(1, 21, 2) {
        hit = query(state, scene);
    }
    let block_hit = hit.is_some_and(FlightHit::is_block) && !state.no_physics;
    if let Some(hit) = hit.filter(|_| !state.no_physics) {
        if release.at_least(1, 16, 0) || !hit.is_block() || collided {
            scene.hit_flight(hit, state, release);
        }
    } else if scene.live
        && !state.no_physics
        && collided
        && scene.hits.explodes
        // (1.13.2 FireworkRocketEntity.java:102-182; 1.14.4 FireworkRocketEntity.java:176-180, 182-197)
        && release.at_least(1, 14, 0)
        && !release.at_least(1, 16, 0)
    {
        scene.removed = true;
    }
    if scene.live {
        scene.hits.life = scene.hits.life.wrapping_add(1);
        scene.removed |= scene.hits.life_time.is_some_and(|life_time| scene.hits.life > life_time);
    }
    ProjectileTickResult::of(state, block_hit, false)
}

/// FishingHook.tick on the client for a hook in no entity (FLYING or BOBBING), from 1.11. A
/// flying hook that reaches water (FluidState.getHeight above 0 where it is) slows to
/// (0.3, 0.2, 0.3) and starts bobbing without moving. Otherwise: from 1.16 the hit test, whose
/// block hit sets the motion along its direction by the squared distance to the hit
/// (onHitBlock), and from 1.21.2 a border hit bounces it; before 1.16 a grounded or collided flying hook stops; a bobbing hook is pulled
/// towards the surface by `bob_random` (in 1.13 the water's own height, FluidState.getHeight
/// with no world until 1.14); -0.03 out of water (from 1.21.6 only off the ground, 1.21.6
/// FishingHook.java:228, 1.21.5 :218); Entity.move; from 1.16 a grounded or collided flying
/// hook stops; then 0.92 (1.21.11 FishingHook.java:149-247, 1.16.5 FishingHook.java,
/// 1.12.2 FishingHook.java).
/// The bite's pull uses the server-synced random and is left out.
///
/// As the server runs it (a live scene): a hook without a player owner, or one the owner stopped, is
/// gone; on the ground it counts to 1200 (1.11.2 FishingHook.java tick, 1.16 onGround, 26.3
/// FishingHook.java:147-162). From 1.16 the query is both sides', and only the server hooks; before,
/// the query is the server's, and a block sets inGround, which stops the hook as the ground does.
/// A hook with a target stops for a tick, then rides it at 0.8 of its height until it is removed,
/// from 1.21.9 until it cannot interact with the level either (1.8.9 and 1.11.2 minY + height * 0.8,
/// 26.3 getY(0.8); 1.21.9 FishingHook.java:188), when it flies again. From 1.21.6 a hooked hook
/// takes no gravity on the tick of its hit (1.21.6 FishingHook.java:228).
fn tick_fishing_hook(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    // (1.10.2 FishingHook.java:169-228 has no states; 1.11.2 FishingHook.java:139-215 FLYING, HOOKED_IN_ENTITY, BOBBING)
    if scene.live && !release.at_least(1, 11, 0) {
        return tick_fishing_hook_before_1_11(state, world, release, scene);
    }
    if scene.live {
        if scene.hits.stop_fishing || !scene.owner.is_some_and(|owner| owner.player) {
            return ProjectileTickResult::of(state, false, true);
        }
        // (1.15.2 FishingHook.java:143; 1.16 FishingHook.java:156)
        let grounded = if release.at_least(1, 16, 0) {
            state.on_ground
        } else {
            scene.hits.in_ground
        };
        if grounded {
            scene.hits.life = scene.hits.life.wrapping_add(1);
            if scene.hits.life >= 1200 {
                return ProjectileTickResult::of(state, false, true);
            }
        // (1.15.2 FishingHook.java:143; 1.16 FishingHook.java:163)
        } else if release.at_least(1, 16, 0) {
            scene.hits.life = 0;
        }
    }
    let block = (
        floor_i32_for(release, state.position.x),
        floor_i32_for(release, state.position.y),
        floor_i32_for(release, state.position.z),
    );
    let in_water = is_water(world.fluid_at(block));
    let height = if in_water {
        water_height(world, block, release)
    } else {
        0.0
    };
    // (1.15.2 FishingHook.java:172-176; 1.16 FishingHook.java:187, 239)
    let modern = release.at_least(1, 16, 0);
    let mut hit = None;
    if !state.bobbing {
        if scene.live && !scene.hits.hook_settled && scene.hits.hooked.is_some() {
            state.velocity = Vec3::ZERO;
            scene.hits.hook_settled = true;
            return ProjectileTickResult::of(state, false, false);
        }
        if scene.live && scene.hits.hook_settled {
            let hooked = scene.hits.hooked;
            let rode = scene.targets.iter().find(|target| {
                Some(target.id) == hooked
                    // (1.21.8 FishingHook.java:188; 1.21.9 FishingHook.java:188)
                    && (!release.at_least(1, 21, 9) || (target.alive && !target.spectator))
            });
            if let Some(target) = rode {
                let height = f64::from((target.bounding_box.max.y - target.bounding_box.min.y) as f32);
                state.position = Vec3::new(
                    target.position.x,
                    target.bounding_box.min.y + height * 0.8,
                    target.position.z,
                );
                // Entity.setPos remakes the box: 1.11.2 FishingHook.java:199, 1.13.2 :207 and
                // 1.14.4 :176 after writing x, y and z (1.13.2 Entity.java:419-426), 1.15
                // FishingHook.java:188, 1.16.5 :194 (Entity.java:469-472); from 1.17 no box is kept,
                // it is made from the position (1.17 Entity.java:367-374)
                if !release.at_least(1, 17, 0) {
                    state.bounding_box = Some(state.kind.bounding_box(state.position));
                }
            } else if hooked.is_some() {
                scene.hits.hooked = None;
                scene.hits.hook_settled = false;
            }
            return ProjectileTickResult::of(state, false, false);
        }
        if height > 0.0 {
            state.velocity = state.velocity.multiply(Vec3::new(0.3, 0.2, 0.3));
            state.bobbing = true;
            return ProjectileTickResult::of(state, false, false);
        }
        if modern {
            hit = clip_blocks(world, state.position, state.velocity, release, ClipShape::Collider);
            let found = scene.flight_hit(state.position, state.velocity, hit, false, release);
            if let Some(FlightHit::Entity(_)) = found {
                hit = None;
            }
            if let Some(found) = found {
                scene.hit_flight(found, state, release);
            }
            match hit {
                // a border hit bounces instead (client1.21.2 FishingHook.java:112-114, 26.3 :119-121),
                // which moves a client's hook until 26.2 and not in 26.3 (26.2 Projectile.java:229-237,
                // 26.3 :278-284)
                Some(hit) if hit.border => {
                    if !release.at_least(26, 3, 0) {
                        state.velocity = border_bounce(state.velocity, release);
                    }
                }
                Some(hit) => {
                    state.velocity = state.velocity.normalized(release)
                        .scale(hit.location.distance_squared(state.position));
                }
                None => {}
            }
        } else {
            if scene.live {
                let block = clip_blocks(world, state.position, state.velocity, release, ClipShape::Collider);
                match scene.flight_hit(state.position, state.velocity, block, false, release) {
                    Some(FlightHit::Block(block)) => {
                        scene.hits.in_ground = true;
                        scene.hit_flight(FlightHit::Block(block), state, release);
                    }
                    Some(found) => scene.hit_flight(found, state, release),
                    None => {}
                }
            }
            if scene.hits.in_ground || state.on_ground || state.horizontal_collision {
                scene.hits.flying_ticks = 0;
                state.velocity = Vec3::ZERO;
            } else {
                scene.hits.flying_ticks = scene.hits.flying_ticks.wrapping_add(1);
            }
        }
    } else {
        let mut distance =
            state.position.y + state.velocity.y - f64::from(block.1) - f64::from(height);
        if distance.abs() < 0.01 {
            distance += java_signum(distance) * 0.1;
        }
        state.velocity = Vec3::new(
            state.velocity.x * 0.9,
            state.velocity.y - distance * f64::from(state.bob_random) * 0.2,
            state.velocity.z * 0.9,
        );
    }
    // (1.21.5 FishingHook.java:217-219 (-0.03 unless in water); 1.21.6 FishingHook.java:228-230 (also not on ground and not hooked))
    let settled = release.at_least(1, 21, 6) && (state.on_ground || scene.hits.hooked.is_some());
    if !in_water && !settled {
        // (a Vec3's add from 1.14, 1.14 FishHookEntity tick, which makes +0 of a -0)
        // (1.13.2 FishingHook.java:229-231; 1.14.4 FishingHook.java:197-199)
        state.velocity = if release.at_least(1, 14, 0) {
            state.velocity + Vec3::new(0.0, -state.gravity, 0.0)
        } else {
            Vec3::new(state.velocity.x, state.velocity.y - state.gravity, state.velocity.z)
        };
    }
    move_projectile(state, world, release, scene.live);
    if modern && !state.bobbing && (state.on_ground || state.horizontal_collision) {
        state.velocity = Vec3::ZERO;
    }
    state.velocity = state.velocity.scale(state.air_drag);
    remake_hook_box(state, release);
    ProjectileTickResult::of(state, hit.is_some_and(|hit| !hit.border), false)
}

/// The end of FishingHook.tick: setPos at its own position remakes the box, which the move left in the middle of
/// (1.8.8 FishingHook.java:415, 1.10.2 :409, 1.11.2 :231, 1.13.2 :239, 1.14.4 :205; 1.15.2 :217 and 1.16.5 :244
/// reapplyPosition). From 1.17 no box is kept: it is made from the position (1.17 Entity.java:367-374).
fn remake_hook_box(state: &mut ProjectileState, release: MinecraftVersion) {
    if !release.at_least(1, 17, 0) {
        state.bounding_box = Some(state.kind.bounding_box(state.position));
    }
}

/// EntityFishHook.onUpdate on the server before 1.11 (1.8.8 FishingHook.java tick, 1.9.4 and
/// 1.10.2 tick), another tick than the state machine of 1.11: a hook rides what it hit at once, in
/// 1.8 only if Entity.hurt accepted it. One that hit a block stands for a tick and comes free the
/// next, as its in-block test compares the block at (-1, -1, -1) with a block it never set: each
/// axis keeps random.nextFloat() * 0.2F (`ground_release`, the server's random), and the count of
/// its flight starts again. In the air it falls by 0.04F and keeps 0.92F of its motion, 0.5F against
/// a block; the five slices of its box in water lift it and slow it. Its bite's pulls are the
/// server's random, which no one sees and this leaves out.
fn tick_fishing_hook_before_1_11(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    if scene.hits.stop_fishing || !scene.owner.is_some_and(|owner| owner.player) {
        return ProjectileTickResult::of(state, false, true);
    }
    if let Some(id) = scene.hits.hooked {
        if let Some(target) = scene.targets.iter().find(|target| target.id == id) {
            let height = f64::from((target.bounding_box.max.y - target.bounding_box.min.y) as f32);
            state.position = Vec3::new(
                target.position.x,
                target.bounding_box.min.y + height * 0.8,
                target.position.z,
            );
            return ProjectileTickResult::of(state, false, false);
        }
        scene.hits.hooked = None;
    }
    if scene.hits.in_ground {
        scene.hits.in_ground = false;
        state.velocity = state.velocity.multiply(scene.hits.ground_release.unwrap_or(Vec3::ZERO));
        scene.hits.flying_ticks = 0;
    } else {
        scene.hits.flying_ticks = scene.hits.flying_ticks.wrapping_add(1);
    }
    let block = clip_blocks(world, state.position, state.velocity, release, ClipShape::Outline);
    let hit = scene.flight_hit(state.position, state.velocity, block, false, release);
    let mut block_hit = false;
    if let Some(hit) = hit {
        block_hit = hit.is_block();
        scene.hits.in_ground = block_hit;
        scene.hit_flight(hit, state, release);
    }
    if scene.hits.in_ground {
        return ProjectileTickResult::of(state, block_hit, false);
    }
    move_projectile(state, world, release, true);
    // (1.8.8 FishingHook.java:302-304)
    let mut inertia = if state.on_ground || state.horizontal_collision {
        0.5_f32
    } else {
        0.92_f32
    };
    let bounds = projectile_box(state.position);
    let height = bounds.max.y - bounds.min.y;
    let mut wet = 0.0;
    for slice in 0..5 {
        let low = bounds.min.y + height * f64::from(slice) / 5.0;
        let high = bounds.min.y + height * f64::from(slice + 1) / 5.0;
        let mut water = false;
        for x in bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32 {
            for y in low.floor() as i32..high.ceil() as i32 {
                for z in bounds.min.z.floor() as i32..bounds.max.z.ceil() as i32 {
                    water |= is_water(world.fluid_at((x, y, z)));
                }
            }
        }
        if water {
            wet += 1.0 / 5.0;
        }
    }
    state.velocity.y += f64::from(0.04_f32) * (wet * 2.0 - 1.0);
    if wet > 0.0 {
        inertia = (f64::from(inertia) * 0.9) as f32;
        state.velocity.y *= 0.8;
    }
    state.velocity = state.velocity.scale(f64::from(inertia));
    remake_hook_box(state, release);
    ProjectileTickResult::of(state, block_hit, false)
}

/// LlamaSpit.tick (1.11+): the hit test and the move take the old motion; a spit in water (from
/// 1.13 until 1.21.4 also one whose block is a bubble column, isInsideWaterOrBubbleColumn), or
/// whose box has no air in it, is removed; otherwise 0.99F and gravity (1.21.11
/// LlamaSpit.java:36-54, 1.21.4 LlamaSpit.java:48, 1.13.2 :121, 1.11.2 LlamaSpitEntity).
/// Before 1.15 the position is advanced before the removal (1.14.4 :62, 1.15 :78-80 only reads
/// it), and until 1.16 the box's cells run floor..ceil (Level.containsMaterial); from 1.16
/// floor..=floor (BlockPos.betweenClosed).
///
/// As the server runs it, a hit comes between the query and the removals, and the spit goes on by
/// the motion from before it, so that a deflection only counts against the deflector (1.21
/// LlamaSpitEntity tick, 26.3 LlamaSpit.java:40-54).
fn tick_llama_spit(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    let movement = state.velocity;
    let block = clip_blocks(world, state.position, movement, release, flight_clip_shape(state.kind, release));
    let hit = scene.flight_hit(state.position, movement, block, false, release);
    if let Some(hit) = hit {
        scene.hit_flight(hit, state, release);
    }
    let block_hit = hit.is_some_and(FlightHit::is_block);
    let target = state.position + movement;
    let bounds = projectile_box(state.position);
    // (1.15.2 LlamaSpit.java:105 (Level.java:599); 1.16 LlamaSpit.java:62 (BlockPos.java:308))
    let modern = release.at_least(1, 16, 0);
    // (1.14.4 LlamaSpit.java:62; 1.15 LlamaSpit.java:78)
    if !release.at_least(1, 15, 0) {
        state.position = target;
    }
    let cells = |min: f64, max: f64| {
        let low = floor_i32_for(release, min);
        if modern {
            low..floor_i32_for(release, max).wrapping_add(1)
        } else {
            low..ceil_i32_for(release, max)
        }
    };
    let mut has_air = false;
    for x in cells(bounds.min.x, bounds.max.x) {
        for y in cells(bounds.min.y, bounds.max.y) {
            for z in cells(bounds.min.z, bounds.max.z) {
                has_air |= world.is_air_at((x, y, z));
            }
        }
    }
    // (1.12.2 LlamaSpit.java:120 (isInWater); 1.13.2 LlamaSpit.java:121 (isInWaterOrBubble))
    let in_bubble_column = release.at_least(1, 13, 0)
        // (1.21.4 LlamaSpit.java:48 (isInWaterOrBubble); 1.21.5 LlamaSpit.java:48 (isInWater))
        && !release.at_least(1, 21, 5)
        && matches!(
            world.effect_at((
                floor_i32_for(release, state.position.x),
                floor_i32_for(release, state.position.y),
                floor_i32_for(release, state.position.z),
            )),
            BlockEffect::BubbleUp | BlockEffect::BubbleDown
        );
    if !has_air || state.in_water || in_bubble_column {
        return ProjectileTickResult::of(state, block_hit, true);
    }
    state.velocity = movement.scale(state.air_drag);
    apply_vector_gravity_from_1_14(&mut state.velocity, state.gravity, release);
    state.position = target;
    ProjectileTickResult::of(state, block_hit, false)
}

/// ShulkerBullet.tick on the client: the steering, gravity and hit test run on the server only;
/// the bullet moves by its motion (1.21.11 ShulkerBullet.java:184-207, 1.9.4 ShulkerBulletEntity).
///
/// As the server runs it (a live scene): towards `bullet_delta`, each axis clamped to 1 after
/// growing by 1.025, a fifth of the way each tick; without one, by gravity. The query comes next,
/// and the move: after a hit until 1.21.1, before it from 1.21.2 (1.9.4 ShulkerBulletEntity tick,
/// 1.21 tick, 1.21.3 tick, 26.3 ShulkerBullet.java:187-219).
fn tick_shulker_bullet(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    scene: &mut HitScene<'_>,
) -> ProjectileTickResult {
    // checkBlockCollision before the move on both sides from 1.16.2, which asks nothing of the
    // bullet's noPhysics (1.16.1 ShulkerBullet.java has none; 1.16.2 :227, 1.16.5
    // ShulkerBulletEntity.java:227, 1.21 :226); from 1.21.2 applyEffectsFromBlocks after it, which
    // the noPhysics bullet skips in 1.21.2 - 1.21.4 (1.21.4 ShulkerBullet.java:49, 221,
    // Entity.java:823-825) and takes from 1.21.5, where isAffectedByBlocks asks only isRemoved
    // (1.21.5 ShulkerBullet.java:213, 249-251; 26.3 :208, 244-246)
    let size = state.kind.size();
    let effects_before_move = release.at_least(1, 16, 2) && !release.at_least(1, 21, 2);
    let effects_after_move = release.at_least(1, 21, 5);
    if !scene.live {
        if effects_before_move {
            let at = state.position;
            push_by_bubble_columns_inside(&mut state.velocity, at, size, world, release);
        }
        let start = state.position;
        state.position = state.position + state.velocity;
        if effects_after_move {
            push_by_bubble_columns_along(&mut state.velocity, start, state.position, size, world, release);
        }
        return ProjectileTickResult::of(state, false, false);
    }
    if scene.hits.peaceful {
        return ProjectileTickResult::of(state, false, true);
    }
    match scene.hits.bullet_delta {
        Some(delta) => {
            let grown = |value: f64| (value * 1.025).clamp(-1.0, 1.0);
            let delta = Vec3::new(grown(delta.x), grown(delta.y), grown(delta.z));
            let motion = state.velocity;
            state.velocity = Vec3::new(
                motion.x + (delta.x - motion.x) * 0.2,
                motion.y + (delta.y - motion.y) * 0.2,
                motion.z + (delta.z - motion.z) * 0.2,
            );
            scene.hits.bullet_delta = Some(delta);
        }
        None => apply_vector_gravity_from_1_14(&mut state.velocity, state.gravity, release),
    }
    let (block, stuck) = flight_block(state, world, release, ClipShape::Collider, scene);
    let hit = scene.flight_hit(state.position, state.velocity, block, stuck, release);
    // (1.21.1 ShulkerBullet.java:218; 1.21.2 ShulkerBullet.java:228)
    let moves_first = release.at_least(1, 21, 2);
    if !moves_first {
        if let Some(hit) = hit {
            scene.hit_flight(hit, state, release);
        }
    }
    if effects_before_move && !scene.removed {
        let at = state.position;
        push_by_bubble_columns_inside(&mut state.velocity, at, size, world, release);
    }
    let start = state.position;
    state.position = state.position + state.velocity;
    if effects_after_move {
        push_by_bubble_columns_along(&mut state.velocity, start, state.position, size, world, release);
    }
    if moves_first {
        if let Some(hit) = hit {
            scene.hit_flight(hit, state, release);
        }
    }
    ProjectileTickResult::of(state, hit.is_some_and(FlightHit::is_block), false)
}

/// Entity.move(SELF) of a 0.25-wide projectile by its motion, with the collision flags it leaves.
/// From 1.21.4 a move with no vertical part leaves onGround as it was on the client, where no
/// one controls the projectile, and clears it on the server, which is the authority on its own
/// entities (1.21.4 Entity.java:707, 1.21.5 :729-732). `server` is that side.
fn move_projectile(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    server: bool,
) {
    if state.no_physics {
        // Entity.move's noPhysics branch keeps the flags until 1.21.9 clears the collisions, the
        // ground never (1.21.8 Entity.java:706-707, 1.21.9 :713-718); before 1.17 the box moves
        // and the entity follows it (1.13.2 Entity.java:570-571, 1.16.5 :610-612)
        state.position = if release.at_least(1, 17, 0) {
            state.position + state.velocity
        } else {
            position_of_bounding_box(projectile_box(state.position).translated(state.velocity))
        };
        if release.at_least(1, 21, 9) {
            state.horizontal_collision = false;
        }
        return;
    }
    // (Math.abs(delta.y) > 0.0 refreshes the ground, false for NaN: 1.21.4 Entity.java:706)
    let keeps_on_ground = !server && release.at_least(1, 21, 4) && !(state.velocity.y.abs() > 0.0);
    let on_ground = state.on_ground;
    let mut body = SimulationState {
        position: state.position,
        velocity: state.velocity,
        half_width: f64::from(PROJECTILE_SIZE / 2.0),
        height: f64::from(PROJECTILE_SIZE),
        on_ground: state.on_ground,
        collided_horizontally: state.horizontal_collision,
        controlled_by_local_instance: true,
        // (1.16.5 Entity.java:631-632; 1.17 Entity.java:559)
        bounding_box: (!release.at_least(1, 17, 0))
            .then(|| state.bounding_box.unwrap_or_else(|| projectile_box(state.position))),
        stuck_speed_multiplier: state.stuck_speed_multiplier,
        // (no LivingEntity: a sweet berry bush leaves it be, powder snow holds it wherever it is,
        // 26.3 SweetBerryBushBlock.java, PowderSnowBlock.java:59-61)
        sweet_berry_bush_immune: true,
        non_living: true,
        ..SimulationState::default()
    };
    let movement = body.velocity;
    // the restitution's getEffectiveGravity is getGravity(): 0 for a rocket, and for a fishing
    // hook 0 in 26.2, where its tick adds a literal -0.03, and its getDefaultGravity 0.03F from
    // 26.3 (26.2 FishingHook.java:227, Entity.java:848, 855, 887-889, 1528-1534; 26.3
    // FishingHook.java:227, 468-470, Entity.java:878, 885, 918-920, 1559-1565)
    let rules = MoveRules {
        gravity: if state.kind == ProjectileKind::FishingHook && release.at_least(26, 3, 0) {
            state.gravity
        } else {
            0.0
        },
        ..PROJECTILE_MOVE
    };
    // the blocks the hook or rocket is in act on it: Entity.move's checkInsideBlocks until 1.21.1,
    // the tick's applyEffectsFromBlocks after the move from 1.21.2 (1.12.2 Entity.java doBlockCollisions
    // in move, 1.21.1 Entity.java:738; 26.3 FishingHook.java:230-231, FireworkRocketEntity.java:147-148)
    let start = body.position;
    let _ = move_vehicle(
        &mut body,
        world,
        release,
        movement,
        rules,
        |body, _| {
            let cells: Vec<_> = blocks_inside(body, release)
                .into_iter()
                .map(|cell| (cell, true))
                .collect();
            let (at, on_ground) = (body.position, body.on_ground);
            apply_projectile_cells(&mut body.velocity, at, PROJECTILE_SIZE, on_ground, &cells, world, release, BubblePushes::Both, false);
            stick_in_blocks(body, world, &cells, release);
        },
    );
    if release.at_least(1, 21, 2) {
        let movements = [Movement {
            from: start,
            to: body.position,
            axis_order: None,
        }];
        let cells = blocks_visited(&movements, body.bounds(), body.half_width, body.height, release);
        let (at, on_ground) = (body.position, body.on_ground);
        apply_projectile_cells(&mut body.velocity, at, PROJECTILE_SIZE, on_ground, &cells, world, release, BubblePushes::Both, false);
        stick_in_blocks(&mut body, world, &cells, release);
    }
    state.stuck_speed_multiplier = body.stuck_speed_multiplier;
    state.position = body.position;
    state.bounding_box = body.bounding_box;
    state.velocity = body.velocity;
    state.on_ground = if keeps_on_ground {
        on_ground
    } else {
        body.on_ground
    };
    state.horizontal_collision = body.collided_horizontally;
}

/// Entity.makeStuckInBlock of the blocks a hook or rocket is in: a cobweb's 0.25, 0.05F, 0.25
/// (WebBlock.entityInside, 1.8.9 Entity.setInWeb), powder snow's 0.9F, 1.5, 0.9F from 1.17, which
/// its next move takes (1.12.2 Entity.java move isInWeb, 26.3 Entity.java:783-789), its fall reset.
fn stick_in_blocks(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    cells: &[((i32, i32, i32), bool)],
    release: MinecraftVersion,
) {
    if let Some(multiplier) = crate::simulation::stuck_speed_multiplier_inside(body, world, cells, release) {
        body.stuck_speed_multiplier = multiplier;
        body.fall_distance = 0.0;
    }
}

/// A 0.25F x 0.25F projectile as the block-effect queries see it.
fn projectile_body(position: Vec3) -> SimulationState {
    SimulationState {
        position,
        half_width: f64::from(PROJECTILE_SIZE / 2.0),
        height: f64::from(PROJECTILE_SIZE),
        ..SimulationState::default()
    }
}

/// EntityDimensions.makeBoundingBox of the 0.25F x 0.25F projectiles.
fn projectile_box(position: Vec3) -> Aabb {
    let half_width = f64::from(PROJECTILE_SIZE / 2.0);
    Aabb::new(
        Vec3::new(position.x - half_width, position.y, position.z - half_width),
        Vec3::new(
            position.x + half_width,
            position.y + f64::from(PROJECTILE_SIZE),
            position.z + half_width,
        ),
    )
}

/// The block shapes a clip tests: ClipContext.Block.COLLIDER, or OUTLINE (the 1.20.3 and 1.20.4
/// wind charge); or Entity.move's FALLDAMAGE_RESETTING with ClipContext.Fluid.WATER, where every
/// #fall_damage_resetting block is a full cube, from 1.21.9 the end gateway and end portal too for a
/// player (1.19 ClipContext.java:91, 1.21.9 ClipContext.java:59-73), and the water's fluid shape
/// counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipShape {
    Collider,
    Outline,
    FallDamageResetting { player: bool },
}

/// A block hit of a projectile's clip: where, and the block whose shape it hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BlockHit {
    pub(crate) location: Vec3,
    pub(crate) block: (i32, i32, i32),
    pub(crate) face: HitFace,
    /// BlockHitResult.isWorldBorderHit (from 1.21.2).
    pub(crate) border: bool,
    /// The border turned the arrow back, which is no hit.
    pub(crate) bounced: bool,
}

impl BlockHit {
    pub(crate) fn report(&self) -> ProjectileBlockHit {
        ProjectileBlockHit {
            block: self.block,
            face: self.face,
            location: self.location,
            border: self.border,
        }
    }
}

/// What a family's query found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FlightHit {
    Entity(Found),
    Block(BlockHit),
}

impl FlightHit {
    /// HitResult.getLocation: where a moved projectile stops.
    pub(crate) fn location(self) -> Vec3 {
        match self {
            Self::Entity((_, point)) => point,
            Self::Block(hit) => hit.location,
        }
    }

    /// `getType() == HitResult.Type.BLOCK`: whether the engine's hit is a block's (the abstract getType of vanilla's hit result is the arm of the enum here).
    pub(crate) const fn is_block(self) -> bool {
        matches!(self, Self::Block(_))
    }
}

/// The block shapes a family clips. The thrown items and llama spit clipped outlines until 1.16.1,
/// which a torch or tall grass stops (1.8.8 Level.rayTrace(a, b) with nothing ignored, 1.15
/// ThrownEntity and LlamaSpitEntity with ShapeType.OUTLINE, 1.16 ThrowableProjectile.java:48, LlamaSpitEntity),
/// and colliders from 1.16.2 (1.16.5 ProjectileUtil.java:59-63). So did the 1.8 fireball (1.8.9
/// ExplosiveProjectileEntity), and ProjectileUtil's colliders hold from 1.9 (1.12.2
/// ProjectileUtil.java:27), but for the outlines of the wind charge of 1.20.3 and 1.20.4 (1.20.4
/// WindCharge.getClipType, gone in 1.20.5). (1.20.4 WindCharge.java:136-137 OUTLINE; 1.16.1
/// ThrowableProjectile.java:48, LlamaSpit.java:51 OUTLINE, 1.16.2 ThrowableProjectile.java:47, ProjectileUtil.java:64
/// COLLIDER; 1.8.8 AbstractHurtingProjectile.java:111 rayTrace with nothing ignored, 1.9.4 ProjectileUtil.java:22
/// ignoring blocks with no collision box)
fn flight_clip_shape(kind: ProjectileKind, release: MinecraftVersion) -> ClipShape {
    let outline = match kind {
        ProjectileKind::WindCharge => !release.at_least(1, 20, 5),
        ProjectileKind::LlamaSpit => !release.at_least(1, 16, 2),
        _ if kind.is_thrown() => !release.at_least(1, 16, 2),
        _ if kind.is_hurting() => !release.at_least(1, 9, 0),
        _ => false,
    };
    if outline {
        ClipShape::Outline
    } else {
        ClipShape::Collider
    }
}

/// What a hurting projectile or shulker bullet's clip gives, and whether it is the test of 1.13 to
/// 1.15 that the projectile's box lies in a block: that answers with the projectile's own cell and
/// its own position, before any clip (1.13.2 ProjectileUtil.java:27-30, 1.15 ProjectileUtil.java:
/// 100-103, 1.15.2 :92-93; gone in 1.16, 1.16 ProjectileUtil.java:59-64 clips at once). The test also meets hard
/// entities, which are not modelled here.
fn flight_block(
    state: &ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    shape: ClipShape,
    scene: &mut HitScene<'_>,
) -> (Option<BlockHit>, bool) {
    let tests_box = scene.live && release.at_least(1, 13, 0) && !release.at_least(1, 16, 0);
    let bounds = state.kind.bounding_box(state.position);
    if tests_box && !(no_block_collision(world, bounds, release) && no_border_cell(world, bounds, state.position, scene, release)) {
        let position = state.position;
        let v = state.velocity;
        let hit = BlockHit {
            location: position,
            block: (position.x.floor() as i32, position.y.floor() as i32, position.z.floor() as i32),
            face: HitFace::nearest(v.x as f32, v.y as f32, v.z as f32),
            border: false,
            bounced: false,
        };
        return (Some(hit), true);
    }
    (clip_blocks(world, state.position, state.velocity, release, shape), false)
}

/// BlockGetter.clip with ClipContext.Fluid.NONE: BlockGetter.traverseBlocks visits the cells along
/// the segment, from the start nudged back by 1.0E-7 of the segment, and the first cell whose shape
/// the segment hits gives the hit (26.2 BlockGetter.java:63-80, :110-163; 1.14 BlockView). From
/// 1.21.2 clipIncludingBorder: a clip starting inside the border that ends outside it, on a block
/// or at its end, hits the border at the clamped end (1.21.2 CollisionGetter.java:119-130,
/// ProjectileUtil.java:51; 1.21.1 ProjectileUtil.java has the plain clip; 26.3 CollisionGetter.java:
/// 114-125, ProjectileUtil.java:67, AbstractArrow.java:236).
pub(crate) fn clip_blocks(
    world: &WorldSnapshot,
    from: Vec3,
    movement: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> Option<BlockHit> {
    let to = from + movement;
    let hit = clip_blocks_between(world, from, to, release, shape);
    if !release.at_least(1, 21, 2) {
        return hit;
    }
    let end = hit.map_or(to, |hit| hit.location);
    if world.is_inside_world_border(from, release) && !world.is_inside_world_border(end, release) {
        let location = world.clamp_to_world_border(end, release);
        let block = (
            floor_i32_for(release, location.x),
            floor_i32_for(release, location.y),
            floor_i32_for(release, location.z),
        );
        // Direction.getApproximateNearest of the way there (26.3 CollisionGetter.java:118)
        let delta = end - from;
        return Some(BlockHit {
            location,
            block,
            face: HitFace::nearest(delta.x as f32, delta.y as f32, delta.z as f32),
            border: true,
            bounced: false,
        });
    }
    hit
}

/// Whether BlockGetter.clip from `from` to `to` hits a block (or, for FALLDAMAGE_RESETTING, water).
pub(crate) fn clip_hits(
    world: &WorldSnapshot,
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> bool {
    clip_blocks_between(world, from, to, release, shape).is_some()
}

fn clip_blocks_between(
    world: &WorldSnapshot,
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> Option<BlockHit> {
    if from == to {
        return None;
    }
    // 1.13.2 still walks the cells the old way, but clips each with the whole segment (1.13.2
    // Level.java:719-878, Block.java:567-568); BlockGetter.traverseBlocks from 1.14 (1.14.4
    // BlockGetter.java:68-96)
    if !release.at_least(1, 14, 0) {
        return clip_blocks_legacy(world, from, to, release, shape);
    }
    traverse_blocks(from, to, release, |block| {
        clip_block(world, block, from, to, release, shape).map(|(location, face)| BlockHit {
            location,
            block,
            face,
            border: false,
            bounced: false,
        })
    })
}

/// BlockGetter.traverseBlocks from 1.14 (1.14.4 BlockGetter.java:68-133, 1.21.11 :102-150): the
/// segment lengthened by 1.0E-7 at each end, its first cell asked first, then each cell it enters
/// in turn until `visit` answers or the walk passes the end. Equal segment ends visit nothing.
pub(crate) fn traverse_blocks<T>(
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
    mut visit: impl FnMut((i32, i32, i32)) -> Option<T>,
) -> Option<T> {
    if from == to {
        return None;
    }
    let lerp = |delta: f64, start: f64, end: f64| start + delta * (end - start);
    let end = Vec3::new(
        lerp(-1.0e-7, to.x, from.x),
        lerp(-1.0e-7, to.y, from.y),
        lerp(-1.0e-7, to.z, from.z),
    );
    let start = Vec3::new(
        lerp(-1.0e-7, from.x, to.x),
        lerp(-1.0e-7, from.y, to.y),
        lerp(-1.0e-7, from.z, to.z),
    );
    let mut block = (
        floor_i32_for(release, start.x),
        floor_i32_for(release, start.y),
        floor_i32_for(release, start.z),
    );
    if let Some(found) = visit(block) {
        return Some(found);
    }
    let sign = |value: f64| {
        if value > 0.0 {
            1
        } else if value < 0.0 {
            -1
        } else {
            0
        }
    };
    let frac = |value: f64| frac_f64_for(release, value);
    let delta = end - start;
    let (step_x, step_y, step_z) = (sign(delta.x), sign(delta.y), sign(delta.z));
    let advance = |step: i32, delta: f64| {
        if step == 0 {
            f64::MAX
        } else {
            f64::from(step) / delta
        }
    };
    let (advance_x, advance_y, advance_z) = (
        advance(step_x, delta.x),
        advance(step_y, delta.y),
        advance(step_z, delta.z),
    );
    let next = |step: i32, advance: f64, start: f64| {
        advance * if step > 0 { 1.0 - frac(start) } else { frac(start) }
    };
    let mut next_x = next(step_x, advance_x, start.x);
    let mut next_y = next(step_y, advance_y, start.y);
    let mut next_z = next(step_z, advance_z, start.z);
    while next_x <= 1.0 || next_y <= 1.0 || next_z <= 1.0 {
        if next_x < next_y {
            if next_x < next_z {
                block.0 += step_x;
                next_x += advance_x;
            } else {
                block.2 += step_z;
                next_z += advance_z;
            }
        } else if next_y < next_z {
            block.1 += step_y;
            next_y += advance_y;
        } else {
            block.2 += step_z;
            next_z += advance_z;
        }
        if let Some(found) = visit(block) {
            return Some(found);
        }
    }
    None
}

/// Level.rayTrace before 1.14 (1.8.8 Level.java:799-923, 1.12.2 Level.java:863-980, 1.13.2
/// Level.java:719-878): each step moves the ray's start to the cell's edge. Before 1.13 each block
/// clips what is left of the ray, whose last sliver a Vec3d's per-axis cutoff can miss; 1.13.2 keeps
/// the moved start in locals and gives each block the whole segment (Level.java:837-838). Up to 201
/// steps.
fn clip_blocks_legacy(
    world: &WorldSnapshot,
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> Option<BlockHit> {
    let end = (to.x.floor() as i32, to.y.floor() as i32, to.z.floor() as i32);
    let mut start = from;
    let mut block = (from.x.floor() as i32, from.y.floor() as i32, from.z.floor() as i32);
    let whole_segment = release.at_least(1, 13, 0);
    let hit_at = |block: (i32, i32, i32), start: Vec3| {
        let start = if whole_segment { from } else { start };
        clip_block(world, block, start, to, release, shape).map(|(location, face)| BlockHit {
            location,
            block,
            face,
            border: false,
            bounced: false,
        })
    };
    if let Some(hit) = hit_at(block, start) {
        return Some(hit);
    }
    let plane = |target: i32, at: i32| match target.cmp(&at) {
        std::cmp::Ordering::Greater => Some(f64::from(at) + 1.0),
        std::cmp::Ordering::Less => Some(f64::from(at)),
        std::cmp::Ordering::Equal => None,
    };
    for _ in 0..=200 {
        if block == end {
            return None;
        }
        let delta = to - start;
        let along = |edge: Option<f64>, at: f64, step: f64| {
            let part = edge.map_or(999.0, |edge| (edge - at) / step);
            // (a -0.0 means a start on the edge, going back)
            if part == 0.0 && part.is_sign_negative() {
                -1.0e-4
            } else {
                part
            }
        };
        let (edge_x, edge_y, edge_z) = (plane(end.0, block.0), plane(end.1, block.1), plane(end.2, block.2));
        let (x, y, z) = (along(edge_x, start.x, delta.x), along(edge_y, start.y, delta.y), along(edge_z, start.z, delta.z));
        let (mut west_low, mut down_low, mut north_low) = (0, 0, 0);
        if x < y && x < z {
            west_low = i32::from(end.0 <= block.0);
            start = Vec3::new(edge_x.unwrap_or(999.0), start.y + delta.y * x, start.z + delta.z * x);
        } else if y < z {
            down_low = i32::from(end.1 <= block.1);
            start = Vec3::new(start.x + delta.x * y, edge_y.unwrap_or(999.0), start.z + delta.z * y);
        } else {
            north_low = i32::from(end.2 <= block.2);
            start = Vec3::new(start.x + delta.x * z, start.y + delta.y * z, edge_z.unwrap_or(999.0));
        }
        block = (
            start.x.floor() as i32 - west_low,
            start.y.floor() as i32 - down_low,
            start.z.floor() as i32 - north_low,
        );
        if let Some(hit) = hit_at(block, start) {
            return Some(hit);
        }
    }
    None
}

/// VoxelShape.clip of the block at `block` (26.2 VoxelShape.java:119-131, 1.13.2 :144-159):
/// from 1.13 a segment shorter than sqrt(1.0E-7) hits nothing, and one whose point 0.001 of the
/// way along is inside the shape hits there, facing the way the segment goes in 1.13.2
/// (Direction.getNearest, VoxelShape.java:157) and back against it from 1.14 (1.14.4
/// VoxelShape.java:134, getOpposite); otherwise AABB.clip, the nearest entering face of the
/// boxes. Before 1.13 the block clips six faces in local coordinates instead.
///
/// 1.13.2's rayTrace clips Block.getShape, the outline, of every block but air and a liquid
/// (Block.java:567-568, method_400 false in AirBlock.java:28 and LiquidBlock.java:85), skipping a
/// block with no collision shape when the caller asks it to, as arrows, hurting projectiles and
/// shulker bullets do (Level.java:737, 836; AbstractArrow.java:205, ProjectileUtil.java:32): the
/// collider here. From 1.14 ClipContext's COLLIDER clips the collision shape itself (1.14.4
/// ClipContext.java). The interaction shape's face override (1.13.2 Block.java:570-573) is not
/// modelled.
/// An unloaded chunk is air on the client. This does not replace the shared cell
/// walker with legacy Level.rayTrace or model block-specific multi-box selectors.
pub(crate) fn clip_block(
    world: &WorldSnapshot,
    block: (i32, i32, i32),
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> Option<(Vec3, HitFace)> {
    if !world.is_chunk_loaded(block.0.div_euclid(16), block.2.div_euclid(16)) {
        return None;
    }
    if !release.at_least(1, 13, 0) {
        return clip_block_legacy(world, block, from, to, shape);
    }
    let ray_trace_1_13 = !release.at_least(1, 14, 0);
    let local_boxes = match shape {
        _ if ray_trace_1_13 && matches!(world.block_cow(block).fluid, FluidState::Water | FluidState::Lava) => Vec::new(),
        ClipShape::Collider if ray_trace_1_13 => {
            // (a block given no outline is clipped by its collision shape, which Block.java:421-423
            // makes the outline unless the block overrides it)
            let collision = world.block_collision_local_boxes(block);
            let outline = world.block_outline_local_boxes(block);
            if collision.is_empty() || outline.is_empty() {
                collision
            } else {
                outline
            }
        }
        ClipShape::Collider => world.block_collision_local_boxes(block),
        ClipShape::Outline => world.block_outline_local_boxes(block),
        ClipShape::FallDamageResetting { player } => {
            fall_damage_resetting_boxes(world, block, player, release)
        }
    };
    if local_boxes.is_empty() {
        return None;
    }
    let delta = to - from;
    if delta.length_squared() < 1.0e-7 {
        return None;
    }
    let inside = from + delta.scale(0.001);
    // VoxelShape.findIndex takes the point less the block's position, in the shape's own
    // coordinates (1.13.2 VoxelShape.java:153-155, 26.3 VoxelShape.java:119-128)
    let origin = Vec3::new(f64::from(block.0), f64::from(block.1), f64::from(block.2));
    let local_inside = Vec3::new(
        inside.x - origin.x,
        inside.y - origin.y,
        inside.z - origin.z,
    );
    let boxes: Vec<Aabb> = local_boxes
        .iter()
        .map(|bounds| bounds.translated(origin))
        .collect();
    if local_boxes.iter().any(|bounds| bounds.contains(local_inside)) {
        let face = HitFace::nearest(delta.x as f32, delta.y as f32, delta.z as f32);
        return Some((inside, if ray_trace_1_13 { face } else { face.opposite() }));
    }
    let delta = to - from;
    let mut best = 1.0;
    let mut hit = None;
    for bounds in boxes {
        bounds.clip_faces(from, delta, &mut best, &mut hit);
    }
    hit.map(|face| (from + delta.scale(best), face))
}

/// Block.method_11559 (1.9.4 Block.java:528-532, 1.12.2 :560-564) subtracts
/// the cell before clipping, then adds it back. Translating the box into world
/// coordinates instead changes rounding, particularly at large coordinates.
fn clip_block_legacy(
    world: &WorldSnapshot,
    block: (i32, i32, i32),
    from: Vec3,
    to: Vec3,
    shape: ClipShape,
) -> Option<(Vec3, HitFace)> {
    let (collision, outline, _) = world.block_geometry(block);
    let has_box = |shapes: &[crate::world::BlockShape]| {
        shapes.iter().any(|shape| {
            let bounds = shape.bounds;
            bounds.x_size() > 0.0 && bounds.y_size() > 0.0 && bounds.z_size() > 0.0
        })
    };
    let boxes = match shape {
        // World.rayTraceBlocks(start, end, false, true, false) skips a block with no collision
        // box and clips Block.rayTrace's box of the rest, getBoundingBox, the outline (1.12.2
        // World.java:897-898, Block.java:555-556); a block given no outline is clipped by its
        // collision shape
        ClipShape::Collider if has_box(&collision) && has_box(&outline) => outline,
        ClipShape::Collider => collision,
        ClipShape::Outline => outline,
        // This query is only used by the 1.21.2+ fall-distance reset path.
        ClipShape::FallDamageResetting { .. } => return None,
    };
    let offset = Vec3::new(f64::from(block.0), f64::from(block.1), f64::from(block.2));
    // Every subtract/add here constructs a legacy Vec3d, including zero normalization.
    let vector = |value: Vec3| {
        let zero = |coordinate: f64| if coordinate == 0.0 { 0.0 } else { coordinate };
        Vec3::new(zero(value.x), zero(value.y), zero(value.z))
    };
    let local_from = vector(from - offset);
    let local_to = vector(to - offset);
    let mut nearest = None;
    for shape in boxes {
        let bounds = shape.bounds;
        if bounds.x_size() <= 0.0 || bounds.y_size() <= 0.0 || bounds.z_size() <= 0.0 {
            continue;
        }
        if let Some((local_hit, face)) = bounds.clip_segment_legacy_face(local_from, local_to) {
            let hit = vector(local_hit + offset);
            let distance = from.distance_squared(hit);
            // Retain the shared nearest-box selection. Legacy per-block selectors,
            // such as StairsBlock's farthest-from-end rule, are a separate contract.
            if nearest.map_or(true, |(best, _, _)| distance < best) {
                nearest = Some((distance, hit, face));
            }
        }
    }
    nearest.map(|(_, hit, face)| (hit, face))
}

/// The shapes FALLDAMAGE_RESETTING and Fluid.WATER give the cell, in its own coordinates: a full cube for a resetting
/// block, and the water's FluidState.getShape, a box up to its height (BlockGetter.clip takes the
/// nearer of the block and fluid hits, so either one hits).
fn fall_damage_resetting_boxes(
    world: &WorldSnapshot,
    block: (i32, i32, i32),
    player: bool,
    release: MinecraftVersion,
) -> Vec<Aabb> {
    let state = world.block_cow(block);
    let origin = Vec3::ZERO;
    let mut boxes = Vec::new();
    if state.fall_damage_resetting
        // (1.21.8 ClipContext.java:57 (tag only); 1.21.9 ClipContext.java:60-81 (a player also resets at end gateway, end portal, and nether portal with delay 0))
        || (player && release.at_least(1, 21, 9) && state.player_fall_damage_resetting)
    {
        boxes.push(Aabb::new(origin, origin + Vec3::new(1.0, 1.0, 1.0)));
    }
    if is_water(state.fluid) {
        let height = world.fluid_height_at(block);
        boxes.push(Aabb::new(origin, origin + Vec3::new(1.0, height, 1.0)));
    }
    boxes
}

/// Whether no block collision box overlaps `query` (Level.noCollision for the blocks).
/// The border's part of 1.13's noCollision: the query takes each cell outside the border as a full
/// block when the entity is inside it, with a block's margin either side that its kept
/// isOutsideWorldBorder chooses, and keeps the answer (1.13.2 LevelReader.java:113-155, 172-200,
/// ProjectileUtil.java:27). From 1.14 the border's shape counts only for a box that meets its edge
/// from inside (1.14.4 LevelReader.java:147-157), which a box that flew out does not.
fn no_border_cell(world: &WorldSnapshot, query: Aabb, position: Vec3, scene: &mut HitScene<'_>, release: MinecraftVersion) -> bool {
    if release.at_least(1, 14, 0) {
        return true;
    }
    let inside = world.is_inside_legacy_world_border(position, scene.hits.outside_border);
    scene.hits.outside_border = !inside;
    !inside || world.legacy_world_border_cells(query, release).iter().all(|cell| !cell.intersects(query))
}

/// The blocks' part of CollisionGetter#noCollision: no block collision box overlaps `query`.
fn no_block_collision(world: &WorldSnapshot, query: Aabb, release: MinecraftVersion) -> bool {
    !world
        .collision_shapes(query, release)
        .iter()
        .any(|bounds| bounds.intersects(query))
}

/// Math.signum: zero stays zero.
fn java_signum(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

/// The block collider the segment from `start` along `movement` meets first.
#[cfg(test)]
fn nearest_block_hit(
    world: &WorldSnapshot,
    start: Vec3,
    movement: Vec3,
    release: MinecraftVersion,
) -> Option<Vec3> {
    clip_blocks(world, start, movement, release, ClipShape::Collider).map(|hit| hit.location)
}

#[cfg(test)]
mod tests {
    use super::{
        tick_arrow, tick_arrow_in_release, tick_arrow_in_world, tick_projectile,
        tick_projectile_in_release, tick_projectile_in_world,
        clip_block, clip_blocks, clip_hits, tick_trident, tick_trident_in_release, tick_trident_in_world,
        ArrowState, ClipShape, ProjectileKind, ProjectileState,
        TridentState,
    };
    use crate::math::{Aabb, Vec3};

    #[test]
    fn a_spit_or_bullet_falls_by_a_vector_add_from_1_14() {
        // 1.14.4 LlamaSpit.java:94 adds (0, -0.06F, 0), which makes a -0 x +0; 1.13.2 :128 only takes y
        let falling = |release| {
            let mut velocity = Vec3::new(-0.0, 0.5, -0.0);
            super::apply_vector_gravity_from_1_14(&mut velocity, f64::from(0.06_f32), release);
            velocity
        };
        let newer = falling(crate::version::MinecraftVersion::new(1, 14, 4));
        assert!(newer.x.is_sign_positive() && newer.z.is_sign_positive());
        let older = falling(crate::version::MinecraftVersion::new(1, 13, 2));
        assert!(older.x.is_sign_negative() && older.z.is_sign_negative());
        assert_eq!(newer.y, older.y);
    }
    use crate::version::MinecraftVersion;
    use crate::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

    fn full_block_world(position: (i32, i32, i32)) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                shape: Some(BlockShape::full_block()),
                outline_shape: Some(BlockShape::full_block()),
                ..Default::default()
            },
        );
        world
    }

    #[test]
    fn a_fishing_hooks_restitution_reads_its_gravity_from_26_3() {
        // Entity.restituteMovementAfterCollisions reads getEffectiveGravity, getGravity(): 0 for
        // the 26.2 hook, which adds a literal -0.03 (26.2 FishingHook.java:227, Entity.java:848,
        // 855, 1528-1534), its getDefaultGravity 0.03F from 26.3 (26.3 FishingHook.java:468-470,
        // Entity.java:878, 885): the bounce off a slime block gains the gravity's share of the
        // portion it moved, and a fall slower than the gravity rests
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 1.0,
                ..Default::default()
            },
        );
        let bounce = |velocity_y: f64, version: MinecraftVersion| {
            let mut hook = ProjectileState::for_kind_in_release(
                ProjectileKind::FishingHook,
                Vec3::new(0.5, 1.0 - velocity_y / 4.0, 0.5),
                Vec3::new(0.0, velocity_y, 0.0),
                version,
            );
            super::move_projectile(&mut hook, &world, version, false);
            assert!(hook.on_ground, "{version}");
            hook.velocity.y
        };
        let gravity = f64::from(0.03_f32);
        let restitution = f64::from(1.0_f32 * 0.8);
        let portion = -0.05 / -0.2;
        let drag = 1.0 + portion * (f64::from(0.98_f32) - 1.0);
        assert_eq!(bounce(-0.2, release(26, 2, 0)), (0.0 + 0.2) * drag * restitution);
        assert_eq!(bounce(-0.2, release(26, 3, 0)), (portion * gravity + 0.2) * drag * restitution);
        // falling 0.02 a tick: past the 26.2 gate (< 0), at rest under the 26.3 one (<= 0.03F)
        assert!(bounce(-0.02, release(26, 2, 0)) > 0.0);
        assert_eq!(bounce(-0.02, release(26, 3, 0)), 0.0);
    }

    #[test]
    fn an_arrow_before_1_14_clips_the_outline_of_a_block_with_a_collision_box() {
        // 1.8 - 1.12 World.rayTraceBlocks(.., ignoreBlockWithoutBoundingBox) clips Block.rayTrace's
        // getBoundingBox of a block with a collision box (1.12.2 World.java:897-898,
        // Block.java:555-556); 1.13.2 the outline of one (VoxelShape rayTrace, Block.java:567-568);
        // from 1.14 COLLIDER clips the collision shape. A fence post 1.5 high outlined to 1.0: a ray
        // at 1.2 passes over it before 1.14 and hits it from 1.14.
        let block = (2, 0, 0);
        let mut world = WorldSnapshot::new();
        let post = |top: f64| BlockShape {
            bounds: Aabb::new(Vec3::new(0.375, 0.0, 0.375), Vec3::new(0.625, top, 0.625)),
            ..BlockShape::full_block()
        };
        world.set_block(
            block,
            BlockState {
                shape: Some(post(1.5)),
                outline_shape: Some(post(1.0)),
                ..Default::default()
            },
        );
        let from = Vec3::new(0.5, 1.2, 0.5);
        let to = Vec3::new(4.5, 1.2, 0.5);
        for (version, hits) in [
            (release(1, 8, 8), false),
            (release(1, 12, 2), false),
            (release(1, 13, 2), false),
            (release(1, 14, 4), true),
            (release(26, 3, 0), true),
        ] {
            let hit = clip_block(&world, block, from, to, version, ClipShape::Collider);
            assert_eq!(hit.is_some(), hits, "{version}");
        }
        // with no outline of its own, the collision box is what the ray meets
        let mut bare = WorldSnapshot::new();
        bare.set_block(
            block,
            BlockState {
                shape: Some(post(1.5)),
                ..Default::default()
            },
        );
        assert!(clip_block(&bare, block, from, to, release(1, 12, 2), ClipShape::Collider).is_some());
    }

    #[test]
    fn the_clips_inside_test_takes_the_point_in_the_blocks_own_coordinates() {
        // VoxelShape.clip: findIndex(testPoint.x - pos.getX()) against the shape's own
        // coordinates (1.13.2 VoxelShape.java:153-155, 26.3 VoxelShape.java:119-128). At x
        // 1000000.2, the world box's min (1000000 + 0.2 = 1000000.2) holds the point, but the
        // point less the block (0.19999999995343387) is short of the shape's 0.2: no inside hit,
        // and the vertical ray from in the box enters no face.
        let block = (1_000_000, 0, 0);
        let mut world = WorldSnapshot::new();
        let shape = BlockShape {
            bounds: Aabb::new(Vec3::new(0.2, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        world.set_block(
            block,
            BlockState {
                shape: Some(shape),
                outline_shape: Some(shape),
                ..Default::default()
            },
        );
        let edge = 1_000_000.0_f64 + 0.2;
        assert!(edge - 1_000_000.0 < 0.2);
        for version in [release(1, 13, 2), release(1, 14, 4), release(26, 3, 0)] {
            for shape in [ClipShape::Collider, ClipShape::Outline] {
                let from = Vec3::new(edge, 0.9, 0.5);
                let to = Vec3::new(edge, -0.1, 0.5);
                assert_eq!(clip_block(&world, block, from, to, version, shape), None, "{version}");
                // a point well inside is inside both ways
                let from = Vec3::new(edge + 0.05, 0.9, 0.5);
                let to = Vec3::new(edge + 0.05, -0.1, 0.5);
                let hit = clip_block(&world, block, from, to, version, shape);
                assert_eq!(hit.map(|(at, _)| at), Some(from + (to - from).scale(0.001)), "{version}");
            }
        }
    }

    #[test]
    fn applies_gravity_and_air_drag_before_move_vector_trace() {
        let mut state = ProjectileState::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0));
        let result = tick_projectile(&mut state, &WorldSnapshot::new());

        assert!((result.position.x - 0.9900000095367432).abs() < 1.0e-12);
        assert!((result.position.y - 0.9603000092506409).abs() < 1.0e-12);
        assert_eq!(result.position, result.velocity);
        assert!(!result.block_hit);
    }

    #[test]
    fn stops_at_nearest_block_hit() {
        let mut world = full_block_world((2, 0, 0));
        world.set_block(
            (4, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..Default::default()
            },
        );
        let mut state = ProjectileState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(5.0, 0.0, 0.0),
            gravity: 0.0,
            air_drag: 1.0,
            power: Vec3::ZERO,
            in_water: false,
            ultra_warm: false,
            first_tick: false,
            kind: ProjectileKind::Generic,
            horizontal_collision: false,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };

        let result = tick_projectile(&mut state, &world);

        assert!(result.block_hit);
        assert_eq!(result.position, Vec3::new(2.0, 0.5, 0.5));
        assert_eq!(state.position, result.position);
    }

    #[test]
    fn projectile_kind_aliases_cover_common_thrown_entity_families() {
        assert_eq!(ProjectileKind::from_name("snowball"), ProjectileKind::Snowball);
        assert_eq!(ProjectileKind::from_name("ender_pearl"), ProjectileKind::EnderPearl);
        assert_eq!(ProjectileKind::from_name("thrown_ender_pearl"), ProjectileKind::EnderPearl);
        assert_eq!(ProjectileKind::from_name("fishing_hook"), ProjectileKind::FishingHook);
        assert_eq!(ProjectileKind::from_name("small_fireball"), ProjectileKind::SmallFireball);
        assert_eq!(ProjectileKind::from_name("minecraft:small_fireball"), ProjectileKind::SmallFireball);
        assert_eq!(ProjectileKind::from_name("entity:small_fireball"), ProjectileKind::SmallFireball);
        assert_eq!(ProjectileKind::from_name("fire_charge"), ProjectileKind::SmallFireball);
        assert_eq!(ProjectileKind::from_name("large_fireball"), ProjectileKind::Fireball);
        assert_eq!(ProjectileKind::from_name("big_fireball"), ProjectileKind::Fireball);
        assert_eq!(ProjectileKind::from_name("wither_skull"), ProjectileKind::WitherSkull);
        assert_eq!(ProjectileKind::from_name("entity:wither_skull"), ProjectileKind::WitherSkull);
        assert_eq!(ProjectileKind::from_name("experience_bottle"), ProjectileKind::ExperienceBottle);
        assert_eq!(ProjectileKind::from_name("exp_bottle"), ProjectileKind::ExperienceBottle);
        assert_eq!(ProjectileKind::from_name("thrown_experience"), ProjectileKind::ExperienceBottle);
        assert_eq!(ProjectileKind::from_name("llama_spit"), ProjectileKind::LlamaSpit);
        assert_eq!(ProjectileKind::from_name("minecraft:llama_spit"), ProjectileKind::LlamaSpit);
        assert_eq!(ProjectileKind::from_name("shulker_bullet"), ProjectileKind::ShulkerBullet);
        assert_eq!(ProjectileKind::from_name("minecraft:firework_rocket"), ProjectileKind::Firework);
        assert_eq!(ProjectileKind::from_name("minecraft:dragon_fireball"), ProjectileKind::DragonFireball);
        assert_eq!(ProjectileKind::from_name("entity:firework_rocket"), ProjectileKind::Firework);
        assert_eq!(ProjectileKind::from_name("trident"), ProjectileKind::Trident);
        assert_eq!(ProjectileKind::from_name("thrown_trident"), ProjectileKind::Trident);
        assert_eq!(ProjectileKind::from_name_checked("unknown_projectile"), None);
        assert_eq!(ProjectileKind::from_name_checked("generic"), None);
    }

    #[test]
    fn projectile_kind_source_defaults_match_java_reference_gravity() {
        assert_eq!(ProjectileKind::Generic.default_gravity(), 0.03);
        assert_eq!(ProjectileKind::Potion.default_gravity(), 0.05);
        assert_eq!(ProjectileKind::ExperienceBottle.default_gravity(), 0.07);
        assert_eq!(ProjectileKind::Firework.default_gravity(), 0.0);
        assert_eq!(ProjectileKind::FishingHook.default_gravity(), 0.03);
        assert_eq!(ProjectileKind::Fireball.default_gravity(), 0.0);
        assert_eq!(ProjectileKind::Firework.default_air_drag(), 1.0);
        // AbstractHurtingProjectile.getInertia returns 0.95F (1.20.4 :131-132)
        assert_eq!(ProjectileKind::Fireball.default_air_drag(), f64::from(0.95_f32));
        assert_eq!(ProjectileKind::FishingHook.default_air_drag(), 0.92);
        assert_eq!(ProjectileKind::LlamaSpit.default_gravity(), 0.06);
        assert_eq!(ProjectileKind::ShulkerBullet.default_gravity(), 0.04);
        let state = ProjectileState::for_kind(
            ProjectileKind::LlamaSpit,
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
        );
        assert_eq!(state.gravity, 0.06);
        assert_eq!(state.air_drag, 0.99_f32 as f64);
    }

    #[test]
    fn uses_vanilla_water_inertia_instead_of_air_drag() {
        let mut state = ProjectileState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 0.0, 0.0),
            gravity: 0.0,
            air_drag: 0.99,
            power: Vec3::ZERO,
            in_water: true,
            first_tick: false,
            kind: crate::projectile::ProjectileKind::Generic,
            horizontal_collision: false,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };

        let result = tick_projectile(&mut state, &WorldSnapshot::new());

        assert_eq!(result.velocity.x, 0.8_f32 as f64);
    }

    #[test]
    fn arrow_applies_water_drag_before_move_and_gravity_after_air_drag() {
        let mut state = ArrowState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 1.0, 0.0),
            gravity: 0.05,
            air_drag: 0.99,
            water_inertia: 0.6,
            in_water: false,
            in_ground: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            fall_impulse: Vec3::ZERO,
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        };

        let result = tick_arrow(&mut state, &WorldSnapshot::new());

        assert_eq!(result.position, Vec3::new(1.0, 1.0, 0.0));
        assert!((result.velocity.x - 0.99).abs() < 1.0e-12);
        assert!((result.velocity.y - 0.94).abs() < 1.0e-12);
    }

    #[test]
    fn arrow_in_water_moves_by_the_motion_from_the_start_of_the_tick() {
        // 26.2 AbstractArrow.java:212-243: the water inertia scales the motion, the clip and the
        // move take the movement captured before it (1.21.1 :239-277 moves, then drags)
        let mut state = ArrowState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 1.0, 0.0),
            gravity: 0.05,
            air_drag: 0.99,
            water_inertia: 0.6,
            in_water: true,
            in_ground: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            fall_impulse: Vec3::ZERO,
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        };

        let result = tick_arrow(&mut state, &WorldSnapshot::new());

        assert_eq!(result.position, Vec3::new(1.0, 1.0, 0.0));
        assert!((result.velocity.x - 0.6).abs() < 1.0e-12);
        assert!((result.velocity.y - 0.55).abs() < 1.0e-12);
    }

    #[test]
    fn arrow_enters_and_stays_in_ground_at_block_hit() {
        let world = full_block_world((1, 0, 0));
        let mut state = ArrowState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(2.0, 0.0, 0.0),
            gravity: 0.0,
            air_drag: 1.0,
            water_inertia: 0.8,
            in_water: false,
            in_ground: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            fall_impulse: Vec3::ZERO,
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        };

        // 26.2 AbstractArrow.java:536-554 (1.21.11 :531-553): the hit position less
        // signum(v) * 0.05F, v = 0; inGroundTime stays 0 on the hit tick (:211-212), shake is 7
        let hit = tick_arrow(&mut state, &world);
        assert!(hit.block_hit);
        assert!(hit.in_ground);
        assert!(!hit.started_falling);
        assert_eq!(hit.position, Vec3::new(1.0 - 0.05000000074505806, 0.5, 0.5));
        assert_eq!(hit.velocity, Vec3::ZERO);
        assert_eq!(hit.in_ground_time, 0);
        assert_eq!(hit.shake_time, 7);
        assert_eq!(state.last_block_position, Some((1, 0, 0)));

        // 1.21.11 AbstractArrow.java:185-202: shakeTime counts down first, inGroundTime counts up
        let resting = tick_arrow(&mut state, &world);
        assert!(!resting.block_hit);
        assert!(resting.in_ground);
        assert_eq!(resting.velocity, Vec3::ZERO);
        assert_eq!(resting.in_ground_time, 1);
        assert_eq!(resting.shake_time, 6);
    }

    #[test]
    fn arrow_despawns_after_1200_ground_ticks() {
        let world = full_block_world((1, 0, 0));
        let mut state = ArrowState {
            position: Vec3::new(1.0, 0.5, 0.5),
            velocity: Vec3::ZERO,
            gravity: 0.0,
            air_drag: 1.0,
            water_inertia: 0.8,
            in_water: false,
            in_ground: true,
            life: 1199,
            in_ground_time: 1199,
            shake_time: 2,
            fall_impulse: Vec3::ZERO,
            last_block_id: Some(0),
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        };

        let result = tick_arrow(&mut state, &world);

        assert_eq!(state.life, 1200);
        assert!(result.discarded);
        assert_eq!(result.in_ground_time, 1200);
        assert_eq!(result.shake_time, 1);
    }

    #[test]
    fn arrow_starts_falling_without_advancing_when_supporting_block_disappears() {
        let mut state = ArrowState {
            position: Vec3::new(1.0, 0.5, 0.5),
            velocity: Vec3::new(1.0, 2.0, 3.0),
            gravity: 0.05,
            air_drag: 0.99,
            water_inertia: 0.8,
            in_water: false,
            in_ground: true,
            life: 55,
            in_ground_time: 12,
            shake_time: 3,
            fall_impulse: Vec3::new(0.1, 0.2, 0.3),
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        };

        let result = tick_arrow(&mut state, &WorldSnapshot::new());

        assert!(result.started_falling);
        assert!(!result.in_ground);
        assert_eq!(result.position, state.position);
        assert!((result.velocity.x - 0.1).abs() < 1.0e-12);
        assert!((result.velocity.y - 0.4).abs() < 1.0e-12);
        assert!((result.velocity.z - 0.9).abs() < 1.0e-12);
        assert_eq!(state.life, 0);
        // 1.21.11 AbstractArrow.java:185-202: the startFalling tick still runs inGroundTime++
        assert_eq!(result.in_ground_time, 13);
        assert_eq!(result.shake_time, 2);
    }

    #[test]
    fn arrow_stays_when_its_block_is_replaced_by_another_solid_block() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            BlockState {
                id: 11,
                shape: Some(BlockShape::full_block()),
                ..Default::default()
            },
        );
        let mut state = ArrowState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(2.0, 0.0, 0.0),
            gravity: 0.0,
            air_drag: 1.0,
            water_inertia: 0.6,
            in_water: false,
            in_ground: false,
            life: 0,
            in_ground_time: 0,
            shake_time: 0,
            fall_impulse: Vec3::new(0.5, 0.5, 0.5),
            last_block_id: None,
            last_block_position: None,
            no_physics: false,
            ultra_warm: false,
        };

        let hit = tick_arrow(&mut state, &world);
        assert!(hit.block_hit);
        world.set_block(
            (1, 0, 0),
            BlockState {
                id: 12,
                shape: Some(BlockShape::full_block()),
                ..Default::default()
            },
        );

        // 1.21.11 AbstractArrow.java:195 and :322-324: it falls only when the state differs AND
        // nothing collides with its point inflated by 0.06; the pulled-back point is 0.05 from
        // the replacement block
        let result = tick_arrow(&mut state, &world);

        assert_eq!(state.last_block_id, Some(11));
        assert!(!result.started_falling);
        assert!(result.in_ground);
        assert_eq!(state.life, 1);

        world.set_block((1, 0, 0), BlockState::default());
        let removed = tick_arrow(&mut state, &world);
        assert!(removed.started_falling);
        assert!(!removed.in_ground);
        assert_eq!(state.life, 0);
    }

    #[test]
    fn trident_uses_99_percent_water_inertia_and_arrow_gravity() {
        let mut state = TridentState::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0));
        state.in_water = true;
        let result = tick_trident(&mut state, &WorldSnapshot::new());

        // 26.2 AbstractArrow.java:166-253 with ThrownTrident's 0.99F water inertia: the move by
        // the motion from the start of the tick, one 0.99F, then the 0.05 gravity
        assert_eq!(result.position, Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(result.velocity.x, f64::from(0.99_f32));
        assert_eq!(result.velocity.y, f64::from(0.99_f32) - 0.05);
        assert!(!result.block_hit);
    }

    #[test]
    fn trident_hit_pulls_back_by_0_05_and_stops() {
        // 26.2 AbstractArrow.java:536-554: pos - signum(v) * 0.05000000074505806, v = 0
        let world = full_block_world((1, 0, 0));
        let mut state = TridentState::new(Vec3::new(0.5, 0.5, 0.5), Vec3::new(2.0, 0.0, 0.0));
        let result = tick_trident(&mut state, &world);

        assert!(result.block_hit);
        assert_eq!(
            result.position,
            Vec3::new(1.0 - 0.05000000074505806, 0.5, 0.5)
        );
        assert_eq!(result.velocity, Vec3::ZERO);
    }

    #[test]
    fn firework_rocket_scales_horizontal_velocity_and_adds_upward_boost() {
        let mut state = ProjectileState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 0.0, 1.0),
            gravity: 0.0,
            air_drag: 1.0,
            power: Vec3::ZERO,
            in_water: false,
            ultra_warm: false,
            first_tick: false,
            kind: ProjectileKind::Firework,
            horizontal_collision: false,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };

        let result = tick_projectile(&mut state, &WorldSnapshot::new());

        assert_eq!(result.velocity.x, 1.15);
        assert_eq!(result.velocity.z, 1.15);
        assert_eq!(result.velocity.y, 0.04);
    }

    #[test]
    fn firework_rocket_respects_horizontal_collision_and_shot_at_angle_flags() {
        let mut collided = ProjectileState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 0.0, 1.0),
            gravity: 0.0,
            air_drag: 1.0,
            power: Vec3::ZERO,
            in_water: false,
            ultra_warm: false,
            first_tick: false,
            kind: ProjectileKind::Firework,
            horizontal_collision: true,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };
        let collided_result = tick_projectile(&mut collided, &WorldSnapshot::new());
        assert_eq!(collided_result.velocity.x, 1.0);
        assert_eq!(collided_result.velocity.z, 1.0);

        let mut angled = ProjectileState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 0.0, 1.0),
            gravity: 0.0,
            air_drag: 1.0,
            power: Vec3::ZERO,
            in_water: false,
            ultra_warm: false,
            first_tick: false,
            kind: ProjectileKind::Firework,
            horizontal_collision: false,
            shot_at_angle: true,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };
        let angled_result = tick_projectile(&mut angled, &WorldSnapshot::new());
        assert_eq!(angled_result.velocity.x, 1.0);
        assert_eq!(angled_result.velocity.z, 1.0);
        assert_eq!(angled_result.velocity.y, 0.0);
    }

    #[test]
    fn hurting_projectile_applies_inertia_before_the_move() {
        // 26.2 AbstractHurtingProjectile.java:96-144: applyInertia (v + normalize(v) *
        // accelerationPower) * inertia, then the move by the new motion; the power vector is
        // pre-1.21 only and the gravity is not used
        let mut state = ProjectileState {
            position: Vec3::ZERO,
            velocity: Vec3::new(1.0, 0.0, 0.0),
            gravity: 0.05,
            air_drag: 0.95,
            power: Vec3::new(0.1, 0.0, 0.0),
            in_water: false,
            ultra_warm: false,
            first_tick: false,
            kind: ProjectileKind::Fireball,
            horizontal_collision: false,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };

        let result = tick_projectile(&mut state, &WorldSnapshot::new());

        assert_eq!(result.velocity.x, (1.0 + 0.1) * 0.95);
        assert_eq!(result.position, Vec3::new((1.0 + 0.1) * 0.95, 0.0, 0.0));
        assert_eq!(result.velocity.y, 0.0);
    }

    #[test]
    fn first_tick_applies_bubble_column_effect_before_gravity() {
        // 1.21.2 ThrowableProjectile.java:43-57 (26.2 the same): the first tick's column under air
        // (+0.1), the gravity, the move to y 0.17, then applyEffectsFromBlocks pushes again
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(false));
        let mut state = ProjectileState {
            position: Vec3::new(0.5, 0.1, 0.5),
            velocity: Vec3::ZERO,
            gravity: 0.03,
            air_drag: 1.0,
            power: Vec3::ZERO,
            in_water: false,
            ultra_warm: false,
            first_tick: true,
            kind: ProjectileKind::Generic,
            horizontal_collision: false,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };

        let _ = tick_projectile(&mut state, &world);

        assert_eq!(state.position.y, 0.1 + (0.0 + 0.1 - 0.03));
        assert_eq!(state.velocity.y, 0.0 + 0.1 - 0.03 + 0.1);
        assert!(!state.first_tick);

        // and every tick after, not only the first (1.21.2 ThrowableProjectile.java:56)
        let _ = tick_projectile(&mut state, &world);
        assert_eq!(state.velocity.y, 0.0 + 0.1 - 0.03 + 0.1 - 0.03 + 0.1);
    }

    #[test]
    fn legacy_throwable_takes_the_box_bubble_columns_before_the_move_from_1_16_2() {
        // 1.21.1 ThrowableProjectile.java:45 (1.20.5 ThrowableProjectile.java tick, 1.16.5 :69):
        // checkInsideBlocks after the hit test, before the motion is read for the move; the
        // column under air is Entity.onAboveBubbleCol's min(1.8, v + 0.1) (1.20.5
        // Entity.java:2357-2368, BubbleColumnBlock.java:48-51). 1.16.1 ThrowableProjectile.tick
        // and 1.16 ThrowableProjectile.tick have no such call
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(false));
        let start = ProjectileState {
            position: Vec3::new(0.5, 0.1, 0.5),
            velocity: Vec3::ZERO,
            gravity: 0.03,
            air_drag: 1.0,
            first_tick: true,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };

        for version in [release(1, 16, 2), release(1, 21, 1)] {
            let mut state = start;
            let _ = tick_projectile_in_release(&mut state, &world, version);
            assert_eq!(state.position.y, 0.1 + 0.1);
            assert_eq!(state.velocity.y, 0.1 - 0.03);
            assert!(!state.first_tick);
        }

        let mut state = start;
        let _ = tick_projectile_in_release(&mut state, &world, release(1, 16, 1));
        assert_eq!(state.position.y, 0.1);
        assert_eq!(state.velocity.y, -0.03);
    }

    #[test]
    fn first_tick_inside_downward_bubble_column_uses_inside_limit() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(true));
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = ProjectileState {
            position: Vec3::new(0.5, 0.1, 0.5),
            velocity: Vec3::new(0.0, -0.29, 0.0),
            gravity: 0.0,
            air_drag: 1.0,
            power: Vec3::ZERO,
            in_water: false,
            ultra_warm: false,
            first_tick: true,
            kind: ProjectileKind::Generic,
            horizontal_collision: false,
            shot_at_angle: false,
            ..ProjectileState::new(Vec3::ZERO, Vec3::ZERO)
        };
        let mut clamped = state;
        let _ = tick_projectile(&mut state, &world);
        let _ = tick_projectile_in_release(&mut clamped, &world, release(1, 21, 4));

        // Projectile.onInsideBubbleColumn adds -0.03 unclamped from 1.21.5 (1.21.5
        // Projectile.java:178-182), on the first tick and again in applyEffectsFromBlocks after the
        // move (1.21.5 ThrowableProjectile.java:57); Entity.onBubbleColumnCollision clamps it at
        // -0.3 until 1.21.4 (1.21.4 Entity.java:2564-2575)
        assert_eq!(state.velocity.y, -0.29 + -0.03 + -0.03);
        assert_eq!(clamped.velocity.y, -0.3);
    }

    fn release(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
        MinecraftVersion::new(major, minor, patch)
    }

    fn thrown(kind: ProjectileKind, position: Vec3, velocity: Vec3) -> ProjectileState {
        ProjectileState {
            first_tick: false,
            ..ProjectileState::for_kind(kind, position, velocity)
        }
    }

    fn bubble_column(drag_down: bool) -> BlockState {
        BlockState {
            id: 13,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                effect: if drag_down {
                    BlockEffect::BubbleDown
                } else {
                    BlockEffect::BubbleUp
                },
                ..BlockShape::full_block()
            }),
            fluid: FluidState::Water,
            ..BlockState::default()
        }
    }

    #[test]
    fn throwable_before_1_21_2_moves_by_the_old_motion_then_drags_and_falls() {
        // 1.21.1 ThrowableProjectile.java:39-66: the move takes the old motion, then 0.99F and the
        // gravity; 1.20.4 :94 and 1.8.8 ThrowableProjectile.java:245 return 0.03F
        let mut state = ProjectileState::for_kind_in_release(
            ProjectileKind::Snowball,
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 0.0),
            release(1, 20, 4),
        );
        assert_eq!(state.gravity, f64::from(0.03_f32));

        let result =
            tick_projectile_in_release(&mut state, &WorldSnapshot::new(), release(1, 20, 4));

        assert_eq!(result.position, Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(result.velocity.x, f64::from(0.99_f32));
        assert_eq!(result.velocity.y, f64::from(0.99_f32) - f64::from(0.03_f32));
        assert_eq!(
            ProjectileKind::ExperienceBottle.default_gravity_in(release(1, 8, 8)),
            f64::from(0.07_f32)
        );
        assert_eq!(
            ProjectileKind::Potion.default_gravity_in(release(1, 20, 5)),
            0.05
        );
    }

    #[test]
    fn throwable_before_1_21_2_moves_the_full_motion_through_a_hit() {
        // 1.21.1 ThrowableProjectile.java:41-66: the hit is handled, the move still goes to pos + v
        let world = full_block_world((2, 0, 0));
        let mut state = ProjectileState {
            gravity: 0.0,
            air_drag: 1.0,
            ..thrown(
                ProjectileKind::Snowball,
                Vec3::new(0.5, 0.5, 0.5),
                Vec3::new(5.0, 0.0, 0.0),
            )
        };

        let result = tick_projectile_in_release(&mut state, &world, release(1, 21, 1));

        assert!(result.block_hit);
        assert_eq!(result.position, Vec3::new(5.5, 0.5, 0.5));
    }

    #[test]
    fn first_tick_bubble_columns_push_from_1_21_2_for_every_cell_of_the_box() {
        // 1.21.2 ThrowableProjectile.java:83-95: each bubble column cell of the 0.25 box; the
        // lower cell has the column above it (inside: +0.06), the upper one air (surface: +0.1)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(false));
        world.set_block((0, 1, 0), bubble_column(false));
        for version in [release(1, 21, 2), release(26, 2, 0)] {
            let mut state = ProjectileState {
                gravity: 0.03,
                air_drag: 1.0,
                ..ProjectileState::new(Vec3::new(0.5, 0.9, 0.5), Vec3::ZERO)
            };
            let _ = tick_projectile_in_release(&mut state, &world, version);
            // then the move to y 1.03 and applyEffectsFromBlocks over the box there, in the upper
            // cell alone (1.21.2 ThrowableProjectile.java:56)
            assert_eq!(state.position.y, 0.9 + (0.0 + 0.06 + 0.1 - 0.03));
            assert_eq!(state.velocity.y, 0.0 + 0.06 + 0.1 - 0.03 + 0.1);
        }

        // 1.21.1 ThrowableProjectile.java:45: checkInsideBlocks over the same two cells (x, y, z
        // order, Entity.java checkInsideBlocks) before the move, which takes the pushed motion
        let mut legacy = ProjectileState {
            gravity: 0.03,
            air_drag: 1.0,
            ..ProjectileState::new(Vec3::new(0.5, 0.9, 0.5), Vec3::ZERO)
        };
        let _ = tick_projectile_in_release(&mut legacy, &world, release(1, 21, 1));
        assert_eq!(legacy.position.y, 0.9 + (0.0 + 0.06 + 0.1));
        assert_eq!(legacy.velocity.y, 0.0 + 0.06 + 0.1 - 0.03);
    }

    fn bubble_up_column(top: i32) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for y in 0..=top {
            world.set_block((0, y, 0), bubble_column(false));
        }
        world
    }

    #[test]
    fn arrows_hurting_projectiles_and_shulker_bullets_are_pushed_by_bubble_columns() {
        // BubbleColumnBlock.entityInside reaches what applies the blocks it is in: an arrow's
        // checkBlockCollision at the end of its flight from 1.13 (1.13.2
        // AbstractArrowEntity.java:302-303, 1.21 PersistentProjectileEntity.java:276), its steps'
        // effects from 1.21.2 (1.21.5 AbstractArrow.java:258, 26.3 :268); a hurting projectile's
        // and a shulker bullet's from 1.16.2, before the move until 1.21.1 and after it from 1.21.2
        // (1.16.1 has none; 1.16.2 AbstractHurtingProjectile.java:74, ShulkerBullet.java:227;
        // 1.21.5 AbstractHurtingProjectile.java:79, ShulkerBullet.java:219; 26.3
        // AbstractHurtingProjectile.java:80, ShulkerBullet.java:208). With a column above, the
        // inside push of +0.06.
        let world = bubble_up_column(9);
        let push = |kind: ProjectileKind, version: MinecraftVersion| {
            // (no acceleration power, which from 1.21 would add 0.1 along the pushed motion)
            let mut state = ProjectileState {
                gravity: 0.0,
                air_drag: 1.0,
                acceleration_power: 0.0,
                ..thrown(kind, Vec3::new(0.5, 5.0, 0.5), Vec3::ZERO)
            };
            let _ = tick_projectile_in_release(&mut state, &world, version);
            (state.position.y, state.velocity.y)
        };
        for kind in [ProjectileKind::Arrow, ProjectileKind::Trident] {
            for version in [release(1, 13, 2), release(1, 21, 1), release(1, 21, 3), release(1, 21, 5), release(26, 3, 0)] {
                assert_eq!(push(kind, version), (5.0, 0.06), "{kind:?} {version}");
            }
        }
        for kind in [ProjectileKind::SmallFireball, ProjectileKind::ShulkerBullet] {
            assert_eq!(push(kind, release(1, 16, 1)), (5.0, 0.0), "{kind:?}");
            // the move takes the pushed motion before 1.21.2
            for version in [release(1, 16, 2), release(1, 21, 1)] {
                assert_eq!(push(kind, version), (5.06, 0.06), "{kind:?} {version}");
            }
            for version in [release(1, 21, 5), release(26, 3, 0)] {
                assert_eq!(push(kind, version), (5.0, 0.06), "{kind:?} {version}");
            }
        }
        // the noPhysics shulker bullet takes no blocks' effects in 1.21.2 - 1.21.4
        assert_eq!(push(ProjectileKind::SmallFireball, release(1, 21, 2)), (5.0, 0.06));
        for version in [release(1, 21, 2), release(1, 21, 4)] {
            assert_eq!(push(ProjectileKind::ShulkerBullet, version), (5.0, 0.0), "{version}");
        }
    }

    #[test]
    fn an_arrow_that_hits_soul_sand_before_1_14_is_slowed_by_it_before_its_move() {
        // the block hit runs the hit block's onEntityCollision on the arrow before the tick's move
        // (1.8.9 AbstractArrowEntity.java:306-308, 1.12.2 :381-383, 1.13.2 :335-337): soul sand
        // makes the hit's 2.5 a move of 1.0, which ends 0.05 short (pj_arrow_over_soul_sand: x 4.45
        // on 1.8.8, 1.12.2 and 1.13.2); from 1.14 the ray passes over its collision box
        let mut world = WorldSnapshot::new();
        let full = BlockShape { effect: BlockEffect::SoulSand, ..BlockShape::full_block() };
        world.set_block(
            (6, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0)),
                    ..full
                }),
                outline_shape: Some(full),
                ..Default::default()
            },
        );
        for version in [release(1, 8, 8), release(1, 12, 2), release(1, 13, 2)] {
            let mut state = ProjectileState {
                gravity: 0.0,
                air_drag: 1.0,
                ..thrown(ProjectileKind::Arrow, Vec3::new(3.5, 0.9, 0.5), Vec3::new(2.97, 0.0, 0.0))
            };
            let result = tick_projectile_in_release(&mut state, &world, version);
            assert!(result.block_hit, "{version}");
            assert!((state.position.x - 4.45).abs() < 1e-6, "{version} {}", state.position.x);
            assert!((state.velocity.x - 1.0).abs() < 1e-6, "{version} {}", state.velocity.x);
        }
    }

    #[test]
    fn a_projectile_beside_a_honey_block_slides_down_it_from_1_15() {
        // HoneyBlock.entityInside slides any entity down its side, not only a living one (1.15
        // HoneyBlock.java:68-75, 26.3 :55-62, 73-97): an arrow's checkBlockCollision reaches it, and
        // from 1.21.2 its applyEffectsFromBlocks; 1.14.4 has no honey
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.0625, 0.0, 0.0625), Vec3::new(0.9375, 0.9375, 0.9375)),
                    effect: BlockEffect::Honey,
                    ..BlockShape::full_block()
                }),
                ..Default::default()
            },
        );
        let slide = |version: MinecraftVersion| {
            let mut state = ProjectileState {
                gravity: 0.0,
                air_drag: 1.0,
                ..thrown(ProjectileKind::Arrow, Vec3::new(0.78, 0.2, 0.5), Vec3::new(0.0, -0.5, 0.0))
            };
            let _ = tick_projectile_in_release(&mut state, &world, version);
            state.velocity.y
        };
        assert_eq!(slide(release(1, 14, 4)), -0.5);
        assert_eq!(slide(release(1, 15, 2)), -0.05);
        assert_eq!(slide(release(1, 21, 1)), -0.05);
        let modern = (-0.05 - 0.08) * f64::from(0.98_f32);
        assert_eq!(slide(release(1, 21, 2)), modern);
        assert_eq!(slide(release(26, 3, 0)), modern);
    }

    #[test]
    fn a_cobweb_slows_the_next_move_of_a_hook_or_rocket() {
        // WebBlock.entityInside makes any entity stuck, 0.25, 0.05F, 0.25, which its next
        // Entity.move multiplies its movement by and clears (1.12.2 Entity.java move isInWeb;
        // 26.3 WebBlock.java, Entity.java:783-789); a hook and a rocket move by Entity.move
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 5, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Web,
                    ..BlockShape::full_block()
                }),
                ..Default::default()
            },
        );
        for kind in [ProjectileKind::FishingHook, ProjectileKind::Firework] {
            for version in [release(1, 12, 2), release(1, 21, 1), release(26, 3, 0)] {
                let mut state = thrown(kind, Vec3::new(0.3, 5.2, 0.5), Vec3::new(0.2, 0.0, 0.0));
                super::move_projectile(&mut state, &world, version, false);
                assert_eq!(state.position.x, 0.5, "{kind:?} {version}");
                assert!(state.stuck_speed_multiplier.x > 0.0, "{kind:?} {version}");
                state.velocity = Vec3::new(0.2, 0.0, 0.0);
                super::move_projectile(&mut state, &world, version, false);
                assert!((state.position.x - (0.5 + 0.2 * 0.25)).abs() < 1e-12, "{kind:?} {version} {}", state.position.x);
            }
        }
    }

    #[test]
    fn a_fishing_hook_in_a_bubble_column_is_pushed_by_it() {
        // a hook moves by Entity.move, whose checkInsideBlocks reaches the column until 1.21.1,
        // and from 1.21.2 by the tick's applyEffectsFromBlocks after it (26.3
        // FishingHook.java:230-231); with a column above, the inside push of +0.06
        let world = bubble_up_column(9);
        for version in [release(1, 13, 2), release(1, 21, 1), release(1, 21, 5), release(26, 3, 0)] {
            let mut hook = ProjectileState {
                ..thrown(ProjectileKind::FishingHook, Vec3::new(0.5, 5.0, 0.5), Vec3::ZERO)
            };
            super::move_projectile(&mut hook, &world, version, false);
            assert_eq!(hook.velocity.y, 0.06, "{version}");
        }
    }

    #[test]
    fn soul_sand_slows_an_arrow_that_ends_its_flight_in_it_until_1_14_4() {
        // the arrow's checkBlockCollision reaches SoulSandBlock's entity slowing, x and z times
        // 0.4, until 1.14.4 (1.8.9 AbstractArrowEntity.java:381, SoulSandBlock.java:25-26; 1.14
        // SoulSandBlock.java:26); 1.15 slows by the speed factor of a move an arrow does not make
        let mut world = WorldSnapshot::new();
        world.set_block(
            (6, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0)),
                    effect: BlockEffect::SoulSand,
                    ..BlockShape::full_block()
                }),
                ..Default::default()
            },
        );
        for (version, slowed) in [
            (release(1, 8, 8), true),
            (release(1, 12, 2), true),
            (release(1, 14, 4), true),
            (release(1, 15, 2), false),
            (release(26, 3, 0), false),
        ] {
            let mut state = ProjectileState {
                gravity: 0.0,
                air_drag: 1.0,
                ..thrown(ProjectileKind::Arrow, Vec3::new(3.5, 0.9, 0.5), Vec3::new(3.0, 0.0, 0.0))
            };
            let _ = tick_projectile_in_release(&mut state, &world, version);
            assert_eq!(state.position.x, 6.5, "{version}");
            assert_eq!(state.velocity.x, if slowed { 3.0 * 0.4 } else { 3.0 }, "{version}");
        }
    }

    #[test]
    fn an_arrow_in_the_ground_takes_fewer_bubble_column_pushes_from_1_21_4() {
        // the in-ground branch applies the blocks from 1.21.2 (1.21.3
        // PersistentProjectileEntity.java:200-202; none in 1.21 :187-196), and AbstractArrow skips
        // the inside push in the ground from 1.21.4 and the surface push from 1.21.5 (1.21.4
        // AbstractArrow.java:352-356, 1.21.5 :362-372, 26.3 :379-389)
        let in_ground = |top: i32, version: MinecraftVersion| {
            let mut arrow = ArrowState {
                gravity: 0.0,
                in_ground: true,
                last_block_id: Some(13),
                ..ArrowState::new(Vec3::new(0.5, 5.0, 0.5), Vec3::ZERO)
            };
            let _ = tick_arrow_in_release(&mut arrow, &bubble_up_column(top), version);
            assert!(arrow.in_ground, "{version}");
            arrow.velocity.y
        };
        for (version, inside, surface) in [
            (release(1, 21, 1), 0.0, 0.0),
            (release(1, 21, 2), 0.06, 0.1),
            (release(1, 21, 3), 0.06, 0.1),
            (release(1, 21, 4), 0.0, 0.1),
            (release(1, 21, 5), 0.0, 0.0),
            (release(26, 3, 0), 0.0, 0.0),
        ] {
            assert_eq!(in_ground(9, version), inside, "{version}");
            assert_eq!(in_ground(5, version), surface, "{version}");
        }
    }

    #[test]
    fn ender_pearl_bubble_columns_stay_clamped_from_1_21_5() {
        // 1.21.5 ThrownEnderpearl.java:246-252 overrides Projectile's unclamped pushes with
        // Entity.handleOnInsideBubbleColumn's max(-0.3, v - 0.03) (1.21.5 Entity.java:2563-2574;
        // 26.3 ThrownEnderpearl.java:244-250)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(true));
        world.set_block((0, 1, 0), bubble_column(true));
        for version in [release(1, 21, 5), release(26, 3, 0)] {
            let mut pearl = ProjectileState {
                kind: ProjectileKind::EnderPearl,
                gravity: 0.0,
                air_drag: 1.0,
                ..ProjectileState::new(Vec3::new(0.5, 0.1, 0.5), Vec3::new(0.0, -1.5, 0.0))
            };
            super::handle_first_tick_bubble_column(&mut pearl, &world, version);
            assert_eq!(pearl.velocity.y, -0.3);

            let mut snowball = ProjectileState {
                kind: ProjectileKind::Snowball,
                ..pearl
            };
            snowball.velocity.y = -1.5;
            super::handle_first_tick_bubble_column(&mut snowball, &world, version);
            assert_eq!(snowball.velocity.y, -1.5 + -0.03);
        }
    }

    #[test]
    fn first_tick_bubble_columns_run_y_outer_from_26_3() {
        // 26.3 ThrowableProjectile.java:85-87: Level.findBlocksIn, in a chunk section y outer, then
        // z, x fastest (BlockScanUtils.java:83-99); before, BlockPos.betweenClosed, z outer (26.2
        // ThrowableProjectile.java). The unclamped double additions round differently
        let mut world = WorldSnapshot::new();
        for z in 0..=1 {
            world.set_block((0, 0, z), bubble_column(false));
            world.set_block((0, 1, z), bubble_column(false));
        }
        let start = ProjectileState {
            gravity: 0.0,
            air_drag: 1.0,
            ..ProjectileState::new(Vec3::new(0.5, 0.9, 0.9), Vec3::new(0.0, -1.0, 0.0))
        };
        let mut modern = start;
        super::handle_first_tick_bubble_column(&mut modern, &world, release(26, 3, 0));
        assert_eq!(modern.velocity.y, -1.0 + 0.06 + 0.06 + 0.1 + 0.1);

        let mut before = start;
        super::handle_first_tick_bubble_column(&mut before, &world, release(26, 2, 0));
        assert_eq!(before.velocity.y, -1.0 + 0.06 + 0.1 + 0.06 + 0.1);
        assert_ne!(modern.velocity.y, before.velocity.y);
    }

    #[test]
    fn hurting_projectile_before_1_21_moves_then_adds_the_power_vector() {
        // 1.20.4 AbstractHurtingProjectile.java:85-110: move by the old motion, then
        // (v + power) * 0.95F
        let mut state = ProjectileState {
            power: Vec3::new(0.1, 0.0, 0.0),
            ..thrown(
                ProjectileKind::Fireball,
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
            )
        };

        let result =
            tick_projectile_in_release(&mut state, &WorldSnapshot::new(), release(1, 20, 4));

        assert_eq!(result.position, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(result.velocity.x, (1.0 + 0.1) * f64::from(0.95_f32));
    }

    #[test]
    fn hurting_projectile_from_1_21_pushes_along_its_direction_by_acceleration_power() {
        // 1.21 AbstractHurtingProjectile.java:99: v + normalize(v) * accelerationPower (0.1),
        // after the move until 1.21.1; 1.21.2 AbstractHurtingProjectile.java:69-113 first
        let velocity = Vec3::new(3.0, 4.0, 0.0);
        let mut legacy = thrown(ProjectileKind::SmallFireball, Vec3::ZERO, velocity);
        let legacy_result =
            tick_projectile_in_release(&mut legacy, &WorldSnapshot::new(), release(1, 21, 1));
        let expected = Vec3::new(
            (3.0 + 3.0 / 5.0 * 0.1) * f64::from(0.95_f32),
            (4.0 + 4.0 / 5.0 * 0.1) * f64::from(0.95_f32),
            0.0,
        );
        assert_eq!(legacy_result.position, velocity);
        assert_eq!(legacy_result.velocity, expected);

        let mut modern = thrown(ProjectileKind::SmallFireball, Vec3::ZERO, velocity);
        let modern_result =
            tick_projectile_in_release(&mut modern, &WorldSnapshot::new(), release(1, 21, 2));
        assert_eq!(modern_result.velocity, expected);
        assert_eq!(modern_result.position, expected);

        let mut in_water = ProjectileState {
            in_water: true,
            ..thrown(
                ProjectileKind::WitherSkull,
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
            )
        };
        let _ = tick_projectile_in_release(&mut in_water, &WorldSnapshot::new(), release(26, 2, 0));
        assert_eq!(in_water.velocity.x, (1.0 + 0.1) * f64::from(0.8_f32));
    }

    #[test]
    fn llama_spit_moves_by_the_old_motion_and_is_removed_in_water() {
        // 1.21.11 LlamaSpit.java:32-48
        let mut state = thrown(
            ProjectileKind::LlamaSpit,
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(1.0, 0.0, 0.0),
        );
        let result =
            tick_projectile_in_release(&mut state, &WorldSnapshot::new(), release(1, 21, 11));
        assert_eq!(result.position, Vec3::new(1.5, 0.5, 0.5));
        assert_eq!(result.velocity, Vec3::new(f64::from(0.99_f32), -0.06, 0.0));
        assert!(!result.discarded);

        // before 1.20.5 the gravity is 0.06F (1.20.4 LlamaSpit.java)
        assert_eq!(
            ProjectileKind::LlamaSpit.default_gravity_in(release(1, 20, 4)),
            f64::from(0.06_f32)
        );

        let mut wet = ProjectileState {
            in_water: true,
            ..thrown(
                ProjectileKind::LlamaSpit,
                Vec3::new(0.5, 0.5, 0.5),
                Vec3::new(1.0, 0.0, 0.0),
            )
        };
        let removed =
            tick_projectile_in_release(&mut wet, &WorldSnapshot::new(), release(1, 21, 11));
        assert!(removed.discarded);
        assert_eq!(removed.position, Vec3::new(0.5, 0.5, 0.5));
    }

    #[test]
    fn shulker_bullet_moves_without_gravity_on_the_client() {
        // 1.21.11 ShulkerBullet.java: gravity and steering are server-side; setPos(pos + v)
        let mut state = thrown(
            ProjectileKind::ShulkerBullet,
            Vec3::ZERO,
            Vec3::new(0.2, 0.1, 0.0),
        );
        let result = tick_projectile(&mut state, &WorldSnapshot::new());
        assert_eq!(result.position, Vec3::new(0.2, 0.1, 0.0));
        assert_eq!(result.velocity, Vec3::new(0.2, 0.1, 0.0));
    }

    #[test]
    fn firework_rocket_slides_along_blocks_and_keeps_its_motion_from_1_16() {
        // 1.21.11 FireworkRocketEntity.java:133-143: move(SELF), then the motion is put back;
        // before 1.16 Entity.move's stop on the collided axis stays (1.13.2
        // FireworkRocketEntity.java)
        let world = full_block_world((1, 0, 0));
        let start = Vec3::new(0.5, 0.25, 0.5);
        let mut modern = thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0));
        let result = tick_projectile_in_release(&mut modern, &world, release(1, 21, 11));
        assert_eq!(result.position.x, 1.0 - 0.125);
        assert_eq!(result.position.y, 0.25 + 0.04);
        assert!(result.horizontal_collision);
        assert_eq!(result.velocity, Vec3::new(1.15, 0.04, 0.0));

        // the next tick's boost has no horizontal factor along the collision (1.16.4+)
        let next = tick_projectile_in_release(&mut modern, &world, release(1, 21, 11));
        assert_eq!(next.velocity.x, 1.15);
        // 1.16 FireworkRocketEntity.java:29-30 boosts by 1.15 along a collision too
        let mut collided = ProjectileState {
            horizontal_collision: true,
            ..thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0))
        };
        let early =
            tick_projectile_in_release(&mut collided, &WorldSnapshot::new(), release(1, 16, 0));
        assert_eq!(early.velocity.x, 1.15);
        // and so up to 1.16.3 (FireworkRocketEntity.java:155); 1.16.4 :155-156 keeps 1.0 along it
        for (patch, factor) in [(2, 1.15), (3, 1.15), (4, 1.0), (5, 1.0)] {
            let mut collided = ProjectileState {
                horizontal_collision: true,
                ..thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0))
            };
            let result = tick_projectile_in_release(
                &mut collided,
                &WorldSnapshot::new(),
                release(1, 16, patch),
            );
            assert_eq!(result.velocity.x, factor, "1.16.{patch}");
        }

        let mut legacy = thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0));
        let legacy_result = tick_projectile_in_release(&mut legacy, &world, release(1, 13, 2));
        assert_eq!(legacy_result.position.x, 1.0 - 0.125);
        assert_eq!(legacy_result.velocity.x, 0.0);
        assert_eq!(legacy_result.velocity.y, 0.04);

        // in water a rocket keeps its speed: no drag nor water inertia
        let mut wet = ProjectileState {
            in_water: true,
            ..thrown(ProjectileKind::Firework, start, Vec3::new(0.0, 0.5, 0.0))
        };
        let wet_result = tick_projectile(&mut wet, &WorldSnapshot::new());
        assert_eq!(wet_result.velocity.y, 0.5 + 0.04);
    }

    #[test]
    fn a_no_physics_rocket_passes_through_and_hits_nothing() {
        // the same hit test, its result unused (1.14.4 FireworkRocketEntity.java:132-136, 1.20.4
        // FireworkRocketEntity.java:144-146, 1.21.3 :139-145, 26.3 :146-152), and Entity.move's
        // noPhysics branch (1.13.2 Entity.java:565-566, 1.16.5 :607-609, 1.21.8 :706-707)
        let world = full_block_world((1, 0, 0));
        // 1.14 tests from where the move left it with the motion that left, so it starts further
        // back: a stop on the wall would leave no motion to test along
        for (version, x, hits) in [
            ("1.13.2", 0.5, false),
            ("1.14", -0.6, true),
            ("1.20.4", 0.5, true),
            ("1.21.1", 0.5, true),
            ("1.21.2", 0.5, true),
            ("26.3", 0.5, true),
        ] {
            let release = MinecraftVersion::parse(version).unwrap();
            let boosted = Vec3::new(1.15, 0.04, 0.0);
            let start = Vec3::new(x, 0.25, 0.5);
            let mut solid = thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0));
            let blocked = tick_projectile_in_release(&mut solid, &world, release);
            assert_eq!(blocked.block_hit, hits, "{version}");

            let mut ghost = ProjectileState {
                no_physics: true,
                ..thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0))
            };
            let passed = tick_projectile_in_release(&mut ghost, &world, release);
            assert!(!passed.block_hit, "{version}");
            assert!(!passed.horizontal_collision, "{version}");
            assert_eq!(passed.position.x, start.x + boosted.x, "{version}");
            assert_eq!(passed.position.y, start.y + boosted.y, "{version}");
            assert_eq!(passed.velocity, boosted, "{version}");
        }

        // the collision flag of an earlier move outlives a noPhysics move until 1.21.9, and the
        // ground in every release (1.21.8 Entity.java:706-707, 1.21.9 :713-718, 26.3 :766-772)
        let start = Vec3::new(0.5, 0.25, 0.5);
        for (version, kept) in [("1.21.8", true), ("1.21.9", false), ("26.3", false)] {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut ghost = ProjectileState {
                no_physics: true,
                horizontal_collision: true,
                on_ground: true,
                ..thrown(ProjectileKind::Firework, start, Vec3::new(1.0, 0.0, 0.0))
            };
            let result = tick_projectile_in_release(&mut ghost, &world, release);
            assert_eq!(result.horizontal_collision, kept, "{version}");
            assert_eq!(ghost.horizontal_collision, kept, "{version}");
            assert!(result.on_ground, "{version}");
            // its own boost read the old flag, as the tick's first step (26.3 :141)
            assert_eq!(result.velocity.x, 1.0, "{version}");
        }
    }

    #[test]
    fn fishing_hook_falls_moves_then_drags() {
        // 1.21.11 FishingHook.java:225-238: -0.03 off the ground out of water, move(SELF),
        // then 0.92
        let mut state = thrown(
            ProjectileKind::FishingHook,
            Vec3::new(0.5, 5.0, 0.5),
            Vec3::new(0.1, 0.0, 0.0),
        );
        let result =
            tick_projectile_in_release(&mut state, &WorldSnapshot::new(), release(1, 21, 11));
        assert_eq!(result.position, Vec3::new(0.6, 5.0 - 0.03, 0.5));
        assert_eq!(result.velocity, Vec3::new(0.1 * 0.92, -0.03 * 0.92, 0.0));
    }

    #[test]
    fn fishing_hook_skips_gravity_on_the_ground_from_1_21_6() {
        // 1.21.5 FishingHook.java:218 tests the water alone; 1.21.6 :228 adds !onGround (1.16
        // FishingHook.java:233 the same as 1.21.5). A hook whose onGround is still set
        // with nothing under it falls by -0.03 until 1.21.5
        let start = ProjectileState {
            on_ground: true,
            ..thrown(
                ProjectileKind::FishingHook,
                Vec3::new(0.5, 5.0, 0.5),
                Vec3::ZERO,
            )
        };
        for version in [release(1, 16, 0), release(1, 21, 5)] {
            let mut state = start;
            let result = tick_projectile_in_release(&mut state, &WorldSnapshot::new(), version);
            assert_eq!(result.position.y, 5.0 - 0.03);
            assert_eq!(result.velocity.y, -0.03 * 0.92);
            assert!(!state.on_ground);
        }

        // 1.21.6: no gravity, a move with no vertical part keeps onGround on the client (1.21.6
        // Entity.java:746, isLocalInstanceAuthoritative is false for a hook) and the hook rests
        let mut state = start;
        for _ in 0..2 {
            let result =
                tick_projectile_in_release(&mut state, &WorldSnapshot::new(), release(1, 21, 6));
            assert_eq!(result.position.y, 5.0);
            assert_eq!(result.velocity, Vec3::ZERO);
            assert!(state.on_ground);
        }
    }

    #[test]
    fn bobbing_hook_pulls_to_the_water_own_height_in_1_13() {
        // 1.13.2 FishingHook.java:159-161: FluidState.getHeight, level / 9
        // (FlowingFluid.java:433), with no look above; 1.14.4 FishingHook.java:140
        // getHeight(world, pos) is 1 under water
        let mut world = WorldSnapshot::new();
        for y in 0..=1 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
        }
        let pull = |height: f64| -(0.5 - height) * f64::from(1.0_f32) * 0.2;
        for (version, height) in [
            (release(1, 13, 2), f64::from(8.0_f32 / 9.0)),
            (release(1, 14, 4), 1.0),
        ] {
            let mut state = ProjectileState {
                bobbing: true,
                bob_random: 1.0,
                ..thrown(
                    ProjectileKind::FishingHook,
                    Vec3::new(0.5, 0.5, 0.5),
                    Vec3::ZERO,
                )
            };
            let result = tick_projectile_in_release(&mut state, &world, version);
            assert_eq!(result.velocity.y, pull(height) * state.air_drag);
        }
    }

    #[test]
    fn llama_spit_moves_before_the_removal_only_until_1_14() {
        // 1.14.4 LlamaSpit.java:62 assigns this.x before the removal; 1.15 :78-80 reads the
        // target and setPosition runs only when the spit stays (:115)
        for (version, x) in [(release(1, 14, 4), 1.5), (release(1, 15, 0), 0.5)] {
            let mut wet = ProjectileState {
                in_water: true,
                ..thrown(
                    ProjectileKind::LlamaSpit,
                    Vec3::new(0.5, 0.5, 0.5),
                    Vec3::new(1.0, 0.0, 0.0),
                )
            };
            let result = tick_projectile_in_release(&mut wet, &WorldSnapshot::new(), version);
            assert!(result.discarded);
            assert_eq!(result.position.x, x);
        }
    }

    #[test]
    fn llama_spit_in_a_bubble_column_is_removed_from_1_13_until_1_21_4() {
        // 1.20.5 LlamaSpit.java:45 isInsideWaterOrBubbleColumn (Entity.java:1209-1223, the
        // block at the position; 1.13.2 :121 method_15575); 1.21.5 LlamaSpit.java:48 isInWater
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(false));
        for (version, removed) in [
            (release(1, 12, 2), false),
            (release(1, 13, 2), true),
            (release(1, 21, 4), true),
            (release(1, 21, 5), false),
        ] {
            let mut state = thrown(
                ProjectileKind::LlamaSpit,
                Vec3::new(0.5, 0.9, 0.5),
                Vec3::new(0.1, 0.0, 0.0),
            );
            let result = tick_projectile_in_release(&mut state, &world, version);
            assert_eq!(result.discarded, removed);
        }
    }

    #[test]
    fn fishing_hook_stops_at_a_block_hit_by_its_squared_distance() {
        // 1.21.11 FishingHook.java:274-277: v = normalize(v) * hit.distanceTo (squared)
        let world = full_block_world((1, 0, 0));
        let mut state = thrown(
            ProjectileKind::FishingHook,
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(2.0, 0.0, 0.0),
        );
        let result = tick_projectile_in_release(&mut state, &world, release(1, 21, 11));
        assert!(result.block_hit);
        assert_eq!(result.position.x, 0.5 + 0.25);
        assert_eq!(result.velocity, Vec3::new(0.25 * 0.92, -0.03 * 0.92, 0.0));
    }

    #[test]
    fn fishing_hook_reaching_water_slows_and_starts_bobbing() {
        // 1.21.11 FishingHook.java:178-182 (1.12.2 FishingHook.java:31-35)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        for version in [release(1, 11, 2), release(1, 21, 11)] {
            let mut state = thrown(
                ProjectileKind::FishingHook,
                Vec3::new(0.5, 0.5, 0.5),
                Vec3::new(1.0, -1.0, 1.0),
            );
            let result = tick_projectile_in_release(&mut state, &world, version);
            assert!(result.bobbing);
            assert_eq!(result.position, Vec3::new(0.5, 0.5, 0.5));
            assert_eq!(result.velocity, Vec3::new(0.3, -0.2, 0.3));
        }
    }

    fn arrow(position: Vec3, velocity: Vec3, release: MinecraftVersion) -> ArrowState {
        ArrowState::new_in_release(position, velocity, release)
    }

    #[test]
    fn arrow_hit_before_1_21_2_pulls_back_along_the_offset_then_moves_drags_and_falls() {
        // 1.21 AbstractArrow.java:454-475 (onBlockHit): v = hit - pos, pos -=
        // normalize(v) * 0.05F; then the tick moves by v, scales by 0.99F and takes the gravity
        // (:252-279), also on the hit tick
        let world = full_block_world((1, 0, 0));
        let mut state = arrow(
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(2.0, 0.0, 0.0),
            release(1, 21, 1),
        );
        let result = tick_arrow_in_release(&mut state, &world, release(1, 21, 1));

        assert!(result.block_hit);
        assert!(result.in_ground);
        assert_eq!(result.position.x, (0.5 - 0.05000000074505806) + 0.5);
        assert_eq!(result.velocity, Vec3::new(0.5 * f64::from(0.99_f32), -0.05, 0.0));

        // the in-ground ticks keep that motion (1.21 :165-179 leaves it alone)
        let resting = tick_arrow_in_release(&mut state, &world, release(1, 21, 1));
        assert!(resting.in_ground);
        assert_eq!(resting.velocity, result.velocity);
        assert_eq!(resting.position, result.position);
    }

    #[test]
    fn arrow_hit_until_1_13_2_casts_the_offset_to_float() {
        let world = full_block_world((1, 0, 0));
        let offset = f64::from(0.7_f32);
        let length = (offset * offset).sqrt() as f32;

        // 1.13.2 AbstractArrow.java:324-330: v = (float)(hit - pos), pos -= v /
        // (Mth.sqrt(v.v) * 20.0F)
        let mut modern = arrow(Vec3::new(0.3, 0.5, 0.5), Vec3::new(2.0, 0.0, 0.0), release(1, 13, 2));
        let result = tick_arrow_in_release(&mut modern, &world, release(1, 13, 2));
        assert!(result.block_hit);
        assert_eq!(result.position.x, (0.3 - offset / f64::from(length * 20.0_f32)) + offset);
        assert_eq!(result.velocity.x, offset * f64::from(0.99_f32));
        // gravity 0.05F before 1.20.5 (1.13.2 :297)
        assert_eq!(result.velocity.y, -f64::from(0.05_f32));

        // 1.12.2 AbstractArrow.java:370-376 (1.8.8 AbstractArrow.java:295-298): pos -= v /
        // 1.8.8 Mth.sqrt(v.v) * 0.05F
        let mut legacy = arrow(Vec3::new(0.3, 0.5, 0.5), Vec3::new(2.0, 0.0, 0.0), release(1, 12, 2));
        let result = tick_arrow_in_release(&mut legacy, &world, release(1, 12, 2));
        assert!(result.block_hit);
        assert_eq!(
            result.position.x,
            (0.3 - offset / f64::from(length) * f64::from(0.05_f32)) + offset
        );
    }

    #[test]
    fn arrow_gravity_is_0_05f_before_1_20_5() {
        // 1.20.4 AbstractArrow.java:258: y - 0.05F; 1.20.5 :266-267: 0.05
        let mut old = arrow(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), release(1, 20, 4));
        let result = tick_arrow_in_release(&mut old, &WorldSnapshot::new(), release(1, 20, 4));
        assert_eq!(result.velocity.y, -0.05000000074505806);
        let mut new = arrow(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), release(1, 20, 5));
        let result = tick_arrow_in_release(&mut new, &WorldSnapshot::new(), release(1, 20, 5));
        assert_eq!(result.velocity.y, -0.05);
    }

    #[test]
    fn arrow_in_water_before_1_21_2_moves_then_takes_the_water_inertia() {
        // 1.21 AbstractArrow.java:263-279: the move by v, then v * 0.6F, then gravity
        let mut state = arrow(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0), release(1, 21, 1));
        state.in_water = true;
        let result = tick_arrow_in_release(&mut state, &WorldSnapshot::new(), release(1, 21, 1));
        assert_eq!(result.position, Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(result.velocity.x, f64::from(0.6_f32));
        assert_eq!(result.velocity.y, f64::from(0.6_f32) - 0.05);
    }

    fn arrow_in_ground(position: Vec3, block: (i32, i32, i32), block_id: u32) -> ArrowState {
        ArrowState {
            in_ground: true,
            last_block_id: Some(block_id),
            last_block_position: Some(block),
            fall_impulse: Vec3::new(0.5, 0.5, 0.5),
            ..ArrowState::new(position, Vec3::new(0.1, 0.0, 0.0))
        }
    }

    fn solid(id: u32) -> BlockState {
        BlockState {
            id,
            shape: Some(BlockShape::full_block()),
            ..Default::default()
        }
    }

    #[test]
    fn arrow_fall_test_follows_the_release() {
        let resting = Vec3::new(0.95, 0.5, 0.5);
        let mut replaced = WorldSnapshot::new();
        replaced.set_block((1, 0, 0), solid(12));

        // 1.9.4 AbstractArrow.java:175-188 (1.8.8 AbstractArrow.java:171-185, 1.10.2): any
        // change of the hit block's state drops the arrow
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(tick_arrow_in_release(&mut state, &replaced, release(1, 10, 2)).started_falling);

        // 1.12.2 AbstractArrow.java:179 (1.11.2, 1.13.2 :222 with the same 0.05): a
        // replacement whose shape meets the box inflated by 0.05 keeps it
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &replaced, release(1, 11, 2)).started_falling);
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &replaced, release(1, 13, 2)).started_falling);

        // until 1.13.2 the state is read at the hit block, from 1.14 at the arrow's own block
        // (1.13.2 :195-196, 1.14.4 AbstractArrow.java:173-174): the unchanged hit block keeps
        // a 1.13.2 arrow whatever its own cell holds
        let mut unchanged = WorldSnapshot::new();
        unchanged.set_block((1, 0, 0), solid(11));
        let mut state = arrow_in_ground(Vec3::new(5.5, 5.5, 5.5), (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &unchanged, release(1, 13, 2)).started_falling);
        let mut state = arrow_in_ground(Vec3::new(5.5, 5.5, 5.5), (1, 0, 0), 11);
        assert!(tick_arrow_in_release(&mut state, &unchanged, release(1, 14, 4)).started_falling);

        // 1.15 ProjectileEntity tick: the 0.5 box inflated by 0.06 reaches a block above the
        // arrow; from 1.16 (AbstractArrow.java:244-246) only the point is tested
        let mut above = WorldSnapshot::new();
        above.set_block((0, 1, 0), solid(5));
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &above, release(1, 15, 2)).started_falling);
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        let result = tick_arrow_in_release(&mut state, &above, release(1, 16, 0));
        assert!(result.started_falling);
        assert_eq!(result.velocity, Vec3::new(0.05, 0.0, 0.0));
    }

    #[test]
    fn trident_before_1_21_2_keeps_the_offset_motion_after_a_hit() {
        // 1.20.4 AbstractArrow.java:420-427 and :244-257 with TridentEntity's
        // 0.99F water inertia (ThrownTrident.java:201)
        let world = full_block_world((1, 0, 0));
        let mut state = TridentState::new_in_release(
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(2.0, 0.0, 0.0),
            release(1, 20, 4),
        );
        state.in_water = true;
        let result = tick_trident_in_release(&mut state, &world, release(1, 20, 4));
        assert!(result.block_hit);
        assert_eq!(result.position.x, (0.5 - 0.05000000074505806) + 0.5);
        assert_eq!(
            result.velocity,
            Vec3::new(0.5 * f64::from(0.99_f32), -0.05000000074505806, 0.0)
        );
    }

    fn snowball(position: Vec3, velocity: Vec3) -> ProjectileState {
        ProjectileState {
            gravity: 0.0,
            air_drag: 1.0,
            first_tick: false,
            ..ProjectileState::for_kind(ProjectileKind::Snowball, position, velocity)
        }
    }

    #[test]
    fn clip_starting_inside_a_shape_hits_a_thousandth_along_the_ray_from_1_13() {
        // 26.2 VoxelShape.java:119-131 (1.13.2 VoxelShape.java:150-165): from + 0.001 * d inside
        // the shape is the hit
        let world = full_block_world((1, 0, 0));
        for version in [release(1, 13, 2), release(26, 2, 0)] {
            assert_eq!(
                super::nearest_block_hit(&world, Vec3::new(1.5, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0), version),
                Some(Vec3::new(1.5 + 0.001, 0.5, 0.5))
            );
            let mut state = snowball(Vec3::new(1.5, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0));
            let result = tick_projectile_in_release(&mut state, &world, version);
            assert!(result.block_hit);
            // Before 1.21.2 the throwable still moves its full velocity after onCollision
            // (1.13.2 ThrowableProjectile.java:176-186); a clip location is not its final position.
            let x = if version.at_least(1, 21, 2) { 1.5 + 0.001 } else { 2.5 };
            assert_eq!(result.position, Vec3::new(x, 0.5, 0.5));
        }
        // Before 1.13 vanilla tests all six faces (1.12.2 AABB.java:290-323).
        // The registered 1.8.8 path uses the supplied 1.8.9 source as a stand-in.
        for version in [
            release(1, 8, 8), release(1, 9, 4), release(1, 10, 2),
            release(1, 11, 2), release(1, 12, 2),
        ] {
            assert_eq!(
                super::nearest_block_hit(&world, Vec3::new(1.5, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0), version),
                Some(Vec3::new(2.0, 0.5, 0.5)), "{version}"
            );
            let mut state = snowball(Vec3::new(1.5, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0));
            let result = tick_projectile_in_release(&mut state, &world, version);
            assert!(result.block_hit, "{version}");
            assert_eq!(result.position, Vec3::new(2.5, 0.5, 0.5), "{version}");
        }
    }

    #[test]
    fn clip_shorter_than_the_1e_7_square_hits_nothing_from_1_13() {
        // 26.2 VoxelShape.java:124-127: d.lengthSqr() < 1.0E-7 is no hit
        let world = full_block_world((1, 0, 0));
        let mut state = snowball(Vec3::new(0.9999, 0.5, 0.5), Vec3::new(3.0e-4, 0.0, 0.0));
        let result = tick_projectile_in_release(&mut state, &world, release(26, 2, 0));
        assert!(!result.block_hit);
        assert_eq!(result.position.x, 0.9999 + 3.0e-4);

        let mut state = snowball(Vec3::new(0.9999, 0.5, 0.5), Vec3::new(3.0e-4, 0.0, 0.0));
        let result = tick_projectile_in_release(&mut state, &world, release(1, 12, 2));
        // Legacy Vec3d's squared-axis cutoff also rejects this short axis.
        assert!(!result.block_hit);
    }

    #[test]
    fn legacy_clip_uses_per_axis_cutoff_instead_of_total_segment_length() {
        let world = full_block_world((0, 0, 0));
        let start = Vec3::new(0.5, 0.5, 0.5);
        let movement = Vec3::new(0.00025, 0.00025, 0.00025);
        let mut legacy = snowball(start, movement);
        let legacy_result = tick_projectile_in_release(&mut legacy, &world, release(1, 12, 2));
        assert!(!legacy_result.block_hit);
        assert_eq!(legacy_result.position, start + movement);
        let mut modern = snowball(start, movement);
        let modern_result = tick_projectile_in_release(&mut modern, &world, release(1, 13, 2));
        assert!(modern_result.block_hit);
        assert_eq!(modern_result.position, start + movement);
        assert_eq!(
            super::nearest_block_hit(&world, start, movement, release(1, 13, 2)),
            Some(start + movement.scale(0.001))
        );
    }

    #[test]
    fn legacy_clip_keeps_block_local_arithmetic_for_partial_shapes() {
        // Block.method_11559 subtracts the cell before clipping and adds it back.
        // Moving the decimal 0.6 face to world X first changes Y and Z's last bits.
        for origin in [-30_000_000, 30_000_000] {
            let mut world = WorldSnapshot::new();
            world.set_block(
                (origin, 0, 0),
                BlockState {
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::new(0.1, 0.1, 0.1), Vec3::new(0.6, 0.8, 0.8)),
                        ..BlockShape::full_block()
                    }),
                    ..Default::default()
                },
            );
            let x = f64::from(origin);
            let hit = super::clip_block(
                &world, (origin, 0, 0),
                Vec3::new(x + 0.2, 0.3, 0.4), Vec3::new(x + 1.2, 0.9, 0.8),
                release(1, 12, 2), super::ClipShape::Collider,
            );
            assert_eq!(hit.map(|hit| hit.0), Some(Vec3::new(x + 0.6, 0.5400000004470349, 0.5600000002980232)));
        }
    }

    #[test]
    fn legacy_clip_selects_the_nearest_of_supplied_partial_boxes() {
        let mut world = WorldSnapshot::new();
        let box_at = |min, max| BlockShape {
            bounds: Aabb::new(Vec3::new(min, 0.0, 0.0), Vec3::new(max, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        // Supply the farther box first: traversal must not return the first component.
        world.set_block_shapes((0, 0, 0), BlockState::default(), vec![box_at(0.75, 1.0), box_at(0.0, 0.25)]);
        let hit = super::clip_block(
            &world, (0, 0, 0), Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5),
            release(1, 12, 2), super::ClipShape::Collider,
        );
        assert_eq!(hit, Some((Vec3::new(0.0, 0.5, 0.5), crate::math::HitFace::West)));
    }

    #[test]
    fn clip_tests_only_the_cells_the_ray_passes() {
        // 26.2 BlockGetter.java:110-163: traverseBlocks visits the cells along the ray, so the
        // part of a fence's 1.5-high shape above its cell is not hit by a ray over the cell
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.375, 0.0, 0.375), Vec3::new(0.625, 1.5, 0.625)),
                    ..BlockShape::full_block()
                }),
                ..Default::default()
            },
        );
        let mut state = snowball(Vec3::new(0.5, 1.2, 0.5), Vec3::new(2.0, 0.0, 0.0));
        let result = tick_projectile_in_release(&mut state, &world, release(26, 2, 0));
        assert!(!result.block_hit);

        let mut state = snowball(Vec3::new(0.5, 0.8, 0.5), Vec3::new(2.0, 0.0, 0.0));
        let result = tick_projectile_in_release(&mut state, &world, release(26, 2, 0));
        assert!(result.block_hit);
        assert_eq!(result.position, Vec3::new(1.375, 0.8, 0.5));
    }

    #[test]
    fn wind_charge_keeps_its_motion_in_air_and_water() {
        // 1.21.11 AbstractWindCharge.java:34-45 (accelerationPower 0) and :127-133 (inertia 1.0F,
        // liquid inertia the same)
        let state = ProjectileState::for_kind(ProjectileKind::WindCharge, Vec3::ZERO, Vec3::ZERO);
        assert_eq!(state.acceleration_power, 0.0);
        assert_eq!(state.gravity, 0.0);
        assert_eq!(state.air_drag, 1.0);
        let mut state = ProjectileState {
            in_water: true,
            ..ProjectileState::for_kind(
                ProjectileKind::WindCharge,
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
            )
        };
        let result = tick_projectile_in_release(&mut state, &WorldSnapshot::new(), release(1, 21, 11));
        assert_eq!(result.velocity, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(result.position, Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn wind_charge_clips_outline_shapes_in_1_20_4() {
        // 1.20.4 WindCharge.java:135-138: RaycastContext.ShapeType.OUTLINE; COLLIDER from
        // 1.20.5 (AbstractHurtingProjectile.getClipType)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            BlockState {
                outline_shape: Some(BlockShape::full_block()),
                ..Default::default()
            },
        );
        let charge = || ProjectileState::for_kind(
            ProjectileKind::WindCharge,
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(2.0, 0.0, 0.0),
        );
        let mut state = charge();
        assert!(tick_projectile_in_release(&mut state, &world, release(1, 20, 4)).block_hit);
        let mut state = charge();
        assert!(!tick_projectile_in_release(&mut state, &world, release(1, 20, 5)).block_hit);
    }

    #[test]
    fn projectile_kind_resolves_the_registry_ids_of_every_release() {
        // 1.21.11 EntityType.java: fishing_bobber, splash_potion and lingering_potion (1.21.5+),
        // wind_charge and breeze_wind_charge; 1.12.2 EntityType.java:227-232 xp_bottle and
        // fireworks_rocket; 1.8.8 EntityType.java:242-248 ThrownPotion, ThrownExpBottle and
        // FireworksRocketEntity
        assert_eq!(ProjectileKind::from_name("minecraft:fishing_bobber"), ProjectileKind::FishingHook);
        assert_eq!(ProjectileKind::from_name("minecraft:splash_potion"), ProjectileKind::Potion);
        assert_eq!(ProjectileKind::from_name("minecraft:lingering_potion"), ProjectileKind::Potion);
        assert_eq!(ProjectileKind::from_name("minecraft:wind_charge"), ProjectileKind::WindCharge);
        assert_eq!(
            ProjectileKind::from_name("minecraft:breeze_wind_charge"),
            ProjectileKind::WindCharge
        );
        assert_eq!(ProjectileKind::from_name("minecraft:xp_bottle"), ProjectileKind::ExperienceBottle);
        assert_eq!(ProjectileKind::from_name("minecraft:fireworks_rocket"), ProjectileKind::Firework);
        assert_eq!(ProjectileKind::from_name("ThrownPotion"), ProjectileKind::Potion);
        assert_eq!(ProjectileKind::from_name("ThrownExpBottle"), ProjectileKind::ExperienceBottle);
        assert_eq!(ProjectileKind::from_name("FireworksRocketEntity"), ProjectileKind::Firework);
    }

    fn wall(position: (i32, i32, i32)) -> WorldSnapshot {
        full_block_world(position)
    }

    #[test]
    fn a_trident_stays_in_the_block_it_hit() {
        // ThrownTrident.tick calls AbstractArrow.tick, whose inGround holds it (1.13.2
        // ThrownTrident.java:57-92, 26.3 ThrownTrident.java:62-99, AbstractArrow.java:192-210, 539-547)
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let world = wall((1, 100, 0));
            let mut state =
                TridentState::new_in_release(Vec3::new(0.5, 100.5, 0.5), Vec3::new(2.0, 0.0, 0.0), release);
            let first = tick_trident_in_release(&mut state, &world, release);
            assert!(first.block_hit && first.in_ground, "{release}");
            let embedded = state.position;
            for tick in 1..=6 {
                let result = tick_trident_in_release(&mut state, &world, release);
                assert_eq!(result.position, embedded, "{release}");
                assert!(result.in_ground && !result.block_hit, "{release}");
                // dealtDamage after more than four ticks in the ground, read at the tick's top
                assert_eq!(result.dealt_damage, tick > 5, "{release}");
            }
            assert_eq!(state.in_ground_time, 6);
        }
    }

    #[test]
    fn an_embedded_trident_falls_when_its_block_goes() {
        let release = MinecraftVersion::new(1, 21, 4);
        let world = wall((1, 100, 0));
        let mut state =
            TridentState::new_in_release(Vec3::new(0.5, 100.5, 0.5), Vec3::new(2.0, 0.0, 0.0), release);
        let _ = tick_trident_in_release(&mut state, &world, release);
        state.last_block_id = Some(99);
        let result = tick_trident_in_release(&mut state, &WorldSnapshot::new(), release);
        assert!(result.started_falling && !result.in_ground);
    }

    #[test]
    fn an_arrow_without_its_state_flies_as_an_arrow() {
        // the same arrow by both ticks (1.21.4 AbstractArrow.java:164-245 against
        // ThrowableProjectile.java:42-57): (1, 97), not the throwable's (0.99, 100.9405)
        let release = MinecraftVersion::new(1, 21, 4);
        let world = WorldSnapshot::new();
        let (from, by) = (Vec3::new(0.0, 100.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let mut generic = ProjectileState::for_kind_in_release(ProjectileKind::Arrow, from, by, release);
        let mut arrow = ArrowState::new_in_release(from, by, release);
        let result = tick_projectile_in_release(&mut generic, &world, release);
        let _ = tick_arrow_in_release(&mut arrow, &world, release);
        assert_eq!(result.position, Vec3::new(1.0, 101.0, 0.0));
        assert_eq!(generic.position, arrow.position);
        assert_eq!(generic.velocity, arrow.velocity);
    }

    #[test]
    fn a_projectile_hits_the_world_border_from_1_21_2() {
        // ProjectileUtil's clipIncludingBorder (1.21.2 :51, CollisionGetter.java:119-130); 1.21.1
        // has the plain clip (ProjectileUtil.java:49)
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        let snowball = |release| {
            let mut state = ProjectileState::for_kind_in_release(
                ProjectileKind::Snowball,
                Vec3::new(4.5, 100.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                release,
            );
            state.gravity = 0.0;
            state.air_drag = 1.0;
            state.first_tick = false;
            let result = tick_projectile_in_release(&mut state, &world, release);
            (result.position.x, result.block_hit)
        };
        assert_eq!(snowball(MinecraftVersion::new(1, 21, 1)), (5.5, false));
        for release in [MinecraftVersion::new(1, 21, 2), MinecraftVersion::new(26, 3, 0)] {
            assert_eq!(snowball(release), (5.0 - f64::from(1.0e-5_f32), true));
        }
        // a ray starting outside passes (the start-inside test)
        let release = MinecraftVersion::new(1, 21, 4);
        let mut outside = ProjectileState::for_kind_in_release(
            ProjectileKind::Snowball,
            Vec3::new(6.0, 100.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            release,
        );
        outside.gravity = 0.0;
        outside.air_drag = 1.0;
        outside.first_tick = false;
        let result = tick_projectile_in_release(&mut outside, &world, release);
        assert!(!result.block_hit);
        assert_eq!(result.position.x, 7.0);
    }

    #[test]
    fn a_batch_refreshes_the_water_at_the_familys_base_tick() {
        // Entity.baseTick's flag: after the move from 1.21.2 (1.21.3 ThrowableProjectile.java:58), before
        // it until then (1.20.4 ThrowableProjectile.java:44); the snowball from air takes water's 0.8F on
        // the tick after it enters
        let release = MinecraftVersion::new(1, 21, 4);
        let mut world = WorldSnapshot::new();
        for x in 1..=8 {
            for y in 100..=102 {
                for z in -1..=1 {
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
        for release in [release, MinecraftVersion::new(1, 20, 4)] {
            let mut state = ProjectileState::for_kind_in_release(
                ProjectileKind::Snowball,
                Vec3::new(0.5, 101.0, 0.5),
                Vec3::new(1.0, 0.0, 0.0),
                release,
            );
            state.gravity = 0.0;
            state.first_tick = false;
            let _ = tick_projectile_in_world(&mut state, &world, release);
            let air_drag = f64::from(0.99_f32);
            let vx = state.velocity.x;
            assert!((vx - air_drag).abs() < 1.0e-9, "{release}");
            let _ = tick_projectile_in_world(&mut state, &world, release);
            assert!(state.in_water, "{release}");
            assert!((state.velocity.x - vx * f64::from(0.8_f32)).abs() < 1.0e-9, "{release}");
        }
        // leaving the water the air's drag comes back
        let release = MinecraftVersion::new(1, 21, 4);
        let mut arrow =
            ArrowState::new_in_release(Vec3::new(7.5, 101.0, 0.5), Vec3::new(2.0, 0.0, 0.0), release);
        arrow.in_water = true;
        arrow.gravity = 0.0;
        let _ = tick_arrow_in_world(&mut arrow, &world, release);
        assert!(!arrow.in_water);
    }

    #[test]
    fn a_no_physics_arrow_flies_through_blocks_without_gravity() {
        // AbstractArrow.isNoPhysics, from 1.13 (1.13.2 AbstractArrow.java:156-167, 187, 298;
        // 1.16.5 AbstractArrow.java:121; 26.3 AbstractArrow.java:165-233): no
        // containment, no in-ground branch, no ray, no gravity; the inertia stays
        let world = wall((0, 100, 0));
        let at = Vec3::new(0.5, 100.5, 0.5);
        let shot = Vec3::new(0.25, 0.0, 0.0);
        let drag = f64::from(0.99_f32);
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut arrow = ArrowState::new_in_release(at, shot, release);
            arrow.no_physics = true;
            let result = tick_arrow_in_release(&mut arrow, &world, release);
            assert!(!result.in_ground && !result.block_hit, "{release}");
            assert_eq!(arrow.position, at + shot, "{release}");
            assert_eq!(arrow.velocity, shot.scale(drag), "{release}");
            // an embedded one leaves the block with in_ground kept, as vanilla's flag stays set
            let mut embedded = ArrowState::new_in_release(at, shot, release);
            embedded.no_physics = true;
            embedded.in_ground = true;
            embedded.in_ground_time = 3;
            let result = tick_arrow_in_release(&mut embedded, &world, release);
            assert!(result.in_ground, "{release}");
            assert_eq!(result.in_ground_time, 0, "{release}");
            assert_eq!(embedded.position, at + shot, "{release}");
            // water takes the place of the air's drag
            let mut wet = ArrowState::new_in_release(at, shot, release);
            wet.no_physics = true;
            wet.in_water = true;
            let _ = tick_arrow_in_release(&mut wet, &world, release);
            assert_eq!(wet.position, at + shot, "{release}");
            assert_eq!(wet.velocity, shot.scale(wet.water_inertia), "{release}");
        }
        // the flag does not exist before 1.13
        let release = MinecraftVersion::new(1, 12, 2);
        let mut arrow = ArrowState::new_in_release(at, shot, release);
        arrow.no_physics = true;
        let plain = {
            let mut plain = ArrowState::new_in_release(at, shot, release);
            let _ = tick_arrow_in_release(&mut plain, &world, release);
            plain
        };
        let _ = tick_arrow_in_release(&mut arrow, &world, release);
        assert_eq!(arrow.position, plain.position);
        assert_eq!(arrow.velocity, plain.velocity);
    }

    #[test]
    fn containment_reads_the_exact_shape_and_the_release_faces() {
        let slab = {
            let mut world = WorldSnapshot::new();
            world.set_block(
                (0, 100, 0),
                BlockState {
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            world
        };
        let release = MinecraftVersion::new(26, 3, 0);
        let embeds = |world: &WorldSnapshot, at: Vec3, release: MinecraftVersion| {
            let mut arrow = ArrowState::new_in_release(at, Vec3::ZERO, release);
            if !release.at_least(1, 14, 0) {
                arrow.last_block_position = Some((0, 100, 0));
                arrow.last_block_id = Some(world.block_cow((0, 100, 0)).id);
            }
            tick_arrow_in_release(&mut arrow, world, release).in_ground
        };
        assert!(embeds(&slab, Vec3::new(0.5, 100.25, 0.5), release));
        assert!(!embeds(&slab, Vec3::new(0.5, 100.75, 0.5), release));
        // the top face is outside
        assert!(!embeds(&slab, Vec3::new(0.5, 100.5, 0.5), release));
        // the lower faces are in from 1.13 (1.13.2 AABB.java:213-218), out before (1.12.2 :270-274)
        let block = wall((0, 100, 0));
        let face = Vec3::new(0.0, 100.0, 0.0);
        assert!(embeds(&block, face, release));
        assert!(embeds(&block, face, MinecraftVersion::new(1, 13, 2)));
        assert!(!embeds(&block, face, MinecraftVersion::new(1, 12, 2)));
    }

    #[test]
    fn a_block_placed_around_a_flying_arrow_embeds_it_on_the_next_tick() {
        let at = Vec3::new(0.5, 100.5, 0.5);
        for release in [MinecraftVersion::new(1, 14, 4), MinecraftVersion::new(26, 3, 0)] {
            let mut arrow = ArrowState::new_in_release(at, Vec3::ZERO, release);
            arrow.gravity = 0.0;
            assert!(!tick_arrow_in_release(&mut arrow, &WorldSnapshot::new(), release).in_ground);
            let result = tick_arrow_in_release(&mut arrow, &wall((0, 100, 0)), release);
            assert!(result.in_ground, "{release}");
            assert_eq!(arrow.position, at, "{release}");
        }
    }

    #[test]
    fn a_block_around_a_stationary_arrow_embeds_it() {
        // the containing block's shape before the flight (1.14.4 AbstractArrow.java:160-173, 1.21.4
        // AbstractArrow.java:164-183), the motion stopped too from 1.21.5 (1.21.5
        // AbstractArrow.java:172-175); until 1.13.2 only the block of the last hit counts
        let world = wall((0, 100, 0));
        let at = Vec3::new(0.5, 100.5, 0.5);
        let shot = Vec3::new(0.25, 0.0, 0.0);
        for (release, keeps) in [
            (MinecraftVersion::new(1, 14, 4), true),
            (MinecraftVersion::new(1, 21, 4), true),
            (MinecraftVersion::new(1, 21, 5), false),
            (MinecraftVersion::new(26, 3, 0), false),
        ] {
            let mut arrow = ArrowState::new_in_release(at, shot, release);
            let result = tick_arrow_in_release(&mut arrow, &world, release);
            assert!(result.in_ground, "{release}");
            assert_eq!(arrow.position, at, "{release}");
            assert_eq!(arrow.velocity, if keeps { shot } else { Vec3::ZERO }, "{release}");
        }
        for release in [MinecraftVersion::new(1, 9, 4), MinecraftVersion::new(1, 13, 2)] {
            let mut stray = ArrowState::new_in_release(at, Vec3::ZERO, release);
            assert!(!tick_arrow_in_release(&mut stray, &world, release).in_ground, "{release}");
            let mut remembered = ArrowState::new_in_release(at, Vec3::ZERO, release);
            remembered.last_block_position = Some((0, 100, 0));
            remembered.last_block_id = Some(world.block_cow((0, 100, 0)).id);
            assert!(tick_arrow_in_release(&mut remembered, &world, release).in_ground, "{release}");
        }
    }

    #[test]
    fn a_replaced_block_keeps_a_trident_and_its_arrow_only_while_something_holds_them() {
        // lastState against the block read, then shouldFall's test (26.3 AbstractArrow.java:192-199,
        // 1.21.4 AbstractArrow.java:193-197, 1.16.5 :156-160, 245; 1.14.4
        // AbstractArrow.java:186; 1.13.2 AbstractArrow.java:188), which ThrownTrident.tick inherits (26.3
        // ThrownTrident.java:62-99, 1.13.2 ThrownTrident.java:57-92): another solid block holds both,
        // a block short of them or none drops both, and the remembered hit is the first block's
        let wall = (1, 100, 0);
        let impulse = Vec3::new(0.5, 0.5, 0.5);
        let low_slab = BlockState {
            id: 13,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.25, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 14, 4),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for replacement in [None, Some(low_slab.clone())] {
                let mut world = WorldSnapshot::new();
                world.set_block(wall, solid(11));
                let (from, by) = (Vec3::new(0.5, 100.5, 0.5), Vec3::new(2.0, 0.0, 0.0));
                let mut trident = TridentState::new_in_release(from, by, release);
                trident.fall_impulse = impulse;
                let mut arrow = ArrowState::new_in_release(from, by, release);
                arrow.fall_impulse = impulse;
                let step = |trident: &mut TridentState, arrow: &mut ArrowState, world: &WorldSnapshot| {
                    let result = tick_trident_in_world(trident, world, release);
                    let control = tick_arrow_in_world(arrow, world, release);
                    // the arrow control, tick by tick
                    assert_eq!(
                        (result.position, result.velocity, result.block_hit, result.in_ground),
                        (control.position, control.velocity, control.block_hit, control.in_ground),
                        "{release}"
                    );
                    assert_eq!(
                        (result.discarded, result.in_ground_time, result.shake_time, result.started_falling),
                        (control.discarded, control.in_ground_time, control.shake_time, control.started_falling),
                        "{release}"
                    );
                    assert_eq!(trident.life, arrow.life, "{release}");
                    assert_eq!(
                        (trident.last_block_id, trident.last_block_position),
                        (arrow.last_block_id, arrow.last_block_position),
                        "{release}"
                    );
                    result
                };

                let hit = step(&mut trident, &mut arrow, &world);
                assert!(hit.block_hit && hit.in_ground, "{release}");
                assert_eq!(hit.shake_time, 7, "{release}");
                assert_eq!(trident.last_block_id, Some(11), "{release}");
                assert_eq!(trident.last_block_position, Some(wall), "{release}");
                let embedded = trident.position;

                world.set_block(wall, solid(12));
                let held = step(&mut trident, &mut arrow, &world);
                assert!(held.in_ground && !held.started_falling, "{release}");
                assert_eq!(held.position, embedded, "{release}");
                assert_eq!((trident.life, held.in_ground_time), (1, 1), "{release}");
                assert_eq!(trident.last_block_id, Some(11), "{release}");
                assert_eq!(trident.last_block_position, Some(wall), "{release}");

                match replacement.clone() {
                    Some(block) => world.set_block(wall, block),
                    None => world.remove_block(wall),
                }
                let resting = trident.velocity;
                let dropped = step(&mut trident, &mut arrow, &world);
                assert!(dropped.started_falling && !dropped.in_ground, "{release}");
                // startFalling: the motion by the random impulse, life 0, no move on this tick
                assert_eq!(dropped.position, embedded, "{release}");
                assert_eq!(dropped.velocity, resting.multiply(impulse), "{release}");
                assert_eq!((trident.life, dropped.in_ground_time), (0, 2), "{release}");

                let flying = step(&mut trident, &mut arrow, &world);
                assert!(!flying.in_ground && !flying.started_falling, "{release}");
                assert_eq!(flying.in_ground_time, 0, "{release}");
            }
        }
    }

    #[test]
    fn an_embedded_trident_despawns_like_an_arrow_without_loyalty() {
        // ThrownTrident.tickDespawn without loyalty is AbstractArrow's ++life >= 1200 (26.3
        // ThrownTrident.java:213-219, AbstractArrow.java:356-362); loyalty is not modelled
        let release = MinecraftVersion::new(1, 21, 4);
        let world = wall((1, 100, 0));
        let mut state =
            TridentState::new_in_release(Vec3::new(0.5, 100.5, 0.5), Vec3::new(2.0, 0.0, 0.0), release);
        let _ = tick_trident_in_world(&mut state, &world, release);
        state.life = 1198;
        assert!(!tick_trident_in_world(&mut state, &world, release).discarded);
        assert_eq!(state.life, 1199);
        assert!(tick_trident_in_world(&mut state, &world, release).discarded);
    }

    #[test]
    fn every_arrow_alias_names_an_abstract_arrow_family() {
        for (name, kind) in [
            ("arrow", ProjectileKind::Arrow),
            ("minecraft:arrow", ProjectileKind::Arrow),
            ("Arrow", ProjectileKind::Arrow),
            ("thrown_arrow", ProjectileKind::Arrow),
            ("spectral_arrow", ProjectileKind::SpectralArrow),
            ("SpectralArrow", ProjectileKind::SpectralArrow),
            ("minecraft:spectral_arrow", ProjectileKind::SpectralArrow),
            ("tipped_arrow", ProjectileKind::TippedArrow),
            ("TippedArrow", ProjectileKind::TippedArrow),
            ("entity:tipped_arrow", ProjectileKind::TippedArrow),
            ("trident", ProjectileKind::Trident),
            ("ThrownTrident", ProjectileKind::Trident),
            ("minecraft:trident", ProjectileKind::Trident),
        ] {
            assert_eq!(ProjectileKind::from_name_checked(name), Some(kind), "{name}");
            assert!(kind.is_arrow(), "{name}");
        }
    }

    #[test]
    fn an_arrow_family_without_its_state_flies_as_its_specialized_tick_in_air_and_water() {
        // AbstractArrow's order on both sides of 1.21.2 (1.21.4 AbstractArrow.java:
        // 166-250, 1.21 :153-279): not ThrowableProjectile's 0.8F water and gravity first (1.21.4
        // ThrowableProjectile.java:42-57); a trident's water inertia is its own 0.99F (26.3
        // ThrownTrident.java:221-223, 1.13.2 ThrownTrident.java:193)
        let world = WorldSnapshot::new();
        let (from, by) = (Vec3::new(0.0, 100.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        for release in [
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(1, 21, 2),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for in_water in [false, true] {
                for kind in [
                    ProjectileKind::Arrow,
                    ProjectileKind::SpectralArrow,
                    ProjectileKind::TippedArrow,
                ] {
                    let mut generic = ProjectileState::for_kind_in_release(kind, from, by, release);
                    generic.in_water = in_water;
                    let mut arrow = ArrowState::new_in_release(from, by, release);
                    arrow.in_water = in_water;
                    for _ in 0..3 {
                        let result = tick_projectile_in_release(&mut generic, &world, release);
                        let control = tick_arrow_in_release(&mut arrow, &world, release);
                        assert_eq!(result.position, control.position, "{release} {kind:?}");
                        assert_eq!(result.velocity, control.velocity, "{release} {kind:?}");
                    }
                }
                let mut generic =
                    ProjectileState::for_kind_in_release(ProjectileKind::Trident, from, by, release);
                generic.in_water = in_water;
                let mut trident = TridentState::new_in_release(from, by, release);
                trident.in_water = in_water;
                for _ in 0..3 {
                    let result = tick_projectile_in_release(&mut generic, &world, release);
                    let control = tick_trident_in_release(&mut trident, &world, release);
                    assert_eq!(result.position, control.position, "{release}");
                    assert_eq!(result.velocity, control.velocity, "{release}");
                }
            }
        }
        // the water tick by hand: the whole old motion moves it, then 0.6F and gravity
        let modern = MinecraftVersion::new(1, 21, 4);
        let mut state =
            ProjectileState::for_kind_in_release(ProjectileKind::TippedArrow, from, by, modern);
        state.in_water = true;
        let result = tick_projectile_in_release(&mut state, &world, modern);
        assert_eq!(result.position, Vec3::new(1.0, 101.0, 0.0));
        assert_eq!(result.velocity.x, f64::from(0.6_f32));
        assert_eq!(result.velocity.y, f64::from(0.6_f32) - 0.05);
    }

    #[test]
    fn genuine_throwables_keep_their_own_integration() {
        // 1.21.4 ThrowableProjectile.java:42-57: gravity, then 0.99F, then the move; 1.20.4 :44-94: the
        // move, then 0.99F, then 0.03F
        let world = WorldSnapshot::new();
        let (from, by) = (Vec3::new(0.0, 100.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let drag = f64::from(0.99_f32);
        let modern = MinecraftVersion::new(1, 21, 4);
        let mut state = ProjectileState::for_kind_in_release(ProjectileKind::Snowball, from, by, modern);
        let result = tick_projectile_in_release(&mut state, &world, modern);
        assert_eq!(result.velocity, Vec3::new(drag, (1.0 - 0.03) * drag, 0.0));
        assert_eq!(result.position, from + result.velocity);
        let old = MinecraftVersion::new(1, 20, 4);
        let mut state = ProjectileState::for_kind_in_release(ProjectileKind::Snowball, from, by, old);
        let result = tick_projectile_in_release(&mut state, &world, old);
        assert_eq!(result.position, from + by);
        assert_eq!(result.velocity, Vec3::new(drag, drag - f64::from(0.03_f32), 0.0));
    }

    const BORDER_EAST: f64 = 5.0 - 9.999999747378752e-6;

    fn ten_wide_border() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        world
    }

    /// A snowball without gravity or drag: where it stops, and whether it hit.
    fn thrown_through(
        world: &WorldSnapshot,
        from: Vec3,
        by: Vec3,
        release: MinecraftVersion,
    ) -> (Vec3, bool) {
        let mut state =
            ProjectileState::for_kind_in_release(ProjectileKind::Snowball, from, by, release);
        state.gravity = 0.0;
        state.air_drag = 1.0;
        state.first_tick = false;
        let result = tick_projectile_in_release(&mut state, world, release);
        (result.position, result.block_hit)
    }

    #[test]
    fn every_face_of_the_border_clamps_the_end_of_the_ray_from_1_21_2() {
        // clipIncludingBorder clamps the clip's end, not the crossing, and keeps its y
        // (client1.21.2 CollisionGetter.java:119-130, 26.3 :114-125): clampVec3ToBound holds x and z
        // to [min, max - 1.0E-5F] (26.3 WorldBorder.java:99-101); 1.21.1 has the plain clip
        // (1.21 ProjectileUtil.java:49, 1.21.1 ProjectileUtil.java:49)
        let world = ten_wide_border();
        let cases = [
            (Vec3::new(4.5, 100.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(BORDER_EAST, 100.0, 0.0)),
            (Vec3::new(-4.5, 100.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), Vec3::new(-5.0, 100.0, 0.0)),
            (Vec3::new(0.0, 100.0, 4.5), Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 100.0, BORDER_EAST)),
            (Vec3::new(0.0, 100.0, -4.5), Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 100.0, -5.0)),
            // the far end held to the face: not where the ray crossed it (x 5 at y 100.25)
            (Vec3::new(4.5, 100.0, -1.0), Vec3::new(1.0, 0.5, 0.25), Vec3::new(BORDER_EAST, 100.5, -0.75)),
            // past two faces
            (Vec3::new(4.5, 100.0, 4.5), Vec3::new(1.0, 0.0, 1.0), Vec3::new(BORDER_EAST, 100.0, BORDER_EAST)),
            (Vec3::new(-4.5, 100.0, -4.5), Vec3::new(-1.0, 0.0, -1.0), Vec3::new(-5.0, 100.0, -5.0)),
        ];
        for (from, by, clamped) in cases {
            for release in [MinecraftVersion::new(1, 21, 2), MinecraftVersion::new(26, 3, 0)] {
                assert_eq!(thrown_through(&world, from, by, release), (clamped, true), "{release}");
            }
            assert_eq!(
                thrown_through(&world, from, by, MinecraftVersion::new(1, 21, 1)),
                (from + by, false)
            );
        }
    }

    #[test]
    fn a_ray_on_the_border_takes_the_half_open_bounds() {
        // isWithinBounds: min <= x < max (26.3 WorldBorder.java:79-81), so the east and south
        // faces are outside and the west and north ones inside; a clip starting outside is plain,
        // one ending inside too
        let world = ten_wide_border();
        for release in [MinecraftVersion::new(1, 21, 2), MinecraftVersion::new(26, 3, 0)] {
            let at = |x: f64, z: f64| Vec3::new(x, 100.0, z);
            let along = |x: f64, z: f64| Vec3::new(x, 0.0, z);
            // starting on the east and south faces: outside, whichever way it goes
            assert_eq!(thrown_through(&world, at(5.0, 0.0), along(1.0, 0.0), release), (at(6.0, 0.0), false));
            assert_eq!(thrown_through(&world, at(5.0, 0.0), along(-1.0, 0.0), release), (at(4.0, 0.0), false));
            assert_eq!(thrown_through(&world, at(0.0, 5.0), along(0.0, 1.0), release), (at(0.0, 6.0), false));
            // starting on the west and north faces: inside, so leaving hits at once
            assert_eq!(thrown_through(&world, at(-5.0, 0.0), along(-1.0, 0.0), release), (at(-5.0, 0.0), true));
            assert_eq!(thrown_through(&world, at(0.0, -5.0), along(0.0, -1.0), release), (at(0.0, -5.0), true));
            // ending on a face: the east one is out, the west one is in
            assert_eq!(thrown_through(&world, at(4.0, 0.0), along(1.0, 0.0), release), (at(BORDER_EAST, 0.0), true));
            assert_eq!(thrown_through(&world, at(-4.0, 0.0), along(-1.0, 0.0), release), (at(-5.0, 0.0), false));
            // starting outside, the way in or on out
            assert_eq!(thrown_through(&world, at(6.0, 0.0), along(1.0, 0.0), release), (at(7.0, 0.0), false));
            assert_eq!(thrown_through(&world, at(6.0, 0.0), along(-2.0, 0.0), release), (at(4.0, 0.0), false));
        }
    }

    #[test]
    fn the_nearer_block_wins_and_a_block_past_the_border_does_not() {
        // the clip's own location decides (client1.21.2 CollisionGetter.java:119-130): a block
        // inside stays the hit, a hit outside becomes the border's at BlockPos.containing(hit)
        let release = MinecraftVersion::new(1, 21, 2);
        let mut nearer = ten_wide_border();
        nearer.set_block((4, 100, 0), solid(21));
        let from = Vec3::new(3.5, 100.5, 0.5);
        let hit = clip_blocks(&nearer, from, Vec3::new(3.0, 0.0, 0.0), release, ClipShape::Collider).unwrap();
        assert_eq!(hit.location, Vec3::new(4.0, 100.5, 0.5));
        assert_eq!(hit.block, (4, 100, 0));
        assert!(!hit.border);

        for wall_x in [5, 6] {
            let mut past = ten_wide_border();
            past.set_block((wall_x, 100, 0), solid(21));
            let from = Vec3::new(4.5, 100.5, 0.5);
            let hit = clip_blocks(&past, from, Vec3::new(3.0, 0.0, 0.0), release, ClipShape::Collider).unwrap();
            assert_eq!(hit.location, Vec3::new(BORDER_EAST, 100.5, 0.5));
            assert_eq!(hit.block, (4, 100, 0));
            assert!(hit.border);
            // the plain clip of 1.21.1 reaches the block
            let plain = clip_blocks(&past, from, Vec3::new(3.0, 0.0, 0.0), MinecraftVersion::new(1, 21, 1), ClipShape::Collider).unwrap();
            assert_eq!(plain.location, Vec3::new(f64::from(wall_x), 100.5, 0.5));
            assert_eq!(plain.block, (wall_x, 100, 0));
            assert!(!plain.border);
        }

        let west = clip_blocks(&ten_wide_border(), Vec3::new(-4.5, 100.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), release, ClipShape::Collider).unwrap();
        assert_eq!(west.block, (-5, 100, 0));
        assert!(west.border);
    }

    #[test]
    fn the_1_13_2_ray_trace_walks_the_old_way_and_clips_the_outline() {
        use crate::math::HitFace;
        let (old, new) = (MinecraftVersion::new(1, 13, 2), MinecraftVersion::new(1, 14, 4));
        // soul sand: collision 0.875 high, outline 1.0 (1.13.2 Block.java:567-568 clips getShape of
        // a block with a collision shape; 1.14.4 ClipContext.Block.COLLIDER the collision shape)
        let mut sand = WorldSnapshot::new();
        sand.set_block((2, 100, 0), BlockState {
            id: 30,
            shape: Some(BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0)), ..BlockShape::full_block() }),
            outline_shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        });
        let skim = (Vec3::new(0.5, 100.95, 0.5), Vec3::new(3.0, 0.0, 0.0));
        assert_eq!(clip_blocks(&sand, skim.0, skim.1, old, ClipShape::Collider).unwrap().location, Vec3::new(2.0, 100.95, 0.5));
        assert_eq!(clip_blocks(&sand, skim.0, skim.1, new, ClipShape::Collider), None);
        // a start inside a shape faces the way the ray goes in 1.13.2 (VoxelShape.java:157), back from
        // 1.14 (1.14.4 VoxelShape.java:134)
        let mut block = WorldSnapshot::new();
        block.set_block((0, 100, 0), solid(31));
        let inside = |release| clip_blocks(&block, Vec3::new(0.5, 100.5, 0.5), Vec3::new(1.0, 0.0, 0.0), release, ClipShape::Collider).unwrap().face;
        assert_eq!(inside(old), HitFace::East);
        assert_eq!(inside(new), HitFace::West);
        // the old walk gives up after 201 cells (1.13.2 Level.java:756-758)
        let mut far = WorldSnapshot::new();
        far.set_block((250, 100, 0), solid(32));
        let long = (Vec3::new(0.5, 100.5, 0.5), Vec3::new(300.0, 0.0, 0.0));
        assert_eq!(clip_blocks(&far, long.0, long.1, old, ClipShape::Collider), None);
        assert_eq!(clip_blocks(&far, long.0, long.1, new, ClipShape::Collider).unwrap().block, (250, 100, 0));
        // water is no block to the old trace (LiquidBlock.java:85)
        let mut water = WorldSnapshot::new();
        water.set_block((2, 100, 0), BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            water_block: true,
            outline_shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        });
        assert_eq!(clip_blocks(&water, Vec3::new(0.5, 100.5, 0.5), Vec3::new(3.0, 0.0, 0.0), old, ClipShape::Outline), None);
    }

    #[test]
    fn a_zero_move_never_meets_the_border() {
        // from equal to to misses at to (26.3 BlockGetter.java:102-104), which is where it began
        let mut world = ten_wide_border();
        world.set_block((0, 100, 0), solid(21));
        let release = MinecraftVersion::new(1, 21, 2);
        for from in [
            Vec3::new(4.5, 100.0, 0.0),
            Vec3::new(5.0, 100.0, 0.0),
            Vec3::new(-5.0, 100.0, 0.0),
            Vec3::new(7.0, 100.0, 0.0),
            // inside a solid block too
            Vec3::new(0.5, 100.5, 0.5),
        ] {
            assert_eq!(clip_blocks(&world, from, Vec3::ZERO, release, ClipShape::Collider), None);
            assert_eq!(thrown_through(&world, from, Vec3::ZERO, release), (from, false));
        }
    }

    #[test]
    fn a_rocket_reports_the_border_hit_from_1_21_2_and_is_stopped_by_entity_move_on_both_sides() {
        // the hit test takes the boosted motion before the move from 1.21.2 (1.21.3
        // FireworkRocketEntity.java:137-147, getCollision at :139, 26.3 :145-147) and moves nothing;
        // Entity.move holds the box at the border in both (26.3 CollisionGetter.java:110-113)
        let world = ten_wide_border();
        let shot = |world: &WorldSnapshot, from: Vec3, release| {
            let mut state = ProjectileState::for_kind_in_release(
                ProjectileKind::Firework,
                from,
                Vec3::new(1.0, 0.0, 0.0),
                release,
            );
            state.shot_at_angle = true;
            let result = tick_projectile_in_release(&mut state, world, release);
            (result.position.x, result.block_hit, result.velocity.x)
        };
        let from = Vec3::new(4.5, 100.0, 0.0);
        // box half width 0.125: x 5 less that, and the motion comes back after the move
        assert_eq!(shot(&world, from, MinecraftVersion::new(1, 21, 1)), (4.875, false, 1.0));
        assert_eq!(shot(&world, from, MinecraftVersion::new(1, 21, 2)), (4.875, true, 1.0));
        assert_eq!(shot(&world, from, MinecraftVersion::new(26, 3, 0)), (4.875, true, 1.0));
        // a block short of the border is an ordinary hit on both sides
        let mut walled = ten_wide_border();
        walled.set_block((3, 100, 0), solid(21));
        let near = Vec3::new(2.5, 100.0, 0.0);
        assert_eq!(shot(&walled, near, MinecraftVersion::new(1, 21, 1)), (2.875, true, 1.0));
        assert_eq!(shot(&walled, near, MinecraftVersion::new(1, 21, 2)), (2.875, true, 1.0));
    }

    #[test]
    fn a_fireball_stops_at_the_border_from_1_21_2() {
        // AbstractHurtingProjectile's getHitResultOnMoveVector, no bounce (1.21.5 Projectile.java:
        // 247-249), and the hit's location is the new position (26.3
        // AbstractHurtingProjectile.java:70-78)
        let world = ten_wide_border();
        let shot = |release| {
            let mut state = ProjectileState::for_kind_in_release(
                ProjectileKind::Fireball,
                Vec3::new(4.5, 100.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                release,
            );
            state.air_drag = 1.0;
            state.acceleration_power = 0.0;
            let result = tick_projectile_in_release(&mut state, &world, release);
            (result.position.x, result.block_hit)
        };
        assert_eq!(shot(MinecraftVersion::new(1, 21, 1)), (5.5, false));
        assert_eq!(shot(MinecraftVersion::new(1, 21, 2)), (BORDER_EAST, true));
    }

    #[test]
    fn an_arrow_and_a_trident_turn_back_from_the_border_from_1_21_2() {
        // AbstractArrow's clipIncludingBorder (1.21.3 AbstractArrow.java:229-231, 26.3
        // AbstractArrow.java:236; 1.21 :201 has the plain raycast) and the bounce that comes with it:
        // client1.21.2 Projectile.java:232-237, AbstractArrow.java:765-767; 1.21.4
        // Projectile.java:225-231, AbstractArrow.java:773-777, ProjectileDeflection
        // SIMPLE's -0.5, then 0.2; 26.3 Projectile.java:262-265 by power 0.2. It is no hit, so
        // no embedding and no shake; the air inertia and the gravity still follow
        let world = ten_wide_border();
        let (from, by) = (Vec3::new(4.5, 100.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let flown = tick_arrow_in_release(
            &mut ArrowState::new_in_release(from, by, MinecraftVersion::new(1, 21, 1)),
            &world,
            MinecraftVersion::new(1, 21, 1),
        );
        assert_eq!(flown.position, from + by);
        assert!(!flown.block_hit && !flown.in_ground);
        for release in [
            MinecraftVersion::new(1, 21, 2),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut arrow = ArrowState::new_in_release(from, by, release);
            let result = tick_arrow_in_release(&mut arrow, &world, release);
            assert_eq!(result.position, Vec3::new(BORDER_EAST, 100.0, 0.0), "{release}");
            assert_eq!(result.velocity.x, -0.1 * f64::from(0.99_f32), "{release}");
            assert_eq!(result.velocity.y, -0.05, "{release}");
            assert!(!result.block_hit && !result.in_ground, "{release}");
            assert_eq!(result.shake_time, 0, "{release}");
            assert_eq!(arrow.last_block_position, None, "{release}");
            // it flies on, back inside
            let next = tick_arrow_in_release(&mut arrow, &world, release);
            assert!(next.position.x < BORDER_EAST && !next.in_ground, "{release}");

            let mut trident = TridentState::new_in_release(from, by, release);
            trident.in_water = true;
            let result = tick_trident_in_release(&mut trident, &world, release);
            assert!(!result.block_hit && !result.in_ground, "{release}");
            assert_eq!(result.velocity.x, -0.1 * f64::from(0.99_f32), "{release}");
        }
        // 26.3's REVERSE takes -power * 0.5 at once; before, -0.5 then 0.2 (26.3
        // ProjectileDeflection.java:12-14; 1.21.5 :10-14, Projectile.java:235-238)
        let bounced = |release| {
            let by = Vec3::new(0.7, 0.0, 0.0);
            let mut arrow = ArrowState::new_in_release(Vec3::new(4.5, 100.0, 0.0), by, release);
            arrow.air_drag = 1.0;
            tick_arrow_in_release(&mut arrow, &world, release).velocity.x
        };
        assert_eq!(bounced(MinecraftVersion::new(26, 2, 0)), 0.7 * -0.5 * 0.2);
        assert_eq!(bounced(MinecraftVersion::new(26, 3, 0)), 0.7 * (-0.2 * 0.5));
    }

    #[test]
    fn a_fishing_hook_turns_back_from_the_border_on_the_client_until_26_3() {
        // FishingHook.shouldBounceOnWorldBorder (client1.21.2 FishingHook.java:112-114, 26.3
        // :119-121): the bounce, not onHitBlock's motion to the hit. deflect moves it on both sides
        // until 26.2 (26.2 Projectile.java:229-237), on the server only in 26.3 (26.3
        // Projectile.java:278-284), and the client's hook keeps its motion. Then -0.03, the move and
        // 0.92 (26.3 FishingHook.java:224-241)
        let world = ten_wide_border();
        let shot = |release| {
            let mut state = ProjectileState::for_kind_in_release(
                ProjectileKind::FishingHook,
                Vec3::new(4.5, 100.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                release,
            );
            let result = tick_projectile_in_release(&mut state, &world, release);
            (result.position.x, result.velocity.x, result.block_hit)
        };
        let collided = (4.875, 0.0, false);
        assert_eq!(shot(MinecraftVersion::new(1, 21, 1)), collided);
        for release in [
            MinecraftVersion::new(1, 21, 2),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 2, 0),
        ] {
            assert_eq!(shot(release), (4.5 + -0.5 * 0.2, -0.5 * 0.2 * 0.92, false), "{release}");
        }
        assert_eq!(shot(MinecraftVersion::new(26, 3, 0)), collided);
    }

    #[test]
    fn rays_other_than_a_projectiles_stay_plain_clips() {
        // Entity's fall-damage ray and Entity.pick are BlockGetter.clip (26.3 Entity.java:799, 2066);
        // only the projectile paths take clipIncludingBorder (26.3 ProjectileUtil.java:67, 82, 188,
        // AbstractArrow.java:236), so the border hits nothing here and hides no block past it
        let mut world = ten_wide_border();
        let release = MinecraftVersion::new(26, 3, 0);
        let (from, past) = (Vec3::new(4.5, 100.5, 0.5), Vec3::new(5.5, 100.5, 0.5));
        let fall = ClipShape::FallDamageResetting { player: true };
        assert!(!clip_hits(&world, from, past, release, fall));
        assert!(!clip_hits(&world, from, past, release, ClipShape::Collider));
        world.set_block((5, 100, 0), solid(21));
        assert!(clip_hits(&world, from, past, release, ClipShape::Collider));
    }

    /// A world whose chunks around the origin are loaded, as EntityFluidInteraction asks from 26.1.
    fn loaded_world() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for chunk_x in -1..=1 {
            for chunk_z in -1..=1 {
                world.set_chunk_loaded(chunk_x, chunk_z, true);
            }
        }
        world
    }

    fn water_flowing(flow: Vec3) -> BlockState {
        BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            fluid_flow: flow,
            ..BlockState::default()
        }
    }

    /// Water filling the columns `xs`, from y 99 to 102 and z -1 to 1.
    fn flood(world: &mut WorldSnapshot, xs: std::ops::RangeInclusive<i32>, flow: Vec3) {
        for x in xs {
            for y in 99..=102 {
                for z in -1..=1 {
                    world.set_block((x, y, z), water_flowing(flow));
                }
            }
        }
    }

    /// A thrown item without gravity, between ticks.
    fn in_flight(position: Vec3, velocity: Vec3, release: MinecraftVersion) -> ProjectileState {
        let mut state =
            ProjectileState::for_kind_in_release(ProjectileKind::Snowball, position, velocity, release);
        state.gravity = 0.0;
        state.first_tick = false;
        state
    }

    const BASE_TICK_FIRST: [MinecraftVersion; 3] = [
        MinecraftVersion::new(1, 12, 2),
        MinecraftVersion::new(1, 20, 4),
        MinecraftVersion::new(1, 21, 1),
    ];
    const BASE_TICK_AFTER_THE_MOVE: [MinecraftVersion; 3] = [
        MinecraftVersion::new(1, 21, 2),
        MinecraftVersion::new(1, 21, 4),
        MinecraftVersion::new(26, 3, 0),
    ];

    #[test]
    fn a_thrown_item_leaving_the_water_takes_the_air_inertia_at_its_base_tick() {
        // isInWater is the last base tick's: after the move from 1.21.2 (client1.21.2
        // ThrowableProjectile.java:43-62, 26.3 :41-60), before it until 1.21.1 (1.21.1 :39-66)
        let mut world = loaded_world();
        flood(&mut world, 1..=3, Vec3::ZERO);
        let (water, air) = (f64::from(0.8_f32), f64::from(0.99_f32));
        let from = Vec3::new(3.5, 100.0, 0.5);
        for release in BASE_TICK_AFTER_THE_MOVE {
            let mut state = in_flight(from, Vec3::new(1.0, 0.0, 0.0), release);
            state.in_water = true;
            let result = tick_projectile_in_world(&mut state, &world, release);
            assert_eq!(result.velocity.x, water, "{release}");
            assert!(!state.in_water, "{release}");
            let result = tick_projectile_in_world(&mut state, &world, release);
            assert_eq!(result.velocity.x, water * air, "{release}");
        }
        for release in BASE_TICK_FIRST {
            let mut state = in_flight(from, Vec3::new(1.0, 0.0, 0.0), release);
            state.in_water = true;
            let result = tick_projectile_in_world(&mut state, &world, release);
            assert_eq!(result.velocity.x, water, "{release}");
            assert!(state.in_water, "{release}");
            let result = tick_projectile_in_world(&mut state, &world, release);
            assert_eq!(result.velocity.x, water * air, "{release}");
            assert!(!state.in_water, "{release}");
        }
    }

    #[test]
    fn the_first_tick_reads_the_water_of_the_base_tick_before_it() {
        // a new entity's wasTouchingWater is false: from 1.21.2 the first tick is in the air's
        // inertia, whatever it started in; until 1.21.1 the base tick has already found the water.
        // firstTick itself only silences the splash (26.3 Entity.java:1692-1694)
        let mut world = loaded_world();
        flood(&mut world, -1..=3, Vec3::ZERO);
        let from = Vec3::new(0.5, 100.0, 0.5);
        for release in BASE_TICK_AFTER_THE_MOVE {
            let mut state = in_flight(from, Vec3::new(1.0, 0.0, 0.0), release);
            state.first_tick = true;
            let result = tick_projectile_in_world(&mut state, &world, release);
            assert_eq!(result.velocity.x, f64::from(0.99_f32), "{release}");
            assert!(state.in_water && !state.first_tick, "{release}");
        }
        for release in BASE_TICK_FIRST {
            let mut state = in_flight(from, Vec3::new(1.0, 0.0, 0.0), release);
            state.first_tick = true;
            let result = tick_projectile_in_world(&mut state, &world, release);
            assert_eq!(result.velocity.x, f64::from(0.8_f32), "{release}");
            assert!(state.in_water && !state.first_tick, "{release}");
        }
    }

    #[test]
    fn shallow_water_counts_by_the_release_box() {
        // a level seven flow is 1/9 of a block. From 1.14 the box deflated by 0.001 touches a fluid
        // whose top is at or above its bottom (1.21.4 Entity.java:3307-3328, 26.3
        // EntityFluidInteraction.java:44-85). Before 1.13 the box shrunk by 0.4F turns over and
        // reaches 0.15 below the entity and 0.15 above its top, and any water cell of it counts up to
        // the ceiling of that box (1.12.2 Entity.java:1043, Level.java:1616)
        let mut world = loaded_world();
        world.set_block(
            (0, 100, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 7,
                ..BlockState::default()
            },
        );
        let touches = |y: f64, release| {
            let mut state = in_flight(Vec3::new(0.5, y, 0.5), Vec3::ZERO, release);
            let _ = tick_projectile_in_world(&mut state, &world, release);
            state.in_water
        };
        for release in [
            MinecraftVersion::new(1, 14, 4),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            assert!(touches(100.1, release), "{release}");
            assert!(!touches(100.12, release), "{release}");
        }
        assert!(touches(100.6, MinecraftVersion::new(1, 12, 2)));
        assert!(!touches(100.6, MinecraftVersion::new(1, 20, 4)));
    }

    #[test]
    fn a_lava_current_pushes_projectiles_from_1_16_at_the_dimensions_scale() {
        // 1.16.5 Entity.java:1064-1069, 26.3 :1702-1705: 0.0023333333333333335, 0.007 where lava
        // is fast. Lava is no water: the air's inertia stays. None before 1.16 (1.15 Entity.java
        // has no lava current).
        let lava = |flow: Vec3| BlockState {
            fluid: FluidState::Lava,
            fluid_source: true,
            fluid_flow: flow,
            ..BlockState::default()
        };
        let throw = |flow: Vec3, ultra_warm, release| {
            let mut world = loaded_world();
            for x in -1..=4 {
                for y in 99..=102 {
                    for z in -1..=1 {
                        world.set_block((x, y, z), lava(flow));
                    }
                }
            }
            let mut state = in_flight(Vec3::new(0.5, 100.0, 0.5), Vec3::new(0.5, 0.0, 0.0), release);
            state.air_drag = 1.0;
            state.ultra_warm = ultra_warm;
            let first = tick_projectile_in_world(&mut state, &world, release);
            (first.position.x, state.velocity.x, state.in_water)
        };
        let east = Vec3::new(1.0, 0.0, 0.0);
        for release in [
            MinecraftVersion::new(1, 21, 2),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            // the push follows the move
            assert_eq!(throw(Vec3::ZERO, false, release), (1.0, 0.5, false), "{release}");
            assert_eq!(throw(east, false, release), (1.0, 0.5 + 0.0023333333333333335, false), "{release}");
            assert_eq!(throw(east, true, release), (1.0, 0.5 + 0.007, false), "{release}");
        }
        for release in [MinecraftVersion::new(1, 16, 5), MinecraftVersion::new(1, 21, 1)] {
            // the push opens the tick and moves it at once
            let push = 0.5 + 0.0023333333333333335;
            assert_eq!(throw(east, false, release), (0.5 + push, push, false), "{release}");
            assert_eq!(throw(Vec3::ZERO, true, release).0, 1.0, "{release}");
        }
        let release = MinecraftVersion::new(1, 15, 2);
        assert_eq!(throw(east, true, release).0, 1.0);
    }

    #[test]
    fn lava_pushes_a_projectile_in_water_only_from_1_16_2() {
        // 1.16 and 1.16.1 Entity.java:1042-1044 return once in water, before lava's push;
        // 1.16.2 Entity.java:1064-1070 pushes by lava whatever the water did
        let mut world = loaded_world();
        for y in 99..=102 {
            for z in -1..=1 {
                world.set_block((0, y, z), water_flowing(Vec3::ZERO));
                world.set_block((1, y, z), BlockState {
                    fluid: FluidState::Lava,
                    fluid_source: true,
                    fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                    ..BlockState::default()
                });
            }
        }
        let push = |release| {
            // the box spans x 0.875 to 1.125, in the water and the lava
            let mut state = in_flight(Vec3::new(1.0, 100.0, 0.5), Vec3::ZERO, release);
            state.air_drag = 1.0;
            tick_projectile_in_world(&mut state, &world, release);
            (state.in_water, state.velocity.x)
        };
        for release in [MinecraftVersion::new(1, 16, 0), MinecraftVersion::new(1, 16, 1)] {
            assert_eq!(push(release), (true, 0.0), "{release}");
        }
        for release in [MinecraftVersion::new(1, 16, 2), MinecraftVersion::new(1, 16, 5)] {
            let (in_water, x) = push(release);
            assert!(in_water && x > 0.0, "{release} {x}");
        }
    }

    #[test]
    fn a_lava_current_moves_an_arrow_until_it_is_embedded_from_1_21_5() {
        let mut world = loaded_world();
        for x in -1..=4 {
            for y in 99..=102 {
                for z in -1..=1 {
                    world.set_block(
                        (x, y, z),
                        BlockState {
                            fluid: FluidState::Lava,
                            fluid_source: true,
                            fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                            ..BlockState::default()
                        },
                    );
                }
            }
        }
        let release = MinecraftVersion::new(26, 3, 0);
        let mut arrow = ArrowState::new_in_release(Vec3::new(0.5, 100.0, 0.5), Vec3::ZERO, release);
        arrow.gravity = 0.0;
        let _ = tick_arrow_in_world(&mut arrow, &world, release);
        assert!(arrow.velocity.x > 0.0);
        // isPushedByFluid is !isInGround from 1.21.5 (1.21.5 AbstractArrow.java:327-329): a noPhysics
        // arrow reaches the flying branch's base tick still in the ground, and is pushed only before
        for (release, pushed) in [
            (MinecraftVersion::new(1, 21, 4), true),
            (MinecraftVersion::new(1, 21, 5), false),
        ] {
            let mut embedded =
                ArrowState::new_in_release(Vec3::new(0.5, 100.0, 0.5), Vec3::ZERO, release);
            embedded.in_ground = true;
            embedded.no_physics = true;
            let _ = tick_arrow_in_world(&mut embedded, &world, release);
            assert_eq!(embedded.velocity.x > 0.0, pushed, "{release}");
        }
    }

    #[test]
    fn a_current_pushes_by_0_014_and_the_water_only_drags() {
        // the same throw in still and flowing water: the drag is the same 0.8F, and the current's
        // normalized 0.014 is the whole difference (1.21.4 Entity.java:3346-3362, 26.3
        // EntityFluidInteraction.java:243-260, 26.3 Entity.java:1698-1701, 1.12.2 Level.java:1616,
        // Entity.java:1043). From 1.21.2 the push follows the move and only the next tick
        // moves by it; until 1.21.1 it opens the tick and moves the projectile at once
        let throw = |flow: Vec3, release| {
            let mut world = loaded_world();
            flood(&mut world, -1..=4, flow);
            let mut state = in_flight(Vec3::new(0.5, 100.0, 0.5), Vec3::new(0.5, 0.0, 0.0), release);
            state.air_drag = 1.0;
            let first = tick_projectile_in_world(&mut state, &world, release);
            let second = tick_projectile_in_world(&mut state, &world, release);
            (first, second, state.in_water)
        };
        let drag = f64::from(0.8_f32);
        let east = Vec3::new(1.0, 0.0, 0.0);
        for release in BASE_TICK_AFTER_THE_MOVE {
            let (first, second, in_water) = throw(Vec3::ZERO, release);
            assert_eq!((first.position.x, first.velocity.x), (1.0, 0.5), "{release}");
            assert_eq!(second.velocity.x, 0.5 * drag, "{release}");
            assert_eq!(second.position.x, 1.0 + 0.5 * drag, "{release}");
            assert!(in_water, "{release}");

            let (first, second, in_water) = throw(east, release);
            assert_eq!((first.position.x, first.velocity.x), (1.0, 0.5 + 0.014), "{release}");
            assert_eq!(second.position.x, 1.0 + (0.5 + 0.014) * drag, "{release}");
            assert_eq!(second.velocity.x, (0.5 + 0.014) * drag + 0.014, "{release}");
            assert!(in_water, "{release}");
        }
        for release in BASE_TICK_FIRST {
            let (first, second, in_water) = throw(Vec3::ZERO, release);
            assert_eq!((first.position.x, first.velocity.x), (1.0, 0.5 * drag), "{release}");
            assert_eq!(second.position.x, 1.0 + 0.5 * drag, "{release}");
            assert!(in_water, "{release}");

            let (first, second, in_water) = throw(east, release);
            assert_eq!(first.position.x, 0.5 + (0.5 + 0.014), "{release}");
            assert_eq!(first.velocity.x, (0.5 + 0.014) * drag, "{release}");
            let pushed = (0.5 + 0.014) * drag + 0.014;
            assert_eq!(second.position.x, 0.5 + (0.5 + 0.014) + pushed, "{release}");
            assert_eq!(second.velocity.x, pushed * drag, "{release}");
            assert!(in_water, "{release}");
        }
    }

    #[test]
    fn hurting_projectiles_take_the_water_at_their_base_tick() {
        // after the move from 1.21.2 (26.3 AbstractHurtingProjectile.java:66-92, 1.21.2 :69-113),
        // before it until 1.21.1 (1.21.1 :66-108); 0.8F in water, 0.95F in the air
        let mut world = loaded_world();
        flood(&mut world, 2..=6, Vec3::ZERO);
        let (air, water) = (f64::from(0.95_f32), f64::from(0.8_f32));
        let shot = |release| {
            let mut state = ProjectileState::for_kind_in_release(
                ProjectileKind::SmallFireball,
                Vec3::new(0.5, 100.0, 0.5),
                Vec3::new(2.0, 0.0, 0.0),
                release,
            );
            state.acceleration_power = 0.0;
            state
        };
        let modern = MinecraftVersion::new(1, 21, 4);
        let mut state = shot(modern);
        assert_eq!(tick_projectile_in_world(&mut state, &world, modern).velocity.x, 2.0 * air);
        assert!(state.in_water);
        assert_eq!(tick_projectile_in_world(&mut state, &world, modern).velocity.x, 2.0 * air * water);
        let old = MinecraftVersion::new(1, 20, 4);
        let mut state = shot(old);
        assert_eq!(tick_projectile_in_world(&mut state, &world, old).velocity.x, 2.0 * air);
        assert!(!state.in_water);
        assert_eq!(tick_projectile_in_world(&mut state, &world, old).velocity.x, 2.0 * air * water);
        assert!(state.in_water);
    }

    #[test]
    fn an_arrow_and_a_trident_take_their_own_water_inertia_at_the_base_tick() {
        // the flag from the base tick after the move from 1.21.2 (26.3 AbstractArrow.java:251,
        // 1.21.3 AbstractArrow.java:242), before all of it until 1.21.1 (1.21
        // AbstractArrow.java:154); 0.6F, a trident's 0.99F
        let mut world = loaded_world();
        flood(&mut world, 1..=8, Vec3::ZERO);
        let air = f64::from(0.99_f32);
        let (from, by) = (Vec3::new(0.5, 100.5, 0.5), Vec3::new(1.0, 0.0, 0.0));
        for (release, entered_on_tick_one) in [
            (MinecraftVersion::new(1, 20, 4), false),
            (MinecraftVersion::new(1, 21, 1), false),
            (MinecraftVersion::new(1, 21, 2), true),
            (MinecraftVersion::new(1, 21, 4), true),
            (MinecraftVersion::new(26, 3, 0), true),
        ] {
            let mut arrow = ArrowState::new_in_release(from, by, release);
            arrow.gravity = 0.0;
            let mut trident = TridentState::new_in_release(from, by, release);
            trident.gravity = 0.0;
            let result = tick_arrow_in_world(&mut arrow, &world, release);
            let _ = tick_trident_in_world(&mut trident, &world, release);
            assert_eq!(result.velocity.x, air, "{release}");
            assert_eq!(arrow.in_water, entered_on_tick_one, "{release}");
            assert_eq!(trident.in_water, entered_on_tick_one, "{release}");
            let result = tick_arrow_in_world(&mut arrow, &world, release);
            assert_eq!(result.position.x, from.x + 1.0 + air, "{release}");
            assert_eq!(result.velocity.x, air * f64::from(0.6_f32), "{release}");
            assert!(arrow.in_water, "{release}");
            let result = tick_trident_in_world(&mut trident, &world, release);
            assert_eq!(result.velocity.x, air * f64::from(0.99_f32), "{release}");
            assert!(trident.in_water, "{release}");
        }
    }

    /// An arrow fired from the air at a wall behind water: the water is at x 1, the wall at x 2.
    fn walled_water(flow: Vec3) -> WorldSnapshot {
        let mut world = loaded_world();
        flood(&mut world, 1..=1, flow);
        world.set_block((2, 100, 0), solid(11));
        world
    }

    #[test]
    fn the_tick_that_embeds_an_arrow_still_runs_its_base_tick() {
        // super.tick() closes the flying branch whatever stepMoveAndHit did (26.3
        // AbstractArrow.java:225-251, 1.21.3 AbstractArrow.java:215-244), and until
        // 1.21.1 it opens every tick, in the ground too (1.21 :154, 1.20.4 PersistentProjectileEntity
        // tick), so an arrow that hits under water knows it
        let world = walled_water(Vec3::ZERO);
        let (from, by) = (Vec3::new(0.5, 100.5, 0.5), Vec3::new(2.0, 0.0, 0.0));
        for release in [
            MinecraftVersion::new(1, 21, 2),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut arrow = ArrowState::new_in_release(from, by, release);
            let result = tick_arrow_in_world(&mut arrow, &world, release);
            assert!(result.in_ground && arrow.in_water, "{release}");
            let mut trident = TridentState::new_in_release(from, by, release);
            let result = tick_trident_in_world(&mut trident, &world, release);
            assert!(result.in_ground && trident.in_water, "{release}");
        }
        let release = MinecraftVersion::new(1, 20, 4);
        let mut arrow = ArrowState::new_in_release(from, by, release);
        let result = tick_arrow_in_world(&mut arrow, &world, release);
        assert!(result.in_ground && !arrow.in_water);
        let result = tick_arrow_in_world(&mut arrow, &world, release);
        assert!(result.in_ground && arrow.in_water);
    }

    #[test]
    fn the_ground_branch_runs_no_base_tick_from_1_21_2() {
        // AbstractArrow.tick's in-ground branch has no super.tick() (26.3 AbstractArrow.java:192-212,
        // 1.21.3 AbstractArrow.java:192-206), on the tick it falls either: isInWater
        // stays what the last flying tick left. Until 1.21.1 it opens that tick too.
        let release = MinecraftVersion::new(1, 21, 4);
        let dry = wall((5, 100, 5));
        let mut arrow = ArrowState {
            in_ground: true,
            in_water: true,
            last_block_id: Some(11),
            ..ArrowState::new_in_release(Vec3::new(0.5, 100.5, 0.5), Vec3::ZERO, release)
        };
        let result = tick_arrow_in_world(&mut arrow, &dry, release);
        assert!(result.started_falling);
        assert!(arrow.in_water);
        let release = MinecraftVersion::new(1, 20, 4);
        let mut arrow = ArrowState {
            in_ground: true,
            in_water: true,
            last_block_id: Some(11),
            ..ArrowState::new_in_release(Vec3::new(0.5, 100.5, 0.5), Vec3::ZERO, release)
        };
        let result = tick_arrow_in_world(&mut arrow, &dry, release);
        assert!(result.started_falling);
        assert!(!arrow.in_water);
    }

    #[test]
    fn a_current_pushes_an_embedded_arrow_until_1_21_5() {
        // isPushedByFluid is !isInGround from 1.21.5 (1.21.5 AbstractArrow.java:327-329, 26.3 :344-346);
        // 1.21.4 AbstractArrow has no override of Entity's true (1.21.4 Entity.java:2907, 3315). So the
        // hit tick's closing base tick pushes the stopped arrow up to 1.21.4, and until 1.21.1 each
        // tick's opening one pushes it in the ground; its position stays
        let south = Vec3::new(0.0, 0.0, 1.0);
        let world = walled_water(south);
        let (from, by) = (Vec3::new(0.5, 100.5, 0.5), Vec3::new(2.0, 0.0, 0.0));

        let release = MinecraftVersion::new(1, 21, 5);
        let mut arrow = ArrowState::new_in_release(from, by, release);
        assert_eq!(tick_arrow_in_world(&mut arrow, &world, release).velocity, Vec3::ZERO);
        assert_eq!(tick_arrow_in_world(&mut arrow, &world, release).velocity, Vec3::ZERO);

        for release in [MinecraftVersion::new(1, 21, 2), MinecraftVersion::new(1, 21, 4)] {
            let mut arrow = ArrowState::new_in_release(from, by, release);
            let pushed = Vec3::new(0.0, 0.0, 0.014);
            let result = tick_arrow_in_world(&mut arrow, &world, release);
            assert!(result.in_ground, "{release}");
            assert_eq!(result.velocity, pushed, "{release}");
            assert_eq!(tick_arrow_in_world(&mut arrow, &world, release).velocity, pushed, "{release}");
        }

        let release = MinecraftVersion::new(1, 20, 4);
        let mut arrow = ArrowState::new_in_release(from, by, release);
        let _ = tick_arrow_in_world(&mut arrow, &world, release);
        let embedded = tick_arrow_in_world(&mut arrow, &world, release);
        assert!(embedded.in_ground);
        let later = tick_arrow_in_world(&mut arrow, &world, release);
        assert!(later.in_ground);
        assert_eq!(later.position, embedded.position);
        assert_eq!(later.velocity.z, embedded.velocity.z + 0.014);
    }

    #[test]
    fn the_other_families_open_with_the_base_tick_in_every_release() {
        // super.tick() is the first thing in FireworkRocketEntity.tick, FishingHook.tick, LlamaSpit.tick
        // and ShulkerBullet.tick, before and after 1.21.2 (1.21.3 FireworkRocketEntity.java:101,
        // FishingHook.java:142; 26.3 FireworkRocketEntity.java:108, FishingHook.java:146,
        // LlamaSpit.java:38, ShulkerBullet.java:185; 1.16.5 LlamaSpit.java:48,
        // ShulkerBullet.java:200, FireworkRocketEntity.java:127, FishingHook.java:150):
        // a current pushes them all, before the tick reads the motion
        let south = Vec3::new(0.0, 0.0, 1.0);
        let mut world = loaded_world();
        flood(&mut world, -1..=3, south);
        let at = Vec3::new(0.5, 100.0, 0.5);
        for release in [
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut bullet =
                ProjectileState::for_kind_in_release(ProjectileKind::ShulkerBullet, at, Vec3::ZERO, release);
            let result = tick_projectile_in_world(&mut bullet, &world, release);
            assert_eq!(result.position, at + Vec3::new(0.0, 0.0, 0.014), "{release}");
            assert!(bullet.in_water, "{release}");

            let mut rocket =
                ProjectileState::for_kind_in_release(ProjectileKind::Firework, at, Vec3::ZERO, release);
            rocket.shot_at_angle = true;
            let result = tick_projectile_in_world(&mut rocket, &world, release);
            assert_eq!(result.position, at + Vec3::new(0.0, 0.0, 0.014), "{release}");
            assert_eq!(result.velocity, Vec3::new(0.0, 0.0, 0.014), "{release}");

            // reaching water slows the pushed motion and starts it bobbing, without moving it
            let mut hook =
                ProjectileState::for_kind_in_release(ProjectileKind::FishingHook, at, Vec3::ZERO, release);
            let result = tick_projectile_in_world(&mut hook, &world, release);
            assert_eq!(result.position, at, "{release}");
            assert_eq!(result.velocity, Vec3::new(0.0, 0.0, 0.014 * 0.3), "{release}");
            assert!(result.bobbing, "{release}");
        }
    }

    #[test]
    fn llama_spit_meets_the_water_of_its_base_tick_in_a_batch() {
        // LlamaSpit.tick: super.tick(), then the removal in water (26.3 LlamaSpit.java:38-50). The
        // box half out of a source's 8/9 surface has air in it, so only isInWater removes it
        let mut world = loaded_world();
        for x in -1..=1 {
            for z in -1..=1 {
                world.set_block((x, 100, z), water_flowing(Vec3::ZERO));
            }
        }
        for release in [MinecraftVersion::new(1, 16, 5), MinecraftVersion::new(1, 21, 4), MinecraftVersion::new(26, 3, 0)] {
            let mut spit = ProjectileState::for_kind_in_release(
                ProjectileKind::LlamaSpit,
                Vec3::new(0.5, 100.8, 0.5),
                Vec3::new(0.1, 0.0, 0.0),
                release,
            );
            assert!(tick_projectile_in_world(&mut spit, &world, release).discarded, "{release}");
        }
    }

    #[test]
    fn a_shulker_bullet_touches_water_by_its_own_box() {
        // sized(0.3125F, 0.3125F) (1.21.5 EntityType.java:699-701): x 0.85 reaches 1.00625, into the
        // water at x 1 that a 0.25 box would miss
        let mut world = loaded_world();
        flood(&mut world, 1..=1, Vec3::ZERO);
        let release = MinecraftVersion::new(1, 21, 4);
        let at = Vec3::new(0.85, 100.0, 0.5);
        let mut bullet =
            ProjectileState::for_kind_in_release(ProjectileKind::ShulkerBullet, at, Vec3::ZERO, release);
        let _ = tick_projectile_in_world(&mut bullet, &world, release);
        assert!(bullet.in_water);
    }

    #[test]
    fn a_projectile_discards_itself_64_blocks_below_the_world() {
        // Entity.checkBelowWorld, as literal -64.0 until 1.16.5 and the level's getMinY - 64 from 1.17
        // (1.16.5 Entity.java:529, 1.17 :461-463, 26.3 :626-629). The moving first releases read the
        // position after the move (1.21.3 ThrowableProjectile.java:58, 26.3 ThrowableProjectile.java:56); a
        // bullet's base tick opens its tick (26.3 ShulkerBullet.java:185). The tick runs to its end.
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 1000.0);
        let down = Vec3::new(0.0, -1.0, 0.0);
        for (release, bottom, moved_first) in [
            (MinecraftVersion::new(1, 8, 8), -64.0, false),
            (MinecraftVersion::new(1, 16, 5), -64.0, false),
            (MinecraftVersion::new(1, 17, 0), -128.0, false),
            (MinecraftVersion::new(1, 21, 1), -128.0, false),
            (MinecraftVersion::new(1, 21, 2), -128.0, true),
            (MinecraftVersion::new(26, 3, 0), -128.0, true),
        ] {
            let snowball_from = |y: f64| {
                let mut state = ProjectileState::for_kind_in_release(
                    ProjectileKind::Snowball,
                    Vec3::new(0.5, y, 0.5),
                    down,
                    release,
                );
                state.gravity = 0.0;
                state.air_drag = 1.0;
                state.first_tick = false;
                tick_projectile_in_world(&mut state, &world, release)
            };
            // half a block above the line: the move crosses it
            let crossing = snowball_from(bottom + 0.5);
            assert_eq!(crossing.discarded, moved_first, "{release:?}");
            assert_eq!(crossing.position.y, bottom - 0.5, "{release:?}");
            // exactly on it: the comparison is strict
            assert!(!snowball_from(bottom + 1.0).discarded, "{release:?}");
            // under it the tick still moves it
            let under = snowball_from(bottom - 0.5);
            assert!(under.discarded, "{release:?}");
            assert_eq!(under.position.y, bottom - 1.5, "{release:?}");

            let mut flying = arrow(Vec3::new(0.5, bottom + 0.5, 0.5), down, release);
            let result = tick_arrow_in_world(&mut flying, &world, release);
            assert_eq!(result.discarded, moved_first, "{release:?}");
            let mut bullet = ProjectileState::for_kind_in_release(
                ProjectileKind::ShulkerBullet,
                Vec3::new(0.5, bottom - 0.5, 0.5),
                Vec3::ZERO,
                release,
            );
            assert!(tick_projectile_in_world(&mut bullet, &world, release).discarded, "{release:?}");
        }
    }

    #[test]
    fn a_level_with_its_own_bottom_moves_the_void_from_1_17() {
        // 1.17's level height decides; a default world keeps the bottom of 0
        let mut nether = WorldSnapshot::new();
        nether.set_world_bounds(0, 256, 0.0, 0.0, 1000.0);
        let mut deep = WorldSnapshot::new();
        deep.set_world_bounds(-2032, 2032, 0.0, 0.0, 1000.0);
        let modern = MinecraftVersion::new(1, 20, 4);
        assert!(super::below_world(-64.5, &nether, modern));
        assert!(!super::below_world(-64.5, &deep, modern));
        assert!(super::below_world(-2096.5, &deep, modern));
        assert!(super::below_world(-64.5, &WorldSnapshot::new(), modern));
        assert!(super::below_world(-64.5, &deep, MinecraftVersion::new(1, 16, 5)));
    }

    #[test]
    fn nothing_solid_stands_at_the_limits_of_the_world() {
        // the limits are no walls: only cells collide, a move across a limit finds nothing in its
        // empty cells, and a block in the last cell still stops it
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 1000.0);
        let release = MinecraftVersion::new(26, 3, 0);
        for (from, by) in [
            (Vec3::new(0.5, -63.5, 0.5), Vec3::new(0.0, -2.0, 0.0)),
            (Vec3::new(0.5, 319.5, 0.5), Vec3::new(0.0, 2.0, 0.0)),
        ] {
            let (to, hit) = thrown_through(&world, from, by, release);
            assert_eq!((to, hit), (from + by, false));
        }
        world.set_block(
            (0, -64, 0),
            BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() },
        );
        assert!(thrown_through(&world, Vec3::new(0.5, -62.5, 0.5), Vec3::new(0.0, -2.0, 0.0), release).1);
    }

    // ---- B290: fixtures that hold each open gate of this file to both of its releases ----

    use crate::projectile_hits::{
        target_hitbox, ProjectileHitReport, ProjectileHitState, ProjectileOwner, ProjectileTarget,
        RocketHolder,
    };

    fn player_owner() -> ProjectileOwner {
        ProjectileOwner {
            id: 9,
            root_vehicle: 9,
            eye_position: Vec3::new(80.0, 101.62, 80.0),
            width: 0.6,
            alive: true,
            player: true,
            spectator: false,
            removed: false,
            won_game: false,
        }
    }

    /// Past every owner's immunity and margin.
    fn settled_hits() -> ProjectileHitState {
        ProjectileHitState {
            tick_count: 20,
            flying_ticks: 20,
            left_owner: true,
            ..ProjectileHitState::default()
        }
    }

    fn live(
        state: &mut ProjectileState,
        hits: &mut ProjectileHitState,
        targets: &[ProjectileTarget],
        owner: Option<ProjectileOwner>,
        world: &WorldSnapshot,
        release: MinecraftVersion,
    ) -> (super::ProjectileTickResult, ProjectileHitReport) {
        super::tick_projectile_with_targets(state, hits, targets, owner, world, release)
    }

    fn flying_kind(kind: ProjectileKind, position: Vec3, velocity: Vec3) -> ProjectileState {
        let mut state = ProjectileState::for_kind(kind, position, velocity);
        state.first_tick = false;
        state
    }

    #[test]
    fn the_border_bounce_rounds_twice_until_26_2_and_once_from_26_3() {
        // 26.2 ProjectileDeflection.java:14 scales by -0.5 and Projectile.java:215 by 0.2, two
        // roundings; 26.3 ProjectileDeflection.java:14 multiplies by -power * 0.5 once
        // (Projectile.java:264 deflects by 0.2). The two agree on a normal double; a subnormal
        // motion shows the first rounding
        let tiny = Vec3::new(5.0 * f64::from_bits(1), 0.0, 0.0);
        let before = super::border_bounce(tiny, release(26, 2, 0)).x;
        let after = super::border_bounce(tiny, release(26, 3, 0)).x;
        assert_eq!(before.to_bits(), (-0.0_f64).to_bits());
        assert_eq!(after, -f64::from_bits(1));
    }

    #[test]
    fn a_wind_charge_bursts_over_the_level_from_1_20_5_one_block_lower_from_1_21_2() {
        // 1.20.4 WindCharge.java has no such test; 1.20.5 AbstractWindCharge.java:127 and 1.21.1
        // :142 getBlockY() > getMaxBuildHeight() + 30; 1.21.2 :141 getMaxY() + 30, one less
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 6.0e7);
        let high = |y: f64, release| {
            let state = flying_kind(ProjectileKind::WindCharge, Vec3::new(0.5, y, 0.5), Vec3::ZERO);
            super::wind_charge_too_high(&state, &world, release)
        };
        assert!(!high(400.0, release(1, 20, 4)));
        assert!(high(400.0, release(1, 20, 5)));
        assert!(!high(350.5, release(1, 21, 1)));
        assert!(high(351.5, release(1, 21, 1)));
        assert!(high(350.5, release(1, 21, 2)));
        assert!(!high(349.5, release(1, 21, 2)));
    }

    #[test]
    fn a_lava_current_pushes_a_projectile_out_of_water_in_1_16_itself() {
        // 1.16 Entity.java:1036-1045 returns before lava's push only when in water; out of water
        // lava pushes at 0.0023333333333333335 as from 1.16.2; 1.15.2 has no lava current
        let mut world = loaded_world();
        for x in -1..=4 {
            for y in 99..=102 {
                for z in -1..=1 {
                    world.set_block((x, y, z), BlockState {
                        fluid: FluidState::Lava,
                        fluid_source: true,
                        fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                        ..BlockState::default()
                    });
                }
            }
        }
        let throw = |release| {
            let mut state = in_flight(Vec3::new(0.5, 100.0, 0.5), Vec3::new(0.5, 0.0, 0.0), release);
            state.air_drag = 1.0;
            tick_projectile_in_world(&mut state, &world, release);
            state.velocity.x
        };
        assert_eq!(throw(release(1, 16, 0)), 0.5 + 0.0023333333333333335);
        assert_eq!(throw(release(1, 15, 2)), 0.5);
    }

    #[test]
    fn an_arrow_falls_by_its_box_grown_0_05_until_1_13_and_0_06_in_1_14() {
        // 1.13.2 AbstractArrowEntity.java:188 expand(0.05) of the hit block's state; 1.14.4
        // AbstractArrow.java:186 inflate(0.06) at the arrow's block. A wall 0.055 from the box
        // stops the 1.14 fall alone
        let world = full_block_world((1, 100, 0));
        let fall = |release| {
            let mut state = ArrowState {
                in_ground: true,
                last_block_id: Some(99),
                last_block_position: Some((0, 100, 0)),
                ..ArrowState::new(Vec3::new(0.695, 100.5, 0.5), Vec3::ZERO)
            };
            tick_arrow_in_release(&mut state, &world, release).started_falling
        };
        assert!(fall(release(1, 13, 2)));
        assert!(!fall(release(1, 14, 4)));
    }

    #[test]
    fn an_arrow_in_water_hits_with_the_water_inertia_applied_from_1_21_4() {
        // 1.21.3 AbstractArrow.java:236 applies the inertia after stepMoveAndHit, which hurts at the
        // whole speed; 1.21.4 :210 applies the water's first. Mth.ceil(speed * 2.0)
        let world = WorldSnapshot::new();
        let position = Vec3::new(2.0, 0.0, 0.0);
        let villager = ProjectileTarget::new(1, position, target_hitbox(position, 1.0, 2.0, 1.0));
        let damage = |release| {
            let mut state = ArrowState {
                in_water: true,
                ..ArrowState::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0))
            };
            let mut hits = settled_hits();
            let (_, report) = super::tick_arrow_with_targets(
                &mut state, &mut hits, &[villager], None, &world, release,
            );
            report.entity_hits.first().and_then(|hit| hit.damage)
        };
        assert_eq!(damage(release(1, 21, 3)), Some(6));
        assert_eq!(damage(release(1, 21, 4)), Some(4));
    }

    #[test]
    fn a_bubble_column_under_a_torch_lifts_from_inside_until_1_21_4() {
        // 1.21.4 BubbleColumnBlock.java:51 asks isAir above; 1.21.5 :52 an empty collision shape
        // and no fluid, which a torch has: the surface's 0.1 instead of the inside's 0.06
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), bubble_column(false));
        world.set_block((0, 101, 0), BlockState { id: 77, ..BlockState::default() });
        let lift = |release| {
            let mut state =
                flying_kind(ProjectileKind::Snowball, Vec3::new(0.5, 100.2, 0.5), Vec3::ZERO);
            super::push_by_bubble_column(&mut state, &world, (0, 100, 0), release);
            state.velocity.y
        };
        assert_eq!(lift(release(1, 21, 4)), 0.06);
        assert_eq!(lift(release(1, 21, 5)), 0.1);
    }

    #[test]
    fn a_bubble_column_makes_a_projectiles_negative_zero_positive_from_1_21_5_but_not_a_pearls() {
        // 1.21.5 Projectile.java:172-182 pushes by Vec3.add(0, d, 0) (Vec3.java:104-106): -0 + 0 is +0;
        // 1.21.4 Entity.java onAboveBubbleCol and the pearl's Entity.handleOnAboveBubbleColumn (1.21.5
        // Entity.java:2536-2547) set x and z back as they were
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), bubble_column(false));
        let signs = |kind, release| {
            let mut state = flying_kind(kind, Vec3::new(0.5, 100.2, 0.5), Vec3::new(-0.0, 0.0, -0.0));
            super::push_by_bubble_column(&mut state, &world, (0, 100, 0), release);
            (state.velocity.x.is_sign_negative(), state.velocity.z.is_sign_negative())
        };
        assert_eq!(signs(ProjectileKind::Snowball, release(1, 21, 4)), (true, true));
        assert_eq!(signs(ProjectileKind::Snowball, release(1, 21, 5)), (false, false));
        assert_eq!(signs(ProjectileKind::EnderPearl, release(1, 21, 5)), (true, true));
    }

    #[test]
    fn a_rocket_follows_its_holder_from_1_11_and_is_boosted_at_an_angle_until_1_13() {
        // 1.11.2 FireworkRocketEntity.java:118-131 stands at the boosted entity (1.10.2 has none)
        let world = WorldSnapshot::new();
        let holder = RocketHolder {
            position: Vec3::new(10.0, 70.0, 20.0),
            velocity: Vec3::new(0.0, -0.1, 1.5),
            gliding: true,
            player: true,
            yaw_degrees: 0.0,
            left_hand: false,
        };
        let mut state =
            flying_kind(ProjectileKind::Firework, Vec3::new(0.0, 64.0, 0.0), Vec3::new(0.0, 0.5, 0.0));
        let mut hits = ProjectileHitState { attached: true, holder: Some(holder), ..settled_hits() };
        live(&mut state, &mut hits, &[], None, &world, release(1, 11, 2));
        assert_eq!(state.position, holder.position);
        assert_eq!(state.velocity, holder.velocity);
        // 1.13.2 FireworkRocketEntity tick boosts every free rocket; 1.14.4
        // FireworkRocketEntity.java:126 not one shot at an angle
        let shot = |release| {
            let mut state = flying_kind(
                ProjectileKind::Firework,
                Vec3::new(0.0, 64.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
            );
            state.shot_at_angle = true;
            tick_projectile_in_release(&mut state, &world, release).position
        };
        assert_eq!(shot(release(1, 13, 2)), Vec3::new(1.15, 64.04, 0.0));
        assert_eq!(shot(release(1, 14, 4)), Vec3::new(1.0, 64.0, 0.0));
    }

    #[test]
    fn a_rocket_bursts_on_a_block_ahead_from_1_16_and_only_on_a_collision_before() {
        // 1.15.2 FireworkRocketEntity.java:226-238 acts on a block only when the move collided;
        // 1.16 :163-165, 208-213 onHitBlock explodes at any block hit. The ray after the move meets
        // the wall the move did not reach
        let mut world = full_block_world((2, 100, 0));
        world.set_chunk_loaded(0, 0, true);
        let gone = |release| {
            let mut state = flying_kind(
                ProjectileKind::Firework,
                Vec3::new(0.5, 100.5, 0.5),
                Vec3::new(1.0, -0.04, 0.0),
            );
            let mut hits = ProjectileHitState { explodes: true, ..settled_hits() };
            let (result, _) = live(&mut state, &mut hits, &[], None, &world, release);
            assert!(!state.horizontal_collision, "{release}");
            result.discarded
        };
        assert!(!gone(release(1, 15, 2)));
        assert!(gone(release(1, 16, 0)));
    }

    #[test]
    fn a_hook_counts_its_life_in_the_block_until_1_15_and_on_the_ground_from_1_16() {
        // 1.15.2 FishingHook.java:143-149 counts life while inGround and never resets it; 1.16
        // FishingHook.java:156-164 counts it onGround and resets it in the air
        let world = loaded_world();
        let tick = |in_ground: bool, on_ground: bool, life: i32, release| {
            let mut state =
                flying_kind(ProjectileKind::FishingHook, Vec3::new(5.5, 100.0, 5.5), Vec3::ZERO);
            state.on_ground = on_ground;
            let mut hits = ProjectileHitState { in_ground, life, ..settled_hits() };
            let (result, _) = live(&mut state, &mut hits, &[], Some(player_owner()), &world, release);
            (result.discarded, hits.life)
        };
        let (old, new) = (release(1, 15, 2), release(1, 16, 0));
        assert!(tick(true, false, 1199, old).0);
        assert!(!tick(true, false, 1199, new).0);
        assert!(tick(false, true, 1199, new).0);
        assert!(!tick(false, true, 1199, old).0);
        assert_eq!(tick(false, false, 7, old), (false, 7));
        assert_eq!(tick(false, false, 7, new), (false, 0));
    }

    #[test]
    fn a_hooked_hook_rides_a_dead_target_until_1_21_8_and_keeps_no_box_from_1_17() {
        // 1.21.8 FishingHook.java:188 rides any hooked entity not removed; 1.21.9 :188 also needs
        // it alive and no spectator. setPos remakes the kept box until 1.16.5 (Entity.java:469-472);
        // from 1.17 the box is made from the position (1.17 Entity.java:367-374)
        let world = loaded_world();
        let at = Vec3::new(8.0, 100.0, 8.0);
        let target = |alive| ProjectileTarget {
            alive,
            ..ProjectileTarget::new(1, at, target_hitbox(at, 1.0, 2.0, 1.0))
        };
        let ride = |alive, release| {
            let mut state =
                flying_kind(ProjectileKind::FishingHook, Vec3::new(5.5, 100.0, 5.5), Vec3::ZERO);
            let mut hits =
                ProjectileHitState { hooked: Some(1), hook_settled: true, ..settled_hits() };
            live(&mut state, &mut hits, &[target(alive)], Some(player_owner()), &world, release);
            (state.position, hits.hooked, state.bounding_box.is_some())
        };
        let riding = Vec3::new(8.0, 100.0 + 2.0 * 0.8, 8.0);
        assert_eq!(ride(false, release(1, 21, 8)), (riding, Some(1), false));
        assert_eq!(ride(false, release(1, 21, 9)).1, None);
        assert_eq!(ride(true, release(1, 16, 5)), (riding, Some(1), true));
        assert_eq!(ride(true, release(1, 17, 0)), (riding, Some(1), false));
        // a flying hook's last setPos keeps a box until 1.16.5 (FishingHook.java:244), none from 1.17
        let fly = |release| {
            let mut state =
                flying_kind(ProjectileKind::FishingHook, Vec3::new(5.5, 100.0, 5.5), Vec3::ZERO);
            live(&mut state, &mut settled_hits(), &[], Some(player_owner()), &world, release);
            state.bounding_box.is_some()
        };
        assert!(fly(release(1, 16, 5)));
        assert!(!fly(release(1, 17, 0)));
    }

    #[test]
    fn a_hooks_fall_adds_a_vec3_from_1_14_which_turns_a_negative_zero_positive() {
        // 1.13.2 FishingHook.java:229-231 subtracts from motionY alone; 1.14.4
        // FishingHook.java:197-199 Vec3.add(0, -0.03, 0), and -0.0 + 0.0 is +0.0
        let world = loaded_world();
        let sign = |release| {
            let mut state = flying_kind(
                ProjectileKind::FishingHook,
                Vec3::new(5.5, 100.0, 5.5),
                Vec3::new(-0.0, 0.0, -0.0),
            );
            let _ = tick_projectile_in_release(&mut state, &world, release);
            (state.velocity.x.is_sign_negative(), state.velocity.z.is_sign_negative())
        };
        assert_eq!(sign(release(1, 13, 2)), (true, true));
        assert_eq!(sign(release(1, 14, 4)), (false, false));
    }

    #[test]
    fn llama_spit_finds_air_in_the_cell_its_box_tops_from_1_16() {
        // 1.15.2 LlamaSpit.java:104 Level.containsMaterial(AIR), floor(min) to ceil(max) exclusive
        // (Level.java:549-561); 1.16 LlamaSpit.java:62 getBlockStates, floor(min) to floor(max)
        // inclusive. A box whose top is y 100 reaches the air of y 100 from 1.16 alone
        let mut world = loaded_world();
        world.set_block((0, 99, 0), BlockState { id: 5, ..BlockState::default() });
        let gone = |release| {
            let mut state = flying_kind(
                ProjectileKind::LlamaSpit,
                Vec3::new(0.5, 99.75, 0.5),
                Vec3::new(0.1, 0.0, 0.0),
            );
            tick_projectile_in_release(&mut state, &world, release).discarded
        };
        assert!(gone(release(1, 15, 2)));
        assert!(!gone(release(1, 16, 0)));
    }

    /// A plant: an outline and no collision shape.
    fn grass() -> BlockState {
        BlockState {
            id: 31,
            outline_shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::new(0.1, 0.0, 0.1), Vec3::new(0.9, 0.8, 0.9)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    #[test]
    fn spit_and_fireballs_stop_on_a_plants_outline_until_their_release_clips_colliders() {
        // llama spit: 1.16.1 LlamaSpit.java:51 OUTLINE, 1.16.2 ProjectileUtil.java:64 COLLIDER;
        // fireballs: 1.8.8 AbstractHurtingProjectile.java:111 rayTrace of every block, 1.9.4
        // ProjectileUtil.java:22 skipping those with no collision box
        let mut world = loaded_world();
        world.set_block((1, 100, 0), grass());
        let stopped = |kind, release| {
            let mut state = flying_kind(kind, Vec3::new(0.5, 100.4, 0.5), Vec3::new(1.0, 0.0, 0.0));
            state.power = Vec3::ZERO;
            tick_projectile_in_release(&mut state, &world, release).block_hit
        };
        assert!(stopped(ProjectileKind::LlamaSpit, release(1, 16, 1)));
        assert!(!stopped(ProjectileKind::LlamaSpit, release(1, 16, 2)));
        assert!(stopped(ProjectileKind::SmallFireball, release(1, 8, 8)));
        assert!(!stopped(ProjectileKind::SmallFireball, release(1, 9, 4)));
    }

    #[test]
    fn a_shulker_bullet_moves_before_a_deflection_from_1_21_2() {
        // 1.21.1 ShulkerBullet.java:216-225 hits (a breeze turns it back by -0.5) and then moves;
        // 1.21.2 :217-229 moves by the motion first
        let world = WorldSnapshot::new();
        let at = Vec3::new(1.0, 0.0, 0.0);
        let breeze = ProjectileTarget {
            deflects: true,
            ..ProjectileTarget::new(1, at, target_hitbox(at, 1.0, 2.0, 1.0))
        };
        let end = |release| {
            let mut state = flying_kind(
                ProjectileKind::ShulkerBullet,
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
            );
            state.gravity = 0.0;
            let mut hits = settled_hits();
            live(&mut state, &mut hits, &[breeze], None, &world, release);
            (state.position.x, state.velocity.x)
        };
        assert_eq!(end(release(1, 21, 1)), (-0.5, -0.5));
        assert_eq!(end(release(1, 21, 2)), (1.0, -0.5));
    }

    #[test]
    fn a_level_move_leaves_a_clients_rocket_on_the_ground_from_1_21_4() {
        // 1.21.3 Entity.java:706 sets onGround after every move; 1.21.4 :707 only when the move has
        // a vertical part or is the local player's
        let world = WorldSnapshot::new();
        let grounded = |release| {
            let mut state = flying_kind(
                ProjectileKind::Firework,
                Vec3::new(0.0, 64.0, 0.0),
                Vec3::new(0.1, 0.0, 0.0),
            );
            state.shot_at_angle = true;
            state.on_ground = true;
            tick_projectile_in_release(&mut state, &world, release).on_ground
        };
        assert!(!grounded(release(1, 21, 3)));
        assert!(grounded(release(1, 21, 4)));
    }

    #[test]
    fn a_fireball_whose_box_is_in_a_block_hits_it_until_1_15_2() {
        // 1.15.2 ProjectileUtil.java:92-93 answers a box that meets a block with a hit at its own
        // position before any clip; 1.16 ProjectileUtil.java:59-64 clips at once, and the ray
        // leads away
        let mut world = full_block_world((1, 100, 0));
        world.set_chunk_loaded(0, 0, true);
        let gone = |release| {
            let mut state = flying_kind(
                ProjectileKind::SmallFireball,
                Vec3::new(0.9, 100.2, 0.5),
                Vec3::new(-0.5, 0.0, 0.0),
            );
            state.power = Vec3::ZERO;
            let (result, _) = live(&mut state, &mut settled_hits(), &[], None, &world, release);
            result.discarded
        };
        assert!(gone(release(1, 15, 2)));
        assert!(!gone(release(1, 16, 0)));
    }

    #[test]
    fn a_ray_clipped_from_the_moved_start_misses_a_last_sliver_until_1_13() {
        // 1.12.2 Level.java:863-980 clips each block from the start moved to its edge, and
        // AxisAlignedBB.calculateIntercept drops an axis whose part of what is left is under
        // 1.0E-7 squared: the top face, met after 0.94 of the ray, is lost. 1.13.2
        // Level.java:837-838 gives the block the whole segment
        let mut world = full_block_world((5, 0, 0));
        world.set_chunk_loaded(0, 0, true);
        let from = Vec3::new(0.5, 1.00096, 0.5);
        let movement = Vec3::new(5.1, -0.00102, 0.0);
        let hit = |release| {
            clip_blocks(&world, from, movement, release, ClipShape::Collider).map(|hit| hit.block)
        };
        assert_eq!(hit(release(1, 12, 2)), None);
        assert_eq!(hit(release(1, 13, 2)), Some((5, 0, 0)));
    }

    /// The engine's own wrappers of the bubble column push: sized_body is the box a projectile of a size is inside
    /// for checkInsideBlocks, push_motion_by_bubble_column applies both pushes of one cell to a motion, and
    /// push_by_bubble_columns_along_where takes each cell a move went through (helpers-close-c.json).
    #[test]
    fn the_bubble_column_wrappers_push_a_motion_by_the_cells_it_meets() {
        let body = super::sized_body(Vec3::new(0.5, 1.0, 0.5), 0.25);
        assert_eq!((body.half_width, body.height), (0.125, 0.25));
        assert_eq!(body.bounds(), Aabb::new(Vec3::new(0.375, 1.0, 0.375), Vec3::new(0.625, 1.25, 0.625)));

        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(false));
        let mut motion = Vec3::ZERO;
        // a column with air above: the surface push, +0.1 up to 1.8 before 1.21.5
        super::push_motion_by_bubble_column(&mut motion, false, &world, (0, 0, 0), release(1, 21, 4));
        assert_eq!(motion.y, 0.1);
        let mut down = Vec3::new(0.0, -0.89, 0.0);
        let mut sinking = WorldSnapshot::new();
        sinking.set_block((0, 0, 0), bubble_column(true));
        super::push_motion_by_bubble_column(&mut down, false, &sinking, (0, 0, 0), release(1, 21, 4));
        assert_eq!(down.y, -0.9);
        // no column, no push
        let mut still = Vec3::ZERO;
        super::push_motion_by_bubble_column(&mut still, false, &world, (5, 5, 5), release(1, 21, 4));
        assert_eq!(still, Vec3::ZERO);

        // a move through the column's cell is pushed once by it; one beside it is not
        let mut through = Vec3::ZERO;
        super::push_by_bubble_columns_along_where(
            &mut through, Vec3::new(0.5, 0.2, -1.5), Vec3::new(0.5, 0.2, 0.5), 0.25, &world, release(1, 21, 4),
            super::BubblePushes::Both,
        );
        assert_eq!(through.y, 0.1);
        let mut beside = Vec3::ZERO;
        super::push_by_bubble_columns_along_where(
            &mut beside, Vec3::new(3.5, 0.2, -1.5), Vec3::new(3.5, 0.2, 0.5), 0.25, &world, release(1, 21, 4),
            super::BubblePushes::Both,
        );
        assert_eq!(beside, Vec3::ZERO);
    }

    #[test]
    fn a_no_physics_arrow_in_a_bubble_column_is_pushed_until_1_21_1() {
        // 1.21.1 AbstractArrow.tick ends a noPhysics flight with checkInsideBlocks (AbstractArrow.java:277-278),
        // which reaches the column; 1.21.2 calls applyEffectsFromBlocks there, and isAffectedByBlocks is
        // false under noPhysics (1.21.2 AbstractArrow.java:233-234, Entity.java:812-814)
        let world = bubble_up_column(9);
        let rise = |version| {
            let mut state = ArrowState { no_physics: true, ..arrow(Vec3::new(0.5, 5.0, 0.5), Vec3::ZERO, version) };
            tick_arrow_in_release(&mut state, &world, version).velocity.y
        };
        assert_eq!(rise(release(1, 21, 1)), 0.06);
        assert_eq!(rise(release(1, 21, 2)), 0.0);
    }

    #[test]
    fn soul_sand_slows_a_projectile_inside_it_until_1_14_4_and_honey_slides_one_from_1_15() {
        // SoulSandBlock.entityInside multiplies x and z by 0.4 until 1.14.4 (1.14 SoulSandBlock.java:26);
        // 1.15 has none and adds HoneyBlock.entityInside's side slide (1.15 HoneyBlock.java:68-75), a
        // block 1.14.4 has not
        let cell_world = |effect| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 0, 0), BlockState {
                shape: Some(BlockShape { effect, ..BlockShape::full_block() }),
                ..BlockState::default()
            });
            world
        };
        let soul_sand = cell_world(BlockEffect::SoulSand);
        let slowed = |version| {
            let mut velocity = Vec3::new(0.5, -0.1, 0.5);
            super::apply_projectile_cells(&mut velocity, Vec3::new(0.5, 0.5, 0.5), 0.25, false, &[((0, 0, 0), true)], &soul_sand, version, super::BubblePushes::Both, false);
            velocity
        };
        assert_eq!(slowed(release(1, 14, 4)), Vec3::new(0.5 * 0.4, -0.1, 0.5 * 0.4));
        assert_eq!(slowed(release(1, 15, 0)), Vec3::new(0.5, -0.1, 0.5));
        // a projectile falling beside the honey's side, its centre past 0.4375 + 0.125 from the block's
        let honey = cell_world(BlockEffect::Honey);
        let slid = |version| {
            let mut velocity = Vec3::new(0.1, -0.2, 0.0);
            super::apply_projectile_cells(&mut velocity, Vec3::new(-0.08, 0.5, 0.5), 0.25, false, &[((0, 0, 0), true)], &honey, version, super::BubblePushes::Both, false);
            velocity
        };
        assert_eq!(slid(release(1, 14, 4)), Vec3::new(0.1, -0.2, 0.0));
        assert_eq!(slid(release(1, 15, 0)).y, -0.05);
    }

    #[test]
    fn a_fishing_hook_in_a_bubble_column_is_pushed_after_its_move_on_1_21_2() {
        // 1.21.2 FishingHook's tick applies the blocks after its move (Entity.move checks none from
        // 1.21.2: 1.21.1 Entity.java:738 does); on 1.21.2 itself the column's push still comes
        let world = bubble_up_column(9);
        let mut hook = thrown(ProjectileKind::FishingHook, Vec3::new(0.5, 5.0, 0.5), Vec3::ZERO);
        super::move_projectile(&mut hook, &world, release(1, 21, 2), false);
        assert_eq!(hook.velocity.y, 0.06);
    }
}
