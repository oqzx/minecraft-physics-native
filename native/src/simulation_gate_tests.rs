//! B290 fixtures for gates in simulation.rs's private functions: a child module of simulation, so it reaches them. Each asserts
//! the value each side of a boundary computes, as its source says (see gate_fixtures_tests.rs).

use super::*;

fn release(name: &str) -> MinecraftVersion {
    MinecraftVersion::parse(name).expect("a registered release")
}

fn player() -> SimulationState {
    SimulationState { position: Vec3::new(0.5, 64.0, 0.5), half_width: 0.3, height: 1.8, is_player: true, ..SimulationState::default() }
}

/// Entity.remainingFireTicks before a state sets it: -getFireImmuneTicks, 20 for a player, to 1.21.5 (1.21.5 Entity.java:212),
/// the field's 0 from 1.21.6 (1.21.6 Entity.java:234).
#[test]
fn unset_fire_ticks_from_1_21_6() {
    assert_eq!(fire_ticks(&player(), release("1.21.5")), -20);
    assert_eq!(fire_ticks(&player(), release("1.21.6")), 0);
}

/// An invulnerable player's fire ticks are at most 1 from 1.16 (1.16 Player.java:1910); 1.15.2 takes them as they are.
#[test]
fn an_invulnerable_players_fire_from_1_16() {
    let set = |name: &str| {
        let mut state = player();
        state.invulnerable = true;
        set_fire_ticks(&mut state, 100, release(name));
        state.remaining_fire_ticks
    };
    assert_eq!(set("1.15.2"), Some(100));
    assert_eq!(set("1.16"), Some(1));
}

/// An ignition's ticks less 15% a level of Fire Protection before 1.21 (1.20.5 LivingEntity.java:3520-3522), times the
/// BURNING_TIME attribute from 1.21 (1.21 LivingEntity.java:3555-3557).
#[test]
fn an_ignition_by_fire_protection_or_burning_time() {
    let lit = |name: &str| {
        let mut state = player();
        state.remaining_fire_ticks = Some(0);
        state.fire_protection = 2;
        state.burning_time = Some(1.0);
        ignite_for_ticks(&mut state, 160, release(name));
        state.remaining_fire_ticks
    };
    assert_eq!(lit("1.20.6"), Some(160 - (160.0_f32 * (2.0 * 0.15_f32)).floor() as i32));
    assert_eq!(lit("1.21"), Some(160));
}

/// A flight into a wall hurts on the server from 1.9 (1.9.4 LivingEntity.java:1363-1371); 1.8.8 has no such damage.
#[test]
fn a_flight_into_a_wall_hurts_from_1_9() {
    let hit = |name: &str| {
        let mut state = player();
        state.collided_horizontally = true;
        deal_wall_damage(&mut state, 1.0, release(name));
        state.wall_damage
    };
    assert_eq!(hit("1.8.8"), 0.0);
    assert_eq!(hit("1.9.4"), f64::from(7.0_f32));
}

/// updateAutoJump's inverse length: Mth.fastInvSqrt in double before 1.15 (1.14.4 LocalPlayer.java:838), in float until
/// 1.19.4 (1.15 LocalPlayer.java:878, 1.19.3 :937), Mth.invSqrt from it (1.19.4 LocalPlayer.java:919).
#[test]
fn the_auto_jump_inverse_length_by_release() {
    use crate::math::{fast_inv_sqrt, fast_inv_sqrt_f32};
    let value = [2.0_f32, 3.0, 0.7, 5.0, 0.3, 11.0]
        .into_iter()
        .find(|&v| fast_inv_sqrt(f64::from(v)) as f32 != fast_inv_sqrt_f32(v) && fast_inv_sqrt_f32(v) != 1.0 / v.sqrt())
        .expect("a length the three tell apart");
    assert_eq!(auto_jump_inverse_length(value, release("1.14.4")), fast_inv_sqrt(f64::from(value)) as f32);
    assert_eq!(auto_jump_inverse_length(value, release("1.15")), fast_inv_sqrt_f32(value));
    assert_eq!(auto_jump_inverse_length(value, release("1.19.3")), fast_inv_sqrt_f32(value));
    assert_eq!(auto_jump_inverse_length(value, release("1.19.4")), 1.0 / value.sqrt());
}

/// An impulse up is Vec3.add from 1.14, which turns a horizontal -0.0 into +0.0 (1.14.4 LocalPlayer.java:684-698); before, a
/// sum on motionY alone keeps it.
#[test]
fn a_vertical_impulse_clears_a_negative_zero_from_1_14() {
    let pushed = |name: &str| {
        let mut state = player();
        state.velocity = Vec3::new(-0.0, 0.0, -0.0);
        add_vertical_impulse(&mut state, 0.42, release(name));
        state.velocity.x
    };
    assert!(pushed("1.13.2").is_sign_negative());
    assert!(pushed("1.14.4").is_sign_positive());
}

/// A world with a suffocating block in the cell of the player's eyes (65 for one standing at 64), its collision box rising
/// `top` into the cell.
fn wall(top: f64) -> WorldSnapshot {
    use crate::world::{BlockShape, BlockState};
    let mut world = WorldSnapshot::new();
    let shape = BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, top, 1.0)), ..BlockShape::full_block() };
    world.set_block((0, 65, 0), BlockState { id: 1, shape: Some(shape), suffocating: Some(true), in_wall: Some(true), ..BlockState::default() });
    world
}

/// Entity.isInWall: to 1.15.2 the eight corners round the eyes, each in a cell that suffocates whatever its shape (1.15.2
/// Entity.java:1172-1187); from 1.16 a box round the eyes, 0.1F high to 1.16.5 (1.16.5 Entity.java:1639) and 1.0E-6 from
/// 1.17 (1.17 Entity.java:1657), against the block's shape; in 1.18 and 1.18.1 that shape is moved to the eyes, not the cell
/// (1.18 Entity.java:1707), from 1.18.2 to its cell again (1.18.2 Entity.java:1640).
#[test]
fn being_in_a_wall_by_release() {
    let mut state = player();
    state.eye_height = Some(1.62);
    // (the eyes at 65.62: a box top at 65.5 is under the corners' 0.05 and in their cell)
    let low = wall(0.5);
    assert!(is_in_wall(&state, &low, release("1.15.2")));
    assert!(!is_in_wall(&state, &low, release("1.16")));
    // (a box top at 65.6 is within the 0.1F box's 0.05 below the eyes, not within 1.0E-6)
    let high = wall(0.6);
    assert!(is_in_wall(&state, &high, release("1.16.5")));
    assert!(!is_in_wall(&state, &high, release("1.17")));
    // (moved to the eyes, the shape covers them)
    assert!(!is_in_wall(&state, &high, release("1.17.1")));
    assert!(is_in_wall(&state, &high, release("1.18")));
    assert!(!is_in_wall(&state, &high, release("1.18.2")));
}

/// Whether Level.doesBoxCollide marks the entity outside the border: always from 1.13 (1.13.2 LevelReader.java:176-177);
/// before, once its walk of columns meets a loaded chunk (1.12.2 Level.java:1133-1134): the box's own columns in 1.8 (1.8.8
/// Level.java:1047), from 1.9 a column more each way, its corners left out (1.9.4 Level.java:1058, 1073).
#[test]
fn the_outside_border_update_by_release() {
    let swept = Aabb::new(Vec3::new(15.2, 64.0, 0.2), Vec3::new(15.8, 65.8, 0.8));
    let loaded = |chunk: (i32, i32)| {
        let mut world = WorldSnapshot::new();
        world.set_block((chunk.0 * 16, 0, chunk.1 * 16), crate::world::BlockState::default());
        world
    };
    assert!(!updates_outside_border(&WorldSnapshot::new(), swept, release("1.12.2")));
    assert!(updates_outside_border(&WorldSnapshot::new(), swept, release("1.13.2")));
    // (the next chunk east: only 1.9's wider walk reaches it, by a column that is no corner)
    assert!(!updates_outside_border(&loaded((1, 0)), swept, release("1.8.8")));
    assert!(updates_outside_border(&loaded((1, 0)), swept, release("1.9.4")));
    // (the chunk to the north-east: 1.9 reaches it only by a corner, which it leaves out)
    assert!(!updates_outside_border(&loaded((1, -1)), swept, release("1.9.4")));
    // (the box's own chunk: 1.8's one column is a corner of its walk, and counts)
    assert!(updates_outside_border(&loaded((0, 0)), swept, release("1.8.8")));
}

/// A riptide trident let go of in water launches from 1.13 (1.13.2 class_3564.java, TridentItem.releaseUsing); 1.12.2 has
/// no riptide.
#[test]
fn a_riptide_launch_from_1_13() {
    let launched = |name: &str| {
        let mut state = player();
        state.in_water = true;
        let rules = VersionPhysicsRules { release: release(name), ..VersionPhysicsRules::default() };
        let fall = FallDamageRules { landing_damage_rounding: LandingDamageRounding::Ceil, restitution_enabled: false, modern_fall_damage: false };
        launch_riptide(&mut state, &WorldSnapshot::new(), 1, rules, fall);
        state.velocity
    };
    assert_eq!(launched("1.12.2"), Vec3::ZERO);
    assert_ne!(launched("1.13.2"), Vec3::ZERO);
}

/// A piston block that reached 1 is removed on the tick after before 1.11 (1.10.2 PistonMovingBlockEntity.java:167-172); from
/// 1.11 it stays, pushing nothing (1.12.2 PistonMovingBlockEntity.java:279-285).
#[test]
fn an_ended_piston_block_stays_from_1_11() {
    let left = |name: &str| {
        let piston = MovingPiston {
            position: (0, 64, 0),
            facing: 1,
            extending: true,
            source: false,
            progress: 1.0,
            shapes: Vec::new(),
            next_shapes: None,
            outline_shapes: Vec::new(),
            bounces: false,
            sticky_for_entities: false,
        };
        let mut scene = TickScene { moving_pistons: vec![piston], ..TickScene::default() };
        scene.advance(false, release(name));
        scene.moving_pistons.len()
    };
    assert_eq!(left("1.10.2"), 0);
    assert_eq!(left("1.11.2"), 1);
}

/// A weak current: 1.21.11 pushes by any flow longer than 0 (1.21.11 Entity.java:3638-3645), 26.1 by none whose squared length
/// is under 1.0E-5 (26.1 EntityFluidInteraction.java:180-187).
#[test]
fn a_weak_current_pushes_until_26_1() {
    let pushed = |name: &str| {
        let mut state = player();
        let contact = FluidContact { touching: true, height: 0.5, flow: Vec3::new(0.001, 0.0, 0.0), flowing_blocks: 1 };
        push_by_fluid(&mut state, contact, 0.014, release(name));
        state.velocity
    };
    assert_ne!(pushed("1.21.11"), Vec3::ZERO);
    assert_eq!(pushed("26.1"), Vec3::ZERO);
}

/// In lava with a gravity of 0 (the attribute's, from 1.20.5), 1.20.4's gravity step still runs, a Vec3.add whose 0.0 turns a
/// -0.0 into 0.0 (1.20.4 LivingEntity.java:2103); from 1.20.5 it runs only while the gravity is not 0 (1.20.5
/// LivingEntity.java:2164).
#[test]
fn lava_gravity_steps_only_while_not_0_from_1_20_5() {
    let x = |name: &str| {
        let mut state = player();
        state.velocity = Vec3::new(-0.0, 0.0, -0.0);
        let rules = VersionPhysicsRules { release: release(name), ..VersionPhysicsRules::default() };
        apply_fluid_travel_vertical(&mut state, FluidState::Lava, 1.0, 0.4, rules, 0.0, false);
        state.velocity.x
    };
    assert!(x("1.20.4").is_sign_positive());
    assert!(x("1.20.5").is_sign_negative());
}

/// The cells Entity.checkInsideBlocks walks: the box deflated by 1.0E-7, x outermost (1.21.1 Entity.java:1046-1053), from
/// 1.21.2 by 1.0E-5F with x innermost (1.21.2 Entity.java:1093, BlockPos.java:396-401).
#[test]
fn the_cells_inside_by_release() {
    let mut state = player();
    state.position = Vec3::new(0.7 + 5.0e-6, 64.0, 0.5);
    assert!(blocks_inside(&state, release("1.21.1")).contains(&(1, 64, 0)));
    assert!(!blocks_inside(&state, release("1.21.2")).contains(&(1, 64, 0)));
    state.position = Vec3::new(1.0, 64.0, 1.0);
    assert_eq!(blocks_inside(&state, release("1.21.1"))[1], (0, 64, 1));
    assert_eq!(blocks_inside(&state, release("1.21.2"))[1], (1, 64, 0));
}

/// A player 0.0005 into a lava cell, with no cell of the move's own list: to 1.16.5 the area deflated by 0.001 misses it (1.16.5
/// Entity.java:702), from 1.17 the one deflated by 1.0E-6 meets it (1.17 Entity.java:635, LevelReader.java:37-45), until 1.21.2
/// takes the move's list instead (1.21.2 Entity.java:790). Not touching fire, its unset ticks go to -20.
#[test]
fn touching_fire_by_release() {
    use crate::world::BlockState;
    let mut world = WorldSnapshot::new();
    world.set_block((1, 64, 0), BlockState { fluid: FluidState::Lava, ..BlockState::default() });
    let ticks = |inside: &[((i32, i32, i32), bool)], name: &str| {
        let mut state = player();
        state.position = Vec3::new(0.7005, 64.0, 0.5);
        state.remaining_fire_ticks = Some(0);
        apply_inside_fire_effects(&mut state, &world, inside, &InsideEffects::default(), release(name));
        state.remaining_fire_ticks
    };
    assert_eq!(ticks(&[], "1.16.5"), Some(-20));
    assert_eq!(ticks(&[], "1.17"), Some(0));
    assert_eq!(ticks(&[], "1.21.1"), Some(0));
    assert_eq!(ticks(&[], "1.21.2"), Some(-20));
    assert_eq!(ticks(&[((1, 64, 0), false)], "1.21.2"), Some(0));
}

fn solid(world: &mut WorldSnapshot, position: (i32, i32, i32)) {
    use crate::world::{BlockShape, BlockState};
    world.set_block(position, BlockState { id: 1, shape: Some(BlockShape::full_block()), suffocating: Some(true), in_wall: Some(true), ..BlockState::default() });
}

fn sneaker() -> SimulationState {
    SimulationState { sneaking: true, on_ground: true, ..player() }
}

fn backed_off(state: &SimulationState, world: &WorldSnapshot, delta: Vec3, name: &str) -> f64 {
    let context = entity_collision_context(state);
    back_off_from_edge(state, world, &context, delta, 0.6, release(name)).x
}

/// A sneaking move off an edge with a NaN y backs off in 1.18.2, which has no test of y (1.18.2 Player.java:1031); 1.19 - 1.20.4
/// skip it, `movement.y <= 0.0` failing for a NaN (1.19 Player.java:1058).
#[test]
fn a_nan_rise_skips_the_back_off_from_1_19() {
    let mut world = WorldSnapshot::new();
    solid(&mut world, (0, 63, 0));
    let delta = Vec3::new(1.0, f64::NAN, 0.0);
    assert!(backed_off(&sneaker(), &world, delta, "1.18.2") < 1.0);
    assert_eq!(backed_off(&sneaker(), &world, delta, "1.19"), 1.0);
}

/// A fall distance a hair over the float step: as a float to 1.21.4 it reaches the step, so the player is not above the ground
/// and backs off nothing (1.21.4 Player.java:1067); as a double from 1.21.5 it falls short, and with the feet inside a block the
/// rest of the step finds it, so the move backs off (1.21.5 Player.java:1020).
#[test]
fn the_back_off_fall_distance_in_double_from_1_21_5() {
    let mut world = WorldSnapshot::new();
    solid(&mut world, (0, 63, 0));
    let mut state = sneaker();
    state.on_ground = false;
    state.position.y = 63.5;
    state.fall_distance = 0.600_000_01;
    let delta = Vec3::new(1.0, 0.0, 0.0);
    assert_eq!(backed_off(&state, &world, delta, "1.21.4"), 1.0);
    assert!(backed_off(&state, &world, delta, "1.21.5") < 1.0);
}

/// A player in a suffocating block at the world border: 1.16.2 - 1.17.1 ask with the entity, the border's shape suffocates every
/// column and no side is open (1.16.5 LocalPlayer.java:433-437); 1.16.1 reads the cells (1.16 LocalPlayer.java:404-462) and
/// 1.18 asks canCollide (1.18 LocalPlayer.java:443-446), both of which find a side.
#[test]
fn moving_towards_a_space_at_the_border() {
    let mut world = WorldSnapshot::new();
    world.set_world_bounds(0, 256, 0.0, 0.0, 10.0);
    solid(&mut world, (4, 64, 0));
    solid(&mut world, (4, 65, 0));
    let pushed = |name: &str| {
        let mut state = player();
        // (its box ends 5e-8 past the border at 5: shrunk by 1e-7 it is inside, so it touches the border from inside)
        state.position = Vec3::new(4.7 + 5.0e-8, 64.0, 0.5);
        move_towards_closest_space(&mut state, &world, release(name));
        state.velocity
    };
    assert_ne!(pushed("1.16.1"), Vec3::ZERO);
    assert_eq!(pushed("1.16.2"), Vec3::ZERO);
    assert_eq!(pushed("1.17.1"), Vec3::ZERO);
    assert_ne!(pushed("1.18"), Vec3::ZERO);
}

/// A swimming player with its feet cell open and the cell above suffocating: before 1.13 the cell is blocked unless both are open
/// (1.12.2 LocalPlayer.java:390); from 1.13 a swimmer tests its cell alone (1.13.2 LocalPlayer.java:403, 448).
#[test]
fn a_swimmer_tests_its_cell_alone_from_1_13() {
    let mut world = WorldSnapshot::new();
    solid(&mut world, (0, 65, 0));
    let pushed = |name: &str| {
        let mut state = player();
        state.swimming = true;
        push_out_of_blocks_before_1_14(&mut state, &world, release(name));
        state.velocity
    };
    assert_ne!(pushed("1.12.2"), Vec3::ZERO);
    assert_eq!(pushed("1.13.2"), Vec3::ZERO);
}

fn fall_rules() -> FallDamageRules {
    FallDamageRules { landing_damage_rounding: LandingDamageRounding::Ceil, restitution_enabled: false, modern_fall_damage: false }
}

/// A move on soul sand ends multiplied by the block's speed factor from 1.15 (1.15 Entity.java:634); 1.14.4 has no speed
/// factor there (1.14.4 Entity.java:569), soul sand slowing only what is inside it.
#[test]
fn a_move_ends_by_the_speed_factor_from_1_15() {
    use crate::world::{BlockEffect, BlockShape, BlockState};
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), BlockState { id: 1, shape: Some(BlockShape { effect: BlockEffect::SoulSand, ..BlockShape::full_block() }), ..BlockState::default() });
    let moved = |name: &str| {
        let mut state = player();
        state.position.y = 1.0;
        state.on_ground = true;
        state.velocity = Vec3::new(0.1, 0.0, 0.0);
        let rules = VersionPhysicsRules { release: release(name), ..VersionPhysicsRules::default() };
        entity_move(&mut state, &world, Vec3::new(0.1, 0.0, 0.0), MoverType::SelfOrPlayer, rules, fall_rules(), None);
        state.velocity.x
    };
    assert_eq!(moved("1.14.4"), 0.1);
    assert_eq!(moved("1.15"), 0.1 * f64::from(0.4_f32));
}

fn ticked(mut state: SimulationState, world: &WorldSnapshot, name: &str) -> SimulationState {
    let rules = VersionPhysicsRules { release: release(name), ..VersionPhysicsRules::default() };
    tick_with_rules(&mut state, world, PhysicsConfig::default(), rules);
    state
}

/// A player in water touching a flowing lava: 1.16 and 1.16.1 return from updateWaterState in water before they measure the
/// lava (1.16 Entity.java:1039-1048), so its current pushes nothing; 1.16.2 always measures it (1.16.5 Entity.java:1043-1049).
#[test]
fn lava_in_water_is_measured_from_1_16_2() {
    use crate::world::BlockState;
    let water = BlockState { fluid: FluidState::Water, fluid_source: true, water_block: true, ..BlockState::default() };
    let lava = BlockState { fluid: FluidState::Lava, fluid_level: 2, fluid_flow: Vec3::new(1.0, 0.0, 0.0), ..BlockState::default() };
    let mut only_water = WorldSnapshot::new();
    only_water.set_block((0, 64, 0), water);
    let mut both = only_water.clone();
    both.set_block((0, 65, 0), lava);
    let velocity = |world: &WorldSnapshot, name: &str| ticked(player(), world, name).velocity;
    assert_eq!(velocity(&both, "1.16"), velocity(&only_water, "1.16"));
    assert_ne!(velocity(&both, "1.16.2"), velocity(&only_water, "1.16.2"));
}

/// A slow falling player rising into a ceiling: from 1.21.2 the gravity takes the delta the move left, 0, so slow falling
/// applies (1.21.2 LivingEntity.java:2194-2197, 2383-2393); 1.21.1 takes the delta it started with, rising, so the full
/// gravity (1.21 LivingEntity.java:2115-2121).
#[test]
fn slow_falling_tests_the_moved_delta_from_1_21_2() {
    let mut world = WorldSnapshot::new();
    solid(&mut world, (0, 66, 0));
    let mut state = player();
    state.slow_falling = true;
    state.velocity = Vec3::new(0.0, 0.3, 0.0);
    assert!(ticked(state.clone(), &world, "1.21.1").velocity.y < -0.05);
    assert!(ticked(state, &world, "1.21.2").velocity.y > -0.02);
}

/// A riptide spin attack that ran into a wall ends after the travel from 1.13 (1.13.2 LivingEntity.java:1955-1963).
#[test]
fn a_spin_attack_into_a_wall_ends_from_1_13() {
    let mut world = WorldSnapshot::new();
    let mut state = player();
    let outcome = move_local_player(&mut world, &mut state, None, &[], (0, 0), true, release("1.13.2"), |_, _| Ok(()), |state, _| {
        state.collided_horizontally = true;
        Ok(TickResult::default())
    })
    .expect("a tick");
    assert!(outcome.auto_spin_attack_ended);
}

/// A boat passenger whose boat's box reaches over its own top but starts above its eyes: 26.1 takes no fluid interaction box
/// when the boat's top is at or over the entity's (26.1 Entity.getFluidInteractionBox, EntityFluidInteraction.java:71), so its
/// eyes are in nothing; 1.21.11 tests whether the eyes are within the boat's height (1.21.11 Entity.java:1602), and they are not,
/// so the water at them counts.
#[test]
fn a_passengers_eyes_by_the_fluid_interaction_box_from_26_1() {
    use crate::world::BlockState;
    let water = BlockState { fluid: FluidState::Water, fluid_source: true, water_block: true, ..BlockState::default() };
    // (in the middle of a chunk: 26.1 scans only where the chunks one block round the box are loaded,
    // EntityFluidInteraction.java:44 hasFluidAndLoaded)
    let mut world = WorldSnapshot::new();
    world.set_block((8, 63, 8), water);
    world.set_block((8, 64, 8), water);
    let mut state = player();
    state.position = Vec3::new(8.5, 63.0, 8.5);
    state.eye_height = Some(1.62);
    state.passenger = true;
    state.boat_vehicle = Some(BoatVehicle { bounding_box: Aabb::new(Vec3::new(8.0, 64.7, 8.0), Vec3::new(9.0, 66.0, 9.0)), underwater: false });
    assert!(entity_eyes_in_fluid(&state, &world, FluidState::Water, release("1.21.11")));
    assert!(!entity_eyes_in_fluid(&state, &world, FluidState::Water, release("26.1")));
}

fn moved_by(state: &mut SimulationState, world: &WorldSnapshot, movement: Vec3, name: &str) -> Vec3 {
    let rules = VersionPhysicsRules { release: release(name), ..VersionPhysicsRules::default() };
    entity_move(state, world, movement, MoverType::SelfOrPlayer, rules, fall_rules(), None).displacement
}

/// A jump onto a step 0.125 high with a ceiling 0.2 over the head there: 1.20.6 lifts the move by the step height as far as the
/// ceiling lets it and lowers it back (1.20.6 Entity.java:916-919), keeping more than 0.19 of the rise; 1.21 lifts it to the
/// lowest face that clears the way, the step's top (1.21 Entity.java:917-926).
#[test]
fn a_step_under_a_ceiling_by_release() {
    use crate::world::{BlockShape, BlockState};
    let mut world = WorldSnapshot::new();
    solid(&mut world, (0, 63, 0));
    solid(&mut world, (1, 63, 0));
    let step = BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.125, 1.0)), ..BlockShape::full_block() };
    world.set_block((1, 64, 0), BlockState { id: 1, shape: Some(step), ..BlockState::default() });
    // (the ceiling's bottom 0.2 over the head of a player on the step: 64.125 + 1.8 + 0.2)
    let ceiling = BlockShape { bounds: Aabb::new(Vec3::new(0.0, 0.125, 0.0), Vec3::new(1.0, 1.0, 1.0)), ..BlockShape::full_block() };
    world.set_block((1, 66, 0), BlockState { id: 1, shape: Some(ceiling), ..BlockState::default() });
    let rise = |name: &str| {
        let mut state = player();
        state.position.x = 0.7 - 1.0e-9;
        state.on_ground = true;
        moved_by(&mut state, &world, Vec3::new(0.3, 0.42, 0.0), name).y
    };
    assert!(rise("1.20.6") > 0.19);
    assert_eq!(rise("1.21"), 0.125);
}

/// A flying player sneaking in water: 1.15.2 also sinks it by 0.04 (1.15.2 LocalPlayer.java aiStep, handleFluidSneak whatever
/// the flight), 1.16 only where fluids affect it, which flight stops (1.16 LocalPlayer.java, isAffectedByFluids). Sneaking
/// lowers both by the flight's own descent; the 0.04 is the rest of the difference.
#[test]
fn a_flying_sneaker_sinks_in_water_until_1_16() {
    use crate::world::BlockState;
    let water = BlockState { fluid: FluidState::Water, fluid_source: true, water_block: true, ..BlockState::default() };
    let mut world = WorldSnapshot::new();
    for y in 63..=66 {
        world.set_block((0, y, 0), water);
    }
    let sneak_drop = |name: &str| {
        let fall = |sneaking: bool| {
            let mut state = player();
            state.flying = true;
            state.sneaking = sneaking;
            ticked(state, &world, name).velocity.y
        };
        fall(false) - fall(true)
    };
    assert!(sneak_drop("1.15.2") - sneak_drop("1.16") > 0.02);
}

/// A fall onto a block whose collision box has no height: before 1.13 the legacy boxes keep it and it stops the fall (1.12.2
/// Level.java:1161-1184, Entity.java:672-766); from 1.13 the voxel shapes take it as empty (1.13.2 LevelReader.java:123).
#[test]
fn a_flat_box_stops_a_fall_before_1_13() {
    use crate::world::{BlockShape, BlockState};
    let mut world = WorldSnapshot::new();
    let flat = BlockShape { bounds: Aabb::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 1.0)), ..BlockShape::full_block() };
    world.set_block((0, 63, 0), BlockState { id: 1, shape: Some(flat), ..BlockState::default() });
    let fall = |name: &str| {
        let mut state = player();
        state.position.y = 64.5;
        moved_by(&mut state, &world, Vec3::new(0.0, -1.0, 0.0), name).y
    };
    assert_eq!(fall("1.12.2"), -0.5);
    assert_eq!(fall("1.13.2"), -1.0);
}

/// A move along x alone that ends 5e-8 short of a lone block: 1.17.1 walks the cells for a move on one axis and drops a
/// movement a block leaves under 1.0E-7 at once (1.17.1 Entity.java:787, Shapes.java:223-283); 1.18 collides the shapes one
/// by one, dropping such a movement only before the next shape, and the block is the last, so the 5e-8 stays (1.18
/// Entity.java:794, Shapes.java:175-185).
#[test]
fn a_single_axis_walk_drops_what_a_lone_block_leaves_until_1_18() {
    let mut world = WorldSnapshot::new();
    solid(&mut world, (1, 64, 0));
    let pushed = |name: &str| {
        let mut state = player();
        state.position = Vec3::new(0.7 - 5.0e-8, 64.0, 0.5);
        moved_by(&mut state, &world, Vec3::new(0.5, 0.0, 0.0), name).x
    };
    assert_eq!(pushed("1.17.1"), 0.0);
    let kept = pushed("1.18");
    assert!(kept > 0.0 && kept < 1.0e-7, "{kept}");
}

/// A move of 5e-8 on x beside a cube the box touches on z: 1.13.2 gathers the shapes meeting the swept box grown by 1.0E-7
/// (1.13.2 LevelReader.java:161-166), where its merger keeps the 1.0E-7 apart (class_4092.java:34-44, `d > e - 1.0E-7`), so
/// the cube is among them, and Shapes.collide then drops a movement under 1.0E-7 before it (Shapes.java:200-212). The
/// legacy list of 1.12.2 holds what meets the box itself, which the cube does not, and leaves the 5e-8 to the move.
#[test]
fn a_touching_cube_drops_a_tiny_move_from_1_13() {
    let mut world = WorldSnapshot::new();
    solid(&mut world, (0, 63, 0));
    let mut state = player();
    state.position = Vec3::new(0.7000008999999999, 63.0, 1.3);
    state.on_ground = true;
    assert_eq!(moved_by(&mut state, &world, Vec3::new(5.0e-8, 0.6, 0.0), "1.13.2").x, 0.0);
}

/// A dead entity is inside no block for Entity.checkInsideBlocks from 1.20.2 (1.20.2 Entity.java:982-984, 1.21.6 :1154-1156);
/// 1.20.1 walks it like a live one.
#[test]
fn a_dead_entity_is_inside_no_block_from_1_20_2() {
    let mut state = player();
    state.dead = true;
    assert!(!blocks_inside(&state, release("1.20.1")).is_empty());
    assert!(blocks_inside(&state, release("1.20.2")).is_empty());
    let world = WorldSnapshot::new();
    let movements = [Movement { from: state.position, to: state.position, axis_order: None }];
    assert!(blocks_inside_moves(&state, &world, &movements, release("26.3")).0.is_empty());
    state.dead = false;
    assert!(!blocks_inside(&state, release("1.20.2")).is_empty());
}

/// NoGravity keeps a water or lava sink only from 1.10: 1.9.4 takes the 0.02 off whatever the flag (1.9.4
/// LivingEntity.java:1444), 1.10.2 tests isNoGravity first.
#[test]
fn no_gravity_keeps_the_fluid_sink_from_1_10() {
    let sunk = |name: &str| {
        let mut state = player();
        state.no_gravity = true;
        crate::legacy_fluid::sink_in_water(&mut state, release(name));
        state.velocity.y
    };
    assert_eq!(sunk("1.9.4"), -0.02);
    assert_eq!(sunk("1.10.2"), 0.0);
}

/// The attribute readers take the state's value, else the attribute's default: SNEAKING_SPEED 0.3, a modifier 1.0
/// (Attributes.SNEAKING_SPEED, AIR_DRAG_MODIFIER, FRICTION_MODIFIER).
#[test]
fn an_unset_attribute_reads_its_default() {
    let mut state = player();
    assert_eq!(sneaking_speed(&state), 0.3);
    state.sneaking_speed = Some(0.6);
    assert_eq!(sneaking_speed(&state), 0.6_f32);
    assert_eq!(effective_modifier(0.0, false), 1.0);
    assert_eq!(effective_modifier(0.0, true), 0.0);
    assert_eq!(effective_modifier(0.5, false), 0.5);
}

/// release_riptide, the engine's wrapper of TridentItem.releaseUsing's launch (launch_riptide, which cites it): it takes the
/// pending launch, keeps what the lift moved and the Movement it recorded, and with nothing pending clears them; its
/// with-profile form runs it with a profile's rules and marks the release done for the tick (helpers-close-c.json).
#[test]
fn the_riptide_release_takes_the_launch_and_keeps_its_lift() {
    let world = WorldSnapshot::new();
    let rules = <crate::versioned::v1_20_4::Profile as VersionPhysicsProfile>::rules();
    let launched = |on_ground| {
        let mut state = SimulationState { in_water: true, on_ground, riptide_launch: Some(3), pitch: 0.0, yaw: 0.0, ..player() };
        crate::simulation::release_riptide(&mut state, &world, rules, fall_rules());
        state
    };
    let flying = launched(false);
    assert_eq!(flying.riptide_launch, None);
    assert!(flying.velocity.z > 2.9, "pushed along its look: {:?}", flying.velocity);
    assert_eq!((flying.riptide_lift, flying.riptide_lift_movement), (Vec3::ZERO, None));
    let grounded = launched(true);
    assert!((grounded.riptide_lift.y - 1.1999999284744263).abs() < 1e-9, "{:?}", grounded.riptide_lift);
    // nothing pending: a lift left from before is cleared
    let mut idle = SimulationState { riptide_lift: Vec3::new(0.0, 1.0, 0.0), ..player() };
    crate::simulation::release_riptide(&mut idle, &world, rules, fall_rules());
    assert_eq!((idle.riptide_lift, idle.velocity), (Vec3::ZERO, Vec3::ZERO));

    let mut profiled = SimulationState { in_water: true, riptide_launch: Some(3), ..player() };
    crate::simulation::release_riptide_with_profile::<crate::versioned::v1_20_4::Profile>(&mut profiled, &world, PhysicsConfig::default());
    assert!(profiled.riptide_released);
    assert_eq!(profiled.velocity, flying.velocity);
}
