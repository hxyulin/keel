//! Keel's deterministic C11 target environment. Values are derived from
//! the target ABI, rather than copied from a host Clang installation.
use keel_ir::Target;
use std::fmt::Write;

/// Predefined object/function macros in `#define` form. Compiler identity is a
/// pinned compatibility profile; capability queries are maintained separately.
pub fn predefined_macros(
    target: Target,
    darwin_deployment: Option<&str>,
) -> Result<String, String> {
    let wasm = target == Target::Wasm32UnknownUnknown;
    let apple = target == Target::Aarch64AppleDarwin;
    let arm_linux = target == Target::Aarch64LinuxGnu;
    if !apple && darwin_deployment.is_some() {
        return Err("macOS deployment target requires aarch64-apple-darwin".into());
    }
    let mut out = String::new();
    let mut def = |name: &str, value: &str| {
        writeln!(out, "#define {name} {value}").unwrap();
    };
    for (name, value) in [
        ("__KEEL__", "1"),
        (
            "__KEEL_VERSION__",
            concat!("\"", env!("CARGO_PKG_VERSION"), "\""),
        ),
        ("__clang__", "1"),
        ("__clang_major__", "18"),
        ("__clang_minor__", "0"),
        ("__clang_patchlevel__", "0"),
        ("__clang_version__", "\"18.0.0 (Keel compatibility)\""),
        ("__GNUC__", "4"),
        ("__GNUC_MINOR__", "2"),
        ("__GNUC_PATCHLEVEL__", "1"),
        ("__GNUC_STDC_INLINE__", "1"),
        ("__STDC__", "1"),
        ("__STDC_HOSTED__", if wasm { "0" } else { "1" }),
        ("__STDC_VERSION__", "201112L"),
        ("__STDC_NO_COMPLEX__", "1"),
        ("__STDC_NO_VLA__", "1"),
        ("__STDC_NO_THREADS__", "1"),
        ("__STDC_UTF_16__", "1"),
        ("__STDC_UTF_32__", "1"),
        ("__STRICT_ANSI__", "1"),
        ("__CHAR_BIT__", "8"),
        ("__BOOL_WIDTH__", "1"),
        ("__SCHAR_MAX__", "127"),
        ("__SHRT_MAX__", "32767"),
        ("__INT_MAX__", "2147483647"),
        (
            "__LONG_MAX__",
            if wasm {
                "2147483647L"
            } else {
                "9223372036854775807L"
            },
        ),
        ("__LONG_LONG_MAX__", "9223372036854775807LL"),
        ("__SHRT_WIDTH__", "16"),
        ("__INT_WIDTH__", "32"),
        ("__LONG_WIDTH__", if wasm { "32" } else { "64" }),
        ("__LLONG_WIDTH__", "64"),
        ("__POINTER_WIDTH__", if wasm { "32" } else { "64" }),
        ("__ORDER_LITTLE_ENDIAN__", "1234"),
        ("__ORDER_BIG_ENDIAN__", "4321"),
        ("__ORDER_PDP_ENDIAN__", "3412"),
        ("__BYTE_ORDER__", "__ORDER_LITTLE_ENDIAN__"),
        ("__LITTLE_ENDIAN__", "1"),
        (
            "__WCHAR_TYPE__",
            if arm_linux { "unsigned int" } else { "int" },
        ),
        ("__WCHAR_WIDTH__", "32"),
        (
            "__WCHAR_MAX__",
            if arm_linux {
                "4294967295U"
            } else {
                "2147483647"
            },
        ),
        ("__CHAR16_TYPE__", "unsigned short"),
        ("__CHAR32_TYPE__", "unsigned int"),
        ("__SIG_ATOMIC_TYPE__", if wasm { "long int" } else { "int" }),
        ("__SIG_ATOMIC_WIDTH__", "32"),
        (
            "__SIG_ATOMIC_MAX__",
            if wasm { "2147483647L" } else { "2147483647" },
        ),
        ("__REGISTER_PREFIX__", ""),
        ("__FINITE_MATH_ONLY__", "0"),
        ("__FLT_RADIX__", "2"),
        ("__FLT_EVAL_METHOD__", "0"),
        ("__DECIMAL_DIG__", "__LDBL_DECIMAL_DIG__"),
        ("__clang_literal_encoding__", "\"UTF-8\""),
        ("__clang_wide_literal_encoding__", "\"UTF-32\""),
    ] {
        def(name, value);
    }
    if wasm {
        def("__wasm", "1");
        def("__wasm__", "1");
        def("__wasm32", "1");
        def("__wasm32__", "1");
        def("__ILP32__", "1");
        def("_ILP32", "1");
        def("__STDC_NO_ATOMICS__", "1");
    } else {
        def("_LP64", "1");
        def("__LP64__", "1");
    }
    def(
        "__VERSION__",
        concat!("\"Keel ", env!("CARGO_PKG_VERSION"), "\""),
    );
    for (name, size) in [
        ("SHORT", 2),
        ("INT", 4),
        ("LONG", if wasm { 4 } else { 8 }),
        ("LONG_LONG", 8),
        ("FLOAT", 4),
        ("DOUBLE", 8),
        ("LONG_DOUBLE", if apple { 8 } else { 16 }),
        ("POINTER", if wasm { 4 } else { 8 }),
        ("SIZE_T", if wasm { 4 } else { 8 }),
        ("PTRDIFF_T", if wasm { 4 } else { 8 }),
        ("WCHAR_T", 4),
        ("WINT_T", 4),
    ] {
        def(&format!("__SIZEOF_{name}__"), &size.to_string());
    }
    if !wasm {
        def("__SIZEOF_INT128__", "16");
    }
    def("__BIGGEST_ALIGNMENT__", if apple { "8" } else { "16" });
    def("__USER_LABEL_PREFIX__", if apple { "_" } else { "" });
    def(
        "__WINT_TYPE__",
        if apple || wasm { "int" } else { "unsigned int" },
    );
    def(
        "__WINT_MAX__",
        if apple || wasm {
            "2147483647"
        } else {
            "4294967295U"
        },
    );
    def("__WINT_WIDTH__", "32");
    if !apple && !wasm {
        def("__WINT_UNSIGNED__", "1");
    }
    let wide = if apple || wasm {
        ("long long int", "long long unsigned int", "LL", "ULL", "ll")
    } else {
        ("long int", "long unsigned int", "L", "UL", "l")
    };
    let types = [
        (8, "signed char", "unsigned char", "", "", "hh"),
        (16, "short", "unsigned short", "", "", "h"),
        (32, "int", "unsigned int", "", "U", ""),
        (64, wide.0, wide.1, wide.2, wide.3, wide.4),
    ];
    for (bits, signed, unsigned, ss, us, fmt) in types {
        let max = ((1u128 << (bits - 1)) - 1).to_string();
        let umax = ((1u128 << bits) - 1).to_string();
        for family in [
            bits.to_string(),
            format!("_LEAST{bits}"),
            format!("_FAST{bits}"),
        ] {
            for (u, ty, limit, suffix) in [("", signed, &max, ss), ("U", unsigned, &umax, us)] {
                let prefix = format!("__{u}INT{family}");
                def(&format!("{prefix}_TYPE__"), ty);
                def(&format!("{prefix}_MAX__"), &format!("{limit}{suffix}"));
                if family.starts_with('_') {
                    def(&format!("{prefix}_WIDTH__"), &bits.to_string());
                }
                for conversion in if u.is_empty() {
                    &['d', 'i'][..]
                } else {
                    &['u', 'o', 'x', 'X'][..]
                } {
                    def(
                        &format!("{prefix}_FMT{conversion}__"),
                        &format!("\"{fmt}{conversion}\""),
                    );
                }
            }
        }
        for (u, suffix) in [("", ss), ("U", us)] {
            def(&format!("__{u}INT{bits}_C_SUFFIX__"), suffix);
            def(
                &format!("__{u}INT{bits}_C(c)"),
                &if suffix.is_empty() {
                    "c".into()
                } else {
                    format!("c##{suffix}")
                },
            );
        }
    }
    for (name, ty, suffix, fmt, unsigned) in [
        ("INTMAX", "long int", "L", "l", false),
        ("UINTMAX", "long unsigned int", "UL", "l", true),
        ("INTPTR", "long int", "L", "l", false),
        ("UINTPTR", "long unsigned int", "UL", "l", true),
        ("PTRDIFF", "long int", "L", "l", false),
        ("SIZE", "long unsigned int", "UL", "l", true),
    ] {
        let (ty, suffix, fmt, bits) = if wasm {
            match name {
                "INTMAX" => ("long long int", "LL", "ll", 64),
                "UINTMAX" => ("long long unsigned int", "ULL", "ll", 64),
                "INTPTR" => ("long int", "L", "l", 32),
                "UINTPTR" => ("long unsigned int", "UL", "l", 32),
                "PTRDIFF" => ("long int", "L", "l", 32),
                "SIZE" => ("long unsigned int", "UL", "l", 32),
                _ => unreachable!(),
            }
        } else {
            (ty, suffix, fmt, 64)
        };
        def(&format!("__{name}_TYPE__"), ty);
        def(&format!("__{name}_WIDTH__"), &bits.to_string());
        def(
            &format!("__{name}_MAX__"),
            &format!(
                "{}{suffix}",
                if bits == 32 && unsigned {
                    "4294967295"
                } else if bits == 32 {
                    "2147483647"
                } else if unsigned {
                    "18446744073709551615"
                } else {
                    "9223372036854775807"
                }
            ),
        );
        for conversion in if unsigned {
            &['u', 'o', 'x', 'X'][..]
        } else {
            &['d', 'i'][..]
        } {
            def(
                &format!("__{name}_FMT{conversion}__"),
                &format!("\"{fmt}{conversion}\""),
            );
        }
        if name.ends_with("MAX") {
            def(&format!("__{name}_C_SUFFIX__"), suffix);
            def(&format!("__{name}_C(c)"), &format!("c##{suffix}"));
        }
    }
    if !wasm {
        for (name, value) in [
            ("RELAXED", 0),
            ("CONSUME", 1),
            ("ACQUIRE", 2),
            ("RELEASE", 3),
            ("ACQ_REL", 4),
            ("SEQ_CST", 5),
        ] {
            def(&format!("__ATOMIC_{name}"), &value.to_string());
        }
        for name in [
            "BOOL", "CHAR", "CHAR16_T", "CHAR32_T", "WCHAR_T", "SHORT", "INT", "LONG", "LLONG",
            "POINTER",
        ] {
            for compiler in ["GCC", "CLANG"] {
                def(&format!("__{compiler}_ATOMIC_{name}_LOCK_FREE"), "2");
            }
        }
        def("__GCC_ATOMIC_TEST_AND_SET_TRUEVAL", "1");
        for size in [1, 2, 4, 8] {
            def(&format!("__GCC_HAVE_SYNC_COMPARE_AND_SWAP_{size}"), "1");
        }
    }
    for (
        prefix,
        suffix,
        mant,
        dig,
        decimal,
        min_exp,
        max_exp,
        min10,
        max10,
        min,
        max,
        epsilon,
        denorm,
    ) in [
        (
            "FLT",
            "F",
            24,
            6,
            9,
            -125,
            128,
            -37,
            38,
            "1.17549435e-38",
            "3.40282347e+38",
            "1.19209290e-7",
            "1.40129846e-45",
        ),
        (
            "DBL",
            "",
            53,
            15,
            17,
            -1021,
            1024,
            -307,
            308,
            "2.2250738585072014e-308",
            "1.7976931348623157e+308",
            "2.2204460492503131e-16",
            "4.9406564584124654e-324",
        ),
        if apple {
            (
                "LDBL",
                "L",
                53,
                15,
                17,
                -1021,
                1024,
                -307,
                308,
                "2.2250738585072014e-308",
                "1.7976931348623157e+308",
                "2.2204460492503131e-16",
                "4.9406564584124654e-324",
            )
        } else if arm_linux || wasm {
            (
                "LDBL",
                "L",
                113,
                33,
                36,
                -16381,
                16384,
                -4931,
                4932,
                "3.36210314311209350626267781732175260e-4932",
                "1.18973149535723176508575932662800702e+4932",
                "1.92592994438723585305597794258492732e-34",
                "6.47517511943802511092443895822764655e-4966",
            )
        } else {
            (
                "LDBL",
                "L",
                64,
                18,
                21,
                -16381,
                16384,
                -4931,
                4932,
                "3.36210314311209350626e-4932",
                "1.18973149535723176502e+4932",
                "1.08420217248550443401e-19",
                "3.64519953188247460253e-4951",
            )
        },
    ] {
        for (name, value) in [
            ("MANT_DIG", mant),
            ("DIG", dig),
            ("DECIMAL_DIG", decimal),
            ("MIN_EXP", min_exp),
            ("MAX_EXP", max_exp),
            ("MIN_10_EXP", min10),
            ("MAX_10_EXP", max10),
        ] {
            let value = if value < 0 {
                format!("({value})")
            } else {
                value.to_string()
            };
            def(&format!("__{prefix}_{name}__"), &value);
        }
        for (name, value) in [
            ("MIN", min),
            ("MAX", max),
            ("NORM_MAX", max),
            ("EPSILON", epsilon),
            ("DENORM_MIN", denorm),
        ] {
            def(&format!("__{prefix}_{name}__"), &format!("{value}{suffix}"));
        }
        for name in ["HAS_DENORM", "HAS_INFINITY", "HAS_QUIET_NAN"] {
            def(&format!("__{prefix}_{name}__"), "1");
        }
    }
    if apple {
        for name in [
            "__APPLE__",
            "__MACH__",
            "__aarch64__",
            "__arm64",
            "__arm64__",
            "__AARCH64EL__",
        ] {
            def(name, "1");
        }
        def("__APPLE_CC__", "6000");
        // A deployment target is a build setting, not a property of the host OS.
        // 11.0 is the first macOS release supporting this target.
        let version = deployment_version(darwin_deployment.unwrap_or("11.0"))?;
        for name in [
            "__ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__",
            "__ENVIRONMENT_OS_VERSION_MIN_REQUIRED__",
        ] {
            def(name, &version.to_string());
        }
    } else if !wasm {
        for name in [
            "__ELF__",
            "__linux",
            "__linux__",
            "__gnu_linux__",
            "__unix",
            "__unix__",
        ] {
            def(name, "1");
        }
    }
    if arm_linux {
        for name in [
            "__aarch64__",
            "__AARCH64EL__",
            "__ARM_64BIT_STATE",
            "__ARM_PCS_AAPCS64",
            "__ARM_ARCH_ISA_A64",
            "__CHAR_UNSIGNED__",
            "__WCHAR_UNSIGNED__",
        ] {
            def(name, if name == "__ARM_NEON_FP" { "0xE" } else { "1" });
        }
        def("__ARM_ARCH", "8");
    } else if !apple && !wasm {
        for name in ["__x86_64", "__x86_64__", "__amd64", "__amd64__"] {
            def(name, "1");
        }
        def("__GCC_ASM_FLAG_OUTPUTS__", "1");
    }
    Ok(out)
}

fn deployment_version(version: &str) -> Result<u32, String> {
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid macOS deployment target".to_string())?;
    if parts.is_empty()
        || parts.len() > 3
        || parts[0] < 11
        || parts.iter().skip(1).any(|&n| n > 99)
        || parts[0] > 999
    {
        return Err(
            "macOS deployment target must be major[.minor[.patch]], with major >= 11".into(),
        );
    }
    Ok(parts[0] * 10000
        + parts.get(1).copied().unwrap_or(0) * 100
        + parts.get(2).copied().unwrap_or(0))
}
