//! Specialize the narrow counts of dynamic record copies in large functions.
use crate::ir_match::{def, int_const};
use crate::{OptimizationContext, VectorTarget};
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, Intrinsic, MemoryFlags,
    Type, Value, VectorType,
};

const MAX_COPIES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyPath {
    Empty,
    Record,
    ShortBuffer,
}

struct CopySite {
    inst: Inst,
    args: [Value; 3],
    path: CopyPath,
}

pub(crate) fn run(f: &mut Function, context: &OptimizationContext) -> bool {
    if !matches!(
        context.target,
        VectorTarget::X86Sse2 | VectorTarget::Aarch64Neon
    ) || f.num_blocks() < 128
    {
        return false;
    }
    let calls = || {
        f.layout_insts().filter_map(|(block, inst)| {
            let args = copy_args(f, inst)?;
            // Skip the isolated slow copy emitted by a previous invocation.
            (f.block_insts(block).first() != Some(&inst)).then_some((inst, args))
        })
    };
    let mut copies: Vec<_> = calls()
        .filter_map(|(inst, args)| {
            let path = record_path(f, args[2])?;
            Some(CopySite { inst, args, path })
        })
        .take(MAX_COPIES + 1)
        .collect();
    if copies.len() > MAX_COPIES
        || !copies.iter().any(|site| site.path == CopyPath::Empty)
        || !copies.iter().any(|site| site.path == CopyPath::Record)
    {
        copies.clear();
    }
    if context.target == VectorTarget::Aarch64Neon && f.local {
        let mut buffers = calls().filter_map(|(inst, args)| {
            let [dst, src, bytes] = args;
            let path = short_buffer(f, dst, src, bytes)?;
            Some(CopySite { inst, args, path })
        });
        if let Some(site) = buffers.next()
            && buffers.next().is_none()
        {
            copies.push(site);
        }
    }
    if copies.is_empty()
        || f.layout_insts().any(|(_, inst)| {
            matches!(
                f.inst_data(inst),
                InstData::InlineAsm { .. }
                    | InstData::X86RflagsRegion { .. }
                    | InstData::Intrinsic {
                        id: Intrinsic::X86RflagsRead | Intrinsic::X86RflagsSwap,
                        ..
                    }
            )
        })
    {
        return false;
    }
    for site in copies {
        specialize(f, site);
    }
    true
}

fn copy_args(f: &Function, inst: Inst) -> Option<[Value; 3]> {
    let InstData::Intrinsic {
        id: Intrinsic::Memcpy,
        args,
    } = f.inst_data(inst)
    else {
        return None;
    };
    if f.inst_loc(inst).is_some() || !f.inst_results(inst).is_empty() {
        return None;
    }
    args.as_slice().try_into().ok()
}

fn record_path(f: &Function, bytes: Value) -> Option<CopyPath> {
    let InstData::Binary {
        op: BinaryOp::Shl,
        args: [count, shift],
    } = *f.inst_data(def(f, bytes)?)
    else {
        return None;
    };
    let InstData::Convert {
        op,
        ty: Type::I64,
        arg,
    } = *f.inst_data(def(f, count)?)
    else {
        return None;
    };
    match (op, f.value_type(arg), int_const(f, shift)) {
        (ConvertOp::Uext, Type::I8, Some(3)) => Some(CopyPath::Empty),
        (ConvertOp::Sext, Type::I32, Some(4)) => Some(CopyPath::Record),
        _ => None,
    }
}

fn loaded_field(f: &Function, value: Value, ty: Type) -> Option<(Value, i64)> {
    let InstData::Load {
        ty: loaded,
        addr,
        flags,
    } = *f.inst_data(def(f, value)?)
    else {
        return None;
    };
    if loaded != ty || flags.volatile {
        return None;
    }
    let InstData::Binary {
        op: BinaryOp::Ptradd,
        args: [base, offset],
    } = *f.inst_data(def(f, addr)?)
    else {
        return None;
    };
    Some((f.resolve(base), int_const(f, offset)?))
}

// Match a copy between buffer fields of two records, with a signed length
// field immediately after the source buffer pointer.
fn short_buffer(f: &Function, dst: Value, src: Value, bytes: Value) -> Option<CopyPath> {
    let InstData::Convert {
        op: ConvertOp::Sext,
        ty: Type::I64,
        arg,
    } = *f.inst_data(def(f, bytes)?)
    else {
        return None;
    };
    let (src_base, src_offset) = loaded_field(f, src, Type::Ptr)?;
    let (dst_base, dst_offset) = loaded_field(f, dst, Type::Ptr)?;
    let (len_base, len_offset) = loaded_field(f, arg, Type::I32)?;
    (src_base == len_base
        && src_base != dst_base
        && src_offset == dst_offset
        && src_offset.checked_add(8) == Some(len_offset))
    .then_some(CopyPath::ShortBuffer)
}

fn specialize(f: &mut Function, site: CopySite) {
    let CopySite {
        inst,
        args: [dst, src, bytes],
        path,
    } = site;
    let block = f.inst_block(inst);
    let position = f
        .block_insts(block)
        .iter()
        .position(|&i| i == inst)
        .unwrap();
    let slow = f.create_block();
    let tail = f.create_block();
    f.move_insts_to_end(block, position, tail);
    f.move_inst(inst, InsertPoint::End(slow));
    f.ins(InsertPoint::End(slow)).jump(tail, &[]);

    let gate = InsertPoint::End(block);
    match path {
        CopyPath::Empty => {
            f.ins(gate).brif(bytes, slow, &[], tail, &[]);
        }
        CopyPath::Record => {
            let fast = f.create_block();
            let sixteen = f.ins(gate).iconst(Type::I64, 16);
            let one = f.ins(gate).icmp(IntCC::Eq, bytes, sixteen);
            f.ins(gate).brif(one, fast, &[], slow, &[]);
            let flags = MemoryFlags {
                trap_ub: true,
                ..MemoryFlags::default()
            };
            let emit = InsertPoint::End(fast);
            let value = f
                .ins(emit)
                .load_with_flags(Type::Vector(VectorType::I8x16), src, flags);
            f.ins(emit).store_with_flags(value, dst, flags);
            f.ins(emit).jump(tail, &[]);
        }
        CopyPath::ShortBuffer => {
            let fast = f.create_block();
            let four = f.ins(gate).iconst(Type::I64, 4);
            let offset = f.ins(gate).isub(bytes, four);
            let short = f.ins(gate).icmp(IntCC::Ule, offset, four);
            f.ins(gate).brif(short, fast, &[], slow, &[]);
            let emit = InsertPoint::End(fast);
            let tail_src = f.ins(emit).ptradd(src, offset);
            let tail_dst = f.ins(emit).ptradd(dst, offset);
            let flags = MemoryFlags {
                trap_ub: true,
                ..MemoryFlags::default()
            };
            // Both words are inside the copy. When they overlap, they write
            // the same bytes in the overlapping portion of the destination.
            let head = f.ins(emit).load_with_flags(Type::I32, src, flags);
            let last = f.ins(emit).load_with_flags(Type::I32, tail_src, flags);
            f.ins(emit).store_with_flags(head, dst, flags);
            f.ins(emit).store_with_flags(last, tail_dst, flags);
            f.ins(emit).jump(tail, &[]);
        }
    }
}
