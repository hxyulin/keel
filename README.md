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
  folding, instruction combining and DCE; or the full pipeline including
  GVN and GCM.
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
and block parameters. Basic and Full use `instcombine` for numeric folding,
integer identities and power-of-two strength reduction. SCCP suite presets
add propagation before combining. `phi-simplify` also remains opt-in: use
`keel-O-phi-late` to try cleanup after code motion. Early phi cleanup caused
large application regressions. Pipeline choices and raw measurements are in
[suite/README.md](suite/README.md#paired-pass-measurements).

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
- Eight optimization passes: constant folding, CFG simplification, global
  value numbering, dead-code elimination, global code motion and opt-in
  sparse conditional constant propagation (`sccp`), plus instruction combining
  and integer strength reduction (`instcombine`) and trivial block-parameter
  elimination (`phi-simplify`).
- A C11-oriented frontend with GNU extensions including `typeof`,
  `__auto_type`, statement expressions, computed goto and case ranges.
  Atomic pointer prefix/postfix `++` and `--`, fixed-type overflow
  builtins, alignment/branch hints and resumable `__builtin_debugtrap`
  are supported, along with GNU member packing and GNU/C11 object
  alignment. Hints preserve values and side effects but do not
  currently guide optimization.
- C variable-length arrays, complex arithmetic, nested functions and
  inline assembly statements are not supported. The ARM64 IR backend
  supports inline assembly and exceptions; the x86-64 backend does not.
- Attribute parsing is broader than attribute semantics: many attributes
  are ignored. Keel does not claim full GCC or Clang compatibility.

See [the C frontend design](docs/design.md#c-frontend) for supported
extensions, compatibility limits and possible next additions.

## Assembler (initial A64 subset)

```sh
cargo run -p keel -- asm crates/keel-asm/examples/return42.s -o return42.o
clang -o return42 return42.o
./return42; echo $?   # 42 on Apple Silicon macOS
cargo run -p keel -- asm crates/keel-asm/examples/stack_roundtrip.s -o stack.o
clang -o stack stack.o
./stack; echo $?      # 42
cargo run -p keel -- asm crates/keel-asm/examples/compute_loop.s -o loop.o
clang -o loop loop.o
./loop; echo $?       # 42
cargo run -p keel -- asm crates/keel-asm/examples/external_call.s -o caller.o
cargo run -p keel -- asm crates/keel-asm/examples/answer.s -o answer.o
clang -o external caller.o answer.o
./external; echo $?   # 42
cargo run -p keel -- asm crates/keel-asm/examples/data_roundtrip.s -o data.o
clang -o data data.o
./data; echo $?       # 42
```

The assembler generates Mach-O objects using tables from Arm’s
machine-readable ISA data. It supports integer arithmetic, logical and
bitfield operations, conditional branches/selects, indirect calls, scalar
floating-point arithmetic/conversions, popcount’s SIMD instructions, and
integer/FP/SIMD loads and stores (including pairs and acquire/release).
Scalar LSE atomics include read-modify-write operations and compare-and-swap;
DMB/DSB/ISB barriers, `ngc`/`ngcs`, `svc`, and SIMD `movi` are supported too.
Symbol addressing supports local `adr`, `adrp` with PAGE/GOT/TLVP modifiers,
page-offset memory operands, and external calls through Mach-O relocations.
Data support includes initialized/zero-fill/TLS sections, integer directives,
pointer tables, and symbol differences. It also supports ordinary CFI unwind
records, compact unwind for canonical ARM64 frames and leaf functions, DWARF 4
line tables, and debug sections for types and variables.
Exception personalities, LSDA tables, and C++ exception propagation are
supported, along with subsection splitting for linker reordering and dead
stripping. Literal loads and other unsupported forms still prevent assembling
arbitrary `keel compile` output.
Unsupported syntax is an error. See [the assembler design](docs/design.md#assembler) for syntax,
source provenance, licensing, and regeneration instructions.

Instruction metrics are available through `keel asm-metrics input.s` and
[`tools/instruction_metrics.py`](tools/instruction_metrics.py). The tool generates
an inventory of all supported A64 forms, optionally imports CPU scheduling
estimates from LLVM, and compares equivalent sequences by size or modeled
execution cost. See [instruction costs](docs/instruction-costs.md) for coverage,
provenance, and the limits of the shared LLVM Apple CPU model.

## Development

- `cargo test -p keel-asm` checks encodings against clang and, on Apple Silicon,
  links and runs the assembler’s examples and memory-operation checks.
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
