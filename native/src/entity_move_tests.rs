//! Entity.move (B083): the shared move of the player's body (`entity_move`, which the tick, a
//! riptide lift, a piston and a shulker box take) and the parts every other entity's move shares
//! with it (`reset_fall_on_clip`, `move_without_physics`, `refreshes_vertical_collision`), each
//! against the vanilla line it ports. docs/entity-move-phases.md lists the items by id.

use crate::math::{Aabb, Vec3};
use crate::piston::move_entity;
use crate::simulation::{
    back_off_from_edge, block_fall_callback_runs, entity_collision_context, entity_move,
    move_without_physics, recorded_movement, refreshes_vertical_collision,
    FallDamageRules, LandingDamageRounding, MoverType, PhysicsConfig, SimulationState,
    VersionPhysicsRules,
};
use crate::vehicle::{move_vehicle, MoveRules};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, BlockShape, BlockState, WorldSnapshot};

fn release(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
    MinecraftVersion::new(major, minor, patch)
}

fn rules(release: MinecraftVersion) -> VersionPhysicsRules {
    VersionPhysicsRules {
        release,
        ..VersionPhysicsRules::default()
    }
}

fn fall_rules(release: MinecraftVersion) -> FallDamageRules {
    FallDamageRules {
        landing_damage_rounding: LandingDamageRounding::Ceil,
        restitution_enabled: release.at_least(26, 2, 0),
        modern_fall_damage: release.at_least(1, 20, 5),
    }
}

fn solid() -> BlockState {
    BlockState {
        shape: Some(BlockShape::full_block()),
        ..BlockState::default()
    }
}

fn effect_block(effect: BlockEffect, bounds: Aabb) -> BlockState {
    BlockState {
        shape: Some(BlockShape {
            bounds,
            effect,
            ..BlockShape::full_block()
        }),
        ..BlockState::default()
    }
}

/// A floor at y = 0 from x = -4 to 4, z = -4 to 4.
fn floor() -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    for x in -4..=4 {
        for z in -4..=4 {
            world.set_block((x, 0, z), solid());
        }
    }
    world
}

fn player_at(position: Vec3) -> SimulationState {
    SimulationState {
        position,
        half_width: 0.3,
        height: 1.8,
        is_player: true,
        controlled_by_local_instance: true,
        client_side: true,
        ..SimulationState::default()
    }
}

/// D1: a shulker box's push takes no edge back-off; SELF and PLAYER moves do (1.11.2
/// Entity.java:614 adds SELF or PLAYER; 26.3 Player.java:831-833).
#[test]
fn a_shulker_box_push_does_not_back_off_from_an_edge() {
    let world = floor();
    for (major, minor, patch) in [(1, 12, 2), (1, 16, 5), (1, 21, 11), (26, 3, 0)] {
        let release = release(major, minor, patch);
        let start = player_at(Vec3::new(4.5, 1.0, 0.5));
        let push = Vec3::new(1.0, 0.0, 0.0);
        let mut sneaking = SimulationState {
            on_ground: true,
            sneaking: true,
            ..start
        };
        let mut pushed = sneaking;
        move_entity(&mut sneaking, &world, push, PhysicsConfig::default(), release, MoverType::SelfOrPlayer);
        move_entity(&mut pushed, &world, push, PhysicsConfig::default(), release, MoverType::ShulkerBox);
        assert!(sneaking.position.x < 5.3, "{release:?}: SELF backs off");
        assert_eq!(pushed.position.x, 5.5, "{release:?}: SHULKER_BOX does not");
    }
}

/// D2: the fall-reset clip comes to a piston's move and to every other entity's (1.19
/// Entity.java:570-580, 26.3 :776-784); 1.18.1 has none (1.18.1 Entity.java:595-598).
#[test]
fn every_move_of_a_block_or_more_resets_a_fall_through_a_resetting_block() {
    let mut world = WorldSnapshot::new();
    world.set_block(
        (0, 5, 0),
        BlockState {
            fall_damage_resetting: true,
            ..BlockState::default()
        },
    );
    let start = Vec3::new(0.5, 6.2, 0.5);
    let drop = Vec3::new(0.0, -1.5, 0.0);
    for (major, minor, patch, resets) in [(1, 18, 1, false), (1, 18, 2, true), (1, 21, 9, true), (26, 3, 0, true)] {
        let release = release(major, minor, patch);
        let mut player = SimulationState {
            fall_distance: 4.0,
            ..player_at(start)
        };
        move_entity(&mut player, &world, drop, PhysicsConfig::default(), release, MoverType::Piston);
        let expected = if resets { 1.5 } else { 5.5 };
        assert_eq!(player.fall_distance as f32, expected, "{release:?}: piston");

        let mut item = SimulationState {
            position: start,
            half_width: 0.125,
            height: 0.25,
            fall_distance: 4.0,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        let counted = MoveRules {
            counts_fall: true,
            ..MoveRules::MINECART
        };
        move_vehicle(&mut item, &world, release, drop, counted, |_, _| {});
        assert_eq!(item.fall_distance as f32, expected, "{release:?}: another entity");
    }
}

/// D3: from 26.2 the restitution is the instance's that simulates the entity, in the tick as in a
/// piston's move (26.2 Entity.java:815, 26.3 :820).
#[test]
fn the_tick_restitutes_only_where_the_instance_simulates() {
    let mut world = WorldSnapshot::new();
    world.set_block((1, 0, 0), solid());
    let start = SimulationState {
        position: Vec3::new(0.5, 0.0, 0.5),
        velocity: Vec3::new(1.0, 0.0, 0.0),
        half_width: 0.3,
        height: 1.8,
        entity_bounciness: 0.5,
        no_gravity: true,
        controlled_by_local_instance: true,
        ..SimulationState::default()
    };
    let mut simulated = start;
    crate::versioned::dispatch("26.2", &mut simulated, &world, PhysicsConfig::default()).unwrap();
    assert!(simulated.velocity.x < 0.0, "bounces back where it simulates");
    let mut remote = SimulationState {
        client_side: true,
        controlled_by_local_instance: false,
        ..start
    };
    crate::versioned::dispatch("26.2", &mut remote, &world, PhysicsConfig::default()).unwrap();
    assert!(remote.collided_horizontally);
    assert!(remote.velocity.x >= 0.0, "a client that does not simulate it leaves the motion");
}

/// D4: the restitution reads the field verticalCollision, which a level move of an entity its
/// instance does not control keeps from 1.21.4 (26.2 Entity.java:846, 26.3 :857).
#[test]
fn restitution_reads_the_kept_vertical_collision() {
    let mut world = WorldSnapshot::new();
    world.set_block((1, 0, 0), solid());
    let release = release(26, 3, 0);
    // a server's player: it simulates (Player.canSimulateMovement) but does not control itself
    let mut state = SimulationState {
        position: Vec3::new(0.5, 0.0, 0.5),
        velocity: Vec3::new(0.5, -0.3, 0.0),
        half_width: 0.3,
        height: 1.8,
        is_player: true,
        collided_vertically: true,
        vertical_collision_below: true,
        on_ground: true,
        ..SimulationState::default()
    };
    let moved = entity_move(
        &mut state,
        &world,
        Vec3::new(0.5, 0.0, 0.0),
        MoverType::Piston,
        rules(release),
        fall_rules(release),
        None,
    );
    assert!(moved.fall.is_some());
    assert!(state.collided_vertically, "the level move keeps the flag");
    // verticalCollisionBelow kept: no bounce off air, so the y motion goes to 0 * restitution
    assert_eq!(state.velocity.y, 0.0);
    assert_eq!(state.velocity.x, 0.0);
}

/// D5, D6: `Math.abs(delta.y) > 0.0` is false for NaN, so a NaN move keeps the vertical flags of
/// an entity its instance does not control, and from 26.2 runs no fall callback (1.21.4
/// Entity.java:706, 26.2 :798).
#[test]
fn a_nan_vertical_move_refreshes_nothing() {
    let remote = SimulationState {
        client_side: true,
        ..SimulationState::default()
    };
    assert!(!refreshes_vertical_collision(&remote, f64::NAN, release(1, 21, 4)));
    assert!(refreshes_vertical_collision(&remote, f64::NAN, release(1, 21, 3)));
    assert!(refreshes_vertical_collision(&remote, -0.1, release(1, 21, 4)));
    let local = SimulationState {
        controlled_by_local_instance: true,
        ..SimulationState::default()
    };
    assert!(refreshes_vertical_collision(&local, f64::NAN, release(26, 3, 0)));
    assert!(!block_fall_callback_runs(&local, true, false, f64::NAN, release(26, 2, 0)));
    assert!(block_fall_callback_runs(&local, true, false, -0.1, release(26, 2, 0)));
}

/// D6: any entity's move, not only a piston's body, keeps the vertical flags, the ground and the
/// support on a level move of what its instance does not control from 1.21.4 (1.21.4
/// Entity.java:706-710, 26.3 :798-803).
#[test]
fn another_entitys_level_move_keeps_its_ground_where_it_does_not_control_it() {
    let world = floor();
    for (major, minor, patch, kept) in [(1, 21, 3, false), (1, 21, 4, true), (26, 3, 0, true)] {
        let release = release(major, minor, patch);
        let mut item = SimulationState {
            position: Vec3::new(0.5, 3.0, 0.5),
            half_width: 0.125,
            height: 0.25,
            on_ground: true,
            collided_vertically: true,
            vertical_collision_below: true,
            client_side: true,
            ..SimulationState::default()
        };
        move_vehicle(&mut item, &world, release, Vec3::new(0.1, 0.0, 0.0), MoveRules::MINECART, |_, _| {});
        assert_eq!(item.on_ground, kept, "{release:?}");
        assert_eq!(item.collided_vertically, kept, "{release:?}");
    }
}

/// D7: every noPhysics move clears the four collision flags from 1.21.9 (1.21.8 Entity.java:706-708,
/// 1.21.9 :713-720), and moves the box first where the entity keeps one (1.16.5 Entity.java:607-609).
#[test]
fn a_move_without_physics_clears_the_flags_from_1_21_9() {
    for (major, minor, patch, cleared) in [(1, 21, 8, false), (1, 21, 9, true)] {
        let release = release(major, minor, patch);
        let mut state = SimulationState {
            collided_horizontally: true,
            minor_horizontal_collision: true,
            collided_vertically: true,
            vertical_collision_below: true,
            no_physics: true,
            ..SimulationState::default()
        };
        move_without_physics(&mut state, Vec3::new(0.0, 1.0, 0.0), release);
        assert_eq!(state.position.y, 1.0);
        assert_eq!(state.collided_horizontally, !cleared);
        assert_eq!(state.minor_horizontal_collision, !cleared);
        assert_eq!(state.collided_vertically, !cleared);
        assert_eq!(state.vertical_collision_below, !cleared);
    }
    let bounds = Aabb::new(Vec3::new(-0.3, 0.0, -0.3), Vec3::new(0.3, 1.8, 0.3));
    let mut boxed = SimulationState {
        bounding_box: Some(bounds),
        half_width: 0.3,
        height: 1.8,
        no_physics: true,
        ..SimulationState::default()
    };
    move_without_physics(&mut boxed, Vec3::new(0.25, 0.0, 0.0), release(1, 16, 5));
    assert_eq!(boxed.bounding_box, Some(bounds.translated(Vec3::new(0.25, 0.0, 0.0))));
    assert_eq!(boxed.position.x, 0.25);
}

/// D8, D32: from 1.17 a removed entity's move stops after checkFallDamage: the fall ends on the
/// slime block, the bounce, the motion stop and the speed factor do not come (1.17
/// Entity.java:567-582, 1.21.11 :793-807, 26.3 :817).
#[test]
fn a_removed_entity_lands_without_the_bounce_or_the_speed_factor() {
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), effect_block(BlockEffect::Slime, Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))));
    for (major, minor, patch, exits) in [(1, 16, 5, false), (1, 17, 0, true), (1, 21, 11, true)] {
        let release = release(major, minor, patch);
        let mut state = SimulationState {
            velocity: Vec3::new(0.2, -0.5, 0.0),
            fall_distance: 3.0,
            removed: true,
            ..player_at(Vec3::new(0.5, 1.2, 0.5))
        };
        let velocity = state.velocity;
        let moved = entity_move(
            &mut state,
            &world,
            velocity,
            MoverType::SelfOrPlayer,
            rules(release),
            fall_rules(release),
            None,
        );
        assert_eq!(moved.removed, exits, "{release:?}");
        assert!(state.on_ground, "{release:?}");
        assert_eq!(state.fall_distance, 0.0, "{release:?}: the fall ends either way");
        if exits {
            assert_eq!(state.velocity, Vec3::new(0.2, -0.5, 0.0), "{release:?}");
        } else {
            assert_eq!(state.velocity.y, 0.5, "{release:?}: the slime block bounces");
        }
    }
}

/// D14: before 1.17 a piston's move is an Entity.move whose fire test raises the fire ticks of
/// what is in fire, as the tick's own (1.12.2 Entity.java:876-886, 1.15.2 :631-653).
#[test]
fn a_piston_move_into_fire_raises_the_fire_ticks_before_1_17() {
    let mut world = floor();
    world.set_block((1, 1, 0), effect_block(BlockEffect::Fire, Aabb::new(Vec3::ZERO, Vec3::ZERO)));
    for (major, minor, patch) in [(1, 12, 2), (1, 15, 2), (1, 16, 5)] {
        let release = release(major, minor, patch);
        let mut state = SimulationState {
            remaining_fire_ticks: Some(-20),
            ..player_at(Vec3::new(1.2, 1.0, 0.5))
        };
        move_entity(&mut state, &world, Vec3::new(0.3, 0.0, 0.0), PhysicsConfig::default(), release, MoverType::Piston);
        assert!(state.remaining_fire_ticks.unwrap() > -20, "{release:?}: {:?}", state.remaining_fire_ticks);
    }
}

/// D15: only LocalPlayer overrides isHorizontalCollisionMinor (1.18 LocalPlayer.java; Entity's is
/// false, 26.3 Entity.java:805-809): a server's player and another client's keep false.
#[test]
fn only_the_local_player_has_a_minor_horizontal_collision() {
    let mut world = WorldSnapshot::new();
    world.set_block((1, 0, 0), solid());
    world.set_block((1, 1, 0), solid());
    let release = release(1, 20, 4);
    let local = SimulationState {
        position: Vec3::new(0.5, 0.0, 0.5),
        input: Vec3::new(1.0, 0.0, 0.01),
        minor_horizontal_collision_supported: true,
        ..player_at(Vec3::new(0.5, 0.0, 0.5))
    };
    for (client_side, controlled, minor) in [(true, true, true), (false, false, false), (true, false, false)] {
        let mut state = SimulationState {
            client_side,
            controlled_by_local_instance: controlled,
            ..local
        };
        entity_move(&mut state, &world, Vec3::new(0.5, 0.0, 0.005), MoverType::SelfOrPlayer, rules(release), fall_rules(release), None);
        assert!(state.collided_horizontally);
        assert_eq!(state.minor_horizontal_collision, minor, "{client_side} {controlled}");
    }
}

/// D28: 1.8.8 ends a landed fall only when its distance is above 0 (1.8.8 Entity.java:790-800);
/// from 1.9 any landing does (1.9.4 Entity.java:889-895).
#[test]
fn a_negative_fall_distance_outlives_a_landing_before_1_9() {
    let world = floor();
    for (major, minor, patch, after) in [(1, 8, 8, -2.0), (1, 9, 4, 0.0)] {
        let release = release(major, minor, patch);
        let mut state = SimulationState {
            fall_distance: -2.0,
            ..player_at(Vec3::new(0.5, 1.05, 0.5))
        };
        entity_move(&mut state, &world, Vec3::new(0.0, -0.1, 0.0), MoverType::SelfOrPlayer, rules(release), fall_rules(release), None);
        assert!(state.on_ground);
        assert_eq!(state.fall_distance, after, "{release:?}");
    }
}

/// D33: an entity's fall is counted in its move before the blocks inside, whose cobweb ends it
/// (1.20.4 Entity.java:757-764): the fall left is 0, not the drop.
#[test]
fn a_cobweb_ends_the_fall_the_move_counted_first() {
    let mut world = WorldSnapshot::new();
    world.set_block((0, 4, 0), effect_block(BlockEffect::Web, Aabb::new(Vec3::ZERO, Vec3::ZERO)));
    for (major, minor, patch) in [(1, 12, 2), (1, 16, 5), (1, 20, 4)] {
        let release = release(major, minor, patch);
        let mut item = SimulationState {
            position: Vec3::new(0.5, 4.6, 0.5),
            half_width: 0.125,
            height: 0.25,
            fall_distance: 2.0,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        let counted = MoveRules {
            counts_fall: true,
            ..MoveRules::MINECART
        };
        move_vehicle(&mut item, &world, release, Vec3::new(0.0, -0.4, 0.0), counted, |body, support| {
            crate::vehicle::apply_effects_inside(body, &world, support, release);
        });
        assert_eq!(item.fall_distance, 0.0, "{release:?}");
    }
}

/// D34: 1.19 - 1.20.4 back off only for `movement.y <= 0.0`, which NaN fails; from 1.20.5 for
/// `!(movement.y > 0.0)`, which it passes (1.20.4 Player.java:1047, 1.20.5 :1029).
#[test]
fn a_nan_vertical_move_backs_off_from_1_20_5_only() {
    let world = floor();
    let state = SimulationState {
        on_ground: true,
        sneaking: true,
        ..player_at(Vec3::new(4.5, 1.0, 0.5))
    };
    let context = entity_collision_context(&state);
    let delta = Vec3::new(1.0, f64::NAN, 0.0);
    let before = back_off_from_edge(&state, &world, &context, delta, 0.6, release(1, 20, 4));
    let after = back_off_from_edge(&state, &world, &context, delta, 0.6, release(1, 20, 5));
    assert_eq!(before.x, 1.0);
    assert!(after.x < 0.8, "{}", after.x);
}

/// D17: a move of 1.21.5+ that moved or was not cut short is one Movement of movementThisTick, its
/// axis order from the requested movement from 1.21.9 (1.21.5 Entity.java:685, 1.21.9 :750, 26.3
/// :788); the tick puts a riptide lift's first.
#[test]
fn a_recorded_move_is_one_movement_from_1_21_5() {
    let state = SimulationState::default();
    let from = Vec3::new(0.5, 1.0, 0.5);
    let requested = Vec3::new(0.0, 1.2, 0.0);
    let moved = Vec3::new(0.0, 0.8, 0.0);
    assert!(recorded_movement(&state, from, requested, moved, release(1, 21, 4)).is_none());
    let movement = recorded_movement(&state, from, requested, moved, release(1, 21, 5)).unwrap();
    assert_eq!((movement.from, movement.to, movement.axis_order), (from, from + moved, Some(moved)));
    let movement = recorded_movement(&state, from, requested, moved, release(26, 3, 0)).unwrap();
    assert_eq!(movement.axis_order, Some(requested));
    // nothing moved after a longer request: not recorded
    assert!(recorded_movement(&state, from, requested, Vec3::ZERO, release(26, 3, 0)).is_none());
}

/// D17: a shulker box's push stays in the entity's movementThisTick from 1.21.5 until its own
/// tick's applyEffectsFromBlocks, which visits the blocks of the push first (26.3 Entity.java:807,
/// 960-963; ShulkerBoxBlockEntity.java:133 applies nothing); a piston's takes its own back (1.21.5
/// PistonMovingBlockEntity.java:182-183, 26.3 :167-168), and 1.21.4 records no move (1.21.4
/// Entity.java:788-802).
#[test]
fn a_shulker_box_push_waits_in_the_movement_list_for_the_next_tick_from_1_21_5() {
    let mut world = WorldSnapshot::new();
    world.set_block((1, 1, 0), effect_block(BlockEffect::Fire, Aabb::new(Vec3::ZERO, Vec3::ZERO)));
    let pushed = |mover, release| {
        let mut state = SimulationState { no_gravity: true, remaining_fire_ticks: Some(-20), ..player_at(Vec3::new(0.5, 1.0, 0.5)) };
        move_entity(&mut state, &world, Vec3::new(2.0, 0.0, 0.0), PhysicsConfig::default(), release, mover);
        state
    };
    let held = pushed(MoverType::ShulkerBox, release(1, 21, 5)).pending_movements.as_slice()[0];
    assert_eq!((held.from, held.to), (Vec3::new(0.5, 1.0, 0.5), Vec3::new(2.5, 1.0, 0.5)));
    assert!(pushed(MoverType::ShulkerBox, release(1, 21, 4)).pending_movements.is_empty());
    assert!(pushed(MoverType::Piston, release(26, 3, 0)).pending_movements.is_empty());
    for (major, minor, patch, burns) in [(1, 21, 4, false), (1, 21, 5, true), (26, 3, 0, true)] {
        let release = release(major, minor, patch);
        let mut state = pushed(MoverType::ShulkerBox, release);
        crate::simulation::tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules(release));
        let after = state.remaining_fire_ticks.unwrap();
        assert_eq!(after > -20, burns, "{release:?}: the fire on the push's way, {after}");
        assert!(state.pending_movements.is_empty(), "{release:?}: the tick took it");
        // and only once: the next tick visits no fire
        crate::simulation::tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules(release));
        assert!(state.remaining_fire_ticks.unwrap() <= after, "{release:?}");
    }
    // a second push before the tick is a batch of its own after the first (1.21.5 Entity.java:720,
    // 811-815): each keeps its axis order
    let mut twice = pushed(MoverType::ShulkerBox, release(26, 3, 0));
    move_entity(&mut twice, &world, Vec3::new(0.0, 0.0, 1.0), PhysicsConfig::default(), release(26, 3, 0), MoverType::ShulkerBox);
    let held: Vec<_> = twice.pending_movements.as_slice().iter().map(|movement| (movement.from, movement.to, movement.axis_order)).collect();
    assert_eq!(held, vec![
        (Vec3::new(0.5, 1.0, 0.5), Vec3::new(2.5, 1.0, 0.5), Some(Vec3::new(2.0, 0.0, 0.0))),
        (Vec3::new(2.5, 1.0, 0.5), Vec3::new(2.5, 1.0, 1.5), Some(Vec3::new(0.0, 0.0, 1.0))),
    ]);
    // so the tick sweeps east and then south, past a fire south-east of the start that one move
    // from the first push's start to the second's end would cross
    let mut corner = WorldSnapshot::new();
    corner.set_block((1, 1, 1), effect_block(BlockEffect::Fire, Aabb::new(Vec3::ZERO, Vec3::ZERO)));
    let release = release(26, 3, 0);
    let mut state = SimulationState { no_gravity: true, remaining_fire_ticks: Some(-20), ..player_at(Vec3::new(0.5, 1.0, 0.5)) };
    move_entity(&mut state, &corner, Vec3::new(2.0, 0.0, 0.0), PhysicsConfig::default(), release, MoverType::ShulkerBox);
    move_entity(&mut state, &corner, Vec3::new(0.0, 0.0, 1.0), PhysicsConfig::default(), release, MoverType::ShulkerBox);
    let mut joined = state;
    joined.pending_movements = crate::simulation::HeldMovements::default();
    joined.pending_movements.push(crate::block_traversal::Movement { from: Vec3::new(0.5, 1.0, 0.5), to: Vec3::new(2.5, 1.0, 1.5), axis_order: None });
    crate::simulation::tick_with_rules(&mut state, &corner, PhysicsConfig::default(), rules(release));
    assert_eq!(state.remaining_fire_ticks, Some(-20), "the two pushes pass the fire by");
    crate::simulation::tick_with_rules(&mut joined, &corner, PhysicsConfig::default(), rules(release));
    assert!(joined.remaining_fire_ticks.unwrap() > -20, "one move joining them would cross it");
}

