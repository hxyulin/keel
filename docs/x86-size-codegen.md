# x86 size emission

`keel cc -Os` and `keel compile -Os` select size-oriented x86 emission in
addition to the Size IR pipeline. The target-specific changes are:

- Omit implicit 16-byte function alignment and 32-byte loop alignment. Explicit
  function alignment remains a lower bound.
- Omit RBP setup in scalar C leaves with no calls, stack storage, callee-saved
  registers, stack arguments, debug information or stack observers. The existing
  SIMD/private-leaf eligibility checks also apply here.
- Omit unused call-alignment padding in leaves with no calls, local storage,
  stack observers, TLS helper, debug information or stack realignment. Saved
  registers still receive their normal pushes, pops and unwind descriptions.
- Use one-byte `leave` instead of `mov rsp, rbp; pop rbp` when no other
  callee-saved registers need restoring.
- Share a return epilogue when the function saves registers and has multiple
  returns. Each redirected return replaces at least seven bytes with a jump
  of at most five bytes. One existing return owns the shared sequence; no helper
  call, new stack frame or extra epilogue copy is introduced. Return values and
  any AVX transition instruction are prepared before the jump.

Leaf-frame removal can shorten frame-pointer-only backtraces through those
leaves. These choices can trade runtime for size. Other optimization presets keep their
existing emission. The last `-O` preset selects the codegen goal; `--opt-tier`
and `-p` override IR passes independently. For example, `-Os --opt-tier none`
keeps size emission, whereas `--opt-tier size` alone changes only IR passes.
Selector and allocator overrides do not reset the size goal. The goal is not
serialized in IR; apply it again when compiling an IR file.

Library callers set `EmissionOptions::goal` to `CodegenGoal::Size` and use the
existing emission-aware compilation entrypoints. The default remains
`CodegenGoal::Speed`. AArch64 currently has no additional goal-specific emission.
Unwind emission and code models remain independent.

## Stack alignment and outlining

Call sites retain the existing SysV alignment and argument layout. Avoiding
padding in a register-only leaf does not relax the call ABI: the leaf makes no
calls and has no aligned local storage or stack observer. Removing arbitrary
16-byte alignment at private call boundaries would need a separate contract.
The proof must follow every direct and indirect call, stack argument, aligned
spill, TLS helper, intrinsic helper, variadic operation and assembly observer.
A private function that calls external code would still need an aligned call.
Reducing a stack adjustment from 16 bytes to 8 often keeps the same instruction
width. Eliminating an unused adjustment removes an instruction; a relaxed ABI
can instead introduce alignment fixups at its boundaries.
The [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex)
defines those boundary requirements.

Return sharing is a bounded tail merge, not a general machine outliner.
General outlining replaces repeated sequences with helper calls. It needs
encoded-byte costs for the sequence, calls, register preservation and helper
frame. It also needs a contract for implicit registers, flags, stack-relative
addresses, debug/unwind metadata and raw assembly. Keel currently streams one
function at a time, so cross-function matching would need a planning stage or
bounded buffering. LLVM's
[machine outliner cost model](https://llvm.org/doxygen/MachineOutliner_8h_source.html)
likewise includes call and frame overhead.

The next contained candidates are tiny/single-use private inlining with a byte
budget, broader equivalent CFG-tail merging, and shorter instruction forms
where the compiler proves flag and register-width effects irrelevant. General
outlining and relaxed private stack alignment are separate backend work.

## Measurements and validation

Four preprocessed C inputs from the pinned Linux v6.12 configuration were
compiled before and after the size-emission change. Both sides used `-Os`, the
kernel code model, the explicit kernel feature policy and disabled optional
unwind tables. The Size IR pipeline, selector and allocator were unchanged.
Counts sum executable sections, including section padding, in each object.
This isolates the new backend choices; it is not a comparison with GCC or a
whole-kernel size, boot or runtime result.

| Kernel C unit | Before, bytes | After, bytes | Reduction |
|---|---:|---:|---:|
| `arch/x86/entry/common.i` | 733 | 665 | 9.3% |
| `kernel/fork.i` | 47,613 | 43,787 | 8.0% |
| `fs/namei.i` | 66,489 | 60,041 | 9.7% |
| `fs/pipe.i` | 20,939 | 19,568 | 6.5% |
| Total | 135,774 | 124,061 | 8.6% |

The [measurement ledger](x86-size-codegen-measurements.json) records source and
object hashes, flags and the baseline checkpoint. No timing improvement is
claimed. A full kernel rebuild, objtool review and boot qualification remain
necessary before replacing the earlier qualified kernel image.


The regression suite checks all six stable selector/allocator combinations,
text/typed/chunked object parity, explicit alignment, stack observers, stack
arguments, a saved-register-only leaf and a return crossing a near jump.
The native x86 fixture additionally checks results for 201 inputs, all
callee-saved GPRs and the requirement that RSP + 8 is 16-byte aligned on
callee entry. Its wrong-result control
must exit normally with status 1.

To export objects and run that fixture on x86-64 Linux:

```sh
KEEL_SIZE_EXPORT_DIR=/tmp/keel-size-objects \
  cargo test -p keel-codegen --test x86_size
for object in /tmp/keel-size-objects/*.o; do
  case "$object" in */padding.o) continue;; esac
  gcc -O2 -Wall -Wextra -Werror \
    crates/keel-codegen/tests/x86_size/harness.c \
    crates/keel-codegen/tests/x86_size/observer.S \
    "$object" /tmp/keel-size-objects/padding.o -o /tmp/keel-size-check
  /tmp/keel-size-check || exit
done
```

The workspace run passed 3,006 tests with zero failures and 15 ignored. Final
iterator/test lint cleanup was followed by the four size regressions, the
three preset tests and exact identity checks for the eight native-qualified
fixture objects and four measured kernel objects. All 16 sampled objects from
non-size presets were byte-identical to the earlier compiler's output.
Workspace all-target/all-feature checking and codegen documentation passed.
Strict Clippy remains blocked by pre-existing linker and IR diagnostics;
ordinary codegen Clippy completed without findings in the added code.
The full test run used a command-local single-codegen-unit override after the
normal profile encountered unresolved compiler-internal constants at Rust test
linking. No persistent build or toolchain setting changed.
