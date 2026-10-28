//! A fast, non-cryptographic hasher for compiler-internal maps.
//!
//! This is the Firefox/rustc "Fx" hash: one rotate, xor and multiply per word.
//! It is not resistant to adversarial keys, which is fine for maps keyed by
//! entities, interned symbols and IR data. The standard `RandomState`
//! (SipHash-1-3) costs several times more per key and showed up as 4-5% of
//! optimized compile time. Hash order is fixed rather than randomized, so code
//! must still not depend on map iteration order for its output.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

pub type FxBuildHasher = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
pub type FxHashSet<T> = HashSet<T, FxBuildHasher>;

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Clone, Copy, Default, Debug)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while let Some((chunk, rest)) = bytes.split_first_chunk::<8>() {
            self.add(u64::from_le_bytes(*chunk));
            bytes = rest;
        }
        if let Some((chunk, rest)) = bytes.split_first_chunk::<4>() {
            self.add(u32::from_le_bytes(*chunk) as u64);
            bytes = rest;
        }
        for &b in bytes {
            self.add(b as u64);
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.add(i as u64);
        self.add((i >> 64) as u64);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    /// Rotated as in rustc-hash 2: the multiply mixes into the high bits,
    /// while hash tables pick buckets with the low ones. Unrotated, keys
    /// differing only above their low bits (aligned addresses, short
    /// strings) crowd the same buckets.
    #[inline]
    fn finish(&self) -> u64 {
        self.hash.rotate_left(26)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_round_trip() {
        let mut m: FxHashMap<u32, &str> = FxHashMap::default();
        for i in 0..1000 {
            m.insert(i, "x");
        }
        assert_eq!(m.len(), 1000);
        assert!((0..1000).all(|i| m.contains_key(&i)));
        let s: FxHashSet<&str> = ["a", "bb", "ccccccccc"].into_iter().collect();
        assert!(s.contains("ccccccccc") && !s.contains("c"));
    }
}
