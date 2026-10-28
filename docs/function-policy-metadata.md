# Function policies

The C frontend carries function policies through declarations, definitions and
direct callee records. The optimizer and native backends consume these policies.

| C attribute | Behavior |
|---|---|
| `cold` | Retains the invocation IR pipeline and prefers x86 size emission. ELF uses `.text.unlikely`. Optional inlining avoids pulling cold bodies into ordinary callers. |
| `hot` | Increases bounded inliner profitability thresholds. ELF uses `.text.hot`. It does not select a higher optimization level. |
| `optimize(...)` | Selects a per-function preset and supported pass switches, described below. |
| `optnone` | Disables optional IR transformations, inlining into/out of the function, and optional native instruction selection. |
| `flatten` | Requests repeated direct-call inlining into this function, including larger helpers, within legality and growth limits. |
| `no_instrument_function` | Suppresses function instrumentation. Keel currently emits none. |
| `no_profile_instrument_function` | Suppresses profile instrumentation. Keel currently emits none. |
| `naked`, `interrupt` | Represented in IR, but executable consumers reject their unimplemented entry/return contracts. |

`__has_attribute` advertises the first seven policy names on native targets.
It remains false for `naked` and `interrupt`. This does not advertise every GCC
optimization option or pragma.

## Optimization options

Supported presets are numeric `0` through `3`, and string `O0`, `O1`, `O2`,
`O3`, `Os` (or `0`, `1`, `2`, `3`, `s`). They select the corresponding Keel
[optimization level](optimization-levels.md). The last preset wins, including
when the invocation has no optimization passes or an explicit pass list.

The following switches accept their positive and `no-` forms, with optional `f`
or `-f` prefixes:

- `inline`, `inline-functions`: optional direct-call inlining.
- `unroll-loops`: the bounded scalar loop-unroll pass.
- `tree-vectorize`, `tree-loop-vectorize`: loop vectorization and vector reduction
  passes, including the Fast reduction recipe.

Switches retain their source ordering independently of presets. Enabling a loop
switch adds the corresponding pass if the preset lacks it. Disabling it removes
that pass family. Explicit vector operations and target legalization are not
automatic vectorization and remain available. `optnone` wins over all presets
and switches. `optnone` with `always_inline` is an error.

A comma-separated list inside one string is split for resolution. Repeated
attributes append options in declaration order. Unsupported options, including
`Ofast`, `O4` and arbitrary GCC flags, produce an error before native emission;
they are not silently ignored. The parser and IR roundtrip still retain unknown
options so other producers can transport them.

Native selectors inherit invocation target capabilities and allocation policy.
An explicit function preset overrides optional selection: O0/optnone uses
Baseline, O1 uses Trees, and O2/O3/Os uses Full. Explicit non-size presets override
invocation size emission; Os selects size emission. Cold defaults to size emission unless
an explicit preset overrides it. Cold alone preserves an empty IR pipeline and
inherits invocation selection; it does not enable optimization at O0. Explicit sections, COMDAT ownership, retention
and minimum alignment take precedence over automatic hot/cold placement.
AArch64 retains its existing frame decisions; ELF hot/cold section placement and
optional selection apply there too. Darwin has no new hot/cold section split.

These changes target Keel's optimizer and native code generator. They do not
add per-function tuning to external LLVM or Cranelift optimization engines.

## Inlining and mixed policies

Flatten runs at most eight inlining rounds. It does not bypass noinline,
optnone, recursion, interposition, ABI restrictions, stack limits or instruction
growth limits. Only available, legally bound direct-call definitions qualify.
It is a bounded request, not a promise that every call disappears. Ordinary
inlining permits different supported optimization-effort presets. Raw option
spellings are not an ABI or semantic incompatibility. Explicit noinline,
optnone/O0 and disabled-inline boundaries remain protected.

For modules with independently optimized definitions, the manager merges plan
frontiers while preserving each function's local pass order. Inlining remains
interleaved with cleanup. The shared manager retains canonical-form restoration,
verification, change tracking and reports. Disabled bodies are excluded before
canonical restoration, not just before a pass's main callback.

Inlining changes only enabled callers and reads the complete recursion graph.
Constant-argument propagation changes only enabled callees, but inspects all
callers, including protected ones. Empty-call elimination changes only enabled
callers. Reachability cleanup treats unscheduled bodies and their references as
roots; a final cleanup covers functions that become dead after another plan
finishes. Existing assembly and escaping-address root rules still apply.

Unscoped module hooks, including ABI-changing hooks, run only when every current
body enables that pass. Custom passes can implement `run_module_for_functions`
with explicit mutation guards. This still conservatively withholds some ABI
optimizations in mixed modules, without disabling basic module cleanup.
Modules with only temperature hints retain the ordinary invocation pipeline.
Observed and ordinary execution use the same scoped plan. Report pipeline
indices refer to the merged schedule.

Mandatory frontend inlining needed for C language/asm contracts and backend
legalization remain distinct from optional optimization. These policies do not
relax instruction-feature requirements or ABI rules. No branch probability
metadata, hot/cold block splitting or profile-guided optimization is added.

## Parsing, merging and text

Ordinary and double-underscore C spellings are accepted. Later declarations
update definitions and direct callee records. Applying function policies to
objects, parameters, typedefs or members is diagnosed.

`optimize` accepts a nonempty list of narrow printable-ASCII strings and
nonnegative integer constants fitting `u32`. Argument boundaries and repeated
options survive text roundtrips. There is no fixed stored-list byte limit.
Boolean requests merge. IR text permits repeated `optimize(...)` groups, but
rejects duplicate standalone flags. Hot with cold, naked with interrupt, and
special entry contracts with alwaysinline reject.

`FunctionAttrs` is Clone, with shared immutable optimization option storage.
`Module::function_attrs` borrows canonical attributes. Callee synchronization and
inliner imports preserve exactly one canonical option list.

The source contract follows GCC's [function attributes](https://gcc.gnu.org/onlinedocs/gcc/Common-Attributes.html)
and Clang's [optnone attribute](https://clang.llvm.org/docs/AttributeReference.html#optnone).
Keel implements the bounded subset above, not GCC's entire flag vocabulary.

## Special entry contracts

Only argument-free `interrupt` is represented. Forms such as `interrupt("IRQ")`
remain rejected. Interrupt signature validation, register preservation,
interrupt/error-code frames and return instructions are not implemented.
Naked functions likewise lack a body, stack and unwind contract for emission.

Modules containing special entry contracts remain opaque to the ordinary
optimizer pipeline. Native text/typed/chunked emission, LLVM export, Cranelift
translation and Wasm emission reject them before publishing output. Definitions,
declarations and callee tables are checked. Existing assembly-owned transition
adapters remain separate.

## Validation

The repair passes 3,039 workspace tests (zero failures, 15 ignored), 14 Python
tests, eight reduced checks and twelve focused optimizer-policy tests. O2/Os/O3
captured kernels link and pass checked TCG boots. Forty KVM benchmark guests
pass; four wrong-result controls fail normally and are rejected. See the
[repair qualification](function-policy-fix.md) and
[matched-preset measurements](linux-kernel-repaired-benchmarks.md) for the source,
configuration and runtime limits. Strict Clippy remains blocked by existing
workspace findings.

## Kernel integration repair

The [investigation](function-policy-investigation.md) preserves the failure at
`55f097ac`. The [repair](function-policy-fix.md) restores ordered schedules,
guarded module cleanup and useful helper inlining. Fresh pinned kernel cores
link and pass the checked QEMU runtime fixture at O2, Os and O3. This is bounded
minimal-kernel qualification, not a new runtime performance comparison.
