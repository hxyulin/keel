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
            fn new(index: usize) -> Self {
                Self(u32::try_from(index).expect("entity index overflow"))
            }

            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(self, f)
            }
        }
    };
}

/// Append-only storage addressed by `K`. Entities are never removed;
/// dead ones are simply unreachable and dropped when the function is rebuilt.
pub struct Arena<K, V> {
    items: Vec<V>,
    _key: PhantomData<K>,
}

impl<K: EntityRef, V> Arena<K, V> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            _key: PhantomData,
        }
    }

    pub fn push(&mut self, value: V) -> K {
        let key = K::new(self.items.len());
        self.items.push(value);
        key
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = K> + use<K, V> {
        (0..self.items.len()).map(K::new)
    }

    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.items.iter().enumerate().map(|(i, v)| (K::new(i), v))
    }
}

impl<K: EntityRef, V> Default for Arena<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: EntityRef, V> Index<K> for Arena<K, V> {
    type Output = V;

    fn index(&self, key: K) -> &V {
        &self.items[key.index()]
    }
}

impl<K: EntityRef, V> IndexMut<K> for Arena<K, V> {
    fn index_mut(&mut self, key: K) -> &mut V {
        &mut self.items[key.index()]
    }
}

impl<K: EntityRef + fmt::Debug, V: fmt::Debug> fmt::Debug for Arena<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
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
}
