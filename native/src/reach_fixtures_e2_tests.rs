//! Fixtures for the B280 reach (scripts/reach-probe.cjs): each runs an ungated function on the
//! releases either side of a boundary where a vanilla method it mirrors changed, so that the probe
//! sees it reached on both sides, and asserts what vanilla does there.

use crate::math::Vec3;
use crate::moving_entity::{tick_moving_entity, MovingEntityInputs, MovingEntityKind, MovingEntityState};
use crate::version::MinecraftVersion;
use crate::world::{BlockState, FluidState, WorldSnapshot};

/// Every registered release, oldest first (scripts/registered-releases.cjs).
pub(crate) const RELEASES: [&str; 50] = [
    "1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2", "1.14.4", "1.15", "1.15.1", "1.15.2",
    "1.16", "1.16.1", "1.16.2", "1.16.3", "1.16.4", "1.16.5", "1.17", "1.17.1", "1.18", "1.18.1",
    "1.18.2", "1.19", "1.19.1", "1.19.2", "1.19.3", "1.19.4", "1.20", "1.20.1", "1.20.2", "1.20.3",
    "1.20.4", "1.20.5", "1.20.6", "1.21", "1.21.1", "1.21.2", "1.21.3", "1.21.4", "1.21.5", "1.21.6",
    "1.21.7", "1.21.8", "1.21.9", "1.21.10", "1.21.11", "26.1", "26.1.1", "26.1.2", "26.2", "26.3",
];

/// The registered releases from `first` on.
pub(crate) fn releases_from(first: &str) -> impl Iterator<Item = (&'static str, MinecraftVersion)> {
    let start = RELEASES.iter().position(|name| *name == first).expect("a registered release");
    RELEASES[start..].iter().map(|name| (*name, MinecraftVersion::parse(name).expect("it parses")))
}

#[test]
fn a_fast_concrete_powder_stops_at_the_water_it_passes_through_on_every_release_from_1_12() {
    // FallingBlockEntity#tick: a concrete powder moving faster than 1 a tick takes the first water
    // source on the segment of its move as where it lands (1.12.2 FallingBlockEntity.java:128-135
    // rayTrace, 1.14.4 BlockGetter#clip SOURCE_ONLY, 26.3 the same)
    let mut world = WorldSnapshot::new();
    world.set_block(
        (0, 103, 0),
        BlockState { fluid: FluidState::Water, fluid_source: true, ..BlockState::default() },
    );
    for (name, release) in releases_from("1.12.2") {
        let mut state = MovingEntityState::new(MovingEntityKind::FallingBlock, Vec3::new(0.5, 105.5, 0.5), Vec3::new(0.0, -3.0, 0.0))
            .on_server();
        state.time = 3;
        state.concrete_powder = true;
        let landed = tick_moving_entity(&mut state, &mut world, &MovingEntityInputs::default(), release).landed_at;
        assert_eq!(landed, Some((0, 103, 0)), "{name}");
    }
}

#[test]
fn lava_in_the_box_keeps_the_fire_ticks_from_their_reset_until_1_21_5() {
    // Entity.move's fire test of the box shrunk by 0.001 (1.8.8 Entity.java:706-722 containsFireBlock:
    // FIRE, FLOWING_LAVA or LAVA; 1.15.2 :635-653; 1.16 :672-680 #fire or LAVA; 1.17 :630-648 by
    // 1.0E-6; 1.21.2-1.21.4 the visited blocks): with lava in it the ticks do not go back to
    // -getFireImmuneTicks, without they do
    use crate::simulation::{apply_inside_fire_effects, InsideEffects, SimulationState};
    let mut lava = WorldSnapshot::new();
    lava.set_block((0, 100, 0), BlockState { fluid: FluidState::Lava, fluid_source: true, ..BlockState::default() });
    let air = WorldSnapshot::new();
    let ticks_after = |world: &WorldSnapshot, release| {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 100.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            remaining_fire_ticks: Some(-5),
            ..SimulationState::default()
        };
        let inside = [((0, 100, 0), true)];
        apply_inside_fire_effects(&mut state, world, &inside, &InsideEffects::default(), release);
        state.remaining_fire_ticks
    };
    for (name, release) in RELEASES[..RELEASES.iter().position(|name| *name == "1.21.5").unwrap()]
        .iter()
        .map(|name| (*name, MinecraftVersion::parse(name).unwrap()))
    {
        assert_ne!(ticks_after(&lava, release), Some(-20), "{name}");
        assert_eq!(ticks_after(&air, release), Some(-20), "{name}");
    }
}

#[test]
fn an_arrow_in_the_open_moves_by_its_motion_then_slows_on_every_release() {
    // AbstractArrow#tick's flight with nothing to hit: the position moves by the whole motion,
    // then the motion is scaled by the drag and gravity taken off (1.8.8 AbstractArrow.java:148-380,
    // 1.9.4 :165-195, 1.13.2, 1.15, 1.21 :153-281, 1.21.11 :165-254)
    use crate::projectile::{tick_projectile_in_release, ProjectileKind, ProjectileState};
    let world = WorldSnapshot::new();
    for (name, release) in releases_from("1.8.8") {
        for kind in [ProjectileKind::Arrow, ProjectileKind::Trident] {
            if kind == ProjectileKind::Trident && !release.at_least(1, 13, 0) {
                continue;
            }
            let mut state = ProjectileState::for_kind(kind, Vec3::new(0.5, 100.0, 0.5), Vec3::new(0.5, 0.0, 0.0));
            let _ = tick_projectile_in_release(&mut state, &world, release);
            assert_eq!(state.position.x, 1.0, "{name} {kind:?}");
            assert!(state.velocity.x < 0.5 && state.velocity.y < 0.0, "{name} {kind:?}: {:?}", state.velocity);
        }
    }
}
