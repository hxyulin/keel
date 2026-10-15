//! Bounded, side-effect-free ASCII prefix scans. Chunk failures re-enter the
//! original scalar loop, preserving its precise first-match pointer live-out.
use super::*;
use keel_ir::MemoryFlags;

struct Scan {
    header: Block,
    pre: Block,
    start: Value,
    end: Value,
}

fn last(f: &Function, block: Block) -> Option<Inst> {
    f.block_insts(block).last().copied()
}

fn same_edge(f: &Function, a: &BlockCall, b: &BlockCall) -> bool {
    a.block == b.block
        && a.args.len() == b.args.len()
        && a.args
            .iter()
            .zip(&b.args)
            .all(|(&a, &b)| f.resolve(a) == f.resolve(b))
}

fn analyze(
    f: &Function,
    header: Block,
    analysis: &mut Option<(ControlFlowGraph, DominatorTree)>,
) -> Option<Scan> {
    let [pointer] = *f.block_params(header) else {
        return None;
    };
    if f.value_type(pointer) != Type::Ptr || f.block_insts(header).len() != 2 {
        return None;
    }
    let [comparison, branch_inst] = *f.block_insts(header) else {
        return None;
    };
    let D::Icmp { cond, args } = *f.inst_data(comparison) else {
        return None;
    };
    let D::Brif {
        cond: condition,
        ref dests,
    } = *f.inst_data(branch_inst)
    else {
        return None;
    };
    if f.resolve(condition) != f.first_result(comparison) {
        return None;
    }
    let (end, test_index) = match (
        cond,
        f.resolve(args[0]) == pointer,
        f.resolve(args[1]) == pointer,
    ) {
        (IntCC::Ult, true, false) => (args[1], 0),
        (IntCC::Ugt, false, true) => (args[0], 0),
        (IntCC::Uge, true, false) => (args[1], 1),
        (IntCC::Ule, false, true) => (args[0], 1),
        _ => return None,
    };
    let test = dests[test_index].block;
    let exit = &dests[1 - test_index];
    if test == header || !dests[test_index].args.is_empty() || !f.block_params(test).is_empty() {
        return None;
    }
    let D::Brif {
        cond: character_condition,
        dests: character_dests,
    } = f.inst_data(last(f, test)?)
    else {
        return None;
    };
    let character_cmp = def(f, *character_condition)?;
    let D::Icmp {
        cond: mut predicate,
        args: mut character_args,
    } = *f.inst_data(character_cmp)
    else {
        return None;
    };
    // Normalize the constant to the right without depending on canonicalization.
    if constant(f, character_args[0]).is_some() {
        character_args.swap(0, 1);
        predicate = match predicate {
            IntCC::Ult => IntCC::Ugt,
            IntCC::Ugt => IntCC::Ult,
            IntCC::Slt => IntCC::Sgt,
            IntCC::Sgt => IntCC::Slt,
            IntCC::Uge => IntCC::Ule,
            IntCC::Ule => IntCC::Uge,
            IntCC::Sge => IntCC::Sle,
            IntCC::Sle => IntCC::Sge,
            other => other,
        };
    }
    let limit = constant(f, character_args[1])?;
    let mut allowed = HashSet::from([last(f, test)?, character_cmp]);
    if let Some(i) = def(f, character_args[1]) {
        allowed.insert(i);
    }
    let mut byte = f.resolve(character_args[0]);
    let extended = if let Some(i) = def(f, byte)
        && let D::Convert {
            op: ConvertOp::Uext,
            arg,
            ..
        } = *f.inst_data(i)
    {
        allowed.insert(i);
        byte = f.resolve(arg);
        true
    } else {
        false
    };
    if f.value_type(byte) != Type::I8 {
        return None;
    }
    let ascii_index = match (predicate, limit, extended) {
        (IntCC::Ult, 128, _) | (IntCC::Slt, 128, true) | (IntCC::Sge, 0, false) => 0,
        (IntCC::Uge, 128, _) | (IntCC::Sge, 128, true) | (IntCC::Slt, 0, false) => 1,
        _ => return None,
    };
    if !same_edge(f, &character_dests[1 - ascii_index], exit) {
        return None;
    }
    let step = character_dests[ascii_index].block;
    if !character_dests[ascii_index].args.is_empty()
        || !f.block_params(step).is_empty()
        || step == test
        || step == header
        || f.block_insts(step).len() != 3
    {
        return None;
    }
    let load = def(f, byte)?;
    let D::Load {
        ty: Type::I8,
        addr,
        flags,
    } = *f.inst_data(load)
    else {
        return None;
    };
    if flags.volatile || f.resolve(addr) != pointer || f.inst_block(load) != test {
        return None;
    }
    allowed.insert(load);
    if f.block_insts(test).iter().any(|i| !allowed.contains(i)) {
        return None;
    }
    let D::Jump { dest: back } = f.inst_data(last(f, step)?) else {
        return None;
    };
    if back.block != header || back.args.len() != 1 {
        return None;
    }
    let increment = def(f, back.args[0])?;
    let D::Binary {
        op: B::Ptradd,
        args: [base, offset],
    } = *f.inst_data(increment)
    else {
        return None;
    };
    if f.resolve(base) != pointer || constant(f, offset) != Some(1) {
        return None;
    }
    let offset_inst = def(f, offset)?;
    if !f.block_insts(step).contains(&offset_inst) || !f.block_insts(step).contains(&increment) {
        return None;
    }
    let (cfg, dom) = analysis.get_or_insert_with(|| {
        let cfg = ControlFlowGraph::compute(f);
        let dom = DominatorTree::compute(f, &cfg);
        (cfg, dom)
    });
    if cfg.preds(test) != [header] || cfg.preds(step) != [test] || cfg.preds(header).len() != 2 {
        return None;
    }
    let pre = *cfg.preds(header).iter().find(|&&p| p != step)?;
    if [header, test, step].contains(&pre)
        || !dom.dominates(header, step)
        || !invariant(f, dom, pre, end)
    {
        return None;
    }
    let D::Jump { dest: entry } = f.inst_data(last(f, pre)?) else {
        return None;
    };
    if entry.block != header || entry.args.len() != 1 {
        return None;
    }
    // Only the induction pointer may escape; scalar re-entry defines it before
    // either original exit and recomputes any character-dependent values.
    for (block, inst) in f.layout_insts() {
        if [header, test, step].contains(&block) {
            continue;
        }
        let mut escapes = false;
        f.inst_data(inst).for_each_operand(|v| {
            if let Some(i) = def(f, v)
                && [header, test, step].contains(&f.inst_block(i))
            {
                escapes = true;
            }
        });
        if escapes {
            return None;
        }
    }
    Some(Scan {
        header,
        pre,
        start: f.resolve(entry.args[0]),
        end: f.resolve(end),
    })
}

fn transform(f: &mut Function, c: Scan) {
    let peel = f.create_block();
    let peel_bound = f.create_block();
    let peel_byte = f.create_block();
    let peel_step = f.create_block();
    let check = f.create_block();
    let chunk = f.create_block();
    let advance = f.create_block();
    let prefix_pointer = f.append_block_param(peel, Type::Ptr);
    let address = emit(
        f,
        peel,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg: prefix_pointer,
        },
    );
    let alignment_mask = iconst(f, peel, Type::I64, 15);
    let alignment = bin(f, peel, B::And, address, alignment_mask);
    let zero = iconst(f, peel, Type::I64, 0);
    let aligned = cmp(f, peel, IntCC::Eq, alignment, zero);
    branch(
        f,
        peel,
        aligned,
        check,
        vec![prefix_pointer],
        peel_bound,
        vec![],
    );
    let before_end = cmp(f, peel_bound, IntCC::Ult, prefix_pointer, c.end);
    branch(
        f,
        peel_bound,
        before_end,
        peel_byte,
        vec![],
        c.header,
        vec![prefix_pointer],
    );
    let byte = emit(
        f,
        peel_byte,
        D::Load {
            ty: Type::I8,
            addr: prefix_pointer,
            flags: MemoryFlags::default(),
        },
    );
    let limit = iconst(f, peel_byte, Type::I8, 128);
    let ascii = cmp(f, peel_byte, IntCC::Ult, byte, limit);
    branch(
        f,
        peel_byte,
        ascii,
        peel_step,
        vec![],
        c.header,
        vec![prefix_pointer],
    );
    let one = iconst(f, peel_step, Type::I64, 1);
    let next = bin(f, peel_step, B::Ptradd, prefix_pointer, one);
    jump(f, peel_step, peel, vec![next]);
    // A bound alone does not prove every byte before it readable: the scalar
    // scan may stop before an inaccessible page. Supported targets protect
    // memory at granules containing whole aligned 16-byte chunks. Peeling
    // ensures even speculative bytes after a match cannot cross a granule.
    let pointer = f.append_block_param(check, Type::Ptr);
    let current = emit(
        f,
        check,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg: pointer,
        },
    );
    let end = emit(
        f,
        check,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg: c.end,
        },
    );
    let ordered = cmp(f, check, IntCC::Ule, current, end);
    let remaining = bin(f, check, B::Isub, end, current);
    let width = iconst(f, check, Type::I64, 16);
    let enough = cmp(f, check, IntCC::Uge, remaining, width);
    let safe = bin(f, check, B::And, ordered, enough);
    branch(f, check, safe, chunk, vec![], c.header, vec![pointer]);
    let packed = emit(
        f,
        chunk,
        D::Load {
            ty: Type::Vector(VectorType::I64x2),
            addr: pointer,
            flags: MemoryFlags::default(),
        },
    );
    let low = emit(
        f,
        chunk,
        D::VectorExtract {
            arg: packed,
            lane: 0,
        },
    );
    let high = emit(
        f,
        chunk,
        D::VectorExtract {
            arg: packed,
            lane: 1,
        },
    );
    let bits = bin(f, chunk, B::Or, low, high);
    let mask = iconst(f, chunk, Type::I64, 0x8080808080808080u64 as i64);
    let high_bits = bin(f, chunk, B::And, bits, mask);
    let zero = iconst(f, chunk, Type::I64, 0);
    let ascii = cmp(f, chunk, IntCC::Eq, high_bits, zero);
    branch(f, chunk, ascii, advance, vec![], c.header, vec![pointer]);
    let next = bin(f, advance, B::Ptradd, pointer, width);
    jump(f, advance, check, vec![next]);
    *f.inst_data_mut(last(f, c.pre).unwrap()) = D::Jump {
        dest: BlockCall {
            block: peel,
            args: vec![c.start],
        },
    };
}

pub(super) fn run(f: &mut Function) -> bool {
    let headers: Vec<_> = f.blocks().collect();
    let mut analysis = None;
    let mut changed = false;
    for header in headers {
        if let Some(scan) = analyze(f, header, &mut analysis) {
            transform(f, scan);
            analysis = None;
            changed = true;
        }
    }
    changed
}
