//! B290 fixtures for the gates of napi_bulk.rs's private functions: a child module of it (see gate_fixtures_tests.rs).

use super::*;

fn batch(name: &str, simulations: Vec<SimulationState>, rest: Vec<EntityRest>, roots: Vec<Option<u32>>) -> EntityBatch {
    let has_passengers = vec![false; simulations.len()];
    EntityBatch {
        version: name.to_owned(),
        release: MinecraftVersion::parse(name).expect("a registered release"),
        config: PhysicsConfig::default(),
        simulations,
        rest,
        has_passengers,
        roots,
    }
}

fn rest(id: u32, vehicle_id: Option<u32>) -> EntityRest {
    EntityRest { id: Some(id), no_physics: false, vehicle_id, pushable: true, pushthrough: 0.0 }
}

fn body(x: f64) -> SimulationState {
    SimulationState { position: Vec3::new(x, 64.0, 0.5), half_width: 0.3, height: 1.8, ..SimulationState::default() }
}

/// Two passengers of one vehicle do not push each other from 1.9, being of the same root vehicle (1.9.4 Entity.java:2225); 1.8
/// skips only an entity and its own rider or vehicle (1.8.8 Entity.java:1025).
#[test]
fn passengers_of_one_vehicle_are_connected_from_1_9() {
    let connected = |name: &str| {
        let batch = batch(
            name,
            vec![body(0.0), body(0.0), body(0.0)],
            vec![rest(0, Some(2)), rest(1, Some(2)), rest(2, None)],
            vec![Some(2), Some(2), Some(2)],
        );
        batch.connected(0, 1)
    };
    assert!(!connected("1.8.8"));
    assert!(connected("1.9.4"));
}

/// A sleeping player's own Entity.push does nothing from 1.9 (1.9.4 Player.java:1867-1871), so the entity that ticks beside it is
/// pushed once less than beside a sleeping mob, which pushes until 1.14; 1.8.9 has no such override, so both push alike.
#[test]
fn a_sleeping_player_pushes_nothing_from_1_9() {
    let moved = |name: &str, player: bool| {
        let mut sleeper = body(0.9);
        sleeper.sleeping = true;
        sleeper.is_player = player;
        let mut batch = batch(name, vec![body(0.5), sleeper], vec![rest(0, None), rest(1, None)], vec![None, None]);
        batch.advance(&WorldSnapshot::new(), 1).expect("a tick");
        batch.simulations[0].velocity.x
    };
    assert_eq!(moved("1.8.8", true), moved("1.8.8", false));
    assert_ne!(moved("1.9.4", true), moved("1.9.4", false));
}
