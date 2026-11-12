//! Paths of the engine no recorded tick reaches, each held to what vanilla's code gives: Mth's helpers
//! (values from the 1.21.4 Mth methods run on Java 21), a spear stab's recoil on the server, the
//! bookkeeping of a world's cells, the merging of full cubes before 1.14, and the refusals of the
//! entity inventory.

use crate::entity_travel::{refuse_for_mob_travel, refuse_for_player_tick, travel_support, TravelFamily};
use crate::math::{atan2_vanilla_on, hsv_to_argb, wrap_degrees_i32, wrap_degrees_i64, Aabb, HitFace, Vec3};
use crate::simulation::{PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::world::{BlockShape, BlockState, EntityCollisionContext, WorldSnapshot};

#[test]
fn hsv_to_argb_is_mths_in_every_sixth_of_the_hue() {
    // Mth.hsvToArgb (1.21.4 Mth.java:451-497), run on Java 21 with alpha 200
    for (hue, saturation, value, argb) in [
        (0.25, 1.0, 1.0, 0xc87f_ff00_u32),
        (0.5625, 1.0, 1.0, 0xc800_9fff),
        (0.75, 1.0, 1.0, 0xc87f_00ff),
        (0.875, 1.0, 1.0, 0xc8ff_00bf),
        (0.3, 0.6, 0.9, 0xc877_e55b),
        (0.6, 0.25, 0.5, 0xc85f_6c7f),
        (0.7, 0.4, 0.8, 0xc88a_7acc),
        (0.95, 0.7, 0.3, 0xc84c_1627),
    ] {
        assert_eq!(hsv_to_argb(hue, saturation, value, 200) as u32, argb, "{hue} {saturation} {value}");
    }
}

#[test]
#[should_panic(expected = "invalid HSV section")]
fn a_negative_hue_has_no_sixth_as_mth_throws() {
    // (int)(-0.5F * 6.0F) % 6 is -3: Mth.hsvToArgb's switch throws (1.21.4 Mth.java:491)
    let _ = hsv_to_argb(-0.5, 1.0, 1.0, 255);
}

#[test]
fn wrap_degrees_of_an_int_and_a_long_are_mths() {
    // Mth.wrapDegrees(int) and Mth.wrapDegrees(long) (1.21.4 Mth.java:188-212) on Java 21
    for (value, wrapped) in [(-181, 179), (-540, -180), (181, -179), (539, 179), (180, -180), (-180, -180), (1000, -80)] {
        assert_eq!(wrap_degrees_i32(value), wrapped, "{value}");
    }
    for (value, wrapped) in [(200_i64, -160.0_f32), (-200, 160.0), (9_000_000_000_000_000_007, 7.0), (-9_000_000_000_000_000_181, 179.0)] {
        assert_eq!(wrap_degrees_i64(value), wrapped, "{value}");
    }
}

#[test]
fn atan2_of_a_nan_is_nan_on_every_java() {
    // Mth.atan2: `if (Double.isNaN(d)) return Double.NaN` of x * x + y * y (1.21.4 Mth.java:370-373)
    for java in [8, 21] {
        assert!(atan2_vanilla_on(f64::NAN, 1.0, java).is_nan());
        assert!(atan2_vanilla_on(1.0, f64::NAN, java).is_nan());
    }
}

#[test]
fn the_faces_have_directions_names() {
    // Direction.getName (1.21.4 Direction.java:32-37, DOWN("down") ... EAST("east"))
    let names: Vec<&str> = [HitFace::Down, HitFace::Up, HitFace::North, HitFace::South, HitFace::West, HitFace::East]
        .into_iter()
        .map(HitFace::name)
        .collect();
    assert_eq!(names, ["down", "up", "north", "south", "west", "east"]);
}

#[test]
fn every_travel_family_has_its_name() {
    let names: Vec<&str> = [
        TravelFamily::Player,
        TravelFamily::Boat,
        TravelFamily::Ridden,
        TravelFamily::Mob,
        TravelFamily::Dragon,
        TravelFamily::Unsupported,
        TravelFamily::Entity,
    ]
    .into_iter()
    .map(TravelFamily::name)
    .collect();
    assert_eq!(names, ["player", "boat", "ridden", "mob", "dragon", "unsupported", "entity"]);
    assert_eq!(travel_support("shulker", MinecraftVersion::new(1, 9, 4)).family.map(TravelFamily::name), Some("unsupported"));
}

#[test]
fn the_inventory_refuses_what_has_no_path() {
    assert_eq!(
        refuse_for_player_tick("shulker"),
        Err("no travel simulation for shulker: it is not run on the player's equations".to_string())
    );
    assert_eq!(refuse_for_player_tick("not_an_entity"), Err("not_an_entity is not in the engine's entity inventory".to_string()));
    assert_eq!(refuse_for_player_tick("player"), Ok(()));
    // "living" names any LivingEntity of no kind of its own, in every release
    assert_eq!(refuse_for_mob_travel("living", MinecraftVersion::new(1, 8, 8)), Ok(()));
    // the strider is in the registry from 1.16 (1.16 EntityType STRIDER), unridden it travels as a mob
    assert_eq!(refuse_for_mob_travel("strider", MinecraftVersion::new(1, 16, 0)), Ok(()));
    assert_eq!(refuse_for_mob_travel("strider", MinecraftVersion::new(1, 15, 2)), Err("strider is not in 1.15.2".to_string()));
    assert_eq!(refuse_for_mob_travel("skeleton_horse", MinecraftVersion::new(1, 8, 8)), Ok(()));
}

fn tick(version: &str, mut state: SimulationState) -> SimulationState {
    crate::versioned::dispatch(version, &mut state, &WorldSnapshot::new(), PhysicsConfig::default()).expect("a release");
    state
}

/// A server's player in the air, sprinting, with a stab's knockback to take.
fn stabbed(knockback: f32) -> SimulationState {
    SimulationState {
        position: Vec3::new(0.5, 80.0, 0.5),
        velocity: Vec3::new(0.5, 0.1, -0.25),
        half_width: 0.3,
        height: 1.8,
        is_player: true,
        sprinting: true,
        client_side: false,
        stab_hit: Some(knockback),
        ..SimulationState::default()
    }
}

/// The same player as the recoil leaves it: its horizontal motion times 0.6 for each time
/// causeExtraKnockback ran, not sprinting, the stab taken.
fn recoiled(knockback: f32, times: u32) -> SimulationState {
    let mut state = SimulationState { sprinting: false, stab_hit: None, ..stabbed(knockback) };
    for _ in 0..times {
        state.velocity.x *= 0.6;
        state.velocity.z *= 0.6;
    }
    state
}

#[test]
fn a_stab_takes_six_tenths_of_the_attackers_motion_on_the_server_from_1_21_11() {
    // Player.causeExtraKnockback: the 0.4F + knockback is above 0, so it multiplies the motion by
    // (0.6, 1.0, 0.6) and ends the sprint (1.21.11 Player.java:1070-1080, called at :1167); the tick
    // then runs as for that motion
    let same = |a: SimulationState, b: SimulationState, what: &str| assert_eq!(format!("{a:?}"), format!("{b:?}"), "{what}");
    same(tick("1.21.11", stabbed(0.0)), tick("1.21.11", recoiled(0.0, 1)), "1.21.11");
    same(tick("1.21.11", stabbed(1.0)), tick("1.21.11", recoiled(1.0, 1)), "1.21.11 with knockback");
    same(tick("26.1.2", stabbed(1.0)), tick("26.1.2", recoiled(1.0, 1)), "26.1.2");
    // from 26.2 a second call with the weapon's knockback alone when it is above 0 (26.2 Player.java:1152-1153)
    same(tick("26.2", stabbed(1.0)), tick("26.2", recoiled(1.0, 2)), "26.2 with knockback");
    same(tick("26.3", stabbed(0.0)), tick("26.3", recoiled(0.0, 1)), "26.3 with none");
    // before spears there is no stab, and a client never takes it (MultiPlayerGameMode sends STAB and stops)
    let unstabbed = |knockback| SimulationState { stab_hit: None, ..stabbed(knockback) };
    same(tick("1.21.10", stabbed(1.0)), tick("1.21.10", unstabbed(1.0)), "1.21.10");
    let client = |state: SimulationState| SimulationState { client_side: true, ..state };
    same(tick("26.3", client(stabbed(1.0))), tick("26.3", client(unstabbed(1.0))), "26.3 client");
    // and the recoil did change the tick
    assert_ne!(tick("26.3", stabbed(1.0)).velocity, tick("26.3", unstabbed(1.0)).velocity);
}

fn stone() -> BlockState {
    BlockState { id: 1, shape: Some(BlockShape::full_block()), ..BlockState::default() }
}

#[test]
fn a_cells_bookkeeping_follows_its_writes() {
    let mut world = WorldSnapshot::new();
    world.set_block((1, 64, 1), stone());
    // properties set to none are gone
    world.set_block_properties((1, 64, 1), vec![("axis".to_string(), "y".to_string())]);
    assert_eq!(world.block_properties((1, 64, 1)).len(), 1);
    world.set_block_properties((1, 64, 1), Vec::new());
    assert!(world.block_properties((1, 64, 1)).is_empty());
    // a block entity of a chunk marked unloaded is no write: EmptyLevelChunk.removeBlockEntity is empty
    world.set_block_entity((1, 64, 1), "minecraft:chest".to_string());
    world.set_chunk_loaded(0, 0, false);
    world.remove_block_entity((1, 64, 1));
    world.set_chunk_loaded(0, 0, true);
    assert_eq!(world.block_entity((1, 64, 1)), Some("minecraft:chest"));
    world.remove_block_entity((1, 64, 1));
    assert_eq!(world.block_entity((1, 64, 1)), None);
    // a section the server sent stays until its last block is gone
    world.set_section_present(0, 4, 0, true);
    assert!(world.is_section_present(0, 4, 0));
    world.remove_block((1, 64, 1));
    assert!(!world.is_section_present(0, 4, 0));
    assert_eq!(world.section_block_count(0, 4, 0), 0);
}

#[test]
fn full_cubes_merge_up_a_column_before_1_14() {
    // DiscreteVoxelShape.forAllBoxes(combine): a run along z grows along x and then up y while the
    // rectangle above is full (1.14.4 DiscreteVoxelShape.java:193-250, as 1.13.2 :200-253)
    let mut world = WorldSnapshot::new();
    for (x, y, z) in [(0, 64, 0), (1, 64, 0), (0, 64, 1), (1, 64, 1), (0, 65, 0), (1, 65, 0), (0, 65, 1), (1, 65, 1), (0, 66, 0)] {
        world.set_block((x, y, z), stone());
    }
    let query = Aabb::new(Vec3::new(-0.5, 63.5, -0.5), Vec3::new(1.5, 67.5, 1.5));
    let boxes = world.block_collision_boxes_merging_full_cubes(query, &EntityCollisionContext::default());
    assert_eq!(
        boxes,
        vec![
            Aabb::new(Vec3::new(0.0, 64.0, 0.0), Vec3::new(2.0, 66.0, 2.0)),
            Aabb::new(Vec3::new(0.0, 66.0, 0.0), Vec3::new(1.0, 67.0, 1.0)),
        ]
    );
}
