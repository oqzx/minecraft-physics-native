mod block_traversal;
pub mod collision;
pub mod entity;
#[path = "../data/mod.rs"]
pub mod data;
pub use data as generated_data;
mod legacy_fluid;
pub mod math;
mod napi_api;
mod napi_player;
pub mod piston;
pub mod player;
pub mod projectile;
pub mod ridden;
pub mod simulation;
mod simulation_tests;
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
pub use napi_api::{
    create_simulation_state, refresh_versioned_pose, simulate_arrow_tick, simulate_boat_tick,
    simulate_horse_tick, simulate_projectile_tick, simulate_tick, simulate_ticks,
    simulate_trident_tick, simulate_versioned_arrow_ticks, simulate_versioned_boat_tick,
    simulate_versioned_boat_ticks,
    simulate_versioned_horse_tick, simulate_versioned_horse_ticks,
    simulate_versioned_projectile_ticks, simulate_versioned_trident_ticks,
    simulate_versioned_tick, version_capabilities, generated_block_has_tag,
    generated_state_index, generated_block_state_count, generated_fluid_state_count,
    generated_fluid_state_index, generated_block_defaults, generated_source_shape_boxes,
    generated_method_shape_boxes,
    generated_exact_method_shape_boxes,
    generated_exact_method_shape_boxes_for_state,
    generated_shape_property_dependencies,
    generated_method_shape_property_dependencies,
    generated_method_shape_predicates,
    generated_method_shape_predicates_match_state,
    generated_method_shape_is_state_dependent,
    generated_method_shape_provenance,
    NativeAabb,
    NativeArrowState, NativeArrowTickResult, NativeBlockShape, NativeBlockState, NativeBlockUpdate,
    NativeBoatState, NativeEntityDescriptor, NativeEntityPushResult, NativeEntityPushState,
    NativeHorseState, NativePoseRefreshResult, NativeProjectileState, NativeProjectileTickResult,
    NativeGeneratedShapeBox,
    NativeGeneratedShapeProvenance,
    NativeSimulationConfig, NativeSimulationState, NativeTridentState, NativeTridentTickResult,
    NativeVec3, NativeVersionCapabilities, NativeVersionedEntityState, NativeWorld,
};
pub use player::{PlayerControls, PlayerState, Pose};
pub use projectile::{
    tick_arrow, tick_projectile, tick_trident, ArrowState, ArrowTickResult, ProjectileKind,
    ProjectileState, ProjectileTickResult, TridentState, TridentTickResult,
};
pub use simulation::{PhysicsConfig, SimulationState, TickResult};
pub use simulation::{TickInputSnapshot, TickOutputSnapshot};
pub use vehicle::{passenger_position, tick_boat_with_passenger, VehicleKind, VehicleState};
pub use version::MinecraftVersion;
pub use versioned::{contract_for_version, VersionBehaviorContract};
pub use voxel_shape::VoxelShape;
pub use world::{BlockEffect, BlockShape, BlockState, FluidState, WorldSnapshot};
