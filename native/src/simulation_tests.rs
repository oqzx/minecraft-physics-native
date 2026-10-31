#[cfg(test)]
mod tests {
    use crate::math::java_float;
    use crate::simulation::{
        back_off_from_edge, entity_collision_context, landing_damage, movement_speed_attribute,
        tick, tick_with_rules, LandingDamageRounding, PhysicsConfig, SimulationState,
        TickInputSnapshot, TickResult, VersionPhysicsProfile, VersionPhysicsRules,
        update_auto_jump,
    };
    use crate::version::MinecraftVersion;
    use crate::world::{
        BlockEffect, BlockShape, BlockState, CollisionQueryEntity, EntityCollider,
        EntityCollisionContext, FluidState, WorldSnapshot,
    };
    use crate::{Aabb, Vec3};

    #[test]
    fn landing_damage_rounding_is_explicit_and_version_selectable() {
        // LivingEntity.computeFallDamage before 1.20.5: ceil((fd - 3.0F - jumpBoost) * mult)
        // (1.20.4 LivingEntity.java:1564), with safe_fall_distance standing for the 3.0F
        assert_eq!(
            landing_damage(4.25, 3.0, 1.0, None, LandingDamageRounding::Ceil),
            2.0
        );
        assert_eq!(
            landing_damage(4.25, 3.0, 1.0, None, LandingDamageRounding::Floor),
            1.0
        );
        assert_eq!(
            landing_damage(3.0, 3.0, 1.0, None, LandingDamageRounding::Ceil),
            0.0
        );
        assert_eq!(
            landing_damage(6.0, 3.0, 1.0, Some(1), LandingDamageRounding::Ceil),
            1.0
        );
    }

    #[test]
    fn landing_damage_formula_follows_the_release() {
        use crate::simulation::attribute_landing_damage;
        use crate::version::MinecraftVersion;
        // 1.20.4 LivingEntity.java:1564: float ceil, Jump Boost subtracted
        assert_eq!(landing_damage(5.0, 3.0, 1.0, None, LandingDamageRounding::Ceil), 2.0);
        assert_eq!(landing_damage(5.0, 3.0, 1.0, Some(0), LandingDamageRounding::Ceil), 1.0);
        // the float subtraction: 3.0000001f - 3.0f is one ulp, ceil 1
        let just_over = f64::from(f32::from_bits(3.0_f32.to_bits() + 1));
        assert_eq!(landing_damage(just_over, 3.0, 1.0, None, LandingDamageRounding::Ceil), 1.0);
        // 1.20.5 LivingEntity.java:1607-1609 (1.21.4 :1662-1664): float, no epsilon, ceil, so an
        // exact excess is not rounded up and fd == safe deals nothing
        let v1_20_5 = MinecraftVersion::new(1, 20, 5);
        let v1_21_4 = MinecraftVersion::new(1, 21, 4);
        assert_eq!(attribute_landing_damage(5.0, 3.0, 1.0, 1.0, LandingDamageRounding::Ceil, v1_20_5), 2.0);
        assert_eq!(attribute_landing_damage(3.0, 3.0, 1.0, 1.0, LandingDamageRounding::Ceil, v1_21_4), 0.0);
        assert_eq!(attribute_landing_damage(4.25, 3.0, 1.0, 1.0, LandingDamageRounding::Ceil, v1_21_4), 2.0);
        assert_eq!(attribute_landing_damage(4.25, 3.0, 1.0, 0.5, LandingDamageRounding::Ceil, v1_21_4), 1.0);
        // 1.21.5 LivingEntity.java:1672-1682: floor((fd + 1.0E-6 - safe) * multiplier)
        let v1_21_5 = MinecraftVersion::new(1, 21, 5);
        assert_eq!(attribute_landing_damage(5.0, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 2.0);
        assert_eq!(attribute_landing_damage(4.9999995, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 2.0);
        assert_eq!(attribute_landing_damage(4.25, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 1.0);
        assert_eq!(attribute_landing_damage(3.0, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 0.0);
    }

    #[test]
    fn release_profiles_pick_the_vanilla_fall_damage_rounding() {
        use crate::simulation::VersionPhysicsProfile;
        // 1.21.4 LivingEntity.java:1664 ceil, 1.21.5 :1678 floor
        assert_eq!(
            <crate::versioned::v1_21_4::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Ceil
        );
        assert_eq!(
            <crate::versioned::v1_21_5::Profile as VersionPhysicsProfile>::landing_damage_rounding(),
            LandingDamageRounding::Floor
        );
    }

    #[test]
    fn fall_distance_accumulates_in_float_before_1_21_5() {
        use crate::simulation::accumulated_fall_distance;
        use crate::version::MinecraftVersion;
        let dy = -0.1;
        // 1.18.1 Entity.java:1019: (float)((double)fd - dy)
        assert_eq!(
            accumulated_fall_distance(0.0, dy, MinecraftVersion::new(1, 18, 1)),
            f64::from(0.1_f32)
        );
        // 1.19 Entity.java:1005: fd -= (float)dy, a float subtraction
        let fd = f64::from(1.3_f32);
        assert_eq!(
            accumulated_fall_distance(fd, dy, MinecraftVersion::new(1, 19, 0)),
            f64::from(1.3_f32 + 0.1_f32)
        );
        assert_eq!(
            accumulated_fall_distance(fd, dy, MinecraftVersion::new(1, 18, 1)),
            f64::from((f64::from(1.3_f32) + 0.1) as f32)
        );
        // 1.21.5 Entity.java:1286: a double field, fd -= (double)(float)dy
        assert_eq!(
            accumulated_fall_distance(1.3, dy, MinecraftVersion::new(1, 21, 5)),
            1.3 + f64::from(0.1_f32)
        );
    }

    #[test]
    fn gravity_moves_state_downward() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            velocity: Vec3::ZERO,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        let result = tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(!result.collided_vertically);
        assert_eq!(state.position.y, 2.0);
        assert_eq!(state.velocity.y, -0.08 * java_float(0.98));

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert_eq!(state.position.y, 2.0 - 0.08 * java_float(0.98));
    }

    #[test]
    fn horizontal_drag_uses_the_ground_state_from_before_the_move() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    slipperiness: 0.98,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.2, 1.05, 0.5),
            velocity: Vec3::new(0.5, -0.1, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.on_ground);
        assert_eq!(state.velocity.x, 0.5 * java_float(0.98));
    }

    #[test]
    fn water_keeps_eight_tenths_of_the_vertical_motion() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        for y in 0..3 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    ..BlockState::default()
                },
            );
        }
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(0.0, 0.1, 0.0),
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.in_water);
        assert_eq!(state.velocity.y, 0.1 * java_float(0.8));
    }

    #[test]
    fn landing_leaves_a_negative_zero_vertical_velocity() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.2, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        let mut restituted = state;

        tick(&mut state, &world, PhysicsConfig::default());
        crate::versioned::dispatch("26.2", &mut restituted, &world, PhysicsConfig::default())
            .expect("26.2 is supported");

        assert!(state.on_ground);
        assert_eq!(state.velocity.y, 0.0);
        assert!(state.velocity.y.is_sign_negative());
        assert!(restituted.on_ground);
        assert_eq!(restituted.velocity.y, 0.0);
        assert!(restituted.velocity.y.is_sign_positive());
    }

    fn ice() -> BlockState {
        BlockState {
            id: 1,
            shape: Some(BlockShape {
                slipperiness: 0.98,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    fn drag_on(world: &WorldSnapshot, position: Vec3, release: MinecraftVersion) -> f64 {
        let mut state = SimulationState {
            position,
            velocity: Vec3::new(0.0, -0.08 * java_float(0.98), 0.1),
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, world, PhysicsConfig::default(), rules);
        state.velocity.z / 0.1
    }

    #[test]
    fn soul_sand_slows_whoever_it_affects_and_soul_soil_slows_no_one() {
        let block = |effect| BlockState {
            id: 1,
            shape: Some(BlockShape {
                effect,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let moved = |effect, position: Vec3| {
            let mut world = WorldSnapshot::new();
            world.set_block((0, 0, 0), block(effect));
            let mut state = SimulationState {
                position,
                velocity: Vec3::new(0.0, 0.0, 0.1),
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release: MinecraftVersion::new(1, 19, 4),
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state.velocity.z
        };
        let hovering = Vec3::new(0.5, 1.2, 0.5);

        assert_eq!(
            moved(BlockEffect::SoulSand, hovering),
            0.1 * java_float(0.4) * java_float(0.98)
        );
        assert_eq!(
            moved(BlockEffect::SoulSoil, hovering),
            0.1 * java_float(0.98)
        );
    }

    #[test]
    fn friction_on_a_slab_comes_from_the_block_beneath_it() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), ice());
        world.set_block(
            (0, 1, 0),
            BlockState {
                id: 2,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let icy = java_float(java_float(0.98) * java_float(0.98));

        for release in [
            MinecraftVersion::new(1, 14, 4),
            MinecraftVersion::new(1, 19, 4),
            MinecraftVersion::new(1, 21, 1),
        ] {
            assert_eq!(drag_on(&world, Vec3::new(0.5, 1.5, 0.5), release), icy);
        }
    }

    #[test]
    fn friction_follows_the_supporting_block_from_1_20() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), ice());
        let over_the_edge = Vec3::new(1.1, 1.0, 0.5);

        assert_eq!(
            drag_on(&world, over_the_edge, MinecraftVersion::new(1, 20, 0)),
            java_float(java_float(0.98) * java_float(0.98))
        );
        assert_eq!(
            drag_on(&world, over_the_edge, MinecraftVersion::new(1, 19, 4)),
            java_float(java_float(0.6) * java_float(0.98))
        );
    }

    #[test]
    fn slow_falling_keeps_full_gravity_while_rising() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            velocity: Vec3::new(0.0, 0.3, 0.0),
            slow_falling: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert_eq!(state.velocity.y, (0.3 - 0.08) * java_float(0.98));
    }

    #[test]
    fn immutable_tick_snapshot_is_reusable_and_does_not_mutate_input() {
        let state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            velocity: Vec3::ZERO,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let input = TickInputSnapshot::new(
            state,
            WorldSnapshot::new(),
            PhysicsConfig::default(),
            VersionPhysicsRules::default(),
            LandingDamageRounding::Ceil,
            false,
            false,
            false,
        );

        let first = input.execute();
        let second = input.execute();

        assert_eq!(first, second);
        assert_eq!(input.state, state);
        assert_eq!(input.state.tick_count, 0);
        assert!(first.state.velocity.y < state.velocity.y);
    }

    #[test]
    fn simulation_config_rejects_non_finite_and_out_of_range_values() {
        let mut config = PhysicsConfig::default();
        config.gravity = f64::NAN;
        assert!(config.validate().is_err());
        config = PhysicsConfig::default();
        config.drag = 2.1;
        assert!(config.validate().is_err());
        assert!(PhysicsConfig::default().validate().is_ok());
    }

    #[test]
    fn ground_input_uses_source_friction_influenced_speed() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    slipperiness: 0.98,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            input: Vec3::new(1.0, 0.0, 0.0),
            on_ground: true,
            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());
        let expected = crate::math::java_float(
            crate::math::java_float(0.1)
                * crate::math::java_float(
                    crate::math::java_float(0.21600002)
                        / crate::math::java_float(0.98_f64 * 0.98 * 0.98),
                ),
        );
        assert!(
            (state.position.x - (0.5 + crate::math::java_float(0.98) * expected)).abs() < 1.0e-7
        );
    }

    #[test]
    fn low_friction_keeps_the_plain_ground_speed_from_26_2() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let walking = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            input: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            friction_modifier: 1.6,
            friction_modifier_present: true,
            ..SimulationState::default()
        };
        let mut modern = walking;
        let mut older = walking;

        crate::versioned::dispatch("26.2", &mut modern, &world, PhysicsConfig::default()).unwrap();
        crate::versioned::dispatch("26.1", &mut older, &world, PhysicsConfig::default()).unwrap();

        let plain_step = f64::from(0.98_f32) * f64::from(0.1_f32);
        assert_eq!(modern.position.x, 0.5 + plain_step);
        assert!(older.position.x > modern.position.x);
    }

    #[test]
    fn input_before_1_14_is_rotated_in_float() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState {
            position: Vec3::new(0.5, 100.0, 0.5),
            input: Vec3::new(0.0, 0.0, 1.0),
            is_player: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch("1.12.2", &mut state, &world, PhysicsConfig::default()).unwrap();
        assert_eq!(state.position.z, 0.5 + f64::from(0.98_f32 * 0.02_f32));
    }

    #[test]
    fn ground_speed_before_1_14_divides_by_the_cubed_inertia() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 99, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        for version in ["1.8.8", "1.12.2", "1.13.2"] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                input: Vec3::new(0.0, 0.0, -1.0),
                on_ground: true,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.velocity.z, -0.05350800074768003, "{version}");
        }
    }

    #[test]
    fn move_flying_before_1_9_turns_the_degrees_into_radians_in_two_roundings() {
        // 1.8.9 Entity.java:942-943: MathHelper.sin(this.yaw * (float) Math.PI / 180.0F); from
        // 1.9.4 Entity.java:1050-1051: this.yaw * (float) (Math.PI / 180.0). At 28.927 degrees
        // the two land on adjacent sine table entries (5266 and 5265).
        let degrees = 28.927_f32;
        let two_roundings = f64::from(0.504_871_43_f32);
        let one_rounding = f64::from(0.504_871_37_f32);
        let release = |version: &str| MinecraftVersion::parse(version).unwrap();
        assert_eq!(crate::simulation::yaw_radians(degrees, release("1.8.8")), two_roundings);
        assert_eq!(crate::simulation::yaw_radians(degrees, release("1.9.4")), one_rounding);
        let world = WorldSnapshot::new();
        let walk = |version: &str, yaw: f64, yaw_degrees: Option<f32>| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                input: Vec3::new(0.0, 0.0, 1.0),
                yaw,
                yaw_degrees,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.velocity
        };
        for (version, radians) in [("1.8.8", two_roundings), ("1.9.4", one_rounding)] {
            assert_eq!(
                walk(version, one_rounding, Some(degrees)),
                walk(version, radians, None),
                "{version}"
            );
        }
        assert_ne!(walk("1.8.8", one_rounding, Some(degrees)), walk("1.8.8", one_rounding, None));
    }

    #[test]
    fn motion_below_0_005_is_negligible_in_1_8() {
        let world = WorldSnapshot::new();
        for (version, moved) in [("1.8.8", 0.0), ("1.9.4", 0.004)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                velocity: Vec3::new(0.004, 0.0, 0.0),
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.x, 0.5 + moved, "{version}");
        }
    }

    #[test]
    fn a_player_over_a_column_not_loaded_sinks_at_a_tenth_before_1_14() {
        let mut world = WorldSnapshot::new();
        world.set_chunk_loaded(0, 0, true);
        let fall = |version: &str, x: f64, y: f64| {
            let mut state = SimulationState {
                position: Vec3::new(x, y, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.velocity.y
        };
        let sinking = -0.1 * java_float(0.98);
        let falling = -0.08 * java_float(0.98);
        assert_eq!(fall("1.8.8", 20.5, 5.0), sinking);
        assert_eq!(fall("1.8.8", 20.5, -5.0), 0.0);
        assert_eq!(fall("1.8.8", 0.5, 5.0), falling);
        assert_eq!(fall("1.13.2", 20.5, 5.0), sinking);
        assert_eq!(fall("1.14.4", 20.5, 5.0), falling);
        assert_eq!(fall("1.8.8", -0.5, 5.0), falling);
        assert_eq!(fall("1.9.4", -0.5, 5.0), sinking);
    }

    #[test]
    fn push_out_of_blocks_before_1_14_tests_the_cell_half_a_block_above_the_feet() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 100, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        for (version, pushed) in [("1.11.2", false), ("1.14.4", true)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 100.9375, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.x != 0.5, pushed, "{version}");
        }
    }

    #[test]
    fn landing_leaves_a_positive_zero_vertical_velocity_before_1_14() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let land = |version| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.2, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .expect("the release is supported");
            assert!(state.on_ground);
            state.velocity.y
        };

        assert!(land("1.8.8") == 0.0 && land("1.8.8").is_sign_positive());
        assert!(land("1.13.2") == 0.0 && land("1.13.2").is_sign_positive());
        assert!(land("1.14.4") == 0.0 && land("1.14.4").is_sign_negative());
    }

    #[test]
    fn no_physics_bypasses_collision_and_clears_collision_flags() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            no_physics: true,
            on_ground: true,
            collided_horizontally: true,
            collided_vertically: true,
            ..SimulationState::default()
        };

        let result = tick(&mut state, &world, PhysicsConfig::default());

        assert!(result.displacement.y < 0.0);
        assert!(state.position.y < 1.0);
        assert!(!state.collided_horizontally);
        assert!(!state.collided_vertically);
    }

    fn position_after_free_move(velocity: Vec3, release: MinecraftVersion) -> Vec3 {
        let mut state = SimulationState {
            position: Vec3::new(-2.0567102507011388, 10.0, 0.5),
            velocity,
            half_width: f64::from(0.3_f32),
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };
        let config = PhysicsConfig {
            negligible_velocity: 0.0,
            ..PhysicsConfig::default()
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, &WorldSnapshot::new(), config, rules);
        state.position
    }

    #[test]
    fn negligible_movement_leaves_the_position_until_1_21_2() {
        let velocity = Vec3::new(2.0e-4, 0.0, 0.0);

        let legacy = position_after_free_move(velocity, MinecraftVersion::new(1, 21, 1));
        let modern = position_after_free_move(velocity, MinecraftVersion::new(1, 21, 2));

        assert_eq!(legacy.x, -2.0567102507011388);
        assert_eq!(modern.x, -2.0567102507011388 + 2.0e-4);
    }

    #[test]
    fn no_physics_keeps_the_collision_flags_before_1_21_9() {
        let collisions_after = |release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.2, -0.5, 0.0),
                is_player: true,
                no_physics: true,
                collided_horizontally: true,
                minor_horizontal_collision: true,
                collided_vertically: true,
                vertical_collision_below: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            [
                state.collided_horizontally,
                state.minor_horizontal_collision,
                state.collided_vertically,
                state.vertical_collision_below,
            ]
        };

        assert_eq!(collisions_after(MinecraftVersion::new(1, 21, 8)), [true; 4]);
        assert_eq!(
            collisions_after(MinecraftVersion::new(1, 21, 9)),
            [false; 4]
        );
    }

    #[test]
    fn no_physics_touches_no_blocks() {
        let mut world = WorldSnapshot::new();
        for (position, effect) in [
            ((1, 1, 0), BlockEffect::SoulSand),
            ((0, 1, 0), BlockEffect::Cactus),
            ((0, 2, 0), BlockEffect::Web),
        ] {
            world.set_block(
                position,
                BlockState {
                    shape: Some(BlockShape {
                        effect,
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
        }
        let mut state = SimulationState {
            position: Vec3::new(1.0, 1.0, 0.5),
            velocity: Vec3::new(0.2, 0.0, 0.0),
            no_physics: true,
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.velocity.x, 0.2 * java_float(0.98));
        assert_eq!(state.stuck_speed_multiplier, Vec3::ZERO);
        assert!(!state.in_web);
        assert_eq!(state.contact_damage, 0.0);
    }

    #[test]
    fn spectators_gather_no_fall_distance() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 10.0, 0.5),
            is_player: true,
            no_physics: true,
            flying: true,
            flying_speed: java_float(0.05),
            sneaking: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.y < 10.0);
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn position_is_read_back_from_the_moved_box_before_1_17() {
        let velocity = Vec3::new(0.1087654312, 0.0, 0.0);

        let legacy = position_after_free_move(velocity, MinecraftVersion::new(1, 16, 5));
        let modern = position_after_free_move(velocity, MinecraftVersion::new(1, 17, 0));

        assert_eq!(legacy.x, -1.9479448195011386);
        assert_eq!(modern.x, -1.9479448195011388);
    }

    #[test]
    fn zero_vertical_motion_preserves_vanilla_vertical_state() {
        let mut state = SimulationState {
            velocity: Vec3::new(0.0, 0.0, 0.0),
            on_ground: true,
            collided_vertically: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.on_ground);
        assert!(state.collided_vertically);
    }

    #[test]
    fn downward_collision_sets_vertical_collision_below() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.collided_vertically);
        assert!(state.vertical_collision_below);
        assert!(state.on_ground);
    }

    #[test]
    fn a_fall_cut_by_less_than_the_equality_tolerance_still_lands() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.000001, 0.5),
            velocity: Vec3::new(0.0, -5.0e-6, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let config = PhysicsConfig {
            negligible_velocity: 0.0,
            ..PhysicsConfig::default()
        };

        tick(&mut state, &world, config);

        assert!(state.collided_vertically);
        assert!(state.on_ground);
    }

    fn velocity_after_move(
        world: &WorldSnapshot,
        state: SimulationState,
        release: MinecraftVersion,
    ) -> Vec3 {
        let mut state = SimulationState {
            half_width: 0.3,
            height: 1.8,
            ..state
        };
        let config = PhysicsConfig {
            negligible_velocity: 0.0,
            ..PhysicsConfig::default()
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, world, config, rules);
        state.velocity
    }

    #[test]
    fn a_z_collision_keeps_the_x_motion_until_1_18_2() {
        let mut world = WorldSnapshot::new();
        for position in [(1, 0, 0), (0, 0, 1)] {
            world.set_block(
                position,
                BlockState {
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        let into_corner = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.5, 0.0, 0.5),
            no_gravity: true,
            ..SimulationState::default()
        };

        let legacy = velocity_after_move(&world, into_corner, MinecraftVersion::new(1, 18, 1));
        let modern = velocity_after_move(&world, into_corner, MinecraftVersion::new(1, 18, 2));

        assert_eq!(legacy.x, 0.5 * java_float(0.98));
        assert_eq!(legacy.z, 0.0);
        assert_eq!(modern.x, 0.0);
        assert_eq!(modern.z, 0.0);
    }

    #[test]
    fn a_negligible_movement_meets_the_world_border_only_from_1_14() {
        let drifting = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.5e-7, 0.0, 0.0),
            no_gravity: true,
            ..SimulationState::default()
        };
        let world = WorldSnapshot::new();

        let boxes = velocity_after_move(&world, drifting, MinecraftVersion::new(1, 12, 2));
        let voxels = velocity_after_move(&world, drifting, MinecraftVersion::new(1, 13, 2));
        let bordered = velocity_after_move(&world, drifting, MinecraftVersion::new(1, 14, 4));

        assert!(boxes.x > 0.0);
        assert!(voxels.x > 0.0);
        assert_eq!(bordered.x, 0.0);
    }

    #[test]
    fn any_cut_of_a_horizontal_movement_is_a_collision_before_1_14() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (-1, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let collides = |release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(0.3, 0.0, 0.5),
                velocity: Vec3::new(-5.0e-6, 0.0, 0.0136),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                ..SimulationState::default()
            };
            let config = PhysicsConfig {
                negligible_velocity: 0.0,
                ..PhysicsConfig::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, config, rules);
            state.collided_horizontally
        };

        assert!(collides(MinecraftVersion::new(1, 13, 2)));
        assert!(!collides(MinecraftVersion::new(1, 14, 4)));
    }

    #[test]
    fn a_corner_collision_stops_both_axes_before_1_14() {
        let mut world = WorldSnapshot::new();
        for position in [(1, 0, 0), (0, 0, 1)] {
            world.set_block(
                position,
                BlockState {
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        let into_corner = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.5, 0.0, 0.5),
            no_gravity: true,
            ..SimulationState::default()
        };

        let velocity = velocity_after_move(&world, into_corner, MinecraftVersion::new(1, 13, 2));

        assert_eq!(velocity.x, 0.0);
        assert_eq!(velocity.z, 0.0);
    }

    #[test]
    fn a_motion_cut_within_the_equality_tolerance_keeps_going_from_1_18_2() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (-1, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let against_wall = SimulationState {
            position: Vec3::new(0.3, 0.0, 0.5),
            velocity: Vec3::new(-5.0e-6, 0.0, 0.0136),
            no_gravity: true,
            ..SimulationState::default()
        };

        let legacy = velocity_after_move(&world, against_wall, MinecraftVersion::new(1, 18, 1));
        let modern = velocity_after_move(&world, against_wall, MinecraftVersion::new(1, 18, 2));

        assert_eq!(legacy.x, 0.0);
        assert!(modern.x < 0.0);
    }

    #[test]
    fn player_minor_horizontal_collision_tracks_small_direction_change() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.9, 0.5),
            velocity: Vec3::new(0.5, 0.0, 0.005),
            input: Vec3::new(1.0, 0.0, 0.01),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            minor_horizontal_collision_supported: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.collided_horizontally);
        assert!(state.minor_horizontal_collision);
    }

    #[test]
    fn minor_horizontal_collision_compares_the_movement_with_the_input_direction() {
        let mut world = WorldSnapshot::new();
        for position in [(0, 0, 1), (1, 0, 1)] {
            world.set_block(
                position,
                BlockState {
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.3, 0.0, 0.25),
            input: Vec3::new(0.0, 0.0, 1.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            minor_horizontal_collision_supported: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        let result = tick(&mut state, &world, PhysicsConfig::default());

        assert!(result.displacement.z < 0.25);
        assert!(state.collided_horizontally);
        assert!(!state.minor_horizontal_collision);
    }

    #[test]
    fn falling_state_lands_on_full_block() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, -1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.2, 0.0),
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        let result = tick(&mut state, &world, PhysicsConfig::default());

        assert!(result.collided_vertically);
        assert!(state.on_ground);
        assert_eq!(state.velocity.y, -0.08 * java_float(0.98));
        assert_eq!(state.position.y, 1.0);
    }

    #[test]
    fn applies_horizontal_input() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.x > 0.0);
    }

    #[test]
    fn movement_input_is_bounded_before_speed_application() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState {
            input: Vec3::new(1.0, 0.0, 1.0),
            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());

        let horizontal_speed =
            (state.velocity.x * state.velocity.x + state.velocity.z * state.velocity.z).sqrt();
        assert!((horizontal_speed - java_float(0.02) * java_float(0.98)).abs() < 1e-12);
    }

    #[test]
    fn speed_slowness_and_sprinting_multiply_the_movement_speed() {
        let base = java_float(0.1);
        let sprint = 1.0 + 0.30000001192092896;
        let speed = 1.0 + 0.20000000298023224;
        let slowness = 1.0 - 0.15000000596046448;

        assert_eq!(
            movement_speed_attribute(base, None, None, true, 1.3, MinecraftVersion::new(1, 21, 1)),
            base * sprint
        );
        assert_eq!(
            movement_speed_attribute(
                base,
                Some(0),
                Some(0),
                true,
                1.3,
                MinecraftVersion::new(1, 21, 1)
            ),
            base * sprint * speed * slowness
        );
        assert_eq!(
            movement_speed_attribute(
                base,
                Some(0),
                Some(0),
                true,
                1.3,
                MinecraftVersion::new(1, 20, 5)
            ),
            base * sprint * slowness * speed
        );
        assert_eq!(
            movement_speed_attribute(
                base,
                Some(0),
                Some(0),
                true,
                1.3,
                MinecraftVersion::new(1, 20, 4)
            ),
            base * slowness * sprint * speed
        );
        assert_eq!(
            movement_speed_attribute(
                base,
                None,
                Some(9),
                false,
                1.3,
                MinecraftVersion::new(1, 21, 1)
            ),
            0.0
        );
    }

    #[test]
    fn slowness_attribute_modifier_is_applied_before_movement_drag() {
        let mut state = SimulationState {
            input: Vec3::new(0.0, 0.0, 1.0),
            on_ground: true,
            no_gravity: true,
            slowness_effect_amplifier: Some(1),
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        // Base 0.1 * (1 - 0.30), then the drag of the default 0.6 friction.
        let ground_drag = java_float(java_float(0.6) * java_float(0.98));
        assert!(
            (state.velocity.z - java_float(0.98) * java_float(0.1 * 0.7) * ground_drag).abs()
                < 1e-12
        );
    }

    #[test]
    fn modern_air_drag_modifier_uses_source_clamped_interpolation() {
        let mut state = SimulationState {
            input: Vec3::new(0.0, 0.0, 1.0),
            no_gravity: true,
            air_drag_modifier: 0.5,
            ..SimulationState::default()
        };

        let mut rules = VersionPhysicsRules::default();
        rules.fluid_travel_split = true;
        tick_with_rules(
            &mut state,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
            rules,
        );

        // clamp(1 - (1 - 0.98) * 0.5, 0, 1) == 0.99, in float.
        let drag = f64::from(1.0_f32 - (1.0_f32 - 0.98_f32) * 0.5_f32);
        assert!((state.velocity.z - java_float(0.98) * java_float(0.02) * drag).abs() < 1e-12);
    }

    #[test]
    fn explicit_zero_air_drag_modifier_is_not_treated_as_missing() {
        let mut state = SimulationState {
            input: Vec3::new(0.0, 0.0, 1.0),
            no_gravity: true,
            air_drag_modifier: 0.0,
            air_drag_modifier_present: true,
            ..SimulationState::default()
        };
        let mut rules = VersionPhysicsRules::default();
        rules.fluid_travel_split = true;

        tick_with_rules(
            &mut state,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
            rules,
        );

        assert!((state.velocity.z - java_float(0.98) * java_float(0.02)).abs() < 1e-12);
    }

    #[test]
    fn modern_friction_modifier_is_applied_to_ground_friction() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            input: Vec3::new(0.0, 0.0, 1.0),
            on_ground: true,
            friction_modifier: 0.5,
            ..SimulationState::default()
        };
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    slipperiness: 0.98,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );

        let mut rules = VersionPhysicsRules::default();
        rules.fluid_travel_split = true;
        tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);

        assert!(state.velocity.z > 0.0);
        assert!(state.velocity.z < 0.1);
    }

    fn airborne_displacement(state: SimulationState, release: MinecraftVersion) -> f64 {
        let mut state = SimulationState {
            input: Vec3::new(1.0, 0.0, 0.0),
            no_gravity: true,
            ..state
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(
            &mut state,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
            rules,
        );
        state.position.x
    }

    #[test]
    fn airborne_players_accelerate_with_the_flying_speed() {
        let player = SimulationState {
            is_player: true,
            ..SimulationState::default()
        };
        let sprinting = SimulationState {
            sprinting: true,
            ..player
        };
        let modern = MinecraftVersion::new(1, 21, 1);

        let input = java_float(0.98);
        assert_eq!(
            airborne_displacement(player, modern),
            input * java_float(0.02)
        );
        assert_eq!(
            airborne_displacement(sprinting, modern),
            input * f64::from(0.025999999_f32)
        );
        assert_eq!(
            airborne_displacement(SimulationState::default(), modern),
            input * java_float(0.02)
        );
    }

    #[test]
    fn airborne_sprint_speed_lags_a_tick_before_1_19_4() {
        let sprinted_before = SimulationState {
            is_player: true,
            was_sprinting: true,
            ..SimulationState::default()
        };
        let started_sprinting = SimulationState {
            is_player: true,
            sprinting: true,
            ..SimulationState::default()
        };

        let input = java_float(0.98);
        assert_eq!(
            airborne_displacement(started_sprinting, MinecraftVersion::new(1, 19, 2)),
            input * java_float(0.02)
        );
        assert_eq!(
            airborne_displacement(sprinted_before, MinecraftVersion::new(1, 19, 2)),
            input * f64::from(0.02_f32 + 0.006_f32)
        );
        assert_eq!(
            airborne_displacement(sprinted_before, MinecraftVersion::new(1, 14, 4)),
            input * f64::from((f64::from(0.02_f32) + 0.005999999865889549) as f32)
        );
    }

    #[test]
    fn creative_flight_doubles_the_flying_speed_while_sprinting() {
        let flying = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: 0.05,
            sprinting: true,
            ..SimulationState::default()
        };

        assert_eq!(
            airborne_displacement(flying, MinecraftVersion::new(1, 21, 1)),
            java_float(0.98) * f64::from(0.05_f32 * 2.0)
        );
    }

    #[test]
    fn sneaking_does_not_slow_flight_in_1_14() {
        let flying = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: java_float(0.05),
            ..SimulationState::default()
        };
        let sneaking = SimulationState {
            sneaking: true,
            moving_slowly: Some(true),
            ..flying
        };
        let spectating = SimulationState {
            sneaking: true,
            moving_slowly: Some(false),
            no_physics: true,
            ..flying
        };
        let release = MinecraftVersion::new(1, 14, 4);

        assert_eq!(
            airborne_displacement(sneaking, release),
            airborne_displacement(flying, release)
        );
        assert_eq!(airborne_displacement(spectating, release), java_float(0.05));
        assert_eq!(
            airborne_displacement(sneaking, MinecraftVersion::new(1, 15, 2)),
            f64::from(0.3_f32 * 0.98_f32) * java_float(0.05)
        );
    }

    #[test]
    fn input_rotation_uses_the_mth_sine_table_of_the_release() {
        for release in [
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(1, 21, 11),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 2.0, 0.5),
                yaw: f64::from(270.0_f32 * 0.017453292_f32),
                input: Vec3::new(0.0, 0.0, 1.0),
                on_ground: true,
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };

            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );

            assert_eq!(state.position.z, 0.5);
            assert_eq!(state.position.x, 0.5 + java_float(0.98) * java_float(0.1));
        }
    }

    #[test]
    fn diagonal_input_is_normalized_by_a_float_length_before_1_17() {
        let diagonal = |release| {
            let mut state = SimulationState {
                input: Vec3::new(1.0, 0.0, 1.0),
                no_gravity: true,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.position.x
        };

        let keys = java_float(0.98);
        let length = (keys * keys + keys * keys).sqrt();
        assert_eq!(
            diagonal(MinecraftVersion::new(1, 16, 5)),
            keys / java_float(length) * java_float(0.02)
        );
        assert_eq!(
            diagonal(MinecraftVersion::new(1, 17, 0)),
            keys / length * java_float(0.02)
        );
    }

    #[test]
    fn gliding_lift_adds_a_literal_before_1_13() {
        let glide = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                look: Vec3::new(0.1, 0.0, 0.1),
                fall_flying: true,
                is_player: true,
                half_width: 0.3,
                height: 0.6,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.velocity.y
        };
        let lift = java_float(java_float((0.1_f64 * 0.1 + 0.1 * 0.1).sqrt()) / 0.4);
        let vertical = |falling: f64| (falling + falling * -0.1 * lift) * java_float(0.98);

        assert_eq!(
            glide(MinecraftVersion::new(1, 12, 2)),
            vertical(-0.08 + lift * 0.06)
        );
        assert_eq!(
            glide(MinecraftVersion::new(1, 13, 2)),
            vertical(0.08 * (-1.0 + lift * 0.75))
        );
    }

    #[test]
    fn gliding_lift_uses_a_float_look_length_before_1_17() {
        let glide = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                look: Vec3::new(0.1, 0.0, 0.1),
                fall_flying: true,
                is_player: true,
                half_width: 0.3,
                height: 0.6,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.velocity.y
        };
        let vertical = |look_length: f64| {
            let lift = java_float(look_length / 0.4);
            let falling = 0.08 * (-1.0 + lift * 0.75);
            (falling + falling * -0.1 * lift) * java_float(0.98)
        };

        let look_length = (0.1_f64 * 0.1 + 0.1 * 0.1).sqrt();
        assert_eq!(
            glide(MinecraftVersion::new(1, 16, 5)),
            vertical(java_float(look_length))
        );
        assert_eq!(
            glide(MinecraftVersion::new(1, 17, 0)),
            vertical(look_length)
        );
        assert_ne!(vertical(java_float(look_length)), vertical(look_length));
    }

    #[test]
    fn a_glide_falls_by_the_final_gravity_from_1_20_5() {
        // 1.20.5+ travel takes d = getFinalGravity(), 0 with NoGravity, and the glide adds
        // d * (-1 + lift * 0.75) (1.20.5 LivingEntity.java:2111, Entity.java:1112-1114; 1.21.11
        // Entity.java:1398-1400, LivingEntity.java:2460-2467); before, the glide's 0.08 ignores
        // NoGravity (1.16.5 LivingEntity.java:1963)
        let glide = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                look: Vec3::new(1.0, 0.0, 0.0),
                fall_flying: true,
                no_gravity: true,
                is_player: true,
                half_width: 0.3,
                height: 0.6,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.velocity.y
        };
        let falling = 0.08 * (-1.0 + 0.75);
        assert_eq!(
            glide(MinecraftVersion::new(1, 20, 4)),
            (falling + falling * -0.1) * java_float(0.98)
        );
        for release in [
            MinecraftVersion::new(1, 20, 5),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            assert_eq!(glide(release), 0.0, "{release:?}");
        }
    }

    fn tick_kept_box(state: SimulationState, release: MinecraftVersion) -> SimulationState {
        let mut state = SimulationState {
            no_gravity: true,
            ..state
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(
            &mut state,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
            rules,
        );
        state
    }

    #[test]
    fn the_position_is_the_centre_of_the_kept_bounding_box_before_1_17() {
        let kept = Aabb::new(
            Vec3::new(0.12928931033455007, 5.0, 0.12928931033455007),
            Vec3::new(0.729289334176408, 6.5, 0.729289334176408),
        );
        let state = SimulationState {
            position: Vec3::new(0.429289322255479, 5.0, 0.429289322255479),
            velocity: Vec3::new(-0.2, 0.0, -0.2),
            half_width: f64::from(0.6_f32) / 2.0,
            height: f64::from(1.5_f32),
            bounding_box: Some(kept),
            ..SimulationState::default()
        };

        let old = tick_kept_box(state, MinecraftVersion::new(1, 16, 5));
        let moved = kept.translated(state.velocity);
        let centre = Vec3::new(
            (moved.min.x + moved.max.x) / 2.0,
            moved.min.y,
            (moved.min.z + moved.max.z) / 2.0,
        );
        assert_eq!((old.position, old.bounding_box), (centre, Some(moved)));
        assert_ne!(centre, state.position + state.velocity);

        let modern = tick_kept_box(state, MinecraftVersion::new(1, 17, 0));
        assert_eq!(
            (modern.position, modern.bounding_box),
            (state.position + state.velocity, None)
        );
    }

    #[test]
    fn a_move_too_short_to_count_leaves_the_kept_bounding_box_before_1_17() {
        let state = SimulationState {
            position: Vec3::new(0.5, 5.0, 0.5),
            velocity: Vec3::new(0.01, 0.0, 0.01),
            stuck_speed_multiplier: Vec3::new(0.01, 0.01, 0.01),
            half_width: f64::from(0.6_f32) / 2.0,
            height: f64::from(1.8_f32),
            ..SimulationState::default()
        };

        let old = tick_kept_box(state, MinecraftVersion::new(1, 16, 5));

        assert_eq!(
            (old.position, old.bounding_box),
            (state.position, Some(state.bounds_around_position()))
        );
    }

    #[test]
    fn a_pose_of_another_size_refits_the_kept_bounding_box_before_1_17() {
        let width = f64::from(0.6_f32);
        let standing = Aabb::new(
            Vec3::new(0.1, 5.0, 0.1),
            Vec3::new(0.1 + width, 5.0 + f64::from(1.8_f32), 0.1 + width),
        );
        let crouching = SimulationState {
            position: Vec3::new(0.4, 5.0, 0.4),
            half_width: width / 2.0,
            height: f64::from(1.5_f32),
            bounding_box: Some(standing),
            ..SimulationState::default()
        };
        let rounded_wider = SimulationState {
            bounding_box: Some(Aabb::new(
                standing.min,
                standing.max + Vec3::new(1.0e-15, 0.0, 1.0e-15),
            )),
            ..crouching
        };
        let sleeping = SimulationState {
            half_width: f64::from(0.2_f32) / 2.0,
            height: f64::from(0.2_f32),
            ..crouching
        };
        let release = MinecraftVersion::new(1, 16, 5);

        let refitted = Some(Aabb::new(
            standing.min,
            standing.min + Vec3::new(width, crouching.height, width),
        ));
        assert_eq!(tick_kept_box(crouching, release).bounding_box, refitted);
        assert_eq!(tick_kept_box(rounded_wider, release).bounding_box, refitted);
        assert_eq!(
            tick_kept_box(sleeping, release).bounding_box,
            Some(sleeping.bounds_around_position())
        );
    }

    #[test]
    fn gliding_moves_with_the_dragged_velocity_and_ignores_the_keys() {
        let glide = |input: Vec3| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                velocity: Vec3::new(0.0, 0.0, 1.0),
                input,
                look: Vec3::new(0.0, 0.0, 1.0),
                fall_flying: true,
                is_player: true,
                half_width: 0.3,
                height: 0.6,
                ..SimulationState::default()
            };
            tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());
            state
        };

        let gliding = glide(Vec3::ZERO);
        let lift = 1.0;
        let falling = 0.08 * (-1.0 + lift * 0.75);
        let dive = falling * -0.1 * lift;
        assert_eq!(gliding.velocity.y, (falling + dive) * java_float(0.98));
        assert_eq!(gliding.position.y, 10.0 + gliding.velocity.y);
        assert_eq!(gliding.position.z, 0.5 + gliding.velocity.z);
        let keyed = glide(Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(
            (keyed.position, keyed.velocity),
            (gliding.position, gliding.velocity)
        );
    }

    #[test]
    fn rotates_input_by_yaw() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            yaw: std::f64::consts::FRAC_PI_2,
            input: Vec3::new(1.0, 0.0, 0.0),
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());
        assert!(state.position.z > 0.09);
        assert!(state.position.x.abs() < 1e-12);
    }

    #[test]
    fn water_movement_effects_increase_horizontal_drag() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 2, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let base = SimulationState {
            position: Vec3::new(0.5, 2.5, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 12.0,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut enhanced = base;
        enhanced.dolphins_grace = 1;
        enhanced.depth_strider = 3;
        let mut base = base;
        tick(&mut base, &world, PhysicsConfig::default());
        tick(&mut enhanced, &world, PhysicsConfig::default());
        assert!(enhanced.velocity.x > base.velocity.x);
    }

    #[test]
    fn depth_strider_uses_vanilla_water_drag_interpolation() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            depth_strider: 3,
            on_ground: true,
            velocity: Vec3::new(1.0, 0.0, 0.0),
            ..SimulationState::default()
        };
        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_20_6::Profile::rules(),
        );

        assert!((state.velocity.x - 0.54600006).abs() < 1e-7);
    }

    #[test]
    fn depth_strider_uses_vanilla_water_input_interpolation() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, -1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            depth_strider: 3,
            movement_speed: Some(0.1),
            input: Vec3::new(0.0, 0.0, 1.0),
            on_ground: true,
            ..SimulationState::default()
        };

        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_20_6::Profile::rules(),
        );

        assert!((state.velocity.z - 0.054600006 * java_float(0.98)).abs() < 1e-7);
    }

    #[test]
    fn modern_water_efficiency_uses_full_attribute_interpolation() {
        assert_eq!(
            crate::versioned::v1_20_6::Profile::rules().water_movement_efficiency_divisor,
            3.0
        );
        assert_eq!(
            crate::versioned::v1_21_1::Profile::rules().water_movement_efficiency_divisor,
            1.0
        );
    }

    #[test]
    fn modern_water_efficiency_is_independent_of_depth_strider() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, -1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        let state_template = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            depth_strider: 0,
            water_movement_efficiency: 3.0,
            movement_speed: Some(0.1),
            input: Vec3::new(0.0, 0.0, 1.0),
            on_ground: true,
            ..SimulationState::default()
        };
        let mut state = state_template.clone();
        let mut depth_strider_state = state_template;
        depth_strider_state.depth_strider = 3;

        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_21_1::Profile::rules(),
        );
        tick_with_rules(
            &mut depth_strider_state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_21_1::Profile::rules(),
        );

        assert!((state.velocity.z - depth_strider_state.velocity.z).abs() < 1e-12);
        assert!(state.velocity.z.is_finite());
    }

    #[test]
    fn water_uses_vanilla_fluid_input_speed() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 2, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.5, 0.5),
            input: Vec3::new(0.0, 0.0, 1.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.z - 0.016 * java_float(0.98)).abs() < 1e-7);
    }

    #[test]
    fn water_applies_fluid_gravity_after_movement() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.y + 0.005).abs() < 1e-12);
    }

    #[test]
    fn sprinting_and_no_gravity_skip_fluid_falling_adjustment() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut sprinting = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(0.0, -0.1, 0.0),
            sprinting: true,
            ..SimulationState::default()
        };
        let mut no_gravity = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(0.0, -0.1, 0.0),
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut sprinting, &world, PhysicsConfig::default());
        tick(&mut no_gravity, &world, PhysicsConfig::default());

        assert!((sprinting.velocity.y + 0.08).abs() < 1e-7);
        assert!((no_gravity.velocity.y + 0.08).abs() < 1e-7);
    }

    #[test]
    fn lava_uses_distinct_horizontal_drag_and_height_threshold() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Lava,
                fluid_level: 4,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.x - 0.5).abs() < 1e-12);
        assert!(state.velocity.y < 0.0);
    }

    #[test]
    fn fluid_currents_normalize_only_above_vanilla_threshold() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_flow: Vec3::new(0.5, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.x - 0.014 * 0.8).abs() < 1e-7);
    }

    #[test]
    fn fluid_currents_apply_to_any_nonzero_flow_until_26_1() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_flow: Vec3::new(0.001, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let pushed = |release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state.velocity.x
        };

        assert!(pushed(MinecraftVersion::new(1, 21, 11)) > 0.0);
        assert_eq!(pushed(MinecraftVersion::new(26, 1, 0)), 0.0);
    }

    #[test]
    fn ultra_warm_dimensions_use_fast_lava_currents() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Lava,
                fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let pushed = |release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                ultra_warm: true,
                controlled_by_local_instance: true,

                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state.velocity.x
        };

        assert!((pushed(MinecraftVersion::new(1, 21, 11)) - 0.007 * 0.5).abs() < 1e-7);
        assert!((pushed(MinecraftVersion::new(26, 1, 0)) - (0.007 + 0.007) * 0.5).abs() < 1e-7);
    }

    #[test]
    fn players_preserve_fluid_flow_magnitude() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_flow: Vec3::new(0.5, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            is_player: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.x - 0.014 * 0.5 * 0.8).abs() < 1e-7);
    }

    #[test]
    fn shallow_player_fluid_flow_is_height_scaled() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_level: 7,
                fluid_flow: Vec3::new(100.0, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.1, 0.5),
            is_player: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        let height = 1.0 / 9.0 - 0.1;
        assert!((state.velocity.x - 0.014 * 100.0 * height * 0.8).abs() < 1e-7);
    }

    #[test]
    fn fluid_currents_average_all_intersecting_fluid_blocks() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        for (position, flow) in [
            ((0, 0, 0), Vec3::new(1.0, 0.0, 0.0)),
            ((1, 0, 0), Vec3::new(0.0, 0.0, 1.0)),
        ] {
            world.set_block(
                position,
                BlockState {
                    fluid: crate::world::FluidState::Water,
                    fluid_flow: flow,
                    ..BlockState::default()
                },
            );
        }
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            half_width: 0.6,
            is_player: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.x > 0.0);
        assert!(state.velocity.z > 0.0);
    }

    #[test]
    fn players_receive_vanilla_minimum_horizontal_fluid_push() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_flow: Vec3::new(0.1, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            is_player: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.x >= 0.0045 * 0.8);
    }

    #[test]
    fn non_players_receive_vanilla_minimum_horizontal_fluid_push() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_flow: Vec3::new(0.1, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.x >= 0.0045 * 0.8);
    }

    #[test]
    fn player_minimum_fluid_push_preserves_vertical_direction() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                fluid_flow: Vec3::new(0.1, 0.1, 0.0),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            is_player: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.y > -0.08);
        assert!(state.velocity.x > 0.0);
    }

    #[test]
    fn short_entities_have_zero_fluid_jump_threshold() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Lava,
                fluid_flow: Vec3::ZERO,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            height: 0.4,
            velocity: Vec3::new(1.0, 0.0, 0.0),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.x - 0.5).abs() < 1e-7);
    }

    #[test]
    fn swimming_does_not_inject_unconditional_look_steering() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 2, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.5, 0.5),
            swimming: true,
            is_player: true,
            look: Vec3::new(0.0, -1.0, 0.0),
            sprinting: true,
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.y < 0.0);
        state.velocity.y = 0.0;
        state.passenger = true;
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.velocity.y, 0.0);
    }

    #[test]
    fn swimming_steering_uses_the_source_head_block_height() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            swimming: true,
            is_player: true,
            no_gravity: true,
            look: Vec3::new(0.0, 0.5, 0.0),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.y > 0.0);
    }

    #[test]
    fn swimming_steering_reads_any_fluid_at_y_plus_one_minus_a_tenth() {
        // Player.travel: getFluidState(BlockPos.containing(x, y + 1.0 - 0.1, z)).isEmpty() (1.13.2
        // PlayerEntity.java:1489, 1.21.11 Player.java:1364, 26.3 :1301)
        let water = BlockState {
            fluid: FluidState::Water,
            ..BlockState::default()
        };
        let lava = BlockState {
            fluid: FluidState::Lava,
            ..BlockState::default()
        };
        let swimmer = |y: f64| SimulationState {
            position: Vec3::new(0.5, y, 0.5),
            swimming: true,
            is_player: true,
            no_gravity: true,
            look: Vec3::new(0.0, 0.5, 0.0),
            ..SimulationState::default()
        };
        // y = 3.1: (3.1 + 1.0) - 0.1 is 3.9999999999999996, the water cell; 3.1 + 0.9 is 4.0
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block((0, 3, 0), water);
        let mut state = swimmer(3.1);
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.velocity.y > 0.0);
        // lava at the head steers too
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block((0, 0, 0), water);
        world.set_block((0, 1, 0), lava);
        let mut state = swimmer(0.5);
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.velocity.y > 0.0);
        // nothing at the head: no steering while looking up
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block((0, 0, 0), water);
        let mut state = swimmer(0.5);
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.velocity.y, 0.0);
    }

    #[test]
    fn fluid_falling_adjustment_skips_a_zero_gravity_from_1_20_5() {
        // LivingEntity.getFluidFallingAdjustedMovement: `gravity != 0.0 && !isSprinting()` from
        // 1.20.5 (1.20.5 LivingEntity.java:2288, 1.21.11 :2545, 26.3 :2609)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        for version in ["1.20.5", "1.21.11", "26.2", "26.3"] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                gravity: Some(0.0),
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.velocity.y, 0.0, "{version}");
        }
    }

    #[test]
    fn jumps_from_ground() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            jump_requested: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.y > 1.0);
        assert!(!state.on_ground);
    }

    #[test]
    fn sprint_jumping_adds_forward_momentum() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            jump_requested: true,
            sprinting: true,
            yaw: 0.0,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        let ground_drag = java_float(java_float(0.6) * java_float(0.98));
        assert_eq!(state.position.z, 0.5 + 0.2);
        assert_eq!(state.velocity.z, 0.2 * ground_drag);
    }

    fn jump_height(state: SimulationState, release: MinecraftVersion) -> f64 {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            jump_requested: true,
            half_width: 0.3,
            height: 1.8,
            ..state
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(
            &mut state,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
            rules,
        );
        state.position.y
    }

    #[test]
    fn jump_boost_adds_a_tenth_per_level_in_the_precision_of_the_release() {
        let boosted = SimulationState {
            jump_boost_amplifier: Some(0),
            ..SimulationState::default()
        };

        assert_eq!(
            jump_height(boosted, MinecraftVersion::new(1, 13, 2)),
            1.0 + (f64::from(0.42_f32) + f64::from(0.1_f32))
        );
        assert_eq!(
            jump_height(boosted, MinecraftVersion::new(1, 16, 5)),
            1.0 + f64::from(0.42_f32 + 0.1_f32)
        );
        assert_eq!(
            jump_height(boosted, MinecraftVersion::new(1, 19, 2)),
            1.0 + (f64::from(0.42_f32) + f64::from(0.1_f32))
        );
        assert_eq!(
            jump_height(boosted, MinecraftVersion::new(1, 21, 1)),
            1.0 + f64::from(0.42_f32 + 0.1_f32)
        );
    }

    #[test]
    fn jumping_keeps_a_faster_rise_from_1_21_2() {
        let rising = SimulationState {
            velocity: Vec3::new(0.0, 0.6, 0.0),
            ..SimulationState::default()
        };

        assert_eq!(
            jump_height(rising, MinecraftVersion::new(1, 21, 1)),
            1.0 + java_float(0.42)
        );
        assert_eq!(
            jump_height(rising, MinecraftVersion::new(1, 21, 2)),
            1.0 + 0.6
        );
    }

    #[test]
    fn a_zero_jump_strength_does_not_jump_from_1_20_5() {
        let grounded = SimulationState {
            jump_strength: Some(0.0),
            ..SimulationState::default()
        };

        assert_eq!(jump_height(grounded, MinecraftVersion::new(1, 21, 1)), 1.0);
    }

    #[test]
    fn releasing_jump_clears_the_jump_delay() {
        let mut state = SimulationState {
            jump_ticks: 5,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert_eq!(state.jump_ticks, 0);
    }

    #[test]
    fn ground_jump_cooldown_blocks_immediate_repeat() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            jump_requested: true,
            jump_ticks: 2,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.velocity.y < 0.0);
    }

    #[test]
    fn swimming_does_not_change_air_vertical_drag() {
        let mut swimming = SimulationState {
            swimming: true,
            is_player: true,
            no_gravity: true,
            velocity: Vec3::new(0.0, 1.0, 0.0),
            ..SimulationState::default()
        };
        let mut ordinary = swimming;
        ordinary.swimming = false;

        tick(
            &mut swimming,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
        );
        tick(
            &mut ordinary,
            &WorldSnapshot::new(),
            PhysicsConfig::default(),
        );

        assert_eq!(swimming.velocity.y, java_float(0.98));
        assert_eq!(swimming.velocity.y, ordinary.velocity.y);
    }

    #[test]
    fn sprinting_uses_source_owned_water_slowdown() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            sprinting: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_19::Profile::rules(),
        );

        assert!((state.velocity.x - 0.9).abs() < 1.0e-7);
    }

    #[test]
    fn water_climbable_jump_does_not_use_land_jump_vertical_boost() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
                    slipperiness: 0.6,
                    fluid: FluidState::Water,
                    effect: BlockEffect::Climbable,
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            jump_requested: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_ne!(state.velocity.y, 0.2);
    }

    #[test]
    fn zero_vertical_request_clears_stale_collision_state() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, -1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.1, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            collided_vertically: true,
            vertical_collision_below: true,
            on_ground: true,
            is_player: true,
            controlled_by_local_instance: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_21_4::Profile::rules(),
        );

        assert!(!state.collided_vertically);
        assert!(!state.vertical_collision_below);
        assert!(!state.on_ground);
    }

    // Entity.move refreshes verticalCollision and onGround on every move until 1.21.4
    // (1.21.3 Entity.java:675-676), which keeps them for a zero-Y move of an entity that is not
    // controlled locally (`Math.abs(movement.y) > 0.0 || isLocalPlayerOrLogicalSideForUpdatingMovement()`,
    // 1.21.4 Entity.java:683).
    #[test]
    fn a_zero_y_move_refreshes_the_ground_state_of_a_remote_entity_until_1_21_4() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, -1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let moved = |rules: VersionPhysicsRules| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 3.0, 0.5),
                velocity: Vec3::new(0.1, 0.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                collided_vertically: true,
                vertical_collision_below: true,
                on_ground: true,
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            (state.collided_vertically, state.on_ground)
        };

        assert_eq!(
            moved(crate::versioned::v1_21_3::Profile::rules()),
            (false, false)
        );
        assert_eq!(
            moved(crate::versioned::v1_20_4::Profile::rules()),
            (false, false)
        );
        assert_eq!(
            moved(crate::versioned::v1_21_4::Profile::rules()),
            (true, true)
        );
        assert_eq!(
            moved(crate::versioned::v26_3::Profile::rules()),
            (true, true)
        );
    }

    // From 1.21.2 the client runs Entity.checkFallDamage (the fall distance, the water push)
    // only for an entity it controls: `!isClientSide() || isLogicalSideForUpdatingMovement()`
    // (1.21.3 Entity.java:686), isLocalInstanceAuthoritative from 1.21.5 (1.21.5 Entity.java:708).
    // Before, it runs for every entity (1.21 Entity.java:656).
    #[test]
    fn a_remote_entity_gains_no_fall_distance_on_the_client_from_1_21_2() {
        let fallen = |rules: VersionPhysicsRules, controlled_by_local_instance: bool| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance,
                ..SimulationState::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.fall_distance
        };

        assert!(fallen(crate::versioned::v1_21::Profile::rules(), false) > 0.0);
        assert_eq!(
            fallen(crate::versioned::v1_21_2::Profile::rules(), false),
            0.0
        );
        assert_eq!(
            fallen(crate::versioned::v1_21_5::Profile::rules(), false),
            0.0
        );
        assert!(fallen(crate::versioned::v1_21_2::Profile::rules(), true) > 0.0);
        assert!(fallen(crate::versioned::v26_3::Profile::rules(), true) > 0.0);
    }

    // Entity.checkSupportingBlock (1.21.11 Entity.java:665-685, the same from 1.20
    // updateSupportingBlockPos): with no block under the box, the box where the move started is
    // tried only when the last check found a block (onGroundNoBlocks); with no move to retry, the
    // block kept before stays; off the ground there is none.
    #[test]
    fn the_supporting_block_is_kept_between_moves() {
        use crate::simulation::check_supporting_block;
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let walked_off = SimulationState {
            position: Vec3::new(1.5, 1.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            ..SimulationState::default()
        };
        let moved = Some(Vec3::new(1.0, 0.0, 0.0));

        let mut state = walked_off;
        assert_eq!(
            check_supporting_block(&world, &mut state, moved),
            Some((0, 0, 0))
        );
        assert!(!state.on_ground_no_blocks);
        assert!(state.supporting_block_kept);

        let mut state = SimulationState {
            on_ground_no_blocks: true,
            ..walked_off
        };
        assert_eq!(check_supporting_block(&world, &mut state, moved), None);
        assert!(state.on_ground_no_blocks);

        let mut state = SimulationState {
            supporting_block: Some((5, 5, 5)),
            supporting_block_kept: true,
            ..walked_off
        };
        assert_eq!(
            check_supporting_block(&world, &mut state, None),
            Some((5, 5, 5))
        );
        assert!(state.on_ground_no_blocks);

        let mut state = SimulationState {
            on_ground: false,
            supporting_block: Some((0, 0, 0)),
            supporting_block_kept: true,
            on_ground_no_blocks: true,
            ..walked_off
        };
        assert_eq!(check_supporting_block(&world, &mut state, moved), None);
        assert!(!state.on_ground_no_blocks);
    }

    // Entity.setOnGround(false) empties mainSupportingBlockPos and onGroundNoBlocks (1.21.11
    // Entity.java:680-684), which Player.tick does each tick the player rides (Player.java:238-241)
    // and a respawned LocalPlayer starts without: a state handed in off the ground (dismounted)
    // keeps neither from before it rode. Landing on a boat-like collider with no block under the box,
    // the box where the move started is tried (onGroundNoBlocks false), finding the slime block
    // there, which bounces the player.
    #[test]
    fn a_state_off_the_ground_keeps_no_supporting_block_from_before() {
        use crate::world::EntityCollider;
        let mut world = WorldSnapshot::new();
        world.set_block(
            (-1, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world.set_entity_colliders(vec![EntityCollider {
            bounds: Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            only_from_above: false,
        }]);
        let dismounted = SimulationState {
            position: Vec3::new(-0.1, 1.0, 0.5),
            velocity: Vec3::new(0.6, -0.1, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: false,
            is_player: true,
            supporting_block: Some((7, 0, 7)),
            supporting_block_kept: true,
            on_ground_no_blocks: true,
            ..SimulationState::default()
        };
        let mut state = dismounted;
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.on_ground);
        assert_eq!(state.supporting_block, Some((-1, 0, 0)));
        assert!(state.velocity.y > 0.0);
    }

    // CollisionGetter.findSupportingBlock walks BlockCollisions with the entity's CollisionContext
    // (1.21.11 CollisionGetter.java:138-153): powder snow holds up an entity that walks on it
    // (PowderSnowBlock.getCollisionShape) and so supports it.
    #[test]
    fn the_supporting_block_uses_the_entity_collision_context() {
        use crate::simulation::check_supporting_block;
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::PowderSnow,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let supported = |powder_snow_walkable: bool| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                powder_snow_walkable,
                ..SimulationState::default()
            };
            check_supporting_block(&world, &mut state, None)
        };

        assert_eq!(supported(true), Some((0, 0, 0)));
        assert_eq!(supported(false), None);
    }

    /// Marks the chunk columns -1..=1 loaded: from 26.1 an entity touches no fluid while a chunk
    /// under its cells widened by one is not (26.1 EntityFluidInteraction.java:44, 94-110).
    fn load_chunks_around_origin(world: &mut WorldSnapshot) {
        for chunk_x in -1..=1 {
            for chunk_z in -1..=1 {
                world.set_chunk_loaded(chunk_x, chunk_z, true);
            }
        }
    }

    fn fluid_block_world(fluid: FluidState) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid,
                ..BlockState::default()
            },
        );
        world
    }

    fn tick_release(
        state: &mut SimulationState,
        world: &WorldSnapshot,
        release: MinecraftVersion,
    ) -> TickResult {
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(state, world, PhysicsConfig::default(), rules)
    }

    #[test]
    fn lava_counts_from_the_move_that_reached_it_before_1_16() {
        let lava = fluid_block_world(FluidState::Lava);
        let release = MinecraftVersion::new(1, 15, 2);
        let mut reaching = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut leaving = SimulationState {
            position: Vec3::new(0.5, 5.0, 0.5),
            in_lava: true,
            ..reaching
        };

        tick_release(&mut reaching, &lava, release);
        tick_release(&mut leaving, &lava, release);

        assert_eq!(reaching.velocity.y, -0.08 * java_float(0.98));
        assert!(reaching.in_lava);
        assert_eq!(leaving.velocity.y, -0.08 / 4.0);
        assert!(!leaving.in_lava);
    }

    #[test]
    fn lava_beside_water_neither_pushes_nor_counts_its_height_in_1_16_and_1_16_1() {
        // 1.16 Entity.updateWaterState returns once touching water, before it measures the lava
        // (1.16 Entity.java:1042-1051); 1.16.2 always measures it (1.16.5 Entity.java:1046-1052)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        world.set_block(
            (1, 0, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_level: 2,
                fluid_flow: Vec3::new(1.0, 0.0, 0.0),
                ..BlockState::default()
            },
        );
        let state = SimulationState {
            position: Vec3::new(0.9, 0.2, 0.5),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        for (release, pushed) in [
            (MinecraftVersion::new(1, 16, 0), false),
            (MinecraftVersion::new(1, 16, 1), false),
            (MinecraftVersion::new(1, 16, 2), true),
        ] {
            let mut state = state;
            tick_release(&mut state, &world, release);
            assert!(state.in_water, "{release}");
            assert_eq!(state.velocity.x > 0.0, pushed, "{release}");
        }
    }

    #[test]
    fn lava_counts_from_the_move_that_reached_its_surface_in_1_16_and_1_16_1() {
        // 1.16 Entity.isInLava is the flag FluidBlock.onEntityCollision sets in the move, for a
        // lava block whose float top is above the box's foot (1.16 Entity.java:657-659,
        // 1160-1166, FluidBlock.java:170-177); from 1.16.2 the lava height of the base tick
        // (1.16.5 Entity.java:1181-1183)
        let lava = fluid_block_world(FluidState::Lava);
        let release = MinecraftVersion::new(1, 16, 1);
        let mut reaching = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut leaving = SimulationState {
            position: Vec3::new(0.5, 5.0, 0.5),
            in_lava: true,
            ..reaching
        };
        // a flowing lava of level 7 tops at 1/9: a foot at 0.2 is above it
        let mut low = fluid_block_world(FluidState::Lava);
        low.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_level: 7,
                ..BlockState::default()
            },
        );
        let mut above_surface = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            ..reaching
        };

        tick_release(&mut reaching, &lava, release);
        tick_release(&mut leaving, &lava, release);
        tick_release(&mut above_surface, &low, release);

        assert_eq!(reaching.velocity.y, -0.08 * java_float(0.98));
        assert!(reaching.in_lava);
        assert!(!leaving.in_lava);
        assert!(!above_surface.in_lava);
    }

    #[test]
    fn lava_depth_counts_from_after_the_move_from_26_1() {
        let mut lava = fluid_block_world(FluidState::Lava);
        load_chunks_around_origin(&mut lava);
        let sinking = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.6, 0.5),
                velocity: Vec3::new(0.0, -0.3, 0.0),
                is_player: true,
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,

                ..SimulationState::default()
            };
            tick_release(&mut state, &lava, release);
            state.velocity.y
        };

        assert_eq!(
            sinking(MinecraftVersion::new(26, 1, 0)),
            -0.3 * 0.5 - 0.08 / 4.0
        );
        assert_eq!(
            sinking(MinecraftVersion::new(1, 21, 11)),
            -0.3 * java_float(0.8) - 0.08 / 16.0 - 0.08 / 4.0
        );
    }

    #[test]
    fn a_jump_swims_up_in_the_lava_the_last_move_reached_before_1_16() {
        let mut in_lava = SimulationState {
            position: Vec3::new(0.5, 5.0, 0.5),
            on_ground: true,
            jump_requested: true,
            in_lava: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick_release(
            &mut in_lava,
            &WorldSnapshot::new(),
            MinecraftVersion::new(1, 14, 4),
        );

        assert_eq!(in_lava.velocity.y, 0.03999999910593033 * 0.5 - 0.08 / 4.0);
        assert!(!in_lava.in_lava);
    }

    #[test]
    fn sprinting_lava_still_applies_source_gravity() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_level: 8,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            sprinting: true,
            no_gravity: false,
            ..SimulationState::default()
        };

        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            crate::versioned::v1_19::Profile::rules(),
        );

        assert!(state.velocity.y < 0.0);
    }

    #[test]
    fn lava_gravity_uses_version_rule_divisor() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_level: 8,
                ..BlockState::default()
            },
        );
        let mut rules = VersionPhysicsRules::default();
        rules.lava_gravity_divisor = 8.0;
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            no_gravity: false,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);

        assert!((state.velocity.y + 0.01).abs() < 1.0e-12);
    }

    #[test]
    fn water_reduces_gravity_and_sets_environment_state() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: crate::world::FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::ZERO,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.in_water);
        assert!(!state.in_lava);
        assert!(state.velocity.y > -0.08);
    }

    #[test]
    fn water_contact_resets_fall_distance() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 0,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            fall_distance: 12.0,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.in_water);
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn entering_water_restarts_the_fall_distance_from_the_last_move() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 8,
                ..BlockState::default()
            },
        );
        let falling = SimulationState {
            position: Vec3::new(0.5, 2.5, 0.5),
            velocity: Vec3::new(0.0, -1.5, 0.0),
            on_ground: false,
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        // before 1.21.5 LivingEntity.fall refreshes the water, then Entity.fall counts the drop
        // whatever the water (1.21.4 LivingEntity.java:341-345, Entity.java:1272-1274)
        let mut state = falling;
        crate::versioned::dispatch("1.21.4", &mut state, &world, PhysicsConfig::default())
            .unwrap();
        assert!(state.in_water);
        assert_eq!(state.fall_distance, 1.5);

        // from 1.21.5 the drop counts only out of the water (1.21.5 Entity.java:1285-1287)
        let mut state = falling;
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.in_water);
        assert_eq!(state.fall_distance, 0.0);
    }

    fn legacy_water_world(blocks: &[((i32, i32, i32), Vec3)]) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        for &(position, flow) in blocks {
            world.set_block(
                position,
                BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    fluid_flow: flow,
                    ..BlockState::default()
                },
            );
        }
        world
    }

    fn legacy_water_player(position: Vec3, velocity: Vec3) -> SimulationState {
        SimulationState {
            position,
            velocity,
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        }
    }

    #[test]
    fn water_pushes_by_the_normalized_sum_of_its_flows_before_1_13() {
        let world = legacy_water_world(&[
            ((0, 0, 0), Vec3::new(1.0, 0.0, 0.0)),
            ((1, 0, 0), Vec3::new(0.0, 0.0, 1.0)),
        ]);
        let pushed = |version| {
            let mut state = legacy_water_player(Vec3::new(1.0, 0.0, 0.5), Vec3::ZERO);
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.in_water, "{version}");
            state.velocity
        };

        let current = 1.0 / f64::from(2.0_f32.sqrt()) * 0.014;
        let legacy = pushed("1.12.2");
        assert_eq!(legacy.x, current * f64::from(0.8_f32));
        assert_eq!(legacy.z, current * f64::from(0.8_f32));
        assert_eq!(pushed("1.13.2").x, 0.007 * f64::from(0.8_f32));
    }

    #[test]
    fn water_is_touched_only_by_the_box_shrunk_by_0_4_before_1_13() {
        let world = legacy_water_world(&[((0, 0, 0), Vec3::ZERO)]);
        let in_water = |version| {
            let mut state = legacy_water_player(Vec3::new(0.5, 0.6, 0.5), Vec3::ZERO);
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.in_water
        };

        assert!(!in_water("1.8.8"));
        assert!(!in_water("1.12.2"));
        assert!(in_water("1.13.2"));
    }

    #[test]
    fn water_reached_by_the_move_is_touched_before_1_13() {
        let world = legacy_water_world(&[((0, 0, 0), Vec3::ZERO)]);
        let mut state = legacy_water_player(Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, -1.2, 0.0));

        crate::versioned::dispatch("1.8.8", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.in_water);
        // a float field: (float)((double)fallDistance - dy) (1.8.9 Entity.java:805)
        assert_eq!(state.fall_distance, f64::from(1.2_f32));
    }

    #[test]
    fn water_keeps_0_8_of_the_motion_and_sinks_by_0_02_before_1_13() {
        let world = legacy_water_world(&[
            ((0, 0, 0), Vec3::ZERO),
            ((0, 1, 0), Vec3::ZERO),
            ((0, 2, 0), Vec3::ZERO),
        ]);
        let travel = |version, sprinting| {
            let mut state = SimulationState {
                sprinting,
                no_gravity: false,
                ..legacy_water_player(Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.1, 0.1, 0.0))
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.in_water, "{version}");
            state.velocity
        };

        for sprinting in [false, true] {
            let legacy = travel("1.12.2", sprinting);
            assert_eq!(legacy.x, 0.1 * f64::from(0.8_f32));
            assert_eq!(legacy.y, 0.1 * f64::from(0.8_f32) - 0.02);
        }
        let modern = travel("1.13.2", true);
        assert_eq!(modern.x, 0.1 * f64::from(0.9_f32));
        assert_eq!(modern.y, 0.1 * f64::from(0.8_f32));
    }

    #[test]
    fn flying_player_swims_up_in_water_before_1_13() {
        let world = legacy_water_world(&[((0, 0, 0), Vec3::ZERO), ((0, 1, 0), Vec3::ZERO)]);
        let mut state = SimulationState {
            flying: true,
            jump_requested: true,
            ..legacy_water_player(Vec3::new(0.5, 0.5, 0.5), Vec3::ZERO)
        };

        crate::versioned::dispatch("1.12.2", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.in_water);
        assert_eq!(state.velocity.y, 0.03999999910593033 * 0.6);
    }

    #[test]
    fn water_under_water_keeps_its_own_height_in_1_13() {
        let world = legacy_water_world(&[
            ((0, 0, 0), Vec3::new(100.0, 0.0, 0.0)),
            ((0, 1, 0), Vec3::ZERO),
        ]);
        let pushed = |version| {
            let mut state = legacy_water_player(Vec3::new(0.5, 0.7, 0.5), Vec3::ZERO);
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.velocity.x
        };

        let height = f64::from(8.0_f32 / 9.0) - (0.7 + 0.001);
        assert_eq!(
            pushed("1.13.2"),
            100.0 * height * 0.5 * 0.014 * f64::from(0.8_f32)
        );
        let height = 1.0 - (0.7 + 0.001);
        assert_eq!(
            pushed("1.14.4"),
            100.0 * height * 0.5 * 0.014 * f64::from(0.8_f32)
        );
    }

    #[test]
    fn water_climbs_after_a_collision_only_from_1_14() {
        let mut world = legacy_water_world(&[
            ((0, 0, 0), Vec3::ZERO),
            ((0, 1, 0), Vec3::ZERO),
            ((0, 2, 0), Vec3::ZERO),
        ]);
        for y in 0..3 {
            world.set_block(
                (1, y, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        let climb = |version| {
            let mut state = SimulationState {
                climbing: true,
                ..legacy_water_player(Vec3::new(0.65, 0.5, 0.5), Vec3::new(0.1, 0.0, 0.0))
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.in_water && state.collided_horizontally, "{version}");
            state.velocity.y
        };

        assert_eq!(climb("1.12.2"), 0.0);
        assert_eq!(climb("1.13.2"), 0.0);
        assert_eq!(climb("1.14.4"), 0.2 * f64::from(0.8_f32));
    }

    #[test]
    fn sneaking_players_sink_in_water_only_from_1_13() {
        let world = legacy_water_world(&[
            ((0, 0, 0), Vec3::ZERO),
            ((0, 1, 0), Vec3::ZERO),
            ((0, 2, 0), Vec3::ZERO),
        ]);
        let sink = |version| {
            let mut state = SimulationState {
                sneaking: true,
                ..legacy_water_player(Vec3::new(0.5, 0.5, 0.5), Vec3::ZERO)
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.in_water, "{version}");
            state.velocity.y
        };

        assert_eq!(sink("1.8.8"), 0.0);
        assert_eq!(sink("1.12.2"), 0.0);
        assert_eq!(sink("1.13.2"), -0.03999999910593033 * f64::from(0.8_f32));
    }

    fn lava_world(position: (i32, i32, i32)) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn lava_is_touched_only_by_the_box_shrunk_by_0_1_and_0_4_before_1_14() {
        let in_lava = |version, world: &WorldSnapshot, position| {
            let mut state = legacy_water_player(position, Vec3::ZERO);
            crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default())
                .unwrap();
            state.in_lava
        };
        let below = lava_world((0, 0, 0));
        let beside = lava_world((1, 1, 0));

        for version in ["1.8.8", "1.12.2", "1.13.2"] {
            assert!(
                !in_lava(version, &below, Vec3::new(0.5, 0.6, 0.5)),
                "{version}"
            );
            assert!(
                in_lava(version, &below, Vec3::new(0.5, 0.5, 0.5)),
                "{version}"
            );
            assert!(
                !in_lava(version, &beside, Vec3::new(0.75, 1.0, 0.5)),
                "{version}"
            );
        }
    }

    #[test]
    fn lava_reached_by_the_move_is_reported_after_it_before_1_14() {
        let world = lava_world((0, 0, 0));
        for version in ["1.8.8", "1.13.2"] {
            let mut state =
                legacy_water_player(Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, -1.2, 0.0));

            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();

            assert!(state.in_lava, "{version}");
            assert_eq!(state.velocity.y, -1.2 * f64::from(0.98_f32), "{version}");
        }
    }

    #[test]
    fn supporting_effect_uses_entity_footprint_not_center_cell() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                    slipperiness: 0.6,
                    fluid: FluidState::Empty,
                    effect: BlockEffect::Magma,
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.9, 1.0, 0.5),
            half_width: 0.5,
            height: 1.0,
            on_ground: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.contact_damage, 1.0);
    }

    #[test]
    fn sneaking_player_backs_off_from_uncovered_edge() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.8, 0.0, 0.0),
            is_player: true,
            sneaking: true,
            on_ground: true,
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.position.x < 1.3);
        assert!(state.position.x > 1.1);
    }

    #[test]
    fn fluid_surface_sets_eye_in_fluid_predicates() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 0,
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 2, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 0,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            height: 1.8,
            half_width: 0.3,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.in_water);
        assert!(state.eye_in_water);
        assert!(!state.eye_in_lava);
    }

    #[test]
    fn the_eyes_are_tested_at_the_eye_height() {
        // Entity.updateFluidOnEyes takes getEyeY(), y + the eye height (1.21.11 Entity.java:1542),
        // not y + height - 0.1: eyes at 1.82 are under a water top of 1 + 8/9F
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        for y in 0..2 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
        }
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            height: 1.8,
            half_width: 0.3,
            eye_height: Some(f64::from(1.62_f32)),
            ..SimulationState::default()
        };

        tick_release(&mut state, &world, MinecraftVersion::new(1, 21, 11));

        assert!(state.eye_in_water);
    }

    #[test]
    fn slow_falling_reduces_descent() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            slow_falling: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.velocity.y > -0.01);
    }

    #[test]
    fn slow_falling_clamps_custom_gravity_to_vanilla_cap() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            slow_falling: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let config = PhysicsConfig {
            gravity: 0.2,
            ..PhysicsConfig::default()
        };

        tick(&mut state, &WorldSnapshot::new(), config);

        assert!((state.velocity.y + 0.01 * java_float(0.98)).abs() < 1.0e-12);
    }

    #[test]
    fn slow_falling_and_levitation_reset_fall_distance() {
        for effect in [0_u8, 1_u8] {
            let mut state = SimulationState {
                position: Vec3::new(0.0, 2.0, 0.0),
                slow_falling: effect == 0,
                levitation: effect == 1,
                fall_distance: 12.0,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };

            tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

            assert!(state.fall_distance < 1.0);
        }
    }

    #[test]
    fn levitation_moves_state_upward() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            levitation: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert_eq!(state.velocity.y, 0.05 * 0.2 * java_float(0.98));

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.y > 2.0);
    }

    #[test]
    fn levitation_amplifier_controls_vertical_target() {
        let mut base = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            levitation: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut amplified = SimulationState {
            levitation_amplifier: Some(2),
            ..base
        };

        for _ in 0..2 {
            tick(&mut base, &WorldSnapshot::new(), PhysicsConfig::default());
            tick(
                &mut amplified,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
            );
        }

        assert!(amplified.velocity.y > base.velocity.y);
        assert!(amplified.position.y > base.position.y);
    }

    #[test]
    fn accumulates_and_resets_fall_distance() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.5, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };
        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());
        assert!(state.fall_distance > 0.0);

        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        state.position = Vec3::new(0.5, 1.9, 0.5);
        state.velocity.y = -1.0;
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn sneaking_reduces_horizontal_input() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            sneaking: true,
            no_physics: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.x > 0.0);
        assert!(state.position.x < 0.04);
    }

    #[test]
    fn item_use_applies_vanilla_movement_slowdown() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            using_item: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.x > 0.0);
        assert!(state.position.x < 0.03);
    }

    #[test]
    fn removed_player_stops_its_move_after_the_collision_then_stops_moving_from_1_19_1() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let standing = || SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
            on_ground: true,
            collided_vertically: true,
            is_player: true,
            controlled_by_local_instance: true,
            removed: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        let mut removed = standing();
        tick_release(&mut removed, &world, MinecraftVersion::new(1, 17, 1));
        assert_eq!(removed.position, Vec3::new(0.5, 1.0, 0.5));
        assert!(removed.on_ground);
        assert_eq!(removed.velocity.y, (-0.0784000015258789 - 0.08) * java_float(0.98));

        let mut skipped = standing();
        tick_release(&mut skipped, &world, MinecraftVersion::new(1, 19, 1));
        assert_eq!(skipped.position, standing().position);
        assert_eq!(skipped.velocity, standing().velocity);
    }

    #[test]
    fn item_in_use_slows_the_input_by_its_use_effects_speed() {
        let walked = |using_item: bool, speed: Option<f32>| {
            let mut state = SimulationState {
                position: Vec3::new(0.0, 2.0, 0.0),
                input: Vec3::new(0.0, 0.0, 1.0),
                is_player: true,
                using_item,
                item_use_speed_multiplier: speed,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_release(&mut state, &WorldSnapshot::new(), MinecraftVersion::new(1, 21, 11));
            state.velocity
        };

        assert_eq!(walked(true, Some(1.0)), walked(false, None));
        assert_eq!(walked(true, None), walked(true, Some(0.2)));
        assert!(walked(true, None).z.abs() < walked(false, None).z.abs());
    }

    #[test]
    fn moving_slowly_decides_the_sneak_slowdown_not_the_sneak_key() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let walking = |sneaking: bool, moving_slowly: bool| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, -0.08 * java_float(0.98), 0.0),
                input: Vec3::new(1.0, 0.0, 0.0),
                on_ground: true,
                is_player: true,
                sneaking,
                moving_slowly: Some(moving_slowly),
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick(&mut state, &world, PhysicsConfig::default());
            state.position.x - 0.5
        };

        assert_eq!(walking(true, false), java_float(0.98) * java_float(0.1));
        assert!((walking(false, true) - 0.98 * 0.3 * 0.1).abs() < 1.0e-8);
    }

    #[test]
    fn swift_sneak_uses_the_sneaking_speed_attribute() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            is_player: true,
            moving_slowly: Some(true),
            sneaking_speed: Some(0.45),
            on_ground: true,
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(
            (state.position.x - f64::from(0.98_f32 * 0.45_f32) * java_float(0.1)).abs() < 1.0e-12
        );
    }

    #[test]
    fn a_zero_sneaking_speed_attribute_stops_a_sneaking_player() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            is_player: true,
            moving_slowly: Some(true),
            sneaking_speed: Some(0.0),
            on_ground: true,
            no_gravity: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert_eq!(state.position.x, 0.0);
    }

    #[test]
    fn a_zero_movement_speed_attribute_walks_nowhere() {
        let walk = |movement_speed| {
            let mut state = SimulationState {
                position: Vec3::new(0.0, 2.0, 0.0),
                input: Vec3::new(1.0, 0.0, 0.0),
                is_player: true,
                sprinting: true,
                movement_speed,
                on_ground: true,
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());
            state.position.x
        };

        assert_eq!(walk(Some(0.0)), 0.0);
        assert!(walk(None) > 0.0);
    }

    fn floor_with_step(step_height: f64) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        let solid = |top: f64| BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, top, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        for x in -1..=1 {
            for z in -1..=3 {
                world.set_block((x, 0, z), solid(1.0));
            }
        }
        world.set_block((0, 1, 2), solid(step_height));
        world
    }

    fn walking_to_step(auto_jump: bool) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, 1.0, 1.5),
            velocity: Vec3::new(0.0, -0.08 * java_float(0.98), 0.0),
            input: Vec3::new(0.0, 0.0, 1.0),
            on_ground: true,
            is_player: true,
            auto_jump,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        }
    }

    #[test]
    fn auto_jump_jumps_a_block_high_step_the_tick_after_walking_towards_it() {
        let world = floor_with_step(1.0);
        let mut state = walking_to_step(true);

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.auto_jump_time, 1);
        assert!(state.on_ground);

        state.jump_requested = false;
        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.auto_jump_time, 0);
        assert!(!state.on_ground);
        assert_eq!(
            state.velocity.y,
            (f64::from(0.42_f32) - 0.08) * java_float(0.98)
        );
    }

    #[test]
    fn auto_jump_needs_the_option_a_step_it_cannot_climb_and_release_1_10() {
        let queued = |auto_jump: bool, step_height: f64, release| {
            let mut state = walking_to_step(auto_jump);
            tick_release(&mut state, &floor_with_step(step_height), release);
            state.auto_jump_time
        };
        let release = MinecraftVersion::new(26, 2, 0);

        assert_eq!(queued(true, 1.0, release), 1);
        assert_eq!(queued(false, 1.0, release), 0);
        assert_eq!(queued(true, 0.5, release), 0);
        assert_eq!(queued(true, 1.5, release), 0);
        assert_eq!(queued(true, 1.0, MinecraftVersion::new(1, 10, 0)), 1);
        assert_eq!(queued(true, 1.0, MinecraftVersion::new(1, 9, 4)), 0);
    }

    fn pool() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -2..=8 {
            for z in -2..=2 {
                world.set_block(
                    (x, 0, z),
                    BlockState {
                        id: 1,
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
                for y in 1..=4 {
                    world.set_block(
                        (x, y, z),
                        BlockState {
                            fluid: FluidState::Water,
                            fluid_source: true,
                            ..BlockState::default()
                        },
                    );
                }
            }
        }
        world
    }

    fn releasing_riptide(level: u8, position: Vec3, on_ground: bool) -> SimulationState {
        SimulationState {
            position,
            on_ground,
            in_water: true,
            is_player: true,
            riptide_launch: Some(level),
            yaw: f64::from(270.0_f32 * 0.017453292_f32),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        }
    }

    #[test]
    fn a_released_riptide_trident_launches_a_player_in_water_along_its_look() {
        let mut state = releasing_riptide(3, Vec3::new(0.5, 2.0, 0.5), false);

        tick(&mut state, &pool(), PhysicsConfig::default());

        assert_eq!(state.velocity.x, 3.0 * f64::from(0.8_f32));
        assert_eq!(state.position.x, 3.5);
        assert_eq!(state.riptide_launch, None);
    }

    #[test]
    fn a_riptide_launch_needs_water_and_release_1_13() {
        let launched = |in_water: bool, release| {
            let mut state = SimulationState {
                in_water,
                ..releasing_riptide(1, Vec3::new(0.5, 2.0, 0.5), false)
            };
            tick_release(&mut state, &pool(), release);
            state.position.x - 0.5
        };

        assert_eq!(launched(true, MinecraftVersion::new(1, 13, 0)), 1.5);
        assert_eq!(launched(false, MinecraftVersion::new(26, 2, 0)), 0.0);
        assert_eq!(launched(true, MinecraftVersion::new(1, 12, 2)), 0.0);
    }

    #[test]
    fn a_riptide_launch_lifts_a_player_on_the_ground() {
        let mut state = releasing_riptide(1, Vec3::new(0.5, 1.0, 0.5), true);

        tick(&mut state, &pool(), PhysicsConfig::default());

        assert!(state.position.y > 2.1);
        assert!(!state.on_ground);
    }

    #[test]
    fn a_riptide_lift_is_stuck_in_the_cobweb_it_reaches_until_1_21_2() {
        // TridentItem.releaseUsing's lift is an Entity.move, whose checkInsideBlocks runs to 1.21.1
        // (1.20.4 Entity.java:713, 1.21 :713); from 1.21.2 it only queues its path, and the tick's
        // check starts from where the tick began (1.21.3 Entity.java:752-767, LivingEntity.java:2785)
        let mut world = pool();
        world.set_block(
            (0, 3, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let launch = |release: &str| {
            let mut state = releasing_riptide(1, Vec3::new(0.5, 1.0, 0.5), true);
            crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state
        };

        let stuck = launch("1.21.1");
        assert!(stuck.move_slowed);
        // the web's 0.25 of the 1.5 push
        assert!(stuck.position.x < 1.0, "{}", stuck.position.x);
        let free = launch("1.21.2");
        assert!(!free.move_slowed);
        assert!(free.position.x > 1.9, "{}", free.position.x);
    }

    #[test]
    fn auto_jump_waits_while_sneaking() {
        let mut state = SimulationState {
            sneaking: true,
            ..walking_to_step(true)
        };

        tick(&mut state, &floor_with_step(1.0), PhysicsConfig::default());

        assert_eq!(state.auto_jump_time, 0);
    }

    fn ice_edge_beside(lower: BlockShape) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), ice());
        world.set_block(
            (1, 0, 0),
            BlockState {
                id: 2,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0)),
                    ..lower
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn walking_off_an_edge_keeps_the_block_the_move_started_on() {
        let world = ice_edge_beside(BlockShape {
            effect: BlockEffect::SoulSand,
            ..BlockShape::full_block()
        });
        let mut state = SimulationState {
            position: Vec3::new(1.25, 1.0, 0.5),
            velocity: Vec3::new(0.2, -0.1, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        crate::versioned::dispatch("1.21.1", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(state.on_ground);
        assert_eq!(state.position.x, 1.45);
        let ice_drag = java_float(java_float(0.98) * java_float(0.91));
        assert_eq!(state.velocity.x, 0.2 * ice_drag);
        assert_eq!(state.previous_position, Some(Vec3::new(1.25, 1.0, 0.5)));
    }

    #[test]
    fn the_previous_position_finds_the_block_left_behind_for_friction() {
        let world = ice_edge_beside(BlockShape::full_block());
        let past_the_edge = SimulationState {
            position: Vec3::new(1.45, 1.0, 0.5),
            velocity: Vec3::new(0.1, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut remembered = SimulationState {
            previous_position: Some(Vec3::new(1.25, 1.0, 0.5)),
            ..past_the_edge
        };
        let mut forgotten = past_the_edge;

        crate::versioned::dispatch("1.21.1", &mut remembered, &world, PhysicsConfig::default())
            .unwrap();
        crate::versioned::dispatch("1.21.1", &mut forgotten, &world, PhysicsConfig::default())
            .unwrap();

        let drag = |friction: f64| java_float(java_float(friction) * java_float(0.91));
        assert_eq!(remembered.velocity.x, 0.1 * drag(0.98));
        assert_eq!(forgotten.velocity.x, 0.1 * drag(0.6));
    }

    fn wall_along_z(suffocating: Option<bool>) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for y in 0..2 {
            for z in -1..=1 {
                world.set_block(
                    (0, y, z),
                    BlockState {
                        id: 1,
                        shape: Some(BlockShape::full_block()),
                        suffocating,
                        ..BlockState::default()
                    },
                );
            }
        }
        world
    }

    fn pushed_out(world: &WorldSnapshot, version: &str, is_player: bool) -> SimulationState {
        let mut state = SimulationState {
            position: Vec3::new(0.2, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player,
            no_gravity: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default()).unwrap();
        state
    }

    #[test]
    fn players_inside_a_suffocating_block_move_toward_its_nearest_open_side() {
        let world = wall_along_z(None);

        let box_centre = ((0.2 - 0.3 + -0.1) + (0.2 + 0.3 + -0.1)) / 2.0;
        for (version, x) in [("1.15.2", box_centre), ("1.21.1", 0.2 + -0.1)] {
            let player = pushed_out(&world, version, true);
            assert_eq!(player.position.x, x, "{version}");
            assert_eq!(player.position.z, 0.5, "{version}");
        }
        assert_eq!(pushed_out(&world, "1.21.1", false).position.x, 0.2);
    }

    #[test]
    fn blocks_that_do_not_suffocate_leave_the_player_inside() {
        let world = wall_along_z(Some(false));

        assert_eq!(pushed_out(&world, "1.21.1", true).position.x, 0.2);
    }

    fn walking_east(world: &WorldSnapshot, version: &str, x: f64) -> SimulationState {
        let mut state = SimulationState {
            position: Vec3::new(x, 0.0, 0.5),
            velocity: Vec3::new(0.5, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default()).unwrap();
        state
    }

    #[test]
    fn the_default_world_border_stops_movement_at_its_edge() {
        let world = WorldSnapshot::new();

        for version in ["1.17", "1.21.1"] {
            let walker = walking_east(&world, version, 29999983.5);
            assert!(walker.collided_horizontally, "{version}");
            assert_eq!(
                walker.position.x,
                29999983.5 + (29999984.0 - (29999983.5 + 0.3))
            );
        }
    }

    #[test]
    fn a_world_border_collides_at_its_block_aligned_edges() {
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);

        let walker = walking_east(&world, "1.21.1", 4.5);

        assert!(walker.collided_horizontally);
        assert_eq!(walker.position.x, 4.5 + (5.0 - (4.5 + 0.3)));
    }

    #[test]
    fn before_1_13_one_layer_of_snow_is_a_floor_of_no_height() {
        // 1.8.9 BlockSnow.java:44-54 (yarn SnowLayerBlock) and 1.12.2 SnowLayerBlock.java:75-79: the
        // collision box is (layers - 1) * 0.125 high, a box of no height for one layer, which
        // AxisAlignedBB.calculateYOffset stops a fall at. From 1.13 Shapes.box makes it empty.
        let mut world = WorldSnapshot::new();
        let block = |height: f64| BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, height, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        world.set_block((0, 0, 0), block(0.875));
        world.set_block((0, 1, 0), block(0.0));
        for (version, y) in [("1.8.8", 1.0), ("1.12.2", 1.0), ("1.13.2", 0.875)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.05, 0.5),
                velocity: Vec3::new(0.0, -0.3, 0.0),
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.y, y, "{version}");
            assert!(state.on_ground, "{version}");
        }
    }

    #[test]
    fn before_1_13_a_sneaking_player_stays_on_one_layer_of_snow() {
        // 1.8.9 Entity.java:487 (1.12.2 :621-626): the sneak back-off tests the lowered box with
        // World.getCollisionBoxes, which before 1.13 holds the box of no height of one layer of
        // snow (1.12.2 SnowLayerBlock.java:75-79, Block.java:395-399): over air, the snow holds the
        // player up and it walks on. From 1.13 the snow has no collision and it backs off.
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for (version, walks_on) in [("1.8.8", true), ("1.12.2", true), ("1.13.2", false)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.2, 0.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                sneaking: true,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.x > 0.6, walks_on, "{version}");
        }
    }

    fn walking_by_the_border(
        world: &WorldSnapshot,
        version: &str,
        position: Vec3,
        velocity: Vec3,
        outside_border: bool,
    ) -> SimulationState {
        let mut state = SimulationState {
            position,
            velocity,
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            outside_border,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default()).unwrap();
        state
    }

    const LEGACY_BORDER_VERSIONS: [&str; 5] = ["1.8.8", "1.9.4", "1.11.2", "1.12.2", "1.13.2"];

    #[test]
    fn before_1_14_the_cells_outside_the_border_collide_as_stone_in_loaded_chunks_only() {
        // 1.8.9 World.java:886-920 (1.12.2 World.java:1127-1175, 1.13.2
        // IWorldReaderBase.java:112-156): a cell outside the border (WorldBorder.contains(BlockPos))
        // is stone for an entity inside it, only in a column whose chunk is loaded.
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);
        let east = Vec3::new(0.5, 0.0, 0.0);
        for version in LEGACY_BORDER_VERSIONS {
            let unloaded =
                walking_by_the_border(&world, version, Vec3::new(4.5, 0.0, 0.5), east, false);
            assert_eq!(unloaded.position.x, 4.5 + 0.5, "{version}");
        }
        world.set_chunk_loaded(0, 0, true);
        for version in LEGACY_BORDER_VERSIONS {
            let walker =
                walking_by_the_border(&world, version, Vec3::new(4.5, 0.0, 0.5), east, false);
            assert!(walker.collided_horizontally, "{version}");
            assert_eq!(walker.position.x, 4.5 + (5.0 - (4.5 + 0.3)), "{version}");
            assert!(!walker.outside_border, "{version}");
        }
    }

    #[test]
    fn before_1_14_an_entity_flagged_outside_the_border_must_be_a_block_inside_to_meet_it() {
        // 1.8.9 World.java:942-960 isInsideBorder: the border shrinks by 1 for an entity flagged
        // outside (Entity.isOutsideBorder) and grows by 1 for any other; World.java:904-908 sets
        // the flag to whether the entity was outside.
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);
        world.set_chunk_loaded(0, 0, true);
        let east = Vec3::new(1.0, 0.0, 0.0);
        for version in LEGACY_BORDER_VERSIONS {
            let flagged =
                walking_by_the_border(&world, version, Vec3::new(4.0, 0.0, 0.5), east, true);
            assert_eq!(flagged.position.x, 4.0 + 1.0, "{version}");
            assert!(flagged.outside_border, "{version}");
            let unflagged =
                walking_by_the_border(&world, version, Vec3::new(4.0, 0.0, 0.5), east, false);
            assert_eq!(unflagged.position.x, 4.0 + (5.0 - (4.0 + 0.3)), "{version}");
            assert!(!unflagged.outside_border, "{version}");
            let outside =
                walking_by_the_border(&world, version, Vec3::new(6.0, 0.0, 0.5), east, false);
            assert_eq!(outside.position.x, 6.0 + 1.0, "{version}");
            assert!(outside.outside_border, "{version}");
        }
    }

    #[test]
    fn before_1_14_an_entity_reaching_past_the_border_meets_the_cells_ahead_of_it() {
        // The stone cells are boxes of their own: one the entity reaches into does not collide
        // (AxisAlignedBB.calculateZOffset; 1.13.2 IWorldReaderBase.java:141 leaves it out), one
        // ahead does. From 1.14 (1.14 Entity.java collide) the border is a shape only for a box
        // that reaches into no part of it.
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);
        world.set_chunk_loaded(0, 0, true);
        let south = Vec3::new(0.0, 0.0, 0.5);
        for version in LEGACY_BORDER_VERSIONS {
            let walker =
                walking_by_the_border(&world, version, Vec3::new(4.9, 0.0, 0.5), south, false);
            assert!(walker.collided_horizontally, "{version}");
            assert_eq!(walker.position.z, 0.5 + (1.0 - (0.5 + 0.3)), "{version}");
        }
        let modern =
            walking_by_the_border(&world, "1.14.4", Vec3::new(4.9, 0.0, 0.5), south, false);
        assert_eq!(modern.position.z, 0.5 + 0.5);
    }

    #[test]
    fn sneaking_backs_off_from_a_drop_of_a_block_until_1_11() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, -1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (1, -1, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.25, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let moved_x = |release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(1.25, 0.0, 0.5),
                velocity: Vec3::new(0.3, 0.0, 0.0),
                on_ground: true,
                is_player: true,
                sneaking: true,
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state.position.x - 1.25
        };

        assert!(moved_x(MinecraftVersion::new(1, 10, 2)) > 0.29);
        assert_eq!(moved_x(MinecraftVersion::new(1, 11, 2)), 0.0);
    }

    #[test]
    fn sneaking_spectators_do_not_back_off_from_edges() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            on_ground: true,
            is_player: true,
            sneaking: true,
            no_physics: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.x > 0.0);
    }

    #[test]
    fn sprinting_keeps_its_speed_modifier_while_sneaking() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.08 * java_float(0.98), 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            on_ground: true,
            is_player: true,
            sprinting: true,
            sneaking: true,
            moving_slowly: Some(false),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        let sprint_speed = java_float(0.1 * (1.0 + 0.30000001192092896));
        assert_eq!(state.position.x, 0.5 + java_float(0.98) * sprint_speed);
    }

    #[test]
    fn a_sprint_without_its_modifier_moves_at_the_walking_speed() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.08 * java_float(0.98), 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            on_ground: true,
            is_player: true,
            sprinting: true,
            sprint_modifier: Some(false),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.position.x, 0.5 + java_float(0.98) * java_float(0.1));
    }

    #[test]
    fn climbing_jumps_up_when_requested_or_blocked() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            jump_requested: true,
            climbing: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert_eq!(state.velocity.y, (0.2 - 0.08) * java_float(0.98));
    }

    #[test]
    fn climbing_limits_horizontal_and_downward_velocity() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            velocity: Vec3::new(0.5, -0.5, -0.5),
            climbing: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        let limit = java_float(0.15);
        assert_eq!(state.position, Vec3::new(limit, 2.0 - limit, -limit));
    }

    #[test]
    fn standing_on_top_of_a_ladder_is_not_climbing() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 3.0, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.position.y, 2.5);
    }

    #[test]
    fn jumping_on_a_ladder_climbs_only_from_1_14() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 99, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 100, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for (version, rise) in [("1.13.2", f64::from(0.42_f32)), ("1.14.4", 0.2)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 100.0, 0.5),
                on_ground: true,
                jump_requested: true,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(
                state.velocity.y,
                (rise - 0.08) * f64::from(0.98_f32),
                "{version}"
            );
        }
    }

    #[test]
    fn ladder_falls_are_clamped_to_a_double_before_1_14() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 100, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for (version, clamp) in [("1.13.2", 0.15), ("1.14.4", f64::from(0.15_f32))] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 100.5, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.y, 100.5 - clamp, "{version}");
        }
    }

    #[test]
    fn sneaking_player_stops_sliding_down_ladder_but_not_scaffolding() {
        let mut ladder_world = WorldSnapshot::new();
        ladder_world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut ladder_state = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            is_player: true,
            sneaking: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        tick(&mut ladder_state, &ladder_world, PhysicsConfig::default());
        assert_eq!(ladder_state.position.y, 2.0);
    }

    #[test]
    fn leaving_climbable_block_does_not_persist_world_derived_climbing() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());
        assert!(!state.climbing);
    }

    #[test]
    fn water_jump_input_adds_vanilla_vertical_impulse() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            jump_requested: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.y > 0.02);
    }

    fn jump_on_ground_in_fluid(
        fluid: FluidState,
        fluid_level: u8,
        eye_height: f64,
        release: MinecraftVersion,
    ) -> f64 {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid,
                fluid_level,
                fluid_source: fluid_level == 0,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            jump_requested: true,
            in_lava: fluid == FluidState::Lava,
            half_width: 0.3,
            height: 1.8,
            eye_height: Some(eye_height),
            ..SimulationState::default()
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
        state.velocity.y
    }

    #[test]
    fn grounded_entities_jump_out_of_fluid_up_to_the_jump_threshold() {
        let release = MinecraftVersion::new(1, 21, 1);
        let jumped = java_float(0.42) * java_float(0.8) - 0.08 / 16.0;
        let swam = 0.03999999910593033 * java_float(0.8) - 0.08 / 16.0;

        assert_eq!(
            jump_on_ground_in_fluid(FluidState::Water, 6, 1.62, release),
            jumped
        );
        assert_eq!(
            jump_on_ground_in_fluid(FluidState::Water, 0, 1.62, release),
            swam
        );
        assert_eq!(
            jump_on_ground_in_fluid(FluidState::Water, 6, 0.3, release),
            swam
        );
    }

    #[test]
    fn sneaking_players_sink_in_water_before_the_motion_cutoff() {
        let mut world = WorldSnapshot::new();
        for y in 0..3 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
        }
        let sink = |is_player: bool, flying: bool, release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player,
                sneaking: true,
                flying,
                no_gravity: true,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state.velocity.y
        };
        let modern = MinecraftVersion::new(1, 21, 1);

        assert_eq!(
            sink(true, false, modern),
            -0.03999999910593033 * java_float(0.8)
        );
        assert_eq!(sink(false, false, modern), 0.0);
        assert_eq!(sink(true, true, modern), 0.0);
        assert!(sink(true, true, MinecraftVersion::new(1, 15, 2)) < 0.0);
    }

    #[test]
    fn depth_strider_sets_the_water_efficiency_until_1_21() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let walk = |version: &str, depth_strider: u8| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                is_player: true,
                input: Vec3::new(0.0, 0.0, 1.0),
                movement_speed: Some(0.1),
                depth_strider,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .expect("the version is supported");
            state.velocity
        };

        assert_eq!(walk("1.20.5", 3), walk("1.20.6", 3));
        assert!(walk("1.20.5", 3).z > walk("1.20.5", 0).z);
    }

    #[test]
    fn grounded_entities_swim_up_in_any_lava_before_1_16() {
        let shallow_lava = |release| jump_on_ground_in_fluid(FluidState::Lava, 6, 1.62, release);

        assert!(shallow_lava(MinecraftVersion::new(1, 15, 2)) < 0.05);
        assert!(shallow_lava(MinecraftVersion::new(1, 16, 0)) > 0.3);
    }

    #[test]
    fn attributes_and_effects_modify_movement() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 2.0, 0.0),
            input: Vec3::new(1.0, 0.0, 0.0),
            movement_speed: Some(0.2),
            speed_effect_amplifier: Some(0),
            slowness_effect_amplifier: Some(0),
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.x > 0.15);
        assert!(state.position.x < 0.25);
    }

    #[test]
    fn jump_strength_and_boost_override_defaults() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 1.0, 0.0),
            on_ground: true,
            jump_requested: true,
            jump_strength: Some(0.5),
            jump_boost_amplifier: Some(1),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.velocity.y > 0.5);
    }

    #[test]
    fn maximum_jump_boost_amplifier_does_not_overflow() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 1.0, 0.0),
            on_ground: true,
            jump_requested: true,
            jump_boost_amplifier: Some(255),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.velocity.y.is_finite());
        assert!(state.velocity.y > 25.0);
    }

    #[test]
    fn computes_fall_damage_on_long_landing() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 10.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.fall_damage > 0.0);
    }

    #[test]
    fn fall_damage_immunity_tag_suppresses_calculated_damage() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 10.0,
            fall_damage_immune: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.fall_damage, 0.0);
    }

    #[test]
    fn safe_fall_distance_reduces_landing_damage() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 10.0,
            safe_fall_distance: 4.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        crate::versioned::dispatch("1.20.5", &mut state, &world, PhysicsConfig::default())
            .unwrap();

        // ceil((10.0F - 4.0F) * 1.0F * 1.0) (1.20.5 LivingEntity.java:1607-1609), the drop of
        // the landing tick not counted before 1.21.5
        assert_eq!(state.fall_damage, 6.0);
    }

    #[test]
    fn safe_fall_distance_is_the_whole_threshold_in_1_20_6() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 5.0,
            safe_fall_distance: 3.0,
            jump_boost_amplifier: Some(0),
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        crate::versioned::dispatch("1.20.6", &mut state, &world, PhysicsConfig::default())
            .unwrap();

        // 1.20.6 has 1.20.5's computeFallDamage: ceil(5.0F - 3.0F), Jump Boost only through the
        // attribute (1.20.5 LivingEntity.java:1602-1609; 1.21 :1575-1582 the same)
        assert_eq!(state.fall_damage, 2.0);
    }

    #[test]
    fn landing_tick_drop_counts_toward_the_damage_from_1_21_5() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let falling = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 4.1,
            safe_fall_distance: 3.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        // 1.21.5 Entity.java:1284-1297: fd -= (float)dy first, then fallOn with it:
        // floor(4.1 + 0.9 + 1.0E-6 - 3.0) = 2, where landing on fd = 4.1 alone gives 1
        let mut state = falling;
        crate::versioned::dispatch("1.21.5", &mut state, &world, PhysicsConfig::default())
            .unwrap();
        assert_eq!(state.fall_damage, 2.0);
        assert_eq!(state.fall_distance, 0.0);
        let mut state = SimulationState {
            fall_distance: 4.0,
            ..falling
        };
        crate::versioned::dispatch("1.21.5", &mut state, &world, PhysicsConfig::default())
            .unwrap();
        assert_eq!(state.fall_damage, 1.0);

        // 1.21.4 Entity.java:1259-1274: the landing tick does not count its drop, so
        // ceil(4.0F - 3.0F) = 1
        let mut state = SimulationState {
            fall_distance: 4.0,
            ..falling
        };
        crate::versioned::dispatch("1.21.4", &mut state, &world, PhysicsConfig::default())
            .unwrap();
        assert_eq!(state.fall_damage, 1.0);
    }

    #[test]
    fn modern_fall_damage_uses_safe_fall_distance_without_legacy_offset() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 6.0,
            safe_fall_distance: 3.0,
            fall_damage_multiplier: Some(1.0),
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        crate::simulation::tick_with_profile_and_restitution::<crate::versioned::v26_2::Profile>(
            &mut state,
            &world,
            PhysicsConfig::default(),
        );

        assert_eq!(state.fall_damage, 3.0);
    }

    #[test]
    fn movement_ray_resets_fall_distance_on_resetting_block() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                fall_damage_resetting: true,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            velocity: Vec3::new(0.0, -2.0, 0.0),
            fall_distance: 5.0,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.fall_distance, 0.0);
    }

    fn falling_through(world: &WorldSnapshot, version: &str, y: f64, drop: f64) -> f64 {
        let mut state = SimulationState {
            position: Vec3::new(0.5, y, 0.5),
            velocity: Vec3::new(0.0, -drop, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 5.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default()).unwrap();
        state.fall_distance
    }

    #[test]
    fn fall_damage_resetting_blocks_are_full_cubes_and_the_drop_still_counts() {
        // a vine: no collision box, only the #fall_damage_resetting tag
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                fall_damage_resetting: true,
                ..BlockState::default()
            },
        );

        // 1.19 Entity.java:571-580: FALLDAMAGE_RESETTING gives the block a full cube
        // (RaycastContext.java:72), the reset comes before checkFallDamage adds this move's drop
        assert_eq!(falling_through(&world, "1.19", 3.0, 2.0), 2.0);
        assert_eq!(falling_through(&world, "1.18.2", 3.0, 2.0), 2.0);
        assert_eq!(falling_through(&world, "26.2", 3.0, 2.0), 2.0);
        // 1.18.1 has no clip (1.18.1 Entity.java move)
        assert_eq!(falling_through(&world, "1.18.1", 3.0, 2.0), 7.0);
    }

    #[test]
    fn water_the_move_passes_through_resets_the_fall_distance() {
        // the clip's ClipContext.Fluid.WATER (1.19 Entity.java:574-576)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 3, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );

        assert_eq!(falling_through(&world, "1.21.4", 4.5, 4.0), 4.0);
        assert_eq!(falling_through(&world, "26.2", 4.5, 4.0), 4.0);
        assert_eq!(falling_through(&world, "1.18.1", 4.5, 4.0), 9.0);
    }

    #[test]
    fn fall_damage_resetting_clip_reaches_8_blocks_from_1_21_9() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                fall_damage_resetting: true,
                ..BlockState::default()
            },
        );

        // 1.21.9 Entity.java:704-705: min(length, 8.0) along the move; the block is 9 down
        assert_eq!(falling_through(&world, "1.21.9", 12.0, 11.0), 16.0);
        assert_eq!(falling_through(&world, "1.21.8", 12.0, 11.0), f64::from(11.0_f32));
    }

    #[test]
    fn end_portals_reset_a_players_fall_from_1_21_9() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                player_fall_damage_resetting: true,
                ..BlockState::default()
            },
        );
        let fall = |version: &str, is_player: bool| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 3.0, 0.5),
                velocity: Vec3::new(0.0, -2.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                is_player,
                fall_distance: 5.0,
                controlled_by_local_instance: true,

                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.fall_distance
        };

        // 1.21.9 ClipContext.java:65-68: END_GATEWAY and END_PORTAL are full cubes for a player
        assert_eq!(fall("1.21.9", true), 2.0);
        assert_eq!(fall("1.21.9", false), 7.0);
        assert_eq!(fall("1.21.8", true), 7.0);
    }

    #[test]
    fn lava_halves_the_fall_distance_every_tick() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let in_lava = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 4.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        // Entity.baseTick: fallDistance *= 0.5F in lava (1.8.9 Entity.java:410, 1.16.5 :523,
        // 1.21.4 :487), *= 0.5 once a double (1.21.5 :490)
        for version in ["1.8.8", "1.12.2", "1.16.5", "1.21.4", "26.2"] {
            let mut state = in_lava;
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.in_lava, "{version}");
            assert_eq!(state.fall_distance, 2.0, "{version}");
        }
    }

    #[test]
    fn short_movement_does_not_reset_fall_distance() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fall_damage_resetting: true,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.8, 0.5),
            velocity: Vec3::new(0.0, -0.2, 0.0),
            fall_distance: 5.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.fall_distance > 5.0);
    }

    #[test]
    fn landing_damage_rounds_up_like_vanilla() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 6.1,
            safe_fall_distance: 3.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        crate::versioned::dispatch("1.20.4", &mut state, &world, PhysicsConfig::default())
            .unwrap();

        // ceil(6.1F - 3.0F - 0.0F) (1.20.4 LivingEntity.java:1564)
        assert_eq!(state.fall_damage, 4.0);
    }

    #[test]
    fn landing_damage_applies_multiplier_before_rounding() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.9, 0.5),
            velocity: Vec3::new(0.0, -1.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            fall_distance: 6.1,
            fall_damage_multiplier: Some(0.5),
            safe_fall_distance: 3.0,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        crate::versioned::dispatch("1.20.4", &mut state, &world, PhysicsConfig::default())
            .unwrap();

        // ceil((6.1F - 3.0F) * 0.5F) (1.20.4 LivingEntity.java:1564)
        assert_eq!(state.fall_damage, 2.0);
    }

    #[test]
    fn fall_flying_glides_with_bounded_descent() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 20.0, 0.0),
            velocity: Vec3::ZERO,
            fall_flying: true,
            look: Vec3::new(0.0, -0.5, 1.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.z > 0.0);
        assert!(state.velocity.y > -0.5);
    }

    #[test]
    fn fall_flying_redirects_velocity_toward_look_direction() {
        let mut state = SimulationState {
            velocity: Vec3::new(1.0, 0.0, 0.0),
            fall_flying: true,
            look: Vec3::new(0.0, 0.0, 1.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.velocity.z > 0.0);
        assert!(state.velocity.x < 1.0);
    }

    #[test]
    fn creative_flying_preserves_and_scales_vertical_velocity() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState {
            flying: true,
            is_player: true,
            velocity: Vec3::new(0.0, 0.5, 0.0),
            fall_distance: 12.0,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!((state.velocity.y - 0.3).abs() < 1e-12);
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn creative_flying_uses_flying_speed_for_horizontal_input() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState {
            flying: true,
            flying_speed: 0.05,
            movement_speed: Some(0.2),
            input: Vec3::new(1.0, 0.0, 0.0),
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.x.abs() < 0.05);
    }

    #[test]
    fn creative_flying_uses_jump_and_sneak_for_vertical_input() {
        let world = WorldSnapshot::new();
        let mut ascending = SimulationState {
            flying: true,
            flying_speed: 0.05,
            jump_requested: true,
            ..SimulationState::default()
        };
        tick(&mut ascending, &world, PhysicsConfig::default());

        let mut descending = SimulationState {
            flying: true,
            flying_speed: 0.05,
            sneaking: true,
            ..SimulationState::default()
        };
        tick(&mut descending, &world, PhysicsConfig::default());

        assert!(ascending.velocity.y > 0.0);
        assert!(descending.velocity.y < 0.0);
    }

    #[test]
    fn creative_flight_adds_the_vertical_impulse_in_float() {
        let world = WorldSnapshot::new();
        let flying = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: java_float(0.05),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let impulse = f64::from(0.05_f32 * 3.0);

        let mut ascending = SimulationState {
            jump_requested: true,
            ..flying
        };
        tick(&mut ascending, &world, PhysicsConfig::default());
        let mut descending = SimulationState {
            sneaking: true,
            ..flying
        };
        tick(&mut descending, &world, PhysicsConfig::default());
        let mut both = SimulationState {
            jump_requested: true,
            sneaking: true,
            velocity: Vec3::new(0.0, 0.25, 0.0),
            ..flying
        };
        tick(&mut both, &world, PhysicsConfig::default());

        assert_eq!(ascending.velocity.y, impulse * 0.6);
        assert_eq!(ascending.position.y, impulse);
        assert_eq!(descending.velocity.y, -impulse * 0.6);
        assert_eq!(both.velocity.y, 0.25 * 0.6);
    }

    #[test]
    fn creative_flight_keeps_its_vertical_motion_through_collisions() {
        let mut world = WorldSnapshot::new();
        for y in [0, 3] {
            world.set_block(
                (0, y, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        let flying = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: java_float(0.05),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut ascending = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, 0.1, 0.0),
            jump_requested: true,
            ..flying
        };
        tick(&mut ascending, &world, PhysicsConfig::default());
        let mut landing = SimulationState {
            position: Vec3::new(0.5, 1.05, 0.5),
            velocity: Vec3::new(0.0, -0.1, 0.0),
            sneaking: true,
            ..flying
        };
        tick(&mut landing, &world, PhysicsConfig::default());

        let impulse = f64::from(0.05_f32 * 3.0);
        assert!(ascending.collided_vertically);
        assert!(ascending.position.y < 1.25);
        assert_eq!(ascending.velocity.y, (0.1 + impulse) * 0.6);
        assert!(landing.on_ground);
        assert!(landing.position.y > 0.9);
        assert_eq!(landing.velocity.y, (-0.1 - impulse) * 0.6);
    }

    #[test]
    fn creative_flight_resets_the_fall_before_travel_from_1_21_2() {
        let descend = |release: MinecraftVersion| {
            let mut state = SimulationState {
                is_player: true,
                flying: true,
                flying_speed: java_float(0.05),
                sneaking: true,
                position: Vec3::new(0.5, 10.0, 0.5),
                fall_distance: 5.0,
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,

                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.fall_distance
        };

        assert_eq!(descend(MinecraftVersion::new(1, 21, 1)), 0.0);
        assert_eq!(
            descend(MinecraftVersion::new(1, 21, 2)),
            f64::from(0.05_f32 * 3.0)
        );
        assert_eq!(
            descend(MinecraftVersion::new(26, 2, 0)),
            f64::from(0.05_f32 * 3.0)
        );
    }

    #[test]
    fn creative_flight_keeps_the_fall_distance_in_1_8() {
        use crate::simulation::accumulated_fall_distance;
        // Player.travel's flight resets fallDistance from 1.9 (1.9.4 PlayerEntity.java:1403-1411);
        // 1.8 only scales the vertical motion by 0.6 (1.8.9 PlayerEntity.java:1303-1309)
        let descend = |version: &str| {
            let mut state = SimulationState {
                is_player: true,
                flying: true,
                flying_speed: java_float(0.05),
                sneaking: true,
                position: Vec3::new(0.5, 10.0, 0.5),
                fall_distance: 5.0,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let world = WorldSnapshot::new();
            let result =
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                    .unwrap();
            (state.fall_distance, result.displacement.y)
        };

        let (fall_distance, dy) = descend("1.8.8");
        assert!(dy < 0.0);
        assert_eq!(
            fall_distance,
            accumulated_fall_distance(5.0, dy, MinecraftVersion::new(1, 8, 8))
        );
        assert_eq!(descend("1.9.4").0, 0.0);
        assert_eq!(descend("1.12.2").0, 0.0);
    }

    #[test]
    fn elytra_raises_the_fall_distance_to_one_before_1_19_3() {
        use crate::simulation::accumulated_fall_distance;
        // LivingEntity.travel's glide: `if (vy > -0.5) fallDistance = 1.0F` for 1.9 - 1.19.2 (1.9.4
        // LivingEntity.java:1328-1329, 1.16.5 :2028-2029, 1.19.2 :2107-2109); from 1.19.3
        // checkSlowFallDistance only lowers a larger one (1.19.3 Entity.java:2156-2158)
        let glide = |version: &str, fall_distance: f64| {
            let mut state = SimulationState {
                is_player: true,
                fall_flying: true,
                position: Vec3::new(0.5, 20.0, 0.5),
                velocity: Vec3::new(0.0, -0.1, 0.5),
                look: Vec3::new(0.0, 0.0, 1.0),
                fall_distance,
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            let world = WorldSnapshot::new();
            let result =
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                    .unwrap();
            (state.fall_distance, result.displacement.y)
        };

        for (version, release) in [
            ("1.9.4", MinecraftVersion::new(1, 9, 4)),
            ("1.12.2", MinecraftVersion::new(1, 12, 2)),
            ("1.16.5", MinecraftVersion::new(1, 16, 5)),
            ("1.19.2", MinecraftVersion::new(1, 19, 2)),
        ] {
            for start in [0.25, 5.0] {
                let (fall_distance, dy) = glide(version, start);
                assert!(dy < 0.0, "{version}");
                assert_eq!(fall_distance, accumulated_fall_distance(1.0, dy, release), "{version}");
            }
        }
        for (version, release) in [
            ("1.19.3", MinecraftVersion::new(1, 19, 3)),
            ("1.21.1", MinecraftVersion::new(1, 21, 1)),
            ("1.21.2", MinecraftVersion::new(1, 21, 2)),
            ("26.3", MinecraftVersion::new(26, 3, 0)),
        ] {
            let (fall_distance, dy) = glide(version, 0.25);
            assert_eq!(fall_distance, accumulated_fall_distance(0.25, dy, release), "{version}");
            let (fall_distance, dy) = glide(version, 5.0);
            assert_eq!(fall_distance, accumulated_fall_distance(1.0, dy, release), "{version}");
        }
    }

    #[test]
    fn fall_flying_checks_the_fall_distance_in_lava_from_1_21_2() {
        // LivingEntity.aiStep -> updateFallFlying -> checkFallDistanceAccumulation every tick the
        // flag is set, in any fluid, from 1.21.2 (1.21.2 LivingEntity.java:2796, 2865; 1.21.11
        // :3021-3026); until 1.21.1 only the glide out of fluids checks it (1.21.1 :2179)
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let in_lava = SimulationState {
            is_player: true,
            fall_flying: true,
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 4.0,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };

        // Entity.baseTick halves it first (1.21.4 Entity.java:487)
        for (version, expected) in [("1.21.1", 2.0), ("1.21.2", 1.0), ("1.21.5", 1.0), ("26.3", 1.0)] {
            let mut state = in_lava;
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.in_lava, "{version}");
            assert_eq!(state.fall_distance, expected, "{version}");
        }
    }

    #[test]
    fn fall_flying_check_sees_the_firework_boost() {
        use crate::simulation::accumulated_fall_distance;
        // FireworkRocketEntity.tick boosts the player before its own tick, so checkSlowFallDistance
        // (1.21.1 LivingEntity.java:2179 in travel; 1.21.2 :2796, 2865 in aiStep) sees vy = -0.4 and
        // lowers the fall distance to 1.0 (1.21.2 Entity.java checkSlowFallDistance: vy > -0.5)
        let glide = |version: &str, boost_y: f64| {
            let mut state = SimulationState {
                is_player: true,
                fall_flying: true,
                position: Vec3::new(0.5, 20.0, 0.5),
                velocity: Vec3::new(0.0, -0.6, 0.5),
                firework_boost: Vec3::new(0.0, boost_y, 0.0),
                look: Vec3::new(0.0, 0.0, 1.0),
                fall_distance: 5.0,
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            let world = WorldSnapshot::new();
            let result =
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                    .unwrap();
            (state.fall_distance, result.displacement.y)
        };

        for (version, release) in [
            ("1.21.1", MinecraftVersion::new(1, 21, 1)),
            ("1.21.2", MinecraftVersion::new(1, 21, 2)),
            ("1.21.5", MinecraftVersion::new(1, 21, 5)),
            ("26.3", MinecraftVersion::new(26, 3, 0)),
        ] {
            let (fall_distance, dy) = glide(version, 0.2);
            assert!(dy < 0.0, "{version}");
            assert_eq!(fall_distance, accumulated_fall_distance(1.0, dy, release), "{version}");
            let (fall_distance, dy) = glide(version, 0.0);
            assert_eq!(fall_distance, accumulated_fall_distance(5.0, dy, release), "{version}");
        }
    }

    #[test]
    fn slow_falling_and_levitation_reset_the_fall_distance_by_the_release() {
        use crate::simulation::accumulated_fall_distance;
        let fall = |version: &str, slow_falling: bool, velocity_y: f64| {
            let mut state = SimulationState {
                is_player: true,
                slow_falling,
                levitation: !slow_falling,
                position: Vec3::new(0.5, 20.0, 0.5),
                velocity: Vec3::new(0.0, velocity_y, 0.0),
                fall_distance: 5.0,
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            let world = WorldSnapshot::new();
            let result =
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                    .unwrap();
            (state.fall_distance, result.displacement.y)
        };

        // until 1.19.4 travel resets it under Slow Falling only when not rising, before the move
        // (1.13.2 LivingEntity.java:1511-1513, 1.19.4 :1986-1990), and under Levitation after the
        // air move (1.13.2 :1609-1612, 1.19.4 :2093-2095); 1.9 - 1.12 Levitation never resets it
        // (1.12.2 LivingEntity.java:1543-1545)
        for (version, release) in [
            ("1.13.2", MinecraftVersion::new(1, 13, 2)),
            ("1.16.5", MinecraftVersion::new(1, 16, 5)),
            ("1.19.4", MinecraftVersion::new(1, 19, 4)),
        ] {
            assert_eq!(fall(version, true, 0.2).0, 5.0, "{version}");
            let (fall_distance, dy) = fall(version, true, -0.5);
            assert_eq!(fall_distance, accumulated_fall_distance(0.0, dy, release), "{version}");
            assert_eq!(fall(version, false, -0.5).0, 0.0, "{version}");
        }
        let (fall_distance, dy) = fall("1.12.2", false, -0.5);
        assert_eq!(
            fall_distance,
            accumulated_fall_distance(5.0, dy, MinecraftVersion::new(1, 12, 2))
        );
        // from 1.20 aiStep resets it for either effect before travel (1.20
        // LivingEntity.java:2566-2567, 1.21.5 :2756-2758)
        for (version, release) in [
            ("1.20", MinecraftVersion::new(1, 20, 0)),
            ("1.21.5", MinecraftVersion::new(1, 21, 5)),
            ("26.3", MinecraftVersion::new(26, 3, 0)),
        ] {
            assert_eq!(fall(version, true, 0.2).0, 0.0, "{version}");
            let (fall_distance, dy) = fall(version, false, -0.5);
            assert!(dy < 0.0, "{version}");
            assert_eq!(fall_distance, accumulated_fall_distance(0.0, dy, release), "{version}");
        }
    }

    #[test]
    fn levitation_resets_the_fall_distance_after_the_landing_until_1_19_4() {
        // a landing under Levitation hurts until 1.19.4, whose reset follows the move (1.19.4
        // LivingEntity.java:2093-2095); from 1.20 the reset before travel spares it (1.20 :2566)
        assert_eq!(levitating_landing("1.19.4"), (17.0, 0.0));
        assert_eq!(levitating_landing("1.20"), (0.0, 0.0));
    }

    fn levitating_landing(version: &str) -> (f64, f64) {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            levitation: true,
            no_gravity: true,
            fall_distance: 20.0,
            safe_fall_distance: 3.0,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
        (state.fall_damage, state.fall_distance)
    }

    #[test]
    fn creative_flight_ends_elytra_flight_after_travel_until_1_21_2() {
        let glide = |release: MinecraftVersion| {
            let mut state = SimulationState {
                is_player: true,
                flying: true,
                fall_flying: true,
                flying_speed: java_float(0.05),
                position: Vec3::new(0.5, 10.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.fall_flying
        };

        assert!(!glide(MinecraftVersion::new(1, 21, 1)));
        assert!(glide(MinecraftVersion::new(1, 21, 2)));
    }

    #[test]
    fn spectators_are_never_on_the_ground() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let flying = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: java_float(0.05),
            movement_speed: Some(0.1),
            input: Vec3::new(0.0, 0.0, 1.0),
            position: Vec3::new(0.5, 1.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut spectating = SimulationState {
            on_ground: true,
            no_physics: true,
            ..flying
        };
        tick(&mut spectating, &world, PhysicsConfig::default());
        let mut airborne = flying;
        tick(&mut airborne, &world, PhysicsConfig::default());
        let mut grounded = SimulationState {
            on_ground: true,
            ..flying
        };
        tick(&mut grounded, &world, PhysicsConfig::default());

        assert!(!spectating.on_ground);
        assert_eq!(spectating.velocity.z, airborne.velocity.z);
        assert_ne!(grounded.velocity.z, airborne.velocity.z);
    }

    #[test]
    fn sneaking_holds_a_flying_player_on_a_ladder_only_before_1_16() {
        let descent = |release: MinecraftVersion| {
            let mut state = SimulationState {
                is_player: true,
                flying: true,
                flying_speed: java_float(0.05),
                sneaking: true,
                climbing: true,
                position: Vec3::new(0.5, 10.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            10.0 - state.position.y
        };

        assert_eq!(descent(MinecraftVersion::new(1, 15, 2)), 0.0);
        assert_eq!(
            descent(MinecraftVersion::new(1, 16, 0)),
            f64::from(0.05_f32 * 3.0)
        );
    }

    #[test]
    fn flying_players_jump_from_the_ground_before_1_16() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let rise = |release: MinecraftVersion| {
            let mut state = SimulationState {
                is_player: true,
                flying: true,
                flying_speed: java_float(0.05),
                jump_requested: true,
                on_ground: true,
                position: Vec3::new(0.5, 1.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            (state.velocity.y, state.jump_ticks)
        };

        assert_eq!(
            rise(MinecraftVersion::new(1, 15, 2)),
            (java_float(0.42) * 0.6, 10)
        );
        assert_eq!(
            rise(MinecraftVersion::new(1, 16, 0)),
            (f64::from(0.05_f32 * 3.0) * 0.6, 0)
        );
    }

    #[test]
    fn creative_flight_ends_on_landing_but_spectators_keep_flying() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let flying = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: java_float(0.05),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut landing = SimulationState {
            position: Vec3::new(0.5, 1.05, 0.5),
            sneaking: true,
            ..flying
        };
        tick(&mut landing, &world, PhysicsConfig::default());
        let mut hovering = SimulationState {
            position: Vec3::new(0.5, 3.0, 0.5),
            ..flying
        };
        tick(&mut hovering, &world, PhysicsConfig::default());
        let mut spectating = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            no_physics: true,
            ..flying
        };
        tick(&mut spectating, &world, PhysicsConfig::default());

        assert!(landing.on_ground);
        assert!(!landing.flying);
        assert!(hovering.flying);
        assert!(spectating.flying);
    }

    #[test]
    fn creative_flight_adds_the_impulse_before_negligible_motion_is_cut() {
        let world = WorldSnapshot::new();
        let mut state = SimulationState {
            is_player: true,
            flying: true,
            flying_speed: java_float(0.05),
            jump_requested: true,
            velocity: Vec3::new(0.0, 0.002, 0.0),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.velocity.y, (0.002 + f64::from(0.05_f32 * 3.0)) * 0.6);
    }

    #[test]
    fn firework_boost_adds_velocity() {
        let mut state = SimulationState {
            position: Vec3::new(0.0, 20.0, 0.0),
            firework_boost: Vec3::new(0.0, 0.2, 0.3),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());

        assert!(state.position.z > 0.2);
        assert!(state.velocity.y > 0.0);
    }

    #[test]
    fn steps_over_low_obstacle_when_grounded() {
        let mut world = WorldSnapshot::new();
        for position in [(0, 0, 0), (1, 0, 0)] {
            world.set_block(
                position,
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }
        world.set_block(
            (1, 1, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: crate::Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.position.x > 0.8);
        assert!(state.position.y >= 1.0);
    }

    #[test]
    fn block_effects_modify_native_movement() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        assert_eq!(world.effect_at((0, 0, 0)), crate::world::BlockEffect::Slime);
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.2, 0.0),
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.velocity.y > 0.0);
    }

    #[test]
    fn stuck_blocks_slow_the_next_move_and_stop_the_motion() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let web = Vec3::new(0.25, java_float(0.05), 0.25);
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(1.0, 1.0, 1.0),
            stuck_speed_multiplier: web,
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 4.0,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.position, Vec3::new(0.5, 1.5, 0.5) + web);
        assert_eq!(state.velocity, Vec3::ZERO);
        assert_eq!(state.stuck_speed_multiplier, web);
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn entering_a_cobweb_slows_only_the_next_move() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(0.1, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.position.x, 0.6);
        assert!(state.in_web);
        assert_eq!(
            state.stuck_speed_multiplier,
            Vec3::new(0.25, java_float(0.05), 0.25)
        );
    }

    fn bubble_column(effect: BlockEffect) -> BlockState {
        BlockState {
            id: 2,
            fluid: FluidState::Water,
            fluid_source: true,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                effect,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    fn bubble_push(
        world: &WorldSnapshot,
        state: SimulationState,
        release: MinecraftVersion,
    ) -> f64 {
        let mut state = SimulationState {
            half_width: 0.3,
            no_gravity: true,
            is_player: true,
            ..state
        };
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, world, PhysicsConfig::default(), rules);
        state.velocity.y
    }

    #[test]
    fn every_bubble_column_block_in_the_box_pushes_where_vanilla_applies_block_effects() {
        let mut world = WorldSnapshot::new();
        for y in 0..3 {
            world.set_block((0, y, 0), bubble_column(BlockEffect::BubbleUp));
        }
        world.set_block(
            (0, 3, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let submerged = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            height: 1.8,
            ..SimulationState::default()
        };

        assert_eq!(
            bubble_push(&world, submerged, MinecraftVersion::new(1, 21, 1)),
            (0.06 + 0.06 + 0.06) * java_float(0.8)
        );
        assert_eq!(
            bubble_push(&world, submerged, MinecraftVersion::new(1, 21, 2)),
            0.06 + 0.06 + 0.06
        );
        let flying = SimulationState {
            flying: true,
            ..submerged
        };
        assert_eq!(
            bubble_push(&world, flying, MinecraftVersion::new(1, 21, 1)),
            0.0
        );
        assert_eq!(
            bubble_push(&world, flying, MinecraftVersion::new(1, 21, 2)),
            0.0
        );
    }

    #[test]
    fn the_bubble_column_top_under_air_pushes_harder() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(BlockEffect::BubbleUp));
        world.set_block((0, 1, 0), bubble_column(BlockEffect::BubbleUp));
        world.set_block((1, 0, 0), bubble_column(BlockEffect::BubbleDown));
        world.set_block((1, 1, 0), bubble_column(BlockEffect::BubbleDown));
        let release = MinecraftVersion::new(1, 21, 2);
        let at = |x: f64| SimulationState {
            position: Vec3::new(x, 0.5, 0.5),
            height: 1.8,
            ..SimulationState::default()
        };

        assert_eq!(bubble_push(&world, at(0.5), release), 0.06 + 0.1);
        assert_eq!(bubble_push(&world, at(1.5), release), -0.03 - 0.03);
    }

    #[test]
    fn bubble_columns_push_from_each_axis_step_of_the_move_from_1_21_5() {
        let mut world = WorldSnapshot::new();
        for x in 0..2 {
            world.set_block((x, 0, 0), bubble_column(BlockEffect::BubbleUp));
            world.set_block((x, 1, 0), bubble_column(BlockEffect::BubbleUp));
        }
        let leaving = SimulationState {
            position: Vec3::new(0.9, 1.2, 0.5),
            velocity: Vec3::new(0.5, -0.5, 0.0),
            height: 1.8,
            ..SimulationState::default()
        };
        let dragged = -0.5 * java_float(0.8);

        assert_eq!(
            bubble_push(&world, leaving, MinecraftVersion::new(1, 21, 4)),
            dragged + 0.06 + 0.1
        );
        assert_eq!(
            bubble_push(&world, leaving, MinecraftVersion::new(1, 21, 5)),
            dragged + 0.06 + 0.06 + 0.1 + 0.1
        );
    }

    #[test]
    fn bubble_columns_push_in_the_direction_of_the_move_from_1_21_9() {
        let mut world = WorldSnapshot::new();
        for y in 0..3 {
            world.set_block((0, y, 0), bubble_column(BlockEffect::BubbleDown));
        }
        let sinking = SimulationState {
            position: Vec3::new(0.5, 1.1, 0.5),
            velocity: Vec3::new(0.0, -0.3, 0.0),
            height: 1.8,
            ..SimulationState::default()
        };

        assert_eq!(
            bubble_push(&world, sinking, MinecraftVersion::new(1, 21, 4)),
            -0.3 - 0.03
        );
        assert_eq!(
            bubble_push(&world, sinking, MinecraftVersion::new(1, 21, 9)),
            -0.3
        );
    }

    #[test]
    fn bubble_columns_left_behind_do_not_push_from_1_21_10() {
        let mut world = WorldSnapshot::new();
        for y in 0..4 {
            world.set_block((0, y, 0), bubble_column(BlockEffect::BubbleUp));
        }
        world.set_block(
            (0, 4, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let rising = SimulationState {
            position: Vec3::new(0.5, 0.8, 0.5),
            velocity: Vec3::new(0.0, 0.3, 0.0),
            height: 1.8,
            ..SimulationState::default()
        };
        let dragged = 0.3 * java_float(0.8);

        assert_eq!(
            bubble_push(&world, rising, MinecraftVersion::new(1, 21, 9)),
            dragged + 0.06 + 0.06 + 0.06
        );
        assert_eq!(
            bubble_push(&world, rising, MinecraftVersion::new(1, 21, 10)),
            dragged + 0.06 + 0.06
        );
    }

    #[test]
    fn nothing_above_a_bubble_column_means_no_collision_and_no_fluid_from_1_21_5() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(BlockEffect::BubbleUp));
        world.set_block(
            (0, 1, 0),
            BlockState {
                id: 3,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Climbable,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let low = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            height: 0.6,
            ..SimulationState::default()
        };

        assert_eq!(
            bubble_push(&world, low, MinecraftVersion::new(1, 21, 4)),
            0.06
        );
        assert_eq!(
            bubble_push(&world, low, MinecraftVersion::new(1, 21, 5)),
            0.1
        );
    }

    #[test]
    fn weaving_uses_the_source_web_stuck_multiplier() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 4.0,
            weaving: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.stuck_speed_multiplier, Vec3::new(0.5, 0.25, 0.5));
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn powder_snow_sticks_the_entity_whose_position_it_holds() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::PowderSnow,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 4.0,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(
            state.stuck_speed_multiplier,
            Vec3::new(java_float(0.9), 1.5, java_float(0.9))
        );
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn powder_snow_immunity_skips_stuck_multiplier_and_zeroing() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::PowderSnow,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            powder_snow_immune: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.in_powder_snow);
        assert!(state.velocity.x > 0.9);
    }

    #[test]
    fn powder_snow_accumulates_and_clears_freeze_ticks() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::PowderSnow,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            height: 1.8,
            no_gravity: true,
            ticks_frozen: 3,
            ..SimulationState::default()
        };

        assert_eq!(state.ticks_frozen, 3);
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.ticks_frozen, 4);

        world.set_block((0, 1, 0), BlockState::default());
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.ticks_frozen, 2);
    }

    #[test]
    fn powder_snow_walkable_entities_collide_with_the_surface() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::PowderSnow,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut walkable = SimulationState {
            position: Vec3::new(0.5, 1.8, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            height: 1.8,
            powder_snow_walkable: true,
            ..SimulationState::default()
        };
        let mut falling = walkable;
        falling.powder_snow_walkable = false;

        for _ in 0..3 {
            tick(&mut walkable, &world, PhysicsConfig::default());
            tick(&mut falling, &world, PhysicsConfig::default());
        }

        assert!(walkable.on_ground);
        assert!(walkable.position.y >= 1.0 - 1.0e-7);
        assert!(falling.position.y < walkable.position.y);
    }

    fn powder_snow_world() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::PowderSnow,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn powder_snow_walkers_climb_when_they_were_in_it_the_tick_before() {
        let world = powder_snow_world();
        let jumping_inside = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            jump_requested: true,
            powder_snow_walkable: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut was_in = SimulationState {
            in_powder_snow: true,
            ..jumping_inside
        };
        let mut arrived = jumping_inside;

        crate::versioned::dispatch("1.21.5", &mut was_in, &world, PhysicsConfig::default())
            .unwrap();
        crate::versioned::dispatch("1.21.5", &mut arrived, &world, PhysicsConfig::default())
            .unwrap();

        assert_eq!(was_in.velocity.y, 0.2 * java_float(0.98));
        assert_eq!(arrived.velocity.y, 0.0);
        assert!(arrived.in_powder_snow);
    }

    #[test]
    fn powder_snow_counts_only_where_the_move_visited_blocks_from_1_21_2() {
        let world = powder_snow_world();
        let in_powder_snow = |version| {
            let mut rising = SimulationState {
                position: Vec3::new(0.5, 0.9758, 0.5),
                velocity: Vec3::new(0.0, 0.1764, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut rising, &world, PhysicsConfig::default())
                .unwrap();
            rising.in_powder_snow
        };

        assert!(!in_powder_snow("1.21.5"));
        assert!(in_powder_snow("1.21.9"));
    }

    #[test]
    fn powder_snow_catches_a_long_fall_on_its_lower_shape() {
        let world = powder_snow_world();
        let falling = SimulationState {
            position: Vec3::new(0.5, 1.2, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            fall_distance: 3.0,
            ..SimulationState::default()
        };
        let mut long_fall = falling;
        let mut short_fall = SimulationState {
            fall_distance: 2.0,
            ..falling
        };

        crate::versioned::dispatch("1.21.1", &mut long_fall, &world, PhysicsConfig::default())
            .unwrap();
        crate::versioned::dispatch("1.21.1", &mut short_fall, &world, PhysicsConfig::default())
            .unwrap();

        assert!(long_fall.on_ground);
        assert_eq!(long_fall.position.y, 1.2 + (f64::from(0.9_f32) - 1.2));
        assert!(!short_fall.on_ground);
        assert!(short_fall.position.y < 0.9);
    }

    #[test]
    fn powder_snow_walkers_sink_when_they_descend() {
        let world = powder_snow_world();
        let mut standing = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            powder_snow_walkable: true,
            ..SimulationState::default()
        };
        let mut descending = SimulationState {
            sneaking: true,
            ..standing
        };

        crate::versioned::dispatch("1.21.1", &mut standing, &world, PhysicsConfig::default())
            .unwrap();
        crate::versioned::dispatch("1.21.1", &mut descending, &world, PhysicsConfig::default())
            .unwrap();

        assert!(standing.on_ground);
        assert_eq!(standing.position.y, 1.0);
        assert!(!descending.on_ground);
        assert!(descending.position.y < 1.0);
    }

    fn place_scaffolding(world: &mut WorldSnapshot, position: (i32, i32, i32), bottom: bool) {
        let stable_boxes = [
            (Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.125, 1.0, 0.125)),
            (Vec3::new(0.0, 0.0, 0.875), Vec3::new(0.125, 1.0, 1.0)),
            (Vec3::new(0.875, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.125)),
            (Vec3::new(0.875, 0.0, 0.875), Vec3::new(1.0, 1.0, 1.0)),
            (Vec3::new(0.0, 0.875, 0.0), Vec3::new(1.0, 1.0, 1.0)),
        ];
        let shapes = stable_boxes
            .iter()
            .map(|&(min, max)| BlockShape {
                bounds: Aabb::new(min, max),
                effect: BlockEffect::Scaffolding,
                ..BlockShape::full_block()
            })
            .collect();
        let state = BlockState {
            id: 1,
            ..BlockState::default()
        };
        world.set_block_shapes(position, state, shapes);
        world.set_block_properties(
            position,
            vec![
                ("bottom".to_owned(), bottom.to_string()),
                ("distance".to_owned(), "1".to_owned()),
            ],
        );
    }

    #[test]
    fn scaffolding_holds_only_entities_above_it_that_do_not_descend() {
        let mut world = WorldSnapshot::new();
        place_scaffolding(&mut world, (0, 0, 0), false);
        let mut standing = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            ..SimulationState::default()
        };
        let mut descending = SimulationState {
            sneaking: true,
            ..standing
        };

        crate::versioned::dispatch("1.21.1", &mut standing, &world, PhysicsConfig::default())
            .unwrap();
        crate::versioned::dispatch("1.21.1", &mut descending, &world, PhysicsConfig::default())
            .unwrap();

        assert!(standing.on_ground);
        assert_eq!(standing.position.y, 1.0);
        assert!(descending.position.y < 1.0);
    }

    #[test]
    fn scaffolding_legs_do_not_block_entities_inside_it() {
        let mut world = WorldSnapshot::new();
        place_scaffolding(&mut world, (0, 0, 0), false);
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.1),
            velocity: Vec3::new(0.1, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };

        crate::versioned::dispatch("1.21.1", &mut state, &world, PhysicsConfig::default()).unwrap();

        assert!(!state.collided_horizontally);
        assert_eq!(state.position.x, 0.6);
    }

    #[test]
    fn hanging_bottom_scaffolding_is_a_floor_to_entities_above_its_base() {
        let mut world = WorldSnapshot::new();
        place_scaffolding(&mut world, (0, 1, 0), true);
        place_scaffolding(&mut world, (2, 1, 0), false);
        let falling = |x: f64| {
            let mut state = SimulationState {
                position: Vec3::new(x, 1.2, 0.5),
                velocity: Vec3::new(0.0, -0.1, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch("1.21.1", &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state
        };

        let over_bottom = falling(0.5);
        let over_supported = falling(2.5);

        assert!(over_bottom.on_ground);
        assert_eq!(over_bottom.position.y, 1.2 + (1.125 - 1.2));
        assert!(!over_supported.on_ground);
        assert_eq!(over_supported.position.y, 1.2 - 0.1);
    }

    #[test]
    fn fully_frozen_entities_take_periodic_freeze_damage() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::PowderSnow,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            height: 1.8,
            ticks_frozen: 140,
            tick_count: 39,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(state.freeze_damage, 1.0);
    }

    #[test]
    fn damage_surfaces_emit_source_specific_contact_damage() {
        let mut magma_world = WorldSnapshot::new();
        magma_world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Magma,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut magma = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            ..SimulationState::default()
        };
        tick(&mut magma, &magma_world, PhysicsConfig::default());
        assert_eq!(magma.contact_damage, 1.0);

        let mut cactus_world = WorldSnapshot::new();
        cactus_world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Cactus,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut cactus = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            height: 1.8,
            ..SimulationState::default()
        };
        tick(&mut cactus, &cactus_world, PhysicsConfig::default());
        assert_eq!(cactus.contact_damage, 1.0);

        let mut soul_campfire_world = WorldSnapshot::new();
        soul_campfire_world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::SoulCampfire,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut soul_campfire = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            height: 1.8,
            ..SimulationState::default()
        };
        tick(
            &mut soul_campfire,
            &soul_campfire_world,
            PhysicsConfig::default(),
        );
        assert_eq!(soul_campfire.contact_damage, 2.0);

        let mut fire_world = WorldSnapshot::new();
        fire_world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::SoulFire,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut fire = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            height: 1.8,
            ticks_frozen: 40,
            ..SimulationState::default()
        };
        tick(&mut fire, &fire_world, PhysicsConfig::default());
        assert_eq!(fire.ticks_frozen, 0);
        assert_eq!(fire.contact_damage, 2.0);
        tick(&mut fire, &WorldSnapshot::new(), PhysicsConfig::default());
        assert_eq!(fire.contact_damage, 0.0);
    }

    #[test]
    fn inside_effects_are_detected_across_the_full_entity_bounds() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Cactus,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            height: 2.0,
            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.contact_damage, 1.0);
    }

    fn passable_effect_world(position: (i32, i32, i32), effect: BlockEffect) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            position,
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn magma_burns_a_fall_flying_entity_and_spares_frost_walker() {
        // MagmaBlock.stepOn tests only isSteppingCarefully and LivingEntity, plus Frost Walker
        // before 1.21 (1.20.4 MagmaBlock.java:29, 1.21.11 MagmaBlock.java:29); from 1.21 Frost
        // Walker's damage immunity to #burn_from_stepping (1.21.11 Enchantments.java:389-394)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Magma,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let on_magma = |release, fall_flying, frost_walker| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                on_ground: true,
                fall_flying,
                frost_walker,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.contact_damage
        };

        for release in [
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 11),
        ] {
            assert_eq!(on_magma(release, true, false), 1.0);
            assert_eq!(on_magma(release, false, true), 0.0);
        }
    }

    #[test]
    fn lit_campfires_spare_frost_walker() {
        // CampfireBlock.entityInside skips a wearer of Frost Walker before 1.21 (1.20.5
        // CampfireBlock.java:104), the enchantment's immunity to the campfire damage type from 1.21
        let world = passable_effect_world((0, 1, 0), BlockEffect::SoulCampfire);
        for release in [MinecraftVersion::new(1, 20, 4), MinecraftVersion::new(1, 21, 11)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.5, 0.5),
                height: 1.8,
                no_gravity: true,
                frost_walker: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.contact_damage, 0.0);
        }
    }

    #[test]
    fn inside_block_signals_come_from_the_box_after_the_move() {
        // Entity.move calls checkInsideBlocks after the move (1.19.2 Entity.java:692), and from
        // 1.21.2 applyEffectsFromBlocks takes the tick's moves: a cactus the move reaches hurts
        // in the same tick
        let world = passable_effect_world((1, 1, 0), BlockEffect::Cactus);
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 11),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.6, 1.0, 0.5),
                velocity: Vec3::new(0.2, 0.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.contact_damage, 1.0, "{release:?}");
        }
    }

    #[test]
    fn inside_block_signals_deflate_the_box_by_the_release_inset() {
        // Entity.checkInsideBlocks deflates by 0.001 before 1.19.3, by 1e-7 to 1.21.1 (1.19.2
        // Entity.java:891-894, 1.19.3 :899-902) and 1.0E-5F from 1.21.2 (1.21.5 Entity.java:1096)
        let world = passable_effect_world((1, 1, 0), BlockEffect::Cactus);
        let touching = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.7005, 1.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.contact_damage
        };
        assert_eq!(touching(MinecraftVersion::new(1, 19, 2)), 0.0);
        assert_eq!(touching(MinecraftVersion::new(1, 19, 3)), 1.0);
        assert_eq!(touching(MinecraftVersion::new(1, 21, 1)), 1.0);
        assert_eq!(touching(MinecraftVersion::new(1, 21, 11)), 1.0);
    }

    #[test]
    fn powder_snow_freezes_from_the_box_after_the_move() {
        // LivingEntity.aiStep's freezing reads isInPowderSnow, which PowderSnowBlock.entityInside
        // sets during this tick's move (1.17 LivingEntity.java:2543-2552, Entity.java:413-415)
        let world = passable_effect_world((1, 1, 0), BlockEffect::PowderSnow);
        for release in [MinecraftVersion::new(1, 17, 0), MinecraftVersion::new(1, 21, 11)] {
            let mut state = SimulationState {
                position: Vec3::new(0.6, 1.0, 0.5),
                velocity: Vec3::new(0.2, 0.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                ticks_frozen: 10,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert!(state.in_powder_snow);
            assert_eq!(state.ticks_frozen, 11, "{release:?}");
        }
    }

    #[test]
    fn lava_clears_the_freeze_ticks() {
        // Entity.lavaIgnite's igniteForTicks from 1.21.5 (1.21.5 Entity.java:538-571) and
        // InsideBlockEffectType.CLEAR_FREEZE from 1.21.9 (1.21.11 LavaFluid.java:117). Before
        // 1.21.5 Entity.baseTick clears them only for an entity already burning at the start of
        // the tick (1.21.4 Entity.java:460-479), which needs the fire ticks the engine does not keep
        let lava = fluid_block_world(FluidState::Lava);
        for release in [
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.2, 0.5),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                in_lava: true,
                ticks_frozen: 40,
                ..SimulationState::default()
            };
            tick_release(&mut state, &lava, release);
            assert_eq!(state.ticks_frozen, 0, "{release:?}");
        }
    }

    #[test]
    fn lava_clears_the_freeze_ticks_along_the_move_from_1_21_5() {
        // Entity.collidedWithFluid tests the fluid's AABB along the move from the box before it
        // (1.21.5 Entity.java:1134-1143), AABB.collidedAlongVector holding either centre or the
        // path between in the AABB grown by half the box less 1e-7 (1.21.5 AABB.java:387-403):
        // the source's top of 8/9 reaches a 1.8 box's centre at 1.7889
        let lava = fluid_block_world(FluidState::Lava);
        let frozen_after_rising_from = |release, y: f64| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, y, 0.5),
                velocity: Vec3::new(0.0, 0.1, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                ticks_frozen: 40,
                ..SimulationState::default()
            };
            tick_release(&mut state, &lava, release);
            assert!((state.position.y - (y + 0.1)).abs() < 1.0e-12);
            state.ticks_frozen
        };
        for release in [
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            // ends with its bottom at 0.95, above the lava, but starts with its centre at 1.75
            assert_eq!(frozen_after_rising_from(release, 0.85), 0, "{release:?}");
            // the centre starts at 1.79, past the grown AABB: no lavaIgnite, aiStep takes two off
            assert_eq!(frozen_after_rising_from(release, 0.89), 38, "{release:?}");
        }
    }

    #[test]
    fn a_fall_through_stacked_powder_snow_freezes_once_a_step_from_1_21_5() {
        // FREEZE adds one tick in each StepBasedCollector step (1.21.5
        // InsideBlockEffectType.java:7-12, InsideBlockEffectApplier.java advanceStep): a 2.5 drop's
        // walk reaches the cells 3 - 5 at step 1 and cell 2 at step 2 (1.21.5
        // BlockGetter.java:172-190, 226-246), two steps; the column's empty collision shape lets
        // the fall through (fallDistance 0 before the move, 1.21.5 PowderSnowBlock.java:113-123)
        let mut world = WorldSnapshot::new();
        for y in 0..6 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                        effect: BlockEffect::PowderSnow,
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
        }
        let frozen = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 4.5, 0.5),
                velocity: Vec3::new(0.0, -2.5, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                ticks_frozen: 10,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.position.y, 2.0);
            assert!(state.in_powder_snow);
            state.ticks_frozen
        };
        // one LivingEntity.aiStep tick before 1.21.5 (1.21.4 LivingEntity.java freezing)
        assert_eq!(frozen(MinecraftVersion::new(1, 21, 4)), 11);
        assert_eq!(frozen(MinecraftVersion::new(1, 21, 5)), 12);
        assert_eq!(frozen(MinecraftVersion::new(1, 21, 8)), 12);
    }

    #[test]
    fn powder_snow_misses_a_long_fall_above_its_lower_shape_from_1_21_5() {
        // PowderSnowBlock.getEntityInsideCollisionShape is the collision shape for the entity,
        // FALLING_COLLISION_SHAPE (0.9F high) past a fall of 2.5 (1.21.5 - 1.21.11
        // PowderSnowBlock.java:108-123, 26.3 :89-104), and Entity.checkInsideBlocks gives
        // entityInside only when the move collides along with it (1.21.11 Entity.java:1225-1226):
        // a box falling from 1.2 to 0.95 keeps its centre above the shape grown by 0.9 - 1e-7.
        // 1.21.4 has only the full inside shape (1.21.4 Entity.java:1072-1100)
        let world = powder_snow_world();
        let fall = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.2, 0.5),
                velocity: Vec3::new(0.0, -0.25, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                fall_distance: 3.0,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert!((state.position.y - 0.95).abs() < 1.0e-12, "{release:?}");
            state
        };
        let stuck = Vec3::new(java_float(0.9), 1.5, java_float(0.9));
        let caught = fall(MinecraftVersion::new(1, 21, 4));
        assert_eq!(caught.stuck_speed_multiplier, stuck);
        assert_eq!(caught.fall_distance, 0.0);
        assert!(caught.in_powder_snow);
        for release in [
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let missed = fall(release);
            assert_eq!(missed.stuck_speed_multiplier, Vec3::ZERO, "{release:?}");
            assert!(missed.fall_distance >= 3.0, "{release:?}");
            assert!(!missed.in_powder_snow, "{release:?}");
        }
        // inside the shape's reach it still sticks
        let mut sinking = SimulationState {
            position: Vec3::new(0.5, 0.95, 0.5),
            velocity: Vec3::new(0.0, -0.01, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            fall_distance: 1.0,
            ..SimulationState::default()
        };
        tick_release(&mut sinking, &world, MinecraftVersion::new(1, 21, 11));
        assert_eq!(sinking.stuck_speed_multiplier, stuck);
    }

    #[test]
    fn soul_speed_reads_the_block_on_in_1_16_and_1_16_1() {
        // LivingEntity.onSoulSpeedBlock: getBlockStateOn, floor(y - 0.2F), in 1.16 and 1.16.1
        // (1.16 LivingEntity.java:653-655, Entity.java:683-698), the block below that affects the
        // movement from 1.16.2 (1.16.5 LivingEntity.java:664-666)
        let world = passable_effect_world((0, 0, 0), BlockEffect::SoulSand);
        let speed = |release, y| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, y, 0.5),
                velocity: Vec3::new(0.1, 0.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                soul_speed: 1,
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.velocity.x
        };
        let lifted = speed(MinecraftVersion::new(1, 16, 2), 1.3);
        assert_eq!(speed(MinecraftVersion::new(1, 16, 0), 1.1), lifted);
        assert_eq!(speed(MinecraftVersion::new(1, 16, 1), 1.1), lifted);
        // 0.3 above the cell the block on is air: the soul sand below slows by 0.4F
        let slowed = speed(MinecraftVersion::new(1, 16, 1), 1.3);
        assert!((slowed - lifted * f64::from(0.4_f32)).abs() < 1.0e-12);
    }

    #[test]
    fn block_fall_callback_runs_on_any_vertical_collision() {
        // Block.updateEntityMovementAfterFallOn on any vertical collision, a ceiling's too (1.21
        // Entity.java:665-668), only for a locally controlled entity from 1.21.2 (1.21.3
        // :698-703, 1.21.11 :768-773); 26.2's restitution takes a vertical move's (26.2
        // Entity.java:793, 815)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 3, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let bumps = |release, controlled_by_local_instance| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, 0.5, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                controlled_by_local_instance,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release).block_fall_callback
        };
        assert!(bumps(MinecraftVersion::new(1, 8, 8), false));
        assert!(bumps(MinecraftVersion::new(1, 21, 1), false));
        // the gate arrives in 1.21.2 itself (origin/client1.21.2 Entity.java
        // isControlledByLocalInstance)
        assert!(!bumps(MinecraftVersion::new(1, 21, 2), false));
        assert!(bumps(MinecraftVersion::new(1, 21, 2), true));
        assert!(!bumps(MinecraftVersion::new(1, 21, 3), false));
        assert!(bumps(MinecraftVersion::new(1, 21, 3), true));
        assert!(bumps(MinecraftVersion::new(26, 3, 0), true));

        let mut free = SimulationState {
            position: Vec3::new(0.5, 10.0, 0.5),
            velocity: Vec3::new(0.0, 0.5, 0.0),
            no_gravity: true,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        let result = tick_release(&mut free, &world, MinecraftVersion::new(1, 21, 11));
        assert!(!result.block_fall_callback);

        // a step-up onto a slab from a move with no y: movement.y > 0 = delta.y collides, but
        // 26.2 restitutes only `movedVertically = Math.abs(delta.y) > 0.0` (origin/client26.2
        // Entity.java:793, 815); onEntityLand before it takes any such collision (1.21.11
        // Entity.java:768-773)
        let mut steps = WorldSnapshot::new();
        ground_block(&mut steps, (0, 0, 0), 1.0);
        ground_block(&mut steps, (1, 0, 0), 1.0);
        ground_block(&mut steps, (1, 1, 0), 0.5);
        let steps_up = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.65, 1.0, 0.5),
                velocity: Vec3::new(0.2, 0.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                on_ground: true,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            let result = tick_release(&mut state, &steps, release);
            assert_eq!(state.position.y, 1.5);
            result.block_fall_callback
        };
        assert!(steps_up(MinecraftVersion::new(1, 21, 11)));
        assert!(steps_up(MinecraftVersion::new(26, 1, 0)));
        assert!(!steps_up(MinecraftVersion::new(26, 2, 0)));
        assert!(!steps_up(MinecraftVersion::new(26, 3, 0)));
    }

    #[test]
    fn inside_effects_ignore_face_touching_entity_bounds() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    effect: BlockEffect::Cactus,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            height: 1.8,
            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(state.contact_damage, 0.0);
    }

    #[test]
    fn inside_wall_uses_the_eye_box_and_no_physics_gate() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            eye_height: Some(1.53),
            no_gravity: true,
            ..SimulationState::default()
        };
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(state.inside_wall);
        state.no_physics = true;
        tick(&mut state, &world, PhysicsConfig::default());
        assert!(!state.inside_wall);
    }

    fn in_wall_at(release: MinecraftVersion, suffocating: Option<bool>, min_y: f64, max_y: f64) -> bool {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.0, min_y, 0.0), Vec3::new(1.0, max_y, 1.0)),
                    ..BlockShape::full_block()
                }),
                suffocating,
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            eye_height: Some(1.53),
            no_gravity: true,
            ..SimulationState::default()
        };
        tick_release(&mut state, &world, release);
        state.inside_wall
    }

    #[test]
    fn inside_wall_needs_a_suffocating_block() {
        // client1.21.11 Entity.java:2200 and 1.8.9 Entity.java:1304 test isSuffocating
        // (isVisuallyOpaque): glass, a full collision cube, is no wall.
        for release in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 21, 11)] {
            assert!(in_wall_at(release, None, 0.0, 1.0));
            assert!(!in_wall_at(release, Some(false), 0.0, 1.0));
        }
    }

    #[test]
    fn inside_wall_before_1_13_tests_its_own_predicate() {
        // 1.8.9 Entity.java:1304 (Block.isLeafBlock: blocksMovement && renderAsNormalBlock,
        // Block.java:330-332) and 1.12.2 Entity.java:1619 (causesSuffocation, Block.java:323-325)
        // are not pushOutOfBlocks' isNormalCube: glowstone, ice or a redstone block is a wall
        // there though no normal cube.
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 12, 2),
        ] {
            let mut world = WorldSnapshot::new();
            let block = |in_wall: Option<bool>| BlockState {
                shape: Some(BlockShape::full_block()),
                suffocating: Some(false),
                in_wall,
                ..BlockState::default()
            };
            world.set_block((0, 1, 0), block(Some(true)));
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                eye_height: Some(1.62),
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert!(state.inside_wall, "{release:?}");
            world.set_block((0, 1, 0), block(None));
            let mut state = SimulationState {
                inside_wall: false,
                position: Vec3::new(0.5, 0.0, 0.5),
                ..state
            };
            tick_release(&mut state, &world, release);
            assert!(!state.inside_wall, "{release:?}");
        }
    }

    #[test]
    fn inside_wall_from_1_16_to_1_17_counts_the_world_border_the_box_touches() {
        // 1.16.5 Entity.java:1648 and 1.17 :1669 call getBlockCollisions with the entity, whose
        // BlockCollisionSpliterator offers the border's shape first (1.16 :56-58, 101-129; 1.17
        // :58-59, 102-125) when the box touches it from inside; the block predicate does not
        // filter it. 1.15 (8 points), 1.18 (getStatesInBox) and 1.19 (BlockPos.betweenClosed)
        // look at blocks alone.
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);
        for (release, touching) in [
            (MinecraftVersion::new(1, 15, 2), false),
            (MinecraftVersion::new(1, 16, 5), true),
            (MinecraftVersion::new(1, 17, 1), true),
            (MinecraftVersion::new(1, 18, 2), false),
            (MinecraftVersion::new(1, 21, 11), false),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(4.75, 0.0, 0.5),
                half_width: 0.25,
                height: 1.8,
                eye_height: Some(1.62),
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.inside_wall, touching, "{release:?}");
            let mut away = SimulationState {
                position: Vec3::new(4.5, 0.0, 0.5),
                half_width: 0.25,
                height: 1.8,
                eye_height: Some(1.62),
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut away, &world, release);
            assert!(!away.inside_wall, "{release:?}");
        }
    }

    #[test]
    fn inside_wall_follows_each_release_algorithm() {
        // A suffocating bottom slab under an eye 0.03 above it: before 1.16 the eye's cell is
        // enough (1.15 Entity.java:1578-1586); 1.16's box 0.1 tall reaches down to it
        // (1.16.5 Entity.java:1645-1648); 1.17's 1e-6 box does not (1.17 Entity.java:1667-1669).
        assert!(in_wall_at(MinecraftVersion::new(1, 15, 2), Some(true), 0.0, 0.5));
        assert!(in_wall_at(MinecraftVersion::new(1, 16, 5), Some(true), 0.0, 0.5));
        assert!(!in_wall_at(MinecraftVersion::new(1, 17, 1), Some(true), 0.0, 0.5));
        // A suffocating top slab around the eye: 1.18 moves the shape to the eye in place of its
        // cell, so it starts 0.5 above the eye (client1.18.1 Entity.java:1705-1708); 1.19 moves it
        // to its cell (1.19 Entity.java:1683-1692).
        assert!(!in_wall_at(MinecraftVersion::new(1, 18, 2), Some(true), 0.5, 1.0));
        assert!(in_wall_at(MinecraftVersion::new(1, 19, 0), Some(true), 0.5, 1.0));
        assert!(in_wall_at(MinecraftVersion::new(1, 18, 2), Some(true), 0.0, 1.0));
    }

    #[test]
    fn a_sleeping_entity_is_never_inside_a_wall() {
        // client1.21.11 LivingEntity.java:3654-3656, 1.8.9 PlayerEntity.java:1111-1113.
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        for release in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 21, 11)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                eye_height: Some(1.53),
                no_gravity: true,
                sleeping: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert!(!state.inside_wall);
        }
    }

    #[test]
    fn non_finite_entity_motion_is_rejected_like_set_delta_movement() {
        let mut state = SimulationState {
            position: Vec3::new(f64::NAN, 0.0, 0.0),
            velocity: Vec3::new(f64::INFINITY, 0.0, 0.0),
            ..SimulationState::default()
        };
        tick(&mut state, &WorldSnapshot::new(), PhysicsConfig::default());
        assert!(state.velocity.is_finite());
        assert_eq!(state.velocity.x, 0.0);
    }

    #[test]
    fn fall_distance_accumulation_matches_vanilla_thresholds() {
        let mut state = SimulationState {
            velocity: Vec3::new(0.0, -0.49, 0.0),
            fall_distance: 12.0,
            ..SimulationState::default()
        };
        state.check_fall_distance_accumulation();
        assert_eq!(state.fall_distance, 1.0);

        state.fall_distance = 12.0;
        state.velocity.y = -0.5;
        state.check_fall_distance_accumulation();
        assert_eq!(state.fall_distance, 12.0);
    }

    #[test]
    fn non_finite_delta_impulses_preserve_existing_velocity() {
        let mut state = SimulationState {
            velocity: Vec3::new(0.25, 0.0, -0.5),
            ..SimulationState::default()
        };
        state.add_delta_movement(Vec3::new(f64::INFINITY, 0.0, 0.0));
        assert_eq!(state.velocity, Vec3::new(0.25, 0.0, -0.5));
        state.add_delta_movement(Vec3::new(0.1, 0.0, 0.2));
        assert_eq!(state.velocity, Vec3::new(0.35, 0.0, -0.3));
    }

    #[test]
    fn flying_entities_ignore_player_stuck_block_multiplier() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            flying: true,
            is_player: true,
            no_gravity: true,
            fall_distance: 4.0,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.x > 0.9);
    }

    #[test]
    fn sweet_berry_bush_applies_source_stuck_multiplier() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::SweetBerryBush,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            fall_distance: 4.0,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert_eq!(
            state.stuck_speed_multiplier,
            Vec3::new(java_float(0.8), 0.75, java_float(0.8))
        );
        assert_eq!(state.fall_distance, 0.0);
    }

    #[test]
    fn berry_bush_immunity_skips_source_stuck_multiplier() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::SweetBerryBush,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            sweet_berry_bush_immune: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.position.x > 1.4);
        assert!(state.velocity.x > 0.9);
    }

    #[test]
    fn soul_sand_applies_block_speed_factor_after_movement() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::SoulSand,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            input: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        let ground_drag = java_float(java_float(0.6) * java_float(0.98));
        let moved = java_float(0.98) * java_float(0.1);
        assert!((state.position.x - (0.5 + moved)).abs() < 1.0e-12);
        assert!((state.velocity.x - moved * java_float(0.4) * ground_drag).abs() < 1.0e-12);
    }

    #[test]
    fn soul_speed_enchantment_disables_soul_block_slowdown() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::SoulSoil,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            input: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            soul_speed: 1,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        let ground_drag = java_float(java_float(0.6) * java_float(0.98));
        let moved = java_float(0.98) * java_float(0.1);
        assert!((state.position.x - (0.5 + moved)).abs() < 1.0e-12);
        assert!((state.velocity.x - moved * ground_drag).abs() < 1.0e-12);
    }

    #[test]
    fn movement_efficiency_lifts_the_soul_sand_slowdown() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::SoulSand,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let base = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            input: Vec3::new(1.0, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut without = base;
        let mut with = base;
        with.movement_efficiency = 1.0;

        crate::versioned::dispatch("26.2", &mut without, &world, PhysicsConfig::default()).unwrap();
        crate::versioned::dispatch("26.2", &mut with, &world, PhysicsConfig::default()).unwrap();

        assert!(with.velocity.x > without.velocity.x);
        assert!(with.velocity.x > 0.0);
    }

    #[test]
    fn slime_step_on_applies_source_horizontal_damping() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(0.05, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        let ground_drag = java_float(java_float(0.6) * java_float(0.98));
        assert!((state.velocity.x - 0.05 * 0.4 * ground_drag).abs() < 1.0e-12);
    }

    #[test]
    fn slime_step_on_skips_damping_when_stepping_carefully() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity: Vec3::new(0.05, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            stepping_carefully: true,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        let ground_drag = java_float(java_float(0.6) * java_float(0.98));
        assert!((state.velocity.x - 0.05 * ground_drag).abs() < 1.0e-12);
    }

    #[test]
    fn slime_bounce_is_full_for_a_player_and_a_mob() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut player = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            velocity: Vec3::new(0.0, -0.2, 0.0),
            half_width: 0.3,
            height: 1.0,
            is_player: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut mob = SimulationState {
            is_player: false,
            ..player
        };

        tick(&mut player, &world, PhysicsConfig::default());
        tick(&mut mob, &world, PhysicsConfig::default());

        // SlimeBlock.bounceUp scales by 0.8 only an entity that is not a LivingEntity, and every
        // entity this travel moves is one (1.21.11 SlimeBlock.java:44, 1.9.4 SlimeBlock.java:38-41)
        assert!((player.velocity.y - 0.2 * java_float(0.98)).abs() < 1.0e-12);
        assert!((mob.velocity.y - 0.2 * java_float(0.98)).abs() < 1.0e-12);
    }

    #[test]
    fn slime_bounce_uses_the_block_landed_on() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Slime,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.5, 0.5),
            velocity: Vec3::new(0.0, -2.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            fall_distance: 20.0,
            fall_damage: 7.0,
            no_gravity: true,
            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.on_ground);
        assert!(state.velocity.y > 0.0);
        assert_eq!(state.fall_distance, 0.0);
        assert_eq!(state.fall_damage, 0.0);
    }

    fn bed() -> BlockState {
        BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5625, 1.0)),
                effect: BlockEffect::Bed,
                ..BlockShape::full_block()
            }),
            bounce_restitution: 0.75,
            ..BlockState::default()
        }
    }

    #[test]
    fn bed_bounces_a_fall_back_up_with_two_thirds_of_its_speed() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bed());
        let falling = SimulationState {
            position: Vec3::new(0.5, 0.8, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut player = falling;
        let mut mob = SimulationState {
            is_player: false,
            ..falling
        };
        let mut sneaking = SimulationState {
            sneaking: true,
            ..falling
        };

        for state in [&mut player, &mut mob, &mut sneaking] {
            crate::versioned::dispatch("1.21.1", state, &world, PhysicsConfig::default()).unwrap();
        }

        let drag = java_float(0.98);
        assert!(player.on_ground);
        assert_eq!(player.velocity.y, 0.5 * 0.6600000262260437 * drag);
        // BedBlock.bounceUp: the 0.8 is for an entity that is not living (1.21.11 BedBlock.java:149-150)
        assert_eq!(mob.velocity.y, 0.5 * 0.6600000262260437 * drag);
        assert_eq!(sneaking.velocity.y, 0.0);
    }

    #[test]
    fn beds_bounce_only_from_1_12() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bed());
        let falling = SimulationState {
            position: Vec3::new(0.5, 0.8, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut before = falling;
        let mut after = falling;

        crate::versioned::dispatch("1.11.2", &mut before, &world, PhysicsConfig::default())
            .unwrap();
        crate::versioned::dispatch("1.12.2", &mut after, &world, PhysicsConfig::default()).unwrap();

        assert_eq!(before.velocity.y, 0.0);
        assert!(after.velocity.y > 0.0);
    }

    #[test]
    fn bed_halves_the_fall_distance_that_hurts() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bed());
        world.set_block(
            (2, 0, 0),
            BlockState {
                id: 2,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5625, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let damage = |x: f64, fall_distance: f64| {
            let mut state = SimulationState {
                position: Vec3::new(x, 0.8, 0.5),
                velocity: Vec3::new(0.0, -1.0, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                fall_distance,
                safe_fall_distance: 3.0,
                ..SimulationState::default()
            };
            crate::versioned::dispatch("1.21.1", &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.fall_damage
        };

        assert_eq!(damage(0.5, 20.0), damage(2.5, 10.0));
        assert!(damage(0.5, 20.0) < damage(2.5, 20.0));
    }

    /// The fall damage of a player landing with a fall distance of 20 on a block with this effect
    /// (topped at `top`), its own drop this tick nothing.
    fn landing_damage_on(version: &str, effect: BlockEffect, top: f64, sneaking: bool) -> (f64, f64) {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, top, 1.0)),
                    effect,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, top, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            sneaking,
            no_gravity: true,
            fall_distance: 20.0,
            safe_fall_distance: 3.0,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
        assert!(state.on_ground, "{version} {effect:?}");
        (state.fall_damage, state.fall_distance)
    }

    #[test]
    fn hay_and_honey_deal_a_fifth_of_the_fall_damage() {
        // HayBlock.fallOn: causeFallDamage(fd, 0.2F) from 1.9 (1.9.4 HayBlock.java:19-20; 1.8.9
        // has no fallOn), HoneyBlock.fallOn the same (1.15 HoneyBlock.java:55-61, 1.21.5 :51-57).
        // ceil((20F - 3F) * 0.2F) = ceil(3.4000001F) before 1.21.5 (1.20.4 LivingEntity.java:1564,
        // 1.21.4 :1662-1664), floor((20 + 1.0E-6 - 3) * 0.2F) from it (1.21.5 :1672-1682).
        assert_eq!(landing_damage_on("1.8.8", BlockEffect::Hay, 1.0, false).0, 17.0);
        for version in ["1.9.4", "1.12.2", "1.16.5", "1.20.4", "1.21.1"] {
            assert_eq!(landing_damage_on(version, BlockEffect::Hay, 1.0, false), (4.0, 0.0), "{version}");
        }
        assert_eq!(landing_damage_on("1.21.5", BlockEffect::Hay, 1.0, false), (3.0, 0.0));
        assert_eq!(landing_damage_on("26.3", BlockEffect::Hay, 1.0, false), (3.0, 0.0));
        assert_eq!(landing_damage_on("1.15", BlockEffect::Honey, 0.9375, false).0, 4.0);
        assert_eq!(landing_damage_on("1.21.11", BlockEffect::Honey, 0.9375, false).0, 3.0);
        assert_eq!(landing_damage_on("1.21.1", BlockEffect::None, 1.0, false).0, 17.0);
    }

    #[test]
    fn an_upward_dripstone_tip_doubles_a_longer_fall() {
        // PointedDripstoneBlock.fallOn: causeFallDamage(fd + 2.0F, 2.0F) (1.17
        // PointedDripstoneBlock.java:128-130, 1.21.4 :149-151): ceil((22F - 3F) * 2.0F); from
        // 1.21.5 causeFallDamage(fd + 2.5, 2.0F) (1.21.5 :147-149): floor((22.5 + 1.0E-6 - 3) * 2)
        for version in ["1.17", "1.19.4", "1.20.4", "1.21.1"] {
            let (damage, fall_distance) =
                landing_damage_on(version, BlockEffect::PointedDripstoneTip, 1.0, false);
            assert_eq!((damage, fall_distance), (38.0, 0.0), "{version}");
        }
        assert_eq!(landing_damage_on("1.21.5", BlockEffect::PointedDripstoneTip, 1.0, false).0, 39.0);
        assert_eq!(landing_damage_on("26.3", BlockEffect::PointedDripstoneTip, 1.0, false).0, 39.0);
    }

    #[test]
    fn landing_on_powder_snow_hurts_only_before_1_18() {
        // PowderSnowBlock.fallOn only plays a sound from 1.18 (1.18 PowderSnowBlock.java:85-91,
        // 1.21.5 :99-105); 1.17 and 1.17.1 (whose jar declares no fallOn) keep Block.fallOn.
        // Over 2.5 blocks of fall the snow holds the player at 0.9F (1.17 :88).
        let top = f64::from(0.9_f32);
        assert_eq!(landing_damage_on("1.17", BlockEffect::PowderSnow, top, false), (17.0, 0.0));
        assert_eq!(landing_damage_on("1.17.1", BlockEffect::PowderSnow, top, false), (17.0, 0.0));
        for version in ["1.18", "1.19.4", "1.21.1", "1.21.5", "26.3"] {
            assert_eq!(landing_damage_on(version, BlockEffect::PowderSnow, top, false), (0.0, 0.0), "{version}");
        }
    }

    #[test]
    fn a_sneaking_landing_on_slime_hurts_only_before_1_21_2() {
        // SlimeBlock.fallOn: an entity suppressing the bounce takes Block.fallOn's damage until
        // 1.21.1 (1.21.1 SlimeBlock.java:24-29); from 1.21.2 it takes none, `if
        // (!isSuppressingBounce()) causeFallDamage(fd, 0.0F)` with no else (1.21.2 :24-28).
        // Entity.checkFallDamage resets the fall distance either way.
        assert_eq!(landing_damage_on("1.20.4", BlockEffect::Slime, 1.0, true), (17.0, 0.0));
        assert_eq!(landing_damage_on("1.21.1", BlockEffect::Slime, 1.0, true), (17.0, 0.0));
        for version in ["1.21.2", "1.21.3", "1.21.5", "26.1.2", "26.2", "26.3"] {
            assert_eq!(landing_damage_on(version, BlockEffect::Slime, 1.0, true), (0.0, 0.0), "{version}");
        }
    }

    fn landing_in_26_2(world: &WorldSnapshot, state: SimulationState) -> SimulationState {
        let mut state = state;
        crate::versioned::dispatch("26.2", &mut state, world, PhysicsConfig::default()).unwrap();
        state
    }

    #[test]
    fn restitution_bounces_a_fall_by_the_block_landed_on_and_stays_on_ground() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 0.75,
                ..BlockState::default()
            },
        );
        let falling = SimulationState {
            position: Vec3::new(0.5, 1.25, 0.5),
            velocity: Vec3::new(0.0, -0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            ..SimulationState::default()
        };

        let player = landing_in_26_2(&world, falling);
        let mob = landing_in_26_2(
            &world,
            SimulationState {
                is_player: false,
                ..falling
            },
        );
        let sneaking = landing_in_26_2(
            &world,
            SimulationState {
                sneaking: true,
                ..falling
            },
        );

        let air_drag = f64::from(0.98_f32);
        let portion = 0.5;
        let bounce = |restitution: f64| {
            ((portion * 0.08 + 0.5) * (1.0 + portion * (air_drag - 1.0)) * restitution - 0.08)
                * air_drag
        };
        assert!(player.on_ground);
        assert_eq!(player.velocity.y, bounce(0.75));
        // Entity.getBlockBounciness: the 0.8F is for an entity that is not living (26.3 Entity.java:905-911)
        assert_eq!(mob.velocity.y, bounce(0.75));
        assert_eq!(sneaking.velocity.y, -0.08 * air_drag);
    }

    #[test]
    fn a_landing_as_fast_as_gravity_bounces_only_before_26_3() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                bounce_restitution: 0.75,
                ..BlockState::default()
            },
        );
        let landing = |version| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.05, 0.5),
                velocity: Vec3::new(0.0, -0.08, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state
        };

        assert!(landing("26.2").velocity.y > 0.0);
        let rested = landing("26.3");
        assert!(rested.on_ground);
        assert_eq!(rested.velocity.y, -0.08 * f64::from(0.98_f32));
    }

    #[test]
    fn restitution_leaves_slower_landings_and_old_slime_bounce_out() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    effect: BlockEffect::Slime,
                    slipperiness: 0.8,
                    ..BlockShape::full_block()
                }),
                bounce_restitution: 1.0,
                ..BlockState::default()
            },
        );
        let walking = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            is_player: true,
            ..SimulationState::default()
        };

        let walked = landing_in_26_2(&world, walking);

        assert!(walked.on_ground);
        assert_eq!(walked.velocity.y, -0.08 * f64::from(0.98_f32));
    }

    #[test]
    fn honey_block_applies_source_downward_slide_velocity() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (2, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Honey,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let slide = |release: MinecraftVersion| {
            let mut state = SimulationState {
                position: Vec3::new(1.65, 0.5, 0.5),
                velocity: Vec3::new(0.08, -0.2, 0.0),
                half_width: 0.3,
                height: 1.8,
                fall_distance: 3.0,
                no_gravity: true,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state
        };

        let before_1_21_2 = slide(MinecraftVersion::new(1, 21, 1));
        assert_eq!(before_1_21_2.velocity.y, -0.05 * java_float(0.98));
        assert_eq!(before_1_21_2.fall_distance, 0.0);
        assert_eq!(
            slide(MinecraftVersion::new(1, 21, 2)).velocity.y,
            (-0.05 - 0.08) * java_float(0.98)
        );
    }

    #[test]
    fn honey_block_does_not_slide_near_block_center() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: crate::world::BlockEffect::Honey,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity: Vec3::new(0.2, -0.2, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            controlled_by_local_instance: true,

            ..SimulationState::default()
        };

        tick(&mut state, &world, PhysicsConfig::default());

        assert!(state.velocity.y < -0.1274);
        // fallDistance -= (double)(float)dy (1.21.5 Entity.java:1286)
        assert_eq!(state.fall_distance, f64::from(0.2_f32));
    }

    #[test]
    fn repeated_ticks_are_deterministic_and_finite() {
        let world = WorldSnapshot::new();
        let initial = SimulationState {
            position: Vec3::new(0.5, 8.0, -2.5),
            velocity: Vec3::new(0.03, 0.0, -0.02),
            input: Vec3::new(0.4, 0.0, 0.7),
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        };
        let mut first = initial;
        let mut second = initial;
        for _ in 0..200 {
            tick(&mut first, &world, PhysicsConfig::default());
            tick(&mut second, &world, PhysicsConfig::default());
        }

        assert_eq!(first, second);
        assert!(first.position.x.is_finite());
        assert!(first.position.y.is_finite());
        assert!(first.position.z.is_finite());
        assert!(first.velocity.x.is_finite());
        assert!(first.velocity.y.is_finite());
        assert!(first.velocity.z.is_finite());
    }

    #[test]
    fn negligible_velocity_components_are_zeroed_before_tick() {
        let mut state = SimulationState {
            velocity: Vec3::new(0.002, -0.002, 0.004),
            ..SimulationState::default()
        };
        let config = PhysicsConfig {
            gravity: 0.0,
            drag: 1.0,
            negligible_velocity: 0.003,
            ..PhysicsConfig::default()
        };

        tick(&mut state, &WorldSnapshot::new(), config);

        assert_eq!(state.velocity.x, 0.0);
        assert_eq!(state.velocity.y, 0.0);
        assert!(state.velocity.z > 0.0);
    }

    #[test]
    fn players_stop_by_their_horizontal_speed_from_1_21_5() {
        let coasting = |is_player: bool, release: MinecraftVersion| {
            let mut state = SimulationState {
                velocity: Vec3::new(0.0025, 0.0, 0.0025),
                is_player,
                no_gravity: true,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release,
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );
            state.velocity.x
        };

        assert!(coasting(true, MinecraftVersion::new(1, 21, 5)) > 0.0);
        assert_eq!(coasting(true, MinecraftVersion::new(1, 21, 4)), 0.0);
        assert_eq!(coasting(false, MinecraftVersion::new(1, 21, 5)), 0.0);
    }

    #[test]
    fn entity_gravity_and_step_height_override_version_rules() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            velocity: Vec3::ZERO,
            input: Vec3::new(1.0, 0.0, 0.0),
            gravity: Some(0.02),
            step_height: Some(1.0),
            no_gravity: false,
            ..SimulationState::default()
        };
        let mut world = WorldSnapshot::new();
        world.set_block(
            (1, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );

        tick_with_rules(
            &mut state,
            &world,
            PhysicsConfig::default(),
            VersionPhysicsRules::default(),
        );

        assert!(state.velocity.y > -0.03);
        assert!(state.position.x > 0.5);
    }

    #[test]
    fn entity_gravity_and_step_height_are_sanitized_as_ranged_attributes() {
        // RangedAttribute.sanitizeValue: NaN is the minimum, the rest clamped to GRAVITY's -1..1 and
        // STEP_HEIGHT's 0..10 (1.21.11 RangedAttribute.java:35-37, Attributes.java:46, 86)
        let tick = |gravity: f64, step_height: f64| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 2.0, 0.5),
                velocity: Vec3::ZERO,
                input: Vec3::new(1.0, 0.0, 0.0),
                gravity: Some(gravity),
                step_height: Some(step_height),
                ..SimulationState::default()
            };
            let mut world = WorldSnapshot::new();
            for y in 0..12 {
                world.set_block(
                    (1, y, 0),
                    BlockState {
                        id: 1,
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
            }
            tick_with_rules(
                &mut state,
                &world,
                PhysicsConfig::default(),
                VersionPhysicsRules::default(),
            );
            (state.position, state.velocity)
        };

        assert_eq!(tick(5.0, 0.6), tick(1.0, 0.6));
        assert_eq!(tick(f64::NAN, 0.6), tick(-1.0, 0.6));
        assert_ne!(tick(-1.0, 0.6), tick(1.0, 0.6));
        assert_eq!(tick(0.08, 20.0), tick(0.08, 10.0));
        assert_eq!(tick(0.08, f64::NAN), tick(0.08, 0.0));
        assert_eq!(tick(0.08, -3.0), tick(0.08, 0.0));
    }

    #[test]
    fn runtime_config_overrides_version_defaults() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            velocity: Vec3::ZERO,
            ..SimulationState::default()
        };
        let config = PhysicsConfig {
            gravity: 0.02,
            drag: 0.5,
            step_height: 1.0,
            ..PhysicsConfig::default()
        };

        tick_with_rules(
            &mut state,
            &WorldSnapshot::new(),
            config,
            VersionPhysicsRules::default(),
        );

        assert!((state.velocity.y + 0.02 * java_float(0.98)).abs() < 1e-12);
    }

    fn ground_block(world: &mut WorldSnapshot, position: (i32, i32, i32), top: f64) {
        world.set_block(
            position,
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, top, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
    }

    /// The x of a sneaking player's move of `delta` from (0.5, y, 0.5) after
    /// Player.maybeBackOffFromEdge with the 0.6 step.
    fn backed_off_x(
        world: &WorldSnapshot,
        state: SimulationState,
        delta: Vec3,
        version: &str,
    ) -> f64 {
        let release = MinecraftVersion::parse(version).unwrap();
        let context = entity_collision_context(&state);
        back_off_from_edge(&state, world, &context, delta, 0.6, release).x
    }

    fn sneaker(y: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, y, 0.5),
            is_player: true,
            sneaking: true,
            on_ground: true,
            half_width: 0.3,
            height: 1.8,
            ..SimulationState::default()
        }
    }

    #[test]
    fn a_sneak_jump_backs_off_from_the_edge_before_1_19() {
        // client1.14.4 Entity.java:600-640 and 1.18 PlayerEntity.java:1018-1020 test no rising
        // move; 1.19 adds `movement.y <= 0.0` (1.19 PlayerEntity.java:1059-1064)
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        let jump = Vec3::new(0.8, 0.42, 0.0);
        for (version, backs_off) in [
            ("1.14.4", true),
            ("1.16.5", true),
            ("1.18.2", true),
            ("1.19", false),
            ("1.21.5", false),
        ] {
            let x = backed_off_x(&world, sneaker(1.0), jump, version);
            assert_eq!(x < 0.8, backs_off, "{version}");
        }
    }

    #[test]
    fn an_airborne_sneaker_backs_off_only_from_1_16_2() {
        // before 1.16.2 only `onGround` (1.16 PlayerEntity.java:1035-1036); from
        // 1.16.2 isAboveGround: the box moved down by the step less the fall distance meets the
        // ground (1.16.5 PlayerEntity.java:1116-1120)
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        let state = SimulationState {
            on_ground: false,
            ..sneaker(1.3)
        };
        let fall = Vec3::new(0.8, -0.1, 0.0);
        assert_eq!(backed_off_x(&world, state, fall, "1.16.1"), 0.8);
        assert!(backed_off_x(&world, state, fall, "1.16.2") < 0.8);
        assert!(backed_off_x(&world, state, fall, "1.21.5") < 0.8);
    }

    #[test]
    fn a_flying_sneaker_backs_off_until_1_16_1() {
        // `!this.abilities.flying` arrives in 1.16.2 (1.16.2 Player.maybeBackOffFromEdge bytecode
        // 0-7; 1.16 PlayerEntity.java has none)
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        let state = SimulationState {
            flying: true,
            ..sneaker(1.0)
        };
        let walk = Vec3::new(0.8, 0.0, 0.0);
        assert!(backed_off_x(&world, state, walk, "1.16.1") < 0.8);
        assert_eq!(backed_off_x(&world, state, walk, "1.16.2"), 0.8);
    }

    #[test]
    fn the_whole_box_meets_a_block_at_body_height_before_1_20_5() {
        // before 1.20.5 the whole box moved by (x, -step, z) (1.20.4 PlayerEntity.java:1059); from
        // 1.20.5 the feet's slab (1.20.5 :1125-1128)
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        ground_block(&mut world, (1, 1, 0), 1.0);
        let walk = Vec3::new(0.8, 0.0, 0.0);
        assert_eq!(backed_off_x(&world, sneaker(1.0), walk, "1.20.4"), 0.8);
        assert!(backed_off_x(&world, sneaker(1.0), walk, "1.20.5") < 0.8);
    }

    #[test]
    fn the_feet_slab_reaches_the_float_step_and_its_epsilon_below() {
        // 1.20.5 - 1.21.4: minY - (float) 0.6F - 1.0E-5F (1.21.4 PlayerEntity.java:1128-1132);
        // 1.21.5 on: minY - 0.6F - 1e-7, 1e-7 in all round (1.21.5 Player.java:1081-1085); the
        // whole box moved by -0.6F before (1.20.4 PlayerEntity.java:1059)
        let step = f64::from(0.6_f32);
        let lower = |top: f64| {
            let mut world = WorldSnapshot::new();
            ground_block(&mut world, (0, 0, 0), 1.0);
            ground_block(&mut world, (1, 0, 0), top);
            world
        };
        let walk = Vec3::new(0.8, 0.0, 0.0);
        let within_the_float_epsilon = lower(1.0 - step - 5.0e-6);
        assert!(backed_off_x(&within_the_float_epsilon, sneaker(1.0), walk, "1.20.4") < 0.8);
        assert_eq!(
            backed_off_x(&within_the_float_epsilon, sneaker(1.0), walk, "1.20.5"),
            0.8
        );
        assert_eq!(
            backed_off_x(&within_the_float_epsilon, sneaker(1.0), walk, "1.21.4"),
            0.8
        );
        assert!(backed_off_x(&within_the_float_epsilon, sneaker(1.0), walk, "1.21.5") < 0.8);
        // a full cube's top 0.6F + 1e-7 down (not 0.6 + 1e-7): the float step reaches it (a full
        // cube takes the strict `intersects`, client1.21.11 BlockCollisions.java:86-88; another
        // shape's overlap of under 1e-7 would not count, Shapes.joinIsNotEmpty)
        let mut cube_below = WorldSnapshot::new();
        ground_block(&mut cube_below, (1, -1, 0), 1.0);
        let past_the_double_step = 0.6 + 1.0e-7 + 1.0e-8;
        assert_eq!(
            backed_off_x(&cube_below, sneaker(past_the_double_step), walk, "1.21.5"),
            0.8
        );
        assert_eq!(
            backed_off_x(&cube_below, sneaker(past_the_double_step), walk, "26.3"),
            0.8
        );
        // a slab's top as far down: its overlap of 1.4e-8 is merged away
        // (client1.21.11 IndirectMerger.java ctor)
        let slab_below = lower(1.0 - 0.6 - 1.0e-7 - 1.0e-8);
        assert!(backed_off_x(&slab_below, sneaker(1.0), walk, "1.21.5") < 0.8);
    }

    #[test]
    fn a_ground_box_within_1e_7_of_the_feet_holds_the_sneaker_until_1_21_4() {
        // the 1e-7 inset all round arrives in 1.21.5 (1.21.5 Player.java:1081-1085)
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        // the move leaves the box overlapping the block by 5e-8
        let walk = Vec3::new(0.8 - 5.0e-8, 0.0, 0.0);
        assert_eq!(backed_off_x(&world, sneaker(1.0), walk, "1.21.4"), walk.x);
        assert!(backed_off_x(&world, sneaker(1.0), walk, "1.21.5") < walk.x);
    }

    #[test]
    fn boxes_the_slab_does_not_meet_leave_the_sneaker_free_to_fall() {
        // BlockCollisions.computeNext keeps only shapes that overlap the query (client1.21.11
        // BlockCollisions.java:86-104): a carpet in the cell below the slab does not hold it
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        ground_block(&mut world, (1, 0, 0), 0.0625);
        let walk = Vec3::new(0.8, 0.0, 0.0);
        for version in ["1.14.4", "1.20.5", "1.21.11"] {
            assert!(
                backed_off_x(&world, sneaker(1.0), walk, version) < 0.8,
                "{version}"
            );
        }
    }

    #[test]
    fn a_boat_under_the_feet_holds_a_sneaker_over_a_gap() {
        // CollisionGetter.noCollision counts the entity colliders (client1.21.11
        // CollisionGetter.java:52-54, EntityGetter.java:54-72; client1.14.4 EntityGetter.java
        // :54-68); before 1.14 World.getCollisionBoxes(entity, box) does (1.8.9 Entity.java:487,
        // 1.12.2 Entity.java:616 and World.java:1180-1203, 1.13.2 Entity.java:627 and World.java
        // :1805-1808)
        let mut world = WorldSnapshot::new();
        world.set_entity_colliders(vec![EntityCollider {
            bounds: Aabb::new(Vec3::new(-0.2, 0.4375, -0.2), Vec3::new(1.175, 1.0, 1.175)),
            only_from_above: false,
        }]);
        let walk = Vec3::new(0.3, 0.0, 0.0);
        for version in [
            "1.8.8", "1.12.2", "1.13.2", "1.14.4", "1.16.5", "1.20.4", "1.21.11",
        ] {
            assert_eq!(
                backed_off_x(&world, sneaker(1.0), walk, version),
                0.3,
                "{version}"
            );
        }
    }

    #[test]
    fn the_fluid_hop_box_is_free_of_shapes_it_does_not_meet() {
        // Entity.isFree: only the shapes overlapping the box count (client1.21.11
        // BlockCollisions.java:86-104, 1.13.2 RenderBlockView.java:122); a fence's 1.5 high shape
        // in the cell beside does, a carpet under the box's cells does not
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 0.0625);
        let hop = Aabb::new(Vec3::new(0.2, 0.5, 0.2), Vec3::new(0.8, 2.3, 0.8));
        let context = EntityCollisionContext::default();
        let entity = CollisionQueryEntity {
            context: &context,
            bounds: hop,
            position: Vec3::new(0.5, 0.5, 0.5),
            outside_border: false,
        };
        for version in ["1.8.8", "1.12.2", "1.13.2", "1.21.11"] {
            let release = MinecraftVersion::parse(version).unwrap();
            assert!(world.is_free_for_entity(hop, entity, release), "{version}");
        }
        assert!(world.is_free(hop));
    }

    fn place_box(world: &mut WorldSnapshot, position: (i32, i32, i32), min: Vec3, max: Vec3) {
        world.set_block(
            position,
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: Aabb::new(min, max),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
    }

    /// The z of a sneaking player's move of 0.3 along +z from `position` after
    /// Player.maybeBackOffFromEdge with the 0.6 step.
    fn backed_off_z_from(world: &WorldSnapshot, position: Vec3, version: &str) -> f64 {
        let release = MinecraftVersion::parse(version).unwrap();
        let state = SimulationState {
            position,
            ..sneaker(position.y)
        };
        let context = entity_collision_context(&state);
        let walk = Vec3::new(0.0, 0.0, 0.3);
        back_off_from_edge(&state, world, &context, walk, 0.6, release).z
    }

    #[test]
    fn a_shape_the_sneaker_presses_within_1e_7_does_not_hold_it_from_1_13() {
        // from 1.13 a block's shape counts only where VoxelShapes.matchesAnywhere /
        // Shapes.joinIsNotEmpty(shape, box, AND) finds it, and the index merger merges coordinates
        // within 1e-7 (1.14 SimpleDoubleListPair.java:34-37, client1.21.11 IndirectMerger.java
        // ctor); a full cube takes the strict `intersects` from 1.16 (1.16.5
        // BlockCollisionSpliterator.java:75-82), every shape before 1.13 (1.12.2 Block.java
        // :395-399)
        let mut half = WorldSnapshot::new();
        place_box(&mut half, (0, 0, 0), Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0));
        // the box's min x 5e-8 inside the shape's face at x = 0.5
        let against_half = Vec3::new(0.8 - 5.0e-8, 0.0, 0.5);
        for (version, z) in [
            ("1.12.2", 0.3),
            // (1.13.2 deflates the lowered box by 1e-7, Entity.java:627)
            ("1.13.2", 0.0),
            ("1.14.4", 0.0),
            ("1.16.5", 0.0),
            ("1.20.4", 0.0),
        ] {
            assert_eq!(
                backed_off_z_from(&half, against_half, version),
                z,
                "{version}"
            );
        }
        // a full cube 5e-8 into the box: merged 1.13 - 1.15, strict before and from 1.16
        let mut cube = WorldSnapshot::new();
        place_box(&mut cube, (-1, 0, 0), Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let against_cube = Vec3::new(0.3 - 5.0e-8, 0.0, 0.5);
        for (version, z) in [
            ("1.12.2", 0.3),
            ("1.14.4", 0.0),
            ("1.15.2", 0.0),
            ("1.16.5", 0.3),
            ("1.20.4", 0.3),
        ] {
            assert_eq!(
                backed_off_z_from(&cube, against_cube, version),
                z,
                "{version}"
            );
        }
    }

    #[test]
    fn the_fluid_hop_box_is_free_of_a_face_it_overlaps_within_1e_7_from_1_13() {
        // Entity.isFree's shapes go through matchesAnywhere / joinIsNotEmpty from 1.13 (1.13.2
        // RenderBlockView.java:123, client1.21.11 BlockCollisions.java:86-104), whose merger merges
        // coordinates within 1e-7 (client1.21.11 IndirectMerger.java ctor); the strict
        // `intersects` before (1.12.2 Block.java:395-399)
        let mut world = WorldSnapshot::new();
        // a fence post, its side at x = 1.375
        place_box(
            &mut world,
            (1, 0, 0),
            Vec3::new(0.375, 0.0, 0.375),
            Vec3::new(0.625, 1.5, 0.625),
        );
        let context = EntityCollisionContext::default();
        let free = |overlap: f64, version: &str| {
            let max_x = 1.375 + overlap;
            let hop = Aabb::new(Vec3::new(max_x - 0.6, 0.6, 0.2), Vec3::new(max_x, 2.4, 0.8));
            let entity = CollisionQueryEntity {
                context: &context,
                bounds: hop.translated(Vec3::new(0.0, -0.6, 0.0)),
                position: Vec3::new(max_x - 0.3, 0.0, 0.5),
                outside_border: false,
            };
            let release = MinecraftVersion::parse(version).unwrap();
            world.is_free_for_entity(hop, entity, release)
        };
        assert!(!free(5.0e-8, "1.8.8"));
        assert!(!free(5.0e-8, "1.12.2"));
        for version in ["1.13.2", "1.14.4", "1.16.5", "1.20.4", "1.21.11", "26.3"] {
            assert!(free(5.0e-8, version), "{version}");
            assert!(!free(2.0e-7, version), "{version}");
        }
    }

    #[test]
    fn the_fluid_hop_box_is_free_of_scaffolding_the_entity_is_not_above() {
        // Entity.isFree takes the shapes in the entity's CollisionContext (client1.21.11
        // Entity.java:642-644): scaffolding is empty to an entity not above it
        // (ScaffoldingBlock.getCollisionShape :113-121)
        let mut world = WorldSnapshot::new();
        place_scaffolding(&mut world, (1, 0, 0), false);
        let hop = Aabb::new(Vec3::new(0.75, 0.6, 0.2), Vec3::new(1.35, 2.4, 0.8));
        let context = EntityCollisionContext {
            bottom: 0.0,
            ..EntityCollisionContext::default()
        };
        let entity = CollisionQueryEntity {
            context: &context,
            bounds: hop.translated(Vec3::new(0.0, -0.6, 0.0)),
            position: Vec3::new(1.05, 0.0, 0.5),
            outside_border: false,
        };
        let release = MinecraftVersion::parse("1.21.11").unwrap();
        assert!(world.is_free_for_entity(hop, entity, release));
        assert!(!world.is_free(hop));
    }

    #[test]
    fn the_world_border_collides_with_no_collision_queries() {
        // 1.18+: the border's shape where it meets the query, for an entity inside and close to it
        // (1.18 CollisionView.java:57-62); 1.14 - 1.17: its shape whatever the query once the
        // entity's box touches it from inside (client1.14.4 LevelReader.java:146-155, 1.16
        // BlockCollisionSpliterator.java:101-115)
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        world.set_chunk_loaded(0, 0, true);
        world.set_chunk_loaded(0, -1, true);
        let context = EntityCollisionContext::default();
        let entity_at = |x: f64| CollisionQueryEntity {
            context: &context,
            bounds: Aabb::new(Vec3::new(x - 0.3, 0.0, -0.3), Vec3::new(x + 0.3, 1.8, 0.3)),
            position: Vec3::new(x, 0.0, 0.0),
            outside_border: false,
        };
        let across = Aabb::new(Vec3::new(4.5, 0.0, -0.3), Vec3::new(5.1, 1.8, 0.3));
        let inside = Aabb::new(Vec3::new(4.0, 0.0, -0.3), Vec3::new(4.6, 1.8, 0.3));
        let release = |version| MinecraftVersion::parse(version).unwrap();
        assert!(!world.no_collision_for_entity(across, entity_at(4.5), release("1.21.11")));
        assert!(world.no_collision_for_entity(inside, entity_at(4.5), release("1.21.11")));
        // the box at the edge touches the border: every query collides before 1.18
        assert!(!world.no_collision_for_entity(inside, entity_at(4.7), release("1.17.1")));
        assert!(!world.no_collision_for_entity(inside, entity_at(4.7), release("1.14.4")));
        assert!(world.no_collision_for_entity(inside, entity_at(4.5), release("1.16.5")));
        // before 1.14 the cells outside are stone to an entity inside
        assert!(!world.no_collision_for_entity(across, entity_at(4.5), release("1.12.2")));
        assert!(world.no_collision_for_entity(inside, entity_at(4.5), release("1.12.2")));
    }

    #[test]
    fn the_1_16_2_push_out_tests_collision_shapes_and_before_any_suffocating_cell() {
        // 1.16 and 1.16.1 LocalPlayer.blocked(): any cell of the column isSuffocating, whatever its
        // shape (1.16 ClientPlayerEntity.java:452-464); 1.16.2 suffocatesAt: a suffocating shape
        // meets the column (1.16.5 ClientPlayerEntity.java:435-439)
        let mut world = WorldSnapshot::new();
        for y in 0..2 {
            for z in -1..=1 {
                world.set_block(
                    (0, y, z),
                    BlockState {
                        id: 1,
                        shape: None,
                        suffocating: Some(true),
                        ..BlockState::default()
                    },
                );
            }
        }
        assert!(pushed_out(&world, "1.16", true).position.x < 0.2);
        assert!(pushed_out(&world, "1.16.1", true).position.x < 0.2);
        assert_eq!(pushed_out(&world, "1.16.2", true).position.x, 0.2);
    }

    #[test]
    fn the_water_block_and_the_bubble_column_keep_their_speed_factor() {
        // Entity.getBlockSpeedFactor reads the block below only when the feet are in neither
        // Blocks.WATER nor Blocks.BUBBLE_COLUMN (1.15 Entity.java:676-684, 1.16.5 :736-744, 1.19.2
        // :745-753, 1.21.11 :996-1004, 26.3 :1131-1139); a waterlogged block or a water plant (speed
        // factor 1.0, its fluid water) does read it
        let soul_sand = BlockState {
            shape: Some(BlockShape {
                effect: BlockEffect::SoulSand,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let water = BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            water_block: true,
            ..BlockState::default()
        };
        let kelp = BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            ..BlockState::default()
        };
        let waterlogged = BlockState {
            fluid: FluidState::Waterlogged,
            fluid_source: true,
            ..BlockState::default()
        };
        let state = SimulationState {
            position: Vec3::new(0.5, 1.2, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            ..SimulationState::default()
        };
        for release in [
            MinecraftVersion::new(1, 15, 0),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 19, 2),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for (feet, factor) in [
                (water, 1.0_f32),
                (bubble_column(BlockEffect::BubbleUp), 1.0),
                (bubble_column(BlockEffect::BubbleDown), 1.0),
                (kelp, 0.4),
                (waterlogged, 0.4),
            ] {
                let mut world = WorldSnapshot::new();
                world.set_block((0, 0, 0), soul_sand);
                world.set_block((0, 1, 0), feet);
                assert_eq!(
                    crate::simulation::block_speed_factor(&world, &state, None, release),
                    factor,
                    "{release:?}"
                );
            }
        }
    }

    fn ladder_world() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Climbable,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    #[test]
    fn a_flying_player_is_not_on_a_climbable_from_1_21_4() {
        // Player.onClimbable: abilities.flying ? false : super.onClimbable() (1.21.4
        // PlayerEntity.java:2219-2221, 1.21.11 Player.java:2038-2040, 26.3 :1876-1878); 1.21.3 has
        // no override
        let world = ladder_world();
        let flying = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            flying: true,
            ..SimulationState::default()
        };
        let climbable = |state: &SimulationState, release: MinecraftVersion| {
            crate::simulation::climbable(&world, state, release)
        };
        assert!(climbable(&flying, MinecraftVersion::new(1, 21, 3)));
        assert!(!climbable(&flying, MinecraftVersion::new(1, 21, 4)));
        assert!(!climbable(&flying, MinecraftVersion::new(26, 3, 0)));
        let walking = SimulationState {
            flying: false,
            ..flying
        };
        assert!(climbable(&walking, MinecraftVersion::new(1, 21, 4)));
        let mob = SimulationState {
            is_player: false,
            ..flying
        };
        assert!(climbable(&mob, MinecraftVersion::new(1, 21, 4)));

        // so LivingEntity.handleOnClimbable no longer holds a flight through a ladder to 0.15
        let moved = |release: &str| {
            let mut state = SimulationState {
                velocity: Vec3::new(0.5, 0.0, 0.0),
                no_gravity: true,
                ..flying
            };
            crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state.position.x - 0.5
        };
        assert_eq!(moved("1.21.3"), f64::from(java_float(0.15)));
        assert_eq!(moved("1.21.4"), 0.5);
    }

    #[test]
    fn a_fall_through_a_cobweb_sticks_from_1_21_2() {
        // From 1.21.2 Entity.checkInsideBlocks calls WebBlock.entityInside for every block the
        // tick's move sweeps (1.21.4 Entity.java:1072-1100, boxTraverseBlocks; 1.21.11
        // Entity.java:859-913 and BlockGetter.java:161-195, WebBlock.java:26-34); before, only for
        // the blocks the box ends in (1.21.1)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 7, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let falling = SimulationState {
            position: Vec3::new(0.5, 8.5, 0.5),
            velocity: Vec3::new(0.0, -5.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let fallen = |release: &str| {
            let mut state = falling;
            crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state
        };
        let before = fallen("1.21.1");
        assert_eq!(before.position.y, 3.5);
        assert_eq!(before.stuck_speed_multiplier, Vec3::ZERO);
        assert!(before.fall_distance > 0.0);
        for release in ["1.21.2", "1.21.4", "1.21.5", "1.21.11", "26.3"] {
            let state = fallen(release);
            assert_eq!(state.position.y, 3.5, "{release}");
            assert_eq!(
                state.stuck_speed_multiplier,
                Vec3::new(0.25, java_float(0.05), 0.25),
                "{release}"
            );
            assert_eq!(state.fall_distance, 0.0, "{release}");
        }
    }

    #[test]
    fn a_faint_current_gets_the_minimum_push_from_1_21_2() {
        // Vec3.normalize is zero below 1.0E-4 to 1.21.1 and below 1.0E-5F from 1.21.2 (1.21.1
        // Vec3.java:63-65, 1.21.2 :71-73): Entity.updateMovementInFluid's 0.0045 minimum push of
        // a still player (1.21.4 Entity.java:3348-3349) needs the current's direction
        use crate::simulation::push_by_fluid;
        use crate::world::FluidContact;
        let contact = FluidContact {
            touching: true,
            height: 0.5,
            flow: Vec3::new(5.0e-5, 0.0, 0.0),
            flowing_blocks: 1,
        };
        for (release, pushed) in [
            (MinecraftVersion::new(1, 21, 1), 0.0),
            (MinecraftVersion::new(1, 21, 2), 0.0045000000000000005),
            (MinecraftVersion::new(1, 21, 11), 0.0045000000000000005),
        ] {
            let mut state = SimulationState {
                is_player: true,
                ..SimulationState::default()
            };
            push_by_fluid(&mut state, contact, 1.0, release);
            assert_eq!(state.velocity.x, pushed, "{release}");
        }
    }

    #[test]
    fn flight_undoes_the_sneak_slowdown_from_1_9_to_1_14() {
        // LocalPlayer.aiStep divides a flying player's sneak-slowed impulses by 0.3D in 1.9 - 1.14
        // (1.9.4 ClientPlayerEntity.java:712-715, 1.13.2 :796-799, 1.14 :717-721); 1.8.9 only
        // moves it down (ClientPlayerEntity.java:596-603)
        let speed = |release: MinecraftVersion, sneaking: bool| {
            let mut state = SimulationState {
                is_player: true,
                flying: true,
                flying_speed: java_float(0.05),
                sneaking,
                input: Vec3::new(0.0, 0.0, 1.0),
                position: Vec3::new(0.5, 10.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_release(&mut state, &WorldSnapshot::new(), release);
            state.velocity.z
        };
        for (release, undone) in [
            (MinecraftVersion::new(1, 8, 8), false),
            (MinecraftVersion::new(1, 9, 4), true),
            (MinecraftVersion::new(1, 12, 2), true),
            (MinecraftVersion::new(1, 13, 2), true),
            (MinecraftVersion::new(1, 14, 4), true),
            (MinecraftVersion::new(1, 15, 0), false),
        ] {
            let (sneaking, plain) = (speed(release, true), speed(release, false));
            if undone {
                assert_eq!(sneaking, plain, "{release:?}");
            } else {
                assert!(sneaking < plain * 0.31, "{release:?}: {sneaking} {plain}");
            }
        }
    }

    fn effect_world(effect: BlockEffect) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    effect,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world
    }

    fn glide_through(version: &str, effect: BlockEffect) -> SimulationState {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            velocity: Vec3::new(0.0, 0.0, 1.0),
            half_width: 0.3,
            height: 0.6,
            is_player: true,
            fall_flying: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(
            version,
            &mut state,
            &effect_world(effect),
            PhysicsConfig::default(),
        )
        .unwrap();
        state
    }

    #[test]
    fn a_climbable_block_stops_a_glide_from_1_21_5() {
        // LivingEntity.travelFallFlying: on a climbable block travelInAir, then stopFallFlying
        // (1.21.5 LivingEntity.java:2299-2302, 1.21.11 :2439-2442); 1.21.4 glides on
        // (LivingEntity.java:2205-2206)
        for version in ["1.21.5", "1.21.11", "26.3"] {
            let stopped = glide_through(version, BlockEffect::Climbable);
            assert!(!stopped.fall_flying, "{version}");
            // handleOnClimbable's 0.15F clamp, then the air's 0.91F drag
            assert_eq!(
                stopped.velocity.z,
                java_float(0.15) * java_float(0.91),
                "{version}"
            );
        }
        let gliding = glide_through("1.21.4", BlockEffect::Climbable);
        assert!(gliding.fall_flying);
        assert!(gliding.velocity.z > 0.9, "{}", gliding.velocity.z);
    }

    #[test]
    fn a_glide_passes_through_vines_from_1_21_11() {
        // LivingEntity.onClimbable: a fall-flying entity is not on a #can_glide_through block
        // (vines, twisting, weeping and cave vines) from 1.21.11 (1.21.11 LivingEntity.java:
        // 1675-1676, data/minecraft/tags/block/can_glide_through.json; 26.3 :1671)
        assert!(!glide_through("1.21.10", BlockEffect::GlideThroughClimbable).fall_flying);
        for version in ["1.21.11", "26.3"] {
            let gliding = glide_through(version, BlockEffect::GlideThroughClimbable);
            assert!(gliding.fall_flying, "{version}");
            assert!(gliding.velocity.z > 0.9, "{version}");
        }
        // a vine is still climbable when the entity does not glide
        let world = effect_world(BlockEffect::GlideThroughClimbable);
        let walking = SimulationState {
            position: Vec3::new(0.5, 2.0, 0.5),
            is_player: true,
            ..SimulationState::default()
        };
        assert!(crate::simulation::climbable(
            &world,
            &walking,
            MinecraftVersion::new(1, 21, 11)
        ));
    }

    #[test]
    fn auto_jump_jumps_onto_the_entities_the_player_collides_with() {
        // LocalPlayer.updateAutoJump scans Level.getCollisions, which holds the boxes of a boat or
        // a shulker (1.10.2 ClientPlayerEntity.java:874, 1.21.11 LocalPlayer.java:1018,
        // CollisionGetter.java:85-89)
        let mut world = floor_with_step(0.0);
        world.set_block((0, 1, 2), BlockState::default());
        world.set_entity_colliders(vec![EntityCollider {
            bounds: Aabb::new(Vec3::new(-0.2, 1.0, 2.0), Vec3::new(1.2, 1.5625, 3.0)),
            only_from_above: false,
        }]);
        for release in [
            MinecraftVersion::new(1, 10, 0),
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 17, 1),
            MinecraftVersion::new(26, 2, 0),
        ] {
            let mut state = walking_to_step(true);
            tick_release(&mut state, &world, release);
            assert_eq!(state.auto_jump_time, 1, "{release:?}");
        }
    }

    #[test]
    fn auto_jump_meets_the_entities_before_the_blocks_from_1_18() {
        // Level.getCollisions: blocks then entities before 1.18 (1.16.5 CollisionView.java:58-59,
        // 1.17 :57-58), entities then blocks from 1.18 (1.18 CollisionView.java:67-70); the first
        // box a line crosses is the step, here a box too high to jump in a block high step
        let mut world = floor_with_step(1.0);
        world.set_entity_colliders(vec![EntityCollider {
            bounds: Aabb::new(Vec3::new(0.1, 1.0, 2.0), Vec3::new(0.9, 2.5, 2.9)),
            only_from_above: false,
        }]);
        let queued = |release| {
            let mut state = walking_to_step(true);
            tick_release(&mut state, &world, release);
            state.auto_jump_time
        };
        assert_eq!(queued(MinecraftVersion::new(1, 17, 1)), 1);
        assert_eq!(queued(MinecraftVersion::new(1, 18, 0)), 0);
        assert_eq!(queued(MinecraftVersion::new(26, 2, 0)), 0);
    }

    #[test]
    fn flight_takes_sneak_and_jump_as_two_steps_before_1_14() {
        // LocalPlayer.aiStep: motionY -= flySpeed * 3.0F, then += it, before 1.14 (1.8.9
        // ClientPlayerEntity.java:596-603, 1.13.2 :797-805); from 1.14 one netted impulse (1.14
        // ClientPlayerEntity.java:719-731), none with both held
        let world = WorldSnapshot::new();
        let flying_y = |both: bool, release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
                is_player: true,
                flying: true,
                flying_speed: f64::from(0.05_f32),
                jump_requested: both,
                sneaking: both,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.velocity.y
        };
        let legacy = MinecraftVersion::new(1, 13, 2);
        let impulse = f64::from(0.05_f32 * 3.0);
        assert_eq!(-0.0784000015258789 - impulse + impulse, -0.07840000152587889);
        assert_ne!(flying_y(true, legacy), flying_y(false, legacy));
        assert_ne!(
            flying_y(true, MinecraftVersion::new(1, 8, 8)),
            flying_y(false, MinecraftVersion::new(1, 8, 8))
        );
        let modern = MinecraftVersion::new(1, 14, 0);
        assert_eq!(flying_y(true, modern), flying_y(false, modern));
    }

    #[test]
    fn auto_jump_starts_from_the_position_from_1_15() {
        // LocalPlayer.updateAutoJump: from position() in 1.15 (1.15 ClientPlayerEntity.java:860), from
        // the box's minY in 1.14 (1.14 ClientPlayerEntity.java:851-852, 1.13.2 :887-888); a box kept
        // 0.45 below the position puts a block high step 1.45 above the feet, too high to jump
        let world = floor_with_step(1.0);
        let queued = |release| {
            let mut state = walking_to_step(true);
            state.bounding_box = Some(
                state
                    .bounds_around_position()
                    .translated(Vec3::new(0.0, -0.45, 0.0)),
            );
            update_auto_jump(
                &mut state,
                &world,
                Vec3::new(0.0, 0.0, 0.1),
                0.1,
                1.0,
                release,
            );
            state.auto_jump_time
        };
        assert_eq!(queued(MinecraftVersion::new(1, 14, 4)), 0);
        assert_eq!(queued(MinecraftVersion::new(1, 15, 0)), 1);
        assert_eq!(queued(MinecraftVersion::new(1, 16, 0)), 1);
    }

    #[test]
    fn auto_jump_takes_the_merged_full_cubes_last_in_1_13_and_1_14_0() {
        // getCollisionShapes keeps the full cubes for one bitset shape after the walk, split into
        // merged boxes (1.13.2 RenderBlockView.java:113-153, VoxelSet.java:203-256; 1.14
        // ViewableWorld.java:145-195); the walk takes each block from 1.14.4 (LevelReader.java:130-145)
        // and before 1.13. The step of two cubes along the way is one box whose center is over the
        // farther cube, free above, where the nearer cube's is under a block: a jump only in 1.13.2
        let mut world = floor_with_step(1.0);
        let stone = world.block((0, 1, 2));
        world.set_block((0, 1, 3), stone);
        world.set_block((0, 2, 2), stone);
        let queued = |release| {
            let mut state = walking_to_step(true);
            tick_release(&mut state, &world, release);
            state.auto_jump_time
        };
        assert_eq!(queued(MinecraftVersion::new(1, 13, 2)), 1);
        assert_eq!(queued(MinecraftVersion::new(1, 14, 0)), 1);
        assert_eq!(queued(MinecraftVersion::new(1, 12, 2)), 0);
        assert_eq!(queued(MinecraftVersion::new(1, 14, 4)), 0);
        assert_eq!(queued(MinecraftVersion::new(1, 15, 0)), 0);
    }

    #[test]
    fn the_merged_full_cubes_follow_the_partial_boxes() {
        // 1.13.2 RenderBlockView.java:150-152: the bitset shape is concatenated after the walk; its
        // boxes run along z, then grow along x and y (VoxelSet.java:203-256)
        let mut world = WorldSnapshot::new();
        let full = BlockState {
            id: 1,
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        let slab = BlockState {
            id: 2,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        world.set_block((0, 0, 0), full);
        world.set_block((0, 0, 1), full);
        world.set_block((1, 0, 0), full);
        world.set_block((1, 0, 1), full);
        world.set_block((0, 1, 0), full);
        world.set_block((2, 0, 0), slab);
        let query = Aabb::new(Vec3::new(0.1, 0.1, 0.1), Vec3::new(2.9, 1.9, 1.9));
        let boxes =
            world.block_collision_boxes_merging_full_cubes(query, &EntityCollisionContext::default());
        assert_eq!(
            boxes,
            vec![
                Aabb::new(Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 0.5, 1.0)),
                Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 2.0)),
                Aabb::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 2.0, 1.0)),
            ]
        );
    }

    #[test]
    fn negative_byte_effect_amplifiers_stay_active_before_1_20_5() {
        // the effect packet reads the amplifier as a signed byte and MobEffectInstance keeps it
        // before 1.20.5 (1.20.4 EntityStatusEffectS2CPacket.java:49, StatusEffectInstance.java:73;
        // 1.8.9 EntityStatusEffectS2CPacket.java:60), so `/effect ... 255` is amplifier -1
        let air = |version: &str, levitation_amplifier: Option<i32>| {
            let mut state = SimulationState {
                is_player: true,
                levitation: true,
                levitation_amplifier,
                position: Vec3::new(0.5, 20.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(
                version,
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
            )
            .unwrap();
            state
        };
        // travel's Levitation: motionY += (0.05 * (amplifier + 1) - motionY) * 0.2, a hover at -1
        // (1.20.4 LivingEntity.java:2157; 1.9.4 EntityLivingBase.java travel)
        for version in ["1.9.4", "1.12.2", "1.16.5", "1.20.4"] {
            let state = air(version, Some(-1));
            assert_eq!(state.velocity.y, 0.0, "{version}");
            assert_eq!(state.position.y, 20.0, "{version}");
            assert!(air(version, Some(-2)).velocity.y < 0.0, "{version}");
        }

        // jumpFromGround adds 0.1F * (amplifier + 1): nothing at -1, less at -2 (1.20.4
        // LivingEntity.java:2017, 1.8.9 EntityLivingBase.java jump)
        let jump =
            |version: &str, jump_boost_amplifier: Option<i32>, jump_strength: Option<f64>| {
                let mut state = SimulationState {
                    is_player: true,
                    on_ground: true,
                    jump_requested: true,
                    jump_boost_amplifier,
                    jump_strength,
                    position: Vec3::new(0.5, 20.0, 0.5),
                    half_width: 0.3,
                    height: 1.8,
                    controlled_by_local_instance: true,
                    ..SimulationState::default()
                };
                crate::versioned::dispatch(
                    version,
                    &mut state,
                    &WorldSnapshot::new(),
                    PhysicsConfig::default(),
                )
                .unwrap();
                state.velocity.y
            };
        for version in ["1.8.8", "1.12.2", "1.16.5", "1.20.4"] {
            assert_eq!(
                jump(version, Some(-1), None),
                jump(version, None, None),
                "{version}"
            );
            let lowered = f64::from(0.41999998688697815_f64 as f32 + 0.1_f32 * -1.0);
            assert_eq!(
                jump(version, Some(-2), None),
                jump(version, None, Some(lowered)),
                "{version}"
            );
        }

        // computeFallDamage's jump boost is (float)(amplifier + 1) (1.20.4 LivingEntity.java:1561)
        assert_eq!(
            landing_damage(4.25, 3.0, 1.0, Some(-1), LandingDamageRounding::Ceil),
            landing_damage(4.25, 3.0, 1.0, None, LandingDamageRounding::Ceil)
        );
        assert_eq!(
            landing_damage(4.25, 3.0, 1.0, Some(-2), LandingDamageRounding::Ceil),
            3.0
        );
    }

    #[test]
    fn a_folded_movement_speed_value_replaces_the_engine_fold() {
        // LivingEntity.getSpeed is the attribute's value with every modifier (1.21.11
        // AttributeInstance.java:150-172): a caller that folds them passes it whole
        let run = |state: SimulationState| {
            let mut state = SimulationState {
                is_player: true,
                input: Vec3::new(0.0, 0.0, 1.0),
                position: Vec3::new(0.5, 20.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                ..state
            };
            crate::versioned::dispatch(
                "1.21.11",
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
            )
            .unwrap();
            state.velocity
        };
        let folded = run(SimulationState {
            sprinting: true,
            sprint_modifier: Some(true),
            speed_effect_amplifier: Some(1),
            movement_speed: Some(0.1),
            movement_speed_value: Some(0.25),
            ..SimulationState::default()
        });
        let plain = run(SimulationState {
            sprinting: true,
            sprint_modifier: Some(false),
            movement_speed: Some(0.25),
            ..SimulationState::default()
        });
        assert_eq!(folded, plain);
    }

    #[test]
    fn slow_falling_tests_the_delta_after_the_air_move_from_1_21_2() {
        // from 1.21.2 travelInAir calls getEffectiveGravity after
        // handleRelativeFrictionAndCalculateMovement's move (1.21.2 LivingEntity.java:2194-2197,
        // 2213-2229; 26.3 :2337-2340, 2382-2397); 1.21 travel tests the delta before the move
        // (1.21 LivingEntity.java:2095-2101). A jump into a ceiling leaves 0 after the move.
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 22, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let ceiling = |version: &str| {
            let mut state = SimulationState {
                is_player: true,
                slow_falling: true,
                position: Vec3::new(0.5, 20.1, 0.5),
                velocity: Vec3::new(0.0, 0.2, 0.0),
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!((state.position.y - 20.2).abs() < 1e-9, "{version}");
            state.velocity.y
        };
        let drag = f64::from(0.98_f32);
        for version in ["1.21.2", "1.21.4", "1.21.11", "26.3"] {
            assert_eq!(ceiling(version), (0.0 - 0.01) * drag, "{version}");
        }
        for version in ["1.20.5", "1.21"] {
            assert_eq!(ceiling(version), (0.0 - 0.08) * drag, "{version}");
        }
    }
}
