//! Arenas for the syntax tree. Tree nodes are `Copy`, so nothing in an
//! arena needs dropping: freeing one frees its chunks and nothing else.
//!
//! File-scope nodes (global initializers) go in the compilation's arena,
//! with everything else that lives as long as it. Each function body has
//! a [`BodyArena`] of its own, which its definition owns, so code
//! generation frees a function's tree in one go once it is emitted.

use bumpalo::Bump;
use std::ptr::NonNull;

/// A function body's arena. The parser hands out its nodes as `&'a`, like
/// everything else in the tree, but they live only as long as the arena:
/// only the function's own definition may refer to them. A `static`
/// local's initializer, which outlives the function, goes in the
/// compilation's arena instead.
pub struct BodyArena(NonNull<Bump>);

impl BodyArena {
    pub fn new() -> Self {
        Self(NonNull::from(Box::leak(Box::new(Bump::new()))))
    }

    /// The arena, for nodes that must not outlive `self`.
    ///
    /// # Safety
    ///
    /// Nothing allocated in it may be used after `self` is dropped.
    pub unsafe fn get<'a>(&self) -> &'a Bump {
        // SAFETY: the box is only freed when `self` is dropped.
        unsafe { self.0.as_ref() }
    }

    /// Bytes of nodes in it, about: a measure of the function's size.
    pub fn used_bytes(&self) -> usize {
        // SAFETY: nothing allocated is used.
        let bump = unsafe { self.get() };
        bump.allocated_bytes() - bump.chunk_capacity()
    }
}

impl Drop for BodyArena {
    fn drop(&mut self) {
        // SAFETY: made by `Box::leak` in `new`, and freed only here.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

impl std::fmt::Debug for BodyArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BodyArena")
    }
}
