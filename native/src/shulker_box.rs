//! A shulker box's lid: ShulkerBoxBlockEntity's animation, the collision box it gives the cell and
//! the push of an opening lid. The block event (`triggerEvent(1, n)`, the server's) is the caller's
//! input, and the tick is the block entity's. The box is the same on the jars of 1.14.4 to 26.3
//! (scripts/java-oracle/move/OracleShulker.java); 1.11 to 1.13.2 have the same source form and no
//! jar with names to run.

use crate::math::{Aabb, Vec3};
use crate::piston::move_entity;
use crate::simulation::{kept_bounding_box, MoverType, PhysicsConfig, SimulationState};
use crate::version::MinecraftVersion;
use crate::world::WorldSnapshot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShulkerBoxStatus {
    #[default]
    Closed,
    Opening,
    Opened,
    Closing,
}

/// The typed state of the block entity: no inventory, no colour, nothing the movement does not read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShulkerBox {
    /// The block's FACING as a Direction's ordinal: down, up, north, south, west, east.
    pub facing: u8,
    pub status: ShulkerBoxStatus,
    pub progress: f32,
    pub progress_old: f32,
    /// The progress its push reads before 1.17: the last step of an opening pushes before the
    /// progress is clamped to 1.
    pub push_progress: f32,
}

impl ShulkerBox {
    /// FACING's unit step: Direction#getStepX, Direction#getStepY and Direction#getStepZ of the
    /// Direction of that ordinal.
    fn step(self) -> Vec3 {
        match self.facing {
            0 => Vec3::new(0.0, -1.0, 0.0),
            1 => Vec3::new(0.0, 1.0, 0.0),
            2 => Vec3::new(0.0, 0.0, -1.0),
            3 => Vec3::new(0.0, 0.0, 1.0),
            4 => Vec3::new(-1.0, 0.0, 0.0),
            _ => Vec3::new(1.0, 0.0, 0.0),
        }
    }

    /// ShulkerBoxBlockEntity.triggerEvent(1, count): only 0 and 1 change the animation
    /// (26.3 ShulkerBoxBlockEntity.java:145-157).
    pub fn block_event(&mut self, count: i32) {
        match count {
            0 => self.status = ShulkerBoxStatus::Closing,
            1 => self.status = ShulkerBoxStatus::Opening,
            _ => {}
        }
    }

    /// updateAnimation (26.3 :78-114); whether its lid pushes what it meets this tick. From 1.17
    /// only an opening lid pushes. Before, tick pushes in either direction while the lid moves, and
    /// updateAnimation itself on the last step of an opening, unclamped (1.16.5
    /// ShulkerBoxBlockEntity.java:65-85, 1.17 :66-85).
    pub fn tick(&mut self, release: MinecraftVersion) -> bool {
        let legacy = !release.at_least(1, 17, 0);
        self.progress_old = self.progress;
        match self.status {
            ShulkerBoxStatus::Closed => self.progress = 0.0,
            ShulkerBoxStatus::Opened => self.progress = 1.0,
            ShulkerBoxStatus::Opening => {
                self.progress += 0.1;
                self.push_progress = self.progress;
                if self.progress >= 1.0 {
                    self.status = ShulkerBoxStatus::Opened;
                    self.progress = 1.0;
                }
                return true;
            }
            ShulkerBoxStatus::Closing => {
                self.progress -= 0.1;
                self.push_progress = self.progress;
                if self.progress <= 0.0 {
                    self.status = ShulkerBoxStatus::Closed;
                    self.progress = 0.0;
                    return false;
                }
                return legacy;
            }
        }
        false
    }

    /// ShulkerBoxBlockEntity#getBoundingBox: the cell's cube, stretched by half the progress, as a
    /// float, towards FACING.
    #[must_use]
    pub fn bounds(self) -> Aabb {
        let cube = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        cube.expand_towards(self.step().scale(f64::from(0.5_f32 * self.progress)))
    }

    /// Shulker.getProgressDeltaAabb(1.0F, facing, progressOld, progress, the cell's bottom centre),
    /// in which the peek is `progress`, not halved (26.3 Shulker.java:250-257).
    fn delta_bounds(self, cell: (i32, i32, i32)) -> Aabb {
        let step = self.step();
        let most = f64::from(self.progress_old.max(self.progress));
        let least = f64::from(self.progress_old.min(self.progress));
        let centred = Aabb::new(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5));
        let back = Vec3::new(-step.x, -step.y, -step.z).scale(1.0 + least);
        centred
            .expand_towards(step.scale(most))
            .contract(back)
            .translated(Vec3::new(
                f64::from(cell.0) + 0.5,
                f64::from(cell.1),
                f64::from(cell.2) + 0.5,
            ))
    }
}

/// pushEntities before 1.17: the lid's slab past the cell, this tick's, and each entity it reaches
/// moves to its far face and 0.01 on (1.16.5 ShulkerBoxBlockEntity.java:120-163).
fn legacy_push(shulker: ShulkerBox, cell: (i32, i32, i32), entity: Aabb) -> Option<Vec3> {
    let step = shulker.step();
    // getAnimationProgress(1.0F): the lerp's float sum from the last tick
    let progress = shulker.progress_old + (shulker.push_progress - shulker.progress_old);
    let lid = ShulkerBox { progress, ..shulker }
        .bounds()
        .contract(Vec3::new(-step.x, -step.y, -step.z))
        .translated(Vec3::new(f64::from(cell.0), f64::from(cell.1), f64::from(cell.2)));
    if !entity.intersects(lid) {
        return None;
    }
    let on_axis = |step: f64, lid_min: f64, lid_max: f64, min: f64, max: f64| {
        if step > 0.0 {
            lid_max - min + 0.01
        } else if step < 0.0 {
            max - lid_min + 0.01
        } else {
            0.0
        }
    };
    Some(Vec3::new(
        on_axis(step.x, lid.min.x, lid.max.x, entity.min.x, entity.max.x) * step.x,
        on_axis(step.y, lid.min.y, lid.max.y, entity.min.y, entity.max.y) * step.y,
        on_axis(step.z, lid.min.z, lid.max.z, entity.min.z, entity.max.z) * step.z,
    ))
}

/// moveCollidedEntities for an entity: moved by the swept box's size and 0.01 along FACING with
/// MoverType.SHULKER_BOX, where the box reaches it (26.3 ShulkerBoxBlockEntity.java:125-139), from
/// 1.17; `legacy_push` before. Whether it moved. (1.16.5 ShulkerBoxBlockEntity.java:117-160 moves it
/// out of the lid's top box; 1.17 ShulkerBoxBlockEntity.java:104-115 by the progress delta box's size.)
pub fn push_entity(
    state: &mut SimulationState,
    world: &WorldSnapshot,
    shulker: ShulkerBox,
    cell: (i32, i32, i32),
    config: PhysicsConfig,
    release: MinecraftVersion,
) -> bool {
    if !release.at_least(1, 17, 0) {
        // the entity keeps a box before 1.17, and a move sets the position from its centre
        state.bounding_box = Some(kept_bounding_box(state));
        let Some(delta) = legacy_push(shulker, cell, state.bounds()) else {
            return false;
        };
        move_entity(state, world, delta, config, release, MoverType::ShulkerBox);
        return true;
    }
    let swept = shulker.delta_bounds(cell);
    if !state.bounds().intersects(swept) {
        return false;
    }
    let step = shulker.step();
    let size = swept.max - swept.min;
    let delta = Vec3::new(
        (size.x + 0.01) * step.x,
        (size.y + 0.01) * step.y,
        (size.z + 0.01) * step.z,
    );
    move_entity(state, world, delta, config, release, MoverType::ShulkerBox);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const R1_16_5: MinecraftVersion = MinecraftVersion::new(1, 16, 5);
    const R1_17: MinecraftVersion = MinecraftVersion::new(1, 17, 0);

    #[test]
    fn a_closing_lid_pushes_until_1_16_5_and_not_from_1_17() {
        // 1.16.5 ShulkerBoxBlockEntity.tick moves what it meets while OPENING or CLOSING
        // (ShulkerBoxBlockEntity.java:64-69); 1.17 updateAnimation pushes in OPENING alone (:72-92)
        let closing = |release| {
            let mut lid = ShulkerBox { facing: 1, ..ShulkerBox::default() };
            lid.block_event(1);
            for _ in 0..11 {
                lid.tick(release);
            }
            lid.block_event(0);
            lid.tick(release)
        };
        assert!(closing(R1_16_5));
        assert!(!closing(R1_17));
    }

    #[test]
    fn an_opening_lid_meets_what_is_over_its_whole_step_from_1_17() {
        // 1.16.5 pushes out of the lid's slab past the cell, half the progress high (1.16.5
        // ShulkerBoxBlockEntity.java:117-160); 1.17 moves what meets getProgressDeltaAabb of the last
        // two progresses, not halved (1.17 ShulkerBoxBlockEntity.java:104-115, Shulker.java:248-253):
        // over a lid going from 0.4 to 0.5, feet at 1.3 are past the first's 1.25 and in the second's
        // 1.4 to 1.5
        let mut world = WorldSnapshot::new();
        let cube = crate::world::BlockShape::full_block();
        world.set_block_shapes((0, 0, 0), crate::world::BlockState { id: 1, shape: Some(cube), ..crate::world::BlockState::default() }, vec![cube]);
        let lid = ShulkerBox {
            facing: 1,
            status: ShulkerBoxStatus::Opening,
            progress: 0.5,
            progress_old: 0.4,
            push_progress: 0.5,
        };
        let pushed = |release| {
            let mut state = SimulationState {
                position: Vec3::new(0.5, 1.3, 0.5),
                half_width: f64::from(0.6_f32 / 2.0),
                height: f64::from(1.8_f32),
                non_living: true,
                controlled_by_local_instance: true,
                ..SimulationState::default()
            };
            (push_entity(&mut state, &world, lid, (0, 0, 0), PhysicsConfig::default(), release), state.position.y)
        };
        assert_eq!(pushed(R1_16_5), (false, 1.3));
        let (moved, y) = pushed(R1_17);
        assert!(moved);
        assert!(y > 1.3, "{y}");
    }
}
