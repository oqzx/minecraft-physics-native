//! The engine itself, not copies of its math: each `walk` of native/fixtures/java-oracle-living-*.txt
//! through versioned::dispatch, on the host and on wasm32-wasip1. The crate is built without its
//! `bindings` feature, so no emnapi is needed. Every state after every tick must be Java's, by the
//! bits, and the digest must be the same on both targets.
//!
//!   cargo run --release --bin oracle-engine --manifest-path scripts/java-oracle/wasm/Cargo.toml
//!   cargo build --release --bin oracle-engine --manifest-path scripts/java-oracle/wasm/Cargo.toml --target wasm32-wasip1
//!   node scripts/java-oracle/wasm/run.mjs TARGET_DIR/wasm32-wasip1/release/oracle-engine.wasm

use minecraft_physics_native::simulation::{PhysicsConfig, SimulationState};
use minecraft_physics_native::version::MinecraftVersion;
use minecraft_physics_native::versioned;
use minecraft_physics_native::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};
use minecraft_physics_native::{Aabb, Vec3};

const FIXTURES: [(MinecraftVersion, &str); 8] = [
    (MinecraftVersion::new(1, 18, 2), include_str!("../../../../native/fixtures/java-oracle-living-1.18.2.txt")),
    (MinecraftVersion::new(1, 20, 4), include_str!("../../../../native/fixtures/java-oracle-living-1.20.4.txt")),
    (MinecraftVersion::new(1, 20, 5), include_str!("../../../../native/fixtures/java-oracle-living-1.20.5.txt")),
    (MinecraftVersion::new(1, 21, 5), include_str!("../../../../native/fixtures/java-oracle-living-1.21.5.txt")),
    (MinecraftVersion::new(1, 21, 10), include_str!("../../../../native/fixtures/java-oracle-living-1.21.10.txt")),
    (MinecraftVersion::new(1, 21, 11), include_str!("../../../../native/fixtures/java-oracle-living-1.21.11.txt")),
    (MinecraftVersion::new(26, 2, 0), include_str!("../../../../native/fixtures/java-oracle-living-26.2.txt")),
    (MinecraftVersion::new(26, 3, 0), include_str!("../../../../native/fixtures/java-oracle-living-26.3.txt")),
];

fn numbers(words: &[&str]) -> Vec<f64> {
    words.iter().map(|word| word.parse().unwrap()).collect()
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn floor(origin: i32, slipperiness: f64, bounds: Aabb) -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    let shape = BlockShape { bounds, slipperiness, fluid: FluidState::Empty, effect: BlockEffect::None };
    let state = BlockState { id: 1, shape: Some(shape), ..BlockState::default() };
    for x in -32..=32 {
        for z in -32..=32 {
            world.set_block_shapes((origin + x, 63, -origin + z), state.clone(), vec![shape]);
        }
    }
    world
}

fn main() {
    let (mut walks, mut ticks, mut failures, mut refused) = (0_u64, 0_u64, 0_u64, 0_u64);
    let mut digest = 0xcbf29ce484222325_u64;
    for (release, fixture) in FIXTURES {
        let mut palette: Vec<(f64, Aabb)> = Vec::new();
        let mut world = WorldSnapshot::new();
        for line in fixture.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words[0] {
                "palette" => {
                    let a = numbers(&words[3..]);
                    let b = &a[4..10];
                    palette.push((a[0], Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5]))));
                }
                "scene" => {
                    let (slipperiness, bounds) = palette[words[3].parse::<usize>().unwrap()];
                    world = floor(words[2].parse().unwrap(), slipperiness, bounds);
                }
                "walk" => {
                    walks += 1;
                    let equals = words.iter().position(|word| *word == "=").unwrap();
                    let head = numbers(&words[2..9]);
                    let count: usize = words[9].parse().unwrap();
                    let inputs = numbers(&words[10..equals]);
                    let expected = numbers(&words[equals + 1..]);
                    let infinite = head[3..6].iter().any(|value| value.is_infinite());
                    let mut state = SimulationState {
                        position: Vec3::new(head[0], head[1], head[2]),
                        velocity: Vec3::new(head[3], head[4], head[5]),
                        half_width: f64::from(0.6_f32 / 2.0),
                        height: f64::from(1.95_f32),
                        movement_speed_value: Some(head[6]),
                        controlled_by_local_instance: true,
                        ..SimulationState::default()
                    };
                    for tick in 0..count {
                        ticks += 1;
                        let input = &inputs[tick * 5..(tick + 1) * 5];
                        let yaw = input[2] as f32;
                        state.input = Vec3::new(input[0], 0.0, input[1]);
                        state.yaw = f64::from(yaw * 0.017453292);
                        state.yaw_degrees = Some(yaw);
                        state.jump_requested = input[3] != 0.0;
                        state.sprinting = input[4] != 0.0;
                        versioned::dispatch(&release.to_string(), &mut state, &world, PhysicsConfig::default()).unwrap();
                        if infinite && !release.at_least(1, 21, 11) {
                            refused += u64::from(state.move_out_of_range);
                            failures += u64::from(!state.move_out_of_range);
                            break;
                        }
                        let actual = [
                            state.position.x, state.position.y, state.position.z,
                            state.velocity.x, state.velocity.y, state.velocity.z,
                            f64::from(u8::from(state.on_ground)), state.fall_distance,
                        ];
                        for value in actual {
                            digest = (digest ^ value.to_bits()).wrapping_mul(0x100000001b3);
                        }
                        let want = &expected[tick * 8..(tick + 1) * 8];
                        if !actual.iter().zip(want).all(|(a, b)| same(*a, *b)) {
                            failures += 1;
                            if failures <= 5 {
                                println!("{release:?} tick {tick}: java {want:?} rust {actual:?}");
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    println!("{walks} walks, {ticks} ticks, {refused} refused as documented, {failures} differ from Java, digest {digest:016x}");
    if failures > 0 {
        std::process::exit(1);
    }
}
