use crate::fast_hash::FastSet;

use crate::math::{floor_i32_for, frac_f64_for, Aabb, Vec3};
use crate::version::MinecraftVersion;

type BlockPosition = (i32, i32, i32);

const CELL_INSET: f64 = 9.999999747378752e-6;

// The cell steps and candidate blocks a traversal has taken, which the tests read.
#[cfg(test)]
thread_local! {
    static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_work(amount: usize) {
    WORK.with(|work| work.set(work.get() + amount));
}

#[cfg(not(test))]
fn count_work(_amount: usize) {}

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
    /// The vector's coordinate on the axis (vanilla: Vec3#get, as the axis steps of a recorded
    /// move and BlockPos#betweenCornersInDirection read it).
    fn of(self, vector: Vec3) -> f64 {
        match self {
            Self::X => vector.x,
            Self::Y => vector.y,
            Self::Z => vector.z,
        }
    }

    /// The cell's coordinate on the axis (vanilla: Vec3i#get; BlockPos#betweenCornersInDirection
    /// takes the axis' choose of the corners' differences).
    fn of_cell(self, (x, y, z): BlockPosition) -> i32 {
        match self {
            Self::X => x,
            Self::Y => y,
            Self::Z => z,
        }
    }

    /// The cell `distance` cells along the axis: one axis' term of the cursor
    /// BlockPos#betweenCornersInDirection sets, its direction's step times its index.
    fn step(self, (x, y, z): BlockPosition, distance: i32) -> BlockPosition {
        match self {
            Self::X => (x + distance, y, z),
            Self::Y => (x, y + distance, z),
            Self::Z => (x, y, z + distance),
        }
    }

    /// The position moved `distance` along the axis (vanilla: Vec3#relative of the axis' positive
    /// direction, each axis step of a recorded move from 1.21.5).
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

/// A block Entity.checkInsideBlocks reaches: the step it is checked in (its box moved from `from`
/// to `to`) and the index BlockGetter.forEachBlockIntersectedBetween gives it, which
/// InsideBlockEffectApplier.StepBasedCollector.advanceStep groups the effects by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Visit {
    pub position: BlockPosition,
    pub precise: bool,
    pub from: Vec3,
    pub to: Vec3,
    pub step: i32,
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
    blocks_visited_where(movements, final_box, half_width, height, release, |_| true)
}

/// blocks_visited with the test checkInsideBlocks makes of each block it reaches (the block's
/// getEntityInsideCollisionShape or its fluid, swept along the step's move): a block that fails
/// it gets no entityInside. Before 1.21.9 it counts as visited all the same (1.21.5
/// Entity.java:1131, 1.21.8 :1161); from 1.21.9 only a block that passes does, and a later step
/// may reach it again (1.21.9 Entity.java:1244, 1.21.11 :1269).
pub(crate) fn blocks_visited_where(
    movements: &[Movement],
    final_box: Aabb,
    half_width: f64,
    height: f64,
    release: MinecraftVersion,
    mut counts: impl FnMut(&Visit) -> bool,
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
    let mut visited = FastSet::default();
    let mut blocks = Vec::new();
    // (1.21.8 Entity.java:1212; 1.21.9 Entity.java:1244)
    let marks_failed = !release.at_least(1, 21, 9);
    let mut visit = |visit: Visit| {
        if visited.contains(&visit.position) {
            return;
        }
        let passes = counts(&visit);
        if passes || marks_failed {
            visited.insert(visit.position);
        }
        if passes {
            blocks.push((visit.position, visit.precise));
        }
    };
    // (1.21.4 Entity.java:1105; 1.21.5 BlockGetter.java:164)
    if !release.at_least(1, 21, 5) {
        let inside = final_box.deflate(Vec3::new(CELL_INSET, CELL_INSET, CELL_INSET));
        for movement in movements {
            for position in box_traverse_blocks(movement.from, movement.to, inside, release) {
                visit(Visit {
                    position,
                    precise: true,
                    from: movement.from,
                    to: movement.to,
                    step: 0,
                });
            }
        }
        return blocks;
    }
    // (1.21.9 Entity.java:1248; 1.21.10 Entity.java:1252-1254)
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
        // (1.21.8 Entity.java:1187; 1.21.9 Entity.java:1244)
        if !release.at_least(1, 21, 9) {
            for (from, to) in steps {
                for (position, step) in blocks_intersected_between(from, to, box_at(to), release) {
                    visit(Visit {
                        position,
                        precise: true,
                        from,
                        to,
                        step,
                    });
                }
            }
            continue;
        }
        let mut budget = 16;
        for (from, to) in steps {
            let target = box_at(to);
            let moved_far = from.distance_squared(to) > 0.9999900000002526 * 0.9999900000002526;
            let mut last_iteration = 0;
            for (position, iteration) in
                blocks_intersected_between_in_direction(from, to, target, budget, release)
            {
                if iteration >= budget {
                    break;
                }
                last_iteration = iteration;
                let precise = !precise_steps || moved_far || intersects_cell(target, position);
                visit(Visit {
                    position,
                    precise,
                    from,
                    to,
                    step: iteration,
                });
            }
            budget -= last_iteration + 1;
        }
        if budget <= 0 {
            for position in between_closed(box_at(movement.to), release) {
                visit(Visit {
                    position,
                    precise: !precise_steps || intersects_cell(box_at(movement.to), position),
                    from: movement.to,
                    to: movement.to,
                    step: 0,
                });
            }
        }
    }
    blocks
}

/// AABB#intersects of a BlockPos: whether the box meets the cell, its far faces at the int sums
/// `x + 1` (isPrecise of the inside-block check).
fn intersects_cell(bounds: Aabb, (x, y, z): BlockPosition) -> bool {
    bounds.intersects(Aabb::new(
        Vec3::new(f64::from(x), f64::from(y), f64::from(z)),
        Vec3::new(
            f64::from(x.wrapping_add(1)),
            f64::from(y.wrapping_add(1)),
            f64::from(z.wrapping_add(1)),
        ),
    ))
}

/// BlockPos.betweenClosed: X fastest, then Y, then Z.
fn between_closed(bounds: Aabb, release: MinecraftVersion) -> Vec<BlockPosition> {
    let (min, max) = (block_of(bounds.min, release), block_of(bounds.max, release));
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

/// BlockPos#containing: the cell holding the position, each coordinate floored.
fn block_of(position: Vec3, release: MinecraftVersion) -> BlockPosition {
    (
        floor_i32_for(release, position.x),
        floor_i32_for(release, position.y),
        floor_i32_for(release, position.z),
    )
}

/// BlockGetter.boxTraverseBlocks (1.21.2 - 1.21.4).
pub(crate) fn box_traverse_blocks(
    from: Vec3,
    to: Vec3,
    bounds: Aabb,
    release: MinecraftVersion,
) -> Vec<BlockPosition> {
    let travel = to - from;
    if travel.length_squared() < f64::from(0.99999_f32 * 0.99999_f32) {
        return between_closed(bounds, release);
    }
    let mut seen = FastSet::default();
    let mut blocks = Vec::new();
    let mut add = |position: BlockPosition| {
        if seen.insert(position) {
            blocks.push(position);
        }
    };
    // the walk runs 1.0E-7 past each end on 1.21.2 and 1.21.3 (1.21.3 BlockGetter.java:175-177), and from
    // the box's corner to where it left on 1.21.4 (1.21.4 :171-173)
    let nudge = if release.at_least(1, 21, 4) {
        Vec3::ZERO
    } else {
        travel.normalized(release).scale(1.0e-7)
    };
    let end = bounds.min + nudge;
    let start = bounds.min - travel - nudge;
    walk_cells(start, end, Some(16), release, |cell, hit, _| {
        let reach = block_of(hit + (bounds.max - bounds.min), release);
        for x in cell.0..=reach.0 {
            for y in cell.1..=reach.1 {
                for z in cell.2..=reach.2 {
                    add((x, y, z));
                }
            }
        }
    });
    for position in between_closed(bounds, release) {
        add(position);
    }
    blocks
}

/// BlockGetter.forEachBlockIntersectedBetween of 1.21.5 - 1.21.8, with each block's index: 0 for
/// a short move, else the walk's step that reaches it, and one past the walk's last for the box
/// after the move (1.21.5 BlockGetter.java:164-184, 220-240).
pub(crate) fn blocks_intersected_between(
    from: Vec3,
    to: Vec3,
    bounds: Aabb,
    release: MinecraftVersion,
) -> Vec<(BlockPosition, i32)> {
    let travel = to - from;
    if travel.length_squared() < f64::from(0.99999_f32 * 0.99999_f32) {
        return between_closed(bounds, release)
            .into_iter()
            .map(|position| (position, 0))
            .collect();
    }
    let mut seen = FastSet::default();
    let mut blocks = Vec::new();
    let steps = walk_cells(
        bounds.min - travel,
        bounds.min,
        Some(16),
        release,
        |cell, hit, step| {
            let reach = block_of(hit + (bounds.max - bounds.min), release);
            for x in cell.0..=reach.0 {
                for y in cell.1..=reach.1 {
                    for z in cell.2..=reach.2 {
                        if seen.insert((x, y, z)) {
                            blocks.push(((x, y, z), step));
                        }
                    }
                }
            }
        },
    );
    for position in between_closed(bounds, release) {
        if seen.insert(position) {
            blocks.push((position, steps + 1));
        }
    }
    blocks
}

/// BlockGetter.forEachBlockIntersectedBetween from 1.21.9, with each block's iteration: the box
/// before the move, the blocks its leading corner sweeps and the box after it, each in the
/// direction of the move. The walk ends at `budget` iterations, as Entity.checkInsideBlocks' visitor
/// ends it (26.3 Entity.java:1371-1374, BlockGetter.java:161-193, 248-250).
pub(crate) fn blocks_intersected_between_in_direction(
    from: Vec3,
    to: Vec3,
    bounds: Aabb,
    budget: i32,
    release: MinecraftVersion,
) -> Vec<(BlockPosition, i32)> {
    let travel = to - from;
    if travel.length_squared() < f64::from(1.0e-5_f32 * 1.0e-5_f32) {
        return between_closed(bounds, release)
            .into_iter()
            .map(|position| (position, 0))
            .collect();
    }
    let mut seen = FastSet::default();
    let mut blocks = Vec::new();
    let back = Vec3::new(-travel.x, -travel.y, -travel.z);
    let start = bounds.translated(back);
    for position in corners_in_direction(block_of(start.min, release), block_of(start.max, release), travel) {
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
    // the signs and steps are the travel's own, not to_corner - from_corner, which can round away
    // from it (26.3 BlockGetter.java:197-213; 1.21.8 :193-200 subtracts)
    let finished = walk_cells_while(from_corner, to_corner, travel, None, release, |cell, hit, _| {
        iterations += 1;
        if iterations >= budget {
            return false;
        }
        let opposite = block_of(Vec3::new(
            hit.x - size.x * f64::from(corner.0),
            hit.y - size.y * f64::from(corner.1),
            hit.z - size.z * f64::from(corner.2),
        ), release);
        for position in corners_in_direction(cell, opposite, travel) {
            if seen.insert(position) {
                blocks.push((position, iterations));
            }
        }
        true
    });
    if !finished {
        return blocks;
    }
    for position in corners_in_direction(block_of(bounds.min, release), block_of(bounds.max, release), travel) {
        if seen.insert(position) {
            blocks.push((position, iterations + 1));
        }
    }
    blocks
}

/// The cells a segment crosses after its first (BlockGetter.addCollisionsAlongTravel): `visit`
/// gets each cell the segment enters, where it enters it, nudged inside the cell, and the step
/// count `p` so far, which the walk returns.
fn walk_cells(
    from: Vec3,
    to: Vec3,
    max_steps: Option<i32>,
    release: MinecraftVersion,
    mut visit: impl FnMut(BlockPosition, Vec3, i32),
) -> i32 {
    let mut steps = 0;
    walk_cells_while(from, to, to - from, max_steps, release, |cell, hit, step| {
        steps = step;
        visit(cell, hit, step);
        true
    });
    steps
}

/// `walk_cells` until `visit` says stop; whether the walk reached its end. `delta` gives the signs
/// and steps: `to - from` up to 1.21.8, the move itself from 1.21.9.
fn walk_cells_while(
    from: Vec3,
    to: Vec3,
    delta: Vec3,
    max_steps: Option<i32>,
    release: MinecraftVersion,
    mut visit: impl FnMut(BlockPosition, Vec3, i32) -> bool,
) -> bool {
    let mut cell = block_of(from, release);
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
        let fraction = frac_f64_for(release, start);
        step * if sign > 0 { 1.0 - fraction } else { fraction }
    };
    let mut t = (
        first(signs.0, steps.0, from.x),
        first(signs.1, steps.1, from.y),
        first(signs.2, steps.2, from.z),
    );
    let mut count = 0;
    while t.0 <= 1.0 || t.1 <= 1.0 || t.2 <= 1.0 {
        count_work(1);
        if t.0 < t.1 {
            if t.0 < t.2 {
                cell.0 = cell.0.wrapping_add(signs.0);
                t.0 += steps.0;
            } else {
                cell.2 = cell.2.wrapping_add(signs.2);
                t.2 += steps.2;
            }
        } else if t.1 < t.2 {
            cell.1 = cell.1.wrapping_add(signs.1);
            t.1 += steps.1;
        } else {
            cell.2 = cell.2.wrapping_add(signs.2);
            t.2 += steps.2;
        }
        if let Some(limit) = max_steps {
            // `if (p++ > 16) break;`
            count += 1;
            if count - 1 > limit {
                break;
            }
        }
        let min = Vec3::new(f64::from(cell.0), f64::from(cell.1), f64::from(cell.2));
        let max = Vec3::new(min.x + 1.0, min.y + 1.0, min.z + 1.0);
        if let Some(hit) = clip(min, max, from, to) {
            // `Mth.clamp(hit.x, i + 1.0E-5F, i + 1.0 - 1.0E-5F)`: the low bound is an int plus a float, so
            // a float, which far from the origin rounds to the cell's face itself; the high bound is a
            // double (1.21.2 BlockGetter.java:232, 1.21.11 :248, 26.3 :265)
            // (Mth.clamp is `d < e ? e : Math.min(d, f)`: a float low bound that rounds past the high one,
            // from 2^24 out, wins, where f64::clamp would panic; 1.21.2 Mth.java:105-107)
            let inside = |value: f64, low: f64| {
                let (low, high) = (f64::from(low as f32 + 1.0e-5_f32), low + 1.0 - CELL_INSET);
                if value < low { low } else { value.min(high) }
            };
            let hit = Vec3::new(
                inside(hit.x, min.x),
                inside(hit.y, min.y),
                inside(hit.z, min.z),
            );
            if !visit(cell, hit, count) {
                return false;
            }
        }
    }
    true
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
    count_work(blocks.len());
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
    use std::cell::Cell;

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
    fn a_hit_is_held_off_a_cells_low_face_by_a_float_sum_far_from_the_origin() {
        // 1001 + 1.0E-5F is 1001.0F: the float has no room for the inset there (26.3 BlockGetter.java:265)
        let mut hits = Vec::new();
        walk_cells(
            Vec3::new(1000.5, 0.5, 0.5),
            Vec3::new(1002.5, 0.5, 0.5),
            None,
            MinecraftVersion::new(1, 21, 11),
            |cell, hit, _| hits.push((cell, hit.x)),
        );
        assert_eq!(hits.first(), Some(&((1001, 0, 0), 1001.0)));
        // near the origin the inset is there, as a float widened
        let mut near = Vec::new();
        walk_cells(Vec3::new(0.5, 0.5, 0.5), Vec3::new(2.5, 0.5, 0.5), None, MinecraftVersion::new(1, 21, 11), |cell, hit, _| {
            near.push((cell, hit.x));
        });
        assert_eq!(near.first(), Some(&((1, 0, 0), f64::from(1.0_f32 + 1.0e-5_f32))));
        // past 2^24 the float of the cell can round up beyond the cell: the low bound wins, as Mth.clamp
        let mut far = Vec::new();
        walk_cells(Vec3::new(16_777_218.5, 0.5, 0.5), Vec3::new(16_777_220.5, 0.5, 0.5), None, MinecraftVersion::new(1, 21, 11), |cell, hit, _| {
            far.push((cell, hit.x));
        });
        assert_eq!(far.first(), Some(&((16_777_219, 0, 0), 16_777_220.0)));
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

    #[test]
    fn the_traversal_budget_ends_the_walk_itself() {
        // the visitor's false ends forEachBlockIntersectedBetween at the 16th iteration (26.3
        // Entity.java:1371-1374, BlockGetter.java:248-250), whatever the distance
        let from = Vec3::new(0.5, 64.0, 0.5);
        let at = |to: Vec3| Aabb::new(to - Vec3::new(0.3, 0.0, 0.3), to + Vec3::new(0.3, 1.8, 0.3));
        let budgeted = |distance: f64, budget: i32| {
            let to = from + Vec3::new(distance, 0.0, 0.0);
            let release = MinecraftVersion::new(26, 3, 0);
            blocks_intersected_between_in_direction(from, to, at(to), budget, release)
        };
        assert!(budgeted(100_000.0, 16).len() < 100);
        assert!(budgeted(100_000.0, 16).iter().all(|&(_, iteration)| iteration < 16));
        // what the budget lets through is the walk's own, block for block
        let unlimited = budgeted(1_000.0, i32::MAX);
        let limited = budgeted(1_000.0, 16);
        assert!(limited.len() < unlimited.len());
        assert_eq!(limited[..], unlimited[..limited.len()]);
        assert_eq!(budgeted(5.0, 16), budgeted(5.0, i32::MAX));
    }

    fn moved(from: (f64, f64, f64), to: (f64, f64, f64)) -> Movement {
        Movement {
            from: Vec3::new(from.0, from.1, from.2),
            to: Vec3::new(to.0, to.1, to.2),
            axis_order: None,
        }
    }

    fn by_axes(mut movement: Movement) -> Movement {
        movement.axis_order = Some(movement.to - movement.from);
        movement
    }

    /// The candidates and cell steps a visit takes.
    fn work_of(movements: &[Movement], release: MinecraftVersion) -> (usize, usize) {
        WORK.with(|work| work.set(0));
        let final_box = player_box(movements[movements.len() - 1].to);
        let visits = blocks_visited(movements, final_box, 0.3, 1.8, release).len();
        (WORK.with(Cell::get), visits)
    }

    fn visited_all(movements: &[Movement], release: MinecraftVersion) -> Vec<BlockPosition> {
        let final_box = player_box(movements[movements.len() - 1].to);
        blocks_visited(movements, final_box, 0.3, 1.8, release)
            .into_iter()
            .map(|(position, _)| position)
            .collect()
    }

    const FROM_1_21_9: [MinecraftVersion; 3] = [
        MinecraftVersion::new(1, 21, 9),
        MinecraftVersion::new(1, 21, 11),
        MinecraftVersion::new(26, 3, 0),
    ];

    #[test]
    fn the_work_counter_adds_what_a_traversal_counts() {
        WORK.with(|work| work.set(0));
        count_work(1);
        count_work(3);
        assert_eq!(WORK.with(Cell::get), 4);
    }

    #[test]
    fn the_work_of_a_long_move_does_not_grow_with_its_length() {
        // The visitor's false ends forEachBlockIntersectedBetween at the 16th iteration (26.3
        // Entity.java:1371-1374, BlockGetter.java:248-250): the walk's steps and candidates stop
        // there, where the audit counted 200,002 for a distance of 100,000 (34 visits)
        let release = MinecraftVersion::new(26, 3, 0);
        let east = |distance: f64| moved((0.5, 64.0, 0.5), (0.5 + distance, 64.0, 0.5));
        let (work, visits) = work_of(&[east(100_000.0)], release);
        assert_eq!(visits, 34);
        assert!(work < 100, "{work}");
        assert_eq!(work_of(&[east(1_000.0)], release), (work, visits));
        assert_eq!(work_of(&[east(100_000_000.0)], release), (work, visits));

        for (from, direction) in [
            ((0.5, 64.0, 0.5), (0.7, -0.5, 0.3)),
            ((-3.5, -8.0, -3.5), (-0.3, 0.5, -0.7)),
            ((-0.5, 64.0, -0.5), (-1.0, 0.0, 0.0)),
            ((0.5, 64.0, 0.5), (0.0, -1.0, 0.0)),
        ] {
            for distance in [1_000.0, 100_000.0, 10_000_000.0] {
                let to = (
                    from.0 + direction.0 * distance,
                    from.1 + direction.1 * distance,
                    from.2 + direction.2 * distance,
                );
                for movement in [moved(from, to), by_axes(moved(from, to))] {
                    let (work, visits) = work_of(&[movement], release);
                    // 16 iterations of three axes, and a box of 2 x 3 x 2 at each
                    assert!(work < 16 * 3 * 12, "{work} for {distance}");
                    assert!(visits < 100, "{visits} for {distance}");
                }
            }
        }
    }

    #[test]
    fn the_older_walk_keeps_its_own_limit_on_the_other_side_of_1_21_9() {
        // `if (p++ > 16) break`, then the box after the move (1.21.8 BlockGetter.java:178-188,
        // 234-236): the walk from the corner at the box's minimum reaches the cells 1 - 17, its first
        // cell left out, and no budget ends the check (1.21.8 Entity.java:1152-1175)
        let long = moved((0.5, 64.0, 0.5), (100_000.5, 64.0, 0.5));
        let mut expected: Vec<_> = (1..=17).flat_map(|x| [(x, 64, 0), (x, 65, 0)]).collect();
        expected.extend([(100_000, 64, 0), (100_000, 65, 0)]);
        for release in [MinecraftVersion::new(1, 21, 5), MinecraftVersion::new(1, 21, 8)] {
            assert_eq!(visited_all(&[long], release), expected);
        }
        let (work, _) = work_of(&[long], MinecraftVersion::new(1, 21, 8));
        assert!(work < 100, "{work}");
        assert_eq!(
            work_of(&[moved((0.5, 64.0, 0.5), (1_000.5, 64.0, 0.5))], MinecraftVersion::new(1, 21, 8)).0,
            work
        );

        // from 1.21.9 the box the move leaves comes first, in the move's direction
        let mut expected: Vec<_> = (0..=15).flat_map(|x| [(x, 64, 0), (x, 65, 0)]).collect();
        expected.extend([(100_000, 64, 0), (100_000, 65, 0)]);
        assert_eq!(visited_all(&[long], MinecraftVersion::new(1, 21, 9)), expected);
    }

    #[test]
    fn the_budget_runs_out_and_falls_back_on_the_final_box_at_its_exact_length() {
        // Iteration 16 is out of a budget of 16 (26.3 Entity.java:1374): at 15 cells the box after the
        // move is the walk's own, at 16 the last accepted iteration spends it all and
        // checkInsideBlocks(to, to, 1) reads the box, at 17 that box is past the cells reached
        // (26.3 Entity.java:1320-1345)
        let along = |distance: f64| moved((0.5, 64.0, 0.5), (0.5 + distance, 64.0, 0.5));
        let cells = |last: i32| (0..=last).flat_map(|x| [(x, 64, 0), (x, 65, 0)]).collect::<Vec<_>>();
        for release in FROM_1_21_9 {
            assert_eq!(visited_all(&[along(15.0)], release), cells(15));
            assert_eq!(visited_all(&[along(16.0)], release), cells(16));
            let mut past = cells(15);
            past.extend([(17, 64, 0), (17, 65, 0)]);
            assert_eq!(visited_all(&[along(17.0)], release), past);
        }
    }

    #[test]
    fn long_moves_keep_their_order_in_every_direction() {
        // Expected lists from a transliteration of 26.3 Entity.checkInsideBlocks,
        // BlockGetter.forEachBlockIntersectedBetween, BlockPos.betweenCornersInDirection and
        // AABB.clip. An iteration of the diagonal walks that gives no new block never meets the
        // visitor and spends no budget, so these two read no final box (26.3 BlockGetter.java:248-250,
        // Entity.java:1342)
        let west = by_axes(moved((-0.5, 64.0, -0.5), (-100_000.5, 64.0, -0.5)));
        let diagonal = moved((0.5, 64.0, 0.5), (70_000.5, -49_936.0, 30_000.5));
        let rising = moved((-3.5, -8.0, -3.5), (-30_003.5, 49_992.0, -70_003.5));
        let axes = by_axes(moved((0.5, 64.0, 0.5), (9.5, 55.0, 9.5)));
        for release in FROM_1_21_9 {
            assert_eq!(visited_all(&[west], release), vec![
                (-1, 64, -1), (-1, 65, -1), (-2, 64, -1), (-2, 65, -1), (-3, 64, -1), (-3, 65, -1),
                (-4, 64, -1), (-4, 65, -1), (-5, 64, -1), (-5, 65, -1), (-6, 64, -1), (-6, 65, -1),
                (-7, 64, -1), (-7, 65, -1), (-8, 64, -1), (-8, 65, -1), (-9, 64, -1), (-9, 65, -1),
                (-10, 64, -1), (-10, 65, -1), (-11, 64, -1), (-11, 65, -1), (-12, 64, -1), (-12, 65, -1),
                (-13, 64, -1), (-13, 65, -1), (-14, 64, -1), (-14, 65, -1), (-15, 64, -1), (-15, 65, -1),
                (-16, 64, -1), (-16, 65, -1), (-100001, 64, -1), (-100001, 65, -1),
            ]);
            assert_eq!(visited_all(&[diagonal], release), vec![
                (0, 65, 0), (0, 64, 0), (1, 65, 0), (1, 64, 0), (0, 63, 0), (1, 63, 0),
                (1, 64, 1), (1, 63, 1), (2, 64, 0), (2, 64, 1), (2, 63, 0), (2, 63, 1),
                (2, 62, 1), (3, 64, 1), (3, 63, 1), (3, 62, 1), (3, 63, 2), (4, 63, 1),
                (4, 63, 2), (3, 62, 2), (4, 62, 1), (4, 62, 2), (3, 61, 1), (3, 61, 2),
                (4, 61, 1), (4, 61, 2), (5, 62, 2), (5, 61, 2), (5, 62, 3), (6, 62, 2),
                (6, 62, 3), (5, 61, 3), (6, 61, 2), (6, 61, 3), (5, 60, 2), (5, 60, 3),
                (6, 60, 2), (6, 60, 3), (7, 61, 2), (7, 61, 3), (7, 60, 2), (7, 60, 3),
                (6, 59, 2), (6, 59, 3), (7, 59, 2), (7, 59, 3),
            ]);
            assert_eq!(visited_all(&[rising], release), vec![
                (-4, -8, -4), (-4, -7, -4), (-4, -8, -5), (-4, -7, -5), (-4, -6, -4), (-4, -6, -5),
                (-5, -8, -5), (-5, -7, -5), (-5, -6, -5), (-4, -7, -6), (-5, -7, -6), (-4, -6, -6),
                (-5, -6, -6), (-4, -5, -5), (-5, -5, -5), (-4, -5, -6), (-5, -5, -6), (-5, -6, -7),
                (-5, -5, -7), (-6, -6, -7), (-6, -5, -7), (-5, -4, -7), (-6, -4, -7), (-5, -6, -8),
                (-6, -6, -8), (-5, -5, -8), (-6, -5, -8), (-5, -4, -8), (-6, -4, -8), (-6, -5, -9),
                (-6, -4, -9), (-6, -3, -8), (-6, -3, -9), (-6, -4, -10), (-7, -4, -10), (-6, -3, -10),
                (-7, -3, -10), (-6, -2, -10), (-7, -2, -10), (-7, -4, -11), (-7, -3, -11), (-7, -2, -11),
            ]);
            assert_eq!(visited_all(&[axes], release), vec![
                (0, 65, 0), (0, 64, 0), (0, 63, 0), (0, 62, 0), (0, 61, 0), (0, 60, 0),
                (0, 59, 0), (0, 58, 0), (0, 57, 0), (0, 56, 0), (0, 55, 0), (1, 55, 0),
                (1, 56, 0), (2, 55, 0), (2, 56, 0), (3, 55, 0), (3, 56, 0), (4, 55, 0),
                (4, 56, 0), (5, 55, 0), (5, 56, 0), (9, 55, 9), (9, 56, 9),
            ]);
        }
    }

    #[test]
    fn short_moves_keep_their_order_in_every_direction() {
        let diagonal = moved((0.5, 64.0, 0.5), (2.3, 63.1, 1.7));
        let negative = moved((-0.5, -2.0, -0.5), (-2.3, -3.1, -1.7));
        for release in FROM_1_21_9 {
            assert_eq!(visited_all(&[diagonal], release), vec![
                (0, 65, 0), (0, 64, 0), (1, 65, 0), (1, 64, 0), (0, 63, 0), (1, 63, 0),
                (1, 65, 1), (1, 64, 1), (1, 63, 1), (2, 65, 1), (2, 64, 1), (2, 63, 1),
            ]);
            assert_eq!(visited_all(&[negative], release), vec![
                (-1, -1, -1), (-1, -2, -1), (-2, -1, -1), (-2, -2, -1), (-1, -3, -1), (-2, -3, -1),
                (-2, -1, -2), (-2, -2, -2), (-2, -3, -2), (-3, -1, -2), (-3, -2, -2), (-3, -3, -2),
                (-3, -4, -2),
            ]);
        }
    }

    #[test]
    fn a_cell_reached_again_is_visited_once_unless_it_failed_from_1_21_9() {
        // One LongSet for the tick's moves (26.3 Entity.java:1320, 1347). It takes a block only once
        // its test passes from 1.21.9 (1.21.9 Entity.java:1244), at once before (1.21.8 :1188)
        let there = moved((0.5, 64.0, 0.5), (4.5, 64.0, 0.5));
        let back = moved((4.5, 64.0, 0.5), (0.5, 64.0, 0.5));
        let cells = (0..=4).flat_map(|x| [(x, 64, 0), (x, 65, 0)]).collect::<Vec<_>>();
        for release in FROM_1_21_9 {
            assert_eq!(visited_all(&[there, back], release), cells);
        }
        let failing_on_the_way_there = |release| {
            blocks_visited_where(
                &[there, back],
                player_box(back.to),
                0.3,
                1.8,
                release,
                |visit| visit.position != (2, 64, 0) || visit.from == back.from,
            )
            .into_iter()
            .map(|(position, _)| position)
            .filter(|&position| position == (2, 64, 0))
            .count()
        };
        assert_eq!(failing_on_the_way_there(MinecraftVersion::new(1, 21, 9)), 1);
        assert_eq!(failing_on_the_way_there(MinecraftVersion::new(1, 21, 8)), 0);
    }

    #[test]
    fn a_long_move_is_precise_wherever_it_went() {
        // movedFar makes every block of a move over a block long precise (26.3 Entity.java:1357,
        // 1392), the final box's by its own test
        let long = moved((0.5, 64.0, 0.5), (100_000.5, 64.0, 0.5));
        let final_box = player_box(long.to);
        let visits = blocks_visited(&[long], final_box, 0.3, 1.8, MinecraftVersion::new(26, 3, 0));
        assert_eq!(visits.len(), 34);
        assert!(visits.iter().all(|&(_, precise)| precise));
    }
}
