//! Global code motion (Click, "Global Code Motion / Global Value Numbering",
//! 1995): the scheduler for floating instructions.
//!
//! Pinned instructions stay put. Each pure instruction may legally go
//! anywhere between two blocks on the dominator tree:
//!
//! - **early**: the deepest block defining one of its operands. Anything
//!   higher would come before an operand exists.
//! - **late**: the common dominator of the blocks that use it. Anything
//!   lower would miss a use.
//!
//! Every block on the dominator-tree path from late up to early is
//! correct; GCM picks the one in the fewest loops, and among those the
//! latest, so a computation leaves loops when it can, and otherwise moves
//! into the branch that needs it, off the paths that do not.
//!
//! An instruction whose block does not change keeps its place: the order
//! within a block is already valid, and moving each instruction to just
//! before its use lengthens the lives of its operands instead (a chain of
//! `select`s sinking to the end of a function keeps every condition alive
//! across everything in between). An instruction that changes block goes
//! just before its first use there, or before the terminator.
//! Fixed object addresses with constant field offsets stay at the late
//! placement: recomputing them is cheap, while keeping hundreds of them
//! live across an interpreter loop makes the allocator spill them.
//!
//! Instructions without operands (constants, symbol and stack addresses)
//! are *rematerialized* instead: each block that uses one gets its own copy
//! just before its first use there. Recomputing one costs an instruction or
//! two; keeping one value alive from the top of the function, or across a
//! loop, costs a register the whole way, and GVN merges every `iconst 1` in
//! a function into one. (A register allocator that rematerializes would do
//! this better, at the point where it actually runs out of registers.)
//!
//! Users are placed before the instructions they use, so "first use" is
//! already final when an instruction is placed. Pure instructions cannot
//! trap, so running one on a path that did not run it before (hoisting out
//! of a loop that runs zero times) is harmless.

use crate::scratch::Scratch;
use keel_ir::{BinaryOp, Block, EntityRef, Function, InsertPoint, Inst, InstData, Value, ValueDef};

/// No instruction or block, in the dense per-instruction arrays below.
const NONE: u32 = u32::MAX;

pub fn run(func: &mut Function) -> bool {
    // An actual FLAGS observation cannot use the usual assumption that pure
    // arithmetic may freely cross pinned instructions. This is deliberately
    // function-wide until IR has explicit machine-FLAGS resource dependencies.
    // It does not promise continuity through other arithmetic/lowering passes.
    if func.layout_insts().any(|(_, i)| {
        matches!(func.inst_data(i), InstData::X86RflagsRegion { .. })
            || matches!(func.inst_data(i), InstData::Intrinsic { id, .. }
            if matches!(id, keel_ir::Intrinsic::X86RflagsRead | keel_ir::Intrinsic::X86RflagsSwap)
                || id.x86_string_port_io().is_some_and(|(_, _, _, owned)| owned))
    }) {
        return false;
    }
    // Legacy FLDENV/FRSTOR may own all eight physical x87 slots until a
    // later reset/save. A pure F80 operation moved across that interval can
    // overflow the hardware stack and corrupt its VALUE, even when portable
    // environment/exception ordering is otherwise unspecified. Keep F80
    // producers/consumers in their original blocks in these functions.
    let owned_x87 = func.layout_insts().any(|(_, i)| {
        matches!(func.inst_data(i), InstData::Intrinsic { id, .. }
            if id.x86_legacy_x87_image().is_some() || id.x86_owned_x87_stack().is_some())
    });
    let analysis = crate::analysis::get(func);
    let domtree = &analysis.dom;
    let loops = analysis.loops(func);

    // Depth of each block in the dominator tree; the entry is 0.
    let mut dom_depth = Scratch::filled(func.num_blocks(), 0u32);
    for &block in &domtree.rpo()[1..] {
        let idom = domtree.idom(block).expect("reachable non-entry block");
        dom_depth[block.index()] = dom_depth[idom.index()] + 1;
    }
    // Preorder interval of each dominator subtree: `a` dominates `b` when
    // `pre[a] <= pre[b] < end[a]`. `common_dominator` walks one idom at a
    // time, so test dominance first; a value used in each block of a long
    // dominator chain would otherwise cost a walk per use. Users come in RPO,
    // so a later user's block never strictly dominates the earlier ones.
    let children = domtree.children();
    let mut pre = Scratch::filled(func.num_blocks(), 0u32);
    let mut end = Scratch::filled(func.num_blocks(), 0u32);
    let mut next = 0;
    let mut stack = vec![(domtree.rpo()[0], false)];
    while let Some((block, done)) = stack.pop() {
        if done {
            end[block.index()] = next;
            continue;
        }
        pre[block.index()] = next;
        next += 1;
        stack.push((block, true));
        stack.extend(children[block].iter().map(|&child| (child, false)));
    }
    let dominates = |a: Block, b: Block| (pre[a.index()]..end[a.index()]).contains(&pre[b.index()]);

    let layout: usize = func.blocks().map(|b| func.block_insts(b).len()).sum();
    let mut floating: Scratch<Inst> = Scratch::new();
    floating.reserve(layout);
    // Who uses each value, among reachable instructions: (value, user) pairs
    // in layout order, then packed by value with `user_start`.
    let mut uses: Scratch<(u32, Inst)> = Scratch::new();
    uses.reserve(2 * layout);
    let mut last_user = Scratch::filled(func.num_values(), NONE);
    let mut user_start = Scratch::filled(func.num_values() + 1, 0u32);
    for &block in domtree.rpo() {
        for &inst in func.block_insts(block) {
            let data = func.inst_data(inst);
            let f80 = owned_x87 && {
                let mut found = func
                    .inst_results(inst)
                    .iter()
                    .any(|&v| func.value_type(v) == keel_ir::Type::F80);
                data.for_each_operand(|v| {
                    found |= func.value_type(func.resolve(v)) == keel_ir::Type::F80
                });
                found
            };
            if !data.is_pinned() && !f80 {
                floating.push(inst);
            }
            data.for_each_operand(|v| {
                let v = func.resolve(v).index();
                // All operands of one instruction are visited together;
                // repeated operands therefore only repeat the last user.
                if last_user[v] != inst.index() as u32 {
                    last_user[v] = inst.index() as u32;
                    user_start[v + 1] += 1;
                    uses.push((v as u32, inst));
                }
            });
        }
    }
    for v in 1..user_start.len() {
        user_start[v] += user_start[v - 1];
    }
    let mut fill: Scratch<u32> = Scratch::new();
    fill.extend_from_slice(&user_start);
    let mut user_list = Scratch::filled(uses.len(), Inst::new(0));
    for &(v, inst) in &uses {
        let slot = &mut fill[v as usize];
        user_list[*slot as usize] = inst;
        *slot += 1;
    }
    drop(uses);
    drop(fill);
    drop(last_user);
    let users = |value: Value| {
        let v = value.index();
        &user_list[user_start[v] as usize..user_start[v + 1] as usize]
    };

    // Early placement, with operands before users: blocks in RPO and
    // definitions before uses within a block.
    // Placement never changes an operand count, so record which floating
    // instructions have none (in `floating` order) while visiting operands.
    let mut early = Scratch::filled(func.num_insts(), NONE);
    let mut leaf: Scratch<bool> = Scratch::new();
    leaf.reserve(floating.len());
    for &inst in &floating {
        let mut block = func.entry_block().expect("reachable code has an entry");
        let mut operands = 0;
        func.inst_data(inst).for_each_operand(|v| {
            operands += 1;
            let def_block = match func.value_def(func.resolve(v)) {
                ValueDef::Param(b, _) => b,
                ValueDef::Result(def, _) => match early[def.index()] {
                    NONE => func.inst_block(def),
                    b => Block::new(b as usize),
                },
                ValueDef::Alias(_) => unreachable!("resolved"),
            };
            if dom_depth[def_block.index()] > dom_depth[block.index()] {
                block = def_block;
            }
        });
        early[inst.index()] = block.index() as u32;
        leaf.push(operands == 0);
    }

    // Late placement and the final choice, users first.
    let mut changed = false;
    // Generation markers avoid allocating or clearing a membership set for
    // every instruction. Each producer is its own unique generation.
    let mut membership = Scratch::filled(func.num_insts(), NONE);
    let mut groups = Groups {
        slot: vec![(NONE, 0); func.num_blocks()],
        blocks: Vec::new(),
        members: Vec::new(),
    };
    let mut inst_users: Scratch<Inst> = Scratch::new();
    for (&inst, &leaf) in floating.iter().zip(&leaf).rev() {
        // The users of all its results (`*_overflow` has two).
        inst_users.clear();
        for &result in func.inst_results(inst) {
            for &user in users(result) {
                if membership[user.index()] != inst.index() as u32 {
                    membership[user.index()] = inst.index() as u32;
                    inst_users.push(user);
                }
            }
        }
        let mut late: Option<Block> = None;
        for &user in &inst_users {
            let b = func.inst_block(user);
            late = Some(match late {
                None => b,
                Some(l) if dominates(l, b) => l,
                Some(l) => domtree.common_dominator(l, b),
            });
        }
        // Unused: leave it for DCE.
        let Some(late) = late else { continue };
        let early = Block::new(early[inst.index()] as usize);

        if leaf {
            changed |= rematerialize(func, inst, &inst_users, &membership, &mut groups);
            continue;
        }

        let mut best = late;
        let mut block = late;
        let mut cheap_address = None;
        // Only a strictly shallower block replaces `best`, so the walk can
        // stop once `best` is outside every loop.
        while block != early && loops.depth(best) != 0 {
            block = domtree.idom(block).expect("early dominates late");
            // An address of a fixed object plus a constant is cheap to
            // recompute. Hoisting it keeps a separate pointer live across
            // the loop (and calls), often costing more spills than it saves.
            if loops.depth(block) < loops.depth(best)
                && !*cheap_address
                    .get_or_insert_with(|| fixed_address(func, func.first_result(inst)))
            {
                best = block;
            }
        }

        if best == func.inst_block(inst) {
            continue;
        }
        let at = match first_user_in(func, best, inst, &membership) {
            Some(user) => InsertPoint::Before(user),
            None => InsertPoint::Before(*func.block_insts(best).last().unwrap()),
        };
        func.move_inst(inst, at);
        changed = true;
    }
    changed
}

/// Fixed object addresses, including constant field offsets. Keep these
/// near their users; ordinary pointer arithmetic still gets loop motion.
fn fixed_address(func: &Function, value: Value) -> bool {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(value)) else {
        return false;
    };
    match func.inst_data(inst) {
        InstData::StackAddr { .. } | InstData::SymbolAddr { .. } => true,
        InstData::Binary {
            op: BinaryOp::Ptradd,
            args,
        } => {
            let ValueDef::Result(offset, _) = func.value_def(func.resolve(args[1])) else {
                return false;
            };
            matches!(func.inst_data(offset), InstData::Iconst { .. })
                && fixed_address(func, args[0])
        }
        _ => false,
    }
}

/// Gives each block using operand-free `inst` its own copy, just before the
/// first use in that block. `inst` itself serves its own block if it has
/// users there, or else the first user block.
/// Scratch for grouping a constant's users by block, reused across calls.
struct Groups {
    /// Per block: the producer that last grouped it, and its group there.
    slot: Vec<(u32, u32)>,
    /// Each group's block, in order of first use.
    blocks: Vec<Block>,
    /// (group, user), sorted stably by group.
    members: Vec<(u32, Inst)>,
}

fn rematerialize(
    func: &mut Function,
    inst: Inst,
    users: &[Inst],
    membership: &[u32],
    groups: &mut Groups,
) -> bool {
    let result = func.first_result(inst);
    let home = func.inst_block(inst);
    let first_block = func.inst_block(users[0]);
    // A single user block needs no grouping or copies. This is the common
    // case; preserve its existing position when that block is already home.
    if users
        .iter()
        .all(|&user| func.inst_block(user) == first_block)
    {
        if first_block == home {
            return false;
        }
        let first =
            first_user_in(func, first_block, inst, membership).expect("a user in its block");
        func.move_inst(inst, InsertPoint::Before(first));
        return true;
    }
    // Group the users by block, blocks in order of first use and users in
    // their given order within each.
    let Groups {
        slot,
        blocks,
        members,
    } = groups;
    let producer = inst.index() as u32;
    blocks.clear();
    members.clear();
    for &user in users {
        let b = func.inst_block(user);
        let slot = &mut slot[b.index()];
        if slot.0 != producer {
            *slot = (producer, blocks.len() as u32);
            blocks.push(b);
        }
        members.push((slot.1, user));
    }
    members.sort_by_key(|&(group, _)| group);
    // The group that keeps `inst` itself rather than a copy.
    let kept = match blocks.iter().position(|&b| b == home) {
        // Keep its place in its own block.
        Some(i) => i,
        None => {
            let user =
                first_user_in(func, blocks[0], inst, membership).expect("a user in its block");
            func.move_inst(inst, InsertPoint::Before(user));
            0
        }
    };
    let changed = blocks.len() > 1 || func.inst_block(inst) != home;
    let data = func.inst_data(inst).clone();
    let mut start = 0;
    for (group, &block) in blocks.iter().enumerate() {
        let end = start + members[start..].partition_point(|&(g, _)| g as usize == group);
        let local = &members[start..end];
        start = end;
        if group == kept {
            continue;
        }
        let first = first_user_in(func, block, inst, membership).expect("a user in its block");
        let copy = func.insert_inst(InsertPoint::Before(first), data.clone());
        func.set_inst_loc(copy, func.inst_loc(inst));
        let copy = func.first_result(copy);
        for &(_, user) in local {
            // Rewrite in place: resolving reads only values, not `user`.
            let mut data = std::mem::replace(func.inst_data_mut(user), InstData::Trap);
            data.for_each_operand_mut(|v| {
                if func.resolve(*v) == result {
                    *v = copy;
                }
            });
            *func.inst_data_mut(user) = data;
        }
    }
    changed
}

/// The first instruction in `block`, in layout order, among `users`.
fn first_user_in(
    func: &Function,
    block: Block,
    producer: Inst,
    membership: &[u32],
) -> Option<Inst> {
    func.block_insts(block)
        .iter()
        .copied()
        // Rematerialization creates new instructions after the membership
        // array was built; they cannot be users of an original instruction.
        .find(|inst| membership.get(inst.index()) == Some(&(producer.index() as u32)))
}

#[cfg(test)]
mod legacy_x87_tests {
    use super::*;

    #[test]
    fn owned_images_pin_f80_results_operands_and_constants_only() {
        let src = r#"
func @f(ptr,ptr,ptr,i64) {
block0(v0:ptr,v1:ptr,v2:ptr,v3:i64):
 v4 = load.f80 align 1 v1
 v5 = fconst.f80 0x3fff, 0x8000000000000000
 v6 = fadd v4,v5
 v7 = fneg v6
 v8 = fcmp.eq v4,v5
 v9 = fptosi.i64 v4
 v10 = iconst.i64 7
 v11 = iadd v3,v10
 intrinsic x86_x87_frstor32(v0)
 jump block1
block1:
 store align 1 v5,v2
 store align 1 v6,v2
 store align 1 v7,v2
 store align 1 v8,v2
 store align 1 v9,v2
 store align 1 v11,v2
 intrinsic x86_x87_fnsave32(v0)
 return
}
"#;
        let mut m = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let f = &mut m.functions[0];
        let entry = f.entry_block().unwrap();
        let mut protected = Vec::new();
        let mut integer = None;
        for (_, i) in f.layout_insts() {
            match f.inst_data(i) {
                InstData::F80const { .. }
                | InstData::Unary { .. }
                | InstData::Fcmp { .. }
                | InstData::Convert { .. }
                | InstData::Binary {
                    op: BinaryOp::Fadd, ..
                } => protected.push(i),
                InstData::Binary {
                    op: BinaryOp::Iadd, ..
                } => integer = Some(i),
                _ => {}
            }
        }
        assert_eq!(protected.len(), 5);
        assert!(run(f));
        for i in protected {
            assert_eq!(
                f.inst_block(i),
                entry,
                "F80 operation moved: {:?}",
                f.inst_data(i)
            );
        }
        assert_ne!(
            f.inst_block(integer.unwrap()),
            entry,
            "integer GCM disabled"
        );
        keel_ir::verify_module(&m).unwrap();
    }
}
