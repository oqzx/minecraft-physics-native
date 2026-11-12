//! The engine's opt-in phase trace: a copy of the state at the named points inside one client tick
//! of the local player, which the replay compares with vanilla's phases (the recorder's schema/phases.ts
//! gives each point's vanilla hook). Nothing here writes to the state, the world or a random
//! source: `emit` copies, and without a `traced` around the tick it only reads a flag.

use std::cell::{Cell, RefCell};

use crate::math::Vec3;
use crate::simulation::SimulationState;

/// A point in the tick, by the name vanilla's phase has in the recorder's schema/phases.ts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    TickStart,
    AfterBaseTick,
    BeforeLivingAiStep,
    BeforeTravel,
    BeforeMove,
    AfterMove,
    AfterTravel,
    AfterAiStep,
    ClientTickEnd,
}

impl Phase {
    /// In the order a tick that travels runs them (phases.ts NATIVE_PHASES).
    pub const ALL: [Phase; 9] = [
        Phase::TickStart,
        Phase::AfterBaseTick,
        Phase::BeforeLivingAiStep,
        Phase::BeforeTravel,
        Phase::BeforeMove,
        Phase::AfterMove,
        Phase::AfterTravel,
        Phase::AfterAiStep,
        Phase::ClientTickEnd,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Phase::TickStart => "tickStart",
            Phase::AfterBaseTick => "afterBaseTick",
            Phase::BeforeLivingAiStep => "beforeLivingAiStep",
            Phase::BeforeTravel => "beforeTravel",
            Phase::BeforeMove => "beforeMove",
            Phase::AfterMove => "afterMove",
            Phase::AfterTravel => "afterTravel",
            Phase::AfterAiStep => "afterAiStep",
            Phase::ClientTickEnd => "clientTickEnd",
        }
    }
}

/// One point the tick passed, and how many times it had passed that point before.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseEvent {
    pub phase: Phase,
    pub occurrence: u32,
    pub state: SimulationState,
    /// LivingEntity's xxa, yya and zza at beforeTravel: what travel takes, which the state does not hold.
    pub travel_vector: Option<Vec3>,
}

thread_local! {
    static TRACING: Cell<bool> = const { Cell::new(false) };
    static EVENTS: RefCell<Vec<PhaseEvent>> = const { RefCell::new(Vec::new()) };
}

/// Records the state at `phase` when a `traced` is running on this thread.
#[inline]
pub(crate) fn emit(phase: Phase, state: &SimulationState) {
    if TRACING.with(Cell::get) {
        record(phase, state, None);
    }
}

/// `emit` of beforeTravel with the travel vector, worked out only for a trace.
#[inline]
pub(crate) fn emit_before_travel(state: &SimulationState, travel_vector: impl FnOnce() -> Vec3) {
    if TRACING.with(Cell::get) {
        record(Phase::BeforeTravel, state, Some(travel_vector()));
    }
}

#[cold]
fn record(phase: Phase, state: &SimulationState, travel_vector: Option<Vec3>) {
    EVENTS.with(|events| {
        let mut events = events.borrow_mut();
        let occurrence = events.iter().filter(|event| event.phase == phase).count() as u32;
        events.push(PhaseEvent { phase, occurrence, state: *state, travel_vector });
    });
}

/// The head or the tail of a vehicle's Entity.move, as the mod's vehiclePhases name them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMoveEvent {
    pub name: &'static str,
    pub position: Vec3,
    pub velocity: Vec3,
}

thread_local! {
    static VEHICLE_TRACING: Cell<bool> = const { Cell::new(false) };
    static VEHICLE_EVENTS: RefCell<Vec<VehicleMoveEvent>> = const { RefCell::new(Vec::new()) };
}

/// Records where a vehicle's move stands when a `traced_vehicle` is running on this thread.
#[inline]
pub(crate) fn emit_vehicle_move(name: &'static str, state: &SimulationState) {
    if VEHICLE_TRACING.with(Cell::get) {
        VEHICLE_EVENTS.with(|events| events.borrow_mut().push(VehicleMoveEvent { name, position: state.position, velocity: state.velocity }));
    }
}

/// Runs `run`, a vehicle's tick, and gives back what it returned with the heads and tails of its moves, in order.
pub fn traced_vehicle<R>(run: impl FnOnce() -> R) -> (R, Vec<VehicleMoveEvent>) {
    VEHICLE_EVENTS.with(|events| events.borrow_mut().clear());
    VEHICLE_TRACING.with(|tracing| tracing.set(true));
    struct Off;
    impl Drop for Off {
        fn drop(&mut self) {
            VEHICLE_TRACING.with(|tracing| tracing.set(false));
        }
    }
    let off = Off;
    let result = run();
    drop(off);
    (result, VEHICLE_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut())))
}

/// Runs `run` and gives back what it returned with the points it passed, in the order it passed them.
pub fn traced<R>(run: impl FnOnce() -> R) -> (R, Vec<PhaseEvent>) {
    EVENTS.with(|events| events.borrow_mut().clear());
    TRACING.with(|tracing| tracing.set(true));
    // (a panic through here leaves no trace on for the next tick)
    struct Off;
    impl Drop for Off {
        fn drop(&mut self) {
            TRACING.with(|tracing| tracing.set(false));
        }
    }
    let off = Off;
    let result = run();
    drop(off);
    (result, EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut())))
}

#[cfg(test)]
mod tests {
    use super::{traced, Phase};
    use crate::math::Vec3;
    use crate::simulation::{PhysicsConfig, SimulationState};
    use crate::versioned::dispatch;
    use crate::world::WorldSnapshot;

    fn falling() -> SimulationState {
        let mut state = SimulationState::default();
        state.position = Vec3::new(0.5, 80.0, 0.5);
        state.velocity = Vec3::new(0.1, -0.2, 0.05);
        state.half_width = 0.3;
        state.height = 1.8;
        state.is_player = true;
        state.controlled_by_local_instance = true;
        state
    }

    #[test]
    fn a_trace_leaves_the_tick_as_it_was() {
        let world = WorldSnapshot::default();
        let config = PhysicsConfig::default();
        let mut plain = falling();
        let plain_result = dispatch("1.20.4", &mut plain, &world, config);
        let mut seen = falling();
        let (seen_result, events) =
            traced(|| dispatch("1.20.4", &mut seen, &world, config));
        assert_eq!(plain_result.unwrap(), seen_result.unwrap());
        assert_eq!(format!("{plain:?}"), format!("{seen:?}"));
        // the tick's own points, in order (the scene's two are tick_local_player_scene's)
        let names: Vec<_> = events.iter().map(|event| event.phase).collect();
        assert_eq!(names, Phase::ALL[..7].to_vec());
        assert!(events.iter().all(|event| event.occurrence == 0));
    }

    #[test]
    fn no_trace_runs_outside_traced() {
        let world = WorldSnapshot::default();
        let mut state = falling();
        let _ = traced(|| ());
        dispatch("1.20.4", &mut state, &world, PhysicsConfig::default()).unwrap();
        let ((), events) = traced(|| ());
        assert!(events.is_empty());
    }

    #[test]
    fn the_fall_reset_of_slow_falling_comes_after_the_base_tick() {
        let world = WorldSnapshot::default();
        let mut state = falling();
        state.fall_distance = 2.0;
        state.slow_falling = true;
        let (_, events) = traced(|| dispatch("26.3", &mut state, &world, PhysicsConfig::default()));
        let at = |phase| events.iter().find(|event| event.phase == phase).unwrap().state.fall_distance;
        // LivingEntity.aiStep, just before travel (26.3 LivingEntity.java:3046-3048)
        assert_eq!(at(Phase::AfterBaseTick), 2.0);
        assert_eq!(at(Phase::BeforeLivingAiStep), 2.0);
        assert_eq!(at(Phase::BeforeTravel), 0.0);
    }

    #[test]
    fn the_points_show_what_the_tick_did_between_them() {
        let world = WorldSnapshot::default();
        let mut state = falling();
        let (_, events) = traced(|| dispatch("1.20.4", &mut state, &world, PhysicsConfig::default()));
        let at = |phase| events.iter().find(|event| event.phase == phase).unwrap().state;
        // the move is between beforeMove and afterMove, the gravity between afterMove and afterTravel
        assert_eq!(at(Phase::BeforeMove).position, falling().position);
        assert_ne!(at(Phase::AfterMove).position, falling().position);
        assert_eq!(at(Phase::AfterMove).position, at(Phase::AfterTravel).position);
        assert!(at(Phase::AfterTravel).velocity.y < at(Phase::AfterMove).velocity.y);
    }
}
