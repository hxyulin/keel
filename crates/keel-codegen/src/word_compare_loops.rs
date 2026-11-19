//! Find small, call-free word-comparison loops for native block layout.
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{BinaryOp, Block, Function, InstData, Type, ValueDef};

#[derive(Default)]
pub(crate) struct Layout {
    pub(crate) headers: FxHashSet<Block>,
    pub(crate) backedges: FxHashMap<Block, Block>,
}

pub(crate) fn layout(f: &Function) -> Layout {
    let mut layout = Layout::default();
    if f.blocks().count() < 2 {
        return layout;
    }
    let candidates: Vec<_> = f.blocks().filter(|&b| word_compare(f, b)).collect();
    if candidates.is_empty() {
        return layout;
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
        let latches = pending.clone();
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
            // Padding an interior compare block puts NOPs on the hot
            // fall-through path from the bounds check.
            if candidates.iter().any(|block| members.contains(block)) {
                layout.headers.insert(header);
                for latch in latches {
                    let successors = cfg.succs(latch);
                    if successors.len() == 2
                        && successors.iter().any(|block| !members.contains(block))
                    {
                        layout.backedges.insert(latch, header);
                    }
                }
            }
            // At most 248 bytes of loop padding per function.
            if layout.headers.len() == 8 {
                break;
            }
        }
    }
    layout
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
    fn bounds_check_falls_through_to_an_unpadded_word_compare() {
        let module = keel_ir::parse(
            "func @f(ptr, ptr, i8) -> i8 {
block0(v0: ptr, v1: ptr, v2: i8):
    jump block1
block1:
    brif v2, block2, block3
block2:
    v3 = load.i64 v0
    v4 = load.i64 v1
    v5 = xor v3, v4
    brif v5, block3, block1
block3:
    return v2
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let func = &module.functions[0];
        assert_eq!(
            layout(func).headers,
            [func.blocks().nth(1).unwrap()].into_iter().collect()
        );
    }

    #[test]
    fn a_non_fallthrough_latch_branches_directly_to_the_word_loop() {
        let module = keel_ir::parse(
            "func @f(ptr,ptr,i8,i8) -> i64 {
entry(v0:ptr,v1:ptr,v2:i8,v7:i8):
 brif v7,exit,compare
compare:
 v3 = load.i64 v0
 v4 = load.i64 v1
 v5 = xor v3,v4
 brif v5,mismatch,latch
latch:
 brif v2,compare,exit
mismatch:
 return v5
exit:
 v6 = uext.i64 v2
 return v6
}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let pipeline = crate::CodegenPipeline {
            selector: &crate::SelectionTier::Full,
            allocator: &crate::regalloc::AllocationTier::Fast,
        };
        for target in crate::Target::NATIVE {
            let text = crate::compile_with_pipeline(&module, target, &pipeline).unwrap();
            assert!(
                text.lines().any(|line| {
                    let op = line.split_whitespace().next().unwrap_or_default();
                    matches!(op, "jne" | "b.ne" | "cbnz")
                        && line.trim_end_matches('"').ends_with("f_block1")
                }),
                "{text}"
            );
        }
    }

    #[test]
    fn word_loop_only_and_calls_excluded() {
        let source = "func @loop(ptr, ptr) -> i64 {\nblock0(v0: ptr, v1: ptr):\n jump block1\nblock1:\n v2 = load.i64 v0\n v3 = load.i64 v1\n v4 = xor v2, v3\n brif v4, block1, block2(v4)\nblock2(v5: i64):\n return v5\n}\n";
        let m = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let layout = layout(&m.functions[0]);
        let h = &layout.headers;
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
        let header = m.functions[0].blocks().nth(1).unwrap();
        assert!(h.contains(&header));
        assert_eq!(layout.backedges.get(&header), Some(&header));
        for body in [
            source.replace("xor v2, v3", "iadd v2, v3"),
            format!(
                "decl @side()\n{}",
                source.replace(" v2 =", " call @side()\n v2 =")
            ),
        ] {
            let m = keel_ir::parse(&body).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert!(super::layout(&m.functions[0]).headers.is_empty());
        }
    }
}
