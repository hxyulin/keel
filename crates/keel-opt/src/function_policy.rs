//! Function-local schedules. Mixed presets conservatively avoid unscoped module rewrites.
use crate::{
    OptError, OptimizationContext, OptimizationReport, OptimizationTier, OptimizeOptions, Pass,
};
use keel_ir::{Function, FunctionOptLevel, FunctionPolicy, Module};

/// Hide module hooks while retaining the pass's target and canonical dispatch.
struct LocalPass<'a>(&'a dyn Pass);
impl Pass for LocalPass<'_> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn description(&self) -> &'static str {
        self.0.description()
    }
    fn run(&self, function: &mut Function) -> bool {
        self.0.run(function)
    }
    fn deterministic(&self) -> bool {
        self.0.deterministic()
    }
    fn requires_canonical_form(&self) -> bool {
        self.0.requires_canonical_form()
    }
    fn produces_canonical_form(&self) -> bool {
        self.0.produces_canonical_form()
    }
    fn run_with_context(&self, function: &mut Function, context: OptimizationContext) -> bool {
        self.0.run_with_context(function, context)
    }
    fn run_canonical_with_context(
        &self,
        function: &mut Function,
        context: OptimizationContext,
    ) -> bool {
        self.0.run_canonical_with_context(function, context)
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
    // A temperature hint must not turn an unoptimized invocation into an
    // optimized one. An explicit optimize preset can do so.
    let level = policy.level.or_else(|| {
        (function.attrs.cold && !defaults.is_empty()).then_some(FunctionOptLevel::Size)
    });
    let names: Vec<_> = match level {
        Some(level) => tier(level).pipeline().to_vec(),
        None => defaults.iter().map(|pass| pass.name()).collect(),
    };
    let mut passes = Vec::new();
    for name in names {
        if (name.starts_with("inline") && policy.inline == Some(false))
            || (name == "loop-unroll" && policy.unroll == Some(false))
            || ((name == "loop-vectorize"
                || name.starts_with("loop-reduce")
                || name == "fast-loop-reduce")
                && policy.vectorize == Some(false))
        {
            continue;
        }
        if let Some(pass) = defaults
            .iter()
            .copied()
            .find(|pass| pass.name() == name)
            .or_else(|| crate::find_pass(name))
        {
            passes.push(pass);
        }
    }
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
        .any(|f| f.attrs.optnone || f.attrs.cold || f.attrs.flatten || !f.attrs.optimize.is_empty())
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
        // Inlining is the only module rewrite with an explicit policy boundary.
        // ABI changes, constant-argument propagation and deletion are withheld
        // until those passes can account for independently optimized bodies.
        let rounds: Vec<_> = module
            .functions
            .iter()
            .zip(&schedules)
            .map(|(function, schedule)| {
                if function.attrs.flatten {
                    8
                } else {
                    schedule
                        .iter()
                        .filter(|pass| pass.name().starts_with("inline"))
                        .count()
                }
            })
            .collect();
        for round in 0..rounds.iter().copied().max().unwrap_or(0) {
            let inline = crate::inline::ScopedInline {
                callers: module
                    .functions
                    .iter()
                    .zip(&rounds)
                    .filter(|(_, rounds)| round < **rounds)
                    .map(|(function, _)| function.name.clone())
                    .collect(),
            };
            if let Some(report) = report.as_deref_mut() {
                crate::run_pipeline_with_context_and_report_raw(
                    module,
                    &[&inline],
                    context,
                    report,
                    options,
                )?;
            } else {
                crate::run_pipeline_with_context_raw(module, &[&inline], context, options)?;
            }
        }
        for (function, schedule) in module.functions.iter_mut().zip(schedules) {
            let locals: Vec<_> = schedule.iter().map(|&pass| LocalPass(pass)).collect();
            let passes: Vec<&dyn Pass> = locals.iter().map(|pass| pass as &dyn Pass).collect();
            // The isolated module has no module hooks, so references to other
            // functions remain untouched. Move the body back even on error.
            let mut local = Module::default();
            let placeholder = Function::new("", function.sig.clone());
            local
                .functions
                .push(std::mem::replace(function, placeholder));
            let result = match report.as_deref_mut() {
                Some(report) => crate::run_pipeline_with_context_and_report_raw(
                    &mut local, &passes, context, report, options,
                ),
                None => crate::run_pipeline_with_context_raw(&mut local, &passes, context, options),
            };
            *function = local.functions.remove(0);
            result?;
        }
        Ok(())
    })())
}
