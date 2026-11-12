//! The tick at the numeric edges, against what the source says of each: a cut by `<` keeps what equals it, the smallest
//! subnormal is a motion like any other under a cut, a signed zero the tick does not touch keeps its sign, and a position at
//! the legal limit moves by what its spacing allows. No epsilon is added anywhere, so these compare exact bits.

use crate::math::{floor_i32_for, Vec3};
use crate::phase::{traced, Phase};
use crate::simulation::{PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::versioned::dispatch;
use crate::world::WorldSnapshot;

const LIMIT: f64 = 2.9999984e7;

fn in_air(velocity: Vec3) -> SimulationState {
    SimulationState {
        position: Vec3::new(0.5, 80.0, 0.5),
        previous_position: Some(Vec3::new(0.5, 80.0, 0.5)),
        velocity,
        half_width: 0.3,
        height: 1.8,
        is_player: true,
        controlled_by_local_instance: true,
        ..SimulationState::default()
    }
}

fn after(version: &str, mut state: SimulationState) -> SimulationState {
    dispatch(version, &mut state, &WorldSnapshot::default(), PhysicsConfig::default()).unwrap();
    state
}

/// What the tick's cut leaves of a horizontal component: the air's drag scales what it kept, and nothing brings back what it did not.
fn kept(version: &str, x: f64, z: f64) -> (bool, bool) {
    let state = after(version, in_air(Vec3::new(x, 0.0, z)));
    (state.velocity.x != 0.0, state.velocity.z != 0.0)
}

#[test]
fn the_smallest_subnormal_motion_is_cut_as_any_small_motion() {
    for version in ["1.8.8", "1.20.4", "1.21.5", "26.3"] {
        for x in [f64::from_bits(1), -f64::from_bits(1), f64::MIN_POSITIVE, -f64::MIN_POSITIVE] {
            let state = after(version, in_air(Vec3::new(x, 0.0, 0.0)));
            assert_eq!(state.velocity.x.to_bits(), 0.0_f64.to_bits(), "{version} {x:e}");
        }
    }
}

#[test]
fn the_cut_is_below_its_value_and_never_at_it() {
    // LivingEntity.aiStep: Math.abs(x) < 0.003 from 1.9, 0.005 before; a player's radial test from 1.21.5
    for (version, cut) in [("1.8.8", 0.005), ("1.9.4", 0.003), ("1.20.4", 0.003), ("26.3", 0.003)] {
        let below = f64::from_bits(f64::to_bits(cut) - 1);
        let above = f64::from_bits(f64::to_bits(cut) + 1);
        assert_eq!(kept(version, below, 0.0).0, false, "{version} below");
        assert_eq!(kept(version, cut, 0.0).0, true, "{version} at");
        assert_eq!(kept(version, above, 0.0).0, true, "{version} above");
    }
}

#[test]
fn two_small_components_part_the_radial_cutoff_from_the_component_one() {
    let (x, z) = (0.0025, 0.0025);
    assert_eq!(kept("1.21.4", x, z), (false, false));
    assert_eq!(kept("1.21.5", x, z), (true, true));
    assert_eq!(kept("26.3", x, z), (true, true));
    // one of them alone is short of the length on both sides of the gate
    assert_eq!(kept("1.21.4", x, 0.0), (false, false));
    assert_eq!(kept("1.21.5", x, 0.0), (false, false));
}

#[test]
fn a_signed_zero_reaches_the_tick_with_its_sign_and_the_cut_clears_it() {
    let mut state = in_air(Vec3::new(0.0, 0.0, -0.0));
    let world = WorldSnapshot::default();
    let (_, events) = traced(|| dispatch("26.3", &mut state, &world, PhysicsConfig::default()).unwrap());
    let at = |phase| events.iter().find(|event| event.phase == phase).unwrap().state.velocity.z.to_bits();
    // a tick with no rocket adds nothing, not a zero that would turn -0.0 into 0.0 (the rocket's boost is its own entity's tick)
    assert_eq!(at(Phase::TickStart), (-0.0_f64).to_bits());
    // the cut sets 0.0, whichever zero it found (26.3 LivingEntity.aiStep)
    assert_eq!(at(Phase::BeforeTravel), 0.0_f64.to_bits());
}

#[test]
fn a_position_at_the_legal_limit_moves_on_its_own_spacing() {
    for sign in [1.0, -1.0] {
        let at = sign * LIMIT;
        let mut state = in_air(Vec3::new(0.5, 0.0, 0.0));
        state.position.x = at;
        state.previous_position = Some(state.position);
        let moved = after("26.3", state);
        assert_eq!(moved.position.x, at + 0.5, "the sum of two doubles, rounded once and no more");
    }
}

#[test]
fn negative_block_rounding_at_the_limit_is_the_floors() {
    let release = MinecraftVersion::new(26, 3, 0);
    assert_eq!(floor_i32_for(release, -LIMIT), -29_999_984);
    assert_eq!(floor_i32_for(release, f64::from_bits((-LIMIT).to_bits() - 1)), -29_999_984 + 0);
    assert_eq!(floor_i32_for(release, f64::from_bits((-LIMIT).to_bits() + 1)), -29_999_985);
    assert_eq!(floor_i32_for(release, -1.0e-300), -1);
    assert_eq!(floor_i32_for(release, -0.0), 0);
}
