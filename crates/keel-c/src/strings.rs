//! Compilation-scoped byte storage and interned names. Frontend references
//! cannot outlive the arena; the returned IR owns the strings it needs.
use bumpalo::Bump;
use keel_ir::fxhash::FxHashSet as HashSet;

pub(crate) struct Strings<'a> {
    arena: &'a Bump,
    /// Interned names by open addressing with linear probing. A power of
    /// two, at most half full.
    names: Vec<Slot<'a>>,
    count: usize,
    literals: HashSet<&'a [u8]>,
}

#[derive(Clone, Copy, Default)]
struct Slot<'a> {
    key: Key,
    name: Option<&'a str>,
}

/// What a lookup compares before the bytes: a name's hash, length and
/// first word, which is all of a name of up to eight bytes.
#[derive(Clone, Copy, Default, PartialEq)]
struct Key {
    hash: u32,
    len: u32,
    head: u64,
}

/// Each word of `bytes` folded in by multiplication. Only the top bits of
/// the last product depend on every byte, so slot indices take their bits
/// from the top.
fn key(bytes: &[u8]) -> Key {
    const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;
    let mut h = bytes.len() as u64;
    let mut head = None;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().unwrap());
        head.get_or_insert(word);
        h = (h.rotate_left(5) ^ word).wrapping_mul(K);
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        let word = rest.iter().fold(0, |w, &b| w << 8 | u64::from(b));
        head.get_or_insert(word);
        h = (h.rotate_left(5) ^ word).wrapping_mul(K);
    }
    Key {
        hash: (h >> 32) as u32,
        len: bytes.len() as u32,
        head: head.unwrap_or(0),
    }
}

/// The first slot to look in for hash `h`, in a table of `len` slots.
fn slot(h: u32, len: usize) -> usize {
    (h >> (32 - len.trailing_zeros())) as usize
}

impl<'a> Strings<'a> {
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            names: vec![Slot::default(); 1024],
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
        let key = key(text);
        let mask = self.names.len() - 1;
        let mut i = slot(key.hash, self.names.len());
        loop {
            match self.names[i] {
                Slot {
                    key: stored,
                    name: Some(name),
                } => {
                    if stored == key && (text.len() <= 8 || name.as_bytes() == text) {
                        return name;
                    }
                }
                Slot { name: None, .. } => break,
            }
            i = (i + 1) & mask;
        }
        let name = self.arena.alloc_str(std::str::from_utf8(text).unwrap());
        self.names[i] = Slot {
            key,
            name: Some(name),
        };
        self.count += 1;
        if self.count * 2 > self.names.len() {
            self.grow();
        }
        name
    }

    /// The interned `text`, if it is.
    pub fn get(&self, text: &str) -> Option<&'a str> {
        let key = key(text.as_bytes());
        let mask = self.names.len() - 1;
        let mut i = slot(key.hash, self.names.len());
        loop {
            match self.names[i] {
                Slot {
                    key: stored,
                    name: Some(name),
                } if stored == key && name == text => return Some(name),
                Slot { name: None, .. } => return None,
                _ => i = (i + 1) & mask,
            }
        }
    }

    fn grow(&mut self) {
        let size = self.names.len() * 2;
        let old = std::mem::replace(&mut self.names, vec![Slot::default(); size]);
        let mask = self.names.len() - 1;
        for entry in old {
            if entry.name.is_some() {
                let mut i = slot(entry.key.hash, self.names.len());
                while self.names[i].name.is_some() {
                    i = (i + 1) & mask;
                }
                self.names[i] = entry;
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
        // Short names are told apart by their length and first word alone.
        let spellings = ["abc", "\0abc", "abcdefgh", "abcdefgh1", "abcdefgh2", "a\0", "a"];
        let names: Vec<_> = spellings.iter().map(|s| strings.intern(s)).collect();
        for (name, spelling) in names.iter().zip(spellings) {
            assert_eq!(*name, spelling);
            assert!(std::ptr::eq(*name, strings.intern(spelling)));
        }
    }
}
