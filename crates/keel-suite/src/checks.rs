//! Unit tests for the suite runners, without building or timing applications.
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, ValueEnum)]
pub enum Group {
    C,
    Ir,
    Asm,
    Linker,
    Integration,
    Common,
}

impl Group {
    fn directory(self) -> &'static str {
        match self {
            Self::C => "suite/c",
            Self::Ir => "suite/ir",
            Self::Asm => "suite/asm",
            Self::Linker => "suite/linker",
            Self::Integration => "suite/integration",
            Self::Common => "suite/common",
        }
    }
}

/// Directories that hold a `test_*.py` file in a NUL-separated path listing.
fn test_directories(listing: &[u8]) -> BTreeSet<String> {
    listing
        .split(|&byte| byte == 0)
        .filter_map(|path| std::str::from_utf8(path).ok())
        .filter_map(|path| {
            let (directory, name) = path.rsplit_once('/')?;
            (name.starts_with("test_") && name.ends_with(".py")).then(|| directory.to_owned())
        })
        .collect()
}

/// Find the group's tracked test directories, so untracked scratch files and
/// new directories need no runner change.
fn tracked_test_directories(root: &Path, group: Group) -> Result<BTreeSet<String>> {
    let directory = group.directory();
    let output = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z", "--", directory])
        .output()
        .context("running git ls-files")?;
    if !output.status.success() {
        bail!(
            "git ls-files failed for {directory}; suite checks need a Git checkout: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let directories = test_directories(&output.stdout);
    if directories.is_empty() {
        bail!("no tracked test_*.py files under {directory}");
    }
    Ok(directories)
}

pub fn run(root: &Path, groups: &[Group], python: &Path) -> Result<bool> {
    let groups = if groups.is_empty() {
        Group::value_variants()
    } else {
        groups
    };
    let mut passed = true;
    for &group in groups {
        for directory in tracked_test_directories(root, group)? {
            println!("Checking {directory}");
            let status = Command::new(python)
                .current_dir(root)
                .args([
                    "-m",
                    "unittest",
                    "discover",
                    "-s",
                    &directory,
                    "-p",
                    "test_*.py",
                ])
                .status()
                .with_context(|| format!("running suite checks in {directory}"))?;
            passed &= status.success();
        }
    }
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directories_group_test_files_by_parent() {
        let listing = b"suite/c/test_a.py\0suite/c/lz4/test_b.py\0suite/c/lz4/test_c.py\0\
            suite/c/lz4/validate.py\0suite/c/notes_test_x.py\0suite/c/test_data.json\0";
        let directories: Vec<_> = test_directories(listing).into_iter().collect();
        assert_eq!(directories, ["suite/c", "suite/c/lz4"]);
    }

    #[test]
    fn every_group_has_tracked_tests() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        if !root.join(".git").exists() {
            eprintln!("skip: not a Git checkout");
            return;
        }
        for &group in Group::value_variants() {
            let directories = tracked_test_directories(&root, group).unwrap();
            assert!(
                directories
                    .iter()
                    .all(|directory| directory.starts_with(group.directory())),
                "{directories:?}"
            );
        }
    }
}
