//! The fixed vectors of the Java oracle through the engine's own math.rs and trig.rs, in a program
//! that builds for the host and for wasm32-wasip1 alike (the engine's crate needs emnapi there):
//! the same lines must give the same bits on both, and Java's.
//!
//!   cargo run --release --manifest-path scripts/java-oracle/wasm/Cargo.toml
//!   cargo build --release --manifest-path scripts/java-oracle/wasm/Cargo.toml --target wasm32-wasip1
//!   node scripts/java-oracle/wasm/run.mjs TARGET_DIR/wasm32-wasip1/release/oracle-wasm.wasm

#![allow(dead_code)]

#[path = "../../../../native/src/math.rs"]
mod math;
#[path = "../../../../native/src/trig.rs"]
mod trig;
#[path = "../../../../native/src/version.rs"]
mod version;

/// The two lookups of simulation.rs that math.rs reads back.
mod simulation {
    use crate::math::{java_cos, java_cos_legacy, java_sin, java_sin_legacy};
    use crate::version::MinecraftVersion;

    pub(crate) fn mth_sin(radians: f64, release: MinecraftVersion) -> f64 {
        if release.at_least(1, 21, 11) {
            java_sin(radians)
        } else {
            java_sin_legacy(radians as f32)
        }
    }

    pub(crate) fn mth_cos(radians: f64, release: MinecraftVersion) -> f64 {
        if release.at_least(1, 21, 11) {
            java_cos(radians)
        } else {
            java_cos_legacy(radians as f32)
        }
    }
}

use math::{atan2_vanilla, friction_influenced_speed, sprint_jump_push, Vec3};
use simulation::{mth_cos, mth_sin};
use version::MinecraftVersion;

const FIXTURES: [(MinecraftVersion, &str); 9] = [
    (MinecraftVersion::new(1, 16, 1), include_str!("../../../../native/fixtures/java-oracle-1.16.1.txt")),
    (MinecraftVersion::new(1, 21, 11), include_str!("../../../../native/fixtures/java-oracle-1.21.11.txt")),
    (MinecraftVersion::new(26, 3, 0), include_str!("../../../../native/fixtures/java-oracle-26.3.txt")),
    (MinecraftVersion::new(1, 16, 1), include_str!("../../../../native/fixtures/java-oracle-geometry-1.16.1.txt")),
    (MinecraftVersion::new(1, 21, 11), include_str!("../../../../native/fixtures/java-oracle-geometry-1.21.11.txt")),
    (MinecraftVersion::new(26, 3, 0), include_str!("../../../../native/fixtures/java-oracle-geometry-26.3.txt")),
    (MinecraftVersion::new(1, 20, 4), include_str!("../../../../native/fixtures/java-oracle-living-1.20.4.txt")),
    (MinecraftVersion::new(1, 21, 11), include_str!("../../../../native/fixtures/java-oracle-living-1.21.11.txt")),
    (MinecraftVersion::new(26, 3, 0), include_str!("../../../../native/fixtures/java-oracle-living-26.3.txt")),
];

fn vec3(a: &[f64]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn of_vec(vector: Vec3) -> Vec<f64> {
    vec![vector.x, vector.y, vector.z]
}

fn run(op: &str, a: &[f64], release: MinecraftVersion) -> Option<Vec<f64>> {
    Some(match op {
        "sin" => vec![mth_sin(a[0], release)],
        "cos" => vec![mth_cos(a[0], release)],
        "jsin" => vec![trig::sin(a[0])],
        "jcos" => vec![trig::cos(a[0])],
        "atan2" => vec![atan2_vanilla(a[0], a[1])],
        "friction" => vec![friction_influenced_speed(a[0], a[1], release)],
        "dims" => {
            let half = f64::from(a[0] as f32 * a[2] as f32 / 2.0);
            let height = f64::from(a[1] as f32 * a[2] as f32);
            vec![a[3] - half, a[4], a[5] - half, a[3] + half, a[4] + height, a[5] + half]
        }
        "vadd" => of_vec(vec3(a).plus(vec3(&a[3..]))),
        "vsub" => of_vec(vec3(a) - vec3(&a[3..])),
        "vcross" => of_vec(vec3(a).cross(vec3(&a[3..]))),
        "vdot" => vec![vec3(a).dot(vec3(&a[3..]))],
        "vlength" => vec![vec3(a).length_in(release)],
        "vnormalize" => of_vec(vec3(a).normalized(release)),
        "sintable" => {
            let mut sum = 0_i64;
            for index in 0..65536 {
                let radians = (f64::from(index) + 0.5) / 10430.378350470453;
                let value = mth_sin(radians, release) as f32;
                sum = sum.wrapping_mul(31).wrapping_add(i64::from(value.to_bits() as i32));
            }
            vec![(sum as u64 >> 11) as f64]
        }
        _ => return None,
    })
}

fn main() {
    let mut lines = 0_u64;
    let mut failures = 0_u64;
    let mut digest = 0xcbf29ce484222325_u64;
    for (release, fixture) in FIXTURES {
        for line in fixture.lines() {
            let Some((left, right)) = line.split_once(" =") else {
                continue;
            };
            let mut left = left.split(' ');
            let op = left.next().unwrap();
            let inputs: Vec<f64> = left.map(|value| value.parse().unwrap()).collect();
            let Some(actual) = run(op, &inputs, release) else {
                continue;
            };
            let expected: Vec<f64> = right.split_whitespace().map(|value| value.parse().unwrap()).collect();
            lines += 1;
            for value in &actual {
                digest = (digest ^ value.to_bits()).wrapping_mul(0x100000001b3);
            }
            let same = actual.len() == expected.len()
                && actual.iter().zip(&expected).all(|(a, e)| a.to_bits() == e.to_bits() || (a.is_nan() && e.is_nan()));
            if !same {
                failures += 1;
                if failures <= 5 {
                    println!("{line}\n    rust = {actual:?}");
                }
            }
        }
    }
    println!("{lines} lines, {failures} differ from Java, digest {digest:016x}");
    if failures > 0 {
        std::process::exit(1);
    }
}
