//! Freezes templates and call sites, orders the sites and applies budgets.
use super::clone::{inline_call, two_functions};
use super::cost::{
    CHEAP_SITE_GROWTH, CallerLedger, MAX_MODULE_GROWTH, MAX_SCC_CALLER_INSTRUCTIONS, ModuleLedger,
    SiteCost, Verdict, budget_verdict, charge, cheap_helper, inline_cost, profitability_reason,
    small_scc_wrapper, tiny_priority_leaf,
};
use super::eligibility::eligibility_reason;
use super::{
    InlineBudget, InlineCost, InlinePolicy, InlineReport, InlineSkip, recursive_definitions,
};
use crate::InlineSite;
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{Function, InstData, Module};

pub(super) fn run_observed(
    module: &mut Module,
    sites: Option<&mut Vec<InlineSite>>,
) -> InlineReport {
    run_observed_policy(module, sites, false, false)
}

fn run_observed_policy(
    module: &mut Module,
    sites: Option<&mut Vec<InlineSite>>,
    admit_noninterposable: bool,
    cheap_reserve: bool,
) -> InlineReport {
    run_observed_policy_scoped(
        module,
        sites,
        InlinePolicy {
            admit_noninterposable,
            cheap_reserve,
            cost: InlineCost::Default,
        },
        None,
    )
}

pub(super) fn run_observed_policy_scoped(
    module: &mut Module,
    sites: Option<&mut Vec<InlineSite>>,
    policy: InlinePolicy,
    callers: Option<&FxHashSet<String>>,
) -> InlineReport {
    let mut budget = if policy.cheap_reserve {
        InlineBudget::new(module)
    } else {
        InlineBudget::default()
    };
    run_observed_policy_budgeted(module, sites, policy, callers, &mut budget)
}

pub(super) fn run_observed_policy_budgeted(
    module: &mut Module,
    mut sites: Option<&mut Vec<InlineSite>>,
    policy: InlinePolicy,
    callers: Option<&FxHashSet<String>>,
    shared: &mut InlineBudget,
) -> InlineReport {
    let InlinePolicy {
        admit_noninterposable,
        cheap_reserve,
        cost: cost_policy,
    } = policy;
    let flatten = module.functions.iter().any(|f| f.attrs.flatten);
    let roots = if cost_policy == InlineCost::Size {
        crate::assembly_symbols::roots(module)
    } else {
        FxHashSet::default()
    };
    let mut calls = FxHashMap::<String, usize>::default();
    if cost_policy == InlineCost::Size {
        for function in &module.functions {
            for (_, instruction) in function.layout_insts() {
                let reference = match function.inst_data(instruction) {
                    InstData::Call { func, .. }
                    | InstData::Invoke { func, .. }
                    | InstData::ReturnCall { func, .. } => *func,
                    _ => continue,
                };
                *calls
                    .entry(function.callee(reference).name.clone())
                    .or_default() += 1;
            }
        }
    }
    // Freeze original sites and caller-size decisions before either phase.
    // The ledgers span both phases: priority changes order, never capacity.
    let original_sites: Vec<Vec<_>> = module
        .functions
        .iter()
        .map(|caller| {
            caller
                .layout_insts()
                .filter_map(|(_, i)| {
                    matches!(
                        caller.inst_data(i),
                        InstData::Call { .. }
                            | InstData::Invoke { .. }
                            | InstData::ReturnCall { .. }
                    )
                    .then_some(i)
                })
                .collect()
        })
        .collect();
    let recursive = recursive_definitions(module, &original_sites);
    // Use the entire structural candidate set for edge checks, including
    // candidates outside an SCC or without legal binding. Binding changes must
    // not turn a formerly opaque small wrapper into a second expansion level.
    let wrapper_bodies: FxHashSet<_> = module
        .functions
        .iter()
        .filter(|f| admit_noninterposable && small_scc_wrapper(f))
        .map(|f| f.name.clone())
        .collect();
    let recursive_bodies: FxHashSet<_> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(i, f)| recursive[*i] && wrapper_bodies.contains(&f.name))
        .map(|(_, f)| f.name.clone())
        .collect();
    let wrappers: FxHashSet<_> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(i, f)| {
            recursive[*i]
                && wrapper_bodies.contains(&f.name)
                && eligibility_reason(f, true).is_none()
                && !f.layout_insts().any(|(_, inst)| match f.inst_data(inst) {
                    InstData::Call { func, .. } => wrapper_bodies.contains(&f.callee(*func).name),
                    _ => false,
                })
        })
        .map(|(_, f)| f.name.clone())
        .collect();
    let explicit_definitions = sites.as_ref().map(|_| {
        module
            .functions
            .iter()
            .filter(|f| f.attrs.alwaysinline)
            .map(|f| f.name.clone())
            .collect::<FxHashSet<_>>()
    });
    // Freeze both templates and their rejection decisions at pass entry.
    // Keep the existing diagnostic precedence (noinline, recursion, legality,
    // profitability); the event stream uses these decisions, not a later probe.
    let mut templates = FxHashMap::default();
    let mut rejected = FxHashMap::default();
    for (index, f) in module.functions.iter().enumerate() {
        let legality = eligibility_reason(f, admit_noninterposable);
        let profitability = if legality.is_none() && !flatten {
            profitability_reason(f, admit_noninterposable)
        } else {
            None
        };
        let recursion = !f.attrs.noinline && recursive[index];
        let reason = if f.attrs.noinline {
            Some("noinline")
        } else if recursion && !wrappers.contains(&f.name) {
            Some("recursive_callee")
        } else {
            legality.or(profitability)
        };
        if let Some(reason) = reason {
            rejected.insert(f.name.clone(), reason);
        } else {
            templates.insert(f.name.clone(), index);
        }
    }
    // A template is copied only just before its first change as a caller;
    // until then the module's own definition is the pass-entry body.
    let mut frozen: Vec<Option<Box<Function>>> = Vec::new();
    frozen.resize_with(module.functions.len(), || None);
    // Templates retain their pass-entry bodies even when used as callers.
    // Only their immutable cost is cached; caller ledgers stay live below.
    let mut template_costs = vec![None; module.functions.len()];
    let mut report = InlineReport::default();
    let tiny_templates: FxHashSet<_> = templates
        .values()
        .map(|&index| &module.functions[index])
        .filter(|f| admit_noninterposable && tiny_priority_leaf(f))
        .map(|f| f.name.as_str())
        .collect();
    let priority_sites: Vec<FxHashSet<_>> = module
        .functions
        .iter()
        .zip(&original_sites)
        .map(|(caller, calls)| {
            // Only a tiny template can be a priority site.
            if tiny_templates.is_empty() {
                return FxHashSet::default();
            }
            calls
                .iter()
                .copied()
                .filter(|&call| {
                    let InstData::Call { func, .. } = caller.inst_data(call) else {
                        return false;
                    };
                    let decl = caller.callee(*func);
                    admit_noninterposable
                        && templates.get(&decl.name).is_some_and(|&index| {
                            let f = &module.functions[index];
                            (decl.attrs.alwaysinline
                                || f.attrs.alwaysinline
                                || (cheap_reserve
                                    && (decl.attrs.inlinehint || f.attrs.inlinehint)
                                    && f.blocks().count() > 1
                                    && f.layout_insts().any(|(_, i)| {
                                        matches!(f.inst_data(i), InstData::Store { .. })
                                    })))
                                && tiny_templates.contains(f.name.as_str())
                        })
                })
                .collect()
        })
        .collect();
    let large_scc_callers: Vec<_> = module
        .functions
        .iter()
        .map(|caller| {
            !wrappers.is_empty() && caller.layout_insts().count() > MAX_SCC_CALLER_INSTRUCTIONS
        })
        .collect();
    let mut ledgers = vec![CallerLedger::default(); module.functions.len()];
    // Preserve the ordinary two phases exactly. Only sites denied solely by
    // module growth enter the separate retry queue; cloned calls never enter it.
    let mut schedule = Vec::new();
    for phase in 0..if admit_noninterposable { 2 } else { 1 } {
        let mut queues: std::collections::VecDeque<_> = original_sites
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                callers.is_none_or(|names| names.contains(&module.functions[*index].name))
            })
            .map(|(index, calls)| {
                (
                    index,
                    calls
                        .iter()
                        .copied()
                        .filter(|call| {
                            !admit_noninterposable
                                || priority_sites[index].contains(call) == (phase == 0)
                        })
                        .collect::<std::collections::VecDeque<_>>(),
                )
            })
            .filter(|(_, calls)| !calls.is_empty())
            .collect();
        while let Some((index, mut calls)) = queues.pop_front() {
            if cheap_reserve {
                schedule.push((
                    index,
                    calls.pop_front().expect("nonempty caller queue"),
                    false,
                ));
            } else {
                schedule.extend(calls.drain(..).map(|call| (index, call, false)));
            }
            if !calls.is_empty() {
                queues.push_back((index, calls));
            }
        }
    }
    let ordinary_end = schedule.len();
    let mut deferred = vec![Vec::new(); module.functions.len()];
    let mut module_ledger = if cheap_reserve {
        ModuleLedger {
            growth: shared.used,
            limit: shared.limit,
            reserve_growth: shared.reserve_used,
            reserve_sites: shared.reserve_sites,
        }
    } else {
        ModuleLedger {
            growth: 0,
            limit: MAX_MODULE_GROWTH,
            reserve_growth: 0,
            reserve_sites: 0,
        }
    };
    let mut cursor = 0;
    loop {
        if cheap_reserve && cursor == ordinary_end {
            // One site per caller per sweep, so a high-fanout early caller
            // cannot spend the entire reserve ahead of late callers.
            let mut callers: std::collections::VecDeque<_> = deferred
                .iter_mut()
                .enumerate()
                .filter(|(_, calls)| !calls.is_empty())
                .map(|(index, calls)| (index, std::mem::take(calls).into_iter()))
                .collect();
            while let Some((index, mut calls)) = callers.pop_front() {
                schedule.push((index, calls.next().unwrap(), true));
                if calls.len() != 0 {
                    callers.push_back((index, calls));
                }
            }
        }
        if cursor == schedule.len() {
            break;
        }
        let (index, call, retry) = schedule[cursor];
        cursor += 1;
        let caller = &module.functions[index];
        let large_scc_caller = large_scc_callers[index];
        let reference = match caller.inst_data(call) {
            InstData::Call { func, .. }
            | InstData::Invoke { func, .. }
            | InstData::ReturnCall { func, .. } => *func,
            _ => unreachable!(),
        };
        let decl = caller.callee(reference);
        let requested = decl.attrs.alwaysinline
            || templates
                .get(&decl.name)
                .is_some_and(|&t| template(&frozen, &module.functions, t).attrs.alwaysinline);
        let reason = if caller.sig.returns_twice || caller.personality.is_some() {
            Some((
                "unsupported caller",
                if caller.sig.returns_twice {
                    "returns_twice_caller"
                } else {
                    "caller_personality"
                },
            ))
        } else if !matches!(caller.inst_data(call), InstData::Call { .. }) {
            Some((
                "unsupported call instruction",
                "unsupported_call_instruction",
            ))
        } else if caller.attrs.optnone
            || keel_ir::FunctionPolicy::resolve(&caller.attrs).is_ok_and(|p| {
                p.disabled()
                    || p.inline == Some(false)
                    || (!p.permits_inlining()
                        && !caller.attrs.flatten
                        && cost_policy != InlineCost::Size)
            })
        {
            Some(("caller optimization policy", "caller_optimization_policy"))
        } else if decl.attrs.noinline || decl.attrs.optnone {
            Some(("noinline policy", "noinline"))
        } else if recursive_bodies.contains(&caller.name) {
            Some(("frozen recursive wrapper body", "recursive_wrapper_body"))
        } else if large_scc_caller && wrappers.contains(&decl.name) {
            Some((
                "recursive wrapper caller size",
                "recursive_wrapper_caller_size",
            ))
        } else if let Some(&t) = templates.get(&decl.name) {
            let callee = template(&frozen, &module.functions, t);
            if decl.sig != callee.sig {
                Some(("ABI signature mismatch", "abi_signature_mismatch"))
            } else if !caller
                .attrs
                .target_features
                .permits_inlining(&callee.attrs.target_features)
            {
                Some(("target feature mismatch", "target_feature_mismatch"))
            } else if callee.name == caller.name {
                Some(("recursive callee", "recursive_callee"))
            } else if !requested && callee.attrs.cold && !caller.attrs.cold && !caller.attrs.flatten
            {
                Some(("cold callee", "cold_callee"))
            } else if cost_policy == InlineCost::Size && !requested && !caller.attrs.flatten && {
                let constant = match caller.inst_data(call) {
                    InstData::Call { args, .. } => args.iter().any(|&argument| {
                        match caller.value_def(caller.resolve(argument)) {
                            keel_ir::ValueDef::Result(instruction, _) => matches!(
                                caller.inst_data(instruction),
                                InstData::Iconst { .. } | InstData::Fconst { .. }
                            ),
                            _ => false,
                        }
                    }),
                    _ => false,
                };
                callee.layout_insts().take(129).count() > 128
                    || !(callee.attrs.inlinehint
                        || constant
                        || (callee.local
                            && calls.get(&callee.name) == Some(&1)
                            && !roots.contains(&callee.name)))
            } {
                Some(("size profitability", "size_profitability"))
            } else if !caller.attrs.flatten && flatten {
                profitability_reason(callee, admit_noninterposable)
                    .map(|r| ("profitability threshold", r))
            } else {
                None
            }
        } else {
            Some({
                let detail = rejected.get(&decl.name).copied().unwrap_or("no_definition");
                let legacy = match detail {
                    "noinline" => "noinline policy",
                    "recursive_callee" => "recursive callee",
                    "no_definition" => "no private definition",
                    r if r.starts_with("profitability_") => "profitability threshold",
                    _ => "unsupported callee or hard stack bound",
                };
                (legacy, detail)
            })
        };
        if let Some((reason, detail)) = reason {
            if let Some(sites) = sites.as_deref_mut() {
                sites.push(InlineSite::new(
                    caller,
                    &decl.name,
                    call,
                    requested
                        || explicit_definitions
                            .as_ref()
                            .is_some_and(|names| names.contains(&decl.name)),
                    detail,
                ));
            }
            if requested {
                report.skipped.push(InlineSkip {
                    caller: caller.name.clone(),
                    callee: decl.name.clone(),
                    site: call,
                    reason,
                });
            }
            continue;
        }
        let callee_index = templates[&decl.name];
        let callee = template(&frozen, &module.functions, callee_index);
        // Bound net layout growth before cleanup. The entry jump replaces
        // the removed call; each cloned return becomes one jump. The
        // continuation only receives moved instructions and parameters.
        // Counting all callee instructions also covers unreachable blocks
        // that the dominator traversal may omit. Snapshot IR is additional.
        let (cost, stack_cost) =
            *template_costs[callee_index].get_or_insert_with(|| inline_cost(callee));
        let InstData::Call { args, .. } = caller.inst_data(call) else {
            unreachable!("only direct calls reach the budget check")
        };
        let site = SiteCost {
            insts: cost,
            stack: stack_cost,
            operands: args.len() + caller.inst_results(call).len(),
            recursive_caller_stack: recursive[index].then(|| {
                caller
                    .stack_slots()
                    .map(|(_, s)| u64::from(s.size))
                    .sum::<u64>()
            }),
            cheap_helper: cheap_reserve
                && !wrappers.contains(&callee.name)
                && cheap_helper(callee, cost, stack_cost),
        };
        let use_cheap = match budget_verdict(&site, &ledgers[index], &module_ledger, retry) {
            Verdict::Inline { reserve } => reserve,
            Verdict::Defer => {
                deferred[index].push(call);
                continue;
            }
            Verdict::Reject(detail) => {
                if let Some(sites) = sites.as_deref_mut() {
                    sites.push(InlineSite::new(
                        caller,
                        &callee.name,
                        call,
                        requested
                            || explicit_definitions
                                .as_ref()
                                .is_some_and(|names| names.contains(&decl.name)),
                        detail,
                    ));
                }
                if requested {
                    report.skipped.push(InlineSkip {
                        caller: caller.name.clone(),
                        callee: callee.name.clone(),
                        site: call,
                        reason: "hard growth safeguard",
                    });
                }
                continue;
            }
        };
        if let Some(sites) = sites.as_deref_mut() {
            sites.push(InlineSite::new(
                caller,
                &callee.name,
                call,
                requested,
                if use_cheap {
                    "cheap_reserve_inlined"
                } else {
                    "inlined"
                },
            ));
        }
        let before = use_cheap.then(|| caller.layout_insts().count());
        if templates.get(&caller.name) == Some(&index) && frozen[index].is_none() {
            frozen[index] = Some(Box::new(caller.clone()));
        }
        // A recursive call was rejected above, so the callee is another function.
        let (caller, callee) = match frozen[callee_index].as_deref() {
            Some(callee) => (&mut module.functions[index], callee),
            None => two_functions(&mut module.functions, index, callee_index),
        };
        inline_call(caller, call, callee);
        if let Some(before) = before {
            // Fail compilation before codegen on an accounting violation.
            // No recoverable post-mutation fallback or artifact is allowed.
            let actual_net_growth = caller
                .layout_insts()
                .count()
                .checked_sub(before)
                .expect("reserve inlining unexpectedly shrank layout");
            assert!(
                actual_net_growth <= cost && cost <= CHEAP_SITE_GROWTH,
                "reserve emitted net growth exceeds conservative preflight"
            );
            if let Some(site) = sites.as_deref_mut().and_then(|s| s.last_mut()) {
                site.experimental_net_growth = Some(actual_net_growth);
            }
        }
        charge(&site, &mut ledgers[index], &mut module_ledger, use_cheap);
        report.changed = true;
        report.callers.push(index);
    }
    if cheap_reserve {
        let budget = shared;
        budget.used = module_ledger.growth;
        budget.reserve_used = module_ledger.reserve_growth;
        budget.reserve_sites = module_ledger.reserve_sites;
    }
    report
}

/// The pass-entry body of template `index`.
fn template<'a>(
    frozen: &'a [Option<Box<Function>>],
    functions: &'a [Function],
    index: usize,
) -> &'a Function {
    frozen[index].as_deref().unwrap_or(&functions[index])
}
