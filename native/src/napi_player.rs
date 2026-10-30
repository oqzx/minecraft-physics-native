use napi::bindgen_prelude::{Float64Array, Float64ArraySlice, Result};
use napi::Error;
use napi_derive::napi;

use crate::entity::{EntityPush, NearbyEntity, NearbyEntityKind};
use crate::math::{Aabb, Vec3};
use crate::napi_api::{
    push_local_player_by_pistons, tick_local_player, validated_simulation_config, NativeAabb,
    NativeMovingPiston, NativeSimulationConfig, NativeSimulationState, NativeWorld,
};
use crate::piston::MovingPiston;
use crate::simulation::{PhysicsConfig, SimulationState};
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
    pub const LENGTH: usize = 28;
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
    pub const LENGTH: usize = 19;
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
}

/// The numbers of one nearby entity in the entities array: its kind (0 inert, 1 living, 2 boat,
/// 3 shulker, 4 happy ghast), whether it ticks before the player, where the player's move meets it
/// (position, box) and where its own tick leaves it (position, box).
pub const ENTITY_LENGTH: usize = 20;

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
    nearby_entities: Vec<NearbyEntity>,
    pushes: Vec<EntityPush>,
    piston_deltas: [f64; 3],
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
            nearby_entities: Vec::new(),
            pushes: Vec::new(),
            piston_deltas: [0.0; 3],
        };
        player.set_state(state);
        Ok(player)
    }

    /// Replaces the whole state (its nearby entities too, when it has them).
    #[napi]
    pub fn set_state(&mut self, mut state: NativeSimulationState) {
        self.nearby_entities = state
            .nearby_entities
            .take()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        state.moving_pistons = None;
        self.state = state.into();
    }

    /// The whole state after the last tick or push.
    #[napi]
    pub fn state(&self) -> NativeSimulationState {
        self.state.into()
    }

    /// Replaces the bounding box kept before 1.17 (Entity.setBoundingBox: a refitted size).
    #[napi]
    pub fn set_bounding_box(&mut self, bounding_box: Option<NativeAabb>) {
        self.state.bounding_box = bounding_box.map(Into::into);
    }

    /// One tick: the input array's values go into the state, the tick runs, and its results come
    /// back laid out as `output`. `entities` holds the nearby entities of this tick (ENTITY_LENGTH
    /// numbers each, in the order the client ticks them).
    #[napi]
    pub fn tick(
        &mut self,
        world: &mut NativeWorld,
        input: Float64ArraySlice,
        entities: Float64ArraySlice,
    ) -> Result<Float64Array> {
        self.step(world, &input, &entities).map(Float64Array::new)
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
    fn step(
        &mut self,
        world: &mut NativeWorld,
        input: &[f64],
        entities: &[f64],
    ) -> Result<Vec<f64>> {
        if input.len() < input::LENGTH {
            return Err(Error::from_reason(
                "the NativePlayer input array is too short",
            ));
        }
        let rockets = self.take_input(input);
        self.nearby_entities = entities
            .chunks_exact(ENTITY_LENGTH)
            .map(nearby_entity)
            .collect();
        self.piston_deltas = [0.0; 3];
        self.pushes = tick_local_player(
            &self.version,
            self.release,
            world,
            &mut self.state,
            &self.nearby_entities,
            rockets,
            self.config,
        )?;
        Ok(self.output())
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
        state.velocity = vec3(values, VELOCITY);
        state.yaw = values[YAW];
        state.pitch = values[PITCH];
        state.look = vec3(values, LOOK);
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
    }
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
    let kind = match values[0] as u8 {
        1 => NearbyEntityKind::Living,
        2 => NearbyEntityKind::Boat,
        3 => NearbyEntityKind::Shulker,
        4 => NearbyEntityKind::HappyGhast,
        _ => NearbyEntityKind::Inert,
    };
    let aabb = |at: usize| Aabb::new(vec3(values, at), vec3(values, at + 3));
    NearbyEntity {
        kind,
        ticks_before_player: values[1] != 0.0,
        position: vec3(values, 2),
        bounding_box: aabb(5),
        tick_position: vec3(values, 11),
        tick_bounding_box: aabb(14),
    }
}

#[cfg(test)]
mod tests {
    use super::{input, input_flag, output, output_flag, NativePlayer, ENTITY_LENGTH};
    use crate::entity::EntityPush;
    use crate::napi_api::{
        simulate_versioned_tick, NativeBlockShape, NativeBlockUpdate, NativeSimulationConfig,
        NativeSimulationState, NativeVec3, NativeWorld,
    };
    use crate::simulation::SimulationState;

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
            bounce_restitution: None,
            suppresses_bounce: None,
            fall_damage_resetting: None,
            suffocating: None,
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
        values[input::FLYING_SPEED] = state.flying_speed.unwrap_or(0.02);
        values[input::ITEM_USE_SPEED_MULTIPLIER] = state.item_use_speed_multiplier.unwrap_or(0.2);
        values
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
}
