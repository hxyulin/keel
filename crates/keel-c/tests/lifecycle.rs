use keel_ir::{Module, Target};
use object::{Object, ObjectSection, ObjectSymbol};

fn module(src: &str, target: Target, debug_info: bool, optimize: bool) -> Module {
    let mut m =
        keel_c::compile(src, "lifecycle.c", keel_c::Options { target, debug_info }).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if optimize {
        keel_opt::optimize(&mut m).unwrap();
    }
    keel_ir::verify_module(&m).unwrap();
    let text = m.to_string();
    let m = keel_ir::parse(&text).unwrap();
    assert_eq!(m.to_string(), text);
    keel_ir::verify_module(&m).unwrap();
    m
}
fn object(m: &Module, target: Target, assembly: bool) -> Vec<u8> {
    if assembly {
        let asm = keel_codegen::compile(m, target).unwrap();
        match target {
            Target::Aarch64AppleDarwin => keel_asm::assemble_object(&asm).unwrap(),
            Target::Aarch64LinuxGnu => keel_asm::assemble_elf_object(&asm).unwrap(),
            Target::X86_64LinuxGnu => keel_asm::x86::assemble_object(&asm).unwrap(),
        }
    } else {
        let p = keel_codegen::compile_program(m, target).unwrap();
        match target {
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
const SRC: &str = r#"
extern int puts(const char *);
static int unused __attribute__((used)) = 17;
static void emitted(void) __attribute__((used));
static void emitted(void) { puts("unused"); }
static void start(void) { puts("start"); }
static void stop(void) { puts("stop"); }
static void start(void) __attribute__((constructor(65535)));
static void stop(void) __attribute__((destructor));
int main(void) { puts("main"); return 0; }
"#;
#[test]
fn lifecycle_roundtrip_emission_and_section_types() {
    for target in Target::ALL {
        for debug in [false, true] {
            for optimize in [false, true] {
                let m = module(SRC, target, debug, optimize);
                let emitted = m.functions.iter().find(|f| f.name == "emitted").unwrap();
                assert!(emitted.attrs.used);
                assert_eq!(emitted.sig.cc, keel_ir::CallConv::C);
                assert!(m.data.iter().any(|d| d.name == "unused"));
                let bytes = object(&m, target, false);
                assert_eq!(bytes, object(&m, target, true));
                let o = object::File::parse(bytes.as_slice()).unwrap();
                if target == Target::Aarch64AppleDarwin {
                    let s = o.section_by_name("__mod_init_func").unwrap();
                    assert_eq!(s.size(), 16);
                    assert_eq!(s.align(), 8);
                    assert!(
                        matches!(s.flags(), object::SectionFlags::MachO { flags } if flags & 255 == 9)
                    );
                    assert!(
                        o.symbols()
                            .any(|s| s.name().unwrap() == "_atexit" && s.is_undefined())
                    );
                } else {
                    for (name, ty) in [(".init_array", 14), (".fini_array", 15)] {
                        let s = o.section_by_name(name).unwrap();
                        assert_eq!(s.size(), 8);
                        assert_eq!(s.align(), 8);
                        use object::read::elf::SectionHeader;
                        let object::File::Elf64(elf) = &o else {
                            panic!("not ELF64")
                        };
                        assert_eq!(
                            elf.elf_section_table()
                                .section(s.index())
                                .unwrap()
                                .sh_type(object::Endianness::Little),
                            ty
                        );
                        assert!(matches!(
                            s.flags(),
                            object::SectionFlags::Elf { sh_flags: 3 }
                        ));
                        assert_eq!(s.relocations().count(), 1);
                    }
                }
            }
        }
    }
}
#[test]
fn invalid_lifecycle_and_retention_contracts() {
    for target in Target::ALL {
        for src in [
            "int f(void) __attribute__((constructor)); int f(void){return 0;}",
            "void f(int x) __attribute__((destructor)); void f(int x){}",
            "void f() __attribute__((constructor)); void f(){}",
            "void f(void) __attribute__((constructor));",
            "int x __attribute__((constructor));",
            "typedef void F(void) __attribute__((constructor));",
            "void f(int x __attribute__((constructor))) {}",
            "struct S { int x __attribute__((used)); };",
            "int f(void){ int x __attribute__((used)); return 0; }",
            "void f(void) __attribute__((constructor,noreturn)); void f(void){}",
            "void f(void) __attribute__((destructor,returns_twice)); void f(void){}",
            "void f(void){} int f(void); void f(void) __attribute__((constructor));",
            "void f(void){} void f(int); void f(void) __attribute__((destructor));",
            "void f(void) __attribute__((constructor(101))); void f(void){}",
            "void f(void) __attribute__((destructor(65536))); void f(void){}",
            "void f(void) __attribute__((constructor(-1))); void f(void){}",
            "void f(void) __attribute__((constructor(65535),constructor(101))); void f(void){}",
            "void f(void) __attribute__((used(1))); void f(void){}",
            "void f(void) __attribute__((constructor())); void f(void){}",
            "void f(void) __attribute__((destructor(1,2))); void f(void){}",
            "void f(void) __attribute__((constructor(65535))){} void f(void) __attribute__((noreturn));",
            "int f(void){return sizeof(int __attribute__((constructor)));}",
            "int side(void); void f(void) __attribute__((constructor((side(),65535)))); void f(void){}",
            "void f(void) __attribute__((constructor(65535.0))); void f(void){}",
        ] {
            assert!(
                keel_c::compile(
                    src,
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
    }
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn arm64_lifecycle_system_native_multi_tu_and_archive() {
    use std::{fs, process::Command};
    fn run(c: &mut Command) -> std::process::Output {
        let o = c.output().unwrap();
        assert!(
            o.status.success(),
            "{c:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        o
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("lifecycle-native");
    fs::create_dir_all(&dir).unwrap();
    let target = Target::Aarch64AppleDarwin;
    let sdk = String::from_utf8(run(Command::new("xcrun").arg("--show-sdk-path")).stdout).unwrap();
    let sdk = std::path::Path::new(sdk.trim());
    let libc = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), sdk).unwrap();
    let other = "extern int puts(const char*); int anchor(void){return 0;} static void b(void) __attribute__((constructor)); static void b(void){puts(\"other-start\");} static void z(void) __attribute__((destructor)); static void z(void){puts(\"other-stop\");}";
    let main = SRC
        .replace(
            "puts(\"main\"); return 0;",
            "puts(\"main\"); return anchor();",
        )
        .replace("int main(void)", "extern int anchor(void); int main(void)");
    for early_exit in [false, true] {
        let main = if early_exit {
            main.replace("return anchor();", "anchor(); exit(0); return 1;")
                .replace(
                    "extern int anchor",
                    "extern void exit(int); extern int anchor",
                )
        } else {
            main.clone()
        };
        let c1 = dir.join("main.c");
        let c2 = dir.join("other.c");
        let oracle_main = main
            .replace(
                "static void start(void) {",
                "static void start(void) __attribute__((constructor)); static void start(void) {",
            )
            .replace(
                "static void stop(void) {",
                "static void stop(void) __attribute__((destructor)); static void stop(void) {",
            );
        fs::write(&c1, &oracle_main).unwrap();
        fs::write(&c2, other).unwrap();
        let oracle = dir.join("oracle");
        run(Command::new("clang")
            .args([&c1, &c2])
            .arg("-o")
            .arg(&oracle));
        let want = run(&mut Command::new(&oracle)).stdout;
        assert_eq!(want, b"start\nother-start\nmain\nother-stop\nstop\n");
        for debug in [false, true] {
            for optimize in [false, true] {
                for assembly in [false, true] {
                    let a = dir.join("main.o");
                    let b = dir.join("other.o");
                    fs::write(
                        &a,
                        object(&module(&main, target, debug, optimize), target, assembly),
                    )
                    .unwrap();
                    fs::write(
                        &b,
                        object(&module(other, target, debug, optimize), target, assembly),
                    )
                    .unwrap();
                    let archive = dir.join("other.a");
                    let _ = fs::remove_file(&archive);
                    run(Command::new("ar").arg("rcs").arg(&archive).arg(&b));
                    for archived in [false, true] {
                        let paths = [&a, if archived { &archive } else { &b }];
                        for native in [false, true] {
                            let exe = dir.join("keel");
                            if native {
                                let bytes: Vec<_> =
                                    paths.iter().map(|p| fs::read(p).unwrap()).collect();
                                let inputs: Vec<_> = bytes
                                    .iter()
                                    .map(|b| keel_link::Input {
                                        name: "lifecycle",
                                        bytes: b,
                                    })
                                    .collect();
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
                                run(Command::new("clang")
                                    .args(paths)
                                    .arg("-Wl,-dead_strip")
                                    .arg("-o")
                                    .arg(&exe));
                            }
                            if !native {
                                // Compiler used emits definitions but grants no linker retention.
                                let bytes = fs::read(&exe).unwrap();
                                let image = object::File::parse(bytes.as_slice()).unwrap();
                                assert!(
                                    !image.symbols().any(|s| matches!(
                                        s.name().unwrap(),
                                        "_emitted" | "_unused"
                                    ))
                                );
                            }
                            assert_eq!(
                                run(&mut Command::new(&exe)).stdout,
                                want,
                                "debug={debug} optimize={optimize} assembly={assembly} archived={archived} native={native} exit={early_exit}"
                            );
                        }
                    }
                }
            }
        }
    }
    // Mixed producer interposition: both registrations must name the winning
    // strong symbol, including the Darwin atexit thunk's function pointer.
    let weak = "extern int puts(const char*); void callback(void) __attribute__((weak,constructor,destructor)); void callback(void){puts(\"weak\");} int main(void){puts(\"main\"); return 0;}";
    let strong = dir.join("strong.c");
    fs::write(
        &strong,
        "extern int puts(const char*); void callback(void){puts(\"strong\");}",
    )
    .unwrap();
    let strong_o = dir.join("strong.o");
    run(Command::new("clang")
        .arg("-c")
        .arg(&strong)
        .arg("-o")
        .arg(&strong_o));
    let weak_o = dir.join("weak.o");
    fs::write(
        &weak_o,
        object(&module(weak, target, false, true), target, false),
    )
    .unwrap();
    let plain_o = dir.join("plain.o");
    fs::write(
        &plain_o,
        object(&module(SRC, target, false, true), target, false),
    )
    .unwrap();
    let archive = dir.join("other.a");
    for (paths, want) in [
        (vec![&weak_o, &strong_o], &b"strong\nmain\nstrong\n"[..]),
        (vec![&strong_o, &weak_o], &b"strong\nmain\nstrong\n"[..]),
        (vec![&plain_o, &archive], &b"start\nmain\nstop\n"[..]),
    ] {
        for native in [false, true] {
            let exe = dir.join("mixed");
            if native {
                let bytes: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
                let inputs: Vec<_> = bytes
                    .iter()
                    .map(|b| keel_link::Input {
                        name: "mixed",
                        bytes: b,
                    })
                    .collect();
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
                run(Command::new("clang")
                    .args(&paths)
                    .arg("-Wl,-dead_strip")
                    .arg("-o")
                    .arg(&exe));
            }
            assert_eq!(run(&mut Command::new(&exe)).stdout, want);
        }
    }
    // Forward declarations must not reorder either lifecycle phase.
    let ordered = "extern int puts(const char*); static void second(void); static void first(void) __attribute__((constructor,destructor)); static void first(void) { puts(\"first\"); } static void second(void) __attribute__((constructor,destructor)); static void second(void) { puts(\"second\"); } int main(void) { puts(\"main\"); return 0; }";
    let ordered_c = dir.join("ordered.c");
    fs::write(&ordered_c, ordered).unwrap();
    let oracle = dir.join("ordered-oracle");
    run(Command::new("clang").arg(&ordered_c).arg("-o").arg(&oracle));
    let want = run(&mut Command::new(&oracle)).stdout;
    assert_eq!(want, b"first\nsecond\nmain\nsecond\nfirst\n");
    for optimize in [false, true] {
        let ordered_o = dir.join("ordered.o");
        fs::write(
            &ordered_o,
            object(&module(ordered, target, false, optimize), target, false),
        )
        .unwrap();
        for native in [false, true] {
            let exe = dir.join("ordered");
            if native {
                let bytes = fs::read(&ordered_o).unwrap();
                keel_link::link_signed(
                    &[keel_link::Input {
                        name: "ordered",
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
                    .arg(&ordered_o)
                    .arg("-Wl,-dead_strip")
                    .arg("-o")
                    .arg(&exe));
            }
            assert_eq!(run(&mut Command::new(&exe)).stdout, want);
        }
    }
    // Clang's modern Darwin destructor ABI needs a linker-synthesized DSO
    // handle, which the existing native executable linker does not provide.
    let clang_other = dir.join("clang-other.o");
    run(Command::new("clang")
        .arg("-c")
        .arg(dir.join("other.c"))
        .arg("-o")
        .arg(&clang_other));
    let exe = dir.join("mixed-lifecycle");
    run(Command::new("clang")
        .args([&plain_o, &clang_other])
        .arg("-o")
        .arg(&exe));
    assert_eq!(
        run(&mut Command::new(&exe)).stdout,
        b"start\nother-start\nmain\nother-stop\nstop\n"
    );
    let bytes = [fs::read(&plain_o).unwrap(), fs::read(&clang_other).unwrap()];
    let inputs: Vec<_> = bytes
        .iter()
        .map(|b| keel_link::Input {
            name: "mixed-lifecycle",
            bytes: b,
        })
        .collect();
    let error = match keel_link::link(
        &inputs,
        keel_link::Options {
            dylibs: vec![libc],
            ..Default::default()
        },
    ) {
        Ok(_) => panic!("native linker unexpectedly supplies Clang DSO handle"),
        Err(e) => e,
    };
    assert!(error.contains("___dso_handle"), "{error}");
}

#[test]
fn elf_native_image_arrays_preserve_weak_symbol_identity() {
    let target = Target::X86_64LinuxGnu;
    let weak =
        "void callback(void) __attribute__((weak,constructor,destructor)); void callback(void) {}";
    let strong = "void callback(void) {}";
    let start = keel_asm::x86::assemble_object(".text\n.globl _start\n_start:\nret\n").unwrap();
    let a = object(&module(weak, target, false, true), target, false);
    let b = object(&module(strong, target, false, true), target, true);
    for units in [vec![&start, &a], vec![&start, &a, &b], vec![&start, &b, &a]] {
        let inputs: Vec<_> = units
            .iter()
            .map(|b| keel_link::Input {
                name: "lifecycle.o",
                bytes: b,
            })
            .collect();
        let exe = keel_link::elf::link(&inputs, Default::default()).unwrap();
        let file = object::File::parse(exe.bytes.as_slice()).unwrap();
        let callback = file
            .symbols()
            .find(|s| s.name().unwrap() == "callback")
            .unwrap()
            .address();
        for name in [".init_array", ".fini_array"] {
            let s = file.section_by_name(name).unwrap();
            let data = s.data().unwrap();
            assert_eq!(data.len(), 8);
            assert_eq!(u64::from_le_bytes(data.try_into().unwrap()), callback);
        }
    }
}

#[test]
fn darwin_registration_allows_ordinary_atexit_and_rejects_conflicting_interface() {
    let target = Target::Aarch64AppleDarwin;
    let callback = "static void done(void) __attribute__((destructor)); static void done(void) {}";
    module(
        &format!("{callback} int atexit(void (*)(void)); int main(void){{return atexit(done);}}"),
        target,
        false,
        true,
    );
    module(
        &format!("{callback} int atexit(void (*)(void)) __attribute__((noinline));"),
        target,
        false,
        true,
    );
    for source in [
        format!("{callback} void atexit(void (*f)(void)){{}}"),
        format!("{callback} int atexit(int); int main(void){{return atexit(0);}}"),
        format!(
            "{callback} int atexit(void (*)(void)) __attribute__((weak)); int main(void){{return atexit(done);}}"
        ),
    ] {
        assert!(
            keel_c::compile(
                &source,
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

#[test]
fn default_priority_expressions_inline_definitions_and_gnu_spellings() {
    for target in Target::ALL {
        let m = module(
            "static inline void f(void) __attribute__((__constructor__(65535+0), __destructor__, __used__)); static inline void f(void){}",
            target,
            false,
            true,
        );
        let f = m.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(f.attrs.constructor && f.attrs.used);
        for priority in ["0", "100", "101", "65534", "65536", "-1"] {
            let source =
                format!("void f(void) __attribute__((constructor({priority}))); void f(void){{}}");
            let error = keel_c::compile(
                &source,
                "bad.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap_err();
            assert!(
                error.message.contains("priority 65535"),
                "{}",
                error.message
            );
        }
    }
}

#[test]
fn registrations_follow_definitions_not_forward_declarations() {
    let source = "static void second(void); static void first(void) __attribute__((constructor,destructor)); static void first(void) {} static void second(void) __attribute__((constructor,destructor)); static void second(void) {}";
    for target in Target::ALL {
        for optimize in [false, true] {
            let m = module(source, target, false, optimize);
            let constructors: Vec<_> = m
                .functions
                .iter()
                .filter(|f| f.attrs.constructor && !f.name.starts_with(".keel.atexit."))
                .map(|f| f.name.as_str())
                .collect();
            assert_eq!(constructors, ["first", "second"]);
            let bytes = object(&m, target, false);
            assert_eq!(bytes, object(&m, target, true));
            let file = object::File::parse(bytes.as_slice()).unwrap();
            let arrays: &[&str] = if target != Target::Aarch64AppleDarwin {
                &[".init_array", ".fini_array"]
            } else {
                &["__mod_init_func"]
            };
            for name in arrays {
                let mut registrations: Vec<_> = file
                    .section_by_name(name)
                    .unwrap()
                    .relocations()
                    .map(|(offset, r)| {
                        let object::RelocationTarget::Symbol(index) = r.target() else {
                            panic!("symbol registration expected")
                        };
                        let mut symbol = file.symbol_by_index(index).unwrap();
                        // ELF may encode local function addresses against the
                        // text section symbol plus an addend.
                        if symbol.kind() == object::SymbolKind::Section {
                            let address = symbol.address().wrapping_add_signed(r.addend());
                            symbol = file
                                .symbols()
                                .find(|s| {
                                    s.kind() == object::SymbolKind::Text
                                        && s.section_index() == symbol.section_index()
                                        && s.address() == address
                                })
                                .unwrap();
                        }
                        (offset, symbol.name().unwrap())
                    })
                    .collect();
                registrations.sort_by_key(|(offset, _)| *offset);
                let names: Vec<_> = registrations
                    .iter()
                    .take(2)
                    .map(|(_, name)| name.trim_start_matches('_'))
                    .collect();
                assert_eq!(names, ["first", "second"]);
            }
            if target != Target::Aarch64AppleDarwin {
                let destructors: Vec<_> = m
                    .functions
                    .iter()
                    .filter(|f| f.attrs.destructor)
                    .map(|f| f.name.as_str())
                    .collect();
                assert_eq!(destructors, ["first", "second"]);
            } else {
                let callbacks: Vec<_> = m
                    .functions
                    .iter()
                    .filter(|f| f.name.starts_with(".keel.atexit."))
                    .flat_map(|f| {
                        f.callees()
                            .filter(|(_, d)| d.name != "atexit")
                            .map(|(_, d)| d.name.as_str())
                    })
                    .collect();
                assert_eq!(callbacks, ["first", "second"]);
            }
        }
    }
}

#[test]
fn darwin_registration_validates_unreferenced_atexit_contracts() {
    let callback = "static void done(void) __attribute__((destructor)); static void done(void) {}";
    for declaration in [
        "int atexit(int);",
        "int atexit(int *);",
        "int atexit(void (*)(int));",
        "int atexit(void (*)(void)) __attribute__((weak));",
        "static int atexit(void (*f)(void)) { return 0; }",
        "static int atexit;",
        "int other(int *) __asm__(\"_atexit\");",
        "void atexit(void (*)(void)); int atexit(void (*)(void));",
    ] {
        let source = format!("{callback} {declaration}");
        let error = keel_c::compile(
            &source,
            "bad.c",
            keel_c::Options {
                target: Target::Aarch64AppleDarwin,
                debug_info: false,
            },
        )
        .unwrap_err();
        assert!(
            error.message.contains("ordinary libc atexit interface"),
            "{source}: {}",
            error.message
        );
    }
}

#[test]
fn lifecycle_accepts_compatible_old_style_definitions_and_qualified_void() {
    for target in Target::ALL {
        for source in [
            "void f(void) __attribute__((constructor,destructor)); void f() {}",
            "void f() {} void f(void) __attribute__((constructor,destructor));",
            "typedef const void V; V f(void) __attribute__((constructor,destructor)); V f(void) {}",
        ] {
            module(source, target, false, true);
        }
    }
}

#[test]
fn darwin_destructor_wrapper_names_avoid_user_symbols() {
    let m = module(
        "void collision(void) __asm__(\"_.keel.atexit.0\"); void collision(void) {} static void done(void) __attribute__((destructor)); static void done(void) {}",
        Target::Aarch64AppleDarwin,
        false,
        true,
    );
    assert!(
        m.functions
            .iter()
            .any(|f| f.name == ".keel.atexit.0" && !f.attrs.constructor)
    );
    assert!(
        m.functions
            .iter()
            .any(|f| f.name == ".keel.atexit.0." && f.attrs.constructor)
    );
    object(&m, Target::Aarch64AppleDarwin, false);
}

#[test]
fn darwin_destructor_wrapper_names_reserve_pruned_declarations() {
    let m = module(
        "void external(void) __asm__(\"_.keel.atexit.0\"); static void done(void) __attribute__((destructor)); static void done(void) {} void probe(void) { __asm__ volatile(\"bl _.keel.atexit.0\" ::: \"x30\"); }",
        Target::Aarch64AppleDarwin,
        false,
        false,
    );
    let bytes = object(&m, Target::Aarch64AppleDarwin, false);
    let file = object::File::parse(bytes.as_slice()).unwrap();
    let symbol = file
        .symbols()
        .find(|s| s.name().unwrap() == "_.keel.atexit.0")
        .unwrap();
    assert!(
        symbol.is_undefined(),
        "generated wrapper captured an inline-assembly reference"
    );
    assert!(
        m.functions
            .iter()
            .any(|f| f.name == ".keel.atexit.0." && f.attrs.constructor)
    );
}
