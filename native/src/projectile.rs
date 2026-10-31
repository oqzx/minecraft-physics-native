use crate::block_traversal::{blocks_visited, Movement};
use crate::math::{java_float, Aabb, Vec3};
use crate::simulation::{blocks_inside, SimulationState};
use crate::vehicle::{is_water, move_vehicle, water_height, MoveRules};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, FluidState, WorldSnapshot};

/// The release the unversioned projectile ticks follow.
const UNVERSIONED_RELEASE: MinecraftVersion = MinecraftVersion::new(26, 2, 0);

/// EntityType's 0.25F x 0.25F box of the thrown items, the fishing hook, llama spit and the
/// firework rocket.
const PROJECTILE_SIZE: f32 = 0.25;

/// What Entity.move collides a fishing hook or a firework rocket with: no step, and from 26.2
/// Entity's restitution (no gravity, 0.98F air drag, no bounciness).
const PROJECTILE_MOVE: MoveRules = MoveRules {
    living: false,
    max_step: 0.0,
    gravity: 0.0,
    air_drag: 0.98_f32 as f64,
    bounciness: 0.0,
    stands_on_lava: false,
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
    pub first_tick: bool,
    pub kind: ProjectileKind,
    pub horizontal_collision: bool,
    pub on_ground: bool,
    pub shot_at_angle: bool,
    /// A fishing hook in FishingHook's BOBBING state (FLYING otherwise).
    pub bobbing: bool,
    /// The fishing hook's random.nextFloat() for this tick's bobbing pull.
    pub bob_random: f32,
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
            first_tick: true,
            kind,
            horizontal_collision: false,
            on_ground: false,
            shot_at_angle: false,
            bobbing: false,
            bob_random: 0.0,
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
    /// (ThrownEntity.java:96 in 1.20.4, :93 in 1.20.5); the fishing hook and the shulker bullet
    /// subtract a double literal in every release.
    #[must_use]
    pub fn default_gravity_in(self, release: MinecraftVersion) -> f64 {
        let gravity = self.default_gravity();
        if release.at_least(1, 20, 5) || matches!(self, Self::FishingHook | Self::ShulkerBullet) {
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
    /// (1.21 AbstractWindChargeEntity.java:35-46).
    #[must_use]
    pub const fn default_acceleration_power(self) -> f64 {
        match self {
            Self::WindCharge => 0.0,
            _ => 0.1,
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
            // splash_potion and lingering_potion from 1.21.5 (1.21.5 EntityType.java:759, 767)
            "potion" | "thrown_potion" | "thrownpotion" | "splash_potion" | "lingering_potion" => {
                Self::Potion
            }
            // 1.8.9 EntityList ThrownExpBottle (EntityList.java:240)
            "experience_bottle" | "experiencebottle" | "xp_bottle" | "xpbottle"
            | "exp_bottle" | "expbottle" | "thrown_experience" | "thrownexperience"
            | "thrownexpbottle" | "experience_bottle_entity" => Self::ExperienceBottle,
            // fireworks_rocket in 1.11-1.12 (1.12.2 EntityType.java:210), 1.8.9
            // FireworksRocketEntity (EntityList.java:245)
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
            // wind_charge from 1.20.3, breeze_wind_charge from 1.20.5 (1.20.5 EntityType.java:247, 928)
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
}

impl ArrowState {
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
        }
    }

    /// `new` with the gravity of `release`: AbstractArrow's 0.05F before 1.20.5 (1.20.4
    /// PersistentProjectileEntity.java:244), 0.05 from 1.20.5 (1.20.5 :265-266).
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
}

impl TridentState {
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
        }
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
}

/// AbstractArrow.tick for a ThrownTrident as the unversioned release (26.2) runs it.
#[must_use]
pub fn tick_trident(state: &mut TridentState, world: &WorldSnapshot) -> TridentTickResult {
    tick_trident_in_release(state, world, UNVERSIONED_RELEASE)
}

/// AbstractArrow.tick for a ThrownTrident of `release` (1.13+): the flight of `arrow_flight` with
/// the trident's 0.99F water inertia (1.13.2 TridentEntity.java:193, 26.2 ThrownTrident.java:222).
#[must_use]
pub fn tick_trident_in_release(
    state: &mut TridentState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> TridentTickResult {
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
    );
    TridentTickResult {
        position: state.position,
        velocity: state.velocity,
        block_hit: hit.is_some(),
    }
}

/// AbstractArrow.tick as the unversioned release (26.2) runs it.
#[must_use]
pub fn tick_arrow(state: &mut ArrowState, world: &WorldSnapshot) -> ArrowTickResult {
    tick_arrow_in_release(state, world, UNVERSIONED_RELEASE)
}

/// AbstractArrow.tick of `release` (1.21.11 AbstractArrow.java:160-250, 1.21
/// PersistentProjectileEntity.java:153-280, 1.15 ProjectileEntity.java, 1.13.2
/// AbstractArrowEntity.java, 1.9.4 AbstractArrowEntity.java:165-195, 1.8.9 EntityArrow.java:145-357). shakeTime counts down at the top of every tick. An arrow in the
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
    state.shake_time = state.shake_time.saturating_sub(1);
    if state.in_ground {
        let arrow_block = (
            state.position.x.floor() as i32,
            state.position.y.floor() as i32,
            state.position.z.floor() as i32,
        );
        let read_block = if release.at_least(1, 14, 0) {
            arrow_block
        } else {
            state.last_block_position.unwrap_or(arrow_block)
        };
        let changed = state.last_block_id != Some(world.block(read_block).id);
        let falls = changed
            && if !release.at_least(1, 11, 0) {
                true
            } else if !release.at_least(1, 14, 0) {
                no_block_collision(
                    world,
                    arrow_box(state.position).inflate(Vec3::new(0.05, 0.05, 0.05)),
                )
            } else if !release.at_least(1, 16, 0) {
                no_block_collision(
                    world,
                    arrow_box(state.position).inflate(Vec3::new(0.06, 0.06, 0.06)),
                )
            } else {
                no_block_collision(
                    world,
                    Aabb::new(state.position, state.position).inflate(Vec3::new(0.06, 0.06, 0.06)),
                )
            };
        if falls {
            state.in_ground = false;
            state.velocity = state.velocity.multiply(state.fall_impulse);
            state.life = 0;
        } else {
            state.life = state.life.saturating_add(1);
        }
        state.in_ground_time = state.in_ground_time.saturating_add(1);
        return ArrowTickResult {
            position: state.position,
            velocity: state.velocity,
            block_hit: false,
            in_ground: state.in_ground,
            discarded: !falls && state.life >= 1200,
            in_ground_time: state.in_ground_time,
            shake_time: state.shake_time,
            started_falling: falls,
        };
    }

    state.in_ground_time = 0;
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
    );
    if let Some(hit) = hit {
        state.in_ground = true;
        state.shake_time = 7;
        state.last_block_id = Some(world.block(hit.block).id);
        state.last_block_position = Some(hit.block);
    }
    ArrowTickResult {
        position: state.position,
        velocity: state.velocity,
        block_hit: hit.is_some(),
        in_ground: state.in_ground,
        discarded: false,
        in_ground_time: state.in_ground_time,
        shake_time: state.shake_time,
        started_falling: false,
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

/// The flight of an AbstractArrow tick, returning its block hit.
///
/// From 1.21.2 (1.21.11 AbstractArrow.java:205-247, onHitBlock :497-516; 1.21.3
/// PersistentProjectileEntity.java tick, onBlockHit :466-471): in water the water inertia first
/// (from 1.21.4; 1.21.2-1.21.3 apply it after the move, which comes to the same), then the clip and
/// the move by the motion from the start of the tick; a hit moves the arrow to the hit, back by
/// 0.05F against the sign of each axis of its motion, and stops it; otherwise the air inertia out
/// of water and the gravity.
///
/// Before (1.21 PersistentProjectileEntity.java:198-277, onBlockHit :431-449): a hit sets the
/// motion to the hit's offset and moves the arrow back by 0.05F along it (from 1.14 by
/// Vec3.normalize; until 1.13.2 the offset is cast to float, 1.13.2 divides by MathHelper.sqrt *
/// 20.0F, 1.8.9-1.12.2 divide by MathHelper.sqrt and multiply by 0.05F: 1.13.2
/// AbstractArrowEntity.java:324-330, 1.12.2 :370-376, 1.8.9 EntityArrow.java:292-297); then the
/// arrow moves by that motion, which takes the water or the air inertia and the gravity, also on
/// the hit tick.
fn arrow_flight(
    position: &mut Vec3,
    velocity: &mut Vec3,
    motion: ArrowMotion,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> Option<BlockHit> {
    let start = *position;
    if release.at_least(1, 21, 2) {
        let movement = *velocity;
        if motion.in_water {
            *velocity = velocity.scale(motion.water_inertia);
        }
        if let Some(hit) = clip_blocks(world, start, movement, release, ClipShape::Collider) {
            let offset_direction = Vec3::new(
                java_signum(velocity.x),
                java_signum(velocity.y),
                java_signum(velocity.z),
            );
            *position = hit.location - offset_direction.scale(f64::from(0.05_f32));
            *velocity = Vec3::ZERO;
            return Some(hit);
        }
        *position = start + movement;
        if !motion.in_water {
            *velocity = velocity.scale(motion.air_drag);
        }
        velocity.y -= motion.gravity;
        return None;
    }

    let hit = clip_blocks(world, start, *velocity, release, ClipShape::Collider);
    if let Some(hit) = hit {
        if release.at_least(1, 14, 0) {
            let offset = hit.location - start;
            *velocity = offset;
            let back = vanilla_normalize(offset, release).scale(f64::from(0.05_f32));
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
        }
    }
    *position = *position + *velocity;
    let inertia = if motion.in_water {
        motion.water_inertia
    } else {
        motion.air_drag
    };
    *velocity = velocity.scale(inertia);
    velocity.y -= motion.gravity;
    hit
}

/// EntityType.ARROW's 0.5F x 0.5F box, which the in-ground test inflates until 1.15 (1.15
/// ProjectileEntity tick, 1.13.2 AbstractArrowEntity tick).
fn arrow_box(position: Vec3) -> Aabb {
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
/// ShulkerBullet's as the client runs them.
#[must_use]
pub fn tick_projectile_in_release(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let result = match state.kind {
        kind if kind.is_hurting() => tick_hurting_projectile(state, world, release),
        ProjectileKind::Firework => tick_firework(state, world, release),
        ProjectileKind::FishingHook => tick_fishing_hook(state, world, release),
        ProjectileKind::LlamaSpit => tick_llama_spit(state, world, release),
        ProjectileKind::ShulkerBullet => tick_shulker_bullet(state),
        _ => tick_throwable(state, world, release),
    };
    state.first_tick = false;
    result
}

/// ThrowableProjectile.tick. From 1.21.2 (1.21.2 ThrowableProjectile.java:43-62): the first
/// tick's bubble columns, gravity, inertia, then the hit test and the move by the new motion,
/// stopping at the hit, and the bubble columns along that move (applyEffectsFromBlocks). Before
/// (1.21.1 :39-66, 1.20.4 ThrownEntity.java:66-95, 1.8.9 EntityThrowable.java:188-230): the hit
/// test, from 1.16.2 the bubble columns of the box (checkBlockCollision/checkInsideBlocks, 1.16.5
/// ThrownEntity.java:71, 1.21.1 ThrowableProjectile.java:45; not in 1.16.1's), and the move by
/// the motion they leave, the full move even on a hit, then inertia and gravity.
fn tick_throwable(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let inertia = if state.in_water {
        f64::from(0.8_f32)
    } else {
        state.air_drag
    };
    if release.at_least(1, 21, 2) {
        if state.first_tick {
            handle_first_tick_bubble_column(state, world, release);
        }
        state.velocity.y -= state.gravity;
        state.velocity = state.velocity.scale(inertia);
        let start = state.position;
        let hit = nearest_block_hit(world, state.position, state.velocity, release);
        state.position = hit.unwrap_or(state.position + state.velocity);
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
        for (cell, precise) in inside {
            if precise {
                push_by_bubble_column(state, world, cell, release);
            }
        }
        return ProjectileTickResult::of(state, hit.is_some(), false);
    }
    let hit = nearest_block_hit(world, state.position, state.velocity, release);
    if release.at_least(1, 16, 2) {
        for cell in blocks_inside(&projectile_body(state.position), release) {
            push_by_bubble_column(state, world, cell, release);
        }
    }
    let movement = state.velocity;
    state.position = state.position + movement;
    state.velocity = movement.scale(inertia);
    state.velocity.y -= state.gravity;
    ProjectileTickResult::of(state, hit.is_some(), false)
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
        bounds.min.x.floor() as i32,
        bounds.min.y.floor() as i32,
        bounds.min.z.floor() as i32,
    );
    let max = (
        bounds.max.x.floor() as i32,
        bounds.max.y.floor() as i32,
        bounds.max.z.floor() as i32,
    );
    let mut cells = Vec::new();
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
/// onBubbleColumnSurfaceCollision/onBubbleColumnCollision (1.21.4 Entity.java:2517-2540); from
/// 1.21.5 Projectile's unclamped onAboveBubbleColumn and onInsideBubbleColumn (1.21.5
/// Projectile.java:172-182), except the ender pearl's, which call Entity's clamped
/// handleOnAboveBubbleColumn/handleOnInsideBubbleColumn (1.21.5 ThrownEnderpearl.java:246-252,
/// Entity.java:2536-2547, 2563-2574; 26.3 ThrownEnderpearl.java:244-250).
fn push_by_bubble_column(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    (x, y, z): (i32, i32, i32),
    release: MinecraftVersion,
) {
    let drag_down = match world.effect_at((x, y, z)) {
        BlockEffect::BubbleDown => true,
        BlockEffect::BubbleUp => false,
        _ => return,
    };
    let above = (x, y + 1, z);
    let nothing_above = if release.at_least(1, 21, 5) {
        !world.has_collision_shape(above) && world.fluid_at(above) == FluidState::Empty
    } else {
        world.is_air_at(above)
    };
    let unclamped = release.at_least(1, 21, 5) && state.kind != ProjectileKind::EnderPearl;
    let velocity_y = state.velocity.y;
    state.velocity.y = match (unclamped, nothing_above, drag_down) {
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
/// the motion's direction (1.21 ExplosiveProjectileEntity.java:99), before it the xPower vector
/// (1.20.6 :100), and scales by 0.8F in water or the air inertia (0.95F, WitherSkull's dangerous
/// 0.73F). A wind charge's liquid inertia is its 1.0F air inertia, and in 1.20.3-1.20.4 it clips
/// the outline shapes (1.21.11 AbstractWindCharge.java:128-135, 1.20.4 WindChargeEntity.java:112-130).
fn tick_hurting_projectile(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let wind_charge = state.kind == ProjectileKind::WindCharge;
    let inertia = if state.in_water && !wind_charge {
        f64::from(0.8_f32)
    } else {
        state.air_drag
    };
    let clip_shape = if wind_charge && !release.at_least(1, 20, 5) {
        ClipShape::Outline
    } else {
        ClipShape::Collider
    };
    let clip = |state: &ProjectileState| {
        clip_blocks(world, state.position, state.velocity, release, clip_shape)
            .map(|hit| hit.location)
    };
    let accelerated = |state: &ProjectileState| {
        let push = if release.at_least(1, 21, 0) {
            vanilla_normalize(state.velocity, release).scale(state.acceleration_power)
        } else {
            state.power
        };
        (state.velocity + push).scale(inertia)
    };
    if release.at_least(1, 21, 2) {
        state.velocity = accelerated(state);
        let hit = clip(state);
        state.position = hit.unwrap_or(state.position + state.velocity);
        return ProjectileTickResult::of(state, hit.is_some(), false);
    }
    let hit = clip(state);
    state.position = state.position + state.velocity;
    state.velocity = accelerated(state);
    ProjectileTickResult::of(state, hit.is_some(), false)
}

/// FireworkRocketEntity.tick for a rocket attached to no one: the boost (from 1.14 not for a
/// rocket shot at an angle, from 1.16.2 none along a horizontal collision), then Entity.move with
/// its collisions; from 1.16 the motion is put back as it was before the move (1.21.11
/// FireworkRocketEntity.java:139-149, 1.16 :29-35, 1.14 FireworkEntity, 1.8.9). The hit test
/// runs before the move from 1.21.2 and after it from 1.14, and moves nothing.
fn tick_firework(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    if !(state.shot_at_angle && release.at_least(1, 14, 0)) {
        let acceleration = if state.horizontal_collision && release.at_least(1, 16, 2) {
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
    let mut hit = if release.at_least(1, 21, 2) {
        nearest_block_hit(world, state.position, movement, release)
    } else {
        None
    };
    move_projectile(state, world, release);
    if release.at_least(1, 16, 0) {
        state.velocity = movement;
    }
    if release.at_least(1, 14, 0) && !release.at_least(1, 21, 2) {
        hit = nearest_block_hit(world, state.position, state.velocity, release);
    }
    ProjectileTickResult::of(state, hit.is_some(), false)
}

/// FishingHook.tick on the client for a hook in no entity (FLYING or BOBBING), from 1.11. A
/// flying hook that reaches water (FluidState.getHeight above 0 where it is) slows to
/// (0.3, 0.2, 0.3) and starts bobbing without moving. Otherwise: from 1.16 the hit test, whose
/// block hit sets the motion along its direction by the squared distance to the hit
/// (onHitBlock); before 1.16 a grounded or collided flying hook stops; a bobbing hook is pulled
/// towards the surface by `bob_random` (in 1.13 the water's own height, FluidState.getHeight
/// with no world until 1.14); -0.03 out of water (from 1.21.6 only off the ground, 1.21.6
/// FishingHook.java:228, 1.21.5 :218); Entity.move; from 1.16 a grounded or collided flying
/// hook stops; then 0.92 (1.21.11 FishingHook.java:149-247, 1.16.5 FishingBobberEntity.java,
/// 1.12.2 FishingBobberEntity.java).
/// The bite's pull uses the server-synced random and is left out.
fn tick_fishing_hook(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let block = (
        state.position.x.floor() as i32,
        state.position.y.floor() as i32,
        state.position.z.floor() as i32,
    );
    let in_water = is_water(world.fluid_at(block));
    let height = if in_water && release.at_least(1, 13, 0) && !release.at_least(1, 14, 0) {
        own_water_height(world, block)
    } else if in_water {
        water_height(world, block, release)
    } else {
        0.0
    };
    let modern = release.at_least(1, 16, 0);
    let mut hit = None;
    if !state.bobbing {
        if height > 0.0 {
            state.velocity = state.velocity.multiply(Vec3::new(0.3, 0.2, 0.3));
            state.bobbing = true;
            return ProjectileTickResult::of(state, false, false);
        }
        if modern {
            hit = nearest_block_hit(world, state.position, state.velocity, release);
            if let Some(hit) = hit {
                state.velocity = vanilla_normalize(state.velocity, release)
                    .scale(hit.distance_squared(state.position));
            }
        } else if state.on_ground || state.horizontal_collision {
            state.velocity = Vec3::ZERO;
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
    if !in_water && !(release.at_least(1, 21, 6) && state.on_ground) {
        state.velocity.y -= state.gravity;
    }
    move_projectile(state, world, release);
    if modern && !state.bobbing && (state.on_ground || state.horizontal_collision) {
        state.velocity = Vec3::ZERO;
    }
    state.velocity = state.velocity.scale(state.air_drag);
    ProjectileTickResult::of(state, hit.is_some(), false)
}

/// LlamaSpit.tick (1.11+): the hit test and the move take the old motion; a spit in water (from
/// 1.13 until 1.21.4 also one whose block is a bubble column, isInsideWaterOrBubbleColumn), or
/// whose box has no air in it, is removed; otherwise 0.99F and gravity (1.21.11
/// LlamaSpit.java:36-54, 1.21.4 LlamaSpitEntity.java:54, 1.13.2 :121, 1.11.2 LlamaSpitEntity).
/// Before 1.15 the position is advanced before the removal (1.14 :79, 1.15 :79-81 only reads
/// it), and until 1.16 the box's cells run floor..ceil (World.containsMaterial); from 1.16
/// floor..=floor (BlockPos.betweenClosed).
fn tick_llama_spit(
    state: &mut ProjectileState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> ProjectileTickResult {
    let movement = state.velocity;
    let hit = nearest_block_hit(world, state.position, movement, release);
    let target = state.position + movement;
    let bounds = projectile_box(state.position);
    let modern = release.at_least(1, 16, 0);
    if !release.at_least(1, 15, 0) {
        state.position = target;
    }
    let cells = |min: f64, max: f64| {
        let low = min.floor() as i32;
        if modern {
            low..max.floor() as i32 + 1
        } else {
            low..max.ceil() as i32
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
    let in_bubble_column = release.at_least(1, 13, 0)
        && !release.at_least(1, 21, 5)
        && matches!(
            world.effect_at((
                state.position.x.floor() as i32,
                state.position.y.floor() as i32,
                state.position.z.floor() as i32,
            )),
            BlockEffect::BubbleUp | BlockEffect::BubbleDown
        );
    if !has_air || state.in_water || in_bubble_column {
        return ProjectileTickResult::of(state, hit.is_some(), true);
    }
    state.velocity = movement.scale(state.air_drag);
    state.velocity.y -= state.gravity;
    state.position = target;
    ProjectileTickResult::of(state, hit.is_some(), false)
}

/// ShulkerBullet.tick on the client: the steering, gravity and hit test run on the server only;
/// the bullet moves by its motion (1.21.11 ShulkerBullet.java:184-207, 1.9.4 ShulkerBulletEntity).
fn tick_shulker_bullet(state: &mut ProjectileState) -> ProjectileTickResult {
    state.position = state.position + state.velocity;
    ProjectileTickResult::of(state, false, false)
}

/// Entity.move(SELF) of a 0.25-wide projectile by its motion, with the collision flags it leaves.
/// From 1.21.4 a move with no vertical part leaves onGround as it was on the client, where no
/// one controls the projectile (1.21.4 Entity.java:707, 1.21.5 :729).
fn move_projectile(state: &mut ProjectileState, world: &WorldSnapshot, release: MinecraftVersion) {
    let keeps_on_ground = release.at_least(1, 21, 4) && state.velocity.y == 0.0;
    let on_ground = state.on_ground;
    let mut body = SimulationState {
        position: state.position,
        velocity: state.velocity,
        half_width: f64::from(PROJECTILE_SIZE / 2.0),
        height: f64::from(PROJECTILE_SIZE),
        on_ground: state.on_ground,
        collided_horizontally: state.horizontal_collision,
        controlled_by_local_instance: true,
        bounding_box: (!release.at_least(1, 17, 0)).then(|| projectile_box(state.position)),
        ..SimulationState::default()
    };
    let movement = body.velocity;
    let _ = move_vehicle(
        &mut body,
        world,
        release,
        movement,
        PROJECTILE_MOVE,
        |_, _| {},
    );
    state.position = body.position;
    state.velocity = body.velocity;
    state.on_ground = if keeps_on_ground {
        on_ground
    } else {
        body.on_ground
    };
    state.horizontal_collision = body.collided_horizontally;
}

/// FluidState.getHeight in 1.13 (method_17810): the water's own amount over 9, with no look at
/// the block above (1.13.2 FishingBobberEntity.java:161, FlowableFluid.java:434).
fn own_water_height(world: &WorldSnapshot, position: (i32, i32, i32)) -> f32 {
    let block = world.block(position);
    let amount =
        if block.fluid_source || block.fluid_falling || !(1..8).contains(&block.fluid_level) {
            8
        } else {
            8 - block.fluid_level
        };
    f32::from(amount) / 9.0
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
/// player (1.19 RaycastContext.java:72, 1.21.9 ClipContext.java:59-73), and the water's fluid shape
/// counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipShape {
    Collider,
    Outline,
    FallDamageResetting { player: bool },
}

/// A block hit of a projectile's clip: where, and the block whose shape it hit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BlockHit {
    location: Vec3,
    block: (i32, i32, i32),
}

/// The block collider the segment from `start` along `movement` meets first.
fn nearest_block_hit(
    world: &WorldSnapshot,
    start: Vec3,
    movement: Vec3,
    release: MinecraftVersion,
) -> Option<Vec3> {
    clip_blocks(world, start, movement, release, ClipShape::Collider).map(|hit| hit.location)
}

/// BlockGetter.clip with ClipContext.Fluid.NONE: BlockGetter.traverseBlocks visits the cells along
/// the segment, from the start nudged back by 1.0E-7 of the segment, and the first cell whose shape
/// the segment hits gives the hit (26.2 BlockGetter.java:63-80, :110-163; 1.14 BlockView).
fn clip_blocks(
    world: &WorldSnapshot,
    from: Vec3,
    movement: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> Option<BlockHit> {
    clip_blocks_between(world, from, from + movement, release, shape)
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
        start.x.floor() as i32,
        start.y.floor() as i32,
        start.z.floor() as i32,
    );
    if let Some(location) = clip_block(world, block, from, to, release, shape) {
        return Some(BlockHit { location, block });
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
    let frac = |value: f64| value - value.floor();
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
        if let Some(location) = clip_block(world, block, from, to, release, shape) {
            return Some(BlockHit { location, block });
        }
    }
    None
}

/// VoxelShape.clip of the block at `block` (26.2 VoxelShape.java:119-131, 1.13.2 rayTrace:150-165):
/// from 1.13 a segment shorter than sqrt(1.0E-7) hits nothing, and one whose point 0.001 of the
/// way along is inside the shape hits there; otherwise AABB.clip, the nearest entering face of the
/// boxes. An unloaded chunk is air on the client.
fn clip_block(
    world: &WorldSnapshot,
    block: (i32, i32, i32),
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
    shape: ClipShape,
) -> Option<Vec3> {
    if !world.is_chunk_loaded(block.0.div_euclid(16), block.2.div_euclid(16)) {
        return None;
    }
    let boxes = match shape {
        ClipShape::Collider => world.block_collision_boxes(block),
        ClipShape::Outline => world.block_outline_boxes(block),
        ClipShape::FallDamageResetting { player } => {
            fall_damage_resetting_boxes(world, block, player, release)
        }
    };
    if boxes.is_empty() {
        return None;
    }
    if release.at_least(1, 13, 0) {
        let delta = to - from;
        if delta.length_squared() < 1.0e-7 {
            return None;
        }
        let inside = from + delta.scale(0.001);
        if boxes.iter().any(|bounds| bounds.contains(inside)) {
            return Some(inside);
        }
    }
    let mut nearest = None;
    for bounds in boxes {
        if let Some(hit) = bounds.clip_segment(from, to) {
            let distance = from.distance_squared(hit);
            if nearest.map_or(true, |(best, _)| distance < best) {
                nearest = Some((distance, hit));
            }
        }
    }
    nearest.map(|(_, hit)| hit)
}

/// The shapes FALLDAMAGE_RESETTING and Fluid.WATER give the cell: a full cube for a resetting
/// block, and the water's FluidState.getShape, a box up to its height (BlockGetter.clip takes the
/// nearer of the block and fluid hits, so either one hits).
fn fall_damage_resetting_boxes(
    world: &WorldSnapshot,
    block: (i32, i32, i32),
    player: bool,
    release: MinecraftVersion,
) -> Vec<Aabb> {
    let state = world.block(block);
    let origin = Vec3::new(f64::from(block.0), f64::from(block.1), f64::from(block.2));
    let mut boxes = Vec::new();
    if state.fall_damage_resetting
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
fn no_block_collision(world: &WorldSnapshot, query: Aabb) -> bool {
    !world
        .collision_shapes(query)
        .iter()
        .any(|bounds| bounds.intersects(query))
}

/// Vec3.normalize: the length is a float until 1.17 (MathHelper.sqrt), and a vector shorter than
/// 1.0E-4 (1.0E-5F from 1.21.2) is zero (1.16 Vec3d.java:66-67, 1.17 :49, 1.21.2 Vec3.java:73).
fn vanilla_normalize(vector: Vec3, release: MinecraftVersion) -> Vec3 {
    let length = vector.length_squared().sqrt();
    let length = if release.at_least(1, 17, 0) {
        length
    } else {
        java_float(length)
    };
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

#[cfg(test)]
mod tests {
    use super::{
        tick_arrow, tick_arrow_in_release, tick_projectile, tick_projectile_in_release,
        tick_trident, tick_trident_in_release, ArrowState, ProjectileKind, ProjectileState,
        TridentState,
    };
    use crate::math::{Aabb, Vec3};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

    fn full_block_world(position: (i32, i32, i32)) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..Default::default()
            },
        );
        world
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
        // ExplosiveProjectileEntity.getDrag returns 0.95F (1.20.4 :131-132)
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
        };

        // 26.2 AbstractArrow.java:536-554 (1.21.11 :497-516): the hit position less
        // signum(v) * 0.05F, v = 0; inGroundTime stays 0 on the hit tick (:205-206), shake is 7
        let hit = tick_arrow(&mut state, &world);
        assert!(hit.block_hit);
        assert!(hit.in_ground);
        assert!(!hit.started_falling);
        assert_eq!(hit.position, Vec3::new(1.0 - 0.05000000074505806, 0.5, 0.5));
        assert_eq!(hit.velocity, Vec3::ZERO);
        assert_eq!(hit.in_ground_time, 0);
        assert_eq!(hit.shake_time, 7);
        assert_eq!(state.last_block_position, Some((1, 0, 0)));

        // 1.21.11 AbstractArrow.java:180-197: shakeTime counts down first, inGroundTime counts up
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
        };

        let result = tick_arrow(&mut state, &WorldSnapshot::new());

        assert!(result.started_falling);
        assert!(!result.in_ground);
        assert_eq!(result.position, state.position);
        assert!((result.velocity.x - 0.1).abs() < 1.0e-12);
        assert!((result.velocity.y - 0.4).abs() < 1.0e-12);
        assert!((result.velocity.z - 0.9).abs() < 1.0e-12);
        assert_eq!(state.life, 0);
        // 1.21.11 AbstractArrow.java:180-197: the startFalling tick still runs inGroundTime++
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

        // 1.21.11 AbstractArrow.java:190 and :312-314: it falls only when the state differs AND
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
        // 1.21.1 ThrowableProjectile.java:45 (1.20.5 ThrownEntity.java tick, 1.16.5 :71):
        // checkInsideBlocks after the hit test, before the motion is read for the move; the
        // column under air is Entity.onBubbleColumnSurfaceCollision's min(1.8, v + 0.1) (1.20.5
        // Entity.java:2305-2316, BubbleColumnBlock.java:46-49). 1.16.1 ThrowableProjectile.tick
        // and 1.16 ThrownEntity.tick have no such call
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
        // -0.3 until 1.21.4 (1.21.4 Entity.java:2529-2540)
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
        // 1.21.1 ThrownEntity.java:44-72: the move takes the old motion, then 0.99F and the
        // gravity; 1.20.4 :96 and 1.8.9 EntityThrowable.java:245 return 0.03F
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
        // 1.21.1 ThrownEntity.java:46-71: the hit is handled, the move still goes to pos + v
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
        // 1.20.4 ExplosiveProjectileEntity.java:85-110: move by the old motion, then
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
        // 1.21 ExplosiveProjectileEntity.java:99: v + normalize(v) * accelerationPower (0.1),
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
        // 1.21.11 LlamaSpit.java:36-54
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

        // before 1.20.5 the gravity is 0.06F (1.20.4 LlamaSpitEntity.java)
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
        // 1.21.11 FireworkRocketEntity.java:139-149: move(SELF), then the motion is put back;
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
    fn fishing_hook_falls_moves_then_drags() {
        // 1.21.11 FishingHook.java:233-246: -0.03 off the ground out of water, move(SELF),
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
        // FishingBobberEntity.java:231 the same as 1.21.5). A hook whose onGround is still set
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
        // 1.13.2 FishingBobberEntity.java:159-161: FluidState.method_17810, level / 9
        // (FlowableFluid.java:434), with no look above; 1.14 FishHookEntity.java:155
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
        // 1.14 LlamaSpitEntity.java:79 assigns this.x before the removal; 1.15 :79-81 reads the
        // target and setPosition runs only when the spit stays (:116)
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
        // 1.20.5 LlamaSpitEntity.java:50 isInsideWaterOrBubbleColumn (Entity.java:1176-1190, the
        // block at the position; 1.13.2 :121 method_15575); 1.21.5 LlamaSpit.java:54 isInWater
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
        // 1.21.11 FishingHook.java:284-287: v = normalize(v) * hit.distanceTo (squared)
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
        // 1.21.11 FishingHook.java:178-182 (1.12.2 FishingBobberEntity.java:30-35)
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
        // 1.21 PersistentProjectileEntity.java:431-449 (onBlockHit): v = hit - pos, pos -=
        // normalize(v) * 0.05F; then the tick moves by v, scales by 0.99F and takes the gravity
        // (:250-277), also on the hit tick
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

        // the in-ground ticks keep that motion (1.21 :165-176 leaves it alone)
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

        // 1.13.2 AbstractArrowEntity.java:324-330: v = (float)(hit - pos), pos -= v /
        // (MathHelper.sqrt(v.v) * 20.0F)
        let mut modern = arrow(Vec3::new(0.3, 0.5, 0.5), Vec3::new(2.0, 0.0, 0.0), release(1, 13, 2));
        let result = tick_arrow_in_release(&mut modern, &world, release(1, 13, 2));
        assert!(result.block_hit);
        assert_eq!(result.position.x, (0.3 - offset / f64::from(length * 20.0_f32)) + offset);
        assert_eq!(result.velocity.x, offset * f64::from(0.99_f32));
        // gravity 0.05F before 1.20.5 (1.13.2 :296)
        assert_eq!(result.velocity.y, -f64::from(0.05_f32));

        // 1.12.2 AbstractArrowEntity.java:370-376 (1.8.9 EntityArrow.java:292-297): pos -= v /
        // MathHelper.sqrt(v.v) * 0.05F
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
        // 1.20.4 PersistentProjectileEntity.java:244: y - 0.05F; 1.20.5 :265-266: 0.05
        let mut old = arrow(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), release(1, 20, 4));
        let result = tick_arrow_in_release(&mut old, &WorldSnapshot::new(), release(1, 20, 4));
        assert_eq!(result.velocity.y, -0.05000000074505806);
        let mut new = arrow(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), release(1, 20, 5));
        let result = tick_arrow_in_release(&mut new, &WorldSnapshot::new(), release(1, 20, 5));
        assert_eq!(result.velocity.y, -0.05);
    }

    #[test]
    fn arrow_in_water_before_1_21_2_moves_then_takes_the_water_inertia() {
        // 1.21 PersistentProjectileEntity.java:261-277: the move by v, then v * 0.6F, then gravity
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

        // 1.9.4 AbstractArrowEntity.java:175-188 (1.8.9 EntityArrow.java:167-180, 1.10.2): any
        // change of the hit block's state drops the arrow
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(tick_arrow_in_release(&mut state, &replaced, release(1, 10, 2)).started_falling);

        // 1.12.2 AbstractArrowEntity.java:179 (1.11.2, 1.13.2 :222 with the same 0.05): a
        // replacement whose shape meets the box inflated by 0.05 keeps it
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &replaced, release(1, 11, 2)).started_falling);
        let mut state = arrow_in_ground(resting, (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &replaced, release(1, 13, 2)).started_falling);

        // until 1.13.2 the state is read at the hit block, from 1.14 at the arrow's own block
        // (1.13.2 :195-196, 1.14 ProjectileEntity.java:174-175): the unchanged hit block keeps
        // a 1.13.2 arrow whatever its own cell holds
        let mut unchanged = WorldSnapshot::new();
        unchanged.set_block((1, 0, 0), solid(11));
        let mut state = arrow_in_ground(Vec3::new(5.5, 5.5, 5.5), (1, 0, 0), 11);
        assert!(!tick_arrow_in_release(&mut state, &unchanged, release(1, 13, 2)).started_falling);
        let mut state = arrow_in_ground(Vec3::new(5.5, 5.5, 5.5), (1, 0, 0), 11);
        assert!(tick_arrow_in_release(&mut state, &unchanged, release(1, 14, 4)).started_falling);

        // 1.15 ProjectileEntity tick: the 0.5 box inflated by 0.06 reaches a block above the
        // arrow; from 1.16 (PersistentProjectileEntity.java:245-247) only the point is tested
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
        // 1.20.4 PersistentProjectileEntity.java:410-417 and :236-249 with TridentEntity's
        // 0.99F water inertia (TridentEntity.java:195)
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
        let mut state = snowball(Vec3::new(1.5, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0));
        let result = tick_projectile_in_release(&mut state, &world, release(26, 2, 0));
        assert!(result.block_hit);
        assert_eq!(result.position, Vec3::new(1.5 + 0.001, 0.5, 0.5));
        // Before 1.13 vanilla tests all six faces (1.12.2 Box.java:294-327), so this ray hits
        // the maxX face at (2.0, 0.5, 0.5); the native clip has no such rule yet, so 1.12.2 is
        // not asserted here.
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
        assert!(result.block_hit);
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
        // 1.21.11 AbstractWindCharge.java:36-47 (accelerationPower 0) and :128-135 (inertia 1.0F,
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
        // 1.20.4 WindChargeEntity.java:127-130: RaycastContext.ShapeType.OUTLINE; COLLIDER from
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
        // wind_charge and breeze_wind_charge; 1.12.2 EntityType.java:205-210 xp_bottle and
        // fireworks_rocket; 1.8.9 EntityList.java:239-245 ThrownPotion, ThrownExpBottle and
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
}
