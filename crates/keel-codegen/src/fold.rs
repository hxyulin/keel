//! Folding values into the instructions that use them, shared by the
//! targets. Each target says which operands of an instruction it can
//! encode in the instruction itself (a stack address as a memory operand,
//! a constant as an immediate); a value folded into every one of its uses
//! needs no code of its own.

#[cfg(test)]
use keel_ir::fxhash::FxHashSet as HashSet;
use keel_ir::{EntityRef, Function, InstData, Type, Value, ValueDef};

/// The value of `v` if an `iconst` defines it.
pub(crate) fn iconst(func: &Function, v: Value) -> Option<i64> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(v)) else {
        return None;
    };
    match *func.inst_data(inst) {
        InstData::Iconst { ty, imm } if ty != Type::I128 => Some(imm),
        _ => None,
    }
}

/// Whether a materialized narrow value has zero bits above its IR width.
/// This is a definition invariant, not a property of narrow types: arguments,
/// block parameters (even with clean incoming edges), calls, truncations and
/// arithmetic do not qualify. Resolve aliases before inspecting the definition.
///
/// Plain i8/i16 loads write a zero-extended full GPR on both targets; AArch64
/// comparisons also write the whole W register. x86 SETcc only writes a byte.
/// Both emitters' scalar register/stack transfers, including Fast/Full spills,
/// edge copies and experimental allocation edits, copy all 64 bits. Thus the
/// invariant follows the value through allocation, without tracking registers.
/// Covered/selected producers and unassigned/rematerialized definitions are
/// deliberately excluded: their machine definition need not be the plain one.
pub(crate) fn zero_extended_narrow(
    func: &Function,
    selection: &crate::select::Selection,
    alloc: &crate::regalloc::Allocation,
    value: Value,
    aarch64: bool,
) -> bool {
    use crate::regalloc::{Loc, OperandKind, ValuePart};
    let value = func.resolve(value);
    if !matches!(func.value_type(value), Type::I8 | Type::I16) {
        return false;
    }
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return false;
    };
    if selection.covered(inst) || selection.matches.contains_key(&inst) {
        return false;
    }
    if !matches!(
        alloc.operand_loc(
            alloc.edits.instruction_index(inst),
            ValuePart::low(value),
            OperandKind::Def
        ),
        Some(Loc::Reg(_) | Loc::Stack(_))
    ) {
        return false;
    }
    matches!(
        func.inst_data(inst),
        InstData::Load {
            ty: Type::I8 | Type::I16,
            ..
        }
    ) || (aarch64
        && matches!(
            func.inst_data(inst),
            InstData::Icmp { .. } | InstData::Fcmp { .. }
        ))
}

/// A set of one function's values, as a dense membership table.
#[derive(Clone, Debug, Default)]
pub(crate) struct ValueSet {
    members: Vec<bool>,
}

impl ValueSet {
    pub(crate) fn contains(&self, value: &Value) -> bool {
        self.members.get(value.index()).copied().unwrap_or(false)
    }

    #[cfg(test)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = Value> + '_ {
        (self.members.iter().enumerate()).filter_map(|(i, &m)| m.then(|| Value::new(i)))
    }
}

/// The `stack_addr` and `iconst` results every use of which is folded
/// into the instruction using it, by `folds`, which appends the operands an
/// instruction folds (each once per operand position).
pub(crate) fn folded_values(
    func: &Function,
    selection: &crate::select::Selection,
    folds: impl Fn(&InstData, &mut Vec<Value>),
) -> ValueSet {
    // Only these results can be folded: start with all of them, and drop each
    // as soon as some use needs it materialized.
    let mut members = vec![false; func.num_values()];
    let mut candidates = false;
    for (_, inst) in func.layout_insts() {
        if let InstData::StackAddr { .. } | InstData::Iconst { .. } = func.inst_data(inst) {
            for &v in func.inst_results(inst) {
                members[v.index()] = true;
                candidates = true;
            }
        }
    }
    if !candidates {
        return ValueSet { members };
    }
    let mut folded = Vec::new();
    for (_, inst) in func.layout_insts() {
        if selection.covered(inst) {
            continue;
        }
        // Folds only decide anything for operands that are still members.
        // Others are either materialized already or can never be folded.
        let mut reads_member = false;
        selection.for_each_operand(func, inst, |v| {
            reads_member |= members[func.resolve(v).index()];
        });
        if !reads_member {
            continue;
        }
        folded.clear();
        if !selection.matches.contains_key(&inst) {
            folds(selection.folded_data(func, inst), &mut folded);
            selection.remaining_folds(func, inst, &mut folded);
        }
        selection.for_each_operand(func, inst, |v| {
            let v = func.resolve(v);
            // Once any use needs materialization, other occurrences cannot
            // make this value folded. Leaving its own fold entries alone
            // does not affect matching any distinct operand value.
            if !members[v.index()] {
                return;
            }
            match folded.iter().position(|&f| f == v) {
                Some(i) => {
                    folded.swap_remove(i);
                }
                None => members[v.index()] = false,
            }
        });
    }
    ValueSet { members }
}

/// What `folds` gave each instruction reading a `stack_addr` or `iconst`
/// result, those it already folded removed: the instruction and the range
/// of `values` holding them.
#[derive(Default)]
pub(crate) struct FoldRecord {
    pub(crate) insts: Vec<(keel_ir::Inst, u32, u32)>,
    pub(crate) values: Vec<Value>,
}

/// [`folded_values`], also recording in `record` what `folds` gives every
/// instruction reading a `stack_addr` or `iconst` result: no other folds
/// anything. A separate walk, so that [`folded_values`] stays as tight.
pub(crate) fn folded_values_recording(
    func: &Function,
    selection: &crate::select::Selection,
    folds: impl Fn(&InstData, &mut Vec<Value>),
    record: &mut FoldRecord,
) -> ValueSet {
    let mut members = vec![false; func.num_values()];
    for (_, inst) in func.layout_insts() {
        if let InstData::StackAddr { .. } | InstData::Iconst { .. } = func.inst_data(inst) {
            for &v in func.inst_results(inst) {
                members[v.index()] = true;
            }
        }
    }
    if !members.contains(&true) {
        return ValueSet { members };
    }
    // Instructions are recorded even once the results they read have
    // dropped out of `members`.
    let results = members.clone();
    let mut folded = Vec::new();
    for (_, inst) in func.layout_insts() {
        if selection.covered(inst) {
            continue;
        }
        let mut reads_result = false;
        selection.for_each_operand(func, inst, |v| {
            reads_result |= results[func.resolve(v).index()];
        });
        if !reads_result {
            continue;
        }
        folded.clear();
        if !selection.matches.contains_key(&inst) {
            folds(selection.folded_data(func, inst), &mut folded);
            selection.remaining_folds(func, inst, &mut folded);
        }
        if !folded.is_empty() {
            let start = record.values.len() as u32;
            record.values.extend_from_slice(&folded);
            record.insts.push((inst, start, record.values.len() as u32));
        }
        // As in `folded_values`.
        selection.for_each_operand(func, inst, |v| {
            let v = func.resolve(v);
            if !members[v.index()] {
                return;
            }
            match folded.iter().position(|&f| f == v) {
                Some(i) => {
                    folded.swap_remove(i);
                }
                None => members[v.index()] = false,
            }
        });
    }
    ValueSet { members }
}

/// The `icmp` and `fcmp` results only used as the condition of the
/// `brif` right after them: the branch tests the flags of the compare
/// itself, a compare and a conditional branch, with no 0/1 value in between.
#[cfg(test)]
pub(crate) fn fused_compares(func: &Function) -> HashSet<Value> {
    fused_compare_pairs(func, &use_counts(func))
        .into_iter()
        .map(|(value, _, _)| value)
        .collect()
}

/// Machine-relevant IR operand occurrences, saturated at 2: selection only
/// distinguishes a single use from none or several. Lifetime-marker addresses
/// require no machine value and do not prevent address folding.
/// Targets count them while finding [`crate::kinds::InstKinds`].
pub(crate) fn use_counts(func: &Function) -> Vec<u8> {
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        if func.inst_data(inst).is_lifetime_marker() {
            continue;
        }
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = (*count + 1).min(2);
        });
    }
    uses
}

pub(crate) fn fused_compare_pairs(
    func: &Function,
    uses: &[u8],
) -> Vec<(Value, keel_ir::Inst, keel_ir::Inst)> {
    compare_pairs(func, false, uses)
}

/// ARM can delay an exact-width integer compare over at most four tiny pure
/// definitions. Selection must transfer its operand occurrences to the branch;
/// emission must generate the compare there, after all intervening lowering.
pub(crate) fn aarch64_compare_pairs(
    func: &Function,
    uses: &[u8],
) -> Vec<(Value, keel_ir::Inst, keel_ir::Inst)> {
    compare_pairs(func, true, uses)
}

/// `uses` are [`use_counts`]: only zero, one, or multiple raw IR uses matter.
fn compare_pairs(
    func: &Function,
    aarch64: bool,
    uses: &[u8],
) -> Vec<(Value, keel_ir::Inst, keel_ir::Inst)> {
    let mut candidates = Vec::new();
    for block in func.blocks() {
        let insts = func.block_insts(block);
        for (i, &inst) in insts.iter().enumerate() {
            let data = func.inst_data(inst);
            let compare = match *data {
                InstData::Icmp { args, .. } => func.value_type(func.resolve(args[0])) != Type::I128,
                InstData::Fcmp { .. } => true,
                _ => false,
            };
            if !compare {
                continue;
            }
            let bounded = aarch64
                && matches!(*data,
                InstData::Icmp { args, .. }
                    if matches!(func.value_type(func.resolve(args[0])), Type::I32 | Type::I64));
            let location = |inst| func.inst_loc(inst).map(|loc| (loc.file, loc.line));
            for (gap, &next) in insts[i + 1..]
                .iter()
                .take(if bounded { 5 } else { 1 })
                .enumerate()
            {
                if let InstData::Brif { cond, .. } = func.inst_data(next) {
                    let result = func.inst_results(inst)[0];
                    // Keep the established adjacent behavior. Nonadjacent
                    // fusion cannot erase independently located source lines.
                    if func.resolve(*cond) == result
                        && (gap == 0 || location(next) == location(inst))
                    {
                        candidates.push((result, inst, next));
                    }
                    break;
                }
                if !bounded
                    || location(next) != location(inst)
                    || !matches!(
                        func.inst_data(next),
                        InstData::Iconst {
                            ty: Type::I32 | Type::I64,
                            ..
                        } | InstData::StackAddr { .. }
                            | InstData::Convert {
                                op: keel_ir::ConvertOp::IntToPtr,
                                ..
                            }
                    )
                {
                    break;
                }
            }
        }
    }
    candidates.retain(|(v, _, _)| uses[v.index()] == 1);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::Selection;
    use keel_ir::fxhash::FxHashMap as HashMap;

    fn function(source: &str) -> Function {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        module.functions.into_iter().next().unwrap()
    }

    fn immediates(func: &Function, data: &InstData) -> Vec<Value> {
        match *data {
            InstData::Binary { args, .. } if iconst(func, args[1]).is_some() => vec![args[1]],
            _ => vec![],
        }
    }

    // Former hash-based analysis, before optional allocation operand filtering
    // (where it had a double-subtraction bug).
    fn reference_folded(func: &Function, selection: &Selection) -> HashSet<Value> {
        let mut values = HashSet::default();
        let mut unfolded = HashSet::default();
        for (_, inst) in func.layout_insts() {
            let data = func.inst_data(inst);
            if matches!(data, InstData::StackAddr { .. } | InstData::Iconst { .. }) {
                values.extend(func.inst_results(inst).iter().copied());
            }
            let mut folds: Vec<_> = if selection.matches.contains_key(&inst) {
                vec![]
            } else {
                immediates(func, data)
                    .into_iter()
                    .map(|v| func.resolve(v))
                    .collect()
            };
            selection.for_each_operand(func, inst, |v| {
                let v = func.resolve(v);
                if let Some(i) = folds.iter().position(|&f| f == v) {
                    folds.swap_remove(i);
                } else {
                    unfolded.insert(v);
                }
            });
        }
        values.retain(|v| !unfolded.contains(v));
        values
    }

    #[test]
    fn dense_folding_matches_hash_analysis_with_aliases_and_selected_leaves() {
        let mut func = function(
            "func @f(i64) -> i64 {
block0(v0: i64):
 v1 = iconst.i64 7
 v2 = iconst.i64 9
 v3 = iadd v1, v1
 v4 = imul v0, v2
 v5 = iadd v3, v4
 return v5
}",
        );
        let constants: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| {
                matches!(func.inst_data(inst), InstData::Iconst { .. })
                    .then(|| func.inst_results(inst)[0])
            })
            .collect();
        for alias in [false, true] {
            if alias {
                func.replace_all_uses(constants[1], constants[0]);
            }
            for target in [crate::select::Target::Aarch64, crate::select::Target::X86] {
                for selection in [Selection::default(), Selection::compute(&func, target)] {
                    assert_eq!(
                        folded_values(&func, &selection, |data, out| {
                            out.extend(immediates(&func, data))
                        })
                        .iter()
                        .collect::<HashSet<_>>(),
                        reference_folded(&func, &selection)
                    );
                }
            }
        }
    }

    #[test]
    fn allocation_folds_are_not_applied_twice_and_later_addresses_still_fold() {
        let func = function(
            "func @f() -> i64 {
 ss0 = stack_slot 8, align 8
block0:
 v0 = iconst.i64 7
 v1 = iadd v0, v0
 v2 = stack_addr ss0
 v3 = load.i64 v2
 v4 = iadd v1, v3
 return v4
}",
        );
        let insts: Vec<_> = func.layout_insts().map(|(_, inst)| inst).collect();
        let constant = func.inst_results(insts[0])[0];
        let address = func.inst_results(insts[2])[0];
        let mut selection = Selection::default();
        for _ in 0..2 {
            selection.fold_operands(&func, |data| immediates(&func, data));
            assert_eq!(selection.operands(&func, insts[1]), vec![constant]);
            let folded = folded_values(&func, &selection, |data, out| match *data {
                InstData::Load { addr, .. } => out.push(addr),
                _ => out.extend(immediates(&func, data)),
            });
            assert_eq!(
                folded.iter().collect::<HashSet<_>>(),
                HashSet::from_iter([address])
            );
        }
    }

    #[test]
    fn covered_instructions_do_not_call_target_folding() {
        let func = function(
            "func @f() -> i64 {
block0:
 v0 = iconst.i64 7
 v1 = iadd v0, v0
 return v0
}",
        );
        let mut selection = Selection::default();
        let binary = func
            .layout_insts()
            .find_map(|(_, inst)| {
                matches!(func.inst_data(inst), InstData::Binary { .. }).then_some(inst)
            })
            .unwrap();
        selection.cover(binary);
        folded_values(&func, &selection, |data, _| {
            assert!(!matches!(data, InstData::Binary { .. }));
        });
    }

    fn reference_compares(func: &Function) -> HashSet<Value> {
        let mut uses = HashMap::<Value, usize>::default();
        let mut candidates = HashSet::default();
        for block in func.blocks() {
            let insts = func.block_insts(block);
            for (i, &inst) in insts.iter().enumerate() {
                let data = func.inst_data(inst);
                data.for_each_operand(|v| *uses.entry(func.resolve(v)).or_default() += 1);
                let compare = match *data {
                    InstData::Icmp { args, .. } => {
                        func.value_type(func.resolve(args[0])) != Type::I128
                    }
                    InstData::Fcmp { .. } => true,
                    _ => false,
                };
                if compare
                    && let Some(&next) = insts.get(i + 1)
                    && let InstData::Brif { cond, .. } = func.inst_data(next)
                {
                    let result = func.inst_results(inst)[0];
                    if func.resolve(*cond) == result {
                        candidates.insert(result);
                    }
                }
            }
        }
        candidates.retain(|v| uses.get(v) == Some(&1));
        candidates
    }

    #[test]
    fn saturated_compare_counts_match_raw_use_analysis() {
        for ty in ["i64", "i128", "f64"] {
            for extra in ["", "v3 = uext.i64 v2\n", "v3 = and v2, v2\n"] {
                let compare = if ty == "f64" { "fcmp.eq" } else { "icmp.eq" };
                let source = format!(
                    "func @f({ty}, {ty}) -> i64 {{
block0(v0: {ty}, v1: {ty}):
 v2 = {compare} v0, v1
 {extra}brif v2, block1, block2
block1:
 v4 = iconst.i64 1
 return v4
block2:
 v5 = iconst.i64 0
 return v5
}}"
                );
                let func = function(&source);
                assert_eq!(fused_compares(&func), reference_compares(&func));
                assert_eq!(
                    fused_compares(&func).len(),
                    usize::from(extra.is_empty() && ty != "i128")
                );
            }
        }
    }

    #[test]
    fn compare_branch_arguments_and_aliases_count_as_raw_uses() {
        let mut func = function(
            "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = icmp.eq v0, v1
 v3 = icmp.ne v0, v1
 brif v3, block1(v2), block2
block1(v4: i8):
 v5 = uext.i64 v4
 return v5
block2:
 v6 = iconst.i64 0
 return v6
}",
        );
        let compares: Vec<_> = func
            .layout_insts()
            .filter_map(|(_, inst)| {
                matches!(func.inst_data(inst), InstData::Icmp { .. })
                    .then(|| func.inst_results(inst)[0])
            })
            .collect();
        assert_eq!(fused_compares(&func), HashSet::from_iter([compares[1]]));
        // The branch condition and its edge argument now resolve to the same
        // adjacent comparison: two reads, so materialize the comparison.
        func.replace_all_uses(compares[0], compares[1]);
        assert_eq!(fused_compares(&func), reference_compares(&func));
        assert!(fused_compares(&func).is_empty());
    }

    fn aarch64_compare_pairs_of(func: &Function) -> Vec<(Value, keel_ir::Inst, keel_ir::Inst)> {
        aarch64_compare_pairs(func, &use_counts(func))
    }

    fn span_source(ty: &str, gap: &str, compare: &str, edge: &str) -> String {
        format!(
            "func @f({ty}, {ty}) -> {ty} {{
block0(v0: {ty}, v1: {ty}):
 v2 = {compare} v0, v1
 {gap}
 brif v2, block1({edge}), block2
block1(v20: {ty}):
 return v20
block2:
 return v1
}}"
        )
    }

    #[test]
    fn arm_bounded_span_transfers_duplicate_uses_to_branch() {
        let func = function(&span_source("i64", "v3 = iconst.i64 7", "icmp.eq", "v0"));
        assert!(fused_compare_pairs(&func, &use_counts(&func)).is_empty()); // x86 contract unchanged
        let pairs = aarch64_compare_pairs(&func, &use_counts(&func));
        assert_eq!(pairs.len(), 1);
        let (cond, compare, branch) = pairs[0];
        let mut selection = Selection::default();
        assert_eq!(
            selection.fuse_aarch64_compares(&func, |_| vec![], &use_counts(&func)),
            HashSet::from_iter([cond])
        );
        assert!(selection.covered(compare));
        // One edge argument plus both comparison operand occurrences. The
        // old comparison no longer reads any operands at allocation time.
        assert!(selection.operands(&func, compare).is_empty());
        let leaves = selection.operands(&func, branch);
        assert_eq!(leaves.len(), 3);
        assert_eq!(leaves[0], leaves[1]);
        assert_ne!(leaves[1], leaves[2]);

        let mut aliases = func.clone();
        aliases.replace_all_uses(leaves[2], leaves[1]);
        let mut selection = Selection::default();
        selection.fuse_aarch64_compares(&aliases, |_| vec![], &use_counts(&aliases));
        assert_eq!(selection.operands(&aliases, branch), vec![leaves[1]; 3]);
    }

    #[test]
    fn arm_spans_reject_nonwhitelist_work_bounds_and_multiuse_booleans() {
        for gap in [
            "v3 = iadd v0, v1",
            "v3 = udiv v0, v1",
            "v3 = icmp.ne v0, v1",
            "v3 = iconst.i64 1\nv4 = iconst.i64 2\nv5 = iconst.i64 3\nv6 = iconst.i64 4\nv7 = iconst.i64 5",
            "v3 = uext.i64 v2",
        ] {
            let func = function(&span_source("i64", gap, "icmp.eq", "v0"));
            assert!(
                aarch64_compare_pairs(&func, &use_counts(&func)).is_empty(),
                "{gap}"
            );
        }
        for (ty, compare) in [
            ("i8", "icmp.eq"),
            ("i16", "icmp.eq"),
            ("i128", "icmp.eq"),
            ("f64", "fcmp.eq"),
        ] {
            let func = function(&span_source(ty, "v3 = iconst.i64 7", compare, "v0"));
            assert!(
                aarch64_compare_pairs(&func, &use_counts(&func)).is_empty(),
                "{ty}"
            );
        }
        for n in 1..=4 {
            let gap = (0..n)
                .map(|i| format!("v{} = iconst.i64 7", i + 3))
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(
                aarch64_compare_pairs_of(&function(&span_source("i64", &gap, "icmp.eq", "v0")))
                    .len(),
                1
            );
        }
    }

    #[test]
    fn arm_spans_preserve_explicit_source_lines() {
        use keel_ir::{SourceFile, SourceLoc};
        let mut func = function(&span_source("i64", "v3 = iconst.i64 7", "icmp.eq", "v0"));
        let insts = func.block_insts(func.entry_block().unwrap()).to_vec();
        let loc = SourceLoc {
            file: SourceFile::new(0),
            line: 10,
            col: 1,
            scope: None,
        };
        for &inst in &insts {
            func.set_inst_loc(inst, Some(loc));
        }
        assert_eq!(aarch64_compare_pairs(&func, &use_counts(&func)).len(), 1);
        for index in [1, 2] {
            func.set_inst_loc(insts[index], Some(SourceLoc { line: 11, ..loc }));
            assert!(aarch64_compare_pairs(&func, &use_counts(&func)).is_empty());
            func.set_inst_loc(insts[index], Some(loc));
        }
        func.set_inst_loc(insts[1], None);
        assert!(aarch64_compare_pairs(&func, &use_counts(&func)).is_empty());
    }
}
