use super::*;
use std::ptr;

fn text(value: &str) -> KeelString {
    KeelString {
        data: value.as_ptr(),
        length: value.len(),
    }
}

#[test]
fn invalid_link_inputs_report_diagnostics_and_preserve_destination() {
    let path = std::env::temp_dir().join(format!("keel-ffi-link-invalid-{}", std::process::id()));
    std::fs::write(&path, b"keep this output").unwrap();
    let path_text = path.to_str().unwrap();
    let mut options = KeelLinkOptions {
        sdk_root: text("/"),
        output_path: text(path_text),
        flags: 0,
    };
    let bytes = b"not an object";
    let mut object = KeelLinkInput {
        name: text("bad.o"),
        data: bytes.as_ptr(),
        length: bytes.len(),
    };
    for case in 0..6 {
        options.flags = if case == 0 { 2 } else { 0 };
        object.name = if case == 1 {
            KeelString {
                data: b"\xff".as_ptr(),
                length: 1,
            }
        } else {
            text("bad.o")
        };
        let tbd = text("/nonexistent-keel-ffi-sdk/libSystem.tbd");
        let mut warnings = ptr::null_mut();
        let mut error = ptr::null_mut();
        // SAFETY: All spans are live; null pointers intentionally test validation.
        let status = unsafe {
            keel_link_signed(
                if case == 2 { ptr::null() } else { &object },
                1,
                if case == 5 { &tbd } else { ptr::null() },
                usize::from(case == 5),
                if case == 3 { ptr::null() } else { &options },
                &mut warnings,
                &mut error,
            )
        };
        assert_eq!(
            status,
            if case < 4 { INVALID } else { COMPILE },
            "case {case}"
        );
        assert!(warnings.is_null());
        assert!(!error.is_null());
        // SAFETY: The failed call returned an owned diagnostic buffer.
        unsafe {
            assert!(super::super::keel_buffer_size(error) > 0);
            super::super::keel_buffer_destroy(error);
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"keep this output");
    }
    assert_eq!(
        // SAFETY: Null outputs must be rejected before reading inputs.
        unsafe {
            keel_link_signed(
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        },
        INVALID
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn signed_link_returns_buffers_and_dead_strips_unused_code() {
    let bytes = keel_asm::assemble_object(".subsections_via_symbols\n.text\n.globl _main\n_main:\nmov w0,#0\nret\n.globl _unused\n_unused:\n.space 65536\nret\n.section __DWARF,__debug_str,regular,debug\n.asciz \"debug\"\n").unwrap();
    let input = KeelLinkInput {
        name: text("memory.o"),
        data: bytes.as_ptr(),
        length: bytes.len(),
    };
    let path = std::env::temp_dir().join(format!("keel-ffi-link-valid-{}", std::process::id()));
    let path_text = path.to_str().unwrap();
    let tbd_path = path.with_extension("tbd");
    std::fs::write(&tbd_path, "--- !tapi-tbd\ntbd-version: 4\ntargets: [ arm64-macos ]\ninstall-name: '/usr/lib/libSystem.B.dylib'\nexports: []\n").unwrap();
    let tbd = text(tbd_path.to_str().unwrap());
    let mut sizes = Vec::new();
    for flags in [0, 1] {
        let options = KeelLinkOptions {
            sdk_root: text("/"),
            output_path: text(path_text),
            flags,
        };
        let mut warnings = ptr::null_mut();
        let mut error = ptr::null_mut();
        // SAFETY: All input spans and output slots are valid for the call.
        let status =
            unsafe { keel_link_signed(&input, 1, &tbd, 1, &options, &mut warnings, &mut error) };
        assert_eq!(status, OK);
        assert!(error.is_null());
        // SAFETY: The successful call returned an owned immutable warning buffer.
        unsafe {
            assert_eq!(
                String::from_utf8_lossy(&(*warnings).0)
                    .matches("DWARF input omitted")
                    .count(),
                1
            );
            super::super::keel_buffer_destroy(warnings);
        }
        let image = std::fs::read(&path).unwrap();
        sizes.push(image.len());
        assert_eq!(&image[..4], &0xfeedfacfu32.to_le_bytes());
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert!(
            std::process::Command::new(&path)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::remove_file(tbd_path).unwrap();
    assert!(
        sizes[1] < sizes[0],
        "dead stripping should shrink output: {sizes:?}"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn elf_link_rejects_invalid_options_and_returns_owned_errors() {
    let mut options = KeelElfLinkOptions {
        target: 3,
        flags: 0,
        entry: text(""),
        interpreter: text(""),
        runpaths: ptr::null(),
        runpath_count: 0,
        script: text(""),
        max_page_size: 0,
    };
    for case in 0..7 {
        options.target = if case == 0 { 1 } else { 3 };
        options.flags = if case == 1 { 8 } else { 0 };
        options.runpath_count = usize::from(case == 2);
        options.entry = if case == 3 {
            text("bad\0entry")
        } else {
            text("")
        };
        let mut image = ptr::null_mut();
        let mut warnings = ptr::null_mut();
        let mut error = ptr::null_mut();
        // SAFETY: All spans are readable; null inputs intentionally test validation.
        let status = unsafe {
            keel_link_elf(
                ptr::null(),
                usize::from(case == 4),
                if case == 5 { ptr::null() } else { &options },
                &mut image,
                &mut warnings,
                &mut error,
            )
        };
        assert_eq!(
            status,
            if case == 6 { COMPILE } else { INVALID },
            "case {case}"
        );
        assert!(image.is_null());
        assert!(warnings.is_null());
        assert!(!error.is_null());
        // SAFETY: The call returned an owned diagnostic.
        unsafe {
            assert!(!(*error).0.is_empty());
            super::super::keel_buffer_destroy(error);
        }
    }
    assert_eq!(
        // SAFETY: Missing output slots are rejected before any input is read.
        unsafe {
            keel_link_elf(
                ptr::null(),
                0,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        },
        INVALID
    );
}
