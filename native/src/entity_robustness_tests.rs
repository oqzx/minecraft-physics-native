//! What extreme input does at the boat, ridden mob, projectile, moving entity and minecart bindings: the
//! call runs or fails, and never panics (which aborts a release build's whole process) or hangs. Each
//! case runs on its own thread, so a panic or a hang is reported with its name.

use std::cell::RefCell;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::minecart::{TAG_ACTIVATOR_RAIL, TAG_POWERED_RAIL, TAG_RAIL};
use crate::napi_api::{
    native_boat_state, native_ridden_mob_state, simulate_arrow_tick, simulate_boat_tick,
    simulate_horse_tick, simulate_projectile_tick, simulate_trident_tick,
    simulate_versioned_arrow_ticks, simulate_versioned_boat_ticks, simulate_versioned_horse_ticks,
    simulate_versioned_projectile_ticks, simulate_versioned_trident_ticks, NativeAabb,
    NativeArrowState, NativeBlockProperty, NativeBlockUpdate, NativeBoatState, NativeHorseState,
    NativeProjectileState, NativeTridentState, NativeVec3, NativeWorld,
};
use crate::napi_minecart::{
    simulate_minecart_client_lerp, simulate_minecart_client_steps,
    simulate_versioned_minecart_push, simulate_versioned_minecart_scene,
    simulate_versioned_minecart_tick, NativeMinecartClientSteps, NativeMinecartLerp,
    NativeMinecartOther, NativeMinecartRider, NativeMinecartRules, NativeMinecartState,
    NativeMinecartStep,
};
use crate::napi_moving_entity::{
    simulate_versioned_mob_travel, simulate_versioned_moving_entity_tick, NativeMobTravelState,
    NativeMovingEntityInputs, NativeMovingEntityMovement, NativeMovingEntityPlayer,
    NativeMovingEntityState,
};
use crate::napi_projectile::{
    simulate_versioned_arrow_hit_tick, simulate_versioned_projectile_hit_tick,
    simulate_versioned_trident_hit_tick, NativeProjectileHitState,
    NativeProjectileOwner, NativeProjectileTarget,
};
use crate::ridden::{RiddenMobKind, RiddenMobState};
use crate::robustness_tests::{block, floor, vec3};
use crate::vehicle::BoatSimulationState;
use crate::Vec3;

const RELEASES: [&str; 6] = ["1.8.8", "1.13.2", "1.16.5", "1.21.1", "1.21.4", "26.3"];
/// For the families whose many cases differ by kind more than by release: each side of the 1.14 move and 1.21.2 effects.
const SOME_RELEASES: [&str; 3] = ["1.13.2", "1.21.4", "26.3"];
const SECONDS: u64 = 30;
const VALUES: [f64; 10] = [
    0.0,
    -0.0,
    5.0e-324,
    1.0e2,
    1.0e9,
    -2_147_483_649.0,
    1.0e300,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
];
const INTS: [i32; 5] = [0, 1, -1, i32::MAX, i32::MIN];

fn run<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (sender, receiver) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || {
            let _ = sender.send(call());
        })
        .expect("a thread");
    match receiver.recv_timeout(Duration::from_secs(SECONDS)) {
        Ok(value) => Ok(value),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("hung".to_owned()),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let panic = thread.join().err();
            let text = panic.and_then(|panic| {
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
            });
            Err(format!("panicked: {text:?}"))
        }
    }
}

/// The cases of one family: what went wrong, by name. A family with hangs stops early, as each leaves
/// its thread running.
#[derive(Default)]
struct Probe {
    failures: Vec<String>,
    hangs: u32,
    ran: u32,
}

impl Probe {
    fn case<T: Send + 'static>(&mut self, name: String, call: impl FnOnce() -> napi::Result<T> + Send + 'static) {
        if self.hangs >= 6 {
            return;
        }
        self.ran += 1;
        match run(call) {
            Ok(_) => {}
            Err(reason) => {
                if reason == "hung" {
                    self.hangs += 1;
                }
                self.failures.push(format!("{name}: {reason}"));
            }
        }
    }

    fn finish(self, family: &str) {
        eprintln!("{family}: {} cases", self.ran);
        assert!(self.failures.is_empty(), "{family}:\n{}", self.failures.join("\n"));
    }
}

fn water(x: i32, y: i32, z: i32) -> NativeBlockUpdate {
    NativeBlockUpdate {
        id: Some(9),
        shape: None,
        fluid: Some(1),
        fluid_level: Some(8),
        fluid_source: Some(true),
        ..block(x, y, z)
    }
}

fn rail(x: i32, y: i32, z: i32, tags: u32, shape: &str, powered: bool) -> NativeBlockUpdate {
    NativeBlockUpdate {
        id: Some(66),
        shape: None,
        tags: Some(tags),
        properties: Some(vec![
            NativeBlockProperty { name: "shape".to_owned(), value: shape.to_owned() },
            NativeBlockProperty { name: "powered".to_owned(), value: powered.to_string() },
        ]),
        ..block(x, y, z)
    }
}

fn scene() -> Arc<NativeWorld> {
    Arc::new(scene_world())
}

/// For the calls that give a world their colliders for the tick: one tick at a time has it.
fn locked_scene() -> Arc<Mutex<NativeWorld>> {
    Arc::new(Mutex::new(scene_world()))
}

/// A floor, a pool at z > 8 and a track along x at y 1.
fn scene_world() -> NativeWorld {
    let mut world = floor();
    let mut updates: Vec<_> = (-8..=8).flat_map(|x| (-8..=8).map(move |z| block(x, 0, z))).collect();
    updates.extend((-8..=8).flat_map(|x| (9..16).flat_map(move |z| (1..4).map(move |y| water(x, y, z)))));
    for x in -8..=8 {
        updates.push(match x {
            -2 => rail(x, 1, 0, TAG_RAIL | TAG_POWERED_RAIL, "east_west", true),
            2 => rail(x, 1, 0, TAG_RAIL | TAG_ACTIVATOR_RAIL, "east_west", true),
            3 => rail(x, 1, 0, TAG_RAIL, "ascending_east", false),
            _ => rail(x, 1, 0, TAG_RAIL, "east_west", false),
        });
    }
    world.apply_block_updates(updates).expect("the scene goes in");
    world
}

type Vector = fn(&mut NativeVec3, f64);

fn axes() -> [(&'static str, Vector); 4] {
    [
        ("x", |vector, value| vector.x = value),
        ("y", |vector, value| vector.y = value),
        ("z", |vector, value| vector.z = value),
        ("all", |vector, value| *vector = vec3(value, value, value)),
    ]
}

fn boxes(value: f64) -> [(&'static str, NativeAabb); 4] {
    [
        ("wide", NativeAabb { min: vec3(-value, 1.0, -value), max: vec3(value, 2.0, value) }),
        ("tall", NativeAabb { min: vec3(0.2, -value, 0.2), max: vec3(0.8, value, 0.8) }),
        ("at", NativeAabb { min: vec3(value, value, value), max: vec3(value, value, value) }),
        ("inverted", NativeAabb { min: vec3(value, 2.0, value), max: vec3(-value, 1.0, -value) }),
    ]
}

/// Every extreme of a state's fields.
fn changes<S: 'static>(
    vectors: &[(&'static str, fn(&mut S) -> &mut NativeVec3)],
    numbers: &[(&'static str, fn(&mut S, f64))],
    ints: &[(&'static str, fn(&mut S, i32))],
    bounding_box: Option<fn(&mut S, NativeAabb)>,
) -> Vec<(String, Box<dyn Fn(&mut S) + Send + Sync>)> {
    let mut cases: Vec<(String, Box<dyn Fn(&mut S) + Send + Sync>)> = Vec::new();
    for value in VALUES {
        for &(name, get) in vectors {
            for (axis, set) in axes() {
                cases.push((format!("{name}.{axis} {value}"), Box::new(move |state| set(get(state), value))));
            }
        }
        for &(name, set) in numbers {
            cases.push((format!("{name} {value}"), Box::new(move |state| set(state, value))));
        }
        if let Some(set) = bounding_box {
            for (name, bounds) in boxes(value) {
                cases.push((format!("box {name} {value}"), Box::new(move |state| set(state, bounds.clone()))));
            }
        }
    }
    // (the edge of the world, for a position: as a motion it is a slow walk and not an extreme one)
    for value in [-3.0e7, 3.0e7] {
        for &(name, get) in vectors.iter().filter(|(name, _)| name.contains("position")) {
            for (axis, set) in axes() {
                cases.push((format!("{name}.{axis} {value}"), Box::new(move |state| set(get(state), value))));
            }
        }
    }
    for value in INTS {
        for &(name, set) in ints {
            cases.push((format!("{name} {value}"), Box::new(move |state| set(state, value))));
        }
    }
    cases
}

#[test]
fn a_boat_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = locked_scene();
    let base = |position: Vec3, in_water: bool| -> NativeBoatState {
        native_boat_state(BoatSimulationState {
            position,
            velocity: Vec3::new(0.1, -0.04, 0.1),
            forward: true,
            left: true,
            in_water,
            water_level: if in_water { 3.9 } else { f64::NEG_INFINITY },
            ..BoatSimulationState::default()
        })
    };
    let cases = changes::<NativeBoatState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("stuck", |state| state.stuck_speed_multiplier.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("yaw", |state, value| state.yaw = value),
            ("yaw degrees", |state, value| state.yaw_degrees = Some(value)),
            ("delta rotation", |state, value| state.delta_rotation = Some(value)),
            ("water level", |state, value| state.water_level = Some(value)),
            ("land friction", |state, value| state.land_friction = Some(value)),
            ("last vertical", |state, value| state.last_vertical_velocity = Some(value)),
            ("speed multiplier", |state, value| state.speed_multiplier = Some(value)),
            ("rider yaw", |state, value| state.rider_yaw = Some(value)),
            ("rider forward", |state, value| state.rider_forward = Some(value)),
            ("rider strafe", |state, value| state.rider_strafe = Some(value)),
            ("paddles", |state, value| state.paddle_phases = Some(vec![value, value])),
        ],
        &[("bubble time", |state, value| state.bubble_time = Some(value))],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    );
    let mut probe = Probe::default();
    for (where_, position, in_water) in [("land", Vec3::new(0.5, 1.0, 0.5), false), ("pool", Vec3::new(0.5, 3.5, 12.5), true)] {
        for (name, change) in &cases {
            let mut state = base(position, in_water);
            change(&mut state);
            let copy = NativeBoatState { ..native_copy(&state) };
            probe.case(format!("no world {where_} {name}"), move || simulate_boat_tick(copy));
            for release in RELEASES {
                let copy = native_copy(&state);
                let world = world.clone();
                probe.case(format!("{release} {where_} {name}"), move || {
                    let mut world = world.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    simulate_versioned_boat_ticks(release.to_owned(), &mut world, vec![copy], 2)
                });
            }
        }
    }
    probe.finish("boat");
}

/// A boat's fields, which N-API values do not clone.
fn native_copy(state: &NativeBoatState) -> NativeBoatState {
    NativeBoatState {
        position: state.position,
        velocity: state.velocity,
        status: state.status.clone(),
        previous_status: state.previous_status.clone(),
        bounding_box: state.bounding_box.clone(),
        stuck_speed_multiplier: state.stuck_speed_multiplier,
        paddle_phases: state.paddle_phases.clone(),
        nearby_entities: None,
        ..*state
    }
}

#[test]
fn a_ridden_mob_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = locked_scene();
    let cases = Arc::new(changes::<NativeHorseState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("previous", |state| state.previous_position.get_or_insert(vec3(0.0, 1.0, 0.0))),
            ("lerp", |state| state.lerp_position.get_or_insert(vec3(0.0, 1.0, 0.0))),
            ("stuck", |state| state.stuck_speed_multiplier.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("yaw", |state, value| state.yaw = value),
            ("forward", |state, value| state.forward = value),
            ("strafe", |state, value| state.strafe = value),
            ("speed", |state, value| state.movement_speed = Some(value)),
            ("jump", |state, value| state.jump_strength = Some(value)),
            ("half width", |state, value| state.half_width = Some(value)),
            ("height", |state, value| state.height = Some(value)),
            ("yaw degrees", |state, value| state.yaw_degrees = Some(value)),
            ("pitch degrees", |state, value| state.pitch_degrees = Some(value)),
            ("rider yaw", |state, value| state.rider_yaw = Some(value)),
            ("rider pitch", |state, value| state.rider_pitch = Some(value)),
            ("water height", |state, value| state.water_height = Some(value)),
            ("fall distance", |state, value| state.fall_distance = Some(value)),
            ("flying speed", |state, value| state.flying_speed = Some(value)),
            ("ridden speed", |state, value| state.ridden_speed = Some(value)),
            ("own forward", |state, value| state.own_forward = Some(value)),
            ("suffocating speed", |state, value| {
                state.suffocating = Some(true);
                state.suffocating_speed = Some(value);
            }),
            ("jump scale", |state, value| state.jump_pending_scale = Some(value)),
            ("gravity", |state, value| state.gravity = Some(value)),
            ("step", |state, value| state.step_height = Some(value)),
            ("efficiency", |state, value| state.water_movement_efficiency = Some(value)),
            ("air drag", |state, value| state.air_drag_modifier = Some(value)),
            ("friction", |state, value| state.friction_modifier = Some(value)),
        ],
        &[
            ("rider jump", |state, value| state.rider_jump = Some(value)),
            ("boost time", |state, value| {
                state.boosting = Some(true);
                state.boost_time = Some(value);
            }),
            ("boost total", |state, value| {
                state.boosting = Some(true);
                state.boost_time_total = Some(value);
            }),
            ("stand counter", |state, value| state.stand_counter = Some(value)),
            ("dash cooldown", |state, value| state.dash_cooldown = Some(value)),
            ("jump boost", |state, value| state.jump_boost_amplifier = Some(value)),
            ("levitation", |state, value| state.levitation_amplifier = Some(value)),
            ("pose time", |state, value| state.pose_time = Some(i64::from(value) * (1 << 32))),
        ],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    ));
    let mobs = [
        ("horse", RiddenMobKind::Horse),
        ("pig", RiddenMobKind::Pig),
        ("strider", RiddenMobKind::Strider),
        ("camel", RiddenMobKind::Camel),
        ("happy_ghast", RiddenMobKind::HappyGhast),
        ("nautilus", RiddenMobKind::Nautilus),
        ("llama", RiddenMobKind::Llama),
    ];
    let mut probe = Probe::default();
    for release in SOME_RELEASES {
        let parsed = crate::version::MinecraftVersion::parse(release).expect("a release");
        for (name, kind) in mobs {
            if RiddenMobKind::parse_for(name, parsed).is_err() {
                continue;
            }
            for (at, position, jumping) in [("land", Vec3::new(0.5, 1.0, 0.5), true), ("pool", Vec3::new(0.5, 2.5, 12.5), false)] {
                for index in 0..cases.len() {
                    let (world, cases) = (world.clone(), cases.clone());
                    probe.case(format!("{release} {name} {at} {}", cases[index].0), move || {
                        let mut mob = RiddenMobState::new(kind, position);
                        mob.on_ground = jumping;
                        mob.rider.forward = 1.0;
                        mob.rider.jumping = jumping;
                        mob.saddled = true;
                        let mut state = native_ridden_mob_state(mob, parsed);
                        state.entity_type = Some(name.to_owned());
                        (cases[index].1)(&mut state);
                        let mut world = world.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        simulate_versioned_horse_ticks(release.to_owned(), &mut world, vec![state], 2)
                    });
                }
            }
        }
    }
    for index in 0..cases.len() {
        let cases = cases.clone();
        probe.case(format!("no world {}", cases[index].0), move || {
            let mut state = native_ridden_mob_state(RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 1.0, 0.5)), crate::version::MinecraftVersion::new(1, 20, 4));
            (cases[index].1)(&mut state);
            simulate_horse_tick(state)
        });
    }
    probe.finish("ridden mob");
}

fn projectile(kind: &str) -> NativeProjectileState {
    NativeProjectileState {
        bounding_box: None,
        position: vec3(0.5, 2.5, 0.5),
        velocity: vec3(0.3, -0.5, 0.1),
        gravity: None,
        air_drag: None,
        power: None,
        acceleration_power: None,
        dangerous: None,
        in_water: None,
        ultra_warm: None,
        first_tick: None,
        kind: Some(kind.to_owned()),
        horizontal_collision: None,
        on_ground: None,
        shot_at_angle: None,
        bobbing: None,
        bob_random: None,
        no_physics: None,
        stuck_speed_multiplier: None,
    }
}

#[test]
fn a_projectile_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = scene();
    let cases = Arc::new(changes::<NativeProjectileState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("power", |state| state.power.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("gravity", |state, value| state.gravity = Some(value)),
            ("air drag", |state, value| state.air_drag = Some(value)),
            ("acceleration", |state, value| state.acceleration_power = Some(value)),
            ("bob random", |state, value| {
                state.bobbing = Some(true);
                state.bob_random = Some(value);
            }),
        ],
        &[],
        None,
    ));
    let mut probe = Probe::default();
    for release in SOME_RELEASES {
        for kind in [
            "snowball", "egg", "ender_pearl", "potion", "experience_bottle", "firework_rocket", "fishing_bobber",
            "small_fireball", "fireball", "wither_skull", "dragon_fireball", "wind_charge", "llama_spit", "shulker_bullet",
        ] {
            for index in 0..cases.len() {
                let (world, cases) = (world.clone(), cases.clone());
                probe.case(format!("{release} {kind} {}", cases[index].0), move || {
                    let mut state = projectile(kind);
                    (cases[index].1)(&mut state);
                    simulate_versioned_projectile_ticks(release.to_owned(), &world, vec![state], 2)
                });
            }
        }
    }
    for index in 0..cases.len() {
        let (world, cases) = (world.clone(), cases.clone());
        probe.case(format!("no release {}", cases[index].0), move || {
            let mut state = projectile("snowball");
            (cases[index].1)(&mut state);
            simulate_projectile_tick(&world, state)
        });
    }
    probe.finish("projectile");
}

fn arrow() -> NativeArrowState {
    NativeArrowState {
        position: vec3(0.5, 2.5, 0.5),
        velocity: vec3(0.3, -0.5, 0.1),
        gravity: None,
        air_drag: None,
        water_inertia: None,
        in_water: None,
        in_ground: None,
        life: None,
        in_ground_time: None,
        shake_time: None,
        fall_impulse: None,
        last_block_id: None,
        last_block_position: None,
        no_physics: None,
        ultra_warm: None,
    }
}

fn trident() -> NativeTridentState {
    NativeTridentState {
        position: vec3(0.5, 2.5, 0.5),
        velocity: vec3(0.3, -0.5, 0.1),
        gravity: None,
        air_drag: None,
        water_inertia: None,
        in_water: None,
        dealt_damage: None,
        in_ground: None,
        life: None,
        in_ground_time: None,
        shake_time: None,
        fall_impulse: None,
        last_block_id: None,
        last_block_position: None,
        no_physics: None,
        ultra_warm: None,
    }
}

fn target(id: i32) -> NativeProjectileTarget {
    NativeProjectileTarget {
        id,
        root_vehicle: None,
        position: vec3(1.0, 2.0, 0.5),
        bounding_box: None,
        width: Some(0.6),
        height: Some(1.8),
        scale: None,
        pickable: None,
        alive: None,
        spectator: None,
        friendly: None,
        invulnerable_player: None,
        hurt: None,
        enderman: None,
        deflects: None,
        deflect_random: None,
        no_physics: None,
        item: None,
        wind_charge: None,
        end_crystal: None,
        dragon_box: None,
    }
}

fn hits() -> NativeProjectileHitState {
    NativeProjectileHitState {
        tick_count: Some(10),
        flying_ticks: Some(10),
        left_owner: Some(true),
        pierce_level: Some(0),
        pierced: Some(Vec::new()),
        last_deflected_by: None,
        base_damage: None,
        loyalty: None,
        pickup_allowed: None,
        ignored: None,
        ignore_time: None,
        life: None,
        life_time: None,
        explodes: None,
        attached: None,
        holder: None,
        in_ground: None,
        ground_release: None,
        hooked: None,
        hook_tracked: None,
        hook_settled: None,
        stop_fishing: None,
        bullet_delta: None,
        chunk_unloaded: None,
        outside_border: None,
        peaceful: None,
        pearl_outlives_owner: None,
    }
}

fn owner() -> NativeProjectileOwner {
    NativeProjectileOwner {
        id: 7,
        root_vehicle: None,
        eye_position: vec3(0.5, 3.0, 0.5),
        width: 0.6,
        alive: true,
        player: true,
        spectator: None,
        removed: None,
        won_game: None,
    }
}

#[test]
fn an_arrow_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let level = scene();
    let arrows = Arc::new(changes::<NativeArrowState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("impulse", |state| state.fall_impulse.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("gravity", |state, value| state.gravity = Some(value)),
            ("air drag", |state, value| state.air_drag = Some(value)),
            ("inertia", |state, value| state.water_inertia = Some(value)),
        ],
        &[("in ground", |state, value| {
            state.in_ground = Some(value != 0);
            state.last_block_id = Some(value as u32);
            state.last_block_position = Some(crate::napi_api::NativeBlockPosition { x: value, y: value, z: value });
        })],
        None,
    ));
    let tridents = Arc::new(changes::<NativeTridentState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("impulse", |state| state.fall_impulse.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("gravity", |state, value| state.gravity = Some(value)),
            ("air drag", |state, value| state.air_drag = Some(value)),
            ("inertia", |state, value| state.water_inertia = Some(value)),
        ],
        &[("life", |state, value| state.life = Some(value as u16))],
        None,
    ));
    let mut probe = Probe::default();
    for release in RELEASES {
        for index in 0..arrows.len() {
            let (world, cases) = (level.clone(), arrows.clone());
            probe.case(format!("{release} arrow {}", cases[index].0), move || {
                let mut state = arrow();
                (cases[index].1)(&mut state);
                simulate_versioned_arrow_ticks(release.to_owned(), &world, vec![state], 2)
            });
            let (world, cases) = (level.clone(), arrows.clone());
            probe.case(format!("{release} arrow among targets {}", cases[index].0), move || {
                let mut state = arrow();
                (cases[index].1)(&mut state);
                simulate_versioned_arrow_hit_tick(release.to_owned(), &world, state, Some(hits()), vec![target(1), target(2)], Some(owner())).map(|_| ())
            });
        }
        if release == "1.8.8" {
            continue;
        }
        for index in 0..tridents.len() {
            let (world, cases) = (level.clone(), tridents.clone());
            probe.case(format!("{release} trident {}", cases[index].0), move || {
                let mut state = trident();
                (cases[index].1)(&mut state);
                simulate_versioned_trident_ticks(release.to_owned(), &world, vec![state], 2)
            });
            let (world, cases) = (level.clone(), tridents.clone());
            probe.case(format!("{release} trident among targets {}", cases[index].0), move || {
                let mut state = trident();
                (cases[index].1)(&mut state);
                let hit = NativeProjectileHitState { loyalty: Some(255), ..hits() };
                simulate_versioned_trident_hit_tick(release.to_owned(), &world, state, Some(hit), vec![target(1)], Some(owner())).map(|_| ())
            });
        }
    }
    for index in 0..arrows.len() {
        let (world, cases) = (level.clone(), arrows.clone());
        probe.case(format!("no release arrow {}", cases[index].0), move || {
            let mut state = arrow();
            (cases[index].1)(&mut state);
            simulate_arrow_tick(&world, state).map(|_| ())
        });
    }
    for index in 0..tridents.len() {
        let (world, cases) = (level.clone(), tridents.clone());
        probe.case(format!("no release trident {}", cases[index].0), move || {
            let mut state = trident();
            (cases[index].1)(&mut state);
            simulate_trident_tick(&world, state).map(|_| ())
        });
    }
    probe.finish("arrow");
}

#[test]
fn what_a_projectile_meets_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let level = scene();
    let targets = Arc::new(changes::<NativeProjectileTarget>(
        &[("position", |state| &mut state.position)],
        &[
            ("width", |state, value| state.width = Some(value)),
            ("height", |state, value| state.height = Some(value)),
            ("scale", |state, value| state.scale = Some(value)),
            ("deflect random", |state, value| {
                state.deflects = Some(true);
                state.deflect_random = Some(value);
            }),
        ],
        &[
            ("id", |state, value| state.id = value),
            ("root", |state, value| state.root_vehicle = Some(value)),
        ],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    ));
    let states = Arc::new(changes::<NativeProjectileHitState>(
        &[],
        &[("base damage", |state, value| state.base_damage = Some(value))],
        &[
            ("tick count", |state, value| state.tick_count = Some(value)),
            ("flying ticks", |state, value| state.flying_ticks = Some(value)),
            ("pierced", |state, value| state.pierced = Some(vec![value, 1])),
            ("deflected by", |state, value| state.last_deflected_by = Some(value)),
            ("pierce", |state, value| state.pierce_level = Some(value as u8)),
        ],
        None,
    ));
    let owners = Arc::new(changes::<NativeProjectileOwner>(
        &[("eye", |state| &mut state.eye_position)],
        &[("width", |state, value| state.width = value)],
        &[("id", |state, value| state.id = value)],
        None,
    ));
    let mut probe = Probe::default();
    for release in RELEASES {
        for index in 0..targets.len() {
            let (world, cases) = (level.clone(), targets.clone());
            probe.case(format!("{release} target {}", cases[index].0), move || {
                let mut victim = target(1);
                (cases[index].1)(&mut victim);
                let state = NativeArrowState { velocity: vec3(1.0, 0.0, 0.0), ..arrow() };
                simulate_versioned_arrow_hit_tick(release.to_owned(), &world, state, Some(hits()), vec![victim, target(2)], Some(owner())).map(|_| ())
            });
        }
        for index in 0..states.len() {
            let (world, cases) = (level.clone(), states.clone());
            probe.case(format!("{release} hit state {}", cases[index].0), move || {
                let mut state = hits();
                (cases[index].1)(&mut state);
                let shot = NativeArrowState { velocity: vec3(1.0, 0.0, 0.0), ..arrow() };
                simulate_versioned_arrow_hit_tick(release.to_owned(), &world, shot, Some(state), vec![target(1), target(2)], Some(owner())).map(|_| ())
            });
        }
        for index in 0..owners.len() {
            let (world, cases) = (level.clone(), owners.clone());
            probe.case(format!("{release} owner {}", cases[index].0), move || {
                let mut who = owner();
                (cases[index].1)(&mut who);
                let shot = NativeArrowState { velocity: vec3(1.0, 0.0, 0.0), ..arrow() };
                simulate_versioned_arrow_hit_tick(release.to_owned(), &world, shot, Some(hits()), vec![target(1)], Some(who)).map(|_| ())
            });
            if release == "1.8.8" {
                continue;
            }
            let (world, cases) = (level.clone(), owners.clone());
            probe.case(format!("{release} returning to owner {}", cases[index].0), move || {
                let mut who = owner();
                (cases[index].1)(&mut who);
                let state = NativeTridentState { dealt_damage: Some(true), ..trident() };
                let hit = NativeProjectileHitState { loyalty: Some(255), ..hits() };
                simulate_versioned_trident_hit_tick(release.to_owned(), &world, state, Some(hit), Vec::new(), Some(who)).map(|_| ())
            });
        }
    }
    probe.finish("projectile targets");
}

#[test]
fn the_server_tick_of_the_other_projectiles_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = scene();
    let shots = Arc::new(changes::<NativeProjectileState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("power", |state| state.power.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("gravity", |state, value| state.gravity = Some(value)),
            ("air drag", |state, value| state.air_drag = Some(value)),
            ("acceleration", |state, value| state.acceleration_power = Some(value)),
        ],
        &[],
        None,
    ));
    let states = Arc::new(changes::<NativeProjectileHitState>(
        &[
            ("ground release", |state| state.ground_release.get_or_insert(vec3(0.1, 0.1, 0.1))),
            ("bullet delta", |state| state.bullet_delta.get_or_insert(vec3(0.1, 0.1, 0.1))),
        ],
        &[],
        &[
            ("ignored", |state, value| state.ignored = Some(value)),
            ("ignore time", |state, value| state.ignore_time = Some(value)),
            ("life", |state, value| state.life = Some(value)),
            ("life time", |state, value| state.life_time = Some(value)),
            ("hooked", |state, value| state.hooked = Some(value)),
            ("hook tracked", |state, value| state.hook_tracked = Some(value)),
            ("tick count", |state, value| state.tick_count = Some(value)),
            ("flying ticks", |state, value| state.flying_ticks = Some(value)),
        ],
        None,
    ));
    let hooked_targets = Arc::new(changes::<NativeProjectileTarget>(
        &[("position", |state| &mut state.position)],
        &[("height", |state, value| state.height = Some(value))],
        &[],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    ));
    let mut probe = Probe::default();
    for release in SOME_RELEASES.iter().chain(&["1.8.8"]).copied() {
        for kind in ["snowball", "ender_pearl", "firework_rocket", "fishing_bobber", "fireball", "wither_skull", "wind_charge", "llama_spit", "shulker_bullet"] {
            for in_ground in [false, true] {
                for index in 0..shots.len() {
                    let (world, cases) = (world.clone(), shots.clone());
                    probe.case(format!("{release} {kind} ground {in_ground} {}", cases[index].0), move || {
                        let mut state = projectile(kind);
                        (cases[index].1)(&mut state);
                        let hit = NativeProjectileHitState { in_ground: Some(in_ground), ..hits() };
                        simulate_versioned_projectile_hit_tick(release.to_owned(), &world, state, Some(hit), vec![target(1)], Some(owner())).map(|_| ())
                    });
                }
                for index in 0..states.len() {
                    let (world, cases) = (world.clone(), states.clone());
                    probe.case(format!("{release} {kind} ground {in_ground} hit state {}", cases[index].0), move || {
                        let mut hit = NativeProjectileHitState { in_ground: Some(in_ground), ..hits() };
                        (cases[index].1)(&mut hit);
                        simulate_versioned_projectile_hit_tick(release.to_owned(), &world, projectile(kind), Some(hit), vec![target(1)], Some(owner())).map(|_| ())
                    });
                }
            }
        }
        // a hook rides what it holds to wherever that is
        for index in 0..hooked_targets.len() {
            let (world, cases) = (world.clone(), hooked_targets.clone());
            probe.case(format!("{release} hooked {}", cases[index].0), move || {
                let mut held = target(1);
                (cases[index].1)(&mut held);
                let hit = NativeProjectileHitState { hooked: Some(1), hook_settled: Some(true), ..hits() };
                simulate_versioned_projectile_hit_tick(release.to_owned(), &world, projectile("fishing_bobber"), Some(hit), vec![held], Some(owner())).map(|_| ())
            });
        }
    }
    probe.finish("server projectile");
}

/// That the probes above are at what they mean: what the scene holds is met, and a kind runs its own tick.
#[test]
fn the_probes_reach_the_paths_they_are_for() {
    let world = scene();
    let rules = NativeMinecartRules { improvements: false, max_minecart_speed: None, min_y: None, tnt_explodes: None, nether_exit: None, spawner_blocks_enabled: None, level_random: None };
    let on_rail = simulate_versioned_minecart_tick("1.21.1".to_owned(), &world, cart("minecart"), rules).expect("it runs");
    assert_eq!(on_rail.state.on_rails, Some(true));
    let improved = NativeMinecartRules { improvements: true, max_minecart_speed: Some(8), min_y: None, tnt_explodes: None, nether_exit: None, spawner_blocks_enabled: None, level_random: None };
    let ticked = simulate_versioned_minecart_tick("26.3".to_owned(), &world, cart("minecart"), improved).expect("it runs");
    assert!(!ticked.steps.is_empty(), "the improved path walked its track");
    let mut level = scene_world();
    let afloat = native_boat_state(BoatSimulationState { position: Vec3::new(0.5, 3.5, 12.5), ..BoatSimulationState::default() });
    let afloat = simulate_versioned_boat_ticks("26.3".to_owned(), &mut level, vec![afloat], 2).expect("it runs");
    assert_eq!(afloat[0].status.as_deref(), Some("in_water"));
    let shot = NativeArrowState { velocity: vec3(1.0, 0.0, 0.0), ..arrow() };
    let hit = simulate_versioned_arrow_hit_tick("26.3".to_owned(), &world, shot, Some(hits()), vec![target(1)], Some(owner())).expect("it runs");
    assert_eq!(hit.entity_hits.len(), 1, "the target is on the ray");
    let down = NativeArrowState { velocity: vec3(0.0, -3.0, 0.0), ..arrow() };
    let hit = simulate_versioned_arrow_hit_tick("26.3".to_owned(), &world, down, Some(hits()), Vec::new(), Some(owner())).expect("it runs");
    assert!(hit.block_hit.is_some(), "the floor stops a ray");
    let held = NativeProjectileHitState { hooked: Some(1), hook_settled: Some(true), ..hits() };
    let rode = simulate_versioned_projectile_hit_tick("1.8.8".to_owned(), &world, projectile("fishing_bobber"), Some(held), vec![target(1)], Some(owner())).expect("it runs");
    assert_eq!(rode.projectile.position.x, target(1).position.x, "a hook before 1.11 rides what it holds");
    let drifted = simulate_versioned_moving_entity_tick("26.3".to_owned(), &mut level, moving("item", true), None).expect("it runs");
    assert_ne!(drifted.state.position.y, 2.5);
    let swam = simulate_versioned_mob_travel("26.3".to_owned(), &world, mob("cod")).expect("it runs");
    assert_ne!(swam.position.z, 12.5);
    // a NoAI mob's travel does nothing (1.16.5 LivingEntity.java:1959-1960)
    for release in ["1.8.8", "1.16.5", "26.3"] {
        let still = simulate_versioned_mob_travel(release.to_owned(), &world, NativeMobTravelState { effective_ai: Some(false), ..mob("cow") }).expect(release);
        assert_eq!((still.position.z, still.velocity.y), (mob("cow").position.z, mob("cow").velocity.y), "{release}");
    }
    // (1.9.4 and 1.10.2: canPassengerSteer is true on the server for a mob nobody rides, so it travels)
    let moved = simulate_versioned_mob_travel("1.10.2".to_owned(), &world, NativeMobTravelState { effective_ai: Some(false), ..mob("cow") }).expect("1.10.2");
    assert_ne!(moved.velocity.y, mob("cow").velocity.y);
}

/// The hook before 1.11 is the server's call's alone, and every other kind of the call has its first release.
#[test]
fn a_hook_before_1_11_runs_on_the_server_and_a_late_kind_in_no_call_of_an_earlier_release() {
    let world = scene();
    for release in ["1.8.8", "1.9.4", "1.10.2"] {
        let reason = refusal(simulate_versioned_projectile_ticks(release.to_owned(), &world, vec![projectile("fishing_bobber")], 1));
        assert!(reason.contains("simulateVersionedProjectileHitTick"), "{reason}");
        let flown = simulate_versioned_projectile_hit_tick(release.to_owned(), &world, projectile("fishing_bobber"), Some(hits()), Vec::new(), Some(owner())).expect(release);
        // EntityFishHook.onUpdate's own fall: 0.04F then 0.92F (1.8.8 FishingHook.java tick)
        assert_eq!(flown.projectile.velocity.y, (-0.5 - f64::from(0.04_f32)) * f64::from(0.92_f32), "{release}");
    }
    simulate_versioned_projectile_ticks("1.11.2".to_owned(), &world, vec![projectile("fishing_bobber")], 1).expect("the client's hook");
    for (kind, before) in [("shulker_bullet", "1.8.8"), ("dragon_fireball", "1.8.8"), ("llama_spit", "1.10.2"), ("wind_charge", "1.20.2")] {
        let run = simulate_versioned_projectile_hit_tick(before.to_owned(), &world, projectile(kind), Some(hits()), Vec::new(), Some(owner()));
        assert!(refusal(run).contains("not supported before"), "{kind}");
    }
    let huge = NativeProjectileState { velocity: vec3(1.0e9, 0.0, 0.0), ..projectile("snowball") };
    let run = simulate_versioned_projectile_hit_tick("26.3".to_owned(), &world, huge, Some(hits()), Vec::new(), Some(owner()));
    assert!(refusal(run).contains("flight"));
}

fn moving(kind: &str, server: bool) -> NativeMovingEntityState {
    NativeMovingEntityState {
        kind: kind.to_owned(),
        authority: Some(if server { "server" } else { "client" }.to_owned()),
        id: None,
        tick_count: None,
        position: vec3(0.5, 2.5, 0.5),
        velocity: vec3(0.1, -0.2, 0.1),
        bounding_box: None,
        on_ground: None,
        collided_horizontally: None,
        collided_vertically: None,
        vertical_collision_below: None,
        in_water: None,
        in_lava: None,
        no_gravity: None,
        no_physics: None,
        stuck_speed_multiplier: None,
        fall_distance: None,
        ultra_warm: None,
        first_tick: None,
        last_movements: None,
        removed: None,
        fuse: None,
        time: None,
        carried_block: Some(1),
        carried_air: None,
        concrete_powder: None,
        discarding: None,
        age: None,
        pickup_delay: None,
        item_empty: None,
        fire_immune: None,
        following_player: None,
        following_time: None,
        orb_ticks: None,
    }
}

#[test]
fn a_moving_entity_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = locked_scene();
    let cases = Arc::new(changes::<NativeMovingEntityState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("stuck", |state| state.stuck_speed_multiplier.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("fall distance", |state, value| state.fall_distance = Some(value)),
            ("movement from", |state, value| {
                state.last_movements = Some(vec![NativeMovingEntityMovement { from: vec3(value, value, value), to: vec3(0.5, 2.5, 0.5), axis_order: None }]);
            }),
            ("movement to", |state, value| {
                state.last_movements = Some(vec![NativeMovingEntityMovement { from: vec3(0.5, 2.5, 0.5), to: vec3(value, 2.5, value), axis_order: None }]);
            }),
            ("movement order", |state, value| {
                state.last_movements = Some(vec![NativeMovingEntityMovement { from: vec3(0.5, 2.5, 0.5), to: vec3(0.5, 3.5, 0.5), axis_order: Some(vec3(value, value, value)) }]);
            }),
        ],
        &[
            ("id", |state, value| state.id = Some(value)),
            ("tick count", |state, value| state.tick_count = Some(value)),
            ("fuse", |state, value| state.fuse = Some(value)),
            ("time", |state, value| state.time = Some(value)),
            ("age", |state, value| state.age = Some(value)),
            ("pickup", |state, value| state.pickup_delay = Some(value)),
            ("following", |state, value| state.following_player = Some(value)),
            ("following time", |state, value| state.following_time = Some(value)),
            ("orb ticks", |state, value| state.orb_ticks = Some(value)),
            ("carried", |state, value| state.carried_block = Some(value as u32)),
        ],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    ));
    let players = Arc::new(changes::<NativeMovingEntityPlayer>(
        &[("player", |state| &mut state.position)],
        &[("eye", |state, value| state.eye_height = value)],
        &[("player id", |state, value| state.id = value)],
        None,
    ));
    let mut probe = Probe::default();
    for release in RELEASES {
        for kind in ["tnt", "falling_block", "item", "experience_orb"] {
            for server in [false, true] {
                for index in 0..cases.len() {
                    let (world, cases) = (world.clone(), cases.clone());
                    probe.case(format!("{release} {kind} server {server} {}", cases[index].0), move || {
                        let mut state = moving(kind, server);
                        (cases[index].1)(&mut state);
                        simulate_versioned_moving_entity_tick(release.to_owned(), &mut world.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), state, Some(random_and_a_player())).map(|_| ())
                    });
                }
                for index in 0..players.len() {
                    let (world, cases) = (world.clone(), players.clone());
                    probe.case(format!("{release} {kind} server {server} {}", cases[index].0), move || {
                        let mut player = NativeMovingEntityPlayer { id: 1, position: vec3(1.0, 2.0, 0.5), eye_height: 1.62, spectator: None, dead: None };
                        (cases[index].1)(&mut player);
                        let state = NativeMovingEntityState { following_player: Some(1), ..moving(kind, server) };
                        let inputs = NativeMovingEntityInputs { players: Some(vec![player]), ..random_and_a_player() };
                        simulate_versioned_moving_entity_tick(release.to_owned(), &mut world.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), state, Some(inputs)).map(|_| ())
                    });
                }
            }
        }
    }
    probe.finish("moving entity");
}

fn random_and_a_player() -> NativeMovingEntityInputs {
    NativeMovingEntityInputs {
        random: Some(vec![0.5; 16]),
        players: Some(vec![NativeMovingEntityPlayer { id: 1, position: vec3(1.0, 2.0, 0.5), eye_height: 1.62, spectator: None, dead: None }]),
        falls_through_below: Some(true),
    }
}

fn mob(kind: &str) -> NativeMobTravelState {
    NativeMobTravelState {
        kind: kind.to_owned(),
        position: vec3(0.5, 2.5, 12.5),
        velocity: vec3(0.1, -0.1, 0.1),
        yaw_degrees: 30.0,
        input: vec3(0.0, 0.0, 1.0),
        speed: 0.1,
        half_width: 0.3,
        height: 0.8,
        max_step: None,
        bounding_box: None,
        on_ground: None,
        collided_horizontally: None,
        collided_vertically: None,
        in_water: Some(true),
        in_lava: None,
        no_gravity: None,
        stuck_speed_multiplier: None,
        fall_distance: None,
        has_target: Some(true),
        guardian_moving: Some(true),
        climbing: None,
        pushed_by_fluid: None,
        fluid_jump_threshold: None,
        under_water: None,
        wants_to_swim: None,
        near_home: None,
        marker: None,
        body_item: None,
        floats_in_liquids: None,
        tick_count: None,
        slow_falling: None,
        levitation_amplifier: None,
        dolphins_grace: None,
        gravity: None,
        water_movement_efficiency: None,
        air_drag_modifier: None,
        discard_friction: None,
        effective_ai: None,
    }
}

#[test]
fn a_travelling_mob_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = scene();
    let cases = Arc::new(changes::<NativeMobTravelState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("input", |state| &mut state.input),
            ("stuck", |state| state.stuck_speed_multiplier.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[
            ("yaw", |state, value| state.yaw_degrees = value),
            ("speed", |state, value| state.speed = value),
            ("half width", |state, value| state.half_width = value),
            ("height", |state, value| state.height = value),
            ("step", |state, value| state.max_step = Some(value)),
            ("fall distance", |state, value| state.fall_distance = Some(value)),
        ],
        &[],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    ));
    let mut probe = Probe::default();
    for release in SOME_RELEASES {
        for kind in ["squid", "cod", "guardian", "ghast", "phantom", "allay", "vex"] {
            for in_water in [true, false] {
                for index in 0..cases.len() {
                    let (world, cases) = (world.clone(), cases.clone());
                    probe.case(format!("{release} {kind} water {in_water} {}", cases[index].0), move || {
                        let mut state = NativeMobTravelState { in_water: Some(in_water), ..mob(kind) };
                        (cases[index].1)(&mut state);
                        simulate_versioned_mob_travel(release.to_owned(), &world, state).map(|_| ())
                    });
                }
            }
        }
    }
    probe.finish("mob travel");
}

fn cart(variant: &str) -> NativeMinecartState {
    let furnace = variant == "furnace_minecart";
    let holds = variant == "chest_minecart" || variant == "hopper_minecart";
    NativeMinecartState {
        variant: variant.to_owned(),
        position: vec3(0.5, 1.0625, 0.5),
        velocity: vec3(0.3, 0.0, 0.0),
        y_rot: 0.0,
        x_rot: None,
        flipped: false,
        on_rails: Some(true),
        on_ground: false,
        first_tick: None,
        in_water: false,
        collided_horizontally: None,
        collided_vertically: None,
        stuck_speed_multiplier: None,
        no_gravity: None,
        ultra_warm: None,
        fall_distance: None,
        bounding_box: None,
        rider: Some(NativeMinecartRider {
            player: true,
            living: Some(true),
            forward: Some(1.0),
            yaw: Some(10.0),
            velocity: Some(vec3(0.1, 0.0, 0.1)),
            move_intent: Some(vec3(0.1, 0.0, 0.1)),
        }),
        fuel: furnace.then_some(100),
        push: furnace.then(|| vec3(1.0, 0.0, 0.0)),
        comparator_signal: holds.then_some(3),
        has_loot_table: None,
        custom_name: None,
        tick_count: None,
        hurt_time: None,
        hurt_dir: None,
        damage: None,
        invulnerable: None,
        fuse: None,
        creative_ignition: None,
        explosion_power_base: None,
        explosion_speed_factor: None,
        random_seed: (variant == "tnt_minecart").then_some(42.0),
        enabled: None,
        inventory: (variant == "hopper_minecart").then(Vec::new),
        last_activated: None,
        spawn_delay: None,
        min_spawn_delay: None,
        max_spawn_delay: None,
        required_player_range: None,
        portal: None,
        portal_entry: None,
        inside_portal: None,
        portal_time: None,
        portal_cooldown: None,
        removed: None,
        team: None,
    }
}

fn other() -> NativeMinecartOther {
    NativeMinecartOther {
        position: vec3(1.0, 1.0, 0.5),
        bounding_box: NativeAabb { min: vec3(0.7, 1.0, 0.2), max: vec3(1.3, 2.8, 0.8) },
        velocity: Some(vec3(0.1, 0.0, 0.0)),
        pushable: Some(true),
        solid: Some(true),
        player: Some(false),
        iron_golem: Some(false),
        living: Some(true),
        passenger: Some(false),
        has_passengers: Some(false),
        rides: None,
        rides_other: None,
        boat: None,
        tick_position: None,
        tick_bounding_box: None,
        ticks_after: None,
        gets_off: None,
        eye_height: None,
        armor_stand: None,
        bat: None,
        team: None,
        boarding_cooldown: None,
        removed: None,
    }
}

fn rule_sets(release: &str) -> Vec<NativeMinecartRules> {
    let parsed = crate::version::MinecraftVersion::parse(release).expect("a release");
    let mut rules = vec![NativeMinecartRules { improvements: false, max_minecart_speed: None, min_y: None, tnt_explodes: None, nether_exit: None, spawner_blocks_enabled: None, level_random: None }];
    if parsed.at_least(1, 21, 2) {
        rules.push(NativeMinecartRules { improvements: true, max_minecart_speed: Some(8), min_y: None, tnt_explodes: None, nether_exit: None, spawner_blocks_enabled: None, level_random: None });
        rules.push(NativeMinecartRules { improvements: true, max_minecart_speed: Some(1000), min_y: None, tnt_explodes: None, nether_exit: None, spawner_blocks_enabled: None, level_random: None });
    }
    rules
}

#[test]
fn a_cart_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let world = scene();
    let cases = Arc::new(changes::<NativeMinecartState>(
        &[
            ("position", |state| &mut state.position),
            ("velocity", |state| &mut state.velocity),
            ("stuck", |state| state.stuck_speed_multiplier.get_or_insert(vec3(0.0, 0.0, 0.0))),
            ("push", |state| state.push.get_or_insert(vec3(0.0, 0.0, 0.0))),
            ("rider velocity", |state| {
                let rider = state.rider.as_mut().expect("a rider");
                rider.velocity.get_or_insert(vec3(0.0, 0.0, 0.0))
            }),
            ("rider intent", |state| {
                let rider = state.rider.as_mut().expect("a rider");
                rider.move_intent.get_or_insert(vec3(0.0, 0.0, 0.0))
            }),
        ],
        &[
            ("y rot", |state, value| state.y_rot = value),
            ("x rot", |state, value| state.x_rot = Some(value)),
            ("fall distance", |state, value| state.fall_distance = Some(value)),
            ("rider forward", |state, value| state.rider.as_mut().expect("a rider").forward = Some(value)),
            ("rider yaw", |state, value| state.rider.as_mut().expect("a rider").yaw = Some(value)),
        ],
        &[
            ("fuel", |state, value| state.fuel = Some(value)),
            ("comparator", |state, value| state.comparator_signal = Some(value)),
        ],
        Some(|state, bounds| state.bounding_box = Some(bounds)),
    ));
    let others = Arc::new(changes::<NativeMinecartOther>(
        &[
            ("other position", |state| &mut state.position),
            ("other velocity", |state| state.velocity.get_or_insert(vec3(0.0, 0.0, 0.0))),
        ],
        &[],
        &[("rides", |state, value| state.rides = Some(value as u32))],
        Some(|state, bounds| state.bounding_box = bounds),
    ));
    let mut probe = Probe::default();
    for release in RELEASES {
        for rules in rule_sets(release) {
            let label = format!("{release} improved {} speed {:?}", rules.improvements, rules.max_minecart_speed);
            for variant in ["minecart", "chest_minecart", "furnace_minecart", "tnt_minecart", "command_block_minecart"] {
                for index in 0..cases.len() {
                    let (world, cases) = (world.clone(), cases.clone());
                    probe.case(format!("{label} {variant} {}", cases[index].0), move || {
                        let mut state = cart(variant);
                        (cases[index].1)(&mut state);
                        simulate_versioned_minecart_tick(release.to_owned(), &world, state, rules).map(|_| ())
                    });
                }
            }
            for index in 0..others.len() {
                let (world, cases) = (world.clone(), others.clone());
                probe.case(format!("{label} scene {}", cases[index].0), move || {
                    let mut near = other();
                    (cases[index].1)(&mut near);
                    simulate_versioned_minecart_scene(release.to_owned(), &world, vec![cart("minecart"), cart("chest_minecart")], vec![near], rules, None, None, None).map(|_| ())
                });
            }
        }
        for index in 0..others.len() {
            let cases = others.clone();
            probe.case(format!("{release} push {}", cases[index].0), move || {
                let mut near = other();
                (cases[index].1)(&mut near);
                simulate_versioned_minecart_push(release.to_owned(), cart("minecart"), near).map(|_| ())
            });
        }
        for index in 0..cases.len() {
            let cases = cases.clone();
            probe.case(format!("{release} pushed {}", cases[index].0), move || {
                let mut state = cart("minecart");
                (cases[index].1)(&mut state);
                simulate_versioned_minecart_push(release.to_owned(), state, other()).map(|_| ())
            });
        }
    }
    probe.finish("minecart");
}

/// The refusal a call ends in, which names what is wrong.
fn refusal<T>(result: napi::Result<T>) -> String {
    result.err().map_or_else(|| "it ran".to_owned(), |error| error.reason)
}

#[test]
fn what_an_entity_cannot_walk_is_refused_with_what_is_wrong() {
    let world = scene();
    let level = RefCell::new(scene_world());
    let huge = vec3(1.0e9, 0.0, 0.0);
    let far = vec3(1.0e300, 1.0, 0.0);
    let wide = NativeAabb { min: vec3(-1.0e5, 1.0, -1.0e5), max: vec3(1.0e5, 2.0, 1.0e5) };
    for release in ["1.8.8", "1.16.5", "26.3"] {
        let version = || release.to_owned();
        let boat = |change: fn(&mut NativeBoatState)| {
            let mut state = native_boat_state(BoatSimulationState { position: Vec3::new(0.5, 1.0, 0.5), ..BoatSimulationState::default() });
            change(&mut state);
            refusal(simulate_versioned_boat_ticks(version(), &mut level.borrow_mut(), vec![state], 1))
        };
        assert!(boat(|state| state.velocity = vec3(0.0, 1.0e9, 0.0)).contains("the tick's move"), "{release}");
        assert!(boat(|state| state.position = vec3(1.0e300, 1.0, 0.0)).contains("position.x"), "{release}");
        let mob = |change: &dyn Fn(&mut NativeHorseState)| {
            let parsed = crate::version::MinecraftVersion::parse(release).expect("a release");
            let mut state = native_ridden_mob_state(RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 1.0, 0.5)), parsed);
            change(&mut state);
            refusal(simulate_versioned_horse_ticks(version(), &mut level.borrow_mut(), vec![state], 1))
        };
        let reason = mob(&|state| state.velocity = huge);
        // (1.8 moves a mob only off the client: 1.8.8 LivingEntity.java:1134, and tick_ridden_mob)
        assert!(release == "1.8.8" || reason.contains("the tick's move"), "{release} {reason}");
        assert!(mob(&|state| state.position = far).contains("position.x"), "{release}");
        assert!(mob(&|state| state.lerp_position = Some(far)).contains("position.x"), "{release}");
        assert!(mob(&|state| state.height = Some(1.0e9)).contains("block cells"), "{release}");
        let moved = |change: &dyn Fn(&mut NativeMovingEntityState)| {
            let mut state = moving("tnt", true);
            change(&mut state);
            refusal(simulate_versioned_moving_entity_tick(version(), &mut level.borrow_mut(), state, None))
        };
        assert!(moved(&|state| state.velocity = huge).contains("the tick's move"), "{release}");
        assert!(moved(&|state| state.position = far).contains("position.x"), "{release}");
        assert!(moved(&|state| state.bounding_box = Some(wide.clone())).contains("boundingBox"), "{release}");
        assert!(moved(&|state| {
            state.last_movements = Some(vec![NativeMovingEntityMovement { from: vec3(0.0, 0.0, 0.0), to: vec3(1.0e9, 1.0e9, 0.0), axis_order: None }]);
        })
        .contains("last movement"), "{release}");
        let travel = |change: &dyn Fn(&mut NativeMobTravelState)| {
            let mut state = mob_in_travel();
            change(&mut state);
            refusal(simulate_versioned_mob_travel(version(), &world, state))
        };
        assert!(travel(&|state| state.velocity = huge).contains("the tick's move"), "{release}");
        assert!(travel(&|state| state.max_step = Some(1.0e9)).contains("the tick's move"), "{release}");
        assert!(travel(&|state| state.half_width = -1.0).contains("halfWidth"), "{release}");
        let flown = |velocity: NativeVec3| refusal(simulate_versioned_arrow_ticks(version(), &world, vec![NativeArrowState { velocity, ..arrow() }], 1));
        assert!(flown(huge).contains("flight"), "{release}");
        let thrown = |air_drag: f64| {
            let state = NativeProjectileState { air_drag: Some(air_drag), ..projectile("snowball") };
            refusal(simulate_versioned_projectile_ticks(version(), &world, vec![state], 1))
        };
        assert!(thrown(1.0e300).contains("flight"), "{release}");
        let tick = |change: &dyn Fn(&mut NativeMinecartState)| {
            let mut state = cart("minecart");
            change(&mut state);
            let rules = NativeMinecartRules { improvements: false, max_minecart_speed: None, min_y: None, tnt_explodes: None, nether_exit: None, spawner_blocks_enabled: None, level_random: None };
            refusal(simulate_versioned_minecart_tick(version(), &world, state, rules))
        };
        assert!(tick(&|state| state.position = far).contains("position.x"), "{release}");
        // (a cart's own motion is clamped to its speed, as vanilla's is: this is the multiplier's move)
        assert!(tick(&|state| {
            state.position = vec3(0.5, 40.0, 0.5);
            state.stuck_speed_multiplier = Some(vec3(1.0e12, 1.0e12, 1.0e12));
        })
        .contains("the tick's move"), "{release}");
    }
}

fn mob_in_travel() -> NativeMobTravelState {
    mob("squid")
}

/// The motion and places a game can reach: nothing here is near a refusal, on any path.
#[test]
fn large_values_a_game_can_reach_still_run_for_every_family() {
    let world = scene();
    let mut level = scene_world();
    for release in RELEASES {
        let version = || release.to_owned();
        let parsed = crate::version::MinecraftVersion::parse(release).expect("a release");
        for (position, velocity) in [
            // the world's edge, and past it as a client can be
            (vec3(29_999_999.5, 3.5, -29_999_999.5), vec3(0.1, 0.0, 0.1)),
            (vec3(1.0e9, 3.5, 1.0e9), vec3(0.1, 0.0, 0.1)),
            // what a velocity packet can carry until 1.21.9 and much more: a TNT cannon's motion, tens of blocks a tick on each axis
            (vec3(0.5, 3.5, 0.5), vec3(3.9, -3.9, 3.9)),
            (vec3(0.5, 3.5, 0.5), vec3(30.0, 30.0, -30.0)),
            (vec3(0.5, 3.5, 0.5), vec3(1000.0, 0.0, 0.0)),
        ] {
            let boat = native_boat_state(BoatSimulationState { position: position.into(), velocity: velocity.into(), ..BoatSimulationState::default() });
            simulate_versioned_boat_ticks(version(), &mut level, vec![boat], 2).expect(release);
            let mut mob = RiddenMobState::new(RiddenMobKind::Horse, position.into());
            mob.velocity = velocity.into();
            simulate_versioned_horse_ticks(version(), &mut level, vec![native_ridden_mob_state(mob, parsed)], 2).expect(release);
            for kind in ["tnt", "falling_block", "item", "experience_orb"] {
                let state = NativeMovingEntityState { position, velocity, ..moving(kind, true) };
                simulate_versioned_moving_entity_tick(version(), &mut level, state, Some(random_and_a_player())).expect(release);
            }
            simulate_versioned_mob_travel(version(), &world, NativeMobTravelState { position, velocity, ..mob_in_travel() }).expect(release);
            simulate_versioned_arrow_ticks(version(), &world, vec![NativeArrowState { position, velocity, ..arrow() }], 2).expect(release);
            let thrown = NativeProjectileState { position, velocity, ..projectile("snowball") };
            simulate_versioned_projectile_ticks(version(), &world, vec![thrown], 2).expect(release);
            for rules in rule_sets(release) {
                let state = NativeMinecartState { position, velocity, ..cart("minecart") };
                simulate_versioned_minecart_tick(version(), &world, state, rules).expect(release);
            }
        }
        // the counters a server keeps run on past an int as Java's do
        let state = NativeMovingEntityState { tick_count: Some(i32::MAX), ..moving("item", true) };
        let ran = simulate_versioned_moving_entity_tick(version(), &mut level, state, None).expect(release);
        assert_eq!(ran.state.tick_count, Some(i32::MIN), "{release}");
    }
}

fn step(value: f64) -> NativeMinecartStep {
    NativeMinecartStep { position: vec3(value, value, value), movement: vec3(value, value, value), y_rot: value, x_rot: value, weight: value }
}

#[test]
fn a_clients_cart_of_extreme_values_runs_or_fails_and_never_panics_or_hangs() {
    let lerps = Arc::new(changes::<NativeMinecartLerp>(
        &[("position", |state| &mut state.position), ("target", |state| &mut state.target)],
        &[
            ("y rot", |state, value| state.y_rot = value),
            ("x rot", |state, value| state.x_rot = value),
            ("target y rot", |state, value| state.target_y_rot = value),
            ("target x rot", |state, value| state.target_x_rot = value),
        ],
        &[("steps", |state, value| state.steps = value)],
        None,
    ));
    let steps = Arc::new(changes::<NativeMinecartClientSteps>(
        &[("position", |state| &mut state.position), ("velocity", |state| &mut state.velocity)],
        &[
            ("y rot", |state, value| state.y_rot = value),
            ("x rot", |state, value| state.x_rot = value),
            ("total weight", |state, value| state.total_weight = value),
            ("pending", |state, value| state.pending = vec![step(value), step(1.0)]),
            ("current", |state, value| state.current = vec![step(1.0), step(value)]),
            ("old", |state, value| state.old = step(value)),
        ],
        &[("delay", |state, value| state.delay = value)],
        None,
    ));
    let mut probe = Probe::default();
    for release in RELEASES {
        for index in 0..lerps.len() {
            let cases = lerps.clone();
            probe.case(format!("{release} lerp {}", cases[index].0), move || {
                let mut state = NativeMinecartLerp {
                    position: vec3(0.0, 1.0, 0.0),
                    y_rot: 0.0,
                    x_rot: 0.0,
                    steps: 3,
                    target: vec3(1.0, 1.0, 0.0),
                    target_y_rot: 10.0,
                    target_x_rot: 10.0,
                };
                (cases[index].1)(&mut state);
                simulate_minecart_client_lerp(release.to_owned(), state).map(|_| ())
            });
        }
    }
    for index in 0..steps.len() {
        for delay in [0, 2] {
            let cases = steps.clone();
            probe.case(format!("steps delay {delay} {}", cases[index].0), move || {
                let mut state = NativeMinecartClientSteps {
                    position: vec3(0.0, 1.0, 0.0),
                    velocity: vec3(0.1, 0.0, 0.0),
                    y_rot: 0.0,
                    x_rot: 0.0,
                    delay,
                    pending: vec![step(1.0)],
                    current: vec![step(1.0)],
                    total_weight: 1.0,
                    old: step(0.0),
                };
                (cases[index].1)(&mut state);
                simulate_minecart_client_steps(state).map(|_| ())
            });
        }
    }
    probe.finish("client cart");
}
