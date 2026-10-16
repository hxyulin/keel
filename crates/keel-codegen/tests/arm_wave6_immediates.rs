//! ARM-specific immediate eligibility and preallocation/materialization checks.
//! Native execution and structured/text parity are separate integration gates.
use keel_codegen::{SelectionOptions, Target, compile_with_options};

fn asm(ty: &str, body: &str, result: &str) -> String {
    let src = format!("func @f({ty}) -> {result} {{\nblock0(v0: {ty}):\n{body}\n}}");
    let module = keel_ir::parse(&src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    compile_with_options(&module, Target::Aarch64AppleDarwin, SelectionOptions::ALL).unwrap()
}

#[test]
fn arithmetic_signed_and_shifted_immediates_use_execution_width() {
    for ty in ["i32", "i64"] {
        for (k, immediate) in [
            (-4095i64, "#4095"),
            (-4096, "#1, lsl #12"),
            (4096, "#1, lsl #12"),
            (16773120, "#4095, lsl #12"),
        ] {
            for op in ["iadd", "isub"] {
                let mnemonic = if (op == "isub") ^ (k < 0) {
                    "sub"
                } else {
                    "add"
                };
                let output = asm(
                    ty,
                    &format!("v1 = iconst.{ty} {k}\nv2 = {op} v0, v1\nreturn v2"),
                    ty,
                );
                assert!(output.contains(&format!("\t{mnemonic}\t")), "{output}");
                assert!(output.contains(immediate), "{output}");
                assert!(
                    !output.contains("\tmovn\t") && !output.contains("\tmovz\t"),
                    "{output}"
                );
            }
        }
    }
    // The i32 bit pattern is +1, despite its signed i64 spelling.
    let output = asm(
        "i32",
        "v1 = iconst.i32 -4294967295\nv2 = iadd v1, v0\nreturn v2",
        "i32",
    );
    assert!(
        output.contains("\tadd\t") && output.contains("#1"),
        "{output}"
    );
}

#[test]
fn negative_compare_only_uses_z_predicates() {
    for ty in ["i32", "i64"] {
        for cc in [
            "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
        ] {
            let output = asm(
                ty,
                &format!("v1 = iconst.{ty} -4096\nv2 = icmp.{cc} v0, v1\nreturn v2"),
                "i8",
            );
            assert_eq!(
                output.contains("\tcmn\t"),
                matches!(cc, "eq" | "ne"),
                "{output}"
            );
        }
        for k in [0i64, 4095, 4096, 16773120] {
            let output = asm(
                ty,
                &format!("v1 = iconst.{ty} {k}\nv2 = icmp.ult v0, v1\nreturn v2"),
                "i8",
            );
            assert!(output.contains("\tcmp\t"), "{output}");
            assert!(!output.contains("\tmovz\t"), "{output}");
        }
    }
}

#[test]
fn shared_and_duplicate_constants_still_materialize_when_needed() {
    for body in [
        "v1 = iconst.i64 -1\nv2 = iadd v0, v1\nv3 = xor v2, v1\nreturn v3",
        "v1 = iconst.i64 -1\nv2 = iadd v1, v1\nreturn v2",
    ] {
        let output = asm("i64", body, "i64");
        assert!(output.contains("\tmovn\t"), "{output}");
        assert!(
            output.contains("\tadd\t") || output.contains("\tsub\t"),
            "{output}"
        );
    }
}

#[test]
fn unencodable_boundaries_keep_register_operands() {
    for (ty, k) in [
        ("i32", -2147483648i64),
        ("i64", i64::MIN),
        ("i64", 4097),
        ("i64", 16777216),
        ("i64", -16777216),
    ] {
        let output = asm(
            ty,
            &format!("v1 = iconst.{ty} {k}\nv2 = iadd v0, v1\nreturn v2"),
            ty,
        );
        let add = output
            .lines()
            .find(|line| line.contains("\tadd\t"))
            .unwrap();
        assert!(!add.contains('#'), "{output}");
    }
    for ty in ["i8", "i16"] {
        let output = asm(
            ty,
            &format!("v1 = iconst.{ty} -1\nv2 = icmp.eq v0, v1\nreturn v2"),
            "i8",
        );
        assert!(!output.contains("\tcmn\t"), "{output}");
    }
}
