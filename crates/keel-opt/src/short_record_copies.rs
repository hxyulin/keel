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
    let copies: Vec<_> = f
        .layout_insts()
        .filter_map(|(block, inst)| {
            // Skip the isolated slow copy emitted by a previous invocation.
            let site = copy_site(f, inst)?;
            (f.block_insts(block).first() != Some(&inst)).then_some(site)
        })
        .take(MAX_COPIES + 1)
        .collect();
    if copies.len() > MAX_COPIES
        || !copies.iter().any(|site| site.path == CopyPath::Empty)
        || !copies.iter().any(|site| site.path == CopyPath::Record)
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

fn copy_site(f: &Function, inst: Inst) -> Option<CopySite> {
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
    let [_, _, bytes] = *args.as_slice() else {
        return None;
    };
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
    let path = match (op, f.value_type(arg), int_const(f, shift)) {
        (ConvertOp::Uext, Type::I8, Some(3)) => CopyPath::Empty,
        (ConvertOp::Sext, Type::I32, Some(4)) => CopyPath::Record,
        _ => return None,
    };
    Some(CopySite {
        inst,
        args: [args[0], args[1], bytes],
        path,
    })
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
    }
}
