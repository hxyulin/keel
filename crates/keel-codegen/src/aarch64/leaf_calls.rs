//! Narrow, module-local preservation proofs for finalized integer leaves.
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
            is_call(func, data)
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
    // Bound buffered code and preparation work. Large functions retain ABI
    // clobbers; none of these cutoffs affect semantic eligibility elsewhere.
    let mut budget = 65_536usize;
    for (index, original) in module.functions.iter().enumerate() {
        if !called.contains(original.name.as_str()) || !eligible(original, module) {
            continue;
        }
        let count = original.layout_insts().take(129).count();
        if count > 128 || count > budget || original.blocks().count() > 16 {
            continue;
        }
        budget -= count;
        #[cfg(feature = "backend-profile")]
        let _function_profile = crate::backend_profile::function(&original.name);
        let func = vector::lower(original);
        let compiler =
            FunctionCompiler::new(&func, module, pools, pipeline, structured, &summaries)?;
        // finish/validate have already accounted for allocation edits. Count
        // reads as well as writes so no unobserved transfer can create a proof.
        let used = compiler.alloc.mentioned_gprs();
        let preserved = REGISTERS
            .caller_saved
            .iter()
            .fold(0, |mask, &r| mask | (1u64 << r))
            & !used;
        let mut output = Output::new(structured);
        let marks = compiler.compile(&mut output);
        summaries.insert(original.name.clone(), (original.sig.clone(), preserved));
        prepared.insert(index, (output, marks));
    }
    Ok((summaries, prepared))
}
