use keel_ir::{InstData as D, Type};
use keel_ir_interp::Interpreter;

fn source(start: i64, bound: i64, ty: Type, rotated: bool) -> String {
    let entry = if rotated {
        "jump block2(v2,v0)"
    } else {
        "jump block1(v2,v0)"
    };
    let header = if rotated {
        String::new()
    } else {
        format!("block1(v4:{ty},v5:{ty}):\n v6=icmp.ult v4,v3\n brif v6,block2,block3(v5)\n")
    };
    let body = if rotated {
        format!("block2(v4:{ty},v5:{ty}):")
    } else {
        "block2:".to_owned()
    };
    let back = if rotated {
        "v12=icmp.ult v11,v3\n brif v12,block2(v11,v9),block3(v9)"
    } else {
        "jump block1(v11,v9)"
    };
    format!(
        "func @f({ty},{ty})->{ty} {{\nblock0(v0:{ty},v1:{ty}):\n v2=iconst.{ty} {start}\n v3=iconst.{ty} {bound}\n {entry}\n{header}{body}\n v7=iconst.{ty} 3\n v8=imul v5,v7\n v9=xor v8,v4\n v10=iconst.{ty} 1\n v11=iadd v4,v10\n {back}\nblock3(v13:{ty}):\n return v13\n}}"
    )
}
fn optimize(src: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["loop-unroll-exact"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
#[test]
fn exact_recurrence_preserves_scalar_order_and_trip_boundaries() {
    for ty in [Type::I32, Type::I64] {
        for rotated in [false, true] {
            for (start, bound) in [(0, 0), (0, 1), (0, 8), (9, 8)] {
                let src = source(start, bound, ty, rotated);
                let before = keel_ir::parse(&src).unwrap();
                let after = optimize(&src);
                assert!(!after.to_string().contains("brif"), "{after}");
                for seed in [0, 1, u64::MAX, u64::MAX - 3, 0x9e3779b97f4a7c15] {
                    let n = (bound - start).max(i64::from(rotated));
                    let mask = if ty == Type::I32 {
                        u32::MAX as u64
                    } else {
                        u64::MAX
                    };
                    let want = (0..n).fold(seed & mask, |s, i| {
                        (s.wrapping_mul(3) ^ (start + i) as u64) & mask
                    });
                    let args = [seed, 0];
                    assert_eq!(super::support::execute(&before.functions[0], &args), want);
                    assert_eq!(super::support::execute(&after.functions[0], &args), want);
                }
                let mut again = after.clone();
                keel_opt::run_passes(&mut again, &["loop-unroll-exact"]).unwrap();
                assert_eq!(again.to_string(), after.to_string());
            }
        }
    }
}
#[test]
fn signed_and_unsigned_maximum_updates_do_not_wrap() {
    for ty in [Type::I32, Type::I64] {
        for signed in [false, true] {
            let max = match (ty, signed) {
                (Type::I32, false) => u32::MAX as u64,
                (Type::I32, true) => i32::MAX as u64,
                (Type::I64, true) => i64::MAX as u64,
                _ => u64::MAX,
            };
            for rotated in [false, true] {
                let src = source((max - 2) as i64, max as i64, ty, rotated)
                    .replace("icmp.ult", if signed { "icmp.slt" } else { "icmp.ult" });
                let before = keel_ir::parse(&src).unwrap();
                let after = optimize(&src);
                assert!(!after.to_string().contains("brif"));
                for seed in [0, 1, u64::MAX] {
                    assert_eq!(
                        super::support::execute(&before.functions[0], &[seed, 0]),
                        super::support::execute(&after.functions[0], &[seed, 0])
                    );
                }
                let overflow = source(max as i64, max as i64, ty, true)
                    .replace("icmp.ult", if signed { "icmp.slt" } else { "icmp.ult" });
                assert_eq!(
                    optimize(&overflow).to_string(),
                    keel_ir::parse(&overflow).unwrap().to_string()
                );
            }
        }
    }
}
#[test]
fn crc_shape_keeps_exact_table_load_order_and_flags() {
    let src = "func @crc(ptr,i32)->i32 {
block0(v0:ptr,v1:i32):
 v2=iconst.i32 0
 jump block1(v2,v1)
block1(v3:i32,v4:i32):
 v5=iconst.i32 1
 v6=ushr v4,v5
 v7=and v4,v5
 v8=uext.i64 v7
 v9=iconst.i64 2
 v10=shl v8,v9
 v11=ptradd v0,v10
 v12=load.i32 align 4 trap_ub v11
 v13=xor v6,v12
 v14=iadd v3,v5
 v15=iconst.i32 8
 v16=icmp.slt v14,v15
 brif v16,block1(v14,v13),block2(v13)
block2(v17:i32):
 return v17
}";
    let before = keel_ir::parse(src).unwrap();
    let after = optimize(src);
    assert_eq!(
        after.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(after.functions[0].inst_data(*i), D::Load { .. }))
            .count(),
        8
    );
    assert!(!after.to_string().contains("brif"));
    let bytes = [0u32.to_le_bytes(), 0xedb88320u32.to_le_bytes()].concat();
    for seed in [0, 1, u32::MAX, 0x80000000, 0x9e3779b9] {
        let mut outputs = Vec::new();
        for m in [&before, &after] {
            let f = &m.functions[0];
            let mut i = Interpreter::for_function(f).with_load_trace();
            let ptr = i.memory_mut().alloc_bytes(&bytes, 4);
            let result = super::support::first_result(
                i.call(
                    &f.name,
                    &super::support::args(f, &[ptr as u128, seed as u128]),
                ),
                f,
            );
            outputs.push((
                result,
                i.load_trace().iter().map(|a| a - ptr).collect::<Vec<_>>(),
            ));
        }
        let mut c = seed;
        let mut trace = Vec::new();
        for _ in 0..8 {
            trace.push(u64::from(c & 1) * 4);
            c = (c >> 1) ^ (0xedb88320u32.wrapping_mul(c & 1));
        }
        assert_eq!(outputs[0], (c as u128, trace));
        assert_eq!(outputs[0], outputs[1]);
    }
}
#[test]
fn unsupported_effects_variable_trips_escapes_and_growth_are_unchanged() {
    let src = source(0, 8, Type::I64, false);
    let cases=[
        source(0,9,Type::I64,false),
        src.replace("icmp.ult v4,v3","icmp.ult v4,v1"),
        src.replace("v11=iadd v4,v10","v11=iadd v4,v7"),
        src.replace("v8=imul v5,v7","v8=udiv v5,v7"),
        format!("decl @effect()\n{}",src.replace(" v8="," call @effect()\n v8=")),
        src.replace("v8=imul v5,v7","v8=imul v5,v7\n v50=iconst.i64 0\n v51=iconst.i64 0\n v52=iconst.i64 0\n v53=iconst.i64 0\n v54=iconst.i64 0\n v55=iconst.i64 0\n v56=iconst.i64 0\n v57=iconst.i64 0\n v58=iconst.i64 0\n v59=iconst.i64 0\n v60=iconst.i64 0\n v61=iconst.i64 0\n v62=iconst.i64 0\n v63=iconst.i64 0\n v64=iconst.i64 0\n v65=iconst.i64 0\n v66=iconst.i64 0\n v67=iconst.i64 0\n v68=iconst.i64 0\n v69=iconst.i64 0"),
        format!("file f0=\"t.c\"\n{}",src.replace("v9=xor v8,v4","v9=xor v8,v4 !f0:1:1")),
    ];
    for case in cases {
        assert_eq!(
            optimize(&case).to_string(),
            keel_ir::parse(&case).unwrap().to_string(),
            "{case}"
        );
    }
    let escaped = source(0, 8, Type::I64, true).replace("return v13", "return v9");
    assert_eq!(
        optimize(&escaped).to_string(),
        keel_ir::parse(&escaped).unwrap().to_string()
    );
    let load = source(0, 8, Type::I64, true)
        .replace("func @f(i64,i64)", "func @f(ptr,i64)")
        .replace("v0:i64", "v0:ptr")
        .replace("jump block2(v2,v0)", "jump block2(v2,v1)")
        .replace("v8=imul v5,v7", "v8=load.i64 volatile v0");
    assert_eq!(
        optimize(&load).to_string(),
        keel_ir::parse(&load).unwrap().to_string()
    );
}

#[test]
fn header_liveouts_parallel_recurrences_and_aliases() {
    for rotated in [false, true] {
        let src = source(0, 8, Type::I64, rotated).replace("return v13", "return v4");
        let before = keel_ir::parse(&src).unwrap();
        let after = optimize(&src);
        let want = if rotated { 7 } else { 8 };
        assert_eq!(super::support::execute(&before.functions[0], &[5, 0]), want);
        assert_eq!(super::support::execute(&after.functions[0], &[5, 0]), want);
    }
    let src = source(0, 8, Type::I64, true);
    let mut m = keel_ir::parse(&src).unwrap();
    let f = &mut m.functions[0];
    let param = f.block_params(f.blocks().nth(1).unwrap())[1];
    let literal = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), D::Iconst { imm: 3, .. }))
        .unwrap()
        .1;
    let old = f.first_result(literal);
    f.replace_all_uses(old, param);
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["loop-unroll-exact"]).unwrap();
    assert_eq!(m.to_string(), before);
    let mut debug = keel_ir::parse(&src).unwrap();
    debug.functions[0].debug_scopes.push(None);
    keel_opt::run_passes(&mut debug, &["loop-unroll-exact"]).unwrap();
    assert!(!debug.to_string().contains("brif"));
}

#[test]
fn rotation_cleanup_exposes_exact_loop_without_cloning_dead_aliases() {
    let src = source(0, 8, Type::I64, false);
    let mut m = keel_ir::parse(&src).unwrap();
    keel_opt::run_passes(
        &mut m,
        &[
            "loop-rotate",
            "phi-simplify",
            "const-fold",
            "simplify-cfg",
            "dce",
            "loop-unroll-exact",
            "dce",
        ],
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(!m.to_string().contains("brif"), "{m}");
    for seed in [0, 1, u64::MAX] {
        assert_eq!(
            super::support::execute(&m.functions[0], &[seed, 0]),
            super::support::execute(&keel_ir::parse(&src).unwrap().functions[0], &[seed, 0])
        );
    }
}

#[test]
fn literal_and_symbol_templates_are_shared_across_iterations() {
    let src = format!(
        "decl @table()\n{}",
        source(0, 8, Type::I64, true).replace(" v8=", " v50=symbol_addr @table\n v8=")
    );
    let after = optimize(&src);
    let f = &after.functions[0];
    let constants = |imm| {
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i),D::Iconst {imm:n,..} if *n==imm))
            .count()
    };
    assert_eq!(constants(3), 1, "One shared multiplier literal");
    assert_eq!(constants(1), 1, "One shared induction-step literal");
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), D::SymbolAddr { .. }))
            .count(),
        1
    );
    // Iteration-dependent arithmetic is still cloned in its original order.
    assert_eq!(
        f.layout_insts()
            .filter(|(_, i)| matches!(
                f.inst_data(*i),
                D::Binary {
                    op: keel_ir::BinaryOp::Imul,
                    ..
                }
            ))
            .count(),
        8
    );
}

#[test]
fn scalar_stores_keep_aliasing_input_observations_and_zero_trips() {
    use keel_ir_interp::{Outcome, Val};
    for trips in [0, 1, 8] {
        for input in [false, true] {
            let read = if input {
                "v20=ptradd v1,v4\n v21=load.i8 v20\n v22=uext.i64 v21"
            } else {
                "v22=iconst.i64 17"
            };
            let src = format!(
                "func @f(ptr,ptr)->i64 {{
block0(v0:ptr,v1:ptr):
 v2=iconst.i64 0
 v3=iconst.i64 {trips}
 jump block1(v2)
block1(v4:i64):
 v5=icmp.ult v4,v3
 brif v5,block2,block3
block2:
 {read}
 v6=load.i64 v0
 v7=xor v6,v22
 v8=iconst.i64 1099511628211
 v9=imul v7,v8
 store v9,v0
 v10=iconst.i64 1
 v11=iadd v4,v10
 jump block1(v11)
block3:
 return v2
}}"
            );
            let before = keel_ir::parse(&src).unwrap();
            let after = optimize(&src);
            assert!(!after.to_string().contains("brif"));
            let stores = after.functions[0]
                .layout_insts()
                .filter(|(_, i)| matches!(after.functions[0].inst_data(*i), D::Store { .. }))
                .count();
            assert_eq!(
                stores,
                if input {
                    trips
                } else {
                    usize::from(trips != 0)
                }
            );
            for offset in [0, 1, 7, 8, 16] {
                let mut outputs = Vec::new();
                for m in [&before, &after] {
                    let f = &m.functions[0];
                    let mut i = Interpreter::for_function(f).with_load_trace();
                    let bytes: Vec<_> = (0..32).map(|v| (v * 137 + 17) as u8).collect();
                    let p = i.memory_mut().alloc_bytes(&bytes, 8);
                    let out = i.call("f", &[Val::Ptr(p), Val::Ptr(p + offset)]);
                    assert!(matches!(out, Outcome::Return(_)), "{out:?}");
                    outputs.push((out, i.memory().read(p, 32).unwrap().to_vec()));
                }
                assert_eq!(
                    outputs[0], outputs[1],
                    "trips={trips}, input={input}, offset={offset}"
                );
            }
            if trips == 0 {
                let mut i = Interpreter::for_function(&after.functions[0]);
                assert!(matches!(
                    i.call("f", &[Val::Ptr(0), Val::Ptr(0)]),
                    Outcome::Return(_)
                ));
            }
            let volatile = src.replace("store v9,v0", "store volatile v9,v0");
            assert_eq!(
                optimize(&volatile).to_string(),
                keel_ir::parse(&volatile).unwrap().to_string()
            );
        }
    }
}

#[test]
fn store_cleanup_keeps_a_write_before_a_trapping_operation() {
    use keel_ir_interp::{Outcome, Val};
    let src = "func @f(ptr,i64)->i64 {
block0(v0:ptr,v1:i64):
 v2=iconst.i64 17
 v3=iconst.i64 29
 store v2,v0
 v4=udiv v3,v1
 store v3,v0
 v5=iconst.i64 0
 v6=iconst.i64 2
 jump block1(v5)
block1(v7:i64):
 v8=icmp.ult v7,v6
 brif v8,block2,block3
block2:
 v9=iconst.i64 1
 v10=iadd v7,v9
 jump block1(v10)
block3:
 return v4
}";
    let after = optimize(src);
    assert!(!after.to_string().contains("brif"));
    for m in [keel_ir::parse(src).unwrap(), after] {
        let mut i = Interpreter::for_function(&m.functions[0]);
        let p = i.memory_mut().alloc_bytes(&0u64.to_le_bytes(), 8);
        assert!(matches!(
            i.call("f", &[Val::Ptr(p), Val::I64(0)]),
            Outcome::Trap(_)
        ));
        assert_eq!(i.memory().read(p, 8).unwrap(), 17u64.to_le_bytes());
    }
}

#[test]
fn ordinary_read_after_a_store_requires_its_own_readability_proof() {
    for (flags, expected_loads) in [("", 1), ("notrap ", 0)] {
        let src = format!(
            "func @f(ptr)->i64 {{
block0(v0:ptr):
 v1=iconst.i64 0
 v2=iconst.i64 1
 v3=iconst.i64 17
 jump block1(v1,v1)
block1(v4:i64,v5:i64):
 v6=icmp.ult v4,v2
 brif v6,block2,block3
block2:
 store v3,v0
 v7=load.i64 {flags}v0
 v8=iadd v5,v7
 v9=iadd v4,v2
 jump block1(v9,v8)
block3:
 return v5
}}"
        );
        let after = optimize(&src);
        assert_eq!(
            after.functions[0]
                .layout_insts()
                .filter(|(_, i)| matches!(after.functions[0].inst_data(*i), D::Load { .. }))
                .count(),
            expected_loads,
            "{after}"
        );
    }
}
