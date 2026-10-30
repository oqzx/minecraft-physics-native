use std::collections::{HashMap, HashSet};

use crate::collision::{reaches_into, BlockVoxelShape};
use crate::math::{Aabb, Vec3};
use crate::version::MinecraftVersion;
use crate::voxel_shape::VoxelShape;

type BlockPosition = (i32, i32, i32);
type ChunkPosition = (i32, i32);
type ChunkIndex = HashMap<ChunkPosition, HashSet<BlockPosition>>;
type SectionPosition = (i32, i32, i32);
type SectionIndex = HashMap<SectionPosition, HashSet<BlockPosition>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionedBlockStateId {
    pub version: crate::version::MinecraftVersion,
    pub id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockShape {
    pub bounds: Aabb,
    pub slipperiness: f64,
    pub fluid: FluidState,
    pub effect: BlockEffect,
}

impl BlockShape {
    #[must_use]
    pub const fn full_block() -> Self {
        Self {
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
            slipperiness: 0.6,
            fluid: FluidState::Empty,
            effect: BlockEffect::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BlockEffect {
    #[default]
    None,
    SoulSand,
    SoulSoil,
    Honey,
    Slime,
    Web,
    PowderSnow,
    SweetBerryBush,
    Climbable,
    Scaffolding,
    Cactus,
    Magma,
    Campfire,
    SoulCampfire,
    Fire,
    SoulFire,
    BubbleUp,
    BubbleDown,
    Bed,
}

/// What vanilla's EntityCollisionContext tells the blocks whose collision shape depends on the
/// entity (powder snow, scaffolding): its feet when the context is made, whether it descends (the
/// sneak key), its fall distance and whether it can walk on powder snow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityCollisionContext {
    pub bottom: f64,
    pub descending: bool,
    pub fall_distance: f64,
    pub walks_on_powder_snow: bool,
}

impl EntityCollisionContext {
    fn is_above(&self, shape_top: f64, y: i32) -> bool {
        self.bottom > f64::from(y) + shape_top - 9.999999747378752e-6
    }
}

enum EntityCollisionShape {
    Stored,
    Empty,
    Box(Aabb),
}

/// An entity a move collides with (Entity.canBeCollidedWith): its box, and whether it is solid
/// only to an entity whose feet are at or above its top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityCollider {
    pub bounds: Aabb,
    pub only_from_above: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FluidContact {
    pub touching: bool,
    pub height: f64,
    pub flow: Vec3,
    pub flowing_blocks: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FluidState {
    #[default]
    Empty,
    Water,
    Lava,
    Waterlogged,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlockState {
    pub id: u32,
    pub tags: u32,
    pub fluid_tags: u32,
    pub shape: Option<BlockShape>,
    pub outline_shape: Option<BlockShape>,
    pub interaction_shape: Option<BlockShape>,
    pub bounce_restitution: f64,
    pub suppresses_bounce: bool,
    pub fluid: FluidState,
    pub fluid_level: u8,
    pub fluid_source: bool,
    pub fluid_flow: Vec3,
    pub fluid_falling: bool,
    pub fall_damage_resetting: bool,
    pub suffocating: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct WorldSnapshot {
    blocks: HashMap<BlockPosition, BlockState>,
    properties: HashMap<BlockPosition, Vec<(String, String)>>,
    state_ids: HashMap<BlockPosition, VersionedBlockStateId>,
    block_entities: HashMap<BlockPosition, String>,
    extra_shapes: HashMap<BlockPosition, Vec<BlockShape>>,
    extra_outline_shapes: HashMap<BlockPosition, Vec<BlockShape>>,
    extra_interaction_shapes: HashMap<BlockPosition, Vec<BlockShape>>,
    chunks: ChunkIndex,
    sections: SectionIndex,
    explicit_sections: HashSet<SectionPosition>,
    loaded_chunks: HashSet<ChunkPosition>,
    min_build_height: i32,
    max_build_height: i32,
    border_center: (f64, f64),
    border_size: f64,
    entity_colliders: Vec<EntityCollider>,
}

const WORLD_BORDER_DEFAULT_SIZE: f64 = 5.9999968e7;
const WORLD_BORDER_ABSOLUTE_MAX: f64 = 29999984.0;

impl Default for WorldSnapshot {
    fn default() -> Self {
        Self {
            blocks: HashMap::new(),
            properties: HashMap::new(),
            state_ids: HashMap::new(),
            block_entities: HashMap::new(),
            extra_shapes: HashMap::new(),
            extra_outline_shapes: HashMap::new(),
            extra_interaction_shapes: HashMap::new(),
            chunks: ChunkIndex::new(),
            sections: SectionIndex::new(),
            explicit_sections: HashSet::new(),
            loaded_chunks: HashSet::new(),
            min_build_height: 0,
            max_build_height: 0,
            border_center: (0.0, 0.0),
            border_size: WORLD_BORDER_DEFAULT_SIZE,
            entity_colliders: Vec::new(),
        }
    }
}

impl WorldSnapshot {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_entity_colliders(&mut self, colliders: Vec<EntityCollider>) {
        self.entity_colliders = colliders;
    }

    /// The boxes of the entity colliders that meet the query (Level.getEntityCollisions), for an
    /// entity whose feet are at the context's bottom.
    #[must_use]
    pub fn entity_collisions(&self, query: Aabb, context: &EntityCollisionContext) -> Vec<Aabb> {
        self.entity_colliders
            .iter()
            .filter(|collider| !collider.only_from_above || context.bottom >= collider.bounds.max.y)
            .map(|collider| collider.bounds)
            .filter(|bounds| bounds.intersects(query))
            .collect()
    }

    pub fn set_block(&mut self, position: (i32, i32, i32), state: BlockState) {
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        self.loaded_chunks.insert(chunk);
        self.extra_shapes.remove(&position);
        self.extra_outline_shapes.remove(&position);
        self.extra_interaction_shapes.remove(&position);
        self.blocks.insert(position, state);
        self.chunks.entry(chunk).or_default().insert(position);
        let section = (chunk.0, position.1.div_euclid(16), chunk.1);
        self.sections.entry(section).or_default().insert(position);
    }

    pub fn set_block_properties(
        &mut self,
        position: (i32, i32, i32),
        properties: Vec<(String, String)>,
    ) {
        if properties.is_empty() {
            self.properties.remove(&position);
        } else {
            self.properties.insert(position, properties);
        }
    }

    pub fn set_block_state_id(
        &mut self,
        position: (i32, i32, i32),
        state_id: VersionedBlockStateId,
    ) {
        self.state_ids.insert(position, state_id);
    }

    #[must_use]
    pub fn block_state_id(&self, position: (i32, i32, i32)) -> Option<VersionedBlockStateId> {
        self.state_ids.get(&position).copied()
    }

    pub fn set_block_entity(&mut self, position: (i32, i32, i32), kind: String) {
        self.block_entities.insert(position, kind);
    }

    pub fn remove_block_entity(&mut self, position: (i32, i32, i32)) {
        self.block_entities.remove(&position);
    }

    #[must_use]
    pub fn block_entity(&self, position: (i32, i32, i32)) -> Option<&str> {
        self.block_entities.get(&position).map(String::as_str)
    }

    #[must_use]
    pub fn block_properties(&self, position: (i32, i32, i32)) -> Vec<(String, String)> {
        self.properties.get(&position).cloned().unwrap_or_default()
    }

    pub fn remove_block(&mut self, position: (i32, i32, i32)) {
        self.extra_shapes.remove(&position);
        self.properties.remove(&position);
        self.state_ids.remove(&position);
        self.block_entities.remove(&position);
        self.blocks.remove(&position);
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        if let Some(positions) = self.chunks.get_mut(&chunk) {
            positions.remove(&position);
            if positions.is_empty() {
                self.chunks.remove(&chunk);
            }
            let section = (chunk.0, position.1.div_euclid(16), chunk.1);
            if let Some(positions) = self.sections.get_mut(&section) {
                positions.remove(&position);
                if positions.is_empty() {
                    self.sections.remove(&section);
                    self.explicit_sections.remove(&section);
                }
            }
        }
    }

    #[must_use]
    pub fn contains_block(&self, position: (i32, i32, i32)) -> bool {
        self.blocks.contains_key(&position)
    }

    #[must_use]
    pub fn is_section_present(&self, chunk_x: i32, section_y: i32, chunk_z: i32) -> bool {
        let section = (chunk_x, section_y, chunk_z);
        self.explicit_sections.contains(&section) || self.sections.contains_key(&section)
    }

    pub fn set_section_present(
        &mut self,
        chunk_x: i32,
        section_y: i32,
        chunk_z: i32,
        present: bool,
    ) {
        let section = (chunk_x, section_y, chunk_z);
        if present {
            self.explicit_sections.insert(section);
        } else {
            self.explicit_sections.remove(&section);
        }
    }

    #[must_use]
    pub fn section_block_count(&self, chunk_x: i32, section_y: i32, chunk_z: i32) -> usize {
        self.sections
            .get(&(chunk_x, section_y, chunk_z))
            .map_or(0, HashSet::len)
    }

    pub fn set_chunk_loaded(&mut self, chunk_x: i32, chunk_z: i32, loaded: bool) {
        let chunk = (chunk_x, chunk_z);
        if loaded {
            self.loaded_chunks.insert(chunk);
        } else {
            self.loaded_chunks.remove(&chunk);
        }
    }

    #[must_use]
    pub fn is_chunk_loaded(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.loaded_chunks.contains(&(chunk_x, chunk_z))
    }

    pub fn set_world_bounds(
        &mut self,
        min_build_height: i32,
        max_build_height: i32,
        border_center_x: f64,
        border_center_z: f64,
        border_size: f64,
    ) {
        self.min_build_height = min_build_height;
        self.max_build_height = max_build_height;
        self.border_center = (border_center_x, border_center_z);
        self.border_size = border_size.max(0.0);
    }

    #[must_use]
    pub fn world_bounds(&self) -> (i32, i32, f64, f64, f64) {
        (
            self.min_build_height,
            self.max_build_height,
            self.border_center.0,
            self.border_center.1,
            self.border_size,
        )
    }

    #[must_use]
    pub fn is_inside_world_border(&self, position: Vec3) -> bool {
        let half = self.border_size * 0.5;
        position.x >= self.border_center.0 - half
            && position.x < self.border_center.0 + half
            && position.z >= self.border_center.1 - half
            && position.z < self.border_center.1 + half
    }

    /// WorldBorder.getMinX / getMaxX / getMinZ / getMaxZ.
    fn world_border_edges(&self) -> (f64, f64, f64, f64) {
        let half = self.border_size / 2.0;
        let edge = |value: f64| value.clamp(-WORLD_BORDER_ABSOLUTE_MAX, WORLD_BORDER_ABSOLUTE_MAX);
        let (center_x, center_z) = self.border_center;
        (
            edge(center_x - half),
            edge(center_x + half),
            edge(center_z - half),
            edge(center_z + half),
        )
    }

    /// WorldBorder.isInsideCloseToBorder: the entity at `position` sweeping `swept` is inside the
    /// border and close to it.
    #[must_use]
    pub fn is_inside_close_to_world_border(&self, position: Vec3, swept: Aabb) -> bool {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        let size = swept.max - swept.min;
        let margin = size.x.abs().max(size.z.abs()).max(1.0);
        let distance = (position.x - min_x)
            .min(max_x - position.x)
            .min(position.z - min_z)
            .min(max_z - position.z);
        distance < margin * 2.0
            && position.x >= min_x - margin
            && position.x < max_x + margin
            && position.z >= min_z - margin
            && position.z < max_z + margin
    }

    /// WorldBorder.getCollisionShape: everything outside the border's block-aligned edges.
    #[must_use]
    pub fn world_border_collision_boxes(&self) -> [Aabb; 4] {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        let (min_x, max_x) = (min_x.floor(), max_x.ceil());
        let (min_z, max_z) = (min_z.floor(), max_z.ceil());
        let infinity = f64::INFINITY;
        let lowest = Vec3::new(-infinity, -infinity, -infinity);
        let highest = Vec3::new(infinity, infinity, infinity);
        [
            Aabb::new(lowest, Vec3::new(min_x, infinity, infinity)),
            Aabb::new(Vec3::new(max_x, -infinity, -infinity), highest),
            Aabb::new(lowest, Vec3::new(infinity, infinity, min_z)),
            Aabb::new(Vec3::new(-infinity, -infinity, max_z), highest),
        ]
    }

    #[must_use]
    pub fn is_inside_build_height(&self, y: i32) -> bool {
        y >= self.min_build_height && y < self.max_build_height
    }

    #[must_use]
    pub fn clamp_to_world_border(&self, position: Vec3) -> Vec3 {
        let half = self.border_size * 0.5;
        Vec3::new(
            position
                .x
                .clamp(self.border_center.0 - half, self.border_center.0 + half - 1.0e-5),
            position.y,
            position
                .z
                .clamp(self.border_center.1 - half, self.border_center.1 + half - 1.0e-5),
        )
    }

    pub fn set_block_shapes(
        &mut self,
        position: (i32, i32, i32),
        state: BlockState,
        shapes: Vec<BlockShape>,
    ) {
        self.set_block_shapes_with_geometry(position, state, shapes.clone(), shapes.clone(), shapes);
    }

    pub fn set_block_shapes_with_outline(
        &mut self,
        position: (i32, i32, i32),
        state: BlockState,
        shapes: Vec<BlockShape>,
        outline_shapes: Vec<BlockShape>,
    ) {
        self.set_block_shapes_with_geometry(position, state, shapes, outline_shapes.clone(), outline_shapes);
    }

    pub fn set_block_shapes_with_geometry(
        &mut self,
        position: (i32, i32, i32),
        mut state: BlockState,
        shapes: Vec<BlockShape>,
        outline_shapes: Vec<BlockShape>,
        interaction_shapes: Vec<BlockShape>,
    ) {
        state.shape = shapes.first().copied();
        let extra_shapes = shapes.into_iter().skip(1).collect::<Vec<_>>();
        state.outline_shape = outline_shapes.first().copied();
        let extra_outline_shapes = outline_shapes.into_iter().skip(1).collect::<Vec<_>>();
        state.interaction_shape = interaction_shapes.first().copied();
        let extra_interaction_shapes = interaction_shapes.into_iter().skip(1).collect::<Vec<_>>();
        self.set_block(position, state);
        if !extra_shapes.is_empty() {
            self.extra_shapes.insert(position, extra_shapes);
        }
        if !extra_outline_shapes.is_empty() {
            self.extra_outline_shapes.insert(position, extra_outline_shapes);
        }
        if !extra_interaction_shapes.is_empty() {
            self.extra_interaction_shapes.insert(position, extra_interaction_shapes);
        }
    }

    pub fn set_block_voxel_shape(
        &mut self,
        position: (i32, i32, i32),
        state: BlockState,
        shape: &VoxelShape,
    ) {
        if shape.boxes().is_empty() {
            self.set_block(
                position,
                BlockState {
                    shape: None,
                    ..state
                },
            );
            return;
        }
        let template = state.shape.unwrap_or(BlockShape {
            bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
            slipperiness: 0.6,
            fluid: state.fluid,
            effect: BlockEffect::None,
        });
        let shapes = shape
            .boxes()
            .iter()
            .map(|bounds| BlockShape {
                bounds: *bounds,
                ..template
            })
            .collect();
        self.set_block_shapes(position, state, shapes);
    }

    #[must_use]
    pub fn block(&self, position: (i32, i32, i32)) -> BlockState {
        self.blocks.get(&position).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn fluid_at(&self, position: (i32, i32, i32)) -> FluidState {
        self.block(position).fluid
    }

    #[must_use]
    pub fn fluid_height_at(&self, position: (i32, i32, i32)) -> f64 {
        let block = self.block(position);
        if block.fluid == FluidState::Empty {
            return 0.0;
        }
        let above = self.block((position.0, position.1 + 1, position.2));
        if fluids_match(block.fluid, above.fluid) {
            return 1.0;
        }
        let amount =
            if block.fluid_source || block.fluid_falling || !(1..8).contains(&block.fluid_level) {
                8
            } else {
                8 - block.fluid_level
            };
        f64::from(f32::from(amount) / 9.0)
    }

    #[must_use]
    pub fn fluid_contact(
        &self,
        fluid: FluidState,
        bounds: Aabb,
        release: MinecraftVersion,
    ) -> FluidContact {
        let area = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
        let interaction = release.at_least(26, 1, 0);
        let bottom = if interaction {
            bounds.min.y
        } else {
            area.min.y
        };
        let mut contact = FluidContact::default();
        for x in area.min.x.floor() as i32..area.max.x.ceil() as i32 {
            for y in area.min.y.floor() as i32..area.max.y.ceil() as i32 {
                for z in area.min.z.floor() as i32..area.max.z.ceil() as i32 {
                    let block = self.block((x, y, z));
                    if !fluid_matches_query(fluid, block.fluid) {
                        continue;
                    }
                    let height = self.fluid_height_at((x, y, z));
                    let top = if interaction {
                        f64::from(y) + height
                    } else {
                        f64::from(y as f32 + height as f32)
                    };
                    if top < area.min.y {
                        continue;
                    }
                    contact.touching = true;
                    contact.height = contact.height.max(top - bottom);
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
        if interaction {
            contact.touching = contact.height > 0.0;
        }
        contact
    }

    #[must_use]
    pub fn has_fluid_block_inside(&self, fluid: FluidState, bounds: Aabb) -> bool {
        let area = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
        (area.min.x.floor() as i32..=area.max.x.floor() as i32).any(|x| {
            (area.min.y.floor() as i32..=area.max.y.floor() as i32).any(|y| {
                (area.min.z.floor() as i32..=area.max.z.floor() as i32)
                    .any(|z| fluid_matches_query(fluid, self.block((x, y, z)).fluid))
            })
        })
    }

    #[must_use]
    pub fn fluid_flow_at(&self, position: (i32, i32, i32)) -> Vec3 {
        self.block(position).fluid_flow
    }

    #[must_use]
    pub fn fluid_height_in_bounds(&self, fluid: FluidState, bounds: Aabb) -> f64 {
        let min_x = (bounds.min.x + 0.001).floor() as i32;
        let max_x = (bounds.max.x - 0.001).ceil() as i32;
        let min_y = (bounds.min.y + 0.001).floor() as i32;
        let max_y = (bounds.max.y - 0.001).ceil() as i32;
        let min_z = (bounds.min.z + 0.001).floor() as i32;
        let max_z = (bounds.max.z - 0.001).ceil() as i32;
        let mut height: f64 = 0.0;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let block = self.block((x, y, z));
                    let matches = fluid_matches_query(fluid, block.fluid);
                    if matches {
                        let top = y as f64 + self.fluid_height_at((x, y, z));
                        if top >= bounds.min.y {
                            height = height.max(top - bounds.min.y);
                        }
                    }
                }
            }
        }
        height
    }

    #[must_use]
    pub fn water_surface_above(&self, bounds: Aabb) -> Option<f64> {
        let min_x = (bounds.min.x + 0.001).floor() as i32;
        let max_x = (bounds.max.x - 0.001).ceil() as i32;
        let min_z = (bounds.min.z + 0.001).floor() as i32;
        let max_z = (bounds.max.z - 0.001).ceil() as i32;
        let start_y = bounds.max.y.floor() as i32;
        let end_y = start_y + 8;
        for y in start_y..=end_y {
            let mut surface = None;
            for x in min_x..max_x {
                for z in min_z..max_z {
                    let block = self.block((x, y, z));
                    if fluid_matches_query(FluidState::Water, block.fluid) {
                        let top = y as f64 + self.fluid_height_at((x, y, z));
                        surface = Some(surface.map_or(top, |current: f64| current.max(top)));
                    }
                }
            }
            if surface.is_some() {
                return surface;
            }
        }
        None
    }

    #[must_use]
    pub fn has_non_source_fluid_in_bounds(&self, fluid: FluidState, bounds: Aabb) -> bool {
        let min_x = (bounds.min.x + 0.001).floor() as i32;
        let max_x = (bounds.max.x - 0.001).ceil() as i32;
        let min_y = (bounds.min.y + 0.001).floor() as i32;
        let max_y = (bounds.max.y - 0.001).ceil() as i32;
        let min_z = (bounds.min.z + 0.001).floor() as i32;
        let max_z = (bounds.max.z - 0.001).ceil() as i32;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let block = self.block((x, y, z));
                    if fluid_matches_query(fluid, block.fluid) && !block.fluid_source {
                        let top = y as f64 + self.fluid_height_at((x, y, z));
                        if top >= bounds.min.y {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    #[must_use]
    pub fn slipperiness_at(&self, position: (i32, i32, i32)) -> f64 {
        self.block(position)
            .shape
            .map_or(0.6, |shape| shape.slipperiness)
    }

    #[must_use]
    pub fn effect_at(&self, position: (i32, i32, i32)) -> BlockEffect {
        self.block(position)
            .shape
            .map_or(BlockEffect::None, |shape| shape.effect)
    }

    #[must_use]
    pub fn supporting_block_effect(&self, bounds: Aabb) -> BlockEffect {
        self.supporting_block_state(bounds)
            .shape
            .map_or(BlockEffect::None, |shape| shape.effect)
    }

    #[must_use]
    pub fn supporting_block_state(&self, bounds: Aabb) -> BlockState {
        let support_area = Aabb::new(
            Vec3::new(bounds.min.x, bounds.min.y - 1.0e-6, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
        );
        let min_x = support_area.min.x.floor() as i32;
        let max_x = support_area.max.x.ceil() as i32;
        let y = support_area.min.y.floor() as i32;
        let min_z = support_area.min.z.floor() as i32;
        let max_z = support_area.max.z.ceil() as i32;
        for x in min_x..max_x {
            for z in min_z..max_z {
                let position = (x, y, z);
                let block = self.block(position);
                let offset = Vec3::new(x as f64, y as f64, z as f64);
                if let Some(shape) = block.shape {
                    if shape.bounds.translated(offset).intersects(support_area) {
                        return block;
                    }
                    if shape.effect != BlockEffect::None
                        && !is_non_empty_shape(shape.bounds)
                        && support_area.min.x < (x + 1) as f64
                        && support_area.max.x > x as f64
                        && support_area.min.z < (z + 1) as f64
                        && support_area.max.z > z as f64
                    {
                        return block;
                    }
                }
                if let Some(shapes) = self.extra_shapes.get(&position) {
                    for shape in shapes {
                        if shape.bounds.translated(offset).intersects(support_area) {
                            return BlockState {
                                shape: Some(*shape),
                                ..block
                            };
                        }
                    }
                }
            }
        }
        BlockState::default()
    }

    #[must_use]
    pub fn effects_in_bounds(&self, query: Aabb) -> Vec<BlockEffect> {
        let query = query.deflate(Vec3::new(1.0e-5, 1.0e-5, 1.0e-5));
        let min_x = query.min.x.floor() as i32;
        let max_x = query.max.x.ceil() as i32;
        let min_y = query.min.y.floor() as i32;
        let max_y = query.max.y.ceil() as i32;
        let min_z = query.min.z.floor() as i32;
        let max_z = query.max.z.ceil() as i32;
        let mut effects = Vec::new();
        for y in min_y..max_y {
            for z in min_z..max_z {
                for x in min_x..max_x {
                    let effect = self.effect_at((x, y, z));
                    if effect != BlockEffect::None {
                        effects.push(effect);
                    }
                }
            }
        }
        effects
    }

    #[must_use]
    pub fn has_collision_shape(&self, position: (i32, i32, i32)) -> bool {
        self.block_collision_shapes(position)
            .any(|shape| is_non_empty_shape(shape.bounds))
    }

    /// The collision boxes of the block at a position, moved there.
    #[must_use]
    pub fn block_collision_boxes(&self, position: (i32, i32, i32)) -> Vec<Aabb> {
        let offset = Vec3::new(
            f64::from(position.0),
            f64::from(position.1),
            f64::from(position.2),
        );
        self.block_collision_shapes(position)
            .filter(|shape| is_non_empty_shape(shape.bounds))
            .map(|shape| shape.bounds.translated(offset))
            .collect()
    }

    #[must_use]
    pub fn is_air_at(&self, position: (i32, i32, i32)) -> bool {
        let block = self.block(position);
        block.id == 0 && block.shape.is_none() && block.fluid == FluidState::Empty
    }

    #[must_use]
    pub fn collision_shapes(&self, query: Aabb) -> Vec<Aabb> {
        let mut shapes = Vec::new();
        self.collision_shapes_into(query, &mut shapes);
        shapes
    }

    #[must_use]
    pub fn outline_shapes(&self, query: Aabb) -> Vec<Aabb> {
        let min_x = query.min.x.floor() as i32;
        let max_x = query.max.x.ceil() as i32;
        let min_y = query.min.y.floor() as i32;
        let max_y = query.max.y.ceil() as i32;
        let min_z = query.min.z.floor() as i32;
        let max_z = query.max.z.ceil() as i32;
        let mut result = Vec::new();
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    if !self.is_chunk_loaded(x.div_euclid(16), z.div_euclid(16)) {
                        continue;
                    }
                    let block = self.block((x, y, z));
                    if let Some(shape) = block.outline_shape {
                        let bounds = shape.bounds.translated(Vec3::new(x as f64, y as f64, z as f64));
                        if bounds.intersects(query) && is_non_empty_shape(shape.bounds) {
                            result.push(bounds);
                        }
                    }
                    if let Some(extra_shapes) = self.extra_outline_shapes.get(&(x, y, z)) {
                        let offset = Vec3::new(x as f64, y as f64, z as f64);
                        for shape in extra_shapes {
                            let bounds = shape.bounds.translated(offset);
                            if bounds.intersects(query) && is_non_empty_shape(shape.bounds) {
                                result.push(bounds);
                            }
                        }
                    }
                }
            }
        }
        result.sort_by(|left, right| {
            left.min.x.total_cmp(&right.min.x)
                .then_with(|| left.min.y.total_cmp(&right.min.y))
                .then_with(|| left.min.z.total_cmp(&right.min.z))
        });
        result
    }

    #[must_use]
    pub fn interaction_shapes(&self, query: Aabb) -> Vec<Aabb> {
        let min_x = query.min.x.floor() as i32;
        let max_x = query.max.x.ceil() as i32;
        let min_y = query.min.y.floor() as i32;
        let max_y = query.max.y.ceil() as i32;
        let min_z = query.min.z.floor() as i32;
        let max_z = query.max.z.ceil() as i32;
        let mut result = Vec::new();
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    if !self.is_chunk_loaded(x.div_euclid(16), z.div_euclid(16)) {
                        continue;
                    }
                    let offset = Vec3::new(x as f64, y as f64, z as f64);
                    if let Some(shape) = self.block((x, y, z)).interaction_shape {
                        let bounds = shape.bounds.translated(offset);
                        if bounds.intersects(query) && is_non_empty_shape(shape.bounds) {
                            result.push(bounds);
                        }
                    }
                    if let Some(extra_shapes) = self.extra_interaction_shapes.get(&(x, y, z)) {
                        result.extend(extra_shapes.iter().filter_map(|shape| {
                            let bounds = shape.bounds.translated(offset);
                            (bounds.intersects(query) && is_non_empty_shape(shape.bounds))
                                .then_some(bounds)
                        }));
                    }
                }
            }
        }
        result.sort_by(|left, right| {
            left.min.x.total_cmp(&right.min.x)
                .then_with(|| left.min.y.total_cmp(&right.min.y))
                .then_with(|| left.min.z.total_cmp(&right.min.z))
        });
        result
    }

    #[must_use]
    pub fn collision_shapes_for_entity(
        &self,
        query: Aabb,
        context: &EntityCollisionContext,
    ) -> Vec<Aabb> {
        let mut shapes = Vec::new();
        self.collect_collision_shapes(query, Some(context), &mut shapes, None);
        shapes
    }

    /// collision_shapes_for_entity, with the whole shapes of the blocks among them the entity's
    /// box is inside.
    #[must_use]
    pub fn collision_shapes_with_entered_for_entity(
        &self,
        query: Aabb,
        entity: Aabb,
        context: &EntityCollisionContext,
    ) -> (Vec<Aabb>, Vec<BlockVoxelShape>) {
        let mut shapes = Vec::new();
        let mut entered = Vec::new();
        let inside = Some((entity, &mut entered));
        self.collect_collision_shapes(query, Some(context), &mut shapes, inside);
        (shapes, entered)
    }

    #[must_use]
    pub fn no_collision_for_entity(&self, bounds: Aabb, context: &EntityCollisionContext) -> bool {
        self.collision_shapes_for_entity(bounds, context).is_empty()
    }

    /// BlockState.getCollisionShape for an entity: the boxes of one block, relative to the block.
    #[must_use]
    pub fn block_collision_boxes_for_entity(
        &self,
        position: BlockPosition,
        context: &EntityCollisionContext,
    ) -> Vec<Aabb> {
        let offset = Vec3::new(
            f64::from(position.0),
            f64::from(position.1),
            f64::from(position.2),
        );
        match self.entity_collision_shape(position, context) {
            EntityCollisionShape::Stored => self
                .block_collision_shapes(position)
                .filter(|shape| is_non_empty_shape(shape.bounds))
                .map(|shape| shape.bounds)
                .collect(),
            EntityCollisionShape::Box(bounds) => vec![bounds.translated(offset.reverse())],
            EntityCollisionShape::Empty => Vec::new(),
        }
    }

    /// CollisionGetter.getBlockCollisions flattened to boxes, in the order BlockCollisions walks
    /// the blocks (x fastest, then y, then z): every box of each block whose shape meets the query.
    #[must_use]
    pub fn block_collision_boxes_in_order(
        &self,
        query: Aabb,
        context: &EntityCollisionContext,
    ) -> Vec<Aabb> {
        let cells = |min: f64, max: f64| {
            ((min - 1.0e-7).floor() as i32 - 1)..=((max + 1.0e-7).floor() as i32 + 1)
        };
        let (cells_x, cells_y, cells_z) = (
            cells(query.min.x, query.max.x),
            cells(query.min.y, query.max.y),
            cells(query.min.z, query.max.z),
        );
        let on_edge = |cell: i32, cells: &std::ops::RangeInclusive<i32>| {
            i32::from(cell == *cells.start() || cell == *cells.end())
        };
        let mut boxes = Vec::new();
        for z in cells_z.clone() {
            for y in cells_y.clone() {
                for x in cells_x.clone() {
                    let edges = on_edge(x, &cells_x) + on_edge(y, &cells_y) + on_edge(z, &cells_z);
                    let position = (x, y, z);
                    let reachable =
                        edges == 0 || (edges == 1 && self.has_large_collision_shape(position));
                    if !reachable || !self.is_chunk_loaded(x.div_euclid(16), z.div_euclid(16)) {
                        continue;
                    }
                    let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    let shape: Vec<Aabb> = self
                        .block_collision_boxes_for_entity(position, context)
                        .into_iter()
                        .map(|bounds| bounds.translated(offset))
                        .collect();
                    if shape.iter().any(|bounds| bounds.intersects(query)) {
                        boxes.extend(shape);
                    }
                }
            }
        }
        boxes
    }

    fn entity_collision_shape(
        &self,
        position: BlockPosition,
        context: &EntityCollisionContext,
    ) -> EntityCollisionShape {
        let (x, y, z) = position;
        let cell = |top: f64| {
            let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
            Aabb::new(min, min + Vec3::new(1.0, top, 1.0))
        };
        match self.effect_at(position) {
            BlockEffect::PowderSnow if context.fall_distance > 2.5 => {
                EntityCollisionShape::Box(cell(f64::from(0.9_f32)))
            }
            BlockEffect::PowderSnow
                if context.walks_on_powder_snow
                    && context.is_above(1.0, y)
                    && !context.descending =>
            {
                EntityCollisionShape::Box(cell(1.0))
            }
            BlockEffect::PowderSnow => EntityCollisionShape::Empty,
            BlockEffect::Scaffolding if context.is_above(1.0, y) && !context.descending => {
                EntityCollisionShape::Stored
            }
            BlockEffect::Scaffolding
                if self.is_unstable_scaffolding_bottom(position) && context.is_above(0.0, y) =>
            {
                EntityCollisionShape::Box(cell(0.125))
            }
            BlockEffect::Scaffolding => EntityCollisionShape::Empty,
            _ => EntityCollisionShape::Stored,
        }
    }

    fn is_unstable_scaffolding_bottom(&self, position: BlockPosition) -> bool {
        let properties = self
            .properties
            .get(&position)
            .map_or(&[][..], Vec::as_slice);
        let value = |name: &str| {
            properties
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        value("bottom") == Some("true") && value("distance").is_some_and(|distance| distance != "0")
    }

    /// BlockState.isSuffocating: as the block says, else a full collision cube.
    #[must_use]
    pub fn is_suffocating(&self, position: BlockPosition) -> bool {
        let block = self.block(position);
        block.suffocating.unwrap_or_else(|| {
            !self.extra_shapes.contains_key(&position)
                && block.shape.is_some_and(|shape| {
                    shape.bounds == Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))
                })
        })
    }

    /// CollisionGetter.collidesWithSuffocatingBlock: a suffocating block's collision shape meets
    /// the area.
    #[must_use]
    pub fn collides_with_suffocating_block(&self, area: Aabb) -> bool {
        let cells = |min: f64, max: f64| min.floor() as i32..=max.floor() as i32;
        cells(area.min.x, area.max.x).any(|x| {
            cells(area.min.y, area.max.y).any(|y| {
                cells(area.min.z, area.max.z).any(|z| {
                    let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    self.is_suffocating((x, y, z))
                        && self.block_collision_shapes((x, y, z)).any(|shape| {
                            is_non_empty_shape(shape.bounds)
                                && shape.bounds.translated(offset).intersects(area)
                        })
                })
            })
        })
    }

    #[must_use]
    pub fn is_free(&self, bounds: Aabb) -> bool {
        self.collision_shapes(bounds).is_empty() && !self.contains_any_liquid(bounds)
    }

    #[must_use]
    pub fn contains_any_liquid(&self, bounds: Aabb) -> bool {
        (bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32).any(|x| {
            (bounds.min.y.floor() as i32..bounds.max.y.ceil() as i32).any(|y| {
                (bounds.min.z.floor() as i32..bounds.max.z.ceil() as i32)
                    .any(|z| self.block((x, y, z)).fluid != FluidState::Empty)
            })
        })
    }

    #[must_use]
    pub fn no_collision(&self, bounds: Aabb) -> bool {
        self.collision_shapes(bounds).is_empty()
    }

    #[must_use]
    pub fn intersects_fall_damage_resetting_block(&self, start: Vec3, delta: Vec3) -> bool {
        let sweep = Aabb::new(
            Vec3::new(
                start.x.min(start.x + delta.x),
                start.y.min(start.y + delta.y),
                start.z.min(start.z + delta.z),
            ),
            Vec3::new(
                start.x.max(start.x + delta.x),
                start.y.max(start.y + delta.y),
                start.z.max(start.z + delta.z),
            ),
        );
        let min_x = sweep.min.x.floor() as i32;
        let max_x = sweep.max.x.ceil() as i32;
        let min_y = sweep.min.y.floor() as i32;
        let max_y = sweep.max.y.ceil() as i32;
        let min_z = sweep.min.z.floor() as i32;
        let max_z = sweep.max.z.ceil() as i32;
        for x in min_x..max_x {
            for y in min_y..max_y {
                for z in min_z..max_z {
                    let block = self.block((x, y, z));
                    if !block.fall_damage_resetting {
                        continue;
                    }
                    if let Some(shape) = block.shape {
                        if shape
                            .bounds
                            .translated(Vec3::new(x as f64, y as f64, z as f64))
                            .ray_intersects(start, delta)
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn collision_shapes_into(&self, query: Aabb, shapes: &mut Vec<Aabb>) {
        self.collect_collision_shapes(query, None, shapes, None);
    }

    fn collect_collision_shapes(
        &self,
        query: Aabb,
        context: Option<&EntityCollisionContext>,
        shapes: &mut Vec<Aabb>,
        mut entered: Option<(Aabb, &mut Vec<BlockVoxelShape>)>,
    ) {
        shapes.clear();
        let cells_x = query.min.x.floor() as i32..query.max.x.ceil() as i32;
        let cells_y = query.min.y.floor() as i32..query.max.y.ceil() as i32;
        let cells_z = query.min.z.floor() as i32..query.max.z.ceil() as i32;
        let min_chunk_x = (cells_x.start - 1).div_euclid(16);
        let max_chunk_x = cells_x.end.div_euclid(16);
        let min_chunk_z = (cells_z.start - 1).div_euclid(16);
        let max_chunk_z = cells_z.end.div_euclid(16);
        for chunk_x in min_chunk_x..=max_chunk_x {
            for chunk_z in min_chunk_z..=max_chunk_z {
                if !self.is_chunk_loaded(chunk_x, chunk_z) {
                    continue;
                }
                if let Some(positions) = self.chunks.get(&(chunk_x, chunk_z)) {
                    for &(x, y, z) in positions {
                        let distance = cells_outside(x, &cells_x)
                            + cells_outside(y, &cells_y)
                            + cells_outside(z, &cells_z);
                        let inside = distance == 0;
                        // As in vanilla's BlockCollisions, a block next to the queried cells only
                        // collides through a shape larger than a block (fences, walls).
                        let bordering = distance == 1 && self.has_large_collision_shape((x, y, z));
                        if !inside && !bordering {
                            continue;
                        }
                        let offset = Vec3::new(x as f64, y as f64, z as f64);
                        match context.map_or(EntityCollisionShape::Stored, |context| {
                            self.entity_collision_shape((x, y, z), context)
                        }) {
                            EntityCollisionShape::Stored => {
                                let boxes = || {
                                    self.block_collision_shapes((x, y, z))
                                        .filter(|shape| is_non_empty_shape(shape.bounds))
                                        .map(move |shape| shape.bounds.translated(offset))
                                };
                                shapes.extend(
                                    boxes().filter(|bounds| inside || bounds.intersects(query)),
                                );
                                if let Some((entity, entered)) = entered.as_mut() {
                                    if boxes().any(|bounds| reaches_into(*entity, bounds)) {
                                        entered.push(BlockVoxelShape {
                                            origin: offset,
                                            boxes: boxes().collect(),
                                        });
                                    }
                                }
                            }
                            EntityCollisionShape::Box(bounds) => {
                                if inside || bounds.intersects(query) {
                                    shapes.push(bounds);
                                }
                            }
                            EntityCollisionShape::Empty => {}
                        }
                    }
                }
            }
        }
        shapes.sort_by(|left, right| {
            left.min
                .x
                .total_cmp(&right.min.x)
                .then_with(|| left.min.y.total_cmp(&right.min.y))
                .then_with(|| left.min.z.total_cmp(&right.min.z))
                .then_with(|| left.max.x.total_cmp(&right.max.x))
                .then_with(|| left.max.y.total_cmp(&right.max.y))
                .then_with(|| left.max.z.total_cmp(&right.max.z))
        });
    }

    #[must_use]
    pub fn supporting_block_position(&self, area: Aabb, position: Vec3) -> Option<BlockPosition> {
        let min = (
            (area.min.x - 1.0e-7).floor() as i32 - 1,
            (area.min.y - 1.0e-7).floor() as i32 - 1,
            (area.min.z - 1.0e-7).floor() as i32 - 1,
        );
        let max = (
            (area.max.x + 1.0e-7).floor() as i32 + 1,
            (area.max.y + 1.0e-7).floor() as i32 + 1,
            (area.max.z + 1.0e-7).floor() as i32 + 1,
        );
        let mut closest: Option<(f64, BlockPosition)> = None;
        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    let edges = [
                        x == min.0 || x == max.0,
                        y == min.1 || y == max.1,
                        z == min.2 || z == max.2,
                    ]
                    .into_iter()
                    .filter(|edge| *edge)
                    .count();
                    if edges > 1 || (edges == 1 && !self.has_large_collision_shape((x, y, z))) {
                        continue;
                    }
                    let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    let supports = self.block_collision_shapes((x, y, z)).any(|shape| {
                        is_non_empty_shape(shape.bounds)
                            && shape.bounds.translated(offset).intersects(area)
                    });
                    if !supports {
                        continue;
                    }
                    let dx = f64::from(x) + 0.5 - position.x;
                    let dy = f64::from(y) + 0.5 - position.y;
                    let dz = f64::from(z) + 0.5 - position.z;
                    let distance = dx * dx + dy * dy + dz * dz;
                    let closer = closest.map_or(true, |(best, (bx, by, bz))| {
                        distance < best || (distance == best && (by, bz, bx) < (y, z, x))
                    });
                    if closer {
                        closest = Some((distance, (x, y, z)));
                    }
                }
            }
        }
        closest.map(|(_, block)| block)
    }

    fn block_collision_shapes(
        &self,
        position: BlockPosition,
    ) -> impl Iterator<Item = BlockShape> + '_ {
        self.block(position).shape.into_iter().chain(
            self.extra_shapes
                .get(&position)
                .into_iter()
                .flatten()
                .copied(),
        )
    }

    fn has_large_collision_shape(&self, position: BlockPosition) -> bool {
        self.block_collision_shapes(position).any(|shape| {
            shape.bounds.min.x < 0.0
                || shape.bounds.min.y < 0.0
                || shape.bounds.min.z < 0.0
                || shape.bounds.max.x > 1.0
                || shape.bounds.max.y > 1.0
                || shape.bounds.max.z > 1.0
        })
    }
}

fn cells_outside(cell: i32, cells: &std::ops::Range<i32>) -> i32 {
    if cell < cells.start {
        cells.start - cell
    } else if cell >= cells.end {
        cell - cells.end + 1
    } else {
        0
    }
}

fn is_non_empty_shape(bounds: Aabb) -> bool {
    bounds.min.x < bounds.max.x && bounds.min.y < bounds.max.y && bounds.min.z < bounds.max.z
}

fn fluids_match(left: FluidState, right: FluidState) -> bool {
    left == right
        || (matches!(left, FluidState::Water | FluidState::Waterlogged)
            && matches!(right, FluidState::Water | FluidState::Waterlogged))
}

fn fluid_matches_query(query: FluidState, actual: FluidState) -> bool {
    query == actual || (query == FluidState::Water && actual == FluidState::Waterlogged)
}

#[cfg(test)]
mod tests {
    use super::{BlockShape, BlockState, WorldSnapshot};
    use crate::math::{Aabb, Vec3};
    use crate::voxel_shape::VoxelShape;
    use crate::world::FluidState;

    #[test]
    fn stores_and_queries_collision_shapes() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (2, 3, 4),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );

        let shapes = world.collision_shapes(Aabb::new(
            Vec3::new(2.0, 3.0, 4.0),
            Vec3::new(3.0, 4.0, 5.0),
        ));

        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].min, Vec3::new(2.0, 3.0, 4.0));
    }

    fn block_with_box(min: Vec3, max: Vec3) -> BlockState {
        BlockState {
            id: 1,
            shape: Some(BlockShape {
                bounds: Aabb::new(min, max),
                ..BlockShape::full_block()
            }),
            ..BlockState::default()
        }
    }

    #[test]
    fn collides_with_shapes_reaching_out_of_the_block_below() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            block_with_box(Vec3::new(0.375, 0.0, 0.375), Vec3::new(0.625, 1.5, 0.625)),
        );
        world.set_block(
            (2, 0, 0),
            block_with_box(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
        );

        let above_fence = Aabb::new(Vec3::new(0.2, 1.2, 0.2), Vec3::new(0.8, 3.0, 0.8));
        assert_eq!(
            world.collision_shapes(above_fence),
            vec![Aabb::new(
                Vec3::new(0.375, 0.0, 0.375),
                Vec3::new(0.625, 1.5, 0.625)
            )]
        );
        let above_full_block = Aabb::new(Vec3::new(2.2, 1.2, 0.2), Vec3::new(2.8, 3.0, 0.8));
        assert!(world.collision_shapes(above_full_block).is_empty());
        let beside_fence_top = Aabb::new(Vec3::new(0.7, 1.2, 0.2), Vec3::new(1.3, 3.0, 0.8));
        assert!(world.collision_shapes(beside_fence_top).is_empty());
    }

    #[test]
    fn keeps_outline_shape_separate_from_collision_shape() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                outline_shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 0.5)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let query = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(world.collision_shapes(query).len(), 1);
        assert_eq!(world.outline_shapes(query).len(), 1);
        assert_eq!(world.outline_shapes(query)[0].max, Vec3::new(0.5, 1.0, 0.5));
    }

    #[test]
    fn preserves_multiple_outline_shapes() {
        let mut world = WorldSnapshot::new();
        let left = BlockShape {
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(0.25, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        let right = BlockShape {
            bounds: Aabb::new(Vec3::new(0.75, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        world.set_block_shapes_with_outline(
            (0, 0, 0),
            BlockState::default(),
            vec![BlockShape::full_block()],
            vec![left, right],
        );
        let shapes = world.outline_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].max.x, 0.25);
        assert_eq!(shapes[1].min.x, 0.75);
    }

    #[test]
    fn keeps_interaction_shape_separate_from_outline_shape() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                outline_shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0)),
                    ..BlockShape::full_block()
                }),
                interaction_shape: Some(BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0)),
                    ..BlockShape::full_block()
                }),
                ..BlockState::default()
            },
        );
        let query = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(world.outline_shapes(query)[0].max.x, 0.5);
        assert_eq!(world.interaction_shapes(query)[0].max.y, 0.5);
    }

    #[test]
    fn loaded_chunk_state_controls_collision_queries() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                outline_shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        assert!(world.is_chunk_loaded(0, 0));
        world.set_chunk_loaded(0, 0, false);
        assert!(world
            .collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
            .is_empty());
        world.set_chunk_loaded(0, 0, true);
        assert_eq!(
            world
                .collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
                .len(),
            1
        );
        assert!(world
            .outline_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
            .len()
            == 1);
        world.set_chunk_loaded(0, 0, false);
        assert!(world
            .outline_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
            .is_empty());
    }

    #[test]
    fn distinguishes_absent_blocks_from_explicit_air() {
        let mut world = WorldSnapshot::new();
        assert!(!world.contains_block((0, 0, 0)));
        world.set_block((0, 0, 0), BlockState::default());
        assert!(world.contains_block((0, 0, 0)));
        assert!(world.is_section_present(0, 0, 0));
        assert_eq!(world.block((0, 0, 0)), BlockState::default());
        world.remove_block((0, 0, 0));
        assert!(!world.contains_block((0, 0, 0)));
        assert!(!world.is_section_present(0, 0, 0));
    }

    #[test]
    fn preserves_typed_block_properties_separately_from_physics_state() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), BlockState::default());
        world.set_block_properties(
            (0, 0, 0),
            vec![
                ("facing".to_owned(), "north".to_owned()),
                ("powered".to_owned(), "false".to_owned()),
            ],
        );
        assert_eq!(
            world.block_properties((0, 0, 0)),
            vec![
                ("facing".to_owned(), "north".to_owned()),
                ("powered".to_owned(), "false".to_owned()),
            ]
        );
        world.remove_block((0, 0, 0));
        assert!(world.block_properties((0, 0, 0)).is_empty());
    }

    #[test]
    fn preserves_versioned_block_state_identity() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), BlockState::default());
        let version = crate::version::MinecraftVersion::new(1, 20, 6);
        world.set_block_state_id(
            (0, 0, 0),
            super::VersionedBlockStateId { version, id: 42 },
        );
        assert_eq!(
            world.block_state_id((0, 0, 0)),
            Some(super::VersionedBlockStateId { version, id: 42 })
        );
        world.remove_block((0, 0, 0));
        assert_eq!(world.block_state_id((0, 0, 0)), None);
    }

    #[test]
    fn exposes_section_block_counts_and_clears_empty_sections() {
        let mut world = WorldSnapshot::new();
        world.set_block((0, 0, 0), BlockState::default());
        world.set_block((1, 15, 1), BlockState::default());
        world.set_block((0, 16, 0), BlockState::default());
        assert_eq!(world.section_block_count(0, 0, 0), 2);
        assert_eq!(world.section_block_count(0, 1, 0), 1);
        world.set_section_present(0, 2, 0, true);
        assert!(world.is_section_present(0, 2, 0));
        assert_eq!(world.section_block_count(0, 2, 0), 0);
        world.set_section_present(0, 2, 0, false);
        assert!(!world.is_section_present(0, 2, 0));
        world.remove_block((0, 0, 0));
        world.remove_block((1, 15, 1));
        assert!(!world.is_section_present(0, 0, 0));
        assert_eq!(world.section_block_count(0, 1, 0), 1);
    }

    #[test]
    fn stores_block_entity_metadata_only_for_present_blocks() {
        let mut world = WorldSnapshot::new();
        assert_eq!(world.block_entity((0, 0, 0)), None);
        world.set_block((0, 0, 0), BlockState::default());
        world.set_block_entity((0, 0, 0), "minecraft:campfire".to_owned());
        assert_eq!(world.block_entity((0, 0, 0)), Some("minecraft:campfire"));
        world.remove_block_entity((0, 0, 0));
        assert_eq!(world.block_entity((0, 0, 0)), None);
        world.set_block_entity((0, 0, 0), "minecraft:chest".to_owned());
        world.remove_block((0, 0, 0));
        assert_eq!(world.block_entity((0, 0, 0)), None);
    }

    #[test]
    fn world_bounds_preserve_height_and_border_contract() {
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        assert!(world.is_inside_world_border(Vec3::new(0.0, 100.0, 0.0)));
        assert!(!world.is_inside_world_border(Vec3::new(6.0, 100.0, 0.0)));
        assert!(!world.is_inside_world_border(Vec3::new(5.0, 100.0, 0.0)));
        assert!(world.is_inside_world_border(Vec3::new(0.0, 320.0, 0.0)));
        assert!(!world.is_inside_build_height(320));
    }

    #[test]
    fn voxel_shape_boxes_are_wired_into_world_collision_queries() {
        let mut world = WorldSnapshot::new();
        let shape = VoxelShape::union(&[
            VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0))),
            VoxelShape::from_box(Aabb::new(
                Vec3::new(0.5, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 1.0),
            )),
        ]);
        world.set_block_voxel_shape(
            (2, 3, 4),
            BlockState {
                id: 1,
                ..Default::default()
            },
            &shape,
        );

        let shapes = world.collision_shapes(Aabb::new(
            Vec3::new(2.0, 3.0, 4.0),
            Vec3::new(3.0, 4.0, 5.0),
        ));
        assert_eq!(shapes.len(), 2);
    }

    #[test]
    fn is_free_rejects_liquid_without_collision_shape() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );

        let bounds = Aabb::new(Vec3::new(0.1, 0.1, 0.1), Vec3::new(0.9, 0.9, 0.9));
        assert!(!world.is_free(bounds));
    }

    #[test]
    fn empty_voxel_shape_removes_collision_geometry() {
        let mut world = WorldSnapshot::new();
        let position = (2, 3, 4);
        let full = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
        world.set_block_voxel_shape(position, BlockState::default(), &full);
        world.set_block_voxel_shape(position, BlockState::default(), &VoxelShape::empty());

        assert!(world
            .collision_shapes(Aabb::new(
                Vec3::new(2.0, 3.0, 4.0),
                Vec3::new(3.0, 4.0, 5.0)
            ))
            .is_empty());
    }

    #[test]
    fn degenerate_multi_shape_entries_do_not_collide() {
        let mut world = WorldSnapshot::new();
        world.set_block_shapes(
            (0, 0, 0),
            BlockState::default(),
            vec![
                BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
                    ..BlockShape::full_block()
                },
                BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)),
                    ..BlockShape::full_block()
                },
            ],
        );

        assert_eq!(
            world
                .collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
                .len(),
            1
        );
    }

    #[test]
    fn empty_shape_removes_block() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block((0, 0, 0), BlockState::default());

        assert!(world
            .collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)))
            .is_empty());
    }

    #[test]
    fn exposes_fluid_level_and_flow() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 4,
                fluid_flow: Vec3::new(1.0, 0.0, -0.5),
                fluid_falling: true,
                ..BlockState::default()
            },
        );

        assert_eq!(world.fluid_height_at((0, 0, 0)), f64::from(8.0_f32 / 9.0));
        assert_eq!(world.fluid_flow_at((0, 0, 0)), Vec3::new(1.0, 0.0, -0.5));
        assert!(world.block((0, 0, 0)).fluid_falling);
    }

    #[test]
    fn preserves_fluid_tags_with_fluid_state() {
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_tags: 0b101,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        assert_eq!(world.block((0, 0, 0)).fluid_tags, 0b101);
    }

    #[test]
    fn fluid_height_reaches_full_height_when_same_fluid_is_above() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 1,
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 1,
                ..BlockState::default()
            },
        );

        assert_eq!(world.fluid_height_at((0, 0, 0)), 1.0);
    }

    #[test]
    fn waterlogged_state_continues_water_height_above() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 4,
                ..BlockState::default()
            },
        );
        world.set_block(
            (0, 1, 0),
            BlockState {
                fluid: super::FluidState::Waterlogged,
                fluid_level: 4,
                ..BlockState::default()
            },
        );

        assert_eq!(world.fluid_height_at((0, 0, 0)), 1.0);
    }

    #[test]
    fn fluid_height_ignores_surfaces_below_query_bounds() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 8,
                ..BlockState::default()
            },
        );

        let bounds = Aabb::new(Vec3::new(-0.3, 1.0, -0.3), Vec3::new(0.3, 2.0, 0.3));
        assert_eq!(
            world.fluid_height_in_bounds(super::FluidState::Water, bounds),
            0.0
        );
    }

    #[test]
    fn shallow_flow_scales_each_block_by_the_fluid_height_reached_so_far() {
        let mut world = WorldSnapshot::default();
        for (x, level, flow) in [
            (0, 7, Vec3::new(1.0, 0.0, 0.0)),
            (1, 6, Vec3::new(3.0, 0.0, 0.0)),
        ] {
            world.set_block(
                (x, 0, 0),
                BlockState {
                    fluid: super::FluidState::Water,
                    fluid_level: level,
                    fluid_flow: flow,
                    ..BlockState::default()
                },
            );
        }

        let contact = world.fluid_contact(
            super::FluidState::Water,
            Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0)),
            crate::version::MinecraftVersion::new(1, 21, 1),
        );
        let first = f64::from(1.0_f32 / 9.0) - 0.001;
        let second = f64::from(2.0_f32 / 9.0) - 0.001;
        assert!(contact.touching);
        assert_eq!(contact.height, second);
        assert_eq!(
            contact.flow,
            Vec3::new(1.0 * first + 3.0 * second, 0.0, 0.0)
        );
        assert_eq!(contact.flowing_blocks, 2);
    }

    #[test]
    fn air_query_rejects_solid_blocks_without_special_effects() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                id: 1,
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );

        assert!(!world.is_air_at((0, 0, 0)));
        assert!(world.is_air_at((1, 0, 0)));
    }

    #[test]
    fn falling_level_eight_fluid_keeps_its_own_height() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 8,
                fluid_falling: true,
                ..BlockState::default()
            },
        );

        assert_eq!(world.fluid_height_at((0, 0, 0)), f64::from(8.0_f32 / 9.0));
    }

    #[test]
    fn falling_levels_decode_their_vanilla_amounts() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 12,
                fluid_falling: true,
                ..BlockState::default()
            },
        );

        assert_eq!(world.fluid_height_at((0, 0, 0)), f64::from(8.0_f32 / 9.0));
    }

    #[test]
    fn source_fluid_is_eight_ninths_tall_without_the_same_fluid_above() {
        let mut world = WorldSnapshot::default();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: super::FluidState::Water,
                fluid_level: 1,
                fluid_source: true,
                ..BlockState::default()
            },
        );

        assert_eq!(world.fluid_height_at((0, 0, 0)), f64::from(8.0_f32 / 9.0));
    }

    #[test]
    fn chunk_index_tracks_negative_and_cross_chunk_updates() {
        let mut world = WorldSnapshot::new();
        for position in [(-1, 2, -1), (16, 2, 16)] {
            world.set_block(
                position,
                BlockState {
                    id: 1,
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }

        let shapes = world.collision_shapes(Aabb::new(
            Vec3::new(-1.0, 2.0, -1.0),
            Vec3::new(17.0, 3.0, 17.0),
        ));
        assert_eq!(shapes.len(), 2);

        world.set_block((-1, 2, -1), BlockState::default());
        assert_eq!(
            world
                .collision_shapes(Aabb::new(
                    Vec3::new(-1.0, 2.0, -1.0),
                    Vec3::new(0.0, 3.0, 0.0),
                ))
                .len(),
            0
        );
    }

    #[test]
    fn preserves_multiple_collision_shapes_for_one_block() {
        let mut world = WorldSnapshot::new();
        world.set_block_shapes(
            (0, 0, 0),
            BlockState {
                id: 1,
                ..BlockState::default()
            },
            vec![
                BlockShape {
                    bounds: Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0)),
                    ..BlockShape::full_block()
                },
                BlockShape {
                    bounds: Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
                    ..BlockShape::full_block()
                },
            ],
        );

        let shapes = world.collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0)));
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].max.x, 0.5);
        assert_eq!(shapes[1].min.x, 0.5);
    }

    #[test]
    fn collision_shape_queries_are_deterministically_ordered() {
        let mut world = WorldSnapshot::new();
        for x in [2, 0, 1] {
            world.set_block(
                (x, 0, 0),
                BlockState {
                    shape: Some(BlockShape::full_block()),
                    ..BlockState::default()
                },
            );
        }

        let shapes = world.collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(3.0, 1.0, 1.0)));
        assert_eq!(shapes[0].min.x, 0.0);
        assert_eq!(shapes[1].min.x, 1.0);
        assert_eq!(shapes[2].min.x, 2.0);
    }
}
