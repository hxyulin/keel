# Linux64 kernel linking and boot

The pinned Linux v6.12 kernel now links and boots without the FPU IR workaround
or a reference startup-object substitution. All 484 captured C inputs compile.
The final GNU link uses 481 Keel C objects and 20 Keel assembly objects. The
independent initramfs smoke test passes under QEMU TCG.

The [ledger](../suite/linux-kernel/audit-kernel-code-model.json) records source,
compiler, object and image hashes, controls and validation. The earlier
[link boundary](linux-kernel-link-fixes.md) describes the failure this increment
resolves.

## Kernel address model

`--code-model kernel` makes x86-64 `symbol_addr` materialize an absolute 64-bit
linked virtual address. This applies to defined and external data and functions,
including weak symbols. It avoids RIP-relative/GOT address materialization for
these operations. Early startup can therefore compute its physical relocation
delta from the linked kernel addresses while executing through a physical map.

The default remains `--code-model pic`, with the existing RIP-relative and GOT
behavior. The kernel model is for statically linked x86-64 Linux images. The CLI
rejects other targets and explicit shared-image intent; the backend rejects
userspace TLS. Text, typed and chunked output have matching object/relocation
witnesses across three selectors, two allocators and normal/empty pools.

An example using the unchanged pinned C instruction-permission policy is:

```sh
keel cc -O --target x86_64-unknown-linux-gnu \
  --link-intent executable --code-model kernel --cpu-level v1 \
  --target-features +popcnt,+rdrand,+rdtscp,+xsave,+clflush,+fsgsbase,+prefetchw,+invpcid,+rdseed,+cx16,+waitpkg \
  --emit-object input.i -o input.o
```

Address-model choice belongs to code generation. It is not stored in emitted IR;
apply the flag again when compiling an IR file. It does not change the CPU policy
or establish a general no-SIMD, interrupt-safe or unwind contract.

## Automatic FPU cleanup

The Full pipeline now includes `constant-args`. It substitutes an integer or
floating literal into a local body only when every direct call supplies the same
literal and the function cannot escape through data, addresses, assembly,
retention or lifecycle roots. Weak, public, variadic, returns-twice and explicit
aggregate-transport cases do not establish this proof. The signature and call
arguments remain unchanged. This pass does not inline or clone the body.

Assembly reference analysis no longer treats every macro definition as opaque.
Plain substitutions reuse names already present in the text or IR operands.
Concatenation, macro counters, quoted escapes, alternate-macro syntax and includes
remain conservative. Operand-built identifiers and unique-label prefixes are
matched conservatively. Register spellings come from the IR register parser.
Ordinary assembly comments do not by themselves retain every function.

This lets ordinary function collection discard the unreferenced
`save_fsave_header`. It also proves the sole call to `__fpu_restore_sig` passes
zero for `ia32_fxstate`; CFG cleanup removes its unused compatibility path.
Neither FPU conversion reference reaches the object. There are no kernel-name
special cases in the optimizer, signature changes, stubs or ignored link errors.
The old pinned workaround remains a historical experiment fixture.

## Controls and runtime result

The same source/config/feature policy is used for the positive and negative
experiments. Ordered normal-archive payloads are byte-checked before and after
objtool. Partial linking, final linking and Linux table sorting pass.

- Omitting `constant-args` only for the FPU unit restores the unresolved
  `convert_to_fxsr` reference and makes the final link fail.
- Omitting `unused-functions` only for that unit restores `convert_from_fxsr`
  and makes the final link fail.
- Compiling only the startup C unit with PIC still links, but reproduces a page
  fault and triple fault before serial output. The kernel-model startup boots.
- Repackaging the reference image reproduces its original `bzImage` bytes and
  boots, checking the external packaging path.

The all-Keel captured core image passes initramfs ELF reads, mapped-memory data
and permission checks, fork/stack COW, pipe contents and child exit/wait status.
Success requires ordered stage markers, one final marker and exit 33. Wrong
markers, a wrong exit code and a corrupted expected memory value are rejected
normally. Faults and timeouts do not count as passing controls.

## Qualification scope

All 992 captured inputs, the resolved config, reference images and three binary
dependencies were rehashed unchanged. No fallback C objects or manual IR edits
are used in the positive kernel link. The saved version strings still name the
reference build tools because their preprocessed inputs are unchanged; those
strings do not identify the compiler that emitted the replacement objects.

The vDSO image, real-mode blobs, boot setup, decompressor and initramfs preparation
remain external. Five captured vDSO objects are not direct members of this core
link. This is not a complete Kbuild invocation from raw source.

There are still 328 objtool warnings. Passing this one-CPU, 128 MiB QEMU 11.1.2 TCG
smoke test does not resolve those warnings or qualify other kernel configs,
SMP, native hardware, arbitrary FP/SIMD use or all kernel ABI paths. It does not
prove absence of miscompilation.

The final workspace run passes 2,951 tests and one doctest, with 13 tests skipped.
Workspace all-target/all-feature checking and both ISA generator freshness checks
pass. Documentation builds; strict Clippy and whole-tree formatting retain
pre-existing findings. Changed Rust regions have no new formatting differences.
