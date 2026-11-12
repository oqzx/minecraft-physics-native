//! Reach fixtures (B280): ungated functions no Rust test or recording ran on the releases either side of a boundary their
//! cited methods change at. Each runs the function through the path the engine takes, on every registered release, and
//! asserts what the release's source computes, so the reach probe (scripts/reach-probe.cjs) sees both sides.

use crate::dragon::{dragon_parts, tick_dragon, tick_dragon_flight, DragonFlightHistory, DragonFlightSample, DragonState};
use crate::math::Vec3;
use crate::version::MinecraftVersion;

/// The 50 registered releases (scripts/release-inventory.cjs canonicalReleases).
const RELEASES: [&str; 50] = [
    "1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2", "1.14.4", "1.15", "1.15.1", "1.15.2", "1.16", "1.16.1",
    "1.16.2", "1.16.3", "1.16.4", "1.16.5", "1.17", "1.17.1", "1.18", "1.18.1", "1.18.2", "1.19", "1.19.1", "1.19.2",
    "1.19.3", "1.19.4", "1.20", "1.20.1", "1.20.2", "1.20.3", "1.20.4", "1.20.5", "1.20.6", "1.21", "1.21.1", "1.21.2",
    "1.21.3", "1.21.4", "1.21.5", "1.21.6", "1.21.7", "1.21.8", "1.21.9", "1.21.10", "1.21.11", "26.1", "26.1.1",
    "26.1.2", "26.2", "26.3",
];

fn release(name: &str) -> MinecraftVersion {
    MinecraftVersion::parse(name).expect("a registered release")
}

/// A dragon on its way to a target, its yaw a turn past 10 degrees.
fn flying() -> DragonState {
    DragonState {
        position: Vec3::new(0.5, 100.0, 0.5),
        velocity: Vec3::new(0.1, 0.0, -0.2),
        yaw: 370.0,
        yaw_acceleration: 2.0,
        in_wall: false,
        bounding_box: None,
        target: Some(Vec3::new(40.0, 110.0, -20.0)),
        steering_target: Some(Vec3::new(40.0, 110.0, -20.0)),
        fly_speed: 0.6,
        turn_speed: 0.5,
        sitting: false,
        history: DragonFlightHistory::default(),
    }
}

/// EnderDragon#aiStep of a living dragon with its AI, on every release: the yaw is wrapped, then the tick's sample is
/// recorded (the first fills every delay), then the flight, then the parts from the history with that sample in it
/// (1.8.8 EnderDragon.java:127, 142, then the flight and the parts; 1.14.4 EnderDragon.java:164, 179, 257; 26.3
/// EnderDragon.java:186, 190, 259).
#[test]
fn a_dragon_tick_wraps_the_yaw_records_it_then_flies_and_places_the_parts_on_every_release() {
    for name in RELEASES {
        let release = release(name);
        let mut ticked = flying();
        let parts = tick_dragon(&mut ticked, release);

        let mut expected = flying();
        expected.yaw = 10.0;
        expected.history = DragonFlightHistory::new(vec![DragonFlightSample { y: 100.0, yaw: 10.0 }; 64]);
        tick_dragon_flight(&mut expected, release);
        assert_eq!(ticked, expected, "{name}");
        assert_eq!(ticked.history.get(0), DragonFlightSample { y: 100.0, yaw: 10.0 }, "{name}: the sample before the flight");
        assert_eq!(ticked.history.samples().len(), 64, "{name}");
        assert_eq!(parts, dragon_parts(&expected, release), "{name}");
        assert!(ticked.position != flying().position, "{name}: a dragon with a target flies");
    }
}
