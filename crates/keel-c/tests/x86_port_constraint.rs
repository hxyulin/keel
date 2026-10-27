use keel_ir::{AsmOperand, InstData, Target};

fn compile(source: &str) -> Result<keel_ir::Module, String> {
    keel_c::compile(
        source,
        "port-constraint.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn port_choice_survives_inlining_and_selects_imm8_or_dx() {
    for constraint in ["Nd", "dN"] {
        for optimize in [false, true] {
            for (value, operand) in [
                ("0", "$0"),
                ("255", "$255"),
                ("256", "%dx"),
                ("65535", "%dx"),
                ("port", "%dx"),
            ] {
                let source = format!(
                    r#"
static __attribute__((always_inline)) inline unsigned char input(unsigned short port) {{
    unsigned char value;
    asm volatile("inb %w1, %b0" : "=a"(value) : "{constraint}"(port));
    return value;
}}
unsigned char read_port(unsigned short port) {{ return input({value}); }}
"#
                );
                let mut module = compile(&source).unwrap();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                keel_ir::verify_module(&module).unwrap();
                let f = module
                    .functions
                    .iter()
                    .find(|f| f.name == "read_port")
                    .unwrap();
                let asm = f
                    .layout_insts()
                    .find_map(|(_, i)| match f.inst_data(i) {
                        InstData::InlineAsm { asm, .. } => Some(asm),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(asm.inputs, [AsmOperand::PortImm8]);
                let roundtrip = keel_ir::parse(&module.to_string()).unwrap();
                keel_ir::verify_module(&roundtrip).unwrap();
                let text = keel_codegen::compile(&module, Target::X86_64LinuxGnu).unwrap();
                assert!(
                    text.contains(&format!("inb {operand}, %al")),
                    "{constraint} {value}: {text}"
                );
                let program =
                    keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
                assert_eq!(
                    keel_asm::x86::assemble_program_object(&program).unwrap(),
                    keel_asm::x86::assemble_object(&text).unwrap()
                );
            }
        }
    }
}

#[test]
fn port_register_choice_respects_clobbers_and_output_contracts() {
    let module = compile(
        r#"unsigned char f(unsigned short port) {
        unsigned char result;
        asm("inb %w1,%b0" : "=a"(result) : "Nd"(port) : "rdx");
        return result;
    }"#,
    )
    .unwrap();
    assert!(
        keel_codegen::compile(&module, Target::X86_64LinuxGnu)
            .unwrap_err()
            .to_string()
            .contains("overlaps")
    );
    for body in [r#"asm("" : "=Nd"(x));"#, r#"asm("" :: "N"(x));"#] {
        assert!(compile(&format!("void f(int x) {{ {body} }}")).is_err());
    }
}

#[test]
fn port_choices_preserve_live_values_with_a_nonprivileged_template() {
    let source = r#"
static __attribute__((always_inline)) inline unsigned short mirror(unsigned short port) {
    unsigned short result;
    asm("movw %w1,%w0" : "=a"(result) : "Nd"(port));
    return result;
}
unsigned long dynamic_port(unsigned short port, unsigned long a, unsigned long b, unsigned long c) {
    return mirror(port) ^ a ^ b ^ c;
}
unsigned long immediate_port(unsigned long a, unsigned long b, unsigned long c) {
    return mirror(255) ^ a ^ b ^ c;
}
unsigned long larger_port(unsigned long a, unsigned long b, unsigned long c) {
    return mirror(256) ^ a ^ b ^ c;
}
"#;
    for optimize in [false, true] {
        let mut module = compile(source).unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
        let object = keel_asm::x86::assemble_program_object(&program).unwrap();
        if let Some(path) = std::env::var_os("KEEL_X86_PORT_CONSTRAINT_EXPORT") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                std::path::Path::new(&path).join(format!("port-opt{optimize}.o")),
                object,
            )
            .unwrap();
        }
    }
}
