//! C stack-call dependencies are inputs, despite their GNU output spelling.
use keel_ir::{AsmOperand, InstData, Intrinsic, Target, Type};

fn compile(body: &str) -> Result<keel_ir::Module, String> {
    let source = format!(
        "register unsigned long sp asm(\"rsp\"); register unsigned long alias_sp asm(\"rsp\");\n{body}"
    );
    keel_c::compile(
        &source,
        "asm-stack-call.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn stack_and_memory_outputs_preserve_source_numbering_without_writeback() {
    let source = r#"
unsigned long named(unsigned long value, unsigned long *memory) {
    unsigned long result = value;
    asm volatile("movq %q[stack],%[result]; movq %[input],%[slot]"
        : [stack] "+r"(sp), [slot] "+m"(*memory), [result] "+r"(result)
        : [input] "r"(value) : "memory", "cc");
    return result;
}
unsigned long numeric(unsigned long value, unsigned long *memory) {
    unsigned long result;
    asm volatile("movq %3,%0; movq %q2,%1"
        : "=r"(result), "=m"(*memory), "+r"(sp) : "0"(value) : "memory");
    return result;
}
void only_stack(void) { asm volatile("" : "+r"(sp)); }
"#;
    for optimize in [false, true] {
        let mut module = compile(source).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        for (name, template, constraints, types) in [
            (
                "named",
                "movq %q1,%0; movq %3,%2",
                "=r,rsp,mreadwrite,r,0,~{memory},~{cc}",
                vec![Type::I64, Type::Ptr, Type::I64, Type::I64],
            ),
            (
                "numeric",
                "movq %3,%0; movq %q2,%1",
                "=r,mwrite,rsp,0,~{memory}",
                vec![Type::Ptr, Type::I64, Type::I64],
            ),
            ("only_stack", "", "rsp", vec![Type::I64]),
        ] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            let (inst, asm, args) = function
                .layout_insts()
                .find_map(|(_, inst)| {
                    if let InstData::InlineAsm { asm, args } = function.inst_data(inst) {
                        Some((inst, asm, args))
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(asm.template, template, "{name}");
            assert_eq!(asm.constraints(), constraints, "{name}");
            assert_eq!(
                asm.outputs.len(),
                usize::from(name != "only_stack"),
                "{name}"
            );
            assert_eq!(function.inst_results(inst).len(), asm.outputs.len());
            assert_eq!(
                args.iter()
                    .map(|&v| function.value_type(function.resolve(v)))
                    .collect::<Vec<_>>(),
                types
            );
            let index = asm
                .inputs
                .iter()
                .position(|op| *op == AsmOperand::StackPointer)
                .unwrap();
            assert!(keel_ir::is_stack_pointer_snapshot(function, args[index]));
            assert_eq!(
                function
                    .layout_insts()
                    .filter(|(_, i)| matches!(
                        function.inst_data(*i),
                        InstData::Intrinsic {
                            id: Intrinsic::StackSave,
                            ..
                        }
                    ))
                    .count(),
                1
            );
        }
        let reparsed = keel_ir::parse(&module.to_string()).unwrap();
        keel_ir::verify_module(&reparsed).unwrap();
    }
}

#[test]
fn only_one_exact_readwrite_rsp_operand_is_allowed() {
    for (body, expected) in [
        ("void f(void){asm(\"\":\"=r\"(sp));}", "exactly +r"),
        ("void f(void){asm(\"\":\"+&r\"(sp));}", "exactly +r"),
        ("void f(void){asm(\"\":\"+q\"(sp));}", "exactly +r"),
        ("void f(void){asm(\"\":\"+m\"(sp));}", "exactly +r"),
        (
            "void f(void){asm(\"\":\"+r\"(sp),\"+r\"(alias_sp));}",
            "only one stack-pointer",
        ),
        (
            "void f(unsigned long x){asm(\"\":\"+r\"(sp):\"0\"(x));}",
            "register output",
        ),
        (
            "void f(unsigned long x){asm(\"\":[stack]\"+r\"(sp):\"[stack]\"(x));}",
            "register output",
        ),
        (
            "void f(void){asm goto(\"\":\"+r\"(sp):::done);done:;}",
            "asm goto output",
        ),
        ("void f(void){sp=0;}", "assignable"),
        ("void *f(void){return &sp;}", "assignable"),
    ] {
        let error = compile(body).unwrap_err();
        assert!(
            error.contains(expected),
            "{body}: expected {expected}: {error}"
        );
    }
}
