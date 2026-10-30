use crate::collision::vanilla_equal;
use crate::math::{Aabb, Vec3};
use crate::simulation::{
    back_off_from_edge, block_speed_factor, collide_with_world, displace, entity_collision_context,
    horizontal_collision_is_minor, main_supporting_block, slowed_by_stuck_speed, PhysicsConfig,
    SimulationState, VersionPhysicsRules,
};
use crate::version::MinecraftVersion;
use crate::world::WorldSnapshot;

/// Direction.from3DDataValue: down, up, north, south, west, east.
const DIRECTIONS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

/// A block a piston moves, which the client ticks after its entities (PistonMovingBlockEntity,
/// TileEntityPiston before 1.13).
#[derive(Clone, Debug, PartialEq)]
pub struct MovingPiston {
    pub position: (i32, i32, i32),
    /// The piston's facing, as Direction.from3DDataValue (0 - 5).
    pub facing: u8,
    pub extending: bool,
    /// The piston's own head or base rather than a block it pushes.
    pub source: bool,
    /// How far it has moved before this tick.
    pub progress: f32,
    /// The collision boxes of the block it moves (getCollisionRelatedBlockState) in its cell,
    /// which push from 1.11.
    pub shapes: Vec<Aabb>,
}

impl MovingPiston {
    fn facing_step(&self) -> (i32, i32, i32) {
        DIRECTIONS[usize::from(self.facing)]
    }

    fn cell(&self) -> Aabb {
        let (x, y, z) = self.position;
        let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
        Aabb::new(min, min + Vec3::new(1.0, 1.0, 1.0))
    }
}

/// The pushes of the moving piston blocks, in the order the client ticks them: each goes 0.5F
/// further this tick and moves the player out of its way. A spectator is not pushed. The pistons
/// of the tick before these moved the player by `piston_deltas` on each axis (Entity.pistonDeltas),
/// which are left with these pushes added.
pub fn push_by_moving_pistons(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    if state.no_physics {
        return;
    }
    for piston in pistons {
        if !release.at_least(1, 9, 0) {
            push_before_1_9(state, world, piston, config, release);
        } else if !release.at_least(1, 11, 0) {
            push_before_1_11(state, world, piston, config, release);
        } else {
            push(state, world, piston, piston_deltas, config, release);
        }
    }
}

/// PistonMovingBlockEntity.moveCollidedEntities: the moved block's boxes, placed at the progress
/// and swept by the distance it goes, move the player out of their way plus 0.01 along the way
/// the block moves (Entity.move(PISTON)); a retracting base then moves it out of the piston's
/// cell (fixEntityWithinPistonBase). The tick that ends the block pushes nothing.
fn push(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    piston: &MovingPiston,
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    if piston.progress >= 1.0 {
        return;
    }
    let progress = piston.progress;
    let distance = f64::from((progress + 0.5) - progress);
    let (fx, fy, fz) = piston.facing_step();
    let sign = if piston.extending { 1 } else { -1 };
    let direction = (fx * sign, fy * sign, fz * sign);
    let extended = f64::from(if piston.extending {
        progress - 1.0
    } else {
        1.0 - progress
    });
    let (x, y, z) = piston.position;
    let origin = Vec3::new(
        f64::from(x) + extended * f64::from(fx),
        f64::from(y) + extended * f64::from(fy),
        f64::from(z) + extended * f64::from(fz),
    );
    let bounds = state.bounds();
    let mut amount = 0.0_f64;
    for shape in &piston.shapes {
        let area = movement_area(shape.translated(origin), direction, distance);
        if area.intersects(bounds) {
            amount = amount.max(movement_out_of(area, direction, bounds));
            if amount >= distance {
                break;
            }
        }
    }
    if amount <= 0.0 {
        return;
    }
    let amount = amount.min(distance) + 0.01;
    move_by_piston(
        state,
        world,
        along(direction, amount),
        piston_deltas,
        config,
        release,
    );
    if piston.extending || !piston.source {
        return;
    }
    let moved = state.bounds();
    let cell = piston.cell();
    if !moved.intersects(cell) {
        return;
    }
    let back = (-direction.0, -direction.1, -direction.2);
    let out = movement_out_of(cell, back, moved) + 0.01;
    let out_of_overlap = movement_out_of(cell, back, moved.intersect(cell)) + 0.01;
    if (out - out_of_overlap).abs() < 0.01 {
        let out = out.min(distance) + 0.01;
        move_by_piston(
            state,
            world,
            along(back, out),
            piston_deltas,
            config,
            release,
        );
    }
}

/// TileEntityPiston.moveCollidedEntities on 1.9 - 1.10: the moved block's box (a head's plate, a
/// retracting base's full cube) at the progress before and after this tick, joined, moves the
/// player out of it plus 0.01 along the way the block moves, by the player's own Entity.move.
/// The tick that ends the block pushes once more.
fn push_before_1_11(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    piston: &MovingPiston,
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    let before = piston.progress;
    let after = if before >= 1.0 {
        before
    } else {
        (before + 0.5).min(1.0)
    };
    let (fx, fy, fz) = piston.facing_step();
    let axis = if fx != 0 {
        0
    } else if fy != 0 {
        1
    } else {
        2
    };
    let sign = fx + fy + fz;
    let shape = if piston.extending {
        head_plate(axis, sign)
    } else {
        Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))
    };
    let (x, y, z) = piston.position;
    let corner = |base: i32, local: f64, offset: f64, step: i32| {
        f64::from(base) + local + offset * f64::from(step)
    };
    let at = |progress: f32| {
        let offset = f64::from(if piston.extending {
            progress - 1.0
        } else {
            1.0 - progress
        });
        Aabb::new(
            Vec3::new(
                corner(x, shape.min.x, offset, fx),
                corner(y, shape.min.y, offset, fy),
                corner(z, shape.min.z, offset, fz),
            ),
            Vec3::new(
                corner(x, shape.max.x, offset, fx),
                corner(y, shape.max.y, offset, fy),
                corner(z, shape.max.z, offset, fz),
            ),
        )
    };
    let area = at(after).minmax(at(before));
    let bounds = state.bounds();
    if !area.intersects(bounds) {
        return;
    }
    let way = if piston.extending { sign } else { -sign };
    let amount = if way > 0 {
        component(area.max, axis) - component(bounds.min, axis)
    } else {
        component(bounds.max, axis) - component(area.min, axis)
    } + 0.01;
    let mut movement = [0.0; 3];
    movement[axis] = amount * f64::from(way);
    let movement = Vec3::new(movement[0], movement[1], movement[2]);
    move_entity(state, world, movement, config, release, true);
}

/// TileEntityPiston.update on 1.8: only an extending block pushes while it moves, by the
/// progress it went plus 0.0625F along the piston's facing, and the tick that ends a block
/// pushes by 0.25F; the player moves when its box meets the block's (BlockPistonMoving: the cell
/// with the side the facing points to drawn in by what is left to go), by its own Entity.move.
fn push_before_1_9(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    piston: &MovingPiston,
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    let before = piston.progress;
    let ended = before >= 1.0;
    if !ended && !piston.extending {
        return;
    }
    let progress = if ended {
        1.0
    } else {
        (before + 0.5_f32).min(1.0)
    };
    let distance = if ended {
        0.25_f32
    } else {
        (progress - before) + 0.0625
    };
    let left = if piston.extending {
        1.0 - progress
    } else {
        progress - 1.0
    };
    let (fx, fy, fz) = piston.facing_step();
    let mut area = piston.cell();
    let draw_in = |min: &mut f64, max: &mut f64, step: i32| {
        let inset = f64::from(step as f32 * left);
        if step < 0 {
            *min -= inset;
        } else {
            *max -= inset;
        }
    };
    draw_in(&mut area.min.x, &mut area.max.x, fx);
    draw_in(&mut area.min.y, &mut area.max.y, fy);
    draw_in(&mut area.min.z, &mut area.max.z, fz);
    if !area.intersects(state.bounds()) {
        return;
    }
    let movement = Vec3::new(
        f64::from(distance * fx as f32),
        f64::from(distance * fy as f32),
        f64::from(distance * fz as f32),
    );
    move_entity(state, world, movement, config, release, true);
}

/// Entity.move(PISTON): at most 0.51 along each axis in a game tick (limitPistonMovement), along
/// the first axis the movement has.
fn move_by_piston(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    let movement = if release.at_least(1, 14, 0) && movement.length_squared() <= 1.0e-7 {
        movement
    } else {
        let components = [movement.x, movement.y, movement.z];
        let Some(axis) = components.iter().position(|value| *value != 0.0) else {
            return;
        };
        let limited = (components[axis] + piston_deltas[axis]).clamp(-0.51, 0.51);
        let amount = limited - piston_deltas[axis];
        piston_deltas[axis] = limited;
        if amount.abs() <= 9.999999747378752e-6 {
            return;
        }
        let mut limited_movement = [0.0; 3];
        limited_movement[axis] = amount;
        Vec3::new(
            limited_movement[0],
            limited_movement[1],
            limited_movement[2],
        )
    };
    if movement == Vec3::ZERO {
        return;
    }
    move_entity(state, world, movement, config, release, false);
}

/// Entity.move of the player by a movement it did not make itself: slowed by the stuck speed
/// multiplier, collided as its own move (stepping up), the collision flags and the ground from
/// it, the motion stopped on the axes it hit and, from 1.15, scaled by the block speed factor.
/// Before 1.11 (moveEntity) a sneaking player backs off from edges too.
fn move_entity(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    config: PhysicsConfig,
    release: MinecraftVersion,
    backs_off_from_edges: bool,
) {
    let rules = VersionPhysicsRules {
        release,
        step_height: state.step_height.unwrap_or(config.step_height),
        ..VersionPhysicsRules::default()
    };
    let mut movement = slowed_by_stuck_speed(state, movement);
    let context = entity_collision_context(state);
    if backs_off_from_edges {
        movement = back_off_from_edge(state, world, &context, movement, rules.step_height, release);
    }
    let collision = collide_with_world(state, world, &context, movement, rules);
    let displacement = collision.movement;
    displace(state, movement, collision, release);
    let blocked = |requested: f64, moved: f64| {
        if release.at_least(1, 14, 0) {
            !vanilla_equal(requested, moved)
        } else {
            requested != moved
        }
    };
    let x_collision = blocked(movement.x, displacement.x);
    let z_collision = blocked(movement.z, displacement.z);
    let y_collision = movement.y != displacement.y;
    state.collided_horizontally = x_collision || z_collision;
    state.minor_horizontal_collision = state.minor_horizontal_collision_supported
        && state.is_player
        && state.collided_horizontally
        && horizontal_collision_is_minor(state, displacement, release);
    state.collided_vertically = y_collision;
    state.vertical_collision_below = y_collision && movement.y < 0.0;
    state.on_ground = state.vertical_collision_below;
    let collisions = [x_collision, y_collision, z_collision];
    stop_blocked_motion(state, movement, displacement, collisions, release);
    if release.at_least(1, 15, 0) {
        let support = main_supporting_block(world, state, Some(displacement));
        let speed_factor = f64::from(block_speed_factor(world, state, support, release));
        state.velocity.x *= speed_factor;
        state.velocity.z *= speed_factor;
    }
}

/// The motion after a move that hit something: from 26.2 a collided axis becomes the motion
/// reversed and scaled by the entity's bounciness (restituteMovementAfterCollisions); before, it
/// stops (1.14 - 1.18.1 put back the x motion when z stops too), and landing multiplies the
/// vertical motion by 0 (zeroes it before 1.14).
fn stop_blocked_motion(
    state: &mut SimulationState,
    movement: Vec3,
    displacement: Vec3,
    [x_collision, y_collision, z_collision]: [bool; 3],
    release: MinecraftVersion,
) {
    let current = state.velocity;
    if release.at_least(26, 2, 0) {
        let restitution = if state.sneaking {
            0.0
        } else {
            state.entity_bounciness
        };
        if x_collision {
            state.velocity.x = -current.x * restitution;
        }
        if y_collision {
            state.velocity.y = -current.y * restitution;
        }
        if z_collision {
            state.velocity.z = -current.z * restitution;
        }
        return;
    }
    let (stops_x, stops_z) = if release.at_least(1, 18, 2) {
        (x_collision, z_collision)
    } else if release.at_least(1, 14, 0) {
        let stops_z = movement.z != displacement.z;
        (movement.x != displacement.x && !stops_z, stops_z)
    } else {
        (movement.x != displacement.x, movement.z != displacement.z)
    };
    if stops_x {
        state.velocity.x = 0.0;
    }
    if stops_z {
        state.velocity.z = 0.0;
    }
    if y_collision {
        state.velocity.y = if release.at_least(1, 14, 0) {
            current.y * 0.0
        } else {
            0.0
        };
    }
}

/// PistonMath.getMovementArea: the slab the box's face toward `direction` sweeps over
/// `distance`.
fn movement_area(bounds: Aabb, direction: (i32, i32, i32), distance: f64) -> Aabb {
    let signed = distance * f64::from(direction.0 + direction.1 + direction.2);
    let low = signed.min(0.0);
    let high = signed.max(0.0);
    let (min, max) = (bounds.min, bounds.max);
    match direction {
        (-1, 0, 0) => Aabb::new(
            Vec3::new(min.x + low, min.y, min.z),
            Vec3::new(min.x + high, max.y, max.z),
        ),
        (1, 0, 0) => Aabb::new(
            Vec3::new(max.x + low, min.y, min.z),
            Vec3::new(max.x + high, max.y, max.z),
        ),
        (0, -1, 0) => Aabb::new(
            Vec3::new(min.x, min.y + low, min.z),
            Vec3::new(max.x, min.y + high, max.z),
        ),
        (0, 0, -1) => Aabb::new(
            Vec3::new(min.x, min.y, min.z + low),
            Vec3::new(max.x, max.y, min.z + high),
        ),
        (0, 0, 1) => Aabb::new(
            Vec3::new(min.x, min.y, max.z + low),
            Vec3::new(max.x, max.y, max.z + high),
        ),
        _ => Aabb::new(
            Vec3::new(min.x, max.y + low, min.z),
            Vec3::new(max.x, max.y + high, max.z),
        ),
    }
}

/// PistonMovingBlockEntity.getMovement: how far `entity` must go along `direction` to leave
/// `area`.
fn movement_out_of(area: Aabb, direction: (i32, i32, i32), entity: Aabb) -> f64 {
    match direction {
        (1, 0, 0) => area.max.x - entity.min.x,
        (-1, 0, 0) => entity.max.x - area.min.x,
        (0, -1, 0) => entity.max.y - area.min.y,
        (0, 0, 1) => area.max.z - entity.min.z,
        (0, 0, -1) => entity.max.z - area.min.z,
        _ => area.max.y - entity.min.y,
    }
}

/// PistonHeadBlock's box before 1.11: the plate on the side its facing points to.
fn head_plate(axis: usize, sign: i32) -> Aabb {
    let mut min = [0.0, 0.0, 0.0];
    let mut max = [1.0, 1.0, 1.0];
    if sign > 0 {
        min[axis] = 0.75;
    } else {
        max[axis] = 0.25;
    }
    Aabb::new(
        Vec3::new(min[0], min[1], min[2]),
        Vec3::new(max[0], max[1], max[2]),
    )
}

fn along(direction: (i32, i32, i32), amount: f64) -> Vec3 {
    Vec3::new(
        amount * f64::from(direction.0),
        amount * f64::from(direction.1),
        amount * f64::from(direction.2),
    )
}

fn component(vector: Vec3, axis: usize) -> f64 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

#[cfg(test)]
mod tests {
    use super::{push_by_moving_pistons, MovingPiston};
    use crate::math::{Aabb, Vec3};
    use crate::simulation::{PhysicsConfig, SimulationState};
    use crate::version::MinecraftVersion;
    use crate::world::WorldSnapshot;

    const EAST: u8 = 5;

    fn standing_player(x: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(x, 101.0, 0.5),
            velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
            half_width: f64::from(0.3_f32),
            height: f64::from(1.8_f32),
            on_ground: true,
            is_player: true,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        }
    }

    fn extending_head(progress: f32) -> MovingPiston {
        MovingPiston {
            position: (0, 101, 0),
            facing: EAST,
            extending: true,
            source: true,
            progress,
            shapes: vec![
                Aabb::new(Vec3::new(0.75, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                Aabb::new(
                    Vec3::new(-0.25, 0.375, 0.375),
                    Vec3::new(0.75, 0.625, 0.625),
                ),
            ],
        }
    }

    fn push(state: &mut SimulationState, piston: MovingPiston, release: MinecraftVersion) {
        let world = WorldSnapshot::new();
        push_by_moving_pistons(
            state,
            &world,
            &[piston],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
    }

    #[test]
    fn an_extending_head_moves_the_player_out_of_its_way_plus_a_hundredth() {
        let release = MinecraftVersion::new(1, 21, 1);
        let mut state = standing_player(0.5);
        push(&mut state, extending_head(0.0), release);
        let out_of_the_way = 0.5 - (0.5 - f64::from(0.3_f32)) + 0.01;
        assert_eq!(state.position.x, 0.5 + out_of_the_way);
        assert_eq!(state.position.x, 0.810000011920929);
        assert!(!state.on_ground);
        assert_eq!(state.velocity.y, -0.0784000015258789);
        push(&mut state, extending_head(0.5), release);
        assert_eq!(state.position.x, 1.310000011920929);
    }

    #[test]
    fn a_block_that_reached_its_end_pushes_nothing_from_1_11() {
        let mut state = standing_player(0.5);
        push(
            &mut state,
            extending_head(1.0),
            MinecraftVersion::new(1, 12, 2),
        );
        assert_eq!(state.position.x, 0.5);
        assert!(state.on_ground);
    }

    #[test]
    fn pistons_move_the_player_at_most_0_51_along_an_axis_in_a_tick() {
        let world = WorldSnapshot::new();
        let mut state = standing_player(0.5);
        let further = MovingPiston {
            position: (1, 101, 0),
            ..extending_head(0.0)
        };
        push_by_moving_pistons(
            &mut state,
            &world,
            &[extending_head(0.0), further],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            MinecraftVersion::new(1, 21, 1),
        );
        assert_eq!(state.position.x, 0.5 + 0.51);
    }

    fn retracting_base() -> MovingPiston {
        MovingPiston {
            position: (-1, 101, 0),
            extending: false,
            ..extending_head(0.0)
        }
    }

    #[test]
    fn a_retracting_head_pulls_the_player_toward_the_base() {
        let mut state = standing_player(-0.75);
        push(
            &mut state,
            retracting_base(),
            MinecraftVersion::new(1, 21, 1),
        );
        let out_of_the_way = (-0.75 + f64::from(0.3_f32)) - (-0.75) + 0.01;
        assert_eq!(state.position.x, -0.75 - out_of_the_way);
    }

    #[test]
    fn a_player_left_inside_the_retracting_base_goes_back_out_of_its_cell() {
        let mut state = standing_player(-0.1);
        push(
            &mut state,
            retracting_base(),
            MinecraftVersion::new(1, 21, 1),
        );
        assert_eq!(state.position.x, (-0.1 - 0.51) + 0.51);
    }

    #[test]
    fn on_1_8_an_extending_block_moves_the_player_by_its_progress_and_a_sixteenth() {
        let release = MinecraftVersion::new(1, 8, 8);
        let mut state = standing_player(0.5);
        push(&mut state, extending_head(0.0), release);
        assert_eq!(state.position.x, 1.0625);
        push(&mut state, extending_head(0.5), release);
        assert_eq!(state.position.x, 1.625);
        push(&mut state, extending_head(1.0), release);
        assert_eq!(state.position.x, 1.625);
    }

    #[test]
    fn on_1_9_the_plate_swept_this_tick_moves_the_player_by_its_own_move() {
        let release = MinecraftVersion::new(1, 10, 2);
        let mut state = standing_player(0.5);
        push(&mut state, extending_head(0.0), release);
        assert_eq!(state.position.x, 0.810000011920929);
        push(&mut state, extending_head(0.5), release);
        assert_eq!(state.position.x, 1.310000011920929);
    }

    #[test]
    fn a_spectator_is_not_pushed() {
        let mut state = SimulationState {
            no_physics: true,
            ..standing_player(0.5)
        };
        push(
            &mut state,
            extending_head(0.0),
            MinecraftVersion::new(1, 21, 1),
        );
        assert_eq!(state.position.x, 0.5);
    }
}
