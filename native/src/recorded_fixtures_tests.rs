//! Ticks of vanilla recordings, run through the engine's N-API functions: native/fixtures/recorded/*.json, which
//! the recorder's scripts/recorded-fixtures.mts makes from the pinned recordings (picks.json says which ticks, and why each). A fixture holds
//! the call's arguments as the native replay seeded them from vanilla (the recorder's replay/engine.ts), the world around the tick
//! (a NativeWorld checkpoint) and vanilla's own values of every field the replay compared: at the tick's end, and at each
//! phase of the tick for the player. The engine must give each of them back exactly (floats by their bits, -0 apart from 0),
//! as the replay's compare does (the recorder's replay/compare.ts compareNative: a vanilla null is an absent value).

use super::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("recorded")
}

/// The fixture's JSON as written: "-0" stands for the -0 JSON has no way to say.
fn revive(value: Value) -> Value {
    match value {
        Value::String(text) if text == "-0" => Value::from(-0.0_f64),
        Value::Array(items) => Value::Array(items.into_iter().map(revive).collect()),
        Value::Object(fields) => Value::Object(fields.into_iter().map(|(key, value)| (key, revive(value))).collect()),
        other => other,
    }
}

fn base64(text: &str) -> Vec<u8> {
    let digit = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("not base64: {c}"),
        }
    };
    let bytes: Vec<u8> = text.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut word = 0_u32;
        for (i, &c) in chunk.iter().enumerate() {
            word |= digit(c) << (18 - 6 * i);
        }
        out.extend_from_slice(&word.to_be_bytes()[1..chunk.len()]);
    }
    out
}

fn world_of(checkpoint: &str) -> NativeWorld {
    let mut world = NativeWorld::new();
    world.inner = WorldSnapshot::decode(&base64(checkpoint)).expect("a checkpoint of this schema");
    world
}

fn arg<T: DeserializeOwned>(args: &[Value], index: usize) -> T {
    serde_json::from_value(args[index].clone()).unwrap_or_else(|error| panic!("argument {index}: {error}"))
}

fn json<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).expect("a state as JSON")
}

fn refused<T>(error: Error) -> T {
    panic!("the engine refused the tick: {error}")
}

/// What the call gives back: its end state (each cart's, for a minecart scene), and the states of the phases it passed (by name, in order).
fn run(fixture: &Value) -> (Value, Vec<(String, Value)>) {
    let version = fixture["version"].as_str().expect("a version").to_owned();
    let args = fixture["args"].as_array().expect("arguments");
    let mut world = world_of(fixture["world"].as_str().expect("a world"));
    match fixture["fn"].as_str().expect("a function") {
        "simulateVersionedTick" => {
            let state = simulate_versioned_tick(version, &mut world, arg(args, 1), arg(args, 2)).unwrap_or_else(refused);
            (json(state), Vec::new())
        }
        "simulateVersionedTickTraced" => {
            let traced = simulate_versioned_tick_traced(version, &mut world, arg(args, 1), arg(args, 2)).unwrap_or_else(refused);
            let phases = traced.phases.into_iter().map(|phase| (phase.name, json(phase.state))).collect();
            (json(traced.state), phases)
        }
        "simulateVersionedBoatTick" => (json(simulate_versioned_boat_tick(version, &mut world, arg(args, 1)).unwrap_or_else(refused)), Vec::new()),
        "simulateVersionedBoatTickTraced" => {
            (json(simulate_versioned_boat_tick_traced(version, &mut world, arg(args, 1)).unwrap_or_else(refused).state), Vec::new())
        }
        "simulateVersionedHorseTick" => (json(simulate_versioned_horse_tick(version, &mut world, arg(args, 1)).unwrap_or_else(refused)), Vec::new()),
        "simulateVersionedHorseTickTraced" => {
            (json(simulate_versioned_horse_tick_traced(version, &mut world, arg(args, 1)).unwrap_or_else(refused).state), Vec::new())
        }
        "simulateVersionedMinecartScene" => {
            let scene = crate::napi_minecart::simulate_versioned_minecart_scene(
                version, &world, arg(args, 1), arg(args, 2), arg(args, 3), arg(args, 4), arg(args, 5), arg(args, 6),
            )
            .unwrap_or_else(refused);
            // (each cart's state after the scene's tick, in the order the carts were given)
            (Value::Array(scene.carts.into_iter().map(|tick| json(tick.state)).collect()), Vec::new())
        }
        "simulateVersionedMovingEntityTick" => {
            let tick = crate::napi_moving_entity::simulate_versioned_moving_entity_tick(version, &mut world, arg(args, 1), arg(args, 2))
                .unwrap_or_else(refused);
            (json(tick.state), Vec::new())
        }
        other => panic!("no fixture runner for {other}"),
    }
}

/// compareNative's `same`, exact: numbers by their bits, a null as an absent value, objects field by field.
fn same(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Null, Value::Null) => true,
        (Value::Number(e), Value::Number(a)) => {
            e.as_f64().map(f64::to_bits) == a.as_f64().map(f64::to_bits)
        }
        (Value::Array(e), Value::Array(a)) => e.len() == a.len() && e.iter().zip(a).all(|(e, a)| same(e, a)),
        (Value::Object(e), Value::Object(a)) => e
            .keys()
            .chain(a.keys())
            .all(|key| same(e.get(key).unwrap_or(&Value::Null), a.get(key).unwrap_or(&Value::Null))),
        (e, a) => e == a,
    }
}

fn compare(label: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    for (key, want) in expected.as_object().expect("expected fields") {
        let got = actual.get(key).unwrap_or(&Value::Null);
        // (a cart gone from the server's view: the engine says why, which the view does not)
        let gone = key == "removed" && *want == Value::Bool(true) && !matches!(got, Value::Null | Value::Bool(false));
        if !gone && !same(want, got) {
            out.push(format!("{label}.{key}: vanilla {want}, engine {got}"));
        }
    }
}

/// The divergences of one recorded tick from vanilla, each "<recording> t=<tick> <where>.<field>: ...".
fn divergences(fixture: &Value) -> Vec<String> {
    let (state, phases) = run(fixture);
    let mut out = Vec::new();
    match (&fixture["expected"]["carts"], &state) {
        (Value::Array(carts), Value::Array(states)) => {
            if carts.len() != states.len() {
                out.push(format!("carts: vanilla {}, engine {}", carts.len(), states.len()));
            }
            for (i, (want, got)) in carts.iter().zip(states).enumerate() {
                compare(&format!("cart#{i}"), want, got, &mut out);
            }
        }
        _ => compare("end", &fixture["expected"], &state, &mut out),
    }
    for phase in fixture["phases"].as_array().map_or(&[][..], Vec::as_slice) {
        let at = phase["at"].as_str().expect("a phase");
        // ("<name>#<i>": the i-th time the tick passed the phase)
        let (name, nth) = at.split_once('#').map_or((at, 0), |(name, i)| (name, i.parse().expect("an occurrence")));
        match phases.iter().filter(|(passed, _)| passed == name).nth(nth) {
            Some((_, actual)) => compare(at, &phase["expected"], actual, &mut out),
            None => out.push(format!("{at}: the engine did not pass it")),
        }
    }
    let source = format!("{} t={}", fixture["recording"].as_str().unwrap_or("?"), fixture["tick"]);
    out.into_iter().map(|line| format!("{source} {line}")).collect()
}

fn fixture_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixture_dir())
        .expect("native/fixtures/recorded")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json") && path.file_name().is_some_and(|name| name != "picks.json"))
        .collect();
    files.sort();
    files
}

/// Every recorded tick of every fixture gives back what vanilla recorded.
#[test]
fn recorded_ticks_match_vanilla() {
    let files = fixture_files();
    assert!(!files.is_empty(), "no recorded fixtures under {}", fixture_dir().display());
    let mut ticks = 0;
    let mut failures = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a fixture");
        let fixture: Value = revive(serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", file.display())));
        for tick in fixture["ticks"].as_array().expect("ticks") {
            ticks += 1;
            failures.extend(divergences(tick));
        }
    }
    assert!(failures.is_empty(), "{} fields of the {ticks} recorded ticks differ from vanilla:\n{}", failures.len(), failures.join("\n"));
}

/// Every pick of picks.json has its fixture, and every fixture its pick.
#[test]
fn every_pick_has_its_fixture() {
    let picks: Value = serde_json::from_str(&std::fs::read_to_string(fixture_dir().join("picks.json")).expect("picks.json")).expect("picks as JSON");
    let mut named: Vec<String> = picks.as_array().expect("picks").iter().map(|pick| format!("{}.json", pick["name"].as_str().expect("a name"))).collect();
    named.sort();
    let mut files: Vec<String> = fixture_files().iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    files.sort();
    assert_eq!(named, files);
}

/// The world of every recorded tick is a checkpoint the engine writes back byte for byte, and its
/// digest is that of the checkpoint (NativeWorld.checkpoint, NativeWorld.contentDigest).
#[test]
fn every_recorded_world_writes_back_as_its_checkpoint() {
    let mut worlds = 0;
    for file in fixture_files() {
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&file).expect("a fixture")).expect("a fixture as JSON");
        for tick in fixture["ticks"].as_array().expect("ticks") {
            let bytes = base64(tick["world"].as_str().expect("a world"));
            let world = world_of(tick["world"].as_str().expect("a world"));
            assert!(world.inner.encode() == bytes, "{}: the world writes back other bytes", file.display());
            assert_eq!(world.content_digest(), crate::world::checkpoint_digest(&bytes), "{}", file.display());
            worlds += 1;
        }
    }
    assert!(worlds > 0);
}
