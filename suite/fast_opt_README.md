# Bounded Fast optimization

`keel cc --opt-tier fast input.c -o output.s` and
`keel compile --opt-tier fast input.keel -o output.s` select the opt-in Fast IR
pipeline. `keel opt --opt-tier fast input.keel` prints its optimized IR.
Library callers can use `keel_opt::OptimizationTier::Fast`.

The default remains None. Basic and Full retain their existing pipelines;
`-O` still selects Full and enables all selection passes. The last `-O` or
`--opt-tier` sets the IR tier; `-p` overrides the pipeline in either order.
An explicit Fast tier does not select the Fast register allocator. Full
allocation and baseline instruction selection remain the defaults.

Fast runs `fast-cfg,fast-local,fast-cfg,dce`. Each CFG pass folds branches,
forwards one jump-only edge per visited destination, removes unreachable
blocks with DFS, and merges straight-line chains up to 64 instructions.
There is no fixed-point loop. One forward reverse-postorder local pass shares
the existing instcombine rules for constant evaluation, integer identities,
casts, comparisons and power-of-two strength reduction. A table of at most
256 expressions reuses pure computations within each block; it clears when
full. A separately bounded table of 256 quotients reuses an already executed
same-block division when computing a matching remainder, retaining its trap
guard. Final mark-and-sweep DCE retains pinned operations, including traps,
ordinary and volatile memory accesses, calls, and terminators.

Fast builds no dominator tree or use graph, does no global value numbering,
reassociation, reciprocal division expansion, inlining or loop analysis.
It can miss simplifications exposed later in the pipeline and is not a
convergent canonicalizer. The budgets bound expression-table memory and
block movement; IR Vec removal and alias-chain costs prevent a strict linear
complexity claim for arbitrary input. The budgets are conservative engineering
limits, not experimentally optimal settings.

Arithmetic rewrites use the existing wrapping and trap rules. Division by zero
and signed MIN/-1 division remain traps. Signed MIN%-1 is defined as zero.
Kept and folded instructions retain their source locations; removed pure
instructions can disappear from debug stepping just as in other optimized tiers.

The goal is useful optimization at low compile cost. Runtime competitive with
Cranelift speed and compile cost competitive with Cranelift none are measurement
targets, not guarantees.

## Measuring

Two diagnostic examples support compile-time measurement. They are not built by
default; copy them into `crates/keel/examples/` and build in release mode.

- `fast_opt_probe.rs`: `fast_opt_probe REPS OUTDIR INPUT.keel|INPUT.i ...`
  times frontend, verification, optimization, codegen, assembly and
  optimizer-through-object separately for each tier, rotating tier order with
  two warmup rounds. Raw-IR parsing and cloning are outside tier timings; C
  frontend timing includes lexing, parsing and IR lowering. The backend uses
  default selection and Full allocation in every arm, with native unwind
  information retained.
- `fast_opt_frontend.rs`: `fast_opt_frontend REPS INPUT.i ...`, built with
  `--features keel-c/profile`, reports lex, parse, IRgen, frontend cleanup and
  final IR destruction, and checks staged versus public Debug IR equality. Do not
  mix these instrumented stage numbers into ordinary compiler totals.

`fast_opt_measure.py` drove the original Fast tier experiment (stage timings,
native correctness checks for kernels, Lua, QuickJS and SQLite, and randomized
runtime samples). It expects investigation artifacts under `target/` that are
not part of the repository, so it is not a general-purpose harness.
