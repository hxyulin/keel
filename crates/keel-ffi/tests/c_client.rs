//! Compile a real C client against the exported library, then execute its output.
use std::path::Path;
use std::process::Command;

fn execute(command: &mut Command) {
    let output = command.output().expect("run C toolchain command");
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn native_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        other => {
            eprintln!("KEEL-SKIP: no native FFI execution target for {other:?}");
            None
        }
    }
}

fn compile_client(cc: &std::ffi::OsStr, root: &Path, deps: &Path, source: &str, out: &Path) {
    execute(
        Command::new(cc)
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic"])
            .arg(root.join(source))
            .arg("-I")
            .arg(root.join("include"))
            .arg("-L")
            .arg(deps)
            .arg("-lkeel_ffi")
            .arg(format!("-Wl,-rpath,{}", deps.display()))
            .arg("-o")
            .arg(out),
    );
}

#[test]
fn c_client_builds_links_and_executes_native_object() {
    let Some(target) = native_target() else {
        return;
    };
    assert_eq!(keel_ffi::keel_abi_version(), 1);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = std::env::current_exe().expect("test executable path");
    let deps = exe.parent().expect("test dependencies directory");
    let work = std::env::temp_dir().join(format!("keel-ffi-c-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("create C test directory");
    let client = work.join("build-function");
    let object = work.join("function.o");
    let consumer = work.join("consumer");
    // CC is an executable path, not a shell command.
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    compile_client(&cc, root, deps, "examples/build_function.c", &client);
    execute(Command::new(&client).arg(target).arg(&object));
    execute(
        Command::new(&cc)
            .arg("-std=c11")
            .arg(root.join("tests/consumer.c"))
            .arg(&object)
            .arg("-o")
            .arg(&consumer),
    );
    execute(&mut Command::new(&consumer));
    // The same public header and client must also compile and link as C++.
    let cxx = std::env::var_os("CXX").unwrap_or_else(|| "c++".into());
    execute(
        Command::new(&cxx)
            .args([
                "-x",
                "c++",
                "-std=c++11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-pedantic",
            ])
            .arg(root.join("examples/build_function.c"))
            .arg("-I")
            .arg(root.join("include"))
            .arg("-L")
            .arg(deps)
            .arg("-lkeel_ffi")
            .arg(format!("-Wl,-rpath,{}", deps.display()))
            .arg("-o")
            .arg(&client),
    );
    execute(Command::new(&client).arg(target).arg(&object));
    execute(
        Command::new(&cc)
            .arg("-std=c11")
            .arg(root.join("tests/consumer.c"))
            .arg(&object)
            .arg("-o")
            .arg(&consumer),
    );
    execute(&mut Command::new(&consumer));
    std::fs::remove_dir_all(work).expect("remove C test directory");
}

#[test]
fn dsl_compiler_parses_source_and_executes_generated_functions() {
    let Some(target) = native_target() else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = std::env::current_exe().expect("test executable path");
    let deps = exe.parent().expect("test dependencies directory");
    let work = std::env::temp_dir().join(format!("keel-ffi-dsl-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("create DSL test directory");
    let compiler = work.join("dslc");
    let object = work.join("functions.o");
    let consumer = work.join("consumer");
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    compile_client(&cc, root, deps, "examples/dslc.c", &compiler);
    execute(
        Command::new(&compiler)
            .arg(target)
            .arg(root.join("examples/functions.dsl"))
            .arg(&object),
    );
    execute(
        Command::new(&cc)
            .arg("-std=c11")
            .arg(root.join("examples/dsl_consumer.c"))
            .arg(&object)
            .arg("-o")
            .arg(&consumer),
    );
    execute(&mut Command::new(&consumer));

    // Language semantics have independent C expectations, including wrapping
    // arithmetic, left associativity and comparison results in both directions.
    let source = work.join("semantics.dsl");
    std::fs::write(&source, "fn wrap(x) = x + 1;\nfn assoc(x) = x - 3 - 2;\nfn negate(x) = -(-x);\nfn choose(x) = if x == 7 then 10 else if x != 2 then -3 else if x <= 2 then 8 else 99;\n").expect("write semantics DSL");
    execute(
        Command::new(&compiler)
            .arg(target)
            .arg(&source)
            .arg(&object),
    );
    let oracle = work.join("semantics.c");
    std::fs::write(
        &oracle,
        r#"#include <stdint.h>
extern int64_t wrap(int64_t), assoc(int64_t), negate(int64_t), choose(int64_t);
int main(void) {
    return !(wrap(INT64_MAX) == INT64_MIN && assoc(10) == 5
        && negate(-9) == -9 && negate(INT64_MIN) == INT64_MIN
        && choose(7) == 10 && choose(3) == -3 && choose(2) == 8);
}
"#,
    )
    .expect("write semantics oracle");
    execute(
        Command::new(&cc)
            .arg("-std=c11")
            .arg(&oracle)
            .arg(&object)
            .arg("-o")
            .arg(&consumer),
    );
    execute(&mut Command::new(&consumer));

    let bad_source = work.join("bad.dsl");
    let mut bad_cases = vec![
        (
            "# empty program".to_owned(),
            "expected at least one function",
        ),
        ("fn f(x) = missing;".to_owned(), "unknown parameter"),
        ("fn f(x, x) = x;".to_owned(), "duplicate parameter"),
        (
            "fn f() = 1; fn f() = 2;".to_owned(),
            "duplicate function name",
        ),
        ("fn f() = 1 + ;".to_owned(), "expected integer"),
        ("fn f(x) = if x < 0 then 1;".to_owned(), "expected 'else'"),
        (
            "fn f(x) = if x then 1 else 2;".to_owned(),
            "expected comparison",
        ),
        (
            "fn f() = 9223372036854775808;".to_owned(),
            "integer literal exceeds",
        ),
        ("fn f() = 1 / 2;".to_owned(), "unexpected character"),
        ("fn f() = 1;\0fn g() = 2;".to_owned(), "embedded NUL"),
        (format!("fn f() = {}1;", "-".repeat(129)), "nesting limit"),
        (" ".repeat(65537), "more than 65536 bytes"),
    ];
    let params = (0..17)
        .map(|i| format!("p{i}"))
        .collect::<Vec<_>>()
        .join(",");
    bad_cases.push((format!("fn f({params}) = 0;"), "too many parameters"));
    for (source, diagnostic) in bad_cases {
        std::fs::write(&bad_source, source).expect("write invalid DSL");
        std::fs::write(&object, b"preserve existing output").expect("seed output");
        let output = Command::new(&compiler)
            .arg(target)
            .arg(&bad_source)
            .arg(&object)
            .output()
            .expect("run DSL compiler on invalid input");
        assert!(!output.status.success(), "must reject {diagnostic}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(diagnostic),
            "expected {diagnostic:?}, got {stderr}"
        );
        assert!(
            stderr.contains("bad.dsl:"),
            "diagnostic needs source path: {stderr}"
        );
        assert_eq!(
            std::fs::read(&object).expect("read preserved output"),
            b"preserve existing output"
        );
    }
    std::fs::write(&bad_source, "fn f(x) =\n  unknown;\n").expect("write located error");
    let output = Command::new(&compiler)
        .arg(target)
        .arg(&bad_source)
        .arg(&object)
        .output()
        .expect("run located diagnostic");
    assert!(String::from_utf8_lossy(&output.stderr).contains("bad.dsl:2:3: unknown parameter"));
    std::fs::remove_dir_all(work).expect("remove DSL test directory");
}

#[test]
fn systems_api_calls_memory_and_globals_execute_before_and_after_optimization() {
    use object::{Object, ObjectSection, ObjectSymbol};
    let Some(native) = native_target() else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe = std::env::current_exe().expect("test executable path");
    let deps = exe.parent().expect("test dependencies directory");
    let work = std::env::temp_dir().join(format!("keel-ffi-systems-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("create systems API test directory");
    let generator = work.join("systems-client");
    let consumer = work.join("consumer");
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    compile_client(&cc, root, deps, "tests/systems_client.c", &generator);
    for target in ["darwin-arm64", "linux-arm64", "linux-x86_64"] {
        for mode in ["baseline", "optimized"] {
            let output = work.join(format!("{target}-{mode}.o"));
            execute(Command::new(&generator).arg(target).arg(&output).arg(mode));
            let bytes = std::fs::read(&output).expect("read generated systems API object");
            let object = object::File::parse(bytes.as_slice()).expect("parse native object");
            let architecture = if target == "linux-x86_64" {
                object::Architecture::X86_64
            } else {
                object::Architecture::Aarch64
            };
            assert_eq!(object.architecture(), architecture);
            assert_eq!(
                object.format(),
                if target == "darwin-arm64" {
                    object::BinaryFormat::MachO
                } else {
                    object::BinaryFormat::Elf
                }
            );
            assert!(
                object
                    .symbols()
                    .any(|s| matches!(s.name(), Ok("system_run" | "_system_run"))
                        && s.is_definition())
            );
            assert!(object.symbols().any(|s| matches!(
                s.name(),
                Ok("helper_pointer" | "_helper_pointer")
            ) && s.is_definition()));
            assert!(
                object.sections().any(|s| s.relocations().next().is_some()),
                "global pointers/calls must produce relocations"
            );
            if target == native {
                execute(
                    Command::new(&cc)
                        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic"])
                        .arg(root.join("tests/systems_consumer.c"))
                        .arg(&output)
                        .arg("-o")
                        .arg(&consumer),
                );
                execute(&mut Command::new(&consumer));
            }
        }
    }
    // Reuse the real systems client to check every new header declaration in C++.
    let cxx = std::env::var_os("CXX").unwrap_or_else(|| "c++".into());
    execute(
        Command::new(cxx)
            .args([
                "-x",
                "c++",
                "-std=c++11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-pedantic",
            ])
            .arg("-fsyntax-only")
            .arg(root.join("tests/systems_client.c"))
            .arg("-I")
            .arg(root.join("include")),
    );
    std::fs::remove_dir_all(work).expect("remove systems API test directory");
}
