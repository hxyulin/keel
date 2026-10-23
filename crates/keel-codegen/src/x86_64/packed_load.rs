//! Exact-footprint loads used only for widening.
use super::*;
use keel_ir::fxhash::FxHashMap as HashMap;

/// A closed native m128 -> YMM signed widening. Retain the load's selected
/// address leaves until the root, so allocation sees their extended lifetime.
pub(super) fn select_wide(
    func: &Function,
    selection: &mut crate::select::Selection,
    features: keel_asm_ir::x86::Features,
) -> HashMap<Inst, Value> {
    let mut selected = HashMap::default();
    if !features.avx || !features.avx2 {
        return selected;
    }
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    for block in func.blocks() {
        for pair in func.block_insts(block).windows(2) {
            let (load, widen) = (pair[0], pair[1]);
            let InstData::Load {
                ty: Type::Vector(keel_ir::VectorType::I32x4),
                addr,
                flags,
            } = *func.inst_data(load)
            else {
                continue;
            };
            let InstData::Convert {
                op: ConvertOp::SwidenLow,
                ty: Type::Vector(keel_ir::VectorType::I64x4),
                arg,
            } = *func.inst_data(widen)
            else {
                continue;
            };
            if flags.volatile
                || func.resolve(arg) != func.first_result(load)
                || uses[func.first_result(load).index()] != 1
                || pair.iter().any(|&i| {
                    selection.covered(i)
                        || selection.matches.contains_key(&i)
                        || selection.has_constraint(i)
                        || selection.conditions.contains_key(&i)
                        || func.inst_loc(i) != func.inst_loc(load)
                        || func.resolve(func.first_result(i)) != func.first_result(i)
                })
            {
                continue;
            }
            let operands = selection.operands(func, load);
            selection.cover(load);
            selection.set_operands(widen, operands);
            selected.insert(widen, func.resolve(addr));
        }
    }
    selected
}

pub(super) fn select(
    func: &Function,
    selection: &mut crate::select::Selection,
    sse41: bool,
) -> HashMap<Inst, Value> {
    // Every chain contains a vector splat; skip the use counts without one.
    if !func
        .layout_insts()
        .any(|(_, i)| matches!(func.inst_data(i), InstData::VectorSplat { .. }))
    {
        return HashMap::default();
    }
    let mut uses = vec![0u8; func.num_values()];
    for (_, inst) in func.layout_insts() {
        func.inst_data(inst).for_each_operand(|v| {
            let count = &mut uses[func.resolve(v).index()];
            *count = count.saturating_add(1);
        });
    }
    let mut selected = HashMap::default();
    for block in func.blocks() {
        for chain in func.block_insts(block).windows(4) {
            let [load, splat, cast, widen] = [chain[0], chain[1], chain[2], chain[3]];
            if !matches!(func.inst_data(splat), InstData::VectorSplat { .. }) {
                continue;
            }
            if chain.iter().any(|&i| {
                selection.covered(i)
                    || selection.matches.contains_key(&i)
                    || selection.has_constraint(i)
            }) {
                continue;
            }
            // Keep explicitly distinct source lines as separate boundaries.
            let mut location = None;
            if chain.iter().any(|&i| {
                if let Some(loc) = func.inst_loc(i) {
                    if let Some((file, line)) = location {
                        return file != loc.file || line != loc.line;
                    }
                    location = Some((loc.file, loc.line));
                }
                false
            }) {
                continue;
            }
            let InstData::Load {
                ty: Type::I64,
                addr,
                flags,
            } = *func.inst_data(load)
            else {
                continue;
            };
            let InstData::VectorSplat { ty, arg } = *func.inst_data(splat) else {
                continue;
            };
            if flags.volatile
                || ty != Type::Vector(keel_ir::VectorType::I64x2)
                || func.resolve(arg) != func.first_result(load)
            {
                continue;
            }
            let InstData::Convert {
                op: ConvertOp::Bitcast,
                ty: narrow,
                arg,
            } = *func.inst_data(cast)
            else {
                continue;
            };
            if func.resolve(arg) != func.first_result(splat) {
                continue;
            }
            let InstData::Convert {
                op: ConvertOp::SwidenLow | ConvertOp::UwidenLow,
                ty: wide,
                arg,
            } = *func.inst_data(widen)
            else {
                continue;
            };
            if func.resolve(arg) != func.first_result(cast)
                || [load, splat, cast]
                    .iter()
                    .any(|&i| uses[func.first_result(i).index()] != 1)
            {
                continue;
            }
            let (Some(narrow), Some(wide)) = (narrow.vector(), wide.vector()) else {
                continue;
            };
            if !narrow.lane_type().is_int()
                || !wide.lane_type().is_int()
                || narrow.lane_type().bytes().unwrap() * 2 != wide.lane_type().bytes().unwrap()
            {
                continue;
            }
            // This closed signed i32 -> i64 chain can read its original m64
            // directly into the final XMM result. Keep the generic low-half
            // load cover for every other shape and effective feature profile.
            let signed_dwords = sse41
                && narrow == keel_ir::VectorType::I32x4
                && wide == keel_ir::VectorType::I64x2
                && chain.iter().all(|&i| {
                    func.inst_loc(i) == func.inst_loc(load)
                        && func.resolve(func.first_result(i)) == func.first_result(i)
                        && !selection.conditions.contains_key(&i)
                })
                && matches!(
                    func.inst_data(widen),
                    InstData::Convert {
                        op: ConvertOp::SwidenLow,
                        ..
                    }
                );
            // Every admitted widening observes exactly the low eight bytes.
            // The upper splat lane is unobservable through its sole use chain.
            // Move only across adjacent pure vector operations, retaining the
            // original load's address leaves and exact eight-byte footprint.
            let operands = selection.operands(func, load);
            selection.cover(load);
            let root = if signed_dwords {
                selection.cover(splat);
                selection.cover(cast);
                widen
            } else {
                splat
            };
            selection.set_operands(root, operands);
            selected.insert(root, func.resolve(addr));
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "func @f(ptr,i64,i64x2,i32x4) {\nblock0(v0:ptr,v1:i64,v2:i64x2,v3:i32x4):\n v4 = load.i64 v0\n v5 = vsplat.i64x2 v4\n v6 = bitcast.i32x4 v5\n v7 = swiden_low.i64x2 v6\n return\n}\n";

    #[test]
    fn signed_root_respects_existing_conditions_covers_and_resolved_results() {
        for at in 0..4 {
            for boundary in 0..4 {
                let mut module = keel_ir::parse(SOURCE).unwrap();
                let f = &mut module.functions[0];
                let chain: Vec<_> = f.layout_insts().take(4).map(|(_, i)| i).collect();
                let mut selection = crate::select::Selection::default();
                match boundary {
                    0 => {
                        selection.conditions.insert(chain[at], (chain[0], false));
                    }
                    1 => selection.cover(chain[at]),
                    2 => {
                        selection.matches.insert(
                            chain[at],
                            crate::select::Match {
                                machine: crate::select::Machine::Lea,
                                args: vec![],
                                shift: 0,
                            },
                        );
                    }
                    _ => {
                        let replacement =
                            [Value::new(1), Value::new(2), Value::new(3), Value::new(2)][at];
                        f.replace_all_uses(f.first_result(chain[at]), replacement);
                    }
                }
                let selected = select(f, &mut selection, true);
                assert!(
                    !selected.contains_key(&chain[3]),
                    "node {at}, boundary {boundary}"
                );
            }
        }
    }

    #[test]
    fn signed_root_inherits_selected_load_leaves() {
        let module = keel_ir::parse(SOURCE).unwrap();
        let f = &module.functions[0];
        let chain: Vec<_> = f.layout_insts().take(4).map(|(_, i)| i).collect();
        let mut selection = crate::select::Selection::default();
        // A selected address can expose more leaves than the raw load operand.
        let leaves = vec![Value::new(0), Value::new(1)];
        selection.set_operands(chain[0], leaves.clone());
        let selected = select(f, &mut selection, true);
        assert_eq!(selected.get(&chain[3]), Some(&Value::new(0)));
        assert_eq!(selection.operands(f, chain[3]), leaves);
        assert!(chain[..3].iter().all(|&i| selection.covered(i)));
        assert!(!selection.covered(chain[3]));
    }
}

#[cfg(test)]
mod wide_tests {
    use super::*;

    const SOURCE: &str = "func @f(ptr,i64,i32x4,i64x4) {\nblock0(v0:ptr,v1:i64,v2:i32x4,v3:i64x4):\n v4 = load.i32x4 v0\n v5 = swiden_low.i64x4 v4\n return\n}\n";

    #[test]
    fn effective_avx_and_avx2_are_both_required() {
        use keel_asm_ir::x86::{CpuLevel, Features};
        let module = keel_ir::parse(SOURCE).unwrap();
        let f = &module.functions[0];
        for (avx, avx2) in [(false, false), (true, false), (false, true), (true, true)] {
            let features = Features {
                avx,
                avx2,
                ..Features::for_level(CpuLevel::V3)
            };
            let selected = select_wide(f, &mut crate::select::Selection::default(), features);
            assert_eq!(selected.len(), usize::from(avx && avx2));
        }
    }

    #[test]
    fn signed_root_respects_existing_conditions_covers_and_resolved_results() {
        for at in 0..2 {
            for boundary in 0..4 {
                let mut module = keel_ir::parse(SOURCE).unwrap();
                let f = &mut module.functions[0];
                let chain: Vec<_> = f.layout_insts().take(2).map(|(_, i)| i).collect();
                let mut selection = crate::select::Selection::default();
                match boundary {
                    0 => {
                        selection.conditions.insert(chain[at], (chain[0], false));
                    }
                    1 => selection.cover(chain[at]),
                    2 => {
                        selection.matches.insert(
                            chain[at],
                            crate::select::Match {
                                machine: crate::select::Machine::Lea,
                                args: vec![],
                                shift: 0,
                            },
                        );
                    }
                    _ => {
                        let replacement = [Value::new(2), Value::new(3)][at];
                        f.replace_all_uses(f.first_result(chain[at]), replacement);
                    }
                }
                let selected = select_wide(
                    f,
                    &mut selection,
                    keel_asm_ir::x86::Features::for_level(keel_asm_ir::x86::CpuLevel::V3),
                );
                assert!(
                    !selected.contains_key(&chain[1]),
                    "node {at}, boundary {boundary}"
                );
            }
        }
    }

    #[test]
    fn signed_root_inherits_selected_load_leaves() {
        let module = keel_ir::parse(SOURCE).unwrap();
        let f = &module.functions[0];
        let chain: Vec<_> = f.layout_insts().take(2).map(|(_, i)| i).collect();
        let mut selection = crate::select::Selection::default();
        // A selected address can expose more leaves than the raw load operand.
        let leaves = vec![Value::new(0), Value::new(1)];
        selection.set_operands(chain[0], leaves.clone());
        let selected = select_wide(
            f,
            &mut selection,
            keel_asm_ir::x86::Features::for_level(keel_asm_ir::x86::CpuLevel::V3),
        );
        assert_eq!(selected.get(&chain[1]), Some(&Value::new(0)));
        assert_eq!(selection.operands(f, chain[1]), leaves);
        assert!(chain[..1].iter().all(|&i| selection.covered(i)));
        assert!(!selection.covered(chain[1]));
    }
}
