# Linux kernel integration

This reference describes current implementation contracts and limits. Use
[testing](../development/testing.md) for validation commands; per-run results belong outside Git.

<a id="linux-kernel-code-model-linux64-kernel-linking-and-boot"></a>
## Linux64 kernel linking and boot

The pinned Linux v6.12 kernel now links and boots without the FPU IR workaround
or a reference startup-object substitution. All 484 captured C inputs compile.
The final GNU link uses 481 Keel C objects and 20 Keel assembly objects. The
independent initramfs smoke test passes under QEMU TCG.

Historical audit ledgers with hashes and controls remain in Git history.
Regenerate results with the [kernel fixture](../../suite/linux-kernel/README.md).

<a id="linux-kernel-code-model-kernel-address-model"></a>
### Kernel address model

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

<a id="linux-kernel-code-model-automatic-fpu-cleanup"></a>
### Automatic FPU cleanup

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

<a id="linux-kernel-code-model-controls-and-runtime-result"></a>
### Controls and runtime result

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

<a id="linux-kernel-code-model-qualification-scope"></a>
### Qualification scope

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

<a id="linux-kernel-native-linker-audit-native-kernel-linking"></a>
## Native kernel linking

Keel now owns archive selection, ELF relocatable aggregation and final linking
for the pinned minimal x86-64 Linux v6.12 core. It evaluates the unchanged
expanded GNU script, keeps Linux notes, applies relocations at high virtual
addresses and emits separate physical load addresses. The physical entry is
`0x1000000`. No reference symbol values, replacement objects, removed notes or
undefined-symbol suppression are used.

Historical qualification and probe ledgers remain in Git history. Use the
[kernel fixture](../../suite/linux-kernel/README.md) to regenerate results.

<a id="linux-kernel-native-linker-audit-native-link-paths"></a>
### Native link paths

Both native routes pass the existing checked VFS, memory/protection,
fork/COW and pipe workloads in QEMU 11.1.2 TCG:

- Keel selects regular archives, emits an ET_REL aggregate, then final-links it.
- Keel selects and final-links the original archives directly.

A third test final-links the existing GNU aggregate with Keel. GNU final links
of the original archives and Keel aggregate provide independent controls. All
five paths boot and complete every runtime stage. Three deliberately broken
userspace fixtures are rejected: a wrong success marker, a wrong exit status
and corrupted memory expectations. Faults and timeouts do not count as passes.

The three archives contain 440, 35 and 25 object members. Rebuilding them as
GNU thin archives preserves all member bytes and order. Thin and regular
archives produce identical Keel aggregate bytes and identical direct final-link
bytes. The final reviewed implementation also regenerates the three qualified
native-final images byte for byte.

The compiler objects come from the previously qualified captured-input build.
This run qualifies the new linker, not a new kernel compilation. External
reference stages still supply boot/setup, decompression and vDSO preparation.
Raw-source Kbuild and arbitrary kernel configurations are not qualified.
The existing objtool warning backlog is unchanged.

<a id="linux-kernel-native-linker-audit-reproduction"></a>
### Reproduction

Use the pinned archives, expanded `vmlinux.lds` and separately compiled
`init/version-timestamp.o` from the captured build:

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
--start-group lib/lib.a arch/x86/lib/lib.a --end-group`.

The 2 MiB LOAD alignment is required by this Linux decompressor. A diagnostic
run without that setting halted in the decompressor before serial output;
the corrected GNU and Keel links pass. The default userspace layout retains
its existing target page size.

Package and test with the reference boot build and external tools on PATH:

```sh
python3 suite/linux-kernel/package.py --reference-build reference/build \
  --vmlinux vmlinux --out packaged
python3 suite/linux-kernel/runtime.py \
  --kernel packaged/arch/x86/boot/bzImage --out runtime
```

The packaging helper checks the pinned config, copies boot artifacts into a
new output directory, sorts exception tables and rebuilds the compressed and
setup images. It never relinks the kernel core. Output directories must not
already exist. Keep their small JSON/log evidence and remove their generated
images and boot-tree copies when the experiment is finished.

<a id="linux-kernel-native-linker-audit-script-and-input-scope"></a>
### Script and input scope

See [GNU linker scripts](../linking/gnu-scripts.md) for the grammar, expression
limits, output-section ordering, symbol rules and remaining unsupported syntax.
`-T path`, `-Tpath`, `--script path` and `--script=path` select it. An explicit
`-e`/`--entry` takes precedence over script ENTRY and participates in archive
selection. Whole-archive controls and nested groups retain left-to-right lazy
selection outside those controls.

Partial links retain contribution identities and relocations rather than
performing GNU ld's default coalescing and unwind compaction. Intermediate
files and some final addresses therefore differ from GNU. Their layouts and
runtime behavior are checked independently. The partial writer preserves
COMDAT groups, common/weak selection and linked section indices. It currently
rejects REL relocations and extended ELF section-index output. Scripted partial
links are unsupported.

Thin archives are materialized by the CLI from member paths relative to their
archive. Library callers supply an explicit reader through
`archive::materialize_thin`; the linker library does not read the filesystem.
Unsupported scripts, unplaced allocated sections and relocation overflow fail
with diagnostics. This is bounded GNU-script support for the captured kernel,
not full GNU ld command-line or script-language compatibility.

<a id="linux-kernel-syscall-codegen-gaps-kernel-syscall-code-generation-gaps"></a>
## Kernel syscall code generation gaps

This records the pre-optimization audit. The subsequent
[clobber-aware implementation](../targets/x86_64/codegen.md) removes much of
the dispatcher overhead measured below; its new function sizes are recorded there.

This audit examines the pinned Linux v6.12 `arch/x86/entry/common.i` and
`kernel/sys.i` inputs. Fresh O2 and Os objects from the current working-tree
release compiler match all four object hashes in the previously qualified
`audit-policy-fix.json` source campaign. Concurrent frontend changes are present
in the compiler build, but do not change these four objects. No new whole-kernel
build or runtime campaign was run for this audit.

<a id="linux-kernel-syscall-codegen-gaps-interpreting-the-runtime-comparison"></a>
### Interpreting the runtime comparison

The earlier chart compares Keel with GCC at each matching preset. A lower Os
ratio means a smaller slowdown relative to GCC Os; it does not mean Keel Os
runs faster than Keel O2. The recorded absolute workload medians are higher for
Os. Moreover, both compilers' Os samples shift sharply after pair 3, including
host startup times. The cause was not established. Do not use these separate
cohorts to rank O2 against Os. A new interleaved four-image campaign with host
and guest clock diagnostics is needed for that question.

<a id="linux-kernel-syscall-codegen-gaps-largest-observed-source-of-dispatcher-growth"></a>
### Largest observed source of dispatcher growth

| Function | GCC O2 bytes | Keel O2 bytes | GCC Os bytes | Keel Os bytes |
|---|---:|---:|---:|---:|
| `do_syscall_64` | 193 | 711 | 168 | 651 |
| `__x64_sys_getpid` | 27 | 50 | 27 | 53 |

The GCC Os objects are the retained reference build. GCC O2 sizes come from
the qualified measurement ledger. Fresh Keel objects reproduce its sizes.

Keel's Os dispatcher wraps `sti` and the inline bounds-mask template with
14 GPR stores followed by 14 GPR loads each. Each template's preservation
costs 148 encoded bytes, for 296 bytes across both templates. That is 61.3%
of the 483-byte dispatcher excess over GCC Os. This counts existing emitted
instructions; it is not a measured saving from an implemented optimization.

The Os object offsets are:

- `0x24..0x6e`: stores before `sti`; `0x6f..0xb9`: restores afterward.
- `0x106..0x150`: stores before the mask template;
  `0x169..0x1b3`: restores afterward.

The backend's `native_inline_asm` always preserves every available non-frame
GPR. Planning reserves `8 * (16 + input_count + output_count)` bytes, and
the presence of an asm home also forces RBX and R12–R15 prologue preservation.
In this dispatcher, the stack allocation is 152 bytes in addition to those
register pushes. These are conservative implementation choices, rather than
requirements imposed by a memory clobber or by `sti` itself.

The first contained optimization should bypass register staging for templates
with no operands and no GPR clobbers. Memory and condition-code effects must
remain intact. A later step can preserve only homes that are live and may be
overwritten by operand setup, template clobbers, outputs or staging scratch
registers. It must handle fixed/tied operands, early clobbers, rematerialization,
flags outputs, asm calls, reserved stacks and custom allocator pools. Simply
deleting the saves from the general path would lose that contract.

<a id="linux-kernel-syscall-codegen-gaps-other-observed-overhead"></a>
### Other observed overhead

1. **Call argument and result placement.** The Os getpid wrapper has 17
   instructions versus GCC's nine. It copies constants and arguments through
   temporary homes, including an overwritten incoming-argument move, then
   moves the call result through RSI before extending it and moving it to RAX.
   ABI-aware allocation and copy elimination are candidates. Some copies are
   needed for genuine parallel-copy cycles, so eliminate only proven redundant
   ones.
2. **Symbol and address folding.** The current kernel model uses a seven-byte
   signed-32-bit symbol-address materialization followed by a three-byte load.
   GCC uses a seven-byte RIP-relative load directly into the argument register.
   The older `movabs` example is superseded for this current object. The
   dispatcher also builds field addresses with separate moves and adds rather
   than consistently selecting base-plus-displacement memory operands.
   Folding needs the appropriate code-model range and relocation guarantees.
3. **Boolean tests and control flow.** The dispatcher emits move/AND/TEST
   sequences, jump-only blocks and separate boolean return preparations.
   GCC selects compact TEST forms and converges more paths. Keel already
   has mask-test fusion and Os shared epilogues; these examples expose gaps
   in their coverage rather than a complete absence of those optimizations.

Recommended order is the operand-free asm fast path, selective preservation,
argument/result copy elimination, then address and test folding. These changes
target code that executes on common syscall paths. Function size alone does
not attribute the measured runtime gap: syscall helpers, VFS, memory management,
and scheduling also contribute. Rebuild, inspect objtool diagnostics, boot the
correctness fixture and rerun controlled timings after each backend change.
