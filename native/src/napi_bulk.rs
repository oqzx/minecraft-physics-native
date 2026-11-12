//! Bulk ticks: every `simulateVersioned*Ticks` family as a batch that owns its states between
//! calls, and `NativeBulkRun`, which lets a caller run one a few ticks at a time. A batch keeps what
//! a returned state cannot carry (a player's scene, a projectile's fluid and base-tick state), so a
//! run in chunks is the same loop as a run in one call, tick for tick, and a stopped run goes on
//! from where it stopped. There is no thread and no global: dropping a run frees all of it.

use napi::{Error, Result};
use napi_derive::napi;

use crate::entity::entity_push_impulses_with_pushthrough;
use crate::math::Vec3;
use crate::napi_api::{
    arrow_state, boat_state, check_fire_draws, check_move, check_projectile_family,
    native_arrow_tick_result, native_boat_state, native_projectile_tick_result,
    native_ridden_mob_state, native_trident_tick_result, projectile_state, ridden_mob_state,
    state_with_scene, trident_state, validate_arrow_flight, validate_projectile_flight,
    validate_tick_state, validate_trident_flight, validated_simulation_config, vehicle_colliders,
    vehicle_release, FieldCheck, FieldRule, LocalPlayerScene, NativeArrowState, NativeArrowTickResult, NativeBoatState,
    NativeHorseState, NativeNearbyEntity, NativeProjectileState, NativeProjectileTickResult,
    NativeSimulationConfig, NativeSimulationState, NativeTridentState, NativeTridentTickResult,
    NativeVersionedEntityState, NativeWorld,
};
use crate::projectile::{
    tick_arrow_in_world, tick_projectile_in_world, tick_trident_in_world, ArrowState,
    ProjectileKind, ProjectileState, TridentState,
};
use crate::ridden::{tick_ridden_mob, RiddenMobState};
use crate::simulation::{PhysicsConfig, SimulationState};
use crate::vehicle::{tick_boat_in_world, BoatSimulationState};
use crate::version::MinecraftVersion;
use crate::versioned;
use crate::world::{EntityCollider, WorldSnapshot};

/// The entity ticks a chunk of a run may take: one tick is always allowed, however many entities
/// it has, and past that a chunk stops here. It is what keeps a cancellation's wait bounded.
pub const MAX_BULK_CHUNK_WORK: u32 = 1 << 14;

fn supported_release(version: &str) -> Result<MinecraftVersion> {
    if !versioned::is_supported(version) {
        return Err(Error::from_reason(format!(
            "unsupported decompiled Minecraft version: {version}"
        )));
    }
    MinecraftVersion::parse(version)
        .ok_or_else(|| Error::from_reason(format!("invalid Minecraft version: {version}")))
}

/// A player's batch: each state with its scene, and what the scene takes from the state to give back.
pub(crate) struct PlayerBatch {
    version: String,
    release: MinecraftVersion,
    config: PhysicsConfig,
    states: Vec<PlayerEntry>,
}

struct PlayerEntry {
    state: SimulationState,
    scene: LocalPlayerScene,
    nearby_entities: Option<Vec<NativeNearbyEntity>>,
    had_pistons: bool,
}

impl PlayerBatch {
    pub(crate) fn new(
        version: String,
        states: Vec<NativeSimulationState>,
        config: NativeSimulationConfig,
    ) -> Result<Self> {
        if !versioned::is_supported(&version) {
            return Err(Error::from_reason(format!(
                "unsupported decompiled Minecraft version: {version}"
            )));
        }
        let states = states
            .into_iter()
            .map(|mut state| {
                let nearby_entities = state.nearby_entities.clone();
                let had_pistons = state.moving_pistons.is_some();
                let scene = LocalPlayerScene::take(&mut state)?;
                Ok(PlayerEntry {
                    state: state.into(),
                    scene,
                    nearby_entities,
                    had_pistons,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let release = supported_release(&version)?;
        let config = validated_simulation_config(config)?;
        Ok(Self { version, release, config, states })
    }

    pub(crate) fn advance(&mut self, world: &mut NativeWorld, ticks: u32) -> Result<()> {
        for _ in 0..ticks {
            for entry in &mut self.states {
                entry.scene.tick(
                    &self.version,
                    self.release,
                    world,
                    &mut entry.state,
                    self.config,
                )?;
            }
        }
        Ok(())
    }

    /// The states alone, as simulateVersionedTick's result drops the scene.
    pub(crate) fn into_states(self) -> Vec<NativeSimulationState> {
        self.states.into_iter().map(|entry| entry.state.into()).collect()
    }

    /// The states with the scene each has reached, each fit to start a new batch.
    fn states_with_scene(&self) -> Vec<NativeSimulationState> {
        self.states
            .iter()
            .map(|entry| {
                state_with_scene(
                    entry.state.into(),
                    &entry.scene,
                    entry.nearby_entities.clone(),
                    entry.had_pistons,
                )
            })
            .collect()
    }
}

/// The rest of an entity's state, beside its simulation.
#[derive(Clone, Copy)]
struct EntityRest {
    id: Option<u32>,
    no_physics: bool,
    vehicle_id: Option<u32>,
    pushable: bool,
    pushthrough: f64,
}

pub(crate) struct EntityBatch {
    version: String,
    release: MinecraftVersion,
    config: PhysicsConfig,
    simulations: Vec<SimulationState>,
    rest: Vec<EntityRest>,
    has_passengers: Vec<bool>,
    roots: Vec<Option<u32>>,
}

impl EntityBatch {
    pub(crate) fn new(
        version: String,
        states: Vec<NativeVersionedEntityState>,
        config: NativeSimulationConfig,
    ) -> Result<Self> {
        if !versioned::is_supported(&version) {
            return Err(Error::from_reason(format!(
                "unsupported decompiled Minecraft version: {version}"
            )));
        }
        for (index, state) in states.iter().enumerate() {
            FieldCheck::new("entity push state")
                .number(&format!("states[{index}].pushthrough"), state.pushthrough, FieldRule::Within(0.0, 1.0))
                .done()?;
        }
        let release = supported_release(&version)?;
        let config = validated_simulation_config(config)?;
        for state in &states {
            validate_tick_state("simulateVersionedEntityTicks", &state.simulation)?;
        }
        let ids: Vec<Option<u32>> = states.iter().map(|state| state.id).collect();
        let vehicles: Vec<Option<u32>> = states.iter().map(|state| state.vehicle_id).collect();
        let mut simulations: Vec<SimulationState> = Vec::with_capacity(states.len());
        let mut rest = Vec::with_capacity(states.len());
        for state in states {
            simulations.push(state.simulation.into());
            rest.push(EntityRest {
                id: state.id,
                no_physics: state.no_physics,
                vehicle_id: state.vehicle_id,
                pushable: state.pushable,
                pushthrough: state.pushthrough,
            });
        }
        // Entity.isVehicle: some entity rides it
        let has_passengers: Vec<bool> = ids
            .iter()
            .map(|id| id.is_some() && vehicles.contains(id))
            .collect();
        // Entity.getRootVehicle: the vehicle at the bottom of what it rides (None for one that rides
        // nothing and has no id: its own root, the root of no other entity)
        let root = |index: usize| -> Option<u32> {
            let mut root = ids[index];
            let mut vehicle = vehicles[index];
            for _ in 0..ids.len() {
                let Some(id) = vehicle else { break };
                root = Some(id);
                vehicle = ids
                    .iter()
                    .position(|other| *other == Some(id))
                    .and_then(|other| vehicles[other]);
            }
            root
        };
        let roots: Vec<Option<u32>> = (0..ids.len()).map(root).collect();
        Ok(Self { version, release, config, simulations, rest, has_passengers, roots })
    }

    /// Entity.push(Entity) skips: from 1.9 entities of the same root vehicle
    /// (isPassengerOfSameVehicle, 1.9.4 Entity.java:2225, 1.21.4 :3209); in 1.8 the entity and its
    /// rider or vehicle (1.8.8 Entity.java:1025)
    fn connected(&self, left: usize, right: usize) -> bool {
        if self.release.at_least(1, 9, 0) {
            self.roots[left].is_some() && self.roots[left] == self.roots[right]
        } else {
            let (first, second) = (&self.rest[left], &self.rest[right]);
            (first.id.is_some() && second.vehicle_id == first.id)
                || (second.id.is_some() && first.vehicle_id == second.id)
        }
    }

    /// Each entity runs its whole tick in turn: its move (travel), then LivingEntity.pushEntities
    /// from where the move left it: every other pushable entity whose box meets its own (1.8 grown
    /// by 0.2F, 0, 0.2F) runs Entity.push(the ticking entity), which pushes both (1.8.8
    /// LivingEntity.java:1458-1488, 1.9.4 :1734-1774, 1.16.5 :2512-2596). The others are taken in
    /// the order given (the level's entity sections in vanilla).
    pub(crate) fn advance(&mut self, world: &WorldSnapshot, ticks: u32) -> Result<()> {
        let minecraft_version = self.release;
        for _ in 0..ticks {
            for index in 0..self.simulations.len() {
                let simulation = &mut self.simulations[index];
                simulation.minor_horizontal_collision_supported =
                    minecraft_version.supports_minor_horizontal_collision();
                let result = versioned::dispatch(&self.version, simulation, world, self.config)
                    .map_err(Error::from_reason)?;
                check_fire_draws(simulation)?;
                simulation.collided_horizontally = result.collided_horizontally;
                simulation.collided_vertically = result.collided_vertically;

                for other in 0..self.simulations.len() {
                    if other == index || !self.rest[other].pushable {
                        continue;
                    }
                    // a sleeper's own Entity.push(ticker) does nothing, to either side: Player's
                    // override from 1.9, LivingEntity's from 1.14 (1.8.9 has neither, 1.9.4
                    // Player.java:1867-1871, 1.13.2 :2001-2005, 1.14.4 LivingEntity.java:1635-1640,
                    // 26.3 :2258-2262)
                    let sleeper = self.simulations[other].sleeping
                        && (minecraft_version.at_least(1, 14, 0)
                            || (minecraft_version.at_least(1, 9, 0)
                                && self.simulations[other].is_player));
                    if sleeper {
                        continue;
                    }
                    let mut reach = self.simulations[index].bounds();
                    if !minecraft_version.at_least(1, 9, 0) {
                        reach = reach
                            .inflate(Vec3::new(f64::from(0.2_f32), 0.0, f64::from(0.2_f32)));
                    }
                    if !self.simulations[other].bounds().intersects(reach) {
                        continue;
                    }
                    let (other_impulse, impulse) = entity_push_impulses_with_pushthrough(
                        self.simulations[other].position,
                        self.simulations[index].position,
                        self.rest[other].no_physics,
                        self.rest[index].no_physics,
                        self.connected(other, index),
                        self.has_passengers[other],
                        self.has_passengers[index],
                        self.rest[other].pushable,
                        self.rest[index].pushable,
                        self.rest[other].pushthrough,
                        minecraft_version,
                    );
                    self.simulations[other].velocity =
                        self.simulations[other].velocity + other_impulse;
                    self.simulations[index].velocity = self.simulations[index].velocity + impulse;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn states(&self) -> Vec<NativeVersionedEntityState> {
        self.simulations
            .iter()
            .zip(&self.rest)
            .map(|(simulation, rest)| NativeVersionedEntityState {
                simulation: (*simulation).into(),
                no_physics: rest.no_physics,
                id: rest.id,
                vehicle_id: rest.vehicle_id,
                pushable: rest.pushable,
                pushthrough: rest.pushthrough,
            })
            .collect()
    }
}

pub(crate) struct BoatBatch {
    release: MinecraftVersion,
    states: Vec<(BoatSimulationState, Vec<EntityCollider>)>,
}

impl BoatBatch {
    pub(crate) fn new(version: &str, states: Vec<NativeBoatState>) -> Result<Self> {
        let release = vehicle_release(version, "boat")?;
        let states = states
            .into_iter()
            .map(|mut state| {
                let colliders = vehicle_colliders(state.nearby_entities.take(), true, release);
                Ok((boat_state(state)?, colliders))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { release, states })
    }

    pub(crate) fn advance(&mut self, world: &mut NativeWorld, ticks: u32) -> Result<()> {
        for _ in 0..ticks {
            for (state, colliders) in &mut self.states {
                world.inner.set_entity_colliders(colliders.clone());
                tick_boat_in_world(state, &mut world.inner, self.release);
                if let Err(error) =
                    check_move("simulateVersionedBoatTicks", state.move_out_of_range)
                {
                    world.inner.set_entity_colliders(Vec::new());
                    return Err(error);
                }
            }
        }
        world.inner.set_entity_colliders(Vec::new());
        Ok(())
    }

    pub(crate) fn states(&self) -> Vec<NativeBoatState> {
        self.states.iter().map(|(state, _)| native_boat_state(*state)).collect()
    }
}

pub(crate) struct HorseBatch {
    release: MinecraftVersion,
    states: Vec<(RiddenMobState, Vec<EntityCollider>)>,
}

impl HorseBatch {
    pub(crate) fn new(version: &str, states: Vec<NativeHorseState>) -> Result<Self> {
        let release = vehicle_release(version, "ridden mob")?;
        let states = states
            .into_iter()
            .map(|mut state| {
                let colliders = vehicle_colliders(state.nearby_entities.take(), false, release);
                Ok((ridden_mob_state(state, release)?, colliders))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { release, states })
    }

    pub(crate) fn advance(&mut self, world: &mut NativeWorld, ticks: u32) -> Result<()> {
        for _ in 0..ticks {
            for (state, colliders) in &mut self.states {
                world.inner.set_entity_colliders(colliders.clone());
                tick_ridden_mob(state, world.snapshot(), self.release);
                if let Err(error) =
                    check_move("simulateVersionedHorseTicks", state.move_out_of_range)
                {
                    world.inner.set_entity_colliders(Vec::new());
                    return Err(error);
                }
            }
        }
        world.inner.set_entity_colliders(Vec::new());
        Ok(())
    }

    pub(crate) fn states(&self) -> Vec<NativeHorseState> {
        self.states
            .iter()
            .map(|(state, _)| native_ridden_mob_state(*state, self.release))
            .collect()
    }
}

pub(crate) struct ProjectileBatch {
    release: MinecraftVersion,
    states: Vec<ProjectileState>,
    results: Vec<NativeProjectileTickResult>,
}

impl ProjectileBatch {
    pub(crate) fn new(version: &str, states: Vec<NativeProjectileState>) -> Result<Self> {
        let release = vehicle_release(version, "projectile")?;
        let states = states
            .into_iter()
            .map(|state| projectile_state(state, Some(release)))
            .collect::<Result<Vec<_>>>()?;
        for state in &states {
            check_projectile_family(state, release, version)?;
            // The client's hook before 1.11 is not this call's: its tick follows the server's interpolation (1.8.8
            // FishingHook.java tick) and comes out of a block by the client's own random, and this state has
            // neither. The server's tick has them: simulateVersionedProjectileHitTick. (1.10.2
            // FishingHook.java:196-198 interpolates toward the server's position; 1.11.2
            // FishingHook.java:139-143 runs one tick on both sides)
            if state.kind == ProjectileKind::FishingHook && !release.at_least(1, 11, 0) {
                return Err(Error::from_reason(format!(
                    "fishing hook simulation is not supported before 1.11: {version}: only the server's tick is, by simulateVersionedProjectileHitTick"
                )));
            }
        }
        Ok(Self { release, states, results: Vec::new() })
    }

    pub(crate) fn advance(&mut self, world: &WorldSnapshot, ticks: u32) -> Result<()> {
        for _ in 0..ticks {
            self.results.clear();
            for state in &mut self.states {
                validate_projectile_flight(state)?;
                let result = tick_projectile_in_world(state, world, self.release);
                self.results.push(native_projectile_tick_result(result, state));
            }
        }
        Ok(())
    }

    pub(crate) fn into_results(self) -> Vec<NativeProjectileTickResult> {
        self.results
    }
}

pub(crate) struct ArrowBatch {
    release: MinecraftVersion,
    states: Vec<ArrowState>,
    results: Vec<NativeArrowTickResult>,
}

impl ArrowBatch {
    pub(crate) fn new(version: &str, states: Vec<NativeArrowState>) -> Result<Self> {
        let release = vehicle_release(version, "arrow")?;
        let states = states
            .into_iter()
            .map(|state| arrow_state(state, Some(release)))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { release, states, results: Vec::new() })
    }

    pub(crate) fn advance(&mut self, world: &WorldSnapshot, ticks: u32) -> Result<()> {
        for _ in 0..ticks {
            self.results.clear();
            for state in &mut self.states {
                validate_arrow_flight(state)?;
                let result = tick_arrow_in_world(state, world, self.release);
                self.results.push(native_arrow_tick_result(result, state));
            }
        }
        Ok(())
    }

    pub(crate) fn into_results(self) -> Vec<NativeArrowTickResult> {
        self.results
    }
}

pub(crate) struct TridentBatch {
    release: MinecraftVersion,
    states: Vec<TridentState>,
    results: Vec<NativeTridentTickResult>,
}

impl TridentBatch {
    pub(crate) fn new(version: &str, states: Vec<NativeTridentState>) -> Result<Self> {
        let release = vehicle_release(version, "trident")?;
        // the trident is 1.13's (EntityType "trident", 1.13.2 TridentEntity)
        // (1.12.2 EntityType.java:211-293 (no trident); 1.13.2 EntityType.java:246, ThrownTrident.java:57)
        if !release.at_least(1, 13, 0) {
            return Err(Error::from_reason(format!(
                "trident simulation is not supported before 1.13: {version}"
            )));
        }
        let states = states
            .into_iter()
            .map(|state| trident_state(state, Some(release)))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { release, states, results: Vec::new() })
    }

    pub(crate) fn advance(&mut self, world: &WorldSnapshot, ticks: u32) -> Result<()> {
        for _ in 0..ticks {
            self.results.clear();
            for state in &mut self.states {
                validate_trident_flight(state)?;
                let result = tick_trident_in_world(state, world, self.release);
                self.results.push(native_trident_tick_result(result, state));
            }
        }
        Ok(())
    }

    pub(crate) fn into_results(self) -> Vec<NativeTridentTickResult> {
        self.results
    }
}

enum Family {
    Player(PlayerBatch),
    Entity(EntityBatch),
    Boat(BoatBatch),
    Horse(HorseBatch),
    Projectile(ProjectileBatch),
    Arrow(ArrowBatch),
    Trident(TridentBatch),
}

/// A bulk simulation a caller runs a chunk at a time, between which it can stop: see
/// `src/bulk.ts`. A run owns its states; every chunk ends on a whole tick of every entity. An error
/// in a chunk finalizes the run, since that tick stopped partway through its entities.
#[napi]
pub struct NativeBulkRun {
    family: Option<Family>,
    entities: u32,
    ticks_done: u32,
    /// The world of its first chunk: as NativePlayer's, a state says nothing of another.
    world: Option<u64>,
    failed: bool,
}

fn run_of(family: Family, entities: usize) -> NativeBulkRun {
    NativeBulkRun {
        family: Some(family),
        entities: entities as u32,
        ticks_done: 0,
        world: None,
        failed: false,
    }
}

#[napi]
impl NativeBulkRun {
    #[napi(factory)]
    pub fn players(
        version: String,
        states: Vec<NativeSimulationState>,
        config: NativeSimulationConfig,
    ) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Player(PlayerBatch::new(version, states, config)?), entities))
    }

    #[napi(factory)]
    pub fn entities(
        version: String,
        states: Vec<NativeVersionedEntityState>,
        config: NativeSimulationConfig,
    ) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Entity(EntityBatch::new(version, states, config)?), entities))
    }

    #[napi(factory)]
    pub fn boats(version: String, states: Vec<NativeBoatState>) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Boat(BoatBatch::new(&version, states)?), entities))
    }

    #[napi(factory)]
    pub fn horses(version: String, states: Vec<NativeHorseState>) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Horse(HorseBatch::new(&version, states)?), entities))
    }

    #[napi(factory)]
    pub fn projectiles(version: String, states: Vec<NativeProjectileState>) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Projectile(ProjectileBatch::new(&version, states)?), entities))
    }

    #[napi(factory)]
    pub fn arrows(version: String, states: Vec<NativeArrowState>) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Arrow(ArrowBatch::new(&version, states)?), entities))
    }

    #[napi(factory)]
    pub fn tridents(version: String, states: Vec<NativeTridentState>) -> Result<Self> {
        let entities = states.len();
        Ok(run_of(Family::Trident(TridentBatch::new(&version, states)?), entities))
    }

    /// The entity ticks one chunk may take, past the single tick always allowed.
    #[napi(getter)]
    pub fn max_chunk_work(&self) -> u32 {
        MAX_BULK_CHUNK_WORK
    }

    /// How many entities the run ticks.
    #[napi(getter)]
    pub fn entity_count(&self) -> u32 {
        self.entities
    }

    /// The whole ticks run so far.
    #[napi(getter)]
    pub fn ticks_done(&self) -> u32 {
        self.ticks_done
    }

    /// Whether the run holds its states: dispose and a failed chunk both end that.
    #[napi(getter)]
    pub fn alive(&self) -> bool {
        self.family.is_some()
    }

    /// Runs `ticks` more whole ticks in `world`, all of them before it returns.
    #[napi]
    pub fn advance(&mut self, world: &mut NativeWorld, ticks: u32) -> Result<()> {
        if self.family.is_none() {
            return Err(self.gone("advance"));
        }
        if ticks > 1 && u64::from(ticks) * u64::from(self.entities) > u64::from(MAX_BULK_CHUNK_WORK) {
            return Err(Error::from_reason(format!(
                "NativeBulkRun.advance: {ticks} ticks of {} entities is past the {MAX_BULK_CHUNK_WORK} entity ticks a chunk takes",
                self.entities
            )));
        }
        if self.world.is_some_and(|last| last != world.identity) {
            return Err(Error::from_reason(
                "NativeBulkRun.advance: this run ticked in another NativeWorld",
            ));
        }
        self.world = Some(world.identity);
        let advanced = match self.family.as_mut().expect("checked above") {
            Family::Player(batch) => batch.advance(world, ticks),
            Family::Entity(batch) => batch.advance(world.snapshot(), ticks),
            Family::Boat(batch) => batch.advance(world, ticks),
            Family::Horse(batch) => batch.advance(world, ticks),
            Family::Projectile(batch) => batch.advance(world.snapshot(), ticks),
            Family::Arrow(batch) => batch.advance(world.snapshot(), ticks),
            Family::Trident(batch) => batch.advance(world.snapshot(), ticks),
        };
        match advanced {
            Ok(()) => {
                self.ticks_done += ticks;
                Ok(())
            }
            Err(error) => {
                self.family = None;
                self.failed = true;
                Err(error)
            }
        }
    }

    /// Frees the states. Advancing or reading the run after this refuses.
    #[napi]
    pub fn dispose(&mut self) {
        self.family = None;
    }

    fn gone(&self, operation: &str) -> Error {
        let why = if self.failed { "a chunk of it failed" } else { "it was disposed" };
        Error::from_reason(format!("NativeBulkRun.{operation}: {why}"))
    }

    fn family_of(&self, operation: &str) -> Result<&Family> {
        self.family.as_ref().ok_or_else(|| self.gone(operation))
    }

    /// The players after the ticks so far, each with the scene it has reached.
    #[napi]
    pub fn player_states(&self) -> Result<Vec<NativeSimulationState>> {
        match self.family_of("playerStates")? {
            Family::Player(batch) => Ok(batch.states_with_scene()),
            _ => Err(Error::from_reason("NativeBulkRun.playerStates: not a run of players")),
        }
    }

    #[napi]
    pub fn entity_states(&self) -> Result<Vec<NativeVersionedEntityState>> {
        match self.family_of("entityStates")? {
            Family::Entity(batch) => Ok(batch.states()),
            _ => Err(Error::from_reason("NativeBulkRun.entityStates: not a run of entities")),
        }
    }

    #[napi]
    pub fn boat_states(&self) -> Result<Vec<NativeBoatState>> {
        match self.family_of("boatStates")? {
            Family::Boat(batch) => Ok(batch.states()),
            _ => Err(Error::from_reason("NativeBulkRun.boatStates: not a run of boats")),
        }
    }

    #[napi]
    pub fn horse_states(&self) -> Result<Vec<NativeHorseState>> {
        match self.family_of("horseStates")? {
            Family::Horse(batch) => Ok(batch.states()),
            _ => Err(Error::from_reason("NativeBulkRun.horseStates: not a run of ridden mobs")),
        }
    }

    /// The result of the last tick, as simulateVersionedProjectileTicks gives: empty before one.
    #[napi]
    pub fn projectile_results(&self) -> Result<Vec<NativeProjectileTickResult>> {
        match self.family_of("projectileResults")? {
            Family::Projectile(batch) => Ok(batch.results.clone()),
            _ => Err(Error::from_reason("NativeBulkRun.projectileResults: not a run of projectiles")),
        }
    }

    #[napi]
    pub fn arrow_results(&self) -> Result<Vec<NativeArrowTickResult>> {
        match self.family_of("arrowResults")? {
            Family::Arrow(batch) => Ok(batch.results.clone()),
            _ => Err(Error::from_reason("NativeBulkRun.arrowResults: not a run of arrows")),
        }
    }

    #[napi]
    pub fn trident_results(&self) -> Result<Vec<NativeTridentTickResult>> {
        match self.family_of("tridentResults")? {
            Family::Trident(batch) => Ok(batch.results.clone()),
            _ => Err(Error::from_reason("NativeBulkRun.tridentResults: not a run of tridents")),
        }
    }
}
#[cfg(test)]
#[path = "napi_bulk_gate_tests.rs"]
mod gate_fixtures;
