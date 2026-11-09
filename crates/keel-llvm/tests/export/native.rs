//! Native agreement on arm64 macOS: each fixture exported to LLVM IR and
//! built by Clang at -O0 and -O2 must behave like Keel's own code, or like
//! the C source built by Clang.
use super::translate;
use keel_ir::Target;
use std::path::Path;
use std::process::{Command, Output};

const TARGET: Target = Target::Aarch64AppleDarwin;
/// The scalar part of a Full round.
const SCALAR: &[&str] = &[
    "inline",
    "simplify-cfg",
    "canonicalize",
    "instcombine",
    "call-contracts",
    "simplify-cfg",
    "gvn",
    "stack-forward",
    "canonicalize",
    "instcombine",
    "dce",
    "gcm",
];

struct Dir(keel_test_support::TempDir);

impl Dir {
    fn write(&self, name: &str, text: &str) -> String {
        std::fs::write(self.0.join(name), text).unwrap();
        name.to_owned()
    }
    fn fixture(&self, name: &str) -> String {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap();
        self.write(name, &text)
    }
    fn clang(&self, args: &[&str]) -> Output {
        let output = Command::new("clang")
            .current_dir(self.0.path())
            .arg("-Wno-override-module")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "clang {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    /// Links `inputs` at `level` and returns the program's output.
    fn run(&self, level: &str, inputs: &[&str]) -> Output {
        let exe = format!("run{}", inputs.join("-").replace(['.', '/'], "_"));
        self.clang(&[&[level], inputs, &["-o", &exe]].concat());
        Command::new(self.0.join(&exe)).output().unwrap()
    }
    fn succeeds(&self, level: &str, inputs: &[&str]) -> Vec<u8> {
        let output = self.run(level, inputs);
        assert!(
            output.status.success(),
            "{inputs:?} at {level}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
    /// Keel's assembly and the exported LLVM IR for `module`.
    fn both(&self, stem: &str, module: &keel_ir::Module) -> (String, String) {
        let ll = keel_llvm::export(module, keel_llvm::TargetProfile::AppleArm64).unwrap();
        let asm = keel_codegen::compile(module, TARGET).unwrap();
        (
            self.write(&format!("{stem}.s"), &asm),
            self.write(&format!("{stem}.ll"), &ll),
        )
    }
}

fn c_module(source: &str) -> keel_ir::Module {
    let options = keel_c::Options {
        target: TARGET,
        ..Default::default()
    };
    keel_c::compile(source, "fixture.c", options).unwrap()
}

#[test]
fn exported_fixtures_run_like_keel_and_clang() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        keel_test_support::skip!("the AppleArm64 profile runs only on arm64 macOS");
    }
    if keel_test_support::clang().is_none() {
        keel_test_support::skip!(keel_test_support::NO_CLANG);
    }
    let dir = Dir(keel_test_support::TempDir::new("llvm-export-native"));
    // The exporter's data layout must be the one this Clang uses.
    let hot = dir.fixture("hot.c");
    dir.clang(&[
        "-target",
        "arm64-apple-macosx11.0.0",
        "-S",
        "-emit-llvm",
        "-O0",
        &hot,
        "-o",
        "clang.ll",
    ]);
    let layout = std::fs::read_to_string(dir.0.join("clang.ll")).unwrap();
    let layout = layout
        .lines()
        .find(|l| l.starts_with("target datalayout"))
        .unwrap();
    let exported = translate("func @f() {\nblock0:\n return\n}\n").unwrap();
    assert!(
        exported.lines().any(|l| l == layout),
        "{layout}\n{exported}"
    );

    for (fixture, driver, levels) in [
        ("core.keel", "driver.c", &["-O0", "-O2"][..]),
        ("extended.keel", "extended_driver.c", &["-O0", "-O2"]),
    ] {
        let text = std::fs::read_to_string(dir.0.join(dir.fixture(fixture))).unwrap();
        let (asm, ll) = dir.both(fixture, &keel_ir::parse(&text).unwrap());
        let driver = dir.fixture(driver);
        let expected = dir.succeeds("-O2", &[&asm, &driver]);
        for level in levels {
            assert_eq!(
                dir.succeeds(level, &[&ll, &driver]),
                expected,
                "{fixture} {level}"
            );
        }
    }

    let wide = keel_ir::parse(include_str!("../fixtures/wide.keel")).unwrap();
    let wide = dir.write(
        "wide.ll",
        &keel_llvm::export(&wide, keel_llvm::TargetProfile::AppleArm64).unwrap(),
    );
    let wide_driver = dir.fixture("wide_driver.c");
    for level in ["-O0", "-O2"] {
        dir.succeeds(level, &[&wide, &wide_driver]);
    }

    // An ordinary strlen with side effects in a separate object: no builtin
    // folding may remove the calls.
    let libcall = keel_ir::parse(include_str!("../fixtures/libcall.keel")).unwrap();
    let (asm, ll) = dir.both("libcall", &libcall);
    let provider = dir.fixture("libcall_provider.c");
    dir.clang(&["-O2", "-fno-builtin", "-c", &provider, "-o", "provider.o"]);
    let driver = dir.fixture("libcall_driver.c");
    let mut outputs = Vec::new();
    for source in [&asm, &ll] {
        for level in ["-O0", "-O2"] {
            outputs.push(dir.succeeds(level, &[source, "provider.o", &driver]));
        }
    }
    assert!(
        outputs.windows(2).all(|w| w[0] == w[1]),
        "strlen side effects differ"
    );
    let optimized = dir
        .clang(&["-O2", "-S", "-emit-llvm", "-x", "ir", &ll, "-o", "-"])
        .stdout;
    let optimized = String::from_utf8(optimized).unwrap();
    assert_eq!(
        optimized.matches("call i64 @strlen(").count(),
        2,
        "{optimized}"
    );
    // Without nobuiltin, Clang folds the calls and the driver fails.
    let unprotected = dir.write(
        "unprotected.ll",
        &std::fs::read_to_string(dir.0.join(&ll))
            .unwrap()
            .replace(" nobuiltin", ""),
    );
    let control = dir.run("-O2", &[&unprotected, "provider.o", &driver]);
    assert!(
        matches!(control.status.code(), Some(1 | 2)),
        "control: {:?}",
        control.status
    );

    let trap = keel_ir::parse(include_str!("../fixtures/trap.keel")).unwrap();
    let (asm, ll) = dir.both("trap", &trap);
    for source in [&asm, &ll] {
        for level in ["-O0", "-O2"] {
            let status = dir.run(level, &[source]).status;
            assert!(
                std::os::unix::process::ExitStatusExt::signal(&status).is_some(),
                "{source} at {level}: {status:?}"
            );
        }
    }

    // C fixtures: Clang's build of the C source is the oracle.
    for (source, driver) in [("hot.c", "hot_driver.c"), ("abi.c", "abi_driver.c")] {
        let text = std::fs::read_to_string(dir.0.join(dir.fixture(source))).unwrap();
        let driver = dir.fixture(driver);
        let expected = dir.succeeds("-O2", &[source, &driver]);
        let raw = c_module(&text);
        let mut scalar = raw.clone();
        keel_opt::run_passes(&mut scalar, SCALAR).unwrap();
        for (stem, module) in [("raw", &raw), ("scalar", &scalar)] {
            let stem = format!("{source}-{stem}");
            let (asm, ll) = dir.both(&stem, module);
            assert_eq!(
                dir.succeeds("-O2", &[&asm, &driver]),
                expected,
                "{stem} Keel"
            );
            for level in ["-O0", "-O2"] {
                assert_eq!(
                    dir.succeeds(level, &[&ll, &driver]),
                    expected,
                    "{stem} LLVM {level}"
                );
            }
        }
    }
}

#[test]
fn unsigned_remainder_values_and_zero_trap() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        keel_test_support::skip!("the AppleArm64 profile runs only on arm64 macOS");
    }
    if keel_test_support::clang().is_none() {
        keel_test_support::skip!(keel_test_support::NO_CLANG);
    }
    let dir = Dir(keel_test_support::TempDir::new("llvm-urem"));
    for width in [8, 16, 32, 64] {
        let src = format!(
            "func @rem(i{width},i{width})->i{width} {{\nblock0(v0:i{width},v1:i{width}):\n v2 = urem v0,v1\n jump block1(v2)\nblock1(v3:i{width}):\n v4 = urem v3,v1\n return v4\n}}"
        );
        let ll = dir.write("rem.ll", &translate(&src).unwrap());
        let c = dir.write("driver.c", &format!("#include <stdint.h>\nextern uint{width}_t rem(uint{width}_t,uint{width}_t);\nint main(int argc,char **argv) {{ volatile uint{width}_t a = UINT{width}_MAX; volatile uint{width}_t b = 10; if (argc > 1) {{ b = 0; return rem(a,b); }} return rem(a,b) != a % b; }}"));
        for level in ["-O0", "-O2"] {
            dir.succeeds(level, &[&ll, &c]);
            let exe = "zero";
            dir.clang(&[level, &ll, &c, "-o", exe]);
            let out = Command::new(dir.0.join(exe)).arg("zero").output().unwrap();
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                out.status.signal(),
                Some(5),
                "width {width}: {:?}",
                out.status
            );
        }
    }
}
