//! Target environment and resource headers are tested without Clang probes.
use keel_c::{PreprocessOptions, predefined_macros, preprocess};
use keel_ir::Target;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn macros(src: &str) -> BTreeMap<String, String> {
    src.lines()
        .filter_map(|line| {
            let line = line.strip_prefix("#define ")?;
            let (name, value) = line.split_once(' ').unwrap_or((line, ""));
            Some((name.into(), value.trim().into()))
        })
        .collect()
}

#[test]
fn derived_abi_macros_match_clang_for_each_target() {
    for target in Target::ALL {
        let mut cmd = Command::new("clang");
        cmd.args([
            "-E",
            "-dM",
            "-std=c11",
            "-x",
            "c",
            "-target",
            target.triple(),
        ]);
        if target == Target::Aarch64AppleDarwin {
            cmd.arg("-mmacosx-version-min=11.0");
        }
        let out = cmd
            .arg("-")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let reference = macros(&String::from_utf8(out.stdout).unwrap());
        let ours = macros(&predefined_macros(target, None).unwrap());
        assert!(
            ours.len() > 300,
            "most macros should be generated from ABI families"
        );
        let mut compared = 0;
        for (name, value) in &ours {
            // These are deliberately Keel-owned identity or capability choices.
            if name.starts_with("__KEEL")
                || name.starts_with("__clang_")
                || name == "__VERSION__"
                || name.starts_with("__STDC_NO_")
            {
                continue;
            }
            if name == "__BOOL_WIDTH__" {
                // Clang 18 reports storage bits (8); newer Clang reports
                // value bits (1). Keel consistently reports value bits.
                assert_eq!(value, "1");
                assert!(matches!(
                    reference.get(name).map(String::as_str),
                    Some("1" | "8")
                ));
                continue;
            }
            if let Some(expected) = reference.get(name) {
                assert_eq!(value, expected, "{}: {name}", target.triple());
                compared += 1;
            }
        }
        assert!(compared > 300);
        assert!(!ours.contains_key("__AVX__") && !ours.contains_key("__ARM_NEON"));
        assert_eq!(ours.get("__KEEL__").unwrap(), "1");
    }
}

#[test]
fn deployment_target_is_explicit_and_validated() {
    let defs = macros(&predefined_macros(Target::Aarch64AppleDarwin, Some("14.2.1")).unwrap());
    assert_eq!(
        defs["__ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__"],
        "140201"
    );
    for bad in ["", "hello", "10.15", "14.100", "14.0.0.0"] {
        assert!(predefined_macros(Target::Aarch64AppleDarwin, Some(bad)).is_err());
    }
}

#[test]
fn embedded_headers_compile_on_both_targets_without_a_sysroot() {
    let src = r#"
#include <stddef.h>
#include <stddef.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdalign.h>
#include <stdnoreturn.h>
#include <limits.h>
#include <float.h>
#include <stdatomic.h>
_Static_assert(sizeof(size_t) == 8 && sizeof(ptrdiff_t) == 8, "LP64");
_Static_assert(sizeof(int64_t) == 8 && sizeof(intmax_t) == 8, "integer types");
_Static_assert(sizeof(wchar_t) == 4 && CHAR_BIT == 8, "characters");
_Static_assert(INT8_MIN == -128 && UINT64_MAX == 18446744073709551615ULL, "integer limits");
_Static_assert(FLT_MANT_DIG == 24 && DBL_MANT_DIG == 53, "IEEE floats");
_Static_assert(alignof(max_align_t) == __BIGGEST_ALIGNMENT__, "maximum alignment");
struct S { char c; int i; };
_Static_assert(offsetof(struct S, i) == 4, "offsetof");
int f(int n, ...) {
    va_list ap, copy;
    va_start(ap, n);
    va_copy(copy, ap);
    int v = va_arg(copy, int);
    va_end(copy); va_end(ap);
    atomic_int value = ATOMIC_VAR_INIT(1);
    atomic_flag flag = ATOMIC_FLAG_INIT;
    atomic_init(&value, v);
    atomic_fetch_add(&value, 1);
    int expected = v + 1;
    atomic_compare_exchange_strong(&value, &expected, 42);
    atomic_flag_test_and_set(&flag); atomic_flag_clear(&flag);
    bool result = atomic_is_lock_free(&value);
    return atomic_load(&value) + result;
}
"#;
    for target in Target::ALL {
        // An empty root prevents Linux host libc from contributing anything.
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("empty-sdk");
        std::fs::create_dir_all(&root).unwrap();
        let opts = PreprocessOptions {
            target,
            sysroot: Some(root),
            ..Default::default()
        };
        let pp = preprocess(src, "headers.c", &opts).unwrap();
        assert!(!pp.contains("/clang/"));
        let module = keel_c::compile(
            &pp,
            "headers.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
    }
}

#[test]
fn feature_answers_describe_keel_and_unknown_features_are_false() {
    let pp = preprocess(r#"
#if !__has_builtin(__builtin_expect) || !__has_builtin(__builtin_sadd_overflow)
#error implemented builtin missing
#endif
#if !__has_feature(c_atomic) || !__has_attribute(packed)
#error implemented feature missing
#endif
#if __has_builtin(__builtin_assume) || __has_builtin(__builtin_add_overflow) || __has_feature(bounds_safety) || __has_attribute(cleanup) || __has_feature(imaginary_future_feature)
#error unsupported feature advertised
#endif
#if __has_feature(modules) || __building_module(anything)
#error modules unavailable
#endif
#if __is_identifier(int) || !__is_identifier(my_variable)
#error identifier query
#endif
int ok;
"#, "queries.c", &PreprocessOptions::default()).unwrap();
    keel_c::compile(&pp, "queries.c", Default::default()).unwrap();
}

#[test]
fn sysroot_system_paths_frameworks_and_user_override_order() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("synthetic-sdk");
    let user = dir.join("user");
    let system = dir.join("explicit-system");
    for d in [
        dir.join("usr/include"),
        dir.join("usr/include/x86_64-linux-gnu"),
        dir.join("System/Library/Frameworks/Demo.framework/Headers"),
        user.clone(),
        system.clone(),
    ] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(dir.join("usr/include/value.h"), "#define ROOT_VALUE 42\n").unwrap();
    std::fs::write(
        dir.join("usr/include/x86_64-linux-gnu/arch.h"),
        "#define ARCH_VALUE 64\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("System/Library/Frameworks/Demo.framework/Headers/demo.h"),
        "#define FRAMEWORK_VALUE 7\n",
    )
    .unwrap();
    std::fs::write(
        user.join("stddef.h"),
        "#define OVERRIDE 1\n#include_next <stddef.h>\n",
    )
    .unwrap();
    std::fs::write(system.join("extra.h"), "#define EXTRA 9\n").unwrap();
    for target in Target::ALL {
        let opts = PreprocessOptions {
            target,
            sysroot: Some(dir.clone()),
            include_dirs: vec![user.clone()],
            system_include_dirs: vec![system.clone()],
            ..Default::default()
        };
        let extra = if target == Target::Aarch64AppleDarwin {
            "#include <Demo/demo.h>\n_Static_assert(FRAMEWORK_VALUE == 7, \"framework\");"
        } else {
            "#include <arch.h>\n_Static_assert(ARCH_VALUE == 64, \"multiarch\");"
        };
        let src = format!(
            "#include <stddef.h>\n#include <value.h>\n#include <extra.h>\n_Static_assert(OVERRIDE && ROOT_VALUE == 42 && EXTRA == 9, \"search\");\n{extra}"
        );
        let pp = preprocess(&src, "search.c", &opts).unwrap();
        keel_c::compile(
            &pp,
            "search.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            preprocess("#include <stdio.h>", "isolation.c", &opts)
                .unwrap_err()
                .message
                .contains("header not found")
        );
    }
}

pub fn host_sysroot() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let out = Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
            .unwrap();
        assert!(out.status.success());
        Some(String::from_utf8(out.stdout).unwrap().trim().into())
    } else {
        None
    }
}

#[test]
fn standalone_environment_compiles_every_existing_fixture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let opts = PreprocessOptions {
        sysroot: host_sysroot(),
        ..Default::default()
    };
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
        .collect();
    paths.sort();
    for path in paths {
        let raw = std::fs::read_to_string(&path).unwrap();
        let pp = preprocess(&raw, path.to_str().unwrap(), &opts)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!pp.contains("/clang/"));
        let module = keel_c::compile(&pp, path.to_str().unwrap(), Default::default())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        keel_ir::verify_module(&module).unwrap();
    }
}

#[test]
fn macro_dump_preserves_definitions_and_can_be_reused() {
    let src = "#define REMOVE 1\n#undef REMOVE\n#define F(x, ...) x + __VA_ARGS__\n#define GNU(args...) args\n#define A 42\n";
    let opts = PreprocessOptions {
        dump_macros: true,
        ..Default::default()
    };
    let dumped = preprocess(src, "macros.c", &opts).unwrap();
    assert!(!dumped.contains("#define REMOVE"));
    assert!(dumped.contains("#define F(x,...) x + __VA_ARGS__"));
    assert!(dumped.contains("#define GNU(args...) args"));
    let pp = preprocess(
        &(dumped + "\nint f(void) { return GNU(F(A, 1)); }\n"),
        "reused.c",
        &PreprocessOptions::default(),
    )
    .unwrap();
    keel_c::compile(&pp, "reused.c", Default::default()).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
#[test]
fn native_pthread_headers_preserve_the_glibc_unwind_buffer_layout() {
    let source = "#include <pthread.h>\n\
        _Static_assert(sizeof(__pthread_unwind_buf_t) == 104, \"size\");\n\
        _Static_assert(_Alignof(__pthread_unwind_buf_t) == 16, \"alignment\");\n\
        int f(void) { return sizeof(pthread_t); }\n";
    let pp = preprocess(source, "pthread.c", &PreprocessOptions::default()).unwrap();
    let module = keel_c::compile(&pp, "pthread.c", Default::default()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
}
