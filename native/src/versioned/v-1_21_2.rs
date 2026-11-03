use crate::simulation::{
    tick_with_profile, LandingDamageRounding, PhysicsConfig, SimulationState, TickResult,
    VersionPhysicsProfile, VersionPhysicsRules,
};
use crate::version::MinecraftVersion;
use crate::versioned::VersionEngine;
use crate::world::WorldSnapshot;

pub struct Engine;
pub struct Profile;

impl VersionPhysicsProfile for Profile {
    fn landing_damage_rounding() -> LandingDamageRounding {
        LandingDamageRounding::Ceil
    }

    fn rules() -> VersionPhysicsRules {
        VersionPhysicsRules {
            release: MinecraftVersion::new(1, 21, 2),
            gravity: 0.08,
            water_gravity_divisor: 16.0,
            lava_gravity_divisor: 4.0,
            water_drag: 0.8,
            water_sprint_drag: 0.9,
            lava_drag: 0.5,
            air_drag: 0.91,
            vertical_drag: 0.98,
            fluid_vertical_drag: 0.8,
            water_flow_scale: 0.014,
            lava_flow_scale: 0.0023333333333333335,
            ultra_warm_lava_flow_scale: 0.007,
            fluid_travel_split: true,
            fluid_falling_checks_gravity_argument: true,
            water_input_speed: 0.02,
            lava_input_speed: 0.02,
            step_height: 0.6,
            swimming_vertical_steering: 0.06,
            swimming_downward_steering: 0.085,
            water_movement_efficiency_divisor: 1.0,
            water_movement_efficiency_attribute: true,
        }
    }
}

impl VersionEngine for Engine {
    const VERSION: &'static str = "1.21.2";
    fn tick(
        state: &mut SimulationState,
        world: &WorldSnapshot,
        config: PhysicsConfig,
    ) -> TickResult {
        tick_with_profile::<Profile>(state, world, config)
    }
}
