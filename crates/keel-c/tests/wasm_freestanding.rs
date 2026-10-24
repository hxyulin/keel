use keel_c::{Options, PreprocessOptions, compile, predefined_macros, preprocess};
use keel_ir::Target;
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
};
const WASM: Target = Target::Wasm32UnknownUnknown;
fn pp() -> PreprocessOptions {
    PreprocessOptions {
        target: WASM,
        ..Default::default()
    }
}
fn macros(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("#define "))
        .map(|l| {
            let (k, v) = l.split_once(' ').unwrap_or((l, ""));
            (k.into(), v.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .collect()
}
#[test]
fn raw_c_to_ir_to_node_with_embedded_headers() {
    let src = include_str!("fixtures/wasm-freestanding.c");
    let text = preprocess(src, "freestanding.c", &pp()).unwrap();
    let m = compile(
        &text,
        "freestanding.c",
        Options {
            target: WASM,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module_with_layout(&m, keel_ir::DataLayout::WASM32).unwrap();
    let bytes = keel_wasm::emit_wasm32_runtime_module(
        &m,
        &keel_wasm::Wasm32RuntimeOptions {
            memory: keel_wasm::Wasm32MemoryOptions {
                scalar: keel_wasm::Options {
                    exports: ["checksum", "local_array"]
                        .into_iter()
                        .map(|s| keel_wasm::Export {
                            name: s.into(),
                            function: s.into(),
                        })
                        .collect(),
                },
                pages: 1,
                export_name: "memory".into(),
            },
            data_floor: 1024,
            stack_bottom: 32768,
            stack_top: 65536,
        },
    )
    .unwrap();
    let script = r#"const assert=require('assert/strict'); const b=require('fs').readFileSync(0); assert(WebAssembly.validate(b)); const e=new WebAssembly.Instance(new WebAssembly.Module(b)).exports; const a=new Uint8Array(e.memory.buffer); a.fill(0xcc,8191,8225); let want=7; for(let i=0;i<32;i++){a[8192+i]=(i*17+129)&255; want=(Math.imul(want,33)^a[8192+i])>>>0;} for(let i=0;i<16;i++)want=(want^((i^0x5a)<<(i&7)))>>>0; want=(want+97+22)>>>0; for(let i=0;i<100;i++){assert.equal(e.checksum(8192,32)>>>0,want); assert.equal(e.local_array(),14);} assert.equal(a[8191],0xcc); assert.equal(a[8224],0xcc);"#;
    let mut node = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("existing Node");
    node.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = node.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    #[cfg(feature = "profile")]
    {
        let (profiled, _) = keel_c::preprocess_with_stats(src, "freestanding.c", &pp()).unwrap();
        assert_eq!(profiled, text);
    }
}
#[test]
fn clang_declaration_and_macro_oracles() {
    let include = format!("{}/include", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("clang")
        .args([
            "--target=wasm32-unknown-unknown",
            "-std=gnu11",
            "-ffreestanding",
            "-nostdinc",
            "-Werror",
            "-fsyntax-only",
            "-I",
            &include,
        ])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/wasm-freestanding.c"
        ))
        .output()
        .expect("existing Clang");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new("clang")
        .args([
            "--target=wasm32-unknown-unknown",
            "-ffreestanding",
            "-std=c11",
            "-dM",
            "-E",
            "-x",
            "c",
            "/dev/null",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let clang = macros(&String::from_utf8(output.stdout).unwrap());
    let keel = macros(&predefined_macros(WASM, None).unwrap());
    let mut compared = 0;
    for (name, value) in &keel {
        if name.starts_with("__KEEL")
            || name.starts_with("__clang_")
            || name == "__VERSION__"
            || name.starts_with("__STDC_NO_")
        {
            continue;
        }
        if let Some(reference) = clang.get(name) {
            assert_eq!(value, reference, "Wasm macro {name}");
            compared += 1;
        }
    }
    assert!(compared > 250, "only {compared} shared macros compared");
    for name in [
        "__STDC_HOSTED__",
        "__LONG_MAX__",
        "__LONG_WIDTH__",
        "__POINTER_WIDTH__",
        "__SIZEOF_LONG__",
        "__SIZEOF_POINTER__",
        "__SIZEOF_LONG_DOUBLE__",
        "__INT64_TYPE__",
        "__UINT64_TYPE__",
        "__INT64_MAX__",
        "__UINT64_MAX__",
        "__INT64_C(c)",
        "__UINT64_C(c)",
        "__INTMAX_TYPE__",
        "__UINTMAX_TYPE__",
        "__SIZE_TYPE__",
        "__PTRDIFF_TYPE__",
        "__INTPTR_TYPE__",
        "__UINTPTR_TYPE__",
        "__WCHAR_TYPE__",
        "__WINT_TYPE__",
        "__WINT_MAX__",
        "__LDBL_MANT_DIG__",
        "__LDBL_MAX__",
        "__LDBL_EPSILON__",
    ] {
        assert_eq!(keel.get(name), clang.get(name), "{name}");
    }
}
#[test]
fn macros_and_capabilities_do_not_claim_native_or_unimplemented_features() {
    let m = macros(&predefined_macros(WASM, None).unwrap());
    for name in [
        "__LP64__",
        "_LP64",
        "__APPLE__",
        "__MACH__",
        "__linux__",
        "__ELF__",
        "__unix__",
        "__x86_64__",
        "__aarch64__",
        "__CHAR_UNSIGNED__",
        "__WINT_UNSIGNED__",
        "__wasi__",
        "__wasm_simd128__",
        "__wasm_atomics__",
        "__SIZEOF_INT128__",
    ] {
        assert!(!m.contains_key(name), "{name}");
    }
    assert!(
        !m.keys()
            .any(|s| s.contains("_ATOMIC_") && !s.contains("SIG_ATOMIC")
                || s.contains("COMPARE_AND_SWAP"))
    );
    assert_eq!(m["__STDC_NO_ATOMICS__"], "1");
    let text=preprocess("#if __has_builtin(__builtin_ia32_pause) || __has_feature(c_atomic) || __has_attribute(vector_size)\n#error false capability\n#endif\n#if __has_include(<immintrin.h>) || __has_include(<__keel_aes.h>) || __has_include(<stdatomic.h>) || __has_include(<stdio.h>)\n#error false header\n#endif\n#if !__has_include(<stdint.h>)\n#error missing portable header\n#endif\n", "cap.c",&pp()).unwrap();
    assert!(!text.contains("#error"));
}
#[test]
fn explicit_freestanding_search_and_override_policy() {
    for name in [
        "stdio.h",
        "stdlib.h",
        "immintrin.h",
        "xmmintrin.h",
        "__keel_sse2_conversions.h",
        "__keel_aes.h",
        "stdatomic.h",
        "/usr/include/stdio.h",
    ] {
        assert!(
            preprocess(&format!("#include <{name}>"), "isolation.c", &pp()).is_err(),
            "{name}"
        );
    }
    for deployment in [Some("13.0"), Some("garbage")] {
        assert!(predefined_macros(WASM, deployment).is_err());
    }
    let mut opts = pp();
    opts.sysroot = Some("/".into());
    assert!(
        preprocess("", "isolation.c", &opts)
            .unwrap_err()
            .message
            .contains("sysroot")
    );
    let dir = std::env::temp_dir().join(format!("keel-wasm-explicit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("custom.h"), "#define CUSTOM 73\n").unwrap();
    for system in [false, true] {
        let mut opts = pp();
        if system {
            opts.system_include_dirs.push(dir.clone());
        } else {
            opts.include_dirs.push(dir.clone());
        }
        let text = preprocess(
            "#include <custom.h>\nint f(void){return CUSTOM;}",
            "explicit.c",
            &opts,
        )
        .unwrap();
        assert!(text.contains("return 73"));
    }
    let mut opts = pp();
    // Public -D/-U semantics remain ordered; overrides never change the C layout.
    opts.defines = vec![
        ("__LONG_MAX__".into(), Some("17".into())),
        ("__wasm32__".into(), None),
    ];
    let text=preprocess("_Static_assert(__LONG_MAX__==17,\"override\"); _Static_assert(sizeof(long)==4,\"target\");", "override.c",&opts).unwrap();
    compile(
        &text,
        "override.c",
        Options {
            target: WASM,
            debug_info: false,
        },
    )
    .unwrap();
    let mut spoofed = pp();
    spoofed.defines = vec![("__x86_64__".into(), Some("1".into()))];
    let text = preprocess(
        "int f(void) { return __builtin_ia32_pause(); }",
        "spoof.c",
        &spoofed,
    )
    .unwrap();
    assert!(
        compile(
            &text,
            "spoof.c",
            Options {
                target: WASM,
                debug_info: false
            }
        )
        .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ambient_host_paths_cannot_restore_headers() {
    const MARKER: &str = "KEEL_WASM_INCLUDE_CHILD";
    if std::env::var_os(MARKER).is_some() {
        for name in ["ambient-only.h", "stdio.h", "Foundation/Foundation.h"] {
            assert!(
                preprocess(&format!("#include <{name}>"), "ambient.c", &pp()).is_err(),
                "ambient {name}"
            );
        }
        let text=preprocess("#if __has_include(<ambient-only.h>) || __has_include_next(<stdio.h>)\n#error ambient headers leaked\n#endif", "ambient.c", &pp()).unwrap();
        assert!(!text.contains("#error"));
        return;
    }
    let dir = std::env::temp_dir().join(format!("keel-wasm-ambient-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("usr/include")).unwrap();
    std::fs::write(dir.join("ambient-only.h"), "#error leaked\n").unwrap();
    std::fs::write(dir.join("usr/include/stdio.h"), "#error leaked\n").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ambient_host_paths_cannot_restore_headers",
            "--nocapture",
        ])
        .env(MARKER, "1")
        .env("CPATH", &dir)
        .env("C_INCLUDE_PATH", &dir)
        .env("SDKROOT", &dir)
        .env("MACOSX_DEPLOYMENT_TARGET", "13.0")
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
