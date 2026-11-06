//! Storage for declared IR stack objects, separate from allocator spill homes.
//!
//! Slots that lifetime markers name may share frame bytes when their
//! lifetimes never overlap. Liveness comes from the markers alone: a marked
//! slot is dead on entry, live from a start and dead after an end, along
//! every path. Unmarked slots, and every slot of a function that has debug
//! homes or a returns-twice call, keep bytes of their own.
use keel_ir::{Block, EntityRef, Function, InstData};
use std::ops::Index;

/// Indexed by the original SlotId, including holes with no frame storage.
/// Address emission must never turn an omitted home into a valid offset.
pub(crate) struct SlotOffsets(Vec<Option<usize>>);

/// Above this many interfering pairs, marked slots get bytes of their own.
fn edge_budget(marked: usize) -> usize {
    16 * marked + 4096
}

impl SlotOffsets {
    /// Lays out the slots from `offset` and returns the end. `debug` keeps a
    /// home for every declared slot; `separate` turns off byte sharing.
    pub(crate) fn layout(
        func: &Function,
        mut offset: usize,
        debug: bool,
        separate: bool,
    ) -> (Self, usize) {
        let base = offset;
        let slots = func.num_stack_slots();
        // Inspect IR before target folding: folded loads/stores and rematerialized
        // addresses still need the object. Keeping every surviving StackAddr is
        // conservative, including escapes, aliases and unused address results.
        // Debug emission indexes slots independently of executing instructions;
        // retain all declared homes whenever this module emits debug information.
        let mut used = vec![debug; slots];
        // Marked slots that may share bytes: no `AggregateVaArg` home or
        // debug variable, whose storage nothing tracks through markers.
        let mut shareable = vec![false; slots];
        let mut own = vec![false; slots];
        let mut returns_twice = false;
        let mut markers = Vec::new();
        // Program points in layout order: a header point per block, then one
        // per instruction.
        let mut points = vec![(0, 0); func.num_blocks()];
        let mut point = 0;
        for block in func.blocks() {
            let header = point;
            point += 1;
            for &inst in func.block_insts(block) {
                match *func.inst_data(inst) {
                    InstData::StackAddr { slot } => used[slot.index()] = true,
                    InstData::AggregateVaArg { home: slot, .. } => {
                        used[slot.index()] = true;
                        own[slot.index()] = true;
                    }
                    InstData::Call { func: callee, .. } | InstData::Invoke { func: callee, .. } => {
                        returns_twice |= func.callee(callee).sig.returns_twice
                    }
                    InstData::CallIndirect { sig, .. } => {
                        returns_twice |= func.signature(sig).returns_twice
                    }
                    InstData::Intrinsic { id, .. } => {
                        if let Some(slot) = func.lifetime_marker_slot(inst) {
                            shareable[slot.index()] = true;
                            let start = id == keel_ir::Intrinsic::LifetimeStart;
                            markers.push((slot.index(), block, point, start));
                        }
                    }
                    _ => {}
                }
                point += 1;
            }
            points[block.index()] = (header, point - 1);
        }
        for var in &func.debug_vars {
            own[var.slot.index()] = true;
        }
        for (shared, own) in shareable.iter_mut().zip(&own) {
            *shared &= !*own;
        }
        let mut offsets = vec![None; slots];
        let mut unshared = offset;
        for (id, slot) in func.stack_slots() {
            if used[id.index()] {
                unshared = unshared.next_multiple_of(slot.align as usize);
                offsets[id.index()] = Some(unshared);
                unshared += slot.size as usize;
            }
        }
        let share = !debug && !separate && !returns_twice && shareable.iter().any(|&s| s);
        if share {
            offsets.fill(None);
            // Unmarked slots first, in declaration order, as without sharing.
            for (id, slot) in func.stack_slots() {
                if used[id.index()] && !shareable[id.index()] {
                    offset = offset.next_multiple_of(slot.align as usize);
                    offsets[id.index()] = Some(offset);
                    offset += slot.size as usize;
                }
            }
            offset = pack(func, &shareable, &markers, &points, offset, &mut offsets);
        } else {
            offset = unshared;
        }
        use crate::backend_profile::{Counter, count_by};
        count_by(Counter::SlotBytes, (offset - base) as u64);
        count_by(Counter::UnsharedSlotBytes, (unshared - base) as u64);
        (Self(offsets), offset)
    }

    /// The largest alignment of any slot with frame storage, at least 1.
    pub(crate) fn align(&self, func: &Function) -> u32 {
        func.stack_slots()
            .filter(|(id, _)| self.0[id.index()].is_some())
            .map(|(_, slot)| slot.align)
            .max()
            .unwrap_or(1)
    }

    /// Only debug frames have an observable home for every declared slot.
    pub(crate) fn debug_offsets(&self) -> impl Iterator<Item = usize> + '_ {
        self.0
            .iter()
            .map(|offset| offset.expect("missing debug stack home"))
    }
}

impl Index<usize> for SlotOffsets {
    type Output = usize;

    fn index(&self, index: usize) -> &usize {
        self.0[index]
            .as_ref()
            .expect("address of omitted stack slot")
    }
}

/// One stretch of program points, both ends included, where a slot is live.
#[derive(Clone, Copy)]
struct Segment {
    start: usize,
    end: usize,
    slot: usize,
}

/// Places the shareable slots from `offset`, sharing bytes between slots
/// whose segments never overlap, and returns the end.
fn pack(
    func: &Function,
    shareable: &[bool],
    markers: &[(usize, Block, usize, bool)],
    points: &[(usize, usize)],
    offset: usize,
    offsets: &mut [Option<usize>],
) -> usize {
    let segments = segments(func, shareable, markers, points);
    let data: Vec<_> = func.stack_slots().map(|(_, d)| d).collect();
    let count = shareable.iter().filter(|&&s| s).count();
    let neighbours = interference(&segments, shareable.len(), edge_budget(count));
    let mut order: Vec<_> = (0..shareable.len()).filter(|&s| shareable[s]).collect();
    let Some(neighbours) = neighbours else {
        // Too many pairs live at once to be worth sharing.
        let mut end = offset;
        for s in order {
            end = end.next_multiple_of(data[s].align as usize);
            offsets[s] = Some(end);
            end += data[s].size as usize;
        }
        return end;
    };
    order.sort_by_key(|&s| {
        (
            std::cmp::Reverse(data[s].size),
            std::cmp::Reverse(data[s].align),
            s,
        )
    });
    let mut end = offset;
    let mut taken = Vec::new();
    for s in order {
        let (size, align) = (data[s].size as usize, data[s].align as usize);
        taken.clear();
        taken.extend(neighbours[s].iter().filter_map(|&n| {
            offsets[n as usize].map(|at| (at, at + data[n as usize].size as usize))
        }));
        taken.sort_unstable();
        // The lowest aligned offset that overlaps no placed neighbour.
        let mut at = offset.next_multiple_of(align);
        for &(from, to) in &taken {
            if from >= at + size {
                break;
            }
            if to > at {
                at = to.next_multiple_of(align);
            }
        }
        offsets[s] = Some(at);
        end = end.max(at + size);
    }
    if cfg!(debug_assertions) {
        for (s, list) in neighbours.iter().enumerate() {
            for &n in list {
                let (a, b) = (offsets[s].unwrap(), offsets[n as usize].unwrap());
                let (a_end, b_end) = (
                    a + data[s].size as usize,
                    b + data[n as usize].size as usize,
                );
                assert!(
                    a_end <= b || b_end <= a || a == a_end || b == b_end,
                    "slots {s} and {n} are live together but share bytes"
                );
            }
        }
    }
    end
}

/// The segments of every shareable slot. A block whose last marker of a slot
/// is a start has the slot live out; the slot is live into each block that a
/// live-out block reaches before any block with a marker of its own.
fn segments(
    func: &Function,
    shareable: &[bool],
    markers: &[(usize, Block, usize, bool)],
    points: &[(usize, usize)],
) -> Vec<Segment> {
    // Group the markers by slot, keeping layout order within each slot.
    let mut first = vec![0usize; shareable.len() + 1];
    for &(slot, ..) in markers {
        first[slot + 1] += 1;
    }
    for i in 1..first.len() {
        first[i] += first[i - 1];
    }
    let mut fill = first.clone();
    let mut by_slot = vec![(Block::new(0), 0, false); markers.len()];
    for &(slot, block, point, start) in markers {
        by_slot[fill[slot]] = (block, point, start);
        fill[slot] += 1;
    }
    let blocks = func.num_blocks();
    // Stamps hold slot + 1, so one array serves every slot.
    let mut live_in = vec![0usize; blocks];
    let mut marked = vec![0usize; blocks];
    let mut worklist = Vec::new();
    let mut entered = Vec::new();
    let mut out = Vec::new();
    for slot in (0..shareable.len()).filter(|&s| shareable[s]) {
        let stamp = slot + 1;
        let events = &by_slot[first[slot]..first[slot + 1]];
        worklist.clear();
        entered.clear();
        for (i, &(block, _, start)) in events.iter().enumerate() {
            marked[block.index()] = stamp;
            let last = events.get(i + 1).is_none_or(|&(next, ..)| next != block);
            if last && start {
                worklist.push(block);
            }
        }
        while let Some(block) = worklist.pop() {
            let Some(&term) = func.block_insts(block).last() else {
                continue;
            };
            for dest in func.inst_data(term).branch_dests() {
                let next = dest.block;
                if live_in[next.index()] == stamp {
                    continue;
                }
                live_in[next.index()] = stamp;
                entered.push(next);
                if marked[next.index()] != stamp {
                    worklist.push(next);
                }
            }
        }
        // Segments of the blocks with markers, in layout order.
        let mut open = None;
        for (i, &(block, point, start)) in events.iter().enumerate() {
            let (header, last) = points[block.index()];
            if i == 0 || events[i - 1].0 != block {
                open = (live_in[block.index()] == stamp).then_some(header);
            }
            match (open, start) {
                (None, true) => open = Some(point),
                (Some(from), false) => {
                    out.push(Segment {
                        start: from,
                        end: point,
                        slot,
                    });
                    open = None;
                }
                _ => {}
            }
            if events.get(i + 1).is_none_or(|&(next, ..)| next != block)
                && let Some(from) = open
            {
                out.push(Segment {
                    start: from,
                    end: last,
                    slot,
                });
            }
        }
        // Blocks the slot lives through.
        for &block in &entered {
            if marked[block.index()] != stamp {
                let (header, last) = points[block.index()];
                out.push(Segment {
                    start: header,
                    end: last,
                    slot,
                });
            }
        }
    }
    out
}

/// Each slot's neighbours: the slots with a segment that overlaps one of
/// its own. `None` when there are more than `budget` pairs.
fn interference(segments: &[Segment], slots: usize, budget: usize) -> Option<Vec<Vec<u32>>> {
    let mut order: Vec<_> = segments.to_vec();
    order.sort_unstable_by_key(|s| (s.start, s.end, s.slot));
    let mut neighbours = vec![Vec::new(); slots];
    let mut pairs = keel_ir::fxhash::FxHashSet::default();
    let mut active: Vec<Segment> = Vec::new();
    for segment in order {
        active.retain(|a| a.end >= segment.start);
        for a in &active {
            if a.slot != segment.slot {
                let pair = (a.slot.min(segment.slot), a.slot.max(segment.slot));
                if pairs.insert(pair) {
                    if pairs.len() > budget {
                        return None;
                    }
                    neighbours[pair.0].push(pair.1 as u32);
                    neighbours[pair.1].push(pair.0 as u32);
                }
            }
        }
        active.push(segment);
    }
    Some(neighbours)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_slots_keep_identity_alignment_and_debug_homes() {
        let module = keel_ir::parse(
            "func @f() -> ptr {
                ss0 = stack_slot 4096, align 16
                ss1 = stack_slot 3, align 1
                ss2 = stack_slot 65536, align 16
                ss3 = stack_slot 16, align 16
            block0:
                v0 = stack_addr ss1
                v1 = stack_addr ss3
                return v1
            }",
        )
        .unwrap();
        let f = &module.functions[0];
        let ((slots, end), report) =
            crate::backend_profile::capture(|| SlotOffsets::layout(f, 8, false, false));
        assert_eq!(slots.0, [None, Some(8), None, Some(16)]);
        assert_eq!(end, 32);
        use crate::backend_profile::Counter;
        assert_eq!(report.counters[Counter::SlotBytes as usize], 24);
        assert_eq!(report.counters[Counter::UnsharedSlotBytes as usize], 24);
        // Even an address with no uses is retained; address escape analysis is
        // deliberately outside frame layout's responsibility.
        assert_eq!(slots[1], 8);
        assert_eq!(slots[3], 16);
        let (debug, end) = SlotOffsets::layout(f, 8, true, false);
        assert_eq!(
            debug.debug_offsets().collect::<Vec<_>>(),
            [16, 4112, 4128, 69664]
        );
        assert_eq!(end, 69680);
    }

    #[test]
    fn removed_address_alias_keeps_only_its_surviving_home() {
        let mut module = keel_ir::parse(
            "func @f() -> ptr {
                ss0 = stack_slot 65536, align 16
                ss1 = stack_slot 8, align 8
            block0:
                v0 = stack_addr ss0
                v1 = stack_addr ss1
                return v0
            }",
        )
        .unwrap();
        let f = &mut module.functions[0];
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        f.replace_all_uses(f.first_result(insts[0]), f.first_result(insts[1]));
        f.remove_inst(insts[0]);
        let (slots, end) = SlotOffsets::layout(f, 0, false, false);
        assert_eq!(slots.0, [None, Some(0)]);
        assert_eq!(end, 8);
        // Each emitter resolves aliases, including when folding an address.
        for target in crate::Target::NATIVE {
            crate::compile_with_options(&module, target, crate::SelectionOptions::ALL).unwrap();
        }
    }

    /// Offsets and end of `source`'s first function laid out from 0.
    fn shared(source: &str) -> (Vec<Option<usize>>, usize) {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let (slots, end) = SlotOffsets::layout(&module.functions[0], 0, false, false);
        (slots.0, end)
    }

    /// A function with three 16-byte slots, `ss0` to `ss2`, and `body`.
    fn three(body: &str) -> String {
        format!(
            "decl @use(ptr)
func @f(i8) {{
    ss0 = stack_slot 16, align 8
    ss1 = stack_slot 16, align 8
    ss2 = stack_slot 16, align 8
block0(v9: i8):
    v0 = stack_addr ss0
    v1 = stack_addr ss1
    v2 = stack_addr ss2
    {body}
}}"
        )
    }

    #[test]
    fn sequential_lifetimes_share_and_overlapping_ones_do_not() {
        let sequential = three(
            "intrinsic lifetime_start(v0)
    call @use(v0)
    intrinsic lifetime_end(v0)
    intrinsic lifetime_start(v1)
    call @use(v1)
    intrinsic lifetime_end(v1)
    call @use(v2)
    return",
        );
        // ss2 has no markers, so it comes first and never shares.
        assert_eq!(shared(&sequential), (vec![Some(16), Some(16), Some(0)], 32));
        let overlapping = three(
            "intrinsic lifetime_start(v0)
    intrinsic lifetime_start(v1)
    call @use(v0)
    intrinsic lifetime_end(v0)
    call @use(v1)
    intrinsic lifetime_end(v1)
    call @use(v2)
    return",
        );
        assert_eq!(
            shared(&overlapping),
            (vec![Some(16), Some(32), Some(0)], 48)
        );
    }

    #[test]
    fn a_back_edge_that_skips_the_end_keeps_the_slot_live_around_the_loop() {
        // ss0 starts before the loop and ends after it, so it is live on the
        // back edge while ss1 lives inside the loop body.
        let looped = three(
            "intrinsic lifetime_start(v0)
    jump block1
block1:
    intrinsic lifetime_start(v1)
    call @use(v1)
    intrinsic lifetime_end(v1)
    call @use(v0)
    brif v9, block1, block2
block2:
    intrinsic lifetime_end(v0)
    intrinsic lifetime_start(v2)
    call @use(v2)
    intrinsic lifetime_end(v2)
    return",
        );
        let (offsets, end) = shared(&looped);
        assert_ne!(offsets[0], offsets[1]);
        // ss2 starts after ss0 and ss1 have ended, so it reuses bytes.
        assert_eq!(end, 32);
        // A start in the loop with its end after the loop: on the back edge
        // the slot is live into the loop header, where ss1 starts.
        let carried = three(
            "jump block1
block1:
    intrinsic lifetime_start(v1)
    call @use(v1)
    intrinsic lifetime_end(v1)
    intrinsic lifetime_start(v0)
    call @use(v0)
    brif v9, block1, block2
block2:
    intrinsic lifetime_end(v0)
    call @use(v2)
    return",
        );
        let (offsets, _) = shared(&carried);
        assert_ne!(offsets[0], offsets[1], "{offsets:?}");
    }

    #[test]
    fn a_lifetime_on_one_branch_only_ends_where_its_paths_do() {
        let branch = three(
            "brif v9, block1, block2
block1:
    intrinsic lifetime_start(v0)
    call @use(v0)
    jump block3
block2:
    jump block3
block3:
    call @use(v0)
    intrinsic lifetime_end(v0)
    intrinsic lifetime_start(v1)
    call @use(v1)
    intrinsic lifetime_end(v1)
    call @use(v2)
    return",
        );
        let (offsets, _) = shared(&branch);
        assert_eq!(offsets[0], offsets[1]);
        // Without the end, ss0 stays live to the return and overlaps ss1.
        let open = branch.replacen("    intrinsic lifetime_end(v0)\n", "", 1);
        let (offsets, _) = shared(&open);
        assert_ne!(offsets[0], offsets[1]);
    }

    #[test]
    fn returns_twice_debug_homes_and_the_switch_keep_separate_bytes() {
        let body = "intrinsic lifetime_start(v0)
    call @use(v0)
    intrinsic lifetime_end(v0)
    intrinsic lifetime_start(v1)
    call @use(v1)
    intrinsic lifetime_end(v1)
    call @use(v2)
    return";
        let separate = (vec![Some(0), Some(16), Some(32)], 48);
        let twice = three(body).replace("decl @use(ptr)", "decl @use(ptr) returns_twice");
        assert_eq!(shared(&twice), separate);
        let module = keel_ir::parse(&three(body)).unwrap();
        let f = &module.functions[0];
        for (debug, separate_slots) in [(true, false), (false, true)] {
            let (slots, end) = SlotOffsets::layout(f, 0, debug, separate_slots);
            assert_eq!((slots.0, end), separate);
        }
    }

    #[test]
    fn an_over_aligned_slot_shares_at_aligned_offsets() {
        let source = "decl @use(ptr)
func @f() {
    ss0 = stack_slot 56, align 8
    ss1 = stack_slot 128, align 64
    ss2 = stack_slot 8, align 8
    ss3 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    v1 = stack_addr ss1
    v2 = stack_addr ss2
    v3 = stack_addr ss3
    call @use(v0)
    intrinsic lifetime_start(v1)
    call @use(v1)
    intrinsic lifetime_end(v1)
    intrinsic lifetime_start(v2)
    intrinsic lifetime_start(v3)
    call @use(v2)
    call @use(v3)
    intrinsic lifetime_end(v2)
    intrinsic lifetime_end(v3)
    return
}";
        let (offsets, end) = shared(source);
        // ss0 is unmarked and keeps [0, 56). ss1 is placed first among the
        // marked slots, at 64. ss2 fills the gap below it, and ss3, live with
        // ss2 but not with ss1, takes ss1's first bytes.
        assert_eq!(offsets, [Some(0), Some(64), Some(56), Some(64)]);
        assert_eq!(end, 192);
    }

    #[test]
    fn too_many_live_pairs_fall_back_to_separate_bytes() {
        let n = 120;
        let mut source = String::from("func @f() {\n");
        for i in 0..n {
            source += &format!("    ss{i} = stack_slot 8, align 8\n");
        }
        source += "block0:\n";
        for i in 0..n {
            source += &format!("    v{i} = stack_addr ss{i}\n    intrinsic lifetime_start(v{i})\n");
        }
        for i in 0..n {
            source += &format!("    intrinsic lifetime_end(v{i})\n");
        }
        source += "    return\n}";
        // 7,140 pairs are live together, above the budget of 6,016.
        let (offsets, end) = shared(&source);
        assert_eq!(end, 8 * n);
        assert!(offsets.iter().enumerate().all(|(i, &o)| o == Some(8 * i)));
    }

    #[test]
    #[should_panic(expected = "address of omitted stack slot")]
    fn omitted_storage_cannot_be_used_as_an_address() {
        let mut f = Function::new("f", keel_ir::Signature::new(&[], &[]));
        f.create_stack_slot(8, 8);
        let (slots, _) = SlotOffsets::layout(&f, 0, false, false);
        let _ = slots[0];
    }
}
