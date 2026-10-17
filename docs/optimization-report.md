# Optimization observations

`keel compile`, `keel cc`, and `keel opt` accept `--opt-report PATH`. PATH must
be a new file, distinct from the input and output; existing files, symlinks,
stdout (`-`), duplicate options, and preprocessing-only combinations are
rejected. Reports do not enter assembly/IR stdout. Report creation/write errors
fail the command before normal output emission. Reports are written after
successful optimization; later code-generation failure can leave a valid
optimization report. Optimizer errors retain the ordinary diagnostics and do
not create a CLI report. Library callers retain completed partial observations
alongside `OptError`; a failed canonical restoration stops before recording a
completed invocation of the requested pass.

For example, `keel cc -O --emit-ir quickjs.i --opt-report quickjs-opt.json
-o quickjs.keel` uses the existing Full pipeline. `keel opt` retains its normal
Full default and portable vector target unless explicitly overridden. No
pipeline, inlining policy, budget, verifier, ABI snapshot, attribute, or debug
metadata change is intended. Public/interposable definitions remain excluded.

Library clients can use `run_passes_for_target_with_report` or
`run_pipeline_for_target_with_report` with an `OptimizationReport`. Existing
entry points and the explicit-request-only `InlineReport` API remain available,
including its legacy broad reason strings. Custom module passes can override
`Pass::run_module_with_report`; its default dispatches to `run_module` once.

## Repeated verification

The CLI always validates the initial input module, including with no optimization
passes. Canonical restoration always runs when required. Repeated checks after
passes and restoration default on when debug assertions are enabled, and off in
ordinary release builds. The `verify-each` Cargo feature on `keel-opt` (forwarded
by `keel`) forces the default on, including development release builds.
`--verify-each` and `--no-verify-each` explicitly override the build default for
`compile`, `cc`, and `opt`, independently of optimization tier, pass selection,
and reporting. Contradictory flags are rejected in either order; repeating the
same flag is accepted. Disabling checks never relaxes a pass's correctness contract.

Library callers must supply validated input modules, as before. Scoped
`OptimizeOptions { verification: VerificationPolicy::Enabled }` or `Disabled`
can be passed to `run_passes_for_target_with_options` and
`run_pipeline_for_target_with_options`; the observed counterparts end in
`_with_report_and_options`. `VerificationPolicy::Default` and legacy entrypoints
use the build default. There is no process-global state or environment override.

## Schema version 2

The JSON root contains `schema_version` and `invocations`. Invocation order is
pipeline order, then module function layout order for function passes. Repeated
passes have distinct zero-based `pipeline_index` values. Each invocation has
`pass`, `target` (vector capability: portable/aarch64-neon/x86-sse2), `scope`
(module/function), nullable `function`, and the pass's actual `changed` return.
The target field is not an ISA tuning or native CPU claim. Each invocation's
`verification_enabled` records the resolved repeated-verification policy, also
when reports accumulate calls with different policies. An empty pipeline has no
invocations or repeated checks to describe.

`pass_ns` measures only pass dispatch/execution (including inliner event
collection when enabled). `verification_ns` separately measures the existing
post-pass verification when enabled. Disabled `verification_ns` and
`restoration_verification_ns` are exactly zero: no verification timer is started.
Initial CLI input validation is outside this report. `restoration_ns` and `restoration_verification_ns`
separate required canonicalization and its verification from the named pass.
`restoration_changed` is null when no restoration ran, otherwise its actual
boolean result. Thus a pass can report `changed: false` while its entry
restoration reports true. Module verification times sum the serial checks of
its functions. All times are elapsed wall-clock nanoseconds, not CPU time.
Count collection and JSON serialization are outside these timed intervals.

`before` counts the invocation input before any required restoration; `after`
counts the named pass output. Counts cover functions, layout blocks, live layout
instructions, block parameters, stack declarations, and summed declared stack
bytes. They exclude detached/dead instruction arena entries. Declared stack
bytes include unreferenced declarations and exclude alignment, saved registers,
spills, and backend frame decisions: **they are not frame bytes**.

`inline_sites` contains the original direct Call/Invoke/ReturnCall input sites
considered by each inline invocation, in caller/site layout order. Each has
caller/callee names, original instruction `site` index, `category` (ordinary or
alwaysinline), and `outcome`. Newly cloned calls are not reconsidered or
reported as input sites in the same invocation. Indirect calls are outside the
existing direct-site loop. An alwaysinline definition remains categorized as
such even when rejected before becoming a template.

Outcomes are `inlined`, or the selected failed check in existing diagnostic and
site decision order; a site may have additional blockers. Template rejection
precedence remains noinline, recursion, legality, profitability. Detailed
legality outcomes cover public definitions, missing definitions, variadic and
returns-twice callees, callee/caller personality, missing entry, oversized byval
snapshots, unsupported intrinsic/VA/EH/tail-call/inline-assembly constructs,
calls to returns-twice functions, unsupported caller/call instruction, and ABI
signature mismatch. Profitability outcomes distinguish blocks, instructions,
and stack declaration bytes. Hard budgets distinguish caller call count,
caller instruction growth, module instruction growth, caller stack growth,
and the callee hard stack declaration bound. These are decisions recorded
while selecting frozen templates or processing actual sites, not explanations
inferred from the final optimized IR. All field and event ordering is stable
except timing values.

Reporting intentionally adds clock reads, IR scans, event/name allocations,
serialization, and I/O. Reported pass time includes event instrumentation and
is not directly comparable to uninstrumented compile cost. Disable reporting
for compile-cost comparisons. Ordinary pass-manager entry points contain no
new report clocks or counts; ordinary skipped-site collections and the
explicit-definition category set are opt-in. The inliner still has its existing
frozen-template/rejection maps and explicit-request skip allocations. Replacing
boolean eligibility predicates with reason-returning predicates and sharing
template decisions can affect overhead; no zero-overhead measurement is claimed.

## Integration and investigation

Base: `21d050463a65fb868321eda5107f941443a2b010`, whose measured compiler baseline
is documented in `current-app-runtime-2026-10-16.md`. This checkpoint instruments
existing choices. The validated MacHost QuickJS diagnostic
records caller instruction growth as the selected blocker for the remaining
53 duplicate-value, 32 exception-test, and 21 alwaysinline integer-constructor
calls in JS_CallInternal; reported/unreported/retained baseline IR match exactly.
A bounded late inlining policy is a separate follow-up, with no gain inferred
from these decisions.
Aggregate CFG promotion remains deferred. Coordinator serial integration and
independent review are required before integration into main.
