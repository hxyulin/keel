# keel

An educational compiler written in Rust: a low-level SSA IR, optimizer,
assembly generators for aarch64 macOS and x86-64 Linux, and a C frontend.
Clang supplies preprocessing, assembly and linking; keel compiles one
preprocessed C translation unit or textual IR module to assembly.

See [docs/design.md](docs/design.md) for the design decisions.

## Usage

```sh
cargo run -p keel -- compile crates/keel-codegen/tests/programs/return42.keel -o return42.s
clang -o return42 return42.s && ./return42; echo $?   # 42

cargo run -p keel -- compile -O input.keel -o out.s   # optimize first
cargo run -p keel -- opt -p gvn,dce input.keel        # run passes, print the IR
cargo run -p keel -- passes                           # list the passes
```

C goes through clang's preprocessor first; keel compiles one preprocessed
file to one assembly file:

```sh
clang -E -std=c11 hello.c -o hello.i
cargo run -p keel -- cc -O hello.i -o hello.s         # -g for debug info
clang -o hello hello.s && ./hello                     # clang -g with -g, then lldb
cargo run -p keel -- cc --emit-ir hello.i             # print the keel IR
```

Both commands take `--target aarch64-apple-darwin` or
`--target x86_64-unknown-linux-gnu`; the default is the host.

Target instruction selection is an optional codegen pass, independent of
`-O`. `--isel all` enables generated tree patterns, allocation of folded
operands, and register hints. `--isel trees,folds,hints` selects individual
passes; `--isel none` (the default) keeps the existing selection. `folds`
removes already folded operands from allocation; ordinary immediate and
address folding remain active in every mode. For example:

```sh
cargo run -p keel -- cc -O --isel all hello.i -o hello.s
cargo suite run -c keel-O,keel-O-select
```

Fewer instructions can still cost more registers or change scheduling, so
selection remains opt-in while its profitability rules are developed.
For C, preprocessing must use the same target and its headers, for example
`clang -E --target=<triple>` with the appropriate sysroot. Passing
`--target` to keel does not change an already preprocessed file.

## Compiler tiers and profiling

The stages can be tuned independently:

- `--opt-tier none|basic|full`: no IR passes; CFG simplification, constant
  folding and DCE; or the existing full pipeline including GVN and GCM.
  `-O` remains an alias for `full`; the default is `none`.
- `--isel-tier baseline|trees|full`: existing per-instruction lowering;
  generated tree selection; or trees plus folded operand liveness and
  register affinities. The default is `baseline`. `--isel` still allows
  individual features. When flags overlap, the last one wins.
- `--regalloc fast|full`: contiguous live intervals without affinities;
  or lifetime holes and register affinities. The default is `full`.
  Both use linear scan with the same ABI and spill handling. These are
  effort choices whose performance needs to be measured per workload.

```sh
cargo run -p keel -- compile --opt-tier basic --isel-tier trees --regalloc fast input.keel -o out.s
cargo suite run -c keel-basic,keel-fast,keel-full
cargo suite run -c keel-basic,keel-basic-sccp,keel-O,keel-O-sccp
```

`keel compile -p pass,pass,...` and `keel cc -p pass,pass,...` select an
explicit IR pipeline, overriding `-O` and `--opt-tier` regardless of flag
order. The opt-in `sccp` pass propagates constants through executable edges
and block parameters. Its suite presets replace `const-fold` with `sccp`;
the default pipelines retain direct constant folding pending performance
evidence.

Library users can implement `keel_opt::Pass` and use `run_pipeline` (which
verifies after every pass), or provide an `InstructionSelector` and
`RegisterAllocator` through `CodegenPipeline`. Target-specific
`compile_with_pipeline` also accepts restricted register pools for spill
experiments. Selection produces the instruction plan consumed by the target
lowering code and allocator; register hints are preferences, never constraints.
The target emitters retain responsibility for ABI handling and assembly.

The `profiling` crate instruments passes, selection, allocation, lowering,
and IR parsing/verification. Instrumentation is inactive in default builds.
To capture with Tracy, build with `cargo build --release -p keel --features
profile-with-tracy` and connect a compatible Tracy profiler before running
compilation. The driver starts and retains the client for the invocation.
Library benchmarks can enable `profile-with-tracy` on the optimizer/codegen
crates and manage the Tracy client themselves. Benchmark uninstrumented
release builds for final timings; use instrumented builds to locate costs.

## Current scope

- SSA IR with integer and floating-point operations, globals, thread-local
  storage, atomics, calls, C ABIs and debug information.
- Seven optimization passes: constant folding, CFG simplification, global
  value numbering, dead-code elimination, global code motion and opt-in
  sparse conditional constant propagation (`sccp`), plus instruction combining
  and integer strength reduction (`instcombine`).
- A C11-oriented frontend with GNU extensions including `typeof`,
  `__auto_type`, statement expressions, computed goto and case ranges.
  Atomic pointer prefix/postfix `++` and `--`, fixed-type overflow
  builtins, alignment/branch hints and resumable `__builtin_debugtrap`
  are supported. Hints preserve values and side effects but do not
  currently guide optimization.
- C variable-length arrays, complex arithmetic, nested functions and
  inline assembly statements are not supported. The ARM64 IR backend
  supports inline assembly and exceptions; the x86-64 backend does not.
- Attribute parsing is broader than attribute semantics: many attributes
  are ignored. Keel does not claim full GCC or Clang compatibility.

See [the C frontend design](docs/design.md#c-frontend) for supported
extensions, compatibility limits and possible next additions.

## Development

- `cargo test --workspace` runs the workspace's unit, golden and
  end-to-end tests on macOS arm64 or x86-64 Linux. End-to-end tests need
  `clang`; C++ codegen drivers also need `clang++`, and ARM64 debug tests
  need a working `lldb` session. A `; target: <triple>` line limits a
  codegen test program to one target.
- To check compilation and execution while excluding debugger sessions,
  use `cargo test --workspace -- --skip debugger --skip debug_info`.
  This does not validate debug information. Downloaded application suites
  run separately through `cargo suite test`.
- IR golden tests live in `crates/keel-ir/tests/golden/`; `BLESS=1 cargo test`
  rewrites the expected `.out` files.
- Codegen tests live in `crates/keel-codegen/tests/programs/`: each `.keel`
  file has an `; expect: exit N` or `; expect: trap` line and may have a
  same-named `.c` driver that it is linked with. A `; opt-passes: a, b`
  line selects the pipeline for its optimized configurations. Each program is compiled
  with the full register set, with a tiny one that forces spills, and
  after the default optimization pipeline, both with optional selection
  disabled and with every selection pass enabled, plus the fast allocator
  with ordinary and forced-spill register pools (eight builds). A program that runs
  for more than 10 seconds fails.
- Pass tests live in `crates/keel-opt/tests/golden/`: each starts with a
  `; passes: a, b` line.
- C tests live in `crates/keel-c/tests/programs/`: each `.c` program is
  built by clang and by keel (as is and with `-O`), and both must print the
  same and exit the same. Separate frontend tests cover GNU inference
  rules where Clang differs and compile selected features for both targets.
- `cargo suite test` builds Lua, SQLite and QuickJS with keel and runs
  their test suites; `cargo suite run` builds them with clang and keel in
  every configuration and times them. Each program is a directory in
  `suite/`; see [suite/README.md](suite/README.md).
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/),
  scoped by area: `feat(ir): ...`, `fix(codegen): ...`, `docs: ...`.
