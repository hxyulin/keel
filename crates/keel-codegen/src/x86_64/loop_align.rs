//! Bound fetch alignment to small, call-free word-comparison loops.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::FxHashSet;
use keel_ir::{BinaryOp, Block, Function, InstData, Type, ValueDef};

pub(super) fn headers(f: &Function) -> FxHashSet<Block> {
    let mut headers = FxHashSet::default();
    if f.blocks().count() < 2 {
        return headers;
    }
    let candidates: Vec<_> = f.blocks().filter(|&b| word_compare(f, b)).collect();
    if candidates.is_empty() {
        return headers;
    }
    let cfg = ControlFlowGraph::compute(f);
    let dom = DominatorTree::compute(f, &cfg);
    for &header in dom.rpo() {
        let mut pending: Vec<_> = cfg
            .preds(header)
            .iter()
            .copied()
            .filter(|&p| dom.dominates(header, p))
            .collect();
        if pending.is_empty() || Some(header) == f.entry_block() {
            continue;
        }
        let mut members = FxHashSet::default();
        members.insert(header);
        let mut instructions = f.block_insts(header).len();
        while let Some(b) = pending.pop() {
            if !members.insert(b) {
                continue;
            }
            instructions += f.block_insts(b).len();
            if members.len() > 8 || instructions > 64 {
                break;
            }
            pending.extend(
                cfg.preds(b)
                    .iter()
                    .copied()
                    .filter(|&p| dom.is_reachable(p)),
            );
        }
        if members.len() <= 8
            && instructions <= 64
            && members.iter().all(|&b| {
                f.block_insts(b)
                    .iter()
                    .all(|&i| f.inst_data(i).call_args().is_none())
            })
        {
            // Compare-word loops are particularly sensitive to splitting their
            // load/XOR/test sequence across fetch boundaries.
            for &block in &candidates {
                if members.contains(&block) && headers.len() < 8 {
                    headers.insert(header);
                    if headers.len() < 8 {
                        headers.insert(block);
                    }
                }
            }
            // At most 248 bytes of loop padding per function.
            if headers.len() == 8 {
                break;
            }
        }
    }
    headers
}

fn word_compare(f: &Function, b: Block) -> bool {
    let Some(&term) = f.block_insts(b).last() else {
        return false;
    };
    let InstData::Brif { cond, .. } = f.inst_data(term) else {
        return false;
    };
    let ValueDef::Result(i, _) = f.value_def(f.resolve(*cond)) else {
        return false;
    };
    let InstData::Binary {
        op: BinaryOp::Xor,
        args,
    } = f.inst_data(i)
    else {
        return false;
    };
    args.iter().all(|&v| {
        let ValueDef::Result(def, _) = f.value_def(f.resolve(v)) else {
            return false;
        };
        matches!(f.inst_data(def), InstData::Load { ty: Type::I64, flags, .. } if !flags.volatile)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_loop_only_and_calls_excluded() {
        let source = "func @loop(ptr, ptr) -> i64 {\nblock0(v0: ptr, v1: ptr):\n jump block1\nblock1:\n v2 = load.i64 v0\n v3 = load.i64 v1\n v4 = xor v2, v3\n brif v4, block1, block2(v4)\nblock2(v5: i64):\n return v5\n}\n";
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let h = headers(&m.functions[0]);
        assert_eq!(h.len(), 1);
        let pipeline = crate::CodegenPipeline {
            selector: &crate::SelectionTier::Full,
            allocator: &crate::regalloc::AllocationTier::Full,
        };
        let text =
            crate::x86_64::compile_with_pipeline(&m, &crate::x86_64::REGISTERS, &pipeline).unwrap();
        assert!(text.contains(".p2align\t5"));
        let program =
            crate::x86_64::compile_program_with_pipeline(&m, &crate::x86_64::REGISTERS, &pipeline)
                .unwrap();
        assert_eq!(
            keel_asm::x86::assemble_program_object(&program).unwrap(),
            keel_asm::x86::assemble_object(&text).unwrap()
        );
        assert!(h.contains(&m.functions[0].blocks().nth(1).unwrap()));
        for body in [
            source.replace("xor v2, v3", "iadd v2, v3"),
            format!(
                "decl @side()\n{}",
                source.replace(" v2 =", " call @side()\n v2 =")
            ),
        ] {
            let m = keel_ir::parse(&body).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert!(headers(&m.functions[0]).is_empty());
        }
    }
}
