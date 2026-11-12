//! A world checkpoint (world/checkpoint.rs) gives back the world it was made of: every part of a
//! WorldSnapshot a tick or a query reads comes back alike, the bytes of the restored world are the
//! bytes it came from, a player ticked in the restored world moves as in the original, and bytes
//! that are no checkpoint of this schema are refused, never read as some other world.

use super::digest;
use crate::math::{Aabb, Vec3};
use crate::simulation::{PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::world::{
    BlockEffect, BlockShape, BlockState, EntityCollider, EntityCollisionContext, FluidState, VersionedBlockStateId,
    WorldSnapshot,
};

fn cell(x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(Vec3::new(x, y, z), Vec3::new(x + 1.0, y + 1.0, z + 1.0))
}

fn shape(bounds: Aabb, slipperiness: f64, fluid: FluidState, effect: BlockEffect) -> BlockShape {
    BlockShape { bounds, slipperiness, fluid, effect }
}

/// A world that holds something of every part a checkpoint writes.
fn every_part() -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    // a floor of stone, ice and slime, every field of a state set somewhere
    for x in -2..=6 {
        for z in -2..=2 {
            let (slipperiness, effect) = match x {
                1 => (0.98, BlockEffect::None),
                3 => (0.8, BlockEffect::Slime),
                5 => (0.6, BlockEffect::Honey),
                _ => (0.6, BlockEffect::None),
            };
            world.set_block(
                (x, 63, z),
                BlockState {
                    id: 1,
                    tags: 0x11,
                    shape: Some(shape(cell(0.0, 0.0, 0.0), slipperiness, FluidState::Empty, effect)),
                    bounce_restitution: if effect == BlockEffect::Slime { 1.0 } else { 0.0 },
                    suffocating: Some(true),
                    in_wall: Some(false),
                    bottom_face_sturdy: None,
                    explosion_resistance: Some(6.0),
                    ..BlockState::default()
                },
            );
        }
    }
    // water, lava and a waterlogged slab with its fluid's flow
    world.set_block(
        (8, 64, 0),
        BlockState {
            fluid: FluidState::Water,
            fluid_level: 8,
            fluid_source: true,
            water_block: true,
            explosion_resistance: Some(100.0),
            ..BlockState::default()
        },
    );
    world.set_block(
        (9, 64, 0),
        BlockState {
            fluid: FluidState::Lava,
            fluid_level: 6,
            fluid_flow: Vec3::new(0.25, -0.5, -0.125),
            fluid_falling: true,
            ..BlockState::default()
        },
    );
    world.set_block_shapes_with_geometry(
        (10, 64, 0),
        BlockState {
            id: 77,
            fluid: FluidState::Waterlogged,
            fluid_tags: 3,
            suppresses_bounce: true,
            fall_damage_resetting: true,
            player_fall_damage_resetting: true,
            ..BlockState::default()
        },
        vec![
            shape(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)), 0.6, FluidState::Waterlogged, BlockEffect::None),
            shape(Aabb::new(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.5, 1.0, 0.5)), 0.6, FluidState::Lava, BlockEffect::PowderSnow),
        ],
        vec![
            shape(cell(0.0, 0.0, 0.0), 0.6, FluidState::Empty, BlockEffect::LayeredCauldron(2)),
            shape(Aabb::new(Vec3::ZERO, Vec3::new(0.25, 0.25, 0.25)), 0.6, FluidState::Empty, BlockEffect::NetherPortal),
        ],
        vec![
            shape(cell(0.0, 0.0, 0.0), 0.6, FluidState::Empty, BlockEffect::EndPortal),
            shape(Aabb::new(Vec3::ZERO, Vec3::new(0.5, 0.5, 0.5)), 0.6, FluidState::Empty, BlockEffect::SweetBerrySapling),
        ],
    );
    world.set_block_properties((10, 64, 0), vec![("type".into(), "bottom".into()), ("waterlogged".into(), "true".into())]);
    world.set_block_state_id(
        (10, 64, 0),
        VersionedBlockStateId::new(MinecraftVersion::new(1, 20, 4), 11_042).expect("1.20.4 is a release"),
    );
    world.set_block_entity((10, 64, 0), "minecraft:chest".to_string());
    // a moving piston block, one under NOCLIP
    world.set_block((12, 64, 0), BlockState { id: 5, ..BlockState::default() });
    world.set_moving_block((12, 64, 0), &[cell(0.0, 0.0, 0.0), cell(0.0, 1.0, 0.0)], true);
    world.set_block((13, 64, 0), BlockState { id: 5, ..BlockState::default() });
    world.set_moving_block((13, 64, 0), &[cell(0.0, 0.0, 0.0)], false);
    world.set_moving_block_noclip((13, 64, 0), (1, 0, 0), &[Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.25, 1.0))]);
    // an empty section the server sent, chunks loaded and not, the build height and border
    world.set_section_present(0, 7, 0, true);
    world.set_chunk_loaded(5, 5, true);
    world.set_chunk_loaded(-3, 2, false);
    world.set_world_bounds(-64, 320, 8.5, -4.25, 1000.0);
    world.set_world_border_absolute_max_size(29_000_000);
    world.set_entity_colliders(vec![
        EntityCollider { bounds: Aabb::new(Vec3::new(14.0, 64.0, 0.0), Vec3::new(15.375, 64.5625, 1.375)), only_from_above: false },
        EntityCollider { bounds: cell(15.0, 64.0, 0.0), only_from_above: true },
    ]);
    // a block written into the unloaded chunk, which 1.13 reads the fluid of
    world.set_release(MinecraftVersion::new(1, 13, 2));
    world.set_block((-40, 64, 40), BlockState { fluid: FluidState::Water, fluid_level: 8, ..BlockState::default() });
    world
}

fn restored(world: &WorldSnapshot) -> WorldSnapshot {
    WorldSnapshot::decode(&world.encode()).expect("a checkpoint of this schema")
}

#[test]
fn the_digest_is_fnv_1a_of_the_bytes() {
    // the 64-bit FNV-1a test vectors (the offset basis for no bytes)
    assert_eq!(digest(b""), "cbf29ce484222325");
    assert_eq!(digest(b"a"), "af63dc4c8601ec8c");
    assert_eq!(digest(b"foobar"), "85944171f73967e8");
    let world = every_part();
    assert_eq!(digest(&world.encode()), digest(&restored(&world).encode()));
    let mut other = every_part();
    other.set_block_entity((10, 64, 0), "minecraft:barrel".to_string());
    assert_ne!(digest(&world.encode()), digest(&other.encode()));
}

#[test]
fn every_block_effect_has_its_own_code_and_comes_back_from_it() {
    let effects = [
        BlockEffect::None,
        BlockEffect::SoulSand,
        BlockEffect::Honey,
        BlockEffect::Slime,
        BlockEffect::Web,
        BlockEffect::Climbable,
        BlockEffect::BubbleUp,
        BlockEffect::BubbleDown,
        BlockEffect::PowderSnow,
        BlockEffect::SweetBerryBush,
        BlockEffect::SoulSoil,
        BlockEffect::Scaffolding,
        BlockEffect::Cactus,
        BlockEffect::Magma,
        BlockEffect::Campfire,
        BlockEffect::SoulCampfire,
        BlockEffect::Fire,
        BlockEffect::SoulFire,
        BlockEffect::Bed,
        BlockEffect::Hay,
        BlockEffect::PointedDripstoneTip,
        BlockEffect::GlideThroughClimbable,
        BlockEffect::LilyPad,
        BlockEffect::SnowLayer,
        BlockEffect::LavaCauldron,
        BlockEffect::LayeredCauldron(1),
        BlockEffect::LayeredCauldron(2),
        BlockEffect::LayeredCauldron(3),
        BlockEffect::SweetBerrySapling,
        BlockEffect::NetherPortal,
        BlockEffect::EndPortal,
    ];
    // the codes are 0 - 30, one each
    let codes: Vec<u8> = effects.iter().map(|effect| effect.code()).collect();
    assert_eq!(codes, (0..=30).collect::<Vec<u8>>());
    for effect in effects {
        assert_eq!(BlockEffect::from_code(effect.code()), Ok(effect));
    }
    // a cauldron's level outside 1 - 3 is held to it
    assert_eq!(BlockEffect::LayeredCauldron(0).code(), 25);
    assert_eq!(BlockEffect::LayeredCauldron(9).code(), 27);
    assert!(BlockEffect::from_code(31).is_err());
    assert!(BlockEffect::from_code(255).is_err());
}

#[test]
fn a_restored_world_is_the_world_it_was_made_of() {
    let world = every_part();
    let bytes = world.encode();
    let back = restored(&world);
    assert_eq!(back.encode(), bytes);
    // the encoding does not hang on the order a world was filled in
    let fill = |order: &[i32]| {
        let mut world = WorldSnapshot::new();
        for &x in order {
            world.set_block((x, 0, 0), BlockState { id: x as u32, ..BlockState::default() });
            world.set_block_properties((x, 0, 0), vec![("n".into(), x.to_string())]);
            world.set_block_entity((x, 0, 0), format!("e{x}"));
        }
        world.encode()
    };
    assert_eq!(fill(&[0, 1, 2, 30, -5]), fill(&[-5, 30, 2, 1, 0]));

    let release = MinecraftVersion::new(1, 20, 4);
    for position in [(0, 63, 0), (3, 63, 1), (8, 64, 0), (9, 64, 0), (10, 64, 0), (12, 64, 0), (13, 64, 0), (-40, 64, 40), (20, 20, 20)] {
        assert_eq!(back.block(position), world.block(position), "{position:?}");
        assert_eq!(back.block_properties(position), world.block_properties(position), "{position:?}");
        assert_eq!(back.block_state_id(position), world.block_state_id(position), "{position:?}");
        assert_eq!(back.block_entity(position), world.block_entity(position), "{position:?}");
        assert_eq!(back.block_collision_boxes(position), world.block_collision_boxes(position), "{position:?}");
        assert_eq!(back.block_outline_boxes(position), world.block_outline_boxes(position), "{position:?}");
        assert_eq!(back.is_moving_block(position), world.is_moving_block(position), "{position:?}");
        assert_eq!(back.moving_block_base(position), world.moving_block_base(position), "{position:?}");
        assert_eq!(back.fluid_flow_at(position), world.fluid_flow_at(position), "{position:?}");
    }
    // what the parts read as
    assert_eq!(back.block((9, 64, 0)).fluid, FluidState::Lava);
    assert_eq!(back.block((10, 64, 0)).fluid, FluidState::Waterlogged);
    assert_eq!(back.block((0, 63, 0)).explosion_resistance, Some(6.0));
    assert_eq!(back.block((10, 64, 0)).interaction_shape.map(|shape| shape.effect), Some(BlockEffect::EndPortal));
    assert_eq!(back.block_outline_boxes((10, 64, 0)).len(), 2);
    assert_eq!(back.block_collision_boxes((10, 64, 0)).len(), 2);
    assert_eq!(back.block_entity((10, 64, 0)), Some("minecraft:chest"));
    assert_eq!(back.block_state_id((10, 64, 0)).map(|id| (id.version(), id.id())), Some((release, 11_042)));
    assert_eq!(back.block_properties((10, 64, 0)).len(), 2);
    assert_eq!(back.moving_block_base((13, 64, 0)), vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.25, 1.0))]);
    assert!(back.moves_through((1, 0, 0)));
    assert!(!back.moves_through((0, 1, 0)));
    assert!(back.is_section_present(0, 7, 0));
    assert_eq!(back.section_block_count(0, 3, 0), world.section_block_count(0, 3, 0));
    assert!(back.is_chunk_loaded(5, 5));
    assert!(!back.is_chunk_loaded(-3, 2));
    assert_eq!(back.world_bounds(), (-64, 320, 8.5, -4.25, 1000.0));
    assert_eq!(back.empty_chunk_reads(), world.empty_chunk_reads());
    assert_eq!(back.block((-40, 64, 40)).fluid, FluidState::Water);
    let query = Aabb::new(Vec3::new(13.0, 63.0, -1.0), Vec3::new(17.0, 66.0, 2.0));
    for bottom in [64.0, 65.0] {
        let context = EntityCollisionContext { bottom, ..EntityCollisionContext::default() };
        assert_eq!(back.entity_collisions(query, &context), world.entity_collisions(query, &context));
    }
    let wide = Aabb::new(Vec3::new(-3.0, 60.0, -3.0), Vec3::new(16.0, 70.0, 3.0));
    assert_eq!(back.collision_shapes(wide, release), world.collision_shapes(wide, release));
    // the border's absolute bound clamps alike
    let far = Vec3::new(4.0e7, 64.0, -4.0e7);
    assert_eq!(back.clamp_to_world_border(far, release), world.clamp_to_world_border(far, release));
}

#[test]
fn a_player_ticked_in_a_restored_world_moves_as_in_the_original() {
    let world = every_part();
    let back = restored(&world);
    // a walk over stone, ice, slime and honey, and a fall onto the slab in the water's way
    let walker = SimulationState {
        position: Vec3::new(-1.5, 64.0, 0.5),
        velocity: Vec3::new(0.4, 0.0, 0.0),
        half_width: 0.3,
        height: 1.8,
        on_ground: true,
        controlled_by_local_instance: true,
        ..SimulationState::default()
    };
    let faller = SimulationState { position: Vec3::new(10.5, 67.0, 0.5), velocity: Vec3::ZERO, on_ground: false, ..walker };
    for version in ["1.8.8", "1.12.2", "1.13.2", "1.20.4", "26.3"] {
        for start in [walker, faller] {
            let (mut original, mut copy) = (start, start);
            for tick in 0..40 {
                let a = crate::versioned::dispatch(version, &mut original, &world, PhysicsConfig::default()).expect("a release");
                let b = crate::versioned::dispatch(version, &mut copy, &back, PhysicsConfig::default()).expect("a release");
                assert_eq!(format!("{a:?}"), format!("{b:?}"), "{version} tick {tick}");
                assert_eq!(format!("{original:?}"), format!("{copy:?}"), "{version} tick {tick}");
            }
            // the walk did go somewhere: it is no tick of an empty world
            assert_ne!(original.position, start.position, "{version}");
        }
    }
}

#[test]
fn bytes_of_no_checkpoint_of_this_schema_are_refused() {
    let world = every_part();
    let bytes = world.encode();
    assert!(WorldSnapshot::decode(b"").unwrap_err().contains("MCPNWLD3"));
    assert!(WorldSnapshot::decode(b"MCPNWLD2").unwrap_err().contains("MCPNWLD3"));
    let mut other = bytes.clone();
    other[7] = b'4';
    assert!(WorldSnapshot::decode(&other).unwrap_err().contains("MCPNWLD3"));
    // every cut short ends early, and none reads as a world
    for end in 8..bytes.len() {
        let error = WorldSnapshot::decode(&bytes[..end]).expect_err("a checkpoint cut short");
        assert!(error.contains("ends early") || error.contains("more entries than it holds"), "{end}: {error}");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(WorldSnapshot::decode(&longer).unwrap_err(), "a world checkpoint goes on past its end");
    // a count of more entries than there are bytes
    let mut counted = bytes.clone();
    counted[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(WorldSnapshot::decode(&counted).unwrap_err(), "a world checkpoint counts more entries than it holds");
}

/// The bytes of a world of one block of `state` at 0, 0, 0 and the offset of the block's state.
fn one_block(state: BlockState) -> (Vec<u8>, usize) {
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), state);
    // the magic, the count of blocks and the cell
    (world.encode(), 8 + 4 + 12)
}

#[test]
fn a_byte_out_of_its_range_is_refused() {
    // a state with no shapes: id, tags, fluid tags (12), three shape flags (3), the restitution (8),
    // suppresses_bounce (1), then the fluid's code, level, source, flow (24), falling, water_block,
    // the two fall damage resetting flags, then suffocating
    let (bytes, state) = one_block(BlockState::default());
    let flag = state + 12;
    let fluid = state + 12 + 3 + 8 + 1;
    let suffocating = fluid + 1 + 1 + 1 + 24 + 4;
    let corrupt = |at: usize, value: u8| {
        let mut bytes = bytes.clone();
        bytes[at] = value;
        WorldSnapshot::decode(&bytes).unwrap_err()
    };
    assert_eq!(corrupt(flag, 2), "a world checkpoint holds 2 for a flag");
    assert_eq!(corrupt(fluid, 4), "a world checkpoint holds fluid 4");
    assert_eq!(corrupt(suffocating, 3), "a world checkpoint holds 3 for an optional flag");
    // each of those is the byte it names: the right values there read back
    assert_eq!(WorldSnapshot::decode(&{ let mut b = bytes.clone(); b[fluid] = 2; b }).unwrap().block((0, 0, 0)).fluid, FluidState::Lava);
    assert_eq!(WorldSnapshot::decode(&{ let mut b = bytes.clone(); b[suffocating] = 1; b }).unwrap().block((0, 0, 0)).suffocating, Some(true));
    // an effect out of range in a shape: the shape's bounds (48) and slipperiness (8), then its fluid
    let (shaped, state) = one_block(BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() });
    let effect = state + 12 + 1 + 48 + 8 + 1;
    let mut bad = shaped.clone();
    bad[effect] = 31;
    assert_eq!(WorldSnapshot::decode(&bad).unwrap_err(), "block effect must be between 0 and 30");
    bad[effect] = 19;
    assert_eq!(WorldSnapshot::decode(&bad).unwrap().block((0, 0, 0)).shape.map(|shape| shape.effect), Some(BlockEffect::Hay));
    // the empty chunk read as a fourth way: its code is the byte before the shared chunk's flag
    let empty = WorldSnapshot::new().encode();
    let mut reads = empty.clone();
    let at = reads.len() - 2;
    reads[at] = 3;
    assert_eq!(WorldSnapshot::decode(&reads).unwrap_err(), "a world checkpoint reads the empty chunk as 3");
    reads[at] = 1;
    assert_eq!(WorldSnapshot::decode(&reads).unwrap().empty_chunk_reads(), crate::world::EmptyChunkReads::Blocks);
}

#[test]
fn a_state_id_of_a_release_not_supported_or_not_named_is_refused() {
    let mut world = WorldSnapshot::new();
    world.set_block((0, 0, 0), BlockState::default());
    world.set_block_state_id((0, 0, 0), VersionedBlockStateId::new(MinecraftVersion::new(1, 20, 4), 9).unwrap());
    let bytes = world.encode();
    let name = bytes.windows(6).position(|window| window == b"1.20.4").expect("the release's name");
    let with = |text: &[u8; 6]| {
        let mut bytes = bytes.clone();
        bytes[name..name + 6].copy_from_slice(text);
        WorldSnapshot::decode(&bytes)
    };
    assert_eq!(with(b"x.20.4").unwrap_err(), "a world checkpoint names release x.20.4");
    assert!(with(b"1.99.4").unwrap_err().contains("not present in the source-backed release registry"));
    assert!(with(b"1.20.\xff").unwrap_err().contains("utf-8"));
    let back = with(b"1.20.3").expect("1.20.3 is a release");
    assert_eq!(back.block_state_id((0, 0, 0)).map(|id| (id.version(), id.id())), Some((MinecraftVersion::new(1, 20, 3), 9)));
}
