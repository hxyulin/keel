//! Hidden definitions stay global within a link but are not exported: ELF
//! `.hidden` (STV_HIDDEN) and Mach-O `.private_extern`, in text and objects.
use keel_codegen::{Target, compile, compile_program};
use object::{Object, ObjectSymbol, SymbolScope};

const SOURCE: &str = "\
data @mutable = hidden align 4 { i32 7 }
data @zero = hidden align 8 { zero 16 }
data @constant = hidden const align 1 { b\"Name\\0\" }
data @table = hidden const align 8 { ptr @f, ptr @mutable }
data @tls = hidden thread_local align 4 { i32 1 }
data @tls_zero = hidden thread_local align 4 { i32 0 }
data @exported = align 4 { i32 3 }
func @f() -> i64 hidden {
block0:
    v0 = iconst.i64 7
    return v0
}
func @g() -> i64 {
block0:
    v0 = call @f()
    return v0
}
";

const HIDDEN: [&str; 7] = [
    "mutable", "zero", "constant", "table", "tls", "tls_zero", "f",
];

fn module() -> keel_ir::Module {
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

fn check_scopes(obj: &[u8], prefix: &str) {
    let obj = object::File::parse(obj).unwrap();
    for name in HIDDEN.iter().map(|n| format!("{prefix}{n}")) {
        let sym = obj.symbols().find(|s| s.name() == Ok(&name)).unwrap();
        assert!(!sym.is_undefined(), "{name}");
        assert_eq!(sym.scope(), SymbolScope::Linkage, "{name}");
    }
    for name in ["exported", "g"].map(|n| format!("{prefix}{n}")) {
        let sym = obj.symbols().find(|s| s.name() == Ok(&name)).unwrap();
        assert_eq!(sym.scope(), SymbolScope::Dynamic, "{name}");
    }
}

#[test]
fn elf_hidden_definitions_are_global_and_stv_hidden() {
    let m = module();
    let asm = compile(&m, Target::X86_64LinuxGnu).unwrap();
    for name in HIDDEN {
        assert!(
            asm.lines()
                .any(|l| l.split_whitespace().eq([".hidden", &format!("\"{name}\"")])),
            "{name}: {asm}"
        );
        assert!(
            asm.lines()
                .any(|l| l.trim() == format!(".globl \"{name}\"")),
            "{name}"
        );
    }
    assert!(!asm.contains(".hidden\t\"exported\"") && !asm.contains(".hidden\t\"g\""));
    let program = compile_program(&m, Target::X86_64LinuxGnu).unwrap();
    let typed = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(typed, keel_asm::x86::assemble_object(&asm).unwrap());
    check_scopes(&typed, "");
    let obj = object::File::parse(typed.as_slice()).unwrap();
    for sym in obj
        .symbols()
        .filter(|s| HIDDEN.contains(&s.name().unwrap()))
    {
        let object::SymbolFlags::Elf { st_info, st_other } = sym.flags() else {
            panic!("ELF symbol flags");
        };
        assert_eq!((st_info >> 4, st_other & 3), (1, 2), "{:?}", sym.name());
    }
}

#[test]
fn macho_hidden_definitions_are_private_extern() {
    let m = module();
    let asm = compile(&m, Target::Aarch64AppleDarwin).unwrap();
    for name in HIDDEN {
        assert!(
            asm.lines().any(|l| l
                .split_whitespace()
                .eq([".private_extern", &format!("_{name}")])),
            "{name}: {asm}"
        );
    }
    assert!(!asm.contains("_exported\n.private") && !asm.contains(".private_extern\t_g\n"));
    let program = compile_program(&m, Target::Aarch64AppleDarwin).unwrap();
    let typed = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(typed, keel_asm::assemble_object(&asm).unwrap());
    check_scopes(&typed, "_");
}

#[test]
fn local_or_undefined_hidden_symbols_are_rejected() {
    for src in [
        "data @d = local hidden align 4 { i32 0 }\n",
        "func @f() local hidden {\nblock0:\n return\n}\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        for target in Target::ALL {
            assert!(compile(&m, target).is_err(), "{src}");
            assert!(compile_program(&m, target).is_err(), "{src}");
        }
    }
    let mut m = keel_ir::parse("decl data @d\n").unwrap();
    m.data[0].hidden = true;
    for target in Target::ALL {
        assert!(compile(&m, target).is_err());
    }
}
