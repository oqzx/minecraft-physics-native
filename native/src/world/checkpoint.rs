//! A world as bytes: everything a tick or query reads of a WorldSnapshot, in a sorted order, so that
//! equal worlds encode alike and a checkpoint can name its world by digest. Doubles go by their bits.
//! The first bytes name the schema: a decoder of another schema refuses it, and any change of what
//! this encodes takes a new one.

use super::{
    BlockEffect, BlockPosition, BlockShape, BlockState, ChunkPosition, EmptyChunkReads, EntityCollider, FluidState,
    MovingBlockCell, SectionPosition, VersionedBlockStateId, WorldSnapshot,
};
use crate::math::{Aabb, Vec3};
use crate::version::MinecraftVersion;
use crate::fast_hash::{FastMap, FastSet};

const MAGIC: &[u8; 8] = b"MCPNWLD3";

/// FNV-1a over the encoded world, which is what makes two worlds the same world to a checkpoint.
#[cfg_attr(not(feature = "bindings"), allow(dead_code))]
pub(crate) fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn flag(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn f64(&mut self, value: f64) {
        self.0.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    fn len(&mut self, value: usize) {
        self.u32(u32::try_from(value).expect("a world of fewer than 2^32 entries"));
    }
    fn text(&mut self, value: &str) {
        self.len(value.len());
        self.0.extend_from_slice(value.as_bytes());
    }
    fn vec3(&mut self, value: Vec3) {
        self.f64(value.x);
        self.f64(value.y);
        self.f64(value.z);
    }
    fn aabb(&mut self, value: Aabb) {
        self.vec3(value.min);
        self.vec3(value.max);
    }
    fn at(&mut self, value: BlockPosition) {
        self.i32(value.0);
        self.i32(value.1);
        self.i32(value.2);
    }
    fn chunk(&mut self, value: ChunkPosition) {
        self.i32(value.0);
        self.i32(value.1);
    }
    fn shape(&mut self, value: &BlockShape) {
        self.aabb(value.bounds);
        self.f64(value.slipperiness);
        self.u8(fluid_code(value.fluid));
        self.u8(value.effect.code());
    }
    fn maybe_shape(&mut self, value: Option<&BlockShape>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.shape(value);
        }
    }
    fn maybe_flag(&mut self, value: Option<bool>) {
        self.u8(value.map_or(2, u8::from));
    }
    fn maybe_f32(&mut self, value: Option<f32>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.u32(value.to_bits());
        }
    }
    fn state(&mut self, value: &BlockState) {
        self.u32(value.id);
        self.u32(value.tags);
        self.u32(value.fluid_tags);
        self.maybe_shape(value.shape.as_ref());
        self.maybe_shape(value.outline_shape.as_ref());
        self.maybe_shape(value.interaction_shape.as_ref());
        self.f64(value.bounce_restitution);
        self.flag(value.suppresses_bounce);
        self.u8(fluid_code(value.fluid));
        self.u8(value.fluid_level);
        self.flag(value.fluid_source);
        self.vec3(value.fluid_flow);
        self.flag(value.fluid_falling);
        self.flag(value.water_block);
        self.flag(value.fall_damage_resetting);
        self.flag(value.player_fall_damage_resetting);
        self.maybe_flag(value.suffocating);
        self.maybe_flag(value.in_wall);
        self.maybe_flag(value.bottom_face_sturdy);
        self.maybe_f32(value.explosion_resistance);
    }
    fn shapes(&mut self, map: &FastMap<BlockPosition, Vec<BlockShape>>) {
        let mut keys: Vec<_> = map.keys().copied().collect();
        keys.sort_unstable();
        self.len(keys.len());
        for key in keys {
            self.at(key);
            self.len(map[&key].len());
            for shape in &map[&key] {
                self.shape(shape);
            }
        }
    }
    fn chunks(&mut self, set: &FastSet<ChunkPosition>) {
        let mut keys: Vec<_> = set.iter().copied().collect();
        keys.sort_unstable();
        self.len(keys.len());
        for key in keys {
            self.chunk(key);
        }
    }
}

fn fluid_code(value: FluidState) -> u8 {
    match value {
        FluidState::Empty => 0,
        FluidState::Water => 1,
        FluidState::Lava => 2,
        FluidState::Waterlogged => 3,
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

type Decoded<T> = Result<T, String>;

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Decoded<&[u8]> {
        let end = self.at.checked_add(count).filter(|end| *end <= self.bytes.len());
        let end = end.ok_or("a world checkpoint ends early")?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }
    fn u8(&mut self) -> Decoded<u8> {
        Ok(self.take(1)?[0])
    }
    fn flag(&mut self) -> Decoded<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(format!("a world checkpoint holds {other} for a flag")),
        }
    }
    fn u32(&mut self) -> Decoded<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
    fn i32(&mut self) -> Decoded<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }
    fn f64(&mut self) -> Decoded<f64> {
        Ok(f64::from_bits(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes"))))
    }
    /// A count of entries: each takes a byte at least, so a broken one cannot ask for more room
    /// than the checkpoint has.
    fn len(&mut self) -> Decoded<usize> {
        let count = self.u32()? as usize;
        if count > self.bytes.len() - self.at {
            return Err("a world checkpoint counts more entries than it holds".to_string());
        }
        Ok(count)
    }
    fn text(&mut self) -> Decoded<String> {
        let count = self.len()?;
        String::from_utf8(self.take(count)?.to_vec()).map_err(|error| error.to_string())
    }
    fn vec3(&mut self) -> Decoded<Vec3> {
        Ok(Vec3::new(self.f64()?, self.f64()?, self.f64()?))
    }
    fn aabb(&mut self) -> Decoded<Aabb> {
        Ok(Aabb::new(self.vec3()?, self.vec3()?))
    }
    fn at(&mut self) -> Decoded<BlockPosition> {
        Ok((self.i32()?, self.i32()?, self.i32()?))
    }
    fn chunk(&mut self) -> Decoded<ChunkPosition> {
        Ok((self.i32()?, self.i32()?))
    }
    fn fluid(&mut self) -> Decoded<FluidState> {
        match self.u8()? {
            0 => Ok(FluidState::Empty),
            1 => Ok(FluidState::Water),
            2 => Ok(FluidState::Lava),
            3 => Ok(FluidState::Waterlogged),
            other => Err(format!("a world checkpoint holds fluid {other}")),
        }
    }
    fn effect(&mut self) -> Decoded<BlockEffect> {
        BlockEffect::from_code(self.u8()?)
    }
    fn shape(&mut self) -> Decoded<BlockShape> {
        Ok(BlockShape {
            bounds: self.aabb()?,
            slipperiness: self.f64()?,
            fluid: self.fluid()?,
            effect: self.effect()?,
        })
    }
    fn maybe_shape(&mut self) -> Decoded<Option<BlockShape>> {
        Ok(if self.flag()? { Some(self.shape()?) } else { None })
    }
    fn maybe_flag(&mut self) -> Decoded<Option<bool>> {
        match self.u8()? {
            0 => Ok(Some(false)),
            1 => Ok(Some(true)),
            2 => Ok(None),
            other => Err(format!("a world checkpoint holds {other} for an optional flag")),
        }
    }
    fn maybe_f32(&mut self) -> Decoded<Option<f32>> {
        Ok(if self.flag()? { Some(f32::from_bits(self.u32()?)) } else { None })
    }
    fn state(&mut self) -> Decoded<BlockState> {
        Ok(BlockState {
            id: self.u32()?,
            tags: self.u32()?,
            fluid_tags: self.u32()?,
            shape: self.maybe_shape()?,
            outline_shape: self.maybe_shape()?,
            interaction_shape: self.maybe_shape()?,
            bounce_restitution: self.f64()?,
            suppresses_bounce: self.flag()?,
            fluid: self.fluid()?,
            fluid_level: self.u8()?,
            fluid_source: self.flag()?,
            fluid_flow: self.vec3()?,
            fluid_falling: self.flag()?,
            water_block: self.flag()?,
            fall_damage_resetting: self.flag()?,
            player_fall_damage_resetting: self.flag()?,
            suffocating: self.maybe_flag()?,
            in_wall: self.maybe_flag()?,
            bottom_face_sturdy: self.maybe_flag()?,
            explosion_resistance: self.maybe_f32()?,
        })
    }
    fn shapes(&mut self) -> Decoded<FastMap<BlockPosition, Vec<BlockShape>>> {
        let mut map = FastMap::default();
        for _ in 0..self.len()? {
            let key = self.at()?;
            let count = self.len()?;
            let mut shapes = Vec::with_capacity(count);
            for _ in 0..count {
                shapes.push(self.shape()?);
            }
            map.insert(key, shapes);
        }
        Ok(map)
    }
    fn chunks(&mut self) -> Decoded<FastSet<ChunkPosition>> {
        let mut set = FastSet::default();
        for _ in 0..self.len()? {
            set.insert(self.chunk()?);
        }
        Ok(set)
    }
}

impl WorldSnapshot {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Writer(MAGIC.to_vec());
        let mut cells: Vec<_> = self.blocks.keys().copied().collect();
        cells.sort_unstable();
        out.len(cells.len());
        for cell in cells {
            out.at(cell);
            out.state(&self.blocks[&cell]);
        }
        let mut properties: Vec<_> = self.properties.keys().copied().collect();
        properties.sort_unstable();
        out.len(properties.len());
        for cell in properties {
            out.at(cell);
            out.len(self.properties[&cell].len());
            for (name, value) in &self.properties[&cell] {
                out.text(name);
                out.text(value);
            }
        }
        let mut ids: Vec<_> = self.state_ids.keys().copied().collect();
        ids.sort_unstable();
        out.len(ids.len());
        for cell in ids {
            out.at(cell);
            out.text(&self.state_ids[&cell].version.to_string());
            out.u32(self.state_ids[&cell].id);
        }
        let mut entities: Vec<_> = self.block_entities.keys().copied().collect();
        entities.sort_unstable();
        out.len(entities.len());
        for cell in entities {
            out.at(cell);
            out.text(&self.block_entities[&cell]);
        }
        out.shapes(&self.extra_shapes);
        out.shapes(&self.extra_outline_shapes);
        out.shapes(&self.extra_interaction_shapes);
        let mut moving: Vec<_> = self.moving_blocks.keys().copied().collect();
        moving.sort_unstable();
        out.len(moving.len());
        for cell in moving {
            let block = &self.moving_blocks[&cell];
            out.at(cell);
            out.flag(block.edge_cells);
            out.flag(block.noclip.is_some());
            if let Some((direction, boxes)) = &block.noclip {
                out.at(*direction);
                out.len(boxes.len());
                for bounds in boxes {
                    out.aabb(*bounds);
                }
            }
        }
        let mut sections: Vec<SectionPosition> = self.explicit_sections.iter().copied().collect();
        sections.sort_unstable();
        out.len(sections.len());
        for section in sections {
            out.at(section);
        }
        out.chunks(&self.loaded_chunks);
        out.chunks(&self.unloaded_chunks);
        out.i32(self.min_build_height);
        out.i32(self.max_build_height);
        out.f64(self.border_center.0);
        out.f64(self.border_center.1);
        out.f64(self.border_size);
        out.f64(self.border_absolute_max);
        out.len(self.entity_colliders.len());
        for collider in &self.entity_colliders {
            out.aabb(collider.bounds);
            out.flag(collider.only_from_above);
        }
        // the shared empty chunk and what the release reads of it (EmptyChunkReads)
        out.u8(match self.empty_chunk_reads {
            EmptyChunkReads::Nothing => 0,
            EmptyChunkReads::Blocks => 1,
            EmptyChunkReads::Fluids => 2,
        });
        out.flag(self.empty_chunk.is_some());
        if let Some(shared) = &self.empty_chunk {
            let bytes = shared.encode();
            out.len(bytes.len());
            out.0.extend_from_slice(&bytes);
        }
        out.0
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
            return Err("not a world checkpoint of this schema (MCPNWLD3)".to_string());
        }
        let mut input = Reader { bytes, at: MAGIC.len() };
        let mut world = Self::default();
        for _ in 0..input.len()? {
            let cell = input.at()?;
            let state = input.state()?;
            world.blocks.insert(cell, state);
            let chunk = (cell.0.div_euclid(16), cell.2.div_euclid(16));
            world.chunks.entry(chunk).or_default().insert(cell);
            let section = (chunk.0, cell.1.div_euclid(16), chunk.1);
            world.sections.entry(section).or_default().insert(cell);
        }
        for _ in 0..input.len()? {
            let cell = input.at()?;
            let count = input.len()?;
            let mut pairs = Vec::with_capacity(count);
            for _ in 0..count {
                pairs.push((input.text()?, input.text()?));
            }
            world.properties.insert(cell, pairs);
        }
        for _ in 0..input.len()? {
            let cell = input.at()?;
            let version = input.text()?;
            let id = input.u32()?;
            let version = MinecraftVersion::parse(&version)
                .ok_or_else(|| format!("a world checkpoint names release {version}"))?;
            world.state_ids.insert(cell, VersionedBlockStateId::new(version, id)?);
        }
        for _ in 0..input.len()? {
            let cell = input.at()?;
            world.block_entities.insert(cell, input.text()?);
        }
        world.extra_shapes = input.shapes()?;
        world.extra_outline_shapes = input.shapes()?;
        world.extra_interaction_shapes = input.shapes()?;
        for _ in 0..input.len()? {
            let cell = input.at()?;
            let edge_cells = input.flag()?;
            let noclip = if input.flag()? {
                let direction = input.at()?;
                let count = input.len()?;
                let mut boxes = Vec::with_capacity(count);
                for _ in 0..count {
                    boxes.push(input.aabb()?);
                }
                Some((direction, boxes))
            } else {
                None
            };
            world.moving_blocks.insert(cell, MovingBlockCell { edge_cells, noclip });
        }
        for _ in 0..input.len()? {
            world.explicit_sections.insert(input.at()?);
        }
        world.loaded_chunks = input.chunks()?;
        world.unloaded_chunks = input.chunks()?;
        world.min_build_height = input.i32()?;
        world.max_build_height = input.i32()?;
        world.border_center = (input.f64()?, input.f64()?);
        world.border_size = input.f64()?;
        world.border_absolute_max = input.f64()?;
        for _ in 0..input.len()? {
            let bounds = input.aabb()?;
            let only_from_above = input.flag()?;
            world.entity_colliders.push(EntityCollider { bounds, only_from_above });
        }
        world.empty_chunk_reads = match input.u8()? {
            0 => EmptyChunkReads::Nothing,
            1 => EmptyChunkReads::Blocks,
            2 => EmptyChunkReads::Fluids,
            other => return Err(format!("a world checkpoint reads the empty chunk as {other}")),
        };
        if input.flag()? {
            let count = input.len()?;
            world.empty_chunk = Some(Box::new(Self::decode(input.take(count)?)?));
        }
        if input.at != bytes.len() {
            return Err("a world checkpoint goes on past its end".to_string());
        }
        Ok(world)
    }
}

#[cfg(test)]
#[path = "../checkpoint_tests.rs"]
mod tests;
