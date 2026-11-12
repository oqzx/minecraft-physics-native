//! The N-API face of `minecart`: a server tick of a cart or of a level's carts, and the two client
//! interpolations, which are entry points of their own and take no world.

use napi::{Error, Result};
use napi_derive::napi;

use crate::math::{Aabb, Vec3};
use crate::minecart::{
    client_lerp_steps, hurt_minecart, push_minecart, tick_client_effects, tick_client_linear,
    tick_client_weighted, tick_minecarts_among, tick_minecarts_with, MinecartClientEffects,
    MinecartClientLerp, MinecartClientSteps, MinecartContainer, MinecartDamage, MinecartExplosion,
    MinecartGateway, MinecartHurt, MinecartItem, MinecartOther, MinecartPortal, MinecartRemoval,
    MinecartRider, MinecartRules, MinecartSpawn, MinecartStack, MinecartState, MinecartStep,
    MinecartTick, MinecartVariant, CollisionRule, MinecartTeam,
};
use crate::napi_api::{
    check_move, validate_kept_box, validate_position, vehicle_release, NativeAabb,
    NativeBlockPosition, NativeVec3, NativeWorld,
};

/// What the level's enabled features select. `improvements` is required: a release alone does not
/// say (1.21.2+ has both paths). `maxMinecartSpeed` is the game rule, required with the
/// improvements. `minY` is the level's bottom (Level.getMinY), required of a cart so low that it
/// decides, from 1.18 where it is no longer 0. `tntExplodes` is the game rule of 1.21.5+.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeMinecartRules {
    pub improvements: bool,
    pub max_minecart_speed: Option<i32>,
    pub min_y: Option<i32>,
    pub tnt_explodes: Option<bool>,
    /// Whether a nether portal has a portal of the level's beyond it: only a player's makes one.
    pub nether_exit: Option<bool>,
    /// The spawnerBlocksEnabled game rule of 1.21.9+, which a spawner cart's spawner runs under.
    pub spawner_blocks_enabled: Option<bool>,
    /// Level.random's raw 48 bits, where the caller knows them: a TNT cart's blast then gives the
    /// exact blocks its rays break (`blown`), drawing from it in the tick's order.
    pub level_random: Option<f64>,
}

/// An end gateway's block entity: its block, the block it sends to (the exit portal under
/// ExactTeleport, else findExitPosition's; none where it has none), its teleportCooldown and age.
/// `undecided` comes back set where the level's random picked among entities on different carts.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartGateway {
    pub position: NativeBlockPosition,
    pub exit: Option<NativeBlockPosition>,
    pub cooldown: Option<i32>,
    pub age: Option<f64>,
    pub undecided: Option<bool>,
}

/// A stack in a hopper cart, an item entity or a container: its `item` is any name that is the
/// same for stacks that merge.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartStack {
    pub item: String,
    pub count: i32,
    pub max_stack: i32,
}

/// An item entity about the carts: what a hopper cart can take, and what a TNT cart's blast hurts
/// and pushes. `position` is Entity.position (its box's bottom centre without it), `health`
/// ItemEntity.health (5), `blastProof` whether a blast's hurt is kept off it (an invulnerable item,
/// a nether star, a stack resistant to explosions). `removed` comes back set where the hurt took
/// its health.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartItem {
    pub bounding_box: NativeAabb,
    pub stack: NativeMinecartStack,
    pub position: Option<NativeVec3>,
    pub velocity: Option<NativeVec3>,
    pub health: Option<i32>,
    pub blast_proof: Option<bool>,
    pub removed: Option<bool>,
}

/// A block's container a hopper cart can take from: the slots open to it, in order.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartContainer {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub slots: Vec<NativeMinecartStack>,
}

/// A hurt from outside a cart's tick: `kind` is `plain`, `fire` or `explosion`.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartHurt {
    pub kind: String,
    pub amount: f64,
    pub creative: Option<bool>,
    /// The burning arrow's velocity's lengthSqr, when one is the direct entity.
    pub burning_arrow: Option<f64>,
}

/// A TNT cart's Level.explode and the blocks its rays reach, air included, as the explosion's
/// toBlow has them: `certain` breaks under any of the level's draws, `possible` under some, and
/// `blown` is the exact list where the rules gave `levelRandom`, with `levelRandom` after it.
/// Breaking them is the server's, after the entities.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartExplosion {
    pub position: NativeVec3,
    pub power: f64,
    pub certain: Vec<NativeBlockPosition>,
    pub possible: Vec<NativeBlockPosition>,
    pub blown: Option<Vec<NativeBlockPosition>>,
    pub level_random: Option<f64>,
}

/// A cart's first passenger: which field counts depends on the release.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeMinecartRider {
    pub player: bool,
    pub living: Option<bool>,
    /// Its forward input, read before 1.14.
    pub forward: Option<f64>,
    /// Its float yaw in degrees, read before 1.14.
    pub yaw: Option<f64>,
    /// Its velocity on the server, read 1.14 - 1.21.1.
    pub velocity: Option<NativeVec3>,
    /// ServerPlayer.getLastClientMoveIntent, read from 1.21.2.
    pub move_intent: Option<NativeVec3>,
}

/// A cart as the server keeps it. `variant` is its entity name.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartState {
    pub variant: String,
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub y_rot: f64,
    pub x_rot: Option<f64>,
    pub flipped: bool,
    pub on_rails: Option<bool>,
    pub on_ground: bool,
    pub first_tick: Option<bool>,
    pub in_water: bool,
    pub collided_horizontally: Option<bool>,
    pub collided_vertically: Option<bool>,
    pub stuck_speed_multiplier: Option<NativeVec3>,
    pub no_gravity: Option<bool>,
    pub ultra_warm: Option<bool>,
    pub fall_distance: Option<f64>,
    pub bounding_box: Option<NativeAabb>,
    pub rider: Option<NativeMinecartRider>,
    pub fuel: Option<i32>,
    pub push: Option<NativeVec3>,
    pub comparator_signal: Option<i32>,
    pub has_loot_table: Option<bool>,
    /// Entity.hasCustomName, which before 1.20.3 keeps a creative hit from discarding the cart.
    pub custom_name: Option<bool>,
    pub tick_count: Option<i32>,
    pub hurt_time: Option<i32>,
    pub hurt_dir: Option<i32>,
    pub damage: Option<f64>,
    pub invulnerable: Option<bool>,
    /// MinecartTNT.fuse, -1 until it is primed.
    pub fuse: Option<i32>,
    /// From 1.21.5, MinecartTNT.ignitionSource has a creative player for its attacker: its blast is that player's.
    pub creative_ignition: Option<bool>,
    pub explosion_power_base: Option<f64>,
    pub explosion_speed_factor: Option<f64>,
    /// The raw 48 bits of Entity.random, which a TNT cart draws from: no client knows it, so it
    /// is given, and comes back as the tick left it.
    pub random_seed: Option<f64>,
    pub enabled: Option<bool>,
    /// A hopper cart's five slots, the empty ones as null: required of one, refused of any other.
    pub inventory: Option<Vec<Option<NativeMinecartStack>>>,
    pub last_activated: Option<i32>,
    /// A spawner cart's BaseSpawner: Delay (20 without it), MinSpawnDelay (200), MaxSpawnDelay (800)
    /// and RequiredPlayerRange (16).
    pub spawn_delay: Option<i32>,
    pub min_spawn_delay: Option<i32>,
    pub max_spawn_delay: Option<i32>,
    pub required_player_range: Option<i32>,
    /// `nether`, `end` or `gateway`, the portal it is in (Entity.portalProcess from 1.21).
    pub portal: Option<String>,
    /// The block of the gateway it went into, required of a `gateway` portal.
    pub portal_entry: Option<NativeBlockPosition>,
    pub inside_portal: Option<bool>,
    pub portal_time: Option<i32>,
    pub portal_cooldown: Option<i32>,
    /// Set by the tick: `destroyed`, `discarded`, `exploded`, `below_world`, `nether` or `end`.
    pub removed: Option<String>,
    /// The scoreboard team it is on, whose collision rule the pushes ask from 1.9.
    pub team: Option<NativeMinecartTeam>,
}

/// A scoreboard team: its name (the same name is the same team) and Team.CollisionRule, `always`,
/// `never`, `pushOtherTeams` or `pushOwnTeam`.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartTeam {
    pub name: String,
    pub collision_rule: String,
    /// PlayerTeam.allowFriendlyFire, true without it: from 26.3 a TNT cart's blast hurts its allies only with it.
    pub allow_friendly_fire: Option<bool>,
}

/// An entity near the carts that is not one of them.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartOther {
    pub position: NativeVec3,
    pub bounding_box: NativeAabb,
    pub velocity: Option<NativeVec3>,
    pub pushable: Option<bool>,
    pub solid: Option<bool>,
    pub player: Option<bool>,
    pub iron_golem: Option<bool>,
    pub living: Option<bool>,
    pub passenger: Option<bool>,
    pub has_passengers: Option<bool>,
    /// Set by the tick: the index of the cart that picked it up.
    pub rides: Option<u32>,
    /// The index among the scene's others of the entity it rides, which rides a cart (a jockey):
    /// from 1.9 nothing of that cart's pushes or collides with it (isPassengerOfSameVehicle).
    pub rides_other: Option<u32>,
    /// AbstractBoat: its tick from 1.9 pushes the carts it reaches.
    pub boat: Option<bool>,
    /// Where its own tick leaves it, from where it pushes: with `tickBoundingBox`, for what a cart ticking ahead of it finds elsewhere.
    pub tick_position: Option<NativeVec3>,
    pub tick_bounding_box: Option<NativeAabb>,
    /// How many of the scene's carts the level ticks ahead of it (its spawn order). Without it, all of them.
    pub ticks_after: Option<u32>,
    /// A player that leaves the cart it `rides` this tick: after that cart's tick, and before its own push from where it landed.
    pub gets_off: Option<bool>,
    /// Its eye height, from which a TNT cart's blast pushes it: without it 0.85 of its box's height.
    pub eye_height: Option<f64>,
    /// An armor stand: it pushes only the rideable carts at its own place, by its own way.
    pub armor_stand: Option<bool>,
    /// A bat, which pushes nothing.
    pub bat: Option<bool>,
    /// The scoreboard team it is on.
    pub team: Option<NativeMinecartTeam>,
    /// Entity.boardingCooldown, from 1.9: while above 0 no cart picks it up. Getting off a cart sets 60.
    pub boarding_cooldown: Option<i32>,
    /// Set by the tick: it left the level with the cart it rides, through a nether or end portal (from 1.21).
    pub removed: Option<bool>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeMinecartStep {
    pub position: NativeVec3,
    pub movement: NativeVec3,
    pub y_rot: f64,
    pub x_rot: f64,
    pub weight: f64,
}

/// What a tick left of one cart.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartTick {
    pub state: NativeMinecartState,
    /// The powered state of the activator rail it passed.
    pub activator: Option<bool>,
    /// What the improved path sends its clients.
    pub steps: Vec<NativeMinecartStep>,
    /// Its fuse was primed: the entity event the clients hear.
    pub primed: bool,
    pub explosions: Vec<NativeMinecartExplosion>,
    pub command_ran: bool,
    /// A spawner cart's turn that the level's random decides: `attempt`, with `spawnAt` its block
    /// (whether anything came, and the delay after, are the server's), or `delay`, a delay of -1
    /// drawn between its min and max.
    pub spawner: Option<String>,
    pub spawn_at: Option<NativeBlockPosition>,
}

#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct NativeMinecartScene {
    pub carts: Vec<NativeMinecartTick>,
    pub others: Vec<NativeMinecartOther>,
    pub items: Vec<NativeMinecartItem>,
    pub containers: Vec<NativeMinecartContainer>,
    pub gateways: Vec<NativeMinecartGateway>,
}

/// What a client's tick counts down of a cart.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeMinecartClientEffects {
    pub hurt_time: i32,
    pub damage: f64,
    pub fuse: i32,
}

/// A client's cart under the old path.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone, Copy)]
pub struct NativeMinecartLerp {
    pub position: NativeVec3,
    pub y_rot: f64,
    pub x_rot: f64,
    pub steps: i32,
    pub target: NativeVec3,
    pub target_y_rot: f64,
    pub target_x_rot: f64,
}

/// A client's cart under the improvements.
#[napi(object)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
#[derive(Clone)]
pub struct NativeMinecartClientSteps {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    pub y_rot: f64,
    pub x_rot: f64,
    pub delay: i32,
    pub pending: Vec<NativeMinecartStep>,
    pub current: Vec<NativeMinecartStep>,
    pub total_weight: f64,
    pub old: NativeMinecartStep,
}

fn finite(name: &str, vector: Vec3) -> Result<Vec3> {
    if vector.is_finite() {
        Ok(vector)
    } else {
        Err(Error::from_reason(format!("minecart: {name} must be finite")))
    }
}

fn variant_name(variant: MinecartVariant) -> &'static str {
    match variant {
        MinecartVariant::Rideable => "minecart",
        MinecartVariant::Chest => "chest_minecart",
        MinecartVariant::Hopper => "hopper_minecart",
        MinecartVariant::Furnace => "furnace_minecart",
        MinecartVariant::Tnt => "tnt_minecart",
        MinecartVariant::Spawner => "spawner_minecart",
        MinecartVariant::CommandBlock => "command_block_minecart",
    }
}

/// A release from 1.18 needs the level's bottom only of what is under the lowest bottom's reach of
/// it: above -64 no level of it can have let a cart go (the lowest vanilla bottom is -64 itself).
fn rules_of(version: &str, rules: NativeMinecartRules, carts: &[MinecartState]) -> Result<MinecartRules> {
    let release = vehicle_release(version, "minecart")?;
    let selected = MinecartRules::new(release, rules.improvements, rules.max_minecart_speed).map_err(Error::from_reason)?;
    let low = carts.iter().any(|cart| cart.position.y < -64.0);
    // (1.17.1 DimensionType.java:90; 1.18 DimensionType.java:89)
    if rules.min_y.is_none() && low && release.at_least(1, 18, 0) {
        return Err(Error::from_reason("minecart: a cart this low needs the level's minY"));
    }
    let selected = selected.in_level(rules.min_y.unwrap_or(0), rules.tnt_explodes.unwrap_or(true));
    Ok(MinecartRules {
        nether_exit: rules.nether_exit.unwrap_or(true),
        spawner_blocks: rules.spawner_blocks_enabled.unwrap_or(true),
        level_random: rules.level_random
            .map(|seed| whole_seed(seed).map_err(|_| Error::from_reason(format!("levelRandom is {seed}: it must be a whole number from 0 to 2^53 - 1"))))
            .transpose()?,
        ..selected
    })
}

/// The items of a call, as the numbers the engine compares.
#[derive(Default)]
struct ItemNames(Vec<String>, Vec<NativeMinecartTeam>);

impl ItemNames {
    /// A team as a number the engine compares, the same for the same name, and its rule.
    fn team(&mut self, value: &NativeMinecartTeam) -> Result<MinecartTeam> {
        let rule = match value.collision_rule.as_str() {
            "always" => CollisionRule::Always,
            "never" => CollisionRule::Never,
            "pushOtherTeams" => CollisionRule::PushOtherTeams,
            "pushOwnTeam" => CollisionRule::PushOwnTeam,
            other => return Err(Error::from_reason(format!("minecart: collision rule {other} is none of always, never, pushOtherTeams, pushOwnTeam"))),
        };
        let index = self.1.iter().position(|known| known.name == value.name).unwrap_or_else(|| {
            self.1.push(value.clone());
            self.1.len() - 1
        });
        Ok(MinecartTeam { id: index as u32 + 1, rule, allow_friendly_fire: value.allow_friendly_fire.unwrap_or(true) })
    }

    fn native_team(&self, team: Option<MinecartTeam>) -> Option<NativeMinecartTeam> {
        team.and_then(|team| self.1.get(team.id as usize - 1).cloned())
    }

    fn id(&mut self, name: &str) -> u32 {
        let index = self.0.iter().position(|known| known == name).unwrap_or_else(|| {
            self.0.push(name.to_owned());
            self.0.len() - 1
        });
        index as u32 + 1
    }

    fn name(&self, id: u32) -> String {
        self.0.get(id as usize - 1).cloned().unwrap_or_default()
    }

    fn stack(&mut self, value: &NativeMinecartStack) -> Result<MinecartStack> {
        if value.count < 0 || value.max_stack <= 0 {
            return Err(Error::from_reason(format!("minecart: a stack of {} has a count of {} of {}", value.item, value.count, value.max_stack)));
        }
        Ok(MinecartStack { item: self.id(&value.item), count: value.count, max_stack: value.max_stack })
    }

    fn native(&self, stack: MinecartStack) -> NativeMinecartStack {
        NativeMinecartStack { item: self.name(stack.item), count: stack.count, max_stack: stack.max_stack }
    }
}

fn portal_of(name: &str, entry: Option<&NativeBlockPosition>) -> Result<MinecartPortal> {
    match (name, entry) {
        ("nether", None) => Ok(MinecartPortal::Nether),
        ("end", None) => Ok(MinecartPortal::End),
        ("gateway", Some(at)) => Ok(MinecartPortal::Gateway((at.x, at.y, at.z))),
        ("gateway", None) => Err(Error::from_reason("minecart: a gateway portal needs its portalEntry")),
        ("nether" | "end", Some(_)) => Err(Error::from_reason(format!("minecart: a {name} portal has no portalEntry"))),
        _ => Err(Error::from_reason(format!("minecart: {name} is no portal"))),
    }
}

fn portal_name(portal: MinecartPortal) -> &'static str {
    match portal {
        MinecartPortal::Nether => "nether",
        MinecartPortal::End => "end",
        MinecartPortal::Gateway(_) => "gateway",
    }
}

fn block_position((x, y, z): (i32, i32, i32)) -> NativeBlockPosition {
    NativeBlockPosition { x, y, z }
}

fn removal_name(removal: MinecartRemoval) -> &'static str {
    match removal {
        MinecartRemoval::Destroyed => "destroyed",
        MinecartRemoval::Discarded => "discarded",
        MinecartRemoval::Exploded => "exploded",
        MinecartRemoval::BelowWorld => "below_world",
        MinecartRemoval::Portal(portal) => portal_name(portal),
    }
}

fn removal_of(name: &str) -> Result<MinecartRemoval> {
    match name {
        "destroyed" => Ok(MinecartRemoval::Destroyed),
        "discarded" => Ok(MinecartRemoval::Discarded),
        "exploded" => Ok(MinecartRemoval::Exploded),
        "below_world" => Ok(MinecartRemoval::BelowWorld),
        portal => portal_of(portal, None).map(MinecartRemoval::Portal),
    }
}

fn vector_or_zero(vector: Option<NativeVec3>, name: &str) -> Result<Vec3> {
    finite(name, vector.map_or(Vec3::ZERO, Vec3::from))
}

fn rider_of(value: NativeMinecartRider) -> Result<MinecartRider> {
    Ok(MinecartRider {
        player: value.player,
        living: value.living.unwrap_or(value.player),
        forward: value.forward.unwrap_or(0.0) as f32,
        yaw: value.yaw.unwrap_or(0.0) as f32,
        velocity: vector_or_zero(value.velocity, "rider.velocity")?,
        move_intent: vector_or_zero(value.move_intent, "rider.moveIntent")?,
    })
}

/// Entity.random's seed, which a double carries exactly: a cast would make a fraction or a negative into another seed.
fn whole_seed(seed: f64) -> Result<u64> {
    const MAX: f64 = 9_007_199_254_740_991.0;
    if seed.fract() != 0.0 || !(0.0..=MAX).contains(&seed) {
        return Err(Error::from_reason(format!(
            "randomSeed is {seed}: it must be a whole number from 0 to {MAX}"
        )));
    }
    Ok(seed as u64)
}

fn state_of(value: NativeMinecartState, names: &mut ItemNames) -> Result<MinecartState> {
    let variant = MinecartVariant::from_entity_name(&value.variant)
        .ok_or_else(|| Error::from_reason(format!("minecart: {} is no minecart", value.variant)))?;
    let holds_items = matches!(variant, MinecartVariant::Chest | MinecartVariant::Hopper);
    if holds_items && value.comparator_signal.is_none() {
        return Err(Error::from_reason(format!("minecart: {} needs its comparatorSignal", value.variant)));
    }
    if variant == MinecartVariant::Furnace && (value.fuel.is_none() || value.push.is_none()) {
        return Err(Error::from_reason("minecart: furnace_minecart needs its fuel and push"));
    }
    let bounding_box = value.bounding_box.map(Aabb::from);
    validate_position("minecart", value.position.into())?;
    validate_kept_box("minecart", bounding_box)?;
    if variant == MinecartVariant::Tnt && value.random_seed.is_none() {
        return Err(Error::from_reason("minecart: tnt_minecart needs its randomSeed"));
    }
    if variant == MinecartVariant::Hopper && value.inventory.is_none() {
        return Err(Error::from_reason("minecart: hopper_minecart needs its inventory"));
    }
    // (a tick reads a hopper cart's alone, and the result would drop any other's without a word:
    // a chest cart's slots reach it as its comparatorSignal)
    if variant != MinecartVariant::Hopper && value.inventory.is_some() {
        return Err(Error::from_reason(format!("minecart: {} has no inventory here, only a hopper_minecart", value.variant)));
    }
    let mut inventory = [MinecartStack::default(); 5];
    let slots = value.inventory.unwrap_or_default();
    if slots.len() > inventory.len() {
        return Err(Error::from_reason("minecart: a hopper cart has five slots"));
    }
    for (slot, stack) in inventory.iter_mut().zip(&slots) {
        if let Some(stack) = stack {
            *slot = names.stack(stack)?;
        }
    }
    let defaults = MinecartState::default();
    Ok(MinecartState {
        variant,
        position: finite("position", value.position.into())?,
        velocity: finite("velocity", value.velocity.into())?,
        y_rot: value.y_rot as f32,
        x_rot: value.x_rot.unwrap_or(0.0) as f32,
        flipped: value.flipped,
        on_rails: value.on_rails.unwrap_or(false),
        on_ground: value.on_ground,
        first_tick: value.first_tick.unwrap_or(false),
        in_water: value.in_water,
        collided_horizontally: value.collided_horizontally.unwrap_or(false),
        collided_vertically: value.collided_vertically.unwrap_or(false),
        stuck_speed_multiplier: vector_or_zero(value.stuck_speed_multiplier, "stuckSpeedMultiplier")?,
        no_gravity: value.no_gravity.unwrap_or(false),
        ultra_warm: value.ultra_warm.unwrap_or(false),
        fall_distance: value.fall_distance.unwrap_or(0.0),
        bounding_box,
        rider: value.rider.map(rider_of).transpose()?,
        fuel: value.fuel.unwrap_or(0),
        push: vector_or_zero(value.push, "push")?,
        comparator_signal: value.comparator_signal.unwrap_or(0),
        has_loot_table: value.has_loot_table.unwrap_or(false),
        custom_name: value.custom_name.unwrap_or(false),
        move_out_of_range: false,
        tick_count: value.tick_count.unwrap_or(defaults.tick_count),
        hurt_time: value.hurt_time.unwrap_or(defaults.hurt_time),
        hurt_dir: value.hurt_dir.unwrap_or(defaults.hurt_dir),
        damage: value.damage.map_or(defaults.damage, |damage| damage as f32),
        invulnerable: value.invulnerable.unwrap_or(false),
        fuse: value.fuse.unwrap_or(defaults.fuse),
        creative_ignition: value.creative_ignition.unwrap_or(false),
        explosion_power_base: value.explosion_power_base.map_or(defaults.explosion_power_base, |power| power as f32),
        explosion_speed_factor: value.explosion_speed_factor.map_or(defaults.explosion_speed_factor, |factor| factor as f32),
        random_seed: value.random_seed.map_or(Ok(0), whole_seed)?,
        enabled: value.enabled.unwrap_or(defaults.enabled),
        inventory,
        last_activated: value.last_activated.unwrap_or(defaults.last_activated),
        spawn_delay: value.spawn_delay.unwrap_or(defaults.spawn_delay),
        min_spawn_delay: value.min_spawn_delay.unwrap_or(defaults.min_spawn_delay),
        max_spawn_delay: value.max_spawn_delay.unwrap_or(defaults.max_spawn_delay),
        required_player_range: value.required_player_range.unwrap_or(defaults.required_player_range),
        portal: value.portal.as_deref().map(|portal| portal_of(portal, value.portal_entry.as_ref())).transpose()?,
        inside_portal: value.inside_portal.unwrap_or(false),
        portal_time: value.portal_time.unwrap_or(0),
        portal_cooldown: value.portal_cooldown.unwrap_or(0),
        removed: value.removed.as_deref().map(removal_of).transpose()?,
        team: value.team.as_ref().map(|team| names.team(team)).transpose()?,
    })
}



fn native_rider(value: MinecartRider) -> NativeMinecartRider {
    NativeMinecartRider {
        player: value.player,
        living: Some(value.living),
        forward: Some(f64::from(value.forward)),
        yaw: Some(f64::from(value.yaw)),
        velocity: Some(value.velocity.into()),
        move_intent: Some(value.move_intent.into()),
    }
}

fn native_state(value: MinecartState, names: &ItemNames) -> NativeMinecartState {
    let furnace = value.variant == MinecartVariant::Furnace;
    let holds_items = matches!(value.variant, MinecartVariant::Chest | MinecartVariant::Hopper);
    let hopper = value.variant == MinecartVariant::Hopper;
    let tnt = value.variant == MinecartVariant::Tnt;
    let spawner = value.variant == MinecartVariant::Spawner;
    NativeMinecartState {
        variant: variant_name(value.variant).to_owned(),
        position: value.position.into(),
        velocity: value.velocity.into(),
        y_rot: f64::from(value.y_rot),
        x_rot: Some(f64::from(value.x_rot)),
        flipped: value.flipped,
        on_rails: Some(value.on_rails),
        on_ground: value.on_ground,
        first_tick: Some(value.first_tick),
        in_water: value.in_water,
        collided_horizontally: Some(value.collided_horizontally),
        collided_vertically: Some(value.collided_vertically),
        stuck_speed_multiplier: Some(value.stuck_speed_multiplier.into()),
        no_gravity: Some(value.no_gravity),
        ultra_warm: Some(value.ultra_warm),
        fall_distance: Some(value.fall_distance),
        bounding_box: value.bounding_box.map(Into::into),
        rider: value.rider.map(native_rider),
        fuel: furnace.then_some(value.fuel),
        push: furnace.then(|| value.push.into()),
        comparator_signal: holds_items.then_some(value.comparator_signal),
        has_loot_table: Some(value.has_loot_table),
        custom_name: Some(value.custom_name),
        tick_count: Some(value.tick_count),
        hurt_time: Some(value.hurt_time),
        hurt_dir: Some(value.hurt_dir),
        damage: Some(f64::from(value.damage)),
        invulnerable: Some(value.invulnerable),
        fuse: tnt.then_some(value.fuse),
        creative_ignition: tnt.then_some(value.creative_ignition),
        explosion_power_base: tnt.then_some(f64::from(value.explosion_power_base)),
        explosion_speed_factor: tnt.then_some(f64::from(value.explosion_speed_factor)),
        random_seed: tnt.then_some(value.random_seed as f64),
        enabled: hopper.then_some(value.enabled),
        inventory: hopper.then(|| {
            value.inventory.iter()
                .map(|&slot| (slot.count > 0).then(|| names.native(slot)))
                .collect()
        }),
        last_activated: Some(value.last_activated),
        spawn_delay: spawner.then_some(value.spawn_delay),
        min_spawn_delay: spawner.then_some(value.min_spawn_delay),
        max_spawn_delay: spawner.then_some(value.max_spawn_delay),
        required_player_range: spawner.then_some(value.required_player_range),
        portal: value.portal.map(|portal| portal_name(portal).to_owned()),
        portal_entry: match value.portal {
            Some(MinecartPortal::Gateway(entry)) => Some(block_position(entry)),
            _ => None,
        },
        inside_portal: Some(value.inside_portal),
        portal_time: Some(value.portal_time),
        portal_cooldown: Some(value.portal_cooldown),
        removed: value.removed.map(|removal| removal_name(removal).to_owned()),
        team: names.native_team(value.team),
    }
}

fn other_of(value: NativeMinecartOther, names: &mut ItemNames) -> Result<MinecartOther> {
    let tick_place = match (value.tick_position, value.tick_bounding_box) {
        (Some(position), Some(bounds)) => Some((finite("other.tickPosition", position.into())?, bounds.into())),
        (None, None) => None,
        _ => return Err(Error::from_reason("minecart: other.tickPosition and other.tickBoundingBox go together")),
    };
    Ok(MinecartOther {
        position: finite("other.position", value.position.into())?,
        bounds: value.bounding_box.into(),
        velocity: vector_or_zero(value.velocity, "other.velocity")?,
        pushable: value.pushable.unwrap_or(false),
        solid: value.solid.unwrap_or(false),
        player: value.player.unwrap_or(false),
        iron_golem: value.iron_golem.unwrap_or(false),
        living: value.living.unwrap_or(false),
        passenger: value.passenger.unwrap_or(false),
        has_passengers: value.has_passengers.unwrap_or(false),
        rides: value.rides.map(|cart| cart as usize),
        rides_other: value.rides_other.map(|other| other as usize),
        boat: value.boat.unwrap_or(false),
        tick_place,
        ticks_after: value.ticks_after.map(|carts| carts as usize),
        gets_off: value.gets_off.unwrap_or(false),
        eye_height: value.eye_height.map(|eye| eye as f32),
        armor_stand: value.armor_stand.unwrap_or(false),
        bat: value.bat.unwrap_or(false),
        team: value.team.as_ref().map(|team| names.team(team)).transpose()?,
        boarding_cooldown: value.boarding_cooldown.unwrap_or(0),
        removed: value.removed.unwrap_or(false),
    })
}

fn native_other(value: MinecartOther, names: &ItemNames) -> NativeMinecartOther {
    NativeMinecartOther {
        position: value.position.into(),
        bounding_box: value.bounds.into(),
        velocity: Some(value.velocity.into()),
        pushable: Some(value.pushable),
        solid: Some(value.solid),
        player: Some(value.player),
        iron_golem: Some(value.iron_golem),
        living: Some(value.living),
        passenger: Some(value.passenger),
        has_passengers: Some(value.has_passengers),
        rides: value.rides.map(|cart| cart as u32),
        rides_other: value.rides_other.map(|other| other as u32),
        boat: Some(value.boat),
        tick_position: value.tick_place.map(|(position, _)| position.into()),
        tick_bounding_box: value.tick_place.map(|(_, bounds)| bounds.into()),
        ticks_after: value.ticks_after.map(|carts| carts as u32),
        gets_off: Some(value.gets_off),
        eye_height: value.eye_height.map(f64::from),
        armor_stand: Some(value.armor_stand),
        bat: Some(value.bat),
        team: names.native_team(value.team),
        boarding_cooldown: Some(value.boarding_cooldown),
        removed: Some(value.removed),
    }
}

fn native_step(value: MinecartStep) -> NativeMinecartStep {
    NativeMinecartStep {
        position: value.position.into(),
        movement: value.movement.into(),
        y_rot: f64::from(value.y_rot),
        x_rot: f64::from(value.x_rot),
        weight: f64::from(value.weight),
    }
}

fn step_of(value: NativeMinecartStep) -> MinecartStep {
    MinecartStep {
        position: value.position.into(),
        movement: value.movement.into(),
        y_rot: value.y_rot as f32,
        x_rot: value.x_rot as f32,
        weight: value.weight as f32,
    }
}

fn native_explosion(value: MinecartExplosion) -> NativeMinecartExplosion {
    let blocks = |list: Vec<(i32, i32, i32)>| list.into_iter().map(block_position).collect::<Vec<_>>();
    NativeMinecartExplosion {
        position: value.position.into(),
        power: f64::from(value.power),
        certain: blocks(value.certain),
        possible: blocks(value.possible),
        blown: value.blown.map(blocks),
        level_random: value.level_random.map(|random| random as f64),
    }
}

fn item_of(item: &NativeMinecartItem, names: &mut ItemNames) -> Result<MinecartItem> {
    let mut out = MinecartItem::new(item.bounding_box.clone().into(), names.stack(&item.stack)?);
    if let Some(position) = item.position {
        out.position = position.into();
        validate_position("minecart item position", out.position)?;
    }
    if let Some(velocity) = item.velocity {
        out.velocity = velocity.into();
        validate_position("minecart item velocity", out.velocity)?;
    }
    out.health = item.health.unwrap_or(5);
    out.blast_proof = item.blast_proof.unwrap_or(false);
    out.removed = item.removed.unwrap_or(false);
    Ok(out)
}

fn native_item(item: MinecartItem, names: &ItemNames) -> NativeMinecartItem {
    NativeMinecartItem {
        bounding_box: item.bounds.into(),
        stack: names.native(item.stack),
        position: Some(item.position.into()),
        velocity: Some(item.velocity.into()),
        health: Some(item.health),
        blast_proof: Some(item.blast_proof),
        removed: Some(item.removed),
    }
}

fn native_tick(state: MinecartState, tick: MinecartTick, names: &ItemNames) -> NativeMinecartTick {
    NativeMinecartTick {
        state: native_state(state, names),
        activator: tick.activator,
        steps: tick.steps.into_iter().map(native_step).collect(),
        primed: tick.primed,
        explosions: tick.explosions.into_iter().map(native_explosion).collect(),
        command_ran: tick.command_ran,
        spawner: tick.spawner.map(|turn| match turn {
            MinecartSpawn::Attempt(_) => "attempt".to_owned(),
            MinecartSpawn::DrewDelay => "delay".to_owned(),
        }),
        spawn_at: match tick.spawner {
            Some(MinecartSpawn::Attempt(at)) => Some(block_position(at)),
            _ => None,
        },
    }
}

/// A server tick of one cart.
#[napi]
pub fn simulate_versioned_minecart_tick(
    version: String,
    world: &NativeWorld,
    state: NativeMinecartState,
    rules: NativeMinecartRules,
) -> Result<NativeMinecartTick> {
    let mut names = ItemNames::default();
    let mut state = state_of(state, &mut names)?;
    let rules = rules_of(&version, rules, std::slice::from_ref(&state))?;
    let mut ticks = tick_minecarts_among(std::slice::from_mut(&mut state), &mut [], &mut [], &mut [], world.snapshot(), rules);
    check_move("simulateVersionedMinecartTick", state.move_out_of_range)?;
    Ok(native_tick(state, ticks.pop().unwrap_or_default(), &names))
}

/// A server tick of a level's carts in its tick order, among the other entities near them.
#[napi]
pub fn simulate_versioned_minecart_scene(
    version: String,
    world: &NativeWorld,
    carts: Vec<NativeMinecartState>,
    others: Vec<NativeMinecartOther>,
    rules: NativeMinecartRules,
    items: Option<Vec<NativeMinecartItem>>,
    containers: Option<Vec<NativeMinecartContainer>>,
    gateways: Option<Vec<NativeMinecartGateway>>,
) -> Result<NativeMinecartScene> {
    let mut names = ItemNames::default();
    let mut carts = carts.into_iter().map(|cart| state_of(cart, &mut names)).collect::<Result<Vec<_>>>()?;
    let rules = rules_of(&version, rules, &carts)?;
    let mut others = others.into_iter().map(|other| other_of(other, &mut names)).collect::<Result<Vec<_>>>()?;
    let mut items = items.unwrap_or_default().iter()
        .map(|item| item_of(item, &mut names))
        .collect::<Result<Vec<_>>>()?;
    let mut containers = containers.unwrap_or_default().iter()
        .map(|container| {
            let slots = container.slots.iter().map(|stack| names.stack(stack)).collect::<Result<Vec<_>>>()?;
            Ok(MinecartContainer { position: (container.x, container.y, container.z), slots })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut gateways = gateways.unwrap_or_default().iter()
        .map(|gateway| {
            let age = gateway.age.unwrap_or(0.0);
            if age.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&age) {
                return Err(Error::from_reason(format!("minecart: a gateway's age is {age}: a whole number from 0")));
            }
            Ok(MinecartGateway {
                position: (gateway.position.x, gateway.position.y, gateway.position.z),
                exit: gateway.exit.as_ref().map(|at| (at.x, at.y, at.z)),
                cooldown: gateway.cooldown.unwrap_or(0),
                age: age as i64,
                undecided: gateway.undecided.unwrap_or(false),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let ticks = tick_minecarts_with(&mut carts, &mut others, &mut items, &mut containers, &mut gateways, world.snapshot(), rules);
    check_move("simulateVersionedMinecartScene", carts.iter().any(|cart| cart.move_out_of_range))?;
    Ok(NativeMinecartScene {
        carts: carts.into_iter().zip(ticks).map(|(cart, tick)| native_tick(cart, tick, &names)).collect(),
        others: others.into_iter().map(|other| native_other(other, &names)).collect(),
        items: items.into_iter()
            .map(|item| native_item(item, &names))
            .collect(),
        containers: containers.into_iter()
            .map(|container| NativeMinecartContainer {
                x: container.position.0,
                y: container.position.1,
                z: container.position.2,
                slots: container.slots.into_iter().map(|stack| names.native(stack)).collect(),
            })
            .collect(),
        gateways: gateways.into_iter()
            .map(|gateway| NativeMinecartGateway {
                position: block_position(gateway.position),
                exit: gateway.exit.map(block_position),
                cooldown: Some(gateway.cooldown),
                age: Some(gateway.age as f64),
                undecided: Some(gateway.undecided),
            })
            .collect(),
    })
}

/// VehicleEntity.hurtServer of what comes from outside the cart's tick.
#[napi]
pub fn simulate_versioned_minecart_hurt(
    version: String,
    world: &NativeWorld,
    cart: NativeMinecartState,
    hurt: NativeMinecartHurt,
    rules: NativeMinecartRules,
) -> Result<NativeMinecartTick> {
    let mut names = ItemNames::default();
    let mut cart = state_of(cart, &mut names)?;
    let rules = rules_of(&version, rules, std::slice::from_ref(&cart))?;
    let kind = match hurt.kind.as_str() {
        "plain" => MinecartDamage::Plain,
        "fire" => MinecartDamage::Fire,
        "explosion" => MinecartDamage::Explosion,
        other => return Err(Error::from_reason(format!("minecart: {other} is no kind of hurt"))),
    };
    let hurt = MinecartHurt {
        kind,
        amount: hurt.amount as f32,
        creative: hurt.creative.unwrap_or(false),
        burning_arrow: hurt.burning_arrow,
    };
    let tick = hurt_minecart(&mut cart, &mut [], hurt, world.snapshot(), rules);
    Ok(native_tick(cart, tick, &names))
}

/// One client tick of what a client counts down: never what the server decides.
#[napi]
pub fn simulate_minecart_client_effects(state: NativeMinecartClientEffects) -> NativeMinecartClientEffects {
    let mut cart = MinecartClientEffects { hurt_time: state.hurt_time, damage: state.damage as f32, fuse: state.fuse };
    tick_client_effects(&mut cart);
    NativeMinecartClientEffects { hurt_time: cart.hurt_time, damage: f64::from(cart.damage), fuse: cart.fuse }
}

/// AbstractMinecart.push(Entity): the cart an entity's own tick pushes, and what that entity takes.
#[napi]
pub fn simulate_versioned_minecart_push(
    version: String,
    cart: NativeMinecartState,
    other: NativeMinecartOther,
) -> Result<NativeMinecartScene> {
    let release = vehicle_release(&version, "minecart")?;
    let mut names = ItemNames::default();
    let mut cart = state_of(cart, &mut names)?;
    let mut other = other_of(other, &mut names)?;
    push_minecart(&mut cart, &mut other, release);
    Ok(NativeMinecartScene {
        carts: vec![native_tick(cart, MinecartTick::default(), &names)],
        others: vec![native_other(other, &names)],
        items: Vec::new(),
        containers: Vec::new(),
        gateways: Vec::new(),
    })
}

/// The steps a client's interpolation of a cart starts with under the old path.
#[napi]
pub fn minecart_client_lerp_steps(version: String) -> Result<i32> {
    Ok(client_lerp_steps(vehicle_release(&version, "minecart")?))
}

/// One client tick of the old path's interpolation.
#[napi]
pub fn simulate_minecart_client_lerp(version: String, state: NativeMinecartLerp) -> Result<NativeMinecartLerp> {
    let release = vehicle_release(&version, "minecart")?;
    let mut cart = MinecartClientLerp {
        position: finite("position", state.position.into())?,
        y_rot: state.y_rot as f32,
        x_rot: state.x_rot as f32,
        steps: state.steps,
        target: finite("target", state.target.into())?,
        target_y_rot: state.target_y_rot as f32,
        target_x_rot: state.target_x_rot as f32,
    };
    tick_client_linear(&mut cart, release);
    Ok(NativeMinecartLerp {
        position: cart.position.into(),
        y_rot: f64::from(cart.y_rot),
        x_rot: f64::from(cart.x_rot),
        steps: cart.steps,
        target: cart.target.into(),
        target_y_rot: f64::from(cart.target_y_rot),
        target_x_rot: f64::from(cart.target_x_rot),
    })
}

/// One client tick of the improvements' interpolation.
#[napi]
pub fn simulate_minecart_client_steps(state: NativeMinecartClientSteps) -> Result<NativeMinecartClientSteps> {
    let mut cart = MinecartClientSteps {
        position: finite("position", state.position.into())?,
        velocity: finite("velocity", state.velocity.into())?,
        y_rot: state.y_rot as f32,
        x_rot: state.x_rot as f32,
        delay: state.delay,
        pending: state.pending.into_iter().map(step_of).collect(),
        current: state.current.into_iter().map(step_of).collect(),
        total_weight: state.total_weight,
        old: step_of(state.old),
    };
    tick_client_weighted(&mut cart);
    Ok(NativeMinecartClientSteps {
        position: cart.position.into(),
        velocity: cart.velocity.into(),
        y_rot: f64::from(cart.y_rot),
        x_rot: f64::from(cart.x_rot),
        delay: cart.delay,
        pending: cart.pending.into_iter().map(native_step).collect(),
        current: cart.current.into_iter().map(native_step).collect(),
        total_weight: cart.total_weight,
        old: native_step(cart.old),
    })
}

#[cfg(test)]
#[path = "napi_minecart_gate_tests.rs"]
mod gate_fixtures;
