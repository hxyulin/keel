//! Reusable arrays for pass-local work, so that passes running on many
//! functions do not allocate and free the same buffers each time.
//!
//! [`Scratch`] holds a vector taken from a per-thread, per-type pool and
//! gives it back, emptied, when dropped. The pipeline empties the pools when
//! it ends, like the analysis cache.

use keel_ir::{Block, Inst, Value};
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};

/// Pooled vectors kept per element type.
const KEPT: usize = 16;

/// The largest vector kept, in bytes.
const KEPT_BYTES: usize = 32 << 10;

pub(crate) trait Pooled: Sized + 'static {
    fn with_pool<R>(f: impl FnOnce(&mut Vec<Vec<Self>>) -> R) -> Option<R>;
}

macro_rules! pooled {
    ($($ty:ty => $pool:ident),* $(,)?) => {
        thread_local! {
            $(static $pool: RefCell<Vec<Vec<$ty>>> = const { RefCell::new(Vec::new()) };)*
        }
        $(impl Pooled for $ty {
            fn with_pool<R>(f: impl FnOnce(&mut Vec<Vec<Self>>) -> R) -> Option<R> {
                $pool.try_with(|pool| f(&mut pool.borrow_mut())).ok()
            }
        })*

        /// Drops every pooled vector.
        pub(crate) fn clear() {
            $(let _ = $pool.try_with(|pool| std::mem::take(&mut *pool.borrow_mut()));)*
        }
    };
}

pooled! {
    u32 => U32S,
    bool => BOOLS,
    Inst => INSTS,
    Block => BLOCKS,
    Value => VALUES,
    (u32, Inst) => USES,
    (Inst, usize) => EDGES,
}

/// A vector from the pool; see the module documentation.
pub(crate) struct Scratch<T: Pooled>(Vec<T>);

impl<T: Pooled> Scratch<T> {
    /// An empty vector, possibly with capacity from earlier use.
    pub(crate) fn new() -> Self {
        Self(
            T::with_pool(|pool| pool.pop())
                .flatten()
                .unwrap_or_default(),
        )
    }

    /// `len` copies of `value`.
    pub(crate) fn filled(len: usize, value: T) -> Self
    where
        T: Clone,
    {
        let mut v = Self::new();
        v.0.resize(len, value);
        v
    }
}

impl<T: Pooled> Drop for Scratch<T> {
    fn drop(&mut self) {
        // Large buffers go back to the allocator, which can lend their
        // memory elsewhere; holding them would raise peak memory.
        let bytes = self.0.capacity() * std::mem::size_of::<T>();
        if bytes == 0 || bytes > KEPT_BYTES {
            return;
        }
        let mut v = std::mem::take(&mut self.0);
        v.clear();
        T::with_pool(|pool| {
            if pool.len() < KEPT {
                pool.push(v);
            }
        });
    }
}

impl<T: Pooled> Deref for Scratch<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Vec<T> {
        &self.0
    }
}

impl<T: Pooled> DerefMut for Scratch<T> {
    fn deref_mut(&mut self) -> &mut Vec<T> {
        &mut self.0
    }
}

impl<'a, T: Pooled> IntoIterator for &'a Scratch<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
