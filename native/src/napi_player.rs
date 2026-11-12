use napi::bindgen_prelude::{Float64Array, Float64ArraySlice, Result};
use napi::Env;
use napi::Error;
use napi_derive::napi;

use crate::entity::{EntityPush, NearbyEntity, NearbyEntityKind};
use crate::math::{Aabb, Vec3};
use crate::napi_api::{shulker_box, NativeShulkerBox};
use crate::napi_api::{
    finish_local_player_tick, move_local_player_tick, push_local_player_by_pistons,
    validate_kept_box, validate_position, validate_size, validate_tick_state,
    validated_simulation_config, NativeAabb, NativeMovingPiston, NativeSimulationConfig,
    NativeSimulationState, NativeWorld, MAX_ATTACHED_ROCKETS,
};
use crate::piston::MovingPiston;
use crate::simulation::{set_pose_height, MovementSpeedAttribute, PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::versioned;

/// The per-tick inputs of a NativePlayer, by their index in its input array.
pub mod input {
    pub const POSITION: usize = 0;
    pub const VELOCITY: usize = 3;
    pub const YAW: usize = 6;
    pub const PITCH: usize = 7;
    pub const LOOK: usize = 8;
    /// The movement keys: left (1) or right (-1), forward (1) or back (-1).
    pub const STRAFE: usize = 11;
    pub const FORWARD: usize = 12;
    pub const HALF_WIDTH: usize = 13;
    pub const HEIGHT: usize = 14;
    pub const EYE_HEIGHT: usize = 15;
    pub const FLAGS: usize = 16;
    pub const FALL_DISTANCE: usize = 17;
    pub const JUMP_TICKS: usize = 18;
    pub const AUTO_JUMP_TIME: usize = 19;
    pub const STUCK_SPEED_MULTIPLIER: usize = 20;
    /// The riptide level of a trident let go of before the tick, 0 for none.
    pub const RIPTIDE_LAUNCH: usize = 23;
    pub const FIREWORK_ROCKETS: usize = 24;
    pub const FIREWORK_ROCKETS_BEFORE_PLAYER: usize = 25;
    pub const FLYING_SPEED: usize = 26;
    /// UseEffects.speedMultiplier of the item in use: 0.2, a spear's 1.0 (1.21.11+).
    pub const ITEM_USE_SPEED_MULTIPLIER: usize = 27;
    /// The float yaw in degrees that YAW holds in radians: moveFlying turns it into radians
    /// itself before 1.9.
    pub const YAW_DEGREES: usize = 28;
    /// The look along the yaw the last aiStep left in yHeadRot, with this tick's pitch: what a
    /// 1.11 rocket that ticks before the player boosts along (D-222). NaN: `LOOK`'s.
    pub const HEAD_LOOK: usize = 29;
    pub const LENGTH: usize = 32;
}

/// What the arrays of this build are: anything that moves an index, or gives a number or a flag
/// another meaning, takes a new version.
pub const LAYOUT_VERSION: u32 = 3;

#[napi(object)]
pub struct NativePlayerLayout {
    pub version: u32,
    pub input_length: u32,
    pub output_length: u32,
    pub entity_length: u32,
}

/// The packed arrays this build reads and writes, for a caller that keeps them apart from it.
#[napi]
pub fn native_player_layout() -> NativePlayerLayout {
    NativePlayerLayout {
        version: LAYOUT_VERSION,
        input_length: input::LENGTH as u32,
        output_length: output::LENGTH as u32,
        entity_length: ENTITY_LENGTH as u32,
    }
}

/// The bits of the input array's flags.
pub mod input_flag {
    pub const JUMP_REQUESTED: u32 = 1 << 0;
    pub const SPRINTING: u32 = 1 << 1;
    pub const WAS_SPRINTING: u32 = 1 << 2;
    pub const SNEAKING: u32 = 1 << 3;
    pub const MOVING_SLOWLY: u32 = 1 << 4;
    pub const SWIMMING: u32 = 1 << 5;
    pub const FLYING: u32 = 1 << 6;
    pub const FALL_FLYING: u32 = 1 << 7;
    pub const USING_ITEM: u32 = 1 << 8;
    pub const AUTO_JUMP: u32 = 1 << 9;
    pub const NO_PHYSICS: u32 = 1 << 10;
    pub const ON_GROUND: u32 = 1 << 11;
    pub const COLLIDED_HORIZONTALLY: u32 = 1 << 12;
    pub const MINOR_HORIZONTAL_COLLISION: u32 = 1 << 13;
    pub const COLLIDED_VERTICALLY: u32 = 1 << 14;
    pub const VERTICAL_COLLISION_BELOW: u32 = 1 << 15;
    pub const IN_WATER: u32 = 1 << 16;
    pub const IN_LAVA: u32 = 1 << 17;
    pub const IN_POWDER_SNOW: u32 = 1 << 18;
    /// The player is where the last tick left it: it keeps the position that tick started from
    /// (for the block that supports it). Without it that is dropped, as after a teleport; the
    /// bounding box kept before 1.17 goes through setBoundingBox.
    pub const UNMOVED: u32 = 1 << 19;
    /// The movement speed attribute holds the sprint modifier (sprintModifier).
    pub const SPRINT_MODIFIER: u32 = 1 << 20;
    /// Removed from the level this tick (LocalPlayer.tickDeath at the 20th tick of death, 1.17+).
    pub const REMOVED: u32 = 1 << 21;
    /// The riptide spin attack runs this tick (LivingEntity.autoSpinAttackTicks > 0, 1.13+).
    pub const AUTO_SPIN_ATTACK: u32 = 1 << 22;
    /// In a bed (LivingEntity.isSleeping; Player.isSleeping before 1.14).
    pub const SLEEPING: u32 = 1 << 23;
    /// Its health is 0 (LivingEntity.isAlive is false).
    pub const DEAD: u32 = 1 << 24;
    /// The bits above: a higher one is of a layout this build does not know.
    pub(super) const KNOWN: u32 = (1 << 25) - 1;
}

/// The results of a NativePlayer's tick, by their index in its output array.
pub mod output {
    pub const POSITION: usize = 0;
    pub const VELOCITY: usize = 3;
    pub const FLAGS: usize = 6;
    pub const FALL_DISTANCE: usize = 7;
    pub const JUMP_TICKS: usize = 8;
    pub const AUTO_JUMP_TIME: usize = 9;
    pub const STUCK_SPEED_MULTIPLIER: usize = 10;
    /// The bounding box kept before 1.17 (min, max), NaN without one.
    pub const BOUNDING_BOX: usize = 13;
    /// The damage LivingEntity.causeFallDamage takes on the tick's landing (calculateFallDamage,
    /// computeFallDamage before 1.21), 0 on a tick without one.
    pub const FALL_DAMAGE: usize = 19;
    /// What the tick's blocks hand hurt, added up over its contacts (a cactus and fire together):
    /// the server's alone, 0 on a client state and on the next tick.
    pub const CONTACT_DAMAGE: usize = 20;
    /// What the tick's flight into a wall hands hurt: the server's alone, 0 on a client state and
    /// on the next tick.
    pub const WALL_DAMAGE: usize = 21;
    /// The freeze damage dealt so far, a running count (LivingEntity.aiStep's every 40 ticks, the
    /// server's alone): the amount of a tick is its rise.
    pub const FREEZE_DAMAGE: usize = 22;
    /// The powder snow modifier the movement speed attribute holds after the tick's frost update
    /// (LivingEntity.aiStep's, the client's too up to 1.21.4), NaN with none or with no attribute
    /// in the state.
    pub const FROST: usize = 23;
    pub const LENGTH: usize = 24;
}

/// The bits of the output array's flags.
pub mod output_flag {
    pub const ON_GROUND: u32 = 1 << 0;
    pub const COLLIDED_HORIZONTALLY: u32 = 1 << 1;
    pub const MINOR_HORIZONTAL_COLLISION: u32 = 1 << 2;
    pub const COLLIDED_VERTICALLY: u32 = 1 << 3;
    pub const IN_WATER: u32 = 1 << 4;
    pub const IN_LAVA: u32 = 1 << 5;
    pub const IN_POWDER_SNOW: u32 = 1 << 6;
    pub const IN_WEB: u32 = 1 << 7;
    /// Still fall flying after the tick: flight and, from 1.21.5, a climbable block stop a glide.
    pub const FALL_FLYING: u32 = 1 << 8;
    /// The spin attack ended in the tick: it hit a living entity, or touching none a wall.
    pub const AUTO_SPIN_ATTACK_ENDED: u32 = 1 << 9;
    /// The tick's move was slowed by the stuck speed multiplier (Entity.isInWeb before 1.14).
    pub const MOVE_SLOWED: u32 = 1 << 10;
    /// The tick's move ran Block.updateEntityMovementAfterFallOn (once a move, whatever it hit).
    pub const BLOCK_FALL_CALLBACK: u32 = 1 << 11;
    /// The eyes after the tick, where from 26.1 checkFallDamage's fluid update leaves them.
    pub const EYE_IN_WATER: u32 = 1 << 12;
    /// Entity.verticalCollisionBelow after the move, which the input array takes back.
    pub const VERTICAL_COLLISION_BELOW: u32 = 1 << 13;
}

/// The numbers of one nearby entity in the entities array: its kind (0 inert, 1 living, 2 boat,
/// 3 shulker, 4 happy ghast, 5 a living entity that neither pushes nor collides, 6 a happy ghast
/// on its still timeout, 7 an attached rocket, 8 a minecart), whether it ticks before the player, where the player's move meets it
/// (position, box), where its own tick leaves it (position, box), and its flags (entity_flag).
pub const ENTITY_LENGTH: usize = 21;

/// The last kind a nearby entity has in the entities array (nearby_entity).
const LAST_ENTITY_KIND: u32 = 8;

/// The bits of a nearby entity's flags in the entities array.
pub mod entity_flag {
    /// The team collision rules keep it from pushing the player (EntitySelector.pushableBy).
    pub const NO_TEAM_PUSH: u32 = 1 << 0;
    /// A spectating player, which the riptide spin's sweep leaves out (NO_SPECTATORS).
    pub const SPECTATOR: u32 = 1 << 1;
    /// The bits above: a higher one is of a layout this build does not know.
    pub(super) const KNOWN: u32 = (1 << 2) - 1;
}

/// A local player whose state the engine keeps between ticks (simulateVersionedTick's tick): the
/// caller sends only what changes each tick, in a Float64Array laid out as `input`, and reads the
/// result from another laid out as `output`. The rest of the state (attributes, effects, the
/// counters the engine keeps) stays until setState replaces it.
#[napi]
pub struct NativePlayer {
    version: String,
    release: MinecraftVersion,
    config: PhysicsConfig,
    state: SimulationState,
    /// The movement speed attribute's parts (movementSpeedAttribute), which the frost update of
    /// each tick reads and changes.
    movement_speed: Option<MovementSpeedAttribute>,
    nearby_entities: Vec<NearbyEntity>,
    pushes: Vec<EntityPush>,
    piston_deltas: [f64; 3],
    /// The riptide spin attack ended in the last tick.
    auto_spin_attack_ended: bool,
    /// The attached rockets left for finishTick.
    rockets_after_player: u32,
    /// tickMove ran and finishTick has not.
    move_pending: bool,
    /// The NativeWorld this state last ticked in, which setState lets go of.
    world: Option<u64>,
}

#[napi]
impl NativePlayer {
    #[napi(constructor)]
    pub fn new(
        version: String,
        state: NativeSimulationState,
        config: NativeSimulationConfig,
    ) -> Result<Self> {
        if !versioned::is_supported(&version) {
            return Err(Error::from_reason(format!(
                "unsupported decompiled Minecraft version: {version}"
            )));
        }
        let release = MinecraftVersion::parse(&version)
            .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))?;
        let mut player = Self {
            version,
            release,
            config: validated_simulation_config(config)?,
            state: SimulationState::default(),
            movement_speed: None,
            nearby_entities: Vec::new(),
            pushes: Vec::new(),
            piston_deltas: [0.0; 3],
            auto_spin_attack_ended: false,
            rockets_after_player: 0,
            move_pending: false,
            world: None,
        };
        player.set_state(state)?;
        Ok(player)
    }

    /// Replaces the whole state (its nearby entities too, when it has them): what the last tick
    /// reported, a tick tickMove left unfinished and the world it ticked in are the old state's, and
    /// go with it. A state that is refused changes nothing.
    #[napi]
    pub fn set_state(&mut self, mut state: NativeSimulationState) -> Result<()> {
        validate_tick_state("NativePlayer.setState", &state)?;
        let movement_speed = state
            .movement_speed_attribute
            .take()
            .map(MovementSpeedAttribute::try_from)
            .transpose()?;
        self.movement_speed = movement_speed;
        self.pushes.clear();
        self.piston_deltas = [0.0; 3];
        self.auto_spin_attack_ended = false;
        self.rockets_after_player = 0;
        self.move_pending = false;
        self.world = None;
        self.nearby_entities = state
            .nearby_entities
            .take()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        state.moving_pistons = None;
        self.state = state.into();
        Ok(())
    }

    /// The whole state after the last tick or push.
    #[napi]
    pub fn state(&self) -> NativeSimulationState {
        let mut state: NativeSimulationState = self.state.into();
        state.movement_speed_attribute = self.movement_speed.as_ref().map(Into::into);
        state
    }

    /// Replaces the bounding box kept before 1.17 (Entity.setBoundingBox: a refitted size).
    #[napi]
    pub fn set_bounding_box(&mut self, bounding_box: Option<NativeAabb>) -> Result<()> {
        let bounding_box = bounding_box.map(Into::into);
        validate_kept_box("NativePlayer.setBoundingBox", bounding_box)?;
        self.state.bounding_box = bounding_box;
        Ok(())
    }

    /// One tick: the input array's values go into the state, the tick runs, and its results come
    /// back laid out as `output`. `entities` holds the nearby entities of this tick (ENTITY_LENGTH
    /// numbers each, in the order the client ticks them).
    #[napi]
    pub fn tick<'env>(
        &mut self,
        env: &'env Env,
        world: &mut NativeWorld,
        input: Float64ArraySlice,
        entities: Float64ArraySlice,
    ) -> Result<Float64ArraySlice<'env>> {
        self.step_move(world, &input, &entities)?;
        self.finish(world);
        self.output_array(env)
    }

    /// `tick` up to Player.tick's pose update (26.2 Player.java:266): the rockets and entities
    /// before the player and its move. The caller chooses the pose where the move left the player,
    /// and finishTick runs what ticks after the player with the box of that pose.
    #[napi]
    pub fn tick_move<'env>(
        &mut self,
        env: &'env Env,
        world: &mut NativeWorld,
        input: Float64ArraySlice,
        entities: Float64ArraySlice,
    ) -> Result<Float64ArraySlice<'env>> {
        self.step_move(world, &input, &entities)?;
        self.output_array(env)
    }

    /// What ticks after the player in a tick tickMove started: the rockets and entities after it,
    /// which meet the box `height` tall, the pose update's, as the block entities' pushes then
    /// do. The results come back laid out as `output`, and entityPushes tells the whole tick's.
    /// Without a tickMove before it there is no tick to finish, which fails and changes nothing.
    #[napi]
    pub fn finish_tick<'env>(
        &mut self,
        env: &'env Env,
        world: &mut NativeWorld,
        height: f64,
    ) -> Result<Float64ArraySlice<'env>> {
        self.finish_with_height(world, height)?;
        self.output_array(env)
    }

    /// What an opening shulker box's lid at a cell does to the player, after its tick as the block
    /// entities tick after the entities; the state comes back laid out as `output`.
    #[napi]
    pub fn push_by_shulker_box(
        &mut self,
        world: &mut NativeWorld,
        shulker: NativeShulkerBox,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<Float64Array> {
        self.check_world("NativePlayer.pushByShulkerBox", world)?;
        // no shulker box before 1.11 (1.10.2 BlockEntity.java:243-263, 1.11.2 :263)
        if !self.release.at_least(1, 11, 0) {
            return Err(Error::from_reason(format!("{:?} has no shulker box", self.release)));
        }
        let shulker = shulker_box(shulker)?;
        crate::shulker_box::push_entity(&mut self.state, world.snapshot(), shulker, (x, y, z), self.config, self.release);
        Ok(Float64Array::new(self.output()))
    }

    /// The pushes the nearby entities took back from the player in the last tick (Entity.push: the
    /// other way), three numbers each: the entity's index in the entities array, then x and z.
    #[napi]
    pub fn entity_pushes(&self) -> Float64Array {
        let values: Vec<f64> = self
            .pushes
            .iter()
            .flat_map(|push| [push.index as f64, push.x, push.z])
            .collect();
        Float64Array::new(values)
    }

    /// The pushes of moving pistons after the tick (simulateVersionedTick's movingPistons), each
    /// call after the others of the same tick; the results come back laid out as `output`.
    #[napi]
    pub fn push_by_moving_pistons(
        &mut self,
        world: &mut NativeWorld,
        pistons: Vec<NativeMovingPiston>,
    ) -> Result<Float64Array> {
        self.check_world("NativePlayer.pushByMovingPistons", world)?;
        let pistons = pistons
            .into_iter()
            .map(MovingPiston::try_from)
            .collect::<Result<Vec<_>>>()?;
        push_local_player_by_pistons(
            world,
            &mut self.state,
            &self.nearby_entities,
            &pistons,
            &mut self.piston_deltas,
            self.config,
            self.release,
        );
        Ok(Float64Array::new(self.output()))
    }
}

impl NativePlayer {
    /// A state ticked in one world says nothing of another (what supports it, what it stands in, what
    /// its last tick left): going on in a new world is a choice setState makes. The first tick of a
    /// state chooses its world.
    fn check_world(&self, operation: &str, world: &NativeWorld) -> Result<()> {
        if self.world.is_some_and(|last| last != world.identity) {
            return Err(Error::from_reason(format!(
                "{operation}: this player's state ticked in another NativeWorld: setState moves it to a new one"
            )));
        }
        Ok(())
    }

    /// finishTick's checks and its finish, without the output.
    fn finish_with_height(&mut self, world: &mut NativeWorld, height: f64) -> Result<()> {
        if !self.move_pending {
            return Err(Error::from_reason(
                "NativePlayer.finishTick: no tickMove started a tick, or it was finished or replaced",
            ));
        }
        self.check_world("NativePlayer.finishTick", world)?;
        validate_size("NativePlayer.finishTick", self.state.half_width, height)?;
        set_pose_height(&mut self.state, height);
        self.finish(world);
        Ok(())
    }

    #[cfg(test)]
    fn step(
        &mut self,
        world: &mut NativeWorld,
        input: &[f64],
        entities: &[f64],
    ) -> Result<Vec<f64>> {
        self.step_move(world, input, entities)?;
        self.finish(world);
        Ok(self.output())
    }

    fn step_move(
        &mut self,
        world: &mut NativeWorld,
        input: &[f64],
        entities: &[f64],
    ) -> Result<()> {
        // a tick that fails leaves the player as the last one that ran
        self.check_world("NativePlayer.tick", world)?;
        validate_input(input)?;
        let nearby_entities = validated_entities(entities)?;
        self.world = Some(world.identity);
        let before = self.state;
        let movement_speed_before = self.movement_speed.clone();
        let rockets = self.take_input(input);
        let auto_spin_attack = input[input::FLAGS] as u32 & input_flag::AUTO_SPIN_ATTACK != 0;
        let (pushes, spin_ended) = move_local_player_tick(
            &self.version,
            self.release,
            world,
            &mut self.state,
            self.movement_speed.as_mut(),
            &nearby_entities,
            rockets,
            auto_spin_attack,
            self.config,
        )
        .inspect_err(|_| {
            self.state = before;
            self.movement_speed = movement_speed_before;
        })?;
        self.nearby_entities = nearby_entities;
        self.piston_deltas = [0.0; 3];
        self.move_pending = true;
        self.pushes = pushes;
        self.auto_spin_attack_ended = spin_ended;
        self.rockets_after_player = rockets.0 - rockets.1.min(rockets.0);
        Ok(())
    }

    fn finish(&mut self, world: &mut NativeWorld) {
        self.move_pending = false;
        let pushes = finish_local_player_tick(
            self.release,
            world,
            &mut self.state,
            &self.nearby_entities,
            std::mem::take(&mut self.rockets_after_player),
            self.config,
        );
        self.pushes.extend(pushes);
    }

    /// The input array into the state; returns the attached rockets and those before the player.
    fn take_input(&mut self, values: &[f64]) -> (u32, u32) {
        use input::*;
        let state = &mut self.state;
        let flags = values[FLAGS] as u32;
        let flag = |bit: u32| flags & bit != 0;
        let position = vec3(values, POSITION);
        if !flag(input_flag::UNMOVED) {
            state.previous_position = None;
        }
        state.position = position;
        // Entity.setDeltaMovement: from 1.21.11 a motion that is not finite is refused, and the
        // player keeps the one its last tick left (1.21.11 Entity.java:3727-3732)
        state.set_delta_movement(vec3(values, VELOCITY), self.release);
        state.yaw = values[YAW];
        state.yaw_degrees = Some(values[YAW_DEGREES] as f32);
        state.pitch = values[PITCH];
        state.look = vec3(values, LOOK);
        let head_look = vec3(values, HEAD_LOOK);
        state.head_look = (head_look.x.is_finite() && head_look.y.is_finite() && head_look.z.is_finite())
            .then_some(head_look);
        state.input = Vec3::new(values[STRAFE], 0.0, values[FORWARD]);
        state.half_width = values[HALF_WIDTH];
        state.height = values[HEIGHT];
        state.eye_height = Some(values[EYE_HEIGHT]);
        state.jump_requested = flag(input_flag::JUMP_REQUESTED);
        state.sprinting = flag(input_flag::SPRINTING);
        state.was_sprinting = flag(input_flag::WAS_SPRINTING);
        state.sprint_modifier = Some(flag(input_flag::SPRINT_MODIFIER));
        state.sneaking = flag(input_flag::SNEAKING);
        state.moving_slowly = Some(flag(input_flag::MOVING_SLOWLY));
        state.swimming = flag(input_flag::SWIMMING);
        state.flying = flag(input_flag::FLYING);
        state.fall_flying = flag(input_flag::FALL_FLYING);
        state.using_item = flag(input_flag::USING_ITEM);
        state.item_use_speed_multiplier = Some(values[ITEM_USE_SPEED_MULTIPLIER] as f32);
        state.removed = flag(input_flag::REMOVED);
        state.auto_jump = flag(input_flag::AUTO_JUMP);
        state.sleeping = flag(input_flag::SLEEPING);
        state.dead = flag(input_flag::DEAD);
        state.no_physics = flag(input_flag::NO_PHYSICS);
        state.on_ground = flag(input_flag::ON_GROUND);
        state.collided_horizontally = flag(input_flag::COLLIDED_HORIZONTALLY);
        state.minor_horizontal_collision = flag(input_flag::MINOR_HORIZONTAL_COLLISION);
        state.collided_vertically = flag(input_flag::COLLIDED_VERTICALLY);
        state.vertical_collision_below = flag(input_flag::VERTICAL_COLLISION_BELOW);
        state.in_water = flag(input_flag::IN_WATER);
        state.in_lava = flag(input_flag::IN_LAVA);
        state.in_powder_snow = flag(input_flag::IN_POWDER_SNOW);
        state.fall_distance = values[FALL_DISTANCE];
        // the landing damage is this tick's: check_fall_damage sets it only on a landing
        state.fall_damage = 0.0;
        state.jump_ticks = values[JUMP_TICKS] as u32;
        state.auto_jump_time = values[AUTO_JUMP_TIME] as u32;
        state.stuck_speed_multiplier = vec3(values, STUCK_SPEED_MULTIPLIER);
        let riptide = values[RIPTIDE_LAUNCH] as u8;
        state.riptide_launch = (riptide > 0).then_some(riptide);
        state.flying_speed = values[FLYING_SPEED];
        let rockets = values[FIREWORK_ROCKETS] as u32;
        (
            rockets,
            (values[FIREWORK_ROCKETS_BEFORE_PLAYER] as u32).min(rockets),
        )
    }

    /// The output as a Float64Array the engine owns, copied in: no external buffer to finalize,
    /// which cost about a sixth of a kept tick.
    fn output_array<'env>(&self, env: &'env Env) -> Result<Float64ArraySlice<'env>> {
        let mut values = [0.0; output::LENGTH];
        self.write_output(&mut values);
        Float64ArraySlice::copy_from(env, values)
    }

    fn output(&self) -> Vec<f64> {
        let mut values = vec![0.0; output::LENGTH];
        self.write_output(&mut values);
        values
    }

    fn write_output(&self, values: &mut [f64]) {
        use output::*;
        let state = &self.state;
        write_vec3(values, POSITION, state.position);
        write_vec3(values, VELOCITY, state.velocity);
        let flags = [
            (state.on_ground, output_flag::ON_GROUND),
            (
                state.collided_horizontally,
                output_flag::COLLIDED_HORIZONTALLY,
            ),
            (
                state.minor_horizontal_collision,
                output_flag::MINOR_HORIZONTAL_COLLISION,
            ),
            (state.collided_vertically, output_flag::COLLIDED_VERTICALLY),
            (state.in_water, output_flag::IN_WATER),
            (state.in_lava, output_flag::IN_LAVA),
            (state.in_powder_snow, output_flag::IN_POWDER_SNOW),
            (state.in_web, output_flag::IN_WEB),
            (state.fall_flying, output_flag::FALL_FLYING),
            (
                self.auto_spin_attack_ended,
                output_flag::AUTO_SPIN_ATTACK_ENDED,
            ),
            (state.move_slowed, output_flag::MOVE_SLOWED),
            (state.block_fall_callback, output_flag::BLOCK_FALL_CALLBACK),
            (state.eye_in_water, output_flag::EYE_IN_WATER),
            (
                state.vertical_collision_below,
                output_flag::VERTICAL_COLLISION_BELOW,
            ),
        ];
        values[FLAGS] = f64::from(
            flags
                .iter()
                .filter(|(set, _)| *set)
                .fold(0, |bits, (_, bit)| bits | bit),
        );
        values[FALL_DISTANCE] = state.fall_distance;
        values[JUMP_TICKS] = f64::from(state.jump_ticks);
        values[AUTO_JUMP_TIME] = f64::from(state.auto_jump_time);
        write_vec3(values, STUCK_SPEED_MULTIPLIER, state.stuck_speed_multiplier);
        let bounds = state.bounding_box.unwrap_or(Aabb::new(
            Vec3::new(f64::NAN, f64::NAN, f64::NAN),
            Vec3::new(f64::NAN, f64::NAN, f64::NAN),
        ));
        write_vec3(values, BOUNDING_BOX, bounds.min);
        write_vec3(values, BOUNDING_BOX + 3, bounds.max);
        values[FALL_DAMAGE] = state.fall_damage;
        values[CONTACT_DAMAGE] = state.contact_damage;
        values[WALL_DAMAGE] = state.wall_damage;
        values[FREEZE_DAMAGE] = state.freeze_damage;
        values[FROST] = self
            .movement_speed
            .as_ref()
            .and_then(|attribute| attribute.frost)
            .unwrap_or(f64::NAN);
    }
}

/// A number of the input array that holds a count or bits: whole and within `max`. What a cast
/// would make of anything else (0 for NaN, the top of the type for a large one) is no value the
/// caller sent.
fn whole_number(name: &str, value: f64, max: u32) -> Result<u32> {
    if value.fract() != 0.0 || value < 0.0 || value > f64::from(max) || value.is_nan() {
        return Err(Error::from_reason(format!(
            "NativePlayer.tick: {name} is {value}, it must be a whole number from 0 to {max}"
        )));
    }
    Ok(value as u32)
}

/// All a tick reads of the input array before it replaces any of the kept state.
fn validate_input(values: &[f64]) -> Result<()> {
    use input::*;
    if values.len() != LENGTH {
        return Err(Error::from_reason(format!(
            "NativePlayer.tick: the input array holds {} numbers, and this build reads {LENGTH} (NativePlayerInput)",
            values.len()
        )));
    }
    let flags = whole_number("the input flags", values[FLAGS], u32::MAX)?;
    if flags & !input_flag::KNOWN != 0 {
        return Err(Error::from_reason(format!(
            "NativePlayer.tick: the input flags {flags:#x} set a bit this build does not know"
        )));
    }
    whole_number("jumpTicks", values[JUMP_TICKS], u32::MAX)?;
    whole_number("autoJumpTime", values[AUTO_JUMP_TIME], u32::MAX)?;
    whole_number("riptideLaunch", values[RIPTIDE_LAUNCH], u32::from(u8::MAX))?;
    whole_number("fireworkRockets", values[FIREWORK_ROCKETS], MAX_ATTACHED_ROCKETS)?;
    whole_number("fireworkRocketsBeforePlayer", values[FIREWORK_ROCKETS_BEFORE_PLAYER], MAX_ATTACHED_ROCKETS)?;
    validate_position("NativePlayer.tick", vec3(values, POSITION))?;
    validate_size("NativePlayer.tick", values[HALF_WIDTH], values[HEIGHT])
}

/// The entities array: whole records of kinds and flags this build knows, with no partial record
/// dropped at its end.
fn validated_entities(values: &[f64]) -> Result<Vec<NearbyEntity>> {
    if values.len() % ENTITY_LENGTH != 0 {
        return Err(Error::from_reason(format!(
            "NativePlayer.tick: the entities array holds {} numbers, not a whole number of entities of {ENTITY_LENGTH}",
            values.len()
        )));
    }
    values
        .chunks_exact(ENTITY_LENGTH)
        .map(|entity| {
            whole_number("an entity's kind", entity[0], LAST_ENTITY_KIND)?;
            let flags = whole_number("an entity's flags", entity[20], u32::MAX)?;
            if flags & !entity_flag::KNOWN != 0 {
                return Err(Error::from_reason(format!(
                    "NativePlayer.tick: the entity flags {flags:#x} set a bit this build does not know"
                )));
            }
            Ok(nearby_entity(entity))
        })
        .collect()
}

fn vec3(values: &[f64], at: usize) -> Vec3 {
    Vec3::new(values[at], values[at + 1], values[at + 2])
}

fn write_vec3(values: &mut [f64], at: usize, vector: Vec3) {
    values[at] = vector.x;
    values[at + 1] = vector.y;
    values[at + 2] = vector.z;
}

fn nearby_entity(values: &[f64]) -> NearbyEntity {
    let kind = match values[0] as u32 {
        1 => NearbyEntityKind::Living,
        2 => NearbyEntityKind::Boat,
        3 => NearbyEntityKind::Shulker,
        4 => NearbyEntityKind::HappyGhast,
        5 => NearbyEntityKind::StillLiving,
        6 => NearbyEntityKind::HappyGhastStaysStill,
        7 => NearbyEntityKind::AttachedRocket,
        8 => NearbyEntityKind::Minecart,
        _ => NearbyEntityKind::Inert,
    };
    let flags = values[20] as u32;
    let aabb = |at: usize| Aabb::new(vec3(values, at), vec3(values, at + 3));
    NearbyEntity {
        kind,
        ticks_before_player: values[1] != 0.0,
        position: vec3(values, 2),
        bounding_box: aabb(5),
        tick_position: vec3(values, 11),
        tick_bounding_box: aabb(14),
        team_pushes: flags & entity_flag::NO_TEAM_PUSH == 0,
        spectator: flags & entity_flag::SPECTATOR != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        entity_flag, input, input_flag, output, output_flag, NativePlayer, ENTITY_LENGTH,
        MAX_ATTACHED_ROCKETS,
    };
    use crate::entity::EntityPush;
    use crate::math::{Aabb, Vec3};
    use crate::napi_api::{
        simulate_versioned_tick, simulate_versioned_tick_outcome, simulate_versioned_ticks, NativeAabb, NativeAttributeModifier, NativeBlockShape, NativeBlockUpdate, NativeBoatVehicle, NativeMovementSpeedAttribute, NativeSimulationConfig,
        NativeSimulationState, NativeVec3, NativeWorld,
    };
    use crate::simulation::{ServerFireDraws, SimulationState};
    use crate::world::{BlockEffect, BlockShape, BlockState};

    fn vec3(x: f64, y: f64, z: f64) -> NativeVec3 {
        NativeVec3 { x, y, z }
    }

    fn config() -> NativeSimulationConfig {
        NativeSimulationConfig {
            gravity: None,
            drag: None,
            movement_speed: None,
            sprint_multiplier: None,
            jump_velocity: None,
            step_height: None,
            negligible_velocity: None,
        }
    }

    fn block(x: i32, y: i32, max_y: f64) -> NativeBlockUpdate {
        NativeBlockUpdate {
            x,
            y,
            z: 0,
            id: Some(1),
            state_version: None,
            block_entity: None,
            remove_block_entity: None,
            properties: None,
            tags: None,
            shape: Some(NativeBlockShape {
                min_x: 0.0,
                min_y: 0.0,
                min_z: 0.0,
                max_x: 1.0,
                max_y,
                max_z: 1.0,
                slipperiness: 0.6,
                effect: None,
                bounce_restitution: None,
                suppresses_bounce: None,
                fall_damage_resetting: None,
            }),
            outline_shape: None,
            interaction_shape: None,
            fluid: None,
            fluid_tags: None,
            fluid_level: None,
            fluid_source: None,
            fluid_flow: None,
            fluid_falling: None,
            water_block: None,
            bounce_restitution: None,
            suppresses_bounce: None,
            fall_damage_resetting: None,
            player_fall_damage_resetting: None,
            suffocating: None,
            in_wall: None,
            bottom_face_sturdy: None,
            explosion_resistance: None,
            remove: None,
        }
    }

    /// A floor along x with a slab on it at x 6.
    fn floor() -> NativeWorld {
        let mut world = NativeWorld::new();
        let mut updates: Vec<NativeBlockUpdate> = (-2..12).map(|x| block(x, 0, 1.0)).collect();
        updates.push(block(6, 1, 0.5));
        world.apply_block_updates(updates).expect("blocks go in");
        world
    }

    fn walker() -> NativeSimulationState {
        let mut state: NativeSimulationState = SimulationState::default().into();
        state.position = vec3(0.5, 1.0, 0.5);
        state.half_width = 0.3;
        state.height = 1.8;
        state.eye_height = Some(1.62);
        state.on_ground = true;
        state.is_player = Some(true);
        state.controlled_by_local_instance = Some(true);
        state.input = Some(vec3(0.0, 0.0, 1.0));
        state.yaw = Some(-std::f64::consts::FRAC_PI_2);
        state.look = Some(vec3(1.0, 0.0, 0.0));
        state.sprinting = Some(true);
        state.movement_speed = Some(0.10000000149011612);
        state.flying_speed = Some(0.05);
        state.safe_fall_distance = Some(3.0);
        state
    }

    /// The input array of a state, as an adapter sends it each tick.
    fn input_of(state: &NativeSimulationState, unmoved: bool) -> Vec<f64> {
        let mut values = vec![0.0; input::LENGTH];
        let mut put =
            |at: usize, v: &NativeVec3| values[at..at + 3].copy_from_slice(&[v.x, v.y, v.z]);
        put(input::POSITION, &state.position);
        put(input::VELOCITY, &state.velocity);
        put(input::LOOK, state.look.as_ref().expect("a look"));
        let keys = state.input.as_ref().expect("keys");
        values[input::YAW] = state.yaw.unwrap_or(0.0);
        values[input::YAW_DEGREES] = state.yaw.unwrap_or(0.0).to_degrees();
        values[input::PITCH] = state.pitch.unwrap_or(0.0);
        values[input::STRAFE] = keys.x;
        values[input::FORWARD] = keys.z;
        values[input::HALF_WIDTH] = state.half_width;
        values[input::HEIGHT] = state.height;
        values[input::EYE_HEIGHT] = state.eye_height.expect("an eye height");
        let flags = [
            (
                state.jump_requested.unwrap_or(false),
                input_flag::JUMP_REQUESTED,
            ),
            (
                state.sprinting.unwrap_or(false),
                input_flag::SPRINTING | input_flag::SPRINT_MODIFIER,
            ),
            (
                state.was_sprinting.unwrap_or(false),
                input_flag::WAS_SPRINTING,
            ),
            (state.on_ground, input_flag::ON_GROUND),
            (state.collided_vertically, input_flag::COLLIDED_VERTICALLY),
            (
                state.collided_horizontally,
                input_flag::COLLIDED_HORIZONTALLY,
            ),
            (unmoved, input_flag::UNMOVED),
        ];
        values[input::FLAGS] = f64::from(
            flags
                .iter()
                .filter(|(set, _)| *set)
                .fold(0, |bits, (_, bit)| bits | bit),
        );
        values[input::FALL_DISTANCE] = state.fall_distance.unwrap_or(0.0);
        values[input::JUMP_TICKS] = f64::from(state.jump_ticks.unwrap_or(0));
        values[input::FLYING_SPEED] = state.flying_speed.unwrap_or(f64::from(0.05_f32));
        values[input::ITEM_USE_SPEED_MULTIPLIER] = state.item_use_speed_multiplier.unwrap_or(0.2);
        values
    }

    fn copy_of(state: &NativeSimulationState) -> NativeSimulationState {
        let mut copy = walker();
        copy.position = vec3(state.position.x, state.position.y, state.position.z);
        copy.velocity = vec3(state.velocity.x, state.velocity.y, state.velocity.z);
        copy.height = state.height;
        copy.on_ground = state.on_ground;
        copy.sprinting = state.sprinting;
        copy.input = state.input.as_ref().map(|keys| vec3(keys.x, keys.y, keys.z));
        copy.no_gravity = state.no_gravity;
        copy.fall_flying = state.fall_flying;
        copy.client_side = state.client_side;
        copy.ticks_frozen = state.ticks_frozen;
        copy.tick_count = state.tick_count;
        copy.freeze_damage = state.freeze_damage;
        copy.in_powder_snow = state.in_powder_snow;
        copy
    }

    fn boat_passenger() -> NativeSimulationState {
        let mut state = walker();
        state.passenger = Some(true);
        state.sprinting = Some(false);
        state.input = Some(vec3(0.0, 0.0, 0.0));
        state.no_gravity = Some(true);
        state.boat_vehicle = Some(NativeBoatVehicle {
            bounding_box: NativeAabb {
                min: vec3(-1.0, 0.0, -1.0),
                max: vec3(2.0, 3.0, 2.0),
            },
            underwater: false,
        });
        state
    }

    #[test]
    fn packed_player_retains_replaces_and_clears_boat_context() {
        for version in ["1.20.4", "26.3"] {
            let mut world = NativeWorld::new();
            // Since 26.1 the fluid interaction also requires the chunks beneath
            // its one-cell margin. A passenger near the origin touches all four.
            for chunk_x in -1..=0 {
                for chunk_z in -1..=0 {
                    world.set_chunk_loaded(chunk_x, chunk_z, true);
                }
            }
            for y in [1, 2] {
                world.set_fluid_state(0, y, 0, 1, 0, 0, true, false, vec3(0.0, 0.0, 0.0))
                    .expect("water fixture");
            }
            let mut player = NativePlayer::new(version.to_string(), boat_passenger(), config())
                .expect("boat passenger is valid");
            let mut state = boat_passenger();
            for tick in 0..7 {
                if tick >= 2 {
                    match tick {
                        2 => state.boat_vehicle.as_mut().unwrap().underwater = true,
                        3 => {
                            let boat = state.boat_vehicle.as_mut().unwrap();
                            boat.underwater = false;
                            boat.bounding_box.max.y = 0.5;
                        }
                        4 => state.boat_vehicle = None,
                        5 => {
                            state.passenger = Some(false);
                            state.boat_vehicle = boat_passenger().boat_vehicle;
                        }
                        _ => state.passenger = Some(true),
                    }
                    // The adapter replaces slow inputs only when their values change.
                    let replacement: NativeSimulationState = SimulationState::from(state).into();
                    player.set_state(replacement).expect("updated context is valid");
                    state = player.state();
                }
                let mut values = input_of(&state, tick > 0);
                if state.in_water.unwrap_or(false) {
                    values[input::FLAGS] = f64::from(values[input::FLAGS] as u32 | input_flag::IN_WATER);
                }
                let expected_boat = state.boat_vehicle.clone().map(Into::into);
                state = simulate_versioned_tick(version.to_string(), &mut world, state, config())
                    .expect("object tick runs");
                player.step(&mut world, &values, &[]).expect("packed tick runs");
                let packed = player.state();
                let expected_water = match tick {
                    0 | 1 | 6 => false,
                    3 => version == "26.3",
                    _ => true,
                };
                assert_eq!(state.in_water, Some(expected_water), "{version}, tick {tick}");
                assert_eq!(packed.in_water, state.in_water, "{version}, tick {tick}");
                assert_eq!(packed.eye_in_water, state.eye_in_water, "{version}, tick {tick}");
                assert_eq!(crate::math::Vec3::from(packed.position), crate::math::Vec3::from(state.position));
                assert_eq!(crate::math::Vec3::from(packed.velocity), crate::math::Vec3::from(state.velocity));
                assert_eq!(player.state.boat_vehicle, expected_boat);
                assert_eq!(SimulationState::from(packed).boat_vehicle, expected_boat);
            }
        }
    }

    #[test]
    fn malformed_boat_replacement_keeps_the_previous_player_and_scene() {
        let mut player = NativePlayer::new("26.3".to_string(), boat_passenger(), config())
            .expect("initial state is valid");
        player.nearby_entities.push(super::nearby_entity(&[0.0; ENTITY_LENGTH]));
        let previous_position = player.state.position;
        let previous_boat = player.state.boat_vehicle;
        for invalid in 0..3 {
            let bad_state = || {
                let mut replacement = boat_passenger();
                replacement.position.x = 123.0;
                let boat = replacement.boat_vehicle.as_mut().unwrap();
                match invalid {
                    0 => boat.bounding_box.min.x = f64::NAN,
                    1 => boat.bounding_box.max.y = f64::NEG_INFINITY,
                    _ => boat.bounding_box.min.z = boat.bounding_box.max.z + 1.0,
                }
                replacement
            };
            let error = player.set_state(bad_state()).expect_err("malformed replacement is refused");
            assert!(error.reason.contains("boatVehicle"));
            assert_eq!(player.state.position, previous_position);
            assert_eq!(player.state.boat_vehicle, previous_boat);
            assert_eq!(player.nearby_entities.len(), 1, "a failed replacement must not clear the scene");
            assert!(NativePlayer::new("26.3".to_string(), bad_state(), config()).is_err());
        }
    }

    #[test]
    fn packed_player_keeps_and_clears_explicit_movement_authority() {
        for version in ["26.2", "26.3"] {
            let mut world = floor();
            let mut player = NativePlayer::new(version.to_string(), walker(), config())
                .expect("initial player is valid");
            assert_eq!(player.state().can_simulate_movement, None);
            for authority in [Some(false), Some(true), None] {
                let mut replacement = player.state();
                replacement.can_simulate_movement = authority;
                player.set_state(replacement).expect("authority replacement is valid");
                assert_eq!(player.state.can_simulate_movement, authority);
                for _ in 0..2 {
                    let before = player.state();
                    player.step(&mut world, &input_of(&before, true), &[])
                        .expect("packed tick keeps the non-packed authority input");
                    assert_eq!(player.state.can_simulate_movement, authority);
                    assert_eq!(player.state().can_simulate_movement, authority);
                }
            }
        }
    }

    fn server_in_fire(draws: Option<Vec<i32>>) -> NativeSimulationState {
        let mut state = walker();
        state.is_player = Some(true);
        state.client_side = Some(false);
        state.remaining_fire_ticks = Some(200);
        state.server_fire_increments = draws;
        state.input = Some(vec3(0.0, 0.0, 0.0));
        state.sprinting = Some(false);
        state
    }

    fn fire_floor() -> NativeWorld {
        let mut world = floor();
        // every cell the walker's box can touch
        for x in -1..3 {
            for z in -1..2 {
                world.inner.set_block((x, 1, z), BlockState {
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                        effect: BlockEffect::Fire,
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                });
            }
        }
        world
    }

    #[test]
    fn the_fire_draws_run_alike_tick_by_tick_in_a_batch_and_in_a_kept_player() {
        let version = "26.3";
        let draws = vec![1, 2, 2, 1, 2];
        let mut world = fire_floor();
        let mut single = server_in_fire(Some(draws.clone()));
        for _ in 0..3 {
            single = simulate_versioned_tick(version.to_string(), &mut world, single, config())
                .expect("it runs");
        }
        let mut world = fire_floor();
        let batch = simulate_versioned_ticks(
            version.to_string(),
            &mut world,
            vec![server_in_fire(Some(draws.clone()))],
            3,
            config(),
        )
        .expect("it runs");
        let mut world = fire_floor();
        let mut player = NativePlayer::new(
            version.to_string(),
            server_in_fire(Some(draws.clone())),
            config(),
        )
        .expect("it runs");
        for tick in 0..3 {
            let values = input_of(&player.state(), tick > 0);
            player.step(&mut world, &values, &[]).expect("the tick runs");
        }
        // three ignitions, each after the base tick takes one off
        assert_eq!(single.remaining_fire_ticks, Some(202));
        assert_eq!(single.server_fire_increments, Some(vec![1, 2]));
        assert_eq!(batch[0].remaining_fire_ticks, single.remaining_fire_ticks);
        assert_eq!(batch[0].server_fire_increments, single.server_fire_increments);
        let kept = player.state();
        assert_eq!(kept.remaining_fire_ticks, single.remaining_fire_ticks);
        assert_eq!(kept.server_fire_increments, single.server_fire_increments);
    }

    #[test]
    fn running_out_of_fire_draws_fails_the_call_and_keeps_the_state() {
        let version = "26.3";
        let mut world = fire_floor();
        let error = simulate_versioned_ticks(
            version.to_string(),
            &mut world,
            vec![server_in_fire(Some(vec![1]))],
            2,
            config(),
        )
        .err()
        .expect("the second ignition has no draw");
        assert!(error.reason.contains("ran out"), "{}", error.reason);
        let mut player =
            NativePlayer::new(version.to_string(), server_in_fire(Some(vec![2])), config())
                .expect("it runs");
        let values = input_of(&player.state(), false);
        player.step(&mut world, &values, &[]).expect("one draw is enough");
        let after_first = player.state();
        let values = input_of(&after_first, true);
        assert!(player.step(&mut world, &values, &[]).is_err());
        let kept = player.state();
        assert_eq!(kept.remaining_fire_ticks, after_first.remaining_fire_ticks);
        assert_eq!(kept.server_fire_increments, Some(vec![]));
        assert!(!player.state.server_fire_draws_exhausted);
    }

    #[test]
    fn a_bad_fire_draw_stream_is_refused_before_it_replaces_anything() {
        let version = "26.3".to_string();
        let mut world = floor();
        let too_many = vec![1; ServerFireDraws::CAPACITY + 1];
        for draws in [vec![0], vec![3], too_many] {
            assert!(simulate_versioned_tick(
                version.clone(),
                &mut world,
                server_in_fire(Some(draws)),
                config()
            )
            .is_err());
        }
        let mut both = server_in_fire(Some(vec![1]));
        both.server_fire_increment = Some(2);
        assert!(simulate_versioned_tick(version.clone(), &mut world, both, config()).is_err());
        let mut player = NativePlayer::new(version, server_in_fire(Some(vec![2, 1])), config())
            .expect("it runs");
        assert!(player.set_state(server_in_fire(Some(vec![7]))).is_err());
        assert_eq!(player.state().server_fire_increments, Some(vec![2, 1]));
    }

    #[test]
    fn a_kept_player_and_the_object_state_agree_on_the_fire_ticks() {
        // B096: both start from the same counter, omitted included, and the kept state returns it
        for version in ["1.21.5", "1.21.6", "26.3"] {
            for is_player in [true, false] {
                for ticks in [None, Some(-5), Some(0), Some(160)] {
                    let mut world = floor();
                    let start = || {
                        let mut state = walker();
                        state.is_player = Some(is_player);
                        state.remaining_fire_ticks = ticks;
                        state
                    };
                    let mut state = start();
                    let mut player = NativePlayer::new(version.to_string(), start(), config())
                        .expect("it runs");
                    for tick in 0..3 {
                        let values = input_of(&state, tick > 0);
                        state = simulate_versioned_tick(version.to_string(), &mut world, state, config())
                            .expect("it runs");
                        player.step(&mut world, &values, &[]).expect("the tick runs");
                        assert_eq!(
                            player.state().remaining_fire_ticks,
                            state.remaining_fire_ticks,
                            "{version} player={is_player} from {ticks:?}, tick {tick}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_kept_player_ticks_as_simulate_versioned_tick_does() {
        for version in ["1.21.11", "1.16.5", "1.12.2"] {
            let mut world = floor();
            let mut player =
                NativePlayer::new(version.to_string(), walker(), config()).expect("it runs");
            let mut state = walker();
            for tick in 0..30 {
                state.jump_requested = Some(tick % 9 == 0);
                let values = input_of(&state, tick > 0);
                state = simulate_versioned_tick(version.to_string(), &mut world, state, config())
                    .expect("it runs");
                let kept = player
                    .step(&mut world, &values, &[])
                    .expect("the tick runs");
                let at = format!("{version} tick {tick}");
                assert_eq!(kept[output::POSITION], state.position.x, "{at}");
                assert_eq!(kept[output::POSITION + 1], state.position.y, "{at}");
                assert_eq!(kept[output::VELOCITY], state.velocity.x, "{at}");
                assert_eq!(kept[output::VELOCITY + 1], state.velocity.y, "{at}");
                let on_ground = kept[output::FLAGS] as u32 & output_flag::ON_GROUND != 0;
                assert_eq!(on_ground, state.on_ground, "{at}");
                let kept_box = state.bounding_box.as_ref().map_or(f64::NAN, |b| b.min.x);
                assert_eq!(
                    kept[output::BOUNDING_BOX].to_bits(),
                    kept_box.to_bits(),
                    "{at}"
                );
            }
            assert!(
                state.position.x > 6.0,
                "{version}: the walk goes over the slab"
            );
        }
    }

    fn refusal<T>(result: napi::Result<T>) -> String {
        result.err().expect("the call is refused").reason
    }

    /// One entity, at (x, 1, 0.5): its numbers in the entities array.
    fn entity_at(x: f64) -> Vec<f64> {
        let mut entity = vec![0.0; ENTITY_LENGTH];
        entity[0] = 1.0;
        for at in [2, 11] {
            entity[at..at + 3].copy_from_slice(&[x, 1.0, 0.5]);
        }
        for at in [5, 14] {
            entity[at..at + 6].copy_from_slice(&[x - 0.3, 1.0, 0.2, x + 0.3, 2.8, 0.8]);
        }
        entity
    }

    #[test]
    fn an_input_array_of_another_layout_is_refused() {
        let mut world = floor();
        let mut player = NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
        let values = input_of(&walker(), false);
        let before = player.state.position;
        for length in [0, input::LENGTH - 1, input::LENGTH + 1] {
            let mut sized = values.clone();
            sized.resize(length, 0.0);
            let reason = refusal(player.step(&mut world, &sized, &[]));
            assert!(reason.contains(&format!("holds {length} numbers")), "{reason}");
        }
        assert_eq!(player.state.position, before);
    }

    #[test]
    fn a_count_or_a_flag_a_cast_would_change_is_refused() {
        let mut world = floor();
        let mut player = NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
        let good = input_of(&walker(), false);
        for (at, value, field) in [
            (input::FLAGS, f64::NAN, "input flags"),
            (input::FLAGS, -1.0, "input flags"),
            (input::FLAGS, 1.5, "input flags"),
            (input::FLAGS, f64::from(1_u32 << 25), "does not know"),
            (input::JUMP_TICKS, f64::NAN, "jumpTicks"),
            (input::JUMP_TICKS, -1.0, "jumpTicks"),
            (input::AUTO_JUMP_TIME, 0.5, "autoJumpTime"),
            (input::RIPTIDE_LAUNCH, 256.0, "riptideLaunch"),
            (input::FIREWORK_ROCKETS, f64::INFINITY, "fireworkRockets"),
            (input::FIREWORK_ROCKETS, f64::from(MAX_ATTACHED_ROCKETS) + 1.0, "fireworkRockets"),
            (input::FIREWORK_ROCKETS_BEFORE_PLAYER, f64::NAN, "fireworkRocketsBeforePlayer"),
            (input::POSITION, 1.0e300, "position.x"),
            (input::HALF_WIDTH, f64::NAN, "halfWidth"),
            (input::HEIGHT, -1.0, "height"),
        ] {
            let mut values = good.clone();
            values[at] = value;
            let reason = refusal(player.step(&mut world, &values, &[]));
            assert!(reason.contains(field), "{reason}");
        }
        // the largest values of each still run
        let mut values = good.clone();
        values[input::RIPTIDE_LAUNCH] = 0.0;
        values[input::JUMP_TICKS] = 4_294_967_295.0;
        values[input::FIREWORK_ROCKETS] = f64::from(MAX_ATTACHED_ROCKETS);
        player.step(&mut world, &values, &[]).expect("the tick runs");
    }

    #[test]
    fn an_entities_array_of_partial_or_unknown_records_is_refused() {
        let mut world = floor();
        let mut player = NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
        let values = input_of(&walker(), false);
        let two = [entity_at(0.5), entity_at(3.0)].concat();
        let reason = refusal(player.step(&mut world, &values, &two[..ENTITY_LENGTH + 3]));
        assert!(reason.contains("not a whole number of entities"), "{reason}");
        for (at, value, field) in [
            (0, 9.0, "kind"),
            (0, -1.0, "kind"),
            (0, 0.5, "kind"),
            (0, f64::NAN, "kind"),
            (20, f64::NAN, "flags"),
            (20, 4.0, "does not know"),
        ] {
            let mut bad = two.clone();
            bad[ENTITY_LENGTH + at] = value;
            let reason = refusal(player.step(&mut world, &values, &bad));
            assert!(reason.contains(field), "{reason}");
        }
        assert!(player.nearby_entities.is_empty());
        player.step(&mut world, &values, &two).expect("whole records run");
        assert_eq!(player.nearby_entities.len(), 2);
        // the last kind, a minecart (src/mineflayer/player.ts ENTITY_KINDS), is one of them
        let mut cart = two.clone();
        cart[ENTITY_LENGTH] = 8.0;
        player.step(&mut world, &values, &cart).expect("a minecart is a kind");
    }

    #[test]
    fn a_tick_that_fails_leaves_the_player_as_the_last_one_that_ran() {
        let mut world = floor();
        let mut player = NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
        let values = input_of(&walker(), false);
        let entity = entity_at(0.5);
        player.step(&mut world, &values, &entity).expect("the tick runs");
        player.piston_deltas = [0.25, 0.0, 0.0];
        let (state, pushes, deltas) = (player.state, player.pushes.clone(), player.piston_deltas);
        assert!(!pushes.is_empty());
        // the validated input is good, and the tick's own move is what it cannot run
        let mut far = values.clone();
        far[input::VELOCITY] = 1.0e9;
        let reason = refusal(player.step(&mut world, &far, &[entity_at(3.0), entity_at(4.0)].concat()));
        assert!(reason.contains("the tick's move"), "{reason}");
        assert_eq!(player.state, state);
        assert_eq!(player.pushes, pushes);
        assert_eq!(player.piston_deltas, deltas);
        assert_eq!(player.nearby_entities.len(), 1);
        assert!(!player.move_pending);
        assert!(world.inner.entity_collisions(Aabb::new(Vec3::new(-9.0, -9.0, -9.0), Vec3::new(9.0, 9.0, 9.0)),
            &crate::simulation::entity_collision_context(&state)).is_empty());
        // the player runs on from there
        player.step(&mut world, &values, &entity).expect("the tick runs");
    }

    #[test]
    fn finish_tick_finishes_the_tick_tick_move_started_once() {
        let mut world = floor();
        let mut player = NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
        let values = input_of(&walker(), false);
        assert!(refusal(player.finish_with_height(&mut world, 1.8)).contains("no tickMove"));
        player.step_move(&mut world, &values, &[]).expect("the move runs");
        let state = player.state;
        assert!(refusal(player.finish_with_height(&mut world, f64::NAN)).contains("height"));
        assert!(refusal(player.finish_with_height(&mut world, -1.0)).contains("height"));
        assert_eq!(player.state, state);
        // a refused height leaves the tick to finish
        player.finish_with_height(&mut world, 1.8).expect("it finishes");
        assert!(refusal(player.finish_with_height(&mut world, 1.8)).contains("no tickMove"));
        // a whole tick leaves nothing to finish, and a new tickMove takes the place of one left open
        player.step(&mut world, &values, &[]).expect("the tick runs");
        assert!(refusal(player.finish_with_height(&mut world, 1.8)).contains("no tickMove"));
        player.step_move(&mut world, &values, &[]).expect("the move runs");
        player.step_move(&mut world, &values, &[]).expect("the move runs");
        player.finish_with_height(&mut world, 1.8).expect("it finishes");
    }

    #[test]
    fn a_new_state_takes_what_the_old_one_left_with_it() {
        let mut world = floor();
        let mut player = NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
        let mut values = input_of(&walker(), false);
        values[input::FIREWORK_ROCKETS] = 2.0;
        // (an entity the client ticks before the player pushes it in tickMove)
        let mut entity = entity_at(0.7);
        entity[1] = 1.0;
        player.step_move(&mut world, &values, &entity).expect("the move runs");
        player.piston_deltas = [0.25, 0.0, 0.0];
        player.auto_spin_attack_ended = true;
        assert_eq!(player.rockets_after_player, 2);
        assert!(!player.pushes.is_empty());
        // refused, it changes nothing
        let mut bad = walker();
        bad.half_width = f64::NAN;
        assert!(refusal(player.set_state(bad)).contains("NativePlayer.setState"));
        assert_eq!(player.rockets_after_player, 2);
        assert!(player.move_pending);
        player.set_state(walker()).expect("a state");
        assert_eq!(player.rockets_after_player, 0);
        assert!(player.pushes.is_empty());
        assert_eq!(player.piston_deltas, [0.0; 3]);
        assert!(!player.auto_spin_attack_ended);
        assert_eq!(player.output()[output::FLAGS] as u32 & output_flag::AUTO_SPIN_ATTACK_ENDED, 0);
        assert!(refusal(player.finish_with_height(&mut world, 1.8)).contains("no tickMove"));
    }

    #[test]
    fn a_kept_box_out_of_order_is_refused_and_a_refusal_keeps_the_last() {
        let mut player = NativePlayer::new("1.16.5".to_string(), walker(), config()).expect("it runs");
        let good = crate::napi_api::NativeAabb { min: vec3(0.0, 1.0, 0.0), max: vec3(1.0, 2.0, 1.0) };
        player.set_bounding_box(Some(good.clone())).expect("a box in order");
        let kept = player.state.bounding_box;
        assert!(refusal(player.set_bounding_box(Some(crate::napi_api::NativeAabb { min: good.max, max: good.min }))).contains("boundingBox"));
        assert!(refusal(player.set_bounding_box(Some(crate::napi_api::NativeAabb {
            min: vec3(-1.0e9, -1.0e9, -1.0e9),
            max: vec3(1.0e9, 1.0e9, 1.0e9),
        })))
        .contains("boundingBox"));
        assert_eq!(player.state.bounding_box, kept);
        player.set_bounding_box(None).expect("none");
        assert_eq!(player.state.bounding_box, None);
    }

    #[test]
    fn a_non_finite_motion_input_keeps_the_last_motion_from_1_21_11() {
        // Entity.setDeltaMovement refuses a motion that is not finite from 1.21.11 (1.21.11
        // Entity.java:3727-3732), and the player keeps the motion its last tick left; 1.21.10
        // takes it (1.21.10 Entity.java:3709-3711)
        for (version, kept) in [("1.21.11", true), ("1.21.10", false)] {
            let mut world = floor();
            let mut player =
                NativePlayer::new(version.to_string(), walker(), config()).expect("it runs");
            let state = walker();
            let first = player
                .step(&mut world, &input_of(&state, false), &[])
                .expect("the tick runs");
            let motion = [
                first[output::VELOCITY],
                first[output::VELOCITY + 1],
                first[output::VELOCITY + 2],
            ];
            let mut values = input_of(&state, false);
            values[input::POSITION..input::POSITION + 3].copy_from_slice(&first[..3]);
            values[input::VELOCITY] = f64::NAN;
            player.take_input(&values);
            let velocity = player.state.velocity;
            if kept {
                assert_eq!([velocity.x, velocity.y, velocity.z], motion, "{version}");
            } else {
                assert!(velocity.x.is_nan(), "{version}");
            }
        }
    }

    #[test]
    fn the_landing_tick_reports_its_fall_damage() {
        // LivingEntity.causeFallDamage's amount on the landing tick (1.21.11
        // LivingEntity.java:1703-1715, 1.16.5 :1609-1622), as simulateVersionedTick computes it;
        // the ticks before and after report none
        for (version, expected) in [("1.21.11", 2.0), ("1.16.5", 2.0)] {
            let mut world = floor();
            let faller = || {
                let mut faller = walker();
                faller.position = vec3(0.5, 6.0, 0.5);
                faller.on_ground = false;
                faller.input = Some(vec3(0.0, 0.0, 0.0));
                faller.sprinting = Some(false);
                faller
            };
            let mut player =
                NativePlayer::new(version.to_string(), faller(), config()).expect("it runs");
            let mut state = faller();
            let mut landed = None;
            for tick in 0..40 {
                let values = input_of(&state, tick > 0);
                state = simulate_versioned_tick(version.to_string(), &mut world, state, config())
                    .expect("it runs");
                let kept = player
                    .step(&mut world, &values, &[])
                    .expect("the tick runs");
                let at = format!("{version} tick {tick}");
                assert_eq!(kept[output::FALL_DISTANCE], state.fall_distance.unwrap(), "{at}");
                if landed.is_none() && state.on_ground {
                    assert_eq!(kept[output::FALL_DAMAGE], state.fall_damage.unwrap(), "{at}");
                    assert_eq!(kept[output::FALL_DAMAGE], expected, "{at}");
                    landed = Some(tick);
                } else {
                    assert_eq!(kept[output::FALL_DAMAGE], 0.0, "{at}");
                }
                state.fall_damage = Some(0.0);
            }
            assert!(landed.is_some(), "{version}: the faller lands");
        }
    }

    #[test]
    fn what_ticks_after_the_player_meets_the_height_finish_tick_gets() {
        // Player.tick's pose update comes after the move and before the entities after the player
        // (26.2 Player.java:266): a shulker at 2.6 pushes the standing box and misses a crouching one
        let shulker = |before: f64| {
            let mut values = vec![0.0; ENTITY_LENGTH];
            values[0] = 3.0;
            values[1] = before;
            for at in [2, 11] {
                values[at..at + 3].copy_from_slice(&[0.1, 2.6, 0.5]);
                values[at + 3..at + 9].copy_from_slice(&[-0.4, 2.6, 0.0, 0.6, 3.6, 1.0]);
            }
            values
        };
        let values = input_of(&walker(), false);
        let run = |entity: &[f64], height: Option<f64>| {
            let mut world = floor();
            let mut player =
                NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
            match height {
                Some(height) => {
                    player.step_move(&mut world, &values, entity).expect("the tick runs");
                    let move_pushes = player.pushes.len();
                    assert_eq!(move_pushes, usize::from(entity[1] != 0.0));
                    player.finish_with_height(&mut world, height).expect("the tick finishes");
                    assert_eq!(player.state.height, height);
                }
                None => {
                    player.step(&mut world, &values, entity).expect("the tick runs");
                }
            }
            (player.state.velocity.x, player.pushes.len())
        };
        let (still, none) = run(&shulker(0.0), Some(1.5));
        assert_eq!(none, 0);
        let (pushed, one) = run(&shulker(0.0), Some(1.8));
        assert_eq!(one, 1);
        assert!(pushed > still);
        // `tick` keeps the height it ran with, and what ticks before the player meets that box
        assert_eq!(run(&shulker(0.0), None), (pushed, 1));
        assert_eq!(run(&shulker(1.0), Some(1.5)).1, 1);
    }

    #[test]
    fn the_packed_output_carries_the_contact_wall_and_freeze_results_of_the_object_state() {
        // a cactus block and a fire in one column: the two hurts of a tick add up, once for the
        // tick that met them (26.3 Entity.java:1969-1975 on the server), and the next tick away
        // from them reports none, as the object state does
        for (version, client_side, expected) in [
            ("1.20.4", false, 2.0),
            ("26.3", false, 2.0),
            ("1.20.4", true, 0.0),
            ("26.3", true, 0.0),
        ] {
            let mut world = NativeWorld::new();
            let hurts = |x: i32, z: i32, effect: u8| {
                let mut update = block(x, 1, 0.0);
                update.z = z;
                let shape = update.shape.as_mut().expect("a shape");
                shape.max_x = 0.0;
                shape.max_z = 0.0;
                shape.effect = Some(effect);
                update
            };
            world
                .apply_block_updates(vec![block(0, 0, 1.0), hurts(0, 0, 12), hurts(0, 1, 16)])
                .expect("blocks go in");
            let first = || {
                let mut state = walker();
                state.position = vec3(0.5, 1.0, 0.9);
                state.client_side = Some(client_side);
                state.input = Some(vec3(0.0, 0.0, 0.0));
                state.sprinting = Some(false);
                state.no_gravity = Some(true);
                // a burning server player reads a draw from 1.21.2, which has to be given
                state.server_fire_increment = (!client_side).then_some(1);
                state
            };
            let mut player =
                NativePlayer::new(version.to_string(), first(), config()).expect("it runs");
            let mut state = first();
            let mut reports = Vec::new();
            for tick in 0..2 {
                let values = input_of(&state, tick > 0);
                state = simulate_versioned_tick(version.to_string(), &mut world, state, config())
                    .expect("it runs");
                let kept = player.step(&mut world, &values, &[]).expect("the tick runs");
                let at = format!("{version} client {client_side} tick {tick}");
                assert_eq!(kept[output::CONTACT_DAMAGE], state.contact_damage.unwrap(), "{at}");
                assert_eq!(kept[output::WALL_DAMAGE], state.wall_damage.unwrap(), "{at}");
                assert_eq!(kept[output::FREEZE_DAMAGE], state.freeze_damage.unwrap(), "{at}");
                reports.push(kept[output::CONTACT_DAMAGE]);
                // the block is gone for the second tick
                world
                    .apply_block_updates(vec![
                        {
                            let mut gone = hurts(0, 0, 12);
                            gone.remove = Some(true);
                            gone
                        },
                        {
                            let mut gone = hurts(0, 1, 16);
                            gone.remove = Some(true);
                            gone
                        },
                    ])
                    .expect("blocks go");
            }
            assert_eq!(reports, [expected, 0.0], "{version} client {client_side}");
        }
    }

    #[test]
    fn the_packed_output_carries_a_flight_into_a_wall_for_one_tick() {
        // LivingEntity.travelFallFlying, the server's alone (26.3 LivingEntity.java:2505-2510)
        for client_side in [false, true] {
            let mut world = NativeWorld::new();
            world
                .apply_block_updates(vec![block(2, 10, 1.0)])
                .expect("blocks go in");
            let mut state = walker();
            state.position = vec3(1.6, 10.0, 0.5);
            state.velocity = vec3(1.0, 0.0, 0.0);
            state.height = 0.6;
            state.on_ground = false;
            state.sprinting = Some(false);
            state.input = Some(vec3(0.0, 0.0, 0.0));
            state.no_gravity = Some(true);
            state.fall_flying = Some(true);
            state.client_side = Some(client_side);
            let mut player =
                NativePlayer::new("26.3".to_string(), copy_of(&state), config()).expect("it runs");
            let mut values = input_of(&state, false);
            values[input::FLAGS] = f64::from(values[input::FLAGS] as u32 | input_flag::FALL_FLYING);
            let hit = player.step(&mut world, &values, &[]).expect("the tick runs");
            assert_eq!(hit[output::WALL_DAMAGE], if client_side { 0.0 } else { 7.0 });
            state.velocity = vec3(0.0, 0.0, 0.0);
            state.position = vec3(0.5, 10.0, 0.5);
            let values = input_of(&state, false);
            let next = player.step(&mut world, &values, &[]).expect("the tick runs");
            assert_eq!(next[output::WALL_DAMAGE], 0.0);
        }
    }

    #[test]
    fn a_tick_makes_the_clients_frost_from_the_attribute_it_keeps() {
        // LivingEntity.aiStep's removePowderSnowSlow and addPowderSnowSlowIfNeeded, on the client
        // up to 1.21.4 (1.17 LivingEntity.java:2555-2556, 1.21.4 :2785-2786) and not from 1.21.5
        // (1.21.5 :2763-2772): as simulateVersionedTick does with the same attribute
        for (version, frost) in [
            ("1.16.5", None),
            ("1.17", Some(f64::from(-0.05_f32 * 0.5_f32))),
            ("1.21.4", Some(f64::from(-0.05_f32 * 0.5_f32))),
            ("1.21.5", None),
            ("26.3", None),
        ] {
            let mut state = walker();
            state.client_side = Some(true);
            state.ticks_frozen = Some(70);
            state.movement_speed_attribute = Some(NativeMovementSpeedAttribute {
                base: 0.10000000149011612,
                modifiers: Vec::new(),
                frost: None,
            });
            let object = copy_of_with_attribute(&state);
            let mut world = floor();
            let mut player =
                NativePlayer::new(version.to_string(), state, config()).expect("a player");
            let values = input_of(&walker(), true);
            let out = player.step(&mut world, &values, &[]).expect("a tick");
            let kept = player.state();
            assert_eq!(kept.movement_speed_attribute.as_ref().expect("kept").frost, frost, "{version}");
            match frost {
                Some(amount) => {
                    assert_eq!(out[output::FROST], amount, "{version}");
                    assert_eq!(
                        kept.movement_speed_value,
                        Some(0.10000000149011612 + amount),
                        "{version}"
                    );
                }
                None => assert!(out[output::FROST].is_nan(), "{version}"),
            }

            let mut world = floor();
            let next = simulate_versioned_tick_outcome(version.to_string(), &mut world, object, config())
                .expect("a tick");
            assert_eq!(next.state.movement_speed_attribute.expect("kept").frost, frost, "{version}");
        }
    }

    fn copy_of_with_attribute(state: &NativeSimulationState) -> NativeSimulationState {
        let mut copy = copy_of(state);
        copy.movement_speed_attribute = state.movement_speed_attribute.clone();
        copy
    }

    #[test]
    fn a_state_without_the_attribute_makes_no_frost_and_a_refused_one_changes_nothing() {
        let mut state = walker();
        state.ticks_frozen = Some(70);
        let mut world = floor();
        let mut player = NativePlayer::new("1.20.4".to_string(), state, config()).expect("a player");
        let out = player.step(&mut world, &input_of(&walker(), true), &[]).expect("a tick");
        assert!(out[output::FROST].is_nan());
        assert!(player.state().movement_speed_attribute.is_none());

        let mut bad = walker();
        bad.movement_speed_attribute = Some(NativeMovementSpeedAttribute {
            base: 0.1,
            modifiers: vec![NativeAttributeModifier { amount: 1.0, operation: "no_such".to_string() }],
            frost: None,
        });
        assert!(player.set_state(bad).is_err());
        assert!(player.state().movement_speed_attribute.is_none());
        assert_eq!(player.state().ticks_frozen, Some(68), "the state stays");
    }

    #[test]
    fn the_packed_output_keeps_the_running_freeze_count() {
        // LivingEntity.aiStep's every 40 ticks (26.3 LivingEntity.java:3082): a running count, of
        // the server's side
        let mut world = NativeWorld::new();
        let mut state = walker();
        state.client_side = Some(false);
        state.ticks_frozen = Some(140);
        state.tick_count = Some(38);
        state.freeze_damage = Some(7.0);
        state.in_powder_snow = Some(true);
        state.sprinting = Some(false);
        state.input = Some(vec3(0.0, 0.0, 0.0));
        state.no_gravity = Some(true);
        let mut powder = block(0, 1, 0.0);
        let shape = powder.shape.as_mut().expect("a shape");
        shape.max_x = 0.0;
        shape.max_z = 0.0;
        shape.effect = Some(8);
        world.apply_block_updates(vec![powder]).expect("blocks go in");
        state.on_ground = false;
        let mut player =
            NativePlayer::new("1.20.4".to_string(), copy_of(&state), config()).expect("it runs");
        let mut counts = Vec::new();
        for tick in 0..3 {
            let mut values = input_of(&state, tick > 0);
            values[input::FLAGS] = f64::from(values[input::FLAGS] as u32 | input_flag::IN_POWDER_SNOW);
            let kept = player.step(&mut world, &values, &[]).expect("the tick runs");
            counts.push(kept[output::FREEZE_DAMAGE]);
        }
        assert_eq!(counts, [7.0, 8.0, 8.0]);
    }

    #[test]
    fn the_packed_output_tells_the_block_fall_callback_of_a_landing_tick_alone() {
        // Entity.move runs Block.updateEntityMovementAfterFallOn on a vertical collision (1.20.4
        // Entity.java:668; from 26.2 only for a move that went vertically, 26.2 Entity.java:793, 815)
        for version in ["1.20.4", "26.3"] {
            let mut world = floor();
            let mut state = walker();
            state.position = vec3(0.5, 1.5, 0.5);
            state.velocity = vec3(0.0, -1.0, 0.0);
            state.on_ground = false;
            state.sprinting = Some(false);
            state.input = Some(vec3(0.0, 0.0, 0.0));
            let mut player =
                NativePlayer::new(version.to_string(), copy_of(&state), config()).expect("it runs");
            let values = input_of(&state, false);
            let flags = |values: &[f64]| values[output::FLAGS] as u32;
            let landed = player.step(&mut world, &values, &[]).expect("the tick runs");
            assert_ne!(flags(&landed) & output_flag::BLOCK_FALL_CALLBACK, 0, "{version}");
            state.position = vec3(0.5, 1.0, 0.5);
            state.velocity = vec3(0.0, 0.0, 0.0);
            state.on_ground = true;
            state.no_gravity = Some(true);
            let values = input_of(&state, false);
            let next = player.step(&mut world, &values, &[]).expect("the tick runs");
            assert_eq!(flags(&next) & output_flag::BLOCK_FALL_CALLBACK, 0, "{version}");
        }
    }

    #[test]
    fn nearby_entities_come_in_their_own_array() {
        let mut world = floor();
        let mut player =
            NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("1.21.11 runs");
        let mut values = input_of(&walker(), false);
        values[input::POSITION + 1] = 1.2;
        values[input::VELOCITY + 1] = -0.5;
        values[input::FLAGS] = 0.0;
        let mut shulker = vec![0.0; ENTITY_LENGTH];
        shulker[0] = 3.0;
        shulker[2..5].copy_from_slice(&[0.5, 0.0, 0.5]);
        shulker[5..11].copy_from_slice(&[0.0, 0.0, 0.0, 1.0, 1.1, 1.0]);
        shulker[11..14].copy_from_slice(&[0.5, 0.0, 0.5]);
        shulker[14..20].copy_from_slice(&[0.0, 0.0, 0.0, 1.0, 1.1, 1.0]);
        let landed = player
            .step(&mut world, &values, &shulker)
            .expect("the tick runs");
        assert_eq!(landed[output::POSITION + 1], 1.1);
    }

    #[test]
    fn the_nearby_entities_take_back_the_pushes_they_give() {
        let mut world = floor();
        let mut player =
            NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("1.21.11 runs");
        let mut values = input_of(&walker(), false);
        values[input::FORWARD] = 0.0;
        values[input::FLAGS] = f64::from(input_flag::ON_GROUND);
        let mut cow = vec![0.0; ENTITY_LENGTH];
        cow[0] = 1.0;
        for at in [2, 11] {
            cow[at..at + 3].copy_from_slice(&[1.0, 1.0, 0.5]);
            cow[at + 3..at + 9].copy_from_slice(&[0.55, 1.0, 0.05, 1.45, 2.4, 0.95]);
        }
        let inert = vec![0.0; ENTITY_LENGTH];
        let entities = [inert, cow].concat();
        let pushed = player
            .step(&mut world, &values, &entities)
            .expect("the tick runs");
        let push = 0.5 / 0.5_f64.sqrt() * f64::from(0.05_f32);
        assert_eq!(pushed[output::VELOCITY], -push);
        assert_eq!(
            player.pushes,
            vec![EntityPush {
                index: 1,
                x: push,
                z: 0.0
            }]
        );
    }

    #[test]
    fn the_team_rules_the_sleep_and_the_death_keep_a_mob_from_pushing() {
        // EntitySelector.pushableBy (1.21.11 EntitySelector.java:40-72), LivingEntity.push(Entity)
        // skipping a sleeper (:2188-2193), LivingEntity.isPushable's isAlive (:3202-3204)
        let mut world = floor();
        let mut cow = vec![0.0; ENTITY_LENGTH];
        cow[0] = 1.0;
        for at in [2, 11] {
            cow[at..at + 3].copy_from_slice(&[1.0, 1.0, 0.5]);
            cow[at + 3..at + 9].copy_from_slice(&[0.55, 1.0, 0.05, 1.45, 2.4, 0.95]);
        }
        let mut push = |flags: u32, entity_flags: u32| {
            let mut player =
                NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
            let mut values = input_of(&walker(), false);
            values[input::FORWARD] = 0.0;
            values[input::FLAGS] = f64::from(input_flag::ON_GROUND | flags);
            let mut entity = cow.clone();
            entity[20] = f64::from(entity_flags);
            player
                .step(&mut world, &values, &entity)
                .expect("the tick runs");
            player.pushes.len()
        };
        assert_eq!(push(0, 0), 1);
        assert_eq!(push(0, entity_flag::NO_TEAM_PUSH), 0);
        assert_eq!(push(input_flag::SLEEPING, 0), 0);
        assert_eq!(push(input_flag::DEAD, 0), 0);
    }

    #[test]
    fn the_input_takes_the_item_use_speed_and_the_removal() {
        let mut world = floor();
        let mut walk = |flags: u32, item_use_speed: f64| {
            let mut player =
                NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
            let mut values = input_of(&walker(), false);
            values[input::FLAGS] = f64::from(values[input::FLAGS] as u32 | flags);
            values[input::ITEM_USE_SPEED_MULTIPLIER] = item_use_speed;
            player
                .step(&mut world, &values, &[])
                .expect("the tick runs")
        };
        let walking = walk(0, 0.2);
        let charging = walk(input_flag::USING_ITEM, 1.0);
        let eating = walk(input_flag::USING_ITEM, 0.2);
        assert_eq!(charging[output::VELOCITY], walking[output::VELOCITY]);
        assert!(eating[output::VELOCITY] < walking[output::VELOCITY]);

        let removed = walk(input_flag::REMOVED, 0.2);
        assert_eq!(removed[output::POSITION], walker().position.x);
    }

    #[test]
    fn a_riptide_spin_bounces_off_the_first_living_entity_it_meets() {
        // LivingEntity.checkAutoSpinAttack after travel, before the pushes: a living entity in the
        // box swept by the move scales the velocity by -0.2 and ends the spin; any other entity
        // leaves both (1.13.2 LivingEntity.java:2018-2035, 1.21.11 :3090-3104)
        let mut world = floor();
        let mut spin = |kind: f64, spinning: bool| {
            let mut player =
                NativePlayer::new("1.21.11".to_string(), walker(), config()).expect("it runs");
            let mut values = input_of(&walker(), false);
            values[input::POSITION + 1] = 3.0;
            values[input::VELOCITY] = 0.5;
            values[input::FORWARD] = 0.0;
            values[input::FLAGS] = if spinning {
                f64::from(input_flag::AUTO_SPIN_ATTACK)
            } else {
                0.0
            };
            // met where the move starts; its own tick leaves it far away, where it pushes no one
            let mut entity = vec![0.0; ENTITY_LENGTH];
            entity[0] = kind;
            entity[2..5].copy_from_slice(&[0.0, 3.0, 0.5]);
            entity[5..11].copy_from_slice(&[-0.3, 3.0, 0.2, 0.3, 4.95, 0.8]);
            entity[11..14].copy_from_slice(&[20.0, 3.0, 0.5]);
            entity[14..20].copy_from_slice(&[19.7, 3.0, 0.2, 20.3, 4.95, 0.8]);
            player
                .step(&mut world, &values, &entity)
                .expect("the tick runs")
        };
        let ended =
            |out: &[f64]| out[output::FLAGS] as u32 & output_flag::AUTO_SPIN_ATTACK_ENDED != 0;
        let plain = spin(0.0, true);
        assert!(!ended(&plain));
        let hit = spin(1.0, true);
        assert!(ended(&hit));
        assert_eq!(hit[output::VELOCITY], plain[output::VELOCITY] * -0.2);
        assert_eq!(
            hit[output::VELOCITY + 1],
            plain[output::VELOCITY + 1] * -0.2
        );
        assert_eq!(hit[output::POSITION], plain[output::POSITION]);
        let not_spinning = spin(1.0, false);
        assert!(!ended(&not_spinning));
        assert_eq!(not_spinning[output::VELOCITY], plain[output::VELOCITY]);
        // an armor stand is a LivingEntity though it neither pushes nor collides: checkAutoSpinAttack
        // tests `instanceof LivingEntity` (1.13.2 LivingEntity.java:2022, 1.16.5 :2580, 1.21.11
        // :3094; 1.13.2 entity/decoration/ArmorStand.java:40 extends LivingEntity)
        let armor_stand = spin(5.0, true);
        assert!(ended(&armor_stand));
        assert_eq!(armor_stand[output::VELOCITY], plain[output::VELOCITY] * -0.2);
    }

    #[test]
    fn a_riptide_spin_bounces_off_a_living_entity_on_each_side_of_its_changes() {
        // checkAutoSpinAttack scales the velocity by -0.2 component by component in 1.13.2
        // (LivingEntity.java:2018-2035), by Vec3.scale(-0.2) from 1.14.4, and with a
        // pattern-matched instanceof from 26.2: the same bounce on each side (B280 reach)
        let mut world = floor();
        for version in ["1.13.2", "1.14.4", "26.1.2", "26.2", "26.3"] {
            let mut spin = |kind: f64| {
                let mut player =
                    NativePlayer::new(version.to_string(), walker(), config()).expect("it runs");
                let mut values = input_of(&walker(), false);
                values[input::POSITION + 1] = 3.0;
                values[input::VELOCITY] = 0.5;
                values[input::FORWARD] = 0.0;
                values[input::FLAGS] = f64::from(input_flag::AUTO_SPIN_ATTACK);
                let mut entity = vec![0.0; ENTITY_LENGTH];
                entity[0] = kind;
                entity[2..5].copy_from_slice(&[0.0, 3.0, 0.5]);
                entity[5..11].copy_from_slice(&[-0.3, 3.0, 0.2, 0.3, 4.95, 0.8]);
                entity[11..14].copy_from_slice(&[20.0, 3.0, 0.5]);
                entity[14..20].copy_from_slice(&[19.7, 3.0, 0.2, 20.3, 4.95, 0.8]);
                player.step(&mut world, &values, &entity).expect("the tick runs")
            };
            let plain = spin(0.0);
            let hit = spin(1.0);
            assert!(hit[output::FLAGS] as u32 & output_flag::AUTO_SPIN_ATTACK_ENDED != 0, "{version}");
            assert_eq!(hit[output::VELOCITY], plain[output::VELOCITY] * -0.2, "{version}");
            assert_eq!(hit[output::VELOCITY + 1], plain[output::VELOCITY + 1] * -0.2, "{version}");
        }
    }

    #[test]
    fn a_riptide_spin_sweeps_the_box_of_its_new_pose_before_1_17() {
        // Entity.setPose -> refreshDimensions refits the kept box at once, so the spin's box swept
        // on the tick after STANDING -> SPIN_ATTACK is already 0.6 tall (1.16.5
        // LivingEntity.java:2504 `Box lv2 = this.getBoundingBox()`, Entity.java refreshDimensions)
        let mut world = floor();
        let mut spin = |height: f64| {
            let mut state = walker();
            state.position = vec3(0.5, 3.0, 0.5);
            state.on_ground = false;
            let width = f64::from(0.6_f32);
            let mut player =
                NativePlayer::new("1.16.5".to_string(), state, config()).expect("it runs");
            // the box kept from the last tick, standing
            player.set_bounding_box(Some(crate::napi_api::NativeAabb {
                min: vec3(0.2, 3.0, 0.2),
                max: vec3(0.2 + width, 3.0 + f64::from(1.8_f32), 0.2 + width),
            })).expect("a box in order");
            let mut values = input_of(&walker(), true);
            values[input::POSITION..input::POSITION + 3].copy_from_slice(&[0.5, 3.0, 0.5]);
            values[input::VELOCITY] = 0.5;
            values[input::FORWARD] = 0.0;
            values[input::HEIGHT] = height;
            values[input::FLAGS] = f64::from(input_flag::AUTO_SPIN_ATTACK | input_flag::UNMOVED);
            // a living entity from 0.7 to 1.7 above the player's feet
            let mut entity = vec![0.0; ENTITY_LENGTH];
            entity[0] = 1.0;
            for at in [2, 11] {
                entity[at..at + 3].copy_from_slice(&[0.5, 3.7, 0.5]);
            }
            entity[5..11].copy_from_slice(&[0.2, 3.7, 0.2, 0.8, 4.7, 0.8]);
            entity[14..20].copy_from_slice(&[20.0, 3.7, 0.2, 20.6, 4.7, 0.8]);
            let out = player
                .step(&mut world, &values, &entity)
                .expect("the tick runs");
            out[output::FLAGS] as u32 & output_flag::AUTO_SPIN_ATTACK_ENDED != 0
        };
        assert!(!spin(f64::from(0.6_f32)));
        assert!(spin(f64::from(1.8_f32)));
    }

    #[test]
    fn object_batch_and_packed_passengers_stay_on_the_seat_and_walk_off_it_alike() {
        // Entity.rideTick, on every entry point: a passenger with keys down stays on the seat it was
        // handed; cleared, the same state walks (26.3 Entity.java:2465-2470)
        for version in ["1.8.8", "1.20.4", "1.21.3", "26.3"] {
            let mut world = floor();
            let start = || {
                let mut state = walker();
                state.position = vec3(3.5, 3.0, 0.5);
                state.velocity = vec3(0.4, 0.2, 0.0);
                state.on_ground = false;
                state.passenger = Some(true);
                state.jump_requested = Some(true);
                state.sprinting = Some(false);
                state
            };
            let batch = crate::napi_api::simulate_versioned_ticks(version.to_string(), &mut world, vec![start()], 3, config())
                .expect("the batch runs")
                .pop()
                .expect("one state");
            let mut object = start();
            let mut player = NativePlayer::new(version.to_string(), start(), config()).expect("initial player is valid");
            for tick in 0..4 {
                let dismounted = tick == 3;
                if dismounted {
                    object.passenger = Some(false);
                    let mut replacement = player.state();
                    replacement.passenger = Some(false);
                    player.set_state(replacement).expect("dismount replacement is valid");
                }
                let values = input_of(&object, tick > 0);
                object = simulate_versioned_tick(version.to_string(), &mut world, object, config())
                    .expect("object tick runs");
                player.step(&mut world, &values, &[]).expect("packed tick runs");
                let packed = player.state();
                assert_eq!(Vec3::from(packed.position), Vec3::from(object.position), "{version}, tick {tick}");
                assert_eq!(Vec3::from(packed.velocity), Vec3::from(object.velocity), "{version}, tick {tick}");
                assert_eq!(packed.passenger, object.passenger, "{version}, tick {tick}");
                if dismounted {
                    assert_ne!(Vec3::from(object.position), Vec3::new(3.5, 3.0, 0.5), "{version}");
                } else {
                    assert_eq!(Vec3::from(object.position), Vec3::new(3.5, 3.0, 0.5), "{version}, tick {tick}");
                    assert_eq!(object.fall_distance, Some(0.0), "{version}, tick {tick}");
                }
                if tick == 2 {
                    assert_eq!(Vec3::from(batch.position), Vec3::from(object.position), "{version}");
                    assert_eq!(Vec3::from(batch.velocity), Vec3::from(object.velocity), "{version}");
                }
            }
        }
    }

    #[test]
    fn the_raw_firework_boost_applies_once_a_tick_and_stays_on_the_state() {
        // the raw input is one rocket tick's impulse before each player tick and is the caller's to
        // clear: a batch, the object API and the packed player spend it the same, one each tick
        let boost = vec3(0.0, 0.05, 0.02);
        let glider = |firework_boost: Option<NativeVec3>| {
            let mut state = walker();
            state.position = vec3(0.5, 40.0, 0.5);
            state.velocity = vec3(0.0, -0.1, 0.4);
            state.on_ground = false;
            state.sprinting = Some(false);
            state.input = Some(vec3(0.0, 0.0, 0.0));
            state.yaw = Some(0.0);
            state.look = Some(vec3(0.0, 0.0, 1.0));
            state.fall_flying = Some(true);
            state.firework_boost = firework_boost;
            state
        };
        for version in ["1.13.2", "1.20.4", "26.3"] {
            let mut world = NativeWorld::new();
            let ticks = 3;
            let batch = crate::napi_api::simulate_versioned_ticks(
                version.to_string(),
                &mut world,
                vec![glider(Some(boost))],
                ticks,
                config(),
            )
            .expect("the batch runs")
            .pop()
            .expect("one state");
            // the same ticks with the impulse added by hand before each
            let mut by_hand = glider(None);
            for _ in 0..ticks {
                by_hand.velocity = vec3(
                    by_hand.velocity.x + boost.x,
                    by_hand.velocity.y + boost.y,
                    by_hand.velocity.z + boost.z,
                );
                by_hand = simulate_versioned_tick(version.to_string(), &mut world, by_hand, config())
                    .expect("object tick runs");
            }
            let mut player = NativePlayer::new(version.to_string(), glider(Some(boost)), config())
                .expect("initial player is valid");
            for tick in 0..ticks {
                let state = player.state();
                let mut values = input_of(&state, tick > 0);
                values[input::FLAGS] = f64::from(values[input::FLAGS] as u32 | input_flag::FALL_FLYING);
                player.step(&mut world, &values, &[]).expect("packed tick runs");
            }
            let packed = player.state();
            for (name, state) in [("batch", &batch), ("packed", &packed)] {
                assert_eq!(Vec3::from(state.velocity), Vec3::from(by_hand.velocity), "{version} {name}");
                assert_eq!(Vec3::from(state.position), Vec3::from(by_hand.position), "{version} {name}");
                assert_eq!(state.firework_boost.map(Vec3::from), Some(Vec3::from(boost)), "{version} {name}");
            }
        }
    }

    #[test]
    fn object_and_packed_players_agree_through_a_sleep_and_a_death() {
        // an immobile tick clears jumping, xxa and zza and resets noJumpDelay, and keeps the keys
        // (26.3 LivingEntity.java:3000-3003, 3035): the object API, its state passed back, and the
        // packed player sent the keys each tick are one trace, the first tick awake included
        for version in ["1.8.8", "1.20.4", "26.3"] {
            let mut world = floor();
            let start = || {
                let mut state = walker();
                state.sprinting = Some(false);
                state.input = Some(vec3(1.0, 0.0, 1.0));
                state.jump_requested = Some(true);
                state.jump_ticks = Some(3);
                state.velocity = vec3(0.1, 0.0, 0.0);
                state
            };
            let mut state = start();
            let mut player = NativePlayer::new(version.to_string(), start(), config())
                .expect("initial player is valid");
            for tick in 0..8 {
                state.sleeping = Some((1..3).contains(&tick));
                state.dead = Some((4..6).contains(&tick));
                let mut values = input_of(&state, tick > 0);
                let immobile = state.sleeping == Some(true) || state.dead == Some(true);
                if immobile {
                    let mut flags = values[input::FLAGS] as u32;
                    if state.sleeping == Some(true) {
                        flags |= input_flag::SLEEPING;
                    } else {
                        flags |= input_flag::DEAD;
                    }
                    values[input::FLAGS] = f64::from(flags);
                }
                state = simulate_versioned_tick(version.to_string(), &mut world, state, config())
                    .expect("object tick runs");
                player.step(&mut world, &values, &[]).expect("packed tick runs");
                let packed = player.state();
                assert_eq!(Vec3::from(packed.position), Vec3::from(state.position), "{version}, tick {tick}");
                assert_eq!(Vec3::from(packed.velocity), Vec3::from(state.velocity), "{version}, tick {tick}");
                assert_eq!(packed.jump_ticks, state.jump_ticks, "{version}, tick {tick}");
                assert_eq!(packed.on_ground, state.on_ground, "{version}, tick {tick}");
                // what the caller asked for comes back in both
                assert_eq!(state.jump_requested, Some(true), "{version}, tick {tick}");
                assert_eq!(state.input.map(Vec3::from), Some(Vec3::new(1.0, 0.0, 1.0)));
                assert_eq!(packed.input.map(Vec3::from), Some(Vec3::new(1.0, 0.0, 1.0)));
                if immobile {
                    assert_eq!(state.jump_ticks, Some(0), "{version}, tick {tick}");
                }
            }
        }
    }
}
