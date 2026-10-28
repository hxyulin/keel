# Clobber-aware inline assembly

X86 inline assembly preserves the GPR homes its lowering sequence may
overwrite: declared register clobbers, assigned operand registers and the RAX
staging scratch. Memory and flags clobbers do not add GPRs to that set.
Saved homes occupy compact frame slots. Only affected callee-saved registers
are added to the enclosing function's prologue and unwind descriptions.

Templates without register operands, outputs or GPR clobbers need no staging
frame. This includes `asm volatile("sti" ::: "memory")` and compiler-only
memory barriers. Immediate-only templates also avoid staging. The assembler
still validates their instructions and target features.

Register inputs are staged before operand initialization. An input whose home
is preserved is read from its saved slot; other inputs use their untouched
homes. Output and flag capture precede restoration, then SSA results are
installed. The implementation retains support for tied/early outputs, fixed
register cycles, rematerialization, forced spills and reserved-stack calls.

## Optimizer integration

`InstData::memory_effects` now uses the declared asm contract:

| Declaration | Memory effect |
|---|---|
| Register/immediate operands and GPR/flags clobbers only | None |
| Read-only memory operands | Read |
| Write-only memory operands | Write |
| Read/write memory operands, or a mixture of reads and writes | ReadWrite |
| `memory` clobber or reserved-stack dependency | Barrier |

The existing `memory-forward` and `load-reuse` passes can retain facts across
register-only and read-only templates. Explicit memory writes still invalidate
their facts conservatively, including facts about other addresses. Existing
consumers such as load narrowing also receive the more precise effects.
Asm remains pinned and is never made pure, deleted by DCE or freely moved by
GCM. Explicit stack-observer and aggregate-escape restrictions remain.
Condition-code clobbers remain distinct from memory effects.

These rules follow the programmer's declarations; they do not inspect the
template to infer missing effects. GNU basic asm already receives conservative
memory/flags clobbers in the C frontend and promises GPR preservation.
See GCC's [extended asm contract](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html)
and [basic asm contract](https://gcc.gnu.org/onlinedocs/gcc/Basic-Asm.html).

## Measured scope

The same two captured Linux v6.12 inputs and flags from the
[dispatcher audit](linux-kernel-syscall-codegen-gaps.md) were compiled before and
after the change. Both sides disable optional unwind tables and use the kernel
code model and explicit kernel audit feature policy.
The [measurement ledger](x86-asm-clobber-measurements.json) records input,
compiler-source and object hashes with the observed execution counts.

| Function | Preset | Before bytes | After bytes | Reduction |
|---|---|---:|---:|---:|
| `do_syscall_64` | O2 | 711 | 412 | 42.1% |
| `do_syscall_64` | Os | 651 | 360 | 44.7% |
| `__x64_sys_getpid` | O2 | 50 | 50 | 0% |
| `__x64_sys_getpid` | Os | 53 | 53 | 0% |

These are function symbol sizes, not kernel image or runtime measurements.
The dispatcher no longer wraps `sti` in GPR saves/restores. Its Os stack
allocation falls from 152 to 48 bytes, excluding register pushes. Copies and
address folding in getpid remain separate work. All four new objects return
zero from the pinned objtool invocation; the sys unit still produces a decode
warning also observed on both pre-change objects. No full kernel rebuild or
boot qualification is claimed for this optimization.

The runtime fixture covers unsigned boundary values, tied outputs, memory
read/write operands and canaries, flags results, constant rematerialization,
live clobbered homes, and independent checks of all five non-frame SysV callee-saved
GPRs. Eighteen generated variants passed native x86-64 Linux execution; a broken
callee-save control returned the expected failure. Assembly text, typed objects
and chunked objects agree for every variant.

The existing 57-function GNU C inline-asm fixture also passed native execution
for all 145 fresh objects against GCC, with all 17 broken controls rejected
normally. This covers fixed-register cycles, early clobbers, narrow views,
multiple outputs, all sixteen flag conditions, macros and memory aliases in
optimized/unoptimized, debug/non-debug and normal/empty-pool configurations.

Workspace validation completed with 3,046 passed, zero failed and 15 ignored
across 581 reported groups. All-target/all-feature checking, documentation and
scoped formatting checks passed. This working-tree run includes the pre-existing
frontend edits in `lex.rs`, `profile.rs` and `strings.rs`; those edits are outside
this optimization's checkpoint. Focused Clippy passed with existing library
warnings. All-target Clippy is blocked by the existing `almost_swapped` diagnostic
in `keel-ir/tests/call_operand_abi.rs`; warnings-denied Clippy also stops on
existing library findings.

Reproduce the export and execution on x86-64 Linux with:

```sh
KEEL_ASM_CLOBBER_EXPORT_DIR=/tmp/asm-clobber-objects \
  cargo test -p keel-codegen --test x86_asm_clobbers
for object in /tmp/asm-clobber-objects/*.o; do
  cc -O2 -Wall -Wextra -Werror \
    crates/keel-codegen/tests/x86_asm_clobbers/harness.c \
    crates/keel-codegen/tests/x86_asm_clobbers/observer.S \
    "$object" -o /tmp/asm-clobber-check
  /tmp/asm-clobber-check || exit
done
```

Preservation still includes affected homes without consulting SSA liveness.
Allocator-aware operand placement could remove more staging and copies, but
needs explicit internal sequence constraints. Function scheduling, outlining,
general instruction selection and hidden architectural effects are unchanged.
