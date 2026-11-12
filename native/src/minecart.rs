//! Minecarts. The rail movement is the server's: AbstractMinecart.tick moves a cart only where the
//! level is not a client's (1.8.8 AbstractMinecart.java:291-306, 1.20.4 :240-250, 26.3
//! OldMinecartBehavior.java:50 and NewMinecartBehavior.java:50). A client interpolates to what the
//! server sends, and what it does is kept apart here, in `tick_client_linear` and
//! `tick_client_weighted`: neither reads a rail.
//!
//! Which server path a cart takes is the level's enabled features, not its release: 26.3
//! AbstractMinecart.java:67-71, 536-538. `MinecartRules::new` is given that selection explicitly.
//!
//! Evidence for the old path, by what breaks where (every side read):
//! * 1.8.9 - 1.9.4: the rider's forward input (LivingEntity), NoGravity not read, the 0.04F gravity.
//! * 1.10: NoGravity is read and the cart picks riders up in its own tick (1.10.2 :337, :376).
//! * 1.13: a rider's input counts for a player alone (1.13.2 :473).
//! * 1.14: the player's velocity replaces its input, and the double constants the float ones
//!   (1.14.4 :200, :278, :283, :337-341, :452).
//! * 1.15.2: a furnace cart's push turns along its motion (1.16 MinecartFurnace.java; no
//!   source of 1.15.1 or 1.15.2 here: mc_furnace_lit recorded on both places it).
//! * 1.16: a rail keeps the speed factor of the block under it out (1.16 :309).
//! * 1.17: water halves the speed, slows gravity and drags (1.17 :308, :383, :419, :571).
//! * 1.21.2: AbstractMinecart splits into the behaviours, the rider's move intent replaces its
//!   velocity, and the furnace cart's speed and push are worked anew (26.3 is the same code).
//!
//! What a tick does besides moving the cart is the server's too. What the world does not hold
//! comes in on the state, never as a guess: `random_seed` (Entity.random, whose draws decide a TNT
//! cart's fuse and blast), `inventory` (what a hopper cart holds), a spawner cart's delays and, on
//! the scene, the item entities and containers it can take from and the end gateways about it. A
//! TNT cart's blast hurts and pushes the scene's other carts and pushes its other entities. What
//! the level's random decides (which entity a gateway takes, what a spawner spawns) is left to the
//! server, and the tick says so. The server's results come out on `MinecartTick`. A client runs
//! none of it: `tick_client_effects` is what its tick of the same code does.

use crate::block_traversal::Movement;
use crate::entity::entity_push_impulses;
use crate::legacy_fluid;
use crate::projectile::{clip_hits, ClipShape};
use crate::math::{
    atan2_vanilla, clamp_f64_for, floor_i32_for, java_float, wrap_degrees_f32, Aabb, Vec3,
};
use crate::simulation::{
    apply_block_effects, apply_inside_block_effects, blocks_inside, blocks_inside_moves,
    fire_or_lava_block, landing_position, mth_cos, mth_sin, push_by_fluid, water_contact,
    SimulationState,
};
use crate::vehicle::{
    apply_effects_inside, lava_flow_scale, move_vehicle_among, MoveRules, VehicleMove,
};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, FluidState, WorldSnapshot};

type BlockPosition = (i32, i32, i32);

/// `BlockState.tags`, as NativeWorldSync sets them: BlockTags.RAILS.
pub const TAG_RAIL: u32 = 1;
/// Blocks.POWERED_RAIL (`golden_rail` before 1.13).
pub const TAG_POWERED_RAIL: u32 = 1 << 1;
/// Blocks.ACTIVATOR_RAIL.
pub const TAG_ACTIVATOR_RAIL: u32 = 1 << 2;
/// BlockState.isRedstoneConductor (before 1.9 Block.isFullCube; isSolidBlock in Yarn).
pub const TAG_REDSTONE_CONDUCTOR: u32 = 1 << 3;

/// AbstractMinecart's dimensions: 0.98F by 0.7F (1.8.8 AbstractMinecart.java:122, 26.3
/// EntityType.java).
const HALF_WIDTH: f32 = 0.98 / 2.0;
const HEIGHT: f32 = 0.7;
/// 1.0E-5F as a double (26.3 NewMinecartBehavior and Vec3.normalize).
const TINY: f64 = 9.999999747378752e-6;

/// AbstractMinecart's subclass: what decides its drag, speed and riders.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MinecartVariant {
    #[default]
    Rideable,
    Chest,
    Hopper,
    Furnace,
    Tnt,
    Spawner,
    CommandBlock,
}

impl MinecartVariant {
    /// The entity's registry name in any release, or None for what is no minecart.
    #[must_use]
    pub fn from_entity_name(name: &str) -> Option<Self> {
        Some(match name {
            "minecart" => Self::Rideable,
            "chest_minecart" => Self::Chest,
            "hopper_minecart" => Self::Hopper,
            "furnace_minecart" => Self::Furnace,
            "tnt_minecart" => Self::Tnt,
            "spawner_minecart" => Self::Spawner,
            "command_block_minecart" | "commandblock_minecart" => Self::CommandBlock,
            _ => return None,
        })
    }

    fn holds_items(self) -> bool {
        matches!(self, Self::Chest | Self::Hopper)
    }
}

/// Which AbstractMinecart.behavior a level gives its carts: OldMinecartBehavior, or
/// NewMinecartBehavior under the minecart_improvements feature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinecartBehavior {
    Old,
    Improved,
}

/// What a level tells its carts: its release, the behaviour its enabled features select and, under
/// the improvements, GameRules.MAX_MINECART_SPEED in blocks a second.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinecartRules {
    pub release: MinecraftVersion,
    pub behavior: MinecartBehavior,
    pub max_minecart_speed: i32,
    /// Level.getMinY: a cart 64 under it is gone. 0 before 1.18 (and in a dimension that starts there).
    pub min_y: i32,
    /// GameRules.TNT_EXPLODES, from 1.21.5 (1.21.5 MinecartTNT.java).
    pub tnt_explodes: bool,
    /// GameRules.RULE_SPAWNER_BLOCKS_ENABLED, from 1.21.9: a spawner cart's spawner runs only with it.
    pub spawner_blocks: bool,
    /// Whether a nether portal has somewhere to take a cart: Entity.getPortalRect finds one or finds
    /// nothing, as only a player's makes one (1.20.4 Entity.java getTeleportTarget). A level's own.
    pub nether_exit: bool,
    /// Level.random's raw 48 bits where the caller knows them, which a blast's rays draw from
    /// (`MinecartExplosion::blown`). No client sees it: without it a blast gives only the blocks
    /// every draw breaks and those some draw could.
    pub level_random: Option<u64>,
}

impl MinecartRules {
    /// `improvements` is whether the level has minecart_improvements enabled. A release before
    /// 1.21.2 has no such feature; with it, the speed rule (1..1000, 26.3 GameRules.java:218) has
    /// to be given. (1.21.1 FeatureFlags.java:37-46 registers no minecart_improvements;
    /// 1.21.2 FeatureFlags.java:45 does.)
    pub fn new(
        release: MinecraftVersion,
        improvements: bool,
        max_minecart_speed: Option<i32>,
    ) -> Result<Self, String> {
        if improvements && !release.at_least(1, 21, 2) {
            return Err(format!("minecart: the improvements feature does not exist in {release}"));
        }
        let max_minecart_speed = match (improvements, max_minecart_speed) {
            (true, None) => {
                return Err("minecart: the improvements need the maxMinecartSpeed game rule".to_owned())
            }
            (true, Some(speed)) if !(1..=1000).contains(&speed) => {
                return Err(format!("minecart: maxMinecartSpeed {speed} is outside 1..1000"))
            }
            (true, Some(speed)) => speed,
            (false, _) => 8,
        };
        Ok(Self {
            release,
            behavior: if improvements { MinecartBehavior::Improved } else { MinecartBehavior::Old },
            max_minecart_speed,
            min_y: 0,
            tnt_explodes: true,
            spawner_blocks: true,
            nether_exit: true,
            level_random: None,
        })
    }

    /// Sets the level's bottom (its min y, which the caller reads from the level) and whether its TNT game rule lets
    /// a cart explode: a setter, which mirrors no method
    #[must_use]
    pub fn in_level(self, min_y: i32, tnt_explodes: bool) -> Self {
        Self { min_y, tnt_explodes, ..self }
    }
}

/// A stack in an inventory or an item entity: what ItemStack.isSameItemSameComponents and its
/// getMaxStackSize say, the item as a number the caller gives each kind of stack. Empty at 0.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinecartStack {
    pub item: u32,
    pub count: i32,
    pub max_stack: i32,
}

/// An ItemEntity near a cart, in the level's order: a hopper cart's pickup leaves what is left of
/// it, and a TNT cart's blast hurts and pushes it (`Scene::blast_items`). A cart's own move never
/// pushes one: ItemEntity is not Entity.isPushable, which both paths' pushes select by (26.3
/// EntitySelector.pushableBy in OldMinecartBehavior and NewMinecartBehavior.pushEntities; 1.8.8
/// MinecartEntity's loop pushes only AbstractMinecart).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinecartItem {
    pub bounds: Aabb,
    pub stack: MinecartStack,
    /// Entity.position: the bottom centre of its box unless the caller says otherwise.
    pub position: Vec3,
    pub velocity: Vec3,
    /// ItemEntity.health, 5 for a new one (NBT Health).
    pub health: i32,
    /// What keeps a blast's hurt off it: Entity.invulnerable, the nether star before 1.21.2
    /// (1.21.1 ItemEntity.java hurt) and a stack whose DAMAGE_RESISTANT holds IS_EXPLOSION from it.
    pub blast_proof: bool,
    /// The hurt took its health: the ItemEntity is gone (ItemEntity.hurt's discard).
    pub removed: bool,
}

impl MinecartItem {
    /// An item entity with its position at its box's bottom centre, at rest and whole.
    #[must_use]
    pub fn new(bounds: Aabb, stack: MinecartStack) -> Self {
        let position = Vec3::new((bounds.min.x + bounds.max.x) / 2.0, bounds.min.y, (bounds.min.z + bounds.max.z) / 2.0);
        Self { bounds, stack, position, velocity: Vec3::ZERO, health: 5, blast_proof: false, removed: false }
    }
}

/// A block's Container a hopper cart can take from, given with what it holds in the slots
/// getSlots(container, DOWN) lists, in order. Whose it is (a chest, a furnace, a double chest) and
/// what canTakeItemFromContainer refuses is the caller's: no client can see into it.
#[derive(Clone, Debug, PartialEq)]
pub struct MinecartContainer {
    pub position: BlockPosition,
    pub slots: Vec<MinecartStack>,
}

/// The portals that take a cart out of its level, and from 1.21 the end gateway at its block,
/// which takes it elsewhere in the level (Entity.portalProcess).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinecartPortal {
    Nether,
    End,
    Gateway(BlockPosition),
}

/// An end gateway's block entity (TheEndGatewayBlockEntity), which no client can see into: its
/// block, the block it sends what it takes to (the exit portal itself under ExactTeleport, else what
/// findExitPosition finds about it: the caller's), its teleportCooldown and its age. A gateway with
/// no exit outside the End sends nothing. Where the level's random picks one of several entities in
/// it and they are not all on the same cart, `undecided` comes back set and none is moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinecartGateway {
    pub position: BlockPosition,
    pub exit: Option<BlockPosition>,
    pub cooldown: i32,
    pub age: i64,
    pub undecided: bool,
}

/// Why a cart left its level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinecartRemoval {
    /// VehicleEntity.destroy: broken, and what it dropped is the server's.
    Destroyed,
    /// A creative player's hit: VehicleEntity.hurtServer's discard, without drops.
    Discarded,
    /// MinecartTNT.explode.
    Exploded,
    /// Entity.onBelowWorld.
    BelowWorld,
    /// It changed dimension: where it comes out is the server's.
    Portal(MinecartPortal),
}

/// What hurts a cart that it is not itself touching: what VehicleEntity.hurtServer and MinecartTNT
/// ask of the damage source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinecartDamage {
    Plain,
    /// DamageTypeTags.IS_FIRE.
    Fire,
    /// DamageTypeTags.IS_EXPLOSION.
    Explosion,
}

/// An attack on a cart from outside its tick (a player's hit, an arrow, an explosion).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinecartHurt {
    pub kind: MinecartDamage,
    pub amount: f32,
    /// The attacker is a player in creative mode.
    pub creative: bool,
    /// The direct entity is a burning arrow, with its velocity's lengthSqr.
    pub burning_arrow: Option<f64>,
}

/// java.util.Random and LegacyRandomSource: Entity.random, by its raw 48 bits (26.3
/// BitRandomSource.java and LegacyRandomSource.java draw as java.util.Random does).
#[derive(Clone, Copy, Debug)]
struct EntityRandom(u64);

impl EntityRandom {
    /// LegacyRandomSource#next: the 48-bit linear congruential step, its top `bits`.
    fn next(&mut self, bits: u32) -> i32 {
        self.0 = self.0.wrapping_mul(25_214_903_917).wrapping_add(11) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i64 as i32
    }

    /// BitRandomSource#nextInt of a bound. Only ever drawn with a bound that is no power of two.
    fn next_int(&mut self, bound: i32) -> i32 {
        loop {
            let sample = self.next(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return modulo;
            }
        }
    }

    /// BitRandomSource#nextFloat.
    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * 5.960_464_5e-8_f32
    }

    /// BitRandomSource#nextDouble.
    fn next_double(&mut self) -> f64 {
        let upper = i64::from(self.next(26));
        let lower = i64::from(self.next(27));
        ((upper << 27) + lower) as f64 * 1.110_223_024_625_156_5e-16
    }
}

/// What a cart's first passenger gives its tick. Which field counts depends on the release: its
/// forward input and yaw (`forward`, `yaw`) before 1.14, its velocity to 1.21.1, its move intent
/// (ServerPlayer.getLastClientMoveIntent) from 1.21.2.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinecartRider {
    pub player: bool,
    /// LivingEntity: before 1.13 any living rider's forward input counts.
    pub living: bool,
    pub forward: f32,
    pub yaw: f32,
    pub velocity: Vec3,
    pub move_intent: Vec3,
}

/// A cart between two ticks, as the server keeps it. The fields a tick cannot find in the world
/// are given: `flipped` (AbstractMinecart.flipped, never sent to a client), `in_water`
/// (Entity.isInWater, the end of the tick before), the furnace's `fuel` and `push`, the
/// `comparator_signal` of what a chest or hopper cart holds (AbstractContainerMenu's), and whether
/// it `has_loot_table` still to roll (1.9+), and what the side effects read: `random_seed` is the
/// raw state of Entity.random, `inventory` a hopper cart's five slots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinecartState {
    pub variant: MinecartVariant,
    pub position: Vec3,
    pub velocity: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub flipped: bool,
    pub on_rails: bool,
    pub on_ground: bool,
    /// AbstractMinecart.firstTick: the improved path sets a new cart onto its rail.
    pub first_tick: bool,
    pub in_water: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub stuck_speed_multiplier: Vec3,
    pub no_gravity: bool,
    /// Its dimension's FAST_LAVA.
    pub ultra_warm: bool,
    pub fall_distance: f64,
    /// The box it keeps before 1.17; else its position's.
    pub bounding_box: Option<Aabb>,
    pub rider: Option<MinecartRider>,
    pub fuel: i32,
    pub push: Vec3,
    pub comparator_signal: i32,
    pub has_loot_table: bool,
    /// Entity.hasCustomName: before 1.20.3 a creative hit destroys a named cart rather than discarding it.
    pub custom_name: bool,
    /// Entity.tickCount, which a command block cart's activation delay counts.
    pub tick_count: i32,
    /// VehicleEntity's synced hurt time, hurt direction and damage.
    pub hurt_time: i32,
    pub hurt_dir: i32,
    pub damage: f32,
    /// Entity.invulnerable.
    pub invulnerable: bool,
    /// MinecartTNT.fuse: -1 until it is primed.
    pub fuse: i32,
    /// MinecartTNT.ignitionSource from 1.21.5: the hurt that primed it was a creative player's, which its blast then is
    /// (VehicleEntity.hurtServer's instabuild test). The server's alone, never saved.
    pub creative_ignition: bool,
    pub explosion_power_base: f32,
    pub explosion_speed_factor: f32,
    /// Entity.random's raw 48 bits.
    pub random_seed: u64,
    /// MinecartHopper.enabled.
    pub enabled: bool,
    pub inventory: [MinecartStack; 5],
    /// MinecartCommandBlock.lastActivated.
    pub last_activated: i32,
    /// A spawner cart's BaseSpawner: its spawnDelay, minSpawnDelay, maxSpawnDelay and
    /// requiredPlayerRange (NBT Delay, MinSpawnDelay, MaxSpawnDelay, RequiredPlayerRange).
    pub spawn_delay: i32,
    pub min_spawn_delay: i32,
    pub max_spawn_delay: i32,
    pub required_player_range: i32,
    /// The portal it is in, whether it was this tick, its time there, its cooldown.
    pub portal: Option<MinecartPortal>,
    pub inside_portal: bool,
    pub portal_time: i32,
    pub portal_cooldown: i32,
    /// Why it has gone, once it has.
    pub removed: Option<MinecartRemoval>,
    /// A move past MAX_WALKED_CELLS did not run, and the bindings refuse the tick (`move_vehicle`).
    pub move_out_of_range: bool,
    /// The scoreboard team it is on, whose collision rule the pushes select by (`pushable_by`).
    pub team: Option<MinecartTeam>,
}

/// Team.CollisionRule (1.9.4 Team.java; 26.3 Team.java), which pushes ask from 1.9.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollisionRule {
    #[default]
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

/// A scoreboard team: the same `id` is the same team (Team.isAlliedTo is identity), and its rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinecartTeam {
    pub id: u32,
    pub rule: CollisionRule,
    /// PlayerTeam.allowFriendlyFire, true for a new team: from 26.3 a cart's blast hurts its allies only with it.
    pub allow_friendly_fire: bool,
}

impl Default for MinecartTeam {
    fn default() -> Self {
        Self { id: 0, rule: CollisionRule::Always, allow_friendly_fire: true }
    }
}

/// The team half of EntitySelector.pushableBy(pusher) for `target`, from 1.9 (1.9.4
/// EntitySelector.java:54-84, 26.3 EntitySelector.java:39-70; 1.8.8 has no collision rule and
/// LivingEntity.pushEntities selects by none, 1.8.8 LivingEntity.java pushEntities): a pusher whose
/// team never collides pushes nothing, a target whose team never collides is not pushed, two of one
/// team are not where either rule is PUSH_OWN_TEAM, and two of different teams (or none) are not
/// where either is PUSH_OTHER_TEAMS. Whether the target isPushable is the caller's.
pub fn pushable_by(release: MinecraftVersion, pusher: Option<MinecartTeam>, target: Option<MinecartTeam>) -> bool {
    if !release.at_least(1, 9, 0) {
        return true;
    }
    let own = pusher.map_or(CollisionRule::Always, |team| team.rule);
    if own == CollisionRule::Never {
        return false;
    }
    let theirs = target.map_or(CollisionRule::Always, |team| team.rule);
    if theirs == CollisionRule::Never {
        return false;
    }
    let same = matches!((pusher, target), (Some(a), Some(b)) if a.id == b.id);
    if (own == CollisionRule::PushOwnTeam || theirs == CollisionRule::PushOwnTeam) && same {
        return false;
    }
    !(own == CollisionRule::PushOtherTeams || theirs == CollisionRule::PushOtherTeams) || same
}

impl Default for MinecartState {
    fn default() -> Self {
        Self {
            variant: MinecartVariant::default(),
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            y_rot: 0.0,
            x_rot: 0.0,
            flipped: false,
            on_rails: false,
            on_ground: false,
            first_tick: false,
            in_water: false,
            collided_horizontally: false,
            collided_vertically: false,
            stuck_speed_multiplier: Vec3::ZERO,
            no_gravity: false,
            ultra_warm: false,
            fall_distance: 0.0,
            bounding_box: None,
            rider: None,
            fuel: 0,
            push: Vec3::ZERO,
            comparator_signal: 0,
            has_loot_table: false,
            custom_name: false,
            tick_count: 0,
            hurt_time: 0,
            hurt_dir: 1,
            damage: 0.0,
            invulnerable: false,
            fuse: -1,
            creative_ignition: false,
            explosion_power_base: 4.0,
            explosion_speed_factor: 1.0,
            random_seed: 0,
            enabled: true,
            inventory: [MinecartStack::default(); 5],
            last_activated: 0,
            spawn_delay: 20,
            min_spawn_delay: 200,
            max_spawn_delay: 800,
            required_player_range: 16,
            portal: None,
            inside_portal: false,
            portal_time: 0,
            portal_cooldown: 0,
            removed: None,
            move_out_of_range: false,
            team: None,
        }
    }
}

/// An entity near a cart that is not a cart the scene ticks: what the move collides with
/// (canBeCollidedWith or isPushable, 26.3 AbstractBoat.java:115-117), pushes and picks up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinecartOther {
    pub position: Vec3,
    pub bounds: Aabb,
    pub velocity: Vec3,
    pub pushable: bool,
    /// Entity.canBeCollidedWith: a boat or a shulker, solid to a cart though not pushed.
    pub solid: bool,
    pub player: bool,
    pub iron_golem: bool,
    /// LivingEntity: before 1.10 a cart picks up only those it is pushed by.
    pub living: bool,
    /// Entity.isPassenger.
    pub passenger: bool,
    pub has_passengers: bool,
    /// The tick the cart at this index picked it up.
    pub rides: Option<usize>,
    /// The other entity (an index of the scene's others) it rides, where that one rides a cart: a
    /// jockey, whose root vehicle (Entity#getRootVehicle) is that cart.
    pub rides_other: Option<usize>,
    /// AbstractBoat: what its tick's end does to a cart is Boat.push (`boat_push_carts`).
    pub boat: bool,
    /// Where its own tick leaves it, when that is not where a cart ticking ahead of it finds it.
    pub tick_place: Option<(Vec3, Aabb)>,
    /// How many of the scene's carts the level ticks ahead of it (spawn order); `None` is all of them.
    pub ticks_after: Option<usize>,
    /// Player.rideTick has it leave its cart this tick, after the cart's tick and before its own turn.
    pub gets_off: bool,
    /// Entity.getEyeHeight, from which a blast pushes it: without it 0.85 of its box's height, the
    /// default of every entity whose type gives none (26.3 EntityDimensions.java:16).
    pub eye_height: Option<f32>,
    /// An ArmorStand: not pushable, and its own pushEntities pushes only the rideable carts whose
    /// position is within 0.2 of its own squared (`armor_stand_push_carts`).
    pub armor_stand: bool,
    /// A Bat: not pushable, and its pushEntities pushes nothing (1.8.8 Bat.java:54-65, 26.3 Bat.java:82-91).
    pub bat: bool,
    /// The scoreboard team it is on.
    pub team: Option<MinecartTeam>,
    /// Entity.boardingCooldown, from 1.9: 60 ticks from leaving a vehicle (Entity.removePassenger), counted down in its
    /// baseTick, in which no cart picks it up (Entity.canRide). (1.8.8 Entity.java has none; 1.9.4 Entity.java:88,
    /// 411-412, 1521, 1556; 26.3 Entity.java:217, 562-563, 2548, 2615.)
    pub boarding_cooldown: i32,
    /// It left the level with the cart it rides, through a nether or end portal (from 1.21, Entity.changeDimension takes
    /// the passengers along): nothing of the scene's meets it any more.
    pub removed: bool,
}

/// Entity#getRootVehicle of one of the scene's others: the cart its chain of vehicles ends in
/// (1.9.4 Entity.java:2215-2223), through the others it rides. A chain that names no cart, or
/// loops, ends in none.
fn root_cart(others: &[MinecartOther], index: usize) -> Option<usize> {
    let mut at = index;
    for _ in 0..=others.len() {
        let other = others.get(at)?;
        if let Some(cart) = other.rides {
            return Some(cart);
        }
        at = other.rides_other?;
    }
    None
}

/// Whether one of the scene's others shares the cart's vehicle, which leaves it out of the cart's
/// pushes and collisions: from 1.9 Entity#isPassengerOfSameVehicle, the same root vehicle, which a
/// jockey on the cart's rider has too (1.9.4 Entity.java:1139-1140, 2225-2227; 1.9.4
/// Level.java:1102); before, only the cart's own rider (1.8.8 Entity.java:1025
/// `arg.rider != this && arg.vehicle != this`, Level.java:1084).
fn shares_cart(others: &[MinecartOther], index: usize, cart: usize, release: MinecraftVersion) -> bool {
    if release.at_least(1, 9, 0) {
        root_cart(others, index) == Some(cart)
    } else {
        others.get(index).is_some_and(|other| other.rides == Some(cart))
    }
}

impl Default for MinecartOther {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
            velocity: Vec3::ZERO,
            pushable: false,
            solid: false,
            player: false,
            iron_golem: false,
            living: false,
            passenger: false,
            has_passengers: false,
            rides: None,
            rides_other: None,
            boat: false,
            tick_place: None,
            ticks_after: None,
            gets_off: false,
            eye_height: None,
            armor_stand: false,
            bat: false,
            team: None,
            boarding_cooldown: 0,
            removed: false,
        }
    }
}

impl MinecartOther {
    /// Entity.canRide's boardingCooldown half, from 1.9 (whether it sneaks is the caller's: no mob a cart picks up does).
    /// (1.8.8 Entity.java has no canRide; 1.9.4 Entity.java:1520-1521)
    fn can_board(&self, release: MinecraftVersion) -> bool {
        !release.at_least(1, 9, 0) || self.boarding_cooldown <= 0
    }

    /// Entity.removePassenger: what leaves a vehicle waits 60 ticks to board one, from 1.9.
    /// (1.8.8 Entity.java has no boardingCooldown; 1.9.4 Entity.java:1550-1556)
    fn leave_vehicle(&mut self, release: MinecraftVersion) {
        self.rides = None;
        self.passenger = false;
        if release.at_least(1, 9, 0) {
            self.boarding_cooldown = 60;
        }
    }
}

/// NewMinecartBehavior.MinecartStep: where a client is to take a cart, of the weight of the way.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinecartStep {
    pub position: Vec3,
    pub movement: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub weight: f32,
}

/// What a tick of a cart leaves besides the cart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MinecartTick {
    /// The activator rail it passed, with its powered state (AbstractMinecart.activateMinecart).
    pub activator: Option<bool>,
    /// The steps the improved path sends the clients (NewMinecartBehavior.lerpSteps).
    pub steps: Vec<MinecartStep>,
    /// The tick primed its fuse: the entity event a client hears (MinecartTNT.primeFuse).
    pub primed: bool,
    /// The blasts of Level.explode, its position and float power. What one does to the blocks and
    /// entities around it is the server's, and a client's comes in the explosion packet.
    pub explosions: Vec<MinecartExplosion>,
    /// The activator rail ran a command block cart's command (BaseCommandBlock.performCommand).
    pub command_ran: bool,
    /// What a spawner cart's spawner did that the level's random decides.
    pub spawner: Option<MinecartSpawn>,
}

/// The turns of a spawner cart's BaseSpawner.serverTick that draw on the level's random, which the
/// server keeps: what comes of them is its own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinecartSpawn {
    /// BaseSpawner.delay drew the delay of a spawner left at -1 between its min and max: the cart's
    /// `spawn_delay` stays -1, for the server's number.
    DrewDelay,
    /// The delay was out and it tried to spawn about this block. Whether anything came, and the
    /// delay after (BaseSpawner.delay on a spawn), are the server's; `spawn_delay` stays 0.
    Attempt(BlockPosition),
}

/// A Level.explode of a TNT cart, and the blocks its rays reach (ServerExplosion.
/// calculateExplodedPositions, Explosion.explode before 1.21.2): each position, air included, as
/// the explosion's toBlow keeps it, in order of position. Every ray's strength is a level random
/// draw, so `certain` is what the weakest draws break and `possible` what the strongest could;
/// `blown` is the exact set where `MinecartRules::level_random` was given. Breaking them, and what
/// they drop, is the server's after the entities (26.3 ServerExplosion.java:238-242, 1.8.8
/// Explosion.java finalizeExplosion): the carts after this one in the tick still find them in the
/// world given.
#[derive(Clone, Debug, PartialEq)]
pub struct MinecartExplosion {
    pub position: Vec3,
    pub power: f32,
    pub certain: Vec<BlockPosition>,
    pub possible: Vec<BlockPosition>,
    pub blown: Option<Vec<BlockPosition>>,
    /// Level.random after the rays' 1352 draws, with `blown`.
    pub level_random: Option<u64>,
}

/// RailShape, in its order (1.8.9 EnumRailDirection, 26.3 RailShape.java).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RailShape {
    NorthSouth,
    EastWest,
    AscendingEast,
    AscendingWest,
    AscendingNorth,
    AscendingSouth,
    SouthEast,
    SouthWest,
    NorthWest,
    NorthEast,
}

impl RailShape {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "north_south" => Self::NorthSouth,
            "east_west" => Self::EastWest,
            "ascending_east" => Self::AscendingEast,
            "ascending_west" => Self::AscendingWest,
            "ascending_north" => Self::AscendingNorth,
            "ascending_south" => Self::AscendingSouth,
            "south_east" => Self::SouthEast,
            "south_west" => Self::SouthWest,
            "north_west" => Self::NorthWest,
            "north_east" => Self::NorthEast,
            _ => return None,
        })
    }

    /// RailShape#isSlope.
    fn is_slope(self) -> bool {
        matches!(
            self,
            Self::AscendingEast | Self::AscendingWest | Self::AscendingNorth | Self::AscendingSouth
        )
    }

    /// AbstractMinecart.exits (26.3 AbstractMinecart.java:554-564; 1.8.8
    /// AbstractMinecart.java:37-48 lists the same, in this order).
    fn exits(self) -> (BlockPosition, BlockPosition) {
        const WEST: BlockPosition = (-1, 0, 0);
        const EAST: BlockPosition = (1, 0, 0);
        const NORTH: BlockPosition = (0, 0, -1);
        const SOUTH: BlockPosition = (0, 0, 1);
        // (BlockPos#below)
        const fn below((x, y, z): BlockPosition) -> BlockPosition {
            (x, y - 1, z)
        }
        match self {
            Self::NorthSouth => (NORTH, SOUTH),
            Self::EastWest => (WEST, EAST),
            Self::AscendingEast => (below(WEST), EAST),
            Self::AscendingWest => (WEST, below(EAST)),
            Self::AscendingNorth => (NORTH, below(SOUTH)),
            Self::AscendingSouth => (below(NORTH), SOUTH),
            Self::SouthEast => (SOUTH, EAST),
            Self::SouthWest => (SOUTH, WEST),
            Self::NorthWest => (NORTH, WEST),
            Self::NorthEast => (NORTH, EAST),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Rail {
    shape: RailShape,
    powered_rail: bool,
    activator: bool,
    powered: bool,
}

/// BlockTags.RAILS (BaseRailBlock#isRail of the state).
pub(crate) fn is_rail(world: &WorldSnapshot, position: BlockPosition) -> bool {
    world.block_cow(position).tags & TAG_RAIL != 0
}

/// BaseRailBlock.isRail and the properties of its state.
fn rail_at(world: &WorldSnapshot, position: BlockPosition) -> Option<Rail> {
    let tags = world.block_cow(position).tags;
    if tags & TAG_RAIL == 0 {
        return None;
    }
    let properties = world.block_properties_ref(position);
    let value = |name: &str| properties.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
    Some(Rail {
        shape: value("shape").and_then(RailShape::parse)?,
        powered_rail: tags & TAG_POWERED_RAIL != 0,
        activator: tags & TAG_ACTIVATOR_RAIL != 0,
        powered: value("powered") == Some("true"),
    })
}

/// BlockBehaviour#isRedstoneConductor of the block at `position`.
fn conductor(world: &WorldSnapshot, position: BlockPosition) -> bool {
    world.block_cow(position).tags & TAG_REDSTONE_CONDUCTOR != 0
}

/// BlockPos#offset.
fn offset((x, y, z): BlockPosition, (dx, dy, dz): BlockPosition) -> BlockPosition {
    (x + dx, y + dy, z + dz)
}

/// EntityDimensions#makeBoundingBox of a cart's dimensions at `position`.
fn box_at(position: Vec3) -> Aabb {
    let half = f64::from(HALF_WIDTH);
    Aabb::new(
        Vec3::new(position.x - half, position.y, position.z - half),
        Vec3::new(position.x + half, position.y + f64::from(HEIGHT), position.z + half),
    )
}

/// Entity#getBoundingBox: the box the cart keeps, else the one its position makes.
fn bounds_of(cart: &MinecartState) -> Aabb {
    cart.bounding_box.unwrap_or_else(|| box_at(cart.position))
}

/// Vec3.normalize as the improved path's release has it.
fn normalized(vector: Vec3) -> Vec3 {
    let length = vector.length();
    if length < TINY {
        Vec3::ZERO
    } else {
        Vec3::new(vector.x / length, vector.y / length, vector.z / length)
    }
}

/// Vec3.atBottomCenterOf.
fn bottom_center((x, y, z): BlockPosition) -> Vec3 {
    Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5)
}

/// Half an exit of the rail's shape as a vector, `new Vec3(exit).scale(0.5)` of
/// NewMinecartBehavior#adjustToRails and NewMinecartBehavior#stepAlongTrack.
fn half_exit((x, y, z): BlockPosition) -> Vec3 {
    Vec3::new(f64::from(x) * 0.5, f64::from(y) * 0.5, f64::from(z) * 0.5)
}

/// Mth.rotLerp of floats.
fn rot_lerp(alpha: f32, from: f32, to: f32) -> f32 {
    from + alpha * wrap_degrees_f32(to - from)
}

/// One tick of a level's carts, in its tick order: each ticks as the ones before it left it, so a
/// cart pushed by another starts its tick from that push.
pub fn tick_minecarts(
    carts: &mut [MinecartState],
    others: &mut [MinecartOther],
    world: &WorldSnapshot,
    rules: MinecartRules,
) -> Vec<MinecartTick> {
    tick_minecarts_among(carts, others, &mut [], &mut [], world, rules)
}

/// `tick_minecarts` with what a hopper cart can take from.
pub fn tick_minecarts_among(
    carts: &mut [MinecartState],
    others: &mut [MinecartOther],
    items: &mut [MinecartItem],
    containers: &mut [MinecartContainer],
    world: &WorldSnapshot,
    rules: MinecartRules,
) -> Vec<MinecartTick> {
    tick_minecarts_with(carts, others, items, containers, &mut [], world, rules)
}

/// `tick_minecarts_among` with the end gateways about the carts.
pub fn tick_minecarts_with(
    carts: &mut [MinecartState],
    others: &mut [MinecartOther],
    items: &mut [MinecartItem],
    containers: &mut [MinecartContainer],
    gateways: &mut [MinecartGateway],
    world: &WorldSnapshot,
    rules: MinecartRules,
) -> Vec<MinecartTick> {
    // Before 1.9 a player ticks in the handling of its move packet, ahead of the level (1.8.8
    // ServerGamePacketListenerImpl.java:228-290); from 1.9 in the connection's tick, after it (1.9.4 :165).
    let players_first = !rules.release.at_least(1, 9, 0);
    let mut level_random = rules.level_random;
    if players_first {
        players_push_carts(carts, others, rules.release);
    }
    let ticks = (0..carts.len())
        .map(|current| {
            others_tick(carts, others, rules.release, |ahead| ahead == Some(current));
            if carts[current].removed.is_some() {
                return MinecartTick::default();
            }
            let mut scene = Scene {
                world,
                rules,
                carts: &mut *carts,
                others: &mut *others,
                items: &mut *items,
                containers: &mut *containers,
                gateways: &mut *gateways,
                level_random: &mut level_random,
                current,
                support: None,
                recorded: Vec::new(),
                old_position: Vec3::ZERO,
                old_y_rot: 0.0,
                consumed_item: false,
                tick: MinecartTick::default(),
            };
            scene.tick();
            scene.tick
        })
        .collect();
    let last = carts.len();
    others_tick(carts, others, rules.release, |ahead| ahead.is_none_or(|carts_ahead| carts_ahead >= last));
    if !players_first {
        players_push_carts(carts, others, rules.release);
    }
    tick_gateways(carts, others, gateways, rules.release);
    ticks
}

/// The end gateways' block entities, which tick after the level's entities (1.20.4
/// ServerLevel.tick, tickBlockEntities after the entities). Before 1.21 the block entity's tick
/// takes what is in its block itself (1.9.4 TheEndGatewayBlockEntity.java:85-101, 1.20.6
/// TheEndGatewayBlockEntity.java:85-106 teleportTick); from 1.21 EndGatewayBlock.entityInside puts
/// an entity in the portal in its own move (`enter_gateway`) and the block entity only counts down
/// (1.21 TheEndGatewayBlockEntity.java:78-91 portalTick). A gateway calls attention to itself every
/// 2400 ticks of its age from 1.11 (1.10.2 TheEndGatewayBlockEntity.java:93-96 does not; 1.11.2
/// TheEndGatewayBlockEntity.java:99).
fn tick_gateways(carts: &mut [MinecartState], others: &[MinecartOther], gateways: &mut [MinecartGateway], release: MinecraftVersion) {
    if !release.at_least(1, 9, 0) {
        return;
    }
    let attention = release.at_least(1, 11, 0);
    for gateway in gateways.iter_mut() {
        let cooling = gateway.cooldown > 0;
        gateway.age += 1;
        if cooling {
            gateway.cooldown -= 1;
            continue;
        }
        if !release.at_least(1, 21, 0) {
            gateway_takes(carts, others, gateway, release);
        }
        if attention && gateway.age % 2400 == 0 {
            gateway.cooldown = gateway_cooldown(release);
        }
    }
}

/// TheEndGatewayBlockEntity.triggerCooldown: 20 ticks until 1.11, 40 from it (1.10.2
/// TheEndGatewayBlockEntity.java:125; 1.11.2 TheEndGatewayBlockEntity.java:130).
fn gateway_cooldown(release: MinecraftVersion) -> i32 {
    if release.at_least(1, 11, 0) {
        40
    } else {
        20
    }
}

/// Before 1.21, what a gateway's own tick does to what is in its block: the first (1.15.2
/// TheEndGatewayBlockEntity.java:106-108), from 1.16 one of them by the level's random and what it
/// rides with it, down at the bottom of the exit block rather than half a block up (1.16
/// TheEndGatewayBlockEntity.java:108, 184; 1.15.2 :167), from 1.16.2 only what is not on a portal
/// cooldown, which it then is (1.16.1 TheEndGatewayBlockEntity.java:106; 1.16.2 :107, 123, 189).
/// teleportEntity sets the cooldown, which triggerCooldown sets again, wherever the exit is
/// (1.20.6 TheEndGatewayBlockEntity.java:156-190).
fn gateway_takes(carts: &mut [MinecartState], others: &[MinecartOther], gateway: &mut MinecartGateway, release: MinecraftVersion) {
    let (x, y, z) = gateway.position;
    let low = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
    let cell = Aabb::new(low, low + Vec3::new(1.0, 1.0, 1.0));
    let roots = release.at_least(1, 16, 0);
    let cooldowns = release.at_least(1, 16, 2);
    // (what each candidate takes through: a cart's index, or None for another entity)
    let mut taken: Vec<Option<usize>> = Vec::new();
    for (index, cart) in carts.iter().enumerate() {
        if cart.removed.is_none() && bounds_of(cart).intersects(cell) && !(cooldowns && cart.portal_cooldown > 0) {
            taken.push(Some(index));
        }
    }
    for other in others.iter().filter(|other| !other.removed && other.bounds.intersects(cell)) {
        let root = if roots { other.rides } else { None };
        if !(cooldowns && root.is_some_and(|cart| carts[cart].portal_cooldown > 0)) {
            taken.push(root);
        }
    }
    let Some(&first) = taken.first() else {
        return;
    };
    // (before 1.16 the first in the level's lists, whose order among the carts and other entities is not known here)
    let decided = if roots { taken.iter().all(|cart| *cart == first) } else { taken.len() == 1 || taken.iter().all(Option::is_some) };
    gateway.undecided |= !decided;
    if let (true, Some(index), Some((ex, ey, ez))) = (decided, first, gateway.exit) {
        let lift = if roots { 0.0 } else { 0.5 };
        let cart = &mut carts[index];
        if cooldowns {
            cart.portal_cooldown = 300;
        }
        cart.position = Vec3::new(f64::from(ex) + 0.5, f64::from(ey) + lift, f64::from(ez) + 0.5);
        if cart.bounding_box.is_some() {
            cart.bounding_box = Some(box_at(cart.position));
        }
    }
    gateway.cooldown = gateway_cooldown(release);
}

/// The turn in the level's order of what is neither a cart nor a player, for those `turn` says it is: the end of each
/// one's tick, from where the tick leaves it.
fn others_tick(carts: &mut [MinecartState], others: &mut [MinecartOther], release: MinecraftVersion, turn: impl Fn(Option<usize>) -> bool) {
    for other in others.iter_mut().filter(|other| !other.removed && !other.player && turn(other.ticks_after)) {
        // (Entity.baseTick counts the boarding cooldown down ahead of LivingEntity.aiStep's pushes: 1.9.4 Entity.java:411-412)
        if other.boarding_cooldown > 0 {
            other.boarding_cooldown -= 1;
        }
        if let Some((position, bounds)) = other.tick_place.take() {
            other.position = position;
            other.bounds = bounds;
        }
        if other.boat {
            boat_push_carts(carts, other, release);
        } else if other.living {
            living_push_cart(carts, other, release);
        }
    }
}

/// The players' turn (`tick_minecarts_among`). A player's server tick travels by its velocity and pushes from there, and
/// the connection then snaps it back to where its packets put it (26.3 ServerGamePacketListenerImpl.java:327-328, 1.20.4
/// ServerGamePacketListenerImpl.java:257-258): `tick_place` is that place.
fn players_push_carts(carts: &mut [MinecartState], others: &mut [MinecartOther], release: MinecraftVersion) {
    for other in others.iter_mut().filter(|other| !other.removed && other.living && other.player) {
        if other.gets_off {
            // (before 1.9 the player's turn is ahead of the level, and what throws it off is the cart's tick to come: an
            // activator rail, 1.8.8 Minecart.java:33-37. It rides through the tick, and pushes on its next turn.
            // 1.8.8 ServerGamePacketListenerImpl.java:228,256,290 tick the player in its move packet; 1.9.4 :165 in the
            // connection's tick, after the level.)
            if !release.at_least(1, 9, 0) {
                continue;
            }
            // (the rider that got off pushes the cart it just left, from where it landed)
            if let Some(cart) = other.rides.and_then(|index| carts.get_mut(index)) {
                cart.rider = None;
            }
            other.leave_vehicle(release);
            other.gets_off = false;
        }
        if let Some((position, bounds)) = other.tick_place.take() {
            other.position = position;
            other.bounds = bounds;
        }
        living_push_cart(carts, other, release);
    }
}

/// The end of Boat.tick from 1.9 (1.9.4 Boat.java:276-292, 1.13.2 :321-337, 1.20.4 :342-357, 26.3 AbstractBoat.java:259-268):
/// `this.push(cart)` with the boat as `this`, so Boat.push's gate (1.9.4 :185-193, 26.3 :168-176) and then Entity.push, not the
/// cart's AbstractMinecart.push: a whole 0.05F along the longer axis. A ridden boat does it too, wherever the client has put it.
/// Up to 1.8 the loop pushes boats alone (1.8.8 Boat.java:356).
fn boat_push_carts(carts: &mut [MinecartState], boat: &mut MinecartOther, release: MinecraftVersion) {
    if !release.at_least(1, 9, 0) {
        return;
    }
    let reach = boat.bounds.inflate(Vec3::new(f64::from(0.2_f32), f64::from(-0.01_f32), f64::from(0.2_f32)));
    for (index, cart) in carts.iter_mut().enumerate() {
        let bounds = bounds_of(cart);
        // (EntitySelector.pushableBy(boat), 1.9.4 Boat.java:276)
        if cart.removed.is_some() || boat.rides == Some(index) || !bounds.intersects(reach) || bounds.min.y > boat.bounds.min.y || !pushable_by(release, boat.team, cart.team) {
            continue;
        }
        let (to_boat, to_cart) = entity_push_impulses(boat.position, cart.position, false, false, false, boat.has_passengers, cart.rider.is_some(), boat.pushable, true, release);
        boat.velocity = boat.velocity + to_boat;
        cart.velocity = cart.velocity + to_cart;
    }
}

/// LivingEntity.pushEntities of a living thing that meets a cart, which the cart takes in AbstractMinecart.push. Before 1.9
/// its box reaches 0.2F further (1.8.8 LivingEntity.java:1473, 1.9.4 :1763), and before 1.10 the push is also where a
/// rideable cart going fast picks a mob up (1.8.8 AbstractMinecart.java:672-682).
/// From 1.9 it pushes only the carts EntitySelector.pushableBy lets it (1.9.4 LivingEntity.java:1763,
/// `pushable_by`). A bat pushes nothing, and an armor stand by its own way (`armor_stand_push_carts`).
fn living_push_cart(carts: &mut [MinecartState], other: &mut MinecartOther, release: MinecraftVersion) {
    if other.bat {
        return;
    }
    if other.armor_stand {
        armor_stand_push_carts(carts, other, release);
        return;
    }
    let reach = if release.at_least(1, 9, 0) { 0.0 } else { f64::from(0.2_f32) };
    let hitbox = other.bounds.inflate(Vec3::new(reach, 0.0, reach));
    for (index, cart) in carts.iter_mut().enumerate() {
        if cart.removed.is_some() || other.rides == Some(index) || !bounds_of(cart).intersects(hitbox) || !pushable_by(release, other.team, cart.team) {
            continue;
        }
        cart_takes_push(cart, index, other, release);
    }
}

/// ArmorStand.pushEntities: the rideable carts its own box meets whose position is within 0.2 of
/// its own squared push away from it, by no team (1.8.8 ArmorStand.java:286-294, 1.12.2 :342-350
/// RIDABLE_MINECARTS, 1.21.5 ArmorStand.java:215-220 isRideable, 26.3 the same).
fn armor_stand_push_carts(carts: &mut [MinecartState], stand: &mut MinecartOther, release: MinecraftVersion) {
    for (index, cart) in carts.iter_mut().enumerate() {
        if cart.removed.is_some()
            || cart.variant != MinecartVariant::Rideable
            || stand.rides == Some(index)
            || !bounds_of(cart).intersects(stand.bounds)
            || stand.position.distance_squared(cart.position) > 0.2
        {
            continue;
        }
        cart_takes_push(cart, index, stand, release);
    }
}

/// AbstractMinecart.push(Entity) of what pushes a cart, with the pick-up before 1.10 (1.9.4
/// AbstractMinecart.java:668-675 push picks up a mob; 1.10.2 AbstractMinecart.java:377-388 its own tick does).
fn cart_takes_push(cart: &mut MinecartState, index: usize, other: &mut MinecartOther, release: MinecraftVersion) {
    {
        if !release.at_least(1, 10, 0)
            && cart.variant == MinecartVariant::Rideable
            && !other.player
            && !other.iron_golem
            && cart.velocity.horizontal_length_squared() > 0.01
            && cart.rider.is_none()
            && !other.passenger
            && other.can_board(release)
        {
            other.rides = Some(index);
            other.passenger = true;
            cart.rider = Some(MinecartRider { living: true, ..MinecartRider::default() });
        }
        push_minecart(cart, other, release);
    }
}

/// VehicleEntity.hurtServer of what comes from outside a cart's tick: the same hurt its tick's own
/// fire and lava take, with what it leaves.
pub fn hurt_minecart(cart: &mut MinecartState, others: &mut [MinecartOther], hurt: MinecartHurt, world: &WorldSnapshot, rules: MinecartRules) -> MinecartTick {
    let mut level_random = rules.level_random;
    let mut scene = Scene {
        world,
        rules,
        carts: std::slice::from_mut(cart),
        others,
        items: &mut [],
        containers: &mut [],
        gateways: &mut [],
        level_random: &mut level_random,
        current: 0,
        support: None,
        recorded: Vec::new(),
        old_position: Vec3::ZERO,
        old_y_rot: 0.0,
        consumed_item: false,
        tick: MinecartTick::default(),
    };
    scene.attack(hurt);
    scene.tick
}

/// One server tick of a cart alone.
pub fn tick_minecart(state: &mut MinecartState, world: &WorldSnapshot, rules: MinecartRules) -> MinecartTick {
    tick_minecarts(std::slice::from_mut(state), &mut [], world, rules)
        .pop()
        .unwrap_or_default()
}

/// AbstractMinecart.push(Entity): what a cart takes from an entity that pushes it (a mob's or
/// player's LivingEntity.pushEntities; another cart goes through `tick_minecarts`). Gives what it
/// pushes back on `other`.
pub fn push_minecart(cart: &mut MinecartState, other: &mut MinecartOther, release: MinecraftVersion) {
    let Some((xa, za)) = minecart_push_impulse(cart.position, other.position, release) else {
        return;
    };
    cart.velocity = cart.velocity.add_components(-xa, 0.0, -za);
    other.velocity = other.velocity.add_components(xa / 4.0, 0.0, za / 4.0);
}

/// AbstractMinecart.push(Entity) up to its use of the push: nothing nearer than 1.0E-4F, the
/// float square root before 1.17 (1.8.8 AbstractMinecart.java:689, 1.16.5 :703, 1.17 :693),
/// 0.1F and the half, and from 1.21.2 the same (26.3 AbstractMinecart.java:439-456).
fn minecart_push_impulse(this: Vec3, entity: Vec3, release: MinecraftVersion) -> Option<(f64, f64)> {
    let mut xa = entity.x - this.x;
    let mut za = entity.z - this.z;
    let mut distance = xa * xa + za * za;
    if distance < f64::from(1.0e-4_f32) {
        return None;
    }
    distance = if release.at_least(1, 17, 0) { distance.sqrt() } else { java_float(distance.sqrt()) };
    xa /= distance;
    za /= distance;
    let pow = (1.0 / distance).min(1.0);
    xa *= pow;
    za *= pow;
    let tenth = f64::from(0.1_f32);
    xa *= tenth;
    za *= tenth;
    // (the release's 1.0F - pushSpeedReduction, nothing for a cart, goes here before 1.17)
    Some((xa * 0.5, za * 0.5))
}

struct Scene<'a> {
    world: &'a WorldSnapshot,
    rules: MinecartRules,
    carts: &'a mut [MinecartState],
    others: &'a mut [MinecartOther],
    items: &'a mut [MinecartItem],
    containers: &'a mut [MinecartContainer],
    gateways: &'a mut [MinecartGateway],
    /// Level.random, which the tick's blasts draw on in their order, where the caller knows it.
    level_random: &'a mut Option<u64>,
    current: usize,
    /// The block the last move stands on (Entity.mainSupportingBlockPos).
    support: Option<BlockPosition>,
    /// Entity.movementThisTick.
    recorded: Vec<Movement>,
    /// Entity.xo, yo, zo and yRotO: where the tick began, and each track step from 1.21.2.
    old_position: Vec3,
    old_y_rot: f32,
    /// MinecartHopper.consumedItemThisFrame.
    consumed_item: bool,
    tick: MinecartTick,
}

impl Scene<'_> {
    fn cart(&mut self) -> &mut MinecartState {
        &mut self.carts[self.current]
    }

    fn me(&self) -> &MinecartState {
        &self.carts[self.current]
    }

    fn release(&self) -> MinecraftVersion {
        self.rules.release
    }

    fn improved(&self) -> bool {
        self.rules.behavior == MinecartBehavior::Improved
    }

    fn at_least(&self, major: u16, minor: u16, patch: u16) -> bool {
        self.release().at_least(major, minor, patch)
    }

    /// Entity.setPos: the box follows.
    fn set_pos(&mut self, position: Vec3) {
        // (1.16.5 Entity.java:631-632; 1.17 Entity.java:561)
        let keeps_box = !self.at_least(1, 17, 0);
        let cart = self.cart();
        cart.position = position;
        cart.bounding_box = keeps_box.then(|| box_at(position));
    }

    /// AbstractMinecart.tick and what each variant does around it (26.3 AbstractMinecart.java:219-241).
    fn tick(&mut self) {
        // Level.tickNonPassenger counts the tick off before it runs (1.8.8 Level.java, 26.3 Level)
        let cart = self.cart();
        cart.tick_count += 1;
        if cart.hurt_time > 0 {
            cart.hurt_time -= 1;
        }
        if cart.damage > 0.0 {
            cart.damage -= 1.0;
        }
        self.check_below_world();
        self.handle_portal();
        self.old_position = self.me().position;
        self.old_y_rot = self.me().y_rot;
        // (1.16.5 Entity.java:631-632; 1.17 Entity.java:559)
        if !self.at_least(1, 17, 0) && self.me().bounding_box.is_none() {
            let bounds = bounds_of(self.me());
            self.cart().bounding_box = Some(bounds);
        }
        match self.rules.behavior {
            MinecartBehavior::Old => self.tick_old(),
            MinecartBehavior::Improved => self.tick_improved(),
        }
        // (isInLava is not on a first tick: 1.16.2 Entity.java:1179, 26.3 :1779-1781)
        if self.update_fluids() && self.at_least(1, 16, 2) && !self.me().first_tick {
            // from 1.16.2, by the jars (AbstractMinecart.tick: isInLava, lavaHurt; 1.16.1 has none); 1.17
            // AbstractMinecart.java:93-96, 26.3 :234-238
            self.lava_hurt();
            let halved = self.halved_fall();
            self.cart().fall_distance = halved;
        }
        self.cart().first_tick = false;
        // MinecartFurnace.tick, after super.tick (1.8.8 MinecartFurnace.java tick; 26.3 :46-58)
        let cart = self.cart();
        if cart.variant == MinecartVariant::Furnace {
            if cart.fuel > 0 {
                cart.fuel -= 1;
            }
            if cart.fuel <= 0 {
                cart.push = Vec3::ZERO;
            }
        }
        match self.me().variant {
            MinecartVariant::Tnt => self.tick_tnt(),
            MinecartVariant::Hopper => self.try_consume_items(),
            MinecartVariant::Spawner => self.tick_spawner(),
            _ => {}
        }
    }

    /// MinecartSpawner.tick after super.tick: BaseSpawner.serverTick at the cart's block (26.3
    /// MinecartSpawner.java:72-75, BaseSpawner.java:101-110; 1.8.8 MinecartSpawner.java:68-70,
    /// BaseSpawner.java:75-98). It counts down while a player is within its requiredPlayerRange of
    /// the block's centre (26.3 EntityGetter.java:106-117, 1.8.8 Level.java:2419-2431; a spectator
    /// is not told apart here), and from 1.21.9 only under the game rule (1.21.8 BaseSpawner.java:92
    /// asks nothing more; 1.21.9 BaseSpawner.java:92 isSpawnerBlockEnabled). At 0 it spawns by the
    /// level's random, the server's.
    fn tick_spawner(&mut self) {
        if self.me().removed.is_some() || (self.at_least(1, 21, 9) && !self.rules.spawner_blocks) {
            return;
        }
        let release = self.release();
        let position = self.me().position;
        let block = (floor_i32_for(release, position.x), floor_i32_for(release, position.y), floor_i32_for(release, position.z));
        let centre = Vec3::new(f64::from(block.0) + 0.5, f64::from(block.1) + 0.5, f64::from(block.2) + 0.5);
        let range = f64::from(self.me().required_player_range);
        let near = self.others.iter().any(|other| other.player && !other.removed && (range < 0.0 || other.position.distance_squared(centre) < range * range));
        if !near {
            return;
        }
        let cart = self.cart();
        // BaseSpawner.delay: the least delay when it is no less than the most (26.3 BaseSpawner.java:196-202)
        if cart.spawn_delay == -1 {
            if cart.max_spawn_delay > cart.min_spawn_delay {
                self.tick.spawner = Some(MinecartSpawn::DrewDelay);
                return;
            }
            cart.spawn_delay = cart.min_spawn_delay;
        }
        if cart.spawn_delay > 0 {
            cart.spawn_delay -= 1;
        } else {
            self.tick.spawner = Some(MinecartSpawn::Attempt(block));
        }
    }

    /// `fallDistance *= 0.5`, in the field's type (a double from 1.21.5).
    fn halved_fall(&self) -> f64 {
        let fall = self.me().fall_distance;
        // (1.21.4 AbstractMinecart.java:227; 1.21.5 AbstractMinecart.java:229)
        if self.at_least(1, 21, 5) {
            fall * 0.5
        } else {
            f64::from(fall as f32 * 0.5)
        }
    }

    /// Entity.checkBelowWorld: under -64 before 1.17 (1.8.9, 1.16 AbstractMinecart.java tick),
    /// then 64 under the level's bottom (1.17 Entity.checkOutOfWorld, 26.3 Entity.java:626-631).
    fn check_below_world(&mut self) {
        let floor = f64::from(self.rules.min_y) - 64.0;
        if self.me().position.y < floor {
            self.remove(MinecartRemoval::BelowWorld);
        }
    }

    /// Entity#remove: Entity#setRemoved keeps the first reason, and the riders get off.
    fn remove(&mut self, why: MinecartRemoval) {
        if self.me().removed.is_none() {
            self.cart().removed = Some(why);
        }
        self.eject();
    }

    /// What a powered activator rail, or the cart's end, does to its riders (Entity#ejectPassengers).
    fn eject(&mut self) {
        self.cart().rider = None;
        let current = self.current;
        let release = self.release();
        for other in self.others.iter_mut().filter(|other| other.rides == Some(current)) {
            other.leave_vehicle(release);
        }
    }

    /// Entity.getPortalWaitTime of what is no player: 1 from 1.9, 0 again by 1.16.4 (1.8.9, 1.9.4 and
    /// 1.16 Entity.java getMaxNetherPortalTime; 1.16.4 getPortalWaitTime). 1.16.1 records as 1.
    /// (1.8.8 Entity.java:427-428 returns 0, 1.9.4 Entity.java:500-501 returns 1; 1.16.1 Entity.java:524-525
    /// returns 1, 1.16.2 Entity.java:555-556 returns 0.)
    fn portal_wait(&self) -> i32 {
        i32::from(self.at_least(1, 9, 0) && !self.at_least(1, 16, 2))
    }

    /// Entity.handlePortal (26.3 Entity.java:2725-2745, PortalProcessor) from 1.21; before it the
    /// counter of Entity.tickPortal (1.20.4 Entity.java) or of the cart's own tick (1.8.9): a cart
    /// goes once it has been in one past its wait. (1.20.6 Entity.java:2125-2152 counts portalTime
    /// itself; 1.21 Entity.java:2208-2232 hands it to the PortalProcessor.)
    fn handle_portal(&mut self) {
        let wait = self.portal_wait();
        let processed = self.at_least(1, 21, 0);
        let cart = self.cart();
        if processed {
            if cart.portal_cooldown > 0 {
                cart.portal_cooldown -= 1;
            }
            let Some(portal) = cart.portal else {
                return;
            };
            if cart.inside_portal {
                cart.inside_portal = false;
                cart.portal_time += 1;
                self.go_through(portal);
            } else {
                cart.portal_time = (cart.portal_time - 4).max(0);
                if cart.portal_time <= 0 {
                    cart.portal = None;
                }
            }
            return;
        }
        if cart.inside_portal {
            // (what stays out is decided on the way in: this asks only that it rides nothing, and a cart does not)
            cart.inside_portal = false;
            let time = cart.portal_time;
            cart.portal_time += 1;
            if time >= wait {
                cart.portal_time = wait;
                let portal = cart.portal.unwrap_or(MinecartPortal::Nether);
                self.go_through(portal);
            }
        } else {
            cart.portal_time = (cart.portal_time - 4).max(0);
        }
        let cart = self.cart();
        if cart.portal_cooldown > 0 {
            cart.portal_cooldown -= 1;
        }
    }

    /// Entity.setPortalCooldown's getDimensionChangingDelay: 300 until 1.21 (1.20.6
    /// Entity.java:2156-2158); in 1.21 and 1.21.1 a player rider's 10, else 300 (1.21.1
    /// Entity.java:2233-2236, Player.java:426-428); from 1.21.2 a vehicle's own 10 (1.21.2 and 26.3
    /// VehicleEntity.java:122-124).
    fn dimension_changing_delay(&self) -> i32 {
        let player_rider = self.me().rider.is_some_and(|rider| rider.player);
        if self.at_least(1, 21, 2) || (self.at_least(1, 21, 0) && player_rider) { 10 } else { 300 }
    }

    /// Entity.changeDimension: it goes, unless the far side has nothing for it, and then only the
    /// cooldown is left of the attempt.
    /// From 1.21 the cart takes its riders with it (`take_riders_out`, `through_gateway`); before it a
    /// cart that carries anyone never gets this far (`enter_portal`).
    fn go_through(&mut self, portal: MinecartPortal) {
        let delay = self.dimension_changing_delay();
        self.cart().portal_cooldown = delay;
        if let MinecartPortal::Gateway(entry) = portal {
            self.through_gateway(entry);
        } else if portal == MinecartPortal::End || self.rules.nether_exit {
            // (1.20.6 Entity.java:2599-2600 lets no vehicle change dimension; 1.21 Entity.java:2588-2632 takes its passengers)
            if self.at_least(1, 21, 0) {
                self.take_riders_out(MinecartRemoval::Portal(portal));
            } else {
                self.remove(MinecartRemoval::Portal(portal));
            }
        }
    }

    /// The cart leaves the level and its riders with it, to ride it on the far side: 1.21
    /// Entity.changeDimension sends each passenger through and has it startRiding the new cart (1.21
    /// Entity.java:2588-2632), 1.21.2 teleportCrossDimension the same (1.21.2 Entity.java:2779-2811).
    fn take_riders_out(&mut self, why: MinecartRemoval) {
        if self.me().removed.is_none() {
            self.cart().removed = Some(why);
        }
        let current = self.current;
        for other in self.others.iter_mut().filter(|other| other.rides == Some(current)) {
            other.removed = true;
        }
    }

    /// EndGatewayBlock.getPortalDestination: the bottom centre of the gateway's exit, in the same
    /// level, the cart's motion and rotation kept (26.3 EndGatewayBlock.java:89-101,
    /// TheEndGatewayBlockEntity.java:130-145; Relative.DELTA and ROTATION). Its riders go with it and
    /// ride on: in 1.21 and 1.21.1 each to the exit itself, off and back on, which leaves it a boarding
    /// cooldown (1.21 Entity.java:2588-2626 changeDimension: unRide, the passengers' own
    /// changeDimension to the same place, startRiding forced); from 1.21.2 each where it was from the
    /// cart, still riding (1.21.2 Entity.java:2759-2777 teleportSameDimension, :2813-2819
    /// calculatePassengerTransition).
    fn through_gateway(&mut self, entry: BlockPosition) {
        let exit = self.gateways.iter().find(|gateway| gateway.position == entry).and_then(|gateway| gateway.exit);
        if let Some((x, y, z)) = exit {
            let to = Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
            let from = self.me().position;
            let keeps_offset = self.at_least(1, 21, 2);
            let current = self.current;
            for other in self.others.iter_mut().filter(|other| other.rides == Some(current)) {
                let at = if keeps_offset { to + (other.position - from) } else { to };
                other.bounds = other.bounds.translated(at - other.position);
                other.position = at;
                if !keeps_offset {
                    other.boarding_cooldown = 60;
                }
            }
            self.set_pos(to);
        }
    }

    /// EndGatewayBlock.entityInside from 1.21: a gateway not cooling down takes the cart in as a
    /// portal and cools down, whether or not the cart's own cooldown keeps it out (1.21
    /// EndGatewayBlock.java:79-91; 1.20.6 has no entityInside of it, its block entity takes what is
    /// in it, `tick_gateways`).
    fn enter_gateway(&mut self, position: BlockPosition) {
        if !self.at_least(1, 21, 0) {
            return;
        }
        let Some(gateway) = self.gateways.iter().position(|gateway| gateway.position == position) else {
            return;
        };
        if self.gateways[gateway].cooldown > 0 {
            return;
        }
        self.enter_portal(MinecartPortal::Gateway(position));
        self.gateways[gateway].cooldown = 40;
    }

    /// NetherPortalBlock.entityInside and, from 1.21, EndPortalBlock's: Entity.setAsInsidePortal
    /// (26.3 Entity.java:2702-2717; 1.20.4 setInNetherPortal). A cart stays out while it carries
    /// anyone until 1.21 (1.8.9 and 1.20.4 canUsePortals; 26.3 Entity.java canUsePortal(false)
    /// asks only that it rides nothing). (1.20.6 Entity.java:2599-2600 canChangeDimensions refuses a
    /// vehicle; 1.21 Entity.java:2194-2205 keeps a PortalProcessor, and :2661-2662 asks no more.)
    fn enter_portal(&mut self, portal: MinecartPortal) {
        let processed = self.at_least(1, 21, 0);
        if !processed && self.me().rider.is_some() {
            return;
        }
        let delay = self.dimension_changing_delay();
        let cart = self.cart();
        if cart.portal_cooldown > 0 {
            cart.portal_cooldown = delay;
            return;
        }
        // (PortalProcessor.isSamePortal asks the block, and a gateway's entry is the first of the tick's: 26.3 Entity.java:2712-2715)
        let same = match (cart.portal, portal) {
            (Some(MinecartPortal::Gateway(_)), MinecartPortal::Gateway(_)) => true,
            (old, new) => old == Some(new),
        };
        if processed && !same {
            cart.portal_time = 0;
        }
        if !(same && cart.inside_portal) {
            cart.portal = Some(portal);
        }
        cart.inside_portal = true;
    }

    /// What the blocks a pass of Entity.checkInsideBlocks holds do to a cart besides its motion:
    /// CactusBlock.entityInside, and from 1.16 BaseFireBlock's (26.3 :116-120); a cell each, for
    /// each pass. Before 1.16 fire burns through Entity.move (`burn`). An end portal takes a cart at
    /// once before 1.21 (1.8.9 and 1.20.4 EndPortalBlock.java). (1.15.2 FireBlock has no
    /// entityInside; 1.16 BaseFireBlock.java:126-133 hurts by its fireDamage. 1.20.6
    /// EndPortalBlock.java:44-53 changes the dimension at once; 1.21 EndPortalBlock.java:49-57
    /// sets it as inside the portal.)
    fn contact_hurts(&mut self, inside: &[((i32, i32, i32), bool)]) {
        for &(position, _) in inside {
            if self.me().removed.is_some() {
                return;
            }
            self.enter_gateway(position);
            match self.world.effect_at(position) {
                BlockEffect::Cactus => self.hurt(MinecartDamage::Plain, 1.0),
                BlockEffect::Fire if self.at_least(1, 16, 0) => self.hurt(MinecartDamage::Fire, 1.0),
                BlockEffect::SoulFire if self.at_least(1, 16, 0) => self.hurt(MinecartDamage::Fire, 2.0),
                BlockEffect::NetherPortal => self.enter_portal(MinecartPortal::Nether),
                BlockEffect::EndPortal if !self.in_end_portal(position) => {}
                BlockEffect::EndPortal if self.at_least(1, 21, 0) => self.enter_portal(MinecartPortal::End),
                BlockEffect::EndPortal if self.me().rider.is_none() => {
                    self.remove(MinecartRemoval::Portal(MinecartPortal::End));
                }
                _ => {}
            }
        }
    }

    /// The block's shape has to meet the box: up to 12/16 of its height from 1.9 (1.9.4
    /// EndPortalBlock.java), and from 6/16 too from 1.17 (1.16.5 and 1.17 EndPortalBlock.java, 26.3
    /// SHAPE). Before 1.9 its cell is enough. (1.8.8 EndPortalBlock.java:77-79 tests no shape; 1.9.4
    /// EndPortalBlock.java:21,79 the box against 0..0.75. 1.16.5 EndPortalBlock.java:23 is box(0, 0, 0,
    /// 16, 12, 16); 1.17 EndPortalBlock.java:23 starts at 6.)
    fn in_end_portal(&self, (x, y, z): BlockPosition) -> bool {
        if !self.at_least(1, 9, 0) {
            return true;
        }
        let at = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
        let bottom = if self.at_least(1, 17, 0) { 6.0 / 16.0 } else { 0.0 };
        let shape = Aabb::new(at + Vec3::new(0.0, bottom, 0.0), at + Vec3::new(1.0, 12.0 / 16.0, 1.0));
        bounds_of(self.me()).intersects(shape)
    }

    /// Entity.move's fire test before 1.16: fire or lava anywhere in the box shrunk by 0.001 burns
    /// it for 1 (1.8.8 Entity.java:707-708, 1.15 :631-639; 1.16 has it no more, only the ticks).
    /// (1.15.2 Entity.java:636-644 burns; 1.16 Entity.java:672-674 only resets the fire ticks.) The cells
    /// end at floor(max + 1) in 1.8.8 Level.java:1508-1514, at Mth.ceil(max) from 1.9 (1.9.4
    /// Level.java:1582-1588, 1.12.2 Level.java:1588-1594, 1.13.2 Level.java:1311-1317; Mth.ceil, 1.9.4
    /// Mth.java:170-172, is the ceiling).
    fn burn_in_move(&mut self) {
        if self.at_least(1, 16, 0) {
            return;
        }
        let bounds = bounds_of(self.me()).deflate(Vec3::new(0.001, 0.001, 0.001));
        let end = |value: f64| fire_cells_end(self.release(), value);
        let (min, max) = (bounds.min, bounds.max);
        let burns = (floor_i32_for(self.release(), min.x)..end(max.x)).any(|x| {
            (floor_i32_for(self.release(), min.y)..end(max.y))
                .any(|y| (floor_i32_for(self.release(), min.z)..end(max.z)).any(|z| fire_or_lava_block(self.world, (x, y, z))))
        });
        if burns {
            self.hurt(MinecartDamage::Fire, 1.0);
        }
    }

    /// Entity.lavaHurt with the draw of its sound, on a hurt that took (26.3 Entity.java:662-671;
    /// 1.20.4 Entity.java setOnFireFromLava). The sound and its draw come in 1.17: 1.16.2 - 1.16.5
    /// only hurt (1.16.5 Entity.java:563-568, 1.17 :489-496).
    fn lava_hurt(&mut self) {
        let takes = self.me().removed.is_some() || !self.me().invulnerable;
        self.hurt(MinecartDamage::Fire, 4.0);
        if takes && self.release().at_least(1, 17, 0) {
            let cart = self.cart();
            let mut random = EntityRandom(cart.random_seed);
            random.next_float();
            cart.random_seed = random.0;
        }
    }

    /// VehicleEntity.hurtServer (AbstractMinecart.hurt before 1.20.3) of the tick's own fire.
    fn hurt(&mut self, kind: MinecartDamage, amount: f32) {
        self.attack(MinecartHurt { kind, amount, creative: false, burning_arrow: None });
    }

    fn attack(&mut self, hurt: MinecartHurt) {
        // MinecartTNT.hurtServer: a burning arrow sets it off first, whatever comes of the hurt, from 1.19.3 by a blast
        // whose source is the arrow's shooter (1.19.2 MinecartTNT.java:64 explode(double); 1.19.3 MinecartTNT.java:69-70
        // DamageSource.explosion(this, source.getEntity()))
        if self.me().variant == MinecartVariant::Tnt {
            if let Some(speed_sqr) = hurt.burning_arrow {
                let creative = hurt.creative && self.at_least(1, 19, 3);
                self.explode(speed_sqr, creative);
            }
        }
        let release = self.release();
        let cart = self.cart();
        // Entity.isInvulnerableTo: `invulnerable` keeps off all but a creative player's hit, in every release (1.8.8
        // Entity.java:1639-1640, 1.20.3 Entity.java:2450-2454; 1.21.2 Entity.java:2713-2714 isInvulnerableToBase, 26.3 :3123-3124)
        if cart.removed.is_some() || (cart.invulnerable && !hurt.creative) {
            return;
        }
        cart.hurt_dir = -cart.hurt_dir;
        cart.hurt_time = 10;
        cart.damage += hurt.amount * 10.0;
        let cart_named = cart.custom_name;
        // shouldSourceDestroy, from 1.20.3 (1.20.4 VehicleEntity.java; 1.20.2 has neither)
        let always = release.at_least(1, 20, 3)
            && cart.variant == MinecartVariant::Tnt
            && (hurt.kind != MinecartDamage::Plain || hurt.burning_arrow.is_some());
        let beaten = cart.damage > 40.0;
        if release.at_least(1, 20, 3) {
            if always || (beaten && !hurt.creative) {
                self.destroy(hurt);
            } else if hurt.creative {
                self.remove(MinecartRemoval::Discarded);
            }
            return;
        }
        // before 1.20.3 a creative hit or a beaten cart ejects, then a creative hit discards only a
        // cart with no custom name (1.8.8 AbstractMinecart.java:204-214, 1.9.4 :219-226, 1.13.2
        // :223-230, 1.14.4 :121-128, 1.16.5 :295-302, 1.20.2 :232-239)
        if hurt.creative || beaten {
            self.eject();
            if hurt.creative && !cart_named {
                self.remove(MinecartRemoval::Discarded);
            } else {
                self.destroy(hurt);
            }
        }
    }

    /// VehicleEntity.destroy with MinecartTNT's: the fire or the speed that sets it off, its blast
    /// at once before 1.12 (1.8.8 MinecartTNT.java dropItems, 1.11.2 :89-99), from there a
    /// fuse of its random's two draws (1.12.2 :89-101, 26.3 MinecartTNT.java destroy).
    fn destroy(&mut self, hurt: MinecartHurt) {
        if self.me().variant != MinecartVariant::Tnt {
            self.remove(MinecartRemoval::Destroyed);
            return;
        }
        let speed_sqr = self.me().velocity.horizontal_length_squared();
        let ignites = hurt.kind != MinecartDamage::Plain || hurt.burning_arrow.is_some();
        let fast = speed_sqr >= f64::from(0.01_f32);
        if !self.at_least(1, 12, 0) {
            if ignites || fast {
                self.explode(speed_sqr, false);
            }
            self.remove(MinecartRemoval::Destroyed);
            return;
        }
        if !ignites && !fast {
            self.remove(MinecartRemoval::Destroyed);
        } else if self.me().fuse < 0 {
            self.prime_fuse(Some(hurt.creative));
            let mut random = EntityRandom(self.me().random_seed);
            let fuse = random.next_int(20) + random.next_int(20);
            let cart = self.cart();
            cart.random_seed = random.0;
            cart.fuse = fuse;
        }
    }

    /// MinecartTNT.primeFuse: 80, and an event for the clients; from 1.21.5 not under the game rule.
    /// From 1.21.5 the hurt that primed it, where there was one (`source`, whether its attacker is a
    /// creative player), is the source its blast keeps (1.21.5 MinecartTNT.java:146-159
    /// ignitionSource; a rail primes it with none, :130-134).
    fn prime_fuse(&mut self, source: Option<bool>) {
        // (1.21.4 MinecartTNT.java:138-139; 1.21.5 MinecartTNT.java:137-146)
        if self.at_least(1, 21, 5) && !self.rules.tnt_explodes {
            return;
        }
        let ignition = self.at_least(1, 21, 5);
        let cart = self.cart();
        cart.fuse = 80;
        if let (true, Some(creative)) = (ignition, source) {
            cart.creative_ignition = creative;
        }
        self.tick.primed = true;
    }

    /// MinecartTNT.tick, after super.tick (1.8.8 MinecartTNT.java tick, 26.3 :49-66): the
    /// fuse runs out, or a wall meets it fast. The fuse's blast keeps the ignition source from 1.21.5,
    /// the wall's from 1.21.9 (1.21.5 MinecartTNT.java:58 and :65 explode(var1) with none; 1.21.9
    /// MinecartTNT.java:65 explode(this.ignitionSource, var1)).
    fn tick_tnt(&mut self) {
        let fuse_source = self.at_least(1, 21, 5) && self.me().creative_ignition;
        let wall_source = self.at_least(1, 21, 9) && self.me().creative_ignition;
        let cart = self.cart();
        let speed_sqr = cart.velocity.horizontal_length_squared();
        if cart.fuse > 0 {
            cart.fuse -= 1;
        } else if cart.fuse == 0 {
            self.explode(speed_sqr, fuse_source);
        }
        if self.me().collided_horizontally {
            let speed_sqr = self.me().velocity.horizontal_length_squared();
            if speed_sqr >= f64::from(0.01_f32) {
                self.explode(speed_sqr, wall_source);
            }
        }
    }

    /// MinecartTNT.explode: a power of 4 and up to 1.5 for each block of speed under 5, by the
    /// cart's random (26.3 :119-131; 1.8.8 MinecartTNT.java explode). From 1.21.5 the game
    /// rule keeps it, and a primed cart goes without it. (1.21.4 MinecartTNT.java:103-108 explodes
    /// always; 1.21.5 MinecartTNT.java:110-116 asks RULE_TNT_EXPLODES.) `creative` is whether the
    /// blast's damage source has a creative player for its attacker, which its hurts take as theirs.
    fn explode(&mut self, speed_sqr: f64, creative: bool) {
        if self.at_least(1, 21, 5) && !self.rules.tnt_explodes {
            if self.me().fuse > -1 {
                self.remove(MinecartRemoval::Discarded);
            }
            return;
        }
        let speed = speed_sqr.sqrt().min(5.0);
        let mut random = EntityRandom(self.me().random_seed);
        let roll = random.next_double();
        let cart = self.cart();
        cart.random_seed = random.0;
        let power = f64::from(cart.explosion_power_base) + f64::from(cart.explosion_speed_factor) * roll * 1.5 * speed;
        let position = cart.position;
        let explosion = self.explosion_rays(position, power as f32);
        self.tick.explosions.push(explosion);
        // (the cart goes after Level.explode, 26.3 MinecartTNT.java:106-107; it is gone here first, so that a blast it sets
        // off in another cart, at once before 1.12, does not set it off again)
        self.remove(MinecartRemoval::Exploded);
        self.blast(position, power as f32, creative);
    }

    /// What a blast does to the entities around it: ServerExplosion.hurtEntities (26.3
    /// ServerExplosion.java:159-209; Explosion.explode before 1.21.2). The scene's other carts take
    /// the hurt (`attack`) and the push, the other entities the push alone; their own resistance is
    /// not given, so the attribute from 1.21 and blast protection before it are taken as none. The
    /// item entities take theirs last (`blast_items`). The blocks it breaks are the server's, after
    /// the entities (26.3 ServerExplosion.java:238-242, 1.8.8 Explosion.java finalizeExplosion), so
    /// no ray's block changes what an entity is seen through: `seen_percent` clips the world as the
    /// rays found it, and blast resistance only shortens the rays. From 1.21.11 a blast of no power
    /// (under 1.0E-5F) touches no entity (1.21.10 ServerExplosion.java:158; 1.21.11
    /// ServerExplosion.java hurtEntities `!(this.radius < 1.0E-5F)`). From 26.3 the cart's team keeps
    /// its hurt off its allies where it allows no friendly fire, though not the push (26.2
    /// ServerExplosion.java hurtEntities; 26.3 ServerExplosion.java:178 doTeamsAllowDamage, Entity.java:2558-2566).
    fn blast(&mut self, center: Vec3, power: f32, creative: bool) {
        let release = self.release();
        if release.at_least(1, 21, 11) && power < 1.0e-5 {
            return;
        }
        let radius = power * 2.0;
        let reach = f64::from(radius);
        let cell = |value: f64| f64::from(floor_i32_for(release, value));
        let query = Aabb::new(
            Vec3::new(cell(center.x - reach - 1.0), cell(center.y - reach - 1.0), cell(center.z - reach - 1.0)),
            Vec3::new(cell(center.x + reach + 1.0), cell(center.y + reach + 1.0), cell(center.z + reach + 1.0)),
        );
        let source = self.current;
        for index in 0..self.carts.len() {
            let cart = &self.carts[index];
            if index == source || cart.removed.is_some() || !bounds_of(cart).intersects(query) {
                continue;
            }
            let Some((damage, push)) = self.blast_hit(center, radius, cart.position, bounds_of(cart), HEIGHT * 0.85, false) else {
                continue;
            };
            // (the hurt before the push, 26.3 ServerExplosion.java:179-193, 1.8.8 Explosion.java:130-132)
            let allied = matches!((self.carts[source].team, self.carts[index].team), (Some(own), Some(theirs)) if own.id == theirs.id);
            let teams_allow = !release.at_least(26, 3, 0) || !allied || self.carts[source].team.is_some_and(|team| team.allow_friendly_fire);
            if teams_allow {
                let primed = self.tick.primed;
                self.current = index;
                self.attack(MinecartHurt { kind: MinecartDamage::Explosion, amount: damage, creative, burning_arrow: None });
                self.current = source;
                self.tick.primed = primed;
            }
            let cart = &mut self.carts[index];
            cart.velocity = blast_push(cart.velocity, push, release);
        }
        for index in 0..self.others.len() {
            let other = self.others[index];
            if other.removed || !other.bounds.intersects(query) {
                continue;
            }
            // (Entity's eye height unless its type says otherwise: 0.85 of its height, 26.3 EntityDimensions.java:16)
            let height = (other.bounds.max.y - other.bounds.min.y) as f32;
            let eye = other.eye_height.unwrap_or(height * 0.85);
            if let Some((_, push)) = self.blast_hit(center, radius, other.position, other.bounds, eye, other.living) {
                self.others[index].velocity = blast_push(other.velocity, push, release);
            }
        }
        self.blast_items(center, radius, query);
    }

    /// The item entities in a blast's reach: ItemEntity.hurt, then the push (26.3
    /// ServerExplosion.java:179-193, 1.8.8 Explosion.java:130-136). The hurt takes its health as a
    /// whole number and it is gone at none (1.8.8 ItemEntity.java:214-216, 1.21.2
    /// ItemEntity.java:302-304). An item is pushed as what is no LivingEntity, from its eye at 0.85
    /// of its height (1.20.5 EntityType ITEM eyeHeight 0.2125F; Entity.getEyeHeight's default
    /// before, 1.8.8 Entity.java). From 1.21.2 to 1.21.5 an item takes nothing of a blast whose
    /// source is in water (1.21.2 ItemEntity.java:313-315 ignoreExplosion and ServerExplosion.java:326-334
    /// shouldAffectBlocklikeEntities, 1.21.5 ServerExplosion.java:302-309 the same; 1.21.6
    /// ServerExplosion.java:302-308 no longer asks isInWater); before 1.21.2 ItemEntity has no
    /// ignoreExplosion of its own (1.21.1 ItemEntity.java).
    fn blast_items(&mut self, center: Vec3, radius: f32, query: Aabb) {
        if self.at_least(1, 21, 2) && !self.at_least(1, 21, 6) && self.me().in_water {
            return;
        }
        for index in 0..self.items.len() {
            let item = self.items[index];
            if item.removed || item.stack.count <= 0 || !item.bounds.intersects(query) {
                continue;
            }
            let eye = (item.bounds.max.y - item.bounds.min.y) as f32 * 0.85;
            let Some((damage, push)) = self.blast_hit(center, radius, item.position, item.bounds, eye, false) else {
                continue;
            };
            let release = self.release();
            let item = &mut self.items[index];
            if !item.blast_proof {
                item.health = (item.health as f32 - damage) as i32;
                item.removed = item.health <= 0;
            }
            item.velocity = blast_push(item.velocity, push, release);
        }
    }

    /// ServerExplosion.calculateExplodedPositions (26.3 ServerExplosion.java:110-155; 1.8.8
    /// Explosion.java:60-101 the same rays): 1352 rays from the centre over the faces of a 16 cube,
    /// each as strong as the radius by 0.7 to 1.3 of a level random draw, losing 0.3 of the
    /// resistance and 0.3 more in each block it steps through, 0.225 a step, and breaking each it is
    /// still strong in. The strength only shortens a ray or not, so the weakest draws break the
    /// blocks every draw does and the strongest all a draw could.
    fn explosion_rays(&mut self, center: Vec3, radius: f32) -> MinecartExplosion {
        // (LegacyRandomSource.nextFloat's lowest and highest, 26.3 BitRandomSource.java nextFloat)
        let weakest = self.rays(center, radius, || 0.0);
        let strongest = self.rays(center, radius, || 16_777_215.0 * 5.960_464_5e-8_f32);
        let mut level_random = (*self.level_random).map(EntityRandom);
        let blown = level_random.as_mut().map(|random| self.rays(center, radius, || random.next_float()));
        let after = level_random.map(|random| random.0);
        if after.is_some() {
            *self.level_random = after;
        }
        MinecartExplosion { position: center, power: radius, certain: weakest, possible: strongest, blown, level_random: after }
    }

    /// The blocks the rays of one set of draws reach, in order of their positions.
    fn rays(&self, center: Vec3, radius: f32, mut draw: impl FnMut() -> f32) -> Vec<BlockPosition> {
        let release = self.release();
        let mut blown = std::collections::BTreeSet::new();
        let axis = |at: i32| f64::from(at as f32 / 15.0 * 2.0 - 1.0);
        // (0.3F as a double: 1.8.8 Explosion.java:97-99 `d * 0.3F`, 26.3 :148-150 the literal)
        let step = f64::from(0.3_f32);
        for xx in 0..16 {
            for yy in 0..16 {
                for zz in 0..16 {
                    if !(xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15) {
                        continue;
                    }
                    let (mut xd, mut yd, mut zd) = (axis(xx), axis(yy), axis(zz));
                    let length = (xd * xd + yd * yd + zd * zd).sqrt();
                    xd /= length;
                    yd /= length;
                    zd /= length;
                    let mut strength = radius * (0.7 + draw() * 0.6);
                    let (mut x, mut y, mut z) = (center.x, center.y, center.z);
                    while strength > 0.0 {
                        let at = (floor_i32_for(release, x), floor_i32_for(release, y), floor_i32_for(release, z));
                        // (1.16.5 Explosion.java:150-156 reads on past the level; 1.17 Explosion.java:166-168 stops)
                        if release.at_least(1, 17, 0) && !self.in_world_bounds(at) {
                            break;
                        }
                        if let Some(resistance) = self.block_resistance(at) {
                            strength -= (resistance + 0.3) * 0.3;
                        }
                        if strength > 0.0 && !self.rail_spared(at) {
                            blown.insert(at);
                        }
                        x += xd * step;
                        y += yd * step;
                        z += zd * step;
                        strength -= 0.225_000_01;
                    }
                }
            }
        }
        blown.into_iter().collect()
    }

    /// Level.isInWorldBounds, which from 1.17 ends a ray (1.16.5 Explosion.java:150-166 reads on
    /// into the air beyond; 1.17 Explosion.java:166-168 breaks): in the build height, where the
    /// world has one, and within 30000000 across (1.17 Level.java:142-152).
    fn in_world_bounds(&self, (x, y, z): BlockPosition) -> bool {
        let bounded = self.world.top_y() > self.world.min_build_height();
        let across = -30_000_000..30_000_000;
        (!bounded || self.world.is_inside_build_height(y)) && across.contains(&x) && across.contains(&z)
    }

    /// The ray's loss in a block: ExplosionDamageCalculator.getBlockExplosionResistance through the
    /// TNT cart's own (26.3 ExplosionDamageCalculator.java:15-17, EntityBasedExplosionDamageCalculator
    /// and MinecartTNT.java:170-172). From 1.13 the greater of the block's and its fluid's (water's
    /// and lava's 100, 1.13.2 WaterFluid.java:113, LavaFluid.java:220), nothing for air with no
    /// fluid (1.13.2 Explosion.java:89-96); before it the block's, nothing for Material.AIR (1.12.2
    /// Explosion.java:85-90). A primed cart's blast loses nothing in a rail or the block under one
    /// (1.8.8 MinecartTNT.java:138-142, 1.13.2 :145-149).
    fn block_resistance(&self, at: BlockPosition) -> Option<f32> {
        let block = self.world.block_cow(at);
        let base = if self.at_least(1, 13, 0) {
            let fluid = (block.fluid != FluidState::Empty).then_some(100.0_f32);
            match (block.explosion_resistance, fluid) {
                (None, None) => None,
                (own, fluid) => Some(own.unwrap_or(0.0).max(fluid.unwrap_or(0.0))),
            }
        } else {
            block.explosion_resistance
        }?;
        Some(if self.rail_spared(at) { 0.0 } else { base })
    }

    /// MinecartTNT.isPrimed (a fuse above -1) with a rail at the block or over it: what the cart's
    /// getBlockExplosionResistance takes as 0 and shouldBlockExplode keeps (1.8.8
    /// MinecartTNT.java:138-149, 26.3 :170-176); every other block breaks where a ray is strong in it.
    fn rail_spared(&self, at: BlockPosition) -> bool {
        self.me().fuse > -1 && (is_rail(self.world, at) || is_rail(self.world, offset(at, (0, 1, 0))))
    }

    /// One entity of a blast's: the hurt it takes and its push, or nothing out of reach. The
    /// distance and the length of the way out are float square roots before 1.17 (1.16.5
    /// Explosion.java:187-192 Mth.sqrt; 1.17 Explosion.java:203-208 Math.sqrt), the distance a float
    /// quotient from 1.14 (1.13.2 Explosion.java:125 Entity.distanceTo, a double; 1.14.4
    /// Explosion.java:170 Mth.sqrt over the float radius). A way out of no length is no hit until
    /// 1.21.9, then a hurt with no push (1.21.8 ServerExplosion.java:158; 1.21.9
    /// ServerExplosion.java:172 Vec3.normalize). The hurt is whole up to 1.20.2, 8 a block of reach
    /// before 1.9 (1.8.8 Explosion.java:130, 1.9.4 Explosion.java:132, 1.20.2 Explosion.java:219;
    /// 1.20.3 Explosion.java:264 ExplosionDamageCalculator's float). What is no LivingEntity is pushed
    /// a whole block's worth in 1.9 and 1.10 (1.10.2 Explosion.java:133-134; 1.11.2 Explosion.java:134
    /// the impact).
    fn blast_hit(&self, center: Vec3, radius: f32, position: Vec3, bounds: Aabb, eye: f32, living: bool) -> Option<(f32, Vec3)> {
        let release = self.release();
        let reach = f64::from(radius);
        let distance_sqr = position.distance_squared(center);
        let distance = if !release.at_least(1, 14, 0) {
            f64::from(distance_sqr.sqrt() as f32) / reach
        } else if !release.at_least(1, 17, 0) {
            f64::from(distance_sqr.sqrt() as f32 / radius)
        } else {
            distance_sqr.sqrt() / reach
        };
        // (`d <= 1.0` until 1.21.9, which a NaN of a blast of no power at the entity's own place fails; `!(d > 1.0)` from it,
        // which the NaN passes: 1.21.8 ServerExplosion.java hurtEntities, 1.21.9 ServerExplosion.java:170)
        let in_reach = if release.at_least(1, 21, 9) { !(distance > 1.0) } else { distance <= 1.0 };
        if !in_reach {
            return None;
        }
        let way = Vec3::new(position.x - center.x, position.y + f64::from(eye) - center.y, position.z - center.z);
        let length = if release.at_least(1, 17, 0) {
            way.length_squared().sqrt()
        } else {
            f64::from(way.length_squared().sqrt() as f32)
        };
        let direction = if release.at_least(1, 21, 9) {
            if length < TINY {
                Vec3::ZERO
            } else {
                Vec3::new(way.x / length, way.y / length, way.z / length)
            }
        } else if length == 0.0 {
            return None;
        } else {
            Vec3::new(way.x / length, way.y / length, way.z / length)
        };
        let impact = (1.0 - distance) * f64::from(self.seen_percent(center, bounds));
        let hurt = (impact * impact + impact) / 2.0;
        let damage = if release.at_least(1, 20, 3) {
            (hurt * 7.0 * reach + 1.0) as f32
        } else {
            let per_block = if release.at_least(1, 9, 0) { 7.0 } else { 8.0 };
            (hurt * per_block * reach + 1.0) as i32 as f32
        };
        let knockback = if !living && release.at_least(1, 9, 0) && !release.at_least(1, 11, 0) { 1.0 } else { impact };
        Some((damage, Vec3::new(direction.x * knockback, direction.y * knockback, direction.z * knockback)))
    }

    /// Explosion.getSeenPercent: the share of points over the box from which nothing blocks the
    /// way to the blast's centre (26.3 ServerExplosion.java:69-100). The steps are floats until
    /// 1.18.2 (1.18.1 Explosion.java:115-117; 1.18.2 Explosion.java:118-120 doubles). The way is
    /// clipped by the blocks' outlines before 1.16, Level.rayTrace with nothing ignored and then
    /// ClipContext.Block.OUTLINE (1.8.8 Level.java:1657, 1.15.2 Explosion.java:95), by their collision
    /// shapes from 1.16 (1.16 Explosion.java:118 COLLIDER).
    fn seen_percent(&self, center: Vec3, bounds: Aabb) -> f32 {
        let release = self.release();
        let shape = if release.at_least(1, 16, 0) { ClipShape::Collider } else { ClipShape::Outline };
        let (min, max) = (bounds.min, bounds.max);
        let xs = 1.0 / ((max.x - min.x) * 2.0 + 1.0);
        let ys = 1.0 / ((max.y - min.y) * 2.0 + 1.0);
        let zs = 1.0 / ((max.z - min.z) * 2.0 + 1.0);
        let x_offset = (1.0 - (1.0 / xs).floor() * xs) / 2.0;
        let z_offset = (1.0 - (1.0 / zs).floor() * zs) / 2.0;
        if xs < 0.0 || ys < 0.0 || zs < 0.0 {
            return 0.0;
        }
        let doubles = release.at_least(1, 18, 2);
        let steps = |step: f64| -> Vec<f64> {
            let mut out = Vec::new();
            if doubles {
                let mut at = 0.0;
                while at <= 1.0 {
                    out.push(at);
                    at += step;
                }
            } else {
                let mut at = 0.0_f32;
                while at <= 1.0 {
                    out.push(f64::from(at));
                    at = (f64::from(at) + step) as f32;
                }
            }
            out
        };
        let (along_x, along_y, along_z) = (steps(xs), steps(ys), steps(zs));
        let (mut hits, mut count) = (0_i32, 0_i32);
        for &xx in &along_x {
            for &yy in &along_y {
                for &zz in &along_z {
                    let from = Vec3::new(
                        min.x + xx * (max.x - min.x) + x_offset,
                        min.y + yy * (max.y - min.y),
                        min.z + zz * (max.z - min.z) + z_offset,
                    );
                    if !clip_hits(self.world, from, center, release, shape) {
                        hits += 1;
                    }
                    count += 1;
                }
            }
        }
        hits as f32 / count as f32
    }

    /// MinecartHopper.tryConsumeItems (1.21.2+) and what its tick did before it (1.8.8
    /// MinecartHopper.java tick; whose cooldown 1.8.9 - 1.19.4 zeroes each tick, as the
    /// position it compares with is never set).
    fn try_consume_items(&mut self) {
        let cart = self.me();
        if cart.variant != MinecartVariant::Hopper || cart.removed.is_some() || !cart.enabled || self.consumed_item {
            return;
        }
        if self.suck_in_items() {
            self.consumed_item = true;
        }
    }

    /// MinecartHopper.suckInItems: HopperBlockEntity.suckInItems, then what lies around the cart.
    fn suck_in_items(&mut self) -> bool {
        if self.suck_from_above() {
            return true;
        }
        let around = bounds_of(self.me()).inflate(Vec3::new(0.25, 0.0, 0.25));
        // only the first until 1.19.4, which then says it took nothing (1.19.3 MinecartHopper.java:110-115 offers
        // only the first and returns false; 1.19.4 MinecartHopper.java:92-97 loops and returns true on the first it
        // takes, as 1.20 MinecartHopper.java:92-97 does)
        let first_alone = !self.at_least(1, 19, 4);
        for index in 0..self.items.len() {
            if self.items[index].stack.count > 0 && !self.items[index].removed && self.items[index].bounds.intersects(around) {
                let taken = self.take_item(index);
                if first_alone {
                    return false;
                }
                if taken {
                    return true;
                }
            }
        }
        false
    }

    /// HopperBlockEntity.suckInItems: the container above its top (a block's cell), or the items
    /// over it. The hopper's place is the cart's and half a block up (26.3 MinecartHopper.java).
    fn suck_from_above(&mut self) -> bool {
        let position = self.me().position;
        let top = position.y + 0.5;
        let above = (floor_i32_for(self.release(), position.x), floor_i32_for(self.release(), top + 1.0), floor_i32_for(self.release(), position.z));
        if let Some(container) = self.containers.iter().position(|container| container.position == above) {
            return self.take_from_container(container);
        }
        for area in self.suck_areas(position) {
            for index in 0..self.items.len() {
                if self.items[index].stack.count > 0 && !self.items[index].removed
                    && self.items[index].bounds.intersects(area)
                    && self.take_item(index)
                {
                    return true;
                }
            }
        }
        false
    }

    /// The boxes it draws items from: a block's worth over it (1.8.8 HopperBlockEntity.java:442), a
    /// block and a half (1.9.4 :430), Hopper's inside and above shapes (1.13 - 1.20.4
    /// Hopper.java), 11/16 to 32/16 of its block as one box from 1.20.5 (26.3 getSuckAabb).
    /// (1.12.2 HopperBlockEntity.java:383-384 one box a block and a half up; 1.13.2 Hopper.java:11-12
    /// INSIDE and ABOVE; 1.20.4 Hopper.java:9-11 still; 1.20.5 Hopper.java:8 the one SUCK_AABB.)
    fn suck_areas(&self, position: Vec3) -> Vec<Aabb> {
        let (x, y, z) = (position.x, position.y, position.z);
        let cuboid = |x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64| {
            Aabb::new(
                Vec3::new(x - 0.5 + x0 / 16.0, y + y0 / 16.0, z - 0.5 + z0 / 16.0),
                Vec3::new(x - 0.5 + x1 / 16.0, y + y1 / 16.0, z - 0.5 + z1 / 16.0),
            )
        };
        if self.at_least(1, 20, 5) {
            vec![cuboid(0.0, 11.0, 0.0, 16.0, 32.0, 16.0)]
        } else if self.at_least(1, 13, 0) {
            vec![cuboid(2.0, 11.0, 2.0, 14.0, 16.0, 14.0), cuboid(0.0, 16.0, 0.0, 16.0, 32.0, 16.0)]
        // (1.8.8 HopperBlockEntity.java:322,442; 1.9.4 HopperBlockEntity.java:312,430)
        } else if self.at_least(1, 9, 0) {
            vec![cuboid(0.0, 8.0, 0.0, 16.0, 32.0, 16.0)]
        } else {
            vec![cuboid(0.0, 16.0, 0.0, 16.0, 32.0, 16.0)]
        }
    }

    /// HopperBlockEntity.addItem(container, entity): true only when all of it went in.
    fn take_item(&mut self, index: usize) -> bool {
        let mut stack = self.items[index].stack;
        self.add_to_inventory(&mut stack);
        self.items[index].stack = stack;
        stack.count == 0
    }

    /// HopperBlockEntity#tryTakeInItemFromSlot, slot by slot: one item, where there is room for it.
    fn take_from_container(&mut self, container: usize) -> bool {
        for slot in 0..self.containers[container].slots.len() {
            let source = self.containers[container].slots[slot];
            if source.count <= 0 {
                continue;
            }
            let mut one = MinecartStack { count: 1, ..source };
            self.add_to_inventory(&mut one);
            if one.count == 0 {
                self.containers[container].slots[slot].count -= 1;
                return true;
            }
        }
        false
    }

    /// HopperBlockEntity.addItem(from, to, stack, side), slot by slot, and the comparator's signal
    /// its drag reads.
    fn add_to_inventory(&mut self, stack: &mut MinecartStack) {
        let release = self.release();
        let cart = self.cart();
        for slot in &mut cart.inventory {
            if stack.count <= 0 {
                break;
            }
            if slot.count <= 0 {
                *slot = *stack;
                stack.count = 0;
            } else if slot.count <= slot.max_stack && slot.item == stack.item {
                let moved = stack.count.min(stack.max_stack - slot.count);
                stack.count -= moved;
                slot.count += moved;
            }
        }
        cart.comparator_signal = comparator_signal(&cart.inventory, release);
    }

    /// Entity.updateFluidInteraction (updateWaterState): the water's current and whether the cart
    /// is in it, then from 1.16 the lava's current; the same as a boat's but for the box. Water ends
    /// a fall (1.8.8 Entity.java:842, 26.3 updateInWaterStateAndDoWaterCurrentPushing). Whether it
    /// is in lava.
    fn update_fluids(&mut self) -> bool {
        let release = self.release();
        let bounds = bounds_of(self.me());
        let world = self.world;
        let ultra_warm = self.me().ultra_warm;
        let cart = self.cart();
        let mut body = SimulationState {
            velocity: cart.velocity,
            bounding_box: Some(bounds),
            ..SimulationState::default()
        };
        if legacy_fluid::applies(release) {
            legacy_fluid::handle_water_movement(&mut body, world, true, release);
            cart.in_water = body.in_water;
            if cart.in_water {
                cart.fall_distance = 0.0;
            }
        } else {
            let water = water_contact(world, bounds, release);
            cart.in_water = water.touching;
            if water.touching {
                push_by_fluid(&mut body, water, 0.014, release);
                cart.fall_distance = 0.0;
            }
        }
        let mut in_lava = false;
        // (1.16 and 1.16.1 stop at the water, Entity.java:1042-1047; 1.16.2 measures both, :1068)
        // (1.15.2 Entity.java:1028-1043 has the water only; 1.16 Entity.java:1036-1044 adds the lava.)
        if release.at_least(1, 16, 2) || (release.at_least(1, 16, 0) && !cart.in_water) {
            let lava = world.fluid_contact(FluidState::Lava, bounds, release);
            in_lava = lava.touching;
            push_by_fluid(&mut body, lava, lava_flow_scale(ultra_warm), release);
        }
        cart.velocity = body.velocity;
        in_lava
    }

    /// Entity.applyGravity: 0.04F as read until 1.14, the double after, 0.005 in water from 1.17.
    /// NoGravity is read from 1.10.
    fn apply_gravity(&mut self) {
        let release = self.release();
        let cart = self.cart();
        // (1.9.4 AbstractMinecart.java:330; 1.10.2 AbstractMinecart.java:337-339)
        if cart.no_gravity && release.at_least(1, 10, 0) {
            return;
        }
        // (1.16.5 AbstractMinecart.java:372; 1.17 AbstractMinecart.java:307)
        cart.velocity.y -= if release.at_least(1, 17, 0) && cart.in_water {
            0.005
        // (1.13.2 AbstractMinecart.java:330; 1.14.4 AbstractMinecart.java:200)
        } else if release.at_least(1, 14, 0) {
            0.04
        } else {
            f64::from(0.04_f32)
        };
    }

    /// getCurrentBlockPosOrRailBelow (26.3 AbstractMinecart.java:243-257): its block, or the rail
    /// under it. The improved path reaches down by the height it rides above a rail.
    fn rail_position(&self) -> BlockPosition {
        let position = self.me().position;
        let release = self.release();
        let x = floor_i32_for(release, position.x);
        let mut y = floor_i32_for(release, position.y);
        let z = floor_i32_for(release, position.z);
        if self.improved() {
            let below = position.y - 0.1 - TINY;
            if is_rail(self.world, (x, floor_i32_for(release, below), z)) {
                y = floor_i32_for(release, below);
            }
        } else if is_rail(self.world, (x, y - 1, z)) {
            y -= 1;
        }
        (x, y, z)
    }

    /// getMaxSpeed: 0.4 and a furnace's 0.2 until 1.17; from there 8 and 4 blocks a second, 4 and
    /// 3 in water (1.17 AbstractMinecart.java:378, FurnaceMinecartEntity); from 1.21.2 the
    /// behaviour's speed, of which a furnace cart gets half, in water three quarters (26.3
    /// MinecartFurnace.java:61-63, OldMinecartBehavior.java:388-390,
    /// NewMinecartBehavior.java:460-462). (1.21.1 AbstractMinecart.java:316-317 and
    /// MinecartFurnace.java:69-70 count blocks a second; 1.21.2 AbstractMinecart.java:260-261 asks the
    /// behaviour, OldMinecartBehavior.java:444-445, and MinecartFurnace.java:65-66 scales it.)
    fn max_speed(&self) -> f64 {
        let cart = self.me();
        let furnace = cart.variant == MinecartVariant::Furnace;
        if self.at_least(1, 21, 2) {
            let base = if self.improved() {
                f64::from(self.rules.max_minecart_speed) * if cart.in_water { 0.5 } else { 1.0 } / 20.0
            } else if cart.in_water {
                0.2
            } else {
                0.4
            };
            return match (furnace, cart.in_water) {
                (true, true) => base * 0.75,
                (true, false) => base * 0.5,
                (false, _) => base,
            };
        }
        // (1.16.5 AbstractMinecart.java:441-443, MinecartFurnace.java:74-76; 1.17 AbstractMinecart.java:377-379, MinecartFurnace.java:73-75)
        if self.at_least(1, 17, 0) {
            let blocks = match (furnace, cart.in_water) {
                (true, true) => 3.0,
                (true, false) => 4.0,
                (false, true) => 4.0,
                (false, false) => 8.0,
            };
            return blocks / 20.0;
        }
        if furnace {
            0.2
        } else {
            0.4
        }
    }

    /// AbstractMinecart.applyNaturalSlowdown with the variants' overrides.
    fn natural_slowdown(&mut self, movement: Vec3) -> Vec3 {
        let variant = self.me().variant;
        if variant.holds_items() {
            return self.container_slowdown(movement);
        }
        let movement = if variant == MinecartVariant::Furnace {
            self.furnace_slowdown(movement)
        } else {
            movement
        };
        self.base_slowdown(movement)
    }

    fn base_slowdown(&self, movement: Vec3) -> Vec3 {
        let cart = self.me();
        let vehicle = cart.rider.is_some();
        // (26.3 NewMinecartBehavior.java:478)
        let factor = if self.improved() {
            if vehicle {
                0.997
            } else {
                0.975
            }
        // (1.13.2 AbstractMinecart.java:577-587; 1.14.4 AbstractMinecart.java:451-454)
        } else if self.at_least(1, 14, 0) {
            if vehicle {
                0.997
            } else {
                0.96
            }
        } else if vehicle {
            f64::from(0.997_f32)
        } else {
            f64::from(0.96_f32)
        };
        let slowed = Vec3::new(movement.x * factor, movement.y * 0.0, movement.z * factor);
        // (1.16.5 AbstractMinecart.java:623; 1.17 AbstractMinecart.java:565)
        if self.at_least(1, 17, 0) && cart.in_water {
            slowed.scale(0.949999988079071)
        } else {
            slowed
        }
    }

    /// AbstractMinecartContainer.applyNaturalSlowdown (1.8.8 AbstractMinecartContainer.java:167-173;
    /// 1.20.4 :101-113; 26.3 :65-77): nothing of the fullness while a loot table is still to be
    /// rolled, and water from 1.17 (1.16.5 AbstractMinecartContainer.java:176-183 has no water; 1.17
    /// AbstractMinecartContainer.java:184-186 takes 0.95F of it).
    fn container_slowdown(&self, movement: Vec3) -> Vec3 {
        let cart = self.me();
        let mut keep = 0.98_f32;
        if !cart.has_loot_table {
            keep += 15_i32.wrapping_sub(cart.comparator_signal) as f32 * 0.001_f32;
        }
        if self.at_least(1, 17, 0) && cart.in_water {
            keep *= 0.95_f32;
        }
        let keep = f64::from(keep);
        Vec3::new(movement.x * keep, movement.y * 0.0, movement.z * keep)
    }

    /// MinecartFurnace.applyNaturalSlowdown, before its super's.
    fn furnace_slowdown(&mut self, movement: Vec3) -> Vec3 {
        let in_water = self.me().in_water;
        let release = self.release();
        let cart = self.cart();
        let push = cart.push;
        // (1.21.1 MinecartFurnace.java:93-110; 1.21.2 MinecartFurnace.java:77-96)
        if release.at_least(1, 21, 2) {
            // 26.3 MinecartFurnace.java:72-87
            if push.length_squared() <= 1.0e-7 {
                return Vec3::new(movement.x * 0.98, 0.0, movement.z * 0.98);
            }
            // calculateNewPushAlong
            cart.push = if push.horizontal_length_squared() > 1.0e-4
                && movement.horizontal_length_squared() > 0.001
            {
                normalized(push.projected_on(movement)).scale(push.length())
            } else {
                push
            };
            let pushed = Vec3::new(movement.x * 0.8, movement.y * 0.0, movement.z * 0.8) + cart.push;
            return if in_water { pushed.scale(0.1) } else { pushed };
        }
        // 1.8.9 - 1.13.2: 1.0E-4 and floats; 1.14: 1.0E-7 and doubles; 1.17: the double square
        // root and water (1.8.8 MinecartFurnace.java applySlowdown, 1.14 :method_7525, 1.17)
        // (1.13.2 MinecartFurnace.java:102-115 against 1.0E-4 with 0.8F and 0.98F; 1.14.4
        // MinecartFurnace.java:101-107 against 1.0E-7 with doubles)
        let modern = release.at_least(1, 14, 0);
        let threshold = if modern { 1.0e-7 } else { 1.0e-4 };
        let squared = push.x * push.x + push.z * push.z;
        let (keep, coast) = if modern {
            (0.8, 0.98)
        } else {
            (f64::from(0.8_f32), f64::from(0.98_f32))
        };
        if squared <= threshold {
            return Vec3::new(movement.x * coast, movement.y * 0.0, movement.z * coast);
        }
        let length = if release.at_least(1, 17, 0) {
            squared.sqrt()
        } else {
            java_float(squared.sqrt())
        };
        cart.push.x /= length;
        cart.push.z /= length;
        let pushed = Vec3::new(
            movement.x * keep + cart.push.x,
            movement.y * 0.0,
            movement.z * keep + cart.push.z,
        );
        // (1.16.5 MinecartFurnace.java:109; 1.17 MinecartFurnace.java:110)
        if release.at_least(1, 17, 0) && in_water {
            pushed.scale(0.1)
        } else {
            pushed
        }
    }

    /// MinecartFurnace.moveAlongTrack (1.8.9 - 1.21.1), after its super's: the push drops against
    /// the motion until 1.15.2, and follows it after; gone from 1.21.2. (1.15.1
    /// MinecartFurnace.java:94-101 drops or rescales it; 1.15.2 MinecartFurnace.java:93-97 turns it
    /// along the motion.)
    fn furnace_after_track(&mut self) {
        // (1.21.1 MinecartFurnace.java:77; 1.21.2 MinecartFurnace.java:92)
        if self.me().variant != MinecartVariant::Furnace || self.at_least(1, 21, 2) {
            return;
        }
        let release = self.release();
        let max = self.max_speed();
        let cart = self.cart();
        let velocity = cart.velocity;
        let squared = velocity.x * velocity.x + velocity.z * velocity.z;
        let push_squared = cart.push.x * cart.push.x + cart.push.z * cart.push.z;
        if push_squared <= 1.0e-4 || squared <= 0.001 {
            return;
        }
        let float_root = |value: f64| {
            // (1.16.5 MinecartFurnace.java:95; 1.17 MinecartFurnace.java:94)
            if release.at_least(1, 17, 0) {
                value.sqrt()
            } else {
                java_float(value.sqrt())
            }
        };
        if release.at_least(1, 15, 2) {
            let speed = float_root(squared);
            let push = float_root(push_squared);
            cart.push.x = velocity.x / speed * push;
            cart.push.z = velocity.z / speed * push;
            return;
        }
        let length = float_root(push_squared);
        cart.push.x /= length;
        cart.push.z /= length;
        if cart.push.x * velocity.x + cart.push.z * velocity.z < 0.0 {
            cart.push.x = 0.0;
            cart.push.z = 0.0;
        } else {
            let scale = length / max;
            cart.push.x *= scale;
            cart.push.z *= scale;
        }
    }

    fn body(&self) -> SimulationState {
        let cart = self.me();
        SimulationState {
            position: cart.position,
            velocity: cart.velocity,
            half_width: f64::from(HALF_WIDTH),
            height: f64::from(HEIGHT),
            on_ground: cart.on_ground,
            collided_horizontally: cart.collided_horizontally,
            collided_vertically: cart.collided_vertically,
            controlled_by_local_instance: true,
            // (1.16.5 Entity.java:630-633; 1.17 Entity.java:558-560)
            bounding_box: (!self.at_least(1, 17, 0)).then(|| bounds_of(cart)),
            stuck_speed_multiplier: cart.stuck_speed_multiplier,
            sweet_berry_bush_immune: true,
            fall_distance: cart.fall_distance,
            ..SimulationState::default()
        }
    }

    fn take(&mut self, body: &SimulationState) {
        let cart = self.cart();
        cart.position = body.position;
        cart.velocity = body.velocity;
        cart.on_ground = body.on_ground;
        cart.collided_horizontally = body.collided_horizontally;
        cart.collided_vertically = body.collided_vertically;
        cart.stuck_speed_multiplier = body.stuck_speed_multiplier;
        cart.move_out_of_range |= body.move_out_of_range;
        cart.bounding_box = body.bounding_box;
        // (what a web or a honey side does to a fall)
        cart.fall_distance = body.fall_distance;
    }

    /// The boxes a move of this cart collides with (Entity.collide's getEntityCollisions under
    /// AbstractMinecart.canCollideWith: 1.8.8 AbstractMinecart.java:165-168, 26.3 :118).
    fn entity_boxes(&self, movement: Vec3) -> Vec<Aabb> {
        let swept = bounds_of(self.me()).expand_towards(movement);
        let carts = self.carts.iter().enumerate()
            .filter(|(index, cart)| *index != self.current && cart.removed.is_none())
            .map(|(_, cart)| bounds_of(cart));
        // (before 1.9 the rider is no exception: Level.doesBoxCollide compares it with the list, 1.8.8 Level.java:1085)
        // (1.8.8 Level.java:1084 compares arg.rider with the list itself; 1.9.4 Level.java:1102 leaves out
        // what shares the vehicle, isPassengerOfSameVehicle)
        let riders_collide = !self.at_least(1, 9, 0);
        let release = self.release();
        let others = self.others.iter().enumerate()
            .filter(|(index, other)| {
                !other.removed
                    && (other.pushable || other.solid)
                    && (riders_collide || !shares_cart(&self.others, *index, self.current, release))
            })
            .map(|(_, other)| other.bounds);
        carts.chain(others).filter(|bounds| bounds.intersects(swept)).collect()
    }

    /// Entity.move of a cart: from 1.21.2 AbstractMinecart.move, which the improved path follows
    /// with pushAndPickupEntities and what is left of a move a rider it picks up stopped, and the
    /// old path with the blocks at where it ends (26.3 AbstractMinecart.java:330-347). (1.21.1
    /// AbstractMinecart.java has no move of its own; 1.21.2 AbstractMinecart.java:342-356 is it.)
    fn move_cart(&mut self, delta: Vec3) -> VehicleMove {
        if !self.improved() {
            let outcome = self.super_move(delta);
            if self.at_least(1, 21, 2) {
                self.apply_effects_here();
                self.recorded.clear();
            }
            return outcome;
        }
        let to_position = self.me().position + delta;
        let outcome = self.super_move(delta);
        if self.push_and_pickup_improved() {
            let rest = to_position - self.me().position;
            return self.super_move(rest);
        }
        outcome
    }

    /// Entity.move of a cart. A rail keeps its speed from 1.16 (1.15.2 AbstractMinecart.java has no
    /// getBlockSpeedFactor; 1.16 AbstractMinecart.java:306-308 gives 1 on a rail). Until 1.21.2 the
    /// move checks the blocks inside itself (1.21.1 Entity.java:738), from it the caller does
    /// (1.21.2 AbstractMinecart.java:356).
    fn super_move(&mut self, delta: Vec3) -> VehicleMove {
        let world = self.world;
        let release = self.release();
        let mut body = self.body();
        let from = body.position;
        let entities = self.entity_boxes(delta);
        // The improved cart's collision context: nothing below its rail, nor ahead up a slope
        // (26.3 MinecartCollisionContext.java:20-45)
        let ignored = if self.improved() { self.ignored_blocks() } else { [None, None] };
        // (what a landing's bounce works out from 26.2: AbstractMinecart.getDefaultGravity, getAirDrag)
        let rules = MoveRules {
            rails_keep_speed: release.at_least(1, 16, 0),
            ignored,
            gravity: if self.me().in_water { 0.005 } else { 0.04 },
            air_drag: self.air_drag(),
            ..MoveRules::MINECART
        };
        let rules = MoveRules::with_gravity_of(rules, self.me().no_gravity);
        // (a web or a honey side stops the fall the move's end counts, 1.20.4 Entity.java move, whatever was in it)
        let stopped = std::cell::Cell::new(false);
        let outcome = move_vehicle_among(
            &mut body,
            world,
            release,
            delta,
            rules,
            &entities,
            |body, support| {
                let fall = std::mem::replace(&mut body.fall_distance, 1.0);
                apply_effects_inside(body, world, support, release);
                stopped.set(body.fall_distance == 0.0);
                body.fall_distance = fall;
            },
        );
        self.support = outcome.support;
        let fall = self.me().fall_distance;
        // (1.21.4 Entity.java:800; 1.21.5 Entity.java:755)
        if release.at_least(1, 21, 5) {
            let moved_length = outcome.moved.length_squared();
            if moved_length > 1.0e-7 || outcome.requested.length_squared() - moved_length < 1.0e-7 {
                self.recorded.push(Movement {
                    from,
                    to: from + outcome.moved,
                    // (1.21.8 Entity.java:772; 1.21.9 Entity.java:785)
                    axis_order: Some(if release.at_least(1, 21, 9) { outcome.requested } else { outcome.moved }),
                });
            }
        }
        self.take(&body);
        self.fall_after_move(outcome.moved.y, world, fall);
        if stopped.get() {
            self.cart().fall_distance = 0.0;
        }
        if !release.at_least(1, 21, 2) {
            // the move's own checkInsideBlocks and, before 1.16, its fire test
            let body = self.body();
            let inside: Vec<_> = blocks_inside(&body, release).into_iter().map(|position| (position, true)).collect();
            self.contact_hurts(&inside);
            self.burn_in_move();
        }
        outcome
    }

    /// Entity.checkFallDamage (Entity.fall before 1.21.2): down it counts, and landing ends it with
    /// the block's fallOn. The field is a double from 1.21.5, whose water counts nothing; the
    /// float's sum is of floats from 1.19 (1.18 and 1.19 Entity.java fall; 1.21.5
    /// Entity.java:1321-1336).
    fn fall_after_move(&mut self, moved_y: f64, world: &WorldSnapshot, fall: f64) {
        let release = self.release();
        let body = self.body();
        let on_ground = body.on_ground;
        let landed_on = world.effect_at(landing_position(world, &body, self.support, release));
        let mut fall = fall;
        if release.at_least(1, 21, 5) && !self.me().in_water && moved_y < 0.0 {
            fall -= f64::from(moved_y as f32);
        }
        if on_ground {
            if fall > 0.0 {
                self.land(fall, landed_on);
            }
            fall = 0.0;
        // (1.21.4 Entity.java:1295-1297; 1.21.5 Entity.java:1322-1324)
        } else if !release.at_least(1, 21, 5) && moved_y < 0.0 {
            // (d2f, fsub in the 1.18.2 jar: accumulated_fall_distance)
            // (1.18.1 Entity.java:1019; 1.18.2 Entity.java:991)
            fall = if release.at_least(1, 18, 2) {
                f64::from(fall as f32 - moved_y as f32)
            } else {
                f64::from((fall - moved_y) as f32)
            };
        }
        self.cart().fall_distance = fall;
    }

    /// Block.fallOn as far as it gives a cart a fall: a bed's is half from 1.12 (1.12.2
    /// BedBlock.java, 26.3 BedBlock.java fallOn), a dripstone's point longer (26.3
    /// PointedDripstoneBlock.java:52-60), and powder snow gives none (26.3 PowderSnowBlock.java:81-87).
    /// Only MinecartTNT.causeFallDamage takes it up (26.3 :133-140, 1.8.8 :107-114). (1.11.2 BedBlock
    /// has no fallOn; 1.12.2 BedBlock.java:147-148 halves it. 1.21.4 MinecartTNT.java:113-116 squares a
    /// float; 1.21.5 MinecartTNT.java:121-124 a double.)
    fn land(&mut self, fall: f64, on: BlockEffect) {
        let fall = match on {
            BlockEffect::Bed if self.at_least(1, 12, 0) => fall * 0.5,
            BlockEffect::PointedDripstoneTip => fall + 2.5,
            BlockEffect::PowderSnow => return,
            _ => fall,
        };
        if self.me().variant != MinecartVariant::Tnt || fall < 3.0 {
            return;
        }
        let power = if self.at_least(1, 21, 5) {
            let power = fall / 10.0;
            power * power
        } else {
            let power = fall as f32 / 10.0;
            f64::from(power * power)
        };
        // (from 1.21.9 by the ignition source, 1.21.9 MinecartTNT.java:121)
        let creative = self.at_least(1, 21, 9) && self.me().creative_ignition;
        self.explode(power, creative);
    }

    /// MinecartCollisionContext#setupContext: on a rail, the block below it and, on a slope, the one
    /// ahead up it.
    fn ignored_blocks(&self) -> [Option<BlockPosition>; 2] {
        let position = self.rail_position();
        let Some(rail) = rail_at(self.world, position) else {
            return [None, None];
        };
        let slope = match rail.shape {
            RailShape::AscendingEast => Some(offset(position, (1, 0, 0))),
            RailShape::AscendingWest => Some(offset(position, (-1, 0, 0))),
            RailShape::AscendingNorth => Some(offset(position, (0, 0, -1))),
            RailShape::AscendingSouth => Some(offset(position, (0, 0, 1))),
            _ => None,
        };
        [Some(offset(position, (0, -1, 0))), slope]
    }

    /// Entity.applyEffectsFromBlocks(position, position): the old path from 1.21.2.
    fn apply_effects_here(&mut self) {
        let position = self.me().position;
        self.apply_effects(&[Movement { from: position, to: position, axis_order: None }]);
    }

    fn apply_effects(&mut self, movements: &[Movement]) {
        let world = self.world;
        let release = self.release();
        let mut body = self.body();
        let (inside, effects) = blocks_inside_moves(&body, world, movements, release);
        apply_block_effects(&mut body, world, self.support, release, &inside);
        self.take(&body);
        self.contact_hurts(&inside);
        // LavaFluid.entityInside, a cell at a time (1.21.5 LavaFluid.java:107-111)
        for _ in 0..effects.lava_hurts {
            if self.me().removed.is_none() {
                self.lava_hurt();
            }
        }
    }

    /// Entity#checkInsideBlocks (MCP's checkBlockCollision) at the end of the tick, before 1.21.2 (after the move's own).
    fn check_block_collision(&mut self) {
        let world = self.world;
        let release = self.release();
        let mut body = self.body();
        let inside: Vec<_> = blocks_inside(&body, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_inside_block_effects(&mut body, world, release, &inside);
        self.take(&body);
        self.contact_hurts(&inside);
    }

    // The old path: AbstractMinecart.tick to 1.21.1, OldMinecartBehavior.tick from 1.21.2.

    fn tick_old(&mut self) {
        self.apply_gravity();
        let position = self.rail_position();
        let rail = rail_at(self.world, position);
        self.cart().on_rails = rail.is_some();
        if let Some(rail) = rail {
            self.move_along_track_old(position, rail);
            self.furnace_after_track();
            if rail.activator {
                self.activate(rail.powered);
            }
        } else {
            self.come_off_track();
        }
        // (1.21.1 AbstractMinecart.java:256; 1.21.2 OldMinecartBehavior.java:102)
        if self.at_least(1, 21, 2) {
            self.apply_effects_here();
            self.recorded.clear();
        } else {
            self.check_block_collision();
        }
        self.rotate_old();
        self.push_and_pickup_old();
    }

    /// AbstractMinecart.activateMinecart: a rideable cart throws its rider off a powered rail and
    /// takes a hurt of 50 that breaks nothing (1.8.8 Minecart.java, 26.3 Minecart.java:41-55),
    /// a TNT cart primes (26.3 MinecartTNT.java), a hopper cart stops (26.3 MinecartHopper.java:44-50)
    /// and a command block cart runs its command, 4 ticks apart (26.3 MinecartCommandBlock.java).
    fn activate(&mut self, powered: bool) {
        self.tick.activator = Some(powered);
        match self.me().variant {
            MinecartVariant::Rideable if powered => {
                self.eject();
                let cart = self.cart();
                if cart.hurt_time == 0 {
                    cart.hurt_dir = -cart.hurt_dir;
                    cart.hurt_time = 10;
                    cart.damage = 50.0;
                }
            }
            MinecartVariant::Tnt if powered && self.me().fuse < 0 => self.prime_fuse(None),
            MinecartVariant::Hopper => self.cart().enabled = !powered,
            MinecartVariant::CommandBlock if powered => {
                let cart = self.cart();
                if cart.tick_count - cart.last_activated >= 4 {
                    cart.last_activated = cart.tick_count;
                    self.tick.command_ran = true;
                }
            }
            _ => {}
        }
    }

    /// The tick's yaw: along the way it moved, turned over by `flipped`, and flipped when it turns
    /// about (26.3 OldMinecartBehavior.java:67-84; 1.8.8 AbstractMinecart.java:335-350).
    fn rotate_old(&mut self) {
        let old_y_rot = self.old_y_rot;
        let old = self.old_position;
        let release = self.release();
        let cart = self.cart();
        cart.x_rot = 0.0;
        let dx = old.x - cart.position.x;
        let dz = old.z - cart.position.z;
        if dx * dx + dz * dz > 0.001 {
            cart.y_rot = (atan2_vanilla(dz, dx, release) * 180.0 / std::f64::consts::PI) as f32;
            if cart.flipped {
                cart.y_rot += 180.0;
            }
        }
        let turned = f64::from(wrap_degrees_f32(cart.y_rot - old_y_rot));
        if turned < -170.0 || turned >= 170.0 {
            cart.y_rot += 180.0;
            cart.flipped = !cart.flipped;
        }
        cart.x_rot %= 360.0;
        cart.y_rot %= 360.0;
    }

    /// AbstractMinecart.comeOffTrack (moveOffRail).
    fn come_off_track(&mut self) {
        let max = self.max_speed();
        let release = self.release();
        let cart = self.cart();
        cart.velocity.x = clamp_f64_for(release, cart.velocity.x, -max, max);
        cart.velocity.z = clamp_f64_for(release, cart.velocity.z, -max, max);
        if cart.on_ground {
            cart.velocity = cart.velocity.scale(0.5);
        }
        let movement = cart.velocity;
        self.move_cart(movement);
        let air_drag = self.air_drag();
        let cart = self.cart();
        if !cart.on_ground {
            cart.velocity = cart.velocity.scale(air_drag);
        }
    }

    /// The air's 0.95: a float before 1.14, a double to 26.1 (1.21.11 AbstractMinecart.java:312),
    /// the float getAirDrag from 26.2 (26.3 AbstractMinecart.java:317, 322). (1.13.2
    /// AbstractMinecart.java:412-414 0.95F; 1.14.4 AbstractMinecart.java:283 0.95D; 26.1.2
    /// AbstractMinecart.java:387 the double; 26.2 AbstractMinecart.java:312,317-318 getAirDrag's float.)
    fn air_drag(&self) -> f64 {
        if !self.at_least(1, 14, 0) || self.at_least(26, 2, 0) {
            f64::from(0.95_f32)
        } else {
            0.95
        }
    }

    /// The rider's way of starting a resting cart, which a halting rail then does not stop. Before
    /// 1.14 any living rider's forward input until 1.13, a player's from it (1.12.2
    /// AbstractMinecart.java:473-475 LivingEntity; 1.13.2 AbstractMinecart.java:468-470 Player).
    fn rider_start_old(&mut self, halting: &mut bool) {
        let release = self.release();
        let cart = self.cart();
        let Some(rider) = cart.rider else {
            return;
        };
        let own = cart.velocity.horizontal_length_squared();
        // (1.21.1 AbstractMinecart.java:393-402; 1.21.2 OldMinecartBehavior.java:192-208)
        if release.at_least(1, 21, 2) {
            // 26.3 OldMinecartBehavior.java:153-169
            let intent = rider.move_intent;
            if rider.player && intent.length_squared() > 0.0 && normalized(intent).length_squared() > 0.0 && own < 0.01 {
                cart.velocity = cart.velocity.add_components(intent.x * 0.001, 0.0, intent.z * 0.001);
                *halting = false;
            }
        } else if release.at_least(1, 14, 0) {
            // 1.14.4 AbstractMinecart.java:336-341, 1.21 :397-401
            let own_rider = rider.velocity;
            if rider.player && own_rider.horizontal_length_squared() > 1.0e-4 && own < 0.01 {
                cart.velocity = cart.velocity.add_components(own_rider.x * 0.1, 0.0, own_rider.z * 0.1);
                *halting = false;
            }
        } else if (if release.at_least(1, 13, 0) { rider.player } else { rider.living }) && rider.forward > 0.0 {
            // 1.8.8 AbstractMinecart.java:443-455; 1.9.4 :452-462, as one constant
            let radians = if release.at_least(1, 9, 0) {
                rider.yaw * (std::f64::consts::PI / 180.0) as f32
            } else {
                rider.yaw * std::f32::consts::PI / 180.0
            };
            // Math.sin and cos of the JVM the release declares (B315)
            let x = -crate::math::math_sin(f64::from(radians), release);
            let z = crate::math::math_cos(f64::from(radians), release);
            if own < 0.01 {
                cart.velocity.x += x * 0.1;
                cart.velocity.z += z * 0.1;
                *halting = false;
            }
        }
    }

    /// 26.3 OldMinecartBehavior.moveAlongTrack (1.8.8 AbstractMinecart.moveAlongTrack).
    fn move_along_track_old(&mut self, pos: BlockPosition, rail: Rail) {
        let world = self.world;
        // (1.13.2 AbstractMinecart.java:485, 540; 1.14.4 AbstractMinecart.java:350, 399)
        let modern = self.at_least(1, 14, 0);
        self.cart().fall_distance = 0.0;
        let position = self.me().position;
        let old_snap = snap_to_rail(world, position, self.release());
        let (x, z) = (position.x, position.z);
        let mut y = f64::from(pos.1);
        let (power_track, mut halt_track) = if rail.powered_rail {
            (rail.powered, !rail.powered)
        } else {
            (false, false)
        };
        let mut slide = 0.0078125;
        // (1.16.5 AbstractMinecart.java:477; 1.17 AbstractMinecart.java:412-415)
        if self.at_least(1, 17, 0) && self.me().in_water {
            slide *= 0.2;
        }
        {
            let cart = self.cart();
            match rail.shape {
                RailShape::AscendingEast => cart.velocity.x -= slide,
                RailShape::AscendingWest => cart.velocity.x += slide,
                RailShape::AscendingNorth => cart.velocity.z += slide,
                RailShape::AscendingSouth => cart.velocity.z -= slide,
                _ => {}
            }
        }
        if rail.shape.is_slope() {
            y += 1.0;
        }
        let (exit0, exit1) = rail.shape.exits();
        let mut x_d = f64::from(exit1.0 - exit0.0);
        let mut z_d = f64::from(exit1.2 - exit0.2);
        let length = (x_d * x_d + z_d * z_d).sqrt();
        {
            let cart = self.cart();
            if cart.velocity.x * x_d + cart.velocity.z * z_d < 0.0 {
                x_d = -x_d;
                z_d = -z_d;
            }
            let pow = cart.velocity.horizontal_length().min(2.0);
            cart.velocity.x = pow * x_d / length;
            cart.velocity.z = pow * z_d / length;
        }
        self.rider_start_old(&mut halt_track);
        if halt_track {
            let cart = self.cart();
            let speed = cart.velocity.horizontal_length();
            if speed < 0.03 {
                cart.velocity = if modern { Vec3::ZERO } else { cart.velocity.scale(0.0) };
            } else {
                cart.velocity.x *= 0.5;
                cart.velocity.y *= 0.0;
                cart.velocity.z *= 0.5;
            }
        }
        let x0 = f64::from(pos.0) + 0.5 + f64::from(exit0.0) * 0.5;
        let z0 = f64::from(pos.2) + 0.5 + f64::from(exit0.2) * 0.5;
        let x1 = f64::from(pos.0) + 0.5 + f64::from(exit1.0) * 0.5;
        let z1 = f64::from(pos.2) + 0.5 + f64::from(exit1.2) * 0.5;
        let x_d = x1 - x0;
        let z_d = z1 - z0;
        let progress = if x_d == 0.0 {
            z - f64::from(pos.2)
        } else if z_d == 0.0 {
            x - f64::from(pos.0)
        } else {
            ((x - x0) * x_d + (z - z0) * z_d) * 2.0
        };
        self.set_pos(Vec3::new(x0 + x_d * progress, y, z0 + z_d * progress));
        let scale = if self.me().rider.is_some() { 0.75 } else { 1.0 };
        let max = self.max_speed();
        let velocity = self.me().velocity;
        self.move_cart(Vec3::new(
            clamp_f64_for(self.release(), scale * velocity.x, -max, max),
            0.0,
            clamp_f64_for(self.release(), scale * velocity.z, -max, max),
        ));
        let at = self.me().position;
        let cell = |value: f64, origin: i32| floor_i32_for(self.release(), value).wrapping_sub(origin);
        let (cell_x, cell_z) = (cell(at.x, pos.0), cell(at.z, pos.2));
        if exit0.1 != 0 && cell_x == exit0.0 && cell_z == exit0.2 {
            self.set_pos(at.add_components(0.0, f64::from(exit0.1), 0.0));
        } else if exit1.1 != 0 && cell_x == exit1.0 && cell_z == exit1.2 {
            self.set_pos(at.add_components(0.0, f64::from(exit1.1), 0.0));
        }
        let slowed = self.natural_slowdown(self.me().velocity);
        self.cart().velocity = slowed;
        let at = self.me().position;
        if let (Some(new_snap), Some(old_snap)) = (snap_to_rail(world, at, self.release()), old_snap) {
            let speed = (old_snap.y - new_snap.y) * 0.05;
            let cart = self.cart();
            let pow = cart.velocity.horizontal_length();
            if pow > 0.0 && modern {
                cart.velocity.x *= (pow + speed) / pow;
                cart.velocity.z *= (pow + speed) / pow;
            } else if pow > 0.0 {
                // (1.13.2 AbstractMinecart.java: the two divide before they multiply, 1.14.4 Vec3.multiply)
                cart.velocity.x = cart.velocity.x / pow * (pow + speed);
                cart.velocity.z = cart.velocity.z / pow * (pow + speed);
            }
            self.set_pos(Vec3::new(at.x, new_snap.y, at.z));
        }
        let at = self.me().position;
        let (xn, zn) = (floor_i32_for(self.release(), at.x), floor_i32_for(self.release(), at.z));
        if xn != pos.0 || zn != pos.2 {
            let cart = self.cart();
            let pow = cart.velocity.horizontal_length();
            cart.velocity.x = pow * f64::from(xn.wrapping_sub(pos.0));
            cart.velocity.z = pow * f64::from(zn.wrapping_sub(pos.2));
        }
        if power_track {
            self.power_track_old(pos, rail.shape);
        }
    }

    /// The powered rail's end of moveAlongTrack: 0.06 along the motion, or off the block at its end
    /// (AbstractMinecart#moveAlongTrack, OldMinecartBehavior#moveAlongTrack from 1.21.2).
    fn power_track_old(&mut self, pos: BlockPosition, shape: RailShape) {
        let world = self.world;
        let cart = self.cart();
        let speed = cart.velocity.horizontal_length();
        if speed > 0.01 {
            cart.velocity.x += cart.velocity.x / speed * 0.06;
            cart.velocity.z += cart.velocity.z / speed * 0.06;
            return;
        }
        match shape {
            RailShape::EastWest => {
                if conductor(world, offset(pos, (-1, 0, 0))) {
                    cart.velocity.x = 0.02;
                } else if conductor(world, offset(pos, (1, 0, 0))) {
                    cart.velocity.x = -0.02;
                }
            }
            RailShape::NorthSouth => {
                if conductor(world, offset(pos, (0, 0, -1))) {
                    cart.velocity.z = 0.02;
                } else if conductor(world, offset(pos, (0, 0, 1))) {
                    cart.velocity.z = -0.02;
                }
            }
            _ => {}
        }
    }

    /// OldMinecartBehavior.pushAndPickupEntities (26.3 :356-378), the end of AbstractMinecart.tick
    /// from 1.10 (1.10.2 :376-394). Before that a cart pushes other carts alone and a rideable
    /// cart picks its riders up when they push it (1.8.8 :351-355, :672-682).
    fn push_and_pickup_old(&mut self) {
        let release = self.release();
        let hitbox = bounds_of(self.me()).inflate(Vec3::new(f64::from(0.2_f32), 0.0, f64::from(0.2_f32)));
        let moving = self.me().velocity.horizontal_length_squared();
        // (>= 0.01 from 1.21.2, 26.3 OldMinecartBehavior.java:358; 1.21 :300 has >)
        // (1.21.1 AbstractMinecart.java:274 > 0.01; 1.21.2 OldMinecartBehavior.java:404 >= 0.01)
        let fast = if self.at_least(1, 21, 2) { moving >= 0.01 } else { moving > 0.01 };
        let picks_up = self.me().variant == MinecartVariant::Rideable && fast;
        if release.at_least(1, 10, 0) && picks_up {
            // (EntitySelector.pushableBy(this) of what it meets, carts too: 1.10.2 AbstractMinecart.java:378, 26.3
            // OldMinecartBehavior.java:359)
            let team = self.me().team;
            for index in 0..self.others.len() {
                let other = self.others[index];
                if other.removed || !other.pushable || !other.bounds.intersects(hitbox) || !pushable_by(release, team, other.team) {
                    continue;
                }
                if !other.player && !other.iron_golem && self.me().rider.is_none() && !other.passenger {
                    // (startRiding, which a boarding cooldown refuses: then it is neither picked up nor pushed, 1.10.2
                    // AbstractMinecart.java:382-383)
                    if other.can_board(release) {
                        self.pick_up(index);
                    }
                } else {
                    self.push_other(index);
                }
            }
            for index in 0..self.carts.len() {
                if index != self.current
                    && self.carts[index].removed.is_none()
                    && bounds_of(&self.carts[index]).intersects(hitbox)
                    && pushable_by(release, team, self.carts[index].team)
                {
                    self.cart_pushed_by_me(index);
                }
            }
            return;
        }
        for index in 0..self.carts.len() {
            if index != self.current && self.carts[index].removed.is_none() && bounds_of(&self.carts[index]).intersects(hitbox) {
                self.cart_pushed_by_me(index);
            }
        }
    }

    /// The cart picks up what it meets: its Entity#startRiding of the cart.
    fn pick_up(&mut self, index: usize) {
        let current = self.current;
        let other = &mut self.others[index];
        other.rides = Some(current);
        other.passenger = true;
        let living = other.living;
        self.cart().rider = Some(MinecartRider { living, ..MinecartRider::default() });
    }

    /// `entity.push(cart)` of what is not a cart: Entity.push, behind Boat.push's gate for a boat (1.9.4 Boat.java:184-192).
    fn push_other(&mut self, index: usize) {
        let release = self.release();
        let cart_position = self.me().position;
        let cart_vehicle = self.me().rider.is_some();
        let cart_bottom = bounds_of(self.me()).min.y;
        let current = self.current;
        let same_vehicle = shares_cart(&self.others, index, current, release);
        let other = &mut self.others[index];
        if other.boat && cart_bottom > other.bounds.min.y {
            return;
        }
        // (Entity.push skips what shares its root vehicle: the cart's own rider pushes nothing and takes nothing from it.
        // 1.9.4 Entity.java:1139-1140 and :2225-2226 isPassengerOfSameVehicle, 26.3 Entity.java push the same)
        let (to_other, to_cart) = entity_push_impulses(
            other.position,
            cart_position,
            false,
            false,
            same_vehicle,
            other.has_passengers,
            cart_vehicle,
            other.pushable,
            true,
            release,
        );
        other.velocity = other.velocity + to_other;
        let cart = self.cart();
        cart.velocity = cart.velocity + to_cart;
    }

    /// `entity.push(cart)` of a cart: AbstractMinecart.push and pushOtherMinecart, with `this`
    /// the cart pushed (26.3 AbstractMinecart.java:435-506; 1.8.8 AbstractMinecart.java:669).
    fn cart_pushed_by_me(&mut self, this: usize) {
        let release = self.release();
        let source = self.current;
        let this_position = self.carts[this].position;
        let source_position = self.carts[source].position;
        let Some((xa, za)) = minecart_push_impulse(this_position, source_position, release) else {
            return;
        };
        let (xo, zo) = if self.improved() {
            let velocity = self.carts[this].velocity;
            (velocity.x, velocity.z)
        } else {
            (source_position.x - this_position.x, source_position.z - this_position.z)
        };
        let direction = legacy_normalized(Vec3::new(xo, 0.0, zo), release);
        let radians = self.yaw_radians(this);
        let facing = legacy_normalized(Vec3::new(radians.0, 0.0, radians.1), release);
        if direction.dot(facing).abs() < f64::from(0.8_f32) && !self.improved() {
            return;
        }
        let movement = self.carts[this].velocity;
        let entity_movement = self.carts[source].velocity;
        let this_furnace = self.carts[this].variant == MinecartVariant::Furnace;
        let source_furnace = self.carts[source].variant == MinecartVariant::Furnace;
        // (1.13.2 AbstractMinecart.java:704-724; 1.14.4 AbstractMinecart.java:602-615)
        let (fifth, slowed) = if self.at_least(1, 14, 0) {
            (0.2, 0.95)
        } else {
            (f64::from(0.2_f32), f64::from(0.95_f32))
        };
        let scaled = |v: Vec3, by: f64| Vec3::new(v.x * by, v.y, v.z * by);
        if source_furnace && !this_furnace {
            self.carts[this].velocity = scaled(movement, fifth)
                .add_components(entity_movement.x - xa, 0.0, entity_movement.z - za);
            self.carts[source].velocity = scaled(entity_movement, slowed);
        } else if !source_furnace && this_furnace {
            self.carts[source].velocity = scaled(entity_movement, fifth)
                .add_components(movement.x + xa, 0.0, movement.z + za);
            self.carts[this].velocity = scaled(movement, slowed);
        } else {
            let xdd = (entity_movement.x + movement.x) / 2.0;
            let zdd = (entity_movement.z + movement.z) / 2.0;
            self.carts[this].velocity = scaled(movement, fifth).add_components(xdd - xa, 0.0, zdd - za);
            self.carts[source].velocity = scaled(entity_movement, fifth).add_components(xdd + xa, 0.0, zdd + za);
        }
    }

    /// The yaw of the cart `this` as (cos, sin) of its radians: Mth's tables with the release's
    /// constant (1.8.9 `yaw * (float)Math.PI / 180.0F`; 1.9.4 `yaw * (float)(Math.PI / 180.0)`).
    fn yaw_radians(&self, this: usize) -> (f64, f64) {
        let yaw = self.carts[this].y_rot;
        // (1.8.8 AbstractMinecart.java:708; 1.9.4 AbstractMinecart.java:702)
        let radians = if self.at_least(1, 9, 0) {
            yaw * 0.017453292_f32
        } else {
            yaw * std::f32::consts::PI / 180.0
        };
        let radians = f64::from(radians);
        (mth_cos(radians, self.release()), mth_sin(radians, self.release()))
    }

    // The improved path: NewMinecartBehavior, from 1.21.2 (the same code to 26.3).

    /// NewMinecartBehavior#tick on the server.
    fn tick_improved(&mut self) {
        let position = self.rail_position();
        let rail = rail_at(self.world, position);
        if self.me().first_tick {
            self.cart().on_rails = rail.is_some();
            self.adjust_to_rails(position, rail, true);
        }
        self.apply_gravity();
        self.move_along_track_improved();
    }

    /// The MinecartStep of where the cart is, as NewMinecartBehavior#adjustToRails and
    /// NewMinecartBehavior#moveAlongTrack record it with a weight.
    fn step_of(&self, weight: f32) -> MinecartStep {
        let cart = self.me();
        MinecartStep {
            position: cart.position,
            movement: cart.velocity,
            y_rot: cart.y_rot,
            x_rot: cart.x_rot,
            weight,
        }
    }

    /// 26.3 NewMinecartBehavior.isDecending.
    fn descending(movement: Vec3, shape: RailShape) -> bool {
        match shape {
            RailShape::AscendingEast => movement.x < 0.0,
            RailShape::AscendingWest => movement.x > 0.0,
            RailShape::AscendingNorth => movement.z > 0.0,
            RailShape::AscendingSouth => movement.z < 0.0,
            _ => false,
        }
    }

    /// 26.3 NewMinecartBehavior.setRotation.
    fn set_rotation(&mut self, mut y_rot: f32, mut x_rot: f32) {
        let cart = self.cart();
        let difference = f64::from((y_rot - cart.y_rot).abs());
        if (175.0..=185.0).contains(&difference) {
            cart.flipped = !cart.flipped;
            y_rot -= 180.0;
            x_rot *= -1.0;
        }
        x_rot = x_rot.clamp(-45.0, 45.0);
        cart.x_rot = x_rot % 360.0;
        cart.y_rot = y_rot % 360.0;
    }

    /// 26.3 NewMinecartBehavior.adjustToRails.
    fn adjust_to_rails(&mut self, target: BlockPosition, rail: Option<Rail>, instant: bool) {
        let Some(rail) = rail else {
            return;
        };
        let (exit0, exit1) = rail.shape.exits();
        let (exit0_half, exit1_half) = (half_exit(exit0), half_exit(exit1));
        let mut out_direction = exit0_half.horizontal();
        let mut in_direction = exit1_half.horizontal();
        let movement = self.me().velocity;
        if movement.length() > TINY && movement.dot(out_direction) < movement.dot(in_direction)
            || Self::descending(in_direction, rail.shape)
        {
            std::mem::swap(&mut out_direction, &mut in_direction);
        }
        let flipped = self.me().flipped;
        let degrees = |z: f64, x: f64| 180.0_f32 - (crate::trig::atan2(z, x) * 180.0 / std::f64::consts::PI) as f32;
        let turned = if flipped { 180.0 } else { 0.0 };
        let mut y_rot = degrees(out_direction.z, out_direction.x) + turned;
        let previous = self.me().position;
        let in_corner = exit0_half.x != exit1_half.x && exit0_half.z != exit1_half.z;
        let center = bottom_center(target);
        let target_position = if in_corner {
            let from_0_to_1 = exit1_half - exit0_half;
            let from_0_to_pos = (previous - center) - exit0_half;
            let travel = from_0_to_1.scale(from_0_to_1.dot(from_0_to_pos) / from_0_to_1.dot(from_0_to_1));
            y_rot = degrees(travel.z, travel.x) + turned;
            center + exit0_half + travel
        } else {
            let difference = exit0_half - exit1_half;
            let (z_snap, x_snap) = (difference.x != 0.0, difference.z != 0.0);
            Vec3::new(
                if x_snap { center.x } else { previous.x },
                f64::from(target.1),
                if z_snap { center.z } else { previous.z },
            )
        };
        // (`setPos(position.add(target.subtract(position)))`, which can land an ulp off the target: 1.21.2
        // NewMinecartBehavior.java:192-193, 26.3 the same)
        self.set_pos(previous + (target_position - previous));
        let mut x_rot = 0.0_f32;
        if exit0.1 != exit1.1 {
            let in_position = center + in_direction;
            let from_in = in_position.distance(self.me().position);
            self.set_pos(self.me().position.add_components(0.0, from_in + 0.1, 0.0));
            x_rot = if flipped { 45.0 } else { -45.0 };
        } else {
            self.set_pos(self.me().position.add_components(0.0, 0.1, 0.0));
        }
        self.set_rotation(y_rot, x_rot);
        let adjusted = previous.distance(self.me().position);
        if adjusted > 0.0 {
            let step = self.step_of(if instant { 0.0 } else { adjusted as f32 });
            self.tick.steps.push(step);
        }
    }

    /// 26.3 NewMinecartBehavior.moveAlongTrack.
    fn move_along_track_improved(&mut self) {
        let world = self.world;
        let mut iteration = TrackIteration::default();
        // (`shouldIterate() && isAlive()`: a cart its blocks broke, or gone before the loop, steps no further. 1.21.2
        // NewMinecartBehavior.java:229, 26.3 NewMinecartBehavior.java:225)
        while (iteration.first || iteration.movement_left > TINY) && self.me().removed.is_none() {
            let initial = self.me().velocity;
            let position = self.rail_position();
            let rail = rail_at(world, position);
            if self.me().on_rails != rail.is_some() {
                self.cart().on_rails = rail.is_some();
                self.adjust_to_rails(position, rail, false);
            }
            if let Some(rail) = rail {
                self.cart().fall_distance = 0.0;
                self.old_position = self.me().position;
                self.old_y_rot = self.me().y_rot;
                if rail.activator {
                    self.activate(rail.powered);
                }
                let speed = self.track_speed(initial.horizontal(), &mut iteration, position, rail);
                if iteration.first {
                    iteration.movement_left = speed.horizontal_length();
                } else {
                    iteration.movement_left += speed.horizontal_length() - initial.horizontal_length();
                }
                self.cart().velocity = speed;
                iteration.movement_left = self.step_along_track(position, rail.shape, iteration.movement_left);
                // MinecartHopper.makeStepAlongTrack (1.21.3 MinecartHopper.java, 26.3 :80-84)
                self.try_consume_items();
            } else {
                self.come_off_track();
                iteration.movement_left = 0.0;
            }
            let step_position = self.me().position;
            let step_delta = step_position - self.old_position;
            let step_length = step_delta.length();
            if step_length > TINY {
                if step_delta.horizontal_length_squared() > TINY {
                    let degrees = |y: f64, x: f64| (crate::trig::atan2(y, x) * 180.0 / std::f64::consts::PI) as f32;
                    let mut y_rot = 180.0_f32 - degrees(step_delta.z, step_delta.x);
                    let grounded = self.me().on_ground && !self.me().on_rails;
                    let mut x_rot = if grounded {
                        0.0
                    } else {
                        90.0_f32 - degrees(step_delta.horizontal_length(), step_delta.y)
                    };
                    let flipped = self.me().flipped;
                    y_rot += if flipped { 180.0 } else { 0.0 };
                    x_rot *= if flipped { -1.0 } else { 1.0 };
                    self.set_rotation(y_rot, x_rot);
                } else if !self.me().on_rails {
                    let cart = self.cart();
                    cart.x_rot = if cart.on_ground { 0.0 } else { rot_lerp(0.2, cart.x_rot, 0.0) };
                }
                let weight = step_length.min(self.max_speed()) as f32;
                let step = self.step_of(weight);
                self.tick.steps.push(step);
            } else if initial.horizontal_length_squared() > 0.0 {
                let step = self.step_of(1.0);
                self.tick.steps.push(step);
            }
            if step_length > TINY || iteration.first {
                self.apply_tick_effects();
                self.apply_tick_effects();
            }
            iteration.first = false;
        }
    }

    /// Entity.applyEffectsFromBlocks(): the moves recorded since the last, and on to the position.
    fn apply_tick_effects(&mut self) {
        let position = self.me().position;
        let mut movements = std::mem::take(&mut self.recorded);
        match movements.last() {
            None => movements.push(Movement { from: self.old_position, to: position, axis_order: None }),
            Some(last) if last.to.distance_squared(position) > 9.999999439624929e-11 => {
                movements.push(Movement { from: last.to, to: position, axis_order: None });
            }
            Some(_) => {}
        }
        self.apply_effects(&movements);
    }

    /// 26.3 NewMinecartBehavior.calculateTrackSpeed.
    fn track_speed(
        &mut self,
        delta: Vec3,
        iteration: &mut TrackIteration,
        position: BlockPosition,
        rail: Rail,
    ) -> Vec3 {
        let mut speed = delta;
        if !iteration.gained_slope_speed {
            let sloped = self.slope_speed(delta, rail.shape);
            if sloped.horizontal_length_squared() != delta.horizontal_length_squared() {
                iteration.gained_slope_speed = true;
                speed = sloped;
            }
        }
        if iteration.first {
            let input = self.player_input_speed(speed);
            if input.horizontal_length_squared() != speed.horizontal_length_squared() {
                iteration.halted = true;
                speed = input;
            }
        }
        if !iteration.halted && rail.powered_rail && !rail.powered {
            let halted = if speed.length() < 0.03 { Vec3::ZERO } else { speed.scale(0.5) };
            if halted.horizontal_length_squared() != speed.horizontal_length_squared() {
                iteration.halted = true;
                speed = halted;
            }
        }
        if iteration.first {
            speed = self.natural_slowdown(speed);
            if speed.length_squared() > 0.0 {
                let capped = speed.length().min(self.max_speed());
                speed = normalized(speed).scale(capped);
            }
        }
        if !iteration.boosted {
            let boosted = self.boost_speed(speed, position, rail);
            if boosted.horizontal_length_squared() != speed.horizontal_length_squared() {
                iteration.boosted = true;
                speed = boosted;
            }
        }
        speed
    }

    /// NewMinecartBehavior#calculateSlopeSpeed.
    fn slope_speed(&self, delta: Vec3, shape: RailShape) -> Vec3 {
        let mut slide = (delta.horizontal_length() * 0.02).max(0.0078125);
        if self.me().in_water {
            slide *= 0.2;
        }
        match shape {
            RailShape::AscendingEast => delta.add_components(-slide, 0.0, 0.0),
            RailShape::AscendingWest => delta.add_components(slide, 0.0, 0.0),
            RailShape::AscendingNorth => delta.add_components(0.0, 0.0, slide),
            RailShape::AscendingSouth => delta.add_components(0.0, 0.0, -slide),
            _ => delta,
        }
    }

    /// NewMinecartBehavior#calculatePlayerInputSpeed.
    fn player_input_speed(&self, delta: Vec3) -> Vec3 {
        let Some(rider) = self.me().rider else {
            return delta;
        };
        if !rider.player || rider.move_intent.length_squared() <= 0.0 {
            return delta;
        }
        let movement = normalized(rider.move_intent);
        if movement.length_squared() > 0.0 && delta.horizontal_length_squared() < 0.01 {
            return delta + normalized(Vec3::new(movement.x, 0.0, movement.z)).scale(0.001);
        }
        delta
    }

    fn boost_speed(&self, delta: Vec3, position: BlockPosition, rail: Rail) -> Vec3 {
        if !rail.powered_rail || !rail.powered {
            return delta;
        }
        if delta.length() > 0.01 {
            return normalized(delta).scale(delta.length() + 0.06);
        }
        // AbstractMinecart.getRedstoneDirection
        let world = self.world;
        let direction = match rail.shape {
            RailShape::EastWest if conductor(world, offset(position, (-1, 0, 0))) => Vec3::new(1.0, 0.0, 0.0),
            RailShape::EastWest if conductor(world, offset(position, (1, 0, 0))) => Vec3::new(-1.0, 0.0, 0.0),
            RailShape::NorthSouth if conductor(world, offset(position, (0, 0, -1))) => Vec3::new(0.0, 0.0, 1.0),
            RailShape::NorthSouth if conductor(world, offset(position, (0, 0, 1))) => Vec3::new(0.0, 0.0, -1.0),
            _ => return delta,
        };
        direction.scale(delta.length() + 0.2)
    }

    /// 26.3 NewMinecartBehavior.stepAlongTrack.
    fn step_along_track(&mut self, pos: BlockPosition, shape: RailShape, movement_left: f64) -> f64 {
        if movement_left < TINY {
            return 0.0;
        }
        let old_position = self.me().position;
        let (exit0, exit1) = shape.exits();
        let mut movement = self.me().velocity.horizontal();
        if movement.length() < TINY {
            self.cart().velocity = Vec3::ZERO;
            return 0.0;
        }
        let in_hill = exit0.1 != exit1.1;
        let in_direction = half_exit(exit1).horizontal();
        let mut out_direction = half_exit(exit0).horizontal();
        if movement.dot(out_direction) < movement.dot(in_direction) {
            out_direction = in_direction;
        }
        let mut out_position = (bottom_center(pos) + out_direction)
            .add_components(0.0, 0.1, 0.0)
            + normalized(out_direction).scale(TINY);
        if in_hill && !Self::descending(movement, shape) {
            out_position = out_position.add_components(0.0, 1.0, 0.0);
        }
        let towards_out = normalized(out_position - self.me().position);
        movement = towards_out.scale(movement.length() / towards_out.horizontal_length());
        let root_two = f64::from(if in_hill { std::f32::consts::SQRT_2 } else { 1.0 });
        let mut new_position = old_position + normalized(movement).scale(movement_left * root_two);
        let mut left = 0.0;
        if old_position.distance_squared(out_position) <= old_position.distance_squared(new_position) {
            left = (out_position - new_position).horizontal_length();
            new_position = out_position;
        }
        self.move_cart(new_position - old_position);
        if in_hill {
            let release = self.release();
            let new_cell = (
                floor_i32_for(release, new_position.x),
                floor_i32_for(release, new_position.y),
                floor_i32_for(release, new_position.z),
            );
            if let Some(new_rail) = rail_at(self.world, new_cell) {
                if self.rest_at_v_shape(shape, new_rail.shape) {
                    return 0.0;
                }
            }
            let from_out = out_position.horizontal().distance(self.me().position.horizontal());
            let project = out_position.y + if Self::descending(movement, shape) { from_out } else { -from_out };
            let at = self.me().position;
            if at.y < project {
                self.set_pos(Vec3::new(at.x, project, at.z));
            }
        }
        if self.me().position.distance(old_position) < TINY && new_position.distance(old_position) > TINY {
            self.cart().velocity = Vec3::ZERO;
            return 0.0;
        }
        self.cart().velocity = movement;
        left
    }

    /// 26.3 NewMinecartBehavior.restAtVShape.
    fn rest_at_v_shape(&mut self, current: RailShape, new: RailShape) -> bool {
        let velocity = self.me().velocity;
        if velocity.length_squared() < 0.005
            && new.is_slope()
            && Self::descending(velocity, current)
            && !Self::descending(velocity, new)
        {
            self.cart().velocity = Vec3::ZERO;
            return true;
        }
        false
    }

    /// 26.3 NewMinecartBehavior.pushAndPickupEntities: whether a rider picked up and nothing
    /// pushed leaves the rest of the move to make.
    fn push_and_pickup_improved(&mut self) -> bool {
        let bounds = bounds_of(self.me());
        let picked_up = self.pick_up_improved(bounds.inflate(Vec3::new(0.2, 0.0, 0.2)));
        let cart = self.me();
        if !cart.collided_horizontally && !cart.collided_vertically {
            return false;
        }
        let pushed = self.push_improved(bounds.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7)));
        picked_up && !pushed
    }

    fn pick_up_improved(&mut self, hitbox: Aabb) -> bool {
        if self.me().variant != MinecartVariant::Rideable || self.me().rider.is_some() {
            return false;
        }
        // (EntitySelector.pushableBy(minecart), 26.3 NewMinecartBehavior.java:493)
        let team = self.me().team;
        let release = self.release();
        for index in 0..self.others.len() {
            let other = self.others[index];
            // (one startRiding refuses, by its boarding cooldown, leaves the next to try: 26.3 NewMinecartBehavior.java pickupEntities)
            if !other.removed
                && other.pushable
                && other.bounds.intersects(hitbox)
                && pushable_by(release, team, other.team)
                && !other.player
                && !other.iron_golem
                && !other.passenger
                && other.can_board(release)
            {
                self.pick_up(index);
                return true;
            }
        }
        false
    }

    fn push_improved(&mut self, hitbox: Aabb) -> bool {
        let mut pushed = false;
        // (a rideable cart's by EntitySelector.pushableBy(minecart), another's every cart: 26.3 NewMinecartBehavior.java:510-528)
        let rideable = self.me().variant == MinecartVariant::Rideable;
        let team = self.me().team;
        let release = self.release();
        if rideable {
            for index in 0..self.others.len() {
                let other = self.others[index];
                if !other.removed
                    && other.pushable
                    && other.bounds.intersects(hitbox)
                    && pushable_by(release, team, other.team)
                    && (other.player || other.iron_golem || self.me().rider.is_some() || other.passenger)
                {
                    self.push_other(index);
                    pushed = true;
                }
            }
        }
        for index in 0..self.carts.len() {
            if index != self.current
                && self.carts[index].removed.is_none()
                && bounds_of(&self.carts[index]).intersects(hitbox)
                && (!rideable || pushable_by(release, team, self.carts[index].team))
            {
                self.cart_pushed_by_me(index);
                pushed = true;
            }
        }
        pushed
    }
}

/// NewMinecartBehavior.TrackIteration.
struct TrackIteration {
    movement_left: f64,
    first: bool,
    gained_slope_speed: bool,
    halted: bool,
    boosted: bool,
}

impl Default for TrackIteration {
    fn default() -> Self {
        Self { movement_left: 0.0, first: true, gained_slope_speed: false, halted: false, boosted: false }
    }
}

/// AbstractContainerMenu.getRedstoneSignalFromContainer of a hopper cart's five slots: each stack's
/// fullness as a float, 14 steps and 1 for holding anything (1.8.8 AbstractContainerMenu.java:526-544,
/// 26.3 Mth.lerpDiscrete, the same). A stack is full at the least of its own and the container's
/// most, which is Container.getMaxStackSize's 64 until 1.20.5 and 99 from it (1.20.4
/// AbstractContainerMenu.java:722 and Container.java:13; 1.20.5 AbstractContainerMenu.java:793 and
/// Container.java:27-32).
fn comparator_signal(inventory: &[MinecartStack; 5], release: MinecraftVersion) -> i32 {
    let most = if release.at_least(1, 20, 5) { 99 } else { 64 };
    let mut fullness = 0.0_f32;
    let mut held = false;
    for slot in inventory.iter().filter(|slot| slot.count > 0) {
        fullness += slot.count as f32 / slot.max_stack.min(most) as f32;
        held = true;
    }
    fullness /= 5.0;
    (fullness * 14.0).floor() as i32 + i32::from(held)
}

/// Entity.push(Vec3) of a blast: from 1.21.11 only a finite push is taken (1.21.10 Entity.java:1824-1826
/// adds any; 1.21.11 Entity.java:1843-1856 asks isFinite).
fn blast_push(velocity: Vec3, push: Vec3, release: MinecraftVersion) -> Vec3 {
    if release.at_least(1, 21, 11) && !(push.x.is_finite() && push.y.is_finite() && push.z.is_finite()) {
        velocity
    } else {
        velocity + push
    }
}

/// Vec3.normalize of the release: the float square root before 1.17, 1.0E-4 before 1.21.2.
fn legacy_normalized(vector: Vec3, release: MinecraftVersion) -> Vec3 {
    let exact = vector.length();
    // (1.16.5 Vec3.java:79; 1.17 Vec3.java:54)
    let length = if release.at_least(1, 17, 0) { exact } else { java_float(exact) };
    // (1.21.1 Vec3.java:65; 1.21.2 Vec3.java:73)
    let minimum = if release.at_least(1, 21, 2) { TINY } else { 1.0e-4 };
    if length < minimum {
        Vec3::ZERO
    } else {
        Vec3::new(vector.x / length, vector.y / length, vector.z / length)
    }
}

/// OldMinecartBehavior.getPos (snapPositionToRail): the place on the rail at a position, or None.
fn snap_to_rail(world: &WorldSnapshot, position: Vec3, release: MinecraftVersion) -> Option<Vec3> {
    let xt = floor_i32_for(release, position.x);
    let mut yt = floor_i32_for(release, position.y);
    let zt = floor_i32_for(release, position.z);
    if is_rail(world, (xt, yt - 1, zt)) {
        yt -= 1;
    }
    let rail = rail_at(world, (xt, yt, zt))?;
    let (exit0, exit1) = rail.shape.exits();
    let half = |value: i32| f64::from(value) * 0.5;
    let x0 = f64::from(xt) + 0.5 + half(exit0.0);
    let y0 = f64::from(yt) + 0.0625 + half(exit0.1);
    let z0 = f64::from(zt) + 0.5 + half(exit0.2);
    let x1 = f64::from(xt) + 0.5 + half(exit1.0);
    let y1 = f64::from(yt) + 0.0625 + half(exit1.1);
    let z1 = f64::from(zt) + 0.5 + half(exit1.2);
    let x_d = x1 - x0;
    let y_d = (y1 - y0) * 2.0;
    let z_d = z1 - z0;
    let progress = if x_d == 0.0 {
        position.z - f64::from(zt)
    } else if z_d == 0.0 {
        position.x - f64::from(xt)
    } else {
        ((position.x - x0) * x_d + (position.z - z0) * z_d) * 2.0
    };
    let mut y = y0 + y_d * progress;
    if y_d < 0.0 {
        y += 1.0;
    } else if y_d > 0.0 {
        y += 0.5;
    }
    Some(Vec3::new(x0 + x_d * progress, y, z0 + z_d * progress))
}

// A client's cart. It reads no rail and no block: it goes where the server last said.

/// A client's cart under the old path: the linear interpolation to the last target.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinecartClientLerp {
    pub position: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub steps: i32,
    pub target: Vec3,
    pub target_y_rot: f32,
    pub target_x_rot: f32,
}

/// The steps a client's lerpTo gives a cart: 3 and 2 more until 1.21.2 (1.8.8
/// AbstractMinecart.java:106, 1.20.4 :717), the handler's 3 from 1.21.5
/// (26.3 LinearInterpolationHandler.java:8, 33). Before 1.14 the packet's own steps are always 3.
/// (1.21.4 OldMinecartBehavior.java:55 takes the steps and 2; 1.21.5 OldMinecartBehavior.java:39 an
/// InterpolationHandler, InterpolationHandler.java:10 of 3.)
#[must_use]
pub fn client_lerp_steps(release: MinecraftVersion) -> i32 {
    if release.at_least(1, 21, 5) {
        3
    } else {
        5
    }
}

/// What a client's tick keeps counting of a cart. The numbers are the server's, by entity data and
/// entity events: this only runs them down between two of its packets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinecartClientEffects {
    pub hurt_time: i32,
    pub damage: f32,
    /// MinecartTNT.fuse: 80 from the priming event, whatever the server's own is.
    pub fuse: i32,
}

/// One client tick of AbstractMinecart.tick's head and MinecartTNT.tick: the counts alone. A client
/// neither moves a cart here nor blows it up (26.3 MinecartTNT.java:119 asks for a ServerLevel, as
/// 1.8.8 :95 for !isClient), breaks it (hurtClient, 26.3 VehicleEntity.java:25-27) or picks up
/// anything (26.3 MinecartHopper.java:94).
pub fn tick_client_effects(cart: &mut MinecartClientEffects) {
    if cart.hurt_time > 0 {
        cart.hurt_time -= 1;
    }
    if cart.damage > 0.0 {
        cart.damage -= 1.0;
    }
    if cart.fuse > 0 {
        cart.fuse -= 1;
    }
}

/// One client tick of the old path's interpolation. Until 1.20.2 the cart works it out itself, dividing by the steps left
/// and from 1.17 turning in floats (1.8.8 AbstractMinecart.java:291-302, 1.16.5 :360-362, 1.17 :295-297); from there
/// Entity.lerpPositionAndRotationStep (1.20.4 Entity.java:3424-3434, 26.3
/// LinearInterpolationHandler.doInterpolate). (1.20.1 AbstractMinecart.java:296-303 divides by lSteps; 1.20.2
/// AbstractMinecart.java:301 calls Entity.java:3417-3425 lerpPositionAndRotationStep.)
pub fn tick_client_linear(cart: &mut MinecartClientLerp, release: MinecraftVersion) {
    if cart.steps <= 0 {
        return;
    }
    let steps = f64::from(cart.steps);
    let turn = crate::math::wrap_degrees(f64::from(cart.target_y_rot) - f64::from(cart.y_rot));
    let pitch = f64::from(cart.target_x_rot) - f64::from(cart.x_rot);
    if release.at_least(1, 20, 2) {
        let share = 1.0 / steps;
        cart.position = cart.position.lerp(cart.target, share);
        cart.y_rot = (f64::from(cart.y_rot) + share * turn) as f32;
        cart.x_rot = (f64::from(cart.x_rot) + share * pitch) as f32;
    } else {
        let step = cart.target - cart.position;
        cart.position = Vec3::new(cart.position.x + step.x / steps, cart.position.y + step.y / steps, cart.position.z + step.z / steps);
        // (1.16.5 AbstractMinecart.java:361; 1.17 AbstractMinecart.java:296)
        if release.at_least(1, 17, 0) {
            cart.y_rot += turn as f32 / cart.steps as f32;
            cart.x_rot += pitch as f32 / cart.steps as f32;
        } else {
            cart.y_rot = (f64::from(cart.y_rot) + turn / steps) as f32;
            cart.x_rot = (f64::from(cart.x_rot) + pitch / steps) as f32;
        }
    }
    cart.steps -= 1;
}

/// A client's cart under the improvements: NewMinecartBehavior's steps and delay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MinecartClientSteps {
    pub position: Vec3,
    pub velocity: Vec3,
    pub y_rot: f32,
    pub x_rot: f32,
    /// lerpDelay.
    pub delay: i32,
    /// lerpSteps: what ClientboundMoveMinecartPacket brought since it last started on some.
    pub pending: Vec<MinecartStep>,
    /// currentLerpSteps.
    pub current: Vec<MinecartStep>,
    pub total_weight: f64,
    /// oldLerp.
    pub old: MinecartStep,
}

/// NewMinecartBehavior.POS_ROT_LERP_TICKS.
const LERP_TICKS: i32 = 3;

/// One client tick of the improvements' interpolation (26.3 NewMinecartBehavior.java:67-91).
pub fn tick_client_weighted(cart: &mut MinecartClientSteps) {
    cart.delay = cart.delay.wrapping_sub(1);
    if cart.delay <= 0 {
        cart.old = MinecartStep {
            position: cart.position,
            movement: cart.velocity,
            y_rot: cart.y_rot,
            x_rot: cart.x_rot,
            weight: 0.0,
        };
        cart.current.clear();
        if !cart.pending.is_empty() {
            cart.current.append(&mut cart.pending);
            cart.total_weight = cart.current.iter().map(|step| f64::from(step.weight)).sum();
            cart.delay = if cart.total_weight == 0.0 { 0 } else { LERP_TICKS };
        }
    }
    if cart.current.is_empty() {
        return;
    }
    let (partial, current, previous) = current_step(cart, 1.0);
    cart.position = previous.position.lerp(current.position, f64::from(partial));
    cart.velocity = previous.movement.lerp(current.movement, f64::from(partial));
    cart.x_rot = rot_lerp(partial, previous.x_rot, current.x_rot);
    cart.y_rot = rot_lerp(partial, previous.y_rot, current.y_rot);
}

/// 26.3 NewMinecartBehavior.getCurrentLerpStep.
fn current_step(cart: &MinecartClientSteps, partial_tick: f32) -> (f32, MinecartStep, MinecartStep) {
    let alpha = ((LERP_TICKS - cart.delay) as f32 + partial_tick) / LERP_TICKS as f32;
    let mut count_up = 0.0_f32;
    let mut partial = 1.0_f32;
    let mut found = None;
    for (index, step) in cart.current.iter().enumerate() {
        if step.weight <= 0.0 {
            continue;
        }
        count_up += step.weight;
        if f64::from(count_up) >= cart.total_weight * f64::from(alpha) {
            let before = count_up - step.weight;
            partial = ((f64::from(alpha) * cart.total_weight - f64::from(before)) / f64::from(step.weight)) as f32;
            found = Some(index);
            break;
        }
    }
    let index = found.unwrap_or(cart.current.len() - 1);
    let previous = if index > 0 { cart.current[index - 1] } else { cart.old };
    (partial, cart.current[index], previous)
}

/// The cell after the last that Level.containsFireBlock reads along an axis: floor(max + 1) in 1.8.8
/// (Level.java:1508-1514), Mth.ceil(max) from 1.9 (1.9.4 Level.java:1582-1588).
fn fire_cells_end(release: MinecraftVersion, value: f64) -> i32 {
    if release.at_least(1, 9, 0) {
        value.ceil() as i32
    } else {
        floor_i32_for(release, value + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{BlockShape, BlockState};

    /// `minecart#0.vel` at tick 19 of the fresh 1.20.4 recording of physrec/v_boat_into_minecart.
    const BOAT_PUSH_1_20_4: f64 = 0.05000000074505806;

    fn release(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
        MinecraftVersion::new(major, minor, patch)
    }

    fn old(version: MinecraftVersion) -> MinecartRules {
        MinecartRules::new(version, false, None).unwrap()
    }

    fn improved(max_speed: i32) -> MinecartRules {
        MinecartRules::new(release(26, 3, 0), true, Some(max_speed)).unwrap()
    }

    fn rail(world: &mut WorldSnapshot, at: BlockPosition, shape: &str) {
        world.set_block(at, BlockState { tags: TAG_RAIL, ..BlockState::default() });
        world.set_block_properties(at, vec![("shape".to_owned(), shape.to_owned())]);
    }

    fn powered(world: &mut WorldSnapshot, at: BlockPosition, shape: &str, on: bool) {
        world.set_block(at, BlockState { tags: TAG_RAIL | TAG_POWERED_RAIL, ..BlockState::default() });
        world.set_block_properties(at, vec![
            ("shape".to_owned(), shape.to_owned()),
            ("powered".to_owned(), on.to_string()),
        ]);
    }

    fn solid(world: &mut WorldSnapshot, at: BlockPosition) {
        world.set_block(at, BlockState {
            shape: Some(BlockShape::full_block()),
            tags: TAG_REDSTONE_CONDUCTOR,
            ..BlockState::default()
        });
    }

    fn straight(length: i32) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -4..length {
            rail(&mut world, (x, 0, 0), "east_west");
            solid(&mut world, (x, -1, 0));
        }
        world
    }

    fn cart_at(x: f64, velocity: f64) -> MinecartState {
        MinecartState {
            position: Vec3::new(x, 0.0625, 0.5),
            velocity: Vec3::new(velocity, 0.0, 0.0),
            ..MinecartState::default()
        }
    }

    fn rider() -> Option<MinecartRider> {
        Some(MinecartRider { player: true, living: true, ..MinecartRider::default() })
    }

    #[test]
    fn the_flag_chooses_the_path_and_a_release_alone_does_not() {
        let at = release(1, 21, 2);
        assert_eq!(MinecartRules::new(at, false, None).unwrap().behavior, MinecartBehavior::Old);
        assert_eq!(MinecartRules::new(at, true, Some(8)).unwrap().behavior, MinecartBehavior::Improved);
        assert!(MinecartRules::new(at, true, None).is_err());
        assert!(MinecartRules::new(at, true, Some(1001)).is_err());
        assert!(MinecartRules::new(release(1, 21, 1), true, Some(8)).is_err());
        let world = straight(30);
        let mut plain = cart_at(1.5, 0.1);
        let mut new = cart_at(1.5, 0.1);
        new.position.y = 0.1;
        tick_minecart(&mut plain, &world, MinecartRules::new(at, false, None).unwrap());
        tick_minecart(&mut new, &world, MinecartRules::new(at, true, Some(8)).unwrap());
        assert_eq!(plain.velocity.x, 0.1 * 0.96);
        assert_eq!(new.velocity.x, 0.1 * 0.975);
    }

    #[test]
    fn a_cart_coasts_along_a_straight_rail() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 0.1);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.position, Vec3::new(1.6, 0.0625, 0.5));
        assert_eq!(cart.velocity, Vec3::new(0.1 * 0.96, 0.0, 0.0));
        assert!(cart.on_rails);
    }

    #[test]
    fn the_old_drag_was_a_float_before_1_14() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 0.1);
        tick_minecart(&mut cart, &world, old(release(1, 13, 2)));
        assert_eq!(cart.velocity.x, 0.1 * f64::from(0.96_f32));
    }

    #[test]
    fn a_rider_keeps_a_cart_going_and_slows_its_move() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 0.2);
        cart.rider = rider();
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.position.x, 1.5 + 0.2 * 0.75);
        assert_eq!(cart.velocity.x, 0.2 * 0.997);
    }

    #[test]
    fn a_cart_is_held_to_the_top_speed_of_its_release() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 1.0);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.position.x, 1.9);
        let mut furnace = cart_at(1.5, 1.0);
        furnace.variant = MinecartVariant::Furnace;
        tick_minecart(&mut furnace, &world, old(release(1, 20, 4)));
        assert_eq!(furnace.position.x, 1.7);
    }

    #[test]
    fn water_halves_the_top_speed_from_1_17() {
        let world = straight(30);
        let mut wet = cart_at(1.5, 1.0);
        wet.in_water = true;
        tick_minecart(&mut wet, &world, old(release(1, 17, 0)));
        assert_eq!(wet.position.x, 1.7);
        let mut before = cart_at(1.5, 1.0);
        before.in_water = true;
        tick_minecart(&mut before, &world, old(release(1, 16, 5)));
        assert_eq!(before.position.x, 1.9);
    }

    #[test]
    fn gravity_in_water_goes_with_1_17() {
        let mut wet = cart_at(0.5, 0.0);
        wet.in_water = true;
        let mut scene = WorldSnapshot::new();
        solid(&mut scene, (0, -9, 0));
        tick_minecart(&mut wet, &scene, old(release(1, 17, 0)));
        assert_eq!(wet.position.y, 0.0625 - 0.005);
        let mut before = cart_at(0.5, 0.0);
        tick_minecart(&mut before, &scene, old(release(1, 16, 5)));
        assert_eq!(before.position.y, 0.0625 - 0.04);
        let mut legacy = cart_at(0.5, 0.0);
        tick_minecart(&mut legacy, &scene, old(release(1, 13, 2)));
        assert_eq!(legacy.position.y, 0.0625 - f64::from(0.04_f32));
    }

    #[test]
    fn no_gravity_is_read_from_1_10() {
        let scene = WorldSnapshot::new();
        let mut floating = cart_at(0.5, 0.0);
        floating.no_gravity = true;
        tick_minecart(&mut floating, &scene, old(release(1, 10, 2)));
        assert_eq!(floating.position.y, 0.0625);
        let mut falling = cart_at(0.5, 0.0);
        falling.no_gravity = true;
        tick_minecart(&mut falling, &scene, old(release(1, 9, 4)));
        assert!(falling.position.y < 0.0625);
    }

    #[test]
    fn a_powered_rail_boosts_and_an_unpowered_one_brakes() {
        let mut world = straight(30);
        powered(&mut world, (2, 0, 0), "east_west", true);
        let mut cart = cart_at(2.5, 0.1);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.velocity.x, 0.1 * 0.96 + 0.06);
        powered(&mut world, (2, 0, 0), "east_west", false);
        let mut braked = cart_at(2.5, 0.1);
        tick_minecart(&mut braked, &world, old(release(1, 20, 4)));
        assert_eq!(braked.position.x, 2.55);
        let mut stopped = cart_at(2.5, 0.02);
        tick_minecart(&mut stopped, &world, old(release(1, 20, 4)));
        assert_eq!(stopped.velocity.x, 0.0);
    }

    #[test]
    fn a_powered_rail_starts_a_resting_cart_off_a_block() {
        let mut world = straight(30);
        powered(&mut world, (2, 0, 0), "east_west", true);
        solid(&mut world, (1, 0, 0));
        let mut cart = cart_at(2.5, 0.0);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.velocity.x, 0.02);
    }

    #[test]
    fn a_curve_turns_the_motion() {
        let mut world = straight(3);
        rail(&mut world, (3, 0, 0), "south_west");
        rail(&mut world, (3, 0, 1), "north_south");
        solid(&mut world, (3, -1, 0));
        solid(&mut world, (3, -1, 1));
        let mut cart = cart_at(2.9, 0.3);
        let rules = old(release(1, 20, 4));
        for _ in 0..4 {
            tick_minecart(&mut cart, &world, rules);
        }
        assert_eq!(cart.position.x.floor(), 3.0);
        assert_eq!(cart.position.z.floor(), 1.0);
        assert!(cart.velocity.z > 0.0 && cart.velocity.x.abs() < 1.0e-12);
    }

    #[test]
    fn a_slope_lifts_the_cart_and_pulls_it_back() {
        let mut world = straight(2);
        rail(&mut world, (2, 0, 0), "ascending_east");
        solid(&mut world, (2, -1, 0));
        let mut cart = cart_at(2.5, 0.1);
        cart.position.y = 0.5625;
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert!(cart.position.y > 0.5625);
        assert!(cart.velocity.x < 0.1 * 0.96 - 0.0078125 * 0.9);
    }

    #[test]
    fn a_cart_runs_into_a_block_and_stops() {
        let mut world = straight(4);
        solid(&mut world, (3, 0, 0));
        let mut cart = cart_at(2.4, 0.2);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert!(cart.position.x < 2.6);
        assert_eq!(cart.velocity.x, 0.0);
        assert!(cart.collided_horizontally);
    }

    #[test]
    fn off_the_rail_a_cart_falls_and_drags() {
        let world = WorldSnapshot::new();
        let mut cart = cart_at(0.5, 0.1);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert!(!cart.on_rails);
        assert_eq!(cart.position.x, 0.6);
        assert_eq!(cart.velocity.x, 0.1 * 0.95);
        assert_eq!(cart.velocity.y, -0.04 * 0.95);
    }

    #[test]
    fn on_the_ground_off_a_rail_a_cart_halves_its_motion() {
        let mut world = WorldSnapshot::new();
        solid(&mut world, (0, -1, 0));
        let mut cart = cart_at(0.5, 0.2);
        cart.position.y = 0.0;
        cart.on_ground = true;
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert!(cart.on_ground);
        assert_eq!(cart.position.x, 0.6);
    }

    #[test]
    fn the_yaw_follows_the_way_and_flips_when_it_turns_back() {
        let world = straight(30);
        let mut cart = cart_at(2.5, 0.2);
        cart.y_rot = 0.0;
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.y_rot, 0.0);
        assert!(cart.flipped);
    }

    #[test]
    fn a_full_chest_cart_drags_more_than_an_empty_one() {
        let world = straight(30);
        let mut empty = cart_at(1.5, 0.1);
        empty.variant = MinecartVariant::Chest;
        tick_minecart(&mut empty, &world, old(release(1, 20, 4)));
        assert_eq!(empty.velocity.x, 0.1 * f64::from(0.98_f32 + 15.0 * 0.001_f32));
        let mut full = cart_at(1.5, 0.1);
        full.variant = MinecartVariant::Chest;
        full.comparator_signal = 15;
        tick_minecart(&mut full, &world, old(release(1, 20, 4)));
        assert_eq!(full.velocity.x, 0.1 * f64::from(0.98_f32));
        let mut unrolled = cart_at(1.5, 0.1);
        unrolled.variant = MinecartVariant::Hopper;
        unrolled.has_loot_table = true;
        tick_minecart(&mut unrolled, &world, old(release(1, 20, 4)));
        assert_eq!(unrolled.velocity.x, 0.1 * f64::from(0.98_f32));
    }

    #[test]
    fn a_rider_getting_off_pushes_its_cart_from_where_its_own_tick_takes_it() {
        let world = straight(30);
        let player = |x: f64| MinecartOther {
            position: Vec3::new(x, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.3, 0.0625, 0.2), Vec3::new(x + 0.3, 1.8625, 0.8)),
            pushable: true,
            living: true,
            player: true,
            passenger: true,
            rides: Some(0),
            gets_off: true,
            ..MinecartOther::default()
        };
        let ridden = || {
            let mut cart = cart_at(2.5, 0.0);
            cart.rider = Some(MinecartRider { player: true, living: true, ..MinecartRider::default() });
            cart
        };
        // from where it landed, off the cart at the player's turn
        let mut others = [player(1.9)];
        let mut carts = [ridden()];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert!(carts[0].rider.is_none());
        assert_eq!((others[0].rides, others[0].passenger), (None, false));
        assert!(carts[0].velocity.x > 0.0);
        // its tick carried it out of reach, whatever the connection then snaps it back to
        let away = player(1.0);
        let mut others = [MinecartOther { tick_place: Some((away.position, away.bounds)), ..player(1.9) }];
        let mut carts = [ridden()];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(carts[0].velocity.x, 0.0);
    }

    #[test]
    fn a_furnace_cart_pushes_itself_along_and_burns_its_fuel() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 0.1);
        cart.variant = MinecartVariant::Furnace;
        cart.fuel = 2;
        cart.push = Vec3::new(1.0, 0.0, 0.0);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.velocity.x, (0.1 * 0.8 + 1.0) * 0.96);
        assert_eq!(cart.fuel, 1);
        assert_eq!(cart.push.x, 1.0);
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.fuel, 0);
        assert_eq!(cart.push, Vec3::ZERO);
    }

    #[test]
    fn the_furnace_push_drops_against_the_motion_before_1_15_2() {
        let world = straight(30);
        // (the slowdown makes a push of length 1, which a speed of 2 outruns)
        let mut cart = cart_at(1.5, 6.0);
        cart.variant = MinecartVariant::Furnace;
        cart.fuel = 100;
        cart.push = Vec3::new(-1.0, 0.0, 0.0);
        tick_minecart(&mut cart, &world, old(release(1, 15, 1)));
        assert_eq!(cart.push, Vec3::ZERO);
        let mut after = cart_at(1.5, 6.0);
        after.variant = MinecartVariant::Furnace;
        after.fuel = 100;
        after.push = Vec3::new(-1.0, 0.0, 0.0);
        tick_minecart(&mut after, &world, old(release(1, 16, 0)));
        assert!(after.push.x > 0.0);
    }

    #[test]
    fn the_furnace_push_keeps_its_length_from_1_21_2() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 0.1);
        cart.variant = MinecartVariant::Furnace;
        cart.fuel = 100;
        cart.push = Vec3::new(0.5, 0.0, 0.0);
        tick_minecart(&mut cart, &world, old(release(1, 21, 2)));
        // (projected on the motion with the tick's gravity in it, which it keeps)
        assert!((cart.push.length() - 0.5).abs() < 1.0e-12);
        assert_eq!(cart.velocity.x, (0.1 * 0.8 + cart.push.x) * 0.96);
    }

    #[test]
    fn the_rider_starts_a_resting_cart_as_its_release_says() {
        let world = straight(30);
        let mut by_input = cart_at(1.5, 0.0);
        by_input.rider = Some(MinecartRider { living: true, forward: 1.0, yaw: -90.0, ..MinecartRider::default() });
        tick_minecart(&mut by_input, &world, old(release(1, 12, 2)));
        assert!(by_input.position.x > 1.5);
        // a mob's input counts no more from 1.13
        let mut mob = cart_at(1.5, 0.0);
        mob.rider = Some(MinecartRider { living: true, forward: 1.0, yaw: -90.0, ..MinecartRider::default() });
        tick_minecart(&mut mob, &world, old(release(1, 13, 2)));
        assert_eq!(mob.position.x, 1.5);
        let mut by_velocity = cart_at(1.5, 0.0);
        by_velocity.rider = Some(MinecartRider { player: true, velocity: Vec3::new(0.2, 0.0, 0.0), ..MinecartRider::default() });
        tick_minecart(&mut by_velocity, &world, old(release(1, 20, 4)));
        assert_eq!(by_velocity.position.x, 1.5 + 0.02 * 0.75);
        let mut by_intent = cart_at(1.5, 0.0);
        by_intent.rider = Some(MinecartRider { player: true, move_intent: Vec3::new(1.0, 0.0, 0.0), ..MinecartRider::default() });
        tick_minecart(&mut by_intent, &world, old(release(1, 21, 2)));
        assert_eq!(by_intent.position.x, 1.5 + 0.001 * 0.75);
    }

    #[test]
    fn a_rail_keeps_the_soul_sand_under_it_out_from_1_16() {
        let mut world = straight(30);
        let soul_sand = BlockShape { effect: crate::world::BlockEffect::SoulSand, ..BlockShape::full_block() };
        world.set_block((2, -1, 0), BlockState { shape: Some(soul_sand), ..BlockState::default() });
        let mut slowed = cart_at(2.3, 0.1);
        tick_minecart(&mut slowed, &world, old(release(1, 15, 0)));
        assert_eq!(slowed.velocity.x, 0.1 * f64::from(0.4_f32) * 0.96);
        let mut kept = cart_at(2.3, 0.1);
        tick_minecart(&mut kept, &world, old(release(1, 16, 0)));
        assert_eq!(kept.velocity.x, 0.1 * 0.96);
    }

    #[test]
    fn water_slows_the_slope_from_1_17() {
        let mut world = straight(2);
        rail(&mut world, (2, 0, 0), "ascending_east");
        solid(&mut world, (2, -1, 0));
        let slide = |at| {
            let mut cart = cart_at(2.5, 0.0);
            cart.position.y = 0.5625;
            cart.in_water = true;
            tick_minecart(&mut cart, &world, old(at));
            cart.velocity.x.abs()
        };
        assert!(slide(release(1, 17, 0)) < 0.003);
        assert!(slide(release(1, 16, 5)) > 0.007);
    }

    #[test]
    fn the_air_drag_is_a_float_again_from_26_2() {
        let world = WorldSnapshot::new();
        let fall = |at| {
            let mut cart = cart_at(0.5, 0.1);
            tick_minecart(&mut cart, &world, old(at));
            cart.velocity.x
        };
        assert_eq!(fall(release(1, 13, 2)), 0.1 * f64::from(0.95_f32));
        assert_eq!(fall(release(1, 14, 0)), 0.1 * 0.95);
        assert_eq!(fall(release(26, 1, 0)), 0.1 * 0.95);
        assert_eq!(fall(release(26, 2, 0)), 0.1 * f64::from(0.95_f32));
    }

    #[test]
    fn a_rider_still_at_its_old_place_stops_the_cart_before_1_9() {
        let world = straight(30);
        let lagging = |at| {
            let mut others = [MinecartOther {
                position: Vec3::new(4.0, 0.0625, 0.5),
                bounds: Aabb::new(Vec3::new(3.7, 0.0625, 0.2), Vec3::new(4.3, 1.8625, 0.8)),
                pushable: true,
                living: true,
                passenger: true,
                rides: Some(0),
                ..MinecartOther::default()
            }];
            let mut carts = [cart_at(3.1, 0.3)];
            carts[0].rider = Some(MinecartRider { living: true, ..MinecartRider::default() });
            tick_minecarts(&mut carts, &mut others, &world, old(at));
            carts[0].collided_horizontally
        };
        assert!(lagging(release(1, 8, 8)));
        assert!(!lagging(release(1, 9, 4)));
    }

    #[test]
    fn the_improved_cart_rests_at_the_bottom_of_a_valley() {
        let mut world = WorldSnapshot::new();
        rail(&mut world, (0, 0, 0), "ascending_west");
        rail(&mut world, (1, 0, 0), "ascending_east");
        solid(&mut world, (0, -1, 0));
        solid(&mut world, (1, -1, 0));
        let mut cart = improved_cart(0.75, 0.05);
        cart.position.y = 0.35;
        let rules = improved(8);
        for _ in 0..40 {
            tick_minecart(&mut cart, &world, rules);
        }
        assert_eq!(cart.velocity, Vec3::ZERO);
    }

    #[test]
    fn the_improved_cart_goes_through_the_block_under_its_slope() {
        // MinecartCollisionContext: the cell ahead of a slope does not stop the cart climbing out of it
        let mut world = straight(2);
        rail(&mut world, (2, 0, 0), "ascending_east");
        solid(&mut world, (2, -1, 0));
        solid(&mut world, (3, 0, 0));
        rail(&mut world, (3, 1, 0), "east_west");
        let mut cart = improved_cart(2.2, 0.3);
        cart.position.y = 0.3;
        tick_minecart(&mut cart, &world, improved(8));
        assert!(!cart.collided_horizontally, "{cart:?}");
        assert!(cart.position.x > 2.3);
    }

    #[test]
    fn a_cart_bumps_a_cart_on_its_way() {
        let world = straight(30);
        let mut carts = [cart_at(2.5, 0.2), cart_at(3.2, 0.0)];
        tick_minecarts(&mut carts, &mut [], &world, old(release(1, 20, 4)));
        assert!(carts[1].velocity.x > 0.0, "{:?}", carts[1]);
        assert!(carts[0].velocity.x < 0.2 * 0.96);
    }

    #[test]
    fn a_moving_cart_picks_a_mob_up_and_a_still_one_does_not() {
        let world = straight(30);
        let mob = |x: f64| MinecartOther {
            position: Vec3::new(x, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.3, 0.0625, 0.2), Vec3::new(x + 0.3, 1.8625, 0.8)),
            pushable: true,
            living: true,
            ..MinecartOther::default()
        };
        let mut others = [mob(3.2)];
        let mut carts = [cart_at(2.5, 0.25)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(others[0].rides, Some(0));
        assert!(carts[0].rider.is_some());
        let mut others = [mob(2.9)];
        let mut carts = [cart_at(2.5, 0.0)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(others[0].rides, None);
    }

    #[test]
    fn a_living_thing_beside_a_cart_pushes_it_after_its_tick() {
        let world = straight(30);
        let beside = MinecartOther {
            position: Vec3::new(3.0, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(2.7, 0.0625, 0.2), Vec3::new(3.3, 1.8625, 0.8)),
            pushable: true,
            living: true,
            ..MinecartOther::default()
        };
        let mut others = [beside];
        let mut carts = [cart_at(2.5, 0.0)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert!(carts[0].velocity.x < 0.0);
        // the box of 1.8 reaches 0.2F further, and a fast cart it meets carries it off
        let far = MinecartOther {
            position: Vec3::new(4.0, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(3.7, 0.0625, 0.2), Vec3::new(4.3, 1.8625, 0.8)),
            ..beside
        };
        let mut others = [far];
        let mut carts = [cart_at(3.0, 0.2)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 8, 8)));
        assert_eq!(others[0].rides, Some(0));
        let mut others = [far];
        let mut carts = [cart_at(3.0, 0.2)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 9, 4)));
        assert_eq!(others[0].rides, None);
    }

    #[test]
    fn a_player_shoves_a_cart_before_its_tick_only_in_1_8() {
        let world = straight(30);
        let shoved = |at| {
            let mut others = [MinecartOther {
                position: Vec3::new(3.0, 0.0625, 0.5),
                bounds: Aabb::new(Vec3::new(2.7, 0.0625, 0.2), Vec3::new(3.3, 1.8625, 0.8)),
                pushable: true,
                player: true,
                living: true,
                ..MinecartOther::default()
            }];
            let mut carts = [cart_at(2.5, 0.0)];
            tick_minecarts(&mut carts, &mut others, &world, old(at));
            carts[0].position.x
        };
        assert!(shoved(release(1, 8, 8)) < 2.5);
        assert_eq!(shoved(release(1, 9, 4)), 2.5);
    }

    /// The recording's physrec/v_boat_into_minecart: the cart spawned first, the ridden boat beside it after.
    fn boat_beside(x: f64) -> MinecartOther {
        MinecartOther {
            position: Vec3::new(x, 0.0, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.6875, 0.0, -0.1875), Vec3::new(x + 0.6875, 0.5625, 1.1875)),
            pushable: true,
            solid: true,
            boat: true,
            has_passengers: true,
            ..MinecartOther::default()
        }
    }

    fn cart_on_the_floor() -> MinecartState {
        MinecartState { on_ground: true, ..resting(MinecartVariant::Rideable, Vec3::new(33.5, 0.0, 0.5)) }
    }

    #[test]
    fn a_boat_ticking_after_the_cart_leaves_it_a_whole_entity_push() {
        let world = floor(40);
        let push = |at: MinecraftVersion| {
            let mut carts = [cart_on_the_floor()];
            let mut others = [boat_beside(32.150015071923065)];
            tick_minecarts(&mut carts, &mut others, &world, old(at));
            assert_eq!(carts[0].position.x, 33.5);
            assert_eq!(others[0].velocity, Vec3::ZERO, "a ridden boat takes nothing back");
            carts[0].velocity
        };
        assert_eq!(push(release(1, 20, 4)), Vec3::new(BOAT_PUSH_1_20_4, 0.0, 0.0));
        // (the float square root before 1.17, as the 1.13.2 recording has it)
        assert_eq!(push(release(1, 13, 2)).x, 0.050000003992758585);
        assert_eq!(push(release(1, 9, 4)).x, 0.050000003992758585);
        assert_eq!(push(release(26, 3, 0)).x, push(release(1, 20, 4)).x);
    }

    #[test]
    fn a_boat_ticking_ahead_of_the_cart_pushes_it_before_the_cart_moves() {
        let world = floor(40);
        let mut carts = [cart_on_the_floor()];
        let mut others = [MinecartOther { ticks_after: Some(0), ..boat_beside(32.150015071923065) }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        // (the ground halves what the push gave, and the cart moves that)
        assert_eq!(carts[0].velocity.x, BOAT_PUSH_1_20_4 * 0.5);
        assert_eq!(carts[0].position.x, 33.5 + BOAT_PUSH_1_20_4 * 0.5);
    }

    #[test]
    fn a_boat_between_two_carts_pushes_the_second_before_it_moves() {
        let world = floor(40);
        let mut carts = [resting(MinecartVariant::Rideable, Vec3::new(-3.5, 0.0, 0.5)), cart_on_the_floor()];
        carts[0].on_ground = true;
        let mut others = [MinecartOther { ticks_after: Some(1), ..boat_beside(32.150015071923065) }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(carts[0].velocity, Vec3::ZERO);
        assert_eq!(carts[1].velocity.x, BOAT_PUSH_1_20_4 * 0.5);
    }

    #[test]
    fn a_boat_pushes_from_where_its_tick_leaves_it() {
        let world = floor(40);
        let mut carts = [cart_on_the_floor()];
        let moved = boat_beside(32.150015071923065);
        let mut others = [MinecartOther { tick_place: Some((moved.position, moved.bounds)), ..boat_beside(332.05002523908257) }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(carts[0].velocity.x, BOAT_PUSH_1_20_4);
        assert_eq!((others[0].position, others[0].tick_place), (moved.position, None));
    }

    #[test]
    fn a_boat_short_of_its_reach_pushes_nothing() {
        let world = floor(40);
        let mut carts = [cart_on_the_floor()];
        tick_minecarts(&mut carts, &mut [boat_beside(32.05002523908257)], &world, old(release(1, 20, 4)));
        assert_eq!(carts[0].velocity, Vec3::ZERO);
    }

    #[test]
    fn up_to_1_8_a_boat_pushes_no_cart() {
        let world = floor(40);
        let mut carts = [cart_on_the_floor()];
        tick_minecarts(&mut carts, &mut [boat_beside(32.150015071923065)], &world, old(release(1, 8, 8)));
        assert_eq!(carts[0].velocity, Vec3::ZERO);
    }

    #[test]
    fn a_boat_pushes_no_cart_above_it_or_with_a_rider() {
        let world = floor(40);
        let mut above = [cart_on_the_floor()];
        above[0].position.y = 0.25;
        tick_minecarts(&mut above, &mut [boat_beside(32.150015071923065)], &world, old(release(1, 20, 4)));
        assert_eq!(above[0].velocity.x, 0.0);
        let mut ridden = [MinecartState { rider: rider(), ..cart_on_the_floor() }];
        tick_minecarts(&mut ridden, &mut [boat_beside(32.150015071923065)], &world, old(release(1, 20, 4)));
        assert_eq!(ridden[0].velocity.x, 0.0);
    }

    #[test]
    fn a_boat_nobody_rides_takes_the_push_back() {
        let world = floor(40);
        let mut carts = [cart_on_the_floor()];
        let mut others = [MinecartOther { has_passengers: false, ..boat_beside(32.150015071923065) }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(others[0].velocity.x, -carts[0].velocity.x);
    }

    #[test]
    fn a_cart_above_a_boat_takes_nothing_in_pushing_it() {
        let world = floor(40);
        let boat = MinecartOther { has_passengers: false, ..boat_beside(32.150015071923065) };
        let pushed = |y: f64| {
            let mut carts = [MinecartState { velocity: Vec3::new(-0.2, 0.0, 0.0), ..cart_on_the_floor() }];
            carts[0].position.y = y;
            let mut others = [MinecartOther { ticks_after: Some(1), ..boat }];
            let mut items = [];
            let mut scene = Scene {
                world: &world,
                rules: old(release(1, 20, 4)),
                carts: &mut carts,
                others: &mut others,
                items: &mut items,
                containers: &mut [],
                gateways: &mut [],
                level_random: &mut None,
                current: 0,
                support: None,
                recorded: Vec::new(),
                old_position: Vec3::ZERO,
                old_y_rot: 0.0,
                consumed_item: false,
                tick: MinecartTick::default(),
            };
            scene.push_other(0);
            others[0].velocity.x
        };
        assert!(pushed(0.0) < 0.0);
        assert_eq!(pushed(0.25), 0.0);
    }

    #[test]
    fn a_jockey_on_the_carts_rider_shares_its_vehicle_from_1_9() {
        // Entity#isPassengerOfSameVehicle compares root vehicles from 1.9 (1.9.4 Entity.java:2225-2227),
        // so a mob riding the cart's rider neither stops the cart's move (1.9.4 Level.java:1102) nor
        // takes its push (Entity.java:1139-1140); 1.8.8 knows only the direct rider
        // (Level.java:1084, Entity.java:1025), and the jockey stands in the way
        let world = straight(30);
        let rider = MinecartOther {
            position: Vec3::new(3.1, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(2.8, 0.0625, 0.2), Vec3::new(3.4, 1.8625, 0.8)),
            rides: Some(0),
            passenger: true,
            has_passengers: true,
            living: true,
            ..MinecartOther::default()
        };
        let jockey = MinecartOther {
            position: Vec3::new(4.0, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(3.7, 0.0625, 0.2), Vec3::new(4.3, 1.8625, 0.8)),
            pushable: true,
            living: true,
            passenger: true,
            rides_other: Some(0),
            ..MinecartOther::default()
        };
        assert_eq!(root_cart(&[rider, jockey], 1), Some(0));
        let stopped = |version: MinecraftVersion| {
            let mut others = [rider, jockey];
            let mut carts = [MinecartState { rider: Some(MinecartRider { living: true, ..MinecartRider::default() }), ..cart_at(3.1, 0.3) }];
            tick_minecarts(&mut carts, &mut others, &world, old(version));
            carts[0].collided_horizontally
        };
        assert!(stopped(release(1, 8, 8)));
        assert!(!stopped(release(1, 9, 4)));
        assert!(!stopped(release(1, 20, 4)));
        assert!(shares_cart(&[rider, jockey], 1, 0, release(1, 9, 4)));
        assert!(!shares_cart(&[rider, jockey], 1, 0, release(1, 8, 8)));
    }

    #[test]
    fn a_mob_in_the_way_stops_the_move() {
        let world = straight(30);
        let mut others = [MinecartOther {
            position: Vec3::new(4.0, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(3.7, 0.0625, 0.2), Vec3::new(4.3, 1.8625, 0.8)),
            pushable: true,
            player: true,
            living: true,
            ..MinecartOther::default()
        }];
        let mut carts = [cart_at(3.1, 0.3)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert!(carts[0].position.x + f64::from(HALF_WIDTH) <= 3.7 + 1.0e-7, "{:?}", carts[0]);
        assert!(carts[0].collided_horizontally);
    }

    #[test]
    fn an_entity_pushes_a_cart_harder_than_it_is_pushed_back() {
        let mut cart = cart_at(2.5, 0.0);
        let mut other = MinecartOther { position: Vec3::new(3.0, 0.0625, 0.5), ..MinecartOther::default() };
        push_minecart(&mut cart, &mut other, release(1, 20, 4));
        assert!(cart.velocity.x < 0.0);
        assert_eq!(other.velocity.x, -cart.velocity.x / 4.0);
    }

    #[test]
    fn the_activator_rail_throws_the_rider_off() {
        let mut world = straight(30);
        world.set_block((2, 0, 0), BlockState { tags: TAG_RAIL | TAG_ACTIVATOR_RAIL, ..BlockState::default() });
        world.set_block_properties((2, 0, 0), vec![
            ("shape".to_owned(), "east_west".to_owned()),
            ("powered".to_owned(), "true".to_owned()),
        ]);
        let mut cart = cart_at(2.5, 0.0);
        cart.rider = rider();
        let tick = tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(tick.activator, Some(true));
        assert!(cart.rider.is_none());
        // and what it threw off is no rider any more: on foot it shoves the cart, whose tick it was in
        let mut carts = [cart_at(2.5, 0.0)];
        carts[0].rider = rider();
        let mut others = [MinecartOther {
            position: Vec3::new(2.0, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(1.7, 0.0625, 0.2), Vec3::new(2.3, 1.8625, 0.8)),
            pushable: true,
            player: true,
            living: true,
            passenger: true,
            rides: Some(0),
            ..MinecartOther::default()
        }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(others[0].rides, None);
        assert!(carts[0].velocity.x > 0.0);
    }

    fn improved_cart(x: f64, velocity: f64) -> MinecartState {
        MinecartState {
            position: Vec3::new(x, 0.1, 0.5),
            velocity: Vec3::new(velocity, 0.0, 0.0),
            on_rails: true,
            ..MinecartState::default()
        }
    }

    #[test]
    fn the_improved_cart_rides_above_its_rail_and_steps_for_the_client() {
        let world = straight(30);
        let mut cart = improved_cart(1.5, 0.1);
        let tick = tick_minecart(&mut cart, &world, improved(8));
        assert_eq!(cart.position.y, 0.1);
        assert_eq!(cart.velocity.x, 0.1 * 0.975);
        assert!((cart.position.x - (1.5 + 0.1 * 0.975)).abs() < 1.0e-9);
        assert_eq!(tick.steps.len(), 1);
        assert_eq!(tick.steps[0].position, cart.position);
    }

    #[test]
    fn the_improved_top_speed_is_the_game_rule() {
        let world = straight(40);
        let mut slow = improved_cart(1.5, 5.0);
        tick_minecart(&mut slow, &world, improved(8));
        assert!((slow.velocity.x - 0.4).abs() < 1.0e-12);
        let mut fast = improved_cart(1.5, 5.0);
        tick_minecart(&mut fast, &world, improved(40));
        assert!((fast.velocity.x - 2.0).abs() < 1.0e-12);
        assert!((fast.position.x - 3.5).abs() < 1.0e-9, "{:?}", fast);
    }

    #[test]
    fn the_improved_cart_crosses_blocks_within_a_tick() {
        let mut world = straight(40);
        powered(&mut world, (2, 0, 0), "east_west", true);
        let mut cart = improved_cart(1.9, 0.5);
        let tick = tick_minecart(&mut cart, &world, improved(40));
        assert!(tick.steps.len() >= 2, "{tick:?}");
        assert!((cart.velocity.x - (0.5 * 0.975 + 0.06)).abs() < 1.0e-9);
    }

    #[test]
    fn the_improved_cart_settles_onto_its_rail_on_its_first_tick() {
        let world = straight(30);
        let mut cart = cart_at(1.5, 0.0);
        cart.position.y = 0.0;
        cart.first_tick = true;
        let tick = tick_minecart(&mut cart, &world, improved(8));
        assert_eq!(cart.position, Vec3::new(1.5, 0.1, 0.5));
        assert!(cart.on_rails && !cart.first_tick);
        assert_eq!(tick.steps[0].weight, 0.0);
    }

    #[test]
    fn the_improved_slide_grows_with_the_speed() {
        let mut world = straight(2);
        rail(&mut world, (2, 0, 0), "ascending_east");
        solid(&mut world, (2, -1, 0));
        let mut cart = improved_cart(2.5, 1.0);
        cart.position.y = 0.6;
        tick_minecart(&mut cart, &world, improved(40));
        assert!(cart.position.x > 2.5 && cart.position.y > 0.6);
        assert!(cart.velocity.horizontal_length() < 0.975);
    }

    #[test]
    fn the_client_old_path_goes_a_share_of_the_way_each_tick() {
        let mut cart = MinecartClientLerp {
            steps: 2,
            target: Vec3::new(1.0, 0.0, 0.0),
            target_y_rot: 170.0,
            y_rot: -170.0,
            ..MinecartClientLerp::default()
        };
        let at = release(26, 3, 0);
        tick_client_linear(&mut cart, at);
        assert_eq!(cart.position.x, 0.5);
        assert_eq!(cart.y_rot, -180.0);
        tick_client_linear(&mut cart, at);
        assert_eq!(cart.position.x, 1.0);
        assert_eq!(cart.steps, 0);
        tick_client_linear(&mut cart, at);
        assert_eq!(cart.position.x, 1.0);
        // (until 1.20.2 it divides by the steps: the rounding of a third is not the same)
        let third = |at| {
            let mut cart = MinecartClientLerp { position: Vec3::new(0.7, 0.0, 0.0), steps: 3, target: Vec3::new(1.9, 0.0, 0.0), ..MinecartClientLerp::default() };
            tick_client_linear(&mut cart, at);
            cart.position.x
        };
        assert_eq!(third(release(1, 20, 1)), 0.7 + (1.9 - 0.7) / 3.0);
        assert_eq!(third(release(1, 20, 2)), 0.7 + (1.0 / 3.0) * (1.9 - 0.7));
    }

    #[test]
    fn the_client_steps_weigh_the_way_over_three_ticks() {
        let step = |x: f64, weight: f32| MinecartStep { position: Vec3::new(x, 0.0, 0.0), weight, ..MinecartStep::default() };
        let mut cart = MinecartClientSteps {
            pending: vec![step(1.0, 1.0), step(3.0, 2.0)],
            ..MinecartClientSteps::default()
        };
        let near = |x: f64, want: f64| assert!((x - want).abs() < 1.0e-6, "{x} {want}");
        tick_client_weighted(&mut cart);
        near(cart.position.x, 1.0);
        tick_client_weighted(&mut cart);
        near(cart.position.x, 2.0);
        tick_client_weighted(&mut cart);
        near(cart.position.x, 3.0);
        assert_eq!(cart.delay, 1);
        tick_client_weighted(&mut cart);
        near(cart.position.x, 3.0);
        assert!(cart.current.is_empty());
    }

    // The side effects.

    fn passable(world: &mut WorldSnapshot, at: BlockPosition, effect: BlockEffect) {
        world.set_block(at, BlockState {
            shape: Some(BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO), effect, ..BlockShape::full_block() }),
            ..BlockState::default()
        });
    }

    fn lava(world: &mut WorldSnapshot, at: BlockPosition) {
        world.set_block(at, BlockState { fluid: FluidState::Lava, fluid_source: true, ..BlockState::default() });
    }

    fn activator(world: &mut WorldSnapshot, at: BlockPosition, on: bool) {
        world.set_block(at, BlockState { tags: TAG_RAIL | TAG_ACTIVATOR_RAIL, ..BlockState::default() });
        world.set_block_properties(at, vec![
            ("shape".to_owned(), "east_west".to_owned()),
            ("powered".to_owned(), on.to_string()),
        ]);
    }

    fn floor(size: i32) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -size..size {
            for z in -size..size {
                solid(&mut world, (x, -1, z));
            }
        }
        world
    }

    fn resting(variant: MinecartVariant, position: Vec3) -> MinecartState {
        MinecartState { variant, position, on_ground: true, random_seed: 42, ..MinecartState::default() }
    }

    fn tnt_on_rail(x: f64, velocity: f64) -> MinecartState {
        MinecartState { variant: MinecartVariant::Tnt, random_seed: 42, ..cart_at(x, velocity) }
    }

    fn hurt(kind: MinecartDamage, amount: f32) -> MinecartHurt {
        MinecartHurt { kind, amount, creative: false, burning_arrow: None }
    }

    #[test]
    fn an_entity_draws_as_java_does() {
        // java.util.Random with its raw state at 42, worked out apart: (x * 0x5DEECE66D + 11) mod 2^48
        let mut random = EntityRandom(42);
        assert_eq!(random.next_int(20), 6);
        assert_eq!(random.next_int(20), 19);
        assert_eq!(random.0, 197_491_923_327_988);
        let mut random = EntityRandom(42);
        assert_eq!(random.next_double(), 0.0037624195461311016);
        assert_eq!(EntityRandom(123_456_789_012).next_double(), 0.8579115295862211);
    }

    #[test]
    fn a_powered_activator_rail_hurts_a_rideable_cart_once_and_breaks_nothing() {
        let mut world = straight(30);
        activator(&mut world, (2, 0, 0), true);
        let rules = old(release(1, 20, 4));
        let mut cart = MinecartState { damage: 3.0, ..cart_at(2.5, 0.0) };
        let tick = tick_minecart(&mut cart, &world, rules);
        assert_eq!(tick.activator, Some(true));
        assert_eq!((cart.hurt_time, cart.hurt_dir, cart.damage), (10, -1, 50.0));
        tick_minecart(&mut cart, &world, rules);
        // the next tick's hurt time is left of the first: no second hurt until it has run out
        assert_eq!((cart.hurt_time, cart.hurt_dir, cart.damage), (9, -1, 49.0));
        assert_eq!(cart.removed, None);
        let mut off = cart_at(2.5, 0.0);
        let mut unpowered = world.clone();
        activator(&mut unpowered, (2, 0, 0), false);
        tick_minecart(&mut off, &unpowered, rules);
        assert_eq!((off.hurt_time, off.damage), (0, 0.0));
        // it is the rideable cart's: a chest cart takes none of it
        let mut chest = MinecartState { variant: MinecartVariant::Chest, ..cart_at(2.5, 0.0) };
        tick_minecart(&mut chest, &world, rules);
        assert_eq!((chest.hurt_time, chest.damage), (0, 0.0));
    }

    #[test]
    fn hurt_time_and_damage_run_down_each_tick() {
        let world = straight(30);
        let mut cart = MinecartState { hurt_time: 2, damage: 1.5, ..cart_at(1.5, 0.0) };
        let rules = old(release(26, 3, 0));
        tick_minecart(&mut cart, &world, rules);
        assert_eq!((cart.hurt_time, cart.damage), (1, 0.5));
        tick_minecart(&mut cart, &world, rules);
        assert_eq!((cart.hurt_time, cart.damage), (0, -0.5));
        tick_minecart(&mut cart, &world, rules);
        assert_eq!((cart.hurt_time, cart.damage), (0, -0.5));
    }

    #[test]
    fn an_activator_rail_primes_a_tnt_cart_which_then_counts_down_and_goes_off() {
        let mut world = straight(30);
        activator(&mut world, (2, 0, 0), true);
        let rules = old(release(1, 20, 4));
        let mut cart = tnt_on_rail(2.5, 0.0);
        let tick = tick_minecart(&mut cart, &world, rules);
        assert!(tick.primed);
        // primed in the tick's move, and its tail counts it off
        assert_eq!(cart.fuse, 79);
        let tick = tick_minecart(&mut cart, &world, rules);
        assert!(!tick.primed);
        assert_eq!(cart.fuse, 78);
        cart.fuse = 0;
        let tick = tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, Some(MinecartRemoval::Exploded));
        // at rest: nothing of its speed, 4 as a float
        assert_eq!(tick.explosions.len(), 1);
        assert_eq!(tick.explosions[0].power, 4.0);
        assert_eq!(tick.explosions[0].position, cart.position);
        let mut unpowered = world.clone();
        activator(&mut unpowered, (2, 0, 0), false);
        let mut off = tnt_on_rail(2.5, 0.0);
        tick_minecart(&mut off, &unpowered, rules);
        assert_eq!(off.fuse, -1);
    }

    #[test]
    fn a_tnt_carts_blast_grows_with_its_speed_by_its_random() {
        let world = straight(30);
        let mut cart = MinecartState { fuse: 0, ..tnt_on_rail(1.5, 0.2) };
        let tick = tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        let speed = cart.velocity.horizontal_length();
        let want = (4.0 + 0.0037624195461311016 * 1.5 * speed) as f32;
        assert_eq!(tick.explosions[0].power, want);
        assert_eq!(cart.random_seed, 197_491_923_327_988);
    }

    #[test]
    fn a_tnt_cart_goes_off_against_a_wall_only_at_speed() {
        // a box moving along the wall as well keeps speed through the collision
        let mut world = floor(8);
        solid(&mut world, (3, 0, 0));
        solid(&mut world, (3, 0, 1));
        solid(&mut world, (3, 0, -1));
        let rules = old(release(1, 20, 4));
        let mut fast = resting(MinecartVariant::Tnt, Vec3::new(2.3, 0.0, 0.5));
        fast.on_ground = false;
        fast.velocity = Vec3::new(0.3, 0.0, 0.3);
        let tick = tick_minecart(&mut fast, &world, rules);
        assert!(fast.collided_horizontally);
        assert_eq!(tick.explosions.len(), 1);
        assert_eq!(fast.removed, Some(MinecartRemoval::Exploded));
        let mut slow = resting(MinecartVariant::Tnt, Vec3::new(2.3, 0.0, 0.5));
        slow.on_ground = false;
        slow.velocity = Vec3::new(0.3, 0.0, 0.01);
        let tick = tick_minecart(&mut slow, &world, rules);
        assert!(slow.collided_horizontally);
        assert!(tick.explosions.is_empty());
    }

    #[test]
    fn a_tnt_cart_goes_off_on_landing_from_three_blocks() {
        let world = floor(8);
        let drop = |release: MinecraftVersion, fall: f64| {
            let mut cart = resting(MinecartVariant::Tnt, Vec3::new(0.5, 0.02, 0.5));
            cart.on_ground = false;
            cart.velocity = Vec3::new(0.0, -0.1, 0.0);
            cart.fall_distance = fall;
            (tick_minecart(&mut cart, &world, old(release)), cart)
        };
        let (tick, cart) = drop(release(1, 20, 4), 2.9);
        assert!(tick.explosions.is_empty());
        assert_eq!(cart.removed, None);
        assert_eq!(cart.fall_distance, 0.0);
        let (tick, cart) = drop(release(1, 20, 4), 3.0);
        assert_eq!(cart.removed, Some(MinecartRemoval::Exploded));
        // (fall / 10 squared, as the floats it was until 1.21.5; its square root the speed. The last
        // move's height is not counted: Entity.fall takes the fall or the landing)
        let power = 3.0_f32 / 10.0;
        let speed = f64::from(power * power).sqrt();
        assert_eq!(tick.explosions[0].power, (4.0 + 0.0037624195461311016 * 1.5 * speed) as f32);
    }

    #[test]
    fn a_bed_halves_the_fall_a_tnt_cart_goes_off_by() {
        let mut world = floor(8);
        world.set_block((0, -1, 0), BlockState {
            shape: Some(BlockShape { effect: BlockEffect::Bed, ..BlockShape::full_block() }),
            ..BlockState::default()
        });
        let drop = |release: MinecraftVersion| {
            let mut cart = resting(MinecartVariant::Tnt, Vec3::new(0.5, 0.02, 0.5));
            cart.on_ground = false;
            cart.velocity = Vec3::new(0.0, -0.1, 0.0);
            cart.fall_distance = 4.0;
            tick_minecart(&mut cart, &world, old(release));
            cart.removed
        };
        assert_eq!(drop(release(1, 20, 4)), None);
        assert_eq!(drop(release(1, 11, 2)), Some(MinecartRemoval::Exploded));
    }

    #[test]
    fn fire_sets_a_tnt_cart_off_by_the_releases_way() {
        let world = straight(8);
        let burn = |version: MinecraftVersion, speed: f64, damage: f32| {
            let mut cart = MinecartState { damage, ..tnt_on_rail(1.5, speed) };
            let tick = hurt_minecart(&mut cart, &mut [], hurt(MinecartDamage::Fire, 1.0), &world, old(version));
            (cart, tick)
        };
        // before 1.12 at once, past the 40 of damage
        let (cart, tick) = burn(release(1, 11, 2), 0.0, 35.0);
        assert_eq!((cart.removed, tick.explosions.len()), (Some(MinecartRemoval::Exploded), 1));
        // from 1.12 by a fuse of two draws of its random, still there
        let (cart, tick) = burn(release(1, 12, 2), 0.0, 35.0);
        assert_eq!((cart.removed, cart.fuse, tick.primed), (None, 6 + 19, true));
        assert_eq!(cart.random_seed, 197_491_923_327_988);
        // not until it is beaten before 1.20.3
        let (cart, _) = burn(release(1, 20, 2), 0.0, 0.0);
        assert_eq!((cart.fuse, cart.damage, cart.hurt_time), (-1, 10.0, 10));
        // from 1.20.3 the first fire does it
        let (cart, tick) = burn(release(1, 20, 3), 0.0, 0.0);
        assert_eq!((cart.fuse, tick.primed), (25, true));
        // a fuse once lit is not lit again, and burns on
        let mut lit = MinecartState { fuse: 7, ..tnt_on_rail(1.5, 0.0) };
        hurt_minecart(&mut lit, &mut [], hurt(MinecartDamage::Fire, 1.0), &world, old(release(1, 20, 4)));
        assert_eq!((lit.fuse, lit.random_seed), (7, 42));
    }

    #[test]
    fn a_plain_hit_breaks_a_still_tnt_cart_and_sets_off_a_moving_one() {
        let world = straight(8);
        let hit = |speed: f64| {
            let mut cart = MinecartState { damage: 35.0, ..tnt_on_rail(1.5, speed) };
            hurt_minecart(&mut cart, &mut [], hurt(MinecartDamage::Plain, 1.0), &world, old(release(1, 20, 4)));
            cart
        };
        let still = hit(0.0);
        assert_eq!((still.removed, still.fuse), (Some(MinecartRemoval::Destroyed), -1));
        let moving = hit(0.2);
        assert_eq!((moving.removed, moving.fuse), (None, 25));
    }

    #[test]
    fn a_burning_arrow_sets_a_tnt_cart_off_at_once() {
        let world = straight(8);
        let mut cart = tnt_on_rail(1.5, 0.0);
        let arrow = MinecartHurt { burning_arrow: Some(4.0), ..hurt(MinecartDamage::Plain, 1.0) };
        let tick = hurt_minecart(&mut cart, &mut [], arrow, &world, old(release(1, 8, 8)));
        assert_eq!(cart.removed, Some(MinecartRemoval::Exploded));
        assert_eq!(tick.explosions[0].power, (4.0 + 0.0037624195461311016 * 1.5 * 2.0) as f32);
    }

    #[test]
    fn a_creative_hit_takes_a_cart_away_with_no_drop() {
        let world = straight(8);
        for version in [release(1, 8, 8), release(1, 20, 4), release(26, 3, 0)] {
            let mut cart = cart_at(1.5, 0.0);
            let creative = MinecartHurt { creative: true, ..hurt(MinecartDamage::Plain, 1.0) };
            hurt_minecart(&mut cart, &mut [], creative, &world, old(version));
            assert_eq!(cart.removed, Some(MinecartRemoval::Discarded), "{version}");
        }
    }

    #[test]
    fn a_creative_hit_breaks_a_named_cart_before_1_20_3() {
        // 1.20.2 AbstractMinecart.java:235-239: discard only with no custom name, else destroy;
        // 1.20.3 VehicleEntity.java hurt discards whatever its name
        let world = straight(8);
        let creative = MinecartHurt { creative: true, ..hurt(MinecartDamage::Plain, 1.0) };
        let hit = |version: MinecraftVersion| {
            let mut cart = MinecartState { custom_name: true, ..cart_at(1.5, 0.0) };
            hurt_minecart(&mut cart, &mut [], creative, &world, old(version));
            cart.removed
        };
        for version in [release(1, 8, 8), release(1, 13, 2), release(1, 20, 2)] {
            assert_eq!(hit(version), Some(MinecartRemoval::Destroyed), "{version}");
        }
        for version in [release(1, 20, 3), release(26, 3, 0)] {
            assert_eq!(hit(version), Some(MinecartRemoval::Discarded), "{version}");
        }
    }

    #[test]
    fn a_rider_thrown_off_in_the_tick_rides_through_it_before_1_9() {
        // the 0.75 of a move with a passenger: 1.8.8 mc_ride_activator t=26 moves 0.1394 at a motion of 0.1858
        let world = straight(8);
        let moved = |version: MinecraftVersion, world: &WorldSnapshot| {
            let mut cart = cart_at(2.5, 0.2);
            cart.rider = Some(MinecartRider { player: true, living: true, ..MinecartRider::default() });
            let mut carts = [cart];
            let mut others = [MinecartOther { rides: Some(0), passenger: true, gets_off: true, player: true, living: true, pushable: true, ..MinecartOther::default() }];
            tick_minecarts_among(&mut carts, &mut others, &mut [], &mut [], world, old(version));
            (carts[0].position.x - 2.5, others[0].gets_off)
        };
        let (old_move, waits) = moved(release(1, 8, 8), &world);
        assert!(waits, "the turn waits for the next tick");
        let (new_move, _) = moved(release(1, 9, 4), &world);
        assert!(old_move < 0.2 * 0.8, "{old_move}");
        assert!(new_move > 0.0);
    }

    #[test]
    fn hits_add_up_past_forty_and_the_rider_gets_off() {
        let world = straight(8);
        let mut others = [MinecartOther { rides: Some(0), passenger: true, ..MinecartOther::default() }];
        let mut cart = MinecartState { rider: Some(MinecartRider::default()), ..cart_at(1.5, 0.0) };
        let rules = old(release(1, 20, 4));
        hurt_minecart(&mut cart, &mut others, hurt(MinecartDamage::Plain, 4.0), &world, rules);
        assert_eq!((cart.damage, cart.hurt_dir, cart.removed), (40.0, -1, None));
        hurt_minecart(&mut cart, &mut others, hurt(MinecartDamage::Plain, 0.5), &world, rules);
        assert_eq!((cart.hurt_dir, cart.removed), (1, Some(MinecartRemoval::Destroyed)));
        assert_eq!((others[0].rides, others[0].passenger), (None, false));
    }

    #[test]
    fn the_game_rule_stops_a_tnt_cart() {
        let mut world = straight(8);
        activator(&mut world, (2, 0, 0), true);
        let rules = old(release(1, 21, 5)).in_level(0, false);
        let mut cart = tnt_on_rail(2.5, 0.0);
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.fuse, -1);
        // what was lit before the rule goes without a blast
        let mut lit = MinecartState { fuse: 0, ..tnt_on_rail(2.5, 0.0) };
        let tick = tick_minecart(&mut lit, &world, rules);
        assert!(tick.explosions.is_empty());
        assert_eq!(lit.removed, Some(MinecartRemoval::Discarded));
        // and nothing happens to one that was not, as no release before it had the rule to ask
        let mut plain = MinecartState { fuse: 0, ..tnt_on_rail(2.5, 0.0) };
        let before = old(release(1, 21, 4)).in_level(0, false);
        assert_eq!(tick_minecart(&mut plain, &world, before).explosions.len(), 1);
    }

    fn stone(count: i32) -> MinecartStack {
        MinecartStack { item: 1, count, max_stack: 64 }
    }

    fn hopper(x: f64) -> MinecartState {
        MinecartState { variant: MinecartVariant::Hopper, ..cart_at(x, 0.0) }
    }

    fn item_at(x: f64, y: f64, stack: MinecartStack) -> MinecartItem {
        MinecartItem::new(Aabb::new(Vec3::new(x - 0.125, y, 0.375), Vec3::new(x + 0.125, y + 0.25, 0.625)), stack)
    }

    fn tick_hopper(cart: &mut MinecartState, items: &mut [MinecartItem], world: &WorldSnapshot, version: MinecraftVersion) {
        tick_minecarts_among(std::slice::from_mut(cart), &mut [], items, &mut [], world, old(version));
    }

    #[test]
    fn a_hopper_cart_takes_an_item_and_grows_heavier_by_it() {
        let world = straight(8);
        let mut cart = hopper(1.5);
        let mut items = [item_at(1.5, 0.0625, stone(3))];
        tick_hopper(&mut cart, &mut items, &world, release(1, 20, 4));
        assert_eq!(items[0].stack.count, 0);
        assert_eq!(cart.inventory[0], stone(3));
        // 3/64 of one of five slots: floor(14 * 0.009375) + 1
        assert_eq!(cart.comparator_signal, 1);
        // a stack that is gone is no longer there to take
        let mut again = hopper(1.5);
        tick_hopper(&mut again, &mut items, &world, release(1, 20, 4));
        assert_eq!(again.inventory[0].count, 0);
    }

    #[test]
    fn a_comparator_reads_a_hopper_carts_stacks() {
        let mut slots = [MinecartStack::default(); 5];
        let version = release(1, 20, 4);
        assert_eq!(comparator_signal(&slots, version), 0);
        slots[0] = stone(64);
        slots[1] = stone(64);
        assert_eq!(comparator_signal(&slots, version), 6);
        slots[2] = MinecartStack { item: 2, count: 1, max_stack: 1 };
        slots[3] = stone(64);
        slots[4] = stone(64);
        assert_eq!(comparator_signal(&slots, version), 15);
    }

    #[test]
    fn a_stack_of_up_to_99_fills_a_hopper_carts_slot_by_99_from_1_20_5() {
        // (60 of a stack of 99 and one of 64: 60/64 + 1/64 is 2 steps of 14 before 1.20.5, 60/99 + 1/64 one from it)
        let mut slots = [MinecartStack::default(); 5];
        slots[0] = MinecartStack { item: 1, count: 60, max_stack: 99 };
        slots[1] = MinecartStack { item: 2, count: 1, max_stack: 64 };
        assert_eq!(comparator_signal(&slots, release(1, 20, 4)), 3);
        assert_eq!(comparator_signal(&slots, release(1, 20, 5)), 2);
        assert_eq!(comparator_signal(&slots, release(26, 3, 0)), 2);
    }

    #[test]
    fn a_hopper_cart_takes_part_of_a_stack_and_tries_the_next_item() {
        let world = straight(8);
        let mut cart = hopper(1.5);
        cart.inventory = [stone(62), MinecartStack { item: 2, count: 64, max_stack: 64 }, MinecartStack { item: 2, count: 64, max_stack: 64 }, MinecartStack { item: 2, count: 64, max_stack: 64 }, MinecartStack { item: 2, count: 64, max_stack: 64 }];
        let mut items = [item_at(1.5, 0.0625, stone(5)), item_at(1.5, 0.0625, MinecartStack { item: 2, count: 1, max_stack: 64 })];
        tick_hopper(&mut cart, &mut items, &world, release(1, 20, 4));
        assert_eq!(cart.inventory[0].count, 64);
        // the rest stays where it is, and addItem said nothing went: the second cannot go in either
        assert_eq!(items[0].stack.count, 3);
        assert_eq!(items[1].stack.count, 1);
    }

    #[test]
    fn a_hopper_cart_merges_only_the_same_item() {
        let world = straight(8);
        let mut cart = hopper(1.5);
        cart.inventory[0] = MinecartStack { item: 2, count: 1, max_stack: 64 };
        let mut items = [item_at(1.5, 0.0625, stone(2))];
        tick_hopper(&mut cart, &mut items, &world, release(1, 20, 4));
        assert_eq!(cart.inventory[0].count, 1);
        assert_eq!(cart.inventory[1], stone(2));
    }

    #[test]
    fn a_hopper_cart_stopped_by_the_rail_takes_nothing_until_an_unpowered_one() {
        let mut world = straight(8);
        activator(&mut world, (2, 0, 0), true);
        let mut cart = hopper(2.5);
        let mut items = [item_at(2.5, 0.0625, stone(3))];
        tick_hopper(&mut cart, &mut items, &world, release(1, 20, 4));
        assert!(!cart.enabled);
        assert_eq!(items[0].stack.count, 3);
        activator(&mut world, (2, 0, 0), false);
        tick_hopper(&mut cart, &mut items, &world, release(1, 20, 4));
        assert!(cart.enabled);
        assert_eq!(items[0].stack.count, 0);
    }

    #[test]
    fn a_hopper_carts_draw_box_goes_by_the_release() {
        let world = straight(8);
        // over its top at the height of each release's box, and out of reach of the 0.25 around it
        let takes = |version: MinecraftVersion, y: f64| {
            let mut cart = hopper(1.5);
            let mut items = [item_at(1.5, y, stone(1))];
            tick_hopper(&mut cart, &mut items, &world, version);
            items[0].stack.count == 0
        };
        let above = 0.0625 + 0.7 + 0.01;
        // (1.8.9: from a block up; 1.9: from half a block; 1.13 on: from 11/16)
        assert!(!takes(release(1, 8, 8), above));
        assert!(takes(release(1, 9, 4), above));
        assert!(takes(release(1, 20, 4), above));
        let high = 0.0625 + 1.9;
        assert!(takes(release(1, 8, 8), high));
        assert!(takes(release(26, 3, 0), high));
        assert!(!takes(release(26, 3, 0), 0.0625 + 2.1));
        // the inside box is narrower than the block's until 1.20.5
        let beside = |version: MinecraftVersion| {
            let mut cart = hopper(1.5);
            let mut items = [MinecartItem::new(Aabb::new(Vec3::new(1.5 + 0.40, 0.8, 0.5), Vec3::new(1.5 + 0.45, 0.9, 0.6)), stone(1))];
            tick_hopper(&mut cart, &mut items, &world, version);
            items[0].stack.count == 0
        };
        assert!(!beside(release(1, 20, 4)));
        assert!(beside(release(1, 20, 5)));
    }

    #[test]
    fn a_hopper_cart_took_only_the_first_item_about_it_until_1_19_4() {
        // (1.19.3 MinecartHopper.java:110-115 offers the first alone; 1.19.4 MinecartHopper.java:92-97 goes on)
        let world = straight(8);
        // out of the box over it, in the 0.25 around it
        let beside = |stack| MinecartItem::new(Aabb::new(Vec3::new(1.5, 0.0625, 0.5 + 0.6), Vec3::new(1.6, 0.1, 0.5 + 0.7)), stack);
        let full = MinecartStack { item: 2, count: 64, max_stack: 64 };
        let run = |version: MinecraftVersion| {
            let mut cart = hopper(1.5);
            cart.inventory = [full, full, full, full, stone(1)];
            let mut items = [beside(full), beside(stone(2))];
            tick_hopper(&mut cart, &mut items, &world, version);
            items[1].stack.count
        };
        assert_eq!(run(release(1, 19, 3)), 2);
        assert_eq!(run(release(1, 19, 4)), 0);
        assert_eq!(run(release(1, 20, 0)), 0);
    }

    #[test]
    fn a_container_over_a_hopper_cart_gives_one_item_and_hides_the_items_there() {
        let world = straight(8);
        let mut cart = hopper(1.5);
        let mut items = [item_at(1.5, 1.0, stone(1))];
        let mut containers = [MinecartContainer { position: (1, 1, 0), slots: vec![MinecartStack::default(), stone(5)] }];
        tick_minecarts_among(std::slice::from_mut(&mut cart), &mut [], &mut items, &mut containers, &world, old(release(1, 20, 4)));
        assert_eq!(cart.inventory[0], stone(1));
        assert_eq!(containers[0].slots[1].count, 4);
        assert_eq!(items[0].stack.count, 1);
    }

    #[test]
    fn a_hopper_cart_takes_once_a_tick_though_it_steps_many_times() {
        let world = straight(30);
        let mut cart = MinecartState { variant: MinecartVariant::Hopper, ..cart_at(1.5, 0.3) };
        cart.position.y = 0.1;
        cart.on_rails = true;
        let mut items = [item_at(2.2, 0.1, stone(1)), item_at(2.3, 0.1, stone(1))];
        tick_minecarts_among(std::slice::from_mut(&mut cart), &mut [], &mut items, &mut [], &world, improved(8));
        assert_eq!(items.iter().filter(|item| item.stack.count == 0).count(), 1);
    }

    #[test]
    fn a_command_block_cart_runs_its_command_four_ticks_apart() {
        let mut world = straight(8);
        activator(&mut world, (2, 0, 0), true);
        let rules = old(release(1, 20, 4));
        let mut cart = MinecartState { variant: MinecartVariant::CommandBlock, tick_count: 100, ..cart_at(2.5, 0.0) };
        let runs: Vec<bool> = (0..6).map(|_| tick_minecart(&mut cart, &world, rules).command_ran).collect();
        assert_eq!(runs, [true, false, false, false, true, false]);
        assert_eq!(cart.last_activated, 105);
        activator(&mut world, (2, 0, 0), false);
        let mut off = MinecartState { variant: MinecartVariant::CommandBlock, tick_count: 100, ..cart_at(2.5, 0.0) };
        assert!(!tick_minecart(&mut off, &world, rules).command_ran);
    }

    fn fire_floor() -> WorldSnapshot {
        let mut world = floor(8);
        passable(&mut world, (0, 0, 0), BlockEffect::Fire);
        world
    }

    fn cart_in_cell() -> MinecartState {
        resting(MinecartVariant::Rideable, Vec3::new(0.5, 0.0, 0.5))
    }

    #[test]
    fn fire_burns_a_cart_once_a_move_before_1_16_and_once_a_pass_after() {
        let world = fire_floor();
        let damage = |version: MinecraftVersion| {
            let mut cart = cart_in_cell();
            tick_minecart(&mut cart, &world, old(version));
            cart.damage
        };
        // a move's burn of 1 is 10
        assert_eq!(damage(release(1, 8, 8)), 10.0);
        assert_eq!(damage(release(1, 15, 2)), 10.0);
        // the move's checkInsideBlocks, and the tick's own
        assert_eq!(damage(release(1, 16, 5)), 20.0);
        assert_eq!(damage(release(1, 21, 1)), 20.0);
        assert_eq!(damage(release(26, 3, 0)), 20.0);
    }

    #[test]
    fn the_fire_test_ends_at_the_ceiling_from_1_9() {
        // (1.8.8 Level.java:1508-1514 floor(max + 1); 1.9.4 Level.java:1582-1588 and 1.12.2 :1588-1594 Mth.ceil)
        assert_eq!(super::fire_cells_end(release(1, 8, 8), 1.0), 2);
        for version in [release(1, 9, 4), release(1, 12, 2), release(1, 15, 2)] {
            assert_eq!(super::fire_cells_end(version, 1.0), 1, "{version:?}");
            assert_eq!(super::fire_cells_end(version, 1.25), 2, "{version:?}");
        }
        assert_eq!(super::fire_cells_end(release(1, 8, 8), 1.25), 2);
    }

    #[test]
    fn soul_fire_burns_double() {
        let mut world = floor(8);
        passable(&mut world, (0, 0, 0), BlockEffect::SoulFire);
        let mut cart = cart_in_cell();
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.damage, 40.0);
    }

    #[test]
    fn a_cart_left_in_fire_breaks() {
        let world = fire_floor();
        let mut cart = cart_in_cell();
        let rules = old(release(1, 20, 4));
        let mut ticks = 0;
        while cart.removed.is_none() {
            tick_minecart(&mut cart, &world, rules);
            ticks += 1;
            assert!(ticks < 10);
        }
        assert_eq!(cart.removed, Some(MinecartRemoval::Destroyed));
        assert_eq!(ticks, 3);
    }

    #[test]
    fn a_cactus_hurts_a_cart_and_magma_does_not() {
        let mut world = floor(8);
        passable(&mut world, (0, 0, 0), BlockEffect::Cactus);
        let mut cart = cart_in_cell();
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.damage, 20.0);
        let mut hot = floor(8);
        hot.set_block((0, -1, 0), BlockState {
            shape: Some(BlockShape { effect: BlockEffect::Magma, ..BlockShape::full_block() }),
            ..BlockState::default()
        });
        let mut cart = cart_in_cell();
        tick_minecart(&mut cart, &hot, old(release(1, 20, 4)));
        assert_eq!(cart.damage, 0.0);
    }

    #[test]
    fn lava_hurts_a_cart_by_the_releases_way() {
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let damage = |version: MinecraftVersion| {
            let mut cart = cart_in_cell();
            tick_minecart(&mut cart, &world, old(version));
            cart.damage
        };
        // Entity.move's fire test, for its lava too
        assert_eq!(damage(release(1, 15, 2)), 10.0);
        // 1.16 dropped that and has no test of a cart's own
        assert_eq!(damage(release(1, 16, 1)), 0.0);
        // AbstractMinecart.tick's, of 4, from 1.16.2
        assert_eq!(damage(release(1, 16, 2)), 40.0);
        assert_eq!(damage(release(1, 17, 0)), 40.0);
        assert_eq!(damage(release(1, 21, 4)), 40.0);
    }

    #[test]
    fn a_cart_in_lava_goes_in_two_ticks_and_halves_its_fall() {
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let rules = old(release(1, 20, 4));
        let mut cart = MinecartState { fall_distance: 2.0, ..cart_in_cell() };
        tick_minecart(&mut cart, &world, rules);
        assert_eq!((cart.removed, cart.fall_distance), (None, 0.0));
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, Some(MinecartRemoval::Destroyed));
        // falling in, it keeps half of what it fell
        let mut cart = MinecartState { fall_distance: 2.0, on_ground: false, ..cart_in_cell() };
        cart.position.y = 0.5;
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.fall_distance, f64::from((2.0_f32 + (0.04_f64 as f32)) * 0.5));
    }

    #[test]
    fn a_cart_takes_no_lava_on_its_first_tick() {
        // isInLava is false on a first tick (1.16.2 Entity.java:1179, 26.3 :1779-1781)
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let mut cart = cart_in_cell();
        cart.first_tick = true;
        tick_minecart(&mut cart, &world, old(release(1, 17, 0)));
        assert_eq!(cart.damage, 0.0);
        tick_minecart(&mut cart, &world, old(release(1, 17, 0)));
        assert_eq!(cart.damage, 40.0);
    }

    #[test]
    fn lava_draws_the_sound_of_its_hurt_from_a_carts_random() {
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let mut cart = MinecartState { random_seed: 42, ..cart_in_cell() };
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        let mut random = EntityRandom(42);
        random.next_float();
        assert_eq!(cart.random_seed, random.0);
    }

    #[test]
    fn lava_hurts_a_cart_without_a_sound_before_1_17() {
        // 1.16.5 Entity.lavaHurt only hurts; the burn sound and its draw come in 1.17 (:489-496)
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let mut cart = MinecartState { random_seed: 42, ..cart_in_cell() };
        tick_minecart(&mut cart, &world, old(release(1, 16, 5)));
        assert_eq!(cart.random_seed, 42);
    }

    #[test]
    fn lava_in_the_blocks_of_a_move_hurts_as_well_from_1_21_5() {
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let mut cart = cart_in_cell();
        tick_minecart(&mut cart, &world, old(release(1, 21, 5)));
        assert!(cart.damage > 40.0 || cart.removed.is_some());
        assert_eq!(cart.removed, Some(MinecartRemoval::Destroyed));
    }

    #[test]
    fn a_tnt_cart_in_lava_primes_where_a_plain_one_only_hurts() {
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let mut cart = MinecartState { variant: MinecartVariant::Tnt, ..cart_in_cell() };
        cart.random_seed = 42;
        let tick = tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert!(tick.primed);
        // the fuse of its draws, counted off in the same tick
        assert_eq!(cart.fuse, 24);
        assert_eq!(cart.removed, None);
    }

    #[test]
    fn a_cart_under_the_bottom_of_its_level_is_gone() {
        let world = WorldSnapshot::new();
        let gone = |rules: MinecartRules, y: f64| {
            let mut cart = resting(MinecartVariant::Rideable, Vec3::new(0.5, y, 0.5));
            tick_minecart(&mut cart, &world, rules);
            cart.removed
        };
        assert_eq!(gone(old(release(1, 16, 5)), -64.0), None);
        assert_eq!(gone(old(release(1, 16, 5)), -64.5), Some(MinecartRemoval::BelowWorld));
        let deep = old(release(1, 18, 2)).in_level(-64, true);
        assert_eq!(gone(deep, -65.0), None);
        assert_eq!(gone(deep, -128.5), Some(MinecartRemoval::BelowWorld));
    }

    #[test]
    fn a_cart_gone_is_out_of_the_tick_of_the_rest() {
        let world = straight(30);
        let rules = old(release(1, 20, 4));
        let mut carts = [
            MinecartState { removed: Some(MinecartRemoval::Exploded), ..tnt_on_rail(1.5, 0.0) },
            cart_at(1.8, 0.0),
        ];
        let before = carts[0];
        let ticks = tick_minecarts(&mut carts, &mut [], &world, rules);
        assert_eq!(ticks[0], MinecartTick::default());
        assert_eq!(carts[0], before);
        // and nothing is pushed by it or pushes it
        assert_eq!(carts[1].velocity.x, 0.0);
    }

    fn portal_world(portal: BlockEffect) -> WorldSnapshot {
        let mut world = floor(8);
        passable(&mut world, (0, 0, 0), portal);
        world
    }

    #[test]
    fn a_cart_goes_through_a_nether_portal_the_tick_after_it_is_in_one() {
        let world = portal_world(BlockEffect::NetherPortal);
        for version in [release(1, 8, 8), release(1, 20, 4), release(26, 3, 0)] {
            let mut cart = cart_in_cell();
            tick_minecart(&mut cart, &world, old(version));
            assert_eq!(cart.removed, None, "{version}");
            assert!(cart.inside_portal, "{version}");
            tick_minecart(&mut cart, &world, old(version));
            assert_eq!(cart.removed, Some(MinecartRemoval::Portal(MinecartPortal::Nether)), "{version}");
        }
    }

    #[test]
    fn a_cart_waits_a_tick_longer_in_a_portal_from_1_9_to_1_16_1() {
        let world = portal_world(BlockEffect::NetherPortal);
        let mut cart = cart_in_cell();
        let rules = old(release(1, 9, 4));
        tick_minecart(&mut cart, &world, rules);
        tick_minecart(&mut cart, &world, rules);
        assert_eq!((cart.removed, cart.portal_time), (None, 1));
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, Some(MinecartRemoval::Portal(MinecartPortal::Nether)));
    }

    #[test]
    fn a_portal_cooldown_keeps_a_cart_where_it_is() {
        let world = portal_world(BlockEffect::NetherPortal);
        let mut cart = MinecartState { portal_cooldown: 5, ..cart_in_cell() };
        let rules = old(release(1, 20, 4));
        tick_minecart(&mut cart, &world, rules);
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, None);
        assert_eq!(cart.portal_cooldown, 300);
    }

    #[test]
    fn a_vehicle_cools_down_for_ten_ticks_from_1_21_2() {
        let world = portal_world(BlockEffect::NetherPortal);
        for (rules, cooldown) in [(old(release(1, 21, 1)), 300), (old(release(1, 21, 2)), 10), (old(release(26, 3, 0)), 10)] {
            let mut cart = MinecartState { portal_cooldown: 5, ..cart_in_cell() };
            tick_minecart(&mut cart, &world, rules);
            assert_eq!(cart.portal_cooldown, cooldown, "{rules:?}");
        }
    }

    #[test]
    fn a_cart_carrying_someone_stays_out_of_a_portal_until_1_21() {
        let world = portal_world(BlockEffect::NetherPortal);
        let ridden = || MinecartState { rider: Some(MinecartRider::default()), ..cart_in_cell() };
        let mut cart = ridden();
        let rules = old(release(1, 20, 4));
        tick_minecart(&mut cart, &world, rules);
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, None);
        let mut cart = ridden();
        let rules = old(release(1, 21, 0));
        tick_minecart(&mut cart, &world, rules);
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, Some(MinecartRemoval::Portal(MinecartPortal::Nether)));
    }

    #[test]
    fn an_end_portal_takes_a_cart_at_once_until_1_21() {
        let mut world = WorldSnapshot::new();
        passable(&mut world, (0, 0, 0), BlockEffect::EndPortal);
        let in_it = || MinecartState { position: Vec3::new(0.5, 0.5, 0.5), no_gravity: true, ..cart_in_cell() };
        let mut cart = in_it();
        tick_minecart(&mut cart, &world, old(release(1, 20, 4)));
        assert_eq!(cart.removed, Some(MinecartRemoval::Portal(MinecartPortal::End)));
        let mut cart = in_it();
        tick_minecart(&mut cart, &world, old(release(1, 21, 0)));
        assert_eq!(cart.removed, None);
        tick_minecart(&mut cart, &world, old(release(1, 21, 0)));
        assert_eq!(cart.removed, Some(MinecartRemoval::Portal(MinecartPortal::End)));
    }

    #[test]
    fn an_end_portal_is_only_the_shape_of_its_block_from_1_9() {
        let mut world = WorldSnapshot::new();
        passable(&mut world, (0, 0, 0), BlockEffect::EndPortal);
        let over_it = |version: MinecraftVersion| {
            let mut cart = MinecartState { position: Vec3::new(0.5, 0.8, 0.5), no_gravity: true, ..cart_in_cell() };
            tick_minecart(&mut cart, &world, old(version));
            cart.removed
        };
        assert_eq!(over_it(release(1, 9, 4)), None);
        assert_eq!(over_it(release(1, 8, 8)), Some(MinecartRemoval::Portal(MinecartPortal::End)));
        // under the shape, from when it has an underside
        let under_it = |version: MinecraftVersion| {
            let mut cart = MinecartState { position: Vec3::new(0.5, -0.45, 0.5), no_gravity: true, ..cart_in_cell() };
            tick_minecart(&mut cart, &world, old(version));
            cart.removed
        };
        assert_eq!(under_it(release(1, 17, 0)), None);
        assert_eq!(under_it(release(1, 16, 5)), Some(MinecartRemoval::Portal(MinecartPortal::End)));
    }

    #[test]
    fn a_nether_portal_with_nothing_beyond_it_leaves_a_cart_its_cooldown() {
        let world = portal_world(BlockEffect::NetherPortal);
        let mut rules = old(release(1, 20, 4));
        rules.nether_exit = false;
        let mut cart = cart_in_cell();
        tick_minecart(&mut cart, &world, rules);
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.removed, None);
        assert_eq!((cart.portal_cooldown, cart.portal_time), (300, 0));
    }

    #[test]
    fn a_fall_off_the_rail_counts_and_a_landing_ends_it() {
        let world = floor(8);
        let rules = old(release(1, 20, 4));
        let mut cart = resting(MinecartVariant::Rideable, Vec3::new(0.5, 2.0, 0.5));
        cart.on_ground = false;
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.fall_distance, f64::from(0.04_f64 as f32));
        // down to the floor, with a landing that adds nothing of its last move and leaves nothing
        for _ in 0..20 {
            tick_minecart(&mut cart, &world, rules);
        }
        assert!(cart.on_ground);
        assert_eq!(cart.fall_distance, 0.0);
    }

    #[test]
    fn fall_distance_adds_in_the_releases_type() {
        let world = WorldSnapshot::new();
        let fell = |version: MinecraftVersion| {
            let mut cart = resting(MinecartVariant::Rideable, Vec3::new(0.5, 50.0, 0.5));
            cart.on_ground = false;
            cart.fall_distance = 0.1;
            cart.velocity = Vec3::new(0.0, -0.3, 0.0);
            tick_minecart(&mut cart, &world, old(version));
            cart.fall_distance
        };
        let moved = -0.34_f64;
        assert_eq!(fell(release(1, 18, 2)), f64::from((f64::from(0.1_f32 as f64 as f32) - moved) as f32).max(0.0).min(10.0).max(fell(release(1, 18, 2))));
        assert_ne!(fell(release(26, 3, 0)) as f32 as f64, fell(release(26, 3, 0)));
    }

    #[test]
    fn water_stops_a_fall_adding_up_and_a_rail_clears_it() {
        let mut world = straight(8);
        let rules = old(release(1, 20, 4));
        let mut cart = MinecartState { fall_distance: 3.0, ..cart_at(1.5, 0.0) };
        tick_minecart(&mut cart, &world, rules);
        assert_eq!(cart.fall_distance, 0.0);
        world.set_block((0, 5, 0), BlockState { fluid: FluidState::Water, fluid_source: true, fluid_level: 8, water_block: true, ..BlockState::default() });
        let mut wet = resting(MinecartVariant::Rideable, Vec3::new(0.5, 5.0, 0.5));
        wet.on_ground = false;
        wet.in_water = true;
        wet.fall_distance = 2.0;
        tick_minecart(&mut wet, &world, rules);
        assert_eq!(wet.fall_distance, 0.0);
    }

    #[test]
    fn a_clients_tick_only_runs_its_numbers_down() {
        let mut cart = MinecartClientEffects { hurt_time: 1, damage: 1.5, fuse: 1 };
        tick_client_effects(&mut cart);
        assert_eq!(cart, MinecartClientEffects { hurt_time: 0, damage: 0.5, fuse: 0 });
        tick_client_effects(&mut cart);
        assert_eq!(cart, MinecartClientEffects { hurt_time: 0, damage: -0.5, fuse: 0 });
    }
    /// A cart in the air nothing moves, so that what a blast gives it is all its tick's change.
    fn floating(variant: MinecartVariant, x: f64) -> MinecartState {
        MinecartState { variant, position: Vec3::new(x, 10.0, 0.5), no_gravity: true, random_seed: 42, ..MinecartState::default() }
    }

    fn lit(x: f64) -> MinecartState {
        MinecartState { fuse: 0, ..floating(MinecartVariant::Tnt, x) }
    }

    /// The blast of a still TNT cart (power 4, a reach of 8) at x 0.5 on a plain cart `away` blocks off in the open, worked
    /// out apart from the engine: Explosion.explode's impact (1 - distance / 8) and the way out from the cart's eyes.
    fn open_blast(away: f64, version: MinecraftVersion) -> (f32, Vec3) {
        open_blast_from(away, f64::from(0.7_f32 * 0.85), version)
    }

    fn open_blast_from(away: f64, eye: f64, version: MinecraftVersion) -> (f32, Vec3) {
        let (distance, length) = if version.at_least(1, 17, 0) {
            (away / 8.0, (away * away + eye * eye).sqrt())
        } else if version.at_least(1, 14, 0) {
            (f64::from(away as f32 / 8.0), f64::from((away * away + eye * eye).sqrt() as f32))
        } else {
            (f64::from(away as f32) / 8.0, f64::from((away * away + eye * eye).sqrt() as f32))
        };
        let impact = 1.0 - distance;
        let per_block = if version.at_least(1, 9, 0) { 7.0 } else { 8.0 };
        let hurt = (impact * impact + impact) / 2.0 * per_block * 8.0 + 1.0;
        let damage = if version.at_least(1, 20, 3) { hurt as f32 } else { hurt as i32 as f32 };
        (damage, Vec3::new(away / length * impact, eye / length * impact, 0.0))
    }

    #[test]
    fn a_blast_hurts_and_pushes_a_cart_out_of_its_way() {
        let world = floor(16);
        for version in [release(26, 3, 0), release(1, 20, 4), release(1, 16, 5), release(1, 13, 2)] {
            // (the plain cart ticks first, so that its velocity after the scene is the push alone)
            let mut carts = [floating(MinecartVariant::Rideable, 8.1), lit(0.5)];
            let ticks = tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(ticks[1].explosions.len(), 1, "{version}");
            assert_eq!(carts[1].removed, Some(MinecartRemoval::Exploded), "{version}");
            let (damage, push) = open_blast(8.1 - 0.5, version);
            assert_eq!(carts[0].removed, None, "{version}");
            assert_eq!(carts[0].damage, damage * 10.0, "{version}");
            assert_eq!(carts[0].hurt_time, 10, "{version}");
            assert_eq!(carts[0].velocity, push, "{version}");
        }
    }

    #[test]
    fn a_near_blast_breaks_a_cart_and_one_out_of_reach_does_nothing() {
        let world = floor(16);
        let mut near = [floating(MinecartVariant::Rideable, 3.5), lit(0.5)];
        tick_minecarts(&mut near, &mut [], &world, old(release(26, 3, 0)));
        assert_eq!(near[0].removed, Some(MinecartRemoval::Destroyed));
        let mut far = [floating(MinecartVariant::Rideable, 8.6), lit(0.5)];
        tick_minecarts(&mut far, &mut [], &world, old(release(26, 3, 0)));
        assert_eq!((far[0].removed, far[0].damage, far[0].velocity), (None, 0.0, Vec3::ZERO));
    }

    #[test]
    fn a_blast_pushed_what_is_no_living_thing_a_whole_block_in_1_9_and_1_10() {
        let world = floor(16);
        let push = |version| {
            let mut carts = [floating(MinecartVariant::Rideable, 8.1), lit(0.5)];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            (carts[0].damage, carts[0].velocity)
        };
        let (damage, whole) = push(release(1, 10, 2));
        assert_eq!(damage, 20.0);
        assert!((whole.length_squared().sqrt() - 1.0).abs() < 1.0e-6, "{whole:?}");
        let (damage, share) = push(release(1, 11, 2));
        let (want_damage, want_push) = open_blast(7.6, release(1, 11, 2));
        assert_eq!((damage, share), (want_damage * 10.0, want_push));
        // (8 a block of reach before 1.9: (int) of 2.62)
        let (want_damage, want_push) = open_blast(7.6, release(1, 8, 8));
        assert_eq!(want_damage, 2.0);
        // (the cart falls in its own tick before 1.10, which reads no NoGravity)
        let (damage, pushed) = push(release(1, 8, 8));
        assert_eq!((damage, pushed.x), (20.0, want_push.x));
    }

    #[test]
    fn a_wall_between_keeps_the_push_off_but_not_the_least_hurt() {
        let mut world = floor(16);
        for y in 8..13 {
            for z in -2..3 {
                world.set_block((4, y, z), BlockState {
                    shape: Some(BlockShape::full_block()),
                    outline_shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                });
            }
        }
        for version in [release(26, 3, 0), release(1, 15, 2), release(1, 12, 2)] {
            let mut carts = [floating(MinecartVariant::Rideable, 8.1), lit(0.5)];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!((carts[0].damage, carts[0].velocity), (10.0, Vec3::ZERO), "{version}");
        }
    }

    #[test]
    fn a_blast_sets_off_a_tnt_cart_at_once_before_1_12_and_lights_it_after() {
        let world = floor(16);
        let mut carts = [lit(0.5), floating(MinecartVariant::Tnt, 3.5)];
        let ticks = tick_minecarts(&mut carts, &mut [], &world, old(release(1, 11, 2)));
        assert_eq!(ticks[0].explosions.len(), 2);
        assert_eq!(carts[1].removed, Some(MinecartRemoval::Exploded));
        let mut carts = [lit(0.5), floating(MinecartVariant::Tnt, 3.5)];
        let ticks = tick_minecarts(&mut carts, &mut [], &world, old(release(1, 12, 2)));
        assert_eq!(ticks[0].explosions.len(), 1);
        assert!(!ticks[0].primed, "the other cart's priming is its own");
        assert_eq!(carts[1].removed, None);
        assert!((0..40).contains(&carts[1].fuse), "{}", carts[1].fuse);
    }

    #[test]
    fn a_blast_pushes_the_entities_about_it_from_their_eyes() {
        let world = floor(16);
        let pig = |x: f64| MinecartOther {
            position: Vec3::new(x, 10.0, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.45, 10.0, 0.05), Vec3::new(x + 0.45, 10.9, 0.95)),
            living: true,
            pushable: true,
            ..MinecartOther::default()
        };
        let mut carts = [lit(0.5)];
        let mut others = [pig(6.5), MinecartOther { eye_height: Some(0.0), ..pig(6.5) }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(26, 3, 0)));
        let eye = f64::from((10.9 - 10.0) as f32 * 0.85);
        let length = (36.0 + eye * eye).sqrt();
        let impact = 1.0 - 6.0 / 8.0;
        assert_eq!(others[0].velocity, Vec3::new(6.0 / length * impact, eye / length * impact, 0.0));
        assert_eq!(others[1].velocity, Vec3::new(impact, 0.0, 0.0));
    }

    fn gateway_at(position: BlockPosition, exit: BlockPosition) -> MinecartGateway {
        MinecartGateway { position, exit: Some(exit), cooldown: 0, age: 300, undecided: false }
    }

    /// A cart resting on the floor in the gateway's block (0, 0, 0).
    fn in_gateway() -> MinecartState {
        resting(MinecartVariant::Rideable, Vec3::new(0.5, 0.0, 0.5))
    }

    fn tick_gateway(cart: &mut MinecartState, others: &mut [MinecartOther], gateway: &mut MinecartGateway, version: MinecraftVersion) {
        tick_minecarts_with(std::slice::from_mut(cart), others, &mut [], &mut [], std::slice::from_mut(gateway), &floor(8), old(version));
    }

    #[test]
    fn a_gateways_own_tick_sends_what_is_in_it_before_1_21() {
        let exit = (40, 3, -20);
        for (version, lift, cooldown, portal_cooldown) in [
            (release(1, 10, 2), 0.5, 20, 0),
            (release(1, 15, 2), 0.5, 40, 0),
            (release(1, 16, 1), 0.0, 40, 0),
            (release(1, 20, 4), 0.0, 40, 300),
        ] {
            let mut cart = in_gateway();
            let mut gateway = gateway_at((0, 0, 0), exit);
            tick_gateway(&mut cart, &mut [], &mut gateway, version);
            assert_eq!(cart.position, Vec3::new(40.5, 3.0 + lift, -19.5), "{version}");
            assert_eq!((gateway.cooldown, cart.portal_cooldown, gateway.age), (cooldown, portal_cooldown, 301), "{version}");
            // (cooling down, it takes nothing and counts down)
            let mut again = in_gateway();
            tick_gateway(&mut again, &mut [], &mut gateway, version);
            assert_eq!((again.position, gateway.cooldown), (Vec3::new(0.5, 0.0, 0.5), cooldown - 1), "{version}");
        }
        // (1.8 has no gateway)
        let mut cart = in_gateway();
        let mut gateway = gateway_at((0, 0, 0), exit);
        tick_gateway(&mut cart, &mut [], &mut gateway, release(1, 8, 8));
        assert_eq!((cart.position, gateway.cooldown), (Vec3::new(0.5, 0.0, 0.5), 0));
    }

    #[test]
    fn a_gateway_leaves_a_cart_on_its_portal_cooldown_from_1_16_2_and_cools_down_with_no_exit() {
        let mut cart = MinecartState { portal_cooldown: 5, ..in_gateway() };
        let mut gateway = gateway_at((0, 0, 0), (40, 3, -20));
        tick_gateway(&mut cart, &mut [], &mut gateway, release(1, 16, 2));
        assert_eq!((cart.position, gateway.cooldown), (Vec3::new(0.5, 0.0, 0.5), 0));
        let mut cart = MinecartState { portal_cooldown: 5, ..in_gateway() };
        tick_gateway(&mut cart, &mut [], &mut gateway, release(1, 16, 1));
        assert_eq!(cart.position, Vec3::new(40.5, 3.0, -19.5));
        let mut cart = in_gateway();
        let mut nowhere = MinecartGateway { exit: None, ..gateway_at((0, 0, 0), (0, 0, 0)) };
        tick_gateway(&mut cart, &mut [], &mut nowhere, release(1, 20, 4));
        assert_eq!((cart.position, nowhere.cooldown), (Vec3::new(0.5, 0.0, 0.5), 40));
    }

    #[test]
    fn a_gateway_picks_among_what_is_in_it_by_the_levels_random() {
        let mut gateway = gateway_at((0, 0, 0), (40, 3, -20));
        let stranger = MinecartOther {
            position: Vec3::new(0.5, 0.0, 0.5),
            bounds: Aabb::new(Vec3::new(0.2, 0.0, 0.2), Vec3::new(0.8, 1.8, 0.8)),
            ..MinecartOther::default()
        };
        let mut cart = in_gateway();
        tick_gateway(&mut cart, &mut [stranger], &mut gateway, release(1, 20, 4));
        assert!(gateway.undecided);
        assert_eq!((cart.position, gateway.cooldown), (Vec3::new(0.5, 0.0, 0.5), 40));
        // (its rider goes with it, whichever of the two the random picks)
        let mut gateway = gateway_at((0, 0, 0), (40, 3, -20));
        let mut cart = in_gateway();
        let mut rider = [MinecartOther { rides: Some(0), passenger: true, ..stranger }];
        tick_gateway(&mut cart, &mut rider, &mut gateway, release(1, 20, 4));
        assert!(!gateway.undecided);
        assert_eq!(cart.position, Vec3::new(40.5, 3.0, -19.5));
    }

    #[test]
    fn from_1_21_a_gateway_takes_a_cart_in_its_move_and_sends_it_on_its_next_tick() {
        let world = floor(8);
        let mut cart = MinecartState { velocity: Vec3::new(0.2, 0.0, 0.0), ..resting(MinecartVariant::Rideable, Vec3::new(-0.55, 0.0, 0.5)) };
        let mut gateway = gateway_at((0, 0, 0), (40, 3, -20));
        let rules = old(release(1, 21, 0));
        tick_minecarts_with(std::slice::from_mut(&mut cart), &mut [], &mut [], &mut [], std::slice::from_mut(&mut gateway), &world, rules);
        assert_eq!((cart.portal, cart.inside_portal), (Some(MinecartPortal::Gateway((0, 0, 0))), true));
        assert_eq!(gateway.cooldown, 39, "set in the move, counted down by its block entity after the entities");
        tick_minecarts_with(std::slice::from_mut(&mut cart), &mut [], &mut [], &mut [], std::slice::from_mut(&mut gateway), &world, rules);
        assert_eq!(cart.portal_cooldown, 300);
        assert!(cart.position.distance_squared(Vec3::new(40.5, 3.0, -19.5)) < 1.0, "{:?}", cart.position);
        // (1.20.6: the gateway's block entity, not the cart's move)
        let mut cart = MinecartState { velocity: Vec3::new(0.2, 0.0, 0.0), ..resting(MinecartVariant::Rideable, Vec3::new(-0.55, 0.0, 0.5)) };
        let mut gateway = gateway_at((0, 0, 0), (40, 3, -20));
        tick_minecarts_with(std::slice::from_mut(&mut cart), &mut [], &mut [], &mut [], std::slice::from_mut(&mut gateway), &world, old(release(1, 20, 6)));
        assert_eq!(cart.portal, None);
        assert_eq!(cart.position, Vec3::new(40.5, 3.0, -19.5));
    }

    // B280 review of minecart.rs: the engine bugs it found, each against the release that has it.

    #[test]
    fn a_creative_hit_takes_an_invulnerable_cart_away_and_no_other_hit_does() {
        let world = straight(8);
        for version in [release(1, 8, 8), release(1, 20, 2), release(1, 20, 3), release(1, 21, 1), release(1, 21, 2), release(26, 3, 0)] {
            let mut cart = MinecartState { invulnerable: true, ..cart_at(2.5, 0.0) };
            hurt_minecart(&mut cart, &mut [], hurt(MinecartDamage::Plain, 1.0), &world, old(version));
            assert_eq!((cart.removed, cart.damage, cart.hurt_time), (None, 0.0, 0), "{version}");
            let creative = MinecartHurt { creative: true, ..hurt(MinecartDamage::Plain, 1.0) };
            hurt_minecart(&mut cart, &mut [], creative, &world, old(version));
            assert_eq!((cart.removed, cart.hurt_time), (Some(MinecartRemoval::Discarded), 10), "{version}");
        }
    }

    #[test]
    fn an_improved_cart_its_blocks_broke_steps_no_further() {
        // (a rail that is a cactus too: the first step's blocks break the cart, which then stays where that step left it)
        let mut world = straight(40);
        world.set_block((2, 0, 0), BlockState {
            tags: TAG_RAIL,
            shape: Some(BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO), effect: BlockEffect::Cactus, ..BlockShape::full_block() }),
            ..BlockState::default()
        });
        world.set_block_properties((2, 0, 0), vec![("shape".to_owned(), "east_west".to_owned())]);
        let mut cart = MinecartState { damage: 39.0, ..improved_cart(1.9, 1.5) };
        let tick = tick_minecart(&mut cart, &world, improved(40));
        assert_eq!(cart.removed, Some(MinecartRemoval::Destroyed));
        assert!(cart.position.x < 2.001, "{:?}", cart.position);
        assert_eq!(tick.steps.len(), 1, "{tick:?}");
        // and a cart gone before the loop takes no step at all: under the level, it falls by its gravity but stays put
        let mut gone = MinecartState { position: Vec3::new(1.5, -100.0, 0.5), on_rails: false, ..improved_cart(1.5, 0.5) };
        let tick = tick_minecart(&mut gone, &world, improved(8));
        assert_eq!(gone.removed, Some(MinecartRemoval::BelowWorld));
        assert_eq!(gone.position, Vec3::new(1.5, -100.0, 0.5));
        assert!(tick.steps.is_empty(), "{tick:?}");
    }

    fn mob_at(x: f64) -> MinecartOther {
        MinecartOther {
            position: Vec3::new(x, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.45, 0.0625, 0.05), Vec3::new(x + 0.45, 0.9625, 0.95)),
            pushable: true,
            living: true,
            ..MinecartOther::default()
        }
    }

    #[test]
    fn a_mob_just_off_a_cart_is_not_picked_up_again_from_1_9() {
        let world = straight(30);
        for (version, rules) in [(release(1, 20, 4), old(release(1, 20, 4))), (release(1, 9, 4), old(release(1, 9, 4))), (release(26, 3, 0), improved(8))] {
            let mut others = [MinecartOther { boarding_cooldown: 30, ..mob_at(3.2) }];
            let mut carts = [MinecartState { on_rails: rules.behavior == MinecartBehavior::Improved, ..cart_at(2.5, 0.25) }];
            if rules.behavior == MinecartBehavior::Improved {
                carts[0].position.y = 0.1;
            }
            tick_minecarts(&mut carts, &mut others, &world, rules);
            assert_eq!((others[0].rides, carts[0].rider.is_some()), (None, false), "{version}");
            assert_eq!(others[0].boarding_cooldown, 29, "{version}: its own tick counts it down");
        }
        // 1.8 has no cooldown
        let mut others = [MinecartOther { boarding_cooldown: 30, ..mob_at(3.6) }];
        let mut carts = [cart_at(3.0, 0.2)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 8, 8)));
        assert_eq!(others[0].rides, Some(0));
        // the activator rail that throws a mob off leaves it the 60 ticks, less its own tick's one
        let mut world = straight(30);
        world.set_block((2, 0, 0), BlockState { tags: TAG_RAIL | TAG_ACTIVATOR_RAIL, ..BlockState::default() });
        world.set_block_properties((2, 0, 0), vec![("shape".to_owned(), "east_west".to_owned()), ("powered".to_owned(), "true".to_owned())]);
        for (version, left) in [(release(1, 8, 8), 0), (release(1, 9, 4), 59), (release(26, 3, 0), 59)] {
            let mut carts = [MinecartState { rider: Some(MinecartRider { living: true, ..MinecartRider::default() }), ..cart_at(2.5, 0.0) }];
            let mut others = [MinecartOther { passenger: true, rides: Some(0), ..mob_at(2.5) }];
            tick_minecarts(&mut carts, &mut others, &world, old(version));
            assert_eq!((others[0].rides, others[0].boarding_cooldown), (None, left), "{version}");
        }
    }

    #[test]
    fn a_cart_pushes_nothing_of_its_own_rider() {
        let world = straight(30);
        for rules in [old(release(1, 20, 4)), old(release(1, 10, 2)), improved(8)] {
            let improved_path = rules.behavior == MinecartBehavior::Improved;
            let mut carts = [MinecartState { rider: Some(MinecartRider { living: true, ..MinecartRider::default() }), on_rails: improved_path, ..cart_at(2.5, 0.3) }];
            if improved_path {
                carts[0].position.y = 0.1;
            }
            // (where the rider was at the tick's start: the cart has moved off it by the time it pushes)
            let mut others = [MinecartOther { passenger: true, rides: Some(0), ..mob_at(2.5) }];
            tick_minecarts(&mut carts, &mut others, &world, rules);
            assert_eq!(others[0].velocity, Vec3::ZERO, "{:?}", rules.release);
            assert_eq!(others[0].rides, Some(0));
        }
    }

    #[test]
    fn from_1_21_a_cart_takes_its_riders_through_a_portal() {
        let world = portal_world(BlockEffect::NetherPortal);
        let rider_in_cell = || MinecartOther { passenger: true, rides: Some(0), ..mob_at(0.5) };
        for version in [release(1, 21, 0), release(1, 21, 2), release(26, 3, 0)] {
            let mut carts = [MinecartState { rider: Some(MinecartRider { living: true, ..MinecartRider::default() }), ..cart_in_cell() }];
            let mut others = [rider_in_cell()];
            tick_minecarts(&mut carts, &mut others, &world, old(version));
            tick_minecarts(&mut carts, &mut others, &world, old(version));
            assert_eq!(carts[0].removed, Some(MinecartRemoval::Portal(MinecartPortal::Nether)), "{version}");
            assert_eq!((others[0].removed, others[0].rides, others[0].passenger), (true, Some(0), true), "{version}");
        }
    }

    #[test]
    fn a_gateway_sends_a_carts_riders_along_by_the_releases_way() {
        // (the gateway's block, which from 1.21.5 the cells a move passes have to hold for its entityInside)
        let mut world = floor(8);
        passable(&mut world, (0, 0, 0), BlockEffect::None);
        // (the rider 0.25 west of its cart, where the move before left it)
        let rider = || MinecartOther { passenger: true, rides: Some(0), ..mob_at(-0.55) };
        let cart = || MinecartState {
            velocity: Vec3::new(0.2, 0.0, 0.0),
            rider: Some(MinecartRider { living: true, ..MinecartRider::default() }),
            ..resting(MinecartVariant::Rideable, Vec3::new(-0.3, 0.0, 0.5))
        };
        for (version, offset, cooldown) in [(release(1, 21, 0), false, 59), (release(1, 21, 2), true, 0), (release(26, 3, 0), true, 0)] {
            let mut carts = [cart()];
            let mut others = [rider()];
            let mut gateways = [gateway_at((0, 0, 0), (40, 3, -20))];
            tick_minecarts_with(&mut carts, &mut others, &mut [], &mut [], &mut gateways, &world, old(version));
            let before = carts[0].position - others[0].position;
            tick_minecarts_with(&mut carts, &mut others, &mut [], &mut [], &mut gateways, &world, old(version));
            let exit = Vec3::new(40.5, 3.0, -19.5);
            let want = if offset { exit - before } else { exit };
            assert_eq!(others[0].position, want, "{version}");
            assert_eq!(others[0].bounds.min.x, want.x - 0.45, "{version}");
            assert_eq!((others[0].rides, others[0].removed, others[0].boarding_cooldown), (Some(0), false, cooldown), "{version}");
        }
    }

    fn twin_carts(power: f32) -> [MinecartState; 2] {
        let lit = MinecartState { fuse: 0, explosion_power_base: power, explosion_speed_factor: 0.0, ..resting(MinecartVariant::Tnt, Vec3::new(2.5, 0.0, 0.5)) };
        [lit, resting(MinecartVariant::Rideable, Vec3::new(2.5, 0.0, 0.5))]
    }

    #[test]
    fn a_blast_of_no_power_on_a_cart_at_its_place_is_nothing_then_nan_then_nothing() {
        let world = floor(8);
        for (version, nan) in [(release(1, 21, 8), false), (release(1, 21, 9), true), (release(1, 21, 10), true), (release(1, 21, 11), false), (release(26, 3, 0), false)] {
            let mut carts = twin_carts(0.0);
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(carts[0].removed, Some(MinecartRemoval::Exploded), "{version}");
            assert_eq!(carts[1].damage.is_nan(), nan, "{version}: {:?}", carts[1]);
            assert_eq!(carts[1].velocity.y.is_nan(), nan, "{version}: {:?}", carts[1]);
            if !nan {
                assert_eq!((carts[1].damage, carts[1].hurt_time), (0.0, 0), "{version}");
            }
        }
    }

    #[test]
    fn from_26_3_a_blast_spares_the_carts_allies_without_friendly_fire_but_still_pushes_them() {
        let world = floor(16);
        let team = MinecartTeam { id: 1, rule: CollisionRule::Always, allow_friendly_fire: false };
        for (version, spared) in [(release(26, 2, 0), false), (release(26, 3, 0), true)] {
            let lit = MinecartState { fuse: 0, team: Some(team), ..resting(MinecartVariant::Tnt, Vec3::new(2.5, 0.0, 0.5)) };
            let ally = MinecartState { team: Some(team), ..resting(MinecartVariant::Rideable, Vec3::new(7.5, 0.0, 0.5)) };
            let mut carts = [lit, ally];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(carts[1].damage == 0.0, spared, "{version}: {:?}", carts[1]);
            assert!(carts[1].velocity.x > 0.0, "{version}");
            // a team that allows it, or none on the other, is hurt as ever
            let friendly = MinecartTeam { allow_friendly_fire: true, ..team };
            let mut carts = [MinecartState { team: Some(friendly), ..lit }, MinecartState { team: Some(friendly), ..ally }];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert!(carts[1].damage > 0.0, "{version}");
            let mut carts = [lit, MinecartState { team: None, ..ally }];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert!(carts[1].damage > 0.0, "{version}");
        }
    }

    #[test]
    fn a_creative_players_blast_takes_carts_away_as_its_hit_from_the_releases_that_keep_its_source() {
        let world = floor(16);
        let near = || MinecartState { invulnerable: true, ..resting(MinecartVariant::Rideable, Vec3::new(5.5, 0.0, 0.5)) };
        // a burning arrow a creative player shot: its shooter is the blast's attacker from 1.19.3
        for (version, gone) in [(release(1, 19, 2), None), (release(1, 19, 3), Some(MinecartRemoval::Discarded)), (release(26, 3, 0), Some(MinecartRemoval::Discarded))] {
            let mut carts = [resting(MinecartVariant::Tnt, Vec3::new(2.5, 0.0, 0.5)), near()];
            let arrow = MinecartHurt { creative: true, burning_arrow: Some(1.0), ..hurt(MinecartDamage::Plain, 1.0) };
            let mut level_random = None;
            let mut scene = Scene {
                world: &world, rules: old(version), carts: &mut carts, others: &mut [], items: &mut [], containers: &mut [],
                gateways: &mut [], level_random: &mut level_random, current: 0, support: None, recorded: Vec::new(),
                old_position: Vec3::ZERO, old_y_rot: 0.0, consumed_item: false, tick: MinecartTick::default(),
            };
            scene.attack(arrow);
            assert_eq!(carts[0].removed, Some(MinecartRemoval::Exploded), "{version}");
            assert_eq!(carts[1].removed, gone, "{version}");
        }
        // a fuse a creative player's blast lit: its own blast is that player's from 1.21.5
        for (version, fuse) in [(release(1, 21, 4), false), (release(1, 21, 5), true), (release(1, 21, 9), true)] {
            let lit = MinecartState { fuse: 0, creative_ignition: true, ..resting(MinecartVariant::Tnt, Vec3::new(2.5, 0.0, 0.5)) };
            let mut carts = [lit, near()];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(carts[1].removed == Some(MinecartRemoval::Discarded), fuse, "{version}: {:?}", carts[1]);
        }
        // the creative player's blast that primes a TNT cart leaves it the source, from 1.21.5
        for (version, kept) in [(release(1, 21, 4), false), (release(1, 21, 5), true)] {
            let mut cart = resting(MinecartVariant::Tnt, Vec3::new(2.5, 0.0, 0.5));
            let blast = MinecartHurt { creative: true, ..hurt(MinecartDamage::Explosion, 2.0) };
            hurt_minecart(&mut cart, &mut [], blast, &world, old(version));
            assert!(cart.fuse > 0, "{version}");
            assert_eq!(cart.creative_ignition, kept, "{version}");
        }
    }

    #[test]
    fn the_improved_cart_settles_onto_a_curve_by_its_offset_as_java_adds_it() {
        // (Java's setPos(position.add(target.subtract(position))): the sum, an ulp off the target here)
        let mut world = straight(8);
        rail(&mut world, (2, 0, 0), "south_east");
        let mut cart = MinecartState { first_tick: true, ..improved_cart(2.285, 0.0) };
        cart.position.z = 0.063;
        tick_minecart(&mut cart, &world, improved(8));
        let target_z = 0.5 + 0.5 + (-0.5 * ((0.5 * (2.285 - 2.5) + -0.5 * ((0.063 - 0.5) - 0.5)) / 0.5));
        assert_eq!(cart.position.z, 0.063 + (target_z - 0.063));
        assert_ne!(cart.position.z, target_z, "the case has to be one the sum misses");
    }

    fn spawner_cart() -> MinecartState {
        resting(MinecartVariant::Spawner, Vec3::new(0.5, 0.0, 0.5))
    }

    fn player_at(x: f64) -> MinecartOther {
        MinecartOther {
            position: Vec3::new(x, 0.0, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.3, 0.0, 0.2), Vec3::new(x + 0.3, 1.8, 0.8)),
            player: true,
            living: true,
            ..MinecartOther::default()
        }
    }

    #[test]
    fn a_spawner_cart_counts_down_with_a_player_near_and_tries_at_0() {
        let world = floor(20);
        let rules = old(release(26, 3, 0));
        let mut cart = MinecartState { spawn_delay: 2, ..spawner_cart() };
        let tick = tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(16.4)], &world, rules).remove(0);
        assert_eq!((cart.spawn_delay, tick.spawner), (1, None));
        // (16 blocks from the block's centre is out of reach)
        tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(16.6)], &world, rules);
        assert_eq!(cart.spawn_delay, 1);
        tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(3.0)], &world, rules);
        let tick = tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(3.0)], &world, rules).remove(0);
        assert_eq!((cart.spawn_delay, tick.spawner), (0, Some(MinecartSpawn::Attempt((0, 0, 0)))));
    }

    #[test]
    fn a_spawner_left_at_minus_one_takes_its_least_delay_or_the_levels_draw() {
        let world = floor(20);
        let rules = old(release(1, 8, 8));
        let mut cart = MinecartState { spawn_delay: -1, min_spawn_delay: 5, max_spawn_delay: 5, ..spawner_cart() };
        tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(3.0)], &world, rules);
        assert_eq!(cart.spawn_delay, 4);
        let mut cart = MinecartState { spawn_delay: -1, ..spawner_cart() };
        let tick = tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(3.0)], &world, rules).remove(0);
        assert_eq!((cart.spawn_delay, tick.spawner), (-1, Some(MinecartSpawn::DrewDelay)));
    }

    #[test]
    fn the_spawner_game_rule_stops_a_spawner_cart_from_1_21_9() {
        let world = floor(20);
        for (version, left) in [(release(1, 21, 8), 1), (release(1, 21, 9), 2)] {
            let rules = MinecartRules { spawner_blocks: false, ..old(version) };
            let mut cart = MinecartState { spawn_delay: 2, ..spawner_cart() };
            tick_minecarts(std::slice::from_mut(&mut cart), &mut [player_at(3.0)], &world, rules);
            assert_eq!(cart.spawn_delay, left, "{version}");
        }
    }
    /// The rays of a blast of `power` from a TNT cart at `at` (primed when `fuse` is above -1).
    fn rays_of(at: Vec3, fuse: i32, power: f32, world: &WorldSnapshot, rules: MinecartRules) -> MinecartExplosion {
        let mut carts = [MinecartState { variant: MinecartVariant::Tnt, position: at, fuse, ..MinecartState::default() }];
        let mut level_random = rules.level_random;
        let mut scene = Scene {
            world,
            rules,
            carts: &mut carts,
            others: &mut [],
            items: &mut [],
            containers: &mut [],
            gateways: &mut [],
            level_random: &mut level_random,
            current: 0,
            support: None,
            recorded: Vec::new(),
            old_position: Vec3::ZERO,
            old_y_rot: 0.0,
            consumed_item: false,
            tick: MinecartTick::default(),
        };
        scene.explosion_rays(at, power)
    }

    fn resisting(world: &mut WorldSnapshot, at: BlockPosition, resistance: f32) {
        world.set_block(at, BlockState {
            shape: Some(BlockShape::full_block()),
            outline_shape: Some(BlockShape::full_block()),
            explosion_resistance: Some(resistance),
            ..BlockState::default()
        });
    }

    /// A wall across x = `x`, from y 7 to 13 and z -3 to 3.
    fn wall(world: &mut WorldSnapshot, x: i32, resistance: f32) {
        for y in 7..14 {
            for z in -3..4 {
                resisting(world, (x, y, z), resistance);
            }
        }
    }

    #[test]
    fn a_blast_in_the_open_breaks_out_to_its_weakest_and_strongest_rays_reach() {
        let world = WorldSnapshot::new();
        let center = Vec3::new(0.5, 10.0, 0.5);
        for version in [release(26, 3, 0), release(1, 16, 5), release(1, 12, 2), release(1, 8, 8)] {
            let blast = rays_of(center, 0, 4.0, &world, old(version));
            let farthest = |list: &[BlockPosition]| {
                list.iter()
                    .map(|&(x, y, z)| Vec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5).distance_squared(center).sqrt())
                    .fold(0.0, f64::max)
            };
            // (2.8 of strength lasts 13 steps of 0.3, 3.6 blocks out; 5.2 lasts 24, 6.9 out)
            assert!(blast.certain.contains(&(0, 10, 0)) && blast.certain.contains(&(3, 10, 0)), "{version}");
            assert!(farthest(&blast.certain) < 3.6 + 0.9, "{version}");
            assert!(!blast.certain.contains(&(5, 10, 0)), "{version}");
            assert!(blast.possible.contains(&(6, 10, 0)) && farthest(&blast.possible) < 6.9 + 0.9, "{version}");
            assert!(blast.certain.iter().all(|at| blast.possible.contains(at)), "{version}");
            assert_eq!((blast.blown, blast.level_random), (None, None), "{version}");
        }
    }

    #[test]
    fn blast_resistance_shortens_the_rays_through_a_wall() {
        let center = Vec3::new(0.5, 10.0, 0.5);
        let beyond = |list: &[BlockPosition]| list.iter().filter(|&&(x, y, z)| x >= 2 && (8..13).contains(&y) && (-2..3).contains(&z)).count();
        for version in [release(26, 3, 0), release(1, 13, 2), release(1, 8, 8)] {
            // obsidian (1200) takes 360 of a ray: nothing of the wall or past it breaks
            let mut world = WorldSnapshot::new();
            wall(&mut world, 2, 1200.0);
            let blast = rays_of(center, 0, 4.0, &world, old(version));
            assert_eq!(beyond(&blast.possible), 0, "{version}");
            // dirt (0.5) takes 0.24 a step: every draw breaks its face, and fewer of the weakest get past it than in the open
            let open = rays_of(center, 0, 4.0, &WorldSnapshot::new(), old(version));
            let mut world = WorldSnapshot::new();
            wall(&mut world, 2, 0.5);
            let blast = rays_of(center, 0, 4.0, &world, old(version));
            assert!(blast.certain.contains(&(2, 10, 0)), "{version}");
            let past = |list: &[BlockPosition]| list.iter().filter(|&&(x, _, _)| x >= 3).count();
            assert!(past(&blast.certain) < past(&open.certain), "{version}");
            assert!(past(&blast.possible) < past(&open.possible), "{version}");
            // stone (6) takes 1.89 a step: only the strongest draws break its face, and none gets through it
            let mut world = WorldSnapshot::new();
            wall(&mut world, 2, 6.0);
            wall(&mut world, 3, 6.0);
            let blast = rays_of(center, 0, 4.0, &world, old(version));
            assert!(!blast.certain.contains(&(2, 10, 0)), "{version}");
            assert!(blast.possible.contains(&(2, 10, 0)), "{version}");
            assert!(!blast.possible.contains(&(3, 10, 0)), "{version}");
        }
    }

    #[test]
    fn a_fluid_resists_a_blast_from_1_13_in_any_block_it_is_in() {
        let center = Vec3::new(0.5, 10.0, 0.5);
        let mut world = WorldSnapshot::new();
        // (water held in a block of no resistance of its own)
        for y in 7..14 {
            for z in -3..4 {
                world.set_block((2, y, z), BlockState { fluid: FluidState::Water, explosion_resistance: Some(0.0), ..BlockState::default() });
            }
        }
        let past = |version| rays_of(center, 0, 4.0, &world, old(version)).possible.contains(&(3, 10, 0));
        assert!(!past(release(1, 13, 2)));
        assert!(!past(release(26, 3, 0)));
        assert!(past(release(1, 12, 2)), "before 1.13 the block's own resistance alone");
    }

    #[test]
    fn a_primed_carts_blast_spares_the_rails_and_what_they_stand_on() {
        let mut world = WorldSnapshot::new();
        rail(&mut world, (0, 10, 0), "east_west");
        resisting(&mut world, (0, 9, 0), 0.5);
        resisting(&mut world, (1, 9, 0), 0.5);
        let center = Vec3::new(0.5, 10.0, 0.5);
        for version in [release(26, 3, 0), release(1, 13, 2), release(1, 8, 8)] {
            let primed = rays_of(center, 0, 4.0, &world, old(version));
            assert!(!primed.possible.contains(&(0, 10, 0)) && !primed.possible.contains(&(0, 9, 0)), "{version}");
            assert!(primed.certain.contains(&(1, 9, 0)), "{version}");
            // (a cart set off unprimed, by a wall or a fall, breaks them)
            let unprimed = rays_of(center, -1, 4.0, &world, old(version));
            assert!(unprimed.certain.contains(&(0, 10, 0)) && unprimed.certain.contains(&(0, 9, 0)), "{version}");
        }
    }

    #[test]
    fn a_ray_ends_at_the_top_of_the_level_from_1_17() {
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(0, 16, 0.0, 0.0, 6.0e7);
        let center = Vec3::new(0.5, 15.5, 0.5);
        let above = |version| rays_of(center, 0, 4.0, &world, old(version)).possible.iter().any(|&(_, y, _)| y >= 16);
        assert!(!above(release(1, 17, 0)));
        assert!(above(release(1, 16, 5)));
    }

    #[test]
    fn the_level_random_gives_the_exact_blast_and_moves_on_by_its_rays() {
        let mut world = WorldSnapshot::new();
        wall(&mut world, 2, 6.0);
        wall(&mut world, 3, 6.0);
        let center = Vec3::new(0.5, 10.0, 0.5);
        let rules = MinecartRules { level_random: Some(0x5DEE_CE66), ..old(release(26, 3, 0)) };
        let blast = rays_of(center, 0, 4.0, &world, rules);
        let blown = blast.blown.clone().unwrap();
        assert!(blast.certain.iter().all(|at| blown.contains(at)));
        assert!(blown.iter().all(|at| blast.possible.contains(at)));
        assert!(blown.len() > blast.certain.len() && blown.len() < blast.possible.len());
        // (one nextFloat a ray: 1352 steps of the generator)
        let mut random = EntityRandom(0x5DEE_CE66);
        for _ in 0..1352 {
            random.next_float();
        }
        assert_eq!(blast.level_random, Some(random.0));
        assert_eq!(rays_of(center, 0, 4.0, &world, rules), blast);
    }

    #[test]
    fn blast_resistance_changes_nothing_of_what_an_entity_is_seen_through() {
        // (the rays and the exposure read the same world, the blocks broken after the entities: a wall shields by its
        // collision shape alone, whatever its resistance, 26.3 ServerExplosion.java:69-100 getSeenPercent, :238-242)
        let hit = |resistance: f32, shaped: bool| {
            let mut world = floor(16);
            for y in 8..13 {
                for z in -2..3 {
                    let shape = shaped.then(BlockShape::full_block);
                    world.set_block((4, y, z), BlockState { shape, outline_shape: shape, explosion_resistance: Some(resistance), ..BlockState::default() });
                }
            }
            let mut carts = [floating(MinecartVariant::Rideable, 6.1), lit(0.5)];
            tick_minecarts(&mut carts, &mut [], &world, old(release(26, 3, 0)));
            (carts[0].damage, carts[0].velocity)
        };
        assert_eq!(hit(1200.0, true), hit(0.0, true));
        assert_eq!(hit(1200.0, true).1, Vec3::ZERO);
        let (_, open) = hit(1200.0, false);
        assert!(open.x > 0.0, "resistance without a shape shields nothing");
    }

    fn dropped(x: f64) -> MinecartItem {
        MinecartItem::new(Aabb::new(Vec3::new(x - 0.125, 10.0, 0.375), Vec3::new(x + 0.125, 10.25, 0.625)), MinecartStack { item: 1, count: 1, max_stack: 64 })
    }

    fn blast_items(items: &mut [MinecartItem], cart: MinecartState, version: MinecraftVersion) {
        blast_items_in(items, cart, &floor(16), version);
    }

    fn blast_items_in(items: &mut [MinecartItem], cart: MinecartState, world: &WorldSnapshot, version: MinecraftVersion) {
        tick_minecarts_among(&mut [cart], &mut [], items, &mut [], world, old(version));
    }

    #[test]
    fn a_blast_burns_up_an_item_near_it_and_pushes_one_further_off() {
        for version in [release(26, 3, 0), release(1, 20, 4), release(1, 16, 5), release(1, 12, 2)] {
            let mut items = [dropped(2.5), dropped(7.9)];
            blast_items(&mut items, lit(0.5), version);
            assert!(items[0].removed, "{version}");
            // (7.4 blocks of 8 off: a hurt of about 3.26, whole up to 1.20.2, off its health of 5)
            let (hurt, push) = open_blast_from(7.4, f64::from(0.25_f32 * 0.85), version);
            assert_eq!((items[1].removed, items[1].health), (false, (5.0 - hurt) as i32), "{version}");
            assert_eq!(items[1].velocity, push, "{version}");
        }
    }

    #[test]
    fn a_blast_proof_item_is_only_pushed_and_a_burnt_one_is_not_picked_up() {
        let mut items = [MinecartItem { blast_proof: true, ..dropped(2.5) }];
        blast_items(&mut items, lit(0.5), release(26, 3, 0));
        assert_eq!((items[0].removed, items[0].health), (false, 5));
        assert!(items[0].velocity.x > 0.0);
        // (the hopper cart beside it takes nothing of what the blast burnt)
        let mut carts = [lit(0.5), floating(MinecartVariant::Hopper, 2.5)];
        let mut items = [dropped(2.5)];
        tick_minecarts_among(&mut carts, &mut [], &mut items, &mut [], &floor(16), old(release(26, 3, 0)));
        assert!(items[0].removed);
        assert_eq!(carts[1].inventory[0].count, 0);
    }

    #[test]
    fn items_take_nothing_of_a_blast_from_water_in_1_21_2_to_1_21_5() {
        // (the cart in a pool, which its tick finds it in)
        let mut world = floor(16);
        for x in -1..2 {
            for z in -1..2 {
                world.set_block((x, 10, z), BlockState { fluid: FluidState::Water, fluid_source: true, water_block: true, ..BlockState::default() });
            }
        }
        let wet = lit(0.5);
        for (version, spared) in [(release(1, 21, 1), false), (release(1, 21, 2), true), (release(1, 21, 5), true), (release(1, 21, 6), false)] {
            let mut items = [dropped(7.9)];
            blast_items_in(&mut items, wet, &world, version);
            assert_eq!(items[0].velocity == Vec3::ZERO, spared, "{version}");
        }
    }

    fn team(id: u32, rule: CollisionRule) -> Option<MinecartTeam> {
        Some(MinecartTeam { id, rule, ..MinecartTeam::default() })
    }

    #[test]
    fn a_team_rule_selects_what_pushes_from_1_9() {
        let v = release(1, 20, 4);
        assert!(pushable_by(v, None, None));
        assert!(!pushable_by(v, team(1, CollisionRule::Never), None));
        assert!(!pushable_by(v, None, team(1, CollisionRule::Never)));
        assert!(!pushable_by(v, team(1, CollisionRule::PushOwnTeam), team(1, CollisionRule::Always)));
        assert!(pushable_by(v, team(1, CollisionRule::PushOwnTeam), team(2, CollisionRule::Always)));
        assert!(!pushable_by(v, team(1, CollisionRule::PushOtherTeams), team(2, CollisionRule::Always)));
        assert!(!pushable_by(v, team(1, CollisionRule::PushOtherTeams), None));
        assert!(pushable_by(v, team(1, CollisionRule::PushOtherTeams), team(1, CollisionRule::Always)));
        // (1.8 has no rule)
        assert!(pushable_by(release(1, 8, 8), team(1, CollisionRule::Never), None));
    }

    #[test]
    fn a_mob_whose_team_never_collides_neither_pushes_a_cart_nor_is_picked_up() {
        let world = straight(30);
        let mob = |x: f64, team: Option<MinecartTeam>| MinecartOther {
            position: Vec3::new(x, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.3, 0.0625, 0.2), Vec3::new(x + 0.3, 1.8625, 0.8)),
            pushable: true,
            living: true,
            team,
            ..MinecartOther::default()
        };
        // (beside a still cart: it pushes it unless its team never collides)
        for (team, pushed) in [(None, true), (team(1, CollisionRule::Never), false)] {
            let mut others = [mob(3.0, team)];
            let mut carts = [cart_at(2.5, 0.0)];
            tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
            assert_eq!(carts[0].velocity.x < 0.0, pushed, "{team:?}");
        }
        // (a fast cart on a team that pushes only other teams picks up no mob of its own team, and one of another)
        for (theirs, picked) in [(team(1, CollisionRule::Always), true), (team(2, CollisionRule::Always), false)] {
            let mut others = [mob(4.0, theirs)];
            let mut carts = [MinecartState { team: team(1, CollisionRule::PushOtherTeams), ..cart_at(3.0, 0.2) }];
            tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
            assert_eq!(others[0].rides.is_some(), picked, "{theirs:?}");
        }
    }

    #[test]
    fn a_bat_pushes_nothing_and_an_armor_stand_only_a_rideable_cart_at_its_place() {
        let world = straight(30);
        let stand = |x: f64| MinecartOther {
            position: Vec3::new(x, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(x - 0.25, 0.0625, 0.25), Vec3::new(x + 0.25, 2.0375, 0.75)),
            living: true,
            ..MinecartOther::default()
        };
        let bat = MinecartOther { bat: true, ..stand(3.0) };
        let mut others = [bat];
        let mut carts = [cart_at(2.5, 0.0)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(carts[0].velocity, Vec3::ZERO);
        // (0.3 off: 0.09 within the 0.2, the cart pushed away; 0.5 off is too far though the boxes meet)
        for (x, pushed) in [(2.8, true), (3.0, false)] {
            let mut others = [MinecartOther { armor_stand: true, ..stand(x) }];
            let mut carts = [cart_at(2.5, 0.0)];
            tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
            assert_eq!(carts[0].velocity.x < 0.0, pushed, "{x}");
        }
        // (a chest cart is no rideable cart)
        let mut others = [MinecartOther { armor_stand: true, ..stand(2.8) }];
        let mut carts = [MinecartState { variant: MinecartVariant::Chest, ..cart_at(2.5, 0.0) }];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 20, 4)));
        assert_eq!(carts[0].velocity, Vec3::ZERO);
        // (and before 1.10 a fast cart it pushes picks it up: AbstractMinecart.applyEntityCollision)
        let mut others = [MinecartOther { armor_stand: true, ..stand(3.1) }];
        let mut carts = [cart_at(3.0, 0.2)];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 8, 8)));
        assert_eq!(others[0].rides, Some(0));
    }

    // B290: each gate below on the two releases either side of its boundary, so that moving it one
    // release either way fails here.

    #[test]
    fn a_gateway_ticks_from_1_9_4_and_calls_attention_with_its_40_from_1_11_2() {
        let exit = (40, 3, -20);
        // (1.8.8 has no gateway; 1.9.4 TheEndGatewayBlockEntity.java:85-101 sends what is in it half a block up, 20 ticks of cooldown :125)
        let mut cart = in_gateway();
        let mut gateway = gateway_at((0, 0, 0), exit);
        tick_gateway(&mut cart, &mut [], &mut gateway, release(1, 9, 4));
        assert_eq!((cart.position, gateway.cooldown), (Vec3::new(40.5, 3.5, -19.5), 20));
        // (an empty gateway at the 2400th tick of its age: 1.10.2 TheEndGatewayBlockEntity.java:93-96 does nothing;
        // 1.11.2 TheEndGatewayBlockEntity.java:99 triggerCooldown, 40 ticks :130)
        for (version, cooldown) in [(release(1, 10, 2), 0), (release(1, 11, 2), 40)] {
            let mut empty = MinecartGateway { age: 2399, ..gateway_at((0, 0, 0), exit) };
            let mut away = resting(MinecartVariant::Rideable, Vec3::new(5.5, 0.0, 0.5));
            tick_gateway(&mut away, &mut [], &mut empty, version);
            assert_eq!((empty.age, empty.cooldown), (2400, cooldown), "{version}");
            let mut cart = in_gateway();
            let mut gateway = gateway_at((0, 0, 0), exit);
            tick_gateway(&mut cart, &mut [], &mut gateway, version);
            assert_eq!(gateway.cooldown, if version.at_least(1, 11, 0) { 40 } else { 20 }, "{version}");
        }
    }

    #[test]
    fn a_gateway_sends_its_root_vehicle_to_the_bottom_of_the_exit_from_1_16_and_nothing_of_its_own_from_1_21() {
        let exit = (40, 3, -20);
        // (1.16 TheEndGatewayBlockEntity.java:108, 184: the root vehicle, at the exit's bottom; 1.15.2 :167 half a block up)
        let mut cart = in_gateway();
        let mut gateway = gateway_at((0, 0, 0), exit);
        tick_gateway(&mut cart, &mut [], &mut gateway, release(1, 16, 0));
        assert_eq!(cart.position, Vec3::new(40.5, 3.0, -19.5));
        // A gateway over a cart's rider and not the cart: 1.20.6 TheEndGatewayBlockEntity.java:85-106 takes the rider's root
        // vehicle; from 1.21 its block entity only counts down (1.21 TheEndGatewayBlockEntity.java:78-91) and the cart's
        // own move does not reach the gateway's block (1.21 EndGatewayBlock.java:79-91 entityInside).
        let rider = MinecartOther {
            position: Vec3::new(0.5, 0.35, 0.5),
            bounds: Aabb::new(Vec3::new(0.2, 0.35, 0.2), Vec3::new(0.8, 2.15, 0.8)),
            player: true,
            living: true,
            passenger: true,
            rides: Some(0),
            ..MinecartOther::default()
        };
        for (version, sent) in [(release(1, 20, 6), true), (release(1, 21, 0), false)] {
            let mut cart = MinecartState { rider: Some(MinecartRider { player: true, living: true, ..MinecartRider::default() }), ..in_gateway() };
            let mut gateway = gateway_at((0, 1, 0), exit);
            tick_gateway(&mut cart, &mut [rider], &mut gateway, version);
            assert_eq!(cart.position == Vec3::new(40.5, 3.0, -19.5), sent, "{version}: {:?}", cart.position);
            assert_eq!(cart.position == Vec3::new(0.5, 0.0, 0.5), !sent, "{version}: {:?}", cart.position);
        }
    }

    #[test]
    fn a_rider_getting_off_pushes_its_cart_in_its_own_turn_from_1_9_4() {
        // (1.9.4 ServerGamePacketListenerImpl.java:165 ticks the player in the connection's tick, after the level)
        let world = straight(30);
        let mut others = [MinecartOther {
            position: Vec3::new(1.9, 0.0625, 0.5),
            bounds: Aabb::new(Vec3::new(1.6, 0.0625, 0.2), Vec3::new(2.2, 1.8625, 0.8)),
            pushable: true,
            living: true,
            player: true,
            passenger: true,
            rides: Some(0),
            gets_off: true,
            ..MinecartOther::default()
        }];
        let mut cart = cart_at(2.5, 0.0);
        cart.rider = Some(MinecartRider { player: true, living: true, ..MinecartRider::default() });
        let mut carts = [cart];
        tick_minecarts(&mut carts, &mut others, &world, old(release(1, 9, 4)));
        assert!(carts[0].rider.is_none());
        assert_eq!((others[0].rides, others[0].passenger, others[0].gets_off), (None, false, false));
        assert!(carts[0].velocity.x > 0.0);
    }

    #[test]
    fn the_tnt_explodes_rule_keeps_a_cart_from_priming_from_1_21_5_alone() {
        // (1.21.4 MinecartTNT.java:138-139 primes whatever the rules; 1.21.5 MinecartTNT.java:137-146 asks TNT_EXPLODES)
        let mut world = straight(8);
        activator(&mut world, (1, 0, 0), true);
        for (version, primes) in [(release(1, 21, 4), true), (release(1, 21, 5), false)] {
            let mut cart = tnt_on_rail(1.5, 0.0);
            let tick = tick_minecart(&mut cart, &world, old(version).in_level(0, false));
            assert_eq!((tick.primed, cart.fuse >= 0), (primes, primes), "{version}");
        }
    }

    #[test]
    fn a_player_riders_cart_cools_down_for_ten_ticks_in_1_21_and_300_before() {
        // A cart in a portal since its last tick, a player rider on it since. 1.20.6 Entity.java:2125-2152 counts it through
        // and cools it down by Entity.getDimensionChangingDelay's 300 (:2156-2158), then counts that down; 1.21.1
        // Entity.java:2233-2236 takes the player rider's 10 (Player.java:426-428), before which it counts down.
        let world = floor(8);
        for (version, cooldown) in [(release(1, 20, 6), 299), (release(1, 21, 0), 10)] {
            let mut rules = old(version);
            rules.nether_exit = false;
            let mut cart = MinecartState {
                rider: Some(MinecartRider { player: true, living: true, ..MinecartRider::default() }),
                portal: Some(MinecartPortal::Nether),
                inside_portal: true,
                ..cart_in_cell()
            };
            tick_minecart(&mut cart, &world, rules);
            assert_eq!((cart.removed, cart.portal_cooldown), (None, cooldown), "{version}");
        }
        // (and only from 1.21 does a ridden cart go in at all: 1.20.6 Entity.java:2599-2600 canChangeDimensions)
        let portal = portal_world(BlockEffect::NetherPortal);
        for (version, inside) in [(release(1, 20, 6), false), (release(1, 21, 0), true)] {
            let mut cart = MinecartState { rider: Some(MinecartRider::default()), ..cart_in_cell() };
            tick_minecart(&mut cart, &portal, old(version));
            assert_eq!(cart.inside_portal, inside, "{version}");
        }
    }

    /// The hurt and push of a blast of `radius` at `center` on a cart at `position` in the open, as vanilla's Explosion
    /// works them out on each side of its boundaries (see `blast_hit`): what `open_blast_from` is for any radius.
    fn blast_reference(version: MinecraftVersion, radius: f32, center: Vec3, position: Vec3, eye: f64, living: bool) -> (f32, Vec3) {
        let reach = f64::from(radius);
        let squared = position.distance_squared(center);
        let distance = if !version.at_least(1, 14, 0) {
            f64::from(squared.sqrt() as f32) / reach
        } else if !version.at_least(1, 17, 0) {
            f64::from(squared.sqrt() as f32 / radius)
        } else {
            squared.sqrt() / reach
        };
        let way = Vec3::new(position.x - center.x, position.y + eye - center.y, position.z - center.z);
        let length = if version.at_least(1, 17, 0) { way.length() } else { f64::from(way.length() as f32) };
        let impact = 1.0 - distance;
        let hurt = (impact * impact + impact) / 2.0;
        let damage = if version.at_least(1, 20, 3) {
            (hurt * 7.0 * reach + 1.0) as f32
        } else {
            (hurt * if version.at_least(1, 9, 0) { 7.0 } else { 8.0 } * reach + 1.0) as i32 as f32
        };
        let knockback = if !living && version.at_least(1, 9, 0) && !version.at_least(1, 11, 0) { 1.0 } else { impact };
        (damage, Vec3::new(way.x / length * knockback, way.y / length * knockback, way.z / length * knockback))
    }

    #[test]
    fn a_blasts_distance_is_a_float_quotient_from_1_14_4_and_its_hurt_7_a_block_from_1_9_4() {
        let world = floor(16);
        let eye = f64::from(HEIGHT * 0.85);
        // (1.13.2 Explosion.java:125 distanceTo over the radius in doubles; 1.14.4 Explosion.java:170 Mth.sqrt over the
        // float radius: a blast of power 3 has a radius of 6, which no power of two divides)
        let mut results = Vec::new();
        for version in [release(1, 13, 2), release(1, 14, 4)] {
            let mut carts = [floating(MinecartVariant::Rideable, 5.6), MinecartState { explosion_power_base: 3.0, ..lit(0.5) }];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            let (damage, push) = blast_reference(version, 6.0, carts[1].position, carts[0].position, eye, false);
            assert_eq!((carts[0].damage, carts[0].velocity), (damage * 10.0, push), "{version}");
            results.push(carts[0].velocity);
        }
        assert_ne!(results[0], results[1]);
        // (1.8.8 Explosion.java:130 8 a block of reach; 1.9.4 Explosion.java:132 7)
        // (the TNT cart falls in its own tick before it goes off before 1.10, which reads no NoGravity)
        let mut damages = Vec::new();
        for version in [release(1, 8, 8), release(1, 9, 4)] {
            let mut carts = [lit(0.5), floating(MinecartVariant::Rideable, 4.5)];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            let (want, _) = blast_reference(version, 8.0, carts[0].position, Vec3::new(4.5, 10.0, 0.5), eye, false);
            assert_eq!((carts[1].removed, carts[1].damage), (Some(MinecartRemoval::Destroyed), want * 10.0), "{version}");
            damages.push(want);
        }
        assert!(damages[0] > damages[1], "{damages:?}");
    }

    #[test]
    fn a_blast_from_a_carts_own_eye_hurts_it_from_1_21_9_and_misses_it_before() {
        // (1.21.8 ServerExplosion.java:158 skips an entity whose way out has no length; 1.21.9 ServerExplosion.java:172
        // Vec3.normalize gives it no push and the hurt)
        let world = floor(16);
        let eye = f64::from(HEIGHT * 0.85);
        let y = 10.0 - eye;
        assert_eq!(y + eye, 10.0);
        for (version, hurt) in [(release(1, 21, 8), false), (release(1, 21, 9), true)] {
            let mut carts = [MinecartState { position: Vec3::new(0.5, y, 0.5), ..floating(MinecartVariant::Rideable, 0.5) }, lit(0.5)];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(carts[0].velocity, Vec3::ZERO, "{version}");
            assert_eq!(carts[0].removed.is_some(), hurt, "{version}");
            assert_eq!(carts[0].damage > 0.0, hurt, "{version}");
        }
    }

    #[test]
    fn a_blast_samples_a_four_block_box_by_double_steps_from_1_18_2() {
        // Explosion.getSeenPercent over a ghast's 4-block box: the step 1/9 added up as a float reaches 10 points before
        // it passes 1 (1.18.1 Explosion.java:115-117), as a double 9 (1.18.2 Explosion.java:118-120). A wall hides part
        // of the box, so that the share seen is some k/1000 on 1.18.1 and some k/729 on 1.18.2.
        let mut world = floor(16);
        solid(&mut world, (2, 10, 0));
        solid(&mut world, (2, 11, 0));
        let ghast = MinecartOther {
            position: Vec3::new(5.0, 10.0, 0.5),
            bounds: Aabb::new(Vec3::new(3.0, 10.0, -1.5), Vec3::new(7.0, 14.0, 2.5)),
            eye_height: Some(2.6),
            living: true,
            ..MinecartOther::default()
        };
        let pushes = |version: MinecraftVersion| {
            let mut carts = [lit(0.5)];
            let mut others = [ghast];
            tick_minecarts(&mut carts, &mut others, &world, old(version));
            let center = carts[0].position;
            let way = Vec3::new(5.0 - center.x, 10.0 + f64::from(2.6_f32) - center.y, 0.5 - center.z);
            let distance = ghast.position.distance(center) / 8.0;
            let seen_by = |count: i32| (0..=count).any(|k| {
                let impact = (1.0 - distance) * f64::from(k as f32 / count as f32);
                others[0].velocity == Vec3::new(way.x / way.length() * impact, way.y / way.length() * impact, way.z / way.length() * impact)
            });
            assert!(others[0].velocity.x > 0.0, "{version}");
            (seen_by(1000), seen_by(729))
        };
        assert_eq!(pushes(release(1, 18, 1)), (true, false));
        assert_eq!(pushes(release(1, 18, 2)), (false, true));
    }

    #[test]
    fn a_hopper_cart_draws_from_half_a_block_up_until_1_13() {
        // An item over the cart's side, past its box and the 2/16 inset of Hopper.INSIDE: 1.12.2
        // HopperBlockEntity.java:383-384 draws from one box from half a block up; 1.13.2 Hopper.java:11-12 from INSIDE
        // (2/16 to 14/16 across, 11/16 up) and ABOVE (a block up).
        let world = straight(8);
        for (version, takes) in [(release(1, 12, 2), true), (release(1, 13, 2), false)] {
            let mut cart = hopper(1.5);
            let mut items = [MinecartItem::new(Aabb::new(Vec3::new(1.5 + 0.40, 0.8, 0.5), Vec3::new(1.5 + 0.45, 0.9, 0.6)), stone(1))];
            tick_hopper(&mut cart, &mut items, &world, version);
            assert_eq!(items[0].stack.count == 0, takes, "{version}");
        }
    }

    #[test]
    fn flowing_lava_pushes_a_cart_from_1_16() {
        // (1.15.2 Entity.java:1028-1043 updateInWaterStateAndDoFluidPushing measures the water alone; 1.16
        // Entity.java:1036-1044 the lava too, out of the water)
        let flowing = |flow: Vec3| {
            let mut world = floor(8);
            world.set_block((0, 0, 0), BlockState { fluid: FluidState::Lava, fluid_level: 4, fluid_flow: flow, ..BlockState::default() });
            world
        };
        let (still, moving) = (flowing(Vec3::ZERO), flowing(Vec3::new(1.0, 0.0, 0.0)));
        for (version, pushed) in [(release(1, 15, 2), false), (release(1, 16, 0), true)] {
            let after = |world: &WorldSnapshot| {
                let mut cart = cart_in_cell();
                tick_minecart(&mut cart, world, old(version));
                cart.velocity
            };
            assert_eq!(after(&still) != after(&moving), pushed, "{version}");
        }
    }

    #[test]
    fn a_furnace_cart_in_water_off_the_rail_goes_its_old_three_blocks_a_second_until_1_21_2() {
        // (1.21.1 MinecartFurnace.java:69-70 3.0 / 20.0 in water; 1.21.2 MinecartFurnace.java:65-66 three quarters of
        // OldMinecartBehavior.java:444-445's 0.2: not the same double)
        assert_ne!(3.0 / 20.0, 0.2 * 0.75);
        let world = WorldSnapshot::new();
        for (version, max) in [(release(1, 21, 1), 3.0 / 20.0), (release(1, 21, 2), 0.2 * 0.75)] {
            let mut cart = MinecartState { variant: MinecartVariant::Furnace, in_water: true, ..floating(MinecartVariant::Furnace, 0.5) };
            cart.velocity = Vec3::new(1.0, 0.0, 0.0);
            tick_minecart(&mut cart, &world, old(version));
            assert_eq!(cart.velocity.x, max * 0.95, "{version}");
        }
    }

    #[test]
    fn water_slows_a_cart_on_its_rail_from_1_17() {
        // (1.16.5 AbstractMinecart.java:623 and AbstractMinecartContainer.java:176-183 have no water; 1.17
        // AbstractMinecart.java:565 0.95, AbstractMinecartContainer.java:184-186 0.95F)
        let world = straight(30);
        for version in [release(1, 16, 5), release(1, 17, 0)] {
            let wet = version.at_least(1, 17, 0);
            let mut cart = MinecartState { in_water: true, ..cart_at(1.5, 0.1) };
            tick_minecart(&mut cart, &world, old(version));
            assert_eq!(cart.velocity.x, if wet { (0.1 * 0.96) * 0.949999988079071 } else { 0.1 * 0.96 }, "{version}");
            let mut chest = MinecartState { variant: MinecartVariant::Chest, in_water: true, ..cart_at(1.5, 0.1) };
            tick_minecart(&mut chest, &world, old(version));
            let mut keep = 0.98_f32 + 15.0 * 0.001_f32;
            if wet {
                keep *= 0.95_f32;
            }
            assert_eq!(chest.velocity.x, 0.1 * f64::from(keep), "{version}");
        }
    }

    #[test]
    fn a_furnace_push_is_a_double_and_slows_in_water_from_1_17() {
        // (1.16.5 MinecartFurnace.java:109 Mth.sqrt and no water; 1.17 MinecartFurnace.java:110 Math.sqrt, a tenth in water)
        let world = straight(30);
        let furnace = |push: Vec3, in_water: bool| MinecartState {
            variant: MinecartVariant::Furnace,
            fuel: 100,
            push,
            in_water,
            ..cart_at(1.5, 0.1)
        };
        let mut aslant = furnace(Vec3::new(1.0, 0.0, 1.0), false);
        tick_minecart(&mut aslant, &world, old(release(1, 17, 0)));
        assert_eq!(aslant.velocity.x, (0.1 * 0.8 + 1.0 / 2.0_f64.sqrt()) * 0.96);
        let mut wet = furnace(Vec3::new(1.0, 0.0, 0.0), true);
        tick_minecart(&mut wet, &world, old(release(1, 16, 5)));
        assert_eq!(wet.velocity.x, (0.1 * 0.8 + 1.0) * 0.96);
    }

    #[test]
    fn a_furnace_push_turns_with_the_motion_after_the_track_until_1_21_2() {
        let world = straight(30);
        let furnace = |velocity: f64, push: Vec3| MinecartState {
            variant: MinecartVariant::Furnace,
            fuel: 100,
            push,
            ..cart_at(1.5, velocity)
        };
        // (1.21.1 MinecartFurnace.java:77 moveAlongTrack turns a push across the rail along the motion, 1.15.2 :93-97)
        let mut across = furnace(0.1, Vec3::new(0.0, 0.0, 1.0));
        tick_minecart(&mut across, &world, old(release(1, 21, 1)));
        assert!(across.push.x > 0.0, "{:?}", across.push);
        // (1.21.2 MinecartFurnace.java:92 has no moveAlongTrack: a push against a faster motion keeps its way)
        let mut against = furnace(0.3, Vec3::new(-0.02, 0.0, 0.0));
        tick_minecart(&mut against, &world, old(release(1, 21, 2)));
        assert!(against.velocity.x > 0.0);
        assert!(against.push.x < 0.0, "{:?}", against.push);
    }

    #[test]
    fn a_bed_halves_a_tnt_carts_fall_from_1_12_2_and_the_power_is_a_double_from_1_21_5() {
        let mut world = floor(8);
        world.set_block((0, -1, 0), BlockState {
            shape: Some(BlockShape { effect: BlockEffect::Bed, ..BlockShape::full_block() }),
            ..BlockState::default()
        });
        let drop = |version: MinecraftVersion| {
            let mut cart = resting(MinecartVariant::Tnt, Vec3::new(0.5, 0.02, 0.5));
            cart.on_ground = false;
            cart.velocity = Vec3::new(0.0, -0.1, 0.0);
            cart.fall_distance = 4.0;
            tick_minecart(&mut cart, &world, old(version));
            cart.removed
        };
        // (1.11.2 BedBlock has no fallOn; 1.12.2 BedBlock.java:147-148 halves it)
        assert_eq!(drop(release(1, 11, 2)), Some(MinecartRemoval::Exploded));
        assert_eq!(drop(release(1, 12, 2)), None);
        // A landing of 3 with no move of its own: (fall / 10) squared as floats (1.21.4 MinecartTNT.java:113-116) or as
        // doubles (1.21.5 MinecartTNT.java:121-124), whose roots part in the power's float with the draw of seed 114.
        let floor = floor(8);
        let power = |version: MinecraftVersion| {
            let mut cart = MinecartState { random_seed: 114, fall_distance: 3.0, ..resting(MinecartVariant::Tnt, Vec3::new(0.5, 0.0, 0.5)) };
            let tick = tick_minecart(&mut cart, &floor, old(version));
            tick.explosions[0].power
        };
        let roll = EntityRandom(114).next_double();
        let as_floats = f64::from((3.0_f32 / 10.0) * (3.0_f32 / 10.0)).sqrt();
        let as_doubles = (0.3_f64 * 0.3).sqrt();
        assert_ne!((4.0 + roll * 1.5 * as_floats) as f32, (4.0 + roll * 1.5 * as_doubles) as f32);
        assert_eq!(power(release(1, 21, 4)), (4.0 + roll * 1.5 * as_floats) as f32);
        assert_eq!(power(release(1, 21, 5)), (4.0 + roll * 1.5 * as_doubles) as f32);
    }

    #[test]
    fn lava_halves_a_carts_fall_in_its_type() {
        // (1.21.4 AbstractMinecart.java:227 fallDistance *= 0.5F on the float; 1.21.5 AbstractMinecart.java:229 the double)
        let mut world = WorldSnapshot::new();
        lava(&mut world, (0, 0, 0));
        let halved = |version: MinecraftVersion, fall: f64| {
            let mut cart = MinecartState { no_gravity: true, fall_distance: fall, ..resting(MinecartVariant::Rideable, Vec3::new(0.5, 0.1, 0.5)) };
            cart.on_ground = false;
            tick_minecart(&mut cart, &world, old(version));
            cart.fall_distance
        };
        // (the least float halves to nothing as a float)
        let least = f64::from(f32::from_bits(1));
        assert_eq!(halved(release(1, 21, 4), least), 0.0);
        assert_eq!(halved(release(1, 21, 5), 0.1), 0.05);
    }

    #[test]
    fn a_cart_at_a_speed_of_exactly_a_tenth_picks_up_from_1_21_2() {
        // (1.21.1 AbstractMinecart.java:274 horizontalDistanceSqr > 0.01; 1.21.2 OldMinecartBehavior.java:404 >= 0.01)
        let (x, z) = (0.0001, 0.0999999499999875);
        assert_eq!(x * x + z * z, 0.01);
        let world = floor(8);
        for (version, picks) in [(release(1, 21, 1), false), (release(1, 21, 2), true)] {
            // (on the ground off a rail a cart's motion halves before its move)
            let mut carts = [MinecartState { velocity: Vec3::new(2.0 * x, 0.0, 2.0 * z), ..resting(MinecartVariant::Rideable, Vec3::new(0.5, 0.0, 0.5)) }];
            let mut others = [MinecartOther {
                position: Vec3::new(-0.25, 0.0, 0.5),
                bounds: Aabb::new(Vec3::new(-0.5, 0.0, 0.2), Vec3::new(0.0, 0.9, 0.8)),
                pushable: true,
                living: true,
                ..MinecartOther::default()
            }];
            tick_minecarts(&mut carts, &mut others, &world, old(version));
            assert_eq!(carts[0].velocity.horizontal_length_squared(), 0.01, "{version}");
            assert_eq!(others[0].rides == Some(0), picks, "{version}");
        }
    }

    #[test]
    fn a_normal_is_a_float_root_until_1_17_and_cut_off_at_1e_4_until_1_21_2() {
        // (1.16.5 Vec3.java:79 Mth.sqrt; 1.17 Vec3.java:54 Math.sqrt; 1.21.1 Vec3.java:65 1.0E-4; 1.21.2 Vec3.java:73 1.0E-5F)
        let aslant = Vec3::new(1.0, 0.0, 1.0);
        let float_root = f64::from(2.0_f64.sqrt() as f32);
        assert_eq!(legacy_normalized(aslant, release(1, 16, 5)).x, 1.0 / float_root);
        assert_eq!(legacy_normalized(aslant, release(1, 17, 0)).x, 1.0 / 2.0_f64.sqrt());
        let short = Vec3::new(5.0e-5, 0.0, 0.0);
        assert_eq!(legacy_normalized(short, release(1, 21, 1)), Vec3::ZERO);
        assert_eq!(legacy_normalized(short, release(1, 21, 2)), Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn a_clients_cart_lerps_five_steps_until_1_21_5_and_turns_in_floats_from_1_17() {
        // (1.21.4 OldMinecartBehavior.java:55 the packet's 3 and 2; 1.21.5 InterpolationHandler.java:10 3)
        assert_eq!(client_lerp_steps(release(1, 21, 4)), 5);
        assert_eq!(client_lerp_steps(release(1, 21, 5)), 3);
        // (1.16.5 AbstractMinecart.java:361 the double share of the turn; 1.17 AbstractMinecart.java:296 a float share)
        let turned = |version: MinecraftVersion| {
            let mut cart = MinecartClientLerp { y_rot: 0.84, target_y_rot: 101.566, x_rot: 0.3, target_x_rot: 40.1, steps: 3, ..MinecartClientLerp::default() };
            tick_client_linear(&mut cart, version);
            (cart.y_rot, cart.x_rot)
        };
        let turn = f64::from(101.566_f32) - f64::from(0.84_f32);
        let pitch = f64::from(40.1_f32) - f64::from(0.3_f32);
        let as_double = ((f64::from(0.84_f32) + turn / 3.0) as f32, (f64::from(0.3_f32) + pitch / 3.0) as f32);
        let as_float = (0.84_f32 + turn as f32 / 3.0, 0.3_f32 + pitch as f32 / 3.0);
        assert_ne!(as_double, as_float);
        assert_eq!(turned(release(1, 16, 5)), as_double);
        assert_eq!(turned(release(1, 17, 0)), as_float);
    }

    #[test]
    fn an_improved_carts_move_is_stepped_axis_by_axis_from_1_21_5_and_swept_whole_before() {
        // Off its rail, a cart sent along x and z at once. 1.21.4 Entity.applyEffectsFromBlocks() takes the line from
        // oldPosition to the position (1.21.4 Entity.java:788-801; the move records nothing, :1092
        // recordMovementThroughBlocks has no caller in the cart); 1.21.5 Entity.move records the move (Entity.java:715-721)
        // and checkInsideBlocks steps it an axis at a time, x before z as x is the longer: the box goes the whole way east
        // before it leaves the row it started in, and sweeps the cell south-east of it, which the whole line passes by.
        let mut world = floor(8);
        passable(&mut world, (2, 0, -1), BlockEffect::Cactus);
        for (version, pricked) in [(release(1, 21, 4), false), (release(1, 21, 5), true)] {
            let mut cart = resting(MinecartVariant::Rideable, Vec3::new(1.41, 0.0, 0.46));
            cart.velocity = Vec3::new(1.0, 0.0, 0.6);
            tick_minecart(&mut cart, &world, MinecartRules::new(version, true, Some(1000)).unwrap());
            assert!(cart.position.x > 1.8 && cart.position.z > 0.7, "{version}: {:?}", cart.position);
            assert_eq!(cart.damage > 0.0, pricked, "{version}: {}", cart.damage);
        }
    }

    #[test]
    fn a_carts_facing_turns_its_yaw_by_a_float_pi_before_1_9_and_by_a_float_quotient_from_it() {
        // AbstractMinecart.applyEntityCollision's facing (Mth.cos and sin of the yaw): 1.8.8 AbstractMinecart.java:708
        // `yaw * (float)Math.PI / 180.0F`, 1.9.4 AbstractMinecart.java:702 `yaw * (float)(Math.PI / 180.0)`. A cart pushed by
        // another along a way at 0.8 of its facing is shoved when the two disagree only in Mth's table, on the release whose
        // facing passes 0.8 alone.
        let (r1_8, r1_9) = (release(1, 8, 8), release(1, 9, 4));
        let facing = |yaw: f32, version: MinecraftVersion| {
            let radians = f64::from(if version.at_least(1, 9, 0) { yaw * 0.017453292_f32 } else { yaw * std::f32::consts::PI / 180.0 });
            legacy_normalized(Vec3::new(mth_cos(radians, version), 0.0, mth_sin(radians, version)), version)
        };
        let yaw = (0..36000).map(|k| k as f32 * 0.01).find(|yaw| facing(*yaw, r1_8) != facing(*yaw, r1_9)).expect("a yaw whose facings part");
        let (old_facing, new_facing) = (facing(yaw, r1_8), facing(yaw, r1_9));
        let this = Vec3::new(0.5, 0.0, 0.5);
        let threshold = f64::from(0.8_f32);
        let source = (0..200_000)
            .map(|step| {
                let angle = f64::from(yaw).to_radians() + 0.6 + f64::from(step) * 1.0e-6;
                Vec3::new(this.x + 0.9 * angle.cos(), 0.0, this.z + 0.9 * angle.sin())
            })
            .find(|source| {
                let way = legacy_normalized(*source - this, r1_8);
                (way.dot(old_facing).abs() < threshold) != (way.dot(new_facing).abs() < threshold)
            })
            .expect("a way between the two facings");
        let world = floor(8);
        // (the pushing cart faces across the way, so that the push back is the same in every run)
        let after = |version: MinecraftVersion, this_yaw: f32| {
            let mut carts = [
                MinecartState { y_rot: yaw + 90.0, ..resting(MinecartVariant::Rideable, source) },
                MinecartState { y_rot: this_yaw, ..resting(MinecartVariant::Rideable, this) },
            ];
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            carts[1].velocity
        };
        for version in [r1_8, r1_9] {
            let way = legacy_normalized(source - this, version);
            let shoved = way.dot(facing(yaw, version)).abs() >= threshold;
            // (a facing across the way is never shoved: the push alone)
            assert_eq!(after(version, yaw) != after(version, yaw + 90.0), shoved, "{version} at {yaw}");
        }
    }

    #[test]
    fn an_improved_carts_blocked_move_steps_its_axes_in_the_order_asked_from_1_21_9() {
        // Off its rail the cart is sent mostly along x, and a wall stops it after a tenth, so that what it moved is mostly
        // along z. Its recorded move is stepped axis by axis in the order of what it moved on 1.21.8 (Entity.java:772,
        // z before x: the box sweeps the cell west of it on the far side) and of what it asked on 1.21.9 (Entity.java:785,
        // x before z: it does not). The cactus there pricks it on 1.21.8 alone; the second applyEffectsFromBlocks, the
        // line from where the tick began (26.3 NewMinecartBehavior.java:280-281), passes it by on both.
        let mut world = floor(8);
        solid(&mut world, (2, 0, 0));
        solid(&mut world, (2, 0, 1));
        passable(&mut world, (0, 0, 1), BlockEffect::Cactus);
        let mut damages = Vec::new();
        for version in [release(1, 21, 8), release(1, 21, 9)] {
            let mut cart = resting(MinecartVariant::Rideable, Vec3::new(1.41, 0.0, 0.4));
            cart.velocity = Vec3::new(1.0, 0.0, 0.6);
            tick_minecart(&mut cart, &world, MinecartRules::new(version, true, Some(1000)).unwrap());
            assert!(cart.collided_horizontally, "{version}: {cart:?}");
            damages.push(cart.damage);
        }
        assert_eq!(damages, vec![10.0, 0.0]);
    }

    #[test]
    fn an_improved_cart_that_leaves_a_slope_sweeps_from_the_slope_s_start_on_1_21_4() {
        // A cart running down a slope in one tick, and off its foot onto the floor in the next iteration. 1.21.4
        // NewMinecartBehavior.moveAlongTrack sets the old position only on a rail (NewMinecartBehavior.java:236-238) and
        // calls applyEffectsFromBlocks() twice an iteration (:279-282), which sweeps oldPosition to the position
        // (Entity.java:787-803) and nothing Entity.move recorded (it records nothing in 1.21.4, Entity.java:668-786;
        // 1.21.5 Entity.java:715-721 does): every call meets the fire over the slope, two on the slope and two off it
        // from its top, each hurting the cart by 10 (BaseFireBlock.entityInside, VehicleEntity.hurtServer), 40 not past
        // the 40 that breaks it. The fire stands over the rail beside a plank, as FireBlock.canSurvive allows. A move
        // recorded on 1.21.4 would have the first call off the rail sweep from the foot of the slope alone, under the fire.
        let mut world = floor(4);
        rail(&mut world, (0, 0, 0), "ascending_east");
        solid(&mut world, (0, -1, 0));
        passable(&mut world, (0, 1, 0), BlockEffect::Fire);
        let run = |version: MinecraftVersion| {
            let mut cart = improved_cart(0.95, -1.4);
            cart.position.y = 0.95;
            tick_minecart(&mut cart, &world, MinecartRules::new(version, true, Some(1000)).unwrap());
            (cart.position, cart.damage)
        };
        let (at, damage_1_21_4) = run(release(1, 21, 4));
        assert!(at.x < 0.0, "{at:?}");
        assert_eq!(damage_1_21_4, 40.0);
    }

    #[test]
    fn a_gateway_sets_a_carts_rider_down_at_the_exit_on_1_21_1_too() {
        // 1.21.1 Entity.changeDimension unrides the passengers, sends each to the same place and has it
        // ride again (1.21.1 Entity.java:2588-2626, Entity.startRiding's boarding cooldown of 60); 1.21.2
        // teleportSameDimension keeps where each was from the cart (1.21.2 Entity.java:2759-2777, 2813-2819)
        let mut world = floor(8);
        passable(&mut world, (0, 0, 0), BlockEffect::None);
        let rider = || MinecartOther { passenger: true, rides: Some(0), ..mob_at(-0.55) };
        let cart = || MinecartState {
            velocity: Vec3::new(0.2, 0.0, 0.0),
            rider: Some(MinecartRider { living: true, ..MinecartRider::default() }),
            ..resting(MinecartVariant::Rideable, Vec3::new(-0.3, 0.0, 0.5))
        };
        for (version, offset, cooldown) in [(release(1, 21, 1), false, 59), (release(1, 21, 2), true, 0)] {
            let mut carts = [cart()];
            let mut others = [rider()];
            let mut gateways = [gateway_at((0, 0, 0), (40, 3, -20))];
            tick_minecarts_with(&mut carts, &mut others, &mut [], &mut [], &mut gateways, &world, old(version));
            let before = carts[0].position - others[0].position;
            tick_minecarts_with(&mut carts, &mut others, &mut [], &mut [], &mut gateways, &world, old(version));
            let exit = Vec3::new(40.5, 3.0, -19.5);
            let want = if offset { exit - before } else { exit };
            assert_eq!(others[0].position, want, "{version}");
            assert_eq!(others[0].boarding_cooldown, cooldown, "{version}");
        }
    }

    #[test]
    fn lava_draws_the_sound_of_its_hurt_from_1_17_itself() {
        // 1.17 Entity.lavaHurt plays the burn sound with a draw of the entity's random (Entity.java:489-496);
        // 1.16.5 only hurts (Entity.java:563-568)
        let mut world = floor(8);
        lava(&mut world, (0, 0, 0));
        let seed = |version| {
            let mut cart = MinecartState { random_seed: 42, ..cart_in_cell() };
            tick_minecart(&mut cart, &world, old(version));
            cart.random_seed
        };
        let mut random = EntityRandom(42);
        random.next_float();
        assert_eq!(seed(release(1, 17, 0)), random.0);
        assert_eq!(seed(release(1, 16, 5)), 42);
    }

    /// A TNT cart a creative player's blast lit (its fuse running, the source kept from 1.21.5) and an
    /// invulnerable cart beside it, which only a creative player's blast takes away.
    fn creative_lit_tnt_and_an_invulnerable_cart(at: Vec3) -> [MinecartState; 2] {
        let lit = MinecartState { fuse: 40, creative_ignition: true, ..resting(MinecartVariant::Tnt, at) };
        let near = MinecartState { invulnerable: true, ..resting(MinecartVariant::Rideable, at - Vec3::new(2.0, 0.0, 0.0)) };
        [lit, near]
    }

    #[test]
    fn a_creative_lit_tnt_cart_against_a_wall_keeps_its_source_from_1_21_9() {
        // MinecartTNT.tick's wall blast: 1.21.8 explode(var1) with no source (MinecartTNT.java:65), 1.21.9
        // explode(this.ignitionSource, var1) (MinecartTNT.java:65)
        let mut world = floor(8);
        for z in -1..=1 {
            solid(&mut world, (3, 0, z));
        }
        for (version, taken) in [(release(1, 21, 8), false), (release(1, 21, 9), true)] {
            let mut carts = creative_lit_tnt_and_an_invulnerable_cart(Vec3::new(2.3, 0.0, 0.5));
            carts[0].on_ground = false;
            carts[0].velocity = Vec3::new(0.3, 0.0, 0.3);
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(carts[0].removed, Some(MinecartRemoval::Exploded), "{version}");
            assert_eq!(carts[1].removed == Some(MinecartRemoval::Discarded), taken, "{version}: {:?}", carts[1]);
        }
    }

    #[test]
    fn a_creative_lit_tnt_cart_landing_keeps_its_source_from_1_21_9() {
        // MinecartTNT.causeFallDamage: 1.21.8 explode(power) with no source (MinecartTNT.java:125), 1.21.9
        // by the ignition source (MinecartTNT.java:121)
        let world = floor(8);
        for (version, taken) in [(release(1, 21, 8), false), (release(1, 21, 9), true)] {
            let mut carts = creative_lit_tnt_and_an_invulnerable_cart(Vec3::new(2.5, 0.02, 0.5));
            carts[0].on_ground = false;
            carts[0].velocity = Vec3::new(0.0, -0.1, 0.0);
            carts[0].fall_distance = 4.0;
            tick_minecarts(&mut carts, &mut [], &world, old(version));
            assert_eq!(carts[0].removed, Some(MinecartRemoval::Exploded), "{version}");
            assert_eq!(carts[1].removed == Some(MinecartRemoval::Discarded), taken, "{version}: {:?}", carts[1]);
        }
    }

    #[test]
    fn a_blast_pushes_by_what_is_not_finite_until_1_21_10() {
        // Entity.push(Vec3): 1.21.10 adds any push (Entity.java:1824-1826), 1.21.11 only a finite one
        // (Entity.java:1843-1856); a push that is not finite is what a blast at an entity's own
        // position gives (the direction divided by a distance of 0)
        let velocity = Vec3::new(0.1, 0.2, 0.3);
        for push in [Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(0.0, f64::INFINITY, 0.0)] {
            assert_eq!(blast_push(velocity, push, release(1, 21, 11)), velocity);
            let added = blast_push(velocity, push, release(1, 21, 10));
            assert!(!(added.x.is_finite() && added.y.is_finite()), "{added:?}");
        }
    }
}
