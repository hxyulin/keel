//! Remove repeated range checks from word comparisons that can exit early.
use super::*;
use keel_ir::{MemoryFlags, ValueDef};

const WORD_BYTES: i64 = 8;
const GROUP_WORDS: i64 = 4;

pub(crate) struct LoopUnrollWords;

impl crate::Pass for LoopUnrollWords {
    fn name(&self) -> &'static str {
        "loop-unroll-words"
    }
    fn description(&self) -> &'static str {
        "unroll guarded word comparisons without reading past a mismatch"
    }
    fn deterministic(&self) -> bool {
        true
    }
    fn run(&self, f: &mut Function, _: &crate::OptimizationContext) -> bool {
        if f.debug_loc.is_some() || f.debug_return.is_some() || !f.debug_vars.is_empty() {
            return false;
        }
        let blocks = f.blocks().collect::<Vec<_>>();
        let mut changed = false;
        for block in blocks {
            if let Some(words) = analyze(f, block) {
                unroll(f, words);
                changed = true;
            }
        }
        changed
    }
}

struct WordLoop {
    header: Block,
    incoming: Inst,
    accesses: [(usize, MemoryFlags); 2],
    end: Value,
    mismatch: Block,
    live_out: Vec<(usize, Value)>,
    suffix: BlockCall,
    next: [Value; 2],
}

fn advance(f: &Function, next: Value, ptr: Value) -> Option<Inst> {
    let inst = def(f, next)?;
    match *f.inst_data(inst) {
        D::Binary {
            op: B::Ptradd,
            args,
        } if f.resolve(args[0]) == ptr && int_const(f, args[1]) == Some(WORD_BYTES) => Some(inst),
        _ => None,
    }
}

fn analyze(f: &Function, header: Block) -> Option<WordLoop> {
    let [left, right] = *f.block_params(header) else {
        return None;
    };
    let pointers = [left, right];
    if pointers.iter().any(|&v| f.value_type(v) != Type::Ptr) {
        return None;
    }

    let [first, second, diff, branch] = *f.block_insts(header) else {
        return None;
    };
    let access = |inst| match *f.inst_data(inst) {
        D::Load {
            ty: Type::I64,
            addr,
            flags,
        } if !flags.volatile => pointers
            .iter()
            .position(|&v| v == f.resolve(addr))
            .map(|idx| (idx, flags)),
        _ => None,
    };
    let accesses = [access(first)?, access(second)?];
    if accesses[0].0 == accesses[1].0 {
        return None;
    }
    let loaded = [f.first_result(first), f.first_result(second)];
    let D::Binary { op: B::Xor, args } = *f.inst_data(diff) else {
        return None;
    };
    let args = args.map(|v| f.resolve(v));
    if args != loaded && args != [loaded[1], loaded[0]] {
        return None;
    }
    let diff = f.first_result(diff);
    let D::Brif { cond, dests } = f.inst_data(branch) else {
        return None;
    };
    let [bad, good] = &**dests;
    let (mismatch, latch) = (bad.block, good.block);
    if f.resolve(*cond) != diff || !bad.args.is_empty() || !good.args.is_empty() {
        return None;
    }
    let body = f.block_insts(latch);
    let &back = body.last()?;
    let D::Brif { cond, dests } = f.inst_data(back) else {
        return None;
    };
    let [again, suffix] = &**dests;
    let [next_left, next_right] = *again.args else {
        return None;
    };
    if again.block != header || [header, latch, mismatch].contains(&suffix.block) {
        return None;
    }
    let step_left = advance(f, next_left, left)?;
    let step_right = advance(f, next_right, right)?;
    let compare = def(f, *cond)?;
    let D::Icmp { cond, args } = *f.inst_data(compare) else {
        return None;
    };
    let [a, b] = args.map(|v| f.resolve(v));
    let next = [next_left, next_right].map(|v| f.resolve(v));
    let end = match cond {
        IntCC::Ult if a == next[0] => b,
        IntCC::Ugt if b == next[0] => a,
        _ => return None,
    };
    if pointers.contains(&end)
        || def(f, end).is_some_and(|i| [header, latch].contains(&f.inst_block(i)))
        || body.iter().any(|&i| {
            ![step_left, step_right, compare, back].contains(&i)
                && !matches!(f.inst_data(i), D::Iconst { .. })
        })
    {
        return None;
    }
    // The suffix edge can carry the new recurrence values and invariants.
    // Other values defined in the scalar loop have no equivalent on this edge.
    let loop_local = |v| match f.value_def(v) {
        ValueDef::Param(b, _) => b == header || b == latch,
        ValueDef::Result(i, _) => [header, latch].contains(&f.inst_block(i)),
        ValueDef::Alias(_) => unreachable!("resolved operand"),
    };
    if suffix
        .args
        .iter()
        .map(|&v| f.resolve(v))
        .any(|v| loop_local(v) && !next.contains(&v))
    {
        return None;
    }
    let analysis = crate::analysis::get(f);
    if Some(header) == f.entry_block()
        || analysis.cfg.preds(latch) != [header]
        || analysis.cfg.preds(mismatch) != [header]
        || !matches!(
            f.block_insts(mismatch).last().map(|&i| f.inst_data(i)),
            Some(D::Return { .. })
        )
    {
        return None;
    }
    let preds = analysis.cfg.preds(header);
    let [a, b] = *preds else {
        return None;
    };
    let pre = if a == latch {
        b
    } else if b == latch {
        a
    } else {
        return None;
    };
    let &incoming = f.block_insts(pre).last()?;
    if !matches!(f.inst_data(incoming), D::Brif { .. } | D::Jump { .. }) {
        return None;
    }

    let values = [left, right, loaded[0], loaded[1], diff];
    let mut needed = [false; 5];
    for (block, inst) in f.layout_insts() {
        if [header, latch, mismatch].contains(&block) && f.inst_loc(inst).is_some() {
            return None;
        }
        if block == header || block == latch {
            continue;
        }
        let mut supported = true;
        f.inst_data(inst).for_each_operand(|v| {
            let v = f.resolve(v);
            if loop_local(v) {
                match values.iter().position(|&x| x == v && block == mismatch) {
                    Some(index) => needed[index] = true,
                    None => supported = false,
                }
            }
        });
        if !supported {
            return None;
        }
    }
    Some(WordLoop {
        header,
        incoming,
        accesses,
        end,
        mismatch,
        live_out: values
            .into_iter()
            .enumerate()
            .filter(|(i, _)| needed[*i])
            .collect(),
        suffix: suffix.clone(),
        next,
    })
}

fn ptr_bits(f: &mut Function, b: Block, v: Value) -> Value {
    emit(
        f,
        b,
        D::Convert {
            op: ConvertOp::PtrToInt,
            ty: Type::I64,
            arg: v,
        },
    )
}

fn unroll(f: &mut Function, words: WordLoop) {
    let mismatch_values = words.live_out.iter().map(|&(_, v)| v).collect::<Vec<_>>();
    let replacement = mismatch_values
        .iter()
        .map(|&v| (v, f.append_block_param(words.mismatch, f.value_type(v))))
        .collect::<Vec<_>>();
    for inst in f.block_insts(words.mismatch).to_vec() {
        let mut data = f.inst_data(inst).clone();
        data.for_each_operand_mut(|v| {
            if let Some(&(_, p)) = replacement.iter().find(|&&(from, _)| from == f.resolve(*v)) {
                *v = p;
            }
        });
        *f.inst_data_mut(inst) = data;
    }
    let scalar_branch = *f.block_insts(words.header).last().unwrap();
    f.inst_data_mut(scalar_branch).branch_dests_mut()[0].args = mismatch_values;

    let guard = f.create_block();
    let group = f.create_block();
    let fallback = f.create_block();
    let initial: [Value; 2] = std::array::from_fn(|_| f.append_block_param(guard, Type::Ptr));
    let base: [Value; 2] = std::array::from_fn(|_| f.append_block_param(group, Type::Ptr));

    let end = ptr_bits(f, guard, words.end);
    let span = iconst(f, guard, Type::I64, (GROUP_WORDS - 1) * WORD_BYTES);
    let limit = bin(f, guard, B::Isub, end, span);
    let representable = cmp(f, guard, IntCC::Uge, end, span);
    let start = ptr_bits(f, guard, initial[0]);
    let fits = cmp(f, guard, IntCC::Ult, start, limit);
    let fits = bin(f, guard, B::And, representable, fits);
    branch(
        f,
        guard,
        fits,
        group,
        initial.to_vec(),
        words.header,
        initial.to_vec(),
    );

    // Every successful comparison reaches the next word. A mismatch takes
    // the original exit before any subsequent load can execute.
    let mut block = group;
    for i in 0..GROUP_WORDS {
        let ptrs = if i == 0 {
            base
        } else {
            let offset = iconst(f, block, Type::I64, i * WORD_BYTES);
            base.map(|p| bin(f, block, B::Ptradd, p, offset))
        };
        let loaded = words.accesses.map(|(idx, flags)| {
            emit(
                f,
                block,
                D::Load {
                    ty: Type::I64,
                    addr: ptrs[idx],
                    flags,
                },
            )
        });
        let diff = bin(f, block, B::Xor, loaded[0], loaded[1]);
        let values = [ptrs[0], ptrs[1], loaded[0], loaded[1], diff];
        let args = words.live_out.iter().map(|&(i, _)| values[i]).collect();
        let next = f.create_block();
        branch(f, block, diff, words.mismatch, args, next, Vec::new());
        block = next;
    }
    let width = iconst(f, block, Type::I64, GROUP_WORDS * WORD_BYTES);
    let advanced = base.map(|p| bin(f, block, B::Ptradd, p, width));
    let start = ptr_bits(f, block, advanced[0]);
    let more = cmp(f, block, IntCC::Ult, start, limit);
    branch(
        f,
        block,
        more,
        group,
        advanced.to_vec(),
        fallback,
        Vec::new(),
    );

    let more = cmp(f, fallback, IntCC::Ult, advanced[0], words.end);
    let suffix = words
        .suffix
        .args
        .iter()
        .map(|&v| {
            let v = f.resolve(v);
            words
                .next
                .iter()
                .position(|&old| old == v)
                .map_or(v, |i| advanced[i])
        })
        .collect();
    branch(
        f,
        fallback,
        more,
        words.header,
        advanced.to_vec(),
        words.suffix.block,
        suffix,
    );
    for dest in f.inst_data_mut(words.incoming).branch_dests_mut() {
        if dest.block == words.header {
            dest.block = guard;
        }
    }
}
