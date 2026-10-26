# Minimal Linux x86-64 compiler target

This fixture targets the **64-bit parts** of Linux v6.12. Host programs,
16/32-bit code and mixed-mode assembly remain external. X86 C inline assembly
is a later implementation stage. Do not strip assembly or attributes from
Linux to make the probe pass.

`source.json` pins the release archive and source identity. `minimal.config`
is the input to `allnoconfig`; `resolved.config` records the reference result.
The seed enables serial output, an ELF initramfs and the disposable guest's
I/O permission API. The latter lets the reference init report a normal QEMU
debug-exit result without requiring an ACPI power-off subsystem.

This is a compatibility fixture. It disables SMP, modules, networking, optional
instrumentation and most devices by starting from `allnoconfig`. It is not a
production or security configuration. The kernel, initramfs program and boot
code currently use an external compiler and linker throughout.

The fixture selects the frame-pointer unwinder. ORC unwind metadata is a later
configuration target. Objtool still runs for architecture-selected features;
disabling ORC does not remove that build requirement.

## Build the reference

Run on x86-64 Linux with GCC 14.4.0, GNU binutils, make, flex, bison, bc, Perl, Python
libelf development headers/libraries and `qemu-system-x86_64` available. Builds
are bounded by `--jobs`. `--hostcc` selects the host compiler independently;
it must be able to find those host development headers. Use a fresh
output directory. Keep the source tree unmodified and verify the release
archive before extraction:

```sh
kernel_audit_root="$PWD/target/linux-kernel"
mkdir -p "$kernel_audit_root"
curl -fL https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz \
  -o "$kernel_audit_root/linux-6.12.tar.xz"
echo "b1a2562be56e42afb3f8489d4c2a7ac472ac23098f1ef1c1e40da601f54625eb  $kernel_audit_root/linux-6.12.tar.xz" | sha256sum --check
tar -xf "$kernel_audit_root/linux-6.12.tar.xz" -C "$kernel_audit_root"
python3 suite/linux-kernel/reference.py \
  --source "$kernel_audit_root/linux-6.12" \
  --out "$kernel_audit_root/reference-out" --jobs 8
```

The script checks the source Makefile hash, configures the kernel, builds a
reference `bzImage` while saving intermediate inputs, builds a static initramfs
program and boots under single-vCPU QEMU TCG. The boot must print exactly one
`KEEL_LINUX64_REFERENCE_OK` line and return debug-exit status 33. Faults,
timeouts and a marker without the expected exit never pass. The reference
image includes external boot/decompressor code; this does not expand Keel's
64-bit implementation scope.

The initramfs uses deterministic gzip/newc output and includes `/dev/console`
without requiring root or host `mknod`. Kbuild's user, host and timestamp
fields are fixed. Compiler versions and resolved configuration still affect
the result; `reference.json` records versions and artifact hashes. A different
compiler may resolve different capability symbols. The runner rejects a
resolved config that differs from the committed pin. Review those differences
rather than silently replacing it. The initial reference uses GCC 14.4.0,
GNU binutils 2.46 and QEMU 11.1.2; newer GCC defaults break the old release's
external boot/decompressor C. Host compilation used a compiler able to find
the system libelf headers. Compiler-capability fields also record other
installed tool versions even when their optional kernel features are disabled.

## Capture and probe

```sh
cargo build -p keel
python3 suite/linux-kernel/audit.py capture \
  --source "$kernel_audit_root/linux-6.12" \
  --build "$kernel_audit_root/reference-out/build" \
  --out "$kernel_audit_root/reference-out/inputs.json"
python3 suite/linux-kernel/audit.py probe \
  --build "$kernel_audit_root/reference-out/build" \
  --manifest "$kernel_audit_root/reference-out/inputs.json" \
  --keel "$PWD/target/debug/keel" --out "$kernel_audit_root/keel-probes"
python3 suite/linux-kernel/audit.py compare \
  --build "$kernel_audit_root/reference-out/build" \
  --probe-out "$kernel_audit_root/keel-probes" \
  --out "$kernel_audit_root/gnu-comparison"
```

Capture joins GCC's saved `.i`/`.s` files with Kbuild's `.o.cmd` commands. It
requires `__KERNEL__` and excludes host programs. It marks boot, the real-mode
`arch/x86/realmode/rm/` subtree and
compatibility subtrees, `-m16`/`-m32` invocations and assembly containing
`.code16`/`.code32` as external. The entire mixed-mode assembly file stays
external; it does not claim coverage for selected lines within it. Saved
compiler-generated `.s` and preprocessed handwritten `.S` inputs are both
included. Per-input hashes and normalized commands remain in the manifest.
Build helpers under `scripts/` and `tools/` also remain external, even if they
use target flags; `scripts/mod/empty.c` is one such target-layout probe.

The probe checks that the resolved configuration and each input still match
the capture. It sends unchanged `.i` files to the C compiler and unchanged
`.s` files to the AT&T assembler. Assembly uses the explicit v3 profile to
separate syntax failures from the default v1 profile; remaining feature-gate
errors remain failures. It does not modify input or retry after removing a
diagnostic. First errors are grouped in `results.json`; later errors can be
masked. An accepted input is not execution evidence or a complete kernel
support verdict. GCC-preprocessed inputs also retain GCC-selected language
and attribute paths, which need to work in their own right.
Probes run in Kbuild's output directory so relative generated-file references
retain their build context. Handling `.incbin` and other file dependencies
still needs an assembler input-access contract; the audit does not inline them.
Keel's unsupported Kbuild flags are not forwarded or silently accepted. The C
probe uses its ordinary userspace code-generation mode. Accepted objects are
not yet checked by objtool or linked into the reference image.

The `compare` command assembles each accepted assembly input with GNU as and
uses the existing `compiler_asm_compat/objinspect.py` normalizer. GNU relocation
relaxation is disabled to match the current assembler contract. It retains
strict section/symbol differences. Shared-section and public-symbol metrics are
additional diagnostics; they do not excuse missing sections or establish whole
object equivalence. Use `--gnu-as` to select the reference assembler.

Generated build trees, saved inputs, objects and raw logs stay outside Git.
Commit only reviewed configuration, tools and compact reports. The preceding
[readiness audit](../../docs/linux-kernel-readiness.md) describes the compiler,
assembler, driver and linker boundaries this fixture measures.

## First configured audit

The [compact results](audit-summary.json) record the resolved config hash,
reference artifact hashes and first-error groups at assembler checkpoint
`429fc34c`. The reference build and a clean rebuild produced identical
`vmlinux` and `bzImage` bytes and both booted successfully. Two independent
initramfs controls returned normally but failed the verdict: a wrong marker
with exit 33, and the correct marker with exit 35.

The final scope contains **484 C inputs and 506 assembly inputs**, comprising
484 compiler-generated and 22 handwritten/data-packaging assembly files.
The 64-bit real-mode setup routine and the assembly that embeds its externally
built legacy blob stay in scope. Two captured build-helper inputs remain
external. Other legacy boot inputs that do not receive `-save-temps` are not
enumerated by this capture; their absence is not a support claim.

| Input kind | Accepted | Rejected | Leading first errors |
| --- | ---: | ---: | --- |
| C | 3 | 481 | 476 x86 inline-asm rejections; 5 file-scope asm rejections. |
| Assembly | 112 | 394 | 265 symbolic `movq` signed-immediate relocations; 40 `pushf` aliases; 20 standalone prefix statements; 19 macro/conditional terminators. |

The accepted C inputs are `capflags.c`, `misc.c` and `union_find.c`; acceptance
can include data-only or largely inactive inputs. These numbers are whole-input
acceptance counts, not instruction support percentages or correctness verdicts.
The capture and all probes preserve the original inputs. All C failures in this
run occur at assembly constructs, so they mask further frontend/backend gaps.

At this initial audit, the assembler work list was:

1. Preserve signed 32-bit symbolic immediate relocations for AT&T `movq`.
   The current instruction model deliberately rejects these, which dominates
   the configured corpus. Do not replace them with an unqualified absolute
   64-bit relocation in a kernel code model.
2. Extend GNU macro defaults, iteration/string conditionals, keyword arguments
   and symbol expressions. Linux's exception tables use `.irp`/`.ifc`; its
   alternatives use grouped label arithmetic and deferred `.skip` sizes.
3. Support standalone `rep; instruction` and data-driven AT&T aliases such as
   `pushf`, `fxrstorq` and debug-register `%dbN` spellings. Review small directive
   gaps such as `.value` and the whitespace form of `.type`.
4. Review the measured RDTSCP/RDRAND/INVPCID/PREFETCHW bindings and explicit
   CLFLUSH policy. The first-error inventory is not an exhaustive optional-ISA
   audit. Also retain the `andb $-129,mem` integer-truncation matcher witness.
5. Define access to generated `.incbin` inputs before claiming complete assembly
   of the packaging files. Do not equate data that embeds legacy machine code
   with a requirement for a legacy-mode instruction encoder.

X86 extended asm, asm-goto and file-scope asm follow this assembler stage.
Kernel code generation must honor its own ABI and environment: the captured
commands request an 8-byte preferred stack boundary, no SIMD/red zone, kernel
addressing and specific optimizer policies. An ordinary SysV userspace target
and instruction feature flags do not establish those contracts.

Validation of the initial pin: the full workspace passed **2,736 tests**, zero
failed and 13 ignored across 503 reported groups. Workspace check, both generator
freshness checks, the three audit harness tests, scoped Rust formatting and
pin/count/document checks passed. Full workspace log:
`/tmp/keel-linux64-workspace.log`. These checks do not turn compile acceptance
into execution qualification of Keel-generated kernel objects.

## Signed immediates and macro/expression follow-up

The [follow-up report](audit-followup.json) uses the same config and unchanged
input hashes. Assembly acceptance grows from **112 to 259** with signed
immediates, then to **326 of 506** with the macro/expression changes. No input
loses acceptance. All 112 previously accepted assembly objects remain byte-for-
byte identical; all 259 accepted at the signed-immediate checkpoint also remain
identical after macro integration. C stays at 3 accepted and 481 rejected at
inline/file-scope assembly.

The [signed-immediate adapter](../../docs/x86-signed-symbolic-immediates.md)
reuses existing generated forms. [GNU language support](../../docs/x86-gnu-assembly-language.md)
now includes defaults, keyword/whitespace arguments, iteration, string and
absolute-symbol conditionals, grouped expressions and deferred alternatives
padding. Expanded arguments are scanned again for labels/statements and nested
macros. GNU integer-field aliases emitted by macros are accepted.

GNU successfully assembles all 326 accepted inputs, but the strict object
comparison reports zero complete matches, just as it does for the original 112.
The GNU ISA-used note is absent from every Keel object; empty default sections,
NOP padding, layout, symbol retention, relocation types and unwind section types
also differ in parts of the corpus. Shared sections have matching bytes in 115
files, relocations in 281 and layout in 312; public symbols match in 311. These
separate measurements are not complete-object or runtime qualification.

The remaining 180 assembly failures first hit `pushf` aliases (100), standalone
prefixes (42), whitespace `.type` syntax (15), optional instruction bindings,
register aliases, `.incbin` dependencies and operand-matcher limits. First errors
still hide later gaps. Register aliases in `vgetrandom-chacha` and the documented
bounded macro/expression cases remain explicit errors. Prefix/adapter and ELF
syntax work remain separate from the x86 inline-assembly work.

Validation of this follow-up passed **2,752 workspace tests**, zero failures
and 13 ignored across 505 reported groups. The assembler-IR/assembler/disassembler
packages passed 489 tests. Workspace check, all 98 generator tests, both
freshness checks, the three audit harness tests, scoped Rust formatting and
pin/count/link/diff checks passed. Final workspace log:
`/tmp/keel-gnu-final-workspace.log`.

```sh
python3 -m unittest discover -s suite/linux-kernel -p 'test_*.py'
```


## Scalar inline assembly follow-up

The [scalar x86 C inline-assembly milestone](../../docs/x86-c-inline-asm.md)
accepts register operands, ties, fixed registers, numeric immediates and clobbers.
Its [unchanged-input probe](audit-inline-asm.json) accepts **4 of 484 C inputs**
and **326 of 506 assembly inputs**, with no crashes or timeouts. All 326
previously accepted assembly objects remain byte-for-byte identical. The new C
input is `arch/x86/lib/inat.i`; acceptance is not an execution result.

The next shared blocker appears in 450 C files: `rip_rel_ptr(void *p)` passes
`"i"(p)` to asm and needs required inlining before its symbolic immediate can
be validated. The current scalar subset checks integer constant expressions
at C parsing time, and the inliner excludes asm-bearing callees. Another 23 C
files first need constraint unions, two need unsupported output constraints
(memory or register/memory union), and five need file-scope asm. These first
errors still hide later gaps. The reference config and captured source inputs
are unchanged; no Keel objects were linked into or executed as the kernel.

## Deferred immediate follow-up

X86 C compilation now expands required inline helpers before checking immediate
constraints, including when optional optimization is disabled. The kernel-style
`rip_rel_ptr(&global)` pattern has local compiler, object and independent
assembler witnesses. The [implemented contract](../../docs/x86-c-inline-asm.md#required-inlining-and-deferred-immediates)
records remaining limits.

The reports above remain historical measurements. The pinned corpus was not
rerun during this increment because its input host was unreachable. The shared
450-file first error is addressed by the new mechanism, but later errors remain
unmeasured. No new acceptance count, kernel link or execution result is claimed.

## Reproduced corpus and deferred-immediate audit

The [new audit](audit-deferred-immediates.json) recreates the corpus on a second
Linux x86-64 machine. All **992 captured files**, including the two external
build helpers, and the complete manifest match byte-for-byte. The resolved
configuration, `vmlinux` and `bzImage` also match the original hashes. The fresh
external reference boot prints the expected marker and exits with status 33.

The toolchain runs in an isolated container. GCC 14.4.0, binutils 2.46, Rust
1.99.1 with LLVM 23.1.1, pahole 1.32 and QEMU 11.1.2 preserve the pinned capability
fields. The host compiler is GCC 16.2.0; its difference from the original host
compiler does not change the reproduced artifacts. Generated data remains
outside Git.

Nix build-shell defaults need care when reproducing this corpus. Implicit
register-clearing hardening changes generated assembly despite identical
captured commands. The shell's synthetic `-rpath ...-nix-shell/lib` also changes
the embedded vDSO. Disable that implicit hardening and omit that synthetic
runtime path while retaining dependency search flags for host tools. Verify
saved-input hashes before treating a new build as the same corpus. Do not
remove instructions or patch saved inputs to force a match.

Keel checkpoint `6b36102f` probes the same **484 C and 506 assembly inputs**:

| Input kind | Accepted | Rejected | First errors |
| --- | ---: | ---: | --- |
| C | 4 | 480 | 429 output constraints, 45 input constraints, five file-scope asm cases, one section-placement case. |
| Assembly | 326 | 180 | Same acceptance set and remaining assembler categories as the previous audit. |

All 450 earlier immediate-validation first errors disappear. They expose 427
output-constraint failures, 22 input-constraint failures and one section-placement
failure. Thus the accepted input sets remain unchanged. The dominant output
witness is `+m` in `asm/bitops.h`. Input failures first require 43 `rm`
alternatives and two `g` constraints. Fixing only immediate validation does not
provide memory operands or constraint selection. These parser failures can
precede IR generation, so this does not qualify every kernel helper call.

There are no crashes, timeouts or acceptance regressions. GNU as accepts all
326 accepted assembly inputs; the strict comparison still finds zero complete
object matches. Shared-section byte, relocation and layout metrics remain 115,
281 and 312, with 311 public-symbol matches. These are separate diagnostics,
not whole-object equality. No Keel object is linked into or executed as the
kernel. Only corpus reproduction, reference boot and audit harness checks ran
for this report; compiler production source is unchanged.

## Flags aliases and explicit memory operands

The [next audit](audit-memory-operands.json) probes the same inputs at Keel
checkpoint `7572b6ef`. Generated GNU AT&T `pushf`/`popf` aliases select the
existing long-mode forms. Explicit C `m`, `=m` and `+m` operands now lower to
addresses of the original objects. Source operand numbering and register ties
survive that lowering. The existing pinned, unknown read/write memory effect
protects optimizer ordering; no optimizer algorithm changes are required.

Assembly acceptance grows from **326 to 402 of 506**. All 100 former `pushf`
first errors disappear: 76 inputs assemble completely and 24 expose later
prefix, symbol, directive, feature or operand gaps. The remaining leading ASM
errors are standalone prefixes (58), `.type` syntax (17), and smaller binding,
expression, feature-policy, operand-matching and `.incbin` groups.

C acceptance stays at **four of 484**. The new first constraints are:

| Constraint | Files | Role |
| --- | ---: | --- |
| `=q` | 254 | Register output class in per-CPU macros. |
| `=@cce` | 57 | Condition-code output in `cmpxchg16b` wrappers. |
| `+rm` | 1 | Read/write register-or-memory output. |
| `iq` | 117 | Immediate-or-register input. |
| `rm` | 43 | Register-or-memory input. |
| `g` | 2 | General input constraint. |

Five file-scope asm cases and one section-placement case also remain. These
first errors can precede IR generation; accepting explicit memory operands does
not provide constraint selection, flag outputs or complete kernel C support.

All 992 captured files and the three reference artifacts were rehashed and
remain unchanged. All 330 previously accepted objects, comprising 326 assembly
and four C objects, remain byte-for-byte identical. There are no crashes,
timeouts or acceptance regressions. GNU as accepts all 402 assembly inputs, but
strict whole-object equality remains zero. Shared-section byte, relocation and
layout matches are 121, 331 and 383, with 377 public-symbol matches.

The full workspace passed 2,766 tests, zero failures and 13 ignored across
507 groups. Nine inline-asm C tests exercise object parity across 145 exported
configurations; all those objects pass native GCC 13.3.0 comparison across the
35-function fixture, with eight broken controls rejected normally. Workspace
and profiling checks, all 98 generator tests, both freshness checks, scoped
formatting and diff checks passed. The existing reference was reused without
rebuilding or rebooting it. No Keel object was linked into or executed as the
kernel.

## The q register class

The [q-class audit](audit-q-registers.json) uses checkpoint `b60de289` on the
same corpus. X86 C accepts standalone `q` inputs and `=q`, `+q`, `=&q` and
`+&q` outputs. In 64-bit mode the frontend normalizes this class to the existing
GPR class; no backend, allocator, IR-schema or optimizer algorithm changes are
needed. Integer values through 64 bits and pointers use their normal operand
widths. Uppercase `Q` and 32-bit `q` semantics remain separate.

All **254** prior `=q` first errors disappear. Each exposes the `%a` template
modifier in the per-CPU macros. The current C first errors are:

| First blocker | Files |
| --- | ---: |
| `%a` template modifier | 254 |
| Input choices: `iq` (117), `rm` (43), `g` (2) | 162 |
| `=@cce` flag outputs | 57 |
| `+rm` output choice | 1 |
| File-scope asm | 5 |
| Section placement | 1 |

C acceptance remains four of 484; assembly remains 402 of 506. All **406**
previously accepted objects retain their exact bytes. All 992 corpus files and
the three reference artifacts were rehashed and match their qualified hashes.
There are no crashes, timeouts or acceptance regressions. Reference artifacts
were reused without rebuilding or rebooting. No Keel object is linked into or
executed as the kernel.

The 39-function inline-asm fixture passes all 145 native GCC 13.3.0 comparisons
and rejects ten broken controls normally. It covers write-only and read/write
q outputs, all scalar widths, pointer values, early clobbers, and byte operands
forced into R8–R10 by the Keel allocator's reserved-register and clobber rules.
Whole-input counts remain first-error measurements, not complete kernel C
qualification.

Full workspace validation at `b60de289` passed 2,767 tests, zero failures and
13 ignored across 507 groups. Workspace and profiling checks, 98 generator
tests, three audit harness tests, both freshness checks, scoped formatting and
diff checks passed. No production source changed after that run.

## Address and negation formatting audit

The [formatting audit](audit-formatting.json) uses checkpoint `2959b536` on
that same corpus. `%a` formats register operands as addresses, preserving
32-bit address widths; numeric immediates become absolute addresses and
symbol-plus-addend immediates become RIP-relative addresses. `%n` prints
negated numeric constants, including wrapping at the signed 64-bit minimum.
Address formatting does not infer a memory effect. Templates must declare
memory accesses using operands or clobbers.

All **254** prior `%a` first errors disappear. They expose 153 `=@ccz` flag
outputs, 80 `rm` input choices and 21 `iq` input choices. The
current C first errors are:

| First blocker | Files |
| --- | ---: |
| Input choices: `iq` (138), `rm` (123), `g` (2) | 263 |
| Flag outputs: `=@ccz` (153), `=@cce` (57) | 210 |
| `+rm` output choice | 1 |
| File-scope asm | 5 |
| Section placement | 1 |

C acceptance remains four of 484; assembly remains 402 of 506. All **406**
previously accepted objects retain their exact bytes. All 992 corpus files and
the three reference artifacts were rehashed unchanged. There are no crashes,
timeouts or acceptance regressions. The reference kernel was neither rebuilt
nor rebooted, and no Keel object was linked into or executed as the kernel.
These are first-error measurements, not complete kernel qualification.

The expanded 45-function fixture passes all 145 native GCC 13.3.0 comparisons
and rejects twelve broken controls normally. It covers pointer reads, 32-bit
register addresses, numeric and symbol addresses, positive/negative numeric
negation and the minimum signed constant. Clang/GNU assembler checks compare
bytes and relocation targets/addends for positive and negative symbol addends.
Native Clang execution of this expanded fixture was not run.

Full workspace validation at `2959b536` passed 2,768 tests, zero failures and
13 ignored across 507 groups. Workspace and profiling checks, all 98 generator
tests, three audit harness tests, both generator freshness checks, scoped
formatting and diff checks passed. No production source changed after that run.

## Constraint choices and flag outputs

The [choice/flag audit](audit-choices-flags.json) uses checkpoint `d14e3958` on
the same corpus. Scalar unions such as `iq`, `rm` and `g` select from supported
immediates and GPRs after required inlining and constant cleanup. Non-GPR
addressable inputs with a memory option use memory. Flag outputs use generated
condition mappings and capture zero-or-one results with `SETcc` directly after
the template. This does not change generated ISA data or instruction encoders.
The [inline-asm contract](../../docs/x86-c-inline-asm.md) records the bounded
selection policy and remaining alternatives/pressure limits.

All **474** prior first errors from input choices, flag outputs and `+rm`
disappear. The current C first errors are:

| First blocker | Files |
| --- | ---: |
| Machine-specific input choices: `er` (179), `Ir` (140) | 319 |
| Empty struct-member declarations | 153 |
| File-scope asm | 7 |
| Section placement | 1 |

The empty-member case is an extra semicolon in the preempt guard structs,
not a file-scope empty declaration. Two previously blocked C files now reach
file-scope asm. These counts identify first diagnostics, not every missing
feature in each file.

C acceptance remains four of 484; assembly remains 402 of 506. All **406**
previously accepted objects retain their exact bytes. All 992 inputs and three
reference artifacts were rehashed after the audit and remain unchanged. There
are no crashes, timeouts or acceptance regressions. The reference kernel was
not rebuilt or rebooted. No Keel object was linked into or executed as the kernel.

The 55-function fixture passes all 145 native GCC 13.3.0 executions and rejects
fifteen broken controls normally. It covers optional immediates, scalar and
aggregate choices, single-evaluation outputs, narrow flags mixed with memory
and tied registers, unused flags and all sixteen conditions captured together.
Comparison conditions also match a C calculation of carry, zero, sign,
overflow and parity. Clang/GNU assembler byte and relocation comparisons pass.
The local Clang C reference rejects sixteen simultaneous flag outputs; native
Clang execution of this fixture was not run.

Full workspace validation at `d14e3958` passed 2,773 tests, zero failures and
13 ignored across 507 groups. Workspace and profiling checks, all 98 generator
tests, three audit harness tests, both freshness checks, scoped formatting and
diff checks passed. No production source changed after that run.

## Ranged choices and empty record members

Checkpoint `212901fc` adds `er`/`re` and `Ir`/`rI` inputs, preserving their
predicates through required inlining and constant cleanup. `e` chooses numeric
signed-32-bit immediates; `I` chooses constants from 0 through 31. Other values
use registers. Symbols use registers for `er` because no code-model contract
establishes their signed-32-bit range. Standalone `e`/`I` constraints remain
unsupported. Empty declarations inside structs and unions add no fields and
preserve layout and initialization.

The [ranged-choice audit](audit-ranged-choices-empty-members.json) uses the
same saved inputs. All 319 prior `er`/`Ir` first errors and all 153 empty-member
first errors disappear. Current C first errors are:

| First blocker | Files |
| --- | ---: |
| Packed bit-fields requiring partial-container access | 434 |
| File-scope asm | 25 |
| GNU fallthrough attribute statements | 7 |
| Section placement | 7 |
| `Nd` input constraints | 4 |
| Alias attributes | 2 |
| `__builtin_has_attribute` | 1 |

The seven remaining `expected a type, found ;` diagnostics occur at
`__attribute__((__fallthrough__));` statements, including `include/linux/signal.h`;
they are distinct from the empty record members fixed in this checkpoint.
C acceptance remains four of 484 and standalone assembly remains 402 of 506.
These are first errors rather than an exhaustive inventory of missing support.

All 406 previously accepted objects remain byte-for-byte identical, with no
crashes, timeouts or acceptance regressions. All 992 inputs and the three
reference artifacts were rehashed unchanged after this audit. No reference
rebuild or reboot occurred, and no Keel object was linked into or run as the
kernel. The 57-function userspace fixture passes all 145 fresh objects against
GCC 13.3.0 and rejects seventeen broken controls normally. Independent C checks
the ranged additions, bit toggles and comparison flag results.

Full workspace validation at `212901fc` passed 2,777 tests, zero failures
and 13 ignored across 508 groups. Workspace and profiling checks, all 98
generator tests, three audit harness tests, both freshness checks, scoped
formatting and diff checks passed. No production source changed after that run.

## File-scope asm and kernel sections

The [file-asm/section audit](audit-file-asm-sections.json) uses implementation
checkpoint `96680637` on the unchanged corpus. GNU basic asm is stored as ordered
module fragments and emitted through the shared x86 assembler, including macros,
aliases, section stacks and relocations. ELF `.init.*` sections are admitted with
storage checks. Zero-initialized `.init.data` occupies PROGBITS bytes. External
section annotations are accepted and inherited by later definitions; imports do
not allocate storage or carry placement into call contracts.

C acceptance increases from **4 to 30 of 484**. All 32 prior first errors from
file-scope asm and section placement disappear: 26 inputs now compile, while six
reach later errors. Current first errors are:

| First blocker | Files |
| --- | ---: |
| Packed bit-fields requiring partial-container access | 434 |
| GNU fallthrough attribute statements | 8 |
| `Nd` input constraints | 4 |
| Semicolon-separated `rep; bsf` prefixes | 3 |
| Alias attributes | 2 |
| POPCNT feature policy | 1 |
| `__builtin_has_attribute` | 1 |
| Assignment type compatibility | 1 |

Standalone assembly remains 402 of 506. All 406 previously accepted objects
retain identical bytes; there are no crashes, timeouts or acceptance regressions.
All 992 inputs and three reference artifacts were rehashed unchanged. No reference
rebuild or reboot occurred. No Keel object was linked into or run as the kernel.
These counts identify first diagnostics, not exhaustive support or kernel
code-generation, objtool, link or boot qualification.

The [file-asm execution fixture](../x86-file-asm/README.md) passes 48 generated
V1 objects and an independent GCC reference, each checked against 512 runtime
input pairs and independent C pointer, data and relocation expectations. Six
broken controls fail normally. All 48 fresh local exports have the exact bytes
executed on Linux. Clang/GNU assembly comparisons pass with the stated GAS weak
alias relocation difference; padding at the assembly/C boundary is explicit.

The complete workspace run reported 2,783 passing tests, one obsolete test
expectation and 13 ignored across 509 groups. The test expected external
section annotations to be rejected. Commit `3e602086` updates that expectation;
all seven tests in its target pass on rerun. This leaves 2,784 distinct workspace
tests passing and no outstanding test failures. No production code changed
after `96680637`.

Workspace, profiling, and all-targets/all-features checks pass. The latter
exposed a missing AArch64 ELF arm in an opt-in allocator test; `27c451b1` repairs
the harness and its test passes. Documentation builds complete with existing
warnings. All 98 generator tests, three audit harness tests, both freshness
checks, scoped formatting and diff checks pass. Strict Clippy stops at 52
diagnostics in untouched `keel-asm-ir` code.

## Packed bit-fields

Checkpoint `eb21397c` supports packed bit-fields that need partial or crossing
container accesses. Loads and stores use only the field's byte span, preserve
neighboring bits, and keep volatile flags. Assignment and increment results use
the stored field width without another volatile read. A Clang layout comparison
also exposed and fixed AArch64 Linux zero-width field alignment.

The [unchanged-input audit](audit-packed-bitfields.json) clears all **434** prior
packed-field first errors. Each now reaches a later function-scope basic-asm
restriction in `arch/x86/include/asm/special_insns.h:301`:

```c
asm volatile(".byte 0xc4, 0xe2, 0x78, 0x49, 0xc0");
```

This is the `tile_release` helper. The parser checks its body even when the
helper is unused. The observed diagnostic is a basic-asm language restriction,
not an instruction feature-policy rejection.

Whole-input acceptance remains **30/484 C inputs** and **402/506 assembly inputs**.
The other C first-error counts remain eight fallthrough attribute statements,
four `Nd` constraints, three semicolon-separated `rep; bsf` prefixes, two alias
attributes, one POPCNT policy failure, one `__builtin_has_attribute`, and one
assignment type mismatch. These counts describe the first error per file.

All **432** previously accepted objects retain their exact bytes. All 992 inputs
and the three reference artifacts were rehashed unchanged, with no crashes,
timeouts or acceptance regressions. The reference was not rebuilt or rebooted.
No Keel object was linked into or executed as the kernel.

The [packed-field fixture suite](../x86-packed-bitfields/README.md) checks 48
explicit-V1 objects, four native-host configurations, guard pages, canaries,
initializers, volatile updates and expression-result truncation. An independent
GCC reference and five broken controls exercise the Linux oracle. The layout
matrix matches 256 generated records against Clang on each of x86-64 Linux and
AArch64 Linux, including record bytes, sizes and alignments. This is userspace
and layout evidence, not a kernel code-generation qualification.

At semantic checkpoint `eb21397c`, the full workspace run passes **2,788 tests**,
with zero failures and 13 ignored across 510 reported groups. The all-targets,
all-features check, 98 generator tests, three audit-harness tests, and both
freshness checks pass. Rustdoc builds with existing warnings. Strict Clippy
still stops at 52 existing diagnostics in untouched `keel-asm-ir` code. Later
changes only format one initializer assignment, strengthen the layout-matrix
count assertion (rerun passed), and record documentation/evidence.
