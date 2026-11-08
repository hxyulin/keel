//! Outline bounded terminal panic records without growing their callers' frames.
use crate::Pass;
use keel_ir::fxhash::FxHashSet;
use keel_ir::{
    AbiParam, Block, CallArgs, CallConv, Function, FunctionAttrs, InsertPoint, InstData, Module,
    Purpose, Type, Value, ValueDef,
};

pub struct OutlineCold;
impl Pass for OutlineCold {
    fn name(&self) -> &'static str {
        "outline-cold"
    }
    fn description(&self) -> &'static str {
        "tail-outline self-contained terminal trap blocks with stack records"
    }
    fn run(&self, _: &mut Function, _: &crate::OptimizationContext) -> bool {
        false
    }
    fn run_module(
        &self,
        module: &mut Module,
        context: &mut crate::ModuleContext<'_>,
    ) -> Option<bool> {
        if !module.assembly.is_empty() {
            return Some(false);
        }
        let initial = module.functions.len();
        let mut changed = false;
        for index in 0..initial {
            let f = &module.functions[index];
            if !(context.enabled)(f) || !eligible(f) {
                continue;
            }
            let candidates: Vec<_> = f
                .blocks()
                .filter(|&b| {
                    Some(b) != f.entry_block()
                        && f.block_params(b).is_empty()
                        && f.block_insts(b)
                            .last()
                            .is_some_and(|&i| matches!(f.inst_data(i), InstData::Trap))
                        && f.block_insts(b).len() <= 128
                        && f.block_insts(b)
                            .iter()
                            .any(|&i| matches!(f.inst_data(i), InstData::StackAddr { .. }))
                })
                .take(2)
                .collect();
            for block in candidates {
                let name = format!("{}.cold.{}", module.functions[index].name, block);
                if module.has_symbol(&name)
                    || module.functions.iter().any(|f| {
                        f.callees().any(|(_, d)| d.name == name)
                            || f.data_symbols().any(|(_, symbol)| symbol == name)
                    })
                    || module
                        .data
                        .iter()
                        .filter_map(|d| d.init.as_ref())
                        .flat_map(|init| &init.items)
                        .any(|item| {
                            matches!(item, keel_ir::DataItem::Ptr { symbol, .. }
                            | keel_ir::DataItem::RttiName { symbol } if symbol == &name)
                        })
                {
                    continue;
                }
                if let Some(helper) = outline(&mut module.functions[index], block, name) {
                    module.functions.push(helper);
                    changed = true;
                }
            }
        }
        Some(changed)
    }
}
fn scalar(ty: Type) -> bool {
    matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr)
}
fn eligible(f: &Function) -> bool {
    f.sig.cc == CallConv::Fast
        && !f.sig.variadic
        && !f.sig.returns_twice
        && !f.attrs.alwaysinline
        && !f.attrs.optnone
        && !f.attrs.cold
        && !f.attrs.naked
        && !f.attrs.interrupt
        && f.personality.is_none()
        && f.debug_loc.is_none()
        && f.debug_vars.is_empty()
        && f.sig
            .params
            .iter()
            .chain(&f.sig.returns)
            .all(|p| p.purpose == Purpose::Normal && scalar(p.ty))
        && f.num_values() <= 4096
        && f.num_insts() <= 4096
        && f.layout_insts().take(257).count() <= 256
        && !f.layout_insts().any(|(_, i)| {
            f.inst_loc(i).is_some()
                || matches!(
                    f.inst_data(i),
                    InstData::InlineAsm { .. }
                        | InstData::Invoke { .. }
                        | InstData::LandingPad { .. }
                        | InstData::Intrinsic { .. }
                )
                || match f.inst_data(i) {
                    InstData::Call { func, .. } => f.callee(*func).sig.returns_twice,
                    InstData::CallIndirect { sig, .. } => f.signature(*sig).returns_twice,
                    _ => false,
                }
        })
}
fn outline(f: &mut Function, block: Block, name: String) -> Option<Function> {
    let instructions = f.block_insts(block).to_vec();
    let mut defined = FxHashSet::default();
    for &i in &instructions {
        defined.extend(f.inst_results(i).iter().copied());
    }
    // No definition can leave a terminal block, even through an alias or
    // unreachable user. Keep its stack objects independent of the caller.
    if f.layout_insts().any(|(b, i)| {
        let mut outside = false;
        if b != block {
            f.inst_data(i)
                .for_each_operand(|v| outside |= defined.contains(&f.resolve(v)));
        }
        outside
    }) {
        return None;
    }
    // A tail call releases the caller frame. Never capture a pointer into
    // another caller-owned object or across an earlier opaque call.
    if f.layout_insts().any(|(b, i)| {
        b != block
            && matches!(
                f.inst_data(i),
                InstData::StackAddr { .. } | InstData::Call { .. } | InstData::CallIndirect { .. }
            )
    }) {
        return None;
    }
    let mut external = Vec::new();
    for &i in &instructions {
        f.inst_data(i).for_each_operand(|v| {
            let v = f.resolve(v);
            if !defined.contains(&v) && !external.contains(&v) {
                external.push(v);
            }
        });
    }
    let literal = |v: Value| match f.value_def(v) {
        ValueDef::Result(i, 0) => matches!(
            f.inst_data(i),
            InstData::Iconst { .. } | InstData::SymbolAddr { .. }
        ),
        _ => false,
    };
    let arguments: Vec<_> = external.iter().copied().filter(|&v| !literal(v)).collect();
    if arguments.len() > 6 || arguments.iter().any(|&v| !scalar(f.value_type(v))) {
        return None;
    }
    let mut helper = f.clone();
    helper.name = name;
    helper.local = true;
    helper.weak_odr = false;
    helper.attrs = FunctionAttrs {
        cold: true,
        noinline: true,
        noreturn: true,
        target_features: f.attrs.target_features.clone(),
        ..FunctionAttrs::default()
    };
    helper.sig.params = arguments
        .iter()
        .map(|&v| AbiParam::new(f.value_type(v)))
        .collect();
    let entry = helper
        .entry_block()
        .expect("outlined function has an entry");
    for i in helper.block_insts(entry).to_vec() {
        helper.remove_inst(i);
    }
    while !helper.block_params(entry).is_empty() {
        helper.remove_block_param(entry, 0);
    }
    for &i in &instructions {
        helper.move_inst(i, InsertPoint::End(entry));
    }
    let removed: Vec<_> = helper.blocks().filter(|&b| b != entry).collect();
    helper.remove_blocks(&removed);
    for &v in &external {
        let replacement = if literal(v) {
            let ValueDef::Result(i, 0) = f.value_def(v) else {
                unreachable!("literal definition")
            };
            let i =
                helper.insert_inst(InsertPoint::Before(instructions[0]), f.inst_data(i).clone());
            helper.first_result(i)
        } else {
            helper.append_block_param(entry, f.value_type(v))
        };
        helper.replace_all_uses(v, replacement);
    }
    // Parameters follow argument order, independently of interspersed literals.
    let callee = f.import_function(&helper.name, helper.sig.clone());
    f.set_callee_attrs(callee, helper.attrs.clone());
    for i in instructions {
        f.remove_inst(i);
    }
    f.append_inst(
        block,
        InstData::ReturnCall {
            func: callee,
            args: CallArgs::from(arguments),
        },
    );
    helper.compact_callees();
    Some(helper)
}
