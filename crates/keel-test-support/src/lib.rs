//! Shared helpers for Keel's tests. Use this crate only as a dev-dependency.
//!
//! # External tools
//!
//! [`clang`], [`gnu_as`] and [`gnu_ld`] each resolve a tool once per test
//! process. An environment variable names the tool explicitly; otherwise the
//! helper searches `PATH` for the per-host default.
//!
//! | Helper | Variable | Default search |
//! | --- | --- | --- |
//! | [`clang`] | `KEEL_CLANG` | `clang` |
//! | [`gnu_as`] | `KEEL_GNU_AS` | Linux: `as`, `x86_64-linux-gnu-as`, `x86_64-elf-as`; elsewhere: `x86_64-elf-as`, `x86_64-linux-gnu-as` |
//! | [`gnu_ld`] | `KEEL_GNU_LD` | Linux: `ld`, `x86_64-linux-gnu-ld`, `x86_64-elf-ld`; elsewhere: `x86_64-elf-ld`, `x86_64-linux-gnu-ld` |
//!
//! `KEEL_TEST_GNU_AS` and `KEEL_TEST_GNU_LD` are deprecated aliases, read only
//! when the primary variable is unset. A tool named by a variable is used as
//! given, so a wrong path fails the test instead of skipping it. A searched
//! candidate is used only if `--version` identifies it as the expected tool:
//! GNU as configured for an x86-64 target, or GNU ld with the `elf_x86_64`
//! emulation.
//!
//! A bare-ELF GNU as (configured for a target such as `x86_64-elf`) treats `/`
//! as a comment character. [`Tool::command`] passes `--divide` to such an
//! assembler, so expressions assemble as they do with the Linux build.
//!
//! # Skips
//!
//! A test that cannot run because a host tool or host capability is missing
//! calls [`skip!`] (which returns from the enclosing function) or
//! [`skip_part!`] (which records the skip and continues, for tests that still
//! check something without the tool). Both print `KEEL-SKIP: <reason>` to
//! standard error. When `KEEL_SKIP_LOG` names a file, they also append one line
//! `<package>::<test>\t<reason>` to it; `tools/test.py` sets it and summarizes
//! the skips by reason. With `KEEL_REQUIRE_ORACLES=1`, both panic instead, so a
//! qualification host fails on a missing tool.
//!
//! # Native x86-64 programs
//!
//! [`native_program!`] describes a program that only runs on x86-64 Linux.
//! [`native::Program::check`] runs it there, and exports it as a packet when
//! `KEEL_TEST_EXPORT_DIR` is set, so `suite/integration/x86-exports/run.py`
//! can run it on a Linux host later. See [`native`].

pub mod native;

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs, process, thread};

/// An external program with the leading arguments every invocation needs.
#[derive(Debug, Clone)]
pub struct Tool {
    program: PathBuf,
    args: Vec<&'static str>,
}

impl Tool {
    /// The program as named by the environment variable or found on `PATH`.
    pub fn path(&self) -> &Path {
        &self.program
    }

    /// A command running this tool with its leading arguments already added.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }
}

/// The skip reason for a missing [`clang`].
pub const NO_CLANG: &str = "clang not found; set KEEL_CLANG";
/// The skip reason for a missing [`gnu_as`].
pub const NO_GNU_AS: &str = "x86-64 GNU as not found; set KEEL_GNU_AS";
/// The skip reason for a missing [`gnu_ld`].
pub const NO_GNU_LD: &str = "x86-64 GNU ld not found; set KEEL_GNU_LD";

/// Clang, from `KEEL_CLANG` or `clang` on `PATH`.
pub fn clang() -> Option<&'static Tool> {
    static TOOL: OnceLock<Option<Tool>> = OnceLock::new();
    TOOL.get_or_init(|| {
        if let Some(program) = configured("KEEL_CLANG", None) {
            return Some(Tool {
                program,
                args: Vec::new(),
            });
        }
        version(Path::new("clang"))
            .filter(|text| text.contains("clang"))
            .map(|_| Tool {
                program: "clang".into(),
                args: Vec::new(),
            })
    })
    .as_ref()
}

/// GNU as for x86-64, from `KEEL_GNU_AS` or the per-host default search.
pub fn gnu_as() -> Option<&'static Tool> {
    static TOOL: OnceLock<Option<Tool>> = OnceLock::new();
    TOOL.get_or_init(|| {
        if let Some(program) = configured("KEEL_GNU_AS", Some("KEEL_TEST_GNU_AS")) {
            let args = version(&program)
                .map(|text| gnu_as_args(&text))
                .unwrap_or_default();
            return Some(Tool { program, args });
        }
        let candidates: &[&str] = if cfg!(target_os = "linux") {
            &["as", "x86_64-linux-gnu-as", "x86_64-elf-as"]
        } else {
            &["x86_64-elf-as", "x86_64-linux-gnu-as"]
        };
        candidates.iter().find_map(|name| {
            let text = version(Path::new(name))?;
            let target = gnu_as_target(&text)?;
            target.starts_with("x86_64").then(|| Tool {
                program: name.into(),
                args: gnu_as_args(&text),
            })
        })
    })
    .as_ref()
}

/// GNU ld for x86-64 ELF, from `KEEL_GNU_LD` or the per-host default search.
pub fn gnu_ld() -> Option<&'static Tool> {
    static TOOL: OnceLock<Option<Tool>> = OnceLock::new();
    TOOL.get_or_init(|| {
        if let Some(program) = configured("KEEL_GNU_LD", Some("KEEL_TEST_GNU_LD")) {
            return Some(Tool {
                program,
                args: Vec::new(),
            });
        }
        let candidates: &[&str] = if cfg!(target_os = "linux") {
            &["ld", "x86_64-linux-gnu-ld", "x86_64-elf-ld"]
        } else {
            &["x86_64-elf-ld", "x86_64-linux-gnu-ld"]
        };
        candidates.iter().find_map(|name| {
            let program = Path::new(name);
            version(program).filter(|text| text.starts_with("GNU ld"))?;
            let emulations = output(program, "-V")?;
            emulations.contains("elf_x86_64").then(|| Tool {
                program: name.into(),
                args: Vec::new(),
            })
        })
    })
    .as_ref()
}

/// The program a variable names, falling back to a deprecated alias.
fn configured(name: &str, alias: Option<&str>) -> Option<PathBuf> {
    if let Some(value) = env::var_os(name).filter(|value| !value.is_empty()) {
        return Some(value.into());
    }
    let alias = alias?;
    let value: OsString = env::var_os(alias).filter(|value| !value.is_empty())?;
    eprintln!("KEEL-NOTE: {alias} is deprecated; set {name} instead");
    Some(value.into())
}

fn version(program: &Path) -> Option<String> {
    output(program, "--version")
}

fn output(program: &Path, arg: &str) -> Option<String> {
    let output = Command::new(program).arg(arg).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The target from GNU as's "configured for a target of `x86_64-elf'." line.
fn gnu_as_target(version: &str) -> Option<&str> {
    if !version.starts_with("GNU assembler") {
        return None;
    }
    let (_, rest) = version.split_once("configured for a target of `")?;
    let (target, _) = rest.split_once('\'')?;
    Some(target)
}

/// `--divide` for a bare-ELF build, where `/` otherwise starts a comment.
fn gnu_as_args(version: &str) -> Vec<&'static str> {
    match gnu_as_target(version) {
        Some(target) if target.ends_with("-elf") => vec!["--divide"],
        _ => Vec::new(),
    }
}

/// Records a skip and returns from the enclosing function.
///
/// The argument is a `format!` string with its arguments, or one expression
/// such as [`NO_GNU_AS`]. The enclosing function must return
/// `()`. See the [crate documentation](crate) for the output contract.
#[macro_export]
macro_rules! skip {
    ($($reason:tt)+) => {{
        $crate::skip_part!($($reason)+);
        return;
    }};
}

/// Records a skipped part of a test without returning.
///
/// Use it when a test still checks something without the missing tool.
#[macro_export]
macro_rules! skip_part {
    ($format:literal $($arguments:tt)*) => {
        $crate::record_skip(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::module_path!(),
            &::std::format!($format $($arguments)*),
        )
    };
    ($reason:expr) => {
        $crate::record_skip(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::module_path!(),
            ::core::convert::AsRef::<str>::as_ref(&$reason),
        )
    };
}

/// The implementation of [`skip!`] and [`skip_part!`]; call those instead.
#[doc(hidden)]
pub fn record_skip(package: &str, module: &str, reason: &str) {
    let reason = reason.replace(['\n', '\t'], " ");
    if env::var_os("KEEL_REQUIRE_ORACLES").is_some_and(|value| value == "1") {
        panic!("KEEL-SKIP not allowed with KEEL_REQUIRE_ORACLES=1: {reason}");
    }
    eprintln!("KEEL-SKIP: {reason}");
    let Some(log) = env::var_os("KEEL_SKIP_LOG").filter(|value| !value.is_empty()) else {
        return;
    };
    let line = format!("{package}::{}\t{reason}\n", test_name(module));
    let written = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .and_then(|mut file| file.write_all(line.as_bytes()));
    if let Err(error) = written {
        eprintln!(
            "KEEL-SKIP: cannot append to {}: {error}",
            Path::new(&log).display()
        );
    }
}

/// The libtest thread name, which is the test path inside its target, prefixed
/// with the target crate. Falls back to the module path off the test thread.
pub(crate) fn test_name(module: &str) -> String {
    let target = module.split("::").next().unwrap_or(module);
    match thread::current().name() {
        Some(name) if name != "main" => format!("{target}::{name}"),
        _ => module.to_owned(),
    }
}

/// A fresh directory under the system temporary directory, removed on drop.
///
/// The directory is kept when the thread is panicking, so a failing test
/// leaves its inputs for inspection.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates `<temp>/keel-<prefix>-<pid>-<n>`, replacing any stale copy.
    pub fn new(prefix: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("keel-{prefix}-{}-{n}", process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("create {}: {error}", path.display()));
        Self { path }
    }

    /// The directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A path inside the directory.
    pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.path.join(name)
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!("kept {} after a panic", self.path.display());
        } else {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELF_AS: &str = "GNU assembler (GNU Binutils) 2.47\n\
        This assembler was configured for a target of `x86_64-elf'.\n";
    const LINUX_AS: &str = "GNU assembler (GNU Binutils for Ubuntu) 2.42\n\
        This assembler was configured for a target of `x86_64-linux-gnu'.\n";

    #[test]
    fn bare_elf_assembler_gets_divide() {
        assert_eq!(gnu_as_target(ELF_AS), Some("x86_64-elf"));
        assert_eq!(gnu_as_args(ELF_AS), ["--divide"]);
        assert_eq!(gnu_as_target(LINUX_AS), Some("x86_64-linux-gnu"));
        assert!(gnu_as_args(LINUX_AS).is_empty());
        assert_eq!(gnu_as_target("Apple clang version 21.0.0"), None);
    }

    #[test]
    fn temp_dir_is_fresh_and_removed() {
        let first = TempDir::new("support");
        let second = TempDir::new("support");
        assert_ne!(first.path(), second.path());
        fs::write(first.join("file"), b"x").unwrap();
        let path = first.path().to_owned();
        drop(first);
        assert!(!path.exists());
        assert!(second.path().is_dir());
    }

    #[test]
    fn test_name_uses_the_libtest_thread() {
        let name = test_name("keel_test_support::tests");
        assert_eq!(
            name,
            "keel_test_support::tests::test_name_uses_the_libtest_thread"
        );
    }
}
