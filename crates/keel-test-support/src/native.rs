//! Native x86-64 programs: run them on an x86-64 Linux host, or export them
//! as packets for `suite/integration/x86-exports/run.py`.
//!
//! A test describes a program with [`native_program!`](crate::native_program):
//! its files (Keel objects, C harnesses, assembly), the commands that build it,
//! the CPU features it needs and the result it must produce. [`Program::check`]
//! then does one or both of the following.
//!
//! - On an x86-64 Linux host it builds and runs the program in a temporary
//!   directory and panics on a wrong result. A missing CPU feature or build
//!   tool records a skip.
//! - When `KEEL_TEST_EXPORT_DIR` names an absolute directory, it writes the packet to
//!   `<dir>/<package>/<test>/<name>/`: the files plus `manifest.json`. The
//!   runner builds and runs every packet under `<dir>` on a Linux host.
//!
//! On any other host without `KEEL_TEST_EXPORT_DIR`, `check` records the skip
//! [`NOT_X86_LINUX`].
//!
//! # Manifest
//!
//! ```json
//! {
//!   "version": 1,
//!   "test": "keel-codegen::x86_fp_intrinsics::x86_fma::fma_exports",
//!   "name": "fma-Baseline-Fast",
//!   "features": ["avx2", "fma"],
//!   "files": ["fma.c", "fma.o"],
//!   "build": [["cc", "-O2", "-mfma", "fma.c", "fma.o", "-o", "program"]],
//!   "run": ["./program"],
//!   "exit": 0,
//!   "stdout": null
//! }
//! ```
//!
//! Build steps and the run command execute in the packet directory. A step
//! whose first word is `cc` uses the configured C compiler ([`clang`] here,
//! `--cc` in the runner). Any other first word names a program on `PATH`, and
//! the packet is skipped when that program is missing. Feature names are the
//! ones `is_x86_feature_detected!` accepts. `stdout` is `null` when any output
//! is accepted.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::{env, fs};

use crate::{NO_CLANG, TempDir, clang, test_name};

/// The skip reason on a host that cannot run x86-64 Linux programs.
pub const NOT_X86_LINUX: &str = "requires x86-64 Linux; set KEEL_TEST_EXPORT_DIR to export";

/// CPU feature names a manifest may list.
pub const FEATURES: &[&str] = &[
    "sse2",
    "sse3",
    "ssse3",
    "sse4.1",
    "sse4.2",
    "popcnt",
    "avx",
    "avx2",
    "fma",
    "f16c",
    "bmi1",
    "bmi2",
    "lzcnt",
    "movbe",
    "pclmulqdq",
    "vpclmulqdq",
    "aes",
    "sha",
    "cmpxchg16b",
    "xsave",
    "xsaveopt",
    "xsavec",
    "avx512f",
    "avx512cd",
    "avx512bw",
    "avx512dq",
    "avx512vl",
];

/// Whether [`Program::check`] runs or exports anything on this host: true on
/// x86-64 Linux and whenever `KEEL_TEST_EXPORT_DIR` is set.
///
/// A test that spends time building programs can check this first and skip
/// with [`NOT_X86_LINUX`] otherwise.
pub fn active() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
        || env::var_os("KEEL_TEST_EXPORT_DIR").is_some_and(|value| !value.is_empty())
}

/// Starts a [`Program`] named `name` for the calling test.
///
/// The name must be unique within the test; it becomes the packet directory.
#[macro_export]
macro_rules! native_program {
    ($name:expr) => {
        $crate::native::Program::new(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::module_path!(),
            $name,
        )
    };
}

/// A native program: its files, build steps, run command and expected result.
#[derive(Debug, Clone)]
#[must_use = "call check() to run or export the program"]
pub struct Program {
    package: String,
    test: String,
    name: String,
    features: BTreeSet<&'static str>,
    files: Vec<(String, Vec<u8>)>,
    build: Vec<Vec<String>>,
    run: Vec<String>,
    exit: i32,
    stdout: Option<String>,
}

impl Program {
    /// Use [`native_program!`](crate::native_program) instead, which fills in
    /// the package and module.
    #[doc(hidden)]
    pub fn new(package: &str, module: &str, name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(
            !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..",
            "program name {name:?} must be one path component"
        );
        Self {
            package: package.to_owned(),
            test: test_name(module),
            name,
            features: BTreeSet::new(),
            files: Vec::new(),
            build: Vec::new(),
            run: vec!["./program".to_owned()],
            exit: 0,
            stdout: None,
        }
    }

    /// Requires CPU features, named as `is_x86_feature_detected!` names them.
    pub fn requires(mut self, features: &[&str]) -> Self {
        for feature in features {
            let known = FEATURES
                .iter()
                .find(|known| *known == feature)
                .unwrap_or_else(|| panic!("unknown CPU feature {feature:?}"));
            self.features.insert(known);
        }
        self
    }

    /// Adds a file to the program directory.
    pub fn file(mut self, name: impl Into<String>, contents: impl AsRef<[u8]>) -> Self {
        let name = name.into();
        assert!(
            !name.is_empty() && !name.contains(['/', '\\']) && name != "manifest.json",
            "file name {name:?} must be one path component other than manifest.json"
        );
        if let Some(slot) = self.files.iter_mut().find(|(file, _)| *file == name) {
            slot.1 = contents.as_ref().to_vec();
        } else {
            self.files.push((name, contents.as_ref().to_vec()));
        }
        self
    }

    /// Adds a build step that runs the C compiler with `args`.
    pub fn compile<S: AsRef<str>>(self, args: impl IntoIterator<Item = S>) -> Self {
        self.step(
            std::iter::once("cc".to_owned()).chain(args.into_iter().map(|s| s.as_ref().to_owned())),
        )
    }

    /// Adds the final build step: the C compiler with `args` and
    /// `-o program`.
    pub fn link<S: AsRef<str>>(self, args: impl IntoIterator<Item = S>) -> Self {
        self.step(
            std::iter::once("cc".to_owned())
                .chain(args.into_iter().map(|s| s.as_ref().to_owned()))
                .chain(["-o".to_owned(), "program".to_owned()]),
        )
    }

    /// Adds a build step that runs `argv` as given.
    pub fn step<S: AsRef<str>>(mut self, argv: impl IntoIterator<Item = S>) -> Self {
        let argv: Vec<String> = argv.into_iter().map(|s| s.as_ref().to_owned()).collect();
        assert!(!argv.is_empty(), "a build step needs a program");
        self.build.push(argv);
        self
    }

    /// Passes arguments to the program.
    pub fn args<S: AsRef<str>>(mut self, args: impl IntoIterator<Item = S>) -> Self {
        self.run
            .extend(args.into_iter().map(|s| s.as_ref().to_owned()));
        self
    }

    /// Expects this exit status instead of 0.
    pub fn exit(mut self, status: i32) -> Self {
        self.exit = status;
        self
    }

    /// Expects exactly this standard output.
    pub fn stdout(mut self, expected: impl Into<String>) -> Self {
        self.stdout = Some(expected.into());
        self
    }

    /// Exports the program when `KEEL_TEST_EXPORT_DIR` is set, and runs it on
    /// an x86-64 Linux host. See the [module documentation](self).
    pub fn check(self) {
        let export = env::var_os("KEEL_TEST_EXPORT_DIR").filter(|value| !value.is_empty());
        if let Some(root) = &export {
            let root = Path::new(root);
            assert!(
                root.is_absolute(),
                "KEEL_TEST_EXPORT_DIR must be absolute, since each test runs in its package directory: {}",
                root.display()
            );
            self.export(root);
        }
        if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            if export.is_none() {
                record_once(&self.package, &self.test, NOT_X86_LINUX);
            }
            return;
        }
        if let Some(feature) = self.features.iter().find(|feature| !host_has(feature)) {
            record_once(&self.package, &self.test, &format!("host lacks {feature}"));
            return;
        }
        self.run_here();
    }

    /// The manifest text.
    pub fn manifest(&self) -> String {
        let mut text = String::from("{\n  \"version\": 1,\n");
        let _ = writeln!(
            text,
            "  \"test\": {},",
            json(&format!("{}::{}", self.package, self.test))
        );
        let _ = writeln!(text, "  \"name\": {},", json(&self.name));
        let _ = writeln!(text, "  \"features\": {},", json_list(self.features.iter()));
        let _ = writeln!(
            text,
            "  \"files\": {},",
            json_list(self.files.iter().map(|(name, _)| name))
        );
        text.push_str("  \"build\": [");
        for (index, step) in self.build.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str("\n    ");
            text.push_str(&json_list(step.iter()));
        }
        text.push_str(if self.build.is_empty() {
            "],\n"
        } else {
            "\n  ],\n"
        });
        let _ = writeln!(text, "  \"run\": {},", json_list(self.run.iter()));
        let _ = writeln!(text, "  \"exit\": {},", self.exit);
        let stdout = self
            .stdout
            .as_deref()
            .map_or_else(|| "null".to_owned(), json);
        let _ = writeln!(text, "  \"stdout\": {stdout}");
        text.push_str("}\n");
        text
    }

    fn export(&self, root: &Path) {
        let directory = root
            .join(&self.package)
            .join(self.test.replace("::", "."))
            .join(&self.name);
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("create {}: {error}", directory.display()));
        self.write_files(&directory);
        let manifest = directory.join("manifest.json");
        fs::write(&manifest, self.manifest())
            .unwrap_or_else(|error| panic!("write {}: {error}", manifest.display()));
    }

    fn write_files(&self, directory: &Path) {
        for (name, contents) in &self.files {
            let path = directory.join(name);
            fs::write(&path, contents)
                .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        }
    }

    fn run_here(&self) {
        let compiler = if self.build.iter().any(|step| step[0] == "cc") {
            let Some(tool) = clang() else {
                record_once(&self.package, &self.test, NO_CLANG);
                return;
            };
            Some(tool)
        } else {
            None
        };
        for step in &self.build {
            if step[0] != "cc" && !on_path(&step[0]) {
                record_once(&self.package, &self.test, &format!("{} not found", step[0]));
                return;
            }
        }
        let directory = TempDir::new(&format!("native-{}", self.name));
        self.write_files(directory.path());
        for step in &self.build {
            let mut command = match (step[0].as_str(), compiler) {
                ("cc", Some(tool)) => tool.command(),
                (program, _) => Command::new(program),
            };
            command.args(&step[1..]).current_dir(directory.path());
            let output = command
                .output()
                .unwrap_or_else(|error| panic!("{}: {step:?}: {error}", self.name));
            assert!(
                output.status.success(),
                "{}: build step {step:?} failed\n{}{}",
                self.name,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = Command::new(directory.join(&self.run[0]))
            .args(&self.run[1..])
            .current_dir(directory.path())
            .output()
            .unwrap_or_else(|error| panic!("{}: run {:?}: {error}", self.name, self.run));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            output.status.code(),
            Some(self.exit),
            "{}: {:?} exited with {}\nstdout:\n{stdout}\nstderr:\n{}",
            self.name,
            self.run,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        if let Some(expected) = &self.stdout {
            assert_eq!(&*stdout, expected, "{}: standard output", self.name);
        }
    }
}

/// Records a skip once per test and reason; a test often checks many
/// programs for the same reason.
fn record_once(package: &str, test: &str, reason: &str) {
    use std::cell::RefCell;
    thread_local! {
        static SEEN: RefCell<BTreeSet<(String, String)>> = const { RefCell::new(BTreeSet::new()) };
    }
    let first = SEEN.with(|seen| {
        seen.borrow_mut()
            .insert((test.to_owned(), reason.to_owned()))
    });
    if first {
        let module = test.split("::").next().unwrap_or(test);
        crate::record_skip(package, module, reason);
    }
}

fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| directory.join(program).is_file())
    })
}

#[cfg(target_arch = "x86_64")]
fn host_has(feature: &str) -> bool {
    match feature {
        "sse2" => std::is_x86_feature_detected!("sse2"),
        "sse3" => std::is_x86_feature_detected!("sse3"),
        "ssse3" => std::is_x86_feature_detected!("ssse3"),
        "sse4.1" => std::is_x86_feature_detected!("sse4.1"),
        "sse4.2" => std::is_x86_feature_detected!("sse4.2"),
        "popcnt" => std::is_x86_feature_detected!("popcnt"),
        "avx" => std::is_x86_feature_detected!("avx"),
        "avx2" => std::is_x86_feature_detected!("avx2"),
        "fma" => std::is_x86_feature_detected!("fma"),
        "f16c" => std::is_x86_feature_detected!("f16c"),
        "bmi1" => std::is_x86_feature_detected!("bmi1"),
        "bmi2" => std::is_x86_feature_detected!("bmi2"),
        "lzcnt" => std::is_x86_feature_detected!("lzcnt"),
        "movbe" => std::is_x86_feature_detected!("movbe"),
        "pclmulqdq" => std::is_x86_feature_detected!("pclmulqdq"),
        "vpclmulqdq" => std::is_x86_feature_detected!("vpclmulqdq"),
        "aes" => std::is_x86_feature_detected!("aes"),
        "sha" => std::is_x86_feature_detected!("sha"),
        "cmpxchg16b" => std::is_x86_feature_detected!("cmpxchg16b"),
        "xsave" => std::is_x86_feature_detected!("xsave"),
        "xsaveopt" => std::is_x86_feature_detected!("xsaveopt"),
        "xsavec" => std::is_x86_feature_detected!("xsavec"),
        "avx512f" => std::is_x86_feature_detected!("avx512f"),
        "avx512cd" => std::is_x86_feature_detected!("avx512cd"),
        "avx512bw" => std::is_x86_feature_detected!("avx512bw"),
        "avx512dq" => std::is_x86_feature_detected!("avx512dq"),
        "avx512vl" => std::is_x86_feature_detected!("avx512vl"),
        _ => panic!("unknown CPU feature {feature:?}"),
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn host_has(_feature: &str) -> bool {
    false
}

fn json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_list<S: AsRef<str>>(items: impl Iterator<Item = S>) -> String {
    let items: Vec<String> = items.map(|item| json(item.as_ref())).collect();
    format!("[{}]", items.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_lists_files_steps_and_expectations() {
        let program = native_program!("fma-Baseline")
            .requires(&["fma", "avx2"])
            .file("fma.c", "int main(void){return 0;}\n")
            .file("fma.o", [0x7f, b'E'])
            .compile(["-c", "fma.c", "-o", "harness.o"])
            .link(["harness.o", "fma.o", "-lm"])
            .args(["a\"b"])
            .exit(3)
            .stdout("PASS\n");
        let manifest = program.manifest();
        assert_eq!(
            manifest,
            concat!(
                "{\n",
                "  \"version\": 1,\n",
                "  \"test\": \"keel-test-support::keel_test_support::native::tests::manifest_lists_files_steps_and_expectations\",\n",
                "  \"name\": \"fma-Baseline\",\n",
                "  \"features\": [\"avx2\", \"fma\"],\n",
                "  \"files\": [\"fma.c\", \"fma.o\"],\n",
                "  \"build\": [\n",
                "    [\"cc\", \"-c\", \"fma.c\", \"-o\", \"harness.o\"],\n",
                "    [\"cc\", \"harness.o\", \"fma.o\", \"-lm\", \"-o\", \"program\"]\n",
                "  ],\n",
                "  \"run\": [\"./program\", \"a\\\"b\"],\n",
                "  \"exit\": 3,\n",
                "  \"stdout\": \"PASS\\n\"\n",
                "}\n",
            )
        );
    }

    #[test]
    fn export_writes_packet_under_package_and_test() {
        let root = TempDir::new("native-export");
        native_program!("one")
            .file("a.c", "x")
            .link(["a.c"])
            .export(root.path());
        let packet = root
            .join("keel-test-support")
            .join("keel_test_support.native.tests.export_writes_packet_under_package_and_test")
            .join("one");
        assert_eq!(fs::read_to_string(packet.join("a.c")).unwrap(), "x");
        assert!(packet.join("manifest.json").is_file());
    }

    #[test]
    #[should_panic(expected = "unknown CPU feature")]
    fn unknown_feature_is_rejected() {
        let _ = native_program!("x").requires(&["avx513"]);
    }
}
