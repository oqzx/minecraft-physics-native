//! Entity.move against the real one: scripts/java-oracle/move/OracleMove.java runs a jar's own
//! Entity.move over worlds of real block states, with every collision flag, the support and the
//! authority seeded either way, by every MoverType and more than once a tick, and
//! `native/fixtures/java-oracle-move-<release>.txt` holds what it left after each move. The entity
//! there is a bare Entity of the player's size: no sneak back-off, no damage. A Level that owns its
//! entity is a server's, one that does not a client's. Larger runs go through ORACLE_MOVE_FILE.
//!
//! The causes of a move, and where each comes in:
//!
//! | MoverType   | entry point in the engine            | stuck speed      | edge back-off | 0.51 limit |
//! |-------------|--------------------------------------|------------------|---------------|------------|
//! | SELF        | the tick's own move (simulation.rs)  | slows it         | Player's      | no         |
//! | PLAYER      | none: only the server's packet       | slows it         | Player's      | no         |
//! | PISTON      | move_by_piston, limit_piston_movement| 1.21.10: no slow | none          | per tick   |
//! | SHULKER     | none: scoped out, a shulker's push   | slows it         | none          | no         |
//! | SHULKER_BOX | none: only Shulker.move              | slows it         | none          | no         |
//!
//! Before 1.11 there is no MoverType: a piston's push is `move_entity` with SelfOrPlayer, and
//! backs off as any move does. MoverType::ShulkerBox stands for SHULKER_BOX and SHULKER.
//! The limit starts over at a new game time: TickScene::advance clears the deltas, and a line's
//! nextTick asks the same of this test.
//!
//! Which release refreshes what on a move with no y (Math.abs(delta.y) > 0.0 false):
//!
//! | release      | who refreshes verticalCollision, onGround and the support on a zero y   |
//! |--------------|-------------------------------------------------------------------------|
//! | up to 1.21.3 | every entity, every move                                                |
//! | from 1.21.4  | the instance that owns the entity: another keeps them                   |
//! | from 1.21.2  | the fall callback and landing stop: the owner only (simulates_landing)  |
//! | from 1.21.5  | checkFallDamage: the owner only, on either side                         |
//! | 1.21.4       | checkFallDamage: no player on the client, whoever owns it               |
//! | from 1.21.9  | a noPhysics move clears the four collision flags; before they stay      |
//! | from 26.2    | the block's callback is gone: restitution, only for a vertical move     |

use crate::math::Vec3;
use crate::piston::{limit_piston_movement, move_entity};
use crate::simulation::{MoverType, PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, BlockShape, BlockState, WorldSnapshot, FENCE_LIKE};
use crate::Aabb;
use std::collections::HashMap;

const FIXTURES: [(MinecraftVersion, &str); 11] = [
    (
        MinecraftVersion::new(1, 18, 2),
        include_str!("../fixtures/java-oracle-move-1.18.2.txt"),
    ),
    (
        MinecraftVersion::new(1, 19, 4),
        include_str!("../fixtures/java-oracle-move-1.19.4.txt"),
    ),
    (
        MinecraftVersion::new(1, 20, 5),
        include_str!("../fixtures/java-oracle-move-1.20.5.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 3),
        include_str!("../fixtures/java-oracle-move-1.21.3.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 4),
        include_str!("../fixtures/java-oracle-move-1.21.4.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 5),
        include_str!("../fixtures/java-oracle-move-1.21.5.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 8),
        include_str!("../fixtures/java-oracle-move-1.21.8.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 9),
        include_str!("../fixtures/java-oracle-move-1.21.9.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 11),
        include_str!("../fixtures/java-oracle-move-1.21.11.txt"),
    ),
    (
        MinecraftVersion::new(26, 2, 0),
        include_str!("../fixtures/java-oracle-move-26.2.txt"),
    ),
    (
        MinecraftVersion::new(26, 3, 0),
        include_str!("../fixtures/java-oracle-move-26.3.txt"),
    ),
];

fn numbers(words: &[&str]) -> Vec<f64> {
    words.iter().map(|word| word.parse().unwrap()).collect()
}

fn palette_state(
    id: u32,
    name: &str,
    boxes: &[f64],
    release: MinecraftVersion,
) -> (BlockState, Vec<BlockShape>) {
    let effect = match name {
        "soulsand" => BlockEffect::SoulSand,
        "honey" => BlockEffect::Honey,
        "slime" => BlockEffect::Slime,
        "bed" => BlockEffect::Bed,
        _ => BlockEffect::None,
    };
    let shapes: Vec<BlockShape> = boxes
        .chunks(6)
        .map(|b| BlockShape {
            bounds: Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])),
            slipperiness: 0.6,
            fluid: crate::world::FluidState::Empty,
            effect,
        })
        .collect();
    // Block.getBounceRestitution of 26.2 (the EFFECT.slime and EFFECT.bed of the adapter)
    let bounce_restitution = match (name, release.at_least(26, 2, 0)) {
        ("slime", true) => 1.0,
        ("bed", true) => 0.75,
        _ => 0.0,
    };
    let state = BlockState {
        id,
        tags: if name.starts_with("fence") { FENCE_LIKE } else { 0 },
        shape: shapes.first().copied(),
        bounce_restitution,
        ..BlockState::default()
    };
    (state, shapes)
}

/// The state of a line: pos3 vel3 onGround h v below minor noBlocks support3 fall stuck3.
const STATE_WORDS: usize = 19;

fn seed(state: &mut SimulationState, a: &[f64]) {
    state.position = Vec3::new(a[0], a[1], a[2]);
    state.velocity = Vec3::new(a[3], a[4], a[5]);
    state.on_ground = a[6] != 0.0;
    state.collided_horizontally = a[7] != 0.0;
    state.collided_vertically = a[8] != 0.0;
    state.vertical_collision_below = a[9] != 0.0;
    state.minor_horizontal_collision = a[10] != 0.0;
    state.on_ground_no_blocks = a[11] != 0.0;
    state.supporting_block =
        (!a[12].is_nan()).then_some((a[12] as i32, a[13] as i32, a[14] as i32));
    state.supporting_block_kept = true;
    state.fall_distance = a[15];
    state.stuck_speed_multiplier = Vec3::new(a[16], a[17], a[18]);
}

fn observed(state: &SimulationState) -> Vec<f64> {
    let flag = |value: bool| f64::from(u8::from(value));
    let support = state.supporting_block.map_or([f64::NAN; 3], |(x, y, z)| {
        [f64::from(x), f64::from(y), f64::from(z)]
    });
    vec![
        state.position.x,
        state.position.y,
        state.position.z,
        state.velocity.x,
        state.velocity.y,
        state.velocity.z,
        flag(state.on_ground),
        flag(state.collided_horizontally),
        flag(state.collided_vertically),
        flag(state.vertical_collision_below),
        flag(state.minor_horizontal_collision),
        flag(state.on_ground_no_blocks),
        support[0],
        support[1],
        support[2],
        state.fall_distance,
        state.stuck_speed_multiplier.x,
        state.stuck_speed_multiplier.y,
        state.stuck_speed_multiplier.z,
    ]
}

const FIELDS: [&str; STATE_WORDS] = [
    "x", "y", "z", "vx", "vy", "vz", "onGround", "horizontal", "vertical", "below", "minor",
    "noBlocks", "sx", "sy", "sz", "fall", "stuckX", "stuckY", "stuckZ",
];

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Mth.equal at its call: a move that falls short of what was asked by 1.0E-5F, ulps either side.
/// Whether the wall counts as a collision is what decides, and both answers must come up.
#[test]
fn a_shortfall_of_1e5_decides_the_horizontal_collision_as_in_the_jar() {
    for (release, fixture) in [
        (MinecraftVersion::new(1, 18, 2), include_str!("../fixtures/java-oracle-tolerance-1.18.2.txt")),
        (MinecraftVersion::new(1, 21, 11), include_str!("../fixtures/java-oracle-tolerance-1.21.11.txt")),
        (MinecraftVersion::new(26, 3, 0), include_str!("../fixtures/java-oracle-tolerance-26.3.txt")),
    ] {
        let flags: Vec<&str> = fixture
            .lines()
            .filter(|line| line.starts_with("case "))
            .map(|line| line.split(" = ").nth(1).unwrap().split(' ').nth(7).unwrap())
            .collect();
        assert!(flags.contains(&"1") && flags.contains(&"0"), "{release:?} has both sides");
        check_fixture(release, fixture);
    }
}

#[test]
fn entity_move_matches_the_jar() {
    for (release, fixture) in FIXTURES {
        check_fixture(release, fixture);
    }
}

/// A larger run of the oracle than the repository keeps: ORACLE_MOVE_FILE=release:path.
#[test]
fn entity_move_matches_the_jar_on_a_file() {
    let Ok(file) = std::env::var("ORACLE_MOVE_FILE") else {
        return;
    };
    let (release, path) = file.split_once(':').unwrap();
    let mut parts = release.split('.').map(|part| part.parse::<u16>().unwrap());
    let release = MinecraftVersion::new(
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap_or(0),
    );
    check_fixture(release, &std::fs::read_to_string(path).unwrap());
}

fn check_fixture(release: MinecraftVersion, fixture: &str) {
    {
        let mut palette: HashMap<u32, (BlockState, Vec<BlockShape>)> = HashMap::new();
        let mut world = WorldSnapshot::new();
        let mut failures = Vec::new();
        let mut by_field: HashMap<&str, u32> = HashMap::new();
        let mut cases = 0;
        for line in fixture.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words[0] {
                "palette" => {
                    let id: u32 = words[1].parse().unwrap();
                    let boxes = numbers(&words[4..]);
                    palette.insert(id, palette_state(id, words[2], &boxes, release));
                }
                "scene" => {
                    world = WorldSnapshot::new();
                    for cell in numbers(&words[3..]).chunks(4) {
                        let (state, shapes) = &palette[&(cell[3] as u32)];
                        let at = (cell[0] as i32, cell[1] as i32, cell[2] as i32);
                        if !shapes.is_empty() {
                            world.set_block_shapes(at, state.clone(), shapes.clone());
                        }
                    }
                }
                "case" => {
                    cases += 1;
                    let equals = words.iter().position(|word| *word == "=").unwrap();
                    let local = words[2] == "1";
                    let cause: u32 = words[3].parse().unwrap();
                    let no_physics = words[4] == "1";
                    let start = numbers(&words[5..5 + STATE_WORDS]);
                    let moves_at = 5 + STATE_WORDS;
                    assert_eq!(words[moves_at], "moves");
                    let deltas = numbers(&words[moves_at + 2..equals]);
                    let expected = numbers(&words[equals + 1..]);
                    let mut state = SimulationState {
                        half_width: f64::from(0.6_f32 / 2.0),
                        height: f64::from(1.8_f32),
                        non_living: true,
                        controlled_by_local_instance: local,
                        client_side: !local,
                        no_physics,
                        step_height: Some(f64::from(0.6_f32)),
                        gravity: Some(0.08),
                        ..SimulationState::default()
                    };
                    seed(&mut state, &start);
                    let config = PhysicsConfig::default();
                    let mut piston_deltas = [0.0; 3];
                    for (index, delta) in deltas.chunks(4).enumerate() {
                        if delta[3] != 0.0 {
                            piston_deltas = [0.0; 3];
                        }
                        let delta = Vec3::new(delta[0], delta[1], delta[2]);
                        if cause == 2 {
                            if let Some(delta) =
                                limit_piston_movement(&state, delta, &mut piston_deltas, release)
                            {
                                move_entity(&mut state, &world, delta, config, release, MoverType::Piston);
                            }
                        } else {
                            // (MoverType's order: SELF, PLAYER, PISTON, SHULKER_BOX, SHULKER)
                            let mover = if cause >= 3 {
                                MoverType::ShulkerBox
                            } else {
                                MoverType::SelfOrPlayer
                            };
                            move_entity(&mut state, &world, delta, config, release, mover);
                        }
                        let actual = observed(&state);
                        let want = &expected[index * STATE_WORDS..(index + 1) * STATE_WORDS];
                        // (the jars before 1.20 keep no support)
                        let supports = |at: &usize| release.at_least(1, 20, 0) || !(11..15).contains(at);
                        let differing: Vec<_> = (0..STATE_WORDS)
                            .filter(supports)
                            .filter(|at| !same(actual[*at], want[*at]))
                            .collect();
                        if !differing.is_empty() {
                            for at in &differing {
                                *by_field.entry(FIELDS[*at]).or_default() += 1;
                            }
                            failures.push(format!(
                                "{line}\n    move {index}: {}",
                                differing
                                    .iter()
                                    .map(|at| format!(
                                        "{} java {} rust {}",
                                        FIELDS[*at], want[*at], actual[*at]
                                    ))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            break;
                        }
                    }
                }
                other => panic!("{other}"),
            }
        }
        assert!(cases >= 500, "{release:?}: {cases} cases");
        assert!(
            failures.is_empty(),
            "{release:?}: {} of {cases} differ, by field {by_field:?}, first:\n{}",
            failures.len(),
            failures[..failures.len().min(14)].join("\n")
        );
    }
}

/// A shulker box against the real block entity: its lid, its box each tick and its push.
#[test]
fn a_shulker_box_opens_as_in_the_jar() {
    use crate::shulker_box::{push_entity, ShulkerBox};
    for (release, fixture) in [
        (MinecraftVersion::new(1, 16, 5), include_str!("../fixtures/java-oracle-shulker-1.16.5.txt")),
        (MinecraftVersion::new(1, 17, 1), include_str!("../fixtures/java-oracle-shulker-1.17.1.txt")),
        (MinecraftVersion::new(1, 20, 4), include_str!("../fixtures/java-oracle-shulker-1.20.4.txt")),
        (MinecraftVersion::new(26, 3, 0), include_str!("../fixtures/java-oracle-shulker-26.3.txt")),
    ] {
        // the box's own cell: a full cube, as the level has it without a block entity to ask
        let mut world = WorldSnapshot::new();
        let cube = BlockShape::full_block();
        world.set_block_shapes((0, 0, 0), BlockState { id: 1, shape: Some(cube), ..BlockState::default() }, vec![cube]);
        let mut moved = 0;
        for line in fixture.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let equals = words.iter().position(|word| *word == "=").unwrap();
            let a = numbers(&words[1..equals]);
            let want = numbers(&words[equals + 1..]);
            let mut lid = ShulkerBox { facing: a[0] as u8, ..ShulkerBox::default() };
            lid.block_event(1);
            match words[0] {
                "shulker" => {
                    for tick in 0..26 {
                        if tick == a[1] as usize {
                            lid.block_event(0);
                        }
                        lid.tick(release);
                        let b = lid.bounds();
                        let actual = [f64::from(lid.progress), b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z];
                        for (index, value) in actual.iter().enumerate() {
                            assert!(same(*value, want[tick * 7 + index]), "{release:?} {line}\n tick {tick} rust {actual:?}");
                        }
                    }
                }
                "push" | "pushclose" => {
                    let mut state = SimulationState {
                        position: Vec3::new(a[1], a[2], a[3]),
                        half_width: f64::from(0.6_f32 / 2.0),
                        height: f64::from(1.8_f32),
                        non_living: true,
                        controlled_by_local_instance: true,
                        step_height: Some(f64::from(0.6_f32)),
                        ..SimulationState::default()
                    };
                    if words[0] == "pushclose" {
                        for _ in 0..11 {
                            lid.tick(release);
                        }
                        lid.block_event(0);
                    }
                    for tick in 0..12 {
                        if lid.tick(release) {
                            push_entity(&mut state, &world, lid, (0, 0, 0), PhysicsConfig::default(), release);
                        }
                        let actual = [state.position.x, state.position.y, state.position.z];
                        for (index, value) in actual.iter().enumerate() {
                            assert!(same(*value, want[tick * 3 + index]), "{release:?} {line}\n tick {tick} rust {actual:?}");
                        }
                    }
                    moved += u32::from(state.position.x != a[1] || state.position.y != a[2] || state.position.z != a[3]);
                }
                other => panic!("{other}"),
            }
        }
        assert!(moved > 5, "{release:?}: the lid moved {moved}");
    }
}
