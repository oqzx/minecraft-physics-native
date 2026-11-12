//! B280: each engine-internal helper that cites no vanilla method is called here by a path only it can have
//! (`Type::name`, `module::name`), so audit/shared-equivalence-reviews/ can name the test that checks its result
//! (scripts/shared-equivalence.cjs helperProblems).

use crate::entity::Dimensions;
use crate::entity_travel::TravelFamily;
use crate::math::{Aabb, Vec3};
use crate::moving_entity::MovingEntityKind;
use crate::phase::Phase;
use crate::projectile::{FlightHit, ProjectileKind};
use crate::projectile_hits::{HitScene, ProjectileHitState, ProjectileReaction, ProjectileTarget};
use crate::simulation::{PhysicsConfig, SimulationState, VersionPhysicsProfile, VersionPhysicsRules};
use crate::vehicle::LegacyBoatState;
use crate::version::MinecraftVersion;
use crate::versioned::{ReleaseEngine, VersionEngine};
use crate::world::{BlockShape, BlockState, EntityCollisionContext, FluidState, VersionedBlockStateId, WorldSnapshot};

#[test]
fn the_engines_value_constructors_keep_what_they_are_given() {
    let dimensions = Dimensions::new(0.3, 1.8);
    assert_eq!((dimensions.half_width, dimensions.height), (0.3, 1.8));
    let vector = Vec3::new(1.0, -2.0, 3.5);
    assert_eq!((vector.x, vector.y, vector.z), (1.0, -2.0, 3.5));
    let bounds = Aabb::new(Vec3::new(0.0, 0.0, 0.0), vector);
    assert_eq!((bounds.min, bounds.max), (Vec3::ZERO, vector));
    let target = ProjectileTarget::new(7, vector, bounds);
    assert_eq!((target.id, target.position, target.bounding_box), (7, vector, bounds));
    let mut hits = ProjectileHitState::default();
    let scene = HitScene::new(&[], None, &mut hits);
    assert!(scene.targets.is_empty());
}

#[test]
fn a_release_name_parses_to_its_numbers_and_back() {
    assert_eq!(MinecraftVersion::parse("1.21.11"), Some(MinecraftVersion::new(1, 21, 11)));
    assert_eq!(MinecraftVersion::parse("26.3"), Some(MinecraftVersion::new(26, 3, 0)));
    assert_eq!(MinecraftVersion::parse("1.x"), None);
    assert_eq!(MinecraftVersion::new(1, 8, 8).to_string(), "1.8.8");
}

#[test]
fn the_names_the_bindings_report_are_each_kinds_own() {
    assert_eq!(TravelFamily::name(TravelFamily::Player), "player");
    assert_eq!(MovingEntityKind::name(MovingEntityKind::PrimedTnt), "tnt");
    assert_eq!(Phase::name(Phase::TickStart), "tickStart");
    assert_eq!(ProjectileReaction::name(ProjectileReaction::Hurt), "hurt");
}

#[test]
fn a_projectile_kind_is_thrown_or_hurting_by_its_vanilla_class() {
    // ThrowableProjectile's family and AbstractHurtingProjectile's, the engine's instanceof
    assert!(ProjectileKind::is_thrown(ProjectileKind::Generic));
    assert!(!ProjectileKind::is_hurting(ProjectileKind::Generic));
    assert!(ProjectileKind::is_hurting(ProjectileKind::SmallFireball));
    assert!(!ProjectileKind::is_thrown(ProjectileKind::SmallFireball));
    assert!(!FlightHit::is_block(FlightHit::Entity((0, Vec3::ZERO))));
}

#[test]
fn a_vehicles_trace_gives_back_what_the_tick_returned() {
    let (value, events) = crate::phase::traced_vehicle(|| 42);
    assert_eq!(value, 42);
    assert!(events.is_empty());
}

#[test]
fn the_math_java_switch_is_set_and_cleared() {
    crate::math::set_math_java(Some(21));
    crate::math::set_math_java(None);
}

#[test]
fn the_world_store_keeps_what_is_set_in_a_loaded_chunk() {
    let mut world = WorldSnapshot::new();
    assert!(!WorldSnapshot::is_chunk_loaded(&world, 0, 0));
    world.set_block((1, 2, 3), BlockState { fluid: FluidState::Water, ..BlockState::default() });
    assert!(WorldSnapshot::is_chunk_loaded(&world, 0, 0));
    assert_eq!(WorldSnapshot::fluid_at(&world, (1, 2, 3)), FluidState::Water);
    WorldSnapshot::set_block_properties(&mut world, (1, 2, 3), vec![("level".to_owned(), "0".to_owned())]);
    WorldSnapshot::set_world_bounds(&mut world, -64, 320, 0.0, 0.0, 1000.0);
    assert_eq!(world.world_bounds(), (-64, 320, 0.0, 0.0, 1000.0));
    let id = VersionedBlockStateId::new(MinecraftVersion::new(1, 21, 11), 5).expect("a registered release");
    assert_eq!((VersionedBlockStateId::version(id), VersionedBlockStateId::id(id)), (MinecraftVersion::new(1, 21, 11), 5));
    assert!(VersionedBlockStateId::new(MinecraftVersion::new(1, 7, 10), 5).is_err());
    world.set_block((0, 0, 0), BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() });
    let boxes = WorldSnapshot::legacy_block_boxes_for_entity(&world, (0, 0, 0), &EntityCollisionContext::default());
    assert_eq!(boxes, vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))]);
}

#[test]
fn a_release_engine_and_its_profile_tick_its_own_release() {
    type Engine = crate::versioned::v1_20_4::Engine;
    type Profile = crate::versioned::v1_20_4::Profile;
    let engine = ReleaseEngine::of::<Engine>();
    assert_eq!(ReleaseEngine::version(engine), "1.20.4");
    let rules = <Profile as VersionPhysicsProfile>::rules();
    assert_eq!(rules.release, MinecraftVersion::new(1, 20, 4));
    let world = WorldSnapshot::new();
    let mut state = SimulationState::default();
    let mut other = SimulationState::default();
    ReleaseEngine::tick(engine, &mut state, &world, PhysicsConfig::default()).expect("a supported release");
    <Engine as VersionEngine>::tick(&mut other, &world, PhysicsConfig::default());
    assert_eq!(state.position, other.position);
}

#[test]
fn the_defaults_are_the_documented_starting_values() {
    let rules = VersionPhysicsRules::default();
    assert_eq!((rules.release, rules.gravity), (MinecraftVersion::new(26, 2, 0), 0.08));
    let config = PhysicsConfig::default();
    assert_eq!(config.gravity, 0.08);
    // a 1.8 boat's speed multiplier starts at 0.07 (1.8.8 Boat field_3879's initializer)
    assert_eq!(LegacyBoatState::default().speed_multiplier, 0.07);
}

/// HeldMovements keeps the pushes others' ticks recorded, in order, and joins the first two past four
/// (helpers-close-c.json).
#[test]
fn the_held_movements_keep_four_and_join_the_first_two_past_them() {
    use crate::block_traversal::Movement;
    use crate::simulation::HeldMovements;
    let step = |x: f64| Movement { from: Vec3::new(x, 0.0, 0.0), to: Vec3::new(x + 1.0, 0.0, 0.0), axis_order: None };
    let mut held = HeldMovements::default();
    assert!(HeldMovements::as_slice(&held).is_empty());
    for x in 0..4 {
        HeldMovements::push(&mut held, step(f64::from(x)));
    }
    assert_eq!(HeldMovements::as_slice(&held), (0..4).map(|x| step(f64::from(x))).collect::<Vec<_>>());
    HeldMovements::push(&mut held, step(4.0));
    let joined = Movement { from: Vec3::new(0.0, 0.0, 0.0), to: Vec3::new(2.0, 0.0, 0.0), axis_order: None };
    assert_eq!(HeldMovements::as_slice(&held), vec![joined, step(2.0), step(3.0), step(4.0)]);
}

/// BlockVoxelShape::moving_piston marks a shape as a moving piston block's and keeps the rest (helpers-close-c.json).
#[test]
fn a_shape_marked_as_a_moving_pistons_keeps_its_boxes() {
    use crate::collision::BlockVoxelShape;
    let shape = BlockVoxelShape::new(Vec3::new(1.0, 2.0, 3.0), vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))]);
    assert!(!shape.moving_piston);
    let marked = BlockVoxelShape::moving_piston(shape.clone(), true);
    assert!(marked.moving_piston);
    assert_eq!((marked.origin, &marked.boxes, &marked.local_boxes), (shape.origin, &shape.boxes, &shape.local_boxes));
    assert_eq!(BlockVoxelShape::moving_piston(marked, false), shape);
}

/// The local boxes of a block's collision and outline shapes, the empty ones left out (helpers-close-c.json).
#[test]
fn a_blocks_local_boxes_are_its_non_empty_shapes_in_its_own_cell() {
    let slab = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0));
    let outline = Aabb::new(Vec3::new(0.25, 0.0, 0.25), Vec3::new(0.75, 0.5, 0.75));
    let mut world = WorldSnapshot::new();
    world.set_block(
        (4, 5, 6),
        BlockState {
            shape: Some(BlockShape { bounds: slab, ..BlockShape::full_block() }),
            outline_shape: Some(BlockShape { bounds: outline, ..BlockShape::full_block() }),
            ..BlockState::default()
        },
    );
    world.set_block((0, 5, 6), BlockState { shape: Some(BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO), ..BlockShape::full_block() }), ..BlockState::default() });
    assert_eq!(WorldSnapshot::block_collision_local_boxes(&world, (4, 5, 6)), vec![slab]);
    assert_eq!(WorldSnapshot::block_outline_local_boxes(&world, (4, 5, 6)), vec![outline]);
    assert!(WorldSnapshot::block_collision_local_boxes(&world, (0, 5, 6)).is_empty());
    assert!(WorldSnapshot::block_outline_local_boxes(&world, (1, 5, 6)).is_empty());
}
