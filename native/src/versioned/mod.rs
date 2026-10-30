use crate::simulation::{
    LandingDamageRounding, PhysicsConfig, SimulationState, TickResult, VersionPhysicsProfile,
};
use crate::world::WorldSnapshot;

pub trait VersionEngine {
    const VERSION: &'static str;

    fn tick(
        state: &mut SimulationState,
        world: &WorldSnapshot,
        config: PhysicsConfig,
    ) -> TickResult;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VersionBehaviorContract {
    pub fluid_travel_split: bool,
    pub fluid_falling_checks_gravity_argument: bool,
    pub water_movement_efficiency_attribute: bool,
    pub water_movement_efficiency_divisor: f64,
    pub landing_damage_rounding: LandingDamageRounding,
}

#[path = "v-1_10_2.rs"]
pub mod v1_10_2;
#[path = "v-1_11_2.rs"]
pub mod v1_11_2;
#[path = "v-1_12_2.rs"]
pub mod v1_12_2;
#[path = "v-1_13_2.rs"]
pub mod v1_13_2;
#[path = "v-1_14_4.rs"]
pub mod v1_14_4;
#[path = "v-1_15.rs"]
pub mod v1_15;
#[path = "v-1_15_1.rs"]
pub mod v1_15_1;
#[path = "v-1_15_2.rs"]
pub mod v1_15_2;
#[path = "v-1_16.rs"]
pub mod v1_16;
#[path = "v-1_16_1.rs"]
pub mod v1_16_1;
#[path = "v-1_16_2.rs"]
pub mod v1_16_2;
#[path = "v-1_16_3.rs"]
pub mod v1_16_3;
#[path = "v-1_16_4.rs"]
pub mod v1_16_4;
#[path = "v-1_16_5.rs"]
pub mod v1_16_5;
#[path = "v-1_17.rs"]
pub mod v1_17;
#[path = "v-1_17_1.rs"]
pub mod v1_17_1;
#[path = "v-1_18.rs"]
pub mod v1_18;
#[path = "v-1_18_1.rs"]
pub mod v1_18_1;
#[path = "v-1_18_2.rs"]
pub mod v1_18_2;
#[path = "v-1_19.rs"]
pub mod v1_19;
#[path = "v-1_19_1.rs"]
pub mod v1_19_1;
#[path = "v-1_19_2.rs"]
pub mod v1_19_2;
#[path = "v-1_19_3.rs"]
pub mod v1_19_3;
#[path = "v-1_19_4.rs"]
pub mod v1_19_4;
#[path = "v-1_20.rs"]
pub mod v1_20;
#[path = "v-1_20_1.rs"]
pub mod v1_20_1;
#[path = "v-1_20_2.rs"]
pub mod v1_20_2;
#[path = "v-1_20_3.rs"]
pub mod v1_20_3;
#[path = "v-1_20_4.rs"]
pub mod v1_20_4;
#[path = "v-1_20_5.rs"]
pub mod v1_20_5;
#[path = "v-1_20_6.rs"]
pub mod v1_20_6;
#[path = "v-1_21.rs"]
pub mod v1_21;
#[path = "v-1_21_1.rs"]
pub mod v1_21_1;
#[path = "v-1_21_10.rs"]
pub mod v1_21_10;
#[path = "v-1_21_11.rs"]
pub mod v1_21_11;
#[path = "v-1_21_2.rs"]
pub mod v1_21_2;
#[path = "v-1_21_3.rs"]
pub mod v1_21_3;
#[path = "v-1_21_4.rs"]
pub mod v1_21_4;
#[path = "v-1_21_5.rs"]
pub mod v1_21_5;
#[path = "v-1_21_6.rs"]
pub mod v1_21_6;
#[path = "v-1_21_7.rs"]
pub mod v1_21_7;
#[path = "v-1_21_8.rs"]
pub mod v1_21_8;
#[path = "v-1_21_9.rs"]
pub mod v1_21_9;
#[path = "v-1_8_8.rs"]
pub mod v1_8_8;
#[path = "v-1_9_4.rs"]
pub mod v1_9_4;
#[path = "v-26_1.rs"]
pub mod v26_1;
#[path = "v-26_1_1.rs"]
pub mod v26_1_1;
#[path = "v-26_1_2.rs"]
pub mod v26_1_2;
#[path = "v-26_2.rs"]
pub mod v26_2;
#[path = "v-26_3.rs"]
pub mod v26_3;

pub fn dispatch(
    version: &str,
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> Result<TickResult, String> {
    match version {
        v1_8_8::Engine::VERSION => Ok(v1_8_8::Engine::tick(state, world, config)),
        v1_9_4::Engine::VERSION => Ok(v1_9_4::Engine::tick(state, world, config)),
        v1_10_2::Engine::VERSION => Ok(v1_10_2::Engine::tick(state, world, config)),
        v1_11_2::Engine::VERSION => Ok(v1_11_2::Engine::tick(state, world, config)),
        v1_12_2::Engine::VERSION => Ok(v1_12_2::Engine::tick(state, world, config)),
        v1_13_2::Engine::VERSION => Ok(v1_13_2::Engine::tick(state, world, config)),
        v1_14_4::Engine::VERSION => Ok(v1_14_4::Engine::tick(state, world, config)),
        v1_15::Engine::VERSION => Ok(v1_15::Engine::tick(state, world, config)),
        v1_15_1::Engine::VERSION => Ok(v1_15_1::Engine::tick(state, world, config)),
        v1_15_2::Engine::VERSION => Ok(v1_15_2::Engine::tick(state, world, config)),
        v1_16::Engine::VERSION => Ok(v1_16::Engine::tick(state, world, config)),
        v1_16_1::Engine::VERSION => Ok(v1_16_1::Engine::tick(state, world, config)),
        v1_16_2::Engine::VERSION => Ok(v1_16_2::Engine::tick(state, world, config)),
        v1_16_3::Engine::VERSION => Ok(v1_16_3::Engine::tick(state, world, config)),
        v1_16_4::Engine::VERSION => Ok(v1_16_4::Engine::tick(state, world, config)),
        v1_16_5::Engine::VERSION => Ok(v1_16_5::Engine::tick(state, world, config)),
        v1_17::Engine::VERSION => Ok(v1_17::Engine::tick(state, world, config)),
        v1_17_1::Engine::VERSION => Ok(v1_17_1::Engine::tick(state, world, config)),
        v1_18::Engine::VERSION => Ok(v1_18::Engine::tick(state, world, config)),
        v1_18_1::Engine::VERSION => Ok(v1_18_1::Engine::tick(state, world, config)),
        v1_18_2::Engine::VERSION => Ok(v1_18_2::Engine::tick(state, world, config)),
        v1_19::Engine::VERSION => Ok(v1_19::Engine::tick(state, world, config)),
        v1_19_1::Engine::VERSION => Ok(v1_19_1::Engine::tick(state, world, config)),
        v1_19_2::Engine::VERSION => Ok(v1_19_2::Engine::tick(state, world, config)),
        v1_19_3::Engine::VERSION => Ok(v1_19_3::Engine::tick(state, world, config)),
        v1_19_4::Engine::VERSION => Ok(v1_19_4::Engine::tick(state, world, config)),
        v1_20::Engine::VERSION => Ok(v1_20::Engine::tick(state, world, config)),
        v1_20_1::Engine::VERSION => Ok(v1_20_1::Engine::tick(state, world, config)),
        v1_20_2::Engine::VERSION => Ok(v1_20_2::Engine::tick(state, world, config)),
        v1_20_3::Engine::VERSION => Ok(v1_20_3::Engine::tick(state, world, config)),
        v1_20_4::Engine::VERSION => Ok(v1_20_4::Engine::tick(state, world, config)),
        v1_20_5::Engine::VERSION => Ok(v1_20_5::Engine::tick(state, world, config)),
        v1_20_6::Engine::VERSION => Ok(v1_20_6::Engine::tick(state, world, config)),
        v1_21::Engine::VERSION => Ok(v1_21::Engine::tick(state, world, config)),
        v1_21_1::Engine::VERSION => Ok(v1_21_1::Engine::tick(state, world, config)),
        v1_21_2::Engine::VERSION => Ok(v1_21_2::Engine::tick(state, world, config)),
        v1_21_3::Engine::VERSION => Ok(v1_21_3::Engine::tick(state, world, config)),
        v1_21_4::Engine::VERSION => Ok(v1_21_4::Engine::tick(state, world, config)),
        v1_21_5::Engine::VERSION => Ok(v1_21_5::Engine::tick(state, world, config)),
        v1_21_6::Engine::VERSION => Ok(v1_21_6::Engine::tick(state, world, config)),
        v1_21_7::Engine::VERSION => Ok(v1_21_7::Engine::tick(state, world, config)),
        v1_21_8::Engine::VERSION => Ok(v1_21_8::Engine::tick(state, world, config)),
        v1_21_9::Engine::VERSION => Ok(v1_21_9::Engine::tick(state, world, config)),
        v1_21_10::Engine::VERSION => Ok(v1_21_10::Engine::tick(state, world, config)),
        v1_21_11::Engine::VERSION => Ok(v1_21_11::Engine::tick(state, world, config)),
        v26_1::Engine::VERSION => Ok(v26_1::Engine::tick(state, world, config)),
        v26_1_1::Engine::VERSION => Ok(v26_1_1::Engine::tick(state, world, config)),
        v26_1_2::Engine::VERSION => Ok(v26_1_2::Engine::tick(state, world, config)),
        v26_2::Engine::VERSION => Ok(v26_2::Engine::tick(state, world, config)),
        v26_3::Engine::VERSION => Ok(v26_3::Engine::tick(state, world, config)),
        _ => Err(format!(
            "unsupported decompiled Minecraft version: {version}"
        )),
    }
}

pub fn is_supported(version: &str) -> bool {
    is_legacy(version) || is_source_backed(version)
}

fn is_legacy(version: &str) -> bool {
    matches!(
        version,
        v1_8_8::Engine::VERSION
            | v1_9_4::Engine::VERSION
            | v1_10_2::Engine::VERSION
            | v1_11_2::Engine::VERSION
            | v1_12_2::Engine::VERSION
            | v1_13_2::Engine::VERSION
    )
}

fn is_source_backed(version: &str) -> bool {
    matches!(
        version,
        v1_14_4::Engine::VERSION
            | v1_15::Engine::VERSION
            | v1_15_1::Engine::VERSION
            | v1_15_2::Engine::VERSION
            | v1_16::Engine::VERSION
            | v1_16_1::Engine::VERSION
            | v1_16_2::Engine::VERSION
            | v1_16_3::Engine::VERSION
            | v1_16_4::Engine::VERSION
            | v1_16_5::Engine::VERSION
            | v1_17::Engine::VERSION
            | v1_17_1::Engine::VERSION
            | v1_18::Engine::VERSION
            | v1_18_1::Engine::VERSION
            | v1_18_2::Engine::VERSION
            | v1_19::Engine::VERSION
            | v1_19_1::Engine::VERSION
            | v1_19_2::Engine::VERSION
            | v1_19_3::Engine::VERSION
            | v1_19_4::Engine::VERSION
            | v1_20::Engine::VERSION
            | v1_20_1::Engine::VERSION
            | v1_20_2::Engine::VERSION
            | v1_20_3::Engine::VERSION
            | v1_20_4::Engine::VERSION
            | v1_20_5::Engine::VERSION
            | v1_20_6::Engine::VERSION
            | v1_21::Engine::VERSION
            | v1_21_1::Engine::VERSION
            | v1_21_2::Engine::VERSION
            | v1_21_3::Engine::VERSION
            | v1_21_4::Engine::VERSION
            | v1_21_5::Engine::VERSION
            | v1_21_6::Engine::VERSION
            | v1_21_7::Engine::VERSION
            | v1_21_8::Engine::VERSION
            | v1_21_9::Engine::VERSION
            | v1_21_10::Engine::VERSION
            | v1_21_11::Engine::VERSION
            | v26_1::Engine::VERSION
            | v26_1_1::Engine::VERSION
            | v26_1_2::Engine::VERSION
            | v26_2::Engine::VERSION
            | v26_3::Engine::VERSION
    ) && crate::generated_data::fluid(version, "water").is_some()
        && crate::generated_data::fluid(version, "lava").is_some()
}

pub fn contract_for_version(version: &str) -> Result<VersionBehaviorContract, String> {
    let contract = match version {
        v1_8_8::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_8_8::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_8_8::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_9_4::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_9_4::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_9_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_10_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_10_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_10_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_11_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_11_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_11_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_12_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_12_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_12_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_13_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_13_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_13_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_14_4::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_14_4::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_14_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_15::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_15::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_15::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_15_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_15_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_15_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_15_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_15_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_15_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_16::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_16::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_16::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_16_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_16_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_16_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_16_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_16_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_16_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_16_3::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_16_3::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_16_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_16_4::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_16_4::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_16_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_16_5::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_16_5::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_16_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_17::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_17::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_17::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_17_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_17_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_17_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_18::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_18::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_18::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_18_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_18_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_18_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_18_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: false,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_18_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_18_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_19::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_19::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_19::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_19::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_19_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_19_1::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_19_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_19_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_19_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_19_2::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_19_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_19_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_19_3::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_19_3::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_19_3::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_19_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_19_4::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_19_4::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_19_4::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_19_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_20::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20_1::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_20_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20_2::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_20_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20_3::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20_3::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_20_3::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20_4::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20_4::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_20_4::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20_5::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20_5::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_20_5::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_20_5::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_20_6::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: false,
            fluid_falling_checks_gravity_argument: v1_20_6::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: false,
            water_movement_efficiency_divisor: v1_20_6::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_20_6::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_1::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_1::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_1::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_2::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_2::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_2::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_3::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_3::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_3::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_3::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_3::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_4::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_4::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_4::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_4::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_4::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_5::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_5::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_5::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_5::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_5::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_6::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_6::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_6::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_6::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_6::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_6::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_7::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_7::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_7::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_7::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_7::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_7::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_8::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_8::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_8::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_8::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_8::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_8::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_9::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_9::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_9::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_9::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_9::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_9::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_10::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_10::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_10::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_10::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_10::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_10::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v1_21_11::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v1_21_11::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v1_21_11::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v1_21_11::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v1_21_11::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v1_21_11::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v26_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v26_1::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v26_1::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v26_1::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v26_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v26_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v26_1_1::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v26_1_1::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v26_1_1::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v26_1_1::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v26_1_1::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v26_1_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v26_1_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v26_1_2::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v26_1_2::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v26_1_2::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v26_1_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v26_1_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v26_2::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v26_2::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v26_2::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v26_2::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v26_2::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v26_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        v26_3::Engine::VERSION => VersionBehaviorContract {
            fluid_travel_split: v26_3::Profile::rules().fluid_travel_split,
            fluid_falling_checks_gravity_argument: v26_3::Profile::rules()
                .fluid_falling_checks_gravity_argument,
            water_movement_efficiency_attribute: v26_3::Profile::rules()
                .water_movement_efficiency_attribute,
            water_movement_efficiency_divisor: v26_3::Profile::rules()
                .water_movement_efficiency_divisor,
            landing_damage_rounding:
                <v26_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
        },
        _ => {
            return Err(format!(
                "unsupported decompiled Minecraft version: {version}"
            ))
        }
    };
    Ok(contract)
}

pub fn entity_pushthrough_enabled(version: &str) -> Result<bool, String> {
    match version {
        v1_8_8::Engine::VERSION
        | v1_9_4::Engine::VERSION
        | v1_10_2::Engine::VERSION
        | v1_11_2::Engine::VERSION
        | v1_12_2::Engine::VERSION
        | v1_13_2::Engine::VERSION
        | v1_14_4::Engine::VERSION
        | v1_15::Engine::VERSION
        | v1_15_1::Engine::VERSION
        | v1_15_2::Engine::VERSION
        | v1_16::Engine::VERSION
        | v1_16_1::Engine::VERSION
        | v1_16_2::Engine::VERSION
        | v1_16_3::Engine::VERSION
        | v1_16_4::Engine::VERSION
        | v1_16_5::Engine::VERSION => Ok(true),
        v1_17::Engine::VERSION
        | v1_17_1::Engine::VERSION
        | v1_18::Engine::VERSION
        | v1_18_1::Engine::VERSION
        | v1_18_2::Engine::VERSION
        | v1_19::Engine::VERSION
        | v1_19_1::Engine::VERSION
        | v1_19_2::Engine::VERSION
        | v1_19_3::Engine::VERSION
        | v1_19_4::Engine::VERSION
        | v1_20::Engine::VERSION
        | v1_20_1::Engine::VERSION
        | v1_20_2::Engine::VERSION
        | v1_20_3::Engine::VERSION
        | v1_20_4::Engine::VERSION
        | v1_20_5::Engine::VERSION
        | v1_20_6::Engine::VERSION
        | v1_21::Engine::VERSION
        | v1_21_1::Engine::VERSION
        | v1_21_2::Engine::VERSION
        | v1_21_3::Engine::VERSION
        | v1_21_4::Engine::VERSION
        | v1_21_5::Engine::VERSION
        | v1_21_6::Engine::VERSION
        | v1_21_7::Engine::VERSION
        | v1_21_8::Engine::VERSION
        | v1_21_9::Engine::VERSION
        | v1_21_10::Engine::VERSION
        | v1_21_11::Engine::VERSION
        | v26_1::Engine::VERSION
        | v26_1_1::Engine::VERSION
        | v26_1_2::Engine::VERSION
        | v26_2::Engine::VERSION
        | v26_3::Engine::VERSION => Ok(false),
        _ => Err(format!(
            "unsupported decompiled Minecraft version: {version}"
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_release_data_has_core_fluids() {
        for release in crate::generated_data::RELEASES {
            for fluid_id in ["water", "lava"] {
                let source = crate::generated_data::fluid(release.version, fluid_id).expect("source fluid");
                let flowing = crate::generated_data::fluid(
                    release.version,
                    &format!("flowing_{fluid_id}"),
                )
                .expect("flowing fluid");
                assert_eq!(source.state_count, 2, "{} source state count", release.version);
                assert_eq!(flowing.state_count, 16, "{} flowing state count", release.version);
                assert_eq!(source.property_names, &["FALLING"]);
                assert_eq!(flowing.property_names, &["LEVEL", "FALLING"]);
                assert_eq!(flowing.property_values.len(), 10);
            }
            assert!(crate::generated_data::fluid_tag(release.version, "water").is_some(), "missing water tag for {}", release.version);
            assert!(crate::generated_data::fluid_tag(release.version, "lava").is_some(), "missing lava tag for {}", release.version);
            assert!(!release.block_tags.is_empty(), "missing block tags for {}", release.version);
            assert!(!release.blocks.is_empty());
        }
    }

    #[test]
    fn generated_block_tag_memberships_resolve_nested_tags() {
        assert!(crate::generated_data::block_has_tag("1.20.6", "oak_leaves", "leaves"));
        assert!(crate::generated_data::block_has_tag("1.20.6", "oak_door", "doors"));
        assert!(!crate::generated_data::block_has_tag("1.20.6", "stone", "leaves"));
        assert!(!crate::generated_data::block_has_tag("unsupported", "stone", "leaves"));
    }

    #[test]
    fn generated_fluid_state_definitions_match_flowing_fluid_source() {
        let water = crate::generated_data::fluid("1.20.6", "water").expect("water fluid");
        assert_eq!(water.state_count, 2);
        assert_eq!(water.property_names, &["FALLING"]);
        assert_eq!(water.property_values, &["false", "true"]);
        let flowing = crate::generated_data::fluid("1.20.6", "flowing_water").expect("flowing water");
        assert_eq!(flowing.state_count, 16);
        assert_eq!(flowing.property_names, &["LEVEL", "FALLING"]);
        assert_eq!(flowing.property_values, &["1", "2", "3", "4", "5", "6", "7", "8", "false", "true"]);
        assert_eq!(
            crate::generated_data::fluid_state_index(
                "1.20.6",
                "flowing_water",
                &[("LEVEL", "1"), ("FALLING", "false")],
            ),
            Some(0)
        );
        assert_eq!(
            crate::generated_data::fluid_state_index(
                "1.20.6",
                "flowing_water",
                &[("LEVEL", "8"), ("FALLING", "true")],
            ),
            Some(15)
        );
        assert_eq!(
            crate::generated_data::fluid_state_index(
                "1.20.6",
                "flowing_water",
                &[("LEVEL", "8"), ("LEVEL", "7")],
            ),
            None
        );
    }

    #[test]
    fn generated_block_lookup_preserves_state_and_shape_metadata() {
        let stone = crate::generated_data::block("1.20.6", "stone").expect("stone registry entry");
        assert_eq!(stone.class_name, Some("Block"));
        assert_eq!(stone.state_count, Some(1));
        assert!(stone.properties.is_empty());
        assert!(!stone.has_collision_shape_method);
        assert!(crate::generated_data::block("unsupported", "stone").is_none());
        let door = crate::generated_data::block("1.20.6", "oak_door").expect("door registry entry");
        assert_eq!(door.property_order, &["HALF", "FACING", "OPEN", "HINGE", "POWERED"]);
        assert_eq!(door.state_count, Some(96));
        let facing = door.properties.iter().find(|property| property.name == "FACING").unwrap();
        assert_eq!(facing.defaults, &["Direction.NORTH"]);
        assert_eq!(crate::generated_data::state_index("1.20.6", "stone", &[]), Some(0));
        assert_eq!(
            crate::generated_data::state_index("1.20.6", "oak_door", &[("OPEN", "false")]),
            None
        );
        assert_eq!(
            crate::generated_data::state_index(
                "1.20.6",
                "oak_fence_gate",
                &[("OPEN", "true"), ("POWERED", "false"), ("IN_WALL", "false")],
            ),
            Some(2)
        );
        assert_eq!(
            crate::generated_data::state_index(
                "1.20.6",
                "oak_fence_gate",
                &[("OPEN", "false"), ("POWERED", "false"), ("UNKNOWN", "false")],
            ),
            None
        );
        assert_eq!(
            crate::generated_data::state_index(
                "1.20.6",
                "oak_fence_gate",
                &[("OPEN", "false"), ("POWERED", "false"), ("POWERED", "true")],
            ),
            None
        );
        assert_eq!(
            crate::generated_data::state_index(
                "1.20.6",
                "oak_door",
                &[
                    ("FACING", "east"),
                    ("HALF", "upper"),
                    ("OPEN", "false"),
                    ("HINGE", "left"),
                    ("POWERED", "false"),
                ],
            ),
            Some(16)
        );
    }

    use super::*;
    use crate::simulation::{
        LandingDamageRounding, PhysicsConfig, SimulationState, VersionPhysicsProfile,
    };
    use crate::world::WorldSnapshot;

    #[test]
    fn dispatches_decompiled_versions() {
        let world = WorldSnapshot::new();
        for version in [
            "1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2", "1.14.4", "1.15", "1.15.1",
            "1.15.2", "1.16", "1.16.1", "1.16.2", "1.16.3", "1.16.4", "1.16.5", "1.17", "1.17.1",
            "1.18", "1.18.1", "1.18.2", "1.19", "1.19.1", "1.19.2", "1.19.3", "1.19.4", "1.20",
            "1.20.1", "1.20.2", "1.20.3", "1.20.4", "1.20.5", "1.20.6", "1.21", "1.21.1", "1.21.2",
            "1.21.3", "1.21.4", "1.21.5", "1.21.6", "1.21.7", "1.21.8", "1.21.9", "1.21.10",
            "1.21.11", "26.1", "26.1.1", "26.1.2", "26.2", "26.3",
        ] {
            let mut state = SimulationState::default();
            assert!(dispatch(version, &mut state, &world, PhysicsConfig::default()).is_ok());
            assert!(state.position.is_finite());
            assert!(state.velocity.is_finite());
        }
    }

    #[test]
    fn profiles_carry_their_release() {
        macro_rules! assert_releases {
            ($($module:ident),*) => {
                $(assert_eq!(
                    crate::version::MinecraftVersion::parse($module::Engine::VERSION),
                    Some($module::Profile::rules().release),
                );)*
            };
        }
        assert_releases!(
            v1_8_8, v1_9_4, v1_10_2, v1_11_2, v1_12_2, v1_13_2, v1_14_4, v1_15, v1_15_1, v1_15_2,
            v1_16, v1_16_1, v1_16_2, v1_16_3, v1_16_4, v1_16_5, v1_17, v1_17_1, v1_18, v1_18_1,
            v1_18_2, v1_19, v1_19_1, v1_19_2, v1_19_3, v1_19_4, v1_20, v1_20_1, v1_20_2, v1_20_3,
            v1_20_4, v1_20_5, v1_20_6, v1_21, v1_21_1, v1_21_2, v1_21_3, v1_21_4, v1_21_5, v1_21_6,
            v1_21_7, v1_21_8, v1_21_9, v1_21_10, v1_21_11, v26_1, v26_1_1, v26_1_2, v26_2, v26_3
        );
    }

    #[test]
    fn rejects_versions_without_source_snapshot() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState::default();
        assert!(dispatch("1.22.1", &mut state, &world, PhysicsConfig::default()).is_err());
        assert!(dispatch("1.19.5", &mut state, &world, PhysicsConfig::default()).is_err());
        assert!(dispatch("26.1.2", &mut state, &world, PhysicsConfig::default()).is_ok());
    }

    #[test]
    fn version_profiles_own_behavioral_rounding_policy() {
        assert_eq!(
            <v1_21_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Ceil
        );
        assert_eq!(
            <v1_21_8::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
        assert_eq!(
            <v1_21_11::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
        assert_eq!(
            <v26_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
        assert_eq!(
            <v26_1_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
        assert_eq!(
            <v26_1_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
        assert_eq!(
            <v26_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
        assert_eq!(
            <v26_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
    }

    #[test]
    fn all_supported_versions_expose_explicit_behavior_contract() {
        for version in [
            "1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2", "1.14.4", "1.15", "1.15.1",
            "1.15.2", "1.16", "1.16.1", "1.16.2", "1.16.3", "1.16.4", "1.16.5", "1.17", "1.17.1",
            "1.18", "1.18.1", "1.18.2", "1.19", "1.19.1", "1.19.2", "1.19.3", "1.19.4", "1.20",
            "1.20.1", "1.20.2", "1.20.3", "1.20.4", "1.20.5", "1.20.6", "1.21", "1.21.1", "1.21.2",
            "1.21.3", "1.21.4", "1.21.5", "1.21.6", "1.21.7", "1.21.8", "1.21.9", "1.21.10",
            "1.21.11", "26.1", "26.1.1", "26.1.2", "26.2", "26.3",
        ] {
            let contract = contract_for_version(version).unwrap();
            let rules = match version {
                "1.8.8" => v1_8_8::Profile::rules(),
                "1.9.4" => v1_9_4::Profile::rules(),
                "1.10.2" => v1_10_2::Profile::rules(),
                "1.11.2" => v1_11_2::Profile::rules(),
                "1.12.2" => v1_12_2::Profile::rules(),
                "1.13.2" => v1_13_2::Profile::rules(),
                "1.14.4" => v1_14_4::Profile::rules(),
                "1.15" => v1_15::Profile::rules(),
                "1.15.1" => v1_15_1::Profile::rules(),
                "1.15.2" => v1_15_2::Profile::rules(),
                "1.16" => v1_16::Profile::rules(),
                "1.16.1" => v1_16_1::Profile::rules(),
                "1.16.2" => v1_16_2::Profile::rules(),
                "1.16.3" => v1_16_3::Profile::rules(),
                "1.16.4" => v1_16_4::Profile::rules(),
                "1.16.5" => v1_16_5::Profile::rules(),
                "1.17" => v1_17::Profile::rules(),
                "1.17.1" => v1_17_1::Profile::rules(),
                "1.18" => v1_18::Profile::rules(),
                "1.18.1" => v1_18_1::Profile::rules(),
                "1.18.2" => v1_18_2::Profile::rules(),
                "1.19" => v1_19::Profile::rules(),
                "1.19.1" => v1_19_1::Profile::rules(),
                "1.19.2" => v1_19_2::Profile::rules(),
                "1.19.3" => v1_19_3::Profile::rules(),
                "1.19.4" => v1_19_4::Profile::rules(),
                "1.20" => v1_20::Profile::rules(),
                "1.20.1" => v1_20_1::Profile::rules(),
                "1.20.2" => v1_20_2::Profile::rules(),
                "1.20.3" => v1_20_3::Profile::rules(),
                "1.20.4" => v1_20_4::Profile::rules(),
                "1.20.5" => v1_20_5::Profile::rules(),
                "1.20.6" => v1_20_6::Profile::rules(),
                "1.21" => v1_21::Profile::rules(),
                "1.21.1" => v1_21_1::Profile::rules(),
                "1.21.2" => v1_21_2::Profile::rules(),
                "1.21.3" => v1_21_3::Profile::rules(),
                "1.21.4" => v1_21_4::Profile::rules(),
                "1.21.5" => v1_21_5::Profile::rules(),
                "1.21.6" => v1_21_6::Profile::rules(),
                "1.21.7" => v1_21_7::Profile::rules(),
                "1.21.8" => v1_21_8::Profile::rules(),
                "1.21.9" => v1_21_9::Profile::rules(),
                "1.21.10" => v1_21_10::Profile::rules(),
                "1.21.11" => v1_21_11::Profile::rules(),
                "26.1" => v26_1::Profile::rules(),
                "26.1.1" => v26_1_1::Profile::rules(),
                "26.1.2" => v26_1_2::Profile::rules(),
                "26.2" => v26_2::Profile::rules(),
                "26.3" => v26_3::Profile::rules(),
                _ => unreachable!(),
            };
            assert_eq!(
                contract.fluid_travel_split, rules.fluid_travel_split,
                "{version}"
            );
            assert_eq!(
                contract.fluid_falling_checks_gravity_argument,
                rules.fluid_falling_checks_gravity_argument,
                "{version}"
            );
            assert_eq!(
                contract.water_movement_efficiency_attribute,
                rules.water_movement_efficiency_attribute,
                "{version}"
            );
            assert_eq!(
                contract.water_movement_efficiency_divisor, rules.water_movement_efficiency_divisor,
                "{version}"
            );
            let landing_rounding = match version {
                "1.8.8" => <v1_8_8::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.9.4" => <v1_9_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.10.2" => <v1_10_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.11.2" => <v1_11_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.12.2" => <v1_12_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.13.2" => <v1_13_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.14.4" => <v1_14_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.15" => <v1_15::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.15.1" => <v1_15_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.15.2" => <v1_15_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.16" => <v1_16::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.16.1" => <v1_16_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.16.2" => <v1_16_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.16.3" => <v1_16_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.16.4" => <v1_16_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.16.5" => <v1_16_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.17" => <v1_17::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.17.1" => <v1_17_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.18" => <v1_18::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.18.1" => <v1_18_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.18.2" => <v1_18_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.19" => <v1_19::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.19.1" => <v1_19_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.19.2" => <v1_19_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.19.3" => <v1_19_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.19.4" => <v1_19_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20" => <v1_20::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20.1" => <v1_20_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20.2" => <v1_20_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20.3" => <v1_20_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20.4" => <v1_20_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20.5" => <v1_20_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.20.6" => <v1_20_6::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21" => <v1_21::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.1" => <v1_21_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.2" => <v1_21_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.3" => <v1_21_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.4" => <v1_21_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.5" => <v1_21_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.6" => <v1_21_6::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.7" => <v1_21_7::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.8" => <v1_21_8::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.9" => <v1_21_9::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "1.21.10" => {
                    <v1_21_10::Profile as VersionPhysicsProfile>::landing_damage_rounding()
                }
                "1.21.11" => {
                    <v1_21_11::Profile as VersionPhysicsProfile>::landing_damage_rounding()
                }
                "26.1" => <v26_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "26.1.1" => <v26_1_1::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "26.1.2" => <v26_1_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "26.2" => <v26_2::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                "26.3" => <v26_3::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
                _ => unreachable!(),
            };
            assert_eq!(
                contract.landing_damage_rounding, landing_rounding,
                "{version}"
            );
        }
    }

    #[test]
    fn version_dispatch_uses_owned_air_drag_policy() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 2.0, 0.5),
            velocity: crate::math::Vec3::new(1.0, 0.0, 0.0),
            ..SimulationState::default()
        };

        dispatch("1.14.4", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!((state.velocity.x - crate::math::java_float(0.91)).abs() < 1.0e-12);
    }

    #[test]
    fn v26_2_collision_cancels_the_collided_horizontal_component() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            crate::world::BlockState {
                shape: Some(crate::world::BlockShape::full_block()),
                ..crate::world::BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.0, 0.5),
            velocity: crate::math::Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            controlled_by_local_instance: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.collided_horizontally);
        assert_eq!(state.velocity.x, 0.0);
    }

    #[test]
    fn v26_2_exposes_the_block_fall_callback_boundary_on_downward_clip() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            crate::world::BlockState {
                shape: Some(crate::world::BlockShape::full_block()),
                ..crate::world::BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 2.0, 0.5),
            velocity: crate::math::Vec3::new(0.0, -2.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            controlled_by_local_instance: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.collided_vertically);
        assert!(state.vertical_collision_below);
        assert!(state.block_fall_callback);

        state.position = crate::math::Vec3::new(0.5, 2.0, 0.5);
        state.velocity = crate::math::Vec3::new(0.0, -2.0, 0.0);
        state.controlled_by_local_instance = false;
        state.on_ground = false;
        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();
        assert!(state.collided_vertically);
        assert!(state.vertical_collision_below);
        assert!(!state.block_fall_callback);
    }

    #[test]
    fn v26_2_uses_explicit_bounciness_for_horizontal_restitution() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            crate::world::BlockState {
                shape: Some(crate::world::BlockShape::full_block()),
                ..crate::world::BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.0, 0.5),
            velocity: crate::math::Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            entity_bounciness: 0.5,
            no_gravity: true,
            ..SimulationState::default()
        };

        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.collided_horizontally);
        assert!(state.velocity.x < 0.0);
        assert!((state.velocity.x + 0.5 * crate::math::java_float(0.91)).abs() < 1.0e-12);

        let mut patch_state = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.0, 0.5),
            velocity: crate::math::Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            entity_bounciness: 0.5,
            no_gravity: true,
            ..SimulationState::default()
        };
        dispatch("26.2", &mut patch_state, &world, PhysicsConfig::default()).unwrap();
        assert!(patch_state.collided_horizontally);
        assert!((patch_state.velocity.x + 0.5 * crate::math::java_float(0.91)).abs() < 1.0e-12);

        let mut pre_restitution_state = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.0, 0.5),
            velocity: crate::math::Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            entity_bounciness: 0.5,
            no_gravity: true,
            ..SimulationState::default()
        };
        dispatch(
            "26.1.1",
            &mut pre_restitution_state,
            &world,
            PhysicsConfig::default(),
        )
        .unwrap();
        assert!(pre_restitution_state.collided_horizontally);
        assert_eq!(pre_restitution_state.velocity.x, 0.0);
    }

    #[test]
    fn v26_2_reads_block_bounciness_from_world_state() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            crate::world::BlockState {
                shape: Some(crate::world::BlockShape::full_block()),
                bounce_restitution: 1.0,
                ..crate::world::BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 1.0, 0.5),
            velocity: crate::math::Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };

        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.collided_vertically);
        let non_living_restitution = f64::from(0.8_f32);
        assert!(
            (state.velocity.y - non_living_restitution * crate::math::java_float(0.98)).abs()
                < 1.0e-12
        );
    }

    #[test]
    fn v26_2_honors_source_block_bounce_suppression_boundary() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            crate::world::BlockState {
                shape: Some(crate::world::BlockShape::full_block()),
                bounce_restitution: 1.0,
                suppresses_bounce: true,
                ..crate::world::BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 1.0, 0.5),
            velocity: crate::math::Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };

        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.collided_vertically);
        assert_eq!(state.velocity.y, 0.0);
    }

    #[test]
    fn v26_2_does_not_apply_block_bounce_to_horizontal_collision() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            crate::world::BlockState {
                shape: Some(crate::world::BlockShape::full_block()),
                bounce_restitution: 1.0,
                ..crate::world::BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.0, 0.5),
            velocity: crate::math::Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };

        dispatch("26.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.collided_horizontally);
        assert_eq!(state.velocity.x, 0.0);
    }

    #[test]
    fn fluid_travel_handler_split_matches_source_breakpoint() {
        assert!(!v1_21_1::Profile::rules().fluid_travel_split);
        assert!(v1_21_4::Profile::rules().fluid_travel_split);
        assert!(v1_21_8::Profile::rules().fluid_travel_split);
        assert!(v1_21_11::Profile::rules().fluid_travel_split);
    }

    #[test]
    fn fluid_falling_gravity_condition_matches_source_breakpoint() {
        for rules in [
            v1_14_4::Profile::rules(),
            v1_15::Profile::rules(),
            v1_16_5::Profile::rules(),
            v1_17_1::Profile::rules(),
            v1_18_2::Profile::rules(),
            v1_19::Profile::rules(),
            v1_19_4::Profile::rules(),
            v1_20_4::Profile::rules(),
        ] {
            assert!(!rules.fluid_falling_checks_gravity_argument);
        }
        for rules in [
            v1_20_5::Profile::rules(),
            v1_20_6::Profile::rules(),
            v1_21_1::Profile::rules(),
            v1_21_4::Profile::rules(),
            v1_21_11::Profile::rules(),
            v26_2::Profile::rules(),
        ] {
            assert!(rules.fluid_falling_checks_gravity_argument);
        }
    }

    #[test]
    fn fluid_falling_adjustment_needs_gravity_and_from_1_20_5_a_nonzero_one() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            crate::world::BlockState {
                fluid: crate::world::FluidState::Water,
                ..crate::world::BlockState::default()
            },
        );
        let sinking = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.5, 0.5),
            velocity: crate::math::Vec3::new(0.0, -0.0035, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let vertical = |version: &str, mut state: SimulationState| {
            dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
            state.velocity.y
        };
        let weightless = SimulationState {
            gravity: Some(0.0),
            ..sinking
        };
        let floating = SimulationState {
            no_gravity: true,
            ..sinking
        };
        let dragged = -0.0035 * crate::math::java_float(0.8);

        assert_eq!(vertical("1.20.4", weightless), -0.003);
        assert_eq!(vertical("1.20.5", weightless), dragged);
        for version in ["1.18.2", "1.19", "1.21.1"] {
            assert_eq!(vertical(version, floating), dragged, "{version}");
        }
    }

    #[test]
    fn water_efficiency_attribute_ownership_matches_all_registered_profiles() {
        for version in [
            "1.14.4", "1.15", "1.15.1", "1.15.2", "1.16", "1.16.1", "1.16.2", "1.16.3", "1.16.4",
            "1.16.5", "1.17", "1.17.1", "1.18", "1.18.1", "1.18.2", "1.19", "1.19.1", "1.19.2",
            "1.19.3", "1.19.4", "1.20", "1.20.1", "1.20.2", "1.20.3", "1.20.4", "1.20.6",
        ] {
            let rules = match version {
                "1.14.4" => v1_14_4::Profile::rules(),
                "1.15" => v1_15::Profile::rules(),
                "1.15.1" => v1_15_1::Profile::rules(),
                "1.15.2" => v1_15_2::Profile::rules(),
                "1.16" => v1_16::Profile::rules(),
                "1.16.1" => v1_16_1::Profile::rules(),
                "1.16.2" => v1_16_2::Profile::rules(),
                "1.16.3" => v1_16_3::Profile::rules(),
                "1.16.4" => v1_16_4::Profile::rules(),
                "1.16.5" => v1_16_5::Profile::rules(),
                "1.17" => v1_17::Profile::rules(),
                "1.17.1" => v1_17_1::Profile::rules(),
                "1.18" => v1_18::Profile::rules(),
                "1.18.1" => v1_18_1::Profile::rules(),
                "1.18.2" => v1_18_2::Profile::rules(),
                "1.19" => v1_19::Profile::rules(),
                "1.19.1" => v1_19_1::Profile::rules(),
                "1.19.2" => v1_19_2::Profile::rules(),
                "1.19.3" => v1_19_3::Profile::rules(),
                "1.19.4" => v1_19_4::Profile::rules(),
                "1.20" => v1_20::Profile::rules(),
                "1.20.1" => v1_20_1::Profile::rules(),
                "1.20.2" => v1_20_2::Profile::rules(),
                "1.20.3" => v1_20_3::Profile::rules(),
                "1.20.4" => v1_20_4::Profile::rules(),
                "1.20.6" => v1_20_6::Profile::rules(),
                _ => unreachable!(),
            };
            assert!(!rules.water_movement_efficiency_attribute, "{version}");
        }
        for rules in [
            v1_21_1::Profile::rules(),
            v1_21_4::Profile::rules(),
            v1_21_5::Profile::rules(),
            v1_21_8::Profile::rules(),
            v1_21_10::Profile::rules(),
            v1_21_11::Profile::rules(),
        ] {
            assert!(rules.water_movement_efficiency_attribute);
        }
    }

    #[test]
    fn all_supported_profiles_have_explicit_water_breakpoint_policies() {
        for (version, expected_attribute, expected_divisor, expected_fluid_falling) in [
            ("1.14.4", false, 3.0, false),
            ("1.15", false, 3.0, false),
            ("1.15.1", false, 3.0, false),
            ("1.15.2", false, 3.0, false),
            ("1.16", false, 3.0, false),
            ("1.16.1", false, 3.0, false),
            ("1.16.2", false, 3.0, false),
            ("1.16.3", false, 3.0, false),
            ("1.16.4", false, 3.0, false),
            ("1.16.5", false, 3.0, false),
            ("1.17", false, 3.0, false),
            ("1.17.1", false, 3.0, false),
            ("1.18", false, 3.0, false),
            ("1.18.1", false, 3.0, false),
            ("1.18.2", false, 3.0, false),
            ("1.19", false, 3.0, false),
            ("1.19.1", false, 3.0, false),
            ("1.19.2", false, 3.0, false),
            ("1.19.3", false, 3.0, false),
            ("1.19.4", false, 3.0, false),
            ("1.20", false, 3.0, false),
            ("1.20.1", false, 3.0, false),
            ("1.20.2", false, 3.0, false),
            ("1.20.3", false, 3.0, false),
            ("1.20.4", false, 3.0, false),
            ("1.20.6", false, 3.0, true),
            ("1.21.1", true, 1.0, true),
            ("1.21.4", true, 1.0, true),
            ("1.21.5", true, 1.0, true),
            ("1.21.8", true, 1.0, true),
            ("1.21.10", true, 1.0, true),
            ("1.21.11", true, 1.0, true),
        ] {
            let rules = match version {
                "1.14.4" => v1_14_4::Profile::rules(),
                "1.15" => v1_15::Profile::rules(),
                "1.15.1" => v1_15_1::Profile::rules(),
                "1.15.2" => v1_15_2::Profile::rules(),
                "1.16" => v1_16::Profile::rules(),
                "1.16.1" => v1_16_1::Profile::rules(),
                "1.16.2" => v1_16_2::Profile::rules(),
                "1.16.3" => v1_16_3::Profile::rules(),
                "1.16.4" => v1_16_4::Profile::rules(),
                "1.16.5" => v1_16_5::Profile::rules(),
                "1.17" => v1_17::Profile::rules(),
                "1.17.1" => v1_17_1::Profile::rules(),
                "1.18" => v1_18::Profile::rules(),
                "1.18.1" => v1_18_1::Profile::rules(),
                "1.18.2" => v1_18_2::Profile::rules(),
                "1.19" => v1_19::Profile::rules(),
                "1.19.1" => v1_19_1::Profile::rules(),
                "1.19.2" => v1_19_2::Profile::rules(),
                "1.19.3" => v1_19_3::Profile::rules(),
                "1.19.4" => v1_19_4::Profile::rules(),
                "1.20" => v1_20::Profile::rules(),
                "1.20.1" => v1_20_1::Profile::rules(),
                "1.20.2" => v1_20_2::Profile::rules(),
                "1.20.3" => v1_20_3::Profile::rules(),
                "1.20.4" => v1_20_4::Profile::rules(),
                "1.20.6" => v1_20_6::Profile::rules(),
                "1.21.1" => v1_21_1::Profile::rules(),
                "1.21.4" => v1_21_4::Profile::rules(),
                "1.21.5" => v1_21_5::Profile::rules(),
                "1.21.8" => v1_21_8::Profile::rules(),
                "1.21.10" => v1_21_10::Profile::rules(),
                "1.21.11" => v1_21_11::Profile::rules(),
                _ => unreachable!(),
            };
            assert_eq!(
                rules.water_movement_efficiency_attribute, expected_attribute,
                "{version}"
            );
            assert_eq!(
                rules.water_movement_efficiency_divisor, expected_divisor,
                "{version}"
            );
            assert_eq!(
                rules.fluid_falling_checks_gravity_argument, expected_fluid_falling,
                "{version}"
            );
        }
    }

    #[test]
    fn v1_19_profile_matches_living_entity_travel_literals() {
        let rules = v1_19::Profile::rules();
        assert_eq!(rules.gravity, 0.08);
        assert_eq!(rules.water_gravity_divisor, 16.0);
        assert_eq!(rules.water_input_speed, 0.02);
        assert_eq!(rules.water_drag, 0.8);
        assert_eq!(rules.water_sprint_drag, 0.9);
        assert_eq!(rules.fluid_vertical_drag, 0.8);
        assert_eq!(rules.water_movement_efficiency_divisor, 3.0);
        assert!(!rules.water_movement_efficiency_attribute);
        assert!(!rules.fluid_travel_split);
    }

    #[test]
    fn entity_pushthrough_ownership_matches_source_breakpoint() {
        assert!(entity_pushthrough_enabled("1.8.8").unwrap());
        assert!(entity_pushthrough_enabled("1.14.4").unwrap());
        assert!(entity_pushthrough_enabled("1.16.5").unwrap());
        assert!(!entity_pushthrough_enabled("1.17.1").unwrap());
        assert!(!entity_pushthrough_enabled("1.21.4").unwrap());
        assert!(!entity_pushthrough_enabled("26.1.2").unwrap());
        assert!(entity_pushthrough_enabled("1.22.1").is_err());
    }

    #[test]
    fn legacy_and_split_fluid_handlers_preserve_source_equivalent_motion() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            crate::world::BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_source: true,
                ..crate::world::BlockState::default()
            },
        );
        let initial = SimulationState {
            position: crate::math::Vec3::new(0.5, 0.2, 0.5),
            velocity: crate::math::Vec3::new(0.03, -0.04, 0.01),
            half_width: 0.3,
            height: 1.8,
            in_water: true,
            ..SimulationState::default()
        };
        let mut legacy = initial;
        let mut split = initial;
        dispatch("1.21.1", &mut legacy, &world, PhysicsConfig::default()).unwrap();
        dispatch("1.21.4", &mut split, &world, PhysicsConfig::default()).unwrap();
        assert_eq!(legacy, split);
    }
}
