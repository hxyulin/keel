//! One entry point for the native C and LLVM-import frontend checks.
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(clap::Args)]
pub struct Options {
    /// New artifact directory (must not already exist).
    #[arg(long)]
    out: PathBuf,
    /// Existing Keel binary; otherwise build privately under the artifact directory.
    #[arg(long)]
    keel: Option<PathBuf>,
    /// LLVM 23 shared library; otherwise discover it through rustc's sysroot.
    #[arg(long)]
    libllvm: Option<PathBuf>,
    /// Fail rather than skip LLVM checks on unsupported hosts.
    #[arg(long)]
    require_llvm: bool,
    /// Python interpreter used by the validation harness.
    #[arg(long, default_value = "python3")]
    python: PathBuf,
}

pub fn run(root: &Path, options: Options, jobs: usize) -> Result<bool> {
    let mut command = Command::new(options.python);
    command
        .current_dir(root)
        .arg(root.join("suite/frontend_validate.py"))
        .arg("--out")
        .arg(options.out)
        .arg("--jobs")
        .arg(jobs.to_string());
    if let Some(keel) = options.keel {
        command.arg("--keel").arg(keel);
    }
    if let Some(libllvm) = options.libllvm {
        command.arg("--libllvm").arg(libllvm);
    }
    if options.require_llvm {
        command.arg("--require-llvm");
    }
    Ok(command.status()?.success())
}
