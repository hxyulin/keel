# Linux64 link fixes and runtime boundary

The compiler fixes reduce the pinned kernel's automatic final link from 22
unresolved references to two: `convert_from_fxsr` and `convert_to_fxsr` in
`arch/x86/kernel/fpu/signal.i`. All 484 C inputs compile with `-O` and explicit
executable binding. GNU ld still performs the kernel link.

A separate experiment resolves those two references through a reviewed FPU IR
workaround. That all-Keel C/assembly image links and passes table sorting, but
triple-faults during early startup. Replacing only `arch/x86/kernel/head64.o`
with its reference object lets the image boot and pass the runtime smoke test.
This is a mixed-image result, not full Keel kernel qualification.

The [ledger](../suite/linux-kernel/audit-link-fixes.json) records compiler and
artifact hashes, substitutions, warnings, controls and validation. The
[earlier link audit](linux-kernel-link-audit.md) remains the historical baseline.
The source, config, feature policy and all 992 captured inputs remain unchanged.

## Compiler changes

The Full optimizer now runs known-bit analysis followed by CFG cleanup. This
removes guards whose conditions are false for every possible value, including
bounded flags and disjoint bit masks. Native instruction combining also removes
pointer/integer round trips and folds numeric pointer offsets when both inputs
are constants. Wasm32 retains its separate optimizer and pointer-width contract.

The `empty-calls` pass removes ordinary void calls only when a bound definition
consists of a single return with normal parameters. It rejects weak,
interposable, variadic, returns-twice and aggregate-copy contracts. `noinline`
still prohibits inlining; it does not force execution of a proven empty body.
Argument evaluation remains in the caller. DCE retains side effects and
potentially trapping reads.

The `unused-functions` pass removes local bodies made unreachable by folding or
inlining. Exports, retention and lifecycle attributes, data initializer pointers,
personalities, function addresses and direct-call edges remain roots. Textual
assembly references remain roots too. Assembly macros can construct symbol names,
so modules containing that syntax conservatively retain their local bodies.
Fast-call definitions also remain while stale callee imports require a local
body. Removed C bodies leave synchronized declarations rather than stale
placement contracts.

Regression tests cover binding, argument effects and traps, recursive dead
components, callbacks, assembly references, retention and fast-call imports.
Two attribute tests needed emission expectations updated after dead bodies disappear.
One vector ABI fixture now explicitly marks its inspected body `used`.

## Temporary FPU workaround

Two different limitations meet in `signal.i`. `save_fsave_header` has no remaining
call, but its module contains assembly macros, so conservative reachability keeps
its body. `__fpu_restore_sig` has one direct call with `ia32_fxstate = 0`, but the
optimizer does not yet specialize that parameter across the call boundary.
Their discarded compatibility paths would reference the two unavailable helpers.

The experiment manually reviews the pinned assembly macros and checks direct
calls, address references, data initializers and assembly text. It removes the
unreferenced `save_fsave_header` body, substitutes zero for uses of the third
parameter inside `__fpu_restore_sig`, and runs ordinary CFG/instruction/DCE
cleanup. Function signatures and call arguments stay unchanged. There are no
unresolved-symbol stubs and no ignore-errors linker options.

The [workaround script](../suite/linux-kernel/fpu-link-workaround.py) accepts only
the exact reviewed optimized IR hash. It reproduces the qualified FPU object
byte for byte. It is an experiment fixture, not an automatic compiler pass:

```sh
python3 suite/linux-kernel/fpu-link-workaround.py \
  --keel /path/to/keel --input signal.keel --out signal.fixed.keel
```

General support needs checked interprocedural constant propagation and better
assembly reference information. Removing macro conservatism without such a
contract would make function collection unsafe.

## Boot diagnosis

The all-Keel C image with the FPU workaround faults after loading the early page
table and transferring to `common_startup_64`. QEMU records a page fault, then a
double fault and triple fault before serial output. It never reaches the initramfs.

The pinned reference commands use `-fno-PIE -mcmodel=kernel`. Early C code relies
on symbol addresses being linked kernel virtual addresses even while executing
through a physical mapping. Keel currently materializes local symbol addresses
with RIP-relative addressing. In `__startup_64`, that changes the address used
to compute the relocation delta. Keel has no explicit kernel code-model contract.
Replacing just the reference `head64.o` gets past this transition and passes the
smoke test. That substitution supports the diagnosis but does not prove every
remaining code-generation path correct.

This is the significant backend boundary. The next increment needs an explicit
kernel code model and addressing tests. Kernel environment qualification must
also cover SIMD/FPU use, red-zone policy, sibling calls and unwind/objtool
contracts. The automatic objects produce 329 objtool warnings; the startup
substitution leaves 328. Zero objtool return codes do not make those warnings
clean. The instruction-feature permission policy is not this backend contract.

## Runtime evidence

The direct kernel link uses 501 objects. The successful mixed image contains
480 Keel C objects, including the manually adjusted FPU unit, one reference C
startup object and 20 Keel assembly objects. Five captured vDSO objects are not
direct link members. The separate vDSO payload, real-mode blobs, initramfs data,
boot setup and decompressor remain externally built.

Repackaging the reference `vmlinux` reproduces the original `bzImage` bytes and
boots successfully. The assembly-only replacement also boots. The
[runtime runner](../suite/linux-kernel/runtime.py) then passes on the reference,
assembly-only and mixed C images under QEMU 11.1.2 TCG, one CPU and 128 MiB RAM.
Its independent GCC-built init checks:

- VFS/open/read/close through the initramfs executable's ELF header.
- Anonymous mappings, page faults, exact data, read permissions and unmapping.
- Fork, stack COW, pipe contents, scheduling, child exit status and wait.

Each success requires all stage markers in order, one final marker and exit 33.
Wrong-marker, wrong-exit and memory-mismatch controls return normally and are
rejected. Faults and timeouts never count as successes.

```sh
python3 suite/linux-kernel/runtime.py \
  --kernel /path/to/bzImage --out /new/runtime-evidence
```

This is bounded emulated smoke coverage. It does not establish absence of
miscompilation, clean objtool output, native hardware behavior or production use.
