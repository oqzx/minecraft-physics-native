use crate::block_traversal::Movement;
use crate::math::{Aabb, Vec3};
use crate::simulation::{
    apply_block_effects, apply_inside_fire_effects, block_below_affecting_movement,
    block_jump_factor, block_position, blocks_inside_moves, can_simulate_movement, entity_move,
    kept_supporting_block, move_without_physics, movement_speed_attribute, recorded_movement,
    update_auto_jump,
    InsideEffects, LandingDamageRounding, FallDamageRules, MoverType, PhysicsConfig,
    SimulationState, VersionPhysicsRules,
};
use crate::vehicle::{apply_effects_inside, move_vehicle, MoveRules};
use crate::version::MinecraftVersion;
use crate::world::{BlockEffect, WorldPatches, WorldSnapshot};

/// Direction.from3DDataValue: down, up, north, south, west, east.
const DIRECTIONS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

/// How what a piston pushes moves itself.
#[derive(Clone, Copy)]
enum Mover {
    Player(PhysicsConfig),
    Body(MoveRules),
}

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
    /// `shapes` once it has gone on this tick, for the pistons that tick after it: a head is short
    /// by its progress (26.3 PistonMovingBlockEntity.java:340-344). None: the same.
    pub next_shapes: Option<Vec<Aabb>>,
    /// The outline boxes of a block it pushes in its cell, whose bounds push on 1.9 - 1.10
    /// (IBlockState.getCollisionBox(BlockView), MCP's getBoundingBox); none: the full cube,
    /// Block.collisionBox's default (1.9.4 Block.java:83, 388).
    pub outline_shapes: Vec<Aabb>,
    /// The block it moves is a slime block: the player it meets takes the motion of the way it
    /// moves along that axis (causeBounce).
    pub bounces: bool,
    /// The block it moves is a honey block: a horizontal move carries the player standing on it
    /// (isStickyForEntities, 1.15+).
    pub sticky_for_entities: bool,
}

impl MovingPiston {
    /// The facing's Direction#getStepX, Direction#getStepY and Direction#getStepZ.
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
    /// it moves on 1.9 - 1.10 (1.9.4 PistonMovingBlockEntity.java:84-89, Block.getCollisionBox(BlockState,
    /// BlockView, BlockPos) at Block.java:388, StateDefinition.java:372).
    fn outline_bounds(&self) -> Aabb {
        let Some(&first) = self.outline_shapes.first() else {
            return Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        };
        self.outline_shapes
            .iter()
            .fold(first, |bounds, shape| bounds.minmax(*shape))
    }

    /// PistonMovingBlockEntity#getMovementDirection: the facing while it extends, the opposite while it retracts.
    pub(crate) fn movement_direction(&self) -> (i32, i32, i32) {
        let (fx, fy, fz) = self.facing_step();
        let sign = if self.extending { 1 } else { -1 };
        (fx * sign, fy * sign, fz * sign)
    }

    /// PistonMovingBlockEntity#moveByPositionAndProgress: where its cell's corner is at the progress before this tick.
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

/// The collision boxes of a moving block in its cell, the block entity's shape, which the player's
/// move meets before the block entity ticks. `piston.shapes` are the boxes of the block at this
/// progress: the head a source piston shows, else the block it moves; `base_shapes` those of the
/// extended base a source piston leaves as it retracts.
///
/// - 1.8: one box whose far side is drawn back by the progress before the last tick, flat for a
///   full block that has yet to enter its cell, and turned over for a shorter one, as Box orders
///   its corners (1.8.8 MovingPistonBlock.java:125-137, 169-200: getAmountExtended(0.0F) is
///   lastProgress).
/// - 1.9 - 1.10: the outline box moved to the progress and to the progress before it, joined in
///   one box (1.9.4 PistonMovingBlockEntity.java method_11701, method_11702).
/// - From 1.11: the shape moved to the progress, and the base (1.12.2 PistonMovingBlockEntity.java:322-350,
///   1.14 getCollisionShape, 26.3 PistonMovingBlockEntity.java:327-352).
#[must_use]
pub fn moving_block_collision_boxes(
    piston: &MovingPiston,
    base_shapes: &[Aabb],
    release: MinecraftVersion,
) -> Vec<Aabb> {
    let (fx, fy, fz) = piston.facing_step();
    let step = Vec3::new(f64::from(fx), f64::from(fy), f64::from(fz));
    let extended = |progress: f32| {
        f64::from(if piston.extending { progress - 1.0 } else { 1.0 - progress })
    };
    // (tick: lastProgress = progress, then progress + 0.5F held at 1.0F, from 0.0F)
    let last_progress = (piston.progress - 0.5).max(0.0);
    // (1.8.8 MovingPistonBlock.java:125-137,169-204 (box stretched back by progressO); 1.9.4 PistonMovingBlockEntity.java:80-89, MovingPistonBlock.java:128-131 (box moved by progress, minmax with progressO))
    if !release.at_least(1, 9, 0) {
        let Some(bounds) = piston.shape_bounds() else {
            return Vec::new();
        };
        let left = f64::from(if piston.extending { 1.0 - last_progress } else { last_progress });
        let (mut min, mut max) = (bounds.min, bounds.max);
        let stretch = |step: f64, min: &mut f64, max: &mut f64| {
            if step < 0.0 {
                *min -= step * left;
            } else {
                *max -= step * left;
            }
        };
        stretch(step.x, &mut min.x, &mut max.x);
        stretch(step.y, &mut min.y, &mut max.y);
        stretch(step.z, &mut min.z, &mut max.z);
        let ordered = |a: f64, b: f64| (a.min(b), a.max(b));
        let (min_x, max_x) = ordered(min.x, max.x);
        let (min_y, max_y) = ordered(min.y, max.y);
        let (min_z, max_z) = ordered(min.z, max.z);
        return vec![Aabb::new(Vec3::new(min_x, min_y, min_z), Vec3::new(max_x, max_y, max_z))];
    }
    // (1.10.2 PistonMovingBlockEntity.java:81-90 (one box, minmax of progress and progressO); 1.11.2 PistonMovingBlockEntity.java:322-350, MovingPistonBlock.java:137-142 (the block's boxes moved by the progress))
    if !release.at_least(1, 11, 0) {
        let bounds = legacy_moved_box(piston);
        let moved = |progress: f32| {
            let by = extended(progress);
            bounds.translated(Vec3::new(step.x * by, step.y * by, step.z * by))
        };
        return vec![moved(piston.progress).minmax(moved(last_progress))];
    }
    let by = extended(piston.progress);
    let moved = Vec3::new(step.x * by, step.y * by, step.z * by);
    base_shapes
        .iter()
        .copied()
        .chain(piston.shapes.iter().map(|shape| shape.translated(moved)))
        .collect()
}

/// MovingPistonBlock.getCollisionShape in the world: the block at the piston's cell takes the boxes
/// of its block entity. From 1.11 a push along the way it goes passes through it until it ends,
/// save for the base a retracting piston leaves (1.10.2 PistonMovingBlockEntity.java has no NOCLIP;
/// 1.11.2 :322-329, 1.13.2 :326-338, 26.3 PistonMovingBlockEntity.java:327-338).
pub fn set_moving_block(
    world: &mut WorldSnapshot,
    piston: &MovingPiston,
    base_shapes: &[Aabb],
    release: MinecraftVersion,
) -> Vec<Aabb> {
    let boxes = moving_block_collision_boxes(piston, base_shapes, release);
    world.set_moving_block(piston.position, &boxes, reads_edge_cells(release));
    if release.at_least(1, 11, 0) && piston.progress < 1.0 {
        world.set_moving_block_noclip(piston.position, piston.movement_direction(), base_shapes);
    }
    boxes
}

/// The pushes of the moving piston blocks, in the order the client ticks them: each goes 0.5F
/// further this tick and moves the player out of its way. A spectator is not among the entities
/// asked (Level.getEntities' NO_SPECTATORS, 1.8.8 Level.java:2173-2175, and no release tests
/// noPhysics here: 1.20.4 PistonMovingBlockEntity.java:128, 26.3 PistonMovingBlockEntity.java:122), and a
/// player has noPhysics only as one (1.8.8 Player.java:265, 26.3 Player.java:218); any other
/// entity with noPhysics is pushed, through what is in its way. The pistons of the tick before
/// these moved the player by `piston_deltas` on each axis (Entity.pistonDeltas), which are left
/// with these pushes added. One that has ticked stands at its new progress for the ones after it
/// (26.3 PistonMovingBlockEntity.java tick sets progress as it goes).
pub fn push_by_moving_pistons(
    state: &mut SimulationState,
    world: &mut WorldSnapshot,
    pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    config: PhysicsConfig,
    release: MinecraftVersion,
) {
    push_in_turn(state, world, pistons, piston_deltas, Mover::Player(config), release);
}

/// The pistons of a tick against an entity that is no player: PistonMovingBlockEntity.moveCollidedEntities
/// and moveStuckEntities ask every entity of the level, whatever its kind, but those of
/// PushReaction.IGNORE_ENTITY (26.3 Entity.java:3757, Marker, Display, Interaction,
/// AreaEffectCloud, OminousItemSpawner, a marker ArmorStand), which the caller does not send. The
/// block entities tick after the entities on either side (26.3 Level.tickBlockEntities), so
/// this follows the entity's own tick. `rules` are the entity's Entity.move's.
pub(crate) fn push_body_by_moving_pistons(
    body: &mut SimulationState,
    world: &mut WorldSnapshot,
    pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    rules: MoveRules,
    release: MinecraftVersion,
) {
    push_in_turn(body, world, pistons, piston_deltas, Mover::Body(rules), release);
}

/// The pistons' writes, of the ticks that went (go_on) and of a NOCLIP move, go into `world` and
/// are taken back before this returns.
fn push_in_turn(
    state: &mut SimulationState,
    world: &mut WorldSnapshot,
    pistons: &[MovingPiston],
    piston_deltas: &mut [f64; 3],
    mover: Mover,
    release: MinecraftVersion,
) {
    if state.no_physics && state.is_player {
        return;
    }
    let mut ticked = WorldPatches::default();
    let mut through = WorldPatches::default();
    for (index, piston) in pistons.iter().enumerate() {
        // (1.8.8 PistonMovingBlockEntity.java:75-108,123-143 (only extending pushes, by the step + 0.0625, 0.25 at the end; extending slime sets motion instead); 1.9.4 PistonMovingBlockEntity.java:91-150,165-183 (every tick, out of the swept box + 0.01; slime sets motion and pushes))
        if !release.at_least(1, 9, 0) {
            push_before_1_9(state, world, piston, mover, release);
        // (1.10.2 PistonMovingBlockEntity.java:92-151,167-185 (swept-box push, the end tick pushes too); 1.11.2 PistonMovingBlockEntity.java:118-171,235-249,276-294 (per-box movement areas, PISTON move under NOCLIP, base fix, no push at the end))
        } else if !release.at_least(1, 11, 0) {
            push_before_1_11(state, world, piston, mover, release);
        } else {
            // the moves under moveEntityByPiston's and fixEntityWithinPistonBase's NOCLIP (1.11.2
            // PistonMovingBlockEntity.java:160-162, 244-246; 26.3 PistonMovingBlockEntity.java:163-170)
            world.through_moving_blocks(&mut through, piston.movement_direction());
            push(state, world, piston, piston_deltas, mover, release);
            carry(state, world, piston, piston_deltas, mover, release);
            world.restore(&mut through);
        }
        // (to 1.10 the tick that ends a block pushes too: 1.8.9, 1.9.4 PistonMovingBlockEntity.java tick)
        if index + 1 < pistons.len() && (piston.progress < 1.0 || !release.at_least(1, 11, 0)) {
            go_on(world, &mut ticked, piston, release);
        }
    }
    world.restore(&mut ticked);
}

/// The cell of a moving block as its tick left it, for the blocks that tick after it: the end of
/// PistonMovingBlockEntity#tick, which steps the progress by 0.5 or, once it reached 1, puts the
/// moved block in its place.
fn go_on(
    world: &mut WorldSnapshot,
    patches: &mut WorldPatches,
    piston: &MovingPiston,
    release: MinecraftVersion,
) {
    if !world.is_moving_block(piston.position) {
        return;
    }
    patches.save(world, piston.position);
    if piston.progress >= 1.0 {
        // its block entity is gone, and the block it moved stands there
        world.set_moving_block(piston.position, &piston.shapes, reads_edge_cells(release));
        return;
    }
    let base = world.moving_block_base(piston.position);
    let after = MovingPiston {
        progress: (piston.progress + 0.5).min(1.0),
        shapes: piston.next_shapes.clone().unwrap_or_else(|| piston.shapes.clone()),
        ..piston.clone()
    };
    set_moving_block(world, &after, &base, release);
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
    mover: Mover,
    release: MinecraftVersion,
) {
    if piston.progress >= 1.0 {
        return;
    }
    // 1.16.2 moveCollidedEntities skips the whole ServerPlayer candidate for slime, not
    // just its impulse (exact 1.16.1/1.16.2 bytecode; 26.2 PistonMovingBlockEntity.java:136).
    // 26.3 replaces that continue with a bounce-only canSimulateMovement check.
    // (1.16.1 PistonMovingBlockEntity.java:131 sends a ServerPlayer its motion and moves it on; 1.16.2
    // PistonMovingBlockEntity.java:132 leaves a ServerPlayer under slime in the inner loop, which goes on to the next)
    if piston.bounces
        && state.is_player
        && !state.client_side
        && release.at_least(1, 16, 2)
        && !release.at_least(26, 3, 0)
    {
        return;
    }
    let progress = piston.progress;
    let distance = f64::from((progress + 0.5) - progress);
    let direction = piston.movement_direction();
    let origin = piston.origin();
    let bounds = state.bounds();
    // Only the bounce is conditional in 26.3; a remote entity still receives the piston move
    // and its callbacks (PistonMovingBlockEntity.java:122-170, MoverType.java:13-14).
    let simulates = !release.at_least(26, 3, 0) || can_simulate_movement(state, release);
    if let Some(shape_bounds) = piston.shape_bounds().filter(|_| piston.bounces && simulates) {
        // the entities of getEntities(movementArea.minmax(box)), the box the shape's bounds, from
        // the cell's corners outward before 1.16 (1.15 PistonMovingBlockEntity.java:102-127, 194-212;
        // 1.16 :104-130; 26.3 PistonMovingBlockEntity.java:110-150)
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
        mover,
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
            mover,
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
    mover: Mover,
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
    let shape = legacy_moved_box(piston);
    let (x, y, z) = piston.position;
    // the box offset by the float progress times the step, then by the position (1.9.4
    // PistonMovingBlockEntity.java:84-92)
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
        // PistonMovingBlockEntity.java:100-137)
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
    // moveEntity before 1.11 takes no MoverType: every call backs off as SELF does (1.10.2
    // Entity.java:574)
    move_by(state, world, movement, mover, release, MoverType::SelfOrPlayer);
}

/// TileEntityPiston.update on 1.8 (1.8.8 PistonMovingBlockEntity.java:126-140): only an extending block pushes while it moves, by the
/// progress it went plus 0.0625F along the piston's facing, and the tick that ends a block
/// pushes by 0.25F (an extending slime block sets the motion instead); the player moves when its
/// box meets the block's (BlockPistonMoving: the cell, or a pushed block's own box, with the side
/// the facing points to drawn in by what is left to go), by its own Entity.move.
fn push_before_1_9(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    piston: &MovingPiston,
    mover: Mover,
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
        // a block it pushes moves its own box, none without one (1.8.8
        // MovingPistonBlock.java:169-203)
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
    // new Box(...) orders each axis, so a box drawn in past its far side turns over rather than
    // empties: a thin one, or one off its cell's face (1.8.9 PistonExtensionBlock.java:165-197,
    // Box.java:22-29)
    let area = Aabb::new(
        Vec3::new(area.min.x.min(area.max.x), area.min.y.min(area.max.y), area.min.z.min(area.max.z)),
        Vec3::new(area.min.x.max(area.max.x), area.min.y.max(area.max.y), area.min.z.max(area.max.z)),
    );
    if !area.intersects(state.bounds()) {
        return;
    }
    if piston.bounces && piston.extending {
        // an extending slime block sets the motion in place of the move (1.8.8
        // PistonMovingBlockEntity.java:89-102)
        bounce(state, (fx, fy, fz));
        return;
    }
    let movement = Vec3::new(
        f64::from(distance * fx as f32),
        f64::from(distance * fy as f32),
        f64::from(distance * fz as f32),
    );
    // moveEntity before 1.11 takes no MoverType: every call backs off as SELF does (1.10.2
    // Entity.java:574)
    move_by(state, world, movement, mover, release, MoverType::SelfOrPlayer);
}

/// PistonMovingBlockEntity.moveStuckEntities (1.15+): a honey block that moves horizontally
/// carries a player on the ground in the box from the block's top to 1.5 over its cell, placed
/// at the progress, with its position over that box (from 1.20 also one the block supports), by
/// the whole distance the block goes this tick (1.15 PistonMovingBlockEntity.java:162-188; 1.20
/// :186-205; 26.3 PistonMovingBlockEntity.java:172-190).
fn carry(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    piston: &MovingPiston,
    piston_deltas: &mut [f64; 3],
    mover: Mover,
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
    // (1.19.4 PistonMovingBlockEntity.java:191,201-205 (1.5000000999999998, position over the box); 1.20 PistonMovingBlockEntity.java:191,201-205 (1.5000010000000001, or isSupportedBy))
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
    // (1.19.4 PistonMovingBlockEntity.java:191,201-205 (1.5000000999999998, position over the box); 1.20 PistonMovingBlockEntity.java:191,201-205 (1.5000010000000001, or isSupportedBy))
    let supported = release.at_least(1, 20, 0)
        && kept_supporting_block(world, state, None, release) == Some(piston.position);
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
        mover,
        release,
    );
}

/// A slime block's bounce: the motion along the axis of `direction` becomes its step (the
/// switch on the axis in PistonMovingBlockEntity#moveCollidedEntities).
fn bounce(state: &mut SimulationState, direction: (i32, i32, i32)) {
    match direction {
        (x, 0, 0) if x != 0 => state.velocity.x = f64::from(x),
        (0, y, 0) if y != 0 => state.velocity.y = f64::from(y),
        (_, _, z) => state.velocity.z = f64::from(z),
    }
}

/// Entity.limitPistonMovement: at most 0.51 along each axis in a game tick, along the first axis the
/// movement has. None: Entity.move stops here, on a movement that is Vec3.ZERO by Vec3.equals, whose
/// Double.compare tells -0.0 from 0.0 (1.14.4 Entity.java:476-480, 26.3 :774-779).
pub(crate) fn limit_piston_movement(
    state: &SimulationState,
    movement: Vec3,
    piston_deltas: &mut [f64; 3],
    release: MinecraftVersion,
) -> Option<Vec3> {
    // (limitPistonMovement is on Entity.move's way past noPhysics, 26.3 Entity.java:766-775)
    let unlimited = state.no_physics
        || (release.at_least(1, 14, 0) && movement.length_squared() <= 1.0e-7);
    let movement = if unlimited {
        movement
    } else {
        let components = [movement.x, movement.y, movement.z];
        let axis = components.iter().position(|value| *value != 0.0)?;
        let limited = (components[axis] + piston_deltas[axis]).clamp(-0.51, 0.51);
        let amount = limited - piston_deltas[axis];
        piston_deltas[axis] = limited;
        if amount.abs() <= 9.999999747378752e-6 {
            return None;
        }
        let mut limited_movement = [0.0; 3];
        limited_movement[axis] = amount;
        Vec3::new(
            limited_movement[0],
            limited_movement[1],
            limited_movement[2],
        )
    };
    let zero = [movement.x, movement.y, movement.z].iter().all(|value| value.to_bits() == 0);
    (state.no_physics || !zero).then_some(movement)
}

/// Entity.move(PISTON) and what PistonMovingBlockEntity.moveEntityByPiston runs after it.
fn move_by_piston(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    piston_deltas: &mut [f64; 3],
    mover: Mover,
    release: MinecraftVersion,
) {
    let start = state.position;
    // a move limited to nothing returns from Entity.move, but from 1.21.2 moveEntityByPiston runs
    // applyEffectsFromBlocks after it whatever the move did (1.21.2 PistonMovingBlockEntity.java:
    // 176-181, 1.21.5 :178-185, 26.3 :163-170 and Entity.java:774-778), at the block the last move
    // left the entity on
    let support = match limit_piston_movement(state, movement, piston_deltas, release) {
        Some(movement) => move_by(state, world, movement, mover, release, MoverType::Piston),
        None if release.at_least(1, 21, 2) => kept_supporting_block(world, state, None, release),
        None => return,
    };
    // (1.21.1 PistonMovingBlockEntity.java:175-179, Entity.java:738,791 (effects inside Entity.move); 1.21.2 PistonMovingBlockEntity.java:179, Entity.java:776-810 (applyEffectsFromBlocks after the move))
    if release.at_least(1, 21, 2) {
        // the effects of the blocks the move went through: from 1.21.5 moveEntityByPiston's
        // applyEffectsFromBlocks(previousPos, position()) of this move alone (1.21.5
        // PistonMovingBlockEntity.java:178-185, 26.3 :163-170); 1.21.2 - 1.21.4 tickBlockCollision()
        // from getLastRenderPos, where the tick started (1.21.3 PistonMovingBlockEntity.java:176-181,
        // Entity.java:776-793)
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
        let (inside, effects) = blocks_inside_moves(state, world, &movements, release);
        apply_block_effects(state, world, support, release, &inside);
        apply_powder_snow(state, world, &inside, &effects, release);
    }
}

/// The fire test of a piston's or shulker box's Entity.move before 1.17, the move's own as the
/// tick's (1.12.2 Entity.java:876-886, 1.15.2 :631-653). PowderSnowBlock.entityInside of the
/// blocks a piston's move went through, from 1.17: it sets
/// Entity.isInPowderSnow, which the next tick's baseTick keeps as wasInPowderSnow (1.17
/// PowderSnowBlock.java:74, Entity.java:416-417; 1.21.5 Entity.java:492-493); from 1.21.5 by
/// each step's InsideBlockEffectType.FREEZE, which adds a frozen tick too. The move's fire and
/// lava ignite, extinguish and clear the freeze as the tick's own do (apply_inside_fire_effects:
/// the move's checkBlockCollision before 1.21.2, its applyEffectsFromBlocks from 1.21.5, 1.21.5
/// PistonMovingBlockEntity.java:178-185).
pub(crate) fn apply_powder_snow(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    inside: &[((i32, i32, i32), bool)],
    effects: &InsideEffects,
    release: MinecraftVersion,
) {
    if state.no_physics {
        return;
    }
    if release.at_least(1, 17, 0)
        && !release.at_least(1, 21, 5)
        && inside
            .iter()
            .any(|&(position, _)| world.effect_at(position) == BlockEffect::PowderSnow)
    {
        state.in_powder_snow = true;
    }
    apply_inside_fire_effects(state, world, inside, effects, release);
}

fn move_by(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    mover: Mover,
    release: MinecraftVersion,
    mover_type: MoverType,
) -> Option<(i32, i32, i32)> {
    match mover {
        Mover::Player(config) => move_entity(state, world, movement, config, release, mover_type),
        Mover::Body(rules) => move_body(state, world, movement, rules, release, mover_type),
    }
}

/// Entity.move of what is no player, as move_entity has it: all of the move under noPhysics
/// (26.3 Entity.java:766-775), a PISTON move unslowed by the stuck multiplier it still uses up from
/// 1.21.10 (1.21.9 Entity.java:729-732, 1.21.10 :729-735), the flags and support kept on a
/// level move of what the local instance does not own from 1.21.4 (1.21.4 Entity.java:682-686,
/// 26.3 :818-823), and no restitution where it cannot simulate from 26.2 (26.2 Entity.java:793, 815;
/// 26.3 :818, 837).
/// The collision walk reads a moving block in the cells on two edges of the box: 1.14.4
/// LevelReader.java:190 and Shapes.java:239, 1.15 CollisionGetter.java:106. 1.13.2 has no such test.
const fn reads_edge_cells(release: MinecraftVersion) -> bool {
    release.at_least(1, 14, 4)
}

fn move_body(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    rules: MoveRules,
    release: MinecraftVersion,
    mover: MoverType,
) -> Option<(i32, i32, i32)> {
    if state.no_physics {
        move_without_physics(state, movement, release);
        return kept_supporting_block(world, state, None, release);
    }
    if mover == MoverType::Piston
        // (1.21.9 Entity.java:729-733 (stuck multiplier on every move); 1.21.10 Entity.java:729-735 (not on a PISTON move, still reset))
        && release.at_least(1, 21, 10)
        && state.stuck_speed_multiplier.length_squared() > 1.0e-7
    {
        state.stuck_speed_multiplier = Vec3::ZERO;
        state.velocity = Vec3::ZERO;
    }
    // (from 26.2 only the side that simulates restitutes; the block's speed factor comes to all,
    // 26.2 Entity.java:815, 827); checkFallDamage at its place in the move
    let rules = MoveRules {
        skips_restitution: release.at_least(26, 2, 0) && !can_simulate_movement(state, release),
        counts_fall: true,
        ..rules
    };
    let outcome = move_vehicle(state, world, release, movement, rules, |body, support| {
        apply_effects_inside(body, world, support, release);
    });
    outcome.support
}

/// Entity.move of the player by a movement it did not make itself: slowed by the stuck speed
/// multiplier (not a piston's from 1.21.10), collided as its own move (stepping up), the
/// collision flags and the ground from it, the fall checked (checkFallDamage), the motion stopped
/// on the axes it hit or bounced off the block it lands on, before 1.21.2 the blocks it ends in
/// (stepOn, checkInsideBlocks), from 1.15 the motion scaled by the block speed factor, its inLava
/// from the blocks it ends in 1.14 - 1.16.1, and LocalPlayer.updateAutoJump after it. Before 1.11
/// (moveEntity, MoverType::SelfOrPlayer) a sneaking player backs off from edges too. The block the
/// player stands on after it is returned.
pub(crate) fn move_entity(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    movement: Vec3,
    config: PhysicsConfig,
    release: MinecraftVersion,
    mover: MoverType,
) -> Option<(i32, i32, i32)> {
    // Entity.move(PISTON) steps with maxUpStep(), the STEP_HEIGHT attribute on a LivingEntity from
    // 1.20.5, which reads through RangedAttribute.sanitizeValue as the tick's does: NaN is 0, the
    // rest clamped to 0..10 (1.21.11 RangedAttribute.java:30-32, Attributes.java:42-80).
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
    let support = kept_supporting_block(world, state, None, release);
    // Entity.move's checkFallDamage and the landing block's updateEntityMovementAfterFallOn (a
    // slime block's or a bed's bounce), restituteMovementAfterCollisions from 26.2, as the tick's
    // own move (1.8.8 Entity.java:654-665, 1.16.5 :642-654, 1.21.10 :775-793, 26.3 :827-840); the
    // profiles' rounding, attribute damage and restitution go by release: Mth.ceil to 1.21.4, Mth.floor
    // from 1.21.5 (1.21.4 LivingEntity.java:1652, 1.21.5 :1640); the SAFE_FALL_DISTANCE attribute from
    // 1.20.5 (1.20.4 LivingEntity.java:1569, 1.20.5 :1600); restitution from 26.2 (26.1.2
    // Entity.java:794 updateEntityMovementAfterFallOn, 26.2 :794 restituteMovementAfterCollisions)
    let fall = FallDamageRules {
        landing_damage_rounding: if release.at_least(1, 21, 5) {
            LandingDamageRounding::Floor
        } else {
            LandingDamageRounding::Ceil
        },
        restitution_enabled: release.at_least(26, 2, 0),
        modern_fall_damage: release.at_least(1, 20, 5),
    };
    let moved = entity_move(state, world, movement, mover, rules, fall, support);
    let support = moved.support;
    if mover == MoverType::ShulkerBox {
        hold_pending_movement(state, start, moved.desired, moved.displacement, release);
    }
    // the move's fire test (before 1.17), and its powder snow and fire from 1.17, after its
    // isRemoved exit (1.15.2 Entity.java:631-653; 1.17 Entity.java:568, 630-648); from 1.21.2 the
    // piston applies them after the move (1.21.1 Entity.java:714 tryCheckInsideBlocks in move; 1.21.2
    // Entity.java:776-810 applyEffectsFromBlocks)
    if !release.at_least(1, 21, 2) && !moved.removed {
        apply_powder_snow(state, world, &moved.inside, &InsideEffects::default(), release);
    }
    // LocalPlayer.move runs updateAutoJump after every move, a piston's too (1.10.2
    // LocalPlayer.java:807-812, 1.21.11 LocalPlayer.java:912-920, 26.3 :977-985), with
    // getSpeed(), the movement speed the tick took. Before 1.13 its ceiling check sees the moving
    // block's box, the tile entity's swept box whatever it moves (1.12.2
    // LocalPlayer.java:879-884, MovingPistonBlock.java:146-149; 1.10.2
    // MovingPistonBlock.java:129-138), which the engine's world does not hold: the block that
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
        block_position(state.position, release),
        block_below_affecting_movement(world, state, support, release),
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

/// A shulker box's move stays in the entity's movementThisTick from 1.21.5: Entity.move records it
/// and the box applies nothing (26.3 Entity.java:807, ShulkerBoxBlockEntity.java:133); only the
/// entity's own applyEffectsFromBlocks takes it (26.3 Entity.java:960-963). A piston's takes its
/// own back (1.21.5 PistonMovingBlockEntity.java:182-183, 26.3 :167-168), and before 1.21.5 no
/// move is recorded (1.21.4 Entity.java:788-802). A second push before that tick is a batch of its
/// own after the first (1.21.5 Entity.java:720, applyEffectsFromBlocks flattening the batches in
/// order, :811-815).
pub(crate) fn hold_pending_movement(state: &mut SimulationState, from: Vec3, requested: Vec3, moved: Vec3, release: MinecraftVersion) {
    let Some(movement) = recorded_movement(state, from, requested, moved, release) else {
        return;
    };
    state.pending_movements.push(movement);
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

/// The box TileEntityPiston moves on 1.9 - 1.10: pushedBlock.getCollisionBox(BlockView, pos), the
/// outline box of a block it pushes, not its collision boxes, the head's plate alone, and the
/// full cube of a retracting base (1.9.4 PistonMovingBlockEntity.java:84-89, PistonHeadBlock.java:26-31).
fn legacy_moved_box(piston: &MovingPiston) -> Aabb {
    let (fx, fy, fz) = piston.facing_step();
    if !piston.source {
        return piston.outline_bounds();
    }
    if !piston.extending {
        return Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    }
    let axis = if fx != 0 {
        0
    } else if fy != 0 {
        1
    } else {
        2
    };
    head_plate(axis, fx + fy + fz)
}

/// PistonHeadBlock's box before 1.11: the plate on the side its facing points to (PistonHeadBlock#getShape,
/// its EAST_AABB to DOWN_AABB in 1.9.4 - 1.10.2).
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

/// The movement of PistonMovingBlockEntity#moveEntityByPiston: `amount` times the direction's steps.
fn along(direction: (i32, i32, i32), amount: f64) -> Vec3 {
    Vec3::new(
        amount * f64::from(direction.0),
        amount * f64::from(direction.1),
        amount * f64::from(direction.2),
    )
}

/// Vec3#get of an axis, by its index.
fn component(vector: Vec3, axis: usize) -> f64 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        moving_block_collision_boxes, push_body_by_moving_pistons, push_by_moving_pistons,
        set_moving_block, MovingPiston,
    };
    use crate::vehicle::MoveRules;
    use crate::math::{Aabb, Vec3};
    use crate::simulation::{PhysicsConfig, SimulationState};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

    const WEST: u8 = 4;
    const EAST: u8 = 5;
    const UP: u8 = 1;

    fn cube() -> Aabb {
        Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))
    }

    /// An item's body just east of the cell (0, 100, 0), the side a pushed block comes from.
    fn item_at(x: f64, y: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(x, y, 0.5),
            half_width: 0.125,
            height: 0.25,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        }
    }

    fn pushed_body(
        mut body: SimulationState,
        world: &WorldSnapshot,
        piston: MovingPiston,
        release: MinecraftVersion,
    ) -> (SimulationState, [f64; 3]) {
        let mut deltas = [0.0; 3];
        let mut pushed = world.clone();
        push_body_by_moving_pistons(&mut body, &mut pushed, &[piston], &mut deltas, MoveRules::MINECART, release);
        assert_eq!(pushed.encode(), world.encode(), "the push leaves the world as it was");
        (body, deltas)
    }

    #[test]
    fn a_piston_moves_what_is_no_player_out_of_its_way_by_its_own_move() {
        // 26.3 PistonMovingBlockEntity.java:110-150: every entity of the level's getEntities, by the
        // same box and the same 0.01 over it
        let release = MinecraftVersion::new(26, 3, 0);
        let world = WorldSnapshot::default();
        let (body, deltas) = pushed_body(item_at(0.3, 100.0), &world, moving(EAST, true, 0.0, cube()), release);
        // the face comes to x=0.5, which is 0.325 past the body's side at 0.175
        assert!((body.position.x - (0.3 + 0.325 + 0.01)).abs() < 1.0e-9);
        assert!(deltas[0] > 0.0);
        assert_eq!(body.velocity, Vec3::ZERO);
    }

    #[test]
    fn slime_throws_a_body_and_honey_carries_one_as_a_player() {
        let release = MinecraftVersion::new(26, 3, 0);
        let world = WorldSnapshot::default();
        let mut slime = moving(EAST, true, 0.0, cube());
        slime.bounces = true;
        let (thrown, _) = pushed_body(item_at(0.3, 100.0), &world, slime, release);
        assert_eq!(thrown.velocity.x, 1.0);
        // a body on top of a honey block moves with it by the progress alone (moveStuckEntities)
        let mut honey = moving(EAST, true, 0.0, cube());
        honey.sticky_for_entities = true;
        let mut on_top = item_at(-0.5, 101.0);
        on_top.on_ground = true;
        let (carried, deltas) = pushed_body(on_top, &world, honey, release);
        assert_eq!(carried.position.x, 0.0);
        assert_eq!(deltas[0], 0.5);
    }

    #[test]
    fn a_no_physics_body_is_pushed_whole_and_a_stuck_body_unslowed_from_1_21_10() {
        // 26.3 Entity.java:766-775 goes round limitPistonMovement; 1.21.10 :729-735 uses the
        // multiplier up on a PISTON move without slowing it, 1.21.9 :729-732 slows it
        let world = WorldSnapshot::default();
        let mut ghost = item_at(0.3, 100.0);
        ghost.no_physics = true;
        let (ghost, deltas) = pushed_body(ghost, &world, moving(EAST, true, 0.0, cube()), MinecraftVersion::new(26, 3, 0));
        assert_eq!(deltas, [0.0; 3]);
        assert!(ghost.position.x > 0.3);
        let stuck = |release| {
            let mut body = item_at(0.3, 100.0);
            body.stuck_speed_multiplier = Vec3::new(0.25, 0.05, 0.25);
            body.velocity = Vec3::new(0.1, 0.0, 0.0);
            pushed_body(body, &world, moving(EAST, true, 0.0, cube()), release).0
        };
        let (old, new) = (stuck(MinecraftVersion::new(1, 21, 9)), stuck(MinecraftVersion::new(1, 21, 10)));
        assert_eq!((old.stuck_speed_multiplier, new.stuck_speed_multiplier), (Vec3::ZERO, Vec3::ZERO));
        assert!(new.position.x > old.position.x);
    }

    #[test]
    fn a_level_push_leaves_the_ground_of_what_the_instance_does_not_own_from_1_21_4() {
        // 1.21.4 Entity.java:706-710, 26.3 :818-823; 1.21.3 takes it from the move
        let world = WorldSnapshot::default();
        let moved = |release, owned| {
            let mut body = item_at(0.3, 100.0);
            body.on_ground = true;
            body.controlled_by_local_instance = owned;
            pushed_body(body, &world, moving(EAST, true, 0.0, cube()), release).0.on_ground
        };
        assert!(moved(MinecraftVersion::new(1, 21, 4), false));
        assert!(!moved(MinecraftVersion::new(1, 21, 4), true));
        assert!(!moved(MinecraftVersion::parse("1.21.3").unwrap(), false));
    }

    fn moving(facing: u8, extending: bool, progress: f32, shape: Aabb) -> MovingPiston {
        MovingPiston {
            position: (0, 100, 0),
            facing,
            extending,
            source: false,
            progress,
            shapes: vec![shape],
            next_shapes: None,
            outline_shapes: vec![shape],
            bounces: false,
            sticky_for_entities: false,
        }
    }

    fn release(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
        MinecraftVersion::new(major, minor, patch)
    }

    #[test]
    fn a_moving_block_is_moved_by_its_progress_from_1_11() {
        // extending: a block behind its cell by what is left of the way (26.3 :346-350)
        for at in [release(1, 11, 2), release(1, 20, 4), release(26, 3, 0)] {
            let boxes = moving_block_collision_boxes(&moving(UP, true, 0.5, cube()), &[], at);
            assert_eq!(boxes, [Aabb::new(Vec3::new(0.0, -0.5, 0.0), Vec3::new(1.0, 0.5, 1.0))]);
            // retracting: ahead of its cell, with the base beneath it
            let plate = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.75, 1.0));
            let boxes = moving_block_collision_boxes(&moving(UP, false, 0.0, cube()), &[plate], at);
            assert_eq!(boxes, [plate, Aabb::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 2.0, 1.0))]);
        }
    }

    #[test]
    fn a_moving_block_joins_its_last_two_places_on_1_9_and_1_10() {
        for at in [release(1, 9, 4), release(1, 10, 2)] {
            let boxes = moving_block_collision_boxes(&moving(UP, true, 0.5, cube()), &[], at);
            assert_eq!(boxes, [Aabb::new(Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.0, 0.5, 1.0))]);
        }
    }

    #[test]
    fn a_moving_head_is_its_plate_alone_on_1_9_and_1_10() {
        // the outline box of pushedBlock: 1.9.4 PistonHeadBlock.java:30-35, not the full cube a head's
        // collision boxes do not make, and a retracting base stays one
        for at in [release(1, 9, 4), release(1, 10, 2)] {
            let mut head = moving(UP, true, 1.0, cube());
            head.source = true;
            let boxes = moving_block_collision_boxes(&head, &[], at);
            assert_eq!(boxes, [Aabb::new(Vec3::new(0.0, 0.25, 0.0), Vec3::new(1.0, 1.0, 1.0))]);
            let mut base = moving(UP, false, 1.0, cube());
            base.source = true;
            let boxes = moving_block_collision_boxes(&base, &[], at);
            assert_eq!(boxes, [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.5, 1.0))]);
        }
    }

    #[test]
    fn a_moving_block_is_drawn_back_by_the_progress_before_on_1_8() {
        let at = release(1, 8, 8);
        // flat from the last tick's 0.0F: its progress is 0.5F, lastProgress is not
        let boxes = moving_block_collision_boxes(&moving(UP, true, 0.5, cube()), &[], at);
        assert_eq!(boxes, [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0))]);
        let boxes = moving_block_collision_boxes(&moving(UP, true, 1.0, cube()), &[], at);
        assert_eq!(boxes, [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))]);
        // a slab's top goes below its bottom, and Box orders them: a wall over the cell's top half
        let slab = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0));
        let boxes = moving_block_collision_boxes(&moving(UP, true, 0.0, slab), &[], at);
        assert_eq!(boxes, [Aabb::new(Vec3::new(0.0, -0.5, 0.0), Vec3::new(1.0, 0.0, 1.0))]);
    }

    #[test]
    fn a_moving_block_stops_a_move_and_ends_with_its_block_entity() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), BlockState::default());
        let moved = [Aabb::new(Vec3::new(0.0, -0.5, 0.0), Vec3::new(1.0, 0.5, 1.0))];
        world.set_moving_block((0, 100, 0), &moved, true);
        assert_eq!(world.block_collision_boxes((0, 100, 0)).len(), 1);
        assert!(!world.is_suffocating((0, 100, 0)));
        // a cell the world does not hold takes nothing
        world.set_moving_block((5, 100, 0), &moved, true);
        assert!(!world.contains_block((5, 100, 0)));
        // written over, the block keeps its own shape
        world.set_block_shapes((0, 100, 0), BlockState::default(), vec![BlockShape::full_block()]);
        world.clear_moving_block((0, 100, 0));
        assert!(world.has_collision_shape((0, 100, 0)));
        world.set_moving_block((0, 100, 0), &moved, true);
        world.clear_moving_block((0, 100, 0));
        assert!(!world.has_collision_shape((0, 100, 0)));
    }

    #[test]
    fn a_moved_shape_reaches_a_box_from_the_cursors_corner_cells_from_1_14_4() {
        assert!(!super::reads_edge_cells(MinecraftVersion::new(1, 13, 2)));
        assert!(super::reads_edge_cells(MinecraftVersion::new(1, 14, 4)));
        let context = crate::world::EntityCollisionContext::default();
        let query = Aabb::new(Vec3::new(1.2, 99.5, 0.2), Vec3::new(1.8, 99.9, 0.8));
        let found = |edge_cells: bool| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 100, 0), BlockState::default());
            let shape = [Aabb::new(Vec3::new(0.5, -1.0, 0.0), Vec3::new(1.5, 0.0, 1.0))];
            world.set_moving_block((0, 100, 0), &shape, edge_cells);
            world.collision_block_shapes_for_entity(query, &context, MinecraftVersion::new(26, 3, 0)).len()
        };
        assert_eq!(found(true), 1);
        assert_eq!(found(false), 0);
    }

    fn standing_player(x: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(x, 101.0, 0.5),
            velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
            half_width: f64::from(0.3_f32),
            height: f64::from(1.8_f32),
            on_ground: true,
            is_player: true,
            controlled_by_local_instance: true,
            client_side: true,
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
            next_shapes: None,
            outline_shapes: Vec::new(),
            bounces: false,
            sticky_for_entities: false,
        }
    }

    fn push(state: &mut SimulationState, piston: MovingPiston, release: MinecraftVersion) {
        let mut world = WorldSnapshot::new();
        push_by_moving_pistons(
            state,
            &mut world,
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
        // Entity.move(PISTON) clears inLava and runs checkBlockCollision (1.14.4
        // Entity.java:567-569, 1.16 :660-662); from 1.16.2 the base tick's lava height sets it
        // (1.16.5 Entity.java:1178-1180)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 101, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let mut in_lava_after = |release| {
            let mut state = standing_player(0.5);
            push_by_moving_pistons(
                &mut state,
                &mut world,
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
        let mut world = WorldSnapshot::new();
        let mut state = standing_player(0.5);
        let further = MovingPiston {
            position: (1, 101, 0),
            ..extending_head(0.0)
        };
        push_by_moving_pistons(
            &mut state,
            &mut world,
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
        // RangedAttribute.java:30-32, Attributes.java:42-80)
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
        let mut pushed = |step_height: f64| {
            let mut state = SimulationState {
                step_height: Some(step_height),
                ..standing_player(0.5)
            };
            push_by_moving_pistons(
                &mut state,
                &mut world,
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

    /// Each side of the releases where a moving block's push and its NOCLIP change.
    const NOCLIP_RELEASES: [MinecraftVersion; 6] = [
        MinecraftVersion::new(1, 8, 9),
        MinecraftVersion::new(1, 9, 4),
        MinecraftVersion::new(1, 10, 2),
        MinecraftVersion::new(1, 11, 0),
        MinecraftVersion::new(1, 21, 1),
        MinecraftVersion::new(26, 3, 0),
    ];

    #[test]
    fn a_spectator_is_not_pushed() {
        for release in NOCLIP_RELEASES {
            let mut state = SimulationState {
                no_physics: true,
                ..standing_player(0.5)
            };
            push(&mut state, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.5, "{release}");
        }
    }

    #[test]
    fn an_entity_with_no_physics_is_pushed_through_what_is_in_its_way() {
        for release in NOCLIP_RELEASES {
            let mut world = WorldSnapshot::new();
            world.set_block(
                (1, 101, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
            let mut pushed = |no_physics: bool| {
                let mut state = SimulationState {
                    no_physics,
                    is_player: false,
                    ..standing_player(0.5)
                };
                push_by_moving_pistons(
                    &mut state,
                    &mut world,
                    &[extending_head(0.0)],
                    &mut [0.0; 3],
                    PhysicsConfig::default(),
                    release,
                );
                state.position.x
            };
            let (through, stopped) = (pushed(true), pushed(false));
            assert!(through > stopped, "{release}: {through} {stopped}");
        }
    }

    /// The player pushed east from 0.5 by a head, with a cell at x = 1 moving as `beside` says.
    fn pushed_beside(beside: &MovingPiston, base: &[Aabb], release: MinecraftVersion) -> f64 {
        let mut world = WorldSnapshot::new();
        world.set_block(
            beside.position,
            BlockState {
                id: 1,
                ..BlockState::default()
            },
        );
        set_moving_block(&mut world, beside, base, release);
        let mut state = standing_player(0.5);
        push_by_moving_pistons(
            &mut state,
            &mut world,
            &[extending_head(0.0)],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        state.position.x
    }

    /// A cube moving at x = 1, its box in the cell at this progress.
    fn moving_cube(extending: bool, progress: f32) -> MovingPiston {
        let back = f64::from(if extending { 1.0 - progress } else { 0.0 });
        MovingPiston {
            progress,
            ..moved_block(
                (1, 101, 0),
                extending,
                Aabb::new(Vec3::new(back, 0.0, 0.0), Vec3::new(1.0 + back, 1.0, 1.0)),
            )
        }
    }

    #[test]
    fn a_push_goes_through_a_block_moving_its_way_from_1_11() {
        for release in NOCLIP_RELEASES {
            let along = moving_cube(true, 0.5);
            let mut world = WorldSnapshot::new();
            world.set_block(along.position, BlockState::default());
            set_moving_block(&mut world, &along, &[], release);
            let noclip = release.at_least(1, 11, 0);
            assert_eq!(world.moves_through((1, 0, 0)), noclip, "{release}");
            if noclip {
                let pushed = pushed_beside(&along, &[], release);
                assert_eq!(pushed, 0.810000011920929, "{release}");
            }
        }
    }

    #[test]
    fn a_push_stops_at_a_block_going_the_other_way_or_ended() {
        let blocked = 1.0 - f64::from(0.3_f32);
        for release in [MinecraftVersion::new(1, 11, 0), MinecraftVersion::new(26, 3, 0)] {
            // (halfway west: its box, half a block back in its shape, is at rest in the cell)
            let west = MovingPiston {
                facing: WEST,
                progress: 0.5,
                ..moved_block(
                    (1, 101, 0),
                    true,
                    Aabb::new(Vec3::new(-0.5, 0.0, 0.0), Vec3::new(0.5, 1.0, 1.0)),
                )
            };
            assert_eq!(pushed_beside(&west, &[], release), blocked, "{release}");
            let ended = pushed_beside(&moving_cube(true, 1.0), &[], release);
            assert_eq!(ended, blocked, "{release}");
        }
    }

    #[test]
    fn a_push_stops_at_the_base_a_retracting_piston_leaves() {
        // (26.3 PistonMovingBlockEntity.java:329-338: under NOCLIP the shape is the base alone)
        let retracting = MovingPiston {
            facing: WEST,
            source: true,
            ..moving_cube(false, 0.0)
        };
        let base = [Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))];
        let release = MinecraftVersion::new(26, 3, 0);
        assert_eq!(retracting.movement_direction(), (1, 0, 0));
        let stopped = pushed_beside(&retracting, &base, release);
        assert_eq!(stopped, 1.0 - f64::from(0.3_f32));
        assert_eq!(pushed_beside(&retracting, &[], release), 0.810000011920929);
    }

    /// A head in its cell: the plate at the end `facing` and the arm behind it, short or not.
    fn head(facing: u8, short: bool) -> Vec<Aabb> {
        let arm = if short { 0.75 } else { 1.0 };
        let (plate, arm) = if facing == EAST {
            ((0.75, 1.0), (0.75 - arm, 0.75))
        } else {
            ((0.0, 0.25), (0.25, 0.25 + arm))
        };
        vec![
            Aabb::new(Vec3::new(plate.0, 0.0, 0.0), Vec3::new(plate.1, 1.0, 1.0)),
            Aabb::new(Vec3::new(arm.0, 0.375, 0.375), Vec3::new(arm.1, 0.625, 0.625)),
        ]
    }

    #[test]
    fn a_piston_that_has_ticked_stands_at_its_new_progress_for_the_next() {
        // physrec/pm_opposed_same_tick on 26.3. The east head pushes the player into the west one,
        // still at 0.5, and ends at 1; the west head's push then goes through the plate the box
        // is in, as far as the east head's arm, whose end is at 0.75
        let release = MinecraftVersion::new(26, 3, 0);
        let piston = |position, facing| MovingPiston {
            position,
            facing,
            source: true,
            shapes: head(facing, true),
            next_shapes: Some(head(facing, false)),
            ..moving(facing, true, 0.5, cube())
        };
        let pistons = [piston((0, 101, 0), EAST), piston((1, 101, 0), WEST)];
        let mut world = WorldSnapshot::new();
        for piston in &pistons {
            world.set_block(
                piston.position,
                BlockState {
                    id: 1,
                    ..BlockState::default()
                },
            );
            set_moving_block(&mut world, piston, &[], release);
        }
        let mut state = standing_player(0.9);
        push_by_moving_pistons(
            &mut state,
            &mut world,
            &pistons,
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        assert!((state.position.x - 1.05).abs() < 1.0e-6, "{}", state.position.x);
    }

    #[test]
    fn on_1_8_a_block_at_the_start_of_its_way_stops_the_player_with_no_depth() {
        // physrec/pm_sticky_stand_on_block on 1.8.8: the moved block ticks first, drawn in to
        // nothing at x = 1, and the head's 0.5625 ends the box there
        let release = MinecraftVersion::new(1, 8, 8);
        let block = MovingPiston {
            position: (1, 101, 0),
            ..moving(EAST, true, 0.0, cube())
        };
        let head = MovingPiston {
            position: (0, 101, 0),
            ..moving(EAST, true, 0.0, cube())
        };
        let mut world = WorldSnapshot::new();
        for piston in [&block, &head] {
            world.set_block(
                piston.position,
                BlockState {
                    id: 1,
                    ..BlockState::default()
                },
            );
            set_moving_block(&mut world, piston, &[], release);
        }
        let mut state = standing_player(0.5);
        push_by_moving_pistons(
            &mut state,
            &mut world,
            &[block, head],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        assert!((state.position.x - 0.7).abs() < 1.0e-6, "{}", state.position.x);
    }

    #[test]
    fn to_1_10_a_block_that_ended_is_the_block_it_moved_for_the_next() {
        // physrec/pm_opposed_same_tick on 1.8.8: the east head pushes 0.25 on its last tick and is
        // a head again; the west head's 0.25 goes through the plate the box is in, to the arm
        let release = MinecraftVersion::new(1, 8, 8);
        let piston = |position, facing| MovingPiston {
            position,
            facing,
            shapes: head(facing, false),
            ..moving(facing, true, 1.0, cube())
        };
        let pistons = [piston((0, 101, 0), EAST), piston((1, 101, 0), WEST)];
        let mut world = WorldSnapshot::new();
        for piston in &pistons {
            world.set_block(
                piston.position,
                BlockState {
                    id: 1,
                    ..BlockState::default()
                },
            );
            set_moving_block(&mut world, piston, &[], release);
        }
        let mut state = standing_player(0.9);
        push_by_moving_pistons(
            &mut state,
            &mut world,
            &pistons,
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        assert!((state.position.x - 1.05).abs() < 1.0e-6, "{}", state.position.x);
    }

    fn moved_block(position: (i32, i32, i32), extending: bool, shape: Aabb) -> MovingPiston {
        MovingPiston {
            position,
            facing: EAST,
            extending,
            source: false,
            progress: 0.0,
            shapes: vec![shape],
            next_shapes: None,
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
        // PistonMovingBlockEntity.java:120-168; 1.13.2 PistonMovingBlockEntity.java:109-150)
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
        // else the entity moves (1.8.8 PistonMovingBlockEntity.java:89-102)
        let release = MinecraftVersion::new(1, 8, 8);
        let mut state = standing_player(1.2);
        push(&mut state, slime_block((1, 101, 0), true), release);
        assert_eq!(state.velocity.x, 1.0);
        assert_eq!(state.position.x, 1.2);

        // a retracting block pushes only the tick it ends, and a slime block's then moves
        // (1.8.8 PistonMovingBlockEntity.java:123-125)
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
    fn on_1_8_a_thin_block_drawn_in_past_its_far_side_turns_over() {
        // getCollisionBox takes the progress off the far side of the pushed block's box and makes a
        // new Box, which orders each axis: a box 0.1875 deep drawn in by 0.5 spans 0.6875 - 1 of its
        // cell and meets the player behind it (1.8.9 PistonExtensionBlock.java:165-197, Box.java:22-29)
        let release = MinecraftVersion::new(1, 8, 8);
        let thin = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(0.1875, 1.0, 1.0)),
        );
        let mut state = standing_player(0.5);
        push(&mut state, thin.clone(), release);
        assert_eq!(state.position.x, 0.5 + 0.5625);
        // past the turned-over box the player is not met
        let mut state = standing_player(0.3);
        push(&mut state, thin, release);
        assert_eq!(state.position.x, 0.3);
    }

    #[test]
    fn on_1_9_a_slime_block_bounces_and_still_pushes() {
        // 1.9.4 PistonMovingBlockEntity.java:100-137: the motion, then the move out of the swept box
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
        // PistonMovingBlockEntity.java:84-89): a full cube at x 0..1.5 this tick
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
        // PistonMovingBlockEntity.java:162-184, 301-303; 1.21.11 PistonMovingBlockEntity.java:187-206)
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
        // reaches 1.5000010000000001 (1.20 PistonMovingBlockEntity.java:191, 201-205); 1.19.4 has
        // neither (1.19.4 PistonMovingBlockEntity.java)
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
        let mut pushed = world.clone();
        push_by_moving_pistons(
            state,
            &mut pushed,
            &[piston],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        assert_eq!(pushed.encode(), world.encode(), "the push leaves the world as it was");
    }

    #[test]
    fn from_1_21_10_a_piston_move_is_not_slowed_by_the_stuck_speed_multiplier() {
        // Entity.move: `if (moverType != MoverType.PISTON) delta = delta.multiply(stuck...)`, the
        // multiplier and the motion still cleared (1.21.10 Entity.java:729-735, 26.3 :783-789);
        // 1.21.9 slows every move (1.21.9 Entity.java:729-732)
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
        // fall and updateEntityMovementAfterFallOn bounces a falling one back (1.8.8
        // Entity.java:654-665, 1.16.5 :642-654, 1.21.10 :740-758); from 26.2
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
    fn a_piston_landing_gives_an_entity_that_is_not_living_less_of_the_bounce() {
        // The living side is above. Slime: SlimeBlock.bounceUp's 0.8 from 1.9, none in 1.8.9 (1.8.8
        // SlimeBlock.java:33-39, 1.9.4 :34-44, 1.21.11 :43). From 26.2 Entity.getBlockBounciness
        // takes 0.8F of the float restitution (26.2 Entity.java:874-881, 26.3 :905-911), the
        // product rounded as a float: 0.75F gives 0.6F.
        let moved = |world: &WorldSnapshot, version, entity: SimulationState| {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut state = SimulationState {
                velocity: Vec3::new(0.0, -0.5, 0.0),
                ..entity
            };
            push_in(&mut state, world, pressing_block(), release);
            assert!(state.on_ground, "{version}");
            state.velocity.y
        };
        let living = standing_player(0.5);
        let not_living = SimulationState {
            non_living: true,
            is_player: false,
            ..living
        };
        let slime = floor_of(BlockEffect::Slime, 1.0);
        for (version, kept) in [
            ("1.8.9", 0.5),
            ("1.9.4", 0.4),
            ("1.12.2", 0.4),
            ("1.20.4", 0.4),
            ("26.1", 0.4),
            ("26.2", 0.5 * 0.800000011920929),
            ("26.3", 0.5 * 0.800000011920929),
        ] {
            assert_eq!(moved(&slime, version, not_living), kept, "{version}");
            assert_eq!(moved(&slime, version, living), 0.5, "{version}");
        }
        let lesser = floor_of(BlockEffect::Slime, 0.75);
        assert_eq!(moved(&lesser, "26.3", not_living), 0.5 * 0.6000000238418579);
        assert_eq!(moved(&lesser, "26.3", living), 0.5 * 0.75);

        // the 26.3 restitution stays the movement's own to simulate (26.3 Entity.java:839), and
        // the entity's own bounciness is not scaled (:878)
        let remote = SimulationState {
            controlled_by_local_instance: false,
            ..not_living
        };
        assert_eq!(moved(&slime, "26.3", remote), -0.5);
        let springy = SimulationState {
            entity_bounciness: 0.9,
            ..not_living
        };
        assert_eq!(moved(&slime, "26.3", springy), 0.5 * 0.9);

        // A bed bounces from 1.12, by 0.66F and 0.8 for what is not living (1.12.2
        // BedBlock.java:155-159, 1.21.11 :142-143); a stone lands either
        let bed = floor_of(BlockEffect::Bed, 0.0);
        assert_eq!(moved(&bed, "1.11.2", not_living), 0.0);
        assert_eq!(moved(&bed, "1.11.2", living), 0.0);
        let bounced = 0.5 * f64::from(0.66_f32);
        assert_eq!(moved(&bed, "1.12.2", not_living), bounced * 0.8);
        assert_eq!(moved(&bed, "1.12.2", living), bounced);
        assert_eq!(moved(&bed, "1.20.4", not_living), bounced * 0.8);
        assert_eq!(moved(&bed, "1.20.4", living), bounced);
        let stone = floor_of(BlockEffect::None, 0.0);
        assert_eq!(moved(&stone, "1.20.4", not_living), -0.0);
    }

    #[test]
    fn a_piston_push_counts_into_the_fall_and_its_landing_deals_the_damage() {
        // checkFallDamage after Entity.move(PISTON): the drop adds to the fall distance (1.21.5+
        // `fallDistance -= (double)(float)dy`, 1.21.5 Entity.java:1323), a landing deals
        // floor(fallDistance + 1.0E-6 - 3.0) and ends the fall (1.21.5 LivingEntity.java:1643-1653)
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
        assert!(!airborne.block_fall_callback);
        assert!(landing.block_fall_callback);
        // only for a player this client moves from 1.21.2 (1.21.3 Entity.java:721-726)
        let mut other = SimulationState {
            controlled_by_local_instance: false,
            ..standing_player(0.5)
        };
        push_in(&mut other, &world, pressing_block(), release);
        assert!(!other.block_fall_callback);
    }

    #[test]
    fn a_piston_after_the_landing_leaves_the_landing_damage_of_the_tick() {
        // One tick, two moves: the player's own lands and Block.fallOn deals the damage; the piston
        // after it finds the fall ended, a fall distance of 0 that calls no fallOn (1.20.4
        // Entity.java:1118, 26.3 :1602), and has nothing to say about the damage
        let world = floor_of(BlockEffect::None, 0.0);
        for (version, release) in [
            ("1.20.4", MinecraftVersion::new(1, 20, 4)),
            ("26.3", MinecraftVersion::new(26, 3, 0)),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 101.05, 0.5),
                velocity: Vec3::new(0.0, -0.1, 0.0),
                on_ground: false,
                fall_distance: 5.0,
                safe_fall_distance: 3.0,
                ..standing_player(0.5)
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.fall_damage, 2.0, "{version}");
            assert_eq!(state.fall_distance, 0.0, "{version}");
            push_in(&mut state, &world, pressing_block(), release);
            assert!(state.on_ground, "{version}");
            assert_eq!(state.fall_damage, 2.0, "{version}");
        }
    }

    #[test]
    fn the_moves_of_a_tick_count_into_one_fall_in_the_order_they_ran() {
        // The tick's own move, then Entity.move(PISTON): each drop goes through checkFallDamage on
        // what the last left (1.20.4 Entity.java:1130 float, 1.21.5 :1323 double)
        let world = WorldSnapshot::new();
        let top = 101.0 + f64::from(1.8_f32);
        for (version, release, count) in [
            (
                "1.20.4",
                MinecraftVersion::new(1, 20, 4),
                (|fd, dy| f64::from(fd as f32 - dy as f32)) as fn(f64, f64) -> f64,
            ),
            ("26.3", MinecraftVersion::new(26, 3, 0), |fd, dy| fd - f64::from(dy as f32)),
        ] {
            let mut state = SimulationState {
                velocity: Vec3::new(0.0, -0.1, 0.0),
                on_ground: false,
                fall_distance: 1.0,
                ..standing_player(0.5)
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            let own = count(1.0, -0.1);
            assert_eq!(state.fall_distance, own, "{version}");
            let before = state.position.y;
            let pushed = top - 0.1 - 102.5 + 0.01;
            push_in(&mut state, &world, pressing_block(), release);
            assert_eq!(state.position.y, before - pushed, "{version}");
            assert_eq!(state.fall_distance, count(own, -pushed), "{version}");
        }
    }

    #[test]
    fn a_piston_push_into_a_cobweb_sticks_the_player_in_it() {
        // Entity.move runs checkInsideBlocks to 1.21.1 (1.16.5 Entity.java:692); from 1.21.2 the
        // piston applies the blocks of the move after it (1.21.3 PistonMovingBlockEntity.java:179,
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
        // LocalPlayer.java:912-920): a player pressed onto the ground walking into a step jumps
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
            // LocalPlayer.java:879-884, MovingPistonBlock.java:146-149; 1.10.2
            // LocalPlayer.java:847-850, MovingPistonBlock.java:129-138)
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
        // drawn in by what is left to go, none for air (1.8.8 MovingPistonBlock.java:169-203,
        // PistonMovingBlockEntity.java:80-86): a bottom slab misses a player standing on its top half
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
        // PistonMovingBlockEntity.java:80-89; 1.10.2 the same)
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
        // PistonMovingBlockEntity.java:84-89, StateDefinition.java:372, Block.java:83, 388)
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
            // BaseRailBlock.java:46, 56) still pushes a player beside it
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
    fn a_push_limited_to_nothing_still_applies_the_blocks_effects_from_1_21_2() {
        // Entity.move returns on a PISTON move limited to Vec3.ZERO (26.3 Entity.java:774-778),
        // and from 1.21.2 moveEntityByPiston calls applyEffectsFromBlocks after it all the same
        // (1.21.1 PistonMovingBlockEntity.java:175-179 has none; 1.21.2 :176-181; 1.21.5 :178-185;
        // 26.3 :163-170): the powder snow the player's head is in sets isInPowderSnow
        let mut world = powder_snow_at((0, 102, 0));
        for (release, in_powder_snow, ticks_frozen) in [
            (MinecraftVersion::new(1, 21, 1), false, 0),
            (MinecraftVersion::new(1, 21, 2), true, 0),
            (MinecraftVersion::new(1, 21, 4), true, 0),
            (MinecraftVersion::new(1, 21, 5), true, 1),
            (MinecraftVersion::new(26, 3, 0), true, 1),
        ] {
            let mut state = standing_player(0.5);
            push_by_moving_pistons(
                &mut state,
                &mut world,
                &[extending_head(0.0)],
                &mut [0.51, 0.0, 0.0],
                PhysicsConfig::default(),
                release,
            );
            assert_eq!(state.position.x, 0.5, "{release}");
            assert_eq!(state.in_powder_snow, in_powder_snow, "{release}");
            assert_eq!(state.ticks_frozen, ticks_frozen, "{release}");
        }
    }

    #[test]
    fn a_piston_push_into_powder_snow_sets_in_powder_snow() {
        // PowderSnowBlock.entityInside of the blocks the move went through sets isInPowderSnow,
        // which the next tick keeps as wasInPowderSnow (1.17 PowderSnowBlock.java:74,
        // Entity.java:416-417; 1.21.3 PistonMovingBlockEntity.java:179); from 1.21.5 by FREEZE, which
        // adds a frozen tick for an entity that can freeze (1.21.5 InsideBlockEffectType.java:7-13,
        // PowderSnowBlock.java:94, PistonMovingBlockEntity.java:186-193, Entity.java:492-493)
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
        // motion times 0 (1.21.11 Entity.java:804-809)
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

    #[test]
    fn a_slime_block_bounces_only_what_simulates_its_movement_from_26_3() {
        // 26.3 PistonMovingBlockEntity.java:122, Entity.java:3721-3727; 26.2 :132-137 has no such test
        let bounced = |client_side, controlled_by_local_instance, release| {
            let mut state = SimulationState {
                client_side,
                controlled_by_local_instance,
                ..standing_player(1.2)
            };
            push(&mut state, slime_block((1, 101, 0), true), release);
            state.velocity.x == 1.0
        };
        let new = MinecraftVersion::new(26, 3, 0);
        assert!(bounced(true, true, new));
        assert!(!bounced(true, false, new));
        assert!(bounced(false, false, new));
        assert!(bounced(true, false, MinecraftVersion::new(26, 2, 0)));
    }

    fn player_in_slime_sweep(
        facing: u8,
        extending: bool,
        client_side: bool,
    ) -> (SimulationState, MovingPiston) {
        let piston = MovingPiston {
            facing,
            ..slime_block((0, 100, 0), extending)
        };
        let center = piston.origin()
            + Vec3::new(0.5, 0.5, 0.5)
            + super::along(piston.movement_direction(), 0.7);
        let radius = Vec3::new(0.3, 0.3, 0.3);
        let state = SimulationState {
            position: Vec3::new(center.x, center.y - 0.3, center.z),
            bounding_box: Some(Aabb::new(center - radius, center + radius)),
            half_width: 0.3,
            height: 0.6,
            velocity: Vec3::ZERO,
            on_ground: false,
            client_side,
            controlled_by_local_instance: client_side,
            ..standing_player(center.x)
        };
        (state, piston)
    }

    #[test]
    fn server_players_are_excluded_from_slime_moves_from_1_16_2_until_26_3() {
        // Exact 1.16.1 bytecode sets velocity then sends a motion packet to ServerPlayer;
        // 1.16.2/.3/.4 jump to the next entity before either bounce or displacement. The
        // same exclusion remains at 26.2 PistonMovingBlockEntity.java:136, removed in 26.3.
        let mut world = WorldSnapshot::new();
        for version in [
            "1.16.1", "1.16.2", "1.16.3", "1.16.4", "1.16.5", "1.21.11", "26.1", "26.2", "26.3",
        ] {
            let release = MinecraftVersion::parse(version).unwrap();
            for facing in 0..6 {
                for extending in [false, true] {
                    for client_side in [false, true] {
                        let (mut state, piston) =
                            player_in_slime_sweep(facing, extending, client_side);
                        let before = state;
                        let direction = piston.movement_direction();
                        let mut deltas = [0.0; 3];
                        push_by_moving_pistons(
                            &mut state,
                            &mut world,
                            &[piston],
                            &mut deltas,
                            PhysicsConfig::default(),
                            release,
                        );
                        let excluded = !client_side
                            && release.at_least(1, 16, 2)
                            && !release.at_least(26, 3, 0);
                        if excluded {
                            assert_eq!(
                                state, before,
                                "{version}, facing={facing}, extending={extending}"
                            );
                            assert_eq!(deltas, [0.0; 3]);
                        } else {
                            let expected = before.position + super::along(direction, 0.51);
                            assert!(
                                (state.position - expected).length_squared() < 1.0e-24,
                                "{version}, facing={facing}, extending={extending}: {:?}",
                                state.position
                            );
                            assert_eq!(
                                state.velocity,
                                super::along(direction, 1.0),
                                "{version}: bounce"
                            );
                            assert_eq!(
                                deltas,
                                [
                                    f64::from(direction.0) * 0.51,
                                    f64::from(direction.1) * 0.51,
                                    f64::from(direction.2) * 0.51
                                ]
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn modern_slime_authority_override_and_default_modes_only_control_the_bounce() {
        // Player uses AUTHORITATIVE_SIDE_AND_SERVER; Entity defaults AUTHORITATIVE_SIDE.
        // An exact result also represents SERVER_AND_CLIENT and custom modes without guessing.
        let release = MinecraftVersion::new(26, 3, 0);
        for (is_player, client_side, local, exact, bounced) in [
            (true, false, false, None, true),
            (true, true, true, None, true),
            (true, true, false, None, false),
            (false, false, true, None, true),
            (false, false, false, None, false),
            (false, true, true, None, true),
            (false, true, false, None, false),
            (true, false, false, Some(false), false),
            (true, true, true, Some(false), false),
            (false, false, true, Some(false), false),
            (false, true, false, Some(true), true),
            (true, true, false, Some(true), true),
        ] {
            let mut state = SimulationState {
                is_player,
                client_side,
                controlled_by_local_instance: local,
                can_simulate_movement: exact,
                ..standing_player(1.2)
            };
            push(&mut state, slime_block((1, 101, 0), true), release);
            assert_eq!(
                state.velocity.x,
                if bounced { 1.0 } else { 0.0 },
                "player={is_player}, client={client_side}, local={local}, exact={exact:?}"
            );
            assert_eq!(
                state.position.x,
                1.2 + 0.51,
                "authority must not suppress displacement"
            );
        }

        for version in ["1.16.1", "1.16.2", "26.2"] {
            for exact in [None, Some(false), Some(true)] {
                let release = MinecraftVersion::parse(version).unwrap();
                // Neither local client players nor generic remote entities had a bounce
                // authority check; the new exact result must not backport it.
                for is_player in [false, true] {
                    let mut state = SimulationState {
                        is_player,
                        controlled_by_local_instance: false,
                        can_simulate_movement: exact,
                        ..standing_player(1.2)
                    };
                    push(&mut state, slime_block((1, 101, 0), true), release);
                    assert_eq!(state.velocity.x, 1.0, "{version}, exact={exact:?}");
                    assert_eq!(state.position.x, 1.2 + 0.51);
                }
                let mut server = SimulationState {
                    client_side: false,
                    can_simulate_movement: exact,
                    ..standing_player(1.2)
                };
                let before = server;
                push(&mut server, slime_block((1, 101, 0), true), release);
                assert_eq!(
                    server == before,
                    release.at_least(1, 16, 2),
                    "{version}: exact override cannot bypass old ServerPlayer exclusion"
                );
            }
        }
    }

    #[test]
    fn historical_server_slime_exclusion_does_not_skip_other_pistons_or_honey() {
        for version in ["1.16.1", "1.16.2", "26.2", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let stone = moved_block(
                (1, 101, 0),
                true,
                Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
            );
            let mut server = SimulationState {
                client_side: false,
                ..standing_player(1.2)
            };
            push(&mut server, stone.clone(), release);
            assert_eq!(server.position.x, 1.2 + 0.51, "{version}: ordinary piston");
            assert_eq!(server.velocity.x, 0.0);

            let mut carried = SimulationState {
                client_side: false,
                ..standing_player(0.5)
            };
            push(&mut carried, honey_block((1, 100, 0), true), release);
            assert_eq!(carried.position.x, 1.0, "{version}: honey carry");

            if release.at_least(1, 16, 2) && !release.at_least(26, 3, 0) {
                let mut listed = SimulationState {
                    client_side: false,
                    ..standing_player(1.2)
                };
                let mut deltas = [0.0; 3];
                push_by_moving_pistons(
                    &mut listed,
                    &mut WorldSnapshot::new(),
                    &[slime_block((1, 101, 0), true), stone],
                    &mut deltas,
                    PhysicsConfig::default(),
                    release,
                );
                assert_eq!(
                    listed.position.x, server.position.x,
                    "later piston still runs"
                );
                assert_eq!(listed.velocity.x, 0.0);
                assert_eq!(deltas, [0.51, 0.0, 0.0]);
            }
        }
    }

    #[test]
    fn server_slime_exclusion_also_prevents_bounce_outside_the_swept_face() {
        for version in ["1.16.1", "1.16.2", "26.2", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut state = SimulationState {
                client_side: false,
                ..standing_player(0.5)
            };
            push(&mut state, slime_block((1, 101, 0), true), release);
            assert_eq!(state.position.x, 0.5);
            let eligible = !release.at_least(1, 16, 2) || release.at_least(26, 3, 0);
            assert_eq!(
                state.velocity.x,
                if eligible { 1.0 } else { 0.0 },
                "{version}"
            );
        }
    }

    #[test]
    fn remote_horizontal_piston_moves_preserve_ground_support_and_run_block_callbacks() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (2, 101, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for version in ["1.21.3", "1.21.4", "26.2", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut state = SimulationState {
                controlled_by_local_instance: false,
                collided_horizontally: true,
                collided_vertically: true,
                vertical_collision_below: true,
                supporting_block: Some((1, 100, 0)),
                supporting_block_kept: true,
                on_ground_no_blocks: false,
                ..standing_player(1.2)
            };
            push_in(&mut state, &world, slime_block((1, 101, 0), true), release);
            assert_eq!(state.position.x, 1.2 + 0.51, "{version}: displacement");
            assert!(!state.collided_horizontally);
            if release.at_least(1, 21, 4) {
                assert!(
                    state.collided_vertically && state.vertical_collision_below && state.on_ground,
                    "{version}: retained vertical flags"
                );
                assert_eq!(state.supporting_block, Some((1, 100, 0)));
                assert!(state.supporting_block_kept);
                assert!(!state.on_ground_no_blocks);
            } else {
                assert!(
                    !state.collided_vertically
                        && !state.vertical_collision_below
                        && !state.on_ground
                );
                assert_eq!(state.supporting_block, None);
            }
            assert_eq!(
                state.stuck_speed_multiplier,
                Vec3::new(0.25, f64::from(0.05_f32), 0.25),
                "{version}: callback still runs"
            );
            assert_eq!(
                state.velocity.x,
                if release.at_least(26, 3, 0) { 0.0 } else { 1.0 }
            );
        }
    }

    #[test]
    fn piston_collision_restitution_uses_simulation_authority_from_26_2() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (2, 101, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let stone = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
        );
        for version in ["26.2", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            for (is_player, client_side, local, exact, stops_26_2, stops_26_3) in [
                (true, true, false, None, false, false),
                (true, true, true, Some(false), true, false),
                (true, false, false, None, true, true),
                (true, false, false, Some(false), true, false),
                (false, false, false, None, false, false),
                (false, false, true, Some(false), true, false),
                (false, true, false, Some(true), false, true),
            ] {
                let mut state = SimulationState {
                    is_player,
                    client_side,
                    controlled_by_local_instance: local,
                    can_simulate_movement: exact,
                    velocity: Vec3::new(0.25, 0.0, 0.0),
                    ..standing_player(1.2)
                };
                push_in(&mut state, &world, stone.clone(), release);
                assert!(
                    state.collided_horizontally,
                    "{version}: remote collision still detected"
                );
                assert!((state.position.x - (2.0 - f64::from(0.3_f32))).abs() < 1.0e-12);
                let stops = if release.at_least(26, 3, 0) {
                    stops_26_3
                } else {
                    stops_26_2
                };
                assert_eq!(state.velocity.x, if stops { 0.0 } else { 0.25 }, "{version}, player={is_player}, client={client_side}, local={local}, exact={exact:?}");
            }
        }
    }

    #[test]
    fn what_cannot_simulate_keeps_its_motion_but_takes_the_blocks_speed_factor_from_26_2() {
        // restitution is the simulating side's, the speed factor comes after it whoever moves
        // (26.2 Entity.java:815, 827)
        let mut world = floor_of(BlockEffect::SoulSand, 0.0);
        let release = MinecraftVersion::new(26, 2, 0);
        let mut state = SimulationState {
            is_player: false,
            client_side: true,
            controlled_by_local_instance: false,
            velocity: Vec3::new(0.25, 0.0, 0.0),
            ..standing_player(0.5)
        };
        let mut deltas = [0.0; 3];
        push_body_by_moving_pistons(&mut state, &mut world, &[extending_head(0.0)], &mut deltas, MoveRules::MINECART, release);
        assert_eq!(state.velocity.x, 0.25 * f64::from(0.4_f32));
    }

    #[test]
    fn a_piston_pushes_through_a_bubble_column_only_a_simulating_entity_is_moved_by_in_26_3() {
        // BubbleColumnBlock.entityInside by the piston move's applyEffectsFromBlocks (26.3
        // PistonMovingBlockEntity.java:163-170): 26.3 asks canSimulateMovement before the motion
        // and not before resetFallDistance, which only a column with something above it does
        // (26.3 Entity.java:2973-2983, 3005-3017); 26.2 asks nothing (26.2 Entity.java:2882-2922)
        let column = |effect| BlockState {
            id: 2,
            fluid: FluidState::Water,
            fluid_source: true,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                effect,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let water = BlockState {
            id: 3,
            fluid: FluidState::Water,
            fluid_source: true,
            ..BlockState::default()
        };
        let stone = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
        );
        for (effect, covered, push) in [
            (BlockEffect::BubbleUp, true, 0.06),
            (BlockEffect::BubbleDown, true, -0.03),
            (BlockEffect::BubbleUp, false, 0.1),
            (BlockEffect::BubbleDown, false, -0.03),
        ] {
            let mut world = WorldSnapshot::new();
            world.set_block((1, 101, 0), column(effect));
            if covered {
                world.set_block((1, 102, 0), water);
            }
            for (version, is_player, client_side, local, exact, pushed) in [
                ("26.2", false, false, false, Some(false), true),
                ("26.2", false, false, false, None, true),
                ("26.2", true, true, false, None, true),
                ("26.3", false, false, false, Some(false), false),
                ("26.3", false, false, false, None, false),
                ("26.3", true, true, false, None, false),
                ("26.3", true, true, true, Some(false), false),
                ("26.3", true, true, true, None, true),
                ("26.3", true, false, false, None, true),
                ("26.3", false, false, true, None, true),
                ("26.3", false, true, false, Some(true), true),
            ] {
                let release = MinecraftVersion::parse(version).unwrap();
                let mut state = SimulationState {
                    is_player,
                    client_side,
                    controlled_by_local_instance: local,
                    can_simulate_movement: exact,
                    height: 0.5,
                    on_ground: false,
                    velocity: Vec3::ZERO,
                    fall_distance: 3.0,
                    ..standing_player(1.2)
                };
                push_in(&mut state, &world, stone.clone(), release);
                let case = format!("{version} {effect:?} covered={covered} player={is_player} client={client_side} local={local} exact={exact:?}");
                assert_eq!(state.position.x, 1.2 + 0.51, "{case}");
                assert_eq!(state.velocity.y, if pushed { push } else { 0.0 }, "{case}");
                assert_eq!(
                    state.fall_distance,
                    if covered { 0.0 } else { 3.0 },
                    "{case}"
                );
            }
        }
    }

    #[test]
    fn piston_restitution_reads_the_retained_vertical_collision() {
        // A horizontal collision can trigger restitution using verticalCollision retained by
        // an entity that simulates movement without being locally authoritative (26.2
        // Entity.java:815,845; 26.3 :839,877). Use an ordinary piston to isolate restitution.
        let mut world = WorldSnapshot::new();
        world.set_block(
            (2, 101, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (1, 100, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 0.8,
                ..BlockState::default()
            },
        );
        let stone = moved_block(
            (1, 101, 0),
            true,
            Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
        );
        for version in ["26.2", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut state = SimulationState {
                client_side: false,
                controlled_by_local_instance: false,
                collided_vertically: true,
                vertical_collision_below: true,
                supporting_block: Some((1, 100, 0)),
                supporting_block_kept: true,
                velocity: Vec3::new(0.25, -0.5, 0.0),
                ..standing_player(1.2)
            };
            push_in(&mut state, &world, stone.clone(), release);
            assert!(
                state.collided_horizontally
                    && state.collided_vertically
                    && state.vertical_collision_below
            );
            assert!(state.on_ground);
            assert_eq!(state.supporting_block, Some((1, 100, 0)));
            assert_eq!(
                state.velocity.y, 0.4,
                "{version}: retained vertical collision reaches floor restitution"
            );
        }
    }

    // B290: fixtures that move each of these gates one release either way.

    #[test]
    fn a_push_goes_through_a_block_moving_its_way_on_1_11_2_and_not_on_1_10_2() {
        // 1.10.2 PistonMovingBlockEntity.java has no NOCLIP; 1.11.2 :322-329 lets a PISTON move
        // pass a block moving its way
        for (release, noclip) in [
            (MinecraftVersion::new(1, 10, 2), false),
            (MinecraftVersion::new(1, 11, 2), true),
        ] {
            let along = moving_cube(true, 0.5);
            let mut world = WorldSnapshot::new();
            world.set_block(along.position, BlockState::default());
            set_moving_block(&mut world, &along, &[], release);
            assert_eq!(world.moves_through((1, 0, 0)), noclip, "{release}");
        }
        let release = MinecraftVersion::new(1, 11, 2);
        assert_eq!(pushed_beside(&moving_cube(true, 0.5), &[], release), 0.810000011920929);
    }

    #[test]
    fn on_1_10_2_a_block_that_ended_is_the_block_it_moved_for_the_next() {
        // to 1.10 a block entity at progress 1 pushes once more and its tick ends it: the block it
        // moved is in its cell for the pistons after it (mc-decompiled 1.10.2 PistonBlockEntity.java:166-183).
        // A slab rising into the cell over the player's head: its moving box reached down into the
        // player's way (one box from the progress before, 1.10.2 :81-90), the slab it leaves does not,
        // so the head pushing the player after it goes on under the slab
        let release = MinecraftVersion::new(1, 10, 2);
        let slab = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0));
        let risen = MovingPiston {
            position: (1, 103, 0),
            facing: UP,
            extending: true,
            source: false,
            progress: 1.0,
            shapes: vec![slab],
            next_shapes: None,
            outline_shapes: vec![slab],
            bounces: false,
            sticky_for_entities: false,
        };
        let mut world = WorldSnapshot::new();
        world.set_block(
            risen.position,
            BlockState {
                id: 1,
                ..BlockState::default()
            },
        );
        set_moving_block(&mut world, &risen, &[], release);
        let mut state = standing_player(0.5);
        push_by_moving_pistons(
            &mut state,
            &mut world,
            &[risen, extending_head(0.0)],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        assert!(state.position.x > 0.75, "{}", state.position.x);
    }

    #[test]
    fn from_1_11_a_retracting_base_that_ended_still_stops_the_next_push() {
        // from 1.11 a retracting piston at progress 1 is removed by its tick and leaves its pushedBlock,
        // the retracted base, a full cube (mc-decompiled 1.11.2 PistonBlockEntity.java:277-285; its
        // shape until then the base with the head, :322-352): a head pushing after it in the tick
        // meets the base, not the head's plate alone, as the engine's cell of base and head does
        let release = MinecraftVersion::new(1, 11, 2);
        let retracting = MovingPiston {
            position: (1, 101, 0),
            facing: WEST,
            extending: false,
            source: true,
            progress: 1.0,
            shapes: vec![Aabb::new(Vec3::new(0.75, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))],
            next_shapes: None,
            outline_shapes: Vec::new(),
            bounces: false,
            sticky_for_entities: false,
        };
        let base = [cube()];
        let mut world = WorldSnapshot::new();
        world.set_block(
            retracting.position,
            BlockState {
                id: 1,
                ..BlockState::default()
            },
        );
        set_moving_block(&mut world, &retracting, &base, release);
        let mut state = standing_player(0.5);
        push_by_moving_pistons(
            &mut state,
            &mut world,
            &[retracting, extending_head(0.0)],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
        );
        assert_eq!(state.position.x, 1.0 - f64::from(0.3_f32));
    }

    #[test]
    fn a_piston_push_sets_in_powder_snow_on_1_17_and_1_21_4_too() {
        // 1.17 PowderSnowBlock.java:74 sets isInPowderSnow from the move's checkInsideBlocks; 1.21.4
        // PowderSnowBlock.java:69 from the piston's applyEffectsFromBlocks (PistonMovingBlockEntity.java:179)
        let world = powder_snow_at((1, 101, 0));
        for release in [MinecraftVersion::new(1, 17, 0), MinecraftVersion::new(1, 21, 4)] {
            let mut state = standing_player(0.5);
            push_in(&mut state, &world, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            assert!(state.in_powder_snow, "{release}");
            assert_eq!(state.ticks_frozen, 0, "{release}");
        }
    }

    fn fire_at(position: (i32, i32, i32)) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    effect: BlockEffect::Fire,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn a_piston_push_into_fire_counts_one_fire_tick_on_1_21_1_and_1_21_2() {
        // BaseFireBlock.entityInside raises negative fire ticks by one (1.21.1 BaseFireBlock.java,
        // 1.21.2 :121-133), once: 1.21.1 in the move's checkInsideBlocks, whose fire test then keeps
        // them (Entity.java:714, 738-745); 1.21.2 only in applyEffectsFromBlocks after the move
        // (PistonMovingBlockEntity.java:179, Entity.java:780-805), which keeps them with fire inside
        let world = fire_at((1, 101, 0));
        for release in [MinecraftVersion::new(1, 21, 1), MinecraftVersion::new(1, 21, 2)] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(-5),
                ..standing_player(0.5)
            };
            push_in(&mut state, &world, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            assert_eq!(state.remaining_fire_ticks, Some(-4), "{release}");
        }
    }

    #[test]
    fn a_piston_push_applies_the_blocks_from_the_ticks_start_until_1_21_4_and_its_own_from_1_21_5() {
        // 1.21.4 PistonMovingBlockEntity.java:179 applyEffectsFromBlocks() from oldPosition (Entity.java:776-777);
        // 1.21.5 PistonMovingBlockEntity.java:178-185 applyEffectsFromBlocks(previousPos, position()) of the push alone
        // (a travel of a block or more walks the box's corner back along it, the start cell left out:
        // 1.21.4 BlockGetter.java:164-181, 183-235; the cobweb is on the way from x -1.5)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (-1, 101, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for (release, stuck) in [
            (MinecraftVersion::new(1, 21, 4), Vec3::new(0.25, f64::from(0.05_f32), 0.25)),
            (MinecraftVersion::new(1, 21, 5), Vec3::ZERO),
        ] {
            let mut state = SimulationState {
                previous_position: Some(Vec3::new(-1.5, 101.0, 0.5)),
                ..standing_player(0.5)
            };
            push_in(&mut state, &world, extending_head(0.0), release);
            assert_eq!(state.position.x, 0.810000011920929, "{release}");
            assert_eq!(state.stuck_speed_multiplier, stuck, "{release}");
        }
    }

    #[test]
    fn a_piston_landing_rounds_its_damage_up_until_1_21_4_and_down_from_1_21_5() {
        // 1.21.4 LivingEntity.java:1621-1627 Mth.ceil in float; 1.21.5 :1643-1653 Mth.floor with 1.0E-6.
        // The server's player: 1.21.4 leaves a client player's landing to the server (Entity.java:721,
        // Player.java:2049)
        let world = floor_of(BlockEffect::None, 0.0);
        for (release, damage) in [(MinecraftVersion::new(1, 21, 4), 3.0), (MinecraftVersion::new(1, 21, 5), 2.0)] {
            let mut landing = SimulationState {
                fall_distance: 5.5,
                safe_fall_distance: 3.0,
                client_side: false,
                ..standing_player(0.5)
            };
            push_in(&mut landing, &world, pressing_block(), release);
            assert!(landing.on_ground, "{release}");
            assert_eq!(landing.fall_damage, damage, "{release}");
        }
    }

    #[test]
    fn a_piston_landing_takes_jump_boost_off_the_fall_until_1_20_4_and_the_attribute_from_1_20_5() {
        // 1.20.4 LivingEntity.java:1569 ceil(fallDistance - 3 - (amplifier + 1)); 1.20.5 :1593-1600
        // the SAFE_FALL_DISTANCE attribute, which Jump Boost raises (MobEffects.java, 1.20.5)
        let world = floor_of(BlockEffect::None, 0.0);
        for (release, safe, damage) in [
            (MinecraftVersion::new(1, 20, 4), 3.0, 1.0),
            (MinecraftVersion::new(1, 20, 5), 5.0, 1.0),
        ] {
            let mut landing = SimulationState {
                fall_distance: 6.0,
                safe_fall_distance: safe,
                jump_boost_amplifier: Some(1),
                ..standing_player(0.5)
            };
            push_in(&mut landing, &world, pressing_block(), release);
            assert!(landing.on_ground, "{release}");
            assert_eq!(landing.fall_damage, damage, "{release}");
        }
    }

    #[test]
    fn from_26_2_a_body_that_cannot_simulate_lands_on_slime_unbounced() {
        // 26.2 Entity.java:815 restitutes only where canSimulateMovement, which on a server is false
        // for what a client-authoritative passenger controls (Entity.java:3581-3600)
        let slime = floor_of(BlockEffect::Slime, 1.0);
        let landed = |controlled| {
            let body = SimulationState {
                position: Vec3::new(0.5, 101.0, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                collided_vertically: true,
                non_living: true,
                client_side: false,
                controlled_by_local_instance: controlled,
                ..SimulationState::default()
            };
            pushed_body(body, &slime, pressing_block(), MinecraftVersion::new(26, 2, 0)).0.velocity.y
        };
        assert!(landed(true) > 0.0);
        assert_eq!(landed(false), -0.5);
    }
}
