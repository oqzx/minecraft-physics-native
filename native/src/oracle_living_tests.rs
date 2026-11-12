//! A LivingEntity's float paths and trajectories against the real one:
//! scripts/java-oracle/living/OracleLiving.java runs a jar's own getFrictionInfluencedSpeed,
//! jumpFromGround, EntityDimensions and aiStep, and `native/fixtures/java-oracle-living-<release>.txt`
//! holds what they returned. Everything is compared by bits, intermediates first: a difference in a
//! `walk` names its tick, its field and the Java and Rust values, and a `friction`, `jump` or `dims`
//! line names the operation by itself. The entity is a zombie-sized LivingEntity without AI on a
//! dry floor, so the fixtures say nothing of a Player's own travel (the recordings do) or of fluids.

use crate::math::Vec3;
use crate::simulation::{
    friction_influenced_speed, jump_from_ground, jump_power, PhysicsConfig, SimulationState,
};
use crate::version::MinecraftVersion;
use crate::versioned;
use crate::world::{BlockShape, BlockState, FluidState, WorldSnapshot};
use crate::Aabb;

/// `release knockback strength x z resistance ground vx vy vz = vx vy vz`, the jar's LivingEntity.knockback.
#[test]
fn knockback_matches_the_jars_by_bits() {
    let text = include_str!("../fixtures/java-oracle-knockback.txt");
    let mut asked = 0;
    for line in text.lines() {
        let (head, answer) = line.split_once(" = ").expect("an answer");
        let words: Vec<&str> = head.split(' ').collect();
        let release = MinecraftVersion::parse(words[0]).expect("a release");
        let n: Vec<f64> = words[2..].iter().map(|w| w.parse().expect("a number")).collect();
        let want: Vec<f64> = answer.split(' ').map(|w| w.parse().expect("a number")).collect();
        let got = crate::simulation::living_knockback(
            Vec3::new(n[5], n[6], n[7]), n[4] == 1.0, n[0], n[1], n[2], n[3], 1.0, release,
        );
        // (a direction vanilla draws at random is no answer: the probe's Random is not ours)
        let Some(got) = got else {
            assert!(release.at_least(1, 21, 0), "{line}");
            continue;
        };
        assert_eq!(
            [got.x.to_bits(), got.y.to_bits(), got.z.to_bits()],
            [want[0].to_bits(), want[1].to_bits(), want[2].to_bits()],
            "{line}: got {got:?}"
        );
        asked += 1;
    }
    assert!(asked > 4000, "{asked}");
}

/// `release input x z item sneak using passenger slowly = x z`: Vec2.normalized and scale, and the private
/// modifyInputSpeedForSquareMovement, of a jar from 1.21.5, in LocalPlayer.modifyInput's order. Analog
/// magnitudes under and over one, opposite and signed zeros, which no key gives.
#[test]
fn analog_input_matches_the_jars_by_bits() {
    let text = include_str!("../fixtures/java-oracle-input.txt");
    let mut asked = 0;
    for line in text.lines() {
        let (head, answer) = line.split_once(" = ").expect("an answer");
        let words: Vec<&str> = head.split(' ').collect();
        let release = MinecraftVersion::parse(words[0]).expect("a release");
        let n: Vec<f32> = words[2..].iter().map(|w| w.parse().expect("a number")).collect();
        let want: Vec<f32> = answer.split(' ').map(|w| w.parse().expect("a number")).collect();
        let mut state = SimulationState::default();
        state.is_player = true;
        state.input = Vec3::new(f64::from(n[0]), 0.0, f64::from(n[1]));
        state.using_item = n[4] == 1.0;
        state.passenger = n[5] == 1.0;
        state.moving_slowly = Some(n[6] == 1.0);
        state.item_use_speed_multiplier = Some(n[2]);
        state.sneaking_speed = Some(f64::from(n[3]));
        let got = crate::simulation::modified_input(crate::simulation::input_move_vector(&state, release), &state);
        assert_eq!([got[0].to_bits(), got[1].to_bits()], [want[0].to_bits(), want[1].to_bits()], "{line}: got {got:?}");
        asked += 1;
    }
    assert!(asked > 500, "{asked}");
}

/// `release sin|cos|jsin|jcos x-bits = y-bits`: scripts/java-oracle/OracleTrigLarge.java, at very large arguments, the ends of
/// the casts, signed zero and what is not finite. NaN compares as NaN: the JVM's bits for one are its own.
#[test]
fn trig_at_very_large_and_non_finite_arguments_matches_the_jars() {
    use crate::simulation::{mth_cos, mth_sin};
    let text = include_str!("../fixtures/java-oracle-trig-large.txt");
    let mut differ = Vec::new();
    for line in text.lines() {
        let (head, answer) = line.split_once(" = ").expect("an answer");
        let words: Vec<&str> = head.split(' ').collect();
        let release = MinecraftVersion::parse(words[0]).expect("a release");
        let x = f64::from_bits(words[2].parse::<i64>().expect("bits") as u64);
        let want = f64::from_bits(answer.parse::<i64>().expect("bits") as u64);
        let got = match words[1] {
            "sin" => mth_sin(x, release),
            "cos" => mth_cos(x, release),
            "jsin" => crate::trig::sin(x),
            _ => crate::trig::cos(x),
        };
        if !(got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan())) {
            differ.push(format!("{} {} {x:e}: java {want:e}, rust {got:e}", words[0], words[1]));
        }
    }
    assert!(differ.is_empty(), "{}", differ.join("
"));
}

const FIXTURES: [(MinecraftVersion, &str); 8] = [
    (
        MinecraftVersion::new(1, 18, 2),
        include_str!("../fixtures/java-oracle-living-1.18.2.txt"),
    ),
    (
        MinecraftVersion::new(1, 20, 4),
        include_str!("../fixtures/java-oracle-living-1.20.4.txt"),
    ),
    (
        MinecraftVersion::new(1, 20, 5),
        include_str!("../fixtures/java-oracle-living-1.20.5.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 5),
        include_str!("../fixtures/java-oracle-living-1.21.5.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 10),
        include_str!("../fixtures/java-oracle-living-1.21.10.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 11),
        include_str!("../fixtures/java-oracle-living-1.21.11.txt"),
    ),
    (
        MinecraftVersion::new(26, 2, 0),
        include_str!("../fixtures/java-oracle-living-26.2.txt"),
    ),
    (
        MinecraftVersion::new(26, 3, 0),
        include_str!("../fixtures/java-oracle-living-26.3.txt"),
    ),
];

fn numbers(words: &[&str]) -> Vec<f64> {
    words.iter().map(|word| word.parse().unwrap()).collect()
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

struct Palette {
    slipperiness: f64,
    bounds: Aabb,
}

fn floor(origin: i32, palette: &Palette) -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    let shape = BlockShape {
        bounds: palette.bounds,
        slipperiness: palette.slipperiness,
        fluid: FluidState::Empty,
        effect: crate::world::BlockEffect::None,
    };
    let state = BlockState {
        id: 1,
        shape: Some(shape),
        ..BlockState::default()
    };
    for x in -32..=32 {
        for z in -32..=32 {
            world.set_block_shapes((origin + x, 63, -origin + z), state.clone(), vec![shape]);
        }
    }
    world
}

const WALK_FIELDS: [&str; 8] = ["x", "y", "z", "vx", "vy", "vz", "onGround", "fall"];

/// One `walk`: the first tick and field to differ, if any.
fn walk(words: &[&str], world: &WorldSnapshot, release: MinecraftVersion) -> Option<String> {
    let equals = words.iter().position(|word| *word == "=").unwrap();
    let head = numbers(&words[2..9]);
    let speed = head[6];
    let ticks: usize = words[9].parse().unwrap();
    let inputs = numbers(&words[10..equals]);
    let expected = numbers(&words[equals + 1..]);
    assert_eq!(inputs.len(), ticks * 5);
    let infinite = head[3..6].iter().any(|value| value.is_infinite());
    let mut state = SimulationState {
        position: Vec3::new(head[0], head[1], head[2]),
        velocity: Vec3::new(head[3], head[4], head[5]),
        half_width: f64::from(0.6_f32 / 2.0),
        height: f64::from(1.95_f32),
        movement_speed_value: Some(speed),
        controlled_by_local_instance: true,
        ..SimulationState::default()
    };
    for tick in 0..ticks {
        let input = &inputs[tick * 5..(tick + 1) * 5];
        let yaw = input[2] as f32;
        state.input = Vec3::new(input[0], 0.0, input[1]);
        state.yaw = f64::from(yaw * 0.017453292);
        state.yaw_degrees = Some(yaw);
        state.jump_requested = input[3] != 0.0;
        state.sprinting = input[4] != 0.0;
        versioned::dispatch(&release.to_string(), &mut state, world, PhysicsConfig::default())
            .unwrap();
        // An infinite motion before 1.21.11, which no setter refuses: Java walks it to the border or
        // to an infinite position. The engine walks no such move and says so: the one place it
        // departs from these lines on purpose (docs/numeric-contract.md).
        if infinite && !release.at_least(1, 21, 11) {
            assert!(state.move_out_of_range, "{release:?}: an infinite move is refused");
            return None;
        }
        let actual = [
            state.position.x,
            state.position.y,
            state.position.z,
            state.velocity.x,
            state.velocity.y,
            state.velocity.z,
            f64::from(u8::from(state.on_ground)),
            state.fall_distance,
        ];
        let want = &expected[tick * 8..(tick + 1) * 8];
        for (index, name) in WALK_FIELDS.iter().enumerate() {
            if !same(actual[index], want[index]) {
                return Some(format!(
                    "tick {tick} {name}: java {} rust {}",
                    want[index], actual[index]
                ));
            }
        }
    }
    None
}

fn line_differs(words: &[&str], release: MinecraftVersion) -> Option<String> {
    let equals = words.iter().position(|word| *word == "=").unwrap();
    let a = numbers(&words[1..equals]);
    let want = numbers(&words[equals + 1..]);
    let actual: Vec<f64> = match words[0] {
        "friction" => vec![friction_influenced_speed(a[0], a[1], release)],
        "jump" => {
            let yaw = a[0] as f32;
            let mut state = SimulationState {
                yaw: f64::from(yaw * 0.017453292),
                sprinting: a[1] != 0.0,
                velocity: Vec3::new(a[2], a[3], a[4]),
                ..SimulationState::default()
            };
            let power = jump_power(&state, PhysicsConfig::default(), release, 1.0);
            jump_from_ground(&mut state, power, release);
            vec![state.velocity.x, state.velocity.y, state.velocity.z]
        }
        "jumpat" => {
            let mut state = SimulationState {
                jump_strength: Some(a[0]),
                velocity: Vec3::new(0.0, a[1], 0.0),
                ..SimulationState::default()
            };
            let power = jump_power(&state, PhysicsConfig::default(), release, 1.0);
            jump_from_ground(&mut state, power, release);
            vec![state.velocity.y]
        }
        "dims" => {
            let (width, height, scale) = (a[0] as f32, a[1] as f32, a[2] as f32);
            let state = SimulationState {
                position: Vec3::new(a[3], a[4], a[5]),
                half_width: f64::from(width * scale / 2.0),
                height: f64::from(height * scale),
                ..SimulationState::default()
            };
            let bounds = state.bounds_around_position();
            vec![
                bounds.min.x,
                bounds.min.y,
                bounds.min.z,
                bounds.max.x,
                bounds.max.y,
                bounds.max.z,
            ]
        }
        other => panic!("{other}"),
    };
    let differs = actual.len() != want.len()
        || actual.iter().zip(&want).any(|(left, right)| !same(*left, *right));
    differs.then(|| format!("java {want:?} rust {actual:?}"))
}

#[test]
fn a_living_entity_matches_the_jar() {
    let mut failures = Vec::new();
    for (release, fixture) in FIXTURES {
        let mut palette: Vec<Palette> = Vec::new();
        let mut world = WorldSnapshot::new();
        let mut counts = [0_u32; 2];
        for line in fixture.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let differs = match words[0] {
                "#" => panic!("the jar threw: {line}"),
                // (test/state-transitions.test.cjs: the sizes are the adapter's)
                "pose" => None,
                "palette" => {
                    let a = numbers(&words[3..]);
                    let b = &a[4..10];
                    palette.push(Palette {
                        slipperiness: a[0],
                        bounds: Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])),
                    });
                    None
                }
                "scene" => {
                    let origin: i32 = words[2].parse().unwrap();
                    let cell: usize = words[3].parse().unwrap();
                    world = floor(origin, &palette[cell]);
                    None
                }
                "walk" => {
                    counts[1] += 1;
                    walk(&words, &world, release)
                }
                _ => {
                    counts[0] += 1;
                    line_differs(&words, release)
                }
            };
            if let Some(difference) = differs {
                let start: String = line.chars().take(70).collect();
                failures.push(format!("{release:?} {start}...: {difference}"));
            }
        }
        assert!(counts[0] >= 200 && counts[1] >= 50, "{release:?}: {counts:?}");
    }
    assert!(
        failures.is_empty(),
        "{} differ, first:\n{}",
        failures.len(),
        failures[..failures.len().min(16)].join("\n")
    );
}
