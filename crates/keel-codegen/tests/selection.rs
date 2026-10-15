//! Cross-target code-shape checks complement the host execution/spill tests.
use keel_codegen::{SelectionOptions, Target, compile_with_options};

fn asm(src: &str, target: Target) -> String {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    compile_with_options(&module, target, SelectionOptions::ALL).unwrap()
}
const MADD: &str = "func @f(i64, i64, i64) -> i64 {
block0(v0: i64, v1: i64, v2: i64):
    v3 = imul v0, v1
    v4 = iadd v3, v2
    return v4
}";
#[test]
fn multiply_add_and_subtract_cover_the_product() {
    for (source, mnemonic) in [
        (MADD.to_string(), "madd"),
        (MADD.replace("iadd v3, v2", "isub v2, v3"), "msub"),
    ] {
        let output = asm(&source, Target::Aarch64AppleDarwin);
        assert!(output.contains(&format!("\t{mnemonic}\t")), "{output}");
        assert!(!output.contains("\tmul\t"), "{output}");
        assert!(
            !output.contains(" v3="),
            "covered result got a register: {output}"
        );
    }
}
#[test]
fn shared_products_and_cross_block_trees_stay_materialized() {
    let shared = MADD.replace("return v4", "v5 = iadd v4, v3\n return v5");
    let cross_block = MADD.replace("v4 = iadd", "jump block1\nblock1:\n v4 = iadd");
    for source in [shared, cross_block] {
        let output = asm(&source, Target::Aarch64AppleDarwin);
        assert!(output.contains("\tmul\t"), "{output}");
        assert!(!output.contains("\tmadd\t"), "{output}");
    }
}
#[test]
fn immediate_product_input_is_materialized_after_fusion() {
    let source = MADD.replace("v3 = imul v0, v1", "v5 = iconst.i64 7\n v3 = imul v0, v5");
    let output = asm(&source, Target::Aarch64AppleDarwin);
    assert!(output.contains("\tmadd\t"), "{output}");
    assert!(output.contains("#0x7"), "{output}");
}

#[test]
fn duplicate_immediate_operand_keeps_its_register_definition() {
    let source = "func @f() -> i64 {
block0:
 v0 = iconst.i64 7
 v1 = iadd v0, v0
 return v1
}";
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let output = asm(source, target);
        // Both targets spell the definition in hex and the add immediate in
        // decimal, so this checks that the register input is initialized.
        assert!(output.contains("0x7"), "{output}");
    }
}
#[test]
fn shifts_cover_both_targets_and_mask_the_count() {
    let source = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iconst.i64 66
 v3 = shl v1, v2
 v4 = iadd v0, v3
 return v4
}";
    let arm = asm(source, Target::Aarch64AppleDarwin);
    assert!(arm.contains("lsl #2"), "{arm}");
    assert!(!arm.contains("\tlsl\t"), "{arm}");
    let x86 = asm(source, Target::X86_64LinuxGnu);
    assert!(x86.contains(" * 4]"), "{x86}");
    assert!(!x86.contains("\tshl\t"), "{x86}");
    let large = asm(&source.replace("66", "4"), Target::X86_64LinuxGnu);
    assert!(large.contains("\tshl\t"), "{large}");
}
#[test]
fn hints_reuse_a_dying_first_input_even_when_a_lower_register_is_free() {
    let source = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = iadd v0, v0
 v3 = iadd v1, v2
 return v3
}";
    let output = asm(source, Target::X86_64LinuxGnu);
    let body = output
        .split(".Lf_block0:")
        .nth(1)
        .unwrap()
        .split("# return")
        .next()
        .unwrap();
    assert!(body.contains("add\trdi, rsi"), "{output}");
    assert!(
        !body.contains("mov\t"),
        "arithmetic needs no copies: {output}"
    );
}

#[test]
fn x86_scaled_address_assembles() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = "func @f(ptr, i64) -> ptr {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 3
 v3 = shl v1, v2
 v4 = ptradd v0, v3
 return v4
}";
    let output = asm(source, Target::X86_64LinuxGnu);
    let object = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("selection-x86.o");
    let mut clang = Command::new("clang")
        .args([
            "--target=x86_64-unknown-linux-gnu",
            "-x",
            "assembler",
            "-c",
            "-",
        ])
        .arg("-o")
        .arg(object)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    clang
        .stdin
        .take()
        .unwrap()
        .write_all(output.as_bytes())
        .unwrap();
    let result = clang.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn different_debug_lines_remain_separately_breakable() {
    let source = format!(
        "file f0 = \"test.c\"\n{}",
        MADD.replace("imul v0, v1", "imul v0, v1 !f0:2:1")
            .replace("iadd v3, v2", "iadd v3, v2 !f0:3:1")
    );
    let output = asm(&source, Target::Aarch64AppleDarwin);
    assert!(output.contains("\tmul\t"), "{output}");
    assert!(!output.contains("\tmadd\t"), "{output}");
}

#[test]
fn tree_selection_is_opt_in_and_independent_from_other_passes() {
    let module = keel_ir::parse(MADD).unwrap();
    for options in [
        SelectionOptions::default(),
        SelectionOptions {
            folded_operands: true,
            ..SelectionOptions::default()
        },
        SelectionOptions {
            register_hints: true,
            ..SelectionOptions::default()
        },
    ] {
        let output = compile_with_options(&module, Target::Aarch64AppleDarwin, options).unwrap();
        assert!(output.contains("\tmul\t"), "{output}");
        assert!(!output.contains("\tmadd\t"), "{output}");
    }
    let output = compile_with_options(
        &module,
        Target::Aarch64AppleDarwin,
        SelectionOptions {
            trees: true,
            ..SelectionOptions::default()
        },
    )
    .unwrap();
    assert!(output.contains("\tmadd\t"), "{output}");
    assert!(!output.contains("\tmul\t"), "{output}");
}

#[test]
fn scalar_canonicalization_exposes_aarch64_immediate_operands() {
    let mut module = keel_ir::parse(
        "func @f(i64) -> i64 {
block0(v0: i64):
 v1 = iconst.i64 37
 v2 = iadd v1, v0
 return v2
}",
    )
    .unwrap();
    let before = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
    assert!(before.contains("\tmovz\t"), "{before}");
    keel_opt::run_passes(&mut module, &["canonicalize"]).unwrap();
    let after = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
    assert!(after.contains("#37"), "{after}");
    assert!(!after.contains("\tmovz\t"), "{after}");
}

#[test]
fn comparison_canonicalization_exposes_aarch64_immediates() {
    for cond in [
        "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
    ] {
        let mut module = keel_ir::parse(&format!("func @f(i64) -> i64 {{\nblock0(v0: i64):\n v1 = iconst.i64 37\n v2 = icmp.{cond} v1, v0\n v3 = uext.i64 v2\n return v3\n}}")).unwrap();
        keel_opt::run_passes(&mut module, &["canonicalize"]).unwrap();
        let after = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
        assert!(after.contains("#37"), "{after}");
        assert!(!after.contains("\tmovz\t"), "{after}");
    }
}
