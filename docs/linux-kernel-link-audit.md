# Pinned Linux64 link audit

Later optimizer fixes and boot experiments are recorded in the
[link fixes and runtime boundary](linux-kernel-link-fixes.md). The measurements
below describe the earlier compiler checkpoint.

The assembly-only replacement links into `vmlinux` with GNU ld and passes Linux's
exception-table sorting step. The full C replacement does not link yet. Its
optimized objects leave 22 undefined references to 21 symbols across ten units.
The [link ledger](../suite/linux-kernel/audit-link.json) records the commands,
controls, object selection, compiler witnesses and artifact hashes.

This experiment uses compiler implementation `a824b9ff`, qualified by the
source-acceptance checkpoint `36623ddb`. It keeps the captured inputs, resolved
config, reference kernel and explicit kernel-audit feature policy fixed.
No compiler, optimizer, assembler or linker implementation changes were made.

## Object selection and controls

There are 484 captured C inputs and 506 assembly inputs. Of the assembly inputs,
484 are GCC-generated versions of the captured C sources. The full variant
selects each C object before its generated assembly duplicate, then adds the
remaining handwritten assembly objects. The assembly-only variant selects all
506 assembly objects assembled by Keel.

The final archive and explicit-input lists contain 501 object inputs. The full
variant supplies 481 Keel C objects and 20 Keel assembly objects. Five captured
vDSO objects are not direct inputs to this link. Their separate vDSO link and
generated image payload remain from the reference build. Real-mode blobs and
initramfs preparation also remain external. This is not a replacement of every
build stage or every embedded code payload.

Each experiment uses separate object copies. The original reference files remain
unchanged. Thin archive copies can retain references to an earlier directory;
the first optimized retry did so and is excluded from the results. The qualified
optimized retry uses regular GNU archives. Quick append preserves the original
member order, including duplicate basenames, and symbol indexes are rebuilt.
Every archived payload is hashed against its selected object before and after
objtool. A reference control using the same archive construction reproduces the
original `vmlinux` bytes after table sorting.

The kernel's GNU linker script, host tools and GNU ld remain external. The link
uses the original whole-archive and conditional-library grouping, includes
`init/version-timestamp.o`, and runs `scripts/sorttable` only after a successful
final link. This config disables kallsyms, BTF and modules; the experiment does
not implement their additional link stages. No unresolved-symbol stubs or
ignore-errors options are used.

## Observed results

- Both reference relinks reproduce the pinned `vmlinux` SHA-256:
  `2c399397364a7240d3b63e45c9cb70e50bd5b0df66547639aed5de32132c27df`.
- The assembly-only replacement passes the partial link, final link, objtool and
  table sorting. Objtool checks 500 objects and emits no warnings. The sorted
  image SHA-256 is
  `dd37e5e847921a08ece606fc380f22a13099098de87ded1bc4c952e494fa4b2a`.
- The unoptimized C replacement passes the partial link. Its final link fails
  with 1,443 undefined references to 416 symbols, including many compile-time
  guards retained in code that optimization normally removes.
- All 484 C inputs compile with `-O`. The byte-verified optimized archive passes
  the partial link but fails the final link before and after objtool. There are
  22 undefined references to 21 symbols. No relocation overflow is observed.
- The optimized C objtool run checks 480 objects and returns zero for each, but
  emits 336 warnings across 169 objects. A zero exit code is not a clean result.
  Warnings include unreachable code, sibling-call stack frames, missing jump
  destinations, decoding and UACCESS state. The previously processed assembly
  objects remain in the optimized link.

The remaining undefined references occur in `alternative.i`, FPU `signal.i`,
`vmscan.i`, `gup.i`, `memory.i`, `mmu_gather.i`, `page_vma_mapped.i`, `page_alloc.i`,
`fs/exec.i` and `maple_tree.i`. They include ten distinct compile-time assertion
symbols, six alternative-section bounds, two FXSR conversion functions, two
huge-page helpers and `lockdep_is_held`.

An explicit `--link-intent executable` pilot compiles four of these units. It
removes the two IBT sealing bounds from `alternative.i`, but leaves its retpoline
and return bounds and the other sampled unresolved symbols. This pilot does not
replace the full optimized link result.

## Next compiler work

Small source witnesses isolate two optimization gaps under explicit executable
intent. GCC 14.4 and Clang 21.1 remove `missing` from both witnesses; Keel retains
an undefined reference. The ledger also includes an ordinary unused-static
control, which all three compilers handle.

A function can become unused after folding a caller or inlining it:

```c
extern void missing(void);
static int dead(int flag) { if (flag) missing(); return 1; }
int live(void) { return dead(0); }
```

The compiler needs reachability cleanup after those transformations. It must
preserve address-taken functions, data initializer references, aliases, file-scope
assembly references, retention roots and binding rules. Initial source-level
unused-function removal does not cover functions that become unused later.

A known empty function can leave an unnecessary argument reference:

```c
extern int missing;
__attribute__((noinline)) void empty(void *p) {}
void live(void) { empty(&missing); }
```

Removing this call does not require inlining it. It needs a checked call-effect
summary and binding contract. Side-effectful argument evaluation must remain,
and weak or interposable definitions cannot inherit the proof automatically.
The kernel's disabled-feature no-op functions expose this distinction. These
witnesses establish compiler gaps, not proof that both fixes remove every
remaining kernel reference.

Before boot work, the backend also needs an explicit kernel environment and
objtool qualification. The link reports orphan `.eh_frame` and `.data.rel.ro`
sections. The audit policy permits instruction features; it does not enforce
Linux's no-red-zone, no-SIMD, unwind or calling-environment requirements.
No `bzImage` packaging or boot was attempted. All 992 captured inputs, the config,
reference artifacts, binary dependencies and compiler binary were rehashed
unchanged after the experiments.
