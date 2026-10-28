use keel_ir::{BinaryOp, InstData, MemoryEffects, Type, VectorBinaryOp, VectorType};

fn checked(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = module.to_string();
    let again = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&again).unwrap();
    assert_eq!(text, again.to_string());
    module
}

#[test]
fn all_shapes_build_parse_and_roundtrip() {
    for shape in VectorType::ALL_128 {
        let lane = shape.lane_type();
        let binary = if lane.is_float() { "vfadd" } else { "viadd" };
        let unary = if lane.is_float() { "vfneg" } else { "vbnot" };
        let cmp = if lane.is_float() {
            "vfcmp.lt"
        } else {
            "vicmp.slt"
        };
        let last = shape.lanes() - 1;
        let shuffle = (0..shape.lanes())
            .map(|i| (2 * shape.lanes() - 1 - i).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "func @vector({lane}, ptr) -> {lane} {{\nblock0(v0: {lane}, v1: ptr):\n v2 = vsplat.{shape} v0\n v3 = load.{shape} v1\n v4 = {binary} v2, v3\n v5 = {unary} v4\n v6 = vinsert v5, v0, {last}\n v7 = vshuffle v6, v3, [{shuffle}]\n v8 = {cmp} v7, v3\n v9 = vselect v8, v7, v3\n store v9, v1\n v10 = vextract v9, {last}\n return v10\n}}\n"
        );
        let m = checked(&source);
        let func = &m.functions[0];
        for block in func.blocks() {
            for inst in func.block_insts(block) {
                let data = func.inst_data(*inst);
                if data.is_vector_instruction() {
                    assert!(!data.is_pinned());
                    assert!(!data.may_trap());
                    assert_eq!(data.memory_effects(), MemoryEffects::None);
                }
            }
        }
        assert_eq!(Type::Vector(shape).bytes(), Some(16));
        assert!(!Type::Vector(shape).is_int());
        assert!(!Type::Vector(shape).is_float());
        assert_eq!(shape.mask().lane_type().bytes(), lane.bytes());
    }
}

#[test]
fn every_arithmetic_opcode_and_vector_bitcasts_verify() {
    for name in [
        "viadd",
        "visub",
        "vimul",
        "vpadd",
        "vpsmin",
        "vpsmax",
        "vpumin",
        "vpumax",
        "vfpadd",
        "vfpmin",
        "vfpmax",
        "vsmin",
        "vsmax",
        "vumin",
        "vumax",
        "vsabd",
        "vuabd",
        "vshadd",
        "vuhadd",
        "vsrhadd",
        "vurhadd",
        "vshsub",
        "vuhsub",
        "vsadd_sat",
        "vuadd_sat",
        "vssub_sat",
        "vusub_sat",
        "vand",
        "vor",
        "vxor",
        "vshl",
        "vushr",
        "vsshr",
        "vfadd",
        "vfsub",
        "vfmul",
        "vfdiv",
        "vfmin",
        "vfmax",
        "vfcopysign",
    ] {
        let op: VectorBinaryOp = name.parse().unwrap();
        let ty = if op.is_float() { "f32x4" } else { "i32x4" };
        checked(&format!(
            "func @op({ty}, {ty}) -> {ty} {{\nblock0(v0: {ty}, v1: {ty}):\n v2 = {name} v0, v1\n return v2\n}}"
        ));
    }
    for name in [
        "vineg",
        "vbnot",
        "vclz",
        "vctz",
        "vpopcnt",
        "vbswap",
        "vfneg",
        "vfabs",
        "vfsqrt",
        "vfceil",
        "vffloor",
        "vftrunc",
        "vfnearest",
    ] {
        let ty = if name.starts_with("vf") {
            "f64x2"
        } else {
            "i16x8"
        };
        checked(&format!(
            "func @op({ty}) -> {ty} {{\nblock0(v0: {ty}):\n v1 = {name} v0\n return v1\n}}"
        ));
    }
    checked(
        "func @casts(i128) -> i128 {\nblock0(v0: i128):\n v1 = bitcast.i8x16 v0\n v2 = bitcast.f64x2 v1\n v3 = bitcast.i128 v2\n return v3\n}",
    );
}

#[test]
fn vector_builders_and_operand_rewriting_preserve_types() {
    use keel_ir::{Function, InsertPoint, IntCC, Signature, UnaryOp};
    let t = Type::Vector(VectorType::I32x4);
    let mut f = Function::new("builders", Signature::new(&[t, t, Type::I32], &[Type::I32]));
    let b = f.create_block();
    let a = f.append_block_param(b, t);
    let c = f.append_block_param(b, t);
    let scalar = f.append_block_param(b, Type::I32);
    let s = f.ins(InsertPoint::End(b)).vsplat(t, scalar);
    let x = f
        .ins(InsertPoint::End(b))
        .vector_binary(VectorBinaryOp::Iadd, a, s);
    let x = f.ins(InsertPoint::End(b)).vector_unary(UnaryOp::Ineg, x);
    let x = f.ins(InsertPoint::End(b)).vinsert(x, scalar, 3);
    let x = f.ins(InsertPoint::End(b)).vshuffle(x, c, vec![0, 7, 1, 6]);
    let mask = f.ins(InsertPoint::End(b)).vicmp(IntCC::Eq, x, c);
    let x = f.ins(InsertPoint::End(b)).vselect(mask, x, c);
    let x = f.ins(InsertPoint::End(b)).vextract(x, 2);
    f.append_inst(b, InstData::Return { args: vec![x] });
    keel_ir::verify(&f).unwrap();
    let mut op = InstData::VectorShuffle {
        args: [a, c],
        lanes: vec![0, 1, 2, 3],
    };
    let mut before = Vec::new();
    op.for_each_operand(|v| before.push(v));
    assert_eq!(before, vec![a, c]);
    op.for_each_operand_mut(|v| {
        if *v == a {
            *v = c
        }
    });
    let mut after = Vec::new();
    op.for_each_operand(|v| after.push(v));
    assert_eq!(after, vec![c, c]);
    // Scalar opcodes continue to reject vector operands.
    let mut module = checked(
        "func @bad(i32x4) -> i32x4 {\nblock0(v0: i32x4):\n v1 = viadd v0, v0\n return v1\n}",
    );
    let f = &mut module.functions[0];
    let inst = f.block_insts(f.entry_block().unwrap())[0];
    let InstData::VectorBinary { args, .. } = f.inst_data(inst).clone() else {
        panic!()
    };
    *f.inst_data_mut(inst) = InstData::Binary {
        op: BinaryOp::Iadd,
        args,
    };
    assert!(keel_ir::verify_module(&module).is_err());
}

#[test]
fn invalid_lanes_shapes_masks_and_scalar_vector_mixing_are_rejected() {
    for (params, body) in [
        ("i32", "v1 = vsplat.i32x4 v0\n v2 = vextract v1, 4"),
        ("i32", "v1 = vsplat.i32 v0"),
        ("i64", "v1 = vsplat.i32x4 v0"),
        ("i32x4", "v1 = vextract v0, 255"),
        ("i32x4", "v1 = vinsert v0, v0, 0"),
        ("i32x4", "v1 = vshuffle v0, v0, [0, 1, 2]"),
        ("i32x4", "v1 = vshuffle v0, v0, [0, 1, 2, 8]"),
        ("f32x4", "v1 = viadd v0, v0"),
        ("i32x4", "v1 = vfadd v0, v0"),
        ("f32x4", "v1 = vicmp.eq v0, v0"),
        ("i32x4", "v1 = vfcmp.eq v0, v0"),
        ("f32x4", "v1 = vselect v0, v0, v0"),
        ("i8x16", "v1 = vbswap v0"),
        ("i32x4", "v1 = bitcast.i64 v0"),
        ("i32", "v1 = vextract v0, 0"),
    ] {
        let src = format!("func @bad({params}) {{\nblock0(v0: {params}):\n {body}\n return\n}}");
        let m = keel_ir::parse(&src).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert!(keel_ir::verify_module(&m).is_err(), "accepted {src}");
    }
    for instruction in [
        "vextract v0, -1",
        "vextract v0, 256",
        "viadd.i32x4 v0, v0",
        "vshuffle v0, v0, [0, -1]",
        "vsplat.i32x3 v0",
    ] {
        let src =
            format!("func @bad(i32x4) {{\nblock0(v0: i32x4):\n v1 = {instruction}\n return\n}}");
        assert!(keel_ir::parse(&src).is_err(), "accepted {src}");
    }
}

#[test]
fn forward_references_and_float_comparison_mask_types_are_inferred() {
    use keel_ir::EntityRef;
    // Layout is not dominance order: a vector use is printed before its definition.
    let m = checked(
        "func @forward(ptr) -> i64x2 {
block0(v0: ptr):
 jump block2
block1:
 v1 = vfcmp.lt v2, v2
 return v1
block2:
 v2 = load.f64x2 v0
 jump block1
}",
    );
    let f = &m.functions[0];
    let block = f.blocks().nth(1).unwrap();
    let cmp = f.block_insts(block)[0];
    assert_eq!(
        f.value_type(f.first_result(cmp)),
        Type::Vector(VectorType::I64x2)
    );
    let mut m = checked(
        "func @bad(i32x4) -> i32x4 {\nblock0(v0: i32x4):\n v1 = viadd v0, v0\n return v1\n}",
    );
    let f = &mut m.functions[0];
    let instruction = f.block_insts(f.entry_block().unwrap())[0];
    // A mutation that changes the inferred result type must be diagnosed;
    // visitors must not silently retain the vector result for scalar extraction.
    *f.inst_data_mut(instruction) = InstData::VectorExtract {
        arg: keel_ir::Value::new(0),
        lane: 0,
    };
    assert!(keel_ir::verify_module(&m).is_err());
}

#[test]
fn integer_reductions_roundtrip_and_reject_float_or_scalar_inputs() {
    use keel_ir::VectorReduceOp;
    for op in VectorReduceOp::ALL {
        for shape in VectorType::ALL_128 {
            let source = format!(
                "func @reduce({shape}) -> {} {{\nblock0(v0: {shape}):\n v1 = {op} v0\n return v1\n}}",
                shape.lane_type()
            );
            if shape.lane_type().is_int() {
                checked(&source);
            } else {
                assert!(keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).is_err());
            }
        }
        let source =
            format!("func @bad(i32) -> i8 {{\nblock0(v0: i32):\n v1 = {op} v0\n return v1\n}}");
        assert!(keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).is_err());
    }
}

#[test]
fn width_change_semantics_validate_all_shape_pairs() {
    for name in [
        "swiden_high",
        "uwiden_high",
        "narrow_low",
        "snarrow_sat_low",
        "unarrow_sat_low",
        "sunarrow_sat_low",
        "spairwise_add_long",
        "upairwise_add_long",
    ] {
        let op: keel_ir::ConvertOp = match name {
            "swiden_high" => keel_ir::ConvertOp::SwidenHigh,
            "uwiden_high" => keel_ir::ConvertOp::UwidenHigh,
            "narrow_low" => keel_ir::ConvertOp::NarrowLow,
            "snarrow_sat_low" => keel_ir::ConvertOp::SnarrowSatLow,
            "unarrow_sat_low" => keel_ir::ConvertOp::UnarrowSatLow,
            "sunarrow_sat_low" => keel_ir::ConvertOp::SunarrowSatLow,
            "spairwise_add_long" => keel_ir::ConvertOp::SpairwiseAddLong,
            _ => keel_ir::ConvertOp::UpairwiseAddLong,
        };
        for src in VectorType::ALL_128 {
            for dst in VectorType::ALL_128 {
                let text = format!(
                    "func @change({src}) -> {dst} {{\nblock0(v0: {src}):\n v1 = {name}.{dst} v0\n return v1\n}}"
                );
                let m = keel_ir::parse(&text).unwrap();
                let from = src.lane_type().bytes().unwrap();
                let to = dst.lane_type().bytes().unwrap();
                let valid = src.lane_type().is_int()
                    && dst.lane_type().is_int()
                    && if name.contains("pairwise") {
                        to == from * 2
                    } else if name.contains("widen") {
                        to > from
                    } else {
                        to < from
                    };
                assert_eq!(keel_ir::verify_module(&m).is_ok(), valid, "{text}");
                if valid {
                    checked(&text);
                    let mut f = keel_ir::Function::new(
                        "built",
                        keel_ir::Signature::new(&[Type::Vector(src)], &[Type::Vector(dst)]),
                    );
                    let block = f.create_block();
                    let arg = f.append_block_param(block, Type::Vector(src));
                    let result =
                        f.ins(keel_ir::InsertPoint::End(block))
                            .convert(op, Type::Vector(dst), arg);
                    f.append_inst(block, InstData::Return { args: vec![result] });
                    keel_ir::verify(&f).unwrap();
                }
            }
        }
    }
}

#[test]
fn explicit_neon_intrinsics_validate_roundtrip_and_remain_state_barriers() {
    use keel_ir::NeonIntrinsic;
    for &op in NeonIntrinsic::ALL.iter().filter(|op| op.memory().is_none()) {
        for ty in VectorType::ALL_128.into_iter().map(Type::Vector).chain([
            Type::I64,
            Type::I32,
            Type::I16,
            Type::F32,
        ]) {
            if op.immediate_range(ty).is_some() {
                continue;
            }
            let inputs = op.operand_types(ty).unwrap_or(vec![ty; op.arity()]);
            let params = inputs
                .iter()
                .map(Type::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let bindings = inputs
                .iter()
                .enumerate()
                .map(|(i, t)| format!("v{i}: {t}"))
                .collect::<Vec<_>>()
                .join(",");
            let args = (0..op.arity_for(ty))
                .map(|i| format!("v{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let block = if bindings.is_empty() {
                "block0".to_string()
            } else {
                format!("block0({bindings})")
            };
            let source = format!(
                "func @arch({params}) -> {ty} {{\n{block}:\n v9 = {op}.{ty}({args})\n return v9\n}}"
            );
            let m = keel_ir::parse(&source).unwrap();
            assert_eq!(
                keel_ir::verify_module(&m).is_ok(),
                op.accepts(ty),
                "{source}"
            );
            if op.accepts(ty) {
                let m = checked(&source);
                let f = &m.functions[0];
                let i = f.block_insts(f.entry_block().unwrap())[0];
                let d = f.inst_data(i);
                assert!(d.is_pinned());
                assert_eq!(d.memory_effects(), MemoryEffects::Barrier);
                let mut operands = Vec::new();
                d.for_each_operand(|v| operands.push(v));
                assert_eq!(operands.len(), op.arity_for(ty));
                let mut wrong = d.clone();
                if let InstData::Neon { args, .. } = &mut wrong {
                    args.push(f.first_result(i));
                }
                let mut f = f.clone();
                *f.inst_data_mut(i) = wrong;
                assert!(keel_ir::verify(&f).is_err());
            }
        }
    }
}

#[test]
fn architectural_immediates_require_in_range_literal_i32_counts() {
    use keel_ir::NeonIntrinsic;
    for &op in NeonIntrinsic::ALL.iter().filter(|op| op.memory().is_none()) {
        for shape in VectorType::ALL_128 {
            let result = Type::Vector(shape);
            let Some(range) = op.immediate_range(result) else {
                continue;
            };
            let inputs = op.operand_types(result).unwrap();
            let vectors = &inputs[..inputs.len() - 1];
            let params = vectors
                .iter()
                .map(Type::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let bindings = vectors
                .iter()
                .enumerate()
                .map(|(i, t)| format!("v{i}: {t}"))
                .collect::<Vec<_>>()
                .join(",");
            let args = vectors
                .iter()
                .enumerate()
                .map(|(i, _)| format!("v{i},"))
                .collect::<String>();
            for n in [
                *range.start() - 1,
                *range.start(),
                *range.end(),
                *range.end() + 1,
            ] {
                let text = format!(
                    "func @shift({params}) -> {result} {{\nblock0({bindings}):\n v5 = iconst.i32 {n}\n v6 = {op}.{result}({args}v5)\n return v6\n}}"
                );
                let m = keel_ir::parse(&text).unwrap();
                assert_eq!(
                    keel_ir::verify_module(&m).is_ok(),
                    range.contains(&n),
                    "{text}"
                );
                if range.contains(&n) {
                    checked(&text);
                }
            }
            let text = format!(
                "func @dynamic({params},i32) -> {result} {{\nblock0({bindings},v5: i32):\n v6 = {op}.{result}({args}v5)\n return v6\n}}"
            );
            assert!(keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).is_err());
        }
    }
}

#[test]
fn structured_neon_rejects_wrong_counts_shapes_and_lanes() {
    for shape in VectorType::ALL_64.into_iter().chain(VectorType::ALL_128) {
        for &op in keel_ir::NeonIntrinsic::ALL {
            let Some(m) = op.memory() else {
                continue;
            };
            let ty = Type::Vector(shape);
            let inputs = op.operand_types(ty).unwrap();
            let params = inputs
                .iter()
                .take(inputs.len() - usize::from(m.lane))
                .enumerate()
                .map(|(i, t)| format!("v{i}: {t}"))
                .collect::<Vec<_>>()
                .join(",");
            let sig = inputs
                .iter()
                .take(inputs.len() - usize::from(m.lane))
                .map(Type::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let args = (0..inputs.len() - usize::from(m.lane))
                .map(|i| format!("v{i}"))
                .chain(m.lane.then_some("v20".into()))
                .collect::<Vec<_>>()
                .join(",");
            let results = (0..op.result_count())
                .map(|i| format!("v{}", 30 + i))
                .collect::<Vec<_>>()
                .join(",");
            for index in [
                -1,
                0,
                i64::from(shape.lanes()) - 1,
                i64::from(shape.lanes()),
            ] {
                let source = format!(
                    "func @f({sig}) {{\nblock0({params}):\n v20 = iconst.i32 {index}\n {}{op}.{shape}({args})\n return\n}}",
                    if m.store {
                        String::new()
                    } else {
                        format!("{results} = ")
                    }
                );
                let module = keel_ir::parse(&source).unwrap();
                assert_eq!(
                    keel_ir::verify_module(&module).is_ok(),
                    !m.lane || (0..i64::from(shape.lanes())).contains(&index),
                    "{source}"
                );
            }
        }
    }
    assert!(
        keel_ir::parse("func @f(ptr) {\nblock0(v0: ptr):\n v1,v2 = neon_ld3.i8x8(v0)\n return\n}")
            .is_err()
    );
}
