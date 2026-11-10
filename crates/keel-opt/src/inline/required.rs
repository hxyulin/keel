//! Mandatory inlining of C always_inline calls and inline-asm finalization.
use super::clone::{inline_call, two_functions};
use super::eligibility::{FrameSemantics, body_eligibility_reason, eligibility_reason_with_frames};
use super::recursive_definitions;
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{Function, Inst, InstData, Module, Symbol};

/// Expand mandatory C inline calls before validating deferred asm immediates.
/// This phase does not use optional optimizer profitability budgets. Hard
/// legality and module growth limits still produce explicit diagnostics.
/// C frame/stack observers bind to the emitted caller frame after expansion.
/// Ordinary IR inlining keeps its frame-preservation restrictions.
pub fn required_inline_asm(module: &mut Module) -> Result<(), String> {
    required_inline_asm_with_c_bodies(module, &[])
}

/// Finalize C asm after mandatory inlining, discarding owned GNU extern-inline
/// implementations while retaining their external ABI declarations.
/// The frontend supplies local strong bodies; ordinary IR callers use
/// [`required_inline_asm`].
///
/// # Errors
/// Returns an error when mandatory inlining is illegal, an owned body is not
/// local and strong, or immediate constraints cannot be finalized.
pub fn required_inline_asm_with_c_bodies(
    module: &mut Module,
    inline_only: &[&str],
) -> Result<(), String> {
    required_inline_asm_with_c_definitions(module, inline_only, &[])
}

/// Finalize mandatory C inlining with explicit ownership of strong public bodies.
/// Direct mandatory calls may use these definitions without changing their
/// public symbols or semantic-interposition attributes. Inline-only GNU bodies
/// retain the separate discard contract of [`required_inline_asm_with_c_bodies`].
///
/// # Errors
/// Returns an error for a missing, local, weak or non-mandatory owned definition,
/// an illegal mandatory call, or an invalid inline-only body.
pub fn required_inline_asm_with_c_definitions(
    module: &mut Module,
    inline_only: &[&str],
    definitions: &[&str],
) -> Result<(), String> {
    for name in definitions {
        let Some(function) = module.functions.iter().find(|f| f.name == *name) else {
            return Err(format!("C-owned mandatory definition @{name} is missing"));
        };
        if function.local
            || function.attrs.weak
            || function.weak_odr
            || !function.attrs.alwaysinline
            || inline_only.contains(name)
        {
            return Err(format!(
                "C-owned mandatory definition @{name} is not a strong public alwaysinline body"
            ));
        }
    }
    discard_unused(module);
    // Only mandatory edges drive expansion. Ordinary calls may close a cycle
    // through an alwaysinline wrapper without requiring infinite expansion.
    let mandatory_definitions: FxHashSet<&str> = module
        .functions
        .iter()
        .filter(|f| f.attrs.alwaysinline)
        .map(|f| f.name.as_str())
        .collect();
    let sites: Vec<Vec<Inst>> = module
        .functions
        .iter()
        .map(|f| {
            f.layout_insts()
                .filter_map(|(_, i)| {
                    let reference = match f.inst_data(i) {
                        InstData::Call { func, .. }
                        | InstData::ReturnCall { func, .. }
                        | InstData::Invoke { func, .. } => *func,
                        _ => return None,
                    };
                    let decl = f.callee(reference);
                    (decl.attrs.alwaysinline || mandatory_definitions.contains(decl.name.as_str()))
                        .then_some(i)
                })
                .collect()
        })
        .collect();
    let (recursive, _) = recursive_definitions(module, &sites);
    let recursive: FxHashSet<String> = module
        .functions
        .iter()
        .zip(recursive)
        .filter(|(_, r)| *r)
        .map(|(f, _)| f.name.clone())
        .collect();
    if sites.iter().any(|sites| !sites.is_empty()) {
        expand_required_with_fallback(module, definitions, &recursive, 1_000_000)?;
    }
    discard_inline_only(module, inline_only)?;
    discard_unused(module);
    // Constant cleanup is required for constraint legality, independently of -O.
    for f in &mut module.functions {
        finalize_asm_operands(f);
    }
    Ok(())
}

/// Removes the local alwaysinline bodies nothing references, keeping a
/// declaration for each.
fn discard_unused(module: &mut Module) {
    let mut live: FxHashSet<String> = module
        .functions
        .iter()
        .filter(|f| {
            !f.local
                || !f.attrs.alwaysinline
                || f.attrs.used
                || f.attrs.retain
                || f.attrs.constructor
                || f.attrs.destructor
        })
        .map(|f| f.name.clone())
        .collect();
    for d in &module.data {
        if let Some(init) = &d.init {
            for item in &init.items {
                if let keel_ir::DataItem::Ptr { symbol, .. }
                | keel_ir::DataItem::RttiName { symbol } = item
                {
                    live.insert(symbol.clone());
                }
            }
        }
    }
    loop {
        let before = live.len();
        for f in &module.functions {
            if !live.contains(&f.name) {
                continue;
            }
            for (_, i) in f.layout_insts() {
                match f.inst_data(i) {
                    InstData::Call { func, .. }
                    | InstData::ReturnCall { func, .. }
                    | InstData::Invoke { func, .. } => {
                        live.insert(f.callee(*func).name.clone());
                    }
                    InstData::SymbolAddr {
                        symbol: Symbol::Func(r),
                    } => {
                        live.insert(f.callee(*r).name.clone());
                    }
                    _ => {}
                }
            }
        }
        if live.len() == before {
            break;
        }
    }
    for f in &module.functions {
        if !live.contains(&f.name) {
            let mut attrs = f.attrs.clone();
            // Placement belongs to an emitted definition, not the ABI
            // declaration retained after its unused body is discarded.
            attrs.section = None;
            if let Some(decl) = module.decls.iter_mut().find(|d| d.name == f.name) {
                decl.attrs.section = None;
            } else {
                module.decls.push(keel_ir::FuncDecl {
                    name: f.name.clone(),
                    sig: f.sig.clone(),
                    attrs,
                });
            }
        }
    }
    module.functions.retain(|f| live.contains(&f.name));
    // Removed calls leave callee imports behind. Refresh their contracts
    // from the declarations that now own the discarded bodies' names.
    module.sync_function_attrs();
}

/// Drops GNU inline-only bodies after expansion and keeps their external
/// ABI declarations.
fn discard_inline_only(module: &mut Module, inline_only: &[&str]) -> Result<(), String> {
    if inline_only.is_empty() {
        return Ok(());
    }
    for name in inline_only {
        if let Some(function) = module.functions.iter().find(|f| f.name == *name) {
            if !function.local || function.attrs.weak || function.weak_odr {
                return Err(format!(
                    "GNU inline implementation @{name} is not a local strong body"
                ));
            }
            if !module.decls.iter().any(|decl| decl.name == *name) {
                module.decls.push(keel_ir::FuncDecl {
                    name: (*name).to_owned(),
                    sig: function.sig.clone(),
                    attrs: function.attrs.clone(),
                });
            }
        }
    }
    module
        .functions
        .retain(|function| !inline_only.contains(&function.name.as_str()));
    for decl in &mut module.decls {
        if inline_only.contains(&decl.name.as_str()) {
            decl.attrs.alwaysinline = false;
            decl.attrs.section = None;
        }
    }
    module.sync_function_attrs();
    Ok(())
}

/// Folds the constants an inline asm statement's immediate operands need.
fn finalize_asm_operands(f: &mut Function) {
    if !f
        .layout_insts()
        .any(|(_, i)| matches!(f.inst_data(i), InstData::InlineAsm { .. }))
    {
        return;
    }
    // Width-selecting C macros can contain invalid asm in constant-dead
    // branches even when every operand is already a legal register or
    // memory operand. Use the CFG so incoming goto edges stay reachable.
    for _ in 0..8 {
        let mut changed = crate::phi_simplify::run(f);
        changed |= crate::const_fold::run(f);
        changed |= crate::simplify_cfg::run(f);
        if !changed {
            break;
        }
    }
    if !has_unresolved_immediate(f) {
        return;
    }
    for _ in 0..8 {
        let mut changed = crate::phi_simplify::run(f);
        changed |= crate::gvn::run(f);
        changed |= crate::memory_forward::run(f);
        if !f.debug_vars.is_empty() {
            changed |= crate::memory_forward::finalize_asm_immediates(f);
        }
        changed |= crate::const_fold::run(f);
        changed |= crate::instcombine::run(f);
        changed |= crate::simplify_cfg::run(f);
        if !changed {
            break;
        }
    }
}

/// Whether an inline asm immediate operand is still neither an integer
/// constant nor, for `AsmOperand::Imm`, a symbolic address.
fn has_unresolved_immediate(f: &Function) -> bool {
    use keel_ir::AsmOperand;
    f.layout_insts().any(|(_, i)| match f.inst_data(i) {
        InstData::InlineAsm { asm, args } => asm.inputs.iter().zip(args).any(|(op, arg)| {
            if !matches!(
                op,
                AsmOperand::Imm
                    | AsmOperand::NumericImm
                    | AsmOperand::RegImm
                    | AsmOperand::RegSignedImm32
                    | AsmOperand::RegShiftImm5
                    | AsmOperand::PortImm8
            ) {
                return false;
            }
            let numeric = matches!(
                f.value_def(f.resolve(*arg)),
                keel_ir::ValueDef::Result(def, _)
                    if matches!(f.inst_data(def), InstData::Iconst { .. })
            );
            !numeric && !(*op == AsmOperand::Imm && keel_ir::symbolic_immediate(f, *arg).is_some())
        }),
        _ => false,
    })
}

#[derive(Clone, Copy)]
enum RequiredCleanup {
    PerCall,
    PerCaller,
}

fn expand_required_with_fallback(
    module: &mut Module,
    definitions: &[&str],
    recursive: &FxHashSet<String>,
    limit: usize,
) -> Result<(), String> {
    // Inlining mutates only function bodies and their local imports. Keep the
    // original templates and budget accounting for an eager retry: deferred
    // cleanup can leave an invalid call in code that eager cleanup removes.
    let original = module.functions.clone();
    if expand_required_calls(
        module,
        definitions,
        recursive,
        limit,
        RequiredCleanup::PerCaller,
    )
    .is_err()
    {
        module.functions = original;
        return expand_required_calls(
            module,
            definitions,
            recursive,
            limit,
            RequiredCleanup::PerCall,
        );
    }
    Ok(())
}

fn expand_required_calls(
    module: &mut Module,
    definitions: &[&str],
    recursive: &FxHashSet<String>,
    limit: usize,
    cleanup: RequiredCleanup,
) -> Result<(), String> {
    let mut expanded = 0usize;
    // Expansion edits only the caller, so definitions keep their indices and
    // the functions before the caller have no mandatory sites left.
    let mut index_of: FxHashMap<String, usize> = FxHashMap::default();
    for (index, function) in module.functions.iter().enumerate() {
        index_of.entry(function.name.clone()).or_insert(index);
    }
    for first in 0..module.functions.len() {
        let mut dirty = false;
        // Layout blocks before `resume` have no mandatory sites left.
        // Inlining appends the continuation and callee blocks to the layout
        // and leaves earlier blocks without sites, so the next site is never
        // before the block after the call's. Cleanup can reorder blocks, so
        // it rescans from the entry.
        let mut resume = 0usize;
        loop {
            let mut next = None;
            let caller = &module.functions[first];
            let sites = caller
                .blocks()
                .enumerate()
                .skip(resume)
                .flat_map(|(index, block)| {
                    caller
                        .block_insts(block)
                        .iter()
                        .map(move |&call| (index, call))
                });
            for (index, call) in sites {
                let reference = match caller.inst_data(call) {
                    InstData::Call { func, .. }
                    | InstData::ReturnCall { func, .. }
                    | InstData::Invoke { func, .. } => *func,
                    _ => continue,
                };
                let decl = caller.callee(reference);
                let definition = index_of.get(decl.name.as_str()).copied();
                if !decl.attrs.alwaysinline
                    && !definition.is_some_and(|i| module.functions[i].attrs.alwaysinline)
                {
                    continue;
                }
                let fail = |why: &str| {
                    format!(
                        "required inlining of @{} in @{} failed: {why}",
                        decl.name, caller.name
                    )
                };
                if !matches!(caller.inst_data(call), InstData::Call { .. }) {
                    return Err(fail("unsupported call instruction"));
                }
                let target = definition.ok_or_else(|| fail("no definition"))?;
                let callee = &module.functions[target];
                if recursive.contains(&callee.name) {
                    return Err(fail("recursive callee"));
                }
                if caller.sig.returns_twice || caller.personality.is_some() {
                    return Err(fail("unsupported caller"));
                }
                if decl.attrs.noinline {
                    return Err(fail("noinline conflicts with always_inline"));
                }
                if caller.attrs.fast_math != callee.attrs.fast_math {
                    return Err(fail("floating math mode mismatch"));
                }
                if decl.sig != callee.sig {
                    return Err(fail("ABI signature mismatch"));
                }
                // Unannotated mandatory wrappers are compiled in the caller's
                // permission context. Every copied operation still passes the
                // backend's local legality checks. Explicit callee permissions
                // cannot be erased this way.
                if !callee.attrs.target_features.is_empty()
                    && !caller
                        .attrs
                        .target_features
                        .permits_inlining(&callee.attrs.target_features)
                {
                    return Err(fail("target feature mismatch"));
                }
                let reason = if definitions.contains(&callee.name.as_str()) {
                    body_eligibility_reason(callee, FrameSemantics::CRequiredInline)
                } else {
                    eligibility_reason_with_frames(callee, true, FrameSemantics::CRequiredInline)
                };
                if let Some(reason) = reason {
                    return Err(fail(reason));
                }
                expanded = expanded
                    .checked_add(callee.layout_insts().count())
                    .ok_or_else(|| fail("growth overflow"))?;
                if expanded > limit {
                    return Err(fail(
                        "required inline growth exceeds one million instructions",
                    ));
                }
                next = Some((index, call, target));
                break;
            }
            let Some((index, call, target)) = next else {
                if dirty {
                    crate::phi_simplify::run(&mut module.functions[first]);
                    crate::simplify_cfg::run(&mut module.functions[first]);
                    dirty = false;
                    resume = 0;
                    // Cleanup can expose or remove sites. Finish this caller
                    // before its body can become a later caller's template.
                    continue;
                }
                break;
            };
            let (caller, callee) = two_functions(&mut module.functions, first, target);
            inline_call(caller, call, callee);
            match cleanup {
                RequiredCleanup::PerCall => {
                    crate::phi_simplify::run(caller);
                    crate::simplify_cfg::run(caller);
                }
                RequiredCleanup::PerCaller => {
                    dirty = true;
                    resume = index + 1;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    fn gated_call(target: &str) -> Module {
        keel_ir::parse(&format!(
            "decl @missing() alwaysinline\n\
             func @caller() {{\nblock0:\n v0 = call @gate()\n brif v0, block1, block2\n\
             block1:\n call @{target}()\n return\nblock2:\n return\n}}\n\
             func @gate() -> i8 alwaysinline local {{\nblock0:\n v0 = iconst.i8 0\n return v0\n}}\n\
             func @heavy() alwaysinline local {{\nblock0:\n v0 = iconst.i64 0\n v1 = iconst.i64 1\n return\n}}"
        ))
        .unwrap()
    }

    #[test]
    fn deferred_required_cleanup_retries_calls_that_become_dead() {
        let mut module = gated_call("missing");
        keel_ir::verify_module(&module).unwrap();
        let mut eager = module.clone();
        let recursive = FxHashSet::default();
        assert!(
            expand_required_calls(
                &mut module.clone(),
                &[],
                &recursive,
                1_000_000,
                RequiredCleanup::PerCaller
            )
            .is_err(),
            "the deferred scan still sees the dead missing definition"
        );
        expand_required_calls(
            &mut eager,
            &[],
            &recursive,
            1_000_000,
            RequiredCleanup::PerCall,
        )
        .unwrap();
        expand_required_with_fallback(&mut module, &[], &recursive, 1_000_000).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(module.to_string(), eager.to_string());
    }

    #[test]
    fn deferred_required_cleanup_retries_calls_after_a_trapping_wrapper() {
        let mut module = keel_ir::parse(
            "decl @missing() alwaysinline\n\
             func @caller() {\nblock0:\n call @stop()\n call @missing()\n return\n}\n\
             func @stop() alwaysinline local {\nblock0:\n trap\n}",
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut eager = module.clone();
        let recursive = FxHashSet::default();
        expand_required_calls(
            &mut eager,
            &[],
            &recursive,
            1_000_000,
            RequiredCleanup::PerCall,
        )
        .unwrap();
        expand_required_with_fallback(&mut module, &[], &recursive, 1_000_000).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(module.to_string(), eager.to_string());
    }

    #[test]
    fn deferred_required_cleanup_restores_growth_accounting_on_retry() {
        let mut module = gated_call("heavy");
        let recursive = FxHashSet::default();
        let mut eager = module.clone();
        // The gate costs two instructions; the dead heavy call must not
        // consume the remaining budget when the original algorithm retries.
        assert!(
            expand_required_calls(
                &mut module.clone(),
                &[],
                &recursive,
                2,
                RequiredCleanup::PerCaller
            )
            .is_err()
        );
        expand_required_calls(&mut eager, &[], &recursive, 2, RequiredCleanup::PerCall).unwrap();
        expand_required_with_fallback(&mut module, &[], &recursive, 2).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(module.to_string(), eager.to_string());

        let mut eager = gated_call("heavy");
        let mut retried = eager.clone();
        let error = expand_required_calls(&mut eager, &[], &recursive, 1, RequiredCleanup::PerCall)
            .unwrap_err();
        assert_eq!(
            expand_required_with_fallback(&mut retried, &[], &recursive, 1).unwrap_err(),
            error
        );
        assert_eq!(retried.to_string(), eager.to_string());
    }

    #[test]
    fn deferred_required_cleanup_finishes_templates_before_later_callers() {
        let mut module = keel_ir::parse(
            "func @outer() -> i64 alwaysinline local {\nblock0:\n v0 = call @inner()\n return v0\n}\n\
             func @caller() -> i64 {\nblock0:\n v0 = call @outer()\n return v0\n}\n\
             func @inner() -> i64 alwaysinline local {\nblock0:\n v0 = iconst.i64 7\n return v0\n}"
        ).unwrap();
        expand_required_calls(
            &mut module,
            &[],
            &FxHashSet::default(),
            1_000_000,
            RequiredCleanup::PerCaller,
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        for f in &module.functions {
            assert_eq!(
                f.blocks().count(),
                1,
                "@{} retains unmerged inline blocks",
                f.name
            );
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. })),
                "@{} retains a required call",
                f.name
            );
        }
    }
}
