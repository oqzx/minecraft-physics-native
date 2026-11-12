//! The dedicated ticks of the entities that move on their own without a brain: primed TNT, falling
//! blocks, dropped items and experience orbs (PrimedTnt, FallingBlockEntity, ItemEntity and
//! ExperienceOrb). A tick reads the world and never writes it: what vanilla does to the world or
//! to other entities comes out as an event of [`MovingEntityOutcome`], and only under
//! [`MovingEntityAuthority::Server`]. A client's prediction therefore places no block, sets off no
//! explosion, merges nothing and collects nothing; the server's packets do those. What a client
//! cannot see comes in as [`MovingEntityInputs`]: the entity's random draws, the players an orb
//! chooses among.
//!
//! [`tick_mob_travel`] is the same for the mobs whose `travel` is their own: the server's AI stays
//! outside, and what it decides comes in as the state a travel starts from.

use crate::entity_travel::{travel_support, TravelFamily};
use crate::block_traversal::Movement;
use crate::legacy_fluid;
use crate::math::{floor_i32_for, java_float, Aabb, Vec3};
use crate::simulation::{
    accumulated_fall_distance, applies_effects_from_blocks, apply_block_effects, block_position, blocks_inside_moves,
    collision_query_entity, entity_collision_context, entity_eyes_in_fluid, entity_fluid_contact,
    movements_this_tick, on_position, push_by_fluid,
    move_without_physics, update_in_lava_after_move,
    SimulationState,
};
use crate::projectile::{clip_block, traverse_blocks, ClipShape};
use crate::ridden::PlainMob;
use crate::vehicle::{
    apply_effects_inside, is_water, lava_flow_scale, move_vehicle, MoveRules, VehicleMove,
};
use crate::version::MinecraftVersion;
use crate::world::{FluidContact, FluidState, WorldSnapshot};

type BlockPosition = (i32, i32, i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovingEntityKind {
    PrimedTnt,
    FallingBlock,
    Item,
    ExperienceOrb,
}

impl MovingEntityKind {
    /// The entity's registry name (`tnt`, `falling_block`, `item`, `experience_orb`).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "tnt" => Some(Self::PrimedTnt),
            "falling_block" => Some(Self::FallingBlock),
            "item" => Some(Self::Item),
            "experience_orb" => Some(Self::ExperienceOrb),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PrimedTnt => "tnt",
            Self::FallingBlock => "falling_block",
            Self::Item => "item",
            Self::ExperienceOrb => "experience_orb",
        }
    }

    /// EntityType's width and height (EntityType#sized; Entity#setBounds in the constructors before
    /// 1.14), which hold from 1.8 (1.8.8 PrimedTnt.java:16,
    /// FallingBlockEntity.java:52, ItemEntity.java:35, ExperienceOrb.java:61; 1.20.4
    /// EntityType.java:290, 297, 353, 505).
    #[must_use]
    pub const fn dimensions(self) -> (f32, f32) {
        match self {
            Self::PrimedTnt | Self::FallingBlock => (0.98, 0.98),
            Self::Item => (0.25, 0.25),
            Self::ExperienceOrb => (0.5, 0.5),
        }
    }

    /// getDefaultGravity. A float widened to double in the motion before 1.14 (1.13.2
    /// PrimedTnt.java:65, ExperienceOrb.java:101), a double after (1.14.4 ItemEntity.java:81,
    /// ExperienceOrb.java:105; 26.3 PrimedTnt.java:80, ExperienceOrb.java:92).
    #[must_use]
    pub fn gravity(self, release: MinecraftVersion) -> f64 {
        let gravity = if self == Self::ExperienceOrb { 0.03 } else { 0.04 };
        if release.at_least(1, 14, 0) {
            gravity
        } else {
            java_float(gravity)
        }
    }
}

/// Whose tick this is. The client and the server run the same tick with different branches
/// (`level.isClientSide()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovingEntityAuthority {
    /// A prediction: the client's branches, with no event.
    Client,
    /// The server's tick: its branches, and what they do to the world as events.
    Server,
}

/// A player an orb can follow, where it stands when the orb ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovingEntityPlayer {
    pub id: i32,
    pub position: Vec3,
    /// Entity.getEyeHeight of its pose.
    pub eye_height: f32,
    pub spectator: bool,
    pub dead: bool,
}

/// What a tick reads that its state does not say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MovingEntityInputs {
    /// The entity's next `random.nextFloat()` draws, in order. A tick that needs more than there
    /// are says so in `random_missing`, and uses 0.
    pub random: Vec<f32>,
    /// The players of the level, in Level.players() order.
    pub players: Vec<MovingEntityPlayer>,
    /// A falling block on a server before 1.14: FallingBlock.isFree of the block under its feet at
    /// y - 0.01, whose air, fire, liquid and replaceable materials no block state here says. Such
    /// a block goes on falling from what it landed on (a fence's top).
    pub falls_through_below: bool,
}

/// What a tick leaves for others to do, and how it used its inputs. Nothing here has happened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MovingEntityOutcome {
    /// PrimedTnt.explode (server only).
    pub explodes: bool,
    /// A falling block ended here (server only): its block goes in, breaks or drops.
    pub landed_at: Option<BlockPosition>,
    /// A falling block that fell too long (server only): its item drops if the rules say so.
    pub expired: bool,
    /// Before 1.18.2 a falling block's first tick takes its own block out of the world, on both
    /// sides. The tick moved as if it had; the caller's world changes from the server's packet.
    pub clears_block: Option<BlockPosition>,
    /// The tick looks for items or orbs to merge with (server only).
    pub merge_check: bool,
    /// How many of `inputs.random` the tick drew.
    pub random_used: u32,
    /// The tick drew more than it was given.
    pub random_missing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovingEntityState {
    pub kind: MovingEntityKind,
    pub authority: MovingEntityAuthority,
    /// Entity.id: an item's rest and an orb's choice of target turn on it.
    pub id: i32,
    /// Entity.tickCount, which Level.tickNonPassenger counts before the tick.
    pub tick_count: i32,
    pub position: Vec3,
    pub velocity: Vec3,
    /// Before 1.17 Entity keeps its box, which the position follows.
    pub bounding_box: Option<Aabb>,
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub vertical_collision_below: bool,
    pub in_water: bool,
    pub in_lava: bool,
    pub no_gravity: bool,
    pub no_physics: bool,
    pub stuck_speed_multiplier: Vec3,
    pub fall_distance: f64,
    /// The dimension is ultrawarm: lava pushes at 0.007.
    pub ultra_warm: bool,
    pub first_tick: bool,
    /// Entity.finalMovementsThisTick: a resting item's blocks from 26.1.
    pub(crate) last_movements: Vec<Movement>,
    pub removed: bool,
    /// PrimedTnt.getFuse.
    pub fuse: i32,
    /// FallingBlockEntity.time.
    pub time: i32,
    /// The carried block's id, as the world's blocks say it.
    pub carried_block: u32,
    /// The carried block is air: it goes at once.
    pub carried_air: bool,
    /// The carried block is a concrete powder: water stops it.
    pub concrete_powder: bool,
    /// 1.18 and 1.18.1: the client keeps a landed block for 50 ms after the server's removal,
    /// frozen (FallingBlockEntity.discardTime; 1.18 FallingBlockEntity.java:103-106).
    pub discarding: bool,
    /// ItemEntity.age, ExperienceOrb.age.
    pub age: i32,
    /// ItemEntity.pickupDelay; ExperienceOrb.throwTime before 1.17.
    pub pickup_delay: i32,
    /// The item's stack is empty: it goes at once.
    pub item_empty: bool,
    /// The item is fire immune (Item.isFireproof from 1.16, a netherite item or ancient debris):
    /// its 1.16 tick plays no burn in lava, so draws nothing for it.
    pub fire_immune: bool,
    /// ExperienceOrb.followingPlayer, by its entity id.
    pub following_player: Option<i32>,
    /// ExperienceOrb.followingTime before 1.17.
    pub following_time: i32,
    /// ExperienceOrb.tickCount before 1.17, its own and not Entity's.
    pub orb_ticks: i32,
    /// A move past MAX_WALKED_CELLS did not run, and the bindings refuse the tick (`move_vehicle`).
    pub move_out_of_range: bool,
}

impl MovingEntityState {
    #[must_use]
    pub fn new(kind: MovingEntityKind, position: Vec3, velocity: Vec3) -> Self {
        Self {
            kind,
            authority: MovingEntityAuthority::Client,
            id: 0,
            tick_count: 0,
            position,
            velocity,
            bounding_box: None,
            on_ground: false,
            collided_horizontally: false,
            collided_vertically: false,
            vertical_collision_below: false,
            in_water: false,
            in_lava: false,
            no_gravity: false,
            no_physics: false,
            stuck_speed_multiplier: Vec3::ZERO,
            fall_distance: 0.0,
            ultra_warm: false,
            first_tick: true,
            last_movements: Vec::new(),
            removed: false,
            fuse: 80,
            time: 0,
            carried_block: 0,
            carried_air: false,
            concrete_powder: false,
            discarding: false,
            age: 0,
            pickup_delay: 0,
            item_empty: false,
            fire_immune: false,
            following_player: None,
            following_time: 0,
            orb_ticks: 0,
            move_out_of_range: false,
        }
    }

    #[must_use]
    pub fn on_server(self) -> Self {
        Self {
            authority: MovingEntityAuthority::Server,
            ..self
        }
    }

    fn server(&self) -> bool {
        self.authority == MovingEntityAuthority::Server
    }

    /// EntityDimensions.makeBoundingBox: half the width in float, either side of the position.
    fn box_at(&self, position: Vec3) -> Aabb {
        let (width, height) = self.kind.dimensions();
        let half = f64::from(width / 2.0);
        Aabb::new(
            Vec3::new(position.x - half, position.y, position.z - half),
            Vec3::new(
                position.x + half,
                position.y + f64::from(height),
                position.z + half,
            ),
        )
    }

    /// The move's rules: the gravity and Entity#getAirDrag's 0.98F that Entity#move's landing
    /// compensates (with Entity#getGravity) from 26.2, and no step.
    fn rules(&self, release: MinecraftVersion) -> MoveRules {
        MoveRules {
            living: false,
            max_step: 0.0,
            // (the 26.2+ landing compensates what gravity adds and drag takes)
            gravity: if self.no_gravity {
                0.0
            } else {
                self.kind.gravity(release)
            },
            air_drag: f64::from(0.98_f32),
            bounciness: 0.0,
            stands_on_lava: false,
            checks_inside_at_fall: false,
            rails_keep_speed: false,
            ignored: [None, None],
            skips_restitution: false,
            counts_fall: true,
        }
    }

    fn body(&self, release: MinecraftVersion) -> SimulationState {
        let (width, height) = self.kind.dimensions();
        // (1.16.5 Entity.java:631-632; 1.17 Entity.java:559)
        let keeps_box = !release.at_least(1, 17, 0);
        SimulationState {
            position: self.position,
            velocity: self.velocity,
            half_width: f64::from(width / 2.0),
            height: f64::from(height),
            on_ground: self.on_ground,
            collided_horizontally: self.collided_horizontally,
            collided_vertically: self.collided_vertically,
            vertical_collision_below: self.vertical_collision_below,
            in_water: self.in_water,
            in_lava: self.in_lava,
            fall_distance: self.fall_distance,
            // (the server owns these, and from 26.3 they simulate on both sides, MoveSimulationType.SERVER_AND_CLIENT:
            // 26.3 ItemEntity.java:429, PrimedTnt.java:187, FallingBlockEntity.java:362, ExperienceOrb.java:373)
            controlled_by_local_instance: self.server(),
            can_simulate_movement: Some(true),
            client_side: !self.server(),
            no_gravity: self.no_gravity,
            no_physics: self.no_physics,
            bounding_box: keeps_box
                .then(|| self.bounding_box.unwrap_or_else(|| self.box_at(self.position))),
            stuck_speed_multiplier: self.stuck_speed_multiplier,
            non_living: true,
            sweet_berry_bush_immune: true,
            // (PowderSnowBlock.getCollisionShape holds a falling block up: 1.17
            // PowderSnowBlock.java:77, 26.3 :84)
            powder_snow_walkable: self.kind == MovingEntityKind::FallingBlock,
            ..SimulationState::default()
        }
    }

    fn adopt(&mut self, body: &SimulationState) {
        self.position = body.position;
        self.velocity = body.velocity;
        self.bounding_box = body.bounding_box;
        self.on_ground = body.on_ground;
        self.collided_horizontally = body.collided_horizontally;
        self.collided_vertically = body.collided_vertically;
        self.vertical_collision_below = body.vertical_collision_below;
        self.in_water = body.in_water;
        self.in_lava = body.in_lava;
        self.fall_distance = body.fall_distance;
        self.stuck_speed_multiplier = body.stuck_speed_multiplier;
        self.move_out_of_range |= body.move_out_of_range;
    }
}

/// The tick's reading of its random inputs.
struct Draws<'a> {
    random: &'a [f32],
    used: usize,
    missing: bool,
}

impl Draws<'_> {
    fn next(&mut self) -> f32 {
        let draw = self.random.get(self.used).copied();
        self.used += 1;
        self.missing |= draw.is_none();
        draw.unwrap_or(0.0)
    }

    /// `(random.nextFloat() - random.nextFloat()) * 0.2F`, as ItemEntity#tick and ExperienceOrb#tick
    /// throw an entity out of lava.
    fn spread(&mut self) -> f64 {
        let (a, b) = (self.next(), self.next());
        f64::from((a - b) * 0.2)
    }
}

/// One tick of the entity: the level's count, then its own tick (Level#checkChunk's tickCount++ in
/// 1.12.2, then PrimedTnt#tick, FallingBlockEntity#tick, ItemEntity#tick or ExperienceOrb#tick). A
/// tick does nothing to an entity that is gone.
pub fn tick_moving_entity(
    state: &mut MovingEntityState,
    world: &mut WorldSnapshot,
    inputs: &MovingEntityInputs,
    release: MinecraftVersion,
) -> MovingEntityOutcome {
    let mut outcome = MovingEntityOutcome::default();
    if state.removed {
        return outcome;
    }
    let mut draws = Draws {
        random: &inputs.random,
        used: 0,
        missing: false,
    };
    state.tick_count = state.tick_count.wrapping_add(1);
    match state.kind {
        MovingEntityKind::PrimedTnt => tick_tnt(state, world, release, &mut outcome),
        MovingEntityKind::FallingBlock => {
            tick_falling_block(state, world, inputs, release, &mut outcome);
        }
        MovingEntityKind::Item => tick_item(state, world, &mut draws, release, &mut outcome),
        MovingEntityKind::ExperienceOrb => {
            tick_orb(state, world, inputs, &mut draws, release, &mut outcome);
        }
    }
    outcome.random_used = draws.used as u32;
    outcome.random_missing = draws.missing;
    outcome
}

/// The tick's gravity: before 1.10 nothing turns it off (1.9.4 PrimedTnt.java:57, 1.10.2 :57-59).
fn apply_gravity(state: &MovingEntityState, body: &mut SimulationState, release: MinecraftVersion) {
    if state.no_gravity && release.at_least(1, 10, 0) {
        return;
    }
    body.velocity.y -= state.kind.gravity(release);
    // from 1.14 the pull is Vec3.add(0, -gravity, 0), which turns a -0 of the other axes into +0 (1.14.4
    // ItemEntity.java:84, PrimedTnt.java:61, FallingBlockEntity.java:118, ExperienceOrb.java:74, 26.3
    // Entity.applyGravity); before, only motionY changes (1.12.2 EntityItem.onUpdate)
    if release.at_least(1, 14, 0) {
        body.velocity.x += 0.0;
        body.velocity.z += 0.0;
    }
}

/// Entity.move, with its noPhysics way out (1.8.8 Entity.java:462-466, 1.21.8 the same; 1.21.9
/// clears the collision flags, 26.3 Entity.java:689-696), and from 1.21.2 applyEffectsFromBlocks
/// after it (before, the move's own check: 1.21.3 PrimedTnt.java:83). The move's check is where
/// 1.14 - 1.16.1 keep isInLava, which the noPhysics way leaves as it was (1.14.4 Entity.java:568,
/// 1.16.1 :661). Counts the fall (checkFallDamage).
fn move_entity(
    state: &mut MovingEntityState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> VehicleMove {
    let movement = body.velocity;
    let old_position = body.position;
    if body.no_physics {
        move_without_physics(body, movement, release);
        record_movements(state, body, old_position, movement, movement, release);
        return VehicleMove {
            requested: movement,
            moved: movement,
            support: None,
            motion_at_landing: body.velocity,
            fluid_heights: None,
        };
    }
    let rules = state.rules(release);
    let outcome = move_vehicle(body, world, release, movement, rules, |body, support| {
        apply_effects_inside(body, world, support, release);
    });
    update_in_lava_after_move(body, world, release);
    let movements = record_movements(
        state,
        body,
        old_position,
        outcome.requested,
        outcome.moved,
        release,
    );
    // (1.21.1 Entity.java:738; 1.21.2 PrimedTnt.java:83, ItemEntity.java:156, FallingBlockEntity.java:137, ExperienceOrb.java:102)
    if release.at_least(1, 21, 2) {
        let (inside, _) = blocks_inside_moves(body, world, &movements, release);
        apply_block_effects(body, world, outcome.support, release, &inside);
    }
    outcome
}

/// Entity.checkFallDamage: a fall counts unless it ends here, and from 1.21.5 before that but not
/// in water (1.20.4 Entity.checkFallDamage, 1.21.5 Entity.java:1321-1331, 26.3 :1595-1607). It runs in the move
/// (MoveRules::counts_fall) before the blocks inside, whose cobweb, honey or bubble column ends the
/// fall after it was counted (1.20.4 Entity.java:757-764).
pub(crate) fn count_fall(body: &mut SimulationState, dy: f64, release: MinecraftVersion) {
    // (from 1.21.2 the move's check runs on the side that controls the entity: the server's; 1.21.1
    // Entity.java:657 calls checkFallDamage on both sides, 1.21.2 Entity.java:679-680 only where
    // !isClientSide or isControlledByLocalInstance)
    if body.client_side && release.at_least(1, 21, 2) {
        return;
    }
    let modern = release.at_least(1, 21, 5);
    if modern && dy < 0.0 && !body.in_water {
        body.fall_distance = accumulated_fall_distance(body.fall_distance, dy, release);
    }
    if body.on_ground {
        body.fall_distance = 0.0;
    } else if !modern && dy < 0.0 {
        body.fall_distance = accumulated_fall_distance(body.fall_distance, dy, release);
    }
}

/// The move's movements, kept as Entity#finalMovementsThisTick for a resting item's blocks.
fn record_movements(
    state: &mut MovingEntityState,
    body: &SimulationState,
    old_position: Vec3,
    requested: Vec3,
    moved: Vec3,
    release: MinecraftVersion,
) -> Vec<Movement> {
    let movements = movements_this_tick(body, old_position, requested, moved, release);
    state.last_movements.clone_from(&movements);
    movements
}

/// Entity's fluid update, which an item and an orb take through Entity.tick and TNT at the end of
/// its own tick: the water's current, and from 1.16 the lava's. Answers the two heights.
fn update_fluids(
    state: &MovingEntityState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (f64, f64) {
    let bounds = body.bounds();
    let mut water_height = 0.0;
    if legacy_fluid::applies(release) {
        match state.kind {
            // an item and an orb take the push over their whole box and no fall from it, and an orb
            // keeps no flag (1.8.8 ItemEntity.java:184-196, ExperienceOrb.java:152-154;
            // 1.12.2 :212, :159; gone in 1.13)
            MovingEntityKind::Item => {
                body.in_water = legacy_fluid::material_acceleration(body, world, bounds, true, release);
            }
            MovingEntityKind::ExperienceOrb => {
                legacy_fluid::material_acceleration(body, world, bounds, true, release);
            }
            _ => legacy_fluid::handle_water_movement(body, world, true, release),
        }
    } else {
        let water = entity_fluid_contact(body, world, FluidState::Water, release);
        water_height = water.height;
        body.in_water = water.touching;
        if water.touching {
            body.fall_distance = 0.0;
            push_by_fluid(body, water, 0.014, release);
        }
    }
    // (as a ridden mob's: 1.16 - 1.16.1 measure no lava in water)
    // (1.16.1 Entity.java:1042; 1.16.2 Entity.java:1068)
    // (1.16.1 Entity.java:1042; 1.16.2 Entity.java:1068)
    let measures_lava = release.at_least(1, 16, 2) || !release.at_least(1, 16, 0) || !body.in_water;
    let lava = if measures_lava {
        entity_fluid_contact(body, world, FluidState::Lava, release)
    } else {
        FluidContact::default()
    };
    // (1.15.2 Entity.java:1031; 1.16 Entity.java:1044)
    if release.at_least(1, 16, 0) {
        if measures_lava {
            push_by_fluid(body, lava, lava_flow_scale(state.ultra_warm), release);
        }
        // (1.16.1 Entity.java:1162; 1.16.2 Entity.java:1179)
        if release.at_least(1, 16, 2) {
            body.in_lava = lava.height > 0.0;
        }
    // (1.13.2 Entity.java:1143-1145; 1.14.4 Entity.java:568-569, 1167-1173)
    } else if !release.at_least(1, 14, 0) {
        body.in_lava = legacy_fluid::touching_lava(world, bounds, release);
    }
    (water_height, lava.height)
}

/// What Entity.tick does for an item and an orb: the fluids, the fall halved in lava, and the end
/// of an entity far under the world (1.8.8 Entity.java:410-413 at -64; 1.17 Entity.java:460-463
/// by the world's bottom, 26.3 Entity.java:552-608).
fn base_tick(
    state: &mut MovingEntityState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> (f64, f64) {
    let heights = update_fluids(state, body, world, release);
    // (isInLava is not on a first tick from 1.16.2, Entity.java:1179; 1.16.1 :1161 has no such test)
    if body.in_lava && !(state.first_tick && release.at_least(1, 16, 2)) {
        body.fall_distance *= 0.5;
    }
    let bottom = if release.at_least(1, 17, 0) {
        f64::from(world.world_bounds().0)
    } else {
        0.0
    };
    if body.position.y < bottom - 64.0 {
        state.removed = true;
    }
    state.first_tick = false;
    heights
}

/// PrimedTnt.tick. It takes no Entity.tick: no base tick, so no end under the world. The fuse runs
/// out at its last count from 1.9, one tick later in 1.8 (1.8.8 PrimedTnt.java:48-72, 1.9.4 :53-77,
/// 1.13.2 :58-83, 1.20.5 :63-85; from 1.21.2 the blocks' effects after the move, 1.21.3 :79-103;
/// 26.3 PrimedTnt.java:84-112). Client and server both discard it; only the server explodes.
fn tick_tnt(
    state: &mut MovingEntityState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
    outcome: &mut MovingEntityOutcome,
) {
    let mut body = state.body(release);
    apply_gravity(state, &mut body, release);
    move_entity(state, &mut body, world, release);
    drag(&mut body, release);
    if body.on_ground {
        landing_bounce(&mut body, release);
    }
    // (1.8.8 PrimedTnt.java:63; 1.9.4 PrimedTnt.java:68-69)
    let last = if release.at_least(1, 9, 0) { 1 } else { 0 };
    state.fuse = state.fuse.wrapping_sub(1);
    if state.fuse < last {
        state.removed = true;
        outcome.explodes = state.server();
    } else {
        update_fluids(state, &mut body, world, release);
    }
    state.adopt(&body);
}

/// TNT's and a falling block's drag: `*= 0.98F` before 1.14, the double 0.98 until 26.2 made it
/// the float getAirDrag (26.1 PrimedTnt.java:85, 26.2 :89; 26.2 FallingBlockEntity.java:233). (1.13.2
/// PrimedTnt.java:67-69 the floats; 1.14.4 PrimedTnt.java:65 scale(0.98D))
fn drag(body: &mut SimulationState, release: MinecraftVersion) {
    let factor = if release.at_least(1, 14, 0) && !release.at_least(26, 2, 0) {
        0.98
    } else {
        f64::from(0.98_f32)
    };
    body.velocity = body.velocity.scale(factor);
}

/// TNT's and a falling block's bounce: `0.7F, -0.5, 0.7F` before 1.14, then the doubles (1.13.2
/// PrimedTnt.java:71-73, FallingBlockEntity.java:154-156; 1.14.4 PrimedTnt.java:67, FallingBlockEntity.java:145).
fn landing_bounce(body: &mut SimulationState, release: MinecraftVersion) {
    let side = if release.at_least(1, 14, 0) {
        0.7
    } else {
        f64::from(0.7_f32)
    };
    body.velocity = Vec3::new(
        body.velocity.x * side,
        body.velocity.y * -0.5,
        body.velocity.z * side,
    );
}

/// FallingBlockEntity.tick. The client falls and does nothing else. Before 1.18.2 the first tick
/// takes the block out of the world where it finds it, and a server with no such block drops the
/// entity (1.8.8 FallingBlockEntity.java:77-93, 1.18 :103-123; from 1.18.2 FallingBlockEntity.fall
/// does that at the spawn). The drag came before the landing until 1.12 (1.11.2 :118-124, 1.12.2
/// :120-182). The block taken out is written into `world` for the tick and put back after it.
fn tick_falling_block(
    state: &mut MovingEntityState,
    world: &mut WorldSnapshot,
    inputs: &MovingEntityInputs,
    release: MinecraftVersion,
    outcome: &mut MovingEntityOutcome,
) {
    if state.carried_air {
        state.removed = true;
        return;
    }
    // (1.18.1 FallingBlockEntity.java:105; 1.18.2 FallingBlockEntity.java:120)
    let clock_era = release.at_least(1, 18, 0) && !release.at_least(1, 18, 2);
    if clock_era && !state.server() && state.discarding {
        return;
    }
    let first = state.time == 0;
    state.time = state.time.wrapping_add(1);
    // (the world with the block taken out, put back as it was when the tick ends)
    let mut world = world.patched();
    // (1.18.1 FallingBlockEntity.java:113; 1.18.2 FallingBlockEntity.java:122)
    if first && !release.at_least(1, 18, 2) {
        let cell = block_position(state.position, release);
        if world.block_cow(cell).id == state.carried_block {
            outcome.clears_block = Some(cell);
            world.set_block(cell, crate::world::BlockState::default());
        } else if state.server() {
            state.removed = true;
            return;
        }
    }
    let world: &WorldSnapshot = &world;
    let mut body = state.body(release);
    apply_gravity(state, &mut body, release);
    let start = body.position;
    move_entity(state, &mut body, world, release);
    // (1.11.2 FallingBlockEntity.java:117; 1.12.2 FallingBlockEntity.java:191)
    let drag_first = !release.at_least(1, 12, 0);
    if drag_first {
        drag(&mut body, release);
    }
    if state.server() && land_falling_block(state, &mut body, start, world, inputs, release, outcome)
    {
        state.adopt(&body);
        return;
    }
    if !drag_first {
        drag(&mut body, release);
    }
    state.adopt(&body);
}

/// The server's part of a falling block's tick. Answers whether the tick ends here, before its
/// drag. What goes into the world is the server's: the event says where. From 1.9 until 1.14 a
/// block on something free underneath it goes on falling (1.9.4 :110-113, 1.13.2 :153-157).
fn land_falling_block(
    state: &mut MovingEntityState,
    body: &mut SimulationState,
    start: Vec3,
    world: &WorldSnapshot,
    inputs: &MovingEntityInputs,
    release: MinecraftVersion,
    outcome: &mut MovingEntityOutcome,
) -> bool {
    let mut cell = block_position(body.position, release);
    // (1.11.2 FallingBlockEntity.java:94; 1.12.2 FallingBlockEntity.java:128-129)
    let concrete = state.concrete_powder && release.at_least(1, 12, 0);
    let mut stuck = concrete && is_water(world.fluid_at(cell));
    if concrete && body.velocity.length_squared() > 1.0 {
        if let Some(water) = water_along(world, start, body.position, release) {
            cell = water;
            stuck = true;
        }
    }
    if !body.on_ground && !stuck {
        // (1.8.8 :138, 1.14.4 :136 to 256; 1.17 :196 the world's bottom and height, over which the
        // top counts as the block above it until 1.21.2, 1.21.3 :156 getTopYInclusive)
        // (1.16.5 FallingBlockEntity.java:181; 1.17 FallingBlockEntity.java:196)
        let outside = if release.at_least(1, 17, 0) {
            let (bottom, height_end, ..) = world.world_bounds();
            // (1.21.1 FallingBlockEntity.java:144; 1.21.2 FallingBlockEntity.java:156)
            let top = if release.at_least(1, 21, 2) {
                height_end - 1
            } else {
                height_end
            };
            cell.1 <= bottom || cell.1 > top
        } else {
            cell.1 < 1 || cell.1 > 256
        };
        if state.time > 100 && outside || state.time > 600 {
            state.removed = true;
            outcome.expired = true;
        }
        return false;
    }
    // (1.13.2 FallingBlockEntity.java:147-150; 1.14.4 FallingBlockEntity.java:144-145; 1.8.8
    // FallingBlockEntity.java:102-106 lands on any ground, 1.9.4 FallingBlockEntity.java:118-122 not over a free block)
    let free_edge = release.at_least(1, 9, 0) && !release.at_least(1, 14, 0);
    if free_edge && !stuck && inputs.falls_through_below {
        body.on_ground = false;
        return true;
    }
    landing_bounce(body, release);
    if !world.is_moving_block(cell) {
        state.removed = true;
        outcome.landed_at = Some(cell);
    }
    false
}

/// Where the concrete powder's BlockGetter#clip ends, when the fluid there is water: FallingBlockEntity#tick
/// clips from its old position to its new one with ClipContext.Block.COLLIDER and Fluid.SOURCE_ONLY
/// and takes a hit whose cell holds water (1.14.4 FallingBlockEntity.java:128-129, 1.16.5 :133-135,
/// 1.21.11 :159-160). BlockGetter#clip walks the cells with traverseBlocks, the first one too, and in
/// each clips the block's collision shape and the fluid's shape when the fluid is a source of any
/// kind, the nearer hit winning and the block on a tie (1.14.4 BlockGetter.java:36-53, 1.21.11
/// :55-71; ClipContext.java:37-42). So a lava source ends the clip with no water, and a block stops
/// it only where the segment meets its shape (VoxelShape#clip, 1.14.4 VoxelShape.java:125-137). The
/// fluid's shape is FlowingFluid#getShape, a box of getHeight (1.14.4 FlowingFluid.java:461-473,
/// 26.1 :445-447), whose cache by fluid state is taken as computed at this cell. Before 1.14 the
/// trace is Level's own: `water_along_legacy`.
fn water_along(
    world: &WorldSnapshot,
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
) -> Option<BlockPosition> {
    if !release.at_least(1, 14, 0) {
        return water_along_legacy(world, from, to, release);
    }
    let hit = traverse_blocks(from, to, release, |cell| {
        let block = clip_block(world, cell, from, to, release, ClipShape::Collider).map(|(at, _)| at);
        let fluid = fluid_source_clip(world, cell, from, to);
        let distance = |at: Option<Vec3>| at.map_or(f64::MAX, |at| (at - from).length_squared());
        let nearer = if distance(block) <= distance(fluid) { block } else { fluid };
        nearer.map(|_| cell)
    })?;
    is_water(world.block_cow(hit).fluid).then_some(hit)
}

/// ClipContext.getFluidShape under Fluid.SOURCE_ONLY clipped by VoxelShape#clip: a source fluid's
/// box of getHeight (1.14.4 ClipContext.java:41-42, FlowingFluid.java:469-473), hit at the point
/// 0.001 along the segment when that is inside it, else where the segment enters it (1.14.4
/// VoxelShape.java:125-137); a segment shorter than sqrt(1.0E-7) hits nothing.
fn fluid_source_clip(world: &WorldSnapshot, cell: BlockPosition, from: Vec3, to: Vec3) -> Option<Vec3> {
    let block = world.block_cow(cell);
    if block.fluid == FluidState::Empty || !block.fluid_source {
        return None;
    }
    let delta = to - from;
    if delta.length_squared() < 1.0e-7 {
        return None;
    }
    let corner = Vec3::new(f64::from(cell.0), f64::from(cell.1), f64::from(cell.2));
    let shape = Aabb::new(corner, corner + Vec3::new(1.0, world.fluid_height_at(cell), 1.0));
    let inside = from + delta.scale(0.001);
    if shape.contains(inside) {
        return Some(inside);
    }
    let mut best = 1.0;
    let mut face = None;
    shape.clip_faces(from, delta, &mut best, &mut face);
    face.map(|_| from + delta.scale(best))
}

/// The concrete powder's trace before 1.14 and the cell it ends in when that is water: 1.12.2
/// Level#rayTrace(from, to, true) (FallingBlockEntity.java:130-131, Level.java:863-977) and 1.13.2
/// Level#method_3614(from, to, SOURCE_ONLY) (FallingBlockEntity.java:129-130, Level.java:719-877).
/// Both test the starting cell first, then step cell by cell for at most 201 steps, ending at the
/// end's cell. 1.12.2 stops at any block but air, fire and flowing liquid, by its bounding box
/// (Block.java:454-460, 555-556; LiquidBlock.java:181-183 a source only, its box the whole cell,
/// Block.java:95, 420), and the hit is water when the block's material is (FallingBlockEntity.java:131).
/// 1.13.2 stops at a block that has collision, by its outline shape (Block.java:469-470, 567-568),
/// or else at a source fluid by a box of its height (Level.java:739-748, FlowingFluid.java:432),
/// and the hit is water when the cell's fluid is (FallingBlockEntity.java:130), a waterlogged block too.
fn water_along_legacy(
    world: &WorldSnapshot,
    from: Vec3,
    to: Vec3,
    release: MinecraftVersion,
) -> Option<BlockPosition> {
    let before_1_13 = !release.at_least(1, 13, 0);
    let corner = |(x, y, z): BlockPosition| Vec3::new(f64::from(x), f64::from(y), f64::from(z));
    // VoxelShape#clip (1.13.2 VoxelShape.java:144-158): the start nudged by a thousandth of the
    // segment inside a box hits, else AABB.clip over the boxes
    let shape_clip = |boxes: &[Aabb], start: Vec3| {
        let delta = to - start;
        if delta.length_squared() < 1.0e-7 {
            return false;
        }
        let nudged = start + delta.scale(0.001);
        boxes.iter().any(|bounds| bounds.contains(nudged) || bounds.clip_segment(start, to).is_some())
    };
    // Some(whether the hit is water) when the trace stops in the cell
    let hit = |cell: BlockPosition, start: Vec3| -> Option<bool> {
        let block = world.block_cow(cell);
        if before_1_13 {
            if block.fluid != FluidState::Empty {
                if !block.fluid_source {
                    return None;
                }
                let whole = Aabb::new(corner(cell), corner(cell) + Vec3::new(1.0, 1.0, 1.0));
                return whole.clip_segment_legacy(start, to).map(|_| is_water(block.fluid));
            }
            let boxes = world.block_outline_boxes(cell);
            let bounds = boxes.iter().copied().reduce(Aabb::minmax)?;
            return bounds.clip_segment_legacy(start, to).map(|_| false);
        }
        let shape_hit = world.has_collision_shape(cell) && shape_clip(&world.block_outline_boxes(cell), from);
        let fluid_hit = !shape_hit && block.fluid != FluidState::Empty && block.fluid_source && {
            // a source's amount is 8: getHeight 8 / 9.0F
            let height = f64::from(8.0_f32 / 9.0_f32);
            let source = Aabb::new(corner(cell), corner(cell) + Vec3::new(1.0, height, 1.0));
            shape_clip(&[source], from)
        };
        (shape_hit || fluid_hit).then(|| is_water(block.fluid))
    };
    let floor = |value: f64| value.floor() as i32;
    let (i, j, k) = (floor(to.x), floor(to.y), floor(to.z));
    let (mut l, mut m, mut n) = (floor(from.x), floor(from.y), floor(from.z));
    let water = |cell: BlockPosition, wet: bool| wet.then_some(cell);
    if let Some(wet) = hit((l, m, n), from) {
        return water((l, m, n), wet);
    }
    let mut at = from;
    for _ in 0..=200 {
        if (l, m, n) == (i, j, k) {
            return None;
        }
        let border = |end: i32, cell: i32| match end.cmp(&cell) {
            std::cmp::Ordering::Greater => Some(f64::from(cell) + 1.0),
            std::cmp::Ordering::Less => Some(f64::from(cell)),
            std::cmp::Ordering::Equal => None,
        };
        let delta = to - at;
        let fraction = |border: Option<f64>, start: f64, d: f64| {
            let value = border.map_or(999.0, |border| (border - start) / d);
            // `g == -0.0` is true of either zero in Java, so a zero fraction is -1.0E-4 (1.12.2
            // Level.java:930-940)
            if value == 0.0 { -1.0e-4 } else { value }
        };
        let (bx, by, bz) = (border(i, l), border(j, m), border(k, n));
        let (g, h, q) = (fraction(bx, at.x, delta.x), fraction(by, at.y, delta.y), fraction(bz, at.z, delta.z));
        let (west, down, north) = (i > l, j > m, k > n);
        // which face it crossed: the cell below it on the axis when it went the negative way
        let (axis, negative) = if g < h && g < q {
            at = Vec3::new(bx.unwrap_or(999.0), at.y + delta.y * g, at.z + delta.z * g);
            (0, !west)
        } else if h < q {
            at = Vec3::new(at.x + delta.x * h, by.unwrap_or(999.0), at.z + delta.z * h);
            (1, !down)
        } else {
            at = Vec3::new(at.x + delta.x * q, at.y + delta.y * q, bz.unwrap_or(999.0));
            (2, !north)
        };
        l = floor(at.x) - i32::from(axis == 0 && negative);
        m = floor(at.y) - i32::from(axis == 1 && negative);
        n = floor(at.z) - i32::from(axis == 2 && negative);
        if let Some(wet) = hit((l, m, n), at) {
            return water((l, m, n), wet);
        }
    }
    None
}

/// Block.isShapeFullCube of the block's collision shape (1.13.2 Level.java:1526, 1.16.5 isFullCube,
/// 26.3 isCollisionShapeFullBlock); before 1.13 World.isBlockFullCube, the average edge of the
/// block's getCollisionBoundingBox at least 1 (1.8.8 Level.java:1759-1763, 1.12.2 World.java:
/// 1835-1838), which the LEGACY_FULL_CUBE tag carries.
fn full_cube(world: &WorldSnapshot, (x, y, z): BlockPosition, release: MinecraftVersion) -> bool {
    if !release.at_least(1, 13, 0) && world.block_cow((x, y, z)).tags & crate::world::LEGACY_FULL_CUBE != 0 {
        return true;
    }
    let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
    let whole = Aabb::new(min, min + Vec3::new(1.0, 1.0, 1.0));
    world
        .block_collision_boxes((x, y, z))
        .iter()
        .any(|shape| *shape == whole)
}

/// Entity.moveTowardsClosestSpace (checkInBlock until 1.16, pushOutOfBlocks before): the entity
/// goes out of the nearest open side of the cell at its middle, at a speed its random draws. Until
/// 1.14 it first asks whether it is stuck at all and answers that. 1.8 sets the speed, up before
/// north and south; 1.9 - 1.10 add it; 1.11 - 1.13 set it and keep three quarters of the rest; from
/// 1.14 north and south go first (1.8.8 Entity.java:1518-1573, 1.9.4 :1730-1782, 1.11.2 :1716-1772,
/// 1.13.2 :1912-1961, 1.14.4 :2071-2102, 26.3 :3039-3067).
fn move_towards_closest_space(
    body: &mut SimulationState,
    world: &WorldSnapshot,
    draws: &mut Draws<'_>,
    release: MinecraftVersion,
) -> bool {
    let bounds = body.bounds();
    let centre = Vec3::new(
        body.position.x,
        (bounds.min.y + bounds.max.y) / 2.0,
        body.position.z,
    );
    let cell = block_position(centre, release);
    // (1.13.2 Entity.java:1913-1915; 1.14.4 Entity.java:2072-2092; ItemEntity.java:90-93; ExperienceOrb.java:82-84)
    if !release.at_least(1, 14, 0) {
        let free = world.no_collision(bounds, release);
        let stuck = if release.at_least(1, 9, 0) {
            !free
        } else {
            !free || full_cube(world, cell, release)
        };
        if !stuck {
            return false;
        }
    }
    let inside = Vec3::new(
        centre.x - f64::from(cell.0),
        centre.y - f64::from(cell.1),
        centre.z - f64::from(cell.2),
    );
    const NORTH: (i32, i32, i32) = (0, 0, -1);
    const SOUTH: (i32, i32, i32) = (0, 0, 1);
    const WEST: (i32, i32, i32) = (-1, 0, 0);
    const EAST: (i32, i32, i32) = (1, 0, 0);
    const UP: (i32, i32, i32) = (0, 1, 0);
    // (1.13.2 Entity.java:1924-1947; 1.14.4 Entity.java:2078)
    let order = if release.at_least(1, 14, 0) {
        [NORTH, SOUTH, WEST, EAST, UP]
    } else if release.at_least(1, 9, 0) {
        [WEST, EAST, NORTH, SOUTH, UP]
    } else {
        [WEST, EAST, UP, NORTH, SOUTH]
    };
    let mut closest = if release.at_least(1, 9, 0) {
        f64::MAX
    } else {
        9999.0
    };
    let mut direction = UP;
    for side in order {
        if full_cube(world, (cell.0 + side.0, cell.1 + side.1, cell.2 + side.2), release) {
            continue;
        }
        let along = if side.0 != 0 {
            inside.x
        } else if side.1 != 0 {
            inside.y
        } else {
            inside.z
        };
        let distance = if side.0 + side.1 + side.2 > 0 {
            1.0 - along
        } else {
            along
        };
        if distance < closest {
            closest = distance;
            direction = side;
        }
    }
    let speed = draws.next() * 0.2 + 0.1;
    let step = (direction.0 + direction.1 + direction.2) as f32;
    let push = f64::from(step * speed);
    let velocity = body.velocity;
    // (1.10.2 Entity.java:1776-1780; 1.11.2 Entity.java:1863-1873)
    if release.at_least(1, 9, 0) && !release.at_least(1, 11, 0) {
        body.velocity = Vec3::new(
            velocity.x + if direction.0 != 0 { push } else { 0.0 },
            velocity.y + if direction.1 != 0 { push } else { 0.0 },
            velocity.z + if direction.2 != 0 { push } else { 0.0 },
        );
        return true;
    }
    // (0.75 from 1.11.2 Entity.java:1863-1873; 1.9 and 1.10 returned above)
    let kept = if release.at_least(1, 11, 0) { 0.75 } else { 1.0 };
    body.velocity = Vec3::new(
        if direction.0 != 0 { push } else { velocity.x * kept },
        if direction.1 != 0 { push } else { velocity.y * kept },
        if direction.2 != 0 { push } else { velocity.z * kept },
    );
    true
}

/// The block whose friction an item or an orb on the ground slides on: a whole block under its box
/// before 1.15, under its position until 1.20, then getOnPos(0.999999F), from the supporting block
/// (1.8.8 ItemEntity.java:94-97, 1.14.4 ItemEntity.java:100, 1.15 :100, 1.20 :137, :176).
fn friction_cell(
    world: &WorldSnapshot,
    body: &SimulationState,
    support: Option<BlockPosition>,
    release: MinecraftVersion,
) -> BlockPosition {
    if release.at_least(1, 20, 0) {
        return on_position(world, body, support, 0.999_999, release);
    }
    let y = if release.at_least(1, 15, 0) {
        body.position.y
    } else {
        body.bounds().min.y
    };
    block_position(Vec3::new(body.position.x, y - 1.0, body.position.z), release)
}

/// `velocity + 5.0E-4F` below 0.06F, the sideways motion at `side` (1.13.2 ItemEntity.java:146-153,
/// 1.14.4 :140-143, 26.3 :184-194).
fn float_item(velocity: Vec3, side: f32) -> Vec3 {
    let side = f64::from(side);
    let rise = if velocity.y < f64::from(0.06_f32) {
        f64::from(5.0e-4_f32)
    } else {
        0.0
    };
    Vec3::new(velocity.x * side, velocity.y + rise, velocity.z * side)
}

/// ItemEntity.tick. The client keeps the age but never ends the item: the server's removal does.
/// Water lifts it from 1.13, by the fluid at its eyes; lava from 1.16; from 1.16.2 by the fluid's
/// height over 0.1 of a block. It moves every tick to 1.13.2; from 1.14.4 it rests three ticks in
/// four once settled, and from 26.1 still takes the blocks of its last move. Up to 1.16.5 lava in
/// its cell throws it about, on the client too, by the random (1.8.8 ItemEntity.java:65-116, 1.11.2
/// :76, 1.13.2 :71, 1.14.4 ItemEntity.java:67-140, 1.15 :72, 1.16 :89, 1.16.5 :89, 1.17 :86,
/// 1.20.5 :115, 26.3 :108-171).
fn tick_item(
    state: &mut MovingEntityState,
    world: &WorldSnapshot,
    draws: &mut Draws<'_>,
    release: MinecraftVersion,
    outcome: &mut MovingEntityOutcome,
) {
    if state.item_empty {
        state.removed = true;
        return;
    }
    let mut body = state.body(release);
    let (water_height, lava_height) = base_tick(state, &mut body, world, release);
    if state.pickup_delay > 0 && state.pickup_delay != 32767 {
        state.pickup_delay -= 1;
    }
    let start = body.position;
    let floats_in = |body: &SimulationState, fluid: FluidState, height: f64| {
        // (1.16.1 ItemEntity.java:101; 1.16.2 ItemEntity.java:102)
        if release.at_least(1, 16, 2) {
            // (1.20.4 ItemEntity.java:145; 1.20.5 ItemEntity.java:136)
            let threshold = if release.at_least(1, 20, 5) {
                0.1_f32
            } else {
                0.25_f32 * 0.85_f32 - 0.111_111_11_f32
            };
            let touching = if fluid == FluidState::Water {
                body.in_water
            } else {
                body.in_lava
            };
            touching && height > f64::from(threshold)
        } else {
            entity_eyes_in_fluid(body, world, fluid, release)
        }
    // (1.12.2 ItemEntity.java:89-90; 1.13.2 ItemEntity.java:86-87,150-157)
    };
    if release.at_least(1, 13, 0) && floats_in(&body, FluidState::Water, water_height) {
        body.velocity = float_item(body.velocity, 0.99);
    // (1.15.2 ItemEntity.java:85; 1.16 ItemEntity.java:103)
    } else if release.at_least(1, 16, 0) && floats_in(&body, FluidState::Lava, lava_height) {
        body.velocity = float_item(body.velocity, 0.95);
    } else {
        apply_gravity(state, &mut body, release);
    }
    // (both sides until 1.11: 1.10.2 ItemEntity.java:76, 1.11.2 :95-99)
    if state.server() || !release.at_least(1, 11, 0) {
        if release.at_least(1, 14, 0) {
            let query = if release.at_least(1, 17, 0) {
                body.bounds().deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))
            } else {
                body.bounds()
            };
            let context = entity_collision_context(&body);
            let free = world.no_collision_for_entity(
                query,
                collision_query_entity(&body, &context),
                release,
            );
            body.no_physics = !free;
            if body.no_physics {
                move_towards_closest_space(&mut body, world, draws, release);
            }
        } else {
            body.no_physics = move_towards_closest_space(&mut body, world, draws, release);
        }
    } else {
        body.no_physics = false;
    }
    state.no_physics = body.no_physics;
    // (1.14.4 ItemEntity.java:96, 1.15 :100; not in 1.14 itself, or in 1.13.2, which are not this rule)
    let rests = release.at_least(1, 14, 4)
        && body.on_ground
        && !(horizontal_speed_squared(body.velocity) > f64::from(1.0e-5_f32))
        && state.tick_count.wrapping_add(state.id) % 4 != 0;
    if rests {
        // (1.21.11 ItemEntity.java:133-135 takes the block effects only when it moves; 26.1 ItemEntity.java:136-137
        // also those of its last movements when it rests)
        if release.at_least(26, 1, 0) && !body.no_physics {
            let (inside, _) = blocks_inside_moves(&body, world, &state.last_movements, release);
            apply_block_effects(&mut body, world, None, release, &inside);
        }
    } else {
        let moved = move_entity(state, &mut body, world, release);
        // (before 1.14 the lava and the merge come before the friction: 1.13.2 ItemEntity.java:100-114,
        // 1.14.4 ItemEntity.java:104-117 after it)
        if !release.at_least(1, 14, 0) {
            item_check(state, &mut body, start, world, draws, release, outcome);
        }
        let friction = if body.on_ground {
            world.slipperiness_at(friction_cell(world, &body, moved.support, release)) as f32 * 0.98
        } else {
            0.98
        };
        // (26.1.2 ItemEntity.java:157 the double 0.98; 26.2 ItemEntity.java:142-148 the float getAirDrag)
        let vertical = if release.at_least(1, 14, 0) && !release.at_least(26, 2, 0) {
            0.98
        } else {
            f64::from(0.98_f32)
        };
        body.velocity = Vec3::new(
            body.velocity.x * f64::from(friction),
            body.velocity.y * vertical,
            body.velocity.z * f64::from(friction),
        // (1.16.1 ItemEntity.java:126-127; 1.16.2 ItemEntity.java:127-130)
        );
        if body.on_ground && (body.velocity.y < 0.0 || !release.at_least(1, 16, 2)) {
            body.velocity.y *= -0.5;
        }
    }
    // (1.13.2 ItemEntity.java:99-127; 1.14.4 ItemEntity.java:96-124)
    if release.at_least(1, 14, 0) {
        item_check(state, &mut body, start, world, draws, release, outcome);
    }
    if state.age != -32768 {
        state.age = state.age.wrapping_add(1);
    }
    update_fluids(state, &mut body, world, release);
    if state.server() && state.age >= 6000 {
        state.removed = true;
    }
    state.adopt(&body);
}

/// The item's look around, every so often: until 1.17 for lava in its cell, and for a merge.
fn item_check(
    state: &MovingEntityState,
    body: &mut SimulationState,
    start: Vec3,
    world: &WorldSnapshot,
    draws: &mut Draws<'_>,
    release: MinecraftVersion,
    outcome: &mut MovingEntityOutcome,
) {
    let moved_cell = moved_to_another_cell(start, body.position, release);
    // (1.13.2 ItemEntity.java:99-100; 1.14.4 ItemEntity.java:109-111)
    let due = if release.at_least(1, 14, 0) {
        state.tick_count % if moved_cell { 2 } else { 40 } == 0
    } else {
        moved_cell || state.tick_count % 25 == 0
    };
    if due {
        // (1.15.2 ItemEntity.java:118-122 throws it up out of lava and plays the burn; 1.16.5 ItemEntity.java:139-141
        // only plays the burn; 1.17 ItemEntity.java:137-138 looks for no lava)
        if !release.at_least(1, 17, 0) && lava_in_cell(body, world, release) {
            // (1.15.2 :118-120 throws it; 1.16 ItemEntity.java:136-137 leaves its motion alone; the burn's sound
            // takes a draw on both sides, from 1.16 only for an item that is not fire immune: 1.16
            // ItemEntity.java:133-134, 1.16.5 :137-138 `&& !this.isFireImmune()`)
            if !release.at_least(1, 16, 0) {
                scatter_in_lava(body, draws);
                draws.next();
            } else if !state.fire_immune {
                draws.next();
            }
        }
        outcome.merge_check = state.server();
    }
}

/// Vec3#horizontalDistanceSqr of the motion (Entity#getHorizontalDistanceSqr before 1.17).
fn horizontal_speed_squared(velocity: Vec3) -> f64 {
    velocity.x * velocity.x + velocity.z * velocity.z
}

/// `(int)` of the coordinates until 1.14, Mth.floor from it (1.13.2 ItemEntity.java:100, 1.14.4
/// ItemEntity.java:111).
fn moved_to_another_cell(from: Vec3, to: Vec3, release: MinecraftVersion) -> bool {
    let cell = |value: f64| {
        if release.at_least(1, 14, 0) {
            floor_i32_for(release, value)
        } else {
            value as i32
        }
    };
    cell(from.x) != cell(to.x) || cell(from.y) != cell(to.y) || cell(from.z) != cell(to.z)
}

/// The fluid of the block at the entity's position is lava: ItemEntity#tick's and ExperienceOrb#tick's
/// test before they throw it out.
fn lava_in_cell(body: &SimulationState, world: &WorldSnapshot, release: MinecraftVersion) -> bool {
    world.fluid_at(block_position(body.position, release)) == FluidState::Lava
}

/// `setDeltaMovement((a - b) * 0.2F, 0.2F, (c - d) * 0.2F)`, x's draws first (ItemEntity#tick,
/// ExperienceOrb#tick).
fn scatter_in_lava(body: &mut SimulationState, draws: &mut Draws<'_>) {
    let x = draws.spread();
    let z = draws.spread();
    body.velocity = Vec3::new(x, f64::from(0.2_f32), z);
}

/// Level.getClosestPlayer(entity, 8.0): the nearest player that is no spectator, strictly nearer
/// than 8 blocks (1.8.8 Level.java:2401-2417).
fn nearest_player(players: &[MovingEntityPlayer], at: Vec3) -> Option<MovingEntityPlayer> {
    let mut nearest: Option<(f64, MovingEntityPlayer)> = None;
    for player in players.iter().filter(|player| !player.spectator) {
        let distance = player.position.distance_squared(at);
        if distance < 64.0 && nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, *player));
        }
    }
    nearest.map(|(_, player)| player)
}

/// ExperienceOrb.tick. Before 1.17 the orb keeps its own tick count and looks for a player when
/// that count runs ahead of its last look; from 1.17 once in 20 of Entity's ticks; from 1.21.5
/// every tick, and the client's first tick does nothing. Water lifts it from 1.13; lava throws it,
/// on the client too. It leaves a block on both sides until 1.21.5, then on the server alone when
/// no player draws it, and from 1.21.6 only if its motion would not take it out. A landing bounces
/// at 0.9 until 1.21.4, then 0.4 of the motion it landed with, and from 1.21.5 only from a real
/// fall (1.8.8 ExperienceOrb.java:85-167, 1.13.2 :89, 1.14.4 ExperienceOrb.java:91, 1.15 :73,
/// 1.17 :56, 1.21.3 :66, 1.21.4 :66, 1.21.5 ExperienceOrb.java:78-124, 1.21.6 :99, 26.3 :96-142).
fn tick_orb(
    state: &mut MovingEntityState,
    world: &WorldSnapshot,
    inputs: &MovingEntityInputs,
    draws: &mut Draws<'_>,
    release: MinecraftVersion,
    outcome: &mut MovingEntityOutcome,
) {
    let modern = release.at_least(1, 21, 5);
    if modern && state.first_tick && !state.server() {
        state.first_tick = false;
        return;
    }
    let mut body = state.body(release);
    base_tick(state, &mut body, world, release);
    // (1.16.5 ExperienceOrb.java:78; 1.17 ExperienceOrb.java:58)
    if !release.at_least(1, 17, 0) && state.pickup_delay > 0 {
        state.pickup_delay -= 1;
    }
    let colliding = !world.no_collision(body.bounds(), release);
    if release.at_least(1, 13, 0) && entity_eyes_in_fluid(&body, world, FluidState::Water, release)
    {
        let v = body.velocity;
        body.velocity = Vec3::new(
            v.x * f64::from(0.99_f32),
            (v.y + f64::from(5.0e-4_f32)).min(f64::from(0.06_f32)),
            v.z * f64::from(0.99_f32),
        );
    } else if !modern || !colliding {
        apply_gravity(state, &mut body, release);
    }
    if lava_in_cell(&body, world, release) {
        scatter_in_lava(&mut body, draws);
        // (the burn's sound takes a draw until 1.17: 1.16.5 ExperienceOrb.java:94)
        if !release.at_least(1, 17, 0) {
            draws.next();
        }
    }
    if !release.at_least(1, 14, 0) || (!modern && !world.no_collision(body.bounds(), release)) {
        move_towards_closest_space(&mut body, world, draws, release);
    }
    if release.at_least(1, 17, 0) && state.tick_count % 20 == 1 {
        outcome.merge_check = state.server();
    }
    follow_player(state, &mut body, inputs, release);
    // (1.21.5 ExperienceOrb.java:98-99; 1.21.6 ExperienceOrb.java:121-124)
    if modern && state.following_player.is_none() && state.server() && colliding {
        let blocked = !release.at_least(1, 21, 6)
            || !world.no_collision(body.bounds().translated(body.velocity), release);
        if blocked {
            move_towards_closest_space(&mut body, world, draws, release);
        }
    }
    let falling = body.velocity.y;
    let moved = move_entity(state, &mut body, world, release);
    let friction = if body.on_ground {
        world.slipperiness_at(friction_cell(world, &body, moved.support, release)) as f32 * 0.98
    } else {
        0.98
    };
    let vertical = if modern {
        f64::from(friction)
    // (1.13.2 ExperienceOrb.java:147; 1.14.4 ExperienceOrb.java:114)
    } else if release.at_least(1, 14, 0) {
        0.98
    } else {
        f64::from(0.98_f32)
    };
    body.velocity = Vec3::new(
        body.velocity.x * f64::from(friction),
        body.velocity.y * vertical,
        body.velocity.z * f64::from(friction),
    );
    if modern {
        let gravity = if state.no_gravity {
            0.0
        } else {
            state.kind.gravity(release)
        };
        if body.vertical_collision_below && falling < -gravity {
            body.velocity.y = -falling * 0.4;
        }
    // (1.21.3 ExperienceOrb.java:110; 1.21.4 ExperienceOrb.java:100,110)
    } else if body.on_ground {
        if release.at_least(1, 21, 4) {
            body.velocity.y = -falling * 0.4;
        // (1.13.2 ExperienceOrb.java:150; 1.14.4 ExperienceOrb.java:116)
        } else if release.at_least(1, 14, 0) {
            body.velocity.y *= -0.9;
        } else {
            body.velocity.y *= f64::from(-0.9_f32);
        }
    }
    state.orb_ticks = state.orb_ticks.wrapping_add(1);
    state.age = state.age.wrapping_add(1);
    if state.age >= 6000 {
        state.removed = true;
    }
    state.adopt(&body);
}

/// The orb's choice of a player and the pull of the one it has.
fn follow_player(
    state: &mut MovingEntityState,
    body: &mut SimulationState,
    inputs: &MovingEntityInputs,
    release: MinecraftVersion,
) {
    let at = body.position;
    let mut target = state
        .following_player
        .and_then(|id| inputs.players.iter().find(|player| player.id == id).copied());
    // (1.21.4 ExperienceOrb.java:83-87; 1.21.5 ExperienceOrb.java:125-131)
    let far = target.is_none_or(|player| player.position.distance_squared(at) > 64.0);
    if release.at_least(1, 21, 5) {
        if far || target.is_some_and(|player| player.spectator) {
            target = nearest_player(&inputs.players, at).filter(|player| !player.dead);
        // (1.16.5 ExperienceOrb.java:100-108; 1.17 ExperienceOrb.java:77-81)
        }
    } else if release.at_least(1, 17, 0) {
        if state.tick_count % 20 == 1 && far {
            target = nearest_player(&inputs.players, at);
        }
        if target.is_some_and(|player| player.spectator || player.dead) {
            target = None;
        }
    } else {
        if state.following_time < state.orb_ticks.wrapping_sub(20).wrapping_add(state.id % 100) {
            if far {
                target = nearest_player(&inputs.players, at);
            }
            state.following_time = state.orb_ticks;
        }
        if target.is_some_and(|player| player.spectator) {
            target = None;
        }
    }
    state.following_player = target.map(|player| player.id);
    let Some(player) = target else { return };
    // (1.13.2 ExperienceOrb.java:125-137; 1.14.4 ExperienceOrb.java:99-106)
    if release.at_least(1, 14, 0) {
        let to = Vec3::new(
            player.position.x - at.x,
            player.position.y + f64::from(player.eye_height) / 2.0 - at.y,
            player.position.z - at.z,
        );
        let distance = to.length_squared();
        if distance < 64.0 || release.at_least(1, 21, 5) {
            let power = 1.0 - distance.sqrt() / 8.0;
            body.velocity = body.velocity + to.normalized(release).scale(power * power * 0.1);
        }
        return;
    }
    // (the whole of the eye height in 1.8, half from 1.9: 1.8.8 ExperienceOrb.java:118, 1.8.8
    // ExperienceOrb.java:118; 1.9.4 ExperienceOrb.java:123)
    let eye = if release.at_least(1, 9, 0) {
        f64::from(player.eye_height) / 2.0
    } else {
        f64::from(player.eye_height)
    };
    let (x, y, z) = (
        (player.position.x - at.x) / 8.0,
        (player.position.y + eye - at.y) / 8.0,
        (player.position.z - at.z) / 8.0,
    );
    let length = (x * x + y * y + z * z).sqrt();
    let share = 1.0 - length;
    if share > 0.0 {
        let (share, pull) = (share * share, 0.1);
        body.velocity = body.velocity
            + Vec3::new(
                x / length * share * pull,
                y / length * share * pull,
                z / length * share * pull,
            );
    }
}

/// The mobs whose `travel` is their own. Their AI stays with the server: the control and the motion
/// a travel starts from are the caller's. (The happy ghast and the nautilus a player controls take
/// ridden.rs.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobTravelKind {
    Squid,
    /// AbstractFish.
    Fish,
    /// Guardian and ElderGuardian.
    Guardian,
    Ghast,
    Phantom,
    Allay,
    Vex,
    /// Any LivingEntity with nothing of its own in `travel`: a blaze, a zombie. A blaze's slow fall
    /// is its aiStep's before the travel, and comes in as motion.
    Living,
    /// LivingEntity's travel, the motion its AI gives it, and no fall counted.
    Bat,
    /// FlyingAnimal: a bee, a parrot.
    FlyingAnimal,
    Dolphin,
    Turtle,
    Axolotl,
    Frog,
    Drowned,
    /// Spider and CaveSpider: its own climbing, and no cobweb.
    Spider,
    PolarBear,
    /// WitherBoss: nothing makes it stuck.
    Wither,
    ArmorStand,
    /// A mob that can be ridden, out of a saddle: ridden.rs's rules of its kind.
    Mount(crate::ridden::RiddenMobKind),
    SulfurCube,
}

impl MobTravelKind {
    /// The entity's registry name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "squid" | "glow_squid" => Some(Self::Squid),
            "cod" | "salmon" | "tropical_fish" | "pufferfish" | "tadpole" => Some(Self::Fish),
            "guardian" | "elder_guardian" => Some(Self::Guardian),
            "ghast" => Some(Self::Ghast),
            "phantom" => Some(Self::Phantom),
            "allay" => Some(Self::Allay),
            "vex" => Some(Self::Vex),
            "living" | "blaze" | "zombie" | "cow" => Some(Self::Living),
            "bat" => Some(Self::Bat),
            "bee" | "parrot" => Some(Self::FlyingAnimal),
            "dolphin" => Some(Self::Dolphin),
            "turtle" => Some(Self::Turtle),
            "axolotl" => Some(Self::Axolotl),
            "frog" => Some(Self::Frog),
            "drowned" => Some(Self::Drowned),
            "spider" | "cave_spider" => Some(Self::Spider),
            "polar_bear" => Some(Self::PolarBear),
            "wither" => Some(Self::Wither),
            "armor_stand" => Some(Self::ArmorStand),
            "strider" => Some(Self::Mount(crate::ridden::RiddenMobKind::Strider)),
            "skeleton_horse" => Some(Self::Mount(crate::ridden::RiddenMobKind::SkeletonHorse)),
            "sulfur_cube" => Some(Self::SulfurCube),
            // (any other LivingEntity of the inventory has nothing of its own: entity_travel.rs)
            other => (travel_support(other, MinecraftVersion::new(u16::MAX, 0, 0)).family == Some(TravelFamily::Mob)).then_some(Self::Living),
        }
    }

    /// Entity.isPushedByFluid of a mob that is not swimming: WaterAnimal, AgeableWaterCreature,
    /// Axolotl and Turtle say no for good (26.3 WaterAnimal.java:59, AgeableWaterCreature.java:53,
    /// Axolotl.java:318, Turtle.java:158).
    #[must_use]
    pub const fn pushed_by_fluid(self) -> bool {
        !matches!(
            self,
            Self::Squid | Self::Fish | Self::Dolphin | Self::Axolotl | Self::Turtle
        )
    }

    /// An empty checkFallDamage (FlyingMob#checkFallDamage, Bat#checkFallDamage, Parrot#checkFallDamage,
    /// Bee#checkFallDamage, Allay#checkFallDamage, Ghast#checkFallDamage, Phantom#checkFallDamage):
    /// FlyingMob until 26.1 (then Ghast and Phantom), Bat, Parrot, Bee
    /// and Allay (1.8.8 FlyingMob.java, Bat.java; 1.12.2 Parrot.java, 1.15
    /// Bee.java, 1.20.4 Allay.java; 26.3 Ghast.java, Phantom.java, Allay.java, Bat.java,
    /// Bee.java, Parrot.java).
    const fn no_fall_check(self) -> bool {
        matches!(
            self,
            Self::Ghast | Self::Phantom | Self::Allay | Self::Bat | Self::FlyingAnimal
        )
    }

    /// LivingEntity.getWaterSlowDown where it is not the 0.8F of every mob (1.10.2
    /// PolarBear.java; 26.3 PolarBear.java:219).
    const fn water_slow_down(self) -> Option<f32> {
        match self {
            Self::PolarBear => Some(0.98),
            _ => None,
        }
    }
}

/// A mob where its AI left it: what `LivingEntity.travel` starts from.
#[derive(Clone, Debug, PartialEq)]
pub struct MobTravelState {
    pub kind: MobTravelKind,
    pub position: Vec3,
    /// The motion after its AI (a move control or a goal may have set it).
    pub velocity: Vec3,
    /// Entity.getYRot, in degrees: what the input is turned by.
    pub yaw: f32,
    /// xxa, yya, zza.
    pub input: Vec3,
    /// LivingEntity.getSpeed.
    pub speed: f32,
    pub half_width: f64,
    pub height: f64,
    pub max_step: f32,
    pub bounding_box: Option<Aabb>,
    pub on_ground: bool,
    pub collided_horizontally: bool,
    pub collided_vertically: bool,
    pub in_water: bool,
    pub in_lava: bool,
    pub no_gravity: bool,
    pub stuck_speed_multiplier: Vec3,
    pub fall_distance: f64,
    /// Mob.getTarget is set: a fish or guardian then does not sink.
    pub has_target: bool,
    /// Guardian.isMoving: its spikes are out, and it does not sink.
    pub guardian_moving: bool,
    /// A move past MAX_WALKED_CELLS did not run, and the bindings refuse the tick (`move_vehicle`).
    pub move_out_of_range: bool,
    /// Spider.isClimbing: its own onClimbable, in place of the block it is in.
    pub climbing: bool,
    /// Entity.isPushedByFluid.
    pub pushed_by_fluid: bool,
    /// Entity.getFluidJumpThreshold.
    pub fluid_jump_threshold: f64,
    /// Entity.isUnderWater.
    pub under_water: bool,
    /// Drowned.wantsToSwim.
    pub wants_to_swim: bool,
    /// Turtle.goingHome, and within 20 blocks of it: it does not sink.
    pub near_home: bool,
    /// ArmorStand.isMarker.
    pub marker: bool,
    /// Fox and Bee: a sweet berry bush does not hold them.
    pub sweet_berry_bush_immune: bool,
    /// SulfurCube.hasBodyItem.
    pub body_item: bool,
    /// SulfurCube.floatsInLiquids.
    pub floats_in_liquids: bool,
    /// Entity.tickCount: what a floating sulfur cube bobs by.
    pub tick_count: i32,
    /// MobEffects.SLOW_FALLING.
    pub slow_falling: bool,
    /// MobEffects.LEVITATION's amplifier.
    pub levitation_amplifier: Option<i32>,
    /// MobEffects.DOLPHINS_GRACE.
    pub dolphins_grace: bool,
    /// Attributes.GRAVITY, from 1.20.5.
    pub gravity: Option<f64>,
    /// Attributes.WATER_MOVEMENT_EFFICIENCY, from 1.21.
    pub water_movement_efficiency: f64,
    /// Attributes.AIR_DRAG_MODIFIER, from 26.2 (absent is 1).
    pub air_drag_modifier: Option<f64>,
    /// LivingEntity.shouldDiscardFriction, from 1.17: set by a goat's or frog's long jump and a
    /// breeze's jump (1.17 LongJumpMidJump; 1.19 frog; 1.20.5 Breeze).
    pub discard_friction: bool,
}

/// What the rest of `travel` needs of the mob's own rules.
fn plain_of(state: &MobTravelState) -> PlainMob {
    PlainMob {
        yaw: state.yaw,
        max_step: state.max_step,
        pushed_by_fluid: state.pushed_by_fluid,
        fluid_jump_threshold: state.fluid_jump_threshold,
        climbing: (state.kind == MobTravelKind::Spider).then_some(state.climbing),
        water_slow_down: state.kind.water_slow_down(),
        omnidirectional: state.kind == MobTravelKind::FlyingAnimal
            || (state.kind == MobTravelKind::SulfurCube && state.body_item),
        no_fall_check: state.kind.no_fall_check(),
        inside_after_move: state.kind == MobTravelKind::Vex,
        discard_friction: state.discard_friction,
        slow_falling: state.slow_falling,
        levitation_amplifier: state.levitation_amplifier,
        dolphins_grace: state.dolphins_grace,
        gravity: state.gravity,
        water_movement_efficiency: state.water_movement_efficiency,
    }
}

/// The speed of a mob's own swim, where it swims: LivingEntity.travel's fluid branch is theirs in
/// water (1.13.2 AbstractFish.java:115-129, Dolphin.java:377-390, Turtle.java:311-324,
/// 1.17 AxolotlEntity, 1.19 FrogEntity, 26.3 Dolphin.java, Turtle.java, Axolotl.java, Frog.java). A
/// drowned swims only to where it wants: in water, under it from 1.21.2 (1.21 Drowned.java,
/// 1.21.3 :201-203, 26.3 Drowned.java).
fn swim_speed(state: &MobTravelState, in_water: bool, release: MinecraftVersion) -> Option<f32> {
    match state.kind {
        MobTravelKind::Fish if in_water => Some(0.01),
        MobTravelKind::Guardian | MobTravelKind::Turtle if in_water => Some(0.1),
        MobTravelKind::Dolphin | MobTravelKind::Axolotl | MobTravelKind::Frog if in_water => {
            Some(state.speed)
        }
        MobTravelKind::Drowned if state.wants_to_swim => {
            // (1.21.1 Drowned.java:201-202 isInWater; 1.21.2 Drowned.java:205-206 isUnderWater)
            let submerged = if release.at_least(1, 21, 2) {
                state.under_water
            } else {
                in_water
            };
            submerged.then_some(0.01)
        }
        _ => None,
    }
}

/// Whether the swim ends with the sink of 0.005 (AbstractFish#travel, Dolphin#travel,
/// Guardian#travel, Turtle#travel).
fn sinks(state: &MobTravelState) -> bool {
    match state.kind {
        MobTravelKind::Fish | MobTravelKind::Dolphin => !state.has_target,
        MobTravelKind::Guardian => !state.guardian_moving && !state.has_target,
        MobTravelKind::Turtle => !state.has_target && !state.near_home,
        _ => false,
    }
}

/// ArmorStand.travel: only with its physics, which a marker lost at 1.14 (1.13.2
/// ArmorStand.java, 1.14.4 :97, 26.3 ArmorStand.java).
fn armor_stand_travels(state: &MobTravelState, release: MinecraftVersion) -> bool {
    !state.no_gravity && !(state.marker && release.at_least(1, 14, 0))
}

/// LivingEntity.travel of a mob with nothing of its own left in it, and a sulfur cube's float in a
/// fluid after it (26.3 SulfurCube.java:176-185).
fn travel_living(
    state: &MobTravelState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> Option<VehicleMove> {
    let bounds = body.bounds();
    let heights = (
        world.fluid_contact(FluidState::Water, bounds, release).height,
        world.fluid_contact(FluidState::Lava, bounds, release).height,
    );
    let in_fluid = body.in_water || body.in_lava;
    let in_water = body.in_water;
    let kind = match state.kind {
        MobTravelKind::Mount(kind) => kind,
        _ => crate::ridden::RiddenMobKind::Pig,
    };
    let moved = crate::ridden::plain_living_travel(
        kind,
        state.position,
        state.speed,
        state.input,
        body,
        world,
        release,
        plain_of(state),
    );
    if state.kind == MobTravelKind::SulfurCube && state.body_item && state.floats_in_liquids && in_fluid {
        let (water, lava) = moved
            .and_then(|moved| moved.fluid_heights)
            .unwrap_or(heights);
        let vibe = 0.2 * crate::simulation::mth_sin(f64::from(state.tick_count as f32 * 0.4), release) as f32;
        let height = if in_water { water } else { lava };
        let immersion = height - state.fluid_jump_threshold + f64::from(vibe);
        if immersion > 0.0 {
            body.velocity = body.velocity + Vec3::new(0.0, immersion.min(1.0) * f64::from(0.04_f32), 0.0);
        }
    }
    moved
}

/// One LivingEntity.travel of the mob, with what its aiStep does right after it from 1.21.2:
/// applyEffectsFromBlocks. Nothing else of its tick: its AI before, and what its aiStep does later
/// (a squid's next motion), are the server's.
pub fn tick_mob_travel(state: &mut MobTravelState, world: &WorldSnapshot, release: MinecraftVersion) {
    // (1.16.5 Entity.java:631-632; 1.17 Entity.java:559)
    let keeps_box = !release.at_least(1, 17, 0);
    let is_vex = state.kind == MobTravelKind::Vex;
    let mut body = SimulationState {
        position: state.position,
        velocity: state.velocity,
        half_width: state.half_width,
        height: state.height,
        on_ground: state.on_ground,
        collided_horizontally: state.collided_horizontally,
        collided_vertically: state.collided_vertically,
        in_water: state.in_water,
        in_lava: state.in_lava,
        no_gravity: state.no_gravity,
        // (Vex.tick lets it through all blocks: 1.11.2 EntityVex, 26.3 Vex.java:71-76)
        no_physics: is_vex,
        web_immune: state.kind == MobTravelKind::Spider,
        stuck_immune: state.kind == MobTravelKind::Wither,
        sweet_berry_bush_immune: state.sweet_berry_bush_immune,
        fall_distance: state.fall_distance,
        controlled_by_local_instance: true,
        stuck_speed_multiplier: state.stuck_speed_multiplier,
        bounding_box: keeps_box.then_some(state.bounding_box).flatten(),
        ..SimulationState::default()
    };
    if keeps_box && body.bounding_box.is_none() {
        let half = state.half_width;
        let at = state.position;
        body.bounding_box = Some(Aabb::new(
            Vec3::new(at.x - half, at.y, at.z - half),
            Vec3::new(at.x + half, at.y + state.height, at.z + half),
        ));
    }
    let old_position = body.position;
    let swims = swim_speed(state, body.in_water, release);
    let moved = match (state.kind, swims) {
        (MobTravelKind::Squid, _) => Some(mob_move(state, &mut body, world, release)),
        (_, Some(speed)) => {
            mob_relative(state, &mut body, speed, release);
            let moved = mob_move(state, &mut body, world, release);
            // (float before 1.14: 1.8.8 Guardian.java:travel, 1.13.2 Dolphin.java:381,
            // 1.14 the double)
            body.velocity = body.velocity.scale(if release.at_least(1, 14, 0) {
                0.9
            } else {
                f64::from(0.9_f32)
            });
            if sinks(state) {
                body.velocity.y -= 0.005;
            }
            Some(moved)
        }
        (MobTravelKind::Ghast | MobTravelKind::Phantom | MobTravelKind::Allay, _) => {
            travel_flying(state, &mut body, world, release)
        }
        (MobTravelKind::ArmorStand, _) if !armor_stand_travels(state, release) => None,
        _ => travel_living(state, &mut body, world, release),
    // (1.21.1 LivingEntity.java:2720; 1.21.2 LivingEntity.java:2817-2819)
    };
    // (the mob's travel is the server's here: !level.isClientSide())
    if release.at_least(1, 21, 2) && applies_effects_from_blocks(&body, release) {
        // Entity.applyEffectsFromBlocks at the end of LivingEntity's travel step (1.21.3
        // LivingEntity.java:2818, 26.3 :3064-3066). A vex is affected though it has noPhysics
        // (1.21.3 Vex.java:65, 26.3 Vex.java:68): that noPhysics is its tick's, and its travel
        // has no move to apply them in; no other mob has it.
        let (requested, displacement) =
            moved.map_or((Vec3::ZERO, Vec3::ZERO), |moved| (moved.requested, moved.moved));
        let movements = movements_this_tick(&body, old_position, requested, displacement, release);
        let (inside, _) = blocks_inside_moves(&body, world, &movements, release);
        let support = moved.and_then(|moved| moved.support);
        body.no_physics = false;
        apply_block_effects(&mut body, world, support, release, &inside);
    }
    state.position = body.position;
    state.velocity = body.velocity;
    state.bounding_box = body.bounding_box;
    state.on_ground = body.on_ground;
    state.collided_horizontally = body.collided_horizontally;
    state.collided_vertically = body.collided_vertically;
    state.fall_distance = body.fall_distance;
    state.stuck_speed_multiplier = body.stuck_speed_multiplier;
    state.move_out_of_range |= body.move_out_of_range;
}

/// Entity#moveRelative: the input turned by the yaw at `speed`, added to the motion.
fn mob_relative(
    state: &MobTravelState,
    body: &mut SimulationState,
    speed: f32,
    release: MinecraftVersion,
) {
    let turn = crate::simulation::yaw_radians(state.yaw, release);
    let push = crate::simulation::relative_movement(state.input, f64::from(speed), turn, release);
    body.velocity = body.velocity + push;
}

/// The move of LivingEntity#travel, with the getEffectiveGravity and getAirDrag the landing's
/// bounce reads from 26.2 (26.2 Entity.java:848, 855-856; 26.3 :878, 885-886): the gravity
/// attribute from 1.20.5 and Slow Falling's 0.01 (26.2 LivingEntity.java:2335-2342), and the
/// 0.98F, or 0.91F for an omnidirectional air mover, modified by AIR_DRAG_MODIFIER (26.2
/// LivingEntity.java:2414-2416, 26.3 :2412-2414).
fn mob_move(
    state: &MobTravelState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> VehicleMove {
    let gravity = crate::ridden::effective_gravity(
        state.gravity,
        state.no_gravity,
        state.slow_falling,
        body.velocity.y,
        release,
    );
    let omnidirectional = plain_of(state).omnidirectional;
    let base_drag = f64::from(if omnidirectional { 0.91_f32 } else { 0.98_f32 });
    let air_drag = if release.at_least(26, 2, 0) {
        // the attribute holds 0 to 2048, NaN its minimum (26.2 Attributes.java:10), 1 when absent
        let modifier = state
            .air_drag_modifier
            .map_or(1.0, |modifier| if modifier.is_nan() { 0.0 } else { modifier.clamp(0.0, 2048.0) });
        crate::simulation::compute_modified_friction(base_drag, modifier)
    } else {
        base_drag
    };
    let rules = MoveRules {
        living: true,
        max_step: state.max_step,
        gravity,
        air_drag,
        bounciness: 0.0,
        stands_on_lava: false,
        checks_inside_at_fall: false,
        rails_keep_speed: false,
        ignored: [None, None],
        skips_restitution: false,
        counts_fall: false,
    };
    crate::ridden::plain_living_move(body, world, release, rules, plain_of(state))
}

/// FlyingMob.travel until 1.21.5 for a ghast and a phantom, Allay.travel for an allay; from 1.21.6
/// LivingEntity.travelFlying with each one's speed in the air (1.8.8 FlyingMob.java,
/// 1.21.4 FlyingMob.java:16-42, 1.21.5 FlyingMob.java:16-42, 1.20.4 Allay.java:169-186,
/// 26.1.2 LivingEntity.java:2406-2410, 26.3 LivingEntity.java:2364-2380, Ghast.java:90, Phantom.java:133, Allay.java:134).
fn travel_flying(
    state: &MobTravelState,
    body: &mut SimulationState,
    world: &WorldSnapshot,
    release: MinecraftVersion,
) -> Option<VehicleMove> {
    if body.in_water || body.in_lava {
        mob_relative(state, body, 0.02, release);
        let moved = mob_move(state, body, world, release);
        let drag = if body.in_water {
            f64::from(0.8_f32)
        } else {
            0.5
        };
        body.velocity = body.velocity.scale(drag);
        return Some(moved);
    // (1.21.5 FlyingMob.java:16-42; 1.21.6 LivingEntity.java:2232-2248, Ghast.java:95, Phantom.java:146, Allay.java:140)
    }
    let modern = release.at_least(1, 21, 6);
    if modern || state.kind == MobTravelKind::Allay {
        let speed = match state.kind {
            MobTravelKind::Ghast => 0.02,
            MobTravelKind::Phantom => 0.2,
            _ => state.speed,
        };
        mob_relative(state, body, speed, release);
        let moved = mob_move(state, body, world, release);
        body.velocity = body.velocity.scale(f64::from(0.91_f32));
        return Some(moved);
    }
    // FlyingMob.travel reads the block at y - 1.0 until 1.19.4 (1.14.4 FlyingMob.java:32, 1.15 :36,
    // 1.19.4 :31), getBlockPosBelowThatAffectsMyMovement from 1.20 (1.20 FlyingMob.java:31)
    let ground = |body: &SimulationState, support| {
        let cell = if release.at_least(1, 20, 0) {
            crate::simulation::block_below_affecting_movement(world, body, support, release)
        } else {
            (
                floor_i32_for(release, body.position.x),
                floor_i32_for(release, body.position.y - 1.0),
                floor_i32_for(release, body.position.z),
            )
        };
        world.slipperiness_at(cell) as f32 * 0.91
    };
    let support = release
        .at_least(1, 20, 0)
        .then(|| crate::simulation::main_supporting_block(world, body, None, release))
        .flatten();
    let friction = if body.on_ground { ground(body, support) } else { 0.91 };
    let speed = if body.on_ground {
        0.1 * (0.162_771_37 / (friction * friction * friction))
    } else {
        0.02
    };
    mob_relative(state, body, speed, release);
    let moved = mob_move(state, body, world, release);
    body.velocity = body.velocity.scale(f64::from(friction));
    Some(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{BlockEffect, BlockShape, BlockState};

    #[test]
    fn an_items_cell_test_is_of_ints() {
        let (old, new) = (R1_13, MinecraftVersion::new(1, 14, 4));
        let at = |x: f64| Vec3::new(x, 64.0, 0.5);
        // `(int)` truncates towards zero until 1.14, Mth.floor floors from it
        assert!(!moved_to_another_cell(at(-0.5), at(0.5), old));
        assert!(moved_to_another_cell(at(-0.5), at(0.5), new));
        // both are ints: past the range two coordinates are the one cell
        assert!(!moved_to_another_cell(at(3.0e9), at(3.0e9 + 5.0), old));
        assert!(!moved_to_another_cell(at(3.0e9), at(3.0e9 + 5.0), new));
    }

    const R1_8: MinecraftVersion = MinecraftVersion::new(1, 8, 8);
    const R1_9: MinecraftVersion = MinecraftVersion::new(1, 9, 4);
    const R1_10: MinecraftVersion = MinecraftVersion::new(1, 10, 2);
    const R1_11: MinecraftVersion = MinecraftVersion::new(1, 11, 2);
    const R1_12: MinecraftVersion = MinecraftVersion::new(1, 12, 2);
    const R1_13: MinecraftVersion = MinecraftVersion::new(1, 13, 2);
    const R1_14: MinecraftVersion = MinecraftVersion::new(1, 14, 4);
    const R1_15: MinecraftVersion = MinecraftVersion::new(1, 15, 0);
    const R1_16: MinecraftVersion = MinecraftVersion::new(1, 16, 1);
    const R1_16_2: MinecraftVersion = MinecraftVersion::new(1, 16, 2);
    const R1_17: MinecraftVersion = MinecraftVersion::new(1, 17, 0);
    const R1_18: MinecraftVersion = MinecraftVersion::new(1, 18, 0);
    const R1_18_2: MinecraftVersion = MinecraftVersion::new(1, 18, 2);
    const R1_20: MinecraftVersion = MinecraftVersion::new(1, 20, 4);
    const R1_20_5: MinecraftVersion = MinecraftVersion::new(1, 20, 5);
    const R1_21_3: MinecraftVersion = MinecraftVersion::new(1, 21, 3);
    const R1_21_4: MinecraftVersion = MinecraftVersion::new(1, 21, 4);
    const R1_21_5: MinecraftVersion = MinecraftVersion::new(1, 21, 5);
    const R1_21_6: MinecraftVersion = MinecraftVersion::new(1, 21, 6);
    const R1_21_8: MinecraftVersion = MinecraftVersion::new(1, 21, 8);
    const R1_21_9: MinecraftVersion = MinecraftVersion::new(1, 21, 9);
    const R26_1: MinecraftVersion = MinecraftVersion::new(26, 1, 0);
    const R26_2: MinecraftVersion = MinecraftVersion::new(26, 2, 0);
    const R26_3: MinecraftVersion = MinecraftVersion::new(26, 3, 0);

    fn solid(id: u32, slipperiness: f64) -> BlockState {
        BlockState {
            id,
            shape: Some(BlockShape {
                slipperiness,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    /// A floor under y = 100.
    fn floor(slipperiness: f64) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -6..=6 {
            for z in -6..=6 {
                world.set_block((x, 99, z), solid(1, slipperiness));
            }
        }
        world
    }

    fn fluid(kind: FluidState) -> BlockState {
        BlockState {
            fluid: kind,
            fluid_source: true,
            ..BlockState::default()
        }
    }

    fn tick(
        state: &mut MovingEntityState,
        world: &WorldSnapshot,
        release: MinecraftVersion,
    ) -> MovingEntityOutcome {
        let mut ticked = world.clone();
        let outcome = tick_moving_entity(state, &mut ticked, &MovingEntityInputs::default(), release);
        assert_eq!(ticked.encode(), world.encode(), "the tick leaves the world as it was");
        outcome
    }

    fn entity(kind: MovingEntityKind, y: f64, velocity: Vec3) -> MovingEntityState {
        MovingEntityState::new(kind, Vec3::new(0.5, y, 0.5), velocity)
    }

    fn one_draw() -> MovingEntityInputs {
        MovingEntityInputs {
            random: vec![0.5],
            ..MovingEntityInputs::default()
        }
    }

    #[test]
    fn gravity_is_a_float_before_1_14_and_the_drag_a_double_until_26_2() {
        let world = floor(0.6);
        let fall = |release| {
            let mut state = entity(MovingEntityKind::PrimedTnt, 110.0, Vec3::ZERO);
            tick(&mut state, &world, release);
            state.velocity.y
        };
        assert_eq!(fall(R1_13), -f64::from(0.04_f32) * f64::from(0.98_f32));
        assert_eq!(fall(R1_14), -0.04 * 0.98);
        assert_eq!(fall(R26_1), -0.04 * 0.98);
        assert_eq!(fall(R26_2), -0.04 * f64::from(0.98_f32));
    }

    #[test]
    fn the_pull_turns_a_negative_zero_of_x_and_z_positive_from_1_14() {
        let world = floor(0.6);
        let after = |release| {
            let mut state = entity(MovingEntityKind::PrimedTnt, 110.0, Vec3::new(-0.0, 0.0, -0.0));
            tick(&mut state, &world, release);
            (state.velocity.x.is_sign_negative(), state.velocity.z.is_sign_negative())
        };
        assert_eq!(after(R1_13), (true, true));
        assert_eq!(after(R1_14), (false, false));
        assert_eq!(after(R26_2), (false, false));
    }

    #[test]
    fn tnt_keeps_seven_tenths_of_its_motion_on_landing() {
        let world = floor(0.6);
        let mut state = entity(MovingEntityKind::PrimedTnt, 100.01, Vec3::new(0.1, -0.2, 0.0));
        tick(&mut state, &world, R1_20);
        assert!(state.on_ground);
        assert_eq!(state.velocity.x, 0.1 * 0.98 * 0.7);
        assert!(state.velocity.y == 0.0 && state.velocity.y.is_sign_positive());
    }

    #[test]
    fn the_fuse_runs_a_tick_longer_in_1_8_and_only_the_server_explodes() {
        let world = floor(0.6);
        for (release, ticks_left) in [(R1_8, 2), (R1_9, 1)] {
            let mut state = entity(MovingEntityKind::PrimedTnt, 100.0, Vec3::ZERO).on_server();
            state.fuse = 1;
            for left in (0..ticks_left).rev() {
                let outcome = tick(&mut state, &world, release);
                assert_eq!(outcome.explodes, left == 0, "{release:?}");
            }
            assert!(state.removed);
        }
        let mut client = entity(MovingEntityKind::PrimedTnt, 100.0, Vec3::ZERO);
        client.fuse = 1;
        let outcome = tick(&mut client, &world, R26_3);
        assert!(client.removed, "the client discards it too");
        assert!(!outcome.explodes);
    }

    #[test]
    fn no_gravity_counts_from_1_10() {
        let world = WorldSnapshot::new();
        let moves = |release| {
            let mut state = entity(MovingEntityKind::Item, 110.0, Vec3::ZERO);
            state.no_gravity = true;
            tick(&mut state, &world, release);
            state.velocity.y
        };
        assert!(moves(R1_9) < 0.0);
        assert_eq!(moves(R1_10), 0.0);
    }

    #[test]
    fn the_void_ends_an_item_and_not_tnt_which_has_no_base_tick() {
        let world = WorldSnapshot::new();
        let mut tnt = entity(MovingEntityKind::PrimedTnt, -70.0, Vec3::ZERO);
        tick(&mut tnt, &world, R1_13);
        assert!(!tnt.removed);
        let mut item = entity(MovingEntityKind::Item, -70.0, Vec3::ZERO);
        tick(&mut item, &world, R1_13);
        assert!(item.removed);
    }

    #[test]
    fn a_client_falling_block_waits_for_the_server_and_places_nothing() {
        let world = floor(0.6);
        let mut client = entity(MovingEntityKind::FallingBlock, 100.0, Vec3::new(0.0, -0.1, 0.0));
        client.time = 5;
        let outcome = tick(&mut client, &world, R26_3);
        assert!(client.on_ground && !client.removed);
        assert_eq!(outcome, MovingEntityOutcome::default());
        let mut server = entity(MovingEntityKind::FallingBlock, 100.0, Vec3::new(0.0, -0.1, 0.0)).on_server();
        server.time = 5;
        let outcome = tick(&mut server, &world, R26_3);
        assert!(server.removed);
        assert_eq!(outcome.landed_at, Some((0, 100, 0)));
    }

    #[test]
    fn a_client_leaves_a_landing_motion_to_the_server_from_1_21_2_until_26_3() {
        // 1.21.2 Entity.java:721-724, 1.21.5 :755-760: the block's fall stop is for the side that simulates;
        // 26.3 ItemEntity.java:429 simulates on both sides; 1.21 Entity.java:692 had no such gate
        let world = floor(0.6);
        let landed = |release| {
            let mut client = entity(MovingEntityKind::PrimedTnt, 100.1, Vec3::new(0.0, -0.3, 0.0));
            tick(&mut client, &world, release);
            client.velocity.y
        };
        assert!(landed(MinecraftVersion::new(1, 21, 8)) > 0.1);
        assert!(landed(MinecraftVersion::new(1, 21, 2)) > 0.1);
        assert_eq!(landed(MinecraftVersion::new(1, 21, 0)).abs(), 0.0);
    }

    #[test]
    fn a_moving_piston_under_a_landed_block_lets_it_bounce_on() {
        let mut world = floor(0.6);
        world.set_block((0, 100, 0), BlockState::default());
        world.set_moving_block((0, 100, 0), &[], false);
        let mut server = entity(MovingEntityKind::FallingBlock, 100.0, Vec3::new(0.0, -0.1, 0.0)).on_server();
        server.time = 5;
        let outcome = tick(&mut server, &world, R26_3);
        assert!(!server.removed);
        assert_eq!(outcome.landed_at, None);
    }

    #[test]
    fn the_first_tick_takes_the_block_out_only_before_1_18_2() {
        let mut world = floor(0.6);
        world.set_block((0, 105, 0), solid(12, 0.6));
        let start = || {
            let mut state = entity(MovingEntityKind::FallingBlock, 105.0, Vec3::ZERO).on_server();
            state.carried_block = 12;
            state
        };
        let mut old = start();
        assert_eq!(tick(&mut old, &world, R1_18).clears_block, Some((0, 105, 0)));
        assert!(old.position.y < 105.0, "it fell out of the block it took");
        let mut new = start();
        assert_eq!(tick(&mut new, &world, R1_18_2).clears_block, None);
        // a server drops an entity whose block is gone, and a client keeps it
        let mut gone = start();
        gone.carried_block = 13;
        tick(&mut gone, &world, R1_18);
        assert!(gone.removed);
        let mut client = start();
        client.carried_block = 13;
        client.authority = MovingEntityAuthority::Client;
        tick(&mut client, &world, R1_18);
        assert!(!client.removed);
    }

    #[test]
    fn only_1_18_and_1_18_1_freeze_a_client_block_that_is_going() {
        let world = WorldSnapshot::new();
        let frozen = |release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 105.0, Vec3::ZERO);
            state.time = 4;
            state.discarding = true;
            tick(&mut state, &world, release);
            state.position.y == 105.0
        };
        assert!(frozen(R1_18));
        assert!(!frozen(R1_17));
        assert!(!frozen(R1_18_2));
    }

    #[test]
    fn a_falling_block_drags_before_its_landing_until_1_12() {
        let world = floor(0.6);
        let landed = |release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 100.0, Vec3::new(0.1, -0.1, 0.0)).on_server();
            state.time = 3;
            state.concrete_powder = false;
            tick(&mut state, &world, release);
            state.velocity.x
        };
        assert_eq!(landed(R1_11), 0.1 * f64::from(0.98_f32) * f64::from(0.7_f32));
        assert_eq!(landed(R1_12), 0.1 * f64::from(0.7_f32) * f64::from(0.98_f32));
    }

    #[test]
    fn from_1_9_until_1_14_a_block_with_nothing_solid_under_its_feet_falls_on() {
        let mut world = floor(0.6);
        let inputs = MovingEntityInputs {
            falls_through_below: true,
            ..MovingEntityInputs::default()
        };
        for (release, carries_on) in [(R1_8, false), (R1_9, true), (R1_13, true), (R1_14, false)] {
            let mut state = entity(MovingEntityKind::FallingBlock, 100.0, Vec3::new(0.0, -0.1, 0.0)).on_server();
            state.time = 3;
            tick_moving_entity(&mut state, &mut world, &inputs, release);
            assert_eq!(!state.removed, carries_on, "{release:?}");
        }
    }

    #[test]
    fn water_stops_a_concrete_powder_and_nothing_else() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 104, 0), fluid(FluidState::Water));
        let in_water = |concrete, release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 104.5, Vec3::ZERO).on_server();
            state.time = 3;
            state.concrete_powder = concrete;
            tick(&mut state, &world, release).landed_at.is_some()
        };
        assert!(in_water(true, R1_13));
        assert!(!in_water(false, R1_13));
        assert!(!in_water(true, R1_11), "there was none before 1.12");
    }

    #[test]
    fn a_fast_concrete_powder_stops_at_the_water_it_passes_through() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 103, 0), fluid(FluidState::Water));
        let mut state = entity(MovingEntityKind::FallingBlock, 105.5, Vec3::new(0.0, -3.0, 0.0)).on_server();
        state.time = 3;
        state.concrete_powder = true;
        assert_eq!(tick(&mut state, &world, R1_20).landed_at, Some((0, 103, 0)));
    }

    #[test]
    fn before_1_14_a_concrete_powders_trace_stops_at_a_plants_box_in_1_12_and_at_any_lava_source() {
        // 1.12.2 Level#rayTrace stops at every block but air, fire and flowing liquid, by its
        // bounding box (1.12.2 Block.java:454-460, 555-556), so tall grass over the water hides
        // it; 1.13.2 picks only blocks with collision (1.13.2 Block.java:469-470) and 1.14 clips
        // the collision shape (1.14.4 FallingBlockEntity.java:128), so both reach the water. A
        // flowing water cell is passed (1.12.2 LiquidBlock.java:181-183, 1.13.2 SOURCE_ONLY); a
        // lava source ends the trace before 1.14 with no water (1.12.2 FallingBlockEntity.java:131,
        // 1.13.2 :130).
        let grass = BlockState {
            id: 31,
            outline_shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::new(0.1, 0.0, 0.1), Vec3::new(0.9, 0.8, 0.9)),
                slipperiness: 0.6,
                fluid: FluidState::Empty,
                effect: BlockEffect::None,
            }),
            ..BlockState::default()
        };
        let lands = |above: BlockState, release| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 103, 0), fluid(FluidState::Water));
            world.set_block((0, 104, 0), above);
            let mut state =
                entity(MovingEntityKind::FallingBlock, 105.5, Vec3::new(0.0, -3.0, 0.0)).on_server();
            state.time = 3;
            state.concrete_powder = true;
            tick(&mut state, &world, release).landed_at
        };
        assert_eq!(lands(grass, R1_12), None, "the grass's box ends the 1.12.2 trace");
        assert_eq!(lands(grass, R1_13), Some((0, 103, 0)));
        assert_eq!(lands(grass, R1_14), Some((0, 103, 0)));
        let flowing = BlockState { fluid_source: false, fluid_level: 3, ..fluid(FluidState::Water) };
        assert_eq!(lands(flowing, R1_12), Some((0, 103, 0)));
        assert_eq!(lands(flowing, R1_13), Some((0, 103, 0)));
        assert_eq!(lands(fluid(FluidState::Lava), R1_12), None);
        assert_eq!(lands(fluid(FluidState::Lava), R1_13), None);
    }

    #[test]
    fn before_1_14_a_concrete_powders_trace_tests_the_cell_it_starts_in() {
        // the first cell is tested before the walk (1.12.2 Level.java:876-883, 1.13.2 Level.java:735-753):
        // a powder starting in a water source and moving out of it lands there
        let mut world = WorldSnapshot::new();
        world.set_block((0, 105, 0), fluid(FluidState::Water));
        for release in [R1_12, R1_13] {
            let mut state =
                entity(MovingEntityKind::FallingBlock, 105.5, Vec3::new(0.0, -3.0, 0.0)).on_server();
            state.time = 3;
            state.concrete_powder = true;
            assert_eq!(tick(&mut state, &world, release).landed_at, Some((0, 105, 0)), "{release:?}");
        }
    }

    #[test]
    fn from_1_14_a_concrete_powders_clip_ends_at_a_lava_source_with_no_water() {
        // BlockGetter#clip under Fluid.SOURCE_ONLY clips any source fluid's shape (1.14.4
        // ClipContext.java:41-42, BlockGetter.java:44-48), and FallingBlockEntity takes the hit
        // only when its cell holds water (1.14.4 FallingBlockEntity.java:128-129, 1.21.11 :159-160):
        // a lava source over the water ends the clip there, with no water
        let lands = |above: BlockState, release| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 103, 0), fluid(FluidState::Water));
            world.set_block((0, 104, 0), above);
            let mut state =
                entity(MovingEntityKind::FallingBlock, 105.5, Vec3::new(0.0, -3.0, 0.0)).on_server();
            state.time = 3;
            state.concrete_powder = true;
            tick(&mut state, &world, release).landed_at
        };
        for release in [R1_14, R1_21_5, R26_3] {
            assert_eq!(lands(fluid(FluidState::Lava), release), None, "{release:?}");
            assert_eq!(lands(BlockState::default(), release), Some((0, 103, 0)), "{release:?}");
            let flowing = BlockState { fluid_source: false, fluid_level: 3, ..fluid(FluidState::Lava) };
            assert_eq!(lands(flowing, release), Some((0, 103, 0)), "flowing lava is passed, {release:?}");
        }
    }

    #[test]
    fn from_1_14_a_concrete_powders_clip_passes_a_block_whose_shape_it_misses() {
        // ClipContext.Block.COLLIDER clips the block's collision shape (1.14.4 ClipContext.java:70,
        // BlockGetter.java:42-43, VoxelShape.java:125-137): a post in the cell's corner, which
        // neither the line through the block's centre nor its 0.98 wide box meets, stops nothing,
        // and the powder reaches the water below
        let lands = |bounds: Aabb, release| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 103, 0), fluid(FluidState::Water));
            world.set_block(
                (0, 104, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape {
                        bounds,
                        slipperiness: 0.6,
                        fluid: FluidState::Empty,
                        effect: BlockEffect::None,
                    }),
                    ..BlockState::default()
                },
            );
            let mut state =
                entity(MovingEntityKind::FallingBlock, 105.5, Vec3::new(0.0, -3.0, 0.0)).on_server();
            state.time = 3;
            state.concrete_powder = true;
            tick(&mut state, &world, release).landed_at
        };
        let post = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.005, 1.0, 0.005));
        for release in [R1_14, R1_21_5, R26_3] {
            assert_eq!(lands(post, release), Some((0, 103, 0)), "{release:?}");
        }
    }

    #[test]
    fn a_block_falls_for_600_ticks_at_most_and_100_outside_the_world() {
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 1.0e7);
        let expired = |time, y, release| {
            let mut state = entity(MovingEntityKind::FallingBlock, y, Vec3::ZERO).on_server();
            state.time = time;
            state.no_gravity = true;
            tick(&mut state, &world, release).expired
        };
        assert!(!expired(599, 100.0, R26_3) || !expired(598, 100.0, R26_3));
        assert!(expired(600, 100.0, R26_3));
        assert!(!expired(99, -64.0, R26_3));
        assert!(expired(100, -64.0, R26_3));
        assert!(!expired(100, 320.0, R1_20), "the height counts as inside until 1.21.2");
        assert!(expired(100, 320.0, R26_3));
        assert!(expired(100, 257.0, R1_14));
    }

    #[test]
    fn an_item_rests_three_ticks_in_four_from_1_14_4() {
        let world = floor(0.6);
        let settled = |id| {
            let mut state = entity(MovingEntityKind::Item, 100.0, Vec3::ZERO);
            state.on_ground = true;
            state.id = id;
            state
        };
        let mut resting = settled(1);
        tick(&mut resting, &world, MinecraftVersion::new(1, 14, 4));
        assert_eq!(resting.velocity.y, -0.04, "gravity, and no move to stop it");
        let mut moving = settled(3);
        tick(&mut moving, &world, MinecraftVersion::new(1, 14, 4));
        assert_ne!(moving.velocity.y, -0.04);
        let mut old = settled(1);
        tick(&mut old, &world, MinecraftVersion::new(1, 13, 2));
        assert_ne!(old.velocity.y, -0.04, "it moved every tick to 1.13.2");
    }

    #[test]
    fn lava_halves_no_fall_on_a_first_tick_from_1_16_2() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), BlockState { fluid: FluidState::Lava, ..BlockState::default() });
        let fall = |release, first_tick| {
            let mut state = entity(MovingEntityKind::PrimedTnt, 100.5, Vec3::ZERO);
            state.first_tick = first_tick;
            state.fall_distance = 4.0;
            state.in_lava = true;
            let mut body = state.body(release);
            base_tick(&mut state, &mut body, &world, release);
            body.fall_distance
        };
        assert_eq!(fall(MinecraftVersion::new(1, 16, 2), true), 4.0);
        assert_eq!(fall(MinecraftVersion::new(1, 16, 2), false), 2.0);
        assert_eq!(fall(MinecraftVersion::new(1, 16, 1), true), 2.0);
    }

    #[test]
    fn an_item_slides_by_the_friction_of_its_block() {
        let world = floor(0.98);
        let mut state = entity(MovingEntityKind::Item, 100.01, Vec3::new(0.2, -0.1, 0.0));
        tick(&mut state, &world, R1_20);
        assert_eq!(state.velocity.x, 0.2 * f64::from(0.98_f32 * 0.98_f32));
    }

    #[test]
    fn an_item_going_up_when_it_lands_turns_back_only_before_1_16_2() {
        let mut world = floor(0.6);
        world.set_block((0, 99, 0), solid(1, 0.6));
        let after = |release| {
            let mut state = entity(MovingEntityKind::Item, 100.0, Vec3::new(0.0, -0.1, 0.0));
            tick(&mut state, &world, release);
            state.velocity.y
        };
        // landing leaves the motion at -0, which only the old way multiplies
        assert!(after(R1_16).is_sign_positive());
        assert!(after(R1_16_2).is_sign_negative());
    }

    #[test]
    fn water_lifts_an_item_and_an_orb_from_1_13_and_lets_them_sink_before() {
        let mut world = WorldSnapshot::new();
        for y in 103..=105 {
            world.set_block((0, y, 0), fluid(FluidState::Water));
        }
        let lift = |kind, release| {
            let mut state = entity(kind, 104.2, Vec3::ZERO);
            tick(&mut state, &world, release);
            state.velocity.y
        };
        assert!(lift(MovingEntityKind::Item, R1_12) < 0.0);
        assert!(lift(MovingEntityKind::Item, R1_13) > 0.0);
        assert!(lift(MovingEntityKind::ExperienceOrb, R1_12) < 0.0);
        assert!(lift(MovingEntityKind::ExperienceOrb, R1_13) > 0.0);
    }

    #[test]
    fn the_move_says_it_is_in_lava_from_1_14_to_1_16_1() {
        // Entity.move: isInLava = false, then the blocks inside set it (1.14.4 Entity.java:568,
        // LiquidBlock.entityInside; 1.16.1 Entity.java:661, above the foot of a lava block's
        // surface, LiquidBlock.java:176-179); 1.16.2 reads the height, and before 1.14 it is a query
        let mut world = WorldSnapshot::new();
        for y in 103..=105 {
            world.set_block((0, y, 0), fluid(FluidState::Lava));
        }
        for release in [R1_14, R1_15, R1_16] {
            let mut state = entity(MovingEntityKind::Item, 104.2, Vec3::ZERO);
            tick(&mut state, &world, release);
            assert!(state.in_lava, "{release:?}");
            // (and the next base tick halves the fall)
            state.fall_distance = 4.0;
            state.in_lava = true;
            state.velocity = Vec3::new(0.0, 0.01, 0.0);
            tick(&mut state, &world, release);
            assert!(state.fall_distance < 2.1, "{release:?}");
            let mut clear = entity(MovingEntityKind::Item, 110.0, Vec3::ZERO);
            clear.in_lava = true;
            tick(&mut clear, &world, release);
            assert!(!clear.in_lava, "{release:?}: the move clears it");
        }
    }

    #[test]
    fn no_physics_leaves_the_lava_flag_as_it_was() {
        // (Entity.move's noPhysics way returns before the check: 1.15 Entity.java:600-625)
        let mut state = entity(MovingEntityKind::Item, 110.0, Vec3::ZERO);
        state.in_lava = true;
        let mut body = state.body(R1_15);
        body.no_physics = true;
        move_entity(&mut state, &mut body, &WorldSnapshot::new(), R1_15);
        assert!(body.in_lava);
    }

    #[test]
    fn lava_lifts_an_item_from_1_16() {
        let mut world = WorldSnapshot::new();
        for y in 103..=105 {
            world.set_block((0, y, 0), fluid(FluidState::Lava));
        }
        let lift = |release| {
            let mut state = entity(MovingEntityKind::Item, 104.2, Vec3::new(0.1, 0.0, 0.0));
            state.tick_count = 1;
            tick(&mut state, &world, release);
            state.velocity.y
        };
        assert!(lift(R1_15) < 0.0);
        assert!(lift(R1_20) > 0.0);
    }

    #[test]
    fn lava_throws_an_item_until_1_17_and_its_sound_takes_a_draw() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 105, 0), fluid(FluidState::Lava));
        let inputs = MovingEntityInputs {
            random: vec![0.75, 0.25, 0.5, 0.0, 0.9],
            ..MovingEntityInputs::default()
        };
        let mut state = entity(MovingEntityKind::Item, 105.5, Vec3::ZERO);
        state.tick_count = 24;
        let outcome = tick_moving_entity(&mut state, &mut world, &inputs, R1_13);
        assert_eq!(outcome.random_used, 5);
        assert_eq!(state.velocity.x, f64::from(0.5_f32 * 0.2) * f64::from(0.98_f32));
        let mut late = entity(MovingEntityKind::Item, 105.5, Vec3::ZERO);
        late.tick_count = 39;
        assert_eq!(tick_moving_entity(&mut late, &mut world, &inputs, R1_17).random_used, 0);
    }

    #[test]
    fn lava_throws_an_item_until_1_16_and_only_sounds_until_1_17() {
        // (1.15.2 ItemEntity.java:118-121 sets the motion and plays the burn; 1.16 ItemEntity.java:136-137 and 1.16.5
        // :139-141 only play it)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 105, 0), fluid(FluidState::Lava));
        let inputs = MovingEntityInputs {
            random: vec![0.75, 0.25, 0.5, 0.0, 0.9],
            ..MovingEntityInputs::default()
        };
        let mut run = |release| {
            let mut state = entity(MovingEntityKind::Item, 105.5, Vec3::ZERO);
            state.tick_count = 39;
            let outcome = tick_moving_entity(&mut state, &mut world, &inputs, release);
            (outcome.random_used, state.velocity.x)
        };
        let (thrown_draws, thrown_x) = run(MinecraftVersion::new(1, 15, 2));
        assert_eq!(thrown_draws, 5);
        assert!(thrown_x != 0.0);
        for release in [MinecraftVersion::new(1, 16, 0), MinecraftVersion::new(1, 16, 5)] {
            assert_eq!(run(release), (1, 0.0), "{release:?}");
        }
    }

    #[test]
    fn a_fire_immune_item_in_lava_draws_nothing_for_the_burn_on_1_16() {
        // `&& !this.isFireImmune()` guards the burn's sound and its draw (1.16 ItemEntity.java:133-134,
        // 1.16.5 :137-138)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 105, 0), fluid(FluidState::Lava));
        let inputs = MovingEntityInputs {
            random: vec![0.75, 0.25, 0.5, 0.0, 0.9],
            ..MovingEntityInputs::default()
        };
        for (release, fire_immune, draws) in [
            (MinecraftVersion::new(1, 16, 0), false, 1),
            (MinecraftVersion::new(1, 16, 0), true, 0),
            (MinecraftVersion::new(1, 16, 5), true, 0),
        ] {
            let mut state = entity(MovingEntityKind::Item, 105.5, Vec3::ZERO);
            state.tick_count = 39;
            state.fire_immune = fire_immune;
            let outcome = tick_moving_entity(&mut state, &mut world, &inputs, release);
            assert_eq!(outcome.random_used, draws, "{release:?} {fire_immune}");
        }
    }

    #[test]
    fn a_tick_that_needs_a_draw_it_was_not_given_says_so() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 105, 0), fluid(FluidState::Lava));
        let mut state = entity(MovingEntityKind::ExperienceOrb, 105.0, Vec3::ZERO);
        assert!(tick(&mut state, &world, R1_20).random_missing);
    }

    fn stuck_world() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 100, 0), solid(1, 0.6));
        world
    }

    fn stuck(kind: MovingEntityKind, y: f64) -> MovingEntityState {
        MovingEntityState::new(kind, Vec3::new(0.1, y, 0.5), Vec3::ZERO)
    }

    #[test]
    fn a_server_item_leaves_a_block_and_a_client_one_stays_from_1_11() {
        let mut world = stuck_world();
        let inputs = one_draw();
        let mut server = stuck(MovingEntityKind::Item, 100.4).on_server();
        tick_moving_entity(&mut server, &mut world, &inputs, R26_3);
        assert!(server.no_physics);
        assert!(server.position.x < 0.1);
        let mut client = stuck(MovingEntityKind::Item, 100.4);
        let outcome = tick_moving_entity(&mut client, &mut world, &inputs, R26_3);
        assert!(!client.no_physics);
        assert_eq!(outcome.random_used, 0);
        let mut old = stuck(MovingEntityKind::Item, 100.4);
        assert_eq!(tick_moving_entity(&mut old, &mut world, &inputs, R1_10).random_used, 1);
    }

    #[test]
    fn the_way_out_of_a_block_has_changed_three_times() {
        let world = stuck_world();
        let inputs = one_draw();
        let after = |release| {
            let mut state = stuck(MovingEntityKind::Item, 100.4);
            state.velocity = Vec3::new(0.04, 0.0, 0.08);
            let mut body = state.body(release);
            let mut draws = Draws {
                random: &inputs.random,
                used: 0,
                missing: false,
            };
            move_towards_closest_space(&mut body, &world, &mut draws, release);
            body.velocity
        };
        let speed = -f64::from(0.5_f32 * 0.2 + 0.1);
        assert_eq!(after(R1_8), Vec3::new(speed, 0.0, 0.08), "set");
        assert_eq!(after(R1_9), Vec3::new(0.04 + speed, 0.0, 0.08), "added");
        assert_eq!(after(R1_11), Vec3::new(speed, 0.0, 0.06), "set, the rest at 3/4");
        assert_eq!(after(R1_14), Vec3::new(speed, 0.0, 0.06));
    }

    #[test]
    fn a_block_whose_bounding_box_fills_its_cell_is_no_way_out_before_1_13() {
        // World.isBlockFullCube before 1.13 is the average edge of getCollisionBoundingBox, at least
        // 1 for a stair's whole-cell getBoundingBox though its boxes are two halves (1.12.2
        // World.java:1835-1838, Block.java:405-407, StairsBlock.java:89-97), which the
        // LEGACY_FULL_CUBE tag says; from 1.13 Block.isShapeFullBlock of the collision shape
        let inputs = one_draw();
        let after = |tagged: bool, release| {
            let mut world = stuck_world();
            world.set_block(
                (-1, 100, 0),
                BlockState {
                    id: 2,
                    tags: if tagged { crate::world::LEGACY_FULL_CUBE } else { 0 },
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            let state = stuck(MovingEntityKind::Item, 100.4);
            let mut body = state.body(release);
            let mut draws = Draws {
                random: &inputs.random,
                used: 0,
                missing: false,
            };
            move_towards_closest_space(&mut body, &world, &mut draws, release);
            body.velocity
        };
        // west, the nearest side, is shut by the tagged stair before 1.13: the item goes up, the
        // next nearest
        let shut = after(true, R1_11);
        assert_eq!(shut.x, 0.0);
        assert!(shut.y > 0.0);
        assert!(after(false, R1_11).x < 0.0);
        assert!(after(true, R1_14).x < 0.0);
    }

    #[test]
    fn north_goes_before_west_from_1_14() {
        let world = stuck_world();
        let inputs = one_draw();
        let after = |release| {
            let mut state = MovingEntityState::new(MovingEntityKind::Item, Vec3::new(0.25, 100.5, 0.25), Vec3::ZERO);
            state.bounding_box = None;
            let mut body = state.body(release);
            let mut draws = Draws {
                random: &inputs.random,
                used: 0,
                missing: false,
            };
            move_towards_closest_space(&mut body, &world, &mut draws, release);
            body.velocity
        };
        assert!(after(R1_13).x < 0.0);
        assert!(after(R1_14).z < 0.0);
    }

    fn player(position: Vec3) -> MovingEntityPlayer {
        MovingEntityPlayer {
            id: 7,
            position,
            eye_height: 1.62,
            spectator: false,
            dead: false,
        }
    }

    fn orb_inputs(position: Vec3) -> MovingEntityInputs {
        MovingEntityInputs {
            players: vec![player(position)],
            ..MovingEntityInputs::default()
        }
    }

    #[test]
    fn an_orb_follows_the_nearest_player_within_8_blocks() {
        let mut world = WorldSnapshot::new();
        let mut pulled = |x: f64| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO).on_server();
            let inputs = orb_inputs(Vec3::new(x, 109.0, 0.5));
            tick_moving_entity(&mut state, &mut world, &inputs, R26_3);
            state
        };
        let near = pulled(4.5);
        assert_eq!(near.following_player, Some(7));
        assert!(near.velocity.x > 0.0);
        let far = pulled(9.5);
        assert_eq!(far.following_player, None);
        assert_eq!(far.velocity.x, 0.0);
    }

    #[test]
    fn a_spectator_draws_no_orb() {
        let mut world = WorldSnapshot::new();
        let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO).on_server();
        let mut watcher = player(Vec3::new(2.5, 110.0, 0.5));
        watcher.spectator = true;
        let inputs = MovingEntityInputs {
            players: vec![watcher],
            ..MovingEntityInputs::default()
        };
        tick_moving_entity(&mut state, &mut world, &inputs, R26_3);
        assert_eq!(state.following_player, None);
    }

    #[test]
    fn an_orb_looks_for_a_player_by_its_own_count_then_every_20_ticks_then_every_tick() {
        let mut world = WorldSnapshot::new();
        let inputs = orb_inputs(Vec3::new(4.5, 110.0, 0.5));
        let mut looks = |tick_count, release| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO).on_server();
            state.tick_count = tick_count;
            state.orb_ticks = 100;
            state.following_time = 90;
            tick_moving_entity(&mut state, &mut world, &inputs, release);
            state.following_player.is_some()
        };
        assert!(!looks(5, R1_16), "it looked 10 of its ticks ago");
        assert!(looks(0, R1_17), "the 1st of 20");
        assert!(!looks(1, R1_17));
        assert!(looks(1, R1_21_5));
    }

    #[test]
    fn the_pull_of_a_player_takes_the_whole_eye_height_in_1_8_and_half_after() {
        let mut world = WorldSnapshot::new();
        // under the orb's feet by half the eye height and over them by the whole
        let inputs = orb_inputs(Vec3::new(0.5, 109.0, 0.5));
        let mut rise = |release| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO).on_server();
            state.following_player = Some(7);
            state.no_gravity = true;
            state.pickup_delay = 0;
            tick_moving_entity(&mut state, &mut world, &inputs, release);
            state.velocity.y
        };
        assert!(rise(R1_8) > rise(R1_9));
    }

    #[test]
    fn a_player_too_far_pulls_only_from_1_21_5_where_the_orb_lets_it_go_first() {
        let mut world = WorldSnapshot::new();
        let inputs = orb_inputs(Vec3::new(8.7, 110.0, 0.5));
        let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO).on_server();
        state.following_player = Some(7);
        state.tick_count = 5;
        tick_moving_entity(&mut state, &mut world, &inputs, R1_20);
        assert_eq!(state.velocity.x, 0.0, "it keeps its player, but past 8 blocks no pull");
        assert_eq!(state.following_player, Some(7));
        state.velocity = Vec3::ZERO;
        state.tick_count = 5;
        tick_moving_entity(&mut state, &mut world, &inputs, R1_21_5);
        assert_eq!(state.following_player, None);
    }

    #[test]
    fn an_orb_on_the_ground_bounces_by_the_release() {
        let world = floor(0.6);
        let bounce = |release| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 100.01, Vec3::new(0.0, -0.3, 0.0)).on_server();
            tick(&mut state, &world, release);
            state.velocity.y
        };
        let falling = -0.3 - 0.03;
        assert_eq!(bounce(R1_21_3), 0.0);
        assert_eq!(bounce(R1_21_4), -falling * 0.4);
        assert_eq!(bounce(R1_21_5), -falling * 0.4);
        let settled = |release| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 100.0, Vec3::ZERO).on_server();
            state.on_ground = true;
            tick(&mut state, &world, release);
            state.velocity.y
        };
        assert_eq!(settled(R1_21_4), 0.03 * 0.4);
        assert_eq!(settled(R1_21_5), 0.0, "only a real fall bounces");
    }

    #[test]
    fn a_client_orb_skips_its_first_tick_from_1_21_5() {
        let world = WorldSnapshot::new();
        let moved = |release| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO);
            tick(&mut state, &world, release);
            assert!(!state.first_tick);
            state.position.y != 110.0
        };
        assert!(moved(R1_20_5));
        assert!(!moved(R1_21_5));
    }

    #[test]
    fn an_orb_in_a_block_stays_from_1_21_6_if_its_motion_takes_it_out() {
        let mut world = stuck_world();
        let mut drew = |release, velocity| {
            let mut state = stuck(MovingEntityKind::ExperienceOrb, 100.2).on_server();
            state.velocity = velocity;
            tick_moving_entity(&mut state, &mut world, &one_draw(), release).random_used
        };
        assert_eq!(drew(R1_21_5, Vec3::new(2.0, 0.0, 0.0)), 1);
        assert_eq!(drew(R1_21_6, Vec3::new(2.0, 0.0, 0.0)), 0);
        assert_eq!(drew(R1_21_6, Vec3::ZERO), 1);
    }

    #[test]
    fn a_client_orb_leaves_a_block_itself_only_until_1_21_5() {
        let mut world = stuck_world();
        let mut drew = |release| {
            let mut state = stuck(MovingEntityKind::ExperienceOrb, 100.2);
            state.first_tick = false;
            tick_moving_entity(&mut state, &mut world, &one_draw(), release).random_used
        };
        assert_eq!(drew(R1_20), 1);
        assert_eq!(drew(R1_21_5), 0);
    }

    #[test]
    fn age_ends_an_item_on_the_server_alone_and_an_orb_on_both_sides() {
        let world = WorldSnapshot::new();
        let ended = |kind, server| {
            let mut state = entity(kind, 110.0, Vec3::ZERO);
            state.age = 5999;
            state.first_tick = false;
            if server {
                state = state.on_server();
            }
            tick(&mut state, &world, R26_3);
            state.removed
        };
        assert!(ended(MovingEntityKind::Item, true));
        assert!(!ended(MovingEntityKind::Item, false));
        assert!(ended(MovingEntityKind::ExperienceOrb, true));
        assert!(ended(MovingEntityKind::ExperienceOrb, false));
    }

    #[test]
    fn only_a_server_item_or_orb_looks_for_a_merge() {
        let world = WorldSnapshot::new();
        let check = |kind, server: bool, count| {
            let mut state = entity(kind, 110.0, Vec3::ZERO);
            state.tick_count = count;
            state.first_tick = false;
            if server {
                state = state.on_server();
            }
            tick(&mut state, &world, R26_3).merge_check
        };
        assert!(check(MovingEntityKind::Item, true, 39));
        assert!(!check(MovingEntityKind::Item, false, 39));
        assert!(check(MovingEntityKind::ExperienceOrb, true, 20));
        assert!(!check(MovingEntityKind::ExperienceOrb, false, 20));
    }

    #[test]
    fn a_pickup_delay_counts_down_but_not_for_ever() {
        let world = WorldSnapshot::new();
        let after = |delay| {
            let mut state = entity(MovingEntityKind::Item, 110.0, Vec3::ZERO);
            state.pickup_delay = delay;
            tick(&mut state, &world, R26_3);
            state.pickup_delay
        };
        assert_eq!(after(10), 9);
        assert_eq!(after(32767), 32767);
    }

    #[test]
    fn a_resting_item_still_takes_the_blocks_of_its_last_move_from_26_1() {
        let mut world = floor(0.6);
        world.set_block(
            (0, 100, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Web,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let held = |release| {
            let mut state = entity(MovingEntityKind::Item, 100.0, Vec3::ZERO);
            state.on_ground = true;
            state.id = 1;
            state.last_movements = vec![Movement {
                from: Vec3::new(0.5, 100.1, 0.5),
                to: Vec3::new(0.5, 100.0, 0.5),
                axis_order: None,
            }];
            tick(&mut state, &world, release);
            state.stuck_speed_multiplier != Vec3::ZERO
        };
        assert!(held(R26_1));
        assert!(!held(R1_20));
    }

    #[test]
    fn no_physics_clears_the_collision_flags_from_1_21_9() {
        let mut world = stuck_world();
        let mut flag = |release| {
            let mut state = MovingEntityState::new(MovingEntityKind::Item, Vec3::new(0.5, 100.5, 0.5), Vec3::ZERO).on_server();
            state.collided_vertically = true;
            tick_moving_entity(&mut state, &mut world, &one_draw(), release);
            state.collided_vertically
        };
        assert!(flag(R1_21_8));
        assert!(!flag(R1_21_9));
    }

    #[test]
    fn from_1_21_5_no_fall_counts_in_water() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 104, 0), fluid(FluidState::Water));
        let fall = |release| {
            let mut state = entity(MovingEntityKind::PrimedTnt, 104.5, Vec3::new(0.0, -0.1, 0.0)).on_server();
            state.in_water = true;
            let mut body = state.body(release);
            move_entity(&mut state, &mut body, &world, release);
            body.fall_distance
        };
        assert!(fall(R1_20) > 0.0);
        assert_eq!(fall(R1_21_5), 0.0);
    }

    #[test]
    fn a_client_counts_a_fall_only_before_1_21_2() {
        let world = WorldSnapshot::new();
        let fall = |release| {
            let mut state = entity(MovingEntityKind::Item, 110.0, Vec3::ZERO);
            tick(&mut state, &world, release);
            state.fall_distance
        };
        assert!(fall(R1_20) > 0.0);
        assert_eq!(fall(R1_21_3), 0.0);
    }

    #[test]
    fn a_cobweb_ends_the_fall_it_stops_before_1_21_2() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 105, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Web,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = entity(MovingEntityKind::Item, 105.5, Vec3::new(0.0, -0.1, 0.0)).on_server();
        tick(&mut state, &world, R1_13);
        assert_eq!(state.fall_distance, 0.0);
        assert_ne!(state.stuck_speed_multiplier, Vec3::ZERO);
    }

    // B290: the gates no fixture caught on a side, each run on the registered release either side of its boundary.

    const fn v(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
        MinecraftVersion::new(major, minor, patch)
    }

    #[test]
    fn a_bubble_column_pushes_tnt_once_a_tick_either_side_of_1_21_2() {
        // BubbleColumnBlock.entityInside, once: from Entity.move's checkInsideBlocks in 1.21.1 (1.21.1
        // Entity.java:738), from PrimedTnt.tick's applyEffectsFromBlocks after the move in 1.21.2 (1.21.2
        // PrimedTnt.java:83), whose move no longer checks (1.21.2 Entity.java:679-680)
        let column = |effect| BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            shape: Some(BlockShape {
                effect,
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let rise = |effect, release| {
            let mut world = WorldSnapshot::new();
            for y in 103..=106 {
                world.set_block((0, y, 0), column(effect));
            }
            // (held in the one cell: a move into the cell under it would meet a second column)
            let mut state = entity(MovingEntityKind::PrimedTnt, 104.0, Vec3::ZERO);
            state.no_gravity = true;
            tick(&mut state, &world, release);
            state.velocity.y
        };
        for release in [v(1, 21, 1), v(1, 21, 2)] {
            let pushed = rise(BlockEffect::BubbleUp, release) - rise(BlockEffect::None, release);
            assert!((pushed - 0.06 * 0.98).abs() < 1.0e-12, "{release:?}: {pushed}");
        }
    }

    #[test]
    fn a_client_counts_a_fall_in_1_21_1_and_not_in_1_21_2() {
        // (1.21.1 Entity.java:657 checkFallDamage on both sides; 1.21.2 Entity.java:679-680 not on a client
        // that does not control the entity)
        let world = WorldSnapshot::new();
        let fall = |release| {
            let mut state = entity(MovingEntityKind::Item, 110.0, Vec3::ZERO);
            tick(&mut state, &world, release);
            state.fall_distance
        };
        assert!(fall(v(1, 21, 1)) > 0.0);
        assert_eq!(fall(v(1, 21, 2)), 0.0);
    }

    /// An item where the corner of a water source meets a flowing lava block diagonally above it, which
    /// no spread turns to stone (LiquidBlock.shouldSpreadLiquid looks at the six faces).
    fn water_and_flowing_lava(with_lava: bool) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block((1, 104, 1), fluid(FluidState::Water));
        if with_lava {
            world.set_block(
                (0, 105, 0),
                BlockState {
                    fluid: FluidState::Lava,
                    fluid_level: 2,
                    fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                    ..BlockState::default()
                },
            );
        }
        world
    }

    fn item_by_the_fluids(world: &WorldSnapshot, position: Vec3, release: MinecraftVersion) -> MovingEntityState {
        let mut state = MovingEntityState::new(MovingEntityKind::Item, position, Vec3::ZERO);
        // (not a tick of item_check's: 5 % 2 and 5 % 40)
        state.tick_count = 4;
        state.first_tick = false;
        tick(&mut state, world, release);
        state
    }

    #[test]
    fn lava_pushes_an_item_from_1_16_and_in_water_only_from_1_16_2() {
        // Entity.updateInWaterStateAndDoFluidPushing: no lava before 1.16 (1.15.2 Entity.java:1031); from 1.16
        // the lava's current unless in water (1.16 Entity.java:1044, 1.16.1 :1042), and from 1.16.2 in water too,
        // where isInLava is its height (1.16.2 Entity.java:1068, 1179)
        let corner = Vec3::new(1.0, 104.8, 1.0);
        for release in [v(1, 16, 0), v(1, 16, 1)] {
            let both = item_by_the_fluids(&water_and_flowing_lava(true), corner, release);
            assert!(both.in_water, "{release:?}");
            assert_eq!(both.velocity.x, 0.0, "{release:?}: no lava is measured in water");
        }
        let both = item_by_the_fluids(&water_and_flowing_lava(true), corner, v(1, 16, 2));
        assert!(both.velocity.x > 0.0);
        assert!(both.in_water && both.in_lava);
        let water = item_by_the_fluids(&water_and_flowing_lava(false), corner, v(1, 16, 2));
        assert_eq!(water.velocity.x, 0.0);
        // in the lava alone
        let lava = Vec3::new(0.5, 105.05, 0.5);
        assert_eq!(item_by_the_fluids(&water_and_flowing_lava(true), lava, v(1, 15, 2)).velocity.x, 0.0);
        assert!(item_by_the_fluids(&water_and_flowing_lava(true), lava, v(1, 16, 0)).velocity.x > 0.0);
    }

    #[test]
    fn the_lava_flag_a_base_tick_halves_the_fall_by_is_the_ticks_own_before_1_14_and_from_1_16_2() {
        // isInLava as the base tick reads it: a query of the fluids before 1.14 (1.13.2 Entity.java:1143-1145),
        // what the last move left from 1.14 to 1.16.1 (1.14.4 Entity.java:568, 1.16.1 Entity.java:661), the
        // lava's height from 1.16.2 (1.16.2 Entity.java:1179)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 104, 0), fluid(FluidState::Lava));
        let fall = |release| {
            let mut state = entity(MovingEntityKind::Item, 104.2, Vec3::ZERO);
            state.tick_count = 4;
            state.first_tick = false;
            state.fall_distance = 4.0;
            state.in_lava = false;
            tick(&mut state, &world, release);
            state.fall_distance
        };
        assert!(fall(v(1, 13, 2)) < 2.5);
        assert!(fall(v(1, 14, 4)) > 3.5);
        assert!(fall(v(1, 16, 1)) > 3.5);
        assert!(fall(v(1, 16, 2)) < 2.5);
    }

    #[test]
    fn the_void_under_an_item_follows_the_worlds_bottom_from_1_17() {
        // (1.16.5 Entity.java:631-632 at -64; 1.17 Entity.java:559 getMinBuildHeight() - 64)
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 1.0e7);
        let gone = |release| {
            let mut state = entity(MovingEntityKind::Item, -100.0, Vec3::ZERO);
            state.no_gravity = true;
            tick(&mut state, &world, release);
            state.removed
        };
        assert!(gone(v(1, 16, 5)));
        assert!(!gone(v(1, 17, 0)));
    }

    #[test]
    fn a_block_falling_under_y_1_is_outside_until_1_17_and_under_the_worlds_bottom_after() {
        // (1.16.5 FallingBlockEntity.java:181 `y < 1 || y > 256`; 1.17 FallingBlockEntity.java:196 the level's
        // bottom and height; 1.21.1 FallingBlockEntity.java:144 getMaxBuildHeight, 1.21.2 :156 getMaxY, one lower)
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 1.0e7);
        let expired = |y, release| {
            let mut state = entity(MovingEntityKind::FallingBlock, y, Vec3::ZERO).on_server();
            state.time = 100;
            state.no_gravity = true;
            tick(&mut state, &world, release).expired
        };
        assert!(expired(-10.0, v(1, 16, 5)));
        assert!(!expired(-10.0, v(1, 17, 0)));
        assert!(!expired(320.0, v(1, 21, 1)));
        assert!(expired(320.0, v(1, 21, 2)));
    }

    #[test]
    fn the_client_clock_of_a_landed_block_is_1_18_and_1_18_1s_alone() {
        // (1.17.1 FallingBlockEntity has no discardTime; 1.18 FallingBlockEntity.java:103-106, 1.18.1 :105;
        // 1.18.2 FallingBlockEntity.java:120 none)
        let world = WorldSnapshot::new();
        let frozen = |release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 105.0, Vec3::ZERO);
            state.time = 4;
            state.discarding = true;
            tick(&mut state, &world, release);
            state.position.y == 105.0
        };
        assert!(!frozen(v(1, 17, 1)));
        assert!(frozen(v(1, 18, 0)));
        assert!(frozen(v(1, 18, 1)));
        assert!(!frozen(v(1, 18, 2)));
        // the first tick takes the block out in 1.18.1 (1.18.1 FallingBlockEntity.java:113) and not in 1.18.2 (:122)
        let mut world = floor(0.6);
        world.set_block((0, 105, 0), solid(12, 0.6));
        let clears = |release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 105.0, Vec3::ZERO).on_server();
            state.carried_block = 12;
            tick(&mut state, &world, release).clears_block
        };
        assert_eq!(clears(v(1, 18, 1)), Some((0, 105, 0)));
        assert_eq!(clears(v(1, 18, 2)), None);
    }

    #[test]
    fn a_block_that_falls_on_over_a_free_block_kept_its_drag_only_until_1_12() {
        // FallingBlock.canFallThrough under it: the tick returns. 1.11.2 FallingBlockEntity.java:115-117 has
        // dragged before (:122); 1.12.2 FallingBlockEntity.java:142-145 returns before its drag (:186-188)
        let mut world = floor(0.6);
        let inputs = MovingEntityInputs {
            falls_through_below: true,
            ..MovingEntityInputs::default()
        };
        let mut carried_on = |release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 100.0, Vec3::new(0.1, -0.1, 0.0)).on_server();
            state.time = 3;
            tick_moving_entity(&mut state, &mut world, &inputs, release);
            assert!(!state.removed);
            state.velocity.x
        };
        assert_eq!(carried_on(v(1, 11, 2)), 0.1 * f64::from(0.98_f32));
        assert_eq!(carried_on(v(1, 12, 2)), 0.1);
    }

    #[test]
    fn water_stops_a_concrete_powder_from_1_12_2() {
        // (1.11.2 FallingBlockEntity.java:94 no concrete; 1.12.2 FallingBlockEntity.java:128-129)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 104, 0), fluid(FluidState::Water));
        let landed = |release| {
            let mut state = entity(MovingEntityKind::FallingBlock, 104.5, Vec3::ZERO).on_server();
            state.time = 3;
            state.concrete_powder = true;
            tick(&mut state, &world, release).landed_at.is_some()
        };
        assert!(!landed(v(1, 11, 2)));
        assert!(landed(v(1, 12, 2)));
    }

    #[test]
    fn a_tie_between_up_and_north_went_up_in_1_8_and_north_from_1_9() {
        // pushOutOfBlocks takes the first of the nearest sides: 1.8.9 Entity.java:1539-1557 west, east, up,
        // north, south; 1.9.4 Entity.java:1720-1745 west, east, north, south, up
        let world = stuck_world();
        let inputs = one_draw();
        let after = |release| {
            // the middle at 0.75 of the cell's height and a quarter of it from the north
            let state = MovingEntityState::new(MovingEntityKind::Item, Vec3::new(0.5, 100.625, 0.25), Vec3::ZERO);
            let mut body = state.body(release);
            let mut draws = Draws {
                random: &inputs.random,
                used: 0,
                missing: false,
            };
            assert!(move_towards_closest_space(&mut body, &world, &mut draws, release));
            body.velocity
        };
        let speed = f64::from(0.5_f32 * 0.2 + 0.1);
        assert_eq!(after(v(1, 8, 8)), Vec3::new(0.0, speed, 0.0));
        assert_eq!(after(v(1, 9, 4)), Vec3::new(0.0, 0.0, -speed));
    }

    #[test]
    fn an_item_slides_on_the_block_under_its_middle_until_1_20_and_on_what_holds_it_after() {
        // over the edge of a row of ice, its middle over the air beside it: 1.19.4 ItemEntity.java:137 the block at
        // y - 1.0 under the middle, 1.20 ItemEntity.java:137, 173-175 getOnPos(0.999999F), in the supporting
        // block's column (1.20 Entity.java:805-815)
        let mut world = WorldSnapshot::new();
        for z in -2..=2 {
            world.set_block((0, 99, z), solid(1, 0.98));
        }
        let slid = |release| {
            let mut state = entity(MovingEntityKind::Item, 100.0, Vec3::new(0.0, 0.0, 0.1));
            state.position.x = 1.05;
            state.on_ground = true;
            tick(&mut state, &world, release);
            assert!(state.on_ground);
            state.velocity.z
        };
        assert_eq!(slid(v(1, 19, 4)), 0.1 * f64::from(0.6_f32 * 0.98));
        assert_eq!(slid(v(1, 20, 0)), 0.1 * f64::from(0.98_f32 * 0.98));
        // y - 1.0 is a whole block down: on a box whose top is a hair under a whole block, y - 0.999999F would
        // name the cell of the item itself (1.19.4 ItemEntity.java:137)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 99, 0), solid(1, 0.98));
        world.set_entity_colliders(vec![crate::world::EntityCollider {
            bounds: Aabb::new(Vec3::new(0.0, 100.5, -2.0), Vec3::new(1.0, 100.999_999_5, 3.0)),
            only_from_above: false,
        }]);
        let mut state = entity(MovingEntityKind::Item, 101.0, Vec3::new(0.0, 0.0, 0.1));
        state.on_ground = true;
        tick(&mut state, &world, v(1, 19, 4));
        assert!(state.on_ground);
        assert_eq!(state.velocity.z, 0.1 * f64::from(0.98_f32 * 0.98));
    }

    #[test]
    fn lava_lifts_an_item_in_1_16_and_not_in_1_15_2() {
        // (1.15.2 ItemEntity.java:85 water alone; 1.16 ItemEntity.java:103 lava)
        let mut world = WorldSnapshot::new();
        for y in 103..=105 {
            world.set_block((0, y, 0), fluid(FluidState::Lava));
        }
        let lift = |release| {
            let mut state = entity(MovingEntityKind::Item, 104.2, Vec3::new(0.1, 0.0, 0.0));
            state.tick_count = 1;
            tick(&mut state, &world, release);
            state.velocity.y
        };
        assert!(lift(v(1, 15, 2)) < 0.0);
        assert!(lift(v(1, 16, 0)) > 0.0);
    }

    #[test]
    fn a_client_item_leaves_a_block_in_1_10_2_and_stays_in_1_11_2() {
        // (1.10.2 ItemEntity.java:76 on both sides; 1.11.2 ItemEntity.java:95-99 the server's)
        let mut world = stuck_world();
        let mut used = |release| {
            let mut client = stuck(MovingEntityKind::Item, 100.4);
            let outcome = tick_moving_entity(&mut client, &mut world, &one_draw(), release);
            (client.no_physics, outcome.random_used)
        };
        assert_eq!(used(v(1, 10, 2)), (true, 1));
        assert_eq!(used(v(1, 11, 2)), (false, 0));
    }

    #[test]
    fn an_entity_that_collides_makes_an_item_leave_from_1_14() {
        // 1.13.2 ItemEntity.java:92 pushOutOfBlocks asks the blocks alone (Entity.java:1918, a null entity);
        // 1.14.4 ItemEntity.java:90-92 noCollision(this), which counts a shulker's or a boat's box
        // (EntityGetter.java:54-69), then checkInBlock with no question of its own (Entity.java:2072)
        let mut world = WorldSnapshot::new();
        world.set_entity_colliders(vec![crate::world::EntityCollider {
            bounds: Aabb::new(Vec3::new(0.0, 100.0, 0.0), Vec3::new(1.0, 101.0, 1.0)),
            only_from_above: false,
        }]);
        let mut left = |release| {
            let mut state = MovingEntityState::new(MovingEntityKind::Item, Vec3::new(0.5, 100.4, 0.5), Vec3::ZERO).on_server();
            let outcome = tick_moving_entity(&mut state, &mut world, &one_draw(), release);
            (state.no_physics, outcome.random_used)
        };
        assert_eq!(left(v(1, 13, 2)), (false, 0));
        assert_eq!(left(v(1, 14, 4)), (true, 1));
    }

    #[test]
    fn an_item_a_hair_into_a_block_is_free_from_1_17() {
        // (1.16.5 ItemEntity.java:113 noCollision(this); 1.17 ItemEntity.java:111 the box deflated by 1.0E-7)
        let mut world = stuck_world();
        let mut stuck_in = |release| {
            let mut state =
                MovingEntityState::new(MovingEntityKind::Item, Vec3::new(0.5, 101.0 - 5.0e-8, 0.5), Vec3::ZERO).on_server();
            tick_moving_entity(&mut state, &mut world, &one_draw(), release);
            state.no_physics
        };
        assert!(stuck_in(v(1, 16, 5)));
        assert!(!stuck_in(v(1, 17, 0)));
    }

    #[test]
    fn a_resting_item_takes_no_blocks_in_1_21_11_and_those_of_its_last_move_in_26_1() {
        // (1.21.11 ItemEntity.java:133-135; 26.1 ItemEntity.java:136-137)
        let mut world = floor(0.6);
        world.set_block(
            (0, 100, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Web,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let held = |release| {
            let mut state = entity(MovingEntityKind::Item, 100.0, Vec3::ZERO);
            state.on_ground = true;
            state.id = 1;
            state.last_movements = vec![Movement {
                from: Vec3::new(0.5, 100.1, 0.5),
                to: Vec3::new(0.5, 100.0, 0.5),
                axis_order: None,
            }];
            tick(&mut state, &world, release);
            state.stuck_speed_multiplier != Vec3::ZERO
        };
        assert!(!held(v(1, 21, 11)));
        assert!(held(v(26, 1, 0)));
    }

    #[test]
    fn an_orb_counts_its_throw_time_and_looks_for_no_merge_until_1_17() {
        // (1.16.5 ExperienceOrb.java:77-78 throwTime; 1.17 ExperienceOrb.java has none, and the merge every 20 ticks, :77)
        let world = WorldSnapshot::new();
        let after = |release| {
            let mut state = entity(MovingEntityKind::ExperienceOrb, 110.0, Vec3::ZERO).on_server();
            state.pickup_delay = 5;
            state.tick_count = 0;
            let outcome = tick(&mut state, &world, release);
            (state.pickup_delay, outcome.merge_check)
        };
        assert_eq!(after(v(1, 16, 5)), (4, false));
        assert_eq!(after(v(1, 17, 0)), (5, true));
    }


    /// A cell whose block is tagged a full cube before 1.13 but whose collision is its lower half
    /// (a stair's getCollisionBoundingBox fills the cell, 1.12.2 World.java:1835-1838,
    /// StairsBlock.java:89-97).
    fn tagged_half(world: &mut WorldSnapshot, at: BlockPosition) {
        world.set_block(
            at,
            BlockState {
                id: 2,
                tags: crate::world::LEGACY_FULL_CUBE,
                shape: Some(BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)), ..BlockShape::full_block() }),
                ..BlockState::default()
            },
        );
    }

    #[test]
    fn a_free_item_in_a_tagged_full_cube_is_pushed_out_on_1_8_8_alone() {
        // 1.8.8 Entity.pushOutOfBlocks goes on when the box collides or the cell is a full cube
        // (`list.isEmpty() && !isBlockFullCube(pos)` returns, Entity.java:1525); 1.9.4 asks the
        // collision list alone (Entity.java:1708). The item's box is over the block's half, its
        // middle in the cell.
        let mut world = WorldSnapshot::new();
        tagged_half(&mut world, (0, 100, 0));
        let inputs = one_draw();
        let pushed = |release| {
            let state = MovingEntityState::new(MovingEntityKind::Item, Vec3::new(0.1, 100.6, 0.5), Vec3::ZERO);
            let mut body = state.body(release);
            let mut draws = Draws { random: &inputs.random, used: 0, missing: false };
            (move_towards_closest_space(&mut body, &world, &mut draws, release), body.velocity)
        };
        let (moved, velocity) = pushed(R1_8);
        assert!(moved);
        assert!(velocity.x < 0.0, "out west, the nearest side: {velocity:?}");
        assert_eq!(pushed(R1_9), (false, Vec3::ZERO));
    }

    #[test]
    fn a_tagged_full_cube_shuts_its_side_on_1_12_2_and_not_on_1_13_2() {
        // before 1.13 a side is shut by World.isBlockFullCube, the tag (1.12.2 World.java:1835-1838);
        // 1.13.2 by the collision shape's being the whole cell (Level.java:1526): the item stuck in
        // stone near its west face goes west on 1.13.2, and on 1.12.2 to the next nearest side
        let inputs = one_draw();
        let after = |release| {
            let mut world = stuck_world();
            tagged_half(&mut world, (-1, 100, 0));
            let mut body = stuck(MovingEntityKind::Item, 100.4).body(release);
            let mut draws = Draws { random: &inputs.random, used: 0, missing: false };
            move_towards_closest_space(&mut body, &world, &mut draws, release);
            body.velocity
        };
        let shut = after(R1_12);
        assert_eq!(shut.x, 0.0);
        assert!(shut.y > 0.0, "{shut:?}");
        assert!(after(R1_13).x < 0.0);
    }

    #[test]
    fn concrete_powder_finds_water_past_the_top_of_soul_sand_from_1_14() {
        // 1.13.2 Level.clip stops at a block with collision by its outline (Block.java:469-470,
        // the soul sand's whole cell); 1.14.4 BlockGetter.clip by the collision shape
        // (ClipContext.java:37-42, the soul sand's 14/16), and goes on into the water source beside
        let mut world = WorldSnapshot::new();
        let lower = BlockShape { bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0)), ..BlockShape::full_block() };
        world.set_block_shapes_with_outline((0, 100, 0), BlockState { id: 3, shape: Some(lower), ..BlockState::default() }, vec![lower], vec![BlockShape::full_block()]);
        world.set_block((1, 100, 0), fluid(FluidState::Water));
        let from = Vec3::new(0.9, 100.99, 0.5);
        let to = Vec3::new(1.5, 100.5, 0.5);
        assert_eq!(water_along(&world, from, to, R1_13), None);
        assert_eq!(water_along(&world, from, to, R1_14), Some((1, 100, 0)));
        // and a source under water: 1.13.2's box is its own 8/9 (FlowingFluid.java:432), 1.14.4's the
        // whole cell where the same fluid is over it (FlowingFluid.java:469-473), so a trace along
        // the top ninth starts in the water from 1.14 alone
        let mut stacked = WorldSnapshot::new();
        stacked.set_block((0, 100, 0), fluid(FluidState::Water));
        stacked.set_block((0, 101, 0), fluid(FluidState::Water));
        let (from, to) = (Vec3::new(0.5, 100.95, 0.5), Vec3::new(0.5, 100.95, 1.5));
        assert_eq!(water_along(&stacked, from, to, R1_13), None);
        assert_eq!(water_along(&stacked, from, to, R1_14), Some((0, 100, 0)));
    }
}

#[cfg(test)]
mod mob_travel_tests {
    use super::*;
    use crate::world::{BlockShape, BlockState};

    const R1_13: MinecraftVersion = MinecraftVersion::new(1, 13, 2);
    const R1_14: MinecraftVersion = MinecraftVersion::new(1, 14, 4);
    const R1_20_4: MinecraftVersion = MinecraftVersion::new(1, 20, 4);
    const R1_20_5: MinecraftVersion = MinecraftVersion::new(1, 20, 5);
    const R1_21_5: MinecraftVersion = MinecraftVersion::new(1, 21, 5);
    const R1_21_6: MinecraftVersion = MinecraftVersion::new(1, 21, 6);
    const R26_3: MinecraftVersion = MinecraftVersion::new(26, 3, 0);

    fn mob(kind: MobTravelKind) -> MobTravelState {
        MobTravelState {
            kind,
            position: Vec3::new(0.5, 110.0, 0.5),
            velocity: Vec3::new(0.1, 0.0, 0.0),
            yaw: 0.0,
            input: Vec3::new(0.0, 0.0, 1.0),
            speed: 0.1,
            half_width: 0.25,
            height: 0.5,
            max_step: 0.6,
            bounding_box: None,
            on_ground: false,
            collided_horizontally: false,
            collided_vertically: false,
            in_water: false,
            in_lava: false,
            no_gravity: false,
            stuck_speed_multiplier: Vec3::ZERO,
            fall_distance: 0.0,
            has_target: false,
            guardian_moving: false,
            move_out_of_range: false,
            climbing: false,
            pushed_by_fluid: kind.pushed_by_fluid(),
            fluid_jump_threshold: 0.4,
            under_water: false,
            wants_to_swim: false,
            near_home: false,
            marker: false,
            sweet_berry_bush_immune: false,
            body_item: false,
            floats_in_liquids: false,
            tick_count: 0,
            slow_falling: false,
            levitation_amplifier: None,
            dolphins_grace: false,
            gravity: None,
            water_movement_efficiency: 0.0,
            air_drag_modifier: None,
            discard_friction: false,
        }
    }

    fn after(state: MobTravelState, release: MinecraftVersion) -> MobTravelState {
        let world = WorldSnapshot::new();
        let mut state = state;
        tick_mob_travel(&mut state, &world, release);
        state
    }

    #[test]
    fn a_landing_mobs_bounce_compensates_its_gravity_attribute_and_air_drag_modifier_from_26_2() {
        // Entity.move's bounce from 26.2: (portion * getEffectiveGravity - current.y) times
        // lerp(portion, 1, getAirDrag) times the restitution (26.2 Entity.java:848-858, 26.3
        // :878-888); LivingEntity.getEffectiveGravity reads Attributes.GRAVITY (26.2
        // LivingEntity.java:2335-2342) and getAirDrag the 0.98F modified by AIR_DRAG_MODIFIER
        // (26.2 LivingEntity.java:2414-2416). Before 26.2 there is no bounce.
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 100, 0),
            BlockState { shape: Some(BlockShape::full_block()), bounce_restitution: 0.8, ..BlockState::default() },
        );
        let bounced = |gravity: Option<f64>, air_drag_modifier: Option<f64>, release| {
            let mut squid = mob(MobTravelKind::Squid);
            squid.position = Vec3::new(0.5, 101.2, 0.5);
            squid.velocity = Vec3::new(0.0, -0.5, 0.0);
            squid.gravity = gravity;
            squid.air_drag_modifier = air_drag_modifier;
            let mut squid = squid;
            tick_mob_travel(&mut squid, &world, release);
            squid.velocity.y
        };
        let expected = |gravity: f64, drag: f64| {
            let portion = -0.2 / -0.5;
            (portion * gravity + 0.5) * (1.0 + portion * (drag - 1.0)) * 0.8
        };
        let drag = f64::from(0.98_f32);
        let r26_2 = MinecraftVersion::new(26, 2, 0);
        assert!((bounced(None, None, r26_2) - expected(0.08, drag)).abs() < 1.0e-12);
        assert!((bounced(Some(0.2), None, r26_2) - expected(0.2, drag)).abs() < 1.0e-12);
        let unmodified = crate::simulation::compute_modified_friction(drag, 0.0);
        assert!((bounced(None, Some(0.0), R26_3) - expected(0.08, unmodified)).abs() < 1.0e-12);
        assert_ne!(bounced(None, Some(0.0), R26_3), bounced(None, None, R26_3));
        let before = MinecraftVersion::new(26, 1, 2);
        assert_eq!(bounced(Some(0.2), Some(0.0), before), bounced(None, None, before));
    }

    #[test]
    fn a_long_jumping_goat_keeps_its_motion_through_the_air_from_1_17() {
        // LivingEntity.travel: with shouldDiscardFriction the motion is set as the gravity left it,
        // undragged (1.17 LivingEntity.java:2123-2126, 636-638; 1.21.11 :2327-2331); a goat parses
        // to the plain LivingEntity travel
        let jumped = |discard: bool, release| {
            let mut goat = mob(MobTravelKind::parse("goat").expect("a goat travels"));
            goat.input = Vec3::ZERO;
            goat.velocity = Vec3::new(0.5, 0.3, 0.0);
            goat.discard_friction = discard;
            after(goat, release).velocity
        };
        let kept = jumped(true, MinecraftVersion::new(1, 17, 0));
        assert_eq!(kept.x, 0.5);
        assert_eq!(kept.y, 0.3 - 0.08);
        let dragged = jumped(false, MinecraftVersion::new(1, 17, 0));
        assert_eq!(dragged.x, 0.5 * f64::from(0.91_f32));
        assert_eq!(jumped(true, R26_3).x, 0.5);
        let before = MinecraftVersion::new(1, 16, 5);
        assert_eq!(jumped(true, before), jumped(false, before), "no such flag before 1.17");
    }

    #[test]
    fn a_squid_goes_by_its_motion_alone() {
        let squid = after(mob(MobTravelKind::Squid), R26_3);
        assert_eq!(squid.position.x, 0.6);
        assert_eq!(squid.velocity, Vec3::new(0.1, 0.0, 0.0));
    }

    #[test]
    fn a_fish_in_water_swims_at_a_hundredth_and_sinks_with_no_target() {
        let mut fish = mob(MobTravelKind::Fish);
        fish.in_water = true;
        fish.velocity = Vec3::ZERO;
        let swum = after(fish.clone(), R26_3);
        assert_eq!(swum.velocity.y, -0.005);
        assert!((swum.velocity.z - 0.01 * 0.9).abs() < 1.0e-9);
        fish.has_target = true;
        assert_eq!(after(fish, R26_3).velocity.y, 0.0);
    }

    #[test]
    fn a_guardian_swims_at_a_tenth_and_its_spikes_keep_it_up() {
        let mut guardian = mob(MobTravelKind::Guardian);
        guardian.in_water = true;
        guardian.velocity = Vec3::ZERO;
        let swum = after(guardian.clone(), R26_3);
        assert_eq!(swum.velocity.y, -0.005);
        assert!((swum.velocity.z - 0.1 * 0.9).abs() < 1.0e-8);
        guardian.guardian_moving = true;
        assert_eq!(after(guardian, R26_3).velocity.y, 0.0);
    }

    #[test]
    fn a_guardian_drags_in_float_before_1_14() {
        let mut guardian = mob(MobTravelKind::Guardian);
        guardian.in_water = true;
        guardian.input = Vec3::ZERO;
        guardian.has_target = true;
        assert_eq!(after(guardian.clone(), R1_13).velocity.x, 0.1 * f64::from(0.9_f32));
        assert_eq!(after(guardian, R1_14).velocity.x, 0.1 * 0.9);
    }

    #[test]
    fn out_of_water_a_fish_takes_a_living_entitys_travel_and_falls() {
        let fish = mob(MobTravelKind::Fish);
        assert!(after(fish, R26_3).velocity.y < 0.0);
    }

    #[test]
    fn a_ghast_flies_at_0_02_and_a_phantom_at_0_2_only_from_1_21_6() {
        let air = |kind, release| {
            let mut flyer = mob(kind);
            flyer.velocity = Vec3::ZERO;
            after(flyer, release).velocity.z
        };
        let drag = f64::from(0.91_f32);
        assert!((air(MobTravelKind::Ghast, R26_3) - 0.02 * drag).abs() < 1.0e-9);
        assert!((air(MobTravelKind::Phantom, R1_21_5) - 0.02 * drag).abs() < 1.0e-9);
        assert!((air(MobTravelKind::Phantom, R1_21_6) - 0.2 * drag).abs() < 1.0e-8);
    }

    #[test]
    fn an_allay_flies_at_its_own_speed() {
        let mut allay = mob(MobTravelKind::Allay);
        allay.velocity = Vec3::ZERO;
        assert!((after(allay, R1_21_5).velocity.z - 0.1 * f64::from(0.91_f32)).abs() < 1.0e-8);
    }

    #[test]
    fn water_and_lava_hold_a_flying_mob() {
        let mut flyer = mob(MobTravelKind::Ghast);
        flyer.input = Vec3::ZERO;
        flyer.in_water = true;
        assert_eq!(after(flyer.clone(), R26_3).velocity.x, 0.1 * f64::from(0.8_f32));
        flyer.in_water = false;
        flyer.in_lava = true;
        assert_eq!(after(flyer, R26_3).velocity.x, 0.05);
    }

    #[test]
    fn the_ground_slows_a_flying_mob_only_until_1_21_6() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 109, 0),
            BlockState {
                shape: Some(BlockShape {
                    slipperiness: 0.6,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let on_ground = |release| {
            let mut flyer = mob(MobTravelKind::Ghast);
            flyer.input = Vec3::ZERO;
            flyer.on_ground = true;
            tick_mob_travel(&mut flyer, &world, release);
            flyer.velocity.x
        };
        assert_eq!(on_ground(R1_21_5), 0.1 * f64::from(0.6_f32 * 0.91_f32));
        assert_eq!(on_ground(R1_21_6), 0.1 * f64::from(0.91_f32));
    }

    #[test]
    fn a_flying_mob_reads_the_ground_a_whole_block_down_before_1_20() {
        // a ghast at y + 0.6 over a low slab of ice: y - 1.0 is the stone under it, y - 0.5000001
        // the ice (1.19.4 FlyingMob.java:31 against 1.15 - 1.19.4 Entity.getBlockPosBelowThatAffectsMyMovement)
        let mut world = WorldSnapshot::new();
        let block = |slipperiness, top| BlockState {
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, top, 1.0)),
                slipperiness,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        world.set_block((0, 109, 0), block(0.6, 1.0));
        world.set_block((0, 110, 0), block(0.98, 0.5));
        let on_ground = |release| {
            let mut flyer = mob(MobTravelKind::Ghast);
            flyer.position.y = 110.6;
            flyer.input = Vec3::ZERO;
            flyer.on_ground = true;
            tick_mob_travel(&mut flyer, &world, release);
            flyer.velocity.x
        };
        assert_eq!(on_ground(MinecraftVersion::new(1, 19, 4)), 0.1 * f64::from(0.6_f32 * 0.91_f32));
        assert_eq!(on_ground(MinecraftVersion::new(1, 15, 0)), 0.1 * f64::from(0.6_f32 * 0.91_f32));
    }

    #[test]
    fn a_vex_goes_through_a_block_that_stops_a_squid() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 110, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let go = |kind| {
            let mut state = mob(kind);
            state.no_gravity = true;
            state.velocity = Vec3::new(0.6, 0.0, 0.0);
            tick_mob_travel(&mut state, &world, R26_3);
            state.position.x
        };
        assert!(go(MobTravelKind::Vex) > 0.75 + 0.3);
        assert!(go(MobTravelKind::Squid) <= 0.75);
    }

    fn web_at(position: (i32, i32, i32)) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn a_cobweb_stops_a_mob_after_its_travel_from_1_21_2() {
        let world = web_at((0, 110, 0));
        let mut state = mob(MobTravelKind::Living);
        state.velocity = Vec3::ZERO;
        state.input = Vec3::ZERO;
        state.fall_distance = 3.0;
        tick_mob_travel(&mut state, &world, R26_3);
        assert_eq!(state.stuck_speed_multiplier, Vec3::new(0.25, java_float(0.05), 0.25));
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn a_spider_goes_through_a_cobweb_and_a_wither_through_anything() {
        let world = web_at((0, 110, 0));
        for kind in [MobTravelKind::Spider, MobTravelKind::Wither] {
            let mut state = mob(kind);
            tick_mob_travel(&mut state, &world, R26_3);
            assert_eq!(state.stuck_speed_multiplier, Vec3::ZERO);
        }
    }

    #[test]
    fn a_vex_feels_a_cobweb_though_it_goes_through_blocks() {
        let world = web_at((0, 110, 0));
        let held = |release| {
            let mut state = mob(MobTravelKind::Vex);
            state.no_gravity = true;
            tick_mob_travel(&mut state, &world, release);
            state.stuck_speed_multiplier
        };
        assert_ne!(held(R1_14), Vec3::ZERO);
        assert_ne!(held(R26_3), Vec3::ZERO);
    }

    #[test]
    fn a_cobweb_ends_the_fall_of_the_move_that_reaches_it_before_1_21_2() {
        let world = web_at((0, 110, 0));
        let mut state = mob(MobTravelKind::Living);
        state.position = Vec3::new(0.5, 110.5, 0.5);
        state.velocity = Vec3::new(0.0, -0.2, 0.0);
        state.input = Vec3::ZERO;
        state.fall_distance = 3.0;
        tick_mob_travel(&mut state, &world, R1_20_4);
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn a_bee_keeps_its_climb_as_the_rest_from_1_20_5_and_by_the_air_alone_from_26_2() {
        let mut bee = mob(MobTravelKind::FlyingAnimal);
        bee.velocity = Vec3::new(0.0, 0.5, 0.0);
        bee.input = Vec3::ZERO;
        bee.no_gravity = true;
        assert_eq!(after(bee.clone(), R1_20_4).velocity.y, 0.5 * f64::from(0.98_f32));
        assert_eq!(after(bee.clone(), R1_20_5).velocity.y, 0.5 * f64::from(0.91_f32));
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 109, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        bee.on_ground = true;
        let on_ground = |release| {
            let mut state = bee.clone();
            tick_mob_travel(&mut state, &world, release);
            state.velocity.y
        };
        // (a block's 0.6 of friction in the drag until 26.2)
        assert_eq!(on_ground(R1_21_5), 0.5 * f64::from(0.6_f32 * 0.91));
        assert_eq!(on_ground(R26_3), 0.5 * f64::from(0.91_f32));
    }

    #[test]
    fn a_plain_mob_is_turned_by_its_yaw() {
        let mut state = mob(MobTravelKind::Living);
        state.velocity = Vec3::ZERO;
        state.yaw = 90.0;
        let turned = after(state, R26_3);
        assert!(turned.velocity.x < 0.0);
        assert!(turned.velocity.z.abs() < 1.0e-7);
    }

    #[test]
    fn a_mob_without_gravity_does_not_sink_in_water() {
        let mut state = mob(MobTravelKind::Living);
        state.velocity = Vec3::ZERO;
        state.input = Vec3::ZERO;
        state.in_water = true;
        state.no_gravity = true;
        assert_eq!(after(state, R26_3).velocity.y, 0.0);
    }

    #[test]
    fn a_plain_mob_steps_up_as_high_as_it_is_given() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 109, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 108, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let climbed = |max_step| {
            let mut state = mob(MobTravelKind::Living);
            state.position = Vec3::new(0.5, 109.0, 0.5);
            state.velocity = Vec3::new(0.4, 0.0, 0.0);
            state.input = Vec3::ZERO;
            state.on_ground = true;
            state.max_step = max_step;
            tick_mob_travel(&mut state, &world, R26_3);
            state.position.y
        };
        assert_eq!(climbed(0.6), 109.0);
        assert_eq!(climbed(1.5), 110.0);
    }

    #[test]
    fn dolphins_and_turtles_sink_and_axolotls_and_frogs_do_not() {
        let swum = |kind, speed| {
            let mut state = mob(kind);
            state.in_water = true;
            state.velocity = Vec3::ZERO;
            state.speed = speed;
            after(state, R26_3).velocity
        };
        assert_eq!(swum(MobTravelKind::Dolphin, 0.3).y, -0.005);
        assert!((swum(MobTravelKind::Dolphin, 0.3).z - 0.3 * 0.9).abs() < 1.0e-6);
        assert_eq!(swum(MobTravelKind::Turtle, 0.9).y, -0.005);
        assert!((swum(MobTravelKind::Turtle, 0.9).z - 0.1 * 0.9).abs() < 1.0e-6);
        assert_eq!(swum(MobTravelKind::Axolotl, 0.3).y, 0.0);
        assert_eq!(swum(MobTravelKind::Frog, 0.3).y, 0.0);
    }

    #[test]
    fn a_turtle_going_home_near_it_holds_its_depth() {
        let mut turtle = mob(MobTravelKind::Turtle);
        turtle.in_water = true;
        turtle.near_home = true;
        assert_eq!(after(turtle, R26_3).velocity.y, 0.0);
    }

    #[test]
    fn a_drowned_swims_only_where_it_wants_and_under_water_from_1_21_2() {
        let mut drowned = mob(MobTravelKind::Drowned);
        drowned.in_water = true;
        drowned.velocity = Vec3::ZERO;
        drowned.wants_to_swim = true;
        let swims = |state: MobTravelState, release| (after(state, release).velocity.z - 0.01 * 0.9).abs() < 1.0e-6;
        assert!(swims(drowned.clone(), R1_14));
        assert!(!swims(drowned.clone(), R26_3));
        drowned.under_water = true;
        assert!(swims(drowned.clone(), R26_3));
        drowned.wants_to_swim = false;
        assert!(!swims(drowned, R26_3));
    }

    #[test]
    fn a_polar_bear_keeps_more_of_its_motion_in_water() {
        let swum = |kind| {
            let mut state = mob(kind);
            state.in_water = true;
            state.input = Vec3::ZERO;
            state.velocity = Vec3::new(1.0, 0.0, 0.0);
            after(state, R26_3).velocity.x
        };
        assert_eq!(swum(MobTravelKind::Living), f64::from(0.8_f32));
        assert_eq!(swum(MobTravelKind::PolarBear), f64::from(0.98_f32));
    }

    #[test]
    fn a_spider_climbs_by_its_flag_and_not_by_a_ladder() {
        let mut spider = mob(MobTravelKind::Spider);
        spider.velocity = Vec3::new(0.0, -0.5, 0.0);
        spider.input = Vec3::ZERO;
        spider.climbing = true;
        let held = after(spider.clone(), R26_3).velocity.y;
        spider.climbing = false;
        assert!(held > after(spider, R26_3).velocity.y);
    }

    #[test]
    fn an_armor_stand_travels_only_with_its_physics() {
        let mut stand = mob(MobTravelKind::ArmorStand);
        stand.input = Vec3::ZERO;
        let at = stand.position;
        assert_ne!(after(stand.clone(), R26_3).position, at);
        stand.marker = true;
        assert_eq!(after(stand.clone(), R26_3).position, at);
        assert_ne!(after(stand.clone(), R1_13).position, at);
        stand.marker = false;
        stand.no_gravity = true;
        assert_eq!(after(stand, R1_13).position, at);
    }

    #[test]
    fn what_flies_counts_no_fall_and_a_blaze_does() {
        let fell = |kind| {
            let mut state = mob(kind);
            state.velocity = Vec3::new(0.0, -0.5, 0.0);
            state.input = Vec3::ZERO;
            after(state, R26_3).fall_distance
        };
        for kind in [MobTravelKind::Bat, MobTravelKind::FlyingAnimal, MobTravelKind::Ghast, MobTravelKind::Phantom, MobTravelKind::Allay] {
            assert_eq!(fell(kind), 0.0);
        }
        assert_eq!(fell(MobTravelKind::Living), 0.5);
    }

    #[test]
    fn no_player_controls_a_mob_so_1_21_4_keeps_its_fall() {
        // (1.21.4 Entity.java:721 skips checkFallDamage for isControlledByPlayer alone: physrec/me_mob_cow_falls)
        let fell = |release| {
            let mut state = mob(MobTravelKind::Living);
            state.velocity = Vec3::new(0.0, -0.5, 0.0);
            state.input = Vec3::ZERO;
            after(state, release).fall_distance
        };
        assert_eq!(fell(MinecraftVersion::new(1, 21, 3)), fell(MinecraftVersion::new(1, 21, 4)));
        assert_ne!(fell(MinecraftVersion::new(1, 21, 4)), 0.0);
    }

    #[test]
    fn a_current_moves_what_it_can_push() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 110, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 2,
                fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let carried = |pushed| {
            let mut state = mob(MobTravelKind::Living);
            state.velocity = Vec3::ZERO;
            state.input = Vec3::ZERO;
            state.no_gravity = true;
            state.pushed_by_fluid = pushed;
            tick_mob_travel(&mut state, &world, R1_21_5);
            state.velocity.x
        };
        assert!(carried(true) > 0.0);
        assert_eq!(carried(false), 0.0);
    }

    #[test]
    fn the_lava_is_shallow_by_the_mobs_own_threshold() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 110, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let lands = |threshold| {
            let mut state = mob(MobTravelKind::Living);
            state.position = Vec3::new(0.5, 110.5, 0.5);
            state.velocity = Vec3::new(0.0, 0.1, 0.0);
            state.input = Vec3::ZERO;
            state.in_lava = true;
            state.fluid_jump_threshold = threshold;
            tick_mob_travel(&mut state, &world, R1_21_5);
            state.velocity.y
        };
        assert_ne!(lands(0.0), lands(0.9));
    }

    #[test]
    fn what_goes_through_blocks_loses_its_collision_flags_from_1_21_9() {
        let mut vex = mob(MobTravelKind::Vex);
        vex.no_gravity = true;
        vex.collided_horizontally = true;
        assert!(after(vex.clone(), MinecraftVersion::new(1, 21, 8)).collided_horizontally);
        assert!(!after(vex, MinecraftVersion::new(1, 21, 9)).collided_horizontally);
    }

    #[test]
    fn levitation_lifts_and_slow_falling_slows_a_mob() {
        let mut cow = mob(MobTravelKind::Living);
        cow.velocity = Vec3::new(0.0, -0.5, 0.0);
        cow.input = Vec3::ZERO;
        let plain = after(cow.clone(), R26_3).velocity.y;
        let mut slow = cow.clone();
        slow.slow_falling = true;
        assert!(after(slow, R26_3).velocity.y > plain);
        let mut up = cow;
        up.levitation_amplifier = Some(1);
        up.velocity.y = 0.0;
        assert!(after(up, R26_3).velocity.y > 0.0);
    }

    #[test]
    fn gravity_and_water_efficiency_are_the_attributes_of_their_release() {
        let mut cow = mob(MobTravelKind::Living);
        cow.velocity = Vec3::ZERO;
        cow.input = Vec3::ZERO;
        cow.gravity = Some(0.01);
        assert!((after(cow.clone(), R26_3).velocity.y + 0.01 * f64::from(0.98_f32)).abs() < 1.0e-9);
        assert!((after(cow.clone(), R1_20_4).velocity.y + 0.08 * f64::from(0.98_f32)).abs() < 1.0e-9);
        let mut swimmer = mob(MobTravelKind::Living);
        swimmer.in_water = true;
        swimmer.velocity = Vec3::ZERO;
        let plain = after(swimmer.clone(), R26_3).velocity.z;
        swimmer.water_movement_efficiency = 1.0;
        assert!(after(swimmer, R26_3).velocity.z > plain);
    }

    #[test]
    fn the_names_are_the_registrys() {
        assert_eq!(MobTravelKind::parse("glow_squid"), Some(MobTravelKind::Squid));
        assert_eq!(MobTravelKind::parse("tadpole"), Some(MobTravelKind::Fish));
        assert_eq!(MobTravelKind::parse("elder_guardian"), Some(MobTravelKind::Guardian));
        assert_eq!(MobTravelKind::parse("cow"), Some(MobTravelKind::Living));
        assert_eq!(MobTravelKind::parse("minecart"), None);
    }



    const fn v(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
        MinecraftVersion::new(major, minor, patch)
    }

    #[test]
    fn a_flying_mob_reads_the_ground_it_stands_on_from_1_20() {
        // the setting of a_flying_mob_reads_the_ground_a_whole_block_down_before_1_20 on 1.20: the low slab
        // of ice at y - 0.5000001 (1.20 FlyingMob.java:31 getBlockPosBelowThatAffectsMyMovement)
        let mut world = WorldSnapshot::new();
        let block = |slipperiness, top| BlockState {
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, top, 1.0)),
                slipperiness,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        world.set_block((0, 109, 0), block(0.6, 1.0));
        world.set_block((0, 110, 0), block(0.98, 0.5));
        let on_ground = |release| {
            let mut flyer = mob(MobTravelKind::Ghast);
            flyer.position.y = 110.6;
            flyer.input = Vec3::ZERO;
            flyer.on_ground = true;
            tick_mob_travel(&mut flyer, &world, release);
            flyer.velocity.x
        };
        assert_eq!(on_ground(v(1, 19, 4)), 0.1 * f64::from(0.6_f32 * 0.91_f32));
        assert_eq!(on_ground(v(1, 20, 0)), 0.1 * f64::from(0.98_f32 * 0.91_f32));
    }
    #[test]
    fn the_air_drag_modifier_compensates_a_bounce_on_26_2_itself() {
        // 26.2 LivingEntity.getAirDrag is the 0.98F modified by AIR_DRAG_MODIFIER (LivingEntity.java:
        // 2414-2416), which Entity.move's bounce compensates (Entity.java:848-858); 26.1.2 has neither
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 100, 0),
            BlockState { shape: Some(BlockShape::full_block()), bounce_restitution: 0.8, ..BlockState::default() },
        );
        let bounced = |air_drag_modifier: Option<f64>, release| {
            let mut squid = mob(MobTravelKind::Squid);
            squid.position = Vec3::new(0.5, 101.2, 0.5);
            squid.velocity = Vec3::new(0.0, -0.5, 0.0);
            squid.air_drag_modifier = air_drag_modifier;
            tick_mob_travel(&mut squid, &world, release);
            squid.velocity.y
        };
        let r26_2 = MinecraftVersion::new(26, 2, 0);
        let portion = -0.2 / -0.5;
        let unmodified = crate::simulation::compute_modified_friction(f64::from(0.98_f32), 0.0);
        let expected = (portion * 0.08 + 0.5) * (1.0 + portion * (unmodified - 1.0)) * 0.8;
        assert!((bounced(Some(0.0), r26_2) - expected).abs() < 1.0e-12);
        assert_ne!(bounced(Some(0.0), r26_2), bounced(None, r26_2));
    }
}
