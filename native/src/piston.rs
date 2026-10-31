use crate::block_traversal::Movement;
use crate::collision::vanilla_equal;
use crate::math::{Aabb, Vec3};
use crate::simulation::{
    apply_block_effects, back_off_from_edge, block_below_affecting_movement, block_jump_factor,
    block_position, block_speed_factor, blocks_inside, blocks_inside_moves, check_fall_damage,
    check_supporting_block, collide_with_world, displace, entity_collision_context,
    horizontal_collision_is_minor, kept_supporting_block, movement_speed_attribute,
    restitute_movement_after_collisions, slowed_by_stuck_speed, update_auto_jump,
    update_in_lava_after_move, FallCheck, FallDamageRules, FreezeStep, LandingDamageRounding,
    PhysicsConfig, SimulationState, VersionPhysicsRules,
};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, WorldSnapshot};

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
    /// which push from 1.11, and on 1.8 those of a block it pushes (Block.getCollisionBox).
    pub shapes: Vec<Aabb>,
    /// The outline boxes of a block it pushes in its cell, whose bounds push on 1.9 - 1.10
    /// (IBlockState.getCollisionBox(BlockView), MCP's getBoundingBox); none: the full cube,
    /// Block.collisionBox's default (1.9.4 Block.java:47, 352).
    pub outline_shapes: Vec<Aabb>,
    /// The block it moves is a slime block: the player it meets takes the motion of the way it
    /// moves along that axis (causeBounce).
    pub bounces: bool,
    /// The block it moves is a honey block: a horizontal move carries the player standing on it
    /// (isStickyForEntities, 1.15+).
    pub sticky_for_entities: bool,
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

    /// The bounds of its boxes in its cell (VoxelShape.bounds; on 1.8 Block.getCollisionBox of the
    /// block it pushes), none for a block without collision.
    fn shape_bounds(&self) -> Option<Aabb> {
        let first = *self.shapes.first()?;
        Some(
            self.shapes
                .iter()
                .fold(first, |bounds, shape| bounds.minmax(*shape)),
        )
    }

    /// The bounds of the outline boxes of a block it pushes, the full cube without them: the box
    /// it moves on 1.9 - 1.10 (1.9.4 PistonBlockEntity.java:83-88, Block.getCollisionBox(BlockState,
    /// BlockView, BlockPos) at Block.java:352, StateManager.java:380).
    fn outline_bounds(&self) -> Aabb {
        let Some(&first) = self.outline_shapes.first() else {
            return Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        };
        self.outline_shapes
            .iter()
            .fold(first, |bounds, shape| bounds.minmax(*shape))
    }

    /// getMovementDirection: the facing while it extends, the opposite while it retracts.
    fn movement_direction(&self) -> (i32, i32, i32) {
        let (fx, fy, fz) = self.facing_step();
        let sign = if self.extending { 1 } else { -1 };
        (fx * sign, fy * sign, fz * sign)
    }

    /// moveByPositionAndProgress: where its cell's corner is at the progress before this tick.
    fn origin(&self) -> Vec3 {
        let (fx, fy, fz) = self.facing_step();
        let extended = f64::from(if self.extending {
            self.progress - 1.0
        } else {
            1.0 - self.progress
        });
        let (x, y, z) = self.position;
        Vec3::new(
            f64::from(x) + extended * f64::from(fx),
            f64::from(y) + extended * f64::from(fy),
            f64::from(z) + extended * f64::from(fz),
        )
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
            carry(state, world, piston, piston_deltas, config, release);
        }
    }
}

/// PistonMovingBlockEntity.moveCollidedEntities: the moved block's boxes, placed at the progress
/// and swept by the distance it goes, move the player out of their way plus 0.01 along the way
/// the block moves (Entity.move(PISTON)); a retracting base then moves it out of the piston's
/// cell (fixEntityWithinPistonBase). A slime block first gives a player in the area of its boxes'
/// bounds and their sweep the motion of the way it moves, pushed or not. The tick that ends the
/// block pushes nothing.
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
    let direction = piston.movement_direction();
    let origin = piston.origin();
    let bounds = state.bounds();
    if let Some(shape_bounds) = piston.shape_bounds().filter(|_| piston.bounces) {
        // the entities of getEntities(movementArea.minmax(box)), the box the shape's bounds, from
        // the cell's corners outward before 1.16 (1.15 PistonBlockEntity.java:103-128, 195-213;
        // 1.16 :105-131; 26.3 PistonMovingBlockEntity.java:110-150)
        let shape_bounds = if release.at_least(1, 16, 0) {
            shape_bounds
        } else {
            shape_bounds.minmax(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
        };
        let area = shape_bounds.translated(origin);
        if movement_area(area, direction, distance)
            .minmax(area)
            .intersects(bounds)
        {
            bounce(state, direction);
        }
    }
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
/// retracting base's full cube, a pushed block's outline box) at the progress before and after
/// this tick, joined, moves the player out of it plus 0.01 along the way the block moves, by the
/// player's own Entity.move. A slime block first sets the motion of the way it moves. The tick
/// that ends the block pushes once more.
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
    let shape = if !piston.source {
        // a block it pushes moves its outline box, pushedBlock.getCollisionBox(BlockView, pos)
        // (1.9.4 PistonBlockEntity.java:83-88), not its collision boxes
        piston.outline_bounds()
    } else if piston.extending {
        head_plate(axis, sign)
    } else {
        Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))
    };
    let (x, y, z) = piston.position;
    // the box offset by the float progress times the step, then by the position (1.9.4
    // PistonBlockEntity.java:83-91)
    let corner = |base: i32, local: f64, offset: f32, step: i32| {
        (local + f64::from(offset * step as f32)) + f64::from(base)
    };
    let at = |progress: f32| {
        let offset = if piston.extending {
            progress - 1.0
        } else {
            1.0 - progress
        };
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
    if piston.bounces {
        // a slime block sets the motion and still moves the player (1.9.4
        // PistonBlockEntity.java:99-136)
        bounce(state, piston.movement_direction());
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
    move_entity(state, world, movement, config, release, false);
}

/// TileEntityPiston.update on 1.8: only an extending block pushes while it moves, by the
/// progress it went plus 0.0625F along the piston's facing, and the tick that ends a block
/// pushes by 0.25F (an extending slime block sets the motion instead); the player moves when its
/// box meets the block's (BlockPistonMoving: the cell, or a pushed block's own box, with the side
/// the facing points to drawn in by what is left to go), by its own Entity.move.
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
    let mut area = if piston.source {
        piston.cell()
    } else {
        // a block it pushes moves its own box, none without one (1.8.9
        // PistonExtensionBlock.java:166-200)
        let Some(shape) = piston.shape_bounds() else {
            return;
        };
        shape.translated(piston.cell().min)
    };
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
    if piston.bounces && piston.extending {
        // an extending slime block sets the motion in place of the move (1.8.9
        // PistonBlockEntity.java:94-107)
        bounce(state, (fx, fy, fz));
        return;
    }
    let movement = Vec3::new(
        f64::from(distance * fx as f32),
        f64::from(distance * fy as f32),
        f64::from(distance * fz as f32),
    );
    move_entity(state, world, movement, config, release, false);
}

/// PistonMovingBlockEntity.moveStuckEntities (1.15+): a honey block that moves horizontally
/// carries a player on the ground in the box from the block's top to 1.5 over its cell, placed
/// at the progress, with its position over that box (from 1.20 also one the block supports), by
/// the whole distance the block goes this tick (1.15 PistonBlockEntity.java:163-189; 1.20
/// :190-209; 26.3 PistonMovingBlockEntity.java:172-190).
fn carry(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    piston: &MovingPiston,
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    if !release.at_least(1, 15, 0) || !piston.sticky_for_entities || piston.progress >= 1.0 {
        return;
    }
    let direction = piston.movement_direction();
    if direction.1 != 0 {
        return;
    }
    let progress = piston.progress;
    let distance = f64::from((progress + 0.5) - progress);
    // VoxelShape.max(Y), negative infinity for an empty shape
    let top = piston
        .shapes
        .iter()
        .fold(f64::NEG_INFINITY, |top, shape| top.max(shape.max.y));
    let ceiling = if release.at_least(1, 20, 0) {
        1.5000010000000001
    } else {
        1.5000000999999998
    };
    let area = Aabb::new(Vec3::new(0.0, top, 0.0), Vec3::new(1.0, ceiling, 1.0))
        .translated(piston.origin());
    if !state.on_ground || !area.intersects(state.bounds()) {
        return;
    }
    let supported = release.at_least(1, 20, 0)
        && kept_supporting_block(world, state, None) == Some(piston.position);
    let position = state.position;
    let over = position.x >= area.min.x
        && position.x <= area.max.x
        && position.z >= area.min.z
        && position.z <= area.max.z;
    if !supported && !over {
        return;
    }
    move_by_piston(
        state,
        world,
        along(direction, distance),
        piston_deltas,
        config,
        release,
    );
}

/// A slime block's bounce: the motion along the axis of `direction` becomes its step.
fn bounce(state: &mut SimulationState, direction: (i32, i32, i32)) {
    match direction {
        (x, 0, 0) if x != 0 => state.velocity.x = f64::from(x),
        (0, y, 0) if y != 0 => state.velocity.y = f64::from(y),
        (_, _, z) => state.velocity.z = f64::from(z),
    }
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
    let start = state.position;
    let support = move_entity(state, world, movement, config, release, true);
    if release.at_least(1, 21, 2) {
        // the effects of the blocks the move went through: from 1.21.5 moveEntityByPiston's
        // applyEffectsFromBlocks(previousPos, position()) of this move alone (1.21.5
        // PistonMovingBlockEntity.java:186-193, 26.3 :163-170); 1.21.2 - 1.21.4 tickBlockCollision()
        // from getLastRenderPos, where the tick started (1.21.3 PistonBlockEntity.java:183-191,
        // Entity.java:752-767)
        let from = if release.at_least(1, 21, 5) {
            start
        } else {
            state.previous_position.unwrap_or(start)
        };
        let movements = [Movement {
            from,
            to: state.position,
            axis_order: None,
        }];
        let (inside, steps) = blocks_inside_moves(state, world, &movements, release);
        apply_block_effects(state, world, support, release, &inside);
        apply_powder_snow(state, world, &inside, &steps, release);
    }
}

/// PowderSnowBlock.entityInside of the blocks a piston's move went through, from 1.17: it sets
/// Entity.isInPowderSnow, which the next tick's baseTick keeps as wasInPowderSnow (1.17
/// PowderSnowBlock.java:70, Entity.java:414-415; 1.21.5 Entity.java:465-466); from 1.21.5 by
/// each step's InsideBlockEffectType.FREEZE, which adds a frozen tick too, and fire and lava
/// clear them, as the tick's own steps do (1.21.5 InsideBlockEffectType.java:7-12,
/// PowderSnowBlock.java:94).
fn apply_powder_snow(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    steps: &[FreezeStep],
    release: MinecraftVersion,
) {
    if state.no_physics || !release.at_least(1, 17, 0) {
        return;
    }
    if !release.at_least(1, 21, 5) {
        if inside
            .iter()
            .any(|&(position, _)| world.effect_at(position) == BlockEffect::PowderSnow)
        {
            state.in_powder_snow = true;
        }
        return;
    }
    let required_to_freeze = state.ticks_required_to_freeze.max(140);
    for step in steps {
        if step.freeze {
            state.in_powder_snow = true;
            if !state.powder_snow_immune {
                state.ticks_frozen = state.ticks_frozen.saturating_add(1).min(required_to_freeze);
            }
        }
        if step.clear {
            state.ticks_frozen = 0;
        }
    }
}

/// Entity.move of the player by a movement it did not make itself: slowed by the stuck speed
/// multiplier (not a piston's from 1.21.10), collided as its own move (stepping up), the
/// collision flags and the ground from it, the fall checked (checkFallDamage), the motion stopped
/// on the axes it hit or bounced off the block it lands on, before 1.21.2 the blocks it ends in
/// (stepOn, checkInsideBlocks), from 1.15 the motion scaled by the block speed factor, its inLava
/// from the blocks it ends in 1.14 - 1.16.1, and LocalPlayer.updateAutoJump after it. Before 1.11
/// (moveEntity, `by_piston` false) a sneaking player backs off from edges too. The block the
/// player stands on after it is returned.
fn move_entity(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    config: PhysicsConfig,
    release: MinecraftVersion,
    by_piston: bool,
) -> Option<(i32, i32, i32)> {
    // Entity.move(PISTON) steps with maxUpStep(), the STEP_HEIGHT attribute on a LivingEntity from
    // 1.20.5, which reads through RangedAttribute.sanitizeValue as the tick's does: NaN is 0, the
    // rest clamped to 0..10 (1.21.11 RangedAttribute.java:35-37, Attributes.java:86).
    let step_height = match state.step_height {
        Some(value) if value.is_nan() => 0.0,
        Some(value) => value.clamp(0.0, 10.0),
        None => config.step_height,
    };
    // the GRAVITY attribute as the tick reads it, for the restitution's gravity compensation
    let gravity = match state.gravity {
        Some(value) if value.is_nan() => -1.0,
        Some(value) => value.clamp(-1.0, 1.0),
        None => config.gravity,
    };
    let rules = VersionPhysicsRules {
        release,
        gravity,
        step_height,
        ..VersionPhysicsRules::default()
    };
    let start = state.position;
    let slowed = slowed_by_stuck_speed(state, movement);
    // from 1.21.10 Entity.move does not slow a PISTON move by the stuck speed multiplier, though
    // it still uses the multiplier up with the motion (1.21.9 Entity.java:693-696; 1.21.10
    // :693-699; 26.3 :783-789)
    let mut movement = if by_piston && release.at_least(1, 21, 10) {
        movement
    } else {
        slowed
    };
    let context = entity_collision_context(state);
    if !by_piston {
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
    let support = check_supporting_block(world, state, Some(displacement));
    // Entity.move's checkFallDamage and the landing block's updateEntityMovementAfterFallOn (a
    // slime block's or a bed's bounce), restituteMovementAfterCollisions from 26.2, as the tick's
    // own move (1.8.9 Entity.java:655-666, 1.16.5 :639-651, 1.21.10 :740-758, 26.3 :827-840); the
    // profiles' rounding, attribute damage and restitution go by release
    let fall = FallDamageRules {
        landing_damage_rounding: if release.at_least(1, 21, 5) {
            LandingDamageRounding::Floor
        } else {
            LandingDamageRounding::Ceil
        },
        restitution_enabled: release.at_least(26, 2, 0),
        modern_fall_damage: release.at_least(1, 20, 5),
    };
    let landing = check_fall_damage(
        state,
        world,
        rules,
        support,
        displacement.y,
        y_collision,
        fall,
    );
    let collisions = [x_collision, y_collision, z_collision];
    if fall.restitution_enabled {
        // only for a move that went vertically into its vertical collision or hit something
        // sideways: a step up from a sideways push keeps the motion (26.2 Entity.java:793, 815;
        // 26.3 :818, 837)
        if (movement.y.abs() > 0.0 && y_collision) || x_collision || z_collision {
            restitute_movement_after_collisions(
                state,
                world,
                support,
                rules,
                collisions,
                displacement.y,
            );
        }
    } else {
        stop_blocked_motion(state, movement, displacement, collisions, landing, release);
    }
    if !release.at_least(1, 21, 2) {
        // stepOn and checkInsideBlocks at the end of the move (1.8.9 Entity.java:677, 701; 1.16.5
        // :655, 689); from 1.21.2 the piston applies them after it
        let inside: Vec<_> = blocks_inside(state, release)
            .into_iter()
            .map(|position| (position, true))
            .collect();
        apply_block_effects(state, world, support, release, &inside);
        apply_powder_snow(state, world, &inside, &[], release);
    }
    if release.at_least(1, 15, 0) {
        let speed_factor = f64::from(block_speed_factor(world, state, support, release));
        state.velocity.x *= speed_factor;
        state.velocity.z *= speed_factor;
    }
    update_in_lava_after_move(state, world, release);
    // LocalPlayer.move runs updateAutoJump after every move, a piston's too (1.10.2
    // ClientPlayerEntity.java:810-815, 1.21.11 LocalPlayer.java:947-955, 26.3 :977-985), with
    // getSpeed(), the movement speed the tick took. Before 1.13 its ceiling check sees the moving
    // block's box, the tile entity's swept box whatever it moves (1.12.2
    // ClientPlayerEntity.java:882-887, PistonExtensionBlock.java:144-147; 1.10.2
    // PistonExtensionBlock.java:126-135), which the engine's world does not hold: the block that
    // pressed the player onto the ground is there, so it never jumps
    if !release.at_least(1, 13, 0) {
        return support;
    }
    let speed = state.movement_speed_value.unwrap_or_else(|| {
        movement_speed_attribute(
            state.movement_speed.unwrap_or(config.movement_speed),
            state.speed_effect_amplifier,
            state.slowness_effect_amplifier,
            state.sprint_modifier.unwrap_or(state.sprinting),
            config.sprint_multiplier,
            release,
        )
    });
    let jump_factor = block_jump_factor(
        world,
        block_position(state.position),
        block_below_affecting_movement(state, support, release),
    );
    update_auto_jump(
        state,
        world,
        state.position - start,
        speed as f32,
        jump_factor,
        release,
    );
    support
}

/// The motion after a move that hit something before 26.2: it stops (1.14 - 1.18.1 put back the
/// x motion when z stops too), and landing multiplies the vertical motion by 0 (zeroes it before
/// 1.14) but where the slime block bounced it (check_fall_damage) or a bed bounces it back by
/// 0.66 (BedBlock.bounceUp, from 1.12), as the tick's own move.
fn stop_blocked_motion(
    state: &mut SimulationState,
    movement: Vec3,
    displacement: Vec3,
    [x_collision, y_collision, z_collision]: [bool; 3],
    landing: FallCheck,
    release: MinecraftVersion,
) {
    let current = state.velocity;
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
    if y_collision && !landing.bounced {
        if landing.landed_on_bed && !state.sneaking {
            if current.y < 0.0 {
                state.velocity.y = -current.y * 0.6600000262260437;
            }
        } else {
            state.velocity.y = if release.at_least(1, 14, 0) {
                current.y * 0.0
            } else {
                0.0
            };
        }
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
    use crate::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

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
            outline_shapes: Vec::new(),
            bounces: false,
            sticky_for_entities: false,
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
    fn a_piston_push_into_lava_sets_in_lava_1_14_to_1_16_1() {
        // Entity.move(PISTON) clears inLava and runs checkBlockCollision (1.14
        // Entity.java:636-638, 1.16 :657-659); from 1.16.2 the base tick's lava height sets it
        // (1.16.5 Entity.java:1181-1183)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 101, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let in_lava_after = |release| {
            let mut state = standing_player(0.5);
            push_by_moving_pistons(
                &mut state,
                &world,
                &[extending_head(0.0)],
                &mut [0.0; 3],
                PhysicsConfig::default(),
                release,
            );
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            state.in_lava
        };

        assert!(in_lava_after(MinecraftVersion::new(1, 14, 4)));
        assert!(in_lava_after(MinecraftVersion::new(1, 16, 1)));
        assert!(!in_lava_after(MinecraftVersion::new(1, 16, 2)));
        assert!(!in_lava_after(MinecraftVersion::new(1, 13, 2)));
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
    fn a_piston_push_steps_with_the_sanitized_step_height() {
        // PistonMovingBlockEntity moves the entity with Entity.move(MoverType.PISTON), which steps
        // with maxUpStep() = getAttributeValue(STEP_HEIGHT), sanitized by
        // RangedAttribute.sanitizeValue: NaN is 0, the rest clamped to 0..10 (1.21.11
        // RangedAttribute.java:35-37, Attributes.java:86)
        let mut world = WorldSnapshot::new();
        for y in 101..113 {
            world.set_block(
                (1, y, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        let pushed = |step_height: f64| {
            let mut state = SimulationState {
                step_height: Some(step_height),
                ..standing_player(0.5)
            };
            push_by_moving_pistons(
                &mut state,
                &world,
                &[extending_head(0.0)],
                &mut [0.0; 3],
                PhysicsConfig::default(),
                MinecraftVersion::new(1, 21, 11),
            );
            state.position
        };

        // 12 blocks up is out of reach of the clamped 10: the wall stops the player
        let blocked = pushed(10.0);
        assert_eq!(blocked.y, 101.0);
        assert_eq!(blocked.x, 1.0 - f64::from(0.3_f32));
        assert_eq!(pushed(20.0), blocked);
        assert_eq!(pushed(-3.0), pushed(0.0));
        assert_eq!(pushed(f64::NAN), pushed(0.0));
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

    fn moved_block(position: (i32, i32, i32), extending: bool, shape: Aabb) -> MovingPiston {
        MovingPiston {
            position,
            facing: EAST,
            extending,
            source: false,
            progress: 0.0,
            shapes: vec![shape],
            outline_shapes: vec![shape],
            bounces: false,
            sticky_for_entities: false,
        }
    }

    fn slime_block(position: (i32, i32, i32), extending: bool) -> MovingPiston {
        MovingPiston {
            bounces: true,
            ..moved_block(
                position,
                extending,
                Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
            )
        }
    }

    #[test]
    fn a_pushed_slime_block_bounces_the_player_then_pushes_it() {
        // moveCollidedEntities: a slime block sets the motion along its movement axis to the
        // direction's step, a client player included, before the push (1.21.11
        // PistonMovingBlockEntity.java:122-175; 1.13.2 PistonBlockEntity.java:109-150)
        for release in [
            MinecraftVersion::new(1, 11, 2),
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = standing_player(1.2);
            push(&mut state, slime_block((1, 101, 0), true), release);
            assert_eq!(state.velocity.x, 1.0, "{release}");
            assert_eq!(state.velocity.y, -0.0784000015258789, "{release}");
            assert_eq!(state.position.x, 1.2 + 0.51, "{release}");

            // in the block's own box but not in its sweep: bounced, not pushed
            let mut state = standing_player(0.5);
            push(&mut state, slime_block((1, 101, 0), true), release);
            assert_eq!(state.velocity.x, 1.0, "{release}");
            assert_eq!(state.position.x, 0.5, "{release}");

            // a pulled slime block bounces the way it moves
            let mut state = standing_player(0.6);
            push(&mut state, slime_block((0, 101, 0), false), release);
            assert_eq!(state.velocity.x, -1.0, "{release}");
            let pulled = (0.6 + f64::from(0.3_f32)) - 0.5 + 0.01;
            assert_eq!(state.position.x, 0.6 - pulled, "{release}");
        }
    }

    #[test]
    fn a_pushed_block_other_than_slime_does_not_bounce() {
        let mut state = standing_player(1.2);
        let stone = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
        );
        push(&mut state, stone, MinecraftVersion::new(1, 21, 11));
        assert_eq!(state.velocity.x, 0.0);
        assert_eq!(state.position.x, 1.2 + 0.51);
    }

    #[test]
    fn on_1_8_an_extending_slime_block_bounces_in_place_of_the_push() {
        // TileEntityPiston.pushEntities: slime && extending sets the motion along the facing,
        // else the entity moves (1.8.9 PistonBlockEntity.java:94-107)
        let release = MinecraftVersion::new(1, 8, 8);
        let mut state = standing_player(1.2);
        push(&mut state, slime_block((1, 101, 0), true), release);
        assert_eq!(state.velocity.x, 1.0);
        assert_eq!(state.position.x, 1.2);

        // a retracting block pushes only the tick it ends, and a slime block's then moves
        // (1.8.9 PistonBlockEntity.java:128-130)
        let mut state = standing_player(0.5);
        let ended = MovingPiston {
            progress: 1.0,
            ..slime_block((0, 101, 0), false)
        };
        push(&mut state, ended, release);
        assert_eq!(state.velocity.x, 0.0);
        assert_eq!(state.position.x, 0.5 + f64::from(0.25_f32));
    }

    #[test]
    fn on_1_9_a_slime_block_bounces_and_still_pushes() {
        // 1.9.4 PistonBlockEntity.java:99-136: the motion, then the move out of the swept box
        let release = MinecraftVersion::new(1, 10, 2);
        let mut state = standing_player(1.2);
        push(&mut state, slime_block((1, 101, 0), true), release);
        assert_eq!(state.velocity.x, 1.0);
        let out = 1.5 - (1.2 - f64::from(0.3_f32)) + 0.01;
        // the box moves by `out`, the position with it
        assert!((state.position.x - (1.2 + out)).abs() < 1e-12);
        assert_eq!(state.position.x, 1.8100000119209287);
    }

    #[test]
    fn on_1_9_a_pushed_block_moves_its_own_box() {
        // the pushed block's getCollisionBox(BlockView), not the head's plate (1.9.4
        // PistonBlockEntity.java:83-88): a full cube at x 0..1.5 this tick
        let release = MinecraftVersion::new(1, 9, 4);
        let stone = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
        );
        let mut state = standing_player(0.5);
        push(&mut state, stone, release);
        let out = 1.5 - (0.5 - f64::from(0.3_f32)) + 0.01;
        assert_eq!(state.position.x, 1.8100000119209287);
        assert!((state.position.x - (0.5 + out)).abs() < 1e-12);
    }

    fn honey_block(position: (i32, i32, i32), extending: bool) -> MovingPiston {
        MovingPiston {
            sticky_for_entities: true,
            ..moved_block(
                position,
                extending,
                Aabb::new(
                    Vec3::new(0.0625, 0.0, 0.0625),
                    Vec3::new(0.9375, 0.9375, 0.9375),
                ),
            )
        }
    }

    #[test]
    fn a_honey_block_carries_the_player_standing_on_it_from_1_15() {
        // moveStuckEntities: a horizontal honey block move carries an on-ground entity in the box
        // from its top to 1.5 over it, its position over that box, by deltaProgress (1.15
        // PistonBlockEntity.java:163-185, 300-302; 1.21.11 PistonMovingBlockEntity.java:194-213)
        for release in [
            MinecraftVersion::new(1, 15, 0),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 19, 4),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = standing_player(0.5);
            push(&mut state, honey_block((1, 100, 0), true), release);
            assert_eq!(state.position.x, 1.0, "{release}");
            assert_eq!(state.position.y, 101.0, "{release}");
            // Entity.move(PISTON) with no vertical move leaves the ground
            assert!(!state.on_ground, "{release}");
        }

        // honey came in 1.15: before it nothing carries
        let mut state = standing_player(0.5);
        push(
            &mut state,
            honey_block((1, 100, 0), true),
            MinecraftVersion::new(1, 14, 4),
        );
        assert_eq!(state.position.x, 0.5);
    }

    #[test]
    fn a_honey_block_carries_only_an_on_ground_player_over_it_moving_horizontally() {
        let release = MinecraftVersion::new(1, 21, 11);
        let mut airborne = SimulationState {
            on_ground: false,
            ..standing_player(0.5)
        };
        push(&mut airborne, honey_block((1, 100, 0), true), release);
        assert_eq!(airborne.position.x, 0.5);

        // the position past the box's side
        let mut beside = standing_player(-0.2);
        push(&mut beside, honey_block((1, 100, 0), true), release);
        assert_eq!(beside.position.x, -0.2);

        // a vertical move carries nothing
        let mut above = standing_player(0.5);
        let rising = MovingPiston {
            facing: 1,
            ..honey_block((0, 100, 0), true)
        };
        push(&mut above, rising, release);
        assert_eq!(above.position, standing_player(0.5).position);
    }

    #[test]
    fn from_1_20_the_honey_block_also_carries_a_player_it_supports() {
        // matchesStickyCritera: isSupportedBy(pos) || the position over the box, and the box
        // reaches 1.5000010000000001 (1.20 PistonBlockEntity.java:195, 205-209); 1.19.4 has
        // neither (1.19.4 PistonBlockEntity.java)
        let supported = |release| {
            let mut state = SimulationState {
                supporting_block: Some((1, 100, 0)),
                supporting_block_kept: true,
                ..standing_player(1.2)
            };
            push(&mut state, honey_block((1, 100, 0), true), release);
            state.position.x
        };
        assert_eq!(supported(MinecraftVersion::new(1, 20, 0)), 1.7);
        assert_eq!(supported(MinecraftVersion::new(1, 19, 4)), 1.2);

        let lifted = |release| {
            let mut state = standing_player(0.5);
            state.position.y = 100.0 + 1.5000005;
            push(&mut state, honey_block((1, 100, 0), true), release);
            state.position.x
        };
        assert_eq!(lifted(MinecraftVersion::new(1, 20, 0)), 1.0);
        assert_eq!(lifted(MinecraftVersion::new(1, 19, 4)), 0.5);
    }

    /// A block a piston above pushes down onto the head of a player standing at (0.5, 101, 0.5).
    fn pressing_block() -> MovingPiston {
        MovingPiston {
            facing: 0,
            ..moved_block(
                (0, 102, 0),
                true,
                Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
            )
        }
    }

    fn floor_of(effect: BlockEffect, bounce_restitution: f64) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 100, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    effect,
                    ..BlockShape::full_block()
                }),
                bounce_restitution,
                ..BlockState::default()
            },
        );
        world
    }

    fn push_in(
        state: &mut SimulationState,
        world: &WorldSnapshot,
        piston: MovingPiston,
        release: MinecraftVersion,
    ) {
        push_by_moving_pistons(
            state,
            world,
            &[piston],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
    }

    #[test]
    fn from_1_21_10_a_piston_move_is_not_slowed_by_the_stuck_speed_multiplier() {
        // Entity.move: `if (moverType != MoverType.PISTON) delta = delta.multiply(stuck...)`, the
        // multiplier and the motion still cleared (1.21.10 Entity.java:693-699, 26.3 :783-789);
        // 1.21.9 slows every move (1.21.9 Entity.java:693-696)
        let out_of_the_way = 0.5 - (0.5 - f64::from(0.3_f32)) + 0.01;
        for (release, moved) in [
            (MinecraftVersion::new(1, 21, 9), out_of_the_way * 0.25),
            (MinecraftVersion::new(1, 21, 10), out_of_the_way),
            (MinecraftVersion::new(26, 3, 0), out_of_the_way),
        ] {
            let mut state = SimulationState {
                stuck_speed_multiplier: Vec3::new(0.25, f64::from(0.05_f32), 0.25),
                velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
                ..standing_player(0.5)
            };
            push(&mut state, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.5 + moved, "{release}");
            assert_eq!(state.stuck_speed_multiplier, Vec3::ZERO, "{release}");
            assert_eq!(state.velocity, Vec3::ZERO, "{release}");
        }
    }

    #[test]
    fn a_piston_pushing_the_player_down_onto_slime_bounces_it() {
        // Entity.move(PISTON) lands the player as its own move does: SlimeBlock.fallOn ends the
        // fall and updateEntityMovementAfterFallOn bounces a falling one back (1.8.9
        // Entity.java:655-666, 1.16.5 :639-651, 1.21.10 :740-758); from 26.2
        // restituteMovementAfterCollisions takes the block's bounciness (26.3 Entity.java:837-840)
        let world = floor_of(BlockEffect::Slime, 1.0);
        for release in [
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = SimulationState {
                velocity: Vec3::new(0.0, -0.5, 0.0),
                fall_distance: 5.0,
                ..standing_player(0.5)
            };
            push_in(&mut state, &world, pressing_block(), release);
            assert_eq!(state.position.y, 101.0, "{release}");
            assert!(state.on_ground, "{release}");
            assert_eq!(state.velocity.y, 0.5, "{release}");
            assert_eq!(state.fall_distance, 0.0, "{release}");
            assert_eq!(state.fall_damage, 0.0, "{release}");
        }

        // a sneaking player lands without a bounce
        let mut state = SimulationState {
            velocity: Vec3::new(0.0, -0.5, 0.0),
            sneaking: true,
            ..standing_player(0.5)
        };
        push_in(
            &mut state,
            &world,
            pressing_block(),
            MinecraftVersion::new(1, 21, 11),
        );
        assert_eq!(state.velocity.y, -0.0);
    }

    #[test]
    fn a_piston_push_counts_into_the_fall_and_its_landing_deals_the_damage() {
        // checkFallDamage after Entity.move(PISTON): the drop adds to the fall distance (1.21.5+
        // `fallDistance -= (double)(float)dy`, 1.21.5 Entity.java:1286), a landing deals
        // floor(fallDistance + 1.0E-6 - 3.0) and ends the fall (1.21.5 LivingEntity.java:1672-1682)
        let release = MinecraftVersion::new(1, 21, 11);
        let top = 101.0 + f64::from(1.8_f32);
        let pushed = top - 102.5 + 0.01;
        let mut airborne = SimulationState {
            on_ground: false,
            fall_distance: 1.0,
            ..standing_player(0.5)
        };
        push(&mut airborne, pressing_block(), release);
        assert_eq!(airborne.position.y, 101.0 - pushed);
        assert!(!airborne.on_ground);
        assert_eq!(airborne.fall_distance, 1.0 + f64::from(pushed as f32));

        let world = floor_of(BlockEffect::None, 0.0);
        let mut landing = SimulationState {
            fall_distance: 5.0,
            safe_fall_distance: 3.0,
            ..standing_player(0.5)
        };
        push_in(&mut landing, &world, pressing_block(), release);
        assert!(landing.on_ground);
        assert_eq!(landing.fall_damage, 2.0);
        assert_eq!(landing.fall_distance, 0.0);
        assert_eq!(landing.velocity.y, -0.0784000015258789 * 0.0);
    }

    #[test]
    fn a_piston_push_into_a_cobweb_sticks_the_player_in_it() {
        // Entity.move runs checkInsideBlocks to 1.21.1 (1.16.5 Entity.java:689); from 1.21.2 the
        // piston applies the blocks of the move after it (1.21.3 PistonBlockEntity.java:189,
        // 1.21.5 PistonMovingBlockEntity.java:190, 26.3 :167)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 101, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for release in [
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(1, 21, 3),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = standing_player(0.5);
            push_in(&mut state, &world, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            assert_eq!(
                state.stuck_speed_multiplier,
                Vec3::new(0.25, f64::from(0.05_f32), 0.25),
                "{release}"
            );
        }
    }

    #[test]
    fn a_piston_push_that_lands_the_player_runs_its_auto_jump() {
        // LocalPlayer.move runs updateAutoJump after Entity.move(PISTON) too (1.21.11
        // LocalPlayer.java:947-955): a player pressed onto the ground walking into a step jumps
        // the next tick. The moving blocks above have no shape for it while they push it
        // (NOCLIP, 1.21.11 PistonMovingBlockEntity.java:358-361), so the ceiling is clear.
        let mut world = floor_of(BlockEffect::None, 0.0);
        world.set_block(
            (0, 101, 1),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let walking = SimulationState {
            auto_jump: true,
            input: Vec3::new(0.0, 0.0, 1.0),
            ..standing_player(0.5)
        };
        for (release, auto_jump_time) in [
            (MinecraftVersion::new(1, 13, 2), 1),
            (MinecraftVersion::new(1, 21, 11), 1),
            // before 1.13 the ceiling check sees the moving block's box (1.12.2
            // ClientPlayerEntity.java:882-887, PistonExtensionBlock.java:144-147; 1.10.2
            // ClientPlayerEntity.java:850-853, PistonExtensionBlock.java:126-135)
            (MinecraftVersion::new(1, 12, 2), 0),
            (MinecraftVersion::new(1, 10, 2), 0),
            // no auto-jump before 1.10
            (MinecraftVersion::new(1, 9, 4), 0),
        ] {
            let mut state = walking;
            push_in(&mut state, &world, pressing_block(), release);
            assert!(state.on_ground, "{release}");
            assert_eq!(state.auto_jump_time, auto_jump_time, "{release}");
        }

        // a push that takes it off the ground does not
        let mut state = walking;
        push_in(
            &mut state,
            &world,
            extending_head(0.0),
            MinecraftVersion::new(1, 21, 11),
        );
        assert!(!state.on_ground);
        assert_eq!(state.auto_jump_time, 0);
    }

    #[test]
    fn on_1_8_a_pushed_block_pushes_only_in_its_own_box() {
        // BlockPistonMoving.getCollisionBox: the moved block's box, its side toward the facing
        // drawn in by what is left to go, none for air (1.8.9 PistonExtensionBlock.java:166-200,
        // PistonBlockEntity.java:85-91): a bottom slab misses a player standing on its top half
        let release = MinecraftVersion::new(1, 8, 8);
        let slab = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
        );
        let mut beside = standing_player(1.2);
        push(&mut beside, slab.clone(), release);
        assert!((beside.position.x - (1.2 + f64::from(0.5_f32 + 0.0625))).abs() < 1e-12);

        let mut above = standing_player(1.2);
        above.position.y = 101.5;
        push(&mut above, slab.clone(), release);
        assert_eq!(above.position.x, 1.2);

        // a block without a box pushes nothing
        let mut state = standing_player(1.2);
        push(
            &mut state,
            MovingPiston {
                shapes: Vec::new(),
                ..slab
            },
            release,
        );
        assert_eq!(state.position.x, 1.2);
    }

    #[test]
    fn on_1_9_and_1_10_the_swept_box_is_the_moved_block_own() {
        // method_11702: pushedBlock.getCollisionBox(BlockView), the head plate for an extending
        // head, the unextended base cube for a retracting one, a slab's outline half (1.9.4
        // PistonBlockEntity.java:79-88; 1.10.2 the same)
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 10, 2),
        ] {
            let slab = moved_block(
                (1, 101, 0),
                true,
                Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
            );
            let mut above = standing_player(1.2);
            above.position.y = 101.5;
            push(&mut above, slab.clone(), release);
            assert_eq!(above.position.x, 1.2, "{release}");
            let mut beside = standing_player(1.2);
            push(&mut beside, slab, release);
            assert_eq!(beside.position.x, 1.8100000119209287, "{release}");

            // a retracting base: its full cube swept from x 0 - 1 to -0.5 - 0.5 pulls a player
            // past its far side
            let mut state = standing_player(-0.75);
            push(&mut state, retracting_base(), release);
            let pulled = (-0.75 + f64::from(0.3_f32)) - (-0.5) + 0.01;
            assert!(
                (state.position.x - (-0.75 - pulled)).abs() < 1e-12,
                "{release}"
            );
        }
    }

    #[test]
    fn on_1_9_and_1_10_a_pushed_block_moves_its_outline_box() {
        // method_11702 takes pushedBlock.getCollisionBox(BlockView, pos), Block's bounding box
        // (the full cube by default), not the World overload's collision box (1.9.4
        // PistonBlockEntity.java:83-88, StateManager.java:380, Block.java:47, 352)
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 10, 2),
        ] {
            // a fence: 1.5 high to collide with, its outline 1.0 (1.9.4 FenceBlock.java:28-31,
            // 46-50): a player standing at 102.2 is over the box it moves
            let fence = MovingPiston {
                shapes: vec![Aabb::new(
                    Vec3::new(0.375, 0.0, 0.375),
                    Vec3::new(0.625, 1.5, 0.625),
                )],
                outline_shapes: vec![Aabb::new(
                    Vec3::new(0.375, 0.0, 0.375),
                    Vec3::new(0.625, 1.0, 0.625),
                )],
                ..moved_block((1, 101, 0), true, Aabb::new(Vec3::ZERO, Vec3::ZERO))
            };
            let mut over = standing_player(1.2);
            over.position.y = 102.2;
            push(&mut over, fence, release);
            assert_eq!(over.position.x, 1.2, "{release}");

            // a rail: no collision box, its 0.125 high outline (1.9.4
            // AbstractRailBlock.java:45, 55) still pushes a player beside it
            let rail = MovingPiston {
                shapes: Vec::new(),
                outline_shapes: vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.125, 1.0))],
                ..moved_block((1, 101, 0), true, Aabb::new(Vec3::ZERO, Vec3::ZERO))
            };
            let mut beside = standing_player(1.2);
            push(&mut beside, rail, release);
            assert_eq!(beside.position.x, 1.8100000119209287, "{release}");

            // soul sand: no outline given is the full cube, though it collides 0.875 high
            // (SoulSandBlock.java:22 overrides only the World overload)
            let soul_sand = MovingPiston {
                shapes: vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0))],
                outline_shapes: Vec::new(),
                ..moved_block((1, 101, 0), true, Aabb::new(Vec3::ZERO, Vec3::ZERO))
            };
            let mut on_top = standing_player(1.2);
            on_top.position.y = 101.9;
            push(&mut on_top, soul_sand, release);
            assert_eq!(on_top.position.x, 1.8100000119209287, "{release}");
        }
    }

    fn powder_snow_at(position: (i32, i32, i32)) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    effect: BlockEffect::PowderSnow,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn a_piston_push_into_powder_snow_sets_in_powder_snow() {
        // PowderSnowBlock.entityInside of the blocks the move went through sets isInPowderSnow,
        // which the next tick keeps as wasInPowderSnow (1.17 PowderSnowBlock.java:70,
        // Entity.java:414-415; 1.21.3 PistonBlockEntity.java:189); from 1.21.5 by FREEZE, which
        // adds a frozen tick for an entity that can freeze (1.21.5 InsideBlockEffectType.java:7-12,
        // PowderSnowBlock.java:94, PistonMovingBlockEntity.java:186-193, Entity.java:465-466)
        let world = powder_snow_at((1, 101, 0));
        for (release, in_powder_snow, ticks_frozen) in [
            (MinecraftVersion::new(1, 16, 5), false, 0),
            (MinecraftVersion::new(1, 17, 1), true, 0),
            (MinecraftVersion::new(1, 21, 1), true, 0),
            (MinecraftVersion::new(1, 21, 3), true, 0),
            (MinecraftVersion::new(1, 21, 5), true, 1),
            (MinecraftVersion::new(1, 21, 11), true, 1),
            (MinecraftVersion::new(26, 3, 0), true, 1),
        ] {
            let mut state = standing_player(0.5);
            push_in(&mut state, &world, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            assert_eq!(state.in_powder_snow, in_powder_snow, "{release}");
            assert_eq!(state.ticks_frozen, ticks_frozen, "{release}");
        }

        // leather boots: in it, but not freezing (Entity.canFreeze)
        let mut state = SimulationState {
            powder_snow_immune: true,
            ..standing_player(0.5)
        };
        push_in(
            &mut state,
            &world,
            extending_head(0.0),
            MinecraftVersion::new(1, 21, 11),
        );
        assert!(state.in_powder_snow);
        assert_eq!(state.ticks_frozen, 0);
    }

    #[test]
    fn from_26_2_a_sideways_push_that_steps_up_keeps_the_vertical_motion() {
        // restituteMovementAfterCollisions runs only for `movedVertically && verticalCollision
        // || horizontalCollision` (26.2 Entity.java:793, 815; 26.3 :818, 837): the step up of a
        // move with no y leaves the motion; before 26.2 the vertical collision lands it, the
        // motion times 0 (1.21.11 Entity.java:768-773)
        let mut world = floor_of(BlockEffect::None, 0.0);
        world.set_block(
            (1, 101, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for (release, velocity_y) in [
            (MinecraftVersion::new(1, 21, 11), -0.0_f64),
            (MinecraftVersion::new(26, 1, 0), -0.0),
            (MinecraftVersion::new(26, 2, 0), -0.0784000015258789),
            (MinecraftVersion::new(26, 3, 0), -0.0784000015258789),
        ] {
            let mut state = standing_player(0.5);
            push_in(&mut state, &world, extending_head(0.0), release);
            assert_eq!(state.position.y, 101.5, "{release}");
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            assert_eq!(
                state.velocity.y.to_bits(),
                velocity_y.to_bits(),
                "{release}"
            );
        }
    }
}
