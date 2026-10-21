//! Keep the original algorithms as an independent differential oracle.
//! FullAffinity adds only the declared scalar consensus preference to its register choices.
use super::*;
use std::collections::BTreeSet;

struct ReferenceLiveness {
    live_in: Vec<BTreeSet<Value>>,
    live_out: Vec<BTreeSet<Value>>,
}
fn reference_liveness(
    func: &Function,
    cfg: &ControlFlowGraph,
    order: &[Block],
    selection: &crate::select::Selection,
) -> ReferenceLiveness {
    let n = func.num_blocks();
    let mut uses = vec![BTreeSet::new(); n];
    let mut defs = vec![BTreeSet::new(); n];
    for &block in order {
        let mut block_uses = BTreeSet::new();
        let mut block_defs: BTreeSet<Value> = func.block_params(block).iter().copied().collect();
        for &inst in func.block_insts(block) {
            if selection.covered(inst) {
                continue;
            }
            selection.for_each_operand(func, inst, |v| {
                if !block_defs.contains(&v) {
                    block_uses.insert(v);
                }
            });
            block_defs.extend(func.inst_results(inst));
        }
        uses[block.index()] = block_uses;
        defs[block.index()] = block_defs;
    }

    let mut live = ReferenceLiveness {
        live_in: vec![BTreeSet::new(); n],
        live_out: vec![BTreeSet::new(); n],
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &block in order.iter().rev() {
            let b = block.index();
            let out: BTreeSet<Value> = cfg
                .succs(block)
                .iter()
                .flat_map(|s| live.live_in[s.index()].iter().copied())
                .collect();
            let mut live_in = uses[b].clone();
            live_in.extend(out.difference(&defs[b]));
            if live_in != live.live_in[b] {
                live.live_in[b] = live_in;
                changed = true;
            }
            live.live_out[b] = out;
        }
    }
    live
}

fn reference_linear_scan(
    intervals: &[Interval],
    pools: &Pools,
    locs: &mut [Vec<Option<Loc>>; 2],
    hints: &std::collections::HashMap<Value, Vec<Value>>,
    affinity_func: Option<&Function>,
) -> Vec<Interval> {
    let mut sorted: Vec<&Interval> = intervals.iter().collect();
    sorted.sort_by_key(|iv| (iv.start(), iv.value, iv.half));

    // Intervals holding a register, with that register: live at the
    // current point (active), or in a hole (inactive).
    let mut active: Vec<(&Interval, Loc)> = Vec::new();
    let mut inactive: Vec<(&Interval, Loc)> = Vec::new();
    let mut spilled = Vec::new();
    for cur in sorted {
        let pos = cur.start();
        // Intervals that have ended give up their register; the others
        // move between active and inactive as they enter or leave holes.
        let mut moved = Vec::new();
        active.retain(|&(iv, reg)| {
            if iv.end() < pos {
                false
            } else if iv.covers(pos) {
                true
            } else {
                moved.push((iv, reg));
                false
            }
        });
        inactive.retain(|&(iv, reg)| {
            if iv.end() < pos {
                false
            } else if iv.covers(pos) {
                active.push((iv, reg));
                false
            } else {
                true
            }
        });
        inactive.extend(moved);

        // Live across a call: only a callee-saved register survives it.
        // Otherwise prefer caller-saved ones, which need no save/restore.
        let (caller_saved, callee_saved, class): (_, _, fn(u8) -> Loc) = if cur.float {
            (
                pools.float_caller_saved,
                pools.float_callee_saved,
                Loc::FReg,
            )
        } else {
            (pools.caller_saved, pools.callee_saved, Loc::Reg)
        };
        let allowed: Vec<Loc> = if cur.crosses_call && cur.vector {
            Vec::new()
        } else if cur.crosses_call {
            callee_saved.iter().map(|&r| class(r)).collect()
        } else {
            caller_saved
                .iter()
                .chain(callee_saved)
                .map(|&r| class(r))
                .collect()
        };
        // An inactive interval's register is only taken where `cur`
        // would need it during one of its ranges.
        let blocked_by_inactive = |reg: Loc| {
            inactive
                .iter()
                .any(|&(iv, r)| r == reg && iv.intersects(cur))
        };
        let available = |r: Loc| {
            allowed.contains(&r)
                && active.iter().all(|&(_, used)| used != r)
                && !blocked_by_inactive(r)
        };
        let preferred = hints
            .get(&cur.value)
            .into_iter()
            .flatten()
            .filter_map(|v| locs[cur.half][v.index()])
            .find(|&r| available(r));
        // Deliberately gather edges independently here rather than sharing the
        // production precomputed table or consensus helper.
        let consensus = || {
            let func = affinity_func?;
            let ValueDef::Param(block, index) = func.value_def(cur.value) else {
                return None;
            };
            if Some(block) == func.entry_block()
                || cur.half != 0
                || !matches!(
                    func.value_type(cur.value),
                    Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
                )
            {
                return None;
            }
            let cfg = ControlFlowGraph::compute(func);
            let mut candidate = None;
            for pred in cfg.reverse_postorder(func) {
                let Some(&term) = func.block_insts(pred).last() else {
                    continue;
                };
                let data = func.inst_data(term);
                for dest in data.branch_dests().iter().filter(|d| d.block == block) {
                    if matches!(data, InstData::Invoke { .. }) {
                        return None;
                    }
                    let arg = func.resolve(dest.args[index as usize]);
                    let loc = locs[0][arg.index()]?;
                    if !matches!(loc, Loc::Reg(_)) {
                        return None;
                    }
                    if candidate.is_some_and(|old| old != loc) {
                        return None;
                    }
                    candidate = Some(loc);
                }
            }
            candidate.filter(|&r| {
                available(r)
                    && match r {
                        Loc::Reg(n) => {
                            pools.caller_saved.contains(&n)
                                || locs[0].iter().any(|&loc| loc == Some(r))
                        }
                        _ => false,
                    }
            })
        };
        let free = preferred
            .or_else(consensus)
            .or_else(|| allowed.iter().copied().find(|&r| available(r)));
        if let Some(reg) = free {
            locs[cur.half][cur.value.index()] = Some(reg);
            active.push((cur, reg));
            continue;
        }

        // Out of registers. Of the active intervals whose register `cur`
        // could use (once they are evicted), find the one that ends last.
        // If it outlives `cur`, take its register and spill it; otherwise
        // spill `cur`.
        let victim = active
            .iter()
            .enumerate()
            .filter(|(_, (_, reg))| allowed.contains(reg) && !blocked_by_inactive(*reg))
            .max_by_key(|(_, (iv, _))| iv.end())
            .map(|(i, _)| i);
        match victim {
            Some(i) if active[i].0.end() > cur.end() => {
                let (evicted, reg) = active.swap_remove(i);
                locs[evicted.half][evicted.value.index()] = None;
                spilled.push(evicted.clone());
                locs[cur.half][cur.value.index()] = Some(reg);
                active.push((cur, reg));
            }
            _ => spilled.push(cur.clone()),
        }
    }
    spilled
}

fn reference_assign_spill_slots(spilled: &mut [Interval], locs: &mut [Vec<Option<Loc>>; 2]) -> u32 {
    spilled.sort_by_key(|iv| (iv.start(), iv.value, iv.half));
    let mut count = 0;
    for vector in [false, true] {
        // Separate fixed-width pools preserve scalar/vector home sizes.
        let mut active: Vec<(u32, u32)> = Vec::new();
        let base = count;
        let units = if vector { 2 } else { 1 };
        for iv in spilled.iter().filter(|iv| iv.vector == vector) {
            active.retain(|&(end, _)| end >= iv.start());
            let slot = (base..)
                .step_by(units as usize)
                .find(|&s| active.iter().all(|&(_, used)| used != s))
                .expect("some slot is free");
            active.push((iv.end(), slot));
            count = count.max(slot + units);
            locs[iv.half][iv.value.index()] = Some(Loc::Stack(slot));
        }
    }
    count
}

fn assert_reference_equivalence(
    func: &Function,
    selection: &crate::select::Selection,
    twice: bool,
) {
    let cfg = ControlFlowGraph::compute(func);
    let order = cfg.reverse_postorder(func);
    assert_eq!(order, DominatorTree::compute(func, &cfg).rpo());
    let actual_live = liveness(func, &cfg, &order, selection);
    let reference_live = reference_liveness(func, &cfg, &order, selection);
    for b in 0..func.num_blocks() {
        assert_eq!(
            actual_live.live_in[b]
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            reference_live.live_in[b],
            "{} block {b} live-in",
            func.name
        );
        assert_eq!(
            actual_live.live_out[b]
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            reference_live.live_out[b],
            "{} block {b} live-out",
            func.name
        );
    }
    let is_call = |d: &InstData| {
        matches!(
            d,
            InstData::Call { .. }
                | InstData::CallIndirect { .. }
                | InstData::Invoke { .. }
                | InstData::Intrinsic { .. }
                | InstData::Fma { .. }
        )
    };
    let returns_twice = |d: &InstData| twice && is_call(d);
    let full = build_intervals(func, &cfg, &order, &is_call, &returns_twice, selection);
    let fast = build_fast_intervals(func, &cfg, &order, &is_call, &returns_twice, selection);
    let mut collapsed = full.clone();
    for iv in &mut collapsed {
        iv.ranges = vec![(iv.start(), iv.end())].into();
    }
    assert_eq!(fast.len(), collapsed.len());
    for (a, b) in fast.iter().zip(&collapsed) {
        assert_eq!(
            *a,
            FastInterval {
                value: b.value,
                half: b.half,
                start: b.start(),
                end: b.end(),
                crosses_call: b.crosses_call,
                preserved_gprs: b.preserved_gprs,
                float: b.float,
                vector: b.vector,
            },
            "{}",
            func.name
        );
    }
    for pools in [
        crate::aarch64::REGISTERS,
        crate::x86_64::REGISTERS,
        Pools {
            caller_saved: &[9],
            callee_saved: &[19],
            float_caller_saved: &[9],
            float_callee_saved: &[19],
        },
        Pools {
            caller_saved: &[],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        },
    ] {
        for tier in [
            AllocationTier::Fast,
            AllocationTier::Full,
            #[cfg(feature = "experimental-regalloc")]
            AllocationTier::FullAffinity,
        ] {
            let intervals = if tier == AllocationTier::Fast {
                &collapsed
            } else {
                &full
            };
            let empty_hints = Default::default();
            let hints = if tier != AllocationTier::Fast {
                &selection.hints
            } else {
                &empty_hints
            };
            let mut expected = [vec![None; func.num_values()], vec![None; func.num_values()]];
            let mut reference_spills = reference_linear_scan(
                intervals,
                &pools,
                &mut expected,
                hints,
                (tier.name() == "full-affinity"
                    && !func.sig.returns_twice
                    && !order.iter().any(|&b| {
                        func.block_insts(b)
                            .iter()
                            .any(|&i| returns_twice(func.inst_data(i)))
                    }))
                .then_some(func),
            );
            let expected_slots = reference_assign_spill_slots(&mut reference_spills, &mut expected);
            let actual = allocate_tier(func, &pools, &is_call, &returns_twice, selection, tier);
            assert_eq!(actual.order, order);
            assert_eq!(
                actual.locs, expected,
                "{} {tier:?}, returns-twice={twice}",
                func.name
            );
            assert_eq!(actual.spill_slots, expected_slots);
        }
    }
}

#[test]
fn corpus_matches_original_liveness_and_allocation() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "keel") {
            continue;
        }
        let module = keel_ir::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for func in &module.functions {
            for target in [crate::select::Target::Aarch64, crate::select::Target::X86] {
                let selection = crate::select::Selection::compute(func, target);
                for twice in [false, true] {
                    assert_reference_equivalence(func, &selection, twice);
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn call_in_a_hole_does_not_pin_or_restrict_fast_interval() {
    let func = function(
        "\
        decl @g() -> i64 returns_twice
        func @f(i64, i8) -> i64 {
        block0(v0: i64, v1: i8):
            v2 = iadd v0, v0
            brif v1, block2, block1
        block1:
            v3 = call @g()
            return v3
        block2:
            jump block3
        block3:
            return v2
        }",
    );
    let cfg = ControlFlowGraph::compute(&func);
    let order = cfg.reverse_postorder(&func);
    let call = |d: &InstData| matches!(d, InstData::Call { .. });
    let selection = crate::select::Selection::default();
    let ivs = build_fast_intervals(&func, &cfg, &order, &call, &call, &selection);
    let iv = ivs.iter().find(|iv| iv.value == Value::new(2)).unwrap();
    assert!(!iv.crosses_call);
    assert!(
        iv.start > 0,
        "a returns-twice call in a hole must not pin the value"
    );
    assert_reference_equivalence(&func, &selection, true);
}

#[test]
fn returns_twice_pins_both_i128_halves_and_spill_slots() {
    let func = function(
        "\
        decl @g() -> i64 returns_twice
        func @f(i128) -> i128 {
        block0(v0: i128):
            v1 = call @g()
            v2 = iadd v0, v0
            return v2
        }",
    );
    let selection = crate::select::Selection::default();
    assert_reference_equivalence(&func, &selection, true);
    let cfg = ControlFlowGraph::compute(&func);
    let order = cfg.reverse_postorder(&func);
    let call = |d: &InstData| matches!(d, InstData::Call { .. });
    let ivs = build_fast_intervals(&func, &cfg, &order, &call, &call, &selection);
    let pinned: Vec<_> = ivs.iter().filter(|iv| iv.value == Value::new(0)).collect();
    assert_eq!(pinned.len(), 2);
    for iv in &pinned {
        assert_eq!(iv.start, 0);
        assert!(iv.crosses_call);
    }
    let pools = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let alloc = allocate_tier(
        &func,
        &pools,
        &call,
        &call,
        &selection,
        AllocationTier::Fast,
    );
    let low = alloc.loc(Value::new(0)).unwrap();
    let high = alloc.hi_loc(Value::new(0)).unwrap();
    assert_ne!(low, high);
    for iv in ivs.iter().filter(|iv| iv.value != Value::new(0)) {
        assert_ne!(alloc.locs[iv.half][iv.value.index()], Some(low));
        assert_ne!(alloc.locs[iv.half][iv.value.index()], Some(high));
    }
}

#[test]
fn sparse_liveness_handles_irreducible_cycles_and_unreachable_blocks() {
    let func = function(
        "\
        func @f(i64, i8) -> i64 {
        block0(v0: i64, v1: i8):
            brif v1, block1, block2
        block1:
            v2 = iadd v0, v0
            brif v1, block2, block3(v2)
        block2:
            v3 = isub v0, v0
            brif v1, block1, block3(v3)
        block3(v4: i64):
            return v4
        block4:
            v5 = iconst.i64 99
            jump block1
        }",
    );
    assert_reference_equivalence(&func, &crate::select::Selection::default(), false);
}

fn function(src: &str) -> Function {
    keel_ir::parse(src)
        .expect("test IR parses")
        .functions
        .into_iter()
        .next()
        .expect("one function")
}
