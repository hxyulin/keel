use keel_ir::{InstData, Type};
use keel_ir_interp::{Interpreter, Outcome, Val};

fn source(flags: &str, between: &str, cc: &str) -> String {
    format!(
        "func @f(ptr) -> i32 {{
block0(v0: ptr):
 v1 = load.i8 {flags} v0
 v2 = uext.i32 v1
 v3 = iconst.i64 1
 v4 = ptradd v0, v3
 {between}
 v5 = load.i8 {flags} v4
 v6 = uext.i32 v5
 v7 = iconst.i32 8
 v8 = shl v6, v7
 v9 = {cc} v2, v8
 return v9
}}"
    )
}

#[test]
fn adjacent_byte_assembly_reads_exactly_two_little_endian_bytes() {
    for flags in ["trap_ub", "notrap"] {
        let (before, after) = super::optimize(&source(flags, "", "or"));
        let loads: Vec<_> = after.functions[0]
            .layout_insts()
            .filter_map(|(_, i)| {
                if let InstData::Load { ty, flags, .. } = after.functions[0].inst_data(i) {
                    Some((*ty, *flags))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(loads.len(), 1, "{after}");
        assert_eq!(loads[0].0, Type::I16, "{after}");
        assert_eq!(loads[0].1.align, 1);
        for bytes in [[0, 0], [0xff, 0], [0, 0xff], [0x34, 0x12], [0xff, 0xff]] {
            for module in [&before, &after] {
                let mut interp = Interpreter::new(module);
                // Exactly two accessible bytes, deliberately unaligned.
                let mem = interp.memory_mut().alloc_bytes(&[9, bytes[0], bytes[1]], 1);
                assert_eq!(
                    interp.call("f", &[Val::Ptr(mem + 1)]),
                    Outcome::Return(vec![Val::I32(
                        u32::from(bytes[0]) | u32::from(bytes[1]) << 8
                    )])
                );
            }
        }
        let mut again = after.clone();
        keel_opt::run_passes(&mut again, &["instcombine", "dce"]).unwrap();
        assert_eq!(again.to_string(), after.to_string());
    }
}

#[test]
fn byte_assembly_keeps_faults_effects_shared_values_and_wrong_orders() {
    let base = source("trap_ub", "", "or");
    let cases = [
        source("", "", "or"),
        source("volatile", "", "or"),
        source("trap_ub", "store v1, v0", "or"),
        source("trap_ub", "v10 = load.i8 v0", "or"),
        base.replace("v3 = iconst.i64 1", "v3 = iconst.i64 2"),
        base.replace("v7 = iconst.i32 8", "v7 = iconst.i32 7"),
        base.replace("v1 = load.i8 trap_ub v0\n v2 = uext.i32 v1\n v3 = iconst.i64 1\n v4 = ptradd v0, v3\n \n v5 = load.i8 trap_ub v4", "v3 = iconst.i64 1\n v4 = ptradd v0, v3\n v5 = load.i8 trap_ub v4\n v1 = load.i8 trap_ub v0\n v2 = uext.i32 v1"),
        base.replace("v6 = uext.i32", "v6 = sext.i32"),
        base.replace("return v9", "v10 = xor v9, v2\n return v10"),
    ];
    for src in cases {
        let (_, after) = super::optimize(&src);
        assert!(
            !after.functions[0].layout_insts().any(|(_, i)| matches!(
                after.functions[0].inst_data(i),
                InstData::Load { ty: Type::I16, .. }
            )),
            "{src}\n{after}"
        );
    }
}
