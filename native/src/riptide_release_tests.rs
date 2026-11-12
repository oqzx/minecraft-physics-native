//! versioned::release_riptide, TridentItem.releaseUsing where the client lets go of the use key
//! (D-214), for every release: the trident only exists from 1.13 (1.13.2 TridentItem), it pushes a
//! player in water or rain along the look by the spin attack strength, 3.0F * ((1.0F + level) / 4.0F)
//! to 1.20.6 (1.20.6 TridentItem.java:106) and the enchantment's 1.5 + 0.75 per level after the
//! first from 1.21 (1.21 TridentItem.java:73), the same numbers, and a player on the ground moves
//! up by 1.1999999284744263 at once (1.20.4 TridentItem.java:94, 26.3 :93). From 26.1 a passenger is not
//! launched (26.1 TridentItem.java:64).

use crate::math::Vec3;
use crate::simulation::{PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::versioned::{release_riptide, SUPPORTED_VERSIONS};
use crate::world::{BlockShape, BlockState, WorldSnapshot};

fn floor() -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    for x in -2..=2 {
        for z in -2..=2 {
            world.set_block((x, 63, z), BlockState { id: 1, shape: Some(BlockShape::full_block()), ..BlockState::default() });
        }
    }
    world
}

fn swimmer(level: u8) -> SimulationState {
    SimulationState {
        position: Vec3::new(0.5, 66.0, 0.5),
        velocity: Vec3::new(0.0, -0.1, 0.0),
        half_width: 0.3,
        height: 1.8,
        in_water: true,
        riptide_launch: Some(level),
        ..SimulationState::default()
    }
}

fn release(version: &str, state: &mut SimulationState, world: &WorldSnapshot) {
    release_riptide(version, state, world, PhysicsConfig::default()).unwrap_or_else(|error| panic!("{version}: {error}"));
}

#[test]
fn every_release_pushes_along_the_look_from_1_13() {
    assert_eq!(SUPPORTED_VERSIONS.len(), 50);
    let world = WorldSnapshot::new();
    for &version in SUPPORTED_VERSIONS {
        let trident = MinecraftVersion::parse(version).unwrap().at_least(1, 13, 0);
        for level in 1..=3_u8 {
            // looking along +z (yaw 0, pitch 0): Mth.sin(0) is 0 and Mth.cos(0) is the table's 1.0F
            let mut state = swimmer(level);
            release(version, &mut state, &world);
            assert_eq!(state.riptide_launch, None, "{version}: the launch is taken");
            assert_eq!(state.riptide_lift, Vec3::ZERO, "{version}: no lift off the ground");
            assert_eq!(state.position, Vec3::new(0.5, 66.0, 0.5), "{version}");
            let strength = 0.75 * f64::from(1 + level);
            let expected = if trident { Vec3::new(0.0, -0.1, strength) } else { Vec3::new(0.0, -0.1, 0.0) };
            assert_eq!(state.velocity, expected, "{version} level {level}");
        }
        // in the rain alike, out of both not at all
        let mut rain = SimulationState { in_water: false, in_rain: true, ..swimmer(3) };
        release(version, &mut rain, &world);
        assert_eq!(rain.velocity.z, if trident { 3.0 } else { 0.0 }, "{version}: rain");
        let mut dry = SimulationState { in_water: false, ..swimmer(3) };
        release(version, &mut dry, &world);
        assert_eq!(dry.velocity, Vec3::new(0.0, -0.1, 0.0), "{version}: dry");
        assert_eq!(dry.riptide_launch, None, "{version}: dry");
        // a trident of no riptide pushes nothing
        let mut none = swimmer(0);
        release(version, &mut none, &world);
        assert_eq!(none.velocity, Vec3::new(0.0, -0.1, 0.0), "{version}: level 0");
    }
}

#[test]
fn every_release_lifts_a_player_on_the_ground_by_1_2_from_1_13() {
    let world = floor();
    for &version in SUPPORTED_VERSIONS {
        let release_of = MinecraftVersion::parse(version).unwrap();
        let mut state = SimulationState { position: Vec3::new(0.5, 64.0, 0.5), velocity: Vec3::ZERO, on_ground: true, ..swimmer(2) };
        release(version, &mut state, &world);
        if release_of.at_least(1, 13, 0) {
            // Player.move(SELF, new Vec3(0.0, 1.1999999284744263, 0.0)) with nothing above, all of it
            // (1.20.4 TridentItem.java:100)
            let lift = 1.199_999_928_474_426_3;
            assert_eq!(state.riptide_lift, Vec3::new(0.0, lift, 0.0), "{version}");
            assert_eq!(state.position, Vec3::new(0.5, 64.0 + lift, 0.5), "{version}");
            assert_eq!(state.velocity, Vec3::new(0.0, 0.0, 2.25), "{version}");
            assert!(!state.on_ground, "{version}: the lift leaves the ground");
        } else {
            assert_eq!(state.riptide_lift, Vec3::ZERO, "{version}");
            assert_eq!(state.position, Vec3::new(0.5, 64.0, 0.5), "{version}");
            assert!(state.on_ground, "{version}");
        }
        // under a ceiling 2.5 above the feet the lift stops at it
        let mut low = floor();
        low.set_block((0, 66, 0), BlockState { id: 1, shape: Some(BlockShape::full_block()), ..BlockState::default() });
        let mut capped = SimulationState { position: Vec3::new(0.5, 64.0, 0.5), velocity: Vec3::ZERO, on_ground: true, ..swimmer(2) };
        release(version, &mut capped, &low);
        let head_room = if release_of.at_least(1, 13, 0) { 66.0 - 1.8 - 64.0 } else { 0.0 };
        assert!((capped.riptide_lift.y - head_room).abs() < 1e-9, "{version}: {:?}", capped.riptide_lift);
    }
}

#[test]
fn a_passenger_is_launched_up_to_1_21_11_only() {
    let world = WorldSnapshot::new();
    for &version in SUPPORTED_VERSIONS {
        let release_of = MinecraftVersion::parse(version).unwrap();
        let mut state = SimulationState { passenger: true, ..swimmer(1) };
        release(version, &mut state, &world);
        let launched = release_of.at_least(1, 13, 0) && !release_of.at_least(26, 1, 0);
        assert_eq!(state.velocity.z, if launched { 1.5 } else { 0.0 }, "{version}");
    }
}

#[test]
fn a_release_not_supported_is_refused() {
    let mut state = swimmer(1);
    assert_eq!(
        release_riptide("1.12.1", &mut state, &WorldSnapshot::new(), PhysicsConfig::default()),
        Err("unsupported decompiled Minecraft version: 1.12.1".to_string())
    );
    assert_eq!(state.riptide_launch, Some(1), "a refused release leaves the state");
}
