use keel_asm::{RelocationKind, assemble, assemble_object};
use object::{Object, ObjectSymbol};
use std::process::Command;
mod common;
use common::{Temp, run};

const SOURCE: &str = r#"
.subsections_via_symbols
.text
.globl _dispatch, _target, _unused
_dispatch:
Ldispatch: b Ltarget
_unused: mov w0,#99
ret
_target:
Ltarget: mov w0,#42
ret
Ltarget_end:
.data
.globl _first, _second, _bounds, _endptr
_first:
Lfirst: .quad 7
Lfirst_end:
_second:
Lsecond: .quad Ltarget
_bounds:
.quad Ltarget_end-Ltarget
.quad Ltarget_end-Ldispatch
.quad Lfirst_end-Lfirst
.quad Lsecond-Lfirst
_endptr: .quad Ltarget_end
"#;
#[test]
fn cross_atom_relocations_preserve_temporary_label_ownership() {
    let a = assemble(SOURCE).unwrap();
    assert!(a.subsections_via_symbols);
    let branch = a
        .relocations
        .iter()
        .find(|r| r.kind == RelocationKind::Branch26)
        .unwrap();
    assert_eq!(branch.symbol, "_target");
    let end = a
        .relocations
        .iter()
        .find(|r| r.symbol == "_target" && r.subtract_symbol.is_none() && r.addend == 8)
        .unwrap();
    assert_eq!(end.kind, RelocationKind::Unsigned);
    let difference = a
        .relocations
        .iter()
        .find(|r| r.subtract_symbol.as_deref() == Some("_dispatch"))
        .unwrap();
    assert_eq!(difference.symbol, "_target");
    assert_eq!(difference.addend, 8);
    let object = assemble_object(SOURCE).unwrap();
    let file = object::File::parse(object.as_slice()).unwrap();
    assert!(matches!(file.flags(),object::FileFlags::MachO{flags} if flags & 0x2000 != 0));
    assert!(file.symbols().all(|s| !s.name().unwrap().starts_with('L')));
}
#[test]
fn fixed_displacements_require_one_atom() {
    for instruction in [
        "adr x0,Lother",
        "b.eq Lother",
        "cbz x0,Lother",
        "tbz x0,#0,Lother",
    ] {
        let source = format!(".subsections_via_symbols\n_one: {instruction}\n_two:\nLother: ret");
        let error = assemble(&source).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("same linker atom"), "{error}");
    }
    assert!(
        assemble(".subsections_via_symbols\n_one: .uleb128 Ltwo-Lone\nLone:\n_two:\nLtwo:")
            .is_err()
    );
    common::compare_clang(
        ".subsections_via_symbols\n_one:\nLstart: nop\nadr x0,Lstart\nb.eq Lend\nb Lstart\nLend: ret",
    );
    common::compare_clang(
        ".subsections_via_symbols\nLbegin:\n_alias: adr x0,Lbegin\nb.eq Lbegin\nret",
    );
    // A late global declaration also starts an atom, despite the L prefix.
    assert!(
        assemble(".subsections_via_symbols\n_one: adr x0,Lexport\nLexport: ret\n.globl Lexport")
            .is_err()
    );
}
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn linker_reorders_and_dead_strips_atoms() {
    let temp = Temp::new();
    let obj = temp.0.join("atoms.o");
    let driver = temp.0.join("main.c");
    let order = temp.0.join("order");
    let exe = temp.0.join("run");
    std::fs::write(&obj, assemble_object(SOURCE).unwrap()).unwrap();
    std::fs::write(
        &order,
        "_target\n_dispatch\n_second\n_bounds\n_endptr\n_first\n",
    )
    .unwrap();
    std::fs::write(
        &driver,
        r#"
#include <stdint.h>
extern int dispatch(void),target(void);
extern uint64_t first,bounds[4];
extern void *second,*endptr;
int main(void) {
 uintptr_t f=(uintptr_t)&first,s=(uintptr_t)&second;
 return dispatch()!=42 || second!=(void*)target || endptr!=(void*)((uintptr_t)target+8)
  || bounds[0]!=8 || (int64_t)bounds[1]!=(int64_t)((uintptr_t)target+8-(uintptr_t)dispatch)
  || bounds[2]!=8 || (int64_t)bounds[3]!=(int64_t)(s-f);
}
"#,
    )
    .unwrap();
    run(Command::new("clang")
        .arg(driver)
        .arg(obj)
        .arg("-Wl,-dead_strip")
        .arg("-Xlinker")
        .arg("-order_file")
        .arg("-Xlinker")
        .arg(order)
        .arg("-o")
        .arg(&exe));
    assert!(common::execute(&exe).success());
    let bytes = std::fs::read(exe).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert!(!file.symbols().any(|s| s.name().unwrap() == "_unused"));
    let address = |name| {
        file.symbols()
            .find(|s| s.name().unwrap() == name)
            .unwrap()
            .address()
    };
    assert!(address("_target") < address("_dispatch"));
    assert!(address("_second") < address("_first"));
}
