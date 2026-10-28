//! Compilation-scoped byte storage and interned names. Frontend references
//! cannot outlive the source or arena; the returned IR owns the strings it needs.
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
/// head, which is all of a name of up to eight bytes.
#[derive(Clone, Copy, Default, PartialEq)]
struct Key {
    hash: u32,
    len: u32,
    head: u64,
}

/// Each word of `bytes` folded in by multiplication. Only the top bits of
/// the last product depend on every byte, so slot indices take their bits
/// from the top. Short names are read in at most two loads, which may
/// overlap: with the length, the head still determines every byte.
fn key(bytes: &[u8]) -> Key {
    const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;
    let word = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let half = |at: usize| u64::from(u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()));
    let n = bytes.len();
    let head = match n {
        8.. => word(0),
        4..8 => half(0) | half(n - 4) << 32,
        1..4 => u64::from(bytes[0]) | u64::from(bytes[n / 2]) << 8 | u64::from(bytes[n - 1]) << 16,
        0 => 0,
    };
    let mut h = ((n as u64).rotate_left(5) ^ head).wrapping_mul(K);
    if n > 8 {
        let mut at = 8;
        while at + 8 <= n {
            h = (h.rotate_left(5) ^ word(at)).wrapping_mul(K);
            at += 8;
        }
        // The last bytes, in a word that may overlap the one before.
        if at < n {
            h = (h.rotate_left(5) ^ word(n - 8)).wrapping_mul(K);
        }
    }
    Key {
        hash: (h >> 32) as u32,
        len: n as u32,
        head,
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

    pub fn arena(&self) -> &'a Bump {
        self.arena
    }

    pub fn intern(&mut self, text: &str) -> &'a str {
        self.intern_with(text, |arena| arena.alloc_str(text))
    }

    /// Interns a source spelling without copying it. An existing name keeps
    /// its canonical address, whether it came from the source or the arena.
    pub fn intern_source(&mut self, text: &'a str) -> &'a str {
        self.intern_with(text, |_| text)
    }

    #[inline]
    fn intern_with(&mut self, source: &str, make: impl FnOnce(&'a Bump) -> &'a str) -> &'a str {
        let text = source.as_bytes();
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
        let name = make(self.arena);
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

    /// Interns literal bytes that already live for the compilation.
    pub fn bytes_source(&mut self, bytes: &'a [u8]) -> &'a [u8] {
        if let Some(&value) = self.literals.get(bytes) {
            return value;
        }
        self.literals.insert(bytes);
        bytes
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
    fn source_literals_and_decoded_literals_share_canonical_storage() {
        let source = vec![0, 255, 1, 128];
        let decoded = vec![7, 8, 9];
        let other_source = decoded.clone();
        let arena = Bump::new();
        let mut strings = Strings::new(&arena);
        let borrowed = strings.bytes_source(&source);
        assert!(std::ptr::eq(borrowed, source.as_slice()));
        assert!(std::ptr::eq(borrowed, strings.bytes(&source.clone())));
        let owned = strings.bytes(&decoded);
        assert!(std::ptr::eq(owned, strings.bytes_source(&other_source)));
        for i in 0..10_000u32 {
            strings.bytes(&i.to_le_bytes());
        }
        assert!(std::ptr::eq(borrowed, strings.bytes_source(&source)));
        assert!(std::ptr::eq(owned, strings.bytes(&decoded)));
    }

    #[test]
    fn borrowed_and_owned_names_keep_one_address_across_growth() {
        let spellings: Vec<_> = (0..10_000)
            .map(|i| format!("common_identifier_prefix_{i:05}"))
            .chain(["".into(), "a".into(), "a\0".into(), "λ中".into()])
            .collect();
        let arena = Bump::new();
        let mut strings = Strings::new(&arena);
        let names: Vec<_> = spellings
            .iter()
            .enumerate()
            .map(|(i, spelling)| {
                if i % 2 == 0 {
                    let name = strings.intern_source(spelling);
                    assert!(std::ptr::eq(name, spelling.as_str()));
                    name
                } else {
                    strings.intern(spelling)
                }
            })
            .collect();
        for (spelling, name) in spellings.iter().zip(names) {
            assert_eq!(name, spelling);
            assert!(std::ptr::eq(name, strings.intern_source(spelling)));
            assert!(std::ptr::eq(name, strings.intern(&spelling.clone())));
            assert!(std::ptr::eq(name, strings.get(spelling).unwrap()));
        }
    }

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
        let spellings = [
            "abc",
            "\0abc",
            "abcdefgh",
            "abcdefgh1",
            "abcdefgh2",
            "a\0",
            "a",
        ];
        let names: Vec<_> = spellings.iter().map(|s| strings.intern(s)).collect();
        for (name, spelling) in names.iter().zip(spellings) {
            assert_eq!(*name, spelling);
            assert!(std::ptr::eq(*name, strings.intern(spelling)));
        }
    }
}
