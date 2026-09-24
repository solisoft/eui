//! A small, fast hasher for the maps the layout and the painter consult per
//! node, per frame.
//!
//! Every one of those maps is keyed on small integers the client made up
//! itself — a node index, a style id, a constraint's bits — so there is no
//! hostile key to defend against, and SipHash's resistance to one is a cost
//! paid on every lookup for an attack that cannot happen. A hit test on a
//! five-thousand-node page did two such lookups per node visited; a
//! relayout, several.
//!
//! This is the multiply-rotate hash rustc uses for the same job (`FxHasher`),
//! written out here rather than taken as a dependency: the crate promises it
//! depends on nothing beyond the other EUI crates, the published `rustc-hash`
//! wants a newer compiler than this workspace's `rust-version`, and the whole
//! thing is a dozen lines.
//!
//! Not for anything a server chooses the keys of. A map keyed on hashes or
//! strings off the wire keeps the standard hasher.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// The multiplier: 2^64 divided by the golden ratio, made odd.
const K: u64 = 0x517c_c1b7_2722_0a95;

/// See the module docs.
#[derive(Debug, Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut w = [0u8; 8];
            w.copy_from_slice(c);
            self.add(u64::from_le_bytes(w));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut w = [0u8; 8];
            for (d, s) in w.iter_mut().zip(rest) {
                *d = *s;
            }
            self.add(u64::from_le_bytes(w));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(u64::from(i));
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// A `BuildHasher` for [`FxHasher`].
pub type FxBuildHasher = BuildHasherDefault<FxHasher>;

/// A `HashMap` hashed with [`FxHasher`].
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// A `HashSet` hashed with [`FxHasher`].
pub type FxHashSet<K> = HashSet<K, FxBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of the layout's memo key.
    type Key = (u32, u32, (u8, u32), (u8, u32));

    #[test]
    fn a_map_keyed_on_small_integers_round_trips() {
        let mut m: FxHashMap<Key, u32> = FxHashMap::default();
        for i in 0..10_000u32 {
            m.insert((i, i ^ 7, (0, i), (2, 0)), i);
        }
        assert_eq!(m.len(), 10_000);
        for i in 0..10_000u32 {
            assert_eq!(m.get(&(i, i ^ 7, (0, i), (2, 0))), Some(&i));
        }
    }

    #[test]
    fn byte_writes_of_any_length_are_hashed_whole() {
        let h = |b: &[u8]| {
            let mut s = FxHasher::default();
            s.write(b);
            s.finish()
        };
        assert_ne!(h(b"abcdefghi"), h(b"abcdefghj"));
        assert_ne!(h(b"abc"), h(b"abd"));
    }
}
