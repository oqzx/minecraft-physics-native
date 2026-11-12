use super::*;
use crate::projectile::{
    tick_arrow_in_world, tick_arrow_with_targets, tick_projectile_with_targets,
    tick_trident_with_targets, ArrowState, ProjectileKind, ProjectileState, ProjectileTickResult,
    ArrowTickResult, TridentState,
};
use crate::world::{BlockShape, BlockState};

const fn v(major: u8, minor: u8, patch: u8) -> MinecraftVersion {
    MinecraftVersion::new(major as _, minor as _, patch as _)
}
const R1_8: MinecraftVersion = v(1, 8, 8);
const R1_12: MinecraftVersion = v(1, 12, 2);
const R1_13: MinecraftVersion = v(1, 13, 2);
const R1_14: MinecraftVersion = v(1, 14, 4);
const R1_15: MinecraftVersion = v(1, 15, 0);
const R1_16: MinecraftVersion = v(1, 16, 5);
const R1_20_2: MinecraftVersion = v(1, 20, 2);
const R1_20_4: MinecraftVersion = v(1, 20, 4);
const R1_20_5: MinecraftVersion = v(1, 20, 5);
const R1_21: MinecraftVersion = v(1, 21, 0);
const R1_21_1: MinecraftVersion = v(1, 21, 1);
const R1_21_2: MinecraftVersion = v(1, 21, 2);
const R1_21_5: MinecraftVersion = v(1, 21, 5);
const R1_21_6: MinecraftVersion = v(1, 21, 6);
const R1_21_8: MinecraftVersion = v(1, 21, 8);
const R1_21_9: MinecraftVersion = v(1, 21, 9);
const R1_21_10: MinecraftVersion = v(1, 21, 10);
const R1_21_11: MinecraftVersion = v(1, 21, 11);
const R26_2: MinecraftVersion = v(26, 2, 0);
const R26_3: MinecraftVersion = v(26, 3, 0);
const EVERY: [MinecraftVersion; 10] = [
    R1_8, R1_13, R1_14, R1_16, R1_20_5, R1_21_1, R1_21_2, R1_21_6, R1_21_11, R26_3,
];

/// A one-wide, two-tall target whose near face is at `x`, centred on z 0, standing on y 0.
fn target(id: i32, x: f64) -> ProjectileTarget {
    let position = Vec3::new(x + 0.5, 0.0, 0.0);
    ProjectileTarget::new(id, position, target_hitbox(position, 1.0, 2.0, 1.0))
}

fn full_block(position: (i32, i32, i32)) -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    world.set_block(
        position,
        BlockState {
            shape: Some(BlockShape::full_block()),
            outline_shape: Some(BlockShape::full_block()),
            ..Default::default()
        },
    );
    world
}

/// An arrow at y 1 along x at 3 blocks a tick.
fn arrow() -> ArrowState {
    ArrowState::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0))
}

/// Past every margin's first ticks and every owner's immunity.
fn aged() -> ProjectileHitState {
    ProjectileHitState {
        tick_count: 20,
        flying_ticks: 20,
        left_owner: true,
        ..Default::default()
    }
}

fn tick(
    state: &mut ArrowState,
    hits: &mut ProjectileHitState,
    targets: &[ProjectileTarget],
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (ArrowTickResult, ProjectileHitReport) {
    tick_arrow_with_targets(state, hits, targets, None, world, release)
}

fn shot(
    targets: &[ProjectileTarget],
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (ArrowState, ArrowTickResult, ProjectileHitReport) {
    let mut state = arrow();
    let (result, report) = tick(&mut state, &mut aged(), targets, world, release);
    (state, result, report)
}

fn ids(report: &ProjectileHitReport) -> Vec<i32> {
    report.entity_hits.iter().map(|hit| hit.target).collect()
}

fn owner(id: i32) -> ProjectileOwner {
    ProjectileOwner {
        id,
        root_vehicle: id,
        eye_position: Vec3::new(0.0, 1.62, 0.0),
        width: 0.6,
        alive: true,
        player: true,
        spectator: false,
        removed: false,
        won_game: false,
    }
}

#[test]
fn hits_the_nearest_target_on_the_ray_and_is_gone() {
    for release in EVERY {
        let (_, result, report) =
            shot(&[target(2, 2.0), target(1, 1.0)], &WorldSnapshot::new(), release);
        assert_eq!(ids(&report), [1], "{release:?}");
        assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hurt);
        assert!(result.discarded);
        assert!(report.block_hit.is_none());
    }
}

#[test]
fn a_hit_is_at_the_entity_until_1_21_1_and_on_the_ray_from_1_21_2() {
    let world = WorldSnapshot::new();
    let (_, _, before) = shot(&[target(1, 1.0)], &world, R1_21_1);
    assert_eq!(before.entity_hits[0].location, Vec3::new(1.5, 0.0, 0.0));
    let (_, _, after) = shot(&[target(1, 1.0)], &world, R1_21_2);
    assert_eq!(after.entity_hits[0].location, Vec3::new(1.0 - f64::from(0.3_f32), 1.0, 0.0));
}

#[test]
fn misses_what_is_past_the_margin() {
    let world = WorldSnapshot::new();
    let beside = |offset: f64| {
        let mut far = target(1, 1.0);
        far.position.z = offset + 0.5;
        far.bounding_box = target_hitbox(far.position, 1.0, 2.0, 1.0);
        far
    };
    let margin = f64::from(TARGET_MARGIN);
    for release in [R1_8, R1_16, R1_21_5] {
        let (_, _, inside) = shot(&[beside(margin - 1.0e-6)], &world, release);
        assert_eq!(ids(&inside), [1], "{release:?} inside the margin");
        let (_, result, outside) = shot(&[beside(margin + 1.0e-6)], &world, release);
        assert!(outside.entity_hits.is_empty(), "{release:?} past the margin");
        assert_eq!(result.position.x, 3.0);
    }
}

#[test]
fn the_margin_takes_the_tick_count_as_a_java_int() {
    // (tickCount - 2) / 20.0F of an int, which wraps (26.3 ProjectileUtil.java:139-141)
    assert_eq!(target_margin(R1_21_6, i32::MIN), TARGET_MARGIN);
    assert_eq!(target_margin(R1_21_6, i32::MIN + 2), 0.0);
}

#[test]
fn the_margin_opens_with_age_from_1_21_6() {
    assert_eq!(target_margin(R1_21_5, 1), TARGET_MARGIN);
    assert_eq!(target_margin(R1_21_6, 2), 0.0);
    assert_eq!(target_margin(R1_21_6, 3), 1.0 / 20.0);
    assert_eq!(target_margin(R1_21_6, 8), TARGET_MARGIN);
    assert_eq!(target_margin(R1_21_6, 100), TARGET_MARGIN);
    // 0.15 off the box needs a margin of 0.2F, the sixth tick
    let mut graze = target(1, 1.0);
    graze.position.z = 0.65;
    graze.bounding_box = target_hitbox(graze.position, 1.0, 2.0, 1.0);
    let world = WorldSnapshot::new();
    let aged_to = |ticks| ProjectileHitState {
        tick_count: ticks,
        left_owner: true,
        ..Default::default()
    };
    let (_, report) = tick(&mut arrow(), &mut aged_to(3), &[graze], &world, R1_21_6);
    assert!(report.entity_hits.is_empty(), "0.1F at tick 4");
    let (_, report) = tick(&mut arrow(), &mut aged_to(5), &[graze], &world, R1_21_6);
    assert_eq!(ids(&report), [1], "0.2F at tick 6");
    // 1.21.5's 0.3F has no age
    let (_, report) = tick(&mut arrow(), &mut aged_to(0), &[graze], &world, R1_21_5);
    assert_eq!(ids(&report), [1]);
}

#[test]
fn a_block_shields_what_is_behind_it_and_not_what_is_before_it() {
    let world = full_block((2, 1, 0));
    for release in EVERY {
        let (_, result, report) = shot(&[target(1, 3.5)], &world, release);
        assert!(report.entity_hits.is_empty(), "{release:?}");
        let hit = report.block_hit.expect("the block");
        assert_eq!((hit.block, hit.face, hit.border), ((2, 1, 0), HitFace::West, false));
        assert_eq!(hit.location, Vec3::new(2.0, 1.0, 0.0));
        assert!(result.in_ground);
        let (_, _, before) = shot(&[target(1, 0.75)], &world, release);
        assert_eq!(ids(&before), [1], "{release:?}");
        assert!(before.block_hit.is_none());
    }
}

#[test]
fn an_equal_distance_keeps_the_first_candidate_from_1_14() {
    let world = WorldSnapshot::new();
    let (_, _, report) = shot(&[target(7, 1.0), target(8, 1.0)], &world, R1_14);
    assert_eq!(ids(&report), [7]);
}

#[test]
fn a_hit_at_the_start_gives_way_to_the_next_candidate_before_1_14() {
    // the first inflated box begins at the arrow, a distance of 0
    let world = WorldSnapshot::new();
    let start = f64::from(TARGET_MARGIN);
    let (first, second) = (target(7, start), target(8, start + 1.5));
    let (_, _, legacy) = shot(&[first, second], &world, R1_12);
    assert_eq!(ids(&legacy), [8]);
    assert_eq!(ids(&shot(&[first], &world, R1_12).2), [7]);
}

#[test]
fn a_start_inside_a_target_hits_by_the_exit_face_before_1_13_only() {
    let inside = target(1, -0.5);
    let world = WorldSnapshot::new();
    assert_eq!(ids(&shot(&[inside], &world, R1_12).2), [1]);
    assert!(shot(&[inside], &world, R1_13).2.entity_hits.is_empty());
}

#[test]
fn scale_and_pose_size_the_box() {
    let at = Vec3::new(1.0, 2.0, 3.0);
    let big = target_hitbox(at, 0.6, 1.8, 2.0);
    let half = f64::from(0.6_f32 * 2.0 / 2.0);
    assert_eq!(big.min, Vec3::new(1.0 - half, 2.0, 3.0 - half));
    assert_eq!(big.max.y, 2.0 + f64::from(1.8_f32 * 2.0));
    // a swimming player's 0.6 height is under the ray that a standing 1.8 meets
    let world = WorldSnapshot::new();
    let foot = Vec3::new(1.5, 0.0, 0.0);
    let swimming = ProjectileTarget::new(1, foot, target_hitbox(foot, 0.6, 0.6, 1.0));
    assert!(shot(&[swimming], &world, R1_14).2.entity_hits.is_empty());
    let standing = ProjectileTarget::new(1, foot, target_hitbox(foot, 0.6, 1.8, 1.0));
    assert_eq!(ids(&shot(&[standing], &world, R1_14).2), [1]);
}

#[test]
fn dead_spectating_and_unpickable_targets_are_no_candidates() {
    let world = WorldSnapshot::new();
    let dead = ProjectileTarget { alive: false, ..target(1, 1.0) };
    let spectator = ProjectileTarget { spectator: true, ..target(1, 1.0) };
    let ghost = ProjectileTarget { pickable: false, ..target(1, 1.0) };
    for release in [R1_12, R26_3] {
        assert!(shot(&[dead], &world, release).2.entity_hits.is_empty());
        assert!(shot(&[spectator], &world, release).2.entity_hits.is_empty());
        assert!(shot(&[ghost], &world, release).2.entity_hits.is_empty());
    }
    // 1.8 asks Entity.collides alone
    assert_eq!(ids(&shot(&[dead], &world, R1_8).2), [1]);
    assert!(shot(&[ghost], &world, R1_8).2.entity_hits.is_empty());
}

#[test]
fn the_owner_is_immune_for_five_flying_ticks_until_1_15() {
    let world = WorldSnapshot::new();
    let shooter = target(1, 1.0);
    for release in [R1_8, R1_13, R1_15] {
        let mut young = ProjectileHitState { flying_ticks: 3, ..Default::default() };
        let (_, report) = tick_arrow_with_targets(
            &mut arrow(), &mut young, &[shooter], Some(owner(1)), &world, release,
        );
        assert!(report.entity_hits.is_empty(), "{release:?} flying tick 4");
        assert_eq!(young.flying_ticks, 4);
        let (_, report) = tick_arrow_with_targets(
            &mut arrow(), &mut young, &[shooter], Some(owner(1)), &world, release,
        );
        assert_eq!(ids(&report), [1], "{release:?} flying tick 5");
    }
}

#[test]
fn the_owner_and_its_vehicle_are_immune_until_it_has_left_from_1_16() {
    let world = WorldSnapshot::new();
    let rider = owner(1);
    let boat = ProjectileTarget { root_vehicle: 1, ..target(2, 1.0) };
    let mut held = ProjectileHitState { flying_ticks: 50, ..Default::default() };
    let (_, report) =
        tick_arrow_with_targets(&mut arrow(), &mut held, &[boat], Some(rider), &world, R1_16);
    assert!(report.entity_hits.is_empty());
    assert!(!held.left_owner);
    // until 1.15 the vehicle is no owner
    let (_, report) = tick_arrow_with_targets(
        &mut arrow(), &mut ProjectileHitState::default(), &[boat], Some(rider), &world, R1_15,
    );
    assert_eq!(ids(&report), [2]);
    let mut left = ProjectileHitState { left_owner: true, ..Default::default() };
    let (_, report) =
        tick_arrow_with_targets(&mut arrow(), &mut left, &[boat], Some(rider), &world, R1_16);
    assert_eq!(ids(&report), [2]);
}

#[test]
fn leaving_the_owner_is_tested_where_each_release_tests_it() {
    // the owner meets the search box of the arrow's start and not of its end
    let world = WorldSnapshot::new();
    let shooter = target(1, -2.0);
    let test = |release| {
        let mut hits = ProjectileHitState::default();
        tick_arrow_with_targets(&mut arrow(), &mut hits, &[shooter], Some(owner(1)), &world, release);
        hits.left_owner
    };
    assert!(!test(R1_16), "at the start of the tick");
    assert!(!test(R1_21_1));
    assert!(test(R1_21_2), "at the moved arrow");
    assert!(test(R1_21_8));
    assert!(!test(R1_21_9), "before the ray");
    assert!(!test(R26_3));
}

#[test]
fn no_owner_has_left_at_once() {
    let world = WorldSnapshot::new();
    let mut hits = ProjectileHitState::default();
    tick_arrow_with_targets(&mut arrow(), &mut hits, &[], None, &world, R1_16);
    assert!(hits.left_owner);
}

#[test]
fn pierce_goes_through_up_to_its_level_and_remembers_the_ids() {
    let world = WorldSnapshot::new();
    let targets = [target(1, 0.5), target(2, 1.0), target(3, 1.5), target(4, 2.0)];
    for release in [R1_14, R1_21_1, R1_21_2, R26_3] {
        let mut hits = ProjectileHitState { pierce_level: 1, ..aged() };
        let (result, report) = tick(&mut arrow(), &mut hits, &targets, &world, release);
        assert_eq!(ids(&report)[..2], [1, 2], "{release:?}");
        let last = report.entity_hits[2];
        assert_eq!((last.target, last.reaction), (3, ProjectileReaction::Spent));
        assert_eq!(hits.pierced, [1, 2]);
        assert!(result.discarded);
    }
    // a pierced entity is not hit again, in a later tick either
    let mut hits = ProjectileHitState { pierce_level: 5, pierced: vec![1], ..aged() };
    let (_, report) = tick(&mut arrow(), &mut hits, &[target(1, 0.5)], &world, R1_14);
    assert!(report.entity_hits.is_empty());
    // there is no pierce before 1.14
    let mut hits = ProjectileHitState { pierce_level: 3, ..aged() };
    let (result, report) = tick(&mut arrow(), &mut hits, &targets, &world, R1_13);
    assert_eq!(ids(&report), [1]);
    assert!(result.discarded);
}

#[test]
fn a_pierced_step_forgets_the_block_until_1_21_1_and_keeps_it_from_1_21_2() {
    let world = full_block((2, 1, 0));
    let mut hits = ProjectileHitState { pierce_level: 2, ..aged() };
    let mut before = arrow();
    let (result, report) = tick(&mut before, &mut hits, &[target(1, 0.75)], &world, R1_21_1);
    assert!(report.block_hit.is_none());
    assert!(!result.in_ground);
    assert_eq!(hits.pierced, [1]);
    assert_eq!(before.position.x, 3.0, "it flies on by its motion");

    let mut hits = ProjectileHitState { pierce_level: 2, ..aged() };
    let (result, report) = tick(&mut arrow(), &mut hits, &[target(1, 0.75)], &world, R1_21_2);
    assert_eq!(report.block_hit.expect("the block").block, (2, 1, 0));
    assert!(result.in_ground);
    assert_eq!(hits.pierce_level, 0, "a block ends the pierce");
    assert!(hits.pierced.is_empty());
}

#[test]
fn a_refusal_turns_the_arrow_back_by_each_release_s_arithmetic() {
    let world = WorldSnapshot::new();
    let wall = ProjectileTarget { hurt: false, ..target(1, 1.0) };
    let (state, result, report) = shot(&[wall], &world, R1_13);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Refused);
    assert!(!result.discarded);
    let turned = 3.0 * f64::from(-0.1_f32);
    assert_eq!(state.position.x, turned);
    assert_eq!(state.velocity.x, turned * f64::from(0.99_f32));
    assert_eq!(shot(&[wall], &world, R1_14).0.position.x, 3.0 * -0.1);
    // from 1.21.2 at the hit, and moved no further
    let (state, _, _) = shot(&[wall], &world, R1_21_2);
    assert_eq!(state.position.x, 1.0 - f64::from(0.3_f32));
    assert_eq!(state.velocity.x, 3.0 * -0.5 * 0.2 * f64::from(0.99_f32));
    // 26.3's -power * 0.5
    assert_eq!(shot(&[wall], &world, R26_3).0.velocity.x, 3.0 * (-0.2 * 0.5) * f64::from(0.99_f32));
    // the shooter's ticks start again until 1.15
    let mut hits = aged();
    tick(&mut arrow(), &mut hits, &[wall], &world, R1_15);
    assert_eq!(hits.flying_ticks, 0);
    let mut hits = aged();
    tick(&mut arrow(), &mut hits, &[wall], &world, R1_16);
    assert_eq!(hits.flying_ticks, 21);
}

#[test]
fn a_slow_refused_arrow_is_gone_from_1_9() {
    let world = WorldSnapshot::new();
    let wall = ProjectileTarget { hurt: false, ..target(1, 0.5) };
    let slow = |release| {
        let mut state = ArrowState::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.25, 0.0, 0.0));
        tick(&mut state, &mut aged(), &[wall], &world, release).0.discarded
    };
    assert!(!slow(R1_8));
    assert!(slow(R1_12), "0.0625 * 0.01 is under 0.001F");
    assert!(!slow(R1_14), "and over 1.0E-7");
}

#[test]
fn deflection_by_release() {
    let world = WorldSnapshot::new();
    let breeze = ProjectileTarget {
        hurt: false,
        deflects: true,
        deflect_random: 0.25,
        ..target(1, 1.0)
    };
    // before 1.20.3 only a refusal
    let (state, _, report) = shot(&[breeze], &world, R1_20_2);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Refused);
    assert_eq!(state.position.x, 3.0 * -0.1);
    // 1.20.3: a quarter turn at half the speed
    let (state, _, report) = shot(&[breeze], &world, R1_20_4);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Deflected);
    assert!(state.position.x.abs() < 1.0e-3 && (state.position.z.abs() - 1.5).abs() < 1.0e-3);
    // 1.20.5: the reversal at half the speed, without asking Entity.hurt
    let hurt_too = ProjectileTarget { hurt: true, ..breeze };
    let (state, result, report) = shot(&[hurt_too], &world, R1_20_5);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Deflected);
    assert!(!result.discarded);
    assert_eq!(state.position.x, -1.5);
    // once by one entity from 1.21; every time in 1.20.5
    let again = |release| {
        let mut hits = ProjectileHitState { last_deflected_by: Some(1), ..aged() };
        let mut state = arrow();
        tick(&mut state, &mut hits, &[breeze], &world, release);
        state.velocity.x
    };
    assert!(again(R1_20_5) < 0.0);
    assert!(again(R1_21) > 0.0);
}

#[test]
fn an_unharmable_player_drops_the_hit_until_1_21_1_and_is_no_candidate_from_1_21_2() {
    let world = full_block((2, 1, 0));
    let ally = ProjectileTarget { friendly: true, ..target(1, 0.75) };
    let (_, result, report) = shot(&[ally], &world, R1_21_1);
    assert!(report.entity_hits.is_empty());
    assert!(report.block_hit.is_none(), "the block goes with it");
    assert!(!result.in_ground);
    let (_, result, report) = shot(&[ally], &world, R1_21_2);
    assert!(report.entity_hits.is_empty());
    assert!(report.block_hit.is_some());
    assert!(result.in_ground);
    // 1.8 treats a creative player so; later Entity.hurt refuses it
    let creative = ProjectileTarget { invulnerable_player: true, ..target(1, 1.0) };
    assert!(shot(&[creative], &WorldSnapshot::new(), R1_8).2.entity_hits.is_empty());
    assert_eq!(ids(&shot(&[creative], &WorldSnapshot::new(), R1_12).2), [1]);
}

#[test]
fn an_arrow_flies_on_past_an_enderman() {
    let world = WorldSnapshot::new();
    let teleports = ProjectileTarget { enderman: true, ..target(1, 1.0) };
    let (_, result, report) = shot(&[teleports], &world, R26_2);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Evaded);
    assert!(!result.discarded);
    // 26.3: Enderman.hurt says false, and that turns nothing back; a true is a hit
    let refuses = ProjectileTarget { hurt: false, ..teleports };
    let (state, result, report) = shot(&[refuses], &world, R26_3);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Evaded);
    assert!(!result.discarded && state.velocity.x > 0.0);
    assert!(shot(&[teleports], &world, R26_3).1.discarded);
    // an enderman with nowhere to go refuses it before 26.3
    let (state, _, report) = shot(&[refuses], &world, R26_2);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Refused);
    assert!(state.velocity.x < 0.0);
}

#[test]
fn from_1_21_11_an_arrow_takes_every_target_of_the_step_ordered_by_position() {
    let world = WorldSnapshot::new();
    // the first box begins nearer, the second entity's position is nearer
    let mut first = target(1, 0.6);
    first.position.x = 2.0;
    let second = target(2, 1.0);
    assert_eq!(ids(&shot(&[first, second], &world, R1_21_10).2), [1], "the nearest on the ray");
    let mut pierced = ProjectileHitState { pierce_level: 3, ..aged() };
    let (_, report) = tick(&mut arrow(), &mut pierced, &[first, second], &world, R1_21_11);
    assert_eq!(ids(&report), [2, 1], "by entity.position");
    assert_eq!(pierced.pierced, [2, 1]);
}

#[test]
fn an_inflated_hit_needs_a_clear_way_to_the_centre_from_1_21_11() {
    // the ray passes 0.2 above the box, inside its margin
    let mut below = target(1, 1.0);
    below.bounding_box = target_hitbox(below.position, 1.0, 0.8, 1.0);
    let clear = WorldSnapshot::new();
    let (_, _, surface) = shot(&[below], &clear, R26_2);
    assert_eq!(ids(&surface), [1]);
    assert_eq!(surface.entity_hits[0].location.x, 1.0, "on the box");
    let (_, _, outside) = shot(&[below], &clear, R26_3);
    assert_eq!(outside.entity_hits[0].location.y, 1.0, "on the ray");
    assert_eq!(outside.entity_hits[0].location.x, 1.0 - f64::from(0.3_f32));
    // a thin plate between the ray and the box's centre shields it, as no older release does
    let mut world = WorldSnapshot::new();
    world.set_block(
        (0, 0, 0),
        BlockState {
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::new(0.8, 0.0, 0.0), Vec3::new(0.85, 0.99, 1.0)),
                ..BlockShape::full_block()
            }),
            ..Default::default()
        },
    );
    assert!(shot(&[below], &world, R1_21_11).2.entity_hits.is_empty());
    assert_eq!(ids(&shot(&[below], &world, R1_21_10).2), [1]);
}

fn trident() -> TridentState {
    TridentState::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0))
}

fn throw(
    release: MinecraftVersion,
    targets: &[ProjectileTarget],
) -> (TridentState, ProjectileHitReport) {
    let mut state = trident();
    let world = WorldSnapshot::new();
    let (result, report) =
        tick_trident_with_targets(&mut state, &mut aged(), targets, None, &world, release);
    assert!(!result.discarded);
    (state, report)
}

#[test]
fn a_trident_deals_its_damage_once_and_turns_back_by_release() {
    let (state, report) = throw(R1_13, &[target(1, 1.0)]);
    assert!(state.dealt_damage);
    assert_eq!(report.entity_hits[0].damage, None);
    assert_eq!(state.position.x, 3.0 * f64::from(-0.01_f32));
    assert_eq!(throw(R1_14, &[target(1, 1.0)]).0.position.x, 3.0 * -0.01);
    assert_eq!(
        throw(R1_21_2, &[target(1, 1.0)]).0.velocity.x,
        3.0 * -0.5 * 0.02 * f64::from(0.99_f32)
    );
    assert_eq!(
        throw(R26_3, &[target(1, 1.0)]).0.velocity.x,
        3.0 * (-0.02 * 0.5) * f64::from(0.99_f32)
    );
    // a refusal turns it back as well, and it stays dealt
    let (state, report) = throw(R1_16, &[ProjectileTarget { hurt: false, ..target(1, 1.0) }]);
    assert!(state.dealt_damage);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Refused);
    // a dealt trident passes through
    let mut state = TridentState { dealt_damage: true, ..trident() };
    let world = WorldSnapshot::new();
    let (_, report) =
        tick_trident_with_targets(&mut state, &mut aged(), &[target(1, 1.0)], None, &world, R1_16);
    assert!(report.entity_hits.is_empty());
}

#[test]
fn a_trident_keeps_its_motion_at_an_enderman_from_1_15() {
    let enderman = ProjectileTarget { enderman: true, ..target(1, 1.0) };
    assert!(throw(R1_14, &[enderman]).0.velocity.x < 0.0);
    assert!(throw(R1_15, &[enderman]).0.velocity.x > 0.0);
    let refused = ProjectileTarget { hurt: false, ..enderman };
    assert!(throw(R26_2, &[refused]).0.velocity.x < 0.0);
    assert!(throw(R26_3, &[refused]).0.velocity.x > 0.0);
    assert!(throw(R26_3, &[refused]).0.dealt_damage);
}

#[test]
fn a_trident_takes_only_the_nearest_where_an_arrow_takes_all() {
    let (_, report) = throw(R26_3, &[target(1, 1.0), target(2, 2.0)]);
    assert_eq!(ids(&report), [1]);
}

fn returning() -> (TridentState, ProjectileHitState) {
    let state = TridentState {
        dealt_damage: true,
        ..TridentState::new(Vec3::new(10.0, 1.0, 0.0), Vec3::ZERO)
    };
    (state, ProjectileHitState { loyalty: 2, ..aged() })
}

#[test]
fn loyalty_returns_to_an_available_owner() {
    let world = WorldSnapshot::new();
    let (mut state, mut hits) = returning();
    let result =
        tick_trident_with_targets(&mut state, &mut hits, &[], Some(owner(1)), &world, R1_16).0;
    assert!(!result.discarded);
    assert!(state.no_physics);
    let to_eye = Vec3::new(-10.0, 0.62, 0.0);
    let pull = to_eye.normalized(R1_16).scale(0.05 * 2.0);
    assert_eq!(state.position.x, 10.0 + pull.x);
    assert!((state.position.y - (1.0 + 0.62 * 0.015 * 2.0 + pull.y)).abs() < 1.0e-12);
    // once returning it needs no dealt damage to go on
    let mut next = TridentState { dealt_damage: false, ..state };
    tick_trident_with_targets(&mut next, &mut hits, &[], Some(owner(1)), &world, R1_16);
    assert!(next.position.x < state.position.x);
}

#[test]
fn the_pull_is_axis_by_axis_in_1_13() {
    let world = WorldSnapshot::new();
    let (mut state, mut hits) = returning();
    state.velocity = Vec3::new(0.3, 0.0, 0.0);
    tick_trident_with_targets(&mut state, &mut hits, &[], Some(owner(1)), &world, R1_13);
    let pull = Vec3::new(-10.0, 0.62, 0.0).normalized(R1_13).x * (0.05 * 2.0);
    assert_eq!(state.position.x, 10.0 + (0.3 + (pull - 0.3 * 0.05)));
}

#[test]
fn loyalty_stays_without_an_owner_and_is_lost_with_a_dead_or_spectating_one() {
    let world = WorldSnapshot::new();
    let (mut state, mut hits) = returning();
    let result = tick_trident_with_targets(&mut state, &mut hits, &[], None, &world, R26_3).0;
    assert!(!result.discarded);
    assert!(!state.no_physics, "getOwner is null");
    for gone in [
        ProjectileOwner { alive: false, ..owner(1) },
        ProjectileOwner { spectator: true, ..owner(1) },
    ] {
        let (mut state, mut hits) = returning();
        let result =
            tick_trident_with_targets(&mut state, &mut hits, &[], Some(gone), &world, R26_3).0;
        assert!(result.discarded);
    }
    // without loyalty nothing asks
    let (mut state, _) = returning();
    let dead = ProjectileOwner { alive: false, ..owner(1) };
    let result =
        tick_trident_with_targets(&mut state, &mut aged(), &[], Some(dead), &world, R26_3).0;
    assert!(!result.discarded);
}

#[test]
fn a_mob_takes_its_trident_back_near_it_from_1_21_2() {
    let world = WorldSnapshot::new();
    let drowned = ProjectileOwner {
        player: false,
        eye_position: Vec3::new(10.0, 2.0, 0.0),
        ..owner(1)
    };
    let reach = |release| {
        let (mut state, mut hits) = returning();
        tick_trident_with_targets(&mut state, &mut hits, &[], Some(drowned), &world, release)
            .0
            .discarded
    };
    assert!(!reach(R1_21_1));
    assert!(reach(R1_21_2));
}

#[test]
fn a_loyal_trident_that_can_be_picked_up_never_ages() {
    let block = (0, -1, 0);
    let world = full_block(block);
    let stuck = TridentState {
        in_ground: true,
        life: 1199,
        last_block_id: Some(world.block(block).id),
        last_block_position: Some(block),
        ..TridentState::new(Vec3::new(0.5, -0.5, 0.5), Vec3::ZERO)
    };
    let mut state = stuck;
    let mut hits = ProjectileHitState { loyalty: 1, pickup_allowed: true, ..Default::default() };
    let result = tick_trident_with_targets(&mut state, &mut hits, &[], None, &world, R1_16).0;
    assert!(!result.discarded);
    assert_eq!(state.life, 1199);
    let mut state = stuck;
    let mut hits = ProjectileHitState { loyalty: 1, ..Default::default() };
    let result = tick_trident_with_targets(&mut state, &mut hits, &[], None, &world, R1_16).0;
    assert!(result.discarded);
}

#[test]
fn state_persists_across_ticks() {
    let world = WorldSnapshot::new();
    let targets = [target(1, 1.0), target(2, 4.0)];
    let mut state = arrow();
    let mut hits = ProjectileHitState { pierce_level: 3, left_owner: true, ..Default::default() };
    let (_, first) = tick(&mut state, &mut hits, &targets, &world, R1_21_5);
    let (_, second) = tick(&mut state, &mut hits, &targets, &world, R1_21_5);
    assert_eq!(ids(&first), [1]);
    assert_eq!(ids(&second), [2]);
    assert_eq!(hits.pierced, [1, 2]);
    assert_eq!(hits.tick_count, 2);
}

#[test]
fn an_arrow_without_targets_is_the_plain_arrow() {
    let world = full_block((5, 1, 0));
    for release in EVERY {
        let (mut plain, mut scene) = (arrow(), arrow());
        let mut hits = ProjectileHitState::default();
        for _ in 0..4 {
            let a = tick_arrow_in_world(&mut plain, &world, release);
            let (b, _) = tick(&mut scene, &mut hits, &[], &world, release);
            assert_eq!(a, b);
        }
    }
}

#[test]
fn gravity_clears_a_negative_zero_from_1_20_5() {
    let world = WorldSnapshot::new();
    let wall = ProjectileTarget { hurt: false, ..target(1, 1.0) };
    assert!(shot(&[wall], &world, R1_20_4).0.velocity.z.is_sign_negative());
    assert!(shot(&[wall], &world, R1_20_5).0.velocity.z.is_sign_positive());
    assert!(shot(&[wall], &world, R26_3).0.velocity.z.is_sign_positive());
}

#[test]
fn a_fall_from_the_ground_starts_the_shooter_s_ticks_again() {
    let world = WorldSnapshot::new();
    let mut state = ArrowState {
        in_ground: true,
        last_block_id: Some(1),
        last_block_position: Some((0, 0, 0)),
        ..arrow()
    };
    let mut hits = aged();
    let (result, _) = tick(&mut state, &mut hits, &[], &world, R1_15);
    assert!(result.started_falling);
    assert_eq!(hits.flying_ticks, 0);
}

#[test]
fn the_water_s_inertia_comes_after_a_refusal_before_1_21_4() {
    let world = WorldSnapshot::new();
    let wall = ProjectileTarget { hurt: false, ..target(1, 1.0) };
    let refuse = |release| {
        let mut state = ArrowState { in_water: true, ..arrow() };
        state.velocity = Vec3::new(0.7, 0.0, 0.0);
        let wide = ProjectileTarget { bounding_box: wall.bounding_box.translated(Vec3::new(-0.5, 0.0, 0.0)), ..wall };
        tick_arrow_with_targets(&mut state, &mut aged(), &[wide], None, &world, release);
        state.velocity.x
    };
    let inertia = f64::from(0.6_f32);
    assert_eq!(refuse(R1_21_2), 0.7 * -0.5 * 0.2 * inertia);
    assert_eq!(refuse(v(1, 21, 4)), 0.7 * inertia * -0.5 * 0.2);
}

#[test]
fn an_enderman_keeps_an_arrow_flying_on_the_oldest_releases_too() {
    let world = WorldSnapshot::new();
    let teleports = ProjectileTarget { enderman: true, ..target(1, 1.0) };
    for release in [R1_8, R1_12, R1_16] {
        let (state, result, report) = shot(&[teleports], &world, release);
        assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Evaded);
        assert!(!result.discarded);
        assert_eq!(state.position.x, 3.0);
    }
}

#[test]
fn an_arrows_damage_follows_the_release() {
    let (old, between, new) = (
        MinecraftVersion::new(1, 13, 2),
        MinecraftVersion::new(1, 15, 2),
        MinecraftVersion::new(1, 16, 0),
    );
    for release in [old, between, new, MinecraftVersion::new(26, 3, 0)] {
        assert_eq!(crate::projectile_hits::arrow_damage(3.0, 2.0, release), 6);
        assert_eq!(crate::projectile_hits::arrow_damage(1.1, 2.0, release), 3);
    }
    // a negative damage: only Math.max and the clamp hold it at 0
    assert_eq!(crate::projectile_hits::arrow_damage(3.0, -2.0, old), -6);
    assert_eq!(crate::projectile_hits::arrow_damage(3.0, -2.0, between), 0);
    assert_eq!(crate::projectile_hits::arrow_damage(3.0, -2.0, new), 0);
    // an enormous one: the old ceil wraps unless the clamp stops it first
    assert_eq!(crate::projectile_hits::arrow_damage(3.0, 1.0e10, old), i32::MIN);
    assert_eq!(crate::projectile_hits::arrow_damage(3.0, 1.0e10, between), i32::MIN);
    assert_eq!(crate::projectile_hits::arrow_damage(3.0, 1.0e10, new), i32::MAX);
    assert_eq!(
        crate::projectile_hits::arrow_damage(3.0, 1.0e10, MinecraftVersion::new(26, 3, 0)),
        i32::MAX
    );
}

// ---- the other families ----

const R1_9: MinecraftVersion = v(1, 9, 4);
const R1_11: MinecraftVersion = v(1, 11, 2);
const R1_16_1: MinecraftVersion = v(1, 16, 1);
const R1_16_2: MinecraftVersion = v(1, 16, 2);

fn flying(kind: ProjectileKind) -> ProjectileState {
    let mut state = ProjectileState::for_kind(kind, Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
    state.first_tick = false;
    state.gravity = 0.0;
    state
}

fn tick_family(
    kind: ProjectileKind,
    hits: &mut ProjectileHitState,
    targets: &[ProjectileTarget],
    owner: Option<ProjectileOwner>,
    release: MinecraftVersion,
) -> (ProjectileState, ProjectileTickResult, ProjectileHitReport) {
    let mut state = flying(kind);
    let world = WorldSnapshot::new();
    let (result, report) = tick_projectile_with_targets(&mut state, hits, targets, owner, &world, release);
    (state, result, report)
}

fn player_owner(id: i32) -> ProjectileOwner {
    ProjectileOwner {
        id,
        root_vehicle: id,
        eye_position: Vec3::new(0.0, 1.62, 0.0),
        width: 0.6,
        alive: true,
        player: true,
        spectator: false,
        removed: false,
        won_game: false,
    }
}

const GONE_AT_A_HIT: [ProjectileKind; 10] = [
    ProjectileKind::Snowball,
    ProjectileKind::Egg,
    ProjectileKind::EnderPearl,
    ProjectileKind::Potion,
    ProjectileKind::ExperienceBottle,
    ProjectileKind::Fireball,
    ProjectileKind::SmallFireball,
    ProjectileKind::WitherSkull,
    ProjectileKind::DragonFireball,
    ProjectileKind::ShulkerBullet,
];

#[test]
fn every_family_is_gone_at_its_target_in_every_release() {
    for release in EVERY {
        for kind in GONE_AT_A_HIT {
            let mut hits = aged();
            let (_, result, report) = tick_family(kind, &mut hits, &[target(7, 2.0)], None, release);
            assert!(result.discarded, "{kind:?} {release:?}");
            assert_eq!(report.entity_hits[0].target, 7);
            assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hurt);
        }
    }
}

#[test]
fn a_family_misses_past_the_margin_and_takes_the_nearest() {
    // half a block wide, 0.25 of the projectile is not on the ray: the margin alone reaches
    let at = |z: f64| {
        let mut target = target(1, 2.0);
        target.bounding_box = target.bounding_box.translated(Vec3::new(0.0, 0.0, z));
        target
    };
    let (_, result, report) = tick_family(ProjectileKind::Snowball, &mut aged(), &[at(0.81)], None, R1_20_4);
    assert!(!result.discarded && report.entity_hits.is_empty());
    assert!(tick_family(ProjectileKind::Snowball, &mut aged(), &[at(0.79)], None, R1_20_4).1.discarded);
    let (_, _, report) = tick_family(ProjectileKind::Egg, &mut aged(), &[target(1, 2.5), target(2, 1.0)], None, R26_3);
    assert_eq!(report.entity_hits[0].target, 2);
}

#[test]
fn the_new_query_margin_opens_with_age_for_every_family() {
    let mut near = target(1, 2.0);
    near.bounding_box = near.bounding_box.translated(Vec3::new(0.0, 0.0, 0.6));
    let young = ProjectileHitState { tick_count: 1, ..aged() };
    let (_, result, _) = tick_family(ProjectileKind::Snowball, &mut young.clone(), &[near], None, R1_21_6);
    assert!(!result.discarded, "no margin on the first ticks from 1.21.6");
    let (_, result, _) = tick_family(ProjectileKind::Snowball, &mut young.clone(), &[near], None, R1_21_5);
    assert!(result.discarded, "0.3 at once until 1.21.5");
}

#[test]
fn a_thrown_item_stops_at_the_hit_from_1_21_2() {
    let (state, ..) = tick_family(ProjectileKind::Snowball, &mut aged(), &[target(1, 2.0)], None, R1_21_2);
    assert!(state.position.x < 2.0, "{:?}", state.position);
    let (state, ..) = tick_family(ProjectileKind::Snowball, &mut aged(), &[target(1, 2.0)], None, R1_21_1);
    assert!(state.position.x > 2.5, "{:?}", state.position);
}

#[test]
fn the_thrower_is_safe_for_five_ticks_on_1_8_and_a_fireballs_for_25() {
    let me = target(5, 1.0);
    let owner = Some(player_owner(5));
    let at = |flying_ticks| ProjectileHitState { flying_ticks, ..aged() };
    assert!(!tick_family(ProjectileKind::Snowball, &mut at(3), &[me], owner, R1_8).1.discarded);
    assert!(tick_family(ProjectileKind::Snowball, &mut at(4), &[me], owner, R1_8).1.discarded);
    assert!(!tick_family(ProjectileKind::Fireball, &mut at(23), &[me], owner, R1_15).1.discarded);
    assert!(tick_family(ProjectileKind::Fireball, &mut at(24), &[me], owner, R1_15).1.discarded);
}

#[test]
fn a_thrown_item_ignores_what_it_starts_by_from_1_9() {
    let neighbour = target(4, 1.0);
    let first = || ProjectileHitState { tick_count: 0, ..aged() };
    let thrower = Some(player_owner(9));
    // the first tick takes whatever is in the box, thrown by someone from 1.11
    let mut hits = first();
    assert!(!tick_family(ProjectileKind::Snowball, &mut hits, &[neighbour], thrower, R1_12).1.discarded);
    assert_eq!(hits.ignored, Some(4));
    assert!(tick_family(ProjectileKind::Snowball, &mut first(), &[neighbour], None, R1_12).1.discarded);
    assert!(!tick_family(ProjectileKind::Snowball, &mut first(), &[neighbour], None, R1_9).1.discarded);
    // 1.14 and 1.15 count down apart from the box
    let mut hits = first();
    assert!(!tick_family(ProjectileKind::Snowball, &mut hits, &[neighbour], thrower, R1_14).1.discarded);
    assert_eq!((hits.ignored, hits.ignore_time), (Some(4), 2));
    // none from 1.16
    assert!(tick_family(ProjectileKind::Snowball, &mut first(), &[neighbour], thrower, R1_16).1.discarded);
}

#[test]
fn an_ignored_entity_returns_after_the_item_clears_it() {
    let mut hits = ProjectileHitState { ignored: Some(4), ignore_time: 0, ..aged() };
    let _ = tick_family(ProjectileKind::Snowball, &mut hits, &[], None, R1_12);
    assert_eq!(hits.ignored, None);
    let mut hits = ProjectileHitState { ignored: Some(4), ignore_time: 1, ..aged() };
    let _ = tick_family(ProjectileKind::Snowball, &mut hits, &[], None, R1_12);
    assert_eq!(hits.ignored, Some(4));
}

#[test]
fn a_pearl_passes_its_thrower_until_1_15() {
    let me = target(5, 1.0);
    let owner = Some(player_owner(5));
    let late = || ProjectileHitState { flying_ticks: 30, left_owner: true, ..aged() };
    let (_, result, report) = tick_family(ProjectileKind::EnderPearl, &mut late(), &[me], owner, R1_8);
    assert!(!result.discarded);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Passed);
    assert!(tick_family(ProjectileKind::EnderPearl, &mut late(), &[me], owner, R1_16).1.discarded);
}

#[test]
fn a_pearl_goes_with_its_dead_player() {
    let dead = Some(ProjectileOwner { alive: false, ..player_owner(5) });
    assert!(tick_family(ProjectileKind::EnderPearl, &mut aged(), &[], dead, R26_3).1.discarded);
    let mut rule = ProjectileHitState { pearl_outlives_owner: true, ..aged() };
    assert!(!tick_family(ProjectileKind::EnderPearl, &mut rule, &[], dead, R26_3).1.discarded);
}

#[test]
fn a_pearl_stays_only_by_a_rule_of_its_release() {
    // 1.20.1 ThrownEnderpearl.java:90-92 discards with no rule; 1.20.2 :90 has the rule
    let dead = Some(ProjectileOwner { alive: false, ..player_owner(5) });
    let lost = |release| {
        let mut rule = ProjectileHitState { pearl_outlives_owner: true, ..aged() };
        tick_family(ProjectileKind::EnderPearl, &mut rule, &[], dead, release).1.discarded
    };
    assert!(lost(v(1, 20, 1)));
    assert!(!lost(R1_20_2));
}

#[test]
fn a_pearl_waits_out_the_credits_from_1_21_9() {
    // showEndCredits removes the player and sets wonGame (26.3 ServerPlayer.java:1135-1143), which
    // ThrownEnderpearl.tick reads from 1.21.9 (26.3 ThrownEnderpearl.java:177; 1.21.8 has none)
    let credits = Some(ProjectileOwner { alive: false, won_game: true, ..player_owner(5) });
    let lost = |release| tick_family(ProjectileKind::EnderPearl, &mut aged(), &[], credits, release).1.discarded;
    assert!(lost(R1_21_8));
    assert!(!lost(R1_21_9));
    assert!(!lost(R26_3));
    let alive = Some(ProjectileOwner { won_game: true, ..player_owner(5) });
    assert!(!tick_family(ProjectileKind::EnderPearl, &mut aged(), &[], alive, R1_21_8).1.discarded);
}

#[test]
fn a_dragon_fireball_does_not_burst_on_its_owner() {
    let me = target(5, 1.0);
    let late = || ProjectileHitState { flying_ticks: 30, left_owner: true, ..aged() };
    let owner = Some(ProjectileOwner { player: false, ..player_owner(5) });
    assert!(!tick_family(ProjectileKind::DragonFireball, &mut late(), &[me], owner, R26_3).1.discarded);
    assert!(tick_family(ProjectileKind::Fireball, &mut late(), &[me], owner, R26_3).1.discarded);
}

#[test]
fn a_fireball_goes_with_its_removed_owner_or_chunk() {
    let removed = Some(ProjectileOwner { removed: true, ..player_owner(5) });
    for release in [R1_8, R1_21_2, R26_3] {
        assert!(tick_family(ProjectileKind::SmallFireball, &mut aged(), &[], removed, release).1.discarded);
        let mut hits = ProjectileHitState { chunk_unloaded: true, ..aged() };
        assert!(tick_family(ProjectileKind::SmallFireball, &mut hits, &[], None, release).1.discarded);
    }
}

#[test]
fn fireballs_and_bullets_pass_no_physics_targets_from_1_9() {
    let ghost = ProjectileTarget { no_physics: true, ..target(1, 2.0) };
    for kind in [ProjectileKind::Fireball, ProjectileKind::ShulkerBullet] {
        assert!(!tick_family(kind, &mut aged(), &[ghost], None, R26_3).1.discarded);
    }
    assert!(tick_family(ProjectileKind::Fireball, &mut aged(), &[ghost], None, R1_8).1.discarded);
    assert!(tick_family(ProjectileKind::Snowball, &mut aged(), &[ghost], None, R26_3).1.discarded);
}

#[test]
fn no_family_takes_a_spectator_in_any_release() {
    // getEntities(this, box) is NO_SPECTATORS before 1.14 (1.8.8 Level.java:2173-2175, 1.13.2
    // EntityGetter.java:18-20); each canHitEntity says !isSpectator() from 1.14
    let spectator = ProjectileTarget { spectator: true, ..target(1, 2.0) };
    for kind in [ProjectileKind::Snowball, ProjectileKind::Fireball, ProjectileKind::LlamaSpit, ProjectileKind::ShulkerBullet] {
        for release in [R1_8, R1_12, R1_13, R1_14, R26_3] {
            if kind == ProjectileKind::LlamaSpit && release == R1_8 {
                continue;
            }
            let (_, result, report) = tick_family(kind, &mut aged(), &[spectator], None, release);
            assert!(!result.discarded && report.entity_hits.is_empty(), "{kind:?} {release}");
        }
        // the same target, no spectator, is taken
        let seen = ProjectileTarget { spectator: false, ..spectator };
        assert!(!tick_family(kind, &mut aged(), &[seen], None, R1_13).2.entity_hits.is_empty(), "{kind:?}");
    }
    let world = WorldSnapshot::new();
    assert!(shot(&[spectator], &world, R1_8).2.entity_hits.is_empty());
}

#[test]
fn wind_charges_pass_each_other_and_end_crystals() {
    let wind = ProjectileTarget { wind_charge: true, ..target(1, 2.0) };
    let crystal = ProjectileTarget { end_crystal: true, ..target(2, 2.0) };
    assert!(!tick_family(ProjectileKind::WindCharge, &mut aged(), &[wind], None, R26_3).1.discarded);
    assert!(!tick_family(ProjectileKind::WindCharge, &mut aged(), &[crystal], None, R26_3).1.discarded);
    assert!(tick_family(ProjectileKind::WindCharge, &mut aged(), &[crystal], None, R1_20_4).1.discarded);
    assert!(tick_family(ProjectileKind::Fireball, &mut aged(), &[crystal], None, R26_3).1.discarded);
}

#[test]
fn a_breeze_turns_all_but_wind_charges_back() {
    let breeze = ProjectileTarget { deflects: true, ..target(1, 2.0) };
    let (state, result, report) = tick_family(ProjectileKind::Snowball, &mut aged(), &[breeze], None, R26_3);
    assert!(!result.discarded);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Deflected);
    assert!(state.velocity.x < 0.0);
    assert!(tick_family(ProjectileKind::WindCharge, &mut aged(), &[breeze], None, R26_3).1.discarded);
    assert!(tick_family(ProjectileKind::Snowball, &mut aged(), &[breeze], None, R1_20_4).1.discarded);
}

#[test]
fn a_deflection_halves_a_fireballs_push_by_release() {
    let breeze = ProjectileTarget { deflects: true, ..target(1, 2.0) };
    let (state, ..) = tick_family(ProjectileKind::Fireball, &mut aged(), &[breeze], None, R26_3);
    assert_eq!(state.acceleration_power, 0.05);
    let (state, ..) = tick_family(ProjectileKind::Fireball, &mut aged(), &[breeze], None, R1_20_5);
    assert_ne!(state.power, Vec3::ZERO);
    let mut hits = ProjectileHitState { last_deflected_by: Some(1), ..aged() };
    let (state, ..) = tick_family(ProjectileKind::Fireball, &mut hits, &[breeze], None, R26_3);
    assert!(state.velocity.x > 0.0, "one entity deflects it once");
}

#[test]
fn llama_spit_is_gone_at_a_hit_until_1_13_and_flies_on_from_1_14() {
    let owner = Some(ProjectileOwner { player: false, ..player_owner(5) });
    let (_, result, report) = tick_family(ProjectileKind::LlamaSpit, &mut aged(), &[target(1, 2.0)], owner, R1_13);
    assert!(result.discarded);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hurt);
    let (_, result, report) = tick_family(ProjectileKind::LlamaSpit, &mut aged(), &[target(1, 2.0)], owner, R1_14);
    assert!(!result.discarded);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hurt);
    let (_, _, report) = tick_family(ProjectileKind::LlamaSpit, &mut aged(), &[target(1, 2.0)], None, R26_3);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Passed);
}

#[test]
fn llama_spit_took_the_uncollidable_until_1_15() {
    let item = ProjectileTarget { pickable: false, ..target(1, 2.0) };
    assert!(!tick_family(ProjectileKind::LlamaSpit, &mut aged(), &[item], None, R1_11).2.entity_hits.is_empty());
    assert!(tick_family(ProjectileKind::LlamaSpit, &mut aged(), &[item], None, R1_16).2.entity_hits.is_empty());
}

#[test]
fn a_deflected_spit_keeps_its_motion() {
    let breeze = ProjectileTarget { deflects: true, ..target(1, 2.0) };
    let (state, ..) = tick_family(ProjectileKind::LlamaSpit, &mut aged(), &[breeze], None, R26_3);
    assert_eq!(state.velocity.x, 3.0 * f64::from(0.99_f32));
}

#[test]
fn a_rocket_hits_from_1_14() {
    let rocket = |release, x| {
        let mut state = flying(ProjectileKind::Firework);
        state.shot_at_angle = true;
        state.position.x = x;
        let world = WorldSnapshot::new();
        tick_projectile_with_targets(&mut state, &mut aged(), &[target(1, 2.0)], None, &world, release).0
    };
    assert!(!rocket(R1_13, -2.0).discarded);
    assert!(rocket(R1_14, -2.0).discarded);
    assert!(rocket(R1_21_2, 0.0).discarded);
}

#[test]
fn a_rocket_looks_after_its_move_until_1_21_1_and_before_it_from_1_21_2() {
    // three blocks a tick from x -3.5: the target is out of the first look, in the second
    let rocket = |release| {
        let mut state = flying(ProjectileKind::Firework);
        state.shot_at_angle = true;
        state.position.x = -3.5;
        let world = WorldSnapshot::new();
        tick_projectile_with_targets(&mut state, &mut aged(), &[target(1, 2.0)], None, &world, release).0.discarded
    };
    assert!(rocket(R1_21_1));
    assert!(!rocket(R1_21_2));
}

#[test]
fn a_rocket_bursts_past_its_lifetime_and_on_a_block_only_with_explosions() {
    let mut hits = ProjectileHitState { life: 10, life_time: Some(10), ..aged() };
    assert!(tick_family(ProjectileKind::Firework, &mut hits, &[], None, R1_8).1.discarded);
    assert_eq!(hits.life, 11);
    let mut hits = ProjectileHitState { life: 9, life_time: Some(10), ..aged() };
    assert!(!tick_family(ProjectileKind::Firework, &mut hits, &[], None, R26_3).1.discarded);
    let world = full_block((2, 1, 0));
    let wall = |explodes| {
        let mut state = flying(ProjectileKind::Firework);
        state.shot_at_angle = true;
        let mut hits = ProjectileHitState { explodes, ..aged() };
        tick_projectile_with_targets(&mut state, &mut hits, &[], None, &world, R26_3).0.discarded
    };
    assert!(wall(true));
    assert!(!wall(false));
}

#[test]
fn an_attached_rocket_follows_its_holder_or_stays_and_never_flies() {
    // 26.3 FireworkRocketEntity.java:108-138: the free rocket's boost and move are the other branch
    let world = WorldSnapshot::new();
    let holder = RocketHolder {
        position: Vec3::new(10.0, 70.0, 20.0),
        velocity: Vec3::new(0.0, -0.1, 1.5),
        gliding: true,
        player: true,
        yaw_degrees: 0.0,
        left_hand: false,
    };
    let tick = |hits: &mut ProjectileHitState, release| {
        let mut state = flying(ProjectileKind::Firework);
        let start = state.position;
        let result = tick_projectile_with_targets(&mut state, hits, &[], None, &world, release).0;
        (state, start, result)
    };
    // at the hand of a gliding player: calculateViewVector(0, 0 + 80) * 0.5 (the recording's rocket has x 0.0076 by a player's 0.5)
    let mut hits = ProjectileHitState { attached: true, holder: Some(holder), ..aged() };
    let (state, _, result) = tick(&mut hits, R26_3);
    assert!((state.position.x - (10.0 - 0.4924)).abs() < 1.0e-3);
    assert_eq!(state.position.y, 70.0);
    assert!((state.position.z - (20.0 + 0.0868)).abs() < 1.0e-3);
    assert_eq!(state.velocity, holder.velocity);
    assert!(!result.discarded);
    // no hand before 1.18.2: 1.16.5 FireworkRocketEntity.java:155
    let mut hits = ProjectileHitState { attached: true, holder: Some(holder), ..aged() };
    assert_eq!(tick(&mut hits, R1_16).0.position, holder.position);
    // a holder that stopped gliding has the rocket at its feet, and a mob always
    let walking = RocketHolder { gliding: false, ..holder };
    let mut hits = ProjectileHitState { attached: true, holder: Some(walking), ..aged() };
    assert_eq!(tick(&mut hits, R26_3).0.position, holder.position);
    let mob = RocketHolder { player: false, ..holder };
    let mut hits = ProjectileHitState { attached: true, holder: Some(mob), ..aged() };
    assert_eq!(tick(&mut hits, R26_3).0.position, holder.position);
    // an id nothing answers to, or a nonliving entity: it stays, and burns down all the same
    let mut hits = ProjectileHitState { attached: true, life: 5, life_time: Some(5), ..aged() };
    let (state, start, result) = tick(&mut hits, R26_3);
    assert_eq!(state.position, start);
    assert_eq!(state.velocity, flying(ProjectileKind::Firework).velocity);
    assert_eq!(hits.life, 6);
    assert!(result.discarded);
    // no attachment before 1.11: 1.10.2 has the free rocket alone
    let mut hits = ProjectileHitState { attached: true, ..aged() };
    let (state, start, _) = tick(&mut hits, v(1, 10, 2));
    assert_ne!(state.position, start);
}

#[test]
fn a_hook_hooks_stops_then_rides_its_target() {
    let owner = Some(player_owner(9));
    let world = WorldSnapshot::new();
    let villager = target(1, 2.0);
    let mut hits = aged();
    let mut state = flying(ProjectileKind::FishingHook);
    let (_, report) = tick_projectile_with_targets(&mut state, &mut hits, &[villager], owner, &world, R26_3);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hooked);
    assert_eq!(hits.hooked, Some(1));
    let (result, _) = tick_projectile_with_targets(&mut state, &mut hits, &[villager], owner, &world, R26_3);
    assert_eq!(result.velocity, Vec3::ZERO);
    assert!(hits.hook_settled);
    let (result, _) = tick_projectile_with_targets(&mut state, &mut hits, &[villager], owner, &world, R26_3);
    assert_eq!(result.position, Vec3::new(villager.position.x, 2.0 * 0.8, 0.0));
    tick_projectile_with_targets(&mut state, &mut hits, &[], owner, &world, R26_3);
    assert_eq!((hits.hooked, hits.hook_settled), (None, false));
}

#[test]
fn a_riding_hooks_box_goes_with_it_from_1_11_until_the_box_is_no_kept_one() {
    // setPos remakes the box: 1.11.2 FishingHook.java:199, 1.13.2 :207 (Entity.java:419-426), 1.14.4 :176,
    // 1.15 :193, 1.16.5 :197; from 1.17 no move reads a kept box
    let box_after_a_ride = |release| {
        let owner = Some(player_owner(9));
        let world = WorldSnapshot::new();
        let villager = target(1, 2.0);
        let mut hits = aged();
        let mut state = flying(ProjectileKind::FishingHook);
        let kept = Aabb::new(Vec3::new(-0.125, 1.0, -0.125), Vec3::new(0.125, 1.25, 0.125));
        state.bounding_box = Some(kept);
        hits.hooked = Some(1);
        hits.hook_settled = true;
        let (result, _) = tick_projectile_with_targets(&mut state, &mut hits, &[villager], owner, &world, release);
        assert_eq!(result.position, Vec3::new(villager.position.x, 2.0 * 0.8, 0.0), "{release:?}");
        (state.bounding_box, kept, ProjectileKind::FishingHook.bounding_box(state.position))
    };
    for release in [v(1, 11, 2), R1_13, R1_14, R1_15, R1_16] {
        let (kept, _, remade) = box_after_a_ride(release);
        assert_eq!(kept, Some(remade), "{release:?}");
    }
    let (kept, before, _) = box_after_a_ride(R26_3);
    assert_eq!(kept, Some(before), "from 1.17 no move reads it");
}

#[test]
fn a_flying_hooks_box_is_made_again_from_its_position_at_the_end_of_its_tick_until_1_17() {
    // 1.8.8 FishingHook.java:415, 1.13.2 :239, 1.16.5 :244: the moved box's middle rounds, the remade box is the position's;
    // pj_fishing_hook_on_ground on 1.8.8 and pj_fishing_hook_hits_target on 1.16 parted by an ulp without it
    let owner = Some(player_owner(9));
    let world = WorldSnapshot::new();
    for release in [R1_8, R1_13, R1_16] {
        let mut hits = aged();
        let mut state = flying(ProjectileKind::FishingHook);
        state.position = Vec3::new(1.6949767493346908, 100.90692534674463, 0.5783538038365146);
        state.velocity = Vec3::new(0.8271410417893843, -0.19687604196014166, -0.008240769786223184);
        state.bounding_box = Some(ProjectileKind::FishingHook.bounding_box(state.position));
        tick_projectile_with_targets(&mut state, &mut hits, &[], owner, &world, release);
        assert_eq!(state.bounding_box, Some(ProjectileKind::FishingHook.bounding_box(state.position)), "{release:?}");
    }
}

#[test]
fn a_hook_before_1_11_leaves_its_box_where_it_was_as_it_rides() {
    // 1.8.8 FishingHook.java:199-202, 1.10.2 :187-190: x, y and z, no setPosition
    let owner = Some(player_owner(9));
    let villager = target(1, 2.0);
    let world = WorldSnapshot::new();
    let mut hits = ProjectileHitState { hooked: Some(1), ..aged() };
    let mut state = flying(ProjectileKind::FishingHook);
    let kept = Aabb::new(Vec3::new(-0.125, 1.0, -0.125), Vec3::new(0.125, 1.25, 0.125));
    state.bounding_box = Some(kept);
    tick_projectile_with_targets(&mut state, &mut hits, &[villager], owner, &world, R1_8);
    assert_eq!(state.position.y, 2.0 * 0.8);
    assert_eq!(state.bounding_box, Some(kept));
}

#[test]
fn a_hooked_hook_takes_no_gravity_from_1_21_6() {
    let fall = |release| {
        let mut state = flying(ProjectileKind::FishingHook);
        state.gravity = 0.03;
        let world = WorldSnapshot::new();
        tick_projectile_with_targets(&mut state, &mut aged(), &[target(1, 2.0)], Some(player_owner(9)), &world, release);
        state.velocity.y
    };
    assert_eq!(fall(R1_21_6), 0.0);
    assert!(fall(R1_21_5) < 0.0);
}

#[test]
fn a_hook_catches_items_and_needs_its_player() {
    let item = ProjectileTarget { pickable: false, item: true, ..target(1, 2.0) };
    let (_, _, report) = tick_family(ProjectileKind::FishingHook, &mut aged(), &[item], Some(player_owner(9)), R1_12);
    assert_eq!(report.entity_hits.len(), 1);
    assert!(tick_family(ProjectileKind::FishingHook, &mut aged(), &[], None, R26_3).1.discarded);
    let mob = Some(ProjectileOwner { player: false, ..player_owner(9) });
    assert!(tick_family(ProjectileKind::FishingHook, &mut aged(), &[], mob, R26_3).1.discarded);
    let mut hits = ProjectileHitState { stop_fishing: true, ..aged() };
    assert!(tick_family(ProjectileKind::FishingHook, &mut hits, &[], Some(player_owner(9)), R26_3).1.discarded);
}

#[test]
fn a_hook_is_in_the_ground_at_a_block_before_1_16() {
    let world = full_block((2, 1, 0));
    let mut state = flying(ProjectileKind::FishingHook);
    let mut hits = aged();
    tick_projectile_with_targets(&mut state, &mut hits, &[], Some(player_owner(9)), &world, R1_12);
    assert!(hits.in_ground);
    assert_eq!(state.velocity.x, 0.0);
}

#[test]
fn a_bullet_steers_by_the_delta_it_is_given() {
    let mut hits = ProjectileHitState { bullet_delta: Some(Vec3::new(1.0, 0.0, 0.0)), ..aged() };
    let (state, ..) = tick_family(ProjectileKind::ShulkerBullet, &mut hits, &[], None, R26_3);
    assert_eq!(hits.bullet_delta, Some(Vec3::new(1.0, 0.0, 0.0)), "clamped at 1");
    assert_eq!(state.velocity.x, 3.0 + (1.0 - 3.0) * 0.2);
    let mut state = flying(ProjectileKind::ShulkerBullet);
    state.gravity = 0.04;
    let world = WorldSnapshot::new();
    tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, R26_3);
    assert_eq!(state.velocity.y, -0.04);
}

#[test]
fn a_bullet_goes_on_peaceful() {
    let mut hits = ProjectileHitState { peaceful: true, ..aged() };
    assert!(tick_family(ProjectileKind::ShulkerBullet, &mut hits, &[], None, R26_3).1.discarded);
}

#[test]
fn a_bullet_never_takes_its_owner_until_1_16() {
    let me = target(5, 1.0);
    let owner = Some(ProjectileOwner { player: false, ..player_owner(5) });
    let late = || ProjectileHitState { left_owner: true, flying_ticks: 100, ..aged() };
    assert!(!tick_family(ProjectileKind::ShulkerBullet, &mut late(), &[me], owner, R1_15).1.discarded);
    assert!(tick_family(ProjectileKind::ShulkerBullet, &mut late(), &[me], owner, R1_16).1.discarded);
}

#[test]
fn a_box_in_a_block_is_a_hit_from_1_13_to_1_15() {
    let world = full_block((0, 1, 0));
    let stuck = |release| {
        let mut state = flying(ProjectileKind::SmallFireball);
        state.position = Vec3::new(0.5, 1.5, 0.5);
        state.velocity = Vec3::new(0.0, 0.0, 0.01);
        tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, release).0.discarded
    };
    assert!(stuck(R1_15));
    assert!(!stuck(R1_12));
}

#[test]
fn a_block_ends_every_flight_but_a_rocket_and_a_hook() {
    let world = full_block((2, 1, 0));
    for kind in GONE_AT_A_HIT.into_iter().chain([ProjectileKind::WindCharge, ProjectileKind::LlamaSpit]) {
        let mut state = flying(kind);
        let (result, report) = tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, R26_3);
        assert!(result.discarded, "{kind:?}");
        assert!(report.block_hit.is_some(), "{kind:?}");
    }
}

#[test]
fn a_block_stops_the_entity_behind_it() {
    let world = full_block((2, 1, 0));
    let mut state = flying(ProjectileKind::Snowball);
    let (_, report) = tick_projectile_with_targets(&mut state, &mut aged(), &[target(1, 2.5)], None, &world, R26_3);
    assert!(report.entity_hits.is_empty() && report.block_hit.is_some());
}

#[test]
fn a_thrown_item_clips_outlines_until_1_16_1() {
    let mut world = WorldSnapshot::new();
    // no collider, a torch's outline
    world.set_block(
        (2, 1, 0),
        BlockState {
            outline_shape: Some(BlockShape::full_block()),
            ..Default::default()
        },
    );
    let hits_block = |release, kind| {
        let mut state = flying(kind);
        state.position = Vec3::new(0.0, 1.5, 0.5);
        tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, release).0.block_hit
    };
    assert!(hits_block(R1_16_1, ProjectileKind::Snowball));
    assert!(!hits_block(R1_16_2, ProjectileKind::Snowball));
    assert!(hits_block(R1_16_1, ProjectileKind::LlamaSpit));
    assert!(!hits_block(R1_16_1, ProjectileKind::SmallFireball));
}

#[test]
fn the_wind_charge_box_sits_0_15_below_its_position() {
    let bounds = ProjectileKind::WindCharge.bounding_box(Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(bounds.min.y, 1.0 - f64::from(0.15_f32));
    assert_eq!(ProjectileKind::Fireball.bounding_box(Vec3::ZERO).max.y, 1.0);
}

#[test]
fn a_wind_charge_bursts_30_above_the_level_from_1_20_5() {
    let mut world = WorldSnapshot::new();
    world.set_world_bounds(0, 256, 0.0, 0.0, 6.0e7);
    let high = |y: f64, release| {
        let mut state = flying(ProjectileKind::WindCharge);
        state.position.y = y;
        tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, release).0.discarded
    };
    assert!(!high(286.0, R1_20_5));
    assert!(high(287.0, R1_20_5));
    assert!(high(286.0, R26_3));
    assert!(!high(285.0, R26_3));
}

#[test]
fn a_hooks_gravity_is_the_floats_from_26_3() {
    assert_eq!(ProjectileKind::FishingHook.default_gravity_in(R26_2), 0.03);
    assert_eq!(ProjectileKind::FishingHook.default_gravity_in(R26_3), f64::from(0.03_f32));
}

#[test]
fn gravity_makes_positive_zero_of_negative_zero_from_1_20_5() {
    let breeze = ProjectileTarget { deflects: true, ..target(1, 2.0) };
    let gravity_after_turn = |release| {
        let mut state = flying(ProjectileKind::Snowball);
        state.gravity = 0.03;
        let world = WorldSnapshot::new();
        let mut hits = aged();
        tick_projectile_with_targets(&mut state, &mut hits, &[breeze], None, &world, release);
        tick_projectile_with_targets(&mut state, &mut hits, &[], None, &world, release);
        state.velocity.z
    };
    assert!(gravity_after_turn(R26_3).is_sign_positive());
}

#[test]
fn a_wind_charge_needs_a_level_to_be_above() {
    let mut state = flying(ProjectileKind::WindCharge);
    state.position.y = 100.0;
    let world = WorldSnapshot::new();
    let (result, _) = tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, R26_3);
    assert!(!result.discarded);
}

#[test]
fn a_still_hook_leaves_the_ground_on_the_server() {
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), BlockState { shape: Some(BlockShape::full_block()), ..Default::default() });
    let mut state = ProjectileState::for_kind_in_release(ProjectileKind::FishingHook, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, R26_3);
    state.on_ground = true;
    state.first_tick = false;
    let (result, _) = tick_projectile_with_targets(&mut state, &mut aged(), &[], Some(player_owner(9)), &world, R26_3);
    assert!(!result.on_ground);
}

#[test]
fn a_hook_before_1_11_rides_what_accepts_it_at_once() {
    let owner = Some(player_owner(9));
    let villager = target(1, 2.0);
    let mut hits = aged();
    let (_, _, report) = tick_family(ProjectileKind::FishingHook, &mut hits, &[villager], owner, R1_8);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hooked);
    let (state, ..) = tick_family(ProjectileKind::FishingHook, &mut hits, &[villager], owner, R1_8);
    assert_eq!(state.position, Vec3::new(villager.position.x, 2.0 * 0.8, 0.0));
    let refusing = ProjectileTarget { hurt: false, ..villager };
    let mut hits = aged();
    let (_, _, report) = tick_family(ProjectileKind::FishingHook, &mut hits, &[refusing], owner, R1_8);
    assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Refused);
    assert_eq!(hits.hooked, None);
}

#[test]
fn a_hook_before_1_11_falls_by_its_own_numbers() {
    let (state, ..) = tick_family(ProjectileKind::FishingHook, &mut aged(), &[], Some(player_owner(9)), R1_8);
    assert_eq!(state.velocity.x, 3.0 * f64::from(0.92_f32));
    assert_eq!(state.velocity.y, -f64::from(0.04_f32) * f64::from(0.92_f32));
}

#[test]
fn a_hook_before_1_11_comes_free_of_a_block_by_the_servers_random() {
    let world = full_block((2, 1, 0));
    let mut state = flying(ProjectileKind::FishingHook);
    let mut hits = aged();
    let owner = Some(player_owner(9));
    tick_projectile_with_targets(&mut state, &mut hits, &[], owner, &world, R1_8);
    assert!(hits.in_ground);
    let at = state.position;
    assert_eq!(state.velocity.x, 3.0, "it stands for the tick");
    hits.ground_release = Some(Vec3::new(0.1, 0.1, 0.1));
    tick_projectile_with_targets(&mut state, &mut hits, &[], owner, &world, R1_8);
    assert!(!hits.in_ground);
    assert_eq!(state.position.x, at.x + 3.0 * 0.1);
}

#[test]
fn a_hook_rides_without_a_hurt_from_1_9_and_takes_items_from_1_10() {
    let owner = Some(player_owner(9));
    let refusing = ProjectileTarget { hurt: false, ..target(1, 2.0) };
    let mut hits = ProjectileHitState { hook_tracked: Some(1), ..aged() };
    tick_family(ProjectileKind::FishingHook, &mut hits, &[refusing], owner, R1_9);
    assert_eq!(hits.hooked, Some(1));
    let item = ProjectileTarget { pickable: false, item: true, ..target(1, 2.0) };
    assert!(tick_family(ProjectileKind::FishingHook, &mut aged(), &[item], owner, R1_9).2.entity_hits.is_empty());
    assert_eq!(tick_family(ProjectileKind::FishingHook, &mut aged(), &[item], owner, v(1, 10, 2)).2.entity_hits.len(), 1);
}

#[test]
fn a_legacy_ray_loses_its_last_sliver_in_the_block_it_enters() {
    // the start moves to the cell's edge, and 0.0003 of y is under a Vec3d's cutoff of 1.0E-7 squared
    let mut world = full_block((4, 99, 0));
    world.set_block((3, 99, 0), BlockState::default());
    let mut state = ProjectileState::for_kind(ProjectileKind::Snowball, Vec3::new(3.98, 100.2548, 0.55), Vec3::new(0.644, -0.2551, 0.0));
    state.first_tick = false;
    let (result, _) = tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, R1_8);
    assert!(!result.block_hit);
    let mut state = ProjectileState::for_kind(ProjectileKind::Snowball, Vec3::new(3.98, 100.2548, 0.55), Vec3::new(0.644, -0.2551, 0.0));
    state.first_tick = false;
    let (result, _) = tick_projectile_with_targets(&mut state, &mut aged(), &[], None, &world, R1_13);
    assert!(result.block_hit);
}

#[test]
fn a_hook_of_1_9_and_1_10_loses_its_first_catch_to_its_own_tracker() {
    let owner = Some(player_owner(9));
    let villager = target(1, 2.0);
    for release in [R1_9, v(1, 10, 2)] {
        let mut hits = aged();
        let (_, _, report) = tick_family(ProjectileKind::FishingHook, &mut hits, &[villager], owner, release);
        assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Passed);
        assert_eq!(hits.hooked, None);
        let (_, _, report) = tick_family(ProjectileKind::FishingHook, &mut hits, &[villager], owner, release);
        assert_eq!(report.entity_hits[0].reaction, ProjectileReaction::Hooked);
        assert_eq!(hits.hooked, Some(1));
    }
    let mut hits = aged();
    tick_family(ProjectileKind::FishingHook, &mut hits, &[villager], owner, R1_11);
    assert_eq!(hits.hooked, Some(1));
}

#[test]
fn a_fireball_meets_the_cells_past_the_border_in_1_13_only() {
    // 1.13.2 ProjectileUtil.java:27 asks noCollision of the box, whose query takes a cell past the border
    // as a full block for an entity inside it, by a block's margin its kept flag chooses (LevelReader.java:113-155,
    // 172-200); 1.14.4 LevelReader.java:147-157 counts the border only for a box meeting its edge from inside.
    // pj_fireball_border_crossing on 1.13.2 blows up at x 6.06 by a border at 5.5
    let mut world = WorldSnapshot::new();
    world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
    world.set_chunk_loaded(0, 0, true);
    let fly = |release, outside_border| {
        let mut hits = ProjectileHitState { outside_border, ..aged() };
        let mut state = flying(ProjectileKind::Fireball);
        state.position = Vec3::new(5.6, 1.0, 0.0);
        state.velocity = Vec3::new(1.0, 0.0, 0.0);
        let (result, _) = tick_projectile_with_targets(&mut state, &mut hits, &[], None, &world, release);
        (result.block_hit, hits.outside_border)
    };
    assert_eq!(fly(R1_13, false), (true, false));
    assert_eq!(fly(R1_13, true), (false, true));
    assert_eq!(fly(R1_14, false), (false, false));
    assert_eq!(fly(R1_12, false), (false, false));
}

// B290: each gate of this file that the sweep left open, on the two releases about its boundary.
const R1_15_2: MinecraftVersion = v(1, 15, 2);
const R1_16_0: MinecraftVersion = v(1, 16, 0);
const R1_20_3: MinecraftVersion = v(1, 20, 3);
const R1_20_6: MinecraftVersion = v(1, 20, 6);

/// A scene of `kind` among no listed targets, for the tests of one predicate.
fn scene_of<'a>(
    kind: ProjectileKind,
    owner: Option<ProjectileOwner>,
    hits: &'a mut ProjectileHitState,
) -> HitScene<'a> {
    let mut scene = HitScene::new(&[], owner, hits);
    scene.kind = kind;
    scene.live = true;
    scene
}

#[test]
fn a_negative_damage_rounds_up_from_itself_until_1_13_and_from_zero_from_1_14() {
    // 1.13.2 AbstractArrowEntity.java: MathHelper.ceil(f * this.damage); 1.14.4 AbstractArrow.java:341
    // Mth.ceil(Math.max(f * baseDamage, 0.0D)): a negative base damage (the arrow's damage tag) is 0 from 1.14
    assert_eq!(arrow_damage(1.0, -1.5, R1_13), -1);
    assert_eq!(arrow_damage(1.0, -1.5, R1_14), 0);
}

#[test]
fn leaving_the_owner_is_no_state_of_1_15_2() {
    // 1.15.2 AbstractArrow.java:465 spares the owner by flightTime and has no leftOwner; 1.16
    // Projectile.java:77-78 checkLeftOwner: no owner has left at once
    let left = |release| {
        let mut hits = ProjectileHitState::default();
        let mut scene = scene_of(ProjectileKind::Arrow, None, &mut hits);
        scene.check_left_owner(Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0), release);
        hits.left_owner
    };
    assert!(!left(R1_15_2));
    assert!(left(R1_16_0));
}

#[test]
fn an_arrows_candidates_on_each_side_of_1_9_1_14_and_1_16() {
    let mut hits = ProjectileHitState { flying_ticks: 50, ..ProjectileHitState::default() };
    let scene = scene_of(ProjectileKind::Arrow, None, &mut hits);
    // 1.8.8 AbstractArrow.java:204 asks collides() alone; 1.9.4 AbstractArrow.java: isAlive() too
    let dead = ProjectileTarget { alive: false, ..target(1, 1.0) };
    assert!(scene.can_hit(&dead, R1_8));
    assert!(!scene.can_hit(&dead, R1_9));
    // 1.13.2 has no piercingIgnoreEntityIds; 1.14.4 AbstractArrow.java:452 skips them
    let mut hits = ProjectileHitState { pierced: vec![1], ..ProjectileHitState::default() };
    let scene = scene_of(ProjectileKind::Arrow, None, &mut hits);
    assert!(scene.can_hit(&target(1, 1.0), R1_13));
    assert!(!scene.can_hit(&target(1, 1.0), R1_14));
    // 1.15.2 AbstractArrow.java:465 spares the owner itself for five flying ticks; 1.16
    // Projectile.java:141 spares what it rides with until the arrow has left
    let mut hits = ProjectileHitState { flying_ticks: 50, ..ProjectileHitState::default() };
    let scene = scene_of(ProjectileKind::Arrow, Some(owner(1)), &mut hits);
    let boat = ProjectileTarget { root_vehicle: 1, ..target(2, 1.0) };
    assert!(scene.can_hit(&boat, R1_15_2));
    assert!(!scene.can_hit(&boat, R1_16_0));
}

#[test]
fn an_arrow_turns_at_every_deflection_in_1_20_6_and_once_by_an_entity_from_1_21() {
    // 1.20.6 Projectile.java:158-170 deflects each time; 1.21 Projectile.java:168-177 tests lastDeflectedBy
    let breeze = ProjectileTarget { deflects: true, ..target(1, 1.0) };
    let turned = |release| {
        let targets = [breeze];
        let mut hits = ProjectileHitState { last_deflected_by: Some(1), ..aged() };
        let mut scene = HitScene::new(&targets, None, &mut hits);
        let mut velocity = Vec3::new(3.0, 0.0, 0.0);
        assert!(scene.hit((0, Vec3::new(0.7, 1.0, 0.0)), &mut velocity, release));
        velocity.x
    };
    assert_eq!(turned(R1_20_6), -1.5);
    assert_eq!(turned(R1_21), 3.0);
}

#[test]
fn a_breeze_turns_an_arrow_by_its_random_from_1_20_3() {
    // 1.20.2 AbstractArrow.java:337-390 has no DEFLECTS_ARROWS; 1.20.3 AbstractArrow.java:345, 396 deflect()
    let breeze = ProjectileTarget { hurt: false, deflects: true, deflect_random: 0.25, ..target(1, 1.0) };
    let world = WorldSnapshot::new();
    assert_eq!(shot(&[breeze], &world, R1_20_2).2.entity_hits[0].reaction, ProjectileReaction::Refused);
    assert_eq!(shot(&[breeze], &world, R1_20_3).2.entity_hits[0].reaction, ProjectileReaction::Deflected);
}

#[test]
fn a_refusal_rounds_a_subnormal_motion_by_each_release_s_arithmetic() {
    // 1.20.4 AbstractArrow.java:399 scale(-0.1); 1.20.5 AbstractArrow.java:401-402 deflect(REVERSE), the
    // scale(-0.5) of ProjectileDeflection.REVERSE, then scale(0.2). The two agree but where halving the
    // motion rounds: 5 of the least subnormal is 0.5 of it by -0.1 (up, 0.1 being over a tenth), 2 by
    // -0.5 (to even) and 0.4 of it by 0.2
    let turned = |release| {
        let mut hits = aged();
        let mut scene = scene_of(ProjectileKind::Arrow, None, &mut hits);
        let mut velocity = Vec3::new(f64::from_bits(5), 0.0, 0.0);
        scene.refused(&mut velocity, release);
        velocity.x.to_bits()
    };
    assert_eq!(turned(R1_20_4), (-f64::from_bits(1)).to_bits());
    assert_eq!(turned(R1_20_5), (-0.0_f64).to_bits());
}

#[test]
fn a_refusal_starts_the_shooters_ticks_again_until_1_15_2() {
    // 1.15.2 AbstractArrow.java:432 flightTime = 0; 1.16 AbstractArrow.java:373-376 has none
    let ticks = |release| {
        let mut hits = aged();
        let mut scene = scene_of(ProjectileKind::Arrow, None, &mut hits);
        scene.refused(&mut Vec3::new(3.0, 0.0, 0.0), release);
        hits.flying_ticks
    };
    assert_eq!(ticks(R1_15_2), 0);
    assert_eq!(ticks(R1_16_0), 20);
}

#[test]
fn a_trident_turns_a_subnormal_motion_back_by_each_release_s_arithmetic() {
    // 1.21.1 ThrownTrident.java:137 multiply(-0.01, -0.1, -0.01); 1.21.2 ThrownTrident.java:144-145 and
    // 26.2 :153-154 deflect(REVERSE) (scale(-0.5)) then multiply(0.02, 0.2, 0.02); 26.3 ThrownTrident.java:152
    // with ProjectileDeflection.java:14 multiply(-power * 0.5), -0.1 on y. Only a halving that rounds tells them apart
    let turned = |release| {
        let mut hits = aged();
        let mut scene = scene_of(ProjectileKind::Trident, None, &mut hits);
        scene.trident = true;
        let mut velocity = Vec3::new(0.0, f64::from_bits(5), 0.0);
        scene.hit_as_trident(&target(1, 1.0), Vec3::ZERO, &mut velocity, release);
        velocity.y.to_bits()
    };
    let tenth = (-f64::from_bits(1)).to_bits();
    let halved = (-0.0_f64).to_bits();
    assert_eq!(turned(R1_21_1), tenth);
    assert_eq!(turned(R1_21_2), halved);
    assert_eq!(turned(R26_2), halved);
    assert_eq!(turned(R26_3), tenth);
}

#[test]
fn the_families_candidates_on_each_side_of_1_9_1_16_and_1_20_5() {
    let mut hits = aged();
    // 1.15.2 LlamaSpit: anything but its owner; 1.16 Projectile.java:138-145 asks isPickable
    let ghost = ProjectileTarget { pickable: false, ..target(1, 2.0) };
    let scene = scene_of(ProjectileKind::LlamaSpit, None, &mut hits);
    assert!(scene.family_can_hit(&ghost, R1_15_2));
    assert!(!scene.family_can_hit(&ghost, R1_16_0));
    // 1.20.4 WindCharge.java:61-63 takes an end crystal; 1.20.5 AbstractWindCharge.java:52-58 not
    let crystal = ProjectileTarget { end_crystal: true, ..target(1, 2.0) };
    let mut hits = aged();
    let scene = scene_of(ProjectileKind::WindCharge, None, &mut hits);
    assert!(scene.family_can_hit(&crystal, R1_20_4));
    assert!(!scene.family_can_hit(&crystal, R1_20_5));
    // 1.8.9 Fireball tick asks collides() alone; 1.9.4 ProjectileUtil passes a noPhysics entity
    let no_physics = ProjectileTarget { no_physics: true, ..target(1, 2.0) };
    let mut hits = aged();
    let scene = scene_of(ProjectileKind::Fireball, None, &mut hits);
    assert!(scene.family_can_hit(&no_physics, R1_8));
    assert!(!scene.family_can_hit(&no_physics, R1_9));
}

#[test]
fn a_thrown_items_query_on_each_side_of_1_14_and_1_16() {
    let neighbour = target(4, 1.0);
    let first = || ProjectileHitState { tick_count: 0, ..aged() };
    let thrower = Some(player_owner(9));
    // 1.13.2 ThrowableEntity.java:145-169 ignores what its thrower's item meets on its first tick, as
    // 1.15.2 ThrowableProjectile.java:109-123 does; 1.16 Projectile.java:138-145 has no such entity
    for release in [R1_13, R1_15_2] {
        assert!(!tick_family(ProjectileKind::Snowball, &mut first(), &[neighbour], thrower, release).1.discarded, "{release}");
    }
    let mut hits = first();
    assert!(tick_family(ProjectileKind::Snowball, &mut hits, &[neighbour], thrower, R1_16_0).1.discarded);
    assert_eq!((hits.ignored, hits.ignore_time), (None, 0), "1.16 keeps no ignored entity");
    // 1.13.2 ThrowableEntity.java:153-160 starts nearest at 0, so a hit at a distance of 0 gives way to the next;
    // 1.14.4 ThrowableProjectile.java:127 ProjectileUtil.getHitResult keeps the strict nearest. AABB.clip takes no
    // hit at the start, so the distance is 0 as its square underflows: the item 1e-200 short of the first box
    let start = f64::from(TARGET_MARGIN);
    let (at, next) = (target(7, start), target(8, start + 1.5));
    let hit = |release| {
        let mut state = flying(ProjectileKind::Snowball);
        state.position.x = -1.0e-200;
        let world = WorldSnapshot::new();
        let (_, report) = tick_projectile_with_targets(&mut state, &mut aged(), &[at, next], None, &world, release);
        report.entity_hits[0].target
    };
    assert_eq!(hit(R1_13), 8);
    assert_eq!(hit(R1_14), 7);
}

#[test]
fn a_families_hit_is_at_the_entity_until_1_21_1_and_on_the_ray_from_1_21_2() {
    // 1.21.1 ProjectileUtil.java:137 new EntityHitResult(entity); 1.21.2 ProjectileUtil.java:139 the clip point
    let at = target(1, 2.0);
    let location = |release| tick_family(ProjectileKind::Snowball, &mut aged(), &[at], None, release).2.entity_hits[0].location;
    assert_eq!(location(R1_21_1), at.position);
    assert_eq!(location(R1_21_2).x, 2.0 - f64::from(TARGET_MARGIN));
}

#[test]
fn a_fireball_turns_at_every_deflection_and_halves_its_power_by_release() {
    let breeze = ProjectileTarget { deflects: true, ..target(1, 2.0) };
    // 1.20.6 Projectile.java:163 deflects each time; 1.21 Projectile.java:171 once by an entity
    let again = |release| {
        let mut hits = ProjectileHitState { last_deflected_by: Some(1), ..aged() };
        tick_family(ProjectileKind::Fireball, &mut hits, &[breeze], None, release).0.velocity.x
    };
    assert!(again(R1_20_6) < 0.0);
    assert!(again(R1_21) > 0.0);
    // 1.20.6 AbstractHurtingProjectile.java:188 the power from the turned motion; 1.21
    // AbstractHurtingProjectile.java:174 half the accelerationPower
    let fresh = flying(ProjectileKind::Fireball);
    let (state, ..) = tick_family(ProjectileKind::Fireball, &mut aged(), &[breeze], None, R1_20_6);
    assert_ne!(state.power, fresh.power);
    assert_eq!(state.acceleration_power, fresh.acceleration_power);
    let (state, ..) = tick_family(ProjectileKind::Fireball, &mut aged(), &[breeze], None, R1_21);
    assert_eq!(state.power, fresh.power);
    assert_eq!(state.acceleration_power, fresh.acceleration_power * 0.5);
}

#[test]
fn an_arrows_hit_at_its_start_gives_way_on_1_13_2_and_not_on_1_14_4() {
    // 1.13.2 AbstractArrowEntity.java: nearest starts at 0.0 and a hit at 0 is replaced (`d1 < d0 || d0 == 0.0D`);
    // 1.14.4 ProjectileUtil.java:117-129 the strict nearest from Double.MAX_VALUE
    // (a distance of 0 from a square that underflows: the arrow 1e-200 short of the first box)
    let world = WorldSnapshot::new();
    let start = f64::from(TARGET_MARGIN);
    let (first, second) = (target(7, start), target(8, start + 1.5));
    let hit = |release| {
        let mut state = ArrowState::new(Vec3::new(-1.0e-200, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
        ids(&tick(&mut state, &mut aged(), &[first, second], &world, release).1)
    };
    assert_eq!(hit(R1_13), [8]);
    assert_eq!(hit(R1_14), [7]);
}
