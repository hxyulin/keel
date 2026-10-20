//! Source-authored allocator/token tests. No compiler or application is invoked.
use super::*;
use crate::select::Selection;

const POOLS: Pools<'static> = Pools {
    caller_saved: &[6, 7, 8, 9],
    callee_saved: &[3, 12, 13],
    float_caller_saved: &[0, 1],
    float_callee_saved: &[],
};
// Deliberately nonsequential textual names. Tests obtain actual Value/Inst keys
// from the Function, never from the printed number in a source identifier.
const SOURCE: &str = "decl @touch(ptr, ptr byval {i64, i64}) -> i64
func @forced(ptr, ptr, i64, i64) -> i64 {
block70(v800: ptr, v801: ptr, v900: i64, v901: i64):
    v950 = iadd v900, v901
    jump block12(v800, v801, v950)
block12(v91: ptr, v7: ptr, v81: i64):
    v99 = iconst.i64 -32
    v400 = ptradd v91, v99
    v500 = call @touch(v7, v400)
    v2 = iconst.i64 -16
    v6 = ptradd v91, v2
    v9 = load.i64 v6
    store v9, v400
    v10 = iadd v500, v9
    v11 = iadd v10, v81
    return v11
}";

#[derive(Clone, Copy)]
struct Case {
    entry: Block,
    block: Block,
    base: Value,
    first: Inst,
    call: Inst,
    second: Inst,
}
fn case(func: &Function) -> Case {
    let entry = func.entry_block().unwrap();
    let jump = *func.block_insts(entry).last().unwrap();
    let block = func.inst_data(jump).branch_dests()[0].block;
    let insts = func.block_insts(block);
    Case {
        entry,
        block,
        base: func.block_params(block)[0],
        first: insts[1],
        call: insts[2],
        second: insts[4],
    }
}
fn is_call(data: &InstData) -> bool {
    matches!(
        data,
        InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Invoke { .. }
    )
}
fn never_twice(_: &InstData) -> bool {
    false
}
fn test_input<'a>(func: &'a Function, selection: &'a Selection) -> AllocationInput<'a> {
    AllocationInput {
        func,
        pools: &POOLS,
        selection,
        is_call: &is_call,
        returns_twice: &never_twice,
    }
}

// A real selected Function and its computed intervals, with explicit baseline
// homes. Earlier live inputs account for the already-saved, now-free callees.
fn forced_baseline(input: &AllocationInput<'_>) -> (Allocation, Vec<Interval>) {
    let f = input.func;
    let c = case(f);
    let cfg = ControlFlowGraph::compute(f);
    let order = cfg.reverse_postorder(f);
    let intervals = build_intervals(
        f,
        &cfg,
        &order,
        input.is_call,
        input.returns_twice,
        input.selection,
    );
    let mut locs = [vec![None; f.num_values()], vec![None; f.num_values()]];
    let mut slots = 3;
    for iv in &intervals {
        locs[iv.half][iv.value.index()] = Some(Loc::Stack(slots));
        slots += if iv.vector { 2 } else { 1 };
    }
    let entry = f.block_params(c.entry);
    let params = f.block_params(c.block);
    for (value, loc) in [
        (entry[0], Loc::Stack(0)),
        (entry[1], Loc::Stack(1)),
        (entry[2], Loc::Reg(12)),
        (entry[3], Loc::Reg(13)),
        (params[0], Loc::Stack(0)),
        (params[1], Loc::Stack(1)),
        (params[2], Loc::Stack(2)),
    ] {
        locs[0][value.index()] = Some(loc);
    }
    let body = f.block_insts(c.block);
    for (inst, reg) in [
        (f.block_insts(c.entry)[0], 6),
        (c.first, 3),
        (c.call, 8),
        (c.second, 6),
        (body[5], 7),
        (body[7], 8),
        (body[8], 8),
    ] {
        let value = f.inst_results(inst)[0];
        if f.resolve(value) == value {
            locs[0][value.index()] = Some(Loc::Reg(reg));
        }
    }
    (
        baseline_allocation(order, locs, slots, input.pools),
        intervals,
    )
}
fn points(f: &Function, order: &[Block]) -> std::collections::HashMap<Inst, u32> {
    let mut result = std::collections::HashMap::new();
    let mut slot = 0;
    for &block in order {
        slot += 1;
        for &inst in f.block_insts(block) {
            result.insert(inst, 2 * slot);
            slot += 1;
        }
    }
    result
}
fn forced_cache<'a>(input: &'a AllocationInput<'a>) -> (Cache<'a>, Vec<(u32, bool, Loc)>) {
    let (alloc, intervals) = forced_baseline(input);
    cache_from(input, alloc, intervals)
}
fn cache_from<'a>(
    input: &'a AllocationInput<'a>,
    alloc: Allocation,
    intervals: Vec<Interval>,
) -> (Cache<'a>, Vec<(u32, bool, Loc)>) {
    let mut ends = vec![0; 2 * input.func.num_values()];
    let mut events = Vec::new();
    for iv in intervals {
        let part = ValuePart {
            value: iv.value,
            high: iv.half == 1,
        };
        match alloc.locs[iv.half][iv.value.index()] {
            Some(Loc::Stack(_)) => {
                let eligible = match input.func.value_def(iv.value) {
                    ValueDef::Result(inst, _) => !matches!(
                        input.func.inst_data(inst),
                        InstData::Iconst { .. } | InstData::StackAddr { .. }
                    ),
                    _ => true,
                };
                if eligible {
                    ends[key(part)] = iv.end();
                }
            }
            Some(loc) => {
                for (from, to) in iv.ranges {
                    events.extend([(from, true, loc), (to + 1, false, loc)]);
                }
            }
            None => {}
        }
    }
    events.sort_unstable_by_key(|&(p, enter, _)| (p, enter));
    let registers = input
        .pools
        .caller_saved
        .iter()
        .chain(input.pools.callee_saved)
        .map(|&r| Register {
            loc: Loc::Reg(r),
            part: None,
            dirty: false,
            pinned: false,
        })
        .collect();
    (
        Cache {
            input,
            alloc,
            registers,
            resident: vec![u32::MAX; 2 * input.func.num_values()],
            ends,
            active: 0,
        },
        events,
    )
}
fn prepare(preference: &mut call::Preference, cache: &Cache<'_>) -> (Case, u32) {
    let c = case(cache.input.func);
    let p = points(cache.input.func, &cache.alloc.order);
    let first_slot = p[&cache.input.func.block_insts(c.block)[0]] / 2;
    preference.prepare(cache.input, c.block, first_slot, &cache.ends);
    (c, p[&c.first])
}
fn unavailable(cache: &Cache<'_>, events: &[(u32, bool, Loc)], read: u32) -> [u64; 8] {
    let mut bits = [0; 8];
    for &(point, enter, loc) in events {
        if point > read {
            break;
        }
        bitset(&mut bits, loc, enter);
    }
    // Match the production union of read/write occupancy: a new global result
    // at read+1 blocks borrowing even when that register was free at read.
    for &(point, enter, loc) in events {
        if point == read + 1 && enter {
            bitset(&mut bits, loc, true);
        }
    }
    assert!(cache.input.permits(Loc::Reg(12)));
    bits
}
fn use_loc(a: &Allocation, inst: Inst, value: Value) -> Loc {
    a.operand_loc(
        a.edits.instruction_index(inst),
        ValuePart::low(value),
        OperandKind::Use,
    )
    .unwrap()
}
fn before(a: &Allocation, inst: Inst) -> &[Transfer] {
    a.edits
        .instruction_index(inst)
        .map_or(&[], |i| a.edits.instruction(i).before.as_slice())
}
fn transfers(a: &Allocation, f: &Function) -> Vec<Transfer> {
    f.layout_insts()
        .flat_map(|(_, i)| before(a, i).iter().copied())
        .collect()
}
fn assert_global_nonoverlap(a: &Allocation, intervals: &[Interval]) {
    for (i, left) in intervals.iter().enumerate() {
        for right in &intervals[i + 1..] {
            let loc = a.locs[left.half][left.value.index()];
            if matches!(loc, Some(Loc::Reg(_) | Loc::FReg(_)))
                && loc == a.locs[right.half][right.value.index()]
            {
                assert!(!left.intersects(right));
            }
        }
    }
}

#[test]
fn forced_home_retains_one_value_across_call_without_a_store_save_or_edge_change() {
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = &module.functions[0];
    let selection = Selection::default();
    let input = test_input(f, &selection);
    let c = case(f);
    let (base, intervals) = forced_baseline(&input);
    let mut old = cache_spills(&input, base, &intervals, false);
    let (base, _) = forced_baseline(&input);
    let mut new = cache_spills(&input, base, &intervals, true);
    assert_global_nonoverlap(&new, &intervals);
    old.finish(&input);
    new.finish(&input);
    old.validate(&input).unwrap();
    new.validate(&input).unwrap();
    super::super::transfer_tests::check(&input, &old, 0);
    super::super::transfer_tests::check(&input, &new, 0);
    assert_eq!(use_loc(&old, c.first, c.base), Loc::Reg(6));
    assert_eq!(use_loc(&old, c.second, c.base), Loc::Reg(7));
    assert_eq!(use_loc(&new, c.first, c.base), Loc::Reg(12));
    assert_eq!(use_loc(&new, c.second, c.base), Loc::Reg(12));
    let context = f.block_params(c.block)[1];
    assert_eq!(use_loc(&old, c.call, context), Loc::Reg(12));
    assert_eq!(use_loc(&new, c.call, context), Loc::Reg(13));
    let old_moves = transfers(&old, f);
    let new_moves = transfers(&new, f);
    let reads = |moves: &[Transfer]| moves.iter().filter(|&&(_, s)| s == Loc::Stack(0)).count();
    assert_eq!(reads(&old_moves), 2);
    assert_eq!(reads(&new_moves), 1);
    assert_eq!(new_moves.len() + 1, old_moves.len());
    let stores = |moves: Vec<Transfer>| {
        moves
            .into_iter()
            .filter(|(d, _)| matches!(d, Loc::Stack(_)))
            .collect::<Vec<_>>()
    };
    assert_eq!(stores(old_moves), stores(new_moves));
    assert_eq!(new.locs, old.locs);
    assert_eq!(new.spill_slots, old.spill_slots);
    assert_eq!(new.callee_saved_used, old.callee_saved_used);
    for (_, i) in f.layout_insts() {
        for k in 0..f.inst_data(i).branch_dests().len() {
            assert_eq!(new.edits.edge(i, k), old.edits.edge(i, k));
        }
    }
}

#[test]
fn different_value_later_fixed_context_keeps_all_old_cache_choices() {
    // R2-F1: base is Any, but preferring it can displace context at the call.
    // The later Fixed context use lies outside base's first/next-use window.
    let source = SOURCE.replace("v9 = load.i64 v6", "v9 = load.i64 v7");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = &module.functions[0];
    let c = case(f);
    let context = f.block_params(c.block)[1];
    let load = f.block_insts(c.block)[5];
    let result = f.inst_results(load)[0];
    assert_ne!(context, c.base);
    let mut selection = Selection::default();
    selection.set_constraint(
        load,
        ValuePart::low(context),
        OperandKind::Use,
        Constraint::Fixed(Loc::Reg(12)),
    );
    let input = test_input(f, &selection);
    for &inst in f.block_insts(c.block) {
        input.for_each_operand(inst, |op| {
            if op.part.value == c.base {
                assert_eq!(op.constraint, Constraint::Any);
            }
        });
    }
    let (a, ivs) = forced_baseline(&input);
    let p = points(f, &a.order);
    assert_eq!(
        [p[&c.first], p[&c.call], p[&c.second], p[&load]],
        [10, 12, 16, 18]
    );
    assert_eq!(a.locs[0][result.index()], Some(Loc::Reg(7)));
    let mut old = cache_spills(&input, a, &ivs, false);
    let (a, _) = forced_baseline(&input);
    let mut new = cache_spills(&input, a, &ivs, true);
    old.finish(&input);
    new.finish(&input);
    old.validate(&input).unwrap();
    new.validate(&input).unwrap();
    assert_global_nonoverlap(&new, &ivs);
    for a in [&old, &new] {
        super::super::transfer_tests::check(&input, a, 0);
        assert_eq!(
            [
                use_loc(a, c.first, c.base),
                use_loc(a, c.call, context),
                use_loc(a, c.second, c.base),
                use_loc(a, load, context),
            ],
            [Loc::Reg(6), Loc::Reg(12), Loc::Reg(7), Loc::Reg(12)]
        );
        assert_eq!(
            a.operand_loc(
                a.edits.instruction_index(load),
                ValuePart::low(result),
                OperandKind::Def
            ),
            Some(Loc::Reg(7))
        );
        assert!(
            !before(a, load)
                .iter()
                .any(|&(_, source)| source == Loc::Stack(1))
        );
    }
    assert_eq!(transfers(&old, f), transfers(&new, f));
    assert_eq!(old.locs, new.locs);
    assert_eq!(old.spill_slots, new.spill_slots);
    assert_eq!(old.callee_saved_used, new.callee_saved_used);
    for (_, inst) in f.layout_insts() {
        let old_index = old.edits.instruction_index(inst);
        let new_index = new.edits.instruction_index(inst);
        assert_eq!(old_index.is_some(), new_index.is_some());
        if let (Some(a), Some(b)) = (old_index, new_index) {
            let a = old.edits.instruction(a);
            let b = new.edits.instruction(b);
            assert_eq!(a.uses, b.uses);
            assert_eq!(a.defs, b.defs);
            assert_eq!(a.before, b.before);
            assert_eq!(a.after, b.after);
        }
        for edge in 0..f.inst_data(inst).branch_dests().len() {
            assert_eq!(old.edits.edge(inst, edge), new.edits.edge(inst, edge));
        }
    }
}

#[test]
fn whole_block_constraint_gate_resets_windows_and_keeps_any_block_opportunity() {
    let source = SOURCE.replace("v9 = load.i64 v6", "v9 = load.i64 v7");
    let module = keel_ir::parse(&source).unwrap();
    let f = &module.functions[0];
    let c = case(f);
    let load = f.block_insts(c.block)[5];
    let context = f.block_params(c.block)[1];
    let any = Selection::default();
    let input = test_input(f, &any);
    let (mut cache, events) = forced_cache(&input);
    let mut pref = call::Preference::new(&cache, &events, true);
    let (_, read) = prepare(&mut pref, &cache);
    assert!(pref.metadata_capacities()[1] > 0);
    let p = points(f, &cache.alloc.order);
    let first_slot = p[&f.block_insts(c.block)[0]] / 2;
    let mut fixed = Selection::default();
    fixed.set_constraint(
        load,
        ValuePart::low(context),
        OperandKind::Use,
        Constraint::Fixed(Loc::Reg(12)),
    );
    let constrained = test_input(f, &fixed);
    pref.prepare(&constrained, c.block, first_slot, &cache.ends);
    // Fresh reset precedes the early return; no previous block windows survive.
    assert_eq!(pref.metadata_capacities(), [0, 0]);
    let bits = unavailable(&cache, &events, read);
    assert!(
        pref.acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
            .is_none()
    );
    pref.prepare(&input, c.block, first_slot, &cache.ends);
    let i = pref
        .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
        .unwrap();
    assert_eq!(cache.registers[i].loc, Loc::Reg(12));
}

// R1 F1: the third read must keep the old pre-write source forwarding at
// READ18, where the load's global result claims r7 at WRITE19. There is no
// constraint in the first-to-second-use window. Reuse constrains only the Def;
// its referenced base input is otherwise Any, so it must be excluded explicitly.
fn later_constraint_keeps_old_bindings(reuse: bool) {
    let source = SOURCE.replace("v9 = load.i64 v6", "v9 = load.i64 v91");
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = &module.functions[0];
    let c = case(f);
    let load = f.block_insts(c.block)[5];
    let result = f.inst_results(load)[0];
    let mut selection = Selection::default();
    if reuse {
        selection.set_constraint(
            load,
            ValuePart::low(result),
            OperandKind::Def,
            Constraint::Reuse(ValuePart::low(c.base)),
        );
        assert_eq!(
            selection.constraint(load, ValuePart::low(c.base), OperandKind::Use),
            Constraint::Any
        );
    } else {
        selection.set_constraint(
            load,
            ValuePart::low(c.base),
            OperandKind::Use,
            Constraint::Fixed(Loc::Reg(7)),
        );
    }
    let input = test_input(f, &selection);
    let (a, ivs) = forced_baseline(&input);
    let p = points(f, &a.order);
    assert_eq!(p[&load], 18);
    assert_eq!(a.locs[0][result.index()], Some(Loc::Reg(7)));
    let mut old = cache_spills(&input, a, &ivs, false);
    let (a, _) = forced_baseline(&input);
    let mut new = cache_spills(&input, a, &ivs, true);
    old.finish(&input);
    new.finish(&input);
    old.validate(&input).unwrap();
    new.validate(&input).unwrap();
    assert_global_nonoverlap(&new, &ivs);
    super::super::transfer_tests::check(&input, &old, 0);
    super::super::transfer_tests::check(&input, &new, 0);
    let context = f.block_params(c.block)[1];
    for a in [&old, &new] {
        assert_eq!(
            [
                use_loc(a, c.first, c.base),
                use_loc(a, c.call, context),
                use_loc(a, c.second, c.base),
                use_loc(a, load, c.base),
            ],
            [Loc::Reg(6), Loc::Reg(12), Loc::Reg(7), Loc::Reg(7)]
        );
        assert_eq!(
            a.operand_loc(
                a.edits.instruction_index(load),
                ValuePart::low(result),
                OperandKind::Def
            ),
            Some(Loc::Reg(7))
        );
        // The final base read is forwarded from pre-write r7; it is not a new
        // home reload into a register that is already write-unavailable.
        assert!(
            !before(a, load)
                .iter()
                .any(|&(_, source)| source == Loc::Stack(0))
        );
    }
    assert_eq!(transfers(&old, f), transfers(&new, f));
    assert_eq!(old.locs, new.locs);
    assert_eq!(old.spill_slots, new.spill_slots);
    assert_eq!(old.callee_saved_used, new.callee_saved_used);
}

#[test]
fn later_third_fixed_use_preserves_old_validation_and_prewrite_source() {
    later_constraint_keeps_old_bindings(false);
}

#[test]
fn later_reuse_def_excludes_its_otherwise_any_input_for_the_whole_block() {
    later_constraint_keeps_old_bindings(true);
}

#[test]
fn wide_metadata_is_dropped_once_before_many_small_nonempty_blocks() {
    // Structural capacity/count regression, not a timing benchmark. Grow BOTH
    // maps in a real wide selected block, then prepare many one-input blocks.
    // Retained-capacity clear would keep the wide tables on every small block.
    for (width, small_blocks) in [(64, 128), (128, 256)] {
        let types = std::iter::repeat_n("ptr", width)
            .chain(["i64", "i64"])
            .collect::<Vec<_>>()
            .join(", ");
        let params = (0..width)
            .map(|i| format!("v{}: ptr", 1000 + i))
            .chain(["v900001: i64".into(), "v900002: i64".into()])
            .collect::<Vec<_>>()
            .join(", ");
        let mut source = format!(
            "decl @touch()\nfunc @metadata({types}) -> i64 {{\nblock0({params}):\n v900000 = iadd v900001, v900002\n"
        );
        for i in 0..width {
            source += &format!(" v{} = load.i64 v{}\n", 2000 + i, 1000 + i);
        }
        source += " call @touch()\n";
        for i in 0..width {
            source += &format!(" v{} = load.i64 v{}\n", 3000 + i, 1000 + i);
        }
        source += " jump block1\n";
        for b in 1..=small_blocks {
            source += &format!("block{b}:\n v{} = load.i64 v1000\n", 4000 + b);
            if b == small_blocks {
                source += &format!(" return v{}\n", 4000 + b);
            } else {
                source += &format!(" jump block{}\n", b + 1);
            }
        }
        source += "}";
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let f = &module.functions[0];
        let selection = Selection::default();
        let input = test_input(f, &selection);
        let cfg = ControlFlowGraph::compute(f);
        let order = cfg.reverse_postorder(f);
        let ivs = build_intervals(
            f,
            &cfg,
            &order,
            input.is_call,
            input.returns_twice,
            &selection,
        );
        let mut locs = [vec![None; f.num_values()], vec![None; f.num_values()]];
        let mut slots = 0;
        for iv in &ivs {
            locs[iv.half][iv.value.index()] = Some(Loc::Stack(slots));
            slots += 1;
        }
        let entry = f.entry_block().unwrap();
        let args = f.block_params(entry);
        locs[0][args[width].index()] = Some(Loc::Reg(12));
        locs[0][args[width + 1].index()] = Some(Loc::Reg(13));
        let alloc = baseline_allocation(order, locs, slots, &POOLS);
        let (cache, events) = cache_from(&input, alloc, ivs);
        let p = points(f, &cache.alloc.order);
        let mut pref = call::Preference::new(&cache, &events, true);
        let mut wide_capacity = 0;
        let mut discarded_capacity = 0;
        let mut small_seen = 0;
        for &block in &cache.alloc.order {
            discarded_capacity += pref.metadata_capacities().iter().sum::<usize>();
            let first = f.block_insts(block)[0];
            pref.prepare(&input, block, p[&first] / 2, &cache.ends);
            let capacities = pref.metadata_capacities();
            if block == entry {
                assert!(capacities[0] >= width && capacities[1] >= width);
                wide_capacity = capacities.iter().sum();
            } else {
                small_seen += 1;
                assert!(capacities[0] > 0 && capacities[0] <= 8);
                assert_eq!(capacities[1], 0);
            }
        }
        // Include the final tables' eventual drop, not only block transitions.
        discarded_capacity += pref.metadata_capacities().iter().sum::<usize>();
        assert_eq!(small_seen, small_blocks);
        assert!(discarded_capacity <= wide_capacity + 8 * small_blocks);
    }
}

#[test]
fn global_owner_entries_and_inactive_hole_return_block_the_inclusive_window() {
    let module = keel_ir::parse(SOURCE).unwrap();
    let selection = Selection::default();
    let input = test_input(&module.functions[0], &selection);
    // read, current write, intermediate read/write, next read, next write.
    for offset in [0, 1, 2, 3, 6, 7] {
        let (mut cache, mut events) = forced_cache(&input);
        let p = points(input.func, &cache.alloc.order);
        let c = case(input.func);
        let read = p[&c.first];
        // Give r12 an earlier owner and a later disjoint range: this is an
        // inactive owner returning from a hole, not just an active-at-read test.
        events.extend([
            (read - 4, true, Loc::Reg(12)),
            (read - 2, false, Loc::Reg(12)),
            (read + offset, true, Loc::Reg(12)),
            (read + offset + 2, false, Loc::Reg(12)),
        ]);
        events.sort_unstable_by_key(|&(p, e, _)| (p, e));
        let mut preference = call::Preference::new(&cache, &events, true);
        let (c, read) = prepare(&mut preference, &cache);
        let unavailable = unavailable(&cache, &events, read);
        let chosen = preference
            .acquire(
                &mut cache,
                ValuePart::low(c.base),
                c.first,
                read,
                &unavailable,
            )
            .unwrap();
        assert_eq!(
            cache.registers[chosen].loc,
            Loc::Reg(13),
            "entry offset {offset}"
        );
    }
    let (mut cache, mut events) = forced_cache(&input);
    let p = points(input.func, &cache.alloc.order);
    let c = case(input.func);
    let read = p[&c.first];
    // The first point AFTER next-use write is permitted, unlike that write itself.
    events.push((p[&c.second] + 2, true, Loc::Reg(12)));
    events.sort_unstable_by_key(|&(p, e, _)| (p, e));
    let mut preference = call::Preference::new(&cache, &events, true);
    prepare(&mut preference, &cache);
    let bits = unavailable(&cache, &events, read);
    let i = preference
        .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
        .unwrap();
    assert_eq!(cache.registers[i].loc, Loc::Reg(12));
}

#[test]
fn preference_never_evicts_occupied_dirty_pinned_or_adds_an_unsaved_register() {
    let module = keel_ir::parse(SOURCE).unwrap();
    let selection = Selection::default();
    let input = test_input(&module.functions[0], &selection);
    for (occupied, dirty, pinned) in [
        (true, false, false),
        (true, true, false),
        (true, false, true),
        (false, false, true),
        (false, true, false),
    ] {
        let (mut cache, events) = forced_cache(&input);
        let mut preference = call::Preference::new(&cache, &events, true);
        let (c, read) = prepare(&mut preference, &cache);
        let r12 = cache
            .registers
            .iter()
            .position(|r| r.loc == Loc::Reg(12))
            .unwrap();
        if occupied {
            cache.occupy(r12, ValuePart::low(input.func.block_params(c.block)[1]));
        }
        cache.registers[r12].dirty = dirty;
        cache.registers[r12].pinned = pinned;
        let bits = unavailable(&cache, &events, read);
        let i = preference
            .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
            .unwrap();
        assert_eq!(cache.registers[i].loc, Loc::Reg(13));
        assert_eq!(cache.registers[r12].dirty, dirty);
        assert_eq!(cache.registers[r12].pinned, pinned);
        assert!(before(&cache.alloc, c.first).is_empty());
    }
    let (mut cache, events) = forced_cache(&input);
    cache.alloc.callee_saved_used = vec![3];
    let mut preference = call::Preference::new(&cache, &events, true);
    let (c, read) = prepare(&mut preference, &cache);
    let bits = unavailable(&cache, &events, read);
    assert!(
        preference
            .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
            .is_none()
    );
    let (mut cache, events) = forced_cache(&input);
    let mut preference = call::Preference::new(&cache, &events, true);
    let (c, read) = prepare(&mut preference, &cache);
    let mut bits = unavailable(&cache, &events, read);
    bitset(&mut bits, Loc::Reg(12), true);
    bitset(&mut bits, Loc::Reg(13), true);
    assert!(
        preference
            .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
            .is_none()
    );
}

#[test]
fn selected_duplicates_covered_reads_and_constraints_define_the_next_use() {
    for variant in 0..7 {
        let module = keel_ir::parse(SOURCE).unwrap();
        let f = &module.functions[0];
        let c = case(f);
        let mut selection = Selection::default();
        let first_result = f.inst_results(c.first)[0];
        match variant {
            0 => selection.set_operands(
                c.first,
                vec![c.base, c.base, f.inst_results(f.block_insts(c.block)[0])[0]],
            ),
            1 => {
                let hidden = f.block_insts(c.block)[3];
                selection.set_operands(hidden, vec![c.base]);
                selection.cover(hidden);
            }
            2 => selection.cover(c.second),
            3 => {
                selection.set_operands(c.second, vec![f.inst_results(f.block_insts(c.block)[3])[0]])
            }
            4 => selection.set_constraint(
                c.first,
                ValuePart::low(c.base),
                OperandKind::Use,
                Constraint::Fixed(Loc::Reg(7)),
            ),
            5 => selection.set_constraint(
                c.first,
                ValuePart::low(first_result),
                OperandKind::Def,
                Constraint::Reuse(ValuePart::low(c.base)),
            ),
            6 => selection.set_constraint(
                c.second,
                ValuePart::low(c.base),
                OperandKind::Use,
                Constraint::Register,
            ),
            _ => unreachable!(),
        }
        let input = test_input(f, &selection);
        let (mut cache, events) = forced_cache(&input);
        let mut preference = call::Preference::new(&cache, &events, true);
        let (_, read) = prepare(&mut preference, &cache);
        let bits = unavailable(&cache, &events, read);
        let selected = preference.acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits);
        assert_eq!(selected.is_some(), variant < 2, "variant {variant}");
        if variant == 0 {
            let (baseline, intervals) = forced_baseline(&input);
            let a = cache_spills(&input, baseline, &intervals, true);
            let uses = &a
                .edits
                .instruction(a.edits.instruction_index(c.first).unwrap())
                .uses;
            assert_eq!(
                uses.iter()
                    .filter(|(p, _)| *p == ValuePart::low(c.base))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn zero_two_hidden_and_unknown_calls_do_not_create_a_window() {
    for variant in 0..5 {
        let mut module = keel_ir::parse(SOURCE).unwrap();
        let f = &mut module.functions[0];
        let c = case(f);
        let mut selection = Selection::default();
        match variant {
            0 => {
                let replacement = f.inst_data(f.block_insts(c.block)[3]).clone();
                *f.inst_data_mut(c.call) = replacement;
            }
            1 => {
                let another = f.inst_data(c.call).clone();
                let at = f.block_insts(c.block)[3];
                *f.inst_data_mut(at) = another;
            }
            2 => selection.cover(c.call),
            3 | 4 => {}
            _ => unreachable!(),
        }
        // Missing call classification and helper-like extra clobbers both fail
        // closed. These are planner contract tests, not valid emission requests.
        let classifier = |data: &InstData| {
            if variant == 3 {
                false
            } else {
                is_call(data) || (variant == 4 && matches!(data, InstData::Iconst { .. }))
            }
        };
        let input = AllocationInput {
            func: f,
            pools: &POOLS,
            selection: &selection,
            is_call: &classifier,
            returns_twice: &never_twice,
        };
        let (mut cache, events) = forced_cache(&input);
        let mut preference = call::Preference::new(&cache, &events, true);
        let (_, read) = prepare(&mut preference, &cache);
        let bits = unavailable(&cache, &events, read);
        assert!(
            preference
                .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
                .is_none(),
            "variant {variant}"
        );
    }
}

#[test]
fn alias_state_is_resolved_without_treating_a_definition_as_a_second_value() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    let f = &mut module.functions[0];
    let c = case(f);
    let alias = f.inst_results(c.first)[0];
    f.replace_all_uses(alias, c.base);
    assert!(matches!(f.value_def(alias),ValueDef::Alias(v) if v==c.base));
    let selection = Selection::default();
    let input = test_input(f, &selection);
    let (mut cache, events) = forced_cache(&input);
    let mut preference = call::Preference::new(&cache, &events, true);
    let (_, read) = prepare(&mut preference, &cache);
    let bits = unavailable(&cache, &events, read);
    // The call now reads the canonical base itself: it is the next selected
    // use, with zero calls strictly between, rather than the later ptradd.
    assert!(
        preference
            .acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
            .is_none()
    );
}

#[test]
fn definitions_reset_next_use_and_block_preparation_discards_old_windows() {
    let module = keel_ir::parse(SOURCE).unwrap();
    let f = &module.functions[0];
    let selection = Selection::default();
    let input = test_input(f, &selection);
    let (mut cache, events) = forced_cache(&input);
    let c = case(f);
    let p = points(f, &cache.alloc.order);
    let read = p[&c.first];
    let mut pref = call::Preference::new(&cache, &events, true);
    prepare(&mut pref, &cache);
    // The original positive window must disappear when the next block is
    // prepared. No cache state is propagated by this metadata helper.
    let entry_first = f.block_insts(c.entry)[0];
    pref.prepare(&input, c.entry, p[&entry_first] / 2, &cache.ends);
    let bits = unavailable(&cache, &events, read);
    assert!(
        pref.acquire(&mut cache, ValuePart::low(c.base), c.first, read, &bits)
            .is_none()
    );

    // An adversarial selected-operand table reads a result at its own defining
    // boundary. This is a metadata contract test, deliberately not verified IR
    // or an emission request. The definition must cut the backward next-use
    // chain instead of pairing that read with the later read across the call.
    let value = f.inst_results(c.first)[0];
    let mut selected = Selection::default();
    selected.set_operands(c.first, vec![value]);
    selected.set_operands(c.call, vec![f.block_params(c.block)[1]]);
    selected.set_operands(c.second, vec![value]);
    let changed = test_input(f, &selected);
    cache.ends[key(ValuePart::low(value))] = p[&c.second] + 1;
    pref.prepare(
        &changed,
        c.block,
        p[&f.block_insts(c.block)[0]] / 2,
        &cache.ends,
    );
    assert!(
        pref.acquire(&mut cache, ValuePart::low(value), c.first, read, &bits)
            .is_none()
    );
}

#[test]
fn excluded_widths_and_high_halves_do_not_change_preference() {
    let module = keel_ir::parse(SOURCE).unwrap();
    let selection = Selection::default();
    let input = test_input(&module.functions[0], &selection);
    let (mut cache, events) = forced_cache(&input);
    let mut preference = call::Preference::new(&cache, &events, true);
    let (c, read) = prepare(&mut preference, &cache);
    let bits = unavailable(&cache, &events, read);
    assert!(
        preference
            .acquire(&mut cache, ValuePart::high(c.base), c.first, read, &bits)
            .is_none()
    );
    for ty in ["i8", "i32", "i128", "f64", "i64"] {
        let op = if ty == "f64" { "fadd" } else { "iadd" };
        let source = format!(
            "decl @touch()\nfunc @width({ty}, i64, i64) -> {ty} {{\nblock90(v777: {ty}, v81: i64, v99: i64):\n v40 = iadd v81, v99\n jump block8(v777)\nblock8(v3: {ty}):\n v1 = {op} v3, v3\n call @touch()\n v2 = {op} v3, v1\n return v2\n}}"
        );
        let m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let f = &m.functions[0];
        let selection = Selection::default();
        let input = test_input(f, &selection);
        let cfg = ControlFlowGraph::compute(f);
        let order = cfg.reverse_postorder(f);
        let ivs = build_intervals(
            f,
            &cfg,
            &order,
            input.is_call,
            input.returns_twice,
            &selection,
        );
        let mut locs = [vec![None; f.num_values()], vec![None; f.num_values()]];
        let mut slots = 0;
        for iv in &ivs {
            locs[iv.half][iv.value.index()] = Some(Loc::Stack(slots));
            slots += 1;
        }
        let entry = f.entry_block().unwrap();
        let params = f.block_params(entry);
        locs[0][params[1].index()] = Some(Loc::Reg(12));
        locs[0][params[2].index()] = Some(Loc::Reg(13));
        let body = f
            .inst_data(*f.block_insts(entry).last().unwrap())
            .branch_dests()[0]
            .block;
        let base = f.block_params(body)[0];
        let first = f.block_insts(body)[0];
        let alloc = baseline_allocation(order, locs, slots, &POOLS);
        let (mut cache, events) = cache_from(&input, alloc, ivs);
        let p = points(f, &cache.alloc.order);
        let read = p[&first];
        let mut pref = call::Preference::new(&cache, &events, true);
        pref.prepare(&input, body, read / 2, &cache.ends);
        let bits = unavailable(&cache, &events, read);
        assert_eq!(
            pref.acquire(&mut cache, ValuePart::low(base), first, read, &bits)
                .is_some(),
            ty == "i64",
            "{ty}"
        );
    }
}

#[test]
fn vector_and_returns_twice_fallbacks_stay_exactly_full() {
    for source in [
        "decl @touch()\nfunc @vector(ptr) {\nblock0(v40: ptr):\n v20 = load.i32x4 v40\n call @touch()\n store v20, v40\n return\n}",
        "decl @sj(ptr) -> i32 returns_twice\nfunc @twice(ptr) -> i32 {\nblock0(v80: ptr):\n v99 = call @sj(v80)\n return v99\n}",
    ] {
        let m = keel_ir::parse(source).unwrap();
        let f = &m.functions[0];
        let selection = Selection::default();
        let twice = |data: &InstData| match data {
            InstData::Call { func: c, .. } => f.callee(*c).sig.returns_twice,
            _ => false,
        };
        let input = AllocationInput {
            func: f,
            pools: &POOLS,
            selection: &selection,
            is_call: &is_call,
            returns_twice: &twice,
        };
        let full = AllocationTier::Full.allocate(&input);
        let split = allocate(&input);
        assert_eq!(full.locs, split.locs);
        assert_eq!(full.spill_slots, split.spill_slots);
        assert_eq!(full.callee_saved_used, split.callee_saved_used);
        assert!(split.edits.is_empty());
    }
}

#[test]
fn dirty_def_call_sources_and_later_global_claim_preserve_tokens() {
    let m = keel_ir::parse(SOURCE).unwrap();
    let f = &m.functions[0];
    let selection = Selection::default();
    let input = test_input(f, &selection);
    let c = case(f);
    for dirty_def in [true, false] {
        let make = || {
            let (mut a, ivs) = forced_baseline(&input);
            if dirty_def {
                // Def still uses the existing acquire/dirty-flush path. The
                // call must read its pre-clobber token while its home is saved.
                a.locs[0][f.inst_results(c.first)[0].index()] = Some(Loc::Stack(a.spill_slots));
                a.spill_slots += 1;
            } else {
                // r12 becomes globally owned just after the proved window.
                // Normal existing eviction must release the clean cache entry.
                let load = f.block_insts(c.block)[5];
                a.locs[0][f.inst_results(load)[0].index()] = Some(Loc::Reg(12));
            }
            (
                baseline_allocation(a.order, a.locs, a.spill_slots, &POOLS),
                ivs,
            )
        };
        let (a, ivs) = make();
        let old = cache_spills(&input, a, &ivs, false);
        let (a, _) = make();
        let new = cache_spills(&input, a, &ivs, true);
        assert_global_nonoverlap(&new, &ivs);
        super::super::transfer_tests::check(&input, &old, 0);
        super::super::transfer_tests::check(&input, &new, 0);
        assert_eq!(old.locs, new.locs);
        assert_eq!(old.spill_slots, new.spill_slots);
        if dirty_def {
            assert!(
                before(&new, c.call)
                    .iter()
                    .any(|(d, _)| matches!(d, Loc::Stack(_)))
            );
        }
    }
}

#[test]
fn phi_cycles_duplicate_edges_and_fused_compare_keep_home_dataflow() {
    let source = SOURCE.replace("    return v11", "    v12 = iconst.i64 0\n    v13 = icmp.eq v11, v12\n    brif v13, block12(v6, v7, v11), block12(v7, v91, v11)");
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    for fuse in [false, true] {
        let mut selection = Selection::default();
        if fuse {
            selection.fuse_compares(f, |_| vec![]);
        }
        let input = test_input(f, &selection);
        let (a, ivs) = forced_baseline(&input);
        let old = cache_spills(&input, a, &ivs, false);
        let (a, _) = forced_baseline(&input);
        let new = cache_spills(&input, a, &ivs, true);
        assert_global_nonoverlap(&new, &ivs);
        for seed in 0..8 {
            super::super::transfer_tests::check(&input, &old, seed);
            super::super::transfer_tests::check(&input, &new, seed);
        }
        assert_eq!(old.locs, new.locs);
        assert_eq!(old.spill_slots, new.spill_slots);
        // Both successor ordinals are retained, even though CFG successor sets
        // deduplicate their target. One path swaps pointer home0/home1 tokens.
        let term = *f.block_insts(case(f).block).last().unwrap();
        assert_eq!(f.inst_data(term).branch_dests().len(), 2);
        for edge in 0..2 {
            assert_eq!(old.edits.edge(term, edge), new.edits.edge(term, edge));
        }
    }
}

#[test]
fn invoke_and_tail_boundaries_never_extend_a_preference_into_another_block() {
    // Source/token checks only: no exception, tail-call or ABI native lowering.
    for source in [
        include_str!("../../tests/programs/exceptions.keel"),
        include_str!("../../tests/programs/tail_calls.keel"),
    ] {
        let m = keel_ir::parse(source).unwrap();
        for f in &m.functions {
            let selection = Selection::default();
            let input = test_input(f, &selection);
            let cfg = ControlFlowGraph::compute(f);
            let order = cfg.reverse_postorder(f);
            let ivs = build_intervals(
                f,
                &cfg,
                &order,
                input.is_call,
                input.returns_twice,
                &selection,
            );
            let full = AllocationTier::Full.allocate(&input);
            let (mut cache, events) = cache_from(&input, full, ivs);
            let p = points(f, &cache.alloc.order);
            let mut pref = call::Preference::new(&cache, &events, true);
            for block in cache.alloc.order.clone() {
                let Some(&first) = f.block_insts(block).first() else {
                    continue;
                };
                pref.prepare(&input, block, p[&first] / 2, &cache.ends);
                for &inst in f.block_insts(block) {
                    if !f.inst_data(inst).is_terminator() {
                        continue;
                    }
                    let read = p[&inst];
                    input.for_each_operand(inst, |op| {
                        if op.kind == OperandKind::Use && !op.part.high {
                            assert!(
                                pref.acquire(&mut cache, op.part, inst, read, &[0; 8])
                                    .is_none()
                            );
                        }
                    });
                }
            }
        }
    }
}
