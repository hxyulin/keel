# Linux kernel readiness

Audit date: 2026-10-26. Keel source: `2da2e6a6`.

Linux is a useful integration target. Completing its instruction inventory
alone will not make Keel a kernel compiler. The first blocking areas are GNU
driver compatibility, x86 inline assembly and kernel code-generation policy.
Using Keel as the assembler also exposes GNU assembly-language gaps.

This is a preliminary audit of 30 selected Linux files and synthetic probes,
not a configured kernel build. No complete kernel translation unit, kernel
link or boot was attempted. There is no measured completion percentage or
reliable calendar estimate yet.

## Scope and evidence

The reference is upstream Linux **v6.12**, commit
`adc218676eef25575469234709c2d87185ca223a`. This fixed reference is not a claim
to use the latest stable patch release. It is a baseline for compatibility
work, not a deployment recommendation.

The [evidence file](provenance/linux-kernel-readiness.json) records source-file
hashes, commands, input snippets, exit statuses and diagnostics. The selected
files cover Kbuild, compiler attributes, x86 state/IRQ/atomic/static-branch
headers, entry assembly, boot Makefiles and the kernel linker script. The
downloaded files and initial reports are under `/tmp/keel-linux-audit`.

After rebuilding the current CLI with `mbx build -p keel`, this audit ran:

| Probe set | Result | Meaning |
| --- | --- | --- |
| 52 instruction spellings | 30 accepted; 21 missing public mnemonics; 1 explicit feature rejection | Individual assembler examples, not kernel coverage or execution. |
| CLFLUSH with its explicit feature | Accepted | Its v3 rejection above is a feature gate, not a missing mnemonic. |
| 19 small GNU C examples | 11 accepted; 8 rejected | Acceptance alone does not establish attribute semantics. |
| The same 19 C examples with Clang | 18 accepted; the deliberate `error`-attribute call rejected | Keel incorrectly accepts that compile-time error witness. |
| 8 GNU assembly-language examples | All rejected by Keel; all accepted by Clang | Independent syntax witnesses for the assembler-language gap. |
| 15 driver options | All rejected | Keel's CLI is not a drop-in GCC/Clang driver. |

The existing read-only `tools/x86-isa/audit_system.py` also ran successfully
against the current ledger. No production code changed, no workspace test
suite was rerun, and no privileged or AVX-512 execution was attempted.

## What AVX-512F exposed

The EVEX implementation fixed concrete problems in shared infrastructure:

- Symbolic register reading used a handwritten inverse mapping. New vector
  classes exposed missing mappings; the inverse now comes from the register
  schema, including Control/Debug and the appended ZMM/Mask classes.
- Two-byte VEX decoding interpreted the R extension bit as W. The generated
  mask-instruction cases exposed that error; W is now read only for three-byte
  VEX prefixes.
- Multiple forms could accept the same unsized memory spelling while selecting
  different input widths. EVEX matching now rejects that ambiguity and generates
  the required AT&T width aliases.

It also demonstrated constraints future families must preserve:

- Encoded vector length, physical register view and active operand width are
  separate values. Rounding can reuse length bits, and narrowing conversions
  do not change the physical register class.
- Displacement compression depends on tuple, element size and broadcast.
  Symbolic relocations must keep full fields; RIP-relative addends must include
  trailing immediates.
- CPU capability, enabled OS state and compiler support are separate. The
  assembler understands AVX-512F/VL, but the compiler has no new 512-bit SSA,
  allocation, calling-convention or state-restoration contract.
- Generated source witnesses need independent byte oracles. They can share a
  spelling across source records and cannot prove full ISA validity or execution.

The shared engines were sufficient for this AVX-512F increment. There is no
evidence here that another general instruction-description framework is needed.
Generating more witnesses and classifying uncovered encoding predicates is a
smaller next step than replacing the encoder again. The legacy/VEX and EVEX
engines still have separate matching/decoding paths, so cross-family regression
cases remain useful when changing shared register, prefix and memory handling.

## Instruction support versus kernel support

The current probes assemble CR3 moves, CLI/STI/HLT, SWAPGS, IRETQ, SYSRETQ,
SYSCALL, SYSENTER, SYSEXITQ, RSM, descriptor loads, INVLPG, MSR operations,
cache-control operations, XSETBV, XSAVE64/XRSTOR64, arbitrary INT, far RET and
LGS. These are examples, not exhaustive mode or operand guarantees.

The following public mnemonics reject in this audit even though corresponding
records already exist in the generated production catalogue:

| Area | Rejected examples |
| --- | --- |
| Supervisor state and timing | `xsaves64`, `xrstors64`, `rdtscp` |
| Access and translation controls | `clac`, `stac`, `invpcid` |
| Segment bases and cache operations | `rdfsbase`, `wrgsbase`, `clflushopt`, `clwb` |
| Ordering and protection | `serialize`, `rdpkru`, `wrpkru`, `endbr64` |
| Virtualization and waiting | `invept`, `invvpid`, `vmcall`, `vmmcall`, `vmrun`, `monitor`, `mwait` |

These are likely adapter/feature-policy increments first. Individual forms
still need matcher and independent byte tests. They must not become v1 features
through an unclassified policy fallback. This list is selected, not a complete
inventory of optional kernel extensions.

Linux sometimes emits optional instructions through `.byte` for compatibility
with older assemblers. For example, the pinned `special_insns.h` does this for
PKRU operations and SERIALIZE. A missing public mnemonic therefore does not
automatically block a particular configured build. Conversely, an imported
mnemonic does not prove that Linux's exact operands, prefixes or directives work.

The remaining Keel-owned SYSRET/entry/far-transfer/SMM contracts in
[the system audit](x86-system-audit.md) remain separate work. Linux owns its
entry and return sequences in assembly. Compiling that assembly does not require
replacing it with typed Keel intrinsics. A small Linux kernel also does not need
every virtualization or SMM feature to be enabled.

## C, IR and backend gaps

**X86 inline assembly is the first implementation priority.** Both the C
frontend and native backend reject it. Even an empty compiler memory barrier
fails. Linux's `special_insns.h`, `irqflags.h` and `barrier.h` use it directly.
Static branches and CPU alternatives also use asm-goto in `jump_label.h` and
`cpufeature.h`.

Supporting these calls needs constraints and real compiler effects: fixed and
tied registers, early clobbers, memory inputs/outputs, flag outputs, symbolic
immediates, template modifiers and local labels. Asm-goto needs explicit CFG
successors and defined output availability on each edge. Passing a string to
the assembler cannot provide those semantics. File-scope assembly and local
register-variable bindings are also rejected by the probes.

Several GNU C building blocks already compile in the probes: statement
expressions, `typeof`, `__auto_type`, computed goto, type/constant-selection
builtins, `offsetof`, section attributes and integer atomic builtins. These
should remain positive controls in the kernel audit rather than speculative
backlog items.

Attributes need semantic classification. `alias` is explicitly rejected.
Named `__seg_gs` syntax rejects. An `address_space(256)` declaration is accepted,
but that is not proof of GS-relative addressing. The unknown-attribute catch-all
also accepts a call to an `error("must fail")` function, which Clang rejects.
Linux's compile-time assertions use such attributes conditionally. Feature
queries and supported semantics must agree before enabling those paths.

**Kernel code generation needs its own environment policy.** The pinned
`arch/x86/Makefile` requests `-mno-red-zone`, `-mcmodel=kernel` and disables SIMD.
Keel rejects those flags and has no explicit kernel environment contract. That
does not establish that its existing frames use a red zone; frame correctness
must be checked independently. The absence of accidental SIMD, high-half symbol
addressing, relocation choices, stack alignment and compiler-generated helper
calls must also be audited. CPU level v1 is a capability profile, not a policy
that forbids using SIMD in ordinary kernel code.

Kbuild also requests overflow, aliasing and null-pointer optimization policies.
Accepting their spellings without connecting them to frontend/optimizer behavior
would be insufficient. Start with minimal configuration and no optional LTO,
sanitizers or instrumentation, while recording each disabled feature explicitly.

## Driver, assembler and link gaps

Keel rejects ordinary `-c`, `-S`, `--version`, `-dumpmachine`, `-std=gnu11` and
the kernel flags tested here. Linux's `scripts/cc-version.sh` and
`include/linux/compiler_types.h` select GCC/Clang behavior. A build adapter or
explicit Keel integration must answer only capability probes it can honor.
Defining a recent `__GNUC__` version alone would enable unsupported assumptions.

If GNU/LLVM remains the assembler initially, the first C milestone avoids a
second independent compatibility project. Replacing it with Keel requires GNU
assembly syntax support as well as instruction support. All eight witnesses
failed: `.code64`, `.code32`, `.code16gcc`, numeric local labels with a section
delta, a required macro argument, `\\@` unique macro labels, `.org` and
`.pushsection`. These tests exercise the current x86 ELF CLI; macro facilities
in other parser paths do not establish support on that path.

Use an external linker and host compiler first. Linux's linker script, partial
links, exception/alternative tables, discarded metadata and relocation checks
need a separate linker audit before using `keel link`. Kernel host tools should
continue to use the host toolchain. Objtool and unwind/mitigation requirements
depend on the chosen configuration and must be captured in the build manifest.

## Proposed acceptance targets

Use a pinned x86-64 minimal configuration with serial output and a tiny initramfs.
The exact `.config` and its hash still need to be created and reviewed. Keep
the kernel reference fixed while making the first compiler changes.

1. **Configured audit:** build the reference kernel with GCC/Clang on Linux,
   record every compiler/assembler command, then probe Keel against actual
   preprocessed translation units and expanded assembly. Group unique failures
   by feature, not repeated diagnostic count. Include compiler-generated
   assembly and alternative instruction sections, not just executed code.
2. **X86 asm and code-generation contract:** implement the measured constraints
   and kernel environment, with negative tests and independent object oracles.
   Close silent semantic acceptance before counting a source as supported.
3. **Mixed-toolchain boot:** compile selected real kernel objects with Keel,
   retain reference assembly/link/boot tools, and boot the resulting kernel
   under QEMU. The initramfs must report a fixed success marker and exit normally.
4. **All 64-bit kernel C:** compile every configured 64-bit kernel C translation
   unit with Keel and pass the same boot check. Log external-tool exceptions
   separately; this is not yet an all-Keel `bzImage`.
5. **Assembler and complete image:** replace the assembler after an exact
   configured-source audit. A fully Keel-built `bzImage` additionally needs
   boot/decompressor support: the pinned build uses 16-bit C flags and mixed
   16/32/64-bit assembly, outside the current long-mode compiler target. Native
   linker replacement is another separately measured milestone.

This makes Linux a measurable compiler target without blocking it on unrelated
SMM or optional virtualization APIs. The next concrete work item is target 1,
followed by x86 extended asm and asm-goto, rather than more generic generators.

## Upstream references

- [Linux v6.12 x86 Makefile](https://github.com/torvalds/linux/blob/v6.12/arch/x86/Makefile).
- [Compiler detection](https://github.com/torvalds/linux/blob/v6.12/scripts/cc-version.sh)
  and [compiler types](https://github.com/torvalds/linux/blob/v6.12/include/linux/compiler_types.h).
- [Static branches](https://github.com/torvalds/linux/blob/v6.12/arch/x86/include/asm/jump_label.h)
  and [system instruction wrappers](https://github.com/torvalds/linux/blob/v6.12/arch/x86/include/asm/special_insns.h).
- [Entry assembly](https://github.com/torvalds/linux/blob/v6.12/arch/x86/entry/entry_64.S),
  [boot Makefile](https://github.com/torvalds/linux/blob/v6.12/arch/x86/boot/Makefile)
  and [compressed image Makefile](https://github.com/torvalds/linux/blob/v6.12/arch/x86/boot/compressed/Makefile).
