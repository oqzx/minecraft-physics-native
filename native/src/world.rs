use std::collections::{HashMap, HashSet};

use crate::collision::{reaches_into, BlockVoxelShape};
use crate::math::{clamp_f64, Aabb, Vec3};
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
    /// The hay bale, whose fallOn deals a fifth of the fall damage (1.9+).
    Hay,
    /// A pointed dripstone tip pointing up, whose fallOn deals double damage for a longer fall.
    PointedDripstoneTip,
    /// A climbable block of #can_glide_through (the vines), which a fall-flying entity is not on
    /// from 1.21.11 (LivingEntity.onClimbable).
    GlideThroughClimbable,
    /// The lily pad, which a boat leaves out of its ground friction (1.13+) and which gives a boat
    /// no collision box before 1.13 (BlockLilyPad.addCollisionBoxesToList).
    LilyPad,
    /// A snow layer (1.8's Blocks.SNOW_LAYER), which a 1.8 boat clears from its corners.
    SnowLayer,
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

/// The entity a no-collision query is made for (CollisionGetter.noCollision(entity, box)): the
/// CollisionContext its blocks' shapes are taken in, and its box, position and
/// Entity.isOutsideBorder, which say whether the world border collides.
#[derive(Clone, Copy, Debug)]
pub struct CollisionQueryEntity<'a> {
    pub context: &'a EntityCollisionContext,
    pub bounds: Aabb,
    pub position: Vec3,
    pub outside_border: bool,
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
    /// Blocks.WATER itself (a source or flowing water, not a waterlogged block or a water plant):
    /// Entity.getBlockSpeedFactor keeps its speed factor without reading the block below (1.15+).
    pub water_block: bool,
    /// In #fall_damage_resetting: a full cube for Entity.move's FALLDAMAGE_RESETTING clip.
    pub fall_damage_resetting: bool,
    /// The end gateway and end portal, full cubes for a player's FALLDAMAGE_RESETTING clip from
    /// 1.21.9 (ClipContext.java:65-68).
    pub player_fall_damage_resetting: bool,
    pub suffocating: Option<bool>,
    /// Entity.isInsideWall's test of the block before 1.13 where it is not `suffocating`
    /// (pushOutOfBlocks' isNormalCube): Block.isLeafBlock in 1.8 - 1.10 and
    /// BlockState.causesSuffocation in 1.11 - 1.12.2; `suffocating` when not given.
    pub in_wall: Option<bool>,
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
    unloaded_chunks: HashSet<ChunkPosition>,
    min_build_height: i32,
    max_build_height: i32,
    border_center: (f64, f64),
    border_size: f64,
    border_absolute_max: f64,
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
            unloaded_chunks: HashSet::new(),
            min_build_height: 0,
            max_build_height: 0,
            border_center: (0.0, 0.0),
            border_size: WORLD_BORDER_DEFAULT_SIZE,
            border_absolute_max: WORLD_BORDER_ABSOLUTE_MAX,
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

    /// Stores the block, which loads a chunk never marked unloaded; in a chunk marked unloaded it
    /// is a no-op, as LevelChunk.setBlockState is on the client's EmptyLevelChunk
    /// (client1.18.1 EmptyLevelChunk.java, 1.14-1.17 EmptyChunk.setBlockState returns null), and
    /// so are the writes of its properties, state id and block entity. (1.8.x's EmptyChunk, which
    /// keeps what is set in it and gives it back for every unloaded chunk, and 1.13.x's, which
    /// gives back the fluid set in it, are not modelled.)
    pub fn set_block(&mut self, position: (i32, i32, i32), state: BlockState) {
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        if self.unloaded_chunks.contains(&chunk) {
            return;
        }
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
        if self.is_marked_unloaded(position) {
            return;
        }
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
        if self.is_marked_unloaded(position) {
            return;
        }
        self.state_ids.insert(position, state_id);
    }

    #[must_use]
    pub fn block_state_id(&self, position: (i32, i32, i32)) -> Option<VersionedBlockStateId> {
        self.state_ids.get(&position).copied()
    }

    pub fn set_block_entity(&mut self, position: (i32, i32, i32), kind: String) {
        if self.is_marked_unloaded(position) {
            return;
        }
        self.block_entities.insert(position, kind);
    }

    pub fn remove_block_entity(&mut self, position: (i32, i32, i32)) {
        if self.is_marked_unloaded(position) {
            return;
        }
        self.block_entities.remove(&position);
    }

    #[must_use]
    pub fn block_entity(&self, position: (i32, i32, i32)) -> Option<&str> {
        self.block_entities.get(&position).map(String::as_str)
    }

    #[must_use]
    pub fn block_properties(&self, position: (i32, i32, i32)) -> Vec<(String, String)> {
        self.loaded_cell(&self.properties, position)
            .cloned()
            .unwrap_or_default()
    }

    /// Removes the block and what it carries; a no-op in a chunk marked unloaded, as setting air
    /// there is (set_block).
    pub fn remove_block(&mut self, position: (i32, i32, i32)) {
        if self.is_marked_unloaded(position) {
            return;
        }
        self.extra_shapes.remove(&position);
        self.extra_outline_shapes.remove(&position);
        self.extra_interaction_shapes.remove(&position);
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

    /// The cell is in a chunk marked unloaded, which writes leave alone.
    fn is_marked_unloaded(&self, position: BlockPosition) -> bool {
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        self.unloaded_chunks.contains(&chunk)
    }

    pub fn set_chunk_loaded(&mut self, chunk_x: i32, chunk_z: i32, loaded: bool) {
        let chunk = (chunk_x, chunk_z);
        if loaded {
            self.loaded_chunks.insert(chunk);
            self.unloaded_chunks.remove(&chunk);
        } else {
            self.loaded_chunks.remove(&chunk);
            self.unloaded_chunks.insert(chunk);
        }
    }

    /// The value stored for the cell, if its chunk is loaded: ClientChunkCache.getChunk gives an
    /// unloaded chunk as the empty chunk, all air with no fluid (1.14+ EmptyChunk.getBlockState
    /// and getFluidState, 1.12.2 EmptyChunk.getBlockState).
    fn loaded_cell<'a, T>(
        &self,
        cells: &'a HashMap<BlockPosition, T>,
        position: BlockPosition,
    ) -> Option<&'a T> {
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        cells
            .get(&position)
            .filter(|_| self.loaded_chunks.contains(&chunk))
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

    /// WorldBorder.setAbsoluteMaxSize: the bound every edge is clamped to (the init border packet's,
    /// server.properties max-world-size; 29999984 until a server says another, 1.21.11
    /// WorldBorder.java:34, ClientPacketListener.java:1805).
    pub fn set_world_border_absolute_max_size(&mut self, absolute_max_size: i32) {
        self.border_absolute_max = f64::from(absolute_max_size);
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

    /// WorldBorder.isWithinBounds(x, z): inside the clamped edges, the west and north ones
    /// excluded before 1.21 (client1.20.6 WorldBorder.java:44-46; 1.17 contains(double, double),
    /// which 1.16 and earlier do not have) and included from 1.21 (client1.21 :57-63).
    #[must_use]
    pub fn is_inside_world_border(&self, position: Vec3, release: MinecraftVersion) -> bool {
        self.is_within_world_border(position, 0.0, release)
    }

    /// WorldBorder.isWithinBounds(x, z, margin).
    fn is_within_world_border(&self, position: Vec3, margin: f64, release: MinecraftVersion) -> bool {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        let after_min = |value: f64, min: f64| {
            if release.at_least(1, 21, 0) {
                value >= min - margin
            } else {
                value > min - margin
            }
        };
        after_min(position.x, min_x)
            && position.x < max_x + margin
            && after_min(position.z, min_z)
            && position.z < max_z + margin
    }

    /// WorldBorder.getMinX / getMaxX / getMinZ / getMaxZ.
    fn world_border_edges(&self) -> (f64, f64, f64, f64) {
        let half = self.border_size / 2.0;
        let max = self.border_absolute_max;
        let edge = |value: f64| value.max(-max).min(max);
        let (center_x, center_z) = self.border_center;
        (
            edge(center_x - half),
            edge(center_x + half),
            edge(center_z - half),
            edge(center_z + half),
        )
    }

    /// WorldBorder.isInsideCloseToBorder: the entity at `position` sweeping `swept` is inside the
    /// border and close to it (client1.18.1 WorldBorder.java:78-81, client1.21 :95-98).
    #[must_use]
    pub fn is_inside_close_to_world_border(
        &self,
        position: Vec3,
        swept: Aabb,
        release: MinecraftVersion,
    ) -> bool {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        let size = swept.max - swept.min;
        let margin = size.x.abs().max(size.z.abs()).max(1.0);
        let distance = (position.x - min_x)
            .min(max_x - position.x)
            .min(position.z - min_z)
            .min(max_z - position.z);
        distance < margin * 2.0 && self.is_within_world_border(position, margin, release)
    }

    /// World.isInsideBorder before 1.14: whether the entity at `position` is inside the border,
    /// which an entity flagged outside it (Entity.isOutsideBorder) has to be a block further in
    /// for and any other a block less.
    #[must_use]
    pub fn is_inside_legacy_world_border(&self, position: Vec3, outside: bool) -> bool {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        let margin = if outside { 1.0 } else { -1.0 };
        position.x > min_x + margin
            && position.x < max_x - margin
            && position.z > min_z + margin
            && position.z < max_z - margin
    }

    /// !WorldBorder.contains(BlockPos) for the column of cells at `x`, `z`.
    #[must_use]
    pub fn is_outside_world_border_cell(&self, x: f64, z: f64) -> bool {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        !(x + 1.0 > min_x && x < max_x && z + 1.0 > min_z && z < max_z)
    }

    /// World.getCollisionBoxes before 1.14 for an entity inside the border: each cell meeting
    /// `query` outside the border, in a loaded chunk, collides as stone (a full cube) in place of
    /// its block.
    #[must_use]
    pub fn legacy_world_border_cells(&self, query: Aabb) -> Vec<Aabb> {
        let cells = |min: f64, max: f64| min.floor() as i32..max.ceil() as i32;
        let mut boxes = Vec::new();
        for x in cells(query.min.x, query.max.x) {
            for z in cells(query.min.z, query.max.z) {
                let outside = self.is_outside_world_border_cell(f64::from(x), f64::from(z));
                if !outside || !self.is_chunk_loaded(x >> 4, z >> 4) {
                    continue;
                }
                for y in cells(query.min.y, query.max.y) {
                    let min = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    boxes.push(Aabb::new(min, min + Vec3::new(1.0, 1.0, 1.0)));
                }
            }
        }
        boxes
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

    /// BlockCollisionSpliterator's border shape for an entity (1.16 - 1.17.1 getBlockCollisions
    /// with an entity: 1.16 BlockCollisionSpliterator.java:56-58, 101-129; 1.17 :58-59, 102-125):
    /// offered, whatever the query, when the entity's box is not strictly inside the border's
    /// block-aligned edges (isInWorldBorder) and touches the outside from within it: the box shrunk
    /// by 1e-7 meets none of it, the box grown by 1e-7 does.
    #[must_use]
    pub fn entity_touches_world_border(&self, entity: Aabb) -> bool {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        let (min_x, max_x) = (min_x.floor(), max_x.ceil());
        let (min_z, max_z) = (min_z.floor(), max_z.ceil());
        let within = |x: f64, z: f64| x > min_x && x < max_x && z > min_z && z < max_z;
        if within(entity.min.x, entity.min.z) && within(entity.max.x, entity.max.z) {
            return false;
        }
        self.entity_meets_world_border_from_inside(entity)
    }

    /// entity_touches_world_border without its isInWorldBorder shortcut, as getBlockCollisions
    /// makes the test in 1.14.4 - 1.15.2 (client1.14.4 LevelReader.java:146-155, 1.15
    /// CollisionView.java:80-88): the entity's box shrunk by 1e-7 meets none of the outside, the
    /// box grown by 1e-7 does.
    fn entity_meets_world_border_from_inside(&self, entity: Aabb) -> bool {
        let border = self.world_border_collision_boxes();
        let margin = Vec3::new(1.0e-7, 1.0e-7, 1.0e-7);
        let (smaller, larger) = (entity.deflate(margin), entity.inflate(margin));
        !border.iter().any(|outside| outside.intersects(smaller))
            && border.iter().any(|outside| outside.intersects(larger))
    }

    #[must_use]
    pub fn is_inside_build_height(&self, y: i32) -> bool {
        y >= self.min_build_height && y < self.max_build_height
    }

    /// WorldBorder.clampVec3ToBound from 1.21.2 (client1.21.2 WorldBorder.java:81-83): the
    /// position held inside the clamped edges, short of the east and south ones by the float
    /// 1.0E-5. Before, only clampToBounds, the block position (its corner here) of the position
    /// held to the edges (client1.20.6 :56-58), from 1.21 a block short of the east and south
    /// ones (client1.21 :73-75).
    #[must_use]
    pub fn clamp_to_world_border(&self, position: Vec3, release: MinecraftVersion) -> Vec3 {
        let (min_x, max_x, min_z, max_z) = self.world_border_edges();
        if release.at_least(1, 21, 2) {
            let inset = f64::from(1.0e-5_f32);
            return Vec3::new(
                clamp_f64(position.x, min_x, max_x - inset),
                position.y,
                clamp_f64(position.z, min_z, max_z - inset),
            );
        }
        let inset = if release.at_least(1, 21, 0) { 1.0 } else { 0.0 };
        Vec3::new(
            clamp_f64(position.x, min_x, max_x - inset).floor(),
            position.y.floor(),
            clamp_f64(position.z, min_z, max_z - inset).floor(),
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
        if self
            .unloaded_chunks
            .contains(&(position.0.div_euclid(16), position.2.div_euclid(16)))
        {
            return;
        }
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
        self.loaded_cell(&self.blocks, position)
            .copied()
            .unwrap_or_default()
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
        if interaction && !self.fluid_interaction_loaded(area) {
            return contact;
        }
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

    /// EntityFluidInteraction.hasFluidAndLoaded: from 26.1 nothing is touched while a chunk under
    /// the cells of the box (deflated by 0.001) widened by one is not loaded (ClientChunkCache.getChunk
    /// gives null, 26.1 EntityFluidInteraction.java:44, 94-110, ClientChunkCache.java:71-80).
    fn fluid_interaction_loaded(&self, area: Aabb) -> bool {
        let chunks_x = (area.min.x.floor() as i32 - 1).div_euclid(16)
            ..=(area.max.x.ceil() as i32).div_euclid(16);
        let chunks_z = (area.min.z.floor() as i32 - 1).div_euclid(16)
            ..=(area.max.z.ceil() as i32).div_euclid(16);
        chunks_x.clone().all(|chunk_x| {
            chunks_z
                .clone()
                .all(|chunk_z| self.is_chunk_loaded(chunk_x, chunk_z))
        })
    }

    /// Whether the eyes at (x, eye_y, z) of an entity with the box are in the fluid:
    /// isInsideOfMaterial before 1.13, the eye block's top plus 0.11111111F in float 1.13 - 1.15
    /// (the own height in 1.13), updateSubmergedInWaterState 0.11111111F below the eyes 1.16 -
    /// 1.20.4 and updateFluidOnEyes at them 1.20.5 - 1.21.11, and from 26.1
    /// EntityFluidInteraction's eyesInside: a fluid of the scan in the eyes' column spanning them,
    /// both ends included, in double (1.8.9 Entity.java:816-828, 1.12.2 :1120-1136, 1.13.2 and
    /// 1.15.2 Entity.isUnderLiquid bytecode `y + (height + 0.11111111F)`, 1.20.4 :1219-1236,
    /// 1.20.5 :1234-1245, 1.21.11 :1539-1550, 26.1 EntityFluidInteraction.java:44-70). (A boat's
    /// passenger and 26.3's getHeightForCamera are not modelled.)
    #[must_use]
    pub fn eyes_in_fluid(
        &self,
        fluid: FluidState,
        bounds: Aabb,
        (x, eye_y, z): (f64, f64, f64),
        release: MinecraftVersion,
    ) -> bool {
        let (cell_x, cell_z) = (x.floor() as i32, z.floor() as i32);
        if release.at_least(26, 1, 0) {
            let area = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
            if !self.fluid_interaction_loaded(area) {
                return false;
            }
            return (area.min.y.floor() as i32..area.max.y.ceil() as i32).any(|y| {
                if !fluid_matches_query(fluid, self.block((cell_x, y, cell_z)).fluid) {
                    return false;
                }
                let bottom = f64::from(y);
                let top = bottom + self.fluid_height_at((cell_x, y, cell_z));
                top >= area.min.y && eye_y >= bottom && eye_y <= top
            });
        }
        let eyes = if release.at_least(1, 16, 0) && !release.at_least(1, 20, 5) {
            eye_y - f64::from(0.11111111_f32)
        } else {
            eye_y
        };
        let cell = (cell_x, eyes.floor() as i32, cell_z);
        let block = self.block(cell);
        if !fluid_matches_query(fluid, block.fluid) {
            return false;
        }
        if !release.at_least(1, 13, 0) {
            let level = if block.fluid_source || block.fluid_falling || block.fluid_level >= 8 {
                0
            } else {
                block.fluid_level
            };
            let percent = f32::from(level + 1) / 9.0 - 0.11111111_f32;
            return eyes < f64::from((cell.1 + 1) as f32 - percent);
        }
        if !release.at_least(1, 16, 0) {
            let height = if release.at_least(1, 14, 0) {
                self.fluid_height_at(cell) as f32
            } else {
                let amount = if block.fluid_source
                    || block.fluid_falling
                    || !(1..8).contains(&block.fluid_level)
                {
                    8
                } else {
                    8 - block.fluid_level
                };
                f32::from(amount) / 9.0
            };
            return eyes < f64::from(cell.1 as f32 + (height + 0.11111111_f32));
        }
        f64::from(cell.1 as f32 + self.fluid_height_at(cell) as f32) > eyes
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

    /// FluidBlock.onEntityCollision's setInLava in 1.16 - 1.16.1: a lava block of the box whose
    /// float top `y + height` is above the box's foot (1.16 FluidBlock.java:170-177; the blocks
    /// of Entity.checkBlockCollision, 1.16 Entity.java:898-908).
    #[must_use]
    pub fn has_lava_above_feet_inside(&self, bounds: Aabb) -> bool {
        let area = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
        (area.min.x.floor() as i32..=area.max.x.floor() as i32).any(|x| {
            (area.min.y.floor() as i32..=area.max.y.floor() as i32).any(|y| {
                (area.min.z.floor() as i32..=area.max.z.floor() as i32).any(|z| {
                    self.block((x, y, z)).fluid == FluidState::Lava && {
                        let top = y as f32 + self.fluid_height_at((x, y, z)) as f32;
                        bounds.min.y < f64::from(top) || f64::from(top) > bounds.max.y
                    }
                })
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
                if let Some(shapes) = self.loaded_cell(&self.extra_shapes, position) {
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

    /// The collision, outline and interaction shapes stored for the block at a position, each
    /// with its first box first, for rewriting the block with its geometry unchanged.
    #[must_use]
    pub fn block_geometry(
        &self,
        position: (i32, i32, i32),
    ) -> (Vec<BlockShape>, Vec<BlockShape>, Vec<BlockShape>) {
        let block = self.block(position);
        let stored = |first: Option<BlockShape>, extra: &HashMap<BlockPosition, Vec<BlockShape>>| {
            first
                .into_iter()
                .chain(self.loaded_cell(extra, position).into_iter().flatten().copied())
                .collect::<Vec<_>>()
        };
        (
            stored(block.shape, &self.extra_shapes),
            stored(block.outline_shape, &self.extra_outline_shapes),
            stored(block.interaction_shape, &self.extra_interaction_shapes),
        )
    }

    /// The outline boxes of the block at a position, moved there.
    #[must_use]
    pub fn block_outline_boxes(&self, position: (i32, i32, i32)) -> Vec<Aabb> {
        let offset = Vec3::new(
            f64::from(position.0),
            f64::from(position.1),
            f64::from(position.2),
        );
        self.block(position)
            .outline_shape
            .into_iter()
            .chain(
                self.loaded_cell(&self.extra_outline_shapes, position)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
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
        self.collect_collision_shapes(query, Some(context), &mut shapes, None, false);
        shapes
    }

    /// collision_shapes_for_entity before 1.13, whose blocks give AxisAlignedBB lists: a box of no
    /// height (one layer of snow, BlockSnow.getCollisionBoundingBox) collides too.
    #[must_use]
    pub fn legacy_collision_shapes_for_entity(
        &self,
        query: Aabb,
        context: &EntityCollisionContext,
    ) -> Vec<Aabb> {
        let mut shapes = Vec::new();
        self.collect_collision_shapes(query, Some(context), &mut shapes, None, true);
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
        self.collect_collision_shapes(query, Some(context), &mut shapes, inside, false);
        (shapes, entered)
    }

    /// CollisionGetter.noCollision(entity, box) (World.isSpaceEmpty before 1.21.5, doesNotCollide
    /// before 1.17, World.getCollisionBoxes(entity, box).isEmpty() before 1.13): no block's
    /// collision shape for the entity, no entity collider and no world border meets the query.
    ///
    /// - Blocks: only the shapes that overlap the query count (BlockCollisions.computeNext's
    ///   `box.intersects` / `joinIsNotEmpty(..., AND)`, client1.21.11 BlockCollisions.java:86-104,
    ///   client1.14.4 LevelReader.java:186; before 1.13 Block.addCollisionBoxToList's strict
    ///   `intersects`, 1.8.9 Block.java:306-310), a box of no height among them before 1.13.
    ///   Before 1.14 a cell outside the border collides as stone for an entity inside it
    ///   (1.8.9 World.java:1071-1087, 1.13.2 RenderBlockView.java:113-139).
    /// - Entity colliders: from 1.14 those meeting the query grown by 1e-7, none for a query whose
    ///   average side is under 1e-7 (client1.14.4 EntityGetter.java:54-68, 1.16.5
    ///   EntityView.java:59-71, client1.21.11 EntityGetter.java:54-72); before, those meeting the
    ///   query (1.8.9 World.java:1093-1101, 1.12.2 World.java:1180-1203, 1.13.2
    ///   class_3593.java:22-37).
    /// - World border: from 1.18 its shape where it meets the query, for an entity inside and
    ///   close to it (1.18 CollisionView.java:57-62, client1.21.11 CollisionGetter.java:73-80); in
    ///   1.14 - 1.17 its shape whatever the query, for an entity whose box touches it from inside
    ///   (getBlockCollisions offers it first: client1.14.4 LevelReader.java:146-155, 1.16
    ///   BlockCollisionSpliterator.java:101-131).
    #[must_use]
    pub fn no_collision_for_entity(
        &self,
        query: Aabb,
        entity: CollisionQueryEntity<'_>,
        release: MinecraftVersion,
    ) -> bool {
        let flat = !release.at_least(1, 13, 0);
        let mut shapes = Vec::new();
        self.collect_collision_shapes(query, Some(entity.context), &mut shapes, None, flat);
        if !release.at_least(1, 14, 0)
            && self.is_inside_legacy_world_border(entity.position, entity.outside_border)
        {
            shapes.retain(|shape| {
                !self.is_outside_world_border_cell(shape.min.x.floor(), shape.min.z.floor())
            });
            shapes.extend(self.legacy_world_border_cells(query));
        }
        if shapes
            .iter()
            .any(|&shape| shape_meets_query(shape, query, release))
        {
            return false;
        }
        let entities = if release.at_least(1, 14, 0) {
            let size = query.max - query.min;
            if (size.x + size.y + size.z) / 3.0 < 1.0e-7 {
                Vec::new()
            } else {
                let grown = query.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
                self.entity_collisions(grown, entity.context)
            }
        } else {
            self.entity_collisions(query, entity.context)
        };
        if !entities.is_empty() {
            return false;
        }
        if release.at_least(1, 18, 0) {
            !(self.is_inside_close_to_world_border(entity.position, query, release)
                && self
                    .world_border_collision_boxes()
                    .iter()
                    .any(|outside| outside.intersects(query)))
        } else if release.at_least(1, 16, 0) {
            !self.entity_touches_world_border(entity.bounds)
        } else if release.at_least(1, 14, 0) {
            !self.entity_meets_world_border_from_inside(entity.bounds)
        } else {
            true
        }
    }

    /// Entity.isFree (doesNotCollide before 1.17, isOffsetPositionInLiquid before 1.13): no
    /// collision for the entity (no_collision_for_entity) and no liquid in the box
    /// (client1.21.11 Entity.java:642-644, 1.14 Entity.java:536-538).
    #[must_use]
    pub fn is_free_for_entity(
        &self,
        bounds: Aabb,
        entity: CollisionQueryEntity<'_>,
        release: MinecraftVersion,
    ) -> bool {
        self.no_collision_for_entity(bounds, entity, release) && !self.contains_any_liquid(bounds)
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

    /// The 1.13 - 1.14.0 getCollisionShapes flattened to boxes (1.13.2
    /// RenderBlockView.java:113-153, 1.14 ViewableWorld.java:145-195): the cells from floor(min) - 1
    /// to ceil(max), x fastest, without those on two edges; each partial shape meeting the query
    /// in that order, then every full cube of the walk set in one bitset shape, which is appended
    /// once when it meets the query and splits into the merged boxes VoxelSet.forEachBox finds
    /// (1.13.2 VoxelSet.java:203-256), its cubes in edge cells or outside the query included.
    #[must_use]
    pub fn block_collision_boxes_merging_full_cubes(
        &self,
        query: Aabb,
        context: &EntityCollisionContext,
    ) -> Vec<Aabb> {
        let cells = |min: f64, max: f64| (min.floor() as i32 - 1)..(max.ceil() as i32 + 1);
        let (cells_x, cells_y, cells_z) = (
            cells(query.min.x, query.max.x),
            cells(query.min.y, query.max.y),
            cells(query.min.z, query.max.z),
        );
        let on_edge = |cell: i32, cells: &std::ops::Range<i32>| {
            i32::from(cell == cells.start || cell == cells.end - 1)
        };
        let size = [cells_x.len(), cells_y.len(), cells_z.len()];
        let index = |x: usize, y: usize, z: usize| (x * size[1] + y) * size[2] + z;
        let mut full = vec![false; size[0] * size[1] * size[2]];
        let mut full_meets_query = false;
        let unit = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let mut boxes = Vec::new();
        for z in cells_z.clone() {
            for y in cells_y.clone() {
                for x in cells_x.clone() {
                    let edges = on_edge(x, &cells_x) + on_edge(y, &cells_y) + on_edge(z, &cells_z);
                    if edges > 1 || !self.is_chunk_loaded(x.div_euclid(16), z.div_euclid(16)) {
                        continue;
                    }
                    let position = (x, y, z);
                    let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    let shape = self.block_collision_boxes_for_entity(position, context);
                    if shape == [unit] {
                        let cell = (
                            (x - cells_x.start) as usize,
                            (y - cells_y.start) as usize,
                            (z - cells_z.start) as usize,
                        );
                        full[index(cell.0, cell.1, cell.2)] = true;
                        full_meets_query |= unit.translated(offset).intersects(query);
                        continue;
                    }
                    let shape: Vec<Aabb> = shape
                        .into_iter()
                        .map(|bounds| bounds.translated(offset))
                        .collect();
                    if shape.iter().any(|bounds| bounds.intersects(query)) {
                        boxes.extend(shape);
                    }
                }
            }
        }
        if !full_meets_query {
            return boxes;
        }
        // VoxelSet.forEachBox with merging: a run along z, grown along x, then down and up in y
        let origin = Vec3::new(
            f64::from(cells_x.start),
            f64::from(cells_y.start),
            f64::from(cells_z.start),
        );
        let run_set = |full: &[bool], from: usize, to: usize, x: isize, y: isize| {
            x >= 0
                && y >= 0
                && (x as usize) < size[0]
                && (y as usize) < size[1]
                && (from..to).all(|z| full[index(x as usize, y as usize, z)])
        };
        let clear_run = |full: &mut [bool], from: usize, to: usize, x: usize, y: usize| {
            for z in from..to {
                full[index(x, y, z)] = false;
            }
        };
        for x in 0..=size[0] {
            for y in 0..=size[1] {
                let mut start = None;
                for z in 0..=size[2] {
                    let set = x < size[0] && y < size[1] && z < size[2] && full[index(x, y, z)];
                    if set {
                        start.get_or_insert(z);
                        continue;
                    }
                    let Some(from) = start.take() else {
                        continue;
                    };
                    let (mut min_x, mut max_x, mut min_y, mut max_y) = (x, x, y, y);
                    clear_run(&mut full, from, z, x, y);
                    while run_set(&full, from, z, min_x as isize - 1, y as isize) {
                        clear_run(&mut full, from, z, min_x - 1, y);
                        min_x -= 1;
                    }
                    while run_set(&full, from, z, max_x as isize + 1, y as isize) {
                        clear_run(&mut full, from, z, max_x + 1, y);
                        max_x += 1;
                    }
                    while (min_x..=max_x)
                        .all(|cx| run_set(&full, from, z, cx as isize, min_y as isize - 1))
                    {
                        for cx in min_x..=max_x {
                            clear_run(&mut full, from, z, cx, min_y - 1);
                        }
                        min_y -= 1;
                    }
                    while (min_x..=max_x)
                        .all(|cx| run_set(&full, from, z, cx as isize, max_y as isize + 1))
                    {
                        for cx in min_x..=max_x {
                            clear_run(&mut full, from, z, cx, max_y + 1);
                        }
                        max_y += 1;
                    }
                    boxes.push(Aabb::new(
                        origin + Vec3::new(min_x as f64, min_y as f64, from as f64),
                        origin + Vec3::new(max_x as f64 + 1.0, max_y as f64 + 1.0, z as f64),
                    ));
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
            .loaded_cell(&self.properties, position)
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
            self.loaded_cell(&self.extra_shapes, position).is_none()
                && block.shape.is_some_and(|shape| {
                    shape.bounds == Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))
                })
        })
    }

    /// Entity.isInsideWall's test of one block (`in_wall`, else `is_suffocating`).
    #[must_use]
    pub fn is_in_wall_block(&self, position: BlockPosition) -> bool {
        self.block(position)
            .in_wall
            .unwrap_or_else(|| self.is_suffocating(position))
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

    /// Entity.isInWall in 1.18: a non-air suffocating block in a cell the area meets whose
    /// collision shape, moved to `origin` in place of its cell, meets the area
    /// (client1.18.1 Entity.java:1705-1708).
    #[must_use]
    pub fn collides_with_suffocating_block_moved_to(&self, area: Aabb, origin: Vec3) -> bool {
        let cells = |min: f64, max: f64| min.floor() as i32..=max.floor() as i32;
        cells(area.min.x, area.max.x).any(|x| {
            cells(area.min.y, area.max.y).any(|y| {
                cells(area.min.z, area.max.z).any(|z| {
                    self.is_suffocating((x, y, z))
                        && self.block_collision_shapes((x, y, z)).any(|shape| {
                            is_non_empty_shape(shape.bounds)
                                && shape.bounds.translated(origin).intersects(area)
                        })
                })
            })
        })
    }

    /// is_free_for_entity without an entity: no block's shape meets the box, no liquid in it.
    #[must_use]
    pub fn is_free(&self, bounds: Aabb) -> bool {
        self.no_collision(bounds) && !self.contains_any_liquid(bounds)
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

    /// CollisionGetter.noCollision(box) without an entity: no block's collision shape overlaps the
    /// box (the shapes in the cells it reaches that do not meet it do not count).
    #[must_use]
    pub fn no_collision(&self, bounds: Aabb) -> bool {
        !self
            .collision_shapes(bounds)
            .iter()
            .any(|shape| shape.intersects(bounds))
    }

    pub fn collision_shapes_into(&self, query: Aabb, shapes: &mut Vec<Aabb>) {
        self.collect_collision_shapes(query, None, shapes, None, false);
    }

    fn collect_collision_shapes(
        &self,
        query: Aabb,
        context: Option<&EntityCollisionContext>,
        shapes: &mut Vec<Aabb>,
        mut entered: Option<(Aabb, &mut Vec<BlockVoxelShape>)>,
        flat: bool,
    ) {
        let kept = |bounds: Aabb| {
            is_non_empty_shape(bounds)
                || (flat
                    && bounds.min.x < bounds.max.x
                    && bounds.min.y <= bounds.max.y
                    && bounds.min.z < bounds.max.z)
        };
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
                                        .filter(|shape| kept(shape.bounds))
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

    /// CollisionGetter.findSupportingBlock: of the blocks whose collision shape for the entity
    /// (BlockCollisions with its CollisionContext) meets `area`, the one closest to `position`.
    #[must_use]
    pub fn supporting_block_position(
        &self,
        area: Aabb,
        position: Vec3,
        context: &EntityCollisionContext,
    ) -> Option<BlockPosition> {
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
                    let supports = self
                        .block_collision_boxes_for_entity((x, y, z), context)
                        .into_iter()
                        .any(|bounds| bounds.translated(offset).intersects(area));
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
            self.loaded_cell(&self.extra_shapes, position)
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

/// Whether a box of a block's collision shape counts as meeting a no-collision query.
///
/// - Before 1.13: the strict `intersects` (1.8.9 Block.java:306-310, 1.12.2 Block.java:395-399).
/// - 1.13 - 1.15: VoxelShapes.matchesAnywhere(query, shape, AND) for every shape, full cubes too
///   (1.13.2 RenderBlockView.java:123, 1.14 ViewableWorld.java:153, 1.15 CollisionView.java:109).
/// - From 1.16: the strict `intersects` for a full cube, else Shapes.joinIsNotEmpty(shape, query,
///   AND) (1.16.5 BlockCollisionSpliterator.java:75-82, client1.21.11 BlockCollisions.java:86-104).
///
/// joinIsNotEmpty gives up where the boxes lie more than 1e-7 apart on an axis
/// (client1.21.11 Shapes.java:179-185), and its index merger merges the coordinates within 1e-7 of
/// each other, so an overlap of up to about 1e-7 on any axis does not count.
fn shape_meets_query(shape: Aabb, query: Aabb, release: MinecraftVersion) -> bool {
    if !release.at_least(1, 13, 0) || (release.at_least(1, 16, 0) && is_full_cube_box(shape)) {
        return shape.intersects(query);
    }
    let axes = [
        ([shape.min.x, shape.max.x], [query.min.x, query.max.x]),
        ([shape.min.y, shape.max.y], [query.min.y, query.max.y]),
        ([shape.min.z, shape.max.z], [query.min.z, query.max.z]),
    ];
    if axes
        .iter()
        .any(|(a, b)| a[1] < b[0] - 1.0e-7 || b[1] < a[0] - 1.0e-7)
    {
        return false;
    }
    // the query is the first shape before 1.16, the block's the first after
    let query_first = !release.at_least(1, 16, 0);
    let merges_equal = release.at_least(1, 14, 0);
    axes.iter().all(|&(a, b)| {
        let (first, second) = if query_first { (b, a) } else { (a, b) };
        merged_sections_meet(first, second, merges_equal)
    })
}

/// A cell of a shape that is Shapes.block(): one unit cube on whole coordinates.
fn is_full_cube_box(bounds: Aabb) -> bool {
    bounds.min.x.fract() == 0.0
        && bounds.min.y.fract() == 0.0
        && bounds.min.z.fract() == 0.0
        && bounds.max.x == bounds.min.x + 1.0
        && bounds.max.y == bounds.min.y + 1.0
        && bounds.max.z == bounds.min.z + 1.0
}

/// IndirectMerger with AND (it keeps only the coordinates where both lists have begun and not
/// ended) for one box's coordinates on an axis against another's: whether a merged section lies in
/// both boxes (client1.21.11 IndirectMerger.java ctor, 1.16.5 SimplePairList.java:23-47,
/// 1.14 SimpleDoubleListPair.java:23-47). A coordinate within 1e-7 of the last kept one replaces
/// its indices instead of opening a section; 1.13.2 (class_4092.java:34-44) merges only one closer
/// than 1e-7 (`d > e - 1.0E-7`), from 1.14 one at 1e-7 too (`d >= e - 1.0E-7`).
fn merged_sections_meet(first: [f64; 2], second: [f64; 2], merges_equal: bool) -> bool {
    if first == second {
        // IdenticalMerger: the one section of the two equal lists
        return true;
    }
    let mut sections: Vec<(usize, usize)> = Vec::with_capacity(4);
    let mut last = f64::NAN;
    let (mut i, mut j) = (0, 0);
    loop {
        let first_left = i < 2;
        let second_left = j < 2;
        if !first_left && !second_left {
            break;
        }
        let from_first = first_left && (!second_left || first[i] < second[j] + 1.0e-7);
        let e = if from_first {
            i += 1;
            first[i - 1]
        } else {
            j += 1;
            second[j - 1]
        };
        if ((i != 0 && first_left) || from_first) && ((j != 0 && second_left) || !from_first) {
            let merged = if merges_equal {
                last >= e - 1.0e-7
            } else {
                last > e - 1.0e-7
            };
            if !merged {
                sections.push((i - 1, j - 1));
                last = e;
            } else if let Some(section) = sections.last_mut() {
                *section = (i - 1, j - 1);
            }
        }
    }
    // the sections are the spans between consecutive kept coordinates
    sections
        .iter()
        .take(sections.len().saturating_sub(1))
        .any(|&section| section == (0, 0))
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
    use crate::version::MinecraftVersion;
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
        let release = MinecraftVersion::new(1, 21, 11);
        assert!(world.is_inside_world_border(Vec3::new(0.0, 100.0, 0.0), release));
        assert!(!world.is_inside_world_border(Vec3::new(6.0, 100.0, 0.0), release));
        assert!(!world.is_inside_world_border(Vec3::new(5.0, 100.0, 0.0), release));
        assert!(world.is_inside_world_border(Vec3::new(0.0, 320.0, 0.0), release));
        assert!(!world.is_inside_build_height(320));
    }

    #[test]
    fn the_west_and_north_border_edges_count_as_inside_from_1_21() {
        // WorldBorder.isWithinBounds: `>` up to 1.20.6 (client1.20.6 WorldBorder.java:44-50),
        // `>=` from 1.21 (client1.21 :57-63); isInsideCloseToBorder uses the margin form.
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        let edge = Vec3::new(-5.0, 0.0, 0.0);
        let old = MinecraftVersion::new(1, 20, 6);
        let new = MinecraftVersion::new(1, 21, 0);
        assert!(!world.is_inside_world_border(edge, old));
        assert!(world.is_inside_world_border(edge, new));
        let swept = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let margin_edge = Vec3::new(-6.0, 0.0, 0.0);
        assert!(!world.is_inside_close_to_world_border(margin_edge, swept, old));
        assert!(world.is_inside_close_to_world_border(margin_edge, swept, new));
    }

    #[test]
    fn world_border_edges_are_clamped_to_the_absolute_max_size() {
        // WorldBorder.StaticBorderExtent.getMinX: Mth.clamp(center - size / 2, -29999984,
        // 29999984) (client1.20.6 WorldBorder.java:384-398).
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 1.0e9);
        let release = MinecraftVersion::new(1, 21, 11);
        assert!(!world.is_inside_world_border(Vec3::new(29999984.5, 0.0, 0.0), release));
        assert_eq!(
            world.clamp_to_world_border(Vec3::new(4.0e7, 70.5, -4.0e7), release),
            Vec3::new(29999984.0 - 9.999999747378752E-6, 70.5, -29999984.0)
        );
    }

    #[test]
    fn world_border_edges_are_clamped_to_the_absolute_max_size_the_server_sends() {
        // WorldBorder.StaticBorderExtent.updateBox clamps each edge to +-absoluteMaxSize (1.21.11
        // WorldBorder.java:558-561), which the init packet sets (ClientPacketListener.java:1805;
        // MinecraftServer.java:476, max-world-size).
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 5.9999968e7);
        world.set_world_border_absolute_max_size(1000);
        let release = MinecraftVersion::new(1, 21, 11);
        assert!(world.is_inside_world_border(Vec3::new(999.5, 0.0, 0.0), release));
        assert!(!world.is_inside_world_border(Vec3::new(1000.0, 0.0, -999.0), release));
        let [west, east, north, south] = world.world_border_collision_boxes();
        assert_eq!(
            (west.max.x, east.min.x, north.max.z, south.min.z),
            (-1000.0, 1000.0, -1000.0, 1000.0)
        );
    }

    #[test]
    fn clamping_to_the_border_follows_the_release() {
        let mut world = WorldSnapshot::new();
        world.set_world_bounds(-64, 320, 0.0, 0.0, 10.0);
        let outside = Vec3::new(7.25, 70.5, -7.25);
        // clampToBounds: the block position of the clamped point (client1.20.6
        // WorldBorder.java:56-58), from 1.21 a block short of the east edge (client1.21 :73-75).
        assert_eq!(
            world.clamp_to_world_border(outside, MinecraftVersion::new(1, 20, 6)),
            Vec3::new(5.0, 70.0, -5.0)
        );
        assert_eq!(
            world.clamp_to_world_border(outside, MinecraftVersion::new(1, 21, 1)),
            Vec3::new(4.0, 70.0, -5.0)
        );
        // clampVec3ToBound from 1.21.2: 9.999999747378752E-6 short of the east edge
        // (client1.21.2 WorldBorder.java:81-83).
        assert_eq!(
            world.clamp_to_world_border(outside, MinecraftVersion::new(1, 21, 2)),
            Vec3::new(5.0 - 9.999999747378752E-6, 70.5, -5.0)
        );
    }

    #[test]
    fn an_unloaded_chunk_reads_as_air_with_no_fluid() {
        // ClientChunkCache.getChunk gives the empty chunk: VOID_AIR and Fluids.EMPTY
        // (1.16.5 EmptyChunk.java:33-46, client26.3 EmptyLevelChunk.java:25-35).
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape {
                    slipperiness: 0.98,
                    ..BlockShape::full_block()
                }),
                fluid: FluidState::Water,
                ..BlockState::default()
            },
        );
        world.set_block_properties((0, 0, 0), vec![("level".to_owned(), "0".to_owned())]);
        world.set_chunk_loaded(0, 0, false);
        assert_eq!(world.block((0, 0, 0)), BlockState::default());
        assert_eq!(world.fluid_at((0, 0, 0)), FluidState::Empty);
        assert_eq!(world.slipperiness_at((0, 0, 0)), 0.6);
        assert!(!world.is_suffocating((0, 0, 0)));
        assert!(world.block_properties((0, 0, 0)).is_empty());
        world.set_chunk_loaded(0, 0, true);
        assert_eq!(world.fluid_at((0, 0, 0)), FluidState::Water);
    }

    #[test]
    fn fluid_is_not_touched_beside_an_unloaded_chunk_from_26_1() {
        // EntityFluidInteraction.update scans nothing unless every chunk under the cells widened
        // by one is loaded (26.1 EntityFluidInteraction.java:44, 94-110; ClientChunkCache.getChunk
        // gives null for one it has not, ClientChunkCache.java:71-80). Before 26.1 the client's
        // hasChunk is always true (1.21.11 Entity.java:3531, 3597-3603, ClientLevel.java:403-405).
        let mut world = WorldSnapshot::new();
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let bounds = Aabb::new(Vec3::new(0.2, 0.1, 0.2), Vec3::new(0.8, 1.9, 0.8));
        let touching = |world: &WorldSnapshot, release| {
            world
                .fluid_contact(FluidState::Water, bounds, release)
                .touching
        };
        // the cells widened by one reach the chunk columns -1 and 0 on both axes
        assert!(!touching(&world, MinecraftVersion::new(26, 1, 0)));
        assert!(touching(&world, MinecraftVersion::new(1, 21, 11)));
        world.set_chunk_loaded(-1, 0, true);
        world.set_chunk_loaded(0, -1, true);
        assert!(!touching(&world, MinecraftVersion::new(26, 1, 0)));
        world.set_chunk_loaded(-1, -1, true);
        assert!(touching(&world, MinecraftVersion::new(26, 1, 0)));
        assert!(touching(&world, MinecraftVersion::new(26, 3, 0)));
        world.set_chunk_loaded(-1, -1, false);
        assert!(!touching(&world, MinecraftVersion::new(26, 3, 0)));
    }

    #[test]
    fn the_eyes_in_fluid_test_follows_the_release() {
        // a water source alone tops at 8/9F: isInsideOfMaterial's (y + 1) - (1/9F - 0.11111111F)
        // before 1.13 (1.12.2 Entity.java:1128-1130), y + (8/9F + 0.11111111F) in float 1.13 -
        // 1.15 (1.15.2 Entity.isUnderLiquid bytecode 102-112), 0.11111111F below the eyes 1.16 -
        // 1.20.4 (1.20.4 Entity.java:1222-1229), at the eyes from 1.20.5 (1.20.5 :1237-1241), and
        // both ends included in double from 26.1 (26.1 EntityFluidInteraction.java:70)
        let mut world = WorldSnapshot::new();
        for chunk_x in -1..=0 {
            for chunk_z in -1..=0 {
                world.set_chunk_loaded(chunk_x, chunk_z, true);
            }
        }
        world.set_block(
            (0, 0, 0),
            BlockState {
                fluid: FluidState::Water,
                fluid_source: true,
                ..BlockState::default()
            },
        );
        let bounds = |eye_y: f64| {
            Aabb::new(
                Vec3::new(0.2, eye_y - 0.5, 0.2),
                Vec3::new(0.8, eye_y + 0.2, 0.8),
            )
        };
        let eyes = |eye_y: f64, release| {
            world.eyes_in_fluid(FluidState::Water, bounds(eye_y), (0.5, eye_y, 0.5), release)
        };
        let top = f64::from(8.0_f32 / 9.0);
        let v = MinecraftVersion::new;
        assert!(eyes(0.99, v(1, 12, 2)));
        assert!(eyes(0.99, v(1, 13, 2)));
        assert!(eyes(0.99, v(1, 15, 2)));
        assert!(!eyes(0.99, v(1, 20, 5)));
        assert!(eyes(0.99, v(1, 16, 0)));
        assert!(eyes(0.99, v(1, 20, 4)));
        assert!(!eyes(1.01, v(1, 20, 4)));
        assert!(eyes(0.85, v(1, 21, 11)));
        assert!(!eyes(top, v(1, 21, 11)));
        assert!(eyes(top, v(26, 1, 0)));
        assert!(!eyes(top + 1.0e-9, v(26, 1, 0)));
        assert!(!world.eyes_in_fluid(FluidState::Lava, bounds(0.5), (0.5, 0.5, 0.5), v(1, 21, 11)));
    }

    #[test]
    fn setting_a_block_in_an_unloaded_chunk_does_nothing() {
        // EmptyLevelChunk.setBlockState returns null and stores nothing (1.14 EmptyChunk.java:37-39,
        // client26.3 EmptyLevelChunk.java:29-31): the chunk stays unloaded.
        let mut world = WorldSnapshot::new();
        world.set_chunk_loaded(0, 0, false);
        world.set_block(
            (0, 0, 0),
            BlockState {
                shape: Some(BlockShape::full_block()),
                ..BlockState::default()
            },
        );
        world.set_block_shapes(
            (1, 0, 0),
            BlockState::default(),
            vec![BlockShape::full_block(), BlockShape::full_block()],
        );
        assert!(!world.is_chunk_loaded(0, 0));
        assert!(!world.contains_block((0, 0, 0)));
        assert!(!world.contains_block((1, 0, 0)));
        world.set_chunk_loaded(0, 0, true);
        assert!(world
            .collision_shapes(Aabb::new(Vec3::ZERO, Vec3::new(2.0, 1.0, 1.0)))
            .is_empty());
    }

    #[test]
    fn removing_a_block_removes_its_outline_and_interaction_boxes() {
        // Air has no outline or interaction shape (AirBlock.getShape is Shapes.empty()).
        let mut world = WorldSnapshot::new();
        let half = BlockShape {
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        let other = BlockShape {
            bounds: Aabb::new(Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
            ..BlockShape::full_block()
        };
        world.set_block_shapes((0, 0, 0), BlockState::default(), vec![half, other]);
        world.remove_block((0, 0, 0));
        let cell = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert!(world.collision_shapes(cell).is_empty());
        assert!(world.outline_shapes(cell).is_empty());
        assert!(world.interaction_shapes(cell).is_empty());
    }

    #[test]
    fn voxel_shape_boxes_are_wired_into_world_collision_queries() {
        let mut world = WorldSnapshot::new();
        let shape = VoxelShape::union(
            MinecraftVersion::new(1, 21, 11),
            &[
                VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(0.5, 1.0, 1.0))),
                VoxelShape::from_box(Aabb::new(
                    Vec3::new(0.5, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 1.0),
                )),
            ],
        );
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
        // Shapes.or merges the two halves into one full cube (1.21.11 VoxelShape.java:96-100).
        assert_eq!(shapes.len(), 1);
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

    #[test]
    fn a_shape_overlapping_the_query_within_1e_7_does_not_meet_it_from_1_13() {
        // Shapes.joinIsNotEmpty(shape, box, AND): the 1e-7 early-out (client1.21.11 Shapes.java
        // :179-185) and the merger's 1e-7 merging (client1.21.11 IndirectMerger.java ctor, 1.16.5
        // SimplePairList.java:34-37); a slab x 0 - 0.5 against a query from 0.5 - eps
        let slab = Aabb::new(Vec3::ZERO, Vec3::new(0.5, 0.5, 1.0));
        let query = |eps: f64| Aabb::new(Vec3::new(0.5 - eps, 0.0, 0.0), Vec3::new(1.5, 1.0, 1.0));
        let release = |version: &str| MinecraftVersion::parse(version).unwrap();
        for version in ["1.13.2", "1.14.4", "1.16.5", "1.17.1", "1.21.11"] {
            for eps in [1.0e-16, 1.0e-9, 5.0e-8] {
                assert!(
                    !super::shape_meets_query(slab, query(eps), release(version)),
                    "{version} {eps}"
                );
            }
            assert!(
                super::shape_meets_query(slab, query(2.0e-7), release(version)),
                "{version}"
            );
            assert!(
                super::shape_meets_query(slab, query(0.25), release(version)),
                "{version}"
            );
            assert!(
                !super::shape_meets_query(slab, query(-0.25), release(version)),
                "{version}"
            );
        }
        // an overlap of 1e-7 itself: 1.13.2 merges only coordinates closer than 1e-7
        // (class_4092.java:37 `d > e - 1.0E-7`), 1.14 on those at 1e-7 too
        // (SimpleDoubleListPair.java:37 `d >= e - 1.0E-7`)
        assert!(super::shape_meets_query(slab, query(1.0e-7), release("1.13.2")));
        for version in ["1.14.4", "1.16.5", "1.17.1", "1.21.11"] {
            assert!(
                !super::shape_meets_query(slab, query(1.0e-7), release(version)),
                "{version}"
            );
        }
        // strict before 1.13 (1.12.2 Block.java:395-399)
        assert!(super::shape_meets_query(
            slab,
            query(1.0e-9),
            release("1.12.2")
        ));
        // a full cube: strict from 1.16 (1.16.5 BlockCollisionSpliterator.java:75-77), merged
        // before (1.15 CollisionView.java:109)
        let cube = Aabb::new(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 1.0));
        let touching = Aabb::new(Vec3::new(-5.0e-8, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        assert!(!super::shape_meets_query(cube, touching, release("1.15.2")));
        assert!(super::shape_meets_query(cube, touching, release("1.16.5")));
        // the query's own box: IdenticalMerger
        assert!(super::shape_meets_query(slab, slab, release("1.21.11")));
    }
}
