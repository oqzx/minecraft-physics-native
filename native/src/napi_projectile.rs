//! Projectiles among entities, for the JavaScript side.

use napi::bindgen_prelude::Result;
use napi::Error;
use napi_derive::napi;

use crate::math::{Aabb, Vec3};
use crate::napi_api::{
    arrow_state, check_projectile_family, native_arrow_tick_result, native_projectile_tick_result,
    native_trident_tick_result, projectile_state, trident_state, validate_arrow_flight, validate_projectile_flight, validate_trident_flight, vehicle_release, NativeAabb,
    NativeArrowState, NativeArrowTickResult, NativeBlockPosition, NativeProjectileState,
    NativeProjectileTickResult, NativeTridentState, NativeTridentTickResult, NativeVec3,
    NativeWorld,
};
use crate::projectile::{
    tick_arrow_with_targets, tick_projectile_with_targets, tick_trident_with_targets,
};
use crate::projectile_hits::{
    target_hitbox, ProjectileHitReport, ProjectileHitState, ProjectileOwner, ProjectileTarget,
    RocketHolder,
};

/// An entity a projectile may hit. The array's order is Level.getEntities's, which decides ties.
#[napi(object)]
#[derive(Clone)]
pub struct NativeProjectileTarget {
    /// Entity.getId.
    pub id: i32,
    /// Entity.getRootVehicle().getId(); its own id when absent.
    pub root_vehicle: Option<i32>,
    pub position: NativeVec3,
    /// The box as it is; without one, `width`, `height` and `scale` make it.
    pub bounding_box: Option<NativeAabb>,
    /// The width of its pose, before its scale.
    pub width: Option<f64>,
    /// The height of its pose, before its scale.
    pub height: Option<f64>,
    /// LivingEntity.getScale; 1 when absent.
    pub scale: Option<f64>,
    /// Entity.isPickable; true when absent.
    pub pickable: Option<bool>,
    /// Entity.isAlive; true when absent.
    pub alive: Option<bool>,
    pub spectator: Option<bool>,
    /// A player the owning player may not harm.
    pub friendly: Option<bool>,
    /// 1.8: Abilities.invulnerable.
    pub invulnerable_player: Option<bool>,
    /// What Entity.hurt answers; true when absent.
    pub hurt: Option<bool>,
    pub enderman: Option<bool>,
    /// The type deflects projectiles (a breeze).
    pub deflects: Option<bool>,
    /// random.nextFloat() of the 1.20.3 deflection.
    pub deflect_random: Option<f64>,
    /// Entity.noPhysics: fireballs and shulker bullets pass through it.
    pub no_physics: Option<bool>,
    /// An ItemEntity, which a fishing hook catches.
    pub item: Option<bool>,
    /// A wind charge, which wind charges pass through.
    pub wind_charge: Option<bool>,
    /// An end crystal, which wind charges pass through.
    pub end_crystal: Option<bool>,
    /// A dragon's part before 1.21.4: the dragon's box, which a search has to meet to find it.
    pub dragon_box: Option<NativeAabb>,
}

/// Projectile.getOwner, when it gives an entity.
#[napi(object)]
#[derive(Clone, Copy)]
pub struct NativeProjectileOwner {
    pub id: i32,
    /// Its own id when absent.
    pub root_vehicle: Option<i32>,
    pub eye_position: NativeVec3,
    pub width: f64,
    pub alive: bool,
    pub player: bool,
    pub spectator: Option<bool>,
    /// Entity.isRemoved: a fireball is gone with it.
    pub removed: Option<bool>,
    /// ServerPlayer.wonGame: in the end credits, which an ender pearl waits out from 1.21.9.
    /// Absent: false.
    pub won_game: Option<bool>,
}

/// What a projectile keeps between ticks beside its motion.
/// The living entity a firework rocket is attached to.
#[napi(object)]
#[derive(Clone)]
pub struct NativeRocketHolder {
    pub position: NativeVec3,
    pub velocity: NativeVec3,
    /// LivingEntity.isFallFlying.
    pub gliding: bool,
    /// A player: only it holds the rocket in a hand off its centre. Absent: true.
    pub player: Option<bool>,
    pub yaw_degrees: Option<f64>,
    /// The hand with the rocket is the left.
    pub left_hand: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeProjectileHitState {
    /// Entity.tickCount.
    pub tick_count: Option<i32>,
    /// The ticks in flight, which end the owner's immunity until 1.15.
    pub flying_ticks: Option<i32>,
    /// Projectile.leftOwner, from 1.16.
    pub left_owner: Option<bool>,
    /// From 1.14.
    pub pierce_level: Option<u8>,
    /// The ids already pierced.
    pub pierced: Option<Vec<i32>>,
    /// From 1.21.
    pub last_deflected_by: Option<i32>,
    /// AbstractArrow.baseDamage; 2 when absent.
    pub base_damage: Option<f64>,
    /// ThrownTrident.ID_LOYALTY.
    pub loyalty: Option<u8>,
    /// pickup == ALLOWED.
    pub pickup_allowed: Option<bool>,
    /// What a thrown item ignores, from 1.9 until 1.15.
    pub ignored: Option<i32>,
    pub ignore_time: Option<i32>,
    /// A rocket's age, or a hook's time on the ground.
    pub life: Option<i32>,
    /// A rocket's lifetime, a random of the server's: without one it never bursts by age.
    pub life_time: Option<i32>,
    /// The rocket has explosions, which a block hit sets off.
    pub explodes: Option<bool>,
    /// FireworkRocketEntity.isAttachedToEntity, from 1.11: such a rocket never flies. Without a
    /// `holder` its id found no living entity, and it stays where it is.
    pub attached: Option<bool>,
    /// What it is attached to, after its boost (the player's engine runs the boost). Vanilla keeps
    /// one it has resolved even when that is gone, and so does the caller.
    pub holder: Option<NativeRocketHolder>,
    /// A hook's inGround, until 1.15.
    pub in_ground: Option<bool>,
    /// What frees a hook from a block before 1.11: per axis random.nextFloat() * 0.2F, the
    /// server's. Without one it keeps none of its motion.
    pub ground_release: Option<NativeVec3>,
    /// The id a hook hooked.
    pub hooked: Option<i32>,
    /// The id a hook of 1.9 or 1.10 tracks, which a hit must meet twice.
    pub hook_tracked: Option<i32>,
    /// The hook rides `hooked` (HOOKED_IN_ENTITY).
    pub hook_settled: Option<bool>,
    /// FishingHook.shouldStopFishing, the server's say on the owner's rod.
    pub stop_fishing: Option<bool>,
    /// A shulker bullet's targetDelta while its target lives; without one it falls. The server
    /// draws it and sends none of it.
    pub bullet_delta: Option<NativeVec3>,
    /// The server has no chunk here: a fireball is gone.
    pub chunk_unloaded: Option<bool>,
    /// Entity.isOutsideWorldBorder as 1.13's collision query of the box leaves it; pass back the one returned.
    pub outside_border: Option<bool>,
    /// A shulker bullet is gone on peaceful.
    pub peaceful: Option<bool>,
    /// enderPearlsVanishOnDeath is off.
    pub pearl_outlives_owner: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeProjectileEntityHit {
    pub target: i32,
    /// EntityHitResult.getLocation.
    pub location: NativeVec3,
    /// "hurt", "refused", "evaded", "deflected", "spent", "passed" or "hooked".
    pub reaction: String,
    /// An arrow's damage before a critical roll.
    pub damage: Option<i32>,
}

#[napi(object)]
pub struct NativeProjectileHitTick {
    pub projectile: NativeProjectileTickResult,
    pub hit_state: NativeProjectileHitState,
    /// In the order vanilla hit them.
    pub entity_hits: Vec<NativeProjectileEntityHit>,
    pub block_hit: Option<NativeProjectileBlockHit>,
    /// A deflection halves it, for the next tick's state.
    pub acceleration_power: f64,
    /// A deflection sets it in 1.20.5 and 1.20.6, for the next tick's state.
    pub power: NativeVec3,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeProjectileBlockHit {
    pub block: NativeBlockPosition,
    /// Direction.getName of the face.
    pub face: String,
    pub location: NativeVec3,
    pub border: bool,
}

#[napi(object)]
pub struct NativeArrowHitTick {
    pub arrow: NativeArrowTickResult,
    pub hit_state: NativeProjectileHitState,
    /// In the order vanilla hit them.
    pub entity_hits: Vec<NativeProjectileEntityHit>,
    pub block_hit: Option<NativeProjectileBlockHit>,
}

#[napi(object)]
pub struct NativeTridentHitTick {
    pub trident: NativeTridentTickResult,
    /// AbstractArrow.isNoPhysics: a returning trident stays so, for the next tick's state.
    pub no_physics: bool,
    pub hit_state: NativeProjectileHitState,
    pub entity_hits: Vec<NativeProjectileEntityHit>,
    pub block_hit: Option<NativeProjectileBlockHit>,
}

fn target(value: &NativeProjectileTarget) -> Result<ProjectileTarget> {
    let position = Vec3::from(value.position);
    let bounding_box = match (value.bounding_box.clone(), value.width, value.height) {
        (Some(bounds), ..) => Aabb::from(bounds),
        (None, Some(width), Some(height)) => target_hitbox(
            position,
            width as f32,
            height as f32,
            value.scale.unwrap_or(1.0) as f32,
        ),
        _ => {
            return Err(Error::from_reason(format!(
                "projectile target {} needs a boundingBox, or a width and a height",
                value.id
            )))
        }
    };
    Ok(ProjectileTarget {
        root_vehicle: value.root_vehicle.unwrap_or(value.id),
        pickable: value.pickable.unwrap_or(true),
        alive: value.alive.unwrap_or(true),
        spectator: value.spectator.unwrap_or(false),
        friendly: value.friendly.unwrap_or(false),
        invulnerable_player: value.invulnerable_player.unwrap_or(false),
        hurt: value.hurt.unwrap_or(true),
        enderman: value.enderman.unwrap_or(false),
        deflects: value.deflects.unwrap_or(false),
        deflect_random: value.deflect_random.unwrap_or(0.0) as f32,
        no_physics: value.no_physics.unwrap_or(false),
        item: value.item.unwrap_or(false),
        wind_charge: value.wind_charge.unwrap_or(false),
        end_crystal: value.end_crystal.unwrap_or(false),
        dragon_box: value.dragon_box.clone().map(Into::into),
        ..ProjectileTarget::new(value.id, position, bounding_box)
    })
}

fn targets(values: &[NativeProjectileTarget]) -> Result<Vec<ProjectileTarget>> {
    values.iter().map(target).collect()
}

fn owner(value: Option<NativeProjectileOwner>) -> Option<ProjectileOwner> {
    value.map(|value| ProjectileOwner {
        id: value.id,
        root_vehicle: value.root_vehicle.unwrap_or(value.id),
        eye_position: value.eye_position.into(),
        width: value.width,
        alive: value.alive,
        player: value.player,
        spectator: value.spectator.unwrap_or(false),
        removed: value.removed.unwrap_or(false),
        won_game: value.won_game.unwrap_or(false),
    })
}

fn hit_state(value: Option<NativeProjectileHitState>) -> ProjectileHitState {
    let defaults = ProjectileHitState::default();
    let Some(value) = value else {
        return defaults;
    };
    ProjectileHitState {
        tick_count: value.tick_count.unwrap_or(defaults.tick_count),
        flying_ticks: value.flying_ticks.unwrap_or(defaults.flying_ticks),
        left_owner: value.left_owner.unwrap_or(defaults.left_owner),
        pierce_level: value.pierce_level.unwrap_or(defaults.pierce_level),
        pierced: value.pierced.unwrap_or_default(),
        last_deflected_by: value.last_deflected_by,
        base_damage: value.base_damage.unwrap_or(defaults.base_damage),
        loyalty: value.loyalty.unwrap_or(defaults.loyalty),
        pickup_allowed: value.pickup_allowed.unwrap_or(defaults.pickup_allowed),
        ignored: value.ignored,
        ignore_time: value.ignore_time.unwrap_or(defaults.ignore_time),
        life: value.life.unwrap_or(defaults.life),
        life_time: value.life_time,
        explodes: value.explodes.unwrap_or(defaults.explodes),
        attached: value.attached.unwrap_or(defaults.attached),
        holder: value.holder.map(Into::into),
        in_ground: value.in_ground.unwrap_or(defaults.in_ground),
        ground_release: value.ground_release.map(Vec3::from),
        hooked: value.hooked,
        hook_tracked: value.hook_tracked,
        hook_settled: value.hook_settled.unwrap_or(defaults.hook_settled),
        stop_fishing: value.stop_fishing.unwrap_or(defaults.stop_fishing),
        bullet_delta: value.bullet_delta.map(Vec3::from),
        chunk_unloaded: value.chunk_unloaded.unwrap_or(defaults.chunk_unloaded),
        outside_border: value.outside_border.unwrap_or(defaults.outside_border),
        peaceful: value.peaceful.unwrap_or(defaults.peaceful),
        pearl_outlives_owner: value
            .pearl_outlives_owner
            .unwrap_or(defaults.pearl_outlives_owner),
    }
}

impl From<NativeRocketHolder> for RocketHolder {
    fn from(value: NativeRocketHolder) -> Self {
        Self {
            position: value.position.into(),
            velocity: value.velocity.into(),
            gliding: value.gliding,
            player: value.player.unwrap_or(true),
            yaw_degrees: value.yaw_degrees.unwrap_or(0.0) as f32,
            left_hand: value.left_hand.unwrap_or(false),
        }
    }
}

impl From<RocketHolder> for NativeRocketHolder {
    fn from(value: RocketHolder) -> Self {
        Self {
            position: value.position.into(),
            velocity: value.velocity.into(),
            gliding: value.gliding,
            player: Some(value.player),
            yaw_degrees: Some(f64::from(value.yaw_degrees)),
            left_hand: Some(value.left_hand),
        }
    }
}

fn native_hit_state(value: ProjectileHitState) -> NativeProjectileHitState {
    NativeProjectileHitState {
        tick_count: Some(value.tick_count),
        flying_ticks: Some(value.flying_ticks),
        left_owner: Some(value.left_owner),
        pierce_level: Some(value.pierce_level),
        pierced: Some(value.pierced),
        last_deflected_by: value.last_deflected_by,
        base_damage: Some(value.base_damage),
        loyalty: Some(value.loyalty),
        pickup_allowed: Some(value.pickup_allowed),
        ignored: value.ignored,
        ignore_time: Some(value.ignore_time),
        life: Some(value.life),
        life_time: value.life_time,
        explodes: Some(value.explodes),
        attached: Some(value.attached),
        holder: value.holder.map(Into::into),
        in_ground: Some(value.in_ground),
        ground_release: value.ground_release.map(NativeVec3::from),
        hooked: value.hooked,
        hook_tracked: value.hook_tracked,
        hook_settled: Some(value.hook_settled),
        stop_fishing: Some(value.stop_fishing),
        bullet_delta: value.bullet_delta.map(NativeVec3::from),
        chunk_unloaded: Some(value.chunk_unloaded),
        outside_border: Some(value.outside_border),
        peaceful: Some(value.peaceful),
        pearl_outlives_owner: Some(value.pearl_outlives_owner),
    }
}

fn native_report(
    report: ProjectileHitReport,
) -> (Vec<NativeProjectileEntityHit>, Option<NativeProjectileBlockHit>) {
    let entity_hits = report
        .entity_hits
        .into_iter()
        .map(|hit| NativeProjectileEntityHit {
            target: hit.target,
            location: hit.location.into(),
            reaction: hit.reaction.name().to_string(),
            damage: hit.damage,
        })
        .collect();
    let block_hit = report.block_hit.map(|hit| NativeProjectileBlockHit {
        block: NativeBlockPosition {
            x: hit.block.0,
            y: hit.block.1,
            z: hit.block.2,
        },
        face: hit.face.name().to_string(),
        location: hit.location.into(),
        border: hit.border,
    });
    (entity_hits, block_hit)
}

/// One server tick of an arrow among `targets`.
#[napi]
pub fn simulate_versioned_arrow_hit_tick(
    version: String,
    world: &NativeWorld,
    state: NativeArrowState,
    hits: Option<NativeProjectileHitState>,
    targets_in_order: Vec<NativeProjectileTarget>,
    projectile_owner: Option<NativeProjectileOwner>,
) -> Result<NativeArrowHitTick> {
    let release = vehicle_release(&version, "arrow")?;
    let mut state = arrow_state(state, Some(release))?;
    validate_arrow_flight(&state)?;
    let mut hit_state = self::hit_state(hits);
    let targets = targets(&targets_in_order)?;
    let (result, report) = tick_arrow_with_targets(
        &mut state,
        &mut hit_state,
        &targets,
        owner(projectile_owner),
        world.snapshot(),
        release,
    );
    let (entity_hits, block_hit) = native_report(report);
    Ok(NativeArrowHitTick {
        arrow: native_arrow_tick_result(result, &state),
        hit_state: native_hit_state(hit_state),
        entity_hits,
        block_hit,
    })
}

/// One server tick of a trident among `targets`, its return to `projectile_owner` included.
#[napi]
pub fn simulate_versioned_trident_hit_tick(
    version: String,
    world: &NativeWorld,
    state: NativeTridentState,
    hits: Option<NativeProjectileHitState>,
    targets_in_order: Vec<NativeProjectileTarget>,
    projectile_owner: Option<NativeProjectileOwner>,
) -> Result<NativeTridentHitTick> {
    let release = vehicle_release(&version, "trident")?;
    // the trident is 1.13's (EntityType "trident", 1.13.2 TridentEntity)
    // (1.12.2 EntityType.java:233-361; 1.13.2 EntityType.java:246)
    if !release.at_least(1, 13, 0) {
        return Err(Error::from_reason(format!(
            "trident simulation is not supported before 1.13: {version}"
        )));
    }
    let mut state = trident_state(state, Some(release))?;
    validate_trident_flight(&state)?;
    let mut hit_state = self::hit_state(hits);
    let targets = targets(&targets_in_order)?;
    let (result, report) = tick_trident_with_targets(
        &mut state,
        &mut hit_state,
        &targets,
        owner(projectile_owner),
        world.snapshot(),
        release,
    );
    let (entity_hits, block_hit) = native_report(report);
    Ok(NativeTridentHitTick {
        trident: native_trident_tick_result(result, &state),
        no_physics: state.no_physics,
        hit_state: native_hit_state(hit_state),
        entity_hits,
        block_hit,
    })
}

/// One server tick of any other projectile among `targets`: the family's query, reaction and
/// removal. The arrow families have their own calls.
#[napi]
pub fn simulate_versioned_projectile_hit_tick(
    version: String,
    world: &NativeWorld,
    state: NativeProjectileState,
    hits: Option<NativeProjectileHitState>,
    targets_in_order: Vec<NativeProjectileTarget>,
    projectile_owner: Option<NativeProjectileOwner>,
) -> Result<NativeProjectileHitTick> {
    let release = vehicle_release(&version, "projectile")?;
    let mut state = projectile_state(state, Some(release))?;
    check_projectile_family(&state, release, &version)?;
    validate_projectile_flight(&state)?;
    let mut hit_state = self::hit_state(hits);
    let targets = targets(&targets_in_order)?;
    let (result, report) = tick_projectile_with_targets(
        &mut state,
        &mut hit_state,
        &targets,
        owner(projectile_owner),
        world.snapshot(),
        release,
    );
    let (entity_hits, block_hit) = native_report(report);
    Ok(NativeProjectileHitTick {
        projectile: native_projectile_tick_result(result, &state),
        hit_state: native_hit_state(hit_state),
        entity_hits,
        block_hit,
        acceleration_power: state.acceleration_power,
        power: state.power.into(),
    })
}

/// The box of an entity of `width` and `height` scaled by `scale`, as `boundingBox` wants it.
#[napi]
pub fn projectile_target_hitbox(
    position: NativeVec3,
    width: f64,
    height: f64,
    scale: Option<f64>,
) -> NativeAabb {
    target_hitbox(
        position.into(),
        width as f32,
        height as f32,
        scale.unwrap_or(1.0) as f32,
    )
    .into()
}
