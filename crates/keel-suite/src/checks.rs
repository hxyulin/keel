//! Unit tests for the suite runners, without building or timing applications.
use anyhow::{Context, Result};
use clap::ValueEnum;
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
    fn directories(self) -> &'static [&'static str] {
        match self {
            Self::C => &["suite/c", "suite/c/lz4", "suite/c/yyjson"],
            Self::Ir => &["suite/ir"],
            Self::Asm => &["suite/asm/compiler_asm_compat"],
            Self::Linker => &["suite/linker/fixtures"],
            Self::Integration => &[
                "suite/integration",
                "suite/integration/linux-kernel",
                "suite/integration/x86-cpl0",
                "suite/integration/page-backing",
            ],
            Self::Common => &["suite/common"],
        }
    }
}

pub fn run(root: &Path, groups: &[Group], python: &Path) -> Result<bool> {
    let groups = if groups.is_empty() {
        Group::value_variants()
    } else {
        groups
    };
    let mut passed = true;
    for group in groups {
        for directory in group.directories() {
            println!("Checking {directory}");
            let status = Command::new(python)
                .current_dir(root)
                .args([
                    "-m",
                    "unittest",
                    "discover",
                    "-s",
                    directory,
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
