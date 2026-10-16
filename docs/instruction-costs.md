# Instruction costs and optimization profitability

The assembler supplies an inventory of every encoding Keel supports and exact
instruction sizes. It does not supply CPU execution timings. Those depend on the
processor, operand form, dependencies, register allocation and memory behavior.
This infrastructure reports these quantities separately; optimizer defaults do
not change as part of adding it.

## Data sources and coverage

`tools/instruction_metrics.py catalog` derives all 534 current encoding forms
from the checksum-pinned Arm specification in `crates/keel-asm/isa/a64.json`.
Every entry has its encoding mask and exact four-byte length. Timing fields are
null unless an explicit CPU model supplies them. Mask matches are candidates,
not a semantic disassembler or permission to assign one cost to all operands.

A modeled catalog constructs one representative word per form, disassembles it
with LLVM, checks its exact round-trip through Keel, then obtains latency,
reciprocal throughput, micro-ops and resource information from
[llvm-mca](https://llvm.org/docs/CommandGuide/llvm-mca.html). The report includes
CPU, tool versions and hashes, specification hashes, samples and explicit reasons
for missing data. A sample is not coverage of all operand variants. In
particular, zero-register idioms, writeback addressing, conversions and vector
arrangements can change costs. Calls, barriers and atomics cannot be assigned
ordinary ALU costs when the scheduling model lacks their behavior.

LLVM 23.1.2 recognizes `apple-m5` but selects **CycloneModel** in its
[processor definitions](https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/lib/Target/AArch64/AArch64Processors.td).
These are provisional scheduling estimates, not measured M5 latencies.
The checked-in catalog
has usable model metrics for 358 representative forms and explicit gaps for 176,
mostly LSE atomics, barriers and call/return behavior. Keep this distinction
when using it. Hardware
calibration needs separate dependency-chain and independent-stream experiments,
including operand variants, timer overhead, CPU identity and repeated samples.
Memory measurements need explicit cache, alignment and alias conditions.

## Generating and comparing metrics

Build the driver with `cargo build -p keel --release`. The LLVM utilities are
optional offline analysis dependencies; code generation and assembly use Keel.
On a keg-only Homebrew installation, pass their absolute paths:

```sh
python3 tools/instruction_metrics.py -o /tmp/encodings.json catalog
python3 tools/instruction_metrics.py -o /tmp/apple-m5.json catalog \
  --cpu apple-m5 \
  --llvm-mca /opt/homebrew/opt/llvm/bin/llvm-mca \
  --llvm-mc /opt/homebrew/opt/llvm/bin/llvm-mc

# Exact metrics on an assembly file, including final branch/address fixups:
target/release/keel asm-metrics input.s -o /tmp/exact.json
python3 tools/instruction_metrics.py -o /tmp/before.json analyze before.s
python3 tools/instruction_metrics.py -o /tmp/after.json analyze after.s
python3 tools/instruction_metrics.py compare /tmp/before.json /tmp/after.json \
  --objective size

# For a small straight-line region, add CPU estimates to each report:
python3 tools/instruction_metrics.py -o /tmp/before.json analyze before.s \
  --cpu apple-m5 --llvm-mca /opt/homebrew/opt/llvm/bin/llvm-mca
python3 tools/instruction_metrics.py -o /tmp/after.json analyze after.s \
  --cpu apple-m5 --llvm-mca /opt/homebrew/opt/llvm/bin/llvm-mca
python3 tools/instruction_metrics.py compare /tmp/before.json /tmp/after.json \
  --objective speed
```

Assembler metrics record source instruction offsets and final words; alignment
padding, text data and other sections do not become source instructions. Exact
text size still includes text padding/data. CPU analysis rejects regions with
extra text bytes, and requires one MCA region with the same instruction count.
It uses `-noalias=false`, refuses CPU identity mismatches, preserves diagnostics,
and declines a speed score for warnings, unmodeled side effects or missing
throughput. The raw resource table is retained for diagnosis, including MCA's aggregate
row (instruction index equal to the instruction count); do not count that
row as another instruction. Timing is never
silently substituted with a guessed default. Out-of-order model estimates are
not proof of semantic equivalence or of hardware speedup.

## The cost function

Keep costs as separate quantities before choosing an objective:

- **Size:** final emitted text bytes, including encoding expansion and padding.
- **Speed within a region:** modeled cycles per iteration of that region,
  accounting for the dependency graph and execution resources.
- **Register pressure:** a constraint before allocation; actual spill/reload and
  move instructions are included when measuring code after allocation.
- **Compile time:** measured separately as a budget for the optimization itself.

The comparison tool implements two explicit objectives: minimize bytes for
`size`, or minimize estimated cycles per iteration for `speed`, requiring the
same CPU, tool binary and simulation iteration count. It never adds instruction
latencies to claim an execution time and does not collapse these dimensions
with arbitrary weights. Simulated iteration costs include warm-up effects;
use sufficiently many iterations and consistent regions when comparing them.

For whole-function profitability, the intended speed objective is
`sum(block_execution_count[b] * estimated_cycles[b])`, using measured execution
counts where available. This is an approximation: dependencies across blocks,
branch prediction, cache misses and calls require additional treatment. Loop
depth is not an execution count, and summing instruction reciprocal throughputs
is not a resource-contention model. Spills already present in the measured
sequence must not be charged twice.

IR transformations need a target-cost interface describing their prospective
lowering. One IR multiply may become one instruction, multiple instructions, or
a helper call; a constant may fold into an immediate or need several moves.
Instruction selection can fuse operations or encode addresses, and allocation
can erase apparent savings through spills. A single target-independent cost per
IR opcode cannot express these cases.

Use the reports first to diagnose measured hot regions and compare actual
lowerings. Next wire calibrated target estimates into a specific profitability
decision, such as rematerialization versus retaining a value or a strength
reduction that changes register pressure. Require correctness tests and paired
application benchmarks for that change. The provisional LLVM timing tables remain diagnostic. The native M5 tuning
section below describes the first live decision based on hardware calibration.

## Validation

```sh
cargo test -p keel-asm
python3 -m unittest discover -s tools/tests -v
KEEL_METRICS_BINARY="$PWD/target/release/keel" \
LLVM_MCA=/opt/homebrew/opt/llvm/bin/llvm-mca \
python3 -m unittest discover -s tools/tests -v
```

Parser/model tests use explicitly synthetic numbers. The optional integration
check runs the real assembler and LLVM model, including rejection of an unknown
CPU. Assembler tests check final branch fixups, text padding/data separation and
unchanged object output for all examples.

## Native Apple M5 tuning

`--tune generic` is the default. `--tune apple-m5` selects measured profitability
choices for Apple arm64 without changing the target, ABI, enabled ISA features,
IR optimization tier or instruction-selection tier. For example:

```sh
keel cc -O --isel-tier full --tune apple-m5 input.i -o output.s
```

The first choice is deliberately small: i32/i64 multiplication by 3, 5 or 9
can use `add r, r, r, lsl #k`. It requires tree selection (`--isel trees`,
`--isel all`, or the corresponding tier). The selector preserves existing
multiply-add covers, shared constants and wrapping integer semantics. Narrow
integers, i128 and other constants retain their previous lowering. An M5 tuning
request for the x86 target is rejected by both the driver and backend APIs.

The native calibration
measures 32- and 64-bit dependency chains and eight independent streams on an
Apple M5 Max. Keel assembles the generated kernels; a small C timing/checking
harness uses the system toolchain. Neither LLVM timing data nor llvm-mca is
needed. Nine randomized pairs after two warmups show about one-third lower
elapsed time in dependency chains; independent-stream medians are neutral to
slightly faster. Results are nanoseconds per operation, not inferred cycles.
QoS is requested but cores and clock frequencies are not pinned. The report
contains all samples, kernel sources, harness, tool hashes and independent
correctness checks.

`suite/generate_m5_tuning.py` emits the compact Rust cost table from this report.
The rule requires the chain ratio's upper 95% bootstrap endpoint below 1.0 and
a parallel-stream median ratio no greater than 1.0. It applies only to the six
measured width/constant combinations and saves constant materialization where
that constant has no other uses. No general instruction-latency model is
inferred from these measurements. The earlier provisional LLVM catalog remains
an offline diagnostic source; this tuning rule does not depend on it.

Reproduce the calibration and check the generated costs:

```sh
python3 suite/measure_m5_integer.py --keel target/release/keel \
  --output /tmp/m5-integer.json
python3 suite/generate_m5_tuning.py --check
```

Compare application behavior using `keel-m5` and `keel-m5-fast` suite presets.
The application comparison
uses identical IR passes and full instruction selection on both sides; the
candidate adds only M5 tuning. Both sides use Keel's assembler, with system
linking. The final application runtime geometric-mean ratio is 1.0013 (0.13% more
time), compile-time ratio is 0.9981 (essentially neutral), and combined text
shrinks by 160 bytes. QuickJS Fibonacci regresses 2.3%; QuickJS string improves
1.9%, Lua sort/string about 0.9%. These local changes do not establish an
overall application speedup. The cause of the Fibonacci regression has not
been profiled. The rule remains experimental and opt-in; generic defaults
are unchanged.
