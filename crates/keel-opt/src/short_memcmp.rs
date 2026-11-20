//! Compare short, dynamic byte ranges without calling the runtime.
use crate::ir_match::{def, int_const};
use crate::{OptimizationContext, VectorTarget};
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, Intrinsic, Type, Value,
};

pub(crate) fn run(f: &mut Function, context: &OptimizationContext) -> bool {
    if !matches!(
        context.target,
        VectorTarget::Aarch64Neon | VectorTarget::X86Sse2
    ) {
        return false;
    }
    let candidate = f.layout_insts().find_map(|(block, inst)| {
        let InstData::Intrinsic {
            id: Intrinsic::Memcmp,
            args,
        } = f.inst_data(inst)
        else {
            return None;
        };
        let &[left, right, size] = args.as_slice() else {
            return None;
        };
        (f.inst_loc(inst).is_none()
            && f.block_insts(block).first() != Some(&inst)
            && decoded_length(f, size))
        .then_some((inst, [left, right, size]))
    });
    if let Some((inst, args)) = candidate {
        specialize(f, inst, args, context.target);
        return true;
    }
    false
}

// Decoded element counts can be small even when their integer type is wide.
// Limit code growth to the case where the input encodes length and a tag.
fn decoded_length(f: &Function, size: Value) -> bool {
    let Some(extension) = def(f, size) else {
        return false;
    };
    let InstData::Convert {
        op: ConvertOp::Sext,
        ty: Type::I64,
        arg,
    } = *f.inst_data(extension)
    else {
        return false;
    };
    let Some(divide) = def(f, arg) else {
        return false;
    };
    let InstData::Binary {
        op: BinaryOp::Sdiv,
        args: [bits, divisor],
    } = *f.inst_data(divide)
    else {
        return false;
    };
    f.value_type(arg) == Type::I32
        && int_const(f, divisor) == Some(2)
        && def(f, bits).is_some_and(|subtract| {
            matches!(f.inst_data(subtract), InstData::Binary { op: BinaryOp::Isub, args }
                if matches!(int_const(f, args[1]), Some(1..=32)))
        })
}

fn specialize(f: &mut Function, inst: Inst, [left, right, size]: [Value; 3], target: VectorTarget) {
    let block = f.inst_block(inst);
    let next = f
        .block_insts(block)
        .iter()
        .position(|&i| i == inst)
        .unwrap()
        + 1;
    let original = f.first_result(inst);
    let fast = f.create_block();
    let slow = f.create_block();
    let tail = f.create_block();
    let result = f.append_block_param(tail, Type::I32);
    f.move_insts_to_end(block, next, tail);
    let fallback = f.append_inst(slow, f.inst_data(inst).clone());
    let fallback = f.first_result(fallback);
    f.replace_all_uses(original, result);
    f.remove_inst(inst);
    f.ins(InsertPoint::End(slow)).jump(tail, &[fallback]);

    let gate = InsertPoint::End(block);
    let four = f.ins(gate).iconst(Type::I64, 4);
    let offset = f.ins(gate).isub(size, four);
    let fits = f.ins(gate).icmp(IntCC::Ule, offset, four);
    f.ins(gate).brif(fits, fast, &[], slow, &[]);

    let at = InsertPoint::End(fast);
    let left_end = f.ins(at).ptradd(left, offset);
    let right_end = f.ins(at).ptradd(right, offset);
    // The two overlapping words cover every byte of each input and stay in bounds.
    let left_head = f.ins(at).load(Type::I32, left);
    let left_tail = f.ins(at).load(Type::I32, left_end);
    let right_head = f.ins(at).load(Type::I32, right);
    let right_tail = f.ins(at).load(Type::I32, right_end);
    let head_differs = f.ins(at).icmp(IntCC::Ne, left_head, right_head);
    let (at, a, b) = if target == VectorTarget::X86Sse2 {
        let bytes = f.create_block();
        let a = f.append_block_param(bytes, Type::I32);
        let b = f.append_block_param(bytes, Type::I32);
        f.ins(at).brif(
            head_differs,
            bytes,
            &[left_head, right_head],
            bytes,
            &[left_tail, right_tail],
        );
        (InsertPoint::End(bytes), a, b)
    } else {
        let a = f.ins(at).select(head_differs, left_head, left_tail);
        let b = f.ins(at).select(head_differs, right_head, right_tail);
        (at, a, b)
    };
    let changed = f.ins(at).xor(a, b);
    let bit = f.ins(at).ctz(changed);
    let whole_byte = f.ins(at).iconst(Type::I32, 24);
    let shift = f.ins(at).and(bit, whole_byte);
    let a = f.ins(at).ushr(a, shift);
    let b = f.ins(at).ushr(b, shift);
    let byte = f.ins(at).iconst(Type::I32, 255);
    let a = f.ins(at).and(a, byte);
    let b = f.ins(at).and(b, byte);
    let difference = f.ins(at).isub(a, b);
    f.ins(at).jump(tail, &[difference]);
}
