# Bounded scalar if-conversion

`if-convert` is an opt-in function pass. No default pipelines, allocator modes
or existing compatibility options change. It admits a branch whose two distinct,
single-predecessor arms end in jumps to the same join. Each arm may be empty.
Both arms together may have at most eight speculated instructions and the join
at most eight parameters. A conditional with two argument-carrying edges directly
to the same join is also supported. Entry/self-target arms and a join equal to
the decision block are rejected.

The pass substitutes incoming arm parameters simultaneously, moves the admitted
instructions before the decision, and selects each outgoing join argument using
the original nonzero condition. Identical arguments need no select. Signedness
and width remain in the original integer operations/comparisons; arithmetic still
wraps. Moved instructions retain source locations. Canonical integer/pointer
parameters and canonical instruction result definitions are required. Arm
values used outside their own arms are excluded, including unreachable users.

The explicit speculation allowlist contains integer constants, compares, selects,
wrapping add/subtract/multiply, integer logic, masked shifts/rotates, high multiply,
integer bit-count/negate/swap operations and integer extensions/truncations.
Loads, stores, atomics, calls (including returns-twice and indirect calls), division
and remainder, floating operations, frame operations, inline assembly, exception
operations and traps are excluded. New IR opcodes do not enter implicitly.

## Integration recommendation

For Full, evaluate `inline,small-memory,simplify-cfg,stack-promote,phi-simplify,dce,
if-convert,simplify-cfg,canonicalize,instcombine,phi-simplify,dce` as a segment in
the combined schedule. Inlining and intrinsic initialization expansion expose
private fields; phi cleanup exposes small scalar diamonds; cleanup after
conversion removes redundant parameters and jumps. Root owns integration with
other workstreams. Keep both new global passes out of Fast until compile cost and
register pressure have been evaluated.

No vectorizer or schedule is edited here. Select formation is sufficient for
branchless scalar backend code, but general multiple-accumulator vector reduction
and select/mask vectorization are further work. Actual `k_stackobj` additionally
needs the escaped-field region/alias proof described in `stack-promote.md`.

## Validation and activation

The combined reduced private fixture removes six field loads and six field
stores (including initialization), leaving one input load, and reduces two
conditional branches to the single loop-exit condition. ARM assembly changes
`b.lt` to two `csel`; x86 static assembly uses two `cmovne` instructions. ARM's
current allocator adds one callee-save `stp`/`ldp` pair after conversion, so this
is elimination of field traffic, not elimination of all stack traffic or proof
of a runtime gain. No performance measurements were made. The actual corpus
kernel remains at seven loads, three stores, and two conditional branches.

Final optimizer validation: `cargo test --offline --locked -j2 -p keel-opt`
passes 339 tests.
Focused `--test stack_promote` has nine tests and `--test if_convert` eleven.
Coverage includes all four integer widths, wrapping, signed/unsigned compares,
nonzero condition polarity, simultaneous edge permutations, empty arms, duplicate
edges, pointers, alias-definition rejection, source locations, bounded work,
partial initialization, debugger homes, escapes, volatile/atomic accesses,
exception/returns-twice/frame barriers and untaken trapping division.

Build the CLI using `cargo build --offline --locked -j2 -p keel`, then run the
validator command documented in `stack-promote.md` with `--if-convert` and
`--out target/stack-field-evidence/stage2`. All compiler/native commands serialize
through the supplied shared lock and use the worktree-local target directory.
The validator performs 84 serial commands:

- ARM64 and x86-64 assembly/object emission for five private-loop variants and
  four diamond/guard variants; both targets also emit whole-corpus assembly.
- 10,401 independent unsigned-C private-loop cases for each of five ARM variants.
- 90,000 independent width/edge/guard cases for each of four ARM variants, plus
  three taken native traps per variant (division by zero, signed division
  overflow, and null load). Untaken null loads and division by zero return
  normally. The guarded load/division branches remain conditional.
- Fresh Clang O2 whole-driver reference with `-ffp-contract=off`, matching all
  795 lines for Fast+both-passes and Full+both-passes.

Only local ARM64 macOS executes native code. x86 validation is static only.
Exact argv, outputs, activation counts and artifact hashes are under the ignored
`target/stack-field-evidence/stage2`; `arm-instructions.json` records field traffic,
conditional selects and callee-save pairs. The shared append-only command log is
`target/kernel-gap-implementation-20261020/lock-events.jsonl`.
