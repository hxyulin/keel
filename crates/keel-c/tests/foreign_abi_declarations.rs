//! Foreign function pointer storage retains ABI identity without lowering calls.
use keel_ir::Target;
fn compile(source: &str) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "foreign-abi.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
}
#[test]
fn microsoft_function_pointer_fields_preserve_layout_and_identity() {
    let source = r#"
 typedef unsigned long Firmware(int);
 typedef unsigned long (__attribute__((ms_abi)) *Foreign)(int);
 typedef unsigned long (*Native)(int);
 struct Table { Firmware __attribute__((ms_abi)) *entry; Foreign other; };
 struct Table table={0};
 _Static_assert(sizeof(struct Table)==16,"table");
 _Static_assert(!__builtin_types_compatible_p(Foreign,Native),"ABI identity");
 void *storage(Foreign input) { table.other=input; return table.entry; }
 int parameter(unsigned long (__attribute__((ms_abi)) *callback)(int)) { return callback!=0; }
 "#;
    let module = compile(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
}
#[test]
fn foreign_calls_definitions_and_implicit_abi_changes_reject() {
    for (source, expected) in [
        (
            "typedef int (__attribute__((ms_abi)) *Foreign)(void);int f(Foreign p){return p();}",
            "unsupported Microsoft",
        ),
        (
            "__attribute__((ms_abi)) int f(void){return 0;}",
            "unsupported Microsoft",
        ),
        (
            "typedef int (__attribute__((ms_abi)) *Foreign)(void);typedef int (*Native)(void);void f(Foreign p){Native q=p;}",
            "calling conventions",
        ),
        (
            "int f(void);__attribute__((ms_abi)) int f(void);",
            "incompatible",
        ),
    ] {
        let error = compile(source).unwrap_err();
        assert!(error.message.contains(expected), "{source}: {error}");
    }
}

#[test]
fn direct_foreign_symbol_addresses_remain_unavailable() {
    let source = "__attribute__((ms_abi)) int foreign(void);void *address(void){return &foreign;}";
    let error = compile(source).unwrap_err();
    assert!(error.message.contains("Microsoft x64"), "{error}");
}
