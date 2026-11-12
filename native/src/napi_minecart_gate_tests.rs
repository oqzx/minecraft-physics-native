//! B290 fixtures for the gates of napi_minecart.rs's private functions: a child module of it (see gate_fixtures_tests.rs).

use super::*;

/// A cart under -64 with no level bottom given: before 1.18 the bottom is 0 whatever the level (1.17.1 DimensionType.java:90), so
/// none is needed; from 1.18 a level can go that low (1.18 DimensionType.java:89), so the bottom is asked for.
#[test]
fn a_cart_under_64_needs_the_level_bottom_from_1_18() {
    let rules = NativeMinecartRules {
        improvements: false,
        max_minecart_speed: None,
        min_y: None,
        tnt_explodes: None,
        nether_exit: None,
        spawner_blocks_enabled: None,
        level_random: None,
    };
    let cart = MinecartState { position: Vec3::new(0.5, -70.0, 0.5), ..MinecartState::default() };
    assert!(rules_of("1.17.1", rules, &[cart]).is_ok());
    assert!(rules_of("1.18", rules, &[cart]).is_err());
}
