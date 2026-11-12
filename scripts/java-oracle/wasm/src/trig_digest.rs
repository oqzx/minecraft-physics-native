//! native/src/trig.rs over whole sets, printing the lines of scripts/java-oracle/TrigDigest.java:
//! on the host and in wasm32-wasip1 alike, so that the three outputs are compared with `diff`.
//!
//!   cargo run --release --bin trig-digest --manifest-path scripts/java-oracle/wasm/Cargo.toml -- floats
//!   node scripts/java-oracle/wasm/run.mjs TARGET_DIR/wasm32-wasip1/release/trig-digest.wasm floats
//!
//! With TRIG_JVM=8 in the environment it prints `trig::java8`'s lines instead, for Java 8's digests (B315).
//! `records SITE PATH` reads a file of scripts/java-oracle/TrigCallSites.java (big-endian records of x, sin, cos and
//! StrictMath's two) and counts the values where this code and the file's Math differ.

#![allow(dead_code)]

#[path = "../../../../native/src/trig.rs"]
mod trig;

fn java8() -> bool {
    static JAVA8: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *JAVA8.get_or_init(|| std::env::var("TRIG_JVM").is_ok_and(|value| value == "8"))
}

fn jsin(x: f64) -> f64 {
    if java8() { trig::java8::sin(x) } else { trig::sin(x) }
}

fn jcos(x: f64) -> f64 {
    if java8() { trig::java8::cos(x) } else { trig::cos(x) }
}

/// Values of this code against a TrigCallSites file's Math, within pi/4 and past it.
fn records(site: &str, path: &str) {
    let bytes = std::fs::read(path).expect("a TrigCallSites file");
    let words: Vec<u64> = bytes.chunks_exact(8).map(|word| u64::from_be_bytes(word.try_into().expect("eight bytes"))).collect();
    let (mut small, mut small_differ, mut large, mut large_differ) = (0u64, 0u64, 0u64, 0u64);
    for record in words.chunks_exact(5) {
        let x = f64::from_bits(record[0]);
        let near = x.abs() <= std::f64::consts::FRAC_PI_4;
        for (value, want) in [(jsin(x), record[1]), (jcos(x), record[2])] {
            let differs = value.to_bits() != want;
            if near { small += 1; small_differ += u64::from(differs); } else { large += 1; large_differ += u64::from(differs); }
            if differs && small_differ + large_differ <= 8 {
                println!("  differs: x {:x} got {:x} want {want:x}", record[0], value.to_bits());
            }
        }
    }
    println!("records {site} within-pi/4 {small} differ {small_differ} past-pi/4 {large} differ {large_differ}");
}

const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0100_0000_01b3;

fn mix(hash: u64, word: u64) -> u64 {
    (hash ^ word).wrapping_mul(PRIME)
}

fn print(name: &str, count: u64, a: u64, b: u64) {
    println!("{name} {count} {a:x} {b:x}");
}

fn floats() {
    for top in 0u32..512 {
        if top & 255 == 255 {
            continue;
        }
        let (mut sin, mut cos, mut count) = (BASIS, BASIS, 0u64);
        for low in 0u32..1 << 23 {
            let x = f64::from(f32::from_bits((top << 23) | low));
            sin = mix(sin, jsin(x).to_bits());
            cos = mix(cos, jcos(x).to_bits());
            count += 1;
        }
        print(&format!("floats {top}"), count, sin, cos);
    }
}

fn tables() {
    let (mut modern, mut legacy) = (BASIS, BASIS);
    for i in 0..65536 {
        let i = f64::from(i);
        modern = mix(modern, u64::from((jsin(i / 10430.378350470453) as f32).to_bits()));
        legacy = mix(legacy, u64::from((jsin(i * std::f64::consts::PI * 2.0 / 65536.0) as f32).to_bits()));
    }
    print("tables", 65536, modern, legacy);
}

fn atan_tables() {
    let (mut asin, mut cos) = (BASIS, BASIS);
    for i in 0..257 {
        let angle = libm::asin(f64::from(i) / 256.0);
        asin = mix(asin, angle.to_bits());
        cos = mix(cos, jcos(angle).to_bits());
    }
    print("atan-tables", 257, asin, cos);
}

fn kick(seed: i64, count: u32) {
    const MASK: u64 = 281_474_976_710_655;
    let mut state = (seed as u64 ^ 25_214_903_917) & MASK;
    let (mut x, mut z) = (BASIS, BASIS);
    for _ in 0..count {
        state = state.wrapping_mul(25_214_903_917).wrapping_add(11) & MASK;
        let upper = (state >> (48 - 26)) as i32;
        state = state.wrapping_mul(25_214_903_917).wrapping_add(11) & MASK;
        let lower = (state >> (48 - 27)) as i32;
        let unit = ((i64::from(upper) << 27) + i64::from(lower)) as f64 * 1.1102230246251565E-16;
        let rot = unit * 6.2831854820251465;
        x = mix(x, (-jsin(rot) * 0.02).to_bits());
        z = mix(z, (-jcos(rot) * 0.02).to_bits());
    }
    print(&format!("kick {seed}"), u64::from(count), x, z);
}

fn read(path: &str) -> Vec<u64> {
    let bytes = std::fs::read(path).expect("a file of doubles");
    bytes.chunks_exact(8).map(|word| u64::from_le_bytes(word.try_into().expect("eight bytes"))).collect()
}

fn file(name: &str, path: &str) {
    let (mut sin, mut cos) = (BASIS, BASIS);
    let words = read(path);
    for &word in &words {
        let x = f64::from_bits(word);
        sin = mix(sin, jsin(x).to_bits());
        cos = mix(cos, jcos(x).to_bits());
    }
    print(&format!("file {name}"), words.len() as u64, sin, cos);
}

fn arc(name: &str, path: &str) {
    let (mut atan2, mut inverse) = (BASIS, BASIS);
    let words = read(path);
    for pair in words.windows(2) {
        let (x, y) = (f64::from_bits(pair[0]), f64::from_bits(pair[1]));
        atan2 = mix(atan2, trig::atan2(x, y).to_bits());
        let unit = x / (x.abs() + y.abs());
        inverse = mix(mix(inverse, libm::acos(unit).to_bits()), libm::asin(unit).to_bits());
    }
    print(&format!("arc {name}"), words.len() as u64 - 1, atan2, inverse);
}

fn bits(path: &str) {
    for word in read(path) {
        let x = f64::from_bits(word);
        println!("{word:x} {:x} {:x}", jsin(x).to_bits(), jcos(x).to_bits());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args[0].as_str() {
        "floats" => floats(),
        "tables" => tables(),
        "atan-tables" => atan_tables(),
        "kick" => kick(args[1].parse().expect("a seed"), args[2].parse().expect("a count")),
        "file" => file(&args[1], &args[2]),
        "arc" => arc(&args[1], &args[2]),
        "bits" => bits(&args[1]),
        "records" => records(&args[1], &args[2]),
        other => panic!("{other}"),
    }
}
