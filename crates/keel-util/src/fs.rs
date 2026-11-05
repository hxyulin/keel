//! Output files that are replaced whole or not at all.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write a new file with `write` and rename it over `path`.
///
/// The temporary file sits in the destination's directory, so the rename
/// stays on one filesystem, and `create_new` keeps concurrent processes from
/// sharing it. The file is opened for reading and writing, so `write` can
/// read back what it wrote. On Unix, `mode` sets its permission bits; `None`
/// keeps the default the process umask gives. After `write` returns, the file
/// is renamed over `path`, so readers see either the old or the new file.
/// It is not flushed to disk: like other compilers and linkers, a crash can
/// lose the new output, and on macOS a full flush doubled link wall time. Any
/// failure removes the temporary file and leaves an existing `path`
/// unchanged.
///
/// The file is closed before the rename, so a running executable at `path`
/// keeps its old inode.
pub fn replace(
    path: &Path,
    mode: Option<u32>,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    let (temporary, mut file) = loop {
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(name);
        temporary_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(temporary_name);
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        write(&mut file)?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
        return result;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, Write};

    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("keel-util-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn entries(&self) -> Vec<String> {
            let mut names: Vec<_> = std::fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn replaces_existing_file_and_can_read_back() {
        let dir = Dir::new("replace");
        let path = dir.0.join("out.o");
        std::fs::write(&path, b"old").unwrap();
        replace(&path, None, |file| {
            file.write_all(b"new contents")?;
            file.rewind()?;
            let mut back = String::new();
            file.read_to_string(&mut back)?;
            assert_eq!(back, "new contents");
            Ok(())
        })
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new contents");
        assert_eq!(dir.entries(), ["out.o"]);
    }

    #[test]
    fn failure_keeps_old_file_and_removes_temporary() {
        let dir = Dir::new("failure");
        let path = dir.0.join("out.o");
        std::fs::write(&path, b"old").unwrap();
        let error = replace(&path, None, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("write failed"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "write failed");
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert_eq!(dir.entries(), ["out.o"]);
    }

    #[cfg(unix)]
    #[test]
    fn sets_requested_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Dir::new("mode");
        let path = dir.0.join("a.out");
        replace(&path, Some(0o755), |file| file.write_all(b"x")).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
