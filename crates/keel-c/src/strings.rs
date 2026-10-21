//! Compilation-scoped byte storage and interned names. Frontend references
//! cannot outlive the arena; the returned IR owns the strings it needs.
use bumpalo::Bump;
use keel_ir::fxhash::FxHashSet as HashSet;

pub(crate) struct Strings<'a> {
    arena: &'a Bump,
    names: HashSet<&'a str>,
    literals: HashSet<&'a [u8]>,
}

impl<'a> Strings<'a> {
    pub fn new(arena: &'a Bump) -> Self {
        Self {
            arena,
            names: HashSet::default(),
            literals: HashSet::default(),
        }
    }

    pub fn intern(&mut self, text: &str) -> &'a str {
        if let Some(&name) = self.names.get(text) {
            return name;
        }
        let name = self.arena.alloc_str(text);
        self.names.insert(name);
        name
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
    }
}
