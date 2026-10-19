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
targets, not guarantees. Results and limitations for the 2026-10-19 experiment
are in `target/fast-opt-investigation-20261019/REPORT.txt`.

The experiment uses a frozen HEAD archive plus only the optimizer/driver
changes and the diagnostic example. Copy `fast_opt_probe.rs` to that archive's
`crates/keel/examples/fast_opt_probe.rs`, then build its release CLI and example
into a private target directory. Run builds and each measurement using
`python3 suite/fast_opt_locked.py LOG.jsonl COMMAND ...`; it holds an exclusive
Python `fcntl.flock` on `/tmp/keel-perf-20261019.lock` and records load averages.
`fast_opt_measure.py ARTIFACT_ROOT stages|checks|extra|bridge|runtime` records exact
commands and refuses to overwrite measurements. It requires the saved suite
preprocessed inputs and frozen bridge from the previous investigations.

`stages` records frontend, verification, optimization, codegen, assembly and
optimizer-through-object timings separately. Raw-IR parsing and cloning are
outside tier timings; C frontend timing includes lexing, parsing and IR lowering.
Tier order rotates in-process, with two warmup rounds. `checks` links saved
objects and checks kernels, exact native trap signals, Lua checksums, QuickJS
language tests and SQLite output. `extra` runs Lua's portable upstream suite
and the remaining QuickJS tests for None and Fast. `bridge` compiles the saved same IR through
the existing frozen harness. Its native rows are an older backend and must not
be presented as candidate native measurements. `runtime` randomizes arms and
checks outputs before accepting each sample. It is a local exploratory run.

The bridge has unwind information disabled while native objects retain it.
Bridge backend timings exclude C frontend, input verification and Keel IR
optimization. Account for these different services and stage boundaries when
interpreting results. The Lua bridge subset is a compile workload; complete
native Lua and QuickJS are separate application checks.

For a separate frontend/lowering breakdown, copy `fast_opt_frontend.rs` to a
frozen checkout's `crates/keel/examples/`, build with `--features keel-c/profile`
in a separate private target directory, and run it under the same lock. It
checks staged versus public Debug IR equality and reports lex, parse, IRgen,
frontend cleanup and final IR destruction. Do not mix these instrumented stage
numbers into the ordinary compiler totals.
