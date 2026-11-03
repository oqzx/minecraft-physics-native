use crate::simulation::{
    LandingDamageRounding, PhysicsConfig, SimulationState, TickResult, VersionPhysicsProfile,
};
use crate::version::MinecraftVersion;
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

impl VersionBehaviorContract {
    /// The fluid, water-efficiency and fall-damage policy the vanilla sources fix for a release,
    /// from its version alone (not from its profile, which dispatch checks against it):
    /// - LivingEntity.travel splits into travelInFluid / travelInAir from 1.21.2 (none in
    ///   origin/client1.21.1 LivingEntity.java; origin/client1.21.2 LivingEntity.java).
    /// - getFluidFallingAdjustedMovement tests its gravity argument (`gravity != 0.0`) from
    ///   1.20.5, `!hasNoGravity()` before (1.20.4 LivingEntity.java:2217, 1.20.5 :2288).
    /// - WATER_MOVEMENT_EFFICIENCY replaces the Depth Strider level over 3.0F from 1.21
    ///   (1.20.5 LivingEntity.java:2122-2133, 1.21 LivingEntity.java:322, 2108).
    /// - calculateFallDamage rounds up until 1.21.4 and down from 1.21.5
    ///   (origin/client1.21.4 LivingEntity.java:1627, origin/client1.21.5 :1648).
    #[must_use]
    pub const fn for_release(release: MinecraftVersion) -> Self {
        let attribute = release.at_least(1, 21, 0);
        Self {
            fluid_travel_split: release.at_least(1, 21, 2),
            fluid_falling_checks_gravity_argument: release.at_least(1, 20, 5),
            water_movement_efficiency_attribute: attribute,
            water_movement_efficiency_divisor: if attribute { 1.0 } else { 3.0 },
            landing_damage_rounding: if release.at_least(1, 21, 5) {
                LandingDamageRounding::Floor
            } else {
                LandingDamageRounding::Ceil
            },
        }
    }

    /// Whether the profile is for this release and follows its contract.
    fn is_met_by<P: VersionPhysicsProfile>(version: &str) -> bool {
        let rules = P::rules();
        MinecraftVersion::parse(version) == Some(rules.release)
            && Self::for_release(rules.release)
                == Self {
                    fluid_travel_split: rules.fluid_travel_split,
                    fluid_falling_checks_gravity_argument: rules
                        .fluid_falling_checks_gravity_argument,
                    water_movement_efficiency_attribute: rules.water_movement_efficiency_attribute,
                    water_movement_efficiency_divisor: rules.water_movement_efficiency_divisor,
                    landing_damage_rounding: P::landing_damage_rounding(),
                }
    }
}

/// A release's tick, accepted only when its profile follows the release's behaviour contract.
fn tick_conforming<E: VersionEngine, P: VersionPhysicsProfile>(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> Result<TickResult, String> {
    if !VersionBehaviorContract::is_met_by::<P>(E::VERSION) {
        return Err(format!(
            "the {} profile breaks its behaviour contract",
            E::VERSION
        ));
    }
    Ok(E::tick(state, world, config))
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

/// Every release with a profile, oldest first (the names dispatch accepts).
pub const SUPPORTED_VERSIONS: &[&str] = &[
    v1_8_8::Engine::VERSION,
    v1_9_4::Engine::VERSION,
    v1_10_2::Engine::VERSION,
    v1_11_2::Engine::VERSION,
    v1_12_2::Engine::VERSION,
    v1_13_2::Engine::VERSION,
    v1_14_4::Engine::VERSION,
    v1_15::Engine::VERSION,
    v1_15_1::Engine::VERSION,
    v1_15_2::Engine::VERSION,
    v1_16::Engine::VERSION,
    v1_16_1::Engine::VERSION,
    v1_16_2::Engine::VERSION,
    v1_16_3::Engine::VERSION,
    v1_16_4::Engine::VERSION,
    v1_16_5::Engine::VERSION,
    v1_17::Engine::VERSION,
    v1_17_1::Engine::VERSION,
    v1_18::Engine::VERSION,
    v1_18_1::Engine::VERSION,
    v1_18_2::Engine::VERSION,
    v1_19::Engine::VERSION,
    v1_19_1::Engine::VERSION,
    v1_19_2::Engine::VERSION,
    v1_19_3::Engine::VERSION,
    v1_19_4::Engine::VERSION,
    v1_20::Engine::VERSION,
    v1_20_1::Engine::VERSION,
    v1_20_2::Engine::VERSION,
    v1_20_3::Engine::VERSION,
    v1_20_4::Engine::VERSION,
    v1_20_5::Engine::VERSION,
    v1_20_6::Engine::VERSION,
    v1_21::Engine::VERSION,
    v1_21_1::Engine::VERSION,
    v1_21_2::Engine::VERSION,
    v1_21_3::Engine::VERSION,
    v1_21_4::Engine::VERSION,
    v1_21_5::Engine::VERSION,
    v1_21_6::Engine::VERSION,
    v1_21_7::Engine::VERSION,
    v1_21_8::Engine::VERSION,
    v1_21_9::Engine::VERSION,
    v1_21_10::Engine::VERSION,
    v1_21_11::Engine::VERSION,
    v26_1::Engine::VERSION,
    v26_1_1::Engine::VERSION,
    v26_1_2::Engine::VERSION,
    v26_2::Engine::VERSION,
    v26_3::Engine::VERSION,
];

/// The physics rules of a supported release's profile.
pub fn rules_for_version(version: &str) -> Option<crate::simulation::VersionPhysicsRules> {
    match version {
        v1_8_8::Engine::VERSION => Some(v1_8_8::Profile::rules()),
        v1_9_4::Engine::VERSION => Some(v1_9_4::Profile::rules()),
        v1_10_2::Engine::VERSION => Some(v1_10_2::Profile::rules()),
        v1_11_2::Engine::VERSION => Some(v1_11_2::Profile::rules()),
        v1_12_2::Engine::VERSION => Some(v1_12_2::Profile::rules()),
        v1_13_2::Engine::VERSION => Some(v1_13_2::Profile::rules()),
        v1_14_4::Engine::VERSION => Some(v1_14_4::Profile::rules()),
        v1_15::Engine::VERSION => Some(v1_15::Profile::rules()),
        v1_15_1::Engine::VERSION => Some(v1_15_1::Profile::rules()),
        v1_15_2::Engine::VERSION => Some(v1_15_2::Profile::rules()),
        v1_16::Engine::VERSION => Some(v1_16::Profile::rules()),
        v1_16_1::Engine::VERSION => Some(v1_16_1::Profile::rules()),
        v1_16_2::Engine::VERSION => Some(v1_16_2::Profile::rules()),
        v1_16_3::Engine::VERSION => Some(v1_16_3::Profile::rules()),
        v1_16_4::Engine::VERSION => Some(v1_16_4::Profile::rules()),
        v1_16_5::Engine::VERSION => Some(v1_16_5::Profile::rules()),
        v1_17::Engine::VERSION => Some(v1_17::Profile::rules()),
        v1_17_1::Engine::VERSION => Some(v1_17_1::Profile::rules()),
        v1_18::Engine::VERSION => Some(v1_18::Profile::rules()),
        v1_18_1::Engine::VERSION => Some(v1_18_1::Profile::rules()),
        v1_18_2::Engine::VERSION => Some(v1_18_2::Profile::rules()),
        v1_19::Engine::VERSION => Some(v1_19::Profile::rules()),
        v1_19_1::Engine::VERSION => Some(v1_19_1::Profile::rules()),
        v1_19_2::Engine::VERSION => Some(v1_19_2::Profile::rules()),
        v1_19_3::Engine::VERSION => Some(v1_19_3::Profile::rules()),
        v1_19_4::Engine::VERSION => Some(v1_19_4::Profile::rules()),
        v1_20::Engine::VERSION => Some(v1_20::Profile::rules()),
        v1_20_1::Engine::VERSION => Some(v1_20_1::Profile::rules()),
        v1_20_2::Engine::VERSION => Some(v1_20_2::Profile::rules()),
        v1_20_3::Engine::VERSION => Some(v1_20_3::Profile::rules()),
        v1_20_4::Engine::VERSION => Some(v1_20_4::Profile::rules()),
        v1_20_5::Engine::VERSION => Some(v1_20_5::Profile::rules()),
        v1_20_6::Engine::VERSION => Some(v1_20_6::Profile::rules()),
        v1_21::Engine::VERSION => Some(v1_21::Profile::rules()),
        v1_21_1::Engine::VERSION => Some(v1_21_1::Profile::rules()),
        v1_21_2::Engine::VERSION => Some(v1_21_2::Profile::rules()),
        v1_21_3::Engine::VERSION => Some(v1_21_3::Profile::rules()),
        v1_21_4::Engine::VERSION => Some(v1_21_4::Profile::rules()),
        v1_21_5::Engine::VERSION => Some(v1_21_5::Profile::rules()),
        v1_21_6::Engine::VERSION => Some(v1_21_6::Profile::rules()),
        v1_21_7::Engine::VERSION => Some(v1_21_7::Profile::rules()),
        v1_21_8::Engine::VERSION => Some(v1_21_8::Profile::rules()),
        v1_21_9::Engine::VERSION => Some(v1_21_9::Profile::rules()),
        v1_21_10::Engine::VERSION => Some(v1_21_10::Profile::rules()),
        v1_21_11::Engine::VERSION => Some(v1_21_11::Profile::rules()),
        v26_1::Engine::VERSION => Some(v26_1::Profile::rules()),
        v26_1_1::Engine::VERSION => Some(v26_1_1::Profile::rules()),
        v26_1_2::Engine::VERSION => Some(v26_1_2::Profile::rules()),
        v26_2::Engine::VERSION => Some(v26_2::Profile::rules()),
        v26_3::Engine::VERSION => Some(v26_3::Profile::rules()),
        _ => None,
    }
}

pub fn dispatch(
    version: &str,
    state: &mut SimulationState,
    world: &WorldSnapshot,
    config: PhysicsConfig,
) -> Result<TickResult, String> {
    match version {
        v1_8_8::Engine::VERSION => {
            tick_conforming::<v1_8_8::Engine, v1_8_8::Profile>(state, world, config)
        }
        v1_9_4::Engine::VERSION => {
            tick_conforming::<v1_9_4::Engine, v1_9_4::Profile>(state, world, config)
        }
        v1_10_2::Engine::VERSION => {
            tick_conforming::<v1_10_2::Engine, v1_10_2::Profile>(state, world, config)
        }
        v1_11_2::Engine::VERSION => {
            tick_conforming::<v1_11_2::Engine, v1_11_2::Profile>(state, world, config)
        }
        v1_12_2::Engine::VERSION => {
            tick_conforming::<v1_12_2::Engine, v1_12_2::Profile>(state, world, config)
        }
        v1_13_2::Engine::VERSION => {
            tick_conforming::<v1_13_2::Engine, v1_13_2::Profile>(state, world, config)
        }
        v1_14_4::Engine::VERSION => {
            tick_conforming::<v1_14_4::Engine, v1_14_4::Profile>(state, world, config)
        }
        v1_15::Engine::VERSION => {
            tick_conforming::<v1_15::Engine, v1_15::Profile>(state, world, config)
        }
        v1_15_1::Engine::VERSION => {
            tick_conforming::<v1_15_1::Engine, v1_15_1::Profile>(state, world, config)
        }
        v1_15_2::Engine::VERSION => {
            tick_conforming::<v1_15_2::Engine, v1_15_2::Profile>(state, world, config)
        }
        v1_16::Engine::VERSION => {
            tick_conforming::<v1_16::Engine, v1_16::Profile>(state, world, config)
        }
        v1_16_1::Engine::VERSION => {
            tick_conforming::<v1_16_1::Engine, v1_16_1::Profile>(state, world, config)
        }
        v1_16_2::Engine::VERSION => {
            tick_conforming::<v1_16_2::Engine, v1_16_2::Profile>(state, world, config)
        }
        v1_16_3::Engine::VERSION => {
            tick_conforming::<v1_16_3::Engine, v1_16_3::Profile>(state, world, config)
        }
        v1_16_4::Engine::VERSION => {
            tick_conforming::<v1_16_4::Engine, v1_16_4::Profile>(state, world, config)
        }
        v1_16_5::Engine::VERSION => {
            tick_conforming::<v1_16_5::Engine, v1_16_5::Profile>(state, world, config)
        }
        v1_17::Engine::VERSION => {
            tick_conforming::<v1_17::Engine, v1_17::Profile>(state, world, config)
        }
        v1_17_1::Engine::VERSION => {
            tick_conforming::<v1_17_1::Engine, v1_17_1::Profile>(state, world, config)
        }
        v1_18::Engine::VERSION => {
            tick_conforming::<v1_18::Engine, v1_18::Profile>(state, world, config)
        }
        v1_18_1::Engine::VERSION => {
            tick_conforming::<v1_18_1::Engine, v1_18_1::Profile>(state, world, config)
        }
        v1_18_2::Engine::VERSION => {
            tick_conforming::<v1_18_2::Engine, v1_18_2::Profile>(state, world, config)
        }
        v1_19::Engine::VERSION => {
            tick_conforming::<v1_19::Engine, v1_19::Profile>(state, world, config)
        }
        v1_19_1::Engine::VERSION => {
            tick_conforming::<v1_19_1::Engine, v1_19_1::Profile>(state, world, config)
        }
        v1_19_2::Engine::VERSION => {
            tick_conforming::<v1_19_2::Engine, v1_19_2::Profile>(state, world, config)
        }
        v1_19_3::Engine::VERSION => {
            tick_conforming::<v1_19_3::Engine, v1_19_3::Profile>(state, world, config)
        }
        v1_19_4::Engine::VERSION => {
            tick_conforming::<v1_19_4::Engine, v1_19_4::Profile>(state, world, config)
        }
        v1_20::Engine::VERSION => {
            tick_conforming::<v1_20::Engine, v1_20::Profile>(state, world, config)
        }
        v1_20_1::Engine::VERSION => {
            tick_conforming::<v1_20_1::Engine, v1_20_1::Profile>(state, world, config)
        }
        v1_20_2::Engine::VERSION => {
            tick_conforming::<v1_20_2::Engine, v1_20_2::Profile>(state, world, config)
        }
        v1_20_3::Engine::VERSION => {
            tick_conforming::<v1_20_3::Engine, v1_20_3::Profile>(state, world, config)
        }
        v1_20_4::Engine::VERSION => {
            tick_conforming::<v1_20_4::Engine, v1_20_4::Profile>(state, world, config)
        }
        v1_20_5::Engine::VERSION => {
            tick_conforming::<v1_20_5::Engine, v1_20_5::Profile>(state, world, config)
        }
        v1_20_6::Engine::VERSION => {
            tick_conforming::<v1_20_6::Engine, v1_20_6::Profile>(state, world, config)
        }
        v1_21::Engine::VERSION => {
            tick_conforming::<v1_21::Engine, v1_21::Profile>(state, world, config)
        }
        v1_21_1::Engine::VERSION => {
            tick_conforming::<v1_21_1::Engine, v1_21_1::Profile>(state, world, config)
        }
        v1_21_2::Engine::VERSION => {
            tick_conforming::<v1_21_2::Engine, v1_21_2::Profile>(state, world, config)
        }
        v1_21_3::Engine::VERSION => {
            tick_conforming::<v1_21_3::Engine, v1_21_3::Profile>(state, world, config)
        }
        v1_21_4::Engine::VERSION => {
            tick_conforming::<v1_21_4::Engine, v1_21_4::Profile>(state, world, config)
        }
        v1_21_5::Engine::VERSION => {
            tick_conforming::<v1_21_5::Engine, v1_21_5::Profile>(state, world, config)
        }
        v1_21_6::Engine::VERSION => {
            tick_conforming::<v1_21_6::Engine, v1_21_6::Profile>(state, world, config)
        }
        v1_21_7::Engine::VERSION => {
            tick_conforming::<v1_21_7::Engine, v1_21_7::Profile>(state, world, config)
        }
        v1_21_8::Engine::VERSION => {
            tick_conforming::<v1_21_8::Engine, v1_21_8::Profile>(state, world, config)
        }
        v1_21_9::Engine::VERSION => {
            tick_conforming::<v1_21_9::Engine, v1_21_9::Profile>(state, world, config)
        }
        v1_21_10::Engine::VERSION => {
            tick_conforming::<v1_21_10::Engine, v1_21_10::Profile>(state, world, config)
        }
        v1_21_11::Engine::VERSION => {
            tick_conforming::<v1_21_11::Engine, v1_21_11::Profile>(state, world, config)
        }
        v26_1::Engine::VERSION => {
            tick_conforming::<v26_1::Engine, v26_1::Profile>(state, world, config)
        }
        v26_1_1::Engine::VERSION => {
            tick_conforming::<v26_1_1::Engine, v26_1_1::Profile>(state, world, config)
        }
        v26_1_2::Engine::VERSION => {
            tick_conforming::<v26_1_2::Engine, v26_1_2::Profile>(state, world, config)
        }
        v26_2::Engine::VERSION => {
            tick_conforming::<v26_2::Engine, v26_2::Profile>(state, world, config)
        }
        v26_3::Engine::VERSION => {
            tick_conforming::<v26_3::Engine, v26_3::Profile>(state, world, config)
        }
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

/// The behaviour contract of a supported release (VersionBehaviorContract::for_release).
pub fn contract_for_version(version: &str) -> Result<VersionBehaviorContract, String> {
    match MinecraftVersion::parse(version) {
        Some(release) if is_supported(version) => Ok(VersionBehaviorContract::for_release(release)),
        _ => Err(format!(
            "unsupported decompiled Minecraft version: {version}"
        )),
    }
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
        // every profile has generated data
        for version in super::SUPPORTED_VERSIONS {
            assert!(crate::generated_data::release(version).is_some(), "no generated data for {version}");
        }
        for release in crate::generated_data::RELEASES {
            let flattened = crate::version::MinecraftVersion::parse(release.version)
                .is_some_and(|version| version.at_least(1, 13, 0));
            for fluid_id in ["water", "lava"] {
                let source = crate::generated_data::fluid(release.version, fluid_id).expect("source fluid");
                let flowing = crate::generated_data::fluid(
                    release.version,
                    &format!("flowing_{fluid_id}"),
                )
                .expect("flowing fluid");
                if flattened {
                    // FlowingFluid: the source has FALLING, the flowing fluid LEVEL 1-8 and FALLING
                    assert_eq!(source.state_count, 2, "{} source state count", release.version);
                    assert_eq!(flowing.state_count, 16, "{} flowing state count", release.version);
                    assert_eq!(source.property_names, &["FALLING"]);
                    assert_eq!(flowing.property_names, &["LEVEL", "FALLING"]);
                    assert_eq!(flowing.property_values.len(), 10);
                } else {
                    // the still and flowing liquid blocks: LEVEL 0-15 (1.12.2 BlockLiquid.java LEVEL)
                    for block in [source, flowing] {
                        assert_eq!(block.state_count, 16, "{} {} state count", release.version, block.id);
                        assert_eq!(block.property_names, &["LEVEL"]);
                        assert_eq!(block.property_values.len(), 16);
                    }
                }
            }
            if flattened {
                assert!(crate::generated_data::fluid_tag(release.version, "water").is_some(), "missing water tag for {}", release.version);
                assert!(crate::generated_data::fluid_tag(release.version, "lava").is_some(), "missing lava tag for {}", release.version);
                assert!(!release.block_tags.is_empty(), "missing block tags for {}", release.version);
            } else {
                // tags start in 1.13
                assert!(release.fluid_tags.is_empty() && release.block_tags.is_empty(), "{}", release.version);
            }
            assert!(!release.blocks.is_empty());
            // every block property domain is resolved
            for block in release.blocks {
                assert!(block.state_count.is_some(), "{} {} has an unresolved state", release.version, block.id);
            }
        }
    }

    #[test]
    fn legacy_generated_data_follows_the_legacy_sources() {
        // baseline tick rate and drop-off: water 5 / 1, lava 30 / 2 outside the nether
        // (1.12.2 BlockLiquid.tickRate, BlockDynamicLiquid.updateTick; 1.13.2 WaterFluid / LavaFluid
        // getTickRate and getLevelDecreasePerBlock)
        for version in ["1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2"] {
            for (id, tick, drop) in [("water", 5, 1), ("flowing_water", 5, 1), ("lava", 30, 2), ("flowing_lava", 30, 2)] {
                let fluid = crate::generated_data::fluid(version, id).expect("legacy fluid");
                assert_eq!((fluid.tick_delay, fluid.normal_drop_off), (Some(tick), Some(drop)), "{version} {id}");
            }
        }
        // the numeric-id registry before the flattening, the string one in 1.13.2
        assert_eq!(crate::generated_data::block("1.12.2", "stone").unwrap().state_count, Some(7));
        assert_eq!(crate::generated_data::block("1.12.2", "log").unwrap().property_order, &["VARIANT", "AXIS"]);
        assert!(crate::generated_data::block("1.12.2", "oak_log").is_none());
        assert_eq!(crate::generated_data::block("1.13.2", "oak_log").unwrap().state_count, Some(3));
        assert_eq!(crate::generated_data::block("1.13.2", "redstone_wire").unwrap().state_count, Some(1296));
        // 1.12.2 hopper: FACING filtered by `arg != Direction.UP` in Direction.values() order
        let hopper = crate::generated_data::block("1.12.2", "hopper").unwrap();
        assert_eq!(hopper.properties[0].values, &["down", "north", "south", "west", "east"]);
        // 1.8.8 is read from 1.8.9: red_flower's TYPE is the FlowerType constants of the RED color
        let flower = crate::generated_data::block("1.8.8", "red_flower").unwrap();
        assert_eq!(flower.state_count, Some(9));
        // 1.13.2 tags: FluidTags keys and the data pack's block tag members, nested tags included
        assert!(crate::generated_data::fluid_tag("1.13.2", "water").is_some());
        assert!(crate::generated_data::block_has_tag("1.13.2", "oak_door", "doors"));
        assert!(crate::generated_data::block_has_tag("1.13.2", "iron_door", "doors"));
        assert!(!crate::generated_data::block_has_tag("1.13.2", "stone", "doors"));
        assert!(crate::generated_data::block_tag("1.12.2", "doors").is_none());
        // 1.13.2 VoxelShape provenance: the slab's outline shape depends on TYPE
        let slab = crate::generated_data::block("1.13.2", "stone_slab").unwrap();
        assert!(slab.has_outline_shape_method && slab.outline_shape_state_dependent);
        assert!(slab.outline_shape_property_dependencies.contains(&"TYPE"));
        assert!(!crate::generated_data::block("1.12.2", "stone_slab").unwrap().has_outline_shape_method);
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
    fn tick_snapshots_run_each_releases_own_engine() {
        use crate::math::Vec3;
        use crate::simulation::TickInputSnapshot;
        use crate::world::{BlockShape, BlockState};
        // a fall onto a bouncy block (restitution from 26.2) and onto a plain one with a safe fall
        // distance of 2 (the SAFE_FALL_DISTANCE attribute from 1.20.5): every release's snapshot
        // lands as its versioned engine does
        let landing = |restitution: f64| {
            let mut world = WorldSnapshot::new();
            world.set_block(
                (0, 0, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    bounce_restitution: restitution,
                    ..BlockState::default()
                },
            );
            let state = SimulationState {
                position: Vec3::new(0.5, 1.25, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                fall_distance: 6.0,
                safe_fall_distance: 2.0,
                fall_damage_multiplier: Some(1.0),
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            (world, state)
        };
        macro_rules! assert_releases {
            ($($module:ident),*) => {
                $(for restitution in [0.0, 0.75] {
                    let (world, state) = landing(restitution);
                    // as the scene sets it before the tick (tick_local_player_scene)
                    let mut ticked = SimulationState {
                        minor_horizontal_collision_supported: $module::Profile::rules()
                            .release
                            .supports_minor_horizontal_collision(),
                        ..state
                    };
                    dispatch($module::Engine::VERSION, &mut ticked, &world, PhysicsConfig::default())
                        .unwrap();
                    let output = TickInputSnapshot::for_profile::<$module::Profile>(
                        state,
                        world,
                        PhysicsConfig::default(),
                    )
                    .execute();
                    assert_eq!(output.state, ticked, "{}", $module::Engine::VERSION);
                })*
            };
        }
        assert_releases!(
            v1_8_8, v1_9_4, v1_10_2, v1_11_2, v1_12_2, v1_13_2, v1_14_4, v1_15, v1_15_1, v1_15_2,
            v1_16, v1_16_1, v1_16_2, v1_16_3, v1_16_4, v1_16_5, v1_17, v1_17_1, v1_18, v1_18_1,
            v1_18_2, v1_19, v1_19_1, v1_19_2, v1_19_3, v1_19_4, v1_20, v1_20_1, v1_20_2, v1_20_3,
            v1_20_4, v1_20_5, v1_20_6, v1_21, v1_21_1, v1_21_2, v1_21_3, v1_21_4, v1_21_5, v1_21_6,
            v1_21_7, v1_21_8, v1_21_9, v1_21_10, v1_21_11, v26_1, v26_1_1, v26_1_2, v26_2, v26_3
        );
        // and the policies differ where the engines do: 26.2 bounces, and the fall damage takes
        // the attributes from 1.20.5 (1.20.5 LivingEntity.java:1602-1609)
        let bounced = |version: &str| {
            let (world, mut state) = landing(0.75);
            dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
            state
        };
        assert!(bounced("26.2").velocity.y > 0.0);
        assert!(bounced("26.1.2").velocity.y <= 0.0);
        assert!(v1_20_5::Profile::modern_fall_damage() && v26_1::Profile::modern_fall_damage());
        assert!(!v1_20_4::Profile::modern_fall_damage());
        assert!(v26_2::Profile::restitution_enabled() && !v26_1_2::Profile::restitution_enabled());
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
    fn the_behavior_contract_follows_the_source_breakpoints() {
        let contract = |version: &str| contract_for_version(version).unwrap();
        // travelInFluid / travelInAir from 1.21.2 (origin/client1.21.1 and client1.21.2
        // LivingEntity.java)
        assert!(!contract("1.21.1").fluid_travel_split);
        assert!(contract("1.21.2").fluid_travel_split);
        // `gravity != 0.0` from 1.20.5 (1.20.4 LivingEntity.java:2217, 1.20.5 :2288)
        assert!(!contract("1.20.4").fluid_falling_checks_gravity_argument);
        assert!(contract("1.20.5").fluid_falling_checks_gravity_argument);
        // WATER_MOVEMENT_EFFICIENCY from 1.21, the Depth Strider level over 3.0F before (1.20.5
        // LivingEntity.java:2122-2133, 1.21 LivingEntity.java:322, 2108)
        assert!(!contract("1.20.6").water_movement_efficiency_attribute);
        assert_eq!(contract("1.20.6").water_movement_efficiency_divisor, 3.0);
        assert!(contract("1.21").water_movement_efficiency_attribute);
        assert_eq!(contract("1.21").water_movement_efficiency_divisor, 1.0);
        // calculateFallDamage: Mth.ceil to 1.21.4, Mth.floor from 1.21.5
        // (origin/client1.21.4 LivingEntity.java:1627, origin/client1.21.5 :1648)
        assert_eq!(
            contract("1.21.4").landing_damage_rounding,
            LandingDamageRounding::Ceil
        );
        assert_eq!(
            contract("1.21.5").landing_damage_rounding,
            LandingDamageRounding::Floor
        );
        assert!(contract_for_version("1.22.1").is_err());
    }

    #[test]
    fn dispatch_rejects_a_profile_that_breaks_its_contract() {
        // a 1.21.5 profile that still rounds the fall damage up, as D-004 had it
        struct Ceiling;
        impl VersionPhysicsProfile for Ceiling {
            fn landing_damage_rounding() -> LandingDamageRounding {
                LandingDamageRounding::Ceil
            }
            fn rules() -> crate::simulation::VersionPhysicsRules {
                v1_21_5::Profile::rules()
            }
        }
        let world = WorldSnapshot::new();
        let mut state = SimulationState::default();
        assert!(tick_conforming::<v1_21_5::Engine, Ceiling>(
            &mut state,
            &world,
            PhysicsConfig::default()
        )
        .is_err());
        assert!(tick_conforming::<v1_21_5::Engine, v1_21_5::Profile>(
            &mut state,
            &world,
            PhysicsConfig::default()
        )
        .is_ok());
        // a profile filed under another release
        assert!(!VersionBehaviorContract::is_met_by::<v1_21_5::Profile>("1.21.4"));
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
        // Entity.getBlockBounciness takes the whole restitution for a LivingEntity, which this
        // travel always moves; the 0.8F is for one that is not living (26.3 Entity.java:905-911)
        let living_restitution = 1.0;
        assert!(
            (state.velocity.y - living_restitution * crate::math::java_float(0.98)).abs()
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
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };
        let mut legacy = initial;
        let mut split = initial;
        dispatch("1.21.1", &mut legacy, &world, PhysicsConfig::default()).unwrap();
        dispatch("1.21.4", &mut split, &world, PhysicsConfig::default()).unwrap();
        assert_eq!(legacy, split);
    }
}
