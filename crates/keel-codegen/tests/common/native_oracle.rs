//! Compile an immutable C oracle once, then link each generated image against it.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

pub struct PreparedOracle {
    directory: PathBuf,
}

impl PreparedOracle {
    pub fn new(tag: &str, source: &str, flags: &[&str]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "keel-oracle-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let oracle = Self { directory };
        fs::write(oracle.directory.join("oracle.c"), source).unwrap();
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            command(
                Command::new("clang")
                    .args(flags)
                    .arg("-c")
                    .arg(oracle.directory.join("oracle.c"))
                    .arg("-o")
                    .arg(oracle.directory.join("oracle.o")),
                tag,
            );
        }
        oracle
    }

    pub fn run(&self, object: &[u8], context: &str) {
        if !cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            return;
        }
        fs::write(self.directory.join("test.o"), object).unwrap();
        command(
            Command::new("clang")
                .arg(self.directory.join("oracle.o"))
                .arg(self.directory.join("test.o"))
                .arg("-o")
                .arg(self.directory.join("test")),
            context,
        );
        command(&mut Command::new(self.directory.join("test")), context);
    }
}

fn command(command: &mut Command, context: &str) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{context}: {command:?}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

impl Drop for PreparedOracle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
