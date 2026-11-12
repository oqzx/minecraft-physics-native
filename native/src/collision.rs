use crate::fast_hash::FastMap;

use crate::legacy_shapes::{only_first, overlap, Shape};
use crate::math::{floor_i32, Aabb, Vec3};
use crate::version::MinecraftVersion;
use crate::world::shape_meets_query;

pub const VANILLA_EQUAL_EPSILON: f64 = 9.999999747378752e-6;
const COLLISION_EPSILON: f64 = 1.0e-7;

/// Mth#equal: the two differ by less than 1.0E-5F.
#[must_use]
pub fn vanilla_equal(left: f64, right: f64) -> bool {
    (left - right).abs() < VANILLA_EQUAL_EPSILON
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// The two axes across a move along this one, whose overlap VoxelShape#collideX tests (its
    /// AxisCycle turns of X, Y and Z).
    const fn others(self) -> [Self; 2] {
        match self {
            Self::X => [Self::Y, Self::Z],
            Self::Y => [Self::X, Self::Z],
            Self::Z => [Self::X, Self::Y],
        }
    }
}

/// VoxelShape.collide of one box, with its 1.0E-7 tolerances.
#[must_use]
pub fn clip_axis(entity: Aabb, obstacle: Aabb, delta: f64, axis: Axis) -> f64 {
    if delta.abs() < COLLISION_EPSILON {
        return 0.0;
    }
    clip_box(entity, obstacle, delta, axis)
}

/// One box's part of VoxelShape.collideX, without the shape's leading 1.0E-7 check: the boxes of
/// one shape clip the movement in turn with no check between them.
fn clip_box(entity: Aabb, obstacle: Aabb, delta: f64, axis: Axis) -> f64 {
    let overlaps = |other: Axis| {
        component(entity.min, other) + COLLISION_EPSILON < component(obstacle.max, other)
            && component(entity.max, other) - COLLISION_EPSILON >= component(obstacle.min, other)
    };
    let [first, second] = axis.others();
    if !overlaps(first) || !overlaps(second) {
        return delta;
    }
    if delta > 0.0 {
        let gap = component(obstacle.min, axis) - component(entity.max, axis);
        if component(entity.max, axis) - COLLISION_EPSILON < component(obstacle.min, axis)
            && gap >= -COLLISION_EPSILON
        {
            return delta.min(gap);
        }
    } else {
        let gap = component(obstacle.max, axis) - component(entity.min, axis);
        if component(entity.min, axis) + COLLISION_EPSILON >= component(obstacle.max, axis)
            && gap <= COLLISION_EPSILON
        {
            return delta.max(gap);
        }
    }
    delta
}

/// Whether a box reaches more than 1.0E-7 into another on every axis: the box's cell range that
/// VoxelShape#collideX takes, its faces 1.0E-7 in, meets the other's.
#[must_use]
pub fn reaches_into(entity: Aabb, bounds: Aabb) -> bool {
    [Axis::X, Axis::Y, Axis::Z].into_iter().all(|axis| {
        component(entity.max, axis) - COLLISION_EPSILON > component(bounds.min, axis)
            && component(entity.min, axis) + COLLISION_EPSILON < component(bounds.max, axis)
    })
}

/// The collision shape of a block as a VoxelShape among a move's colliders: the block at
/// `origin`, its boxes in the block as BlockState.getCollisionShape gives them (`local_boxes`)
/// and moved there (`boxes`, VoxelShape.move). A box inside it collides with it over its grid,
/// not box by box.
#[derive(Clone, Debug)]
pub struct BlockVoxelShape {
    pub origin: Vec3,
    pub boxes: Vec<Aabb>,
    pub local_boxes: Vec<Aabb>,
    /// The shape is the Shapes.block() object: a whole-cell box alone (WorldSnapshot::is_shapes_block).
    pub shapes_block: bool,
    /// The block is Blocks.MOVING_PISTON, whose shape is its block entity's: Shapes.collide's walk
    /// reads it in a cell on two edges of the walk (1.14.4 Shapes.java:239, 1.17.1 :268).
    pub moving_piston: bool,
    /// The grids of `boxes` on the three axes under a test, made at the first collide that walks
    /// them: a move collides with the same shape on each axis and each step height.
    grids: std::cell::OnceCell<(GridTest, [Vec<f64>; 3])>,
}

/// The shape alone: the grids it caches are a function of it.
impl PartialEq for BlockVoxelShape {
    fn eq(&self, other: &Self) -> bool {
        self.origin == other.origin
            && self.boxes == other.boxes
            && self.local_boxes == other.local_boxes
            && self.shapes_block == other.shapes_block
            && self.moving_piston == other.moving_piston
    }
}

fn is_whole_cell(local_boxes: &[Aabb]) -> bool {
    matches!(local_boxes, [only] if *only == Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
}

impl BlockVoxelShape {
    /// The shape of the block at `origin` from its boxes in the block, moved there as
    /// VoxelShape#move moves them.
    #[must_use]
    pub fn new(origin: Vec3, local_boxes: Vec<Aabb>) -> Self {
        let boxes = local_boxes.iter().map(|bounds| bounds.translated(origin)).collect();
        Self {
            origin,
            boxes,
            shapes_block: is_whole_cell(&local_boxes),
            local_boxes,
            moving_piston: false,
            grids: std::cell::OnceCell::new(),
        }
    }

    /// The shape as one of a moving piston block's, or not.
    #[must_use]
    pub fn moving_piston(self, moving_piston: bool) -> Self {
        Self { moving_piston, ..self }
    }

    /// Whether BlockCollisions gives the shape to a query (BlockCollisions#computeNext):
    /// `shape_meets_query`, or the boxes' strict overlap without a release.
    fn meets(&self, query: Aabb, release: Option<MinecraftVersion>) -> bool {
        self.boxes.iter().any(|bounds| match release {
            Some(release) => shape_meets_query(*bounds, self.shapes_block, query, release),
            None => bounds.intersects(query),
        })
    }

    /// The shape of the block at `origin` from its boxes in world coordinates (the boxes in the
    /// block taken back from them, exact for boxes on a dyadic grid).
    #[must_use]
    pub fn from_world(origin: Vec3, boxes: Vec<Aabb>) -> Self {
        let local_boxes = boxes
            .iter()
            .map(|bounds| Aabb::new(bounds.min - origin, bounds.max - origin))
            .collect::<Vec<_>>();
        Self {
            origin,
            boxes,
            shapes_block: is_whole_cell(&local_boxes),
            local_boxes,
            moving_piston: false,
            grids: std::cell::OnceCell::new(),
        }
    }

    /// Whether the box reaches into a box of the shape, so that VoxelShape#collideX finds it
    /// within the shape's cells (`reaches_into`).
    fn is_entered_by(&self, entity: Aabb) -> bool {
        self.boxes
            .iter()
            .any(|bounds| reaches_into(entity, *bounds))
    }

    /// VoxelShape.getCoords: Shapes.create gives a box on a 1/2^n grid of its block on every axis
    /// that whole grid, any other box its bounds; Shapes.or joins them.
    pub(crate) fn grid(&self, axis: Axis, test: GridTest) -> Vec<f64> {
        grid_lines(self.boxes.iter().map(|bounds| (*bounds, self.origin)), axis, test)
    }

    /// VoxelShape#collide of the block's shape: one walk of its grid (VoxelShape.collideX).
    fn collide(&self, axis: Axis, entity: Aabb, delta: f64, test: GridTest) -> f64 {
        let walk = |test| [Axis::X, Axis::Y, Axis::Z].map(|axis| self.grid(axis, test));
        let fresh;
        let grids = match self.grids.get() {
            Some((cached, grids)) if *cached == test => grids,
            Some(_) => {
                fresh = walk(test);
                &fresh
            }
            None => &self.grids.get_or_init(|| (test, walk(test))).1,
        };
        collide_over_grid(grids, &self.boxes, axis, entity, delta)
    }

    /// BlockState.hasLargeCollisionShape (exceedsCube before 1.17): a box reaches out of the cell.
    fn is_large(&self) -> bool {
        self.local_boxes.iter().any(|bounds| {
            bounds.min.x < 0.0
                || bounds.min.y < 0.0
                || bounds.min.z < 0.0
                || bounds.max.x > 1.0
                || bounds.max.y > 1.0
                || bounds.max.z > 1.0
        })
    }

    /// VoxelShape.collide of the shape moved to its block (an element of Shapes.collide's list):
    /// an empty shape passes the movement on, a non-empty one drops a movement under 1.0E-7 and
    /// clips the rest over its grid where the box is inside it, else box by box with no check in
    /// between.
    fn collide_moved(&self, axis: Axis, entity: Aabb, delta: f64, test: GridTest) -> f64 {
        if self.boxes.is_empty() {
            return delta;
        }
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        // (box by box is the grid's walk for one box, and for more unless the box is within a
        // rounding of a face: the walk ends at the first full layer whether that clips or not)
        if self.boxes.len() > 1 || self.is_entered_by(entity) {
            self.collide(axis, entity, delta, test)
        } else {
            clip_boxes(&self.boxes, entity, delta, axis)
        }
    }

    /// VoxelShape.calculateMaxDistance of the shape in its block against the box moved by minus
    /// the block's position, as 1.14 - 1.17's single-axis walk takes it (`box.offset(-x, -y,
    /// -z)`, 1.16.5 Shapes.java:281-281): the gaps are taken in block coordinates.
    fn collide_local(&self, axis: Axis, entity: Aabb, delta: f64, test: GridTest) -> f64 {
        if self.local_boxes.is_empty() {
            return delta;
        }
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        let local = entity.translated(self.origin.reverse());
        if self.local_boxes.len() > 1 || self.local_boxes.iter().any(|bounds| reaches_into(local, *bounds)) {
            let grids = [Axis::X, Axis::Y, Axis::Z].map(|axis| {
                grid_lines(self.local_boxes.iter().map(|bounds| (*bounds, Vec3::ZERO)), axis, test)
            });
            collide_over_grid(&grids, &self.local_boxes, axis, local, delta)
        } else {
            clip_boxes(&self.local_boxes, local, delta, axis)
        }
    }
}

/// Puts block shapes in the order BlockCollisions walks their cells (Cursor3D: x fastest, then y,
/// then z; client1.21.11 Cursor3D.java:31-40). Cursor3D over any box gives its cells in this same
/// order, so the shapes of a larger query keep it for every smaller one.
pub fn sort_in_cursor_order(blocks: &mut [BlockVoxelShape]) {
    blocks.sort_by(|left, right| {
        left.origin
            .z
            .total_cmp(&right.origin.z)
            .then_with(|| left.origin.y.total_cmp(&right.origin.y))
            .then_with(|| left.origin.x.total_cmp(&right.origin.x))
    });
}

/// VoxelShape.collide of a shape of these boxes that a box is not inside: an empty shape passes
/// the movement on; a non-empty one drops a movement under 1.0E-7 (VoxelShape.collideX,
/// client1.21.11 VoxelShape.java:207-212) and clips the rest by each box in turn.
fn clip_boxes(boxes: &[Aabb], entity: Aabb, delta: f64, axis: Axis) -> f64 {
    if boxes.is_empty() {
        return delta;
    }
    if delta.abs() < COLLISION_EPSILON {
        return 0.0;
    }
    boxes
        .iter()
        .fold(delta, |delta, bounds| clip_box(entity, *bounds, delta, axis))
}

/// VoxelShape.collide of the border's shape: its edges are its grid, and a box inside it stops at
/// the first full layer beyond its leading face. Past a corner the box stands in the row outside
/// one edge, whose next cell is still outside the other edge, so that cell holds it at the first
/// edge (26.3 VoxelShape.java:226-238, WorldBorder.java:405-406).
fn collide_border(boxes: &[Aabb], entity: Aabb, delta: f64, axis: Axis) -> f64 {
    if !boxes.iter().any(|bounds| reaches_into(entity, *bounds)) {
        return clip_boxes(boxes, entity, delta, axis);
    }
    let grids = [Axis::X, Axis::Y, Axis::Z].map(|axis| {
        let mut lines: Vec<f64> = boxes
            .iter()
            .flat_map(|bounds| [component(bounds.min, axis), component(bounds.max, axis)])
            .collect();
        lines.sort_by(f64::total_cmp);
        lines.dedup();
        lines
    });
    collide_over_grid(&grids, boxes, axis, entity, delta)
}

/// One VoxelShape of Shapes.collide's list.
#[derive(Clone, Copy, Debug)]
enum Collider<'a> {
    /// A shape of one box: an entity collider (Shapes.create of its box) or a box of a flat list.
    Box(Aabb),
    /// The full cubes 1.13 joins into one shape.
    Joined(&'a [Aabb]),
    /// The world border's shape (an empty slice stands for a border shape that meets nothing). A
    /// box inside it collides with it over its grid, as a block's (collide_border).
    Border(&'a [Aabb]),
    /// A block's shape, with the release's test of its grid.
    Block(&'a BlockVoxelShape, GridTest),
}

impl Collider<'_> {
    /// VoxelShape#collide of one collider: a box's clip, the border's grid, a block's shape.
    fn collide(self, axis: Axis, entity: Aabb, delta: f64) -> f64 {
        match self {
            Self::Box(bounds) => clip_axis(entity, bounds, delta, axis),
            Self::Joined(boxes) => clip_boxes(boxes, entity, delta, axis),
            Self::Border(boxes) => collide_border(boxes, entity, delta, axis),
            Self::Block(shape, test) => shape.collide_moved(axis, entity, delta, test),
        }
    }
}

/// The colliders of a move as Entity.collectColliders lists them: the entity colliders, then the
/// world border's shape, then the blocks (client1.21.11 Entity.java:1146-1160; 1.14.4 - 1.17.1
/// put the entity colliders and the border before getBlockCollisions too, 1.16.5
/// Entity.java:790-795, 830-832).
#[derive(Clone, Copy, Default)]
pub struct Colliders<'a> {
    /// Level.getEntityCollisions of the whole move, each a shape of its own. Vanilla collects
    /// them once for the move and every step-up attempt (client1.21.11 Entity.java:1096).
    pub entities: &'a [Aabb],
    /// The world border's shape where it is among the colliders.
    pub border: Option<&'a [Aabb]>,
    /// From 1.18 whether the border is among the colliders is asked again of each collideBoundingBox
    /// and collectColliders, of its own box: `isInsideCloseToBorder(entity, box)`, whose margin is the
    /// box's size (1.18 Entity.java:800-801, 1.20.6 :913, 1.21.11 :1126). Where it is set, `border`
    /// is the border's shape and this says which attempts take it.
    pub border_near: Option<BorderNear<'a>>,
    /// Boxes each a shape of its own (a flat obstacle list), after the border.
    pub boxes: &'a [Aabb],
    /// The border's shape getBlockCollisions offers again at the head of the blocks, for an
    /// entity whose box touches it from within (1.14 - 1.17.1: 1.15 CollisionGetter.java:80-88,
    /// 1.16.5 CollisionSpliterator.java:56-58, 100-112; WorldSnapshot::
    /// block_collisions_offer_world_border).
    pub block_border: Option<&'a [Aabb]>,
    /// The blocks' shapes in BlockCollisions' order (Cursor3D: x fastest, then y, then z,
    /// client1.21.11 Cursor3D.java:31-40), last; each attempt takes those meeting its box.
    pub blocks: &'a [BlockVoxelShape],
    /// The release whose BlockCollisions tests the blocks (`shape_meets_query`); strict overlap
    /// without one.
    pub release: Option<MinecraftVersion>,
}

/// The test of whether the world border is among a collision attempt's colliders, of its box.
pub type BorderNear<'a> = &'a dyn Fn(Aabb) -> bool;

impl<'a> Colliders<'a> {
    /// The colliders of an attempt whose box is `query`: without the border where border_near says
    /// it is not close to it (WorldBorder#isInsideCloseToBorder of the box Entity#collideBoundingBox
    /// and Entity#collectColliders give it).
    fn border_for(self, query: Aabb) -> Self {
        match self.border_near {
            Some(near) if !near(query) => Colliders { border: None, ..self },
            _ => self,
        }
    }

    /// The shapes of an attempt sweeping `query`: every entity collider and the border, and the
    /// boxes and blocks meeting the query (Entity#collectColliders).
    fn listed(self, query: Aabb) -> Vec<Collider<'a>> {
        let test = GridTest::of(self.release);
        self.entities
            .iter()
            .map(|bounds| Collider::Box(*bounds))
            .chain(self.border.map(Collider::Border))
            .chain(
                self.boxes
                    .iter()
                    .filter(|bounds| bounds.intersects(query))
                    .map(|bounds| Collider::Box(*bounds)),
            )
            .chain(self.block_border.map(Collider::Border))
            .chain(
                self.blocks
                    .iter()
                    .filter(|shape| shape.meets(query, self.release))
                    .map(|shape| Collider::Block(shape, test)),
            )
            .collect()
    }
}

/// VoxelShape.getCoords of boxes each made by Shapes.create in the block at its origin (and moved
/// with it): a box on a 1/2^n grid of that block on every axis brings the whole grid, any other
/// box its bounds; Shapes.or joins the lines.
pub(crate) fn grid_lines(
    boxes: impl Iterator<Item = (Aabb, Vec3)>,
    axis: Axis,
    test: GridTest,
) -> Vec<f64> {
    let mut lines: Vec<f64> = Vec::new();
    let mut count = 0;
    for (bounds, block) in boxes {
        count += 1;
        let origin = component(block, axis);
        let local = Aabb::new(bounds.min - block, bounds.max - block);
        let bits = [Axis::X, Axis::Y, Axis::Z]
            .map(|axis| grid_bits(component(local.min, axis), component(local.max, axis), test));
        if bits.iter().all(Option::is_some) {
            let parts = 1_u32 << bits[axis_index(axis)].unwrap_or(0);
            lines.extend((0..=parts).map(|line| f64::from(line) / f64::from(parts) + origin));
        } else {
            lines.extend([component(bounds.min, axis), component(bounds.max, axis)]);
        }
    }
    lines.sort_by(f64::total_cmp);
    if count == 1 {
        // (one box is Shapes.create's own, and no merger has run over it)
        return lines;
    }
    // IndirectMerger: a coordinate joins the last kept when `d >= e - 1.0E-7` (1.21.11
    // IndirectMerger.java:53, 1.16.5 IndirectMerger.java:37; 1.13.2's `>` differs on equality alone)
    lines.dedup_by(|line, kept| *kept >= *line - COLLISION_EPSILON);
    lines
}

/// VoxelShape.collideX over the shape's grid: the first layer of cells beyond the face the box
/// leads with that holds a full cell across the box stops it at that layer. A box inside the
/// shape so passes the layers it is in.
pub(crate) fn collide_over_grid(
    grids: &[Vec<f64>; 3],
    boxes: &[Aabb],
    axis: Axis,
    entity: Aabb,
    delta: f64,
) -> f64 {
    if delta.abs() < COLLISION_EPSILON {
        return 0.0;
    }
    let grid = |axis: Axis| &grids[axis_index(axis)];
    let cells = |axis: Axis| grid(axis).len() - 1;
    let find_index = |axis: Axis, value: f64| {
        grid(axis).iter().take_while(|line| **line <= value).count() as isize - 1
    };
    let across = |other: Axis| {
        let first = find_index(other, component(entity.min, other) + COLLISION_EPSILON);
        let last = find_index(other, component(entity.max, other) - COLLISION_EPSILON);
        first.max(0) as usize..((last + 1).max(0) as usize).min(cells(other))
    };
    let [first, second] = axis.others();
    // (the cells of the border's shape reach to the infinities)
    let center = |axis: Axis, cell: usize| match (grid(axis)[cell], grid(axis)[cell + 1]) {
        (low, high) if low.is_infinite() && high.is_infinite() => 0.0,
        (low, high) if low.is_infinite() => high - 1.0,
        (low, high) if high.is_infinite() => low + 1.0,
        (low, high) => (low + high) / 2.0,
    };
    let is_full = |layer: usize| {
        across(first).any(|cell_first| {
            across(second).any(|cell_second| {
                let point = set_component(
                    set_component(
                        set_component(Vec3::ZERO, axis, center(axis, layer)),
                        first,
                        center(first, cell_first),
                    ),
                    second,
                    center(second, cell_second),
                );
                boxes.iter().any(|bounds| bounds.contains(point))
            })
        })
    };
    if delta > 0.0 {
        let leading = component(entity.max, axis);
        let behind = find_index(axis, leading - COLLISION_EPSILON);
        let layers = (behind + 1).max(0) as usize..cells(axis);
        if let Some(layer) = layers.into_iter().find(|layer| is_full(*layer)) {
            let gap = grid(axis)[layer] - leading;
            if gap >= -COLLISION_EPSILON {
                return delta.min(gap);
            }
        }
    } else {
        let leading = component(entity.min, axis);
        let behind = find_index(axis, leading + COLLISION_EPSILON);
        let layers = 0..behind.max(0) as usize;
        if let Some(layer) = layers.rev().find(|layer| is_full(*layer)) {
            let gap = grid(axis)[layer + 1] - leading;
            if gap <= COLLISION_EPSILON {
                return delta.max(gap);
            }
        }
    }
    delta
}

/// How Entity.collide steps up onto what blocks a horizontal movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepUp {
    /// Until 1.20.6: the movement lifted by the step height, or after the highest lift the
    /// swept box allows, then lowered back.
    Lifted,
    /// From 1.21: the movement lifted to each height of a collider's faces within the step,
    /// lowest first, until one goes further.
    CandidateHeights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollisionRules {
    pub step_up: StepUp,
    /// Until 1.17 the world border's shape is a collider wherever the entity is inside it, so
    /// a movement always meets a collider, even one that sweeps no block.
    /// 1.14.4 - 1.17.1: a movement along one axis (two components zero) walks the blocks cell by
    /// cell in block coordinates (Entity.collideBoundingBoxHeuristically, client1.14.4
    /// Entity.java:725-735; 1.16.5 Entity.java:824-836, Shapes.java:232-296; 1.17
    /// Shapes.java:223-283; gone in 1.18).
    pub single_axis_walk: bool,
}

/// Entity.collide over the colliders as shapes: each VoxelShape of the list, entity collider,
/// world border or block, drops a movement under 1.0E-7 before it collides (Shapes.collide,
/// client1.21.11 Shapes.java:175-185), and a block's shape resolves in one walk of its grid
/// (VoxelShape.collideX, VoxelShape.java:207-212), so the check falls between whole shapes in the
/// order Entity.collectColliders lists them, not between their boxes.
#[must_use]
pub fn collide_shapes(
    bounds: Aabb,
    movement: Vec3,
    colliders: Colliders<'_>,
    on_ground: bool,
    max_step: f32,
    rules: CollisionRules,
) -> Vec3 {
    let collided = if movement.length_squared() == 0.0 {
        movement
    } else {
        collide_bounding_box(bounds, movement, colliders, rules)
    };
    let landing = movement.y != collided.y && movement.y < 0.0;
    let blocked = movement.x != collided.x || movement.z != collided.z;
    if max_step <= 0.0 || !(on_ground || landing) || !blocked {
        return collided;
    }
    match rules.step_up {
        StepUp::Lifted => {
            let colliders = |bounds: Aabb, movement: Vec3| {
                collide_bounding_box(bounds, movement, colliders, rules)
            };
            lifted_step_up(bounds, movement, collided, colliders, f64::from(max_step))
        }
        StepUp::CandidateHeights => {
            candidate_step_up(bounds, movement, collided, landing, colliders, max_step)
        }
    }
}

/// Entity#collide's step up until 1.20.6: the move lifted by the step height, or the horizontal
/// move after the highest lift the swept box allows, whichever goes further, lowered back.
fn lifted_step_up(
    bounds: Aabb,
    movement: Vec3,
    collided: Vec3,
    collide_bounding_box: impl Fn(Aabb, Vec3) -> Vec3,
    max_step: f64,
) -> Vec3 {
    let horizontal = Vec3::new(movement.x, 0.0, movement.z);
    let mut stepped = collide_bounding_box(bounds, Vec3::new(movement.x, max_step, movement.z));
    let lift = collide_bounding_box(
        bounds.expand_towards(horizontal),
        Vec3::new(0.0, max_step, 0.0),
    );
    if lift.y < max_step {
        let lifted = collide_bounding_box(bounds.translated(lift), horizontal) + lift;
        if lifted.horizontal_length_squared() > stepped.horizontal_length_squared() {
            stepped = lifted;
        }
    }
    if stepped.horizontal_length_squared() <= collided.horizontal_length_squared() {
        return collided;
    }
    let lowering = Vec3::new(0.0, -stepped.y + movement.y, 0.0);
    stepped + collide_bounding_box(bounds.translated(stepped), lowering)
}

/// Entity.collide from 1.21: collectColliders gives every entity collider of the move, the
/// border and the blocks meeting the step's box (client1.21.11 Entity.java:1074-1090);
/// collectCandidateStepUpHeights takes their y coordinates.
fn candidate_step_up(
    bounds: Aabb,
    movement: Vec3,
    collided: Vec3,
    landing: bool,
    colliders: Colliders<'_>,
    max_step: f32,
) -> Vec3 {
    let grounded = if landing {
        bounds.translated(Vec3::new(0.0, collided.y, 0.0))
    } else {
        bounds
    };
    let mut query = grounded.expand_towards(Vec3::new(movement.x, f64::from(max_step), movement.z));
    if !landing {
        query = query.expand_towards(Vec3::new(0.0, -9.999999747378752e-6, 0.0));
    }
    let colliders = colliders.border_for(query);
    // a flat list's boxes bring the whole shape of their block (BlockCollisions)
    let test = GridTest::of(colliders.release);
    let boxes = block_shapes_in(colliders.boxes, query);
    let blocks: Vec<BlockVoxelShape> = colliders
        .blocks
        .iter()
        .filter(|shape| shape.meets(query, colliders.release))
        .cloned()
        .collect();
    let block_coordinates: Vec<Vec<f64>> = boxes
        .iter()
        .map(|collider| y_coordinates(*collider, test))
        .chain(blocks.iter().map(|shape| shape.grid(Axis::Y, test)))
        .collect();
    let heights = candidate_step_heights(
        grounded,
        colliders.entities,
        &block_coordinates,
        max_step,
        collided.y as f32,
        test,
    );
    let step_colliders = Colliders {
        boxes: &boxes,
        blocks: &blocks,
        ..colliders
    };
    let listed = step_colliders.listed(query);
    for height in heights {
        let lifted = Vec3::new(movement.x, f64::from(height), movement.z);
        let stepped = collide_with_shapes(lifted, grounded, &listed, colliders.release);
        if stepped.horizontal_length_squared() > collided.horizontal_length_squared() {
            let distance_to_ground = bounds.min.y - grounded.min.y;
            return stepped + Vec3::new(0.0, -distance_to_ground, 0.0);
        }
    }
    collided
}

/// Entity#collectCandidateStepUpHeights: each collider's y coordinates as heights above the box,
/// those under zero or equal to the collided y skipped, a collider's walk ending past the step,
/// sorted.
fn candidate_step_heights(
    grounded: Aabb,
    entities: &[Aabb],
    blocks: &[Vec<f64>],
    max_step: f32,
    skipped: f32,
    test: GridTest,
) -> Vec<f32> {
    let mut heights: Vec<f32> = Vec::new();
    let coordinates = entities
        .iter()
        .map(|collider| entity_y_coordinates(*collider, test))
        .chain(blocks.iter().cloned());
    for collider_coordinates in coordinates {
        for coordinate in collider_coordinates {
            let height = (coordinate - grounded.min.y) as f32;
            if height < 0.0 || height == skipped {
                continue;
            }
            if height > max_step {
                break;
            }
            if !heights.contains(&height) {
                heights.push(height);
            }
        }
    }
    heights.sort_by(f32::total_cmp);
    heights
}

/// VoxelShape.getCoords(Y) of a block's box: the grid lines of a box aligned to eighths of its
/// block (Shapes.create), else its bottom and top.
fn y_coordinates(collider: Aabb, test: GridTest) -> Vec<f64> {
    let origin = block_origin(collider);
    let local = Aabb::new(collider.min - origin, collider.max - origin);
    let aligned = [
        grid_bits(local.min.x, local.max.x, test),
        grid_bits(local.min.y, local.max.y, test),
        grid_bits(local.min.z, local.max.z, test),
    ];
    match aligned {
        [Some(_), Some(bits), Some(_)] => {
            let parts = 1_u32 << bits;
            (0..=parts)
                .map(|line| f64::from(line) / f64::from(parts) + origin.y)
                .collect()
        }
        _ => vec![collider.min.y, collider.max.y],
    }
}

/// VoxelShape.getCoords(Y) of an entity collider: Shapes.create(AABB) of its box in world
/// coordinates, a grid only for a box on a 1/2^n grid of the cube at the world's origin.
fn entity_y_coordinates(collider: Aabb, test: GridTest) -> Vec<f64> {
    grid_lines(std::iter::once((collider, Vec3::ZERO)), Axis::Y, test)
}

/// How Shapes.findBits tells a coordinate on a 1/2^n grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GridTest {
    /// abs(f - floor(f)) < 1.0E-7 of the scaled coordinate, before 1.17: a coordinate just under
    /// a line is off the grid (1.13.2 Shapes.java:99, 1.16.5 :113).
    Floor,
    /// abs(f - round(f)) < 1.0E-7 * 2^n, from 1.17 (1.17 Shapes.java:107).
    Round,
}

impl GridTest {
    /// The test of a release, the current one without a release (1.16.5 Shapes.java:113-114
    /// floor; 1.17 Shapes.java:107-108 round).
    pub(crate) fn of(release: Option<MinecraftVersion>) -> Self {
        match release {
            Some(release) if !release.at_least(1, 17, 0) => Self::Floor,
            _ => Self::Round,
        }
    }
}

/// Shapes#findBits: the least n of 0 to 3 that puts both coordinates on a 1/2^n grid.
pub(crate) fn grid_bits(min: f64, max: f64, test: GridTest) -> Option<u32> {
    if min < -1.0e-7 || max > 1.0000001 {
        return None;
    }
    (0..=3).find(|bits| {
        let parts = f64::from(1_u32 << bits);
        let aligned = |value: f64| {
            let scaled = value * parts;
            match test {
                GridTest::Floor => (scaled - scaled.floor()).abs() < 1.0e-7,
                GridTest::Round => (scaled - scaled.round()).abs() < 1.0e-7 * parts,
            }
        };
        aligned(min) && aligned(max)
    })
}

fn block_origin(collider: Aabb) -> Vec3 {
    Vec3::new(
        collider.min.x.floor(),
        collider.min.y.floor(),
        collider.min.z.floor(),
    )
}

/// BlockCollisions: the whole shape of every block with a box meeting the query
/// (BlockCollisions#computeNext).
fn block_shapes_in(obstacles: &[Aabb], query: Aabb) -> Vec<Aabb> {
    let blocks: Vec<Vec3> = obstacles
        .iter()
        .filter(|obstacle| obstacle.intersects(query))
        .map(|obstacle| block_origin(*obstacle))
        .collect();
    obstacles
        .iter()
        .copied()
        .filter(|obstacle| blocks.contains(&block_origin(*obstacle)))
        .collect()
}

/// Entity.collideBoundingBox: the shapes of the colliders meeting the swept box, collided axis
/// by axis; in 1.14.4 - 1.17.1 a movement along one axis walks the blocks instead.
fn collide_bounding_box(
    bounds: Aabb,
    movement: Vec3,
    colliders: Colliders<'_>,
    rules: CollisionRules,
) -> Vec3 {
    let zeros = [movement.x, movement.y, movement.z]
        .iter()
        .filter(|component| **component == 0.0)
        .count();
    if rules.single_axis_walk && zeros >= 2 {
        return collide_single_axis(bounds, movement, colliders);
    }
    let query = bounds.expand_towards(movement);
    let colliders = colliders.border_for(query);
    let listed = colliders.listed(query);
    collide_with_shapes(movement, bounds, &listed, colliders.release)
}

/// Entity.collideWithShapes: no collision without a collider, else y, then the larger of x and z
/// after the smaller (Direction.axisStepOrder).
///
/// From 1.21.5 the result starts from Vec3.ZERO and takes only the axes that moved, so an axis of
/// -0.0 comes back +0.0 (1.21.5 Entity.java:1059-1069); before, `new Vec3(d, e, f)` of the movement
/// keeps it (1.21.4 Entity.java:1011-1035). No collider gives the movement itself on both sides.
fn collide_with_shapes(
    movement: Vec3,
    bounds: Aabb,
    colliders: &[Collider<'_>],
    release: Option<MinecraftVersion>,
) -> Vec3 {
    if colliders.is_empty() {
        return movement;
    }
    let collided = collide_axes(movement, bounds, |axis, entity, delta| {
        shapes_collide(axis, entity, colliders, delta)
    });
    if !release.is_some_and(|release| release.at_least(1, 21, 5)) {
        return collided;
    }
    let moved = |delta: f64, value: f64| if delta == 0.0 { 0.0 } else { value };
    Vec3::new(moved(movement.x, collided.x), moved(movement.y, collided.y), moved(movement.z, collided.z))
}

/// The movement collided on y, then on the larger of x and z after the smaller, the box moved
/// after each axis that moved but the last (Entity.collideBoundingBox / collideWithShapes).
fn collide_axes(
    movement: Vec3,
    bounds: Aabb,
    collide_axis: impl Fn(Axis, Aabb, f64) -> f64,
) -> Vec3 {
    let mut entity = bounds;
    let Vec3 {
        mut x,
        mut y,
        mut z,
    } = movement;
    if y != 0.0 {
        y = collide_axis(Axis::Y, entity, y);
        if y != 0.0 {
            entity = entity.translated(Vec3::new(0.0, y, 0.0));
        }
    }
    let z_first = x.abs() < z.abs();
    if z_first && z != 0.0 {
        z = collide_axis(Axis::Z, entity, z);
        if z != 0.0 {
            entity = entity.translated(Vec3::new(0.0, 0.0, z));
        }
    }
    if x != 0.0 {
        x = collide_axis(Axis::X, entity, x);
        if !z_first && x != 0.0 {
            entity = entity.translated(Vec3::new(x, 0.0, 0.0));
        }
    }
    if !z_first && z != 0.0 {
        z = collide_axis(Axis::Z, entity, z);
    }
    Vec3::new(x, y, z)
}

/// Shapes.collide: before each shape a movement under 1.0E-7 is dropped, then the shape collides
/// it (client1.21.11 Shapes.java:175-185).
fn shapes_collide(axis: Axis, entity: Aabb, colliders: &[Collider<'_>], mut delta: f64) -> f64 {
    for collider in colliders {
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        delta = collider.collide(axis, entity, delta);
    }
    delta
}

/// Entity.collideBoundingBox of a movement along one axis in 1.14.4 - 1.17.1: each axis walks the
/// blocks around the box (Shapes.collide(AABB, LevelReader, ...)), then the entity colliders and
/// the border collide what is left. A flat list's boxes collide with them.
fn collide_single_axis(bounds: Aabb, movement: Vec3, colliders: Colliders<'_>) -> Vec3 {
    let blocks: FastMap<(i32, i32, i32), &BlockVoxelShape> = colliders
        .blocks
        .iter()
        .map(|shape| {
            let origin = shape.origin;
            ((origin.x as i32, origin.y as i32, origin.z as i32), shape)
        })
        .collect();
    let others: Vec<Collider<'_>> = colliders
        .entities
        .iter()
        .map(|bounds| Collider::Box(*bounds))
        .chain(colliders.border.map(Collider::Border))
        .chain(colliders.boxes.iter().map(|bounds| Collider::Box(*bounds)))
        .collect();
    let test = GridTest::of(colliders.release);
    collide_axes(movement, bounds, |axis, entity, delta| {
        walk_axis(axis, entity, &blocks, &others, delta, test)
    })
}

/// Shapes.collide (1.16.5 Shapes.java:235-296, client1.14.4
/// Shapes.collide(AABB, LevelReader, ...)): a box under 1.0E-6 on an axis passes the movement on;
/// else a movement under 1.0E-7 is dropped, and the cells around the box are walked layer by layer
/// along the movement (the movement axis outermost, then the axes AxisCycle puts after it), up to
/// the layer the movement reaches. A cell on one face of the walk counts only for a large shape,
/// which a moving piston's always is (its dynamicShape leaves the state no cache, and
/// hasLargeCollisionShape is then true: 1.17.1 Blocks.java:360-371, BlockBehaviour.java:435-437;
/// 1.14.4 BlockState.java:96-98), and one on an edge only for a moving piston (1.14.4
/// Shapes.java:239, 1.17.1 :268 `s != 2 || is(Blocks.MOVING_PISTON)`), whose shape is its
/// block entity's at its progress; each block's shape collides the box in block coordinates, and a movement it
/// leaves under 1.0E-7 is dropped at once. The entity colliders and the border collide the rest,
/// each with only its own check.
fn walk_axis(
    axis: Axis,
    entity: Aabb,
    blocks: &FastMap<(i32, i32, i32), &BlockVoxelShape>,
    others: &[Collider<'_>],
    mut delta: f64,
    test: GridTest,
) -> f64 {
    let size = entity.max - entity.min;
    if size.x < 1.0e-6 || size.y < 1.0e-6 || size.z < 1.0e-6 {
        return delta;
    }
    if delta.abs() < COLLISION_EPSILON {
        return 0.0;
    }
    // AxisCycle.between(axis, Z).opposite() cycles X, Y onto the axes after the movement's
    let (first, second) = match axis {
        Axis::X => (Axis::Y, Axis::Z),
        Axis::Y => (Axis::Z, Axis::X),
        Axis::Z => (Axis::X, Axis::Y),
    };
    // Java's int arithmetic (this walk ends at 1.17.1, well before Mth.floor saturates)
    let i = floor_i32(component(entity.min, first) - COLLISION_EPSILON).wrapping_sub(1);
    let j = floor_i32(component(entity.max, first) + COLLISION_EPSILON).wrapping_add(1);
    let k = floor_i32(component(entity.min, second) - COLLISION_EPSILON).wrapping_sub(1);
    let l = floor_i32(component(entity.max, second) + COLLISION_EPSILON).wrapping_add(1);
    let low = component(entity.min, axis) - COLLISION_EPSILON;
    let high = component(entity.max, axis) + COLLISION_EPSILON;
    let positive = delta > 0.0;
    let m = if positive {
        floor_i32(component(entity.max, axis) - COLLISION_EPSILON).wrapping_sub(1)
    } else {
        floor_i32(component(entity.min, axis) + COLLISION_EPSILON).wrapping_add(1)
    };
    let reach = |delta: f64| {
        if delta > 0.0 {
            floor_i32(high + delta).wrapping_add(1)
        } else {
            floor_i32(low + delta).wrapping_sub(1)
        }
    };
    let mut n = reach(delta);
    let step = if positive { 1 } else { -1 };
    let mut p = m;
    while if positive { p <= n } else { p >= n } {
        for q in i..=j {
            for r in k..=l {
                let faces = i32::from(q == i || q == j)
                    + i32::from(r == k || r == l)
                    + i32::from(p == m || p == n);
                if faces >= 3 {
                    continue;
                }
                let mut cell = [0; 3];
                cell[axis_index(first)] = q;
                cell[axis_index(second)] = r;
                cell[axis_index(axis)] = p;
                let Some(shape) = blocks.get(&(cell[0], cell[1], cell[2])) else {
                    continue;
                };
                if faces == 2 && !shape.moving_piston {
                    continue;
                }
                if faces == 1 && !shape.moving_piston && !shape.is_large() {
                    continue;
                }
                delta = shape.collide_local(axis, entity, delta, test);
                if delta.abs() < COLLISION_EPSILON {
                    return 0.0;
                }
                n = reach(delta);
            }
        }
        p += step;
    }
    others
        .iter()
        .fold(delta, |delta, collider| collider.collide(axis, entity, delta))
}

/// The box and the movement Entity.move reports after its collision before 1.14. A step up moves
/// the box up and back down, and reports the lowering as the vertical movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyMove {
    pub bounds: Aabb,
    pub movement: Vec3,
}

/// Entity.move's collision before 1.14: the box moves on y, then x, then z against the boxes the
/// move sweeps, AxisAlignedBB's offsets before 1.13 and VoxelShape's from 1.13. A grounded or
/// landing entity blocked horizontally also tries the move lifted by the step height, against
/// the box swept by the horizontal move and against the box itself, keeps the one that goes
/// further, lowers it back by the lift and takes it when it goes further than the plain move.
#[must_use]
pub fn collide_legacy(
    bounds: Aabb,
    movement: Vec3,
    obstacles: &[Aabb],
    entities: &[Aabb],
    on_ground: bool,
    step_height: f32,
    voxel_shapes: bool,
) -> LegacyMove {
    let offset = |axis: Axis, entity: Aabb, colliders: &LegacyColliders, delta: f64| {
        if voxel_shapes {
            shapes_collide(axis, entity, &colliders.listed(), delta)
        } else {
            colliders.flat.iter().fold(delta, |delta, collider| {
                aabb_offset(entity, *collider, delta, axis)
            })
        }
    };
    let colliders = legacy_colliders(obstacles, entities, bounds, movement, voxel_shapes);
    let Vec3 { x: m, y: n, z: o } = movement;
    let (mut x, mut y, mut z) = (m, n, o);
    let mut moved = bounds;
    if n != 0.0 {
        y = offset(Axis::Y, moved, &colliders, n);
        moved = moved.translated(Vec3::new(0.0, y, 0.0));
    }
    if m != 0.0 {
        x = offset(Axis::X, moved, &colliders, m);
        moved = moved.translated(Vec3::new(x, 0.0, 0.0));
    }
    if o != 0.0 {
        z = offset(Axis::Z, moved, &colliders, o);
        moved = moved.translated(Vec3::new(0.0, 0.0, z));
    }
    let plain = LegacyMove {
        bounds: moved,
        movement: Vec3::new(x, y, z),
    };
    let grounded = on_ground || (n != y && n < 0.0);
    if step_height <= 0.0 || !grounded || (m == x && o == z) {
        return plain;
    }
    let step = f64::from(step_height);
    let step_colliders = legacy_colliders(
        obstacles,
        entities,
        bounds,
        Vec3::new(m, step, o),
        voxel_shapes,
    );
    let across = |rise: f64| {
        let raised = bounds.translated(Vec3::new(0.0, rise, 0.0));
        let dx = offset(Axis::X, raised, &step_colliders, m);
        let moved_x = raised.translated(Vec3::new(dx, 0.0, 0.0));
        let dz = offset(Axis::Z, moved_x, &step_colliders, o);
        LegacyMove {
            bounds: moved_x.translated(Vec3::new(0.0, 0.0, dz)),
            movement: Vec3::new(dx, rise, dz),
        }
    };
    let swept_box = bounds.expand_towards(Vec3::new(m, 0.0, o));
    let swept = across(offset(Axis::Y, swept_box, &step_colliders, step));
    let lifted = across(offset(Axis::Y, bounds, &step_colliders, step));
    let stepped = if swept.movement.horizontal_length_squared()
        > lifted.movement.horizontal_length_squared()
    {
        swept
    } else {
        lifted
    };
    let down = offset(
        Axis::Y,
        stepped.bounds,
        &step_colliders,
        -stepped.movement.y,
    );
    if x * x + z * z >= stepped.movement.horizontal_length_squared() {
        return plain;
    }
    LegacyMove {
        bounds: stepped.bounds.translated(Vec3::new(0.0, down, 0.0)),
        movement: Vec3::new(stepped.movement.x, down, stepped.movement.z),
    }
}

/// The colliders of a move before 1.14: AxisAlignedBB lists before 1.13 (`flat`), VoxelShapes
/// from 1.13 (the blocks' shapes, the full cubes joined in one, the entity boxes).
#[derive(Debug, Default)]
struct LegacyColliders {
    flat: Vec<Aabb>,
    blocks: Vec<BlockVoxelShape>,
    full_cubes: Vec<Aabb>,
    entities: Vec<Aabb>,
}

impl LegacyColliders {
    /// The VoxelShapes in the order 1.13's getCollisionBoxes streams them (LevelReader#method_16378:
    /// the blocks' shapes, then the full cubes' joined shape; the entity boxes after).
    fn listed(&self) -> Vec<Collider<'_>> {
        self.blocks
            .iter()
            .map(|shape| Collider::Block(shape, GridTest::Floor))
            .chain((!self.full_cubes.is_empty()).then_some(Collider::Joined(&self.full_cubes)))
            .chain(self.entities.iter().map(|bounds| Collider::Box(*bounds)))
            .collect()
    }
}

/// Level.doesBoxCollide before 1.14, the entity colliders after the blocks. From 1.13 the
/// query is LevelReader.method_16365: the box swept and grown by 1.0E-7, less the box moved 1.0E-7
/// back along the move (an L of the leading faces, and of the low faces on an axis it does not
/// move on), and each join is on the merged grid (legacy_shapes.rs). Over the cells around that
/// shape, x fastest, but those on two faces of the range, it leaves out the blocks the box is in,
/// gives each other block's shape that meets it, then the full cubes joined in one shape if that
/// meets it, then the entity boxes meeting its bounds (1.13.2 LevelReader.java:113-167, 173-180,
/// EntityView:24-37).
fn legacy_colliders(
    obstacles: &[Aabb],
    entities: &[Aabb],
    bounds: Aabb,
    movement: Vec3,
    voxel_shapes: bool,
) -> LegacyColliders {
    if !voxel_shapes {
        return LegacyColliders {
            flat: obstacles.iter().chain(entities).copied().collect(),
            ..LegacyColliders::default()
        };
    }
    let back = |delta: f64| if delta > 0.0 { -COLLISION_EPSILON } else { COLLISION_EPSILON };
    let epsilon = Vec3::new(COLLISION_EPSILON, COLLISION_EPSILON, COLLISION_EPSILON);
    let inside = Shape::create(bounds);
    let start = Shape::create(bounds.translated(Vec3::new(back(movement.x), back(movement.y), back(movement.z))));
    let query = only_first(&Shape::create(bounds.expand_towards(movement).inflate(epsilon)), &start);
    if query.is_empty() {
        return LegacyColliders::default();
    }
    let (query_low, query_high) = query.bounds();
    let low = query_low.map(|value| floor_i32(value).wrapping_sub(1));
    let high = query_high.map(|value| floor_i32(value.ceil()).wrapping_add(1));
    let on_edges = |cell: [i32; 3]| {
        (0..3)
            .filter(|&axis| cell[axis] == low[axis] || cell[axis] == high[axis] - 1)
            .count()
            >= 2
    };
    let mut shapes: Vec<(Vec3, Vec<Aabb>)> = Vec::new();
    for obstacle in obstacles {
        let block = block_origin(*obstacle);
        match shapes.iter_mut().find(|(origin, _)| *origin == block) {
            Some((_, boxes)) => boxes.push(*obstacle),
            None => shapes.push((block, vec![*obstacle])),
        }
    }
    shapes.sort_by(|(left, _), (right, _)| {
        left.z
            .total_cmp(&right.z)
            .then_with(|| left.y.total_cmp(&right.y))
            .then_with(|| left.x.total_cmp(&right.x))
    });
    let mut colliders = LegacyColliders::default();
    let mut cubes: Vec<[i32; 3]> = Vec::new();
    for (origin, boxes) in shapes {
        let cell = [origin.x as i32, origin.y as i32, origin.z as i32];
        if (0..3).any(|axis| cell[axis] < low[axis] || cell[axis] >= high[axis]) || on_edges(cell) {
            continue;
        }
        let local: Vec<Aabb> = boxes.iter().map(|bounds| bounds.translated(origin.reverse())).collect();
        let lines = [Axis::X, Axis::Y, Axis::Z].map(|axis| {
            grid_lines(local.iter().map(|bounds| (*bounds, Vec3::ZERO)), axis, GridTest::Floor)
        });
        let shape = Shape::of_boxes(lines, &local);
        if overlap(&inside.moved(origin.reverse()), &shape) {
            continue;
        }
        if boxes == [Aabb::new(origin, origin + Vec3::new(1.0, 1.0, 1.0))] {
            colliders.full_cubes.extend(boxes);
            cubes.push(cell);
        } else if overlap(&query, &shape.moved(origin)) {
            colliders.blocks.push(BlockVoxelShape::from_world(origin, boxes));
        }
    }
    if !cubes.is_empty() && !overlap(&query, &Shape::of_cubes(low, high, |cell| cubes.contains(&cell))) {
        colliders.full_cubes.clear();
    }
    let reach = Aabb::new(
        Vec3::new(query_low[0], query_low[1], query_low[2]),
        Vec3::new(query_high[0], query_high[1], query_high[2]),
    );
    colliders.entities = entities
        .iter()
        .copied()
        .filter(|entity| entity.intersects(reach))
        .collect();
    colliders
}

/// AxisAlignedBB.calculateX/Y/ZOffset (AABB#method_583, AABB#method_589 and AABB#method_594 of the
/// legacy trees, which the mapping leaves unnamed): exact overlaps, no tolerance.
fn aabb_offset(entity: Aabb, obstacle: Aabb, delta: f64, axis: Axis) -> f64 {
    let overlaps = |other: Axis| {
        component(entity.max, other) > component(obstacle.min, other)
            && component(entity.min, other) < component(obstacle.max, other)
    };
    let [first, second] = axis.others();
    if !overlaps(first) || !overlaps(second) {
        return delta;
    }
    if delta > 0.0 && component(entity.max, axis) <= component(obstacle.min, axis) {
        let gap = component(obstacle.min, axis) - component(entity.max, axis);
        if gap < delta {
            return gap;
        }
    } else if delta < 0.0 && component(entity.min, axis) >= component(obstacle.max, axis) {
        let gap = component(obstacle.max, axis) - component(entity.min, axis);
        if gap > delta {
            return gap;
        }
    }
    delta
}

fn axis_index(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    }
}

/// Vec3#get: the vector's coordinate on the axis.
fn component(value: Vec3, axis: Axis) -> f64 {
    match axis {
        Axis::X => value.x,
        Axis::Y => value.y,
        Axis::Z => value.z,
    }
}

/// Vec3#with: the vector with its coordinate on the axis replaced.
fn set_component(value: Vec3, axis: Axis, component: f64) -> Vec3 {
    match axis {
        Axis::X => Vec3::new(component, value.y, value.z),
        Axis::Y => Vec3::new(value.x, component, value.z),
        Axis::Z => Vec3::new(value.x, value.y, component),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        block_shapes_in, candidate_step_heights, clip_axis, collide_legacy,
        collide_shapes, entity_y_coordinates, sort_in_cursor_order, vanilla_equal,
        collide_border, y_coordinates, Axis, BlockVoxelShape, Colliders, CollisionRules, GridTest,
        StepUp,
    };

    /// (a test's) Entity.collide: the movement collided with the obstacles, stepping up when a grounded or
    /// landing entity runs into something. `obstacles` are boxes each a shape of its own (blocks'
    /// boxes, the world border's), `entities` the entity colliders, which vanilla keeps in a list of
    /// their own and lists first, and `entered` the shapes of the blocks among the obstacles the
    /// entity is inside, which collide as whole shapes after the rest.
    #[allow(clippy::too_many_arguments)]
    fn collide(
        bounds: Aabb,
        movement: Vec3,
        obstacles: &[Aabb],
        entities: &[Aabb],
        entered: &[BlockVoxelShape],
        on_ground: bool,
        max_step: f32,
        rules: CollisionRules,
    ) -> Vec3 {
        let boxes: Vec<Aabb> = obstacles
            .iter()
            .copied()
            .filter(|obstacle| !entered.iter().any(|shape| shape.boxes.contains(obstacle)))
            .collect();
        let colliders = Colliders {
            entities,
            border: None,
            border_near: None,
            boxes: &boxes,
            block_border: None,
            blocks: entered,
            release: None,
        };
        collide_shapes(bounds, movement, colliders, on_ground, max_step, rules)
    }

    use crate::math::{Aabb, Vec3};

    const LIFTED: CollisionRules = CollisionRules {
        step_up: StepUp::Lifted,
        single_axis_walk: false,
    };
    const CANDIDATE_HEIGHTS: CollisionRules = CollisionRules {
        step_up: StepUp::CandidateHeights,
        single_axis_walk: false,
    };

    fn box_at(x: f64, y: f64, z: f64) -> Aabb {
        Aabb::new(Vec3::new(x, y, z), Vec3::new(x + 1.0, y + 1.0, z + 1.0))
    }

    #[test]
    fn clips_positive_motion_at_contact() {
        let entity = box_at(0.0, 0.0, 0.0);
        let obstacle = box_at(2.0, 0.0, 0.0);

        assert_eq!(clip_axis(entity, obstacle, 2.5, Axis::X), 1.0);
    }

    #[test]
    fn clips_negative_motion_at_contact() {
        let entity = box_at(2.0, 0.0, 0.0);
        let obstacle = box_at(0.0, 0.0, 0.0);

        assert_eq!(clip_axis(entity, obstacle, -2.5, Axis::X), -1.0);
    }

    fn half_block_step() -> Aabb {
        Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.5, 1.0))
    }

    #[test]
    fn lifted_step_up_climbs_the_obstacle_and_lowers_back_onto_it() {
        let entity = box_at(0.0, 0.0, 0.0);
        let movement = Vec3::new(1.5, 0.0, 0.0);

        let motion = collide(
            entity,
            movement,
            &[half_block_step()],
            &[],
            &[],
            true,
            1.0,
            LIFTED,
        );

        assert_eq!(motion, Vec3::new(1.5, 0.5, 0.0));
    }

    #[test]
    fn lifted_step_up_of_a_landing_starts_from_the_box_before_the_fall() {
        let entity = Aabb::new(Vec3::new(0.0, 0.1, 0.0), Vec3::new(1.0, 1.1, 1.0));
        let obstacles = [
            box_at(0.0, -1.0, 0.0),
            box_at(1.0, -1.0, 0.0),
            half_block_step(),
        ];
        let movement = Vec3::new(0.5, -0.3, 0.0);

        let motion = collide(entity, movement, &obstacles, &[], &[], false, 0.6, LIFTED);

        let step = f64::from(0.6_f32);
        assert_eq!(motion, Vec3::new(0.5, step + (0.5 - (0.1 + step)), 0.0));
    }

    #[test]
    fn lifted_step_up_keeps_the_straight_lift_when_it_goes_further() {
        let entity = box_at(0.0, 0.0, 0.0);
        let ceiling = Aabb::new(Vec3::new(1.2, 1.3, 0.0), Vec3::new(3.0, 2.3, 1.0));
        let obstacles = [
            box_at(0.0, -1.0, 0.0),
            box_at(1.0, -1.0, 0.0),
            half_block_step(),
            ceiling,
        ];
        let movement = Vec3::new(0.5, -0.0784, 0.0);

        let motion = collide(entity, movement, &obstacles, &[], &[], true, 0.6, LIFTED);

        let step = f64::from(0.6_f32);
        assert_eq!(motion, Vec3::new(1.2 - 1.0, step + (0.5 - step), 0.0));
    }

    #[test]
    fn candidate_step_up_lifts_to_the_lowest_face_that_clears_the_way() {
        let entity = box_at(0.0, 0.0, 0.0);
        let step = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.125, 1.0));
        let ceiling = Aabb::new(Vec3::new(1.0, 1.2, 0.0), Vec3::new(2.0, 2.2, 1.0));
        let obstacles = [
            box_at(0.0, -1.0, 0.0),
            box_at(1.0, -1.0, 0.0),
            step,
            ceiling,
        ];
        let jump = Vec3::new(0.3, 0.42, 0.0);

        let modern = collide(
            entity,
            jump,
            &obstacles,
            &[],
            &[],
            true,
            0.6,
            CANDIDATE_HEIGHTS,
        );
        let legacy = collide(entity, jump, &obstacles, &[], &[], true, 0.6, LIFTED);

        assert_eq!(modern, Vec3::new(0.3, 0.125, 0.0));
        assert!(legacy.y > 0.19);
    }

    #[test]
    fn candidate_step_up_of_a_landing_counts_from_the_ground() {
        let entity = Aabb::new(Vec3::new(0.0, 0.05, 0.0), Vec3::new(1.0, 1.05, 1.0));
        let obstacles = [
            box_at(0.0, -1.0, 0.0),
            box_at(1.0, -1.0, 0.0),
            half_block_step(),
        ];
        let movement = Vec3::new(0.3, -0.1, 0.0);

        let motion = collide(
            entity,
            movement,
            &obstacles,
            &[],
            &[],
            false,
            0.6,
            CANDIDATE_HEIGHTS,
        );

        assert_eq!(motion, Vec3::new(0.3, 0.5 + -(0.05 - (0.05 + -0.05)), 0.0));
    }

    #[test]
    fn step_heights_come_from_the_voxel_grid_of_a_block_box() {
        let slab = Aabb::new(Vec3::new(3.0, 64.0, 5.0), Vec3::new(4.0, 64.5, 6.0));
        let fence = Aabb::new(Vec3::new(3.375, 64.0, 5.375), Vec3::new(3.625, 65.5, 5.625));
        let carpet = Aabb::new(Vec3::new(3.0, 64.0, 5.0), Vec3::new(4.0, 64.0625, 6.0));

        assert_eq!(y_coordinates(slab, GridTest::Round), vec![64.0, 64.5, 65.0]);
        assert_eq!(y_coordinates(fence, GridTest::Round), vec![64.0, 65.5]);
        assert_eq!(y_coordinates(carpet, GridTest::Round), vec![64.0, 64.0625]);
    }

    #[test]
    fn an_entity_collider_is_a_shape_in_world_coordinates() {
        // 1.21 Shapes.java:47-52, 75-89: cuboid(entity box) is a grid only for a box on a
        // 1/2^n grid of the cube at the origin; elsewhere its coordinates are its bounds.
        let near_origin = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.5, 1.0));
        let elsewhere = Aabb::new(Vec3::new(3.0, 64.0, 5.0), Vec3::new(4.0, 64.5, 6.0));

        assert_eq!(entity_y_coordinates(near_origin, GridTest::Round), vec![0.0, 0.5, 1.0]);
        assert_eq!(entity_y_coordinates(elsewhere, GridTest::Round), vec![64.0, 64.5]);
        assert_eq!(y_coordinates(elsewhere, GridTest::Round), vec![64.0, 64.5, 65.0]);
    }

    #[test]
    fn step_heights_of_an_entity_collider_are_its_world_shape_coordinates() {
        // 1.21 Entity.java:911, 924-926, 942-964: collectStepHeights takes getPointPositions(Y) of
        // each entity collider as Shapes.box made it, not as a block shape of the cell of
        // its minimum corner.
        let grounded = box_at(0.0, 0.0, 0.0);
        let collider = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.5, 0.25, 1.0));

        assert_eq!(
            candidate_step_heights(grounded, &[collider], &[], 0.6, -1.0, GridTest::Round),
            vec![0.0, 0.25]
        );
        assert_eq!(
            candidate_step_heights(grounded, &[], &[y_coordinates(collider, GridTest::Round)], 0.6, -1.0, GridTest::Round),
            vec![0.0, 0.25, 0.5]
        );
    }

    #[test]
    fn voxel_collision_of_1_13_leaves_out_only_blocks_the_box_reaches_into() {
        // 1.13.2 IWorldReaderBase.java:112-156 skips a block position whose shape the box meets,
        // for blocks only; Level.java:1903-1905 appends the entity shapes after them.
        let carpet = Aabb::new(Vec3::new(0.0, 64.0, 0.0), Vec3::new(1.0, 64.0625, 1.0));
        let boat = Aabb::new(
            Vec3::new(0.2, 64.0625, 0.2),
            Vec3::new(1.575, 64.625, 1.575),
        );
        let player = Aabb::new(Vec3::new(0.5, 64.3, 0.5), Vec3::new(1.1, 66.1, 1.1));
        let fall = Vec3::new(0.0, -0.5, 0.0);

        let moved = collide_legacy(player, fall, &[carpet], &[boat], false, 0.6, true);

        assert_eq!(moved.movement.y, 64.0625 - 64.3);
    }

    #[test]
    fn a_block_meeting_the_query_brings_its_whole_shape() {
        let floor = Aabb::new(Vec3::new(3.0, 64.0, 5.0), Vec3::new(4.0, 64.125, 6.0));
        let wall = Aabb::new(Vec3::new(3.0, 64.0, 5.0), Vec3::new(3.125, 65.0, 6.0));
        let elsewhere = box_at(7.0, 64.0, 5.0);
        let query = Aabb::new(Vec3::new(2.5, 64.5, 5.0), Vec3::new(3.1, 65.5, 6.0));

        assert_eq!(
            block_shapes_in(&[floor, wall, elsewhere], query),
            vec![floor, wall]
        );
    }

    #[test]
    fn a_box_inside_a_shape_of_several_boxes_stands_on_a_line_of_its_grid() {
        let origin = Vec3::new(0.0, 100.0, 0.0);
        let plate = Aabb::new(Vec3::new(0.75, 100.0, 0.0), Vec3::new(1.0, 101.0, 1.0));
        let arm = Aabb::new(
            Vec3::new(-0.25, 100.375, 0.375),
            Vec3::new(0.75, 100.625, 0.625),
        );
        let piston_head = BlockVoxelShape::from_world(origin, vec![plate, arm]);
        let entity = Aabb::new(Vec3::new(0.6, 100.5, 0.2), Vec3::new(1.2, 102.3, 0.8));
        let fall = Vec3::new(0.0, -0.2, 0.0);

        let boxes = collide(entity, fall, &[plate, arm], &[], &[], false, 0.6, LIFTED);
        let shape = collide(
            entity,
            fall,
            &[plate, arm],
            &[],
            &[piston_head],
            false,
            0.6,
            LIFTED,
        );

        assert_eq!(boxes.y, -0.2);
        assert_eq!(shape.y, -0.125);
    }

    #[test]
    fn a_box_inside_a_box_on_a_quarter_grid_stops_at_the_line_below_it() {
        let block = Aabb::new(Vec3::new(3.0, 64.0, 5.0), Vec3::new(4.0, 64.75, 6.0));
        let shape = BlockVoxelShape::from_world(Vec3::new(3.0, 64.0, 5.0), vec![block]);
        let entity = Aabb::new(Vec3::new(3.2, 64.6, 5.2), Vec3::new(3.8, 66.4, 5.8));
        let fall = Vec3::new(0.0, -0.5, 0.0);

        let landed = collide(entity, fall, &[block], &[], &[shape], false, 0.6, LIFTED);

        assert_eq!(landed.y, 64.5 - 64.6);
    }

    #[test]
    fn legacy_collision_moves_on_y_then_x_then_z() {
        let entity = box_at(0.0, 0.0, 0.0);
        let corner = [box_at(1.0, 0.0, 1.0)];
        let movement = Vec3::new(0.5, 0.0, 0.8);

        let legacy = collide_legacy(entity, movement, &corner, &[], false, 0.6, false);
        let modern = collide(entity, movement, &corner, &[], &[], false, 0.6, LIFTED);

        assert_eq!(legacy.movement, Vec3::new(0.5, 0.0, 0.0));
        assert_eq!(legacy.bounds, entity.translated(Vec3::new(0.5, 0.0, 0.0)));
        assert_eq!(modern, Vec3::new(0.0, 0.0, 0.8));
    }

    #[test]
    fn legacy_boxes_collide_without_a_tolerance() {
        let entity = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.00000005, 1.0, 1.0));
        let ahead = [box_at(2.0, 0.0, 0.0)];
        let movement = Vec3::new(0.5, 0.0, 0.0);

        assert_eq!(
            collide_legacy(entity, movement, &ahead, &[], false, 0.6, false).movement,
            movement
        );
        assert_eq!(
            collide_legacy(entity, movement, &ahead, &[], false, 0.6, true)
                .movement
                .x,
            2.0 - 2.00000005
        );
    }

    #[test]
    fn legacy_step_up_lowers_back_by_the_lift_only() {
        let entity = box_at(0.0, 0.0, 0.0);
        let step = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.125, 1.0));
        let ceiling = Aabb::new(Vec3::new(1.0, 1.2, 0.0), Vec3::new(2.0, 2.2, 1.0));
        let obstacles = [
            box_at(0.0, -1.0, 0.0),
            box_at(1.0, -1.0, 0.0),
            step,
            ceiling,
        ];
        let jump = Vec3::new(0.3, 0.42, 0.0);

        let moved = collide_legacy(entity, jump, &obstacles, &[], true, 0.6, false);

        let lift = 1.2 - 1.0;
        assert_eq!(moved.movement, Vec3::new(0.3, 0.125 - lift, 0.0));
        assert_eq!(moved.bounds.min, Vec3::new(0.3, 0.125, 0.0));
    }

    #[test]
    fn voxel_collision_of_1_13_leaves_out_a_block_the_box_reaches_into() {
        let entity = Aabb::new(Vec3::new(2.6, 0.0, 0.0), Vec3::new(3.6, 1.0, 0.5));
        let entered = Aabb::new(Vec3::new(3.0, 0.0, 0.0), Vec3::new(3.5, 1.0, 0.5));
        let ahead = Aabb::new(Vec3::new(3.5, 0.0, 0.75), Vec3::new(4.0, 1.0, 1.0));
        let movement = Vec3::new(0.0, 0.0, 0.5);

        let boxes = collide_legacy(entity, movement, &[entered, ahead], &[], false, 0.6, false);
        let voxels = collide_legacy(entity, movement, &[entered, ahead], &[], false, 0.6, true);

        assert_eq!(boxes.movement.z, 0.25);
        assert_eq!(voxels.movement.z, 0.5);
    }

    #[test]
    fn keeps_a_negligible_movement_with_no_obstacle_near_the_swept_box() {
        let entity = box_at(0.0, 0.0, 0.0);
        let movement = Vec3::new(0.5e-7, 0.0, 0.0);
        let far = [box_at(5.0, 0.0, 0.0)];
        let near = [box_at(1.0, 0.0, 0.0)];

        assert_eq!(
            collide(entity, movement, &far, &[], &[], false, 0.6, LIFTED),
            movement
        );
        assert_eq!(
            collide(entity, movement, &near, &[], &[], false, 0.6, LIFTED),
            Vec3::ZERO
        );
    }

    #[test]
    fn a_box_past_a_corner_of_the_border_stops_at_its_grid_line() {
        let outside = [
            Aabb::new(Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), Vec3::new(3.0, f64::INFINITY, f64::INFINITY)),
            Aabb::new(Vec3::new(14.0, f64::NEG_INFINITY, f64::NEG_INFINITY), Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY)),
            Aabb::new(Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), Vec3::new(f64::INFINITY, f64::INFINITY, 3.0)),
            Aabb::new(Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, 14.0), Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY)),
        ];
        let at = |x: f64, z: f64| Aabb::new(Vec3::new(x - 0.3, 100.0, z - 0.3), Vec3::new(x + 0.3, 101.8, z + 0.3));
        // outside both edges: the next cell of its row is outside the other edge, a full layer
        assert!((collide_border(&outside, at(2.6, 2.6), 0.15, Axis::X) - 0.1).abs() < 1.0e-9);
        assert!((collide_border(&outside, at(2.6, 2.6), 0.15, Axis::Z) - 0.1).abs() < 1.0e-9);
        // outside one edge only: the row's next cell is inside the border, and free
        assert_eq!(collide_border(&outside, at(2.6, 8.0), 0.15, Axis::X), 0.15);
        // not along its own axis, where the whole grid has one layer
        assert_eq!(collide_border(&outside, at(2.6, 2.6), -0.08, Axis::Y), -0.08);
        // inside the border its faces clip as boxes
        assert!((collide_border(&outside, at(13.6, 8.0), 0.15, Axis::X) - 0.1).abs() < 1.0e-9);
    }

    #[test]
    fn each_attempt_asks_whether_the_border_is_close_to_its_own_box_from_1_18() {
        // isInsideCloseToBorder(entity, box) of each collideBoundingBox's box (1.18 Entity.java:800-801,
        // 1.20.6 :913): the margin is the box's size, so a wide attempt can take the border a narrow one does not
        let border = [Aabb::new(Vec3::new(10.0, -100.0, -100.0), Vec3::new(20.0, 100.0, 100.0))];
        let near = |query: Aabb| query.max.x - query.min.x > 1.0;
        let colliders = Colliders { border: Some(&border), border_near: Some(&near), ..Colliders::default() };
        let wide = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 1.0, 1.0));
        let narrow = Aabb::new(Vec3::ZERO, Vec3::new(0.6, 1.0, 0.6));
        assert!(colliders.border_for(wide).border.is_some());
        assert!(colliders.border_for(narrow).border.is_none());
        let fixed = Colliders { border: Some(&border), ..Colliders::default() };
        assert!(fixed.border_for(narrow).border.is_some());
    }

    #[test]
    fn the_world_border_drops_a_negligible_movement_until_1_18() {
        let entity = box_at(0.0, 0.0, 0.0);
        let movement = Vec3::new(0.5e-7, 0.0, 0.0);
        let bordered = Colliders {
            entities: &[],
            border: Some(&[]),
            border_near: None,
            boxes: &[],
            block_border: None,
            blocks: &[],
            release: None,
        };

        assert_eq!(
            collide_shapes(entity, movement, bordered, false, 0.6, LIFTED),
            Vec3::ZERO
        );
        assert_eq!(
            collide(entity, movement, &[], &[], &[], false, 0.6, LIFTED),
            movement
        );
    }

    #[test]
    fn allows_motion_when_only_touching_other_axes() {
        let entity = box_at(0.0, 1.0, 0.0);
        let obstacle = box_at(2.0, 0.0, 0.0);

        assert_eq!(clip_axis(entity, obstacle, 2.5, Axis::X), 2.5);
    }

    #[test]
    fn pushes_back_an_entity_reaching_less_than_the_epsilon_into_a_box() {
        let entity = Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.00000005, 1.0, 1.0));
        let obstacle = box_at(2.0, 0.0, 0.0);

        assert_eq!(clip_axis(entity, obstacle, 0.5, Axis::X), 2.0 - 2.00000005);
        let reaching_back = Aabb::new(Vec3::new(-0.00000005, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        let behind = box_at(-1.0, 0.0, 0.0);
        assert_eq!(clip_axis(reaching_back, behind, -0.5, Axis::X), 0.00000005);
    }

    #[test]
    fn ignores_a_box_overlapped_by_less_than_the_epsilon_on_another_axis() {
        let entity = Aabb::new(
            Vec3::new(0.0, 0.00000005, 0.0),
            Vec3::new(1.0, 1.00000005, 1.0),
        );

        assert_eq!(clip_axis(entity, box_at(2.0, 1.0, 0.0), 2.5, Axis::X), 2.5);
        assert_eq!(clip_axis(entity, box_at(2.0, 0.0, 0.0), 2.5, Axis::X), 1.0);
        assert_eq!(clip_axis(entity, box_at(2.0, -1.0, 0.0), 2.5, Axis::X), 2.5);
    }

    #[test]
    fn matches_vanilla_equality_tolerance() {
        assert!(vanilla_equal(1.0, 1.0 + 0.5e-5));
        assert!(!vanilla_equal(1.0, 1.0 + 2.0e-5));
        assert!(vanilla_equal(0.0, 9.9999997e-6));
        assert!(!vanilla_equal(0.0, 9.9999998e-6));
    }

    #[test]
    fn rejects_sub_epsilon_motion_like_vanilla_shapes_collide() {
        let entity = box_at(0.0, 0.0, 0.0);
        let obstacle = box_at(2.0, 0.0, 0.0);

        assert_eq!(clip_axis(entity, obstacle, 0.5e-7, Axis::X), 0.0);
        assert_eq!(clip_axis(entity, obstacle, -0.5e-7, Axis::X), 0.0);
    }

    const WALK: CollisionRules = CollisionRules {
        single_axis_walk: true,
        ..LIFTED
    };

    /// A bottom stair at the block (1, 0, 0) facing west: its half slab and its upper step.
    fn stair_at(x: f64) -> BlockVoxelShape {
        BlockVoxelShape::new(
            Vec3::new(x, 0.0, 0.0),
            vec![
                Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                Aabb::new(Vec3::new(0.5, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            ],
        )
    }

    fn blocks_only(blocks: &[BlockVoxelShape]) -> Colliders<'_> {
        Colliders {
            blocks,
            ..Colliders::default()
        }
    }

    #[test]
    fn the_border_offered_again_by_the_block_stream_is_another_check() {
        // 1.14 - 1.17.1 getBlockCollisions with the entity offers the border's shape first when
        // the entity's box touches it from within (1.15 CollisionGetter.java:80-88, 1.16.5
        // CollisionSpliterator.java:56-58, 100-112), after the border Entity.collide put
        // among the colliders (1.16.5 Entity.java:787-795, 831): the border clips a box 5e-8
        // inside it to 5e-8, which the second border's check drops
        let entity = Aabb::new(Vec3::new(9.0, 0.0, 0.0), Vec3::new(10.0 - 5.0e-8, 1.0, 1.0));
        let outside = [Aabb::new(
            Vec3::new(10.0, f64::NEG_INFINITY, f64::NEG_INFINITY),
            Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
        )];
        let push = Vec3::new(0.5, 0.0, 0.1);
        let once = Colliders {
            border: Some(&outside),
            ..Colliders::default()
        };
        assert_eq!(collide_shapes(entity, push, once, false, 0.6, WALK).x, 10.0 - (10.0 - 5.0e-8));
        let twice = Colliders {
            block_border: Some(&outside),
            ..once
        };
        assert_eq!(collide_shapes(entity, push, twice, false, 0.6, WALK).x, 0.0);
        // a single-axis move walks the cells itself (Shapes.collide), with no
        // block stream
        let along = Vec3::new(0.5, 0.0, 0.0);
        assert_eq!(collide_shapes(entity, along, twice, false, 0.6, WALK).x, 10.0 - (10.0 - 5.0e-8));
    }

    #[test]
    fn the_sub_epsilon_check_falls_between_whole_shapes() {
        // Shapes.collide drops a movement under 1.0E-7 before each VoxelShape of the list
        // (client1.21.11 Shapes.java:175-185), and a shape resolves in one walk of its grid
        // (VoxelShape.java:207-212, no check between its boxes): pressing 5e-8 short of a lone
        // stair keeps the 5e-8 its first box leaves, though the stair has a second box.
        let entity = Aabb::new(Vec3::new(0.4, 0.0, 0.0), Vec3::new(1.0 - 5.0e-8, 1.0, 1.0));
        let push = Vec3::new(0.7, 0.0, 0.0);
        let stair = [stair_at(1.0)];
        let gap = 1.0 - (1.0 - 5.0e-8);

        let moved = collide_shapes(entity, push, blocks_only(&stair), false, 0.6, LIFTED);
        assert_eq!(moved.x, gap);
        // the same boxes as a flat list are two shapes: the second drops the 5e-8
        let flat = stair[0].boxes.clone();
        assert_eq!(collide(entity, push, &flat, &[], &[], false, 0.6, LIFTED).x, 0.0);
        // another block's shape after the stair drops it too
        let mut both = vec![stair_at(1.0), BlockVoxelShape::new(
            Vec3::new(1.0, 0.0, 1.0),
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))],
        )];
        sort_in_cursor_order(&mut both);
        let entity_across = Aabb::new(
            Vec3::new(0.4, 0.0, 0.5),
            Vec3::new(1.0 - 5.0e-8, 1.0, 1.5),
        );
        assert_eq!(
            collide_shapes(entity_across, push, blocks_only(&both), false, 0.6, LIFTED).x,
            0.0
        );
    }

    #[test]
    fn colliders_go_entities_border_then_blocks_in_cursor_order() {
        // Entity.collectColliders: entity colliders, the border, then BlockCollisions in Cursor3D
        // order, x fastest, then y, then z (client1.21.11 Entity.java:1146-1160, Cursor3D.java:
        // 30-40). Only a residual the last shape leaves survives: with the stair last in that
        // order the 5e-8 stays; with a block of a later row after it, it is dropped.
        let entity = Aabb::new(Vec3::new(0.4, 0.0, 0.0), Vec3::new(1.0 - 5.0e-8, 1.0, 1.0));
        let push = Vec3::new(0.5, 0.0, 0.0);
        let gap = 1.0 - (1.0 - 5.0e-8);
        let floor_below = BlockVoxelShape::new(
            Vec3::new(0.0, -1.0, 0.0),
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))],
        );
        let floor_above = BlockVoxelShape::new(
            Vec3::new(0.0, 1.0, 0.0),
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.0000001, 1.0))],
        );
        let mut earlier = vec![stair_at(1.0), floor_below.clone()];
        sort_in_cursor_order(&mut earlier);
        assert_eq!(earlier[0], floor_below);
        let tall = Aabb::new(entity.min - Vec3::new(0.0, 1.0e-6, 0.0), entity.max);
        let entity_low = tall;
        assert_eq!(
            collide_shapes(entity_low, push, blocks_only(&earlier), false, 0.6, LIFTED).x,
            gap
        );
        let mut later = vec![stair_at(1.0), floor_above];
        sort_in_cursor_order(&mut later);
        assert_eq!(later[0], stair_at(1.0));
        let entity_high = Aabb::new(entity.min, entity.max + Vec3::new(0.0, 1.0e-6, 0.0));
        assert_eq!(
            collide_shapes(entity_high, push, blocks_only(&later), false, 0.6, LIFTED).x,
            0.0
        );
        // the border's shape comes before the blocks, so the stair stays last
        let bordered = Colliders {
            border: Some(&[]),
            ..blocks_only(&later[..1])
        };
        assert_eq!(collide_shapes(entity, push, bordered, false, 0.6, LIFTED).x, gap);
        // an entity collider comes first whether or not it meets the attempt
        let far_boat = [Aabb::new(Vec3::new(5.0, 0.0, 5.0), Vec3::new(6.0, 1.0, 6.0))];
        let with_entity = Colliders {
            entities: &far_boat,
            ..blocks_only(&later[..1])
        };
        assert_eq!(collide_shapes(entity, push, with_entity, false, 0.6, LIFTED).x, gap);
        // alone, a far entity collider is still a shape: a negligible move meets its check
        let negligible = Vec3::new(5.0e-8, 0.0, 0.0);
        let only_entity = Colliders {
            entities: &far_boat,
            ..Colliders::default()
        };
        assert_eq!(
            collide_shapes(entity, negligible, only_entity, false, 0.6, LIFTED),
            Vec3::ZERO
        );
        assert_eq!(
            collide_shapes(entity, negligible, Colliders::default(), false, 0.6, LIFTED),
            negligible
        );
    }

    #[test]
    fn a_one_axis_move_of_1_14_to_1_17_walks_the_blocks_and_drops_their_residuals() {
        // Entity.collideBoundingBoxHeuristically: a movement with two zero components goes
        // through Shapes.collide(AABB, LevelReader, ...) (client1.14.4 Entity.java:725-735,
        // 1.16.5 Entity.java:824-836), which returns 0 as soon as a block leaves the movement
        // under 1.0E-7 (1.16.5 Shapes.java:281-284) where the shape list keeps the residual
        // of its last shape.
        let floor = [BlockVoxelShape::new(
            Vec3::new(0.0, 0.0, 0.0),
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))],
        )];
        let entity = Aabb::new(Vec3::new(0.2, 1.0 + 5.0e-8, 0.2), Vec3::new(0.8, 2.8, 0.8));
        let fall = Vec3::new(0.0, -0.5, 0.0);
        let residual = 1.0 - (1.0 + 5.0e-8);
        assert_eq!(
            collide_shapes(entity, fall, blocks_only(&floor), false, 0.6, LIFTED).y,
            residual
        );
        assert_eq!(
            collide_shapes(entity, fall, blocks_only(&floor), false, 0.6, WALK).y,
            0.0
        );
        // a move on two axes takes the shape list in either release
        let slide = Vec3::new(0.1, -0.5, 0.0);
        assert_eq!(
            collide_shapes(entity, slide, blocks_only(&floor), false, 0.6, WALK).y,
            residual
        );
        // the walk drops a negligible movement with nothing around (1.16.5 Shapes.java:241-243)
        let negligible = Vec3::new(0.0, 5.0e-8, 0.0);
        assert_eq!(
            collide_shapes(entity, negligible, Colliders::default(), false, 0.6, WALK),
            Vec3::ZERO
        );
        assert_eq!(
            collide_shapes(entity, negligible, Colliders::default(), false, 0.6, LIFTED),
            negligible
        );
        // and passes any movement of a box under 1.0E-6 thick on an axis (1.16.5 Shapes.java:237-239)
        let flat = Aabb::new(Vec3::new(0.2, 1.1, 0.2), Vec3::new(0.8, 1.1 + 5.0e-7, 0.8));
        assert_eq!(
            collide_shapes(flat, fall, blocks_only(&floor), false, 0.6, WALK),
            fall
        );
        // the entity colliders and the border collide after the blocks, each with its own check
        let boat = [Aabb::new(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0 + 4.0e-8, 1.0))];
        let above = Aabb::new(Vec3::new(0.2, 1.0 + 5.0e-8, 0.2), Vec3::new(0.8, 2.8, 0.8));
        let with_boat = Colliders {
            entities: &boat,
            ..Colliders::default()
        };
        assert_eq!(
            collide_shapes(above, fall, with_boat, false, 0.6, WALK).y,
            (1.0 + 4.0e-8) - (1.0 + 5.0e-8)
        );
    }

    #[test]
    fn the_one_axis_walk_takes_the_gap_in_block_coordinates() {
        // The walk collides each block's shape with the box moved by minus the block's position
        // (`box.offset(-x, -y, -z)`, 1.16.5 Shapes.java:281-281): 0.25 - (0.078 - 1.0) is
        // one ulp above (1.0 + 0.25) - 0.078.
        let wall = [BlockVoxelShape::new(
            Vec3::new(1.0, 0.0, 0.0),
            vec![Aabb::new(Vec3::new(0.25, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))],
        )];
        let entity = Aabb::new(Vec3::new(-0.522, 0.0, 0.0), Vec3::new(0.078, 1.0, 1.0));
        let push = Vec3::new(2.0, 0.0, 0.0);
        let world_gap = (1.0 + 0.25) - 0.078;
        let local_gap = 0.25 - (0.078 - 1.0);
        assert_ne!(world_gap, local_gap);
        assert_eq!(
            collide_shapes(entity, push, blocks_only(&wall), false, 0.6, LIFTED).x,
            world_gap
        );
        assert_eq!(
            collide_shapes(entity, push, blocks_only(&wall), false, 0.6, WALK).x,
            local_gap
        );
        // a fence in the layer beyond the walk's reach counts (hasLargeCollisionShape)
        let fence = [BlockVoxelShape::new(
            Vec3::new(1.0, -1.0, 0.0),
            vec![Aabb::new(Vec3::new(0.375, 0.0, 0.375), Vec3::new(0.625, 1.5, 0.625))],
        )];
        let above_fence = Aabb::new(Vec3::new(1.2, 0.6, 0.2), Vec3::new(1.8, 2.4, 0.8));
        let fall = Vec3::new(0.0, -0.5, 0.0);
        assert_eq!(
            collide_shapes(above_fence, fall, blocks_only(&fence), false, 0.6, WALK).y,
            1.5 - (0.6 + 1.0)
        );
    }

    #[test]
    fn a_partial_shape_within_the_epsilon_is_not_offered_to_the_move() {
        // BlockCollisions tests a shape but Shapes.block() by joinIsNotEmpty(AND), whose merger drops
        // an overlap within 1e-7 (26.3 BlockCollisions.java:91-103, IndirectMerger.java): a box
        // reaching 5e-8 into a partial block keeps its 1e-8 move
        let shape = |local: Aabb| vec![BlockVoxelShape::new(Vec3::new(0.0, 0.0, 0.0), vec![local])];
        let partial = shape(Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)));
        let bounds = Aabb::new(Vec3::new(-0.1, 0.1, 0.1), Vec3::new(0.5 + 5.0e-8, 0.9, 0.9));
        let movement = Vec3::new(1.0e-8, -0.0784, 0.0);
        let colliders = |blocks, release| Colliders {
            blocks,
            release,
            ..Colliders::default()
        };
        let modern = Some(crate::version::MinecraftVersion::new(1, 21, 11));
        assert_eq!(
            collide_shapes(bounds, movement, colliders(&partial, modern), false, 0.6, WALK),
            movement
        );
        // the same boxes by strict overlap collide
        assert_eq!(
            collide_shapes(bounds, movement, colliders(&partial, None), false, 0.6, WALK).x,
            0.0
        );
        // past the epsilon the shape is offered
        let deeper = Aabb::new(Vec3::new(-0.1, 0.1, 0.1), Vec3::new(0.5 + 2.0e-7, 0.9, 0.9));
        assert_eq!(
            collide_shapes(deeper, movement, colliders(&partial, modern), false, 0.6, WALK).x,
            0.0
        );
        // the whole cell keeps the strict test: Shapes.block() is not merged
        let cube = shape(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
        assert!(cube[0].shapes_block);
        let inside = Aabb::new(Vec3::new(-1.0, 0.1, 0.1), Vec3::new(5.0e-8, 0.9, 0.9));
        assert_eq!(
            collide_shapes(inside, movement, colliders(&cube, modern), false, 0.6, WALK).x,
            0.0
        );
    }

    #[test]
    fn the_merge_boundary_of_exactly_1e_7_keeps_a_shape_from_the_move() {
        // A query 1e-7 past the slab's face merges into it (26.3 IndirectMerger.java, 1.14.4
        // IndirectMerger.java:46): no shape is listed and collideWithShapes passes the move
        // on, a sub-1e-7 axis too (26.3 Entity.collideWithShapes); one more ulp lists the shape and
        // Shapes.collide drops that axis (26.3 Shapes.java:176-186). 1.14.4 - 1.17.1 take this path
        // for a move along two axes (1.16.5 Entity.java:834-865), the list path from 1.18
        let slab = [BlockVoxelShape::new(
            Vec3::ZERO,
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))],
        )];
        let exact: f64 = 1.0e-7;
        let above = f64::from_bits(exact.to_bits() + 1);
        let bounds = |max: f64| Aabb::new(Vec3::new(-0.6, 0.1, 0.1), Vec3::new(max, 0.4, 0.9));
        let movement = Vec3::new(0.0, -0.0784, 1.0e-8);
        let version = crate::version::MinecraftVersion::new;
        for (release, rules) in [
            (version(1, 14, 4), WALK),
            (version(1, 16, 5), WALK),
            (version(1, 17, 1), WALK),
            (version(1, 18, 2), LIFTED),
            (version(1, 21, 11), CANDIDATE_HEIGHTS),
            (version(26, 3, 0), CANDIDATE_HEIGHTS),
        ] {
            let moved = |max| {
                let colliders = Colliders {
                    blocks: &slab,
                    release: Some(release),
                    ..Colliders::default()
                };
                collide_shapes(bounds(max), movement, colliders, false, 0.6, rules)
            };
            assert_eq!(moved(5.0e-8), movement, "{release:?}");
            assert_eq!(moved(exact), movement, "{release:?}");
            assert_eq!(moved(above), Vec3::new(0.0, movement.y, 0.0), "{release:?}");
        }
    }

    #[test]
    fn an_axis_that_does_not_move_comes_back_positive_zero_from_1_21_5() {
        // 1.21.5 collideWithShapes starts from Vec3.ZERO and sets the axes that moved (Entity.java:1059-1069);
        // 1.21.4 builds the result from the movement's own -0.0 (Entity.java:1011-1035)
        let slab = [BlockVoxelShape::new(Vec3::ZERO, vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))])];
        let bounds = Aabb::new(Vec3::new(0.2, 0.6, 0.2), Vec3::new(0.8, 2.4, 0.8));
        let movement = Vec3::new(-0.0, -0.3, -0.0);
        let version = crate::version::MinecraftVersion::new;
        let moved = |release| {
            let colliders = Colliders { blocks: &slab, release: Some(release), ..Colliders::default() };
            collide_shapes(bounds, movement, colliders, false, 0.6, CANDIDATE_HEIGHTS)
        };
        let older = moved(version(1, 21, 4));
        let newer = moved(version(1, 21, 5));
        assert_eq!(older.y, newer.y);
        assert!(older.x.is_sign_negative() && older.z.is_sign_negative());
        assert!(newer.x.is_sign_positive() && newer.z.is_sign_positive());
    }

    #[test]
    fn the_single_axis_walk_never_lists_a_shape_and_drops_a_tiny_move_itself() {
        // Shapes.collide drops a movement under 1e-7 before it walks a cell
        // (1.16.5 Shapes.java:241-242), whatever the overlap; from 1.18 there is no such walk
        let slab = [BlockVoxelShape::new(
            Vec3::ZERO,
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))],
        )];
        let filtered = Aabb::new(Vec3::new(-0.6, 0.1, 0.1), Vec3::new(1.0e-7, 0.4, 0.9));
        let tiny = Vec3::new(0.0, 0.0, 1.0e-8);
        let moved = |release, rules| {
            let colliders = Colliders {
                blocks: &slab,
                release: Some(release),
                ..Colliders::default()
            };
            collide_shapes(filtered, tiny, colliders, false, 0.6, rules)
        };
        let version = crate::version::MinecraftVersion::new;
        assert_eq!(moved(version(1, 16, 5), WALK), Vec3::ZERO);
        assert_eq!(moved(version(1, 18, 2), LIFTED), tiny);
    }

    #[test]
    fn the_identity_of_the_full_block_and_an_empty_shape_keep_their_own_answers() {
        // Shapes.block() meets by AABB.intersects from 1.16 and merges like any shape in 1.14 - 1.15
        // (1.16.5 CollisionSpliterator.java:74-75); an empty shape meets nothing (26.3
        // BlockCollisions.java:91-103)
        let cube = [BlockVoxelShape::new(
            Vec3::ZERO,
            vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))],
        )];
        let empty = [BlockVoxelShape::new(Vec3::ZERO, Vec::new())];
        let bounds = Aabb::new(Vec3::new(-0.6, 0.1, 0.1), Vec3::new(5.0e-8, 0.4, 0.9));
        let movement = Vec3::new(0.0, -0.0784, 1.0e-8);
        let moved = |blocks: &[BlockVoxelShape], release, rules| {
            let colliders = Colliders {
                blocks,
                release: Some(release),
                ..Colliders::default()
            };
            collide_shapes(bounds, movement, colliders, false, 0.6, rules)
        };
        let version = crate::version::MinecraftVersion::new;
        assert_eq!(moved(&cube, version(1, 15, 2), WALK), movement);
        assert_eq!(moved(&cube, version(1, 16, 5), WALK).z, 0.0);
        assert_eq!(moved(&cube, version(26, 3, 0), CANDIDATE_HEIGHTS).z, 0.0);
        assert_eq!(moved(&empty, version(26, 3, 0), CANDIDATE_HEIGHTS), movement);
    }

    #[test]
    fn grid_lines_merge_as_the_indirect_merger_at_its_tolerance() {
        // `d >= e - 1.0E-7`: inclusive, in that form (1.21.11 IndirectMerger.java:53). The sum
        // here rounds so that e - d is above 1.0E-7 while d >= e - 1.0E-7 still holds.
        let boxes = |top: f64| {
            vec![(Aabb::new(Vec3::ZERO, Vec3::new(1.0, top, 1.0)), Vec3::ZERO)]
        };
        let two = |low: f64, high: f64| {
            let (mut a, b) = (boxes(low), boxes(high));
            a.extend(b);
            super::grid_lines(a.into_iter(), Axis::Y, GridTest::Round)
        };
        let (d, e) = (0.3, 0.3 + 1.0e-7);
        assert!(d >= e - 1.0e-7);
        assert_eq!(two(d, e), vec![0.0, d, 1.0].into_iter().filter(|v| *v != 1.0).collect::<Vec<_>>());
        assert_eq!(two(d, 0.3 + 2.0e-7).len(), 3);
    }

    /// A case of the fixture of scripts/java-oracle/Oracle113.java: the box, the move, the boxes
    /// in the world, the bounds of the shapes the jar's query gave and what its collide left.
    struct LegacyCase {
        bounds: Aabb,
        movement: Vec3,
        obstacles: Vec<Aabb>,
        shapes: Vec<Aabb>,
        moved: Vec3,
    }

    fn legacy_cases() -> Vec<LegacyCase> {
        let text = include_str!("../fixtures/java-oracle-collisions-1.13.2.txt");
        let boxes = |tokens: &[&str]| -> Vec<Aabb> {
            let values: Vec<f64> = tokens.iter().map(|token| token.parse().unwrap()).collect();
            values
                .chunks(6)
                .map(|at| Aabb::new(Vec3::new(at[0], at[1], at[2]), Vec3::new(at[3], at[4], at[5])))
                .collect()
        };
        let mut palette: Vec<Vec<Aabb>> = Vec::new();
        let mut cases = Vec::new();
        for line in text.lines() {
            let tokens: Vec<&str> = line.split(' ').collect();
            if tokens[0] == "palette" {
                palette.push(boxes(&tokens[3..]));
                continue;
            }
            let number = |index: usize| tokens[index].parse::<f64>().unwrap();
            let bounds = boxes(&tokens[1..7])[0];
            let movement = Vec3::new(number(7), number(8), number(9));
            let cells: usize = tokens[11].parse().unwrap();
            let mut obstacles = Vec::new();
            for cell in 0..cells {
                let at = 12 + cell * 4;
                let origin = Vec3::new(number(at), number(at + 1), number(at + 2));
                let id: usize = tokens[at + 3].parse().unwrap();
                obstacles.extend(palette[id].iter().map(|local| local.translated(origin)));
            }
            let shapes_at = 12 + cells * 4;
            assert_eq!(tokens[shapes_at], "shapes");
            let count: usize = tokens[shapes_at + 1].parse().unwrap();
            let shapes = boxes(&tokens[shapes_at + 2..shapes_at + 2 + count * 6]);
            let moved_at = shapes_at + 2 + count * 6;
            assert_eq!(tokens[moved_at], "moved");
            let moved = Vec3::new(number(moved_at + 1), number(moved_at + 2), number(moved_at + 3));
            cases.push(LegacyCase { bounds, movement, obstacles, shapes, moved });
        }
        cases
    }

    fn bounds_of(boxes: &[Aabb]) -> Aabb {
        boxes.iter().skip(1).fold(boxes[0], |all, bounds| {
            Aabb::new(
                Vec3::new(all.min.x.min(bounds.min.x), all.min.y.min(bounds.min.y), all.min.z.min(bounds.min.z)),
                Vec3::new(all.max.x.max(bounds.max.x), all.max.y.max(bounds.max.y), all.max.z.max(bounds.max.z)),
            )
        })
    }

    #[test]
    fn the_query_and_the_collision_of_1_13_2_are_the_jars() {
        let cases = legacy_cases();
        assert!(cases.len() >= 3000);
        let (mut query_wrong, mut moved_wrong) = (0, 0);
        let mut first = None;
        for case in &cases {
            let colliders = super::legacy_colliders(&case.obstacles, &[], case.bounds, case.movement, true);
            let mut listed: Vec<Aabb> = colliders.blocks.iter().map(|shape| bounds_of(&shape.boxes)).collect();
            if !colliders.full_cubes.is_empty() {
                listed.push(bounds_of(&colliders.full_cubes));
            }
            if listed != case.shapes {
                query_wrong += 1;
                first = first.or(Some((case.bounds, case.movement, listed.clone(), case.shapes.clone())));
            }
            let moved = collide_legacy(case.bounds, case.movement, &case.obstacles, &[], false, 0.0, true);
            let same = |a: f64, b: f64| a.to_bits() == b.to_bits() || (a == 0.0 && b == 0.0);
            if !(same(moved.movement.x, case.moved.x) && same(moved.movement.y, case.moved.y) && same(moved.movement.z, case.moved.z)) {
                moved_wrong += 1;
                first = first.or(Some((case.bounds, case.movement, vec![moved.bounds], case.shapes.clone())));
            }
        }
        assert_eq!((query_wrong, moved_wrong), (0, 0), "first: {first:?}");
    }

    #[test]
    fn the_grid_of_a_box_is_the_one_each_jar_gives_it() {
        // Shapes.create over boxes on and just off a 1/2^n grid: the test turned at 1.17, and
        // only a coordinate just under a line tells them apart (scripts/java-oracle/OracleGrid.java)
        let text = include_str!("../fixtures/java-oracle-grid.txt");
        let mut told_apart = 0;
        for line in text.lines() {
            let (left, right) = line.split_once(" = ").unwrap();
            let tokens: Vec<&str> = left.split(' ').collect();
            let release = crate::version::MinecraftVersion::parse(tokens[1]).unwrap();
            let values: Vec<f64> = tokens[2..].iter().map(|token| token.parse().unwrap()).collect();
            let bounds = Aabb::new(
                Vec3::new(values[0], values[1], values[2]),
                Vec3::new(values[3], values[4], values[5]),
            );
            let mut lists = right.split(' ');
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                let count: usize = lists.next().unwrap().parse().unwrap();
                let expected: Vec<f64> = lists.by_ref().take(count).map(|token| token.parse().unwrap()).collect();
                let test = GridTest::of(Some(release));
                let grid = |test| super::grid_lines(std::iter::once((bounds, Vec3::ZERO)), axis, test);
                assert_eq!(grid(test), expected, "{line}");
                if grid(GridTest::Floor) != grid(GridTest::Round) {
                    told_apart += 1;
                }
            }
        }
        assert!(told_apart > 100, "{told_apart}");
    }

    /// Touching planes, gaps either side of 1.0E-7, negative cells and diagonals whose axis order
    /// decides the end, against each jar (scripts/java-oracle/OracleEdgeCollide.java).
    #[test]
    fn the_edges_of_a_move_are_each_jars() {
        use super::{aabb_offset, shapes_collide, Collider};

        let same = |a: f64, b: f64| a.to_bits() == b.to_bits() || (a == 0.0 && b == 0.0);
        let box_at = |values: &[f64]| {
            Aabb::new(
                Vec3::new(values[0], values[1], values[2]),
                Vec3::new(values[3], values[4], values[5]),
            )
        };
        let axis_of = |name: &str| match name {
            "x" => Axis::X,
            "y" => Axis::Y,
            _ => Axis::Z,
        };
        let cells = |numbers: &[f64]| -> Vec<BlockVoxelShape> {
            numbers
                .chunks(9)
                .map(|cell| {
                    BlockVoxelShape::new(Vec3::new(cell[0], cell[1], cell[2]), vec![box_at(&cell[3..9])])
                })
                .collect()
        };
        let text = include_str!("../fixtures/java-oracle-edge-collide.txt");
        let (mut compared, mut wrong) = (0, Vec::new());
        for line in text.lines() {
            let (left, right) = line.split_once(" = ").unwrap();
            let tokens: Vec<&str> = left.split(' ').collect();
            let numbers = |from: usize| -> Vec<f64> {
                tokens[from..].iter().map(|token| token.parse().unwrap()).collect()
            };
            let expected: Vec<f64> = right.split(' ').map(|token| token.parse().unwrap()).collect();
            let matched = match tokens[0] {
                "edge" => {
                    let values = numbers(2);
                    let blocks = cells(&values[10..]);
                    let release = crate::version::MinecraftVersion::parse(tokens[1]);
                    let colliders = Colliders { blocks: &blocks, release, ..Colliders::default() };
                    let movement = Vec3::new(values[6], values[7], values[8]);
                    // (a shape the release's walk does not offer this sweep cannot be in its list)
                    let swept = box_at(&values).expand_towards(movement);
                    if !blocks.iter().all(|shape| shape.meets(swept, release)) {
                        continue;
                    }
                    let moved = collide_shapes(box_at(&values), movement, colliders, false, 0.0, LIFTED);
                    same(moved.x, expected[0]) && same(moved.y, expected[1]) && same(moved.z, expected[2])
                }
                "axis" => {
                    let values = numbers(3);
                    let blocks = cells(&values[8..]);
                    let listed: Vec<Collider<'_>> =
                        blocks.iter().map(|shape| Collider::Block(shape, GridTest::Floor)).collect();
                    same(shapes_collide(axis_of(tokens[2]), box_at(&values), &listed, values[6]), expected[0])
                }
                _ => {
                    let values = numbers(3);
                    same(aabb_offset(box_at(&values), box_at(&values[6..]), values[12], axis_of(tokens[2])), expected[0])
                }
            };
            compared += 1;
            if !matched && wrong.len() < 8 {
                wrong.push(line);
            }
        }
        assert!(compared > 5000, "{compared}");
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
