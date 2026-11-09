//! Module-independent macOS and Linux executable linking.
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

/// A named object, archive or shared library, borrowed for one link call.
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

/// Host-independent ELF executable policies. Zero-length optional strings use defaults.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeelElfLinkOptions {
    pub target: u32,
    /// Bit 0 collects sections, bit 1 relaxes GOT loads, bit 2 merges constants.
    pub flags: u32,
    pub entry: KeelString,
    pub interpreter: KeelString,
    pub runpaths: *const KeelString,
    pub runpath_count: usize,
    pub script: KeelString,
    /// Zero selects the target default.
    pub max_page_size: u64,
}

// SAFETY: The exported entry point requires readable nested spans.
unsafe fn link_elf(
    objects: *const KeelLinkInput,
    object_count: usize,
    options: *const KeelElfLinkOptions,
) -> Result<keel_link::elf::Executable> {
    // SAFETY: Nonnull options point to an initialized value for this call.
    let options = unsafe { options.as_ref() }.ok_or_else(|| invalid("null ELF link options"))?;
    if options.flags & !7 != 0 {
        return Err(invalid("unknown ELF link flags"));
    }
    let machine = match super::pipeline::target(options.target)? {
        keel_ir::Target::Aarch64LinuxGnu => keel_link::elf::Machine::Aarch64,
        keel_ir::Target::X86_64LinuxGnu => keel_link::elf::Machine::X86_64,
        _ => return Err(invalid("ELF linking requires a Linux target")),
    };
    // SAFETY: Both arrays and all nested spans remain readable during the call.
    let objects = unsafe { input(objects, object_count)? };
    let mut inputs = Vec::with_capacity(objects.len());
    for object in objects {
        inputs.push(keel_link::Input {
            // SAFETY: The input contract covers the nested name and byte spans.
            name: unsafe { object.name.text()? },
            // SAFETY: The caller retains the object's bytes during linking.
            bytes: unsafe { input(object.data, object.length)? },
        });
    }
    let mut policies = keel_link::elf::Options {
        machine,
        gc_sections: options.flags & 1 != 0,
        relax_got: options.flags & 2 != 0,
        merge_constants: options.flags & 4 != 0,
        interpreter: match machine {
            keel_link::elf::Machine::Aarch64 => "/lib/ld-linux-aarch64.so.1",
            keel_link::elf::Machine::X86_64 => "/lib64/ld-linux-x86-64.so.2",
        }
        .into(),
        max_page_size: (options.max_page_size != 0).then_some(options.max_page_size),
        ..Default::default()
    };
    // SAFETY: Optional nonempty strings and the runpath array follow the input contract.
    unsafe {
        if options.entry.length != 0 {
            policies.entry_override = Some(options.entry.text()?.into());
        }
        if options.interpreter.length != 0 {
            policies.interpreter = options.interpreter.text()?.into();
        }
        if options.script.length != 0 {
            policies.script = Some(options.script.text()?.into());
        }
        for path in input(options.runpaths, options.runpath_count)? {
            policies.runpath.push(path.text()?.into());
        }
    }
    keel_link::elf::link(&inputs, policies).map_err(|error| (COMPILE, error.to_string()))
}

/// Links ELF objects, archives and shared libraries for an explicit Linux target.
///
/// Returns owned image and warning buffers on success, or an owned error buffer
/// on failure. All three output slots are required. No output file is written.
///
/// # Safety
/// Inputs and their nested spans must remain readable for this call. Outputs
/// must be distinct writable slots, without live buffers, and not overlap inputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn keel_link_elf(
    objects: *const KeelLinkInput,
    object_count: usize,
    options: *const KeelElfLinkOptions,
    image: *mut *mut KeelBuffer,
    warnings: *mut *mut KeelBuffer,
    error: *mut *mut KeelBuffer,
) -> KeelStatus {
    if image.is_null() || warnings.is_null() || error.is_null() {
        return INVALID;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: The caller supplies readable input spans, including nested spans.
        unsafe { link_elf(objects, object_count, options) }
    }));
    let (status, executable, messages, diagnostic) = match result {
        Ok(Ok(output)) => (
            OK,
            Some(output.bytes),
            Some(output.warnings.join("\n").into_bytes()),
            None,
        ),
        Ok(Err((status, message))) => (status, None, None, Some(message.into_bytes())),
        Err(payload) => {
            std::mem::forget(payload);
            (
                PANIC,
                None,
                None,
                Some(b"internal panic during ELF linking".to_vec()),
            )
        }
    };
    let buffer = |bytes: Option<Vec<u8>>| {
        bytes.map_or(std::ptr::null_mut(), |bytes| {
            Box::into_raw(Box::new(KeelBuffer(bytes)))
        })
    };
    // SAFETY: All output slots are writable, distinct and free of live buffers.
    unsafe {
        image.write(buffer(executable));
        warnings.write(buffer(messages));
        error.write(buffer(diagnostic));
    }
    status
}
