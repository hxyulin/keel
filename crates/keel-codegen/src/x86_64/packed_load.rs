//! Load exactly the observed low 64 bits of a splat used only for widening.
use super::*;
use keel_ir::fxhash::FxHashMap as HashMap;

pub(super) fn select(
    func: &Function,
    selection: &mut crate::select::Selection,
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
            // Every admitted widening observes exactly the low eight bytes.
            // The upper splat lane is unobservable through its sole use chain.
            // Move only across the adjacent pure splat, retaining the original
            // load's selected address leaves and exact eight-byte footprint.
            let operands = selection.operands(func, load);
            selection.cover(load);
            selection.set_operands(splat, operands);
            selected.insert(splat, func.resolve(addr));
        }
    }
    selected
}
