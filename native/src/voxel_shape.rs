use crate::collision::{collide_over_grid, grid_lines, Axis};
use crate::math::{Aabb, Vec3};
use crate::version::MinecraftVersion;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelShape {
    boxes: Vec<Aabb>,
    /// The block each box was made in (Shapes.create), moved with it: its grid lines come from there.
    /// Shapes.or rebuilds its boxes with Shapes.box in absolute coordinates, so theirs is zero.
    grid_origins: Vec<Vec3>,
}

impl VoxelShape {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_box(bounds: Aabb) -> Self {
        Self {
            boxes: vec![bounds],
            grid_origins: vec![Vec3::ZERO],
        }
    }

    /// Shapes.or over the shapes in turn: the varargs or reduces with Shapes::or, and one shape
    /// comes back as it is (1.21.11 Shapes.java:123-129; 1.13.2 VoxelShapes.java:125-127).
    #[must_use]
    pub fn union(release: MinecraftVersion, shapes: &[Self]) -> Self {
        let Some((first, rest)) = shapes.split_first() else {
            return Self::empty();
        };
        rest.iter()
            .fold(first.clone(), |joined, shape| joined.or(release, shape))
    }

    /// Shapes.or: joinUnoptimized(a, b, OR).optimize(). The join holds the cells of either shape
    /// over the merged grid lines of both; optimize rebuilds it from a Shapes.box of each box
    /// forAllBoxes merges those cells into, so the grid comes from the merged boxes, in absolute
    /// coordinates (1.21.11 Shapes.java:131-160 and VoxelShape.java:96-100; 1.13.2
    /// VoxelShapes.java:129-131 and VoxelShape.java:107-111).
    fn or(&self, release: MinecraftVersion, other: &Self) -> Self {
        let parts: Vec<(Aabb, Vec3)> = self
            .boxes
            .iter()
            .copied()
            .zip(self.grid_origins.iter().copied())
            .chain(
                other
                    .boxes
                    .iter()
                    .copied()
                    .zip(other.grid_origins.iter().copied()),
            )
            .collect();
        let grids = [Axis::X, Axis::Y, Axis::Z].map(|axis| grid_lines(parts.iter().copied(), axis));
        let size = grids.each_ref().map(|grid| grid.len().saturating_sub(1));
        let center = |grid: &[f64], cell: usize| (grid[cell] + grid[cell + 1]) / 2.0;
        let mut cells = vec![false; size[0] * size[1] * size[2]];
        for x in 0..size[0] {
            for y in 0..size[1] {
                for z in 0..size[2] {
                    let point = Vec3::new(
                        center(&grids[0], x),
                        center(&grids[1], y),
                        center(&grids[2], z),
                    );
                    cells[(x * size[1] + y) * size[2] + z] =
                        parts.iter().any(|(bounds, _)| bounds.contains(point));
                }
            }
        }
        let boxes: Vec<Aabb> = merged_boxes(&mut cells, size, release)
            .into_iter()
            .map(|[x1, y1, z1, x2, y2, z2]| {
                Aabb::new(
                    Vec3::new(grids[0][x1], grids[1][y1], grids[2][z1]),
                    Vec3::new(grids[0][x2], grids[1][y2], grids[2][z2]),
                )
            })
            .collect();
        Self {
            grid_origins: vec![Vec3::ZERO; boxes.len()],
            boxes,
        }
    }

    #[must_use]
    pub fn boxes(&self) -> &[Aabb] {
        &self.boxes
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

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
        if self.is_empty() {
            return delta;
        }
        let grids = [Axis::X, Axis::Y, Axis::Z].map(|axis| {
            grid_lines(
                self.boxes
                    .iter()
                    .copied()
                    .zip(self.grid_origins.iter().copied()),
                axis,
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
            };
        }
        result
    }
}

/// BitSetDiscreteVoxelShape.forAllBoxes with combining: a run of full cells along Z from the
/// first full cell, widened along X while the next strip is full, then along Y while the next
/// row of strips is; each cell it takes is cleared. From 1.18 the cells are walked Y, X, Z
/// (1.21.11 BitSetDiscreteVoxelShape.java:151-191, 1.18 BitSetVoxelSet.java:152-192); before,
/// X, Y, Z (1.17 BitSetVoxelSet.java:152-192; 1.13.2 VoxelSet.java:203-256, whose widening
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
    use crate::collision::{grid_lines, Axis};
    use crate::math::{Aabb, Vec3};
    use crate::version::MinecraftVersion;

    const RELEASE: MinecraftVersion = MinecraftVersion::new(1, 21, 11);

    fn bounds(min: Vec3, max: Vec3) -> Aabb {
        Aabb::new(min, max)
    }

    fn grid(shape: &VoxelShape, axis: Axis) -> Vec<f64> {
        grid_lines(
            shape
                .boxes
                .iter()
                .copied()
                .zip(shape.grid_origins.iter().copied()),
            axis,
        )
    }

    #[test]
    fn union_merges_the_joined_cells_into_boxes() {
        // Shapes.or optimizes the join: forAllBoxes merges the two halves into one full cube, and
        // a shape or'd with itself is that shape (1.21.11 Shapes.java:131-132, VoxelShape.java:96-100).
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
        // run at (1, 0) grows up through (1, 1) first (1.21.11 BitSetDiscreteVoxelShape.java:154-185).
        // Before, X then Y: the run at (0, 1) grows along X first (1.17 BitSetVoxelSet.java:155-187).
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
        // collideX: |delta| < 1.0E-7 on a non-empty shape gives 0 (1.21.11 VoxelShape.java:263-265)
        assert_eq!(shape.collide(Axis::X, entity, 5.0e-8), 0.0);
    }

    #[test]
    fn collide_walks_the_grid_of_the_merged_boxes_for_a_box_inside_the_shape() {
        // A bottom slab joined with a 1/8 layer under it: Shapes.or optimizes the join, whose full
        // cells below 0.5 forAllBoxes merges into the slab box alone, and Shapes.box of it gives
        // the Y lines 0, 0.5, 1. A box inside the slab has no layer below it, and the layer above
        // is empty, so it is not stopped either way (1.21.11 VoxelShape.java:258-310 and
        // VoxelShape.java:96-100).
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
        // the moved block's 1/2^n grid (1.21.11 Shapes.java:60-70, VoxelShape.java:96-100).
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
