//! Ordered function-local schedules with guarded module passes.
use crate::{
    OptError, OptimizationContext, OptimizationReport, OptimizationTier, OptimizeOptions, Pass,
};
use keel_ir::{Function, FunctionOptLevel, FunctionPolicy, Module};

/// Preserve pass behavior and ordering while limiting its mutation scope.
struct ScopedPass<'a> {
    pass: &'a dyn Pass,
    functions: keel_ir::fxhash::FxHashSet<String>,
}
impl Pass for ScopedPass<'_> {
    fn name(&self) -> &'static str {
        self.pass.name()
    }
    fn description(&self) -> &'static str {
        self.pass.description()
    }
    fn enabled_for(&self, function: &Function) -> bool {
        self.functions.contains(&function.name) && self.pass.enabled_for(function)
    }
    fn run(&self, function: &mut Function) -> bool {
        self.pass.run(function)
    }
    fn run_module_tracked(
        &self,
        module: &mut Module,
        sites: Option<&mut Vec<crate::InlineSite>>,
        changed: &mut Vec<bool>,
    ) -> Option<bool> {
        self.pass
            .run_module_for_functions(module, &|f| self.enabled_for(f), sites, changed)
    }
    fn run_module_with_inline_budget(
        &self,
        module: &mut Module,
        _enabled: &dyn Fn(&Function) -> bool,
        sites: Option<&mut Vec<crate::InlineSite>>,
        changed: &mut Vec<bool>,
        budget: &mut crate::InlineBudget,
    ) -> Option<bool> {
        self.pass.run_module_with_inline_budget(
            module,
            &|f| self.enabled_for(f),
            sites,
            changed,
            budget,
        )
    }

    fn deterministic(&self) -> bool {
        self.pass.deterministic()
    }
    fn requires_canonical_form(&self) -> bool {
        self.pass.requires_canonical_form()
    }
    fn produces_canonical_form(&self) -> bool {
        self.pass.produces_canonical_form()
    }
    fn run_with_context(&self, function: &mut Function, context: OptimizationContext) -> bool {
        self.pass.run_with_context(function, context)
    }
    fn run_canonical_with_context(
        &self,
        function: &mut Function,
        context: OptimizationContext,
    ) -> bool {
        self.pass.run_canonical_with_context(function, context)
    }
}

fn tier(level: FunctionOptLevel) -> OptimizationTier {
    match level {
        FunctionOptLevel::None => OptimizationTier::None,
        FunctionOptLevel::Fast => OptimizationTier::Fast,
        FunctionOptLevel::Balanced => OptimizationTier::Balanced,
        FunctionOptLevel::Full => OptimizationTier::Full,
        FunctionOptLevel::Size => OptimizationTier::Size,
        _ => OptimizationTier::None,
    }
}

fn selected<'a>(
    function: &Function,
    defaults: &[&'a dyn Pass],
) -> Result<Vec<&'a dyn Pass>, OptError> {
    let policy =
        FunctionPolicy::resolve(&function.attrs).map_err(|message| OptError::FunctionPolicy {
            func: function.name.clone(),
            message,
        })?;
    let level = policy.level;
    let planned = match level {
        Some(level) => tier(level)
            .pipeline()
            .iter()
            .map(|name| crate::find_pass(name).expect("registered preset pass"))
            .collect(),
        None => defaults.to_vec(),
    };
    let mut passes: Vec<_> = planned
        .into_iter()
        .filter(|pass| {
            let name = pass.name();
            !((name.starts_with("inline") && policy.inline == Some(false))
                || (name == "loop-unroll" && policy.unroll == Some(false))
                || ((name == "loop-vectorize"
                    || name.starts_with("loop-reduce")
                    || name == "fast-loop-reduce")
                    && policy.vectorize == Some(false)))
        })
        .collect();
    if !policy.disabled() {
        for (enabled, name) in [
            (policy.inline, "inline-noninterposable"),
            (policy.unroll, "loop-unroll"),
            (policy.vectorize, "loop-vectorize"),
        ] {
            if enabled == Some(true) && !passes.iter().any(|pass| pass.name() == name) {
                passes.push(crate::find_pass(name).expect("registered function policy pass"));
            }
        }
    }
    if function.attrs.flatten && !policy.disabled() && policy.inline != Some(false) {
        passes.retain(|pass| !pass.name().starts_with("inline"));
        let mut flatten = Vec::new();
        for _ in 0..8 {
            for name in [
                "inline-noninterposable",
                "simplify-cfg",
                "canonicalize",
                "instcombine",
                "dce",
            ] {
                flatten.push(crate::find_pass(name).expect("registered flatten pass"));
            }
        }
        flatten.extend(passes);
        passes = flatten;
    }
    Ok(passes)
}

/// Return `None` for the ordinary fast path, otherwise execute scoped schedules.
pub(super) fn run(
    module: &mut Module,
    defaults: &[&dyn Pass],
    context: OptimizationContext,
    mut report: Option<&mut OptimizationReport>,
    options: OptimizeOptions,
) -> Option<Result<(), OptError>> {
    if !module
        .functions
        .iter()
        .any(|f| f.attrs.optnone || f.attrs.flatten || !f.attrs.optimize.is_empty())
    {
        return None;
    }
    Some((|| {
        // Resolve every schedule before mutation. Preserve the original functions
        // on both success and error; reports use the same raw runner as usual.
        let schedules = module
            .functions
            .iter()
            .map(|f| selected(f, defaults))
            .collect::<Result<Vec<_>, _>>()?;
        let mut positions = vec![0; schedules.len()];
        let mut scoped: Vec<ScopedPass<'_>> = Vec::new();
        let mut order = Vec::new();
        // Merge plan frontiers, never moving a pass ahead of an earlier pass in
        // that function's plan. All annotations share the same analysis cache,
        // canonical restoration, verification and reporting manager.
        while let Some((first, _)) = schedules
            .iter()
            .enumerate()
            .find(|(i, plan)| positions[*i] < plan.len())
        {
            let pass = schedules[first][positions[first]];
            let mut functions = keel_ir::fxhash::FxHashSet::default();
            for ((function, plan), position) in
                module.functions.iter().zip(&schedules).zip(&mut positions)
            {
                if plan
                    .get(*position)
                    .is_some_and(|next| std::ptr::eq(*next, pass))
                {
                    functions.insert(function.name.clone());
                    *position += 1;
                }
            }
            // Equivalent schedules share an instance so unchanged passes keep
            // their cache entries across repeated rounds.
            let index = scoped
                .iter()
                .position(|candidate| {
                    std::ptr::eq(candidate.pass, pass) && candidate.functions == functions
                })
                .unwrap_or_else(|| {
                    scoped.push(ScopedPass { pass, functions });
                    scoped.len() - 1
                });
            order.push(index);
        }
        // A function whose plan ended earlier can become dead after another
        // plan's final rewrite. Only plans requesting GC participate; all other
        // bodies remain roots, including explicit O0/optnone bodies.
        let functions = module
            .functions
            .iter()
            .zip(&schedules)
            .filter(|(_, plan)| plan.iter().any(|pass| pass.name() == "unused-functions"))
            .map(|(function, _)| function.name.clone())
            .collect::<keel_ir::fxhash::FxHashSet<_>>();
        if !functions.is_empty() {
            order.push(scoped.len());
            scoped.push(ScopedPass {
                pass: crate::find_pass("unused-functions").expect("registered reachability pass"),
                functions,
            });
        }
        let passes: Vec<&dyn Pass> = order
            .iter()
            .map(|&index| &scoped[index] as &dyn Pass)
            .collect();
        match report.as_deref_mut() {
            Some(report) => crate::run_pipeline_with_context_and_report_raw(
                module, &passes, context, report, options,
            ),
            None => crate::run_pipeline_with_context_raw(module, &passes, context, options),
        }
    })())
}
