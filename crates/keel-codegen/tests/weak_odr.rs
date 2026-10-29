#[path = "weak_odr/hidden_visibility.rs"]
mod hidden_visibility;
#[path = "weak_odr/weak.rs"]
mod weak;

use keel_codegen::{Target, compile, compile_program};
use keel_ir::{InstData, Module};
use object::{Object, ObjectSection, ObjectSymbol, SymbolFlags};

fn source(tu: &str) -> String {
    format!(
        "data @type_name = weak_odr hidden const align 1 {{ b\"Name\\0\" }}\n\
         data @type_info = weak_odr hidden const align 8 {{ rtti_name @type_name }}\n\
         func @type_name_{tu}() -> ptr {{\nblock0:\n v0 = symbol_addr @type_name\n return v0\n}}\n\
         func @type_info_{tu}() -> ptr {{\nblock0:\n v0 = symbol_addr @type_info\n return v0\n}}\n\
         data @shared = weak_odr align 8 {{ i64 11 }}\n\
         data @zero = weak_odr align 8 {{ i64 0 }}\n\
         data @vtable = weak_odr const align 8 {{ ptr @method, ptr @shared, ptr @zero }}\n\
         func @method() -> i64 noinline weak_odr {{\nblock0:\n v0 = iconst.i64 7\n return v0\n}}\n\
         func @method_{tu}() -> ptr {{\nblock0:\n v0 = symbol_addr @method\n return v0\n}}\n\
         func @shared_{tu}() -> ptr {{\nblock0:\n v0 = symbol_addr @shared\n return v0\n}}\n\
         func @zero_{tu}() -> ptr {{\nblock0:\n v0 = symbol_addr @zero\n return v0\n}}\n\
         func @vtable_{tu}() -> ptr {{\nblock0:\n v0 = symbol_addr @vtable\n return v0\n}}\n\
         func @call_{tu}() -> i64 {{\nblock0:\n v0 = call @method()\n return v0\n}}\n"
    )
}
fn module(tu: &str) -> Module {
    let mut m = keel_ir::parse(&source(tu)).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["inline", "inline-noninterposable"]).unwrap();
    keel_opt::optimize_with_tier(&mut m, keel_opt::OptimizationTier::Full).unwrap();
    assert_eq!(m.functions.len(), 8);
    assert_eq!(m.data.len(), 5);
    keel_ir::verify_module(&m).unwrap();
    assert!(
        m.functions
            .iter()
            .find(|f| f.name == "method")
            .unwrap()
            .weak_odr
    );
    // `call_*` observes which definition the link binds, including a strong
    // one that breaks the ODR promise, so `@method` stays out of line.
    let caller = m
        .functions
        .iter()
        .find(|f| f.name == format!("call_{tu}"))
        .unwrap();
    assert!(
        caller
            .layout_insts()
            .any(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. }))
    );
    m
}

#[test]
fn typed_and_text_objects_preserve_weak_atoms_and_relocations() {
    let m = module("a");
    let asm = compile(&m, Target::Aarch64AppleDarwin).unwrap();
    assert!(asm.contains(".subsections_via_symbols"));
    assert!(asm.lines().any(|line| {
        line.split_whitespace()
            .eq([".private_extern", "_type_name"])
    }));
    assert!(asm.lines().any(|line| {
        line.split_whitespace()
            .eq([".private_extern", "_type_info"])
    }));
    let program = compile_program(&m, Target::Aarch64AppleDarwin).unwrap();
    let typed = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(typed, keel_asm::assemble_object(&asm).unwrap());
    let obj = object::File::parse(typed.as_slice()).unwrap();
    for name in [
        "_method",
        "_shared",
        "_zero",
        "_vtable",
        "_type_name",
        "_type_info",
    ] {
        let sym = obj.symbols().find(|s| s.name().unwrap() == name).unwrap();
        assert!(sym.is_global() && sym.is_definition(), "{name}");
        assert!(
            matches!(sym.flags(), SymbolFlags::MachO { n_desc } if n_desc & 0x80 != 0),
            "{name}"
        );
    }
    for name in ["_type_name", "_type_info"] {
        let sym = obj.symbols().find(|s| s.name().unwrap() == name).unwrap();
        assert_eq!(sym.scope(), object::SymbolScope::Linkage);
    }
    let info = obj
        .symbols()
        .find(|s| s.name().unwrap() == "_type_info")
        .unwrap();
    let section = obj.section_by_index(info.section_index().unwrap()).unwrap();
    let offset = info.address() - section.address();
    let bits = &section.data().unwrap()[offset as usize..offset as usize + 8];
    assert_eq!(u64::from_le_bytes(bits.try_into().unwrap()), 1u64 << 63);
    let (_, reloc) = section.relocations().find(|(o, _)| *o == offset).unwrap();
    assert_eq!(reloc.size(), 64);
    let object::RelocationTarget::Symbol(target) = reloc.target() else {
        panic!("tag must retain symbol relocation");
    };
    assert_eq!(
        obj.symbol_by_index(target).unwrap().name().unwrap(),
        "_type_name"
    );
    for sym in obj.symbols().filter(|s| s.name().unwrap().ends_with("_a")) {
        assert!(matches!(sym.flags(), SymbolFlags::MachO { n_desc } if n_desc & 0x80 == 0));
    }
    assert!(
        compile(&m, Target::X86_64LinuxGnu)
            .unwrap_err()
            .message
            .contains("rtti_name")
    );
    assert!(
        compile_program(&m, Target::X86_64LinuxGnu)
            .unwrap_err()
            .message
            .contains("rtti_name")
    );
}

#[test]
fn malformed_linkage_is_rejected_even_without_verification() {
    for suffix in [
        "local weak_odr",
        "weak_odr thread_local",
        "local hidden",
        "weak_odr hidden thread_local",
    ] {
        let m = keel_ir::parse(&format!("data @d = {suffix} align 8 {{ i64 0 }}\n")).unwrap();
        assert!(keel_ir::verify_module(&m).is_err());
        assert!(compile(&m, Target::Aarch64AppleDarwin).is_err());
        assert!(compile_program(&m, Target::Aarch64AppleDarwin).is_err());
    }
}

// The test runner must hold suite/common/fast_opt_locked.py's host lease. This test
// does not acquire another lease, matching the other native codegen tests.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_linker_coalesces_translation_units_and_prefers_strong_definitions() {
    use std::{fs, process::Command};
    fn run(cmd: &mut Command) {
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{cmd:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let dir = std::env::temp_dir().join(format!("keel-weak-odr-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("driver.c"), r#"
#include <stdint.h>
extern void *method_a(void), *method_b(void), *shared_a(void), *shared_b(void);
extern void *zero_a(void), *zero_b(void), *vtable_a(void), *vtable_b(void);
extern long call_a(void), call_b(void);
extern long method(void), shared, zero;
extern const char type_name[];
extern const uintptr_t type_info[];
extern void *type_name_a(void), *type_name_b(void), *type_info_a(void), *type_info_b(void);
#ifdef STRONG
long method(void) { return 17; }
long shared = 19;
long zero = 23;
#define METHOD 17
#define DATA 19
#define ZERO 23
#else
#define METHOD 7
#define DATA 11
#define ZERO 0
#endif
int main(void) {
  if (method_a()!=method_b() || shared_a()!=shared_b() || zero_a()!=zero_b() || vtable_a()!=vtable_b()) return 1;
  if (method_a()!=(void*)method || shared_a()!=&shared || zero_a()!=&zero) return 5;
  if (type_name_a()!=type_name_b() || type_info_a()!=type_info_b() || type_name_a()!=type_name || type_info_a()!=type_info) return 6;
  if (*(uintptr_t*)type_info_a()!=((uintptr_t)type_name_a() + (UINT64_C(1)<<63))) return 7;
  void **v = vtable_a();
  if (v[0]!=method_a() || v[1]!=shared_a() || v[2]!=zero_a()) return 2;
  if (((long(*)(void))v[0])()!=METHOD || call_a()!=METHOD || call_b()!=METHOD) return 3;
  if (*(long*)v[1]!=DATA || *(long*)v[2]!=ZERO) return 4;
  return 0;
}
"#).unwrap();
    for route in ["clang", "typed", "text"] {
        let mut objects = Vec::new();
        for tu in ["a", "b"] {
            let m = module(tu);
            let path = dir.join(format!("{route}-{tu}.o"));
            let asm = compile(&m, Target::Aarch64AppleDarwin).unwrap();
            match route {
                "clang" => {
                    let src = dir.join(format!("{tu}.s"));
                    fs::write(&src, &asm).unwrap();
                    run(Command::new("clang")
                        .arg("-c")
                        .arg(src)
                        .arg("-o")
                        .arg(&path));
                }
                "typed" => {
                    let p = compile_program(&m, Target::Aarch64AppleDarwin).unwrap();
                    fs::write(
                        &path,
                        keel_asm::assemble_program_object(&p, Default::default()).unwrap(),
                    )
                    .unwrap();
                }
                _ => fs::write(&path, keel_asm::assemble_object(&asm).unwrap()).unwrap(),
            }
            objects.push(path);
        }
        for strong in [false, true] {
            let exe = dir.join(format!("{route}-{strong}"));
            let mut cmd = Command::new("clang");
            if strong {
                cmd.arg("-DSTRONG");
            }
            run(cmd
                .arg(dir.join("driver.c"))
                .args(&objects)
                .arg("-o")
                .arg(&exe));
            run(&mut Command::new(exe));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn elf_rtti_tag_is_rejected_independently() {
    {
        let src = "data @name = align 1 { i8 0 }\ndata @d = align 8 { rtti_name @name }\n";
        let m = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(compile(&m, Target::X86_64LinuxGnu).is_err());
        assert!(compile_program(&m, Target::X86_64LinuxGnu).is_err());
    }
}

#[test]
fn elf_weak_odr_emits_real_groups_on_typed_and_text_routes() {
    use object::ObjectComdat;
    let m = keel_ir::parse("data @d = weak_odr align 8 { i64 0 }\ndata @p = weak_odr const align 8 { ptr @d }\nfunc @f() -> i64 weak_odr {\nblock0:\n v0 = iconst.i64 7\n return v0\n}\nfunc @ordinary() weak {\nblock0:\n return\n}\n").unwrap();
    keel_ir::verify_module(&m).unwrap();
    let asm = compile(&m, Target::X86_64LinuxGnu).unwrap();
    let program = compile_program(&m, Target::X86_64LinuxGnu).unwrap();
    let typed = keel_asm::x86::assemble_program_object(&program).unwrap();
    let text = keel_asm::x86::assemble_object(&asm).unwrap();
    assert_eq!(typed, text);
    let f = object::File::parse(typed.as_slice()).unwrap();
    let groups: Vec<_> = f
        .comdats()
        .map(|g| (g.name().unwrap().to_owned(), g.sections().count()))
        .collect();
    assert_eq!(groups.len(), 3);
    for name in ["d", "p", "f"] {
        assert!(groups.iter().any(|g| g.0 == name));
        assert!(
            f.symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap()
                .is_weak()
        );
    }
    assert!(
        f.symbols()
            .find(|s| s.name().unwrap() == "ordinary")
            .unwrap()
            .is_weak()
    );
    assert!(!groups.iter().any(|g| g.0 == "ordinary"));
    assert_eq!(groups.iter().find(|g| g.0 == "p").unwrap().1, 2); // data and RELA
}
