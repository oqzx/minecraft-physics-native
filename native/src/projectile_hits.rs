//! What a projectile hits on its way: the entities the caller lists, in the order the level gives
//! them, against the ray the blocks leave. Each family has its own query and its own answer to a
//! hit, by release.
//!
//! Vanilla decides some of this inside the target (Entity.hurt), by random, or from state the server
//! never sends; those decisions are inputs of `ProjectileTarget` and `ProjectileHitState`, so a tick
//! depends on nothing the caller cannot see. The tick models the server, as the rest of the arrow
//! does.

use crate::math::{ceil_i32_for, clamp_f64_for, java_max_f64, Aabb, HitFace, Vec3};
use crate::projectile::{
    border_bounce, clip_blocks, BlockHit, ClipShape, FlightHit, ProjectileKind, ProjectileState,
};
use crate::simulation::{mth_cos, mth_sin};
use crate::version::MinecraftVersion;
use crate::world::WorldSnapshot;

/// ProjectileUtil.DEFAULT_ENTITY_HIT_RESULT_MARGIN, the 0.3F every arrow query inflated by until
/// 1.21.5 (1.8.8 AbstractArrow.java:205, 1.12.2 AbstractArrow.java:409, 1.21.5
/// ProjectileUtil.java:107). This is a target margin, not an entity's push size.
const TARGET_MARGIN: f32 = 0.3;

/// An entity a projectile may hit, in the order Level.getEntities gives it. The order decides ties:
/// an equal distance keeps the earlier candidate (from 1.14), a distance of zero the later one
/// (before 1.14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileTarget {
    /// Entity.getId: what pierced ids and hits name.
    pub id: i32,
    /// Entity.getRootVehicle().getId(): its own id when it rides nothing.
    pub root_vehicle: i32,
    /// Entity.position(): the location of a hit before 1.21.2, and what orders the hits of one
    /// step from 1.21.11.
    pub position: Vec3,
    /// Entity.getBoundingBox() now, of its pose and scale (`target_hitbox`).
    pub bounding_box: Aabb,
    /// Entity.isPickable (collides before 1.17).
    pub pickable: bool,
    pub alive: bool,
    pub spectator: bool,
    /// The target is a player the owning player may not harm (Player.canHarmPlayer false).
    pub friendly: bool,
    /// 1.8's Abilities.invulnerable: the arrow passes through a creative player.
    pub invulnerable_player: bool,
    /// What Entity.hurt answers this hit: armor, invulnerability ticks and Invulnerable are the
    /// target's. False turns an arrow back.
    pub hurt: bool,
    /// An enderman, which teleports away from a projectile.
    pub enderman: bool,
    /// The type deflects this projectile (a breeze): from 1.20.3 the DEFLECTS_ARROWS or
    /// DEFLECTS_TRIDENTS tag, from 1.20.5 Entity.deflection (deflection in 26.x).
    pub deflects: bool,
    /// random.nextFloat() of 1.20.3's Projectile.deflect, which turns the motion by it.
    pub deflect_random: f32,
    /// Entity.noPhysics: a fireball and a shulker bullet pass through it (from 1.9).
    pub no_physics: bool,
    /// An ItemEntity, which a fishing hook catches.
    pub item: bool,
    /// An AbstractWindCharge, which wind charges pass through.
    pub wind_charge: bool,
    /// An end crystal, which wind charges pass through from 1.20.5.
    pub end_crystal: bool,
    /// The box of the ender dragon this part belongs to, before 1.21.4: Level.getEntities finds a
    /// part through its dragon, which the search then has to meet (1.8.9 Chunk, 1.16.5
    /// LevelChunk.java:520-538, 1.21.3 Level.java:578-589, 1.21.4 :583).
    pub dragon_box: Option<Aabb>,
}

/// The damage of a hit: Mth.ceil(speed * damage) until 1.13.2, of Math.max(.., 0.0) in 1.14 - 1.15.2,
/// of Mth.clamp(.., 0.0, 2.147483647E9) from 1.16 (1.12.2 AbstractArrow.java:302, client1.14.4
/// AbstractArrow.java:341, 1.16 AbstractArrow.java:285, 26.3 AbstractArrow.java:412).
pub(crate) fn arrow_damage(speed: f64, base_damage: f64, release: MinecraftVersion) -> i32 {
    let raw = speed * base_damage;
    let bounded = if release.at_least(1, 16, 0) {
        clamp_f64_for(release, raw, 0.0, 2.147483647e9)
    } else if release.at_least(1, 14, 0) {
        java_max_f64(raw, 0.0)
    } else {
        raw
    };
    ceil_i32_for(release, bounded)
}

impl ProjectileTarget {
    /// A living target that nothing excludes, hurt by the hit.
    #[must_use]
    pub const fn new(id: i32, position: Vec3, bounding_box: Aabb) -> Self {
        Self {
            id,
            root_vehicle: id,
            position,
            bounding_box,
            pickable: true,
            alive: true,
            spectator: false,
            friendly: false,
            invulnerable_player: false,
            hurt: true,
            enderman: false,
            deflects: false,
            deflect_random: 0.0,
            no_physics: false,
            item: false,
            wind_charge: false,
            end_crystal: false,
            dragon_box: None,
        }
    }

    /// Whether Level.getEntities gives it for this box.
    pub(crate) fn in_search(&self, search: Aabb) -> bool {
        self.bounding_box.intersects(search)
            && self.dragon_box.map_or(true, |dragon| dragon.intersects(search))
    }
}

/// EntityDimensions.makeBoundingBox of EntityDimensions.scale: a `width` by `height` box in floats
/// about `position` (26.3 EntityDimensions.java, 1.8.8 Entity.setBoundingBox's width / 2.0F). The
/// width and height are those of the entity's pose; the scale is LivingEntity.getScale (age and,
/// from 1.20.5, the SCALE attribute).
#[must_use]
pub fn target_hitbox(position: Vec3, width: f32, height: f32, scale: f32) -> Aabb {
    let half_width = f64::from(width * scale / 2.0);
    let height = f64::from(height * scale);
    Aabb::new(
        Vec3::new(position.x - half_width, position.y, position.z - half_width),
        Vec3::new(position.x + half_width, position.y + height, position.z + half_width),
    )
}

/// What Projectile.getOwner returns, when it returns anything: None is an owner that was never set,
/// is gone from the level or is not loaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileOwner {
    pub id: i32,
    pub root_vehicle: i32,
    /// Entity.getEyePosition: where a loyal trident returns to.
    pub eye_position: Vec3,
    /// Entity.getBbWidth.
    pub width: f64,
    pub alive: bool,
    /// A player: it picks a trident up instead of the trident vanishing near it.
    pub player: bool,
    /// A spectating player: a trident cannot return to it.
    pub spectator: bool,
    /// Entity.isRemoved: a fireball whose owner is removed is gone.
    pub removed: bool,
    /// ServerPlayer.wonGame: the player is in the end credits, which a pearl waits out from 1.21.9.
    pub won_game: bool,
}

/// The LivingEntity a firework rocket is attached to, after its boost: the rocket stands at it,
/// from 1.18.2 at a gliding player's hand, and takes its motion (26.3 FireworkRocketEntity.java:119-137).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RocketHolder {
    pub position: Vec3,
    pub velocity: Vec3,
    /// LivingEntity.isFallFlying.
    pub gliding: bool,
    /// Entity.getHandHoldingItemAngle answers for a player alone.
    pub player: bool,
    pub yaw_degrees: f32,
    /// The hand with the rocket is the left: the off hand of a right-handed player alone holds one.
    pub left_hand: bool,
}

/// What a projectile keeps between its ticks, beside its motion.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileHitState {
    /// Entity.tickCount, before this tick's increment.
    pub tick_count: i32,
    /// flyingTick, the owner's immunity until 1.15: a shooter is hit from the fifth tick.
    pub flying_ticks: i32,
    /// Projectile.leftOwner, from 1.16.
    pub left_owner: bool,
    /// AbstractArrow.getPierceLevel, from 1.14.
    pub pierce_level: u8,
    /// AbstractArrow.piercingIgnoreEntityIds.
    pub pierced: Vec<i32>,
    /// Projectile.lastDeflectedBy, from 1.21.
    pub last_deflected_by: Option<i32>,
    /// AbstractArrow.baseDamage, with the enchantments' modifyDamage of its weapon.
    pub base_damage: f64,
    /// ThrownTrident.ID_LOYALTY.
    pub loyalty: u8,
    /// pickup == ALLOWED: a loyal trident does not despawn.
    pub pickup_allowed: bool,
    /// ThrowableEntity's ignored entity from 1.9 until 1.15: what stood at the throw, which the
    /// item passes through while it stays near.
    pub ignored: Option<i32>,
    /// How long `ignored` lasts after the item is clear of it.
    pub ignore_time: i32,
    /// FireworkRocketEntity.life, and FishingHook.life on the ground.
    pub life: i32,
    /// FireworkRocketEntity.lifetime, a random of the server's: None never bursts by age.
    pub life_time: Option<i32>,
    /// The rocket has explosions, which a block hit sets off (the client sees it in the item).
    pub explodes: bool,
    /// FireworkRocketEntity.isAttachedToEntity, from 1.11: it never flies again, whatever becomes
    /// of what it is attached to (DATA_ATTACHED_TO_TARGET is set once, 26.3
    /// FireworkRocketEntity.java:67-72, 108-138).
    pub attached: bool,
    /// What the attachment resolved to. None: an id no living entity answers to, so the rocket
    /// stays where it is (26.3 :110-118); the level keeps a resolved entity even once it is gone.
    pub holder: Option<RocketHolder>,
    /// FishingHook.inGround until 1.15, the server's flag for a hook that hit a block.
    pub in_ground: bool,
    /// What frees a hook from a block before 1.11: per axis random.nextFloat() * 0.2F, drawn by the
    /// server. None keeps none of its motion.
    pub ground_release: Option<Vec3>,
    /// FishingHook.hookedIn: the id of what it hooked.
    pub hooked: Option<i32>,
    /// DATA_HOOKED_ENTITY's id in 1.9 and 1.10, where setting a new one clears the catch.
    pub hook_tracked: Option<i32>,
    /// FishingHook.currentState is HOOKED_IN_ENTITY: the hook rides `hooked`.
    pub hook_settled: bool,
    /// FishingHook.shouldStopFishing: the owner put the rod away or went too far, which the server
    /// decides from its inventory.
    pub stop_fishing: bool,
    /// ShulkerBullet's targetDeltaX, Y and Z while its final target lives: None falls by gravity.
    /// The server draws each next delta by random and sends none of them, so this is an input and
    /// the tick only clamps and grows it.
    pub bullet_delta: Option<Vec3>,
    /// Level.hasChunkAt is false on the server, which a fireball does not survive.
    pub chunk_unloaded: bool,
    /// Entity.isOutsideWorldBorder, which 1.13's collision query of a fireball's box keeps (1.13.2
    /// LevelReader.java:172-200).
    pub outside_border: bool,
    /// The difficulty is peaceful: a shulker bullet is gone.
    pub peaceful: bool,
    /// The game rule enderPearlsVanishOnDeath is off: a dead owner keeps the pearl flying.
    pub pearl_outlives_owner: bool,
}

impl Default for ProjectileHitState {
    /// A projectile's state before its first tick: nothing hit or ignored, and an arrow's 2.0 base
    /// damage (AbstractArrow#baseDamage).
    fn default() -> Self {
        Self {
            tick_count: 0,
            flying_ticks: 0,
            left_owner: false,
            pierce_level: 0,
            pierced: Vec::new(),
            last_deflected_by: None,
            base_damage: 2.0,
            loyalty: 0,
            pickup_allowed: false,
            ignored: None,
            ignore_time: 0,
            life: 0,
            life_time: None,
            explodes: false,
            attached: false,
            holder: None,
            in_ground: false,
            ground_release: None,
            hooked: None,
            hook_tracked: None,
            hook_settled: false,
            stop_fishing: false,
            bullet_delta: None,
            chunk_unloaded: false,
            outside_border: false,
            peaceful: false,
            pearl_outlives_owner: false,
        }
    }
}

/// How a hit ended for the projectile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileReaction {
    /// Entity.hurt accepted it.
    Hurt,
    /// Entity.hurt refused it, and the projectile turned back.
    Refused,
    /// An enderman went away: the projectile flies on.
    Evaded,
    /// The target's type deflected the projectile.
    Deflected,
    /// A piercing arrow had no pierce left: it is gone, and the target untouched.
    Spent,
    /// The projectile flew on, the hit having done nothing to it.
    Passed,
    /// A fishing hook caught the target.
    Hooked,
}

impl ProjectileReaction {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hurt => "hurt",
            Self::Refused => "refused",
            Self::Evaded => "evaded",
            Self::Deflected => "deflected",
            Self::Spent => "spent",
            Self::Passed => "passed",
            Self::Hooked => "hooked",
        }
    }
}

/// A hit on an entity, in the order vanilla made them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileEntityHit {
    pub target: i32,
    /// EntityHitResult.getLocation: the entity's position before 1.21.2, the point on the ray from
    /// then (1.21.1 ProjectileUtil.java:137, 1.21.2 :139); from 26.3 the point on the inflated box
    /// when the ray misses the box itself (26.3 ProjectileUtil.java:195).
    pub location: Vec3,
    pub reaction: ProjectileReaction,
    /// An arrow's damage before a critical roll, Mth.ceil(speed * baseDamage). None for a trident.
    pub damage: Option<i32>,
}

/// A hit on a block or on the world border.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileBlockHit {
    pub block: (i32, i32, i32),
    pub face: HitFace,
    pub location: Vec3,
    /// BlockHitResult.isWorldBorderHit, from 1.21.2.
    pub border: bool,
}

/// What a tick hit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectileHitReport {
    pub entity_hits: Vec<ProjectileEntityHit>,
    pub block_hit: Option<ProjectileBlockHit>,
}

/// One tick's view of the scene, for the ticks in `projectile`.
pub(crate) struct HitScene<'a> {
    pub targets: &'a [ProjectileTarget],
    /// The family: its box, query and reactions. An arrow's for an arrow and a trident.
    pub kind: ProjectileKind,
    /// The tick is the server's and meets `targets`; otherwise it is the plain flight, in which a
    /// hit only reports a block.
    pub live: bool,
    /// A hit that the base tick comes before, from 1.21.2 (`FlightHit`).
    pub pending: Option<FlightHit>,
    pub owner: Option<ProjectileOwner>,
    pub hits: &'a mut ProjectileHitState,
    pub trident: bool,
    /// ThrownTrident.dealtDamage.
    pub dealt_damage: bool,
    pub report: ProjectileHitReport,
    /// A hit discarded the projectile.
    pub removed: bool,
    left_owner_checked: bool,
}

/// What `find_hits` found: the target's index and the hit's location.
pub(crate) type Found = (usize, Vec3);

impl<'a> HitScene<'a> {
    pub(crate) fn new(
        targets: &'a [ProjectileTarget],
        owner: Option<ProjectileOwner>,
        hits: &'a mut ProjectileHitState,
    ) -> Self {
        Self {
            targets,
            kind: ProjectileKind::Arrow,
            live: false,
            pending: None,
            owner,
            hits,
            trident: false,
            dealt_damage: false,
            report: ProjectileHitReport::default(),
            removed: false,
            left_owner_checked: false,
        }
    }

    /// AbstractArrow.getPierceLevel: there is none before 1.14.
    pub(crate) fn pierce_level(&self, release: MinecraftVersion) -> u8 {
        // (1.13.2 AbstractArrow.java:225-228; 1.14.4 AbstractArrow.java:226-231, 589-591)
        if release.at_least(1, 14, 0) {
            self.hits.pierce_level
        } else {
            0
        }
    }

    /// Projectile.checkLeftOwner (from 1.16; 1.16.5 Projectile.java:95-111, 26.3
    /// Projectile.java:136-153; 1.16 Projectile.java:77-78, where 1.15.2 AbstractArrow.java:465 spares the owner by
    /// flightTime alone): once per tick the projectile is clear of everything the owner
    /// rides with, or has no owner, it has left. The owner and what it rides must be among the
    /// targets to hold the projectile back.
    pub(crate) fn check_left_owner(
        &mut self,
        position: Vec3,
        velocity: Vec3,
        release: MinecraftVersion,
    ) {
        if !release.at_least(1, 16, 0) || self.hits.left_owner || self.left_owner_checked {
            return;
        }
        self.left_owner_checked = true;
        let Some(owner) = self.owner else {
            self.hits.left_owner = true;
            return;
        };
        let reach = self.search_box(position, velocity);
        self.hits.left_owner = !self.targets.iter().any(|target| {
            target.root_vehicle == owner.root_vehicle
                && target.pickable
                && !target.spectator
                && target.in_search(reach)
        });
    }

    /// Whether the owner's immunity and the rest of Projectile.canHitEntity let this target be hit.
    ///
    /// 1.8: Entity.collides and the owner's five ticks (1.8.8 AbstractArrow.java:204). 1.9: not a
    /// spectator, alive (1.12.2 AbstractArrow.java:38-42). 1.14: not yet pierced (1.15
    /// AbstractArrow.java:462-466). 1.16: the owner's vehicle until the arrow has left (1.16.5
    /// Projectile.java:157-164). 1.21.2: a player the owner may not harm is no candidate; an
    /// earlier arrow picks it and then drops the whole hit (1.21.1
    /// AbstractArrow, 1.21.2 AbstractArrow.java:553).
    fn can_hit(&self, target: &ProjectileTarget, release: MinecraftVersion) -> bool {
        if !target.pickable {
            return false;
        }
        // the query itself refuses a spectator in every release (1.8.8 Level.java:2173-2175
        // getEntitiesIn with NO_SPECTATORS, 1.8.8 AbstractArrow.java:199)
        if target.spectator || (release.at_least(1, 9, 0) && !target.alive) {
            return false;
        }
        if release.at_least(1, 21, 2) && target.friendly {
            return false;
        }
        // (1.13.2 AbstractArrow.java:41 has no pierce; 1.14.4 AbstractArrow.java:452 piercingIgnoreEntityIds)
        if release.at_least(1, 14, 0) && self.hits.pierced.contains(&target.id) {
            return false;
        }
        // (1.15.2 AbstractArrow.java:465; 1.16 Projectile.java:141)
        if release.at_least(1, 16, 0) {
            self.owner.map_or(true, |owner| {
                self.hits.left_owner || owner.root_vehicle != target.root_vehicle
            })
        } else {
            self.owner.map_or(true, |owner| owner.id != target.id)
                || self.hits.flying_ticks >= 5
        }
    }

    /// AbstractArrow.findHitEntity, findHitEntities from 1.21.11. The search box is the arrow's,
    /// stretched by its motion now and inflated by 1.0 (1.8.8 AbstractArrow.java:199, 26.3
    /// AbstractArrow.java:594). A trident that dealt its damage finds nothing, and takes the
    /// nearest even where an arrow takes all (26.3 ThrownTrident.java:109-116).
    pub(crate) fn find_hits(
        &self,
        from: Vec3,
        to: Vec3,
        velocity: Vec3,
        world: &WorldSnapshot,
        release: MinecraftVersion,
    ) -> Vec<Found> {
        if self.trident && self.dealt_damage {
            return Vec::new();
        }
        let search = self.search_box(from, velocity);
        let candidates = self.targets.iter().enumerate().filter(|(_, target)| {
            target.in_search(search) && self.can_hit(target, release)
        });
        let margin = target_margin(release, self.hits.tick_count);
        // (1.21.10 AbstractArrow.java:258; 1.21.11 AbstractArrow.java:260-261)
        if release.at_least(1, 21, 11) && !self.trident {
            let mut found = many_hits(candidates, from, to, margin, world, release);
            // a stable sort by the entity's own position, not by the hit (1.21.11
            // AbstractArrow.java:260-261, 26.3 :260-261)
            found.sort_by(|a, b| {
                let distance = |hit: &Found| from.distance_squared(self.targets[hit.0].position);
                distance(a).total_cmp(&distance(b))
            });
            return found;
        }
        nearest_hit(candidates, from, to, margin, release).into_iter().collect()
    }

    /// Projectile.hitTargetOrDeflectSelf and onHitEntity of the target `index`. Returns whether the
    /// target deflected the projectile, which ends the step.
    pub(crate) fn hit(
        &mut self,
        (index, ray_point): Found,
        velocity: &mut Vec3,
        release: MinecraftVersion,
    ) -> bool {
        let target = self.targets[index];
        // (1.21.1 ProjectileUtil.java:137; 1.21.2 ProjectileUtil.java:139)
        let location = if release.at_least(1, 21, 2) {
            ray_point
        } else {
            target.position
        };
        // Entity.getProjectileDeflection from 1.20.5; from 1.21 one entity deflects it only once
        // (1.20.5 Projectile.java:158-170, 1.21 :168-177, 26.3 Projectile.java:246-255)
        if release.at_least(1, 20, 5) && target.deflects {
            if !release.at_least(1, 21, 0) || self.hits.last_deflected_by != Some(target.id) {
                *velocity = velocity.scale(-0.5);
                self.hits.last_deflected_by = Some(target.id);
            }
            self.record(target.id, location, ProjectileReaction::Deflected, None);
            return true;
        }
        if self.trident {
            self.hit_as_trident(&target, location, velocity, release);
        } else {
            self.hit_as_arrow(&target, location, velocity, release);
        }
        false
    }

    /// AbstractArrow.onHitEntity (26.3 AbstractArrow.java:404-498, 1.16.5
    /// AbstractArrow.java:280-385, 1.13.2 AbstractArrow.java:343-403, 1.8.8
    /// AbstractArrow.java:229-307).
    fn hit_as_arrow(
        &mut self,
        target: &ProjectileTarget,
        location: Vec3,
        velocity: &mut Vec3,
        release: MinecraftVersion,
    ) {
        let speed = f64::from(velocity.length() as f32);
        let damage = arrow_damage(speed, self.hits.base_damage, release);
        let pierce = self.pierce_level(release);
        if pierce > 0 {
            if self.hits.pierced.len() >= usize::from(pierce) + 1 {
                self.removed = true;
                self.record(target.id, location, ProjectileReaction::Spent, Some(damage));
                return;
            }
            self.hits.pierced.push(target.id);
        }
        let reaction = if target.hurt {
            // Enderman.hurt teleports and says true, and the arrow flies on; from 26.3 it says
            // false, and a true is an ordinary hit (26.2 AbstractArrow.java:448-451, 26.3 :445)
            if target.enderman && !release.at_least(26, 3, 0) {
                ProjectileReaction::Evaded
            } else {
                self.removed = pierce == 0;
                ProjectileReaction::Hurt
            }
        } else if target.enderman && release.at_least(26, 3, 0) {
            // Entity.projectileReceivesSideEffectsOnHit (26.3 AbstractArrow.java:484, Enderman.java:362)
            ProjectileReaction::Evaded
        // (1.20.2 AbstractArrow.java:337-390 has no DEFLECTS_ARROWS; 1.20.3 AbstractArrow.java:345, 396 and 1.20.4 the
        // same; 1.20.5 AbstractArrow.java:401 deflects through ProjectileDeflection instead)
        } else if target.deflects && release.at_least(1, 20, 3) && !release.at_least(1, 20, 5) {
            *velocity = turned_by_random(*velocity, target.deflect_random, release);
            ProjectileReaction::Deflected
        } else {
            self.refused(velocity, release);
            ProjectileReaction::Refused
        };
        self.record(target.id, location, reaction, Some(damage));
    }

    /// An arrow turns back at a refusal, -0.1 of its motion: as floats until 1.13.2, and -0.5 by
    /// 0.2 from 1.20.5, -power * 0.5 from 26.3 (`border_bounce`). A slow one is gone, from 1.9
    /// (1.13.2 AbstractArrow.java:391-402, 1.16.5 AbstractArrow.java:370-382,
    /// 1.20.5 :228-235, 26.3 AbstractArrow.java:484-497); 1.8.8 keeps it (AbstractArrow.java:276-283).
    /// Until 1.15 the shooter's five ticks start again.
    fn refused(&mut self, velocity: &mut Vec3, release: MinecraftVersion) {
        if !release.at_least(1, 14, 0) {
            *velocity = velocity.scale(f64::from(-0.1_f32));
        } else if !release.at_least(1, 20, 5) {
            *velocity = velocity.scale(-0.1);
        } else {
            *velocity = border_bounce(*velocity, release);
        }
        // (1.15.2 AbstractArrow.java:432 flightTime = 0; 1.16 AbstractArrow.java:373-376 not)
        if !release.at_least(1, 16, 0) {
            self.hits.flying_ticks = 0;
        }
        // (1.13.2 AbstractArrow.java:397; 1.14.4 AbstractArrow.java:427)
        let too_slow = if release.at_least(1, 14, 0) {
            1.0e-7
        } else {
            f64::from(0.001_f32)
        };
        // (1.8.8 AbstractArrow.java:277-282 keeps it; 1.9.4 AbstractArrow.java:349 removes it)
        if release.at_least(1, 9, 0) && velocity.length_squared() < too_slow {
            self.removed = true;
        }
    }

    /// ThrownTrident.onHitEntity (1.13.2 ThrownTrident.java:113-162, 1.20.4 :115-138, 1.21.2
    /// ThrownTrident.java:144, 26.3 :123-156): dealtDamage whatever the target says; the motion
    /// turns back at 0.01, 0.1, 0.01 of itself, in floats in 1.13.2. An enderman that took it from
    /// 1.15 keeps the motion (1.14.4 has no such test; 1.15 ThrownTrident.java:130), and from 26.3
    /// an enderman that refused it does.
    fn hit_as_trident(
        &mut self,
        target: &ProjectileTarget,
        location: Vec3,
        velocity: &mut Vec3,
        release: MinecraftVersion,
    ) {
        self.dealt_damage = true;
        let mut reaction = if target.hurt {
            ProjectileReaction::Hurt
        } else {
            ProjectileReaction::Refused
        };
        // (26.2 ThrownTrident.java:136-138; 26.3 ThrownTrident.java:150-153)
        let side_effects = if release.at_least(26, 3, 0) {
            target.hurt || !target.enderman
        } else {
            // (1.14.4 ThrownTrident.java:119; 1.15 ThrownTrident.java:130)
            !(target.hurt && target.enderman && release.at_least(1, 15, 0))
        };
        if !side_effects {
            reaction = ProjectileReaction::Evaded;
        // (1.20.2 ThrownTrident.java:126-140 has no DEFLECTS_TRIDENTS; 1.20.3 ThrownTrident.java:133-134)
        } else if !target.hurt
            && target.deflects
            && release.at_least(1, 20, 3)
            && !release.at_least(1, 20, 5)
        {
            *velocity = turned_by_random(*velocity, target.deflect_random, release);
            reaction = ProjectileReaction::Deflected;
        } else if release.at_least(26, 3, 0) {
            *velocity = Vec3::new(
                velocity.x * (-0.02 * 0.5),
                velocity.y * (-0.2 * 0.5),
                velocity.z * (-0.02 * 0.5),
            );
        // (1.21.1 ThrownTrident.java:137; 1.21.2 ThrownTrident.java:144)
        } else if release.at_least(1, 21, 2) {
            *velocity = velocity.scale(-0.5).multiply(Vec3::new(0.02, 0.2, 0.02));
        // (1.13.2 ThrownTrident.java:134-136; 1.14.4 ThrownTrident.java:129)
        } else if release.at_least(1, 14, 0) {
            *velocity = velocity.multiply(Vec3::new(-0.01, -0.1, -0.01));
        } else {
            *velocity = velocity.multiply(Vec3::new(
                f64::from(-0.01_f32),
                f64::from(-0.1_f32),
                f64::from(-0.01_f32),
            ));
        }
        self.record(target.id, location, reaction, None);
    }

    /// The search box of a query: the projectile's box stretched by its motion, inflated by 1.0
    /// (ProjectileUtil#getHitResult, AbstractArrow#findHitEntity).
    fn search_box(&self, from: Vec3, velocity: Vec3) -> Aabb {
        self.kind
            .bounding_box(from)
            .expand_towards(velocity)
            .inflate(Vec3::new(1.0, 1.0, 1.0))
    }

    /// Whether a family other than the arrow may take this target. Until 1.15 each family had its
    /// own test; Projectile.canHitEntity holds from 1.16 (1.16.5 Projectile.java:157-164, 26.3
    /// Projectile.java:331-338), with the family's overrides.
    ///
    /// Thrown items: Entity.collides and the owner's five ticks (1.8.8 ThrowableProjectile.java:165);
    /// `thrown_hit_1_9` from 1.9; not ignored in 1.14 and 1.15 (1.15 ThrownEntity:
    /// `!isSpectator() && collides()`). A hurting projectile: the owner from its 25th tick, from 1.9
    /// not a noPhysics target (1.8.8 AbstractHurtingProjectile.java:125,
    /// 1.12.2 ProjectileUtil.java:43, 1.15 ProjectileUtil.java:68); not noPhysics from 1.16 either
    /// (26.3 AbstractHurtingProjectile.java:119). A shulker bullet never takes its owner until 1.16
    /// (1.13.2 ShulkerBulletEntity tick). A firework takes the living from 1.14 (1.14
    /// FireworkEntity: `!isSpectator() && isAlive() && collides()`). Llama spit took anything but
    /// its owner, collidable or not, until 1.15 (1.11.2 LlamaSpit.java:141, 1.14 :
    /// `!isSpectator() && != owner`). A fishing hook also takes an item (1.11.2
    /// FishingHook.java:288, 401; 1.14 FishHookEntity; 26.3 FishingHook.java:263).
    fn family_can_hit(&self, target: &ProjectileTarget, release: MinecraftVersion) -> bool {
        let kind = self.kind;
        let is_owner = self.owner.map(|owner| owner.id) == Some(target.id);
        // no release lets a spectator be a candidate: before 1.14 the query is getEntities(this, box)
        // with NO_SPECTATORS (1.8.8 Level.java:2173-2175, 1.12.2 :2163-2165, 1.13.2
        // EntityGetter.java:18-20), from 1.14 each test says !isSpectator()
        if target.spectator {
            return false;
        }
        // (1.15.2 ThrowableProjectile.java:109, 123 each family's own; 1.16 Projectile.java:138-145)
        if release.at_least(1, 16, 0) {
            let base = target.pickable
                && target.alive
                && self.owner.map_or(true, |owner| {
                    self.hits.left_owner || owner.root_vehicle != target.root_vehicle
                });
            return match kind {
                ProjectileKind::FishingHook => base || (target.alive && target.item),
                ProjectileKind::ShulkerBullet => base && !target.no_physics,
                ProjectileKind::WindCharge => {
                    base && !target.no_physics
                        && !target.wind_charge
                        // (1.20.4 WindCharge.java:61-63; 1.20.5 AbstractWindCharge.java:52-58)
                        && !(target.end_crystal && release.at_least(1, 20, 5))
                }
                _ if kind.is_hurting() => base && !target.no_physics,
                _ => base,
            };
        }
        match kind {
            ProjectileKind::LlamaSpit => !is_owner,
            ProjectileKind::Firework => target.alive && target.pickable,
            ProjectileKind::ShulkerBullet => !is_owner && target.pickable && !target.no_physics,
            ProjectileKind::FishingHook => {
                // (no items until 1.9.4's tick: 1.10.2 FishingHook.java:245, 414)
                let takes_items = target.item && release.at_least(1, 10, 0);
                (target.pickable || takes_items) && (!is_owner || self.hits.flying_ticks >= 5)
            }
            _ if kind.is_hurting() => {
                target.pickable
                    && (!is_owner || self.hits.flying_ticks >= 25)
                    && (!release.at_least(1, 9, 0) || !target.no_physics)
            }
            _ if !release.at_least(1, 9, 0) => {
                target.pickable && (!is_owner || self.hits.flying_ticks >= 5)
            }
            _ => target.pickable && self.hits.ignored != Some(target.id),
        }
    }

    /// ThrowableProjectile.tick of 1.9 to 1.13 (1.9.4 ThrowableProjectile, 1.13.2 ThrowableProjectile.java:141-168): what the box
    /// meets on the item's first tick is ignored, with a thrower from 1.11, and stays so until the
    /// item has been clear of it for two more ticks. The last candidate of the list says whether it
    /// is still near, which is how the source reads.
    fn thrown_hit_1_9(
        &mut self,
        from: Vec3,
        to: Vec3,
        search: Aabb,
        release: MinecraftVersion,
    ) -> Option<Found> {
        let margin = f64::from(TARGET_MARGIN);
        let mut nearest = 0.0;
        let mut found = None;
        let mut near = false;
        for (index, target) in self.targets.iter().enumerate() {
            // getEntitiesIn lists no spectator (1.12.2 Level.java:2163-2165, ThrowableProjectile.java:156)
            if !target.bounding_box.intersects(search) || target.spectator || !target.pickable {
                continue;
            }
            if self.hits.ignored == Some(target.id) {
                near = true;
            } else if self.hits.tick_count < 2
                && self.hits.ignored.is_none()
                // (1.10.2 ThrowableProjectile.java:165; 1.11.2 ThrowableProjectile.java:165)
                && (!release.at_least(1, 11, 0) || self.owner.is_some())
            {
                self.hits.ignored = Some(target.id);
                near = true;
            } else {
                near = false;
                let bounds = target.bounding_box.inflate(Vec3::new(margin, margin, margin));
                if let Some(point) = clip_box(bounds, from, to, release) {
                    let distance = from.distance_squared(point);
                    if distance < nearest || nearest == 0.0 {
                        nearest = distance;
                        found = Some((index, point));
                    }
                }
            }
        }
        if self.hits.ignored.is_some() {
            if near {
                self.hits.ignore_time = 2;
            } else {
                self.run_down_ignored();
            }
        }
        found
    }

    /// `ignoreTime-- <= 0` ends the ignoring (ThrowableProjectile#tick, its timeToIgnore before 1.14:
    /// ThrowableProjectile#timeToIgnore).
    fn run_down_ignored(&mut self) {
        let left = self.hits.ignore_time;
        self.hits.ignore_time = self.hits.ignore_time.wrapping_sub(1);
        if left <= 0 {
            self.hits.ignored = None;
        }
    }

    /// The first half of ThrowableProjectile.tick in 1.14 and 1.15 (1.15 ThrowableProjectile): the first candidate
    /// in the box either holds its ignoring open or, thrown by someone on the first tick, starts it.
    fn thrown_ignore_1_14(&mut self, search: Aabb) {
        for target in self.targets {
            if !target.bounding_box.intersects(search) || target.spectator || !target.pickable {
                continue;
            }
            if self.hits.ignored == Some(target.id) {
                self.hits.ignore_time = self.hits.ignore_time.wrapping_add(1);
                break;
            }
            if self.owner.is_some() && self.hits.tick_count < 2 && self.hits.ignored.is_none() {
                self.hits.ignored = Some(target.id);
                self.hits.ignore_time = 3;
                break;
            }
        }
    }

    /// The family's query: ProjectileUtil.getHitResultOnMoveVector (getCollision), whose entity
    /// replaces the block on the way to it (26.3 ProjectileUtil.java:66-77), or the own loop each
    /// family had before it. `stuck` is ProjectileUtil's test of 1.13 to 1.15 that the projectile's
    /// box is in a block, which answers with that block and looks at no entity.
    pub(crate) fn flight_hit(
        &mut self,
        from: Vec3,
        velocity: Vec3,
        block: Option<BlockHit>,
        stuck: bool,
        release: MinecraftVersion,
    ) -> Option<FlightHit> {
        if stuck || !self.live {
            return block.map(FlightHit::Block);
        }
        let to = block.map_or(from + velocity, |hit| hit.location);
        let search = self.search_box(from, velocity);
        let thrown = self.kind.is_thrown();
        // (1.13.2 ThrowableProjectile.java:141-170; 1.14.4 ThrowableProjectile.java:108-132; 1.8.8
        // ThrowableProjectile.java:165 spares the thrower for five ticks, 1.9.4 ThrowableProjectile.java:157-162 ignores
        // what it meets first)
        let found = if thrown && release.at_least(1, 9, 0) && !release.at_least(1, 14, 0) {
            self.thrown_hit_1_9(from, to, search, release)
        } else {
            // (1.13.2 ThrowableProjectile.java:141-170; 1.14.4 ThrowableProjectile.java:108-132; 1.15.2
            // ThrowableProjectile.java:109-123 still ignores; 1.16 Projectile.java:138-145 does not)
            let ignores = thrown && release.at_least(1, 14, 0) && !release.at_least(1, 16, 0);
            if ignores {
                self.thrown_ignore_1_14(search);
            }
            let candidates = self.targets.iter().enumerate().filter(|(_, target)| {
                target.bounding_box.intersects(search) && self.family_can_hit(target, release)
            });
            let margin = target_margin(release, self.hits.tick_count);
            let found = nearest_hit(candidates, from, to, margin, release);
            if ignores && self.hits.ignored.is_some() {
                self.run_down_ignored();
            }
            found
        };
        found.map(FlightHit::Entity).or(block.map(FlightHit::Block))
    }

    /// Projectile.hitTargetOrDeflectSelf (hitOrDeflect from 1.20.5) of a family other than the arrow.
    /// A deflection turns the motion and halves the acceleration of a hurting projectile: from 1.21
    /// accelerationPower, in 1.20.5 and 1.20.6 the power from the turned motion (1.20.5
    /// AbstractHurtingProjectile.java:181-191, 1.21 :169-176). A breeze lets wind charges through
    /// (1.20.5 Breeze.java:174-176, 26.3 Breeze.java:178-182).
    pub(crate) fn hit_flight(
        &mut self,
        hit: FlightHit,
        state: &mut ProjectileState,
        release: MinecraftVersion,
    ) {
        if !self.live {
            return;
        }
        match hit {
            FlightHit::Block(block) => self.hit_block(block, release),
            FlightHit::Entity((index, ray_point)) => {
                let target = self.targets[index];
                // (1.21.1 ProjectileUtil.java:137; 1.21.2 ProjectileUtil.java:139)
                let location = if release.at_least(1, 21, 2) {
                    ray_point
                } else {
                    target.position
                };
                let deflects = target.deflects
                    && self.kind != ProjectileKind::WindCharge
                    && release.at_least(1, 20, 5);
                if deflects {
                    // (1.20.6 Projectile.java:163; 1.21 Projectile.java:171)
                    if !release.at_least(1, 21, 0) || self.hits.last_deflected_by != Some(target.id) {
                        state.velocity = state.velocity.scale(-0.5);
                        self.hits.last_deflected_by = Some(target.id);
                        if state.kind.is_hurting() {
                            // (1.20.6 AbstractHurtingProjectile.java:188; 1.21 AbstractHurtingProjectile.java:174)
                            if release.at_least(1, 21, 0) {
                                state.acceleration_power *= 0.5;
                            } else {
                                state.power = state.velocity.scale(0.05);
                            }
                        }
                    }
                    self.record(target.id, location, ProjectileReaction::Deflected, None);
                    return;
                }
                let reaction = self.family_reaction(&target, release);
                self.record(target.id, location, reaction, None);
            }
        }
    }

    /// What the family does to itself and the target when it hits: it is gone, but for these.
    ///
    /// An ender pearl hits its thrower and flies on until 1.15 (1.8.8 ThrownEnderpearl.java:32, 1.14.4
    /// ThrownEnderpearl.java:49; no such test in 1.16). A dragon fireball does not burst on its
    /// owner (1.12.2 DragonFireballEntity, 26.3 DragonFireball.java:32). Llama spit was gone at any
    /// hit until 1.13 and flies on through entities from 1.14, hurting them only with a living
    /// owner (1.11.2 LlamaSpit.java:179-187, 1.14.4 :124-131, 26.3 LlamaSpit.java:65-82). A
    /// fishing hook takes what it hit (26.3 FishingHook.java:267-273).
    fn family_reaction(
        &mut self,
        target: &ProjectileTarget,
        release: MinecraftVersion,
    ) -> ProjectileReaction {
        let is_owner = self.owner.map(|owner| owner.id) == Some(target.id);
        let hurt = if target.hurt {
            ProjectileReaction::Hurt
        } else {
            ProjectileReaction::Refused
        };
        match self.kind {
            // (1.15.2 ThrownEnderpearl.java:51; 1.16 ThrownEnderpearl.java:41)
            ProjectileKind::EnderPearl if is_owner && !release.at_least(1, 16, 0) => {
                ProjectileReaction::Passed
            }
            ProjectileKind::DragonFireball if is_owner => ProjectileReaction::Passed,
            ProjectileKind::LlamaSpit => {
                // (1.13.2 LlamaSpit.java:180-188; 1.14.4 LlamaSpit.java:124-132)
                self.removed = !release.at_least(1, 14, 0);
                if self.owner.is_some() {
                    hurt
                } else {
                    ProjectileReaction::Passed
                }
            }
            // in 1.8 only what Entity.hurt accepts (1.8.9 FishingBobberEntity tick, not in 1.9.4's)
            ProjectileKind::FishingHook if !target.hurt && !release.at_least(1, 9, 0) => hurt,
            // setting the tracked id calls onTrackedDataSet on the server too, which drops the
            // catch of 1.9 and 1.10: only a second hit on the same id, which sets nothing, holds
            // (1.9.4 FishingHook.java:134-143, 1.10.2 :135-144; 1.11.2 :129-136 keeps it)
            ProjectileKind::FishingHook
                if release.at_least(1, 9, 0)
                    && !release.at_least(1, 11, 0)
                    && self.hits.hook_tracked != Some(target.id) =>
            {
                self.hits.hook_tracked = Some(target.id);
                ProjectileReaction::Passed
            }
            ProjectileKind::FishingHook => {
                self.hits.hooked = Some(target.id);
                ProjectileReaction::Hooked
            }
            _ => {
                self.removed = true;
                hurt
            }
        }
    }

    /// onHitBlock: every family but two is gone. A rocket without explosions stays (1.16.5
    /// FireworkRocketEntity.java:213-221, 26.3 :220-228), and a fishing hook is its tick's business.
    fn hit_block(&mut self, block: BlockHit, _release: MinecraftVersion) {
        self.report.block_hit = Some(block.report());
        match self.kind {
            ProjectileKind::Firework => self.removed |= self.hits.explodes,
            ProjectileKind::FishingHook => {}
            _ => self.removed = true,
        }
    }

    fn record(
        &mut self,
        target: i32,
        location: Vec3,
        reaction: ProjectileReaction,
        damage: Option<i32>,
    ) {
        self.report.entity_hits.push(ProjectileEntityHit {
            target,
            location,
            reaction,
            damage,
        });
    }
}

/// The margin a target's box grows by: 0.3F until 1.21.5, from 1.21.6 ProjectileUtil.computeMargin,
/// which opens from nothing over the projectile's first ticks (1.21.5 ProjectileUtil.java:107, 1.21.6
/// :111, 26.3 :139-141).
pub(crate) fn target_margin(release: MinecraftVersion, tick_count: i32) -> f32 {
    if release.at_least(1, 21, 6) {
        (tick_count.wrapping_sub(2) as f32 / 20.0).min(TARGET_MARGIN).max(0.0)
    } else {
        TARGET_MARGIN
    }
}

/// AABB.clip, the six-face Box raycast before 1.13 (which also hits from inside): 1.12.2 AABB.java:290-323,
/// 1.13.2 AABB.java:242-255.
fn clip_box(bounds: Aabb, from: Vec3, to: Vec3, release: MinecraftVersion) -> Option<Vec3> {
    if release.at_least(1, 13, 0) {
        bounds.clip_segment(from, to)
    } else {
        bounds.clip_segment_legacy(from, to)
    }
}

/// The nearest of the candidates' inflated boxes along the ray. From 1.14 ProjectileUtil's strict
/// nearest (1.15 ProjectileUtil.java:117-129, 26.3 :143-160); before, nearest starts at 0 and any
/// hit replaces a nearest of 0, so a hit at the very start gives way to the next candidate (1.8.8
/// AbstractArrow.java:202-216, 1.13.2 AbstractArrow.java:425-437).
fn nearest_hit<'t>(
    candidates: impl Iterator<Item = (usize, &'t ProjectileTarget)>,
    from: Vec3,
    to: Vec3,
    margin: f32,
    release: MinecraftVersion,
) -> Option<Found> {
    let margin = f64::from(margin);
    let legacy = !release.at_least(1, 14, 0);
    let mut nearest = if legacy { 0.0 } else { f64::MAX };
    let mut found = None;
    for (index, target) in candidates {
        let bounds = target.bounding_box.inflate(Vec3::new(margin, margin, margin));
        if let Some(point) = clip_box(bounds, from, to, release) {
            let distance = from.distance_squared(point);
            if distance < nearest || (legacy && nearest == 0.0) {
                nearest = distance;
                found = Some((index, point));
            }
        }
    }
    found
}

/// ProjectileUtil.getManyEntityHitResult (1.21.11 ProjectileUtil.java:170-201, 26.3 :172-204):
/// every candidate the ray itself meets, and those only its inflated box meets when nothing blocks
/// the way from there to the box's centre. An arrow does not take a start inside a target. From
/// 26.3 an inflated hit stays on the inflated box (26.3 ProjectileUtil.java:195, AbstractArrow.java:586).
fn many_hits<'t>(
    candidates: impl Iterator<Item = (usize, &'t ProjectileTarget)>,
    from: Vec3,
    to: Vec3,
    margin: f32,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> Vec<Found> {
    let margin = f64::from(margin);
    let mut found = Vec::new();
    for (index, target) in candidates {
        let bounds = target.bounding_box;
        if let Some(point) = bounds.clip_segment(from, to) {
            found.push((index, point));
        } else if margin > 0.0 {
            let inflated = bounds.inflate(Vec3::new(margin, margin, margin));
            let Some(outside) = inflated.clip_segment(from, to) else {
                continue;
            };
            let center = (bounds.min + bounds.max).scale(0.5);
            let towards = clip_blocks(world, outside, center - outside, release, ClipShape::Collider)
                .map_or(center, |hit| hit.location);
            if let Some(surface) = bounds.clip_segment(outside, towards) {
                // (26.2 ProjectileUtil.java:195; 26.3 ProjectileUtil.java:195)
                found.push((index, if release.at_least(26, 3, 0) { outside } else { surface }));
            }
        }
    }
    found
}

/// AbstractArrow.deflect of 1.20.3 and 1.20.4: a turn by random.nextFloat() * 360 degrees at half
/// the speed (client1.20.3 AbstractArrow.java:412-417, Vec3.yRot's floats).
fn turned_by_random(velocity: Vec3, random: f32, release: MinecraftVersion) -> Vec3 {
    let angle = f64::from(random * 360.0 * 0.017453292_f32);
    let cos = f64::from(mth_cos(angle, release) as f32);
    let sin = f64::from(mth_sin(angle, release) as f32);
    Vec3::new(
        velocity.x * cos + velocity.z * sin,
        velocity.y,
        velocity.z * cos - velocity.x * sin,
    )
    .scale(0.5)
}

/// ThrownTrident.tick's return to the owner (1.13.2 ThrownTrident.java:57-92, 1.16.5 :64-99, 1.21.2
/// ThrownTrident.java:68-89, 26.3 :62-99). Returns whether the trident is gone: its owner is dead or
/// spectating, or from 1.21.2 a mob owner has it back. No owner, no return.
pub(crate) fn return_to_owner(
    position: &mut Vec3,
    velocity: &mut Vec3,
    no_physics: &mut bool,
    dealt_damage: bool,
    scene: &HitScene<'_>,
    release: MinecraftVersion,
) -> bool {
    let loyalty = f64::from(scene.hits.loyalty);
    let Some(owner) = scene.owner else {
        return false;
    };
    if loyalty <= 0.0 || !(dealt_damage || *no_physics) {
        return false;
    }
    if !owner.alive || owner.spectator {
        return true;
    }
    let to_eye = owner.eye_position - *position;
    // (1.21.1 ThrownTrident.java:63-70; 1.21.2 ThrownTrident.java:74)
    if release.at_least(1, 21, 2) && !owner.player && to_eye.length() < owner.width + 1.0 {
        return true;
    }
    *no_physics = true;
    position.y += to_eye.y * 0.015 * loyalty;
    let pull = to_eye.normalized(release);
    let accel = 0.05 * loyalty;
    // (1.13.2 ThrownTrident.java:79-83; 1.14.4 ThrownTrident.java:76-77)
    *velocity = if release.at_least(1, 14, 0) {
        velocity.scale(0.95) + pull.scale(accel)
    } else {
        // the motion takes the pull and loses 0.05 axis by axis (1.13.2 ThrownTrident.java:81-84)
        Vec3::new(
            velocity.x + (pull.x * accel - velocity.x * 0.05),
            velocity.y + (pull.y * accel - velocity.y * 0.05),
            velocity.z + (pull.z * accel - velocity.z * 0.05),
        )
    };
    false
}

#[cfg(test)]
#[path = "projectile_hits_tests.rs"]
mod tests;
