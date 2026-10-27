# Minimal Linux x86-64 compiler target

The latest [kernel boot qualification](../../docs/linux-kernel-code-model.md)
uses Keel for every captured core C and assembly object, with GNU ld for linking.
The [native linker audit](../../docs/linux-kernel-native-linker-audit.md) records
the remaining script/layout and input-selection gaps. The paired
[KVM benchmarks](../../docs/linux-kernel-benchmarks.md) compare the pinned GCC
and Keel images for size, startup and five checked kernel workloads.

Latest measured status: **484/484 C inputs** and **506/506 assembly inputs**
accepted under the opt-in [kernel audit policy](kernel-audit-policy.json).
The [C contract checkpoint](audit-c-contracts.json) implements checked asm register
sharing, cleanup-aware jumps, packed pointer relocations, ELF notes and function
alignment. All 975 preceding accepted inputs remain accepted; 798 objects are
byte-identical and 177 C objects change. All 506 assembly objects are
byte-identical. The pinned source-to-object acceptance goal is complete.
Original and earlier optional policies retain their historical results below.

The subsequent [link audit](audit-link.json) links the assembly-only replacement
with GNU ld and passes table sorting. The full C replacement still fails the
final link after all 484 inputs compile with `-O`: 21 symbols remain unresolved.
The [link audit notes](../../docs/linux-kernel-link-audit.md) record optimizer
witnesses and objtool diagnostics. No new kernel was packaged or booted.

This fixture targets the **64-bit parts** of Linux v6.12. Host programs,
16/32-bit code and mixed-mode assembly remain external. The supported x86 C
inline-assembly subset is recorded in the follow-ups below. Do not strip
assembly or attributes from Linux to make the probe pass.

`source.json` pins the release archive and source identity. `minimal.config`
is the input to `allnoconfig`; `resolved.config` records the reference result.
The seed enables serial output, an ELF initramfs and the disposable guest's
I/O permission API. The latter lets the reference init report a normal QEMU
debug-exit result without requiring an ACPI power-off subsystem.

This is a compatibility fixture. It disables SMP, modules, networking, optional
instrumentation and most devices by starting from `allnoconfig`. It is not a
production or security configuration. The booted reference kernel, initramfs program and boot code use an external
compiler and linker. Link experiments use separate object copies.

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

`probe --asm-target-features <list>` and `--c-target-features <list>` select
explicit extra features independently. Omit them to retain the original
assembler v3 and C v1 policies. Results record both policies. Keep reports from
different policies separate when comparing acceptance counts.

The opt-in [kernel audit policy](kernel-audit-policy.json) grants instruction
availability for this corpus without changing Keel's CPU levels. Select it with:

```sh
python3 suite/linux-kernel/audit.py probe \
  --build /path/to/build --manifest inputs.json \
  --keel target/debug/keel --out probe-kernel-policy \
  --feature-policy suite/linux-kernel/kernel-audit-policy.json
```

This keeps C at v1 and assembly at v3, then supplies the JSON's explicit extra
features. The C list grants POPCNT, RDRAND, RDTSCP, XSAVE, CLFLUSH, FSGSBASE,
PREFETCHW, INVPCID, RDSEED, CX16 and WAITPKG. The assembler retains the preceding
eight-feature permission list. Each permission has a rationale in the JSON.
The probe validates canonical names against the generated feature schema and
records the policy name, content hash and resolved flags. Combining this option
with either target-feature override is rejected before probing.

This policy permits assembly of the captured instructions. It does not change
normal compiler defaults, establish CPU/OS state preconditions, disable the
red zone or SIMD, or qualify kernel ABI, linking or execution. Ordinary probes
without feature options retain the original policies. Historical reports keep
their original permissions and remain separate from the named-policy results.

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

## Basic asm and kernel-sized alignment

The [unchanged-input audit](audit-basic-asm-alignment.json) clears the previous
434 function-scope basic-asm first errors. Nonempty x86 basic asm uses existing
pinned IR assembly with memory/flags clobbers and literal percent signs. It
assumes preserved GPRs and ordinary C stack/fallthrough behavior. The raw
`tile_release` bytes assemble when emitted; no AMX execution is claimed.

Those inputs first expose the 4 KiB C alignment limit at `struct irq_stack`.
C object declarations and static IR data now share a 64 KiB alignment limit.
Linux's 16 KiB local and global objects use existing storage paths. Direct IR
stack-slot limits and restrictions on by-value over-aligned aggregates remain
unchanged. No backend implementation changed in this increment.

Acceptance grows from **30 to 33 C inputs**: `lib/rbtree.i`, `lib/rcuref.i`
and `lib/timerqueue.i` now compile. Assembly remains **402 of 506**. All 432
previously accepted objects retain their exact bytes. All 992 captured inputs
and the config, `vmlinux` and `bzImage` were rehashed unchanged. There are no
crashes, timeouts or acceptance regressions. The external reference was not
rebuilt or rebooted, and no Keel object was linked into or run as the kernel.

Current C first errors are:

| First blocker | Files |
| --- | ---: |
| Asm-goto | 419 |
| GNU fallthrough attribute statements | 20 |
| `Nd` port constraints | 4 |
| Semicolon-separated `rep; bsf` prefixes | 3 |
| Alias attributes | 2 |
| POPCNT feature policy | 1 |
| `__builtin_has_attribute` | 1 |
| Assignment type compatibility | 1 |

Implementation stops at **asm-goto**, as requested for work needing significant
backend changes. `cpufeature.h:178` uses it for static CPU-feature tests. The
current asm node has no branch successors. This needs explicit IR/CFG edges,
SSA liveness and allocation across those edges, output-availability rules and
backend emission. Treating the text as ordinary fallthrough asm would miscompile
the kernel. The smaller C gaps remain recorded rather than bypassing this stop.

The 104 standalone assembly first errors remain standalone prefixes (58),
whitespace `.type` syntax (17), debug-register aliases (5), optional instruction
bindings (14), feature gates (3), operand matching (4), `.incbin` access (2) and
one macro expression. These counts describe first diagnostics, not all missing
features. Kernel environment policy, objtool, linking and a mixed-toolchain boot
remain separate qualification steps even after source acceptance.

Two optimized/unoptimized userspace objects verify that basic asm writes remain
visible to C across 2,001 input values each. Two more verify 16 KiB global and
local addresses over 2,001 invocations each. Both independent C oracles reject
their deliberately broken controls with exit 1. These executions do not qualify
Keel-generated kernel code.

The full workspace run passed 2,797 regular tests and found one stale golden
diagnostic naming the old 4 KiB limit. Updating that fixture and rerunning its
target passes, giving 2,798 distinct regular tests verified, plus one doctest;
13 tests remain ignored. The full run was not repeated. All-targets/all-features
check, rustdoc, the three audit tests and both generator freshness checks pass.
Scoped Clippy completes with existing library warnings. Full run and fixture
rerun logs are `/tmp/keel-kernel-gap-tests.log` and
`/tmp/keel-kernel-gap-golden.log`.

## C and assembly expansion

The [expansion audit](audit-c-asm-expansion.json) adds the following support:

- GNU `fallthrough`/`__fallthrough__` attributes on null statements inside
  switches. They emit no operation and preserve switch control flow.
- GNU statement expressions ending in one or more named labels followed by a
  value. Labels stay before the value's evaluation. Trailing unlabeled null
  statements do not remove the value; labeled null statements and compound
  statements retain void type.
- X86 `Nd`/`dN` asm inputs. Numeric constants 0 through 255 select an immediate;
  other values select RDX. The predicate survives required inlining. `%w` works
  with both alternatives, preserving the `$` prefix on immediate operands.
- Standalone x86 prefixes such as `rep; bsf`, emitted as byte statements so
  labels, alignment, section state and relocation positions match GNU as.
  `.type` accepts whitespace syntax and `STT_FUNC`/`STT_OBJECT`/`STT_TLS`/`STT_NOTYPE`.
- Generated adapters for RDTSCP, FS/GS-base reads/writes, RDRAND, PREFETCHW and
  INVPCID; generated debug-register `%dbN` and `fxsaveq`/`fxrstorq` aliases.
  Full-width byte immediates accept negative eight-bit patterns down to -256;
  wider sign-extending immediates keep their signed ranges. Segment loads admit
  GPR32 sources through the existing MOV forms.

Optional capabilities have explicit feature fields and policy. Existing CPU
profile memberships and compiler instruction semantics remain unchanged.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 40 | 486 |
| Explicit optional features | 42 | 499 |

The second probe adds `+popcnt` for C and
`+rdtscp,+fsgsbase,+rdrand,+prefetchw,+invpcid,+clflush` for assembly. Its extra
acceptance reflects permission to encode those instructions. It does not prove
their availability on a v3 CPU or establish a kernel code-generation policy.

All 435 previously accepted objects remain byte-for-byte identical under both
policies. All 992 captured inputs and three reference artifacts were rehashed
unchanged, with no crashes, timeouts or acceptance regressions. No reference
rebuild or reboot occurred. No Keel object was linked into or run as the kernel.

The original-policy C first errors are now asm-goto (439), alias attributes (2),
POPCNT permission (2) and `__builtin_has_attribute` (1). Both POPCNT cases accept
under the explicit policy. The 20 fallthrough errors, four `Nd` errors, three
prefix errors and one assignment-type error no longer appear. Asm-goto still
needs the IR/CFG and backend work described in the preceding follow-up.

The seven assembly failures under the explicit policy are `.incbin` file access
(2), `sysretl`, `tpause`, `rdseed`, `.fill`, and the register-alias expression in
`vgetrandom-chacha`. Under the original policy, 13 more inputs first fail explicit
optional-feature checks. These remain first-error counts, not a complete feature
inventory or kernel correctness qualification.

Validation passes 2,811 workspace tests and one doctest, with zero failures and
13 ignored tests. All-targets/all-features check, 99 x86 generator tests, four
Arm generator tests, four audit-harness tests and both freshness checks pass.
Scoped Clippy completes with existing library warnings. The new 80-case adapter
matrix matches Clang and GNU bytes and round-trips through Intel formatting.
Prefix tests also compare GNU labels, symbol types and relocation offsets.
Native statement-expression tests cover optimization/debug combinations;
objects assemble for all three native targets. Two native Linux port-constraint
objects pass all 65,536 input values and live-register checks using a safe MOV
template. Port I/O and optional system instructions were not executed.

Full workspace log: `/tmp/keel-kernel-expansion-tests.log`.

## Binary inputs and attribute queries

The [follow-up audit](audit-binary-inputs-attributes.json) adds GNU `.fill`,
register symbols in instruction operands, explicit binary inputs, checked
`nonstring` attribute queries, and generated SYSRETL/RDSEED/TPAUSE adapters.
RDSEED and WAITPKG have separate optional feature fields; CPU profile
memberships remain unchanged. The generator also stops inferring GNU-invalid
RDRAND/RDSEED width suffixes and TPAUSE suffixes. Canonical forms remain supported.

The assembler's [binary-input APIs](../../docs/x86-gnu-assembly-language.md#binary-inputs)
accept a resolver callback. Input bytes are retained once per filename before
layout, including across chunks. Existing library APIs do not read files.
The CLI supplies working-directory lookup for x86 assembly and C/IR objects.
`.fill` shares deferred-space convergence checks; register aliases preserve
the API's initial syntax mode as well as in-source dialect changes.

`__builtin_has_attribute(expr, nonstring)` retains checked declaration/member
metadata and leaves the operand unevaluated. Unsupported queries diagnose.
This clears `lib/earlycpio.i`; alias attributes remain explicit errors because
they need whole-unit target, type, contract, retention and linkage validation.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 41 | 488 |
| Explicit optional features | 43 | 503 |

The explicit policy adds `+rdseed,+waitpkg` to the preceding assembler feature
list and retains `+popcnt` for C. All 526 inputs accepted under the previous
original policy remain accepted and produce identical object bytes under both
new policies. All 992 captured inputs and the three reference artifacts were
rehashed unchanged, with no crashes, timeouts or acceptance regressions.

Three binary dependencies are now recorded with sizes and hashes:
`realmode.bin`, `realmode.relocs` and `initramfs_inc_data`. Their bytes match
the corresponding sections in the existing GNU reference build objects and
both new Keel probe outputs. This is byte packaging of externally built legacy
code, not legacy-mode compiler support. The reference was not rebuilt or
rebooted, and no Keel object was linked into or run as the kernel.

Clearing the previous seven assembly failures exposes three later first errors:

| Input | Next blocker |
| --- | --- |
| `arch/x86/entry/entry_64.s` | Function-style GNU macro invocation `STACK_FRAME_NON_STANDARD(clear_bhb_loop)` |
| `arch/x86/entry/vdso/vgetrandom-chacha.s` | `.octa` integer data |
| `arch/x86/kernel/head_64.s` | Full-width symbolic relocation addend outside the current signed-i64 range |

The original policy has another 15 assembly first errors at explicit feature
gates. The explicit-policy C failures remain asm-goto (439) and alias attributes
(2); the original policy additionally rejects two POPCNT users. These are
first-error counts rather than complete kernel compatibility or correctness
claims. Significant asm-goto backend work remains outside this increment.

Validation passes 2,823 regular workspace tests and one doctest, with zero
failures and 13 ignored tests. All-targets/all-features check, 99 x86 generator
tests, four Arm generator tests, four audit tests and both freshness checks pass.
The new 19-case instruction matrix matches Clang/GNU bytes. `.fill` and alias
tests compare GNU output, with Clang comparisons where it supports the GNU
spelling. Binary-input tests compare GNU bytes and label offsets and check
source/structured/chunked parity and both CLI object paths. GCC validates
`nonstring` queries; native host execution and three-target object assembly pass.
Scoped Clippy completes with existing library warnings. System instructions
were not executed. Full log: `/tmp/keel-kernel-completion-tests.log`.

## Asm goto, function aliases and assembly completion

The [follow-up audit](audit-asm-goto-aliases.json) adds output-free x86 asm goto,
checked function aliases, adjacent parenthesized GNU macro invocations,
128-bit `.octa` literals and full-word x86 ELF relocation addends. No generated
ISA, compiler IR, allocator or backend files changed.

Asm goto lowers label references to restoration trampolines and an ordinary IR
switch. Every outgoing path restores saved GPR homes before entering C code.
The [contract](../../docs/x86-c-inline-asm.md#asm-goto) covers input constraints,
clobbers, named/numeric labels, backedges and metadata references. Output
operands, other targets and cleanup/VLA functions remain diagnosed. Function
aliases resolve forward definitions and chains by final assembler name with
whole-unit type, contract and linkage checks. Weak aliases retain replaceable
call/address references. This subset is x86-64 Linux only; object aliases and
lifecycle/retain/section attributes on aliases remain rejected.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 45 | 490 |
| Explicit optional features | 47 | 506 |

The optional policy remains unchanged: `+popcnt` for C, and
`+rdtscp,+fsgsbase,+rdrand,+prefetchw,+invpcid,+clflush,+rdseed,+waitpkg` for
assembly. All 529 previously accepted original-policy objects and all 546
previously accepted optional-policy objects remain accepted and byte-identical.
All 992 captured inputs, binary dependencies and reference artifacts rehash
unchanged. No crashes, timeouts or acceptance regressions occur.

Both weak-alias inputs, `vgetcpu.i` and `blake2s-generic.i`, now compile.
`powerflags.i` and `fpu/bugs.i` also compile after asm goto support. Under the
optional policy, the remaining 437 C first errors are:

- 436 nonzero-depth `__builtin_frame_address` queries, first seen in
  `arch/x86/include/asm/thread_info.h`.
- One RDRAND feature gate in `arch/x86/kernel/cpu/rdrand.i`; the C policy enables
  POPCNT only. The original policy has two additional POPCNT rejections.

All assembly inputs are accepted with explicit permissions. The original
policy still rejects 16 inputs at optional feature gates. First errors can
mask later unsupported C forms. Frame-chain access needs a checked frame and
calling-convention contract before further compiler support.

Independent GNU comparisons cover all three newly accepted assembly inputs.
The ChaCha input's shared sections match in bytes, layout and relocations.
Whole-object equality is not claimed: GNU emits additional property/default
sections, and the entry/head inputs also differ in branch relaxation, alignment
bytes, symbols and relocation representation. Focused reduced witnesses match
GNU/Clang where each accepts the spelling. No Keel object was linked into or
executed as the kernel, and the reference kernel was not rebuilt or rebooted.

Validation passed 2,830 workspace tests and one doctest, with 13 ignored.
All-targets/all-features check, 99 generator tests, four audit harness tests,
both freshness checks, scoped formatting and Rustdoc passed. Scoped Clippy
completed with existing warnings. After that full run, two test-only fixes
made the cross-host target explicit and forced alias-pointer comparisons to
observe actual linker addresses rather than Clang constant folding. Production
code is unchanged; all four focused tests passed again on macOS and all five
passed on Linux.

All 24 asm-goto object variants passed both GCC and Clang native runners,
with two independent reference executions. Two broken branch controls failed
normally with exit 1. Eight alias executions covered optimization on/off and
weak-default/strong-override binding with both compilers. Full workspace log:
`/tmp/keel-goto-alias-tests-final.log`; native log:
`/tmp/keel-goto-alias-native-complete.log`.

## Frame and stack observers

The [follow-up audit](audit-c-stack-observers.json) implements native frame
queries at constant depths 0..64, x86-64 Linux return-address queries at depth
zero, pointer extraction/frobbing and read-only global `rsp` bindings. The
[C observer contract](../../docs/c-stack-observers.md) records the frame-chain
preconditions and target limits. C-required inlining rebinds observations to
the emitted frame; ordinary IR inlining keeps its frame-preservation guards.
No intrinsic IDs, assembler tables or backend emitters change.

The fresh corpus no longer stops at frame queries. It instead reaches Linux's
`ASM_CALL_CONSTRAINT`, expanded as `"+r"(current_stack_pointer)` in the
`clear_page` alternatives helper. This is a reserved-stack dependency, not an
ordinary register-variable load/store. The frontend now diagnoses the missing
contract explicitly. The prior parser discarded `register` and created an
ordinary object named `rsp`, so that earlier acceptance did not model this
operand correctly. General stack-pointer writes remain unsupported.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 38 | 490 |
| Explicit optional features | 40 | 506 |

There are 444 C first errors at the stack-call contract under both policies.
The original policy has another two POPCNT first errors. The optional C policy
now enables `+popcnt,+rdrand`; the assembly permission list is unchanged. RDRAND
was independently accepted with explicit permission by the preceding compiler,
but its current input reaches the earlier stack-call diagnostic. Probe rows
and summaries now record each feature-gated instruction and its complete
required feature set, without inferring missing permissions from GCC flags.

Seven previously accepted C inputs now reject: `cpu/powerflags.i`,
`fpu/bugs.i`, `lib/iomap_copy.i`, `lib/rbtree.i`, `lib/rcuref.i`, `lib/string.i`
and `lib/timerqueue.i` (the first two are under `arch/x86/kernel`). This is an
intentional withdrawal of acceptance at an unimplemented contract. Another
28 original-policy and 30 optional-policy C objects change to remove bogus
`rsp` data symbols; every shared executable section remains byte-identical.
All retained assembly objects are byte-identical. All 992 captured inputs,
binary dependencies and reference artifacts rehash unchanged. No crashes or
timeouts occur. No Keel object is linked into or executed as the kernel.

The full workspace passes 2,840 tests and one doctest, with 13 ignored. A final
diagnostic refinement then passes all eight affected C tests and five audit
tests. Workspace all-targets/all-features check, 99 generator tests, both
freshness checks and scoped formatting pass. Clippy completes with existing
warnings; Rustdoc completes with an unrelated existing bare-URL warning.

All 72 matrix objects pass native execution with both GCC and Clang, for 144
generated executions, plus six independently compiled references and three
normally rejected controls. Frame/return references use `-O2`; stack-register
references use `-O0` because optimized GCC merges global-register reads across
`alloca`. Every optimized/unoptimized Keel variant is checked against separate
inline-asm RSP observations. The native Rust frame target also exercises
controlled caller chains, dynamic frames and required inline helpers with both
compilers. Unsafe walks beyond the known caller chain are not executed.

Logs: `/tmp/keel-c-frame-tests.log`, `/tmp/keel-c-frame-native-final.log` and
`/tmp/keel-c-frame-final-probe.log`. Further implementation stops at the reserved
stack operand/backend contract, as requested for significant backend work.

## Reserved-stack asm calls

The [follow-up audit](audit-rsp-stack-call.json) adds the reserved-stack IR and
backend contract authorized after the preceding stop. Exact `"+r"` on the bound
RSP variable normalizes to an input-only `rsp` dependency with no SSA stack
result or writeback. The verifier checks its I64 `StackSave`/`PtrToInt`
provenance, uniqueness and reserved-register conflicts. C source operand names
and numbers remain stable across register, memory and stack operands.

The backend renders live physical RSP and never installs the snapshot value.
A later input expression may allocate stack space without rewinding the stack.
Token-bearing asm receives an ABI call barrier for caller-clobbered GPR/SIMD
values. The template must balance stack changes, preserve the frame bases and
DF, maintain call alignment and return normally. This does not infer argument
or return types or model unwinding/nonlocal exits. Linux's `=D`/`D` read-then-write
pair is admitted in this mode when types match and the output is non-early;
competing inputs, duplicate outputs and clobber conflicts remain errors.
Ordinary asm keeps its prior fixed-register matching policy. The
[contract](../../docs/x86-c-inline-asm.md#balanced-stack-calls) records the limits.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 65 | 490 |
| Explicit optional features | 68 | 506 |

This restores all seven previously withdrawn inputs and accepts another 20 C
inputs under the original policy, for 27 new acceptances relative to the last
checkpoint. The optional policy gains 28 C inputs. All 528 previously accepted
original-policy objects and all 546 optional-policy objects remain accepted
and byte-identical. All 992 captured inputs, binary dependencies and reference
artifacts rehash unchanged. No crashes or timeouts occur. The reference was not
rebuilt/rebooted, and no Keel object was linked into or executed as the kernel.

The optional permissions remain `+popcnt,+rdrand` for C and the existing eight
assembly features. The next C first errors under that policy are:

| First-error family | Inputs |
| --- | ---: |
| Aggregate `q` asm operand in `pgtable.h` (`pmd_t` in `WRITE_ONCE`) | 256 |
| Constant-expression requirement at runtime `offsetof` in `rbtree_latch.h` | 136 |
| Nonconstant static initializers | 5 |
| Matching asm operand type/width | 4 |
| Required inlining of public `native_save_fl` definitions | 8 |
| RDTSCP permission in `trace_clock.i` | 1 |
| Other syntax, section and builtin gaps | 6 |

The last group includes GNU local labels, declaration syntax, custom-section
rules and `__builtin_isdigit`. These remain first diagnostics, not an exhaustive
list. `trace_clock.i` independently compiles when `+rdtscp` is explicitly added
to the C policy; that supplemental single-input check does not change the
headline corpus policy or CPU profile membership. Public inline binding guards
remain intact.

Validation passes 2,848 workspace tests and one doctest, with 13 ignored.
Workspace all-targets/all-features check, 99 generator tests, five audit tests
and both freshness checks pass. Scoped formatting, Clippy and Rustdoc complete
with existing warnings. All-target IR Clippy separately encounters the unchanged
`almost_swapped` denial in `tests/call_operand_abi.rs:15`; the new IR contract
target and consumer checks pass.

All 48 v1/v3 object variants pass with GCC and Clang native harnesses, for 96
executions. Independent assembly callees check exact entry/return RSP, nested
alignment, eight register/stack arguments, GPR/SIMD liveness, dynamic and aligned
frames, alternative-call metadata, output remapping and input `alloca` after
the snapshot. Four controls fail normally with exact exits 1, 5, 7 and 9 for
missing scalar/SIMD behavior, wrong stack arguments, omitted page writes and
missing operand allocation. The shared C fixture is not treated as a GNU
compiler reference: its reserved-stack contract deliberately supplies implicit
ABI SIMD clobbers that GNU inline asm would need to declare explicitly.

Logs: `/tmp/keel-rsp-contract-tests.log`, `/tmp/keel-rsp-contract-remote.log`
and `/tmp/keel-rsp-contract-evidence.json`.

## Runtime offsets and aggregate asm operands

The [follow-up audit](audit-offsetof-aggregate-asm.json) adds runtime integer
array designators to `__builtin_offsetof` and bounded x86 aggregate GPR operands.
Both use existing scalar IR/backend operations; no ISA tables or public
intrinsic IDs change.

[Offset designators](../../docs/c-runtime-offsetof.md) compute target-sized
unsigned byte offsets, including nested/anonymous/flexible members and zero
strides. Index effects run once. Constant results remain usable in enums,
initializers and static assertions. Pointer-member traversal and bitfields
reject. Evaluated-operand proofs now also prevent runtime effects from being
discarded in constant controls or static initializers, while retaining final
weak-binding checks. Unevaluated queries remain unevaluated.

[Aggregate operands](../../docs/x86-c-inline-asm.md#aggregate-register-operands)
load/store the full representation of 1/2/4/8-byte structs and unions in a GPR.
Output addresses are evaluated once. Returned/compound inputs, initialized
padding, packed alignment and recursive volatile effects are preserved. Generic
constraints retain memory fallback. Const-member outputs reject in both paths;
atomic register packing, different-width ties and wider register modes remain
conservative bounds. Aggregate immediates now diagnose instead of reaching a
non-scalar conversion panic. The fixture includes the kernel's `WRITE_ONCE`
switch, whose byte/word/dword/qword branches must all type-check an S8 wrapper.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 118 | 490 |
| Explicit optional features | 122 | 506 |

This adds 53 original-policy and 54 optional-policy C acceptances. All 555
previously accepted original-policy objects and all 574 optional-policy objects
remain accepted and byte-identical. The permission lists are unchanged. All
992 captured inputs, binary dependencies and reference artifacts rehash
unchanged, with no crashes, timeouts or acceptance regressions. The reference
was not rebuilt/rebooted, and no Keel object was linked into or run as the kernel.

The next optional-policy C first errors are:

| Family | Inputs |
| --- | ---: |
| Packed-enum size assertion in `rw_hint.h` | 293 |
| Static initializers | 22 |
| Declaration syntax | 16 |
| Required inlining of public `native_save_fl` definitions | 15 |
| Other asm, syntax, section, builtin and permission gaps | 16 |

The common assertion is `sizeof(enum rw_hint) == 1`; enum packing remains a
layout/ABI gap. The remaining group includes four matching-operand type errors,
three asm-string syntax errors, three GNU local-label errors, three
section/lifecycle errors, `__builtin_isdigit`, output-bearing asm goto and one
RDTSCP permission error. These counts describe first errors, not exhaustive
compatibility. Public inline-binding restrictions remain intact.

Validation passes 2,855 workspace tests and one doctest, with 13 ignored.
Workspace all-targets/all-features check, 99 generator tests, five audit tests,
both freshness checks, scoped formatting and Rustdoc pass. Scoped Clippy
completes with existing warnings. The private Wasm32 offset test executes with
Node locally; it is filtered from the isolated Linux shell, which lacks Node.
All seven remaining focused Linux tests pass.

Two 24-object matrices pass GCC and Clang native harnesses, for 96 generated
executions plus four compiler references. Six broken controls fail normally
with exact exits 1, 7, 11, 8, 9 and 14 for wrong offsets, omitted comma/zero-stride
index effects, lost padding, omitted packed updates and omitted `WRITE_ONCE`
writes. Another 214 independent compiler probes check aggregate mode boundaries.
The shared offset reference omits the nested `constant_p` wrapper because of
observed Clang side effects there; Keel objects execute it and check its
unevaluated contract. Direct offset calculations and index effects compare
against both compilers.

Logs: `/tmp/keel-offset-aggregate-tests.log`,
`/tmp/keel-offset-aggregate-native-final.log` and
`/tmp/keel-offset-aggregate-evidence.json`.

## Packed enum layout

GNU packed enum definitions now retain their selected integer representation
through tag references, record layout, scalar operations and calls. The
[implementation notes](../../docs/c-packed-enums.md) describe range selection
and unsupported combinations. No IR, backend, intrinsic registry or ISA data
changes are needed.

The [follow-up audit](audit-packed-enums.json) clears the packed-enum assertion
in all 293 previously affected C inputs. Those inputs now stop at asm names
on local register variables in `include/linux/fs.h`. Acceptance remains
118/484 C and 490/506 assembly under the original policy, or 122/484 C and
506/506 assembly with the unchanged optional-feature permissions.

All 608 previously accepted original-policy objects and all 628 optional-policy
objects remain accepted and byte-identical. There are no crashes or timeouts.
All 992 captured inputs, binary dependencies and reference artifacts rehash
unchanged. The reference kernel was not rebuilt or rebooted, and no Keel object
was linked into or run as the kernel.

Five focused tests pass locally and on Linux. All 24 generated x86 objects pass
with both GCC and Clang callers, for 48 native executions plus two compiler
references. Four broken-store/promotion controls return their expected nonzero
exits. These checks cover layout boundaries, signedness, initializers, narrow
loads/stores, scalar and record calls, volatile access, field canaries and
integer promotions.

Full workspace validation passes 2,860 tests and one doctest, with 13 ignored.
Workspace all-targets/all-features check, scoped Clippy and Rustdoc complete;
Clippy retains existing warnings. The 99 generator tests, five audit tests,
both generator freshness checks, scoped formatting and diff checks pass.
Logs: `/tmp/keel-packed-enum-final-tests.log`,
`/tmp/keel-packed-enum-final-remote.log` and
`/tmp/keel-packed-enum-evidence.json`.

## Named local register operands

[Local register declarations](../../docs/c-local-register-variables.md) now
constrain direct x86 extended-asm operands through the existing fixed-register
IR. Input, output, read/write, early-clobber and matching operands retain their
binding. The declaration does not reserve a hardware register outside asm.
No backend, ISA-table or IR schema changes are needed.

The [follow-up audit](audit-local-registers.json) clears the declaration error
in all 293 previously affected C inputs. They now reach different-width ties
between RAX pointer inputs and EAX status outputs. Together with four earlier
tie failures, this is the first error in 297 C inputs. Matching operands still
require the same IR type; this checkpoint does not relax that contract.

Acceptance remains 118/484 C and 490/506 assembly under the original policy,
or 122/484 C and 506/506 assembly with unchanged optional-feature permissions.
All 608 previously accepted original-policy objects and all 628 optional-policy
objects remain accepted and byte-identical. There are no crashes or timeouts.
All 992 inputs, binary dependencies and reference artifacts rehash unchanged.
The reference kernel was not rebuilt or rebooted. No Keel object was linked
into or run as the kernel.

All 24 generated x86 variants pass with GCC and Clang callers, for 48 native
executions. The fixture exercises all 14 nonreserved GPRs with hardcoded
register reads/writes, plus pointers, aliases, narrow values, shadowing,
volatile accesses, ties and a balanced hidden call. Two compiler references
omit the Keel stack-call extension. The Clang reference additionally binds
the tied output explicitly to account for an observed missing return-value
transfer; GCC and Keel pass the input-bound case unchanged. Six broken output,
read/write and call-result controls return the expected nonzero exits.

Full workspace validation passes 2,863 tests and one doctest, with 13 ignored.
Workspace all-targets/all-features check, scoped Clippy and Rustdoc complete;
Clippy retains existing warnings. All 99 generator tests, five audit tests,
both freshness checks, scoped formatting and diff checks pass.
Logs: `/tmp/keel-local-register-final-tests.log`,
`/tmp/keel-local-register-final-remote.log` and
`/tmp/keel-local-register-evidence.json`.

## C acceptance and ABI stop boundary

The [C acceptance checkpoint](audit-c-acceptance.json) implements mixed-width
GPR ties, pointer-only transparent unions, GNU local labels, ancestor return
addresses, static compound copies, initialized flexible-array storage,
GNU extern-inline ownership and declaration-only foreign function ABI metadata.
It also fixes chunk-boundary fallthrough recognition and adjacent static-assert
messages. [The contracts and stop boundary](../../docs/c-linux64-acceptance.md)
describe the supported bounds.

| Policy | Accepted C / 484 | Accepted assembly / 506 |
| --- | ---: | ---: |
| Original C v1, assembly v3 | 390 | 490 |
| Unchanged explicit optional features | 397 | 506 |

This adds 272 original-policy and 275 optional-policy C acceptances. All 608
previously accepted original-policy objects and all 628 optional-policy objects
remain accepted. Of those, 529 and 547 respectively remain byte-identical.
The changed 79 and 81 objects each remove the previously emitted global
`native_save_fl` definition; GNU extern-inline bodies are inline implementations
and do not own the external symbol. No other defined global function is added
or removed in those changed objects. The ledger records each symbol difference.

The final audit reports no crashes or timeouts. Static flexible-array storage
now includes the initialized tail, fixing the `init_mm` data-buffer panic
exposed in an intermediate probe. All 992 inputs, binary dependencies and
reference artifacts rehash unchanged. The reference kernel was not rebuilt or
rebooted. No Keel object was linked into or run as the kernel.

The full C acceptance goal stops at the requested architecture boundary:
`mm/vma.i` passes an empty `vm_userfaultfd_ctx` by value, including across public
function interfaces. The IR requires nonempty `byval`/`sret` layouts. GCC 14.4
and Clang 21.1 assembly oracles confirm that the empty SysV argument occupies
no register, while an expression returning an empty value still executes.
Correct support needs explicit source-to-IR argument/return mapping and
side-effect preservation. The frontend now gives a direct unsupported-ABI
diagnostic. The goal remains incomplete; 87 optional-policy C inputs reject.

The largest remaining first-error groups are const-qualified writes (18),
feature permissions (13), shared custom sections (11), unsupported builtins
(9), constant expressions (9), output-bearing asm goto (5), and switches with
cleanup locals (3). Other groups include empty aggregate ABI values, fixed
register overlap, LAR operand matching, public required-inline binding and
alignment/section constraints. These are first errors, not a full inventory.

Full workspace validation passes 2,880 tests and one doctest, with 13 ignored.
Workspace all-targets/all-features check, the profiled C ownership check,
scoped Clippy and Rustdoc pass; Clippy retains existing warnings. All 99
generator tests, five audit tests, both freshness checks and scoped formatting
checks pass. The final Linux qualification runs 100 generated executions,
seven compiler references and seven normally rejected controls across the tie,
return-address, transparent-union and inline-ownership fixtures.

Logs: `/tmp/keel-c-acceptance-final-tests.log`,
`/tmp/keel-c-acceptance-final-remote.log`,
`/tmp/keel-c-acceptance-evidence.json` and `/tmp/keel-empty-abi-oracle.log`.

### Empty aggregate ABI checkpoint

[The follow-up ledger](audit-empty-aggregate-abi.json) records x86-64 SysV
omission of complete, fieldless, alignment-one GNU structs/unions. The frontend
keeps local parameter objects and expression effects, but emits no ABI operand
or hidden result pointer for those values. IR aggregate layouts stay nonempty.
[The mapping contract](../../docs/c-empty-aggregate-abi.md) records the retained
limits and how to export the native interoperability fixture.

`mm/vma.i` now accepts. Under the unchanged optional-feature policy, C acceptance
is 398/484 and assembly acceptance is 506/506. The original pinned policy accepts
391/484 C inputs and 490/506 assembly inputs. Every prior accepted object remains
byte-identical. All 992 captured inputs and the reference artifacts were rehashed
unchanged; no reference rebuild, kernel link or boot was performed.

The full workspace run passes 2,884 tests and one doctest, with 13 tests skipped.
The final fixture passes 96 generated native executions across GCC and Clang,
two compiler references and two normally failing controls. It covers indirect
calls, stack arguments, ordinary aggregate returns, variadics, required inlining,
debug objects and empty-returning cleanup callbacks. Remaining C rejections are
still led by const-qualified writes, feature permissions and shared-section
storage/retention rules.

### GNU conditional qualifier checkpoint

[The qualifier ledger](audit-gnu-conditional-qualifiers.json) records a frontend
fix to GNU `a ?: b`: the single-evaluation temporary holds an unqualified value.
The source read and pointer-pointee qualifiers remain intact. This removes all
18 const-write first errors and admits 13 more C inputs. Five expose other
existing boundaries.

The unchanged optional policy now accepts 411/484 C and 506/506 assembly inputs;
the original pinned policy accepts 404/484 C and 490/506 assembly inputs. Every
prior accepted object is byte-identical. All captured inputs and reference
artifacts were rehashed unchanged. No kernel link, rebuild or boot was performed.
Full validation passes 2,887 tests and one doctest. Native checks pass four local
Arm executions, eight generated Linux executions and two compiler references.

### ELF section storage checkpoint

[The storage ledger](audit-section-storage.json) records static ELF placement
for pointer-bearing readonly constants and zero objects in `.ref.data`. All 11
prior shared-section errors disappear. Under unchanged permissions, acceptance
is 420/484 C and 506/506 assembly inputs; the original policy accepts 411/484 C
and 490/506 assembly inputs. Nine more C inputs accept under the optional policy.

All prior acceptances remain. Only two prior objects change: their `.init.rodata`
section loses `SHF_WRITE`; contents, symbols and relocations remain unchanged.
All other objects are byte-identical. All captured inputs and reference artifacts
were rehashed unchanged. No kernel link or boot was performed.

Clang object comparisons cover both ELF targets. GCC/Clang static native checks
pass two generated executions and two references. Full validation passes 2,891
tests plus one doctest; scoped Clippy and workspace check pass. Broad Clippy stops
at an unchanged `almost_swapped` error in `keel-ir/tests/call_operand_abi.rs:15`.
See [the section contract](../../docs/c-sections-retain.md) for retained limits.

### Native constant-expression checkpoint

[The constant-expression ledger](audit-constant-expressions.json) records native
integer byte-swap folding and opaque integer symbol/addend initializers. It
removes ten `htons` case-label errors and the `TOP_OF_INIT_STACK` initializer
error. The optional policy accepts 426/484 C and 506/506 assembly inputs; the
original policy accepts 416/484 C and 490/506 assembly inputs. Six more C inputs
accept under unchanged optional permissions.

All prior optional-policy acceptances are retained and byte-identical. Captured
inputs and reference artifacts were rehashed unchanged. No kernel link or boot
was performed. The remaining first errors include newly exposed const-member
and cleanup-goto cases; these are not constant-expression failures.

Full validation passes 2,896 tests and one doctest. Five focused tests preserve
runtime effects, weak binding, opaque address truth and existing relocation
limits. Native checks pass four local Arm executions plus a Clang reference,
eight generated x86 PIE executions and two GCC/Clang references. A negative
regression ensures symbolic addresses cannot overwrite bitfield storage.

The 484/484 source-to-object goal was still open at the checkpoint recorded in
[audit-c-acceptance-goal.json](audit-c-acceptance-goal.json), with
native builtin, generic-pointer, output-bearing asm-goto and cleanup-switch
witnesses. Feature permissions remain explicit. See the
[goal contract](../../docs/c-linux64-acceptance.md#full-acceptance-goal).

## Explicit kernel-audit permissions

The user-approved `kernel-audit-policy.json` is separate from compiler CPU
profiles. The named-policy rerun accepts 460/484 C inputs and 506/506 assembly
inputs. The preceding optional policy accepts 446/484 C and 506/506 assembly;
its report remains unchanged. All 14 feature-permission first errors clear,
including the TPAUSE permission reached after RDTSCP in the delay input.

All 952 previously accepted objects remain byte-identical. All 992 saved inputs,
the config, reference binaries and binary input dependencies were rehashed
unchanged. The compiler binary and its qualified source are unchanged.
Seven policy tests pass on both audit hosts. Real CLI checks show that default
and explicit v1 still reject RDTSCP and XGETBV, while the named policy assembles
them. These witnesses do not execute either instruction.

The [policy report](audit-kernel-policy.json) records resolved flags, the policy
hash, remaining first errors, new acceptances and unchanged objects. This round
does not change compiler, assembler, decoder or CPU-profile source. Rust
workspace validation is not repeated for this tooling/data change; the earlier
compiler qualification remains scoped to its recorded checkpoint. Kernel ABI,
linking, boot and runtime qualification remain separate.

## Link fixes and runtime boundary

The [follow-up](../../docs/linux-kernel-link-fixes.md) reduces the automatic C
link from 22 unresolved references to two FPU helpers. A reviewed, pinned IR
workaround resolves them, but the all-Keel C image triple-faults during its early
page-table transition. Substituting only the reference `head64.o` produces an
image that boots and passes VFS, memory, fork/COW, pipe and wait checks.

The successful image uses 480 Keel C objects, one reference C startup object and
20 Keel assembly objects. Its FPU unit includes the manual workaround. This is
mixed-image evidence. The remaining backend boundary is an explicit kernel code
model; objtool still reports warnings. See [audit-link-fixes.json](audit-link-fixes.json)
for exact scope, artifact hashes and controls. `runtime.py` can run the bounded
smoke test against a supplied `bzImage`; it builds an independent userspace init.

## Kernel address model and automatic linking

The [kernel code-model follow-up](../../docs/linux-kernel-code-model.md) removes
both temporary substitutions from the preceding checkpoint. All 484 captured C
inputs compile with `-O --link-intent executable --code-model kernel` and the
unchanged audit feature policy. GNU ld links 481 Keel C objects and 20 Keel ASM
objects; table sorting, boot and the runtime smoke checks pass under QEMU TCG.

[The ledger](audit-kernel-code-model.json) includes two pass-omission link
controls, a PIC-startup boot control and three runtime-verdict controls. The
positive link uses no manually edited IR or reference C replacement. External
boot/decompression/vDSO stages and 328 objtool warnings remain outside this
bounded qualification. Default userspace address generation is unchanged.
