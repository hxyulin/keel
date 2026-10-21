//! Dense `u32` handles into per-function arenas.
//!
//! IR objects never point at each other. They hold small typed indices
//! (`Value`, `Inst`, `Block`) into an [`Arena`] owned by the function, so the
//! IR is compact, cheap to clone and has no ownership cycles.

use std::fmt;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// A typed index into an [`Arena`].
pub trait EntityRef: Copy + Eq {
    fn new(index: usize) -> Self;
    fn index(self) -> usize;
}

/// Declares a `u32` handle type that prints as `<prefix><index>`, e.g. `v3`.
#[macro_export]
macro_rules! entity {
    ($(#[$attr:meta])* $name:ident, $prefix:literal) => {
        $(#[$attr])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $crate::entity::EntityRef for $name {
            #[inline]
            fn new(index: usize) -> Self {
                Self(u32::try_from(index).expect("entity index overflow"))
            }

            #[inline]
            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                $crate::entity::write_handle(f, $prefix, self.0)
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(self, f)
            }
        }
    };
}

/// Writes `<prefix><index>` in one piece. Printing IR and assembly comments
/// formats many handles; this skips the general formatting machinery.
#[doc(hidden)]
pub fn write_handle(f: &mut fmt::Formatter<'_>, prefix: &str, index: u32) -> fmt::Result {
    let mut buf = [0u8; 24];
    let mut digits = [0u8; 10];
    if prefix.len() > buf.len() - digits.len() {
        return write!(f, "{prefix}{index}");
    }
    buf[..prefix.len()].copy_from_slice(prefix.as_bytes());
    let mut start = digits.len();
    let mut n = index;
    loop {
        start -= 1;
        digits[start] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let end = prefix.len() + digits.len() - start;
    buf[prefix.len()..end].copy_from_slice(&digits[start..]);
    f.write_str(std::str::from_utf8(&buf[..end]).expect("ASCII"))
}

/// Append-only storage addressed by `K`. Entities are never removed;
/// dead ones are simply unreachable and dropped when the function is rebuilt.
pub struct Arena<K, V> {
    items: Vec<V>,
    _key: PhantomData<K>,
}

impl<K: EntityRef, V> Arena<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            _key: PhantomData,
        }
    }

    #[inline]
    pub fn push(&mut self, value: V) -> K {
        let key = K::new(self.items.len());
        self.items.push(value);
        key
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    pub fn keys(&self) -> impl Iterator<Item = K> + use<K, V> {
        (0..self.items.len()).map(K::new)
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.items.iter().enumerate().map(|(i, v)| (K::new(i), v))
    }
}

impl<K, V: Clone> Clone for Arena<K, V> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            _key: PhantomData,
        }
    }
}

impl<K: EntityRef, V> Default for Arena<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: EntityRef, V> Index<K> for Arena<K, V> {
    type Output = V;

    #[inline]
    fn index(&self, key: K) -> &V {
        &self.items[key.index()]
    }
}

impl<K: EntityRef, V> IndexMut<K> for Arena<K, V> {
    #[inline]
    fn index_mut(&mut self, key: K) -> &mut V {
        &mut self.items[key.index()]
    }
}

impl<K: EntityRef + fmt::Debug, V: fmt::Debug> fmt::Debug for Arena<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// Finds arena entries by key without storing the keys twice: a slot holds
/// an entry's index and 32 bits of its key's hash, and a lookup compares the
/// key itself through the arena. Arenas below [`KeyIndex::FROM`] entries are
/// scanned instead, so most functions never build one, and an unbuilt index
/// is a single null pointer.
///
/// Entries are only ever added, and an equal key added later probes past an
/// earlier one, so a lookup finds the first entry with a key, like a scan.
#[derive(Clone, Default)]
pub(crate) struct KeyIndex(Option<Box<KeyTable>>);

#[derive(Clone)]
struct KeyTable {
    /// Open addressing with linear probing, a power of two long: `(hash,
    /// index + 1)`, with index 0 marking a free slot.
    slots: Vec<(u32, u32)>,
    /// Each indexed entry's hash, by index.
    hashes: Vec<u32>,
    /// Entries whose keys may have changed since they were indexed.
    touched: Vec<u32>,
}

impl KeyIndex {
    /// Arena size from which lookups go through an index.
    pub(crate) const FROM: usize = 16;

    /// The top half of a 64-bit hash, the best mixed by Fx's multiply.
    #[inline]
    pub(crate) fn hash_of<T: std::hash::Hash + ?Sized>(key: &T) -> u32 {
        use std::hash::Hasher;
        let mut hasher = crate::fxhash::FxHasher::default();
        key.hash(&mut hasher);
        (hasher.finish() >> 32) as u32
    }

    /// Number of entries indexed.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.as_ref().map_or(0, |t| t.hashes.len())
    }

    /// Indexes the hashes of `len` entries' keys, in order, replacing
    /// everything.
    pub(crate) fn rebuild(&mut self, len: usize, hashes: impl Iterator<Item = u32>) {
        let mut table = KeyTable {
            slots: vec![(0, 0); (len * 2).next_power_of_two().max(64)],
            hashes: Vec::with_capacity(len),
            touched: Vec::new(),
        };
        for hash in hashes {
            table.push(hash);
        }
        self.0 = Some(Box::new(table));
    }

    /// The first entry with hash `hash` that `is` accepts. The index must be
    /// built.
    #[inline]
    pub(crate) fn find(&self, hash: u32, mut is: impl FnMut(usize) -> bool) -> Option<usize> {
        let table = self.0.as_ref().expect("index is built");
        let mask = table.slots.len() - 1;
        let mut i = table.home(hash);
        loop {
            let (h, entry) = table.slots[i];
            if entry == 0 {
                return None;
            }
            if h == hash && is(entry as usize - 1) {
                return Some(entry as usize - 1);
            }
            i = (i + 1) & mask;
        }
    }

    /// Indexes the entry just added, whose key hashes to `hash`, if the
    /// index is built.
    #[inline]
    pub(crate) fn push(&mut self, hash: u32) {
        if let Some(table) = &mut self.0 {
            table.push(hash);
        }
    }

    /// Notes that entry `index`'s key may change, if the index is built.
    /// After `limit` such notes, drops the index instead: rebuilding it
    /// costs no more than checking them all.
    #[inline]
    pub(crate) fn touch(&mut self, index: usize, limit: usize) {
        if let Some(table) = &mut self.0 {
            if table.touched.len() >= limit {
                self.0 = None;
            } else {
                table.touched.push(index as u32);
            }
        }
    }

    /// Drops the index if a touched entry's key no longer hashes as it was
    /// indexed; `hash_at` hashes an entry's current key.
    #[inline]
    pub(crate) fn check_touched(&mut self, hash_at: impl Fn(usize) -> u32) {
        let Some(table) = &mut self.0 else {
            return;
        };
        if table.touched.is_empty() {
            return;
        }
        if table
            .touched
            .iter()
            .any(|&i| hash_at(i as usize) != table.hashes[i as usize])
        {
            self.0 = None;
        } else {
            table.touched.clear();
        }
    }
}

impl KeyTable {
    #[inline]
    fn home(&self, hash: u32) -> usize {
        (hash >> (32 - self.slots.len().trailing_zeros())) as usize
    }

    fn push(&mut self, hash: u32) {
        if (self.hashes.len() + 1) * 2 > self.slots.len() {
            let len = self.slots.len() * 2;
            self.slots.clear();
            self.slots.resize(len, (0, 0));
            for index in 0..self.hashes.len() {
                self.place(self.hashes[index], index);
            }
        }
        self.place(hash, self.hashes.len());
        self.hashes.push(hash);
    }

    fn place(&mut self, hash: u32, index: usize) {
        let mask = self.slots.len() - 1;
        let mut i = self.home(hash);
        while self.slots[i].1 != 0 {
            i = (i + 1) & mask;
        }
        let entry = u32::try_from(index + 1).expect("entity index overflow");
        self.slots[i] = (hash, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    entity!(Thing, "t");

    #[test]
    fn push_and_index() {
        let mut arena: Arena<Thing, &str> = Arena::new();
        let a = arena.push("a");
        let b = arena.push("b");
        assert_eq!((a.to_string(), b.to_string()), ("t0".into(), "t1".into()));
        arena[b] = "c";
        assert_eq!(arena[a], "a");
        assert_eq!(arena.keys().collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(arena[b], "c");
    }

    #[test]
    fn handles_print_their_index() {
        let max = Thing::new(u32::MAX as usize);
        assert_eq!(format!("{max} {:?}", Thing::new(10)), "t4294967295 t10");
        entity!(Long, "fifteen_letters");
        assert_eq!(Long::new(7).to_string(), "fifteen_letters7");
    }
}
