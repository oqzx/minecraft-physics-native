//! The VoxelShape joins of 1.13.2's collision query (Shapes.java, the mergers class_4085,
//! class_4089 and class_4092), which decide on cells of the merged grid lines: a line within
//! 1.0E-7 of the last one kept joins it, so what touches, or reaches in by less than that, is not
//! met. Only LevelReader.method_16365's few joins run here, once for each block of a 1.13.2 move.

use crate::collision::{grid_bits, GridTest};
use crate::math::{Aabb, Vec3};

const EPSILON: f64 = 1.0e-7;

/// The grid lines of a shape on one axis; `cube` for class_4083, whose i / n the cube merger reads.
#[derive(Clone, Debug, PartialEq)]
struct Lines {
    values: Vec<f64>,
    cube: bool,
}

impl Lines {
    /// The lines i / parts of a CubePointRange (CubePointRange#getDouble).
    fn cube(parts: u32) -> Self {
        Self {
            values: (0..=parts).map(|line| f64::from(line) / f64::from(parts)).collect(),
            cube: true,
        }
    }

    fn cells(&self) -> usize {
        self.values.len() - 1
    }
}

/// A VoxelShape: its grid lines and its cells, x outermost as VoxelSet's index.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    lines: [Lines; 3],
    cells: Vec<bool>,
}

fn component(value: Vec3, axis: usize) -> f64 {
    [value.x, value.y, value.z][axis]
}

impl Shape {
    /// Shapes#empty: one empty cell of the unit cube.
    fn empty() -> Self {
        Self::of_lines(std::array::from_fn(|_| Lines::cube(1)), vec![false])
    }

    fn of_lines(lines: [Lines; 3], cells: Vec<bool>) -> Self {
        Self { lines, cells }
    }

    /// The cells of grid lines where `filled` says, by cell index.
    fn filled(lines: [Lines; 3], filled: impl Fn([usize; 3]) -> bool) -> Self {
        let size = lines.each_ref().map(Lines::cells);
        let mut cells = Vec::with_capacity(size[0] * size[1] * size[2]);
        for x in 0..size[0] {
            for y in 0..size[1] {
                for z in 0..size[2] {
                    cells.push(filled([x, y, z]));
                }
            }
        }
        Self { lines, cells }
    }

    /// Shapes.create, its grid told by the test of 1.13.2.
    pub(crate) fn create(bounds: Aabb) -> Self {
        let bits: [Option<u32>; 3] = [0, 1, 2].map(|axis| {
            grid_bits(component(bounds.min, axis), component(bounds.max, axis), GridTest::Floor)
        });
        let [Some(x), Some(y), Some(z)] = bits else {
            let lines = [0, 1, 2].map(|axis| Lines {
                values: vec![component(bounds.min, axis), component(bounds.max, axis)],
                cube: false,
            });
            return Self::of_lines(lines, vec![true]);
        };
        if [x, y, z] == [0, 0, 0] {
            let center = bounds.min.x <= 0.5
                && 0.5 < bounds.max.x
                && bounds.min.y <= 0.5
                && 0.5 < bounds.max.y
                && bounds.min.z <= 0.5
                && 0.5 < bounds.max.z;
            return if center { Self::unit() } else { Self::empty() };
        }
        let parts = [x, y, z].map(|bits| 1_u32 << bits);
        let range = |axis: usize| {
            let scale = f64::from(parts[axis]);
            // (int)Math.round: these are small and never halves off the grid's own tolerance
            let low = crate::math::java_round(component(bounds.min, axis) * scale);
            let high = crate::math::java_round(component(bounds.max, axis) * scale);
            (low, high)
        };
        let ranges = [range(0), range(1), range(2)];
        Self::filled([0, 1, 2].map(|axis| Lines::cube(parts[axis])), |cell| {
            (0..3).all(|axis| {
                let at = cell[axis] as i64;
                ranges[axis].0 <= at && at < ranges[axis].1
            })
        })
    }

    /// Shapes.block().
    fn unit() -> Self {
        Self::of_lines(std::array::from_fn(|_| Lines::cube(1)), vec![true])
    }

    /// A shape of a block: the lines of its boxes, its cells where a box holds their center.
    pub(crate) fn of_boxes(lines: [Vec<f64>; 3], boxes: &[Aabb]) -> Self {
        let lines = lines.map(|values| Lines { values, cube: false });
        Self::over(lines, boxes)
    }

    fn over(lines: [Lines; 3], boxes: &[Aabb]) -> Self {
        let centers = lines.clone().map(|lines| {
            lines.values.windows(2).map(|cell| (cell[0] + cell[1]) / 2.0).collect::<Vec<f64>>()
        });
        Self::filled(lines, |cell| {
            let point = Vec3::new(centers[0][cell[0]], centers[1][cell[1]], centers[2][cell[2]]);
            boxes.iter().any(|bounds| bounds.contains(point))
        })
    }

    /// The whole cubes of the range: the shape LevelReader#method_16378 joins them in (class_4101).
    pub(crate) fn of_cubes(low: [i32; 3], high: [i32; 3], cubes: impl Fn([i32; 3]) -> bool) -> Self {
        let lines = [0, 1, 2].map(|axis| Lines {
            values: (low[axis]..=high[axis]).map(f64::from).collect(),
            cube: false,
        });
        Self::filled(lines, |cell| {
            cubes([0, 1, 2].map(|axis| low[axis] + cell[axis] as i32))
        })
    }

    /// VoxelShape#move (MCP's offset): OffsetDoubleList, no longer a cube's lines.
    pub(crate) fn moved(&self, by: Vec3) -> Self {
        let lines = [0, 1, 2].map(|axis| Lines {
            values: self.lines[axis].values.iter().map(|line| line + component(by, axis)).collect(),
            cube: false,
        });
        Self::of_lines(lines, self.cells.clone())
    }

    /// VoxelShape#isEmpty: no cell is full.
    pub(crate) fn is_empty(&self) -> bool {
        !self.cells.iter().any(|cell| *cell)
    }

    /// BitSetDiscreteVoxelShape#getIndex of a cell inside the shape, x outermost; none outside it.
    fn index(&self, [x, y, z]: [i32; 3]) -> Option<usize> {
        let size = self.lines.each_ref().map(Lines::cells);
        let inside = |at: i32, size: usize| at >= 0 && (at as usize) < size;
        (inside(x, size[0]) && inside(y, size[1]) && inside(z, size[2]))
            .then(|| (x as usize * size[1] + y as usize) * size[2] + z as usize)
    }

    /// DiscreteVoxelShape.isFullWide.
    fn holds(&self, cell: [i32; 3]) -> bool {
        self.index(cell).is_some_and(|index| self.cells[index])
    }

    /// The first and past-the-last filled cell on each axis, the bounds class_4081 keeps
    /// (BitSetDiscreteVoxelShape#firstFull and BitSetDiscreteVoxelShape#lastFull).
    fn filled_range(&self) -> [(usize, usize); 3] {
        let size = self.lines.each_ref().map(Lines::cells);
        let mut range = size.map(|size| (size, 0));
        for x in 0..size[0] {
            for y in 0..size[1] {
                for z in 0..size[2] {
                    if self.cells[(x * size[1] + y) * size[2] + z] {
                        for (axis, at) in [x, y, z].into_iter().enumerate() {
                            range[axis].0 = range[axis].0.min(at);
                            range[axis].1 = range[axis].1.max(at + 1);
                        }
                    }
                }
            }
        }
        range
    }

    /// VoxelShape#min and VoxelShape#max (MCP's getMinimum and getMaximum) of each axis; the shape is not empty.
    pub(crate) fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let range = self.filled_range();
        (
            [0, 1, 2].map(|axis| self.lines[axis].values[range[axis].0]),
            [0, 1, 2].map(|axis| self.lines[axis].values[range[axis].1]),
        )
    }
}

/// A merger's lines, and for each cell the cell of either shape it stands over (-1 for none).
struct Merged {
    lines: Lines,
    cells: Vec<(i32, i32)>,
}

/// The greatest common divisor of the two cell counts, whose least common multiple Shapes#lcm
/// takes for the cube merger (Guava's IntMath gcd there).
fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Shapes.createIndexMerger. A shape past the other (class_4094) never comes
/// here: both joins' lists overlap, and AND leaves first on the shapes' bounds.
fn merge(first: &Lines, second: &Lines, prior: usize, keep_first: bool, keep_second: bool) -> Merged {
    if first.cube && second.cube {
        let (i, j) = (first.cells(), second.cells());
        let parts = i * j / gcd(i, j);
        if prior * parts <= 256 {
            let (step_i, step_j) = (i / gcd(i, j), j / gcd(i, j));
            return Merged {
                lines: Lines::cube(parts as u32),
                cells: (0..parts).map(|k| ((k / step_j) as i32, (k / step_i) as i32)).collect(),
            };
        }
    }
    if first.values == second.values {
        return Merged {
            lines: Lines { values: first.values.clone(), cube: false },
            cells: (0..first.cells() as i32).map(|cell| (cell, cell)).collect(),
        };
    }
    indirect(&first.values, &second.values, keep_first, keep_second)
}

/// class_4092 (IndirectMerger): the two sorted lists walked together, a line that is not past
/// the last kept by 1.0E-7 moving that cell's indices on instead. Its constructor builds the lines
/// and indices that IndirectMerger#forMergedIndexes walks.
fn indirect(first: &[f64], second: &[f64], keep_first: bool, keep_second: bool) -> Merged {
    let (mut i, mut j) = (0_usize, 0_usize);
    let mut last = f64::NAN;
    let mut values: Vec<f64> = Vec::with_capacity(first.len() + second.len());
    let mut cells: Vec<(i32, i32)> = Vec::with_capacity(first.len() + second.len());
    loop {
        let more_first = i < first.len();
        let more_second = j < second.len();
        if !more_first && !more_second {
            break;
        }
        let from_first = more_first && (!more_second || first[i] < second[j] + EPSILON);
        let line = if from_first {
            i += 1;
            first[i - 1]
        } else {
            j += 1;
            second[j - 1]
        };
        if ((i != 0 && more_first) || from_first || keep_second)
            && ((j != 0 && more_second) || !from_first || keep_first)
        {
            let at = (i as i32 - 1, j as i32 - 1);
            if !(last > line - EPSILON) {
                cells.push(at);
                values.push(line);
                last = line;
            } else if let Some(cell) = cells.last_mut() {
                *cell = at;
            }
        }
    }
    if values.is_empty() {
        values.push(first[first.len() - 1].min(second[second.len() - 1]));
    }
    cells.truncate(values.len() - 1);
    Merged { lines: Lines { values, cube: false }, cells }
}

/// The three mergers of a join, each told the cells of the ones before it (Shapes#joinUnoptimized
/// and Shapes#joinIsNotEmpty).
fn mergers(first: &Shape, second: &Shape, keep_first: bool, keep_second: bool) -> [Merged; 3] {
    let x = merge(&first.lines[0], &second.lines[0], 1, keep_first, keep_second);
    let y = merge(&first.lines[1], &second.lines[1], x.lines.cells(), keep_first, keep_second);
    let z = merge(
        &first.lines[2],
        &second.lines[2],
        x.lines.cells() * y.lines.cells(),
        keep_first,
        keep_second,
    );
    [x, y, z]
}

/// Shapes.join(first, second, ONLY_FIRST).
pub(crate) fn only_first(first: &Shape, second: &Shape) -> Shape {
    if first.is_empty() {
        return Shape::empty();
    }
    if second.is_empty() {
        return first.clone();
    }
    let [x, y, z] = mergers(first, second, true, false);
    let cells = |merged: &Merged| merged.cells.clone();
    let (of_x, of_y, of_z) = (cells(&x), cells(&y), cells(&z));
    let lines = [x.lines, y.lines, z.lines];
    Shape::filled(lines, |[cell_x, cell_y, cell_z]| {
        let (a_x, b_x) = of_x[cell_x];
        let (a_y, b_y) = of_y[cell_y];
        let (a_z, b_z) = of_z[cell_z];
        first.holds([a_x, a_y, a_z]) && !second.holds([b_x, b_y, b_z])
    })
}

/// Shapes.joinIsNotEmpty(first, second, AND).
pub(crate) fn overlap(first: &Shape, second: &Shape) -> bool {
    if first.is_empty() || second.is_empty() {
        return false;
    }
    let (first_low, first_high) = first.bounds();
    let (second_low, second_high) = second.bounds();
    for axis in [0, 1, 2] {
        if first_high[axis] < second_low[axis] - EPSILON
            || second_high[axis] < first_low[axis] - EPSILON
        {
            return false;
        }
    }
    let [x, y, z] = mergers(first, second, false, false);
    x.cells.iter().any(|(a_x, b_x)| {
        y.cells.iter().any(|(a_y, b_y)| {
            z.cells.iter().any(|(a_z, b_z)| {
                first.holds([*a_x, *a_y, *a_z]) && second.holds([*b_x, *b_y, *b_z])
            })
        })
    })
}
