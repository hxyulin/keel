//! Ordinary C weak linkage, deliberately distinct from ODR coalescing.
use keel_ir::{InstData, Module, Target};
use object::{Object, ObjectSection, ObjectSymbol};

fn module(src: &str, target: Target) -> Module {
    let mut m = keel_c::compile(
        src,
        "weak.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["inline", "inline-noninterposable"]).unwrap();
    keel_opt::optimize_with_tier(&mut m, keel_opt::OptimizationTier::Full).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let text = m.to_string();
    let recovered = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&recovered).unwrap();
    assert_eq!(
        keel_ir::parse(&recovered.to_string()).unwrap().to_string(),
        recovered.to_string()
    );
    recovered
}
fn object(src: &str, target: Target, text: bool) -> Vec<u8> {
    let m = module(src, target);
    if text {
        let asm = keel_codegen::compile(&m, target).unwrap();
        match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::Aarch64AppleDarwin => keel_asm::assemble_object(&asm).unwrap(),
            Target::Aarch64LinuxGnu => keel_asm::assemble_elf_object(&asm).unwrap(),
            Target::X86_64LinuxGnu => keel_asm::x86::assemble_object(&asm).unwrap(),
        }
    } else {
        let p = keel_codegen::compile_program(&m, target).unwrap();
        match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::Aarch64AppleDarwin => {
                keel_asm::assemble_program_object(&p, Default::default()).unwrap()
            }
            Target::Aarch64LinuxGnu => {
                keel_asm::assemble_elf_program_object(&p, Default::default()).unwrap()
            }
            Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(&p).unwrap(),
        }
    }
}
fn definitions(tu: &str) -> String {
    // All binding arrives late, after definitions, initializers and references.
    let f = if tu == "b" { 9 } else { 7 };
    let d = if tu == "b" { 15 } else { 11 };
    let c = if tu == "b" { 17 } else { 13 };
    format!(
        r#"
int f(void) {{ return {f}; }}
int d={d}, z;
const int c={c};
int *p_{tu}=&d;
int (*q_{tu})(void)=f;
int value_{tu}(void) {{ return f()+d+z+c; }}
void *addr_{tu}(void) {{ return &d; }}
void *fn_{tu}(void) {{ return f; }}
int guard_{tu}(void) {{ return f ? 1 : 0; }}
int hf(void) {{ return {f}; }}
int hd={d};
int hidden_{tu}(void) {{ return hf()+hd; }}
void *hidden_addr_{tu}(void) {{ return &hd; }}
void *hidden_fn_{tu}(void) {{ return hf; }}
int hf(void) __attribute__((weak,visibility("hidden")));
extern int hd __attribute__((weak,visibility("hidden")));
int f(void) __attribute__((weak));
extern int d __attribute__((weak));
extern int z __attribute__((weak));
extern const int c __attribute__((weak));
"#
    )
}
const MISSING: &str = r#"
int absent(void);
extern int missing;
int (*optional_function)(void)=absent;
int *optional_data=&missing;
int probe(void) { return absent ? absent() : 0; }
int data_probe(void) { return &missing ? missing : 0; }
int address_probe(void) { return !!&missing; }
_Bool bool_probe(void) { return &missing; }
int absent(void) __attribute__((weak));
extern int missing __attribute__((weak));
"#;
const PROVIDER: &str =
    "int absent(void) { return 31; } int missing=37; int anchor(void) { return 1; }";
const STRONG: &str = "int f(void) { return 17; } int d=19,z=23; const int c=29; __attribute__((visibility(\"hidden\"))) int hf(void) { return 41; } __attribute__((visibility(\"hidden\"))) int hd=43;";

#[test]
fn weak_symbols_and_symbol_relocations_survive_all_emission_routes() {
    for target in Target::NATIVE {
        for src in [definitions("a"), MISSING.into()] {
            let typed = object(&src, target, false);
            assert_eq!(typed, object(&src, target, true));
            let obj = object::File::parse(typed.as_slice()).unwrap();
            let prefix = if target == Target::Aarch64AppleDarwin {
                "_"
            } else {
                ""
            };
            let names: &[&str] = if src == MISSING {
                &["absent", "missing"]
            } else {
                &["f", "d", "z", "c", "hf", "hd"]
            };
            for name in names {
                let symbol = obj
                    .symbols()
                    .find(|s| s.name().unwrap() == format!("{prefix}{name}"))
                    .unwrap();
                assert!(symbol.is_weak(), "{target:?}: {name}: {:?}", symbol.flags());
                assert_eq!(symbol.is_undefined(), src == MISSING);
                // Every code and initializer reference retains symbol binding.
                let references = obj.sections().flat_map(|s| s.relocations()).filter(|(_, r)| {
                    matches!(r.target(), object::RelocationTarget::Symbol(i) if i == symbol.index())
                }).count();
                assert!(references > 0, "{name}");
            }
            let m = module(&src, target);
            if src != MISSING {
                let f = m.functions.iter().find(|f| f.name == "f").unwrap();
                assert!(f.attrs.weak && !f.weak_odr && !f.local);
                let caller = m.functions.iter().find(|f| f.name == "value_a").unwrap();
                assert!(
                    caller
                        .layout_insts()
                        .any(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. }))
                );
            }
        }
    }
}

#[test]
fn unsupported_weak_storage_visibility_and_constant_forms_diagnose() {
    for target in Target::NATIVE {
        let retained = module(
            "__attribute__((weak)) int unused_function(void) { return 1; } __attribute__((weak)) const int unused_constant=3; __attribute__((weak)) int unused_zero;",
            target,
        );
        assert_eq!(retained.functions.len(), 1);
        assert!(retained.functions[0].attrs.weak);
        assert_eq!(retained.data.len(), 2);
        assert!(retained.data.iter().all(|d| d.weak && d.init.is_some()));
        for src in [
            "static int f(void) __attribute__((weak));",
            "static int x __attribute__((weak));",
            "_Thread_local int x __attribute__((weak));",
            "extern _Thread_local int x __attribute__((weak));",
            "int f(void) __attribute__((weak,visibility(\"hidden\")));",
            "extern int x __attribute__((weak,visibility(\"internal\")));",
            "extern int x; int a = !&x; extern int x __attribute__((weak));",
        ] {
            assert!(
                keel_c::compile(
                    src,
                    "weak.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "{src}"
            );
        }
        // Hidden definitions can be coalesced within the linkage unit.
        let bytes = object(
            "__attribute__((weak, visibility(\"hidden\"))) int h(void) { return 1; } __attribute__((weak,visibility(\"hidden\"))) int hd;",
            target,
            false,
        );
        let file = object::File::parse(bytes.as_slice()).unwrap();
        for s in file
            .symbols()
            .filter(|s| s.is_definition() && s.is_global())
        {
            assert!(s.is_weak());
            assert_eq!(s.scope(), object::SymbolScope::Linkage);
        }
    }
}

#[test]
fn x86_elf_links_weak_definitions_and_zero_references_without_a_runtime() {
    let target = Target::X86_64LinuxGnu;
    let a = object(&definitions("a"), target, false);
    let b = object(&definitions("b"), target, true);
    let strong = object(STRONG, target, false);
    let missing = object(MISSING, target, false);
    let start = keel_asm::x86::assemble_object(".text\n.globl _start\n_start:\nret\n").unwrap();
    for rest in [vec![&a, &b], vec![&a, &strong], vec![&strong, &a]] {
        let mut bytes = vec![&start, &missing];
        bytes.extend(rest);
        let inputs = bytes
            .iter()
            .map(|b| keel_link::Input {
                name: "weak.o",
                bytes: b,
            })
            .collect::<Vec<_>>();
        let exe = keel_link::elf::link(&inputs, Default::default()).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let addr = |name| {
            file.symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap()
                .address()
        };
        let read = |at| {
            let offset = (at - 0x400000) as usize;
            u64::from_le_bytes(exe.bytes[offset..offset + 8].try_into().unwrap())
        };
        assert_eq!(read(addr("p_a")), addr("d"));
        assert_eq!(read(addr("q_a")), addr("f"));
        assert_eq!(read(addr("optional_data")), 0);
        assert_eq!(read(addr("optional_function")), 0);
        assert!(file.section_by_name(".got").unwrap().size() >= 16);
        if bytes.contains(&&strong) {
            let at = (addr("d") - 0x400000) as usize;
            assert_eq!(
                u32::from_le_bytes(exe.bytes[at..at + 4].try_into().unwrap()),
                19
            );
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn arm64_system_and_keel_runtime_overrides_nulls_identity_and_archive_demand() {
    use std::{fs, process::Command};
    fn run(cmd: &mut Command) {
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{cmd:?}: {:?}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let dir = std::env::temp_dir().join(format!("keel-c-weak-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let target = Target::Aarch64AppleDarwin;
    // Clang is both a symbol/relocation oracle and a mixed-object producer.
    for (name, src) in [
        ("a", definitions("a")),
        ("b", definitions("b")),
        ("strong", STRONG.into()),
        ("missing", MISSING.into()),
        ("provider", PROVIDER.into()),
    ] {
        // Clang ignores weak added after a definition, and may fold a prior
        // reference's null guard. Compare the equivalent early declaration;
        // Keel deliberately merges binding over the complete translation unit.
        let prelude = if name == "a" || name == "b" {
            "int f(void) __attribute__((weak)); extern int d __attribute__((weak)); extern int z __attribute__((weak)); extern const int c __attribute__((weak)); int hf(void) __attribute__((weak,visibility(\"hidden\"))); extern int hd __attribute__((weak,visibility(\"hidden\")));"
        } else if name == "missing" {
            "int absent(void) __attribute__((weak)); extern int missing __attribute__((weak));"
        } else {
            ""
        };
        fs::write(dir.join(format!("{name}.c")), format!("{prelude}\n{src}")).unwrap();
        run(Command::new("clang")
            .arg("-c")
            .arg("-O2")
            .arg(dir.join(format!("{name}.c")))
            .arg("-o")
            .arg(dir.join(format!("clang-{name}.o"))));
        for text in [false, true] {
            fs::write(
                dir.join(format!("{text}-{name}.o")),
                object(&src, target, text),
            )
            .unwrap();
        }
        let clang = fs::read(dir.join(format!("clang-{name}.o"))).unwrap();
        let native = object(&src, target, false);
        let c = object::File::parse(clang.as_slice()).unwrap();
        let k = object::File::parse(native.as_slice()).unwrap();
        for s in k.symbols().filter(|s| s.is_weak()) {
            let cs = c
                .symbols()
                .find(|cs| cs.name().unwrap() == s.name().unwrap())
                .unwrap();
            assert_eq!(cs.flags(), s.flags());
            assert_eq!(cs.is_undefined(), s.is_undefined());
            for file in [&c, &k] {
                let symbol = file
                    .symbols()
                    .find(|cs| cs.name().unwrap() == s.name().unwrap())
                    .unwrap();
                assert!(file.sections().flat_map(|s| s.relocations()).any(|(_,r)| matches!(r.target(), object::RelocationTarget::Symbol(i) if i == symbol.index())));
            }
        }
    }
    for route in ["false", "true", "clang"] {
        let archive = dir.join(format!("{route}.a"));
        run(Command::new("ar")
            .arg("rcs")
            .arg(&archive)
            .arg(dir.join(format!("{route}-provider.o"))));
        for (case, extra, value, present, demand) in [
            ("two", vec!["b"], 31, false, false),
            ("override", vec!["strong"], 88, false, false),
            ("reverse", vec!["strong"], 88, false, false),
            ("provided", vec!["provider"], 31, true, false),
            ("archive-weak", vec!["archive"], 31, true, false),
            ("archive-demand", vec!["archive"], 31, true, true),
        ] {
            let src = format!(
                r#"
extern int f(void),d,z; extern const int c;
extern int value_a(void),guard_a(void); extern void *addr_a(void),*fn_a(void);
extern int *p_a; extern int (*q_a)(void);
extern int hidden_a(void),hd,hf(void); extern void *hidden_addr_a(void),*hidden_fn_a(void);
extern int probe(void),data_probe(void),address_probe(void); extern _Bool bool_probe(void);
extern int (*optional_function)(void); extern int *optional_data;
{anchor_decl}
{second_decl}
int main(void) {{
 if (value_a()!={value} || !guard_a() || addr_a()!=&d || fn_a()!=(void*)f || p_a!=&d || q_a!=f) return 1;
 if (hidden_a()!={hidden_value} || hidden_addr_a()!=&hd || hidden_fn_a()!=(void*)hf) return 6;
 if (probe()!={p} || data_probe()!={data} || address_probe()!={yes} || bool_probe()!={yes}) return 2;
 if (!!optional_function!={yes} || !!optional_data!={yes}) return 3;
 {anchor_call}
 {second_check}
 return 0;
}}
"#,
                hidden_value = if value == 88 { 84 } else { 18 },
                p = if present { 31 } else { 0 },
                data = if present { 37 } else { 0 },
                yes = u8::from(present),
                second_decl = if case == "two" {
                    "extern int hidden_b(void); extern void *hidden_addr_b(void),*hidden_fn_b(void); extern int value_b(void),guard_b(void); extern void *addr_b(void),*fn_b(void); extern int *p_b; extern int (*q_b)(void);"
                } else {
                    ""
                },
                second_check = if case == "two" {
                    "if (hidden_b()!=18 || hidden_addr_b()!=hidden_addr_a() || hidden_fn_b()!=hidden_fn_a() || value_b()!=31 || !guard_b() || addr_b()!=addr_a() || fn_b()!=fn_a() || p_b!=p_a || q_b!=q_a) return 5;"
                } else {
                    ""
                },
                anchor_decl = if demand {
                    "extern int anchor(void);"
                } else {
                    ""
                },
                anchor_call = if demand {
                    "if(anchor()!=1) return 4;"
                } else {
                    ""
                }
            );
            let main = dir.join("main.o");
            fs::write(&main, object(&src, target, false)).unwrap();
            let mut paths = vec![
                main,
                dir.join(format!("{route}-missing.o")),
                dir.join(format!("{route}-a.o")),
            ];
            let mut additional = extra
                .iter()
                .map(|name| {
                    if *name == "archive" {
                        archive.clone()
                    } else {
                        dir.join(format!("{route}-{name}.o"))
                    }
                })
                .collect::<Vec<_>>();
            if case == "reverse" {
                additional.extend(paths);
                paths = additional;
            } else {
                paths.extend(additional);
            }
            // Exercise mixed Clang/Keel input as well as each assembly route.
            for linker in ["system", "keel"] {
                let exe = dir.join(format!("{route}-{case}-{linker}"));
                if linker == "system" {
                    let mut cmd = Command::new("clang");
                    // Apple ld requires explicit allowance even for N_WEAK_REF.
                    if !present {
                        cmd.arg("-Wl,-U,_absent,-U,_missing");
                    }
                    run(cmd.args(&paths).arg("-o").arg(&exe));
                } else {
                    let contents = paths
                        .iter()
                        .map(|p| fs::read(p).unwrap())
                        .collect::<Vec<_>>();
                    let inputs = contents
                        .iter()
                        .map(|b| keel_link::Input {
                            name: "fixture",
                            bytes: b,
                        })
                        .collect::<Vec<_>>();
                    let opts = keel_link::Options {
                        dylibs: vec![keel_link::Dylib {
                            install_name: "/usr/lib/libSystem.B.dylib".into(),
                            exports: Default::default(),
                            weak_exports: Default::default(),
                        }],
                        ..Default::default()
                    };
                    keel_link::link_signed(&inputs, opts, &exe).unwrap();
                }
                run(&mut Command::new(&exe));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn elf_archive_members_require_strong_demand() {
    let target = Target::X86_64LinuxGnu;
    let provider = object(PROVIDER, target, false);
    let mut archive = b"!<arch>\n".to_vec();
    archive.extend(
        format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            "provider.o/",
            0,
            0,
            0,
            "100644",
            provider.len()
        )
        .bytes(),
    );
    archive.extend(&provider);
    if provider.len() % 2 != 0 {
        archive.push(b'\n');
    }
    let missing = object(MISSING, target, false);
    for demand in [false, true] {
        let start = keel_asm::x86::assemble_object(if demand {
            ".text\n.globl _start\n_start:\ncall anchor\nret\n"
        } else {
            ".text\n.globl _start\n_start:\nret\n"
        })
        .unwrap();
        let inputs = [
            keel_link::Input {
                name: "start.o",
                bytes: &start,
            },
            keel_link::Input {
                name: "missing.o",
                bytes: &missing,
            },
            keel_link::Input {
                name: "provider.a",
                bytes: &archive,
            },
        ];
        let exe = keel_link::elf::link(&inputs, Default::default()).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        assert_eq!(
            file.symbols()
                .any(|s| s.name().unwrap() == "anchor" && s.is_definition()),
            demand
        );
        let ptr = file
            .symbols()
            .find(|s| s.name().unwrap() == "optional_data")
            .unwrap()
            .address();
        let at = (ptr - 0x400000) as usize;
        let value = u64::from_le_bytes(exe.bytes[at..at + 8].try_into().unwrap());
        assert_eq!(value != 0, demand);
    }
}

#[cfg(unix)]
#[test]
fn clang_x86_objects_agree_on_weak_binding_and_symbolic_relocations() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-weak-elf-oracle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (name, src, prelude) in [
        (
            "defined",
            definitions("a"),
            "int f(void) __attribute__((weak)); extern int d __attribute__((weak)); extern int z __attribute__((weak)); extern const int c __attribute__((weak)); int hf(void) __attribute__((weak,visibility(\"hidden\"))); extern int hd __attribute__((weak,visibility(\"hidden\")));",
        ),
        (
            "undefined",
            MISSING.into(),
            "int absent(void) __attribute__((weak)); extern int missing __attribute__((weak));",
        ),
    ] {
        let source = dir.join(format!("{name}.c"));
        let output = dir.join(format!("{name}.o"));
        fs::write(&source, format!("{prelude}\n{src}")).unwrap();
        let out = Command::new("clang")
            .args(["--target=x86_64-unknown-linux-gnu", "-O2", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let clang = fs::read(output).unwrap();
        let keel = object(&src, Target::X86_64LinuxGnu, false);
        let c = object::File::parse(clang.as_slice()).unwrap();
        let k = object::File::parse(keel.as_slice()).unwrap();
        for s in k.symbols().filter(|s| s.is_weak()) {
            let cs = c
                .symbols()
                .find(|cs| cs.name().unwrap() == s.name().unwrap())
                .unwrap();
            assert!(cs.is_weak());
            assert_eq!(cs.is_undefined(), s.is_undefined());
            assert_eq!(cs.scope(), s.scope());
            for (file, sym) in [(&c, cs.index()), (&k, s.index())] {
                let relocs = file
                    .sections()
                    .flat_map(|s| s.relocations())
                    .filter(
                        |(_, r)| matches!(r.target(),object::RelocationTarget::Symbol(i) if i==sym),
                    )
                    .collect::<Vec<_>>();
                assert!(!relocs.is_empty(), "{}", s.name().unwrap());
                assert!(
                    relocs.iter().all(|(_, r)| matches!(r.size(), 32 | 64)
                    // object 0.36 does not decode GOTPCRELX widths.
                    || matches!(r.flags(), object::RelocationFlags::Elf { r_type: 41 | 42 })),
                    "{relocs:?}"
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn static_address_truth_uses_final_strong_binding_like_clang() {
    use std::{fs, process::Command};
    let src = r#"
extern int external;
int function(void);
static int local;
int result[] = { !&external, !!&external, (_Bool)&external,
                 function ? 7 : 9, &local && 3, &external || 0,
                 __builtin_expect(!!&external, 1) };
int external;
int function(void) { return 1; }
"#;
    let dir = std::env::temp_dir().join(format!("keel-strong-truth-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("truth.c"), src).unwrap();
    for target in Target::NATIVE {
        let triple = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::Aarch64AppleDarwin => "arm64-apple-macos11",
            Target::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
            Target::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
        };
        let output = dir.join(format!("{triple}.o"));
        let out = Command::new("clang")
            .args(["-target", triple, "-Wno-pointer-bool-conversion", "-c"])
            .arg(dir.join("truth.c"))
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let clang = fs::read(output).unwrap();
        let keel = object(src, target, false);
        assert_eq!(keel, object(src, target, true));
        let result_bytes = |bytes: &[u8]| {
            let file = object::File::parse(bytes).unwrap();
            let symbol = file
                .symbols()
                .find(|s| matches!(s.name(), Ok("result" | "_result")))
                .unwrap();
            let section = file
                .section_by_index(symbol.section_index().unwrap())
                .unwrap();
            section
                .data_range(symbol.address(), 28)
                .unwrap()
                .unwrap()
                .to_vec()
        };
        assert_eq!(result_bytes(&keel), result_bytes(&clang));
        assert_eq!(
            result_bytes(&keel),
            [0i32, 1, 1, 7, 1, 1, 1]
                .into_iter()
                .flat_map(i32::to_le_bytes)
                .collect::<Vec<_>>()
        );
        // A declaration after the initializer still invalidates the truth proof.
        for address in [
            "!&external",
            "!!&external",
            "(_Bool)&external",
            "function ? 7 : 9",
            "&external && 1",
            "&external || 0",
            "__builtin_expect(!!&external, 1)",
        ] {
            let source = format!(
                "extern int external; int function(void); int result={address}; extern int external __attribute__((weak)); int function(void) __attribute__((weak));"
            );
            assert!(
                keel_c::compile(
                    &source,
                    "truth.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "{source}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
