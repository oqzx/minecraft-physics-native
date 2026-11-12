//! Writes into a world that are taken back: the world a move sees for a while (a piston's NOCLIP,
//! a legacy boat's cleared lily pads, a falling block's own cell) as the world itself, written
//! over in place and put back as it was, not as a copy of the whole world.

use super::{BlockPosition, BlockShape, BlockState, MovingBlockCell, WorldSnapshot};

/// The cells as they were before the writes since, newest last: WorldSnapshot::restore puts them
/// back in the reverse order.
#[derive(Debug, Default)]
pub(crate) struct WorldPatches(Vec<SavedCell>);

/// Everything of a cell that set_block, set_moving_block and set_moving_block_noclip write.
#[derive(Debug)]
struct SavedCell {
    position: BlockPosition,
    chunk_loaded: bool,
    block: Option<BlockState>,
    moving: Option<MovingBlockCell>,
    extra: [Option<Vec<BlockShape>>; 3],
    /// Whether the chunk's index held the cell, none when it had no entry.
    in_chunk: Option<bool>,
    in_section: Option<bool>,
    /// In a chunk marked unloaded, the shared empty chunk's cell (set_block writes there): none
    /// when the world had no empty chunk.
    empty_chunk: Option<Option<Box<SavedCell>>>,
}

/// The world written over through set_block for a while: put back as it was when this is dropped.
pub(crate) struct Patched<'a> {
    world: &'a mut WorldSnapshot,
    patches: WorldPatches,
}

impl Patched<'_> {
    /// set_block, taken back with the rest.
    pub(crate) fn set_block(&mut self, position: BlockPosition, state: BlockState) {
        self.world.patch_block(&mut self.patches, position, state);
    }
}

impl std::ops::Deref for Patched<'_> {
    type Target = WorldSnapshot;

    fn deref(&self) -> &WorldSnapshot {
        self.world
    }
}

impl Drop for Patched<'_> {
    fn drop(&mut self) {
        self.world.restore(&mut self.patches);
    }
}

impl WorldPatches {
    /// Keeps the cell as it is, for a write to it that restore takes back.
    pub(crate) fn save(&mut self, world: &WorldSnapshot, position: BlockPosition) {
        self.0.push(world.save_cell(position));
    }
}

impl WorldSnapshot {
    /// The world to write over for a while (Patched).
    pub(crate) fn patched(&mut self) -> Patched<'_> {
        Patched { world: self, patches: WorldPatches::default() }
    }

    fn save_cell(&self, position: BlockPosition) -> SavedCell {
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        let section = (chunk.0, position.1.div_euclid(16), chunk.1);
        SavedCell {
            position,
            chunk_loaded: self.loaded_chunks.contains(&chunk),
            block: self.blocks.get(&position).copied(),
            moving: self.moving_blocks.get(&position).cloned(),
            extra: [
                self.extra_shapes.get(&position).cloned(),
                self.extra_outline_shapes.get(&position).cloned(),
                self.extra_interaction_shapes.get(&position).cloned(),
            ],
            in_chunk: self.chunks.get(&chunk).map(|cells| cells.contains(&position)),
            in_section: self.sections.get(&section).map(|cells| cells.contains(&position)),
            empty_chunk: self.unloaded_chunks.contains(&chunk).then(|| {
                self.empty_chunk
                    .as_ref()
                    .map(|shared| Box::new(shared.save_cell(super::in_empty_chunk(position))))
            }),
        }
    }

    fn restore_cell(&mut self, saved: SavedCell) {
        let position = saved.position;
        let chunk = (position.0.div_euclid(16), position.2.div_euclid(16));
        let section = (chunk.0, position.1.div_euclid(16), chunk.1);
        if saved.chunk_loaded {
            self.loaded_chunks.insert(chunk);
        } else {
            self.loaded_chunks.remove(&chunk);
        }
        put_back(&mut self.blocks, position, saved.block);
        put_back(&mut self.moving_blocks, position, saved.moving);
        let [extra, outline, interaction] = saved.extra;
        put_back(&mut self.extra_shapes, position, extra);
        put_back(&mut self.extra_outline_shapes, position, outline);
        put_back(&mut self.extra_interaction_shapes, position, interaction);
        put_back_member(&mut self.chunks, chunk, position, saved.in_chunk);
        put_back_member(&mut self.sections, section, position, saved.in_section);
        match saved.empty_chunk {
            None => {}
            Some(None) => self.empty_chunk = None,
            Some(Some(cell)) => {
                if let Some(shared) = self.empty_chunk.as_mut() {
                    shared.restore_cell(*cell);
                }
            }
        }
    }

    /// Puts back every cell the patches kept, newest first, which leaves the world as it was
    /// before the first.
    pub(crate) fn restore(&mut self, patches: &mut WorldPatches) {
        while let Some(saved) = patches.0.pop() {
            self.restore_cell(saved);
        }
    }

    /// set_block, taken back by restore.
    pub(crate) fn patch_block(
        &mut self,
        patches: &mut WorldPatches,
        position: BlockPosition,
        state: BlockState,
    ) {
        patches.save(self, position);
        self.set_block(position, state);
    }
}

fn put_back<T>(cells: &mut crate::fast_hash::FastMap<BlockPosition, T>, position: BlockPosition, saved: Option<T>) {
    match saved {
        Some(value) => {
            cells.insert(position, value);
        }
        None => {
            cells.remove(&position);
        }
    }
}

/// The index entry `key` holds the cell again as it did: `held` none when the entry was not there.
fn put_back_member<K: std::hash::Hash + Eq>(
    index: &mut crate::fast_hash::FastMap<K, crate::fast_hash::FastSet<BlockPosition>>,
    key: K,
    position: BlockPosition,
    held: Option<bool>,
) {
    match held {
        None => {
            index.remove(&key);
        }
        Some(true) => {
            index.entry(key).or_default().insert(position);
        }
        Some(false) => {
            if let Some(cells) = index.get_mut(&key) {
                cells.remove(&position);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Aabb, Vec3};

    fn sorted<K: Copy + Ord>(
        index: &crate::fast_hash::FastMap<K, crate::fast_hash::FastSet<BlockPosition>>,
    ) -> Vec<(K, Vec<BlockPosition>)> {
        let mut cells: Vec<_> = index
            .iter()
            .map(|(key, cells)| {
                let mut cells: Vec<_> = cells.iter().copied().collect();
                cells.sort_unstable();
                (*key, cells)
            })
            .collect();
        cells.sort_unstable();
        cells
    }

    /// The world as its fields hold it, the indices a checkpoint leaves out too.
    fn fields(world: &WorldSnapshot) -> String {
        format!(
            "{:?} {:?} {:?} {:?}",
            world.encode(),
            sorted(&world.chunks),
            sorted(&world.sections),
            world.empty_chunk.as_ref().map(|shared| fields(shared))
        )
    }

    fn stone() -> BlockState {
        BlockState {
            id: 1,
            shape: Some(BlockShape::full_block()),
            ..BlockState::default()
        }
    }

    #[test]
    fn a_noclip_move_is_taken_back() {
        let mut world = WorldSnapshot::new();
        let moving = (1, 100, 0);
        world.set_block(moving, stone());
        let head = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.25));
        let plate = Aabb::new(Vec3::ZERO, Vec3::new(0.25, 1.0, 1.0));
        world.set_moving_block(moving, &[head, plate], true);
        world.set_moving_block_noclip(moving, (1, 0, 0), &[plate]);
        let before = fields(&world);
        let mut patches = WorldPatches::default();
        world.through_moving_blocks(&mut patches, (0, 1, 0));
        assert_eq!(fields(&world), before, "no cell moves up");
        world.through_moving_blocks(&mut patches, (1, 0, 0));
        assert_eq!(world.block_collision_local_boxes(moving), vec![plate]);
        assert!(!world.moves_through((1, 0, 0)));
        world.restore(&mut patches);
        assert_eq!(fields(&world), before);
        assert!(world.moves_through((1, 0, 0)));
    }

    #[test]
    fn a_cleared_block_is_put_back_in_a_loaded_chunk_an_unloaded_one_and_a_new_cell() {
        let mut world = WorldSnapshot::new();
        world.set_release(crate::version::MinecraftVersion::new(1, 8, 8));
        world.set_block((0, 64, 0), stone());
        world.set_block_shapes((1, 64, 0), stone(), vec![BlockShape::full_block(), BlockShape::full_block()]);
        world.set_chunk_loaded(1, 0, false);
        world.set_block((17, 64, 0), stone());
        let before = fields(&world);
        let mut patches = WorldPatches::default();
        for cell in [(0, 64, 0), (1, 64, 0), (17, 64, 0), (40, 64, 40), (0, 64, 0)] {
            world.patch_block(&mut patches, cell, BlockState::default());
        }
        assert_ne!(fields(&world), before);
        world.restore(&mut patches);
        assert_eq!(fields(&world), before);
    }

    #[test]
    fn a_write_into_an_unloaded_chunk_takes_back_the_empty_chunk_it_made() {
        let mut world = WorldSnapshot::new();
        world.set_chunk_loaded(0, 0, false);
        let before = fields(&world);
        let mut patches = WorldPatches::default();
        world.patch_block(&mut patches, (3, 64, 3), stone());
        assert!(world.empty_chunk.is_some());
        world.restore(&mut patches);
        assert!(world.empty_chunk.is_none());
        assert_eq!(fields(&world), before);
    }
}
