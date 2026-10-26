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
