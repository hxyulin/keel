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
    for shape in VectorType::ALL {
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
