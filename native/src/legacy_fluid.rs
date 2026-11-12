use std::ops::{Range, RangeInclusive};

use crate::math::{ceil_i32, floor_i32, Aabb, Vec3};
use crate::simulation::SimulationState;
use crate::version::MinecraftVersion;
use crate::world::{BlockState, CollisionQueryEntity, FluidContact, FluidState, WorldSnapshot};

#[must_use]
pub(crate) fn applies(release: MinecraftVersion) -> bool {
    // (1.12.2 Entity.java:1040-1043; 1.13.2 Entity.java:1052-1055)
    !release.at_least(1, 13, 0)
}

/// Entity.handleWaterMovement: Level.method_3610 over the box shrunk by 0.4F
/// vertically and 0.001 all round. Each step builds a new AxisAlignedBB, whose constructor
/// orders its corners (1.12.2 AABB.java:39-45; 1.8.8 AABB.java:23-29), so a box lower than 0.8 (a
/// boat) is turned the right way up again after crossing over.
pub(crate) fn handle_water_movement(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    pushed_by_water: bool,
    release: MinecraftVersion,
) {
    if state.boat_excludes_water(release) {
        state.in_water = false;
        return;
    }
    let bounds = state.bounds();
    let inset = f64::from(0.4_f32);
    // expand(0.0, -0.4F, 0.0) (1.12.2 AABB.java:154-162), then contract(0.001) (1.12.2
    // AABB.java:285-287) or increment(0.001, 0.001, 0.001) (1.8.8 AABB.java:186-194)
    let shrunk = Aabb::ordered(
        Vec3::new(bounds.min.x, bounds.min.y + inset, bounds.min.z),
        Vec3::new(bounds.max.x, bounds.max.y - inset, bounds.max.z),
    );
    let area = Aabb::ordered(
        Vec3::new(
            shrunk.min.x + 0.001,
            shrunk.min.y + 0.001,
            shrunk.min.z + 0.001,
        ),
        Vec3::new(
            shrunk.max.x - 0.001,
            shrunk.max.y - 0.001,
            shrunk.max.z - 0.001,
        ),
    );
    let touching = material_acceleration(state, world, area, pushed_by_water, release);
    state.in_water = touching;
    if touching {
        state.fall_distance = 0.0;
    }
}

/// Level.method_3610: whether water reaches into `area`, and the push of its flows,
/// summed and normalized.
pub(crate) fn material_acceleration(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    area: Aabb,
    pushed_by_water: bool,
    release: MinecraftVersion,
) -> bool {
    let layers = cells(area.min.y, area.max.y, release);
    let top = f64::from(layers.end);
    let mut touching = false;
    let mut flow = Vec3::ZERO;
    for x in cells(area.min.x, area.max.x, release) {
        for y in layers.clone() {
            for z in cells(area.min.z, area.max.z, release) {
                let block = world.block_cow((x, y, z));
                if block.fluid != FluidState::Water {
                    continue;
                }
                if top >= f64::from((y + 1) as f32 - height_percent(&block)) {
                    touching = true;
                    flow = flow + block.fluid_flow;
                }
            }
        }
    }
    if flow.length_in(release) > 0.0 && pushed_by_water {
        state.velocity = state.velocity + flow.normalized(release).scale(0.014);
    }
    touching
}

/// 1.13's Entity#checkAndHandleWater (yarn's handleFluidAcceleration): as from 1.14, but a fluid under the same fluid keeps
/// its own height.
#[must_use]
pub(crate) fn fluid_contact_1_13(
    world: &WorldSnapshot,
    fluid: FluidState,
    bounds: Aabb,
) -> FluidContact {
    let area = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
    let mut contact = FluidContact::default();
    for x in floor_i32(area.min.x)..ceil_i32(area.max.x) {
        for y in floor_i32(area.min.y)..ceil_i32(area.max.y) {
            for z in floor_i32(area.min.z)..ceil_i32(area.max.z) {
                let block = world.block_cow((x, y, z));
                let matches = block.fluid == fluid
                    || (fluid == FluidState::Water && block.fluid == FluidState::Waterlogged);
                if !matches {
                    continue;
                }
                let top = f64::from(y as f32 + own_height(&block));
                if top < area.min.y {
                    continue;
                }
                contact.touching = true;
                contact.height = contact.height.max(top - area.min.y);
                let flow = if contact.height < 0.4 {
                    block.fluid_flow.scale(contact.height)
                } else {
                    block.fluid_flow
                };
                contact.flow = contact.flow + flow;
                contact.flowing_blocks += 1;
            }
        }
    }
    contact
}

/// Entity.isInLava before 1.14: any lava in the box shrunk by 0.1F sideways and 0.4F vertically,
/// whose corners Box's constructor orders (1.8.8 AABB.java:23-30, Entity.java:922-924; 1.13.2
/// AABB.java:38-44, Entity.java:1143-1144).
#[must_use]
pub(crate) fn touching_lava(
    world: &WorldSnapshot,
    bounds: Aabb,
    release: MinecraftVersion,
) -> bool {
    let side = f64::from(0.1_f32);
    let vertical = f64::from(0.4_f32);
    let area = Aabb::ordered(
        Vec3::new(
            bounds.min.x + side,
            bounds.min.y + vertical,
            bounds.min.z + side,
        ),
        Vec3::new(
            bounds.max.x - side,
            bounds.max.y - vertical,
            bounds.max.z - side,
        ),
    );
    cells(area.min.x, area.max.x, release).any(|x| {
        cells(area.min.y, area.max.y, release).any(|y| {
            cells(area.min.z, area.max.z, release)
                .any(|z| world.block_cow((x, y, z)).fluid == FluidState::Lava)
        })
    })
}

/// Entity.isOffsetPositionInLiquid's test of the moved box: no collision for the entity
/// (Level.doesBoxCollide(entity, box), its colliders and the border's stone among them) and no
/// liquid.
#[must_use]
pub(crate) fn is_free(
    world: &WorldSnapshot,
    entity: CollisionQueryEntity<'_>,
    bounds: Aabb,
    release: MinecraftVersion,
) -> bool {
    // (1.8.8 Level.java:1485-1496; 1.9.4 Level.java:1557-1568)
    if release.at_least(1, 9, 0) {
        return world.is_free_for_entity(bounds, entity, release);
    }
    let liquid = liquid_cells(bounds.min.x, bounds.max.x).any(|x| {
        liquid_cells(bounds.min.y, bounds.max.y).any(|y| {
            liquid_cells(bounds.min.z, bounds.max.z)
                .any(|z| world.block_cow((x, y, z)).fluid != FluidState::Empty)
        })
    });
    world.no_collision_for_entity(bounds, entity, release) && !liquid
}

/// LivingEntity#travel's (MCP's moveEntityWithHeading's) water branch after the move: 0.8F of the vertical motion,
/// less 0.02, which NoGravity keeps from 1.10 (`if (!this.isNoGravity())`, 1.10.2 LivingEntity.java:1505); 1.8.8 and 1.9.4
/// have no NoGravity and always sink (1.8.8 LivingEntity.java:1230).
pub(crate) fn sink_in_water(state: &mut SimulationState, release: MinecraftVersion) {
    state.velocity.y *= f64::from(0.8_f32);
    if !(state.no_gravity && release.at_least(1, 10, 0)) {
        state.velocity.y -= 0.02;
    }
}

fn cells(min: f64, max: f64, release: MinecraftVersion) -> Range<i32> {
    // (1.8.8 Level.java:1533-1539; 1.9.4 Level.java:1610-1616)
    let end = if release.at_least(1, 9, 0) {
        ceil_i32(max)
    } else {
        floor_i32(max + 1.0)
    };
    floor_i32(min)..end
}

/// The cells Level#containsFluid scans on one axis, Mth.floor of the minimum to Mth.floor of the
/// maximum, both included.
fn liquid_cells(min: f64, max: f64) -> RangeInclusive<i32> {
    floor_i32(min)..=floor_i32(max)
}

/// LiquidBlock#getHeightPercent of the block's level, a falling or source liquid's taken as 0.
fn height_percent(block: &BlockState) -> f32 {
    let level = if block.fluid_source || block.fluid_falling || block.fluid_level >= 8 {
        0
    } else {
        block.fluid_level
    };
    f32::from(level + 1) / 9.0
}

/// The fluid's amount over 9 (vanilla: FlowingFluid#getHeight in 1.13, FluidState#getOwnHeight
/// from 1.14): 8 for a source or a falling fluid, else 8 less the level.
fn own_height(block: &BlockState) -> f32 {
    let amount =
        if block.fluid_source || block.fluid_falling || !(1..8).contains(&block.fluid_level) {
            8
        } else {
            8 - block.fluid_level
        };
    f32::from(amount) / 9.0
}

#[cfg(test)]
mod tests {
    use super::{cells, is_free, touching_lava};
    use crate::math::{Aabb, Vec3};
    use crate::version::MinecraftVersion;
    use crate::world::{
        BlockShape, BlockState, CollisionQueryEntity, EntityCollider, EntityCollisionContext,
        FluidState, WorldSnapshot,
    };

    /// is_free for an entity whose box is the moved box, inside the border.
    fn free(world: &WorldSnapshot, bounds: Aabb, release: MinecraftVersion) -> bool {
        let context = EntityCollisionContext {
            bottom: bounds.min.y,
            ..EntityCollisionContext::default()
        };
        let entity = CollisionQueryEntity {
            context: &context,
            bounds,
            position: Vec3::new(
                (bounds.min.x + bounds.max.x) / 2.0,
                bounds.min.y,
                (bounds.min.z + bounds.max.z) / 2.0,
            ),
            outside_border: false,
        };
        is_free(world, entity, bounds, release)
    }

    #[test]
    fn block_ranges_reach_past_a_whole_maximum_only_in_1_8() {
        assert_eq!(cells(0.5, 2.0, MinecraftVersion::new(1, 8, 8)), 0..3);
        assert_eq!(cells(0.5, 2.0, MinecraftVersion::new(1, 9, 4)), 0..2);
        assert_eq!(cells(0.5, 2.5, MinecraftVersion::new(1, 8, 8)), 0..3);
        assert_eq!(cells(0.5, 2.5, MinecraftVersion::new(1, 9, 4)), 0..3);
    }

    #[test]
    fn one_layer_of_snow_blocks_the_hop_before_1_13() {
        // Entity.doesNotCollide (1.12.2 Entity.java:553-555, 1.8.8 :459-460) tests
        // Level.getCollisionBoxes, which holds the box of no height of one layer of snow (1.12.2
        // SnowLayerBlock.java:75-79) when the moved box strictly meets it, and no box of a block it
        // does not meet (Block.appendCollisionBoxes, 1.12.2 Block.java:435-439).
        let shape = |height: f64| BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, height, 1.0)),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        };
        let mut world = WorldSnapshot::new();
        world.set_block((0, 1, 0), shape(0.0));
        let across = Aabb::new(Vec3::new(0.2, 0.95, 0.2), Vec3::new(0.8, 2.75, 0.8));
        let above = Aabb::new(Vec3::new(0.2, 1.0, 0.2), Vec3::new(0.8, 2.8, 0.8));
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 12, 2),
        ] {
            assert!(!free(&world, across, release));
            assert!(free(&world, above, release));
        }
        let mut world = WorldSnapshot::new();
        world.set_block((0, 1, 0), shape(0.5));
        let over_the_slab = Aabb::new(Vec3::new(0.2, 1.6, 0.2), Vec3::new(0.8, 3.4, 0.8));
        assert!(free(&world, over_the_slab, MinecraftVersion::new(1, 12, 2)));
    }

    #[test]
    fn liquid_on_a_whole_maximum_blocks_the_hop_only_in_1_8() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 2, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let bounds = Aabb::new(Vec3::new(0.2, 1.0, 0.2), Vec3::new(0.8, 2.0, 0.8));

        assert!(!free(&world, bounds, MinecraftVersion::new(1, 8, 8)));
        assert!(free(&world, bounds, MinecraftVersion::new(1, 9, 4)));
    }

    #[test]
    fn an_entity_collider_the_box_meets_blocks_the_hop_before_1_13() {
        // Level.doesBoxCollide(entity, box) adds the boxes of the entities around that meet the
        // box (1.8.8 Level.java:1081-1089, 1.12.2 Level.java:1161-1184)
        let mut world = WorldSnapshot::new();
        world.set_entity_colliders(vec![EntityCollider {
            bounds: Aabb::new(Vec3::new(0.7, 1.0, 0.2), Vec3::new(2.075, 1.5625, 1.575)),
            only_from_above: false,
        }]);
        let meeting = Aabb::new(Vec3::new(0.2, 1.2, 0.2), Vec3::new(0.8, 3.0, 0.8));
        let beside = Aabb::new(Vec3::new(-0.7, 1.2, 0.2), Vec3::new(-0.1, 3.0, 0.8));
        for release in [
            MinecraftVersion::new(1, 8, 8),
            MinecraftVersion::new(1, 12, 2),
        ] {
            assert!(!free(&world, meeting, release));
            assert!(free(&world, beside, release));
        }
    }

    /// Which of `cells` hold lava that touching_lava finds for a box 0.6 wide and `height` high
    /// standing at (x, y, z), one lava block in the world at a time.
    fn lava_found(
        (x, y, z): (f64, f64, f64),
        height: f32,
        cells: &[(i32, i32, i32)],
        release: MinecraftVersion,
    ) -> Vec<(i32, i32, i32)> {
        let bounds = Aabb::new(
            Vec3::new(x - 0.3, y, z - 0.3),
            Vec3::new(x + 0.3, y + f64::from(height), z + 0.3),
        );
        cells
            .iter()
            .copied()
            .filter(|&cell| {
                let mut world = WorldSnapshot::new();
                world.set_block(
                    cell,
                    BlockState {
                        fluid: FluidState::Lava,
                        fluid_source: true,
                        ..BlockState::default()
                    },
                );
                touching_lava(&world, bounds, release)
            })
            .collect()
    }

    #[test]
    fn the_lava_box_is_turned_the_right_way_up_around_a_contracted_height_of_0_8() {
        // Entity.isInLava contracts the box by 0.1F, 0.4F, 0.1F; Box's constructor orders the
        // corners (1.8.8 Entity.java:922-924, AABB.java:23-30; 1.9.4 Entity.java:1031-1032, AABB.java:
        // 24-31, 111-119; 1.13.2 Entity.java:1143-1145, AABB.java:38-45, 228-230), and
        // Level.containsMaterial reads floor(min) until ceil(max) (1.13.2 Level.java:1362-1384)
        let column = |y: i32| (0, y, 0);
        let around = [63, 64, 65, 66, 67].map(column);
        for release in [
            MinecraftVersion::new(1, 9, 4),
            MinecraftVersion::new(1, 12, 2),
            MinecraftVersion::new(1, 13, 2),
        ] {
            let found = |y: f64, height: f32, cells: &[(i32, i32, i32)]| {
                lava_found((0.5, y, 0.5), height, cells, release)
            };
            // below 0.8F: 64.9 - 65.1
            assert_eq!(found(64.7, 0.6, &around), vec![column(64), column(65)]);
            // at 0.8F: 65.1 twice, in the one cell
            assert_eq!(found(64.7, 0.8, &around), vec![column(65)]);
            // above 0.8F: 65.1 - 65.2, and 65.1 - 66.1
            assert_eq!(found(64.7, 0.9, &around), vec![column(65)]);
            assert_eq!(found(64.7, 1.8, &around), vec![column(65), column(66)]);

            // on whole coordinates: ceil(65.0) leaves the cell 65 out of 64.8 - 65.0, floor(65.0)
            // keeps it in 65.0 - 65.2, and 65.0 - 65.0 reads no cell at all
            let foot_contracted_to_65 = 65.0 - f64::from(0.4_f32);
            let head_contracted_to_65 = 65.0 - (f64::from(0.6_f32) - f64::from(0.4_f32));
            assert_eq!(found(foot_contracted_to_65, 0.6, &around), vec![column(64)]);
            assert_eq!(found(head_contracted_to_65, 0.6, &around), vec![column(65)]);
            assert_eq!(found(foot_contracted_to_65, 0.8, &around), vec![]);
        }
    }

    #[test]
    fn the_lava_box_is_turned_the_right_way_up_at_negative_coordinates() {
        let release = MinecraftVersion::new(1, 13, 2);
        let found = |y: f64, height: f32, cells: &[(i32, i32, i32)]| {
            lava_found((-0.5, y, -0.5), height, cells, release)
        };
        let column = |y: i32| (-1, y, -1);
        let around = [-6, -5, -4, -3, -2].map(column);
        // -3.1 - -2.9, floor -4 until ceil -2
        assert_eq!(found(-3.3, 0.6, &around), vec![column(-4), column(-3)]);
        // the next column along holds the box no longer: -0.7 - -0.3 reads the cell -1 alone
        assert_eq!(found(-3.3, 0.6, &[(0, -4, 0), (-2, -4, -2)]), vec![]);
        // -3.2 - -3.0: ceil(-3.0) leaves the cell -3 out
        let foot_contracted_to_minus_3 = -3.0 - f64::from(0.4_f32);
        assert_eq!(found(foot_contracted_to_minus_3, 0.6, &around), vec![column(-4)]);
        // -3.0 - -3.0 reads no cell
        assert_eq!(found(foot_contracted_to_minus_3, 0.8, &around), vec![]);
    }

    #[test]
    fn a_short_box_reads_one_cell_more_in_1_8() {
        // 1.8.8 Level.containsMaterial reads until floor(max + 1) (generic short entities: no
        // registered player is this short there; 1.8.8 itself is unavailable, 1.8.9's source stands)
        let release = MinecraftVersion::new(1, 8, 8);
        let column = |y: i32| (0, y, 0);
        let around = [63, 64, 65, 66].map(column);
        assert_eq!(
            lava_found((0.5, 64.7, 0.5), 0.6, &around, release),
            vec![column(64), column(65)]
        );
        // 65.0 - 65.0 reads the cell 65 where 1.9.4 reads none
        let foot_contracted_to_65 = 65.0 - f64::from(0.4_f32);
        assert_eq!(
            lava_found((0.5, foot_contracted_to_65, 0.5), 0.8, &around, release),
            vec![column(65)]
        );
    }
}
