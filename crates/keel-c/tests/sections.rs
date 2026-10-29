use keel_ir::{Module, Target};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{fs, path::Path, process::Command};

fn module(src: &str, target: Target, debug_info: bool, optimize: bool) -> Module {
    let mut m = keel_c::compile(src, "sections.c", keel_c::Options { target, debug_info }).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if optimize {
        keel_opt::optimize(&mut m).unwrap();
    }
    keel_ir::verify_module(&m).unwrap();
    let text = m.to_string();
    let m = keel_ir::parse(&text).unwrap();
    let canonical = m.to_string();
    assert_eq!(keel_ir::parse(&canonical).unwrap().to_string(), canonical);
    keel_ir::verify_module(&m).unwrap();
    m
}
fn object(m: &Module, target: Target, text: bool) -> Vec<u8> {
    if text {
        let asm = keel_codegen::compile(m, target).unwrap();
        match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::Aarch64AppleDarwin => keel_asm::assemble_object(&asm).unwrap(),
            Target::Aarch64LinuxGnu => keel_asm::assemble_elf_object(&asm).unwrap(),
            Target::X86_64LinuxGnu => keel_asm::x86::assemble_object(&asm).unwrap(),
        }
    } else {
        let p = keel_codegen::compile_program(m, target).unwrap();
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
/// Keel's text and direct routes must give the same object, so tests that
/// link objects link only this one.
fn checked_object(m: &Module, target: Target) -> Vec<u8> {
    let bytes = object(m, target, false);
    assert!(
        object(m, target, true) == bytes,
        "text and direct objects differ"
    );
    bytes
}
fn names(target: Target) -> [&'static str; 4] {
    match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::Aarch64AppleDarwin => [
            "__TEXT,__custom_code",
            "__DATA,__custom_data",
            "__TEXT,__custom_ro",
            "__DATA,__custom_zero",
        ],
        Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => {
            [".custom_code", ".custom_data", ".custom_ro", ".custom_zero"]
        }
    }
}
fn source(target: Target) -> String {
    let [code, data, ro, zero] = names(target);
    format!(
        r#"
int datum __attribute__((section("{data}"),aligned(32))) = 19;
const int readonly __attribute__((section("{ro}"))) = 23;
int zeros[4] __attribute__((section("{zero}"),aligned(64)));
static int kept_data __attribute__((retain)) = 41;
static int kept_zero __attribute__((retain));
static const int kept_ro __attribute__((retain)) = 43;
static int kept_fn(void) {{ return 47; }}
static int kept_fn(void) __attribute__((__retain__));
int placed(void) {{ zeros[1] = 3; return datum + readonly + zeros[1]; }}
int placed(void) __attribute__((section("{code}")));
int main(void) {{ return placed() != 45; }}
"#
    )
}
#[test]
fn section_roundtrip_optimized_direct_and_text_objects() {
    for target in Target::NATIVE {
        for debug in [false, true] {
            for optimize in [false, true] {
                let m = module(&source(target), target, debug, optimize);
                let kept = m.functions.iter().find(|f| f.name == "kept_fn").unwrap();
                assert!(kept.attrs.retain);
                assert_eq!(kept.sig.cc, keel_ir::CallConv::C);
                let bytes = object(&m, target, false);
                assert_eq!(bytes, object(&m, target, true));
                let o = object::File::parse(bytes.as_slice()).unwrap();
                for (i, name) in names(target).iter().enumerate() {
                    let short = name.split_once(',').map_or(*name, |(_, s)| s);
                    let s = o.section_by_name(short).unwrap();
                    assert!(s.size() > 0);
                    if i == 1 {
                        assert!(s.align() >= 32);
                    }
                    if i == 3 {
                        assert!(s.align() >= 64);
                    }
                    if target != Target::Aarch64AppleDarwin {
                        let expected = [6, 3, 2, 3][i];
                        assert_eq!(s.flags(), object::SectionFlags::Elf { sh_flags: expected });
                        assert_eq!(s.kind() == object::SectionKind::UninitializedData, i == 3);
                    } else if i == 0 {
                        assert!(
                            matches!(s.flags(), object::SectionFlags::MachO { flags } if flags & 0x80000000 != 0)
                        );
                    }
                }
                for name in ["kept_data", "kept_zero", "kept_ro", "kept_fn"] {
                    let symbol_name = if target == Target::Aarch64AppleDarwin {
                        format!("_{name}")
                    } else {
                        name.into()
                    };
                    let sym = o
                        .symbols()
                        .find(|s| s.name().unwrap() == symbol_name)
                        .unwrap();
                    if target != Target::Aarch64AppleDarwin {
                        let section = o.section_by_index(sym.section_index().unwrap()).unwrap();
                        assert!(
                            matches!(section.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 0x200000 != 0)
                        );
                    } else {
                        assert!(
                            matches!(sym.flags(), object::SymbolFlags::MachO { n_desc } if n_desc & 0x20 != 0)
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn placement_and_retention_diagnose_before_pruning() {
    for target in Target::NATIVE {
        let [code, data, _, _] = names(target);
        let cases = [
            "int x __attribute__((retain(1)));".into(),
            "int x __attribute__((section(1)));".into(),
            "int x __attribute__((section(\"\")));".into(),
            "int x __attribute__((section(\"bad\\nname\")));".into(),
            "int x __attribute__((section(L\"wide\")));".into(),
            "extern int x __attribute__((retain));".into(),
            format!("extern int x __attribute__((section(\"{data}\"), retain));"),
            "void f(void) { int x __attribute__((retain)); }".into(),
            format!("void f(void) {{ int x __attribute__((section(\"{data}\"))); }}"),
            format!("typedef int I __attribute__((section(\"{data}\")));"),
            format!("struct S {{ int x __attribute__((section(\"{data}\"))); }};"),
            format!("void f(int x __attribute__((section(\"{data}\")))) {{}}"),
            format!(
                "static void f(void) __attribute__((section(\"{data}\"))); static void f(void) {{}} static int x __attribute__((section(\"{data}\")));"
            ),
            format!(
                "int x __attribute__((section(\"{data}\"))); int x __attribute__((section(\"{code}\")));"
            ),
        ];
        for src in cases {
            assert!(
                keel_c::compile(
                    &src,
                    "bad.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "accepted {target:?}: {src}"
            );
        }
        let bad_names: &[&str] = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => &[
                ".init_array",
                ".hash",
                ".relr.dyn",
                ".note.evil",
                ".tdata",
                ".tbss",
                ".gnu.linkonce.d.x",
                ".gnu_debuglink",
                ".gnu_debugaltlink",
                ".gnu_debugdata",
                ".eh_frame",
                ".debug_info",
                ".data,aw",
                ".rodata",
            ],
            Target::Aarch64AppleDarwin => &[
                "__DATA",
                "__DATA,__thread_data",
                "__DATA,__mod_init_func",
                "__TEXT,__custom_data",
                "__DATA,__bss",
                "__BAD,__data",
                "__DATA,__123456789012345",
                "__DATA,__data,regular",
            ],
        };
        for name in bad_names {
            let src = format!("static int x __attribute__((section(\"{name}\"))) = 1;");
            assert!(
                keel_c::compile(
                    &src,
                    "bad.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "accepted {src}"
            );
        }
        let src = format!(
            "static int x __attribute__((section(\"{data}\"))) = 1; static int y __attribute__((section(\"{data}\"))) = 0;"
        );
        assert!(
            keel_c::compile(
                &src,
                "bad.c",
                keel_c::Options {
                    target,
                    debug_info: false
                }
            )
            .is_err()
        );
    }
}
fn run(c: &mut Command) -> std::process::Output {
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "{c:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    o
}
fn symbol_names(bytes: &[u8]) -> Vec<String> {
    object::File::parse(bytes)
        .unwrap()
        .symbols()
        .filter_map(|s| s.name().ok().map(str::to_owned))
        .collect()
}
#[test]
fn elf_system_gc_and_native_acceptance() {
    let ld = std::env::var("KEEL_TEST_GNU_LD").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/x86_64-elf-ld".into()
        } else {
            "ld".into()
        }
    });
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sections-elf");
    fs::create_dir_all(&dir).unwrap();
    let src = r#"
static int retained_data __attribute__((retain)) = 17;
static int retained_zero __attribute__((retain));
static const int retained_ro __attribute__((retain)) = 19;
static int retained_fn(void) __attribute__((retain,section(".text")));
static int retained_fn(void) { return 23; }
static int used_data __attribute__((used,section(".used_data"))) = 29;
static int used_fn(void) __attribute__((used,section(".used_code")));
static int used_fn(void) { return 31; }
static int used_default(void) __attribute__((used));
static int used_default(void) { return 37; }
void _start(void) __attribute__((section(".entry")));
void _start(void) {}
"#;
    for debug in [false, true] {
        for text in [false, true] {
            let bytes = object(
                &module(src, Target::X86_64LinuxGnu, debug, true),
                Target::X86_64LinuxGnu,
                text,
            );
            let input = dir.join("input.o");
            fs::write(&input, &bytes).unwrap();
            let exe = dir.join("gc");
            run(Command::new(&ld)
                .arg("--gc-sections")
                .arg(&input)
                .arg("-o")
                .arg(&exe));
            let output = fs::read(&exe).unwrap();
            let symbols = symbol_names(&output);
            for name in [
                "retained_data",
                "retained_zero",
                "retained_ro",
                "retained_fn",
            ] {
                assert!(symbols.iter().any(|s| s == name), "missing {name}");
            }
            for name in ["used_data", "used_fn", "used_default"] {
                assert!(!symbols.iter().any(|s| s == name), "kept {name}");
            }
            let native = keel_link::elf::link(
                &[keel_link::Input {
                    name: "sections.o",
                    bytes: &bytes,
                }],
                Default::default(),
            )
            .unwrap();
            let file = object::File::parse(native.bytes.as_slice()).unwrap();
            assert!(file.sections().any(|s| matches!(s.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 0x200000 != 0)));
            assert!(symbol_names(&native.bytes).iter().any(|s| s == "used_fn")); // native linker currently keeps all included sections
        }
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn arm64_custom_sections_execute_with_system_and_native_linkers() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sections-arm64");
    fs::create_dir_all(&dir).unwrap();
    let sdk = String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout).unwrap();
    let sdk = Path::new(sdk.trim());
    let libc = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    for debug in [false, true] {
        for optimize in [false, true] {
            let bytes = checked_object(
                &module(
                    &source(Target::Aarch64AppleDarwin),
                    Target::Aarch64AppleDarwin,
                    debug,
                    optimize,
                ),
                Target::Aarch64AppleDarwin,
            );
            let input = dir.join("input.o");
            fs::write(&input, &bytes).unwrap();
            for native in [false, true] {
                let exe = dir.join("run");
                if native {
                    keel_link::link_signed(
                        &[keel_link::Input {
                            name: "input.o",
                            bytes: &bytes,
                        }],
                        keel_link::Options {
                            dylibs: vec![libc.clone()],
                            ..Default::default()
                        },
                        &exe,
                    )
                    .unwrap();
                } else {
                    run(Command::new("clang")
                        .arg(&input)
                        .arg("-Wl,-dead_strip")
                        .arg("-o")
                        .arg(&exe));
                }
                run(&mut Command::new(&exe));
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn arm64_dead_strip_weak_identity_and_archive_extraction() {
    let target = Target::Aarch64AppleDarwin;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sections-arm64-retain");
    fs::create_dir_all(&dir).unwrap();
    let sdk = String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout).unwrap();
    let sdk = Path::new(sdk.trim());
    let libc = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    let src = r#"
static int retained_data __attribute__((retain,section("__DATA,__retained"))) = 11;
static int retained_zero __attribute__((retain));
static const int retained_ro __attribute__((retain)) = 13;
static int retained_fn(void) __attribute__((retain,section("__TEXT,__retained")));
static int retained_fn(void) { return 17; }
static int used_data __attribute__((used,section("__DATA,__used"))) = 19;
static int used_fn(void) __attribute__((used,section("__TEXT,__used")));
static int used_fn(void) { return 23; }
int weak_data __attribute__((weak,retain)) = 31;
int weak_fn(void) __attribute__((weak,retain));
int weak_fn(void) { return 37; }
int anchor(void) { return weak_data + weak_fn(); }
"#;
    let strong_c = dir.join("strong.c");
    fs::write(
        &strong_c,
        "int weak_data = 41; int weak_fn(void) { return 43; }",
    )
    .unwrap();
    let strong_o = dir.join("strong.o");
    run(Command::new("clang")
        .arg("-c")
        .arg(&strong_c)
        .arg("-o")
        .arg(&strong_o));
    let strong_bytes = fs::read(&strong_o).unwrap();
    let bytes = checked_object(&module(src, target, true, true), target);
    let file = object::File::parse(bytes.as_slice()).unwrap();
    for name in ["_weak_data", "_weak_fn"] {
        let sym = file.symbols().find(|s| s.name().unwrap() == name).unwrap();
        assert!(
            matches!(sym.flags(), object::SymbolFlags::MachO { n_desc } if n_desc & 0xa0 == 0xa0)
        );
        assert!(sym.is_global());
    }
    let input = dir.join("retained.o");
    fs::write(&input, &bytes).unwrap();
    let archive = dir.join("retained.a");
    let _ = fs::remove_file(&archive);
    run(Command::new("ar").arg("rcs").arg(&archive).arg(&input));
    let archive_bytes = fs::read(&archive).unwrap();
    for demanded in [false, true] {
        let main_src = if demanded {
            "int anchor(void); int main(void) { return anchor() != 84; }"
        } else {
            "int main(void) { return 0; }"
        };
        let main_bytes = checked_object(&module(main_src, target, false, true), target);
        let main_o = dir.join("main.o");
        fs::write(&main_o, &main_bytes).unwrap();
        for archived in [false, true] {
            for reverse in [false, true] {
                for native in [false, true] {
                    let exe = dir.join("run");
                    let member_path = if archived { &archive } else { &input };
                    let member_bytes = if archived { &archive_bytes } else { &bytes };
                    if native {
                        let mut inputs = vec![keel_link::Input {
                            name: "main.o",
                            bytes: &main_bytes,
                        }];
                        let parts = if reverse {
                            [(&strong_bytes, "strong.o"), (member_bytes, "member")]
                        } else {
                            [(member_bytes, "member"), (&strong_bytes, "strong.o")]
                        };
                        inputs.extend(
                            parts
                                .into_iter()
                                .map(|(bytes, name)| keel_link::Input { name, bytes }),
                        );
                        keel_link::link_signed(
                            &inputs,
                            keel_link::Options {
                                dylibs: vec![libc.clone()],
                                ..Default::default()
                            },
                            &exe,
                        )
                        .unwrap();
                    } else {
                        let mut c = Command::new("clang");
                        c.arg(&main_o);
                        if reverse {
                            c.arg(&strong_o).arg(member_path);
                        } else {
                            c.arg(member_path).arg(&strong_o);
                        }
                        run(c.arg("-Wl,-dead_strip").arg("-o").arg(&exe));
                    }
                    run(&mut Command::new(&exe));
                    if !native {
                        let symbols = symbol_names(&fs::read(&exe).unwrap());
                        for name in [
                            "_retained_data",
                            "_retained_zero",
                            "_retained_ro",
                            "_retained_fn",
                        ] {
                            assert_eq!(
                                symbols.iter().any(|s| s == name),
                                !archived || demanded,
                                "{name}: archived={archived} demanded={demanded}"
                            );
                        }
                        for name in ["_used_data", "_used_fn"] {
                            assert!(!symbols.iter().any(|s| s == name), "{name}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn elf_weak_identity_and_archive_extraction_with_gc() {
    let target = Target::X86_64LinuxGnu;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sections-elf-archive");
    fs::create_dir_all(&dir).unwrap();
    let ld = std::env::var("KEEL_TEST_GNU_LD").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/x86_64-elf-ld".into()
        } else {
            "ld".into()
        }
    });
    let member_src = "int weak_data __attribute__((weak,retain)) = 31; int weak_fn(void) __attribute__((weak,retain)); int weak_fn(void) {return 37;} static int archived_root __attribute__((retain)) = 17; int anchor(void) {return weak_data + weak_fn();}";
    let member = object(&module(member_src, target, false, true), target, false);
    let o = object::File::parse(member.as_slice()).unwrap();
    for name in ["weak_data", "weak_fn"] {
        assert!(
            o.symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap()
                .is_weak()
        );
    }
    let member_o = dir.join("member.o");
    fs::write(&member_o, &member).unwrap();
    let archive = dir.join("member.a");
    let _ = fs::remove_file(&archive);
    let ar = std::env::var("KEEL_TEST_GNU_AR").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/x86_64-elf-ar".into()
        } else {
            "ar".into()
        }
    });
    run(Command::new(ar).arg("rcs").arg(&archive).arg(&member_o));
    let archive_bytes = fs::read(&archive).unwrap();
    let strong = object(
        &module(
            "int weak_data = 41; int weak_fn(void) {return 43;}",
            target,
            false,
            true,
        ),
        target,
        true,
    );
    let strong_o = dir.join("strong.o");
    fs::write(&strong_o, &strong).unwrap();
    for demanded in [false, true] {
        let start = object(
            &module(
                if demanded {
                    "int anchor(void); int result; void _start(void) {result=anchor();}"
                } else {
                    "void _start(void) {}"
                },
                target,
                false,
                true,
            ),
            target,
            false,
        );
        let start_o = dir.join("start.o");
        fs::write(&start_o, &start).unwrap();
        for reverse in [false, true] {
            let exe = dir.join("gc");
            let mut c = Command::new(&ld);
            c.arg("--gc-sections").arg(&start_o);
            if reverse {
                c.arg(&strong_o).arg(&archive);
            } else {
                c.arg(&archive).arg(&strong_o);
            }
            run(c.arg("-o").arg(&exe));
            let bytes = fs::read(&exe).unwrap();
            let names = symbol_names(&bytes);
            assert_eq!(names.iter().any(|s| s == "archived_root"), demanded);
            if demanded {
                let file = object::File::parse(bytes.as_slice()).unwrap();
                let weak = file
                    .symbols()
                    .find(|s| s.name().unwrap() == "weak_data")
                    .unwrap();
                assert!(!weak.is_weak());
                let section = file
                    .section_by_index(weak.section_index().unwrap())
                    .unwrap();
                let offset = (weak.address() - section.address()) as usize;
                assert_eq!(
                    &section.data().unwrap()[offset..offset + 4],
                    &41i32.to_le_bytes()
                );
            }
            let mut inputs = vec![keel_link::Input {
                name: "start.o",
                bytes: &start,
            }];
            if reverse {
                inputs.extend([
                    keel_link::Input {
                        name: "strong.o",
                        bytes: &strong,
                    },
                    keel_link::Input {
                        name: "archive",
                        bytes: &archive_bytes,
                    },
                ]);
            } else {
                inputs.extend([
                    keel_link::Input {
                        name: "archive",
                        bytes: &archive_bytes,
                    },
                    keel_link::Input {
                        name: "strong.o",
                        bytes: &strong,
                    },
                ]);
            }
            let native = keel_link::elf::link(&inputs, Default::default()).unwrap();
            // Native links preserve every included section, but archive selection
            // still follows symbol demand.
            assert_eq!(
                object::File::parse(native.bytes.as_slice())
                    .unwrap()
                    .sections()
                    .any(|s| s.name().unwrap().contains("archived_root")),
                demanded
            );
        }
    }
}

#[test]
fn placement_alone_is_not_an_emission_root_and_static_locals_retain() {
    for target in Target::NATIVE {
        let [code, data, _, _] = names(target);
        let src = format!(
            r#"
static int pruned_data __attribute__((section("{data}"))) = 7;
static int pruned_function(void) __attribute__((section("{code}")));
static int pruned_function(void) {{ return 9; }}
static int uncalled(void) {{ static int local __attribute__((retain,section("{data}"))) = 11; return local; }}
int main(void) {{ return 0; }}
"#
        );
        // Give the pruned object a distinct name: same-section mixed retention
        // requests intentionally diagnose even when one definition is discarded.
        let src = src.replace(
            &format!("pruned_data __attribute__((section(\"{data}\")))"),
            &format!(
                "pruned_data __attribute__((section(\"{}\")))",
                if target != Target::Aarch64AppleDarwin {
                    ".pruned_data"
                } else {
                    "__DATA,__pruned_data"
                }
            ),
        );
        let m = module(&src, target, false, true);
        assert!(
            !m.functions
                .iter()
                .any(|f| f.name == "pruned_function" || f.name == "uncalled")
        );
        assert_eq!(m.data.len(), 1);
        assert!(m.data[0].retain);
        assert!(m.data[0].section.is_some());
        for text in [false, true] {
            object(&m, target, text);
        }
    }
}

#[test]
fn retained_relocations_keep_bound_weak_symbol_identity() {
    for target in Target::NATIVE {
        let section = if target != Target::Aarch64AppleDarwin {
            ".data.rel.ro.custom"
        } else {
            "__DATA,__ptrs"
        };
        let src = format!(
            r#"
int weak_data __attribute__((weak)) = 13;
int weak_fn(void) __attribute__((weak));
int weak_fn(void) {{ return 17; }}
static int *const data_pointer __attribute__((retain,section("{section}"))) = &weak_data;
static int (*const code_pointer)(void) __attribute__((retain,section("{section}"))) = weak_fn;
int main(void) {{ return 0; }}
"#
        );
        for optimize in [false, true] {
            let m = module(&src, target, true, optimize);
            for text in [false, true] {
                let bytes = object(&m, target, text);
                let o = object::File::parse(bytes.as_slice()).unwrap();
                let name = section.split_once(',').map_or(section, |(_, s)| s);
                let s = o.section_by_name(name).unwrap();
                assert_eq!(s.relocations().count(), 2);
                let mut names = Vec::new();
                for (_, r) in s.relocations() {
                    let object::RelocationTarget::Symbol(index) = r.target() else {
                        panic!("expected bound symbol relocation");
                    };
                    let sym = o.symbol_by_index(index).unwrap();
                    assert!(sym.is_weak());
                    names.push(sym.name().unwrap().trim_start_matches('_').to_owned());
                }
                names.sort();
                assert_eq!(names, ["weak_data", "weak_fn"]);
            }
        }
    }
}

#[test]
fn elf_conventional_section_names_preserve_types_and_retention() {
    let target = Target::X86_64LinuxGnu;
    let src = r#"
int normal_data = 3;
int zeros __attribute__((retain,section(".data")));
int extra_zeros __attribute__((section(".data.zero")));
int kept(void) __attribute__((retain,section(".text")));
int kept(void) {return 7;}
int main(void) {return kept();}
"#;
    for debug in [false, true] {
        for optimize in [false, true] {
            let m = module(src, target, debug, optimize);
            let direct = object(&m, target, false);
            assert_eq!(direct, object(&m, target, true));
            let o = object::File::parse(direct.as_slice()).unwrap();
            assert!(
                matches!(o.sections().find(|s| s.name().unwrap()==".text" && matches!(s.flags(),object::SectionFlags::Elf{sh_flags} if sh_flags & 0x200000 != 0)).unwrap().flags(),object::SectionFlags::Elf{sh_flags} if sh_flags == 0x200006)
            );
            for name in [".data", ".data.zero"] {
                assert_eq!(
                    o.section_by_name(name).unwrap().kind(),
                    object::SectionKind::Data
                );
            }
        }
    }
    for (kind, name) in [("int", ".rodata1"), ("int f(void)", ".data1")] {
        let src = format!(
            "{kind} __attribute__((section(\"{name}\"))) {}",
            if kind == "int" {
                "= 3;"
            } else {
                "{ return 1; }"
            }
        );
        assert!(
            keel_c::compile(
                &src,
                "bad.c",
                keel_c::Options {
                    target,
                    debug_info: false
                }
            )
            .is_err()
        );
    }
}

// Each record occupies 32 bytes so strengthening the second input's alignment
// does not insert a hole that a C array iterator could mistake for a record.
fn registration_sources(target: Target) -> [String; 3] {
    let (section, code, alias) = match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => {
            ("registry", "plugin_code", "plugin_one")
        }
        Target::Aarch64AppleDarwin => ("__DATA,registry", "__TEXT,plugin_code", "_plugin_one"),
    };
    let record = "struct Entry { int (*run)(void); int *value; long tag; long reserved; };";
    let first = format!(
        r#"
{record}
static int value = 11;
static void setup(void) __attribute__((constructor,section("{code}")));
static void setup(void) {{ value = 31; }}
int first(void) __asm__("{alias}") __attribute__((weak,section("{code}")));
int first(void) {{ return 13; }}
static struct Entry one = {{ first, &value, 17, 0 }};
static struct Entry one __attribute__((retain,section("{section}"),aligned(8)));
"#
    );
    let second = format!(
        r#"
{record}
static int value = 19;
static int second(void) __attribute__((section("{code}")));
static int second(void) {{ return 23; }}
static void uncalled(void) {{
    static struct Entry two __attribute__((retain,section("{section}"),aligned(32))) = {{ second, &value, 29, 0 }};
}}
"#
    );
    let bounds = match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => {
            r#"
extern struct Entry __start_registry[], __stop_registry[];
static struct Entry *begin(void) { return __start_registry; }
static struct Entry *end(void) { return __stop_registry; }
"#
        }
        // Darwin's boundary labels have no C ABI underscore. Keel's current
        // asm-label declarations require that underscore, so use its supported
        // inline-asm address operands to reference the actual linker names.
        Target::Aarch64AppleDarwin => {
            r#"
static struct Entry *begin(void) {
    struct Entry *p;
    __asm__("adrp %0, section$start$__DATA$registry@PAGE\n\tadd %0, %0, section$start$__DATA$registry@PAGEOFF" : "=r"(p));
    return p;
}
static struct Entry *end(void) {
    struct Entry *p;
    __asm__("adrp %0, section$end$__DATA$registry@PAGE\n\tadd %0, %0, section$end$__DATA$registry@PAGEOFF" : "=r"(p));
    return p;
}
"#
        }
    };
    let reader = format!(
        r#"
{record}
{bounds}
// Ordinary data between the two registration inputs must stay outside bounds.
long unrelated __attribute__((retain)) = 97;
int main(void) {{
    struct Entry *p = begin(), *stop = end();
    int sum = 0, count = 0;
    if ((unsigned long)p % 32) return 1;
    for (; p < stop; ++p) {{
        sum += p->run() + *p->value + p->tag;
        if (p->reserved) return 2;
        ++count;
    }}
    return count != 2 || sum != 132;
}}
void _start(void) {{ main(); }}
"#
    );
    [first, reader, second]
}

fn registration_value(file: &object::File<'_>, name: &str) -> u64 {
    file.symbols()
        .find(|s| s.name().unwrap() == name)
        .unwrap()
        .address()
}

fn registration_pointer(file: &object::File<'_>, address: u64) -> u64 {
    let section = file
        .sections()
        .find(|s| address >= s.address() && address + 8 <= s.address() + s.size())
        .unwrap();
    let at = (address - section.address()) as usize;
    u64::from_le_bytes(section.data().unwrap()[at..at + 8].try_into().unwrap())
}

#[test]
fn c_registration_tables_with_native_bounds_and_system_gc() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-registration-bounds");
    fs::create_dir_all(&dir).unwrap();
    let ld = std::env::var("KEEL_TEST_GNU_LD").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/opt/homebrew/bin/x86_64-elf-ld".into()
        } else {
            "ld".into()
        }
    });
    // This fixture invokes an x86 GNU linker and the Darwin system linker.
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for debug in [false, true] {
            for optimize in [false, true] {
                let modules =
                    registration_sources(target).map(|src| module(&src, target, debug, optimize));
                assert!(!modules[2].functions.iter().any(|f| f.name == "uncalled"));
                let objects = modules.each_ref().map(|m| checked_object(m, target));
                let inputs: Vec<_> = objects
                    .iter()
                    .map(|bytes| keel_link::Input {
                        name: "registration.o",
                        bytes,
                    })
                    .collect();
                if target != Target::Aarch64AppleDarwin {
                    let native = keel_link::elf::link(&inputs, Default::default()).unwrap();
                    for (i, bytes) in objects.iter().enumerate() {
                        fs::write(dir.join(format!("{i}.o")), bytes).unwrap();
                    }
                    let exe = dir.join("elf-gc");
                    run(Command::new(&ld)
                        .arg("--gc-sections")
                        // Bound references must not become implicit GC roots:
                        // the C retain attributes keep these records alive.
                        .args(["-z", "start-stop-gc"])
                        .args((0..3).map(|i| dir.join(format!("{i}.o"))))
                        .arg("-o")
                        .arg(&exe));
                    for bytes in [&native.bytes, &fs::read(&exe).unwrap()] {
                        let file = object::File::parse(bytes.as_slice()).unwrap();
                        let begin = registration_value(&file, "__start_registry");
                        let end = registration_value(&file, "__stop_registry");
                        assert_eq!(begin % 32, 0);
                        assert_eq!(end - begin, 64);
                        assert_eq!(
                            registration_pointer(&file, begin),
                            registration_value(&file, "plugin_one")
                        );
                        assert_eq!(
                            registration_pointer(&file, begin + 32),
                            registration_value(&file, "second")
                        );
                        let unrelated = registration_value(&file, "unrelated");
                        assert!(unrelated < begin || unrelated >= end);
                        assert_eq!(registration_pointer(&file, begin + 16), 17);
                        assert_eq!(registration_pointer(&file, begin + 48), 29);
                        for (offset, value) in [(0, 11), (32, 19)] {
                            let pointer = registration_pointer(&file, begin + offset + 8);
                            // Both referents are int objects; do not read past the last one.
                            let section = file
                                .sections()
                                .find(|s| {
                                    pointer >= s.address() && pointer + 4 <= s.address() + s.size()
                                })
                                .unwrap();
                            let at = (pointer - section.address()) as usize;
                            assert_eq!(
                                &section.data().unwrap()[at..at + 4],
                                &i32::to_le_bytes(value)
                            );
                        }
                    }
                } else {
                    let native = keel_link::link(
                        &inputs,
                        keel_link::Options {
                            dylibs: vec![keel_link::Dylib {
                                install_name: "/usr/lib/libSystem.B.dylib".into(),
                                exports: Default::default(),
                                weak_exports: Default::default(),
                            }],
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let file = object::File::parse(native.bytes.as_slice()).unwrap();
                    let section = file.section_by_name("registry").unwrap();
                    assert_eq!(section.size(), 64);
                    assert_eq!(section.address() % 32, 0);
                    assert_eq!(
                        registration_value(&file, "section$start$__DATA$registry"),
                        section.address()
                    );
                    assert_eq!(
                        registration_value(&file, "section$end$__DATA$registry"),
                        section.address() + 64
                    );
                    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                    {
                        let sdk = String::from_utf8(
                            run(Command::new("xcrun").arg("--show-sdk-path")).stdout,
                        )
                        .unwrap();
                        let sdk = Path::new(sdk.trim());
                        let libc =
                            keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk)
                                .unwrap();
                        let exe = dir.join("arm64-run");
                        keel_link::link_signed(
                            &inputs,
                            keel_link::Options {
                                dylibs: vec![libc],
                                ..Default::default()
                            },
                            &exe,
                        )
                        .unwrap();
                        run(&mut Command::new(&exe));
                        for (i, bytes) in objects.iter().enumerate() {
                            fs::write(dir.join(format!("{i}.o")), bytes).unwrap();
                        }
                        run(Command::new("clang")
                            .args((0..3).map(|i| dir.join(format!("{i}.o"))))
                            .arg("-Wl,-dead_strip")
                            .arg("-o")
                            .arg(&exe));
                        run(&mut Command::new(&exe));
                    }
                }
            }
        }
    }
}

#[test]
fn validating_discarded_section_pointers_does_not_root_their_referents() {
    for target in Target::NATIVE {
        let section = names(target)[1];
        let src = format!(
            r#"
static int datum = 7;
static int callback(void) {{ return 11; }}
static int *p __attribute__((section("{section}"))) = &datum;
static int (*fn)(void) __attribute__((section("{section}"))) = callback;
static char *str __attribute__((section("{section}"))) = "discarded";
int main(void) {{ return 0; }}
"#
        );
        let m = module(&src, target, true, true);
        assert_eq!(m.functions.len(), 1);
        assert_eq!(m.functions[0].name, "main");
        assert!(m.data.is_empty());
    }
}

#[test]
fn elf_c_registration_contributions_reject_incompatible_cross_tu_storage() {
    let target = Target::X86_64LinuxGnu;
    for text in [false, true] {
        let first = object(
            &module(
                "int first __attribute__((retain,section(\"registry\"))) = 1; void _start(void) {}",
                target,
                true,
                true,
            ),
            target,
            text,
        );
        for src in [
            "int second __attribute__((section(\"registry\"))) = 2;",
            "int second __attribute__((retain,section(\"registry\")));",
            "const int second __attribute__((retain,section(\"registry\"))) = 2;",
            "int second(void) __attribute__((retain,section(\"registry\"))); int second(void) { return 2; }",
        ] {
            let second = object(&module(src, target, true, true), target, text);
            let error = keel_link::elf::link(
                &[
                    keel_link::Input {
                        name: "first.o",
                        bytes: &first,
                    },
                    keel_link::Input {
                        name: "second.o",
                        bytes: &second,
                    },
                ],
                Default::default(),
            )
            .err()
            .unwrap();
            assert!(
                error.contains("incompatible registration section contributions"),
                "{error}"
            );
        }
    }
}

fn tls_source(target: Target) -> String {
    let (data, zero) = match target {
        keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
        Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => (".custom_tls", ".custom_tls_zero"),
        Target::Aarch64AppleDarwin => ("__DATA,__custom_tls", "__DATA,__custom_tzero"),
    };
    format!(
        r#"
_Thread_local int value = 17;
extern _Thread_local int value __attribute__((section("{data}"), aligned(32), retain));
__thread int zeros[4] __attribute__((section("{zero}"), aligned(64), retain));
static _Thread_local int kept __attribute__((retain)) = 29;
static __thread int kept_zero __attribute__((retain));
static int anchor = 23;
static _Thread_local int *pointer __attribute__((section("{data}"), retain)) = &anchor;
static _Thread_local int compiler_only __attribute__((used)) = 31;
int step(int n) {{
    static __thread int local __attribute__((section("{data}"), retain)) = 5;
    if (value != 17 + n || zeros[0] != n || local != 5 + n || *pointer != 23) return 1;
    if ((unsigned long)&value % 32 || (unsigned long)&zeros % 64) return 2;
    value++; zeros[0]++; local++;
    return 0;
}}
int *address(void) {{ return &value; }}
"#
    )
}

#[test]
fn tls_template_sections_identity_alignment_retention_and_roundtrip() {
    for target in Target::NATIVE {
        for debug in [false, true] {
            for optimize in [false, true] {
                let m = module(&tls_source(target), target, debug, optimize);
                assert!(
                    m.data
                        .iter()
                        .find(|d| d.name == "value")
                        .unwrap()
                        .thread_local
                );
                let bytes = object(&m, target, false);
                assert_eq!(bytes, object(&m, target, true));
                let o = object::File::parse(bytes.as_slice()).unwrap();
                let (data, zero) = if target != Target::Aarch64AppleDarwin {
                    (".custom_tls", ".custom_tls_zero")
                } else {
                    ("__custom_tls", "__custom_tzero")
                };
                for (name, align, kind) in [(data, 32, 0x11), (zero, 64, 0x12)] {
                    let s = o.section_by_name(name).unwrap();
                    assert!(s.align() >= align);
                    if target != Target::Aarch64AppleDarwin {
                        assert_eq!(s.flags(), object::SectionFlags::Elf { sh_flags: 0x200403 });
                        assert_eq!(
                            s.kind(),
                            if kind == 0x12 {
                                object::SectionKind::UninitializedTls
                            } else {
                                object::SectionKind::Tls
                            }
                        );
                    } else {
                        assert!(
                            matches!(s.flags(), object::SectionFlags::MachO { flags } if flags & 255 == kind)
                        );
                        if kind == 0x12 {
                            assert!(s.data().unwrap().is_empty());
                        }
                    }
                }
                for name in ["value", "zeros", "kept", "kept_zero", "step.local"] {
                    if target == Target::Aarch64AppleDarwin {
                        for symbol in [format!("_{name}"), format!("_{name}$tlv$init")] {
                            let sym = o.symbols().find(|s| s.name().unwrap() == symbol).unwrap();
                            assert!(
                                matches!(sym.flags(), object::SymbolFlags::MachO { n_desc } if n_desc & 0x20 != 0)
                            );
                            let s = o.section_by_index(sym.section_index().unwrap()).unwrap();
                            assert!(
                                matches!(s.flags(), object::SectionFlags::MachO { flags } if flags & 255 == if symbol.ends_with("$init") { if matches!(name, "zeros" | "kept_zero") { 0x12 } else { 0x11 } } else { 0x13 })
                            );
                        }
                    } else {
                        let sym = o.symbols().find(|s| s.name().unwrap() == name).unwrap();
                        assert_eq!(sym.kind(), object::SymbolKind::Tls);
                    }
                }
            }
        }
    }
}

#[test]
fn tls_placement_and_redeclarations_diagnose_before_pruning() {
    for target in Target::NATIVE {
        let invalid = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => vec![
                ".text",
                ".data",
                ".bss",
                ".rodata",
                ".tbss",
                ".gnu_debugdata",
                ".init_array",
            ],
            Target::Aarch64AppleDarwin => vec![
                "__TEXT,__custom",
                "__DATA,__data",
                "__DATA,__thread_vars",
                "__DATA,__thread_ptrs",
                "__DATA,__thread_bss",
                "__DATA,__mod_init_func",
            ],
        };
        for section in invalid {
            let src =
                format!("static _Thread_local int x __attribute__((section(\"{section}\"))) = 3;");
            assert!(
                keel_c::compile(
                    &src,
                    "bad.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "{src}"
            );
        }
        let section = if target != Target::Aarch64AppleDarwin {
            ".mixed"
        } else {
            "__DATA,__mixed"
        };
        for src in [
            "void f(void) { _Thread_local int x; }".into(),
            "_Thread_local int f(void) { return 0; }".into(),
            "_Thread_local int x; extern int x;".into(),
            "extern int x; _Thread_local int x;".into(),
            "_Thread_local int x; void f(void) { extern int x; }".into(),
            "_Thread_local int x __attribute__((weak, retain));".into(),
            "extern _Thread_local int x __attribute__((retain));".into(),
            format!(
                "static _Thread_local int x __attribute__((section(\"{section}\"))) = 3; static int y __attribute__((section(\"{section}\"))) = 4;"
            ),
            format!(
                "static _Thread_local int x __attribute__((section(\"{section}\"))) = 3; static _Thread_local int y __attribute__((section(\"{section}\")));"
            ),
        ] {
            assert!(
                keel_c::compile(
                    &src,
                    "bad.c",
                    keel_c::Options {
                        target,
                        debug_info: false
                    }
                )
                .is_err(),
                "{src}"
            );
        }
        let valid = if target != Target::Aarch64AppleDarwin {
            ".tdata.named"
        } else {
            "__DATA,__thread_data"
        };
        module(
            &format!("_Thread_local int x __attribute__((section(\"{valid}\"), retain)) = 1;"),
            target,
            false,
            false,
        );
        let valid = if target != Target::Aarch64AppleDarwin {
            ".tbss.named"
        } else {
            "__DATA,__thread_bss"
        };
        module(
            &format!("_Thread_local int x __attribute__((section(\"{valid}\"), retain));"),
            target,
            false,
            false,
        );
    }
}

#[test]
fn qualified_tls_keeps_template_storage_with_late_attributes() {
    for target in Target::NATIVE {
        let (data, zero) = match target {
            keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
            Target::X86_64LinuxGnu | Target::Aarch64LinuxGnu => {
                (".tdata.qualified", ".tbss.qualified")
            }
            Target::Aarch64AppleDarwin => ("__DATA,__thread_data", "__DATA,__thread_bss"),
        };
        let src = format!(
            r#"
static _Thread_local const volatile int qualified = 17;
static _Thread_local const int zero = 0;
extern _Thread_local const volatile int qualified __attribute__((section("{data}"),retain,aligned(32)));
extern _Thread_local const int zero __attribute__((section("{zero}"),retain,aligned(64)));
int read(void) {{ return qualified + zero; }}
"#
        );
        for optimize in [false, true] {
            let m = module(&src, target, true, optimize);
            for (name, alignment) in [("qualified", 32), ("zero", 64)] {
                let d = m.data.iter().find(|d| d.name == name).unwrap();
                assert!(d.thread_local && d.local && d.retain);
                let init = d.init.as_ref().unwrap();
                assert!(!init.constant);
                assert_eq!(init.align, alignment);
            }
            let bytes = object(&m, target, false);
            assert_eq!(bytes, object(&m, target, true));
        }
    }
}

#[test]
fn darwin_tls_template_labels_do_not_alias_user_asm_labels() {
    let target = Target::Aarch64AppleDarwin;
    let src = r#"
_Thread_local int value __attribute__((retain,section("__DATA,__custom_tls"))) = 17;
int ordinary __asm__("_value$tlv$init") = 41;
int another __asm__("_value$tlv$init$") = 43;
int main(void) { return value != 17 || ordinary != 41 || another != 43; }
"#;
    for debug in [false, true] {
        for optimize in [false, true] {
            let m = module(src, target, debug, optimize);
            let bytes = object(&m, target, false);
            assert_eq!(bytes, object(&m, target, true));
            let f = object::File::parse(bytes.as_slice()).unwrap();
            for (name, expected) in [("_value$tlv$init", 41u32), ("_value$tlv$init$", 43)] {
                let s = f.symbols().find(|s| s.name().unwrap() == name).unwrap();
                let section = f.section_by_index(s.section_index().unwrap()).unwrap();
                assert_eq!(section.name().unwrap(), "__data");
                let offset = (s.address() - section.address()) as usize;
                assert_eq!(
                    &section.data().unwrap()[offset..offset + 4],
                    &expected.to_le_bytes()
                );
            }
            let template = f
                .symbols()
                .find(|s| s.name().unwrap() == "_value$tlv$init$$")
                .unwrap();
            assert!(
                matches!(template.flags(), object::SymbolFlags::MachO { n_desc } if n_desc & 0x20 != 0)
            );
            assert_eq!(
                f.section_by_index(template.section_index().unwrap())
                    .unwrap()
                    .name()
                    .unwrap(),
                "__custom_tls"
            );
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tls-label-collision");
                fs::create_dir_all(&dir).unwrap();
                let input = dir.join("input.o");
                fs::write(&input, &bytes).unwrap();
                let sdk =
                    String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout)
                        .unwrap();
                let sdk = Path::new(sdk.trim());
                let libc =
                    keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
                for native in [false, true] {
                    let exe = dir.join("run");
                    if native {
                        keel_link::link_signed(
                            &[keel_link::Input {
                                name: "input.o",
                                bytes: &bytes,
                            }],
                            keel_link::Options {
                                dylibs: vec![libc.clone()],
                                ..Default::default()
                            },
                            &exe,
                        )
                        .unwrap();
                    } else {
                        run(Command::new("clang")
                            .arg(&input)
                            .arg("-Wl,-dead_strip")
                            .arg("-o")
                            .arg(&exe));
                    }
                    run(&mut Command::new(&exe));
                }
            }
        }
    }

    // Imports also occupy the user's symbol namespace. They must stay imports,
    // rather than being silently resolved to the generated local template.
    for declaration in [
        "extern int other __asm__(\"_value$tlv$init\"); int *get(void) { return &other; }",
        "extern int other(void) __asm__(\"_value$tlv$init\"); int get(void) { return other(); }",
    ] {
        let m = module(
            &format!("_Thread_local int value __attribute__((retain)) = 17; {declaration}"),
            target,
            true,
            true,
        );
        let bytes = object(&m, target, false);
        assert_eq!(bytes, object(&m, target, true));
        let f = object::File::parse(bytes.as_slice()).unwrap();
        assert!(
            f.symbols()
                .find(|s| s.name().unwrap() == "_value$tlv$init")
                .unwrap()
                .is_undefined()
        );
        assert!(
            f.symbols()
                .any(|s| s.name().unwrap() == "_value$tlv$init$" && !s.is_undefined())
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_tls_runtime_source() -> String {
    // Keep runtime alignment within dyld's allocation guarantee, while also
    // exercising TLS through lifecycle roots and a cleanup callback on every
    // worker iteration. Main-thread state is checked again during teardown.
    tls_source(Target::Aarch64AppleDarwin)
        .replace("32", "8")
        .replace("64", "16")
        .replace("static int anchor = 23;", "static int anchor;")
        .replace(
            "int step(int n) {",
            r#"
extern void abort(void);
static _Thread_local int cleaned __attribute__((retain));
static void boot(void) __attribute__((constructor));
static void boot(void) {
    if (value != 17 || zeros[0] || cleaned) abort();
    anchor = 23;
}
static void finish(void) __attribute__((destructor));
static void finish(void) {
    if (value != 18 || zeros[0] != 1 || cleaned != 1) abort();
}
static void clean(int *p) { if (*p != 23) abort(); cleaned++; }
int step(int n) {
    int guard __attribute__((cleanup(clean))) = anchor;
    if (cleaned != n) return 6;
"#,
        )
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn arm64_custom_tls_threads_system_gc_and_native_links() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tls-arm64");
    fs::create_dir_all(&dir).unwrap();
    let harness = dir.join("harness.c");
    fs::write(&harness, r#"
#include <pthread.h>
extern int step(int);
extern int *address(void);
extern _Thread_local int value;
extern _Thread_local int zeros[4];
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cond = PTHREAD_COND_INITIALIZER;
static int ready, release;
static int *addresses[4];
static void *worker(void *arg) {
    long id = (long)arg;
    addresses[id] = address();
    if (&value != addresses[id] || value != 17 || zeros[0]) return (void *)3;
    pthread_mutex_lock(&lock);
    ready++; pthread_cond_broadcast(&cond);
    while (!release) pthread_cond_wait(&cond, &lock);
    pthread_mutex_unlock(&lock);
    for (int n = 0; n < 100; n++) { int r = step(n); if (r) return (void *)(long)r; }
    if (value != 117 || zeros[0] != 100) return (void *)4;
    return 0;
}
int main(void) {
    pthread_t threads[4];
    for (long i = 0; i < 4; i++) if (pthread_create(&threads[i], 0, worker, (void *)i)) return 3;
    pthread_mutex_lock(&lock);
    while (ready != 4) pthread_cond_wait(&cond, &lock);
    for (int i = 0; i < 4; i++) for (int j = 0; j < i; j++) if (addresses[i] == addresses[j]) return 4;
    release = 1; pthread_cond_broadcast(&cond); pthread_mutex_unlock(&lock);
    for (int i = 0; i < 4; i++) { void *result; if (pthread_join(threads[i], &result)) return 5; if (result) return 10 + (int)(long)result; }
    return step(0);
}
"#).unwrap();
    let harness_o = dir.join("harness.o");
    run(Command::new("clang")
        .arg("-c")
        .arg(&harness)
        .arg("-o")
        .arg(&harness_o));
    let harness_bytes = fs::read(&harness_o).unwrap();
    let sdk = String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout).unwrap();
    let sdk = Path::new(sdk.trim());
    let libc = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    for debug in [false, true] {
        for optimize in [false, true] {
            for text in [false, true] {
                let bytes = object(
                    &module(
                        &darwin_tls_runtime_source(),
                        Target::Aarch64AppleDarwin,
                        debug,
                        optimize,
                    ),
                    Target::Aarch64AppleDarwin,
                    text,
                );
                let input = dir.join("input.o");
                fs::write(&input, &bytes).unwrap();
                for mode in 0..3 {
                    let native = mode != 0;
                    let exe = dir.join("run");
                    if native {
                        keel_link::link_signed(
                            &[
                                keel_link::Input {
                                    name: "input.o",
                                    bytes: &bytes,
                                },
                                keel_link::Input {
                                    name: "harness.o",
                                    bytes: &harness_bytes,
                                },
                            ],
                            keel_link::Options {
                                dylibs: vec![libc.clone()],
                                dead_strip: mode == 2,
                                ..Default::default()
                            },
                            &exe,
                        )
                        .unwrap();
                    } else {
                        run(Command::new("clang")
                            .arg(&input)
                            .arg(&harness_o)
                            .arg("-Wl,-dead_strip")
                            .arg("-o")
                            .arg(&exe));
                        let linked = fs::read(&exe).unwrap();
                        let names = symbol_names(&linked);
                        assert!(names.iter().any(|n| n == "_kept"));
                        for n in ["_kept$tlv$init", "_kept_zero", "_kept_zero$tlv$init"] {
                            assert!(names.iter().any(|s| s == n), "missing retained TLS {n}");
                        }
                        assert!(!names.iter().any(|n| n == "_compiler_only"));
                    }
                    run(&mut Command::new(&exe));
                }
            }
        }
    }
    // Clang requires explicit Mach-O TLS section types in its C section
    // spelling; bare custom names produce regular storage and an invalid TLV
    // offset. Keel derives those types from TLS storage and the initializer.
    let clang_source = dir.join("clang.c");
    let oracle = darwin_tls_runtime_source()
        .replace("_Thread_local int value = 17;\n", "")
        .replace("extern _Thread_local int value", "_Thread_local int value")
        .replacen("retain));", "retain)) = 17;", 1)
        .replace(
            "__DATA,__custom_tls",
            "__DATA,__custom_tls,thread_local_regular",
        )
        .replace(
            "__DATA,__custom_tzero",
            "__DATA,__custom_tzero,thread_local_zerofill",
        );
    fs::write(&clang_source, oracle).unwrap();
    for opt in ["-O0", "-O2"] {
        let clang_o = dir.join("clang.o");
        run(Command::new("clang")
            .arg("-c")
            .arg("-g")
            // Match Keel's executable atexit policy; Clang's default DSO
            // destructor thunk requires the unsupported ___dso_handle.
            .arg("-fno-use-cxa-atexit")
            .arg(opt)
            .arg(&clang_source)
            .arg("-o")
            .arg(&clang_o));
        let clang_bytes = fs::read(&clang_o).unwrap();
        for mode in 0..3 {
            let native = mode != 0;
            let exe = dir.join("clang-run");
            if native {
                keel_link::link_signed(
                    &[
                        keel_link::Input {
                            name: "clang.o",
                            bytes: &clang_bytes,
                        },
                        keel_link::Input {
                            name: "harness.o",
                            bytes: &harness_bytes,
                        },
                    ],
                    keel_link::Options {
                        dylibs: vec![libc.clone()],
                        dead_strip: mode == 2,
                        ..Default::default()
                    },
                    &exe,
                )
                .unwrap();
            } else {
                run(Command::new("clang")
                    .arg(&clang_o)
                    .arg(&harness_o)
                    .arg("-Wl,-dead_strip")
                    .arg("-o")
                    .arg(&exe));
            }
            run(&mut Command::new(&exe));
        }
    }
}

#[test]
fn elf_custom_tls_system_gc_pt_tls_relocations_and_native_diagnostic() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tls-elf");
    fs::create_dir_all(&dir).unwrap();
    let tool = |env: &str, suffix: &str| {
        std::env::var(env).unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                format!("/opt/homebrew/bin/x86_64-elf-{suffix}")
            } else {
                suffix.into()
            }
        })
    };
    let ld = tool("KEEL_TEST_GNU_LD", "ld");
    let readelf = tool("KEEL_TEST_GNU_READELF", "readelf");
    let target = Target::X86_64LinuxGnu;
    // A defined access uses local-exec TPOFF32; a separate TU's declaration
    // uses initial-exec GOTTPOFF, requiring system-linker TLS GOT processing.
    let user = module(
        "extern _Thread_local int value; int imported(void) { return value; }",
        target,
        false,
        false,
    );
    let entry = module(
        "extern int step(int); extern int imported(void); void _start(void) { step(0); imported(); }",
        target,
        false,
        false,
    );
    let raw_tls =
        keel_asm::x86::assemble_object(".section .anonymous_tls,\"awT\",@progbits\n.zero 4\n")
            .unwrap();
    let err = keel_link::elf::link(
        &[keel_link::Input {
            name: "raw.o",
            bytes: &raw_tls,
        }],
        Default::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("TLS section") && err.contains("external ELF linking"),
        "{err}"
    );
    for debug in [false, true] {
        for optimize in [false, true] {
            for text in [false, true] {
                let bytes = object(
                    &module(&tls_source(target), target, debug, optimize),
                    target,
                    text,
                );
                let imported = object(&user, target, text);
                let entry_bytes = object(&entry, target, text);
                let file = object::File::parse(bytes.as_slice()).unwrap();
                assert!(
                    file.sections()
                        .flat_map(|s| s.relocations())
                        .any(|(_, r)| matches!(
                            r.flags(),
                            object::RelocationFlags::Elf { r_type: 23 }
                        ))
                ); // R_X86_64_TPOFF32
                let file = object::File::parse(imported.as_slice()).unwrap();
                assert!(
                    file.sections()
                        .flat_map(|s| s.relocations())
                        .any(|(_, r)| matches!(
                            r.flags(),
                            object::RelocationFlags::Elf { r_type: 22 }
                        ))
                ); // R_X86_64_GOTTPOFF
                let input = dir.join("input.o");
                let other = dir.join("other.o");
                let start = dir.join("start.o");
                let exe = dir.join("run");
                fs::write(&input, &bytes).unwrap();
                fs::write(&other, &imported).unwrap();
                fs::write(&start, &entry_bytes).unwrap();
                run(Command::new(&ld)
                    .arg("--gc-sections")
                    .arg("-e")
                    .arg("_start")
                    .arg(&start)
                    .arg(&other)
                    .arg(&input)
                    .arg("-o")
                    .arg(&exe));
                let linked = fs::read(&exe).unwrap();
                let names = symbol_names(&linked);
                assert!(names.iter().any(|n| n == "kept"));
                assert!(names.iter().any(|n| n == "kept_zero"));
                assert!(!names.iter().any(|n| n == "compiler_only"));
                let headers =
                    String::from_utf8(run(Command::new(&readelf).arg("-lW").arg(&exe)).stdout)
                        .unwrap();
                assert!(
                    headers.lines().any(|l| l.trim_start().starts_with("TLS ")),
                    "{headers}"
                );
                let error = keel_link::elf::link(
                    &[keel_link::Input {
                        name: "input.o",
                        bytes: &bytes,
                    }],
                    Default::default(),
                )
                .unwrap_err();
                assert!(
                    error.contains("external ELF linking") && error.contains("PT_TLS"),
                    "{error}"
                );
                let error = keel_link::elf::link(
                    &[keel_link::Input {
                        name: "other.o",
                        bytes: &imported,
                    }],
                    Default::default(),
                )
                .unwrap_err();
                assert!(error.contains("external ELF linking"), "{error}");
                // System ELF linker catches the cross-TU TLS/non-TLS symbol
                // incompatibility; C's erased object metadata cannot infer it.
                let wrong = object(
                    &module("int value = 1;", target, false, false),
                    target,
                    text,
                );
                let wrong_o = dir.join("wrong.o");
                fs::write(&wrong_o, &wrong).unwrap();
                let output = Command::new(&ld)
                    .arg("-e")
                    .arg("imported")
                    .arg(&other)
                    .arg(&wrong_o)
                    .arg("-o")
                    .arg(dir.join("invalid"))
                    .output()
                    .unwrap();
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains("TLS"));
            }
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn darwin_native_cross_tu_tls_reference_requires_descriptor() {
    let target = Target::Aarch64AppleDarwin;
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tls-cross-tu");
    fs::create_dir_all(&dir).unwrap();
    let sdk = String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout).unwrap();
    let sdk = Path::new(sdk.trim());
    let libc = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    for text in [false, true] {
        let user = object(
            &module(
                "extern _Thread_local int value; int main(void) { return value; }",
                target,
                false,
                false,
            ),
            target,
            text,
        );
        let wrong = object(
            &module("int value = 3;", target, false, false),
            target,
            text,
        );
        let err = keel_link::link(
            &[
                keel_link::Input {
                    name: "user.o",
                    bytes: &user,
                },
                keel_link::Input {
                    name: "wrong.o",
                    bytes: &wrong,
                },
            ],
            keel_link::Options {
                dylibs: vec![libc.clone()],
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(
            err.contains("TLVP relocation must reference a TLS descriptor"),
            "{err}"
        );
        let user_o = dir.join("user.o");
        let wrong_o = dir.join("wrong.o");
        fs::write(&user_o, &user).unwrap();
        fs::write(&wrong_o, &wrong).unwrap();
        let result = Command::new("clang")
            .arg(&user_o)
            .arg(&wrong_o)
            .arg("-o")
            .arg(dir.join("run"))
            .output()
            .unwrap();
        // Apple ld currently accepts this incompatible source declaration.
        // Do not execute the resulting getter call into ordinary data.
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_dead_strip_retention_used_and_callback_table_match_apple() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-gc-retention");
    fs::create_dir_all(&dir).unwrap();
    let sdk = String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout).unwrap();
    let sdk = Path::new(sdk.trim());
    let libc = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    let source = r#"
int kept __attribute__((retain,section("__DATA,__kept_gc"))) = 41;
int compiler_only __attribute__((used,section("__DATA,__used_gc"))) = 43;
int dead_fn(void) __attribute__((used,section("__TEXT,__dead_gc")));
int dead_fn(void) { return 47; }
int callback(void) __attribute__((section("__TEXT,__callback")));
int callback(void) { return 37; }
int (*registration)(void) __attribute__((retain,section("__DATA,__registry"))) = callback;
int main(void) { return registration() != 37; }
"#;
    for optimize in [false, true] {
        for text in [false, true] {
            let bytes = object(
                &module(source, Target::Aarch64AppleDarwin, true, optimize),
                Target::Aarch64AppleDarwin,
                text,
            );
            let input = dir.join("input.o");
            fs::write(&input, &bytes).unwrap();
            for native in [false, true] {
                let exe = dir.join("program");
                if native {
                    keel_link::link_signed(
                        &[keel_link::Input {
                            name: "retention.o",
                            bytes: &bytes,
                        }],
                        keel_link::Options {
                            dylibs: vec![libc.clone()],
                            dead_strip: true,
                            ..Default::default()
                        },
                        &exe,
                    )
                    .unwrap();
                } else {
                    run(Command::new("clang")
                        .arg(&input)
                        .arg("-Wl,-dead_strip")
                        .arg("-o")
                        .arg(&exe));
                }
                let image = fs::read(&exe).unwrap();
                let names = symbol_names(&image);
                for name in ["_kept", "_registration", "_callback"] {
                    assert!(names.iter().any(|n| n == name), "missing {name}");
                }
                for name in ["_compiler_only", "_dead_fn"] {
                    assert!(!names.iter().any(|n| n == name), "retained {name}");
                }
                run(&mut Command::new(&exe));
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
