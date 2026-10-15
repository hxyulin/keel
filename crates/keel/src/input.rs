//! Mapped input for stable files, with ordinary reads for unmappable inputs.
//! Mappings live through parsing only;
//! returned compiler/assembler data owns its storage independently.
use memmap2::{Mmap, MmapOptions};
use std::{fs::File, io, io::Read, path::Path};

pub(super) enum Source {
    Read(String),
    Mapped(Mmap),
}

impl Source {
    /// The input must not be changed or truncated by another process until
    /// parsing completes.
    pub(super) fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if metadata.is_file() && metadata.len() != 0 {
            // SAFETY: CLI inputs remain stable while parsing. No reference
            // escapes parsing, and the caller drops Source before writing its
            // output. Copy-on-write does not protect against external truncation;
            // the documented CLI input-stability contract still applies.
            if let Ok(map) = unsafe { MmapOptions::new().map_copy_read_only(&file) } {
                return Ok(Self::Mapped(map));
            }
            // Filesystems without mmap support retain read behavior.
        }
        // Empty files and streams cannot use an ordinary file-backed map.
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        Ok(Self::Read(text))
    }

    pub(super) fn text(&self) -> io::Result<&str> {
        match self {
            Self::Read(text) => Ok(text),
            // Validate ASCII and multibyte UTF-8 together; an ASCII prescan
            // adds a second pass for Unicode input. The CLI uses a generic
            // error, so it does not need compat's detailed error position.
            Self::Mapped(map) => simdutf8::basic::from_utf8(map).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                )
            }),
        }
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
}
