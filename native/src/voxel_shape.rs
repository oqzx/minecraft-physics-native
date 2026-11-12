use crate::collision::{collide_over_grid, grid_bits, grid_lines, Axis, GridTest};
use crate::math::{Aabb, Vec3};
use crate::version::MinecraftVersion;

/// Shapes.java's threshold: below it a box has no size and two lines are one.
const LINE_EPSILON: f64 = 1.0e-7;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelShape {
    boxes: Vec<Aabb>,
    /// The block each box was made in (Shapes.create), moved with it: its grid lines come from there.
    /// Shapes.or rebuilds its boxes with Shapes.box in absolute coordinates, so theirs is zero.
    grid_origins: Vec<Vec3>,
    /// Each box in the block it was made in, as Shapes.create had it: taking the block back off a moved
    /// box would round the lines the grid test reads.
    local_boxes: Vec<Aabb>,
    /// What a join left: its cells over its own lines. Those lines are not the lines of `boxes` once
    /// the IndirectMerger has dropped one that came within 1.0E-7 of the last it kept.
    joined: Option<Box<DiscreteShape>>,
}

/// A VoxelShape as Java holds one: the cells of a BitSetDiscreteVoxelShape over the coordinate lists.
#[derive(Clone, Debug, Default, PartialEq)]
struct DiscreteShape {
    size: [usize; 3],
    full: Vec<bool>,
    lines: [Vec<f64>; 3],
    /// A CubePointRange, on its axis: the lines are i/n and the mergers may take the DiscreteCubeMerger.
    /// A moved shape's OffsetDoubleList is none.
    cube: [bool; 3],
}

/// IndexMerger: the merged lines, and for each merged cell the cell of either side it came from.
struct Merger {
    lines: Vec<f64>,
    cube: bool,
    cells: Vec<(isize, isize, usize)>,
}

impl DiscreteShape {
    /// BitSetDiscreteVoxelShape#getIndex: x outermost, then y, then z.
    fn index(size: [usize; 3], x: usize, y: usize, z: usize) -> usize {
        (x * size[1] + y) * size[2] + z
    }

    /// DiscreteVoxelShape.isFullWide: outside the shape a cell is empty, as the mergers' -1 is.
    fn is_full(&self, x: isize, y: isize, z: isize) -> bool {
        let inside = |cell: isize, size: usize| cell >= 0 && (cell as usize) < size;
        inside(x, self.size[0])
            && inside(y, self.size[1])
            && inside(z, self.size[2])
            && self.full[Self::index(self.size, x as usize, y as usize, z as usize)]
    }

    /// Shapes.create: a box on a 1/2^n grid on every axis is a CubeVoxelShape of that grid, any
    /// other an ArrayVoxelShape of its bounds; a box without size is no shape (1.21.11
    /// Shapes.java:39-64).
    fn create(bounds: Aabb, test: GridTest) -> Option<Self> {
        let min = [bounds.min.x, bounds.min.y, bounds.min.z];
        let max = [bounds.max.x, bounds.max.y, bounds.max.z];
        if (0..3).any(|axis| max[axis] - min[axis] < LINE_EPSILON) {
            return None;
        }
        let bits = [0, 1, 2].map(|axis| grid_bits(min[axis], max[axis], test));
        if let [Some(x), Some(y), Some(z)] = bits {
            let size = [1_usize << x, 1_usize << y, 1_usize << z];
            let cell = |axis: usize, value: f64| (value * size[axis] as f64).round() as usize;
            let mut full = vec![false; size[0] * size[1] * size[2]];
            for x in cell(0, min[0])..cell(0, max[0]) {
                for y in cell(1, min[1])..cell(1, max[1]) {
                    for z in cell(2, min[2])..cell(2, max[2]) {
                        full[Self::index(size, x, y, z)] = true;
                    }
                }
            }
            let lines = [0, 1, 2].map(|axis| (0..=size[axis]).map(|line| line as f64 / size[axis] as f64).collect());
            return Some(Self { size, full, lines, cube: [true; 3] });
        }
        Some(Self {
            size: [1, 1, 1],
            full: vec![true],
            lines: [0, 1, 2].map(|axis| vec![min[axis], max[axis]]),
            cube: [false; 3],
        })
    }

    fn translated(mut self, offset: Vec3) -> Self {
        let by = [offset.x, offset.y, offset.z];
        for axis in 0..3 {
            if by[axis] != 0.0 {
                self.lines[axis].iter_mut().for_each(|line| *line += by[axis]);
            }
        }
        // (VoxelShape.move: an ArrayVoxelShape of OffsetDoubleLists, which is no CubePointRange)
        if offset != Vec3::ZERO {
            self.cube = [false; 3];
        }
        self
    }

    /// Shapes.joinUnoptimized with OR: the cells of both over the merged lines (1.21.11
    /// Shapes.java:111-134, BitSetDiscreteVoxelShape.join; 1.13.2 Shapes.java:124-149).
    fn or(&self, other: &Self) -> Self {
        let x = merger(1, &self.lines[0], self.cube[0], &other.lines[0], other.cube[0]);
        let y = merger(x.lines.len() - 1, &self.lines[1], self.cube[1], &other.lines[1], other.cube[1]);
        let z = merger(
            (x.lines.len() - 1) * (y.lines.len() - 1),
            &self.lines[2],
            self.cube[2],
            &other.lines[2],
            other.cube[2],
        );
        let size = [x.lines.len() - 1, y.lines.len() - 1, z.lines.len() - 1];
        let mut full = vec![false; size[0] * size[1] * size[2]];
        for &(x1, x2, xr) in &x.cells {
            for &(y1, y2, yr) in &y.cells {
                for &(z1, z2, zr) in &z.cells {
                    if self.is_full(x1, y1, z1) || other.is_full(x2, y2, z2) {
                        full[Self::index(size, xr, yr, zr)] = true;
                    }
                }
            }
        }
        let cube = x.cube && y.cube && z.cube;
        Self { size, full, lines: [x.lines, y.lines, z.lines], cube: [cube; 3] }
    }

    /// VoxelShape.forAllBoxes: the merged boxes over the lines.
    fn boxes(&self, release: MinecraftVersion) -> Vec<Aabb> {
        let mut cells = self.full.clone();
        merged_boxes(&mut cells, self.size, release)
            .into_iter()
            .map(|[x1, y1, z1, x2, y2, z2]| {
                Aabb::new(
                    Vec3::new(self.lines[0][x1], self.lines[1][y1], self.lines[2][z1]),
                    Vec3::new(self.lines[0][x2], self.lines[1][y2], self.lines[2][z2]),
                )
            })
            .collect()
    }

    /// VoxelShape.optimize: an OR of a Shapes.box of each box, or nothing where none has a size.
    fn optimized(&self, release: MinecraftVersion, test: GridTest) -> Option<Self> {
        self.boxes(release)
            .into_iter()
            .fold(None, |joined: Option<Self>, bounds| match (joined, Self::create(bounds, test)) {
                (Some(joined), Some(next)) => Some(joined.or(&next)),
                (joined, next) => joined.or(next),
            })
    }
}

/// Shapes.createIndexMerger (1.21.11 :236-253; 1.13.2 Shapes.java createListMerger).
fn merger(cost: usize, first: &[f64], first_cube: bool, second: &[f64], second_cube: bool) -> Merger {
    let (first_size, second_size) = (first.len() - 1, second.len() - 1);
    if first_cube && second_cube {
        let divisor = gcd(first_size, second_size);
        let size = first_size * (second_size / divisor);
        if (cost as u64) * (size as u64) <= 256 {
            let (first_div, second_div) = (first_size / divisor, second_size / divisor);
            return Merger {
                lines: (0..=size).map(|line| line as f64 / size as f64).collect(),
                cube: true,
                cells: (0..size).map(|i| ((i / second_div) as isize, (i / first_div) as isize, i)).collect(),
            };
        }
    }
    if first[first_size] < second[0] - LINE_EPSILON {
        return non_overlapping(first, second, false);
    }
    if second[second_size] < first[0] - LINE_EPSILON {
        return non_overlapping(second, first, true);
    }
    if first == second {
        return Merger {
            lines: first.to_vec(),
            cube: false,
            cells: (0..first_size).map(|i| (i as isize, i as isize, i)).collect(),
        };
    }
    indirect(first, second)
}

/// The greatest common divisor that Shapes#lcm and the DiscreteCubeMerger take of the two cell
/// counts (Guava's IntMath gcd there).
fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// NonOverlappingMerger: the lower list, then the upper's, and the gap between them is a cell of
/// neither (the lower's last index is past its last cell; NonOverlappingMerger#forMergedIndexes).
fn non_overlapping(lower: &[f64], upper: &[f64], swap: bool) -> Merger {
    let low = lower.len();
    let mut cells: Vec<(isize, isize, usize)> = (0..low).map(|i| (i as isize, -1, i)).collect();
    cells.extend((0..upper.len() - 1).map(|i| ((low - 1) as isize, i as isize, low + i)));
    if swap {
        cells.iter_mut().for_each(|cell| *cell = (cell.1, cell.0, cell.2));
    }
    Merger { lines: lower.iter().chain(upper).copied().collect(), cube: false, cells }
}

/// IndirectMerger for an OR, where neither side may be skipped: both lists in order, and a line
/// within 1.0E-7 of the last kept only moves that cell's indices on (1.21.11
/// IndirectMerger.java:14-64, 1.16.5 IndirectMerger.java; NaN starts it, so the first line is kept).
/// Its constructor builds the lines and indices that IndirectMerger#forMergedIndexes walks.
fn indirect(first: &[f64], second: &[f64]) -> Merger {
    let (mut i, mut j) = (0_usize, 0_usize);
    let mut last = f64::NAN;
    let mut lines: Vec<f64> = Vec::with_capacity(first.len() + second.len());
    let mut indices: Vec<(isize, isize)> = Vec::with_capacity(first.len() + second.len());
    loop {
        let (first_out, second_out) = (i >= first.len(), j >= second.len());
        if first_out && second_out {
            break;
        }
        let chose_first = !first_out && (second_out || first[i] < second[j] + LINE_EPSILON);
        if chose_first {
            i += 1;
        } else {
            j += 1;
        }
        let pair = (i as isize - 1, j as isize - 1);
        let next = if chose_first { first[i - 1] } else { second[j - 1] };
        if last >= next - LINE_EPSILON {
            *indices.last_mut().expect("a line was kept") = pair;
        } else {
            lines.push(next);
            indices.push(pair);
            last = next;
        }
    }
    if lines.len() < 2 {
        return Merger { lines: vec![0.0], cube: false, cells: Vec::new() };
    }
    let cells = indices[..lines.len() - 1].iter().enumerate().map(|(cell, pair)| (pair.0, pair.1, cell)).collect();
    Merger { lines, cube: false, cells }
}

impl VoxelShape {
    /// Shapes#empty: no boxes.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Shapes#create of a box in world coordinates: its grid is the cube at the world's origin.
    #[must_use]
    pub fn from_box(bounds: Aabb) -> Self {
        Self {
            boxes: vec![bounds],
            grid_origins: vec![Vec3::ZERO],
            local_boxes: vec![bounds],
            joined: None,
        }
    }

    /// Shapes.or over the shapes in turn: the varargs or reduces with Shapes::or, and one shape
    /// comes back as it is (1.21.11 Shapes.java:103-109; 1.13.2 Shapes.java:116-118).
    #[must_use]
    pub fn union(release: MinecraftVersion, shapes: &[Self]) -> Self {
        let Some((first, rest)) = shapes.split_first() else {
            return Self::empty();
        };
        rest.iter()
            .fold(first.clone(), |joined, shape| joined.or(release, shape))
    }

    /// Shapes.or: joinUnoptimized(a, b, OR).optimize(). The join holds the cells of either shape
    /// over the lines the mergers keep; optimize rebuilds it from a Shapes.box of each box
    /// forAllBoxes merges those cells into, so the grid comes from the merged boxes, in absolute
    /// coordinates (1.21.11 Shapes.java:111-134 and VoxelShape.java:75-79; 1.13.2
    /// Shapes.java:120-122 and VoxelShape.java:102-106).
    fn or(&self, release: MinecraftVersion, other: &Self) -> Self {
        let test = GridTest::of(Some(release));
        let joined = match (self.discrete(test), other.discrete(test)) {
            (Some(first), Some(second)) => first.or(&second),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => return Self::empty(),
        };
        joined.optimized(release, test).map_or_else(Self::empty, |shape| Self::from_discrete(shape, release))
    }

    /// Shapes.joinUnoptimized(a, b, OR): the shape before the optimization point. A move asked of
    /// it meets the join's lines, not those of the boxes optimize makes of it.
    #[cfg(test)]
    fn or_unoptimized(&self, release: MinecraftVersion, other: &Self) -> Self {
        let test = GridTest::of(Some(release));
        match (self.discrete(test), other.discrete(test)) {
            (Some(first), Some(second)) => Self::from_discrete(first.or(&second), release),
            (Some(only), None) | (None, Some(only)) => Self::from_discrete(only, release),
            (None, None) => Self::empty(),
        }
    }

    fn from_discrete(shape: DiscreteShape, release: MinecraftVersion) -> Self {
        let boxes = shape.boxes(release);
        Self { grid_origins: vec![Vec3::ZERO; boxes.len()], local_boxes: boxes.clone(), boxes, joined: Some(Box::new(shape)) }
    }

    /// The shape as Java holds it: what a join left, or the OR of its boxes, each a Shapes.create
    /// in the block it was made in and moved from there.
    fn discrete(&self, test: GridTest) -> Option<DiscreteShape> {
        if let Some(joined) = &self.joined {
            return Some((**joined).clone());
        }
        self.local_boxes
            .iter()
            .zip(&self.grid_origins)
            .filter_map(|(local, origin)| DiscreteShape::create(*local, test).map(|shape| shape.translated(*origin)))
            .reduce(|joined, next| joined.or(&next))
    }

    #[must_use]
    pub fn boxes(&self) -> &[Aabb] {
        &self.boxes
    }

    /// VoxelShape#isEmpty: the shape has no box.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// VoxelShape#bounds: the box around all the boxes; none for an empty shape, where vanilla
    /// throws.
    #[must_use]
    pub fn bounds(&self) -> Option<Aabb> {
        let first = *self.boxes.first()?;
        let mut min = first.min;
        let mut max = first.max;
        for bounds in self.boxes.iter().skip(1) {
            min.x = min.x.min(bounds.min.x);
            min.y = min.y.min(bounds.min.y);
            min.z = min.z.min(bounds.min.z);
            max.x = max.x.max(bounds.max.x);
            max.y = max.y.max(bounds.max.y);
            max.z = max.z.max(bounds.max.z);
        }
        Some(Aabb::new(min, max))
    }

    /// VoxelShape.collide: collideX over the merged cell grid of the boxes, so a box already
    /// inside the shape passes the layers it is in (VoxelShape.java:254-310 in 1.21.11, the same
    /// from 1.13.2).
    #[must_use]
    pub fn collide(&self, axis: Axis, entity: Aabb, delta: f64) -> f64 {
        self.collide_with_test(axis, entity, delta, GridTest::Round)
    }

    /// `collide` by the grid test of a release, whose lines differ before 1.17 (GridTest;
    /// VoxelShape#collide).
    #[must_use]
    pub fn collide_in(&self, release: MinecraftVersion, axis: Axis, entity: Aabb, delta: f64) -> f64 {
        self.collide_with_test(axis, entity, delta, GridTest::of(Some(release)))
    }

    /// VoxelShape#collideX over the shape's grid: an empty shape passes the movement on; the grid
    /// is a join's own lines, else those of each box's Shapes.create under `test`.
    fn collide_with_test(&self, axis: Axis, entity: Aabb, delta: f64, test: GridTest) -> f64 {
        if self.is_empty() {
            return delta;
        }
        if let Some(joined) = &self.joined {
            return collide_over_grid(&joined.lines, &self.boxes, axis, entity, delta);
        }
        let grids = [Axis::X, Axis::Y, Axis::Z].map(|axis| {
            grid_lines(
                self.boxes
                    .iter()
                    .copied()
                    .zip(self.grid_origins.iter().copied()),
                axis,
                test,
            )
        });
        collide_over_grid(&grids, &self.boxes, axis, entity, delta)
    }

    #[must_use]
    pub fn translated(&self, offset: Vec3) -> Self {
        Self {
            boxes: self
                .boxes
                .iter()
                .map(|bounds| bounds.translated(offset))
                .collect(),
            grid_origins: self
                .grid_origins
                .iter()
                .map(|origin| *origin + offset)
                .collect(),
            local_boxes: self.local_boxes.clone(),
            joined: self.joined.as_ref().map(|shape| Box::new((**shape).clone().translated(offset))),
        }
    }

    #[must_use]
    pub fn rotated_y_quarters(&self, quarters: u8) -> Self {
        self.rotated(quarters, rotate_y_quarter)
    }

    #[must_use]
    pub fn rotated_x_quarters(&self, quarters: u8) -> Self {
        self.rotated(quarters, rotate_x_quarter)
    }

    #[must_use]
    pub fn rotated_z_quarters(&self, quarters: u8) -> Self {
        self.rotated(quarters, rotate_z_quarter)
    }

    fn rotated(&self, quarters: u8, rotate: fn(Aabb) -> Aabb) -> Self {
        let turns = quarters % 4;
        let mut result = self.clone();
        for _ in 0..turns {
            result = Self {
                boxes: result.boxes.iter().map(|bounds| rotate(*bounds)).collect(),
                grid_origins: result
                    .grid_origins
                    .iter()
                    .map(|origin| {
                        rotate(Aabb::new(*origin, *origin + Vec3::new(1.0, 1.0, 1.0))).min
                    })
                    .collect(),
                local_boxes: result.local_boxes.iter().map(|local| rotate(*local)).collect(),
                joined: None,
            };
        }
        result
    }
}

/// BitSetDiscreteVoxelShape.forAllBoxes with combining: a run of full cells along Z from the
/// first full cell, widened along X while the next strip is full, then along Y while the next
/// row of strips is; each cell it takes is cleared. From 1.18 the cells are walked Y, X, Z
/// (1.21.11 BitSetDiscreteVoxelShape.java:147-188, 1.18 BitSetDiscreteVoxelShape.java:151-191); before,
/// X, Y, Z (1.17 BitSetDiscreteVoxelShape.java:151-191; 1.13.2 DiscreteVoxelShape.java:200-253, whose widening
/// towards lower X and Y never finds a cell, those were all taken already).
fn merged_boxes(
    cells: &mut [bool],
    size: [usize; 3],
    release: MinecraftVersion,
) -> Vec<[usize; 6]> {
    let index = |x: usize, y: usize, z: usize| (x * size[1] + y) * size[2] + z;
    let strip_full = |cells: &[bool], from: usize, to: usize, x: usize, y: usize| {
        x < size[0] && y < size[1] && (from..to).all(|z| cells[index(x, y, z)])
    };
    let starts: Vec<(usize, usize)> = if release.at_least(1, 18, 0) {
        (0..size[1])
            .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
            .collect()
    } else {
        (0..size[0])
            .flat_map(|x| (0..size[1]).map(move |y| (x, y)))
            .collect()
    };
    let mut boxes = Vec::new();
    for (x, y) in starts {
        let mut start = None;
        for z in 0..=size[2] {
            if z < size[2] && cells[index(x, y, z)] {
                start.get_or_insert(z);
            } else if let Some(from) = start.take() {
                let mut end_x = x;
                let mut end_y = y;
                let clear = |cells: &mut [bool], x: usize, y: usize| {
                    cells[index(x, y, from)..index(x, y, z)].fill(false);
                };
                clear(cells, x, y);
                while strip_full(cells, from, z, end_x + 1, y) {
                    clear(cells, end_x + 1, y);
                    end_x += 1;
                }
                while (x..=end_x).all(|column| strip_full(cells, from, z, column, end_y + 1)) {
                    for column in x..=end_x {
                        clear(cells, column, end_y + 1);
                    }
                    end_y += 1;
                }
                boxes.push([x, y, from, end_x + 1, end_y + 1, z]);
            }
        }
    }
    boxes
}

fn bounds_from_corners(corners: [Vec3; 8]) -> Aabb {
    let mut min = corners[0];
    let mut max = corners[0];
    for corner in corners.iter().skip(1) {
        min.x = min.x.min(corner.x);
        min.y = min.y.min(corner.y);
        min.z = min.z.min(corner.z);
        max.x = max.x.max(corner.x);
        max.y = max.y.max(corner.y);
        max.z = max.z.max(corner.z);
    }
    Aabb::new(min, max)
}

fn rotate_y_quarter(bounds: Aabb) -> Aabb {
    let corners = [
        Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
        Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
        Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
        Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
        Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
    ];
    bounds_from_corners(corners.map(|corner| Vec3::new(1.0 - corner.z, corner.y, corner.x)))
}

fn rotate_x_quarter(bounds: Aabb) -> Aabb {
    let corners = [
        Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
        Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
        Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
        Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
        Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
    ];
    bounds_from_corners(corners.map(|corner| Vec3::new(corner.x, 1.0 - corner.z, corner.y)))
}

fn rotate_z_quarter(bounds: Aabb) -> Aabb {
    let corners = [
        Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
        Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
        Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
        Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
        Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
    ];
    bounds_from_corners(corners.map(|corner| Vec3::new(corner.y, 1.0 - corner.x, corner.z)))
}

#[cfg(test)]
mod tests {
    use super::VoxelShape;
    use crate::collision::{grid_lines, Axis, GridTest};
    use crate::math::{Aabb, Vec3};
    use crate::version::MinecraftVersion;

    const RELEASE: MinecraftVersion = MinecraftVersion::new(1, 21, 11);

    fn bounds(min: Vec3, max: Vec3) -> Aabb {
        Aabb::new(min, max)
    }

    /// scripts/java-oracle/OracleUnion.java: the ordered boxes of Shapes.or and of joinUnoptimized over
    /// the same inputs, and moves asked of each, on six releases either side of the 1.17 and 1.18 gates.
    #[test]
    fn union_and_join_match_the_jars_box_by_box() {
        let text = include_str!("../fixtures/java-oracle-union.txt");
        let number = |word: &str| word.parse::<f64>().expect("a number");
        let parse_release = |text: &str| {
            let mut parts = text.split('.').map(|part| part.parse::<u16>().expect("a release"));
            let major = parts.next().expect("major") as _;
            let minor = parts.next().expect("minor") as _;
            MinecraftVersion::new(major, minor, parts.next().unwrap_or(0) as _)
        };
        let aabb = |w: &[f64]| Aabb::new(Vec3::new(w[0], w[1], w[2]), Vec3::new(w[3], w[4], w[5]));
        let mut last: Option<VoxelShape> = None;
        let (mut unions, mut moves) = (0, 0);
        for line in text.lines() {
            let (asked, answer) = line.split_once(" = ").expect("an answer");
            let words: Vec<&str> = asked.split(' ').collect();
            let release = parse_release(words[1]);
            if words[0] == "collide" {
                let axis = [Axis::X, Axis::Y, Axis::Z][words[3].parse::<usize>().expect("axis")];
                let values: Vec<f64> = words[4..].iter().map(|w| number(w)).collect();
                let shape = last.as_ref().expect("a shape before");
                let got = shape.collide_in(release, axis, aabb(&values[..6]), values[6]);
                assert_eq!(got.to_bits(), number(answer).to_bits(), "{line}: got {got}");
                moves += 1;
                continue;
            }
            let values: Vec<f64> = words[3..].iter().map(|w| number(w)).collect();
            let shapes: Vec<VoxelShape> = values
                .chunks(9)
                .map(|w| {
                    let offset = Vec3::new(w[0], w[1], w[2]);
                    let shape = VoxelShape::from_box(aabb(&w[3..]));
                    if offset == Vec3::ZERO { shape } else { shape.translated(offset) }
                })
                .collect();
            let shape = if words[0] == "join" {
                shapes[0].or_unoptimized(release, &shapes[1])
            } else {
                VoxelShape::union(release, &shapes)
            };
            let want: Vec<f64> = answer.split(' ').skip(1).map(number).collect();
            let got: Vec<f64> = shape
                .boxes()
                .iter()
                .flat_map(|b| [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z])
                .collect();
            assert_eq!(got, want, "{line}");
            unions += 1;
            last = Some(shape);
        }
        assert!(unions > 500 && moves > 1000, "{unions} unions, {moves} moves");
    }

    fn grid(shape: &VoxelShape, axis: Axis) -> Vec<f64> {
        grid_lines(
            shape
                .boxes
                .iter()
                .copied()
                .zip(shape.grid_origins.iter().copied()),
            axis,
            GridTest::Round,
        )
    }

    #[test]
    fn union_merges_the_joined_cells_into_boxes() {
        // Shapes.or optimizes the join: forAllBoxes merges the two halves into one full cube, and
        // a shape or'd with itself is that shape (1.21.11 Shapes.java:111-112, VoxelShape.java:75-79).
        let box_a = bounds(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0));
        let box_b = bounds(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        let shape = VoxelShape::union(
            RELEASE,
            &[
                VoxelShape::from_box(box_a),
                VoxelShape::from_box(box_b),
                VoxelShape::from_box(box_a),
            ],
        );
        assert_eq!(
            shape.boxes(),
            &[bounds(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))]
        );
        assert!(!shape.is_empty());
        assert_eq!(
            shape.bounds(),
            Some(bounds(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
        );
        // The varargs or of one shape reduces to that shape, unoptimized (Shapes.java:127-129).
        let single = VoxelShape::from_box(box_a);
        assert_eq!(VoxelShape::union(RELEASE, std::slice::from_ref(&single)), single);
        assert!(VoxelShape::empty().is_empty());
        assert_eq!(VoxelShape::empty().bounds(), None);
        assert!(VoxelShape::union(RELEASE, &[]).is_empty());
    }

    #[test]
    fn union_merges_cells_in_the_order_of_its_release() {
        // Cells (x, y) full at (1, 0), (0, 1), (1, 1). From 1.18 forAllBoxes walks Y then X: the
        // run at (1, 0) grows up through (1, 1) first (1.21.11 BitSetDiscreteVoxelShape.java:150-181).
        // Before, X then Y: the run at (0, 1) grows along X first (1.17 BitSetDiscreteVoxelShape.java:154-186).
        let shapes = [
            VoxelShape::from_box(bounds(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))),
            VoxelShape::from_box(bounds(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0))),
        ];
        assert_eq!(
            VoxelShape::union(RELEASE, &shapes).boxes(),
            &[
                bounds(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                bounds(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.5, 1.0, 1.0)),
            ]
        );
        assert_eq!(
            VoxelShape::union(MinecraftVersion::new(1, 17, 1), &shapes).boxes(),
            &[
                bounds(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                bounds(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 0.5, 1.0)),
            ]
        );
    }

    #[test]
    fn quarter_turn_rotates_box_about_block_center() {
        let shape =
            VoxelShape::from_box(bounds(Vec3::new(0.0, 0.0, 0.25), Vec3::new(0.5, 1.0, 0.75)));
        let rotated = shape.rotated_y_quarters(1);
        assert_eq!(
            rotated.boxes(),
            &[bounds(Vec3::new(0.25, 0.0, 0.0), Vec3::new(0.75, 1.0, 0.5))]
        );
    }

    #[test]
    fn x_and_z_quarter_turns_rotate_about_block_center() {
        let shape =
            VoxelShape::from_box(bounds(Vec3::new(0.0, 0.0, 0.25), Vec3::new(0.5, 0.5, 0.75)));
        assert_eq!(
            shape.rotated_x_quarters(1).boxes(),
            &[bounds(Vec3::new(0.0, 0.25, 0.0), Vec3::new(0.5, 0.75, 0.5))]
        );
        assert_eq!(
            shape.rotated_z_quarters(1).boxes(),
            &[bounds(Vec3::new(0.0, 0.5, 0.25), Vec3::new(0.5, 1.0, 0.75))]
        );
    }

    #[test]
    fn collide_applies_each_indexed_box_and_preserves_empty_shape_motion() {
        let entity = bounds(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let shape =
            VoxelShape::from_box(bounds(Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 1.0)));
        assert_eq!(shape.collide(Axis::X, entity, 2.5), 1.0);
        assert_eq!(VoxelShape::empty().collide(Axis::X, entity, 2.5), 2.5);
        // collideX: |delta| < 1.0E-7 on a non-empty shape gives 0 (1.21.11 VoxelShape.java:211-212)
        assert_eq!(shape.collide(Axis::X, entity, 5.0e-8), 0.0);
    }

    #[test]
    fn collide_walks_the_grid_of_the_merged_boxes_for_a_box_inside_the_shape() {
        // A bottom slab joined with a 1/8 layer under it: Shapes.or optimizes the join, whose full
        // cells below 0.5 forAllBoxes merges into the slab box alone, and Shapes.box of it gives
        // the Y lines 0, 0.5, 1. A box inside the slab has no layer below it, and the layer above
        // is empty, so it is not stopped either way (1.21.11 VoxelShape.java:207-256 and
        // VoxelShape.java:75-79).
        let shape = VoxelShape::union(
            RELEASE,
            &[
                VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))),
                VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 0.125, 1.0))),
            ],
        );
        assert_eq!(
            shape.boxes(),
            &[bounds(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))]
        );
        assert_eq!(grid(&shape, Axis::Y), vec![0.0, 0.5, 1.0]);
        let inside = bounds(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.75, 0.375, 0.75));
        assert_eq!(shape.collide(Axis::Y, inside, -1.0), -1.0);
        assert_eq!(shape.collide(Axis::Y, inside, 1.0), 1.0);
        // A box above the shape lands on its top.
        let above = bounds(Vec3::new(0.25, 1.5, 0.25), Vec3::new(0.75, 2.0, 0.75));
        assert_eq!(shape.collide(Axis::Y, above, -2.0), -1.0);
        // One full cube has the grid 0, 1: a box inside it is not stopped.
        let cube = VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
        assert_eq!(cube.collide(Axis::Y, inside, -1.0), -1.0);
        // Two halves or'd are that full cube, grid 0, 1: a box inside the upper half moving down
        // is not stopped at 0.5.
        let halves = VoxelShape::union(
            RELEASE,
            &[
                VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))),
                VoxelShape::from_box(bounds(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0))),
            ],
        );
        assert_eq!(grid(&halves, Axis::Y), vec![0.0, 1.0]);
        let upper = bounds(Vec3::new(0.25, 0.6, 0.25), Vec3::new(0.75, 0.9, 0.75));
        assert_eq!(halves.collide(Axis::Y, upper, -1.0), -1.0);
    }

    #[test]
    fn a_moved_shape_keeps_the_grid_of_its_block() {
        // Shapes.move offsets the grid; the moved half slab still stops a box at its face.
        let slab = VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)))
            .translated(Vec3::new(3.0, 0.0, 0.0));
        let above = bounds(Vec3::new(3.25, 1.0, 0.25), Vec3::new(3.75, 2.0, 0.75));
        assert_eq!(slab.collide(Axis::Y, above, -1.0), -0.5);
        assert_eq!(
            slab.translated(Vec3::new(-3.0, 0.0, 0.0)),
            VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)))
        );
    }

    #[test]
    fn an_or_of_a_moved_shape_takes_the_grid_of_its_absolute_boxes() {
        // Shapes.or rebuilds the moved slab with Shapes.box(3, 0, 0, 4, 0.5, 1): outside the block
        // on X, findBits gives -1 and the ArrayVoxelShape holds each axis's bounds alone, not
        // the moved block's 1/2^n grid (1.21.11 Shapes.java:52-58, VoxelShape.java:75-79).
        let slab = VoxelShape::from_box(bounds(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)))
            .translated(Vec3::new(3.0, 0.0, 0.0));
        assert_eq!(grid(&slab, Axis::Y), vec![0.0, 0.5, 1.0]);
        let joined = VoxelShape::union(RELEASE, &[slab, VoxelShape::empty()]);
        assert_eq!(
            joined.boxes(),
            &[bounds(Vec3::new(3.0, 0.0, 0.0), Vec3::new(4.0, 0.5, 1.0))]
        );
        assert_eq!(grid(&joined, Axis::X), vec![3.0, 4.0]);
        assert_eq!(grid(&joined, Axis::Y), vec![0.0, 0.5]);
    }
}
