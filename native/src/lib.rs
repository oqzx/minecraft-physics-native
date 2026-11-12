mod block_traversal;
pub mod fast_hash;
pub mod collision;
pub mod entity;
pub mod entity_travel;
#[path = "../data/mod.rs"]
pub mod data;
#[path = "../data/block_shapes.rs"]
pub mod block_shapes;
pub mod dragon;
pub use data as generated_data;
mod legacy_fluid;
mod legacy_shapes;
pub mod math;
pub mod minecart;
pub mod moving_entity;
#[cfg(feature = "bindings")]
mod napi_api;
#[cfg(feature = "bindings")]
mod napi_bulk;
#[cfg(feature = "bindings")]
mod napi_minecart;
#[cfg(feature = "bindings")]
mod napi_moving_entity;
#[cfg(feature = "bindings")]
mod napi_player;
#[cfg(feature = "bindings")]
mod napi_projectile;
pub mod phase;
pub mod piston;
pub mod player;
pub mod projectile;
pub mod projectile_hits;
pub mod ridden;
pub mod shulker_box;
pub mod simulation;
mod simulation_tests;
#[cfg(test)]
mod gate_fixtures_tests;
#[cfg(test)]
mod helper_tests;
#[cfg(test)]
mod riptide_release_tests;
#[cfg(test)]
mod engine_paths_tests;
#[cfg(test)]
mod oracle_living_tests;
#[cfg(test)]
mod entity_move_tests;
#[cfg(test)]
mod oracle_move_tests;
#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
mod numeric_tests;
#[cfg(test)]
mod robustness_tests;
#[cfg(test)]
mod entity_robustness_tests;
#[cfg(test)]
mod reach_fixtures_e2_tests;
#[cfg(test)]
mod reach_fixtures_e1_tests;
pub mod trig;
pub mod vehicle;
pub mod version;
pub mod versioned;
pub mod voxel_shape;
pub mod world;

pub use collision::{clip_axis, Axis};
pub use entity::{
    entity_push_impulses, entity_push_impulses_with_pushthrough, Dimensions, EntityFlags,
    EntityState,
};
pub use math::{Aabb, Vec3};
#[cfg(feature = "bindings")]
pub use napi_api::{
    collision_box_meets_query, create_simulation_state, refresh_versioned_pose, simulate_arrow_tick, simulate_boat_tick,
    simulate_horse_tick, simulate_projectile_tick, simulate_tick, simulate_ticks,
    simulate_trident_tick, simulate_versioned_arrow_ticks, simulate_versioned_boat_tick,
    simulate_versioned_boat_ticks,
    simulate_versioned_horse_tick, simulate_versioned_horse_ticks,
    simulate_versioned_projectile_ticks, simulate_versioned_trident_ticks,
    simulate_versioned_tick, simulate_versioned_tick_outcome, simulate_versioned_tick_traced,
    version_capabilities,
    generated_block_has_tag,
    generated_state_index, generated_block_state_count, generated_fluid_state_count,
    generated_fluid_state_index, generated_block_defaults, generated_source_shape_boxes,
    generated_method_shape_boxes,
    generated_exact_method_shape_boxes,
    generated_exact_method_shape_boxes_for_state,
    generated_state_shape,
    generated_state_shape_for_properties,
    generated_shape_property_dependencies,
    generated_method_shape_property_dependencies,
    generated_method_shape_predicates,
    generated_method_shape_predicates_match_state,
    generated_method_shape_is_state_dependent,
    generated_method_shape_provenance,
    NativeAabb,
    NativeArrowState, NativeArrowTickResult, NativeBlockPosition, NativeBlockShape,
    NativeBlockState, NativeBlockUpdate,
    NativeBoatState, NativeEntityDescriptor, NativeEntityPushResult, NativeEntityPushState,
    NativeHorseState, NativePoseRefreshResult, NativeProjectileState, NativeProjectileTickResult,
    NativeGeneratedShapeBox,
    NativeGeneratedShapeProvenance,
    NativeSimulationConfig, NativeSimulationState, NativeTridentState, NativeTridentTickResult,
    NativeVec3, NativeVersionCapabilities, NativeVersionedEntityState, NativeWorld,
};
pub use player::{PlayerControls, PlayerState, Pose};
pub use projectile::{
    tick_arrow, tick_arrow_in_release, tick_arrow_in_world, tick_projectile,
    tick_projectile_in_release, tick_projectile_in_world, tick_projectile_with_targets, tick_trident, tick_trident_in_release,
    tick_trident_in_world, ArrowState, ArrowTickResult, ProjectileKind, ProjectileState,
    ProjectileTickResult, TridentState, TridentTickResult,
};
pub use simulation::{PhysicsConfig, SimulationState, TickResult};
pub use simulation::{TickInputSnapshot, TickOutputSnapshot};
pub use vehicle::{
    boat_passenger_position, boat_passenger_yaw, tick_boat_with_passenger, BoatSeat, VehicleKind,
    VehicleState,
};
pub use version::MinecraftVersion;
pub use versioned::{contract_for_version, ReleaseEngine, VersionBehaviorContract};
pub use voxel_shape::VoxelShape;
pub use world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};

// Who owns what. A release is chosen once: `NativePlayer` takes it at construction and keeps it, `ReleaseEngine` and the
// TypeScript engine classes are that choice and nothing else, and a stateless call names it each time. Only an exact
// registered release is accepted, at construction, and no nearest release stands in for another.
//
// Simulation state: `NativePlayer` owns its `SimulationState`, its nearby entities and what a tick reported; only
// `setState`, `setBoundingBox` and the ticks change them. `setState` is the reset: it replaces the whole state, drops the
// last tick's pushes and an unfinished `tickMove`, and a refused state changes nothing. A failed tick leaves the state of
// the last tick that ran. A teleport is `setState` with the new position; the box kept before 1.17 and the supporting
// block of later releases are in the state, so whoever replaces it says what they become, and ordinary ticks keep them.
// Stateless calls own nothing: a state goes in, and its successor comes out.
//
// World storage: `NativeWorld` owns its blocks, and a tick only borrows it for the call. A world may serve any number of
// players and releases at once, each block carrying the release its state id is of. A `NativePlayer` belongs to the world
// it last ticked in: another world is refused until `setState` moves it, and a tick `tickMove` started ends in the world
// it began in.
//
// Crossing N-API: input objects and typed arrays are read during the call and none is kept, nor written to. Every output
// is the caller's own copy: a kept array or object is not changed by later ticks, and editing one changes nothing in the
// engine. Native objects have no `dispose`: JavaScript's collector frees them, once, and none holds a reference to another
// to outlive. The adapter's `NativeEngine` owns the syncs and listeners it makes, and `releaseWorld` and `dispose` undo them.

/// What may cross threads, which the compiler holds to. The engine's own values are plain data, with no
/// interior mutability: `SimulationState`, `PhysicsConfig`, `WorldSnapshot` and the tick snapshots are `Send`
/// and `Sync`, and a tick of one world from several threads shares nothing but read-only tables (the
/// generated release data, and the trigonometry tables behind `OnceLock`).
///
/// `NativeWorld` and `NativePlayer` are `Send` and `Sync` as Rust values, which does not make them shareable
/// from JavaScript: a napi class belongs to the isolate that made it. The addon loads once per worker, each
/// makes its own objects, and what crosses is data (a state, a block update): an object handed to another
/// worker arrives without its native half. No call runs off the JavaScript thread, so a typed array is only
/// borrowed for the length of a call. Any asynchronous call must own its data (`SimulationState` and
/// `WorldSnapshot` can be moved to a thread as they are); the wasm builds are not covered by this.
// (a type whose parameter must be Send + Sync, named for each: the compiler refuses one that is not)
struct SendAndSync<T: Send + Sync>(std::marker::PhantomData<T>);
const _: [SendAndSync<SimulationState>; 0] = [];
const _: [SendAndSync<PhysicsConfig>; 0] = [];
const _: [SendAndSync<WorldSnapshot>; 0] = [];
const _: [SendAndSync<TickInputSnapshot>; 0] = [];
const _: [SendAndSync<TickOutputSnapshot>; 0] = [];
#[cfg(feature = "bindings")]
const _: [SendAndSync<napi_api::NativeWorld>; 0] = [];
#[cfg(feature = "bindings")]
const _: [SendAndSync<napi_player::NativePlayer>; 0] = [];
