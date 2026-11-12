//! A fixed, fast hasher for the engine's own maps (the world's cells, chunks and sections), in
//! place of the standard library's SipHash with a random key. Nothing the engine computes depends
//! on a map's order: what iterates one sorts first (the checkpoint, the collision walk). So the
//! hasher changes how long a lookup takes and nothing else.
//!
//! The algorithm is rustc-hash 2's FxHasher: each word is added and multiplied by one odd
//! constant, and the finish rotates the well-mixed high bits down to the bucket index.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

pub type FastBuild = BuildHasherDefault<FxHasher>;
pub type FastMap<K, V> = HashMap<K, V, FastBuild>;
pub type FastSet<K> = HashSet<K, FastBuild>;

const K: u64 = 0xf135_7aea_2e62_a9c5;

#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = self.hash.wrapping_add(word).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8])));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
        self.add(bytes.len() as u64);
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    #[inline]
    fn write_i32(&mut self, value: i32) {
        self.add(u64::from(value as u32));
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash.rotate_left(26)
    }
}

#[cfg(test)]
mod tests {
    use super::FastMap;

    #[test]
    fn cells_of_a_grid_stay_apart() {
        let mut map = FastMap::default();
        for x in -40..40 {
            for y in -64..64 {
                for z in -40..40 {
                    map.insert((x, y, z), x * 31 + y * 7 + z);
                }
            }
        }
        assert_eq!(map.len(), 80 * 128 * 80);
        assert_eq!(map.get(&(-40, -64, -40)), Some(&(-40 * 31 - 64 * 7 - 40)));
        assert_eq!(map.get(&(39, 63, 39)), Some(&(39 * 31 + 63 * 7 + 39)));
        assert_eq!(map.get(&(40, 0, 0)), None);
    }
}
