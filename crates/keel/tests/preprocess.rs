#[path = "preprocess/incbin.rs"]
mod incbin;

use std::path::Path;
use std::process::{Command, Output};

fn invoke(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn success(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn kernel_header_flags_preserve_include_order_and_map_macro_paths() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-kernel-headers");
    std::fs::create_dir_all(dir.join("headers")).unwrap();
    std::fs::write(dir.join("first.h"), "#define OFFSET 2\n").unwrap();
    std::fs::write(
        dir.join("headers/second.h"),
        "const char *header = __FILE__;\nconst char *root = __BASE_FILE__;\nint level = __INCLUDE_LEVEL__;\n#define VALUE (BASE + OFFSET)\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#ifdef __STRICT_ANSI__\n#error expected GNU C11\n#endif\n#if __has_include(<stddef.h>)\n#error standard headers must be absent\n#endif\nint value = VALUE;\n",
    )
    .unwrap();
    let text = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-std=gnu11",
            "-nostdinc",
            "-Iheaders",
            "-DBASE=40",
            "-include",
            "first.h",
            "-include",
            "second.h",
            "-fmacro-prefix-map=headers/=mapped/",
            "-fmacro-prefix-map=main.c=app.c",
            "main.c",
        ],
    ));
    for expected in [
        "\"mapped/second.h\"",
        "\"app.c\"",
        "int level = 1",
        "int value = ( 40 + 2 )",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[test]
fn kernel_character_options_affect_preprocessing_and_c_types() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-kernel-characters");
    std::fs::create_dir_all(&dir).unwrap();
    let flags = [
        "--target=x86_64-linux-gnu",
        "-funsigned-char",
        "-fshort-wchar",
    ];
    let body = r#"
        typedef __WCHAR_TYPE__ wide;
        _Static_assert(sizeof(wide) == 2, "wchar width");
        _Static_assert((char)-1 == 255, "plain char is unsigned");
        _Static_assert('\xff' == 255, "ordinary character");
        _Static_assert(_Generic(L'x', unsigned short: 1, default: 0), "wide character type");
        _Static_assert(sizeof(L"𐐀") == 6, "UTF-16 surrogate pair");
        _Static_assert(sizeof("a" L"b") == 6, "wide concatenation");
        wide units[] = L"𐐀";
        int extend(char c) { return c; }
    "#;
    std::fs::write(
        dir.join("flags.c"),
        format!(
            "#if '\\xff' != 255 || __SIZEOF_WCHAR_T__ != 2 || !__CHAR_UNSIGNED__\n#error wrong character macros\n#endif\n{body}"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("flags.i"),
        body.replace("__WCHAR_TYPE__", "unsigned short"),
    )
    .unwrap();
    for file in ["flags.c", "flags.i"] {
        let mut args = vec!["cc", "--emit-ir"];
        args.extend(flags);
        args.push(file);
        let ir = success(invoke(&dir, &args));
        assert!(ir.contains("uext.i32"), "{file}: {ir}");
        assert!(
            ir.contains("@units") && ir.contains("align 2"),
            "{file}: {ir}"
        );
    }
}

#[test]
fn kernel_saved_temps_and_dependencies_follow_the_object_path() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-kernel-side-files");
    for subdir in ["build", "user", "system"] {
        std::fs::create_dir_all(dir.join(subdir)).unwrap();
    }
    std::fs::write(dir.join("user/text $.h"), "#define VALUE 40\n").unwrap();
    std::fs::write(dir.join("system/number.h"), "#define EXTRA 2\n").unwrap();
    std::fs::write(dir.join("forced.h"), "#define FORCED 1\n").unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#include \"text $.h\"\n#include <number.h>\nint value(void) { return VALUE + EXTRA + FORCED; }\n",
    )
    .unwrap();
    success(invoke(
        &dir,
        &[
            "cc",
            "--target=x86_64-linux-gnu",
            "-c",
            "-nostdinc",
            "-Iuser",
            "-isystem",
            "system",
            "-include",
            "forced.h",
            "-save-temps=obj",
            "-Wp,-MMD,build/value.d",
            "-o",
            "build/value.o",
            "main.c",
        ],
    ));
    let read = |path| std::fs::read_to_string(dir.join(path)).unwrap();
    assert!(read("build/value.i").contains("40 + 2 + 1"));
    assert!(read("build/value.s").contains("\"value\":"));
    let deps = read("build/value.d");
    assert!(deps.starts_with("build/value.o:"));
    for dependency in ["main.c", "forced.h", "user/text\\ $$.h"] {
        assert!(deps.contains(dependency), "{deps}");
    }
    assert!(!deps.contains("number.h"), "{deps}");
    success(invoke(
        &dir,
        &[
            "asm",
            "--target",
            "x86_64-linux-gnu",
            "build/value.s",
            "-o",
            "build/reassembled.o",
        ],
    ));
    assert_eq!(
        std::fs::read(dir.join("build/value.o")).unwrap(),
        std::fs::read(dir.join("build/reassembled.o")).unwrap()
    );
}

#[test]
fn kernel_gnu_macro_arguments_elide_only_the_omitted_comma() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-kernel-gnu-macros");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.c"),
        r#"
#define ZERO(...) { 2, ## __VA_ARGS__ }
#define NEXT(x, y) x + y
int a[] = ZERO();
int b = NEXT(40,
#if WANT_TWO
2
#else
5
#endif
);
"#,
    )
    .unwrap();
    let output = success(invoke(
        &dir,
        &["cc", "-E", "-std=gnu11", "-DWANT_TWO=1", "main.c"],
    ));
    assert!(output.contains("int a [ ] = { 2 }"), "{output}");
    assert!(
        output.contains("int b =") && output.contains("40 + 2"),
        "{output}"
    );
}

#[test]
fn kernel_warning_errors_are_scoped_to_the_requested_diagnostic() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-kernel-errors");
    std::fs::create_dir_all(&dir).unwrap();
    for (warning, source) in [
        ("date-time", "const char *date = __DATE__;"),
        ("implicit-int", "static value;"),
        (
            "implicit-function-declaration",
            "int f(void) { return undeclared(); }",
        ),
        ("strict-prototypes", "int f() { return 0; }"),
        ("return-type", "int f(int x) { if (x) return 2; }"),
        ("return-type", "int f(void) { return; }"),
        ("return-type", "void f(void) { return 2; }"),
        (
            "incompatible-pointer-types",
            "long *f(int *p) { return p; }",
        ),
        (
            "designated-init",
            "struct __attribute__((designated_init)) S { int n; }; struct S s = { 1 };",
        ),
    ] {
        std::fs::write(dir.join("main.c"), source).unwrap();
        let flag = format!("-Werror={warning}");
        let out = invoke(&dir, &["cc", &flag, "--emit-ir", "main.c"]);
        assert!(!out.status.success(), "{warning}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        let expected = if warning == "implicit-function-declaration" {
            "is not declared"
        } else {
            warning
        };
        assert!(stderr.contains(expected), "{stderr}");
    }
}

#[test]
fn kernel_warnings_report_without_failing_compilation() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-kernel-warnings");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("warnings.c"),
        "#if UNSET_CONFIG\n#error unexpected\n#endif\n\
         #if defined(SHORT_CIRCUITED) && SHORT_CIRCUITED\n#error unexpected\n#endif\n\
         inline int helper(void) { return 1; }\n\
         int takes_pointer(int *);\n\
         void (*generic)(void) = (void (*)(void))takes_pointer;\n\
         long (*mismatch)(int *) = (long (*)(int *))takes_pointer;\n\
         int f(int n) { int buf[n]; buf[0] = n; return buf[0]; }\n",
    )
    .unwrap();
    let run = |flags: &[&str]| {
        let mut args = vec!["cc", "--emit-ir", "warnings.c"];
        args.extend(flags);
        let result = invoke(&dir, &args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stderr).unwrap()
    };
    assert!(run(&[]).is_empty());
    let warnings = ["-Wundef", "-Wvla", "-Wmissing-prototypes"];
    let result = run(&warnings);
    assert!(!result.contains("SHORT_CIRCUITED"), "{result}");
    assert!(!result.contains("helper"), "{result}");
    for warning in warnings {
        assert!(result.contains(&format!("[{warning}]")), "{result}");
    }
    assert!(run(&["-Wmissing-declarations"]).contains("[-Wmissing-declarations]"));
    assert_eq!(
        run(&["-Wcast-function-type"])
            .matches("[-Wcast-function-type]")
            .count(),
        1
    );
}

#[test]
fn switch_warning_distinguishes_fallthrough_from_dispatch_and_exits() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-switch-warnings");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("switch.c"),
        r#"
        extern void side(void);
        int run(int n, int k) {
            switch (n) {
            case 0:
            case 1: k = n;
            case 2:
                if (k) break;
                else return 0;
            case 3:
                side();
                __attribute__((fallthrough));
            case 4:
                if (0) side();
                else return 0;
            case 5:
                if (k) { case 6: return 1; }
                side();
            case 7:
                side();
                goto after;
            after:
            default:
                break;
            }
            return k;
        }
    "#,
    )
    .unwrap();
    for flag in ["-Wimplicit-fallthrough", "-Wimplicit-fallthrough=5"] {
        let result = invoke(&dir, &["cc", "--emit-ir", flag, "switch.c"]);
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(result.status.success(), "{stderr}");
        let lines: Vec<_> = stderr
            .lines()
            .filter(|s| s.contains("[-Wimplicit-fallthrough]"))
            .collect();
        assert_eq!(lines.len(), 3, "{stderr}");
        assert!(lines[0].starts_with("switch.c:7:"), "{stderr}");
        assert!(lines[1].starts_with("switch.c:17:"), "{stderr}");
        assert!(lines[2].starts_with("switch.c:19:"), "{stderr}");
    }
    let disabled = invoke(
        &dir,
        &[
            "cc",
            "--emit-ir",
            "-Wimplicit-fallthrough=5",
            "-Wimplicit-fallthrough=0",
            "switch.c",
        ],
    );
    assert!(disabled.status.success() && disabled.stderr.is_empty());
}

#[test]
fn enum_conversion_checks_implicit_conversions_using_source_types() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-enum-warnings");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("enum.c"),
        r#"
        enum first { A };
        enum second { B };
        enum __attribute__((packed)) tiny { T };
        typedef enum first First;
        typedef enum second Second;
        struct pair { Second b; };
        struct pair exported = { .b = A };
        void take(Second);
        Second convert(First a, Second (*callback)(Second)) {
            Second b[] = { a };
            b[0] = a;
            take(a);
            callback(a);
            b[0] = (Second)a;
            b[0] = 0;
            return 1 ? a : A;
        }
        Second promoted(enum tiny a) { return 1 ? a : T; }
    "#,
    )
    .unwrap();
    let args = ["cc", "--emit-ir", "-Wenum-conversion", "enum.c"];
    let result = invoke(&dir, &args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let warnings = String::from_utf8(result.stderr).unwrap();
    assert_eq!(
        warnings
            .matches("from enum `first` to enum `second`")
            .count(),
        6,
        "{warnings}"
    );
    assert!(
        warnings.contains("from enum `tiny` to enum `second`"),
        "{warnings}"
    );
    let disabled = invoke(
        &dir,
        &[
            "cc",
            "--emit-ir",
            "-Wenum-conversion",
            "-Wno-enum-conversion",
            "enum.c",
        ],
    );
    assert!(disabled.status.success() && disabled.stderr.is_empty());
    assert_eq!(result.stdout, disabled.stdout);
}

#[test]
fn c_warning_groups_enable_diagnostics_and_respect_specific_overrides() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-warning-groups");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("groups.c"),
        r#"
        static int never_called(void) { return 1; }
        static const int unused_const = 1;
        enum first { FIRST };
        enum second { SECOND };
        extern void use(int);
        int run(int ignored_parameter, int signed_value, unsigned unsigned_value) {
            typedef int unused_type;
            int never_read;
            int write_only;
            int size_used;
            int keep __attribute__((unused));
            int count = 1;
            (void)sizeof(size_used);
            write_only = 1;
            count;
            (void)count;
            if (signed_value < unsigned_value);
            if (FIRST == SECOND) use(0);
            switch (signed_value) {
            case 0:
                use(1);
            default:
                break;
            }
        unused_label:
            return 2;
        }
        int no_return(void) {}
        int statement_result(int n) { return ({ int local = n; local; }); }
        extern int effect(void);
        void conditional_effect(int n) { n ? effect() : (n ? -22 : -6); n && effect(); }
    "#,
    )
    .unwrap();
    let run = |flags: &[&str]| {
        let mut args = vec!["cc", "--emit-ir", "groups.c"];
        args.extend_from_slice(flags);
        let output = invoke(&dir, &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        (output.stdout, String::from_utf8(output.stderr).unwrap())
    };
    let (baseline, quiet) = run(&[]);
    assert!(quiet.is_empty(), "{quiet}");
    let (diagnosed, wall) = run(&["-Wall"]);
    assert_eq!(baseline, diagnosed);
    for flag in [
        "unused-function",
        "unused-const-variable",
        "unused-local-typedefs",
        "unused-variable",
        "unused-but-set-variable",
        "unused-value",
        "unused-label",
        "return-type",
        "enum-compare",
    ] {
        assert!(wall.contains(&format!("[-W{flag}]")), "{flag}\n{wall}");
    }
    assert_eq!(wall.matches("[-Wunused-value]").count(), 1, "{wall}");
    assert!(!wall.contains("unused-parameter"), "{wall}");
    assert!(
        !wall.contains("size_used") && !wall.contains("variable `keep`"),
        "{wall}"
    );
    let (_, extra) = run(&["-Wextra", "-Wunused"]);
    for flag in [
        "unused-parameter",
        "sign-compare",
        "empty-body",
        "implicit-fallthrough",
    ] {
        assert!(extra.contains(&format!("[-W{flag}]")), "{flag}\n{extra}");
    }
    let (_, suppressed) = run(&[
        "-Wno-unused-parameter",
        "-Wno-sign-compare",
        "-Wno-unused-const-variable",
        "-Wno-unused-but-set-variable",
        "-Wno-implicit-fallthrough",
        "-Wextra",
        "-Wall",
        "-Wunused",
    ]);
    for flag in [
        "unused-parameter",
        "sign-compare",
        "unused-const-variable",
        "unused-but-set-variable",
        "implicit-fallthrough",
    ] {
        assert!(
            !suppressed.contains(&format!("[-W{flag}]")),
            "{flag}\n{suppressed}"
        );
    }
    assert!(
        suppressed.contains("unused variable `never_read`"),
        "{suppressed}"
    );
    assert!(suppressed.contains("[-Wreturn-type]"), "{suppressed}");
}

#[test]
fn raw_c_preprocess_compile_and_preprocessed_input() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-preprocess");
    std::fs::create_dir_all(dir.join("headers")).unwrap();
    std::fs::write(dir.join("headers/value.h"), "#define VALUE (BASE + 2)\n").unwrap();
    std::fs::write(dir.join("main.c"), "#include <value.h>\n#ifdef OMIT\n#error should be undefined\n#endif\nint main(void) { return VALUE; }\n").unwrap();
    let preprocessed = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-Iheaders",
            "-DBASE=40",
            "-DOMIT",
            "-UOMIT",
            "main.c",
        ],
    ));
    assert!(preprocessed.contains("40 + 2"), "{preprocessed}");
    let ir = success(invoke(
        &dir,
        &[
            "cc",
            "--emit-ir",
            "-I",
            "headers",
            "-D",
            "BASE=40",
            "-U",
            "OMIT",
            "main.c",
        ],
    ));
    assert!(ir.contains("@main"));
    std::fs::write(dir.join("main.i"), preprocessed).unwrap();
    let from_i = success(invoke(
        &dir,
        &[
            "cc",
            "--emit-ir",
            "--sysroot",
            "does-not-exist-sdk",
            "main.i",
        ],
    ));
    assert_eq!(from_i, ir);
    let conflict = invoke(&dir, &["cc", "-E", "--emit-ir", "main.c"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be used together"));
    let conflict = invoke(&dir, &["cc", "-E", "--emit-object", "main.c"]);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot be used together"));
}

#[test]
fn default_environment_uses_sysroot_and_embedded_headers_without_clang() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-sysroot");
    std::fs::create_dir_all(dir.join("sdk/usr/include")).unwrap();
    std::fs::create_dir_all(dir.join("system")).unwrap();
    std::fs::write(dir.join("sdk/usr/include/sdk.h"), "#define SDK_VALUE 40\n").unwrap();
    std::fs::write(dir.join("system/extra.h"), "#define EXTRA_VALUE 2\n").unwrap();
    std::fs::write(dir.join("main.c"), "#include <sdk.h>\n#include <extra.h>\n#include <stddef.h>\n#include <stdint.h>\n#ifndef __KEEL__\n#error expected native environment\n#endif\nint main(void) { return SDK_VALUE + EXTRA_VALUE + sizeof(uint64_t) - sizeof(size_t); }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .env("PATH", "")
        .args([
            "cc",
            "--sysroot=sdk",
            "-isystem",
            "system",
            "--emit-ir",
            "main.c",
        ])
        .output()
        .unwrap();
    let ir = success(out);
    assert!(ir.contains("@main"));
    success(invoke(
        &dir,
        &[
            "cc",
            "-isysroot",
            "sdk",
            "-isystem",
            "system",
            "--emit-ir",
            "main.c",
        ],
    ));
    for args in [
        vec!["cc", "--sysroot=", "main.c"],
        vec!["cc", "--sysroot", "missing-directory", "main.c"],
    ] {
        assert!(!invoke(&dir, &args).status.success());
    }
    for flag in ["--clang", "--no-clang"] {
        let out = invoke(&dir, &["cc", flag, "main.c"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unknown flag"));
    }
}

#[test]
fn macro_dump_reports_target_abi_and_compatibility_identity() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-macros");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("empty.c"), "#define USER 42\n").unwrap();
    let out = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-dM",
            "--target",
            "x86_64-unknown-linux-gnu",
            "empty.c",
        ],
    ));
    assert!(out.contains("#define __KEEL__ 1"));
    assert!(out.contains("#define __SIZEOF_LONG_DOUBLE__ 16"));
    assert!(out.contains("#define __INT64_TYPE__ long int"));
    assert!(out.contains("#define USER 42"));
    let out = success(invoke(
        &dir,
        &[
            "cc",
            "-E",
            "-dM",
            "--target",
            "aarch64-apple-darwin",
            "-mmacosx-version-min=14.2",
            "empty.c",
        ],
    ));
    assert!(out.contains("#define __ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__ 140200"));
    assert!(out.contains("#define __INT64_TYPE__ long long int"));
}

#[test]
fn preprocessing_releases_mapped_input_before_in_place_output() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-preprocess-in-place");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#define ANSWER 42\nint main(void) { return ANSWER; }\n",
    )
    .unwrap();
    success(invoke(&dir, &["cc", "-E", "main.c", "-o", "main.c"]));
    assert!(
        std::fs::read_to_string(dir.join("main.c"))
            .unwrap()
            .contains("return 42")
    );
    std::fs::rename(dir.join("main.c"), dir.join("main.i")).unwrap();
    assert!(success(invoke(&dir, &["cc", "--emit-ir", "main.i"])).contains("@main"));
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn raw_c_preprocessing_composes_with_native_object_output() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-preprocess-object");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#define ANSWER 42\nint main(void) { return ANSWER; }\n",
    )
    .unwrap();
    success(invoke(
        &dir,
        &["cc", "--emit-object", "main.c", "-o", "main.o"],
    ));
    assert_eq!(
        &std::fs::read(dir.join("main.o")).unwrap()[..4],
        &[0xcf, 0xfa, 0xed, 0xfe]
    );
}

#[test]
fn linux_object_from_preprocessed_input_needs_no_external_tools() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-linux-native-no-tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.i"), "int main(void) { return 42; }\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .env("PATH", &dir)
        .args([
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-object",
            "main.i",
            "-o",
            "main.o",
        ])
        .output()
        .unwrap();
    success(output);
    assert_eq!(&std::fs::read(dir.join("main.o")).unwrap()[..4], b"\x7fELF");
}

#[test]
fn standalone_linux_assembler_needs_no_external_tools() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-linux-asm-no-tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("answer.s"),
        ".intel_syntax noprefix\n.globl answer\nanswer: mov eax,42; ret\n",
    )
    .unwrap();
    success(
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .env("PATH", &dir)
            .args([
                "asm",
                "--target",
                "x86_64-unknown-linux-gnu",
                "answer.s",
                "--out-dir",
                "objects",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(
        &std::fs::read(dir.join("objects/answer.o")).unwrap()[..4],
        b"\x7fELF"
    );
}
