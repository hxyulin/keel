//! Ordered archive selection for the native ELF linker.
use crate::Input;

/// An input or archive-selection control in command-line order.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum Request<'a> {
    /// A concrete object, regular archive, or shared library.
    File { name: &'a str, bytes: &'a [u8] },
    /// Include every member of subsequent archives.
    WholeArchive,
    /// Restore demand-driven selection for subsequent archives.
    NoWholeArchive,
    /// Begin a group whose archives are searched to a fixed point.
    StartGroup,
    /// End the most recent group.
    EndGroup,
}

/// Selected objects and shared libraries, borrowing the original input storage.
///
/// Pass [`Self::inputs`] to [`crate::elf::link`]. Archives have already been
/// expanded; shared libraries retain their first encounter order.
#[derive(Debug)]
pub struct Selection<'a> {
    pub(crate) inputs: Vec<(String, &'a [u8])>,
}

impl Selection<'_> {
    /// Borrow the selected concrete inputs for a native ELF link.
    pub fn inputs(&self) -> Vec<Input<'_>> {
        self.inputs
            .iter()
            .map(|(name, bytes)| Input { name, bytes })
            .collect()
    }
}
