use keel_link::{Dylib, Input, Options};
use std::{
    collections::BTreeSet,
    io::{self, Cursor, Seek, SeekFrom, Write},
};

fn options(debug: bool) -> Options {
    Options {
        dylibs: vec![Dylib {
            install_name: "/usr/lib/libSystem.B.dylib".into(),
            exports: BTreeSet::new(),
            weak_exports: BTreeSet::new(),
        }],
        debug_directory: debug.then(|| std::env::temp_dir().join("keel-file-output-debug")),
        ..Options::default()
    }
}
fn object() -> Vec<u8> {
    keel_asm::assemble_object(".text\n.globl _main\n_main:\n.cfi_startproc\nmov w0,#0\nret\n.cfi_endproc\n.data\n.p2align 6\n.quad _main\n.section __TEXT,__const\n.p2align 8\n.quad 42\n.zerofill __DATA,__bss,_space,65536,4\n.section __DWARF,__debug_str,regular,debug\n.asciz \"exact snapshot\"\n").unwrap()
}

// Exercises short writes and errors from a caller-owned output implementation.
struct Sink {
    bytes: Cursor<Vec<u8>>,
    fail_after: usize,
    written: usize,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.written >= self.fail_after {
            return Err(io::Error::other("injected write failure"));
        }
        let n = bytes.len().min(7).min(self.fail_after - self.written);
        let n = self.bytes.write(&bytes[..n])?;
        self.written += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for Sink {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.bytes.seek(pos)
    }
}

#[test]
fn file_output_matches_byte_api_with_padding_bss_unwind_and_debug() {
    let bytes = object();
    let inputs = [Input {
        name: "memory.o",
        bytes: &bytes,
    }];
    for debug in [false, true] {
        let expected = keel_link::link(&inputs, options(debug)).unwrap();
        let mut sink = Sink {
            bytes: Cursor::new(Vec::new()),
            fail_after: usize::MAX,
            written: 0,
        };
        let artifacts = keel_link::link_to(&inputs, options(debug), &mut sink).unwrap();
        assert_eq!(sink.bytes.position(), expected.bytes.len() as u64);
        assert_eq!(sink.bytes.into_inner(), expected.bytes);
        assert_eq!(artifacts.warnings, expected.warnings);
        assert_eq!(artifacts.debug_objects.len(), expected.debug_objects.len());
        if debug {
            assert_eq!(artifacts.debug_objects[0].bytes, bytes);
            assert_eq!(
                artifacts.debug_objects[0].path,
                expected.debug_objects[0].path
            );
        }
    }
}

#[test]
fn file_output_rejects_nonempty_sinks_and_propagates_write_errors() {
    let bytes = object();
    let inputs = [Input {
        name: "memory.o",
        bytes: &bytes,
    }];
    let mut occupied = Cursor::new(b"keep existing bytes".to_vec());
    assert!(
        keel_link::link_to(&inputs, options(false), &mut occupied)
            .err()
            .unwrap()
            .contains("must be empty")
    );
    assert_eq!(occupied.into_inner(), b"keep existing bytes");
    for fail_after in [0, 64, 16384, 32768] {
        let mut sink = Sink {
            bytes: Cursor::new(Vec::new()),
            fail_after,
            written: 0,
        };
        assert!(
            keel_link::link_to(&inputs, options(false), &mut sink)
                .err()
                .unwrap()
                .contains("injected write failure")
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn direct_signed_output_matches_buffered_and_failure_preserves_destination() {
    let root = std::env::temp_dir().join(format!("keel-direct-signed-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let bytes = object();
    let inputs = [Input {
        name: "memory.o",
        bytes: &bytes,
    }];
    let expected = keel_link::link(&inputs, options(false)).unwrap();
    let old = root.join("buffered");
    let new = root.join("direct");
    keel_link::write_signed(&expected, &old).unwrap();
    let warnings = keel_link::link_signed(&inputs, options(false), &new).unwrap();
    assert_eq!(warnings, expected.warnings);
    let signed = std::fs::read(&new).unwrap();
    assert_eq!(signed, std::fs::read(&old).unwrap());
    assert!(
        std::process::Command::new("codesign")
            .args(["--verify", "--strict"])
            .arg(&new)
            .status()
            .unwrap()
            .success()
    );
    assert!(std::process::Command::new(&new).status().unwrap().success());
    assert!(
        keel_link::link_signed(
            &[Input {
                name: "bad.o",
                bytes: b"invalid"
            }],
            options(false),
            &new
        )
        .is_err()
    );
    assert_eq!(signed, std::fs::read(&new).unwrap());
    assert!(!std::fs::read_dir(&root).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".keel-link-")
    }));
    std::fs::remove_dir_all(root).unwrap();
}
