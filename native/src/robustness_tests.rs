//! What malformed input does at the bindings: it fails the call or runs, and never panics (which aborts a release build's
//! whole process) or hangs. Each case runs on its own thread, so a panic or a hang fails its test alone.

use std::sync::mpsc;
use std::time::Duration;

use crate::napi_api::{
    simulate_versioned_entity_ticks, simulate_versioned_tick, NativeAabb, NativeBlockShape,
    NativeBlockUpdate, NativeSimulationConfig, NativeSimulationState, NativeVec3,
    NativeVersionedEntityState, NativeWorld, MAX_ATTACHED_ROCKETS,
};
use crate::simulation::SimulationState;

pub(crate) fn vec3(x: f64, y: f64, z: f64) -> NativeVec3 {
    NativeVec3 { x, y, z }
}

fn config() -> NativeSimulationConfig {
    NativeSimulationConfig {
        gravity: None,
        drag: None,
        movement_speed: None,
        sprint_multiplier: None,
        jump_velocity: None,
        step_height: None,
        negligible_velocity: None,
    }
}

pub(crate) fn block(x: i32, y: i32, z: i32) -> NativeBlockUpdate {
    NativeBlockUpdate {
        x,
        y,
        z,
        id: Some(1),
        state_version: None,
        block_entity: None,
        remove_block_entity: None,
        properties: None,
        tags: None,
        shape: Some(NativeBlockShape {
            min_x: 0.0,
            min_y: 0.0,
            min_z: 0.0,
            max_x: 1.0,
            max_y: 1.0,
            max_z: 1.0,
            slipperiness: 0.6,
            effect: None,
            fall_damage_resetting: None,
            bounce_restitution: None,
            suppresses_bounce: None,
        }),
        outline_shape: None,
        interaction_shape: None,
        fluid: None,
        fluid_tags: None,
        fluid_level: None,
        fluid_source: None,
        fluid_flow: None,
        fluid_falling: None,
        water_block: None,
        fall_damage_resetting: None,
        player_fall_damage_resetting: None,
        bounce_restitution: None,
        suppresses_bounce: None,
        suffocating: None,
        in_wall: None,
        bottom_face_sturdy: None,
        explosion_resistance: None,
        remove: None,
    }
}

pub(crate) fn floor() -> NativeWorld {
    let mut world = NativeWorld::new();
    let updates = (-4..4).flat_map(|x| (-4..4).map(move |z| block(x, 0, z))).collect();
    world.apply_block_updates(updates).expect("blocks go in");
    world
}

fn walker() -> NativeSimulationState {
    let mut state: NativeSimulationState = SimulationState::default().into();
    state.position = vec3(0.5, 1.0, 0.5);
    state.half_width = 0.3;
    state.height = 1.8;
    state.eye_height = Some(1.62);
    state.is_player = Some(true);
    state.controlled_by_local_instance = Some(true);
    state.movement_speed = Some(0.1);
    state
}

/// The call on its own thread: None when it did not end in time, and a panic fails the test.
fn in_time<T: Send + 'static>(seconds: u64, call: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || {
            let _ = sender.send(call());
        })
        .expect("a thread");
    match receiver.recv_timeout(Duration::from_secs(seconds)) {
        Ok(value) => Some(value),
        Err(mpsc::RecvTimeoutError::Timeout) => None,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let panic = thread.join().err().and_then(|panic| panic.downcast_ref::<String>().cloned());
            panic!("the call panicked: {panic:?}");
        }
    }
}

const RELEASES: [&str; 4] = ["1.8.8", "1.16.5", "1.21.1", "26.3"];
const EXTREMES: [f64; 7] = [
    0.0,
    1.0e9,
    -2_147_483_649.0,
    1.0e300,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
];

type Change = Box<dyn Fn(&mut NativeSimulationState) + Send>;

fn extreme_changes() -> Vec<(String, Change)> {
    let mut cases: Vec<(String, Change)> = Vec::new();
    for value in EXTREMES {
        for other in EXTREMES {
            cases.push((format!("position {value} {other}"), Box::new(move |state| state.position = vec3(value, 1.0, other))));
            cases.push((format!("position y {value} {other}"), Box::new(move |state| state.position = vec3(0.5, value, other))));
            cases.push((format!("velocity {value} {other}"), Box::new(move |state| state.velocity = vec3(value, 0.0, other))));
            cases.push((format!("velocity y {value} {other}"), Box::new(move |state| state.velocity = vec3(0.0, value, other))));
        }
        cases.push((format!("half width {value}"), Box::new(move |state| state.half_width = value)));
        cases.push((format!("height {value}"), Box::new(move |state| state.height = value)));
        cases.push((format!("fall distance {value}"), Box::new(move |state| state.fall_distance = Some(value))));
        cases.push((format!("box {value}"), Box::new(move |state| {
            state.bounding_box = Some(NativeAabb { min: vec3(-value, 1.0, -value), max: vec3(value, 2.0, value) });
        })));
        cases.push((format!("box y {value}"), Box::new(move |state| {
            state.bounding_box = Some(NativeAabb { min: vec3(0.2, -value, 0.2), max: vec3(0.8, value, 0.8) });
        })));
        cases.push((format!("step height {value}"), Box::new(move |state| state.step_height = Some(value))));
        cases.push((format!("speed {value}"), Box::new(move |state| {
            state.movement_speed = Some(value);
            state.input = Some(vec3(0.0, 0.0, 1.0));
        })));
        cases.push((format!("look {value}"), Box::new(move |state| {
            state.look = Some(vec3(value, value, value));
            state.fall_flying = Some(true);
            state.firework_rockets = Some(1);
        })));
        cases.push((format!("sneaking off an edge at {value}"), Box::new(move |state| {
            state.velocity = vec3(value, 0.0, value);
            state.sneaking = Some(true);
            state.on_ground = true;
        })));
        cases.push((format!("keys {value}"), Box::new(move |state| state.input = Some(vec3(value, 0.0, value)))));
        cases.push((format!("yaw {value}"), Box::new(move |state| state.yaw = Some(value))));
        cases.push((format!("stuck {value}"), Box::new(move |state| state.stuck_speed_multiplier = Some(vec3(value, value, value)))));
    }
    cases
}

/// The scene of test/collision-boundary.test.cjs, by the binding's own calls and by the world's:
/// the same boxes in the same order, by their bits, on both.
#[test]
fn collision_boundary_matches_the_direct_query() {
    use crate::math::{Aabb, Vec3};
    use crate::napi_api::{NativeBlockShape, NativeWorld};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockShape, BlockState, WorldSnapshot};
    let boxes = [
        [0.0, 0.0, 0.0, 1.0 / 3.0, 0.1 + 0.2, 1.0],
        [0.5 + 1.0e-7, 0.0, 0.0, 1.0, 1.0 - 1.0e-7, 1.0],
    ];
    let cells = [(3, 4, 5), (-1, -60, -1), (-29_999_984, 70, 29_999_983)];
    let mut binding = NativeWorld::new();
    let mut direct = WorldSnapshot::new();
    for (x, y, z) in cells {
        let shape = |b: &[f64; 6]| NativeBlockShape {
            min_x: b[0], min_y: b[1], min_z: b[2], max_x: b[3], max_y: b[4], max_z: b[5],
            slipperiness: 0.6, effect: None, bounce_restitution: None, suppresses_bounce: None,
            fall_damage_resetting: None,
        };
        binding
            .set_block_shapes(x, y, z, 1, boxes.iter().map(shape).collect(), None)
            .expect("the shapes go in");
        let shapes: Vec<BlockShape> = boxes
            .iter()
            .map(|b| BlockShape {
                bounds: Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])),
                ..BlockShape::full_block()
            })
            .collect();
        direct.set_block_shapes(
            (x, y, z),
            BlockState { id: 1, shape: Some(shapes[0]), ..BlockState::default() },
            shapes,
        );
    }
    let release = MinecraftVersion::new(26, 3, 0);
    for (x, y, z) in cells {
        let (x, y, z) = (f64::from(x), f64::from(y), f64::from(z));
        let (min, max) = (Vec3::new(x - 0.5, y - 0.5, z - 0.5), Vec3::new(x + 1.5, y + 1.5, z + 1.5));
        let through = binding
            .collision_shapes(vec3(min.x, min.y, min.z), vec3(max.x, max.y, max.z), Some("26.3".to_string()))
            .expect("a legal query");
        let straight = direct.collision_shapes(Aabb::new(min, max), release);
        assert_eq!(through.len(), 2);
        assert_eq!(through.len(), straight.len());
        for (left, right) in through.iter().zip(&straight) {
            let bits = |v: &NativeVec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
            let of = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
            assert_eq!(bits(&left.min), of(right.min));
            assert_eq!(bits(&left.max), of(right.max));
        }
    }
}

#[test]
fn an_extreme_state_runs_or_fails_and_never_panics_or_hangs() {
    for release in RELEASES {
        for (name, change) in extreme_changes() {
            let mut state = walker();
            change(&mut state);
            let version = release.to_string();
            let outcome = in_time(10, move || {
                let mut world = floor();
                simulate_versioned_tick(version, &mut world, state, config()).map(|_| ()).map_err(|error| error.reason)
            });
            assert!(outcome.is_some(), "{release} {name} did not end");
        }
    }
}

#[test]
fn what_a_tick_cannot_walk_is_refused_with_what_is_wrong() {
    let refused = |change: fn(&mut NativeSimulationState)| {
        let mut state = walker();
        change(&mut state);
        simulate_versioned_tick("1.21.1".to_string(), &mut floor(), state, config())
            .err()
            .map(|error| error.reason)
            .expect("the call fails")
    };
    assert!(refused(|state| state.position.x = 1.0e300).contains("position.x"));
    assert!(refused(|state| state.position.z = f64::NEG_INFINITY).contains("position.z"));
    assert!(refused(|state| state.half_width = -0.5).contains("halfWidth"));
    assert!(refused(|state| state.height = f64::NAN).contains("height"));
    assert!(refused(|state| state.half_width = 1.0e6).contains("block cells"));
    assert!(refused(|state| {
        state.bounding_box = Some(NativeAabb { min: vec3(1.0, 1.0, 1.0), max: vec3(0.0, 2.0, 2.0) });
    })
    .contains("boundingBox"));
    assert!(refused(|state| {
        state.bounding_box = Some(NativeAabb { min: vec3(-1.0e5, 1.0, -1.0e5), max: vec3(1.0e5, 2.0, 1.0e5) });
    })
    .contains("boundingBox"));
    // every way to a large move ends at the same refusal
    assert!(refused(|state| state.velocity = vec3(1.0e9, 0.0, 0.0)).contains("the tick's move"));
    assert!(refused(|state| state.velocity = vec3(f64::INFINITY, 0.0, 0.0)).contains("the tick's move"));
    assert!(refused(|state| {
        state.look = Some(vec3(1.0e9, 0.0, 0.0));
        state.fall_flying = Some(true);
        state.firework_rockets = Some(1);
        state.firework_rockets_before_player = Some(1);
    })
    .contains("the tick's move"));
    assert!(refused(|state| state.firework_rockets = Some(MAX_ATTACHED_ROCKETS + 1)).contains("fireworkRockets"));
}

#[test]
fn large_values_a_game_can_reach_still_run() {
    let runs = |change: fn(&mut NativeSimulationState)| {
        for release in RELEASES {
            let mut state = walker();
            change(&mut state);
            simulate_versioned_tick(release.to_string(), &mut floor(), state, config()).expect(release);
        }
    };
    runs(|_| {});
    // the edge of the world, and past it as a client can be
    runs(|state| state.position = vec3(29_999_999.5, 1.0, -29_999_999.5));
    runs(|state| state.position = vec3(1.0e9, 1.0, 1.0e9));
    // a move of a thousand blocks a tick, and of a hundred on each axis
    runs(|state| state.velocity = vec3(1000.0, 0.0, 0.0));
    runs(|state| state.velocity = vec3(100.0, 100.0, -100.0));
    // sizes of no extent are legal, and a motion that is not finite is the engine's to handle
    runs(|state| state.half_width = 0.0);
    runs(|state| state.height = 0.0);
    runs(|state| state.velocity = vec3(f64::NAN, 0.0, 0.0));
    runs(|state| state.position = vec3(f64::NAN, 1.0, 0.5));
    runs(|state| {
        state.bounding_box = Some(NativeAabb { min: vec3(f64::NAN, 1.0, 0.2), max: vec3(f64::NAN, 2.8, 0.8) });
    });
}

#[test]
fn a_world_query_past_what_a_call_walks_is_refused_at_once() {
    let world = in_time(10, || {
        let world = floor();
        for huge in [1.0e300, 2_147_483_648.0, 1.0e6] {
            let (min, max) = (vec3(-huge, -huge, -huge), vec3(huge, huge, huge));
            let reasons = [
                world.collision_shapes(min, max, None).err().expect("a refusal").reason,
                world.outline_shapes(min, max, None).err().expect("a refusal").reason,
                world.interaction_shapes(min, max, None).err().expect("a refusal").reason,
            ];
            for (kind, reason) in ["collision", "outline", "interaction"].iter().zip(reasons) {
                assert!(reason.starts_with(kind), "{reason}");
            }
        }
        assert!(world.collision_shapes(vec3(f64::NAN, 0.0, 0.0), vec3(1.0, 1.0, 1.0), None).is_err());
        assert!(world.collision_shapes(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 1.0), None).is_err());
        world
    })
    .expect("a query ends");
    // what fits goes on working, at the far end of the coordinates too
    let shapes = world.collision_shapes(vec3(-4.0, 0.0, -4.0), vec3(4.0, 2.0, 4.0), None).expect("it runs");
    assert_eq!(shapes.len(), 64);
    let far = 1_000_000_000.0;
    assert!(world.collision_shapes(vec3(far, 0.0, far), vec3(far + 16.0, 256.0, far + 16.0), None).expect("it runs").is_empty());
    assert!(world.outline_shapes(vec3(-far, 0.0, -far), vec3(-far + 16.0, 256.0, -far + 16.0), None).expect("it runs").is_empty());
}

#[test]
fn a_batch_of_block_updates_goes_in_whole_or_not_at_all() {
    let bad_fluid = |x| NativeBlockUpdate { fluid: Some(9), ..block(x, 2, 0) };
    let empty_entity = |x| NativeBlockUpdate { block_entity: Some(String::new()), ..block(x, 2, 0) };
    let bad_release = |x| NativeBlockUpdate { state_version: Some("9.9.9".to_string()), ..block(x, 2, 0) };
    let removal = || NativeBlockUpdate { remove: Some(true), ..block(0, 0, 0) };
    for (bad, reason) in [
        (bad_fluid(3), "fluid must be"),
        (empty_entity(3), "block entity kind"),
        (bad_release(3), "9.9.9"),
    ] {
        let mut world = floor();
        let error = world
            .apply_block_updates(vec![removal(), block(1, 2, 0), bad])
            .expect_err("the batch is refused");
        assert!(error.reason.contains("update 2 at (3, 2, 0)"), "{}", error.reason);
        assert!(error.reason.contains(reason), "{}", error.reason);
        // the removal before it did not happen, nor the block after the batch's start
        assert!(world.block_at(0, 0, 0).present);
        assert!(!world.block_at(1, 2, 0).present);
    }
}

#[test]
fn world_bounds_say_which_of_them_is_wrong() {
    let mut world = floor();
    let reason = |result: napi::Result<()>| result.expect_err("refused").reason;
    assert!(reason(world.set_world_bounds(10, 10, 0.0, 0.0, 100.0, None)).contains("minBuildHeight"));
    assert!(reason(world.set_world_bounds(0, 10, f64::NAN, 0.0, 100.0, None)).contains("borderCenterX"));
    assert!(reason(world.set_world_bounds(0, 10, 0.0, f64::INFINITY, 100.0, None)).contains("borderCenterZ"));
    assert!(reason(world.set_world_bounds(0, 10, 0.0, 0.0, -1.0, None)).contains("borderSize"));
    // a size of zero is legal
    world.set_world_bounds(0, 10, 0.0, 0.0, 0.0, None).expect("a border of no size");
}

#[test]
fn entities_that_ride_each_other_in_a_ring_end() {
    let entity = |id: u32, vehicle: u32| NativeVersionedEntityState {
        simulation: walker(),
        no_physics: false,
        id: Some(id),
        vehicle_id: Some(vehicle),
        pushable: true,
        pushthrough: 0.0,
    };
    for release in RELEASES {
        let version = release.to_string();
        let states = in_time(10, move || {
            let world = floor();
            let ring = vec![entity(1, 2), entity(2, 3), entity(3, 1), entity(4, 4)];
            simulate_versioned_entity_ticks(version, &world, ring, 3, config()).map_err(|error| error.reason)
        })
        .expect("a ring of vehicles ends");
        assert_eq!(states.expect("it runs").len(), 4);
    }
}
