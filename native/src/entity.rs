use crate::math::{abs_max_for, Aabb, Vec3};
use crate::simulation::{climbable, SimulationState};
use crate::version::MinecraftVersion;
use crate::world::{EntityCollider, WorldSnapshot};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimensions {
    pub half_width: f64,
    pub height: f64,
}

impl Dimensions {
    #[must_use]
    pub const fn new(half_width: f64, height: f64) -> Self {
        Self { half_width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityFlags {
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub vertical_collision_below: bool,
    pub in_water: bool,
    pub in_lava: bool,
    pub climbing: bool,
}

/// An entity's motion state as a prediction clones and merges it: where it is and moves, its
/// size and contact flags, and its own gravity: the synced NoGravity flag (Entity.isNoGravity,
/// 1.10+) and the GRAVITY and STEP_HEIGHT attribute values (1.20.5+), None for the profile's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub dimensions: Dimensions,
    pub flags: EntityFlags,
    pub no_gravity: bool,
    pub gravity: Option<f64>,
    pub step_height: Option<f64>,
}

impl EntityState {
    /// The box around the position, as wide as the dimensions and as tall (vanilla:
    /// EntityDimensions#makeBoundingBox).
    #[must_use]
    pub fn bounds(self) -> Aabb {
        Aabb::new(
            Vec3::new(
                self.position.x - self.dimensions.half_width,
                self.position.y,
                self.position.z - self.dimensions.half_width,
            ),
            Vec3::new(
                self.position.x + self.dimensions.half_width,
                self.position.y + self.dimensions.height,
                self.position.z + self.dimensions.half_width,
            ),
        )
    }
}

/// Entity#push with no pushthrough: entity_push_impulses_with_pushthrough with 0.
#[must_use]
pub fn entity_push_impulses(
    left_position: Vec3,
    right_position: Vec3,
    left_no_physics: bool,
    right_no_physics: bool,
    passenger_of_same_vehicle: bool,
    left_vehicle: bool,
    right_vehicle: bool,
    left_pushable: bool,
    right_pushable: bool,
    release: MinecraftVersion,
) -> (Vec3, Vec3) {
    entity_push_impulses_with_pushthrough(
        left_position,
        right_position,
        left_no_physics,
        right_no_physics,
        passenger_of_same_vehicle,
        left_vehicle,
        right_vehicle,
        left_pushable,
        right_pushable,
        0.0,
        release,
    )
}

/// Entity.push(Entity) with the left entity as `this`: the impulses the left and the right entity
/// take. Before 1.17 the left entity's `(1.0F - this.pushthrough)` scales both (1.8.8
/// Entity.java:1043-1044, 1.16.5 :1309-1310); 1.17 removed it (:1262-1291), so `pushthrough` is
/// ignored from there on, whatever the caller holds. Before 1.17 too the square root is a
/// float's (Mth.sqrt, see push_impulse) and nothing pushes closer than 0.01F. A side `vehicle`
/// has passengers (isVehicle: 1.8.9 `rider == null`, 1.16.5 hasPassengers) and takes nothing; from
/// 1.19 a side that is not pushable takes nothing either (1.19 Entity.java:1301-1307; 1.18 :1267
/// has no isPushable, 26.3 :1931-1937).
#[must_use]
pub fn entity_push_impulses_with_pushthrough(
    left_position: Vec3,
    right_position: Vec3,
    left_no_physics: bool,
    right_no_physics: bool,
    passenger_of_same_vehicle: bool,
    left_vehicle: bool,
    right_vehicle: bool,
    left_pushable: bool,
    right_pushable: bool,
    pushthrough: f64,
    release: MinecraftVersion,
) -> (Vec3, Vec3) {
    // Entity.push(Entity) skips entities that are passengers of the same vehicle.
    if passenger_of_same_vehicle
        || left_no_physics
        || right_no_physics
        || !left_position.is_finite()
        || !right_position.is_finite()
    {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let Some((mut x, mut z)) = push_impulse(left_position, right_position, release) else {
        return (Vec3::ZERO, Vec3::ZERO);
    };
    // (1.16.5 Entity.java:1306-1307; 1.17 Entity.java:1279-1281)
    if !release.at_least(1, 17, 0) {
        let pushthrough_scale = f64::from(1.0_f32 - pushthrough as f32);
        x *= pushthrough_scale;
        z *= pushthrough_scale;
    }
    // (1.18.2 Entity.java:1270; 1.19 Entity.java:1316)
    let gated = release.at_least(1, 19, 0);
    let left_impulse = if !left_vehicle && (!gated || left_pushable) {
        Vec3::new(-x, 0.0, -z)
    } else {
        Vec3::ZERO
    };
    let right_impulse = if !right_vehicle && (!gated || right_pushable) {
        Vec3::new(x, 0.0, z)
    } else {
        Vec3::ZERO
    };
    (left_impulse, right_impulse)
}

/// The vanilla class behind an entity near the player, which decides how it pushes the player
/// and whether the player's move collides with it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NearbyEntityKind {
    /// LivingEntity.pushEntities: pushes a player whose box meets its own.
    Living,
    /// Boat.tick: pushes a player whose box meets its own grown by 0.2, -0.01, 0.2 while the
    /// player's feet are not above its own; a collider (Boat.canBeCollidedWith).
    Boat,
    /// A living entity that is a collider (Shulker.canBeCollidedWith).
    Shulker,
    /// A living entity that is a collider to a player whose feet are at or above its top
    /// (HappyGhast.canBeCollidedWith on the client).
    HappyGhast,
    /// An adult happy ghast on its still timeout (the synced STAYS_STILL): a collider from every
    /// side (HappyGhast.canBeCollidedWith: isOnStillTimeout, 1.21.6 HappyGhast.java:504-513,
    /// 1.21.11 :511-520, 26.3 :522-531).
    HappyGhastStaysStill,
    /// A living entity that neither pushes nor collides: the armor stand (ArmorStand.isPushable,
    /// ArmorStand.pushEntities, which pushes minecarts only), which still bounces a riptide spin
    /// (an ArmorStand is a LivingEntity: 1.13.2 entity/decoration/ArmorStand.java:40, 1.21.4
    /// :49).
    StillLiving,
    /// A firework rocket attached to the player: in its turn of the client's tick it boosts a
    /// gliding player (FireworkRocketEntity.tick, 1.11.2 FireworkRocketEntity.java:113-120, 1.21.11
    /// :121-126), between the pushes of the entities that tick before and after it. Neither
    /// pushes nor collides.
    AttachedRocket,
    /// Neither pushes nor collides: projectiles, items, rockets.
    /// A minecart: pushes no client's player, but is pushable, so a boat collides with it.
    Minecart,
    Inert,
}

impl NearbyEntityKind {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "living" => Self::Living,
            "boat" => Self::Boat,
            "shulker" => Self::Shulker,
            "happy_ghast" => Self::HappyGhast,
            "happy_ghast_stays_still" => Self::HappyGhastStaysStill,
            "still_living" => Self::StillLiving,
            "attached_rocket" => Self::AttachedRocket,
            "minecart" => Self::Minecart,
            _ => Self::Inert,
        }
    }

    /// The kind's answer to canBeCollidedWith as a collider at `bounds` (vanilla:
    /// AbstractBoat#canBeCollidedWith, Shulker#canBeCollidedWith, HappyGhast#canBeCollidedWith).
    fn collider(self, bounds: Aabb) -> Option<EntityCollider> {
        match self {
            Self::Boat | Self::Shulker | Self::HappyGhastStaysStill => Some(EntityCollider {
                bounds,
                only_from_above: false,
            }),
            Self::HappyGhast => Some(EntityCollider {
                bounds,
                only_from_above: true,
            }),
            Self::Living | Self::StillLiving | Self::AttachedRocket | Self::Minecart | Self::Inert => None,
        }
    }
}

/// An entity near the local player during one client tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NearbyEntity {
    pub kind: NearbyEntityKind,
    /// Where the player's move meets it.
    pub position: Vec3,
    pub bounding_box: Aabb,
    /// Where its own tick leaves it, from where it pushes the player.
    pub tick_position: Vec3,
    pub tick_bounding_box: Aabb,
    /// The client ticks it before the player (1.14 - 1.16 tick a hash map of entities).
    pub ticks_before_player: bool,
    /// The team collision rules let it push the player (EntitySelector.pushableBy: neither the
    /// pusher's nor the player's rule is never, nor pushOwnTeam between allies, nor
    /// pushOtherTeams between others; 1.9.4 EntitySelector.java:54-82, 1.16.5
    /// EntitySelector.java:27-54, 1.21.11 EntitySelector.java:40-72).
    pub team_pushes: bool,
    /// A spectating player: no LivingEntity to the riptide spin, which getEntities' NO_SPECTATORS
    /// leaves out of its sweep (26.3 LivingEntity.java:3188-3195, EntityGetter.java:29-31,
    /// EntitySelector.java:26; 1.13.2 class_3593.java:18-19).
    pub spectator: bool,
}

/// LivingEntity.checkAutoSpinAttack (1.13+), right after the move of a player in the riptide spin:
/// the first living entity in the box swept from `before` to the box after the move stops the spin
/// and bounces the player back at -0.2 times its velocity; touching no entity, the spin stops on a
/// wall (1.13.2 LivingEntity.java:2018-2035, 1.16.5 :2574-2589, 1.21.11 :3090-3104). Returns
/// whether the spin ends.
pub fn check_auto_spin_attack(
    state: &mut SimulationState,
    before: Aabb,
    entities: &[NearbyEntity],
) -> bool {
    let swept = before.minmax(state.bounds());
    let mut touched = entities
        .iter()
        .filter(|entity| !entity.spectator && entity.bounding_box.intersects(swept))
        .peekable();
    if touched.peek().is_none() {
        return state.collided_horizontally;
    }
    let living = touched.any(|entity| {
        matches!(
            entity.kind,
            NearbyEntityKind::Living
                | NearbyEntityKind::Shulker
                | NearbyEntityKind::HappyGhast
                | NearbyEntityKind::HappyGhastStaysStill
                | NearbyEntityKind::StillLiving
        )
    });
    if living {
        state.velocity = state.velocity.scale(-0.2);
    }
    living
}

/// The colliders among the entities near the player: where the player's move meets them, or
/// where their own ticks leave them (for what moves the player after every entity ticked).
/// (vanilla: EntityGetter#getEntityCollisions, the entities that can be collided with)
#[must_use]
pub fn nearby_entity_colliders(
    entities: &[NearbyEntity],
    after_their_ticks: bool,
) -> Vec<EntityCollider> {
    entities
        .iter()
        .filter_map(|entity| {
            // (what ticks before the player has moved by the time the player does)
            let bounds = if after_their_ticks || entity.ticks_before_player {
                entity.tick_bounding_box
            } else {
                entity.bounding_box
            };
            entity.kind.collider(bounds)
        })
        .collect()
}

/// The colliders among the entities near a vehicle its player rides, the vehicle and those sharing
/// it left out by the caller. A mob collides as the player does (Entity.canCollideWith: 26.3
/// Entity.java:2457-2459). A boat also collides with whatever is pushable (1.11.2
/// Boat.getCollideAgainstBox, 1.16.5 Boat.java:155-161, 26.3 AbstractBoat.java:111-117),
/// and before 1.11 with every entity (1.8.8 Boat.java:110-112, 1.10.2
/// Boat.getCollideAgainstBox).
#[must_use]
pub fn vehicle_entity_colliders(
    entities: &[NearbyEntity],
    boat: bool,
    release: MinecraftVersion,
) -> Vec<EntityCollider> {
    entities
        .iter()
        .filter_map(|entity| {
            let bounds = if entity.ticks_before_player {
                entity.tick_bounding_box
            } else {
                entity.bounding_box
            };
            let hard = boat
                && match entity.kind {
                    NearbyEntityKind::Living
                    | NearbyEntityKind::HappyGhast
                    | NearbyEntityKind::Minecart => true,
                    NearbyEntityKind::StillLiving | NearbyEntityKind::Inert => {
                        // (1.10.2 Boat.java:133-135; 1.11.2 Boat.java:133-135)
                        !release.at_least(1, 11, 0)
                    }
                    _ => false,
                };
            if hard {
                Some(EntityCollider {
                    bounds,
                    only_from_above: false,
                })
            } else {
                entity.kind.collider(bounds)
            }
        })
        .collect()
}

/// The push a nearby entity took back from the player it pushed (Entity.push: the other way), by
/// its index among the nearby entities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityPush {
    pub index: usize,
    pub x: f64,
    pub z: f64,
}

/// The pushes of the entities that tick before the local player (or after it), in the order the
/// client ticks them. From 1.9 the client's entities push the local player alone
/// (EntitySelector.pushableBy) when the team rules let them and it is pushable: not a spectator,
/// not removed, and from 1.11 alive and not on a climbable (LivingEntity.isPushable: `!removed`
/// in 1.9.4 LivingEntity.java:1846 and 1.10.2 :1882, isAlive() && !isClimbing() in 1.11.2 :1942,
/// 1.21.11 :3202). A living pusher's push (doPush -> LivingEntity.push(Entity)) skips a sleeping
/// player (1.9.4 Player.java:1867, 1.16.5 LivingEntity.java:1910, 1.21.11 :2188); a
/// boat's (Boat.push -> Entity.push) does not. The attached rockets among them boost the player in
/// their turns (each entity runs its whole tick in the client's order: ClientLevel.tickEntities).
/// Returns the pushes they took back.
pub fn push_by_nearby_entities(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    entities: &[NearbyEntity],
    before_player: bool,
    release: MinecraftVersion,
) -> Vec<EntityPush> {
    let mut pushes = Vec::new();
    let pushable = release.at_least(1, 9, 0)
        && !(state.is_player && state.no_physics)
        && !state.removed
        // (1.10.2 LivingEntity.java:1882-1884; 1.11.2 LivingEntity.java:1932-1934)
        && !(release.at_least(1, 11, 0) && (state.dead || climbable(world, state, release)));
    // Entity.push moves each side only if it carries no passengers: the other is still pushed
    // (1.8.9 Entity.java has no such test; 1.9.4 Entity.java:1164-1169, 26.3 :1931-1937)
    let moves_player = !(release.at_least(1, 9, 0) && state.is_vehicle);
    for (index, entity) in entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.ticks_before_player == before_player)
    {
        if entity.kind == NearbyEntityKind::AttachedRocket {
            boost_by_attached_rockets(state, 1, release);
            continue;
        }
        if !pushable || !entity.team_pushes {
            continue;
        }
        let player_box = state.bounds();
        let other_box = entity.tick_bounding_box;
        match entity.kind {
            NearbyEntityKind::Boat => {
                let reach = other_box.inflate(Vec3::new(
                    f64::from(0.2_f32),
                    f64::from(-0.01_f32),
                    f64::from(0.2_f32),
                ));
                if !player_box.intersects(reach) || player_box.min.y > other_box.min.y {
                    continue;
                }
                if let Some((x, z)) = push_impulse(entity.tick_position, state.position, release) {
                    if moves_player {
                        state.velocity = state.velocity + Vec3::new(x, 0.0, z);
                    }
                    pushes.push(EntityPush {
                        index,
                        x: -x,
                        z: -z,
                    });
                }
            }
            NearbyEntityKind::Living
            | NearbyEntityKind::Shulker
            | NearbyEntityKind::HappyGhast
            | NearbyEntityKind::HappyGhastStaysStill => {
                if state.sleeping || !player_box.intersects(other_box) {
                    continue;
                }
                if let Some((x, z)) = push_impulse(state.position, entity.tick_position, release) {
                    if moves_player {
                        state.velocity = state.velocity + Vec3::new(-x, 0.0, -z);
                    }
                    pushes.push(EntityPush { index, x, z });
                }
            }
            NearbyEntityKind::StillLiving
            | NearbyEntityKind::AttachedRocket
            | NearbyEntityKind::Minecart
            | NearbyEntityKind::Inert => {}
        }
    }
    pushes
}

/// Entity.push(Entity) from `from` toward `to`: what `to` gets, 0.05 away from `from`, nothing
/// when they are 0.01F or less apart. The square root is a float's before 1.17 (Mth.sqrt).
fn push_impulse(from: Vec3, to: Vec3, release: MinecraftVersion) -> Option<(f64, f64)> {
    let mut x = to.x - from.x;
    let mut z = to.z - from.z;
    let distance = abs_max_for(x, z, release);
    if !(distance >= f64::from(0.01_f32)) {
        return None;
    }
    // (1.16.5 Entity.java:1294; 1.17 Entity.java:1263)
    let distance = if release.at_least(1, 17, 0) {
        distance.sqrt()
    } else {
        f64::from(distance.sqrt() as f32)
    };
    x /= distance;
    z /= distance;
    let scale = (1.0 / distance).min(1.0);
    x *= scale;
    z *= scale;
    x *= f64::from(0.05_f32);
    z *= f64::from(0.05_f32);
    Some((x, z))
}

/// FireworkRocketEntity.tick of the rockets attached to the player (from 1.11): each pulls a
/// gliding player toward 1.5 times its look vector.
pub fn boost_by_attached_rockets(
    state: &mut SimulationState,
    rockets: u32,
    release: MinecraftVersion,
) {
    // (1.10.2 FireworkRocketEntity.java:99; 1.11.2 FireworkRocketEntity.java:118-125)
    let look = state.look;
    boost_along(state, rockets, look, release);
}

/// The attached rockets that tick before the player: in 1.11 a rocket boosts along
/// LivingEntity.getLookAngle, getViewVector(1.0F) of the head yaw (1.11.2 LivingEntity.java:1917-1925,
/// FireworkRocketEntity.java:120), which the player's last aiStep set to its yaw then
/// (Player.serverAiStep, 1.11.2 Player.java:468-472): the state's head_look where the caller gives
/// it (D-222). From 1.12 Entity.getLookAngle reads the yaw itself (1.12.2 Entity.java:1729-1731).
pub fn boost_by_attached_rockets_before_player(
    state: &mut SimulationState,
    rockets: u32,
    release: MinecraftVersion,
) {
    let head = release.at_least(1, 11, 0) && !release.at_least(1, 12, 0);
    let look = state.head_look.filter(|_| head).unwrap_or(state.look);
    boost_along(state, rockets, look, release);
}

fn boost_along(state: &mut SimulationState, rockets: u32, look: Vec3, release: MinecraftVersion) {
    // (1.10.2 FireworkRocketEntity.java:96 has no attached entity; 1.11.2 FireworkRocketEntity.java:118-125)
    if !release.at_least(1, 11, 0) || !state.fall_flying {
        return;
    }
    for _ in 0..rockets {
        let movement = state.velocity;
        state.velocity = movement
            + Vec3::new(
                look.x * 0.1 + (look.x * 1.5 - movement.x) * 0.5,
                look.y * 0.1 + (look.y * 1.5 - movement.y) * 0.5,
                look.z * 0.1 + (look.z * 1.5 - movement.z) * 0.5,
            );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        boost_by_attached_rockets, entity_push_impulses, entity_push_impulses_with_pushthrough,
        nearby_entity_colliders, push_by_nearby_entities, vehicle_entity_colliders, Dimensions,
        EntityState, NearbyEntity, NearbyEntityKind,
    };
    use crate::math::{Aabb, Vec3};
    use crate::simulation::{PhysicsConfig, SimulationState};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockEffect, BlockShape, BlockState, EntityCollider, WorldSnapshot};

    fn player_at(x: f64, z: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(x, 64.0, z),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            ..SimulationState::default()
        }
    }

    // D-197: getEntities(this, minmax) is NO_SPECTATORS (26.3 LivingEntity.java:3188-3195,
    // EntityGetter.java:29-31): a spectator alone in the sweep leaves the list empty, so a wall
    // still ends the spin, where any other entity there, living or not, keeps it going
    #[test]
    fn a_spectator_in_the_riptide_sweep_leaves_the_wall_to_end_the_spin() {
        let mut state = player_at(0.0, 0.0);
        state.collided_horizontally = true;
        state.velocity = Vec3::new(1.0, 0.0, 0.0);
        let before = state.bounds();
        let mut spectator = entity_at(NearbyEntityKind::Inert, 0.2, 0.0, 0.3);
        spectator.spectator = true;
        let mut walled = state.clone();
        assert!(super::check_auto_spin_attack(&mut walled, before, &[spectator]));
        assert_eq!(walled.velocity, Vec3::new(1.0, 0.0, 0.0));
        let mut kept = state.clone();
        assert!(!super::check_auto_spin_attack(&mut kept, before, &[entity_at(NearbyEntityKind::Inert, 0.2, 0.0, 0.3)]));
        let mut living = spectator;
        living.kind = NearbyEntityKind::Living;
        let mut bounced = state.clone();
        assert!(super::check_auto_spin_attack(&mut bounced, before, &[living]));
        assert_eq!(bounced.velocity, Vec3::new(1.0, 0.0, 0.0));
    }

    fn entity_at(kind: NearbyEntityKind, x: f64, z: f64, half_width: f64) -> NearbyEntity {
        let position = Vec3::new(x, 64.0, z);
        let bounding_box = Aabb::new(
            Vec3::new(x - half_width, 64.0, z - half_width),
            Vec3::new(x + half_width, 65.4, z + half_width),
        );
        NearbyEntity {
            kind,
            position,
            bounding_box,
            tick_position: position,
            tick_bounding_box: bounding_box,
            ticks_before_player: false,
            team_pushes: true,
            spectator: false,
        }
    }

    #[test]
    fn a_boat_collides_with_what_is_pushable_and_a_mob_with_what_collides() {
        // 26.3 AbstractBoat.java:111-117 and Entity.java:2457-2459; everything up to 1.10
        // (1.10.2 Boat.getCollideAgainstBox), the pushable from 1.11
        let colliders = |kind, boat, release| {
            vehicle_entity_colliders(&[entity_at(kind, 2.0, 0.0, 0.3)], boat, release).len()
        };
        let old = MinecraftVersion::parse("1.10.2").unwrap();
        let new = MinecraftVersion::parse("1.11.2").unwrap();
        for kind in [
            NearbyEntityKind::Living,
            NearbyEntityKind::Minecart,
            NearbyEntityKind::Boat,
            NearbyEntityKind::Shulker,
        ] {
            assert_eq!(colliders(kind, true, new), 1, "{kind:?}");
        }
        assert_eq!(colliders(NearbyEntityKind::StillLiving, true, new), 0);
        assert_eq!(colliders(NearbyEntityKind::Inert, true, new), 0);
        assert_eq!(colliders(NearbyEntityKind::StillLiving, true, old), 1);
        assert_eq!(colliders(NearbyEntityKind::Inert, true, old), 1);
        assert_eq!(colliders(NearbyEntityKind::Living, false, new), 0);
        assert_eq!(colliders(NearbyEntityKind::Minecart, false, new), 0);
        assert_eq!(colliders(NearbyEntityKind::Boat, false, new), 1);
    }

    #[test]
    fn a_mob_meeting_the_player_pushes_it_away_from_its_centre() {
        let world = WorldSnapshot::new();
        let mut state = player_at(0.0, 0.0);
        let cow = entity_at(NearbyEntityKind::Living, 0.7, 0.25, 0.45);
        push_by_nearby_entities(
            &mut state,
            &world,
            &[cow],
            false,
            MinecraftVersion::new(1, 21, 1),
        );
        let distance = 0.7_f64.sqrt();
        let impulse = |offset: f64| offset / distance * 1.0 * f64::from(0.05_f32);
        assert_eq!(
            state.velocity,
            Vec3::new(-impulse(0.7), 0.0, -impulse(0.25))
        );
    }

    #[test]
    fn before_1_17_the_push_distance_is_a_float_square_root() {
        let world = WorldSnapshot::new();
        let mut state = player_at(0.0, 0.0);
        let cow = entity_at(NearbyEntityKind::Living, 0.7, 0.0, 0.45);
        push_by_nearby_entities(
            &mut state,
            &world,
            &[cow],
            false,
            MinecraftVersion::new(1, 16, 5),
        );
        let distance = f64::from(0.7_f64.sqrt() as f32);
        let expected = -(0.7 / distance * (1.0 / distance).min(1.0) * f64::from(0.05_f32));
        assert_eq!(state.velocity.x, expected);
        assert_ne!(state.velocity.x, -(0.7_f64.sqrt() * f64::from(0.05_f32)));
    }

    #[test]
    fn mobs_push_only_from_1_9_and_only_when_their_boxes_meet() {
        let world = WorldSnapshot::new();
        let cow = entity_at(NearbyEntityKind::Living, 0.7, 0.0, 0.45);
        let far_cow = entity_at(NearbyEntityKind::Living, 0.8, 0.0, 0.45);
        let mut legacy = player_at(0.0, 0.0);
        push_by_nearby_entities(
            &mut legacy,
            &world,
            &[cow],
            false,
            MinecraftVersion::new(1, 8, 8),
        );
        assert_eq!(legacy.velocity, Vec3::ZERO);
        let mut apart = player_at(0.0, 0.0);
        push_by_nearby_entities(
            &mut apart,
            &world,
            &[far_cow, entity_at(NearbyEntityKind::Inert, 0.0, 0.0, 0.3)],
            false,
            MinecraftVersion::new(1, 9, 4),
        );
        assert_eq!(apart.velocity, Vec3::ZERO);
    }

    #[test]
    fn only_the_entities_on_the_requested_side_of_the_player_tick_push() {
        let world = WorldSnapshot::new();
        let mut cow = entity_at(NearbyEntityKind::Living, 0.7, 0.0, 0.45);
        cow.ticks_before_player = true;
        let mut state = player_at(0.0, 0.0);
        let release = MinecraftVersion::new(1, 16, 5);
        push_by_nearby_entities(&mut state, &world, &[cow], false, release);
        assert_eq!(state.velocity, Vec3::ZERO);
        push_by_nearby_entities(&mut state, &world, &[cow], true, release);
        assert!(state.velocity.x < 0.0);
    }

    #[test]
    fn an_entity_pushes_from_where_its_own_tick_left_it() {
        let world = WorldSnapshot::new();
        let mut cow = entity_at(NearbyEntityKind::Living, 2.0, 0.0, 0.45);
        let ticked = entity_at(NearbyEntityKind::Living, 0.5, 0.0, 0.45);
        cow.tick_position = ticked.position;
        cow.tick_bounding_box = ticked.bounding_box;
        let mut state = player_at(0.0, 0.0);
        push_by_nearby_entities(
            &mut state,
            &world,
            &[cow],
            false,
            MinecraftVersion::new(1, 21, 1),
        );
        let distance = 0.5_f64.sqrt();
        assert_eq!(state.velocity.x, -(0.5 / distance * f64::from(0.05_f32)));
    }

    #[test]
    fn a_boat_pushes_a_player_within_its_grown_box_unless_the_player_is_above_it() {
        let world = WorldSnapshot::new();
        let boat = |y: f64| {
            let position = Vec3::new(1.0, y, 0.0);
            let bounding_box = Aabb::new(
                Vec3::new(0.3125, y, -0.6875),
                Vec3::new(1.6875, y + 0.5625, 0.6875),
            );
            NearbyEntity {
                kind: NearbyEntityKind::Boat,
                position,
                bounding_box,
                tick_position: position,
                tick_bounding_box: bounding_box,
                ticks_before_player: false,
                team_pushes: true,
                spectator: false,
            }
        };
        let release = MinecraftVersion::new(1, 21, 1);
        let mut beside = player_at(0.0, 0.0);
        push_by_nearby_entities(&mut beside, &world, &[boat(64.0)], false, release);
        assert_eq!(beside.velocity, Vec3::new(-f64::from(0.05_f32), 0.0, 0.0));
        let mut above = player_at(0.0, 0.0);
        push_by_nearby_entities(&mut above, &world, &[boat(63.9)], false, release);
        assert_eq!(above.velocity, Vec3::ZERO);
    }

    #[test]
    fn a_boat_push_adds_a_positive_zero_where_a_mob_push_adds_a_negative_one() {
        let world = WorldSnapshot::new();
        let release = MinecraftVersion::new(1, 21, 1);
        let mut boat = entity_at(NearbyEntityKind::Boat, 0.5, 0.0, 0.6875);
        boat.bounding_box.max.y = 64.5625;
        boat.tick_bounding_box = boat.bounding_box;
        let mut pushed_by_boat = player_at(0.0, 0.0);
        pushed_by_boat.velocity = Vec3::new(0.0, 0.0, -0.0);
        push_by_nearby_entities(&mut pushed_by_boat, &world, &[boat], false, release);
        assert!(pushed_by_boat.velocity.z.is_sign_positive());
        let mut pushed_by_cow = player_at(0.0, 0.0);
        pushed_by_cow.velocity = Vec3::new(0.0, 0.0, -0.0);
        let cow = entity_at(NearbyEntityKind::Living, 0.5, 0.0, 0.45);
        push_by_nearby_entities(&mut pushed_by_cow, &world, &[cow], false, release);
        assert!(pushed_by_cow.velocity.z.is_sign_negative());
    }

    #[test]
    fn a_player_on_a_climbable_or_a_spectator_is_not_pushed() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 64, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.0, 0.0, 0.8125), Vec3::new(1.0, 1.0, 1.0)),
                    effect: BlockEffect::Climbable,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let cow = entity_at(NearbyEntityKind::Living, 1.0, 0.5, 0.45);
        let release = MinecraftVersion::new(1, 21, 1);
        let mut climbing = player_at(0.5, 0.5);
        push_by_nearby_entities(&mut climbing, &world, &[cow], false, release);
        assert_eq!(climbing.velocity, Vec3::ZERO);
        let mut spectator = player_at(2.0, 0.5);
        spectator.no_physics = true;
        let close = entity_at(NearbyEntityKind::Living, 2.5, 0.5, 0.45);
        push_by_nearby_entities(&mut spectator, &world, &[close], false, release);
        assert_eq!(spectator.velocity, Vec3::ZERO);
        // 1.9.4 - 1.10.2 LivingEntity.isPushable is `!removed` alone (1.9.4 LivingEntity.java:1846,
        // 1.10.2 :1882): a climbing player is pushed; 1.11.2 :1942 adds !isClimbing()
        for (release, pushed) in [((1, 10, 2), true), ((1, 11, 2), false)] {
            let mut climbing = player_at(0.5, 0.5);
            let release = MinecraftVersion::new(release.0, release.1, release.2);
            push_by_nearby_entities(&mut climbing, &world, &[cow], false, release);
            assert_eq!(climbing.velocity != Vec3::ZERO, pushed, "{release:?}");
        }
    }

    #[test]
    fn a_player_carrying_passengers_is_not_pushed_but_still_pushes_from_1_9() {
        // Entity.push moves each side only where it is no vehicle (1.9.4 Entity.java:1164-1169,
        // 26.3 :1931-1937): a mob or a boat that meets the player is pushed, the player is not (D-206)
        let world = WorldSnapshot::new();
        let cow = entity_at(NearbyEntityKind::Living, 0.5, 0.0, 0.45);
        let boat = entity_at(NearbyEntityKind::Boat, 0.5, 0.0, 0.6875);
        for other in [cow, boat] {
            for (release, carries, moved) in [
                (MinecraftVersion::new(1, 21, 11), false, true),
                (MinecraftVersion::new(1, 21, 11), true, false),
                (MinecraftVersion::new(1, 9, 4), true, false),
            ] {
                let mut player = player_at(0.0, 0.0);
                player.is_vehicle = carries;
                let pushes = push_by_nearby_entities(&mut player, &world, &[other], false, release);
                assert_eq!(pushes.len(), 1, "{release:?} {:?}", other.kind);
                assert_eq!(player.velocity != Vec3::ZERO, moved, "{release:?} {:?}", other.kind);
            }
        }
    }

    #[test]
    fn a_dead_or_removed_player_is_not_pushed() {
        // LivingEntity.isPushable: isAlive() from 1.11 (1.11.2 LivingEntity.java:1942, isAlive
        // :1070 `!removed && getHealth() > 0.0F`; 1.21.11 :3202); before, `!removed` alone
        // (1.9.4 :1846)
        let world = WorldSnapshot::new();
        let cow = entity_at(NearbyEntityKind::Living, 0.5, 0.0, 0.45);
        for (release, dead_pushed) in [
            ((1, 10, 2), true),
            ((1, 11, 2), false),
            ((1, 21, 11), false),
        ] {
            let release = MinecraftVersion::new(release.0, release.1, release.2);
            let mut dead = player_at(0.0, 0.0);
            dead.dead = true;
            push_by_nearby_entities(&mut dead, &world, &[cow], false, release);
            assert_eq!(dead.velocity != Vec3::ZERO, dead_pushed, "{release:?}");
            let mut removed = player_at(0.0, 0.0);
            removed.removed = true;
            assert!(
                push_by_nearby_entities(&mut removed, &world, &[cow], false, release).is_empty()
            );
            assert_eq!(removed.velocity, Vec3::ZERO);
        }
    }

    #[test]
    fn a_sleeping_player_is_pushed_by_a_boat_but_not_by_a_mob() {
        // a mob's doPush is player.push(mob), LivingEntity.push(Entity): `if (!isSleeping())`
        // (1.9.4 Player.java:1867, 1.16.5 LivingEntity.java:1910, 1.21.11 :2188); a boat's
        // is boat.push(player), Entity.push (1.21.11 AbstractBoat.java:167-175, 267)
        let world = WorldSnapshot::new();
        let release = MinecraftVersion::new(1, 21, 11);
        let mut sleeper = player_at(0.0, 0.0);
        sleeper.sleeping = true;
        let cow = entity_at(NearbyEntityKind::Living, 0.5, 0.0, 0.45);
        let ghast = entity_at(NearbyEntityKind::HappyGhastStaysStill, 0.5, 0.0, 2.0);
        assert!(
            push_by_nearby_entities(&mut sleeper, &world, &[cow, ghast], false, release).is_empty()
        );
        assert_eq!(sleeper.velocity, Vec3::ZERO);
        let boat = entity_at(NearbyEntityKind::Boat, 0.5, 0.0, 0.6875);
        let pushes = push_by_nearby_entities(&mut sleeper, &world, &[boat], false, release);
        assert_eq!(pushes.len(), 1);
        assert!(sleeper.velocity.x < 0.0);
    }

    #[test]
    fn an_entity_the_team_rules_keep_from_pushing_neither_pushes_nor_is_pushed_back() {
        // EntitySelector.pushableBy (1.21.11 EntitySelector.java:40-72): no push either way
        let world = WorldSnapshot::new();
        let release = MinecraftVersion::new(1, 21, 11);
        for kind in [NearbyEntityKind::Living, NearbyEntityKind::Boat] {
            let mut entity = entity_at(kind, 0.5, 0.0, 0.6875);
            entity.team_pushes = false;
            let mut state = player_at(0.0, 0.0);
            assert!(
                push_by_nearby_entities(&mut state, &world, &[entity], false, release).is_empty()
            );
            assert_eq!(state.velocity, Vec3::ZERO);
            entity.team_pushes = true;
            assert_eq!(
                push_by_nearby_entities(&mut state, &world, &[entity], false, release).len(),
                1
            );
        }
        // a boat the team rules keep from pushing is still a collider
        let mut boat = entity_at(NearbyEntityKind::Boat, 0.5, 0.0, 0.6875);
        boat.team_pushes = false;
        assert_eq!(nearby_entity_colliders(&[boat], false).len(), 1);
    }

    #[test]
    fn boats_shulkers_and_happy_ghasts_collide_and_mobs_do_not() {
        let colliders = nearby_entity_colliders(
            &[
                entity_at(NearbyEntityKind::Boat, 0.0, 0.0, 0.6875),
                entity_at(NearbyEntityKind::Shulker, 2.0, 0.0, 0.5),
                entity_at(NearbyEntityKind::HappyGhast, 6.0, 0.0, 2.0),
                entity_at(NearbyEntityKind::Living, 9.0, 0.0, 0.45),
                entity_at(NearbyEntityKind::Inert, 9.0, 0.0, 0.125),
                // on its still timeout a happy ghast collides from every side (1.21.11
                // HappyGhast.java:511-520: isOnStillTimeout, the synced STAYS_STILL :487-488)
                entity_at(NearbyEntityKind::HappyGhastStaysStill, 12.0, 0.0, 2.0),
                entity_at(NearbyEntityKind::StillLiving, 15.0, 0.0, 0.25),
            ],
            false,
        );
        let flags: Vec<bool> = colliders
            .iter()
            .map(|collider| collider.only_from_above)
            .collect();
        assert_eq!(flags, vec![false, false, true, false]);
    }

    #[test]
    fn what_ticks_before_the_player_meets_its_move_where_that_tick_left_it() {
        let mut boat = entity_at(NearbyEntityKind::Boat, 2.0, 0.0, 0.45);
        let ticked = entity_at(NearbyEntityKind::Boat, 0.5, 0.0, 0.45);
        boat.tick_position = ticked.position;
        boat.tick_bounding_box = ticked.bounding_box;
        let later = boat;

        boat.ticks_before_player = true;
        assert_eq!(
            nearby_entity_colliders(&[boat], false),
            nearby_entity_colliders(&[boat], true)
        );
        assert_ne!(
            nearby_entity_colliders(&[later], false),
            nearby_entity_colliders(&[later], true)
        );
    }

    #[test]
    fn after_every_entity_ticked_the_colliders_are_where_their_ticks_left_them() {
        let mut boat = entity_at(NearbyEntityKind::Boat, 0.0, 0.0, 0.6875);
        boat.tick_bounding_box = boat.bounding_box.translated(Vec3::new(0.25, 0.0, 0.0));
        let during_move = nearby_entity_colliders(&[boat], false);
        let after_ticks = nearby_entity_colliders(&[boat], true);
        assert_eq!(during_move[0].bounds, boat.bounding_box);
        assert_eq!(after_ticks[0].bounds, boat.tick_bounding_box);
    }

    fn collider_world(bounds: Aabb, only_from_above: bool) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_entity_colliders(vec![EntityCollider {
            bounds,
            only_from_above,
        }]);
        world
    }

    fn falling_player(y: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, y, 0.5),
            velocity: Vec3::new(0.0, -0.3, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        }
    }

    #[test]
    fn the_player_lands_on_a_boat_it_falls_onto() {
        let boat = Aabb::new(
            Vec3::new(-0.2, 64.0, -0.2),
            Vec3::new(1.175, 64.5625, 1.175),
        );
        let world = collider_world(boat, false);
        for version in ["1.12.2", "1.16.5", "1.21.1"] {
            let mut state = falling_player(64.7);
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.y, 64.5625, "{version}");
            assert!(state.on_ground, "{version}");
        }
    }

    #[test]
    fn a_happy_ghast_is_solid_only_to_a_player_whose_feet_are_at_or_above_its_top() {
        let ghast = Aabb::new(Vec3::new(-1.5, 60.0, -1.5), Vec3::new(2.5, 64.0, 2.5));
        let world = collider_world(ghast, true);
        let mut above = falling_player(64.0);
        crate::versioned::dispatch("1.21.6", &mut above, &world, PhysicsConfig::default()).unwrap();
        assert_eq!(above.position.y, 64.0);
        assert!(above.on_ground);
        let mut inside = falling_player(63.9);
        crate::versioned::dispatch("1.21.6", &mut inside, &world, PhysicsConfig::default())
            .unwrap();
        assert!(inside.position.y < 63.9);
    }

    #[test]
    fn since_26_3_a_collider_within_step_height_above_the_head_blocks_a_step_up() {
        // 26.3 Entity.java:1187 getEntityCollisions(this,
        // aabb.expandTowards(movement).expandTowards(0.0, maxUpStep(), 0.0)); 26.2 Entity.java:1156
        // queries only aabb.expandTowards(movement), which misses a boat 0.1 above the head.
        let boat = Aabb::new(Vec3::new(-1.0, 65.9, -1.0), Vec3::new(3.0, 66.4625, 2.0));
        let mut world = collider_world(boat, false);
        let block = |height: f64| BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, height, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        for x in -1..3 {
            world.set_block((x, 63, 0), block(1.0));
        }
        world.set_block((1, 64, 0), block(0.5));
        for (version, x, y) in [("26.2", 0.8, 64.5), ("26.3", 0.7, 64.0)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 64.0, 0.5),
                velocity: Vec3::new(0.3, -0.0784000015258789, 0.0),
                on_ground: true,
                ..falling_player(64.0)
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!((state.position.x, state.position.y), (x, y), "{version}");
        }
    }

    fn gliding_player() -> SimulationState {
        SimulationState {
            velocity: Vec3::new(0.25, -0.5, 1.0),
            look: Vec3::new(0.0, -0.17364817766693033, 0.984807753012208),
            fall_flying: true,
            ..player_at(0.0, 0.0)
        }
    }

    // D-222: a 1.11 rocket that ticks before the player boosts along getLookAngle, getViewVector(1.0F)
    // of yHeadRot (1.11.2 LivingEntity.java:1917-1925), the yaw the player's last aiStep left there
    // (Player.java:468-472); from 1.12 Entity.getLookAngle reads the yaw (1.12.2 Entity.java:1729-1731)
    #[test]
    fn a_1_11_rocket_before_the_player_boosts_along_the_head_yaw() {
        let head = Vec3::new(1.0, 0.0, 0.0);
        let boosted = |release: MinecraftVersion, head_look: Option<Vec3>| {
            let mut state = SimulationState { head_look, ..gliding_player() };
            super::boost_by_attached_rockets_before_player(&mut state, 1, release);
            state.velocity
        };
        let along = |look: Vec3| {
            let mut state = SimulationState { look, ..gliding_player() };
            boost_by_attached_rockets(&mut state, 1, MinecraftVersion::new(1, 11, 2));
            state.velocity
        };
        let v1_11 = MinecraftVersion::new(1, 11, 2);
        assert_eq!(boosted(v1_11, Some(head)), along(head));
        assert_ne!(boosted(v1_11, Some(head)), along(gliding_player().look));
        // no head look given: the look
        assert_eq!(boosted(v1_11, None), along(gliding_player().look));
        // 1.12 reads the yaw itself
        assert_eq!(boosted(MinecraftVersion::new(1, 12, 2), Some(head)), {
            let mut state = gliding_player();
            boost_by_attached_rockets(&mut state, 1, MinecraftVersion::new(1, 12, 2));
            state.velocity
        });
    }

    #[test]
    fn each_attached_rocket_pulls_a_gliding_player_toward_its_look() {
        let release = MinecraftVersion::new(1, 21, 1);
        let mut state = gliding_player();
        boost_by_attached_rockets(&mut state, 2, release);
        let look = gliding_player().look;
        let mut expected = gliding_player().velocity;
        for _ in 0..2 {
            expected = expected
                + Vec3::new(
                    look.x * 0.1 + (look.x * 1.5 - expected.x) * 0.5,
                    look.y * 0.1 + (look.y * 1.5 - expected.y) * 0.5,
                    look.z * 0.1 + (look.z * 1.5 - expected.z) * 0.5,
                );
        }
        assert_eq!(state.velocity, expected);
    }

    #[test]
    fn an_attached_rocket_boosts_in_its_turn_between_the_pushes() {
        // each entity runs its whole tick in the client's order (ClientLevel.tickEntities): a cow
        // ticking before the rocket pushes the player first, and the rocket's boost then halves
        // that push (1.21.11 FireworkRocketEntity.java:121-126: v + look * 0.1 + (look * 1.5 - v)
        // * 0.5); a cow ticking after it pushes the boosted velocity
        let release = MinecraftVersion::new(1, 21, 1);
        let world = WorldSnapshot::new();
        let cow = entity_at(NearbyEntityKind::Living, 0.7, 0.0, 0.45);
        let rocket = entity_at(NearbyEntityKind::AttachedRocket, 0.0, 0.0, 0.125);
        let boost = |v: Vec3, look: Vec3| {
            v + Vec3::new(
                look.x * 0.1 + (look.x * 1.5 - v.x) * 0.5,
                look.y * 0.1 + (look.y * 1.5 - v.y) * 0.5,
                look.z * 0.1 + (look.z * 1.5 - v.z) * 0.5,
            )
        };
        let mut pushed = gliding_player();
        let taken = push_by_nearby_entities(&mut pushed, &world, &[cow], false, release)[0];
        let push = Vec3::new(-taken.x, 0.0, -taken.z);
        let look = gliding_player().look;

        let mut push_first = gliding_player();
        let pushes =
            push_by_nearby_entities(&mut push_first, &world, &[cow, rocket], false, release);
        assert_eq!(pushes.len(), 1);
        assert_eq!(push_first.velocity, boost(pushed.velocity, look));

        let mut boost_first = gliding_player();
        push_by_nearby_entities(&mut boost_first, &world, &[rocket, cow], false, release);
        assert_eq!(
            boost_first.velocity,
            boost(gliding_player().velocity, look) + push
        );
        assert_ne!(push_first.velocity.x, boost_first.velocity.x);

        // a rocket on the other side of the player's tick waits for its own pass; one never
        // boosts a player that does not glide
        let mut other_side = gliding_player();
        push_by_nearby_entities(&mut other_side, &world, &[rocket], true, release);
        assert_eq!(other_side.velocity, gliding_player().velocity);
        let mut walking = SimulationState {
            fall_flying: false,
            ..gliding_player()
        };
        push_by_nearby_entities(&mut walking, &world, &[rocket], false, release);
        assert_eq!(walking.velocity, gliding_player().velocity);
    }

    #[test]
    fn rockets_boost_only_a_gliding_player_and_only_from_1_11() {
        let mut walking = SimulationState {
            fall_flying: false,
            ..gliding_player()
        };
        boost_by_attached_rockets(&mut walking, 1, MinecraftVersion::new(1, 21, 1));
        assert_eq!(walking.velocity, gliding_player().velocity);
        let mut legacy = gliding_player();
        boost_by_attached_rockets(&mut legacy, 1, MinecraftVersion::parse("1.10.2").unwrap());
        assert_eq!(legacy.velocity, gliding_player().velocity);
    }

    #[test]
    fn computes_entity_bounds_from_position() {
        let entity = EntityState {
            position: Vec3::new(2.0, 3.0, 4.0),
            velocity: Vec3::ZERO,
            dimensions: Dimensions::new(0.3, 1.8),
            flags: Default::default(),
            no_gravity: false,
            gravity: None,
            step_height: None,
        };

        assert_eq!(entity.bounds().min, Vec3::new(1.7, 3.0, 3.7));
        assert_eq!(entity.bounds().max, Vec3::new(2.3, 4.8, 4.3));
    }

    const MODERN: MinecraftVersion = MinecraftVersion::new(1, 21, 1);

    #[test]
    fn entity_push_matches_vanilla_horizontal_impulse() {
        let (left, right) = entity_push_impulses(
            Vec3::ZERO,
            Vec3::new(1.0, 4.0, 0.0),
            false,
            false,
            false,
            false,
            false,
            true,
            true,
            MODERN,
        );
        assert_eq!(left, Vec3::new(-0.05_f32 as f64, 0.0, -0.0));
        assert_eq!(right, Vec3::new(0.05_f32 as f64, 0.0, 0.0));
    }

    #[test]
    fn entity_push_respects_threshold_and_gates() {
        assert_eq!(
            entity_push_impulses(
                Vec3::ZERO,
                Vec3::new(0.009, 0.0, 0.0),
                false,
                false,
                false,
                false,
                false,
                true,
                true,
                MODERN,
            ),
            (Vec3::ZERO, Vec3::ZERO)
        );
        assert_eq!(
            entity_push_impulses(
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                true,
                false,
                false,
                false,
                false,
                true,
                true,
                MODERN,
            ),
            (Vec3::ZERO, Vec3::ZERO)
        );
    }

    #[test]
    fn entity_push_threshold_is_the_float_0_01() {
        // `dd >= 0.009999999776482582` (0.01F; 1.8.8 Entity.java:1030, 26.3 :1918): 0.00999999
        // apart still pushes
        let apart = 0.009_999_999_8;
        assert!(apart < 0.01 && apart >= f64::from(0.01_f32));
        let (left, right) = entity_push_impulses(
            Vec3::ZERO,
            Vec3::new(apart, 0.0, 0.0),
            false,
            false,
            false,
            false,
            false,
            true,
            true,
            MODERN,
        );
        assert!(left.x < 0.0 && right.x > 0.0);
    }

    #[test]
    fn entity_push_takes_a_float_square_root_before_1_17() {
        // Mth.sqrt(double) is `(float)Math.sqrt` before 1.17 (1.16.5 Entity.java:1294);
        // 1.17 :1269 takes Math.sqrt
        let to = Vec3::new(0.3, 0.0, 0.1);
        let push = |release| {
            entity_push_impulses(
                Vec3::ZERO,
                to,
                false,
                false,
                false,
                false,
                false,
                true,
                true,
                release,
            )
            .1
        };
        let impulse =
            |distance: f64| (0.3 / distance) * (1.0 / distance).min(1.0) * 0.05_f32 as f64;
        let legacy = f64::from(0.3_f64.sqrt() as f32);
        assert_eq!(push(MinecraftVersion::new(1, 16, 5)).x, impulse(legacy));
        assert_eq!(
            push(MinecraftVersion::new(1, 17, 1)).x,
            impulse(0.3_f64.sqrt())
        );
        assert_ne!(impulse(legacy), impulse(0.3_f64.sqrt()));
    }

    #[test]
    fn entity_push_gates_unpushable_sides_only_from_1_19() {
        // 1.19 Entity.java:1316-1322 adds `&& isPushable()` to both sides; 1.18 :1267-1273 has
        // only !isVehicle (hasPassengers)
        for (release, pushed) in [((1, 18, 2), true), ((1, 19, 0), false)] {
            let release = MinecraftVersion::new(release.0, release.1, release.2);
            let (left, right) = entity_push_impulses(
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                false,
                false,
                false,
                false,
                false,
                false,
                false,
                release,
            );
            assert_eq!(left != Vec3::ZERO, pushed, "{release:?}");
            assert_eq!(right != Vec3::ZERO, pushed, "{release:?}");
        }
        // a vehicle (one with passengers) takes nothing in every release
        let (left, right) = entity_push_impulses(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            false,
            false,
            false,
            true,
            false,
            true,
            true,
            MinecraftVersion::new(1, 8, 9),
        );
        assert_eq!(left, Vec3::ZERO);
        assert_eq!(right.x, 0.05_f32 as f64);
    }

    #[test]
    fn entity_push_skips_passengers_of_same_vehicle() {
        assert_eq!(
            entity_push_impulses(
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                false,
                false,
                true,
                false,
                false,
                true,
                true,
                MODERN,
            ),
            (Vec3::ZERO, Vec3::ZERO)
        );
    }

    #[test]
    fn entity_push_preserves_vanilla_double_precision_after_float_impulse_constant() {
        let (left, right) = entity_push_impulses(
            Vec3::ZERO,
            Vec3::new(3.0, 0.0, 4.0),
            false,
            false,
            false,
            false,
            false,
            true,
            true,
            MODERN,
        );
        let impulse = 0.05_f32 as f64;
        assert_eq!(left, Vec3::new(-0.75 * impulse, 0.0, -1.0 * impulse));
        assert_eq!(right, Vec3::new(0.75 * impulse, 0.0, 1.0 * impulse));
    }

    #[test]
    fn entity_push_applies_source_pushthrough_to_both_impulses() {
        // `d *= 1.0F - this.pushSpeedReduction` on both sides (1.8.8 Entity.java:1043-1044,
        // 1.16.5 :1306-1307): the calling entity's float
        let (left, right) = entity_push_impulses_with_pushthrough(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            false,
            false,
            false,
            false,
            false,
            true,
            true,
            0.4,
            MinecraftVersion::new(1, 16, 5),
        );
        let impulse = 0.05_f32 as f64 * f64::from(1.0_f32 - 0.4_f32);
        assert_ne!(f64::from(1.0_f32 - 0.4_f32), 0.6);
        assert_eq!(left, Vec3::new(-impulse, 0.0, 0.0));
        assert_eq!(right, Vec3::new(impulse, 0.0, 0.0));
    }

    fn pushthrough_impulses(pushthrough: f64, release: MinecraftVersion) -> (Vec3, Vec3) {
        entity_push_impulses_with_pushthrough(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            false,
            false,
            false,
            false,
            false,
            true,
            true,
            pushthrough,
            release,
        )
    }

    #[test]
    fn entity_push_ignores_pushthrough_from_1_17() {
        // 1.17 Entity.java:1262-1291 has no pushSpeedReduction, 1.16.5 :1306-1307 scales by it
        let plain = f64::from(0.05_f32);
        for release in [
            MinecraftVersion::new(1, 17, 0),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(26, 3, 0),
        ] {
            assert_eq!(
                pushthrough_impulses(1.0, release),
                (Vec3::new(-plain, 0.0, 0.0), Vec3::new(plain, 0.0, 0.0))
            );
        }
        assert_eq!(
            pushthrough_impulses(1.0, MinecraftVersion::new(1, 16, 5)),
            (Vec3::ZERO, Vec3::ZERO)
        );
    }

    #[test]
    fn entity_push_needs_two_apart_centres_and_two_physical_entities() {
        let release = MinecraftVersion::new(26, 3, 0);
        let at = Vec3::new(4.0, 70.0, -9.0);
        let push = |left: Vec3, right: Vec3, left_ghost: bool, right_ghost: bool| {
            entity_push_impulses(
                left, right, left_ghost, right_ghost, false, false, false, true, true, release,
            )
        };
        // Coincident centres: absMax is 0, under 0.01F. A height difference alone is no distance.
        assert_eq!(push(at, at, false, false), (Vec3::ZERO, Vec3::ZERO));
        assert_eq!(
            push(at, at + Vec3::new(0.0, 5.0, 0.0), false, false),
            (Vec3::ZERO, Vec3::ZERO)
        );
        // !entity.noPhysics && !this.noPhysics (1.8.8 Entity.java:1026, 26.3 :1914): either side
        let apart = at + Vec3::new(0.5, 0.0, 0.0);
        assert_ne!(push(at, apart, false, false).0, Vec3::ZERO);
        assert_eq!(push(at, apart, true, false), (Vec3::ZERO, Vec3::ZERO));
        assert_eq!(push(at, apart, false, true), (Vec3::ZERO, Vec3::ZERO));
    }
}
