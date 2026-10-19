//! Mapped text or binary input, with reads for unmappable inputs.
//! Mappings live until the caller drops Source. Returned compiler and assembler
//! data owns its storage independently; link inputs stay mapped through linking.
use memmap2::{Mmap, MmapOptions};
use std::{fs::File, io, io::Read, path::Path};

pub(super) enum Source {
    Read(Vec<u8>),
    Mapped(Mmap),
}

impl Source {
    /// The input must not be changed or truncated by another process until
    /// the source is dropped.
    pub(super) fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if metadata.is_file() && metadata.len() != 0 {
            // SAFETY: CLI inputs remain stable while parsing. No reference
            // escapes the Source lifetime. Text consumers drop Source before
            // writing output; linking installs output by atomic replacement. Copy-on-write does not protect against external truncation;
            // the documented CLI input-stability contract still applies.
            if let Ok(map) = unsafe { MmapOptions::new().map_copy_read_only(&file) } {
                return Ok(Self::Mapped(map));
            }
            // Filesystems without mmap support retain read behavior.
        }
        // Empty files and streams cannot use an ordinary file-backed map.
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(Self::Read(bytes))
    }

    pub(super) fn bytes(&self) -> &[u8] {
        match self {
            Self::Read(bytes) => bytes,
            Self::Mapped(map) => map,
        }
    }

    pub(super) fn text(&self) -> io::Result<&str> {
        std::str::from_utf8(self.bytes()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "keel-mmap-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn read_and_map_preserve_text_and_invalid_utf8_errors() {
        let temp = Temp::new();
        let path = temp.0.join("input");
        for bytes in [
            b"".as_slice(),
            "// λ界\r\n.text\nret\n".as_bytes(),
            &[0xff, b'\n'],
        ] {
            std::fs::write(&path, bytes).unwrap();
            let expected = std::fs::read_to_string(&path);
            let actual = Source::open(&path).unwrap();
            match expected {
                Ok(text) => assert_eq!(actual.text().unwrap(), text),
                Err(error) => {
                    let actual = actual.text().unwrap_err();
                    assert_eq!(actual.kind(), error.kind());
                    assert_eq!(actual.to_string(), error.to_string());
                }
            }
        }
        assert!(Source::open(temp.0.join("missing")).is_err());
    }

    #[test]
    fn mapped_validation_matches_std_at_vector_boundaries() {
        let temp = Temp::new();
        let path = temp.0.join("input.s");
        for length in [0, 1, 15, 16, 31, 32, 63, 64, 65, 127, 128, 129] {
            for suffix in [
                "// λ中🦀\n.asciz \"λ中🦀\"\n".as_bytes(),
                &[0xff],
                &[0xe2, 0x82],
            ] {
                let mut bytes = vec![b' '; length];
                bytes.extend_from_slice(suffix);
                std::fs::write(&path, &bytes).unwrap();
                let source = Source::open(&path).unwrap();
                match std::str::from_utf8(&bytes) {
                    Ok(text) => assert_eq!(source.text().unwrap(), text),
                    Err(_) => assert_eq!(
                        source.text().unwrap_err().kind(),
                        io::ErrorKind::InvalidData
                    ),
                }
            }
        }
    }

    #[test]
    fn mapped_assembly_output_outlives_input_and_allows_in_place_output() {
        let temp = Temp::new();
        let path = temp.0.join("input.s");
        std::fs::write(&path, ".text\nret\n").unwrap();
        let expected = keel_asm::assemble_object(".text\nret\n").unwrap();
        super::super::run(vec![
            "asm".into(),
            path.to_str().unwrap().into(),
            "-o".into(),
            path.to_str().unwrap().into(),
        ])
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), expected);
    }

    #[cfg(unix)]
    #[test]
    fn mapped_selection_falls_back_for_non_regular_input() {
        let source = Source::open("/dev/null").unwrap();
        assert!(matches!(source, Source::Read(_)));
        assert_eq!(source.text().unwrap(), "");
    }

    #[test]
    fn regular_files_map_automatically_and_empty_files_use_reads() {
        let temp = Temp::new();
        let path = temp.0.join("input.s");
        std::fs::write(&path, ".text\nret\n").unwrap();
        let source = Source::open(&path).unwrap();
        assert!(matches!(source, Source::Mapped(_)));
        drop(source);
        std::fs::write(&path, "").unwrap();
        let source = Source::open(&path).unwrap();
        assert!(matches!(source, Source::Read(_)));
        assert_eq!(source.text().unwrap(), "");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn mapped_binary_linking_allows_atomic_in_place_output() {
        let temp = Temp::new();
        let fixtures = [
            (
                "aarch64-apple-darwin",
                keel_asm::assemble_object(".text\n.globl _main\n_main:\nmov w0,#0\nret\n").unwrap(),
            ),
            (
                "x86_64-unknown-linux-gnu",
                keel_asm::x86::assemble_object(
                    ".text\n.globl _start\n_start:\nmov eax,60\nxor edi,edi\nsyscall\n",
                )
                .unwrap(),
            ),
        ];
        for (target, bytes) in fixtures {
            let input = temp.0.join(format!("{target}.o"));
            let output = temp.0.join(format!("{target}.exe"));
            std::fs::write(&input, &bytes).unwrap();
            let source = Source::open(&input).unwrap();
            assert!(matches!(source, Source::Mapped(_)));
            assert_eq!(source.bytes(), bytes);
            drop(source);
            let run = |output: &Path| {
                super::super::run(vec![
                    "link".into(),
                    "--target".into(),
                    target.into(),
                    input.to_str().unwrap().into(),
                    "-o".into(),
                    output.to_str().unwrap().into(),
                ])
                .unwrap()
            };
            run(&output);
            let expected = std::fs::read(&output).unwrap();
            run(&input);
            assert_eq!(std::fs::read(&input).unwrap(), expected);
            if target == "aarch64-apple-darwin" {
                assert!(
                    std::process::Command::new(&input)
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
    }
}
