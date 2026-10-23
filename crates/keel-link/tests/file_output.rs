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
    check_signature(&signed);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        assert!(
            std::process::Command::new("/usr/bin/codesign")
                .args(["--verify", "--strict"])
                .arg(&new)
                .status()
                .unwrap()
                .success()
        );
        assert!(std::process::Command::new(&new).status().unwrap().success());
    }
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
    let invalid = keel_link::Executable {
        bytes: vec![0; 16384],
        debug_objects: vec![],
        warnings: vec![],
    };
    assert!(keel_link::write_signed(&invalid, &new).is_err());
    assert_eq!(signed, std::fs::read(&new).unwrap());

    assert!(!std::fs::read_dir(&root).unwrap().any(|p| {
        p.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".keel-link-")
    }));
    std::fs::remove_dir_all(root).unwrap();
}

// Independently decode the embedded structure and hash the final file bytes.
fn check_signature(bytes: &[u8]) {
    use sha2::{Digest, Sha256};
    let le32 = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let le64 = |at| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize;
    let mut at = 32;
    let mut signature = None;
    let mut text = None;
    let mut linkedit = None;
    for _ in 0..le32(16) {
        if le32(at) == 0x1d {
            assert_eq!(le32(at + 4), 16);
            assert!(signature.replace((le32(at + 8), le32(at + 12))).is_none());
        }
        if le32(at) == 0x19 {
            if &bytes[at + 8..at + 14] == b"__TEXT" {
                text = Some((le64(at + 40), le64(at + 48)));
            }
            if &bytes[at + 8..at + 18] == b"__LINKEDIT" {
                linkedit = Some((le64(at + 40), le64(at + 48), le64(at + 32)));
            }
        }
        at += le32(at + 4);
    }
    assert_eq!(at, 32 + le32(20));
    let (offset, size) = signature.unwrap();
    assert_eq!(offset % 16, 0);
    assert_eq!(offset + size, bytes.len());
    let (file, len, vm) = linkedit.unwrap();
    assert_eq!(file + len, bytes.len());
    assert_eq!(vm, len.div_ceil(16384) * 16384);
    let blob = &bytes[offset..];
    let be32 = |at| u32::from_be_bytes(blob[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!(be32(0), 0xfade0cc0);
    assert_eq!(be32(4), size);
    assert_eq!(be32(8), 1);
    assert_eq!(be32(12), 0);
    let cd = &blob[be32(16)..];
    let be32 = |at| u32::from_be_bytes(cd[at..at + 4].try_into().unwrap()) as usize;
    let be64 = |at| u64::from_be_bytes(cd[at..at + 8].try_into().unwrap()) as usize;
    assert_eq!(be32(0), 0xfade0c02);
    assert_eq!(be32(4), cd.len());
    assert_eq!(be32(8), 0x20400);
    assert_eq!(be32(12), 0x20002);
    assert_eq!(be32(24), 0);
    assert_eq!(be32(28), offset.div_ceil(4096));
    assert_eq!(be32(32), offset);
    assert_eq!(&cd[36..40], &[32, 2, 0, 12]);
    assert_eq!(&cd[be32(20)..be32(20) + 12], b"keel.native\0");
    assert_eq!((be64(64), be64(72)), text.unwrap());
    assert_eq!(be64(80), 1);
    assert_eq!(be32(16) + be32(28) * 32, cd.len());
    for (page, hash) in bytes[..offset]
        .chunks(4096)
        .zip(cd[be32(16)..].chunks_exact(32))
    {
        assert_eq!(Sha256::digest(page).as_slice(), hash);
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn signed_replacements_launch_and_tampering_is_rejected() {
    let root = std::env::temp_dir().join(format!("keel-sign-replace-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("program");
    for iteration in 0..20 {
        let code = iteration % 2;
        let object = keel_asm::assemble_object(&format!(
            ".text\n.globl _main\n_main:\nmov w0,#{code}\nret\n.section __DATA,__discarded\n.quad 42\n"
        ))
        .unwrap();
        keel_link::link_signed(
            &[Input {
                name: "main.o",
                bytes: &object,
            }],
            Options {
                dead_strip: iteration % 2 != 0,
                ..options(false)
            },
            &path,
        )
        .unwrap();
        assert_eq!(
            std::process::Command::new(&path).status().unwrap().code(),
            Some(code)
        );
        assert!(
            std::process::Command::new("/usr/bin/codesign")
                .args(["--verify", "--strict"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
    }
    // Copy to a fresh inode before tampering, so rejection cannot be supplied
    // by a prior execution's kernel signature cache.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[16384] ^= 1;
    let tampered = root.join("tampered");
    std::fs::write(&tampered, bytes).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&tampered, std::fs::Permissions::from_mode(0o755)).unwrap();
    let rejected = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&tampered)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("invalid signature"));
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        std::process::Command::new(&tampered)
            .status()
            .unwrap()
            .signal(),
        Some(9)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn signatures_cover_multiple_read_batches_and_partial_final_pages() {
    let object = keel_asm::assemble_object(
        ".text\n.globl _main\n_main:\nmov w0,#0\nret\n.section __TEXT,__const\n.space 140000\n",
    )
    .unwrap();
    let inputs = [Input {
        name: "large.o",
        bytes: &object,
    }];
    let unsigned = keel_link::link(&inputs, options(false)).unwrap();
    assert!(unsigned.bytes.len() > 2 * 65536);
    assert_ne!(unsigned.bytes.len() % 4096, 0);
    let path = std::env::temp_dir().join(format!("keel-sign-large-{}", std::process::id()));
    keel_link::link_signed(&inputs, options(false), &path).unwrap();
    check_signature(&std::fs::read(&path).unwrap());
    std::fs::remove_file(path).unwrap();
}
