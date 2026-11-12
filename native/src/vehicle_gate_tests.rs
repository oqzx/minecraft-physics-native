//! B290 fixtures for the gates of vehicle.rs's private functions: a child module of it (see gate_fixtures_tests.rs).

use super::*;
use crate::world::{BlockShape, BlockState, FluidState, WorldSnapshot};

/// A boat that comes out of the air onto water under a waterlogged top slab: 1.20.6 sets it on the surface anyway (1.20.6
/// Boat.java:570), from 1.21 only where the box there meets nothing (1.21 Boat.java:594).
#[test]
fn a_boat_is_set_on_the_water_only_where_it_fits_from_1_21() {
    let water = BlockState { fluid: FluidState::Water, fluid_source: true, ..BlockState::default() };
    let slab = BlockShape { bounds: Aabb::new(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)), ..BlockShape::full_block() };
    let mut world = WorldSnapshot::new();
    world.set_block((0, 63, 0), water);
    world.set_block((0, 64, 0), BlockState { id: 1, shape: Some(slab), ..water });
    let floated = |name: &str| {
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 63.0, 0.5),
            previous_status: BoatStatus::InAir,
            status: BoatStatus::InWater,
            last_yd: -1.0,
            ..BoatSimulationState::default()
        };
        float_boat(&mut boat, &world, MinecraftVersion::parse(name).expect("a registered release"));
        boat.position.y
    };
    assert!(floated("1.20.6") > 64.0);
    assert_eq!(floated("1.21"), 63.0);
}

/// A client's boat nobody rides, falling: 1.21.1 Entity.move checks the fall of every move (Entity.java:681), 1.21.2 only on
/// the server or for what the local instance controls (Entity.java:709-711), which a boat with no player in it is not.
#[test]
fn a_clients_boat_nobody_rides_counts_no_fall_from_1_21_2() {
    let mut world = WorldSnapshot::new();
    for x in -2..=2 {
        for z in -2..=2 {
            world.set_block((x, 63, z), BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() });
        }
    }
    let mut fell = |name: &str| {
        let mut boat = BoatSimulationState {
            position: Vec3::new(0.5, 69.9, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            status: BoatStatus::InAir,
            previous_status: BoatStatus::InAir,
            fall_distance: 1.0,
            ..BoatSimulationState::default()
        };
        tick_boat_in_world(&mut boat, &mut world, MinecraftVersion::parse(name).expect("a registered release"));
        boat.fall_distance
    };
    assert!(fell("1.21.1") > 1.4);
    assert_eq!(fell("1.21.2"), 1.0);
}
