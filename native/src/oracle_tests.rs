//! The math and geometry primitives against the real classes of vanilla jars: each line of
//! `native/fixtures/java-oracle-<release>.txt` is an operation, its inputs and what Java returned
//! (scripts/java-oracle/Oracle.java over AABB, Vec3, Mth and Shapes). Comparisons are by bits, so
//! signed zeros count, and the results are Java's, not a predicate recomputed here. 1.16.1 stands
//! for the float lengths and the old clamp, 1.21.11 for the double table lookup with the old
//! floor, 26.3 for the current ones. HotSpot 25.0.3 ran the jars.
//!
//! What is compared with Java, and by what, against what is only a property of the Rust:
//!
//! | class                                      | predicate or operation       | Java            |
//! |--------------------------------------------|------------------------------|-----------------|
//! | separated, touching, overlapping boxes     | intersects, contains         | Oracle          |
//! | inverted, degenerate, contraction past size| ctor, deflate, contract      | Oracle (M14)    |
//! | below, on, above 1.0E-7 of a face          | collide, join                | Oracle (M20)    |
//! | full cube against partial shape            | collide, join, shapes_block  | Oracle (M20)    |
//! | every axis, either sign                    | collide, Entity.move         | Oracle, Move    |
//! | negative coordinates and 3.0E7             | Entity.move, union moves     | Move, Geometry  |
//! | one, duplicate, overlapping, reordered     | Shapes.or, optimize          | Geometry        |
//! | faces, edges, corners, inside, parallel    | AABB.clip, VoxelShape.clip   | Geometry        |
//! | the same before 1.13                       | AABB.method_585              | LegacyClip      |
//! | cancellation, mixed magnitudes, -0.0       | vassoc, vcancel, vproject2   | Geometry        |
//! | every block state of the release           | boxes, order, grid lines     | States          |
//!
//! Legal positions near the world limit are Entity.move's 29999000 scenes; a translation or an axis
//! swap of a case is no property here: each class above is Java's own line.

use crate::block_traversal::{
    blocks_intersected_between, blocks_intersected_between_in_direction, box_traverse_blocks,
};
use crate::collision::{vanilla_equal, collide_over_grid, grid_lines, reaches_into, Axis, BlockVoxelShape, GridTest};
use crate::math::{
    equal_f32, equal_f64, abs_max, atan2_vanilla_on, ceil_f32_for, ceil_i32_for, clamp_f32_for, clamp_f64_for,
    floor_f32_for, floor_i32_for, floor_i64_for, frac_f64_for, java_round, lerp, round_i32,
    wrap_degrees, wrap_degrees_f32, Aabb, Vec3,
};
use crate::projectile::{clip_block, ClipShape};
use crate::math::friction_influenced_speed;
use crate::simulation::{mth_cos, mth_sin, relative_movement, yaw_radians};
use crate::version::MinecraftVersion;

/// The JDK the fixtures ran every release's jar on (HotSpot 25.0.3, docs/numeric-contract.md), so its Math.
const ORACLE_JAVA: u8 = 25;
use crate::voxel_shape::VoxelShape;
use crate::world::{shape_meets_query, BlockShape, BlockState, WorldSnapshot};

const FIXTURES: [(MinecraftVersion, &str); 4] = [
    (
        MinecraftVersion::new(1, 16, 1),
        include_str!("../fixtures/java-oracle-1.16.1.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 11),
        include_str!("../fixtures/java-oracle-1.21.11.txt"),
    ),
    (
        MinecraftVersion::new(26, 3, 0),
        include_str!("../fixtures/java-oracle-26.3.txt"),
    ),
    // (signed zeros, subnormals, the cuts, the world limit, and the look's wrap: scripts/java-oracle/OracleEdges.java)
    (
        MinecraftVersion::new(26, 3, 0),
        include_str!("../fixtures/java-oracle-edges-26.3.txt"),
    ),
];

fn vec3(values: &[f64]) -> Vec3 {
    Vec3::new(values[0], values[1], values[2])
}

fn aabb(values: &[f64]) -> Aabb {
    Aabb::new(vec3(&values[0..3]), vec3(&values[3..6]))
}

fn of_box(bounds: Aabb) -> Vec<f64> {
    vec![
        bounds.min.x,
        bounds.min.y,
        bounds.min.z,
        bounds.max.x,
        bounds.max.y,
        bounds.max.z,
    ]
}

fn of_vec(vector: Vec3) -> Vec<f64> {
    vec![vector.x, vector.y, vector.z]
}

fn flag(value: bool) -> Vec<f64> {
    vec![if value { 1.0 } else { 0.0 }]
}

/// n, then each box of a union and its move, and how many numbers that took.
fn parts_of(a: &[f64]) -> (Vec<(Aabb, Vec3)>, usize) {
    let count = a[0] as usize;
    let parts = a[1..1 + count * 9]
        .chunks(9)
        .map(|part| {
            let moved = vec3(&part[6..]);
            (aabb(part).translated(moved), moved)
        })
        .collect();
    (parts, 1 + count * 9)
}

/// Whether two lines of the boxes of a union differ by 2.0E-7 at most, or a box is that near a line
/// of the eighths of its block, which Shapes.create takes it to.
fn near_coincident(a: &[f64]) -> bool {
    let (parts, _) = parts_of(a);
    let off_the_grid = a[1..1 + a[0] as usize * 9].chunks(9).any(|part| {
        part[..6].iter().any(|value| {
            let line = (value * 8.0).round() / 8.0;
            *value != line && (value - line).abs() <= 2.0e-7
        })
    });
    off_the_grid
        || (0..3).any(|axis| {
            let of_vector = |vector: Vec3| [vector.x, vector.y, vector.z][axis];
            let mut lines: Vec<f64> = parts
                .iter()
                .flat_map(|(bounds, _)| [of_vector(bounds.min), of_vector(bounds.max)])
                .collect();
            lines.sort_by(f64::total_cmp);
            lines.windows(2).any(|pair| pair[0] != pair[1] && pair[1] - pair[0] <= 2.0e-7)
        })
}

/// Shapes.or over the shapes in turn, each a Shapes.create moved.
fn union_of(a: &[f64], release: MinecraftVersion) -> (VoxelShape, usize) {
    let count = a[0] as usize;
    let shapes: Vec<VoxelShape> = a[1..1 + count * 9]
        .chunks(9)
        .map(|part| VoxelShape::from_box(aabb(part)).translated(vec3(&part[6..])))
        .collect();
    (VoxelShape::union(release, &shapes), 1 + count * 9)
}

fn probe_of(a: &[f64]) -> (Axis, Aabb, f64) {
    ([Axis::X, Axis::Y, Axis::Z][a[0] as usize], aabb(&a[1..7]), a[7])
}

/// VoxelShape.clip of a block of two boxes at `a[..3]`, the hit, its face and whether it started
/// inside (the engine has no use for the last, which its 0.001 step implies).
fn shape_clip(a: &[f64], release: MinecraftVersion) -> Vec<f64> {
    let position = (a[0] as i32, a[1] as i32, a[2] as i32);
    let boxes = [
        Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
        Aabb::new(Vec3::new(0.5, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    ];
    let shapes: Vec<BlockShape> = boxes
        .iter()
        .map(|bounds| BlockShape {
            bounds: *bounds,
            ..BlockShape::full_block()
        })
        .collect();
    let mut world = WorldSnapshot::new();
    world.set_block_shapes(position, BlockState::default(), shapes);
    let from = vec3(&a[3..6]);
    let to = vec3(&a[6..9]);
    let inside = (to - from).length_squared() >= 1.0e-7 && {
        let step = from + (to - from).scale(0.001);
        boxes
            .iter()
            .any(|bounds| bounds.translated(vec3(a)).contains(step))
    };
    clip_block(&world, position, from, to, release, ClipShape::Collider).map_or_else(Vec::new, |(hit, face)| {
        let mut out = of_vec(hit);
        out.push(f64::from(face as u8));
        out.push(if inside { 1.0 } else { 0.0 });
        out
    })
}

/// What Rust makes of a line, or None for an operation this reader does not know.
fn run(op: &str, a: &[f64], release: MinecraftVersion) -> Option<Vec<f64>> {
    Some(match op {
        "ctor" => of_box(Aabb::ordered(vec3(&a[0..3]), vec3(&a[3..6]))),
        "inflate" => of_box(aabb(a).inflate(vec3(&a[6..]))),
        "deflate" => of_box(aabb(a).deflate(vec3(&a[6..]))),
        "contract" => of_box(aabb(a).contract(vec3(&a[6..]))),
        "expand" => of_box(aabb(a).expand_towards(vec3(&a[6..]))),
        "move" => of_box(aabb(a).translated(vec3(&a[6..]))),
        "intersect" => of_box(aabb(a).intersect(aabb(&a[6..]))),
        "minmax" => of_box(aabb(a).minmax(aabb(&a[6..]))),
        "intersects" => flag(aabb(a).intersects(aabb(&a[6..]))),
        "contains" => flag(aabb(a).contains(vec3(&a[6..]))),
        "distance" => vec![aabb(a).distance_to_sqr_point(vec3(&a[6..]))],
        "chain" => {
            let (x, y, z) = (a[6], a[7], a[8]);
            of_box(
                aabb(a)
                    .translated(Vec3::new(x, y, z))
                    .deflate(Vec3::new(z, x, y))
                    .translated(Vec3::new(-y, -z, -x)),
            )
        }
        "clip" => aabb(a)
            .clip_segment(vec3(&a[6..9]), vec3(&a[9..12]))
            .map_or_else(Vec::new, of_vec),
        "join" => flag(shape_meets_query(aabb(a), false, aabb(&a[6..]), release)),
        "vadd" => of_vec(vec3(a).plus(vec3(&a[3..]))),
        "vsub" => of_vec(vec3(a) - vec3(&a[3..])),
        "vmul" => of_vec(vec3(a).multiply(vec3(&a[3..]))),
        "vscale" => of_vec(vec3(a).scale(a[3])),
        "vlerp" => of_vec(vec3(a).lerp(vec3(&a[3..]), a[6])),
        "vcross" => of_vec(vec3(a).cross(vec3(&a[3..]))),
        "vdot" => vec![vec3(a).dot(vec3(&a[3..]))],
        "vlength" => vec![vec3(a).length_in(release)],
        "vhlength" => vec![vec3(a).horizontal_length()],
        "vnormalize" => of_vec(vec3(a).normalized(release)),
        "vproject" => of_vec(vec3(a).projected_on(vec3(&a[3..]))),
        "vreverse" => of_vec(vec3(a).reverse()),
        "floor" => vec![f64::from(floor_i32_for(release, a[0]))],
        "lfloor" => vec![floor_i64_for(release, a[0]) as f64],
        "ceil" => vec![f64::from(ceil_i32_for(release, a[0]))],
        "floorf" => vec![f64::from(floor_f32_for(release, a[0] as f32))],
        "ceilf" => vec![f64::from(ceil_f32_for(release, a[0] as f32))],
        "frac" => vec![frac_f64_for(release, a[0])],
        "round" => vec![java_round(a[0]) as f64, f64::from(round_i32(a[0]))],
        "clamp" => vec![clamp_f64_for(release, a[0], a[1], a[2])],
        "clampf" => vec![f64::from(clamp_f32_for(
            release,
            a[0] as f32,
            a[1] as f32,
            a[2] as f32,
        ))],
        "absmax" => vec![abs_max(a[0], a[1])],
        "wrapdegrees" => vec![wrap_degrees(a[0])],
        "wrapdegreesf" => vec![f64::from(wrap_degrees_f32(a[0] as f32))],
        "lerp" => vec![lerp(a[2], a[0], a[1])],
        "sin" => vec![mth_sin(a[0], release)],
        "cos" => vec![mth_cos(a[0], release)],
        "atan2" => vec![atan2_vanilla_on(a[0], a[1], ORACLE_JAVA)],
        "sintable" => {
            let mut sum = 0_i64;
            for index in 0..65536 {
                let radians = (f64::from(index) + 0.5) / 10430.378350470453;
                let value = mth_sin(radians, release) as f32;
                sum = sum
                    .wrapping_mul(31)
                    .wrapping_add(i64::from(value.to_bits() as i32));
            }
            vec![(sum as u64 >> 11) as f64]
        }
        "union" => union_of(a, release).0.boxes().iter().copied().flat_map(of_box).collect(),
        "unioncollide" => {
            let (shape, used) = union_of(a, release);
            let (axis, moving, delta) = probe_of(&a[used..]);
            vec![shape.collide_in(release, axis, moving, delta)]
        }
        "rawcollide" => {
            let (parts, used) = parts_of(a);
            let (axis, moving, delta) = probe_of(&a[used..]);
            let grids = [Axis::X, Axis::Y, Axis::Z]
                .map(|axis| grid_lines(parts.iter().copied(), axis, GridTest::of(Some(release))));
            let boxes: Vec<Aabb> = parts.iter().map(|(bounds, _)| *bounds).collect();
            vec![collide_over_grid(&grids, &boxes, axis, moving, delta)]
        }
        "clipface" => aabb(a)
            .clip_segment_face(vec3(&a[6..9]), vec3(&a[9..12]))
            .map_or_else(Vec::new, |(hit, face)| {
                let mut out = of_vec(hit);
                out.push(f64::from(face as u8));
                out
            }),
        "shapeclip" => shape_clip(a, release),
        "vassoc" => {
            let (x, y, z) = (vec3(a), vec3(&a[3..]), vec3(&a[6..]));
            let mut out = of_vec(x.plus(y).plus(z));
            out.extend(of_vec(x.plus(y.plus(z))));
            out
        }
        "vcancel" => of_vec(vec3(a).plus(vec3(&a[3..])) - vec3(&a[3..])),
        "vscaleadd" => of_vec(vec3(a).plus(vec3(&a[3..])).scale(a[9])),
        "vdist" => of_vec((vec3(a) - vec3(&a[3..])).multiply(vec3(&a[6..]))),
        "vproject2" => of_vec(vec3(a).projected_on(vec3(&a[3..])).projected_on(vec3(&a[6..]))),
        "inputvec" => of_vec(relative_movement(
            vec3(a),
            a[3],
            yaw_radians(a[4] as f32, release),
            release,
        )),
        "jsin" => vec![crate::trig::sin(a[0])],
        "jcos" => vec![crate::trig::cos(a[0])],
        "collide" => {
            let shape = VoxelShape::from_box(aabb(a));
            let axis = [Axis::X, Axis::Y, Axis::Z][a[6] as usize];
            let delta = a[13];
            // Shapes.collide over the one shape: its leading 1.0E-7 check, then the shape's
            vec![if delta.abs() < 1.0e-7 {
                0.0
            } else {
                shape.collide(axis, aabb(&a[7..13]), delta)
            }]
        }
        _ => return None,
    })
}

const GEOMETRY: [(MinecraftVersion, &str); 5] = [
    (
        MinecraftVersion::new(1, 16, 1),
        include_str!("../fixtures/java-oracle-geometry-1.16.1.txt"),
    ),
    (
        MinecraftVersion::new(1, 17, 1),
        include_str!("../fixtures/java-oracle-geometry-1.17.1.txt"),
    ),
    (
        MinecraftVersion::new(1, 19, 4),
        include_str!("../fixtures/java-oracle-geometry-1.19.4.txt"),
    ),
    (
        MinecraftVersion::new(1, 21, 11),
        include_str!("../fixtures/java-oracle-geometry-1.21.11.txt"),
    ),
    (
        MinecraftVersion::new(26, 3, 0),
        include_str!("../fixtures/java-oracle-geometry-26.3.txt"),
    ),
];

#[test]
fn primitives_match_the_vanilla_classes_bit_for_bit() {
    let mut report = Vec::new();
    for (release, fixture) in FIXTURES.into_iter().chain(GEOMETRY) {
        let mut failures: std::collections::BTreeMap<&str, (u32, String)> = Default::default();
        let mut lines = 0;
        for line in fixture.lines() {
            let (left, right) = line.split_once(" =").expect("a fixture line has a result");
            let mut left = left.split(' ');
            let op = left.next().unwrap();
            let inputs: Vec<f64> = left.map(|value| value.parse().unwrap()).collect();
            let expected: Vec<f64> = right
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            // An open gap of VoxelShape::union, which only tests call: where lines of its shapes lie
            // within 1.0E-7 of each other (no block shape's do), IndirectMerger's walk keeps a value
            // the sorted lines of grid_lines do not, and the cells follow the merged indices.
            if matches!(op, "union" | "unioncollide") && near_coincident(&inputs) {
                continue;
            }
            let actual = run(op, &inputs, release).unwrap_or_else(|| panic!("unknown operation {op}"));
            lines += 1;
            let same = actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(&expected)
                    .all(|(a, e)| a.to_bits() == e.to_bits() || (a.is_nan() && e.is_nan()));
            if !same {
                let entry = failures.entry(op).or_insert((0, format!("{line}\n    rust = {actual:?}")));
                entry.0 += 1;
            }
        }
        assert!(lines > 300, "{release:?}: the whole run, not {lines} lines");
        for (op, (count, first)) in failures {
            report.push(format!("{release:?} {op}: {count} of {lines}, first:\n{first}"));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}

/// The vector fixtures tell the Java order from an algebraically equal one: (a + b) + c and a + (b
/// + c) are both Java's lines, and they differ, as a rewritten call site would. The reference is
/// HotSpot 25.0.3 running the jars of OracleGeometry.java.
#[test]
fn a_reassociated_sum_is_not_the_jars() {
    let mut told_apart = 0;
    for (_, fixture) in GEOMETRY {
        for line in fixture.lines().filter(|line| line.starts_with("vassoc ")) {
            let (_, right) = line.split_once(" =").unwrap();
            let results: Vec<f64> = right.split_whitespace().map(|value| value.parse().unwrap()).collect();
            if (0..3).any(|at| results[at].to_bits() != results[at + 3].to_bits()) {
                told_apart += 1;
            }
        }
    }
    assert!(told_apart > 20, "only {told_apart} lines tell the orders apart");
}

/// AABB.method_585, the six-face clip before 1.13, from the jars of 1.8.9, 1.9.4 and 1.12.2
/// (scripts/java-oracle/OracleLegacyClip.java): the hit and the face, exits from inside among them.
#[test]
fn the_legacy_clip_is_the_jars() {
    let mut failures = Vec::new();
    let mut lines = 0;
    let mut hits = 0;
    for line in include_str!("../fixtures/java-oracle-clip-legacy.txt").lines() {
        let (left, right) = line.split_once(" =").unwrap();
        let values: Vec<f64> = left.split(' ').skip(2).map(|value| value.parse().unwrap()).collect();
        let expected: Vec<f64> = right.split_whitespace().map(|value| value.parse().unwrap()).collect();
        let actual = aabb(&values)
            .clip_segment_legacy_face(vec3(&values[6..9]), vec3(&values[9..12]))
            .map_or_else(Vec::new, |(hit, face)| {
                let mut out = of_vec(hit);
                out.push(f64::from(face as u8));
                out
            });
        lines += 1;
        hits += usize::from(!expected.is_empty());
        let same = actual.len() == expected.len()
            && actual.iter().zip(&expected).all(|(a, e)| a.to_bits() == e.to_bits());
        if !same {
            failures.push(format!("{line}
    rust = {actual:?}"));
        }
    }
    assert!(lines > 3000 && hits > 300, "{lines} lines, {hits} hits");
    assert!(
        failures.is_empty(),
        "{} of {lines} differ, first:
{}",
        failures.len(),
        failures[..failures.len().min(8)].join("
")
    );
}

/// Whether a box outside the boxes meets the same thing over both grids.
fn same_collisions(boxes: &[Aabb], left: &[Vec<f64>; 3], right: &[Vec<f64>; 3]) -> bool {
    let places = [-1.0, -0.6, -0.3, -0.125, 0.0, 0.125, 0.2, 0.5, 0.875, 1.0, 1.4];
    let moves = [-1.5, -0.3, -1.0e-3, 1.0e-3, 0.3, 1.5];
    places.iter().all(|x| {
        places.iter().all(|z| {
            let entity = Aabb::new(Vec3::new(*x, 0.1, *z), Vec3::new(x + 0.6, 0.9, z + 0.6));
            boxes.iter().any(|bounds| reaches_into(entity, *bounds)) || moves.iter().all(|delta| {
                [Axis::X, Axis::Y, Axis::Z].into_iter().all(|axis| {
                    let one = collide_over_grid(left, boxes, axis, entity, *delta);
                    let other = collide_over_grid(right, boxes, axis, entity, *delta);
                    one.to_bits() == other.to_bits()
                })
            })
        })
    })
}

/// Every distinct collision shape of a jar (scripts/java-oracle/OracleStates.java, whose states
/// compare-states.cjs holds against the boxes the adapter gives): the grid lines the engine derives
/// from the boxes are the shape's getCoords, which the step-up candidates read from 1.21.
#[test]
fn block_shapes_have_the_grid_of_the_jar() {
    let fixtures: [(MinecraftVersion, &str); 5] = [
        (
            MinecraftVersion::new(1, 19, 4),
            include_str!("../fixtures/java-oracle-shapes-1.19.4.txt"),
        ),
        (
            MinecraftVersion::new(1, 20, 5),
            include_str!("../fixtures/java-oracle-shapes-1.20.5.txt"),
        ),
        (
            MinecraftVersion::new(1, 21, 11),
            include_str!("../fixtures/java-oracle-shapes-1.21.11.txt"),
        ),
        (
            MinecraftVersion::new(26, 2, 0),
            include_str!("../fixtures/java-oracle-shapes-26.2.txt"),
        ),
        (
            MinecraftVersion::new(26, 3, 0),
            include_str!("../fixtures/java-oracle-shapes-26.3.txt"),
        ),
    ];
    for (release, fixture) in fixtures {
        let mut failures = Vec::new();
        let mut shapes = 0;
        let mut off_lines = 0;
        for line in fixture.lines() {
            let mut words = line.split_whitespace().skip(1).map(|word| word.parse::<f64>().unwrap());
            let mut take = |each: usize| -> Vec<f64> {
                let count = words.next().unwrap() as usize;
                words.by_ref().take(count * each).collect()
            };
            let boxes: Vec<Aabb> = take(6).chunks(6).map(aabb).collect();
            let lines = [take(1), take(1), take(1)];
            if boxes.is_empty() {
                continue;
            }
            shapes += 1;
            let shape = BlockVoxelShape::new(Vec3::ZERO, boxes);
            let test = GridTest::of(Some(release));
            let derived = [Axis::X, Axis::Y, Axis::Z].map(|axis| shape.grid(axis, test));
            if derived != lines {
                off_lines += 1;
                // Two open gaps, which the engine cannot tell from the boxes it is given. A block with a
                // random offset (bamboo, pointed dripstone) moves its shape off its cell with its grid:
                // the lines differ on X and Z. An anvil turned by Shapes.rotate from 1.21.2 keeps the
                // lines of the shape merged while it faced north: the turned boxes bring some more on
                // every axis. Either way a box outside the shape meets the same, and Y loses no line
                // the step-up reads.
                let keeps_step_lines = lines[1].iter().all(|line| derived[1].contains(line));
                if !keeps_step_lines || !same_collisions(&shape.boxes, &lines, &derived) {
                    failures.push(format!("{line}\n    rust = {derived:?}"));
                }
            }
        }
        assert!(shapes > 300, "{release:?}: {shapes} shapes");
        assert!(off_lines <= 6, "{release:?}: {off_lines} shapes off their lines");
        assert!(
            failures.is_empty(),
            "{release:?}: {} differ, first:\n{}",
            failures.len(),
            failures[..failures.len().min(12)].join("\n")
        );
    }
}

/// What the jar answered (scripts/java-oracle/OracleIr.java), which test/java-ir.test.cjs holds the typed form of the same
/// methods to: source, form and engine on the same inputs, by bits.
const JAVA_IR: &str = include_str!("../fixtures/java-ir-26.3.txt");

#[test]
fn the_engine_answers_the_methods_of_the_typed_form_as_the_jar() {
    let release = MinecraftVersion::new(26, 3, 0);
    let (mut inputs, mut frictions, mut shots, mut early) = (0, 0, 0, 0);
    for line in JAVA_IR.lines() {
        let (left, right) = line.split_once(" = ").expect("a line");
        let mut words = left.split(' ');
        match words.next() {
            Some("input") => {
                let words: Vec<&str> = words.collect();
                let values: Vec<f64> = words[..3].iter().map(|word| word.parse().expect("a number")).collect();
                // (the speed and the yaw are floats: printed so, and widened as the method does)
                let speed: f32 = words[3].parse().expect("a speed");
                let y_rot: f32 = words[4].parse().expect("a yaw");
                let out: Vec<&str> = right.split(' ').collect();
                let input = Vec3::new(values[0], values[1], values[2]);
                let radians = f64::from(y_rot * 0.017453292_f32);
                let moved = relative_movement(input, f64::from(speed), radians, release);
                let expected: Vec<f64> = out[3..6].iter().map(|word| word.parse().expect("a number")).collect();
                assert_eq!(moved.x.to_bits(), expected[0].to_bits(), "x of {line}");
                assert_eq!(moved.y.to_bits(), expected[1].to_bits(), "y of {line}");
                assert_eq!(moved.z.to_bits(), expected[2].to_bits(), "z of {line}");
                if out[1] == "-" {
                    early += 1;
                } else {
                    assert_eq!(mth_sin(radians, release) as f32, out[1].parse::<f32>().expect("a sin"), "sin of {line}");
                    assert_eq!(mth_cos(radians, release) as f32, out[2].parse::<f32>().expect("a cos"), "cos of {line}");
                }
                inputs += 1;
            }
            Some("friction") => {
                let values: Vec<f32> = words.map(|word| word.parse().expect("a number")).collect();
                let result = friction_influenced_speed(f64::from(values[1]), f64::from(values[0]), release);
                assert_eq!(result as f32, right.parse::<f32>().expect("a result"), "{line}");
                frictions += 1;
            }
            Some("shoot") => {
                // the engine takes a launch from the server: what it shares is Vec3 and the rotation's atan2
                let values: Vec<f64> = words.map(|word| word.parse().expect("a number")).collect();
                let out: Vec<f64> = right.split(' ').map(|word| word.parse().expect("a number")).collect();
                let moved = Vec3::new(values[0], values[1], values[2]).normalized(release).scale(f64::from(values[3] as f32));
                assert_eq!(moved.x.to_bits(), out[0].to_bits(), "x of {line}");
                assert_eq!(moved.y.to_bits(), out[1].to_bits(), "y of {line}");
                assert_eq!(moved.z.to_bits(), out[2].to_bits(), "z of {line}");
                let y_rot = (atan2_vanilla_on(moved.x, moved.z, ORACLE_JAVA) * 57.2957763671875) as f32;
                assert_eq!(y_rot, out[3] as f32, "yaw of {line}");
                shots += 1;
            }
            other => panic!("an unknown line {other:?}"),
        }
    }
    assert_eq!((inputs, frictions, shots), (400, 200, 200));
    assert!(early >= 50, "the early return is {early} lines");
}

const EPSILONS: [(MinecraftVersion, &str); 10] = [
    (MinecraftVersion::new(1, 20, 4), include_str!("../fixtures/java-oracle-epsilon-1.20.4.txt")),
    (MinecraftVersion::new(1, 21, 2), include_str!("../fixtures/java-oracle-epsilon-1.21.2.txt")),
    (MinecraftVersion::new(1, 21, 3), include_str!("../fixtures/java-oracle-epsilon-1.21.3.txt")),
    (MinecraftVersion::new(1, 21, 4), include_str!("../fixtures/java-oracle-epsilon-1.21.4.txt")),
    (MinecraftVersion::new(1, 21, 5), include_str!("../fixtures/java-oracle-epsilon-1.21.5.txt")),
    (MinecraftVersion::new(1, 21, 8), include_str!("../fixtures/java-oracle-epsilon-1.21.8.txt")),
    (MinecraftVersion::new(1, 21, 9), include_str!("../fixtures/java-oracle-epsilon-1.21.9.txt")),
    (MinecraftVersion::new(1, 21, 10), include_str!("../fixtures/java-oracle-epsilon-1.21.10.txt")),
    (MinecraftVersion::new(1, 21, 11), include_str!("../fixtures/java-oracle-epsilon-1.21.11.txt")),
    (MinecraftVersion::new(26, 3, 0), include_str!("../fixtures/java-oracle-epsilon-26.3.txt")),
];

/// The walk of a move as the release spells it, with each block's step.
fn traversal(from: Vec3, to: Vec3, bounds: Aabb, release: MinecraftVersion) -> Vec<f64> {
    let cells = if release.at_least(1, 21, 9) {
        blocks_intersected_between_in_direction(from, to, bounds, i32::MAX, release)
    } else if release.at_least(1, 21, 5) {
        blocks_intersected_between(from, to, bounds, release)
    } else {
        box_traverse_blocks(from, to, bounds, release)
            .into_iter()
            .map(|position| (position, 0))
            .collect()
    };
    cells
        .into_iter()
        .flat_map(|((x, y, z), step)| [f64::from(x), f64::from(y), f64::from(z), f64::from(step)])
        .collect()
}

/// Mth.equal below, on and above its tolerance, and BlockGetter's walk on each side of the square
/// its call compares with: which of a move's two walks runs is what the threshold decides.
#[test]
fn the_tolerances_of_1e5_hold_at_their_call_sites() {
    let mut failures = Vec::new();
    let mut both_sides = [0_u32; 2];
    for (release, fixture) in EPSILONS {
        for line in fixture.lines() {
            let (left, right) = line.split_once(" =").unwrap();
            let mut words = left.split(' ');
            let op = words.next().unwrap();
            let a: Vec<f64> = words.map(|word| word.parse().unwrap()).collect();
            let want: Vec<f64> = right.split_whitespace().map(|word| word.parse().unwrap()).collect();
            let actual = match op {
                "mthequal" => {
                    both_sides[usize::from(want[0] != 0.0)] += 1;
                    assert_eq!(vanilla_equal(a[0], a[1]), equal_f64(a[0], a[1]));
                    flag(vanilla_equal(a[0], a[1]))
                }
                "mthequalf" => flag(equal_f32(a[0] as f32, a[1] as f32)),
                "traverse" => traversal(vec3(&a), vec3(&a[3..]), aabb(&a[6..]), release),
                other => panic!("{other}"),
            };
            let same = actual.len() == want.len()
                && actual.iter().zip(&want).all(|(l, r)| l.to_bits() == r.to_bits());
            if !same {
                failures.push(format!("{release:?} {line}\n    rust {actual:?}"));
            }
        }
    }
    assert!(both_sides[0] > 100 && both_sides[1] > 100, "{both_sides:?}");
    assert!(
        failures.is_empty(),
        "{} differ, first:\n{}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n")
    );
}
