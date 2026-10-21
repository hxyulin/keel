//! Compilation-scoped byte storage and interned names. Frontend references
//! cannot outlive the arena; the returned IR owns the strings it needs.
use bumpalo::Bump;
use keel_ir::fxhash::FxHashSet as HashSet;

pub(crate) struct Strings<'a> {
    arena: &'a Bump,
    /// Interned names by open addressing with linear probing: each slot
    /// the name's hash and the name. A power of two, at most half full.
    names: Vec<(u32, Option<&'a str>)>,
    count: usize,
    literals: HashSet<&'a [u8]>,
}

/// A word-at-a-time multiplicative hash. Only the top bits of the last
/// product depend on every byte, so slot indices take their bits from the
/// top.
fn hash(bytes: &[u8]) -> u32 {
    const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;
    let mut h = bytes.len() as u64;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(chunk.try_into().unwrap())).wrapping_mul(K);
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        let word = rest.iter().fold(0, |w, &b| w << 8 | u64::from(b));
        h = (h.rotate_left(5) ^ word).wrapping_mul(K);
    }
    (h >> 32) as u32
}

/// The first slot to look in for hash `h`, in a table of `len` slots.
fn slot(h: u32, len: usize) -> usize {
    (h >> (32 - len.trailing_zeros())) as usize
}

impl<'a> Strings<'a> {
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            names: vec![(0, None); 1024],
            count: 0,
            literals: HashSet::default(),
        }
    }

    pub fn intern(&mut self, text: &str) -> &'a str {
        self.intern_utf8(text.as_bytes())
    }

    /// Interns UTF-8 `text`, such as source spellings, validated only when
    /// new. Panics if it is not UTF-8.
    pub fn intern_utf8(&mut self, text: &[u8]) -> &'a str {
        let h = hash(text);
        let mask = self.names.len() - 1;
        let mut i = slot(h, self.names.len());
        loop {
            match self.names[i] {
                (stored, Some(name)) => {
                    if stored == h && name.as_bytes() == text {
                        return name;
                    }
                }
                (_, None) => break,
            }
            i = (i + 1) & mask;
        }
        let name = self.arena.alloc_str(std::str::from_utf8(text).unwrap());
        self.names[i] = (h, Some(name));
        self.count += 1;
        if self.count * 2 > self.names.len() {
            self.grow();
        }
        name
    }

    fn grow(&mut self) {
        let size = self.names.len() * 2;
        let old = std::mem::replace(&mut self.names, vec![(0, None); size]);
        let mask = self.names.len() - 1;
        for (h, name) in old {
            if name.is_some() {
                let mut i = slot(h, self.names.len());
                while self.names[i].1.is_some() {
                    i = (i + 1) & mask;
                }
                self.names[i] = (h, name);
            }
        }
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> &'a [u8] {
        if let Some(&value) = self.literals.get(bytes) {
            return value;
        }
        let value = self.arena.alloc_slice_copy(bytes);
        self.literals.insert(value);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_survive_growth_and_binary_literals_are_distinct_from_names() {
        let arena = Bump::new();
        let mut strings = Strings::new(&arena);
        let name = strings.intern("repeated_λ");
        let binary = strings.bytes(&[0, 255, 0, 128]);
        for i in 0..10_000u32 {
            strings.intern(&format!("long_identifier_with_distinct_spelling_{i}"));
            strings.bytes(&i.to_le_bytes());
        }
        assert_eq!(name, "repeated_λ");
        assert_eq!(binary, &[0, 255, 0, 128]);
        assert!(std::ptr::eq(name, strings.intern("repeated_λ")));
        assert!(std::ptr::eq(binary, strings.bytes(&[0, 255, 0, 128])));
        assert_ne!(strings.intern("Repeated_λ"), name);
        for i in 0..10_000u32 {
            let spelling = format!("long_identifier_with_distinct_spelling_{i}");
            let a = strings.intern(&spelling);
            assert_eq!(a, spelling);
            assert!(std::ptr::eq(a, strings.intern(&spelling)));
        }
        assert!(std::ptr::eq(strings.intern(""), strings.intern("")));
    }
}
