use crate::math::{abs_max, Aabb, Vec3};
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub dimensions: Dimensions,
    pub flags: EntityFlags,
}

impl EntityState {
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
        0.0,
    )
}

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
    left_pushthrough: f64,
    right_pushthrough: f64,
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
    let mut x = right_position.x - left_position.x;
    let mut z = right_position.z - left_position.z;
    let distance = abs_max(x, z);
    if distance < 0.01 || !distance.is_finite() {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let distance = distance.sqrt();
    let scale = (1.0 / distance).min(1.0);
    x = (x / distance) * scale * 0.05_f32 as f64;
    z = (z / distance) * scale * 0.05_f32 as f64;
    let left_pushthrough_scale = 1.0 - left_pushthrough;
    let right_pushthrough_scale = 1.0 - right_pushthrough;
    let left_impulse = if !left_vehicle && left_pushable {
        Vec3::new(-x * left_pushthrough_scale, 0.0, -z * left_pushthrough_scale)
    } else {
        Vec3::ZERO
    };
    let right_impulse = if !right_vehicle && right_pushable {
        Vec3::new(x * right_pushthrough_scale, 0.0, z * right_pushthrough_scale)
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
    /// Neither pushes nor collides: projectiles, items, rockets.
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
            _ => Self::Inert,
        }
    }

    fn collider(self, bounds: Aabb) -> Option<EntityCollider> {
        match self {
            Self::Boat | Self::Shulker => Some(EntityCollider {
                bounds,
                only_from_above: false,
            }),
            Self::HappyGhast => Some(EntityCollider {
                bounds,
                only_from_above: true,
            }),
            Self::Living | Self::Inert => None,
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
}

/// The colliders among the entities near the player: where the player's move meets them, or
/// where their own ticks leave them (for what moves the player after every entity ticked).
#[must_use]
pub fn nearby_entity_colliders(
    entities: &[NearbyEntity],
    after_their_ticks: bool,
) -> Vec<EntityCollider> {
    entities
        .iter()
        .filter_map(|entity| {
            let bounds = if after_their_ticks {
                entity.tick_bounding_box
            } else {
                entity.bounding_box
            };
            entity.kind.collider(bounds)
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
/// (EntitySelector.pushableBy) when it is pushable: not a spectator, not on a climbable. Returns
/// the pushes they took back.
pub fn push_by_nearby_entities(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    entities: &[NearbyEntity],
    before_player: bool,
    release: MinecraftVersion,
) -> Vec<EntityPush> {
    let mut pushes = Vec::new();
    if !release.at_least(1, 9, 0) || (state.is_player && state.no_physics) {
        return pushes;
    }
    if climbable(world, state) {
        return pushes;
    }
    for (index, entity) in entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.ticks_before_player == before_player)
    {
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
                    state.velocity = state.velocity + Vec3::new(x, 0.0, z);
                    pushes.push(EntityPush {
                        index,
                        x: -x,
                        z: -z,
                    });
                }
            }
            NearbyEntityKind::Living | NearbyEntityKind::Shulker | NearbyEntityKind::HappyGhast => {
                if !player_box.intersects(other_box) {
                    continue;
                }
                if let Some((x, z)) = push_impulse(state.position, entity.tick_position, release) {
                    state.velocity = state.velocity + Vec3::new(-x, 0.0, -z);
                    pushes.push(EntityPush { index, x, z });
                }
            }
            NearbyEntityKind::Inert => {}
        }
    }
    pushes
}

/// Entity.push(Entity) from `from` toward `to`: what `to` gets, 0.05 away from `from`, nothing
/// when they are 0.01F or less apart. The square root is a float's before 1.17 (Mth.sqrt).
fn push_impulse(from: Vec3, to: Vec3, release: MinecraftVersion) -> Option<(f64, f64)> {
    let mut x = to.x - from.x;
    let mut z = to.z - from.z;
    let distance = abs_max(x, z);
    if !(distance >= f64::from(0.01_f32)) {
        return None;
    }
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
    if !release.at_least(1, 11, 0) || !state.fall_flying {
        return;
    }
    let look = state.look;
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
        nearby_entity_colliders, push_by_nearby_entities, Dimensions, EntityState, NearbyEntity,
        NearbyEntityKind,
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
        }
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
            ],
            false,
        );
        let flags: Vec<bool> = colliders
            .iter()
            .map(|collider| collider.only_from_above)
            .collect();
        assert_eq!(flags, vec![false, false, true]);
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

    fn gliding_player() -> SimulationState {
        SimulationState {
            velocity: Vec3::new(0.25, -0.5, 1.0),
            look: Vec3::new(0.0, -0.17364817766693033, 0.984807753012208),
            fall_flying: true,
            ..player_at(0.0, 0.0)
        }
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
    fn rockets_boost_only_a_gliding_player_and_only_from_1_11() {
        let mut walking = SimulationState {
            fall_flying: false,
            ..gliding_player()
        };
        boost_by_attached_rockets(&mut walking, 1, MinecraftVersion::new(1, 21, 1));
        assert_eq!(walking.velocity, gliding_player().velocity);
        let mut legacy = gliding_player();
        boost_by_attached_rockets(&mut legacy, 1, MinecraftVersion::new(1, 10, 2));
        assert_eq!(legacy.velocity, gliding_player().velocity);
    }

    #[test]
    fn computes_entity_bounds_from_position() {
        let entity = EntityState {
            position: Vec3::new(2.0, 3.0, 4.0),
            velocity: Vec3::ZERO,
            dimensions: Dimensions::new(0.3, 1.8),
            flags: Default::default(),
        };

        assert_eq!(entity.bounds().min, Vec3::new(1.7, 3.0, 3.7));
        assert_eq!(entity.bounds().max, Vec3::new(2.3, 4.8, 4.3));
    }

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
            ),
            (Vec3::ZERO, Vec3::ZERO)
        );
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
        );
        let impulse = 0.05_f32 as f64;
        assert_eq!(left, Vec3::new(-0.75 * impulse, 0.0, -1.0 * impulse));
        assert_eq!(right, Vec3::new(0.75 * impulse, 0.0, 1.0 * impulse));
    }

    #[test]
    fn entity_push_applies_source_pushthrough_to_both_impulses() {
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
            0.2,
        );
        let left_impulse = 0.05_f32 as f64 * 0.6;
        let right_impulse = 0.05_f32 as f64 * 0.8;
        assert_eq!(left, Vec3::new(-left_impulse, 0.0, 0.0));
        assert_eq!(right, Vec3::new(right_impulse, 0.0, 0.0));
    }

    #[test]
    fn entity_push_uses_independent_pushthrough_on_each_side() {
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
            0.75,
            0.25,
        );
        let left_impulse = 0.05_f32 as f64 * 0.25;
        let right_impulse = 0.05_f32 as f64 * 0.75;
        assert_eq!(left, Vec3::new(-left_impulse, 0.0, 0.0));
        assert_eq!(right, Vec3::new(right_impulse, 0.0, 0.0));
    }
}
