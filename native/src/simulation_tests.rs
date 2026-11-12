#[cfg(test)]
mod tests {
    use crate::math::java_float;
    use crate::simulation::{ServerFireDraws, 
        apply_inside_fire_effects, back_off_from_edge, entity_collision_context, landing_damage,
        movement_speed_attribute, tick, tick_with_rules, InsideEffectStep, InsideEffects, LandingDamageRounding, PhysicsConfig, SimulationState,
        TickInputSnapshot, TickResult, VersionPhysicsProfile, VersionPhysicsRules,
        update_auto_jump, BoatVehicle, entity_fluid_contact, entity_eyes_in_fluid,
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
        // (1.20.4 LivingEntity.java:1572), with safe_fall_distance standing for the 3.0F
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
        // 1.20.4 LivingEntity.java:1572: float ceil, Jump Boost subtracted
        assert_eq!(landing_damage(5.0, 3.0, 1.0, None, LandingDamageRounding::Ceil), 2.0);
        assert_eq!(landing_damage(5.0, 3.0, 1.0, Some(0), LandingDamageRounding::Ceil), 1.0);
        // the float subtraction: 3.0000001f - 3.0f is one ulp, ceil 1
        let just_over = f64::from(f32::from_bits(3.0_f32.to_bits() + 1));
        assert_eq!(landing_damage(just_over, 3.0, 1.0, None, LandingDamageRounding::Ceil), 1.0);
        // 1.20.5 LivingEntity.java:1597-1599 (1.21.4 :1625-1627): float, no epsilon, ceil, so an
        // exact excess is not rounded up and fd == safe deals nothing
        let v1_20_5 = MinecraftVersion::new(1, 20, 5);
        let v1_21_4 = MinecraftVersion::new(1, 21, 4);
        assert_eq!(attribute_landing_damage(5.0, 3.0, 1.0, 1.0, LandingDamageRounding::Ceil, v1_20_5), 2.0);
        assert_eq!(attribute_landing_damage(3.0, 3.0, 1.0, 1.0, LandingDamageRounding::Ceil, v1_21_4), 0.0);
        assert_eq!(attribute_landing_damage(4.25, 3.0, 1.0, 1.0, LandingDamageRounding::Ceil, v1_21_4), 2.0);
        assert_eq!(attribute_landing_damage(4.25, 3.0, 1.0, 0.5, LandingDamageRounding::Ceil, v1_21_4), 1.0);
        // 1.21.5 LivingEntity.java:1643-1653: floor((fd + 1.0E-6 - safe) * multiplier)
        let v1_21_5 = MinecraftVersion::new(1, 21, 5);
        assert_eq!(attribute_landing_damage(5.0, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 2.0);
        assert_eq!(attribute_landing_damage(4.9999995, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 2.0);
        assert_eq!(attribute_landing_damage(4.25, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 1.0);
        assert_eq!(attribute_landing_damage(3.0, 3.0, 1.0, 1.0, LandingDamageRounding::Floor, v1_21_5), 0.0);
    }

    #[test]
    fn release_profiles_pick_the_vanilla_fall_damage_rounding() {
        use crate::simulation::VersionPhysicsProfile;
        // 1.21.4 LivingEntity.java:1627 ceil, 1.21.5 :1648 floor
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
        // 1.21.5 Entity.java:1323: a double field, fd -= (double)(float)dy
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
        assert_eq!(state.velocity.x, 0.5 * java_float(0.91));
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
            controlled_by_local_instance: true,
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
        state.velocity.z
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
            0.1 * java_float(0.4) * java_float(0.91)
        );
        assert_eq!(
            moved(BlockEffect::SoulSoil, hovering),
            0.1 * java_float(0.91)
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
        let icy = java_float(java_float(0.98) * java_float(0.91));

        for release in [
            MinecraftVersion::new(1, 14, 4),
            MinecraftVersion::new(1, 19, 4),
            MinecraftVersion::new(1, 21, 1),
        ] {
            assert_eq!(
                drag_on(&world, Vec3::new(0.5, 1.5, 0.5), release),
                0.1 * icy
            );
        }
    }

    #[test]
    fn friction_follows_the_supporting_block_from_1_20() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), ice());
        let over_the_edge = Vec3::new(1.1, 1.0, 0.5);

        assert_eq!(
            drag_on(&world, over_the_edge, MinecraftVersion::new(1, 20, 0)),
            0.1 * java_float(java_float(0.98) * java_float(0.91))
        );
        assert_eq!(
            drag_on(&world, over_the_edge, MinecraftVersion::new(1, 19, 4)),
            0.1 * java_float(java_float(0.6) * java_float(0.91))
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
    fn entity_state_clones_and_merges_keep_the_entitys_own_gravity() {
        use crate::entity::{Dimensions, EntityFlags, EntityState};
        // Entity.isNoGravity (1.21.11 Entity.java:1443-1445) and the GRAVITY and STEP_HEIGHT
        // attributes (1.21.11 Attributes.java:26-66, 42-80) are the entity's own: a prediction that
        // goes through EntityState and back keeps them
        let full = SimulationState {
            position: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(0.0, 0.5, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            gravity: Some(0.04),
            step_height: Some(1.0),
            sprinting: true,
            ..SimulationState::default()
        };
        let entity = EntityState::from(full);
        assert!(entity.no_gravity);
        assert_eq!(entity.gravity, Some(0.04));
        assert_eq!(entity.step_height, Some(1.0));
        let cloned = SimulationState::from(entity);
        assert!(cloned.no_gravity);
        assert_eq!(cloned.gravity, Some(0.04));
        assert_eq!(cloned.step_height, Some(1.0));

        let predicted = EntityState {
            position: Vec3::new(4.0, 5.0, 6.0),
            velocity: Vec3::ZERO,
            dimensions: Dimensions::new(0.3, 1.5),
            flags: EntityFlags {
                on_ground: true,
                ..EntityFlags::default()
            },
            no_gravity: false,
            gravity: None,
            step_height: Some(0.6),
        };
        let mut merged = full;
        merged.merge_entity_state(predicted);
        assert_eq!(merged.position, predicted.position);
        assert_eq!(merged.height, 1.5);
        assert!(merged.on_ground);
        assert!(!merged.no_gravity);
        assert_eq!(merged.gravity, None);
        assert_eq!(merged.step_height, Some(0.6));
        // what EntityState has no place for stays
        assert!(merged.sprinting);

        // and the merged state ticks with them: no gravity keeps it in the air
        let mut floating = SimulationState::from(EntityState {
            no_gravity: true,
            ..entity
        });
        floating.velocity = Vec3::ZERO;
        floating.on_ground = false;
        tick(&mut floating, &WorldSnapshot::new(), PhysicsConfig::default());
        assert_eq!(floating.velocity.y, 0.0);
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
        // 1.8.8 Entity.java:937-938: Mth.sin(this.yaw * (float) Math.PI / 180.0F); from
        // 1.9.4 Entity.java:1046-1047: this.yaw * (float) (Math.PI / 180.0). At 28.927 degrees
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
    fn only_the_client_sinks_over_a_column_not_loaded_and_a_column_that_loads_ends_it() {
        // world.isClient && !loaded (1.8.8 LivingEntity.java:1177-1181): the server takes gravity.
        // Leaving a loaded column, an unloaded destination and a re-entry differ by the column alone.
        let mut world = WorldSnapshot::new();
        world.set_chunk_loaded(0, 0, true);
        let fall = |world: &WorldSnapshot, version: &str, x: f64, client_side: bool| {
            let mut state = SimulationState {
                position: Vec3::new(x, 5.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                client_side,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default()).unwrap();
            (state.velocity.y, state.tick_count)
        };
        let sinking = -0.1 * java_float(0.98);
        let falling = -0.08 * java_float(0.98);
        for version in ["1.8.8", "1.9.4", "1.13.2"] {
            assert_eq!(fall(&world, version, 20.5, true), (sinking, 1), "{version}");
            assert_eq!(fall(&world, version, 20.5, false), (falling, 1), "{version}");
            assert_eq!(fall(&world, version, 0.5, true), (falling, 1), "{version}");
        }
        // the client's own player keeps ticking over the empty column in every release, counters too
        for version in ["1.14.4", "1.16.5", "1.21.1", "1.21.2", "26.3"] {
            assert_eq!(fall(&world, version, 20.5, true).1, 1, "{version}");
        }
        world.set_chunk_loaded(1, 0, true);
        assert_eq!(fall(&world, "1.13.2", 20.5, true), (falling, 1));
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
                client_side: true,
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

        assert_eq!(state.velocity.x, 0.2 * java_float(0.91));
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

        assert_eq!(legacy.x, 0.5 * java_float(0.91));
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
            controlled_by_local_instance: true,
            client_side: true,
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
        assert!((horizontal_speed - java_float(0.02) * java_float(0.91)).abs() < 1e-12);
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
        let ground_drag = java_float(java_float(0.6) * java_float(0.91));
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

        // clamp(1 - (1 - 0.91) * 0.5, 0, 1) == 0.955, in float.
        let drag = f64::from(1.0_f32 - (1.0_f32 - 0.91_f32) * 0.5_f32);
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
        // d * (-1 + lift * 0.75) (1.20.5 LivingEntity.java:2107, Entity.java:1143-1145; 1.21.11
        // Entity.java:1443-1445, LivingEntity.java:2434-2441); before, the glide's 0.08 ignores
        // NoGravity (1.16.5 LivingEntity.java:1961)
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
    fn a_changed_pose_height_rebuilds_the_kept_box_from_its_corner_with_the_float_width() {
        // 1.16.5 walk/sneak_br t=20: aiStep left a box 1.1e-16 narrower than 0.6F; the pose update
        // to standing rebuilds it from its corner (Entity#refreshDimensions, 1.16.5
        // Entity.java:2326-2327), and the recorded end-of-tick box is that one
        let kept = Aabb::new(
            Vec3::new(-1.1436244409274199, 100.0, -1.1436244409274199),
            Vec3::new(-0.5436244170855621, 101.5, -0.5436244170855621),
        );
        assert_ne!(kept.max.x - kept.min.x, f64::from(0.6_f32));
        let mut state = SimulationState {
            half_width: 0.3,
            height: 1.5,
            bounding_box: Some(kept),
            ..SimulationState::default()
        };
        crate::simulation::set_pose_height(&mut state, f64::from(1.8_f32));
        assert_eq!(
            state.bounding_box,
            Some(Aabb::new(
                kept.min,
                Vec3::new(-0.54362441708556197, 100.0 + f64::from(1.8_f32), -0.54362441708556197),
            ))
        );
        // the same size again leaves the box: the size only changes on a changed pose
        let squared = state.bounding_box;
        state.bounding_box = Some(kept);
        crate::simulation::set_pose_height(&mut state, f64::from(1.8_f32));
        assert_eq!(state.bounding_box, Some(kept));
        assert_ne!(Some(kept), squared);
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
        // Player.java:1473, 1.21.11 Player.java:1326, 26.3 :1301)
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
        // 1.20.5 (1.20.5 LivingEntity.java:2290, 1.21.11 :2522, 26.3 :2609)
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

        let ground_drag = java_float(java_float(0.6) * java_float(0.91));
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
    fn a_ground_jump_too_weak_to_lift_still_starts_the_delay() {
        // LivingEntity.aiStep sets noJumpDelay after jumpFromGround, whatever the power: the
        // refusal is inside jumpFromGround (1.20.5 LivingEntity.java:2074-2086, 26.3 :2303-2315,
        // :3024-3027)
        for strength in [0.0, 1.0e-6] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                on_ground: true,
                jump_requested: true,
                jump_strength: Some(strength),
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            let rules = VersionPhysicsRules {
                release: MinecraftVersion::new(1, 21, 1),
                ..VersionPhysicsRules::default()
            };
            tick_with_rules(
                &mut state,
                &WorldSnapshot::new(),
                PhysicsConfig::default(),
                rules,
            );

            assert_eq!(state.jump_ticks, 10);
            assert_eq!(state.velocity.y, 0.0);
        }
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
    // (1.21.3 Entity.java:698-699), which keeps them for a zero-Y move of an entity that is not
    // controlled locally (`Math.abs(movement.y) > 0.0 || isLocalPlayerOrLogicalSideForUpdatingMovement()`,
    // 1.21.4 Entity.java:707).
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
    // (1.21.3 Entity.java:709), isLocalInstanceAuthoritative from 1.21.5 (1.21.5 Entity.java:743).
    // Before, it runs for every entity (1.21 Entity.java:656).
    #[test]
    fn a_remote_entity_gains_no_fall_distance_on_the_client_from_1_21_2() {
        let fallen = |rules: VersionPhysicsRules, controlled_by_local_instance: bool| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.0),
                half_width: 0.3,
                height: 1.8,
                client_side: true,
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

    // Entity.checkSupportingBlock (1.21.11 Entity.java:699-722, the same from 1.20
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
            check_supporting_block(&world, &mut state, moved, MinecraftVersion::new(26, 3, 0)),
            Some((0, 0, 0))
        );
        assert!(!state.on_ground_no_blocks);
        assert!(state.supporting_block_kept);

        let mut state = SimulationState {
            on_ground_no_blocks: true,
            ..walked_off
        };
        assert_eq!(check_supporting_block(&world, &mut state, moved, MinecraftVersion::new(26, 3, 0)), None);
        assert!(state.on_ground_no_blocks);

        let mut state = SimulationState {
            supporting_block: Some((5, 5, 5)),
            supporting_block_kept: true,
            ..walked_off
        };
        assert_eq!(
            check_supporting_block(&world, &mut state, None, MinecraftVersion::new(26, 3, 0)),
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
        assert_eq!(check_supporting_block(&world, &mut state, moved, MinecraftVersion::new(26, 3, 0)), None);
        assert!(!state.on_ground_no_blocks);
    }

    // Entity.setOnGround(false) empties mainSupportingBlockPos and onGroundNoBlocks (1.21.11
    // Entity.java:680-684), which Player.tick does each tick the player rides (Player.java:223-226)
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
    // (1.21.11 CollisionGetter.java:139-154): powder snow holds up an entity that walks on it
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
            check_supporting_block(&world, &mut state, None, MinecraftVersion::new(26, 3, 0))
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
        // 1.16 Entity.updateInWaterStateAndDoFluidPushing returns once touching water, before it measures the lava
        // (1.16 Entity.java:1039-1048); 1.16.2 always measures it (1.16.5 Entity.java:1043-1049)
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
        // 1.16 Entity.isInLava is the flag LiquidBlock.entityInside sets in the move, for a
        // lava block whose float top is above the box's foot (1.16 Entity.java:660-662,
        // 1160-1166, LiquidBlock.java:173-180); from 1.16.2 the lava height of the base tick
        // (1.16.5 Entity.java:1178-1180)
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
    fn the_eyes_count_from_after_the_move_from_26_1() {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_level: 0,
                ..BlockState::default()
            },
        );
        let sinking = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                velocity: Vec3::new(0.0, -0.3, 0.0),
                is_player: true,
                half_width: 0.3,
                height: 1.8,
                eye_height: Some(1.62),
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.eye_in_lava
        };

        assert!(sinking(MinecraftVersion::new(26, 1, 0)));
        assert!(!sinking(MinecraftVersion::new(1, 21, 11)));
    }

    #[test]
    fn the_eyes_count_from_after_the_move_before_1_16() {
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
        let sinking = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                velocity: Vec3::new(0.0, -0.3, 0.0),
                is_player: true,
                half_width: 0.3,
                height: 1.8,
                eye_height: Some(1.62),
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.eye_in_water
        };

        assert!(sinking(MinecraftVersion::new(1, 15, 2)));
        assert!(!sinking(MinecraftVersion::new(1, 16, 5)));
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

    fn lava_against_a_wall(velocity: Vec3, no_gravity: bool, release: &str) -> SimulationState {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        let lava = BlockState {
            fluid: FluidState::Lava,
            fluid_level: 8,
            fluid_source: true,
            ..BlockState::default()
        };
        for y in 0..3 {
            world.set_block((0, y, 0), lava);
            world.set_block_shapes((1, y, 0), BlockState::default(), vec![BlockShape::full_block()]);
            world.set_block_shapes((0, y, 1), BlockState::default(), vec![BlockShape::full_block()]);
        }
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            velocity,
            in_lava: true,
            no_gravity,
            half_width: 0.3,
            height: 1.8,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default())
            .expect("a supported release");
        state
    }

    #[test]
    fn lavas_gravity_adds_the_zeros_that_clear_a_walls_negative_zero() {
        let into_the_walls = Vec3::new(0.5, 0.0, 0.5);
        // from 26.2 a wall leaves -x * 0.0, which the 0.0 of Vec3.add turns into 0.0
        for release in ["26.2", "26.3"] {
            let state = lava_against_a_wall(into_the_walls, false, release);
            assert_eq!(state.velocity.x.to_bits(), 0.0_f64.to_bits(), "{release}");
            assert_eq!(state.velocity.z.to_bits(), 0.0_f64.to_bits(), "{release}");
            // no gravity, no add: the -0.0 stays
            let state = lava_against_a_wall(into_the_walls, true, release);
            assert_eq!(state.velocity.x.to_bits(), (-0.0_f64).to_bits(), "{release}");
            assert_eq!(state.velocity.z.to_bits(), (-0.0_f64).to_bits(), "{release}");
        }
        // before, the wall sets 0.0
        let state = lava_against_a_wall(into_the_walls, false, "26.1");
        assert_eq!(state.velocity.x.to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn past_a_corner_of_the_border_a_walk_stops_at_its_edges_from_1_18() {
        let walk = |release: &str| {
            let mut world = WorldSnapshot::new();
            load_chunks_around_origin(&mut world);
            world.set_world_bounds(-64, 320, 8.5, 8.5, 10.0);
            for x in 0..4 {
                for z in 0..4 {
                    world.set_block_shapes((x, 99, z), BlockState::default(), vec![BlockShape::full_block()]);
                }
            }
            let mut state = SimulationState {
                position: Vec3::new(2.6, 100.0, 2.6),
                velocity: Vec3::new(0.15, 0.0, 0.15),
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default())
                .expect("a supported release");
            state
        };
        for release in ["1.18", "26.3"] {
            let state = walk(release);
            assert!((state.position.x - 2.7).abs() < 1.0e-9, "{release}: {}", state.position.x);
            assert!((state.position.z - 2.7).abs() < 1.0e-9, "{release}: {}", state.position.z);
            assert!(state.collided_horizontally, "{release}");
        }
        // an entity outside has the shape from 1.18 alone
        let state = walk("1.17.1");
        assert!((state.position.x - 2.75).abs() < 1.0e-9, "{}", state.position.x);
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
    fn water_gravity_uses_version_rule_divisor() {
        // getFluidFallingAdjustedMovement subtracts gravity / 16.0 in water (1.20.4
        // LivingEntity.java:2219-2222, 1.21.11 :2524-2527); the profile's divisor is the one used
        let fall = |divisor: Option<f64>| {
            let mut world = WorldSnapshot::new();
            load_chunks_around_origin(&mut world);
            world.set_block(
                (0, 0, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_level: 8,
                    ..BlockState::default()
                },
            );
            let mut rules = VersionPhysicsRules::default();
            if let Some(divisor) = divisor {
                rules.water_gravity_divisor = divisor;
            }
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.2, 0.5),
                no_gravity: false,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            state.velocity.y
        };

        assert_eq!(VersionPhysicsRules::default().water_gravity_divisor, 16.0);
        assert!((fall(None) + 0.08 / 16.0).abs() < 1.0e-12);
        assert!((fall(Some(8.0)) + 0.08 / 8.0).abs() < 1.0e-12);
    }

    #[test]
    fn a_tick_past_the_int_range_does_not_panic_on_either_floor() {
        for release in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 21, 11), MinecraftVersion::new(26, 3, 0)] {
            for coordinate in [-3.0e9, 3.0e9, -2147483648.5, 2147483647.5] {
                let mut rules = VersionPhysicsRules::default();
                rules.release = release;
                let mut state = SimulationState {
                    position: Vec3::new(coordinate, 70.0, coordinate),
                    half_width: 0.3,
                    height: 1.8,
                    ..SimulationState::default()
                };
                let world = WorldSnapshot::new();
                tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
                assert!(state.position.is_finite());
            }
        }
    }

    #[test]
    fn default_config_keeps_the_profile_gravity_and_step_height() {
        // LivingEntity.travel's 0.08 gravity and the 0.6F step height (1.20.4
        // LivingEntity.java:2038, :245) come from the profile unless the config sets other values
        let fall = |config: PhysicsConfig| {
            let mut rules = VersionPhysicsRules::default();
            rules.gravity = 0.04;
            let mut state = SimulationState {
                position: Vec3::new(0.5, 10.0, 0.5),
                no_gravity: false,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_with_rules(&mut state, &WorldSnapshot::new(), config, rules);
            state.velocity.y
        };
        assert!((fall(PhysicsConfig::default()) + 0.04 * java_float(0.98)).abs() < 1.0e-12);
        let custom = PhysicsConfig {
            gravity: 0.02,
            ..PhysicsConfig::default()
        };
        assert!((fall(custom) + 0.02 * java_float(0.98)).abs() < 1.0e-12);

        let mut rules = VersionPhysicsRules::default();
        rules.step_height = 0.0;
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
        tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
        assert!(state.position.x < 0.8);
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
        // whatever the water (1.21.4 LivingEntity.java:317-322, Entity.java:1295-1297)
        let mut state = falling;
        crate::versioned::dispatch("1.21.4", &mut state, &world, PhysicsConfig::default())
            .unwrap();
        assert!(state.in_water);
        assert_eq!(state.fall_distance, 1.5);

        // from 1.21.5 the drop counts only out of the water (1.21.5 Entity.java:1322-1324)
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
        // a float field: (float)((double)fallDistance - dy) (1.8.8 Entity.java:802)
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

        // (1.8.8 has no NoGravity: the water takes its 0.02 off whatever the flag, 1.8.8 LivingEntity.java:1230)
        assert_eq!(sink("1.8.8"), -0.02);
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

    fn gliding_player(y: f64) -> SimulationState {
        SimulationState {
            fall_flying: true,
            no_gravity: false,
            look: Vec3::new(0.0, 0.0, 1.0),
            // the elytra pose of Player.updatePlayerPose (1.9.4 Player.java:367-368, 1.13.2
            // :422-423)
            height: java_float(0.6),
            ..legacy_water_player(Vec3::new(0.5, y, 0.5), Vec3::new(0.1, -0.1, 0.0))
        }
    }

    #[test]
    fn a_gliding_player_travels_in_the_lava_its_turned_over_box_touches_before_1_14() {
        // Source-predicted; no live trace of this fixture exists. isTouchingLava contracts the box
        // by 0.4F each way, which takes the 0.6F high box past itself, and Box's constructor turns
        // it back (1.9.4 Entity.java:1031-1032, AABB.java:25-32, 112-120; 1.13.2 Entity.java:1143-
        // 1146, AABB.java:38-45, 228-230): 64.9 - 65.1 at y 64.7. Level.containsMaterial reads floor
        // to ceil, the cells 64 and 65 (1.13.2 Level.java:1362-1384). The lava branch of travel comes
        // before the elytra's: the velocity is halved and loses 0.02 (1.9.4 LivingEntity.java:1326-
        // 1327, 1439-1444; 1.13.2 :1517-1519, 1631-1636)
        for version in ["1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2"] {
            for cell in [(0, 64, 0), (0, 65, 0)] {
                let mut state = gliding_player(64.7);
                crate::versioned::dispatch(
                    version,
                    &mut state,
                    &lava_world(cell),
                    PhysicsConfig::default(),
                )
                .unwrap();

                assert!(state.in_lava, "{version} {cell:?}");
                assert_eq!(state.velocity.x, 0.1 * 0.5, "{version} {cell:?}");
                assert_eq!(state.velocity.y, -0.1 * 0.5 - 0.02, "{version} {cell:?}");
                assert_eq!(state.velocity.z, 0.0, "{version} {cell:?}");
            }
            // lava past the box leaves the elytra's travel, whose forward look turns the motion
            let mut state = gliding_player(64.7);
            crate::versioned::dispatch(
                version,
                &mut state,
                &lava_world((0, 66, 0)),
                PhysicsConfig::default(),
            )
            .unwrap();

            assert!(!state.in_lava, "{version}");
            assert!(state.velocity.z > 0.0, "{version}");
        }
    }

    #[test]
    fn the_modern_lava_flag_keeps_its_own_box_and_its_own_tick_from_1_14() {
        // Source-predicted control. From 1.14 isInLava is the flag 1.14.4 LiquidBlock.entityInside sets
        // in the move's block check, over the box contracted by 0.001 (1.14.4 Entity.java:1167-1173,
        // LiquidBlock.java:158-161): the first tick still glides, the next is in lava, and a box whose
        // legacy contraction misses the cell below reads it
        let world = lava_world((0, 65, 0));
        let mut state = gliding_player(64.7);
        crate::versioned::dispatch("1.14.4", &mut state, &world, PhysicsConfig::default()).unwrap();
        assert!(state.in_lava);
        assert!(state.velocity.z > 0.0);
        let (velocity_x, velocity_y) = (state.velocity.x, state.velocity.y);
        crate::versioned::dispatch("1.14.4", &mut state, &world, PhysicsConfig::default()).unwrap();
        assert_eq!(state.velocity.x, velocity_x * 0.5);
        assert_eq!(state.velocity.y, velocity_y * 0.5 - 0.08 / 4.0);

        let below = lava_world((0, 64, 0));
        let dipped = |version| {
            let mut state = SimulationState {
                velocity: Vec3::ZERO,
                ..gliding_player(64.95)
            };
            crate::versioned::dispatch(version, &mut state, &below, PhysicsConfig::default())
                .unwrap();
            state.in_lava
        };
        assert!(!dipped("1.13.2"));
        assert!(dipped("1.14.4"));
    }

    #[test]
    fn a_walk_past_a_partial_block_within_the_epsilon_keeps_its_move_and_flags() {
        // Side by side with a slab: 5e-8 into its width neither lists it nor blocks the box (26.3
        // BlockCollisions.java:91-103; VoxelShape.collide reads the box's edge 1e-7 in), 2e-7 into
        // it does both, on the release's own path: the block walk for this one-axis move to 1.17.1
        // (1.16.5 Entity.java:834, Shapes.java:235-296), the list from 1.18
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 1),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let walk = |reach: f64, version: &str| {
            let mut state = SimulationState {
                position: Vec3::new(reach - 0.3, 0.0, 0.65),
                velocity: Vec3::new(0.0, 0.0, 0.1),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            state
        };
        for version in ["1.14.4", "1.16.5", "1.17.1", "1.18.2", "1.21.11", "26.3"] {
            let past = walk(5.0e-8, version);
            assert_eq!(past.position.z, 0.65 + 0.1, "{version}");
            assert!(!past.collided_horizontally, "{version}");

            let into = walk(2.0e-7, version);
            assert_eq!(into.position.z, 0.65 + (1.0 - (0.65 + 0.3)), "{version}");
            assert!(into.collided_horizontally, "{version}");
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
        // Entity.updateFluidOnEyes takes getEyeY(), y + the eye height (1.21.11 Entity.java:1591),
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
    fn a_passenger_takes_no_item_use_slowdown_in_any_release() {
        // isUsingItem() && !hasVehicle() (1.8.8 LocalPlayer.java:538, 1.13.2 :698, 1.14.4 :596,
        // 1.20.4 :672, 1.21.4 :642; 1.21.5 LocalPlayer.java:610, 26.3 :695): the rider's travel from
        // rest, whose result stays in its motion, runs on the whole input
        let travelled = |release, using_item, passenger| {
            let mut state = SimulationState {
                position: Vec3::new(0.0, 2.0, 0.0),
                is_player: true,
                input: Vec3::new(1.0, 0.0, 1.0),
                using_item,
                passenger,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_release(&mut state, &WorldSnapshot::new(), release);
            (state.velocity.x, state.velocity.z)
        };
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let free = travelled(release, false, true);
            assert_ne!(free, (0.0, 0.0), "{release:?}");
            assert_eq!(travelled(release, true, true), free, "{release:?}");
            let (walked, used) = (travelled(release, false, false), travelled(release, true, false));
            assert!(used.0.abs() < walked.0.abs() && used.0 != 0.0, "{release:?}: {used:?} {walked:?}");
        }
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
    fn a_removed_player_still_runs_its_base_tick_from_1_19_1() {
        // D-215: LivingEntity.tick runs super.tick(), Entity.baseTick with its fluid refresh and the
        // fall halved in lava, and skips only aiStep for a removed entity (1.21.4 LivingEntity.java
        // tick, LocalPlayer.java:836-840): a removed player in lava keeps its place and motion and
        // halves its fall distance
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState { id: 1, shape: Some(BlockShape::full_block()), ..BlockState::default() },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        // (26.1 scans the fluids only with the chunks around loaded: EntityFluidInteraction)
        for (x, z) in [(-1, -1), (-1, 0), (0, -1)] {
            world.set_chunk_loaded(x, z, true);
        }
        for release in [MinecraftVersion::new(1, 19, 1), MinecraftVersion::new(1, 21, 4), MinecraftVersion::new(26, 3, 0)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                on_ground: true,
                is_player: true,
                controlled_by_local_instance: true,
                removed: true,
                fall_distance: 4.0,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.position, Vec3::new(0.5, 1.0, 0.5), "{release:?}");
            assert!(state.in_lava, "{release:?}");
            assert_eq!(state.fall_distance, 2.0, "{release:?}");
        }
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
    fn a_riptide_launch_needs_water_or_rain_and_release_1_13() {
        let launched = |in_water: bool, in_rain: bool, passenger: bool, release| {
            let mut state = SimulationState {
                in_water,
                in_rain,
                passenger,
                ..releasing_riptide(1, Vec3::new(0.5, 2.0, 0.5), false)
            };
            tick_release(&mut state, &pool(), release);
            state.position.x - 0.5
        };

        assert_eq!(launched(true, false, false, MinecraftVersion::new(1, 13, 0)), 1.5);
        assert_eq!(launched(false, true, false, MinecraftVersion::new(1, 13, 0)), 1.5);
        assert_eq!(launched(false, true, false, MinecraftVersion::new(26, 3, 0)), 1.5);
        assert_eq!(launched(false, false, false, MinecraftVersion::new(26, 2, 0)), 0.0);
        assert_eq!(launched(true, false, false, MinecraftVersion::new(1, 12, 2)), 0.0);
        assert_eq!(launched(false, true, false, MinecraftVersion::new(1, 12, 2)), 0.0);
        // 26.1 keeps a passenger from it (26.3 TridentItem.java:64), rain or not. Before, it launches,
        // and the push is the motion rideTick then zeroes: the seat stays (1.20.4 Entity.java:1850-1854,
        // 1.21.11 and 26.3 Entity.java:2465-2470); the launch shows in the lift off the ground
        let lift = |release| {
            let mut state = SimulationState {
                in_rain: true,
                passenger: true,
                ..releasing_riptide(1, Vec3::new(0.5, 1.0, 0.5), true)
            };
            tick_release(&mut state, &pool(), release);
            assert_eq!(state.position, Vec3::new(0.5, 1.0, 0.5));
            state.riptide_lift.y
        };
        assert_eq!(launched(false, true, true, MinecraftVersion::new(26, 1, 0)), 0.0);
        assert_eq!(lift(MinecraftVersion::new(26, 1, 0)), 0.0);
        assert_eq!(launched(false, true, true, MinecraftVersion::new(1, 21, 11)), 0.0);
        assert!((lift(MinecraftVersion::new(1, 21, 11)) - 1.1999999).abs() < 1.0e-6);
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
        // (1.20.4 Entity.java:716, 1.21 :738); from 1.21.2 it only queues its path, and the tick's
        // check starts from where the tick began (1.21.3 Entity.java:776-793, LivingEntity.java:2818)
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
    fn a_riptide_lift_s_own_path_is_swept_from_1_21_5() {
        // D-189: from 1.21.5 the lift is an Entity.move that records its Movement in
        // movementThisTick, which the tick's applyEffectsFromBlocks sweeps with its own (1.21.5
        // Entity.java:685, 707-721, 1119-1135; 26.3 :788, 946-957); 1.21.4 sweeps from the old
        // position the tick set after the lift (1.21.4 Entity.java:788-802). The sweep steps past
        // its start before it visits (1.21.5 BlockGetter.java:164-245), so a lift from the top of
        // soul sand (y 1.875) sweeps the cell at y 2, which the box after the lift (from y 3.075)
        // is not in: a cobweb there catches the player from 1.21.5 alone
        let mut world = pool();
        world.set_block(
            (0, 1, 0),
            BlockState {
                id: 31,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.875, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 2, 0),
            BlockState {
                id: 30,
                shape: Some(BlockShape {
                    effect: BlockEffect::Web,
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let caught = |release: &str| {
            let mut state = releasing_riptide(1, Vec3::new(0.5, 1.875, 0.5), true);
            crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.position.y > 3.0, "{release}: lifted to {}", state.position.y);
            state.stuck_speed_multiplier != Vec3::ZERO
        };
        assert!(!caught("1.21.4"));
        assert!(caught("1.21.5"));
        assert!(caught("26.3"));
    }

    #[test]
    fn a_riptide_lift_into_a_ceiling_over_slime_keeps_its_rise() {
        // D-211: the lift is an Entity.move, whose vertical collision runs the block under the
        // feet's updateEntityAfterFallOn, a ceiling's too: the slime block's bounceUp changes
        // nothing while the motion rises, where any other block zeroes it (1.13.2 Entity.java
        // move, SlimeBlock.java:52-64; 1.20.4 Entity.java:663, SlimeBlock.java:45-60; 26.1.2
        // the same). A lift under a ceiling 0.0125 above the head, looking up 45 degrees, over a
        // floor of slime blocks or of stone
        let world = |floor: BlockEffect| {
            let mut world = WorldSnapshot::new();
            for z in 0..=2 {
                world.set_block(
                    (0, 0, z),
                    BlockState {
                        shape: Some(BlockShape { effect: floor, ..BlockShape::full_block() }),
                        ..BlockState::default()
                    },
                );
            }
            world.set_block(
                (0, 2, 0),
                BlockState {
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::new(0.0, 0.8125, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            world
        };
        let rise = |floor: BlockEffect, release: &str| {
            let mut state = SimulationState {
                in_water: false,
                in_rain: true,
                pitch: f64::from(-45.0_f32 * 0.017453292_f32),
                yaw: 0.0,
                ..releasing_riptide(1, Vec3::new(0.5, 1.0, 0.5), true)
            };
            crate::versioned::dispatch(release, &mut state, &world(floor), PhysicsConfig::default())
                .unwrap();
            state.velocity.y
        };
        for release in ["1.13.2", "1.20.4", "26.1.2"] {
            assert!(rise(BlockEffect::Slime, release) > 0.5, "{release}: {}", rise(BlockEffect::Slime, release));
            assert!(rise(BlockEffect::None, release) <= 0.0, "{release}: {}", rise(BlockEffect::None, release));
        }
    }

    #[test]
    fn a_riptide_lift_puts_out_a_wet_burning_player_before_its_tick_to_1_21_1() {
        // D-228: the lift is an Entity.move, whose fire part runs to 1.21.1 (1.17 Entity.java:630-648,
        // 1.21.1 :738-760): a burning player in the rain is put out by it, so the server's baseTick
        // finds no fire to burn down and leaves the frozen ticks (1.17 Entity.java:421-436), which
        // aiStep then thaws by 2. From 1.21.2 the move applies nothing itself (1.21.2 Entity.java:776-810)
        let frozen = |release: &str| {
            let mut state = SimulationState {
                in_water: false,
                in_rain: true,
                client_side: false,
                remaining_fire_ticks: Some(40),
                ticks_frozen: 10,
                ..releasing_riptide(1, Vec3::new(0.5, 1.0, 0.5), true)
            };
            crate::versioned::dispatch(release, &mut state, &floor_with_step(1.0), PhysicsConfig::default())
                .unwrap();
            assert!(state.position.y > 2.1, "{release}: lifted");
            state.ticks_frozen
        };
        assert_eq!(frozen("1.17"), 8);
        assert_eq!(frozen("1.21.1"), 8);
        assert_eq!(frozen("1.21.2"), 0);
    }

    #[test]
    fn a_riptide_release_lifts_the_player_before_the_entities_tick() {
        // D-214: Minecraft.tick handles the use key, whose release launches and lifts the player,
        // before the level ticks its entities (1.13.2 Minecraft.java:1115, 1146; 1.16.5 :1420,
        // 1443; 26.3 :1786, 1797, 1930): a mob that ticks before the player and meets only the box
        // under the lift pushes nothing
        use crate::entity::{NearbyEntity, NearbyEntityKind};
        let mut world = WorldSnapshot::new();
        for x in -1..=4 {
            for z in -1..=1 {
                world.set_block(
                    (x, 0, z),
                    BlockState { id: 1, shape: Some(BlockShape::full_block()), ..BlockState::default() },
                );
            }
        }
        let bounds = Aabb::new(Vec3::new(0.5, 1.0, 0.2), Vec3::new(1.1, 1.5, 0.8));
        let mob = NearbyEntity {
            kind: NearbyEntityKind::Living,
            position: Vec3::new(0.8, 1.0, 0.5),
            bounding_box: bounds,
            tick_position: Vec3::new(0.8, 1.0, 0.5),
            tick_bounding_box: bounds,
            ticks_before_player: true,
            team_pushes: true,
            spectator: false,
        };
        for version in ["1.13.2", "1.16.5", "1.21.4", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut state = SimulationState {
                in_water: false,
                in_rain: true,
                controlled_by_local_instance: true,
                ..releasing_riptide(1, Vec3::new(0.5, 1.0, 0.5), true)
            };
            let outcome = crate::simulation::tick_local_player_scene(
                &mut world.clone(),
                &mut state,
                None,
                &[mob],
                (0, 0),
                false,
                &[],
                &mut [0.0; 3],
                PhysicsConfig::default(),
                release,
                |state, world| crate::versioned::release_riptide(version, state, world, PhysicsConfig::default()),
                |state, world| crate::versioned::dispatch(version, state, world, PhysicsConfig::default()),
            )
            .unwrap();
            assert!(state.position.y > 2.1, "{version}: lifted");
            assert!(outcome.entity_pushes.is_empty(), "{version}: {:?}", outcome.entity_pushes);
        }
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
        // 1.8.8 SnowLayerBlock.java:45-57 and 1.12.2 SnowLayerBlock.java:75-79: the
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
        // 1.8.8 Entity.java:489 (1.12.2 :621-626): the sneak back-off tests the lowered box with
        // Level.getCollisionBoxes, which before 1.13 holds the box of no height of one layer of
        // snow (1.12.2 SnowLayerBlock.java:75-79, Block.java:435-439): over air, the snow holds the
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
    fn before_1_14_the_cells_outside_the_border_collide_as_stone_whether_the_chunk_is_loaded_or_not() {
        // 1.8.8 Level.java:1045-1072 (1.12.2 Level.java:1108-1156, 1.13.2 LevelReader.java:112-156):
        // a cell outside the border (WorldBorder.isWithinBounds(BlockPos)) is stone for an entity
        // inside it, in every column blockExists / hasChunkAt takes, which on the client is every
        // column, loaded or not (1.8.8 ClientChunkCache.java:31-33; 1.12.2
        // MultiPlayerLevel.java:103-105; 1.13.2 MultiPlayerLevel.java:110-112)
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);
        let east = Vec3::new(0.5, 0.0, 0.0);
        for loaded in [false, true] {
            world.set_chunk_loaded(0, 0, loaded);
            for version in LEGACY_BORDER_VERSIONS {
                let walker =
                    walking_by_the_border(&world, version, Vec3::new(4.5, 0.0, 0.5), east, false);
                assert!(walker.collided_horizontally, "{version} loaded {loaded}");
                assert_eq!(walker.position.x, 4.5 + (5.0 - (4.5 + 0.3)), "{version} loaded {loaded}");
                assert!(!walker.outside_border, "{version} loaded {loaded}");
            }
        }
    }

    #[test]
    fn a_move_of_nothing_asks_nothing_of_the_border_in_1_13_2() {
        // 1.13.2 Entity.java:675 skips the query on an all-zero move, 1.12.2 Level.java:1133 does not
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 9.0);
        world.set_chunk_loaded(0, 0, true);
        let still = |version| walking_by_the_border(&world, version, Vec3::new(6.0, 0.0, 0.5), Vec3::ZERO, false);
        assert!(still("1.12.2").outside_border);
        assert!(!still("1.13.2").outside_border);
    }

    #[test]
    fn before_1_14_an_entity_flagged_outside_the_border_must_be_a_block_inside_to_meet_it() {
        // 1.8.8 Level.java:930-948 isInsideBorder: the border shrinks by 1 for an entity flagged
        // outside (Entity.isOutsideBorder) and grows by 1 for any other; Level.java:892-896 sets
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
        // ahead does. From 1.14 (1.14.4 Entity.java collide) the border is a shape only for a box
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

        // (the base is 0.1F widened: Player.createAttributes, D-194)
        let sprint_speed = java_float(f64::from(0.1_f32) * (1.0 + 0.30000001192092896));
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
    fn every_profile_routes_fluid_travel_through_its_release_handler() {
        use crate::simulation::FluidTravelHandler;
        // LivingEntity.travel moves its water and lava branches into travelInFluid in 1.21.2
        // (origin/client1.21.1 LivingEntity.java:2126-2177; origin/client1.21.2 :2202-2203,
        // 2241-2293): every profile before selects the inline handler, every one from it the split
        for version in crate::versioned::SUPPORTED_VERSIONS {
            let rules = crate::versioned::rules_for_version(version).expect("a profile");
            let expected = if rules.release.at_least(1, 21, 2) {
                FluidTravelHandler::TravelInFluid
            } else {
                FluidTravelHandler::InlineTravel
            };
            assert_eq!(FluidTravelHandler::for_rules(rules), expected, "{version}");
        }
    }

    #[test]
    fn fluid_travel_handlers_run_their_own_release_branches() {
        use crate::simulation::{FluidTravelAfterMove, FluidTravelHandler};
        // a block over the head keeps the 0.3 hop out of the fluid from firing
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block((0, 1, 0), BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        });
        let after_move = FluidTravelAfterMove {
            fluid: FluidState::Water,
            fluid_height: 0.5,
            fluid_jump_threshold: 0.4,
            water_slowdown: 0.8,
            gravity: 0.08,
            going_down: true,
            start_y: 0.0,
        };
        let run = |handler: FluidTravelHandler, rules: VersionPhysicsRules| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                velocity: Vec3::new(0.1, -0.1, 0.0),
                half_width: 0.3,
                height: 1.8,
                collided_horizontally: true,
                climbing: true,
                ..SimulationState::default()
            };
            handler.after_move(&mut state, &world, rules, &after_move);
            state.velocity
        };
        // in water against a ladder: 1.14+ (both handlers) climbs at 0.2 before the 0.8 drag and
        // the -gravity / 16 fall, while 1.13.2's inline travel has no climb in water (1.13.2
        // LivingEntity.java:1646-1684; 1.14.4 LivingEntity.java:1773-1775)
        let climbed = 0.2 * 0.800000011920929 - 0.08 / 16.0;
        let v1_21_2 = crate::versioned::rules_for_version("1.21.2").unwrap();
        let v1_21_1 = crate::versioned::rules_for_version("1.21.1").unwrap();
        let v1_13_2 = crate::versioned::rules_for_version("1.13.2").unwrap();
        assert_eq!(run(FluidTravelHandler::TravelInFluid, v1_21_2).y, climbed);
        assert_eq!(run(FluidTravelHandler::InlineTravel, v1_21_1).y, climbed);
        assert_eq!(
            run(FluidTravelHandler::InlineTravel, v1_13_2).y,
            -0.1 * 0.800000011920929 - 0.08 / 16.0
        );
        assert_eq!(run(FluidTravelHandler::TravelInFluid, v1_21_2).x, 0.1 * f64::from(0.8_f32));
        // lava: 0.5 drag over the jump threshold and -gravity / 4 in both forms
        let lava = FluidTravelAfterMove {
            fluid: FluidState::Lava,
            ..after_move
        };
        for (handler, rules) in [
            (FluidTravelHandler::InlineTravel, v1_21_1),
            (FluidTravelHandler::TravelInFluid, v1_21_2),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                velocity: Vec3::new(0.1, -0.1, 0.0),
                ..SimulationState::default()
            };
            handler.after_move(&mut state, &WorldSnapshot::new(), rules, &lava);
            assert_eq!(state.velocity, Vec3::new(0.05, -0.1 * 0.5 - 0.08 / 4.0, 0.0));
        }
    }

    /// One tick of a player standing in a water source, walking forward at movement speed 0.1.
    fn water_walk_velocity(version: &str, depth_strider: u8, water_efficiency: f64) -> f64 {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        world.set_block((0, -1, 0), BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        });
        world.set_block((0, 0, 0), BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            ..BlockState::default()
        });
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            on_ground: true,
            is_player: true,
            input: Vec3::new(0.0, 0.0, 1.0),
            movement_speed: Some(0.1),
            depth_strider,
            water_movement_efficiency: water_efficiency,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
            .expect("a supported release");
        state.velocity.z
    }

    #[test]
    fn depth_strider_and_water_efficiency_hold_independently_in_every_release() {
        // The water branch of travel, on the ground: before 1.21 the Depth Strider level (at most
        // 3) moves the 0.8F slow-down toward 0.54600006F and the 0.02F speed toward getSpeed() by
        // level / 3F (1.8.8 LivingEntity.java:1209-1228, 1.20.6 LivingEntity.java:2122-2133);
        // from 1.21 the WATER_MOVEMENT_EFFICIENCY attribute does so by its full value, and the
        // enchantment level alone does nothing (1.21 LivingEntity.java:2130-2140). The forward
        // input is 0.98F; moveRelative multiplies it in float through 1.13.2 (1.13.2
        // Entity.java:moveRelative) and in double from 1.14 (1.14.4 Entity.java:getInputVector).
        for version in crate::versioned::SUPPORTED_VERSIONS {
            let release = MinecraftVersion::parse(version).unwrap();
            let attribute = release.at_least(1, 21, 0);
            let expected = |efficiency: f32| {
                let (slowdown, speed) = if efficiency > 0.0 {
                    let divisor = if attribute { 1.0_f32 } else { 3.0_f32 };
                    (
                        0.8_f32 + (0.54600006_f32 - 0.8_f32) * efficiency / divisor,
                        0.02_f32 + (0.1_f32 - 0.02_f32) * efficiency / divisor,
                    )
                } else {
                    (0.8_f32, 0.02_f32)
                };
                let pushed = if release.at_least(1, 14, 0) {
                    f64::from(0.98_f32) * f64::from(speed)
                } else {
                    f64::from(0.98_f32 * speed)
                };
                pushed * f64::from(slowdown)
            };
            for level in 0..=4_u8 {
                let efficiency = if attribute { 0.0 } else { f32::from(level.min(3)) };
                assert_eq!(
                    water_walk_velocity(version, level, 0.0),
                    expected(efficiency),
                    "{version} depth strider {level}"
                );
            }
            for value in [0.0, 0.25, 0.5, 1.0] {
                let efficiency = if attribute { value as f32 } else { 0.0 };
                assert_eq!(
                    water_walk_velocity(version, 0, value),
                    expected(efficiency),
                    "{version} water efficiency {value}"
                );
            }
        }
    }

    /// One tick of an entity falling at 0.1 through a water column, with or without NoGravity.
    fn water_fall_velocity(version: &str, no_gravity: bool) -> f64 {
        let mut world = WorldSnapshot::new();
        load_chunks_around_origin(&mut world);
        for y in -2..=3 {
            world.set_block((0, y, 0), BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            });
        }
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            velocity: Vec3::new(0.0, -0.1, 0.0),
            half_width: 0.3,
            height: 1.8,
            no_gravity,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
            .expect("a supported release");
        state.velocity.y
    }

    #[test]
    fn fluid_falling_gravity_condition_holds_across_1_18_2_and_1_19() {
        // getFluidFallingAdjustedMovement tests `!isNoGravity() && !isSprinting()` on both sides
        // of 1.18.2 / 1.19 (origin/client1.18.1 LivingEntity.java:2151-2163, origin/client1.19
        // :2205-2217), so a NoGravity entity keeps the 0.8 drag alone and any other one also
        // loses gravity / 16; from 1.20.5 the test is on the gravity argument, which NoGravity
        // makes 0 (1.20.5 LivingEntity.java:2290), the same result for this fixture
        let dragged = -0.1 * 0.800000011920929;
        for version in ["1.18", "1.18.1", "1.18.2", "1.19", "1.19.1", "1.19.2", "1.20.4", "1.20.5"] {
            assert_eq!(water_fall_velocity(version, true), dragged, "{version} NoGravity");
            assert_eq!(water_fall_velocity(version, false), dragged - 0.08 / 16.0, "{version}");
        }
        assert_eq!(water_fall_velocity("1.18.2", true), water_fall_velocity("1.19", true));
        assert_eq!(water_fall_velocity("1.18.2", false), water_fall_velocity("1.19", false));
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

        // ceil((10.0F - 4.0F) * 1.0F * 1.0) (1.20.5 LivingEntity.java:1597-1599), the drop of
        // the landing tick not counted before 1.21.5
        assert_eq!(state.fall_damage, 6.0);
    }

    // D-186: a player standing on the ground with a negative safe fall distance takes no damage:
    // Block.fallOn runs only for a fall distance above 0 (26.3 Entity.java:1601-1604, 1.20.5 the
    // same), so ceil(0 - safe) never reaches the damage
    #[test]
    fn a_negative_safe_fall_distance_deals_nothing_without_a_fall() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        for version in ["1.20.5", "1.21.4", "1.21.5", "26.3"] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                fall_distance: 0.0,
                safe_fall_distance: -3.0,
                controlled_by_local_instance: true,

                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert!(state.on_ground, "{version}");
            assert_eq!(state.fall_damage, 0.0, "{version}");
        }
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
        // attribute (1.20.5 LivingEntity.java:1593-1599; 1.21 :1590-1596 the same)
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

        // 1.21.5 Entity.java:1321-1331: fd -= (float)dy first, then fallOn with it:
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

        // 1.21.4 Entity.java:1287-1297: the landing tick does not count its drop, so
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
        // (ClipContext.java:91), the reset comes before checkFallDamage adds this move's drop
        assert_eq!(falling_through(&world, "1.19", 3.0, 2.0), 2.0);
        assert_eq!(falling_through(&world, "1.18.2", 3.0, 2.0), 2.0);
        assert_eq!(falling_through(&world, "26.2", 3.0, 2.0), 2.0);
        // 1.18.1 has no clip (1.18.1 Entity.java move)
        assert_eq!(falling_through(&world, "1.18.1", 3.0, 2.0), 7.0);
    }

    #[test]
    fn water_the_move_passes_through_resets_the_fall_distance() {
        // the clip's ClipContext.Fluid.WATER (1.19 Entity.java:581-581)
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

        // 1.21.9 Entity.java:740-741: min(length, 8.0) along the move; the block is 9 down
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

        // 1.21.9 ClipContext.java:66-69: END_GATEWAY and END_PORTAL are full cubes for a player
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

        // Entity.baseTick: fallDistance *= 0.5F in lava (1.8.8 Entity.java:412, 1.16.5 :526,
        // 1.21.4 :514), *= 0.5 once a double (1.21.5 :518)
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

        // ceil(6.1F - 3.0F - 0.0F) (1.20.4 LivingEntity.java:1572)
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

        // ceil((6.1F - 3.0F) * 0.5F) (1.20.4 LivingEntity.java:1572)
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
    fn the_engine_never_starts_a_glide_and_keeps_one_it_is_given() {
        // the start is LocalPlayer.aiStep's and the end of one in the air the server's (26.3
        // LocalPlayer.java:833, LivingEntity.java:3119-3127): a jump in the air starts nothing,
        // and a flag the caller set glides with no equipment to read
        for release in [
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 21, 1),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let world = WorldSnapshot::new();
            let start = |fall_flying| SimulationState {
                position: Vec3::new(0.5, 20.0, 0.5),
                velocity: Vec3::new(0.0, -0.5, 0.5),
                is_player: true,
                jump_requested: true,
                fall_flying,
                ..SimulationState::default()
            };
            let mut falling = start(false);
            tick_release(&mut falling, &world, release);
            assert!(!falling.fall_flying, "{release:?}");
            let mut gliding = start(true);
            tick_release(&mut gliding, &world, release);
            assert!(gliding.fall_flying, "{release:?}");
            assert_ne!(gliding.velocity, falling.velocity, "{release:?}");
        }
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
        // Player.travel's flight resets fallDistance from 1.9 (1.9.4 Player.java:1389-1397);
        // 1.8 only scales the vertical motion by 0.6 (1.8.8 Player.java:1296-1302)
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
        // LivingEntity.java:1328-1329, 1.16.5 :2026-2027, 1.19.2 :2105-2107); from 1.19.3
        // checkSlowFallDistance only lowers a larger one (1.19.3 Entity.java:2169-2171)
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

        // Entity.baseTick halves it first (1.21.4 Entity.java:514)
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
        // air move (1.13.2 :1610-1612, 1.19.4 :2103-2105); 1.9 - 1.12 Levitation never resets it
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
        // LivingEntity.java:2578-2579, 1.21.5 :2736-2738)
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
        // LivingEntity.java:2103-2105); from 1.20 the reset before travel spares it (1.20 :2578)
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
    fn the_ticks_own_bubble_column_push_asks_can_simulate_movement_from_26_3() {
        // the tick's applyEffectsFromBlocks reaches the same handleOnInsideBubbleColumn (26.3
        // Entity.java:3005-3017; 26.2 Entity.java:2911-2922 has no test)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), bubble_column(BlockEffect::BubbleUp));
        world.set_block((0, 1, 0), bubble_column(BlockEffect::BubbleUp));
        let at = |exact| SimulationState {
            position: Vec3::new(0.5, 0.25, 0.5),
            height: 0.5,
            client_side: true,
            controlled_by_local_instance: true,
            can_simulate_movement: exact,
            ..SimulationState::default()
        };
        for (release, exact, pushed) in [
            (MinecraftVersion::new(26, 2, 0), Some(false), true),
            (MinecraftVersion::new(26, 3, 0), Some(false), false),
            (MinecraftVersion::new(26, 3, 0), Some(true), true),
        ] {
            assert_eq!(
                bubble_push(&world, at(exact), release),
                if pushed { 0.06 } else { 0.0 },
                "{release} {exact:?}"
            );
        }
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
    fn powder_snow_holds_a_falling_block_whose_feet_are_inside_it() {
        // (PowderSnowBlock.getCollisionShape: a FallingBlockEntity takes the full block wherever its
        // feet are, after the long fall's lower shape: 1.17 PowderSnowBlock.java:92-98, 26.3 :98-104)
        let world = powder_snow_world();
        let query = Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 2.0, 2.0));
        let release = crate::version::MinecraftVersion::new(1, 21, 1);
        let shapes = |context: EntityCollisionContext| world.collision_shapes_for_entity(query, &context, release);
        let inside = EntityCollisionContext {
            bottom: 0.5,
            walks_on_powder_snow: true,
            ..EntityCollisionContext::default()
        };
        assert!(shapes(inside).is_empty());
        let falling = EntityCollisionContext { falling_block: true, ..inside };
        assert_eq!(shapes(falling), vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))]);
        assert_eq!(shapes(EntityCollisionContext { descending: true, ..falling }), shapes(falling));
        let caught = shapes(EntityCollisionContext { fall_distance: 3.0, ..falling });
        assert_eq!(caught, vec![Aabb::new(Vec3::ZERO, Vec3::new(1.0, f64::from(0.9_f32), 1.0))]);
        // (the moving entity's body: no living entity, walking on powder snow)
        let body = SimulationState {
            position: Vec3::new(0.5, 0.5, 0.5),
            non_living: true,
            powder_snow_walkable: true,
            ..SimulationState::default()
        };
        assert!(crate::simulation::entity_collision_context(&body).falling_block);
        assert!(!crate::simulation::entity_collision_context(&SimulationState { non_living: false, ..body }).falling_block);
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

    /// A server's glide at `velocity` along its own direction, a stone wall at x = 2 a gap of 0.1
    /// from the box, unless `edit` moves the state; `around` adds to the world.
    fn wall_impact(
        velocity: Vec3,
        around: impl FnOnce(&mut WorldSnapshot),
        edit: impl FnOnce(&mut SimulationState),
        release: MinecraftVersion,
    ) -> SimulationState {
        let mut world = WorldSnapshot::new();
        for y in 8..=12 {
            for z in -2..=4 {
                world.set_block(
                    (2, y, z),
                    BlockState {
                        id: 1,
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
            }
        }
        around(&mut world);
        let speed = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
        let mut state = SimulationState {
            position: Vec3::new(1.6, 10.0, 0.5),
            half_width: 0.3,
            height: 0.6,
            velocity,
            is_player: true,
            fall_flying: true,
            no_gravity: true,
            controlled_by_local_instance: true,
            look: Vec3::new(velocity.x / speed, 0.0, velocity.z / speed),
            ..SimulationState::default()
        };
        edit(&mut state);
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
        state
    }

    fn head_on(speed: f64, release: MinecraftVersion) -> f64 {
        wall_impact(Vec3::new(speed, 0.0, 0.0), |_| {}, |_| {}, release).wall_damage
    }

    #[test]
    fn a_flight_into_a_wall_deals_ten_times_the_speed_lost_less_3() {
        // the speed is the motion's before the glide's 0.99, the damage a float:
        // (float)(lost * 10 - 3) (1.9.4 LivingEntity.java:1335, 1363-1371; 1.20.4 :2144; 1.21.3
        // :2295-2303; 26.3 :2504-2510, 2546-2553). 1.8.9's LivingEntity has no flyIntoWall.
        for version in [
            "1.9.4", "1.13.2", "1.16.5", "1.20.4", "1.21.1", "1.21.3", "1.21.5", "1.21.11", "26.2",
            "26.3",
        ] {
            let release = MinecraftVersion::parse(version).unwrap();
            assert_eq!(head_on(1.0, release), 7.0, "{version}");
            // 0.31 * 10 - 3 is 0.10000000000000009 in double
            assert_eq!(head_on(0.31, release), 0.10000000149011612, "{version}");
            // around the threshold, 0.3 * 10.0 being 3.0 exactly; a speed taken after the glide's
            // 0.99 would miss the last
            assert_eq!(head_on(0.29999999999999993, release), 0.0, "{version}");
            assert_eq!(head_on(0.3, release), 0.0, "{version}");
            assert_eq!(
                head_on(0.30000000000000004, release),
                f64::from(4.440892098500626e-16_f64 as f32),
                "{version}"
            );
        }
        assert_eq!(head_on(1.0, MinecraftVersion::new(1, 8, 9)), 0.0);
    }

    #[test]
    fn a_glancing_flight_into_a_wall_loses_only_the_speed_the_wall_took() {
        // equal speeds: the motion along z stays, the glide's 0.99 of it, and counts as kept
        // (26.3 LivingEntity.java:2505-2510, 2548)
        let release = MinecraftVersion::new(26, 3, 0);
        let glancing = wall_impact(Vec3::new(0.8, 0.0, 0.6), |_| {}, |_| {}, release);
        assert!(glancing.collided_horizontally);
        assert_eq!(glancing.velocity.x.abs(), 0.0);
        let kept = glancing.velocity.z;
        assert!((kept - 0.6 * f64::from(0.99_f32)).abs() < 1.0e-12);
        let lost = (0.8_f64 * 0.8 + 0.6 * 0.6).sqrt() - kept;
        assert_eq!(glancing.wall_damage, f64::from((lost * 10.0 - 3.0) as f32));
        assert!(glancing.wall_damage > 0.0 && glancing.wall_damage < 7.0);

        // nearer along the wall the same speed loses too little
        let shallow = wall_impact(Vec3::new(0.6, 0.0, 0.8), |_| {}, |_| {}, release);
        assert!(shallow.collided_horizontally);
        assert_eq!(shallow.wall_damage, 0.0);
    }

    #[test]
    fn the_speed_a_flight_keeps_is_the_one_entity_move_left() {
        // Entity.move ends by the block speed factor before travelFallFlying reads the speed
        // (26.3 Entity.java:850-851, LivingEntity.java:2505-2510): a slow block's mob loses more;
        // a gliding player keeps a factor of 1 (block_speed_factor)
        let release = MinecraftVersion::new(26, 3, 0);
        let soul_sand = |world: &mut WorldSnapshot| {
            world.set_block(
                (1, 10, 1),
                BlockState {
                    id: 2,
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                        effect: BlockEffect::SoulSand,
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
        };
        let mob = |state: &mut SimulationState| state.is_player = false;
        let flight = Vec3::new(0.8, 0.0, 0.6);
        let slowed = wall_impact(flight, soul_sand, mob, release);
        let unslowed = wall_impact(flight, |_| {}, mob, release);
        assert!((slowed.velocity.z - unslowed.velocity.z * f64::from(0.4_f32)).abs() < 1.0e-12);
        let lost = (0.8_f64 * 0.8 + 0.6 * 0.6).sqrt() - slowed.velocity.z;
        assert_eq!(slowed.wall_damage, f64::from((lost * 10.0 - 3.0) as f32));
        assert!(slowed.wall_damage > unslowed.wall_damage);
        let player = wall_impact(flight, soul_sand, |_| {}, release);
        assert_eq!(player.wall_damage, unslowed.wall_damage);
    }

    #[test]
    fn a_wall_impact_is_only_the_servers_and_only_a_flights() {
        // the client's glide skips it, for the same collision and motion (26.3
        // LivingEntity.java:2508, 1.9.4 :1363)
        for version in ["1.9.4", "1.20.4", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let client = wall_impact(
                Vec3::new(1.0, 0.0, 0.0),
                |_| {},
                |state| state.client_side = true,
                release,
            );
            let server = wall_impact(Vec3::new(1.0, 0.0, 0.0), |_| {}, |_| {}, release);
            assert_eq!(client.wall_damage, 0.0, "{version}");
            assert_eq!(server.wall_damage, 7.0, "{version}");
            assert!(client.collided_horizontally, "{version}");
            assert_eq!(client.position, server.position, "{version}");
            assert_eq!(client.velocity, server.velocity, "{version}");
        }
        let release = MinecraftVersion::new(26, 3, 0);
        // free flight: the speed drops by the glide's 0.99 and nothing is hit
        let free = wall_impact(
            Vec3::new(1.0, 0.0, 0.0),
            |_| {},
            |state| state.position = Vec3::new(-20.0, 10.0, 0.5),
            release,
        );
        assert!(!free.collided_horizontally);
        assert_eq!(free.wall_damage, 0.0);
        // a landing straight down collides below alone
        let landing = wall_impact(
            Vec3::new(0.0, -1.5, 0.0),
            |world| {
                world.set_block(
                    (1, 9, 0),
                    BlockState {
                        id: 1,
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
            },
            |state| {
                state.position = Vec3::new(1.5, 10.1, 0.5);
                state.look = Vec3::new(1.0, 0.0, 0.0);
            },
            release,
        );
        assert!(landing.collided_vertically && !landing.collided_horizontally);
        assert_eq!(landing.wall_damage, 0.0);
        // no glide, no impact: without the flag the same hit is an ordinary one
        let walking = wall_impact(
            Vec3::new(1.0, 0.0, 0.0),
            |_| {},
            |state| state.fall_flying = false,
            release,
        );
        assert!(walking.collided_horizontally);
        assert_eq!(walking.wall_damage, 0.0);
        // nor does the next tick repeat it, and the fall's damage stays apart
        let mut state = wall_impact(Vec3::new(1.0, 0.0, 0.0), |_| {}, |_| {}, release);
        assert_eq!(state.fall_damage, 0.0);
        let rules = VersionPhysicsRules {
            release,
            ..VersionPhysicsRules::default()
        };
        tick_with_rules(&mut state, &WorldSnapshot::new(), PhysicsConfig::default(), rules);
        assert_eq!(state.wall_damage, 0.0);
    }

    #[test]
    fn a_glide_stopped_on_a_climbable_block_hits_no_wall_from_1_21_5() {
        // 1.21.5 travelFallFlying leaves a climbable block's travel to travelInAir (1.21.5
        // LivingEntity.java:2273-2276); before, the glide goes on (1.21.3 :2295-2303)
        let climbing = |release| {
            wall_impact(
                Vec3::new(1.0, 0.0, 0.0),
                |_| {},
                |state| state.climbing = true,
                release,
            )
        };
        assert_eq!(climbing(MinecraftVersion::new(1, 21, 4)).wall_damage, 7.0);
        assert_eq!(climbing(MinecraftVersion::new(1, 21, 5)).wall_damage, 0.0);
    }

    #[test]
    fn contact_damage_is_the_servers_alone() {
        // hurt is the server's in every family (1.8.8 LocalPlayer.java:88-90, LivingEntity.java:673-675;
        // 1.20.4 LocalPlayer.java:149-151, LivingEntity.java:1096-1098; 1.21.3 MagmaBlock.java:30; 26.3
        // Entity.java:1969-1975); the block's other effects stay on the client's side
        let effect_world = |effect, position| {
            let mut world = WorldSnapshot::new();
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
            world
        };
        let dealt = |world: &WorldSnapshot, client_side, release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                // gravity's step of a standing entity
                velocity: Vec3::new(0.0, -0.0784, 0.0),
                on_ground: true,
                // the local player on a client, another entity on a server: both tick their moves
                is_player: client_side,
                client_side,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            tick_release(&mut state, world, release);
            state.contact_damage
        };
        let magma = effect_world(BlockEffect::Magma, (0, 0, 0));
        let fire = passable_effect_world((0, 1, 0), BlockEffect::SoulFire);
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 3),
            MinecraftVersion::new(26, 3, 0),
        ] {
            // magma is not a block of 1.8 (no MagmaBlock in the 1.8.9 or 1.9.4 sources)
            if release.at_least(1, 10, 0) {
                assert_eq!(dealt(&magma, false, release), 1.0, "{release:?}");
                assert_eq!(dealt(&magma, true, release), 0.0, "{release:?}");
            }
            assert_eq!(dealt(&fire, false, release), 2.0, "{release:?}");
            assert_eq!(dealt(&fire, true, release), 0.0, "{release:?}");
        }
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
        // before 1.21 (1.20.4 MagmaBlock.java:31, 1.21.11 MagmaBlock.java:29); from 1.21 Frost
        // Walker's damage immunity to #burn_from_stepping (1.21.11 Enchantments.java:139-139)
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
    fn fire_resistance_refuses_what_burns_and_not_a_cactus() {
        // LivingEntity.hurtServer refuses a #is_fire damage under the effect (1.20.4
        // LivingEntity.java:1108, 26.3 :1154), hot_floor and campfire among them (1.21.11
        // DamageTypeTagsProvider.java:23-24; hotFloor.setFire, 1.10.2 DamageSource.java:20)
        let release = MinecraftVersion::new(26, 3, 0);
        for (effect, unprotected) in [
            (BlockEffect::Campfire, 1.0),
            (BlockEffect::SoulCampfire, 2.0),
            (BlockEffect::Fire, 1.0),
            (BlockEffect::SoulFire, 2.0),
            (BlockEffect::Cactus, 1.0),
        ] {
            let world = passable_effect_world((0, 1, 0), effect);
            let touching = |fire_resistance| {
                let mut state = SimulationState {
                    position: Vec3::new(0.5, 1.5, 0.5),
                    height: 1.8,
                    no_gravity: true,
                    fire_resistance,
                    ..SimulationState::default()
                };
                tick_release(&mut state, &world, release);
                state.contact_damage
            };
            assert_eq!(touching(false), unprotected, "{effect:?}");
            let expected = if effect == BlockEffect::Cactus { unprotected } else { 0.0 };
            assert_eq!(touching(true), expected, "{effect:?}");
        }
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
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            on_ground: true,
            fire_resistance: true,
            ..SimulationState::default()
        };
        tick_release(&mut state, &world, release);
        assert_eq!(state.contact_damage, 0.0);
    }

    #[test]
    fn lit_campfires_spare_frost_walker() {
        // CampfireBlock.entityInside skips a wearer of Frost Walker before 1.21 (1.20.5
        // CampfireBlock.java:102), the enchantment's immunity to the campfire damage type from 1.21
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
    fn a_campfire_reaches_the_whole_of_its_block_on_26_3() {
        // CampfireBlock has no getEntityInsideCollisionShape (26.3 CampfireBlock.java), so the default
        // Shapes.block() holds (BlockBehaviour.java:298-300) and Entity.java:1386-1387 counts any box in
        // its cell, over the 7/16 of its shape (:270) too; a box out of the cell is not inside
        let world = passable_effect_world((0, 1, 0), BlockEffect::Campfire);
        let dealt = |y: f64| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, y, 0.5),
                height: 1.8,
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, MinecraftVersion::new(26, 3, 0));
            state.contact_damage
        };
        // (7/16 up the cell, at the top of the shape, and over it)
        assert_eq!(dealt(1.0 + 7.0 / 16.0), 1.0);
        assert_eq!(dealt(1.9), 1.0);
        assert_eq!(dealt(2.1), 0.0);
    }

    #[test]
    fn inside_block_signals_come_from_the_box_after_the_move() {
        // Entity.move calls checkInsideBlocks after the move (1.19.2 Entity.java:688), and from
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
        // Entity.java:891-894, 1.19.3 :904-907) and 1.0E-5F from 1.21.2 (1.21.5 Entity.java:1128)
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
        // Entity.lavaIgnite's igniteForTicks from 1.21.5 (1.21.5 Entity.java:571-604) and
        // InsideBlockEffectType.CLEAR_FREEZE from 1.21.9 (1.21.11 LavaFluid.java:108). Before
        // 1.21.5 Entity.baseTick clears them only for an entity already burning at the start of
        // the tick (1.21.4 Entity.java:486-506), which needs the fire ticks the engine does not keep
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
        // (1.21.5 Entity.java:1167-1176), AABB.collidedAlongVector holding either centre or the
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
        // InsideBlockEffectType.java:7-13, InsideBlockEffectApplier.java advanceStep): a 2.5 drop's
        // walk reaches the cells 3 - 5 at step 1 and cell 2 at step 2 (1.21.5
        // BlockGetter.java:164-184, 220-240), two steps; the column's empty collision shape lets
        // the fall through (fallDistance 0 before the move, 1.21.5 PowderSnowBlock.java:96-106)
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
        // PowderSnowBlock.java:92-106, 26.3 :89-104), and Entity.checkInsideBlocks gives
        // entityInside only when the move collides along with it (1.21.11 Entity.java:1266-1267):
        // a box falling from 1.2 to 0.95 keeps its centre above the shape grown by 0.9 - 1e-7.
        // 1.21.4 has only the full inside shape (1.21.4 Entity.java:1096-1124)
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
        // (1.16 LivingEntity.java:650-652, Entity.java:683-698), the block below that affects the
        // movement from 1.16.2 (1.16.5 LivingEntity.java:661-663)
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
        // Entity.java:690-693), only for a locally controlled entity from 1.21.2 (1.21.3
        // :721-726, 1.21.11 :804-809); 26.2's restitution takes a vertical move's (26.2
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
        // Entity.java:804-809)
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

        // nor does it restitute the motion then: `canSimulateMovement() && (movedVertically &&
        // verticalCollision || horizontalCollision)` (26.2 Entity.java:793, 815; 26.3 :818,
        // 837), so a bouncy entity's step up is not bounced off its 0 vertical motion (whose
        // portionWithMovement would divide by it)
        let vertical_motion_after_step = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.65, 1.0, 0.5),
                velocity: Vec3::new(0.2, 0.0, 0.0),
                entity_bounciness: 0.5,
                half_width: 0.3,
                height: 1.8,
                no_gravity: true,
                on_ground: true,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(release, &mut state, &steps, PhysicsConfig::default())
                .expect("the release is supported");
            assert_eq!(state.position.y, 1.5, "{release}");
            state.velocity.y
        };
        for release in ["1.21.11", "26.1", "26.2", "26.3"] {
            assert_eq!(vertical_motion_after_step(release), 0.0, "{release}");
        }
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

    #[test]
    fn the_knockbacks_length_is_a_float_root_to_1_16_5() {
        let (x, z) = (0.1, 0.2);
        let root = (x * x + z * z as f64).sqrt();
        let push = |release| {
            crate::simulation::living_knockback(Vec3::ZERO, false, f64::from(0.4_f32), x, z, 0.0, 1.0, release).unwrap().x
        };
        let float_root = f64::from(root as f32);
        let power = f64::from(0.4_f32);
        assert_eq!(push(MinecraftVersion::new(1, 16, 5)), -(x / float_root * power));
        assert_eq!(push(MinecraftVersion::new(1, 17, 0)), -(x / root * power));
    }

    #[test]
    fn rain_stops_the_rise_of_the_fire_ticks_and_puts_a_fire_out_before_1_16() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), BlockState { fluid: FluidState::Lava, ..BlockState::default() });
        let ticks_after = |in_rain, fire_immune| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                in_rain,
                fire_immune,
                remaining_fire_ticks: Some(5),
                ..SimulationState::default()
            };
            crate::simulation::legacy_inside_fire(&mut state, &world, &[], -20, MinecraftVersion::new(1, 15, 2));
            state.remaining_fire_ticks
        };
        assert_eq!(ticks_after(false, false), Some(6));
        assert_eq!(ticks_after(true, false), Some(-20));
        assert_eq!(ticks_after(false, true), Some(6), "1.15.2 Entity.java:636-641 does not ask");
    }

    #[test]
    fn inside_wall_moves_the_shape_to_the_eye_to_1_18_1_and_to_its_cell_from_1_18_2() {
        // a thin floor of the cell: at the eye, 1.53 up, it stands around the eye box; in its cell it is
        // below it (1.18.1 Entity.java:1707, 1.18.2 :1640)
        assert!(in_wall_at(MinecraftVersion::new(1, 18, 1), Some(true), 0.0, 0.1));
        assert!(!in_wall_at(MinecraftVersion::new(1, 18, 2), Some(true), 0.0, 0.1));
        assert!(!in_wall_at(MinecraftVersion::new(1, 19, 0), Some(true), 0.0, 0.1));
    }

    #[test]
    fn a_move_with_no_y_lands_from_26_2_only_if_it_also_hit_something_sideways() {
        // restituteMovementAfterCollisions: movedVertically && verticalCollision || horizontalCollision,
        // and the vertical part follows the collision (26.2 Entity.java:815, 846)
        let release = MinecraftVersion::new(26, 2, 0);
        let state = SimulationState { controlled_by_local_instance: true, ..SimulationState::default() };
        assert!(!crate::simulation::block_fall_callback_runs(&state, true, false, 0.0, release));
        assert!(crate::simulation::block_fall_callback_runs(&state, true, true, 0.0, release));
        assert!(crate::simulation::block_fall_callback_runs(&state, true, false, -0.1, release));
        assert!(!crate::simulation::block_fall_callback_runs(&state, false, true, 0.0, release));
    }

    #[test]
    fn the_sneak_is_a_float_product_from_1_18_2() {
        let input = |release| {
            let state = SimulationState {
                sneaking: true,
                is_player: true,
                input: Vec3::new(f64::from(0.7_f32), 0.0, 0.0),
                ..SimulationState::default()
            };
            crate::simulation::input_move_vector(&state, release)[0]
        };
        assert_eq!(input(MinecraftVersion::new(1, 18, 1)), (f64::from(0.7_f32) * 0.3) as f32);
        assert_eq!(input(MinecraftVersion::new(1, 18, 2)), 0.7_f32 * 0.3_f32);
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
        // client1.21.11 Entity.java:2200 and 1.8.8 Entity.java:1299 test isSuffocating
        // (isVisuallyOpaque): glass, a full collision cube, is no wall.
        for release in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 21, 11)] {
            assert!(in_wall_at(release, None, 0.0, 1.0));
            assert!(!in_wall_at(release, Some(false), 0.0, 1.0));
        }
    }

    #[test]
    fn inside_wall_before_1_13_tests_its_own_predicate() {
        // 1.8.8 Entity.java:1299 (Block.isLeafBlock: blocksMovement && renderAsNormalBlock,
        // Block.java:367-369) and 1.12.2 Entity.java:1609 (causesSuffocation, Block.java:365-367)
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
        // 1.16.5 Entity.java:1640 and 1.17 :1658 call getBlockCollisions with the entity, whose
        // BlockCollisionSpliterator offers the border's shape first (1.16 :47-48, 103-130; 1.17
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
            // (a hair past the edge: the join's 1e-7 merge leaves out a box flush with it, as a
            // recorded 1.17.1 player against the world border shows)
            let mut state = SimulationState {
                position: Vec3::new(4.75 + 5.0e-8, 0.0, 0.5),
                half_width: 0.25,
                height: 1.8,
                eye_height: Some(1.62),
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.inside_wall, touching, "{release:?}");
            let mut flush = SimulationState {
                position: Vec3::new(4.75, 0.0, 0.5),
                half_width: 0.25,
                height: 1.8,
                eye_height: Some(1.62),
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut flush, &world, release);
            assert!(!flush.inside_wall, "{release:?}");
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
        // enough (1.15 Entity.java:1572-1580); 1.16's box 0.1 tall reaches down to it
        // (1.16.5 Entity.java:1637-1640); 1.17's 1e-6 box does not (1.17 Entity.java:1656-1658).
        assert!(in_wall_at(MinecraftVersion::new(1, 15, 2), Some(true), 0.0, 0.5));
        assert!(in_wall_at(MinecraftVersion::new(1, 16, 5), Some(true), 0.0, 0.5));
        assert!(!in_wall_at(MinecraftVersion::new(1, 17, 1), Some(true), 0.0, 0.5));
        // A suffocating top slab around the eye: 1.18 moves the shape to the eye in place of its
        // cell, so it starts 0.5 above the eye (client1.18.1 Entity.java:1705-1708); 1.18.2 moves it
        // to its cell (1.18.2 Entity.java:1640, 1.19 Entity.java:1683-1692).
        assert!(!in_wall_at(MinecraftVersion::new(1, 18, 1), Some(true), 0.5, 1.0));
        assert!(in_wall_at(MinecraftVersion::new(1, 19, 0), Some(true), 0.5, 1.0));
        assert!(in_wall_at(MinecraftVersion::new(1, 18, 2), Some(true), 0.5, 1.0));
        assert!(in_wall_at(MinecraftVersion::new(1, 18, 1), Some(true), 0.0, 1.0));
    }

    #[test]
    fn inside_wall_joins_the_collision_shape_with_the_eye_box() {
        // From 1.19 Entity.isInWall joins each suffocating shape with the eye box through
        // Shapes.joinIsNotEmpty(shape.move(pos), Shapes.create(eyeBox), AND) (1.19
        // Entity.java:1683-1692, client1.21.11 Entity.java:2197-2203), whose index merger folds
        // coordinates within 1e-7 together (Shapes.java:179-193): an overlap of 5e-8 is none. In
        // 1.16 - 1.17.1 getBlockCollisions tests the Shapes.block() object with the strict
        // AABB.intersects (1.17 Entity.java:1658, 1.16.5 CollisionSpliterator.java:74-82), so the
        // same sliver of a full cube counts there.
        let in_wall = |release: MinecraftVersion, min_y: f64, overlap: f64| {
            let mut world = WorldSnapshot::new();
            world.set_block(
                (0, 1, 0),
                BlockState {
                    shape: Some(BlockShape {
                        bounds: Aabb::new(Vec3::new(0.0, min_y, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                        ..BlockShape::full_block()
                    }),
                    suffocating: Some(true),
                    ..BlockState::default()
                },
            );
            // the eye box is 1e-6 tall around the eye: its top reaches `overlap` into the block
            let eye = 1.0 + min_y + overlap - 5.0e-7;
            let mut state = SimulationState {
                position: Vec3::new(0.5, eye - f64::from(1.53_f32), 0.5),
                half_width: 0.3,
                height: 1.8,
                eye_height: Some(1.53),
                no_gravity: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.inside_wall
        };
        let modern = MinecraftVersion::new(1, 21, 11);
        assert!(in_wall(modern, 0.5, 5.0e-7));
        assert!(!in_wall(modern, 0.5, 5.0e-8));
        assert!(in_wall(modern, 0.0, 5.0e-7));
        assert!(!in_wall(modern, 0.0, 5.0e-8));
        let spliterator = MinecraftVersion::new(1, 17, 1);
        assert!(in_wall(spliterator, 0.0, 5.0e-8));
        assert!(!in_wall(spliterator, 0.5, 5.0e-8));
    }

    #[test]
    fn mth_floor_of_a_position_past_the_int_range_wraps_until_26_1() {
        // 1.19.4 Mth.floor: the cast then one down wraps to the top (1.19.4 Mth.java);
        // 26.3 Mth.floor saturates (26.3 Mth.java:69-71)
        let state = SimulationState {
            position: Vec3::new(-3.0e9, 64.0, 0.5),
            ..SimulationState::default()
        };
        let world = WorldSnapshot::new();
        let at = |release| crate::simulation::block_below_affecting_movement(&world, &state, None, release).0;
        assert_eq!(at(MinecraftVersion::new(1, 19, 4)), i32::MAX);
        assert_eq!(at(MinecraftVersion::new(26, 3, 0)), i32::MIN);
    }

    #[test]
    fn a_fall_past_the_int_range_deals_what_mth_ceil_leaves() {
        // computeFallDamage is an int: Mth.ceil wraps a huge fall to a negative, no damage, in 1.21.4
        // (1.21.4 LivingEntity.java:1621-1627); Mth.floor stays at the top from 1.21.5 (1.21.5
        // :1643-1653), the same value 26.3's cast gives
        use crate::simulation::{attribute_landing_damage, LandingDamageRounding};
        let damage = |rounding, release| attribute_landing_damage(3.0e9, 3.0, 1.0, 1.0, rounding, release);
        let (ceil, floor) = (LandingDamageRounding::Ceil, LandingDamageRounding::Floor);
        assert_eq!(damage(ceil, MinecraftVersion::new(1, 21, 4)), 0.0);
        assert_eq!(damage(floor, MinecraftVersion::new(1, 21, 11)), f64::from(i32::MAX));
        assert_eq!(damage(floor, MinecraftVersion::new(26, 3, 0)), f64::from(i32::MAX));
        assert_eq!(attribute_landing_damage(10.0, 3.0, 1.0, 1.0, ceil, MinecraftVersion::new(1, 21, 4)), 7.0);
    }

    #[test]
    fn the_same_block_is_a_wall_by_eye_height_width_and_flags() {
        // Entity.isInWall reads the eye and 0.8 of the width, each release in its form (1.15
        // Entity.java:1572-1580, 1.16.5 :1637-1640, 1.17 :1656-1658, 1.19 :1683-1692, 26.3 :2325-2335)
        // and Entity.noPhysics stops it: a crouch's eye below the block, a scale's narrower box beside
        // it, a spectator and a sleeper are no wall where a plain player is. Negative cells round down.
        let mut world = WorldSnapshot::new();
        world.set_block(
            (-1, 1, -1),
            BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() },
        );
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 15, 2),
            MinecraftVersion::new(1, 16, 5),
            MinecraftVersion::new(1, 17, 1),
            MinecraftVersion::new(1, 18, 2),
            MinecraftVersion::new(1, 19, 0),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let in_wall = |x: f64, half_width: f64, eye: f64, no_physics: bool, sleeping: bool| {
                let mut state = SimulationState {
                    position: Vec3::new(x, 0.0, -0.5),
                    half_width,
                    height: 1.8,
                    eye_height: Some(eye),
                    no_gravity: true,
                    no_physics,
                    sleeping,
                    ..SimulationState::default()
                };
                tick_release(&mut state, &world, release);
                state.inside_wall
            };
            assert!(in_wall(-0.5, 0.3, 1.5, false, false), "{release:?}");
            assert!(!in_wall(-0.5, 0.3, 0.9, false, false), "crouched {release:?}");
            assert!(!in_wall(-0.5, 0.3, 1.5, true, false), "spectator {release:?}");
            assert!(!in_wall(-0.5, 0.3, 1.5, false, true), "sleeping {release:?}");
            // beside the block: only the width reaches over x=0
            assert!(in_wall(0.2, 0.3, 1.5, false, false), "wide {release:?}");
            assert!(!in_wall(0.2, 0.15, 1.5, false, false), "scaled {release:?}");
        }
    }

    #[test]
    fn a_sleeper_in_a_wall_is_no_wall_and_still_pushed_out() {
        // LivingEntity.isInWall answers false for a sleeper (26.3 LivingEntity.java:3823-3825), and
        // LocalPlayer.aiStep's moveTowardsClosestSpace asks only noPhysics and from 26.3 isPassenger
        // (1.20.4 LocalPlayer.java:685, 26.3 LocalPlayer.java:774): the phases part
        let world = wall_along_z(None);
        for version in ["1.21.1", "26.3"] {
            let mut state = SimulationState {
                position: Vec3::new(0.2, 0.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                sleeping: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
            assert!(!state.inside_wall, "{version}");
            assert_eq!(state.position.x, 0.2 + -0.1, "{version}");
            // a passenger is spared the push only from 26.3
            let mut riding = SimulationState {
                position: Vec3::new(0.2, 0.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                passenger: true,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(version, &mut riding, &world, PhysicsConfig::default()).unwrap();
            assert_eq!(riding.position.x, 0.2, "the seat holds it: {version}");
            assert_eq!(riding.velocity.x == 0.0, version == "26.3", "{version}");
        }
    }

    #[test]
    fn a_spectator_leaves_the_ground_and_its_block_on_the_tick_it_turns() {
        // Player.tick: noPhysics = isSpectator(), then setOnGround(false) (26.3 Player.java:218-220,
        // 1.20.4 Player.java), which empties the supporting block (1.21.11 Entity.java:680-684)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState { shape: Some(BlockShape::full_block()), ..BlockState::default() },
        );
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            velocity: Vec3::new(0.0, -0.1, 0.0),
            is_player: true,
            on_ground: true,
            ..SimulationState::default()
        };
        let release = MinecraftVersion::new(26, 3, 0);
        tick_release(&mut state, &world, release);
        assert!(state.on_ground);
        assert_eq!(state.supporting_block, Some((0, 0, 0)));
        state.no_physics = true;
        tick_release(&mut state, &world, release);
        assert!(!state.on_ground);
        assert_eq!(state.supporting_block, None);
        // and back on the ground the first move finds it again
        state.no_physics = false;
        state.position = Vec3::new(0.5, 1.0, 0.5);
        state.velocity = Vec3::new(0.0, -0.1, 0.0);
        tick_release(&mut state, &world, release);
        assert!(state.on_ground);
        assert_eq!(state.supporting_block, Some((0, 0, 0)));
    }

    #[test]
    fn a_sleeping_entity_is_never_inside_a_wall() {
        // client1.21.11 LivingEntity.java:3654-3656, 1.8.8 Player.java:1104-1106.
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
    fn non_finite_entity_motion_is_taken_before_1_21_11() {
        // 1.21.10 Entity.setDeltaMovement has no finite guard (1.21.10 Entity.java:3709-3711;
        // 1.21.11 :3727-3732 refuses it)
        let mut set = SimulationState {
            velocity: Vec3::new(0.25, 0.0, -0.5),
            ..SimulationState::default()
        };
        set.set_delta_movement(Vec3::new(f64::NAN, 0.0, 0.0), MinecraftVersion::new(1, 21, 11));
        assert_eq!(set.velocity, Vec3::new(0.25, 0.0, -0.5));
        set.set_delta_movement(Vec3::new(f64::NAN, 0.0, 0.0), MinecraftVersion::new(1, 21, 10));
        assert!(set.velocity.x.is_nan());
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
        // 1.21.11 Entity.addDeltaMovement: a non-finite impulse is ignored (Entity.java:3734-3739)
        let guarded = MinecraftVersion::new(1, 21, 11);
        state.add_delta_movement(Vec3::new(f64::INFINITY, 0.0, 0.0), guarded);
        assert_eq!(state.velocity, Vec3::new(0.25, 0.0, -0.5));
        // a finite impulse whose sum is not finite is refused by setDeltaMovement (:3664-3668)
        state.add_delta_movement(Vec3::new(f64::MAX, 0.0, 0.0), guarded);
        state.add_delta_movement(Vec3::new(f64::MAX, 0.0, 0.0), guarded);
        assert!(state.velocity.is_finite());
        state.velocity = Vec3::new(0.25, 0.0, -0.5);
        state.add_delta_movement(Vec3::new(0.1, 0.0, 0.2), guarded);
        assert_eq!(state.velocity, Vec3::new(0.35, 0.0, -0.3));
        // 1.21.10 adds anything (Entity.java:3713-3715)
        state.add_delta_movement(Vec3::new(f64::INFINITY, 0.0, 0.0), MinecraftVersion::new(1, 21, 10));
        assert_eq!(state.velocity.x, f64::INFINITY);
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

    fn world_with_bush(effect: BlockEffect) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
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

    fn pricked(effect: BlockEffect, velocity: Vec3, state: SimulationState) -> f64 {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            velocity,
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..state
        };
        tick(&mut state, &world_with_bush(effect), PhysicsConfig::default());
        state.contact_damage
    }

    #[test]
    fn a_grown_bush_pricks_what_moved_0_003f_along_an_axis() {
        // (26.3 SweetBerryBushBlock.java:80-88)
        let base = SimulationState::default();
        let step = f64::from(0.003_f32);
        assert_eq!(pricked(BlockEffect::SweetBerryBush, Vec3::new(step, 0.0, 0.0), base), 1.0);
        assert_eq!(pricked(BlockEffect::SweetBerryBush, Vec3::new(0.0, 0.0, -step), base), 1.0);
        assert_eq!(pricked(BlockEffect::SweetBerryBush, Vec3::new(0.0029, 0.0, 0.0029), base), 0.0);
        assert_eq!(pricked(BlockEffect::SweetBerryBush, Vec3::new(0.0, -0.5, 0.0), base), 0.0);
        // age 0 slows and deals nothing, on the client nothing is dealt, and what is immune is not hurt
        assert_eq!(pricked(BlockEffect::SweetBerrySapling, Vec3::new(0.1, 0.0, 0.0), base), 0.0);
        let client = SimulationState { client_side: true, ..base };
        assert_eq!(pricked(BlockEffect::SweetBerryBush, Vec3::new(0.1, 0.0, 0.0), client), 0.0);
        let fox = SimulationState { sweet_berry_bush_immune: true, ..base };
        assert_eq!(pricked(BlockEffect::SweetBerryBush, Vec3::new(0.1, 0.0, 0.0), fox), 0.0);
    }

    #[test]
    fn a_young_bush_slows_as_a_grown_one() {
        let mut state = SimulationState {
            position: Vec3::new(0.5, 1.5, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            ..SimulationState::default()
        };
        let world = world_with_bush(BlockEffect::SweetBerrySapling);
        tick(&mut state, &world, PhysicsConfig::default());
        assert_eq!(
            state.stuck_speed_multiplier,
            Vec3::new(java_float(0.8), 0.75, java_float(0.8))
        );
    }

    #[test]
    fn fire_resistance_refuses_every_hot_surface_and_leaves_the_rest() {
        // LivingEntity.hurt returns false for a fire source under the effect (1.8.8
        // LivingEntity.java:683, 1.20.4 :1108, 26.3 :1154); a hot floor and a fire are such sources,
        // a cactus is not
        let hit = |effect: BlockEffect, below: bool, fire_resistance: bool| {
            let mut world = WorldSnapshot::new();
            world.set_block(
                (0, i32::from(!below), 0),
                BlockState {
                    shape: Some(BlockShape {
                        effect,
                        bounds: if below {
                            BlockShape::full_block().bounds
                        } else {
                            Aabb::new(Vec3::ZERO, Vec3::ZERO)
                        },
                        ..BlockShape::full_block()
                    }),
                    ..BlockState::default()
                },
            );
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                on_ground: below,
                no_gravity: true,
                fire_resistance,
                ..SimulationState::default()
            };
            tick(&mut state, &world, PhysicsConfig::default());
            state.contact_damage
        };
        assert_eq!(hit(BlockEffect::Magma, true, false), 1.0);
        assert_eq!(hit(BlockEffect::Magma, true, true), 0.0);
        for (effect, damage) in [
            (BlockEffect::Campfire, 1.0),
            (BlockEffect::SoulCampfire, 2.0),
            (BlockEffect::Fire, 1.0),
            (BlockEffect::SoulFire, 2.0),
        ] {
            assert_eq!(hit(effect, false, false), damage);
            assert_eq!(hit(effect, false, true), 0.0);
        }
        assert_eq!(hit(BlockEffect::Cactus, false, false), 1.0);
        assert_eq!(hit(BlockEffect::Cactus, false, true), 1.0);
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

        let ground_drag = java_float(java_float(0.6) * java_float(0.91));
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

        let ground_drag = java_float(java_float(0.6) * java_float(0.91));
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

        let ground_drag = java_float(java_float(0.6) * java_float(0.91));
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

        let ground_drag = java_float(java_float(0.6) * java_float(0.91));
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
        // entity this travel moves is one (1.21.11 SlimeBlock.java:43, 1.9.4 SlimeBlock.java:38-41)
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
        // BedBlock.bounceUp: the 0.8 is for an entity that is not living (1.21.11 BedBlock.java:142-143)
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
        landing_damage_as(true, version, effect, top, sneaking)
    }

    fn landing_damage_as(is_player: bool, version: &str, effect: BlockEffect, top: f64, sneaking: bool) -> (f64, f64) {
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
            is_player,
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
        // has no fallOn), HoneyBlock.fallOn the same (1.15 HoneyBlock.java:55-61, 1.21.5 :48-54).
        // ceil((20F - 3F) * 0.2F) = ceil(3.4000001F) before 1.21.5 (1.20.4 LivingEntity.java:1572,
        // 1.21.4 :1662-1664), floor((20 + 1.0E-6 - 3) * 0.2F) from it (1.21.5 :1643-1653).
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
        // PointedDripstoneBlock.java:131-133, 1.21.4 :135-137): ceil((22F - 3F) * 2.0F); from
        // 1.21.5 causeFallDamage(fd + 2.5, 2.0F) (1.21.5 :135-137): floor((22.5 + 1.0E-6 - 3) * 2)
        for version in ["1.17", "1.19.4", "1.20.4", "1.21.1"] {
            let (damage, fall_distance) =
                landing_damage_on(version, BlockEffect::PointedDripstoneTip, 1.0, false);
            assert_eq!((damage, fall_distance), (38.0, 0.0), "{version}");
        }
        assert_eq!(landing_damage_on("1.21.5", BlockEffect::PointedDripstoneTip, 1.0, false).0, 39.0);
        assert_eq!(landing_damage_on("26.3", BlockEffect::PointedDripstoneTip, 1.0, false).0, 39.0);
    }

    #[test]
    fn landing_on_powder_snow_hurts_only_a_mob_before_1_18() {
        // PowderSnowBlock.fallOn only plays a sound from 1.18 (1.18 PowderSnowBlock.java:87-93,
        // 1.21.5 :84-90); 1.17 and 1.17.1 (whose jar declares no fallOn) keep Block.fallOn for a mob,
        // which lands in its own move. A player's landing is ServerPlayer.doCheckFallDamage, after the
        // server's move ran the block's slowMovement, which zeroes the fall (1.17 Entity.java:2134-2137,
        // ServerGamePacketListenerImpl.java:852, :884): fd_on_powder_snow_h6 on 1.17 and 1.17.1, health 20.
        // Over 2.5 blocks of fall the snow holds the player at 0.9F (1.17 :115).
        let top = f64::from(0.9_f32);
        assert_eq!(landing_damage_as(false, "1.17", BlockEffect::PowderSnow, top, false).0, 17.0);
        assert_eq!(landing_damage_as(false, "1.17.1", BlockEffect::PowderSnow, top, false).0, 17.0);
        assert_eq!(landing_damage_as(false, "1.18", BlockEffect::PowderSnow, top, false).0, 0.0);
        assert_eq!(landing_damage_on("1.17", BlockEffect::PowderSnow, top, false), (0.0, 0.0));
        assert_eq!(landing_damage_on("1.17.1", BlockEffect::PowderSnow, top, false), (0.0, 0.0));
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
            controlled_by_local_instance: true,
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
        // fallDistance -= (double)(float)dy (1.21.5 Entity.java:1323)
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
        // STEP_HEIGHT's 0..10 (1.21.11 RangedAttribute.java:30-32, Attributes.java:26-66, 42-80)
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
        // client1.14.4 Entity.java:600-640 and 1.18 Player.java:1017-1019 test no rising
        // move; 1.19 adds `movement.y <= 0.0` (1.19 Player.java:1056-1061)
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
        // before 1.16.2 only `onGround` (1.16 Player.java:1030-1031); from
        // 1.16.2 isAboveGround: the box moved down by the step less the fall distance meets the
        // ground (1.16.5 Player.java:1111-1114)
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
        // 0-7; 1.16 Player.java has none)
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
        // before 1.20.5 the whole box moved by (x, -step, z) (1.20.4 Player.java:1055); from
        // 1.20.5 the feet's slab (1.20.5 :1076-1079)
        let mut world = WorldSnapshot::new();
        ground_block(&mut world, (0, 0, 0), 1.0);
        ground_block(&mut world, (1, 1, 0), 1.0);
        let walk = Vec3::new(0.8, 0.0, 0.0);
        assert_eq!(backed_off_x(&world, sneaker(1.0), walk, "1.20.4"), 0.8);
        assert!(backed_off_x(&world, sneaker(1.0), walk, "1.20.5") < 0.8);
    }

    #[test]
    fn the_feet_slab_reaches_the_float_step_and_its_epsilon_below() {
        // 1.20.5 - 1.21.4: minY - (float) 0.6F - 1.0E-5F (1.21.4 Player.java:1070-1073);
        // 1.21.5 on: minY - 0.6F - 1e-7, 1e-7 in all round (1.21.5 Player.java:1023-1026); the
        // whole box moved by -0.6F before (1.20.4 Player.java:1055)
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
        // cube takes the strict `intersects`, client1.21.11 BlockCollisions.java:96-98; another
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
        // the 1e-7 inset all round arrives in 1.21.5 (1.21.5 Player.java:1023-1026)
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
        // CollisionGetter.java:52-54, EntityGetter.java:51-77; client1.14.4 EntityGetter.java
        // :54-68); before 1.14.4 Level.getCollisionBoxes(entity, box) does (1.8.8 Entity.java:489,
        // 1.12.2 Entity.java:619 and Level.java:1161-1184, 1.13.2 Entity.java:632 and Level.java
        // :1794-1797)
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
        // BlockCollisions.java:86-104, 1.13.2 LevelReader.java:122); a fence's 1.5 high shape
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
        assert!(world.is_free(hop, MinecraftVersion::new(26, 3, 0)));
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
        // from 1.13 a block's shape counts only where Shapes.joinIsNotEmpty /
        // Shapes.joinIsNotEmpty(shape, box, AND) finds it, and the index merger merges coordinates
        // within 1e-7 (1.14.4 IndirectMerger.java:42-46, client1.21.11 IndirectMerger.java
        // ctor); a full cube takes the strict `intersects` from 1.16 (1.16.5
        // CollisionSpliterator.java:75-82), every shape before 1.13 (1.12.2 Block.java
        // :435-439)
        let mut half = WorldSnapshot::new();
        place_box(&mut half, (0, 0, 0), Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0));
        // the box's min x 5e-8 inside the shape's face at x = 0.5
        let against_half = Vec3::new(0.8 - 5.0e-8, 0.0, 0.5);
        for (version, z) in [
            ("1.12.2", 0.3),
            // (1.13.2's merger merges the coordinates closer than 1e-7, class_4092.java:37)
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
        // 1.5e-7 inside the face: no release merges it away, 1.13.x included, whose lowered box is
        // the bounding box offset with no deflate (1.13.2 Entity.java:632, 644, 653)
        let pressed_half = Vec3::new(0.8 - 1.5e-7, 0.0, 0.5);
        for version in ["1.12.2", "1.13.2", "1.14.4", "1.16.5", "1.20.4"] {
            assert_eq!(
                backed_off_z_from(&half, pressed_half, version),
                0.3,
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
    fn an_entity_collider_the_sneaker_overlaps_by_5e_8_holds_it_in_1_13_2() {
        // 1.13.2 Level.getCollisions adds the entity boxes that `intersects` the lowered box, and
        // that box is the bounding box offset by (x, -stepHeight, z) with no deflate (1.13.2
        // Level.java:1794-1797, class_3593.java:22-31, Entity.java:632, 644, 653); from 1.14 the
        // colliders meeting the query grown by 1e-7 (client1.14.4 EntityGetter.java:54-68)
        let mut world = WorldSnapshot::new();
        // its +x side 5e-8 inside the sneaker's -x side at 0.5
        world.set_entity_colliders(vec![EntityCollider {
            bounds: Aabb::new(Vec3::new(-0.5, 0.5, -1.0), Vec3::new(0.5 + 5.0e-8, 1.0, 2.0)),
            only_from_above: false,
        }]);
        let at = Vec3::new(0.8, 1.0, 0.5);
        for version in ["1.12.2", "1.13.2", "1.14.4", "1.16.5", "1.21.11"] {
            assert_eq!(backed_off_z_from(&world, at, version), 0.3, "{version}");
        }
        // clear of it by 5e-8 the sneaker backs off, but from 1.14 the 1e-7 growth still meets it;
        // from 1.21.5 the feet's slab is 1e-7 in all round (1.21.5 Player.java:1019-1026), out of
        // its reach again
        let clear = Vec3::new(0.8 + 1.0e-7, 1.0, 0.5);
        for (version, z) in [
            ("1.12.2", 0.0),
            ("1.13.2", 0.0),
            ("1.14.4", 0.3),
            ("1.20.4", 0.3),
            ("1.21.11", 0.0),
        ] {
            assert_eq!(backed_off_z_from(&world, clear, version), z, "{version}");
        }
    }

    #[test]
    fn a_full_unit_box_of_a_joined_shape_takes_the_merger_from_1_16() {
        // the spliterator's strict branch is for the Shapes.block() object only (1.16.5
        // CollisionSpliterator.java:74, client1.21.11 BlockCollisions.java:91); a block whose
        // shape is joined from several boxes, one of them filling the cell, goes through
        // joinIsNotEmpty and its 1e-7 merging (client1.21.11 Shapes.java:179-185)
        let unit = BlockShape {
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        let inner = BlockShape {
            bounds: Aabb::new(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.75, 0.75, 0.75)),
            ..BlockShape::full_block()
        };
        let mut joined = WorldSnapshot::new();
        joined.set_block_shapes(
            (-1, 0, 0),
            BlockState {
                id: 1,
                ..BlockState::default()
            },
            vec![unit, inner],
        );
        let mut block = WorldSnapshot::new();
        place_box(&mut block, (-1, 0, 0), Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        // (the whole box before 1.20.5; the feet's slab after lies below this cube)
        let against_cube = Vec3::new(0.3 - 5.0e-8, 0.0, 0.5);
        for version in ["1.16.5", "1.18.2", "1.20.4"] {
            assert_eq!(backed_off_z_from(&block, against_cube, version), 0.3, "{version}");
            assert_eq!(backed_off_z_from(&joined, against_cube, version), 0.0, "{version}");
        }
    }

    #[test]
    fn the_fluid_hop_box_meets_a_shapes_block_cube_strictly_and_a_joined_one_merged() {
        // Entity.isFree from 1.16: the strict `intersects` for the Shapes.block() object, the
        // merger for any other shape, a joined one filling the cell included (1.16.5
        // CollisionSpliterator.java:74-82, client1.21.11 BlockCollisions.java:91-100)
        let unit = BlockShape {
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        let inner = BlockShape {
            bounds: Aabb::new(Vec3::new(0.25, 0.25, 0.25), Vec3::new(0.75, 0.75, 0.75)),
            ..BlockShape::full_block()
        };
        let mut joined = WorldSnapshot::new();
        joined.set_block_shapes(
            (1, 1, 0),
            BlockState {
                id: 1,
                ..BlockState::default()
            },
            vec![unit, inner],
        );
        let mut block = WorldSnapshot::new();
        place_box(&mut block, (1, 1, 0), Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let context = EntityCollisionContext::default();
        // the hop box 5e-8 into the cube's -x face at x = 1
        let hop = Aabb::new(Vec3::new(0.4 + 5.0e-8, 0.6, 0.2), Vec3::new(1.0 + 5.0e-8, 2.4, 0.8));
        let entity = CollisionQueryEntity {
            context: &context,
            bounds: hop.translated(Vec3::new(0.0, -0.6, 0.0)),
            position: Vec3::new(0.7, 0.0, 0.5),
            outside_border: false,
        };
        for version in ["1.16.5", "1.20.4", "1.21.11", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            assert!(!block.is_free_for_entity(hop, entity, release), "{version}");
            assert!(joined.is_free_for_entity(hop, entity, release), "{version}");
        }
        // merged for both 1.13 - 1.15 (1.15 CollisionGetter.java:109)
        let release = MinecraftVersion::parse("1.15.2").unwrap();
        assert!(block.is_free_for_entity(hop, entity, release));
    }

    #[test]
    fn the_fluid_hop_box_is_free_of_a_face_it_overlaps_within_1e_7_from_1_13() {
        // Entity.isFree's shapes go through matchesAnywhere / joinIsNotEmpty from 1.13 (1.13.2
        // LevelReader.java:123, client1.21.11 BlockCollisions.java:86-104), whose merger merges
        // coordinates within 1e-7 (client1.21.11 IndirectMerger.java ctor); the strict
        // `intersects` before (1.12.2 Block.java:435-439)
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
        // Entity.java:676-678): scaffolding is empty to an entity not above it
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
        assert!(!world.is_free(hop, MinecraftVersion::new(26, 3, 0)));
    }

    #[test]
    fn the_world_border_collides_with_no_collision_queries() {
        // 1.18+: the border's shape where it meets the query, for an entity inside and close to it
        // (1.18 CollisionGetter.java:57-62); 1.14 - 1.17: its shape whatever the query once the
        // entity's box touches it from inside (client1.14.4 LevelReader.java:146-155, 1.16
        // CollisionSpliterator.java:101-115)
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
        assert!(!world.no_collision_for_entity(inside, entity_at(4.7 + 5.0e-8), release("1.17.1")));
        assert!(!world.no_collision_for_entity(inside, entity_at(4.7 + 5.0e-8), release("1.14.4")));
        // (flush with it, it does not: the join's 1e-7 merge drops the sliver)
        assert!(world.no_collision_for_entity(inside, entity_at(4.7), release("1.17.1")));
        assert!(world.no_collision_for_entity(inside, entity_at(4.5), release("1.16.5")));
        // before 1.14 the cells outside are stone to an entity inside
        assert!(!world.no_collision_for_entity(across, entity_at(4.5), release("1.12.2")));
        assert!(world.no_collision_for_entity(inside, entity_at(4.5), release("1.12.2")));
    }

    #[test]
    fn the_1_16_2_push_out_tests_collision_shapes_and_before_any_suffocating_cell() {
        // 1.16 and 1.16.1 LocalPlayer.blocked(): any cell of the column isSuffocating, whatever its
        // shape (1.16 LocalPlayer.java:450-462); 1.16.2 suffocatesAt: a suffocating shape
        // meets the column (1.16.5 LocalPlayer.java:433-437)
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
        // Blocks.WATER nor Blocks.BUBBLE_COLUMN (1.15 Entity.java:681-689, 1.16.5 :739-747, 1.19.2
        // :745-753, 1.21.11 :1026-1034, 26.3 :1131-1139); a waterlogged block or a water plant (speed
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
        // Player.java:2053-2058, 1.21.11 Player.java:1960-1962, 26.3 :1876-1878); 1.21.3 has
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
        // tick's move sweeps (1.21.4 Entity.java:1096-1124, boxTraverseBlocks; 1.21.11
        // Entity.java:897-951 and BlockGetter.java:161-195, WebBlock.java:24-34); before, only for
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
    fn a_long_fall_reaches_block_effects_only_as_far_as_the_budget() {
        // A fall of 30 from 40. From 1.21.9 the walk accepts 15 iterations, the cells 39 - 25 (26.3
        // Entity.java:1374, BlockGetter.java:248-250), then checkInsideBlocks(to, to, 1) reads the
        // final box at 10 and 11 (:1342-1344); 1.21.8 walks the cells 39 - 23 and always ends with
        // that box (1.21.8 BlockGetter.java:186-197, 234-236). A cobweb sticks (WebBlock.entityInside),
        // powder snow freezes, a bubble column pushes, here precise by movedFar (26.3
        // Entity.java:1357, 1392, BubbleColumnBlock.entityInside): each block's effect or nothing
        let falling = SimulationState {
            position: Vec3::new(0.5, 40.0, 0.5),
            velocity: Vec3::new(0.0, -30.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ticks_frozen: 10,
            ..SimulationState::default()
        };
        let empty_shaped = |effect| BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                effect,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let fallen = |cell: Option<(i32, BlockState)>, version: &str| {
            let mut world = WorldSnapshot::new();
            if let Some((y, block)) = cell {
                world.set_block((0, y, 0), block);
            }
            let mut state = falling;
            crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                .unwrap();
            assert_eq!(state.position.y, 10.0, "{version}");
            state
        };
        let plain_water = BlockState {
            fluid: FluidState::Water,
            fluid_source: true,
            ..BlockState::default()
        };
        for (y, reached) in [
            (25, ["1.21.8", "1.21.9", "1.21.10", "26.3"].as_slice()),
            (24, ["1.21.8"].as_slice()),
            (23, ["1.21.8"].as_slice()),
            (22, [].as_slice()),
            (11, ["1.21.8", "1.21.9", "1.21.10", "26.3"].as_slice()),
            (10, ["1.21.8", "1.21.9", "1.21.10", "26.3"].as_slice()),
        ] {
            for version in ["1.21.8", "1.21.9", "1.21.10", "26.3"] {
                let expected = reached.contains(&version);
                let stuck = fallen(Some((y, empty_shaped(BlockEffect::Web))), version);
                assert_eq!(
                    stuck.stuck_speed_multiplier != Vec3::ZERO,
                    expected,
                    "web {y} {version}"
                );

                let frozen = fallen(Some((y, empty_shaped(BlockEffect::PowderSnow))), version);
                let unfrozen = fallen(None, version);
                assert_eq!(
                    frozen.ticks_frozen > unfrozen.ticks_frozen,
                    expected,
                    "powder snow {y} {version}"
                );

                let column = BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    ..empty_shaped(BlockEffect::BubbleUp)
                };
                let pushed = fallen(Some((y, column)), version);
                let unpushed = fallen(Some((y, plain_water)), version);
                assert_eq!(
                    pushed.velocity.y != unpushed.velocity.y,
                    expected,
                    "bubbles {y} {version}"
                );
            }
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
        // (1.9.4 LocalPlayer.java:710-713, 1.13.2 :791-794, 1.14.4 :682-687); 1.8.9 only
        // moves it down (1.8.8 LocalPlayer.java:594-601)
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
        // (1.21.5 LivingEntity.java:2273-2276, 1.21.11 :2412-2415); 1.21.4 glides on
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
        // a shulker (1.10.2 LocalPlayer.java:871, 1.21.11 LocalPlayer.java:1018,
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
        // Level.getCollisions: blocks then entities before 1.18 (1.16.5 CollisionGetter.java:58-59,
        // 1.17 :57-58), entities then blocks from 1.18 (1.18 CollisionGetter.java:67-70); the first
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
        // LocalPlayer.aiStep: motionY -= flySpeed * 3.0F, then += it, before 1.14 (1.8.8
        // LocalPlayer.java:594-601, 1.13.2 :792-800); from 1.14 one netted impulse (1.14.4
        // LocalPlayer.java:684-698), none with both held
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
        // LocalPlayer.updateAutoJump: from position() in 1.15 (1.15 LocalPlayer.java:860), from
        // the box's minY in 1.14 (1.14.4 LocalPlayer.java:817-819, 1.13.2 :881-882); a box kept
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
        // merged boxes (1.13.2 LevelReader.java:113-155, DiscreteVoxelShape.java:200-253; 1.14.4
        // LevelReader.java:179-201); the walk takes each block from 1.14.4 (LevelReader.java:130-145)
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
    fn auto_jump_takes_the_first_box_of_the_1_12_walk_x_then_z_then_y() {
        // D-195: before 1.13 Level.getCollisionBoxes walks x outermost, then z, then y (1.12.2
        // Level.method_13689; 1.10.2 Level.java:1055-1092), and the first box the lines cross
        // decides (1.12.2 LocalPlayer.java:904-928). Walking along +x+z, a cube at x 2, z 1 with a
        // cube on it and a free cube at x 1, z 2: 1.12 takes the free one first and jumps, the
        // z-outermost walk of 1.14.4 (LevelReader.java:130-145) the covered one and does not
        let full = BlockState {
            id: 1,
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        let mut world = WorldSnapshot::new();
        world.set_block((2, 1, 1), full);
        world.set_block((2, 2, 1), full);
        world.set_block((1, 1, 2), full);
        let queued = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                input: Vec3::new(0.0, 0.0, 1.0),
                on_ground: true,
                is_player: true,
                auto_jump: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            crate::simulation::update_auto_jump(&mut state, &world, Vec3::new(0.2, 0.0, 0.2), 0.3, 1.0, release);
            state.auto_jump_time
        };
        for release in [MinecraftVersion::new(1, 10, 2), MinecraftVersion::new(1, 11, 2), MinecraftVersion::new(1, 12, 2)] {
            assert_eq!(queued(release), 1, "{release:?}");
        }
        assert_eq!(queued(MinecraftVersion::new(1, 14, 4)), 0);
    }

    #[test]
    fn the_1_12_auto_jump_probes_a_brewing_stand_by_its_bounding_box() {
        // D-196: before 1.13 the probe above the step reads getCollisionBoundingBox, one box: a
        // brewing stand's is its 0.125 base (getShape; 1.12.2 BrewingStandBlock.java:36, 89), not
        // its rod's 0.875 collision box, so a step under one is low enough to jump in 1.12.2 and
        // too high in 1.13.2, whose probe takes the collision shape
        let mut world = floor_with_step(1.0);
        let shape = |max_y: f64, min_xz: f64, max_xz: f64| BlockShape {
            bounds: Aabb::new(Vec3::new(min_xz, 0.0, min_xz), Vec3::new(max_xz, max_y, max_xz)),
            ..BlockShape::full_block()
        };
        let stand = BlockState {
            id: 2,
            tags: crate::world::LEGACY_PROBE_OUTLINE,
            ..BlockState::default()
        };
        world.set_block_shapes_with_outline(
            (0, 2, 2),
            stand,
            vec![shape(0.875, 0.4375, 0.5625), shape(0.125, 0.0, 1.0)],
            vec![shape(0.125, 0.0, 1.0)],
        );
        let queued = |release| {
            let mut state = walking_to_step(true);
            tick_release(&mut state, &world, release);
            state.auto_jump_time
        };
        assert_eq!(queued(MinecraftVersion::new(1, 12, 2)), 1);
        assert_eq!(queued(MinecraftVersion::new(1, 13, 2)), 0);
        // and the lily pad's is none (BushBlock.getCollisionShape), its 0.09375 box ignored
        let mut lily = floor_with_step(1.0);
        lily.set_block((0, 2, 2), BlockState { id: 3, tags: crate::world::LEGACY_PROBE_NONE, shape: Some(shape(1.0, 0.0, 1.0)), ..BlockState::default() });
        let mut state = walking_to_step(true);
        tick_release(&mut state, &lily, MinecraftVersion::new(1, 12, 2));
        assert_eq!(state.auto_jump_time, 1);
    }

    #[test]
    fn the_merged_full_cubes_follow_the_partial_boxes() {
        // 1.13.2 LevelReader.java:151-154: the bitset shape is concatenated after the walk; its
        // boxes run along z, then grow along x and y (DiscreteVoxelShape.java:200-253)
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
        // before 1.20.5 (1.20.4 ClientboundUpdateMobEffectPacket.java:50, MobEffectInstance.java:76;
        // 1.8.8 ClientboundUpdateMobEffectPacket.java:59), so `/effect ... 255` is amplifier -1
        let air = |version: &str, levitation_amplifier: Option<i32>| {
            let mut state = SimulationState {
                is_player: true,
                levitation: true,
                levitation_amplifier,
                position: Vec3::new(0.5, 20.0, 0.5),
                half_width: 0.3,
                height: 1.8,
                controlled_by_local_instance: true,
                client_side: true,
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
        // (1.20.4 LivingEntity.java:2157; 1.9.4 LivingEntity.java:1417-1418)
        for version in ["1.9.4", "1.12.2", "1.16.5", "1.20.4"] {
            let state = air(version, Some(-1));
            assert_eq!(state.velocity.y, 0.0, "{version}");
            assert_eq!(state.position.y, 20.0, "{version}");
            assert!(air(version, Some(-2)).velocity.y < 0.0, "{version}");
        }

        // jumpFromGround adds 0.1F * (amplifier + 1): nothing at -1, less at -2 (1.20.4
        // LivingEntity.java:2017, 1.8.8 LivingEntity.java:1110-1113)
        let jump =
            |version: &str, jump_boost_amplifier: Option<i32>, jump_strength: Option<f64>| {
                let mut state = SimulationState {
                    is_player: true,
                    client_side: true,
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

        // computeFallDamage's jump boost is (float)(amplifier + 1) (1.20.4 LivingEntity.java:1569)
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
        // (1.21 LivingEntity.java:2115-2121). A jump into a ceiling leaves 0 after the move.
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

    fn effect_block(effect: BlockEffect, bounds: Aabb) -> BlockState {
        BlockState {
            shape: Some(BlockShape {
                bounds,
                effect,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    fn fire_world(effect: BlockEffect) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 1, 0), effect_block(effect, Aabb::new(Vec3::ZERO, Vec3::ZERO)));
        world
    }

    fn standing_in_fire(is_player: bool) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player,
            no_gravity: true,
            ticks_frozen: 100,
            ..SimulationState::default()
        }
    }

    #[test]
    fn fire_clears_the_freeze_ticks_once_the_fire_ticks_reach_0_before_1_21_5() {
        // BaseFireBlock.entityInside adds a fire tick and sets eight seconds of fire when
        // they reach 0 (1.17 BaseFireBlock.java:124-132; 1.21.3 :121-133 adds them only below
        // 0); a player starts at -getFireImmuneTicks = -20 (1.17 Player.java:433-435,
        // Entity.java:176). The burning Entity.baseTick of the next tick clears the freeze ticks
        // (1.17 Entity.java:419-439, 1.21.4 :486-506); until then aiStep takes two off a tick
        let world = fire_world(BlockEffect::Fire);
        for release in [
            MinecraftVersion::new(1, 17, 0),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
        ] {
            let mut state = standing_in_fire(true);
            for _ in 0..20 {
                tick_release(&mut state, &world, release);
                assert_eq!(state.contact_damage, 1.0, "{release:?}");
            }
            assert_eq!(state.remaining_fire_ticks, Some(160), "{release:?}");
            assert_eq!(state.ticks_frozen, 60, "{release:?}");
            tick_release(&mut state, &world, release);
            assert_eq!(state.ticks_frozen, 0, "{release:?}");
            // the base tick's one off, the contact's one back (a player's from 1.21.2)
            assert_eq!(state.remaining_fire_ticks, Some(160), "{release:?}");
        }
        // any other entity starts at -1: the first contact ignites it
        let release = MinecraftVersion::new(1, 20, 4);
        let mut mob = standing_in_fire(false);
        tick_release(&mut mob, &world, release);
        assert_eq!(mob.remaining_fire_ticks, Some(160));
        assert_eq!(mob.ticks_frozen, 98);
        tick_release(&mut mob, &world, release);
        assert_eq!(mob.ticks_frozen, 0);
    }

    #[test]
    fn fire_ignition_clears_the_freeze_ticks_at_once_from_1_21_5_and_clear_freeze_from_1_21_9() {
        // BaseFireBlock.fireIgnite's igniteForSeconds(8) clears them (1.21.5
        // BaseFireBlock.java:123-134, Entity.java:598-604) once the player's ticks reach 0; from
        // 1.21.9 the fire's CLEAR_FREEZE clears them on the first contact (1.21.9
        // BaseFireBlock.java:118-123, InsideBlockEffectType.java)
        let world = fire_world(BlockEffect::SoulFire);
        let cold = SimulationState {
            remaining_fire_ticks: Some(-20),
            ..standing_in_fire(true)
        };
        for release in [MinecraftVersion::new(1, 21, 5), MinecraftVersion::new(1, 21, 8)] {
            let mut state = cold;
            for _ in 0..19 {
                tick_release(&mut state, &world, release);
            }
            assert_eq!(state.remaining_fire_ticks, Some(-1), "{release:?}");
            assert_eq!(state.ticks_frozen, 62, "{release:?}");
            assert_eq!(state.contact_damage, 2.0, "{release:?}");
            tick_release(&mut state, &world, release);
            assert_eq!(state.remaining_fire_ticks, Some(160), "{release:?}");
            assert_eq!(state.ticks_frozen, 0, "{release:?}");
        }
        for release in [MinecraftVersion::new(1, 21, 9), MinecraftVersion::new(26, 3, 0)] {
            let mut state = cold;
            tick_release(&mut state, &world, release);
            assert_eq!(state.ticks_frozen, 0, "{release:?}");
            assert_eq!(state.remaining_fire_ticks, Some(-19), "{release:?}");
        }
    }

    #[test]
    fn rain_puts_a_fire_free_players_ticks_to_0_before_1_21_9() {
        // Entity.clearFire is setRemainingFireTicks(0) up to 1.21.8 (1.21.5 Entity.java:581-583,
        // 1.21.8 :641-643) and min(0, ticks) from 1.21.9 (1.21.9 Entity.java:648-650); the rain's
        // clearFire runs after the inside effects (1.21.5 Entity.java:849-851). In 1.21.5 a
        // player that was not burning keeps the 0 (the reset is for one that burned: :817-819);
        // from 1.21.6 the 0 counts as raised (1.21.6 Entity.java:886-890), so the ticks go 0,
        // -20, 0 while the rain lasts
        let empty = WorldSnapshot::new();
        let rained_on = SimulationState {
            in_rain: true,
            remaining_fire_ticks: Some(-20),
            ..standing_in_fire(true)
        };
        for (release, after) in [
            (MinecraftVersion::new(1, 21, 5), [0, 0, 0]),
            (MinecraftVersion::new(1, 21, 6), [0, -20, 0]),
            (MinecraftVersion::new(1, 21, 8), [0, -20, 0]),
            (MinecraftVersion::new(1, 21, 9), [-20, -20, -20]),
            (MinecraftVersion::new(26, 3, 0), [-20, -20, -20]),
        ] {
            let mut state = rained_on;
            for ticks in after {
                tick_release(&mut state, &empty, release);
                assert_eq!(state.remaining_fire_ticks, Some(ticks), "{release:?}");
            }
        }
        // so the next fire contact of 1.21.5-1.21.8 reaches 0 at once: BaseFireBlock.fireIgnite
        // adds a player's tick and igniteForSeconds(8) clears the freeze ticks (1.21.5
        // BaseFireBlock.java:123-134, Entity.java:598-604), not twenty contacts later
        let fire = fire_world(BlockEffect::Fire);
        for release in [MinecraftVersion::new(1, 21, 5), MinecraftVersion::new(1, 21, 8)] {
            let mut state = rained_on;
            tick_release(&mut state, &empty, release);
            assert_eq!(state.ticks_frozen, 98, "{release:?}");
            state.in_rain = false;
            tick_release(&mut state, &fire, release);
            assert_eq!(state.ticks_frozen, 0, "{release:?}");
            assert_eq!(state.remaining_fire_ticks, Some(160), "{release:?}");
        }
    }

    #[test]
    fn freeze_ticks_over_the_limit_fall_by_two() {
        // LivingEntity.aiStep clamps only the count up, min(required, i + 1); the count down is
        // max(0, i - 2) (1.17 LivingEntity.java:2546-2552, 1.21.5 :2767-2768)
        for release in [MinecraftVersion::new(1, 20, 4), MinecraftVersion::new(1, 21, 5)] {
            let mut state = SimulationState {
                ticks_frozen: 300,
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &WorldSnapshot::new(), release);
            assert_eq!(state.ticks_frozen, 298, "{release:?}");
            let mut snowed = SimulationState {
                position: Vec3::new(0.5, 0.0, 0.5),
                ticks_frozen: 300,
                ..standing_in_fire(true)
            };
            tick_release(&mut snowed, &powder_snow_world(), release);
            assert_eq!(snowed.ticks_frozen, 140, "{release:?}");
        }
    }

    #[test]
    fn fire_ticks_reset_after_leaving_fire_as_the_release_tests_it() {
        // Entity.applyEffectsFromBlocks puts the ticks back to -getFireImmuneTicks: in 1.21.5 only
        // for an entity that burned before and no longer does (1.21.5 Entity.java:857-859), from
        // 1.21.6 for any entity not burning whose ticks no ignition raised (1.21.6
        // Entity.java:887-890, 26.3 :937-940); before 1.21.5 with no fire or lava in the box
        // (1.17 Entity.java:633-638)
        let fire = fire_world(BlockEffect::Fire);
        let empty = WorldSnapshot::new();
        for (release, after) in [
            (MinecraftVersion::new(1, 20, 4), -20),
            (MinecraftVersion::new(1, 21, 5), -10),
            (MinecraftVersion::new(1, 21, 6), -20),
            (MinecraftVersion::new(26, 3, 0), -20),
        ] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(-20),
                ..standing_in_fire(true)
            };
            for _ in 0..10 {
                tick_release(&mut state, &fire, release);
            }
            assert_eq!(state.remaining_fire_ticks, Some(-10), "{release:?}");
            tick_release(&mut state, &empty, release);
            assert_eq!(state.remaining_fire_ticks, Some(after), "{release:?}");
        }
    }

    #[test]
    fn a_dead_entity_keeps_its_fire_count_from_1_21_2_to_1_21_4() {
        // 1.21.3 Entity.java:795 asks isAlive before it resets the count; 1.21.1 does not ask
        let empty = WorldSnapshot::new();
        for (release, after) in [
            (MinecraftVersion::new(1, 21, 1), -20),
            (MinecraftVersion::new(1, 21, 2), 0),
            (MinecraftVersion::new(1, 21, 4), 0),
        ] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(0),
                dead: true,
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &empty, release);
            assert_eq!(state.remaining_fire_ticks, Some(after), "{release:?}");
        }
    }

    #[test]
    fn a_burning_entity_keeps_its_freeze_ticks_cleared_until_put_out_before_1_21_5() {
        // the burning baseTick clears them every tick (1.17 Entity.java:437-440); water puts the
        // fire out first (Entity.updateInWaterStateAndDoWaterCurrentPushing, 1.17 Entity.java:1059-1070), powder snow after
        // the move (1.17 Entity.java:645-647), and the freezing then counts again
        let release = MinecraftVersion::new(1, 20, 4);
        let burning = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ticks_frozen: 50,
            remaining_fire_ticks: Some(100),
            ..SimulationState::default()
        };
        let mut dry = burning;
        tick_release(&mut dry, &WorldSnapshot::new(), release);
        assert_eq!(dry.ticks_frozen, 0);
        assert_eq!(dry.remaining_fire_ticks, Some(99));

        let mut wet = burning;
        tick_release(&mut wet, &fluid_block_world(FluidState::Water), release);
        assert!(wet.in_water);
        assert_eq!(wet.ticks_frozen, 48);
        assert_eq!(wet.remaining_fire_ticks, Some(-20));

        let mut rained_on = SimulationState {
            in_rain: true,
            ..burning
        };
        tick_release(&mut rained_on, &WorldSnapshot::new(), release);
        // LivingEntity.baseTick's isWet after the burning baseTick (1.20.4 LivingEntity.java:425)
        assert_eq!(rained_on.ticks_frozen, 0);
        assert_eq!(rained_on.remaining_fire_ticks, Some(-20));

        let mut snowed = burning;
        tick_release(&mut snowed, &powder_snow_world(), release);
        assert!(snowed.in_powder_snow);
        assert_eq!(snowed.remaining_fire_ticks, Some(-20));
        assert_eq!(snowed.ticks_frozen, 1);
        tick_release(&mut snowed, &powder_snow_world(), release);
        assert_eq!(snowed.ticks_frozen, 2);
    }

    #[test]
    fn rain_and_powder_snow_put_the_fire_out_from_1_21_5() {
        // the rain's clearFire after the inside effects and powder snow's EXTINGUISH (1.21.5
        // Entity.java:849-851, PowderSnowBlock.java:79-93); no burning baseTick clears the
        // freeze ticks from 1.21.5 (1.21.5 Entity.java:498-512)
        let release = MinecraftVersion::new(1, 21, 11);
        let burning = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ticks_frozen: 50,
            remaining_fire_ticks: Some(100),
            ..SimulationState::default()
        };
        let mut dry = burning;
        tick_release(&mut dry, &WorldSnapshot::new(), release);
        assert_eq!(dry.ticks_frozen, 48);
        assert_eq!(dry.remaining_fire_ticks, Some(99));
        let mut rained_on = SimulationState {
            in_rain: true,
            ..burning
        };
        tick_release(&mut rained_on, &WorldSnapshot::new(), release);
        assert_eq!(rained_on.remaining_fire_ticks, Some(-20));
        let mut snowed = burning;
        tick_release(&mut snowed, &powder_snow_world(), release);
        assert_eq!(snowed.remaining_fire_ticks, Some(-20));
        assert_eq!(snowed.ticks_frozen, 51);
    }

    #[test]
    fn lava_sets_the_fire_ticks_with_fire_protection_or_burning_time() {
        // setOnFireFromLava's fifteen seconds in the base tick before 1.21.5 (1.17
        // Entity.java:443-445, 485-492), less 15% a Fire Protection level before 1.21 (1.17
        // ProtectionEnchantment.java:59-66), times BURNING_TIME from 1.21 (1.21
        // LivingEntity.java:3555-3557); the next base tick counts one off
        let lava = fluid_block_world(FluidState::Lava);
        let in_lava = SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ..SimulationState::default()
        };
        let mut plain = in_lava;
        tick_release(&mut plain, &lava, MinecraftVersion::new(1, 20, 4));
        assert_eq!(plain.remaining_fire_ticks, Some(300));
        let mut protected = SimulationState {
            fire_protection: 4,
            ..in_lava
        };
        tick_release(&mut protected, &lava, MinecraftVersion::new(1, 20, 4));
        assert_eq!(protected.remaining_fire_ticks, Some(120));
        let mut attribute = SimulationState {
            burning_time: Some(0.5),
            ..in_lava
        };
        tick_release(&mut attribute, &lava, MinecraftVersion::new(1, 21, 1));
        assert_eq!(attribute.remaining_fire_ticks, Some(150));
        // a fire-immune entity neither ignites nor takes fire contact damage (1.17
        // Entity.java:486, BaseFireBlock.java:125)
        let mut immune = SimulationState {
            fire_immune: true,
            ticks_frozen: 50,
            ..standing_in_fire(false)
        };
        tick_release(&mut immune, &fire_world(BlockEffect::Fire), MinecraftVersion::new(1, 20, 4));
        assert_eq!(immune.contact_damage, 0.0);
        assert_eq!(immune.ticks_frozen, 48);
        assert!(immune.remaining_fire_ticks.unwrap_or(-1) <= 0);
    }

    fn cauldron_world(effect: BlockEffect) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        // the cauldron's floor (AbstractCauldronBlock.SHAPE's 3 - 4 pixel bottom)
        world.set_block(
            (0, 0, 0),
            effect_block(
                effect,
                Aabb::new(Vec3::new(0.0, 0.1875, 0.0), Vec3::new(1.0, 0.25, 1.0)),
            ),
        );
        world
    }

    #[test]
    fn a_lava_cauldron_clears_the_freeze_ticks_as_the_release_applies_it() {
        // before 1.21.5 setOnFireFromLava with the entity in its lava (1.17
        // LavaCauldronBlock.java:26-30, AbstractCauldronBlock.java:52-54), whose burning base
        // tick clears them next tick; 1.21.5 - 1.21.8 lavaIgnite at once (1.21.5
        // LavaCauldronBlock.java:31-36); from 1.21.9 CLEAR_FREEZE through its filled inside shape
        // (1.21.9 LavaCauldronBlock.java:38-46)
        let world = cauldron_world(BlockEffect::LavaCauldron);
        let inside = SimulationState {
            position: Vec3::new(0.5, 0.25, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            no_gravity: true,
            ticks_frozen: 50,
            ..SimulationState::default()
        };
        let mut legacy = inside;
        tick_release(&mut legacy, &world, MinecraftVersion::new(1, 20, 4));
        assert_eq!(legacy.remaining_fire_ticks, Some(300));
        assert_eq!(legacy.ticks_frozen, 48);
        tick_release(&mut legacy, &world, MinecraftVersion::new(1, 20, 4));
        assert_eq!(legacy.ticks_frozen, 0);
        for release in [
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 9),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = inside;
            tick_release(&mut state, &world, release);
            assert_eq!(state.ticks_frozen, 0, "{release:?}");
            assert_eq!(state.remaining_fire_ticks, Some(300), "{release:?}");
        }
        // above the content (feet at 0.9375 or more) it is not in the lava
        let mut above = SimulationState {
            position: Vec3::new(0.5, 0.95, 0.5),
            ..inside
        };
        tick_release(&mut above, &world, MinecraftVersion::new(1, 21, 5));
        assert_eq!(above.ticks_frozen, 48);
    }

    #[test]
    fn a_cauldrons_filled_inside_shape_holds_from_1_21_6() {
        // 1.21.5 lavaIgnite only for an entity whose box tops the floor's 0.25 and whose feet are
        // under the content (AbstractCauldronBlock.java:49-51, LavaCauldronBlock.java:31-36);
        // from 1.21.6 LAVA_IGNITE for a box meeting the cauldron with its content (1.21.6
        // LavaCauldronBlock.java:37-44, 51), its floor too
        let world = cauldron_world(BlockEffect::LavaCauldron);
        let low = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            half_width: 0.3,
            height: 0.2,
            no_gravity: true,
            ..SimulationState::default()
        };
        let lit = |release: MinecraftVersion| {
            let mut state = low;
            tick_release(&mut state, &world, release);
            state.remaining_fire_ticks.unwrap_or(0) > 0
        };
        assert!(!lit(MinecraftVersion::new(1, 21, 5)));
        for release in [
            MinecraftVersion::new(1, 21, 6),
            MinecraftVersion::new(1, 21, 8),
            MinecraftVersion::new(1, 21, 9),
        ] {
            assert!(lit(release), "{release:?}");
        }
    }

    #[test]
    fn a_client_applies_block_effects_only_to_what_it_controls_from_1_21_2() {
        // !level.isClientSide() || isControlledByLocalInstance() (1.21.2 LivingEntity.java:2817),
        // isLocalInstanceAuthoritative() from 1.21.5 (1.21.5 :2755, 26.3 :3064)
        let world = cauldron_world(BlockEffect::LavaCauldron);
        let inside = SimulationState {
            position: Vec3::new(0.5, 0.25, 0.5),
            half_width: 0.3,
            height: 1.8,
            no_gravity: true,
            client_side: true,
            ..SimulationState::default()
        };
        let lit = |controlled: bool, release: MinecraftVersion| {
            let mut state = SimulationState { controlled_by_local_instance: controlled, ..inside };
            tick_release(&mut state, &world, release);
            state.remaining_fire_ticks.unwrap_or(0) > 0
        };
        for release in [
            MinecraftVersion::new(1, 21, 2),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 3, 0),
        ] {
            assert!(lit(true, release), "{release:?}");
            assert!(!lit(false, release), "{release:?}");
        }
    }

    #[test]
    fn a_water_cauldron_puts_a_burning_entity_out() {
        // LayeredCauldronBlock.entityInside extinguishes a burning entity in its water
        // (1.17 LayeredCauldronBlock.java:51-58); 1.21.5 - 1.21.8 clearFire at once (1.21.5
        // LayeredCauldronBlock.java:64-71); EXTINGUISH from 1.21.9 (1.21.9
        // LayeredCauldronBlock.java:67-79)
        let world = cauldron_world(BlockEffect::LayeredCauldron(3));
        for release in [
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.25, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                remaining_fire_ticks: Some(100),
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert_eq!(state.remaining_fire_ticks, Some(-20), "{release:?}");
        }
    }

    #[test]
    fn a_honey_slide_on_the_way_ends_the_fall_a_later_powder_snow_reads_from_1_21_5() {
        // checkInsideBlocks visits the cells in order and each entityInside runs at once: the
        // honey block the falling box slides down beside resets the fall (HoneyBlock.entityInside,
        // doSlideMovement, 1.21.11 HoneyBlock.java:61-70, 106-115), so the powder snow below gives
        // the full block for its inside shape rather than FALLING_COLLISION_SHAPE (1.21.11
        // PowderSnowBlock.java:92-106) and the entity sticks
        let mut world = powder_snow_world();
        let without_honey = world.clone();
        world.set_block(
            (1, 1, 0),
            effect_block(
                BlockEffect::Honey,
                Aabb::new(
                    Vec3::new(0.0625, 0.0, 0.0625),
                    Vec3::new(0.9375, 0.9375, 0.9375),
                ),
            ),
        );
        let fall = |world: &WorldSnapshot, release| {
            let mut state = SimulationState {
                position: Vec3::new(0.75, 3.5, 0.5),
                velocity: Vec3::new(0.0, -2.55, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                fall_distance: 3.0,
                ..SimulationState::default()
            };
            tick_release(&mut state, world, release);
            assert!((state.position.y - 0.95).abs() < 1.0e-9, "{release:?}");
            state
        };
        for release in [MinecraftVersion::new(1, 21, 5), MinecraftVersion::new(1, 21, 11)] {
            let slid = fall(&world, release);
            assert!(slid.in_powder_snow, "{release:?}");
            assert_eq!(
                slid.stuck_speed_multiplier,
                Vec3::new(java_float(0.9), 1.5, java_float(0.9)),
                "{release:?}"
            );
            let missed = fall(&without_honey, release);
            assert!(!missed.in_powder_snow, "{release:?}");
            assert!(missed.fall_distance > 2.5, "{release:?}");
        }
    }

    #[test]
    fn the_powder_snow_inside_test_takes_the_release_collided_along_vector_inset() {
        // AABB.collidedAlongVector grows the boxes by exactly half the size in 1.21.5 - 1.21.8
        // and by half less 1e-7 from 1.21.9 (1.21.5 AABB.java:392, 1.21.9 :368): a centre that
        // ends 5e-8 inside the grown FALLING_COLLISION_SHAPE counts only before 1.21.9
        let world = powder_snow_world();
        let top = java_float(0.9) + 0.9 - 5.0e-8;
        let fall = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, top - 0.9 + 0.25, 0.5),
                velocity: Vec3::new(0.0, -0.25, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                no_gravity: true,
                fall_distance: 3.0,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.in_powder_snow
        };
        assert!(fall(MinecraftVersion::new(1, 21, 5)));
        assert!(fall(MinecraftVersion::new(1, 21, 8)));
        assert!(!fall(MinecraftVersion::new(1, 21, 9)));
        assert!(!fall(MinecraftVersion::new(26, 3, 0)));
    }

    #[test]
    fn only_the_server_takes_periodic_freeze_damage() {
        // LivingEntity.aiStep hurts a fully frozen entity on the server alone (1.17
        // LivingEntity.java:2554, 1.21.11 :2981-2988, 26.3 :3074-3087)
        let world = fire_world(BlockEffect::PowderSnow);
        for release in [
            MinecraftVersion::new(1, 17, 0),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for client_side in [false, true] {
                let mut state = SimulationState {
                    ticks_frozen: 140,
                    tick_count: 38,
                    client_side,
                    ..standing_in_fire(true)
                };
                // Carry state through both damage boundaries: 39/40/41 and 79/80/81.
                // freeze_damage is an accumulated output, not a per-tick flag.
                for tick_count in 39..=81 {
                    tick_release(&mut state, &world, release);
                    assert_eq!(state.tick_count, tick_count);
                    assert_eq!(
                        state.freeze_damage,
                        if client_side { 0.0 } else { f64::from(tick_count / 40) },
                        "{release:?}, client={client_side}, tick={tick_count}"
                    );
                    assert_eq!(state.ticks_frozen, 140);
                }
            }
        }
    }

    #[test]
    fn the_tick_count_wraps_as_a_java_int_and_keeps_its_freeze_period() {
        let world = fire_world(BlockEffect::PowderSnow);
        let release = MinecraftVersion::new(1, 21, 11);
        let ticked = |tick_count: u32| {
            let mut state = SimulationState {
                ticks_frozen: 140,
                tick_count,
                client_side: false,
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &world, release);
            state
        };
        // -41 ticks to -40: Java's -40 % 40 is 0, the same bits as an unsigned number are not
        let negative = ticked((-41_i32) as u32);
        assert_eq!(negative.tick_count as i32, -40);
        assert_eq!(negative.freeze_damage, 1.0);
        // -39 ticks to -38: Java's remainder keeps the sign and is no 0
        assert_eq!(ticked((-39_i32) as u32).freeze_damage, 0.0);
        // 2147483647 ticks over to -2147483648, and -1 on to 0, which is a damage tick
        assert_eq!(ticked(i32::MAX as u32).tick_count as i32, i32::MIN);
        let wrapped = ticked(u32::MAX);
        assert_eq!(wrapped.tick_count, 0);
        assert_eq!(wrapped.freeze_damage, 1.0);
    }

    #[test]
    fn periodic_freeze_damage_respects_immunity_and_the_counter_after_thawing() {
        // Damage follows aiStep's thawing, so leaving with 140 ticks prevents this
        // tick's damage, but a command-set counter of 300 remains fully frozen.
        // canFreeze independently rejects damage even with an above-limit counter.
        let snow = fire_world(BlockEffect::PowderSnow);
        let air = WorldSnapshot::new();
        for release in [
            MinecraftVersion::new(1, 17, 0),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            for client_side in [false, true] {
                for powder_snow_immune in [false, true] {
                    for ticks_frozen in [140, 300] {
                        for stays_in_snow in [false, true] {
                            let mut state = SimulationState {
                                ticks_frozen,
                                tick_count: 39,
                                client_side,
                                // (the local player: its client applies the block effects)
                                controlled_by_local_instance: client_side,
                                powder_snow_immune,
                                in_powder_snow: true,
                                ..standing_in_fire(true)
                            };
                            tick_release(&mut state, if stays_in_snow { &snow } else { &air }, release);
                            let freezes = stays_in_snow && !powder_snow_immune;
                            let expected_counter = if client_side {
                                if release.at_least(1, 21, 5) && freezes { 140 } else { ticks_frozen }
                            } else if freezes {
                                140
                            } else {
                                ticks_frozen - 2
                            };
                            assert_eq!(state.ticks_frozen, expected_counter,
                                "{release:?}, client={client_side}, immune={powder_snow_immune}, initial={ticks_frozen}, snow={stays_in_snow}");
                            assert_eq!(state.in_powder_snow, stays_in_snow);
                            let takes_damage = !client_side && !powder_snow_immune && expected_counter >= 140;
                            assert_eq!(state.freeze_damage, if takes_damage { 1.0 } else { 0.0 },
                                "{release:?}, client={client_side}, immune={powder_snow_immune}, initial={ticks_frozen}, snow={stays_in_snow}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn client_inside_block_freeze_still_reaches_the_threshold_without_damage() {
        // From 1.21.5 InsideBlockEffectType.FREEZE increments on both sides;
        // LivingEntity.aiStep's periodic damage remains server-only. Before that
        // boundary the client retains its synchronized counter even inside snow.
        let snow = fire_world(BlockEffect::PowderSnow);
        for release in [
            MinecraftVersion::new(1, 17, 0),
            MinecraftVersion::new(1, 21, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut state = SimulationState {
                client_side: true,
                controlled_by_local_instance: true,
                ticks_frozen: 139,
                tick_count: 39,
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &snow, release);
            let expected_counter = if release.at_least(1, 21, 5) { 140 } else { 139 };
            assert_eq!(state.ticks_frozen, expected_counter, "{release:?}");
            assert!(state.in_powder_snow);
            assert_eq!(state.freeze_damage, 0.0, "{release:?}");
            tick_release(&mut state, &WorldSnapshot::new(), release);
            assert_eq!(state.ticks_frozen, expected_counter, "{release:?}");
            assert!(!state.in_powder_snow);
            assert_eq!(state.freeze_damage, 0.0, "{release:?}");
        }
    }

    #[test]
    fn a_wet_client_the_server_says_burns_goes_back_to_the_immune_ticks() {
        // Entity.isOnFire reads the shared flag on a client (1.20.4 Entity.java:2127-2130), so the
        // wet test after the move sees it with the ticks cleared (1.20.4 :724-726)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 1, 0),
            BlockState {
                id: 8,
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        // lava in the box: the test before it, for a box with no fire, leaves the ticks alone
        world.set_block(
            (0, 2, 0),
            BlockState {
                id: 10,
                fluid: FluidState::Lava,
                ..BlockState::default()
            },
        );
        let run_at = |release, client_side, on_fire_flag| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.2, 0.5),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                in_water: true,
                no_gravity: true,
                remaining_fire_ticks: Some(0),
                client_side,
                on_fire_flag,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            state.remaining_fire_ticks
        };
        let run = |client_side, on_fire_flag| {
            run_at(MinecraftVersion::new(1, 20, 4), client_side, on_fire_flag)
        };
        // through 1.10.2 the test reads the ticks alone (1.10.2 Entity.java:822, 1.11.2 :875)
        assert_eq!(run_at(MinecraftVersion::new(1, 10, 2), true, true), Some(0));
        assert_eq!(run_at(MinecraftVersion::new(1, 11, 2), true, true), Some(-20));
        assert_eq!(run(true, true), Some(-20));
        assert_eq!(run(true, false), Some(0));
        // the server keeps no such reading: its ticks are all it asks
        assert_eq!(run(false, true), Some(0));
    }

    #[test]
    fn a_dry_client_keeps_its_fire_ticks_from_1_21_6() {
        // the reset to -getFireImmuneTicks: any side in 1.21.5 for an entity that just went out
        // (Entity.java:857-859), the server's alone from 1.21.6 (Entity.java:888-890)
        let air = WorldSnapshot::new();
        for (client_side, ticks) in [(false, -20), (true, 0)] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(0),
                client_side,
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &air, MinecraftVersion::new(1, 21, 6));
            assert_eq!(state.remaining_fire_ticks, Some(ticks));
        }
    }

    #[test]
    fn the_post_move_fire_reset_follows_the_side_and_the_release_for_each_counter() {
        // M23/M25. 1.21.6 Entity.java:888 and 26.3 :1028-1030: the server alone, for anything not
        // burning whose ticks no ignition raised. 1.21.5 :857-859: either side, only for an entity
        // that burned before the move. The client's base tick clears positive ticks first and
        // the server's takes one off.
        let air = WorldSnapshot::new();
        let run = |release, is_player, client_side, ticks, in_rain| {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(ticks),
                client_side,
                in_rain,
                ..standing_in_fire(is_player)
            };
            tick_release(&mut state, &air, release);
            state.remaining_fire_ticks.expect("set by the test")
        };
        for release in [
            MinecraftVersion::new(1, 21, 6),
            MinecraftVersion::new(1, 21, 11),
            MinecraftVersion::new(26, 3, 0),
        ] {
            // Entity.clearFire is setRemainingFireTicks(0) before 1.21.9 and keeps a negative
            // counter from then on
            let negative_client = if release.at_least(1, 21, 9) { -5 } else { 0 };
            for (is_player, reset) in [(true, -20), (false, 0)] {
                for (ticks, server, client) in [
                    (-5, reset, negative_client),
                    (0, reset, 0),
                    (1, reset, 0),
                    (2, 1, 0),
                    (160, 159, 0),
                ] {
                    let at = format!("{release:?} player={is_player} ticks={ticks}");
                    assert_eq!(run(release, is_player, false, ticks, false), server, "{at}");
                    assert_eq!(run(release, is_player, true, ticks, false), client, "{at}");
                }
                assert_eq!(run(release, is_player, false, 5, true), reset, "{release:?}");
                assert_eq!(run(release, is_player, true, 5, true), 0, "{release:?}");
            }
        }
        // the narrower 1.21.5 test: nothing that was not burning after the base tick is reset
        let release = MinecraftVersion::new(1, 21, 5);
        for (is_player, reset) in [(true, -20), (false, -1)] {
            for (ticks, server, client) in
                [(-5, -5, 0), (0, 0, 0), (1, 0, 0), (2, 1, 0), (160, 159, 0)]
            {
                let at = format!("1.21.5 player={is_player} ticks={ticks}");
                assert_eq!(run(release, is_player, false, ticks, false), server, "{at}");
                assert_eq!(run(release, is_player, true, ticks, false), client, "{at}");
            }
            assert_eq!(run(release, is_player, false, 5, true), reset);
            assert_eq!(run(release, is_player, true, 5, true), 0);
        }
    }

    #[test]
    fn ignition_and_extinguishing_in_one_tick_keep_the_was_ignited_test() {
        // Entity.applyEffectsFromBlocks reads the ticks before checkInsideBlocks: an ignition that
        // raised them stops the reset even when the same step puts the fire out (1.21.6
        // Entity.java:874-890, 26.3 :1015-1030). A step applies FIRE_IGNITE before EXTINGUISH.
        let world = WorldSnapshot::new();
        let step = InsideEffectStep {
            fire_ignite: true,
            extinguish: true,
            ..InsideEffectStep::default()
        };
        let effects = InsideEffects {
            steps: vec![step],
            ..InsideEffects::default()
        };
        let run = |release, is_player, client_side, ticks| {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(ticks),
                client_side,
                ..standing_in_fire(is_player)
            };
            apply_inside_fire_effects(&mut state, &world, &[], &effects, release);
            state.remaining_fire_ticks.expect("set by the test")
        };
        for release in [MinecraftVersion::new(1, 21, 6), MinecraftVersion::new(26, 3, 0)] {
            // raised from -5 to -4: no reset on the server, whatever was put out, and clearFire
            // leaves 0 before 1.21.9 and the negative counter from then on
            let raised = if release.at_least(1, 21, 9) { -4 } else { 0 };
            assert_eq!(run(release, true, false, -5), raised, "{release:?}");
            assert_eq!(run(release, true, true, -5), raised, "{release:?}");
            // ignited to 160 and cleared to 0, no higher than before: the server resets
            assert_eq!(run(release, true, false, 0), -20, "{release:?}");
            assert_eq!(run(release, false, false, 0), 0, "{release:?}");
            assert_eq!(run(release, true, true, 0), 0, "{release:?}");
            assert_eq!(run(release, true, false, 5), -20, "{release:?}");
            assert_eq!(run(release, true, true, 5), 0, "{release:?}");
        }
        // in 1.21.5 it needs a fire burning before the move, and the client resets too
        let release = MinecraftVersion::new(1, 21, 5);
        assert_eq!(run(release, true, false, 0), 0);
        assert_eq!(run(release, true, false, 5), -20);
        assert_eq!(run(release, true, true, 5), -20);
        assert_eq!(run(release, false, false, 5), -1);
        assert_eq!(run(release, false, true, 5), -1);
    }

    #[test]
    fn the_generic_fire_delay_carries_through_extinguishing_and_the_next_contact() {
        // M25: 1 to 0 at 1.21.6 (1.21.5 Entity.java:3276-3278, 1.21.6 :3464-3466), the player's
        // override stays 20 and an invulnerable player's ticks stay at most 1.
        let air = WorldSnapshot::new();
        let fire = fire_world(BlockEffect::Fire);
        for (release, is_player, delay) in [
            (MinecraftVersion::new(1, 21, 5), false, -1),
            (MinecraftVersion::new(1, 21, 6), false, 0),
            (MinecraftVersion::new(26, 3, 0), false, 0),
            (MinecraftVersion::new(1, 21, 5), true, -20),
            (MinecraftVersion::new(1, 21, 6), true, -20),
            (MinecraftVersion::new(26, 3, 0), true, -20),
        ] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(5),
                in_rain: true,
                ..standing_in_fire(is_player)
            };
            tick_release(&mut state, &air, release);
            assert_eq!(state.remaining_fire_ticks, Some(delay), "{release:?}");
            state.in_rain = false;
            tick_release(&mut state, &fire, release);
            let expected = if is_player { -19 } else { 160 };
            assert_eq!(state.remaining_fire_ticks, Some(expected), "{release:?}");
        }
        for release in [MinecraftVersion::new(1, 21, 5), MinecraftVersion::new(26, 3, 0)] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(160),
                invulnerable: true,
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &air, release);
            assert_eq!(state.remaining_fire_ticks, Some(1), "{release:?}");
            tick_release(&mut state, &air, release);
            let out = if release.at_least(1, 21, 6) { -20 } else { 0 };
            assert_eq!(state.remaining_fire_ticks, Some(out), "{release:?}");
        }
    }

    #[test]
    fn a_later_fire_ignition_adds_ticks_for_a_server_player_alone() {
        // BaseFireBlock.fireIgnite: negative ticks +1 for all, else ServerPlayer nextInt(1, 3)
        // (1.21.5 BaseFireBlock.java:123-137, 26.3 :122-136); the eight seconds are 160. The
        // block's touch follows a base tick that takes the 160 to 159.
        let world = fire_world(BlockEffect::Fire);
        let release = MinecraftVersion::new(1, 21, 4);
        let burning = |is_player, server_fire_increment| SimulationState {
            remaining_fire_ticks: Some(160),
            server_fire_increment,
            ..standing_in_fire(is_player)
        };
        for (is_player, increment, ticks) in [
            (true, None, 160),
            (true, Some(1), 160),
            (true, Some(2), 161),
            (false, Some(2), 160),
        ] {
            let mut state = burning(is_player, increment);
            tick_release(&mut state, &world, release);
            assert_eq!(state.remaining_fire_ticks, Some(ticks));
        }
        // two FIRE_IGNITE steps of one tick: the second adds to a ServerPlayer's, not a LocalPlayer's
        let release = MinecraftVersion::new(26, 3, 0);
        let step = InsideEffectStep {
            fire_ignite: true,
            ..InsideEffectStep::default()
        };
        let effects = InsideEffects {
            steps: vec![step, step],
            ..InsideEffects::default()
        };
        for (client_side, ticks) in [(true, 160), (false, 162)] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(0),
                server_fire_increment: Some(2),
                client_side,
                ..standing_in_fire(true)
            };
            apply_inside_fire_effects(&mut state, &world, &[], &effects, release);
            assert_eq!(state.remaining_fire_ticks, Some(ticks));
        }
    }

    #[test]
    fn each_server_player_ignition_reads_its_own_draw_in_order() {
        // BaseFireBlock.fireIgnite calls nextInt(1, 3) once per eligible ignition (1.21.5
        // BaseFireBlock.java:123-137, 26.3 :122-136), so no fixed value gives 203 from 200 and
        // 1, 2. A step applies FIRE_IGNITE once, however many fire blocks it touches.
        let world = WorldSnapshot::new();
        let release = MinecraftVersion::new(26, 3, 0);
        let step = InsideEffectStep {
            fire_ignite: true,
            ..InsideEffectStep::default()
        };
        let two_steps = InsideEffects {
            steps: vec![step, step],
            ..InsideEffects::default()
        };
        let run = |start: i32, draws: &[i32], change: fn(&mut SimulationState)| {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(start),
                server_fire_draws: ServerFireDraws::new(draws),
                ..standing_in_fire(true)
            };
            change(&mut state);
            apply_inside_fire_effects(&mut state, &world, &[], &two_steps, release);
            (
                state.remaining_fire_ticks.expect("set by the test"),
                state.server_fire_draws.expect("given").to_vec(),
                state.server_fire_draws_exhausted,
            )
        };
        let none: fn(&mut SimulationState) = |_| {};
        assert_eq!(run(200, &[1, 2, 2], none), (203, vec![2], false));
        // the first draw counts although the eight seconds overwrite what it added
        assert_eq!(run(0, &[1, 2], none), (162, vec![], false));
        assert_eq!(run(0, &[2, 1], none), (161, vec![], false));
        // from -1 the first ignition is the deterministic +1 and draws nothing
        assert_eq!(run(-1, &[2, 1], none), (162, vec![1], false));
        assert_eq!(run(-5, &[2], none), (-3, vec![2], false));
        // nothing is read by a client, a generic entity or a fire-immune player
        assert_eq!(run(200, &[2, 2], |state| state.client_side = true), (200, vec![2, 2], false));
        assert_eq!(run(200, &[2, 2], |state| state.is_player = false), (200, vec![2, 2], false));
        // (not on fire, so the server's post-move reset still runs)
        assert_eq!(run(200, &[2, 2], |state| state.fire_immune = true), (-20, vec![2, 2], false));
        // an invulnerable player's clamp hides the draw, not its consumption
        assert_eq!(run(0, &[2, 2, 1], |state| state.invulnerable = true), (1, vec![1], false));
        // running out is reported, never a quiet repeat
        assert_eq!(run(200, &[2], none), (203, vec![], true));
    }

    #[test]
    fn fire_ignition_draws_start_at_1_21_2_and_are_decided_at_each_ignition() {
        // 1.21.1 adds one for every entity and draws nothing; 1.21.2 is the first with nextInt
        let world = fire_world(BlockEffect::Fire);
        for (release, ticks, rest) in [
            (MinecraftVersion::new(1, 21, 1), 201, vec![2, 1]),
            (MinecraftVersion::new(1, 21, 2), 201, vec![1]),
            (MinecraftVersion::new(1, 21, 4), 201, vec![1]),
        ] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(201),
                server_fire_draws: ServerFireDraws::new(&[2, 1]),
                ..standing_in_fire(true)
            };
            tick_release(&mut state, &world, release);
            let expected = if release.at_least(1, 21, 2) { ticks + 1 } else { ticks };
            assert_eq!(state.remaining_fire_ticks, Some(expected), "{release:?}");
            assert_eq!(state.server_fire_draws.expect("given").to_vec(), rest, "{release:?}");
        }
        // a step between two ignitions puts the fire out: the next is decided on the 0 it left
        let release = MinecraftVersion::new(26, 3, 0);
        let ignite = InsideEffectStep {
            fire_ignite: true,
            ..InsideEffectStep::default()
        };
        let extinguish = InsideEffectStep {
            extinguish: true,
            ..InsideEffectStep::default()
        };
        let effects = InsideEffects {
            steps: vec![ignite, extinguish, ignite],
            ..InsideEffects::default()
        };
        let mut state = SimulationState {
            remaining_fire_ticks: Some(200),
            server_fire_draws: ServerFireDraws::new(&[1, 2]),
            ..standing_in_fire(true)
        };
        apply_inside_fire_effects(&mut state, &WorldSnapshot::new(), &[], &effects, release);
        assert_eq!(state.remaining_fire_ticks, Some(160));
        assert_eq!(state.server_fire_draws.expect("given").to_vec(), Vec::<i32>::new());
        // a burning-time attribute changes the eight seconds, not the reading of the draws
        let mut state = SimulationState {
            remaining_fire_ticks: Some(0),
            burning_time: Some(0.5),
            server_fire_draws: ServerFireDraws::new(&[1, 2, 1]),
            ..standing_in_fire(true)
        };
        let effects = InsideEffects {
            steps: vec![ignite],
            ..InsideEffects::default()
        };
        apply_inside_fire_effects(&mut state, &WorldSnapshot::new(), &[], &effects, release);
        assert_eq!(state.remaining_fire_ticks, Some(80));
        assert_eq!(state.server_fire_draws.expect("given").to_vec(), vec![2, 1]);
    }

    #[test]
    fn a_fire_draw_stream_keeps_its_order_up_to_its_capacity() {
        let draws: Vec<i32> = (0..ServerFireDraws::CAPACITY).map(|at| 1 + (at % 3 == 0) as i32).collect();
        assert_eq!(ServerFireDraws::new(&draws).expect("it fits").to_vec(), draws);
        let mut over = draws;
        over.push(1);
        assert_eq!(ServerFireDraws::new(&over), None);
        assert_eq!(ServerFireDraws::new(&[0]), None);
        assert_eq!(ServerFireDraws::new(&[3]), None);
    }

    #[test]
    fn a_fresh_entitys_fire_ticks_and_the_generic_reset_follow_the_release() {
        // -getFireImmuneTicks up to 1.21.5 (Entity.java:212), the field's 0 from 1.21.6 (:234); the
        // generic reset value goes from 1 to 0 at 1.21.6 (1.21.5 Entity.java:3276-3278, 1.21.6
        // :3464-3466), a player's stays 20
        let air = WorldSnapshot::new();
        let fire = fire_world(BlockEffect::Fire);
        for (release, is_player, reset) in [
            (MinecraftVersion::new(1, 21, 6), false, 0),
            (MinecraftVersion::new(1, 21, 6), true, -20),
            (MinecraftVersion::new(26, 3, 0), false, 0),
            (MinecraftVersion::new(26, 3, 0), true, -20),
        ] {
            let mut state = SimulationState {
                remaining_fire_ticks: Some(0),
                ..standing_in_fire(is_player)
            };
            tick_release(&mut state, &air, release);
            assert_eq!(state.remaining_fire_ticks, Some(reset));
        }
        let mut mob = standing_in_fire(false);
        tick_release(&mut mob, &fire, MinecraftVersion::new(1, 21, 5));
        assert_eq!(mob.remaining_fire_ticks, Some(160));
        let mut player = standing_in_fire(true);
        tick_release(&mut player, &fire, MinecraftVersion::new(1, 21, 5));
        assert_eq!(player.remaining_fire_ticks, Some(-19));
        // one touch ignites a local player from the field's 0
        let mut local = SimulationState {
            client_side: true,
            controlled_by_local_instance: true,
            ..standing_in_fire(true)
        };
        tick_release(&mut local, &fire, MinecraftVersion::new(1, 21, 9));
        assert_eq!(local.remaining_fire_ticks, Some(160));
    }

    #[test]
    fn a_dead_or_sleeping_player_neither_walks_nor_jumps() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        let resting = |dead, sleeping| SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            on_ground: true,
            input: Vec3::new(0.0, 0.0, 1.0),
            jump_requested: true,
            dead,
            sleeping,
            ..SimulationState::default()
        };
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let mut alive = resting(false, false);
            tick_release(&mut alive, &world, release);
            assert!(alive.position.y > 1.4 && alive.position.z > 0.5);
            assert_eq!(alive.jump_ticks, 10);
            for (dead, sleeping) in [(true, false), (false, true)] {
                let mut state = resting(dead, sleeping);
                state.jump_ticks = 5;
                tick_release(&mut state, &world, release);
                assert_eq!(state.position, Vec3::new(0.5, 1.0, 0.5), "{release:?}");
                // aiStep's noJumpDelay = 0 when not jumping (26.3 LivingEntity.java:3035)
                assert_eq!(state.jump_ticks, 0);
            }
        }
    }

    #[test]
    fn an_immobile_entity_keeps_its_motion_and_gravity() {
        // only the input goes: travel still runs (1.20.4 LivingEntity.java:2563-2566)
        let mut state = SimulationState {
            position: Vec3::new(0.5, 20.0, 0.5),
            velocity: Vec3::new(0.2, 0.0, 0.0),
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            dead: true,
            input: Vec3::new(0.0, 0.0, 1.0),
            ..SimulationState::default()
        };
        tick_release(&mut state, &WorldSnapshot::new(), MinecraftVersion::new(1, 20, 4));
        assert_eq!(state.position.x, 0.7);
        assert_eq!(state.position.z, 0.5);
        assert!(state.velocity.y < 0.0);
        // a mob that sleeps is not immobile (LivingEntity.isImmobile is isDead alone)
        let mut mob = SimulationState {
            position: Vec3::new(0.5, 20.0, 0.5),
            half_width: 0.3,
            height: 1.8,
            sleeping: true,
            input: Vec3::new(0.0, 0.0, 1.0),
            ..SimulationState::default()
        };
        tick_release(&mut mob, &WorldSnapshot::new(), MinecraftVersion::new(1, 20, 4));
        assert!(mob.position.z > 0.5);
    }

    /// A stone floor at y 0 around the origin.
    fn immobile_floor() -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        for x in -4..=8 {
            for z in -4..=8 {
                world.set_block(
                    (x, 0, z),
                    BlockState {
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
            }
        }
        world
    }

    const IMMOBILE_RELEASES: [MinecraftVersion; 5] = [
        MinecraftVersion::new(1, 8, 8),
        MinecraftVersion::new(1, 13, 2),
        MinecraftVersion::new(1, 20, 4),
        MinecraftVersion::new(1, 21, 5),
        MinecraftVersion::new(26, 3, 0),
    ];

    fn immobile_player(input: Vec3, jump: bool, sprinting: bool, velocity: Vec3) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, 1.0, 0.5),
            velocity,
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            on_ground: velocity.y <= 0.0,
            input,
            jump_requested: jump,
            sprinting,
            ..SimulationState::default()
        }
    }

    #[test]
    fn immobile_controls_add_nothing_to_the_motion_an_external_push_left() {
        // the input goes and nothing else (1.8.8 LivingEntity.java:1427-1432, 1.20.4 :2563-2566,
        // 26.3 :3000-3003): an immobile trace is the trace of the same player with no keys down,
        // here over forward, diagonal and jump, with a sprint held, from rest and from a knockback
        // (LivingEntity.knockback only writes the delta movement)
        let world = immobile_floor();
        let controls = [
            (Vec3::new(0.0, 0.0, 1.0), false),
            (Vec3::new(1.0, 0.0, 1.0), false),
            (Vec3::new(-1.0, 0.0, -1.0), true),
            (Vec3::ZERO, true),
        ];
        let motions = [
            Vec3::ZERO,
            Vec3::new(0.2, -0.0784, 0.1),
            Vec3::new(0.0, 0.3, 0.0),
            Vec3::new(0.1, 0.4, -0.1),
        ];
        for release in IMMOBILE_RELEASES {
            for motion in motions {
                for sprinting in [false, true] {
                    let mut idle = immobile_player(Vec3::ZERO, false, sprinting, motion);
                    let mut trace = Vec::new();
                    for _ in 0..4 {
                        tick_release(&mut idle, &world, release);
                        trace.push((idle.position, idle.velocity, idle.on_ground));
                    }
                    for (input, jump) in controls {
                        for (dead, sleeping) in [(true, false), (false, true)] {
                            let mut held = immobile_player(input, jump, sprinting, motion);
                            held.dead = dead;
                            held.sleeping = sleeping;
                            for expected in &trace {
                                tick_release(&mut held, &world, release);
                                assert_eq!(
                                    (held.position, held.velocity, held.on_ground),
                                    *expected,
                                    "{release:?} {input:?} {jump} {motion:?} {sprinting}"
                                );
                            }
                        }
                        // and the same keys do move a player that is neither (a player already
                        // rising from the push is off the ground: the jump alone does nothing)
                        if input != Vec3::ZERO || motion.y <= 0.0 {
                            let mut alive = immobile_player(input, jump, sprinting, motion);
                            tick_release(&mut alive, &world, release);
                            assert_ne!(alive.position, trace[0].0, "{release:?} {input:?} {jump}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn immobile_motion_advances_by_the_source_drag_and_gravity() {
        // a knockback in the air: the move takes the whole delta, then 0.91F air friction on x and
        // z and (y - 0.08) * 0.98F on y (1.20.4 LivingEntity.java travel, 26.3 :2382-2415 travelInAir)
        let air = WorldSnapshot::new();
        for release in [MinecraftVersion::new(1, 20, 4), MinecraftVersion::new(26, 3, 0)] {
            for (dead, sleeping) in [(true, false), (false, true)] {
                let mut state = immobile_player(
                    Vec3::new(1.0, 0.0, 1.0),
                    true,
                    false,
                    Vec3::new(0.1, 0.4, -0.2),
                );
                state.position.y = 20.0;
                state.dead = dead;
                state.sleeping = sleeping;
                tick_release(&mut state, &air, release);
                assert_eq!(state.position, Vec3::new(0.6, 20.4, 0.3), "{release:?}");
                let friction = f64::from(0.91_f32);
                assert_eq!(state.velocity.x, 0.1 * friction, "{release:?}");
                assert_eq!(state.velocity.z, -0.2 * friction, "{release:?}");
                let expected_y = (0.4 - 0.08) * f64::from(0.98_f32);
                assert_eq!(state.velocity.y, expected_y, "{release:?}");
            }
        }
    }

    #[test]
    fn immobile_jump_in_water_and_on_a_ladder_is_no_jump() {
        // jumping cleared comes before aiStep's fluid jump (26.3 LivingEntity.java:3012-3031) and
        // the ladder's climb, which reads it after the move (from 1.14; before it, only a wall)
        let mut water = WorldSnapshot::new();
        // from 26.1 the fluid scan also needs the chunks under its one-cell margin
        for chunk_x in -1..=0 {
            for chunk_z in -1..=0 {
                water.set_chunk_loaded(chunk_x, chunk_z, true);
            }
        }
        for y in 1..=4 {
            water.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
        }
        let air = WorldSnapshot::new();
        for release in IMMOBILE_RELEASES {
            for (world, y) in [(&water, 2.0), (&air, 20.0)] {
                let run = |jump: bool, dead: bool| {
                    let mut state = immobile_player(Vec3::ZERO, jump, false, Vec3::ZERO);
                    state.position.y = y;
                    state.on_ground = false;
                    state.climbing = y == 20.0;
                    state.dead = dead;
                    tick_release(&mut state, world, release);
                    state.velocity
                };
                assert_eq!(run(true, true), run(false, false), "{release:?} y {y}");
                if release.at_least(1, 14, 0) || y == 2.0 {
                    assert_ne!(run(true, false), run(false, false), "{release:?} y {y}");
                }
            }
        }
    }

    #[test]
    fn an_immobile_tick_releases_the_jump_cooldown_and_the_next_can_jump() {
        // aiStep's noJumpDelay = 0 when not jumping (26.3 LivingEntity.java:3035): a sleep ends a
        // cooldown a held jump would have kept. The player stands as gravity leaves it: a move
        // with no motion down meets no floor and leaves the ground.
        let world = immobile_floor();
        let standing = Vec3::new(0.0, (0.0 - 0.08) * f64::from(0.98_f32), 0.0);
        for release in IMMOBILE_RELEASES {
            for (dead, sleeping) in [(true, false), (false, true)] {
                let start = || {
                    let mut state = immobile_player(Vec3::ZERO, true, false, standing);
                    state.jump_ticks = 10;
                    state
                };
                let mut awake = start();
                tick_release(&mut awake, &world, release);
                assert_eq!(awake.jump_ticks, 9);
                assert_eq!(awake.position.y, 1.0);
                let mut resting = start();
                resting.dead = dead;
                resting.sleeping = sleeping;
                tick_release(&mut resting, &world, release);
                assert_eq!(resting.jump_ticks, 0);
                assert_eq!(resting.position.y, 1.0);
                // the first tick awake jumps at once, as a player with no cooldown does
                resting.dead = false;
                resting.sleeping = false;
                let mut fresh = immobile_player(Vec3::ZERO, true, false, standing);
                fresh.velocity = resting.velocity;
                tick_release(&mut resting, &world, release);
                tick_release(&mut fresh, &world, release);
                assert!(resting.position.y > 1.4, "{release:?}");
                assert_eq!(resting.position, fresh.position);
                assert_eq!(resting.velocity, fresh.velocity);
                assert_eq!(resting.jump_ticks, 10);
            }
        }
    }

    #[test]
    fn an_immobile_state_keeps_the_controls_it_was_asked_for() {
        // jumping, xxa and zza are the entity's, the keys stay LocalPlayer.input's (26.3
        // LivingEntity.java:3000-3003, LocalPlayer.java:676): a state passed back after the sleep
        // walks on its first tick awake, as the packed player sent the same keys does
        let world = immobile_floor();
        for release in IMMOBILE_RELEASES {
            let keys = Vec3::new(1.0, 0.0, 1.0);
            let standing = Vec3::new(0.0, (0.0 - 0.08) * f64::from(0.98_f32), 0.0);
            let mut state = immobile_player(keys, true, false, standing);
            state.sleeping = true;
            tick_release(&mut state, &world, release);
            assert_eq!(state.input, keys, "{release:?}");
            assert!(state.jump_requested, "{release:?}");
            assert_eq!(state.position, Vec3::new(0.5, 1.0, 0.5));
            state.sleeping = false;
            tick_release(&mut state, &world, release);
            assert!(state.position.y > 1.4 && state.position.z > 0.5, "{release:?}");
        }
    }

    #[test]
    fn a_pushed_sleeper_arms_the_auto_jump_its_keys_ask_for() {
        // canAutoJump's isMoving and updateAutoJump read LocalPlayer.input, which an immobile tick
        // leaves alone (1.20.4 LocalPlayer.java:884, 891-892, 1006-1016; 26.3
        // LocalPlayer.java:995-1003, 1109-1114): pushed at a block with a key down the jump is armed,
        // aiStep clears it while immobile, and a tick awake takes it
        let mut world = immobile_floor();
        world.set_block(
            (0, 1, 1),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        for release in [
            MinecraftVersion::new(1, 13, 2),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 3, 0),
        ] {
            let pushed = |keys: Vec3| {
                let mut state = immobile_player(keys, false, false, Vec3::new(0.0, -0.0784, 0.05));
                state.auto_jump = true;
                state.sleeping = true;
                tick_release(&mut state, &world, release);
                state
            };
            assert_eq!(pushed(Vec3::ZERO).auto_jump_time, 0, "{release:?}");
            let mut armed = pushed(Vec3::new(0.0, 0.0, 1.0));
            assert_eq!(armed.auto_jump_time, 1, "{release:?}");
            // still asleep: spent, no jump
            let mut asleep = armed;
            asleep.input = Vec3::ZERO;
            tick_release(&mut asleep, &world, release);
            assert_eq!(asleep.auto_jump_time, 0, "{release:?}");
            assert_eq!(asleep.position.y, 1.0, "{release:?}");
            // awake: it jumps with no jump key
            armed.sleeping = false;
            armed.input = Vec3::ZERO;
            tick_release(&mut armed, &world, release);
            assert!(armed.position.y > 1.4, "{release:?}");
        }
    }

    fn rider(velocity: Vec3) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, 3.5, 0.5),
            velocity,
            half_width: 0.3,
            height: 1.8,
            is_player: true,
            passenger: true,
            input: Vec3::new(1.0, 0.0, 1.0),
            jump_requested: true,
            sprinting: true,
            ..SimulationState::default()
        }
    }

    const RIDE_RELEASES: [MinecraftVersion; 8] = [
        MinecraftVersion::new(1, 8, 8),
        MinecraftVersion::new(1, 12, 2),
        MinecraftVersion::new(1, 16, 5),
        MinecraftVersion::new(1, 20, 4),
        MinecraftVersion::new(1, 21, 3),
        MinecraftVersion::new(1, 21, 4),
        MinecraftVersion::new(1, 21, 11),
        MinecraftVersion::new(26, 3, 0),
    ];

    #[test]
    fn a_passenger_tick_is_a_ride_tick_not_ordinary_travel() {
        // Entity.rideTick zeroes the motion, ticks, and the vehicle's positionRider puts the rider
        // back; LivingEntity.rideTick resets the fall distance (1.8.8 Entity.java:1316-1325,
        // LivingEntity.java:1508-1513; 1.12.2 Entity.java:1629-1639, LivingEntity.java:1939-1944;
        // 1.20.4 Entity.java:1850-1854; 26.3 Entity.java:2465-2470, LivingEntity.java:3233-3236)
        let world = immobile_floor();
        for release in RIDE_RELEASES {
            let mut state = rider(Vec3::new(0.7, 0.9, -0.4));
            state.fall_distance = 7.0;
            let result = tick_release(&mut state, &world, release);
            assert_eq!(state.position, Vec3::new(0.5, 3.5, 0.5), "{release:?}");
            assert_eq!(result.displacement, Vec3::ZERO, "{release:?}");
            assert_eq!(state.fall_distance, 0.0, "{release:?}");
            // the motion it came with is gone: what is left is one air travel of its keys from
            // rest (the rider's own aiStep runs): gravity (0 - 0.08) * 0.98F and the air's input
            assert_eq!(state.velocity.y, -0.08 * f64::from(0.98_f32), "{release:?}");
            assert!(state.velocity.x.abs() < 0.05 && state.velocity.z.abs() < 0.05, "{release:?}");
            // a walker with the same keys does go
            let mut walker = rider(Vec3::ZERO);
            walker.passenger = false;
            tick_release(&mut walker, &world, release);
            assert_ne!(walker.position, Vec3::new(0.5, 3.5, 0.5), "{release:?}");
        }
    }

    #[test]
    fn a_ride_tick_ends_on_the_seat_the_caller_moves() {
        // positionRider follows the vehicle, which the engine does not know: each tick the seat is
        // what the caller hands in, and dismounted the same state walks on from where it sat with
        // the motion the last ride tick left (Entity.removeVehicle leaves it, 26.3 Entity.java:2568-2579)
        let world = immobile_floor();
        for release in RIDE_RELEASES {
            let mut state = rider(Vec3::ZERO);
            for seat in [Vec3::new(1.5, 3.5, 0.5), Vec3::new(2.0, 3.0, 1.0)] {
                state.position = seat;
                tick_release(&mut state, &world, release);
                assert_eq!(state.position, seat, "{release:?}");
            }
            state.passenger = false;
            let left = state.velocity;
            tick_release(&mut state, &world, release);
            assert_eq!(state.position.y, 3.0 + left.y, "{release:?}");
        }
    }

    #[test]
    fn a_passenger_keeps_the_ground_only_before_1_21_4() {
        // Player.tick: isSpectator() alone to 1.21.3, || isPassenger from 1.21.4 (1.21.3
        // Player.java:231, 1.21.4 :234-235, 26.3 Player.java:219-221): a rider on the ground
        // can jump in its own tick before it, not after
        let world = immobile_floor();
        let run = |release: MinecraftVersion| {
            let mut state = rider(Vec3::ZERO);
            state.position.y = 1.0;
            state.on_ground = true;
            state.sprinting = false;
            state.input = Vec3::ZERO;
            tick_release(&mut state, &world, release);
            assert_eq!(state.position.y, 1.0, "{release:?}");
            state.jump_ticks
        };
        assert_eq!(run(MinecraftVersion::new(1, 20, 4)), 10);
        assert_eq!(run(MinecraftVersion::new(1, 21, 3)), 10);
        assert_eq!(run(MinecraftVersion::new(1, 21, 4)), 0);
        assert_eq!(run(MinecraftVersion::new(26, 3, 0)), 0);
    }

    #[test]
    fn a_flying_passenger_takes_the_plain_travel() {
        // Player.travel's flight is !isPassenger (1.8.8 Player.java:1296, 1.12.2 :1406, 1.20.4
        // :1452, 26.3 Player.java:1294-1296): the jump key's impulse is LocalPlayer.aiStep's, then
        // gravity and drag as ever, and no 0.6
        let air = immobile_floor();
        for release in RIDE_RELEASES {
            let mut state = rider(Vec3::ZERO);
            state.flying = true;
            state.flying_speed = f64::from(0.05_f32);
            state.input = Vec3::ZERO;
            state.sprinting = false;
            tick_release(&mut state, &air, release);
            assert_eq!(state.position, Vec3::new(0.5, 3.5, 0.5), "{release:?}");
            let impulse = f64::from(0.05_f32 * 3.0);
            let expected = (impulse - 0.08) * f64::from(0.98_f32);
            assert!((state.velocity.y - expected).abs() < 1.0e-9, "{release:?}: {}", state.velocity.y);
            assert!(state.flying, "{release:?}");
            // not riding, the 0.6 of its flight
            let mut flying = rider(Vec3::ZERO);
            flying.passenger = false;
            flying.flying = true;
            flying.flying_speed = f64::from(0.05_f32);
            flying.input = Vec3::ZERO;
            flying.sprinting = false;
            tick_release(&mut flying, &air, release);
            assert!((flying.velocity.y - impulse * 0.6).abs() < 1.0e-9, "{release:?}: {}", flying.velocity.y);
        }
    }

    #[test]
    fn a_passenger_is_pushed_out_of_blocks_until_26_3() {
        // !isPassenger in the aiStep's moveTowardsClosestSpace from 26.3 (26.2 LocalPlayer.java:753,
        // 26.3 :774); its push is the motion it leaves, the seat stays either way
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 3, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                suffocating: Some(true),
                ..BlockState::default()
            },
        );
        let run = |release: MinecraftVersion| {
            let mut state = rider(Vec3::ZERO);
            state.position = Vec3::new(0.9, 3.0, 0.5);
            state.input = Vec3::ZERO;
            state.jump_requested = false;
            state.sprinting = false;
            tick_release(&mut state, &world, release);
            assert_eq!(state.position, Vec3::new(0.9, 3.0, 0.5), "{release:?}");
            state.velocity.x.abs() + state.velocity.z.abs()
        };
        assert_eq!(run(MinecraftVersion::new(26, 3, 0)), 0.0);
        assert_ne!(run(MinecraftVersion::new(26, 2, 0)), 0.0);
    }

    #[test]
    fn a_rocket_before_the_ride_tick_leaves_the_rider_nothing() {
        // the boost comes before the rider's tick and rideTick takes it away: the raw input too
        let air = WorldSnapshot::new();
        for release in RIDE_RELEASES {
            let mut boosted = rider(Vec3::ZERO);
            boosted.firework_boost = Vec3::new(0.3, 0.3, 0.3);
            let mut plain = rider(Vec3::ZERO);
            tick_release(&mut boosted, &air, release);
            tick_release(&mut plain, &air, release);
            assert_eq!(boosted.velocity, plain.velocity, "{release:?}");
            assert_eq!(boosted.position, plain.position, "{release:?}");
        }
    }

    #[test]
    fn the_raw_firework_boost_reaches_the_gravity_choice() {
        // Slow Falling's gravity is chosen on the motion the rocket already changed (26.3
        // LivingEntity.java:2337-2339): vy +0.1, so 0.08: (0.1 - 0.08 * 0.25) * 0.98F
        for version in ["1.13.2", "1.20.4", "1.21.2", "1.21.11", "26.3"] {
            let glide = |velocity_y: f64, boost_y: f64| {
                let mut state = SimulationState {
                    is_player: true,
                    fall_flying: true,
                    slow_falling: true,
                    position: Vec3::new(0.5, 20.0, 0.5),
                    velocity: Vec3::new(0.0, velocity_y, 0.0),
                    firework_boost: Vec3::new(0.0, boost_y, 0.0),
                    look: Vec3::new(0.0, 0.0, 1.0),
                    half_width: 0.3,
                    height: 1.8,
                    controlled_by_local_instance: true,
                    ..SimulationState::default()
                };
                let world = WorldSnapshot::new();
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default())
                    .unwrap();
            state.velocity.y
            };
            let raw = glide(-0.1, 0.2);
            assert!((raw - 0.0784).abs() < 1.0e-7, "{version}: {raw}");
            assert_eq!(raw, glide(0.1, 0.0), "{version}");
        }
    }

    /// One level glide tick of a Slow Falling player, by 26.3 LivingEntity.updateFallFlyingMovement
    /// (:2526-2543): gravity * (-1 + 0.75), the drop turned forward, then 0.98F. The look is level,
    /// so cos(lean)^2 = 1.
    fn source_level_glide_y(velocity_y: f64, gravity: f64) -> f64 {
        let mut y = velocity_y + gravity * (-1.0 + 0.75);
        if y < 0.0 {
            y += y * -0.1;
        }
        y * 0.9800000190734863
    }

    fn raw_glide(version: &str, velocity: Vec3, boost: Vec3, slow_falling: bool) -> SimulationState {
        let mut state = SimulationState {
            is_player: true,
            fall_flying: true,
            slow_falling,
            position: Vec3::new(0.5, 20.0, 0.5),
            velocity,
            firework_boost: boost,
            look: Vec3::new(0.0, 0.0, 1.0),
            half_width: 0.3,
            height: 1.8,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        };
        crate::versioned::dispatch(version, &mut state, &WorldSnapshot::new(), PhysicsConfig::default())
            .unwrap();
        state
    }

    #[test]
    fn the_raw_firework_boost_crosses_zero_either_way_as_a_pre_tick_impulse() {
        // FireworkRocketEntity.tick writes the motion before the player's tick (26.3
        // FireworkRocketEntity.java:123-128) and getEffectiveGravity tests the motion's sign (26.3
        // LivingEntity.java:2337-2339, 1.13.2 :1511-1513): the raw input is that same impulse
        for version in ["1.13.2", "1.20.4", "1.21.2", "1.21.11", "26.3"] {
            for (velocity_y, boost_y, gravity) in [
                // up across zero: no longer falling, the full 0.08 (the ticket's 0.0784000015)
                (-0.1, 0.2, 0.08),
                // down across zero: falling now, Slow Falling's 0.01
                (0.1, -0.2, 0.01),
                // a zero impulse leaves the choice to the motion
                (-0.1, 0.0, 0.01),
                (0.1, 0.0, 0.08),
                // landing on exactly zero is falling (getDeltaMovement().y <= 0.0)
                (-0.25, 0.25, 0.01),
            ] {
                let total = velocity_y + boost_y;
                let raw = raw_glide(version, Vec3::new(0.0, velocity_y, 0.0), Vec3::new(0.0, boost_y, 0.0), true);
                let impulse = raw_glide(version, Vec3::new(0.0, total, 0.0), Vec3::ZERO, true);
                assert_eq!(raw.velocity, impulse.velocity, "{version} {velocity_y} {boost_y}");
                assert_eq!(raw.position, impulse.position, "{version} {velocity_y} {boost_y}");
                let expected = source_level_glide_y(total, gravity);
                assert!((raw.velocity.y - expected).abs() < 1.0e-9, "{version} {velocity_y} {boost_y}: {} against {expected}", raw.velocity.y);
            }
            let ticket = raw_glide(version, Vec3::new(0.0, -0.1, 0.0), Vec3::new(0.0, 0.2, 0.0), true);
            assert!((ticket.velocity.y - 0.0784000015).abs() < 1.0e-9, "{version}");
        }
    }

    #[test]
    fn the_raw_firework_boost_reaches_a_glide_without_slow_falling() {
        // no Slow Falling before 1.13, the impulse's boundary is the same (1.9.4 and 1.12.2
        // LivingEntity.travel read the motion the rocket left, FireworkRocketEntity from 1.11 only)
        for version in ["1.9.4", "1.12.2", "1.20.4"] {
            for (velocity_y, boost_y) in [(-0.1, 0.2), (0.1, -0.2), (0.1, 0.0)] {
                let total = velocity_y + boost_y;
                let raw = raw_glide(version, Vec3::new(0.0, velocity_y, 0.0), Vec3::new(0.0, boost_y, 0.0), false);
                let impulse = raw_glide(version, Vec3::new(0.0, total, 0.0), Vec3::ZERO, false);
                assert_eq!(raw.velocity, impulse.velocity, "{version}");
                let expected = source_level_glide_y(total, 0.08);
                assert!((raw.velocity.y - expected).abs() < 1.0e-9, "{version}: {}", raw.velocity.y);
            }
        }
    }

    #[test]
    fn the_negligible_cut_reads_the_motion_the_raw_firework_boost_left() {
        // the rocket's tick comes before aiStep's 0.003 cut (26.3 LivingEntity.java:2973-2996): the
        // boost can bring a motion into the cut and take one out of it
        for version in ["1.13.2", "1.20.4", "1.21.5", "26.3"] {
            // into it: -0.0025 is cut to 0, which falls: Slow Falling's 0.01
            let into = raw_glide(version, Vec3::new(0.0, -0.1, 0.0), Vec3::new(0.0, 0.0975, 0.0), true);
            let expected = source_level_glide_y(0.0, 0.01);
            assert!((into.velocity.y - expected).abs() < 1.0e-12, "{version}: {}", into.velocity.y);
            // into it from above, the same: a cut after the gravity's choice would have taken 0.08
            let above = raw_glide(version, Vec3::new(0.0, 0.01, 0.0), Vec3::new(0.0, -0.0075, 0.0), true);
            assert_eq!(above.velocity.y, into.velocity.y, "{version}");
            // out of it: 0.001 alone would be cut, 0.011 rises and takes 0.08
            let out = raw_glide(version, Vec3::new(0.0, 0.001, 0.0), Vec3::new(0.0, 0.01, 0.0), true);
            let expected = source_level_glide_y(0.001 + 0.01, 0.08);
            assert!((out.velocity.y - expected).abs() < 1.0e-9, "{version}: {}", out.velocity.y);
            // a horizontal motion: from 1.21.5 the player's cut reads the length, before each axis
            let raw = raw_glide(version, Vec3::new(0.2, -0.1, 0.0), Vec3::new(-0.1985, 0.0, 0.0), true);
            let impulse = raw_glide(version, Vec3::new(0.2 - 0.1985, -0.1, 0.0), Vec3::ZERO, true);
            assert_eq!(raw.velocity, impulse.velocity, "{version}");
            assert_eq!(raw.position, impulse.position, "{version}");
            assert_eq!(raw.position.x, 0.5, "{version}");
        }
    }

    #[test]
    fn the_raw_firework_boost_falls_between_the_scene_rockets() {
        // the rockets the client ticks before the player move it first, the raw input is the last
        // thing before the player's tick, those after it see what the tick left (26.3
        // FireworkRocketEntity.java:123-128: motion + look * 0.1 + (look * 1.5 - motion) * 0.5)
        let rocket = |velocity: Vec3, look: Vec3| {
            velocity
                + Vec3::new(
                    look.x * 0.1 + (look.x * 1.5 - velocity.x) * 0.5,
                    look.y * 0.1 + (look.y * 1.5 - velocity.y) * 0.5,
                    look.z * 0.1 + (look.z * 1.5 - velocity.z) * 0.5,
                )
        };
        let look = Vec3::new(0.0, 0.0, 1.0);
        let start = Vec3::new(0.0, -0.1, 0.4);
        let boost = Vec3::new(0.0, 0.05, 0.0);
        let world = WorldSnapshot::new();
        for (version, release) in [
            ("1.12.2", MinecraftVersion::new(1, 12, 2)),
            ("1.20.4", MinecraftVersion::new(1, 20, 4)),
            ("26.3", MinecraftVersion::new(26, 3, 0)),
        ] {
            let scene = |rockets: (u32, u32), velocity: Vec3, firework_boost: Vec3| {
                let mut state = SimulationState {
                    is_player: true,
                    fall_flying: true,
                    position: Vec3::new(0.5, 20.0, 0.5),
                    velocity,
                    firework_boost,
                    look,
                    half_width: 0.3,
                    height: 1.8,
                    controlled_by_local_instance: true,
                    ..SimulationState::default()
                };
                let mut world = world.clone();
                crate::simulation::tick_local_player_scene(
                    &mut world,
                    &mut state,
                    None,
                    &[],
                    rockets,
                    false,
                    &[],
                    &mut [0.0; 3],
                    PhysicsConfig::default(),
                    release,
                    |state, world| {
                        crate::versioned::release_riptide(version, state, world, PhysicsConfig::default())
                    },
                    |state, world| {
                        crate::versioned::dispatch(version, state, world, PhysicsConfig::default())
                    },
                )
                .unwrap();
                state
            };
            // before the player: rocket, then raw, then the tick
            let before = scene((1, 1), start, boost);
            assert_eq!(before.velocity, scene((0, 0), rocket(start, look) + boost, Vec3::ZERO).velocity, "{version}");
            // after the player: raw, then the tick, then the rocket, once
            let after = scene((1, 0), start, boost);
            let ticked = scene((0, 0), start + boost, Vec3::ZERO);
            assert_eq!(after.velocity, rocket(ticked.velocity, look), "{version}");
            // the raw input is no rocket: with none attached it adds only itself
            assert_eq!(scene((0, 0), start, boost).velocity, ticked.velocity, "{version}");
        }
    }

    #[test]
    fn a_contracted_lava_box_is_reordered_before_1_14() {
        // Entity.isInLava's expand(-0.1F, -0.4F, -0.1F) makes a Box, whose constructor sorts the
        // corners (1.13.2 AABB.java:38-44, Entity.java:1143-1144): an elytra-sized 0.6 high box at
        // y 64.7 spans 64.9..65.1 and reaches the lava in 64 (1.9.4 Player.java:367-368)
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 64, 0),
            BlockState {
                fluid: FluidState::Lava,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let height = f64::from(0.6_f32);
        for release in [MinecraftVersion::new(1, 9, 4), MinecraftVersion::new(1, 13, 2)] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 64.7, 0.5),
                half_width: 0.3,
                height,
                is_player: true,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert!(state.in_lava, "{release:?}");
        }
    }

    fn water_under(ceiling: BlockState) -> WorldSnapshot {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        world.set_block((0, 1, 0), ceiling);
        for (chunk_x, chunk_z) in [(-1, -1), (-1, 0), (0, -1), (0, 0)] {
            world.set_chunk_loaded(chunk_x, chunk_z, true);
        }
        world
    }

    #[test]
    fn the_eyes_take_the_camera_height_of_a_source_under_a_sturdy_block_from_26_3() {
        // 26.3 EntityFluidInteraction.java:84-88 reads getHeightForCamera, 1 here (FluidState.java:
        // 53-61); 26.2 :70-71 the 8/9F of getHeight. The contact stays at getHeight.
        let stone = BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        };
        let body = Aabb::new(Vec3::new(0.46, 0.81, 0.46), Vec3::new(0.54, 0.99, 0.54));
        let eyes = |world: &WorldSnapshot, eye_y: f64, release: MinecraftVersion| {
            world.eyes_in_fluid(FluidState::Water, body, (0.5, eye_y, 0.5), release)
        };
        let sturdy = water_under(stone);
        let new = MinecraftVersion::new(26, 3, 0);
        let old = MinecraftVersion::new(26, 2, 0);
        assert!(eyes(&sturdy, 0.972, new));
        assert!(!eyes(&sturdy, 0.972, old));
        assert!(eyes(&sturdy, 0.8, old));
        assert!(!eyes(&sturdy, 1.0 + 1.0e-9, new));
        // the block's own word beats its boxes: leaves have an empty support shape
        let leaves = water_under(BlockState {
            bottom_face_sturdy: Some(false),
            explosion_resistance: None,
            ..stone
        });
        assert!(!eyes(&leaves, 0.972, new));
        // a bottom face not full is no sturdy one, two halves are
        let half = BlockShape {
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        let mut world = water_under(BlockState::default());
        world.set_block_shapes((0, 1, 0), BlockState::default(), vec![half]);
        assert!(!eyes(&world, 0.972, new));
        let other_half = BlockShape {
            bounds: Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            ..half
        };
        world.set_block_shapes((0, 1, 0), BlockState::default(), vec![half, other_half]);
        assert!(eyes(&world, 0.972, new));
        // a top slab has no bottom face
        world.set_block_shapes(
            (0, 1, 0),
            BlockState::default(),
            vec![BlockShape {
                bounds: Aabb::new(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                ..half
            }],
        );
        assert!(!eyes(&world, 0.972, new));
        // flowing water keeps its height
        let mut flowing = water_under(stone);
        flowing.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_level: 1,
                ..BlockState::default()
            },
        );
        assert!(!eyes(&flowing, 0.972, new));
    }

    /// The fluid `cell` at the origin, under `ceiling`.
    fn fluid_under(cell: BlockState, ceiling: BlockState) -> WorldSnapshot {
        let mut world = water_under(ceiling);
        world.set_block((0, 0, 0), cell);
        world
    }

    fn source_of(fluid: FluidState) -> BlockState {
        BlockState {
            fluid,
            fluid_source: true,
            ..BlockState::default()
        }
    }

    fn solid_ceiling() -> BlockState {
        BlockState {
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        }
    }

    /// Whether eyes at each height in the origin's column are in `fluid`, for a body from 0.5 up to
    /// the ceiling.
    fn eye_heights(
        world: &WorldSnapshot,
        fluid: FluidState,
        heights: &[f64],
        release: MinecraftVersion,
    ) -> Vec<bool> {
        let body = Aabb::new(Vec3::new(0.46, 0.5, 0.46), Vec3::new(0.54, 0.99, 0.54));
        heights
            .iter()
            .map(|&eye_y| world.eyes_in_fluid(fluid, body, (0.5, eye_y, 0.5), release))
            .collect()
    }

    #[test]
    fn the_eyes_end_at_the_camera_height_itself_with_both_fluids() {
        // 26.3 EntityFluidInteraction.java:84-88: eyeY >= fluidBottom and eyeY <= the top for the
        // camera, both ends included; FluidState.java:53-61: 1 for a source under a face sturdy
        // down, else getHeight, 8/9F for a source alone (FlowingFluid.getOwnHeight)
        let top = f64::from(8.0_f32 / 9.0);
        let step = 1.0e-9;
        let new = MinecraftVersion::new(26, 3, 0);
        let old = MinecraftVersion::new(26, 2, 0);
        let heights = [-step, 0.0, top - step, top, top + step, 1.0 - step, 1.0, 1.0 + step];
        let at_8_9 = vec![false, true, true, true, false, false, false, false];
        let at_1 = vec![false, true, true, true, true, true, true, false];
        for fluid in [FluidState::Water, FluidState::Lava] {
            let sturdy = fluid_under(source_of(fluid), solid_ceiling());
            assert_eq!(eye_heights(&sturdy, fluid, &heights, new), at_1, "{fluid:?}");
            assert_eq!(eye_heights(&sturdy, fluid, &heights, old), at_8_9, "{fluid:?}");

            let open = fluid_under(source_of(fluid), BlockState::default());
            assert_eq!(eye_heights(&open, fluid, &heights, new), at_8_9, "{fluid:?}");
            assert_eq!(eye_heights(&open, fluid, &heights, old), at_8_9, "{fluid:?}");

            let unsturdy = fluid_under(
                source_of(fluid),
                BlockState {
                    bottom_face_sturdy: Some(false),
                    explosion_resistance: None,
                    ..solid_ceiling()
                },
            );
            assert_eq!(eye_heights(&unsturdy, fluid, &heights, new), at_8_9, "{fluid:?}");

            // the other fluid's eyes see nothing in the same cells
            let other = if fluid == FluidState::Water {
                FluidState::Lava
            } else {
                FluidState::Water
            };
            assert_eq!(
                eye_heights(&sturdy, other, &[0.5, 1.0], new),
                vec![false, false],
                "{fluid:?}"
            );
        }
        // a waterlogged block's water is a source too (the water query takes it)
        let waterlogged = fluid_under(source_of(FluidState::Waterlogged), solid_ceiling());
        assert_eq!(eye_heights(&waterlogged, FluidState::Water, &heights, new), at_1);
    }

    #[test]
    fn a_flowing_fluid_keeps_its_height_under_a_sturdy_face() {
        // FluidState.getHeightForCamera asks isSource first (26.3 FluidState.java:53-55): amount 7
        // stands 7/9F, a falling cell 8/9F, under stone too
        let step = 1.0e-9;
        for release in [MinecraftVersion::new(26, 2, 0), MinecraftVersion::new(26, 3, 0)] {
            for fluid in [FluidState::Water, FluidState::Lava] {
                let flowing = fluid_under(
                    BlockState {
                        fluid,
                        fluid_level: 1,
                        ..BlockState::default()
                    },
                    solid_ceiling(),
                );
                let top = f64::from(7.0_f32 / 9.0);
                assert_eq!(
                    eye_heights(&flowing, fluid, &[top - step, top, top + step, 1.0], release),
                    vec![true, true, false, false],
                    "{fluid:?}"
                );
                let falling = fluid_under(
                    BlockState {
                        fluid,
                        fluid_falling: true,
                        ..BlockState::default()
                    },
                    solid_ceiling(),
                );
                let top = f64::from(8.0_f32 / 9.0);
                assert_eq!(
                    eye_heights(&falling, fluid, &[top - step, top, top + step, 1.0], release),
                    vec![true, true, false, false],
                    "{fluid:?}"
                );
            }
        }
    }

    #[test]
    fn the_same_fluid_above_tops_a_source_at_1_in_both_releases() {
        // FlowingFluid.getHeight: 1 under the same fluid (26.2 and 26.3 alike), not the camera rule
        let mut world = fluid_under(source_of(FluidState::Water), BlockState::default());
        world.set_block((0, 1, 0), source_of(FluidState::Water));
        for release in [MinecraftVersion::new(26, 2, 0), MinecraftVersion::new(26, 3, 0)] {
            assert_eq!(
                eye_heights(&world, FluidState::Water, &[1.0, 1.0 + 1.0e-9], release),
                vec![true, false]
            );
        }
    }

    #[test]
    fn only_a_full_bottom_face_lifts_the_camera_height() {
        // BlockState.isFaceSturdy(DOWN) with SupportType.FULL is Block.isFaceFull of the support
        // shape: the slice 1.0E-7 in covers the whole face (26.3 BlockBehaviour, VoxelShape
        // getFaceShape): eyes between 8/9F and 1 are in the source under a full face only
        let piece = |min: (f64, f64, f64), max: (f64, f64, f64)| BlockShape {
            bounds: Aabb::new(Vec3::new(min.0, min.1, min.2), Vec3::new(max.0, max.1, max.2)),
            ..BlockShape::full_block()
        };
        let full_plan = |y0, y1| vec![piece((0.0, y0, 0.0), (1.0, y1, 1.0))];
        let cases: [(&str, Vec<BlockShape>, bool); 8] = [
            ("bottom slab", full_plan(0.0, 0.5), true),
            ("carpet", full_plan(0.0, 1.0 / 16.0), true),
            ("top slab", full_plan(0.5, 1.0), false),
            ("raised plate", full_plan(1.0 / 16.0, 0.5), false),
            ("half", vec![piece((0.0, 0.0, 0.0), (0.5, 1.0, 1.0))], false),
            (
                "two halves",
                vec![
                    piece((0.0, 0.0, 0.0), (0.5, 1.0, 1.0)),
                    piece((0.5, 0.0, 0.0), (1.0, 0.5, 1.0)),
                ],
                true,
            ),
            (
                "a ring with a hole",
                vec![
                    piece((0.0, 0.0, 0.0), (0.25, 1.0, 1.0)),
                    piece((0.75, 0.0, 0.0), (1.0, 1.0, 1.0)),
                    piece((0.25, 0.0, 0.0), (0.75, 1.0, 0.25)),
                    piece((0.25, 0.0, 0.75), (0.75, 1.0, 1.0)),
                ],
                false,
            ),
            (
                "a stair's bottom half with its upper step",
                vec![
                    piece((0.0, 0.0, 0.0), (1.0, 0.5, 1.0)),
                    piece((0.5, 0.5, 0.0), (1.0, 1.0, 1.0)),
                ],
                true,
            ),
        ];
        let new = MinecraftVersion::new(26, 3, 0);
        let old = MinecraftVersion::new(26, 2, 0);
        for fluid in [FluidState::Water, FluidState::Lava] {
            for (name, shapes, sturdy) in &cases {
                let mut world = fluid_under(source_of(fluid), BlockState::default());
                world.set_block_shapes((0, 1, 0), BlockState::default(), shapes.clone());
                assert_eq!(world.bottom_face_sturdy((0, 1, 0)), *sturdy, "{name}");
                assert_eq!(
                    eye_heights(&world, fluid, &[0.95, 1.0], new),
                    vec![*sturdy, *sturdy],
                    "{name}"
                );
                assert_eq!(
                    eye_heights(&world, fluid, &[0.95, 1.0], old),
                    vec![false, false],
                    "{name}"
                );
            }
            // the block's own word for a support shape its collision does not show
            let told = fluid_under(
                source_of(fluid),
                BlockState {
                    bottom_face_sturdy: Some(true),
                    explosion_resistance: None,
                    ..BlockState::default()
                },
            );
            assert_eq!(eye_heights(&told, fluid, &[0.95], new), vec![true]);
        }
    }

    #[test]
    fn the_bottom_face_of_collision_boxes_is_the_jars_for_every_shape_of_26_3() {
        // scripts/java-oracle/OracleSturdy.java: `face full n box...` for each distinct collision
        // shape, full by Block.isFaceFull(DOWN). A block's word covers what its support shape adds
        // (leaves, snow, the chorus flower); BlockMapper was checked against every state of the jar
        let text = include_str!("../fixtures/java-oracle-bottom-face-26.3.txt");
        let mut shapes_checked = 0;
        for line in text.lines() {
            let tokens: Vec<&str> = line.split(' ').collect();
            let values: Vec<f64> = tokens[3..].iter().map(|token| token.parse().unwrap()).collect();
            let shapes: Vec<BlockShape> = values
                .chunks(6)
                .map(|at| BlockShape {
                    bounds: Aabb::new(Vec3::new(at[0], at[1], at[2]), Vec3::new(at[3], at[4], at[5])),
                    ..BlockShape::full_block()
                })
                .collect();
            let mut world = WorldSnapshot::new();
            world.set_block_shapes((0, 1, 0), BlockState::default(), shapes);
            assert_eq!(world.bottom_face_sturdy((0, 1, 0)), tokens[1] == "1", "{line}");
            shapes_checked += 1;
        }
        assert!(shapes_checked > 100);
    }

    #[test]
    fn the_camera_height_reaches_only_the_cells_the_ordinary_height_lets_through() {
        // EntityFluidInteraction skips a fluid whose getHeight top is under the box's foot before it
        // reads the camera height (26.3 EntityFluidInteraction.java:73-74, 84): feet above 8/9F
        // have no eyes in a source that reaches 1 for the camera
        let world = fluid_under(source_of(FluidState::Water), solid_ceiling());
        let new = MinecraftVersion::new(26, 3, 0);
        let eyes = |foot: f64| {
            let body = Aabb::new(Vec3::new(0.46, foot, 0.46), Vec3::new(0.54, 0.99, 0.54));
            world.eyes_in_fluid(FluidState::Water, body, (0.5, 0.95, 0.5), new)
        };
        // the query deflates the box by 0.001 (26.3 Entity.getFluidInteractionBox)
        let top = f64::from(8.0_f32 / 9.0);
        assert!(eyes(top - 0.001));
        assert!(!eyes(top - 0.0009));
    }

    #[test]
    fn the_camera_height_leaves_the_tick_but_for_the_eye_flags() {
        // Only the eye test reads getHeightForCamera: the height and the current read getHeight and
        // getFlow (26.3 EntityFluidInteraction.java:73, 91-99), so a player scaled to fit under
        // stone has the same contact and the same move in both releases and both ceilings
        let flow = Vec3::new(0.6, 0.0, 0.8);
        let foot = 0.3;
        let top = f64::from(8.0_f32 / 9.0);
        for fluid in [FluidState::Water, FluidState::Lava] {
            let cell = BlockState {
                fluid_flow: flow,
                ..source_of(fluid)
            };
            let under_stone = fluid_under(cell, solid_ceiling());
            let in_the_open = fluid_under(cell, BlockState::default());
            let player = SimulationState {
                position: Vec3::new(0.5, foot, 0.5),
                half_width: 0.04,
                height: 0.69,
                eye_height: Some(0.66),
                is_player: true,
                controlled_by_local_instance: true,
                client_side: true,
                no_gravity: true,
                ..SimulationState::default()
            };
            let new = MinecraftVersion::new(26, 3, 0);
            let old = MinecraftVersion::new(26, 2, 0);
            let contact = entity_fluid_contact(&player, &under_stone, fluid, new);
            assert!(contact.touching);
            assert_eq!(contact.height, top - foot);
            assert_eq!(contact.flow, flow);
            assert_eq!(contact.flowing_blocks, 1);
            assert_eq!(entity_fluid_contact(&player, &under_stone, fluid, old), contact);
            assert_eq!(entity_fluid_contact(&player, &in_the_open, fluid, new), contact);

            let ticked = |world: &WorldSnapshot, version: &str| {
                let mut state = player;
                crate::versioned::dispatch(version, &mut state, world, PhysicsConfig::default())
                    .unwrap();
                state
            };
            let (lifted, plain) = (ticked(&under_stone, "26.3"), ticked(&under_stone, "26.2"));
            let open = ticked(&in_the_open, "26.3");
            // the native outputs themselves, with no adapter recomputing them
            let eye_flags = |state: &SimulationState| (state.eye_in_water, state.eye_in_lava);
            let is_water = fluid == FluidState::Water;
            assert_eq!(eye_flags(&lifted), (is_water, !is_water), "{fluid:?}");
            assert_eq!(eye_flags(&plain), (false, false), "{fluid:?}");
            assert_eq!(eye_flags(&open), (false, false), "{fluid:?}");
            for other in [&plain, &open] {
                assert_eq!(lifted.position, other.position, "{fluid:?}");
                assert_eq!(lifted.velocity, other.velocity, "{fluid:?}");
                assert_eq!(lifted.in_water, other.in_water, "{fluid:?}");
                assert_eq!(lifted.in_lava, other.in_lava, "{fluid:?}");
            }
        }
    }

    fn boat_fluid_state(underwater: bool) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, 0.2, 0.5),
            half_width: 0.2,
            height: 1.6,
            eye_height: Some(0.4),
            is_player: true,
            controlled_by_local_instance: true,
            client_side: true,
            passenger: true,
            no_gravity: true,
            boat_vehicle: Some(BoatVehicle {
                bounding_box: Aabb::new(Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.0, 3.0, 1.0)),
                underwater,
            }),
            ..SimulationState::default()
        }
    }

    fn boat_fluid_world(fluid: FluidState) -> WorldSnapshot {
        let mut world = fluid_block_world(fluid);
        load_chunks_around_origin(&mut world);
        world.set_block((0, 0, 0), BlockState {
            fluid,
            fluid_source: true,
            fluid_flow: Vec3::new(1.0, 0.0, 0.0),
            ..BlockState::default()
        });
        world
    }

    #[test]
    fn boat_passenger_water_exclusion_changes_at_1_9_and_1_19_3() {
        // 1.8.9 Entity has no boat guard (the registered 1.8.8 source stand-in); 1.9.4 :948
        // skips every boat, 1.19.3 :1080 first permits an underwater boat's passenger.
        let world = boat_fluid_world(FluidState::Water);
        for (version, dry_boat_water, underwater_boat_water) in [
            ("1.8.8", true, true),
            ("1.9.4", false, false),
            ("1.12.2", false, false),
            ("1.13.2", false, false),
            ("1.15.2", false, false),
            ("1.16.1", false, false),
            ("1.19.2", false, false),
            ("1.19.3", false, true),
            ("1.21.11", false, true),
        ] {
            for (underwater, expected) in [(false, dry_boat_water), (true, underwater_boat_water)] {
                let mut state = boat_fluid_state(underwater);
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
                assert_eq!(state.in_water, expected, "{version}, underwater={underwater}");
                assert_eq!(state.velocity.x > 0.0, expected, "{version}: current");
            }
            let mut ordinary = SimulationState { passenger: false, ..boat_fluid_state(false) };
            crate::versioned::dispatch(version, &mut ordinary, &world, PhysicsConfig::default()).unwrap();
            assert!(ordinary.in_water, "{version}: inactive context");
        }
        // The old water exclusion is not a blanket fluid exclusion.
        let mut state = boat_fluid_state(false);
        crate::versioned::dispatch("1.21.11", &mut state, &boat_fluid_world(FluidState::Lava), PhysicsConfig::default()).unwrap();
        assert!(state.in_lava);
        assert!(state.velocity.x > 0.0);
    }

    #[test]
    fn boat_passenger_eyes_keep_their_historical_containment_and_offset_rules() {
        // 1.16.1 updateFluidOnEyes calls AABB.contains (all axes, maxima exclusive),
        // 1.16.2 tests only minY <= eye <= maxY; 1.20.5 removes the eye offset.
        let mut state = SimulationState {
            position: Vec3::new(0.5, 0.0, 0.5),
            eye_height: Some(0.6),
            ..boat_fluid_state(false)
        };
        let sampled_y = java_float(0.6) - f64::from(0.11111111_f32);
        for fluid in [FluidState::Water, FluidState::Lava] {
            let world = boat_fluid_world(fluid);
            let eyes = |state: &SimulationState, version: &str| {
                entity_eyes_in_fluid(state, &world, fluid, MinecraftVersion::parse(version).unwrap())
            };
            // Before 1.16 the guard is unconditional even for an underwater boat.
            let old = boat_fluid_state(true);
            assert!(!eyes(&old, "1.9.4"));
            assert!(!eyes(&old, "1.15.2"));
            state.boat_vehicle.as_mut().unwrap().bounding_box = Aabb::new(
                Vec3::new(2.0, sampled_y - 0.1, 2.0), Vec3::new(3.0, sampled_y + 0.1, 3.0),
            );
            assert!(eyes(&state, "1.16.1"));
            assert!(!eyes(&state, "1.16.2"));
            state.boat_vehicle.as_mut().unwrap().bounding_box = Aabb::new(
                Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.0, sampled_y, 1.0),
            );
            assert!(eyes(&state, "1.16.1"), "exclusive maximum");
            assert!(!eyes(&state, "1.16.2"), "inclusive maximum");
            state.boat_vehicle.as_mut().unwrap().bounding_box = Aabb::new(
                Vec3::new(0.0, sampled_y, 0.0), Vec3::new(1.0, sampled_y + 0.1, 1.0),
            );
            assert!(!eyes(&state, "1.16.1"), "inclusive minimum");
            assert!(!eyes(&state, "1.16.2"), "inclusive minimum");
            state.boat_vehicle.as_mut().unwrap().bounding_box = Aabb::new(
                Vec3::new(0.0, 0.55, 0.0), Vec3::new(1.0, 0.65, 1.0),
            );
            assert!(eyes(&state, "1.20.4"));
            assert!(!eyes(&state, "1.20.5"));
            state.boat_vehicle.as_mut().unwrap().underwater = true;
            assert!(eyes(&state, "1.20.5"));
            state.boat_vehicle.as_mut().unwrap().underwater = false;
        }
    }

    #[test]
    fn boat_passenger_modern_scan_keeps_original_depth_and_deflates_only_once() {
        // 26.1 Entity.getFluidInteractionBox deflates before the boat trims; the tracker
        // still subtracts the original entity minY (26.3 EntityFluidInteraction.java:57,91).
        for version in ["26.1", "26.1.1", "26.1.2", "26.2", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            for fluid in [FluidState::Water, FluidState::Lava] {
                let world = boat_fluid_world(fluid);
                let mut trimmed = SimulationState {
                    position: Vec3::new(0.5, 0.6, 0.5),
                    ..boat_fluid_state(false)
                };
                trimmed.boat_vehicle.as_mut().unwrap().bounding_box.max.y = 0.8;
                let original_bounds = trimmed.bounds();
                let area = trimmed.fluid_interaction_area().unwrap();
                assert_eq!(area.min.y, 0.8);
                assert_eq!(area.max.y, original_bounds.max.y - 0.001);
                assert_eq!(area.min.x, original_bounds.min.x + 0.001);
                let ordinary = SimulationState { boat_vehicle: None, ..trimmed };
                let plain_contact = entity_fluid_contact(&ordinary, &world, fluid, release);
                let contact = entity_fluid_contact(&trimmed, &world, fluid, release);
                assert_eq!(contact, plain_contact, "{version}: same fluid cells and original depth");
                assert_eq!(contact.height, f64::from(8.0_f32 / 9.0) - 0.6);
                assert_eq!(contact.flow.x, contact.height, "shallow current uses original depth");
                assert_eq!(trimmed.bounds(), original_bounds, "query must not resize entity");
                trimmed.boat_vehicle.as_mut().unwrap().bounding_box.max.y = original_bounds.max.y - 0.001;
                assert_eq!(trimmed.fluid_interaction_area(), None, "coverage compares deflated maxY");
                assert_eq!(entity_fluid_contact(&trimmed, &world, fluid, release), Default::default());
                assert!(!entity_eyes_in_fluid(&trimmed, &world, fluid, release));
                trimmed.boat_vehicle.as_mut().unwrap().underwater = true;
                assert_eq!(entity_fluid_contact(&trimmed, &world, fluid, release), plain_contact);
            }
        }
    }

    #[test]
    fn boat_passenger_null_scan_clears_stale_flags_and_dismount_restores_queries() {
        for version in ["26.1", "26.2", "26.3"] {
            let world = boat_fluid_world(FluidState::Water);
            for no_physics in [false, true] {
                let mut state = SimulationState {
                    in_water: true, in_lava: true, eye_in_water: true, eye_in_lava: true,
                    no_physics,
                    ..boat_fluid_state(false)
                };
                let before = state.bounds();
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
                assert!(!state.in_water && !state.in_lava && !state.eye_in_water && !state.eye_in_lava, "{version}");
                assert_eq!(state.velocity, Vec3::ZERO, "null scan applies no current");
                assert_eq!(state.bounds(), before, "fluid exclusion preserves physical bounds");
                state.passenger = false;
                crate::versioned::dispatch(version, &mut state, &world, PhysicsConfig::default()).unwrap();
                assert!(state.in_water && state.eye_in_water, "{version}: dismount");
                assert!(state.velocity.x > 0.0);
            }
        }
    }

    #[test]
    fn boat_passenger_fall_refresh_does_not_restore_excluded_water() {
        // LivingEntity.checkFallDamage rechecks fluids after movement. Passing the vehicle
        // context only to baseTick would let this second query erase the falling distance.
        let world = boat_fluid_world(FluidState::Water);
        for version in ["1.12.2", "1.20.4", "1.21.11", "26.1", "26.3"] {
            let release = MinecraftVersion::parse(version).unwrap();
            let mut state = SimulationState {
                in_water: false, fall_distance: 2.0,
                ..boat_fluid_state(false)
            };
            let before = state.bounds();
            crate::simulation::check_fall_damage(
                &mut state, &world,
                VersionPhysicsRules { release, ..VersionPhysicsRules::default() },
                None, -0.1, false,
                crate::simulation::FallDamageRules {
                    landing_damage_rounding: LandingDamageRounding::Floor,
                    restitution_enabled: release.at_least(26, 2, 0),
                    modern_fall_damage: release.at_least(1, 20, 5),
                },
            );
            assert!(!state.in_water, "{version}: post-move water exclusion");
            assert!(state.fall_distance > 2.0, "{version}: fall not reset by excluded water");
            assert_eq!(state.velocity, Vec3::ZERO, "no post-move current");
            assert_eq!(state.bounds(), before);
        }
    }

    #[test]
    fn the_supporting_block_takes_the_shape_test_of_block_collisions() {
        // findSupportingBlock reads BlockCollisions (26.3 CollisionGetter.java): a partial shape
        // overlapped within 1e-7 does not support, Shapes.block() by the strict test does
        let context = EntityCollisionContext::default();
        let area = Aabb::new(Vec3::new(0.1, 0.5 - 5.0e-8, 0.1), Vec3::new(0.9, 1.0, 0.9));
        let mut world = WorldSnapshot::new();
        world.set_block_shapes(
            (0, 0, 0),
            BlockState::default(),
            vec![BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                ..BlockShape::full_block()
            }],
        );
        assert_eq!(world.supporting_block_position(area, Vec3::new(0.5, 0.5, 0.5), &context, MinecraftVersion::new(26, 3, 0)), None);
        let mut world = WorldSnapshot::new();
        world.set_block_shapes((0, 0, 0), BlockState::default(), vec![BlockShape::full_block()]);
        let area = Aabb::new(Vec3::new(0.1, 1.0 - 5.0e-8, 0.1), Vec3::new(0.9, 1.5, 0.9));
        assert_eq!(
            world.supporting_block_position(area, Vec3::new(0.5, 1.0, 0.5), &context, MinecraftVersion::new(26, 3, 0)),
            Some((0, 0, 0))
        );
    }

    // ---- B097: fall history ----

    fn faller(y: f64) -> SimulationState {
        SimulationState {
            position: Vec3::new(0.5, y, 0.5),
            half_width: 0.3,
            height: 1.8,
            safe_fall_distance: 3.0,
            is_player: true,
            controlled_by_local_instance: true,
            ..SimulationState::default()
        }
    }

    fn step(version: &str, state: &mut SimulationState, world: &WorldSnapshot) {
        crate::versioned::dispatch(version, state, world, PhysicsConfig::default()).unwrap();
    }

    #[test]
    fn a_long_fractional_descent_counts_each_move_in_the_precision_of_the_release() {
        // Java's intermediates, written out here rather than through accumulated_fall_distance:
        // 1.18.1 Entity.java:1019 `(float)((double)fd - dy)`, 1.19 :1005 `fd -= (float)dy`,
        // 1.21.5 :1323 and 26.3 :1598 `fd -= (double)(float)dy`
        let world = WorldSnapshot::new();
        let families: [(&str, fn(f64, f64) -> f64); 5] = [
            ("1.16.5", |fd, dy| f64::from((fd - dy) as f32)),
            ("1.18.1", |fd, dy| f64::from((fd - dy) as f32)),
            ("1.19", |fd, dy| f64::from(fd as f32 - dy as f32)),
            ("1.21.5", |fd, dy| fd - f64::from(dy as f32)),
            ("26.3", |fd, dy| fd - f64::from(dy as f32)),
        ];
        for (version, count) in families {
            let mut state = faller(300.0);
            let (mut fall_distance, mut velocity) = (0.0_f64, 0.0_f64);
            for tick_number in 0..40 {
                step(version, &mut state, &world);
                if velocity < 0.0 {
                    fall_distance = count(fall_distance, velocity);
                }
                velocity = (velocity - 0.08) * java_float(0.98);
                assert_eq!(state.velocity.y, velocity, "{version} tick {tick_number}");
                assert_eq!(state.fall_distance, fall_distance, "{version} tick {tick_number}");
            }
            assert_eq!(state.fall_damage, 0.0, "{version}");
        }
    }

    #[test]
    fn upward_motion_leaves_the_fall_distance_for_the_drop_to_count_on() {
        // Entity.checkFallDamage counts only a move going down and a rise in the air leaves the
        // distance as it is (1.20.4 Entity.java:1116-1130, 26.3 :1596-1610)
        let world = WorldSnapshot::new();
        for version in ["1.8.8", "1.20.4", "26.3"] {
            let mut state = SimulationState {
                velocity: Vec3::new(0.0, 0.1, 0.0),
                fall_distance: 2.0,
                ..faller(300.0)
            };
            step(version, &mut state, &world);
            assert_eq!(state.fall_distance, 2.0, "{version}");
            while state.velocity.y >= 0.0 {
                step(version, &mut state, &world);
                assert_eq!(state.fall_distance, 2.0, "{version}");
            }
            step(version, &mut state, &world);
            assert!(state.fall_distance > 2.0, "{version}");
        }
    }

    #[test]
    fn a_step_up_lands_with_no_drop_of_its_own() {
        // Entity.move: a fall that steps onto a slab ends on the ground (verticalCollisionBelow
        // goes by the movement it asked for), and the movement it made goes up, so the 1.21.5
        // checkFallDamage adds nothing before the landing (26.3 Entity.java:1597-1599) and the
        // distance stays 4.97: floor 1 where the 0.05 of the way down would have made it 2;
        // ceil 2 before 1.21.5 either way (1.20.4 LivingEntity.java:1572)
        let mut world = WorldSnapshot::new();
        for x in -1..3 {
            world.set_block(
                (x, 0, 0),
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
                id: 2,
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for (version, damage) in [("1.20.4", 2.0), ("26.3", 1.0)] {
            let mut state = SimulationState {
                velocity: Vec3::new(0.6, -0.1, 0.0),
                fall_distance: 4.97,
                ..faller(1.05)
            };
            step(version, &mut state, &world);
            assert_eq!(state.position.y, 1.5, "{version}");
            assert!(state.on_ground, "{version} {:?} {:?} {}", state.position, state.velocity, state.collided_vertically);
            assert_eq!(state.fall_damage, damage, "{version}");
            assert_eq!(state.fall_distance, 0.0, "{version}");
        }
    }

    #[test]
    fn a_state_reseeded_through_its_entity_state_keeps_its_fall_history() {
        // merge_entity_state replaces what EntityState holds and nothing else: a tick reseeded from
        // its own prediction goes on with the same fall distance and landing as an unbroken one
        use crate::entity::EntityState;
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        for version in ["1.18.1", "1.20.4", "26.3"] {
            let mut unbroken = faller(9.0);
            let mut reseeded = unbroken;
            let mut landed = 0.0_f64;
            for tick_number in 0..40 {
                step(version, &mut unbroken, &world);
                step(version, &mut reseeded, &world);
                let prediction = EntityState::from(reseeded);
                reseeded.merge_entity_state(prediction);
                let at = format!("{version} tick {tick_number}");
                assert_eq!(reseeded.position, unbroken.position, "{at}");
                assert_eq!(reseeded.fall_distance, unbroken.fall_distance, "{at}");
                assert_eq!(reseeded.fall_damage, unbroken.fall_damage, "{at}");
                landed = landed.max(unbroken.fall_damage);
            }
            assert_eq!(landed, 5.0, "{version}: a fall of 8 blocks hurts");
        }
    }

    // ---- B098: the landing damage ----

    #[test]
    fn the_modern_epsilon_decides_a_fall_ending_on_a_whole_point() {
        use crate::simulation::attribute_landing_damage;
        // calculateFallPower `fallDistance + 1.0E-6 - safe`, then Mth.floor (1.21.5
        // LivingEntity.java:1643-1653, 26.3 :1783-1790): just under a point counts as the point
        // within the epsilon, and no further
        let modern = MinecraftVersion::new(26, 3, 0);
        let floor = LandingDamageRounding::Floor;
        for (fall_distance, safe, damage) in [
            (4.0, 3.0, 1.0),
            (4.0 - 1.0e-7, 3.0, 1.0),
            (4.0 - 2.0e-6, 3.0, 0.0),
            (3.0, 3.0, 0.0),
            (5.0 - 1.0e-7, 3.0, 2.0),
            (1.0 - 1.0e-7, 0.0, 1.0),
            (0.5, 0.0, 0.0),
            (11.0, 10.0, 1.0),
            (9.0, 10.0, 0.0),
        ] {
            assert_eq!(
                attribute_landing_damage(fall_distance, safe, 1.0, 1.0, floor, modern),
                damage,
                "{fall_distance} {safe}"
            );
        }
        // the attribute multiplies after the block's, in double
        assert_eq!(attribute_landing_damage(5.0, 3.0, 0.2, 2.5, floor, modern), 1.0);
        assert_eq!(attribute_landing_damage(5.0, 3.0, 1.0, 0.0, floor, modern), 0.0);
        // 1.20.5 - 1.21.4: ceil on a float, so any excess is a point (1.20.5 LivingEntity.java:1593-1601)
        let attribute = MinecraftVersion::new(1, 21, 4);
        let ceil = LandingDamageRounding::Ceil;
        let just_over = f64::from(f32::from_bits(3.0_f32.to_bits() + 1));
        assert_eq!(attribute_landing_damage(just_over, 3.0, 1.0, 1.0, ceil, attribute), 1.0);
        assert_eq!(attribute_landing_damage(3.0, 3.0, 1.0, 0.5, ceil, attribute), 0.0);
        assert_eq!(attribute_landing_damage(0.25, 0.0, 1.0, 1.0, ceil, attribute), 1.0);
        assert_eq!(attribute_landing_damage(5.0, 3.0, 1.0, 0.25, ceil, attribute), 1.0);
    }

    #[test]
    fn a_bounced_landing_reports_its_damage_on_its_own_tick_only() {
        // The damage is the amount of the tick's Entity.checkFallDamage -> causeFallDamage (1.20.4
        // Entity.java:1116-1130, 26.3 LivingEntity.java:1722-1745): a bed bounces the player up and
        // the next tick, in the air, lands on nothing
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape {
                    effect: BlockEffect::Bed,
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for version in ["1.20.4", "1.21.11"] {
            let mut state = SimulationState {
                velocity: Vec3::new(0.0, -1.0, 0.0),
                fall_distance: 13.0,
                ..faller(1.5)
            };
            step(version, &mut state, &world);
            assert!(state.on_ground, "{version} {:?} {:?} {}", state.position, state.velocity, state.collided_vertically);
            assert!(state.fall_damage > 0.0, "{version}");
            assert!(state.velocity.y > 0.0, "{version}: the bed bounces it");
            step(version, &mut state, &world);
            assert!(!state.on_ground, "{version}");
            assert_eq!(state.fall_damage, 0.0, "{version}");
        }
    }

    // ---- B108: the jump's counter ----

    fn on_the_floor(fields: SimulationState) -> (WorldSnapshot, SimulationState) {
        let mut world = WorldSnapshot::new();
        for x in -3..4 {
            for z in -3..4 {
                world.set_block(
                    (x, 0, z),
                    BlockState {
                        id: 1,
                        shape: Some(BlockShape::full_block()),
                        ..BlockState::default()
                    },
                );
            }
        }
        (
            world,
            SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, -0.0784000015258789, 0.0),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                is_player: true,
                controlled_by_local_instance: true,
                jump_requested: true,
                ..fields
            },
        )
    }

    #[test]
    fn a_jump_with_no_power_still_arms_the_delay_and_moves_nothing() {
        // LivingEntity.aiStep sets noJumpDelay = 10 after jumpFromGround, which from 1.20.5 returns
        // at once for a power of at most 1.0E-5F (26.3 LivingEntity.java:2303-2306, :3024-3026)
        for version in ["1.20.5", "1.21.11", "26.3"] {
            for strength in [0.0, 1.0e-5] {
                let (world, mut state) = on_the_floor(SimulationState {
                    jump_strength: Some(strength),
                    sprinting: true,
                    ..SimulationState::default()
                });
                step(version, &mut state, &world);
                assert_eq!(state.jump_ticks, 10, "{version} {strength}");
                assert_eq!(state.position.y, 1.0, "{version} {strength}");
                assert_eq!(state.velocity.z, 0.0, "{version}: no sprint impulse either");
                assert!(state.on_ground, "{version} {strength}");
            }
        }
    }

    #[test]
    fn the_jump_counts_down_before_it_tests_the_delay() {
        // LivingEntity.aiStep: `if (noJumpDelay > 0) --noJumpDelay` first, the test `== 0` later in
        // the same tick (1.8.8 LivingEntity.java:1394-1395, :1445; 1.20.4 :2513-2514, :2576;
        // 26.3 :2959-2960, :3024)
        for version in ["1.8.8", "1.20.4", "26.3"] {
            for (delay, launches, left) in [(0, true, 10), (1, true, 10), (2, false, 1), (10, false, 9)] {
                let (world, mut state) = on_the_floor(SimulationState {
                    jump_ticks: delay,
                    ..SimulationState::default()
                });
                step(version, &mut state, &world);
                assert_eq!(state.position.y > 1.0, launches, "{version} {delay}");
                assert_eq!(state.jump_ticks, left, "{version} {delay}");
            }
        }
    }

    #[test]
    fn a_held_jump_traces_the_delay_tick_by_tick() {
        // The launch arms 10, the air ticks count it down with the key still down, and the landing
        // after it ran out launches at once: physrec/j_hop_held records the same on every release
        for version in ["1.8.8", "1.20.4", "26.3"] {
            let (world, mut state) = on_the_floor(SimulationState::default());
            let mut launches = Vec::new();
            let mut trace = Vec::new();
            for tick_number in 0..40 {
                let grounded = state.on_ground;
                step(version, &mut state, &world);
                trace.push(state.jump_ticks);
                if grounded && state.position.y > 1.0 {
                    launches.push(tick_number);
                    assert_eq!(state.jump_ticks, 10, "{version} tick {tick_number}");
                }
            }
            assert_eq!(&trace[..11], &[10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0], "{version}");
            assert!(launches.len() >= 3, "{version}: {launches:?}");
            assert_eq!(launches[0], 0, "{version}");
            assert!(launches.windows(2).all(|pair| pair[1] - pair[0] > 10), "{version}: {launches:?}");
        }
    }

    #[test]
    fn releasing_the_key_clears_the_delay_and_a_repress_jumps_at_once() {
        // `else { noJumpDelay = 0 }` (1.8.8 LivingEntity.java:1450, 1.20.4 :2587, 26.3 :3035)
        for version in ["1.8.8", "1.20.4", "26.3"] {
            let (world, mut state) = on_the_floor(SimulationState {
                jump_ticks: 7,
                ..SimulationState::default()
            });
            state.jump_requested = false;
            step(version, &mut state, &world);
            assert_eq!(state.jump_ticks, 0, "{version}");
            assert_eq!(state.position.y, 1.0, "{version}");
            state.jump_requested = true;
            step(version, &mut state, &world);
            assert_eq!(state.jump_ticks, 10, "{version}");
            assert!(state.position.y > 1.0, "{version}");
        }
    }

    #[test]
    fn a_jump_held_in_the_air_or_swimming_only_counts_down() {
        // Only the ground branch arms the delay and only the released key clears it: a swim up
        // (jumpInLiquid) and a tick in the air leave it to run out (26.3 LivingEntity.java:3016-3035)
        let mut world = WorldSnapshot::new();
        for y in 0..4 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_level: 8,
                    ..BlockState::default()
                },
            );
        }
        for version in ["1.20.4", "26.3"] {
            let mut swimming = SimulationState {
                in_water: true,
                jump_requested: true,
                jump_ticks: 5,
                ..faller(1.5)
            };
            step(version, &mut swimming, &world);
            assert_eq!(swimming.jump_ticks, 4, "{version}");
            let mut airborne = SimulationState {
                jump_requested: true,
                jump_ticks: 3,
                ..faller(50.0)
            };
            step(version, &mut airborne, &WorldSnapshot::new());
            assert_eq!(airborne.jump_ticks, 2, "{version}");
        }
    }

    #[test]
    fn a_head_clip_ends_the_rise_and_keeps_the_delay() {
        // A ceiling 2 above the feet stops the 0.42 launch inside its first tick (Entity.collide):
        // the blocked motion goes to 0, then gravity and drag, and the delay stays armed
        for version in ["1.20.4", "26.3"] {
            let (mut world, mut state) = on_the_floor(SimulationState::default());
            world.set_block(
                (0, 3, 0),
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
            let top = state.height;
            step(version, &mut state, &world);
            assert!((state.position.y - (3.0 - top)).abs() < 1.0e-9, "{version}");
            assert!(state.collided_vertically, "{version}");
            assert!(!state.on_ground, "{version}");
            assert_eq!(state.velocity.y, (0.0 - 0.08) * java_float(0.98), "{version}");
            assert_eq!(state.jump_ticks, 10, "{version}");
        }
    }

    #[test]
    fn a_jump_over_a_faster_rise_keeps_it_and_arms_the_delay_from_1_21_2() {
        // jumpFromGround's `Math.max((double)jumpPower, movement.y)` (26.3 LivingEntity.java:2307-2308);
        // before 1.21.2 the power replaces the motion
        for (version, kept) in [("1.21.1", false), ("1.21.2", true), ("26.3", true)] {
            let (world, mut state) = on_the_floor(SimulationState::default());
            state.velocity.y = 0.6;
            step(version, &mut state, &world);
            let want = if kept { 0.6 } else { f64::from(0.42_f32) };
            assert!((state.position.y - 1.0 - want).abs() < 1.0e-12, "{version}");
            assert_eq!(state.jump_ticks, 10, "{version}");
        }
    }

    #[test]
    fn a_jump_leaves_the_fall_distance_to_the_landing() {
        // Nothing in LivingEntity.jumpFromGround touches fallDistance (1.8.8 LivingEntity.java:1110,
        // 1.20.4 :2020, 26.3 :2303): a state seeded on the ground with one still has it after the launch
        for version in ["1.8.8", "1.20.4", "26.3"] {
            let (world, mut state) = on_the_floor(SimulationState {
                fall_distance: 2.0,
                ..SimulationState::default()
            });
            step(version, &mut state, &world);
            assert!(state.position.y > 1.0, "{version}");
            assert_eq!(state.fall_distance, 2.0, "{version}");
        }
    }

    // ---- B114: gravity by travel family ----

    #[test]
    fn levitation_outranks_no_gravity_in_the_air_and_gravity_goes_on_under_slow_falling() {
        // LivingEntity.travelInAir (26.3 :2382-2397): the Levitation branch comes first whatever the
        // gravity; without it getEffectiveGravity is min(gravity, 0.01) falling under Slow Falling
        // (:2338-2340) and 0 with NoGravity (Entity.java:1563-1565)
        let world = WorldSnapshot::new();
        let drag = java_float(0.98);
        for version in ["1.20.5", "1.21.11", "26.3"] {
            let run = |no_gravity: bool, levitation: bool, slow_falling: bool, gravity: Option<f64>, velocity: f64| {
                let mut state = SimulationState {
                    velocity: Vec3::new(0.0, velocity, 0.0),
                    no_gravity,
                    levitation,
                    slow_falling,
                    gravity,
                    ..faller(300.0)
                };
                step(version, &mut state, &world);
            state.velocity.y
            };
            let lifted = (-0.25 + (0.05 + 0.25) * 0.2) * drag;
            assert_eq!(run(true, true, false, None, -0.25), lifted, "{version}");
            assert_eq!(run(false, true, false, None, -0.25), lifted, "{version}");
            assert_eq!(run(true, false, true, None, -0.25), -0.25 * drag, "{version}");
            assert_eq!(run(true, false, false, None, 0.25), 0.25 * drag, "{version}");
            // Slow Falling caps a falling move and leaves a rising one
            assert_eq!(run(false, false, true, None, -0.25), (-0.25 - 0.01) * drag, "{version}");
            assert_eq!(run(false, false, true, None, 0.25), (0.25 - 0.08) * drag, "{version}");
            // zero and negative gravity go through the cap as they are
            assert_eq!(run(false, false, true, Some(-0.02), -0.25), (-0.25 + 0.02) * drag, "{version}");
            assert_eq!(run(false, false, true, Some(0.0), -0.25), -0.25 * drag, "{version}");
            assert_eq!(run(false, false, false, Some(0.5), -0.25), (-0.25 - 0.5) * drag, "{version}");
            // RangedAttribute clamps to -1..1 (1.21.11 Attributes.java:26-66, RangedAttribute.java:30-32)
            assert_eq!(run(false, false, false, Some(2.0), -0.25), (-0.25 - 1.0) * drag, "{version}");
            assert_eq!(run(false, false, false, Some(-2.0), -0.25), (-0.25 + 1.0) * drag, "{version}");
        }
    }

    #[test]
    fn slow_falling_chooses_by_the_sign_and_counts_zero_as_falling() {
        // `<= 0.0`: 1.21 LivingEntity.java:2118, 26.3 :2338-2339. The motion stays above 0.003, or
        // aiStep zeroes it first (26.3 :2992).
        let world = WorldSnapshot::new();
        for version in ["1.20.4", "1.21.1", "1.21.2", "26.3"] {
            for (velocity, gravity) in [(0.005, 0.08), (0.0, 0.01), (-0.005, 0.01)] {
                let mut state = SimulationState {
                    velocity: Vec3::new(0.0, velocity, 0.0),
                    slow_falling: true,
                    ..faller(300.0)
                };
                step(version, &mut state, &world);
                assert_eq!(
                    state.velocity.y,
                    (velocity - gravity) * java_float(0.98),
                    "{version} {velocity}"
                );
            }
        }
    }

    #[test]
    fn no_gravity_stops_fluid_gravity_but_not_a_swim_up() {
        // travelInLava's `if (baseGravity != 0.0)` (26.3 LivingEntity.java:2454-2456); the jump's
        // 0.04 comes before travel and is no gravity (:3016-3019)
        let mut world = WorldSnapshot::new();
        for y in 0..8 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Lava,
                    fluid_level: 8,
                    ..BlockState::default()
                },
            );
        }
        // (from 26.1 the fluid needs the chunks around its cells, EntityFluidInteraction.java:94-110)
        for x in -1..=0 {
            for z in -1..=0 {
                world.set_chunk_loaded(x, z, true);
            }
        }
        for version in ["1.20.4", "26.3"] {
            let mut sinking = SimulationState {
                in_lava: true,
                no_gravity: true,
                velocity: Vec3::new(0.0, -0.1, 0.0),
                ..faller(3.5)
            };
            step(version, &mut sinking, &world);
            assert_eq!(sinking.velocity.y, -0.05, "{version}");
            let mut swimming = SimulationState {
                in_lava: true,
                no_gravity: true,
                jump_requested: true,
                velocity: Vec3::new(0.0, -0.1, 0.0),
                ..faller(3.5)
            };
            step(version, &mut swimming, &world);
            assert_eq!(swimming.velocity.y, (-0.1 + 0.03999999910593033) * 0.5, "{version}");
        }
    }

    // ---- B121: the border and the columns between ticks ----

    fn walking_east_at(world: &WorldSnapshot, version: &str, x: f64) -> SimulationState {
        let mut state = SimulationState {
            position: Vec3::new(x, 0.0, 0.5),
            velocity: Vec3::new(0.25, 0.0, 0.0),
            no_gravity: true,
            ..faller(0.0)
        };
        step(version, &mut state, world);
        state
    }

    #[test]
    fn a_shrinking_border_collides_at_the_snapshot_of_each_tick() {
        // Entity.collide offers the border by the edges of the moment (26.3 Entity.java:1262-1263,
        // WorldBorder.java:121-123, from 1.21.11 the extent's previousSize, :340-346): the snapshot
        // a caller gives is that tick's, and an edge that has passed the player stops nothing
        let mut world = WorldSnapshot::new();
        for version in ["1.16.5", "1.21.11", "26.3"] {
            world.set_world_bounds(-64, 320, 0.0, 0.0, 20.0);
            let free = walking_east_at(&world, version, 8.5);
            assert_eq!(free.position.x, 8.5 + 0.25, "{version}");
            world.set_world_bounds(-64, 320, 0.0, 0.0, 18.0);
            let stopped = walking_east_at(&world, version, 8.5);
            assert_eq!(stopped.position.x, 8.5 + (9.0 - (8.5 + 0.3)), "{version}");
            assert!(stopped.collided_horizontally, "{version}");
            world.set_world_bounds(-64, 320, 0.0, 0.0, 14.0);
            let outside = walking_east_at(&world, version, 8.5);
            assert_eq!(outside.position.x, 8.5 + 0.25, "{version}");
        }
    }

    #[test]
    fn a_player_outside_the_border_walks_in_and_meets_it_from_inside_next() {
        // The border's shape has nothing between an entity outside it and the inside
        // (WorldBorder.getCollisionShape, 26.3 Entity.java:1262-1263), which then stops a move out
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        for version in ["1.16.5", "26.3"] {
            let mut state = SimulationState {
                position: Vec3::new(5.6, 0.0, 0.5),
                velocity: Vec3::new(-0.5, 0.0, 0.0),
                no_gravity: true,
                ..faller(0.0)
            };
            step(version, &mut state, &world);
            assert_eq!(state.position.x, 5.6 - 0.5, "{version}");
            assert!(!state.collided_horizontally, "{version}");
            let mut state = SimulationState {
                position: Vec3::new(4.5, 0.0, 0.5),
                velocity: Vec3::new(0.5, 0.0, 0.0),
                no_gravity: true,
                ..faller(0.0)
            };
            step(version, &mut state, &world);
            assert_eq!(state.position.x, 4.5 + (5.0 - (4.5 + 0.3)), "{version}");
            assert!(state.collided_horizontally, "{version}");
        }
    }

    #[test]
    fn a_column_unloaded_and_loaded_again_keeps_the_fallback_to_itself() {
        // Known air falls by gravity, missing data sinks at 0.1 and the reload ends it (1.13.2
        // LivingEntity.java travel); from 1.14 a client's world says every chunk is there
        // (26.3 ClientLevel.java:490-492) and it falls on
        let mut world = WorldSnapshot::new();
        let fall = |world: &WorldSnapshot, version: &str| {
            let mut state = SimulationState { client_side: true, ..faller(50.0) };
            step(version, &mut state, world);
            state.velocity.y
        };
        let sinking = -0.1 * java_float(0.98);
        let falling = -0.08 * java_float(0.98);
        for (version, missing) in [("1.8.8", sinking), ("1.13.2", sinking), ("1.14.4", falling), ("26.3", falling)] {
            world.set_chunk_loaded(0, 0, true);
            assert_eq!(fall(&world, version), falling, "{version} loaded");
            world.set_chunk_loaded(0, 0, false);
            assert_eq!(fall(&world, version), missing, "{version} unloaded");
            world.set_chunk_loaded(0, 0, true);
            assert_eq!(fall(&world, version), falling, "{version} reloaded");
        }
    }

    /// A client tick of a frozen player on `world`, with the attribute's parts; the value it leaves.
    fn tick_with_frost(
        version: &str,
        world: &WorldSnapshot,
        mut state: SimulationState,
        attribute: &mut crate::simulation::MovementSpeedAttribute,
    ) -> f64 {
        let release = MinecraftVersion::parse(version).unwrap();
        crate::simulation::tick_local_player_scene(
            &mut world.clone(),
            &mut state,
            Some(attribute),
            &[],
            (0, 0),
            false,
            &[],
            &mut [0.0; 3],
            PhysicsConfig::default(),
            release,
            |state, world| {
                crate::versioned::release_riptide(version, state, world, PhysicsConfig::default())
            },
            |state, world| {
                crate::versioned::dispatch(version, state, world, PhysicsConfig::default())
            },
        )
        .unwrap();
        state.movement_speed_value.unwrap()
    }

    const BASE_SPEED: f64 = 0.10000000149011612;

    fn frozen_player(y: f64, ticks_frozen: u32) -> SimulationState {
        SimulationState {
            client_side: true,
            ticks_frozen,
            ticks_required_to_freeze: 140,
            movement_speed_value: Some(BASE_SPEED),
            no_gravity: true,
            ..faller(y)
        }
    }

    fn speed_attribute(frost: Option<f64>) -> crate::simulation::MovementSpeedAttribute {
        crate::simulation::MovementSpeedAttribute { base: BASE_SPEED, frost, ..Default::default() }
    }

    fn stone() -> BlockState {
        BlockState { id: 1, shape: Some(BlockShape::full_block()), ..BlockState::default() }
    }

    #[test]
    fn the_client_makes_its_frost_modifier_at_the_end_of_ai_step_up_to_1_21_4() {
        // fy_powder_snow at t=12 on 1.20.4: one frozen tick and no packet yet, the value after
        // aiStep and the modifier as the client holds it (1.17 LivingEntity.java:2555-2556, :523)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), stone());
        for version in ["1.17", "1.20.4", "1.21.1", "1.21.4"] {
            let mut attribute = speed_attribute(None);
            let value = tick_with_frost(version, &world, frozen_player(1.1, 1), &mut attribute);
            assert_eq!(value, 0.09964285863679834, "{version}");
            assert_eq!(attribute.frost, Some(-0.0003571428533177823), "{version}");
        }
        // frozen past the limit, the scale stops at 1
        let mut attribute = speed_attribute(None);
        tick_with_frost("1.20.4", &world, frozen_player(1.1, 300), &mut attribute);
        assert_eq!(attribute.frost, Some(f64::from(-0.05_f32)));
        // not a client's from 1.21.5 (1.21.5 LivingEntity.java:2763), and no frost before 1.17
        for version in ["1.16.5", "1.21.5", "26.3"] {
            let mut attribute = speed_attribute(None);
            let value = tick_with_frost(version, &world, frozen_player(1.1, 1), &mut attribute);
            assert_eq!(value, BASE_SPEED, "{version}");
            assert_eq!(attribute.frost, None, "{version}");
        }
    }

    #[test]
    fn the_frost_modifier_goes_when_the_landing_block_is_air_or_nothing_is_frozen() {
        // addPowderSnowSlowIfNeeded asks for a landing block that is not air and frozen ticks
        // (1.17 LivingEntity.java:508-521, 1.21.4 :499-513); removeFrost has taken the old one
        let held = f64::from(-0.05_f32 * (20.0_f32 / 140.0));
        let mut stone_world = WorldSnapshot::new();
        stone_world.set_block((0, 0, 0), stone());
        for version in ["1.17", "1.20.4", "1.21.4"] {
            let mut attribute = speed_attribute(Some(held));
            let mut state = frozen_player(1.1, 20);
            state.movement_speed_value = Some(BASE_SPEED + held);
            let value = tick_with_frost(version, &WorldSnapshot::new(), state, &mut attribute);
            assert_eq!((value, attribute.frost), (BASE_SPEED, None), "{version}");
            let mut attribute = speed_attribute(Some(held));
            let mut state = frozen_player(1.1, 0);
            state.movement_speed_value = Some(BASE_SPEED + held);
            let value = tick_with_frost(version, &stone_world, state, &mut attribute);
            assert_eq!((value, attribute.frost), (BASE_SPEED, None), "{version}");
        }
    }

    #[test]
    fn a_frost_modifier_that_stays_leaves_the_value_the_caller_gave() {
        // (the caller's value is vanilla's own fold, in an order of modifiers the engine does not know)
        let held = f64::from(-0.05_f32 * (20.0_f32 / 140.0));
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), stone());
        let mut attribute = speed_attribute(Some(held));
        let mut state = frozen_player(1.1, 20);
        state.movement_speed_value = Some(0.0625);
        assert_eq!(tick_with_frost("1.20.4", &world, state, &mut attribute), 0.0625);
    }

    #[test]
    fn the_frost_value_folds_every_modifier_as_the_attribute_does() {
        // the frost adds to the base before the multipliers (1.17 AttributeInstance.java:130-148)
        use crate::simulation::{AttributeModifier, AttributeOperation};
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), stone());
        let frost = f64::from(-0.05_f32 * (1.0_f32 / 140.0));
        let mut attribute = crate::simulation::MovementSpeedAttribute {
            base: BASE_SPEED,
            modifiers: vec![
                AttributeModifier { amount: 0.3, operation: AttributeOperation::AddMultipliedTotal },
                AttributeModifier { amount: 0.2, operation: AttributeOperation::AddMultipliedBase },
                AttributeModifier { amount: 0.01, operation: AttributeOperation::AddValue },
            ],
            frost: None,
        };
        let added = BASE_SPEED + 0.01 + frost;
        let value = tick_with_frost("1.20.4", &world, frozen_player(1.1, 1), &mut attribute);
        assert_eq!(value, (added + added * 0.2) * 1.3);
    }

    /// A block with getOnPos's tag bits, standing 1.5 tall.
    fn fence_like(tags: u32) -> BlockState {
        BlockState {
            id: 2,
            tags,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.5, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    #[test]
    fn get_on_pos_stops_at_a_supporting_wall_or_gate_and_a_fence_up_to_half_a_block() {
        // 26.3 Entity.java:1115, 1.20 :812; the height decides, so stand high enough over the block
        // to tell the offsets apart: a support kept from the last move while the position moved
        use crate::simulation::on_position;
        use crate::world::{FENCE_LIKE, WALL_OR_GATE};
        let release = MinecraftVersion::new(1, 20, 0);
        let state = SimulationState { position: Vec3::new(0.5, 10.5, 0.5), ..SimulationState::default() };
        let support = Some((0, 0, 0));
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), fence_like(FENCE_LIKE));
        assert_eq!(on_position(&world, &state, support, 0.2, release), (0, 0, 0));
        assert_eq!(on_position(&world, &state, support, 0.5, release), (0, 0, 0));
        assert_eq!(on_position(&world, &state, support, 0.500_001, release), (0, 9, 0));
        assert_eq!(on_position(&world, &state, support, 0.999_999, release), (0, 9, 0));
        world.set_block((0, 0, 0), fence_like(FENCE_LIKE | WALL_OR_GATE));
        assert_eq!(on_position(&world, &state, support, 0.2, release), (0, 0, 0));
        assert_eq!(on_position(&world, &state, support, 0.500_001, release), (0, 0, 0));
        assert_eq!(on_position(&world, &state, support, 0.999_999, release), (0, 0, 0));
        world.set_block((0, 0, 0), BlockState::default());
        assert_eq!(on_position(&world, &state, support, 0.2, release), (0, 10, 0));
        assert_eq!(on_position(&world, &state, None, 0.2, release), (0, 10, 0));
    }

    #[test]
    fn landing_on_a_fence_like_block_reads_it_and_not_what_stands_on_it() {
        use crate::simulation::landing_position;
        use crate::world::{FENCE_LIKE, WALL_OR_GATE};
        let tip = BlockState {
            id: 3,
            shape: Some(BlockShape { effect: BlockEffect::PointedDripstoneTip, ..BlockShape::full_block() }),
            ..BlockState::default()
        };
        let state = SimulationState { position: Vec3::new(0.5, 1.5, 0.5), ..SimulationState::default() };
        let mut world = WorldSnapshot::new();
        world.set_block((0, 1, 0), tip);
        for tags in [FENCE_LIKE, FENCE_LIKE | WALL_OR_GATE] {
            world.set_block((0, 0, 0), fence_like(tags));
            let support = Some((0, 0, 0));
            let modern = MinecraftVersion::new(1, 20, 0);
            assert_eq!(landing_position(&world, &state, support, modern), (0, 0, 0));
            // before 1.20 the fence is only looked through to from air (1.19.4 Entity.java:738-744)
            let legacy = MinecraftVersion::new(1, 19, 4);
            assert_eq!(landing_position(&world, &state, None, legacy), (0, 1, 0));
        }
        world.set_block((0, 1, 0), BlockState::default());
        for version in [MinecraftVersion::new(1, 8, 8), MinecraftVersion::new(1, 19, 4)] {
            assert_eq!(landing_position(&world, &state, None, version), (0, 0, 0));
        }
    }

    #[test]
    fn a_fence_under_the_landing_height_keeps_the_frost() {
        // floor(y - 0.2F) is the air over a fence's top half, and Entity.getLandingPos gives the
        // fence: under the air before 1.20 (1.17 Entity.java:683-688), the supporting block after
        let fence = BlockState {
            id: 2,
            tags: crate::world::FENCE_LIKE,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.5, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), fence);
        for version in ["1.17", "1.19.4", "1.20", "1.21.4"] {
            let mut state = frozen_player(1.5, 1);
            state.on_ground = true;
            state.supporting_block = Some((0, 0, 0));
            state.supporting_block_kept = true;
            state.no_gravity = false;
            state.velocity.y = -0.0784;
            let mut attribute = speed_attribute(None);
            tick_with_frost(version, &world, state, &mut attribute);
            assert!(attribute.frost.is_some(), "{version}");
        }
    }

    /// Entity.move records the move split by the collided movement before 1.21.9 (1.21.8 Entity.java:737, its `true`
    /// reads lv2) and by the requested one from it (1.21.9 Entity.java:750, its arg2), where the two can order axes apart.
    #[test]
    fn the_recorded_move_splits_by_the_requested_axes_from_1_21_9() {
        let state = SimulationState { position: Vec3::new(0.3, 1.0, 0.0), ..SimulationState::default() };
        let (old, requested, moved) = (Vec3::new(0.0, 1.1, 0.0), Vec3::new(0.3, -0.5, 0.0), Vec3::new(0.3, -0.1, 0.0));
        let order = |release| {
            crate::simulation::movements_this_tick(&state, old, requested, moved, release)[0].axis_order
        };
        assert_eq!(order(MinecraftVersion::new(1, 21, 8)), Some(moved));
        assert_eq!(order(MinecraftVersion::new(1, 21, 9)), Some(requested));
        assert_eq!(order(MinecraftVersion::new(26, 3, 0)), Some(requested));
        assert_eq!(order(MinecraftVersion::new(1, 21, 5)), Some(moved));
        assert_eq!(order(MinecraftVersion::new(1, 21, 4)), None);
    }

    fn slime() -> BlockState {
        BlockState {
            shape: Some(BlockShape {
                effect: BlockEffect::Slime,
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    #[test]
    fn a_flying_player_steps_on_slime_only_from_1_15() {
        // stepOn sits inside canClimb() before 1.15, !flying for a player (1.8.9 Entity.java:668,
        // 677, Player.java:1578-1580; 1.12.2 Entity.java:830, 839, Player.java:1649-1651; 1.14
        // Entity.java:602, 610); 1.15 asks onGround alone (1.15 Entity.java:589-591) (D-165)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), slime());
        let stepped = |release: MinecraftVersion, flying: bool| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.2, 0.0, -0.1),
                half_width: 0.3,
                height: 1.8,
                on_ground: true,
                is_player: true,
                flying,
                ..SimulationState::default()
            };
            crate::simulation::apply_block_effects(&mut state, &world, Some((0, 0, 0)), release, &[]);
            state.velocity
        };
        let kept = Vec3::new(0.2, 0.0, -0.1);
        let damped = Vec3::new(0.2 * 0.4, 0.0, -0.1 * 0.4);
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 14, 4),
        ] {
            assert_eq!(stepped(release, true), kept, "{release:?}");
            assert_eq!(stepped(release, false), damped, "{release:?}");
        }
        for release in [MinecraftVersion::new(1, 15, 0), MinecraftVersion::new(1, 21, 11)] {
            assert_eq!(stepped(release, true), damped, "{release:?}");
        }
    }

    #[test]
    fn a_ceiling_hit_over_slime_keeps_the_fall_distance() {
        // the slime block's fall callback runs on any vertical collision and leaves a rising motion
        // alone, but the fall ends in checkFallDamage only on the ground (1.8.9 Entity.java,
        // 1.20.4 Entity.java:649-664, 1.21.5 SlimeBlock.java:33-41): a head hit keeps it (D-187)
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), slime());
        world.set_block(
            (0, 2, 0),
            BlockState {
                shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::new(0.0, 0.85, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 20, 4),
            MinecraftVersion::new(1, 21, 5),
            MinecraftVersion::new(26, 1, 2),
        ] {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.0, 0.5),
                velocity: Vec3::new(0.0, 0.42, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                controlled_by_local_instance: true,
                fall_distance: 2.0,
                ..SimulationState::default()
            };
            tick_release(&mut state, &world, release);
            assert!(state.collided_vertically && !state.on_ground, "{release:?}");
            assert_eq!(state.fall_distance, 2.0, "{release:?}");
            assert!(state.velocity.y > 0.0, "{release:?}");
        }
    }

    #[test]
    fn the_water_snap_of_1_13_reads_the_motion_after_the_move() {
        // 1.13 tests velocityY <= 0.0 after the move and the drag (1.13.2 LivingEntity.java:1674),
        // 1.14 the motion before travel (1.14 LivingEntity.java:1935): a slow-falling player that
        // sinks onto slime under water bounces up, which 1.13 does not snap to -0.003 (D-191); the
        // jump of 1.13.2 LivingEntity.java:1909-1913, 1925-1930 comes after the cutoff
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), slime());
        for y in 1..=2 {
            world.set_block(
                (0, y, 0),
                BlockState {
                    fluid: FluidState::Water,
                    fluid_source: true,
                    ..BlockState::default()
                },
            );
        }
        let after = |version: &str| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.002, 0.5),
                // the jump in water adds 0.04 after the cutoff of motions under 0.003: -0.0024
                velocity: Vec3::new(0.0, -0.0424, 0.0),
                half_width: 0.3,
                height: 1.8,
                is_player: true,
                controlled_by_local_instance: true,
                in_water: true,
                jump_requested: true,
                slow_falling: true,
                ..SimulationState::default()
            };
            let rules = crate::versioned::rules_for_version(version).expect("a profile");
            tick_with_rules(&mut state, &world, PhysicsConfig::default(), rules);
            assert!(state.on_ground, "{version} {:?} {:?} {}", state.position, state.velocity, state.collided_vertically);
            state.velocity.y
        };
        // bounced to about +0.0024, dragged to 0.00192, then less the slow fall's 0.01 / 16
        let up = after("1.13.2");
        assert!((up - (0.0024 * 0.8 - 0.01 / 16.0)).abs() < 1.0e-8, "{up}");
        assert_eq!(after("1.14.4"), -0.003);
    }

    #[test]
    fn a_no_gravity_body_sinks_in_lava_on_1_9_4_and_not_from_1_10() {
        // 1.9.4 LivingEntity.travel takes 0.02 off in lava whatever NoGravity says (LivingEntity.java:1444);
        // 1.10.2 tests isNoGravity first
        let sink = |release: &str| {
            let mut world = WorldSnapshot::new();
            load_chunks_around_origin(&mut world);
            for y in 0..3 {
                world.set_block((0, y, 0), BlockState { fluid: FluidState::Lava, fluid_level: 8, fluid_source: true, ..BlockState::default() });
            }
            let mut state = SimulationState {
                position: Vec3::new(0.5, 0.5, 0.5),
                no_gravity: true,
                half_width: 0.3,
                height: 1.8,
                ..SimulationState::default()
            };
            crate::versioned::dispatch(release, &mut state, &world, PhysicsConfig::default()).expect("a supported release");
            state.velocity.y
        };
        assert!(sink("1.9.4") < 0.0, "{}", sink("1.9.4"));
        assert_eq!(sink("1.10.2"), 0.0);
    }
}
