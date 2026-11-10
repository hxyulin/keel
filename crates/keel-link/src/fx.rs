//! A fast non-cryptographic hasher for maps keyed by symbol names. Link
//! inputs are trusted to the same degree as the code they contain, so hash
//! flooding resistance is not needed.
use std::hash::{BuildHasherDefault, Hasher};

pub(crate) type Map<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FxHasher>>;
pub(crate) type Set<K> = std::collections::HashSet<K, BuildHasherDefault<FxHasher>>;

/// FxHash, as used by rustc: rotate, xor and multiply per word.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FxHasher(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
    fn finish(&self) -> u64 {
        // hashbrown indexes buckets with the low bits; move the mixed high bits there.
        self.0.rotate_left(26)
    }
}
