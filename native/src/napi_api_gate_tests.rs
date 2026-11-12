//! B290 fixtures for the gates of napi_api.rs's private functions: a child module of it (see gate_fixtures_tests.rs).

use super::*;
use crate::world::{BlockShape, BlockState};

/// The collision shapes a query asks for: before 1.13 the legacy list, which keeps a box of no height (1.12.2 Level.java:1161-1184);
/// from 1.13 the voxel shapes, in which such a box is empty (1.13.2 LevelReader.java:123).
#[test]
fn the_queried_shapes_keep_a_flat_box_before_1_13() {
    let mut world = NativeWorld::new();
    let flat = BlockShape { bounds: crate::math::Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0)), ..BlockShape::full_block() };
    world.inner.set_block((0, 64, 0), BlockState { id: 1, shape: Some(flat), ..BlockState::default() });
    let shapes = |name: &str| {
        world
            .collision_shapes(NativeVec3 { x: 0.2, y: 63.9, z: 0.2 }, NativeVec3 { x: 0.8, y: 64.5, z: 0.8 }, Some(name.to_owned()))
            .expect("a query")
            .len()
    };
    assert_eq!(shapes("1.12.2"), 1);
    assert_eq!(shapes("1.13.2"), 0);
}

/// Entity.getViewVector: 1.12.2 turns by -yaw - PI and the negated pitch (1.12.2 Entity.java getVectorForRotation), 1.13 by the
/// pitch and -yaw (1.13.2 Entity.java calculateViewVector). The same vector in exact arithmetic, they round apart for some angles.
#[test]
fn the_view_vector_by_release() {
    let release = |name: &str| MinecraftVersion::parse(name).expect("a registered release");
    let found = (0..400).any(|step| {
        let (pitch, yaw) = (f64::from(step) * 0.013 - 1.3, f64::from(step) * 0.037);
        view_vector(pitch, yaw, release("1.12.2")) != view_vector(pitch, yaw, release("1.13.2"))
    });
    assert!(found, "an angle the two formulas round apart");
}
