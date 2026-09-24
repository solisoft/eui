//! A fast hasher for the arena's integer-keyed maps.
//!
//! The maps this crate looks up on every op and every hit test are keyed on
//! small integers — `(owner, node id)`, key atoms, arena indices — and std's
//! SipHash spends most of each lookup on a guarantee sized for strings. What
//! is here is one folded 64×64→128 multiply per integer written and one more
//! to finish, the core of `foldhash` and `wyhash`, and nothing else.
//!
//! **Seeded, and not the unkeyed FxHash**, because the ids are a server's.
//! A node id is whatever the server chose, and FxHash's low bits are a
//! function of the key's low bits alone: a server that numbered its nodes in
//! multiples of 4 096 would put every one of them in the same probe chain,
//! and a million-node `Mount` would cost a million squared. Folding the high
//! half of the product into the low half moves every input bit into the
//! bucket index, and a seed drawn from std's own per-process randomness means
//! the collisions cannot be worked out ahead of time. That is the same
//! minimum `foldhash` claims, and it is not SipHash's: a map keyed on
//! strings a server sends keeps SipHash.
//!
//! In-crate rather than a dependency: forty lines, on the path a hostile
//! batch reaches, in a crate whose dependency list is `eui-proto` alone.

use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

/// A `HashMap` over [`Fold`].
pub(crate) type FastMap<K, V> = HashMap<K, V, FoldState>;

/// A `HashSet` over [`Fold`].
pub(crate) type FastSet<K> = HashSet<K, FoldState>;

/// Odd constants with no structure — the fractional digits of pi, as
/// `foldhash` uses.
const MULTIPLE: u64 = 0x243f_6a88_85a3_08d3;
const FINISH: u64 = 0x1319_8a2e_0370_7345;

/// Multiply, and fold the 128-bit product into 64 bits so the high half —
/// which depends on every bit of both operands — lands in the low bits a
/// hash table takes its bucket from.
#[inline]
fn fold(a: u64, b: u64) -> u64 {
    let product = u128::from(a).wrapping_mul(u128::from(b));
    (product as u64) ^ ((product >> 64) as u64)
}

/// A hasher for integer keys. See the module notes for why it is seeded.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fold {
    state: u64,
}

impl Hasher for Fold {
    #[inline]
    fn write_u64(&mut self, v: u64) {
        self.state = fold(self.state ^ v, MULTIPLE);
    }

    #[inline]
    fn write_u32(&mut self, v: u32) {
        self.write_u64(u64::from(v));
    }

    #[inline]
    fn write_u16(&mut self, v: u16) {
        self.write_u64(u64::from(v));
    }

    #[inline]
    fn write_u8(&mut self, v: u8) {
        self.write_u64(u64::from(v));
    }

    #[inline]
    fn write_usize(&mut self, v: usize) {
        self.write_u64(v as u64);
    }

    /// Anything that is not an integer. Nothing in this crate hashes one,
    /// but the trait requires it, and eight bytes at a time is correct for
    /// whatever does.
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            for (w, b) in word.iter_mut().zip(chunk) {
                *w = *b;
            }
            self.write_u64(u64::from_le_bytes(word));
        }
        self.write_usize(bytes.len());
    }

    /// One more fold on the way out. A single multiply is linear in its
    /// input: ids a stride apart land a stride apart, and 4 096 ids that
    /// share their low twelve bits filled only about 600 of 4 096 buckets.
    /// The second fold is what makes them look random — about 2 600, which
    /// is what random keys fill.
    #[inline]
    fn finish(&self) -> u64 {
        fold(self.state, FINISH)
    }
}

/// Builds a [`Fold`] from a seed drawn once per map.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FoldState {
    seed: u64,
}

impl Default for FoldState {
    /// A seed from std's `RandomState`: per process, and varied per map, and
    /// no dependency to draw it.
    fn default() -> Self {
        Self { seed: RandomState::new().hash_one(0x5eed_u64) | 1 }
    }
}

impl BuildHasher for FoldState {
    type Hasher = Fold;

    #[inline]
    fn build_hasher(&self) -> Fold {
        Fold { state: self.seed }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects, clippy::indexing_slicing)]

    use super::*;

    /// The case the seed and the fold are there for: ids a server chose to
    /// share their low bits. Unkeyed FxHash puts every one of these in one
    /// bucket of a 4 096-bucket table; this must spread them.
    #[test]
    fn ids_that_share_their_low_bits_still_spread() {
        let state = FoldState::default();
        let mut buckets = std::collections::HashSet::new();
        for i in 1..=4096u32 {
            buckets.insert(state.hash_one(i << 12) & 4095);
        }
        // Random keys fill 1 - 1/e of them, about 2 589.
        assert!(buckets.len() > 2300, "only {} of 4096 buckets used", buckets.len());
    }

    #[test]
    fn the_maps_work() {
        let mut m: FastMap<(u16, u32), u32> = FastMap::default();
        for i in 0..10_000u32 {
            m.insert(((i % 3) as u16, i), i);
        }
        for i in 0..10_000u32 {
            assert_eq!(m.get(&((i % 3) as u16, i)), Some(&i));
        }
        assert_eq!(m.get(&(1, 0)), None);
    }
}
