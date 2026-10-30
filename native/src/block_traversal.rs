use std::collections::HashSet;

use crate::math::{Aabb, Vec3};
use crate::version::MinecraftVersion;

type BlockPosition = (i32, i32, i32);

const CELL_INSET: f64 = 9.999999747378752e-6;

/// A move as Entity records it for the inside-block effects. From 1.21.5 a recorded move is
/// checked one axis at a time, in the axis order of `axis_order`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Movement {
    pub from: Vec3,
    pub to: Vec3,
    pub axis_order: Option<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn of(self, vector: Vec3) -> f64 {
        match self {
            Self::X => vector.x,
            Self::Y => vector.y,
            Self::Z => vector.z,
        }
    }

    fn of_cell(self, (x, y, z): BlockPosition) -> i32 {
        match self {
            Self::X => x,
            Self::Y => y,
            Self::Z => z,
        }
    }

    fn step(self, (x, y, z): BlockPosition, distance: i32) -> BlockPosition {
        match self {
            Self::X => (x + distance, y, z),
            Self::Y => (x, y + distance, z),
            Self::Z => (x, y, z + distance),
        }
    }

    fn relative(self, position: Vec3, distance: f64) -> Vec3 {
        match self {
            Self::X => Vec3::new(position.x + distance, position.y, position.z),
            Self::Y => Vec3::new(position.x, position.y + distance, position.z),
            Self::Z => Vec3::new(position.x, position.y, position.z + distance),
        }
    }
}

/// Direction.axisStepOrder: Y first, then the horizontal axis that moves less.
fn axis_step_order(movement: Vec3) -> [Axis; 3] {
    if movement.x.abs() < movement.z.abs() {
        [Axis::Y, Axis::Z, Axis::X]
    } else {
        [Axis::Y, Axis::X, Axis::Z]
    }
}

/// The blocks Entity.checkInsideBlocks visits from 1.21.2, each once and in vanilla's order, with
/// whether it touches the box at the end of its step (isPrecise, which only bubble columns read).
pub(crate) fn blocks_visited(
    movements: &[Movement],
    final_box: Aabb,
    half_width: f64,
    height: f64,
    release: MinecraftVersion,
) -> Vec<(BlockPosition, bool)> {
    let box_at = |position: Vec3| {
        Aabb::new(
            Vec3::new(position.x - half_width, position.y, position.z - half_width),
            Vec3::new(
                position.x + half_width,
                position.y + height,
                position.z + half_width,
            ),
        )
        .deflate(Vec3::new(CELL_INSET, CELL_INSET, CELL_INSET))
    };
    let mut visited = HashSet::new();
    let mut blocks = Vec::new();
    let mut visit = |position: BlockPosition, precise: bool| {
        if visited.insert(position) {
            blocks.push((position, precise));
        }
    };
    if !release.at_least(1, 21, 5) {
        let inside = final_box.deflate(Vec3::new(CELL_INSET, CELL_INSET, CELL_INSET));
        for movement in movements {
            for position in box_traverse_blocks(movement.from, movement.to, inside) {
                visit(position, true);
            }
        }
        return blocks;
    }
    let precise_steps = release.at_least(1, 21, 10);
    for movement in movements {
        let delta = movement.to - movement.from;
        let mut steps = Vec::new();
        match movement.axis_order {
            Some(order) if delta.length_squared() > 0.0 => {
                let mut position = movement.from;
                for axis in axis_step_order(order) {
                    let distance = axis.of(delta);
                    if distance != 0.0 {
                        let to = axis.relative(position, distance);
                        steps.push((position, to));
                        position = to;
                    }
                }
            }
            _ => steps.push((movement.from, movement.to)),
        }
        if !release.at_least(1, 21, 9) {
            for (from, to) in steps {
                for position in blocks_intersected_between(from, to, box_at(to)) {
                    visit(position, true);
                }
            }
            continue;
        }
        let mut budget = 16;
        for (from, to) in steps {
            let target = box_at(to);
            let moved_far = from.distance_squared(to) > 0.9999900000002526 * 0.9999900000002526;
            let mut last_iteration = 0;
            for (position, iteration) in blocks_intersected_between_in_direction(from, to, target) {
                if iteration >= budget {
                    break;
                }
                last_iteration = iteration;
                let precise = !precise_steps || moved_far || intersects_cell(target, position);
                visit(position, precise);
            }
            budget -= last_iteration + 1;
        }
        if budget <= 0 {
            for position in between_closed(box_at(movement.to)) {
                visit(
                    position,
                    !precise_steps || intersects_cell(box_at(movement.to), position),
                );
            }
        }
    }
    blocks
}

fn intersects_cell(bounds: Aabb, (x, y, z): BlockPosition) -> bool {
    bounds.intersects(Aabb::new(
        Vec3::new(f64::from(x), f64::from(y), f64::from(z)),
        Vec3::new(f64::from(x + 1), f64::from(y + 1), f64::from(z + 1)),
    ))
}

/// BlockPos.betweenClosed: X fastest, then Y, then Z.
fn between_closed(bounds: Aabb) -> Vec<BlockPosition> {
    let (min, max) = (block_of(bounds.min), block_of(bounds.max));
    let mut blocks = Vec::new();
    for z in min.2..=max.2 {
        for y in min.1..=max.1 {
            for x in min.0..=max.0 {
                blocks.push((x, y, z));
            }
        }
    }
    blocks
}

fn block_of(position: Vec3) -> BlockPosition {
    (
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

/// BlockGetter.boxTraverseBlocks (1.21.2 - 1.21.4).
fn box_traverse_blocks(from: Vec3, to: Vec3, bounds: Aabb) -> Vec<BlockPosition> {
    let travel = to - from;
    if travel.length_squared() < f64::from(0.99999_f32 * 0.99999_f32) {
        return between_closed(bounds);
    }
    let mut seen = HashSet::new();
    let mut blocks = Vec::new();
    let mut add = |position: BlockPosition| {
        if seen.insert(position) {
            blocks.push(position);
        }
    };
    let nudge = normalize(travel).scale(1.0e-7);
    let end = bounds.min + nudge;
    let start = bounds.min - travel - nudge;
    walk_cells(start, end, Some(16), |cell, hit| {
        let reach = block_of(hit + (bounds.max - bounds.min));
        for x in cell.0..=reach.0 {
            for y in cell.1..=reach.1 {
                for z in cell.2..=reach.2 {
                    add((x, y, z));
                }
            }
        }
    });
    for position in between_closed(bounds) {
        add(position);
    }
    blocks
}

/// BlockGetter.forEachBlockIntersectedBetween of 1.21.5 - 1.21.8.
fn blocks_intersected_between(from: Vec3, to: Vec3, bounds: Aabb) -> Vec<BlockPosition> {
    let travel = to - from;
    if travel.length_squared() < f64::from(0.99999_f32 * 0.99999_f32) {
        return between_closed(bounds);
    }
    let mut seen = HashSet::new();
    let mut blocks = Vec::new();
    walk_cells(bounds.min - travel, bounds.min, Some(16), |cell, hit| {
        let reach = block_of(hit + (bounds.max - bounds.min));
        for x in cell.0..=reach.0 {
            for y in cell.1..=reach.1 {
                for z in cell.2..=reach.2 {
                    if seen.insert((x, y, z)) {
                        blocks.push((x, y, z));
                    }
                }
            }
        }
    });
    for position in between_closed(bounds) {
        if seen.insert(position) {
            blocks.push(position);
        }
    }
    blocks
}

/// BlockGetter.forEachBlockIntersectedBetween from 1.21.9, with each block's iteration: the box
/// before the move, the blocks its leading corner sweeps and the box after it, each in the
/// direction of the move.
fn blocks_intersected_between_in_direction(
    from: Vec3,
    to: Vec3,
    bounds: Aabb,
) -> Vec<(BlockPosition, i32)> {
    let travel = to - from;
    if travel.length_squared() < f64::from(1.0e-5_f32 * 1.0e-5_f32) {
        return between_closed(bounds)
            .into_iter()
            .map(|position| (position, 0))
            .collect();
    }
    let mut seen = HashSet::new();
    let mut blocks = Vec::new();
    let back = Vec3::new(-travel.x, -travel.y, -travel.z);
    let start = bounds.translated(back);
    for position in corners_in_direction(block_of(start.min), block_of(start.max), travel) {
        seen.insert(position);
        blocks.push((position, 0));
    }
    let size = bounds.max - bounds.min;
    let corner = furthest_corner(travel);
    let center = Vec3::new(
        bounds.min.x + 0.5 * (bounds.max.x - bounds.min.x),
        bounds.min.y + 0.5 * (bounds.max.y - bounds.min.y),
        bounds.min.z + 0.5 * (bounds.max.z - bounds.min.z),
    );
    let to_corner = Vec3::new(
        center.x + size.x * 0.5 * f64::from(corner.0),
        center.y + size.y * 0.5 * f64::from(corner.1),
        center.z + size.z * 0.5 * f64::from(corner.2),
    );
    let from_corner = to_corner - travel;
    let mut iterations = 0;
    walk_cells(from_corner, to_corner, None, |cell, hit| {
        iterations += 1;
        let opposite = block_of(Vec3::new(
            hit.x - size.x * f64::from(corner.0),
            hit.y - size.y * f64::from(corner.1),
            hit.z - size.z * f64::from(corner.2),
        ));
        for position in corners_in_direction(cell, opposite, travel) {
            if seen.insert(position) {
                blocks.push((position, iterations));
            }
        }
    });
    for position in corners_in_direction(block_of(bounds.min), block_of(bounds.max), travel) {
        if seen.insert(position) {
            blocks.push((position, iterations + 1));
        }
    }
    blocks
}

/// The cells a segment crosses after its first (BlockGetter.addCollisionsAlongTravel): `visit`
/// gets each cell the segment enters and where it enters it, nudged inside the cell.
fn walk_cells(
    from: Vec3,
    to: Vec3,
    max_steps: Option<i32>,
    mut visit: impl FnMut(BlockPosition, Vec3),
) {
    let delta = to - from;
    let mut cell = block_of(from);
    let sign = |value: f64| {
        if value == 0.0 {
            0
        } else if value > 0.0 {
            1
        } else {
            -1
        }
    };
    let signs = (sign(delta.x), sign(delta.y), sign(delta.z));
    let step_of = |sign: i32, delta: f64| {
        if sign == 0 {
            f64::MAX
        } else {
            f64::from(sign) / delta
        }
    };
    let steps = (
        step_of(signs.0, delta.x),
        step_of(signs.1, delta.y),
        step_of(signs.2, delta.z),
    );
    let first = |sign: i32, step: f64, start: f64| {
        let fraction = start - start.floor();
        step * if sign > 0 { 1.0 - fraction } else { fraction }
    };
    let mut t = (
        first(signs.0, steps.0, from.x),
        first(signs.1, steps.1, from.y),
        first(signs.2, steps.2, from.z),
    );
    let mut count = 0;
    while t.0 <= 1.0 || t.1 <= 1.0 || t.2 <= 1.0 {
        if t.0 < t.1 {
            if t.0 < t.2 {
                cell.0 += signs.0;
                t.0 += steps.0;
            } else {
                cell.2 += signs.2;
                t.2 += steps.2;
            }
        } else if t.1 < t.2 {
            cell.1 += signs.1;
            t.1 += steps.1;
        } else {
            cell.2 += signs.2;
            t.2 += steps.2;
        }
        if let Some(limit) = max_steps {
            if count > limit {
                break;
            }
            count += 1;
        }
        let min = Vec3::new(f64::from(cell.0), f64::from(cell.1), f64::from(cell.2));
        let max = Vec3::new(min.x + 1.0, min.y + 1.0, min.z + 1.0);
        if let Some(hit) = clip(min, max, from, to) {
            let inside =
                |value: f64, low: f64| value.clamp(low + CELL_INSET, low + 1.0 - CELL_INSET);
            visit(
                cell,
                Vec3::new(
                    inside(hit.x, min.x),
                    inside(hit.y, min.y),
                    inside(hit.z, min.z),
                ),
            );
        }
    }
}

/// BlockPos.betweenCornersInDirection: from the corner the move leaves, the first axis of
/// Direction.axisStepOrder outermost.
fn corners_in_direction(
    first: BlockPosition,
    second: BlockPosition,
    direction: Vec3,
) -> Vec<BlockPosition> {
    let min = (
        first.0.min(second.0),
        first.1.min(second.1),
        first.2.min(second.2),
    );
    let max = (
        first.0.max(second.0),
        first.1.max(second.1),
        first.2.max(second.2),
    );
    let start = (
        if direction.x >= 0.0 { min.0 } else { max.0 },
        if direction.y >= 0.0 { min.1 } else { max.1 },
        if direction.z >= 0.0 { min.2 } else { max.2 },
    );
    let [outer, middle, inner] = axis_step_order(direction);
    let sign = |axis: Axis| if axis.of(direction) >= 0.0 { 1 } else { -1 };
    let span = |axis: Axis| axis.of_cell(max) - axis.of_cell(min);
    let mut blocks = Vec::new();
    for i in 0..=span(outer) {
        for j in 0..=span(middle) {
            for k in 0..=span(inner) {
                let position = outer.step(start, sign(outer) * i);
                let position = middle.step(position, sign(middle) * j);
                blocks.push(inner.step(position, sign(inner) * k));
            }
        }
    }
    blocks
}

/// BlockGetter.getFurthestCorner.
fn furthest_corner(direction: Vec3) -> (i32, i32, i32) {
    let (x, y, z) = (direction.x.abs(), direction.y.abs(), direction.z.abs());
    let sign = |value: f64| if value >= 0.0 { 1 } else { -1 };
    let (x_sign, y_sign, z_sign) = (sign(direction.x), sign(direction.y), sign(direction.z));
    if x <= y && x <= z {
        (-x_sign, -z_sign, y_sign)
    } else if y <= z {
        (z_sign, -y_sign, -x_sign)
    } else {
        (-y_sign, x_sign, -z_sign)
    }
}

fn normalize(vector: Vec3) -> Vec3 {
    let length = vector.length_squared().sqrt();
    if length < f64::from(1.0e-5_f32) {
        Vec3::ZERO
    } else {
        Vec3::new(vector.x / length, vector.y / length, vector.z / length)
    }
}

/// AABB.clip: where the segment enters the box through a face, if it does.
fn clip(min: Vec3, max: Vec3, from: Vec3, to: Vec3) -> Option<Vec3> {
    let delta = to - from;
    let mut scale = 1.0;
    let mut hit = false;
    let mut face = |d: [f64; 3], plane: f64, b: (f64, f64), c: (f64, f64), start: [f64; 3]| {
        let s = (plane - start[0]) / d[0];
        let pb = start[1] + s * d[1];
        let pc = start[2] + s * d[2];
        if 0.0 < s
            && s < scale
            && b.0 - 1.0e-7 < pb
            && pb < b.1 + 1.0e-7
            && c.0 - 1.0e-7 < pc
            && pc < c.1 + 1.0e-7
        {
            scale = s;
            hit = true;
        }
    };
    let x_first = [delta.x, delta.y, delta.z];
    let x_start = [from.x, from.y, from.z];
    if delta.x > 1.0e-7 {
        face(x_first, min.x, (min.y, max.y), (min.z, max.z), x_start);
    } else if delta.x < -1.0e-7 {
        face(x_first, max.x, (min.y, max.y), (min.z, max.z), x_start);
    }
    let y_first = [delta.y, delta.z, delta.x];
    let y_start = [from.y, from.z, from.x];
    if delta.y > 1.0e-7 {
        face(y_first, min.y, (min.z, max.z), (min.x, max.x), y_start);
    } else if delta.y < -1.0e-7 {
        face(y_first, max.y, (min.z, max.z), (min.x, max.x), y_start);
    }
    let z_first = [delta.z, delta.x, delta.y];
    let z_start = [from.z, from.x, from.y];
    if delta.z > 1.0e-7 {
        face(z_first, min.z, (min.x, max.x), (min.y, max.y), z_start);
    } else if delta.z < -1.0e-7 {
        face(z_first, max.z, (min.x, max.x), (min.y, max.y), z_start);
    }
    hit.then(|| {
        Vec3::new(
            from.x + scale * delta.x,
            from.y + scale * delta.y,
            from.z + scale * delta.z,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player_box(position: Vec3) -> Aabb {
        Aabb::new(
            Vec3::new(position.x - 0.3, position.y, position.z - 0.3),
            Vec3::new(position.x + 0.3, position.y + 1.8, position.z + 0.3),
        )
    }

    fn visited(movement: Movement, release: MinecraftVersion) -> Vec<BlockPosition> {
        blocks_visited(&[movement], player_box(movement.to), 0.3, 1.8, release)
            .into_iter()
            .map(|(position, _)| position)
            .collect()
    }

    #[test]
    fn a_long_move_visits_the_blocks_it_sweeps_before_the_box() {
        let fall = Movement {
            from: Vec3::new(0.5, 5.0, 0.5),
            to: Vec3::new(0.5, 2.0, 0.5),
            axis_order: None,
        };

        assert_eq!(
            visited(fall, MinecraftVersion::new(1, 21, 2)),
            vec![(0, 4, 0), (0, 5, 0), (0, 6, 0), (0, 3, 0), (0, 2, 0)]
        );
    }

    #[test]
    fn a_short_move_visits_the_box_x_first_then_y_then_z() {
        let step = Movement {
            from: Vec3::new(0.9, 1.0, 0.9),
            to: Vec3::new(1.0, 1.0, 1.0),
            axis_order: None,
        };
        let expected = vec![
            (0, 1, 0),
            (1, 1, 0),
            (0, 2, 0),
            (1, 2, 0),
            (0, 1, 1),
            (1, 1, 1),
            (0, 2, 1),
            (1, 2, 1),
        ];

        assert_eq!(visited(step, MinecraftVersion::new(1, 21, 4)), expected);
        assert_eq!(visited(step, MinecraftVersion::new(1, 21, 8)), expected);
    }

    #[test]
    fn moves_visit_the_box_they_leave_first_in_their_direction_from_1_21_9() {
        let sink = Movement {
            from: Vec3::new(0.5, 1.1, 0.5),
            to: Vec3::new(0.5, 0.8, 0.5),
            axis_order: Some(Vec3::new(0.0, -0.3, 0.0)),
        };

        assert_eq!(
            visited(sink, MinecraftVersion::new(1, 21, 9)),
            vec![(0, 2, 0), (0, 1, 0), (0, 0, 0)]
        );
    }

    #[test]
    fn blocks_off_the_box_at_the_end_of_a_step_are_not_precise_from_1_21_10() {
        let rise = Movement {
            from: Vec3::new(0.5, 0.8, 0.5),
            to: Vec3::new(0.5, 1.1, 0.5),
            axis_order: Some(Vec3::new(0.0, 0.3, 0.0)),
        };
        let precise = |release| {
            blocks_visited(&[rise], player_box(rise.to), 0.3, 1.8, release)
                .into_iter()
                .filter(|&(_, precise)| precise)
                .count()
        };

        assert_eq!(precise(MinecraftVersion::new(1, 21, 9)), 3);
        assert_eq!(precise(MinecraftVersion::new(1, 21, 10)), 2);
    }

    #[test]
    fn recorded_moves_are_checked_one_axis_at_a_time_from_1_21_5() {
        let diagonal = Movement {
            from: Vec3::new(0.9, 1.2, 0.5),
            to: Vec3::new(1.4, 0.7, 0.5),
            axis_order: Some(Vec3::new(0.5, -0.5, 0.0)),
        };

        assert!(!visited(diagonal, MinecraftVersion::new(1, 21, 4)).contains(&(0, 0, 0)));
        assert!(visited(diagonal, MinecraftVersion::new(1, 21, 5)).contains(&(0, 0, 0)));
    }
}
