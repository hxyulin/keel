# Compiler bridge for the six measured families

`recurrences.c` contains six independent header-free functions named
`bridge_<family>` (hyphens become underscores). Each corresponds to both
`arm-more-<family>` and `x86-more-<family>` in `../COVERAGE.md`: madd, rotate,
udiv10, branch-select, sext8 and narrow-load. These ordinary C loops reproduce
semantics, not the hand-written assembly baseline or candidate layout. The
four arguments have the same register ABI as the measured kernels on the two
LP64 targets. The unsigned 64-bit type is `unsigned long long`; a compile-time
size assertion rejects incompatible targets. No signed overflow, signed
conversion, variable out-of-range shift, OS header or inline assembly is used.
At zero iterations every function returns seed without accessing data. For
nonzero memory cases provide 256 allocated immutable ordinary 64-bit words,
mask <=255; other families ignore data/mask. Arithmetic wraps modulo 2^64.

This directory is an independent source/capture interface. It does not alter
the manifests, oracles, ASM, harness schema, compiler or optimization pipeline.
There is no benchmark, native execution, cost simulator, automatic build,
installation or optimization-success requirement.

Source-only checks (small; avoid running them during timing campaigns):

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest discover \
  -s suite/pattern-lab/compiler-cases -p test_bridge.py -v
```

The tests interpret the actual bounded C grammar and compare with the existing
independent mathematical definitions, including wraparound, zero/noaccess,
all sign bytes and every memory index. They exercise parsing and function
boundaries with synthetic instruction snippets. They do not certify C parsing,
machine encodings, ABI execution or compiler correctness.

To capture compiler output, supply an existing executable and a JSON identity
file with nonempty `sha256`, `revision`, `tree`,
`build_command` and `toolchain` fields. Use full commit/tree and exact build
argv, feature/flag/environment/toolchain identities; these are externally
supplied provenance, not information inferred from an adjacent checkout.
The script verifies the executable SHA-256 before and after capture. Do not
supply a launcher/wrapper that hides another binary. No tool is auto-selected.

```sh
python3 suite/pattern-lab/compiler-cases/capture.py --execute \
  --compiler /absolute/path/to/keel --identity /absolute/path/identity.json \
  --output suite/reports/pattern-lab-bridge/capture-001
```

The output must be a new directory with an existing parent. Eight sequential
compiler commands run in fixed target/raw-optimized/IR-assembly order. Per-command
deadline is 20 seconds (explicit 0.05..30 allowed), total compiler budget 180
seconds. No shell, build, assembler, linker or native kernel runs. The exact
header/macro-free source bytes are retained as `.i`, bypassing preprocessing,
SDK discovery and OS headers for both target ABIs. This is deliberately a
frontend/codegen bridge, not a test of preprocessing. Both raw and optimized
Keel IR and both assembly tiers are retained for each target. Commands,
stdout/stderr bytes (including failures/timeouts), artifact hashes, caller
identity, host and source checkout identity are retained in `report.json`.
Keep the executable and sources unchanged during capture. The timeout reaps
the direct compiler child; only supply a direct compiler executable with no
wrapper descendants. An interruption can leave a partial directory; preserve
it and any artifacts, never overwrite it or claim it completed.

Per-function reports use `supported` for an observed candidate-like instruction
feature, `missed` for a baseline-like feature without a recognized candidate,
`alternative` for parsed code with neither, and `unsupported` for unavailable
emission/parser evidence. These are descriptive features, **not** an instruction
operand/dataflow equivalence proof or a finding that an optimization ought to
fire. Raw instruction lines and complete assembly are retained for independent
review. For example UMULH/MUL alone does not prove correct division magic;
CSEL/CMOV alone does not establish the exact measured selection. Loop branches
alone cannot establish the branch-select baseline; an unrecognized x86 branch
form remains alternative. ARM and x86 ROR need an exact immediate operand 13 (decimal or hex),
with Intel/AT&T operand order distinguished. Keel ARM semicolon IR comments
are excluded from instructions. Local `.L...` and `L<function>_...` labels
are recognized; unknown nonlocal labels fail closed before directive handling. Canonical rotate,
constant division and sign extension may already optimize before the nominal
selection tier; inspect retained IR and operands before assigning a cause.
The parser targets Keel GAS output and recognizes a small Intel/AT&T vocabulary;
unknown syntax stays explicit. Instruction counts cover the complete function,
including prologue/loop/return, not just the measured body. Object code sizes
are unavailable without assembler evidence and are recorded as null; x86 byte
sizes must never be inferred from instruction counts.

No measurements are embedded here. Join `measured_id` to the retained
campaign records, preserving order strata, intervals, exact compiler
and source identities. Assembly shape observations do not transfer timings
from a hand-written kernel to generated loops. Register allocation, loop
control, unrolling and layout can differ. Branch outcome predictability is
uncharacterized. Narrow-load becomes index zero after its first iteration
with mask255, uses warmed data, and only equates low-byte/word-mask loads on
little-endian ordinary immutable fully allocated memory. Volatile, atomic,
concurrent, device and fault-sensitive memory are outside that equivalence.
Micro gains do not establish application gains; whole-application correctness,
code size and performance validation remain separate work. Clang comparison
is deferred: any later comparison must use these same source bytes and target
ABI, with separately retained tool identity/flags, rather than another native
benchmark harness.
