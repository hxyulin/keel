//! Module-independent signed arm64 macOS executable linking.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use super::{COMPILE, INVALID, KeelBuffer, KeelStatus, OK, PANIC, Result, input, invalid};

/// Borrowed UTF-8 bytes, without a terminating NUL.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelString {
    pub data: *const u8,
    pub length: usize,
}

impl KeelString {
    // SAFETY: Callers of the exported API provide valid borrowed spans.
    unsafe fn text<'a>(self) -> Result<&'a str> {
        // SAFETY: The caller provides a readable span for the call.
        let bytes = unsafe { input(self.data, self.length)? };
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("link string is not UTF-8"))?;
        if text.is_empty() || text.contains('\0') {
            return Err(invalid("link string must be nonempty and contain no NUL"));
        }
        Ok(text)
    }
}

/// A named Mach-O object or archive, borrowed for one link call.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelLinkInput {
    pub name: KeelString,
    pub data: *const u8,
    pub length: usize,
}

/// Signed executable output path, SDK root, and linker flags.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelLinkOptions {
    pub sdk_root: KeelString,
    pub output_path: KeelString,
    /// Bit 0 enables dead stripping; all other bits must be zero.
    pub flags: u32,
}

// SAFETY: The exported function's contract covers every nested input span.
unsafe fn link(
    objects: *const KeelLinkInput,
    object_count: usize,
    tbd_paths: *const KeelString,
    tbd_count: usize,
    options: *const KeelLinkOptions,
) -> Result<Vec<String>> {
    // SAFETY: Nonnull options must point to an initialized readable value.
    let options = unsafe { options.as_ref() }.ok_or_else(|| invalid("null link options"))?;
    if options.flags & !1 != 0 {
        return Err(invalid("unknown link flags"));
    }
    // SAFETY: Each string and array is borrowed and readable for this call.
    let (root, path, objects, paths) = unsafe {
        (
            options.sdk_root.text()?,
            options.output_path.text()?,
            input(objects, object_count)?,
            input(tbd_paths, tbd_count)?,
        )
    };
    let inputs = objects
        .iter()
        .map(|object| {
            Ok(keel_link::Input {
                // SAFETY: The input array contract includes its nested spans.
                name: unsafe { object.name.text()? },
                // SAFETY: The caller keeps the object bytes readable during linking.
                bytes: unsafe { input(object.data, object.length)? },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let dylibs = paths
        .iter()
        .map(|path| {
            // SAFETY: The caller keeps each path span readable during linking.
            let path = unsafe { path.text()? };
            keel_link::Dylib::from_tbd(Path::new(path), Path::new(root))
                .map_err(|error| (COMPILE, error.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;
    keel_link::link_signed(
        &inputs,
        keel_link::Options {
            dead_strip: options.flags & 1 != 0,
            dylibs,
            ..keel_link::Options::default()
        },
        Path::new(path),
    )
    .map_err(|error| (COMPILE, error.to_string()))
}

/// Links borrowed objects/archives and explicit TBD paths to a signed macOS arm64 executable.
///
/// On success, writes a warning buffer and a null error pointer. On failure,
/// writes a null warning pointer and an error buffer. Null output pointers return
/// INVALID without writing either output. Buffers use `keel_buffer_destroy`.
/// The destination is replaced atomically only after a successful link and sign.
///
/// # Safety
/// All nonnull pointers must satisfy the header's readable input and writable,
/// nonoverlapping output contracts, including every nested span. Both output
/// slots must be available for new ownership; they must not hold live buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_link_signed(
    objects: *const KeelLinkInput,
    object_count: usize,
    tbd_paths: *const KeelString,
    tbd_count: usize,
    options: *const KeelLinkOptions,
    warnings: *mut *mut KeelBuffer,
    error: *mut *mut KeelBuffer,
) -> KeelStatus {
    if warnings.is_null() || error.is_null() {
        return INVALID;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: The caller supplies all nested input spans for this call.
        unsafe { link(objects, object_count, tbd_paths, tbd_count, options) }
    }));
    let (status, warning_bytes, error_bytes) = match result {
        Ok(Ok(messages)) => (OK, Some(messages.join("\n").into_bytes()), None),
        Ok(Err((status, message))) => (status, None, Some(message.into_bytes())),
        Err(payload) => {
            // Match the module boundary: a hostile panic payload must not unwind
            // again when dropped outside the barrier.
            std::mem::forget(payload);
            (PANIC, None, Some(b"internal panic during linking".to_vec()))
        }
    };
    let buffer = |bytes: Option<Vec<u8>>| {
        bytes.map_or(std::ptr::null_mut(), |bytes| {
            Box::into_raw(Box::new(KeelBuffer(bytes)))
        })
    };
    // SAFETY: Both outputs are writable, distinct, and do not overlap inputs.
    unsafe {
        warnings.write(buffer(warning_bytes));
        error.write(buffer(error_bytes));
    }
    status
}

#[cfg(test)]
mod tests;
