//! Fixtures for version gates no recording or other test told apart (B290, docs/version-gates.md). Each test runs the gated
//! code on the releases either side of a boundary, with an input where vanilla's two sides differ, and asserts each side's
//! own value as its source computes it: so the gate moved one release either way fails here. scripts/gate-mutants.cjs proves
//! that on the gate's own mutants.

use crate::math::{java_float, Vec3};
use crate::simulation::living_knockback;
use crate::version::MinecraftVersion;

fn release(name: &str) -> MinecraftVersion {
    MinecraftVersion::parse(name).expect("a registered release")
}

fn knock(name: &str, velocity: Vec3, on_ground: bool, strength: f64, x: f64, z: f64, resistance: f64, roll: f64) -> Option<Vec3> {
    living_knockback(velocity, on_ground, strength, x, z, resistance, roll, release(name))
}

fn bits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

/// 1.8.8 pushes 0.4F whatever strength it is given, and lifts in the air too (1.8.8 LivingEntity.java:816-831); 1.9 takes the
/// strength, as a float, and lifts only on the ground (1.9.4 LivingEntity.java:935-950).
#[test]
fn knockback_strength_and_lift_from_1_9() {
    let velocity = Vec3::new(0.2, 0.1, -0.2);
    let cap = java_float(0.4);
    // (an air hit at strength 0.5, from (3, 4): length 5 in both)
    let old = knock("1.8.8", velocity, false, 0.5, 3.0, 4.0, 0.0, 1.0).unwrap();
    let want_old = Vec3::new(0.1 - 3.0 / 5.0 * cap, (0.05 + cap).min(cap), -0.1 - 4.0 / 5.0 * cap);
    assert_eq!(bits(old), bits(want_old));
    let new = knock("1.9.4", velocity, false, 0.5, 3.0, 4.0, 0.0, 1.0).unwrap();
    let want_new = Vec3::new(0.1 - 3.0 / 5.0 * 0.5, 0.1, -0.1 - 4.0 / 5.0 * 0.5);
    assert_eq!(bits(new), bits(want_new));
}

/// Up to 1.13.2 the lift is capped at 0.4F (1.13.2 LivingEntity.java:1101-1117); from 1.14 by Math.min against the double 0.4
/// (1.14.4 LivingEntity.java:1178-1183).
#[test]
fn knockback_lift_cap_from_1_14() {
    let velocity = Vec3::new(0.2, 0.1, -0.2);
    let old = knock("1.13.2", velocity, true, 0.5, 3.0, 4.0, 0.0, 1.0).unwrap();
    assert_eq!(old.y.to_bits(), java_float(0.4).to_bits());
    let new = knock("1.14.4", velocity, true, 0.5, 3.0, 4.0, 0.0, 1.0).unwrap();
    assert_eq!(new.y.to_bits(), 0.4_f64.to_bits());
}

/// Up to 1.15.2 the resistance is a roll: a draw under it takes no knockback at all (1.15.2 LivingEntity.java:1380); from 1.16
/// it scales the strength, in float (1.16 LivingEntity.java:1510-1512), and a strength of nothing left returns at once.
#[test]
fn knockback_resistance_from_1_16() {
    let velocity = Vec3::new(0.2, 0.1, -0.2);
    // (a roll under the resistance)
    assert_eq!(bits(knock("1.15.2", velocity, true, 0.5, 3.0, 4.0, 0.5, 0.2).unwrap()), bits(velocity));
    let scaled = f64::from((f64::from(0.5_f32) * 0.5) as f32);
    let want = Vec3::new(0.1 - 3.0 / 5.0 * scaled, (0.05 + scaled).min(0.4), -0.1 - 4.0 / 5.0 * scaled);
    assert_eq!(bits(knock("1.16", velocity, true, 0.5, 3.0, 4.0, 0.5, 0.2).unwrap()), bits(want));
    // (a roll over it: 1.15.2 takes the whole strength, 1.16 the scaled one)
    let whole = Vec3::new(0.1 - 3.0 / 5.0 * 0.5, (0.05_f64 + 0.5).min(0.4), -0.1 - 4.0 / 5.0 * 0.5);
    assert_eq!(bits(knock("1.15.2", velocity, true, 0.5, 3.0, 4.0, 0.5, 0.9).unwrap()), bits(whole));
    assert_eq!(bits(knock("1.16", velocity, true, 0.5, 3.0, 4.0, 0.5, 0.9).unwrap()), bits(want));
    // (a strength of nothing: 1.15.2 still halves the motion, 1.16 returns it as it was, 1.16 LivingEntity.java:1510)
    let halved = Vec3::new(0.1, (0.05_f64).min(0.4), -0.1);
    assert_eq!(bits(knock("1.15.2", velocity, true, 0.0, 3.0, 4.0, 0.0, 0.9).unwrap()), bits(halved));
    assert_eq!(bits(knock("1.16", velocity, true, 0.0, 3.0, 4.0, 0.0, 0.9).unwrap()), bits(velocity));
}

/// Up to 1.16.5 the strength and the direction's length are floats (1.16.5 LivingEntity.java:1522-1530, Vec3.normalize by
/// Mth.sqrt); from 1.17 doubles (1.17 LivingEntity.java:1376-1384, Vec3.normalize by Math.sqrt).
#[test]
fn knockback_in_double_from_1_17() {
    let velocity = Vec3::new(0.0, 0.0, 0.0);
    let old = knock("1.16.5", velocity, false, 0.1, 1.0, 2.0, 0.3, 0.9).unwrap();
    let power = f64::from((f64::from(0.1_f32) * 0.7) as f32);
    let length = f64::from(5.0_f64.sqrt() as f32);
    assert_eq!(bits(old), bits(Vec3::new(-(1.0 / length * power), 0.0, -(2.0 / length * power))));
    let new = knock("1.17", velocity, false, 0.1, 1.0, 2.0, 0.3, 0.9).unwrap();
    let power = 0.1 * 0.7;
    let length = 5.0_f64.sqrt();
    assert_eq!(bits(new), bits(Vec3::new(-(1.0 / length * power), 0.0, -(2.0 / length * power))));
}

/// From 1.21 a direction shorter than 1.0E-5F squared is drawn again at random (1.21 LivingEntity.java:1463-1476), which the
/// caller must do; before, it takes no horizontal push (1.20.6 LivingEntity.java:1473-1481, Vec3.normalize's 1.0E-4).
#[test]
fn knockback_redraws_a_short_direction_from_1_21() {
    let velocity = Vec3::new(0.2, 0.1, -0.2);
    let old = knock("1.20.6", velocity, true, 0.5, 0.0, 0.0, 0.0, 1.0).unwrap();
    assert_eq!(bits(old), bits(Vec3::new(0.1, 0.4, -0.1)));
    assert!(knock("1.21", velocity, true, 0.5, 0.0, 0.0, 0.0, 1.0).is_none());
}

/// The capabilities a release has (version.rs): fluid physics from 1.13 (1.13.2 Entity.java:1052-1055), honey blocks from
/// 1.15 (1.15 Blocks.java:2036), fall flying from 1.9 (1.9.4 LivingEntity.java:1327), minor horizontal collision from 1.18
/// (1.18 Entity.java:574).
#[test]
fn capabilities_start_at_their_releases() {
    let caps = |name: &str| release(name).capabilities();
    assert!(!caps("1.12.2").fluid_physics && caps("1.13.2").fluid_physics);
    assert!(!caps("1.14.4").honey_blocks && caps("1.15").honey_blocks);
    assert!(!caps("1.8.8").fall_flying && caps("1.9.4").fall_flying);
    assert!(!caps("1.17.1").minor_horizontal_collision && caps("1.18").minor_horizontal_collision);
    assert!(!release("1.17.1").supports_minor_horizontal_collision() && release("1.18").supports_minor_horizontal_collision());
}

/// The Java major each release's version json declares (javaVersion.majorVersion): 8 to 1.16.5, 16 for 1.17 and 1.17.1, 17
/// to 1.20.4, 21 to 1.21.11, 25 from 26.1.
#[test]
fn declared_java_majors_by_release() {
    use crate::math::declared_java_major;
    for (name, major) in [("1.16.5", 8), ("1.17", 16), ("1.17.1", 16), ("1.18", 17), ("1.20.4", 17), ("1.20.5", 21), ("1.21.11", 21), ("26.1", 25)] {
        assert_eq!(declared_java_major(release(name)), major, "{name}");
    }
}

/// Mth.abs(float) is `f >= 0.0F ? f : -f` before 1.14, which keeps -0.0 (1.13.2 Mth.java:190-192), and Math.abs from it
/// (1.14.4 Mth.java:63-65).
#[test]
fn mth_abs_keeps_a_negative_zero_before_1_14() {
    use crate::math::abs_f32_for;
    assert!(abs_f32_for(release("1.13.2"), -0.0).is_sign_negative());
    assert!(abs_f32_for(release("1.14.4"), -0.0).is_sign_positive());
}

/// Mth.floor, lfloor and ceil take `(int) value` and step it before 26.1, which wraps past the int's range (1.21.11
/// Mth.java:65-96), and Math.floor and Math.ceil from it (26.1 Mth.java:65-90).
#[test]
fn mth_rounding_wraps_past_the_range_before_26_1() {
    use crate::math::{ceil_i32_for, floor_f32_for, floor_i64_for};
    assert_eq!(floor_f32_for(release("1.21.11"), -3.0e9), i32::MAX);
    assert_eq!(floor_f32_for(release("26.1"), -3.0e9), i32::MIN);
    assert_eq!(ceil_i32_for(release("1.21.11"), 3.0e9), i32::MIN);
    assert_eq!(ceil_i32_for(release("26.1"), 3.0e9), i32::MAX);
    assert_eq!(floor_i64_for(release("1.21.11"), -1.0e19), i64::MAX);
    assert_eq!(floor_i64_for(release("26.1"), -1.0e19), i64::MIN);
}

/// From 26.2 a friction up to 0.6 leaves the speed as it is (26.2 LivingEntity.java:2641); before, every friction takes
/// 0.21600002F over its cube (26.1.2 LivingEntity.java:2660).
#[test]
fn friction_up_to_0_6_keeps_the_speed_from_26_2() {
    use crate::math::friction_influenced_speed;
    let speed = 0.1;
    let old = f64::from(speed as f32 * (0.21600002_f32 / (0.5_f32 * 0.5 * 0.5)));
    assert_eq!(friction_influenced_speed(speed, 0.5, release("26.1.2")).to_bits(), old.to_bits());
    assert_eq!(friction_influenced_speed(speed, 0.5, release("26.2")).to_bits(), f64::from(speed as f32).to_bits());
}

fn player_at(x: f64, z: f64) -> crate::simulation::SimulationState {
    crate::simulation::SimulationState {
        position: Vec3::new(x, 64.0, z),
        half_width: 0.3,
        height: 1.8,
        is_player: true,
        ..crate::simulation::SimulationState::default()
    }
}

/// A cow whose box meets the player's pushes it from 1.9 (1.9.4 LivingEntity.java:1846, collideWithNearbyEntities and
/// Entity.applyEntityCollision); the 1.8 player is pushed by none of it on the client (1.8.8 EntityPlayerSP has no
/// pushOutOfBlocks of entities, and collideWithNearbyEntities asks canBePushed of the player itself, false for the local one).
#[test]
fn a_meeting_mob_pushes_the_player_from_1_9() {
    use crate::entity::{push_by_nearby_entities, NearbyEntity, NearbyEntityKind};
    use crate::world::WorldSnapshot;
    let world = WorldSnapshot::new();
    let bounding_box = crate::Aabb::new(Vec3::new(0.25, 64.0, -0.45), Vec3::new(1.15, 65.4, 0.45));
    let cow = NearbyEntity {
        kind: NearbyEntityKind::Living,
        position: Vec3::new(0.7, 64.0, 0.0),
        bounding_box,
        tick_position: Vec3::new(0.7, 64.0, 0.0),
        tick_bounding_box: bounding_box,
        ticks_before_player: false,
        team_pushes: true,
        spectator: false,
    };
    let mut old = player_at(0.0, 0.0);
    push_by_nearby_entities(&mut old, &world, &[cow], false, release("1.8.8"));
    assert_eq!(bits(old.velocity), bits(Vec3::ZERO));
    let mut new = player_at(0.0, 0.0);
    push_by_nearby_entities(&mut new, &world, &[cow], false, release("1.9.4"));
    let distance = f64::from(0.7_f64.sqrt() as f32);
    let push = 0.7 / distance * (1.0 / distance).min(1.0) * f64::from(0.05_f32);
    assert_eq!(new.velocity.x.to_bits(), (-push).to_bits());
}

/// A rocket attached to a gliding player pulls it toward 1.5 times its look from 1.11 (1.11.2 FireworkRocketEntity.java:118-125);
/// there is no attached rocket before (1.10.2 FireworkRocketEntity.java:99).
#[test]
fn an_attached_rocket_boosts_a_glider_from_1_11() {
    use crate::entity::boost_by_attached_rockets;
    let glider = || {
        let mut state = player_at(0.0, 0.0);
        state.fall_flying = true;
        state.look = Vec3::new(1.0, 0.0, 0.0);
        state
    };
    let mut old = glider();
    boost_by_attached_rockets(&mut old, 1, release("1.10.2"));
    assert_eq!(bits(old.velocity), bits(Vec3::ZERO));
    let mut new = glider();
    boost_by_attached_rockets(&mut new, 1, release("1.11.2"));
    assert_eq!(bits(new.velocity), bits(Vec3::new(0.1 + 1.5 * 0.5, 0.0, 0.0)));
}

/// The speed, slowness and sprint modifiers of the movement speed, in the order the modifier map iterates them: slowness,
/// sprint, speed to 1.20.4 (1.20.4 AttributeInstance.java:52), sprint, slowness, speed in 1.20.5 and 1.20.6 (1.20.5
/// AttributeInstance.java:55), sprint, speed, slowness from 1.21 (1.21 AttributeInstance.java:25). The products differ
/// in their last bits for some bases and levels, found here.
#[test]
fn movement_speed_modifiers_in_their_releases_order() {
    use crate::simulation::movement_speed_attribute;
    let sprint = java_float(1.3 - 1.0);
    let fold = |base: f64, order: [f64; 3]| order.iter().fold(base, |value, amount| value * (1.0 + amount));
    let mut found = None;
    'search: for base in [0.1, 0.13, 0.7, 0.09, 0.11, 0.23, 0.3, 0.37] {
        for speed_level in 0..6 {
            for slow_level in 0..4 {
                let speed = 0.200_000_002_980_232_24 * (f64::from(speed_level) + 1.0);
                let slowness = -0.150_000_005_960_464_48 * (f64::from(slow_level) + 1.0);
                let old = fold(base, [slowness, sprint, speed]);
                let middle = fold(base, [sprint, slowness, speed]);
                let new = fold(base, [sprint, speed, slowness]);
                if old != middle && middle != new {
                    found = Some((base, speed_level, slow_level, old, middle, new));
                    break 'search;
                }
            }
        }
    }
    let (base, speed_level, slow_level, old, middle, new) = found.expect("a base and levels the orders tell apart");
    let at = |name: &str| movement_speed_attribute(base, Some(speed_level), Some(slow_level), true, 1.3, release(name)).to_bits();
    assert_eq!(at("1.20.4"), old.to_bits());
    assert_eq!(at("1.20.5"), middle.to_bits());
    assert_eq!(at("1.20.6"), middle.to_bits());
    assert_eq!(at("1.21"), new.to_bits());
}

/// A boat's passenger is kept out of the water's current up to 1.21.11 (1.21.11 Entity.java:1566-1572); from 26.1 the boat
/// trims the passenger's fluid box instead (26.1 Entity.java:1572-1583).
#[test]
fn a_boat_passenger_skips_the_water_test_until_26_1() {
    use crate::simulation::BoatVehicle;
    let mut state = player_at(0.0, 0.0);
    state.passenger = true;
    state.boat_vehicle = Some(BoatVehicle { bounding_box: crate::Aabb::new(Vec3::new(-0.7, 63.5, -0.7), Vec3::new(0.7, 64.06, 0.7)), underwater: false });
    assert!(state.boat_excludes_water(release("1.21.11")));
    assert!(!state.boat_excludes_water(release("26.1")));
}

fn soul_sand_world() -> crate::world::WorldSnapshot {
    use crate::world::{BlockEffect, BlockShape, BlockState, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), BlockState { id: 1, shape: Some(BlockShape { effect: BlockEffect::SoulSand, ..BlockShape::full_block() }), ..BlockState::default() });
    world
}

/// On soul sand a soul speed walker keeps its speed from 1.16 (1.16 LivingEntity.java:647-654); 1.15.2 knows no soul speed
/// (1.15.2 Entity.java:681-689). From 1.21 the factor is lerped by the movement efficiency attribute toward 1 (1.21
/// LivingEntity.java:475-476), which 1.20.6 has not (1.20.6 LivingEntity.java:469-470).
#[test]
fn soul_sand_speed_with_soul_speed_and_efficiency() {
    use crate::simulation::block_speed_factor;
    let world = soul_sand_world();
    let mut state = player_at(0.5, 0.5);
    state.position.y = 1.0;
    state.soul_speed = 1;
    assert_eq!(block_speed_factor(&world, &state, None, release("1.15.2")), 0.4);
    assert_eq!(block_speed_factor(&world, &state, None, release("1.16")), 1.0);
    let mut efficient = player_at(0.5, 0.5);
    efficient.position.y = 1.0;
    efficient.movement_efficiency = 1.0;
    assert_eq!(block_speed_factor(&world, &efficient, None, release("1.20.6")), 0.4);
    assert_eq!(block_speed_factor(&world, &efficient, None, release("1.21")), 1.0);
}

/// The block below that affects the movement is one block under the feet to 1.14.4 (1.14.4 LivingEntity.java:1828) and
/// 0.5000001 under them from 1.15 (1.15 Entity.java:692).
#[test]
fn the_block_below_from_1_15_is_half_a_block_down() {
    use crate::simulation::block_below_affecting_movement;
    let world = crate::world::WorldSnapshot::new();
    let mut state = player_at(0.5, 0.5);
    state.position.y = 1.6;
    assert_eq!(block_below_affecting_movement(&world, &state, None, release("1.14.4")), (0, 0, 0));
    assert_eq!(block_below_affecting_movement(&world, &state, None, release("1.15")), (0, 1, 0));
}

/// A power at most 1.0E-5F does not jump from 1.20.5 (1.20.5 LivingEntity.java:2072-2073); 1.20.4 sets it whatever it is
/// (1.20.4 LivingEntity.java:2031-2033).
#[test]
fn a_negligible_jump_power_jumps_until_1_20_5() {
    use crate::simulation::jump_from_ground;
    let mut old = player_at(0.0, 0.0);
    old.velocity.y = -0.0784;
    jump_from_ground(&mut old, 1.0e-6, release("1.20.4"));
    assert_eq!(old.velocity.y, 1.0e-6);
    let mut new = player_at(0.0, 0.0);
    new.velocity.y = -0.0784;
    jump_from_ground(&mut new, 1.0e-6, release("1.20.5"));
    assert_eq!(new.velocity.y, -0.0784);
}

/// The sneak scales the impulse by 0.3F to 1.18.2 (1.18.2 KeyboardInput.java:32) and by the sneaking speed from 1.19
/// (1.19 LocalPlayer.java:738); from 1.21.4 the item's 0.2F comes before the sneak (1.21.4 LocalPlayer.java:642), after it
/// before (1.21.3 LocalPlayer.java:643), which a float product can tell apart.
#[test]
fn the_input_scaling_order_and_the_sneaking_speed() {
    use crate::simulation::input_move_vector;
    let mut sneak = player_at(0.0, 0.0);
    sneak.input = Vec3::new(1.0, 0.0, 1.0);
    sneak.sneaking = true;
    sneak.sneaking_speed = Some(0.5);
    assert_eq!(input_move_vector(&sneak, release("1.18.2")), [0.3, 0.3]);
    assert_eq!(input_move_vector(&sneak, release("1.19")), [0.5, 0.5]);
    // an impulse whose two float products differ by order
    let impulse = [0.7_f32, 0.9, 0.33, 0.47, 0.61, 0.83, 0.13]
        .into_iter()
        .find(|x| x * 0.2 * 0.3 != x * 0.3 * 0.2)
        .expect("an impulse the order changes");
    let mut both = player_at(0.0, 0.0);
    both.input = Vec3::new(f64::from(impulse), 0.0, 0.0);
    both.sneaking = true;
    both.using_item = true;
    assert_eq!(input_move_vector(&both, release("1.21.3"))[0], impulse * 0.3 * 0.2);
    assert_eq!(input_move_vector(&both, release("1.21.4"))[0], impulse * 0.2 * 0.3);
}

/// A slime block bounces what is not living by 0.8 from 1.9 (1.9.4 SlimeBlock.java:34-44); 1.8.8 bounces all alike (1.8.8
/// SlimeBlock.java:33-39).
#[test]
fn a_slime_bounce_keeps_0_8_of_a_non_living_from_1_9() {
    use crate::simulation::landing_bounce;
    let mut state = player_at(0.0, 0.0);
    state.non_living = true;
    assert_eq!(landing_bounce(&state, release("1.8.8")), 1.0);
    assert_eq!(landing_bounce(&state, release("1.9.4")), 0.8);
}

/// From 1.21.2 a client instance it does not control neither applies the blocks' effects after travel (1.21.2
/// LivingEntity.java:2817) nor runs a block's fall callback (1.21.2 Entity.java:709-724); 1.21.1 does both on every
/// instance (1.21.1 Entity.java:690-692). A removed entity runs no fall callback from 1.17 (1.17 Entity.java:570), and from
/// 26.2 a move that asked no vertical part and met no wall runs none (26.2 Entity.java:793, 815-816).
#[test]
fn which_instances_run_the_effects_and_the_fall_callback() {
    use crate::simulation::{applies_effects_from_blocks, block_fall_callback_runs, simulates_landing};
    let mut remote = player_at(0.0, 0.0);
    remote.client_side = true;
    remote.controlled_by_local_instance = false;
    assert!(applies_effects_from_blocks(&remote, release("1.21.1")));
    assert!(!applies_effects_from_blocks(&remote, release("1.21.2")));
    assert!(simulates_landing(&remote, release("1.21.1")));
    assert!(!simulates_landing(&remote, release("1.21.2")));
    let mut removed = player_at(0.0, 0.0);
    removed.removed = true;
    assert!(block_fall_callback_runs(&removed, true, false, -0.1, release("1.16.5")));
    assert!(!block_fall_callback_runs(&removed, true, false, -0.1, release("1.17")));
    let mut local = player_at(0.0, 0.0);
    local.controlled_by_local_instance = true;
    assert!(block_fall_callback_runs(&local, true, false, 0.0, release("26.1.2")));
    assert!(!block_fall_callback_runs(&local, true, false, 0.0, release("26.2")));
}

fn bordered() -> crate::world::WorldSnapshot {
    let mut world = crate::world::WorldSnapshot::new();
    world.set_world_bounds(0, 256, 0.0, 0.0, 10.0);
    world
}

/// A box just past the border's east edge (5 + 1e-7: shrunk by 1e-7 it meets none of the outside, grown by 1e-7 it does).
/// getBlockCollisions offers the border to it from 1.14 (1.14.4 LevelReader.java:149-156), not before (1.13.2
/// LevelReader.java:134-136, the border is cells there); the border term of noCollision says so too (1.14.4
/// LevelReader.java:149-157).
#[test]
fn the_border_offered_to_a_box_touching_it_from_inside() {
    let world = bordered();
    let touching = crate::Aabb::new(Vec3::new(4.4, 64.0, 0.0), Vec3::new(5.0 + 1.0e-7, 65.8, 0.6));
    assert!(!world.block_collisions_offer_world_border(touching, release("1.13.2")));
    assert!(world.block_collisions_offer_world_border(touching, release("1.14.4")));
    let query = crate::Aabb::new(Vec3::new(4.4, 64.0, 0.0), Vec3::new(4.9, 65.8, 0.6));
    let at = Vec3::new(4.7, 64.0, 0.3);
    assert!(world.no_border_collision_for_entity(query, at, touching, release("1.13.2")));
    assert!(!world.no_border_collision_for_entity(query, at, touching, release("1.14.4")));
}

/// WorldBorder.clampToBounds holds a position to the edges, then takes its block: to the east edge itself before 1.21
/// (1.20.6 WorldBorder.java:56-58), to a block short of it from 1.21 (1.21 WorldBorder.java:73-75).
#[test]
fn the_border_clamp_stops_a_block_short_from_1_21() {
    let world = bordered();
    let outside = Vec3::new(10.0, 64.0, 0.0);
    assert_eq!(world.clamp_to_world_border(outside, release("1.20.6")).x, 5.0);
    assert_eq!(world.clamp_to_world_border(outside, release("1.21")).x, 4.0);
}

/// A boat passenger's eyes in water: 1.8.8 has no boat test and finds the water (1.8.8 Entity.java:907-912); 1.9 - 1.15.2
/// say no in any boat (1.9.4 Entity.java:1013-1015); from 1.16 only eyes inside the boat's box are kept out of the water
/// (1.16 Entity.java:1069-1075).
#[test]
fn a_boat_passengers_eyes_in_the_water() {
    use crate::simulation::{entity_eyes_in_fluid, BoatVehicle};
    use crate::world::{BlockState, FluidState, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), BlockState { fluid: FluidState::Water, fluid_source: true, ..BlockState::default() });
    let mut state = player_at(0.5, 0.5);
    state.position.y = 0.2;
    state.half_width = 0.2;
    state.height = 1.6;
    state.eye_height = Some(0.4);
    state.passenger = true;
    // (a boat below the eyes)
    state.boat_vehicle = Some(BoatVehicle { bounding_box: crate::Aabb::new(Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.0, 0.1, 1.0)), underwater: false });
    let eyes = |name: &str| entity_eyes_in_fluid(&state, &world, FluidState::Water, release(name));
    assert!(eyes("1.8.8"));
    assert!(!eyes("1.9.4"));
    assert!(!eyes("1.15.2"));
    assert!(eyes("1.16"));
}

/// Where a boat seats a passenger, 1.9 - 1.20.1: the seat's forward offset turned by -yRot - 90 degrees with Mth's sin and cos,
/// at the float sum of the boat's mounted offset and the passenger's (1.17 Boat.java:650-668).
fn seated(boat: &crate::vehicle::BoatSimulationState, offset: f32, mounted: f64, passenger_offset: f64, name: &str) -> Vec3 {
    use crate::simulation::{mth_cos, mth_sin};
    let yaw = -boat.yaw_degrees * 0.017453292;
    let angle = yaw - 1.5707964;
    let cos = mth_cos(f64::from(angle), release(name));
    let sin = mth_sin(f64::from(angle), release(name));
    let forward = f64::from(offset);
    let height = (mounted + passenger_offset) as f32;
    Vec3::new(boat.position.x + (forward * cos + 0.0 * sin), boat.position.y + f64::from(height), boat.position.z + (0.0 * cos - forward * sin))
}

/// A raft mounts its passenger at -0.1 to 1.19.2 like any boat (1.19.2 Boat.java:151), at 0.3 from 1.19.3 (1.19.3 Boat.java:154).
#[test]
fn a_rafts_mount_from_1_19_3() {
    use crate::vehicle::{boat_passenger_position, BoatSeat, BoatSimulationState};
    let boat = BoatSimulationState { position: Vec3::new(0.5, 64.0, 0.5), yaw_degrees: 30.0, ..BoatSimulationState::default() };
    let raft = BoatSeat { index: 0, passengers: 1, animal: false, chest: false, raft: true, passenger_offset: 0.0 };
    assert_eq!(bits(boat_passenger_position(&boat, raft, release("1.19.2"))), bits(seated(&boat, 0.0, -0.1, 0.0, "1.19.2")));
    assert_eq!(bits(boat_passenger_position(&boat, raft, release("1.19.3"))), bits(seated(&boat, 0.0, 0.3, 0.0, "1.19.3")));
}

/// An orb pulled toward a player whose feet are within 8 blocks and whose half eye height is past them: before 1.21.5 only
/// a distance to the eyes under 8 pulls (1.21.4 ExperienceOrb.java:99-106 `d0 < 64.0`); from 1.21.5 any target pulls, by the
/// square of 1 - d/8 (1.21.5 ExperienceOrb.java:125-136).
#[test]
fn an_orb_pulled_past_8_blocks_from_1_21_5() {
    use crate::moving_entity::{tick_moving_entity, MovingEntityInputs, MovingEntityKind, MovingEntityPlayer, MovingEntityState};
    let mut world = crate::world::WorldSnapshot::new();
    let mut tick = |name: &str, with_player: bool| {
        let mut state = MovingEntityState::new(MovingEntityKind::ExperienceOrb, Vec3::new(0.5, 110.0, 0.5), Vec3::ZERO).on_server();
        state.following_player = Some(7);
        let players = if with_player {
            vec![MovingEntityPlayer { id: 7, position: Vec3::new(0.5, 117.5, 0.5), eye_height: 1.62, spectator: false, dead: false }]
        } else {
            Vec::new()
        };
        tick_moving_entity(&mut state, &mut world, &MovingEntityInputs { players, ..MovingEntityInputs::default() }, release(name));
        state.velocity
    };
    assert_eq!(bits(tick("1.21.4", true)), bits(tick("1.21.4", false)));
    assert_ne!(bits(tick("1.21.5", true)), bits(tick("1.21.5", false)));
}

fn burning(ticks: i32) -> crate::simulation::SimulationState {
    let mut state = player_at(0.5, 0.5);
    state.remaining_fire_ticks = Some(ticks);
    state
}

/// The server's fire in Entity.baseTick. Water puts the fire out before the burning counts down from 1.17 (1.17
/// Entity.java:1059-1070, checkWaterState before the fire), so the countdown's freeze reset (1.17 Entity.java:419-439) does not
/// come; a fire-immune entity burns down four a tick to 1.21.8 (1.21.8 Entity.java:497-510) and is cleared at once from 1.21.9
/// (1.21.9 Entity.java:551-563), which an entity no LivingEntity.baseTick clears after (a boat) keeps.
#[test]
fn the_server_and_client_fire_of_the_base_tick() {
    use crate::simulation::base_tick_fire;
    let world = crate::world::WorldSnapshot::new();
    let mut wet = burning(100);
    wet.in_water = true;
    wet.ticks_frozen = 50;
    base_tick_fire(&mut wet, &world, release("1.17"));
    assert_eq!((wet.remaining_fire_ticks, wet.ticks_frozen), (Some(0), 50));
    let mut immune = burning(100);
    immune.fire_immune = true;
    immune.boat = true;
    base_tick_fire(&mut immune, &world, release("1.21.8"));
    assert_eq!(immune.remaining_fire_ticks, Some(96));
    let mut immune = burning(100);
    immune.fire_immune = true;
    immune.boat = true;
    base_tick_fire(&mut immune, &world, release("1.21.9"));
    assert_eq!(immune.remaining_fire_ticks, Some(0));
}

/// A move blocked on x and z: up to 1.13.2 each blocked axis stops (1.13.2 Entity.java:793-799); 1.14 - 1.18.1 stop x only
/// when z goes on (1.14.4 Entity.java:520-525).
#[test]
fn a_move_blocked_on_both_axes_stops_x_only_alone_from_1_14() {
    use crate::simulation::{stop_blocked_motion, FallCheck};
    let stop = |name: &str| {
        let mut state = player_at(0.0, 0.0);
        state.velocity = Vec3::new(0.1, 0.0, 0.1);
        stop_blocked_motion(&mut state, Vec3::new(0.1, 0.0, 0.1), Vec3::ZERO, [true, false, true], FallCheck::default(), release(name));
        state.velocity
    };
    assert_eq!(bits(stop("1.13.2")), bits(Vec3::new(0.0, 0.0, 0.0)));
    assert_eq!(bits(stop("1.14.4")), bits(Vec3::new(0.1, 0.0, 0.0)));
}

/// The Movement a move records takes its axis order from the collided movement in 1.21.5 - 1.21.8 (1.21.5 Entity.java:720-721)
/// and from the requested one from 1.21.9 (1.21.9 Entity.java:750-751).
#[test]
fn a_recorded_movements_axis_order_from_1_21_9() {
    use crate::simulation::recorded_movement;
    let state = player_at(0.0, 0.0);
    let (requested, moved) = (Vec3::new(1.0, 0.0, 1.0), Vec3::new(0.5, 0.0, 1.0));
    assert_eq!(recorded_movement(&state, Vec3::ZERO, requested, moved, release("1.21.8")).unwrap().axis_order, Some(moved));
    assert_eq!(recorded_movement(&state, Vec3::ZERO, requested, moved, release("1.21.9")).unwrap().axis_order, Some(requested));
}

/// The entity colliders of a move are those meeting the blocks' query before 1.14 (1.13.2 Entity.java:676, 708) and the swept
/// box grown by 1.0E-7 from it (1.14.4 Entity.java:696).
#[test]
fn the_entity_collision_query_from_1_14() {
    use crate::simulation::entity_collision_query;
    let swept = crate::Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0));
    let query = crate::Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 3.0, 2.0));
    assert_eq!(entity_collision_query(swept, query, 0.6, release("1.13.2")), query);
    assert_eq!(entity_collision_query(swept, query, 0.6, release("1.14.4")), swept.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7)));
}

/// Eyes in a flowing water of level 3 at the level's bottom cell: 1.12.2 tests them below (1 - ((3 + 1) / 9F - 0.11111111F))
/// in float (1.12.2 Entity.java:1118-1119), 1.13.2 below the water's own height plus 0.11111111F (1.13.2 Entity.java:1140),
/// which round apart in the cell at 0. From 1.14 the height is FlowingFluid.getHeight, a whole block under the same fluid
/// (1.14.4 Entity.java:1162, FlowingFluid.java:461-462), where 1.13.2 still takes the cell's own.
#[test]
fn eyes_in_a_flowing_water_by_release() {
    use crate::world::{BlockState, FluidState, WorldSnapshot};
    let flowing = BlockState { fluid: FluidState::Water, fluid_level: 3, ..BlockState::default() };
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), flowing);
    let bounds = crate::Aabb::new(Vec3::new(0.2, -1.0, 0.2), Vec3::new(0.8, 0.8, 0.8));
    let at = |world: &WorldSnapshot, y: f64, name: &str| world.eyes_in_fluid(FluidState::Water, bounds, (0.5, y, 0.5), release(name));
    assert!(!at(&world, 0.666_666_65, "1.12.2"));
    assert!(at(&world, 0.666_666_65, "1.13.2"));
    world.set_block((0, 1, 0), flowing);
    assert!(!at(&world, 0.9, "1.13.2"));
    assert!(at(&world, 0.9, "1.14.4"));
}

fn dragon() -> crate::dragon::DragonState {
    use crate::dragon::{DragonFlightHistory, DragonState};
    DragonState {
        position: Vec3::new(0.5, 100.0, 0.5),
        velocity: Vec3::new(0.1, 0.0, -0.2),
        yaw: 30.0,
        yaw_acceleration: 2.0,
        in_wall: false,
        bounding_box: None,
        target: Some(Vec3::new(40.0, 110.0, -20.0)),
        steering_target: None,
        fly_speed: 0.6,
        turn_speed: 0.5,
        sitting: false,
        history: DragonFlightHistory::default(),
    }
}

/// A dragon's move goes through its bounding box, which the position follows, to 1.16.5 (1.16.5 Entity.java:574-576 setBoundingBox
/// then setLocationFromBoundingbox); from 1.17 the position moves and the box is not kept (1.17 Entity.java:574-575).
#[test]
fn a_dragon_moves_its_box_until_1_17() {
    use crate::dragon::tick_dragon_flight;
    let boxed = crate::Aabb::new(Vec3::new(-3.5, 100.0, -3.5), Vec3::new(4.5, 108.0, 4.5));
    let flown = |name: &str| {
        let mut state = dragon();
        state.bounding_box = Some(boxed);
        tick_dragon_flight(&mut state, release(name));
        state.bounding_box
    };
    assert_ne!(flown("1.16.5"), Some(boxed));
    assert_eq!(flown("1.17"), Some(boxed));
}

/// The tail's turn from the body's: the yaw difference wrapped in double from 1.21.2 (1.21.2 EnderDragon.java:306), from the float
/// difference before (1.21.1 EnderDragon.java:333). Where the difference wraps they round apart; where it does not they agree.
#[test]
fn the_dragon_tails_wrapped_turn_from_1_21_2() {
    use crate::dragon::{dragon_parts, DragonFlightHistory, DragonFlightSample};
    let parts = |tail_yaw: f32, name: &str| {
        let mut state = dragon();
        state.history = DragonFlightHistory::new(
            (0..20)
                .map(|delay| DragonFlightSample { y: 100.0, yaw: if delay < 10 { -170.1 } else { tail_yaw } })
                .collect(),
        );
        dragon_parts(&state, release(name)).into_iter().map(|part| part.position).collect::<Vec<_>>()
    };
    // (the turn goes through Mth.sin's table, whose steps are wider than the rounding, so most wrapping yaws agree: find one)
    let wraps_apart = (0..100_000).map(|step| 150.0 + step as f32 * 0.000_37).any(|tail| parts(tail, "1.21.1") != parts(tail, "1.21.2"));
    assert!(wraps_apart, "a wrapping yaw whose turn rounds apart");
    assert_eq!(parts(-150.3, "1.21.1"), parts(-150.3, "1.21.2"));
}

/// A dragon's parts enter the entity lists after the dragon until 1.21.4, after every other entity from it (1.21.3
/// Level.java:578-589, 1.21.4 Level.java:583).
#[test]
fn a_dragons_parts_follow_it_until_1_21_4() {
    use crate::dragon::parts_follow_dragon;
    assert!(parts_follow_dragon(release("1.21.3")));
    assert!(!parts_follow_dragon(release("1.21.4")));
}

/// A fluid contact's depth from the box's bottom deflated by 0.001 to 1.21.11 (1.21.11 Entity.java:3687), from the box's own
/// bottom from 26.1 (26.1 EntityFluidInteraction.java:74). The box sits in the middle of a chunk: 26.1 scans only where the
/// chunks one block round the box are loaded (26.1 EntityFluidInteraction.java:44).
#[test]
fn the_fluid_depth_from_the_box_bottom_from_26_1() {
    use crate::world::{BlockState, FluidState, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    world.set_block((8, 64, 8), BlockState { fluid: FluidState::Water, fluid_source: true, water_block: true, ..BlockState::default() });
    let bounds = crate::Aabb::new(Vec3::new(8.2, 64.2, 8.2), Vec3::new(8.8, 66.0, 8.8));
    let height = |name: &str| world.fluid_contact(FluidState::Water, bounds, release(name)).height;
    // (1.21.11 also sums the top in float: the difference is the 0.001 within a float's rounding)
    let difference = height("26.1") - height("1.21.11");
    assert!((difference - 0.001).abs() < 1.0e-5, "{difference}");
}

fn query_entity(context: &crate::world::EntityCollisionContext, bounds: crate::Aabb) -> crate::world::CollisionQueryEntity<'_> {
    crate::world::CollisionQueryEntity { context, bounds, position: Vec3::new((bounds.min.x + bounds.max.x) / 2.0, bounds.min.y, (bounds.min.z + bounds.max.z) / 2.0), outside_border: false }
}

/// A collision box of no height counts before 1.13, AABB.intersects holding it (1.12.2 Level.java:1161-1184); from 1.13 a shape
/// of no height is empty (1.13.2 LevelReader.java:123).
#[test]
fn a_box_of_no_height_collides_before_1_13() {
    use crate::world::{BlockShape, BlockState, EntityCollisionContext, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    let flat = BlockShape { bounds: crate::Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0)), ..BlockShape::full_block() };
    world.set_block((0, 64, 0), BlockState { id: 1, shape: Some(flat), ..BlockState::default() });
    let context = EntityCollisionContext::default();
    let query = crate::Aabb::new(Vec3::new(0.2, 63.9, 0.2), Vec3::new(0.8, 64.5, 0.8));
    let entity = query_entity(&context, query);
    assert!(!world.no_collision_for_entity(query, entity, release("1.12.2")));
    assert!(world.no_collision_for_entity(query, entity, release("1.13.2")));
}

/// A query reaching past the border for an entity well inside it: before 1.14 the cells outside the border are stone to
/// Level.doesBoxCollide (1.13.2 Level.java:1794-1797); from 1.14 the border is offered only to an entity touching it (1.14.4
/// LevelReader.java:146-155).
#[test]
fn the_border_cells_collide_before_1_14() {
    use crate::world::EntityCollisionContext;
    let mut world = bordered();
    // (a block loads the chunk: Level.doesBoxCollide makes stone only of loaded cells)
    world.set_block((0, 0, 0), crate::world::BlockState::default());
    let context = EntityCollisionContext::default();
    let own = crate::Aabb::new(Vec3::new(2.7, 64.0, 0.2), Vec3::new(3.3, 65.8, 0.8));
    let query = crate::Aabb::new(Vec3::new(4.5, 64.0, 0.2), Vec3::new(5.5, 65.8, 0.8));
    let entity = query_entity(&context, own);
    assert!(!world.no_collision_for_entity(query, entity, release("1.13.2")));
    assert!(world.no_collision_for_entity(query, entity, release("1.14.4")));
}

/// The Shapes.block() object meets a query by the strict AABB.intersects from 1.16 (1.16.5 CollisionSpliterator.java:74-82); 1.15.2
/// joins it as any shape, where an overlap within 1e-7 is none (1.15.2 CollisionGetter.java:109).
#[test]
fn the_block_shape_meets_a_query_strictly_from_1_16() {
    use crate::world::shape_meets_query;
    let cube = crate::Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    let query = crate::Aabb::new(Vec3::new(1.0 - 5.0e-8, 0.2, 0.2), Vec3::new(1.6, 0.8, 0.8));
    assert!(!shape_meets_query(cube, true, query, release("1.15.2")));
    assert!(shape_meets_query(cube, true, query, release("1.16")));
}

/// Shapes.joinIsNotEmpty merges the query first before 1.16 (1.15.2 CollisionGetter.java:109) and the block's shape first from it
/// (1.16 CollisionSpliterator.java:81). The IndirectMerger keeps the first list's coordinate where two lie within 1e-7, so a
/// query of x from 1 - 1e-7 to 1 + 5e-8 meets a unit cube one way and not the other.
#[test]
fn the_join_order_from_1_16() {
    use crate::world::shape_meets_query;
    let cube = crate::Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    let query = crate::Aabb::new(Vec3::new(1.0 - 1.0e-7, 0.0, 0.0), Vec3::new(1.0 + 5.0e-8, 1.0, 1.0));
    assert!(shape_meets_query(cube, false, query, release("1.15.2")));
    assert!(!shape_meets_query(cube, false, query, release("1.16")));
}

/// Three quarters of a slab, the cell at x 0 - 0.5, y 0 - 0.5 left out, joined by Shapes.or: forAllBoxes walks x then y to 1.17
/// (1.17 BitSetDiscreteVoxelShape.java:151-191), so its first box is the upper row; from 1.18 y then x (1.18
/// BitSetDiscreteVoxelShape.java:151-191), so its first box is the right column.
#[test]
fn the_joined_boxes_walk_y_first_from_1_18() {
    use crate::voxel_shape::VoxelShape;
    let part = |min: (f64, f64), max: (f64, f64)| VoxelShape::from_box(crate::Aabb::new(Vec3::new(min.0, min.1, 0.0), Vec3::new(max.0, max.1, 1.0)));
    let shapes = [part((0.5, 0.0), (1.0, 0.5)), part((0.0, 0.5), (0.5, 1.0)), part((0.5, 0.5), (1.0, 1.0))];
    let first = |name: &str| VoxelShape::union(release(name), &shapes).boxes()[0];
    assert_eq!(first("1.17.1"), crate::Aabb::new(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)));
    assert_eq!(first("1.18"), crate::Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)));
}

fn rocket(position: Vec3, velocity: Vec3) -> crate::projectile::ProjectileState {
    use crate::projectile::{ProjectileKind, ProjectileState};
    let mut state = ProjectileState::for_kind(ProjectileKind::Firework, position, velocity);
    state.first_tick = false;
    state.gravity = 0.0;
    state.shot_at_angle = true;
    state
}

fn aged_rocket(explodes: bool) -> crate::projectile_hits::ProjectileHitState {
    crate::projectile_hits::ProjectileHitState { tick_count: 20, flying_ticks: 20, left_owner: true, explodes, ..Default::default() }
}

/// An exploding rocket whose box grazes a block its line misses: 1.14 and 1.15 explode it on the collision alone (1.14.4
/// FireworkRocketEntity.java:176-180, 182-197); 1.13.2 has no such test (FireworkRocketEntity.java:102-182) and from 1.16 only
/// a hit explodes it (1.16 FireworkRocketEntity.java:154-160).
#[test]
fn a_rocket_grazing_a_block_explodes_in_1_14_and_1_15() {
    use crate::projectile::tick_projectile_with_targets;
    use crate::world::{BlockShape, BlockState, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    world.set_block((2, 1, 0), BlockState { id: 1, shape: Some(BlockShape::full_block()), outline_shape: Some(BlockShape::full_block()), ..BlockState::default() });
    let burst = |name: &str| {
        // (a box 0.8 - 1.05 high meets the block from 1; the line at 0.8 passes under it)
        let mut state = rocket(Vec3::new(0.0, 0.8, 0.5), Vec3::new(3.0, 0.0, 0.0));
        let mut hits = aged_rocket(true);
        tick_projectile_with_targets(&mut state, &mut hits, &[], None, &world, release(name)).0.discarded
    };
    assert!(!burst("1.13.2"));
    assert!(burst("1.14.4"));
    assert!(burst("1.15.2"));
    assert!(!burst("1.16"));
}

/// A rocket with noPhysics moves its box, which the position follows, to 1.16.5 (1.16.5 Entity.java:610-612), and its position
/// from 1.17 (1.17 Entity.java:574-575); the box's centre rounds apart from the sum for some x.
#[test]
fn a_no_physics_rocket_moves_its_position_from_1_17() {
    use crate::projectile::tick_projectile_with_targets;
    let world = crate::world::WorldSnapshot::new();
    let v = 0.31;
    let x = (1..2000)
        .map(|i| f64::from(i) * 0.0137 + 0.003)
        .find(|&x| ((x - 0.125 + v) + (x + 0.125 + v)) / 2.0 != x + v)
        .expect("an x whose box centre rounds apart");
    let moved = |name: &str| {
        let mut state = rocket(Vec3::new(x, 1.0, 0.5), Vec3::new(v, 0.0, 0.0));
        state.no_physics = true;
        let mut hits = aged_rocket(false);
        tick_projectile_with_targets(&mut state, &mut hits, &[], None, &world, release(name));
        state.position.x
    };
    assert_ne!(moved("1.16.5"), x + v);
    assert_eq!(moved("1.17"), x + v);
}

fn mob(id: i32, x: f64) -> crate::projectile_hits::ProjectileTarget {
    use crate::projectile_hits::{target_hitbox, ProjectileTarget};
    let position = Vec3::new(x + 0.5, 0.0, 0.0);
    ProjectileTarget::new(id, position, target_hitbox(position, 1.0, 2.0, 1.0))
}

fn thrower(id: i32) -> crate::projectile_hits::ProjectileOwner {
    crate::projectile_hits::ProjectileOwner {
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

fn thrown(kind: crate::projectile::ProjectileKind, tick_count: i32, targets: &[crate::projectile_hits::ProjectileTarget], owner: Option<crate::projectile_hits::ProjectileOwner>, name: &str) -> bool {
    use crate::projectile::{tick_projectile_with_targets, ProjectileState};
    let mut state = ProjectileState::for_kind(kind, Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
    state.first_tick = false;
    state.gravity = 0.0;
    let mut hits = crate::projectile_hits::ProjectileHitState { tick_count, flying_ticks: 30, left_owner: true, ..Default::default() };
    tick_projectile_with_targets(&mut state, &mut hits, targets, owner, &crate::world::WorldSnapshot::new(), release(name)).0.discarded
}

/// A thrown item's first tick ignores what is in its box: anything's in 1.10.2 (1.10.2 ThrowableProjectile.java:165), from 1.11
/// only where someone threw it (1.11.2 ThrowableProjectile.java:165), so a snowball nobody threw hits its neighbour.
#[test]
fn an_unthrown_item_ignores_its_neighbour_until_1_11() {
    use crate::projectile::ProjectileKind;
    assert!(!thrown(ProjectileKind::Snowball, 0, &[mob(4, 1.0)], None, "1.10.2"));
    assert!(thrown(ProjectileKind::Snowball, 0, &[mob(4, 1.0)], None, "1.11.2"));
}

/// An ender pearl passes its thrower until 1.15.2 (1.15.2 ThrownEnderpearl.java:51); from 1.16 it lands on it (1.16
/// ThrownEnderpearl.java:41).
#[test]
fn a_pearl_passes_its_thrower_until_1_16() {
    use crate::projectile::ProjectileKind;
    assert!(!thrown(ProjectileKind::EnderPearl, 20, &[mob(5, 1.0)], Some(thrower(5)), "1.15.2"));
    assert!(thrown(ProjectileKind::EnderPearl, 20, &[mob(5, 1.0)], Some(thrower(5)), "1.16"));
}

/// A trident refused by an entity that deflects tridents: 1.20.2 has no DEFLECTS_TRIDENTS (1.20.2 ThrownTrident.java:126-140), so
/// it is refused; 1.20.3 and 1.20.4 turn it (1.20.3 ThrownTrident.java:133-134).
#[test]
fn a_trident_deflected_in_1_20_3_and_1_20_4() {
    use crate::projectile::{tick_trident_with_targets, TridentState};
    use crate::projectile_hits::{ProjectileHitState, ProjectileReaction, ProjectileTarget};
    let breeze = ProjectileTarget { hurt: false, deflects: true, deflect_random: 0.25, ..mob(1, 1.0) };
    let reaction = |name: &str| {
        let mut state = TridentState::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
        let mut hits = ProjectileHitState { tick_count: 20, flying_ticks: 20, left_owner: true, ..Default::default() };
        let (_, report) = tick_trident_with_targets(&mut state, &mut hits, &[breeze], None, &crate::world::WorldSnapshot::new(), release(name));
        report.entity_hits[0].reaction
    };
    assert_eq!(reaction("1.20.2"), ProjectileReaction::Refused);
    assert_eq!(reaction("1.20.3"), ProjectileReaction::Deflected);
    assert_eq!(reaction("1.20.4"), ProjectileReaction::Deflected);
}

/// A ridden horse swimming with Dolphin's Grace slows down by 0.96 from 1.13 (1.13.2 LivingEntity.java:1646-1650), not by its
/// own 0.8F.
#[test]
fn dolphins_grace_on_a_ridden_horse_from_1_13() {
    use crate::ridden::{tick_ridden_mob, RiddenMobKind, RiddenMobState};
    use crate::world::{BlockState, FluidState, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    for x in -4..=4 {
        for y in 98..=104 {
            for z in -4..=4 {
                world.set_block((x, y, z), BlockState { fluid: FluidState::Water, fluid_source: true, ..BlockState::default() });
            }
        }
    }
    let swim = |grace: bool| {
        let mut horse = RiddenMobState::new(RiddenMobKind::Horse, Vec3::new(0.5, 101.0, 0.5));
        horse.velocity = Vec3::new(0.1, 0.0, 0.0);
        horse.yaw = 270.0;
        horse.rider.yaw = 270.0;
        horse.dolphins_grace = grace;
        tick_ridden_mob(&mut horse, &world, release("1.13.2"));
        horse.velocity.x
    };
    assert_eq!(swim(false), 0.1 * 0.98 * f64::from(0.8_f32));
    assert_eq!(swim(true), 0.1 * 0.98 * f64::from(0.96_f32));
}

/// Mth.absMax(double, double): 1.19.4 - 1.21.8 negate what is below 0 and take Math.max, which leaves two negative zeros
/// one (1.21.8 Mth.java:115-125); 1.21.9 takes Math.max of Math.abs, positive zero (1.21.9 Mth.java:130-132).
#[test]
fn abs_max_of_two_negative_zeros_is_positive_from_1_21_9() {
    use crate::math::abs_max_for;
    assert!(abs_max_for(-0.0, -0.0, release("1.21.8")).is_sign_negative());
    assert!(abs_max_for(-0.0, -0.0, release("1.21.9")).is_sign_positive());
}

/// The border term of noCollision for a box that touches the border from inside, asked about a query that does not reach it:
/// 1.17.1 offers the border's shape whenever the entity's box touches it (CollisionSpliterator.java:102-116), and the
/// query's test takes it; 1.18 only where the border's shape meets the query and the entity is inside and close to it
/// (CollisionGetter.java:61-62, 78-81).
#[test]
fn the_border_stops_only_a_query_that_meets_it_from_1_18() {
    let world = bordered();
    let touching = crate::Aabb::new(Vec3::new(4.4, 64.0, 0.0), Vec3::new(5.0 + 1.0e-7, 65.8, 0.6));
    let query = crate::Aabb::new(Vec3::new(4.4, 64.0, 0.0), Vec3::new(4.9, 65.8, 0.6));
    let at = Vec3::new(4.7, 64.0, 0.3);
    assert!(!world.no_border_collision_for_entity(query, at, touching, release("1.17.1")));
    assert!(world.no_border_collision_for_entity(query, at, touching, release("1.18")));
}

/// A block a piston moves east, half its way, in the edge column west of a box and the layer under it, its fence post
/// reaching up and east into the box: 1.11.2 and 1.12.2 getCollisionBoxes walk that cell (Level.java:1120-1124,
/// :1124-1128 leave out only the layer over the box in an edge column), 1.10.2 leaves out the layer under it too
/// (Level.java:1071-1074), and 1.13.2 every cell on two faces of its range (LevelReader.java:130-131).
#[test]
fn a_moving_block_in_an_edge_column_under_the_box_collides_on_1_11_and_1_12_alone() {
    use crate::world::{CollisionQueryEntity, EntityCollisionContext, WorldSnapshot};
    let mut world = WorldSnapshot::new();
    world.set_block((0, 99, 0), crate::world::BlockState::default());
    let post = crate::Aabb::new(Vec3::new(0.875, 0.0, 0.375), Vec3::new(1.125, 1.5, 0.625));
    world.set_moving_block((0, 99, 0), &[post], false);
    let query = crate::Aabb::new(Vec3::new(1.05, 100.4, 0.2), Vec3::new(1.65, 102.2, 0.8));
    let met = |name: &str| !world.legacy_collision_shapes(query, release(name)).is_empty();
    assert!(!met("1.10.2"));
    assert!(met("1.11.2"));
    assert!(met("1.12.2"));
    assert!(!met("1.13.2"));
    // the auto-jump's walk in its order, 1.10.2's and 1.11.2's
    let context = EntityCollisionContext::default();
    let walked = |name: &str| {
        let entity = CollisionQueryEntity { context: &context, bounds: query, position: Vec3::new(1.35, 100.4, 0.5), outside_border: false };
        !world.legacy_collision_boxes_in_order(query, entity, release(name)).is_empty()
    };
    assert!(!walked("1.10.2"));
    assert!(walked("1.11.2"));
}
