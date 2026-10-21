//! Bounded, module-local preservation proofs for finalized integer call chains.
//!
//! x9-x15 have no implicit uses in integer lowering: every use belongs to the
//! allocation (including sparse transfers). ABI shuffles and helpers use the
//! separately reserved x0-x8/x16/x17. Floating-point preservation is unchanged.
use super::*;
use std::collections::{HashMap, HashSet};

pub(super) type Summaries = HashMap<String, (Signature, u64)>;
pub(super) type Prepared = HashMap<usize, (Output, Vec<(String, Option<DebugScope>)>)>;

pub(super) fn eligible(func: &Function, module: &Module) -> bool {
    func.local
        && !func.weak_odr
        && func.sig.cc == CallConv::Fast
        && !func.sig.variadic
        && !func.sig.returns_twice
        && func.sig.params.len() <= 8
        && func.sig.returns.len() <= 1
        && module.files.is_empty()
        && func.personality.is_none()
        && func.num_stack_slots() == 0
        && func.sig.params.iter().chain(&func.sig.returns).all(|p| p.purpose == Purpose::Normal)
        && (0..func.num_values()).all(|i| matches!(func.value_type(Value::new(i)), Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr))
        && !func.layout_insts().any(|(_, i)| {
            let data = func.inst_data(i);
            (is_call(func, data) && !matches!(data, InstData::Call { .. }))
                || matches!(data, InstData::Intrinsic { .. } | InstData::InlineAsm { .. } | InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. } | InstData::LandingPad { .. })
                || matches!(data, InstData::SymbolAddr { symbol } if module.data_object(func.symbol_name(*symbol)).is_some_and(|d| d.thread_local))
        })
}

pub(super) fn prepare(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
    structured: bool,
) -> Result<(Summaries, Prepared), CodegenError> {
    let mut summaries = Summaries::new();
    let mut prepared = Prepared::new();
    if !module.files.is_empty()
        || !module
            .functions
            .iter()
            .any(|f| f.local && f.sig.cc == CallConv::Fast)
    {
        return Ok((summaries, prepared));
    }
    let called: HashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.layout_insts().filter_map(|(_, i)| {
                let InstData::Call { func: callee, .. } = f.inst_data(i) else {
                    return None;
                };
                let decl = f.callee(*callee);
                (decl.sig.cc == CallConv::Fast).then_some(decl.name.as_str())
            })
        })
        .collect();
    // Work is bounded independently of call-graph shape. Each round sees only
    // summaries finalized in earlier rounds, so source order cannot affect the
    // depth limit. Cycles (and their callers) never become ready. A chain with
    // an unknown, mismatched, or ineligible callee also retains ABI clobbers.
    const MAX_FUNCTIONS: usize = 512;
    const MAX_DEPTH: usize = 16;
    const MAX_INSTRUCTIONS: usize = 65_536;
    const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
    const MAX_OUTPUT_ITEMS: usize = 65_536;
    let mut budget = MAX_INSTRUCTIONS;
    let mut pending = Vec::new();
    for (index, original) in module.functions.iter().enumerate() {
        let count = original.layout_insts().take(129).count();
        if !called.contains(original.name.as_str())
            || count > 128
            || count > budget
            || original.blocks().take(17).count() > 16
            || !eligible(original, module)
        {
            continue;
        }
        if pending.len() == MAX_FUNCTIONS {
            break;
        }
        budget -= count;
        pending.push(index);
    }
    let caller_mask = REGISTERS
        .caller_saved
        .iter()
        .fold(0, |mask, &r| mask | (1u64 << r));
    let mut output_bytes = 0;
    let mut output_items = 0;
    for _ in 0..MAX_DEPTH {
        let mut ready = Vec::new();
        pending.retain(|&index| {
            let original = &module.functions[index];
            let mut nested_clobbers = 0;
            for (_, inst) in original.layout_insts() {
                if let InstData::Call { func: callee, .. } = original.inst_data(inst) {
                    let decl = original.callee(*callee);
                    let Some((sig, preserved)) = summaries.get(&decl.name) else {
                        return true;
                    };
                    if &decl.sig != sig {
                        return true;
                    }
                    nested_clobbers |= caller_mask & !preserved;
                }
            }
            ready.push((index, nested_clobbers));
            false
        });
        if ready.is_empty() {
            break;
        }
        for (index, nested_clobbers) in ready {
            let original = &module.functions[index];
            #[cfg(feature = "backend-profile")]
            let _function_profile = crate::backend_profile::function(&original.name);
            let func = vector::lower(original);
            let compiler =
                FunctionCompiler::new(&func, module, pools, pipeline, structured, &summaries)?;
            // finish/validate have accounted for defs AND uses, including
            // allocation transfers. Integer ABI shuffles and lowering scratch
            // use x0-x8/x16/x17, outside this proof's x9-x15 bank. Calls are the
            // only admitted implicit clobbers and are covered transitively.
            let used = compiler.alloc.mentioned_gprs() | nested_clobbers;
            let preserved = caller_mask & !used;
            let mut output = Output::new(structured);
            let marks = compiler.compile(&mut output);
            match &output {
                Output::Text(text) => output_bytes += text.len(),
                Output::Structured(program) => output_items += program.items.len(),
            }
            if output_bytes > MAX_OUTPUT_BYTES || output_items > MAX_OUTPUT_ITEMS {
                // Never publish a summary for a body that will be regenerated.
                return Ok((summaries, prepared));
            }
            summaries.insert(original.name.clone(), (original.sig.clone(), preserved));
            prepared.insert(index, (output, marks));
        }
    }
    Ok((summaries, prepared))
}
