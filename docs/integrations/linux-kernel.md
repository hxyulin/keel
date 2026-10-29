# Linux kernel integration

Keel compiles, links and boots the pinned minimal Linux v6.12 x86-64 core. All
484 captured C inputs compile, the GNU or Keel link uses Keel C and assembly
objects, and the image passes the initramfs smoke test under QEMU. The
[kernel fixture](../../suite/integration/linux-kernel/README.md) captures the
inputs, builds the images and runs the checks.

## Compiling kernel inputs

Kernel objects use the [`kernel` code model](../targets/x86_64/codegen.md#code-models),
which materializes symbol addresses as sign-extended 32-bit absolute values,
and the pinned instruction policy:

```sh
keel cc -O --target x86_64-unknown-linux-gnu \
  --link-intent executable --code-model kernel --cpu-level v1 \
  --target-features +popcnt,+rdrand,+rdtscp,+xsave,+clflush,+fsgsbase,+prefetchw,+invpcid,+rdseed,+cx16,+waitpkg \
  --emit-object input.i -o input.o
```

The code model is not stored in IR; apply the flag again when compiling an IR
file. It does not change the CPU policy or establish a no-SIMD, interrupt-safe
or unwind contract.

Two Full optimizer passes remove the FPU signal-frame conversion references
that the minimal configuration cannot link: `constant-args` proves that the
only call to `__fpu_restore_sig` passes zero for `ia32_fxstate`, and
`unused-functions` drops the unreferenced `save_fsave_header`. The optimizer
has no kernel-specific cases. Assembly reference analysis treats plain macro
substitutions precisely and concatenation, counters, escapes, alternate macro
syntax and includes conservatively.

## Native kernel linking

Keel selects archives, aggregates ELF relocatable objects and final-links the
core with the unchanged expanded GNU script. It keeps Linux notes, applies
relocations at high virtual addresses and emits separate physical load
addresses; the physical entry is `0x1000000`. Two routes work: an `-r`
aggregate followed by a final link, or a direct final link of the archives.
Regular and thin archives give identical bytes. Use the pinned archives,
expanded `vmlinux.lds` and the separately compiled `init/version-timestamp.o`:

```sh
cargo build -p keel
KEEL=target/debug/keel
$KEEL link --target x86_64-unknown-linux-gnu -r \
  --whole-archive vmlinux.a --no-whole-archive \
  --start-group lib/lib.a arch/x86/lib/lib.a --end-group -o aggregate.o
$KEEL link --target x86_64-unknown-linux-gnu \
  -z max-page-size=0x200000 --script=vmlinux.lds \
  aggregate.o init/version-timestamp.o -o vmlinux
```

For the direct route, replace the final link's two object inputs with
`--whole-archive vmlinux.a init/version-timestamp.o --no-whole-archive
--start-group lib/lib.a arch/x86/lib/lib.a --end-group`. The Linux
decompressor needs the 2 MiB load alignment.

Package and test with the reference boot build and external tools on PATH:

```sh
python3 suite/integration/linux-kernel/package.py --reference-build reference/build \
  --vmlinux vmlinux --out packaged
python3 suite/integration/linux-kernel/runtime.py \
  --kernel packaged/arch/x86/boot/bzImage --out runtime
```

The packaging helper checks the pinned config, copies boot artifacts into a
new output directory, sorts exception tables and rebuilds the compressed and
setup images. It never relinks the core. Output directories must not exist.

The [ELF linker](../linking/elf.md) owns partial linking, thin archives and the
command line, and [GNU linker scripts](../linking/gnu-scripts.md) owns the script
subset. An explicit `-e`/`--entry` takes precedence over script `ENTRY` and
takes part in archive selection.

## Scope and limits

The vDSO image, real-mode blobs, boot setup, decompressor and initramfs
preparation come from the reference build. This is not a Kbuild invocation
from raw source, and other kernel configurations are not covered. Objtool
still reports warnings for the Keel objects. The QEMU smoke test checks
initramfs ELF reads, mapped-memory data and permissions, fork and stack
copy-on-write, pipe contents and child exit status, with ordered stage
markers and exit 33. It does not cover SMP, native hardware, arbitrary FP or
SIMD use, or all kernel ABI paths, and does not prove the absence of
miscompilation. [Kernel benchmarks](../benchmarks/linux-kernel.md) compare
size and runtime with the GCC build.
