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

`asm`, `compile`, `cc`, and `opt` map input files by default.
They use a read-only copy-on-write file mapping instead of an owned read
buffer for nonempty regular files, while retaining UTF-8 validation. Keep
mapped inputs unchanged until parsing completes; copy-on-write does not
protect against another process truncating the file. Empty files, streams,
and filesystems that reject mapping fall back to ordinary reads. Mappings
are released before writing output, including when output reuses the input
path.

To compare the two paths on already generated Lua assembly, run
`python3 suite/measure_assembler_io.py --compiler target/release/keel
--read-compiler target/assembler-bench/pre-default-read-keel
--input-dir target/suite/lua/build/keel --work-dir target/assembler-bench/io
--output target/assembler-bench/io/report.json`. Builds and tests should finish
before timing. This measures warm-cache CLI costs, not cold-storage latency.
The read comparison requires an archived release driver from before mapping
became the default, such as commit `cc01b96`. Historical reports retain the
commands and flags used at the time.

## x86-64 Linux

The native Linux path targets x86-64-v2 and the System V ABI, emitting Intel
syntax for Clang's integrated assembler or GNU `as`, with ELF linking through
Clang. On Linux, preprocess against the Linux system headers:

```sh
cargo build --release -p keel
clang -E -std=c11 hello.c -o hello.i
target/release/keel cc --target x86_64-unknown-linux-gnu hello.i -o hello.s
clang hello.s -o hello
./hello
```

The [NUC bring-up report](suite/reports/x86-linux-bringup-20261015.html) records
native execution on Ubuntu 24.04 / Intel i7-1360P, with Rust 1.99 and Clang 18.
Compiler tests, ordinary/optimized C fixtures, both assemblers with debug info,
and Lua's portable tests in all four compiler/selection modes pass. QuickJS
and SQLite currently stop at an alignment-changing typedef in glibc's
`pthread.h`; that frontend feature remains unsupported.

`--emit-object` and the structured compiler/assembler API currently target
AArch64 Mach-O. Native x86 instruction encoding and an ELF writer remain to
be implemented. The x86 backend also diagnoses exceptions, inline assembly
and selected i128 operations/conversions it cannot lower yet.

For native compiler checks, use
`cargo test --workspace --exclude keel-asm --all-targets`. The portable ARM
assembler core can be tested with
`cargo test -p keel-asm --lib --test structured`; the full ARM differential
suite additionally needs a reference Clang that recognizes its requested
features, including `-mcpu=apple-m5` (Ubuntu Clang 18 does not).

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

CPU tuning is independent of these effort settings. `--tune generic` preserves
the defaults; `--tune apple-m5` enables measured Apple M5 choices when tree
selection is enabled, for example `-O --isel-tier full --tune apple-m5`.
It does not enable additional ISA features. See [native M5 tuning](docs/instruction-costs.md#native-apple-m5-tuning)
for calibration and application results.

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

## Assembler (A64 subset)

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
The generated table currently contains 1647 encoding forms, counting register
widths, addressing modes, and atomic ordering variants separately. This is
a supported subset, rather than complete A64 or GNU assembler compatibility.
Data support includes initialized/zero-fill/TLS sections, integer directives,
pointer tables, and symbol differences. It also supports ordinary CFI unwind
records, compact unwind for canonical ARM64 frames and leaf functions, DWARF 4
line tables, and debug sections for types and variables.
Exception personalities, LSDA tables, and C++ exception propagation are
supported, along with subsection splitting for linker reordering and dead
stripping. The tests assemble all current backend fixtures and C fixtures
(including optimization and debug configurations). General NEON arithmetic and
selected SVE/SME forms are supported. `--cpu apple-m5` enables the modeled M5
numeric, matrix, crypto, CRC, PAC and BTI extensions; it does not claim complete
M5 ISA coverage or enable ordinary non-streaming SVE. Broader system syntax and
some instruction families remain unsupported.
Unsupported syntax is an error. See [the assembler design](docs/design.md#assembler) for syntax,
source provenance, licensing, and regeneration instructions.

Assemble multiple independent files in one process to amortize startup:

```sh
target/release/keel asm --cpu apple-m5 src/one.s src/two.s --out-dir build/objects
```

Each input produces `<stem>.o` in the output directory, which is created if
necessary. Files are processed serially with separate symbol and directive
state; memory for each input is released before the next. Output-name collisions
and outputs aliasing an input are rejected before any object is written. On
macOS, names differing only by case are conservatively treated as collisions.
The batch stops on its first error, keeping objects already written. A single
input can still use `-o file.o`; `-o` and `--out-dir` are mutually exclusive.
Use `--` before input names beginning with a dash. This assembler accepts AArch64
syntax and emits Apple arm64 Mach-O; x86 AT&T/Intel syntax and ELF output require
separate target support.

Hand-written assembly can use named `.macro` parameters (`\name` substitution
and `\()` concatenation), nested `.rept` blocks, and literal `.if`/`.elseif`/
`.else` conditionals. See [the standalone example](crates/keel-asm/examples/standalone.s).
The supported subset and expansion limits are described in
[the assembler design](docs/design.md#assembler); includes, symbolic conditional
expressions and advanced macro parameter modes are not yet supported.

The compiler can emit an object in-process, without writing an intermediate
assembly file or starting another assembler process:

```sh
target/release/keel cc input.i --emit-object -o output.o
target/release/keel compile input.keel --emit-object -o output.o
```

Library callers can compose the same stages directly:

```rust,ignore
let program = keel_codegen::compile_program(
    &module, keel_codegen::Target::Aarch64AppleDarwin,
)?;
let object = keel_asm::assemble_program_object(
    &program, keel_asm::TargetFeatures::default(),
)?;
```

Both stages depend on `keel-asm-ir`, which owns generated `Opcode` and
`FieldId` identities, instructions, symbolic references and typed directives.
All compiler-generated AArch64 instructions, data, CFI, DWARF and exception
metadata bypass source formatting and parsing, with normal encoding and feature
validation. Conditional branches stay structured during far-branch relaxation.
Only user inline assembly enters the program as source text; inline macros use
an explicit validated expansion fallback. Assembly text output remains available.
Direct object emission currently targets AArch64 Mach-O; see
[the API design](docs/design.md#assembler).
The completed migration covers all 85,988 generated Lua instructions, with zero
source-text bytes. In the [33-file Lua comparison](suite/reports/compiler-object-api-complete-20261015.html),
the API is 1.68× faster for codegen plus object generation and 1.48× for the
complete CLI workflow than the assembly-text round trip. Paired CLI time is
5.2% lower than the previous partial API; all objects are byte-identical.

Instruction metrics are available through `keel asm-metrics input.s` and
[`tools/instruction_metrics.py`](tools/instruction_metrics.py). The tool generates
an inventory of all supported A64 forms, optionally imports CPU scheduling
estimates from LLVM, and compares equivalent sequences by size or modeled
execution cost. See [instruction costs](docs/instruction-costs.md) for coverage,
provenance, and the limits of the shared LLVM Apple CPU model.

To measure assembly speed against Clang on Apple Silicon macOS:

```sh
python3 suite/measure_assembler.py --output target/assembler-bench/report.json
```

The script builds release binaries, checks instruction bytes against Clang,
and records randomized paired timings for `keel asm`, `clang -x assembler -c`,
and the direct `clang -cc1as` entry point. Preprocessing and C compilation are
outside timing. Inputs cover tiny files, 1K–100K mixed instructions, and
generated C with 1,000 functions, with/without debug information. These are
scaling workloads, not an application-suite performance claim. Raw samples,
tool versions, source hashes, and in-process parse/encode versus Mach-O writer
timings are retained. The stage profiler can also run separately with
`cargo run --release -p keel-asm --example profile -- input.s 30`.

To compare IR parsing, AArch64 assembly text generation, and text assembly
independently, build `cargo build --release -p keel-asm --example profile_stages`
on each revision and retain both binaries (copy the example into an older
checkout if needed). Then run:

```sh
python3 suite/measure_pipeline_stages.py --baseline /path/to/baseline-profile_stages \
  --candidate target/release/examples/profile_stages \
  --output target/pipeline-stages/comparison.json
```

The script generates arithmetic, loop, and register-pressure scaling inputs
from existing IR fixtures. It randomizes paired process order, rotates stage
order, discards warmups, and retains raw samples, binary and input hashes,
and bootstrap intervals. File reading, verification, and prerequisite stages
are outside each timer; result allocation and destruction are included.
These synthetic inputs measure compiler throughput, rather than application
runtime. The [October 15 comparison](suite/reports/pipeline-stages-aarch64-2026-10-15.json)
records the byte-scanning IR lexer, direct assembly formatting, and encoder
field-validation changes.

For backend attribution on application inputs, build the opt-in profiler:

```sh
cargo build --release -p keel-asm --features backend-profile --example profile_backend
python3 suite/profile_backend.py --input-dir target/suite/lua/pre \
  --rounds 9 --output target/backend-profile/lua.json
```

Use repeatable `--input file.i` arguments for individual C or `.keel` inputs;
`--opt`, `--debug`, and `--selection-all` configure the measured backend.
Source parsing, IR verification, and optional optimization finish before timing.
The profiler compares text and structured output, verifies identical objects,
and records nested inclusive and exclusive stage timings, per-function totals,
and active-collection overhead. Timing scopes compile out unless the
`keel-codegen/backend-profile` feature is enabled. Library callers can use
`keel_codegen::backend_profile::capture(|| compile(...))` for a synchronous,
thread-local capture; nested captures on one thread are rejected.

Register allocation is independently selectable with `--regalloc full` (the
default) or `--regalloc fast`. Both use reachable reverse postorder and sparse
worklist liveness. Full preserves lifetime holes and selection affinities;
Fast builds conservative scalar endpoints directly and uses a separate
contiguous-interval scanner. It can produce more spills, while avoiding the
Full allocator's range construction and inactive-interval handling. Both obey
the same ABI, call-clobber, returns-twice, and i128 rules. `--isel-tier` and
IR optimization settings remain independent.

The profiler accepts `--regalloc-fast` and records static instruction counts,
code bytes, aggregate spill slots and callee saves, and explicit spill-helper
load/store calls. Those helper counts exclude other memory accesses and are
not dynamic traffic measurements. Additional coarse stages separate
constant/address folding from compare fusion and lowering entry, body, finish,
and allocation annotations; no per-instruction timers are used.

The [allocator candidate comparison](suite/reports/backend-tiers-aarch64-2026-10-15/report.html)
compares saved Fast/Full implementations against the new paths, with identical
IR/selection settings and separate generated-code runtime measurements.
`suite/measure_backend_tiers.py` builds direct-object application binaries,
checks byte-identical output against each saved tier, and performs randomized
paired measurements using `--build-only`, `--profile-only`, or `--runtime-only`.
Supply `--suite-cache`, `--baseline-compiler`, `--candidate-compiler`, both
`--baseline-profiler`/`--candidate-profiler`, `--work-dir`, and `--output`.
Save the baseline binaries before changing the backend; both profilers must
support `--regalloc-fast`. The renderer `suite/render_backend_tiers.py` reads
`builds.json`, `profiles.json`, `runtime.json`, and `compatibility.json` from
`--input-dir` and writes a summary, interactive HTML, and compressed raw data
to `--output-dir`. Compiler-speed results exclude the frontend and do not
establish whole-build gains.

The [application backend profile](suite/reports/backend-profile-aarch64-2026-10-15/report.html)
covers all 33 Lua files in default, optimized, and debug configurations,
plus the QuickJS core and SQLite amalgamation. Its directory retains a summary
and compressed raw samples. `suite/render_backend_profile.py` rebuilds the report
from `lua.json`, `lua-opt.json`, `lua-debug.json`, and `large.json` using
`--input-dir` and `--output-dir`. These measurements attribute backend time;
they exclude the frontend, IR optimization, startup, and file I/O and do not
establish full-build speedups.

For a before/after comparison, copy the release compiler and stage profiler
before changing the assembler, then pass `--baseline-compiler <copy>` and
`--baseline-profiler <copy>`. To measure actual suite assembly, prepare it
with `cargo suite test lua -c keel`, then pass
`--only-inputs --input-dir target/suite/lua/build/keel`. The report includes
per-file measurements and summed serial assembly costs; those totals are
distinct from a parallel build's wall time.

`suite/measure_assembler_candidates.py` compares multiple prebuilt candidates
in randomized paired rounds, including in-process timings and command-line
costs. Supply `--candidate NAME CLI PROFILE` for each candidate, including
`baseline`, plus `--input-dir`, `--work-dir` and `--output`. Each input must
produce byte-identical Mach-O objects across candidates. The October 15
[candidate measurements](suite/reports/assembler-candidates-aarch64-2026-10-15.json)
cover all 33 Lua files, mixed instructions, debug output, and separate scanning
stress inputs. Allocation reduction was retained; the SIMD comment-scanning
experiment remains on `perf/simd-source-scanning` (`ac18b71`) because its
strong stress-input gains did not improve ordinary Lua parsing.

A subsequent scanner fuses line splitting and comment removal, reusing NEON
delimiter masks across ordinary instruction lines and using a scalar fallback
on other targets. Combined with per-pass bump scratch and borrowed symbol
names, the [second candidate run](suite/reports/assembler-arena-simd-aarch64-2026-10-15.json)
reduced Lua's in-process assembly time by about 15% and summed CLI time by
about 5%, relative to the allocation-reduced baseline. All 37 inputs produced
identical objects. Arena scratch covers fixup queues, label order, debug rows,
frames and CFI data; independently owned outputs, ordered-map nodes, and some
parser/writer buffers remain heap allocated. The allocation audit is at the
top of `crates/keel-asm/src/parser.rs`. Separate
[memory measurements](suite/reports/assembler-arena-memory-aarch64-2026-10-15.json)
show the debug-heavy input's peak RSS increasing by about 0.6 MiB.

The [mapped-input comparison](suite/reports/assembler-mmap-aarch64-2026-10-15.json)
found no clear overall warm-cache speedup before these parser changes. These measurements
do not establish cold-storage behavior or benefits on larger applications.

For synthetic large-file scaling checks:

```sh
python3 suite/generate_assembler_scale.py --output-dir target/assembler-bench/scale --mib 64 256
python3 suite/measure_assembler_io.py --compiler target/release/keel \
  --read-compiler target/assembler-bench/pre-default-read-keel \
  --input-dir target/assembler-bench/scale --input-group synthetic-scale \
  --work-dir target/assembler-bench/scale-objects --rounds 11 --warmups 1 \
  --output target/assembler-bench/scale-report.json
```

The [64/256 MiB scaling run](suite/reports/assembler-mmap-scale-aarch64-2026-10-15.json)
found less than 1% paired end-to-end speed change on both instruction-dense
and function-heavy inputs. The function-heavy 256 MiB case had a small
measurable gain; the instruction-dense case's interval included no gain.
The [memory check](suite/reports/assembler-mmap-scale-memory-aarch64-2026-10-15.json)
showed a larger benefit: macOS peak memory footprint dropped by approximately
the input file size, while peak resident size stayed similar because mapped
file pages still count toward residency. These generated files are scaling
inputs, not complete applications; the measurements use a warm filesystem cache.

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
