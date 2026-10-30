use crate::math::{Aabb, Vec3};

pub const VANILLA_EQUAL_EPSILON: f64 = 9.999999747378752e-6;
const COLLISION_EPSILON: f64 = 1.0e-7;

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

#[must_use]
pub fn resolve_motion(entity: Aabb, desired: Vec3, obstacles: &[Aabb]) -> (Vec3, [bool; 3]) {
    let mut resolved = Vec3::ZERO;
    let mut collided = [false; 3];
    let axes = if desired.x.abs() < desired.z.abs() {
        [Axis::Y, Axis::Z, Axis::X]
    } else {
        [Axis::Y, Axis::X, Axis::Z]
    };

    for axis in axes {
        let mut delta = component(desired, axis);
        for obstacle in obstacles {
            let clipped = clip_axis(entity.translated(resolved), *obstacle, delta, axis);
            if clipped != delta {
                collided[axis_index(axis)] = true;
                delta = clipped;
            }
        }
        resolved = set_component(resolved, axis, delta);
    }

    (resolved, collided)
}

/// Whether a box reaches more than 1.0E-7 into another on every axis.
#[must_use]
pub fn reaches_into(entity: Aabb, bounds: Aabb) -> bool {
    [Axis::X, Axis::Y, Axis::Z].into_iter().all(|axis| {
        component(entity.max, axis) - COLLISION_EPSILON > component(bounds.min, axis)
            && component(entity.min, axis) + COLLISION_EPSILON < component(bounds.max, axis)
    })
}

/// The collision shape of a block (its boxes, the block at `origin`) as a VoxelShape. A box inside
/// it collides with it over its grid, not box by box.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockVoxelShape {
    pub origin: Vec3,
    pub boxes: Vec<Aabb>,
}

impl BlockVoxelShape {
    fn is_entered_by(&self, entity: Aabb) -> bool {
        self.boxes
            .iter()
            .any(|bounds| reaches_into(entity, *bounds))
    }

    /// VoxelShape.getCoords: Shapes.create gives a box on a 1/2^n grid of its block on every axis
    /// that whole grid, any other box its bounds; Shapes.or joins them.
    fn grid(&self, axis: Axis) -> Vec<f64> {
        let origin = component(self.origin, axis);
        let mut lines: Vec<f64> = Vec::new();
        for bounds in &self.boxes {
            let local = Aabb::new(bounds.min - self.origin, bounds.max - self.origin);
            let bits = [Axis::X, Axis::Y, Axis::Z]
                .map(|axis| grid_bits(component(local.min, axis), component(local.max, axis)));
            if bits.iter().all(Option::is_some) {
                let parts = 1_u32 << bits[axis_index(axis)].unwrap_or(0);
                lines.extend((0..=parts).map(|line| f64::from(line) / f64::from(parts) + origin));
            } else {
                lines.extend([component(bounds.min, axis), component(bounds.max, axis)]);
            }
        }
        lines.sort_by(f64::total_cmp);
        lines.dedup_by(|line, kept| *line - *kept < COLLISION_EPSILON);
        lines
    }

    /// VoxelShape.collideX: the first layer of cells beyond the face the box leads with that holds
    /// a full cell across the box stops it at that layer. A box inside the shape so passes the
    /// layers it is in.
    fn collide(&self, axis: Axis, entity: Aabb, delta: f64) -> f64 {
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        let grids = [Axis::X, Axis::Y, Axis::Z].map(|axis| self.grid(axis));
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
        let center = |axis: Axis, cell: usize| (grid(axis)[cell] + grid(axis)[cell + 1]) / 2.0;
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
                    self.boxes.iter().any(|bounds| bounds.contains(point))
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
    pub border_always_collides: bool,
}

/// Entity.collide: the movement collided with the obstacles, stepping up when a grounded or
/// landing entity runs into something. `entered` holds the shapes of the blocks among the
/// obstacles the entity is inside.
#[must_use]
pub fn collide(
    bounds: Aabb,
    movement: Vec3,
    obstacles: &[Aabb],
    entered: &[BlockVoxelShape],
    on_ground: bool,
    max_step: f32,
    rules: CollisionRules,
) -> Vec3 {
    let border = rules.border_always_collides;
    let collided = if movement.length_squared() == 0.0 {
        movement
    } else {
        collide_bounding_box(bounds, movement, obstacles, entered, border)
    };
    let landing = movement.y != collided.y && movement.y < 0.0;
    let blocked = movement.x != collided.x || movement.z != collided.z;
    if max_step <= 0.0 || !(on_ground || landing) || !blocked {
        return collided;
    }
    match rules.step_up {
        StepUp::Lifted => {
            let colliders = |bounds: Aabb, movement: Vec3| {
                collide_bounding_box(bounds, movement, obstacles, entered, border)
            };
            lifted_step_up(bounds, movement, collided, colliders, f64::from(max_step))
        }
        StepUp::CandidateHeights => candidate_step_up(
            bounds, movement, collided, landing, obstacles, entered, max_step,
        ),
    }
}

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

fn candidate_step_up(
    bounds: Aabb,
    movement: Vec3,
    collided: Vec3,
    landing: bool,
    obstacles: &[Aabb],
    entered: &[BlockVoxelShape],
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
    let colliders = block_shapes_in(obstacles, query);
    for height in candidate_step_heights(grounded, &colliders, max_step, collided.y as f32) {
        let lifted = Vec3::new(movement.x, f64::from(height), movement.z);
        let stepped = collide_with_shapes(lifted, grounded, &colliders, entered, false);
        if stepped.horizontal_length_squared() > collided.horizontal_length_squared() {
            let distance_to_ground = bounds.min.y - grounded.min.y;
            return stepped + Vec3::new(0.0, -distance_to_ground, 0.0);
        }
    }
    collided
}

fn candidate_step_heights(
    grounded: Aabb,
    colliders: &[Aabb],
    max_step: f32,
    skipped: f32,
) -> Vec<f32> {
    let mut heights: Vec<f32> = Vec::new();
    for collider in colliders {
        for coordinate in y_coordinates(*collider) {
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
fn y_coordinates(collider: Aabb) -> Vec<f64> {
    let origin = block_origin(collider);
    let local = Aabb::new(collider.min - origin, collider.max - origin);
    let aligned = [
        grid_bits(local.min.x, local.max.x),
        grid_bits(local.min.y, local.max.y),
        grid_bits(local.min.z, local.max.z),
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

fn grid_bits(min: f64, max: f64) -> Option<u32> {
    if min < -1.0e-7 || max > 1.0000001 {
        return None;
    }
    (0..=3).find(|bits| {
        let parts = f64::from(1_u32 << bits);
        let aligned = |value: f64| {
            let scaled = value * parts;
            (scaled - scaled.round()).abs() < 1.0e-7 * parts
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

/// BlockCollisions: the whole shape of every block with a box meeting the query.
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

fn collide_bounding_box(
    bounds: Aabb,
    movement: Vec3,
    obstacles: &[Aabb],
    entered: &[BlockVoxelShape],
    border: bool,
) -> Vec3 {
    let colliders = colliders_in(obstacles, bounds.expand_towards(movement));
    collide_with_shapes(movement, bounds, &colliders, entered, border)
}

fn colliders_in(obstacles: &[Aabb], query: Aabb) -> Vec<Aabb> {
    obstacles
        .iter()
        .copied()
        .filter(|obstacle| obstacle.intersects(query))
        .collect()
}

fn collide_with_shapes(
    movement: Vec3,
    bounds: Aabb,
    colliders: &[Aabb],
    entered: &[BlockVoxelShape],
    border: bool,
) -> Vec3 {
    if colliders.is_empty() && !border {
        return movement;
    }
    let mut entity = bounds;
    let Vec3 {
        mut x,
        mut y,
        mut z,
    } = movement;
    if y != 0.0 {
        y = shapes_collide(Axis::Y, entity, colliders, entered, border, y);
        if y != 0.0 {
            entity = entity.translated(Vec3::new(0.0, y, 0.0));
        }
    }
    let z_first = x.abs() < z.abs();
    if z_first && z != 0.0 {
        z = shapes_collide(Axis::Z, entity, colliders, entered, border, z);
        if z != 0.0 {
            entity = entity.translated(Vec3::new(0.0, 0.0, z));
        }
    }
    if x != 0.0 {
        x = shapes_collide(Axis::X, entity, colliders, entered, border, x);
        if !z_first && x != 0.0 {
            entity = entity.translated(Vec3::new(x, 0.0, 0.0));
        }
    }
    if !z_first && z != 0.0 {
        z = shapes_collide(Axis::Z, entity, colliders, entered, border, z);
    }
    Vec3::new(x, y, z)
}

fn shapes_collide(
    axis: Axis,
    entity: Aabb,
    colliders: &[Aabb],
    entered: &[BlockVoxelShape],
    border: bool,
    mut delta: f64,
) -> f64 {
    if border && delta.abs() < COLLISION_EPSILON {
        return 0.0;
    }
    let inside: Vec<&BlockVoxelShape> = entered
        .iter()
        .filter(|shape| shape.is_entered_by(entity))
        .collect();
    for collider in colliders {
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        if !inside.iter().any(|shape| shape.boxes.contains(collider)) {
            delta = clip_axis(entity, *collider, delta, axis);
        }
    }
    for shape in inside {
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        delta = shape.collide(axis, entity, delta);
    }
    delta
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
    on_ground: bool,
    step_height: f32,
    voxel_shapes: bool,
) -> LegacyMove {
    let offset = |axis: Axis, entity: Aabb, colliders: &[Aabb], delta: f64| {
        if voxel_shapes {
            shapes_collide(axis, entity, colliders, &[], false, delta)
        } else {
            colliders.iter().fold(delta, |delta, collider| {
                aabb_offset(entity, *collider, delta, axis)
            })
        }
    };
    let colliders = legacy_colliders(obstacles, bounds, movement, voxel_shapes);
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
    let step_colliders = legacy_colliders(obstacles, bounds, Vec3::new(m, step, o), voxel_shapes);
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

/// World.getCollisionBoxes before 1.14. From 1.13 it leaves out the blocks the box already reaches
/// into and lists the full cubes, joined in one shape, after the other blocks.
fn legacy_colliders(
    obstacles: &[Aabb],
    bounds: Aabb,
    movement: Vec3,
    voxel_shapes: bool,
) -> Vec<Aabb> {
    if !voxel_shapes {
        return obstacles.to_vec();
    }
    let epsilon = Vec3::new(COLLISION_EPSILON, COLLISION_EPSILON, COLLISION_EPSILON);
    let swept = bounds.expand_towards(movement).inflate(epsilon);
    let inside = bounds.deflate(epsilon);
    let entered: Vec<Vec3> = obstacles
        .iter()
        .filter(|obstacle| obstacle.intersects(inside))
        .map(|obstacle| block_origin(*obstacle))
        .collect();
    let met: Vec<Vec3> = obstacles
        .iter()
        .filter(|obstacle| obstacle.intersects(swept))
        .map(|obstacle| block_origin(*obstacle))
        .collect();
    let (full_cubes, partial): (Vec<Aabb>, Vec<Aabb>) = obstacles
        .iter()
        .copied()
        .filter(|obstacle| {
            let block = block_origin(*obstacle);
            met.contains(&block) && !entered.contains(&block)
        })
        .partition(|obstacle| {
            let block = block_origin(*obstacle);
            obstacle.min == block && obstacle.max == block + Vec3::new(1.0, 1.0, 1.0)
        });
    partial.into_iter().chain(full_cubes).collect()
}

/// AxisAlignedBB.calculateX/Y/ZOffset: exact overlaps, no tolerance.
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

fn component(value: Vec3, axis: Axis) -> f64 {
    match axis {
        Axis::X => value.x,
        Axis::Y => value.y,
        Axis::Z => value.z,
    }
}

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
        block_shapes_in, clip_axis, collide, collide_legacy, resolve_motion, vanilla_equal,
        y_coordinates, Axis, BlockVoxelShape, CollisionRules, StepUp,
    };
    use crate::math::{Aabb, Vec3};

    const LIFTED: CollisionRules = CollisionRules {
        step_up: StepUp::Lifted,
        border_always_collides: false,
    };
    const CANDIDATE_HEIGHTS: CollisionRules = CollisionRules {
        step_up: StepUp::CandidateHeights,
        border_always_collides: false,
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

    #[test]
    fn resolves_multiple_axes_in_order() {
        let entity = box_at(0.0, 0.0, 0.0);
        let obstacle = box_at(2.0, 0.0, 2.0);
        let (motion, collided) = resolve_motion(entity, Vec3::new(2.5, 0.0, 2.5), &[obstacle]);

        assert_eq!(motion, Vec3::new(2.5, 0.0, 1.0));
        assert_eq!(collided, [false, false, true]);
    }

    #[test]
    fn follows_vanilla_axis_order_when_z_dominates() {
        let entity = box_at(0.0, 0.0, 0.0);
        let obstacle = box_at(0.0, 0.0, 2.0);
        let (motion, collided) = resolve_motion(entity, Vec3::new(1.0, 0.0, 2.5), &[obstacle]);

        assert_eq!(motion, Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(collided, [false, false, true]);
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

        let motion = collide(entity, movement, &obstacles, &[], false, 0.6, LIFTED);

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

        let motion = collide(entity, movement, &obstacles, &[], true, 0.6, LIFTED);

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

        let modern = collide(entity, jump, &obstacles, &[], true, 0.6, CANDIDATE_HEIGHTS);
        let legacy = collide(entity, jump, &obstacles, &[], true, 0.6, LIFTED);

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

        assert_eq!(y_coordinates(slab), vec![64.0, 64.5, 65.0]);
        assert_eq!(y_coordinates(fence), vec![64.0, 65.5]);
        assert_eq!(y_coordinates(carpet), vec![64.0, 64.0625]);
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
        let piston_head = BlockVoxelShape {
            origin,
            boxes: vec![plate, arm],
        };
        let entity = Aabb::new(Vec3::new(0.6, 100.5, 0.2), Vec3::new(1.2, 102.3, 0.8));
        let fall = Vec3::new(0.0, -0.2, 0.0);

        let boxes = collide(entity, fall, &[plate, arm], &[], false, 0.6, LIFTED);
        let shape = collide(
            entity,
            fall,
            &[plate, arm],
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
        let shape = BlockVoxelShape {
            origin: Vec3::new(3.0, 64.0, 5.0),
            boxes: vec![block],
        };
        let entity = Aabb::new(Vec3::new(3.2, 64.6, 5.2), Vec3::new(3.8, 66.4, 5.8));
        let fall = Vec3::new(0.0, -0.5, 0.0);

        let landed = collide(entity, fall, &[block], &[shape], false, 0.6, LIFTED);

        assert_eq!(landed.y, 64.5 - 64.6);
    }

    #[test]
    fn legacy_collision_moves_on_y_then_x_then_z() {
        let entity = box_at(0.0, 0.0, 0.0);
        let corner = [box_at(1.0, 0.0, 1.0)];
        let movement = Vec3::new(0.5, 0.0, 0.8);

        let legacy = collide_legacy(entity, movement, &corner, false, 0.6, false);
        let modern = collide(entity, movement, &corner, &[], false, 0.6, LIFTED);

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
            collide_legacy(entity, movement, &ahead, false, 0.6, false).movement,
            movement
        );
        assert_eq!(
            collide_legacy(entity, movement, &ahead, false, 0.6, true)
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

        let moved = collide_legacy(entity, jump, &obstacles, true, 0.6, false);

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

        let boxes = collide_legacy(entity, movement, &[entered, ahead], false, 0.6, false);
        let voxels = collide_legacy(entity, movement, &[entered, ahead], false, 0.6, true);

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
            collide(entity, movement, &far, &[], false, 0.6, LIFTED),
            movement
        );
        assert_eq!(
            collide(entity, movement, &near, &[], false, 0.6, LIFTED),
            Vec3::ZERO
        );
    }

    #[test]
    fn the_world_border_drops_a_negligible_movement_until_1_18() {
        let entity = box_at(0.0, 0.0, 0.0);
        let movement = Vec3::new(0.5e-7, 0.0, 0.0);
        let bordered = CollisionRules {
            border_always_collides: true,
            ..LIFTED
        };

        assert_eq!(
            collide(entity, movement, &[], &[], false, 0.6, bordered),
            Vec3::ZERO
        );
        assert_eq!(
            collide(entity, movement, &[], &[], false, 0.6, LIFTED),
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

    #[test]
    fn reports_collision_when_vanilla_resolves_sub_epsilon_motion_to_zero() {
        let entity = box_at(0.0, 0.0, 0.0);
        let obstacle = box_at(1.0, 0.0, 0.0);
        let (motion, collided) = resolve_motion(entity, Vec3::new(0.5e-7, 0.0, 0.0), &[obstacle]);

        assert_eq!(motion.x, 0.0);
        assert!(collided[0]);
    }
}
