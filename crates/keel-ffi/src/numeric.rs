//! Numeric instructions, vector shapes and checked arithmetic.
use super::{KeelBlock, KeelModule, KeelStatus, KeelValue, input, invalid, output, run, ty};
use keel_ir::{FloatCC, InsertPoint, OverflowOp, Type, UnaryOp, VectorType};

pub(super) fn type_id(kind: Type) -> u32 {
    match kind {
        Type::I8 => 1,
        Type::I16 => 2,
        Type::I32 => 3,
        Type::I64 => 4,
        Type::Ptr => 5,
        Type::I128 => 6,
        Type::F32 => 7,
        Type::F64 => 8,
        Type::F80 => 9,
        Type::Vector(v) => 0x1_0000 | (u32::from(v.lanes()) << 8) | type_id(v.lane_type()),
    }
}

api! {
    /// Returns a validated vector type tag for a lane type and lane count.
    fn keel_type_vector(module: *mut KeelModule, lane: u32, lanes: u32, out: *mut u32) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |_| {
            let lane = ty(lane)?;
            let shape = VectorType::ALL.iter().find(|v| v.lane_type() == lane && u32::from(v.lanes()) == lanes)
                .ok_or_else(|| invalid("unsupported vector shape"))?;
            Ok(type_id(Type::Vector(*shape)))
        }) }
    }
}

api! {
    /// Queries a value's type without inspecting Rust storage.
    fn keel_value_type(module: *mut KeelModule, value: KeelValue, out: *mut u32) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            m.function_index(super::KeelFunction { owner: value.owner, index: value.function })?;
            let v = m.value(value.function, value)?;
            Ok(type_id(m.ir.functions[value.function as usize].value_type(v)))
        }) }
    }
}

api! {
    /// Appends an IEEE f32/f64 constant from exact bits; f32 uses the low 32 bits.
    fn keel_build_fconst(module: *mut KeelModule, block: KeelBlock, kind: u32, bits: u64, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let kind = ty(kind)?;
            if !matches!(kind, Type::F32 | Type::F64) { return Err(invalid("fconst requires f32 or f64")); }
            let builder = m.ir.functions[block.function as usize].ins(InsertPoint::End(b));
            let v = if kind == Type::F32 { builder.f32const(f32::from_bits(bits as u32)) } else { builder.f64const(f64::from_bits(bits)) };
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends an x87 f80 constant from its explicit significand and sign/exponent.
    fn keel_build_f80const(module: *mut KeelModule, block: KeelBlock, significand: u64, sign_exponent: u16, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).f80const(significand, sign_exponent);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends scalar integer or floating unary arithmetic.
    fn keel_build_unary(module: *mut KeelModule, block: KeelBlock, operation: u32, value: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let value = m.value(block.function, value)?;
            let op = match operation {
                1 => UnaryOp::Ineg, 2 => UnaryOp::Bnot, 3 => UnaryOp::Clz, 4 => UnaryOp::Ctz,
                5 => UnaryOp::Popcnt, 6 => UnaryOp::Bswap,
                7 => UnaryOp::Fneg, 8 => UnaryOp::Fabs, 9 => UnaryOp::Fsqrt,
                10 => UnaryOp::Fceil, 11 => UnaryOp::Ffloor, 12 => UnaryOp::Ftrunc, 13 => UnaryOp::Fnearest,
                _ => return Err(invalid("unknown unary operation")),
            };
            let f = &mut m.ir.functions[block.function as usize];
            let kind = f.value_type(value);
            if (if op.is_float() { !kind.is_float() } else { !kind.is_int() }) || (op == UnaryOp::Bswap && kind == Type::I8) {
                return Err(invalid("invalid unary operand type"));
            }
            let v = f.ins(InsertPoint::End(b)).unary(op, value);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends arithmetic producing a wrapped result and an i8 overflow flag.
    fn keel_build_overflow(module: *mut KeelModule, block: KeelBlock, operation: u32,
        lhs: KeelValue, rhs: KeelValue, results: *mut KeelValue, result_count: usize) {
        // SAFETY: Public pointer contract covers module and a disjoint two-value output.
        unsafe { run(module, |m| {
            let b = m.insertion(block)?;
            if result_count != 2 || results.is_null() { return Err(invalid("overflow needs two result slots")); }
            let a = m.value(block.function, lhs)?;
            let c = m.value(block.function, rhs)?;
            let op = match operation { 1 => OverflowOp::Uadd, 2 => OverflowOp::Sadd, 3 => OverflowOp::Usub,
                4 => OverflowOp::Ssub, 5 => OverflowOp::Umul, 6 => OverflowOp::Smul, _ => return Err(invalid("unknown overflow operation")) };
            let f = &mut m.ir.functions[block.function as usize];
            if !f.value_type(a).is_int() || f.value_type(a) != f.value_type(c) { return Err(invalid("overflow requires equal integer types")); }
            let (v, flag) = f.ins(InsertPoint::End(b)).overflow(op, a, c);
            results.write(m.value_id(block.function, v));
            results.add(1).write(m.value_id(block.function, flag));
            Ok(())
        }) }
    }
}

api! {
    /// Appends a floating comparison producing i8, including ordered/unordered tests.
    fn keel_build_fcmp(module: *mut KeelModule, block: KeelBlock, condition: u32, lhs: KeelValue, rhs: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let a = m.value(block.function, lhs)?;
            let c = m.value(block.function, rhs)?;
            let cond = match condition { 1 => FloatCC::Eq, 2 => FloatCC::Ne, 3 => FloatCC::Lt, 4 => FloatCC::Le,
                5 => FloatCC::Gt, 6 => FloatCC::Ge, 7 => FloatCC::Ord, 8 => FloatCC::Uno, _ => return Err(invalid("unknown float condition")) };
            let f = &mut m.ir.functions[block.function as usize];
            if !f.value_type(a).is_float() || f.value_type(a) != f.value_type(c) { return Err(invalid("float comparison requires equal float types")); }
            let v = f.ins(InsertPoint::End(b)).fcmp(cond, a, c);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a select with an integer condition and equal-typed scalar arms.
    fn keel_build_select(module: *mut KeelModule, block: KeelBlock, condition: KeelValue, yes: KeelValue, no: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?;
            let condition = m.value(block.function, condition)?;
            let yes = m.value(block.function, yes)?;
            let no = m.value(block.function, no)?;
            let f = &mut m.ir.functions[block.function as usize];
            if !f.value_type(condition).is_int() || f.value_type(yes) != f.value_type(no) { return Err(invalid("select requires integer condition and equal arm types")); }
            let v = f.ins(InsertPoint::End(b)).select(condition, yes, no);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends fused floating multiply-add on three same-type values.
    fn keel_build_fma(module: *mut KeelModule, block: KeelBlock, a: KeelValue, b: KeelValue, c: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let at = m.insertion(block)?;
            let a = m.value(block.function, a)?; let b = m.value(block.function, b)?; let c = m.value(block.function, c)?;
            let f = &mut m.ir.functions[block.function as usize];
            let kind = f.value_type(a);
            if !kind.is_float() || kind != f.value_type(b) || kind != f.value_type(c) { return Err(invalid("fma requires equal float types")); }
            let v = f.ins(InsertPoint::End(at)).fma(a, b, c);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a vector splat from a matching scalar lane value.
    fn keel_build_vsplat(module: *mut KeelModule, block: KeelBlock, kind: u32, value: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let value = m.value(block.function, value)?;
            let kind = ty(kind)?;
            if kind.vector().is_none_or(|v| v.lane_type() != m.ir.functions[block.function as usize].value_type(value)) { return Err(invalid("splat lane type does not match vector")); }
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).vsplat(kind, value);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Appends a vector lane extraction with a checked lane index.
    fn keel_build_vextract(module: *mut KeelModule, block: KeelBlock, vector: KeelValue, lane: u32, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let vector = m.value(block.function, vector)?;
            let shape = m.ir.functions[block.function as usize].value_type(vector).vector().ok_or_else(|| invalid("extract requires vector"))?;
            if lane >= u32::from(shape.lanes()) { return Err(invalid("lane out of range")); }
            let v = m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).vextract(vector, lane as u8);
            Ok(m.value_id(block.function, v))
        }) }
    }
}

api! {
    /// Terminates a block with a target trap. Runtime panic reporting is frontend-owned.
    fn keel_build_trap(module: *mut KeelModule, block: KeelBlock) {
        // SAFETY: Public pointer contract covers module.
        unsafe { run(module, |m| { let b = m.insertion(block)?; m.ir.functions[block.function as usize].ins(InsertPoint::End(b)).trap(); Ok(()) }) }
    }
}

api! {
    /// Appends a named portable vector binary operation from the core registry.
    fn keel_build_vector_binary(module: *mut KeelModule, block: KeelBlock, operation: *const u8, length: usize,
        lhs: KeelValue, rhs: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers operation text and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let lhs = m.value(block.function, lhs)?; let rhs = m.value(block.function, rhs)?;
            let operation = std::str::from_utf8(input(operation, length)?).map_err(|_| invalid("operation must be UTF-8"))?;
            let op = operation.parse::<keel_ir::VectorBinaryOp>().map_err(|_| invalid("unknown vector binary operation"))?;
            let f = &mut m.ir.functions[block.function as usize];
            let kind = f.value_type(lhs);
            if kind != f.value_type(rhs) || kind.vector().is_none_or(|v| v.lane_type().is_float() != op.is_float()) { return Err(invalid("vector operands do not match operation")); }
            let v = f.ins(InsertPoint::End(b)).vector_binary(op, lhs, rhs);
            Ok(m.value_id(block.function, v))
        }) }
    }
}
api! {
    /// Appends a horizontal integer reduction, preserving the lane's integer width.
    fn keel_build_vector_reduce(module: *mut KeelModule, block: KeelBlock, operation: u32, value: KeelValue, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let value = m.value(block.function, value)?;
            let op = *keel_ir::VectorReduceOp::ALL.get(operation as usize).ok_or_else(|| invalid("unknown vector reduction"))?;
            let f = &mut m.ir.functions[block.function as usize];
            if f.value_type(value).vector().is_none_or(|v| !v.lane_type().is_int()) { return Err(invalid("reduction requires integer vector")); }
            let v = f.ins(InsertPoint::End(b)).vector_reduce(op, value);
            Ok(m.value_id(block.function, v))
        }) }
    }
}
api! {
    /// Appends a vector lane insertion with checked scalar type and lane index.
    fn keel_build_vinsert(module: *mut KeelModule, block: KeelBlock, vector: KeelValue, value: KeelValue, lane: u32, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers module and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let vector = m.value(block.function, vector)?; let value = m.value(block.function, value)?;
            let f = &mut m.ir.functions[block.function as usize];
            if f.value_type(vector).vector().is_none_or(|v| lane >= u32::from(v.lanes()) || v.lane_type() != f.value_type(value)) { return Err(invalid("invalid vector insertion")); }
            let v = f.ins(InsertPoint::End(b)).vinsert(vector, value, lane as u8);
            Ok(m.value_id(block.function, v))
        }) }
    }
}
api! {
    /// Appends a two-vector lane shuffle with a checked lane map.
    fn keel_build_vshuffle(module: *mut KeelModule, block: KeelBlock, lhs: KeelValue, rhs: KeelValue, lanes: *const u8, count: usize, out: *mut KeelValue) {
        // SAFETY: Public pointer contract covers lane map and output.
        unsafe { output(module, out, |m| {
            let b = m.insertion(block)?; let lhs = m.value(block.function, lhs)?; let rhs = m.value(block.function, rhs)?;
            let lanes = input(lanes, count)?;
            let f = &mut m.ir.functions[block.function as usize];
            let kind = f.value_type(lhs);
            if kind != f.value_type(rhs) || kind.vector().is_none_or(|v| count != usize::from(v.lanes()) || lanes.iter().any(|i| *i >= 2 * v.lanes())) { return Err(invalid("invalid shuffle shapes or lane map")); }
            let v = f.ins(InsertPoint::End(b)).vshuffle(lhs, rhs, lanes.to_vec());
            Ok(m.value_id(block.function, v))
        }) }
    }
}
