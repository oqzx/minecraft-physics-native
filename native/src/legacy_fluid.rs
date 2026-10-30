use std::ops::{Range, RangeInclusive};

use crate::math::{Aabb, Vec3};
use crate::simulation::SimulationState;
use crate::version::MinecraftVersion;
use crate::world::{BlockState, FluidContact, FluidState, WorldSnapshot};

#[must_use]
pub(crate) fn applies(release: MinecraftVersion) -> bool {
    !release.at_least(1, 13, 0)
}

/// Entity.handleWaterMovement: World.handleMaterialAcceleration over the box shrunk by 0.4F
/// vertically and 0.001 all round.
pub(crate) fn handle_water_movement(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    pushed_by_water: bool,
    release: MinecraftVersion,
) {
    let bounds = state.bounds();
    let inset = f64::from(0.4_f32);
    let area = Aabb::new(
        Vec3::new(
            bounds.min.x + 0.001,
            bounds.min.y + inset + 0.001,
            bounds.min.z + 0.001,
        ),
        Vec3::new(
            bounds.max.x - 0.001,
            bounds.max.y - inset - 0.001,
            bounds.max.z - 0.001,
        ),
    );
    let layers = cells(area.min.y, area.max.y, release);
    let top = f64::from(layers.end);
    let mut touching = false;
    let mut flow = Vec3::ZERO;
    for x in cells(area.min.x, area.max.x, release) {
        for y in layers.clone() {
            for z in cells(area.min.z, area.max.z, release) {
                let block = world.block((x, y, z));
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
    if length(flow) > 0.0 && pushed_by_water {
        state.velocity = state.velocity + normalize(flow).scale(0.014);
    }
    state.in_water = touching;
    if touching {
        state.fall_distance = 0.0;
    }
}

/// 1.13's Entity.handleFluidAcceleration: as from 1.14, but a fluid under the same fluid keeps
/// its own height.
#[must_use]
pub(crate) fn fluid_contact_1_13(
    world: &WorldSnapshot,
    fluid: FluidState,
    bounds: Aabb,
) -> FluidContact {
    let area = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
    let mut contact = FluidContact::default();
    for x in area.min.x.floor() as i32..area.max.x.ceil() as i32 {
        for y in area.min.y.floor() as i32..area.max.y.ceil() as i32 {
            for z in area.min.z.floor() as i32..area.max.z.ceil() as i32 {
                let block = world.block((x, y, z));
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

/// Entity.isInLava before 1.14: any lava in the box shrunk by 0.1F sideways and 0.4F vertically.
#[must_use]
pub(crate) fn touching_lava(
    world: &WorldSnapshot,
    bounds: Aabb,
    release: MinecraftVersion,
) -> bool {
    let side = f64::from(0.1_f32);
    let vertical = f64::from(0.4_f32);
    let area = Aabb::new(
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
                .any(|z| world.block((x, y, z)).fluid == FluidState::Lava)
        })
    })
}

/// Entity.isOffsetPositionInLiquid's test of the moved box: no collision and no liquid.
#[must_use]
pub(crate) fn is_free(world: &WorldSnapshot, bounds: Aabb, release: MinecraftVersion) -> bool {
    if release.at_least(1, 9, 0) {
        return world.is_free(bounds);
    }
    let liquid = liquid_cells(bounds.min.x, bounds.max.x).any(|x| {
        liquid_cells(bounds.min.y, bounds.max.y).any(|y| {
            liquid_cells(bounds.min.z, bounds.max.z)
                .any(|z| world.block((x, y, z)).fluid != FluidState::Empty)
        })
    });
    world.no_collision(bounds) && !liquid
}

/// LivingEntity.moveEntityWithHeading's water branch after the move: 0.8F of the vertical motion,
/// less 0.02.
pub(crate) fn sink_in_water(state: &mut SimulationState) {
    state.velocity.y *= f64::from(0.8_f32);
    if !state.no_gravity {
        state.velocity.y -= 0.02;
    }
}

fn cells(min: f64, max: f64, release: MinecraftVersion) -> Range<i32> {
    let end = if release.at_least(1, 9, 0) {
        max.ceil()
    } else {
        (max + 1.0).floor()
    };
    min.floor() as i32..end as i32
}

fn liquid_cells(min: f64, max: f64) -> RangeInclusive<i32> {
    min.floor() as i32..=max.floor() as i32
}

fn height_percent(block: &BlockState) -> f32 {
    let level = if block.fluid_source || block.fluid_falling || block.fluid_level >= 8 {
        0
    } else {
        block.fluid_level
    };
    f32::from(level + 1) / 9.0
}

fn own_height(block: &BlockState) -> f32 {
    let amount =
        if block.fluid_source || block.fluid_falling || !(1..8).contains(&block.fluid_level) {
            8
        } else {
            8 - block.fluid_level
        };
    f32::from(amount) / 9.0
}

fn length(vector: Vec3) -> f64 {
    f64::from(vector.length_squared().sqrt() as f32)
}

fn normalize(vector: Vec3) -> Vec3 {
    let length = length(vector);
    if length < 1.0e-4 {
        Vec3::ZERO
    } else {
        Vec3::new(vector.x / length, vector.y / length, vector.z / length)
    }
}

#[cfg(test)]
mod tests {
    use super::{cells, is_free};
    use crate::math::{Aabb, Vec3};
    use crate::version::MinecraftVersion;
    use crate::world::{BlockState, FluidState, WorldSnapshot};

    #[test]
    fn block_ranges_reach_past_a_whole_maximum_only_in_1_8() {
        assert_eq!(cells(0.5, 2.0, MinecraftVersion::new(1, 8, 8)), 0..3);
        assert_eq!(cells(0.5, 2.0, MinecraftVersion::new(1, 9, 4)), 0..2);
        assert_eq!(cells(0.5, 2.5, MinecraftVersion::new(1, 8, 8)), 0..3);
        assert_eq!(cells(0.5, 2.5, MinecraftVersion::new(1, 9, 4)), 0..3);
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

        assert!(!is_free(&world, bounds, MinecraftVersion::new(1, 8, 8)));
        assert!(is_free(&world, bounds, MinecraftVersion::new(1, 9, 4)));
    }
}
