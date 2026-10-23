# Bounded application-gap research at dbe0b1f6

Research base: `dbe0b1f65f8ed0a18d84e27fe8644529a0d64810` (2026-10-23).
This checkpoint changes documentation only. It provides two ranked implementation
hypotheses, with static evidence, for the benchmark owner to assess. It does not
establish which transformation gives the largest current runtime improvement.
No new timing campaign, application profile, production change or default change
was made. Main and other worktrees were not modified.

## Evidence boundary and blocked inputs

The frozen LinuxHost release compiler is SHA-256
`0a67deaa510b5eb75d03aefcd46096f5187f0e5ef0dd0e638ababfe2aff1ebcd`.
All 1,985 source blobs in its `base-manifest.json` were checked against their
Git blob hashes. Existing build receipts record default release features,
`CARGO_CACHE_LINKS=0`, `suite/fast_opt_locked.py`,
`cargo build --offline --locked -j2 --release -p keel --bin keel
--example width_comparison`. No rebuild was needed for this research.

Fresh native Linux **raw source** attempts for Lua 5.4.7 `lvm.c`, QuickJS
2024-01-13 `libregexp.c`, and SQLite 3.46.1 `sqlite3.c` all reject the glibc
`__TC__` declaration at `bits/floatn.h:83:54`. These used the original definitions
recorded in [the header investigation](c-system-header-compat.md), including
SQLite's existing `LONGDOUBLE_TYPE=double`. Separately, retained Linux
**Clang-preprocessed inputs** for `lvm`, `quickjs`, and `sqlite3` reject `__TC__`
at column 52. Those frontend failures occur before application optimization.
The documented GCC `_FloatN` blockers remain separate; this round did not rerun
that GCC preprocessing matrix. Unsupported aggregate value ABI rejection in
[C unsupported-value safety](c-unsupported-values.md) must remain fail-closed.

Unmodified retained Darwin-preprocessed inputs were also tried directly through
the x86 C frontend. All three reject the SDK `math.h:223:16` comparison with
`invalid operands to ==`. They are therefore **not working x86 C inputs**.
No headers, application source or identity macros were edited to bypass these
failures. The compatible-input boundary is current Darwin frontend acceptance:

- `lvm.i`: SHA-256 `1642463467c055deb286d79cdef886f516814fcd009c935f58e588c7d7639a1b`.
- `quickjs.i`: `877a28259c32f1eb9692ecf441eb440abb74477e41b9cc0ce3004cf36b2d6755`.
- `sqlite3.i`: `13dc74cfc684571ee5334df5890a955bb428ff7105f3d6415bee0951184dc6c7`.

To obtain a bounded **diagnostic replay**, the current Darwin frontend emitted
unoptimized IR from these exact bytes. The current LinuxHost compiler then ran
`opt --opt-tier full --target x86_64-unknown-linux-gnu --cpu-level v3`, with
explicit v1 controls, followed by `compile` with the same level and full
selection/allocation. This is current x86 Full optimization and assembly
lowering of foreign-target frontend IR, not successful native x86 C compilation.
Darwin type/layout/frontend choices remain in that input; no native execution
or cross-target ABI compatibility is inferred. Earlier target-default replay
outputs are retained separately and are not the explicit-level evidence.

The matching Clang 23.1.1 references compile the same `.i` bytes with
`-O2 -ffp-contract=off -fblocks -march=x86-64-v3`, and `-march=x86-64` controls.
`-fblocks` accepts retained Darwin header declarations; no source is changed.
The FP-contraction control is explicit and is not a new stock-Clang timing
comparison. Only three application functions were inspected: `JS_CallInternal`,
`luaV_execute`, and `sqlite3FpDecode`. Clang inlines the last function completely;
its separate-function comparison is unavailable. A header-free digit-loop
reduction supplies an additional matched scalar Clang reference.

All compiler work used the shared lock. The first LinuxHost job waited 122.6 seconds
for other work; that is scheduling metadata, not compilation latency. Generated
assembly establishes lowering only: this research did not assemble/decode new
objects, run native oracles, or rerun release legality tests. Production feature
checks stayed enabled. The [maintained ISA contract](userspace-isa-status.md),
[C intrinsics](c-x86-intrinsics.md), and [GNU vectors](gnu-vector-mode.md)
remain the legality/ABI boundaries.

## What the bounded inspection supports

Historical LinuxHost profiles from compiler `6e6d2c2` were verified against the retained
archive and manifests: 436 archive, metadata, capture and binary hash checks.
They prioritize the VM functions and decimal formatting only. For example,
those old captures place Lua fib's `luaV_execute` near 79–80%, QuickJS fib's
`JS_CallInternal` near 77%, and SQLite queries' `sqlite3FpDecode` near 5–6%.
They do **not** describe the current hot distribution or predict removable time.
Old helper self-costs cannot be assigned to helpers after intervening inlining.

Current diagnostic Full IR has 956 blocks, 1,646 loads and 954 stores in
`JS_CallInternal`; `luaV_execute` has 860 blocks, 660 loads and 256 stores;
`sqlite3FpDecode` has 60 blocks, 30 loads and 29 stores. These counts agree
between explicit v3 and v1. Declared slots (876, 22 and 3 respectively) are not
allocator spills or live frame sizes. The corresponding stack subtraction
immediates are 4,392, 280 and 40 bytes. Matching Clang VM prologues subtract
840 and 136 bytes; different private calling conventions, spills, stack
protection and layout prevent treating those differences as removable bytes.

There is no packed numeric loop in these inspected regions. The actual SQLite
loop `while(v) { p->zBuf[i--]=(v%10)+'0'; v/=10; }` has a quotient recurrence
and a data-dependent digit count. Its useful immediate gap is scalar arithmetic
reuse. Its surrounding scaling loops and Dekker arithmetic carry FP dependencies;
packing/reassociation/contraction would require separate semantic justification.
The actual Lua float fixture additionally has interpreter dispatch, library
calls and an ordered sum. Availability of AVX2 does not establish useful SIMD
activation there. No additional application corpus with a demonstrated hot,
independent numeric array loop was qualified in this bounded round.

## 1. Remove repeated reloads of the same compiler spill locally

**Class:** backend emission/register-allocation consequence, not SIMD or missing
loop recognition. **Owners:** `crates/keel-codegen/src/x86_64.rs`, a small typed
helper near `x86_64/output.rs` if appropriate, and focused codegen regressions.
Do not implement an assembly-text rewriter or redesign allocation.

Current `JS_CallInternal` assembly, block662, contains:

```asm
mov rax, qword ptr [rbp + -4120]
mov rsi, rax
add rsi, 1
mov rax, qword ptr [rbp + -4120]
movzx edi, byte ptr [rax]
```

`luaV_execute` block813 has the same pattern at `[rbp + -264]`, with an increment
of four and a subsequent dword instruction fetch. The first frame read already
supplies the address for both operations. Clang's corresponding dispatch region
keeps the instruction pointer in `%r12`, using `leaq 4(%r12)` and `movl (%r12)`.
This is a code-shape comparison, not instruction-by-instruction PC attribution.

A deliberately narrow assembly screen found 161 such same-register/same-frame
repeat-read candidates in the QuickJS VM and 48 in Lua, at both v3 and v1. It
admits only up to eight intervening pure register operations, with no labels,
calls or memory operations. These are syntactic candidates, not typed spill
admissions, dynamic reload counts or a measured gain. They are independent of
the held byval-source forwarding experiment.

Start with one remembered **compiler-owned spill** read in one emitted basic
block. Suppress a later identical-width read into the same scratch register
only while typed register effects prove that register and spill unchanged.
Invalidate on every partial-register clobber, label/edge, unknown effect, call,
assembly/raw region, frame/stack operation or relevant write. Route text, typed
and chunked output through the same decision. Keep the initial pilot to a short
window and a fixed state bound; no CFG dataflow, PHIs or memory motion.
`reload-step.keel` in the external artifacts is a small input for forced-spill
unit pools; it is not claimed to force spills under the ordinary allocator.

**Gates:** distinguish compiler spills from user/debug/volatile/atomic homes;
check RAX/EAX/AX/AL/AH clobbers and flags, branch joins, calls, spills/results,
frame observers and source boundaries. Keep the original first successful
8-byte read and the original byte/dword application load, store/call order and
ABI transport. No wider read, new speculative access or FP/MXCSR change is
allowed. Require strict v1/v3 feature checks, release guard, normal/empty pools,
both allocators and text/typed/chunked byte parity, then native finite oracles.
Only after activation should the benchmark owner compare unchanged-input
QuickJS/Lua fib and sort, compile cost and a negative control. The 209 VM screen
hits motivate the pilot; their runtime value is unmeasured.

## 2. Deduplicate reciprocal arithmetic created by late cleanup

**Class:** scalar optimizer/lowering cleanup. **Owners:**
`crates/keel-opt/src/instcombine.rs`, focused optimizer regressions, with the
caller of late cleanup in `private_aggregate.rs`/the Full schedule inspected as
needed. Do not add another unconditional whole-module GVN round on this evidence.

In current `sqlite3FpDecode` Full IR, block59 computes the high product by
`0xcccccccccccccccd` and shifts by three to form the remainder's quotient,
stores one digit, then recomputes the identical high product and shift for
`v/=10`. The emitted loop contains two scalar `mul` instructions at v3 **and**
v1. This is not missing constant-division lowering: that lowering already exists.
An explicit diagnostic `gvn,dce` on the same application Full IR reduces the
function from two high multiplies to one and from 524 to 514 emitted instructions.
That latter delta includes any other GVN cleanup and is not isolated to one rule.
No time or object-size saving was measured.

The header-free `decimal.c` reduction already gets one high multiply under
ordinary Full; Clang v3 uses one `mulx`, and its v1 control uses scalar baseline
instructions. Thus there is no claim that every default digit loop misses this
optimization. `late-digit.keel` isolates remainder-before-division lowering:
`opt -p instcombine` produces two high multiplies; `instcombine,gvn,dce` produces
one. Full has instcombine/conditional aggregate cleanup after its final GVN,
which is a concrete scheduling risk, but the exact pass that recreates this
application duplicate was not traced. Trace that activation before implementation.

A bounded follow-up can reuse exact, already computed pure reciprocal/shift
expressions within the affected block, or arrange cleanup only for functions
whose late lowering introduces them. Preserve finite work caps and avoid
unrelated schedule or inlining-policy changes.

**Gates:** exact type/width/operands and modular arithmetic; unsigned values
0/1/9/10/UINT64_MAX and randomized native digit strings; signed widths and
negative divisors only if deliberately admitted. Never introduce/move a possibly
trapping division or change zero/MIN-divided-by-minus-one behavior. Preserve
all stores, their byte widths, alias behavior and fault order. Do not change
SQLite FP scaling, Dekker operations, NaNs, signed zeros, rounding modes,
exception behavior or contraction. Keep the same ABI and v1/v3 feature guard;
no BMI2 requirement may leak into v1. Native application correctness and a
locked benchmark are still required before a payoff claim.

## Recommendation and receipts

For this round, spend implementation effort first on the small backend reload
pilot, with the scalar late-cleanup investigation second. Keep header/type/ABI
compatibility work a separate, explicit frontend project. Do not promote the
historical byval pilot on its preserved 929-load static reduction: final-source
runtime benefit remains unproven. Likewise, the prior large/late promotion
experiment does not establish a runtime win. Neither is a fresh missing-feature
finding here. SIMD/AVX-512 expansion is not justified by these VM observations.
There is no current app geomean, speedup ranking by measured runtime, or native
application-compatibility claim.

Local and LinuxHost artifacts: `/tmp/keel-dbe0-app-gap-research/`. Main receipts are
`current-provenance.json` (local copy of the initial LinuxHost probe),
`frontend-commands.json`, `explicit-replay-commands.json`,
`replay-commands.json` (raw-source failures and Clang references),
`function-stats.json`, selected function `.ir`/`.s` files, `spill-screen.json`,
`decimal-commands.json`, `reducer-commands.json`, `historical-verification.json`
and lock JSONL files. The original build source/receipts remain at
`/tmp/keel-round-dbe0-research-current` and its adjacent build logs on each host.
Scripts, input bytes, generated IR/assembly, binaries, profiles and reports are
outside Git. Rust compilation/tests were not needed for this documentation-only
change; static probe success is not a production correctness gate.
